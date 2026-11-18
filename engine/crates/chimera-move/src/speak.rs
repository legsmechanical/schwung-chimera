//! Speak mode: a key speaks the whole line (the lyrics) with the engine's own speech planner
//! (CMUdict G2P, Klatt durations, phrase intonation from the punctuation), at the key's pitch.
//!
//! - The key sets the speaking pitch: the line is planned once at a reference pitch and every
//!   pitch target is scaled by key / reference (the planner's intonation is proportional to its
//!   base pitch, so this is the same line planned at the key's pitch).
//! - A key pressed while a line is being spoken cuts it (its gestures end, those not begun are
//!   dropped) and starts the line again. Letting go does nothing: the line plays out.
//! - Velocity sets the breath, as in Sing.
//!
//! Planning a line allocates and takes milliseconds per sentence, too long for the render thread,
//! which must stay ahead of the audio callback. It runs on a planner thread of its own at normal
//! (SCHED_OTHER) priority; the render thread only asks (a channel send) and picks the result up
//! without waiting. A line is re-planned when its text, the body, Rate or Expression changes. A key
//! pressed while the line it would speak is still being planned waits for it (up to
//! [`PENDING_MAX_S`]). The planned gestures are handed to the voice a short horizon ahead of the
//! clock, so a cut has little to drop and the score never holds a whole long line.

use chimera::anatomy::human::BodyMix;
use chimera::control::score::{ControlKey, Gesture};
use chimera::engine::Engine;
use chimera::math::midi_to_hz;
use chimera::phonetics::english::EnglishG2P;
use chimera::phonetics::planner::{PhraseInput, PlanOptions, plan_phrases};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;

/// the pitch lines are planned at (Hz); a key's pitch scales every pitch target from it
const REF_F0: f64 = 100.0;
/// gestures go to the voice this far ahead of the clock (s): past the planner's earliest lead-in
const HORIZON: f64 = 0.25;
/// a line starts this far after the key (s), as Sing's notes
const LEAD: f64 = 0.006;
/// a key waits at most this long for its line to be planned (s); later, it is forgotten
pub const PENDING_MAX_S: f64 = 1.0;
/// the planner's default pitch excursion of a stressed syllable, at Expression's default
const ACCENT_AT_DEFAULT: f64 = 0.14;
const EXPRESSION_DEFAULT: f64 = 0.35;
/// how long after the last gesture's end a line still counts as speaking (s)
const TAIL: f64 = 0.05;

/// What a line is planned from.
#[derive(Clone, Debug, PartialEq)]
pub struct LineKey {
    pub text: String,
    pub body: BodyMix,
    pub rate: f64,
    pub expression: f64,
}

struct Request {
    serial: u64,
    key: LineKey,
}

/// A planned line: gestures (times from 0) sorted by when they start acting.
/// Shared, so a line being spoken keeps its own gestures when a new plan arrives.
pub struct Line {
    serial: u64,
    gestures: Arc<[Gesture]>,
    end: f64,
}

/// The planned line for `key` (allocates).
pub fn plan_line(g2p: &EnglishG2P, key: &LineKey) -> (Vec<Gesture>, f64) {
    let rate = key.rate.clamp(0.25, 4.0);
    let inputs: Vec<PhraseInput> = g2p
        .to_phrases(&key.text)
        .into_iter()
        .map(|p| PhraseInput { segments: p.segments, intonation: Some(p.intonation), pause: Some(p.pause / rate) })
        .collect();
    if inputs.is_empty() {
        return (Vec::new(), 0.0);
    }
    let accent = ACCENT_AT_DEFAULT * key.expression.max(0.0) / EXPRESSION_DEFAULT;
    let plan = plan_phrases(&inputs, &PlanOptions { body: key.body, t0: 0.0, rate: Some(rate), f0: Some(REF_F0), accent: Some(accent), ..Default::default() });
    let mut gs = plan.gestures;
    gs.sort_by(|a, b| start_of(a).total_cmp(&start_of(b)));
    let end = gs.iter().map(Gesture::end).fold(plan.end, f64::max);
    (gs, end)
}

/// when a gesture starts acting
fn start_of(g: &Gesture) -> f64 {
    g.t0 - g.ramp_in
}

struct Playing {
    /// the line being spoken (a plan that arrives meanwhile is for the next key)
    gestures: Arc<[Gesture]>,
    end: f64,
    /// the line's time 0 on the voice clock
    off: f64,
    ratio: f64,
    breath: f64,
    tag: f64,
    /// the next gesture to hand to the voice
    idx: usize,
}

pub struct Speaker {
    tx: Option<Sender<Request>>,
    rx: Receiver<Line>,
    thread: Option<JoinHandle<()>>,
    /// what the latest request asked for, and its generation
    want: LineKey,
    asked: u64,
    line: Line,
    playing: Option<Playing>,
    /// a key waiting for its line: note, velocity, seconds waited
    pending: Option<(u8, u8, f64)>,
    next_tag: f64,
    scratch: Vec<Gesture>,
    g2p: Arc<EnglishG2P>,
}

/// The planner thread runs at normal priority. Threads inherit their creator's policy, and the
/// render thread is SCHED_FIFO: a FIFO planner would hold its core for a whole plan.
#[cfg(target_os = "linux")]
fn normal_priority() {
    #[repr(C)]
    struct SchedParam {
        sched_priority: i32,
    }
    unsafe extern "C" {
        fn sched_setscheduler(pid: i32, policy: i32, param: *const SchedParam) -> i32;
    }
    let p = SchedParam { sched_priority: 0 };
    // SCHED_OTHER = 0; pid 0 is the calling thread
    unsafe { sched_setscheduler(0, 0, &p) };
}
#[cfg(not(target_os = "linux"))]
fn normal_priority() {}

