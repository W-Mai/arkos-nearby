use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};

use mirui::core::resource::{FormatMatch, LoadContext, LoadError, LoadInput, Loader, ProbeLoader};
use mirui::render::texture::{ColorFormat, Texture, TextureMeta};

use super::{ArtworkKey, PREVIEW_HEIGHT, PREVIEW_WIDTH, source_key};

const PREFIX: &str = "nearby-artwork:";
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const MAX_DECODE_BYTES: u64 = 24 * 1024 * 1024;
const WORK_QUEUE: usize = 4;
static REVISION: AtomicU64 = AtomicU64::new(1);

pub(super) fn token(key: &ArtworkKey) -> String {
    format!(
        "{PREFIX}{}",
        serde_json::to_string(key).expect("artwork key")
    )
}

struct OpenedImage {
    file: File,
    key: ArtworkKey,
    meta: TextureMeta,
}

struct Pixels {
    bytes: Vec<u8>,
    width: u16,
    height: u16,
}

enum Output {
    Opened(OpenedImage),
    Pixels(Pixels),
}

#[derive(Default)]
struct Completion {
    value: Option<Result<Output, LoadError>>,
    wake: Option<Waker>,
}

type Job = Box<dyn FnOnce() + Send>;

/// File I/O and image codecs execute outside the resource-owner thread.
pub(super) struct ArtworkDecoder {
    send: mpsc::SyncSender<Job>,
    roots: Arc<[PathBuf]>,
}

impl ArtworkDecoder {
    pub(super) fn new() -> Self {
        Self::with_roots(vec![
            PathBuf::from("/roms"),
            PathBuf::from("/roms2"),
            PathBuf::from("/opt/arkos-nearby/saves/backups"),
            PathBuf::from("/home/ark/.config/retroarch"),
            PathBuf::from("/home/ark/.config/retroarch32"),
        ])
    }

    fn with_roots(roots: Vec<PathBuf>) -> Self {
        let (send, jobs) = mpsc::sync_channel::<Job>(WORK_QUEUE);
        std::thread::Builder::new()
            .name("nearby-artwork".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    job();
                }
            })
            .expect("artwork worker");
        Self {
            send,
            roots: roots.into(),
        }
    }

    fn submit(
        &self,
        state: &mut DecodeState,
        job: impl FnOnce() -> Result<Output, LoadError> + Send + 'static,
    ) -> Result<(), LoadError> {
        let completion = Arc::new(Mutex::new(Completion::default()));
        let output = completion.clone();
        state.waiting = Some(Box::new(move || {
            if Arc::strong_count(&output) == 1 {
                return;
            }
            let value = job();
            let wake = {
                let mut output = output.lock().expect("artwork completion");
                output.value = Some(value);
                output.wake.take()
            };
            if let Some(wake) = wake {
                wake.wake();
            }
        }));
        state.completion = Some(completion);
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct DecodeState {
    completion: Option<Arc<Mutex<Completion>>>,
    opened: Option<OpenedImage>,
    decoding: bool,
    waiting: Option<Job>,
}

impl DecodeState {
    fn poll(
        &mut self,
        decoder: &ArtworkDecoder,
        cx: &LoadContext<'_>,
    ) -> Poll<Result<Output, LoadError>> {
        if let Some(job) = self.waiting.take() {
            match decoder.send.try_send(job) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(job)) => {
                    self.waiting = Some(job);
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Poll::Ready(Err(LoadError::Failed("artwork worker stopped")));
                }
            }
        }
        let mut completion = self
            .completion
            .as_ref()
            .expect("submitted artwork")
            .lock()
            .expect("artwork completion");
        completion.wake = Some(cx.waker().clone());
        match completion.value.take() {
            Some(value) => Poll::Ready(value),
            None => Poll::Pending,
        }
    }
}

fn key(cx: &LoadContext<'_>) -> Result<ArtworkKey, LoadError> {
    serde_json::from_str(cx.token().strip_prefix(PREFIX).ok_or(LoadError::NotMine)?)
        .map_err(|_| LoadError::Failed("invalid artwork key"))
}

