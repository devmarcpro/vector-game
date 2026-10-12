//! `vector-emerg`: probes for the interactions between systems (see docs/MATRICE_INTERACTIONS.md).
//! Plays short scripted scenarios on the real sim and prints what comes out of them, to spot
//! out-of-scale values, unstable feedback loops and trivial exploits.
//!   emerg dv        delta-v and thrust of every start ship and of the best fits per hull
//!   emerg arb       round trips at one market (buy then sell the same goods) at several sizes
//!   emerg loop      an idle year: do prices, money and population stay in a sane range?
use std::sync::Arc;
use vector_data::{find_data_dir, GameData};
use vector_sim::crew::DAY;
use vector_sim::start::{new_sim_with, StartConfig};
use vector_sim::ShipInput;

fn main() {
    let dir = find_data_dir().expect("data/ not found");
    let data = Arc::new(GameData::load(&dir).expect("data"));
    let what = std::env::args().nth(1).unwrap_or_else(|| "dv".into());
    match what.as_str() {
        "dv" => dv(&data),
        "arb" => arb(&data),
        "loop" => idle_loop(&data),
        "sys" => sys(&data),
        "colony" => colony(&data),
        _ => eprintln!("usage: emerg dv|arb|loop"),
    }
}

fn dv(data: &Arc<GameData>) {
    for (si, sc) in data.scenarios.iter().enumerate() {
        for k in 0..sc.ships.len() {
            let mut cfg = StartConfig::new(7);
            cfg.scenario = si;
            cfg.ship = k;
            let Ok(s) = new_sim_with(&cfg, Arc::clone(data)) else {
                continue;
            };
            let st = &s.ship.stats;
            println!(
                "{:<22} ship {k} {:<18} isp {:>8.0} s  dv {:>9.0} m/s  thrust {:>9.0} N  mass {:>8.0} kg  twr {:.2}",
                sc.id,
                sc.ships[k].0,
                st.isp,
                s.ship.delta_v(),
                st.thrust,
                s.ship.mass(),
                st.thrust / (s.ship.mass() * 9.80665)
            );
        }
    }
    println!("-- engines");
    for m in data.modules.iter().filter(|m| m.thrust > 0.0) {
        println!(
            "{:<18} isp {:>9.0} s (ve {:>9.0} m/s) thrust {:>9.0} N mass {:>7.0} kg  flow {:.2} kg/s",
            m.id,
            m.isp,
            m.isp * 9.80665,
            m.thrust,
            m.mass,
            m.thrust / (m.isp.max(1.0) * 9.80665)
        );
    }
}

fn arb(data: &Arc<GameData>) {
    let cfg = StartConfig::new(11);
    let mut s = new_sim_with(&cfg, Arc::clone(data)).expect("sim");
    let Some((st, _)) = s.nearest_station() else {
        return;
    };
    let mut worst: Vec<(f64, String, f64)> = Vec::new();
    for res in 0..data.resources.len() {
        for frac in [0.02, 0.1, 0.3, 0.6, 0.95] {
            let mut m = s.market(st).clone();
            let kg = m.stock[res] * frac;
            if kg < 1.0 {
                continue;
            }
            let (got, cost) = m.buy(data, res, kg, 1.0);
            let back = m.sell(data, res, got, 1.0);
            if cost > 0.0 {
                worst.push((back / cost, data.resources[res].id.clone(), frac));
            }
        }
    }
    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("round trips at one market (back/cost; >1 = free money):");
    for (r, id, f) in worst.iter().take(8) {
        println!("  {r:.4}  {id:<22} {:.0} % of the stock", f * 100.0);
    }
    println!(
        "  profitable round trips: {} of {}",
        worst.iter().filter(|w| w.0 > 1.0).count(),
        worst.len()
    );
}

