//! Renders an ABC score, stores it as mu-law, and plays it through the
//! mixer on the default audio device: the whole pipeline in one process.
//! A game splits it in two, rendering in its build script.
//!
//! ```text
//! cargo run --example play --features render,output -- tests/probes/chord.abc
//! ```

use std::process::ExitCode;
use std::time::Duration;

use sonomorph::chip8::{mulaw, render_file, Command, MixerConfig, Output, Sound};

/// The one sound this example plays.
#[derive(Clone, Copy, PartialEq)]
struct Clip(&'static [u8]);

impl Sound for Clip {
    fn pcm(self) -> &'static [u8] {
        self.0
    }
}

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: play <score.abc>");
        return ExitCode::from(2);
    };
    let config = MixerConfig::default();
    let samples = match render_file(&path, config.sample_rate as u32) {
        Ok(samples) => samples,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    // A game embeds these bytes with include_bytes!; here they live for the
    // rest of the process.
    let bytes: Vec<u8> = samples.iter().copied().map(mulaw::encode).collect();
    let clip = Clip(Box::leak(bytes.into_boxed_slice()));

    let output = match Output::open_with(config) {
        Ok(output) => output,
        Err(err) => {
            eprintln!("audio: {err}");
            return ExitCode::FAILURE;
        }
    };
    output.send(Command::Play {
        sound: clip,
        gain: 1.0,
        pan: 0.0,
        pitch: 1.0,
        duck: 0.0,
    });
    let secs = samples.len() as f32 / config.sample_rate;
    println!("playing {path}: {secs:.1} s at {} Hz", output.sample_rate());
    std::thread::sleep(Duration::from_secs_f32(secs + 0.3));
    ExitCode::SUCCESS
}
