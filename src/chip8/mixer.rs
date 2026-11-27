//! The mixer: runs inside the audio callback, so it allocates nothing per
//! sample and blocks on nothing. It plays three kinds of sound:
//!
//! - **effects**: one-shot voices with a gain, a stereo position and a
//!   pitch (a playback rate, so one rendered note plays a whole scale);
//! - **stingers**: effects that also *duck* the music while they play;
//! - **music**: one looping track at a time, crossfaded on change. A new
//!   track waits for the sounds that hold the music ([`Sound::holds_music`]):
//!   it starts when the fanfare has finished, never under it.
//!
//! It is pure arithmetic over embedded mu-law bytes, tested without a
//! device; [`Output`](super::Output) only feeds it buffers.

use super::mulaw;

/// A sound the mixer can play: a handle to embedded samples.
///
/// A game implements this on its own sound enum, so commands name sounds
/// the way the game does and the mixer never looks anything up by name.
///
/// ```
/// use sonomorph::chip8::Sound;
///
/// #[derive(Clone, Copy, PartialEq)]
/// enum Cue {
///     Jump,
///     Fanfare,
/// }
///
/// impl Sound for Cue {
///     fn pcm(self) -> &'static [u8] {
///         match self {
///             // In a game: include_bytes! of what a build script rendered.
///             Cue::Jump => &[0xFF, 0x80, 0x00, 0x80, 0xFF],
///             Cue::Fanfare => &[0xFF; 4],
///         }
///     }
///
///     fn holds_music(self) -> bool {
///         self == Cue::Fanfare
///     }
/// }
/// ```
pub trait Sound: Copy + PartialEq + Send + 'static {
    /// The sound's samples: mono, 8-bit mu-law ([`mulaw`]), at the
    /// mixer's [`MixerConfig::sample_rate`]. Read in place, never copied.
    fn pcm(self) -> &'static [u8];

    /// Whether a music change waits until this sound has finished: a
    /// fanfare that ends a level, a sound that starts play. `false` unless
    /// overridden.
    fn holds_music(self) -> bool {
        false
    }
}

/// What a game asks the mixer to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command<S> {
    /// A one-shot sound.
    Play {
        /// The sound.
        sound: S,
        /// Its level, 1 as rendered.
        gain: f32,
        /// Its stereo position, -1 (left) to 1 (right).
        pan: f32,
        /// Its playback rate, 1 as rendered; 2 is an octave up and twice as
        /// short.
        pitch: f32,
        /// Above 0, the music is lowered for that many seconds: a stinger.
        duck: f32,
    },
    /// Switch the music to this loop, or to silence, crossfading. Asking
    /// for the track already playing changes nothing.
    Music(Option<S>),
    /// The music's level from the game's state: 1 in play, lower in a pause.
    MusicLevel(f32),
    /// The player's sound setting: effects on or off, music on or off.
    Enable {
        /// Whether effects and stingers sound. Turning them off silences
        /// the ones playing.
        effects: bool,
        /// Whether music sounds. It fades rather than stopping.
        music: bool,
    },
    /// Fade out, in a moment, the sounds that hold the music: a stinger the
    /// player has already moved past.
    CutHeld,
}

/// How a mixer sounds. The defaults are the levels BRYX was mixed at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerConfig {
    /// The rate every [`Sound::pcm`] was rendered at, in Hz.
    pub sample_rate: f32,
    /// Voices that can sound at once; past this the oldest is stolen.
    pub voices: usize,
    /// The music's level under the effects.
    pub music_gain: f32,
    /// The music's level, relative, while a stinger plays.
    pub duck_level: f32,
    /// How long one music loop takes to crossfade into the next, and to
    /// fade in or out, in seconds.
    pub crossfade_secs: f32,
    /// How fast ducking releases once the stinger is done, in seconds.
    pub duck_release_secs: f32,
    /// Overall level, into the soft limiter (`tanh`).
    pub master: f32,
    /// How quickly [`Command::CutHeld`] silences a held sound: fast, but
    /// not a click. In seconds.
    pub cut_secs: f32,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            sample_rate: 22_050.0,
            voices: 32,
            music_gain: 0.52,
            duck_level: 0.25,
            crossfade_secs: 1.2,
            duck_release_secs: 0.5,
            master: 1.5,
            cut_secs: 0.08,
        }
    }
}

