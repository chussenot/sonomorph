//! Jam sessions: a score's arrangement played on, section by section, each
//! a seeded variation.

use sonomorph::chip8::{JamSection, JamSession, RenderError};

const RATE: u32 = 22_050;

fn song_path() -> String {
    format!("{}/tests/songs/arranged.abc", env!("CARGO_MANIFEST_DIR"))
}

fn sections(seed: u64, n: usize) -> Vec<JamSection> {
    let mut jam = JamSession::open(song_path(), seed, 4.0).unwrap();
    (0..n).map(|_| jam.render_next(RATE).unwrap()).collect()
}

#[test]
fn the_same_seed_plays_the_same_sections() {
    let first = sections(0xC0FFEE, 4);
    assert_eq!(sections(0xC0FFEE, 4), first);
    assert_ne!(
        sections(0xBEEF, 4)
            .into_iter()
            .map(|s| s.samples)
            .collect::<Vec<_>>(),
        first.into_iter().map(|s| s.samples).collect::<Vec<_>>(),
        "another seed varies differently"
    );
}

#[test]
fn sections_count_up_and_scroll_the_arrangement() {
    let played = sections(7, 3);
    // Four seconds at 120 BPM in sixteenths is 32 steps: two patterns.
    for (n, section) in played.iter().enumerate() {
        assert_eq!(section.iteration, n as u32 + 1);
        assert_eq!(section.slots_per_section, 2);
        assert_eq!(section.arrangement_offset, 2 * (n + 1));
        assert!(!section.samples.is_empty());
    }
}

#[test]
fn a_section_length_out_of_range_is_clamped() {
    for seconds in [f64::NAN, -1.0, 0.0] {
        let jam = JamSession::open(song_path(), 1, seconds).unwrap();
        assert_eq!(jam.slots_per_section(), 1, "{seconds}");
    }
    let jam = JamSession::open(song_path(), 1, f64::INFINITY).unwrap();
    assert_eq!(jam.slots_per_section(), 4, "the whole arrangement");
}

#[test]
fn a_session_plays_the_same_on_another_thread() {
    let here = sections(42, 2);
    let mut jam = JamSession::open(song_path(), 42, 4.0).unwrap();
    let there = std::thread::spawn(move || {
        (0..2)
            .map(|_| jam.render_next(RATE).unwrap())
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    assert_eq!(there, here);
}

#[test]
fn a_score_without_an_arrangement_jams_on_the_whole_score() {
    // The sequencer gives an unarranged score one pattern of all its
    // steps, so there is always something to vary.
    let path = format!("{}/tests/probes/chord.abc", env!("CARGO_MANIFEST_DIR"));
    let mut jam = JamSession::open(&path, 1, 4.0).unwrap();
    assert_eq!(jam.slots_per_section(), 1);
    assert!(!jam.render_next(RATE).unwrap().samples.is_empty());
}

#[test]
fn a_score_that_cannot_jam_says_why() {
    let dir = std::env::temp_dir().join(format!("sonomorph-jam-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("empty.abc");
    std::fs::write(&empty, "X:1\nT:nothing\nM:4/4\nL:1/16\nK:C\n").unwrap();
    let result = JamSession::open(&empty, 1, 4.0);
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(result, Err(RenderError::Parse(_))), "{result:?}");
    let err = JamSession::open("no/such/score.abc", 1, 4.0).unwrap_err();
    assert!(matches!(err, RenderError::Io { .. }), "{err:?}");
}

#[test]
fn a_bad_rate_is_refused_and_the_session_goes_on() {
    let mut jam = JamSession::open(song_path(), 3, 4.0).unwrap();
    assert!(matches!(
        jam.render_next(0),
        Err(RenderError::SampleRate(0))
    ));
    assert_eq!(jam.iteration(), 0, "nothing was rendered");
    assert_eq!(jam.render_next(RATE).unwrap().iteration, 1);
}
