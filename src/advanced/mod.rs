//! The advanced engine: orchestral and synthesized scores rendered by the
//! studio, stored as QOA, streamed by a mixer. **In development** on the
//! `advanced` branch.
//!
//! The pipeline has the same two halves as [`chip8`](crate::chip8):
//!
//! 1. **Render** (`render-advanced` feature, in a build script):
//!    `render_score` turns a `.score` (timed notes on named instruments,
//!    group buses, aux reverbs and delays, a master limiter) into 32-bit
//!    float stereo at 48 kHz, deterministically. Instruments come from the
//!    crate's library (`LIBRARY`): a sampled orchestra (VSCO 2 Community
//!    Edition, CC0) and an analog-style synth.
//! 2. **Store**: `Render::to_qoa` encodes music and stingers as QOA, about
//!    3.2 bits a sample. An effect renders at twice the mixer's rate and
//!    [`fold_effect`] brings it to mono at that rate, for mu-law
//!    ([`chip8::mulaw`](crate::chip8::mulaw)) and playback-rate pitch.
//! 3. **Play**: a [`Mixer`] decodes QOA as it plays ([`qoa`]) and
//!    resamples it with a cubic; mu-law effects play as in chip8. Music
//!    crossfades on an equal-power curve. [`Output`] (`output` feature)
//!    runs it on the default device.
//!
//! The `.score` language is described in `docs/advanced.md`.

mod effect;
mod mixer;
#[cfg(feature = "output")]
mod output;
pub mod qoa;
#[cfg(feature = "render-advanced")]
mod render;
mod stream;

pub use crate::chip8::{Command, MixerConfig};
#[cfg(feature = "output")]
pub use crate::device::OutputError;
pub use effect::fold_effect;
pub use mixer::{Clip, Mixer, Sound};
#[cfg(feature = "output")]
pub use output::Output;
#[cfg(feature = "render-advanced")]
pub use render::{render_score, render_score_with, Render, RenderError, LIBRARY};