struct Voice {
    data: &'static [u8],
    pos: f64,
    step: f64,
    left: f32,
    right: f32,
    /// New music waits while this voice sounds.
    holds_music: bool,
    /// 1 while playing; falls to 0 at `fade_step` a frame once cut.
    fade: f32,
    fade_step: f32,
}

struct Loop<S> {
    sound: S,
    data: &'static [u8],
    pos: f64,
    /// 0..1 crossfade level, moving toward `target`.
    level: f32,
    target: f32,
}

/// Effects, stingers and one music loop, mixed into a stereo stream.
///
/// Feed it [`Command`]s with [`apply`](Self::apply) and pull samples with
/// [`render`](Self::render). Neither allocates after construction.
pub struct Mixer<S: Sound> {
    config: MixerConfig,
    /// Sound samples per output frame at pitch 1.
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

/// Equal-power stereo gains for a pan in -1..1: a sound keeps its loudness
/// as it moves across the field.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Linear interpolation into `data` (mu-law bytes, read in place) at
/// fractional position `pos`. Past the end is silence.
fn sample_at(data: &[u8], pos: f64) -> f32 {
    let i = pos as usize;
    let frac = (pos - i as f64) as f32;
    let a = data.get(i).map_or(0.0, |&b| mulaw::sample(b));
    let b = data.get(i + 1).map_or(0.0, |&b| mulaw::sample(b));
    a + (b - a) * frac
}

impl<S: Sound> Mixer<S> {
    /// A mixer with the default [`MixerConfig`], writing at `output_rate`
    /// Hz (the device's rate).
    pub fn new(output_rate: u32) -> Self {
        Self::with_config(MixerConfig::default(), output_rate)
    }

