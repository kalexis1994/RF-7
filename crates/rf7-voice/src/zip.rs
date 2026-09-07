//! The ZIP container, read from its own directory.
//!
//! Cartridges are published as ZIPs: a folder holding the same bank in four
//! formats, or a collection of banks. Reading one needs no allocator and no
//! dependency — a ZIP keeps a directory at the end listing every entry, and
//! from there each entry is a slice of the same bytes, stored or compressed
//! with [DEFLATE](crate::inflate).
//!
//! The directory at the end is the authority here, not the header in front
//! of each entry: when an archiver writes a file whose size it does not know
//! yet, the header's sizes are zero and the real ones are only in the
//! directory. The header is read for one thing, where the data begins.
//!
//! What is refused rather than guessed at: encryption, the ZIP64 extensions,
//! compression methods other than the two above, and any offset or length
//! that does not land inside the file. A cartridge is a few kilobytes, so
//! nothing legitimate is lost by refusing the rest.

use crate::inflate::{InflateError, inflate};

/// What a ZIP begins with, and how a caller recognises one.
pub const SIGNATURE: [u8; 4] = [b'P', b'K', 3, 4];
/// What one with nothing in it begins with, which is its end record.
const EMPTY_SIGNATURE: [u8; 4] = [b'P', b'K', 5, 6];
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// The end record, without the comment that may follow it.
const END_LENGTH: usize = 22;
/// One directory record, without its name, extra field and comment.
const CENTRAL_LENGTH: usize = 46;
/// One entry header, without its name and extra field.
const LOCAL_LENGTH: usize = 30;
/// A value standing in for one too large for its field, which is the ZIP64
/// extensions saying to look elsewhere. RF-7 does not.
const OVERFLOWED: u32 = u32::MAX;
/// Entries in the directory, past which the file is not a cartridge archive.
const MAX_ENTRIES: usize = 4096;

const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;
/// Bit zero of an entry's flags: the entry is encrypted.
const FLAG_ENCRYPTED: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZipError {
    /// No directory at the end: not a ZIP, or one cut short.
    NotAnArchive,
    /// A record reaching past the end of the file.
    Truncated,
    /// More entries than a cartridge archive would hold.
    Entries,
    /// An entry that would need a password.
    Encrypted,
    /// An entry compressed some other way.
    Method,
    /// An entry too large for its own header to describe.
    Zip64,
    /// An entry whose contents disagree with its checksum.
    Checksum,
    /// An entry larger than the buffer it was being read into.
    Output,
    /// The compressed data itself is broken.
    Data(InflateError),
}

impl core::fmt::Display for ZipError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAnArchive => formatter.write_str("this file has no ZIP directory in it"),
            Self::Truncated => formatter.write_str("the ZIP directory points outside the file"),
            Self::Entries => formatter.write_str("the ZIP holds more entries than RF-7 reads"),
            Self::Encrypted => formatter.write_str("the ZIP is encrypted"),
            Self::Method => formatter.write_str("the ZIP was compressed a way RF-7 cannot read"),
            Self::Zip64 => formatter.write_str("the ZIP uses the extensions for very large files"),
            Self::Checksum => formatter.write_str("a file inside the ZIP failed its checksum"),
            Self::Output => formatter.write_str("a file inside the ZIP is too large to read"),
            Self::Data(error) => write!(formatter, "{error}"),
        }
    }
}

impl core::error::Error for ZipError {}

/// Whether these bytes open like a ZIP. A file that does is read as one, so
/// that a broken archive reports what is wrong with it rather than being
/// mistaken for a cartridge of an unrecognised length.
pub fn is_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE) || bytes.starts_with(&EMPTY_SIGNATURE)
}

/// A ZIP, and the directory that describes it.
#[derive(Clone, Copy, Debug)]
pub struct Archive<'a> {
    bytes: &'a [u8],
    directory: &'a [u8],
    entries: usize,
}

