//! Research and technology gating, anomaly exploration and environmental hazards.
use crate::combat::Wreck;
use crate::crew::DAY;
use crate::voyage::SCAN_FULL;
use crate::{Event, Sim};
use vector_core::{hash_coord, Rng, Vec3};
use vector_world::orbit::AU;
use vector_world::{AnomalyKind, BodyKind, StarClass};

/// Research points per day from one point of lab `science` at baseline staffing.
pub const LAB_POINTS_PER_DAY: f64 = 6.0;
pub const FLARE_WARNING_S: f64 = 300.0;
pub(crate) const METEOR_MEAN_INTERVAL: f64 = 6.0 * 3600.0;

/// Combined environmental effects at a point.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    /// Radiation dose rate in Gy per hour.
    pub radiation: f64,
    /// Multiplier on sensor range (1 = clear).
    pub sensor_mult: f64,
    /// Multiplier on attitude agility (magnetic interference).
    pub agility_mult: f64,
    /// Extra acceleration from gravity wells.
    pub gravity: Vec3,
    /// Ion flux damaging electronics (damage points per second).
    pub ion: f64,
    /// Probability per second of being thrown out of position by a rift.
    pub rift: f64,
}

/// Half width (m) of the ion tail of a comet, and the damage per second at its axis.
pub const COMET_TAIL_WIDTH: f64 = 3.0e5;
pub const COMET_TAIL_ION: f64 = 4.0;

/// Distance from `p` to the segment `a`-`b`.
fn distance_to_segment(p: Vec3, a: Vec3, b: Vec3) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}

pub fn flare_probability_per_day(c: StarClass) -> f64 {
    match c {
        StarClass::M => 0.04,
        StarClass::K => 0.02,
        StarClass::G => 0.012,
        StarClass::F => 0.01,
        _ => 0.006,
    }
}

impl Sim {
    // ---- technology -------------------------------------------------------

    pub fn is_unlocked(&self, kind: &str, id: &str) -> bool {
        !self.rules.tech_locks
            || self
                .data
                .unlocking_tech(kind, id)
                .is_none_or(|t| self.researched[t])
    }

    /// Whether every prerequisite of tech `t` is researched.
    pub fn tech_available(&self, t: usize) -> bool {
        !self.researched[t]
            && self.data.techs[t]
                .requires
                .iter()
                .all(|r| self.data.tech_index(r).is_some_and(|i| self.researched[i]))
            && self.missing_findings(t).is_empty()
    }

    /// The discoveries tech `t` still waits for (see `Tech::needs`).
    pub fn missing_findings(&self, t: usize) -> Vec<String> {
        self.data.techs[t]
            .needs
            .iter()
            .filter(|n| !self.findings.contains(*n))
            .cloned()
            .collect()
    }

    /// Note what the ship and the colonies hold, and what the labs and colonies study, for the
    /// technologies that wait for discoveries.
    fn scan_findings(&mut self) {
        let mut wanted: Vec<&str> = Vec::new();
        for (t, tech) in self.data.techs.iter().enumerate() {
            if !self.researched[t] {
                wanted.extend(tech.needs.iter().map(String::as_str));
            }
        }
        let mut found: Vec<String> = Vec::new();
        for n in wanted {
            if self.findings.contains(n) {
                continue;
            }
            match n.split_once(':') {
                Some(("resource", id)) => {
                    if let Some(i) = self.data.resource_index(id) {
                        if self.ship.cargo.kg[i] > 0.0
                            || self.colonies.iter().any(|c| c.storage[i] > 0.0)
                        {
                            found.push(n.to_string());
                        }
                    }
                }
                Some(("study", "colony"))
                    if self.colonies.iter().any(|c| c.life.discoveries > 0) =>
                {
                    found.push(n.to_string());
                }
                Some(("study", "lab")) if self.ship_research_rate() > 0.0 => {
                    found.push(n.to_string());
                }
                _ => {}
            }
        }
        self.findings.extend(found);
    }

