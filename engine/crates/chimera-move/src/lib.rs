//! CHIMERA on the Move: the performer between the Schwung plugin and the engine.
//!
//! The plugin (src/dsp/chimera_plugin.c) owns the host API, the parameter mirror and a render
//! thread; every call into this crate comes from that ONE thread, never from the audio callback.
//! So everything here may allocate (planning a sung note does, a body or creature switch builds a
//! new airway) and none of it is real-time code in the host's sense. What it must do is keep the
//! render thread ahead of the callback, which the light engine is sized for.
//!
//! What a key does depends on the mode:
//! - Sing: the next syllable of the lyrics at the key's pitch. A key pressed while another is
//!   down sings legato; letting go of the sounding key while another is held glides back to it.
//!   Velocity sets the breath. (A port of the app's `src/ui/singing.ts`.)
//! - Vowel: the key sets the pitch and opens the breath; the mouth is the Mouth/Throat pages.
//! - Beatbox: the key's pitch class picks a sound (see `BEAT_KEYS`); the pattern follows Move's
//!   transport.
//! - Songbird / Seal / Gator / Elephant: the key sets the creature's pitch and breathes, or plays
//!   one of its calls at that pitch (the Keys param).

mod mic;
mod params_gen;
pub mod sing;

use chimera::anatomy::human::{ArticKey, ArticPatch, BodyMix};
use chimera::creatures::creature_by_id;
use chimera::engine::Engine;
use chimera::math::midi_to_hz;
use chimera::control::beat_player::BeatPlayer;
use chimera::phonetics::beatbox::{BEAT_PRESETS, BeatStep, beat_ready, parse_beat};
use chimera::phonetics::english::EnglishG2P;
use chimera::phonetics::planner::vowel_artic;
use chimera::voice::human_voice::SourcePatch;
pub use params_gen::*;
use std::ffi::{CStr, c_char};
use std::io::Read;

pub const MODE_SING: usize = 0;
pub const MODE_VOWEL: usize = 1;
pub const MODE_BEAT: usize = 2;

/// Beatbox: a key's pitch class -> sound. A, A# and B hum a bass note at the key's own pitch.
pub const BEAT_KEYS: [&str; 12] = ["B", "P!", "Pf", "K", "t", "tss", "!", "^K", "kch", "m", "m", "m"];

/// Bodies are rebuilt (a new airway, several ms of work) only once the knobs have rested this long.
const BODY_SETTLE_S: f64 = 0.12;

pub struct Chimera {
    fs: f64,
    eng: Engine,
    g2p: EnglishG2P,
    p: [f64; N_PARAMS],
    sing: sing::Singer,
    /// keys down in Vowel / creature modes, oldest first
    held: Vec<u8>,
    /// the body the engine has, and how long the knobs have rested on a different one
    body: BodyMix,
    body_rest: f64,
    /// beatbox: the sequencer is locked to the transport
    beat_synced: bool,
    gain: f32,
    /// beatbox's close-mic chain, and the output limiter every mode goes through
    mic: mic::Mic,
    limiter: mic::Limiter,
    /// values the controller changed itself (picking a vowel sets the mouth, picking a pattern
    /// its swing), for the plugin's mirror
    published: Vec<(usize, f64)>,
}

fn source_patch(s: Src, v: f64) -> SourcePatch {
    let mut p = SourcePatch::default();
    match s {
        Src::Pressure => p.pressure = Some(v),
        Src::Tension => p.tension = Some(v),
        Src::VibratoRate => p.vibrato_rate = Some(v),
        Src::Glide => p.glide = Some(v / 1000.0),
        Src::Brightness => p.brightness = Some(v),
        Src::Breathiness => p.breathiness = Some(v),
        Src::Roughness => p.roughness = Some(v),
        Src::Fry => p.fry = Some(v),
        Src::Drift => p.drift = Some(v),
    }
    p
}

/// Map a key to a creature's pitch range: whole-octave shift so middle C lands near the middle
/// of the range, stragglers folded in (the app's `creatureKeyHz`).
fn creature_key_hz(midi: f64, min: f64, max: f64) -> f64 {
    let shift = ((min * max).sqrt() / midi_to_hz(60.0)).log2().round();
    let mut f = midi_to_hz(midi) * 2f64.powf(shift);
    while f > max {
        f /= 2.0;
    }
    while f < min {
        f *= 2.0;
    }
    f.min(max)
}

