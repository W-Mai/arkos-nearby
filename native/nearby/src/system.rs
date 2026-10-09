use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Serialize, de::DeserializeOwned};

use crate::{Result, require};

pub fn command(program: &str, args: &[&str], seconds: u64, check: bool) -> Result<String> {
    let mut process = Command::new(program)
        .args(args)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = process.stdout.take().unwrap();
    let mut stderr = process.stderr.take().unwrap();
    let output = thread::spawn(move || {
        let mut data = Vec::new();
        stdout.read_to_end(&mut data).map(|_| data)
    });
    let errors = thread::spawn(move || {
        let mut data = Vec::new();
        stderr.read_to_end(&mut data).map(|_| data)
    });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        if let Some(status) = process.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(-(process.id() as i32), libc::SIGKILL);
            }
            let _ = process.kill();
            let _ = process.wait();
            return Err(format!("Command timed out: {program}").into());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = output
        .join()
        .map_err(|_| "Command output reader failed")??;
    let errors = errors.join().map_err(|_| "Command error reader failed")??;
    require(
        !check || status.success(),
        &format!("{program}: {}", String::from_utf8_lossy(&errors)),
    )?;
    Ok(String::from_utf8_lossy(&output).trim().into())
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary = path.with_extension(format!("{}.next", random_id()?));
    private_write(&temporary, &serde_json::to_vec(value)?)?;
    fs::rename(&temporary, path)?;
    File::open(path.parent().ok_or("State has no parent")?)?.sync_all()?;
    Ok(())
}

pub fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub fn random_id() -> Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn wall_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub fn monotonic() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let mut value = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value);
        }
        value.tv_sec as f64 + value.tv_nsec as f64 / 1e9
    }
    #[cfg(not(target_os = "linux"))]
    {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_secs_f64()
    }
}

pub struct Lock(File);
impl Lock {
    pub fn existing(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        require(
            unsafe {
                libc::flock(
                    std::os::fd::AsRawFd::as_raw_fd(&file),
                    libc::LOCK_EX | libc::LOCK_NB,
                )
            } == 0,
            "Another native game owns the display",
        )?;
        Ok(Self(file))
    }
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        require(
            unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) } == 0,
            "Session lock failed",
        )?;
        Ok(Self(file))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.0), libc::LOCK_UN);
        }
    }
}
