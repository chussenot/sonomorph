//! The chip8 engine: ABC scores rendered by an 8-bit-style synthesizer,
//! stored as mu-law, played by a mixer.
//!
//! The pipeline has a build half and a run half:
//!
//! 1. **Render** (`render` feature, in a build script): [`render_file`]
//!    turns an ABC file into signed 16-bit mono samples, deterministically:
//!    the same file and rate always give the same samples.
//! 2. **Store**: [`mulaw::encode`] packs each sample into one byte, about
//!    13 bits of range with a noise floor that follows the signal down. The
//!    bytes are embedded with `include_bytes!`.
//! 3. **Play**: a [`Mixer`] reads the embedded bytes in place: one-shot
//!    effects with gain, pan and pitch; stingers that duck the music; one
//!    looping music track, crossfaded on change. [`Output`] (`output`
//!    feature) runs it on the default audio device.
//!
//! A program that works on scores rather than playing them (an editor, a
//! tracker) also gets, with `render`, [`render_file_u8`] (the engine's own
//! 8-bit output and its statistics) and [`Song`] (what a score declares);
//! with `jam`, [`JamSession`] (endless seeded variations on a score); and
//! with `midi`, [`export_midi`].
//!
//! What the engine understands, beyond standard ABC (held notes, dynamics,
//! chords, drum synthesis, automation, reverb, seamless loops), is listed
//! in `engine/chip8/PROVENANCE.md` and `docs/chip8.md`.

#[cfg(feature = "jam")]
mod jam;
#[cfg(feature = "midi")]
mod midi;
mod mixer;
pub mod mulaw;
#[cfg(feature = "output")]
mod output;
#[cfg(feature = "render")]
mod render;
#[cfg(feature = "render")]
pub mod song;

#[cfg(feature = "output")]
pub use crate::device::OutputError;
#[cfg(feature = "jam")]
pub use jam::{JamSection, JamSession, MAX_SECTION_SECONDS};
#[cfg(feature = "midi")]
pub use midi::{export_midi, MidiExportStats};
pub use mixer::{pan_gains, Command, Mixer, MixerConfig, Sound};
#[cfg(feature = "output")]
pub use output::Output;
#[cfg(feature = "render")]
pub use render::{render_file, render_file_u8, RenderError, RenderStats};
#[cfg(feature = "render")]
pub use song::Song;