impl Chimera {
    /// `dict_gz`: the CMU pronouncing dictionary (gzip). Without it, words go through the
    /// letter-to-sound rules.
    pub fn new(fs: f64, light: bool, dict_gz: Option<&[u8]>) -> Chimera {
        let mut g2p = EnglishG2P::new();
        if let Some(gz) = dict_gz {
            let mut text = String::new();
            if flate2::read::GzDecoder::new(gz).read_to_string(&mut text).is_ok() {
                g2p.load_dict(&text);
            }
        }
        let mut c = Chimera {
            fs,
            eng: Engine::with_light(fs, light),
            g2p,
            p: DEFAULTS,
            sing: sing::Singer::default(),
            held: Vec::with_capacity(16),
            body: BodyMix::MALE,
            body_rest: 0.0,
            beat_synced: false,
            gain: DEFAULTS[P_VOLUME] as f32,
            mic: mic::Mic::new(fs),
            limiter: mic::Limiter::new(fs),
            published: Vec::new(),
        };
        c.sing.set_lyrics(params_gen_default_lyrics(), &c.g2p);
        c.apply_all();
        c
    }

    pub fn is_light(&self) -> bool {
        self.eng.is_light()
    }

    pub fn mode(&self) -> usize {
        self.p[P_MODE] as usize
    }

    fn creature_slot(&self) -> Option<usize> {
        let m = self.mode();
        (3..=6).contains(&m).then(|| m - 3)
    }

    fn body_mix(&self) -> BodyMix {
        BodyMix::new(self.p[P_FEMALE], self.p[P_CHILD])
    }

    /// Send every param to a fresh engine (start, quality switch).
    fn apply_all(&mut self) {
        self.body = self.body_mix();
        if self.body != BodyMix::MALE {
            self.eng.set_body(self.body);
        }
        for id in 0..N_PARAMS {
            if id != P_MODE && id != P_VOWEL {
                self.apply(id, true);
            }
        }
        self.enter_mode(self.mode());
    }

    /// A param changed (`v` already stored).
    fn apply(&mut self, id: usize, all: bool) {
        let v = self.p[id];
        match DRIVES[id] {
            Drive::Source(s) => self.eng.set_source(&source_patch(s, v)),
            Drive::Artic(k, lo, hi) => {
                if self.mode() != MODE_BEAT {
                    let mut a = ArticPatch::default();
                    a.set(ArticKey::ALL[k], lo + v * (hi - lo));
                    self.eng.set_artic(&a, Some(true));
                }
            }
            Drive::Creature(mode, cid, key) => {
                if self.mode() == mode
                    && let Some(i) = creature_by_id(cid).and_then(|d| d.param_index(key))
                {
                    self.eng.set_creature_params(&[(i, v)]);
                }
            }
            Drive::Own => match id {
                P_FEMALE | P_CHILD => self.body_rest = 0.0,
                P_VIBRATO => self.update_vibrato(),
                P_WHISPER => self.eng.set_source(&SourcePatch { voiced: Some(v < 0.5), ..Default::default() }),
                P_VOLUME => self.gain = v as f32,
                // Keys decides whether a held key breathes (Pitch) or plays a call
                P_HOLD | P_SB_HOLD | P_SE_HOLD | P_GA_HOLD | P_EL_HOLD | P_SB_KEYS | P_SE_KEYS | P_GA_KEYS | P_EL_KEYS => self.update_gate(),
                // a set Vowel is only remembered: the mouth is its own knobs (a restored state's,
                // a preset's); `pick_vowel` is what moves the mouth to the vowel
                P_PATTERN => self.set_pattern(v as usize, false),
                P_SWING => self.eng.set_beat_tempo(None, Some(v)),
                P_MODE if !all => self.enter_mode(v as usize),
                P_MIC => self.mic.reset(),
                _ => {}
            },
        }
    }

