use crate::{Result, require};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub value: String,
    pub source: PathBuf,
}
pub type Values = BTreeMap<String, Entry>;

#[derive(Clone)]
pub struct Input {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}
impl Input {
    pub fn read(path: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;
        match fs::metadata(path) {
            Ok(meta) => {
                require(
                    meta.is_file() && meta.len() <= 2 * 1024 * 1024,
                    "配置文件过大或类型不符",
                )?;
                Ok(Self {
                    path: path.into(),
                    bytes: Some(fs::read(path)?),
                    mode: meta.mode() & 0o777,
                    uid: meta.uid(),
                    gid: meta.gid(),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                path: path.into(),
                bytes: None,
                mode: 0o644,
                uid: unsafe { libc::geteuid() },
                gid: unsafe { libc::getegid() },
            }),
            Err(error) => Err(error.into()),
        }
    }
    pub fn revision(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.path.as_os_str().as_encoded_bytes());
        hash.update([u8::from(self.bytes.is_some())]);
        hash.update(self.bytes.as_deref().unwrap_or_default());
        hash.update(self.mode.to_le_bytes());
        hash.update(self.uid.to_le_bytes());
        hash.update(self.gid.to_le_bytes());
        format!("{:x}", hash.finalize())
    }
}

pub fn expand(value: &str, home: &Path, base: &Path) -> PathBuf {
    let path = if value == "~" {
        home.to_path_buf()
    } else if let Some(tail) = value.strip_prefix("~/") {
        home.join(tail)
    } else {
        PathBuf::from(value)
    };
    let path = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            _ => result.push(part.as_os_str()),
        }
    }
    result
}

pub fn value(line: &str) -> Option<String> {
    let line = line.trim();
    if let Some(quoted) = line.strip_prefix('"') {
        Some(quoted.split('"').next()?.into())
    } else {
        Some(line.split_whitespace().next()?.split('#').next()?.into())
    }
}
pub fn assignment(line: &str) -> Option<(&str, String)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (key, text) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
    {
        None
    } else {
        Some((key, value(text)?))
    }
}

#[derive(Clone, Default)]
pub struct Graph {
    pub inputs: BTreeMap<PathBuf, Input>,
    pub preview: BTreeMap<PathBuf, Option<Vec<u8>>>,
    locations: BTreeMap<PathBuf, Location>,
    access: Option<Access>,
}

