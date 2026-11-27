# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html); in
0.x, a minor release may break the API.

## [Unreleased]

### Added

- `sonomorph::chip8`, extracted from BRYX (`chussenot/bryx` at
  `a995311`):
  - the Electric Pulse engine as BRYX forked it (`engine/chip8/`),
    behind `render_file` and the `render` feature, with one change:
    `%%arrangement` is split without `strtok`, so renders on several
    threads at once cannot cross (`tests/render.rs` holds it);
  - G.711 mu-law (`chip8::mulaw`), as BRYX stored its cues;
  - the mixer (`Mixer`, `Command`, `MixerConfig`), generic over a game's
    `Sound` instead of BRYX's own cue list, with BRYX's levels as its
    defaults;
  - the device stream (`Output`, the `output` feature) through cpal.
- The engine probes, one score per feature with a measurement each
  (`tests/probes.rs`).
- A golden test: every probe renders to the same length and checksum as
  BRYX's renderer did (`tests/render.rs`). At extraction, all 56 of BRYX's
  cues and 15 probes were also compared sample for sample (71 identical),
  and the mu-law encoder over every 16-bit value (identical).
- `sonomorph-render` (with `render`): a score to WAV or raw 16-bit, the
  interface of BRYX's old renderer tool; `examples/play.rs`;
  `docs/chip8.md`.
- What Electric Pulse's editor needs, so it can use the engine from here
  instead of its own copy (`docs/chip8.md`, "Working on scores"):
  - `render_file_u8` and `RenderStats` (`render`): the engine's 8-bit
    output and its statistics;
  - `Song` (`render`): a parsed score, read through views Sonomorph owns
    (`engine/chip8/sonomorph_song.[ch]`) rather than the parser's layout;
  - `JamSession` (`jam`) and `export_midi` (`midi`): Electric Pulse's jam
    generator and MIDI exporter, from its commit `06c79fe`, with their
    large structures moved off the stack;
  - `RenderError::Parse`, for a file the engine reads but cannot use;
  - the engine's C tests from Electric Pulse (`engine/chip8/tests/`), run
    by `cargo test` and, in CI, under ASan and UBSan. Their built-in song
    checksums are re-recorded for the fork's 16-bit mix (each of the first
    16 bytes within two steps of the original), and the drive clamp is
    checked at the fork's scale.
