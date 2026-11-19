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

fn published(c: &mut Chimera) -> std::collections::HashMap<u32, f64> {
    let (mut ids, mut vals) = ([0u32; 64], [0f64; 64]);
    let n = c.take_published(&mut ids, &mut vals);
    ids[..n].iter().copied().zip(vals[..n].iter().copied()).collect()
}

#[test]
fn a_picked_vowel_publishes_the_mouth_and_a_set_one_does_not() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    // a restored state or a preset sets the vowel quietly: the saved mouth knobs stay
    c.set(P_VOWEL, 9.0);
    c.set(P_SWEEP, 0.0);
    assert!(published(&mut c).is_empty());
    c.pick_vowel(9); // Oo: rounded, protruded
    let got = published(&mut c);
    assert!(got.get(&(P_ROUND as u32)).is_some_and(|&v| v > 0.8), "{got:?}");
    assert!(got.get(&(P_PROTRUDE as u32)).is_some_and(|&v| v > 0.8), "{got:?}");
}

#[test]
fn the_vowel_knob_runs_the_mouth_along_its_path() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    // the ends of the path: Oo (rounded, high back tongue) and Ee (spread, high front tongue)
    c.set(P_SWEEP, 0.0);
    c.pick_sweep();
    let oo = published(&mut c);
    c.set(P_SWEEP, 1.0);
    c.pick_sweep();
    let ee = published(&mut c);
    let k = |m: &std::collections::HashMap<u32, f64>, id: usize| m[&(id as u32)];
    println!("Oo round {:.2} front {:.2}; Ee round {:.2} front {:.2}", k(&oo, P_ROUND), k(&oo, P_TONGUE_X), k(&ee, P_ROUND), k(&ee, P_TONGUE_X));
    assert!(k(&oo, P_ROUND) > 0.8 && k(&ee, P_ROUND) < 0.2);
    assert!(k(&ee, P_TONGUE_X) > k(&oo, P_TONGUE_X) + 0.2);
    // halfway is Ah: the jaw open wider than at either end
    c.set(P_SWEEP, 0.5);
    c.pick_sweep();
    let ah = published(&mut c);
    assert!(k(&ah, P_JAW) > k(&oo, P_JAW) + 0.2 && k(&ah, P_JAW) > k(&ee, P_JAW) + 0.2, "{ah:?}");
}

#[test]
fn pressure_pushes_the_vowel_without_touching_the_knobs() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    c.set(P_SWEEP, 0.25);
    c.pick_sweep();
    published(&mut c);
    c.set(P_PRESS_AMT, 1.0);
    c.note_on(57, 100);
    run(&mut c, 0.3);
    let before = c.eng_artic_jaw();
    c.set_pressure(0.5);
    assert!((c.sweep_effective() - 0.75).abs() < 1e-9);
    run(&mut c, 0.5);
    let after = c.eng_artic_jaw();
    assert!(published(&mut c).is_empty(), "pressure must not move the knobs");
    // Oh (0.25) -> Eh (0.75) through Ah: the engine's jaw moved
    println!("jaw {before:.3} -> {after:.3}");
    assert!((after - before).abs() > 0.05);
    // the other way, and clamped at the path's end
    c.set(P_PRESS_AMT, -1.0);
    c.set_pressure(1.0);
    assert_eq!(c.sweep_effective(), 0.0);
    // out of Vowel mode, pressure does nothing
    c.set(P_MODE, MODE_SING as f64);
    c.set_pressure(1.0);
    assert_eq!(c.sweep_effective(), 0.25);
}

#[test]
fn velocity_sets_the_vowel_breath() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    let level = |c: &mut Chimera, v: u8| {
        c.note_on(57, v);
        run(c, 0.4);
        let x = run(c, 0.4);
        c.note_off(57);
        run(c, 0.8);
        rms(&x)
    };
    let (soft, hard) = (level(&mut c, 30), level(&mut c, 127));
    println!("vowel velocity 30: {soft:.4}, 127: {hard:.4}");
    assert!(hard > soft * 1.3);
}

#[test]
fn position_is_the_next_syllable_and_playing_moves_it() {
    let mut c = Chimera::new(FS, true, None);
    c.set_lyrics("one two three four five");
    c.set(P_MODE, MODE_SING as f64);
    assert_eq!(c.cursor(), 0);
    c.set(P_POSITION, 3.0);
    assert_eq!(c.cursor(), 3);
    c.note_on(60, 100);
    c.note_off(60);
    assert_eq!(c.cursor(), 4);
    // the same value written again still moves it back: playing moved it meanwhile
    c.set(P_POSITION, 3.0);
    assert_eq!(c.cursor(), 3);
    // held at the last syllable
    c.set(P_POSITION, 200.0);
    assert_eq!(c.cursor(), 4);
    // the Lyrics page's syllables
    c.set_lyrics("hello there");
    let v = c.view().to_string();
    println!("{v:?}");
    assert!(v.starts_with("S|"));
    let words: Vec<&str> = v[2..].split(' ').filter(|w| !w.is_empty()).collect();
    assert_eq!(words.len(), 2);
    assert_eq!(words[0].split(chimera_move::sing::SYL_SEP).count(), 2, "hel-lo: {v:?}");
}

