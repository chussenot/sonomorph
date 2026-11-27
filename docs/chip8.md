# The chip8 engine

How to write a score for the chip8 engine, render it, store it and play
it, and what each step guarantees. The engine's history and the full list
of what BRYX's fork added to Electric Pulse's ABC dialect are in
[`engine/chip8/PROVENANCE.md`](../engine/chip8/PROVENANCE.md).

## The pipeline

| Step | Call | Output | Guarantee |
|---|---|---|---|
| Render (build time, `render`) | `chip8::render_file(path, rate)` | `Vec<i16>`, mono | The same file at the same rate renders the same samples on any machine and any thread. The engine keeps no global state. |
| Store | `chip8::mulaw::encode` | one byte a sample | G.711 mu-law: about 13 bits of range, a noise floor that follows the signal down. `decode(encode(x))` is within 1/16 of `x`'s segment. |
| Play (run time) | `chip8::Mixer`, `chip8::Output` (`output`) | interleaved `f32` frames | No allocation and no lock in the callback. Commands reach the mixer over a channel and apply at the next buffer. |

Render at the rate the mixer expects: `MixerConfig::sample_rate`, 22,050 Hz
by default. That is the rate the engine was tuned at, and the rate BRYX
ships. The mixer resamples to the device's rate as it plays, and also
applies pitch, by reading the samples faster or slower.

### What a render contains

- **A plain score** renders to the end of its last note's release, including
  any silence the release leaves. An effect that should free its voice the
  moment it ends can be trimmed. BRYX drops trailing samples within one 8-bit
  step of zero (`|s| <= 256`) from its effects and stingers.
- **A `%%loop on` score** renders twice and keeps the second pass, so its
  start carries its own end's echoes and release tails. Its length is
  exactly one pass. Never trim a loop: the seam depends on the length.

## Writing a score

A score is an ABC file (header fields `X:`, `T:`, `M:`, `L:`, `Q:`, `K:`),
plus `%%` directives that set up instruments, effect buses and the mix.
Every note is quantized to sixteenth-note steps.

```abc
X:1
T:fanfare
M:4/4
L:1/16
Q:1/4=160
K:C
% A rising arpeggio to a held E, over a bass.
%%hold on
%%master 86.0
%%bandlimit on
%%instrument lead wave=pulse amp=27 duty=28 attack=6 decay=120 sustain=70 release=280 gate=88 vibrato=6 fx=0
%%instrument bass wave=triangle amp=30 attack=2 decay=160 sustain=60 release=280 gate=90 fx=0
%%effect 0 delay_steps=3 delay_feedback=34 delay_mix=26 lowpass=10 reverb=12 reverb_size=55 reverb_damp=45 mix=100
V:lead instrument=lead
| !mf!E2 G2 B2 e2 !f!g2 b2 !ff!e'4 |
V:bass instrument=bass
| E,4 G,4 C,4 E,4 |
```

`tests/docs.rs` renders every `abc` block on this page, so the examples
here are scores the engine accepts.

### Directives

| Directive | Sets |
|---|---|
| `%%instrument <name> key=value ...` | A named instrument: `wave=square\|pulse\|triangle\|sine\|noise`, `amp`, `duty`, the envelope (`attack`, `decay`, `sustain`, `release`, `gate`), `vibrato`, its FX bus `fx=<n>`, and the fork's keys (`stack`, `detune`, `pwm`, `pwm_rate`, `vibrato_rate`, `vibrato_delay`, `accent`, `accents`, `pitch`, `pitch_time`, `noise`, `bandlimit`). |
| `V:<name> instrument=<instrument>` | A voice. The lines that follow are its notes. |
| `%%effect <n> key=value ...` | FX bus `n` (0 to 3): `delay_steps`, `delay_feedback`, `delay_mix`, `drive`, `lowpass`, `ladder`, `ladder_cutoff`, `ladder_resonance`, `reverb`, `reverb_size`, `reverb_damp`, `sidechain`, `sidechain_release`, `sidechain_key`, `mode=insert`, `mix`. |
| `%%pattern`, `%%arrangement` | Named patterns, and the order they play in. |
| `%%swing <percent>` | Lengthens each on-step and shortens the off-step after it: 50 is straight, 75 the most. |
| `%%sidechain amount=<n> release=<ms>` | The sidechain of a song with a single, unnumbered FX bus. |
| `%%hold on` | A written note sounds once, for its whole length. |
| `%%master <percent>` | Output gain after the mix. |
| `%%loop on` | A seamless loop (see above). |
| `%%bandlimit on` | Smooths square and pulse edges (PolyBLEP), so high notes do not alias. |
| `%%ramp <target> <param> <from> <to> <start> <end>` | Moves a bus parameter, a voice's duty or the song's tempo between two steps. |

Each directive the fork added has a probe in `tests/probes/` that shows
it in use, and a test in `tests/probes.rs` that measures it.

### Notation

| Write | For |
|---|---|
| `!pp!` `!p!` `!mp!` `!mf!` `!f!` `!ff!` (and `!ppp!`, `!fff!`) | Dynamics: the voice's velocity until the next mark |
| `!>!` | An accent on the next note |
| `[EGB]4` | A chord in one voice |
| `z4` | A rest |
| `\|: ... :\|` | A repeat |

### Limits

12 voices, 12 instruments, 4 FX buses, 4,096 steps a voice (256 bars), 64
patterns and 64 arrangement slots (`engine/chip8/electric_pulse.h`). The
engine has no tuplets: it quantizes to sixteenth-note steps.

## Playing

A game names its sounds with an enum that implements `chip8::Sound` (see
the README), and sends `chip8::Command`s:

| Command | Effect |
|---|---|
| `Play { sound, gain, pan, pitch, duck }` | A one-shot voice. Past `MixerConfig::voices`, the oldest voice is stolen. `duck > 0` lowers the music for that many seconds. |
| `Music(Some(sound))` | Crossfades to that loop. Asking for the loop already playing changes nothing. If a sound that `holds_music` is sounding, the new loop waits for it to end. |
| `Music(None)` | Fades the music out. |
| `MusicLevel(level)` | The music's level, eased toward `level` (0 to 1). |
| `Enable { effects, music }` | The player's sound setting. |
| `CutHeld` | Fades out, within `MixerConfig::cut_secs`, the sounds that hold the music. |

`MixerConfig`'s defaults are the levels BRYX is mixed at: music at 0.52
under the effects, ducked to a quarter by a stinger, 1.2 s crossfades, and
a `tanh` soft limiter after a master gain of 1.5. `Mixer` works without a
device, so a game can test its own sound logic against it headless.
`Mixer::voice_count` and `Mixer::music_track` report what is sounding.
