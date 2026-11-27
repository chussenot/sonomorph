//! A parsed song, for a program that shows or edits one (`render`
//! feature): what a score declares, read through views Sonomorph owns
//! (`engine/chip8/sonomorph_song.h`), never through the parser's own
//! layout.

use std::ffi::CStr;
use std::os::raw::c_int;
use std::path::Path;

use super::render::{existing_c_path, RenderError};

mod ffi {
    use std::os::raw::{c_char, c_double, c_int};

    /// The parser's song; opaque here on purpose.
    #[repr(C)]
    pub struct AbcMusic {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct SongInfo {
        pub title: [u8; 128],
        pub bpm: c_int,
        pub step_ms: c_int,
        pub swing_pct: c_int,
        pub instrument_count: c_int,
        pub pattern_count: c_int,
        pub arrangement_length: c_int,
        pub fx_bus_count: c_int,
        pub fx_bus_slots: c_int,
        pub voice_count: c_int,
    }

    #[repr(C)]
    pub struct InstrumentInfo {
        pub name: [u8; 32],
        pub preset: [u8; 32],
        pub amplitude: c_int,
        pub waveform: c_int,
        pub duty_cycle: c_int,
        pub attack_ms: c_int,
        pub decay_ms: c_int,
        pub sustain_level: c_int,
        pub release_ms: c_int,
        pub gate_percent: c_int,
        pub vibrato_cents: c_int,
        pub glide_ms: c_int,
        pub fx_bus: c_int,
    }

    #[repr(C)]
    pub struct PatternInfo {
        pub name: [u8; 32],
        pub length: c_int,
        pub defined: c_int,
    }

    #[repr(C)]
    pub struct FxBusInfo {
        pub enabled: c_int,
        pub delay_steps: c_int,
        pub delay_feedback: c_int,
        pub delay_mix: c_int,
        pub drive_amount: c_int,
        pub lowpass_amount: c_int,
        pub sidechain_amount: c_int,
        pub sidechain_release_ms: c_int,
        pub mix_percent: c_int,
        pub ladder_amount: c_int,
        pub ladder_cutoff: c_int,
        pub ladder_resonance: c_int,
    }

    #[repr(C)]
    pub struct VoiceInfo {
        pub name: [u8; 32],
        pub instrument: [u8; 32],
        pub amplitude: c_int,
        pub staccato: c_int,
        pub waveform: c_int,
        pub duty_cycle: c_int,
        pub attack_ms: c_int,
        pub decay_ms: c_int,
        pub sustain_level: c_int,
        pub release_ms: c_int,
        pub gate_percent: c_int,
        pub vibrato_cents: c_int,
        pub vibrato_rate: c_int,
        pub glide_ms: c_int,
        pub fx_bus: c_int,
        pub step_count: c_int,
    }

    extern "C" {
        pub fn sono_song_load(path: *const c_char) -> *mut AbcMusic;
        pub fn sono_song_free(music: *mut AbcMusic);
        pub fn sono_song_info(music: *const AbcMusic, out: *mut SongInfo);
        pub fn sono_song_instrument(
            music: *const AbcMusic,
            index: c_int,
            out: *mut InstrumentInfo,
        ) -> c_int;
        pub fn sono_song_pattern(
            music: *const AbcMusic,
            index: c_int,
            out: *mut PatternInfo,
        ) -> c_int;
        pub fn sono_song_fx_bus(music: *const AbcMusic, index: c_int, out: *mut FxBusInfo)
            -> c_int;
        pub fn sono_song_voice(music: *const AbcMusic, index: c_int, out: *mut VoiceInfo) -> c_int;
        pub fn sono_song_arrangement(music: *const AbcMusic, index: c_int) -> *const c_char;
        pub fn sono_song_voice_steps(music: *const AbcMusic, index: c_int) -> *const c_double;
        #[cfg(test)]
        pub fn sono_song_view_size(which: c_int) -> std::os::raw::c_long;
    }
}

/// A voice's oscillator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    /// A square wave (50% duty).
    Square,
    /// A pulse wave of the voice's `duty_cycle`.
    Pulse,
    /// A triangle wave.
    Triangle,
    /// Noise.
    Noise,
    /// A code this version does not name.
    Other(i32),
}

