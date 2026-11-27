//! A score as a Standard MIDI File.

use std::path::{Path, PathBuf};

use sonomorph::chip8::{export_midi, RenderError};

fn song_path() -> String {
    format!("{}/tests/songs/arranged.abc", env!("CARGO_MANIFEST_DIR"))
}

/// A directory of its own for each test, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("sonomorph-midi-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn join(&self, file: &str) -> PathBuf {
        self.0.join(file)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The chunks of a Standard MIDI File: (type, body).
fn chunks(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        out.push((&bytes[at..at + 4], &bytes[at + 8..at + 8 + len]));
        at += 8 + len;
    }
    assert_eq!(at, bytes.len(), "the chunks fill the file exactly");
    out
}

#[test]
fn a_score_exports_one_track_per_voice() {
    let scratch = Scratch::new("tracks");
    let out = scratch.join("arranged.mid");
    let stats = export_midi(song_path(), &out).unwrap();
    assert_eq!(stats.track_count, 2);
    assert_eq!(stats.tempo_bpm, 120);
    assert!(stats.note_count > 0);

    let bytes = std::fs::read(&out).unwrap();
    let chunks = chunks(&bytes);
    let (kind, header) = chunks[0];
    assert_eq!(kind, b"MThd");
    let word = |i: usize| u16::from_be_bytes([header[i], header[i + 1]]);
    assert_eq!(word(0), 1, "format 1");
    assert_eq!(usize::from(word(2)), chunks.len() - 1);
    assert_eq!(u32::from(word(4)), stats.ticks_per_quarter);
    assert!(chunks[1..].iter().all(|(kind, _)| *kind == b"MTrk"));
    // A conductor track (the tempo), then one per voice.
    assert_eq!(chunks.len(), 2 + stats.track_count as usize);
}

#[test]
fn an_export_is_the_same_on_every_thread() {
    let scratch = Scratch::new("threads");
    export_midi(song_path(), scratch.join("alone.mid")).unwrap();
    let alone = std::fs::read(scratch.join("alone.mid")).unwrap();
    std::thread::scope(|scope| {
        for n in 0..8 {
            let out = scratch.join(&format!("{n}.mid"));
            scope.spawn(move || export_midi(song_path(), &out).unwrap());
        }
    });
    for n in 0..8 {
        assert_eq!(
            std::fs::read(scratch.join(&format!("{n}.mid"))).unwrap(),
            alone
        );
    }
}

#[test]
fn a_missing_score_is_an_io_error_and_writes_nothing() {
    let scratch = Scratch::new("missing");
    let out = scratch.join("out.mid");
    let err = export_midi("no/such/score.abc", &out).unwrap_err();
    assert!(
        matches!(&err, RenderError::Io { path, .. } if path == Path::new("no/such/score.abc")),
        "{err:?}"
    );
    assert!(!out.exists());
}

#[test]
fn an_unwritable_output_is_an_io_error_naming_it() {
    let scratch = Scratch::new("unwritable");
    let out = scratch.join("no/such/dir/out.mid");
    let err = export_midi(song_path(), &out).unwrap_err();
    assert!(
        matches!(&err, RenderError::Io { path, .. } if *path == out),
        "{err:?}"
    );
}

#[test]
fn a_failed_export_leaves_the_output_as_it_was() {
    let scratch = Scratch::new("empty");
    let score = scratch.join("empty.abc");
    std::fs::write(&score, "X:1\nT:nothing\nM:4/4\nL:1/16\nK:C\n").unwrap();
    let out = scratch.join("empty.mid");
    std::fs::write(&out, "an earlier export").unwrap();
    let err = export_midi(&score, &out).unwrap_err();
    assert!(matches!(err, RenderError::Parse(_)), "{err:?}");
    assert_eq!(std::fs::read(&out).unwrap(), b"an earlier export");
    let left: Vec<_> = std::fs::read_dir(&scratch.0).unwrap().collect();
    assert_eq!(left.len(), 2, "no temporary file is left: {left:?}");
}

#[test]
fn a_score_can_be_exported_over_itself() {
    let scratch = Scratch::new("itself");
    let score = scratch.join("arranged.abc");
    std::fs::copy(song_path(), &score).unwrap();
    let stats = export_midi(&score, &score).unwrap();
    assert_eq!(stats.track_count, 2);
    assert!(std::fs::read(&score).unwrap().starts_with(b"MThd"));
}
