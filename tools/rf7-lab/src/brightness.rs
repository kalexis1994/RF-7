//! Spectral centroid: where the energy of a note sits, in hertz.
//!
//! Two windows, because a struck sound and a held one are not the same
//! question: the first quarter second, where a hammer or a pluck lives, and
//! the second after it, where the body is — or where nothing is, which is
//! also worth knowing. One number per window per voice, so two voices can be
//! compared for brightness without anyone listening. It does not say a voice
//! is good; it says whether it is in the same part of the spectrum as the one
//! it is meant to resemble, which is the question a hand-designed bank can
//! answer against a real cartridge.

use crate::{Options, engine, read_library};
use rf7_analysis::spectrum;
use rf7_dsp::Engine;
use std::error::Error;

pub const HELP: &str =
    "  rf7-lab brightness --program N [--cartridge PATH] [--note N] [--velocity N]
                         [--against PATH --against-program N]
Renders a note of one voice, and optionally a second voice from another
library, and prints each one's spectral centroid and peak over the attack
(the first quarter second) and the body (the second after it). --against
with no --cartridge compares a factory voice with one from a real cartridge.
";

const ATTACK_SECONDS: f64 = 0.25;
const BODY_SECONDS: f64 = 1.0;
const CEILING_HZ: f64 = 8_000.0;
/// Below this peak a window is reported as silent rather than given a
/// centroid of whatever noise is left.
const AUDIBLE: f32 = 1e-4;

/// Centroid of the magnitude spectrum up to `ceiling_hz`, in hertz.
pub fn centroid(samples: &[f32], sample_rate: f64, ceiling_hz: f64) -> f64 {
    let spectrum = spectrum(samples, sample_rate);
    let bin = spectrum.bin_hz();
    let mut weighted = 0.0;
    let mut total = 0.0;
    for (index, magnitude) in spectrum.magnitudes().iter().enumerate() {
        let hz = index as f64 * bin;
        if hz > ceiling_hz {
            break;
        }
        weighted += hz * magnitude;
        total += magnitude;
    }
    if total > 0.0 { weighted / total } else { 0.0 }
}

fn describe(samples: &[f32], rate: f64) -> String {
    let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
    if peak < AUDIBLE {
        "  silent          ".to_owned()
    } else {
        format!(
            "{:>7.0} Hz @{:.3}",
            centroid(samples, rate, CEILING_HZ),
            peak
        )
    }
}

fn measure(mut engine: Engine, name: &str, options: &Options) {
    let rate = f64::from(options.sample_rate);
    engine.note_on(0, options.note, options.velocity);
    let attack: Vec<f32> = (0..(rate * ATTACK_SECONDS) as usize)
        .map(|_| engine.next_sample())
        .collect();
    let body: Vec<f32> = (0..(rate * BODY_SECONDS) as usize)
        .map(|_| engine.next_sample())
        .collect();
    println!(
        "  {:<12} attack {}   body {}",
        name,
        describe(&attack, rate),
        describe(&body, rate)
    );
}

pub fn run(options: &Options, against: Option<(&str, usize)>) -> Result<(), Box<dyn Error>> {
    let first = engine(options)?;
    let name = first.program_name(options.program).to_owned();
    println!(
        "note {} velocity {}: centroid and peak over the first {ATTACK_SECONDS} s, then the next {BODY_SECONDS} s",
        options.note, options.velocity
    );
    measure(first, &name, options);
    if let Some((path, program)) = against {
        let mut other = Engine::new(options.sample_rate)?;
        other.load_library(read_library(
            std::path::Path::new(path),
            options.swap_banks,
        )?);
        if !other.select_program(program) {
            return Err(format!("no program {} in {path}", program + 1).into());
        }
        let name = other.program_name(program).to_owned();
        measure(other, &name, options);
    }
    Ok(())
}
