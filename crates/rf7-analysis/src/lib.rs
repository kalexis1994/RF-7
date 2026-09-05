//! Offline measurement of frequency-modulated audio.
//!
//! The one question this crate exists to answer: given a recording of two
//! operators — one modulating the other — how deep was the modulation? In FM
//! the spectrum is known in closed form: lines at the carrier plus and minus
//! every multiple of the modulator, with amplitudes given by Bessel functions
//! of the modulation index. The *ratios* between those lines depend on the
//! index alone, not on how loud the recording was or what it went through, so
//! the index can be read off a recording of unknown level.
//!
//! No dependencies: the FFT and the Bessel functions are here, and are tested
//! against known values before anything is trusted to them.

mod bessel;
mod estimate;
mod fft;

pub use bessel::bessel_j;
pub use estimate::{AnalysisError, IndexEstimate, Line, estimate_index};
pub use fft::{Spectrum, fft_in_place, spectrum};
