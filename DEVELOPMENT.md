# Development

## Layout

| Path | What |
|---|---|
| `src/params.json` | THE table: every param, page, preset and lyric set. Everything below is generated from it. |
| `src/settings-schema.json` | Engine and Buffer, as per-module settings (Schwung Manager writes `config.json` beside the module; the render thread reads it) |
| `tools/gen.mjs` | → `src/module.json`, `src/dsp/params_gen.h`, `engine/crates/chimera-move/src/params_gen.rs`, `build/contract.json` |
| `src/dsp/chimera_plugin.c` | Plugin API v2: the param mirror, presets/state, and the render thread with its rings |
| `engine/crates/chimera-move/` | The performer (keys → sung syllables, vowels, beatbox, creatures), the close-mic chain, a C ABI |
| `engine/crates/chimera/` | The CHIMERA engine, vendored from upstream at `engine/UPSTREAM_COMMIT`, with the Move port's changes (`engine/move-port.patch` is the light path's first two commits) |
| `tools/cells.mjs` | Plans every mode's pages with the Move's own `param_pages` code and prints each cell's label and widget |
| `tests/probe.c` | Loads `dsp.so` on the Move outside the host, plays every mode at real-time pace, reports underruns |
| `tests/device_check.py` | Loads the module into slot 0 of the live host through `schwung-testd` (refuses if slot 0 is in use or an overtake tool owns the pads) |

`./scripts/test.sh` runs the performer's tests natively; `--engine` adds the whole engine suite,
which must stay green: the light path is opt-in and the original is unchanged.

## Threading

Every plugin entry point runs on the SPI callback, and one voice costs ~20% (light) to ~60% (full)
of a CM5 core with blocks that spike far above average, so the engine runs on a thread of its own.
It is created in `create_instance` (the host's SCHED_OTHER loader thread), sets itself SCHED_FIFO 10
(below Move's Link Main at 35) pinned to cores 0-2, and renders Buffer blocks ahead into a ring.
`render_block` copies from the ring and posts a semaphore. The callback never waits on the render
thread.

- **Param values are latest-wins**, not queued: a value per id plus a dirty bitmap. A scene or LFO
  writing every block costs nothing, and nothing can be dropped. Writing an unchanged value does
  nothing on either side.
- **The cmd ring** carries only notes, all-notes-off, Again and *picks*, with 64 slots kept for
  note-offs (a note-on that does not fit is dropped whole; a note-off that does not fit silences
  everything).
- **Picks**: choosing a vowel moves the mouth knobs, choosing a pattern takes its swing. Only a
  write that changes Vowel or Pattern is a pick; a restored state or a preset sets them quietly
  (a preset then picks its vowel, so its posture wins over the table's default mouth). Every write
  has a generation number, and a pick's results never replace a value written after it, on either
  side: that is what keeps a saved state's mouth and swing when the set reloads.
- **A panic** in the engine is caught at the C ABI (`panic = "unwind"`); the engine then renders
  silence and `get_error` / `load_error` say so. It never takes the host down.

Measured on a CM5 with the callback pinned to core 3 (`tests/probe.c`, run from a copy of the
module): at most ~22 µs of callback CPU per block, zero underruns through every mode, a body sweep
and a switch to Full through the settings file; a saved mouth and swing survive a restore into a
fresh instance.

## The light engine (what was traded)

Measured on a CM5 (2.4 GHz, Move running), one voice at 44.1 kHz:

| | Full | Light |
|---|---|---|
| Human voice | 58-65% | ~20% |
| Songbird / Seal / Gator / Elephant | 41 / 69 / 106 / 66% | 14 / 10 / 16 / 29% |

- **Airway at 1x** (88.2 kHz → 44.1 kHz). Stable for all three bodies: the section counts already
  scale with the rate (Courant 0.970 against the 0.971 limit). Costs a top end 5-8 dB brighter at
  4-8 kHz and wider high formants.
- **Geometry coefficients refreshed every 4 samples**, ramped linearly in between. Holding them
  flat turned the steady airflow into a click train at fs/4. Ports driven per sample (glottis,
  syrinx valves) keep the exact per-sample solve, and so does the stability floor of a junction
  next to one: a glottis closed at a refresh and open three samples later would otherwise leave
  that junction without its share of the floor until the next refresh.
- **Tube shape rebuilt every 128 samples** (was 64); the articulators still integrate every 64
  (their semi-implicit step is near its limit for fast gestures at 128).
- **Creatures** at half their rate (songbird 4x → 2x, seal and gator 2x → 1x) with longer
  sections (seal and gator x1.5, elephant x2). A tube's minimum section count yields to the time
  step in light mode: the songbird's beak sections at 2x were shorter than one step of sound and
  its airway went NaN, silently, after the first call.

The engine's speech battery (`chimera diagnose`, `CHIMERA_OS=1 CHIMERA_GEO=4 CHIMERA_DIV=2`)
scores light against full: vowel F1/F2 errors 6-10% both, durations and pitch identical,
unstressed F2 7% → 11%, B3 155 → 245 Hz.

## The beat sync

The pattern follows Move's transport: `transport()` hands the host's beat and tempo to
`BeatPlayer::sync` (a Move-port addition). A re-sync continues from the first step not yet
scheduled, so it never skips or repeats one; it re-anchors when the tempo moves by more than 0.1%
(Move's measured tempo wobbles in its last digits every beat; the first version re-anchored on
every wobble and pushed the pattern ahead of itself, playing nothing), when the phase drifts, or
when the transport jumps. A tempo outside 40-240 bpm is followed at half or double time.

## Not ported

- Speak (text to speech) and singing lyrics to a built-in tune: the keys sing instead.
- The app's visuals (the head cutaway, spectra).
- Korean and Russian lyrics work in the engine, but the Move's keyboard types ASCII.
