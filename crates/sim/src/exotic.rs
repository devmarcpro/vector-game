//! High science fiction: exotic physics (antimatter, warp, exotic matter...), precursor ruins and
//! the Fermi paradox. Pure rules at the top; the emergent studying of exotic phenomena at the
//! bottom hooks into anomaly exploration (`science.rs`), the multiverse paradox and the crew log.
//! Nothing here is scripted: the phenomenon, the ruins and the truth behind the silence of the
//! galaxy all come from `hash(seed, coordinates)`.
use crate::{Event, Sim};
use vector_core::{hash_coord, Rng};
use vector_data::{Exotic, GameData};

/// Speed of light (m/s).
pub const C: f64 = 299_792_458.0;
/// Joules released inside the ship per hull point of damage (game scale).
pub const J_PER_DAMAGE: f64 = 2.0e11;
/// Fraction of the stored antimatter lost per second when the magnetic trap is unpowered.
pub const TRAP_LEAK_PER_S: f64 = 0.002;
/// Precursor ruins: share of systems touched by the ancients.
pub const PRECURSOR_SHARE: f64 = 0.2;
/// Clues about the Fermi paradox needed to understand what happened.
pub const CLUES_NEEDED: u32 = 5;

/// Energy released when `antimatter_kg` meets as much ordinary matter: E = 2 m c^2.
pub fn annihilation_energy_j(antimatter_kg: f64) -> f64 {
    2.0 * antimatter_kg.max(0.0) * C * C
}

/// Hull damage of an antimatter containment breach.
pub fn breach_damage(antimatter_kg: f64) -> f64 {
    annihilation_energy_j(antimatter_kg) / J_PER_DAMAGE
}

/// Energy (J) to keep an Alcubierre bubble around a ship of `mass_kg` at `v_c` times the speed
/// of light: grows faster than the square of the speed (the bubble thins and needs more
/// negative energy to stay stable).
pub fn warp_energy_j(mass_kg: f64, v_c: f64) -> f64 {
    let v = v_c.max(0.0);
    mass_kg.max(0.0) * C * C * v * v * (1.0 + v * v / 4.0)
}

/// Dose (Gy) a world receives when a warp bubble drops its swept-up particles on it.
pub fn warp_wake_dose_gy(v_c: f64) -> f64 {
    0.002 * v_c.max(0.0).powi(2)
}

/// Whether the ancients left ruins in the system at `star_pos` (light years).
pub fn is_precursor_system(galaxy_seed: u64, star_pos: [f64; 3]) -> bool {
    let h = hash_coord(
        galaxy_seed ^ 0xA9C1E,
        (star_pos[0] * 10.0) as i64,
        (star_pos[1] * 10.0) as i64,
        (star_pos[2] * 10.0) as i64,
    );
    (h >> 11) as f64 / (1u64 << 53) as f64 <= PRECURSOR_SHARE
}

/// Indices (in `GameData::exotics`) of the entries of a kind.
pub fn of_kind(data: &GameData, kind: &str) -> Vec<usize> {
    (0..data.exotics.len())
        .filter(|&i| data.exotics[i].kind == kind)
        .collect()
}

/// The true answer to the Fermi paradox of this galaxy (index in `GameData::exotics`).
pub fn fermi_truth(data: &GameData, galaxy_seed: u64) -> Option<usize> {
    let list = of_kind(data, "fermi");
    let h = hash_coord(galaxy_seed ^ 0xFE42, 7, 7, 7);
    (!list.is_empty()).then(|| list[(h % list.len() as u64) as usize])
}

/// How well a first contact goes (0 total misunderstanding .. 1 perfect): translation skill
/// against the strangeness of the other (0 close .. 1 alien), plus a little luck.
pub fn contact_understanding(translation: f64, strangeness: f64, luck: f64) -> f64 {
    (translation.clamp(0.0, 2.0) * 0.5 * (1.0 - 0.6 * strangeness.clamp(0.0, 1.0))
        + 0.3 * (luck.clamp(0.0, 1.0) - 0.5))
        .clamp(0.0, 1.0)
}

/// What studying an exotic phenomenon yields.
pub struct Outcome {
    pub research: f64,
    pub credits: f64,
    pub damage: f64,
    pub loot: Vec<(usize, f64)>,
    pub understanding: Option<f64>,
}