    /// Engine: Light or Full. A new engine with every param sent again; the dictionary (the slow
    /// part of starting up) is kept.
    pub fn set_light(&mut self, light: bool) {
        if light == self.eng.is_light() {
            return;
        }
        self.sing.stop(&mut self.eng);
        self.held.clear();
        self.eng = Engine::with_light(self.fs, light);
        self.beat_synced = false;
        self.apply_all();
    }

    /// Set a param (the plugin's set_param / a preset / a restored state). Writing the value a
    /// param already has does nothing: scenes and LFOs re-write values every block.
    pub fn set(&mut self, id: usize, v: f64) {
        if id >= N_PARAMS || !v.is_finite() {
            return;
        }
        if id == P_RESTART {
            self.sing.restart();
            return;
        }
        let prev = self.p[id];
        if prev == v {
            return;
        }
        self.p[id] = v;
        if id == P_MODE {
            if (prev as usize) != (v as usize) {
                self.leave_mode(prev as usize);
                self.enter_mode(v as usize);
            }
            return;
        }
        self.apply(id, false);
    }

    pub fn set_lyrics(&mut self, text: &str) {
        self.sing.set_lyrics(text, &self.g2p);
    }

    fn update_vibrato(&mut self) {
        // sung notes carry their own vibrato gesture; the source's is for Vowel mode
        let depth = if self.mode() == MODE_VOWEL { self.p[P_VIBRATO] } else { 0.0 };
        self.eng.set_source(&SourcePatch { vibrato_depth: Some(depth), ..Default::default() });
    }

    fn hold_on(&self) -> bool {
        match self.mode() {
            MODE_VOWEL => self.p[P_HOLD] > 0.5,
            m if m >= 3 => self.p[CREATURE_KEYS[m - 3].1] > 0.5,
            _ => false,
        }
    }

    /// Keys breathe in Vowel mode and while a creature's keys set its pitch; sung notes and
    /// beatbox sounds bring their own breath, and calls theirs.
    fn update_gate(&mut self) {
        let keys_breathe = match self.mode() {
            MODE_VOWEL => true,
            m if m >= 3 => self.p[CREATURE_KEYS[m - 3].0] < 0.5,
            _ => false,
        };
        let on = self.hold_on() || (keys_breathe && !self.held.is_empty());
        self.eng.set_gate(if on { 1.0 } else { 0.0 });
    }

    fn leave_mode(&mut self, m: usize) {
        self.sing.stop(&mut self.eng);
        self.held.clear();
        self.eng.set_gate(0.0);
        if m == MODE_BEAT {
            self.eng.set_beat_playing(false);
            self.beat_synced = false;
        }
    }

    fn enter_mode(&mut self, m: usize) {
        if let Some(slot) = (3..=6).contains(&m).then(|| m - 3) {
            let def = creature_by_id(CREATURE_IDS[slot]);
            let initial: Vec<(usize, f64)> = match def {
                Some(d) => (0..N_PARAMS)
                    .filter_map(|id| match DRIVES[id] {
                        Drive::Creature(mm, _, key) if mm == m => d.param_index(key).map(|i| (i, self.p[id])),
                        _ => None,
                    })
                    .collect(),
                None => Vec::new(),
            };
            self.eng.set_creature(def, &initial);
        } else {
            self.eng.set_creature(None, &[]);
            if m == MODE_BEAT {
                // a beatboxer waits with the jaw nearly closed; the sounds bring their own breath
                self.eng.set_artic(&beat_ready(self.body), Some(true));
                self.set_pattern(self.p[P_PATTERN] as usize, false);
            } else {
                // the performer's mouth comes back (Sing sings from it as the base posture)
                for id in 0..N_PARAMS {
                    if let Drive::Artic(..) = DRIVES[id] {
                        self.apply(id, true);
                    }
                }
            }
        }
        self.update_vibrato();
        self.update_gate();
    }

    /// The player picked a vowel: the mouth moves to its calibrated posture and the mouth knobs
    /// follow (published back to the plugin).
    pub fn pick_vowel(&mut self, i: usize) {
        if i >= VOWEL_IPA.len() {
            return;
        }
        self.p[P_VOWEL] = i as f64;
        self.set_vowel(i);
    }

