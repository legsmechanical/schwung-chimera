//! Light creatures: stable airways (Courant number under the limit) and calls that keep sounding
//! after the first (a songbird at 2x went NaN after one call before minimum sections yielded).
use chimera::creatures::CREATURES;
use chimera::engine::Engine;
fn rms(b: &[f32]) -> f64 { (b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / b.len() as f64).sqrt() }
#[test]
fn light_creatures_are_stable_and_keep_calling() {
    for d in CREATURES.iter() {
        let mut e = Engine::with_light(44100.0, true);
        e.set_creature(Some(d), &[]);
        let cfl = e.creature().unwrap().courant();
        let mut b = vec![0f32; 44100];
        let mut rs = vec![];
        for call in 0..d.calls.len() {
            for _ in 0..2 {
                e.play_call(call, None, None);
                for c in b.chunks_mut(128) { e.render(c); }
                assert!(b.iter().all(|x| x.is_finite()));
                rs.push(rms(&b));
                for c in b.chunks_mut(128) { e.render(c); }
            }
        }
        println!("{:9} {} nodes, Courant {cfl:.3}, call rms {:?}", d.id, e.creature().unwrap().airway_nodes(), rs.iter().map(|r| format!("{r:.3}")).collect::<Vec<_>>());
        assert!(cfl <= 0.971, "{}: Courant {cfl}", d.id);
        for (i, r) in rs.iter().enumerate() {
            assert!(*r > 0.002, "{}: call {} pass {} silent", d.id, i / 2, i % 2);
        }
    }
}
