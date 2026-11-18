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
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public/data/cmudict.txt.gz")).ok()
}

#[test]
fn every_mode_sounds_and_stops() {
    let d = dict();
    for light in [true, false] {
        let mut c = Chimera::new(FS, light, d.as_deref());
        // Sing: three syllables, legato on the second
        c.set(P_MODE, MODE_SING as f64);
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
        c.set(P_MODE, MODE_VOWEL as f64);
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
        c.set(P_MODE, MODE_BEATBOX as f64);
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
        for m in MODE_FIRST_CREATURE..MODE_FIRST_CREATURE + 4 {
            c.set(P_MODE, m as f64);
            c.note_on(60, 100);
            let x = run(&mut c, 0.8);
            c.note_off(60);
            run(&mut c, 1.0);
            let q = run(&mut c, 0.5);
            assert!(check(&format!("creature {m} keys"), &x) > 0.003);
            assert!(check(&format!("creature {m} released"), &q) < 0.01);
            let (keys, _) = CREATURE_KEYS[m - MODE_FIRST_CREATURE];
            c.set(keys, 1.0);
            c.note_on(60, 100);
            let call = run(&mut c, 1.2);
            c.note_off(60);
            assert!(check(&format!("creature {m} call"), &call) > 0.003);
            c.set(keys, 0.0);
            run(&mut c, 1.0);
        }

        // a body change settles, then sings
        c.set(P_MODE, MODE_SING as f64);
        c.set(P_FEMALE, 1.0);
        run(&mut c, 0.3);
        c.note_on(67, 100);
        let f = run(&mut c, 0.5);
        c.note_off(67);
        assert!(check("female after settle", &f) > 0.01);
    }
}

#[test]
fn a_picked_vowel_publishes_the_mouth_and_a_set_one_does_not() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    let (mut ids, mut vals) = ([0u32; 32], [0f64; 32]);
    // a restored state or a preset sets the vowel quietly: the saved mouth knobs stay
    c.set(P_VOWEL, 9.0);
    assert_eq!(c.take_published(&mut ids, &mut vals), 0);
    c.pick_vowel(9); // Oo: rounded, protruded
    let n = c.take_published(&mut ids, &mut vals);
    let got: std::collections::HashMap<u32, f64> = ids[..n].iter().copied().zip(vals[..n].iter().copied()).collect();
    assert!(got.get(&(P_ROUND as u32)).is_some_and(|&v| v > 0.8), "{got:?}");
    assert!(got.get(&(P_PROTRUDE as u32)).is_some_and(|&v| v > 0.8), "{got:?}");
    // a pattern: set quietly keeps the swing, picked takes the pattern's own
    c.set(P_SWING, 0.47);
    c.set(P_PATTERN, 3.0);
    assert_eq!(c.take_published(&mut ids, &mut vals), 0);
    c.pick_pattern(3);
    let n = c.take_published(&mut ids, &mut vals);
    assert!(n == 1 && ids[0] == P_SWING as u32, "{:?}", &ids[..n]);
}

#[test]
fn rewriting_a_value_does_nothing() {
    // a scene or an LFO writes every block: the close mic must not reset each time
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_BEATBOX as f64);
    c.note_on(36, 100);
    let mut x = Vec::new();
    for _ in 0..(0.4 * FS / 128.0) as usize {
        c.set(P_MIC, 1.0);
        c.set(P_PATTERN, 1.0);
        let mut b = [0f32; 128];
        c.render(&mut b);
        x.extend_from_slice(&b);
    }
    assert!(check("kick under rewrites", &x) > 0.003);
}

/// Steps the pattern plays while the host transport runs `beats` at `bpm` (the tempo reported
/// with `wobble`, as Move's measured tempo does), starting at `from`.
fn steps_played(c: &mut Chimera, from: f64, beats: f64, bpm: f64, wobble: f64) -> usize {
    let mut beat = from;
    let (mut last, mut n) = (c.beat_step(), 0);
    let blocks = (beats * 60.0 / bpm * FS / 128.0) as usize;
    for i in 0..blocks {
        let jitter = if (beat as i64) % 2 == 0 { wobble } else { -wobble };
        c.transport(beat, bpm + jitter * (i % 3) as f64);
        let mut b = [0f32; 128];
        c.render(&mut b);
        beat += 128.0 / FS * bpm / 60.0;
        let s = c.beat_step();
        if s != last && s >= 0 {
            n += 1;
        }
        last = s;
    }
    n
}