/// Rules of the study: temporal instability (paradoxes) raises both reward and danger; the
/// ancients' work is worth twice as much in a precursor system; entities must be understood.
pub fn study(
    data: &GameData,
    e: &Exotic,
    rng: &mut Rng,
    paradox: u32,
    precursor_system: bool,
    translation: f64,
) -> Outcome {
    let time_factor = 1.0 + 0.1 * f64::from(paradox.min(10));
    let mut research = rng.range(e.research.0, e.research.1) * time_factor;
    if precursor_system && e.kind == "precursor" {
        research *= 2.0;
    }
    let mut credits = rng.range(e.credits.0, e.credits.1);
    let mut damage = e.danger * rng.range(20.0, 100.0) * time_factor;
    let mut understanding = None;
    if e.kind == "entity" {
        let u = contact_understanding(translation, e.danger, rng.f64());
        understanding = Some(u);
        research *= 0.5 + u;
        credits *= u;
        if u > 0.6 {
            damage = 0.0;
        }
    }
    let loot = e
        .loot
        .iter()
        .filter_map(|(r, lo, hi)| data.resource_index(r).map(|i| (i, rng.range(*lo, *hi))))
        .collect();
    Outcome {
        research: research.round(),
        credits: credits.round(),
        damage,
        loot,
        understanding,
    }
}

impl Sim {
    /// Antimatter kept in the hold (kg).
    pub fn antimatter_stored(&self) -> f64 {
        self.data
            .resource_index("antimatter")
            .and_then(|r| self.ship.cargo.kg.get(r).copied())
            .unwrap_or(0.0)
    }

    /// Whether a powered magnetic trap holds the antimatter.
    pub fn trap_powered(&self) -> bool {
        self.ship
            .design
            .modules(&self.data)
            .any(|(i, m)| m.category == "containment" && self.ship.sys.powered[i])
    }

    /// Antimatter leaks out of an unpowered trap and annihilates against the hull.
    pub(crate) fn tick_antimatter(&mut self, dt: f64) {
        let stored = self.antimatter_stored();
        if stored <= 0.0 || self.trap_powered() {
            return;
        }
        let Some(r) = self.data.resource_index("antimatter") else {
            return;
        };
        let lost = stored * (1.0 - (-TRAP_LEAK_PER_S * dt).exp());
        let data = std::sync::Arc::clone(&self.data);
        let taken = self.ship.cargo.remove(&data, r, lost);
        let dmg = breach_damage(taken);
        if dmg > 0.0 {
            self.damage_player(dmg);
            self.events.push(Event::MarketNews(
                "ALERTE : FUITE D'ANTIMATIERE, ANNIHILATION CONTRE LA COQUE".to_string(),
            ));
        }
    }

    /// The exotic phenomenon (if any) behind anomaly `body` of this system: about one anomaly in
    /// three hides one; precursor systems favour the ruins of the ancients.
    pub fn exotic_at(&self, body: usize) -> Option<usize> {
        let h = hash_coord(self.system.seed ^ 0xE0, body as i64, 3, 9);
        let unit = (h >> 11) as f64 / (1u64 << 53) as f64;
        if unit >= 0.34 {
            return None;
        }
        let precursor = is_precursor_system(self.galaxy.seed, self.here_pos);
        let mut pool: Vec<usize> = (0..self.data.exotics.len())
            .filter(|&i| self.data.exotics[i].kind != "fermi")
            .collect();
        if precursor && unit < 0.2 {
            pool = of_kind(&self.data, "precursor");
        }
        (!pool.is_empty()).then(|| pool[((h >> 3) % pool.len() as u64) as usize])
    }

