//! Law and justice. Every faction has the laws its mores give it (`data/laws.ron`: a law is in
//! force where ANY of its conditions holds on the eight axes of `data/ethos.ron`), and only in
//! its own space: the local law applies where the faction has jurisdiction, nowhere else.
//! The sim reports acts (`offend`); where the act is illegal and somebody saw it, the player
//! gets a fine owed and a notoriety in that jurisdiction. Serious crimes follow the player into
//! the jurisdictions linked by a treaty (an alliance in `factions.ron`: extradition). The
//! patrol at the docks collects what is owed, a bribe or a lawyer lightens the sentence, and a
//! warrant (high notoriety) closes the docks. Notoriety fades with the days.
use crate::bourse::DAY;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};

/// Notoriety lost per day.
pub const NOTORIETY_DECAY: f32 = 0.01;
/// Notoriety (here, directly or by extradition) at which the docks are closed to the player.
pub const WARRANT: f32 = 3.0;
/// Share of a serious crime's notoriety that follows the player into allied jurisdictions.
pub const EXTRADITION_SHARE: f32 = 0.5;
/// Reputation lost with the people per level of severity.
pub const REP_PER_SEVERITY: f32 = 2.0;
/// Chance that an act is seen, away from a station and at a station.
pub const SEEN_AWAY: f64 = 0.5;
pub const SEEN_DOCKED: f64 = 0.95;

/// What the player's record says in one jurisdiction.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub faction: usize,
    /// Notoriety of all crimes, and of the serious ones only (the part that is extradited).
    pub notoriety: f32,
    pub grave: f32,
    /// Fines not yet paid.
    pub owed: f64,
    /// Date of the last update (notoriety fades from there).
    pub t: f64,
}

/// What an offence cost.
#[derive(Clone, Debug, PartialEq)]
pub struct Offence {
    pub law: usize,
    pub fine: f64,
}

impl Sim {
    /// Laws in force in the space of `faction`.
    pub fn laws_in_force(&self, faction: usize) -> Vec<usize> {
        let d = &self.data;
        let values = d.ethos.profile(Some(d.factions[faction].id.as_str()));
        (0..d.laws.len())
            .filter(|&l| d.laws[l].in_force(&d.ethos, &values))
            .collect()
    }

    /// Whether `faction` has a law against the act.
    pub fn is_illegal(&self, faction: usize, act: &str) -> bool {
        self.data
            .law_index(act)
            .is_some_and(|l| self.laws_in_force(faction).contains(&l))
    }

    /// The treaty between two jurisdictions: the same people, or an alliance either way.
    pub fn extradition_treaty(&self, a: usize, b: usize) -> bool {
        let (fa, fb) = (&self.data.factions[a], &self.data.factions[b]);
        a == b || fa.allies.contains(&fb.id) || fb.allies.contains(&fa.id)
    }

    fn record_index(&mut self, faction: usize) -> usize {
        let now = self.time;
        match self.trade.record.iter().position(|r| r.faction == faction) {
            Some(i) => {
                let r = &mut self.trade.record[i];
                let fade = ((now - r.t).max(0.0) / DAY) as f32 * NOTORIETY_DECAY;
                r.notoriety = (r.notoriety - fade).max(0.0);
                r.grave = (r.grave - fade).max(0.0);
                r.t = now;
                i
            }
            None => {
                self.trade.record.push(Record {
                    faction,
                    t: now,
                    ..Record::default()
                });
                self.trade.record.len() - 1
            }
        }
    }

    /// Fines owed in a jurisdiction.
    pub fn fines_owed(&self, faction: usize) -> f64 {
        self.trade
            .record
            .iter()
            .find(|r| r.faction == faction)
            .map_or(0.0, |r| r.owed)
    }

    /// Notoriety of the player in a jurisdiction: its own record, or the share of the serious
    /// crimes committed in the jurisdictions it has a treaty with. Read-only (fades by date).
    pub fn notoriety_in(&self, faction: usize) -> f32 {
        let now = self.time;
        let fade = |r: &Record, v: f32| {
            (v - ((now - r.t).max(0.0) / DAY) as f32 * NOTORIETY_DECAY).max(0.0)
        };
        let own = self
            .trade
            .record
            .iter()
            .find(|r| r.faction == faction)
            .map_or(0.0, |r| fade(r, r.notoriety));
        let abroad = self
            .trade
            .record
            .iter()
            .filter(|r| r.faction != faction && self.extradition_treaty(r.faction, faction))
            .map(|r| fade(r, r.grave) * EXTRADITION_SHARE)
            .fold(0.0, f32::max);
        own.max(abroad)
    }

