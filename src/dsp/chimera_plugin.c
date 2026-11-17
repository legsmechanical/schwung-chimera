/*
 * CHIMERA for Schwung / Ableton Move: plugin API v2 around the Rust engine (chimera-move).
 *
 * THREADING. Every plugin entry point runs on the SPI audio callback (MODULES.md "there is no
 * control thread"). The engine cannot: one voice costs ~20% of a CM5 core (light) or ~60%
 * (full), with blocks that spike well past their average, and planning a sung note or switching a
 * body allocates. So the engine lives on a render thread of its own, created in create_instance
 * (which runs on the host's SCHED_OTHER loader thread, so the thread does not inherit FIFO 70),
 * pinned off core 3 and given a low FIFO priority below Move's Link Main (35). It renders ahead
 * into a ring; render_block only copies out of it. Every other entry point only touches this
 * file's mirror and lock-free rings: no allocation, no locks, no I/O, no logging.
 *
 *   callback -> worker   cmd ring    (params, notes, lyrics by slot)
 *   worker -> callback   audio ring  (mono float, `latency` blocks ahead)
 *   worker -> callback   pub ring    (values the engine set itself: a vowel moves the mouth)
 *   callback -> worker   transport   (seqlock: beat position, tempo, at which read position)
 *
 * The worker is woken by a semaphore the callback posts once per block.
 *
 * MIT (engine: CHIMERA, Jameson Hodge)
 */
#define _GNU_SOURCE
#include <errno.h>
#include <math.h>
#include <pthread.h>
#include <sched.h>
#include <semaphore.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "host/plugin_api_v1.h"
#include "params_gen.h"

#define EXPORT __attribute__((visibility("default")))
#define BUILD_ID "chimera 0.1.0"

/* ---- the Rust side (engine/crates/chimera-move) */
typedef struct Chimera Chimera;
extern Chimera *chm_create(double sample_rate, int light, const char *dict_path);
extern void chm_destroy(Chimera *h);
extern void chm_render(Chimera *h, float *out, size_t n);
extern void chm_set(Chimera *h, uint32_t id, double v);
extern void chm_set_lyrics(Chimera *h, const char *text);
extern void chm_note_on(Chimera *h, uint8_t note, uint8_t velocity);
extern void chm_note_off(Chimera *h, uint8_t note);
extern void chm_all_notes_off(Chimera *h);
extern void chm_transport(Chimera *h, double beat, double bpm);
extern size_t chm_take_published(Chimera *h, uint32_t *ids, double *vals, size_t max);
extern size_t chm_next_syllable(Chimera *h, uint8_t *buf, size_t cap);

static const host_api_v1_t *g_host;

#define BLOCK 128
#define AUDIO_CAP 4096 /* samples, power of two: 32 blocks */
#define CMD_CAP 1024   /* power of two */
#define PUB_CAP 256    /* power of two */
#define LYR_MAX 512
#define LYR_SLOTS 8
static const int kLatencyBlocks[3] = {2, 4, 8}; /* Low / Normal / Safe: 5.8, 11.6, 23.2 ms */

enum { CMD_SET, CMD_NOTE_ON, CMD_NOTE_OFF, CMD_ALL_OFF, CMD_LYRICS };
typedef struct {
    uint8_t type, a, b;
    uint32_t id;
    double v;
} cmd_t;

typedef struct {
    uint32_t id;
    double v;
} pub_t;

typedef struct {
    /* ---- callback side: the mirror get_param answers from */
    float val[CP_COUNT];
    char lyrics[LYR_MAX];
    int preset;
    int octave;
    int8_t note_map[128]; /* key pressed -> note sent (octave at press time) */

    /* ---- rings */
    cmd_t cmd[CMD_CAP];
    _Atomic uint32_t cmd_w, cmd_r;
    char lyr_slot[LYR_SLOTS][LYR_MAX];
    uint32_t lyr_next;
    float audio[AUDIO_CAP];
    _Atomic uint64_t aud_w, aud_r; /* sample positions */
    pub_t pub[PUB_CAP];
    _Atomic uint32_t pub_w, pub_r;

    /* ---- transport seqlock */
    _Atomic uint32_t tr_seq;
    double tr_beat, tr_bpm;
    uint64_t tr_read; /* audio read position when the beat was sampled */

    /* ---- shared flags */
    _Atomic int ready;  /* the engine exists and has the state */
    _Atomic int stop;
    _Atomic int latency;
    _Atomic uint32_t underruns;
    _Atomic uint32_t syl_seq;
    char syl[64];

    /* ---- worker */
    pthread_t thread;
    int thread_ok;
    sem_t wake;
    double sr;
    char dict_path[512];
} inst_t;

