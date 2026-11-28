# The advanced engine

> In development, on the `advanced` branch. What is here renders and plays
> BRYX's orchestral score byte for byte (on one machine, against BRYX's own
> renderer); its API may still change.

The studio renders `.score` files: timed notes on named instruments (a
sampled orchestra, an analog-style synth), through group buses, aux
reverbs and delays, and a master with a limiter, in 32-bit float stereo at
48 kHz. A game stores music and stingers as QOA and decodes them as they
play; effects go the chip8 way, as mono mu-law pitched by playback rate.

## The pipeline

| Step | Call | Output | Guarantee |
|---|---|---|---|
| Render (build time, `render-advanced`) | `advanced::render_score(path, rate)` | `Render`: float stereo | Deterministic on a machine: every voice's randomness comes from the score's `seed`. Across machines the last bits of a float can differ (libm is chosen per CPU), so cache renders per machine, as a build's `OUT_DIR` does. Reentrant: render on as many threads as there are scores. |
| Store music | `Render::to_qoa()` | QOA bytes | 16-bit TPDF dither from a fixed seed, then QOA (about 3.2 bits a sample). A loop's predictor is warmed on its own end, so the wrap does not click. |
| Store an effect | `render_score(path, Some(44_100))`, `Render::to_s16()`, `advanced::fold_effect`, `chip8::mulaw::encode` | mu-law bytes at 22,050 Hz | Rendered at twice the effect rate, folded to mono behind a 63-tap anti-alias filter. |
| Play (run time) | `advanced::Mixer`, `advanced::Output` (`output`) | interleaved `f32` frames | QOA decoded a slice at a time and resampled with a cubic. No allocation and no lock on the callback's path. |

A render takes seconds where a chip8 cue takes milliseconds. A build
script that renders many scores should render them on threads, and cache
each render under a hash of what it reads: the score, the library and this
crate's version.

## The library

`advanced::LIBRARY` is the instrument library in this crate's source
(`library/`): the mixing desk, the orchestra and the synth palette. A
score reaches it with the `@library/` prefix; any other relative path is
relative to the file that names it.

| File | Defines |
|---|---|
| `@library/instruments/desk.inc` | Aux returns `room`, `stage`, `hall`, `long` (reverbs) and `echo` (a delay); buses `orchestra`, `electronic`, `percussion`. Include it first. |
| `@library/instruments/orchestra.inc` | `strings_high`, `strings_mid`, `strings_low`, `strings_bass`, with `_spic`, `_trem` and `_pizz` articulations; `brass_horns`, `brass_trumpets`, `brass_trombones`, `brass_low`, with `_stac`; `timpani`, `timpani_roll`, `percussion`. |
| `@library/instruments/electronic.inc` | `synth_bass`, `synth_sequence`, `synth_pad`, `synth_lead`, `synth_glass`, `electronic_drone`, `electronic_air`, `electronic_pulse`, `electronic_metal`. |
| `@library/samples/*.sfz` | The sample maps, for an instrument of a game's own: `inst cello sfz=@library/samples/celli_sus.sfz`. |

## Writing a score

```score
score 1
title A short cue
tempo 0 96
length 8
tail 2
seed 1
include @library/instruments/desk.inc
include @library/instruments/orchestra.inc
include @library/instruments/electronic.inc

# A string chord under a horn call, a synth pulse at the bottom.
note 0 8 strings_low C3 70
note 0 8 strings_mid G3 64
note 0 8 strings_high E4 60
note 1 1.5 brass_horns G4 90
note 2.5 0.5 brass_horns C5 96
note 3 4 brass_horns E5 100
note 0 0.5 synth_bass C2 100
note 2 0.5 synth_bass C2 90
note 4 0.5 synth_bass G1 100
note 6 0.5 synth_bass C2 90
ramp master.gain 6 8 0 -12 smooth
```

`tests/docs.rs` renders every `score` block on this page, so the examples
here are scores the studio accepts.

### Lines

Times are in beats; the tempo map turns them into seconds. `#` at the
start of a line or after a space starts a comment (inside a word it is a
sharp: `F#3`).

