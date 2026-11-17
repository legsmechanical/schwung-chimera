//! Shared helpers for building creature airways.

use crate::airway::network::{AirwayNetwork, Area, PortSpec, Tube, TubeSpec};
use crate::math::{js_max, js_min};
use crate::physics::{C_SOUND, RHO, WallParams};

/// Smallest stable section length at this internal rate (CFL 0.93).
pub fn dx_min(fs: f64) -> f64 {
    C_SOUND / fs / 0.93
}

std::thread_local! {
    /// Move port: sections this many times longer than stability needs (fewer of them: less work,
    /// a lower spatial resolution). Read while a creature is built; 1 is the original.
    static SECTION_SCALE: core::cell::Cell<f64> = const { core::cell::Cell::new(1.0) };
    /// Move port: a tube's minimum section count yields to the time step (see `sections`).
    static FIT_MIN: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Move port: build creatures on this thread with sections `scale` times longer (≥ 1), and
/// with minimum section counts that yield to stability (`fit_min`). Both are for a creature
/// simulated below its native rate: a tube's floor of 3 sections made a songbird's 1 cm beak
/// sections shorter than one step of sound at 2x, and its airway went NaN (silent for good)
/// after the first call. Upstream rates never reach that, so the original keeps its floors.
pub fn set_section_scale(scale: f64) {
    SECTION_SCALE.with(|s| s.set(js_max(1.0, scale)));
    FIT_MIN.with(|f| f.set(scale > 0.0));
}

/// Move port: back to the original build (scale 1, floors kept).
pub fn reset_section_scale() {
    SECTION_SCALE.with(|s| s.set(1.0));
    FIT_MIN.with(|f| f.set(false));
}

/// Number of sections for a tube: as fine as stability allows, but at least `min`, at most `max`.
pub fn sections(fs: f64, length: f64, min: f64, max: f64, coarse: f64) -> usize {
    let scale = SECTION_SCALE.with(|s| s.get());
    let min = if FIT_MIN.with(|f| f.get()) { js_max(1.0, js_min(min, (length / dx_min(fs)).floor())) } else { min };
    js_max(min, js_min(max, (length / (dx_min(fs) * coarse * scale)).floor())) as usize
}

/// A tube's initial area: uniform, or a function of position u ∈ (0,1).
pub enum TubeArea<'a> {
    Uniform(f64),
    Profile(&'a dyn Fn(f64) -> f64),
}

/// The optional settings of [`tube`] (TS's `opts` object; `None` takes the TS default).
#[derive(Clone, Copy, Debug, Default)]
pub struct TubeOpts {
    pub wall: Option<WallParams>,
    pub coarse: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub perimeter_factor: Option<f64>,
    pub viscous_factor: Option<f64>,
    pub wall_radiation: Option<f64>,
}

pub fn tube(net: &mut AirwayNetwork, name: &str, fs: f64, length: f64, area: TubeArea, opts: TubeOpts) -> Tube {
    let n = sections(fs, length, opts.min.unwrap_or(3.0), opts.max.unwrap_or(120.0), opts.coarse.unwrap_or(1.0));
    let areas = match area {
        TubeArea::Uniform(a) => Area::Uniform(a),
        TubeArea::Profile(f) => Area::Profile((0..n).map(|i| f((i as f64 + 0.5) / n as f64)).collect()),
    };
    net.add_tube(
        TubeSpec::new(name, n, length, areas)
            .wall(opts.wall)
            .perimeter(opts.perimeter_factor.unwrap_or(1.0))
            .viscous(opts.viscous_factor.unwrap_or(1.0))
            .radiating(opts.wall_radiation.unwrap_or(0.0)),
    )
}

/// Lungs/air-sac pressure node feeding a tube through a mildly resistive connection.
pub fn lungs(net: &mut AirwayNetwork, into: usize, area: f64) -> usize {
    let node = net.add_pressure_node("lungs");
    net.add_port(node, into, PortSpec::new(area, 1.0).kinetic(0.0, 0.0).resistance((0.5 * RHO * C_SOUND) / area));
    node
}

/// Fill a tube's areas from a function of position u ∈ (0,1).
pub fn fill_areas(t: Tube, f: impl Fn(f64) -> f64, out: &mut [f64]) -> &[f64] {
    for i in 0..t.n {
        out[i] = js_max(0.0, f((i as f64 + 0.5) / t.n as f64));
    }
    out
}

pub const CMH2O: f64 = 980.665;
