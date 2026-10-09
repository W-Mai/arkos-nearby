use crate::model::Action;
use mirui::render::texture::ColorFormat;
use mirui::types::PhysicalRect;
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn record_init(stage: &str, detail: serde_json::Value) -> io::Result<()> {
    let directory = Path::new("/opt/arkos-nearby/logs");
    std::fs::create_dir_all(directory)?;
    std::fs::set_permissions(directory, Permissions::from_mode(0o700))?;
    let temporary = directory.join("native-init.next");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    let value = serde_json::json!({
        "stage": stage,
        "pid": std::process::id(),
        "time_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "detail": detail,
    });
    serde_json::to_writer(&mut file, &value).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(temporary, directory.join("native-init.json"))?;
    File::open(directory)?.sync_all()
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Default)]
struct VariableInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    transp: Bitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct FixedInfo {
    id: [u8; 16],
    smem_start: usize,
    smem_len: u32,
    fb_type: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: usize,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

pub struct Framebuffer {
    file: File,
    pub width: u16,
    pub height: u16,
    pub format: ColorFormat,
    stride: usize,
    offset: usize,
    first_write: bool,
}

fn checked(result: libc::c_int) -> io::Result<()> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

impl Framebuffer {
    pub fn open() -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/fb0")?;
        let mut variable = VariableInfo::default();
        // SAFETY: The C layout contains integers only; ioctl fills it synchronously.
        let mut fixed: FixedInfo = unsafe { std::mem::zeroed() };
        unsafe {
            checked(libc::ioctl(file.as_raw_fd(), 0x4600, &mut variable))?;
            checked(libc::ioctl(file.as_raw_fd(), 0x4602, &mut fixed))?;
        }
        if variable.xres != 640 || variable.yres != 480 || variable.rotate != 0 {
            return Err(io::Error::other("Expected the installed 640x480 panel"));
        }
        let format = match variable.bits_per_pixel {
            32 if variable.red.offset == 16 && variable.blue.offset == 0 => ColorFormat::BGRA8888,
            32 if variable.red.offset == 0 && variable.blue.offset == 16 => ColorFormat::RGBA8888,
            16 if variable.red.offset == 11 && variable.blue.offset == 0 => ColorFormat::RGB565,
            _ => return Err(io::Error::other("Unsupported installed panel format")),
        };
        let stride = fixed.line_length as usize;
        let offset = variable.yoffset as usize * stride
            + variable.xoffset as usize * format.bytes_per_pixel();
        let last = offset + 479 * stride + 640 * format.bytes_per_pixel();
        if stride < 640 * format.bytes_per_pixel() || last > fixed.smem_len as usize {
            return Err(io::Error::other("Panel stride does not fit its storage"));
        }
        record_init(
            "after_fb_open",
            serde_json::json!({
                "fd": file.as_raw_fd(),
                "path": "/dev/fb0",
                "transport": "pwrite",
                "len": fixed.smem_len,
                "offset": 0,
                "view_offset": offset,
                "format": format!("{format:?}"),
                "stride": stride,
                "width": variable.xres,
                "height": variable.yres,
            }),
        )?;
        Ok(Self {
            file,
            width: 640,
            height: 480,
            format,
            stride,
            offset,
            first_write: true,
        })
    }

    pub fn flush(&mut self, pixels: &[u8], area: PhysicalRect) -> io::Result<()> {
        let bytes = self.format.bytes_per_pixel();
        let left = usize::from(area.x()).min(640);
        let top = usize::from(area.y()).min(480);
        let right = usize::from(area.right()).min(640);
        let bottom = usize::from(area.bottom()).min(480);
        if right <= left || bottom <= top {
            return Ok(());
        }
        if self.stride == 640 * bytes {
            let source = top * self.stride;
            let length = (bottom - top) * self.stride;
            if self.first_write {
                record_init(
                    "before_fb_write",
                    serde_json::json!({
                        "transport":"pwrite", "fd":self.file.as_raw_fd(),
                        "path":"/dev/fb0", "offset":self.offset + source,
                        "rows":bottom-top, "bytes":length, "stride":self.stride,
                        "format":format!("{:?}",self.format),
                    }),
                )?;
                self.first_write = false;
            }
            return self.file.write_all_at(
                &pixels[source..source + length],
                (self.offset + source) as u64,
            );
        }
        for row in top..bottom {
            let source = (row * 640 + left) * bytes;
            let target = self.offset + row * self.stride + left * bytes;
            let length = (right - left) * bytes;
            if length == 0 {
                continue;
            }
            if self.first_write {
                record_init(
                    "before_fb_write",
                    serde_json::json!({
                        "transport": "pwrite",
                        "fd": self.file.as_raw_fd(),
                        "path": "/dev/fb0",
                        "offset": target,
                        "row": row,
                        "row_bytes": length,
                        "stride": self.stride,
                        "format": format!("{:?}", self.format),
                    }),
                )?;
                self.first_write = false;
            }
            self.file
                .write_all_at(&pixels[source..source + length], target as u64)?;
        }
        Ok(())
    }
}

