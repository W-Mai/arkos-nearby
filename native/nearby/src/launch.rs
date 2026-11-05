use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::context::{Context, INSTALL};
use crate::system::{mkdir, private_write, random_id, write};
use crate::{Result, require};

const BASE: &str = "/run/arkos-nearby-launch";

fn path(id: &str) -> Result<PathBuf> {
    require(crate::protocol::hex(id, 32), "Invalid launch identity")?;
    Ok(Path::new(BASE).join(id))
}

pub fn finish(id: &str) -> Result<()> {
    require(
        unsafe { libc::geteuid() } == 0,
        "Launch cleanup requires root",
    )?;
    let directory = path(id)?;
    if !directory.exists() {
        return Ok(());
    }
    let metadata = directory.symlink_metadata()?;
    require(
        metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o022 == 0,
        "Launch directory ownership differs",
    )?;
    crate::arcade::stop_launch(&directory, id)?;
    fs::remove_dir_all(directory)?;
    Ok(())
}

struct Scope {
    id: String,
    retained: bool,
    created: bool,
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.created && !self.retained {
            let _ = finish(&self.id);
        }
    }
}

pub(crate) fn append_configuration(args: &mut Vec<String>, path: &Path) -> Result<()> {
    let mut merged = Vec::new();
    let mut retained = Vec::new();
    let mut values = std::mem::take(args).into_iter();
    while let Some(value) = values.next() {
        if value == "--appendconfig" {
            merged.push(values.next().ok_or("Missing appended configuration")?);
        } else if let Some(value) = value.strip_prefix("--appendconfig=") {
            merged.push(value.into());
        } else {
            retained.push(value);
        }
    }
    merged.push(
        path.to_str()
            .ok_or("Invalid launch configuration path")?
            .into(),
    );
    retained.push(format!("--appendconfig={}", merged.join("|")));
    *args = retained;
    Ok(())
}

fn options(
    context: &Context,
    directory: &Path,
    settings: &crate::game::Settings,
) -> Result<Option<PathBuf>> {
    let overrides = crate::core_choice::solo_options(&context.game_path, &context.core_id);
    if let Some(overrides) = overrides {
        let file = directory.join("core-options.cfg");
        private_write(
            &file,
            crate::game::isolated(std::str::from_utf8(&settings.options)?, overrides).as_bytes(),
        )?;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
        let config = directory.join("launch.cfg");
        private_write(
            &config,
            format!(
                "core_options_path = \"{}\"\ngame_specific_options = \"false\"\n",
                file.display()
            )
            .as_bytes(),
        )?;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o644))?;
        Ok(Some(config))
    } else {
        Ok(None)
    }
}

pub fn resolve(frontend: &str, args: &[String]) -> Result<()> {
    let mut arguments = args.to_vec();
    let save_ticket = crate::saves::launch::take(&mut arguments)?;
    let args = &arguments;
    require(
        unsafe { libc::geteuid() } == 0,
        "Launch resolution requires root",
    )?;
    require(
        matches!(frontend, "retroarch" | "retroarch32"),
        "Unknown frontend",
    )?;
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--menu" | "--version" | "--help"))
        || !args.iter().any(|arg| {
            matches!(arg.as_str(), "-L" | "--libretro") || arg.starts_with("--libretro=")
        })
    {
        let id = random_id()?;
        let mut output = std::io::stdout().lock();
        for arg in [&id, &frontend.to_owned(), &String::new()]
            .into_iter()
            .chain(args.iter())
        {
            output.write_all(arg.as_bytes())?;
            output.write_all(&[0])?;
        }
        return Ok(());
    }
    crate::configuration::recover()?;
    let initial = crate::context::parse(frontend, args)?;
    let mut scope = Scope {
        id: random_id()?,
        retained: false,
        created: false,
    };
    let base = Path::new(BASE);
    fs::create_dir_all(base)?;
    fs::set_permissions(base, fs::Permissions::from_mode(0o755))?;
    let directory = path(&scope.id)?;
    fs::create_dir(&directory)?;
    scope.created = true;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    let (mut context, settings) = crate::arcade::single_player(&directory, &scope.id, &initial)?;
    if let Some(id) = save_ticket {
        crate::session::try_while_idle(|| {
            crate::saves::launch::apply(&id, &mut context, &settings, &directory)
        })?;
    }
    let runtime = crate::core_runtime::directory(&context.core_id, context.core_elf_bits)?
        .unwrap_or_default();
    if let Some(config) = options(&context, &directory, &settings)? {
        append_configuration(&mut context.argv, &config)?;
    }
    let record = json!({"launch_id":scope.id,"core_id":context.core_id,"core_sha256":context.core_sha256,
        "origin":context.origin(),"game_path":context.game_path,"frontend":context.frontend});
    write(&directory.join("selection.json"), &record)?;
    mkdir(&Path::new(INSTALL).join("logs"))?;
    write(
        &Path::new(INSTALL).join("logs/last-single-player-selection.json"),
        &record,
    )?;
    let mut output = std::io::stdout().lock();
    for arg in [&scope.id, &context.frontend, &runtime]
        .into_iter()
        .chain(context.argv.iter())
    {
        require(!arg.contains('\0'), "Launch argument contains NUL")?;
        output.write_all(arg.as_bytes())?;
        output.write_all(&[0])?;
    }
    output.flush()?;
    scope.retained = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_overrides_preserve_existing_configs_and_game_arguments() {
        let mut args = vec![
            "--appendconfig=one.cfg".into(),
            "--appendconfig".into(),
            "two.cfg".into(),
            "-L".into(),
            "core.so".into(),
            "/roms/snes/Game ' name.zip".into(),
        ];
        append_configuration(&mut args, Path::new("/run/launch.cfg")).unwrap();
        assert_eq!(
            args,
            [
                "-L",
                "core.so",
                "/roms/snes/Game ' name.zip",
                "--appendconfig=one.cfg|two.cfg|/run/launch.cfg"
            ]
        );
        assert!(path("../../etc").is_err());
    }
}
