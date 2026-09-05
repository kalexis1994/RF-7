//! Everything RF-7 can play at once, however it arrived.
//!
//! A DX7 cartridge holds thirty-two voices, and that is still the unit the
//! System Exclusive containers carry. What reaches RF-7 is rarely one of them:
//! a cartridge ROM dumped from its chip is 8192 bytes with no header at all, a
//! collection downloaded as one file is several bulk dumps end to end, and a
//! single patch is 163 bytes on its own. A library is however many voices came
//! out of any of those.
//!
//! ROM dumps carry no checksum — there is no room for one in a chip image — so
//! a raw bank is accepted on its shape alone: the right length, and every byte
//! seven-bit. The corrections count is then the only quality signal there is,
//! which is why it travels with the library rather than being discarded.

use crate::{
    Corrections,
    packed::{PACKED_VOICE_LENGTH, decode_packed},
    parameters::Voice,
    sysex::{BULK_DUMP_LENGTH, SysexError, VOICE_DUMP_LENGTH, VOICES_PER_CARTRIDGE},
};

/// As many voices as RF-7 will offer as programs at once.
pub const MAX_VOICES: usize = 128;
/// One bank of thirty-two packed voices, as a chip holds them.
pub const RAW_BANK_LENGTH: usize = VOICES_PER_CARTRIDGE * PACKED_VOICE_LENGTH;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    voices: [Voice; MAX_VOICES],
    corrections: [u32; MAX_VOICES],
    length: usize,
    found: usize,
}

impl Default for Library {
    /// One INIT VOICE, so a library is never empty and never a special case.
    fn default() -> Self {
        Self::from_voices(&[Voice::init()])
    }
}

impl Library {
    /// Keeps the first [`MAX_VOICES`]; [`Library::found`] reports the rest.
    pub fn from_voices(voices: &[Voice]) -> Self {
        let mut library = Self {
            voices: [Voice::init(); MAX_VOICES],
            corrections: [0; MAX_VOICES],
            length: voices.len().min(MAX_VOICES),
            found: voices.len(),
        };
        library.voices[..library.length].copy_from_slice(&voices[..library.length]);
        library
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// How many voices the source held, which can exceed what was kept.
    pub fn found(&self) -> usize {
        self.found
    }

    pub fn voice(&self, index: usize) -> Option<&Voice> {
        self.voices.get(..self.length)?.get(index)
    }

    pub fn voices(&self) -> &[Voice] {
        &self.voices[..self.length]
    }

    /// Bytes clamped across the whole library while decoding.
    pub fn corrections(&self) -> Corrections {
        Corrections(self.corrections[..self.length].iter().sum())
    }

    pub fn voice_corrections(&self, index: usize) -> Corrections {
        Corrections(if index < self.length {
            self.corrections[index]
        } else {
            0
        })
    }

    fn push(&mut self, voice: Voice, corrections: u32) {
        self.found += 1;
        if self.length < MAX_VOICES {
            self.voices[self.length] = voice;
            self.corrections[self.length] = corrections;
            self.length += 1;
        }
    }

    fn empty() -> Self {
        Self {
            voices: [Voice::init(); MAX_VOICES],
            corrections: [0; MAX_VOICES],
            length: 0,
            found: 0,
        }
    }
}

/// Read whatever shape these bytes are.
///
/// Accepted: any whole number of 4104-byte bulk dumps, one 163-byte voice
/// dump, or any whole number of 4096-byte raw banks. Anything else is refused
/// rather than guessed at, because a length RF-7 does not recognise is far
/// more likely to be a different file than a cartridge with a byte missing.
pub fn decode_library(bytes: &[u8]) -> Result<Library, SysexError> {
    if bytes.is_empty() {
        return Err(SysexError::Length { found: 0 });
    }
    if bytes.len() == VOICE_DUMP_LENGTH {
        let decoded = crate::decode_voice_dump(bytes)?;
        let mut library = Library::empty();
        library.push(decoded.voice, decoded.corrections.0);
        return Ok(library);
    }
    if bytes.len().is_multiple_of(BULK_DUMP_LENGTH) {
        let mut library = Library::empty();
        for dump in bytes.as_chunks::<BULK_DUMP_LENGTH>().0 {
            let cartridge = crate::decode_bulk_dump(dump)?;
            for (index, voice) in cartridge.voices().iter().enumerate() {
                library.push(*voice, cartridge.voice_corrections(index).0);
            }
        }
        return Ok(library);
    }
    if bytes.len().is_multiple_of(RAW_BANK_LENGTH) {
        return decode_raw_banks(bytes);
    }
    Err(SysexError::Length { found: bytes.len() })
}

/// Chip images: packed voices with no header, no checksum and no framing.
fn decode_raw_banks(bytes: &[u8]) -> Result<Library, SysexError> {
    if let Some(offset) = bytes.iter().position(|byte| *byte & 0x80 != 0) {
        return Err(SysexError::DataBit { offset });
    }
    let mut library = Library::empty();
    for packed in bytes.as_chunks::<PACKED_VOICE_LENGTH>().0 {
        let decoded = decode_packed(packed);
        library.push(decoded.voice, decoded.corrections.0);
    }
    Ok(library)
}
