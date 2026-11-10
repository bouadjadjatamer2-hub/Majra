//! ZIP extraction that runs while the archive is still downloading.
//!
//! The transfer core writes blocks out of order, so extraction cannot simply read the file
//! from the front. Instead the writer publishes a *contiguous prefix*: the largest offset
//! such that every byte before it has reached disk. The extractor walks local file headers
//! from offset zero and only starts an entry once its bytes are inside that prefix.
//!
//! Some archives store sizes in a trailing data descriptor. Those entries are left to the
//! final pass, which reads the central directory after the download completes. The final
//! pass also provides the exact entry count and total uncompressed size for the UI.
//!
//! Limits of this first slice: ZIP only (stored and deflate), no encryption, no ZIP64
//! entry sizes, and non-UTF-8 names are read lossily.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::read::DeflateDecoder;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{MajraError, Result};

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const UNKNOWN_SIZE: u64 = u32::MAX as u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionSnapshot {
    /// pending | running | paused | cancelled | done | error
    pub status: String,
    pub entries_done: u64,
    pub entries_total: Option<u64>,
    pub bytes_written: u64,
    pub bytes_total: Option<u64>,
    pub current: Option<String>,
    pub folder: String,
    pub error: Option<String>,
}

impl ExtractionSnapshot {
    pub fn new(folder: &Path) -> Self {
        Self {
            status: "pending".into(),
            entries_done: 0,
            entries_total: None,
            bytes_written: 0,
            bytes_total: None,
            current: None,
            folder: folder.display().to_string(),
            error: None,
        }
    }
}

pub enum LocalStep {
    /// One entry was written; `next` is the offset of the following local header.
    Extracted {
        next: u64,
        name: String,
        written: u64,
    },
    /// A directory or an entry that cannot be extracted safely; skip its bytes.
    Skipped { next: u64 },
    /// The local headers ran out, or they need the central directory to be understood.
    Stop,
}

struct LocalHeader {
    flags: u16,
    method: u16,
    crc: u32,
    comp: u64,
    uncomp: u64,
    name: String,
    data_start: u64,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// Reject absolute paths, parent traversal and Windows-reserved characters before touching
/// the disk. An archive must never decide where outside its own folder a file lands.
fn safe_target(root: &Path, raw: &str) -> Option<PathBuf> {
    let normalized = raw.replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        let mut cleaned = String::new();
        for character in part.chars() {
            let reserved = matches!(character, ':' | '<' | '>' | '"' | '|' | '?' | '*');
            if reserved || (character as u32) < 32 {
                cleaned.push('_');
            } else {
                cleaned.push(character);
            }
        }
        let cleaned = cleaned
            .trim_end_matches([' ', '.'])
            .to_string();
        if !cleaned.is_empty() {
            parts.push(cleaned);
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut target = root.to_path_buf();
    for part in parts {
        target.push(part);
    }
    Some(target)
}

fn read_local(file: &mut File, offset: u64) -> io::Result<Option<LocalHeader>> {
    let length = file.metadata()?.len();
    if offset.saturating_add(30) > length {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut head = [0u8; 30];
    file.read_exact(&mut head)?;
    if u32_at(&head, 0) != LOCAL_SIG {
        return Ok(None);
    }
    let flags = u16_at(&head, 6);
    let method = u16_at(&head, 8);
    let crc = u32_at(&head, 14);
    let comp = u32_at(&head, 18) as u64;
    let uncomp = u32_at(&head, 22) as u64;
    let name_length = u16_at(&head, 26) as usize;
    let extra_length = u16_at(&head, 28) as usize;

    let mut name = vec![0u8; name_length];
    file.read_exact(&mut name)?;
    let mut extra = vec![0u8; extra_length];
    file.read_exact(&mut extra)?;
    // The UTF-8 flag is the common case; legacy CP437 names are read lossily rather than
    // pulling a code page table into the core.
    let name = String::from_utf8_lossy(&name).to_string();

    Ok(Some(LocalHeader {
        flags,
        method,
        crc,
        comp,
        uncomp,
        name,
        data_start: offset + 30 + name_length as u64 + extra_length as u64,
    }))
}

struct EntryData {
    start: u64,
    comp: u64,
    method: u16,
    uncomp: u64,
    crc: u32,
}

fn extract_range(
    file: &mut File,
    data: &EntryData,
    target: &Path,
    cancel: &CancellationToken,
) -> Result<u64> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    file.seek(SeekFrom::Start(data.start))?;
    let mut reader = file.take(data.comp);
    let mut output = File::create(target)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut written = 0u64;

    if data.method == 0 {
        loop {
            if cancel.is_cancelled() {
                return Err(MajraError::Cancelled);
            }
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read])?;
            written += read as u64;
        }
    } else {
        let mut decoder = DeflateDecoder::new(reader);
        loop {
            if cancel.is_cancelled() {
                return Err(MajraError::Cancelled);
            }
            let read = decoder.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read])?;
            written += read as u64;
        }
    }

    output.flush()?;
    if data.uncomp != 0 && written != data.uncomp {
        return Err(MajraError::Message(format!(
            "entry '{}' unpacked to {written} bytes, expected {}",
            target.display(),
            data.uncomp
        )));
    }
    if data.crc != 0 && hasher.finalize() != data.crc {
        return Err(MajraError::Message(format!(
            "entry '{}' failed its CRC check",
            target.display()
        )));
    }
    Ok(written)
}

