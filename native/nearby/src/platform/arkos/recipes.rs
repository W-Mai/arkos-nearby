use crate::platform::content::Launch;
use howff_platform::arkos;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Result, require};

#[derive(Clone, Debug)]
pub struct System {
    pub name: String,
    pub title: String,
    pub path: PathBuf,
    pub extensions: String,
    command: String,
    emulators: Vec<Emulator>,
}

#[derive(Clone, Debug)]
struct Emulator {
    name: String,
    cores: Vec<String>,
    command: Option<String>,
}

fn field(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.children()
        .find(|child| child.has_tag_name(name))
        .and_then(|child| child.text())
        .unwrap_or_default()
        .trim()
        .into()
}

pub fn parse(text: &str) -> Result<Vec<System>> {
    let mut systems = Vec::new();
    // Parse each system separately: manufacturer Options commands contain bare ampersands.
    for block in text.split("<system>").skip(1) {
        let Some((block, _)) = block.split_once("</system>") else {
            continue;
        };
        let xml = format!("<system>{block}</system>");
        let Ok(doc) = roxmltree::Document::parse(&xml) else {
            continue;
        };
        let node = doc.root_element();
        let path = PathBuf::from(field(node, "path"));
        if !arkos::ROM_ROOTS.iter().any(|root| path.starts_with(root)) {
            continue;
        }
        let emulators = node
            .descendants()
            .filter(|n| n.has_tag_name("emulator"))
            .map(|n| Emulator {
                name: n.attribute("name").unwrap_or_default().into(),
                cores: n
                    .descendants()
                    .filter(|c| c.has_tag_name("core"))
                    .filter_map(|c| c.text().map(str::to_owned))
                    .collect(),
                command: n.attribute("command").map(str::to_owned),
            })
            .collect();
        let command = field(node, "command");
        if command.is_empty() {
            continue;
        }
        systems.push(System {
            name: field(node, "name"),
            title: field(node, "fullname"),
            path,
            extensions: field(node, "extension"),
            command,
            emulators,
        });
    }
    require(!systems.is_empty(), "No local system launch recipes")?;
    Ok(systems)
}

pub fn load() -> Result<Vec<System>> {
    let path = [
        arkos::home::ES_SYSTEMS.as_str(),
        arkos::firmware::ES_SYSTEMS,
    ]
    .into_iter()
    .find(|p| Path::new(p).is_file())
    .ok_or("Missing system catalog")?;
    require(
        fs::metadata(path)?.len() <= 4 * 1024 * 1024,
        "System catalog is too large",
    )?;
    parse(&fs::read_to_string(path)?)
}

