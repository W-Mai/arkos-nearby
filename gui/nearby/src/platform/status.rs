use howff_platform::device::{LinuxStatus, Status};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct Sample {
    clock: String,
    status: Status,
}

/// One replaceable sample; slow frames never accumulate a telemetry backlog.
pub struct Monitor {
    latest: Arc<Mutex<Option<Sample>>>,
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Monitor {
    pub fn start() -> std::io::Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let output = Arc::clone(&latest);
        let (stop, stopped) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("howff-status".into())
            .spawn(move || {
                let source = LinuxStatus::new("/");
                let mut hardware = None;
                let mut hardware_at = Instant::now();
                loop {
                    if hardware.is_none() || hardware_at.elapsed() >= Duration::from_secs(5) {
                        hardware = Some(source.read());
                        hardware_at = Instant::now();
                    }
                    let sample = Sample {
                        clock: local_time(),
                        status: hardware.unwrap_or_default(),
                    };
                    if let Ok(mut slot) = output.lock() {
                        *slot = Some(sample);
                    }
                    if stopped.recv_timeout(Duration::from_secs(1))
                        != Err(mpsc::RecvTimeoutError::Timeout)
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            latest,
            stop,
            worker: Some(worker),
        })
    }

    pub fn present(&self, model: &crate::render::device_status::DeviceStatusHandle) {
        let sample = self.latest.try_lock().ok().and_then(|mut slot| slot.take());
        if let Some(sample) = sample {
            model.present(sample.clock, sample.status);
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn local_time() -> String {
    // SAFETY: time() accepts null and localtime_r writes into the supplied tm buffer.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
        if libc::localtime_r(&now, local.as_mut_ptr()).is_null() {
            return "--:--".into();
        }
        let local = local.assume_init();
        format!("{:02}:{:02}", local.tm_hour, local.tm_min)
    }
}
