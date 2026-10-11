//! Care: diagnosis, first aid, drugs, surgery of wounds, defibrillation, cryostasis. The actions
//! of the player on the body model of `body.rs`; the same ones serve the infirmary of the ship
//! and the hospital of a station (see `surgery.rs` for organs and implants).
use crate::body;
use crate::Sim;
use vector_core::{hash_coord, Rng};

/// Fee of a wound operation at the hospital of a station, credits.
pub const WOUND_SURGERY_FEE: f64 = 600.0;
/// Medicine (kg) used by a round of first aid per wound.
pub const FIRST_AID_KG: f64 = 0.02;

impl Sim {
    /// The skill (0..10) of whoever looks after `patient`: the best medic aboard (not the patient),
    /// else a layman, else the patient themselves.
    pub fn carer_skill(&self, patient: usize) -> f64 {
        let Some(med) = self.data.skill_index("medecine") else {
            return 0.0;
        };
        let mut best = 0.0f64;
        for (i, p) in self.crew.members.iter().enumerate() {
            if !p.alive || p.body.cryo || p.body.unconscious() {
                continue;
            }
            let s = f64::from(p.skills[med]);
            let s = if self.data.roles[p.role].id == "medic" {
                s
            } else {
                s * 0.6
            };
            let s = if i == patient { s * 0.5 } else { s };
            best = best.max(s);
        }
        best
    }

    fn medicine_kg(&self) -> f64 {
        self.data
            .resource_index("medicine")
            .map_or(0.0, |m| self.ship.cargo.kg[m])
    }

    fn use_medicine(&mut self, kg: f64) {
        let data = std::sync::Arc::clone(&self.data);
        if let Some(m) = data.resource_index("medicine") {
            self.ship.cargo.remove(&data, m, kg);
        }
    }

