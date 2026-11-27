/*
 * sonomorph_studio.c: what BRYX's build-time renderer (audio/render.c at
 * BRYX 709ca61) did with a studio render, as calls the Rust side makes:
 *
 *   sono_studio_to_s16   float stereo -> 16-bit stereo, TPDF-dithered from
 *                        a fixed seed (the only quantization in the path)
 *   sono_studio_to_qoa   the same 16-bit, encoded as QOA, with a loop's
 *                        predictor warmed on its own end (no click at the
 *                        wrap)
 *
 * The two bodies are BRYX's, unchanged, so a score encodes to the bytes
 * it encoded to there. SONOMORPH: the file itself is new.
 */
#include <math.h>
#include <stdlib.h>
#include <string.h>

#include "studio.h"

#define QOA_IMPLEMENTATION
#include "../vendor/qoa.h"

/* Float stereo to 16 bit with TPDF dither from a fixed seed. */
short *sono_studio_to_s16(const StRender *r)
{
    size_t n = (size_t)r->frame_count * 2;
    short *s = malloc(n * sizeof *s);
    uint32_t seed = 0x5EED1234u;
    for (size_t i = 0; s && i < n; i++) {
        float d = (st_randf(&seed) + st_randf(&seed)) * 0.5f / 32768.0f;
        float v = r->frames[i] + d;
        long q = lrintf(v * 32767.0f);
        s[i] = (short)(q > 32767 ? 32767 : q < -32768 ? -32768 : q);
    }
    return s;
}

/* qoa_encode(), with one change for a loop: the encoder's predictor is
 * first run over the loop's last frame, so the first frame header carries
 * the state the loop's end leaves, and a decoder that wraps from the end
 * to the start predicts the first samples from the true preceding ones.
 * QOA otherwise starts from silence, and its first slice, coded against a
 * cold predictor, would put a click at every wrap. The file is plain QOA:
 * every frame header holds its own predictor state. */
static void *qoa_encode_seamless(const short *pcm, qoa_desc *qoa, int loop, unsigned int *out_len)
{
    unsigned int frames = (qoa->samples + QOA_FRAME_LEN - 1) / QOA_FRAME_LEN;
    unsigned int max = 8 + frames * qoa_max_frame_size(qoa);
    unsigned char *bytes = malloc(max);
    unsigned char *scratch = malloc(qoa_max_frame_size(qoa));
    if (!bytes || !scratch) {
        free(bytes);
        free(scratch);
        return NULL;
    }
    for (unsigned int c = 0; c < qoa->channels; c++) {
        qoa->lms[c].weights[0] = 0;
        qoa->lms[c].weights[1] = 0;
        qoa->lms[c].weights[2] = -(1 << 13);
        qoa->lms[c].weights[3] = (1 << 14);
        for (int i = 0; i < QOA_LMS_LEN; i++)
            qoa->lms[c].history[i] = 0;
    }
    if (loop) {
        unsigned int warm = qoa->samples < QOA_FRAME_LEN ? qoa->samples : QOA_FRAME_LEN;
        qoa_encode_frame(pcm + (size_t)(qoa->samples - warm) * qoa->channels, qoa, warm, scratch);
    }
    unsigned int p = qoa_encode_header(qoa, bytes);
    for (unsigned int at = 0; at < qoa->samples; at += QOA_FRAME_LEN) {
        unsigned int len = qoa->samples - at < QOA_FRAME_LEN ? qoa->samples - at : QOA_FRAME_LEN;
        p += qoa_encode_frame(pcm + (size_t)at * qoa->channels, qoa, len, bytes + p);
    }
    free(scratch);
    *out_len = p;
    return bytes;
}

unsigned char *sono_studio_to_qoa(const StRender *r, unsigned int *out_len)
{
    qoa_desc desc;
    void *encoded;
    short *s16;

    *out_len = 0;
    if (r->frame_count <= 0) return NULL;
    s16 = sono_studio_to_s16(r);
    if (!s16) return NULL;
    memset(&desc, 0, sizeof desc);
    desc.channels = 2;
    desc.samplerate = (unsigned)r->rate;
    desc.samples = (unsigned)r->frame_count;
    encoded = qoa_encode_seamless(s16, &desc, r->loop, out_len);
    free(s16);
    return encoded;
}

void sono_free(void *p)
{
    free(p);
}