/* ------------------------------------------------------------------ rings (SPSC) */

static int cmd_push(inst_t *in, cmd_t c) {
    uint32_t w = atomic_load_explicit(&in->cmd_w, memory_order_relaxed);
    uint32_t r = atomic_load_explicit(&in->cmd_r, memory_order_acquire);
    if (w - r >= CMD_CAP) return 0; /* full: dropped (a param's next write carries it) */
    in->cmd[w & (CMD_CAP - 1)] = c;
    atomic_store_explicit(&in->cmd_w, w + 1, memory_order_release);
    return 1;
}

static int cmd_pop(inst_t *in, cmd_t *c) {
    uint32_t r = atomic_load_explicit(&in->cmd_r, memory_order_relaxed);
    uint32_t w = atomic_load_explicit(&in->cmd_w, memory_order_acquire);
    if (r == w) return 0;
    *c = in->cmd[r & (CMD_CAP - 1)];
    atomic_store_explicit(&in->cmd_r, r + 1, memory_order_release);
    return 1;
}

static void pub_push(inst_t *in, uint32_t id, double v) {
    uint32_t w = atomic_load_explicit(&in->pub_w, memory_order_relaxed);
    uint32_t r = atomic_load_explicit(&in->pub_r, memory_order_acquire);
    if (w - r >= PUB_CAP) return;
    in->pub[w & (PUB_CAP - 1)] = (pub_t){id, v};
    atomic_store_explicit(&in->pub_w, w + 1, memory_order_release);
}

/* callback side: fold what the engine published into the mirror */
static void pub_drain(inst_t *in) {
    uint32_t r = atomic_load_explicit(&in->pub_r, memory_order_relaxed);
    uint32_t w = atomic_load_explicit(&in->pub_w, memory_order_acquire);
    for (; r != w; ++r) {
        pub_t p = in->pub[r & (PUB_CAP - 1)];
        if (p.id < CP_COUNT) in->val[p.id] = (float)p.v;
    }
    atomic_store_explicit(&in->pub_r, r, memory_order_release);
}

static void send_set(inst_t *in, int id) { cmd_push(in, (cmd_t){CMD_SET, 0, 0, (uint32_t)id, in->val[id]}); }

static void send_lyrics(inst_t *in) {
    uint32_t s = in->lyr_next++ % LYR_SLOTS;
    memcpy(in->lyr_slot[s], in->lyrics, LYR_MAX);
    cmd_push(in, (cmd_t){CMD_LYRICS, 0, 0, s, 0});
}

/* ------------------------------------------------------------------ worker */

/* the worker's own copy of the state, for building (or rebuilding) the engine */
typedef struct {
    double val[CP_COUNT];
    char lyrics[LYR_MAX];
} wstate_t;

static Chimera *build_engine(inst_t *in, const wstate_t *ws) {
    Chimera *h = chm_create(in->sr, ws->val[CP_QUALITY] < 0.5, in->dict_path[0] ? in->dict_path : NULL);
    chm_set_lyrics(h, ws->lyrics);
    /* in id order (the vowel before the mouth knobs it would otherwise override), mode last */
    for (int id = 0; id < CP_COUNT; ++id)
        if (id != CP_MODE && cp_params[id].type != CP_STRING && !cp_params[id].trigger) chm_set(h, id, ws->val[id]);
    chm_set(h, CP_MODE, ws->val[CP_MODE]);
    return h;
}