    pub fn start_research(&mut self, t: usize) -> Result<(), &'static str> {
        if t >= self.data.techs.len() {
            return Err("technologie inconnue");
        }
        if self.researched[t] {
            return Err("deja maitrisee");
        }
        if !self.missing_findings(t).is_empty() {
            return Err("decouvertes manquantes (voir la fiche de la technologie)");
        }
        if !self.tech_available(t) {
            return Err("prerequis manquants");
        }
        self.current_research = Some(t);
        self.research_progress = 0.0;
        Ok(())
    }

    /// Research points per day from the ship's laboratories (powered, scientist-assisted).
    pub fn ship_research_rate(&self) -> f64 {
        let role = crate::crew::Crew::role_index(&self.data, "scientist");
        let eff = self.crew.role_efficiency(&self.data, role, self.time);
        // A supernova remnant or a planetary nebula is worth studying: the labs work harder.
        let eff = eff * (1.0 + self.cosmic_effects().interest);
        self.ship
            .design
            .modules(&self.data)
            .filter(|(i, m)| m.science > 0.0 && self.ship.sys.powered[*i])
            .map(|(i, m)| m.science * self.ship.sys.health[i] * LAB_POINTS_PER_DAY * eff)
            .sum::<f64>()
            * self.mind_factor(crate::mind::T_ANALYSE)
    }

    /// Collect lab output and colony research into the pool, then spend it on the current project.
    pub fn tick_research(&mut self, dt: f64) {
        let speed = self.rules.research_speed;
        self.scan_findings();
        self.research_pool += self.ship_research_rate() * dt / DAY * speed;
        for c in &mut self.colonies {
            if c.research > 0.0 {
                self.research_pool += c.research * speed;
                c.research = 0.0;
            }
        }
        let Some(t) = self.current_research else {
            return;
        };
        let cost = self.data.techs[t].cost;
        let take = self
            .research_pool
            .min((cost - self.research_progress).max(0.0));
        self.research_progress += take;
        self.research_pool -= take;
        if self.research_progress + 1e-9 >= cost {
            self.researched[t] = true;
            self.current_research = None;
            self.research_progress = 0.0;
            self.events.push(Event::ResearchDone { tech: t });
        }
    }

    // ---- anomalies and hazards ----------------------------------------------

    /// Environmental effects at `p` from the anomalies of the current system.
    pub fn environment_at(&self, p: Vec3) -> Environment {
        let mut e = Environment {
            sensor_mult: 1.0,
            agility_mult: 1.0,
            ..Default::default()
        };
        for (i, b) in self.bodies.iter().enumerate() {
            let BodyKind::Anomaly(kind) = b.kind else {
                continue;
            };
            let Some(def) = self.data.anomaly_def(kind.id()) else {
                continue;
            };
            let d = (p - b.pos).length();
            if d >= def.radius {
                continue;
            }
            let _ = i;
            let f = 1.0 - d / def.radius;
            let strength = def.strength * self.hazard_scale();
            match def.hazard.as_str() {
                "radiation" => e.radiation += strength * 120.0 * f * f,
                "gravity" => {
                    let gm = strength * 5.0e14;
                    e.gravity = e.gravity + (b.pos - p) * (gm / (d.max(1000.0).powi(3)));
                }
                "ion" => {
                    e.ion += strength * 40.0 * f;
                    e.sensor_mult *= 1.0 - 0.8 * strength * f.sqrt();
                }
                "sensor" => e.sensor_mult *= 1.0 - 0.85 * strength * f.sqrt(),
                "magnetic" => e.agility_mult *= 1.0 - 0.7 * strength * f.sqrt(),
                "rift" => e.rift += strength * 0.02 * f,
                _ => {}
            }
        }
        // The star itself: the beam of a pulsar, the X-rays of a black hole's disc, and the
        // gravitational lens of the hole that magnifies what the sensors see at its focus.
        let r = p.length();
        let scale = self.hazard_scale();
        e.radiation += self.system.star_radiation(r) * scale;
        e.sensor_mult *= self.system.lens_gain(r);
        // The medium between the stars and the nucleus of the galaxy.
        let cosmic = self.cosmic_effects();
        e.radiation += cosmic.radiation * scale;
        e.sensor_mult *= cosmic.sensor_mult;
        // The ion tail of a comet, blown away from the star: plasma that eats electronics.
        for (i, b) in self.system.bodies.iter().enumerate() {
            let Some(tail) = self.system.comet_tail(i, self.time) else {
                continue;
            };
            let head = self.bodies[i].pos;
            let d = distance_to_segment(p, head, head + tail);
            if d < COMET_TAIL_WIDTH {
                let f = 1.0 - d / COMET_TAIL_WIDTH;
                e.ion += COMET_TAIL_ION * f * scale;
                e.sensor_mult *= 1.0 - 0.3 * f;
            }
            let _ = b;
        }
        e.sensor_mult = e.sensor_mult.max(0.05);
        e.agility_mult = e.agility_mult.max(0.15);
        e
    }

    /// Apply a radiation dose (Gy) to the crew and, when severe, to electronics.
    pub fn irradiate(&mut self, dose: f64) {
        if dose <= 0.0 || self.rules.invulnerable {
            return;
        }
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 100.0) as i64,
            13,
            57,
        ));
        let sick = self.data.disease_index("radiation");
        for p in self.crew.members.iter_mut().filter(|p| p.alive) {
            p.body.pending_dose += dose as f32;
            p.health = (p.health - (dose * 0.04) as f32).max(0.0);
            // Radiation sickness needs about a gray at once; below, only the dose is booked.
            if dose > 1.0 && p.disease.is_none() && rng.f64() < (dose - 0.5).min(1.0) {
                if let Some(d) = sick {
                    p.disease = Some((d, self.data.diseases[d].duration_days as f32));
                }
            }
        }
    }

    /// Hurt the crew in proportion to damage taken by the ship.
    pub(crate) fn injure_crew(&mut self, dmg: f64) {
        if dmg < 30.0 {
            return;
        }
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 100.0) as i64,
            7,
            59,
        ));
        // What a hit of the hull does to a body: a blow, a cut, a burn, a lung torn by the
        // sudden drop of pressure when the hit breaches the cabin.
        let kinds = [
            "contusion",
            "coupure",
            "fracture_os",
            "brulure_plaie",
            "traumatisme_crane",
        ];
        let data = std::sync::Arc::clone(&self.data);
        let p_hurt = (dmg / 800.0).min(0.5);
        for p in self
            .crew
            .members
            .iter_mut()
            .filter(|p| p.alive && !p.body.cryo)
        {
            // The armour worn (`gear.ron`, the same figure that stops blows on foot) stops its
            // share of what reaches the body.
            let stopped = p.gear_stat(&data, "defense").clamp(0.0, 0.8);
            if rng.f64() < p_hurt * (1.0 - stopped) {
                let loss = (rng.range(0.1, 0.4) * (1.0 - stopped)) as f32;
                p.health = (p.health - loss).max(0.0);
                let k = kinds[(rng.next_u64() % kinds.len() as u64) as usize];
                let pick = rng.next_u64();
                let mut hurt = Vec::new();
                hurt.extend(p.body.strike(&data, k, (loss * 2.0).min(1.0), pick));
                if dmg >= 500.0 && rng.f64() < 0.5 {
                    let to = 101.3 * (1.0 - (dmg / 1500.0).min(1.0));
                    hurt.extend(p.body.rapid_decompression(&data, 101.3, to));
                }
                for (o, d) in hurt {
                    if let Some(v) = p.organs.get_mut(o) {
                        *v = (*v - d).max(0.0);
                    }
                }
            }
        }
    }

    /// Per-step hazards: anomaly fields, flares, meteoroid impacts. `dt` in seconds.
    pub(crate) fn hazard_tick(&mut self, dt: f64) {
        if self.in_transit() {
            return;
        }
        self.radiation_tick(dt);
        self.structural_tick(dt);
        let env = self.environment_at(self.ship.pos);
        self.env_sensor = env.sensor_mult;
        self.env_agility = env.agility_mult;
        self.env_gravity = env.gravity;
        let shielded = self.ship.sys.shield_pts > 0.0;
        if env.radiation > 0.0 {
            let protection = self.dose_protection();
            self.irradiate(env.radiation * dt / 3600.0 * protection);
            if shielded {
                self.ship.sys.shield_pts =
                    (self.ship.sys.shield_pts - env.radiation * dt * 0.5).max(0.0);
            }
        }
        if env.ion > 0.0 && self.docked.is_none() {
            let dmg = env.ion * dt * if shielded { 0.15 } else { 1.0 };
            let pick = hash_coord(self.system.seed, (self.time * 10.0) as i64, 5, 61);
            let data = std::sync::Arc::clone(&self.data);
            self.ship
                .sys
                .damage_modules(&data, &self.ship.design, dmg, pick);
            self.ship.refresh_stats(&data);
        }
        if env.rift > 0.0 && self.docked.is_none() {
            let mut rng = Rng::new(hash_coord(
                self.system.seed,
                (self.time * 10.0) as i64,
                9,
                63,
            ));
            if rng.f64() < env.rift * dt {
                let dir = Vec3::new(
                    rng.range(-1.0, 1.0),
                    rng.range(-0.2, 0.2),
                    rng.range(-1.0, 1.0),
                )
                .normalized();
                self.ship.pos = self.ship.pos + dir * rng.range(2.0e4, 5.0e5);
                self.events.push(Event::RiftJump);
            }
        }
        self.cosmic_tick();
        self.flare_tick();
        self.meteor_tick(dt);
    }

    /// Stellar flares: scheduled deterministically per day, announced 5 minutes ahead.
    fn flare_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        if day != self.flare_day {
            self.flare_day = day;
            let BodyKind::Star(class) = self.system.bodies[0].kind else {
                return;
            };
            let mut rng = Rng::new(hash_coord(self.system.seed, day, 77, 5));
            // Black holes and rogue worlds have no surface to flare; a neutron star flares only
            // if it is a magnetar (its bursts are the fiercest).
            let p = match class {
                StarClass::X | StarClass::R => 0.0,
                StarClass::N => match self.system.pulsar() {
                    Some(pl) if pl.magnetar => 0.08,
                    _ => 0.0,
                },
                c => flare_probability_per_day(c),
            };
            self.pending_flare = (rng.f64() < p * self.rules.event_freq).then(|| {
                (
                    day as f64 * DAY + rng.f64() * DAY,
                    rng.range(0.2, 1.0),
                    false,
                )
            });
        }
        let Some((t, severity, warned)) = self.pending_flare else {
            return;
        };
        if !warned && self.time >= t - FLARE_WARNING_S {
            self.pending_flare = Some((t, severity, true));
            self.events.push(Event::FlareWarning);
        }
        if self.time >= t {
            self.pending_flare = None;
            let d_au = (self.ship.pos.length() / AU).max(0.05);
            let mut dose = severity * 2.0 / (d_au * d_au);
            // A variable star is brighter at some times than at others; a magnetar burst is worse.
            dose *= self.system.luminosity_at(self.time) / self.system.luminosity.max(1e-12);
            if self.system.star_class == StarClass::N {
                dose *= 4.0;
            }
            if self.docked.is_some() {
                dose *= crate::navigation::DOCKED_COVER * 2.0;
            } else if self.ship.sys.shield_pts > 0.3 * self.effective.shield_cap.max(1.0) {
                dose *= 0.15;
                self.ship.sys.shield_pts *= 0.5;
            }
            self.irradiate(dose);
            if dose > 0.3 {
                let pick = hash_coord(self.system.seed, day_hash(t), 3, 67);
                let data = std::sync::Arc::clone(&self.data);
                self.ship
                    .sys
                    .damage_modules(&data, &self.ship.design, dose * 60.0, pick);
                self.ship.refresh_stats(&data);
            }
            self.events.push(Event::FlareHit { dose });
        }
    }

    /// Meteoroid impacts while crossing an asteroid belt.
    fn meteor_tick(&mut self, dt: f64) {
        if self.docked.is_some() || self.system.belts.is_empty() {
            return;
        }
        let r = self.ship.pos.length();
        if !self
            .system
            .belts
            .iter()
            .any(|b| r >= b.inner && r <= b.outer)
        {
            return;
        }
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 100.0) as i64,
            11,
            71,
        ));
        if rng.f64() < dt / METEOR_MEAN_INTERVAL * self.rules.event_freq {
            let dmg = rng.range(20.0, 120.0);
            self.events.push(Event::MeteorImpact { damage: dmg });
            self.damage_player(dmg);
        }
    }

    /// Reward for scanning an anomaly completely (once per anomaly).
    pub(crate) fn explore_anomaly(&mut self, i: usize) {
        let BodyKind::Anomaly(kind) = self.bodies[i].kind else {
            return;
        };
        if self.explored.get(i).copied().unwrap_or(true) {
            return;
        }
        self.explored[i] = true;
        self.findings.insert(format!("anomaly:{}", kind.id()));
        let Some(def) = self.data.anomaly_def(kind.id()).cloned() else {
            return;
        };
        let mut rng = Rng::new(hash_coord(self.system.seed, i as i64, 91, 73));
        let research = (rng.range(def.research.0, def.research.1) * self.research_scale()).round();
        let credits = rng.range(def.credits.0, def.credits.1).round();
        self.research_pool += research;
        self.credits += credits;
        if self.multiverse.dimension > 0 {
            self.exotic_find(i);
        }
        let mut cargo: Vec<(usize, f64)> = def
            .loot
            .iter()
            .filter_map(|(r, lo, hi)| {
                self.data
                    .resource_index(r)
                    .map(|ri| (ri, rng.range(*lo, *hi).round()))
            })
            .collect();
        cargo.extend(self.study_exotic(i));
        if !cargo.is_empty() {
            let id = self.next_wreck;
            self.next_wreck += 1;
            let pos = self.bodies[i].pos + Vec3::new(0.0, 0.0, -(self.bodies[i].radius + 300.0));
            self.wrecks.push(Wreck {
                id,
                name: format!("RESTES - {}", kind.label().to_uppercase()),
                pos,
                vel: self.bodies[i].vel,
                cargo,
                born: self.time,
            });
        }
        self.events.push(Event::AnomalyExplored {
            kind_id: kind.id(),
            research,
            credits,
        });
    }

    /// Wrecks left in orbit follow their anomaly (so they stay where the player expects them).
    pub(crate) fn drift_wrecks(&mut self, dt: f64) {
        for w in &mut self.wrecks {
            w.pos = w.pos + w.vel * dt;
        }
    }

    pub fn anomaly_kind(&self, body: usize) -> Option<AnomalyKind> {
        match self.bodies.get(body)?.kind {
            BodyKind::Anomaly(k) => Some(k),
            _ => None,
        }
    }

    /// Called by the scanner when body `i` reaches full knowledge.
    pub(crate) fn on_fully_scanned(&mut self, i: usize) {
        if self.scan[i] >= SCAN_FULL {
            self.explore_anomaly(i);
            self.research_pool += 4.0; // every complete survey teaches something
        }
    }
}

