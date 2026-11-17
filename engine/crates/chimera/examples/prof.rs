//! Move port: where a voice's time goes (build with --features prof).
use chimera::airway::network::NPROF;
use chimera::anatomy::human::BODY_MALE;
use chimera::phonetics::ipa::tokenize_ipa;
use chimera::phonetics::planner::{PlanOptions, plan_utterance};
use chimera::voice::human_voice::{HumanVoice, PROF, SourcePatch};
use std::sync::atomic::Ordering;

fn main() {
    let fs = 44100.0;
    for (os, geo, div) in [(2usize, 0usize, 1usize), (1, 4, 2)] {
        PROF.iter().chain(NPROF.iter()).for_each(|p| p.store(0, Ordering::Relaxed));
        let mut v = HumanVoice::new(fs, BODY_MALE, os);
        v.set_light(geo, div);
        v.set_source(&SourcePatch::f0(110.0));
        let plan = plan_utterance(&tokenize_ipa("ðə ˈnɔɹθ ˈwɪnd ænd ðə ˈsʌn wɚ dɪˈspjutɪŋ ˈwɪtʃ wəz ðə ˈstɹɔŋɡɚ"), &PlanOptions { body: "male".into(), t0: 0.05, f0: Some(110.0), ..Default::default() });
        v.schedule(&plan.gestures);
        let secs = 6.0;
        let mut b = [0f32; 128];
        let t = std::time::Instant::now();
        for _ in 0..(secs * fs / 128.0) as usize {
            v.render(&mut b);
        }
        let total = t.elapsed().as_secs_f64();
        println!("os={os} geo={geo} div={div}: {:.1}% of a core (timers included)", 100.0 * total / secs);
        let pct = |ns: u64| 100.0 * ns as f64 * 1e-9 / total;
        for (i, n) in ["control", "glottis", "airway step", "decimator", "output", " ctl: score+artic", " ctl: shape", " ctl: contacts", " ctl: begin_block"].iter().enumerate() {
            println!("   {n:18} {:5.1}%", pct(PROF[i].load(Ordering::Relaxed)));
        }
        if geo > 0 {
            for (i, n) in ["geo update", "ramps", "vol src", "noise", "branches", "walls", "nodes"].iter().enumerate() {
                println!("     step: {n:12} {:5.1}%", pct(NPROF[i].load(Ordering::Relaxed)));
            }
        }
    }
}
