//! The audio device both engines play on (`output` feature): one `cpal`
//! output stream whose callback owns a mixer. The game thread reaches the
//! mixer only through a channel of commands, so the callback never waits on
//! a lock the game holds.

use std::fmt;
use std::sync::mpsc::{Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

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

/// What runs in the callback: a mixer of either engine.
pub(crate) trait Mix: Send + 'static {
    type Command: Send + 'static;
    fn apply(&mut self, command: Self::Command);
    fn render(&mut self, out: &mut [f32], channels: usize);
}

/// An open stream and the way to its mixer.
pub(crate) struct Device<C> {
    // Dropping the stream stops the sound; it lives as long as `Device`.
    _stream: cpal::Stream,
    commands: Sender<C>,
    sample_rate: u32,
}

impl<C: Send + 'static> Device<C> {
    /// Opens the default output device and runs the mixer `make` builds
    /// for the device's rate.
    pub(crate) fn open<M>(make: impl FnOnce(u32) -> M) -> Result<Self, OutputError>
    where
        M: Mix<Command = C>,
    {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(OutputError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| OutputError::Device(e.to_string()))?;
        let config = supported.config();
        let (commands, inbox) = std::sync::mpsc::channel();
        let mixer = make(config.sample_rate);
        let channels = usize::from(config.channels);
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32, M>(&device, &config, mixer, inbox, channels),
            SampleFormat::I16 => build::<i16, M>(&device, &config, mixer, inbox, channels),
            SampleFormat::U16 => build::<u16, M>(&device, &config, mixer, inbox, channels),
            SampleFormat::I32 => build::<i32, M>(&device, &config, mixer, inbox, channels),
            other => return Err(OutputError::UnsupportedFormat(other.to_string())),
        }?;
        stream
            .play()
            .map_err(|e| OutputError::Device(e.to_string()))?;
        Ok(Self {
            _stream: stream,
            commands,
            sample_rate: config.sample_rate,
        })
    }

    /// Never blocks. If the stream has died the command is dropped and the
    /// game goes on silent.
    pub(crate) fn send(&self, command: C) {
        let _ = self.commands.send(command);
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

fn build<T, M>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer: M,
    inbox: Receiver<M::Command>,
    channels: usize,
) -> Result<cpal::Stream, OutputError>
where
    T: SizedSample + FromSample<f32>,
    M: Mix,
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
