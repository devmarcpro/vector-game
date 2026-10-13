//! High science fiction, part two: what exotic physics does continuously around the ship.
//! The Alcubierre bubble is paid in antimatter (E = 2mc2 inside `warp_energy_j`), the quantum
//! link beats the couriers of `news.rs`, anomalies hiding a naked singularity, a cosmic string,
//! a stasis field, a nanite bloom or a strangelet seed act on the ship (or a colony) every step,
//! the Kardashev scale ranks civilisations and the ship alike, and a body can be cloned.
//! Nothing is stored: every effect is a pure function of the seed, the ship and the date.
use crate::exotic::{warp_energy_j, warp_wake_dose_gy, C};
use crate::Sim;
use vector_world::BodyKind;

/// Game-scale factor between the physical energy of a bubble and what the hold must pay.
pub const WARP_SCALE: f64 = 1.0e-9;
/// Range (light years) inside which the quantum link is instantaneous.
pub const QCOMM_RANGE_LY: f64 = crate::reach::REGION_LY;
/// Time (s) in which a strangelet seed eats a whole world (e-folding of the population).
pub const STRANGELET_TAU_S: f64 = 60.0 * 86_400.0;
/// Nanites (kg) and credits needed to grow a copy of a crew member.
pub const CLONE_NANITES_KG: f64 = 1.0;
pub const CLONE_COST: f64 = 80_000.0;

/// Apparent speed (in c) the bubble must hold to cross `dist_ly`: longer jumps are faster.
pub fn warp_factor(dist_ly: f64) -> f64 {
    1.0 + dist_ly.max(0.0) / 10.0
}

/// Antimatter (kg) the hold pays for one bubble jump: the warp energy converted at 2 c2 per kg.
pub fn warp_antimatter_kg(mass_kg: f64, dist_ly: f64) -> f64 {
    warp_energy_j(mass_kg, warp_factor(dist_ly)) * WARP_SCALE / (2.0 * C * C)
}

/// Kardashev scale: the one law of `megastructure`.
pub use crate::megastructure::kardashev;

/// Power (W) of a civilisation at a given Kardashev level (inverse of `kardashev`).
pub fn power_of_kardashev(k: f64) -> f64 {
    10f64.powf(10.0 * k + 6.0)
}

/// What a phenomenon does to a ship at distance `r` (m).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldEffect {
    /// Acceleration toward the phenomenon (m/s2).
    pub pull: f64,
    /// Hull damage per second.
    pub damage_per_s: f64,
    /// Velocity drag (1/s): a stasis field freezes motion.
    pub drag: f64,
    /// Temporal instability gained per day spent inside.
    pub paradox_per_day: f64,
}

/// Radius (m) of the zone of influence of a phenomenon, 0 if it has no continuous effect.
pub fn field_radius(id: &str) -> f64 {
    match id {
        "naked_singularity" => 5.0e7,
        "cosmic_string" => 2.0e7,
        "stasis_vault" => 3.0e6,
        "nano_bloom" => 5.0e5,
        _ => 0.0,
    }
}

/// Effect of the phenomenon `id` at distance `r` metres.
pub fn field_effect(id: &str, r: f64) -> FieldEffect {
    let radius = field_radius(id);
    if radius <= 0.0 || r > radius {
        return FieldEffect::default();
    }
    let r = r.max(1.0e3);
    match id {
        "naked_singularity" => FieldEffect {
            pull: 1.0e16 / (r * r),
            damage_per_s: 40.0 * (2.0e6 / r).powi(2),
            paradox_per_day: 1.0,
            ..Default::default()
        },
        "cosmic_string" => FieldEffect {
            paradox_per_day: 2.0,
            ..Default::default()
        },
        "stasis_vault" => FieldEffect {
            drag: 0.5,
            ..Default::default()
        },
        "nano_bloom" => FieldEffect {
            damage_per_s: 2.0,
            ..Default::default()
        },
        _ => FieldEffect::default(),
    }
}

impl Sim {
    /// Whether an Alcubierre drive in working order is installed.
    pub fn warp_drive_installed(&self) -> bool {
        self.ship
            .design
            .modules(&self.data)
            .any(|(i, m)| m.id == "exo_warp_drive" && self.ship.sys.health[i] > 0.0)
    }

    /// Antimatter (kg) a warp jump over `dist_ly` costs now (0 without a warp drive).
    pub fn warp_cost_kg(&self, dist_ly: f64) -> f64 {
        if self.warp_drive_installed() {
            warp_antimatter_kg(self.ship.mass(), dist_ly)
        } else {
            self.drive_profile().antimatter_per_ly * dist_ly
        }
    }