/* the host's beat at the first sample of the block about to be rendered (< 0: stopped) */
static void transport_at(inst_t *in, uint64_t write_pos, double *beat, double *bpm) {
    double b, t;
    uint64_t rd;
    uint32_t s0, s1;
    do {
        s0 = atomic_load_explicit(&in->tr_seq, memory_order_acquire);
        b = in->tr_beat;
        t = in->tr_bpm;
        rd = in->tr_read;
        atomic_thread_fence(memory_order_acquire);
        s1 = atomic_load_explicit(&in->tr_seq, memory_order_acquire);
    } while ((s0 & 1) || s0 != s1);
    *bpm = t;
    *beat = b < 0 ? -1.0 : b + (double)(int64_t)(write_pos - rd) / in->sr * t / 60.0;
}

static void *worker(void *arg) {
    inst_t *in = (inst_t *)arg;
    /* below Link Main (FIFO 35), off core 3 (the SPI callback's); SCHED_OTHER if FIFO is refused */
    struct sched_param sp = {.sched_priority = 10};
    if (sched_setscheduler(0, SCHED_FIFO, &sp) != 0) {
        sp.sched_priority = 0;
        sched_setscheduler(0, SCHED_OTHER, &sp);
    }
    cpu_set_t set;
    CPU_ZERO(&set);
    CPU_SET(0, &set);
    CPU_SET(1, &set);
    CPU_SET(2, &set);
    sched_setaffinity(0, sizeof set, &set);

    wstate_t *wsp = calloc(1, sizeof *wsp); /* the worker may allocate: it is not the callback */
    if (!wsp) return NULL;
    /* defaults, then every command queued so far (create_instance queued the first preset
     * before this thread existed): never a read of the callback's mirror, which a state
     * restore may be writing on the loader thread right now */
    for (int i = 0; i < CP_COUNT; ++i) wsp->val[i] = cp_params[i].def;
    snprintf(wsp->lyrics, LYR_MAX, "%s", CP_LYRICS_DEFAULT);
    cmd_t c;
    while (cmd_pop(in, &c)) {
        if (c.type == CMD_SET && c.id < CP_COUNT) wsp->val[c.id] = c.v;
        else if (c.type == CMD_LYRICS) memcpy(wsp->lyrics, in->lyr_slot[c.id % LYR_SLOTS], LYR_MAX);
    }
    Chimera *h = build_engine(in, wsp);
    atomic_store(&in->ready, 1);

    float buf[BLOCK];
    uint32_t ids[32];
    double vals[32];
    while (!atomic_load(&in->stop)) {
        uint64_t w = atomic_load_explicit(&in->aud_w, memory_order_relaxed);
        for (;;) {
            /* commands first, so a note lands in the very next block rendered */
            while (cmd_pop(in, &c)) {
                switch (c.type) {
                case CMD_SET:
                    if (c.id >= CP_COUNT) break;
                    if (!cp_params[c.id].trigger) wsp->val[c.id] = c.v;
                    if (c.id != CP_LATENCY) chm_set(h, c.id, c.v);
                    break;
                case CMD_NOTE_ON: chm_note_on(h, c.a, c.b); break;
                case CMD_NOTE_OFF: chm_note_off(h, c.a); break;
                case CMD_ALL_OFF: chm_all_notes_off(h); break;
                case CMD_LYRICS:
                    memcpy(wsp->lyrics, in->lyr_slot[c.id % LYR_SLOTS], LYR_MAX);
                    wsp->lyrics[LYR_MAX - 1] = 0;
                    chm_set_lyrics(h, wsp->lyrics);
                    break;
                }
            }
            uint64_t r = atomic_load_explicit(&in->aud_r, memory_order_acquire);
            int target = kLatencyBlocks[atomic_load(&in->latency)] * BLOCK;
            if ((int64_t)(w - r) >= target) break;
            double beat, bpm;
            transport_at(in, w, &beat, &bpm);
            chm_transport(h, beat, bpm);
            chm_render(h, buf, BLOCK);
            for (int i = 0; i < BLOCK; ++i) in->audio[(w + (uint64_t)i) & (AUDIO_CAP - 1)] = buf[i];
            w += BLOCK;
            atomic_store_explicit(&in->aud_w, w, memory_order_release);
            size_t n;
            while ((n = chm_take_published(h, ids, vals, 32)) > 0)
                for (size_t i = 0; i < n; ++i) {
                    if (ids[i] < CP_COUNT) wsp->val[ids[i]] = vals[i];
                    pub_push(in, ids[i], vals[i]);
                }
        }
        /* the next syllable, for get_param */
        uint32_t s = atomic_load_explicit(&in->syl_seq, memory_order_relaxed);
        atomic_store_explicit(&in->syl_seq, s + 1, memory_order_release);
        chm_next_syllable(h, (uint8_t *)in->syl, sizeof in->syl);
        atomic_store_explicit(&in->syl_seq, s + 2, memory_order_release);

        struct timespec ts;
        clock_gettime(CLOCK_REALTIME, &ts);
        ts.tv_nsec += 20 * 1000 * 1000;
        if (ts.tv_nsec >= 1000000000L) ts.tv_sec += 1, ts.tv_nsec -= 1000000000L;
        while (sem_timedwait(&in->wake, &ts) != 0 && errno == EINTR) {}
    }
    chm_destroy(h);
    free(wsp);
    return NULL;
}