#[test]
fn the_pattern_stays_locked_to_the_transport() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_BEATBOX as f64);
    c.pick_pattern(1);
    // warm up, then 8 beats with the tempo wobbling in its last digits: 32 steps, none skipped
    steps_played(&mut c, 0.0, 1.0, 100.0, 0.03);
    let n = steps_played(&mut c, 1.0, 8.0, 100.0, 0.03);
    println!("8 beats at a wobbling 100 bpm: {n} steps");
    assert!((31..=33).contains(&n), "{n} steps");
    // the transport jumps back (a loop, a restart): the pattern carries on at once
    let n = steps_played(&mut c, 0.0, 2.0, 100.0, 0.0);
    println!("2 beats after a jump back: {n} steps");
    assert!(n >= 6, "{n} steps after the jump");
    // 300 bpm is followed at half time: 4 host beats = 2 pattern beats = 8 steps
    c.transport(-1.0, 300.0);
    run(&mut c, 0.3);
    steps_played(&mut c, 0.0, 2.0, 300.0, 0.0);
    let n = steps_played(&mut c, 2.0, 8.0, 300.0, 0.0);
    println!("8 beats at 300 bpm: {n} steps");
    assert!((15..=17).contains(&n), "{n} steps");
}

#[test]
fn switching_keys_to_a_call_stops_the_breath() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_SONGBIRD as f64);
    c.note_on(60, 100);
    assert!(check("songbird held", &run(&mut c, 0.5)) > 0.003);
    // still holding the key: Keys -> a call, so the held key no longer breathes
    c.set(CREATURE_KEYS[0].0, 1.0);
    run(&mut c, 3.0);
    assert!(check("songbird after keys -> call", &run(&mut c, 0.5)) < 0.01);
}

#[test]
fn the_close_mic_brings_beatbox_up_to_the_voice() {
    let mut c = Chimera::new(FS, true, None);
    let level = |c: &mut Chimera| {
        c.set(P_MODE, MODE_BEATBOX as f64);
        let mut x = Vec::new();
        for n in [36u8, 38, 40, 36, 38, 40] {
            c.note_on(n, 100);
            x.extend(run(c, 0.25));
        }
        x
    };
    c.set(P_MIC, 0.0);
    let dry = level(&mut c);
    c.set(P_MODE, MODE_SING as f64);
    c.set(P_MIC, 1.0);
    let wet = level(&mut c);
    let (d, w) = (check("beatbox dry", &dry), check("beatbox close mic", &wet));
    let peak = wet.iter().fold(0f32, |m, s| m.max(s.abs()));
    println!("close mic peak {peak:.3}");
    assert!(w > 4.0 * d, "close mic {w} vs dry {d}");
    assert!(peak <= 0.892, "limiter: peak {peak}");
}

/// median voiced pitch (Hz) over 2048-sample frames
fn median_f0(x: &[f32]) -> f64 {
    use chimera::analysis::pitch::{YinOpts, yin_pitch};
    let mut f: Vec<f64> = x
        .chunks(2048)
        .filter(|c| c.len() == 2048)
        .map(|c| yin_pitch(&c.iter().map(|&s| s as f64).collect::<Vec<_>>(), FS, YinOpts { min_f0: Some(40.0), max_f0: Some(800.0), threshold: Some(0.2) }).f0)
        .filter(|&f| f > 0.0)
        .collect();
    assert!(!f.is_empty(), "no voiced frame");
    f.sort_by(f64::total_cmp);
    f[f.len() / 2]
}

/// render until the line has been spoken (at most `max` s); returns the audio
fn speak_out(c: &mut Chimera, max: f64) -> Vec<f32> {
    let mut out = Vec::new();
    while c.speaking() && (out.len() as f64) < max * FS {
        out.extend(run(c, 0.1));
    }
    out
}

fn speaker(text: &str) -> Chimera {
    let d = dict();
    let mut c = Chimera::new(FS, true, d.as_deref());
    c.set_lyrics(text);
    c.set(P_MODE, MODE_SPEAK as f64);
    // the line is planned on its own thread, and picked up while rendering
    for _ in 0..400 {
        if !c.speak_planning() {
            break;
        }
        let mut b = [0f32; 128];
        c.render(&mut b);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!c.speak_planning(), "the line was never planned");
    c
}

#[test]
fn a_key_speaks_the_whole_line_and_letting_go_does_not_stop_it() {
    let mut c = speaker("Hello there. How are you today?");
    c.note_on(48, 100);
    let a = run(&mut c, 0.3);
    c.note_off(48);
    assert!(c.speaking());
    let rest = speak_out(&mut c, 10.0);
    let whole = a.len() + rest.len();
    println!("spoken for {:.2} s", whole as f64 / FS);
    assert!(check("speak start", &a) > 0.003);
    assert!(check("speak rest", &rest) > 0.003);
    assert!(whole as f64 / FS > 1.2, "the whole line, not a word");
    run(&mut c, 0.4);
    assert!(check("speak after the line", &run(&mut c, 0.4)) < 0.003);
}

#[test]
fn a_key_sets_the_speaking_pitch() {
    let mut f = Vec::new();
    for note in [45u8, 57] {
        let mut c = speaker("Say hello to my friend.");
        c.note_on(note, 100);
        let x = speak_out(&mut c, 6.0);
        let m = median_f0(&x);
        println!("key {note}: median f0 {m:.1} Hz");
        f.push(m);
    }
    let ratio = f[1] / f[0];
    assert!((ratio - 2.0).abs() < 0.2, "an octave up: ratio {ratio}");
}

