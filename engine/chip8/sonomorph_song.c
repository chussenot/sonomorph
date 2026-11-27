/*
 * sonomorph_song.c: the parsed song behind stable views (sonomorph_song.h).
 * SONOMORPH: the file is new.
 */
#include <stdlib.h>
#include <string.h>

#include "sonomorph_song.h"

static void copy_name(char *out, size_t n, const char *in)
{
    size_t len = strnlen(in, n - 1);
    memcpy(out, in, len);
    out[len] = 0;
}

AbcMusic *sono_song_load(const char *path)
{
    AbcMusic *music;
    if (!path) return NULL;
    music = calloc(1, sizeof(*music));
    if (!music) return NULL;
    if (abc_load(path, music) != 0) {
        free(music);
        return NULL;
    }
    return music;
}

void sono_song_free(AbcMusic *music)
{
    free(music);
}

void sono_song_info(const AbcMusic *music, SonoSongInfo *out)
{
    memset(out, 0, sizeof(*out));
    copy_name(out->title, sizeof out->title, music->title);
    out->bpm = music->bpm;
    out->step_ms = music->step_ms;
    out->swing_pct = music->swing_pct;
    out->instrument_count = music->instrument_count;
    out->pattern_count = music->pattern_count;
    out->arrangement_length = music->arrangement_length;
    out->fx_bus_count = music->fx_bus_count;
    out->fx_bus_slots = ABC_MAX_FX_BUSES;
    out->voice_count = music->voice_count;
}

int sono_song_instrument(const AbcMusic *music, int index, SonoInstrumentInfo *out)
{
    const AbcInstrument *in;
    if (index < 0 || index >= music->instrument_count || index >= ABC_MAX_INSTRUMENTS)
        return -1;
    in = &music->instruments[index];
    memset(out, 0, sizeof(*out));
    copy_name(out->name, sizeof out->name, in->name);
    copy_name(out->preset, sizeof out->preset, in->preset);
    out->amplitude = in->amplitude;
    out->waveform = in->waveform;
    out->duty_cycle = in->duty_cycle;
    out->attack_ms = in->attack_ms;
    out->decay_ms = in->decay_ms;
    out->sustain_level = in->sustain_level;
    out->release_ms = in->release_ms;
    out->gate_percent = in->gate_percent;
    out->vibrato_cents = in->vibrato_cents;
    out->glide_ms = in->glide_ms;
    out->fx_bus = in->fx_bus;
    return 0;
}

int sono_song_pattern(const AbcMusic *music, int index, SonoPatternInfo *out)
{
    const AbcPattern *p;
    if (index < 0 || index >= music->pattern_count || index >= ABC_MAX_PATTERNS)
        return -1;
    p = &music->patterns[index];
    memset(out, 0, sizeof(*out));
    copy_name(out->name, sizeof out->name, p->name);
    out->length = p->length;
    out->defined = p->defined;
    return 0;
}

int sono_song_fx_bus(const AbcMusic *music, int index, SonoFxBusInfo *out)
{
    const AbcFxBus *b;
    if (index < 0 || index >= ABC_MAX_FX_BUSES)
        return -1;
    b = &music->fx_buses[index];
    memset(out, 0, sizeof(*out));
    out->enabled = b->enabled;
    out->delay_steps = b->delay_steps;
    out->delay_feedback = b->delay_feedback;
    out->delay_mix = b->delay_mix;
    out->drive_amount = b->drive_amount;
    out->lowpass_amount = b->lowpass_amount;
    out->sidechain_amount = b->sidechain_amount;
    out->sidechain_release_ms = b->sidechain_release_ms;
    out->mix_percent = b->mix_percent;
    out->ladder_amount = b->ladder_amount;
    out->ladder_cutoff = b->ladder_cutoff;
    out->ladder_resonance = b->ladder_resonance;
    return 0;
}

int sono_song_voice(const AbcMusic *music, int index, SonoVoiceInfo *out)
{
    const AbcVoice *v;
    if (index < 0 || index >= music->voice_count || index >= ABC_MAX_VOICES)
        return -1;
    v = &music->voices[index];
    memset(out, 0, sizeof(*out));
    copy_name(out->name, sizeof out->name, v->name);
    copy_name(out->instrument, sizeof out->instrument, v->instrument_ref);
    out->amplitude = v->amplitude;
    out->staccato = v->staccato;
    out->waveform = v->waveform;
    out->duty_cycle = v->duty_cycle;
    out->attack_ms = v->attack_ms;
    out->decay_ms = v->decay_ms;
    out->sustain_level = v->sustain_level;
    out->release_ms = v->release_ms;
    out->gate_percent = v->gate_percent;
    out->vibrato_cents = v->vibrato_cents;
    out->vibrato_rate = v->vibrato_rate;
    out->glide_ms = v->glide_ms;
    out->fx_bus = v->fx_bus;
    out->step_count = v->note_count < 0 ? 0
                    : v->note_count > ABC_MAX_NOTES ? ABC_MAX_NOTES
                    : v->note_count;
    return 0;
}

const char *sono_song_arrangement(const AbcMusic *music, int index)
{
    if (index < 0 || index >= music->arrangement_length || index >= ABC_MAX_ARRANGEMENT)
        return NULL;
    return music->arrangement[index];
}

const double *sono_song_voice_steps(const AbcMusic *music, int index)
{
    if (index < 0 || index >= music->voice_count || index >= ABC_MAX_VOICES)
        return NULL;
    return music->voices[index].freqs;
}

long sono_song_view_size(int which)
{
    switch (which) {
    case 0: return (long)sizeof(SonoSongInfo);
    case 1: return (long)sizeof(SonoInstrumentInfo);
    case 2: return (long)sizeof(SonoPatternInfo);
    case 3: return (long)sizeof(SonoFxBusInfo);
    case 4: return (long)sizeof(SonoVoiceInfo);
    default: return -1;
    }
}
