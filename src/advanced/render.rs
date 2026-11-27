//! The studio behind a safe call (`render-advanced` feature).

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::{Path, PathBuf};

/// The instrument library this crate ships (`library/`): the mixing desk,
/// the orchestra (VSCO 2 Community Edition samples, CC0) and the synth
/// palette. A score reaches it as `include @library/instruments/desk.inc`,
/// or names a sample map as `sfz=@library/samples/violins_sus.sfz`.
///
/// It is a path in this crate's source, so it exists where the crate was
/// built from source: in a build script, which is where scores render.
pub const LIBRARY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/library");

/// `engine/advanced/studio.h`'s `StRender`.
#[repr(C)]
struct StRender {
    frames: *mut f32,
    frame_count: c_int,
    rate: c_int,
    looped: c_int,
}

mod ffi {
    use super::StRender;
    use std::os::raw::{c_char, c_int, c_uint, c_void};

    extern "C" {
        /// Renders a `.score`; 0 on success, else `err` says why. Reentrant.
        pub fn st_render_score_lib(
            path: *const c_char,
            rate: c_int,
            library: *const c_char,
            out: *mut StRender,
            err: *mut c_char,
            errlen: usize,
        ) -> c_int;
        pub fn st_render_free(render: *mut StRender);
        /// `engine/advanced/sonomorph_studio.c`.
        pub fn sono_studio_to_s16(render: *const StRender) -> *mut i16;
        pub fn sono_studio_to_qoa(render: *const StRender, out_len: *mut c_uint) -> *mut u8;
        pub fn sono_free(p: *mut c_void);
    }
}

