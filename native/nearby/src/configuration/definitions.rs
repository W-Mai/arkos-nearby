use crate::context::{Context, INSTALL};
use crate::{Result, require};
use std::path::Path;

pub fn read(context: &Context, system: &Path) -> Result<Vec<(String, String)>> {
    let directory = Path::new("/run/arkos-nearby-configuration").join(crate::system::random_id()?);
    crate::system::mkdir(&directory)?;
    let result = (|| -> Result<Vec<(String, String)>> {
        let helper = format!("{INSTALL}/core-inspect{}", context.core_elf_bits);
        let mut args = vec![
            format!("NEARBY_OPTION_SYSTEM={}", system.display()),
            format!("NEARBY_OPTION_SAVE={}", directory.display()),
        ];
        if let Some(runtime) =
            crate::core_runtime::directory(&context.core_id, context.core_elf_bits)?
        {
            args.push(format!("LD_LIBRARY_PATH={runtime}"));
        }
        args.extend([
            helper,
            context.core_path.to_string_lossy().into_owned(),
            "--options".into(),
        ]);
        let output = crate::system::command(
            "env",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            15,
            true,
        )?;
        let value = output
            .lines()
            .rev()
            .find_map(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .filter(|v| v["options"].is_array())
            })
            .ok_or("核心未返回选项定义")?;
        require(
            value["library_name"].as_str() == Some(context.identity.core.name.as_str()),
            "核心元数据身份已变化",
        )?;
        Ok(value["options"]
            .as_array()
            .unwrap()
            .iter()
            .take(1024)
            .filter_map(|value| {
                Some((
                    value["key"].as_str()?.into(),
                    value["definition"].as_str()?.into(),
                ))
            })
            .collect())
    })();
    let cleanup = std::fs::remove_dir_all(&directory);
    result.and_then(|value| {
        cleanup?;
        Ok(value)
    })
}
