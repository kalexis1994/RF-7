//! The estimator against signals whose index is known exactly.
//!
//! The first half builds phase-modulated sines directly from the formula, so
//! the only thing under test is the analysis. The second half renders RF-7's
//! own calibration voice, so what is under test is the engine's modulation
//! depth — and that is the number every patch in the bank hangs from.

use core::f64::consts::{PI, TAU};
use rf7_analysis::{AnalysisError, estimate_index, spectrum};
use rf7_dsp::Engine;
use rf7_voice::{Library, calibration_voice};

const RATE: f64 = 48_000.0;
const CARRIER: f64 = 261.625_565;
const MODULATOR: f64 = CARRIER * 4.0;

fn synthetic(beta: f64, seconds: f64) -> Vec<f32> {
    (0..(RATE * seconds) as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            (TAU * CARRIER * t + beta * (TAU * MODULATOR * t).sin()).sin() as f32
        })
        .collect()
}

#[test]
fn a_known_index_is_recovered_from_the_formula() {
    for beta in [0.3, 1.0, 2.0, 4.0, TAU, 9.5, 14.0] {
        let estimate = estimate_index(
            &spectrum(&synthetic(beta, 2.0), RATE),
            CARRIER,
            MODULATOR,
            8,
        )
        .expect("a clean ratio must estimate");
        assert!(
            (estimate.beta - beta).abs() < beta.max(0.5) * 0.01,
            "index {beta} came back as {} (residual {})",
            estimate.beta,
            estimate.residual
        );
        assert!(
            estimate.residual < 0.03,
            "residual {} at index {beta}",
            estimate.residual
        );
    }
}

#[test]
fn the_estimate_does_not_care_how_loud_the_recording_was() {
    let quiet: Vec<f32> = synthetic(3.0, 2.0).iter().map(|s| s * 0.01).collect();
    let loud: Vec<f32> = synthetic(3.0, 2.0).iter().map(|s| s * 4.0).collect();
    let a = estimate_index(&spectrum(&quiet, RATE), CARRIER, MODULATOR, 8).unwrap();
    let b = estimate_index(&spectrum(&loud, RATE), CARRIER, MODULATOR, 8).unwrap();
    assert!((a.beta - b.beta).abs() < 0.01);
    assert!((a.beta - 3.0).abs() < 0.03);
}

#[test]
fn a_colliding_ratio_is_refused_rather_than_averaged() {
    let signal = synthetic(2.0, 1.0);
    let spectrum = spectrum(&signal, RATE);
    // 1:1 folds order -2 onto the carrier and -1 onto zero hertz.
    assert!(matches!(
        estimate_index(&spectrum, CARRIER, CARRIER, 4),
        Err(AnalysisError::Collision { .. })
    ));
    // A modulator so high that the eighth order is above Nyquist.
    assert!(matches!(
        estimate_index(&spectrum, CARRIER, 5_000.0, 8),
        Err(AnalysisError::AboveNyquist { .. })
    ));
    assert_eq!(
        estimate_index(&spectrum, 0.0, MODULATOR, 8).unwrap_err(),
        AnalysisError::BadArgument
    );
}

#[test]
fn silence_is_reported_as_no_signal() {
    let silence = vec![0.0f32; 48_000];
    assert_eq!(
        estimate_index(&spectrum(&silence, RATE), CARRIER, MODULATOR, 4).unwrap_err(),
        AnalysisError::NoSignal
    );
}

fn render_calibration(level: u8) -> Vec<f32> {
    let mut engine = Engine::new(RATE as f32).expect("48 kHz");
    engine.set_gain(1.0);
    engine.load_library(Library::from_voices(&[calibration_voice(level)]));
    engine.note_on(0, 60, 127);
    // Half a second for the envelopes to sit, then two seconds of tone.
    for _ in 0..24_000 {
        engine.next_sample();
    }
    (0..96_000).map(|_| engine.next_sample()).collect()
}

#[test]
fn rf7_at_full_modulator_level_deviates_by_exactly_one_cycle() {
    // MODULATION_CYCLES is 1.0: a modulator at unity gain swings the carrier
    // by one cycle, which is 2π radians of index. This is the engine's own
    // claim, measured the same way a real DX7 will be.
    let estimate = estimate_index(
        &spectrum(&render_calibration(99), RATE),
        CARRIER,
        MODULATOR,
        8,
    )
    .unwrap();
    assert!(
        (estimate.beta - TAU).abs() < 0.05,
        "the engine's index at level 99 is {} (residual {})",
        estimate.beta,
        estimate.residual
    );
    assert!(estimate.residual < 0.05);
}

#[test]
fn the_engines_level_curve_is_the_one_its_tables_declare() {
    // At output level L the modulator gain is 2^((32·scale(L) − 4064) / 256),
    // so the index should follow 2π times that. If this ever drifts, either
    // the level tables or the operator kernel changed without the other.
    for (level, expected_gain) in [(90u8, 0.459_479f64), (80, 0.192_889), (70, 0.081_020)] {
        let estimate = estimate_index(
            &spectrum(&render_calibration(level), RATE),
            CARRIER,
            MODULATOR,
            6,
        )
        .unwrap();
        let expected = TAU * expected_gain;
        assert!(
            (estimate.beta - expected).abs() < expected * 0.03 + 0.01,
            "level {level}: measured {} against {expected}",
            estimate.beta
        );
    }
    let _ = PI;
}
