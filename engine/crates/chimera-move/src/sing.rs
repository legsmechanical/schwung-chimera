//! Sing mode: the lyrics become syllables with a cursor; each key sings the next syllable at its
//! pitch. A port of the app's live Sing controller (`src/ui/singing.ts`, singNoteOn/Off):
//! - a key pressed while the previous one still sounds sings legato (the previous syllable's
//!   coda runs straight into the next onset);
//! - a "-" in the lyrics holds the vowel onto a new pitch (melisma);
//! - letting go of the sounding key while another is down glides back to that key, same vowel;
//! - velocity sets the breath.

use chimera::anatomy::human::BodyMix;
use chimera::control::score::{ControlKey, Gesture};
use chimera::engine::Engine;
use chimera::phonetics::Syllable;
use chimera::phonetics::english::EnglishG2P;
use chimera::phonetics::planner::EXHALE_LABEL;
use chimera::phonetics::sing::{LegatoFrom, MelismaOptions, NoteOnOptions, plan_melisma, plan_note_off, plan_note_on};
use chimera::phonetics::syllables::lyrics_to_syllables;

/// Between two syllables of one word in the Lyrics page's view (never printed).
pub const SYL_SEP: char = '\u{b7}';

/// Gestures are scheduled this far ahead of the voice clock (the app's live lead).
const LEAD: f64 = 0.006;

struct Sounding {
    midi: u8,
    /// the syllable whose vowel is sounding (a melisma keeps its parent's, for the coda)
    syl: Syllable,
    tag: f64,
}

pub struct Singer {
    syllables: Vec<Syllable>,
    /// the syllable the next key sings
    cursor: usize,
    /// the syllable the last key sang (the Lyrics page's highlight); None until a key plays from
    /// where the cursor was put
    sung: Option<usize>,
    sounding: Option<Sounding>,
    /// keys down, oldest first
    keys: Vec<u8>,
    /// a note uses tag (articulation, breath) and tag + 1 (pitch, vibrato)
    next_tag: f64,
    /// endings (glide, coda) that may still be playing: tag, and tag + 1 for the breath-out
    tails: Vec<f64>,
    /// endings whose breath-out has not been dropped by a following note
    exhales_live: bool,
    scratch: Vec<Gesture>,
    /// bumped when the syllables change (the Lyrics page re-reads them)
    serial: u64,
}

impl Default for Singer {
    fn default() -> Self {
        Singer { syllables: Vec::new(), cursor: 0, sung: None, sounding: None, keys: Vec::with_capacity(16), next_tag: 2.0, tails: Vec::with_capacity(9), exhales_live: false, scratch: Vec::new(), serial: 1 }
    }
}

/// sung when the lyrics are empty
fn ah() -> Syllable {
    lyrics_to_syllables("ah", &EnglishG2P::new()).into_iter().next().expect("the letter rules read \"ah\"")
}

/// breath level for a key velocity (1 at 96, the app's on-screen keys)
pub(crate) fn breath_for(velocity: u8) -> f64 {
    let v = velocity as f64;
    0.62 + 0.4 * (v / 96.0).clamp(0.0, 1.0) + 0.12 * ((v - 96.0) / 31.0).max(0.0)
}

impl Singer {
    pub fn set_lyrics(&mut self, text: &str, g2p: &EnglishG2P) {
        self.syllables = lyrics_to_syllables(text, g2p);
        self.cursor = 0;
        self.sung = None;
        self.serial += 1;
    }

    /// The syllable the next key sings.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Move to syllable `i` (held at the last one).
    pub fn set_cursor(&mut self, i: usize) {
        let i = i.min(self.syllables.len().saturating_sub(1));
        if self.sung.take().is_some() || i != self.cursor {
            self.serial += 1;
        }
        self.cursor = i;
    }

    /// The syllable the Lyrics page lights: the one the last key sang, or, since the lyrics or
    /// the cursor were set, the one the next key sings.
    pub fn lit(&self) -> usize {
        self.sung.unwrap_or(self.cursor)
    }

    /// Changes whenever the syllables or the lit syllable do.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// The syllables for the Lyrics page: each one's text, then ' ' after a word's last syllable
    /// or SYL_SEP inside a word; and the serial that changes with them.
    pub fn view(&self, out: &mut String) -> u64 {
        out.clear();
        for s in &self.syllables {
            out.push_str(&s.text);
            out.push(if s.word_end { ' ' } else { SYL_SEP });
        }
        self.serial
    }

    pub fn next_text(&self) -> &str {
        self.syllables.get(self.cursor).map_or("", |s| s.text.as_str())
    }

    fn is_melisma(s: &Syllable) -> bool {
        s.melisma == Some(true)
    }

