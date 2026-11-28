//! The advanced mixer: the chip8 mixer's model (effects, stingers that duck
//! the music, one music loop that waits for the fanfare) over two kinds of
//! clip: mu-law mono effects, and QOA stereo renders decoded as they play.
//! Music crossfades on an equal-power curve, so two unrelated tracks keep
//! their loudness through the change.
//!
//! It runs inside the audio callback: no allocation and no lock on its
//! path (the `no_alloc` integration test counts allocator calls), and a
//! fixed amount of decoding per frame. Ported from BRYX's mixer on its
//! orchestral-score branch.

use crate::chip8::{mulaw, pan_gains, Command, MixerConfig};

use super::stream::QoaPlayer;

/// What a sound is made of: embedded bytes, read in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clip {
    /// Mono 8-bit mu-law at [`MixerConfig::sample_rate`]: an effect,
    /// pitched by playback rate (chip8 renders, or studio renders folded
    /// down to mono).
    MuLaw(&'static [u8]),
    /// A QOA stream at its own rate, mono or stereo
    /// ([`Render::to_qoa`](super::Render::to_qoa)): music and stingers.
    Qoa(&'static [u8]),
}

/// A sound the advanced mixer can play.
///
/// ```
/// use sonomorph::advanced::{Clip, Sound};
///
/// #[derive(Clone, Copy, PartialEq)]
/// enum Cue {
///     Hit,
///     Theme,
/// }
///
/// impl Sound for Cue {
///     fn clip(self) -> Clip {
///         match self {
///             // In a game: include_bytes! of what a build script rendered.
///             Cue::Hit => Clip::MuLaw(&[0xFF, 0x80, 0x00, 0xFF]),
///             Cue::Theme => Clip::Qoa(&[]),
///         }
///     }
/// }
/// ```
pub trait Sound: Copy + PartialEq + Send + 'static {
    /// The bytes this sound plays.
    fn clip(self) -> Clip;

    /// Whether a music change waits until this sound has finished.
    fn holds_music(self) -> bool {
        false
    }
}

/// What an effect voice plays.
#[allow(clippy::large_enum_variant)] // one player inline beats a box per voice
enum Source {
    /// Mu-law mono, read in place at `step` clip samples per output frame.
    MuLaw {
        data: &'static [u8],
        pos: f64,
        step: f64,
    },
    /// QOA, played once.
    Qoa(QoaPlayer),
}

impl Source {
    fn finished(&self) -> bool {
        match self {
            Source::MuLaw { data, pos, .. } => *pos >= data.len() as f64,
            Source::Qoa(player) => player.finished(),
        }
    }
}

struct Voice {
    source: Source,
    left: f32,
    right: f32,
    /// New music waits while this voice sounds.
    holds_music: bool,
    /// 1 while playing; falls to 0 at `fade_step` a frame once cut.
    fade: f32,
    fade_step: f32,
}

/// What a music loop plays.
#[allow(clippy::large_enum_variant)]
enum LoopSource {
    MuLaw {
        data: &'static [u8],
        pos: f64,
    },
    Qoa(QoaPlayer),
    /// A clip that does not decode plays as silence and still crossfades.
    Silent,
}

struct Loop<S> {
    sound: S,
    source: LoopSource,
    /// 0..1 crossfade position, moving toward `target`; the gain is its
    /// equal-power curve ([`crossfade_gain`]).
    level: f32,
    target: f32,
}

/// Effects, stingers and one music loop, mixed into a stereo stream.
pub struct Mixer<S: Sound> {
    config: MixerConfig,
    /// Mu-law samples per output frame at pitch 1.
    rate_ratio: f64,
    output_rate: f32,
    voices: Vec<Voice>,
    /// The playing loop first; at most one fading one after it.
    music: Vec<Loop<S>>,
    music_level: f32,
    music_level_now: f32,
    duck_secs: f32,
    duck_now: f32,
    effects_on: bool,
    music_on: bool,
    /// A music change waiting for the held sounds to finish.
    pending: Option<Option<S>>,
}

