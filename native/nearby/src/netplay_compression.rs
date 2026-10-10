use std::path::Path;

use crate::content::digest;
use crate::context::Context;
use crate::handheld_link::LinkMode;
use crate::{Result, require};

pub const FRONTEND_SHA: &str = "81f3cf1b8850f9457730346d3f973dfb03d5316d62c4bb92b1654372bbc86e77";
pub const ASSET_SHA: &str = "381d05d1ff68ff5f4eca503965f35a6fe99922e280f4e52620e68b21dca0d633";
pub const ASSET: &str = "/opt/arkos-nearby/assets/netplay-compression.so";

pub fn eligible(context: &Context) -> bool {
    context.frontend == "retroarch"
        && context.frontend_path == Path::new("/opt/retroarch/bin/retroarch")
        && context.frontend_sha256 == FRONTEND_SHA
        && context.core_elf_bits == 64
        && matches!(context.link_mode, None | Some(LinkMode::GbDual))
}

pub fn environment(context: &Context) -> Result<Option<String>> {
    if !eligible(context) {
        return Ok(None);
    }
    require(
        digest(Path::new(ASSET))? == ASSET_SHA,
        "Installed Netplay compression helper differs",
    )?;
    Ok(Some(format!("LD_PRELOAD={ASSET}")))
}