/* ------------------------------------------------------------------ params */

static int find_param(const char *key) {
    for (int i = 0; i < CP_COUNT; ++i)
        if (!strcmp(cp_params[i].key, key)) return i;
    return -1;
}

static float clampf(float v, float lo, float hi) { return v < lo ? lo : v > hi ? hi : v; }

/* parse a value for param `id`: an enum takes its index or an option name */
static int parse_value(int id, const char *val, float *out) {
    const cp_param_t *p = &cp_params[id];
    if (p->type == CP_ENUM) {
        for (int i = 0; i < p->n_options; ++i)
            if (!strcmp(p->options[i], val)) return *out = (float)i, 1;
        char *end;
        long i = strtol(val, &end, 10);
        if (end == val) return 0;
        *out = (float)(i < 0 ? 0 : i >= p->n_options ? p->n_options - 1 : i);
        return 1;
    }
    char *end;
    float v = strtof(val, &end);
    if (end == val || !isfinite(v)) return 0;
    *out = clampf(v, p->min, p->max);
    return 1;
}

static void set_lyrics_text(inst_t *in, const char *text) {
    snprintf(in->lyrics, LYR_MAX, "%s", text ? text : "");
    send_lyrics(in);
}

/* a param written (by the grid, a preset, or a restored state) */
static void set_value(inst_t *in, int id, float v) {
    const cp_param_t *p = &cp_params[id];
    if (p->trigger) {
        if (v >= 0.5f) cmd_push(in, (cmd_t){CMD_SET, 0, 0, (uint32_t)id, 1.0});
        return;
    }
    in->val[id] = v;
    if (id == CP_LATENCY) atomic_store(&in->latency, (int)v);
    if (id == CP_WORDS) {
        const char *t = cp_words_text[(int)v];
        if (t) set_lyrics_text(in, t);
    }
    send_set(in, id);
}

static void apply_preset(inst_t *in, int idx) {
    if (idx < 0 || idx >= CP_PRESET_COUNT) return;
    const cp_preset_t *pr = &cp_presets[idx];
    in->preset = idx;
    cmd_push(in, (cmd_t){CMD_ALL_OFF, 0, 0, 0, 0});
    for (int id = 0; id < CP_COUNT; ++id) {
        if (cp_params[id].type == CP_STRING || cp_params[id].trigger) continue;
        if (id == CP_QUALITY || id == CP_LATENCY) continue; /* setup, not sound */
        in->val[id] = pr->vals[id];
        send_set(in, id);
    }
    set_lyrics_text(in, pr->lyrics);
    /* the vowel again, last: a preset names a vowel, so its posture wins over default mouth knobs */
    send_set(in, CP_VOWEL);
}

/* ---- state: {"preset":N,"lyrics":"...","p":{"key":value,...}} */

static int put(char *buf, int len, int n, const char *fmt, ...) __attribute__((format(printf, 4, 5)));
static int put(char *buf, int len, int n, const char *fmt, ...) {
    if (n < 0 || n >= len) return -1;
    va_list ap;
    va_start(ap, fmt);
    int k = vsnprintf(buf + n, (size_t)(len - n), fmt, ap);
    va_end(ap);
    return (k < 0 || n + k >= len) ? -1 : n + k;
}

