//! The modulation index of a two-operator FM recording.
//!
//! Lines are read at `carrier + n × modulator` for `n` in `-orders..=orders`.
//! A line below zero hertz folds back to its absolute frequency, so the
//! carrier-to-modulator ratio has to be one where no two orders land on the
//! same folded line — a modulator at four times the carrier keeps every order
//! on its own odd harmonic. A ratio that collides is refused, not averaged.

use crate::{Spectrum, bessel_j};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnalysisError {
    /// Two orders fold onto the same frequency at this ratio.
    Collision {
        first: i32,
        second: i32,
    },
    /// A needed line lies above the Nyquist frequency of the recording.
    AboveNyquist {
        order: i32,
        hz: f64,
    },
    /// No line was found at all: the recording is silent where it matters.
    NoSignal,
    BadArgument,
}

impl core::fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Collision { first, second } => write!(
                formatter,
                "orders {first} and {second} fold onto the same line at this ratio; choose a ratio without collisions"
            ),
            Self::AboveNyquist { order, hz } => write!(
                formatter,
                "order {order} sits at {hz:.0} Hz, above what this recording can hold"
            ),
            Self::NoSignal => formatter
                .write_str("no spectral lines found where the carrier and sidebands should be"),
            Self::BadArgument => formatter
                .write_str("carrier and modulator must be positive and orders at least one"),
        }
    }
}

impl std::error::Error for AnalysisError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub order: i32,
    pub expected_hz: f64,
    pub found_hz: f64,
    /// Normalised so the squares over every line sum to one.
    pub measured: f64,
    /// The model's value at the estimated index, on the same normalisation.
    pub model: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexEstimate {
    /// The modulation index, in radians of peak phase deviation.
    pub beta: f64,
    /// Root of the summed squared differences between the normalised measured
    /// and model lines. Below a few hundredths the fit is trustworthy.
    pub residual: f64,
    pub lines: Vec<Line>,
}

/// Estimate the index from a spectrum, given the two operator frequencies.
pub fn estimate_index(
    spectrum: &Spectrum,
    carrier_hz: f64,
    modulator_hz: f64,
    orders: i32,
) -> Result<IndexEstimate, AnalysisError> {
    // A NaN frequency must fail here too, which `<= 0.0` alone would not.
    let positive = |hz: f64| hz.is_finite() && hz > 0.0;
    if !positive(carrier_hz) || !positive(modulator_hz) || orders < 1 {
        return Err(AnalysisError::BadArgument);
    }
    let range: Vec<i32> = (-orders..=orders).collect();
    let expected: Vec<f64> = range
        .iter()
        .map(|n| (carrier_hz + f64::from(*n) * modulator_hz).abs())
        .collect();
    let tolerance = modulator_hz * 0.02;
    for (i, a) in expected.iter().enumerate() {
        if *a > spectrum.nyquist_hz() {
            return Err(AnalysisError::AboveNyquist {
                order: range[i],
                hz: *a,
            });
        }
        for (j, b) in expected.iter().enumerate().skip(i + 1) {
            if (a - b).abs() < tolerance * 2.0 {
                return Err(AnalysisError::Collision {
                    first: range[i],
                    second: range[j],
                });
            }
        }
    }
    let read: Vec<(f64, f64)> = expected
        .iter()
        .map(|hz| spectrum.line(*hz, tolerance))
        .collect();
    let energy: f64 = read.iter().map(|(m, _)| m * m).sum();
    if energy <= 0.0 {
        return Err(AnalysisError::NoSignal);
    }
    let measured: Vec<f64> = read.iter().map(|(m, _)| m / energy.sqrt()).collect();

    let cost = |beta: f64| -> f64 {
        let model = normalised_model(&range, beta);
        measured
            .iter()
            .zip(&model)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f64>()
    };
    // A coarse sweep over every index an operator can reach, then a fine one
    // around the best. The cost has local minima, so the sweep is not
    // optional.
    let mut best = (0.0, cost(0.0));
    let mut beta = 0.0;
    while beta <= 30.0 {
        let c = cost(beta);
        if c < best.1 {
            best = (beta, c);
        }
        beta += 0.01;
    }
    let mut fine = best;
    let mut b = (best.0 - 0.01).max(0.0);
    while b <= best.0 + 0.01 {
        let c = cost(b);
        if c < fine.1 {
            fine = (b, c);
        }
        b += 0.0001;
    }
    let model = normalised_model(&range, fine.0);
    let lines = range
        .iter()
        .enumerate()
        .map(|(i, order)| Line {
            order: *order,
            expected_hz: expected[i],
            found_hz: read[i].1,
            measured: measured[i],
            model: model[i],
        })
        .collect();
    Ok(IndexEstimate {
        beta: fine.0,
        residual: fine.1.sqrt(),
        lines,
    })
}

fn normalised_model(range: &[i32], beta: f64) -> Vec<f64> {
    let raw: Vec<f64> = range.iter().map(|n| bessel_j(*n, beta).abs()).collect();
    let energy: f64 = raw.iter().map(|v| v * v).sum::<f64>().sqrt();
    if energy <= 0.0 {
        return raw;
    }
    raw.into_iter().map(|v| v / energy).collect()
}
