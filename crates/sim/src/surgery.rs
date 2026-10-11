//! Surgery and the organ trade: transplants of grafts bought or harvested, prostheses, and
//! harvesting organs from the dead (or, at a heavy price in morale, from the living).
use crate::Sim;
use vector_core::{hash_coord, Rng};

/// Fee of the hospital of a station for one operation, credits.
pub const SURGERY_FEE: f64 = 1500.0;
/// Base chance that a graft is rejected (lowered by the surgeon's medicine skill).
pub const REJECTION: f64 = 0.2;
/// Below this value of the "corps" axis a people holds the body sacred.
pub const SACRED_BODY: f64 = -0.4;
pub const SACRILEGE_FINE: f64 = 800.0;
/// A clone costs this much of the value of the organ to grow.
pub const CLONE_COST_FACTOR: f64 = 1.2;
/// Integrity of a fresh prosthesis: never worn by disease, never mends.
pub const PROSTHESIS_INTEGRITY: f32 = 0.85;

/// Natural, grafted or artificial.
pub const KIND_NATURAL: u8 = 0;
pub const KIND_GRAFT: u8 = 1;
pub const KIND_PROSTHESIS: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Implant {
    Graft,
    Prosthesis,
}

impl Sim {
    /// Local mores: where the body is sacred (the "corps" axis very low) operating, cloning or
    /// harvesting is a sacrilege: a fine and the loss of reputation of the territory's people.
    fn moral_offense(&mut self, what: &str) {
        let Some(f) = self.faction else {
            return;
        };
        let profile = self
            .data
            .ethos
            .profile(Some(self.data.factions[f].id.as_str()));
        let corps = self
            .data
            .ethos
            .axis("corps")
            .and_then(|i| profile.get(i))
            .copied()
            .unwrap_or(0.0);
        if corps >= SACRED_BODY {
            return;
        }
        let fine = self.credits.clamp(0.0, SACRILEGE_FINE);
        self.credits -= fine;
        self.change_reputation(Some(f), -0.2);
        let people = self.data.factions[f].name.clone();
        self.events.push(crate::Event::MarketNews(format!(
            "Moeurs locales ({people}) : {what} est un sacrilege ici, amende de {fine:.0} CR."
        )));
    }

