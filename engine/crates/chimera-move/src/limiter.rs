//! The output stage every mode goes through: a -1 dBFS peak limiter (instant attack, smooth
//! release), because the Move's output is int16 and a loud voice can pass full scale.

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
