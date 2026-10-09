use std::path::Path;

use crate::content::digest;
use crate::context::INSTALL;
use crate::{Result, require};

pub const LIBRARIES: [(&str, &str); 2] = [
    (
        "libstdc++.so.6",
        "c116f7a27884c399ed3ab89eed9c0150a8f09ed5990b5a36d2c18ac14c0a3d66",
    ),
    (
        "libgcc_s.so.1",
        "3f81f337c558b6a08924b05dc73b5fcc117d39c11ba306a49f1f973b960adc2c",
    ),
];

pub fn directory(core_id: &str, bits: u8) -> Result<Option<String>> {
    if core_id != "mame" || bits != 64 {
        return Ok(None);
    }
    let root = Path::new(INSTALL).join("compat/arm64");
    for (name, sha256) in LIBRARIES {
        require(
            digest(&root.join(name))? == sha256,
            "Private core runtime differs",
        )?;
    }
    Ok(Some(root.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_runtime_is_scoped_to_the_matching_mame_abi() {
        for (core, bits) in [
            ("mame", 32),
            ("mame2003_plus", 64),
            ("gpsp", 64),
            ("tgbdual", 64),
        ] {
            assert!(directory(core, bits).unwrap().is_none());
        }
        for (name, pin) in LIBRARIES {
            assert!(!name.contains('/'));
            assert!(crate::protocol::hex(pin, 64));
        }
    }
}
