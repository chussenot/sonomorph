//! Builds the chip8 engine (`engine/chip8/`, see its PROVENANCE.md) into a
//! static library, with the `render` feature only.
//!
//! The flags are the ones BRYX built its renderer with (ISO C99, the POSIX
//! feature macros, -O2), so a score renders to the same samples here as it
//! did there: the integration test `render.rs` holds a reference checksum.

fn main() {
    #[cfg(feature = "render")]
    build_chip8();
}

#[cfg(feature = "render")]
fn build_chip8() {
    const SOURCES: &[&str] = &[
        "abc.c",
        "audio_dsp.c",
        "audio_engine.c",
        "audio_fx.c",
        "audio_mix.c",
        "audio_seq.c",
        "audio_song_builtin.c",
    ];
    println!("cargo:rerun-if-changed=engine/chip8");

    let dir = std::path::Path::new("engine/chip8");
    let mut build = cc::Build::new();
    build
        .include(dir)
        .files(SOURCES.iter().map(|file| dir.join(file)))
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
