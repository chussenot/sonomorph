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
//! What the engine understands, beyond standard ABC (held notes, dynamics,
//! chords, drum synthesis, automation, reverb, seamless loops), is listed
//! in `engine/chip8/PROVENANCE.md` and `docs/chip8.md`.

mod mixer;
pub mod mulaw;
#[cfg(feature = "output")]
mod output;
#[cfg(feature = "render")]
mod render;

#[cfg(feature = "output")]
pub use crate::device::OutputError;
pub use mixer::{pan_gains, Command, Mixer, MixerConfig, Sound};
#[cfg(feature = "output")]
pub use output::Output;
#[cfg(feature = "render")]
pub use render::{render_file, RenderError};