fn open(key: ArtworkKey, roots: &[PathBuf]) -> Result<OpenedImage, LoadError> {
    let path = Path::new(&key.0)
        .canonicalize()
        .map_err(|_| LoadError::Failed("artwork file unavailable"))?;
    if !roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| path.starts_with(root))
    {
        return Err(LoadError::Failed("artwork outside game library"));
    }
    let mut file = File::open(path).map_err(|_| LoadError::Failed("artwork open failed"))?;
    if file
        .metadata()
        .map_err(|_| LoadError::Failed("artwork metadata unavailable"))?
        .len()
        > MAX_SOURCE_BYTES
    {
        return Err(LoadError::Failed("artwork file limit"));
    }
    if source_key(key.0.clone()) != key {
        return Err(LoadError::Failed("artwork source changed"));
    }
    let reader = image::ImageReader::new(BufReader::new(&mut file))
        .with_guessed_format()
        .map_err(|_| LoadError::Failed("artwork format unavailable"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| LoadError::Failed("artwork header invalid"))?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(LoadError::Failed("artwork dimensions limit"));
    }
    let ratio = (f64::from(PREVIEW_WIDTH) / f64::from(width))
        .min(f64::from(PREVIEW_HEIGHT) / f64::from(height));
    let meta = TextureMeta {
        width: (f64::from(width) * ratio).round().max(1.0) as u16,
        height: (f64::from(height) * ratio).round().max(1.0) as u16,
        format: ColorFormat::RGBA8888,
    };
    Ok(OpenedImage { file, key, meta })
}

fn decode(mut opened: OpenedImage) -> Result<Pixels, LoadError> {
    opened
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|_| LoadError::Failed("artwork seek failed"))?;
    let mut reader = image::ImageReader::new(BufReader::new(&mut opened.file))
        .with_guessed_format()
        .map_err(|_| LoadError::Failed("artwork format unavailable"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let pixels = reader
        .decode()
        .map_err(|_| LoadError::Failed("artwork decode failed"))?
        .thumbnail(PREVIEW_WIDTH, PREVIEW_HEIGHT)
        .to_rgba8();
    if source_key(opened.key.0.clone()) != opened.key {
        return Err(LoadError::Failed("artwork source changed"));
    }
    Ok(Pixels {
        width: pixels.width() as u16,
        height: pixels.height() as u16,
        bytes: pixels.into_raw(),
    })
}

impl Loader<Texture<'static>> for ArtworkDecoder {
    type State = DecodeState;

    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Token(token) if token.starts_with(PREFIX) => FormatMatch::Match,
            _ => FormatMatch::NoMatch,
        }
    }

    fn poll_load(
        &self,
        state: &mut DecodeState,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        if !state.decoding {
            if state.opened.is_none() && state.completion.is_none() {
                let key = key(cx)?;
                let roots = self.roots.clone();
                self.submit(state, move || {
                    open(key, &roots).and_then(decode).map(Output::Pixels)
                })?;
                state.decoding = true;
            } else {
                if state.opened.is_none() {
                    core::task::ready!(self.poll_probe(state, cx))?;
                }
                let opened = state.opened.take().expect("probed artwork source");
                state.completion = None;
                self.submit(state, move || decode(opened).map(Output::Pixels))?;
                state.decoding = true;
            }
        }
        match core::task::ready!(state.poll(self, cx))? {
            Output::Pixels(pixels) => {
                let revision = REVISION.fetch_add(1, Ordering::Relaxed);
                Poll::Ready(
                    Texture::from_vec(
                        pixels.bytes,
                        pixels.width,
                        pixels.height,
                        ColorFormat::RGBA8888,
                    )
                    .map(|texture| texture.with_cache_revision(revision))
                    .map_err(|_| LoadError::Failed("artwork texture invalid")),
                )
            }
            Output::Opened(_) => {
                Poll::Ready(Err(LoadError::Failed("unexpected artwork completion")))
            }
        }
    }
}

