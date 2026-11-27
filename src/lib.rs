//! Sonomorph: game audio, split where a game needs it split.
//!
//! A game's music and effects are written as scores, rendered to samples
//! **at build time**, embedded in the binary, and mixed **at run time** by
//! a callback that never allocates or blocks. Rendering and playback are
//! separate features, so a build script that renders needs no audio device
//! and a game that plays needs no C compiler.
//!
//! # Engines
//!
//! - [`chip8`]: an 8-bit-style synthesizer (square, pulse, triangle and
//!   noise voices, FX buses, a small reverb) that renders the ABC notation
//!   dialect of the Electric Pulse engine. Its samples are stored as 8-bit
//!   mu-law, one byte a sample.
//!
//! - [`advanced`] (in development): a studio that renders `.score` files
//!   with a sampled orchestra and an analog-style synth, through buses,
//!   reverbs and a limiter, in float stereo. Its music is stored as QOA and
//!   decoded as it plays.
//!
//! # Features
//!
//! | Feature | Adds | Needs |
//! |---|---|---|
//! | `render` | [`chip8::render_file`], the C engine | a C compiler |
//! | `render-advanced` | `advanced::render_score`, the studio | a C compiler |
//! | `output` | [`chip8::Output`], the mixer on an audio device | ALSA headers on Linux |
//!
//! The mixer ([`chip8::Mixer`]) and the codec ([`chip8::mulaw`]) are pure
//! Rust and always available. See the README for a build script that
//! renders a game's cues and the code that plays them.

#![warn(missing_docs)]

pub mod advanced;
pub mod chip8;
#[cfg(feature = "output")]
mod device;
