//! Bounded request/response IPC for a managed platform plugin process.

pub use howff_platform::ipc::{PluginRequest, PluginResponse};
use howff_platform::ipc::{read_frame, write_frame};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::{Result, require};

pub fn invoke(
    program: &Path,
    args: &[&str],
    request: PluginRequest,
    timeout: Duration,
) -> Result<PluginResponse> {
    require(
        program.is_absolute(),
        "Plugin executable must use an absolute path",
    )?;
    request.validate()?;
    let mut child = Command::new(program)
        .args(args)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("Plugin stdin unavailable")?;
    write_frame(&mut stdin, &request)?;
    drop(stdin);
    let mut stdout = child.stdout.take().ok_or("Plugin stdout unavailable")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result: howff_platform::ipc::Result<PluginResponse> = read_frame(&mut stdout);
        let _ = sender.send(result);
    });
    let response = match receiver.recv_timeout(timeout) {
        Ok(result) => result?,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("Plugin request timed out".into());
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err("Plugin response reader failed".into());
        }
    };
    let status = child.wait()?;
    require(status.success(), "Plugin process exited with an error")?;
    response.validate_for(request.request_id)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip_preserves_request() {
        let request = PluginRequest::new(
            7,
            "session-1".into(),
            "paths".into(),
            serde_json::json!({"root":"/roms"}),
        )
        .unwrap();
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &request).unwrap();
        let decoded: PluginRequest = read_frame(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(decoded.request_id, 7);
        assert_eq!(decoded.capability, "paths");
    }

    #[test]
    fn rejects_invalid_response_identity() {
        let response = PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: 8,
            ok: true,
            body: Some(Value::Null),
            error: None,
        };
        assert!(response.validate_for(7).is_err());
    }

    #[test]
    fn kills_a_plugin_that_does_not_respond() {
        let request = PluginRequest::new(1, "session".into(), "paths".into(), Value::Null).unwrap();
        let error = invoke(
            Path::new("/bin/sh"),
            &["-c", "sleep 1"],
            request,
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
    }
}
