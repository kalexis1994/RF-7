//! The two System Exclusive containers a DX7 sends and accepts.
//!
//! ```text
//! bulk dump    F0 43 0n 09 20 00  32 x 128 packed bytes  checksum F7   4104 bytes
//! voice dump   F0 43 0n 00 01 1B  155 unpacked bytes     checksum F7    163 bytes
//! ```
//!
//! `n` is the sub-status channel and is accepted at any value 0..=15; RF-7
//! reads files, not a MIDI cable, and the channel a cartridge was dumped on
//! says nothing about the voices in it.

use crate::{
    Corrections, Decoded,
    packed::{PACKED_VOICE_LENGTH, decode_packed, encode_packed},
    parameters::Voice,
    unpacked::{UNPACKED_VOICE_LENGTH, decode_unpacked, encode_unpacked},
};

pub const VOICES_PER_CARTRIDGE: usize = 32;
pub const BULK_DUMP_LENGTH: usize = 4104;
pub const VOICE_DUMP_LENGTH: usize = 163;

const BULK_PAYLOAD: usize = VOICES_PER_CARTRIDGE * PACKED_VOICE_LENGTH;
const HEADER: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysexError {
    /// The byte count is not one of the two documented lengths.
    Length {
        found: usize,
    },
    /// The message does not open F0 43, close F7, or carry the expected
    /// format and byte-count words.
    Header,
    /// A payload byte has bit 7 set. System Exclusive data is seven-bit.
    DataBit {
        offset: usize,
    },
    Checksum {
        found: u8,
        expected: u8,
    },
}

impl core::fmt::Display for SysexError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Length { found } => write!(
                formatter,
                "expected a {BULK_DUMP_LENGTH}-byte cartridge or a {VOICE_DUMP_LENGTH}-byte voice dump, found {found} bytes"
            ),
            Self::Header => formatter.write_str("not a Yamaha DX7 voice or cartridge dump"),
            Self::DataBit { offset } => write!(
                formatter,
                "byte {offset} has bit 7 set; System Exclusive data is seven-bit"
            ),
            Self::Checksum { found, expected } => write!(
                formatter,
                "checksum {found:#04x} does not match the computed {expected:#04x}"
            ),
        }
    }
}

impl core::error::Error for SysexError {}

/// Thirty-two voices and the evidence of how faithful their bytes were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cartridge {
    voices: [Voice; VOICES_PER_CARTRIDGE],
    corrections: [u32; VOICES_PER_CARTRIDGE],
}

impl Cartridge {
    pub const fn from_voices(voices: [Voice; VOICES_PER_CARTRIDGE]) -> Self {
        Self {
            voices,
            corrections: [0; VOICES_PER_CARTRIDGE],
        }
    }

    pub fn voice(&self, index: usize) -> Option<&Voice> {
        self.voices.get(index)
    }

    pub fn voices(&self) -> &[Voice; VOICES_PER_CARTRIDGE] {
        &self.voices
    }

    /// Bytes the decode had to clamp across the whole cartridge.
    pub fn corrections(&self) -> Corrections {
        Corrections(self.corrections.iter().sum())
    }

    pub fn voice_corrections(&self, index: usize) -> Corrections {
        Corrections(self.corrections.get(index).copied().unwrap_or(0))
    }
}

/// The seven-bit two's complement checksum Yamaha appends to a payload.
pub fn checksum(payload: &[u8]) -> u8 {
    let sum = payload
        .iter()
        .fold(0u8, |total, byte| total.wrapping_add(*byte));
    sum.wrapping_neg() & 0x7f
}