#[test]
fn rewriting_a_value_does_nothing() {
    // a scene or an LFO writes every block: a held vowel must not restart each time
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    c.note_on(57, 100);
    let mut x = Vec::new();
    for _ in 0..(0.6 * FS / 128.0) as usize {
        c.set(P_MODE, MODE_VOWEL as f64);
        c.set(P_HOLD, 0.0);
        let mut b = [0f32; 128];
        c.render(&mut b);
        x.extend_from_slice(&b);
    }
    assert!(check("vowel under rewrites", &x[x.len() / 2..]) > 0.01);
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
fn the_line_page_knows_when_each_word_ends() {
    let mut c = speaker("Hello there, I am a voice made of air.");
    let v = c.view().to_string();
    println!("{v}");
    let f: Vec<&str> = v.splitn(3, '|').collect();
    assert_eq!(f[0], "P");
    assert_eq!(f[2], "Hello there, I am a voice made of air.");
    let ends: Vec<i64> = f[1].split(',').map(|x| x.parse().unwrap()).collect();
    assert_eq!(ends.len(), 9);
    assert!(ends.windows(2).all(|w| w[1] >= w[0]) && ends[0] > 50, "{ends:?}");
    // the comma's pause comes after "there"
    let gaps: Vec<i64> = ends.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps[1] > gaps[2], "{gaps:?}");
    assert!(c.speak_elapsed().is_none());
    let s0 = c.view_serial();
    c.note_on(60, 100);
    run(&mut c, 0.3);
    let e = c.speak_elapsed().expect("speaking");
    assert!((0.2..0.4).contains(&e), "{e}");
    assert_ne!(c.view_serial(), s0);
}

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

/// The Vowel knob's path as mouth-knob values (male body), for the Vowel page's picture
/// (src/params.json "sweep_knobs"): `./scripts/test.sh` prints it.
#[test]
fn print_the_path_for_the_picture() {
    let mut c = Chimera::new(FS, true, None);
    c.set(P_MODE, MODE_VOWEL as f64);
    let keys = [("tongue_x", P_TONGUE_X), ("tongue_y", P_TONGUE_Y), ("jaw", P_JAW), ("lips", P_LIPS), ("round", P_ROUND), ("protrude", P_PROTRUDE)];
    let mut rows = Vec::new();
    for (i, _) in SWEEP_PATH.iter().enumerate() {
        c.set(P_SWEEP, i as f64 / (SWEEP_PATH.len() - 1) as f64);
        c.pick_sweep();
        let got = published(&mut c);
        rows.push(format!("[{}]", keys.iter().map(|(_, id)| format!("{:.3}", got.get(&(*id as u32)).copied().unwrap_or(-1.0))).collect::<Vec<_>>().join(", ")));
    }
    println!("sweep_knobs ({}): [{}]", keys.iter().map(|k| k.0).collect::<Vec<_>>().join(", "), rows.join(", "));
}

#[test]
fn the_lyrics_view_names_the_syllable_sounding_not_the_next() {
    let d = dict();
    let mut c = Chimera::new(FS, true, d.as_deref());
    c.set(P_MODE, MODE_SING as f64);
    c.set_lyrics("one two three");
    let serial = |c: &Chimera| c.view_serial();
    // nothing sung yet: the page lights the cursor
    assert!(c.view().starts_with("S|0|one two"), "{}", c.view());
    let s0 = serial(&c);
    c.note_on(60, 100);
    assert_eq!(c.cursor(), 1);
    assert!(c.view().starts_with("S|0|"), "{}", c.view());
    // held after release, until the next key
    c.note_off(60);
    assert!(c.view().starts_with("S|0|"));
    c.note_on(62, 100);
    assert!(c.view().starts_with("S|1|"));
    c.note_off(62);
    // moving Position lights the cursor, and the page hears of it (also before any key)
    let s1 = serial(&c);
    c.set(P_POSITION, 0.0);
    assert!(c.view().starts_with("S|0|"));
    assert_ne!(serial(&c), s1);
    let s2 = serial(&c);
    c.set(P_POSITION, 1.0);
    assert!(c.view().starts_with("S|1|"));
    assert_ne!(serial(&c), s2, "a turn with nothing sung must still reach the page");
    // the cursor wraps; the sung index is the syllable just started
    c.set(P_POSITION, 2.0);
    let s3 = serial(&c);
    c.note_on(64, 100);
    assert!(c.view().starts_with("S|2|"));
    assert_ne!(serial(&c), s3, "the first key after a turn must reach the page");
    assert_eq!(c.cursor(), 0);
}