static int put_json_str(char *buf, int len, int n, const char *s) {
    n = put(buf, len, n, "\"");
    for (; *s && n >= 0; ++s) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\') n = put(buf, len, n, "\\%c", c);
        else if (c < 0x20) n = put(buf, len, n, " ");
        else n = put(buf, len, n, "%c", c);
    }
    return put(buf, len, n, "\"");
}

static int build_state(inst_t *in, char *buf, int len) {
    int n = put(buf, len, 0, "{\"v\":1,\"preset\":%d,\"lyrics\":", in->preset);
    n = put_json_str(buf, len, n, in->lyrics);
    n = put(buf, len, n, ",\"p\":{");
    int first = 1;
    for (int id = 0; id < CP_COUNT; ++id) {
        if (cp_params[id].type == CP_STRING || cp_params[id].trigger) continue;
        n = put(buf, len, n, "%s\"%s\":%.5g", first ? "" : ",", cp_params[id].key, in->val[id]);
        first = 0;
    }
    return put(buf, len, n, "}}");
}

/* the value after "key": in json (NULL if absent) */
static const char *json_find(const char *json, const char *key) {
    char pat[48];
    snprintf(pat, sizeof pat, "\"%s\":", key);
    const char *p = strstr(json, pat);
    return p ? p + strlen(pat) : NULL;
}

static void restore_state(inst_t *in, const char *json) {
    const char *p = json_find(json, "preset");
    if (p) in->preset = atoi(p);
    if ((p = json_find(json, "lyrics")) && *p == '"') {
        char out[LYR_MAX];
        int n = 0;
        for (++p; *p && *p != '"' && n < LYR_MAX - 1; ++p) {
            if (*p == '\\' && p[1]) ++p;
            out[n++] = *p;
        }
        out[n] = 0;
        set_lyrics_text(in, out);
    }
    const char *obj = json_find(json, "p");
    if (!obj) return;
    cmd_push(in, (cmd_t){CMD_ALL_OFF, 0, 0, 0, 0});
    /* in id order: the vowel lands before the mouth values the user may have moved since */
    for (int id = 0; id < CP_COUNT; ++id) {
        if (cp_params[id].type == CP_STRING || cp_params[id].trigger || id == CP_WORDS) continue;
        const char *v = json_find(obj, cp_params[id].key);
        if (!v) continue;
        char *end;
        float f = strtof(v, &end);
        if (end == v || !isfinite(f)) continue;
        f = cp_params[id].type == CP_ENUM ? clampf(roundf(f), 0, (float)(cp_params[id].n_options - 1)) : clampf(f, cp_params[id].min, cp_params[id].max);
        in->val[id] = f;
        if (id == CP_LATENCY) atomic_store(&in->latency, (int)f);
        send_set(in, id);
    }
    /* Words is restored without re-reading its text: the saved lyrics are the truth */
    const char *wv = json_find(obj, "words");
    if (wv) {
        in->val[CP_WORDS] = clampf(roundf(strtof(wv, NULL)), 0, (float)(cp_params[CP_WORDS].n_options - 1));
        send_set(in, CP_WORDS);
    }
}

/* ------------------------------------------------------------------ plugin API */

static void *create_instance(const char *module_dir, const char *json_defaults) {
    (void)json_defaults;
    inst_t *in = calloc(1, sizeof *in);
    if (!in) return NULL;
    in->sr = g_host && g_host->sample_rate > 0 ? g_host->sample_rate : 44100;
    for (int i = 0; i < CP_COUNT; ++i) in->val[i] = cp_params[i].def;
    snprintf(in->lyrics, LYR_MAX, "%s", CP_LYRICS_DEFAULT);
    memset(in->note_map, -1, sizeof in->note_map);
    in->tr_beat = -1;
    in->tr_bpm = 120;
    atomic_store(&in->latency, (int)cp_params[CP_LATENCY].def);
    if (module_dir && *module_dir) snprintf(in->dict_path, sizeof in->dict_path, "%s/cmudict.txt.gz", module_dir);
    in->preset = 0;
    apply_preset(in, 0);
    sem_init(&in->wake, 0, 0);
    /* explicit policy: never inherit the caller's (see the header comment) */
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setinheritsched(&attr, PTHREAD_EXPLICIT_SCHED);
    pthread_attr_setschedpolicy(&attr, SCHED_OTHER);
    struct sched_param sp = {.sched_priority = 0};
    pthread_attr_setschedparam(&attr, &sp);
    in->thread_ok = pthread_create(&in->thread, &attr, worker, in) == 0;
    pthread_attr_destroy(&attr);
    return in;
}

