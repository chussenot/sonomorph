//! The studio's render call: its errors, its determinism across threads,
//! the library lookup, and that it renders exactly what the studio rendered
//! in BRYX, where it came from.

use sonomorph::advanced::{render_score, render_score_with, RenderError, LIBRARY};

fn probe_path(name: &str) -> String {
    format!(
        "{}/tests/studio_probes/{name}.score",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// FNV-1a over bytes.
fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Each studio probe's frame count and the checksum of its float frames
/// (little-endian), as BRYX's renderer wrote them at BRYX commit 709ca61
/// (`audio/render.c`, `.f32` output, the score's own rate). The studio moved
/// here with one change, its state on the heap instead of in statics; a
/// difference here is a change in what every score sounds like.
const BRYX_RENDERS: &[(&str, usize, u64)] = &[
    ("studio_layers", 216_000, 0x4680_67e1_a6c6_290d),
    ("studio_limiter", 240_000, 0x0cd7_f1c7_8316_9662),
    ("studio_loop", 504_000, 0x0f34_7a15_1c5e_bae1),
    ("studio_reverb", 336_000, 0xf286_f9d1_655c_7d42),
    ("studio_seam", 192_000, 0xaebe_9a06_72e8_9877),
    ("studio_seating", 240_000, 0x161f_bdb1_c686_3e12),
    ("studio_sidechain", 216_000, 0xc28e_f74d_522b_d995),
    ("studio_tuning", 216_000, 0x9115_4827_6e30_7349),
];

#[test]
fn every_studio_probe_renders_as_it_did_in_bryx() {
    assert!(!BRYX_RENDERS.is_empty());
    for &(name, frames, hash) in BRYX_RENDERS {
        let render = render_score(probe_path(name), None).unwrap();
        let got = fnv1a(render.frames().iter().flat_map(|s| s.to_le_bytes()));
        assert_eq!(
            (render.frame_count(), got),
            (frames, hash),
            "{name} renders differently from BRYX's studio"
        );
    }
}

#[test]
fn renders_on_several_threads_at_once_are_each_exact() {
    // The studio once kept its state in statics; two renders at once would
    // have mixed. Each thread's render must equal the one made alone.
    let names = [
        "studio_tuning",
        "studio_reverb",
        "studio_loop",
        "studio_seating",
    ];
    let alone: Vec<_> = names
        .iter()
        .map(|n| render_score(probe_path(n), None).unwrap())
        .collect();
    let together: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|n| scope.spawn(move || render_score(probe_path(n), None).unwrap()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for ((name, a), b) in names.iter().zip(&alone).zip(&together) {
        assert!(a == b, "{name} differs when rendered alongside others");
    }
}

#[test]
fn the_library_is_where_at_library_paths_point() {
    assert!(std::path::Path::new(LIBRARY)
        .join("instruments/orchestra.inc")
        .exists());
    // Without a library, the same score cannot find its instruments.
    let err = render_score_with(probe_path("studio_seating"), None, None).unwrap_err();
    assert!(err.to_string().contains("@library"), "{err}");
}

#[test]
fn the_rate_can_be_set_or_left_to_the_score() {
    let own = render_score(probe_path("studio_tuning"), None).unwrap();
    assert_eq!(own.sample_rate(), 48_000);
    let half = render_score(probe_path("studio_tuning"), Some(24_000)).unwrap();
    assert_eq!(half.sample_rate(), 24_000);
    let ratio = own.frame_count() as f64 / half.frame_count() as f64;
    assert!((ratio - 2.0).abs() < 0.01, "{ratio}");
}

#[test]
fn qoa_and_16_bit_are_the_render_and_decode_back_to_it() {
    // `studio_seam` is a loop (`loop on`), `studio_loop` a held note.
    let render = render_score(probe_path("studio_seam"), None).unwrap();
    assert!(render.is_loop());
    assert!(!render_score(probe_path("studio_loop"), None)
        .unwrap()
        .is_loop());
    let s16 = render.to_s16();
    assert_eq!(s16.len(), render.frames().len());
    let qoa = render.to_qoa();
    let mut stream = sonomorph::advanced::qoa::QoaStream::new(&qoa).unwrap();
    assert_eq!(stream.len() as usize, render.frame_count());
    assert_eq!(stream.sample_rate(), 48_000);
    // QOA is lossy: decoded, it stays close to the 16-bit it encoded.
    let mut worst = 0i32;
    for frame in s16.chunks_exact(2) {
        let [l, r] = stream.next_frame().unwrap();
        worst = worst
            .max((i32::from(l) - i32::from(frame[0])).abs())
            .max((i32::from(r) - i32::from(frame[1])).abs());
    }
    assert!(worst < 2_000, "QOA error {worst}");
    // Deterministic, dither included.
    assert_eq!(render.to_qoa(), qoa);
    assert_eq!(render.to_s16(), s16);
}

#[test]
fn a_bad_score_says_where_and_why() {
    let dir = std::env::temp_dir().join(format!("sonomorph-studio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.score");
    std::fs::write(&bad, "score 1\nlength 4\nnote 0 1 nobody C4 100\n").unwrap();
    let result = render_score(&bad, None);
    std::fs::remove_dir_all(&dir).ok();
    let err = result.unwrap_err();
    assert!(matches!(err, RenderError::Score(_)), "{err:?}");
    assert!(err.to_string().contains("bad.score"), "{err}");

    let missing = render_score("no/such.score", None).unwrap_err();
    assert!(missing.to_string().contains("no/such.score"), "{missing}");
    assert!(matches!(
        render_score(probe_path("studio_tuning"), Some(u32::MAX)),
        Err(RenderError::SampleRate(_))
    ));
}