    /// Who may operate: the hospital of a docked station, or a living medic aboard with a
    /// working infirmary. Returns the surgeon's medicine skill (0..10).
    fn surgeon_skill(&self, patient: usize) -> Result<f64, &'static str> {
        if self.docked.is_some() {
            return Ok(4.0);
        }
        if self.effective.medbay <= 0.0 {
            return Err("pas d'infirmerie alimentee");
        }
        let med = self
            .data
            .skill_index("medecine")
            .ok_or("competence inconnue")?;
        self.crew
            .members
            .iter()
            .enumerate()
            .filter(|(i, p)| *i != patient && p.alive && self.data.roles[p.role].id == "medic")
            .map(|(_, p)| f64::from(p.skills[med]))
            .fold(None, |best: Option<f64>, s| {
                Some(best.map_or(s, |b| b.max(s)))
            })
            .ok_or("aucun medecin disponible")
    }

    /// Replace organ `organ` of crew member `member` with a graft or a prosthesis taken from the
    /// hold. Returns a short report.
    pub fn implant(
        &mut self,
        member: usize,
        organ: usize,
        kind: Implant,
    ) -> Result<String, &'static str> {
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le patient est mort");
        }
        let def = self.data.organs.get(organ).ok_or("organe inconnu")?;
        let prefix = match kind {
            Implant::Graft => "organ_",
            Implant::Prosthesis => "prosthesis_",
        };
        // A grown clone is preferred to a graft: it is the patient's own match, no rejection.
        let clone = self
            .data
            .resource_index(&format!("clone_{}", def.id))
            .filter(|&c| kind == Implant::Graft && self.ship.cargo.kg[c] >= 1.0);
        let item = match clone {
            Some(c) => c,
            None => self
                .data
                .resource_index(&format!("{prefix}{}", def.id))
                .ok_or("piece inconnue")?,
        };
        if self.ship.cargo.kg[item] < 1.0 {
            return Err("piece absente de la soute");
        }
        let skill = self.surgeon_skill(member)?;
        let fee = if self.docked.is_some() {
            SURGERY_FEE
        } else {
            0.0
        };
        if fee > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= fee;
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, item, 1.0);
        let (name, organ_name) = {
            let p = &self.crew.members[member];
            (p.name.clone(), data.organs[organ].name.clone())
        };
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            i64::from(self.crew.members[member].id),
            organ as i64,
            self.time as i64,
        ));
        let p = &mut self.crew.members[member];
        p.organs.resize(data.organs.len(), 1.0);
        p.organ_kind.resize(data.organs.len(), KIND_NATURAL);
        let report = match kind {
            Implant::Prosthesis => {
                p.organs[organ] = PROSTHESIS_INTEGRITY;
                p.organ_kind[organ] = KIND_PROSTHESIS;
                format!("{name} : prothese ({organ_name}) posee")
            }
            Implant::Graft => {
                p.organ_kind[organ] = KIND_GRAFT;
                let chance = if clone.is_some() {
                    0.0
                } else {
                    REJECTION * (1.0 - skill / 12.0)
                };
                if rng.f64() < chance {
                    p.organs[organ] = 0.2;
                    format!("{name} : REJET du greffon ({organ_name})")
                } else {
                    p.organs[organ] = 1.0;
                    format!("{name} : greffe reussie ({organ_name})")
                }
            }
        };
        let now = self.time;
        self.crew.log_event(now, report.clone());
        self.moral_offense("operer un corps");
        Ok(report)
    }

    /// Grow a clone of organ `organ` at the hospital of the station (it costs more than the
    /// organ is worth) and put it in the hold: a graft that is never rejected.
    pub fn grow_clone(&mut self, organ: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre : il faut un hopital")?;
        let data = std::sync::Arc::clone(&self.data);
        let def = data.organs.get(organ).ok_or("organe inconnu")?;
        let item = data
            .resource_index(&format!("clone_{}", def.id))
            .ok_or("piece inconnue")?;
        let cost = def.value * CLONE_COST_FACTOR;
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        if self.ship.cargo.add(&data, item, 1.0) < 1.0 {
            return Err("soute pleine");
        }
        self.credits -= cost;
        self.moral_offense("cloner un organe");
        Ok(cost)
    }

    /// Take organ `organ` out of crew member `member` into the hold as a graft. Allowed on the
    /// dead, or on a living person for a non-vital organ (everybody then remembers it).
    pub fn harvest(&mut self, member: usize, organ: usize) -> Result<String, &'static str> {
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        let def = self.data.organs.get(organ).ok_or("organe inconnu")?;
        let living = p.alive;
        if living && def.vital {
            return Err("organe vital: impossible sur un vivant");
        }
        let integrity = p.organs.get(organ).copied().unwrap_or(1.0);
        if integrity < 0.3 {
            return Err("organe trop abime");
        }
        let item = self
            .data
            .resource_index(&format!("organ_{}", def.id))
            .ok_or("piece inconnue")?;
        let data = std::sync::Arc::clone(&self.data);
        if self.ship.cargo.add(&data, item, 1.0) < 1.0 {
            return Err("soute pleine");
        }
        let (name, organ_name) = (p.name.clone(), def.name.clone());
        let id = p.id;
        {
            let p = &mut self.crew.members[member];
            p.organs[organ] = 0.0;
            p.organ_kind.resize(data.organs.len(), KIND_NATURAL);
            p.organ_kind[organ] = KIND_NATURAL;
            if living {
                p.morale = (p.morale - 0.4).max(0.0);
                p.loyalty = (p.loyalty - 0.4).max(0.0);
            }
        }
        // The others do not like it, the friends of a living victim even less.
        for q in self
            .crew
            .members
            .iter_mut()
            .filter(|q| q.alive && q.id != id)
        {
            let friend = q.relation(id) > 20;
            let hit = if living { 0.2 } else { 0.05 } + if friend { 0.1 } else { 0.0 };
            q.morale = (q.morale - hit).max(0.0);
        }
        let report = format!("{name} : {organ_name} preleve");
        let now = self.time;
        self.crew.log_event(now, report.clone());
        self.moral_offense("prelever un organe");
        Ok(report)
    }
}

