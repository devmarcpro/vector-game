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

    /// A ship of `role` 2.5 km ahead, targeted (hostile ones attack at once).
    pub fn creative_spawn_npc(&mut self, role: crate::npc::NpcRole, hostile: bool) -> CxResult {
        self.creative_check()?;
        let id = self.next_npc;
        self.next_npc += 1;
        let mut rng = vector_core::Rng::new(id as u64 ^ 0xC0FFEE);
        let name = self.npc_name(role, &mut rng, self.faction);
        let stats = crate::combat::npc_stats(role, 2);
        let ahead = self.ship.rot.col(2) * 2500.0;
        let state = if hostile {
            crate::npc::NpcState::Combat {
                mode: crate::npc::AiMode::Pursue,
            }
        } else {
            crate::npc::NpcState::Orbit {
                around: self.ref_body.unwrap_or(0),
                phase: 0.0,
            }
        };
        self.npcs.push(crate::npc::Npc {
            id,
            role,
            name: name.clone(),
            faction: self.faction,
            pos: self.ship.pos + ahead,
            vel: self.ship.vel,
            state,
            cargo: None,
            last_scan: 0.0,
            home: 0,
            hull: stats.hull,
            hull_max: stats.hull,
            shield: stats.shield,
            shield_max: stats.shield,
            level: 2,
            hostile,
            disabled: false,
            provoked: hostile,
            hail_deadline: None,
        });
        self.target_npc = Some(self.npcs.len() - 1);
        Ok(format!("{name} apparait"))
    }

    pub fn creative_clear_npcs(&mut self) -> CxResult {
        self.creative_check()?;
        let n = self.npcs.len();
        self.npcs.clear();
        self.target_npc = None;
        Ok(format!("{n} vaisseaux supprimes"))
    }

    /// Recruit candidate `k` of an imaginary pool: no station, no cost, no berth limit.
    pub fn creative_recruit_any(&mut self, k: usize) -> CxResult {
        self.creative_check()?;
        let pool =
            crate::crew::candidates(&self.data, self.system.seed, 0, self.time, 6, &self.crew);
        let p = pool.into_iter().nth(k % 6).ok_or("aucun candidat")?;
        if self
            .crew
            .members
            .iter()
            .any(|m| m.alive && m.name == p.name)
        {
            return Err("deja a bord".into());
        }
        let (name, role) = (p.name.clone(), p.role);
        self.crew.hire(&self.data, p, role, self.time);
        Ok(format!("{name} rejoint l'equipage"))
    }

    /// Edit one crew member: field 0 skills to 10, 1 health, 2 age by `delta` years, 3 mood.
    pub fn creative_edit_crew(&mut self, idx: usize, field: u8, delta: f32) -> CxResult {
        self.creative_check()?;
        let p = self.crew.members.get_mut(idx).ok_or("membre inconnu")?;
        match field {
            0 => p
                .skills
                .iter_mut()
                .for_each(|s| *s = (*s + delta).clamp(0.0, 10.0)),
            1 => p.health = (p.health + delta).clamp(0.0, 1.0),
            2 => p.age = (p.age + delta).max(0.0),
            _ => {
                p.morale = 1.0;
                p.stress = 0.0;
                p.loyalty = 1.0;
            }
        }
        Ok(format!("{} modifie", p.name))
    }

    /// Rewind to anchor `i` for nothing (and set anchors for nothing).
    pub fn creative_rewind(&mut self, i: usize) -> CxResult {
        self.creative_check()?;
        let was = std::mem::replace(&mut self.rules.cx.free_build, true);
        let r = self.rewind(i).map_err(String::from);
        self.rules.cx.free_build = was;
        r
    }

    pub fn creative_set_anchor(&mut self) -> CxResult {
        self.creative_check()?;
        let was = std::mem::replace(&mut self.rules.cx.free_build, true);
        let r = self.set_anchor();
        self.rules.cx.free_build = was;
        r.map(|_| "ancre posee".to_string()).map_err(String::from)
    }

    /// A ship of hull `hull` joins the fleet at once.
    pub fn creative_add_fleet_ship(&mut self, hull: usize) -> CxResult {
        self.creative_check()?;
        let was = std::mem::replace(&mut self.rules.cx.free_build, true);
        let r = self.buy_fleet_ship(hull);
        self.rules.cx.free_build = was;
        r.map(|id| format!("vaisseau {id} ajoute"))
            .map_err(String::from)
    }

    /// A stellar flare in ten seconds of the given severity (0.2..1).
    pub fn creative_flare(&mut self, severity: f64) -> CxResult {
        self.creative_check()?;
        self.pending_flare = Some((self.time + 10.0, severity.clamp(0.2, 1.0), false));
        Ok("eruption stellaire imminente".into())
    }

    /// Jump to the `k`-th nearest star (0 = nearest other than here): no range, no fuel.
    pub fn creative_jump(&mut self, k: usize) -> CxResult {
        self.creative_check()?;
        let here = self.here;
        let list: Vec<_> = self
            .galaxy
            .nearby(self.here_pos, 60.0)
            .into_iter()
            .filter(|e| e.id != here)
            .collect();
        let target = list.get(k).ok_or("aucune etoile")?.clone();
        self.docked = None;
        self.jump = None;
        let was = std::mem::replace(&mut self.rules.cx.free_travel, true);
        let r = self.start_jump(&target);
        self.rules.cx.free_travel = was;
        r.map(|_| "saut engage".to_string()).map_err(String::from)
    }

    /// Jump to the `k`-th nearest galaxy not visited yet (0 = nearest): no range, no fuel, no drive.
    pub fn creative_galaxy(&mut self, k: usize) -> CxResult {
        self.creative_check()?;
        let uni = self.universe();
        let here = uni.galaxy(self.galaxy_id).map_or([0.0; 3], |g| g.pos);
        let mut list: Vec<_> = uni
            .galaxies_near(here, 9.0)
            .into_iter()
            .filter(|g| g.id != self.galaxy_id && !self.galaxy_stash.contains_key(&g.id))
            .collect();
        list.sort_by(|a, b| {
            let d = |p: [f64; 3]| (0..3).map(|i| (p[i] - here[i]).powi(2)).sum::<f64>();
            d(a.pos).total_cmp(&d(b.pos)).then(a.id.cmp(&b.id))
        });
        let g = list.get(k).ok_or("aucune galaxie")?.id;
        let entry = self.galaxy_entry_star(g).ok_or("galaxie vide")?;
        self.docked = None;
        self.jump = None;
        let was = std::mem::replace(&mut self.rules.cx.free_travel, true);
        let r = self.start_jump_to(&entry, 0.0, Some(g));
        self.rules.cx.free_travel = was;
        r.map(|_| "saut intergalactique engage".to_string())
            .map_err(String::from)
    }

    pub fn creative_delete_npc(&mut self, k: usize) -> CxResult {
        self.creative_check()?;
        if k >= self.npcs.len() {
            return Err("vaisseau inconnu".into());
        }
        self.npcs.remove(k);
        self.target_npc = None;
        Ok("supprime".into())
    }

    pub fn creative_duplicate_npc(&mut self, k: usize) -> CxResult {
        self.creative_check()?;
        let mut n = self.npcs.get(k).ok_or("vaisseau inconnu")?.clone();
        n.id = self.next_npc;
        self.next_npc += 1;
        n.pos = n.pos + Vec3::new(300.0, 0.0, 0.0);
        self.npcs.push(n);
        Ok("duplique".into())
    }

    /// Bring NPC `k` 2.5 km ahead of the ship, repaired.
    pub fn creative_move_npc(&mut self, k: usize) -> CxResult {
        self.creative_check()?;
        let (pos, vel, ahead) = (self.ship.pos, self.ship.vel, self.ship.rot.col(2) * 2500.0);
        let n = self.npcs.get_mut(k).ok_or("vaisseau inconnu")?;
        n.pos = pos + ahead;
        n.vel = vel;
        n.hull = n.hull_max;
        n.shield = n.shield_max;
        n.disabled = false;
        Ok("deplace et repare".into())
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
    fn creative_galaxy_hop_changes_galaxy_and_refuses_when_off() {
        let mut s = sim();
        assert!(s.creative_galaxy(0).is_err());
        s.creative_enable(true);
        let before = s.galaxy_id;
        s.creative_galaxy(0).unwrap();
        for _ in 0..4000 {
            if s.jump.is_none() {
                break;
            }
            s.advance(1000.0, &crate::ShipInput::default());
        }
        assert!(s.jump.is_none(), "the jump finished");
        assert_ne!(s.galaxy_id, before, "we are in another galaxy");
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

    #[test]
    fn effects_of_each_remaining_flag() {
        use crate::npc::NpcRole;
        let mut s = sim();
        assert!(s.creative_spawn_npc(NpcRole::Pirate, true).is_err());
        s.creative_enable(true);
        // one_hit_kill
        s.creative_spawn_npc(NpcRole::Pirate, true).unwrap();
        let k = s.npcs.len() - 1;
        let n = s.npcs.len();
        s.creative_set_flag(11, true).unwrap();
        assert!(s.damage_npc(k, 1.0), "destroyed by a scratch");
        assert_eq!(s.npcs.len(), n - 1);
        s.creative_set_flag(11, false).unwrap();
        // delete / duplicate
        s.creative_spawn_npc(NpcRole::Trader, false).unwrap();
        let c = s.npcs.len();
        s.creative_duplicate_npc(c - 1).unwrap();
        assert_eq!(s.npcs.len(), c + 1);
        s.creative_delete_npc(0).unwrap();
        assert_eq!(s.npcs.len(), c);
        // all_knowledge: every language is understood
        s.creative_recruit_any(1).unwrap();
        let p = s.crew.members[0].id;
        s.creative_set_flag(8, true).unwrap();
        assert_eq!(s.fluency(p, 0), 1.0);
        // max timescale lifted, travel free
        assert_eq!(s.rules.max_ts(), 1_000_000.0);
        let normal = crate::rules::Rules::default();
        assert_eq!(normal.max_ts(), normal.max_timescale);
        // recruit
        let n = s.crew.members.len();
        s.creative_recruit_any(0).unwrap();
        assert!(s.crew.members.len() >= n);
        // fleet ship from nowhere
        let f = s.fleet.len();
        s.docked = None;
        s.creative_add_fleet_ship(0).unwrap();
        assert_eq!(s.fleet.len(), f + 1);
        // flare
        s.creative_flare(1.0).unwrap();
        assert!(s.pending_flare.is_some());
        // anchor and rewind cost nothing
        s.credits = 0.0;
        s.creative_set_anchor().unwrap();
        s.creative_rewind(0).unwrap();
    }

    #[test]
    fn fly_free_multiplies_thrust_and_ghost_is_a_flag() {
        let mut a = sim();
        let mut b = sim();
        b.creative_enable(true);
        b.creative_set_flag(9, true).unwrap();
        let input = crate::ShipInput {
            fwd: 1.0,
            ..Default::default()
        };
        for s in [&mut a, &mut b] {
            s.ship.vel = vector_core::Vec3::ZERO;
            s.ship.assist = false;
            s.docked = None;
            s.ship.pos = vector_core::Vec3::new(0.0, 0.0, 1.0e13);
            s.advance(10.0, &input);
        }
        assert!(
            b.ship.vel.length() > 5.0 * a.ship.vel.length(),
            "{} vs {}",
            b.ship.vel.length(),
            a.ship.vel.length()
        );
        assert!(!a.rules.ghost());
        b.creative_set_flag(10, true).unwrap();
        assert!(b.rules.ghost());
    }
}
