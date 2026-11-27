//! A QOA render played at the device's rate: decoded as it plays and
//! resampled with a Catmull-Rom cubic. Ported from BRYX's `StudioLoop`.

use super::qoa::{QoaError, QoaStream};

/// A QOA stream, decoded and resampled to the output rate. No allocation:
/// the decoder reads the embedded bytes, and the cubic keeps four frames.
pub(crate) struct QoaPlayer {
    qoa: QoaStream<'static>,
    /// Four source frames; the output lies between `hist[1]` and `hist[2]`.
    hist: [[f32; 2]; 4],
    /// Position between `hist[1]` and `hist[2]`, 0..1.
    pub(crate) frac: f64,
    /// Source frames per output frame (times the pitch).
    pub(crate) step: f64,
    /// A loop wraps to its start; a one-shot stops at its end.
    looping: bool,
    /// A one-shot has run out of frames.
    done: bool,
}

impl QoaPlayer {
    /// A player for `bytes` writing at `output_rate`. A mono stream plays
    /// on both sides.
    pub(crate) fn new(
        bytes: &'static [u8],
        output_rate: f32,
        looping: bool,
    ) -> Result<Self, QoaError> {
        let qoa = QoaStream::new(bytes)?;
        let step = f64::from(qoa.sample_rate()) / f64::from(output_rate.max(1.0));
        let mut player = Self {
            qoa,
            hist: [[0.0; 2]; 4],
            frac: 0.0,
            step,
            looping,
            done: false,
        };
        // Prime so the first output is the first frame: [0, f0, f1, f2].
        for i in 1..4 {
            player.hist[i] = player.pull();
        }
        Ok(player)
    }

    /// The next source frame. A loop wraps to its start at the end (the
    /// studio renders a loop with its tails already wrapped, so the seam is
    /// part of the music); a one-shot is silent past it.
    fn pull(&mut self) -> [f32; 2] {
        let frame = self.qoa.next_frame().or_else(|| {
            if !self.looping {
                self.done = true;
                return None;
            }
            self.qoa.rewind();
            self.qoa.next_frame()
        });
        // The decoder repeats a mono stream's one channel on the right.
        frame.map_or([0.0; 2], |[l, r]| {
            [f32::from(l) / 32_768.0, f32::from(r) / 32_768.0]
        })
    }

    /// A one-shot that has played its last frame.
    pub(crate) fn finished(&self) -> bool {
        self.done
    }

    /// The next output frame, left and right.
    pub(crate) fn next(&mut self) -> (f32, f32) {
        let t = self.frac as f32;
        let [p0, p1, p2, p3] = self.hist;
        let out = (
            catmull_rom(p0[0], p1[0], p2[0], p3[0], t),
            catmull_rom(p0[1], p1[1], p2[1], p3[1], t),
        );
        self.frac += self.step;
        while self.frac >= 1.0 {
            self.frac -= 1.0;
            self.hist.copy_within(1..4, 0);
            self.hist[3] = self.pull();
        }
        out
    }
}

/// The Catmull-Rom spline through `p1` (t = 0) and `p2` (t = 1).
#[inline]
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    p1 + 0.5
        * t
        * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 12,000 stereo frames at 48 kHz, made by the reference encoder
    /// (`qoa.rs`'s tests check the decoder against it bit for bit).
    const REF_QOA: &[u8] = include_bytes!("testdata/ref.qoa");

    #[test]
    fn a_loop_at_its_own_rate_plays_the_decoded_frames_and_wraps() {
        let mut stream = QoaPlayer::new(REF_QOA, 48_000.0, true).unwrap();
        let mut reference = QoaStream::new(REF_QOA).unwrap();
        let len = reference.len() as usize;
        let decoded: Vec<[i16; 2]> = std::iter::from_fn(|| reference.next_frame()).collect();
        assert_eq!(decoded.len(), len);
        for i in 0..len + 2_500 {
            let [l, r] = decoded[i % len];
            let want = (f32::from(l) / 32_768.0, f32::from(r) / 32_768.0);
            assert_eq!(stream.next(), want, "frame {i}");
        }
        assert!(
            decoded.iter().any(|[l, r]| l != r),
            "the reference is stereo, and both sides come through"
        );
    }

    #[test]
    fn the_cubic_passes_through_its_points_and_follows_a_sine() {
        assert_eq!(catmull_rom(0.3, -0.2, 0.9, 0.1, 0.0), -0.2);
        assert!((catmull_rom(0.3, -0.2, 0.9, 0.1, 1.0) - 0.9).abs() < 1e-6);
        assert!(
            (catmull_rom(1.0, 2.0, 3.0, 4.0, 0.25) - 2.25).abs() < 1e-6,
            "exact on a line"
        );
        // A 1 kHz sine at 48 kHz, read back between its samples, as the
        // resampler does for a 44.1 kHz device: within 0.1% of full scale.
        let f = |n: f32| (n * std::f32::consts::TAU * 1_000.0 / 48_000.0).sin();
        let mut worst = 0.0f32;
        for k in 0..480 {
            let t = (k as f32 * 0.37).fract();
            let n = k as f32;
            let got = catmull_rom(f(n - 1.0), f(n), f(n + 1.0), f(n + 2.0), t);
            worst = worst.max((got - f(n + t)).abs());
        }
        assert!(worst < 1e-3, "{worst}");
    }

    #[test]
    fn a_loop_on_a_44_1_khz_device_runs_at_the_right_speed() {
        // 44,100 output frames are one second: 48,000 source frames, so the
        // 12,000-frame reference has gone round four times, with no drift.
        let mut stream = QoaPlayer::new(REF_QOA, 44_100.0, true).unwrap();
        assert!((stream.step - 48_000.0 / 44_100.0).abs() < 1e-12);
        for _ in 0..44_100 {
            let (l, r) = stream.next();
            assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.01 && r.abs() <= 1.01);
        }
        assert!(
            stream.frac < 1e-6 || stream.frac > 1.0 - 1e-6,
            "{}",
            stream.frac
        );
    }

    #[test]
    fn a_one_shot_stops_at_its_end() {
        let mut stream = QoaPlayer::new(REF_QOA, 48_000.0, false).unwrap();
        for _ in 0..12_000 {
            stream.next();
        }
        assert!(stream.finished());
        assert_eq!(stream.next(), (0.0, 0.0));
    }

    #[test]
    fn bytes_that_are_not_qoa_are_refused() {
        assert!(QoaPlayer::new(b"not a qoa stream at all", 48_000.0, true).is_err());
    }
}
