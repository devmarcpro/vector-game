//! Long-run stability and speed benchmark: `sim_bench [drift] [years...]` (default 1 10 100).
//! By default the ship idles docked at the home station; `drift` leaves it free-floating instead
//! (adaptive sub-stepping, much slower and a good stress test of the integrator).
use std::sync::Arc;
use std::time::Instant;
use vector_data::{find_data_dir, GameData};
use vector_sim::crew::DAY;
use vector_sim::scene::new_sim;
use vector_sim::ShipInput;

fn check(s: &vector_sim::Sim) -> Result<(), String> {
    if !s.credits.is_finite() || !s.ship.pos.length().is_finite() || !s.ship.fuel.is_finite() {
        return Err("non-finite player state".into());
    }
    for (st, m) in &s.markets {
        for r in 0..m.stock.len() {
            let p = m.price(&s.data, r);
            if !(m.stock[r].is_finite() && m.stock[r] >= -1e-6 && p.is_finite() && p > 0.0) {
                return Err(format!("bad market state at station {st}, resource {r}"));
            }
        }
    }
    Ok(())
}

fn main() {
    let dir = find_data_dir().expect("data/ not found");
    let data = Arc::new(GameData::load(&dir).expect("data"));
    let drift = std::env::args().any(|a| a == "drift");
    let args: Vec<u32> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let years = if args.is_empty() {
        vec![1, 10, 100]
    } else {
        args
    };
    let mut failed = false;
    for y in years {
        let mut s = new_sim(0x5EC7, Arc::clone(&data));
        s.researched.fill(true);
        let station = s
            .system
            .bodies
            .iter()
            .position(|b| b.kind == vector_world::BodyKind::Station)
            .expect("home system has a station");
        s.spawn_near(station, s.bodies[station].radius + 200.0);
        if !drift {
            s.try_dock().expect("docks at the home station");
        }
        let t = Instant::now();
        let days = u64::from(y) * 365;
        let mut worst: Result<(), String> = Ok(());
        for d in 0..days {
            s.advance(DAY, &ShipInput::default());
            if d % 30 == 0 && worst.is_ok() {
                worst = check(&s);
            }
            if (d + 1) % 3650 == 0 {
                println!(
                    "  .. year {:>3}: {:7.1} s elapsed, log {} done_contracts {} deltas {}",
                    (d + 1) / 365,
                    t.elapsed().as_secs_f64(),
                    s.crew.log.len(),
                    s.done_contracts.len(),
                    s.deltas.len()
                );
            }
        }
        let secs = t.elapsed().as_secs_f64();
        let verdict = worst.and_then(|()| check(&s));
        println!(
            "{y:>3} year(s): {secs:7.2} s wall ({:.0}x real time)  crew {} npcs {} colonies {} -> {}",
            f64::from(y) * 365.0 * 86400.0 / secs.max(1e-9),
            s.crew.members.len(),
            s.npcs.len(),
            s.colonies.len(),
            match &verdict {
                Ok(()) => "stable".to_string(),
                Err(e) => format!("FAILED: {e}"),
            }
        );
        failed |= verdict.is_err();
    }
    std::process::exit(i32::from(failed));
}
