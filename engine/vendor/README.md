# Vendored C headers (build-time only)

Two single-file C libraries, copied byte for byte from upstream and compiled
only into the advanced engine's renderer (the `render-advanced` feature;
`build.rs` builds it with `cc`). Neither ships in a game binary, and neither
is a Cargo dependency. They came from BRYX (`audio/vendor/` at `709ca61`).

| File | Upstream | Commit | Licence | Why |
|---|---|---|---|---|
| `dr_flac.h` | https://github.com/mackron/dr_libs | `4ffbded724cc3e3d2838eebca70858bf5984385a` | Public domain (Unlicense) or MIT No Attribution, at the user's choice (see the end of the file) | Decodes the FLAC orchestral samples in `library/samples/` |
| `qoa.h` | https://github.com/phoboslab/qoa | `93b0680030d1312e7a2829e53272353cc1de6ece` | MIT (`qoa.LICENSE`) | Encodes renders to QOA (`engine/advanced/sonomorph_studio.c`), which `src/advanced/qoa.rs` decodes as a game plays |

Do not edit them: update by copying a newer upstream file and its commit here.