    /// Whether a powered quantum link is installed.
    pub fn qcomm_active(&self) -> bool {
        self.ship
            .design
            .modules(&self.data)
            .any(|(i, m)| m.id == "exo_qcomm" && self.ship.sys.powered[i])
    }

    /// Delay (days) news from `dist_ly` away takes: the quantum link kills it within its range.
    pub fn courier_delay_days(&self, dist_ly: f64, base_days: f64) -> f64 {
        if self.qcomm_active() && dist_ly <= QCOMM_RANGE_LY {
            0.0
        } else {
            base_days
        }
    }

    /// Kardashev level of the ship, from the power its reactors can deliver.
    pub fn ship_kardashev(&self) -> f64 {
        kardashev(self.ship.stats.power_capacity.max(0.0))
    }

    /// Radiation of the wake when a bubble collapses: reaches the crew through the plating.
    pub(crate) fn warp_wake(&mut self, dist_ly: f64) {
        let dose = warp_wake_dose_gy(warp_factor(dist_ly)) * self.dose_protection();
        self.irradiate(dose);
        self.crew.log_event(
            self.time,
            format!(
                "BULLE DE DISTORSION EFFONDREE : SILLAGE {:.1} MGY RECU A BORD.",
                dose * 1e3
            ),
        );
    }

