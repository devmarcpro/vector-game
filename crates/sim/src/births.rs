//! Births aboard. Two crew members who are not close kin, of opposite sex, adults, in good spirits
//! and fond of each other may conceive; the child arrives after a (shortened) gestation, joins the
//! family tree under its parents, and receives a genome recombined from the genomes they actually
//! carry: a mutation of a parent is handed down. The child cannot work before it grows up.
use crate::bourse::DAY;
use crate::crew::{person_from_kin, Person};
use crate::genealogy::{self, Kin};
use crate::planet::unit;
use crate::{Event, Sim};
use vector_core::hash_coord;

/// Days of gestation (shortened for play), odds per day that an eligible couple conceives.
pub const GESTATION_DAYS: f64 = 60.0;
pub const CONCEPTION_ODDS: f64 = 0.004;
/// Ages of parents, the affection needed, the mood needed.
pub const PARENT_AGE: (f32, f32) = (18.0, 45.0);
pub const MIN_AFFECTION: i16 = 25;
pub const MIN_MORALE: f32 = 0.55;

impl Sim {
    /// Whether crew member `id` is expecting a child.
    pub fn is_pregnant(&self, id: u32) -> bool {
        self.crew.pregnancies.iter().any(|p| p.0 == id)
    }

    fn can_parent(p: &Person) -> bool {
        p.alive
            && p.health > 0.5
            && !p.body.cryo
            && p.morale >= MIN_MORALE
            && p.age >= PARENT_AGE.0
            && p.age <= PARENT_AGE.1
    }

    /// Couples who could conceive now: (father index, mother index).
    pub fn eligible_couples(&self) -> Vec<(usize, usize)> {
        let m = &self.crew.members;
        let mut out = Vec::new();
        for (i, f) in m.iter().enumerate() {
            if !f.kin.male() || !Self::can_parent(f) {
                continue;
            }
            for (j, mo) in m.iter().enumerate() {
                if mo.kin.male() || !Self::can_parent(mo) || self.is_pregnant(mo.id) {
                    continue;
                }
                let fond = f
                    .relations
                    .iter()
                    .find(|r| r.0 == mo.id)
                    .is_some_and(|r| r.1 >= MIN_AFFECTION)
                    && mo
                        .relations
                        .iter()
                        .find(|r| r.0 == f.id)
                        .is_some_and(|r| r.1 >= MIN_AFFECTION);
                let kin = genealogy::relationship(&self.crew.families, f.kin, mo.kin).is_some();
                if fond && !kin {
                    out.push((i, j));
                }
            }
        }
        out
    }

