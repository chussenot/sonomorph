//! The C engine behind a safe call (`render` feature).

use std::ffi::CString;
use std::fmt;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};

mod ffi {
    use std::os::raw::{c_char, c_int};

    /// `engine/chip8/audio_engine.h`'s `AudioRenderStats`.
    #[repr(C)]
    #[derive(Default)]
    pub struct AudioRenderStats {
        pub sample_count: u64,
        pub duration_ms: f64,
        pub min_sample: c_int,
        pub max_sample: c_int,
        pub peak: c_int,
        pub clipping_count: u64,
        pub checksum: u64,
        pub render_time_ms: f64,
    }

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
        /// The same render quantized once to unsigned 8-bit with triangular
        /// dither, and its statistics.
        pub fn audio_engine_render_abc_file(
            path: *const c_char,
            sample_rate: c_int,
            out_len: *mut c_int,
            out_stats: *mut AudioRenderStats,
        ) -> *mut u8;
        /// Frees an 8-bit buffer.
        pub fn audio_engine_free_buffer(buffer: *mut u8);
    }
}

/// Why a score did not render, load or export.
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
    /// The engine could not use the file: it does not parse into a song,
    /// or (for a jam session or a MIDI export) holds no notes.
    Parse(PathBuf),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleRate(rate) => write!(f, "unsupported sample rate {rate} Hz"),
            Self::Path(path) => write!(f, "{}: path is not valid UTF-8 C text", path.display()),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Engine(path) => write!(f, "{}: the engine rendered nothing", path.display()),
            Self::Parse(path) => write!(f, "{}: the engine could not use it", path.display()),
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

/// `sample_rate` as the engine's `int`, refused when zero or too large.
pub(crate) fn engine_rate(sample_rate: u32) -> Result<c_int, RenderError> {
    c_int::try_from(sample_rate)
        .ok()
        .filter(|&rate| rate > 0)
        .ok_or(RenderError::SampleRate(sample_rate))
}

/// The path of an existing file, as C text. The engine reports a missing
/// file and a bad score the same way (NULL); reading the metadata first
/// tells the two apart.
pub(crate) fn existing_c_path(path: &Path) -> Result<CString, RenderError> {
    std::fs::metadata(path).map_err(|source| RenderError::Io {
        path: path.to_owned(),
        source,
    })?;
    c_path(path)
}

/// Any path as C text.
pub(crate) fn c_path(path: &Path) -> Result<CString, RenderError> {
    path.to_str()
        .and_then(|text| CString::new(text).ok())
        .ok_or_else(|| RenderError::Path(path.to_owned()))
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
    let rate = engine_rate(sample_rate)?;
    let c_path = existing_c_path(path)?;

    let mut len: c_int = 0;
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the
    // call, and `len` is a valid place for the engine to write the length.
    let buffer = unsafe { ffi::audio_engine_render_abc_file_s16(c_path.as_ptr(), rate, &mut len) };
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

/// What a render measured, for an 8-bit render ([`render_file_u8`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RenderStats {
    /// Samples rendered.
    pub sample_count: u64,
    /// Their duration, in milliseconds.
    pub duration_ms: f64,
    /// The lowest sample (0..=255).
    pub min_sample: i32,
    /// The highest sample (0..=255).
    pub max_sample: i32,
    /// The largest distance from the 128 center.
    pub peak: i32,
    /// Clipped samples: in a run at a rail (0 or 255), the fourth and each
    /// one after it (a waveform's peak touching the rail is not clipping).
    pub clipping_count: u64,
    /// FNV-1a 64 over the samples, from the engine's own offset basis
    /// (`1469598103934665603`: the standard one with its last digit lost,
    /// kept so that checksums recorded before stay valid). Equal renders
    /// have equal checksums.
    pub checksum: u64,
    /// How long the render took, in milliseconds of CPU time.
    pub render_time_ms: f64,
}

impl From<ffi::AudioRenderStats> for RenderStats {
    fn from(s: ffi::AudioRenderStats) -> Self {
        Self {
            sample_count: s.sample_count,
            duration_ms: s.duration_ms,
            min_sample: s.min_sample,
            max_sample: s.max_sample,
            peak: s.peak,
            clipping_count: s.clipping_count,
            checksum: s.checksum,
            render_time_ms: s.render_time_ms,
        }
    }
}

/// Renders the ABC score at `path` as unsigned 8-bit mono (128 is silence)
/// at `sample_rate` Hz, with its [`RenderStats`]: the engine's original
/// output format. The 16-bit mix ([`render_file`]) is quantized once, with
/// triangular dither from a fixed seed, so the render is deterministic and
/// each sample is within one and a half steps of `128 + s16 / 256`.
///
/// # Errors
///
/// As [`render_file`].
pub fn render_file_u8(
    path: impl AsRef<Path>,
    sample_rate: u32,
) -> Result<(Vec<u8>, RenderStats), RenderError> {
    let path = path.as_ref();
    let rate = engine_rate(sample_rate)?;
    let c_path = existing_c_path(path)?;

    let mut len: c_int = 0;
    let mut stats = ffi::AudioRenderStats::default();
    // SAFETY: as in `render_file`; `stats` is a valid place for the engine
    // to write its statistics.
    let buffer =
        unsafe { ffi::audio_engine_render_abc_file(c_path.as_ptr(), rate, &mut len, &mut stats) };
    let samples = owned_u8(buffer, len);
    match samples {
        Some(samples) => Ok((samples, stats.into())),
        None => Err(RenderError::Engine(path.to_owned())),
    }
}

/// Copies and frees an 8-bit buffer the engine returned; `None` for NULL or
/// empty.
pub(crate) fn owned_u8(buffer: *mut u8, len: c_int) -> Option<Vec<u8>> {
    if buffer.is_null() {
        return None;
    }
    let samples = match usize::try_from(len) {
        // SAFETY: on success the engine returns a buffer of exactly `len`
        // initialized bytes, valid until freed below.
        Ok(len) if len > 0 => unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec(),
        _ => Vec::new(),
    };
    // SAFETY: `buffer` came from the engine's allocator and is freed once.
    unsafe { ffi::audio_engine_free_buffer(buffer) };
    (!samples.is_empty()).then_some(samples)
}