    /// A mixer with `config`, writing at `output_rate` Hz.
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
        let current = self.music_track();
        if current == next {
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
            self.music.insert(
                0,
                Loop {
                    sound,
                    data: sound.pcm(),
                    pos: 0.0,
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
                if self.voices.len() >= self.config.voices {
                    self.voices.remove(0);
                }
                let (l, r) = pan_gains(pan);
                self.voices.push(Voice {
                    data: sound.pcm(),
                    pos: 0.0,
                    step: self.rate_ratio * f64::from(pitch.max(0.05)),
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
                let s = sample_at(voice.data, voice.pos) * voice.fade;
                left += s * voice.left;
                right += s * voice.right;
                voice.pos += voice.step;
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
                if track.data.is_empty() {
                    continue;
                }
                let s = sample_at(track.data, track.pos) * track.level * music_gain;
                left += s;
                right += s;
                track.pos += self.rate_ratio;
                if track.pos >= track.data.len() as f64 {
                    track.pos -= track.data.len() as f64;
                }
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
        self.voices
            .retain(|v| v.pos < v.data.len() as f64 && v.fade > 0.0);
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
    use std::sync::OnceLock;

    const RATE: u32 = 22_050;

    /// Test sounds: synthetic tones, mu-law encoded once.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Tone {
        /// 0.2 s, an effect.
        Blip,
        /// 1.5 s, holds the music.
        Fanfare,
        /// 2 s loops.
        LoopA,
        LoopB,
    }

    fn tone(secs: f32, hz: f32) -> &'static [u8] {
        let n = (secs * RATE as f32) as usize;
        let samples: Vec<u8> = (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let s = (t * hz * std::f32::consts::TAU).sin() * 0.8;
                mulaw::encode((s * 32_767.0) as i16)
            })
            .collect();
        Box::leak(samples.into_boxed_slice())
    }

    impl Sound for Tone {
        fn pcm(self) -> &'static [u8] {
            static BANK: OnceLock<[&'static [u8]; 4]> = OnceLock::new();
            let bank = BANK.get_or_init(|| {
                [
                    tone(0.2, 880.0),
                    tone(1.5, 440.0),
                    tone(2.0, 220.0),
                    tone(2.0, 330.0),
                ]
            });
            bank[self as usize]
        }

        fn holds_music(self) -> bool {
            self == Tone::Fanfare
        }
    }

    fn play(sound: Tone) -> Command<Tone> {
        Command::Play {
            sound,
            gain: 1.0,
            pan: 0.0,
            pitch: 1.0,
            duck: 0.0,
        }
    }

    #[test]
    fn equal_power_pan_keeps_loudness() {
        for pan in [-1.0, -0.5, 0.0, 0.3, 1.0] {
            let (l, r) = pan_gains(pan);
            assert!((l * l + r * r - 1.0).abs() < 1e-5, "pan {pan}");
        }
        let (l, r) = pan_gains(-1.0);
        assert!(l > 0.99 && r < 0.01, "hard left");
    }

    #[test]
    fn interpolation_reads_between_samples() {
        let data = [mulaw::SILENCE, 0x80]; // silence, then full scale
        assert!(mulaw::sample(0x80) > 0.97);
        assert!((sample_at(&data, 0.5) - mulaw::sample(0x80) / 2.0).abs() < 1e-6);
        assert_eq!(sample_at(&data, 5.0), 0.0, "past the end is silence");
    }

    #[test]
    fn an_effect_plays_through_and_frees_its_voice() {
        let mut mixer = Mixer::new(44_100);
        mixer.apply(play(Tone::Blip));
        assert_eq!(mixer.voice_count(), 1);
        let mut out = vec![0.0f32; 2 * 4_096];
        mixer.render(&mut out, 2);
        assert!(out.iter().any(|s| s.abs() > 0.05), "audible");
        assert!(out.iter().all(|s| s.abs() <= 1.0), "limited");
        for _ in 0..40 {
            mixer.render(&mut out, 2);
        }
        assert_eq!(mixer.voice_count(), 0, "a finished voice is freed");
    }

    #[test]
    fn a_doubled_pitch_finishes_in_half_the_time() {
        let frames_until_silent = |pitch: f32| {
            let mut mixer = Mixer::new(RATE);
            mixer.apply(Command::Play {
                sound: Tone::Blip,
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
            frames
        };
        let normal = frames_until_silent(1.0) as f32;
        let double = frames_until_silent(2.0) as f32;
        assert!((double / normal - 0.5).abs() < 0.05, "{double} vs {normal}");
    }

    #[test]
    fn voices_are_capped_by_stealing_the_oldest() {
        let config = MixerConfig::default();
        let mut mixer = Mixer::new(44_100);
        for _ in 0..(config.voices + 10) {
            mixer.apply(play(Tone::Blip));
        }
        assert_eq!(mixer.voice_count(), config.voices);
    }

    #[test]
    fn music_switches_by_crossfade_and_a_repeat_request_is_ignored() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        assert_eq!(mixer.music.len(), 1, "same track: no restart");
        let mut out = vec![0.0f32; 2 * RATE as usize];
        mixer.render(&mut out, 2);
        mixer.apply(Command::Music(Some(Tone::LoopB)));
        assert_eq!(mixer.music_track(), Some(Tone::LoopB));
        assert_eq!(mixer.music.len(), 2, "the old loop fades out");
        mixer.render(&mut out, 2);
        mixer.render(&mut out, 2);
        assert_eq!(mixer.music.len(), 1, "and is gone after the crossfade");
        mixer.apply(Command::Music(None));
        assert_eq!(mixer.music_track(), None);
    }

    #[test]
    fn a_loop_wraps_without_stopping() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        let mut out = vec![0.0f32; 2 * RATE as usize];
        for _ in 0..5 {
            mixer.render(&mut out, 2);
        }
        assert_eq!(mixer.music_track(), Some(Tone::LoopA), "still playing");
        assert!(out.iter().any(|s| s.abs() > 0.05), "and still sounding");
    }

    #[test]
    fn new_music_waits_for_the_fanfare_to_finish() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        let mut out = vec![0.0f32; 2 * 2_205];
        mixer.render(&mut out, 2);
        mixer.apply(Command::Play {
            sound: Tone::Fanfare,
            gain: 1.0,
            pan: 0.0,
            pitch: 1.0,
            duck: 2.7,
        });
        mixer.apply(Command::Music(Some(Tone::LoopB)));
        assert_eq!(
            mixer.music_track(),
            None,
            "the old track fades, the new one waits"
        );
        let fanfare = Tone::Fanfare.pcm().len() as f64 / f64::from(RATE);
        let mut played = 0.0;
        while mixer.music_track().is_none() {
            mixer.render(&mut out, 2);
            played += 0.1;
            assert!(played < fanfare + 0.5, "the music never came back");
        }
        assert!(
            played + 0.1 >= fanfare,
            "started {played:.1}s in, under a {fanfare:.1}s fanfare"
        );
        assert_eq!(mixer.music_track(), Some(Tone::LoopB));
        assert!(mixer.music[0].pos < 2.0 * 2_205.0, "and from its start");
    }

    #[test]
    fn a_stinger_ducks_the_music_then_lets_it_back() {
        let level = |out: &[f32]| out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        let mut out = vec![0.0f32; 2 * RATE as usize];
        mixer.render(&mut out, 2);
        mixer.render(&mut out, 2);
        let open = level(&out);
        // A silent stinger: only the duck is heard.
        mixer.apply(Command::Play {
            sound: Tone::Blip,
            gain: 0.0,
            pan: 0.0,
            pitch: 1.0,
            duck: 1.0,
        });
        let mut short = vec![0.0f32; 2 * 2_205];
        mixer.render(&mut short, 2);
        mixer.render(&mut short, 2);
        let ducked = level(&short);
        assert!(ducked < 0.5 * open, "ducked {ducked:.2} of {open:.2}");
        mixer.render(&mut out, 2);
        mixer.render(&mut out, 2);
        assert!(level(&out) > 0.9 * open, "released");
    }

    #[test]
    fn cutting_held_sounds_fades_them_out_and_frees_the_music() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(play(Tone::Fanfare));
        mixer.apply(play(Tone::Blip));
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        assert_eq!(mixer.music_track(), None, "held under the fanfare");
        mixer.apply(Command::CutHeld);
        let mut out = vec![0.0f32; 2 * 4_410];
        mixer.render(&mut out, 2);
        assert_eq!(mixer.voice_count(), 0, "cut in 80 ms; the blip ran out too");
        assert_eq!(mixer.music_track(), Some(Tone::LoopA));
    }