    /// Once a day: conceptions, and births that came due.
    pub(crate) fn births_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.births_day;
        self.births_day = day;
        if last == i64::MIN || day <= last {
            return;
        }
        // Gestation stands still while the mother sleeps in a caisson.
        let frozen: Vec<u32> = self
            .crew
            .members
            .iter()
            .filter(|p| p.alive && p.body.cryo)
            .map(|p| p.id)
            .collect();
        for p in &mut self.crew.pregnancies {
            if frozen.contains(&p.0) {
                p.2 += (day - last) as f64 * DAY;
            }
        }
        // Births first.
        let due: Vec<(u32, u32, f64)> = self
            .crew
            .pregnancies
            .iter()
            .copied()
            .filter(|p| p.2 <= self.time)
            .collect();
        for (mid, fid, _) in due {
            self.crew.pregnancies.retain(|p| p.0 != mid);
            self.give_birth(fid, mid);
        }
        // Then conceptions (the cabin must have room).
        if (self.crew.count() + self.crew.pregnancies.len()) as f64 >= self.ship.stats.crew_cap {
            return;
        }
        for (i, j) in self.eligible_couples() {
            let (fid, mid) = (self.crew.members[i].id, self.crew.members[j].id);
            let h = hash_coord(
                self.galaxy.seed ^ 0xB147,
                day,
                i64::from(fid),
                i64::from(mid),
            );
            if unit(h) < CONCEPTION_ODDS && !self.is_pregnant(mid) {
                self.crew
                    .pregnancies
                    .push((mid, fid, self.time + GESTATION_DAYS * DAY));
                let name = self.crew.members[j].name.clone();
                self.crew
                    .log_event(self.time, format!("{name} attend un enfant."));
            }
        }
    }

    /// The child of father `fid` and mother `mid` is born.
    pub(crate) fn give_birth(&mut self, fid: u32, mid: u32) {
        let find = |id: u32| self.crew.members.iter().position(|p| p.id == id);
        let (Some(fi), Some(mi)) = (find(fid), find(mid)) else {
            return;
        };
        if !self.crew.members[mi].alive {
            return;
        }
        let (father, mother) = (self.crew.members[fi].clone(), self.crew.members[mi].clone());
        let fam = genealogy::new_family(&mut self.crew.families, father.kin, mother.kin, 1);
        let kin = Kin { fam, k: 0 };
        let seed = hash_coord(
            self.galaxy.seed ^ 0xB1B7,
            i64::from(fid),
            i64::from(mid),
            self.time as i64,
        );
        let mut child = person_from_kin(
            &self.data,
            &self.crew.families,
            kin,
            seed,
            None,
            Some((father.kin, mother.kin, 1)),
        );
        // Heredity: the genes the parents really carry, mutations included.
        child.genome = genealogy::child_genome(&self.data, kin, &father.genome, &mother.genome);
        child.age = 0.0;
        child.salary = 0.0;
        child.traits.clear();
        for s in &mut child.skills {
            *s *= 0.1;
        }
        child.instability = (father.instability + mother.instability) * 0.25;
        child.morale = 0.9;
        let name = child.name.clone();
        self.crew.hire(&self.data, child, 0, self.time);
        self.crew.log_event(
            self.time,
            format!(
                "Naissance a bord : {name}, enfant de {} et {}.",
                father.name, mother.name
            ),
        );
        self.events.push(Event::MarketNews(format!(
            "Naissance a bord : {name} ! Un enfant de plus dans l'equipage."
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::generate_person;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// A sim with a fond, unrelated, adult couple aboard; returns (father id, mother id).
    fn couple() -> (Sim, u32, u32) {
        let mut s = new_sim(5, data());
        s.crew.members.clear();
        let mut ids = Vec::new();
        for want_male in [true, false] {
            let mut seed = 500u64;
            let p = loop {
                let mut p = generate_person(&s.data, seed, None);
                seed += 1;
                if p.kin.male() == want_male
                    && (ids.is_empty() || p.kin.fam != s.crew.members[0].kin.fam)
                {
                    p.age = 30.0;
                    p.health = 1.0;
                    p.morale = 0.9;
                    break p;
                }
            };
            let k = s.crew.hire(&s.data, p, 1, 0.0);
            ids.push(s.crew.members[k].id);
        }
        for (a, b) in [(0, 1), (1, 0)] {
            let other = ids[b];
            s.crew.members[a].relations.push((other, 60));
        }
        s.ship.stats.crew_cap = 20.0;
        (s, ids[0], ids[1])
    }

    #[test]
    fn only_fond_unrelated_adults_of_both_sexes_can_conceive() {
        let (mut s, f, m) = couple();
        assert_eq!(s.eligible_couples().len(), 1);
        // Too young, too unhappy, not fond, already expecting: each rules the couple out.
        let fi = s.crew.members.iter().position(|p| p.id == f).unwrap();
        let mi = s.crew.members.iter().position(|p| p.id == m).unwrap();
        s.crew.members[fi].age = 12.0;
        assert!(
            s.eligible_couples().is_empty(),
            "a child cannot be a father"
        );
        s.crew.members[fi].age = 30.0;
        s.crew.members[mi].morale = 0.2;
        assert!(s.eligible_couples().is_empty(), "unhappy");
        s.crew.members[mi].morale = 0.9;
        s.crew.members[mi].relations.clear();
        assert!(s.eligible_couples().is_empty(), "not fond");
        s.crew.members[mi].relations.push((f, 60));
        assert_eq!(s.eligible_couples().len(), 1);
        s.crew.pregnancies.push((m, f, 1.0e12));
        assert!(s.is_pregnant(m) && s.eligible_couples().is_empty());
    }

    #[test]
    fn a_couple_eventually_has_a_child_who_cannot_work_yet() {
        let (mut s, f, m) = couple();
        let n0 = s.crew.members.len();
        let mut born = false;
        for d in 1..2000 {
            s.time = f64::from(d) * DAY;
            s.births_tick();
            if s.crew.members.len() > n0 {
                born = true;
                break;
            }
        }
        assert!(born, "a child is born");
        let c = s.crew.members.last().unwrap();
        assert_eq!(c.age, 0.0);
        assert_eq!(c.salary, 0.0);
        assert!(c.family_def.is_some());
        let (fk, mk) = {
            let g = |id| s.crew.members.iter().find(|p| p.id == id).unwrap().kin;
            (g(f), g(m))
        };
        assert!(
            genealogy::relationship(&s.crew.families, c.kin, fk).is_some(),
            "the father is its father"
        );
        assert!(genealogy::relationship(&s.crew.families, c.kin, mk).is_some());
        assert!(!s.is_pregnant(m), "no longer expecting");
        assert!(s.crew.log.iter().any(|l| l.text.starts_with("Naissance")));
    }

    #[test]
    fn a_parents_mutation_is_handed_down() {
        let (mut s, f, m) = couple();
        let d = Arc::clone(&s.data);
        // Both parents carry allele 3 everywhere (as if a mutation had set it): so do the children.
        for p in &mut s.crew.members {
            p.genome.fill(3);
        }
        let n0 = s.crew.members.len();
        let (mut same, mut total) = (0, 0);
        for k in 0..30 {
            s.time = 1000.0 * DAY + f64::from(k) * DAY;
            s.give_birth(f, m);
            let c = s.crew.members.last().unwrap();
            total += c.genome.len();
            same += c.genome.iter().filter(|g| **g == 3).count();
        }
        assert_eq!(s.crew.members.len(), n0 + 30);
        let share = same as f64 / total as f64;
        assert!(share > 0.95, "{share} of the alleles are inherited");
        assert_eq!(
            s.crew.members.last().unwrap().genome.len(),
            2 * d.genes.len()
        );
    }

    #[test]
    fn a_full_cabin_blocks_conception() {
        let (mut s, _, _) = couple();
        s.ship.stats.crew_cap = 2.0;
        for d in 1..600 {
            s.time = f64::from(d) * DAY;
            s.births_tick();
        }
        assert!(s.crew.pregnancies.is_empty(), "no room, no pregnancy");
        assert_eq!(s.crew.members.len(), 2);
    }
}
