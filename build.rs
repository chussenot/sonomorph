//! Builds the engines' C into static libraries:
//! - `render`: the chip8 engine (`engine/chip8/`, see its PROVENANCE.md);
//!   `jam` and `midi` add their own source files to it;
//! - `render-advanced`: the studio (`engine/advanced/`) with its vendored
//!   FLAC decoder and QOA encoder (`engine/vendor/`).
//!
//! The flags are the ones BRYX built its renderer with (ISO C99, the POSIX
//! feature macros, -O2), so a score renders to the same samples here as it
//! did there: the integration test `render.rs` holds a reference checksum.

fn main() {
    #[cfg(feature = "render")]
    build_chip8();
    #[cfg(feature = "render-advanced")]
    build_studio();
}

#[cfg(feature = "render")]
fn build_chip8() {
    let mut sources = vec![
        "abc.c",
        "audio_dsp.c",
        "audio_engine.c",
        "audio_fx.c",
        "audio_mix.c",
        "audio_seq.c",
        "audio_song_builtin.c",
        "sonomorph_song.c",
    ];
    if cfg!(feature = "jam") {
        sources.push("audio_jam.c");
    }
    if cfg!(feature = "midi") {
        sources.push("midi_export.c");
    }
    println!("cargo:rerun-if-changed=engine/chip8");

    let dir = std::path::Path::new("engine/chip8");
    let mut build = cc::Build::new();
    build
        .include(dir)
        .files(sources.iter().map(|file| dir.join(file)))
        // Whatever profile builds this crate (a build-dependency is
        // unoptimized by default), the engine renders at full speed.
        .opt_level(2)
        .warnings(false);
    if !build.get_compiler().is_like_msvc() {
        build
            .std("c99")
            .define("_DEFAULT_SOURCE", None)
            .define("_XOPEN_SOURCE", "600");
    }
    build.compile("sonomorph_chip8");

    // The engine calls libm (sin, pow, tanh). MSVC's runtime has them.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        println!("cargo:rustc-link-lib=m");
    }
}

#[cfg(feature = "render-advanced")]
fn build_studio() {
    const SOURCES: &[&str] = &[
        "studio.c",
        "studio_common.c",
        "studio_fx.c",
        "studio_sampler.c",
        "studio_synth.c",
        "sonomorph_studio.c",
    ];
    println!("cargo:rerun-if-changed=engine/advanced");
    println!("cargo:rerun-if-changed=engine/vendor");

    let dir = std::path::Path::new("engine/advanced");
    let mut build = cc::Build::new();
    build
        .include(dir)
        .files(SOURCES.iter().map(|file| dir.join(file)))
        // BRYX built its renderer at -O2; a studio render takes seconds even
        // so, and it must render the same bytes here.
        .opt_level(2)
        .warnings(false);
    if !build.get_compiler().is_like_msvc() {
        build
            .std("c99")
            .define("_DEFAULT_SOURCE", None)
            .define("_XOPEN_SOURCE", "600");
    }
    build.compile("sonomorph_studio");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        println!("cargo:rustc-link-lib=m");
    }
}