    /// The player does `act` in the space of `faction` (the local territory when None). Where
    /// the act is illegal and seen, a fine is owed and the notoriety rises; returns the offence.
    pub fn offend(&mut self, faction: Option<usize>, act: &str) -> Option<Offence> {
        let f = faction.or(self.faction)?;
        let law = self.data.law_index(act)?;
        if !self.laws_in_force(f).contains(&law) {
            return None;
        }
        let seen = if self.docked.is_some() {
            SEEN_DOCKED
        } else {
            SEEN_AWAY
        };
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            law as i64,
            (self.time * 10.0) as i64,
            f as i64,
        ));
        if rng.f64() > seen {
            return None;
        }
        let l = self.data.laws[law].clone();
        let i = self.record_index(f);
        let sev = f32::from(l.severity);
        let r = &mut self.trade.record[i];
        r.owed += l.fine;
        r.notoriety += sev;
        if l.severity >= 2 {
            r.grave += sev;
        }
        self.change_reputation(Some(f), -REP_PER_SEVERITY * sev);
        let people = self.data.factions[f].name.clone();
        self.events.push(Event::MarketNews(format!(
            "Loi de {people} : {} ({}). Amende due : {:.0} CR.",
            l.name.to_lowercase(),
            l.text.to_lowercase(),
            l.fine
        )));
        Some(Offence { law, fine: l.fine })
    }

    /// Pay the fines owed in a jurisdiction (from anywhere: a bank transfer).
    pub fn pay_fines(&mut self, faction: usize) -> Result<f64, &'static str> {
        let owed = self.fines_owed(faction);
        if owed <= 0.0 {
            return Err("aucune amende due");
        }
        if self.credits < owed {
            return Err("fonds insuffisants");
        }
        self.credits -= owed;
        let i = self.record_index(faction);
        let r = &mut self.trade.record[i];
        r.owed = 0.0;
        r.notoriety *= 0.5;
        r.grave *= 0.5;
        Ok(owed)
    }

    /// Cost of a bribe in a jurisdiction, and its chance (the less honourable the people, the
    /// more open to it).
    pub fn bribe_terms(&self, faction: usize) -> (f64, f64) {
        let d = &self.data;
        let values = d.ethos.profile(Some(d.factions[faction].id.as_str()));
        let honour = d
            .ethos
            .axis("honneur")
            .and_then(|a| values.get(a))
            .copied()
            .unwrap_or(0.0);
        (
            600.0 + 800.0 * f64::from(self.notoriety_in(faction)),
            (0.5 - 0.5 * honour).clamp(0.05, 0.9),
        )
    }

    /// Bribe the officials: on success the record is wiped, on failure the money is lost and
    /// the attempt is a new crime.
    pub fn bribe_officials(&mut self, faction: usize) -> Result<bool, &'static str> {
        let (cost, chance) = self.bribe_terms(faction);
        if self.notoriety_in(faction) <= 0.0 && self.fines_owed(faction) <= 0.0 {
            return Err("rien a effacer");
        }
        if self.credits < cost {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            faction as i64,
            (self.time * 10.0) as i64,
            77,
        ));
        let ok = rng.f64() < chance;
        let i = self.record_index(faction);
        let r = &mut self.trade.record[i];
        if ok {
            r.owed = 0.0;
            r.notoriety = 0.0;
            r.grave = 0.0;
        } else {
            r.notoriety += 1.0;
            r.owed += 500.0;
            self.change_reputation(Some(faction), -REP_PER_SEVERITY);
        }
        Ok(ok)
    }

    /// Fee of a lawyer in a jurisdiction.
    pub fn lawyer_fee(&self, faction: usize) -> f64 {
        400.0 + 0.1 * self.fines_owed(faction)
    }

    /// A lawyer halves the fines and the notoriety.
    pub fn hire_lawyer(&mut self, faction: usize) -> Result<(), &'static str> {
        if self.fines_owed(faction) <= 0.0 && self.notoriety_in(faction) <= 0.0 {
            return Err("pas d'affaire en cours");
        }
        let fee = self.lawyer_fee(faction);
        if self.credits < fee {
            return Err("fonds insuffisants");
        }
        self.credits -= fee;
        let i = self.record_index(faction);
        let r = &mut self.trade.record[i];
        r.owed *= 0.5;
        r.notoriety *= 0.5;
        r.grave *= 0.5;
        Ok(())
    }

    /// Whether a warrant closes the docks of the jurisdiction to the player.
    pub fn has_warrant(&self, faction: usize) -> bool {
        self.notoriety_in(faction) >= WARRANT
    }

    /// The patrol at the docks: who is known here may be arrested, the fines owed are seized
    /// and, for a crime committed elsewhere (extradition), a fine is added.
    pub fn law_patrol(&mut self) -> Option<f64> {
        let f = self.faction?;
        let n = self.notoriety_in(f);
        let owed = self.fines_owed(f);
        if n <= 0.0 && owed <= 0.0 {
            return None;
        }
        let aggr = self.data.factions[f].aggression;
        let p = (0.25 * f64::from(n) + 0.3 * aggr).min(0.95);
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            f as i64,
            (self.time * 10.0) as i64,
            83,
        ));
        if rng.f64() > p {
            return None;
        }
        let extra = if self
            .trade
            .record
            .iter()
            .find(|r| r.faction == f)
            .is_none_or(|r| r.notoriety < n)
        {
            500.0 * f64::from(n)
        } else {
            0.0
        };
        let take = (owed + extra).min(self.credits);
        self.credits -= take;
        let i = self.record_index(f);
        let r = &mut self.trade.record[i];
        r.owed = (owed + extra - take).max(0.0);
        r.notoriety = r.notoriety.max(n) * 0.5;
        r.grave *= 0.5;
        let people = self.data.factions[f].name.clone();
        self.events.push(Event::MarketNews(format!(
            "Arrestation par la police de {people} : {take:.0} CR saisis."
        )));
        Some(take)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn sim() -> Sim {
        let mut s = new_sim(5, data());
        s.credits = 1.0e6;
        s
    }

    fn fac(s: &Sim, id: &str) -> usize {
        s.data.faction_index(id).expect("faction")
    }

    /// Offend until seen (the act may go unseen away from a station).
    fn offend_seen(s: &mut Sim, f: usize, act: &str) -> Offence {
        for k in 0..200 {
            s.time = f64::from(k) * 7.0;
            if let Some(o) = s.offend(Some(f), act) {
                return o;
            }
        }
        panic!("never seen");
    }

    #[test]
    fn the_local_law_follows_the_mores_of_the_jurisdiction() {
        let s = sim();
        let (verdant, concorde) = (fac(&s, "verdant"), fac(&s, "concorde"));
        // Where the body is sacred, cloning is a crime; among the open it is not.
        assert!(s.is_illegal(verdant, "clonage"));
        assert!(!s.is_illegal(concorde, "clonage"));
        // Some laws hold everywhere.
        assert!(s.is_illegal(verdant, "meurtre") && s.is_illegal(concorde, "meurtre"));
        assert!(!s.is_illegal(concorde, "inconnue"));
    }

    #[test]
    fn a_legal_act_costs_nothing_and_an_illegal_one_a_fine_and_notoriety() {
        let mut s = sim();
        let (verdant, concorde) = (fac(&s, "verdant"), fac(&s, "concorde"));
        for k in 0..50 {
            s.time = f64::from(k);
            assert!(s.offend(Some(concorde), "clonage").is_none());
        }
        assert_eq!(s.notoriety_in(concorde), 0.0);
        let rep = s.reputation_with(Some(verdant));
        let o = offend_seen(&mut s, verdant, "clonage");
        assert_eq!(o.fine, 800.0);
        assert_eq!(s.fines_owed(verdant), 800.0);
        assert!(s.notoriety_in(verdant) > 0.0);
        assert!(s.reputation_with(Some(verdant)) < rep);
    }

    #[test]
    fn serious_crimes_follow_the_player_into_allied_jurisdictions_only() {
        let mut s = sim();
        let (concorde, helios, hegemonie) =
            (fac(&s, "concorde"), fac(&s, "helios"), fac(&s, "hegemonie"));
        assert!(s.extradition_treaty(concorde, helios));
        assert!(!s.extradition_treaty(concorde, hegemonie));
        offend_seen(&mut s, concorde, "meurtre");
        s.time = 0.0;
        let here = s.notoriety_in(concorde);
        let ally = s.notoriety_in(helios);
        assert!(ally > 0.0 && ally < here, "{ally} < {here}");
        assert_eq!(s.notoriety_in(hegemonie), 0.0, "no treaty, no extradition");
        // A misdemeanour is not extradited.
        let mut t = sim();
        offend_seen(&mut t, concorde, "agression");
        t.time = 0.0;
        assert_eq!(t.notoriety_in(helios), 0.0);
    }

    #[test]
    fn notoriety_fades_with_the_days() {
        let mut s = sim();
        let concorde = fac(&s, "concorde");
        offend_seen(&mut s, concorde, "meurtre");
        let n0 = s.notoriety_in(concorde);
        s.time += 100.0 * DAY;
        assert!(s.notoriety_in(concorde) < n0 - 0.9);
        s.time += 400.0 * DAY;
        assert_eq!(s.notoriety_in(concorde), 0.0);
    }

    #[test]
    fn fines_can_be_paid_and_a_lawyer_halves_them() {
        let mut s = sim();
        let verdant = fac(&s, "verdant");
        offend_seen(&mut s, verdant, "prelevement");
        let owed = s.fines_owed(verdant);
        s.hire_lawyer(verdant).unwrap();
        assert_eq!(s.fines_owed(verdant), owed * 0.5);
        let credits = s.credits;
        assert_eq!(s.pay_fines(verdant), Ok(owed * 0.5));
        assert_eq!(s.credits, credits - owed * 0.5);
        assert_eq!(s.pay_fines(verdant), Err("aucune amende due"));
        s.credits = 0.0;
        offend_seen(&mut s, verdant, "prelevement");
        assert_eq!(s.pay_fines(verdant), Err("fonds insuffisants"));
    }

    #[test]
    fn a_bribe_works_better_on_the_less_honourable_and_can_fail() {
        let mut s = sim();
        let (errants, helios) = (fac(&s, "errants"), fac(&s, "helios"));
        // Honour: errants are honourable (hard to bribe), helios opportunists.
        assert!(s.bribe_terms(helios).1 > s.bribe_terms(errants).1);
        offend_seen(&mut s, helios, "meurtre");
        let (mut won, mut lost) = (false, false);
        for k in 0..400 {
            s.time = 10_000.0 + f64::from(k) * 13.0;
            s.credits = 1.0e6;
            if s.notoriety_in(helios) <= 0.0 {
                offend_seen(&mut s, helios, "meurtre");
            }
            match s.bribe_officials(helios) {
                Ok(true) => {
                    assert_eq!(s.fines_owed(helios), 0.0);
                    assert_eq!(s.notoriety_in(helios), 0.0);
                    won = true;
                }
                Ok(false) => lost = true,
                Err(_) => {}
            }
            if won && lost {
                break;
            }
        }
        assert!(won && lost, "a bribe sometimes works and sometimes fails");
    }

    #[test]
    fn the_patrol_seizes_what_is_owed_and_a_warrant_is_a_matter_of_notoriety() {
        let mut s = sim();
        let hegemonie = fac(&s, "hegemonie");
        s.faction = Some(hegemonie);
        assert!(s.law_patrol().is_none(), "a clean record is left alone");
        offend_seen(&mut s, hegemonie, "meurtre");
        assert!(s.has_warrant(hegemonie), "a capital crime closes the docks");
        let credits = s.credits;
        let mut seized = None;
        for k in 0..100 {
            s.time = f64::from(k) * 3.0;
            if let Some(t) = s.law_patrol() {
                seized = Some(t);
                break;
            }
        }
        let t = seized.expect("the patrol caught the player");
        assert!(t >= 2500.0 && s.credits == credits - t);
        assert_eq!(s.fines_owed(hegemonie), 0.0);
    }

    #[test]
    fn every_law_of_the_data_is_in_force_somewhere_and_some_are_local() {
        let s = sim();
        let n = s.data.factions.len();
        let counts: Vec<usize> = (0..s.data.laws.len())
            .map(|l| (0..n).filter(|&f| s.laws_in_force(f).contains(&l)).count())
            .collect();
        assert!(counts.iter().all(|&c| c >= 1));
        assert!(counts.iter().any(|&c| c < n), "some laws are local");
    }
}
