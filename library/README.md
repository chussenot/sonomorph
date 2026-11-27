# The instrument library

What a `.score` plays, shared by every game that uses the advanced
engine. A score reaches it through the `@library/` prefix, which
`advanced::render_score` resolves to this directory:

```text
include @library/instruments/desk.inc
include @library/instruments/orchestra.inc
include @library/instruments/electronic.inc
inst cello sfz=@library/samples/celli_sus.sfz attack=0.1 release=0.3
```

| Path | What |
|---|---|
| `instruments/desk.inc` | The mixing desk: four reverb spaces (room, stage, hall, long), a tempo-synced echo, and the group buses. Include it first. |
| `instruments/orchestra.inc` | The orchestra, seated: strings (sustain, spiccato, tremolo, pizzicato), brass (sustain and staccato), timpani, percussion kit. Built on `samples/`. |
| `instruments/electronic.inc` | The analog-style synth palette: bass, sequence, pads, drone, air, glass. |
| `samples/` | About 25 MB of orchestral samples (16-bit mono FLAC, 44.1 kHz) and their SFZ maps. See `samples/README.md`. |

The instruments came from BRYX's orchestral score (`audio/instruments/` at
BRYX `709ca61`), unchanged. A game's own instruments (BRYX's gameplay
effects, for one) live with the game and use the samples through
`@library/samples/...`.

## Licenses

- `samples/`: a curated subset of Versilian Studios'
  [VSCO 2 Community Edition](https://github.com/sgossner/VSCO-2-CE)
  (commit `440300901dfe9275fd84e0b7763af1f8443ae62e`), **CC0 1.0
  Universal**: public domain, no conditions. `samples/curate.py` made the
  subset (mono, trimmed, sustains looped, normalized per instrument,
  dithered to 16 bit, stored as FLAC). Every `<region>` line in
  `samples/*.sfz` names the VSCO file it came from. CC0 asks for no
  credit; this record keeps the source of every sound known.
- `instruments/`: part of Sonomorph, MIT.

## Size

The samples make this crate too large for crates.io (10 MB). Depend on it
by git; a published crate would ship the library separately.
