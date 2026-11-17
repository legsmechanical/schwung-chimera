//! Move port: real-time cost per voice at 44.1 kHz in 128-frame blocks, full vs light.
use chimera::anatomy::human::{BODY_FEMALE, BODY_MALE};
use chimera::creatures::CREATURES;
use chimera::creatures::voice::CreatureVoice;
use chimera::phonetics::ipa::tokenize_ipa;
use chimera::phonetics::planner::{PlanOptions, plan_utterance};
use chimera::voice::human_voice::{HumanVoice, SourcePatch};
use std::time::Instant;

const FS: f64 = 44100.0;

fn bad(b: &[f32]) -> bool {
    b.iter().any(|x| !x.is_finite())
}

/// (% of one core on average, worst block as % of its 2.9 ms deadline)
fn time_blocks(secs: f64, mut f: impl FnMut(&mut [f32])) -> (f64, f64) {
    let blocks = (secs * FS / 128.0) as usize;
    let mut buf = [0f32; 128];
    let mut worst = 0f64;
    let t = Instant::now();
    for _ in 0..blocks {
        let b = Instant::now();
        f(&mut buf);
        worst = worst.max(b.elapsed().as_secs_f64());
    }
    (100.0 * t.elapsed().as_secs_f64() / secs, 100.0 * worst / (128.0 / FS))
}

fn main() {
    let secs = 6.0;
    let text = "ðə ˈnɔɹθ ˈwɪnd ænd ðə ˈsʌn wɚ dɪˈspjutɪŋ ˈwɪtʃ wəz ðə ˈstɹɔŋɡɚ";
    for (body, name, f0) in [(BODY_MALE, "male", 110.0), (BODY_FEMALE, "female", 200.0)] {
        for (os, geo, div) in [(2usize, 0usize, 1usize), (1, 4, 2)] {
            let mut v = HumanVoice::new(FS, body, os);
            v.set_light(geo, div);
            v.set_source(&SourcePatch::f0(f0));
            let plan = plan_utterance(&tokenize_ipa(text), &PlanOptions { body: name.into(), t0: 0.05, f0: Some(f0), ..Default::default() });
            v.schedule(&plan.gestures);
            let (avg, worst) = time_blocks(secs, |b| v.render(b));
            println!("speech {name:6} os={os} geo={geo} div={div}: avg {avg:5.1}% of a core, worst block {worst:5.1}%");
        }
    }
    let only = std::env::args().nth(1);
    for def in CREATURES.iter() {
        if only.as_deref().is_some_and(|o| o != def.id) {
            continue;
        }
        let full = def.oversample as usize;
        let light = (full / 2).max(1);
        let cfgs = [(full, 0usize, 1.0), (light, 4, 1.0), (light, 4, 1.5), (light, 4, 2.0)];
        for (m, geo, scale) in cfgs {
            if geo > 0 {
                chimera::creatures::build::set_section_scale(scale);
            }
            let mut v = CreatureVoice::with_rate(def, FS, &[], m);
            chimera::creatures::build::reset_section_scale();
            v.set_geo_every(geo);
            v.set_gate(1.0);
            let mut finite = true;
            let (avg, worst) = time_blocks(secs, |b| {
                v.render(b);
                if bad(b) {
                    finite = false;
                }
            });
            let mut out = [0f32; 128];
            for c in 0..def.calls.len() {
                v.play_call(c, None, 1.0);
                for _ in 0..(1.5 * FS / 128.0) as usize {
                    v.render(&mut out);
                    if bad(&out) {
                        finite = false;
                    }
                }
            }
            println!("creature {:9} os={m} geo={geo} sect x{scale}: {:3} nodes, avg {avg:5.1}% of a core, worst block {worst:5.1}%, finite {finite}", def.id, v.airway_nodes());
        }
    }
}