fn day_hash(t: f64) -> i64 {
    (t / 60.0) as i64
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

    fn sim_with_anomaly(kind: Option<AnomalyKind>) -> (Sim, usize) {
        let g = Galaxy::new(12);
        for e in g.nearby([0.0; 3], 400.0) {
            let sys = generate_system(e.seed);
            let found = sys.bodies.iter().position(|b| match (b.kind, kind) {
                (BodyKind::Anomaly(a), Some(k)) => a == k,
                (BodyKind::Anomaly(_), None) => true,
                _ => false,
            });
            if let Some(i) = found {
                let mut s = Sim::new_in_galaxy(g, &e, data());
                s.reveal_all();
                return (s, i);
            }
        }
        panic!("no system with that anomaly");
    }

    fn near(s: &mut Sim, body: usize, dist: f64) {
        s.ship.pos = s.bodies[body].pos + Vec3::new(0.0, 0.0, -dist);
        s.ship.vel = s.bodies[body].vel;
        s.ship.assist = false;
        s.update_ref();
    }

    #[test]
    fn locked_content_requires_research_and_prerequisites() {
        let (mut s, _) = sim_with_anomaly(None);
        s.rules.tech_locks = true;
        assert!(s.is_unlocked("module", "fusion_mk1"), "tier 1 is free");
        assert!(!s.is_unlocked("module", "fusion_mk3"));
        let t = s.data.tech_index("fusion_mk3").expect("tech");
        assert!(
            s.start_research(t).is_err(),
            "needs fusion_mk2? no: needs plasma physics first"
        );
        let pp = s.data.tech_index("plasma_physics").expect("tech");
        s.start_research(pp).expect("plasma physics is open");
        assert!(s.tech_available(pp));
        s.research_pool = 1.0e6;
        s.tick_research(1.0);
        assert!(s.researched[pp] && s.current_research.is_none());
        // fusion_mk3 depends on fusion_mk2's tech only if gated; start it now.
        if s.tech_available(t) {
            s.start_research(t).expect("start");
            s.research_pool = 1.0e6;
            s.tick_research(1.0);
            assert!(s.is_unlocked("module", "fusion_mk3"));
        }
        // Buying a locked hull is refused until its tech is known.
        let (mut s2, _) = sim_with_anomaly(None);
        let st = s2
            .system
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Station);
        if let Some(st) = st {
            s2.rules.tech_locks = true;
            s2.spawn_near(st, s2.bodies[st].radius + 200.0);
            s2.try_dock().expect("dock");
            s2.credits = 1.0e9;
            let frigate = s2.data.hull_index("frigate").expect("hull");
            assert!(
                s2.buy_hull(frigate).is_err(),
                "the frigate hull must be locked"
            );
            let fm = s2.data.module_index("shield_mk3").expect("module");
            assert!(s2.shop_buy_module(fm).is_err(), "locked module");
        }
    }

    #[test]
    fn labs_and_colonies_feed_the_research_pool() {
        let (mut s, _) = sim_with_anomaly(None);
        let data = Arc::clone(&s.data);
        s.ship.design.install_first_fit(&data, "lab_mk1").ok();
        s.ship.refresh_stats(&data);
        s.ship.sys.powered.iter_mut().for_each(|p| *p = true);
        let rate = s.ship_research_rate();
        let had_lab = s.ship.design.modules(&data).any(|(_, m)| m.science > 0.0);
        if had_lab {
            assert!(rate > 1.0, "a lab must produce points: {rate}");
            let p0 = s.research_pool;
            s.tick_research(DAY);
            assert!(s.research_pool > p0);
        }
        // Colony research flows into the same pool.
        let (mut s2, _) = sim_with_anomaly(None);
        let planet = s2
            .system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()))
            .expect("planet");
        s2.ship.cargo.cap_mass = 1.0e6;
        s2.ship.cargo.cap_vol = 1.0e6;
        let d2 = Arc::clone(&s2.data);
        for (id, kg) in [
            ("construction_materials", 3000.0),
            ("colonists", 400.0),
            ("food", 300.0),
            ("water", 600.0),
        ] {
            s2.ship
                .cargo
                .add(&d2, d2.resource_index(id).expect("res"), kg);
        }
        s2.credits = 1.0e6;
        let alt = s2.bodies[planet].radius + 5000.0;
        near(&mut s2, planet, alt);
        let ci = s2.found_colony(planet, None).expect("colony");
        s2.colonies[ci].research = 77.0;
        let p0 = s2.research_pool;
        s2.tick_research(1.0);
        assert!((s2.research_pool - p0 - 77.0).abs() < 1e-6);
        assert_eq!(s2.colonies[ci].research, 0.0);
    }

    #[test]
    fn full_scan_of_an_anomaly_pays_once_and_leaves_loot() {
        let (mut s, i) = sim_with_anomaly(Some(AnomalyKind::AncientStation));
        let (pool0, credits0) = (s.research_pool, s.credits);
        s.scan[i] = SCAN_FULL;
        s.on_fully_scanned(i);
        assert!(s.research_pool > pool0 + 100.0, "research reward");
        assert!(s.credits >= credits0);
        assert!(
            !s.wrecks.is_empty(),
            "salvageable remains appear next to it"
        );
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, Event::AnomalyExplored { .. })));
        let (p1, w1) = (s.research_pool, s.wrecks.len());
        s.on_fully_scanned(i);
        assert!(
            s.research_pool - p1 < 10.0 && s.wrecks.len() == w1,
            "rewarded only once"
        );
    }

    #[test]
    fn radiation_hurts_crew_and_a_shield_protects() {
        let (mut s, i) = sim_with_anomaly(Some(AnomalyKind::RadiationBelt));
        let data = Arc::clone(&s.data);
        let p = crate::crew::generate_person(&data, 5, None);
        s.crew.hire(&data, p, 0, 0.0);
        near(&mut s, i, 10_000.0);
        let h0 = s.crew.members[0].health;
        for _ in 0..600 {
            s.hazard_tick(1.0);
        }
        let unshielded = h0 - s.crew.members[0].health;
        assert!(unshielded > 0.1, "a radiation belt must hurt: {unshielded}");
        s.crew.members[0].health = h0;
        s.ship.sys.shield_pts = 1.0e9;
        for _ in 0..600 {
            s.hazard_tick(1.0);
        }
        let shielded = h0 - s.crew.members[0].health;
        assert!(
            shielded < unshielded * 0.4,
            "shield protects: {shielded} vs {unshielded}"
        );
        // Far away: no effect.
        near(&mut s, i, 5.0e7);
        s.crew.members[0].health = h0;
        s.ship.sys.shield_pts = 0.0;
        for _ in 0..600 {
            s.hazard_tick(1.0);
        }
        assert_eq!(s.crew.members[0].health, h0);
    }

    #[test]
    fn gravity_wells_pull_nebulae_blind_and_magnetic_fields_blunt_agility() {
        let (mut s, i) = sim_with_anomaly(Some(AnomalyKind::GravityWell));
        near(&mut s, i, 60_000.0);
        let e = s.environment_at(s.ship.pos);
        let toward = (s.bodies[i].pos - s.ship.pos).normalized();
        assert!(
            e.gravity.dot(toward) > 1.0,
            "pulls toward the well: {}",
            e.gravity.length()
        );
        let (s2, i2) = sim_with_anomaly(Some(AnomalyKind::Nebula));
        let mut s2 = s2;
        near(&mut s2, i2, 50_000.0);
        assert!(
            s2.environment_at(s2.ship.pos).sensor_mult < 0.5,
            "nebula blinds sensors"
        );
        let (mut s3, i3) = sim_with_anomaly(Some(AnomalyKind::MagneticAnomaly));
        near(&mut s3, i3, 20_000.0);
        assert!(s3.environment_at(s3.ship.pos).agility_mult < 0.7);
        near(&mut s3, i3, 9.0e7);
        let calm = s3.environment_at(s3.ship.pos);
        assert!((calm.agility_mult - 1.0).abs() < 1e-9 && (calm.sensor_mult - 1.0).abs() < 1e-9);
    }

    #[test]
    fn stellar_flares_are_announced_and_shields_or_docking_protect() {
        let (mut s, _) = sim_with_anomaly(None);
        let data = Arc::clone(&s.data);
        let p = crate::crew::generate_person(&data, 9, None);
        s.crew.hire(&data, p, 0, 0.0);
        s.ship.pos = Vec3::new(AU * 0.5, 0.0, 0.0);
        s.update_ref();
        let (mut warned, mut hit) = (false, 0.0);
        // Try many days until a flare occurs.
        for day in 0..400 {
            s.time = f64::from(day) * DAY + 100.0;
            s.flare_tick();
            for _ in 0..2 {
                s.time += DAY / 2.0;
                s.events.clear();
                s.flare_tick();
                for e in &s.events {
                    match e {
                        Event::FlareWarning => warned = true,
                        Event::FlareHit { dose } => hit = *dose,
                        _ => {}
                    }
                }
            }
            if hit > 0.0 {
                break;
            }
        }
        assert!(
            hit > 0.0,
            "a flare should occur within 400 days around any star"
        );
        assert!(warned, "flares are announced first");
        let _ = ShipInput::default();
    }

    #[test]
    fn belts_rain_meteoroids_and_hits_injure_the_crew() {
        let g = Galaxy::new(5);
        let e = g
            .nearby([0.0; 3], 200.0)
            .into_iter()
            .find(|e| !generate_system(e.seed).belts.is_empty())
            .expect("belt system");
        let mut s = Sim::new_in_galaxy(g, &e, data());
        let belt = s.system.belts[0].clone();
        s.ship.pos = Vec3::new((belt.inner + belt.outer) / 2.0, 0.0, 0.0);
        s.update_ref();
        let hull0 = s.ship.hull;
        let mut hits = 0;
        for _ in 0..15_000 {
            s.time += 60.0;
            s.events.clear();
            s.meteor_tick(60.0);
            hits += s
                .events
                .iter()
                .filter(|e| matches!(e, Event::MeteorImpact { .. }))
                .count();
        }
        assert!(
            hits > 20,
            "about one impact every six hours of belt crossing: {hits}"
        );
        assert!(s.ship.hull < hull0);
        // Outside the belt nothing falls.
        s.ship.pos = Vec3::new(belt.outer * 3.0, 0.0, 0.0);
        let hull1 = s.ship.hull;
        for _ in 0..500 {
            s.time += 60.0;
            s.meteor_tick(60.0);
        }
        assert_eq!(s.ship.hull, hull1);
        // Injuries from a hard hit.
        let data = Arc::clone(&s.data);
        for i in 0..4 {
            let p = crate::crew::generate_person(&data, 30 + i, None);
            s.crew.hire(&data, p, 0, 0.0);
        }
        let before: f32 = s.crew.members.iter().map(|p| p.health).sum();
        for k in 0..20 {
            s.time += 1.0 + f64::from(k);
            s.injure_crew(600.0);
        }
        let after: f32 = s.crew.members.iter().map(|p| p.health).sum();
        assert!(after < before - 0.5, "heavy hits hurt people");
        assert!(
            s.crew.members.iter().any(|p| !p.body.wounds.is_empty()),
            "and cause localised wounds"
        );
    }

    #[test]
    fn the_analysis_budget_scales_the_laboratories() {
        let (mut s, _) = sim_with_anomaly(None);
        let data = Arc::clone(&s.data);
        // Make room: the starter ship has no free internal slot.
        s.ship.design.slots.iter_mut().for_each(|x| *x = None);
        s.ship
            .design
            .install_first_fit(&data, "lab_mk1")
            .expect("a lab fits");
        s.ship.refresh_stats(&data);
        s.ship.sys.powered.iter_mut().for_each(|p| *p = true);
        s.ship.sys.temp_k = 290.0;
        s.mind.alloc = [4, 4, 4, 4, 4];
        let even = s.ship_research_rate();
        assert!(even > 0.0, "a lab works");
        s.mind.alloc = [0, 10, 0, 0, 0];
        let focused = s.ship_research_rate();
        s.mind.alloc = [10, 0, 0, 0, 0];
        let starved = s.ship_research_rate();
        assert!(
            focused > even && even > starved,
            "{starved} < {even} < {focused}"
        );
    }
    #[test]
    fn a_moderate_dose_does_not_give_radiation_sickness_but_a_big_one_can() {
        let mut s = crate::scene::new_sim(5, data());
        assert!(s.crew.alive().count() >= 1);
        let sick = s.data.disease_index("radiation").expect("disease");
        let mut big = 0;
        for k in 0..60 {
            s.time = f64::from(k) * 17.3;
            for p in s.crew.members.iter_mut() {
                p.disease = None;
            }
            s.irradiate(0.5);
            assert!(
                s.crew.members.iter().all(|p| p.disease.is_none()),
                "0.5 Gy must not make anyone sick"
            );
            s.irradiate(3.0);
            big += s
                .crew
                .members
                .iter()
                .filter(|p| p.disease.is_some_and(|d| d.0 == sick))
                .count();
        }
        assert!(big > 0, "3 Gy must be able to make the crew sick");
    }
}
