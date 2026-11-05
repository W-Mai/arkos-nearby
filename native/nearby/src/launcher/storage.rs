use crate::{Result, require};
use nearby_menu::details::{Document, Fact, FileStatus, Row};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
struct Space {
    total: u64,
    available: u64,
    used: u64,
}

impl Space {
    fn blocks<T: Into<u64>>(size: u64, total: T, free: T, available: T) -> Result<Self> {
        require(size > 0, "Invalid filesystem block size")?;
        let total = total
            .into()
            .checked_mul(size)
            .ok_or("Filesystem capacity overflow")?;
        let free = free
            .into()
            .checked_mul(size)
            .ok_or("Filesystem capacity overflow")?;
        let available = available
            .into()
            .checked_mul(size)
            .ok_or("Filesystem capacity overflow")?;
        require(
            free <= total && available <= free,
            "Invalid filesystem capacity",
        )?;
        Ok(Self {
            total,
            available,
            used: total - free,
        })
    }

    fn read(path: &Path) -> Result<Self> {
        let path = CString::new(path.as_os_str().as_encoded_bytes())?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let stat = unsafe { stat.assume_init() };
        Self::blocks(
            if stat.f_frsize == 0 {
                stat.f_bsize
            } else {
                stat.f_frsize
            },
            stat.f_blocks,
            stat.f_bfree,
            stat.f_bavail,
        )
    }
}

struct Mount {
    path: PathBuf,
    source: String,
    filesystem: String,
    readonly: bool,
}

fn unescape(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let (fields, filesystem) = line.split_once(" - ")?;
            let fields: Vec<_> = fields.split_whitespace().collect();
            let mut filesystem = filesystem.split_whitespace();
            Some(Mount {
                path: unescape(fields.get(4)?).into(),
                readonly: fields.get(5)?.split(',').any(|option| option == "ro"),
                filesystem: filesystem.next()?.into(),
                source: unescape(filesystem.next()?),
            })
        })
        .collect()
}

fn bytes(value: u64) -> String {
    let (unit, scale) = if value >= 1024 * 1024 * 1024 {
        ("GiB", 1024_f64.powi(3))
    } else {
        ("MiB", 1024_f64.powi(2))
    };
    format!("{:.1} {unit}", value as f64 / scale)
}

pub(super) fn read() -> Document {
    let roots = [
        ("系统分区", Path::new("/")),
        ("游戏目录", Path::new("/roms")),
        ("扩展游戏目录", Path::new("/roms2")),
    ];
    let roots: Vec<_> = roots
        .into_iter()
        .filter(|(_, path)| path.try_exists().unwrap_or(true))
        .collect();
    collect(
        &roots,
        &mounts(&fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()),
        |path| Ok((fs::metadata(path)?.dev(), Space::read(path)?)),
    )
}

fn collect(
    roots: &[(&str, &Path)],
    mounts: &[Mount],
    mut probe: impl FnMut(&Path) -> Result<(u64, Space)>,
) -> Document {
    let mut document = Document {
        key: "system-storage".into(),
        title: "可用 / 总容量".into(),
        ..Document::default()
    };
    let mut devices = BTreeMap::new();
    for (name, path) in roots {
        let (device, space) = match probe(path) {
            Ok(value) => value,
            Err(error) => {
                document.storage.push(Row {
                    name: (*name).into(),
                    summary: "容量读取失败".into(),
                    path: path.display().to_string(),
                    facts: vec![
                        Fact::new("目录", path.display().to_string()),
                        Fact::new("状态", error.to_string()),
                    ],
                    ..Row::default()
                });
                continue;
            }
        };
        if let Some(index) = devices.get(&device).copied() {
            let row: &mut Row = &mut document.storage[index];
            row.facts
                .push(Fact::new("目录", path.display().to_string()));
            continue;
        }
        devices.insert(device, document.storage.len());
        let mount = mounts
            .iter()
            .filter(|mount| path.starts_with(&mount.path))
            .max_by_key(|mount| mount.path.components().count());
        let mut facts = vec![
            Fact::new("总容量", bytes(space.total)),
            Fact::new("已用", bytes(space.used)),
            Fact::new("可用", bytes(space.available)),
        ];
        if let Some(mount) = mount {
            facts.extend([
                Fact::new("挂载点", mount.path.display().to_string()),
                Fact::new("文件系统", &mount.filesystem),
                Fact::new("设备", &mount.source),
                Fact::new("状态", if mount.readonly { "只读" } else { "读写" }),
            ]);
        }
        facts.push(Fact::new("目录", path.display().to_string()));
        document.storage.push(Row {
            name: (*name).into(),
            summary: format!("{} / {}", bytes(space.available), bytes(space.total)),
            path: mount.map_or_else(
                || path.display().to_string(),
                |mount| mount.path.display().to_string(),
            ),
            status: FileStatus::Found,
            facts,
            ..Row::default()
        });
    }
    document.platform = format!("{} 个分区", document.storage.len());
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_distinguishes_reserved_blocks_and_rejects_invalid_arithmetic() {
        let space = Space::blocks(1024, 100_u64, 40, 30).unwrap();
        assert_eq!(
            (space.total, space.used, space.available),
            (102400, 61440, 30720)
        );
        for values in [
            (0, 1, 1, 1),
            (2, u64::MAX, 0, 0),
            (1, 10, 11, 0),
            (1, 10, 5, 6),
        ] {
            assert!(Space::blocks(values.0, values.1, values.2, values.3).is_err());
        }
    }

    #[test]
    fn roots_share_one_filesystem_and_mount_matching_uses_path_components() {
        let mounts = mounts(
            "1 0 0:1 / / rw - ext4 /dev/root rw\n2 1 0:2 / /roms ro - vfat /dev/mmcblk0p3 ro\n3 1 0:3 / /roms\\040archive rw - ext4 /dev/disk\\134040 rw\n",
        );
        let document = collect(
            &[
                ("System", Path::new("/")),
                ("Games", Path::new("/roms")),
                ("Second", Path::new("/roms2")),
            ],
            &mounts,
            |path| {
                Ok((
                    if path == Path::new("/roms") { 2 } else { 1 },
                    Space::blocks(1, 100_u64, 40, 30)?,
                ))
            },
        );
        assert_eq!(document.storage.len(), 2);
        assert_eq!(document.storage[0].facts.last().unwrap().value, "/roms2");
        assert_eq!(document.storage[1].path, "/roms");
        assert!(
            document.storage[1]
                .facts
                .iter()
                .any(|fact| fact.value == "只读")
        );
        assert_eq!(mounts[2].path, Path::new("/roms archive"));
        assert_eq!(mounts[2].source, "/dev/disk\\040");
    }
}
