//! Creative mode: the single API behind the creative panel and the console. Every action
//! refuses when `Rules::creative` is off and reuses the normal Sim state, so nothing is duplicated;
//! restrictions in the other systems are lifted through `Rules` accessors (`free`, `invuln`...).
use crate::rules::CreativeFlags;
use crate::Sim;
use vector_core::Vec3;

pub type CxResult = Result<String, String>;

impl Sim {
    pub fn creative_check(&self) -> Result<(), String> {
        if self.rules.creative {
            Ok(())
        } else {
            Err("MODE CREATIF REQUIS".into())
        }
    }

    /// Turn creative mode on mid-game: the game stays saveable and is marked `creative`.
    pub fn creative_enable(&mut self, on: bool) {
        self.rules.creative = on;
    }

    pub fn creative_set_flag(&mut self, i: usize, on: bool) -> CxResult {
        self.creative_check()?;
        let f = self.rules.cx.get_mut(i).ok_or("drapeau inconnu")?;
        *f = on;
        if i == 8 && on {
            self.creative_reveal_map()?;
        }
        if i == 7 && on {
            self.creative_unlock_all_tech()?;
        }
        Ok(format!(
            "{}: {}",
            CreativeFlags::name(i),
            if on { "OUI" } else { "NON" }
        ))
    }

    /// Put the ship next to body `i` of the system, at rest relative to it.
    pub fn creative_teleport_body(&mut self, i: usize) -> CxResult {
        self.creative_check()?;
        let b = self.bodies.get(i).ok_or("corps inconnu")?;
        let d = b.radius * 3.0 + 200.0;
        self.docked = None;
        self.spawn_near(i, d);
        Ok("teleporte".into())
    }

    pub fn creative_teleport_pos(&mut self, p: Vec3) -> CxResult {
        self.creative_check()?;
        self.docked = None;
        self.ship.pos = p;
        self.update_ref();
        Ok("teleporte".into())
    }

    pub fn creative_give_credits(&mut self, amount: f64) -> CxResult {
        self.creative_check()?;
        self.credits = (self.credits + amount).max(0.0);
        Ok(format!("credits {:.0}", self.credits))
    }

    /// Fill fuel, repair the hull and modules, heal and feed the crew.
    pub fn creative_full_service(&mut self) -> CxResult {
        self.creative_check()?;
        self.ship.fuel = self.ship.stats.fuel_cap;
        self.ship.hull = self.ship.stats.hull_max;
        self.ship.sys.health.fill(1.0);
        let data = std::sync::Arc::clone(&self.data);
        self.ship.refresh_stats(&data);
        self.ship.fuel = self.ship.stats.fuel_cap;
        self.ship.hull = self.ship.stats.hull_max;
        for p in self.crew.members.iter_mut().filter(|p| p.alive) {
            p.health = 1.0;
            p.hunger = 0.0;
            p.thirst = 0.0;
            p.fatigue = 0.0;
            p.stress = 0.0;
            p.morale = 1.0;
            p.body.pending_dose = 0.0;
            p.body.acute_dose = 0.0;
            p.body.wounds.clear();
        }
        Ok("vaisseau et equipage remis a neuf".into())
    }

    /// Fill the hold with `kg` of every resource that fits.
    pub fn creative_fill_cargo(&mut self, kg: f64) -> CxResult {
        self.creative_check()?;
        let data = std::sync::Arc::clone(&self.data);
        for r in 0..data.resources.len() {
            self.ship.cargo.add(&data, r, kg);
        }
        Ok("soute remplie".into())
    }

    pub fn creative_unlock_all_tech(&mut self) -> CxResult {
        self.creative_check()?;
        self.researched.fill(true);
        Ok("toutes les technologies".into())
    }

    pub fn creative_reveal_map(&mut self) -> CxResult {
        self.creative_check()?;
        for l in &mut self.scan {
            *l = (*l).max(crate::voyage::SCAN_SURVEYED);
        }
        Ok("carte revelee".into())
    }