pub(crate) fn settings(text: &str) -> BTreeMap<String, String> {
    let xml = format!(
        "<settings>{}</settings>",
        text.lines()
            .filter(|l| !l.trim_start().starts_with("<?xml"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    roxmltree::Document::parse(&xml)
        .map(|doc| {
            doc.descendants()
                .filter_map(|n| Some((n.attribute("name")?.into(), n.attribute("value")?.into())))
                .collect()
        })
        .unwrap_or_default()
}

fn atom(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

impl System {
    pub fn accepts(&self, path: &Path) -> bool {
        path.extension().is_some_and(|ext| {
            self.extensions.split_whitespace().any(|e| {
                e.trim_start_matches('.')
                    .eq_ignore_ascii_case(&ext.to_string_lossy())
            })
        })
    }

    fn expand(
        &self,
        game: &Path,
        defaults: &BTreeMap<String, String>,
        overrides: &BTreeMap<String, String>,
    ) -> Result<String> {
        let preferred = overrides
            .get("emulator")
            .filter(|s| !s.is_empty())
            .or_else(|| defaults.get(&format!("{}.emulator", self.name)));
        let emulator = self
            .emulators
            .iter()
            .find(|e| Some(&e.name) == preferred)
            .or_else(|| self.emulators.first());
        require(
            preferred.is_none_or(|p| p.is_empty() || self.emulators.iter().any(|e| &e.name == p)),
            "Configured emulator is unavailable",
        )?;
        let preferred_core = overrides
            .get("core")
            .filter(|s| !s.is_empty())
            .or_else(|| defaults.get(&format!("{}.core", self.name)));
        let core = emulator
            .and_then(|e| {
                e.cores
                    .iter()
                    .find(|c| Some(*c) == preferred_core)
                    .or_else(|| e.cores.first())
            })
            .map(String::as_str)
            .unwrap_or("");
        if let Some(core_override) = overrides.get("core").filter(|s| !s.is_empty()) {
            require(
                emulator.is_some_and(|e| e.cores.contains(core_override)),
                "Configured game core is unavailable",
            )?;
        }
        let governor = overrides
            .get("governor")
            .filter(|s| !s.is_empty())
            .or_else(|| defaults.get(&format!("{}.governor", self.name)))
            .or_else(|| defaults.get("GlobalPerformanceGovernor"))
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("performance");
        let emulator_name = emulator.map(|e| e.name.as_str()).unwrap_or("");
        require(
            [emulator_name, core, governor].iter().all(|v| atom(v)),
            "Invalid local launch identifier",
        )?;
        require(
            matches!(governor, "performance" | "ondemand" | "powersave"),
            "Unsupported system governor",
        )?;
        let template = emulator
            .and_then(|e| e.command.as_ref())
            .unwrap_or(&self.command);
        // Substitute once so tokens and shell syntax inside filenames remain literal data.
        let mut output = String::new();
        let mut remaining = template.as_str();
        while let Some(pos) = remaining.find('%') {
            output.push_str(&remaining[..pos]);
            remaining = &remaining[pos..];
            let end = remaining[1..]
                .find('%')
                .ok_or("Unclosed launch placeholder")?
                + 1;
            let token = &remaining[..=end];
            output.push_str(&match token {
                "%EMULATOR%" => emulator_name.into(),
                "%CORE%" => core.into(),
                "%GOVERNOR%" => governor.into(),
                "%ROM%" => quote(game.to_str().ok_or("Invalid game path")?),
                "%SYSTEM%" => quote(&self.name),
                "%HOME%" => quote(arkos::home::ROOT),
                "%BASENAME%" => quote(
                    &game
                        .file_stem()
                        .ok_or("Missing game name")?
                        .to_string_lossy(),
                ),
                _ => return Err(format!("Unsupported launch placeholder: {token}").into()),
            });
            remaining = &remaining[end + 1..];
        }
        output.push_str(remaining);
        if let Some(command) = output.strip_suffix("; sudo perfnorm") {
            output = format!(
                "{command}; nearby_game_status=$?; sudo perfnorm; exit \"$nearby_game_status\""
            );
        }
        Ok(output)
    }
}

fn hooks(event: &str, args: &[&str]) -> Result<String> {
    let mut commands = Vec::new();
    for base in [arkos::firmware::ES_HOOKS, arkos::home::ES_HOOKS.as_str()] {
        let directory = Path::new(base).join(event);
        if !directory.is_dir() {
            continue;
        }
        let mut files: Vec<_> = fs::read_dir(directory)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        for file in files {
            commands.push(format!(
                "{} {}",
                quote(file.to_str().ok_or("Invalid hook path")?),
                args.iter()
                    .map(|arg| quote(arg))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    Ok(commands.join("; "))
}

pub fn finish() -> Result<()> {
    let command = hooks("game-end", &["", ""])?;
    if !command.is_empty() {
        crate::system::command("/bin/bash", &["-c", &command], 30, false)?;
    }
    crate::system::command(
        "sudo",
        &["-n", arkos::system_program::PERFORMANCE_HELPER.as_str()],
        30,
        true,
    )?;
    Ok(())
}

pub(crate) fn recipe(path: &Path) -> Result<Launch> {
    let path = path.canonicalize()?;
    require(path.is_file(), "Game file is unavailable")?;
    let systems = load()?;
    let system = systems
        .iter()
        .filter(|s| path.starts_with(&s.path) && s.accepts(&path))
        .max_by_key(|s| s.path.components().count())
        .ok_or("No system launch recipe for this game")?;
    let defaults =
        settings(&fs::read_to_string(arkos::home::ES_SETTINGS.as_str()).unwrap_or_default());
    let mut overrides = BTreeMap::new();
    let list = super::metadata::path(&system.path);
    if fs::metadata(&list).is_ok_and(|m| m.len() <= 32 * 1024 * 1024) {
        let text = fs::read_to_string(list)?;
        let document = roxmltree::Document::parse(&text)?;
        for game in document.descendants().filter(|n| n.has_tag_name("game")) {
            if system
                .path
                .join(field(game, "path"))
                .canonicalize()
                .is_ok_and(|p| p == path)
            {
                for name in ["emulator", "core", "governor"] {
                    overrides.insert(name.into(), field(game, name));
                }
                break;
            }
        }
    }
    let command = system.expand(&path, &defaults, &overrides)?;
    Ok(Launch { command, path })
}

pub fn prepare(path: &Path) -> Result<Launch> {
    let Launch { mut command, path } = recipe(path)?;
    let before = hooks(
        "game-start",
        &[
            path.to_str().ok_or("Invalid game path")?,
            &path
                .file_stem()
                .ok_or("Missing game name")?
                .to_string_lossy(),
        ],
    )?;
    if !before.is_empty() {
        command = format!("{before}; {command}");
    }
    Ok(Launch { command, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_recipes_preserve_overrides_and_quote_content_once() {
        let systems=parse("<systemList><system><name>nes</name><path>/roms/nes</path><extension>.nes</extension><command>%EMULATOR% -L %CORE% %ROM% %GOVERNOR%</command><emulators><emulator name='retroarch'><cores><core>nestopia</core><core>fceumm</core></cores></emulator></emulators></system><system><path>/opt/system</path><command>bad &amp; tools</command></system></systemList>").unwrap();
        assert_eq!(systems.len(), 1);
        let overrides = BTreeMap::from([
            ("core".into(), "fceumm".into()),
            ("governor".into(), "ondemand".into()),
        ]);
        let path = Path::new("/roms/nes/a'$(id)%CORE%.nes");
        let result = systems[0]
            .expand(path, &BTreeMap::new(), &overrides)
            .unwrap();
        assert_eq!(
            result,
            "retroarch -L fceumm '/roms/nes/a'\\''$(id)%CORE%.nes' ondemand"
        );
        assert!(systems[0].accepts(path));
        assert!(!systems[0].accepts(Path::new("/roms/nes/save.srm")));
        assert!(
            systems[0]
                .expand(
                    path,
                    &BTreeMap::new(),
                    &BTreeMap::from([("core".into(), "missing".into())])
                )
                .is_err()
        );
    }
}
