//! A radix-2 FFT and the magnitude spectrum built on it.

use core::f64::consts::PI;

/// In-place complex FFT. `re` and `im` must be the same power-of-two length.
pub fn fft_in_place(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert_eq!(n, im.len(), "real and imaginary parts differ in length");
    assert!(n.is_power_of_two(), "the FFT length must be a power of two");
    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // Butterflies.
    let mut length = 2;
    while length <= n {
        let angle = -2.0 * PI / length as f64;
        let (w_re, w_im) = (angle.cos(), angle.sin());
        let half = length / 2;
        for start in (0..n).step_by(length) {
            let (mut c_re, mut c_im) = (1.0, 0.0);
            for k in 0..half {
                let (a, b) = (start + k, start + k + half);
                let t_re = re[b] * c_re - im[b] * c_im;
                let t_im = re[b] * c_im + im[b] * c_re;
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
                let next_re = c_re * w_re - c_im * w_im;
                c_im = c_re * w_im + c_im * w_re;
                c_re = next_re;
            }
        }
        length <<= 1;
    }
}

/// Magnitudes from DC to Nyquist, and the width of one bin.
#[derive(Clone, Debug)]
pub struct Spectrum {
    magnitudes: Vec<f64>,
    bin_hz: f64,
    /// The width of one bin *before* zero padding: what a line's main lobe
    /// is measured in.
    resolution_hz: f64,
}

impl Spectrum {
    pub fn bin_hz(&self) -> f64 {
        self.bin_hz
    }

    pub fn nyquist_hz(&self) -> f64 {
        self.bin_hz * (self.magnitudes.len() - 1) as f64
    }

    pub fn magnitudes(&self) -> &[f64] {
        &self.magnitudes
    }

    /// The amplitude of the line nearest `hz` within `tolerance_hz`, and
    /// where it actually was.
    ///
    /// Not the peak bin: a line that falls between two bins loses up to 1.4 dB
    /// of peak to the window, and two lines losing different amounts would
    /// give a ratio that is not theirs. The energy across the window's main
    /// lobe does not depend on where the line fell, so that is what is read.
    /// A line that is not there returns the noise at that spot, which is why
    /// an estimate reports its residual rather than a verdict.
    pub fn line(&self, hz: f64, tolerance_hz: f64) -> (f64, f64) {
        let last = self.magnitudes.len().saturating_sub(1);
        let low = ((hz - tolerance_hz) / self.bin_hz).floor().max(0.0) as usize;
        let high = (((hz + tolerance_hz) / self.bin_hz).ceil() as usize).min(last);
        let mut peak = (0.0, low);
        for bin in low..=high {
            if self.magnitudes[bin] > peak.0 {
                peak = (self.magnitudes[bin], bin);
            }
        }
        // A Hann main lobe is four resolution bins wide; integrate over it.
        let lobe = (2.0 * self.resolution_hz / self.bin_hz).ceil() as usize;
        let from = peak.1.saturating_sub(lobe);
        let to = (peak.1 + lobe).min(last);
        let energy: f64 = self.magnitudes[from..=to].iter().map(|m| m * m).sum();
        (energy.sqrt(), peak.1 as f64 * self.bin_hz)
    }
}

/// Hann-windowed magnitude spectrum, zero-padded to the next power of two.
///
/// The window keeps a line's energy from smearing into its neighbours, which
/// matters here more than usual: the sidebands the estimator reads sit at
/// exact multiples of the modulator, and leakage from a strong one would be
/// read as a weak one that does not exist.
pub fn spectrum(samples: &[f32], sample_rate: f64) -> Spectrum {
    let n = samples.len().max(2).next_power_of_two();
    let mut re = vec![0.0f64; n];
    let mut im = vec![0.0f64; n];
    let count = samples.len().max(1) as f64;
    for (index, sample) in samples.iter().enumerate() {
        let window = 0.5 - 0.5 * (2.0 * PI * index as f64 / count).cos();
        re[index] = f64::from(*sample) * window;
    }
    fft_in_place(&mut re, &mut im);
    let magnitudes = (0..=n / 2)
        .map(|bin| (re[bin] * re[bin] + im[bin] * im[bin]).sqrt())
        .collect();
    Spectrum {
        magnitudes,
        bin_hz: sample_rate / n as f64,
        resolution_hz: sample_rate / count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pure_tone_lands_in_its_own_bin() {
        let rate = 48_000.0;
        let hz = 1_500.0;
        let samples: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * PI * hz * i as f64 / rate).sin() as f32)
            .collect();
        let spectrum = spectrum(&samples, rate);
        let (peak, at) = spectrum.line(hz, 50.0);
        assert!((at - hz).abs() <= spectrum.bin_hz());
        // Nothing comparable an octave away.
        let (elsewhere, _) = spectrum.line(hz * 2.0, 50.0);
        assert!(
            elsewhere < peak * 1e-3,
            "leakage {elsewhere} against {peak}"
        );
    }

    #[test]
    fn the_transform_inverts_itself_up_to_scale() {
        let n = 1024;
        let original: Vec<f64> = (0..n).map(|i| ((i * 7) % 13) as f64 - 6.0).collect();
        let mut re = original.clone();
        let mut im = vec![0.0; n];
        fft_in_place(&mut re, &mut im);
        // Inverse via conjugation: conj(FFT(conj(X))) / n.
        for value in &mut im {
            *value = -*value;
        }
        fft_in_place(&mut re, &mut im);
        for (index, value) in original.iter().enumerate() {
            assert!((re[index] / n as f64 - value).abs() < 1e-9);
        }
    }
}
