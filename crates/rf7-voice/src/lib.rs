//! The DX7 voice data model: parameters, the two documented byte layouts and
//! the System Exclusive containers that carry them.
//!
//! This crate holds no audio state and performs no I/O. It exists so the
//! engine, the laboratory and the plugin agree on exactly one interpretation
//! of a cartridge byte, and so that interpretation is testable on its own.
//!
//! No Yamaha voice data is stored here. [`factory`] voices are written for
//! RF-7; a user's own cartridges arrive at runtime as resource bytes.
#![no_std]

mod factory;
mod packed;
mod parameters;
mod sysex;
mod unpacked;

pub use factory::{FACTORY_VOICES, factory_cartridge, factory_voice};
pub use packed::{PACKED_VOICE_LENGTH, decode_packed, encode_packed};
pub use parameters::{
    Curve, Lfo, LfoWaveform, NAME_LENGTH, OPERATORS, Operator, Voice, printable_name,
};
pub use sysex::{
    BULK_DUMP_LENGTH, Cartridge, SysexError, VOICE_DUMP_LENGTH, VOICES_PER_CARTRIDGE, checksum,
    decode_bulk_dump, decode_voice_dump, encode_bulk_dump, encode_voice_dump,
};
pub use unpacked::{UNPACKED_VOICE_LENGTH, decode_unpacked, encode_unpacked};

/// How many out-of-range bytes a decode had to clamp.
///
/// Cartridges collected in the wild routinely carry values the DX7 itself
/// would have ignored. Decoding never fails on them, but it never hides them
/// either: the count travels with the voice so a report can say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Corrections(pub u32);

impl Corrections {
    pub const fn is_clean(self) -> bool {
        self.0 == 0
    }
}

/// A decoded voice and the evidence of how faithful the source bytes were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub voice: Voice,
    pub corrections: Corrections,
}
