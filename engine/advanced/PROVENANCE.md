# The advanced engine (the studio): where it comes from

The studio was written for [BRYX](https://github.com/chussenot/bryx)'s
orchestral score, on its `claude/orchestral-score` branch. Nothing in it
comes from Electric Pulse or from any other project's code; the two
single-header libraries it uses are vendored in `engine/vendor/`.

It came to Sonomorph from BRYX commit
`709ca61866330fd042bb732b6daa3c1d9a14cbaf`, with the changes below, each
marked `SONOMORPH` in the source. At the move, all 39 of BRYX's scores
(QOA), its 17 effects (16-bit) and the 8 probes (float) were rendered here
and by BRYX's renderer on the same machine and compared byte for byte: 64
identical, rendered eight at a time on threads. `tests/studio_render.rs`
keeps every probe's frame count and the RMS of each quarter second of it,
as BRYX rendered it, within 0.1%.

| File | Role |
|---|---|
| `studio.h` | The contract every module shares: helpers, envelopes, patches, effect parameters, `st_render_score` and `st_render_score_lib` |
| `studio.c` | The `.score` format, the scheduler, the mixer (instruments, buses, aux returns, master), the loop seam |
| `studio_common.c` | Seeded RNG, dB and MIDI helpers, the exponential ADSR, patch defaults |
| `studio_synth.[ch]` | The analog-style voice: band-limited oscillators, stacks, SVF and ladder filters, envelopes, LFOs, glide |
| `studio_sampler.[ch]` | The SFZ-subset loader and the sampler voice: zones, velocity layers, loops, stacks |
| `studio_fx.[ch]` | FDN reverb, ping-pong delay, biquads, compressor, lookahead limiter |
| `sonomorph_studio.c` | The 16-bit dither and the seamless QOA encode, as BRYX's `audio/render.c` did them, as library calls |
| `tests/test_*.c` | Each module's own checks |

## What changed in the move

- **Reentrant.** `st_render_score` kept the score, the voices and the mix
  buffers in function-level `static`s, which was safe while every render
  was its own process. It now keeps them in one heap block per call, so
  a build script can render on several threads. Same sizes, same zeroed
  start, same arithmetic: the byte-for-byte comparison above was run
  eight renders at a time.
- **No strtok.** The `.score` tokenizer split each line with `strtok`,
  whose hidden static pointer every thread shares; it now splits with a
  local `next_token` that cuts the same tokens.
- **The library.** A path written `@library/...`, in an `include` or an
  `sfz=`, resolves against the instrument library's directory, which
  `st_render_score_lib` takes (Rust: `advanced::LIBRARY`). A score
  outside this repository reaches the shared instruments that way.
  `st_render_score` is unchanged, with no library.
- **The C tests** read the samples from `library/samples/`.

## Rules

- Deterministic: the same input renders the same bytes on a machine.
  Randomness only from `st_rand` (a seeded xorshift), never `rand()` or
  time. Across machines the last bits can differ: the studio computes in
  float and calls libm, and glibc picks its implementations per CPU (one
  probe renders a different checksum on GitHub's runners). That is why the
  golden test compares fingerprints, not checksums.
- Build-time only: allocation is fine in `*_init` and `*_load`, never per
  sample.
- Frequencies and times in Hz and seconds, gains linear unless a name says
  `_db`.

## Changing it

Edit, mark the change `SONOMORPH`, and run:

```sh
# Each module's own checks, from engine/advanced/:
cc -std=c99 -O2 -Wall -Wextra -Werror -o t tests/test_synth.c studio_synth.c studio_common.c -lm && ./t
cc -std=c99 -O2 -Wall -Wextra -Werror -o t tests/test_fx.c studio_fx.c studio_common.c -lm && ./t
cc -std=c99 -O2 -Wall -Wextra -Werror -o t tests/test_sampler.c studio_sampler.c studio_common.c -lm && ./t
# And what it renders:
cargo test --all-features
```

`tests/studio_render.rs` must still pass unchanged: if a fingerprint
moves, every game's score sounds different, which needs a reason and a
CHANGELOG entry, never a silently updated table. CI runs all of the above.
