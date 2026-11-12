use howff_platform::arkos;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::system::{read, write};
use crate::{Result, require};

pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const CODE_LENGTH: usize = 12;

pub fn normalize(value: &str) -> Result<String> {
    let code: String = value
        .chars()
        .filter(|c| *c != '-' && *c != ' ')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    require(
        code.len() == CODE_LENGTH && code.bytes().all(|byte| ALPHABET.contains(&byte)),
        "Use the twelve-character pairing code shown on the other handheld",
    )?;
    Ok(code)
}
pub fn display(code: &str) -> Result<String> {
    let code = normalize(code)?;
    Ok(format!("{}-{}-{}", &code[..4], &code[4..8], &code[8..]))
}
pub fn generate() -> Result<String> {
    use std::io::Read;
    let mut bytes = [0; CODE_LENGTH];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes
        .iter()
        .map(|byte| ALPHABET[(byte & 31) as usize] as char)
        .collect())
}
pub fn credential(code: &str) -> Result<(String, String)> {
    let code = normalize(code)?;
    let mut key = Sha256::new();
    key.update(b"arkos-nearby/wpa2/v1\0");
    key.update(code.as_bytes());
    let key = format!("{:x}", key.finalize());
    let mut group = Sha256::new();
    group.update(b"arkos-nearby/group/v1\0");
    group.update(code.as_bytes());
    Ok((
        key[..48].into(),
        format!("{:x}", group.finalize())[..16].into(),
    ))
}
fn path() -> std::path::PathBuf {
    arkos::application::PAIRING.path().to_path_buf()
}
pub(crate) fn validate(saved: &Value) -> Result<()> {
    require(saved["schema"] == 2, "Unsupported pairing configuration")?;
    let code = saved["pairing_code"]
        .as_str()
        .ok_or("Missing pairing code")?;
    let (key, group) = credential(code)?;
    require(
        saved["psk"] == key && saved["group_id"] == group,
        "Pairing configuration differs from its code",
    )
}
pub fn ensure_at(path: &Path) -> Result<()> {
    if path.exists() {
        let info = path.metadata()?;
        require(
            info.uid() == unsafe { libc::geteuid() } && info.mode() & 0o077 == 0,
            "Pairing configuration is not privately owned",
        )?;
        let saved: Value = read(path)?;
        return validate(&saved);
    }
    let code = generate()?;
    let (psk, group) = credential(&code)?;
    write(
        path,
        &json!({"schema":2,"psk":psk,"pairing_code":code,"group_id":group}),
    )
}
pub fn show() -> Result<Value> {
    crate::session::identity(false)?;
    let saved = crate::session::credentials()?;
    let code = display(
        saved["pairing_code"]
            .as_str()
            .ok_or("Missing pairing code")?,
    )?;
    Ok(json!({"pairing_code":code,"group_id":saved["group_id"]}))
}
pub fn set(code: &str) -> Result<Value> {
    crate::session::identity(false)?;
    let code = normalize(code)?;
    let (key, group) = credential(&code)?;
    crate::session::while_idle(|| {
        let mut saved = if path().is_file() {
            crate::session::credentials()?
        } else {
            json!({})
        };
        saved["schema"] = 2.into();
        saved["psk"] = key.into();
        saved["pairing_code"] = code.clone().into();
        saved["group_id"] = group.into();
        saved["manual_fallback"] = true.into();
        write(&path(), &saved)?;
        show()
    })
}
pub fn new_code() -> Result<Value> {
    set(&generate()?)
}
