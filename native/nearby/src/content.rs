use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::protocol::{ContentIdentity, MAX_CONTENT};
use crate::{Result, require};

pub fn confined(path: &Path, roots: &[PathBuf]) -> Result<PathBuf> {
    let actual = path.canonicalize()?;
    require(
        actual.is_file()
            && roots
                .iter()
                .filter_map(|root| root.canonicalize().ok())
                .any(|root| actual.starts_with(root)),
        "File is outside the local catalog",
    )?;
    Ok(actual)
}

pub fn digest(path: &Path) -> Result<String> {
    let mut stream = File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 65536];
    loop {
        let length = stream.read(&mut block)?;
        if length == 0 {
            break;
        }
        hash.update(&block[..length]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn identity(title: String, mut stream: impl Read) -> Result<ContentIdentity> {
    let mut hash = Sha256::new();
    let mut crc = crc32fast::Hasher::new();
    let mut size = 0;
    let mut block = [0u8; 65536];
    loop {
        let count = stream.read(&mut block)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        require(size <= MAX_CONTENT, "Content limit exceeded")?;
        hash.update(&block[..count]);
        crc.update(&block[..count]);
    }
    let result = ContentIdentity {
        title,
        sha256: format!("{:x}", hash.finalize()),
        size,
        crc32: crc.finalize(),
    };
    result.validate()?;
    Ok(result)
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn allowed(name: &[u8], extensions: &[String]) -> bool {
    name.rsplit(|b| *b == b'.').next().is_some_and(|suffix| {
        extensions
            .iter()
            .any(|extension| suffix.eq_ignore_ascii_case(extension.as_bytes()))
    })
}

fn zip_identity(title: String, mut file: File, extensions: &[String]) -> Result<ContentIdentity> {
    let length = file.metadata()?.len();
    let tail_length = length.min(65557) as usize;
    file.seek(SeekFrom::End(-(tail_length as i64)))?;
    let mut tail = vec![0; tail_length];
    file.read_exact(&mut tail)?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|offset| {
            tail[*offset..].starts_with(b"PK\x05\x06")
                && *offset + 22 + usize::from(u16_at(&tail, *offset + 20)) == tail.len()
        })
        .ok_or("ZIP directory is missing")?;
    require(
        u16_at(&tail, end + 4) == 0 && u16_at(&tail, end + 6) == 0,
        "Multi-disk ZIP is unsupported",
    )?;
    let entries = u16_at(&tail, end + 10);
    require(
        entries == u16_at(&tail, end + 8) && entries < u16::MAX,
        "ZIP directory limit exceeded",
    )?;
    let central_size = u32_at(&tail, end + 12) as u64;
    let central_offset = u32_at(&tail, end + 16) as u64;
    require(
        central_offset
            .checked_add(central_size)
            .is_some_and(|n| n <= length - tail_length as u64 + end as u64),
        "ZIP directory is out of bounds",
    )?;
    file.seek(SeekFrom::Start(central_offset))?;
    let mut member = None;
    for _ in 0..entries {
        let mut header = [0; 46];
        file.read_exact(&mut header)?;
        require(
            header.starts_with(b"PK\x01\x02"),
            "Invalid ZIP directory entry",
        )?;
        let name_length = u16_at(&header, 28) as usize;
        let mut name = vec![0; name_length];
        file.read_exact(&mut name)?;
        file.seek(SeekFrom::Current(
            i64::from(u16_at(&header, 30)) + i64::from(u16_at(&header, 32)),
        ))?;
        require(
            file.stream_position()? <= central_offset + central_size,
            "ZIP entry exceeds its directory",
        )?;
        if !name.ends_with(b"/") && allowed(&name, extensions) {
            require(
                member.is_none(),
                "Archive needs one compatible content member",
            )?;
            let size = u32_at(&header, 24) as u64;
            require(
                size <= MAX_CONTENT
                    && size > 0
                    && u16_at(&header, 8) & 1 == 0
                    && u16_at(&header, 34) == 0,
                "Invalid bounded ZIP member",
            )?;
            member = Some((
                u32_at(&header, 42) as u64,
                u32_at(&header, 20) as u64,
                size,
                u32_at(&header, 16),
                u16_at(&header, 10),
                name,
            ));
        }
    }
    let (offset, compressed, size, crc, method, name) =
        member.ok_or("Archive has no compatible content")?;
    file.seek(SeekFrom::Start(offset))?;
    let mut header = [0; 30];
    file.read_exact(&mut header)?;
    require(
        header.starts_with(b"PK\x03\x04")
            && u16_at(&header, 6) & 1 == 0
            && u16_at(&header, 8) == method,
        "ZIP local header differs",
    )?;
    let mut local_name = vec![0; usize::from(u16_at(&header, 26))];
    file.read_exact(&mut local_name)?;
    require(local_name == name, "ZIP member identity differs")?;
    file.seek(SeekFrom::Current(i64::from(u16_at(&header, 28))))?;
    require(
        file.stream_position()?
            .checked_add(compressed)
            .is_some_and(|n| n <= central_offset),
        "ZIP content exceeds archive bounds",
    )?;
    let stream = file.take(compressed);
    let result = match method {
        0 => identity(title, stream)?,
        8 => identity(title, flate2::read::DeflateDecoder::new(stream))?,
        _ => return Err("Unsupported ZIP compression".into()),
    };
    require(
        result.size == size && result.crc32 == crc,
        "ZIP content checksum differs",
    )?;
    Ok(result)
}

pub fn fingerprint(
    path: &Path,
    extensions: &[String],
    roots: &[PathBuf],
    block_extract: bool,
) -> Result<ContentIdentity> {
    let actual = confined(path, roots)?;
    let title = actual
        .file_stem()
        .ok_or("Game has no title")?
        .to_string_lossy()
        .into_owned();
    let extension = actual
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let file = File::open(actual)?;
    if extension == "zip" && !block_extract {
        return zip_identity(title, file, extensions);
    }
    require(
        extensions
            .iter()
            .any(|item| item.eq_ignore_ascii_case(&extension))
            && file.metadata()?.len() <= MAX_CONTENT,
        "Unsupported or oversized content",
    )?;
    identity(title, file)
}