#[test]
fn a_new_key_starts_the_line_again_and_all_notes_off_cuts_it() {
    let mut c = speaker("One two three four five six seven eight nine ten.");
    c.note_on(48, 100);
    run(&mut c, 1.0);
    c.note_on(50, 100);
    // restarted: a full line from here, longer than what was left of the first
    let again = speak_out(&mut c, 10.0);
    let mut d = speaker("One two three four five six seven eight nine ten.");
    d.note_on(50, 100);
    let full = speak_out(&mut d, 10.0);
    println!("again {:.2} s, full {:.2} s", again.len() as f64 / FS, full.len() as f64 / FS);
    assert!((again.len() as f64 - full.len() as f64).abs() < 0.15 * FS);
    c.note_on(48, 100);
    run(&mut c, 0.5);
    c.all_notes_off();
    assert!(!c.speaking());
    run(&mut c, 0.3);
    assert!(check("speak after all notes off", &run(&mut c, 0.4)) < 0.003);
}

#[test]
fn rate_replans_the_line() {
    let mut c = speaker("The quick brown fox jumps over the lazy dog.");
    c.note_on(48, 100);
    let slow = speak_out(&mut c, 10.0).len();
    c.set(P_RATE, 2.0);
    assert!(c.speak_planning());
    // a key pressed while the line is planned waits for it
    c.note_on(48, 100);
    let fast = speak_out(&mut c, 10.0).len();
    assert!(!c.speak_planning());
    println!("rate 1: {:.2} s, rate 2: {:.2} s", slow as f64 / FS, fast as f64 / FS);
    assert!((fast as f64) < 0.7 * slow as f64);
}

#[test]
fn new_lyrics_are_spoken_once_planned() {
    let mut c = speaker("Hi.");
    c.note_on(48, 100);
    let short = speak_out(&mut c, 10.0).len();
    c.set_lyrics("This line is a great deal longer than the one before it, by a mile.");
    c.note_on(48, 100);
    let long = speak_out(&mut c, 15.0).len();
    println!("short {:.2} s, long {:.2} s", short as f64 / FS, long as f64 / FS);
    assert!(long > 3 * short);
    // and Sing still sings after Speak
    c.set(P_MODE, MODE_SING as f64);
    c.note_on(60, 100);
    assert!(check("sing after speak", &run(&mut c, 0.4)) > 0.01);
}

#[test]
fn a_line_being_spoken_is_not_mixed_with_a_new_plan() {
    // Rate turned while a line is spoken: that line plays out as planned (same length as one
    // spoken with nothing turned), and the next key speaks the new plan
    let text = "Seven silly swans swam south in the summer sun.";
    let mut a = speaker(text);
    a.note_on(48, 100);
    let plain = speak_out(&mut a, 10.0).len();
    let mut c = speaker(text);
    c.note_on(48, 100);
    let mut x = run(&mut c, 0.3);
    c.set(P_RATE, 2.0);
    x.extend(speak_out(&mut c, 10.0));
    println!("plain {:.2} s, turned mid-line {:.2} s", plain as f64 / FS, x.len() as f64 / FS);
    assert!((x.len() as f64 - plain as f64).abs() < 0.15 * FS);
    assert!(x.iter().all(|s| s.is_finite()));
}

/// `CHIMERA_DEMO=1 ./scripts/test.sh` writes build/speak-demo.wav: a few lines through the whole
/// performer (light engine, the Move's output chain).
#[test]
fn speak_demo() {
    if std::env::var("CHIMERA_DEMO").is_err() {
        return;
    }
    let mut out: Vec<f32> = Vec::new();
    let mut say = |text: &str, note: u8, female: f64, rate: f64, expr: f64| {
        let mut c = speaker(text);
        c.set(P_FEMALE, female);
        c.set(P_RATE, rate);
        c.set(P_EXPRESSION, expr);
        run(&mut c, 0.3);
        while c.speak_planning() {
            run(&mut c, 0.01);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        c.note_on(note, 100);
        out.extend(speak_out(&mut c, 20.0));
        out.extend(run(&mut c, 0.5));
    };
    say("Breathe in, breathe out, I am made of air. Tongue and lips and throat, singing anywhere.", 45, 0.0, 1.0, 0.35);
    say("Hello there. How are you today?", 57, 1.0, 1.0, 0.35);
    say("Hello there. How are you today?", 52, 1.0, 1.0, 0.35);
    say("Is this thing on? Can you hear me?", 45, 0.0, 1.0, 0.9);
    say("Daisy, Daisy, give me your answer, do. I'm half crazy, all for the love of you.", 47, 0.0, 1.8, 0.6);
    say("Daisy, Daisy, give me your answer, do.", 45, 0.0, 0.6, 0.2);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../build/speak-demo.wav");
    let mut w = Vec::new();
    let bytes = (out.len() * 2) as u32;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + bytes).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&44100u32.to_le_bytes());
    w.extend_from_slice(&88200u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&bytes.to_le_bytes());
    for s in &out {
        w.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, w).unwrap();
    println!("wrote {path}: {:.1} s", out.len() as f64 / FS);
}