/// kg of mutagen of one injection.
pub const MUTAGEN_DOSE: f64 = 0.1;

/// Fee of the hospital for fitting an implant, credits.
pub const IMPLANT_FEE: f64 = 3000.0;

impl Sim {
    /// Fit implant `cyber` (a part from the hold) on crew member `member`.
    pub fn install_cyber(&mut self, member: usize, cyber: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le patient est mort");
        }
        let def = data.cybernetics.get(cyber).ok_or("implant inconnu")?;
        if p.implants.contains(&cyber) {
            return Err("deja installe");
        }
        let cap = vector_data::CYBER_SLOTS
            .iter()
            .find(|(s, _)| *s == def.slot)
            .map_or(0, |(_, n)| *n);
        let used = p
            .implants
            .iter()
            .filter(|&&i| data.cybernetics[i].slot == def.slot)
            .count();
        if used >= cap {
            return Err("emplacement deja occupe");
        }
        let item = data
            .resource_index(&format!("cyber_{}", def.id))
            .ok_or("piece inconnue")?;
        if self.ship.cargo.kg[item] < 1.0 {
            return Err("piece absente de la soute");
        }
        self.surgeon_skill(member)?;
        let fee = if self.docked.is_some() {
            IMPLANT_FEE
        } else {
            0.0
        };
        if fee > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= fee;
        self.ship.cargo.remove(&data, item, 1.0);
        let p = &mut self.crew.members[member];
        p.implants.push(cyber);
        p.cyber_ok = true;
        let report = format!("{} : implant {} installe", p.name, def.name);
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// Take implant `cyber` out of crew member `member` back into the hold.
    pub fn remove_cyber(&mut self, member: usize, cyber: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        let at = p
            .implants
            .iter()
            .position(|&i| i == cyber)
            .ok_or("pas installe")?;
        let def = &data.cybernetics[cyber];
        let item = data
            .resource_index(&format!("cyber_{}", def.id))
            .ok_or("piece inconnue")?;
        self.surgeon_skill(member)?;
        if self.ship.cargo.add(&data, item, 1.0) < 1.0 {
            return Err("soute pleine");
        }
        let p = &mut self.crew.members[member];
        p.implants.remove(at);
        let report = format!("{} : implant {} retire", p.name, def.name);
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// Inject a dose of mutagen into `member`: one gene shifts at once and the genetic
    /// instability jumps (more mutations, and cancers, follow while it stays high).
    pub fn inject_mutagen(&mut self, member: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le patient est mort");
        }
        let item = data.resource_index("mutagen").ok_or("pas de mutagene")?;
        if self.ship.cargo.kg[item] < MUTAGEN_DOSE {
            return Err("pas assez de mutagene en soute");
        }
        self.ship.cargo.remove(&data, item, MUTAGEN_DOSE);
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            i64::from(p.id),
            self.time as i64,
            19,
        ));
        let p = &mut self.crew.members[member];
        let (gi, dir) = crate::genealogy::mutate(&data, &mut p.genome, &mut rng);
        p.instability = (p.instability + 0.4).min(1.0);
        let report = format!(
            "{} : mutation du gene {} ({dir:+})",
            p.name, data.genes[gi].name
        );
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// An intruder takes the hackable implants of `member` for `days` days (they stop, their
    /// owner is stressed). Returns how many were taken.
    pub fn hack_implants(&mut self, member: usize, days: f64) -> usize {
        let now = self.time;
        let data = std::sync::Arc::clone(&self.data);
        let Some(p) = self.crew.members.get_mut(member) else {
            return 0;
        };
        let n = p
            .implants
            .iter()
            .filter(|&&i| data.cybernetics[i].hackable)
            .count();
        if n > 0 {
            p.hacked_until = now + days * 86_400.0;
            p.hacked = true;
            let text = format!("{} : implants piratés !", p.name);
            self.crew.log_event(now, text);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        Arc::new(GameData::load(&dir).expect("data loads"))
    }

    /// A ship docked with room in the hold and credits.
    fn docked_sim() -> Sim {
        let d = data();
        let mut s = new_sim(5, Arc::clone(&d));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s
    }

    fn give(s: &mut Sim, id: &str, kg: f64) {
        let d = Arc::clone(&s.data);
        let r = d.resource_index(id).expect("resource");
        s.ship.cargo.add(&d, r, kg);
    }

    #[test]
    fn a_prosthesis_replaces_a_dead_organ_and_costs_the_part_and_the_fee() {
        let mut s = docked_sim();
        let lungs = s.data.organ_index("poumons").expect("lungs");
        s.crew.members[0].organs[lungs] = 0.1;
        assert!(
            s.implant(0, lungs, Implant::Prosthesis).is_err(),
            "no part in the hold"
        );
        give(&mut s, "prosthesis_poumons", 1.0);
        let credits = s.credits;
        s.implant(0, lungs, Implant::Prosthesis).expect("operation");
        let p = &s.crew.members[0];
        assert_eq!(p.organs[lungs], PROSTHESIS_INTEGRITY);
        assert_eq!(p.organ_kind[lungs], KIND_PROSTHESIS);
        assert_eq!(s.credits, credits - SURGERY_FEE);
        let r = s.data.resource_index("prosthesis_poumons").expect("r");
        assert!(s.ship.cargo.kg[r] < 0.5, "the part was used");
    }

    #[test]
    fn grafts_can_be_rejected_at_the_expected_rate() {
        let mut rejected = 0;
        let trials = 300;
        let mut s = docked_sim();
        let liver = s.data.organ_index("foie").expect("liver");
        for t in 0..trials {
            s.time = f64::from(t) * 3600.0;
            s.credits = 1.0e6;
            give(&mut s, "organ_foie", 1.0);
            let report = s.implant(0, liver, Implant::Graft).expect("operation");
            if report.contains("REJET") {
                rejected += 1;
                assert_eq!(s.crew.members[0].organs[liver], 0.2);
            } else {
                assert_eq!(s.crew.members[0].organs[liver], 1.0);
            }
        }
        let rate = f64::from(rejected) / f64::from(trials);
        let expected = REJECTION * (1.0 - 4.0 / 12.0);
        assert!(
            (rate - expected).abs() < 0.08 && rejected > 0,
            "rate {rate} vs {expected}"
        );
    }

    #[test]
    fn harvesting_the_dead_fills_the_hold_and_the_living_remember() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        let sib = {
            let me = s.crew.members[0].kin;
            let sib = crate::genealogy::siblings(&s.crew.families, me)[0];
            crate::crew::person_from_kin(&d, &s.crew.families, sib, 99, None, None)
        };
        s.crew.hire(&d, sib, 2, 0.0);
        let heart = d.organ_index("coeur").expect("heart");
        let eyes = d.organ_index("yeux").expect("eyes");
        // A living person cannot give a heart, but can give an eye.
        assert!(s.harvest(1, heart).is_err());
        let morale = s.crew.members[0].morale;
        s.harvest(1, eyes).expect("an eye of the living");
        assert!(s.crew.members[0].morale < morale, "the brother minds");
        assert_eq!(s.crew.members[1].organs[eyes], 0.0);
        let item = d.resource_index("organ_yeux").expect("r");
        assert!(s.ship.cargo.kg[item] >= 1.0);
        // The dead can give anything.
        s.crew.members[1].alive = false;
        s.harvest(1, heart).expect("the heart of the dead");
        assert!(s.ship.cargo.kg[d.resource_index("organ_coeur").expect("r")] >= 1.0);
        assert!(s.harvest(1, heart).is_err(), "nothing left to take");
    }

    #[test]
    fn implants_boost_skills_stop_without_parts_or_power_and_can_be_hacked() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        let eye = d.cyber_index("oeil").expect("eye");
        let hand = d.cyber_index("main_fine").expect("hand");
        let pilot = d.skill_index("pilotage").expect("skill");
        let before = f64::from(s.crew.members[0].skills[pilot]);
        // Needs the part first.
        assert!(s.install_cyber(0, eye).is_err());
        give(&mut s, "cyber_oeil", 1.0);
        give(&mut s, "cyber_main_fine", 2.0);
        s.install_cyber(0, eye).expect("eye");
        assert!(s.install_cyber(0, eye).is_err(), "already fitted");
        s.install_cyber(0, hand).expect("one hand");
        s.install_cyber(0, hand).expect_err("the same twice");
        let p = &s.crew.members[0];
        let skill_key = "skill:pilotage";
        assert!((p.implant_effect(&d, skill_key) - 1.0).abs() < 1e-9);
        assert!(before + p.implant_effect(&d, skill_key) > before);
        // Hacked: the hackable ones stop, then come back.
        assert_eq!(s.hack_implants(0, 2.0), 2);
        assert_eq!(
            s.crew.members[0].implant_effect(&d, skill_key),
            0.0,
            "the intruder stops them at once"
        );
        give(&mut s, "spare_parts", 50.0);
        let mut cargo = std::mem::take(&mut s.ship.cargo);
        let mut credits = s.credits;
        let mut ctx = crate::crew::CrewCtx {
            data: &d,
            cargo: &mut cargo,
            credits: &mut credits,
            life_ok: true,
            life_capacity: 9.0,
            crew_cap: 9.0,
            food_per_day: 0.0,
            medbay: 0.0,
            workshop: 0.0,
            overheating: false,
            seed: 1,
            needs: false,
            event_freq: 0.0,
        };
        s.crew.update(&mut ctx, 3600.0);
        assert!(s.crew.members[0].hacked);
        assert_eq!(
            s.crew.members[0].implant_effect(&d, skill_key),
            0.0,
            "hacked implants stop"
        );
        assert!(s.crew.members[0].implant_strain(&d) > 0.25);
        // Without life support power they stop too, hacked or not.
        s.crew.members[0].hacked_until = 0.0;
        let mut ctx2 = crate::crew::CrewCtx {
            life_ok: false,
            ..ctx
        };
        s.crew.update(&mut ctx2, 7200.0);
        assert!(!s.crew.members[0].cyber_ok);
    }

    #[test]
    fn mutagen_shifts_one_allele_and_instability_decays_but_radiation_feeds_it() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        assert!(s.inject_mutagen(0).is_err(), "needs mutagen");
        give(&mut s, "mutagen", 1.0);
        let before = s.crew.members[0].genome.clone();
        let report = s.inject_mutagen(0).expect("injection");
        let after = &s.crew.members[0].genome;
        let changed: Vec<usize> = (0..before.len())
            .filter(|&i| before[i] != after[i])
            .collect();
        assert_eq!(changed.len(), 1, "exactly one allele moved: {report}");
        assert_eq!(before[changed[0]].abs_diff(after[changed[0]]), 1);
        assert!(after.iter().all(|&a| a <= 3));
        assert!(s.crew.members[0].instability >= 0.4);
        // Time heals instability when nothing feeds it; radiation sickness feeds it.
        let mut cargo = std::mem::take(&mut s.ship.cargo);
        let mut credits = s.credits;
        let mut ctx = crate::crew::CrewCtx {
            data: &d,
            cargo: &mut cargo,
            credits: &mut credits,
            life_ok: true,
            life_capacity: 9.0,
            crew_cap: 9.0,
            food_per_day: 0.0,
            medbay: 0.0,
            workshop: 0.0,
            overheating: false,
            seed: 3,
            needs: false,
            event_freq: 0.0,
        };
        s.crew.members[0].instability = 0.1;
        s.crew.update(&mut ctx, 10.0 * 86_400.0);
        assert!(s.crew.members[0].instability < 0.1, "it decays");
        s.crew.members[0].disease = Some((d.disease_index("radiation").expect("d"), 40.0));
        let low = s.crew.members[0].instability;
        s.crew.update(&mut ctx, 20.0 * 86_400.0);
        assert!(
            s.crew.members[0].instability > low + 0.1,
            "radiation feeds it"
        );
    }

    #[test]
    fn a_very_unstable_person_keeps_mutating_a_stable_one_does_not() {
        let d = data();
        let run = |instability: f32| -> usize {
            let mut s = docked_sim();
            let mut cargo = std::mem::take(&mut s.ship.cargo);
            let mut credits = s.credits;
            let mut ctx = crate::crew::CrewCtx {
                data: &d,
                cargo: &mut cargo,
                credits: &mut credits,
                life_ok: true,
                life_capacity: 9.0,
                crew_cap: 9.0,
                food_per_day: 20.0,
                medbay: 0.0,
                workshop: 0.0,
                overheating: false,
                seed: 4,
                needs: false,
                event_freq: 0.0,
            };
            let before = s.crew.members[0].genome.clone();
            s.crew.members[0].instability = instability;
            s.crew.update(&mut ctx, 12.0 * 86_400.0);
            let after = &s.crew.members[0].genome;
            (0..before.len()).filter(|&i| before[i] != after[i]).count()
        };
        assert!(run(0.9) >= 1, "an unstable genome drifts");
        assert_eq!(run(0.0), 0, "a stable genome does not");
    }

    #[test]
    fn a_grown_clone_is_never_rejected_and_costs_more_than_the_organ() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        let liver = d.organ_index("foie").unwrap();
        let credits = s.credits;
        let cost = s.grow_clone(liver).expect("grow");
        assert!((cost - d.organs[liver].value * CLONE_COST_FACTOR).abs() < 1e-6);
        assert_eq!(s.credits, credits - cost);
        // Even over many operations a clone graft never fails, where plain grafts sometimes do.
        for t in 0..80 {
            s.time = f64::from(t) * 3600.0;
            s.credits = 1.0e6;
            s.grow_clone(liver).unwrap();
            let report = s.implant(0, liver, Implant::Graft).unwrap();
            assert!(!report.contains("REJET"), "a clone is never rejected");
            // The clone was used, not a plain graft.
            let c = d.resource_index("clone_foie").unwrap();
            assert!(s.ship.cargo.kg[c] < 1.5, "one clone was used each time");
        }
        s.undock();
        assert!(s.grow_clone(liver).is_err(), "no hospital in space");
    }

    #[test]
    fn where_the_body_is_sacred_surgery_costs_a_fine_and_reputation() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        let verdant = d
            .factions
            .iter()
            .position(|f| f.id == "verdant")
            .expect("verdant");
        let concorde = d
            .factions
            .iter()
            .position(|f| f.id == "concorde")
            .expect("concorde");
        let liver = d.organ_index("foie").unwrap();
        let events_before = s.events.len();
        // Among open people nothing happens.
        s.faction = Some(concorde);
        let (credits, rep) = (s.credits, s.reputation_with(Some(concorde)));
        s.grow_clone(liver).unwrap();
        let cost = d.organs[liver].value * CLONE_COST_FACTOR;
        assert_eq!(s.credits, credits - cost);
        assert_eq!(s.reputation_with(Some(concorde)), rep);
        assert_eq!(s.events.len(), events_before);
        // Among those who hold the body sacred it is a sacrilege.
        s.faction = Some(verdant);
        let (credits, rep) = (s.credits, s.reputation_with(Some(verdant)));
        s.grow_clone(liver).unwrap();
        assert_eq!(s.credits, credits - cost - SACRILEGE_FINE);
        assert!(s.reputation_with(Some(verdant)) < rep - 0.15);
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, crate::Event::MarketNews(t) if t.contains("sacrilege"))));
    }
}