static void destroy_instance(void *instance) {
    inst_t *in = instance;
    if (!in) return;
    atomic_store(&in->stop, 1);
    sem_post(&in->wake);
    if (in->thread_ok) pthread_join(in->thread, NULL);
    sem_destroy(&in->wake);
    free(in);
}

static void on_midi(void *instance, const uint8_t *msg, int len, int source) {
    (void)source;
    inst_t *in = instance;
    if (!in || len < 1) return;
    const uint8_t status = msg[0] & 0xF0;
    const int d1 = len > 1 ? msg[1] & 0x7F : 0, d2 = len > 2 ? msg[2] & 0x7F : 0;
    int ready = atomic_load_explicit(&in->ready, memory_order_acquire);
    switch (status) {
    case 0x90:
        if (d2 > 0) {
            if (!ready) return; /* still building: a note now would sound late, out of place */
            int key = d1 + in->octave * 12;
            key = key < 0 ? 0 : key > 127 ? 127 : key;
            if (in->note_map[d1] >= 0 && in->note_map[d1] != key)
                cmd_push(in, (cmd_t){CMD_NOTE_OFF, (uint8_t)in->note_map[d1], 0, 0, 0});
            in->note_map[d1] = (int8_t)key;
            cmd_push(in, (cmd_t){CMD_NOTE_ON, (uint8_t)key, (uint8_t)d2, 0, 0});
            break;
        }
        /* velocity 0 is a note-off */
        __attribute__((fallthrough));
    case 0x80:
        if (in->note_map[d1] >= 0) {
            cmd_push(in, (cmd_t){CMD_NOTE_OFF, (uint8_t)in->note_map[d1], 0, 0, 0});
            in->note_map[d1] = -1;
        }
        break;
    case 0xB0:
        if (d1 == 123 || d1 == 120) {
            memset(in->note_map, -1, sizeof in->note_map);
            cmd_push(in, (cmd_t){CMD_ALL_OFF, 0, 0, 0, 0});
        }
        break;
    default: break;
    }
}

static void set_param(void *instance, const char *key, const char *val) {
    inst_t *in = instance;
    if (!in || !key || !val) return;
    pub_drain(in);
    if (!strcmp(key, "state")) return restore_state(in, val);
    if (!strcmp(key, "preset")) return apply_preset(in, atoi(val));
    if (!strcmp(key, "all_notes_off")) {
        memset(in->note_map, -1, sizeof in->note_map);
        cmd_push(in, (cmd_t){CMD_ALL_OFF, 0, 0, 0, 0});
        return;
    }
    if (!strcmp(key, "octave_transpose")) {
        int o = atoi(val);
        in->octave = o < -4 ? -4 : o > 4 ? 4 : o;
        return;
    }
    if (!strcmp(key, "lyrics")) {
        set_lyrics_text(in, val);
        /* typed words are the player's own */
        if (in->val[CP_WORDS] != 0) {
            in->val[CP_WORDS] = 0;
            send_set(in, CP_WORDS);
        }
        return;
    }
    int id = find_param(key);
    if (id < 0) return;
    float v;
    if (!parse_value(id, val, &v)) return;
    set_value(in, id, v);
}

static int out_str(char *buf, int len, const char *s) {
    size_t n = strlen(s);
    if ((int)n >= len) return -1;
    memcpy(buf, s, n + 1);
    return (int)n;
}