| Line | Sets |
|---|---|
| `score 1` | The format version. |
| `title <words>` | A name. |
| `rate <Hz>` | The render rate (default 48,000; `render_score`'s `rate` overrides it). |
| `tempo <beat> <bpm>` | A new tempo from `<beat>` on. |
| `length <beats>` | The piece, or the loop. Required. |
| `loop on` | Render warm-up passes and keep the last: the seam carries the end's tails (reverb, delay, releases) into the start. |
| `tail <seconds>` | Extra render after the end (not a loop). |
| `seed <n>` | The seed for every voice's randomness. |
| `include <path>` | Another file: an instrument library. |
| `aux <name> reverb preset=room\|stage\|hall\|long [rt60= predelay= size= damping= lowcut= width= mod=] [gain=<dB>]` | A reverb return. |
| `aux <name> delay time=<beats> [time_r=<beats>] feedback= [pingpong=1] [lowcut=<Hz>] [highcut=<Hz>] [gain=<dB>]` | A delay return. |
| `bus <name> [gain=<dB>] [hpf=<Hz>] [lowshelf=<Hz>:<dB>] [highshelf=<Hz>:<dB>] [peak=<Hz>:<q>:<dB>] [comp=<threshold dB>:<ratio>:<attack s>:<release s>[:<makeup dB>]] [key=<instrument>] [send=<aux>:<dB>]...` | A group bus: EQ, a compressor (keyed by an instrument's notes for sidechain ducking), sends. |
| `master [gain=<dB>] [comp=...] [limit=<ceiling dB>]` | The master: gain, compressor, limiter. |
| `inst <name> sfz=<path> \| synth [bus=<name>] [gain=<dB>] [pan=-1..1] [send=<aux>:<dB>]... [poly=<voices>]`, then patch keys | An instrument (see below). |
| `note <beat> <beats> <inst> <pitch> <velocity 1..127> [legato]` | A note. Pitch is a MIDI number (may be fractional) or a name (`E4`, `F#3`, `Bb2`). |
| `ramp <target>.<param> <beat0> <beat1> <from> <to> [lin\|smooth]` | Automation. Target: an instrument, a bus, an aux or `master`. Params: `gain` (dB, all), `cutoff` (octaves, instruments), `pitch` (cents, instruments). A lane holds its last value after a ramp. |

**Sampler patch keys:** `attack=`, `decay=`, `sustain=`, `release=`,
`velcurve=`, `offset=`, `tune=`, `stack=`, `stack_detune=`,
`stack_delay=`, `spread=`.

**Synth patch keys:** `wave=saw|square|pulse|triangle|sine|noise`, `pw=`,
`stack=`, `detune=`, `spread=`, `sub=`, `noise=`, the amplitude envelope
(`attack=`, `decay=`, `sustain=`, `release=`), the filter
(`filter=none|lp|hp|bp|ladder`, `cutoff=`, `res=`, `keytrack=`,
`fenv=<octaves>`, `fattack=`, `fdecay=`, `fsustain=`, `frelease=`,
`velcutoff=`), `drive=`, `pitchenv=<semitones>`, `pitchtime=`, `glide=`,
`mono=1`, and two LFOs (`lfo1=<shape>:<rate Hz>:<delay s>`, `lfo1_pitch=`,
`lfo1_cutoff=`, `lfo1_amp=`, `lfo1_pan=`, `lfo1_pw=`, and `lfo2_*`).

The signal path: voices, then the instrument (gain, pan, sends), then its
group bus (EQ, compressor, gain, sends), then the master. The aux returns
take the sends and feed the master.

## Playing

A game's sounds implement `advanced::Sound`, returning a `Clip`:
`Clip::Qoa(bytes)` for music and stingers, `Clip::MuLaw(bytes)` for
effects. The commands are chip8's (`advanced::Command` is
`chip8::Command`), with one difference in how music sounds: the advanced
mixer crossfades on an equal-power curve, so two unrelated tracks keep
their loudness through the change. A QOA clip carries its own rate. The
mixer resamples it to the device's, and a stinger's `pitch` scales that.
