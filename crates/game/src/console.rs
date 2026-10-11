//! Debug console commands (toggle with the ~ / backquote key).
use vector_core::Vec3;
use vector_sim::Sim;

#[derive(Default)]
pub struct Console {
    pub open: bool,
    pub line: String,
    pub log: Vec<String>,
}

impl Console {
    pub fn print(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 12 {
            self.log.remove(0);
        }
    }
}

/// Execute one command; returns output lines. `timescale` is mutated by `time`.
pub fn run(cmd: &str, sim: &mut Sim, timescale: &mut f64) -> Vec<String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let num = |i: usize| parts.get(i).and_then(|s| s.parse::<f64>().ok());
    match parts.first().copied() {
        None => vec![],
        Some("help") => vec![
            "help list tp x y z | tpbody NAME | stop | speed V".into(),
            "fuel | repair | assist | time N".into(),
        ],
        Some("list") => sim
            .bodies
            .iter()
            .map(|b| {
                format!(
                    "{} r={:.0}m @ {:.0},{:.0},{:.0}",
                    b.name, b.radius, b.pos.x, b.pos.y, b.pos.z
                )
            })
            .collect(),
        Some("tp") => match (num(1), num(2), num(3)) {
            (Some(x), Some(y), Some(z)) => {
                sim.ship.pos = Vec3::new(x, y, z);
                vec!["ok".into()]
            }
            _ => vec!["usage: tp x y z".into()],
        },
        Some("tpbody") => {
            let name = parts[1..].join(" ").to_uppercase();
            match sim.bodies.iter().find(|b| b.name == name) {
                Some(b) => {
                    sim.ship.pos = b.pos - Vec3::new(0.0, 0.0, b.radius * 3.0 + 200.0);
                    sim.ship.vel = Vec3::ZERO;
                    vec!["ok".into()]
                }
                None => vec![format!("unknown body '{name}'")],
            }
        }
        Some("stop") => {
            sim.ship.vel = Vec3::ZERO;
            sim.ship.ang = Vec3::ZERO;
            vec!["ok".into()]
        }
        Some("speed") => match num(1) {
            Some(v) => {
                sim.ship.vel = sim.ship.forward() * v;
                vec!["ok".into()]
            }
            None => vec!["usage: speed v".into()],
        },
        Some("fuel") => {
            sim.ship.fuel = sim.ship.stats.fuel_cap;
            vec!["fuel refilled".into()]
        }
        Some("repair") => {
            sim.ship.hull = sim.ship.stats.hull_max;
            sim.ship.sys.health.fill(1.0);
            let data = std::sync::Arc::clone(&sim.data);
            sim.ship.refresh_stats(&data);
            vec!["hull repaired".into()]
        }
        Some("assist") => {
            sim.ship.assist = !sim.ship.assist;
            vec![format!("assist {}", sim.ship.assist)]
        }
        Some("refit") => {
            let ids: Vec<&str> = parts.iter().skip(2).copied().collect();
            match parts.get(1).map(|h| sim.refit(h, &ids)) {
                Some(Ok(())) => vec!["ship refitted".into()],
                Some(Err(e)) => vec![e],
                None => vec!["usage: refit HULL MODULE...".into()],
            }
        }
        Some("warship") => match sim.refit(
            "frigate",
            &[
                "fusion_mk3",
                "torch_mk3",
                "battery_mk3",
                "radiator_mk4",
                "lifesup_mk1",
                "cargo_mk3",
                "tank_mk3",
                "shield_mk2",
                "habitat_mk2",
                "pulselaser_mk2",
                "massdriver_mk1",
                "missilerack_mk2",
                "pointdefense_mk1",
            ],
        ) {
            Ok(()) => {
                let data = std::sync::Arc::clone(&sim.data);
                for (id, kg) in [
                    ("ammo_slugs", 600.0),
                    ("missiles", 30.0),
                    ("food", 100.0),
                    ("water", 200.0),
                ] {
                    if let Some(r) = data.resource_index(id) {
                        sim.ship.cargo.add(&data, r, kg);
                    }
                }
                vec!["warship ready".into()]
            }
            Err(e) => vec![e],
        },
        Some("time") => match num(1) {
            Some(t) if (0.0..=1000.0).contains(&t) => {
                *timescale = t;
                vec![format!("timescale x{t}")]
            }
            _ => vec!["usage: time 0..1000".into()],
        },
        Some(other) => vec![format!("unknown command '{other}' (try help)")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;

    #[test]
    fn commands_work() {
        let data = std::sync::Arc::new(
            vector_data::GameData::load(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            )
            .expect("data"),
        );
        let mut sim = new_sim(1, data);
        let mut ts = 1.0;
        run("tp 1 2 3", &mut sim, &mut ts);
        assert_eq!(sim.ship.pos, Vec3::new(1.0, 2.0, 3.0));
        run("time 10", &mut sim, &mut ts);
        assert_eq!(ts, 10.0);
        assert!(run("tpbody nowhere", &mut sim, &mut ts)[0].contains("unknown"));
        sim.ship.fuel = 0.0;
        run("fuel", &mut sim, &mut ts);
        assert_eq!(sim.ship.fuel, sim.ship.stats.fuel_cap);
    }
}