fn idle_loop(data: &Arc<GameData>) {
    let cfg = StartConfig::new(5);
    let mut s = new_sim_with(&cfg, Arc::clone(data)).expect("sim");
    let input = ShipInput::default();
    println!(
        "start: credits {:.0} crew {} alive-flag {} docked {:?} fuel {:.0}",
        s.credits,
        s.crew.members.len(),
        s.crew.members.iter().filter(|p| p.alive).count(),
        s.docked,
        s.ship.fuel
    );
    for day in 1..=360 {
        s.advance(DAY, &input);
        if day % 60 == 0 {
            println!(
                "day {day:>3}: credits {:>12.0} loan {:>10.0} crew {} news {} surprises {}",
                s.credits,
                s.bank.loan,
                s.crew.members.iter().filter(|p| p.alive).count(),
                s.news.len(),
                s.surprises.count
            );
        }
    }
}

fn sys(data: &Arc<GameData>) {
    for seed in [7u64] {
        let n = new_sim_with(&StartConfig::new(seed), Arc::clone(data))
            .expect("sim")
            .bodies
            .len();
        for p in 0..n {
            let mut s = new_sim_with(&StartConfig::new(seed), Arc::clone(data)).expect("sim");
            let d = (s.bodies[p].pos - s.ship.pos).length() / 1.496e11;
            let (f0, m0) = (s.ship.fuel, s.ship.mass());
            if d > 8.0 || s.engage_autopilot(p).is_err() {
                continue;
            }
            let mut days = 0;
            while s.autopilot.is_some() && days < 300 {
                s.advance(DAY, &ShipInput::default());
                days += 1;
            }
            println!(
                "seed {seed} body {p:>2} {:<14} {d:>7.3} AU  days {days:>3} fuel used {:>7.0} of {:.0} kg (ship {:.0} kg) done {}",
                s.bodies[p].name,
                f0 - s.ship.fuel,
                s.ship.stats.fuel_cap,
                m0,
                s.autopilot.is_none()
            );
        }
    }
}

/// A colony left to itself for three years (no resupply): population, treasury, supplies, epidemics.
fn colony(data: &Arc<GameData>) {
    for seed in [3u64, 9] {
        let mut s = new_sim_with(&StartConfig::new(seed), Arc::clone(data)).expect("sim");
        s.credits += 1.0e7;
        let mut rules = s.rules.clone();
        rules.invulnerable = true;
        s.set_rules(rules);
        for (id, kg) in [
            ("construction_materials", 9_000.0),
            ("colonists", 1_200.0),
            ("food", 900.0),
            ("water", 1_800.0),
        ] {
            if let Some(r) = data.resource_index(id) {
                s.ship.cargo.add(data, r, kg);
            }
        }
        s.undock();
        let mut ci = None;
        for i in 1..s.bodies.len() {
            if !matches!(s.bodies[i].kind, vector_world::BodyKind::Planet(k) if !k.is_giant()) {
                continue;
            }
            s.spawn_near(i, s.bodies[i].radius + 5_000.0);
            if let Ok(c) = s.found_colony(i, None) {
                ci = Some(c);
                break;
            }
        }
        let Some(ci) = ci else {
            println!("seed {seed}: no colony could be founded");
            continue;
        };
        println!(
            "seed {seed}: colony {} on {:?} tags {:?} pop {:.1} housing {:.1} buildings {}",
            s.colonies[ci].name,
            s.bodies[s.colonies[ci].body].kind,
            s.colonies[ci].tags,
            s.colonies[ci].pop,
            s.colonies[ci].stats.housing,
            s.colonies[ci].buildings.len()
        );
        for step in 1..=36 {
            // Jump the clock: `advance` next to a ground takes minimal steps and barely moves time.
            s.time += 30.0 * DAY;
            s.update_colonies_pub();
            let c = &s.colonies[ci];
            if step % 3 == 0 {
                println!(
                    "  day {:>4}: pop {:>8.1} housing {:>8.1} happy {:.2} unrest {:.2} treasury {:>12.0} income {:>9.0} upkeep {:>9.0} food {:.2} water {:.2} power {:.2} sick {:.1} outbreaks {} pest {:.2}",
                    step * 30,
                    c.pop,
                    c.stats.housing,
                    c.happiness,
                    c.unrest,
                    c.treasury,
                    c.stats.income,
                    c.stats.upkeep,
                    c.stats.food_sat,
                    c.stats.water_sat,
                    c.stats.power_sat,
                    c.sick(),
                    c.outbreaks.len(),
                    c.pest
                );
            }
        }
    }
}