impl Waveform {
    fn from_code(code: c_int) -> Self {
        match code {
            0 => Self::Square,
            1 => Self::Pulse,
            2 => Self::Triangle,
            3 => Self::Noise,
            other => Self::Other(other),
        }
    }

    /// Its name in the ABC dialect (`wave=`), or `"other"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Pulse => "pulse",
            Self::Triangle => "triangle",
            Self::Noise => "noise",
            Self::Other(_) => "other",
        }
    }
}

/// A named instrument (`%%instrument`). Levels and times are the engine's
/// units: amplitude 0..=127, percentages 0..=100, times in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Instrument {
    /// Its name, as voices refer to it.
    pub name: String,
    /// The built-in preset it starts from, if any (empty when none).
    pub preset: String,
    /// Level, 0..=127.
    pub amplitude: i32,
    /// Oscillator.
    pub waveform: Waveform,
    /// Pulse duty, percent.
    pub duty_cycle: i32,
    /// Envelope attack, ms.
    pub attack_ms: i32,
    /// Envelope decay, ms.
    pub decay_ms: i32,
    /// Envelope sustain level, percent.
    pub sustain_level: i32,
    /// Envelope release, ms.
    pub release_ms: i32,
    /// How much of each step the note sounds, percent.
    pub gate_percent: i32,
    /// Vibrato depth, cents.
    pub vibrato_cents: i32,
    /// Portamento, ms.
    pub glide_ms: i32,
    /// FX bus, 0-based.
    pub fx_bus: i32,
}

/// A named pattern (`%%pattern`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// Its name, as the arrangement refers to it.
    pub name: String,
    /// Its length in steps, as written.
    pub length: i32,
    /// Whether it was defined (an arrangement can name an undefined one).
    pub defined: bool,
}

/// An FX bus (`%%effect`). Amounts are percentages unless named otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FxBus {
    /// Whether the score switched it on.
    pub enabled: bool,
    /// Echo delay, in steps.
    pub delay_steps: i32,
    /// Echo feedback.
    pub delay_feedback: i32,
    /// Echo level.
    pub delay_mix: i32,
    /// Saturation.
    pub drive_amount: i32,
    /// One-pole low-pass.
    pub lowpass_amount: i32,
    /// Sidechain ducking depth.
    pub sidechain_amount: i32,
    /// Sidechain release, ms (0: the engine's default).
    pub sidechain_release_ms: i32,
    /// The bus's output level (0: the engine's default, 100).
    pub mix_percent: i32,
    /// Ladder filter level (0: off).
    pub ladder_amount: i32,
    /// Ladder cutoff, percent of Nyquist.
    pub ladder_cutoff: i32,
    /// Ladder resonance.
    pub ladder_resonance: i32,
}

/// A voice (`V:`): its sound, and the frequency of each step.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    /// Its name.
    pub name: String,
    /// The instrument it plays (empty: its own settings).
    pub instrument: String,
    /// Level, 0..=127.
    pub amplitude: i32,
    /// Notes cut to 3/4 of their step instead of 9/10.
    pub staccato: bool,
    /// Oscillator.
    pub waveform: Waveform,
    /// Pulse duty, percent.
    pub duty_cycle: i32,
    /// Envelope attack, ms.
    pub attack_ms: i32,
    /// Envelope decay, ms.
    pub decay_ms: i32,
    /// Envelope sustain level, percent.
    pub sustain_level: i32,
    /// Envelope release, ms.
    pub release_ms: i32,
    /// How much of each step the note sounds, percent.
    pub gate_percent: i32,
    /// Vibrato depth, cents.
    pub vibrato_cents: i32,
    /// Vibrato rate, millihertz (0: the engine's default).
    pub vibrato_rate: i32,
    /// Portamento, ms.
    pub glide_ms: i32,
    /// FX bus, 0-based.
    pub fx_bus: i32,
    /// The frequency on each step, in Hz; 0 is a rest. A chord's upper
    /// notes are not listed.
    pub steps: Vec<f64>,
}

