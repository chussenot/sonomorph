//! What a program that edits or shows a score reads back: the parsed song
//! (`Song`) and the engine's own 8-bit render with its statistics.

use sonomorph::chip8::song::{FxBus, Waveform};
use sonomorph::chip8::{render_file, render_file_u8, RenderError, Song};

const RATE: u32 = 22_050;

fn song_path() -> String {
    format!("{}/tests/songs/arranged.abc", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn a_song_reads_back_what_its_score_declares() {
    let song = Song::load(song_path()).unwrap();
    assert_eq!(song.title, "Arranged");
    assert_eq!((song.bpm, song.step_ms, song.swing_pct), (120, 125, 56));

    let names: Vec<&str> = song.instruments.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["kick", "lead"]);
    let lead = &song.instruments[1];
    assert_eq!(lead.waveform, Waveform::Pulse);
    assert_eq!(
        (
            lead.amplitude,
            lead.duty_cycle,
            lead.attack_ms,
            lead.decay_ms
        ),
        (40, 25, 2, 60)
    );
    assert_eq!(
        (lead.sustain_level, lead.release_ms, lead.gate_percent),
        (60, 80, 80)
    );
    assert_eq!((lead.vibrato_cents, lead.glide_ms, lead.fx_bus), (10, 6, 1));
    assert_eq!(song.instruments[0].waveform, Waveform::Noise);

    let patterns: Vec<(&str, i32, bool)> = song
        .patterns
        .iter()
        .map(|p| (p.name.as_str(), p.length, p.defined))
        .collect();
    assert_eq!(
        patterns,
        [("A", 16, true), ("B", 16, true), ("C", 16, true)]
    );
    assert_eq!(song.arrangement, ["A", "B", "A", "C"]);

    assert_eq!(song.fx_bus_count, 2);
    assert!(
        song.fx_buses.len() >= 2,
        "every bus the engine has is listed"
    );
    assert_eq!(
        song.fx_buses[0],
        FxBus {
            enabled: true,
            drive_amount: 30,
            lowpass_amount: 20,
            sidechain_amount: 50,
            sidechain_release_ms: 150,
            mix_percent: 90,
            ..FxBus::default()
        }
    );
    assert_eq!(
        (
            song.fx_buses[1].delay_steps,
            song.fx_buses[1].delay_feedback,
            song.fx_buses[1].delay_mix,
            song.fx_buses[1].mix_percent
        ),
        (4, 40, 30, 70)
    );
    assert!(song.fx_buses[2..].iter().all(|bus| !bus.enabled));

    let voices: Vec<(&str, &str)> = song
        .voices
        .iter()
        .map(|v| (v.name.as_str(), v.instrument.as_str()))
        .collect();
    assert_eq!(voices, [("kick", "kick"), ("lead", "lead")]);
    for voice in &song.voices {
        assert_eq!(voice.steps.len(), 48, "{}: three bars of 16", voice.name);
    }
    // The lead's first bar: C4 E4 G4 c4, four steps each.
    let lead = &song.voices[1];
    let c4 = 440.0 * 2f64.powf(-9.0 / 12.0);
    for (step, semitones) in [(0, 0.0), (4, 4.0), (8, 7.0), (12, 12.0)] {
        let want = c4 * 2f64.powf(semitones / 12.0);
        for (held, &hz) in lead.steps[step..step + 4].iter().enumerate() {
            assert!(
                (hz - want).abs() < 0.01,
                "step {}: {hz} Hz, want {want}",
                step + held
            );
        }
    }
    // The kick strikes on steps 0 and 8 of its first bar and rests between.
    let kick = &song.voices[0].steps;
    assert!(kick[0] > 0.0 && kick[8] > 0.0);
    assert!(kick[2..8].iter().all(|&f| f == 0.0));
}

#[test]
fn a_song_that_cannot_be_read_is_an_io_error() {
    let err = Song::load("no/such/score.abc").unwrap_err();
    assert!(matches!(err, RenderError::Io { .. }), "{err:?}");
}

#[test]
fn a_song_loads_the_same_on_every_thread() {
    let alone = Song::load(song_path()).unwrap();
    let loaded: Vec<Song> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| Song::load(song_path()).unwrap()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(loaded.iter().all(|song| *song == alone));
}

#[test]
fn the_8_bit_render_is_the_16_bit_render_dithered_once() {
    let (bytes, _) = render_file_u8(song_path(), RATE).unwrap();
    let wide = render_file(song_path(), RATE).unwrap();
    assert_eq!(bytes.len(), wide.len());
    // Triangular dither spans one step either side, rounding half of one.
    for (i, (&b, &s)) in bytes.iter().zip(&wide).enumerate() {
        let exact = 128.0 + f64::from(s) / 256.0;
        assert!(
            (f64::from(b) - exact).abs() <= 1.5,
            "sample {i}: {b}, from {s}"
        );
    }
    // And the dither is seeded: the same render twice is the same bytes.
    assert_eq!(render_file_u8(song_path(), RATE).unwrap().0, bytes);
}

#[test]
fn the_statistics_describe_the_samples() {
    let (bytes, stats) = render_file_u8(song_path(), RATE).unwrap();
    assert_eq!(stats.sample_count, bytes.len() as u64);
    let seconds = bytes.len() as f64 / f64::from(RATE);
    assert!((stats.duration_ms - seconds * 1000.0).abs() < 1e-6);
    let min = i32::from(*bytes.iter().min().unwrap());
    let max = i32::from(*bytes.iter().max().unwrap());
    assert_eq!((stats.min_sample, stats.max_sample), (min, max));
    assert_eq!(stats.peak, (128 - min).max(max - 128));
    // The engine's basis, not FNV's standard 14695981039346656037.
    let fnv = bytes
        .iter()
        .fold(1_469_598_103_934_665_603_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    assert_eq!(stats.checksum, fnv);
    assert_eq!(
        stats.clipping_count, 0,
        "the fixture is mixed below the rails"
    );
    assert!(stats.render_time_ms >= 0.0);
}

#[test]
fn the_8_bit_render_refuses_what_the_16_bit_one_does() {
    assert!(matches!(
        render_file_u8("no/such/score.abc", RATE),
        Err(RenderError::Io { .. })
    ));
    assert!(matches!(
        render_file_u8(song_path(), 0),
        Err(RenderError::SampleRate(0))
    ));
}
