/*
 * CHIMERA UI for Schwung / Ableton Move
 *
 * The root screen (preset browsing + octave) comes from the shared sound_generator_ui base,
 * which also ends held notes on an octave change; every parameter page is declared by the
 * plugin's ui_hierarchy (get_param) and drawn by the host's knob grid.
 *
 * MIT (engine: CHIMERA, Jameson Hodge)
 */

import { createSoundGeneratorUI } from '/data/UserData/schwung/shared/sound_generator_ui.mjs';

const ui = createSoundGeneratorUI({
    moduleName: 'CHIMERA',
    showPolyphony: false,
    showOctave: true,
});

globalThis.init                  = ui.init;
globalThis.tick                  = ui.tick;
globalThis.onMidiMessageInternal = ui.onMidiMessageInternal;
globalThis.onMidiMessageExternal = ui.onMidiMessageExternal;
