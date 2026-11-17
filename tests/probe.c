/*
 * Load dist/chimera/dsp.so the way the host does and play it at real-time pace: a block every
 * 2.9 ms from this thread (standing in for the SPI callback), notes in every mode, a running
 * transport for the beatbox pattern. Reports how long the engine takes to come up, underruns,
 * output level per mode, and the slowest render_block call. Writes the audio to a WAV.
 *
 *   probe <module dir> <out.wav>          (on the Move: built by scripts/build_probe.sh)
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
    for (int mode = 0; mode < 7; ++mode) {
        char v[8];
        snprintf(v, sizeof v, "%d", mode);
        api->set_param(inst, "mode", v);
        run(0.3);
        if (mode == 2) {
            g_beat = 0.0;
            double r = run(2.4);
            printf("mode %d pattern rms %.4f    ", mode, r);
            g_beat = -1.0;
            status("");
        }
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
    /* a body sweep: twenty knob writes, one airway rebuild once the knob rests */
    api->set_param(inst, "mode", "0");
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
    /* a preset, a state round trip, the Full engine */
    api->set_param(inst, "preset", "3");
    api->get_param(inst, "preset_name", buf, sizeof buf);
    printf("preset 3                   %s\n", buf);
    n = api->get_param(inst, "state", buf, sizeof buf);
    printf("state                      %d bytes\n", n);
    api->set_param(inst, "state", buf);
    api->set_param(inst, "quality", "Full");
    run(1.0);
    midi(0x90, 60, 100);
    double rf = run(0.8);
    midi(0x80, 60, 0);
    run(0.8);
    printf("full engine note rms %.4f  ", rf);
    status("");
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
    return 0;
}