impl Speaker {
    /// Plans `key` now (on the caller's thread, before any audio) and starts the planner thread.
    pub fn new(g2p: Arc<EnglishG2P>, key: LineKey) -> Speaker {
        let (gestures, end) = plan_line(&g2p, &key);
        let shared = g2p.clone();
        let (req_tx, req_rx) = channel::<Request>();
        let (line_tx, line_rx) = channel::<Line>();
        let thread = std::thread::Builder::new()
            .name("chimera-plan".into())
            .spawn(move || {
                normal_priority();
                while let Ok(mut r) = req_rx.recv() {
                    // only the latest request matters
                    while let Ok(n) = req_rx.try_recv() {
                        r = n;
                    }
                    let planned = std::panic::catch_unwind(|| plan_line(&g2p, &r.key));
                    let (gestures, end) = planned.unwrap_or_default();
                    if line_tx.send(Line { serial: r.serial, gestures: gestures.into(), end }).is_err() {
                        return;
                    }
                }
            })
            .ok();
        Speaker {
            tx: Some(req_tx),
            rx: line_rx,
            thread,
            want: key,
            asked: 0,
            line: Line { serial: 0, gestures: gestures.into(), end },
            playing: None,
            pending: None,
            // Sing's tags are even numbers from 2 up; these never meet them
            next_tag: 1e9,
            scratch: Vec::with_capacity(512),
            g2p: shared,
        }
    }

    /// The line to speak changed (text, body, rate or expression): plan it again.
    pub fn want(&mut self, key: LineKey) {
        if key == self.want {
            return;
        }
        self.want = key;
        self.asked += 1;
        let req = Request { serial: self.asked, key: self.want.clone() };
        match &self.tx {
            Some(tx) if self.thread.is_some() => {
                if tx.send(req).is_err() {
                    self.plan_here();
                }
            }
            _ => self.plan_here(),
        }
    }

    /// no planner thread (it could not start): plan on this thread
    fn plan_here(&mut self) {
        let (gestures, end) = plan_line(&self.g2p, &self.want);
        self.line = Line { serial: self.asked, gestures: gestures.into(), end };
    }

    pub fn wanted(&self) -> &LineKey {
        &self.want
    }

    /// true while the line for the latest request is not here yet
    pub fn planning(&self) -> bool {
        self.line.serial != self.asked
    }

    /// a line is being spoken, or a key waits for its line
    pub fn speaking(&self) -> bool {
        self.playing.is_some() || self.pending.is_some()
    }

    pub fn note_on(&mut self, eng: &mut Engine, note: u8, velocity: u8) {
        self.cut(eng);
        if self.planning() {
            self.pending = Some((note, velocity, 0.0));
            return;
        }
        self.start(eng, note, velocity);
    }

    fn start(&mut self, eng: &mut Engine, note: u8, velocity: u8) {
        if self.line.gestures.is_empty() {
            return;
        }
        let tag = self.next_tag;
        self.next_tag += 1.0;
        self.playing = Some(Playing { gestures: self.line.gestures.clone(), end: self.line.end, off: eng.clock() + LEAD, ratio: midi_to_hz(note as f64) / REF_F0, breath: crate::sing::breath_for(velocity), tag, idx: 0 });
    }

    /// End the line being spoken (its gestures ramp out; those not begun are dropped).
    pub fn cut(&mut self, eng: &mut Engine) {
        self.pending = None;
        if let Some(p) = self.playing.take() {
            eng.release(p.tag);
        }
    }

    /// Before each render: pick up a planned line, start a waiting key, hand the voice the
    /// gestures that start within the horizon.
    pub fn tick(&mut self, eng: &mut Engine, dt: f64) {
        loop {
            match self.rx.try_recv() {
                Ok(l) => {
                    if l.serial > self.line.serial {
                        self.line = l;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // the planner died: plan here from now on
                    self.thread = None;
                    if self.planning() {
                        self.plan_here();
                    }
                    break;
                }
            }
        }
        if let Some((note, vel, waited)) = self.pending {
            if !self.planning() {
                self.pending = None;
                self.start(eng, note, vel);
            } else if waited + dt > PENDING_MAX_S {
                self.pending = None;
            } else {
                self.pending = Some((note, vel, waited + dt));
            }
        }
        let Some(p) = self.playing.as_mut() else { return };
        let now = eng.clock();
        let gs = &p.gestures;
        self.scratch.clear();
        while p.idx < gs.len() && p.off + start_of(&gs[p.idx]) <= now + HORIZON {
            let mut g = gs[p.idx];
            g.t0 += p.off;
            g.t1 += p.off;
            if let Some(f) = g.targets.get(ControlKey::F0) {
                g.targets.set(ControlKey::F0, f * p.ratio);
            }
            if let Some(b) = g.targets.get(ControlKey::BREATH) {
                g.targets.set(ControlKey::BREATH, b * p.breath);
            }
            g.tag = Some(p.tag);
            self.scratch.push(g);
            p.idx += 1;
        }
        if !self.scratch.is_empty() {
            eng.voice_mut().schedule(&self.scratch);
        }
        if p.idx >= gs.len() && now > p.off + p.end + TAIL {
            self.playing = None;
        }
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        // the thread runs this library's code: it must be gone before the library can be unloaded
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