    /// Anomalies of this system that hide a phenomenon: (body index, phenomenon index).
    pub fn active_phenomena(&self) -> Vec<(usize, usize)> {
        self.bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(b.kind, BodyKind::Anomaly(_)))
            .filter_map(|(i, _)| self.exotic_at(i).map(|e| (i, e)))
            .collect()
    }

    /// One step of the continuous effects of the exotic phenomena of the system.
    pub(crate) fn tick_exotic_fields(&mut self, dt: f64) {
        if self.docked.is_some() || self.in_transit() || self.multiverse.dimension > 0 {
            return;
        }
        let list = self.active_phenomena();
        if list.is_empty() {
            return;
        }
        let day = crate::bourse::DAY;
        let new_day = ((self.time - dt) / day).floor() != (self.time / day).floor();
        for (b, e) in list {
            let id = self.data.exotics[e].id.clone();
            let studied = self.explored.get(b).copied().unwrap_or(false);
            if id == "strangelet_seed" && !studied {
                let k = (-dt / STRANGELET_TAU_S).exp();
                let seed = self.system.seed;
                let mut ate = false;
                for c in self.colonies.iter_mut().filter(|c| c.system_seed == seed) {
                    c.pop *= k;
                    ate = true;
                }
                if ate && new_day {
                    self.crew.log_event(
                        self.time,
                        "GRAINE DE STRANGELET : UNE COLONIE DU SYSTEME SE DEPEUPLE, LA MATIERE SE CONVERTIT."
                            .to_string(),
                    );
                }
                continue;
            }
            if studied && id == "nano_bloom" {
                continue;
            }
            let r = (self.ship.pos - self.bodies[b].pos).length();
            let f = field_effect(&id, r);
            if f.pull > 0.0 {
                let dir = (self.bodies[b].pos - self.ship.pos).normalized();
                self.ship.vel = self.ship.vel + dir * (f.pull.min(1.0e4) * dt);
            }
            if f.drag > 0.0 {
                self.ship.vel = self.ship.vel * (-f.drag * dt).exp();
            }
            if f.damage_per_s > 0.0 {
                self.damage_player(f.damage_per_s * dt);
            }
            if f.paradox_per_day > 0.0 && new_day {
                let add = f.paradox_per_day.round() as u32;
                self.multiverse.paradox = (self.multiverse.paradox + add).min(10);
                self.crew.log_event(
                    self.time,
                    "LE TEMPS SE DEFORME AUTOUR DE LA COQUE : PARADOXE EN HAUSSE.".to_string(),
                );
            }
        }
    }

    /// Lines of the HUD for the exotic systems: (text, alert).
    pub fn exotic_hud(&self) -> Vec<(String, bool)> {
        let mut v = Vec::new();
        let am = self.antimatter_stored();
        if am > 0.0 {
            let ok = self.trap_powered();
            v.push((
                format!(
                    "ANTIMATIERE {:.2} G  PIEGE {}  E=2MC2 {:.2e} J",
                    am * 1e3,
                    if ok { "ACTIF" } else { "COUPE: FUITE" },
                    crate::exotic::annihilation_energy_j(am)
                ),
                !ok,
            ));
        }
        if self.warp_drive_installed() {
            let text = if self.in_transit() {
                let d = self.jump.as_ref().map_or(0.0, |j| j.dist_ly);
                format!("BULLE ALCUBIERRE ACTIVE  V {:.1} C", warp_factor(d))
            } else {
                format!(
                    "DISTORSION PRETE : 10 AL = {:.1} G D'ANTIMATIERE",
                    self.warp_cost_kg(10.0) * 1e3
                )
            };
            v.push((text, false));
        }
        for (b, e) in self.active_phenomena() {
            let id = &self.data.exotics[e].id;
            let radius = field_radius(id);
            if radius <= 0.0 {
                continue;
            }
            let r = (self.ship.pos - self.bodies[b].pos).length();
            if r < radius * 4.0 {
                v.push((
                    format!(
                        "{} A {:.0} KM",
                        self.data.exotics[e].name.to_uppercase(),
                        r / 1e3
                    ),
                    r < radius,
                ));
            }
        }
        v
    }

    /// Grow a copy of crew member `idx` out of nanites: same skills (slightly blurred), a
    /// body of at most thirty years, and a delicate question of identity for everyone aboard.
    pub fn clone_crew(&mut self, idx: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let nan = data.resource_index("nanites").ok_or("pas de nanites")?;
        let p = self
            .crew
            .members
            .get(idx)
            .filter(|p| p.alive)
            .ok_or("personne a copier")?
            .clone();
        if self.ship.cargo.kg.get(nan).copied().unwrap_or(0.0) < CLONE_NANITES_KG {
            return Err("nanites insuffisantes (1 KG)");
        }
        if self.credits < CLONE_COST {
            return Err("fonds insuffisants");
        }
        self.credits -= CLONE_COST;
        self.ship.cargo.remove(&data, nan, CLONE_NANITES_KG);
        let mut c = p.clone();
        c.name = format!("{} (COPIE)", p.name);
        c.health = 0.6;
        c.age = p.age.min(30.0);
        c.relations.clear();
        for s in &mut c.skills {
            *s *= 0.8;
        }
        let (orig_id, role) = (p.id, p.role);
        let k = self.crew.hire(&data, c, role, self.time);
        let new_id = self.crew.members[k].id;
        self.crew.members[k].add_relation(orig_id, -15);
        if let Some(o) = self.crew.members.iter_mut().find(|q| q.id == orig_id) {
            o.add_relation(new_id, -15);
        }
        for q in self.crew.members.iter_mut().filter(|q| q.alive) {
            q.morale = (q.morale - 0.03).max(0.0);
        }
        let msg = format!(
            "{} A ETE COPIE : DEUX PERSONNES REVENDIQUENT UNE MEME VIE.",
            p.name.to_uppercase()
        );
        self.crew.log_event(self.time, msg.clone());
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exotic::annihilation_energy_j;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;

    fn sim() -> Sim {
        let data = Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        );
        let g = Galaxy::new(8);
        let e = g.nearby([0.0; 3], 100.0).into_iter().next().unwrap();
        Sim::new_in_galaxy(Galaxy::new(8), &e, data)
    }

    #[test]
    fn warp_jump_is_paid_in_antimatter_matching_the_energy() {
        let (m, d) = (5.0e4, 20.0);
        let kg = warp_antimatter_kg(m, d);
        let e = annihilation_energy_j(kg) / 2.0 * 2.0;
        let want = warp_energy_j(m, warp_factor(d)) * WARP_SCALE;
        assert!((e - want).abs() / want < 1e-9);
        assert!(warp_antimatter_kg(m, 40.0) > 2.0 * warp_antimatter_kg(m, 20.0));
        assert!(warp_antimatter_kg(2.0 * m, d) > warp_antimatter_kg(m, d));
    }

    #[test]
    fn kardashev_scale_matches_sagan() {
        assert!((kardashev(1.0e16) - 1.0).abs() < 1e-9);
        assert!((kardashev(1.0e26) - 2.0).abs() < 1e-9);
        assert!((kardashev(power_of_kardashev(2.6)) - 2.6).abs() < 1e-9);
        assert!(kardashev(1.0e6) < 1e-9);
    }

    #[test]
    fn singularity_pulls_harder_and_hurts_only_up_close() {
        let near = field_effect("naked_singularity", 2.0e6);
        let far = field_effect("naked_singularity", 2.0e7);
        assert!(near.pull > 50.0 * far.pull && near.damage_per_s > far.damage_per_s);
        assert_eq!(
            field_effect("naked_singularity", 1.0e9),
            FieldEffect::default()
        );
        assert!(field_effect("stasis_vault", 1.0e5).drag > 0.0);
        assert_eq!(
            field_effect("antimatter_pocket", 1.0),
            FieldEffect::default()
        );
    }

    #[test]
    fn without_a_warp_drive_or_link_nothing_changes() {
        let s = sim();
        assert!(!s.warp_drive_installed() && s.warp_cost_kg(10.0) == 0.0);
        assert!(!s.qcomm_active());
        assert_eq!(s.courier_delay_days(30.0, 5.0), 5.0);
    }

    #[test]
    fn a_warp_jump_burns_antimatter_not_fuel_and_leaves_a_wake() {
        let mut s = sim();
        let d = Arc::clone(&s.data);
        let who = crate::crew::generate_person(&d, 1, None);
        s.crew.hire(&d, who, 1, 0.0);
        let g = Galaxy::new(8);
        let target = g
            .nearby(s.here_pos, 40.0)
            .into_iter()
            .find(|e| e.id != s.here)
            .expect("a neighbour");
        let slot = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.jump_range > 0.0)
            .map(|(i, _)| i)
            .expect("an FTL drive");
        s.ship.design.slots[slot] = s.data.module_index("exo_warp_drive");
        s.ship.refresh_stats(&s.data.clone());
        assert!(s.warp_drive_installed());
        s.ship.pos = vector_core::Vec3::new(4.0e11, 0.0, 0.0);
        s.update_ref();
        let d = target.distance_to(s.here_pos);
        assert_eq!(s.jump_fuel_cost(d), 0.0);
        let need = s.warp_cost_kg(d);
        assert!(need > 0.0);
        assert_eq!(
            s.start_jump(&target),
            Err("antimatiere insuffisante pour la bulle")
        );
        let r = s.data.resource_index("antimatter").unwrap();
        let data = Arc::clone(&s.data);
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.ship.cargo.add(&data, r, need * 1.5);
        s.effective.jump_range = 1.0e3;
        s.start_jump(&target).expect("start");
        let (fuel0, am0) = (s.ship.fuel, s.antimatter_stored());
        s.jump.as_mut().unwrap().charge_left = 0.0;
        s.charge_tick(1.0);
        assert!(s.in_transit());
        assert_eq!(s.ship.fuel, fuel0, "no propellant burnt");
        assert!((am0 - s.antimatter_stored() - need).abs() < need * 1e-6);
        let dose0: f32 = s.crew.members.iter().map(|p| p.body.pending_dose).sum();
        s.arrive();
        let dose1: f32 = s.crew.members.iter().map(|p| p.body.pending_dose).sum();
        assert!(dose1 > dose0, "the wake irradiates the crew");
    }

    #[test]
    fn the_quantum_link_makes_news_instant_but_only_within_its_range() {
        let mut s = sim();
        s.ship.design.slots[0] = s.data.module_index("exo_qcomm");
        s.ship.sys.powered[0] = true;
        assert!(s.qcomm_active());
        assert_eq!(s.courier_delay_days(30.0, 5.0), 0.0);
        assert_eq!(s.courier_delay_days(90.0, 5.0), 5.0);
        s.ship.sys.powered[0] = false;
        assert_eq!(
            s.courier_delay_days(30.0, 5.0),
            5.0,
            "unpowered link is mute"
        );
    }

    #[test]
    fn cloning_costs_nanites_and_credits_and_unsettles_the_crew() {
        let mut s = sim();
        let d = Arc::clone(&s.data);
        let who = crate::crew::generate_person(&d, 1, None);
        s.crew.hire(&d, who, 1, 0.0);
        let nan = s.data.resource_index("nanites").unwrap();
        let data = Arc::clone(&s.data);
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.credits = 1.0e6;
        assert!(s.clone_crew(0).is_err(), "no nanites yet");
        s.ship.cargo.add(&data, nan, 2.0);
        let n = s.crew.members.len();
        let c0 = s.credits;
        s.clone_crew(0).expect("cloned");
        assert_eq!(s.crew.members.len(), n + 1);
        assert!(s.crew.members[n].name.contains("COPIE"));
        assert!((c0 - s.credits - CLONE_COST).abs() < 1e-6);
        assert!((s.ship.cargo.kg[nan] - 1.0).abs() < 1e-9);
        s.credits = 0.0;
        assert!(s.clone_crew(0).is_err(), "no funds");
    }
}
