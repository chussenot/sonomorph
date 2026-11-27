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

/// Each studio probe as BRYX's renderer wrote it at BRYX commit 709ca61
/// (`audio/render.c`, `.f32` output, the score's own rate): its frame
/// count, and the RMS of every quarter second (12,000 frames) of each
/// channel.
///
/// A fingerprint, not a checksum: the studio computes in float and calls
/// libm, whose results glibc picks per CPU (its FMA variants), so a render
/// is the same on one machine but can differ in the last bit between two.
/// That is far under these tolerances; a change to how a score sounds is
/// far over them. At the move, renders on one machine were also compared
/// byte for byte with BRYX's (see PROVENANCE.md).
const BRYX_FINGERPRINTS: &[(&str, usize, &[[f32; 2]])] = &[
    (
        "studio_layers",
        216_000,
        &[
            [1.061497e-3, 1.061497e-3],
            [3.237221e-3, 3.237221e-3],
            [5.219752e-3, 5.219752e-3],
            [4.850149e-3, 4.850149e-3],
            [5.833297e-3, 5.833297e-3],
            [7.748293e-3, 7.748293e-3],
            [9.553406e-3, 9.553406e-3],
            [3.210006e-3, 3.210006e-3],
            [6.80677e-2, 6.80677e-2],
            [1.489664e-1, 1.489664e-1],
            [1.15377e-1, 1.15377e-1],
            [1.839599e-1, 1.839599e-1],
            [1.51675e-1, 1.51675e-1],
            [1.563263e-1, 1.563263e-1],
            [1.722738e-1, 1.722738e-1],
            [4.582992e-2, 4.582992e-2],
            [0e0, 0e0],
            [0e0, 0e0],
        ],
    ),
    (
        "studio_limiter",
        240_000,
        &[
            [2.753819e-1, 3.034899e-1],
            [2.774023e-1, 3.039661e-1],
            [2.982636e-1, 2.686185e-1],
            [2.86004e-1, 2.968471e-1],
            [2.802165e-1, 3.041892e-1],
            [2.557359e-1, 3.189566e-1],
            [2.742643e-1, 3.109129e-1],
            [2.757615e-1, 3.075484e-1],
            [2.760557e-1, 2.978505e-1],
            [2.86912e-1, 2.510116e-1],
            [2.734485e-1, 2.940377e-1],
            [2.412183e-1, 3.129844e-1],
            [2.498503e-1, 3.060693e-1],
            [2.600094e-1, 2.882578e-1],
            [3.055258e-1, 2.697225e-1],
            [3.162948e-1, 2.866515e-1],
            [2.83971e-1, 2.251203e-1],
            [2.067006e-1, 2.386404e-1],
            [2.02137e-1, 1.812697e-1],
            [1.735812e-1, 1.014312e-1],
        ],
    ),
    (
        "studio_loop",
        504_000,
        &[
            [1.204985e-1, 1.204985e-1],
            [1.654057e-1, 1.654057e-1],
            [1.585937e-1, 1.585937e-1],
            [1.743842e-1, 1.743842e-1],
            [1.660103e-1, 1.660103e-1],
            [1.391391e-1, 1.391391e-1],
            [1.496524e-1, 1.496524e-1],
            [1.633841e-1, 1.633841e-1],
            [1.499617e-1, 1.499617e-1],
            [1.587432e-1, 1.587432e-1],
            [1.829994e-1, 1.829994e-1],
            [1.731525e-1, 1.731525e-1],
            [1.40303e-1, 1.40303e-1],
            [1.482316e-1, 1.482316e-1],
            [1.620656e-1, 1.620656e-1],
            [1.523924e-1, 1.523924e-1],
            [1.578398e-1, 1.578398e-1],
            [1.744581e-1, 1.744581e-1],
            [1.806748e-1, 1.806748e-1],
            [1.406023e-1, 1.406023e-1],
            [1.513613e-1, 1.513613e-1],
            [1.600077e-1, 1.600077e-1],
            [1.553663e-1, 1.553663e-1],
            [1.538534e-1, 1.538534e-1],
            [1.662405e-1, 1.662405e-1],
            [1.830694e-1, 1.830694e-1],
            [1.473358e-1, 1.473358e-1],
            [1.551269e-1, 1.551269e-1],
            [1.557733e-1, 1.557733e-1],
            [1.551056e-1, 1.551056e-1],
            [1.55307e-1, 1.55307e-1],
            [1.635052e-1, 1.635052e-1],
            [1.778764e-1, 1.778764e-1],
            [1.574196e-1, 1.574196e-1],
            [1.507206e-1, 1.507206e-1],
            [1.535765e-1, 1.535765e-1],
            [1.585672e-1, 1.585672e-1],
            [1.536586e-1, 1.536586e-1],
            [1.657624e-1, 1.657624e-1],
            [1.772104e-1, 1.772104e-1],
            [6.104072e-2, 6.104072e-2],
            [5.124763e-4, 5.124763e-4],
        ],
    ),
    (
        "studio_reverb",
        336_000,
        &[
            [1.400468e-2, 1.395215e-2],
            [3.590829e-3, 3.388056e-3],
            [1.408559e-3, 1.389523e-3],
            [5.830566e-4, 5.999922e-4],
            [2.56623e-4, 2.580545e-4],
            [1.124121e-4, 1.188375e-4],
            [5.018179e-5, 5.076967e-5],
            [2.210193e-5, 2.247497e-5],
            [1.024288e-5, 1.009046e-5],
            [4.533636e-6, 4.82337e-6],
            [2.179517e-6, 2.25078e-6],
            [9.911394e-7, 9.728618e-7],
            [4.511388e-7, 4.482883e-7],
            [2.029735e-7, 2.069643e-7],
            [9.400908e-8, 9.699945e-8],
            [4.48425e-8, 4.369952e-8],
            [2.100798e-8, 2.072877e-8],
            [1.00055e-8, 9.974345e-9],
            [4.55914e-9, 4.693352e-9],
            [2.170311e-9, 2.082089e-9],
            [1.037515e-9, 1.04181e-9],
            [4.713699e-10, 4.954022e-10],
            [2.354643e-10, 2.408521e-10],
            [1.097154e-10, 1.121831e-10],
            [5.158781e-11, 5.031155e-11],
            [2.419655e-11, 2.460042e-11],
            [1.195916e-11, 1.165073e-11],
            [5.630402e-12, 5.538701e-12],
        ],
    ),
    (
        "studio_seam",
        192_000,
        &[
            [2.485175e-1, 2.004484e-1],
            [2.000568e-1, 2.107843e-1],
            [1.656314e-1, 1.582542e-1],
            [1.232122e-1, 1.270898e-1],
            [9.346932e-2, 9.250615e-2],
            [3.944873e-2, 4.212881e-2],
            [4.323244e-2, 3.332536e-2],
            [3.014905e-2, 2.557166e-2],
            [1.90027e-2, 1.531956e-2],
            [1.826001e-2, 1.496761e-2],
            [9.308872e-3, 1.160075e-2],
            [8.594295e-3, 6.561171e-3],
            [8.453168e-1, 1.099287e0],
            [8.177675e-1, 1.066778e0],
            [9.285498e-1, 1.261024e0],
            [3.476806e-1, 6.488656e-1],
        ],
    ),
    (
        "studio_seating",
        240_000,
        &[
            [5.193629e-2, 2.460126e-2],
            [9.510938e-2, 4.244672e-2],
            [7.159509e-2, 4.123714e-2],
            [1.037032e-1, 4.597632e-2],
            [8.996606e-2, 4.435871e-2],
            [9.834928e-2, 4.248014e-2],
            [5.177781e-2, 2.442322e-2],
            [1.117691e-2, 9.704504e-3],
            [4.602562e-2, 8.736475e-2],
            [5.358514e-2, 1.00482e-1],
            [5.740807e-2, 9.497736e-2],
            [5.347774e-2, 8.449097e-2],
            [5.800339e-2, 9.378487e-2],
            [5.905171e-2, 9.431705e-2],
            [3.940522e-2, 6.827625e-2],
            [1.708148e-2, 1.089268e-2],
            [8.56036e-3, 5.085676e-3],
            [3.907792e-3, 2.520838e-3],
            [1.619738e-3, 1.0004e-3],
            [9.09022e-4, 4.740085e-4],
        ],
    ),
    (
        "studio_sidechain",
        216_000,
        &[
            [6.850579e-1, 6.850579e-1],
            [4.323633e-1, 4.323633e-1],
            [2.738152e-1, 2.738152e-1],
            [3.511338e-1, 3.511338e-1],
            [2.258443e-1, 2.258443e-1],
            [2.158834e-1, 2.158834e-1],
            [4.137567e-1, 4.137567e-1],
            [6.754999e-1, 6.754999e-1],
            [2.545406e-1, 2.545406e-1],
            [3.032439e-1, 3.032439e-1],
            [2.592647e-1, 2.592647e-1],
            [3.724131e-1, 3.724131e-1],
            [2.200065e-1, 2.200065e-1],
            [2.253268e-1, 2.253268e-1],
            [4.418266e-1, 4.418266e-1],
            [7.028782e-1, 7.028782e-1],
            [1.50524e-1, 1.50524e-1],
            [0e0, 0e0],
        ],
    ),
    (
        "studio_tuning",
        216_000,
        &[
            [5.692779e-2, 5.692779e-2],
            [9.225546e-2, 9.225546e-2],
            [9.212792e-2, 9.212792e-2],
            [1.171192e-1, 1.171192e-1],
            [1.172632e-1, 1.172632e-1],
            [1.086955e-1, 1.086955e-1],
            [1.192655e-1, 1.192655e-1],
            [3.471419e-2, 3.471419e-2],
            [5.557965e-1, 5.557965e-1],
            [5.567769e-1, 5.567769e-1],
            [5.567769e-1, 5.567769e-1],
            [5.567769e-1, 5.567769e-1],
            [5.567769e-1, 5.567769e-1],
            [5.567769e-1, 5.567769e-1],
            [5.567769e-1, 5.567769e-1],
            [8.060466e-2, 8.060466e-2],
            [0e0, 0e0],
            [0e0, 0e0],
        ],
    ),
];

