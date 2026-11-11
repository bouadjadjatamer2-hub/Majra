//! Archive extraction that runs while the archive is still downloading.
//!
//! The transfer core writes blocks out of order, so extraction cannot simply read the file
//! from the front. Instead the writer publishes a *contiguous prefix*: the largest offset
//! such that every byte before it has reached disk. The extractor walks local file headers
//! from offset zero and only starts an entry once its bytes are inside that prefix.
//!
//! ZIP entries may store their sizes in a trailing data descriptor. Those entries are left
//! to the final pass, which reads the central directory after the download completes. The
//! final pass also provides the exact entry count and total uncompressed size for the UI.
//!
//! TAR is sequential by design, so its entries unpack progressively with no central index.
//! A `.tar.gz` (or `.tgz`) has to be decompressed from the start, so it is unpacked in one
//! streaming final pass; the download still runs at full speed beside it.
//!
//! Limits of this slice: ZIP (stored and deflate) and TAR/TAR.GZ. No encryption, no RAR,
//! no ZIP64 entry sizes, and non-UTF-8 names are read lossily.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::read::{DeflateDecoder, GzDecoder};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{MajraError, Result};

/// What kind of archive the bytes on disk look like. Detection only needs the first
/// 512 bytes, which is why it is safe to call on a partially downloaded file once the
/// contiguous prefix has reached that far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
    Other,
}

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
        let cleaned = cleaned.trim_end_matches([' ', '.']).to_string();
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

/// A TAR block is a fixed 512-byte header followed by padded data. `ustar` is present in
/// every format we accept; requiring it keeps a random 512-byte chunk from looking like TAR.
fn is_tar_header(block: &[u8]) -> bool {
    if block.len() < 512 || &block[257..262] != b"ustar" {
        return false;
    }
    let Some(stored) = tar_number(&block[148..156]) else {
        return false;
    };
    let mut sum = 0u64;
    for (index, byte) in block.iter().enumerate() {
        sum += if (148..156).contains(&index) { b' ' as u64 } else { *byte as u64 };
    }
    sum == stored
}

/// TAR numbers are octal text, except for base-256 values that GNU uses for very large sizes.
fn tar_number(bytes: &[u8]) -> Option<u64> {
    if bytes.first().is_some_and(|first| first & 0x80 != 0) {
        let mut value = (bytes[0] & 0x7f) as u64;
        for byte in &bytes[1..] {
            value = value.checked_mul(256)?.checked_add(*byte as u64)?;
        }
        return Some(value);
    }
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[..end]);
    let text = text.trim();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(text, 8).ok()
}

fn tar_text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim_end().to_string()
}

fn tar_name(block: &[u8]) -> String {
    let name = tar_text(&block[0..100]);
    let prefix = tar_text(&block[345..500]);
    if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    }
}

fn padded(size: u64) -> u64 {
    size.saturating_add(511) / 512 * 512
}

/// True when the first bytes are a ZIP, TAR or gzip stream. Used to avoid reporting an
/// unpack error for a user who ticked the box on an ISO or a video.
pub fn detect(path: &Path) -> ArchiveKind {
    let Ok(mut file) = File::open(path) else {
        return ArchiveKind::Other;
    };
    let mut head = [0u8; 512];
    let read = file.read(&mut head).unwrap_or(0);
    if read >= 4 {
        let signature = u32_at(&head, 0);
        if signature == LOCAL_SIG || signature == EOCD_SIG {
            return ArchiveKind::Zip;
        }
    }
    if read >= 2 && head[0] == 0x1f && head[1] == 0x8b {
        return ArchiveKind::TarGz;
    }
    if read >= 512 && is_tar_header(&head) {
        return ArchiveKind::Tar;
    }
    ArchiveKind::Other
}

pub fn is_archive(path: &Path) -> bool {
    detect(path) != ArchiveKind::Other
}

