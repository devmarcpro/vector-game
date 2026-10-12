//! Astronomy in play: the medium between the stars and the nucleus of the galaxy (see
//! `vector_world::cosmos`) act on the system where the ship is, and the gravitational waves of
//! distant mergers reach its gravimeters. The stellar side (pulsars, black holes, companions,
//! comets) is applied in `science.rs` (environment, flares) and `lib.rs` (solar flux).
use crate::{Event, Sim};
use vector_world::cosmos::CosmicEffects;

/// Research points per unit of (strain / threshold)^0.5, at most `GW_MAX_POINTS`.
pub const GW_POINTS: f64 = 15.0;
pub const GW_MAX_POINTS: f64 = 150.0;

impl Sim {
    /// Effects of the medium and of the nucleus at this system (cached while the ship stays).
    pub fn cosmic_effects(&self) -> CosmicEffects {
        match self.cosmic_cache {
            Some((star, e)) if star == self.here => e,
            _ => self.galaxy.cosmic_effects(self.here_pos),
        }
    }

    /// Per-step upkeep: refresh the cache when the ship has changed system, and once a day listen
    /// for gravitational waves (at most the last three days when time was skipped).
    pub(crate) fn cosmic_tick(&mut self) {
        if self.cosmic_cache.is_none_or(|(s, _)| s != self.here) {
            self.cosmic_cache = Some((self.here, self.galaxy.cosmic_effects(self.here_pos)));
        }
        let day = (self.time / crate::crew::DAY).floor() as i64;
        let last = self.last_cosmic_day;
        self.last_cosmic_day = day;
        if last == i64::MIN || day <= last {
            return;
        }
        let lab = self.ship_research_rate() > 0.0;
        for e in self
            .galaxy
            .gw_heard(self.here_pos, (last + 1).max(day - 2), day)
        {
            let h = e.strain_at(self.here_pos);
            let d = ((self.here_pos[0] - e.pos[0]).powi(2)
                + (self.here_pos[1] - e.pos[1]).powi(2)
                + (self.here_pos[2] - e.pos[2]).powi(2))
            .sqrt();
            let pts = ((h / vector_world::cosmos::GW_THRESHOLD).sqrt() * GW_POINTS)
                .min(GW_MAX_POINTS)
                * if e.kilonova { 1.5 } else { 1.0 };
            let what = if e.kilonova {
                "ETOILES A NEUTRONS"
            } else {
                "TROUS NOIRS"
            };
            let tail = if lab {
                self.research_pool += pts;
                format!("+{pts:.0} POINTS DE RECHERCHE")
            } else {
                "AUCUN LABORATOIRE POUR L'ANALYSER".to_string()
            };
            self.events.push(Event::MarketNews(format!(
                "ONDES GRAVITATIONNELLES : FUSION DE {:.0} ET {:.0} MASSES SOLAIRES ({what}), A {d:.0} AL, DEFORMATION {h:.0e}. {tail}",
                e.m1, e.m2
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::DAY;
    use vector_core::Vec3;
    use vector_world::orbit::AU;
    use vector_world::stellar::StarStage;
    use vector_world::BodyKind;

    fn data() -> std::sync::Arc<vector_data::GameData> {
        std::sync::Arc::new(
            vector_data::GameData::load(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            )
            .expect("data"),
        )
    }

    /// A sim in the first system (searching seeds) whose star is at this stage, reduced to the
    /// star alone when `alone` (so that nothing else adds to the fields measured).
    fn sim_of(stage: StarStage, alone: bool) -> Sim {
        for k in 0..60_000u64 {
            let seed = k.wrapping_mul(0x9E37_79B9);
            if vector_world::peek_star(seed).stage != stage {
                continue;
            }
            let mut sys = vector_world::generate_system(seed);
            if alone {
                sys.bodies.truncate(1);
                sys.belts.clear();
            }
            return Sim::new(sys, data());
        }
        panic!("no star at stage {stage:?}");
    }

    fn at(z: f64) -> Vec3 {
        Vec3::new(0.0, 0.0, z)
    }

    #[test]
    fn a_pulsar_irradiates_as_the_inverse_square_and_an_ordinary_star_does_not() {
        let s = sim_of(StarStage::NeutronStar, true);
        let (near, far) = (
            s.environment_at(at(0.1 * AU)),
            s.environment_at(at(0.2 * AU)),
        );
        assert!(near.radiation > 10.0, "{}", near.radiation);
        assert!((near.radiation / far.radiation - 4.0).abs() < 0.05);
        let sun = sim_of(StarStage::MainSequence, true);
        assert_eq!(sun.environment_at(at(0.1 * AU)).radiation, 0.0);
    }

    #[test]
    fn a_black_hole_focuses_the_sensors_at_its_focal_distance_only() {
        let s = sim_of(StarStage::BlackHole, true);
        let f = s.system.lens_focus();
        let at_focus = s.environment_at(at(f)).sensor_mult;
        assert!(at_focus > 2.5, "{at_focus}");
        assert!(s.environment_at(at(f * 10.0)).sensor_mult < 1.01);
        assert!(s.environment_at(at(f * 0.01)).sensor_mult < 1.01);
        // A disc-fed hole also burns.
        let hot = (0..40_000u64)
            .map(|k| k.wrapping_mul(0x9E37_79B9))
            .filter(|&seed| vector_world::peek_star(seed).stage == StarStage::BlackHole)
            .map(vector_world::generate_system)
            .find(|sys| sys.accreting())
            .expect("an accreting hole");
        let mut hot = hot;
        hot.bodies.truncate(1);
        hot.belts.clear();
        let hs = Sim::new(hot, data());
        assert!(hs.environment_at(at(0.05 * AU)).radiation > 0.1);
    }

    #[test]
    fn the_ion_tail_of_a_comet_hurts_electronics_on_its_axis_only() {
        let (mut s, i) = (0..40_000u64)
            .map(|k| k.wrapping_mul(0x9E37_79B9))
            .map(vector_world::generate_system)
            .find_map(|sys| {
                sys.bodies
                    .iter()
                    .position(|b| b.kind == BodyKind::Anomaly(vector_world::AnomalyKind::Comet))
                    .map(|i| (Sim::new(sys, data()), i))
            })
            .expect("a comet");
        // Wait until the comet is near enough to the star to have a tail.
        let period = s.system.bodies[i].orbit.as_ref().expect("orbit").period();
        let t = (0..2000)
            .map(|k| f64::from(k) * period / 2000.0)
            .find(|&t| s.system.comet_tail(i, t).is_some())
            .expect("a tail");
        s.time = t;
        s.refresh_bodies();
        let tail = s.system.comet_tail(i, t).expect("tail");
        let head = s.bodies[i].pos;
        let on_axis = s.environment_at(head + tail * 0.5).ion;
        let off_axis = s
            .environment_at(head + tail * 0.5 + Vec3::new(0.0, 3.0e6, 0.0))
            .ion;
        assert!(
            on_axis - off_axis > 0.5 * crate::science::COMET_TAIL_ION,
            "{on_axis} vs {off_axis}"
        );
        // Far from the star, in the dormant part of the orbit, the comet has no tail and no plasma.
        let far = (0..2000)
            .map(|k| f64::from(k) * period / 2000.0)
            .find(|&t| s.system.comet_tail(i, t).is_none())
            .expect("dormant");
        s.time = far;
        s.refresh_bodies();
        let p = s.bodies[i].pos;
        assert!(s.environment_at(p).ion - off_axis < 0.5 * crate::science::COMET_TAIL_ION);
    }

    #[test]
    fn variable_stars_vary_the_solar_power_and_companions_add_light() {
        let mut s = sim_of(StarStage::RedGiant, true);
        let v = s.system.variable().expect("variable");
        s.ship.pos = at(10.0 * AU * s.system.luminosity.sqrt());
        let period = v.period_days * 86_400.0;
        let flux: Vec<f64> = (0..40)
            .map(|k| {
                s.time = f64::from(k) * period / 40.0;
                s.solar_flux()
            })
            .collect();
        let (lo, hi) = flux
            .iter()
            .fold((f64::MAX, 0.0f64), |(l, h), &f| (l.min(f), h.max(f)));
        assert!(
            hi / lo > 1.0 + v.amplitude,
            "the panels follow the star: {lo} {hi}"
        );
        // Next to a companion sun, the light is far stronger than the primary alone gives.
        let mut b = (0..30_000u64)
            .map(|k| k.wrapping_mul(0x9E37_79B9))
            .map(vector_world::generate_system)
            .find(|sys| !sys.companions().is_empty() && sys.stage == StarStage::MainSequence)
            .map(|sys| Sim::new(sys, data()))
            .expect("a binary");
        let c = b.system.companions()[0];
        b.ship.pos = b.bodies[c].pos + Vec3::new(0.0, 0.0, 2.0e10);
        let alone = b.system.luminosity_at(b.time) / (b.ship.pos.length() / AU).powi(2);
        assert!(
            b.solar_flux() > alone * 1.5,
            "{} vs {alone}",
            b.solar_flux()
        );
    }

    #[test]
    fn black_holes_and_rogue_worlds_never_flare_and_a_red_dwarf_does() {
        let count = |mut s: Sim| {
            let mut n = 0;
            for day in 0..500 {
                s.time = f64::from(day) * DAY + 5.0;
                s.hazard_tick(0.1);
                n += usize::from(s.pending_flare.is_some());
                s.pending_flare = None;
            }
            n
        };
        assert_eq!(count(sim_of(StarStage::BlackHole, true)), 0);
        assert_eq!(count(sim_of(StarStage::Rogue, true)), 0);
        let red = (0..20_000u64)
            .map(|k| k.wrapping_mul(0x9E37_79B9))
            .find(|&seed| vector_world::peek_star(seed).class == vector_world::StarClass::M)
            .expect("a red dwarf");
        let mut sys = vector_world::generate_system(red);
        sys.bodies.truncate(1);
        sys.belts.clear();
        assert!(count(Sim::new(sys, data())) > 3);
    }

    fn sim_at(pos: [f64; 3]) -> Sim {
        let mut s = Sim::new(vector_world::generate_system(1), data());
        s.here_pos = pos;
        s
    }

    #[test]
    fn the_cache_follows_the_ship_and_empty_space_is_clear() {
        let mut s = sim_at([0.0, 5000.0, 0.0]);
        s.cosmic_tick();
        assert_eq!(s.cosmic_effects(), CosmicEffects::default());
        // Move into the heart of a remnant: the effects change as soon as the tick runs.
        let g = s.galaxy;
        let f = (-80..80)
            .flat_map(|x| (-80..80).map(move |z| (x, z)))
            .flat_map(|(x, z)| g.features_in_cell((x, 0, z)))
            .find(|f| f.kind == vector_world::cosmos::FeatureKind::SupernovaRemnant)
            .expect("a remnant");
        s.here_pos = f.pos;
        s.here = vector_world::galaxy::StarRef {
            sector: (999, 999, 999),
            index: 0,
        };
        s.cosmic_tick();
        assert!(s.cosmic_effects().radiation > 10.0);
    }

    #[test]
    fn a_merger_heard_with_a_lab_pays_research_and_one_without_does_not() {
        let mut s = sim_at([0.0; 3]);
        let g = s.galaxy;
        // A merger a few light-years away is always heard: look for the first day that has one.
        let day = (0..400).find(|&d| g.gw_event(d).is_some()).expect("event");
        let e = g.gw_event(day).expect("event");
        s.here_pos = [e.pos[0] + 30.0, e.pos[1], e.pos[2]];
        s.here = vector_world::galaxy::StarRef {
            sector: (7777, 0, 0),
            index: 0,
        };
        s.time = (day - 1) as f64 * DAY + 10.0;
        s.cosmic_tick();
        s.events.clear();
        s.research_pool = 0.0;
        s.time = day as f64 * DAY + 10.0;
        s.cosmic_tick();
        let heard = s.events.iter().any(
            |ev| matches!(ev, Event::MarketNews(t) if t.starts_with("ONDES GRAVITATIONNELLES")),
        );
        assert!(heard, "a strong merger next door is heard");
        let lab = s.ship_research_rate() > 0.0;
        assert_eq!(
            s.research_pool > 0.0,
            lab,
            "research is paid only with a lab"
        );
        // Nothing the day after.
        s.events.clear();
        let before = s.research_pool;
        s.time = (day + 1) as f64 * DAY + 10.0;
        s.cosmic_tick();
        if g.gw_event(day + 1).is_none() {
            assert!(s.events.is_empty() && s.research_pool == before);
        }
    }
}
