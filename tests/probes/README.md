# Engine probes

Small ABC files, one per feature of the chip8 engine. `tests/probes.rs`
renders each and checks the rendered audio: that `%%hold` holds, that `!p!`
is quieter than `!f!`, that a chord sounds three pitches, and so on.
`tests/render.rs` holds each probe's length and checksum as BRYX rendered it,
so a change to the engine that changes any of them is caught.

Every probe runs at 120 BPM with sixteenth-note steps: one step is 125 ms,
2,756.25 samples at 22,050 Hz.
