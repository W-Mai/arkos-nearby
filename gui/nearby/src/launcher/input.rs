use super::LauncherHandle;
use super::controls::{self, Controls};
use crate::model::Action;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

const X: u16 = 307;
const FUNCTION: u16 = 708;
const DEVICE: &str = "/dev/input/by-path/platform-odroidgo3-joypad-event-joystick";

pub struct Input {
    file: File,
    x: bool,
    function: bool,
    launcher: bool,
    dropped: bool,
    controls: Controls,
    axes: Vec<(u16, i32, i32)>,
}

impl Input {
    pub fn open() -> io::Result<Option<Self>> {
        let launcher = match std::env::var("ARKOS_NEARBY_LAUNCHER_INPUT").as_deref() {
            Ok("1") => true,
            Ok("0") => false,
            _ => return Ok(None),
        };
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(DEVICE)?;
        let mut result = Self {
            file,
            x: false,
            function: false,
            launcher,
            dropped: false,
            controls: Controls::default(),
            axes: Vec::new(),
        };
        result.resync()?;
        Ok(Some(result))
    }

    fn resync(&mut self) -> io::Result<()> {
        let mut bits = [0u8; 96];
        // SAFETY: EVIOCGKEY writes at most the encoded 96-byte buffer length.
        let result =
            unsafe { libc::ioctl(self.file.as_raw_fd(), 0x80604518u32 as _, bits.as_mut_ptr()) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        self.x = bits[usize::from(X) / 8] & (1 << (X % 8)) != 0;
        self.function = bits[usize::from(FUNCTION) / 8] & (1 << (FUNCTION % 8)) != 0;
        let mut pressed: Vec<_> = (0_u16..768)
            .filter(|code| bits[usize::from(*code) / 8] & (1 << (*code % 8)) != 0)
            .collect();
        self.axes.clear();
        for code in [0_u16, 1, 3, 4] {
            let mut info = [0_i32; 6];
            // SAFETY: EVIOCGABS writes the six-i32 input_absinfo buffer.
            let result = unsafe {
                libc::ioctl(
                    self.file.as_raw_fd(),
                    (0x80184540_u32 + u32::from(code)) as _,
                    info.as_mut_ptr(),
                )
            };
            if result >= 0 {
                self.axes.push((code, info[1], info[2]));
                if controls::axis(code, info[0], info[1], info[2]).is_some() {
                    pressed.push(1024 + code);
                }
            }
        }
        self.controls.reset(pressed.into_iter());
        Ok(())
    }

    pub fn poll(&mut self, state: &LauncherHandle, at: u64) -> io::Result<Vec<Action>> {
        let mut actions = Vec::new();
        let mut data = [0u8; 24];
        loop {
            match self.file.read(&mut data) {
                Ok(24) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    state.cancel();
                    return Err(e);
                }
                _ => {
                    state.cancel();
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Gamepad disconnected",
                    ));
                }
            }
            let kind = u16::from_ne_bytes(data[16..18].try_into().unwrap());
            let code = u16::from_ne_bytes(data[18..20].try_into().unwrap());
            let value = i32::from_ne_bytes(data[20..24].try_into().unwrap());
            if kind == 0 && code == 3 {
                self.dropped = true;
                state.cancel();
            }
            if self.dropped {
                if kind == 0 && code == 0 {
                    self.resync()?;
                    self.dropped = false;
                }
                continue;
            }
            if kind == 3 {
                if let Some((_, min, max)) = self.axes.iter().find(|axis| axis.0 == code) {
                    let action = controls::axis(code, value, *min, *max);
                    if action.is_some() {
                        state.cancel();
                    }
                    actions.extend(self.controls.update(1024 + code, action, at));
                }
                continue;
            }
            if kind != 1 || value == 2 {
                continue;
            }
            match code {
                FUNCTION => {
                    self.function = value == 1;
                    if self.function {
                        state.cancel();
                        if self.x && self.launcher {
                            actions.push(Action::LauncherHome);
                        }
                    }
                }
                X => {
                    if value == 1 && !self.x {
                        if self.function && self.launcher {
                            state.cancel();
                            actions.push(Action::LauncherHome);
                        } else {
                            state.press(at);
                        }
                    } else if value == 0 && self.x {
                        state.release(at);
                    }
                    self.x = value == 1;
                }
                _ => {
                    if value == 1 {
                        state.cancel();
                    }
                    actions.extend(self.controls.update(
                        code,
                        if value == 1 {
                            controls::button_for(code, self.launcher)
                        } else {
                            None
                        },
                        at,
                    ));
                }
            }
        }
        actions.extend(self.controls.repeat(at));
        Ok(actions)
    }
}