static int get_param(void *instance, const char *key, char *buf, int len) {
    inst_t *in = instance;
    if (!in || !key || !buf || len <= 0) return -1;
    pub_drain(in);
    if (!strcmp(key, "ui_hierarchy")) return out_str(buf, len, cp_ui_hierarchy);
    if (!strcmp(key, "chain_params")) return out_str(buf, len, cp_chain_params);
    if (!strcmp(key, "name")) return snprintf(buf, (size_t)len, "CHIMERA");
    if (!strcmp(key, "state")) return build_state(in, buf, len);
    if (!strcmp(key, "preset")) return snprintf(buf, (size_t)len, "%d", in->preset);
    if (!strcmp(key, "preset_count")) return snprintf(buf, (size_t)len, "%d", CP_PRESET_COUNT);
    if (!strcmp(key, "preset_name")) return snprintf(buf, (size_t)len, "%s", cp_presets[in->preset].name);
    if (!strcmp(key, "octave_transpose")) return snprintf(buf, (size_t)len, "%d", in->octave);
    if (!strcmp(key, "lyrics")) return snprintf(buf, (size_t)len, "%s", in->lyrics);
    if (!strcmp(key, "chm_build")) return snprintf(buf, (size_t)len, "%s", BUILD_ID);
    if (!strcmp(key, "chm_status"))
        return snprintf(buf, (size_t)len, "ready=%d underruns=%u fill=%d", atomic_load(&in->ready), atomic_load(&in->underruns),
                        (int)(atomic_load(&in->aud_w) - atomic_load(&in->aud_r)));
    if (!strcmp(key, "next_syllable")) {
        char tmp[64];
        uint32_t s0, s1;
        do {
            s0 = atomic_load_explicit(&in->syl_seq, memory_order_acquire);
            memcpy(tmp, in->syl, sizeof tmp);
            atomic_thread_fence(memory_order_acquire);
            s1 = atomic_load_explicit(&in->syl_seq, memory_order_acquire);
        } while ((s0 & 1) || s0 != s1);
        tmp[63] = 0;
        return snprintf(buf, (size_t)len, "%s", tmp);
    }
    int id = find_param(key);
    if (id < 0) return -1;
    if (cp_params[id].type == CP_ENUM) return snprintf(buf, (size_t)len, "%d", (int)in->val[id]);
    return snprintf(buf, (size_t)len, "%.4f", in->val[id]);
}

static int get_error(void *instance, char *buf, int len) {
    (void)instance;
    if (buf && len > 0) buf[0] = 0;
    return 0;
}

static void render_block(void *instance, int16_t *out, int frames) {
    inst_t *in = instance;
    if (!out || frames <= 0) return;
    if (!in) {
        memset(out, 0, (size_t)frames * 4);
        return;
    }
    pub_drain(in);
    uint64_t r = atomic_load_explicit(&in->aud_r, memory_order_relaxed);
    /* the transport as of this block's first sample, for the worker's beat prediction */
    double beat = g_host && g_host->get_beat_position ? g_host->get_beat_position() : -1.0;
    double bpm = g_host && g_host->get_bpm ? g_host->get_bpm() : 120.0;
    uint32_t s = atomic_load_explicit(&in->tr_seq, memory_order_relaxed);
    atomic_store_explicit(&in->tr_seq, s + 1, memory_order_release);
    in->tr_beat = beat;
    in->tr_bpm = bpm > 0 ? bpm : 120.0;
    in->tr_read = r;
    atomic_store_explicit(&in->tr_seq, s + 2, memory_order_release);

    uint64_t w = atomic_load_explicit(&in->aud_w, memory_order_acquire);
    int avail = (int)(w - r);
    int n = avail < frames ? avail : frames;
    for (int i = 0; i < n; ++i) {
        float v = in->audio[(r + (uint64_t)i) & (AUDIO_CAP - 1)];
        v = v != v ? 0.0f : v < -1.0f ? -1.0f : v > 1.0f ? 1.0f : v;
        int16_t x = (int16_t)lrintf(v * 32767.0f);
        out[2 * i] = x;
        out[2 * i + 1] = x;
    }
    if (n < frames) {
        memset(out + 2 * n, 0, (size_t)(frames - n) * 4);
        if (atomic_load_explicit(&in->ready, memory_order_relaxed)) atomic_fetch_add(&in->underruns, 1);
    }
    atomic_store_explicit(&in->aud_r, r + (uint64_t)n, memory_order_release);
    sem_post(&in->wake);
}

static plugin_api_v2_t g_api = {
    MOVE_PLUGIN_API_VERSION_2, create_instance, destroy_instance, on_midi, set_param, get_param, get_error, render_block,
};

EXPORT plugin_api_v2_t *move_plugin_init_v2(const host_api_v1_t *host) {
    g_host = host;
    return &g_api;
}
