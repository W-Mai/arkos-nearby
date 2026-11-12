use howff_platform::arkos;
use std::path::Path;

use crate::content::digest;
use crate::context::Context;
use crate::handheld_link::LinkMode;
use crate::{Result, require};

pub const FRONTEND_SHA: &str = "81f3cf1b8850f9457730346d3f973dfb03d5316d62c4bb92b1654372bbc86e77";
pub const ASSET_SHA: &str = "b3439305d945e1ca4187131cf176d7b97a377e908555f85088dda387c3eb55bb";
pub const ASSET: &str = arkos::application::COMPRESSION.as_str();

pub fn eligible(context: &Context) -> bool {
    context.frontend == "retroarch"
        && context.frontend_path == Path::new(arkos::firmware::RETROARCH.as_str())
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