/// True when the file starts with a ZIP signature. Used to avoid reporting an unpack error
/// for a user who ticked the box on an ISO or a video.
pub fn looks_like_zip(path: &Path) -> bool {
    let mut head = [0u8; 4];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    if file.read_exact(&mut head).is_err() {
        return false;
    }
    let signature = u32_at(&head, 0);
    signature == LOCAL_SIG || signature == EOCD_SIG
}

/// The byte offset that must already be on disk before the entry at `offset` can be read.
/// `None` means the local header cannot describe the entry (data descriptor, ZIP64, or a
/// signature that is not a local header), so the final central-directory pass must handle it.
pub fn local_requirement(path: &Path, offset: u64) -> Result<Option<u64>> {
    let mut file = File::open(path)?;
    let Some(header) = read_local(&mut file, offset)? else {
        return Ok(None);
    };
    if header.flags & 8 != 0
        || header.comp == UNKNOWN_SIZE
        || header.uncomp == UNKNOWN_SIZE
    {
        return Ok(None);
    }
    Ok(Some(header.data_start.saturating_add(header.comp)))
}

/// Extract the entry whose local header starts at `offset`, if its bytes are already known.
pub fn local_step(
    path: &Path,
    offset: u64,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<LocalStep> {
    let mut file = File::open(path)?;
    let Some(header) = read_local(&mut file, offset)? else {
        return Ok(LocalStep::Stop);
    };
    let next = header.data_start.saturating_add(header.comp);

    if header.flags & 1 != 0 {
        return Err(MajraError::Message(
            "the archive is encrypted and cannot be unpacked".into(),
        ));
    }
    // A data descriptor means the real sizes live after the entry. The final pass handles it.
    if header.flags & 8 != 0
        || header.comp == UNKNOWN_SIZE
        || header.uncomp == UNKNOWN_SIZE
    {
        return Ok(LocalStep::Stop);
    }
    if header.method != 0 && header.method != 8 {
        return Ok(LocalStep::Stop);
    }

    let Some(target) = safe_target(root, &header.name) else {
        return Ok(LocalStep::Skipped { next });
    };
    if header.name.ends_with('/') {
        fs::create_dir_all(&target)?;
        return Ok(LocalStep::Skipped { next });
    }

    let written = extract_range(
        &mut file,
        &EntryData {
            start: header.data_start,
            comp: header.comp,
            method: header.method,
            uncomp: header.uncomp,
            crc: header.crc,
        },
        &target,
        cancel,
    )?;
    Ok(LocalStep::Extracted {
        next,
        name: header.name,
        written,
    })
}

struct CentralEntry {
    flags: u16,
    method: u16,
    crc: u32,
    comp: u64,
    uncomp: u64,
    local_offset: u64,
    name: String,
}

fn apply_zip64_extra(extra: &[u8], entry: &mut CentralEntry) {
    let mut cursor = 0usize;
    while cursor + 4 <= extra.len() {
        let id = u16_at(extra, cursor);
        let length = u16_at(extra, cursor + 2) as usize;
        let end = (cursor + 4 + length).min(extra.len());
        let body = &extra[cursor + 4..end];
        if id == 0x0001 {
            let mut position = 0usize;
            if entry.uncomp == UNKNOWN_SIZE && position + 8 <= body.len() {
                entry.uncomp = u64_at(body, position);
                position += 8;
            }
            if entry.comp == UNKNOWN_SIZE && position + 8 <= body.len() {
                entry.comp = u64_at(body, position);
                position += 8;
            }
            if entry.local_offset == UNKNOWN_SIZE && position + 8 <= body.len() {
                entry.local_offset = u64_at(body, position);
            }
        }
        cursor += 4 + length;
    }
}

fn find_eocd(file: &mut File) -> io::Result<Option<(u64, u64, u64)>> {
    let length = file.metadata()?.len();
    let window = length.min(66_000);
    if window < 22 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(length - window))?;
    let mut buffer = vec![0u8; window as usize];
    file.read_exact(&mut buffer)?;

    for index in (0..=buffer.len() - 22).rev() {
        if u32_at(&buffer, index) != EOCD_SIG {
            continue;
        }
        let mut entries = u16_at(&buffer, index + 10) as u64;
        let mut size = u32_at(&buffer, index + 12) as u64;
        let mut offset = u32_at(&buffer, index + 16) as u64;

        // ZIP64: the locator sits immediately before the classic end record.
        if (entries == u16::MAX as u64 || size == UNKNOWN_SIZE || offset == UNKNOWN_SIZE)
            && index >= 20
            && u32_at(&buffer, index - 20) == ZIP64_LOCATOR_SIG
        {
            let zip64_offset = u64_at(&buffer, index - 20 + 8);
            let mut head = [0u8; 56];
            file.seek(SeekFrom::Start(zip64_offset))?;
            if file.read_exact(&mut head).is_ok() && u32_at(&head, 0) == ZIP64_EOCD_SIG {
                entries = u64_at(&head, 32);
                size = u64_at(&head, 40);
                offset = u64_at(&head, 48);
            }
        }
        return Ok(Some((offset, size, entries)));
    }
    Ok(None)
}

