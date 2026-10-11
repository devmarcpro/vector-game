//! Non-player ships: traders and miners move goods between stations, patrols inspect, pirates lurk.
//! They obey the same market rules as the player and make stocks (and prices) move.
use crate::crew::DAY;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng, Vec3};
use vector_world::BodyKind;

/// Cruise speed between bodies (m/s).
pub const CRUISE_SPEED: f64 = 1.5e5;
pub const MIN_TRIP: f64 = 600.0;
/// Largest lot a trader carries (kg).
pub const TRADE_KG: f64 = 4000.0;
pub const PATROL_RADIUS: f64 = 8_000.0;
pub const SCAN_RANGE: f64 = 20_000.0;
pub const SCAN_COOLDOWN: f64 = 6.0 * 3600.0;
pub const PIRATE_ALERT_RANGE: f64 = 40_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcRole {
    Trader,
    Miner,
    Pirate,
    Patrol,
}

impl NpcRole {
    pub fn label(self) -> &'static str {
        match self {
            NpcRole::Trader => "CARGO",
            NpcRole::Miner => "MINEUR",
            NpcRole::Pirate => "PIRATE",
            NpcRole::Patrol => "PATROUILLE",
        }
    }
}

/// Tactical posture of a ship in a fight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiMode {
    Pursue,
    Flee,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NpcState {
    Docked {
        at: usize,
        until: f64,
    },
    Travel {
        from: Vec3,
        to: usize,
        t0: f64,
        t1: f64,
    },
    /// Out of the system (selling the surplus, bringing back what is missing).
    Away {
        home: usize,
        until: f64,
    },
    Working {
        rock: usize,
        until: f64,
    },
    Lurk {
        rock: usize,
    },
    Orbit {
        around: usize,
        phase: f64,
    },
    /// Explicitly flown (integrated) while fighting.
    Combat {
        mode: AiMode,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Npc {
    pub id: u32,
    pub role: NpcRole,
    pub name: String,
    pub faction: Option<usize>,
    pub pos: Vec3,
    pub vel: Vec3,
    pub state: NpcState,
    pub cargo: Option<(usize, f64)>,
    pub last_scan: f64,
    pub home: usize,
    pub hull: f64,
    pub hull_max: f64,
    pub shield: f64,
    pub shield_max: f64,
    pub level: u8,
    /// Shoots at the player.
    pub hostile: bool,
    /// Crippled: drifts, harmless, can be boarded.
    pub disabled: bool,
    /// The player attacked it first (it fights back or flees).
    pub provoked: bool,
    pub hail_deadline: Option<f64>,
}

impl Npc {
    pub fn visible(&self) -> bool {
        !matches!(self.state, NpcState::Away { .. })
    }
    pub fn color_class(&self) -> u8 {
        match self.role {
            NpcRole::Trader => 0,
            NpcRole::Miner => 1,
            NpcRole::Pirate => 2,
            NpcRole::Patrol => 3,
        }
    }
}

fn smooth(u: f64) -> (f64, f64) {
    let u = u.clamp(0.0, 1.0);
    (u * u * (3.0 - 2.0 * u), 6.0 * u * (1.0 - u))
}

impl Sim {
    fn npc_name(&self, role: NpcRole, rng: &mut Rng, faction: Option<usize>) -> String {
        use vector_world::names::{self, Spec};
        let culture = faction
            .and_then(|f| {
                self.data
                    .cultures
                    .iter()
                    .position(|c| c.id == self.data.factions[f].culture)
            })
            .unwrap_or((rng.next_u64() % self.data.cultures.len() as u64) as usize);
        let c = self.data.cultures[culture].id.as_str();
        let h = rng.next_u64();
        // Two draws, as the former syllable name did: the rest of the traffic stays the same.
        let h2 = rng.next_u64();
        // Pirates and traders sail named ships; miners and patrols are known by their captain.
        let who = match role {
            NpcRole::Pirate => Spec::new(if h & 1 == 0 {
                "ship_pirate_m"
            } else {
                "ship_pirate_f"
            })
            .name(h >> 1),
            NpcRole::Trader => Spec::new(if h & 1 == 0 {
                "ship_civil_m"
            } else {
                "ship_civil_f"
            })
            .culture(Some(c))
            .name(h >> 1),
            NpcRole::Miner => names::given_name(c, h & 1 == 1, h >> 1),
            NpcRole::Patrol => {
                format!(
                    "{} {}",
                    names::title(c, h & 1 == 1, h >> 1),
                    names::surname(c, h2)
                )
            }
        };
        format!("{} {}", role.label(), who).to_uppercase()
    }

    /// Populate the freshly entered system with traffic (deterministic from the system and the date).
    pub fn spawn_traffic(&mut self) {
        self.npcs.clear();
        self.target_npc = None;
        let stations: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| self.bodies[i].kind == BodyKind::Station)
            .collect();
        let rocks: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| matches!(self.bodies[i].kind, BodyKind::Asteroid(_)))
            .collect();
        let epoch = (self.time / (30.0 * DAY)).floor() as i64;
        let mut rng = Rng::new(hash_coord(self.system.seed, epoch, 123, 9));
        let faction = self.faction;
        let make = |sim: &mut Sim, rng: &mut Rng, role: NpcRole, home: usize, state: NpcState| {
            let id = sim.next_npc;
            sim.next_npc += 1;
            let name = sim.npc_name(role, rng, faction);
            let level = 1 + (rng.next_u64() % 3) as u8;
            let stats = crate::combat::npc_stats(role, level);
            sim.npcs.push(Npc {
                id,
                role,
                name,
                faction,
                pos: Vec3::ZERO,
                vel: Vec3::ZERO,
                state,
                cargo: None,
                last_scan: 0.0,
                home,
                hull: stats.hull,
                hull_max: stats.hull,
                shield: stats.shield,
                shield_max: stats.shield,
                level,
                hostile: false,
                disabled: false,
                provoked: false,
                hail_deadline: None,
            });
        };
        let now = self.time;
        for &st in &stations {
            for _ in 0..(1 + rng.next_u64() % 2) {
                let until = now + rng.range(0.0, 8.0 * 3600.0);
                make(
                    self,
                    &mut rng,
                    NpcRole::Trader,
                    st,
                    NpcState::Docked { at: st, until },
                );
            }
            if faction.is_some() {
                let phase = rng.range(0.0, std::f64::consts::TAU);
                make(
                    self,
                    &mut rng,
                    NpcRole::Patrol,
                    st,
                    NpcState::Orbit { around: st, phase },
                );
            }
            if !rocks.is_empty() {
                let until = now + rng.range(0.0, 4.0 * 3600.0);
                make(
                    self,
                    &mut rng,
                    NpcRole::Miner,
                    st,
                    NpcState::Docked { at: st, until },
                );
            }
        }
        if !rocks.is_empty() {
            let pirates = if faction.is_none() {
                2
            } else {
                usize::from(rng.f64() < 0.5)
            };
            for _ in 0..pirates {
                let rock = rocks[(rng.next_u64() % rocks.len() as u64) as usize];
                make(
                    self,
                    &mut rng,
                    NpcRole::Pirate,
                    rock,
                    NpcState::Lurk { rock },
                );
            }
        }
        self.place_npcs();
    }

    /// Compute every NPC's position and velocity for the current time.
    pub(crate) fn place_npcs(&mut self) {
        let t = self.time;
        for k in 0..self.npcs.len() {
            let (pos, vel) = {
                let n = &self.npcs[k];
                let ring = |sim: &Sim, body: usize, radius: f64, w: f64| {
                    let b = &sim.bodies[body];
                    let a = (f64::from(n.id) * 2.399) + t * w;
                    (
                        b.pos
                            + Vec3::new(
                                a.cos() * radius,
                                0.2 * radius * (f64::from(n.id)).sin(),
                                a.sin() * radius,
                            ),
                        b.vel,
                    )
                };
                match &n.state {
                    NpcState::Docked { at, .. } => {
                        ring(self, *at, 2000.0 + f64::from(n.id % 5) * 500.0, 1.0e-3)
                    }
                    NpcState::Working { rock, .. } => {
                        ring(self, *rock, self.bodies[*rock].radius + 400.0, 1.0e-3)
                    }
                    NpcState::Lurk { rock } => {
                        ring(self, *rock, self.bodies[*rock].radius + 3000.0, 2.0e-4)
                    }
                    NpcState::Orbit { around, phase } => {
                        let b = &self.bodies[*around];
                        let a = phase + t * std::f64::consts::TAU / (3.0 * 3600.0);
                        (
                            b.pos
                                + Vec3::new(a.cos() * PATROL_RADIUS, 0.0, a.sin() * PATROL_RADIUS),
                            b.vel,
                        )
                    }
                    NpcState::Travel { from, to, t0, t1 } => {
                        let dur = (t1 - t0).max(1.0);
                        let (s, ds) = smooth((t - t0) / dur);
                        let target = self.system.state(*to, *t1).0;
                        (*from + (target - *from) * s, (target - *from) * (ds / dur))
                    }
                    NpcState::Away { .. } => (Vec3::new(1.0e15, 0.0, 0.0), Vec3::ZERO),
                    NpcState::Combat { .. } => (n.pos, n.vel),
                }
            };
            self.npcs[k].pos = pos;
            self.npcs[k].vel = vel;
        }
    }

    fn start_travel(&mut self, k: usize, to: usize) {
        let from = self.npcs[k].pos;
        let target = self.bodies[to].pos;
        let dur = ((target - from).length() / CRUISE_SPEED).max(MIN_TRIP);
        self.npcs[k].state = NpcState::Travel {
            from,
            to,
            t0: self.time,
            t1: self.time + dur,
        };
    }

    /// Advance all NPC state machines to the current time.
    pub(crate) fn update_npcs(&mut self) {
        if self.npcs.is_empty() {
            return;
        }
        let now = self.time;
        let stations: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| self.bodies[i].kind == BodyKind::Station)
            .collect();
        let rocks: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| matches!(self.bodies[i].kind, BodyKind::Asteroid(_)))
            .collect();
        for k in 0..self.npcs.len() {
            let mut rng = Rng::new(hash_coord(
                self.system.seed,
                i64::from(self.npcs[k].id),
                now as i64,
                5,
            ));
            let state = self.npcs[k].state.clone();
            let role = self.npcs[k].role;
            match state {
                NpcState::Docked { at, until } if now >= until => match role {
                    NpcRole::Trader => self.trader_depart(k, at, &stations, &mut rng),
                    NpcRole::Miner => {
                        self.unload(k, at);
                        if rocks.is_empty() {
                            self.npcs[k].state = NpcState::Docked {
                                at,
                                until: now + 6.0 * 3600.0,
                            };
                        } else {
                            let rock = rocks[(rng.next_u64() % rocks.len() as u64) as usize];
                            self.start_travel(k, rock);
                        }
                    }
                    _ => {}
                },
                NpcState::Travel { to, t1, .. } if now >= t1 => {
                    let kind = self.bodies[to].kind;
                    let next = match (role, kind) {
                        (NpcRole::Miner, BodyKind::Asteroid(_)) => NpcState::Working {
                            rock: to,
                            until: now + rng.range(3.0, 8.0) * 3600.0,
                        },
                        (_, BodyKind::Station) => {
                            self.unload(k, to);
                            NpcState::Docked {
                                at: to,
                                until: now + rng.range(2.0, 10.0) * 3600.0,
                            }
                        }
                        _ => NpcState::Docked {
                            at: self.npcs[k].home,
                            until: now + 3600.0,
                        },
                    };
                    self.npcs[k].state = next;
                }
                NpcState::Working { rock, until } if now >= until => {
                    self.mine_haul(k, rock, &mut rng);
                    let home = self.npcs[k].home;
                    self.start_travel(k, home);
                }
                NpcState::Away { home, until } if now >= until => {
                    // Back with whatever the station is shortest of.
                    let kg = rng.range(0.4, 1.0) * TRADE_KG;
                    let short = {
                        let m = self.market(home);
                        (0..m.stock.len()).min_by(|&a, &b| m.fullness(a).total_cmp(&m.fullness(b)))
                    };
                    self.npcs[k].cargo = short.map(|r| (r, kg));
                    let dir =
                        Vec3::new(rng.range(-1.0, 1.0), 0.1, rng.range(-1.0, 1.0)).normalized();
                    let from = self.bodies[home].pos + dir * 5.0e8;
                    self.npcs[k].pos = from;
                    self.npcs[k].state = NpcState::Travel {
                        from,
                        to: home,
                        t0: now,
                        t1: now + 5.0e8 / 2.0e4,
                    };
                }
                NpcState::Orbit { .. } => self.patrol_scan(k),
                NpcState::Lurk { .. } => self.pirate_watch(k),
                _ => {}
            }
        }
        self.place_npcs();
    }

    /// Hand the cargo over to the station market.
    fn unload(&mut self, k: usize, station: usize) {
        if let Some((res, kg)) = self.npcs[k].cargo.take() {
            self.market(station).stock[res] += kg;
        }
    }

    fn trader_depart(&mut self, k: usize, at: usize, stations: &[usize], rng: &mut Rng) {
        self.unload(k, at);
        let others: Vec<usize> = stations.iter().copied().filter(|&s| s != at).collect();
        // Load the most abundant good (relative to its reference level).
        let (res, kg) = {
            let m = self.market(at);
            let best = (0..m.stock.len())
                .max_by(|&a, &b| m.fullness(a).total_cmp(&m.fullness(b)))
                .unwrap_or(0);
            let kg = (0.25 * m.stock[best]).min(TRADE_KG);
            if m.fullness(best) > 1.3 && kg > 50.0 {
                m.stock[best] -= kg;
                (Some(best), kg)
            } else {
                (None, 0.0)
            }
        };
        self.npcs[k].cargo = res.map(|r| (r, kg));
        if others.is_empty() {
            // The only station: carry the surplus out of the system and come back later.
            self.npcs[k].cargo = None;
            self.npcs[k].state = NpcState::Away {
                home: at,
                until: self.time + rng.range(3.0, 10.0) * DAY,
            };
        } else {
            let dest = others[(rng.next_u64() % others.len() as u64) as usize];
            self.start_travel(k, dest);
        }
    }

    fn mine_haul(&mut self, k: usize, rock: usize, rng: &mut Rng) {
        let BodyKind::Asteroid(class) = self.bodies[rock].kind else {
            return;
        };
        let want = rng
            .range(2000.0, 6000.0)
            .min(self.remaining_mass(rock) * 0.5);
        if want < 10.0 {
            return;
        }
        self.mined[rock] += want;
        let top = self
            .data
            .asteroids
            .iter()
            .find(|a| a.id == class.id())
            .and_then(|a| {
                a.composition
                    .iter()
                    .filter(|(r, _)| r != "regolith")
                    .max_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(r, _)| r.clone())
            })
            .and_then(|id| self.data.resource_index(&id));
        self.npcs[k].cargo = top.map(|r| (r, want * 0.6));
        let frac = (1.0 - self.mined[rock] / self.system.bodies[rock].mass).max(0.0);
        self.bodies[rock].radius = self.system.bodies[rock].radius * frac.cbrt();
    }

    /// Patrols inspect the player when close, at most once per cooldown.
    fn patrol_scan(&mut self, k: usize) {
        if self.docked.is_some() || self.faction.is_none() {
            return;
        }
        if self.time - self.npcs[k].last_scan < SCAN_COOLDOWN {
            return;
        }
        if (self.npcs[k].pos - self.ship.pos).length() > SCAN_RANGE {
            return;
        }
        self.npcs[k].last_scan = self.time;
        self.events.push(Event::PatrolScan);
        self.customs_check();
        if self.is_wanted() {
            self.npcs[k].hostile = true;
            self.npcs[k].state = NpcState::Combat {
                mode: AiMode::Pursue,
            };
            self.events.push(Event::PatrolHostile);
        }
    }

    /// Pirates notice rich-looking ships passing close by.
    fn pirate_watch(&mut self, k: usize) {
        if self.rules.ai_aggression <= 0.0
            || self.docked.is_some()
            || self.time - self.npcs[k].last_scan < DAY
        {
            return;
        }
        if (self.npcs[k].pos - self.ship.pos).length() > PIRATE_ALERT_RANGE {
            return;
        }
        let value: f64 = (0..self.ship.cargo.kg.len())
            .map(|r| self.ship.cargo.kg[r] * self.data.resources[r].value)
            .sum();
        if value > 2000.0 / self.rules.ai_aggression.max(0.1) {
            self.npcs[k].last_scan = self.time;
            self.events.push(Event::PirateAlert);
            self.npcs[k].state = NpcState::Combat {
                mode: AiMode::Pursue,
            };
        }
    }

    /// Nearest-first list of NPC indices currently visible.
    pub fn visible_npcs(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.npcs.len())
            .filter(|&i| self.npcs[i].visible())
            .collect();
        v.sort_by(|&a, &b| {
            (self.npcs[a].pos - self.ship.pos)
                .length()
                .total_cmp(&(self.npcs[b].pos - self.ship.pos).length())
        });
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShipInput;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;
    use vector_world::generate_system;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn sim_where<F: Fn(&vector_world::System) -> bool>(f: F) -> Sim {
        let g = Galaxy::new(8);
        let e = g
            .nearby([0.0; 3], 120.0)
            .into_iter()
            .find(|e| f(&generate_system(e.seed)))
            .expect("matching system");
        let mut s = Sim::new_in_galaxy(g, &e, data());
        s.reveal_all();
        s
    }

    fn count_kind(s: &vector_world::System, k: fn(&BodyKind) -> bool) -> usize {
        s.bodies.iter().filter(|b| k(&b.kind)).count()
    }

    fn two_stations_and_belt() -> Sim {
        sim_where(|s| {
            count_kind(s, |k| *k == BodyKind::Station) >= 2
                && count_kind(s, |k| matches!(k, BodyKind::Asteroid(_))) > 0
        })
    }

    #[test]
    fn traffic_spawns_deterministically_with_every_role() {
        let a = two_stations_and_belt();
        let b = two_stations_and_belt();
        assert!(a.npcs.len() >= 6, "{}", a.npcs.len());
        assert_eq!(
            a.npcs.iter().map(|n| n.name.clone()).collect::<Vec<_>>(),
            b.npcs.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        );
        for role in [NpcRole::Trader, NpcRole::Miner] {
            assert!(a.npcs.iter().any(|n| n.role == role), "{role:?} missing");
        }
        assert!(a.npcs.iter().all(|n| n.pos.length().is_finite()));
    }

    #[test]
    fn traders_move_goods_between_stations() {
        let mut s = two_stations_and_belt();
        let stations: Vec<usize> = (0..s.bodies.len())
            .filter(|&i| s.bodies[i].kind == BodyKind::Station)
            .collect();
        for &st in &stations {
            s.market(st);
        }
        let snapshot: Vec<Vec<f64>> = stations
            .iter()
            .map(|st| s.markets[st].stock.clone())
            .collect();
        let mut travelled = false;
        for _ in 0..60 {
            s.advance(DAY, &ShipInput::default());
            travelled |= s
                .npcs
                .iter()
                .any(|n| matches!(n.state, NpcState::Travel { .. }));
        }
        assert!(travelled, "someone must have flown between stations");
        let changed = stations.iter().zip(&snapshot).any(|(st, old)| {
            // Relaxation alone would converge to targets; NPC lots add visible deviations in total mass flow.
            s.markets[st]
                .stock
                .iter()
                .zip(old)
                .any(|(a, b)| (a - b).abs() > 1.0)
        });
        assert!(changed);
        assert!(s
            .npcs
            .iter()
            .all(|n| n.pos.length().is_finite() && n.vel.length().is_finite()));
    }

    #[test]
    fn a_year_of_traffic_keeps_markets_sane() {
        let mut s = two_stations_and_belt();
        for _ in 0..365 {
            s.advance(DAY, &ShipInput::default());
        }
        for (st, m) in &s.markets {
            for r in 0..m.stock.len() {
                assert!(
                    m.stock[r].is_finite() && m.stock[r] >= -1e-6,
                    "negative stock at {st}/{r}: {}",
                    m.stock[r]
                );
                assert!(
                    m.stock[r] < 60.0 * m.reference[r],
                    "runaway stock {r}: {} vs {}",
                    m.stock[r],
                    m.reference[r]
                );
                let p = m.price(&s.data, r);
                assert!(p.is_finite() && p > 0.0);
            }
        }
        assert!(s
            .npcs
            .iter()
            .all(|n| n.cargo.is_none_or(|(_, kg)| kg.is_finite() && kg >= 0.0)));
    }

    #[test]
    fn single_station_traders_leave_and_return() {
        let mut s = sim_where(|sys| count_kind(sys, |k| *k == BodyKind::Station) == 1);
        let mut away = false;
        let mut back = false;
        for _ in 0..40 {
            s.advance(DAY, &ShipInput::default());
            let any_away = s
                .npcs
                .iter()
                .any(|n| matches!(n.state, NpcState::Away { .. }));
            away |= any_away;
            back |= away
                && s.npcs.iter().any(|n| {
                    n.role == NpcRole::Trader && !matches!(n.state, NpcState::Away { .. })
                });
        }
        assert!(
            away && back,
            "traders should cycle through the outside world"
        );
    }

    #[test]
    fn travel_interpolation_is_continuous_and_lands_on_the_station() {
        let mut s = two_stations_and_belt();
        let st: Vec<usize> = (0..s.bodies.len())
            .filter(|&i| s.bodies[i].kind == BodyKind::Station)
            .collect();
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Trader)
            .expect("trader");
        s.npcs[k].pos = s.bodies[st[0]].pos;
        s.npcs[k].state = NpcState::Travel {
            from: s.bodies[st[0]].pos,
            to: st[1],
            t0: s.time,
            t1: s.time + 1000.0,
        };
        let total = (s.system.state(st[1], s.time + 1000.0).0 - s.bodies[st[0]].pos).length();
        let mut last = s.npcs[k].pos;
        for _ in 0..100 {
            s.time += 10.0;
            s.place_npcs();
            let p = s.npcs[k].pos;
            assert!((p - last).length() < total * 0.05 + 1.0, "no teleporting");
            last = p;
        }
        let end = s.system.state(st[1], s.time).0;
        assert!(
            (s.npcs[k].pos - end).length() < 1.0,
            "arrives at the destination"
        );
    }

    #[test]
    fn patrols_inspect_smugglers_in_claimed_space() {
        let mut s = two_stations_and_belt();
        s.faction = Some(s.data.faction_index("hegemonie").expect("f"));
        s.spawn_traffic();
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Patrol)
            .expect("patrol");
        let gold = s.data.resource_index("gold").expect("gold");
        let data = Arc::clone(&s.data);
        let mut caught = false;
        for n in 0..80 {
            s.ship.cargo.add(
                &data,
                gold,
                if s.ship.cargo.kg[gold] > 0.0 {
                    0.0
                } else {
                    20.0
                },
            );
            s.update_npcs();
            s.ship.pos = s.npcs[k].pos + Vec3::new(3000.0, 0.0, 0.0);
            s.ship.vel = s.npcs[k].vel;
            s.time += SCAN_COOLDOWN + 1.0 + f64::from(n);
            s.refresh_bodies();
            s.events.clear();
            s.update_npcs();
            if s.events.iter().any(|e| matches!(e, Event::Customs { .. })) {
                caught = true;
                break;
            }
        }
        assert!(
            caught,
            "a patrol next to a smuggler should find the contraband sooner or later"
        );
    }

    #[test]
    fn pirates_alert_when_a_rich_ship_passes() {
        let mut s = two_stations_and_belt();
        s.faction = None;
        s.spawn_traffic();
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .expect("pirate");
        let gold = s.data.resource_index("gold").expect("gold");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, gold, 100.0);
        s.ship.pos = s.npcs[k].pos + Vec3::new(10_000.0, 0.0, 0.0);
        s.events.clear();
        s.time += 2.0 * DAY;
        s.update_npcs();
        assert!(s.events.iter().any(|e| matches!(e, Event::PirateAlert)));
    }
}