    /// The player picked a pattern: its steps, and its own swing (published back).
    pub fn pick_pattern(&mut self, i: usize) {
        self.p[P_PATTERN] = i as f64;
        self.set_pattern(i, true);
    }

    /// The vowel's posture for the body the knobs ask for (during a body change's settle time
    /// the engine still has the old one; the posture is the new body's, which is what stays).
    fn set_vowel(&mut self, i: usize) {
        let Some(ipa) = VOWEL_IPA.get(i) else { return };
        let Some(patch) = vowel_artic(ipa, self.body_mix()) else { return };
        for id in 0..N_PARAMS {
            if let Drive::Artic(k, lo, hi) = DRIVES[id]
                && let Some(x) = patch.get(ArticKey::ALL[k])
                && hi != lo
            {
                let v = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
                self.p[id] = v;
                self.published.push((id, v));
            }
        }
        if self.mode() != MODE_BEAT {
            self.eng.set_artic(&patch, Some(true));
        }
    }

    /// A pattern's steps, and with `take_swing` its own swing (published back).
    fn set_pattern(&mut self, i: usize, take_swing: bool) {
        if i == 0 {
            self.eng.set_beat_playing(false);
            self.beat_synced = false;
            return;
        }
        if let Some(pr) = BEAT_PRESETS.get(i - 1) {
            self.eng.set_beat_steps(&parse_beat(pr.text));
            if take_swing {
                self.p[P_SWING] = pr.swing;
                self.published.push((P_SWING, pr.swing));
            }
            self.eng.set_beat_tempo(None, Some(self.p[P_SWING]));
        }
    }

