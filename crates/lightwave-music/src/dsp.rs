use std::f32::consts::PI;
use std::sync::Arc;

use anyhow::{Result, bail};
use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

/// Windowed FFT folded into log-spaced frequency bins normalized to 0..=1.
pub struct Analyzer {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// Half-open FFT-bin ranges, one per output bin.
    ranges: Vec<(usize, usize)>,
    output: Vec<f32>,
    /// 2 / sum(window): maps a windowed peak back to sine amplitude.
    norm: f32,
    gain: f32,
}

impl Analyzer {
    pub fn new(
        fft_size: usize,
        sample_rate: u32,
        bins: usize,
        min_freq: f32,
        max_freq: f32,
        gain: f32,
    ) -> Result<Self> {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(fft_size);

        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / fft_size as f32).cos()))
            .collect();
        let norm = 2.0 / window.iter().sum::<f32>();

        let ranges = bin_ranges(fft_size, sample_rate, bins, min_freq, max_freq)?;

        Ok(Self {
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            ranges,
            output: vec![0.0; bins],
            norm,
            gain,
        })
    }

    pub fn analyze(&mut self, samples: &[f32]) -> &[f32] {
        for ((dst, &sample), &w) in self.input.iter_mut().zip(samples).zip(&self.window) {
            *dst = sample * w;
        }

        self.fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .expect("buffer lengths are fixed at construction");

        for (out, &(lo, hi)) in self.output.iter_mut().zip(&self.ranges) {
            let peak = self.spectrum[lo..hi]
                .iter()
                .map(|c| c.norm())
                .fold(0.0f32, f32::max);

            *out = (peak * self.norm * self.gain).clamp(0.0, 1.0);
        }

        &self.output
    }
}

fn bin_ranges(
    fft_size: usize,
    sample_rate: u32,
    bins: usize,
    min_freq: f32,
    max_freq: f32,
) -> Result<Vec<(usize, usize)>> {
    let nyquist = sample_rate as f32 / 2.0;
    let max_freq = max_freq.min(nyquist);

    if min_freq >= max_freq {
        bail!("min-freq {min_freq} Hz must be below max-freq {max_freq} Hz (Nyquist is {nyquist} Hz)");
    }

    let spectrum_len = fft_size / 2 + 1;
    let hz_per_bin = sample_rate as f32 / fft_size as f32;
    // Skip the DC bin; it only encodes the signal's mean.
    let fft_bin = |freq: f32| ((freq / hz_per_bin) as usize).clamp(1, fft_size / 2);

    let ratio = max_freq / min_freq;
    let mut ranges = Vec::with_capacity(bins);
    let mut lo = fft_bin(min_freq);

    for i in 0..bins {
        let edge = min_freq * ratio.powf((i + 1) as f32 / bins as f32);
        // Give every output bin at least one FFT bin, sharing the top one
        // when the FFT is too small to resolve them all.
        let hi = fft_bin(edge).max(lo + 1).min(spectrum_len).max(lo);

        ranges.push((lo, hi));
        lo = hi;
    }

    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_lands_in_the_expected_bin() {
        let sample_rate = 48_000;
        let fft_size = 2048;
        let bins = 32;
        let (min_freq, max_freq) = (40.0f32, 16_000.0f32);

        let mut analyzer =
            Analyzer::new(fft_size, sample_rate, bins, min_freq, max_freq, 1.0).unwrap();

        let freq = 440.0f32;
        let samples: Vec<f32> = (0..fft_size)
            .map(|i| (2.0 * PI * freq * i as f32 / sample_rate as f32).sin())
            .collect();

        let output = analyzer.analyze(&samples).to_vec();

        let expected = ((freq / min_freq).ln() / (max_freq / min_freq).ln() * bins as f32) as usize;
        let loudest = output
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;

        assert!(
            loudest.abs_diff(expected) <= 1,
            "440 Hz peaked in bin {loudest}, expected ~{expected}"
        );
        // Full-scale sine ≈ 1.0 minus Hann scalloping loss.
        assert!(
            output[loudest] > 0.7,
            "expected near-unity amplitude, got {}",
            output[loudest]
        );
    }

    #[test]
    fn ranges_are_contiguous_and_in_bounds() {
        let fft_size = 1024;
        let ranges = bin_ranges(fft_size, 44_100, 48, 40.0, 16_000.0).unwrap();

        let mut prev_hi = ranges[0].0;
        for &(lo, hi) in &ranges {
            assert_eq!(lo, prev_hi);
            assert!(hi >= lo);
            assert!(hi <= fft_size / 2 + 1);
            prev_hi = hi;
        }
    }

    #[test]
    fn rejects_min_freq_above_nyquist() {
        assert!(bin_ranges(1024, 8_000, 16, 5_000.0, 16_000.0).is_err());
    }
}
