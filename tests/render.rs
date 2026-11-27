//! The render call's contract: its errors, its determinism, and that it
//! renders exactly what the engine rendered in BRYX, where it came from.

use sonomorph::chip8::{render_file, RenderError};

const RATE: u32 = 22_050;

fn probe_path(name: &str) -> String {
    format!("{}/tests/probes/{name}.abc", env!("CARGO_MANIFEST_DIR"))
}

/// FNV-1a over the samples as little-endian bytes: the engine's own
/// checksum (`audio_engine.c`), applied to the 16-bit render.
fn fnv1a(samples: &[i16]) -> u64 {
    samples
        .iter()
        .flat_map(|s| s.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// Each probe's length and checksum as BRYX's renderer produced them, at
/// BRYX commit a995311 (`audio/render.c`, the engine built with -O2,
/// rendered at 22,050 Hz). The engine moved here unchanged; a difference
/// is a change in what every score sounds like, and needs a reason.
const BRYX_RENDERS: &[(&str, usize, u64)] = &[
    ("accents", 44_100, 0xcb61_4c94_8054_fe02),
    ("bandlimit", 88_200, 0x585f_17c1_036f_b246),
    ("chord", 44_100, 0x54a4_a4e1_419d_dc52),
    ("duck", 44_100, 0x631e_242c_9243_1bcd),
    ("hold", 88_200, 0xcd50_75e7_4119_c1a1),
    ("kick", 44_100, 0x3798_2bd2_d355_2783),
    ("loop", 44_100, 0x1def_fb10_43c0_7c28),
    ("noise", 132_300, 0xbeb2_75cf_6b73_dccb),
    ("quiet", 44_100, 0xcee2_8731_8bf8_2482),
    ("ramp", 44_100, 0xd660_2d5e_246e_f0aa),
    ("reverb", 44_100, 0x46d0_fafa_252f_c247),
    ("struck", 88_200, 0xe2ca_575c_0bab_e5cb),
    ("tempo", 30_735, 0xd116_cbb4_d750_e9d5),
    ("velocity", 88_200, 0xef21_35ab_8d01_419e),
    ("wide", 385_875, 0x3b53_4a7a_5d61_1ba9),
];

#[test]
fn every_probe_renders_as_it_did_in_bryx() {
    assert!(!BRYX_RENDERS.is_empty());
    for &(name, len, hash) in BRYX_RENDERS {
        let samples = render_file(probe_path(name), RATE).unwrap();
        assert_eq!(
            (samples.len(), fnv1a(&samples)),
            (len, hash),
            "{name} renders differently from BRYX's engine"
        );
    }
}

#[test]
fn a_render_is_the_same_on_every_thread() {
    let reference = render_file(probe_path("chord"), RATE).unwrap();
    let renders: Vec<Vec<i16>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| render_file(probe_path("chord"), RATE).unwrap()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for render in renders {
        assert!(render == reference, "a concurrent render differs");
    }
}

#[test]
fn the_rate_sets_the_length() {
    let half = render_file(probe_path("tempo"), 22_050).unwrap().len() as f64;
    let full = render_file(probe_path("tempo"), 44_100).unwrap().len() as f64;
    assert!((full / half - 2.0).abs() < 0.001, "{half} then {full}");
}

#[test]
fn a_missing_file_is_an_io_error() {
    let err = render_file("no/such/score.abc", RATE).unwrap_err();
    assert!(matches!(err, RenderError::Io { .. }), "{err:?}");
    assert!(err.to_string().contains("no/such/score.abc"), "{err}");
}

#[test]
fn a_zero_or_huge_rate_is_refused() {
    for rate in [0, u32::MAX] {
        let err = render_file(probe_path("chord"), rate).unwrap_err();
        assert!(
            matches!(err, RenderError::SampleRate(r) if r == rate),
            "{err:?}"
        );
    }
}

#[test]
fn a_file_without_notes_renders_nothing_and_says_so() {
    let dir = std::env::temp_dir().join(format!("sonomorph-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("empty.abc");
    std::fs::write(&empty, "X:1\nT:nothing\nM:4/4\nL:1/16\nK:C\n").unwrap();
    let result = render_file(&empty, RATE);
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(result, Err(RenderError::Engine(_))), "{result:?}");
}
