//! `sonomorph-render`: renders an ABC score with the chip8 engine, to
//! audition it without a game around it, or to measure it from a script.
//!
//! ```text
//! sonomorph-render <in.abc> <out.wav|out.s16> [sample_rate, default 22050]
//! ```
//!
//! `.wav` writes a 16-bit mono WAV; any other extension writes the raw
//! samples, signed 16-bit little-endian mono, with nothing around them (the
//! interface of BRYX's old `audio/render.c`, which its `compose.py` calls).
//!
//! ```text
//! cargo install --git https://github.com/chussenot/sonomorph --features render sonomorph-render
//! cargo run --features render --bin sonomorph-render -- tests/probes/chord.abc chord.wav
//! ```

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output, rate) = match args.as_slice() {
        [input, output] => (input, output, 22_050),
        [input, output, rate] => match rate.parse() {
            Ok(rate) => (input, output, rate),
            Err(_) => return usage(),
        },
        _ => return usage(),
    };
    let samples = match sonomorph::chip8::render_file(input, rate) {
        Ok(samples) => samples,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let written = if output.ends_with(".wav") {
        write_wav(output, rate, &samples)
    } else {
        write_raw(output, &samples)
    };
    if let Err(err) = written {
        eprintln!("{output}: {err}");
        return ExitCode::FAILURE;
    }
    eprintln!(
        "{output}: {} samples, {:.2} s at {rate} Hz",
        samples.len(),
        samples.len() as f64 / f64::from(rate)
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: sonomorph-render <in.abc> <out.wav|out.s16> [sample_rate, default 22050]");
    ExitCode::from(2)
}

/// A canonical 44-byte RIFF header, then the samples, little-endian.
fn write_wav(path: &str, rate: u32, samples: &[i16]) -> std::io::Result<()> {
    let data_len = u32::try_from(samples.len() * 2).expect("under 4 GiB");
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data_len).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    out.write_all(&1u16.to_le_bytes())?; // PCM
    out.write_all(&1u16.to_le_bytes())?; // mono
    out.write_all(&rate.to_le_bytes())?;
    out.write_all(&(rate * 2).to_le_bytes())?; // bytes per second
    out.write_all(&2u16.to_le_bytes())?; // block align
    out.write_all(&16u16.to_le_bytes())?; // bits per sample
    out.write_all(b"data")?;
    out.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        out.write_all(&sample.to_le_bytes())?;
    }
    out.flush()
}

/// The samples alone: signed 16-bit little-endian mono.
fn write_raw(path: &str, samples: &[i16]) -> std::io::Result<()> {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(path, bytes)
}