    pub fn creative_set_all_reputation(&mut self, rep: f32) -> CxResult {
        self.creative_check()?;
        for f in 0..self.data.factions.len() {
            self.set_reputation(f, rep);
        }
        Ok(format!("relations {rep:.0}"))
    }

    /// Bring every dead crew member back (not those lost to age of records, only flags).
    pub fn creative_revive_crew(&mut self) -> CxResult {
        self.creative_check()?;
        let mut n = 0;
        for p in self.crew.members.iter_mut().filter(|p| !p.alive) {
            p.alive = true;
            p.health = 1.0;
            n += 1;
        }
        Ok(format!("{n} ressuscites"))
    }

    /// Let `days` days pass at once (the normal step loop, so everything evolves).
    pub fn creative_advance_days(&mut self, days: f64) -> CxResult {
        self.creative_check()?;
        self.advance(days * 86_400.0, &crate::ShipInput::default());
        Ok(format!("+{days:.0} jour(s)"))
    }

    /// Found a colony on body `i` at once (the normal founding rules and costs, lifted by the flags).
    pub fn creative_found_colony(&mut self, i: usize) -> CxResult {
        self.creative_check()?;
        let was = std::mem::replace(&mut self.rules.cx.free_build, true);
        let r = self.found_colony(i, None);
        self.rules.cx.free_build = was;
        r.map(|c| format!("colonie {c} fondee"))
            .map_err(String::from)
    }

    pub fn creative_stop(&mut self) -> CxResult {
        self.creative_check()?;
        self.ship.vel = Vec3::ZERO;
        self.ship.ang = Vec3::ZERO;
        Ok("arret".into())
    }
}

#[cfg(test)]
mod tests {
    use crate::rules::Rules;

    fn sim() -> crate::Sim {
        {
            let g = vector_world::galaxy::Galaxy::new(12);
            let e = g.nearby([0.0; 3], 400.0).into_iter().next().expect("star");
            crate::Sim::new_in_galaxy(g, &e, crate::test_support::data())
        }
    }

    #[test]
    fn off_refuses_and_on_applies() {
        let mut s = sim();
        let c0 = s.credits;
        assert!(s.creative_give_credits(1e6).is_err());
        assert!(s.creative_set_flag(1, true).is_err());
        assert_eq!(s.credits, c0);
        s.creative_enable(true);
        s.creative_give_credits(1e6).unwrap();
        assert_eq!(s.credits, c0 + 1e6);
    }

    #[test]
    fn flags_have_measurable_effects() {
        let mut s = sim();
        s.creative_enable(true);
        assert!(!s.rules.free() && s.rules.needs() && !s.is_wanted() == !s.is_wanted());
        s.creative_set_flag(1, true).unwrap();
        s.creative_set_flag(3, true).unwrap();
        s.creative_set_flag(4, true).unwrap();
        s.creative_set_flag(0, true).unwrap();
        assert!(s.rules.free() && !s.rules.needs() && s.rules.invuln());
        assert_eq!(s.rules.fuel_mult(), 0.0);
        let h = s.ship.hull;
        s.hurt_ship(1e9, 0);
        assert_eq!(s.ship.hull, h, "invulnerable");
        s.creative_set_flag(6, true).unwrap();
        assert!(!s.is_wanted());
    }

    #[test]
    fn all_tech_teleport_and_save_roundtrip() {
        let mut s = sim();
        s.creative_enable(true);
        s.creative_set_flag(7, true).unwrap();
        assert!(s.researched.iter().all(|&r| r));
        s.creative_teleport_pos(vector_core::Vec3::new(5.0, 6.0, 7.0))
            .unwrap();
        assert_eq!(s.ship.pos, vector_core::Vec3::new(5.0, 6.0, 7.0));
        s.creative_teleport_body(0).unwrap();
        let r = Rules {
            creative: true,
            ..Default::default()
        };
        let js = postcard::to_allocvec(&r).unwrap();
        let back: Rules = postcard::from_bytes(&js).unwrap();
        assert_eq!(back, r);
    }
}
