//! Studio renders down to the effects path: mono mu-law at the mixer's
//! rate, pitched by playback rate like any chip8 effect.

/// Folds a 16-bit stereo render made at **twice** the effect rate (for the
/// default [`MixerConfig`](super::MixerConfig), 44,100 Hz) down to mono at
/// the effect rate, ready for [`mulaw::encode`](crate::chip8::mulaw::encode).
///
/// The two sides are averaged, then low-passed before every other sample is
/// dropped: a 63-tap Blackman-windowed sinc cutting at 0.225 cycles per
/// input sample, so nothing above the new Nyquist folds back as an alias.
/// The taps sum to one: a level stays a level.
///
/// Ported from BRYX's build script, unchanged, so an effect folds to the
/// same samples here as it did there.
pub fn fold_effect(stereo: &[i16]) -> Vec<i16> {
    const TAPS: usize = 63;
    let mono: Vec<f64> = stereo
        .chunks_exact(2)
        .map(|lr| (f64::from(lr[0]) + f64::from(lr[1])) * 0.5)
        .collect();
    let cut = 0.225; // cycles per input sample
    let mid = (TAPS / 2) as f64;
    let mut h: Vec<f64> = (0..TAPS)
        .map(|i| {
            let x = i as f64 - mid;
            let sinc = if x == 0.0 {
                2.0 * cut
            } else {
                (2.0 * std::f64::consts::PI * cut * x).sin() / (std::f64::consts::PI * x)
            };
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (TAPS - 1) as f64).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / (TAPS - 1) as f64).cos();
            sinc * w
        })
        .collect();
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= sum);
    (0..mono.len() / 2)
        .map(|n| {
            let c = 2 * n;
            let acc: f64 = h
                .iter()
                .enumerate()
                .map(|(k, &w)| {
                    let i = c as isize + k as isize - mid as isize;
                    if i < 0 || i as usize >= mono.len() {
                        0.0
                    } else {
                        w * mono[i as usize]
                    }
                })
                .sum();
            acc.round().clamp(-32_768.0, 32_767.0) as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stereo sine at `cycles` per input sample, both sides alike.
    fn sine(cycles: f64, frames: usize) -> Vec<i16> {
        (0..frames)
            .flat_map(|i| {
                let s = (10_000.0 * (std::f64::consts::TAU * cycles * i as f64).sin()) as i16;
                [s, s]
            })
            .collect()
    }

    fn rms(x: &[i16]) -> f64 {
        (x.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    #[test]
    fn half_the_frames_come_out_mono() {
        let out = fold_effect(&sine(0.01, 1_000));
        assert_eq!(out.len(), 500);
    }

    #[test]
    fn a_tone_under_the_new_nyquist_passes_and_one_above_is_cut() {
        // 0.05 cycles per input sample: 2,205 Hz at 44.1 kHz.
        let low = fold_effect(&sine(0.05, 8_000));
        // 0.4: 17,640 Hz, which would alias to 4,410 Hz at 22.05 kHz.
        let high = fold_effect(&sine(0.4, 8_000));
        let inner = |x: &[i16]| rms(&x[100..x.len() - 100]);
        assert!((inner(&low) / (10_000.0 / 2f64.sqrt()) - 1.0).abs() < 0.02);
        assert!(inner(&high) < 10.0, "alias at {:.1}", inner(&high));
    }

    #[test]
    fn a_level_stays_a_level_and_the_sides_are_averaged() {
        let stereo: Vec<i16> = (0..400).flat_map(|_| [3_000, 1_000]).collect();
        let out = fold_effect(&stereo);
        assert!(
            out[50..150].iter().all(|&s| s == 2_000),
            "{:?}",
            &out[50..60]
        );
    }
}