/// A score as the engine parsed it: its tempo, instruments, patterns,
/// arrangement, FX buses and voices. Read-only, and owned: nothing here
/// refers back to the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    /// `T:`.
    pub title: String,
    /// Beats per minute.
    pub bpm: i32,
    /// One default-length note, in ms.
    pub step_ms: i32,
    /// `%%swing`, percent (0: not set, the same as 50).
    pub swing_pct: i32,
    /// `%%instrument`s, in order.
    pub instruments: Vec<Instrument>,
    /// `%%pattern`s, in order.
    pub patterns: Vec<Pattern>,
    /// `%%arrangement`: pattern names, in playing order.
    pub arrangement: Vec<String>,
    /// The FX buses the score declared.
    pub fx_bus_count: usize,
    /// Every bus the engine has, declared or not (a voice may send to one
    /// the score did not declare).
    pub fx_buses: Vec<FxBus>,
    /// `V:`s, in order.
    pub voices: Vec<Voice>,
}

/// C text in a fixed buffer (declared `u8` here: the same size and
/// alignment as C's `char`, signed or not), trimmed.
fn text(buffer: &[u8]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..len]).trim().to_owned()
}

/// Frees the parsed song on every path out of `Song::load`.
struct Parsed(*mut ffi::AbcMusic);

impl Drop for Parsed {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `sono_song_load` and is freed once.
        unsafe { ffi::sono_song_free(self.0) };
    }
}

impl Song {
    /// Parses the ABC score at `path`.
    ///
    /// # Errors
    ///
    /// [`RenderError::Io`] or [`RenderError::Path`] when the file cannot be
    /// read or named to C, [`RenderError::Parse`] when the engine cannot
    /// parse it.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, RenderError> {
        let path = path.as_ref();
        let c_path = existing_c_path(path)?;
        // SAFETY: a valid NUL-terminated path; NULL or a heap song back.
        let raw = unsafe { ffi::sono_song_load(c_path.as_ptr()) };
        if raw.is_null() {
            return Err(RenderError::Parse(path.to_owned()));
        }
        let parsed = Parsed(raw);
        let music = parsed.0 as *const ffi::AbcMusic;

        // SAFETY (every call below): `music` is a live parsed song, and each
        // `out` is a valid, writable view of the type the C side fills.
        let mut info: ffi::SongInfo = unsafe { std::mem::zeroed() };
        unsafe { ffi::sono_song_info(music, &mut info) };

        let instruments = (0..info.instrument_count.max(0))
            .filter_map(|i| {
                let mut v: ffi::InstrumentInfo = unsafe { std::mem::zeroed() };
                (unsafe { ffi::sono_song_instrument(music, i, &mut v) } == 0).then(|| Instrument {
                    name: text(&v.name),
                    preset: text(&v.preset),
                    amplitude: v.amplitude,
                    waveform: Waveform::from_code(v.waveform),
                    duty_cycle: v.duty_cycle,
                    attack_ms: v.attack_ms,
                    decay_ms: v.decay_ms,
                    sustain_level: v.sustain_level,
                    release_ms: v.release_ms,
                    gate_percent: v.gate_percent,
                    vibrato_cents: v.vibrato_cents,
                    glide_ms: v.glide_ms,
                    fx_bus: v.fx_bus,
                })
            })
            .collect();

        let patterns = (0..info.pattern_count.max(0))
            .filter_map(|i| {
                let mut v: ffi::PatternInfo = unsafe { std::mem::zeroed() };
                (unsafe { ffi::sono_song_pattern(music, i, &mut v) } == 0).then(|| Pattern {
                    name: text(&v.name),
                    length: v.length,
                    defined: v.defined != 0,
                })
            })
            .collect();

