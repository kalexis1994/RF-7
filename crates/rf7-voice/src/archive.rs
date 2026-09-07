//! A cartridge file, whatever container it came in.
//!
//! The collections people download are ZIPs, and what is inside one is not
//! only a cartridge: the same bank is published as a System Exclusive dump,
//! as a MIDI file, and in the formats two old editors used, all four in the
//! same folder. Every one of them carries the same voices, so a reader that
//! went by content alone would find each bank three times over.
//!
//! So this picks by name, in one order: the dumps if there are any, then the
//! chip images, then anything at all. Whatever it picks it reads in the order
//! the names sort in, which is what puts a cartridge's bank A in front of its
//! bank B, and it skips a second copy of a file it has already taken. An
//! entry that turns out not to be a cartridge is passed over rather than
//! failing the archive: a folder with a `readme` in it still installs.

use crate::{
    library::{Library, decode_library},
    sysex::SysexError,
    zip::{Archive, Entry, ZipError, is_archive},
};

/// The extensions a cartridge is published under, in the order they are
/// preferred. The first one an archive holds any of is the only one read.
const SUFFIXES: [&[u8]; 3] = [b".syx", b".bin", b".dx7"];

/// How much room the caller has to give [`decode_cartridge`], and so the
/// largest single file it will read out of an archive. A hundred and twenty
/// eight banks in one file is far past what RF-7 can play, so nothing
/// anybody publishes is turned away by it — and a file that does exceed it
/// is passed over rather than read in half, which is also what stops an
/// archive from expanding without limit.
pub const SCRATCH_BYTES: usize = 512 * 1024;

/// How much one archive may decompress to in total, over every file read out
/// of it. This is the other half of the same bound: an archive of a thousand
/// enormous entries costs a fixed amount of work rather than its own size.
const MAX_UNPACKED_BYTES: usize = 4 * 1024 * 1024;

/// Files one archive may contribute. Every cartridge file holds at least one
/// voice, so this is past the point where RF-7 has all it can play.
const MAX_SOURCES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CartridgeError {
    /// The bytes are not a shape RF-7 reads.
    Sysex(SysexError),
    /// The archive itself could not be read.
    Archive(ZipError),
    /// The archive was read, and held no cartridge.
    Empty,
}

impl core::fmt::Display for CartridgeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Sysex(error) => write!(formatter, "{error}"),
            Self::Archive(error) => write!(formatter, "{error}"),
            Self::Empty => formatter.write_str("this archive holds no cartridge RF-7 can read"),
        }
    }
}

impl core::error::Error for CartridgeError {}

/// Read a cartridge file: a bulk dump, a single voice, a chip image, or a ZIP
/// holding any number of those.
///
/// `scratch` is where one entry of an archive is decompressed, and wants
/// [`SCRATCH_BYTES`]; a file larger than it is passed over. Bytes that are
/// not an archive never touch it.
pub fn decode_cartridge(bytes: &[u8], scratch: &mut [u8]) -> Result<Library, CartridgeError> {
    if !is_archive(bytes) {
        return decode_library(bytes).map_err(CartridgeError::Sysex);
    }
    match decode_archive(bytes, scratch) {
        Ok(library) => Ok(library),
        // A cartridge that happens to open with the same four bytes a ZIP
        // does is still a cartridge, so the plain reading is tried before
        // the archive's complaint is reported.
        Err(error) => decode_library(bytes).map_err(|_| error),
    }
}

fn decode_archive(bytes: &[u8], scratch: &mut [u8]) -> Result<Library, CartridgeError> {
    let archive = Archive::open(bytes).map_err(CartridgeError::Archive)?;
    let suffix = preferred_suffix(&archive);
    let mut library = Library::empty();
    let mut seen = [(0_u32, 0_usize); MAX_SOURCES];
    let mut sources = 0;
    let mut decoded = 0;
    let mut unpacked = 0;
    let mut previous: Option<&[u8]> = None;
    while sources < MAX_SOURCES {
        let Some(entry) = next_in_order(&archive, suffix, previous) else {
            break;
        };
        previous = Some(entry.name);
        if seen[..sources].contains(&(entry.crc, entry.size)) {
            continue;
        }
        seen[sources] = (entry.crc, entry.size);
        sources += 1;
        if entry.size == 0 || entry.size > scratch.len() {
            continue;
        }
        if unpacked + entry.size > MAX_UNPACKED_BYTES {
            break;
        }
        let Ok(length) = entry.extract(scratch) else {
            continue;
        };
        unpacked += length;
        let Ok(part) = decode_library(&scratch[..length]) else {
            continue;
        };
        decoded += 1;
        library.absorb(&part);
    }
    if decoded == 0 {
        return Err(CartridgeError::Empty);
    }
    Ok(library)
}

/// The extension this archive's cartridges are published under, or `None`
/// when it holds none of them and every file is a candidate.
fn preferred_suffix(archive: &Archive<'_>) -> Option<&'static [u8]> {
    SUFFIXES.into_iter().find(|suffix| {
        archive
            .entries()
            .filter_map(Result::ok)
            .any(|entry| !entry.is_directory() && entry.has_suffix(suffix))
    })
}

/// The next candidate after `previous` in name order.
///
/// The directory is walked again for each one rather than sorted into a
/// buffer: an archive holds a handful of entries, and this crate has no
/// allocator to sort them in.
fn next_in_order<'a>(
    archive: &Archive<'a>,
    suffix: Option<&[u8]>,
    previous: Option<&[u8]>,
) -> Option<Entry<'a>> {
    let mut best: Option<Entry<'a>> = None;
    for entry in archive.entries().filter_map(Result::ok) {
        if entry.is_directory() || suffix.is_some_and(|suffix| !entry.has_suffix(suffix)) {
            continue;
        }
        if previous
            .is_some_and(|previous| order(entry.name, previous) != core::cmp::Ordering::Greater)
        {
            continue;
        }
        if best.is_none_or(|best| order(entry.name, best.name) == core::cmp::Ordering::Less) {
            best = Some(entry);
        }
    }
    best
}

/// Names in the order a listing shows them: letter case aside, and then by
/// the bytes themselves so that no two different names compare equal.
fn order(left: &[u8], right: &[u8]) -> core::cmp::Ordering {
    left.iter()
        .map(u8::to_ascii_lowercase)
        .cmp(right.iter().map(u8::to_ascii_lowercase))
        .then_with(|| left.cmp(right))
}
