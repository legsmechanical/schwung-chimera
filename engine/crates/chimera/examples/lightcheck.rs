//! Move port: stability of the light voice at a 44.1 kHz airway, and A/B renders to listen to.
//! Usage: lightcheck <outdir>
use chimera::anatomy::human::{BODY_CHILD, BODY_FEMALE, BODY_MALE, HumanBody, NEUTRAL};
use chimera::phonetics::ipa::tokenize_ipa;
use chimera::phonetics::planner::{PlanOptions, plan_utterance};
use chimera::voice::human_airway::HumanAirway;
use chimera::voice::human_voice::{HumanVoice, SourcePatch};
use std::io::Write;

const FS: f64 = 44100.0;
const TEXT: &str = "ðə ˈnɔɹθ ˈwɪnd ænd ðə ˈsʌn wɚ dɪˈspjutɪŋ ˈwɪtʃ wəz ðə ˈstɹɔŋɡɚ / ʃi ˈsɛlz ˈsi ˈʃɛlz baɪ ðə ˈsi ˈʃɔɹ";

pub fn wav(path: &str, x: &[f32]) {
    let n = x.len() as u32;
    let mut h = Vec::new();
    h.extend(b"RIFF");
    h.extend((36 + n * 2).to_le_bytes());
    h.extend(b"WAVEfmt ");
    h.extend(16u32.to_le_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend((FS as u32).to_le_bytes());
    h.extend((FS as u32 * 2).to_le_bytes());
    h.extend(2u16.to_le_bytes());
    h.extend(16u16.to_le_bytes());
    h.extend(b"data");
    h.extend((n * 2).to_le_bytes());
    for &s in x {
        h.extend(((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::File::create(path).unwrap().write_all(&h).unwrap();
}

fn speech(body: HumanBody, name: &str, f0: f64, light: bool) -> Vec<f32> {
    let mut v = HumanVoice::new(FS, body, if light { 1 } else { 2 });
    if light {
        v.set_light(4, 2);
    }
    v.set_source(&SourcePatch::f0(f0));
    let mut t0 = 0.05;
    for u in TEXT.split('/') {
        let plan = plan_utterance(&tokenize_ipa(u.trim()), &PlanOptions { body: name.into(), t0, f0: Some(f0), ..Default::default() });
        v.schedule(&plan.gestures);
        t0 = plan.end + 0.35;
    }
    let mut out = vec![0f32; (FS * (t0 + 0.2)) as usize];
    for b in out.chunks_mut(128) {
        v.render(b);
    }
    out
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or("/out".into());
    let rms = |x: &[f32]| (x.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    for (body, name, f0) in [(BODY_MALE, "male", 110.0), (BODY_FEMALE, "female", 200.0), (BODY_CHILD, "child", 250.0)] {
        // the worst Courant number over the poses the engine's own guard test sweeps
        let mut air = HumanAirway::new(FS, body);
        let mut worst: f64 = 0.0;
        for tb_y in [0.0, 2.0, 5.67, 8.0] {
            for lp in [-0.2, 1.5] {
                for lx in [-1.6, 1.6] {
                    let mut a = NEUTRAL;
                    a.tb_y = tb_y;
                    a.lip_prot = lp;
                    a.larynx = lx;
                    air.apply(&a, 0.0);
                    worst = worst.max(air.net.check_cfl());
                }
            }
        }
        let full = speech(body, name, f0, false);
        let light = speech(body, name, f0, true);
        println!("{name:6}: Courant {worst:.3} (limit 0.971), finite {}, RMS light/full {:.2}", light.iter().all(|s| s.is_finite()), rms(&light) / rms(&full));
        wav(&format!("{dir}/speech_{name}_full.wav"), &full);
        wav(&format!("{dir}/speech_{name}_light.wav"), &light);
    }
}
