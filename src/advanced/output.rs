//! The advanced mixer on the default audio device (`output` feature).

use crate::chip8::{Command, MixerConfig};
use crate::device::{Device, Mix, OutputError};

use super::mixer::{Mixer, Sound};

impl<S: Sound> Mix for Mixer<S> {
    type Command = Command<S>;

    fn apply(&mut self, command: Command<S>) {
        Mixer::apply(self, command);
    }

    fn render(&mut self, out: &mut [f32], channels: usize) {
        Mixer::render(self, out, channels);
    }
}

/// An advanced [`Mixer`] playing on the default output device until
/// dropped. Its callback never waits on the game: commands reach it over a
/// channel.
pub struct Output<S: Sound> {
    device: Device<Command<S>>,
}

impl<S: Sound> Output<S> {
    /// Opens the default output device with a mixer of the default
    /// [`MixerConfig`].
    ///
    /// # Errors
    ///
    /// [`OutputError`] when there is no device or it refuses the stream.
    pub fn open() -> Result<Self, OutputError> {
        Self::open_with(MixerConfig::default())
    }

    /// Opens the default output device with a mixer of `config`.
    ///
    /// # Errors
    ///
    /// [`OutputError`] when there is no device or it refuses the stream.
    pub fn open_with(config: MixerConfig) -> Result<Self, OutputError> {
        let device = Device::open(|rate| Mixer::with_config(config, rate))?;
        Ok(Self { device })
    }

    /// Hands a command to the mixer; it applies at the start of the next
    /// buffer. Never blocks.
    pub fn send(&self, command: Command<S>) {
        self.device.send(command);
    }

    /// The device's rate, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.device.sample_rate()
    }
}