/// The fingerprint of a render, as above.
fn fingerprint(frames: &[f32]) -> Vec<[f32; 2]> {
    frames
        .chunks(2 * 12_000)
        .map(|block| {
            let n = (block.len() / 2) as f64;
            let rms = |c: usize| {
                let sum: f64 = block
                    .iter()
                    .skip(c)
                    .step_by(2)
                    .map(|&s| f64::from(s).powi(2))
                    .sum();
                (sum / n).sqrt() as f32
            };
            [rms(0), rms(1)]
        })
        .collect()
}

#[test]
fn every_studio_probe_renders_as_it_did_in_bryx() {
    assert!(!BRYX_FINGERPRINTS.is_empty());
    for &(name, frames, want) in BRYX_FINGERPRINTS {
        let render = render_score(probe_path(name), None).unwrap();
        assert_eq!(render.frame_count(), frames, "{name}: length");
        let got = fingerprint(render.frames());
        assert_eq!(got.len(), want.len(), "{name}: blocks");
        for (block, (g, w)) in got.iter().zip(want).enumerate() {
            for c in 0..2 {
                // 0.1% of the level, and a floor for near-silent blocks.
                let tolerance = w[c] * 1e-3 + 1e-6;
                assert!(
                    (g[c] - w[c]).abs() <= tolerance,
                    "{name}: block {block}, channel {c}: RMS {} against BRYX's {}",
                    g[c],
                    w[c]
                );
            }
        }
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
    for [want_l, want_r] in s16.as_chunks::<2>().0 {
        let [l, r] = stream.next_frame().unwrap();
        worst = worst
            .max((i32::from(l) - i32::from(*want_l)).abs())
            .max((i32::from(r) - i32::from(*want_r)).abs());
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