    /// The host transport at this engine's clock (`beat` < 0: stopped).
    pub fn transport(&mut self, beat: f64, bpm: f64) {
        let wants = self.mode() == MODE_BEAT && self.p[P_PATTERN] >= 1.0 && beat >= 0.0 && bpm > 0.0;
        if !wants {
            if self.beat_synced {
                self.eng.set_beat_playing(false);
                self.beat_synced = false;
            }
            return;
        }
        // a tempo outside what a beatboxer can play is followed at half or double time
        let (mut beat, mut bpm) = (beat, bpm);
        while bpm > BeatPlayer::MAX_BPM {
            bpm *= 0.5;
            beat *= 0.5;
        }
        while bpm < BeatPlayer::MIN_BPM {
            bpm *= 2.0;
            beat *= 2.0;
        }
        // locked: re-anchor when the tempo moved by more than 0.1% (Move's measured tempo
        // wobbles in its last digits from beat to beat), or the phase drifted by > 1/50 beat, or
        // the transport jumped. A re-anchor never skips or repeats a step.
        let drift = (self.eng.beat().beat_at(self.eng.voice().clock) - beat).abs();
        if !self.beat_synced || (self.eng.beat().bpm - bpm).abs() > bpm * 1e-3 || drift > 0.02 {
            self.eng.sync_beat(beat, bpm);
            self.beat_synced = true;
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let midi = note as f64;
        match self.mode() {
            MODE_SING => self.sing.note_on(&mut self.eng, note, velocity, self.body, self.p[P_VIBRATO]),
            MODE_BEAT => {
                let id = BEAT_KEYS[note as usize % 12];
                let mut step = BeatStep::new(id);
                if id == "m" {
                    // a hum sits low: fold the key into E1..D#3
                    let mut n = midi;
                    while n > 51.0 {
                        n -= 12.0;
                    }
                    while n < 28.0 {
                        n += 12.0;
                    }
                    step.note = Some(n);
                }
                step.accent = Some((velocity as f64 / 100.0).clamp(0.5, 1.27));
                self.eng.beat_hit(&step);
            }
            m => {
                self.held.retain(|&k| k != note);
                self.held.push(note);
                if m == MODE_VOWEL {
                    self.eng.set_source(&SourcePatch::f0(midi_to_hz(midi)));
                } else if let Some(slot) = self.creature_slot() {
                    self.creature_note(slot, midi, true);
                }
                self.update_gate();
            }
        }
    }

    fn creature_note(&mut self, slot: usize, midi: f64, pressed: bool) {
        let Some(def) = creature_by_id(CREATURE_IDS[slot]) else { return };
        let Some(pi) = def.params.iter().position(|p| p.role == Some(chimera::creatures::types::ParamRole::Pitch)) else { return };
        let pp = &def.params[pi];
        let f = creature_key_hz(midi, pp.min, pp.max);
        let keys = self.p[CREATURE_KEYS[slot].0] as usize;
        if keys == 0 {
            self.eng.set_creature_params(&[(pi, f)]);
        } else if pressed {
            self.eng.play_call(keys - 1, Some(f), None);
        }
    }

    pub fn note_off(&mut self, note: u8) {
        match self.mode() {
            MODE_SING => self.sing.note_off(&mut self.eng, note, self.body, self.p[P_VIBRATO]),
            MODE_BEAT => {}
            _ => {
                self.held.retain(|&k| k != note);
                // fall back to the most recent key still held (legato)
                if let Some(&last) = self.held.last() {
                    if self.mode() == MODE_VOWEL {
                        self.eng.set_source(&SourcePatch::f0(midi_to_hz(last as f64)));
                    } else if let Some(slot) = self.creature_slot()
                        && self.p[CREATURE_KEYS[slot].0] < 0.5
                    {
                        self.creature_note(slot, last as f64, false);
                    }
                }
                self.update_gate();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        self.sing.stop(&mut self.eng);
        self.held.clear();
        self.update_gate();
    }

    /// Render `out.len()` mono samples.
    pub fn render(&mut self, out: &mut [f32]) {
        // a body change waits for the knobs to rest: each one builds a new airway
        if self.body_mix() != self.body {
            self.body_rest += out.len() as f64 / self.fs;
            if self.body_rest >= BODY_SETTLE_S {
                self.body = self.body_mix();
                self.sing.stop(&mut self.eng);
                self.eng.set_body(self.body);
                self.update_vibrato();
                self.update_gate();
                if self.mode() == MODE_BEAT {
                    self.eng.set_artic(&beat_ready(self.body), Some(true));
                }
            }
        }
        self.eng.render(out);
        let g = self.gain as f64;
        let mic = self.mode() == MODE_BEAT && self.p[P_MIC] > 0.5;
        for s in out.iter_mut() {
            let mut x = *s as f64;
            if mic {
                x = self.mic.run(x);
            }
            *s = self.limiter.run(x * g) as f32;
        }
    }

    /// Drain the values the controller set itself.
    pub fn take_published(&mut self, ids: &mut [u32], vals: &mut [f64]) -> usize {
        let n = self.published.len().min(ids.len()).min(vals.len());
        for (i, (id, v)) in self.published.drain(..n).enumerate() {
            ids[i] = id as u32;
            vals[i] = v;
        }
        n
    }

    /// The syllable the next key sings (Sing mode), for the screen.
    pub fn next_syllable(&self) -> &str {
        self.sing.next_text()
    }

    /// The beat step under the playhead (−1 when the pattern is not playing).
    pub fn beat_step(&self) -> i64 {
        self.eng.beat_step()
    }
}

fn params_gen_default_lyrics() -> &'static str {
    "Breathe in, breathe out, I am made of air. Tongue and lips and throat, singing anywhere."
}

// ------------------------------------------------------------------------------------ C ABI
//
// A panic must never cross into C (undefined behaviour) nor abort the host process, which would
// take Move's whole audio down with it. Every entry point runs under `catch_unwind`; after a
// panic the engine is marked failed, renders silence and ignores everything else, and the
// plugin reports it through `get_error` / `load_error`.

pub struct Handle {
    c: Option<Chimera>,
}

fn guard(h: *mut Handle, f: impl FnOnce(&mut Chimera)) {
    if h.is_null() {
        return;
    }
    let h = unsafe { &mut *h };
    let Some(c) = h.c.as_mut() else { return };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(c))).is_err() {
        h.c = None;
    }
}

