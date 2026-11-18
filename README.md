# CHIMERA Voice for Move

A physically modeled human voice for [Schwung](https://github.com/charlesvestal/schwung) on Ableton
Move. Every sound comes from a simulated airway (lungs, vocal folds, tongue, lips, nose), not
samples. A port of Jameson Hodge's [CHIMERA](https://github.com/jamesondh/chimera-vocal-synth)
engine, lightened to run on the Move. (CHIMERA's beatbox and creatures are to come as modules of
their own.)

One voice at a time. Pick a mode with **Mode**:

| Mode | What a key does | Its drawn page |
|---|---|---|
| **Speak** | Speaks the whole of the lyrics at the key's pitch, with the rhythm and intonation of speech (a question ends high, a statement falls). A new key starts the line again; letting go does not stop it. Velocity sets the breath. **Rate** is how fast, **Expr** how much the pitch moves. | **Line**: the text, the word being said lit |
| **Sing** | Sings the next syllable of the lyrics at its pitch. Hold a key while playing the next for legato; velocity sets the breath. **Pos** is the syllable the next key sings: turn it to start anywhere. | **Lyrics**: the syllables, the next one lit |
| **Vowel** | Sets the pitch and breathes, as hard as the key was struck. **Vowel** sweeps the mouth through Oo, Oh, Ah, Eh and Ee (the Mouth knobs follow); pad pressure pushes it further along, as far as **Push** says (negative: back the other way). **Hold** drones with no key down. **Mouth** and **Throat** shape it by hand; **Pick** jumps the mouth to one of 13 vowels. | **Vowel**: the mouth, moving with the knob and pressure; **Mouth**: the mouth from its own knobs |

**Words** picks built-in lyrics and **Lyrics** opens the keyboard to type your own (Speak and Sing
share them). The main page also carries Female and Child (the body, blended continuously) and
Volume. **Tone** (breath, pressed, bright, airy, rough, fry, whisper, drift) applies to every mode;
**Pitch** has vibrato rate (Sing, Vowel) and glide (Vowel). The first page is the preset browser;
Schwung's own My Presets page saves your sounds.

Sets saved with CHIMERA 0.3 keep their mode; one saved in Beatbox or a creature mode comes back
in Sing.

## CPU

The engine runs on its own thread, off the audio callback. **Engine** and **Buffer** are the
module's settings in Schwung Manager (`move.local:7700` → Modules → CHIMERA Voice), because they are the
device's CPU budget rather than part of a sound: a scene or an LFO cannot reach them, and a set
saved on a CM5 with Full does not overload a CM4. **Engine**:

- **Light** (default): about 20% of one CM5 core. Roughly 2.2x that on a CM4 (estimated, not
  measured).
- **Full**: the original engine, about 60% of a CM5 core. CM5 only.

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
