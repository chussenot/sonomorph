#ifndef SONOMORPH_SONG_H
#define SONOMORPH_SONG_H

/*
 * sonomorph_song.h: a parsed song, read through views that Sonomorph owns.
 * SONOMORPH: the file is new.
 *
 * The parser's AbcMusic is an internal structure: its limits and fields
 * change with the engine (BRYX's fork made it nineteen times larger), so a
 * consumer that mirrors its layout corrupts memory the day it changes.
 * These functions keep it on the heap and copy what a reader needs into
 * small, stable view structs. Their layout is fixed here; the Rust side
 * mirrors them in the same crate and a test compares the sizes.
 */

#include <stddef.h>

#include "electric_pulse.h"

typedef struct {
    char title[128];
    int bpm;
    int step_ms;
    int swing_pct;
    int instrument_count;
    int pattern_count;
    int arrangement_length;
    int fx_bus_count;      /* the buses the score declared */
    int fx_bus_slots;      /* the buses the engine has (ABC_MAX_FX_BUSES) */
    int voice_count;
} SonoSongInfo;

typedef struct {
    char name[32];
    char preset[32];
    int amplitude;
    int waveform;
    int duty_cycle;
    int attack_ms;
    int decay_ms;
    int sustain_level;
    int release_ms;
    int gate_percent;
    int vibrato_cents;
    int glide_ms;
    int fx_bus;
} SonoInstrumentInfo;

typedef struct {
    char name[32];
    int length;
    int defined;
} SonoPatternInfo;

typedef struct {
    int enabled;
    int delay_steps;
    int delay_feedback;
    int delay_mix;
    int drive_amount;
    int lowpass_amount;
    int sidechain_amount;
    int sidechain_release_ms;
    int mix_percent;
    int ladder_amount;
    int ladder_cutoff;
    int ladder_resonance;
} SonoFxBusInfo;

typedef struct {
    char name[32];
    char instrument[32];
    int amplitude;
    int staccato;
    int waveform;
    int duty_cycle;
    int attack_ms;
    int decay_ms;
    int sustain_level;
    int release_ms;
    int gate_percent;
    int vibrato_cents;
    int vibrato_rate;
    int glide_ms;
    int fx_bus;
    int step_count;
} SonoVoiceInfo;

/* Parses the ABC file at `path` onto the heap. NULL if it cannot be read or
 * parsed. Free it with sono_song_free. */
AbcMusic *sono_song_load(const char *path);
void sono_song_free(AbcMusic *music);

void sono_song_info(const AbcMusic *music, SonoSongInfo *out);
/* Each returns 0 and fills `out`, or -1 for an index out of range. */
int sono_song_instrument(const AbcMusic *music, int index, SonoInstrumentInfo *out);
int sono_song_pattern(const AbcMusic *music, int index, SonoPatternInfo *out);
int sono_song_fx_bus(const AbcMusic *music, int index, SonoFxBusInfo *out);
int sono_song_voice(const AbcMusic *music, int index, SonoVoiceInfo *out);
/* The arrangement's `index`th pattern name, or NULL out of range. Valid as
 * long as `music` is. */
const char *sono_song_arrangement(const AbcMusic *music, int index);
/* The voice's frequency on each step (0 = rest), `step_count` of them, or
 * NULL out of range. Valid as long as `music` is. */
const double *sono_song_voice_steps(const AbcMusic *music, int index);

/* sizeof of each view, in the order they are declared above, for the Rust
 * side's layout test. -1 past the last. */
long sono_song_view_size(int which);

#endif