#[derive(Clone)]
pub struct Access {
    pub uid: u32,
    pub groups: Vec<u32>,
}
impl Access {
    fn permits(&self, meta: &fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        let shift = if self.uid == meta.uid() {
            6
        } else if self.groups.contains(&meta.gid()) {
            3
        } else {
            0
        };
        self.uid == 0 || (meta.mode() >> shift) & 3 == 3
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Location {
    identity: Option<(u64, u64, u32, u32, u32)>,
    directory: bool,
    writable: bool,
    error: Option<i32>,
}
impl Location {
    fn read(path: &Path, access: Option<&Access>) -> Self {
        use std::os::unix::fs::MetadataExt;
        match fs::metadata(path) {
            Ok(meta) => Self {
                identity: Some((meta.dev(), meta.ino(), meta.mode(), meta.uid(), meta.gid())),
                directory: meta.is_dir(),
                writable: access.is_none_or(|access| access.permits(&meta))
                    && std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).is_ok_and(
                        |path| unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 },
                    ),
                error: None,
            },
            Err(error) => Self {
                identity: None,
                directory: false,
                writable: false,
                error: error.raw_os_error(),
            },
        }
    }
}
impl Graph {
    pub fn set_access(&mut self, access: Option<Access>) {
        self.access = access;
    }
    pub fn with_preview(preview: BTreeMap<PathBuf, Option<Vec<u8>>>) -> Self {
        Self {
            preview,
            ..Self::default()
        }
    }
    fn location(&mut self, path: &Path) -> Result<&Location> {
        require(
            self.locations.len() < 256 || self.locations.contains_key(path),
            "目录依赖过多",
        )?;
        Ok(self
            .locations
            .entry(path.into())
            .or_insert_with(|| Location::read(path, self.access.as_ref())))
    }
    pub fn directory(&mut self, path: &Path) -> Result<bool> {
        Ok(self.location(path)?.directory)
    }
    pub fn can_create_directory(&mut self, path: &Path) -> Result<bool> {
        let location = self.location(path)?;
        if location.directory {
            return Ok(true);
        }
        if location.error != Some(libc::ENOENT) {
            return Ok(false);
        }
        let mut parent = path.parent();
        while let Some(path) = parent {
            let location = self.location(path)?;
            if location.directory {
                return Ok(location.writable);
            }
            if location.error != Some(libc::ENOENT) {
                return Ok(false);
            }
            parent = path.parent();
        }
        Ok(false)
    }
    pub fn included(&mut self, path: &Path, home: &Path) -> Result<Values> {
        let input = self.observe(path)?;
        let mut result = Values::new();
        if let Some(bytes) = input.bytes {
            for line in std::str::from_utf8(&bytes)?.lines() {
                if let Some(text) = line.trim().strip_prefix("#include ")
                    && let Some(name) = value(text)
                {
                    for (key, value) in
                        self.load(&expand(&name, home, path.parent().unwrap_or(home)), home)?
                    {
                        result.entry(key).or_insert(value);
                    }
                }
            }
        }
        Ok(result)
    }
    pub fn observe(&mut self, path: &Path) -> Result<Input> {
        if let Some(input) = self.inputs.get(path) {
            return Ok(input.clone());
        }
        require(self.inputs.len() < 128, "配置依赖文件过多")?;
        let mut input = Input::read(path)?;
        if let Some(bytes) = self.preview.get(path) {
            input.bytes = bytes.clone();
        }
        self.inputs.insert(path.into(), input.clone());
        Ok(input)
    }
    pub fn exists(&self, path: &Path) -> bool {
        if let Some(bytes) = self.preview.get(path) {
            bytes.is_some()
        } else if let Some(input) = self.inputs.get(path) {
            input.bytes.is_some()
        } else {
            path.is_file()
        }
    }
    pub fn load(&mut self, path: &Path, home: &Path) -> Result<Values> {
        self.visit(path, home, &mut BTreeSet::new())
    }
    fn visit(&mut self, path: &Path, home: &Path, stack: &mut BTreeSet<PathBuf>) -> Result<Values> {
        require(
            stack.len() < 16 && stack.insert(path.into()),
            "配置 include 存在循环或层数过多",
        )?;
        let input = self.observe(path)?;
        let mut result = Values::new();
        if let Some(bytes) = input.bytes {
            for line in std::str::from_utf8(&bytes)?.lines() {
                if let Some(text) = line.trim().strip_prefix("#include ") {
                    if let Some(name) = value(text) {
                        let include = expand(&name, home, path.parent().unwrap_or(home));
                        for (key, entry) in self.visit(&include, home, stack)? {
                            result.entry(key).or_insert(entry);
                        }
                    }
                } else if let Some((key, value)) = assignment(line) {
                    result.entry(key.into()).or_insert(Entry {
                        value,
                        source: path.into(),
                    });
                }
            }
        }
        // RetroArch keeps the first definition, including entries inserted by #include.
        stack.remove(path);
        Ok(result)
    }
    pub fn revision(&self) -> String {
        let mut hash = Sha256::new();
        for input in self.inputs.values() {
            hash.update(input.revision().as_bytes());
        }
        for (path, location) in &self.locations {
            hash.update(path.as_os_str().as_encoded_bytes());
            hash.update(format!("{location:?}").as_bytes());
        }
        format!("{:x}", hash.finalize())
    }
    pub fn unchanged(&self) -> Result<()> {
        for input in self.inputs.values() {
            require(
                input.revision() == Input::read(&input.path)?.revision(),
                "配置文件已变更，请重新读取",
            )?;
        }
        for (path, location) in &self.locations {
            require(
                *location == Location::read(path, self.access.as_ref()),
                "存档目录已变更，请重新读取",
            )?;
        }
        Ok(())
    }
}

pub fn rewrite(original: &[u8], changes: &BTreeMap<String, Option<String>>) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(original)?;
    let mut lines = Vec::new();
    let mut replacements = BTreeMap::new();
    for line in text.lines() {
        if let Some((key, _)) = assignment(line)
            && let Some(next) = changes.get(key)
        {
            if let Some(next) = next
                && !replacements.contains_key(key)
            {
                require(
                    !next.contains(['"', '\n', '\r', '\0']),
                    "配置值包含无效字符",
                )?;
                let suffix = line
                    .split_once('"')
                    .and_then(|(_, value)| value.split_once('"'))
                    .map(|(_, tail)| tail)
                    .filter(|tail| tail.trim_start().starts_with('#'))
                    .unwrap_or("");
                replacements.insert(key.to_string(), format!("{key} = \"{next}\"{suffix}"));
            }
        } else {
            lines.push(line.to_string());
        }
    }
    for (key, next) in changes {
        if !replacements.contains_key(key)
            && let Some(next) = next
        {
            require(
                !next.contains(['"', '\n', '\r', '\0']),
                "配置值包含无效字符",
            )?;
            replacements.insert(key.clone(), format!("{key} = \"{next}\""));
        }
    }
    let mut result = replacements.into_values().collect::<Vec<_>>();
    result.extend(lines);
    Ok((result.join("\n") + "\n").into_bytes())
}
