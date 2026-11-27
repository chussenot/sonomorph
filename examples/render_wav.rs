//! Renders an ABC score to a 16-bit mono WAV file, to audition a score
//! without a game around it.
//!
//! ```text
//! cargo run --example render_wav --features render -- tests/probes/chord.abc chord.wav
//! cargo run --example render_wav --features render -- song.abc song.wav 44100
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
    if let Err(err) = write_wav(output, rate, &samples) {
        eprintln!("{output}: {err}");
        return ExitCode::FAILURE;
    }
    println!(
        "{output}: {} samples, {:.2} s at {rate} Hz",
        samples.len(),
        samples.len() as f64 / f64::from(rate)
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: render_wav <in.abc> <out.wav> [sample_rate, default 22050]");
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
