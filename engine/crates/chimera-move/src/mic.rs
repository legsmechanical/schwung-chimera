//! The app's "close mic" chain for beatboxing (beatboxers cup the mic), which the browser builds
//! from WebAudio nodes outside the engine (`src/audio/AudioEngine.ts`): proximity bass (low shelf
//! 120 Hz +9 dB), presence (peaking 3.5 kHz, Q 0.8, +3 dB), a fast compressor (threshold -28 dB,
//! knee 8 dB, ratio 5, attack 2 ms, release 120 ms) and make-up x2.2.
//!
//! WebAudio's DynamicsCompressor also applies an automatic make-up gain (Chromium:
//! (1 / full-range gain)^0.6, the curve's gain at 0 dBFS): with these settings 0 dBFS comes out
//! at -22.4 dB, so +13.4 dB. Without it the beatbox sits ~15x below the sung voice.
//!
//! Plus the output stage every mode goes through: a -1 dBFS peak limiter (instant attack,
//! smooth release), because the Move's output is int16 and the mic's make-up can exceed full scale.

use std::f64::consts::PI;

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ cookbook low shelf, shelf slope 1 (WebAudio's lowshelf ignores Q).
    fn low_shelf(fs: f64, f: f64, gain_db: f64) -> Biquad {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * f / fs;
        let (sw, cw) = w.sin_cos();
        let alpha = sw / 2.0 * 2f64.sqrt(); // S = 1
        let sa = 2.0 * a.sqrt() * alpha;
        let b0 = a * ((a + 1.0) - (a - 1.0) * cw + sa);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cw);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cw - sa);
        let a0 = (a + 1.0) + (a - 1.0) * cw + sa;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cw);
        let a2 = (a + 1.0) + (a - 1.0) * cw - sa;
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }

    /// RBJ cookbook peaking EQ.
    fn peaking(fs: f64, f: f64, q: f64, gain_db: f64) -> Biquad {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * f / fs;
        let (sw, cw) = w.sin_cos();
        let alpha = sw / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Biquad {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cw / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cw / a0,
            a2: (1.0 - alpha / a) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

const THRESHOLD_DB: f64 = -28.0;
const KNEE_DB: f64 = 8.0;
const RATIO: f64 = 5.0;
const MAKEUP: f64 = 2.2;

/// Static curve with a soft knee: output level (dB) for an input level (dB).
fn curve_db(x: f64) -> f64 {
    let lo = THRESHOLD_DB - KNEE_DB / 2.0;
    let hi = THRESHOLD_DB + KNEE_DB / 2.0;
    if x <= lo {
        x
    } else if x >= hi {
        THRESHOLD_DB + (x - THRESHOLD_DB) / RATIO
    } else {
        let d = x - lo;
        x + (1.0 / RATIO - 1.0) * d * d / (2.0 * KNEE_DB)
    }
}

pub struct Mic {
    shelf: Biquad,
    presence: Biquad,
    env_db: f64,
    att: f64,
    rel: f64,
    auto_makeup: f64,
}

impl Mic {
    pub fn new(fs: f64) -> Mic {
        let full_range_db = curve_db(0.0); // gain at 0 dBFS
        Mic {
            shelf: Biquad::low_shelf(fs, 120.0, 9.0),
            presence: Biquad::peaking(fs, 3500.0, 0.8, 3.0),
            env_db: -120.0,
            att: 1.0 - (-1.0 / (0.002 * fs)).exp(),
            rel: 1.0 - (-1.0 / (0.12 * fs)).exp(),
            auto_makeup: 10f64.powf(-full_range_db * 0.6 / 20.0),
        }
    }

    pub fn reset(&mut self) {
        self.shelf.reset();
        self.presence.reset();
        self.env_db = -120.0;
    }

    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.presence.run(self.shelf.run(x));
        let lvl = 20.0 * (y.abs() + 1e-9).log10();
        let k = if lvl > self.env_db { self.att } else { self.rel };
        self.env_db += k * (lvl - self.env_db);
        let gain_db = curve_db(self.env_db) - self.env_db;
        y * 10f64.powf(gain_db / 20.0) * self.auto_makeup * MAKEUP
    }
}

/// -1 dBFS peak limiter: instant attack, 120 ms release; transparent below the ceiling.
pub struct Limiter {
    gain: f64,
    rel: f64,
}

impl Limiter {
    const CEILING: f64 = 0.891;

    pub fn new(fs: f64) -> Limiter {
        Limiter { gain: 1.0, rel: 1.0 - (-1.0 / (0.12 * fs)).exp() }
    }

    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let p = x.abs();
        let target = if p > Self::CEILING { Self::CEILING / p } else { 1.0 };
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain += (target - self.gain) * self.rel;
        }
        x * self.gain
    }
}