/// One file inside the archive.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    /// The name as the archive stores it, directories and all.
    pub name: &'a [u8],
    /// What the file's contents come to when decompressed.
    pub size: usize,
    /// The checksum the archive claims for those contents.
    pub crc: u32,
    method: u16,
    compressed: usize,
    header: usize,
    bytes: &'a [u8],
}

impl<'a> Archive<'a> {
    /// Find the directory and check that it lies inside the file.
    pub fn open(bytes: &'a [u8]) -> Result<Self, ZipError> {
        let end = end_record(bytes).ok_or(ZipError::NotAnArchive)?;
        let entries = usize::from(read_u16(bytes, end + 10).ok_or(ZipError::Truncated)?);
        let size = read_u32(bytes, end + 12).ok_or(ZipError::Truncated)? as usize;
        let offset = read_u32(bytes, end + 16).ok_or(ZipError::Truncated)? as usize;
        if entries > MAX_ENTRIES {
            return Err(ZipError::Entries);
        }
        let directory = offset
            .checked_add(size)
            .and_then(|end| bytes.get(offset..end))
            .ok_or(ZipError::Truncated)?;
        Ok(Self {
            bytes,
            directory,
            entries,
        })
    }

    /// How many entries the directory says there are.
    pub fn len(&self) -> usize {
        self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries == 0
    }

    /// The entries, in the order the directory lists them. An entry the
    /// reader cannot make sense of ends the walk rather than being skipped:
    /// the directory is a chain, and a record of the wrong length loses the
    /// place in it.
    pub fn entries(&self) -> Entries<'a> {
        Entries {
            bytes: self.bytes,
            directory: self.directory,
            at: 0,
            left: self.entries,
        }
    }
}

/// A walk over the directory.
#[derive(Clone, Debug)]
pub struct Entries<'a> {
    bytes: &'a [u8],
    directory: &'a [u8],
    at: usize,
    left: usize,
}

impl<'a> Iterator for Entries<'a> {
    type Item = Result<Entry<'a>, ZipError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.left == 0 {
            return None;
        }
        let record = self.directory.get(self.at..)?;
        if read_u32(record, 0)? != CENTRAL_SIGNATURE {
            return None;
        }
        let flags = read_u16(record, 8)?;
        let method = read_u16(record, 10)?;
        let crc = read_u32(record, 16)?;
        let compressed = read_u32(record, 20)?;
        let size = read_u32(record, 24)?;
        let name_length = usize::from(read_u16(record, 28)?);
        let extra_length = usize::from(read_u16(record, 30)?);
        let comment_length = usize::from(read_u16(record, 32)?);
        let header = read_u32(record, 42)?;
        let name = record.get(CENTRAL_LENGTH..CENTRAL_LENGTH + name_length)?;
        self.at += CENTRAL_LENGTH + name_length + extra_length + comment_length;
        self.left -= 1;

        if flags & FLAG_ENCRYPTED != 0 {
            return Some(Err(ZipError::Encrypted));
        }
        if compressed == OVERFLOWED || size == OVERFLOWED || header == OVERFLOWED {
            return Some(Err(ZipError::Zip64));
        }
        Some(Ok(Entry {
            name,
            size: size as usize,
            crc,
            method,
            compressed: compressed as usize,
            header: header as usize,
            bytes: self.bytes,
        }))
    }
}