/// Why a score did not render.
#[derive(Debug)]
pub enum RenderError {
    /// The sample rate does not fit the engine's `int`.
    SampleRate(u32),
    /// A path is not UTF-8 or holds a NUL byte, so it cannot reach C.
    Path(PathBuf),
    /// The studio refused the score: a parse error (with its file and
    /// line), a sample map or sample it could not load, or a missing
    /// `length`.
    Score(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleRate(rate) => write!(f, "unsupported sample rate {rate} Hz"),
            Self::Path(path) => write!(f, "{}: path is not valid UTF-8 C text", path.display()),
            Self::Score(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for RenderError {}

/// A rendered score: 32-bit float stereo, interleaved.
#[derive(Debug, Clone, PartialEq)]
pub struct Render {
    frames: Vec<f32>,
    rate: u32,
    looped: bool,
}

impl Render {
    /// The samples, interleaved left and right.
    pub fn frames(&self) -> &[f32] {
        &self.frames
    }

    /// Stereo frames: half the samples.
    pub fn frame_count(&self) -> usize {
        self.frames.len() / 2
    }

    /// The rate it rendered at, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Whether the score was a loop (`loop on`): its end flows into its
    /// start, so it should play wrapped and never be trimmed.
    pub fn is_loop(&self) -> bool {
        self.looped
    }

    /// A view the C side reads; it never writes through `frames`.
    fn as_c(&self) -> StRender {
        StRender {
            frames: self.frames.as_ptr().cast_mut(),
            frame_count: c_int::try_from(self.frame_count()).unwrap_or(c_int::MAX),
            rate: c_int::try_from(self.rate).unwrap_or(c_int::MAX),
            looped: c_int::from(self.looped),
        }
    }

    /// 16-bit stereo, interleaved, with triangular dither from a fixed
    /// seed: the same render always gives the same samples.
    pub fn to_s16(&self) -> Vec<i16> {
        if self.frames.is_empty() {
            return Vec::new();
        }
        let view = self.as_c();
        // SAFETY: `view` points at `self.frames`, valid for the call; the
        // function returns a malloc'ed buffer of 2 * frame_count samples or
        // NULL.
        let ptr = unsafe { ffi::sono_studio_to_s16(&view) };
        assert!(!ptr.is_null(), "out of memory converting a render");
        // SAFETY: as above; the buffer is freed once, after the copy.
        let samples = unsafe { std::slice::from_raw_parts(ptr, self.frames.len()) }.to_vec();
        unsafe { ffi::sono_free(ptr.cast::<c_void>()) };
        samples
    }

    /// The render as QOA, which [`qoa::QoaStream`](super::qoa::QoaStream)
    /// decodes and [`Mixer`](super::Mixer) plays. A loop is encoded with
    /// its predictor warmed on its own end, so the wrap does not click.
    pub fn to_qoa(&self) -> Vec<u8> {
        if self.frames.is_empty() {
            return Vec::new();
        }
        let view = self.as_c();
        let mut len: c_uint = 0;
        // SAFETY: as in `to_s16`; on success the buffer holds `len` bytes.
        let ptr = unsafe { ffi::sono_studio_to_qoa(&view, &mut len) };
        assert!(!ptr.is_null(), "out of memory encoding a render");
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) }.to_vec();
        unsafe { ffi::sono_free(ptr.cast::<c_void>()) };
        bytes
    }
}

/// Renders the `.score` at `path` with this crate's instrument library
/// ([`LIBRARY`]) at `rate` Hz, or at the score's own `rate` (48 kHz unless
/// it says otherwise) when `rate` is `None`.
///
/// Deterministic: the studio seeds every voice's randomness from the score,
/// so the same score renders the same samples, on any thread, on a machine.
/// Between machines the last bits can differ: the studio computes in float
/// and calls libm, which glibc picks per CPU.
///
/// # Errors
///
/// [`RenderError`] when the score, an included file, a sample map or a
/// sample cannot be read or parsed.
pub fn render_score(path: impl AsRef<Path>, rate: Option<u32>) -> Result<Render, RenderError> {
    render_score_with(path, rate, Some(Path::new(LIBRARY)))
}

/// As [`render_score`], with `library` the directory `@library/` paths
/// resolve against (`None`: no library).
///
/// # Errors
///
/// As [`render_score`].
pub fn render_score_with(
    path: impl AsRef<Path>,
    rate: Option<u32>,
    library: Option<&Path>,
) -> Result<Render, RenderError> {
    let path = path.as_ref();
    let rate = match rate {
        None => 0,
        Some(rate) => c_int::try_from(rate)
            .ok()
            .filter(|&r| r > 0)
            .ok_or(RenderError::SampleRate(rate))?,
    };
    let c_text = |p: &Path| {
        p.to_str()
            .and_then(|text| CString::new(text).ok())
            .ok_or_else(|| RenderError::Path(p.to_owned()))
    };
    let c_path = c_text(path)?;
    let c_library = library.map(c_text).transpose()?;

    let mut out = StRender {
        frames: std::ptr::null_mut(),
        frame_count: 0,
        rate: 0,
        looped: 0,
    };
    let mut err = [0 as c_char; 512];
    // SAFETY: every pointer is valid for the call; `err` is a buffer of the
    // length given; on success `out` owns a malloc'ed buffer freed below.
    let status = unsafe {
        ffi::st_render_score_lib(
            c_path.as_ptr(),
            rate,
            c_library.as_ref().map_or(std::ptr::null(), |l| l.as_ptr()),
            &mut out,
            err.as_mut_ptr(),
            err.len(),
        )
    };
    if status != 0 {
        // SAFETY: the studio writes a NUL-terminated message on failure.
        let why = unsafe { CStr::from_ptr(err.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        unsafe { ffi::st_render_free(&mut out) };
        return Err(RenderError::Score(if why.is_empty() {
            format!("{}: the studio rendered nothing", path.display())
        } else {
            why
        }));
    }
    let samples = usize::try_from(out.frame_count).unwrap_or(0) * 2;
    let frames = if out.frames.is_null() || samples == 0 {
        Vec::new()
    } else {
        // SAFETY: on success `frames` holds 2 * frame_count floats.
        unsafe { std::slice::from_raw_parts(out.frames, samples) }.to_vec()
    };
    let render = Render {
        frames,
        rate: u32::try_from(out.rate).unwrap_or(0),
        looped: out.looped != 0,
    };
    // SAFETY: frees what the studio allocated, once.
    unsafe { ffi::st_render_free(&mut out) };
    Ok(render)
}
