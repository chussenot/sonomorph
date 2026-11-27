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
- `examples/render_wav.rs` and `examples/play.rs`; `docs/chip8.md`.
