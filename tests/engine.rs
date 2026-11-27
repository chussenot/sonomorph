//! The engine's own C tests (`engine/chip8/tests/`), which came with it
//! from Electric Pulse: oscillators, envelopes, the sequencer's timing, the
//! FX units, and the built-in song's render, checked from inside.
//!
//! Each is compiled with the C compiler the `render` feature already needs
//! (`$CC`, or `cc`) and the flags `build.rs` uses, then run. With
//! `SONOMORPH_C_SANITIZE` set (CI sets it), they run under AddressSanitizer
//! and UndefinedBehaviorSanitizer as well.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

const ENGINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/engine/chip8");

/// What each test links besides itself.
const ENGINE_SOURCES: &[&str] = &[
    "abc.c",
    "audio_dsp.c",
    "audio_engine.c",
    "audio_fx.c",
    "audio_mix.c",
    "audio_seq.c",
    "audio_song_builtin.c",
];

fn build_and_run(test: &str, sources: &[&str]) {
    let engine = Path::new(ENGINE);
    let binary = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    let compiler = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let mut cc = Command::new(&compiler);
    cc.args(["-std=c99", "-O2"])
        .args(["-D_DEFAULT_SOURCE", "-D_XOPEN_SOURCE=600"])
        .arg("-I")
        .arg(engine)
        .arg("-o")
        .arg(&binary)
        .arg(engine.join("tests").join(format!("{test}.c")))
        .args(sources.iter().map(|file| engine.join(file)));
    if std::env::var_os("SONOMORPH_C_SANITIZE").is_some() {
        cc.args([
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
            "-fno-omit-frame-pointer",
        ]);
    }
    cc.arg("-lm");
    let built = cc
        .output()
        .unwrap_or_else(|e| panic!("{test}: cannot run {compiler}: {e}"));
    assert!(
        built.status.success(),
        "{test} does not compile:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let ran = Command::new(&binary).output().unwrap();
    let output = String::from_utf8_lossy(&ran.stdout);
    assert!(
        ran.status.success(),
        "{test} failed:\n{output}\n{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert!(output.contains("All tests passed."), "{test}:\n{output}");
}

#[test]
fn the_dsp_tests_pass() {
    build_and_run("test_audio_dsp", &["audio_dsp.c"]);
}

#[test]
fn the_sequencer_and_fx_tests_pass() {
    build_and_run("test_audio_seq", ENGINE_SOURCES);
}
