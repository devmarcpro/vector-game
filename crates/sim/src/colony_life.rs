//! The life of a colony: what the people do, drawn from what the colony is (its mood, its
//! policies, its mores, its services and its research) and told in its journal, which feeds the
//! news (`news.rs`). Nothing is scripted: a strike is the anger of unhappy workers under heavy
//! hours, a fête the joy of a happy colony with places to enjoy, a birth wave the surplus of a
//! colony with room. Epidemics are in `epidemic.rs`; famines, riots and catastrophes are told
//! where they are computed.
use crate::colony::Colony;
use crate::crew::DAY;
use crate::planet::unit;
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_data::GameData;

/// Births (cumulated) that make a line in the journal.
pub const BIRTH_LINE: f64 = 10.0;
/// Research points between two announced discoveries.
pub const DISCOVERY_STEP: f64 = 250.0;
/// Chance per day of a strike for a colony at the bottom of its mood.
const STRIKE_RATE: f64 = 0.05;
/// Chance per day of a fête for a colony with all it needs to enjoy itself.
const FETE_RATE: f64 = 0.01;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ColonyLife {
    /// Births since the last line of the journal.
    pub births: f64,
    /// Days of strike left.
    pub strike_days: f64,
    /// Days of fête left.
    pub fete_days: f64,
    /// Years of the colony already celebrated, and discoveries already announced.
    pub anniversary: i64,
    pub discoveries: i64,
}

impl Colony {
    /// Mores of the colony on axis `id` of `ethos.ron` (0 when unknown).
    fn mores(&self, data: &GameData, id: &str) -> f64 {
        data.ethos
            .axis(id)
            .and_then(|a| self.culture.get(a))
            .copied()
            .unwrap_or(0.0)
    }

    /// Production multiplier of the labour: 1, or a half while the workers are on strike. Heavy
    /// hours and a low mood start strikes; free-spirited mores make them likelier and martial
    /// law keeps them down.
    pub(crate) fn strike_factor(&mut self, data: &GameData, now: f64, days: f64) -> f64 {
        if self.life.strike_days > 0.0 {
            self.life.strike_days -= days;
            if self.life.strike_days <= 0.0 {
                self.log_line(now, "La greve prend fin : les ateliers rouvrent.");
                return 1.0;
            }
            return 0.5;
        }
        if self.pop < 20.0 {
            return 1.0;
        }
        let (_, work_mult, _) = self.policy_level(data, "work");
        let anger = (0.5 - self.happiness).max(0.0)
            + 0.3 * work_mult.max(0.0)
            + 0.1 * (-self.mores(data, "autorite")).max(0.0);
        let martial = self.policy_value(data, "security") >= 2;
        let p = STRIKE_RATE * anger * 2.0 * days * if martial { 0.4 } else { 1.0 };
        let key = hash_coord(self.system_seed, i64::from(self.id), (now / DAY) as i64, 33);
        if unit(key) < p {
            self.life.strike_days = 3.0 + 4.0 * unit(hash_coord(key, 7, 7, 7));
            self.log_line(now, "Greve : les travailleurs posent leurs outils.");
            return 0.5;
        }
        1.0
    }

    /// Mood bonus of a fête running.
    pub fn fete_mood(&self) -> f64 {
        if self.life.fete_days > 0.0 {
            0.06
        } else {
            0.0
        }
    }