/// The byte offset that must already be on disk before the entry at `offset` can be read.
/// `None` means the header cannot describe the entry yet (data descriptor, ZIP64, a TAR
/// long-name record, or a signature that is not a local header), so the final pass handles it.
pub fn local_requirement(path: &Path, offset: u64) -> Result<Option<u64>> {
    match detect(path) {
        ArchiveKind::Zip => zip_local_requirement(path, offset),
        ArchiveKind::Tar => tar_local_requirement(path, offset),
        _ => Ok(None),
    }
}

/// Extract the entry whose header starts at `offset`, if its bytes are already known.
pub fn local_step(
    path: &Path,
    offset: u64,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<LocalStep> {
    match detect(path) {
        ArchiveKind::Zip => zip_local_step(path, offset, root, cancel),
        ArchiveKind::Tar => tar_local_step(path, offset, root, cancel),
        _ => Ok(LocalStep::Stop),
    }
}

fn zip_local_requirement(path: &Path, offset: u64) -> Result<Option<u64>> {
    let mut file = File::open(path)?;
    let Some(header) = read_local(&mut file, offset)? else {
        return Ok(None);
    };
    if header.flags & 8 != 0 || header.comp == UNKNOWN_SIZE || header.uncomp == UNKNOWN_SIZE {
        return Ok(None);
    }
    Ok(Some(header.data_start.saturating_add(header.comp)))
}

fn zip_local_step(
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
    if header.flags & 8 != 0 || header.comp == UNKNOWN_SIZE || header.uncomp == UNKNOWN_SIZE {
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

struct TarHeader {
    name: String,
    size: u64,
    kind: u8,
    data_start: u64,
    next: u64,
}

fn read_tar_header(file: &mut File, offset: u64) -> io::Result<Option<TarHeader>> {
    let length = file.metadata()?.len();
    if offset.saturating_add(512) > length {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut block = [0u8; 512];
    file.read_exact(&mut block)?;
    if block.iter().all(|byte| *byte == 0) || !is_tar_header(&block) {
        return Ok(None);
    }
    let size = tar_number(&block[124..136]).unwrap_or(0);
    Ok(Some(TarHeader {
        name: tar_name(&block),
        size,
        kind: block[156],
        data_start: offset + 512,
        next: offset + 512 + padded(size),
    }))
}

fn tar_local_requirement(path: &Path, offset: u64) -> Result<Option<u64>> {
    let mut file = File::open(path)?;
    let Some(header) = read_tar_header(&mut file, offset)? else {
        return Ok(None);
    };
    // GNU long names sit in their own record; the final sequential pass stitches them.
    if header.kind == b'L' || header.kind == b'K' {
        return Ok(None);
    }
    Ok(Some(header.next))
}

fn tar_local_step(
    path: &Path,
    offset: u64,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<LocalStep> {
    let mut file = File::open(path)?;
    let Some(header) = read_tar_header(&mut file, offset)? else {
        return Ok(LocalStep::Stop);
    };
    if header.kind == b'L' || header.kind == b'K' {
        return Ok(LocalStep::Stop);
    }
    let Some(target) = safe_target(root, &header.name) else {
        return Ok(LocalStep::Skipped { next: header.next });
    };
    // '5' is a directory; '0' and NUL are regular files. Links, devices and sparse files
    // are skipped rather than followed, because a link may point outside the folder.
    if header.kind == b'5' || header.name.ends_with('/') {
        fs::create_dir_all(&target)?;
        return Ok(LocalStep::Skipped { next: header.next });
    }
    if header.kind != b'0' && header.kind != 0 {
        return Ok(LocalStep::Skipped { next: header.next });
    }
    let written = copy_range(&mut file, header.data_start, header.size, &target, cancel)?;
    Ok(LocalStep::Extracted {
        next: header.next,
        name: header.name,
        written,
    })
}

/// Copy exactly `length` bytes from `start` into `target`. Shared by TAR's stored entries.
fn copy_range(
    file: &mut File,
    start: u64,
    length: u64,
    target: &Path,
    cancel: &CancellationToken,
) -> Result<u64> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    file.seek(SeekFrom::Start(start))?;
    let mut reader = file.take(length);
    let mut output = File::create(target)?;
    let mut buffer = vec![0u8; 128 * 1024];
    let mut written = 0u64;
    loop {
        if cancel.is_cancelled() {
            return Err(MajraError::Cancelled);
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        written += read as u64;
    }
    output.flush()?;
    Ok(written)
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

/// Extract everything the progressive pass left behind. The entry total is unknown for a
/// gzip stream until it ends, which is why the progress callback receives an `Option`.
/// Returns (entries_total, entries_extracted_now, bytes_written_now, bytes_total).
pub fn extract_remaining(
    path: &Path,
    root: &Path,
    skip: &HashSet<String>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(&str, u64, u64, Option<u64>),
) -> Result<(u64, u64, u64, u64)> {
    match detect(path) {
        ArchiveKind::Zip => extract_remaining_zip(path, root, skip, cancel, progress),
        ArchiveKind::Tar => {
            let mut file = File::open(path)?;
            extract_tar_stream(&mut file, root, skip, cancel, progress)
        }
        ArchiveKind::TarGz => extract_remaining_gz(path, root, skip, cancel, progress),
        ArchiveKind::Other => Err(MajraError::Message(
            "the file is not a ZIP or TAR archive".into(),
        )),
    }
}

fn extract_remaining_zip(
    path: &Path,
    root: &Path,
    skip: &HashSet<String>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(&str, u64, u64, Option<u64>),
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
        progress(&entry.name, extracted_now, bytes_now, Some(entries_total));
    }

    Ok((entries_total, extracted_now, bytes_now, bytes_total))
}

/// Read one 512-byte block. `false` means a clean end of stream or the zero block that
/// terminates a TAR archive.
fn read_block<R: Read>(reader: &mut R, block: &mut [u8; 512]) -> Result<bool> {
    let mut filled = 0usize;
    while filled < 512 {
        let read = reader.read(&mut block[filled..])?;
        if read == 0 {
            return Ok(false);
        }
        filled += read;
    }
    Ok(!block.iter().all(|byte| *byte == 0))
}

fn discard<R: Read>(reader: &mut R, mut remaining: u64, cancel: &CancellationToken) -> Result<()> {
    let mut buffer = vec![0u8; 64 * 1024];
    while remaining > 0 {
        if cancel.is_cancelled() {
            return Err(MajraError::Cancelled);
        }
        let want = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..want])?;
        if read == 0 {
            return Err(MajraError::Message(
                "the archive ended in the middle of an entry".into(),
            ));
        }
        remaining -= read as u64;
    }
    Ok(())
}

/// Walk a TAR stream from the first header to the end record. A plain `.tar` passes its
/// `File` here; a `.tar.gz` passes its decompressor, so both share one sequential parser.
fn extract_tar_stream<R: Read>(
    reader: &mut R,
    root: &Path,
    skip: &HashSet<String>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(&str, u64, u64, Option<u64>),
) -> Result<(u64, u64, u64, u64)> {
    let mut entries = 0u64;
    let mut done = 0u64;
    let mut bytes = 0u64;
    let mut total = 0u64;
    let mut long_name: Option<String> = None;
    let mut block = [0u8; 512];

    while read_block(reader, &mut block)? {
        if cancel.is_cancelled() {
            return Err(MajraError::Cancelled);
        }
        if !is_tar_header(&block) {
            return Err(MajraError::Message(
                "the TAR stream has a damaged header".into(),
            ));
        }
        let size = tar_number(&block[124..136]).unwrap_or(0);
        let kind = block[156];
        let header_name = tar_name(&block);
        let padded_size = padded(size);

        if kind == b'L' {
            // GNU long name: the data of this record is the name of the next entry.
            let mut name = vec![0u8; size.min(64 * 1024) as usize];
            reader.read_exact(&mut name)?;
            long_name = Some(tar_text(&name));
            discard(reader, padded_size - name.len() as u64, cancel)?;
            continue;
        }
        if kind == b'K' {
            discard(reader, padded_size, cancel)?;
            continue;
        }

        entries += 1;
        total = total.saturating_add(size);
        let name = long_name.take().unwrap_or(header_name);

        if kind == b'5' || name.ends_with('/') {
            if let Some(target) = safe_target(root, &name) {
                fs::create_dir_all(target)?;
            }
            discard(reader, padded_size, cancel)?;
            continue;
        }
        if kind != b'0' && kind != 0 {
            discard(reader, padded_size, cancel)?;
            continue;
        }
        if skip.contains(&name) {
            discard(reader, padded_size, cancel)?;
            continue;
        }
        let Some(target) = safe_target(root, &name) else {
            discard(reader, padded_size, cancel)?;
            continue;
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        let mut remaining = size;
        let mut buffer = vec![0u8; 128 * 1024];
        let mut written = 0u64;
        while remaining > 0 {
            if cancel.is_cancelled() {
                return Err(MajraError::Cancelled);
            }
            let want = remaining.min(buffer.len() as u64) as usize;
            let read = reader.read(&mut buffer[..want])?;
            if read == 0 {
                return Err(MajraError::Message(format!(
                    "entry '{name}' ended early"
                )));
            }
            output.write_all(&buffer[..read])?;
            written += read as u64;
            remaining -= read as u64;
        }
        output.flush()?;
        discard(reader, padded_size - size, cancel)?;
        done += 1;
        bytes = bytes.saturating_add(written);
        progress(&name, done, bytes, None);
    }

    Ok((entries, done, bytes, total))
}

/// The name a single-member `.gz` (not a TAR) should take on disk.
fn gz_output_name(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let lower = name.to_ascii_lowercase();
    for suffix in [".tar.gz", ".tgz", ".gz"] {
        if lower.ends_with(suffix) {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    format!("{name}.out")
}

/// Decompress a gzip file from the start. A TAR inside is unpacked entry by entry; a plain
/// gzip member is written as the single file it holds.
fn extract_remaining_gz(
    path: &Path,
    root: &Path,
    skip: &HashSet<String>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(&str, u64, u64, Option<u64>),
) -> Result<(u64, u64, u64, u64)> {
    let file = File::open(path)?;
    let mut decoder = GzDecoder::new(file);

    // The first block decides between "TAR inside" and "one compressed file".
    let mut first = vec![0u8; 512];
    let mut filled = 0usize;
    while filled < first.len() {
        let read = decoder.read(&mut first[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    if filled >= 512 && is_tar_header(&first[..512]) {
        let mut reader = io::Cursor::new(first[..filled].to_vec()).chain(decoder);
        return extract_tar_stream(&mut reader, root, skip, cancel, progress);
    }

    let name = gz_output_name(path);
    let Some(target) = safe_target(root, &name) else {
        return Err(MajraError::Message(
            "the gzip file has no usable name".into(),
        ));
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = File::create(&target)?;
    output.write_all(&first[..filled])?;
    let mut bytes = filled as u64;
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        if cancel.is_cancelled() {
            return Err(MajraError::Cancelled);
        }
        let read = decoder.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        bytes += read as u64;
    }
    output.flush()?;
    progress(&name, 1, bytes, Some(1));
    Ok((1, 1, bytes, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one ustar header plus its padded data, enough for the parser under test.
    fn tar_entry(name: &str, data: &[u8], kind: u8) -> Vec<u8> {
        let mut block = vec![0u8; 512];
        block[..name.len()].copy_from_slice(name.as_bytes());
        block[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        block[156] = kind;
        block[257..262].copy_from_slice(b"ustar");
        block[262] = 0;
        block[263..265].copy_from_slice(b"00");
        block[148..156].fill(b' ');
        let sum: u64 = block.iter().map(|byte| *byte as u64).sum();
        block[148..156].copy_from_slice(format!("{:06o}\0 ", sum).as_bytes());
        block.extend_from_slice(data);
        block.resize(512 + padded(data.len() as u64) as usize, 0);
        block
    }

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("majra-extract-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

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

    #[test]
    fn a_tar_stream_is_recognized_and_walked_in_order() {
        let root = scratch("tar");
        let mut archive = tar_entry("hello.txt", b"hello majra", b'0');
        archive.extend_from_slice(&tar_entry("nested/", b"", b'5'));
        archive.extend_from_slice(&tar_entry("nested/second.bin", &[7u8; 700], b'0'));
        archive.extend_from_slice(&[0u8; 1024]);

        let mut reader = io::Cursor::new(archive);
        let cancel = CancellationToken::new();
        let mut seen = Vec::new();
        let (entries, done, bytes, total) = extract_tar_stream(
            &mut reader,
            &root,
            &HashSet::new(),
            &cancel,
            &mut |name, _done, _bytes, _total| seen.push(name.to_string()),
        )
        .expect("extract");

        assert_eq!(entries, 3);
        assert_eq!(done, 2);
        assert_eq!(bytes, 11 + 700);
        assert_eq!(total, 711);
        assert_eq!(seen, vec!["hello.txt", "nested/second.bin"]);
        assert_eq!(fs::read(root.join("hello.txt")).unwrap(), b"hello majra");
        assert_eq!(fs::read(root.join("nested/second.bin")).unwrap().len(), 700);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tar_entry_may_not_escape_its_folder() {
        let root = scratch("tar-escape");
        let mut archive = tar_entry("../evil.txt", b"no", b'0');
        archive.extend_from_slice(&[0u8; 1024]);

        let mut reader = io::Cursor::new(archive);
        let cancel = CancellationToken::new();
        let (entries, done, _bytes, _total) =
            extract_tar_stream(&mut reader, &root, &HashSet::new(), &cancel, &mut |_, _, _, _| {})
                .expect("extract");

        assert_eq!(entries, 1);
        assert_eq!(done, 0);
        assert!(!root.parent().unwrap().join("evil.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tar_gz_is_decompressed_and_unpacked() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let root = scratch("tgz");
        let mut archive = tar_entry("docs/readme.md", b"# majra", b'0');
        archive.extend_from_slice(&tar_entry("docs/data.bin", &[3u8; 600], b'0'));
        archive.extend_from_slice(&[0u8; 1024]);

        let path = root.join("bundle.tar.gz");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&archive).unwrap();
        fs::write(&path, encoder.finish().unwrap()).unwrap();
        assert_eq!(detect(&path), ArchiveKind::TarGz);

        let cancel = CancellationToken::new();
        let (entries, done, bytes, total) = extract_remaining(
            &path,
            &root.join("out"),
            &HashSet::new(),
            &cancel,
            &mut |_, _, _, _| {},
        )
        .expect("extract");

        assert_eq!(entries, 2);
        assert_eq!(done, 2);
        assert_eq!(bytes, 7 + 600);
        assert_eq!(total, 607);
        assert_eq!(fs::read(root.join("out/docs/readme.md")).unwrap(), b"# majra");
        assert_eq!(fs::read(root.join("out/docs/data.bin")).unwrap().len(), 600);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_plain_gzip_becomes_one_file_without_its_suffix() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let root = scratch("gz");
        let path = root.join("notes.txt.gz");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(b"a plain gzip member").unwrap();
        fs::write(&path, encoder.finish().unwrap()).unwrap();

        let cancel = CancellationToken::new();
        let (entries, done, bytes, _total) = extract_remaining(
            &path,
            &root.join("out"),
            &HashSet::new(),
            &cancel,
            &mut |_, _, _, _| {},
        )
        .expect("extract");

        assert_eq!(entries, 1);
        assert_eq!(done, 1);
        assert_eq!(bytes, 19);
        assert_eq!(
            fs::read(root.join("out/notes.txt")).unwrap(),
            b"a plain gzip member"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn detection_needs_real_headers_not_zeros() {
        let root = scratch("detect");
        let path = root.join("sample.tar");
        fs::write(&path, tar_entry("a.txt", b"a", b'0')).unwrap();
        assert_eq!(detect(&path), ArchiveKind::Tar);

        let zeros = root.join("zeros.bin");
        fs::write(&zeros, vec![0u8; 1024]).unwrap();
        assert_eq!(detect(&zeros), ArchiveKind::Other);

        let _ = fs::remove_dir_all(&root);
    }
}