impl Entry<'_> {
    /// Whether the entry is a folder rather than a file.
    pub fn is_directory(&self) -> bool {
        self.name.last() == Some(&b'/') || self.name.last() == Some(&b'\\')
    }

    /// Whether the name ends with this suffix, letter case aside.
    pub fn has_suffix(&self, suffix: &[u8]) -> bool {
        let Some(tail) = self.name.len().checked_sub(suffix.len()) else {
            return false;
        };
        self.name[tail..]
            .iter()
            .zip(suffix)
            .all(|(byte, want)| byte.eq_ignore_ascii_case(want))
    }

    /// Read the file into `output`, returning how many bytes it holds.
    ///
    /// The checksum is verified before the bytes are handed back, so a
    /// caller never sees a partially correct file.
    pub fn extract(&self, output: &mut [u8]) -> Result<usize, ZipError> {
        let start = self
            .header
            .checked_add(LOCAL_LENGTH)
            .ok_or(ZipError::Truncated)?;
        let local = self
            .bytes
            .get(self.header..start)
            .ok_or(ZipError::Truncated)?;
        if read_u32(local, 0) != Some(LOCAL_SIGNATURE) {
            return Err(ZipError::Truncated);
        }
        // The name and extra field are written twice, and the two copies may
        // differ in length; where the data starts follows this one.
        let name_length = usize::from(read_u16(local, 26).ok_or(ZipError::Truncated)?);
        let extra_length = usize::from(read_u16(local, 28).ok_or(ZipError::Truncated)?);
        let data = start
            .checked_add(name_length + extra_length)
            .ok_or(ZipError::Truncated)?;
        let compressed = data
            .checked_add(self.compressed)
            .and_then(|end| self.bytes.get(data..end))
            .ok_or(ZipError::Truncated)?;
        let room = output.get_mut(..self.size).ok_or(ZipError::Output)?;
        let written = match self.method {
            METHOD_STORED => {
                if compressed.len() != self.size {
                    return Err(ZipError::Truncated);
                }
                room.copy_from_slice(compressed);
                self.size
            }
            METHOD_DEFLATE => inflate(compressed, room).map_err(ZipError::Data)?,
            _ => return Err(ZipError::Method),
        };
        if written != self.size {
            return Err(ZipError::Truncated);
        }
        if crc32(&output[..written]) != self.crc {
            return Err(ZipError::Checksum);
        }
        Ok(written)
    }
}

/// Where the end record starts, searched backwards: it is last in the file,
/// behind a comment of up to sixty-five kilobytes.
fn end_record(bytes: &[u8]) -> Option<usize> {
    let last = bytes.len().checked_sub(END_LENGTH)?;
    let first = last.saturating_sub(u16::MAX as usize);
    for at in (first..=last).rev() {
        if read_u32(bytes, at) != Some(END_SIGNATURE) {
            continue;
        }
        // The comment's length must account for the rest of the file, which
        // is what tells a real record from the same four bytes inside data.
        let comment = usize::from(read_u16(bytes, at + 20)?);
        if at + END_LENGTH + comment == bytes.len() {
            return Some(at);
        }
    }
    None
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// CRC-32, as ZIP and everything else of its generation compute it: the
/// reflected polynomial `0xEDB88320`, four bits at a time off a table small
/// enough to read.
pub fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 16] = [
        0x0000_0000,
        0x1db7_1064,
        0x3b6e_20c8,
        0x26d9_30ac,
        0x76dc_4190,
        0x6b6b_51f4,
        0x4db2_6158,
        0x5005_713c,
        0xedb8_8320,
        0xf00f_9344,
        0xd6d6_a3e8,
        0xcb61_b38c,
        0x9b64_c2b0,
        0x86d3_d2d4,
        0xa00a_e278,
        0xbdbd_f21c,
    ];
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        crc = (crc >> 4) ^ TABLE[(crc & 0xf) as usize];
        crc = (crc >> 4) ^ TABLE[(crc & 0xf) as usize];
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_is_the_one_every_archiver_computes() {
        // The check value RFC 1952 prints for this string, and the empty
        // case, and a whole bank of zeros as a ZIP would record it.
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(&[0; 4096]), 0xc71c_0011);
    }

    #[test]
    fn nothing_short_of_an_end_record_opens() {
        assert!(Archive::open(&[]).is_err());
        assert!(Archive::open(&[0; 64]).is_err());
        assert!(!is_archive(&[]));
        assert!(is_archive(&[b'P', b'K', 3, 4, 0]));
        assert!(is_archive(&[b'P', b'K', 5, 6, 0]), "one with nothing in it");
    }
}
