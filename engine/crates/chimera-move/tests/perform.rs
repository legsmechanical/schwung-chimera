//! Every mode, driven the way the plugin drives it: notes in, audio out. Checks the output is
//! finite, sounds while notes are played and falls silent after.
use chimera_move::*;

const FS: f64 = 44100.0;

fn run(c: &mut Chimera, secs: f64) -> Vec<f32> {
    let mut out = vec![0f32; (secs * FS) as usize / 128 * 128];
    for b in out.chunks_mut(128) {
        c.render(b);
    }
    out
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn check(name: &str, x: &[f32]) -> f64 {
    assert!(x.iter().all(|s| s.is_finite()), "{name}: non-finite output");
    let r = rms(x);
    println!("{name:28} rms {r:.4}");
    r
}

fn dict() -> Option<Vec<u8>> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/cmudict.txt.gz")).ok()
}

#[test]
fn every_mode_sounds_and_stops() {
    let d = dict();
    for light in [true, false] {
        let mut c = Chimera::new(FS, light, d.as_deref());
        // Sing: three syllables, legato on the second
        c.set(P_MODE, 0.0);
        c.note_on(60, 100);
        let a = run(&mut c, 0.4);
        c.note_on(62, 100);
        c.note_off(60);
        let b = run(&mut c, 0.4);
        c.note_off(62);
        c.note_on(64, 90);
        let e = run(&mut c, 0.5);
        c.note_off(64);
        run(&mut c, 0.8);
        let quiet = run(&mut c, 0.5);
        assert!(check("sing note 1", &a) > 0.01 && check("sing legato", &b) > 0.01 && check("sing note 3", &e) > 0.01);
        assert!(check("sing after release", &quiet) < 0.003);

        // Vowel: a held key, a vowel change, then hold
        c.set(P_MODE, 1.0);
        c.set(P_VOWEL, 1.0);
        c.note_on(57, 100);
        let v = run(&mut c, 0.6);
        c.note_off(57);
        run(&mut c, 0.6);
        let vq = run(&mut c, 0.4);
        assert!(check("vowel held", &v) > 0.01);
        assert!(check("vowel released", &vq) < 0.003);
        c.set(P_HOLD, 1.0);
        let h = run(&mut c, 0.6);
        assert!(check("vowel hold drone", &h) > 0.01);
        c.set(P_HOLD, 0.0);
        run(&mut c, 0.8);

        // Beatbox: a hit per pad class, then the pattern on a running transport
        c.set(P_MODE, 2.0);
        let mut hits = Vec::new();
        for n in [36u8, 38, 40, 41, 42, 45] {
            c.note_on(n, 100);
            hits.extend(run(&mut c, 0.3));
        }
        assert!(check("beatbox hits", &hits) > 0.003);
        let mut pat = Vec::new();
        let mut beat = 0.0;
        for _ in 0..(2.0 * FS / 128.0) as usize {
            c.transport(beat, 96.0);
            let mut b = [0f32; 128];
            c.render(&mut b);
            pat.extend_from_slice(&b);
            beat += 128.0 / FS * 96.0 / 60.0;
        }
        assert!(check("beatbox pattern", &pat) > 0.003);
        c.transport(-1.0, 96.0);
        run(&mut c, 0.6);

        // the creatures: pitch on the keys, then a call
        for m in 3..=6 {
            c.set(P_MODE, m as f64);
            c.note_on(60, 100);
            let x = run(&mut c, 0.8);
            c.note_off(60);
            run(&mut c, 1.0);
            let q = run(&mut c, 0.5);
            assert!(check(&format!("creature {m} keys"), &x) > 0.003);
            assert!(check(&format!("creature {m} released"), &q) < 0.01);
            let (keys, _) = CREATURE_KEYS[m - 3];
            c.set(keys, 1.0);
            c.note_on(60, 100);
            let call = run(&mut c, 1.2);
            c.note_off(60);
            assert!(check(&format!("creature {m} call"), &call) > 0.003);
            c.set(keys, 0.0);
            run(&mut c, 1.0);
        }

        // a body change settles, then sings
        c.set(P_MODE, 0.0);
        c.set(P_FEMALE, 1.0);
        run(&mut c, 0.3);
        c.note_on(67, 100);
        let f = run(&mut c, 0.5);
        c.note_off(67);
        assert!(check("female after settle", &f) > 0.01);
    }
}

#[test]
fn a_vowel_publishes_the_mouth() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, 1.0);
    c.set(P_VOWEL, 9.0); // Oo: rounded, protruded
    let (mut ids, mut vals) = ([0u32; 32], [0f64; 32]);
    let n = c.take_published(&mut ids, &mut vals);
    let got: std::collections::HashMap<u32, f64> = ids[..n].iter().copied().zip(vals[..n].iter().copied()).collect();
    assert!(got.get(&(P_ROUND as u32)).is_some_and(|&v| v > 0.8), "{got:?}");
    assert!(got.get(&(P_PROTRUDE as u32)).is_some_and(|&v| v > 0.8), "{got:?}");
}

#[test]
fn the_close_mic_brings_beatbox_up_to_the_voice() {
    let mut c = Chimera::new(FS, true, None);
    let level = |c: &mut Chimera| {
        c.set(P_MODE, 2.0);
        let mut x = Vec::new();
        for n in [36u8, 38, 40, 36, 38, 40] {
            c.note_on(n, 100);
            x.extend(run(c, 0.25));
        }
        x
    };
    c.set(P_MIC, 0.0);
    let dry = level(&mut c);
    c.set(P_MODE, 0.0);
    c.set(P_MIC, 1.0);
    let wet = level(&mut c);
    let (d, w) = (check("beatbox dry", &dry), check("beatbox close mic", &wet));
    let peak = wet.iter().fold(0f32, |m, s| m.max(s.abs()));
    println!("close mic peak {peak:.3}");
    assert!(w > 4.0 * d, "close mic {w} vs dry {d}");
    assert!(peak <= 0.892, "limiter: peak {peak}");
}