impl ProbeLoader<Texture<'static>> for ArtworkDecoder {
    fn poll_probe(
        &self,
        state: &mut DecodeState,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<TextureMeta, LoadError>> {
        if let Some(opened) = &state.opened {
            return Poll::Ready(Ok(opened.meta));
        }
        if state.completion.is_none() {
            let key = key(cx)?;
            let roots = self.roots.clone();
            self.submit(state, move || open(key, &roots).map(Output::Opened))?;
        }
        match core::task::ready!(state.poll(self, cx))? {
            Output::Opened(opened) => {
                let meta = opened.meta;
                state.opened = Some(opened);
                Poll::Ready(Ok(meta))
            }
            Output::Pixels(_) => Poll::Ready(Err(LoadError::Failed("unexpected artwork metadata"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::core::cache::MaxSize;
    use mirui::core::resource::ResourceManager;
    use std::rc::Rc;
    use std::task::Context;
    use std::time::{Duration, Instant};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let path = std::env::temp_dir().join(format!(
                "nearby-decoder-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn image(&self, name: &str, width: u32, height: u32) -> ArtworkKey {
            let path = self.0.join(name);
            image::RgbaImage::from_pixel(width, height, image::Rgba([201, 3, 223, 255]))
                .save(&path)
                .unwrap();
            source_key(path.to_string_lossy().into_owned())
        }

        fn manager(&self, budget: usize) -> ResourceManager<Texture<'static>> {
            let images = ResourceManager::new(
                MaxSize::Bytes(budget),
                Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap(),
            )
            .with_probes(
                MaxSize::Count(16),
                TextureMeta {
                    width: 1,
                    height: 1,
                    format: ColorFormat::RGBA8888,
                },
            );
            images.add_probed_loader(ArtworkDecoder::with_roots(vec![self.0.clone()]));
            images
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn wait(mut condition: impl FnMut() -> bool, images: &ResourceManager<Texture<'static>>) {
        let stop = Instant::now() + Duration::from_secs(3);
        while !condition() {
            assert!(Instant::now() < stop, "resource completion timed out");
            images.poll_pending(&mut Context::from_waker(Waker::noop()));
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn metadata_and_pixels_share_the_resource_protocol_and_cached_identity() {
        let fixture = Fixture::new();
        let key = fixture.image("cover.png", 320, 200);
        let images = fixture.manager(2 * 1024 * 1024);
        let name = token(&key);
        assert!(images.probe(&name).is_none());
        wait(|| images.probe(&name).is_some(), &images);
        let meta = images.probe(&name).unwrap();
        assert_eq!((meta.width, meta.height), (256, 160));
        let a = images.load(name.clone());
        let b = images.load(name.clone());
        assert_eq!(a.get().cache_revision, 0);
        wait(|| a.get().cache_revision != 0, &images);
        let first = a.get();
        assert!(Rc::ptr_eq(&first, &b.get()));
        assert_eq!((first.width(), first.height()), (meta.width, meta.height));
        assert!(!first.transient);
        let revision = first.cache_revision;
        images.reload(&name).unwrap();
        wait(|| a.get().cache_revision != revision, &images);
        assert!(first.cache_revision == revision && a.get().cache_revision != 0);
        assert!(Rc::ptr_eq(&a.get(), &b.get()));
    }

    #[test]
    fn late_source_versions_are_rejected_and_new_versions_load() {
        let fixture = Fixture::new();
        let old = fixture.image("changed.png", 32, 24);
        let new = fixture.image("changed.png", 96, 48);
        assert_ne!(old, new);
        let images = fixture.manager(1024 * 1024);
        let obsolete = images.load(token(&old));
        let current = images.load(token(&new));
        wait(|| current.get().cache_revision != 0, &images);
        assert_eq!(obsolete.get().cache_revision, 0);
        assert!(current.get().cache_revision > 0);
    }

    #[test]
    fn retired_consumers_cannot_publish_an_obsolete_generation() {
        let fixture = Fixture::new();
        let key = fixture.image("retired.png", 1024, 768);
        let images = fixture.manager(1024 * 1024);
        let name = token(&key);
        let obsolete = images.load(name.clone());
        obsolete.get();
        images.remove_token(&name);
        drop(obsolete);
        let current = images.load(name);
        wait(|| current.get().cache_revision != 0, &images);
        assert_eq!((current.get().width(), current.get().height()), (256, 192));
    }
}