pub fn decode_bulk_dump(bytes: &[u8]) -> Result<Cartridge, SysexError> {
    if bytes.len() != BULK_DUMP_LENGTH {
        return Err(SysexError::Length { found: bytes.len() });
    }
    // 09 20 00: format 9 (32 voices), byte count 0x20_00 = 4096.
    check_frame(bytes, &[0x09, 0x20, 0x00])?;
    let payload = &bytes[HEADER..HEADER + BULK_PAYLOAD];
    check_payload(payload, HEADER)?;
    check_checksum(payload, bytes[HEADER + BULK_PAYLOAD])?;
    let mut voices = [Voice::init(); VOICES_PER_CARTRIDGE];
    let mut corrections = [0u32; VOICES_PER_CARTRIDGE];
    for (index, voice) in voices.iter_mut().enumerate() {
        let mut block = [0u8; PACKED_VOICE_LENGTH];
        block.copy_from_slice(&payload[index * PACKED_VOICE_LENGTH..][..PACKED_VOICE_LENGTH]);
        let decoded = decode_packed(&block);
        *voice = decoded.voice;
        corrections[index] = decoded.corrections.0;
    }
    Ok(Cartridge {
        voices,
        corrections,
    })
}

pub fn encode_bulk_dump(cartridge: &Cartridge, channel: u8) -> [u8; BULK_DUMP_LENGTH] {
    let mut bytes = [0u8; BULK_DUMP_LENGTH];
    bytes[..HEADER].copy_from_slice(&[0xf0, 0x43, channel & 0x0f, 0x09, 0x20, 0x00]);
    for (index, voice) in cartridge.voices.iter().enumerate() {
        bytes[HEADER + index * PACKED_VOICE_LENGTH..][..PACKED_VOICE_LENGTH]
            .copy_from_slice(&encode_packed(voice));
    }
    bytes[HEADER + BULK_PAYLOAD] = checksum(&bytes[HEADER..HEADER + BULK_PAYLOAD]);
    bytes[BULK_DUMP_LENGTH - 1] = 0xf7;
    bytes
}

pub fn decode_voice_dump(bytes: &[u8]) -> Result<Decoded, SysexError> {
    if bytes.len() != VOICE_DUMP_LENGTH {
        return Err(SysexError::Length { found: bytes.len() });
    }
    // 00 01 1B: format 0 (one voice), byte count 0x01_1B = 155.
    check_frame(bytes, &[0x00, 0x01, 0x1b])?;
    let payload = &bytes[HEADER..HEADER + UNPACKED_VOICE_LENGTH];
    check_payload(payload, HEADER)?;
    check_checksum(payload, bytes[HEADER + UNPACKED_VOICE_LENGTH])?;
    let mut block = [0u8; UNPACKED_VOICE_LENGTH];
    block.copy_from_slice(payload);
    Ok(decode_unpacked(&block))
}

pub fn encode_voice_dump(voice: &Voice, channel: u8) -> [u8; VOICE_DUMP_LENGTH] {
    let mut bytes = [0u8; VOICE_DUMP_LENGTH];
    bytes[..HEADER].copy_from_slice(&[0xf0, 0x43, channel & 0x0f, 0x00, 0x01, 0x1b]);
    bytes[HEADER..HEADER + UNPACKED_VOICE_LENGTH].copy_from_slice(&encode_unpacked(voice));
    bytes[HEADER + UNPACKED_VOICE_LENGTH] =
        checksum(&bytes[HEADER..HEADER + UNPACKED_VOICE_LENGTH]);
    bytes[VOICE_DUMP_LENGTH - 1] = 0xf7;
    bytes
}

fn check_frame(bytes: &[u8], format: &[u8; 3]) -> Result<(), SysexError> {
    let opens = bytes[0] == 0xf0 && bytes[1] == 0x43 && bytes[2] & 0xf0 == 0;
    if !opens || &bytes[3..HEADER] != format || bytes[bytes.len() - 1] != 0xf7 {
        return Err(SysexError::Header);
    }
    Ok(())
}

fn check_payload(payload: &[u8], base: usize) -> Result<(), SysexError> {
    match payload.iter().position(|byte| *byte & 0x80 != 0) {
        Some(offset) => Err(SysexError::DataBit {
            offset: base + offset,
        }),
        None => Ok(()),
    }
}

fn check_checksum(payload: &[u8], found: u8) -> Result<(), SysexError> {
    let expected = checksum(payload);
    if found != expected {
        return Err(SysexError::Checksum { found, expected });
    }
    Ok(())
}
