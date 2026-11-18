# CHIMERA for Move

A physically modeled voice for [Schwung](https://github.com/charlesvestal/schwung) on Ableton Move.
Every sound comes from a simulated airway (lungs, vocal folds, tongue, lips, nose), not samples. A
port of Jameson Hodge's [CHIMERA](https://github.com/jamesondh/chimera-vocal-synth) engine,
lightened to run on the Move.

One voice at a time. Pick a mode on the **Play** page:

| Mode | What a key does |
|---|---|
| **Sing** | Sings the next syllable of the lyrics at its pitch. Hold a key while playing the next for legato; velocity sets the breath. **Words** picks built-in lyrics, **Lyrics** opens the keyboard to type your own, **Again** goes back to the first word. |
| **Vowel** | Sets the pitch and breathes. Shape the sound by hand on **Throat** and **Mouth**; **Vowel** jumps the mouth to a vowel and the knobs follow. **Hold** drones with no key down. |
| **Beatbox** | Pads by note name: C kick, C# lip pop, D snare, D# rimshot, E hi-hat, F open hat, F# tongue click, G inward snare, G# crash, A to B hum a bass note. **Beat** plays a pattern in time with Move's transport. **Mic** is the close-mic sound (proximity bass, compression). |
| **Songbird, Seal, Gator, Elephant** | **Keys: Pitch** plays the creature at the key's pitch while held; the other settings play one of its calls at the key's pitch. **Hold** keeps it breathing. |

**Play** always starts Mode, Volume; then the mode's own controls (Hold and Keys for Vowel and
the creatures). **Voice** (body: Female and Child blend continuously; breath, pressed, vibrato,
glide) and **Tone** (bright, airy, rough, fry, whisper, drift) apply to Sing, Vowel and Beatbox.
Every creature page starts Pitch, Breath. The first page is the preset browser; Schwung's own My
Presets page saves your sounds.

## CPU

The engine runs on its own thread, off the audio callback. **Engine** and **Buffer** are the
module's settings in Schwung Manager (`move.local:7700` → Modules → CHIMERA), because they are the
device's CPU budget rather than part of a sound: a scene or an LFO cannot reach them, and a set
saved on a CM5 with Full does not overload a CM4. **Engine**:

- **Light** (default): about 20% of one CM5 core for the voice, 11-29% for the creatures. Roughly
  2.2x that on a CM4 (estimated, not measured).
- **Full**: the original engine, about 60% of a CM5 core (the alligator cannot keep up). CM5 only.

**Buffer** sets how far ahead the engine renders: Low 6 ms, Normal 12 ms, Safe 23 ms. Raise it if
you hear dropouts. Both take effect within a second of saving, without reloading the module.

## Install

From a release: Schwung Manager → Install Custom Module → this repository's URL. Or build it:

```bash
./scripts/build.sh      # needs Docker and node
./scripts/install.sh    # copies dist/chimera to the Move (MOVE_HOST overrides move.local)
```

The installer never restarts anything. A module the host has already loaded keeps its old code
until the host restarts.

## License

MIT, as is the CHIMERA engine (`engine/`). The CMU Pronouncing Dictionary keeps its own license
(`engine/public/data/CMUDICT-LICENSE.txt`). Build notes are in [DEVELOPMENT.md](DEVELOPMENT.md).