    /// Births, anniversaries, fêtes and discoveries of the tick; told in the journal.
    /// `rec` is the coverage of recreation services, 0..1.
    pub(crate) fn life_events(&mut self, now: f64, days: f64, births: f64, rec: f64) {
        self.life.births += births;
        let mut guard = 0;
        while self.life.births >= BIRTH_LINE && guard < 3 {
            guard += 1;
            self.life.births -= BIRTH_LINE;
            let pop = self.pop;
            self.log_line(
                now,
                format!("Naissances : dix bebes de plus, la colonie compte {pop:.0} ames."),
            );
        }
        let years = ((now - self.founded) / (365.0 * DAY)).floor() as i64;
        if years > self.life.anniversary {
            self.life.anniversary = years;
            if self.happiness > 0.4 {
                self.life.fete_days = 2.0;
                self.log_line(
                    now,
                    format!("Fete de la fondation : {years} an(s) deja, la colonie celebre."),
                );
            }
        }
        if self.life.fete_days > 0.0 {
            self.life.fete_days -= days;
        } else if self.happiness > 0.6 && self.pop > 30.0 {
            let key = hash_coord(self.system_seed, i64::from(self.id), (now / DAY) as i64, 35);
            if unit(key) < FETE_RATE * rec * days * 4.0 {
                self.life.fete_days = 2.0;
                self.log_line(
                    now,
                    "Fete : la colonie se rassemble et chante jusqu'au matin.",
                );
            }
        }
        let marks = (self.research / DISCOVERY_STEP).floor() as i64;
        if marks > self.life.discoveries {
            self.life.discoveries = marks;
            let r = self.research;
            self.log_line(
                now,
                format!(
                    "Decouverte : les chercheurs publient une percee ({r:.0} points de recherche)."
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::tests::founded;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn lines(c: &Colony, word: &str) -> usize {
        c.log.iter().filter(|(_, l)| l.contains(word)).count()
    }

    #[test]
    fn unhappy_overworked_colonies_strike_and_a_strike_stops_production() {
        let d = data();
        let wp = d.policy_index("work").unwrap();
        let strikes = |happy: f64, overtime: bool| {
            let (mut s, ci) = founded();
            if overtime {
                s.colonies[ci].policies[wp] = 2;
            }
            let mut n = 0;
            for _ in 0..400 {
                let c = &mut s.colonies[ci];
                c.pop = 100.0;
                c.happiness = happy;
                let before = c.life.strike_days;
                c.strike_factor(&d, s.time, 1.0);
                if before == 0.0 && c.life.strike_days > 0.0 {
                    n += 1;
                }
                s.time += 20.0 * DAY;
                c.life.strike_days = 0.0;
            }
            n
        };
        let (angry, content) = (strikes(0.1, true), strikes(0.9, false));
        assert!(angry >= 3, "the angry strike: {angry}");
        assert_eq!(content, 0, "the content do not");
        // While it lasts, the colony produces half.
        let (mut s, ci) = founded();
        s.colonies[ci].life.strike_days = 4.0;
        assert_eq!(s.colonies[ci].strike_factor(&d, s.time, 1.0), 0.5);
        s.colonies[ci].life.strike_days = 0.5;
        assert_eq!(s.colonies[ci].strike_factor(&d, s.time, 1.0), 1.0);
        assert!(lines(&s.colonies[ci], "greve") >= 1);
    }

    #[test]
    fn births_discoveries_and_anniversaries_are_told_and_feasts_lift_the_mood() {
        let (mut s, ci) = founded();
        let c = &mut s.colonies[ci];
        c.pop = 100.0;
        c.happiness = 0.8;
        c.life_events(c.founded + DAY, 1.0, 25.0, 0.0);
        assert_eq!(lines(c, "Naissances"), 2, "ten births make a line");
        c.research = 600.0;
        c.life_events(c.founded + 2.0 * DAY, 1.0, 0.0, 0.0);
        assert_eq!(lines(c, "Decouverte"), 1);
        c.life_events(c.founded + 3.0 * DAY, 1.0, 0.0, 0.0);
        assert_eq!(lines(c, "Decouverte"), 1, "told once");
        assert_eq!(c.fete_mood(), 0.0);
        c.life_events(c.founded + 400.0 * DAY, 1.0, 0.0, 0.0);
        assert_eq!(lines(c, "fondation"), 1);
        assert!(c.fete_mood() > 0.0, "the anniversary is a fete");
        // With places to enjoy, happy colonies hold fêtes by themselves.
        let (mut t, cj) = founded();
        let c = &mut t.colonies[cj];
        c.pop = 100.0;
        c.happiness = 0.9;
        for d in 0..300 {
            c.life.fete_days = 0.0;
            c.life_events(c.founded + f64::from(d) * DAY, 1.0, 0.0, 1.0);
        }
        assert!(lines(c, "Fete :") >= 1);
    }

    #[test]
    fn a_colonys_own_life_enters_the_journal_while_it_runs() {
        let (mut s, ci) = founded();
        let d = Arc::clone(&s.data);
        for _ in 0..90 {
            let c = &mut s.colonies[ci];
            c.pop = c.pop.max(60.0);
            for (id, kg) in [
                ("food", 4000.0),
                ("water", 4000.0),
                ("consumer_goods", 400.0),
            ] {
                c.storage[d.resource_index(id).unwrap()] = kg;
            }
            s.time += 10.0 * DAY;
            s.update_colonies();
        }
        let c = &s.colonies[ci];
        assert!(
            lines(c, "Naissances") + lines(c, "Fete") + lines(c, "Decouverte") >= 1,
            "{:?}",
            c.log
        );
    }
}
