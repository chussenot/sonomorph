//! Endless variations on a song (`jam` feature): the score's arrangement
//! played section by section, each section a seeded variation of the one
//! before (velocities humanized; sometimes the arrangement shuffled, a drum
//! fill added on the last bar, a voice muted in a pattern, the lead moved
//! an octave).

use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use super::render::{engine_rate, existing_c_path, owned_u8, RenderError};

mod ffi {
    use std::os::raw::{c_char, c_double, c_int};

    #[repr(C)]
    pub struct AudioJamSession {
        _private: [u8; 0],
    }

    extern "C" {
        pub fn audio_jam_session_open(
            path: *const c_char,
            seed: u64,
            section_seconds: c_double,
        ) -> *mut AudioJamSession;
        pub fn audio_jam_session_render_next(
            session: *mut AudioJamSession,
            sample_rate: c_int,
            out_len: *mut c_int,
        ) -> *mut u8;
        pub fn audio_jam_session_iteration(session: *const AudioJamSession) -> c_int;
        pub fn audio_jam_session_arrangement_offset(session: *const AudioJamSession) -> c_int;
        pub fn audio_jam_session_slots_per_section(session: *const AudioJamSession) -> c_int;
        pub fn audio_jam_session_close(session: *mut AudioJamSession);
    }
}

/// The longest section a session asks for, in seconds. Longer requests are
/// clamped to it: the engine converts the length to a step count in an
/// `int`, and an hour is far past any arrangement.
pub const MAX_SECTION_SECONDS: f64 = 3600.0;

/// One rendered section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JamSection {
    /// Unsigned 8-bit mono (128 is silence), at the rate asked for.
    pub samples: Vec<u8>,
    /// Which variation this is, from 1.
    pub iteration: u32,
    /// Where the next section starts in the score's arrangement, in slots
    /// (it counts on past the arrangement's end, which wraps).
    pub arrangement_offset: usize,
    /// How many arrangement slots a section covers.
    pub slots_per_section: usize,
}

/// A song being varied, section after section, from a seed.
///
/// Deterministic: the same file, seed and section length give the same
/// sections in the same order. The session owns its parsed song; the file
/// is read once, when it opens.
pub struct JamSession {
    raw: NonNull<ffi::AudioJamSession>,
    path: PathBuf,
}

// SAFETY: the session is a heap block the engine reads and writes only
// through calls made with `&mut self` (rendering) or `&self` (reading three
// integers); it holds no thread-local or global state, so it can move to
// another thread. It is not `Sync`, so no two threads use it at once.
unsafe impl Send for JamSession {}

impl JamSession {
    /// Parses the ABC score at `path` and starts a session of sections of
    /// about `section_seconds` each (whole arrangement slots, at least one;
    /// a negative or NaN length counts as zero, a longer one than
    /// [`MAX_SECTION_SECONDS`] as that).
    ///
    /// # Errors
    ///
    /// [`RenderError::Io`] or [`RenderError::Path`] when the file cannot be
    /// read or named to C; [`RenderError::Parse`] when the engine cannot
    /// parse it or it holds no notes. A score without `%%arrangement` is
    /// one pattern of all its steps, and a section is all of it.
    pub fn open(
        path: impl AsRef<Path>,
        seed: u64,
        section_seconds: f64,
    ) -> Result<Self, RenderError> {
        let path = path.as_ref();
        let c_path = existing_c_path(path)?;
        // The engine converts the length to an `int` step count: NaN or a
        // length past that is undefined behavior there, so neither reaches it.
        let seconds = if section_seconds.is_nan() {
            0.0
        } else {
            section_seconds.clamp(0.0, MAX_SECTION_SECONDS)
        };
        // SAFETY: a valid NUL-terminated path that outlives the call.
        let raw = unsafe { ffi::audio_jam_session_open(c_path.as_ptr(), seed, seconds) };
        NonNull::new(raw)
            .map(|raw| Self {
                raw,
                path: path.to_owned(),
            })
            .ok_or_else(|| RenderError::Parse(path.to_owned()))
    }

    /// Renders the next section at `sample_rate` Hz and moves on.
    ///
    /// # Errors
    ///
    /// [`RenderError::SampleRate`] for a rate out of range;
    /// [`RenderError::Engine`] when the section renders nothing.
    pub fn render_next(&mut self, sample_rate: u32) -> Result<JamSection, RenderError> {
        let rate = engine_rate(sample_rate)?;
        let mut len: c_int = 0;
        // SAFETY: `raw` is a live session, used by this thread alone
        // (`&mut self`), and `len` is a valid place to write the length.
        let buffer =
            unsafe { ffi::audio_jam_session_render_next(self.raw.as_ptr(), rate, &mut len) };
        let samples =
            owned_u8(buffer, len).ok_or_else(|| RenderError::Engine(self.path.clone()))?;
        Ok(JamSection {
            samples,
            iteration: self.iteration(),
            arrangement_offset: self.arrangement_offset(),
            slots_per_section: self.slots_per_section(),
        })
    }

    /// How many sections have been rendered.
    pub fn iteration(&self) -> u32 {
        // SAFETY: `raw` is a live session; the call only reads.
        let n = unsafe { ffi::audio_jam_session_iteration(self.raw.as_ptr()) };
        u32::try_from(n).unwrap_or(0)
    }

    /// Where the next section starts in the arrangement, in slots.
    pub fn arrangement_offset(&self) -> usize {
        // SAFETY: as above.
        let n = unsafe { ffi::audio_jam_session_arrangement_offset(self.raw.as_ptr()) };
        usize::try_from(n).unwrap_or(0)
    }

    /// How many arrangement slots a section covers.
    pub fn slots_per_section(&self) -> usize {
        // SAFETY: as above.
        let n = unsafe { ffi::audio_jam_session_slots_per_section(self.raw.as_ptr()) };
        usize::try_from(n).unwrap_or(0)
    }
}

impl Drop for JamSession {
    fn drop(&mut self) {
        // SAFETY: the session came from `audio_jam_session_open` and is
        // closed once, here.
        unsafe { ffi::audio_jam_session_close(self.raw.as_ptr()) };
    }
}

impl std::fmt::Debug for JamSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JamSession")
            .field("path", &self.path)
            .field("iteration", &self.iteration())
            .field("arrangement_offset", &self.arrangement_offset())
            .field("slots_per_section", &self.slots_per_section())
            .finish()
    }
}
