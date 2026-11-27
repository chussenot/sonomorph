//! The audio device (`output` feature): one `cpal` output stream whose
//! callback owns the mixer. The game thread talks to it only through a
//! channel of [`Command`]s, so the callback never waits on a lock the game
//! holds.

use std::fmt;
use std::marker::PhantomData;
use std::sync::mpsc::{Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use super::mixer::{Command, Mixer, MixerConfig, Sound};

/// Why the device could not be opened. A game should carry on silent.
#[derive(Debug)]
pub enum OutputError {
    /// The host has no output device (a headless machine).
    NoDevice,
    /// The device's sample format is one the mixer does not write.
    UnsupportedFormat(String),
    /// The device refused: its configuration, the stream or playback.
    Device(String),
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => f.write_str("no audio output device"),
            Self::UnsupportedFormat(format) => write!(f, "unsupported sample format {format}"),
            Self::Device(err) => f.write_str(err),
        }
    }
}

impl std::error::Error for OutputError {}

/// A [`Mixer`] playing on the default output device until dropped.
pub struct Output<S: Sound> {
    // Dropping the stream stops the sound; it lives as long as `Output`.
    _stream: cpal::Stream,
    commands: Sender<Command<S>>,
    sample_rate: u32,
    _sound: PhantomData<S>,
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
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(OutputError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| OutputError::Device(e.to_string()))?;
        let stream_config = supported.config();
        let (commands, inbox) = std::sync::mpsc::channel();
        let mixer = Mixer::with_config(config, stream_config.sample_rate);
        let channels = usize::from(stream_config.channels);
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32, S>(&device, &stream_config, mixer, inbox, channels),
            SampleFormat::I16 => build::<i16, S>(&device, &stream_config, mixer, inbox, channels),
            SampleFormat::U16 => build::<u16, S>(&device, &stream_config, mixer, inbox, channels),
            SampleFormat::I32 => build::<i32, S>(&device, &stream_config, mixer, inbox, channels),
            other => return Err(OutputError::UnsupportedFormat(other.to_string())),
        }?;
        stream
            .play()
            .map_err(|e| OutputError::Device(e.to_string()))?;
        Ok(Self {
            _stream: stream,
            commands,
            sample_rate: stream_config.sample_rate,
            _sound: PhantomData,
        })
    }

    /// Hands a command to the mixer; it applies at the start of the next
    /// buffer. Never blocks. If the stream has died the command is dropped
    /// and the game goes on silent.
    pub fn send(&self, command: Command<S>) {
        let _ = self.commands.send(command);
    }

    /// The device's rate, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

fn build<T, S>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer: Mixer<S>,
    inbox: Receiver<Command<S>>,
    channels: usize,
) -> Result<cpal::Stream, OutputError>
where
    T: SizedSample + FromSample<f32>,
    S: Sound,
{
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                while let Ok(command) = inbox.try_recv() {
                    mixer.apply(command);
                }
                // Grows once to the device's buffer size, then never again.
                scratch.resize(data.len(), 0.0);
                mixer.render(&mut scratch, channels);
                for (out, &s) in data.iter_mut().zip(&scratch) {
                    *out = T::from_sample(s);
                }
            },
            |err| eprintln!("sonomorph: stream error: {err}"),
            None,
        )
        .map_err(|e| OutputError::Device(e.to_string()))
}
