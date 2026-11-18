/*
 * Load dist/chimera/dsp.so the way the host does and play it at real-time pace: a block every
 * 2.9 ms from this thread (standing in for the SPI callback), notes in every mode, pad pressure,
 * the drawn pages' live values. Reports how long the engine takes to come up, underruns, output
 * level per mode, and the slowest render_block call. Writes the audio to a WAV.
 *
 *   probe <module dir> <out.wav>          (on the Move, from a COPY of dist/chimera: it writes
 *                                          and removes <module dir>/config.json)
 *
 *   build: docker run --rm -v "$PWD:/build" -u "$(id -u):$(id -g)" -w /build chimera-builder \
 *            aarch64-linux-gnu-gcc -O2 -std=gnu11 -Wall tests/probe.c -o build/probe -ldl -lm
 *   run:   scp -r dist/chimera build/probe ableton@move.local:/data/UserData/chimera-probe/
 *          ssh ableton@move.local 'cd /data/UserData/chimera-probe && taskset -c 3 ./probe chimera out.wav'
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "../src/dsp/host/plugin_api_v1.h"

static double g_beat = -1.0;
static float host_bpm(void) { return 100.0f; }
static double host_beat(void) { return g_beat; }
static void host_log(const char *m) { fprintf(stderr, "host log: %s\n", m); }

static double now_s(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec + (double)t.tv_nsec * 1e-9;
}

static plugin_api_v2_t *api;
static void *inst;
static int16_t *wav;
static size_t wav_n, wav_cap;
static double worst_call;
static int failures;

static double cpu_s(void) {
    struct timespec t;
    clock_gettime(CLOCK_THREAD_CPUTIME_ID, &t);
    return (double)t.tv_sec + (double)t.tv_nsec * 1e-9;
}

static void block(void) {
    int16_t out[256];
    /* this thread's own CPU time: on core 3 the probe runs under Move's FIFO-70 audio threads,
     * so wall time would count their preemption as ours */
    double t0 = cpu_s();
    api->render_block(inst, out, 128);
    double dt = cpu_s() - t0;
    if (dt > worst_call) worst_call = dt;
    if (wav_n + 128 > wav_cap) wav = realloc(wav, (wav_cap = wav_cap * 2 + 65536) * sizeof *wav);
    for (int i = 0; i < 128; ++i) wav[wav_n++] = out[2 * i];
    if (g_beat >= 0) g_beat += 128.0 / 44100.0 * 100.0 / 60.0;
}

/* run `secs` of real time at block pace; returns the RMS of what came out */
static double run(double secs) {
    size_t start = wav_n;
    double next = now_s();
    int blocks = (int)(secs * 44100.0 / 128.0);
    for (int b = 0; b < blocks; ++b) {
        next += 128.0 / 44100.0;
        block();
        double wait = next - now_s();
        if (wait > 0) {
            struct timespec ts = {0, (long)(wait * 1e9)};
            nanosleep(&ts, NULL);
        }
    }
    double acc = 0;
    for (size_t i = start; i < wav_n; ++i) acc += (double)wav[i] * wav[i];
    return sqrt(acc / (double)(wav_n - start)) / 32767.0;
}

static void midi(uint8_t s, uint8_t d1, uint8_t d2) {
    uint8_t m[3] = {s, d1, d2};
    api->on_midi(inst, m, 3, 0);
}

static void status(const char *what) {
    char buf[256];
    api->get_param(inst, "chm_status", buf, sizeof buf);
    printf("%-26s %s\n", what, buf);
}