    /// Study the exotic phenomenon of a freshly explored anomaly; returns the extra loot.
    pub(crate) fn study_exotic(&mut self, body: usize) -> Vec<(usize, f64)> {
        let Some(i) = self.exotic_at(body) else {
            return Vec::new();
        };
        let e = self.data.exotics[i].clone();
        let mut rng = Rng::new(hash_coord(self.system.seed ^ 0xE1, body as i64, 4, 4));
        let role = crate::crew::Crew::role_index(&self.data, "scientist");
        let tr = self.crew.role_efficiency(&self.data, role, self.time);
        let precursor = is_precursor_system(self.galaxy.seed, self.here_pos);
        let o = study(
            &self.data,
            &e,
            &mut rng,
            self.multiverse.paradox,
            precursor,
            tr,
        );
        self.research_pool += o.research;
        self.credits += o.credits;
        self.damage_player(o.damage);
        let mut text = format!("PHENOMENE EXOTIQUE : {}.", e.name.to_uppercase());
        if let Some(u) = o.understanding {
            text += &format!(" Comprehension mutuelle {:.0} %.", u * 100.0);
            if u < 0.3 {
                text += " Malentendu : l'entite l'a pris pour une menace.";
            }
        }
        if e.kind == "precursor" {
            self.multiverse.exotic_clues += 1;
            text += &format!(
                " Indice sur le silence de la galaxie {}/{}.",
                self.multiverse.exotic_clues.min(CLUES_NEEDED),
                CLUES_NEEDED
            );
        }
        self.crew.log_event(self.time, text);
        if e.kind == "precursor" && self.multiverse.exotic_clues == CLUES_NEEDED {
            if let Some(t) = fermi_truth(&self.data, self.galaxy.seed) {
                let t = &self.data.exotics[t];
                self.crew.log_event(
                    self.time,
                    format!(
                        "PARADOXE DE FERMI RESOLU : {}. {}",
                        t.name.to_uppercase(),
                        t.text
                    ),
                );
                self.research_pool += 500.0;
            }
        }
        o.loot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_world::galaxy::Galaxy;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn sim() -> Sim {
        let g = Galaxy::new(8);
        let e = g.nearby([0.0; 3], 100.0).into_iter().next().unwrap();
        Sim::new_in_galaxy(Galaxy::new(8), &e, data())
    }

    #[test]
    fn one_gram_of_antimatter_releases_exactly_two_m_c2() {
        let e = annihilation_energy_j(0.001);
        assert!((e - 2.0 * 0.001 * 299_792_458.0_f64.powi(2)).abs() < 1.0);
        assert!((e - 1.7975e14).abs() / e < 1e-3, "{e}");
        assert_eq!(annihilation_energy_j(-5.0), 0.0);
        assert!((annihilation_energy_j(2.0) - 2.0 * annihilation_energy_j(1.0)).abs() < 1.0);
    }

    #[test]
    fn warp_cost_grows_faster_than_the_speed_and_with_the_mass() {
        let m = 1.0e5;
        let (a, b, c) = (
            warp_energy_j(m, 1.0),
            warp_energy_j(m, 2.0),
            warp_energy_j(m, 4.0),
        );
        assert!(a < b && b < c);
        assert!(b > 4.0 * a, "super-quadratic: {a} {b}");
        assert!(warp_energy_j(2.0 * m, 2.0) > warp_energy_j(m, 2.0));
        assert!(warp_wake_dose_gy(10.0) > warp_wake_dose_gy(2.0));
        assert_eq!(warp_energy_j(m, 0.0), 0.0);
    }

    #[test]
    fn fermi_truth_and_ruins_are_deterministic_and_vary_with_the_seed() {
        let d = data();
        assert_eq!(fermi_truth(&d, 5), fermi_truth(&d, 5));
        let all: std::collections::HashSet<_> =
            (0..40).filter_map(|s| fermi_truth(&d, s)).collect();
        assert!(all.len() >= 3, "{} distinct answers", all.len());
        let n = (0..2000)
            .filter(|&i| is_precursor_system(9, [i as f64 * 1.3, 2.0, -4.0]))
            .count();
        assert!((250..550).contains(&n), "{n} of 2000");
    }

    #[test]
    fn contact_needs_translation_and_strangeness_hurts() {
        assert!(contact_understanding(1.5, 0.1, 0.5) > contact_understanding(0.2, 0.1, 0.5));
        assert!(contact_understanding(1.0, 0.9, 0.5) < contact_understanding(1.0, 0.1, 0.5));
        assert!((0.0..=1.0).contains(&contact_understanding(9.0, 0.0, 1.0)));
    }

    #[test]
    fn studying_is_deterministic_and_paradoxes_raise_reward_and_danger() {
        let d = data();
        let e = d.exotics.iter().find(|e| e.id == "cosmic_string").unwrap();
        let go = |p| study(&d, e, &mut Rng::new(3), p, false, 1.0);
        let (a, b) = (go(0), go(8));
        assert_eq!(go(0).research, a.research);
        assert!(b.research > a.research && b.damage > a.damage);
        let ruin = d.exotics.iter().find(|e| e.kind == "precursor").unwrap();
        let plain = study(&d, ruin, &mut Rng::new(3), 0, false, 1.0);
        let touched = study(&d, ruin, &mut Rng::new(3), 0, true, 1.0);
        assert!(touched.research > 1.9 * plain.research);
    }

    #[test]
    fn an_unpowered_trap_lets_antimatter_eat_the_hull() {
        let mut s = sim();
        let r = s.data.resource_index("antimatter").unwrap();
        let data = Arc::clone(&s.data);
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.ship.cargo.add(&data, r, 5.0);
        assert!(!s.trap_powered(), "no trap installed");
        let before = s.ship.hull;
        for _ in 0..30 {
            s.tick_antimatter(1.0);
        }
        assert!(s.antimatter_stored() < 5.0, "antimatter leaked");
        assert!(s.ship.hull < before, "the hull took the annihilation");
    }

    #[test]
    fn exploring_precursor_ruins_collects_clues_until_the_silence_is_explained() {
        let mut s = sim();
        let log0 = s.crew.log.len();
        s.multiverse.exotic_clues = CLUES_NEEDED - 1;
        let b = (0..5000)
            .find(|&b| {
                s.exotic_at(b)
                    .is_some_and(|i| s.data.exotics[i].kind == "precursor")
            })
            .expect("some anomaly hides ruins");
        s.study_exotic(b);
        assert_eq!(s.multiverse.exotic_clues, CLUES_NEEDED);
        assert!(s.crew.log.len() >= log0 + 2, "clue and revelation logged");
    }
}