/// Build an engine (allocates; call on the render thread). `dict_path` may be NULL. NULL when
/// the engine could not be built.
///
/// # Safety
/// `dict_path` is NULL or a NUL-terminated path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_create(sample_rate: f64, light: i32, dict_path: *const c_char) -> *mut Handle {
    let dict = if dict_path.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(dict_path) }.to_str().ok().and_then(|p| std::fs::read(p).ok())
    };
    match std::panic::catch_unwind(|| Chimera::new(sample_rate, light != 0, dict.as_deref())) {
        Ok(c) => Box::into_raw(Box::new(Handle { c: Some(c) })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `h` came from `chm_create` (or is NULL) and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_destroy(h: *mut Handle) {
    if !h.is_null() {
        let b = unsafe { Box::from_raw(h) };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(b)));
    }
}

/// 1 once the engine has failed (a panic inside it); it then renders silence.
///
/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_failed(h: *mut Handle) -> i32 {
    (h.is_null() || unsafe { &*h }.c.is_none()) as i32
}

/// # Safety
/// `h` is live; `out` holds `n` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_render(h: *mut Handle, out: *mut f32, n: usize) {
    let out = unsafe { std::slice::from_raw_parts_mut(out, n) };
    let mut done = false;
    guard(h, |c| {
        c.render(out);
        done = true;
    });
    if !done {
        out.fill(0.0);
    }
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_set(h: *mut Handle, id: u32, v: f64) {
    guard(h, |c| c.set(id as usize, v));
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_pick_vowel(h: *mut Handle, i: u32) {
    guard(h, |c| c.pick_vowel(i as usize));
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_pick_pattern(h: *mut Handle, i: u32) {
    guard(h, |c| c.pick_pattern(i as usize));
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_set_light(h: *mut Handle, light: i32) {
    guard(h, |c| c.set_light(light != 0));
}

/// # Safety
/// `h` is live; `text` is NUL-terminated UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_set_lyrics(h: *mut Handle, text: *const c_char) {
    let t = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    guard(h, |c| c.set_lyrics(&t));
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_note_on(h: *mut Handle, note: u8, velocity: u8) {
    guard(h, |c| c.note_on(note & 0x7f, velocity & 0x7f));
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_note_off(h: *mut Handle, note: u8) {
    guard(h, |c| c.note_off(note & 0x7f));
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_all_notes_off(h: *mut Handle) {
    guard(h, |c| c.all_notes_off());
}

/// # Safety
/// `h` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_transport(h: *mut Handle, beat: f64, bpm: f64) {
    guard(h, |c| c.transport(beat, bpm));
}

/// # Safety
/// `h` is live; `ids` and `vals` hold `max` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_take_published(h: *mut Handle, ids: *mut u32, vals: *mut f64, max: usize) -> usize {
    let (ids, vals) = unsafe { (std::slice::from_raw_parts_mut(ids, max), std::slice::from_raw_parts_mut(vals, max)) };
    let mut n = 0;
    guard(h, |c| n = c.take_published(ids, vals));
    n
}

/// The next syllable, NUL-terminated into `buf` (cut at a character boundary). Returns its length.
///
/// # Safety
/// `h` is live; `buf` holds `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chm_next_syllable(h: *mut Handle, buf: *mut u8, cap: usize) -> usize {
    if cap == 0 {
        return 0;
    }
    let buf = unsafe { std::slice::from_raw_parts_mut(buf, cap) };
    buf[0] = 0;
    let mut n = 0;
    guard(h, |c| {
        let s = c.next_syllable();
        let mut k = s.len().min(cap - 1);
        while !s.is_char_boundary(k) {
            k -= 1;
        }
        buf[..k].copy_from_slice(&s.as_bytes()[..k]);
        buf[k] = 0;
        n = k;
    });
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_inside_the_engine_is_contained() {
        let h = unsafe { chm_create(44100.0, 1, std::ptr::null()) };
        assert!(!h.is_null());
        assert_eq!(unsafe { chm_failed(h) }, 0);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        guard(h, |_| panic!("test"));
        std::panic::set_hook(hook);
        assert_eq!(unsafe { chm_failed(h) }, 1);
        // every call after is a no-op, and the audio is silence
        let mut out = [1f32; 128];
        unsafe {
            chm_note_on(h, 60, 100);
            chm_render(h, out.as_mut_ptr(), out.len());
        }
        assert!(out.iter().all(|&s| s == 0.0));
        unsafe { chm_destroy(h) };
    }
}
