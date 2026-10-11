//! Surgery and the organ trade: transplants of grafts bought or harvested, prostheses, and
//! harvesting organs from the dead (or, at a heavy price in morale, from the living).
use crate::Sim;
use vector_core::{hash_coord, Rng};

/// Fee of the hospital of a station for one operation, credits.
pub const SURGERY_FEE: f64 = 1500.0;
/// Base chance that a graft is rejected (lowered by the surgeon's medicine skill).
pub const REJECTION: f64 = 0.2;
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
        let item = self
            .data
            .resource_index(&format!("{prefix}{}", def.id))
            .ok_or("piece inconnue")?;
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
                let chance = REJECTION * (1.0 - skill / 12.0);
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
        Ok(report)
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
        Ok(report)
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
}
