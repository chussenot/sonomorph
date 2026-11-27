//! `sonomorph-render`: renders a score, to audition it without a game
//! around it, or to measure it from a script.
//!
//! ```text
//! sonomorph-render <in.abc> <out.wav|out.s16> [sample_rate, default 22050]
//! sonomorph-render <in.score> <out.wav|out.qoa|out.f32|out.s16> [sample_rate]
//! ```
//!
//! An `.abc` score renders with chip8 (the `render` feature) to mono; a
//! `.score` with the studio (`render-advanced`) to stereo, at the score's
//! own rate unless one is given. `.wav` writes a 16-bit WAV; `.qoa` the
//! QOA a game embeds; `.f32` the raw float frames; any other extension the
//! raw signed 16-bit little-endian samples, with nothing around them (the
//! interface of BRYX's old `audio/render.c`, which its scripts call).
//!
//! ```text
//! cargo install --git https://github.com/chussenot/sonomorph --features render,render-advanced sonomorph-render
//! sonomorph-render tests/probes/chord.abc chord.wav
//! ```

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output, rate) = match args.as_slice() {
        [input, output] => (input, output, None),
        [input, output, rate] => match rate.parse() {
            Ok(rate) => (input, output, Some(rate)),
            Err(_) => return usage(),
        },
        _ => return usage(),
    };
    let result = if input.ends_with(".score") {
        studio(input, output, rate)
    } else {
        chip8(input, output, rate.unwrap_or(22_050))
    };
    match result {
        Ok(summary) => {
            eprintln!("{output}: {summary}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: sonomorph-render <in.abc|in.score> <out.wav|out.qoa|out.f32|out.s16> [sample_rate]"
    );
    ExitCode::from(2)
}

fn chip8(input: &str, output: &str, rate: u32) -> Result<String, String> {
    let samples = sonomorph::chip8::render_file(input, rate).map_err(|e| e.to_string())?;
    let written = if output.ends_with(".wav") {
        write_wav(output, rate, 1, &samples)
    } else {
        write_raw(output, &samples)
    };
    written.map_err(|e| format!("{output}: {e}"))?;
    Ok(format!(
        "{} samples, {:.2} s at {rate} Hz, mono",
        samples.len(),
        samples.len() as f64 / f64::from(rate)
    ))
}

#[cfg(feature = "render-advanced")]
fn studio(input: &str, output: &str, rate: Option<u32>) -> Result<String, String> {
    let render = sonomorph::advanced::render_score(input, rate).map_err(|e| e.to_string())?;
    let written = if output.ends_with(".qoa") {
        std::fs::write(output, render.to_qoa())
    } else if output.ends_with(".f32") {
        let bytes: Vec<u8> = render
            .frames()
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        std::fs::write(output, bytes)
    } else if output.ends_with(".wav") {
        write_wav(output, render.sample_rate(), 2, &render.to_s16())
    } else {
        write_raw(output, &render.to_s16())
    };
    written.map_err(|e| format!("{output}: {e}"))?;
    Ok(format!(
        "{} frames, {:.2} s at {} Hz, stereo{}",
        render.frame_count(),
        render.frame_count() as f64 / f64::from(render.sample_rate()),
        render.sample_rate(),
        if render.is_loop() { ", a loop" } else { "" }
    ))
}

#[cfg(not(feature = "render-advanced"))]
fn studio(_input: &str, _output: &str, _rate: Option<u32>) -> Result<String, String> {
    Err("rendering a .score needs the `render-advanced` feature".into())
}

/// A canonical 44-byte RIFF header, then the samples, little-endian.
fn write_wav(path: &str, rate: u32, channels: u16, samples: &[i16]) -> std::io::Result<()> {
    let data_len = u32::try_from(samples.len() * 2).expect("under 4 GiB");
    let block = 2 * channels;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data_len).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    out.write_all(&1u16.to_le_bytes())?; // PCM
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&rate.to_le_bytes())?;
    out.write_all(&(rate * u32::from(block)).to_le_bytes())?; // bytes per second
    out.write_all(&block.to_le_bytes())?; // block align
    out.write_all(&16u16.to_le_bytes())?; // bits per sample
    out.write_all(b"data")?;
    out.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        out.write_all(&sample.to_le_bytes())?;
    }
    out.flush()
}

/// The samples alone: signed 16-bit little-endian.
fn write_raw(path: &str, samples: &[i16]) -> std::io::Result<()> {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(path, bytes)
}
