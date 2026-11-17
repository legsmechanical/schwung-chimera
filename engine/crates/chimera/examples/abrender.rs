//! Move port: A/B listening files — each one plays the full engine, a beep, then the light one.
//! Usage: abrender <outdir>
use chimera::anatomy::human::{BODY_CHILD, BODY_FEMALE, BODY_MALE, HumanBody};
use chimera::creatures::CREATURES;
use chimera::creatures::build::set_section_scale;
use chimera::creatures::types::CreatureDef;
use chimera::creatures::voice::CreatureVoice;
use chimera::phonetics::ipa::tokenize_ipa;
use chimera::phonetics::planner::{PlanOptions, plan_utterance};
use chimera::voice::human_voice::{HumanVoice, SourcePatch};
use std::io::Write;

const FS: f64 = 44100.0;
const TEXT: &str = "ðə ˈnɔɹθ ˈwɪnd ænd ðə ˈsʌn wɚ dɪˈspjutɪŋ ˈwɪtʃ wəz ðə ˈstɹɔŋɡɚ / ʃi ˈsɛlz ˈsi ˈʃɛlz baɪ ðə ˈsi ˈʃɔɹ / ˈɑɑɑɑɑɑ";

/// the light settings the module will use, per creature: (oversample, section scale)
pub fn creature_light(id: &str) -> (usize, f64) {
    match id {
        "songbird" => (2, 1.0),
        "seal" => (1, 1.5),
        "alligator" => (1, 1.5),
        "elephant" => (1, 2.0),
        _ => (1, 1.0),
    }
}

fn wav(path: &str, x: &[f32]) {
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

/// a short quiet beep between A and B
fn divider(out: &mut Vec<f32>) {
    out.extend(std::iter::repeat_n(0.0, (0.5 * FS) as usize));
    for i in 0..(0.12 * FS) as usize {
        out.push(0.15 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / FS).sin() as f32);
    }
    out.extend(std::iter::repeat_n(0.0, (0.5 * FS) as usize));
}

fn speech(body: HumanBody, name: &str, f0: f64, light: bool) -> Vec<f32> {
    let mut v = HumanVoice::new(FS, body, if light { 1 } else { 2 });
    if light {
        v.set_light(4, 2);
    }
    v.set_source(&SourcePatch { f0: Some(f0), vibrato_depth: Some(30.0), ..Default::default() });
    let mut t0 = 0.05;
    for u in TEXT.split('/') {
        let plan = plan_utterance(&tokenize_ipa(u.trim()), &PlanOptions { body: name.into(), t0, f0: Some(f0), ..Default::default() });
        v.schedule(&plan.gestures);
        t0 = plan.end + 0.35;
    }
    let mut out = vec![0f32; (FS * (t0 + 0.3)) as usize];
    for b in out.chunks_mut(128) {
        v.render(b);
    }
    out
}

fn creature(def: &'static CreatureDef, light: bool) -> Vec<f32> {
    let (m, scale) = if light { chimera::engine::light_creature(def.id) } else { (def.oversample as usize, 1.0) };
    if light {
        set_section_scale(scale);
    }
    let mut v = CreatureVoice::with_rate(def, FS, &[], m);
    chimera::creatures::build::reset_section_scale();
    if light {
        v.set_geo_every(4);
    }
    let mut out = Vec::new();
    let run = |v: &mut CreatureVoice, secs: f64, out: &mut Vec<f32>| {
        let mut b = [0f32; 128];
        for _ in 0..(secs * FS / 128.0) as usize {
            v.render(&mut b);
            out.extend_from_slice(&b);
        }
    };
    // a held note, then every call
    v.set_gate(1.0);
    run(&mut v, 2.0, &mut out);
    v.set_gate(0.0);
    run(&mut v, 0.5, &mut out);
    for c in 0..def.calls.len() {
        v.play_call(c, None, 1.0);
        run(&mut v, 2.2, &mut out);
    }
    out
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or("/out".into());
    for (body, name, f0) in [(BODY_MALE, "male", 110.0), (BODY_FEMALE, "female", 200.0), (BODY_CHILD, "child", 250.0)] {
        let mut out = speech(body, name, f0, false);
        divider(&mut out);
        out.extend(speech(body, name, f0, true));
        wav(&format!("{dir}/AB_voice_{name}.wav"), &out);
    }
    for def in CREATURES.iter() {
        let mut out = creature(def, false);
        divider(&mut out);
        out.extend(creature(def, true));
        wav(&format!("{dir}/AB_creature_{}.wav", def.id), &out);
    }
}