int main(int argc, char **argv) {
    if (argc < 3) return fprintf(stderr, "usage: probe <module dir> <out.wav>\n"), 2;
    char path[600];
    snprintf(path, sizeof path, "%s/dsp.so", argv[1]);
    void *so = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!so) return fprintf(stderr, "dlopen: %s\n", dlerror()), 1;
    move_plugin_init_v2_fn init = (move_plugin_init_v2_fn)dlsym(so, "move_plugin_init_v2");
    static host_api_v1_t host;
    host.api_version = 1;
    host.sample_rate = 44100;
    host.frames_per_block = 128;
    host.log = host_log;
    host.get_bpm = host_bpm;
    host.get_beat_position = host_beat;
    api = init(&host);
    double t0 = now_s();
    inst = api->create_instance(argv[1], "{}");
    printf("create_instance            %.1f ms\n", (now_s() - t0) * 1e3);
    char buf[16384];
    int n = api->get_param(inst, "ui_hierarchy", buf, sizeof buf);
    printf("ui_hierarchy               %d bytes\n", n);
    n = api->get_param(inst, "chain_params", buf, sizeof buf);
    printf("chain_params               %d bytes\n", n);

    /* until the engine is up */
    double t1 = now_s();
    while (now_s() - t1 < 10.0) {
        run(0.05);
        api->get_param(inst, "chm_status", buf, sizeof buf);
        if (strstr(buf, "ready=1") && !strstr(buf, "fill=0 ")) break;
    }
    printf("engine ready after         %.0f ms\n", (now_s() - t1) * 1e3);
    run(0.3);
    worst_call = 0;

    const int notes[4] = {60, 62, 64, 67};
    static const char *const modes[] = {"Speak", "Sing", "Vowel"};
    for (int mode = 0; mode < 3; ++mode) {
        api->set_param(inst, "mode", modes[mode]);
        run(0.3);
        double acc = 0;
        for (int i = 0; i < 4; ++i) {
            midi(0x90, (uint8_t)notes[i], 100);
            acc += run(0.45);
            midi(0x80, (uint8_t)notes[i], 0);
            run(0.1);
        }
        run(0.8);
        printf("mode %d notes rms %.4f      ", mode, acc / 4);
        status("");
    }
    /* Speak: a whole line, then the line re-planned while it is spoken (Rate swept, new
     * lyrics typed, a body change): the planner thread must never hold the render thread up */
    api->set_param(inst, "mode", "Speak");
    run(0.3);
    midi(0x90, 48, 100);
    double rs = run(1.0);
    {
        /* the Line page: how far into the line, growing with the audio played */
        char v1[4096], v2[4096];
        api->get_param(inst, "chm_view", v1, sizeof v1);
        rs += run(0.5);
        api->get_param(inst, "chm_view", v2, sizeof v2);
        long e1 = v1[0] == 'P' ? atol(v1 + 2) : -9, e2 = v2[0] == 'P' ? atol(v2 + 2) : -9;
        int ok = e1 > 800 && e1 < 1300 && e2 - e1 > 400 && e2 - e1 < 700;
        printf("line page elapsed          %ld -> %ld ms: %s\n", e1, e2, ok ? "ok" : "FAIL");
        if (!ok) failures++;
        printf("line page view             %.100s\n", v2);
    }
    rs += run(1.5);
    midi(0x80, 48, 0);
    printf("speak line rms %.4f        ", rs / 3);
    status("");
    midi(0x90, 50, 110);
    for (int i = 0; i <= 10; ++i) {
        char v[16];
        snprintf(v, sizeof v, "%.2f", 0.5 + i * 0.15);
        api->set_param(inst, "rate", v);
        run(0.1);
    }
    midi(0x80, 50, 0);
    api->set_param(inst, "female", "1"); /* a body change cuts the line (a new airway): before the key */
    run(0.3);
    api->set_param(inst, "lyrics", "Four score and seven years ago our fathers brought forth on this continent a new nation, "
                                    "conceived in liberty, and dedicated to the proposition that all men are created equal.");
    midi(0x90, 52, 100);
    run(0.05);
    midi(0x80, 52, 0);
    double rl = run(4.0);
    printf("speak replans rms %.4f     ", rl);
    status("");
    api->set_param(inst, "female", "0");
    api->set_param(inst, "rate", "1");
    api->set_param(inst, "preset", "0");
    run(0.5);

    /* a body sweep: twenty knob writes, one airway rebuild once the knob rests */
    api->set_param(inst, "mode", "Sing");
    run(0.3);
    status("before body sweep");
    for (int i = 0; i <= 20; ++i) {
        char v[16];
        snprintf(v, sizeof v, "%.2f", i / 20.0);
        api->set_param(inst, "female", v);
        run(0.03);
    }
    run(0.4);
    midi(0x90, 67, 100);
    double rb = run(0.6);
    midi(0x80, 67, 0);
    run(0.6);
    printf("female note rms %.4f       ", rb);
    status("");
    /* a preset, a state round trip */
    api->set_param(inst, "preset", "3");
    api->get_param(inst, "preset_name", buf, sizeof buf);
    printf("preset 3                   %s\n", buf);
    n = api->get_param(inst, "state", buf, sizeof buf);
    printf("state                      %d bytes\n", n);
    api->set_param(inst, "state", buf);

    /* Sing's Position: the next syllable, live; a key moves it on */
    {
        api->set_param(inst, "mode", "Sing");
        api->set_param(inst, "lyrics", "one two three four five six");
        run(0.2);
        api->set_param(inst, "position", "3");
        char p1[32], p2[32], pe[32], v[4096];
        api->get_param(inst, "position", p1, sizeof p1);
        midi(0x90, 60, 100);
        run(0.3);
        midi(0x80, 60, 0);
        run(0.3);
        api->get_param(inst, "position:effective", pe, sizeof pe);
        api->get_param(inst, "position", p2, sizeof p2);
        api->get_param(inst, "chm_view", v, sizeof v);
        int ok = atoi(p1) == 3 && atoi(p2) == 4 && atoi(pe) == 4 && !strncmp(v, "S|one two", 9);
        printf("position                   %s -> %s (%s), view %.30s: %s\n", p1, p2, pe, v, ok ? "ok" : "FAIL");
        if (!ok) failures++;
    }

    /* Vowel: the Vowel knob moves the mouth knobs; pad pressure pushes it (live) and the
     * knobs stay; velocity sets the breath */
    {
        api->set_param(inst, "mode", "Vowel");
        api->set_param(inst, "sweep", "0");
        run(0.3);
        char rd[32], j0[32];
        api->get_param(inst, "round", rd, sizeof rd);
        api->get_param(inst, "jaw", j0, sizeof j0);
        api->set_param(inst, "press_amt", "1");
        midi(0x90, 57, 100);
        double rp0 = run(0.5);
        char e0[32], e1[32], e2[32], j1[32];
        api->get_param(inst, "sweep:effective", e0, sizeof e0);
        midi(0xA0, 57, 127);
        double rp1 = run(0.5);
        api->get_param(inst, "sweep:effective", e1, sizeof e1);
        api->get_param(inst, "jaw", j1, sizeof j1);
        midi(0x80, 57, 0);
        run(0.4);
        api->get_param(inst, "sweep:effective", e2, sizeof e2);
        int ok = atof(rd) > 0.8 && atof(e0) == 0 && atof(e1) > 0.99 && atof(e2) == 0 && !strcmp(j0, j1);
        printf("vowel knob + pressure      round %s, effective %s -> %s -> %s, jaw %s -> %s: %s\n", rd, e0, e1, e2, j0, j1, ok ? "ok" : "FAIL");
        printf("  rms unpressed %.4f pressed %.4f\n", rp0, rp1);
        if (!ok) failures++;
        midi(0x90, 57, 25);
        double soft = run(0.6);
        midi(0x80, 57, 0);
        run(0.6);
        midi(0x90, 57, 127);
        double hard = run(0.6);
        midi(0x80, 57, 0);
        run(0.6);
        ok = hard > soft * 1.3;
        printf("vowel velocity             soft %.4f hard %.4f: %s\n", soft, hard, ok ? "ok" : "FAIL");
        if (!ok) failures++;
        api->set_param(inst, "sweep", "0.25");
        run(0.2);
    }

    /* a saved mouth survives a reload: pick a vowel, move the jaw, save, and restore into a
     * fresh instance (neither the vowel's posture nor the Vowel knob's may overwrite the knobs) */
    api->set_param(inst, "mode", "Vowel");
    api->set_param(inst, "vowel", "9");
    run(0.3);
    api->set_param(inst, "jaw", "0.9100");
    run(0.2);
    n = api->get_param(inst, "state", buf, sizeof buf);
    void *inst2 = api->create_instance(argv[1], "{}");
    api->set_param(inst2, "state", buf);
    {
        void *keep = inst;
        inst = inst2;
        run(1.5);
        char jaw[32], sw[32], vw[32];
        api->get_param(inst2, "jaw", jaw, sizeof jaw);
        api->get_param(inst2, "sweep", sw, sizeof sw);
        api->get_param(inst2, "vowel", vw, sizeof vw);
        int ok = fabs(atof(jaw) - 0.91) < 1e-3 && fabs(atof(sw) - 0.25) < 1e-3 && atoi(vw) == 9;
        printf("restore keeps knobs        jaw %s vowel knob %s vowel %s: %s\n", jaw, sw, vw, ok ? "ok" : "FAIL");
        if (!ok) failures++;
        api->destroy_instance(inst2);
        inst = keep;
    }

    /* a state saved before Speak (v1): Vowel was mode 1, and must come back as Vowel */
    {
        void *inst3 = api->create_instance(argv[1], "{}");
        api->set_param(inst3, "state", "{\"v\":1,\"preset\":0,\"lyrics\":\"ah\",\"p\":{\"mode\":1,\"jaw\":0.25}}");
        char m[32], jaw[32];
        api->get_param(inst3, "mode", m, sizeof m);
        api->get_param(inst3, "jaw", jaw, sizeof jaw);
        int ok = atoi(m) == 2 && fabs(atof(jaw) - 0.25) < 1e-3;
        printf("v1 state migrates          mode %s jaw %s: %s\n", m, jaw, ok ? "ok" : "FAIL");
        if (!ok) failures++;
        /* v2: Sing 0, Speak 1, Beatbox 3 (not in this module: back as Sing) */
        api->set_param(inst3, "state", "{\"v\":2,\"preset\":0,\"lyrics\":\"ah\",\"p\":{\"mode\":1}}");
        api->get_param(inst3, "mode", m, sizeof m);
        ok = atoi(m) == 0;
        api->set_param(inst3, "state", "{\"v\":2,\"preset\":0,\"lyrics\":\"ah\",\"p\":{\"mode\":3}}");
        char m3[32];
        api->get_param(inst3, "mode", m3, sizeof m3);
        ok = ok && atoi(m3) == 1;
        printf("v2 state migrates          Speak -> %s, Beatbox -> %s: %s\n", m, m3, ok ? "ok" : "FAIL");
        if (!ok) failures++;
        api->get_param(inst, "state", buf, sizeof buf);
        api->set_param(inst3, "state", buf);
        api->get_param(inst3, "mode", m, sizeof m);
        char m0[32];
        api->get_param(inst, "mode", m0, sizeof m0);
        ok = !strcmp(m, m0);
        printf("v3 state keeps the mode    %s -> %s: %s\n", m0, m, ok ? "ok" : "FAIL");
        if (!ok) failures++;
        api->destroy_instance(inst3);
    }

    /* the Full engine, through the module's settings file (probe runs on a copy of the module) */
    char cfg[1024];
    snprintf(cfg, sizeof cfg, "%s/config.json", argv[1]);
    FILE *cf = fopen(cfg, "w");
    if (cf) {
        fputs("{\"engine\":\"full\",\"buffer\":\"normal\"}\n", cf);
        fclose(cf);
    }
    api->set_param(inst, "mode", "Sing");
    run(2.5);
    status("after engine=full");
    midi(0x90, 60, 100);
    double rf = run(0.8);
    midi(0x80, 60, 0);
    run(0.8);
    printf("full engine note rms %.4f  ", rf);
    status("");
    remove(cfg);
    printf("slowest render_block       %.1f us (own CPU time)\n", worst_call * 1e6);

    t0 = now_s();
    api->destroy_instance(inst);
    printf("destroy_instance           %.1f ms\n", (now_s() - t0) * 1e3);

    FILE *f = fopen(argv[2], "wb");
    if (f) {
        uint32_t bytes = (uint32_t)(wav_n * 2);
        fwrite("RIFF", 1, 4, f);
        uint32_t x = 36 + bytes;
        fwrite(&x, 4, 1, f);
        fwrite("WAVEfmt ", 1, 8, f);
        x = 16; fwrite(&x, 4, 1, f);
        uint16_t s = 1; fwrite(&s, 2, 1, f);
        fwrite(&s, 2, 1, f);
        x = 44100; fwrite(&x, 4, 1, f);
        x = 88200; fwrite(&x, 4, 1, f);
        s = 2; fwrite(&s, 2, 1, f);
        s = 16; fwrite(&s, 2, 1, f);
        fwrite("data", 1, 4, f);
        fwrite(&bytes, 4, 1, f);
        fwrite(wav, 2, wav_n, f);
        fclose(f);
    }
    if (failures) printf("%d check(s) FAILED\n", failures);
    return failures ? 1 : 0;
}