    /// The syllable a melisma at index i continues.
    fn parent_of(&self, i: usize) -> Syllable {
        for k in (0..=i).rev() {
            if !Self::is_melisma(&self.syllables[k]) {
                return self.syllables[k].clone();
            }
        }
        ah()
    }

    fn schedule(eng: &mut Engine, gs: &[Gesture]) {
        eng.schedule(gs, LEAD, false);
    }

    fn start_note(&mut self, eng: &mut Engine, syl: Syllable, midi: u8, velocity: u8, body: BodyMix, vibrato: f64) {
        let tag = self.next_tag;
        self.next_tag += 2.0;
        // earlier codas may finish, but their breath-out must not open the folds on this onset
        if self.exhales_live {
            for &t in &self.tails {
                eng.release(t + 1.0);
            }
        }
        self.exhales_live = false;
        let from = self.sounding.take();
        if let Some(f) = &from {
            eng.release(f.tag);
            eng.release(f.tag + 1.0);
        }
        let legato_from = from.map(|f| LegatoFrom { syl: f.syl, midi: f.midi as f64 });
        let mut gs = plan_note_on(&syl, midi as f64, &NoteOnOptions { body, vibrato, tag, legato_from });
        let breath = breath_for(velocity);
        for g in gs.iter_mut() {
            if let Some(b) = g.targets.get(ControlKey::BREATH) {
                g.targets.set(ControlKey::BREATH, b * breath);
            }
        }
        Self::schedule(eng, &gs);
        self.sounding = Some(Sounding { midi, syl, tag });
    }

    /// The held vowel moves to a new pitch (melisma, or back to a key still down).
    fn glide(&mut self, eng: &mut Engine, midi: u8, vibrato: f64) {
        let Some(s) = self.sounding.as_mut() else { return };
        eng.release(s.tag + 1.0);
        let gs = plan_melisma(midi as f64, &MelismaOptions { vibrato, tag: s.tag });
        s.midi = midi;
        Self::schedule(eng, &gs);
    }

    pub fn note_on(&mut self, eng: &mut Engine, midi: u8, velocity: u8, body: BodyMix, vibrato: f64) {
        self.keys.retain(|&k| k != midi);
        self.keys.push(midi);
        if self.syllables.is_empty() {
            self.syllables.push(ah());
        }
        let i = self.cursor % self.syllables.len();
        let syl = self.syllables[i].clone();
        if Self::is_melisma(&syl) && self.sounding.is_some() {
            self.glide(eng, midi, vibrato);
        } else if Self::is_melisma(&syl) {
            // a melisma with nothing held: sing its vowel again, without the onset
            let mut p = self.parent_of(i);
            p.onset.clear();
            self.start_note(eng, p, midi, velocity, body, vibrato);
        } else {
            self.start_note(eng, syl, midi, velocity, body, vibrato);
        }
        self.cursor = (i + 1) % self.syllables.len();
        if self.sung != Some(i) {
            self.sung = Some(i);
            self.serial += 1;
        }
    }

    pub fn note_off(&mut self, eng: &mut Engine, midi: u8, body: BodyMix, vibrato: f64) {
        self.keys.retain(|&k| k != midi);
        let Some(s) = &self.sounding else { return };
        if s.midi != midi {
            return;
        }
        if let Some(&last) = self.keys.last() {
            self.glide(eng, last, vibrato);
            return;
        }
        let s = self.sounding.take().unwrap();
        eng.release(s.tag);
        eng.release(s.tag + 1.0);
        // a melisma comes next: this vowel carries on there, and the coda waits for the chain's end
        if self.syllables.get(self.cursor).is_some_and(Self::is_melisma) {
            return;
        }
        let gs = plan_note_off(&s.syl, midi as f64, body);
        if gs.is_empty() {
            return;
        }
        let tag = self.next_tag;
        self.next_tag += 2.0;
        self.scratch.clear();
        self.scratch.extend(gs.into_iter().map(|g| {
            let t = if g.label_str() == EXHALE_LABEL { tag + 1.0 } else { tag };
            g.tag(t)
        }));
        let gs = std::mem::take(&mut self.scratch);
        Self::schedule(eng, &gs);
        self.scratch = gs;
        self.tails.push(tag);
        if self.tails.len() > 8 {
            self.tails.remove(0); // long finished by now
        }
        self.exhales_live = true;
    }

    /// Stop everything sung (mode or body change, all notes off).
    pub fn stop(&mut self, eng: &mut Engine) {
        if let Some(s) = self.sounding.take() {
            eng.release(s.tag);
            eng.release(s.tag + 1.0);
        }
        for &t in &self.tails {
            eng.release(t);
            eng.release(t + 1.0);
        }
        self.tails.clear();
        self.exhales_live = false;
        self.keys.clear();
    }
}
