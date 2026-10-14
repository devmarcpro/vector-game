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

fn one(r: Result<String, String>) -> Vec<String> {
    vec![r.unwrap_or_else(|e| e)]
}

/// Execute one command; returns output lines. `timescale` is mutated by `time`.
pub fn run(cmd: &str, sim: &mut Sim, timescale: &mut f64) -> Vec<String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let num = |i: usize| parts.get(i).and_then(|s| s.parse::<f64>().ok());
    let cheat = !matches!(parts.first().copied(), None | Some("help" | "list"));
    if cheat && !sim.rules.creative {
        return vec!["MODE CREATIF REQUIS (REGLES DE PARTIE)".into()];
    }
    match parts.first().copied() {
        None => vec![],
        Some("help") => vec![
            "help list tp x y z | tpbody NAME | stop | speed V".into(),
            "fuel | repair | assist | time N | credits N | flag I on|off".into(),
            "alltech | reveal | service | days N | spawn pirate|patrol|miner|trader | clearnpcs | recruit K | anchor | rewind I | flare S | jump K | fleetship H  (panneau: Arret defil)".into(),
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
            (Some(x), Some(y), Some(z)) => one(sim.creative_teleport_pos(Vec3::new(x, y, z))),
            _ => vec!["usage: tp x y z".into()],
        },
        Some("tpbody") => {
            let name = parts[1..].join(" ").to_uppercase();
            match sim.bodies.iter().find(|b| b.name == name) {
                Some(b) => {
                    let i = sim
                        .bodies
                        .iter()
                        .position(|x| x.name == b.name)
                        .unwrap_or(0);
                    one(sim.creative_teleport_body(i))
                }
                None => vec![format!("unknown body '{name}'")],
            }
        }
        Some("stop") => one(sim.creative_stop()),
        Some("credits") => match num(1) {
            Some(c) => one(sim.creative_give_credits(c)),
            None => vec!["usage: credits N".into()],
        },
        Some("flag") => match (num(1), parts.get(2)) {
            (Some(i), Some(v)) => one(sim.creative_set_flag(i as usize, *v == "on")),
            _ => vec!["usage: flag 0..11 on|off".into()],
        },
        Some("spawn") => {
            use vector_sim::npc::NpcRole::*;
            let (role, hostile) = match parts.get(1).copied() {
                Some("pirate") => (Pirate, true),
                Some("patrol") => (Patrol, false),
                Some("miner") => (Miner, false),
                _ => (Trader, false),
            };
            one(sim.creative_spawn_npc(role, hostile))
        }
        Some("clearnpcs") => one(sim.creative_clear_npcs()),
        Some("recruit") => one(sim.creative_recruit_any(num(1).unwrap_or(0.0) as usize)),
        Some("anchor") => one(sim.creative_set_anchor()),
        Some("rewind") => one(sim.creative_rewind(num(1).unwrap_or(0.0) as usize)),
        Some("flare") => one(sim.creative_flare(num(1).unwrap_or(1.0))),
        Some("jump") => one(sim.creative_jump(num(1).unwrap_or(0.0) as usize)),
        Some("galaxy") => one(sim.creative_galaxy(num(1).unwrap_or(0.0) as usize)),
        Some("fleetship") => one(sim.creative_add_fleet_ship(num(1).unwrap_or(0.0) as usize)),
        Some("alltech") => one(sim.creative_unlock_all_tech()),
        Some("reveal") => one(sim.creative_reveal_map()),
        Some("service") => one(sim.creative_full_service()),
        Some("days") => match num(1) {
            Some(d) => one(sim.creative_advance_days(d)),
            None => vec!["usage: days N".into()],
        },
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
        let p0 = sim.ship.pos;
        assert!(run("tp 1 2 3", &mut sim, &mut ts)[0].contains("CREATIF"));
        assert_eq!(sim.ship.pos, p0, "cheats are refused outside creative mode");
        assert!(run("help", &mut sim, &mut ts)[0].contains("help"));
        sim.set_rules(vector_sim::rules::Rules {
            creative: true,
            ..Default::default()
        });
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
