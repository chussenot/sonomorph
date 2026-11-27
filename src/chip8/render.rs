//! The C engine behind a safe call (`render` feature).

use std::ffi::CString;
use std::fmt;
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};

mod ffi {
    use std::os::raw::{c_char, c_int};

    extern "C" {
        /// `engine/chip8/audio_engine.h`: renders an ABC file as signed
        /// 16-bit mono. Returns a `malloc`ed buffer of `*out_len` samples,
        /// or NULL. Reentrant: the engine keeps no global state.
        pub fn audio_engine_render_abc_file_s16(
            path: *const c_char,
            sample_rate: c_int,
            out_len: *mut c_int,
        ) -> *mut i16;
        /// Frees what the call above returned.
        pub fn audio_engine_free_s16(buffer: *mut i16);
    }
}

/// Why a score did not render.
#[derive(Debug)]
pub enum RenderError {
    /// The sample rate is zero or does not fit the engine's `int`.
    SampleRate(u32),
    /// The path is not UTF-8 or holds a NUL byte, so it cannot reach the
    /// engine's `fopen`.
    Path(PathBuf),
    /// The file could not be read.
    Io {
        /// The score's path.
        path: PathBuf,
        /// What the operating system said.
        source: std::io::Error,
    },
    /// The engine read the file and produced no sound: it is not a score
    /// the engine can parse, or it holds no notes.
    Engine(PathBuf),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleRate(rate) => write!(f, "unsupported sample rate {rate} Hz"),
            Self::Path(path) => write!(f, "{}: path is not valid UTF-8 C text", path.display()),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Engine(path) => write!(f, "{}: the engine rendered nothing", path.display()),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Renders the ABC score at `path` as signed 16-bit mono samples at
/// `sample_rate` Hz.
///
/// Deterministic: the engine has no global state and seeds its noise and
/// dither generators per render, so the same file at the same rate gives
/// the same samples, on any thread, in any order.
///
/// A score marked `%%loop on` renders its loop with its own tail carried
/// over the seam; anything else renders to the end of its last release,
/// silence included (trim it if a voice should free itself early).
///
/// # Errors
///
/// [`RenderError`] when the rate is out of range, the path cannot be
/// passed to C, the file cannot be read, or the engine renders nothing.
pub fn render_file(path: impl AsRef<Path>, sample_rate: u32) -> Result<Vec<i16>, RenderError> {
    let path = path.as_ref();
    let rate = c_int::try_from(sample_rate)
        .ok()
        .filter(|&rate| rate > 0)
        .ok_or(RenderError::SampleRate(sample_rate))?;
    // The engine reports a missing file and a bad score the same way (NULL);
    // reading the metadata first tells the two apart.
    std::fs::metadata(path).map_err(|source| RenderError::Io {
        path: path.to_owned(),
        source,
    })?;
    let c_path = path
        .to_str()
        .and_then(|text| CString::new(text).ok())
        .ok_or_else(|| RenderError::Path(path.to_owned()))?;

    let mut len: c_int = 0;
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
    // call, and `len` is a valid place for the engine to write the length.
    let buffer = unsafe {
        ffi::audio_engine_render_abc_file_s16(c_path.as_ptr() as *const c_char, rate, &mut len)
    };
    if buffer.is_null() {
        return Err(RenderError::Engine(path.to_owned()));
    }
    let samples = match usize::try_from(len) {
        Ok(len) if len > 0 => {
            // SAFETY: on success the engine returns a buffer of exactly
            // `len` initialized samples, which stays valid until freed below.
            unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec()
        }
        _ => Vec::new(),
    };
    // SAFETY: `buffer` came from the engine's allocator and is freed once.
    unsafe { ffi::audio_engine_free_s16(buffer) };
    if samples.is_empty() {
        return Err(RenderError::Engine(path.to_owned()));
    }
    Ok(samples)
}
