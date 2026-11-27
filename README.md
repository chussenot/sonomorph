# Sonomorph

Game audio for Rust, split where a game needs it split: scores are
**rendered at build time** by a synthesis engine, embedded in the binary,
and **mixed at run time** by a callback that never allocates or blocks.

```text
 build.rs (render feature)                    the game (output feature)
 ┌────────────────────────────────┐          ┌──────────────────────────────┐
 │ jump.abc ──render_file──► i16  │          │ Command::Play { sound: Jump }│
 │            mulaw::encode ► u8  │──bytes──►│ Mixer: voices, duck, music   │
 │            OUT_DIR/jump.ulaw   │ include_ │ Output: cpal device stream   │
 └────────────────────────────────┘  bytes!  └──────────────────────────────┘
```

Why this shape:

- **No synthesis in the game.** The engine is C and runs once per build. The
  game carries samples and a small mixer: no C compiler, no engine and no
  audio decoding at run time.
- **Deterministic.** The same score at the same rate renders the same
  samples every time, on any thread. chip8 mixes in fixed point, and its
  probes render bit for bit alike on BRYX's machines and on CI's. A score
  is source code: diff it, review it, rebuild it.
- **Small.** Samples are stored as 8-bit mu-law, about 13 bits of range in
  one byte. BRYX's 33 music loops and 23 effects take 23 MB of its binary.

Sonomorph was extracted from [BRYX](https://github.com/chussenot/bryx), a
brick-breaker, whose score and effects it renders.

## Engines

| Module | Sound | Status |
|---|---|---|
| `sonomorph::chip8` | 8-bit-style synthesis: square, pulse, triangle and noise voices, FX buses, a small reverb, from ABC notation | This branch (`main`) |
| `sonomorph::advanced` | Orchestral rendering: sampled instruments, synth layers, a studio effects chain, streamed music | The `advanced` branch, in development |

The chip8 engine is the Electric Pulse engine as BRYX forked it. Its ABC
dialect goes past standard ABC with held notes, dynamics, chords, drum
synthesis, automation, reverb and seamless loops, listed in
[`docs/chip8.md`](docs/chip8.md).

## Features

Nothing is on by default. Ask for each half where it is needed:

| Feature | Adds | Needs | Where |
|---|---|---|---|
| `render` | `chip8::render_file` and the C engine; for a tool that works on scores, `render_file_u8` (8-bit, with statistics) and `chip8::Song` (what a score declares) | a C compiler | `[build-dependencies]` |
| `jam` | `chip8::JamSession`: endless seeded variations on a score | as `render` | where the variations play |
| `midi` | `chip8::export_midi`: a score as a Standard MIDI File | as `render` | where the export runs |
| `output` | `chip8::Output`: the mixer on the default audio device | ALSA headers on Linux (`libasound2-dev`) | `[dependencies]` |
| (always) | `chip8::Mixer`, `chip8::Sound`, `chip8::mulaw` | nothing | either |

With Cargo's feature resolver (edition 2021 and later), the build
dependency's `render` never reaches the game binary.

## Use

```toml
[dependencies]
sonomorph = { git = "https://github.com/chussenot/sonomorph", features = ["output"] }

[build-dependencies]
sonomorph = { git = "https://github.com/chussenot/sonomorph", features = ["render"] }
```

Render each score in `build.rs`:

```rust,ignore
use sonomorph::chip8::{mulaw, render_file};

fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=audio");
    for name in ["jump", "fanfare", "theme"] {
        let samples = render_file(format!("audio/{name}.abc"), 22_050).unwrap();
        let bytes: Vec<u8> = samples.into_iter().map(mulaw::encode).collect();
        std::fs::write(out.join(format!("{name}.ulaw")), bytes).unwrap();
    }
}
```

Name the sounds and play them:

```rust,ignore
use sonomorph::chip8::{Command, Output, Sound};

#[derive(Clone, Copy, PartialEq)]
enum Cue { Jump, Fanfare, Theme }

impl Sound for Cue {
    fn pcm(self) -> &'static [u8] {
        match self {
            Cue::Jump => include_bytes!(concat!(env!("OUT_DIR"), "/jump.ulaw")),
            Cue::Fanfare => include_bytes!(concat!(env!("OUT_DIR"), "/fanfare.ulaw")),
            Cue::Theme => include_bytes!(concat!(env!("OUT_DIR"), "/theme.ulaw")),
        }
    }
    // New music waits for the fanfare to finish.
    fn holds_music(self) -> bool { self == Cue::Fanfare }
}

let audio = Output::<Cue>::open(); // Err on a headless machine: play silent
if let Ok(audio) = &audio {
    audio.send(Command::Music(Some(Cue::Theme)));
    audio.send(Command::Play { sound: Cue::Jump, gain: 1.0, pan: -0.3, pitch: 1.0, duck: 0.0 });
}
```

[`docs/chip8.md`](docs/chip8.md) covers the mixer's commands, its
configuration, and what each part guarantees.

To hear a score without a game, `sonomorph-render` renders it to WAV (or
to raw 16-bit, for a script that measures it), and the `play` example runs
the whole pipeline in one process:

```sh
cargo install --git https://github.com/chussenot/sonomorph --features render sonomorph-render
sonomorph-render tests/probes/chord.abc chord.wav
cargo run --example play --features render,output -- tests/probes/chord.abc
```

## Develop

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets -- -D warnings        # no features: pure Rust only
cargo test --all-features
```

CI runs these on Linux. The tests:

- the probes (`tests/probes.rs`): one small score per engine feature, each
  measured on what it renders;
- the render contract (`tests/render.rs`), including the checksum of every
  probe as BRYX rendered it before the engine moved here;
- the engine's own C tests (`engine/chip8/tests/`, run by `tests/engine.rs`;
  `SONOMORPH_C_SANITIZE=1` adds ASan and UBSan, as CI does);
- what an editor reads and plays (`tests/song.rs`, `tests/jam.rs`,
  `tests/midi.rs`);
- the mixer and the codec, as unit tests.

Changes to the engine follow `engine/chip8/PROVENANCE.md`, "Changing it":
mark them, keep them opt-in, never update a checksum to make a test pass.
Commits follow [Conventional Commits](https://www.conventionalcommits.org).

## License

MIT, see [LICENSE](LICENSE). The engine's history is in
[`engine/chip8/PROVENANCE.md`](engine/chip8/PROVENANCE.md).