pub struct Terminal {
    file: File,
    original: libc::termios,
    mode: libc::c_int,
    bytes: Vec<u8>,
    escaped_at: Option<Instant>,
}

impl Terminal {
    pub fn open() -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty1")?;
        // SAFETY: tcgetattr initializes the complete termios struct before use.
        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        let mut mode = 0;
        unsafe {
            checked(libc::tcgetattr(file.as_raw_fd(), &mut original))?;
            checked(libc::ioctl(file.as_raw_fd(), 0x4b3b, &mut mode))?;
        }
        let mut terminal = Self {
            file,
            original,
            mode,
            bytes: Vec::new(),
            escaped_at: None,
        };
        let mut raw = terminal.original;
        unsafe {
            libc::cfmakeraw(&mut raw);
            raw.c_cc[libc::VMIN] = 0;
            raw.c_cc[libc::VTIME] = 0;
            checked(libc::tcsetattr(
                terminal.file.as_raw_fd(),
                libc::TCSANOW,
                &raw,
            ))?;
            checked(libc::ioctl(terminal.file.as_raw_fd(), 0x4b3a, 1))?;
            let flags = libc::fcntl(terminal.file.as_raw_fd(), libc::F_GETFL);
            checked(libc::fcntl(
                terminal.file.as_raw_fd(),
                libc::F_SETFL,
                flags | libc::O_NONBLOCK,
            ))?;
            libc::tcflush(terminal.file.as_raw_fd(), libc::TCIFLUSH);
        }
        terminal.escaped_at = None;
        Ok(terminal)
    }

    pub fn poll(&mut self) -> io::Result<Vec<Action>> {
        let mut input = [0_u8; 128];
        match self.file.read(&mut input) {
            Ok(count) => self.bytes.extend_from_slice(&input[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        let mut actions = Vec::new();
        while let Some(&first) = self.bytes.first() {
            if first == 27 {
                if self.bytes.len() == 1 {
                    let since = self.escaped_at.get_or_insert_with(Instant::now);
                    if since.elapsed() < Duration::from_millis(25) {
                        break;
                    }
                    actions.push(Action::Back);
                    self.bytes.remove(0);
                } else {
                    let sequences: &[(&[u8], Action)] = &[
                        (b"\x1b[A", Action::Up),
                        (b"\x1b[B", Action::Down),
                        (b"\x1b[C", Action::Right),
                        (b"\x1b[D", Action::Left),
                        (b"\x1bOA", Action::Up),
                        (b"\x1bOB", Action::Down),
                        (b"\x1bOC", Action::Right),
                        (b"\x1bOD", Action::Left),
                        (b"\x1b[5~", Action::PageUp),
                        (b"\x1b[6~", Action::PageDown),
                    ];
                    if let Some((sequence, action)) = sequences
                        .iter()
                        .find(|(sequence, _)| self.bytes.starts_with(sequence))
                    {
                        actions.push(*action);
                        self.bytes.drain(..sequence.len());
                    } else if sequences
                        .iter()
                        .any(|(sequence, _)| sequence.starts_with(&self.bytes))
                    {
                        break;
                    } else {
                        self.bytes.remove(0);
                    }
                }
                self.escaped_at = None;
            } else {
                self.bytes.remove(0);
                if first == b'\r' || first == b'\n' {
                    actions.push(Action::Confirm);
                } else if first == b'r' || first == b'R' {
                    actions.push(Action::Refresh);
                }
            }
        }
        Ok(actions)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: The saved terminal state belongs to the still-open tty1 fd.
        unsafe {
            libc::tcsetattr(self.file.as_raw_fd(), libc::TCSANOW, &self.original);
            libc::ioctl(self.file.as_raw_fd(), 0x4b3a, self.mode);
        }
    }
}
