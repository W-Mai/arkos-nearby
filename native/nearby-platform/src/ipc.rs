//! Bounded JSON frames shared by the host and platform plugin processes.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::fmt;
use std::io::{Read, Write};

pub const FRAME_SCHEMA: u8 = 1;
pub const MAX_FRAME: usize = 64 * 1024;
const MAX_SESSION: usize = 128;
const MAX_CAPABILITY: usize = 64;
const MAX_ERROR: usize = 512;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Json(error) => error.fmt(f),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRequest {
    pub schema: u8,
    pub request_id: u64,
    pub session_id: String,
    pub capability: String,
    pub body: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginResponse {
    pub schema: u8,
    pub request_id: u64,
    pub ok: bool,
    pub body: Option<Value>,
    pub error: Option<String>,
}

impl PluginRequest {
    pub fn new(
        request_id: u64,
        session_id: String,
        capability: String,
        body: Value,
    ) -> Result<Self> {
        let value = Self {
            schema: FRAME_SCHEMA,
            request_id,
            session_id,
            capability,
            body,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != FRAME_SCHEMA {
            return Err(Error::Invalid("Unsupported plugin request schema"));
        }
        if self.request_id == 0 {
            return Err(Error::Invalid("Invalid plugin request id"));
        }
        if self.session_id.is_empty()
            || self.session_id.len() > MAX_SESSION
            || self.session_id.chars().any(char::is_control)
        {
            return Err(Error::Invalid("Invalid plugin session id"));
        }
        if self.capability.is_empty()
            || self.capability.len() > MAX_CAPABILITY
            || !self.capability.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
            })
        {
            return Err(Error::Invalid("Invalid plugin capability"));
        }
        Ok(())
    }
}

impl PluginResponse {
    pub fn validate_for(&self, request_id: u64) -> Result<()> {
        if self.schema != FRAME_SCHEMA {
            return Err(Error::Invalid("Unsupported plugin response schema"));
        }
        if self.request_id != request_id {
            return Err(Error::Invalid("Plugin response id does not match request"));
        }
        if self.ok {
            if self.error.is_some() {
                return Err(Error::Invalid(
                    "Successful plugin response contains an error",
                ));
            }
        } else {
            if self.body.is_some() {
                return Err(Error::Invalid("Failed plugin response contains a body"));
            }
            let error = self.error.as_deref().unwrap_or_default();
            if error.is_empty() || error.len() > MAX_ERROR || error.chars().any(char::is_control) {
                return Err(Error::Invalid("Invalid plugin response error"));
            }
        }
        Ok(())
    }
}

pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Invalid("Plugin frame exceeds the size limit"));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> Result<T> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(Error::Invalid("Invalid plugin frame length"));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip_is_bounded() {
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
    fn response_identity_is_checked() {
        let response = PluginResponse {
            schema: FRAME_SCHEMA,
            request_id: 8,
            ok: true,
            body: Some(Value::Null),
            error: None,
        };
        assert!(response.validate_for(7).is_err());
    }
}