/// Read the central directory and extract every entry that the progressive pass left behind.
/// Returns (entries_total, entries_extracted_now, bytes_written_now, bytes_total).
pub fn extract_remaining(
    path: &Path,
    root: &Path,
    skip: &HashSet<String>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(&str, u64, u64, u64),
) -> Result<(u64, u64, u64, u64)> {
    let mut directory = File::open(path)?;
    let Some((directory_offset, directory_size, entries_total)) = find_eocd(&mut directory)? else {
        return Err(MajraError::Message(
            "the archive has no ZIP end record".into(),
        ));
    };
    if directory_size == 0 {
        return Err(MajraError::Message("the archive is empty".into()));
    }

    // Two handles: the central directory cursor must not be disturbed by the seeks that
    // local headers and entry payloads need.
    let mut data = File::open(path)?;
    directory.seek(SeekFrom::Start(directory_offset))?;
    let mut extracted_now = 0u64;
    let mut bytes_now = 0u64;
    let mut bytes_total = 0u64;
    let mut buffer = vec![0u8; 46];

    for _ in 0..entries_total {
        if cancel.is_cancelled() {
            return Err(MajraError::Cancelled);
        }
        if directory.read_exact(&mut buffer).is_err() || u32_at(&buffer, 0) != CENTRAL_SIG {
            break;
        }
        let flags = u16_at(&buffer, 8);
        let method = u16_at(&buffer, 10);
        let crc = u32_at(&buffer, 16);
        let name_length = u16_at(&buffer, 28) as usize;
        let extra_length = u16_at(&buffer, 30) as usize;
        let comment_length = u16_at(&buffer, 32) as usize;

        let mut entry = CentralEntry {
            flags,
            method,
            crc,
            comp: u32_at(&buffer, 20) as u64,
            uncomp: u32_at(&buffer, 24) as u64,
            local_offset: u32_at(&buffer, 42) as u64,
            name: String::new(),
        };
        let mut name = vec![0u8; name_length];
        directory.read_exact(&mut name)?;
        let mut extra = vec![0u8; extra_length];
        directory.read_exact(&mut extra)?;
        if comment_length > 0 {
            let mut comment = vec![0u8; comment_length];
            directory.read_exact(&mut comment)?;
        }
        entry.name = String::from_utf8_lossy(&name).to_string();
        apply_zip64_extra(&extra, &mut entry);
        bytes_total = bytes_total.saturating_add(entry.uncomp);

        if skip.contains(&entry.name) {
            continue;
        }
        if entry.flags & 1 != 0 {
            return Err(MajraError::Message(
                "the archive is encrypted and cannot be unpacked".into(),
            ));
        }
        let Some(target) = safe_target(root, &entry.name) else {
            continue;
        };
        if entry.name.ends_with('/') {
            fs::create_dir_all(&target)?;
            continue;
        }
        if entry.method != 0 && entry.method != 8 {
            return Err(MajraError::Message(format!(
                "entry '{}' uses an unsupported compression method",
                entry.name
            )));
        }

        // Local headers may carry different name/extra lengths than the central copy.
        let data_start = match read_local(&mut data, entry.local_offset)? {
            Some(header) => header.data_start,
            None => {
                return Err(MajraError::Message(format!(
                    "entry '{}' points outside the archive",
                    entry.name
                )))
            }
        };
        let written = extract_range(
            &mut data,
            &EntryData {
                start: data_start,
                comp: entry.comp,
                method: entry.method,
                uncomp: entry.uncomp,
                crc: entry.crc,
            },
            &target,
            cancel,
        )?;
        extracted_now += 1;
        bytes_now = bytes_now.saturating_add(written);
        progress(&entry.name, extracted_now, bytes_now, entries_total);
    }

    Ok((entries_total, extracted_now, bytes_now, bytes_total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_traversal_and_drive_paths() {
        let root = Path::new("C:\\out");
        assert!(safe_target(root, "../evil.txt").is_none());
        assert!(safe_target(root, "folder/../../evil.txt").is_none());
        assert!(safe_target(root, "C:/Windows/evil.txt").is_some());
        assert_eq!(
            safe_target(root, "folder/readme.txt"),
            Some(PathBuf::from("C:\\out\\folder\\readme.txt"))
        );
    }

    #[test]
    fn trims_windows_reserved_tails() {
        assert_eq!(
            safe_target(Path::new("out"), "a/b. "),
            Some(PathBuf::from("out").join("a").join("b"))
        );
    }
}
