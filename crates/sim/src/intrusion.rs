//! Intrusions: now and then somebody tries to break into the ship's systems (the AI). They
//! siphon credits, take over the implants of the crew for a few days, or flood the ship with
//! messages that stress everybody. A good engineer aboard makes them rarer. Deterministic from
//! the system seed and the day.
use crate::bourse::DAY;
use crate::{Event, Sim};
use vector_core::{hash_coord, Rng};

/// Base chance per day of an intrusion attempt (scaled by the "event frequency" rule).
pub const INTRUSION_RATE: f64 = 0.012;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrusion {
    /// Credits siphoned.
    Siphon,
    /// The hackable implants of the crew taken for a few days.
    Implants,
    /// A flood of messages: stress for everybody aboard.
    Flood,
}

impl Sim {
    /// How hard the intrusions are to land: the best engineer aboard lowers their chance.
    fn firewall(&self) -> f64 {
        let skill = self.data.skill_index("ingenierie").map_or(0.0, |s| {
            self.crew
                .alive()
                .map(|p| f64::from(p.skills[s]))
                .fold(0.0, f64::max)
        });
        1.0 / (1.0 + 0.15 * skill)
    }

    /// Chance of an attempt on a given day.
    pub fn intrusion_chance(&self) -> f64 {
        INTRUSION_RATE * self.rules.event_freq * self.firewall()
    }

    /// Carry out an intrusion; returns what the AI reports.
    pub fn apply_intrusion(&mut self, kind: Intrusion) -> String {
        match kind {
            Intrusion::Siphon => {
                let taken = (self.credits * 0.03).clamp(0.0, 2_000.0);
                self.credits -= taken;
                format!("INTRUSION : {taken:.0} CR siphonnes sur le compte du bord.")
            }
            Intrusion::Implants => {
                let mut n = 0;
                for i in 0..self.crew.members.len() {
                    n += self.hack_implants(i, 2.0);
                }
                if n == 0 {
                    "INTRUSION repoussee : personne n'a d'implant a pirater.".to_string()
                } else {
                    format!("INTRUSION : {n} implants pris en main pendant 2 jours.")
                }
            }
            Intrusion::Flood => {
                for p in self.crew.members.iter_mut().filter(|p| p.alive) {
                    p.stress = (p.stress + 0.2).min(1.0);
                }
                "INTRUSION : messages injectes dans toutes les cabines, l'equipage est secoue."
                    .to_string()
            }
        }
    }

    /// Roll the days that passed since the last call.
    pub(crate) fn intrusion_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.intrusion_day;
        self.intrusion_day = day;
        if last == i64::MIN || day <= last || day - last > 3 || self.rules.event_freq <= 0.0 {
            return;
        }
        for d in (last + 1)..=day {
            let mut rng = Rng::new(hash_coord(self.system.seed, d, 0, 37));
            if rng.f64() >= self.intrusion_chance() {
                continue;
            }
            let kind = match rng.next_u64() % 3 {
                0 => Intrusion::Siphon,
                1 => Intrusion::Implants,
                _ => Intrusion::Flood,
            };
            let text = self.apply_intrusion(kind);
            self.events.push(Event::MarketNews(text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")))
    }

    #[test]
    fn each_kind_of_intrusion_does_what_it_says() {
        let mut s = sim();
        s.credits = 10_000.0;
        let msg = s.apply_intrusion(Intrusion::Siphon);
        assert!(msg.contains("siphonnes") && s.credits < 10_000.0 && s.credits >= 8_000.0);
        let stress = s.crew.members[0].stress;
        s.apply_intrusion(Intrusion::Flood);
        assert!(s.crew.members[0].stress > stress);
        // Without implants the attempt fails; with one it is taken over.
        assert!(s.apply_intrusion(Intrusion::Implants).contains("repoussee"));
        let eye = s.data.cyber_index("oeil").unwrap();
        s.crew.members[0].implants.push(eye);
        assert!(s
            .apply_intrusion(Intrusion::Implants)
            .contains("1 implants"));
        assert!(s.crew.members[0].hacked);
    }

    #[test]
    fn intrusions_follow_the_rule_and_a_good_engineer_makes_them_rarer() {
        let mut s = sim();
        let base = s.intrusion_chance();
        assert!((base - INTRUSION_RATE * firewall_of(&s)).abs() < 1e-12);
        s.rules.event_freq = 0.0;
        assert_eq!(s.intrusion_chance(), 0.0, "the rule switches them off");
        s.rules.event_freq = 1.0;
        // An excellent engineer.
        let eng = s.data.skill_index("ingenierie").unwrap();
        s.crew.members[0].skills[eng] = 10.0;
        assert!(s.intrusion_chance() < INTRUSION_RATE * 0.5);
        // Over many days with a huge frequency they do happen, and are reported.
        s.rules.event_freq = 100.0;
        s.credits = 1.0e6;
        s.time = 50.0 * DAY;
        s.intrusion_tick();
        for _ in 0..30 {
            s.time += DAY;
            s.intrusion_tick();
        }
        assert!(
            s.events
                .iter()
                .filter(|e| matches!(e, Event::MarketNews(t) if t.starts_with("INTRUSION")))
                .count()
                >= 5
        );
    }

    fn firewall_of(s: &Sim) -> f64 {
        s.firewall()
    }
}
