# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html); in
0.x, a minor release may break the API.

## [Unreleased]

### Added

- `sonomorph::advanced` (in development), extracted from BRYX's
  orchestral-score branch (`709ca61`):
  - the studio (`engine/advanced/`), behind `render_score` and the
    `render-advanced` feature, made reentrant (its state moved from
    statics to the heap, its tokenizer off `strtok`) and given the
    `@library/` path prefix;
  - the instrument library (`library/`): the desk, orchestra and synth
    palettes, and the curated VSCO 2 CE samples (CC0);
  - `Render::to_qoa` and `Render::to_s16`, BRYX's dither and seamless QOA
    encode; `fold_effect`, its effect fold-down; the QOA decoder
    (`advanced::qoa`);
  - the advanced mixer (`Mixer`, `Sound`, `Clip`): QOA streamed and
    resampled with a cubic, mu-law effects, equal-power music crossfades;
    and its `Output`.
- The studio probes and a golden render test; at extraction, BRYX's 39
  scores, 17 effects and 8 probes rendered here byte for byte as there
  (64 identical, on eight threads at once).
- `tests/no_alloc.rs`: neither mixer allocates on the callback's path.
- `docs/advanced.md`: the `.score` language, the library, the pipeline.
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

### Changed

- The device stream behind `chip8::Output` is shared with
  `advanced::Output`; `chip8::OutputError` is the same type as
  `advanced::OutputError`.