    #[test]
    fn effects_off_silences_effects_and_keeps_music() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Enable {
            effects: false,
            music: true,
        });
        mixer.apply(play(Tone::Blip));
        assert_eq!(mixer.voice_count(), 0);
        mixer.apply(Command::Music(Some(Tone::LoopA)));
        let mut out = vec![0.0f32; 2 * RATE as usize];
        mixer.render(&mut out, 2);
        assert!(out.iter().any(|s| s.abs() > 0.01), "music still plays");
    }

    #[test]
    fn mono_and_wide_layouts_are_filled() {
        let mut mixer = Mixer::new(RATE);
        mixer.apply(Command::Play {
            sound: Tone::Blip,
            gain: 1.0,
            pan: -1.0,
            pitch: 1.0,
            duck: 0.0,
        });
        let mut mono = vec![0.0f32; 512];
        mixer.render(&mut mono, 1);
        assert!(mono.iter().any(|s| s.abs() > 0.01), "mono gets the sum");
        let mut quad = vec![0.0f32; 4 * 512];
        mixer.render(&mut quad, 4);
        let (left, right, rest) = quad.chunks(4).fold((0.0f32, 0.0f32, 0.0f32), |acc, f| {
            (
                acc.0 + f[0].abs(),
                acc.1 + f[1].abs(),
                acc.2 + f[2].abs() + f[3].abs(),
            )
        });
        assert!(left > 1.0 && right < 1e-3, "panned hard left");
        assert_eq!(rest, 0.0, "channels past two are silent");
    }

    #[test]
    fn the_config_sets_the_voice_limit() {
        let config = MixerConfig {
            voices: 3,
            ..MixerConfig::default()
        };
        let mut mixer = Mixer::<Tone>::with_config(config, RATE);
        for _ in 0..10 {
            mixer.apply(play(Tone::Blip));
        }
        assert_eq!(mixer.voice_count(), 3);
    }
}