        let arrangement = (0..info.arrangement_length.max(0))
            .filter_map(|i| {
                let name = unsafe { ffi::sono_song_arrangement(music, i) };
                // SAFETY: non-NULL names are NUL-terminated inside the song.
                (!name.is_null()).then(|| {
                    unsafe { CStr::from_ptr(name) }
                        .to_string_lossy()
                        .trim()
                        .to_owned()
                })
            })
            .collect();

        let fx_buses = (0..info.fx_bus_slots.max(0))
            .filter_map(|i| {
                let mut v: ffi::FxBusInfo = unsafe { std::mem::zeroed() };
                (unsafe { ffi::sono_song_fx_bus(music, i, &mut v) } == 0).then_some(FxBus {
                    enabled: v.enabled != 0,
                    delay_steps: v.delay_steps,
                    delay_feedback: v.delay_feedback,
                    delay_mix: v.delay_mix,
                    drive_amount: v.drive_amount,
                    lowpass_amount: v.lowpass_amount,
                    sidechain_amount: v.sidechain_amount,
                    sidechain_release_ms: v.sidechain_release_ms,
                    mix_percent: v.mix_percent,
                    ladder_amount: v.ladder_amount,
                    ladder_cutoff: v.ladder_cutoff,
                    ladder_resonance: v.ladder_resonance,
                })
            })
            .collect();

        let voices = (0..info.voice_count.max(0))
            .filter_map(|i| {
                let mut v: ffi::VoiceInfo = unsafe { std::mem::zeroed() };
                if unsafe { ffi::sono_song_voice(music, i, &mut v) } != 0 {
                    return None;
                }
                let steps_ptr = unsafe { ffi::sono_song_voice_steps(music, i) };
                let steps = if steps_ptr.is_null() {
                    Vec::new()
                } else {
                    // SAFETY: the voice holds at least `step_count` steps
                    // (the C side clamps it to the array).
                    unsafe { std::slice::from_raw_parts(steps_ptr, v.step_count.max(0) as usize) }
                        .to_vec()
                };
                Some(Voice {
                    name: text(&v.name),
                    instrument: text(&v.instrument),
                    amplitude: v.amplitude,
                    staccato: v.staccato != 0,
                    waveform: Waveform::from_code(v.waveform),
                    duty_cycle: v.duty_cycle,
                    attack_ms: v.attack_ms,
                    decay_ms: v.decay_ms,
                    sustain_level: v.sustain_level,
                    release_ms: v.release_ms,
                    gate_percent: v.gate_percent,
                    vibrato_cents: v.vibrato_cents,
                    vibrato_rate: v.vibrato_rate,
                    glide_ms: v.glide_ms,
                    fx_bus: v.fx_bus,
                    steps,
                })
            })
            .collect();

        Ok(Song {
            title: text(&info.title),
            bpm: info.bpm,
            step_ms: info.step_ms,
            swing_pct: info.swing_pct,
            instruments,
            patterns,
            arrangement,
            fx_bus_count: usize::try_from(info.fx_bus_count).unwrap_or(0),
            fx_buses,
            voices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;
    use std::mem::size_of;

    /// The mirrors above and the C views must agree, or `Song::load`
    /// reads past what C wrote. Same order as `sono_song_view_size`.
    #[test]
    fn the_rust_views_have_the_c_views_sizes() {
        let rust = [
            size_of::<ffi::SongInfo>(),
            size_of::<ffi::InstrumentInfo>(),
            size_of::<ffi::PatternInfo>(),
            size_of::<ffi::FxBusInfo>(),
            size_of::<ffi::VoiceInfo>(),
        ];
        for (which, rust) in rust.into_iter().enumerate() {
            // SAFETY: a pure function of its argument.
            let c = unsafe { ffi::sono_song_view_size(which as _) };
            assert_eq!(usize::try_from(c).ok(), Some(rust), "view {which}");
        }
        // SAFETY: as above.
        assert_eq!(unsafe { ffi::sono_song_view_size(5) }, -1);
    }
}