    fn patient(&self, member: usize) -> Result<&crate::crew::Person, &'static str> {
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le patient est mort");
        }
        Ok(p)
    }

    /// What the carer finds on a member: lines of text (wound, part, estimated severity), then
    /// vital signs. Internal wounds need a skilled eye or the infirmary's scanner.
    pub fn diagnose(&self, member: usize) -> Result<Vec<String>, &'static str> {
        let p = self.patient(member)?;
        let skill = self.carer_skill(member);
        let (seen, hidden) = p.body.diagnose(&self.data, skill, self.effective.medbay);
        let mut lines = Vec::new();
        for (i, sev) in seen {
            let w = &p.body.wounds[i];
            lines.push(format!(
                "{} ({}) : {:.0} %{}{}",
                self.data.wounds[w.kind as usize].name,
                self.data.body_parts[w.part as usize].name,
                sev * 100.0,
                if w.dressed > 0.0 { ", panse" } else { "" },
                if w.infection > 0.0 { ", INFECTEE" } else { "" }
            ));
        }
        if hidden > 0 {
            lines.push("des lesions internes sont soupconnees : il faut un medecin competent ou l'infirmerie".into());
        }
        lines.push(format!(
            "sang {:.0} %  oxygene {:.0} %  temperature {:.1} C  douleur {:.0} %",
            p.body.blood * 100.0,
            p.body.oxygen * 100.0,
            p.body.core_temp,
            p.body.pain * 100.0
        ));
        Ok(lines)
    }

    /// First aid: dress and splint every external wound, with the care of the best carer.
    pub fn first_aid(&mut self, member: usize) -> Result<String, &'static str> {
        let skill = self.carer_skill(member);
        let p = self.patient(member)?;
        let open = p
            .body
            .wounds
            .iter()
            .filter(|w| !self.data.wounds[w.kind as usize].internal)
            .count();
        if open == 0 {
            return Err("rien a panser");
        }
        let need = FIRST_AID_KG * open as f64;
        if self.medicine_kg() < need {
            return Err("pas assez de medicaments en soute");
        }
        self.use_medicine(need);
        let data = std::sync::Arc::clone(&self.data);
        let p = &mut self.crew.members[member];
        let n = p.body.dress_wounds(&data, skill);
        let name = p.name.clone();
        let report = format!("{name} : {n} plaie(s) pansee(s) (competence {skill:.0})");
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// Give a dose of a drug of `data/drugs.ron` taken from the hold.
    pub fn give_drug(&mut self, member: usize, drug: usize) -> Result<String, &'static str> {
        self.patient(member)?;
        let data = std::sync::Arc::clone(&self.data);
        let def = data.drugs.get(drug).ok_or("medicament inconnu")?;
        let r = data
            .resource_index(&def.resource)
            .ok_or("ressource inconnue")?;
        if self.ship.cargo.kg[r] < def.dose {
            return Err("pas assez de medicaments en soute");
        }
        self.ship.cargo.remove(&data, r, def.dose);
        let p = &mut self.crew.members[member];
        p.body.take_drug(&data, drug);
        let liver = data
            .organ_index("foie")
            .and_then(|o| p.organs.get(o).copied())
            .unwrap_or(1.0);
        let pressure = p.body.overdose_pressure(&data, liver);
        let name = p.name.clone();
        let mut report = format!("{name} : {} administre", def.name.to_lowercase());
        if pressure > body::OVERDOSE {
            report.push_str(" - DANGER DE SURDOSE");
        }
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// Operate on wound `wound` of a member (the infirmary with a medic aboard, or the hospital
    /// of a station): the wound is cleaned and sutured, internal bleeding is stopped. A clumsy
    /// surgeon can make it worse.
    pub fn operate_wound(&mut self, member: usize, wound: usize) -> Result<String, &'static str> {
        self.patient(member)?;
        let w = self.crew.members[member]
            .body
            .wounds
            .get(wound)
            .ok_or("plaie inconnue")?
            .clone();
        let skill = self.surgeon_skill(member)?;
        let fee = if self.docked.is_some() {
            WOUND_SURGERY_FEE
        } else {
            0.0
        };
        if fee > self.credits {
            return Err("fonds insuffisants");
        }
        if self.medicine_kg() < 0.1 {
            return Err("pas assez de medicaments en soute");
        }
        self.credits -= fee;
        self.use_medicine(0.1);
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            i64::from(self.crew.members[member].id),
            wound as i64,
            self.time as i64,
        ));
        let botch = rng.f64() < 0.3 * (1.0 - skill / 10.0);
        let data = std::sync::Arc::clone(&self.data);
        let p = &mut self.crew.members[member];
        let name = p.name.clone();
        let kind = data.wounds[w.kind as usize].name.to_lowercase();
        let report = if botch {
            p.body.wounds[wound].sev = (w.sev + 0.15).min(1.0);
            p.body.wounds[wound].infection = p.body.wounds[wound].infection.max(0.1);
            format!("{name} : operation de {kind} ratee, la plaie s'aggrave")
        } else {
            let q = &mut p.body.wounds[wound];
            q.dressed = 1.0;
            q.infection = 0.0;
            q.sev = (q.sev - (0.4 + 0.05 * skill as f32)).max(0.02);
            format!("{name} : {kind} operee (chirurgien {skill:.0})")
        };
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// Shock a failing person back: only when they are unconscious or near death, with the
    /// infirmary's defibrillator and the skill of the carer.
    pub fn defibrillate(&mut self, member: usize) -> Result<String, &'static str> {
        let p = self.patient(member)?;
        if !(p.body.unconscious() || p.health < 0.2) {
            return Err("le patient n'en a pas besoin");
        }
        let skill = self.carer_skill(member);
        let medbay = self.effective.medbay;
        if medbay <= 0.0 && self.docked.is_none() {
            return Err("pas de defibrillateur sans infirmerie");
        }
        if self.medicine_kg() < 0.05 {
            return Err("pas assez de medicaments en soute");
        }
        self.use_medicine(0.05);
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            i64::from(self.crew.members[member].id),
            self.time as i64,
            31,
        ));
        let ok = rng.f64() < body::revive_chance(skill, medbay.max(1.0), 1.0).max(0.3);
        let p = &mut self.crew.members[member];
        let name = p.name.clone();
        let report = if ok {
            p.body.oxygen = p.body.oxygen.max(0.7);
            p.body.blood = p.body.blood.max(0.5);
            p.health = (p.health + 0.2).min(1.0);
            format!("{name} : choc electrique, le coeur repart")
        } else {
            format!("{name} : le choc n'a rien donne")
        };
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// Put a member in cryostasis (needs the powered infirmary or a station's hospital): no
    /// needs, no ageing, wounds stop; if the power fails the sleeper thaws badly.
    pub fn cryo_start(&mut self, member: usize) -> Result<String, &'static str> {
        self.patient(member)?;
        if self.effective.medbay <= 0.0 && self.docked.is_none() {
            return Err("pas d'infirmerie alimentee");
        }
        if self.crew.members[member].body.cryo {
            return Err("deja en cryostase");
        }
        self.crew.members[member].body.cryo = true;
        let name = self.crew.members[member].name.clone();
        let report = format!("{name} entre en cryostase");
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    pub fn cryo_wake(&mut self, member: usize) -> Result<String, &'static str> {
        self.patient(member)?;
        if !self.crew.members[member].body.cryo {
            return Err("pas en cryostase");
        }
        self.crew.members[member].body.thaw();
        let name = self.crew.members[member].name.clone();
        let report = format!("{name} est rechauffe");
        let now = self.time;
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// A blow, burn or fall hurts a member: wounds picked by the cause (`kind` is the id of a wound
    /// of `data/wounds.ron`), the organs below the wound take part of it.
    pub fn wound_member(&mut self, member: usize, kind: &str, sev: f32, pick: u64) {
        let data = std::sync::Arc::clone(&self.data);
        let Some(p) = self.crew.members.get_mut(member) else {
            return;
        };
        if !p.alive {
            return;
        }
        if let Some((o, d)) = p.body.strike(&data, kind, sev, pick) {
            if let Some(v) = p.organs.get_mut(o) {
                if p.organ_kind.get(o).copied() != Some(2) {
                    *v = (*v - d).max(0.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::crew::Crew;
    use crate::scene::new_sim;
    use crate::Sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn docked_sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let d = Arc::new(GameData::load(&dir).expect("data loads"));
        let mut s = new_sim(5, Arc::clone(&d));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s
    }

    fn empty_medicine(s: &mut Sim) {
        let m = s.data.resource_index("medicine").expect("medicine");
        s.ship.cargo.kg[m] = 0.0;
    }

    fn give(s: &mut Sim, id: &str, kg: f64) {
        let d = Arc::clone(&s.data);
        let r = d.resource_index(id).expect("resource");
        s.ship.cargo.add(&d, r, kg);
    }

    #[test]
    fn first_aid_needs_medicine_and_pans_wounds_better_with_skill() {
        let mut s = docked_sim();
        s.crew.members[0].body.add_wound(&s.data, "coupure", 2, 0.6);
        empty_medicine(&mut s);
        assert!(s.first_aid(0).is_err(), "no medicine in the hold");
        give(&mut s, "medicine", 5.0);
        let before = s.ship.cargo.kg[s.data.resource_index("medicine").expect("m")];
        s.first_aid(0).expect("dressed");
        let after = s.ship.cargo.kg[s.data.resource_index("medicine").expect("m")];
        assert!(after < before, "it costs medicine");
        let q_lay = s.crew.members[0].body.wounds[0].dressed;
        assert!(q_lay > 0.3, "{q_lay}");
        // The quality of the dressing follows the skill of the carer: the same cut, dressed by a
        // lone patient and then by a medic of skill 9.
        let med = s.data.skill_index("medecine").expect("skill");
        let dress = |medic: bool| {
            let mut t = docked_sim();
            t.crew.members[0].skills[med] = 0.0;
            t.crew.members[0].body.add_wound(&t.data, "coupure", 2, 0.6);
            give(&mut t, "medicine", 5.0);
            if medic {
                let d = Arc::clone(&t.data);
                let mut p = crate::crew::generate_person(&d, 77, Some(med));
                p.skills[med] = 9.0;
                let role = Crew::role_index(&d, "medic");
                t.crew.hire(&d, p, role, 0.0);
            }
            t.first_aid(0).expect("dressed");
            t.crew.members[0].body.wounds[0].dressed
        };
        let (lay, doctor) = (dress(false), dress(true));
        assert!(
            doctor > lay + 0.1,
            "a skilled hand dresses better: {lay} {doctor}"
        );
    }

    #[test]
    fn surgery_closes_internal_bleeding_and_costs_a_fee() {
        let mut s = docked_sim();
        give(&mut s, "medicine", 5.0);
        s.crew.members[0]
            .body
            .add_wound(&s.data, "hemorragie_interne", 1, 0.8);
        let credits = s.credits;
        let report = s.operate_wound(0, 0);
        assert!(report.is_ok(), "{report:?}");
        assert!(s.credits < credits, "the hospital bills the operation");
        let w = &s.crew.members[0].body.wounds[0];
        assert!(w.dressed >= 1.0 || w.sev > 0.8, "sutured or botched");
        assert!(s.operate_wound(0, 9).is_err(), "no such wound");
    }

    #[test]
    fn a_drug_is_taken_from_the_hold_and_overdose_is_flagged() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        let opioid = d.drug_index("opioide").expect("o");
        let sed = d.drug_index("sedatif").expect("s");
        empty_medicine(&mut s);
        assert!(s.give_drug(0, opioid).is_err(), "none in the hold");
        give(&mut s, "medicine", 5.0);
        s.give_drug(0, opioid).expect("ok");
        s.give_drug(0, opioid).expect("ok");
        let report = s.give_drug(0, sed).expect("ok");
        assert!(report.contains("SURDOSE"), "{report}");
        assert!(s.crew.members[0].body.pain == 0.0);
    }

    #[test]
    fn cryostasis_stops_time_and_dies_without_power() {
        let mut s = docked_sim();
        let d = Arc::clone(&s.data);
        s.cryo_start(0).expect("docked: the hospital freezes");
        assert!(s.cryo_start(0).is_err());
        let age = s.crew.members[0].age;
        let hunger = s.crew.members[0].hunger;
        s.crew.members[0].body.add_wound(&d, "coupure", 2, 0.8);
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
            medbay: 1.0,
            workshop: 0.0,
            overheating: false,
            seed: 3,
            needs: true,
            event_freq: 0.0,
        };
        let t0 = s.crew.last.max(1.0);
        s.crew.last = t0;
        s.crew.update(&mut ctx, t0 + 200.0 * crate::crew::DAY);
        let p = &s.crew.members[0];
        assert!(
            p.alive && (p.age - age).abs() < 1e-6,
            "no ageing in the freezer"
        );
        assert!((p.hunger - hunger).abs() < 1e-6, "no hunger");
        assert!(p.body.blood > 0.99, "no bleeding");
        // The power fails: the sleeper does not survive the thaw.
        ctx.medbay = 0.0;
        s.crew.update(&mut ctx, t0 + 260.0 * crate::crew::DAY);
        assert!(!s.crew.members[0].alive, "thawed without power");
    }
}
