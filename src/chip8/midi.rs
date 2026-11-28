//! A score as a Standard MIDI File (`midi` feature): parsed, sequenced and
//! written, never rendered. A conductor track with the tempo, then one
//! track per voice, notes and velocities as the sequencer holds them.

use std::os::raw::c_int;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::render::{c_path, existing_c_path, RenderError};

mod ffi {
    use std::os::raw::{c_char, c_int};

    #[repr(C)]
    #[derive(Default)]
    pub struct MidiExportStats {
        pub track_count: c_int,
        pub note_count: c_int,
        pub ticks_per_quarter: c_int,
        pub tempo_bpm: c_int,
    }

    extern "C" {
        pub fn midi_export_abc_file(
            abc_path: *const c_char,
            out_path: *const c_char,
            out_stats: *mut MidiExportStats,
        ) -> c_int;
    }
}

/// What an export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MidiExportStats {
    /// Voice tracks, one per voice; the file also holds a conductor track
    /// (the tempo) before them.
    pub track_count: u32,
    /// Notes (note-on and note-off pairs).
    pub note_count: u32,
    /// Resolution, in ticks per quarter note.
    pub ticks_per_quarter: u32,
    /// The tempo written, in beats per minute.
    pub tempo_bpm: u32,
}

/// Writes the ABC score at `abc_path` as a format 1 Standard MIDI File at
/// `out_path`, replacing it.
///
/// The file is written beside `out_path` under a temporary name and moved
/// into place once complete, so a failed export leaves whatever was at
/// `out_path` as it was, and the output may even be the score itself.
///
/// # Errors
///
/// [`RenderError::Io`] when the score cannot be read or the output cannot
/// be written; [`RenderError::Path`] when either path cannot be named to C
/// (or the output names no file); [`RenderError::Parse`] when the engine
/// cannot parse the score or it holds no notes.
pub fn export_midi(
    abc_path: impl AsRef<Path>,
    out_path: impl AsRef<Path>,
) -> Result<MidiExportStats, RenderError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (abc_path, out_path) = (abc_path.as_ref(), out_path.as_ref());
    let c_abc = existing_c_path(abc_path)?;
    let name = out_path
        .file_name()
        .ok_or_else(|| RenderError::Path(out_path.to_owned()))?;
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let temp = out_path.with_file_name(temp_name);
    let c_temp = c_path(&temp)?;
    let io = |source| RenderError::Io {
        path: out_path.to_owned(),
        source,
    };
    // The engine answers a parse failure and a write failure with the same
    // code; creating the file here first gives the second its own error.
    std::fs::File::create(&temp).map_err(io)?;

    let mut stats = ffi::MidiExportStats::default();
    // SAFETY: two valid NUL-terminated paths and a valid place for the
    // statistics, all outliving the call.
    let rc = unsafe { ffi::midi_export_abc_file(c_abc.as_ptr(), c_temp.as_ptr(), &mut stats) };
    let moved = if rc == 0 {
        std::fs::rename(&temp, out_path).map_err(io)
    } else {
        Err(RenderError::Parse(abc_path.to_owned()))
    };
    if let Err(err) = moved {
        // Best effort: the temporary file is not a MIDI file.
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    let count = |n: c_int| u32::try_from(n).unwrap_or(0);
    Ok(MidiExportStats {
        track_count: count(stats.track_count),
        note_count: count(stats.note_count),
        ticks_per_quarter: count(stats.ticks_per_quarter),
        tempo_bpm: count(stats.tempo_bpm),
    })
}