/// Linear interpolation into mu-law `data` at fractional position `pos`.
/// Past the end is silence.
fn sample_at(data: &[u8], pos: f64) -> f32 {
    let i = pos as usize;
    let frac = (pos - i as f64) as f32;
    let a = data.get(i).map_or(0.0, |&b| mulaw::sample(b));
    let b = data.get(i + 1).map_or(0.0, |&b| mulaw::sample(b));
    a + (b - a) * frac
}

/// The gain at crossfade position `level` (0..1): a quarter sine, so two
/// unrelated tracks crossing keep the loudness (cos² + sin² = 1) where a
/// linear fade dips 3 dB in the middle.
#[inline]
fn crossfade_gain(level: f32) -> f32 {
    (level.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin()
}

impl<S: Sound> Mixer<S> {
    /// A mixer with the default [`MixerConfig`], writing at `output_rate`.
    pub fn new(output_rate: u32) -> Self {
        Self::with_config(MixerConfig::default(), output_rate)
    }

    /// A mixer with `config`, writing at `output_rate` Hz.
    /// [`MixerConfig::sample_rate`] is the rate of the [`Clip::MuLaw`]
    /// clips; a QOA clip carries its own.
    pub fn with_config(config: MixerConfig, output_rate: u32) -> Self {
        let voices = config.voices.max(1);
        Self {
            config: MixerConfig { voices, ..config },
            rate_ratio: f64::from(config.sample_rate) / f64::from(output_rate.max(1)),
            output_rate: output_rate.max(1) as f32,
            voices: Vec::with_capacity(voices),
            music: Vec::with_capacity(2),
            music_level: 1.0,
            music_level_now: 1.0,
            duck_secs: 0.0,
            duck_now: 1.0,
            effects_on: true,
            music_on: true,
            pending: None,
        }
    }

    /// The configuration this mixer runs with.
    pub fn config(&self) -> &MixerConfig {
        &self.config
    }

    /// Whether a sound that holds the music is still sounding.
    fn music_held(&self) -> bool {
        self.voices.iter().any(|v| v.holds_music && v.fade > 0.0)
    }

    /// Crossfade to `next` now.
    fn switch_music(&mut self, next: Option<S>) {
        if self.music_track() == next {
            return;
        }
        for playing in &mut self.music {
            playing.target = 0.0;
        }
        self.music.retain(|l| l.level > 0.0);
        if self.music.len() > 1 {
            // Never more than one loop fading out: drop the quietest.
            self.music.truncate(1);
        }
        if let Some(sound) = next {
            let source = match sound.clip() {
                Clip::MuLaw(data) => LoopSource::MuLaw { data, pos: 0.0 },
                Clip::Qoa(bytes) => QoaPlayer::new(bytes, self.output_rate, true)
                    .map_or(LoopSource::Silent, LoopSource::Qoa),
            };
            self.music.insert(
                0,
                Loop {
                    sound,
                    source,
                    level: 0.0,
                    target: 1.0,
                },
            );
        }
    }

    /// Carries out one command.
    pub fn apply(&mut self, command: Command<S>) {
        match command {
            Command::Play {
                sound,
                gain,
                pan,
                pitch,
                duck,
            } => {
                if !self.effects_on {
                    return;
                }
                if duck > 0.0 {
                    self.duck_secs = self.duck_secs.max(duck);
                }
                let pitch = f64::from(pitch.max(0.05));
                let source = match sound.clip() {
                    Clip::MuLaw(data) => Source::MuLaw {
                        data,
                        pos: 0.0,
                        step: self.rate_ratio * pitch,
                    },
                    Clip::Qoa(bytes) => match QoaPlayer::new(bytes, self.output_rate, false) {
                        Ok(mut player) => {
                            player.step *= pitch;
                            Source::Qoa(player)
                        }
                        // Not QOA: nothing to play.
                        Err(_) => return,
                    },
                };
                if self.voices.len() >= self.config.voices {
                    self.voices.remove(0);
                }
                let (l, r) = pan_gains(pan);
                self.voices.push(Voice {
                    source,
                    left: gain * l,
                    right: gain * r,
                    holds_music: sound.holds_music(),
                    fade: 1.0,
                    fade_step: 0.0,
                });
            }
            Command::Music(next) => {
                if next.is_some() && self.music_held() {
                    // The old track still fades out now; the new one waits
                    // for the held sounds to finish (`render`).
                    self.switch_music(None);
                    self.pending = Some(next);
                } else {
                    self.pending = None;
                    self.switch_music(next);
                }
            }
            Command::CutHeld => {
                let step = 1.0 / (self.config.cut_secs * self.output_rate);
                for voice in self.voices.iter_mut().filter(|v| v.holds_music) {
                    voice.fade_step = step;
                }
            }
            Command::MusicLevel(level) => self.music_level = level.clamp(0.0, 1.0),
            Command::Enable { effects, music } => {
                self.effects_on = effects;
                self.music_on = music;
                if !effects {
                    self.voices.clear();
                }
            }
        }
    }

    /// Fills `out` (interleaved, `channels` per frame) with the mix. One
    /// channel gets both sides summed; past two, the rest are silent.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let dt = 1.0 / self.output_rate;
        let fade_step = dt / self.config.crossfade_secs;
        let MixerConfig {
            music_gain,
            duck_level,
            duck_release_secs,
            master,
            ..
        } = self.config;
        for frame in out.chunks_mut(channels) {
            let (mut left, mut right) = (0.0f32, 0.0f32);

            for voice in &mut self.voices {
                let (l, r) = match &mut voice.source {
                    Source::MuLaw { data, pos, step } => {
                        let s = sample_at(data, *pos);
                        *pos += *step;
                        (s, s)
                    }
                    Source::Qoa(player) => player.next(),
                };
                left += l * voice.fade * voice.left;
                right += r * voice.fade * voice.right;
                voice.fade = (voice.fade - voice.fade_step).max(0.0);
            }

            // Music: level eases toward its target (pause, mode), the duck
            // holds while a stinger plays and releases after.
            let music_target = if self.music_on { self.music_level } else { 0.0 };
            self.music_level_now += (music_target - self.music_level_now) * (dt * 6.0).min(1.0);
            if self.duck_secs > 0.0 {
                self.duck_secs -= dt;
                self.duck_now += (duck_level - self.duck_now) * (dt * 30.0).min(1.0);
            } else {
                self.duck_now += (1.0 - self.duck_now) * (dt / duck_release_secs * 4.0).min(1.0);
            }
            let music_gain = music_gain * self.music_level_now * self.duck_now;
            for track in &mut self.music {
                track.level = if track.level < track.target {
                    (track.level + fade_step).min(track.target)
                } else {
                    (track.level - fade_step).max(track.target)
                };
                let gain = crossfade_gain(track.level) * music_gain;
                let (l, r) = match &mut track.source {
                    LoopSource::Qoa(player) => player.next(),
                    LoopSource::MuLaw { data, pos } => {
                        if data.is_empty() {
                            (0.0, 0.0)
                        } else {
                            let s = sample_at(data, *pos);
                            *pos += self.rate_ratio;
                            if *pos >= data.len() as f64 {
                                *pos -= data.len() as f64;
                            }
                            (s, s)
                        }
                    }
                    LoopSource::Silent => (0.0, 0.0),
                };
                left += l * gain;
                right += r * gain;
            }

            // Soft limit: transparent at normal levels, never a hard clip
            // when a dozen sounds start on the same frame.
            let (l, r) = ((left * master).tanh(), (right * master).tanh());
            match frame {
                [mono] => *mono = (l + r) * 0.5,
                [a, b, rest @ ..] => {
                    *a = l;
                    *b = r;
                    rest.fill(0.0);
                }
                [] => {}
            }
        }
        self.voices.retain(|v| !v.source.finished() && v.fade > 0.0);
        self.music.retain(|t| t.target > 0.0 || t.level > 0.0);
        if self.pending.is_some() && !self.music_held() {
            if let Some(next) = self.pending.take() {
                self.switch_music(next);
            }
        }
    }

    /// Voices sounding now (effects and stingers).
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// The loop the music is heading to: the one playing or fading in.
    pub fn music_track(&self) -> Option<S> {
        self.music
            .first()
            .filter(|l| l.target > 0.0)
            .map(|l| l.sound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advanced::qoa::QoaStream;
    use std::sync::OnceLock;

    /// 12,000 stereo frames at 48 kHz (a quarter second), made by the
    /// reference encoder.
    const REF_QOA: &[u8] = include_bytes!("testdata/ref.qoa");
    const RATE: u32 = 44_100;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Cue {
        /// 0.2 s of mu-law tone.
        Blip,
        /// The QOA reference, as a stinger that holds the music.
        Stinger,
        /// The QOA reference as a loop.
        Track,
        /// A mu-law loop.
        Tone,
        /// Bytes that are not QOA.
        Broken,
    }

    fn tone(secs: f32, hz: f32) -> &'static [u8] {
        let rate = MixerConfig::default().sample_rate;
        let n = (secs * rate) as usize;
        let bytes: Vec<u8> = (0..n)
            .map(|i| {
                let s = (i as f32 / rate * hz * std::f32::consts::TAU).sin() * 0.8;
                mulaw::encode((s * 32_767.0) as i16)
            })
            .collect();
        Box::leak(bytes.into_boxed_slice())
    }

    impl Sound for Cue {
        fn clip(self) -> Clip {
            static TONES: OnceLock<(&'static [u8], &'static [u8])> = OnceLock::new();
            let (blip, loop_tone) = *TONES.get_or_init(|| (tone(0.2, 880.0), tone(2.0, 220.0)));
            match self {
                Cue::Blip => Clip::MuLaw(blip),
                Cue::Tone => Clip::MuLaw(loop_tone),
                Cue::Stinger | Cue::Track => Clip::Qoa(REF_QOA),
                Cue::Broken => Clip::Qoa(b"nope"),
            }
        }

        fn holds_music(self) -> bool {
            self == Cue::Stinger
        }
    }

    fn play(sound: Cue) -> Command<Cue> {
        Command::Play {
            sound,
            gain: 1.0,
            pan: 0.0,
            pitch: 1.0,
            duck: 0.0,
        }
    }

    #[test]
    fn the_crossfade_keeps_the_loudness() {
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            let power = crossfade_gain(x).powi(2) + crossfade_gain(1.0 - x).powi(2);
            assert!((power - 1.0).abs() < 1e-5, "at {x}: {power}");
        }
        assert_eq!(crossfade_gain(0.0), 0.0);
        assert!((crossfade_gain(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_qoa_one_shot_plays_once_and_frees_its_voice() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(play(Cue::Stinger));
        // A quarter second at 48 kHz.
        let mut out = vec![0.0f32; 2 * 4_410];
        mixer.render(&mut out, 2);
        assert!(out.iter().any(|s| s.abs() > 0.01), "audible");
        assert!(mixer.music_held(), "and holding the music while it sounds");
        for _ in 0..2 {
            mixer.render(&mut out, 2);
        }
        assert_eq!(mixer.voice_count(), 0, "gone after its last frame");
        assert!(!mixer.music_held());
        mixer.render(&mut out, 2);
        assert!(out.iter().all(|s| *s == 0.0), "and silent after it");
    }

    #[test]
    fn a_qoa_track_plays_in_stereo_and_keeps_looping() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Cue::Track)));
        let mut out = vec![0.0f32; 2 * RATE as usize];
        for _ in 0..3 {
            mixer.render(&mut out, 2);
        }
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(out.iter().any(|s| s.abs() > 0.01), "audible");
        assert!(
            out.chunks(2).any(|f| (f[0] - f[1]).abs() > 1e-3),
            "left and right differ"
        );
        assert_eq!(mixer.music_track(), Some(Cue::Track));
    }

    #[test]
    fn a_mulaw_loop_also_plays_as_music() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Cue::Tone)));
        let mut out = vec![0.0f32; 2 * RATE as usize];
        for _ in 0..4 {
            mixer.render(&mut out, 2);
        }
        assert!(out.iter().any(|s| s.abs() > 0.05), "still sounding");
    }

    #[test]
    fn a_pitched_qoa_one_shot_finishes_in_half_the_time() {
        let frames_until_silent = |pitch: f32| {
            let mut mixer = Mixer::new(48_000);
            mixer.apply(Command::Play {
                sound: Cue::Stinger,
                gain: 1.0,
                pan: 0.0,
                pitch,
                duck: 0.0,
            });
            let mut frames = 0;
            let mut out = [0.0f32; 2 * 64];
            while mixer.voice_count() > 0 {
                mixer.render(&mut out, 2);
                frames += 64;
            }
            frames as f32
        };
        let ratio = frames_until_silent(2.0) / frames_until_silent(1.0);
        assert!((ratio - 0.5).abs() < 0.02, "{ratio}");
    }

    #[test]
    fn a_clip_that_is_not_qoa_is_skipped_and_music_crossfades_through_it() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(play(Cue::Broken));
        assert_eq!(mixer.voice_count(), 0);
        mixer.apply(Command::Music(Some(Cue::Broken)));
        let mut out = vec![0.0f32; 2 * 4_410];
        mixer.render(&mut out, 2);
        assert!(out.iter().all(|s| *s == 0.0));
        assert_eq!(mixer.music_track(), Some(Cue::Broken));
        mixer.apply(Command::Music(Some(Cue::Track)));
        assert_eq!(mixer.music_track(), Some(Cue::Track));
    }

    #[test]
    fn new_music_waits_for_the_stinger_then_starts_from_its_start() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Cue::Tone)));
        let mut out = vec![0.0f32; 2 * 441];
        mixer.render(&mut out, 2);
        mixer.apply(Command::Play {
            sound: Cue::Stinger,
            gain: 1.0,
            pan: 0.0,
            pitch: 1.0,
            duck: 0.5,
        });
        mixer.apply(Command::Music(Some(Cue::Track)));
        assert_eq!(mixer.music_track(), None, "waits under the stinger");
        let stinger = {
            let q = QoaStream::new(REF_QOA).unwrap();
            f64::from(q.len()) / f64::from(q.sample_rate())
        };
        let mut played = 0.0;
        while mixer.music_track().is_none() {
            mixer.render(&mut out, 2);
            played += 0.01;
            assert!(played < stinger + 0.1, "the music never came back");
        }
        assert!(played + 0.01 >= stinger, "started {played:.2}s in");
        assert_eq!(mixer.music_track(), Some(Cue::Track));
    }

    #[test]
    fn cutting_held_sounds_frees_the_music() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(play(Cue::Stinger));
        mixer.apply(Command::Music(Some(Cue::Track)));
        assert_eq!(mixer.music_track(), None);
        mixer.apply(Command::CutHeld);
        let mut out = vec![0.0f32; 2 * 4_410];
        mixer.render(&mut out, 2);
        assert_eq!(mixer.voice_count(), 0, "cut in 80 ms");
        assert_eq!(mixer.music_track(), Some(Cue::Track));
    }

    #[test]
    fn voices_are_capped_and_effects_off_keeps_the_music() {
        let mut mixer = Mixer::new(RATE);
        for _ in 0..40 {
            mixer.apply(play(Cue::Blip));
        }
        assert_eq!(mixer.voice_count(), MixerConfig::default().voices);
        mixer.apply(Command::Enable {
            effects: false,
            music: true,
        });
        assert_eq!(mixer.voice_count(), 0);
        mixer.apply(play(Cue::Blip));
        assert_eq!(mixer.voice_count(), 0);
        mixer.apply(Command::Music(Some(Cue::Track)));
        let mut out = vec![0.0f32; 2 * RATE as usize];
        mixer.render(&mut out, 2);
        assert!(out.iter().any(|s| s.abs() > 0.01), "music still plays");
    }
}
