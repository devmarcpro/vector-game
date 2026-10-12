//! Archaeology. Ruins and wrecks hold artefacts whose history is the chronicle of their sector
//! (`history::provenance`): who made them, when, and why the place fell. Digging on foot reveals
//! them (a careful dig keeps the context, a pillage takes more and destroys it); carbon dating tells
//! how old a thing really is, and the better the AI's analysis budget the smaller the error, which
//! is how forgeries (made later than they claim) are caught; translating texts, maps and lost
//! technologies is work for the translation budget; the finds can be sold to collectors (who pay
//! most and ask no questions), given to a museum (documented finds only) or handed back to the
//! heirs of those who made them, which the people of the sector remember.
use crate::bourse::DAY;
use crate::history::{self, year_at};
use crate::mind::{T_ANALYSE, T_TRADUCTION};
use crate::politics::Cell;
use crate::{planet::unit, Sim};
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;

/// Credits the reagents of a dating cost.
pub const DATING_COST: f64 = 40.0;
/// Share of the value a museum pays, and of the apparent value a collector pays.
pub const MUSEUM_SHARE: f64 = 0.5;
pub const COLLECTOR_SHARE: f64 = 0.8;
/// A pillaged find lost its context: worth this much of its value; a documented one this much.
pub const PILLAGE_FACTOR: f64 = 0.6;
pub const DOCUMENTED_FACTOR: f64 = 1.3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Artefact {
    pub id: u32,
    /// Index in `data.history.artefacts`.
    pub kind: usize,
    pub cell: Cell,
    pub builder: Option<usize>,
    pub built: i32,
    pub fallen: i32,
    pub cause: Option<usize>,
    /// A forgery: made `forged_years` ago only, but looks as old as `built`.
    pub fake: bool,
    /// Year it was really made.
    pub made: i32,
    pub documented: bool,
    /// Best dating so far: measured age (years) and its error bar.
    pub measured: Option<(f64, f64)>,
    pub progress: f64,
    pub translated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dest {
    Collector,
    Museum,
    Heirs,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Archive {
    pub artefacts: Vec<Artefact>,
    pub next_id: u32,
    /// Dig carefully (slower, keeps the context) rather than pillage.
    pub careful: bool,
    pub dug: Vec<u64>,
    pub restituted: u32,
    pub pillaged: u32,
}

impl Artefact {
    /// How old it claims to be, years.
    pub fn apparent_age(&self, now: f64) -> f64 {
        now - f64::from(self.built)
    }

    pub fn true_age(&self, now: f64) -> f64 {
        now - f64::from(self.made)
    }

    /// Error bar (years) of a dating at an analysis work rate.
    pub fn dating_sigma(&self, now: f64, rate: f64) -> f64 {
        (0.15 * self.true_age(now) + 20.0) / (1.0 + rate.max(0.0))
    }

    /// Noise of a dating, -1..1, a pure function of the find.
    fn noise(&self) -> f64 {
        2.0 * unit(hash_coord(
            0xDA7E,
            i64::from(self.id),
            i64::from(self.made),
            1,
        )) - 1.0
    }

    /// Whether the best dating so far proves it younger than it looks (more than 2 error bars).
    pub fn suspect(&self, now: f64) -> bool {
        self.measured
            .is_some_and(|(m, sigma)| (self.apparent_age(now) - m).abs() > 2.0 * sigma)
    }
}

impl Sim {
    fn artefact_value(&self, a: &Artefact) -> f64 {
        let def = &self.data.history.artefacts[a.kind];
        let now = year_at(self.time);
        let rarity = 1.0 + a.apparent_age(now) / 1000.0;
        let ctx = if a.documented {
            DOCUMENTED_FACTOR
        } else {
            PILLAGE_FACTOR
        };
        def.value * rarity * ctx
    }

    /// What the find looks worth (a forgery looks as good as the real thing).
    pub fn artefact_price(&self, i: usize) -> f64 {
        self.artefact_value(&self.archive.artefacts[i])
    }

    /// What it is really worth: nothing, for a forgery.
    pub fn artefact_true_value(&self, i: usize) -> f64 {
        let a = &self.archive.artefacts[i];
        if a.fake {
            self.artefact_value(a) * 0.02
        } else {
            self.artefact_value(a)
        }
    }

    /// Search a site on foot: the finds of a ruin or a wreck, with the provenance of the place.
    /// `site` is the id of the site, `key` its own hash. Returns what was found.
    pub fn dig_site(&mut self, site: &str, key: u64) -> Vec<usize> {
        let (n_base, p_fake, theme) = match site {
            "ruine" => (2.0, 0.12, "ruine"),
            "epave" => (1.0, 0.04, "epave"),
            "station_ancienne" => (1.0, 0.08, "station_ancienne"),
            _ => return Vec::new(),
        };
        if self.archive.dug.contains(&key) {
            return Vec::new();
        }
        self.archive.dug.push(key);
        let seed = self.galaxy.seed;
        let cell = self.here_cell();
        let holder = crate::politics::faction_of(&self.data, &self.galaxy, cell);
        let prov = history::provenance(&self.data, seed, cell, holder, theme, key);
        let careful = self.archive.careful;
        let rate = self.mind_rate(T_ANALYSE);
        // A careful dig takes fewer things out; a good analysis budget spots one more.
        let mut n = n_base * if careful { 0.7 } else { 1.3 } + if rate > 1.5 { 1.0 } else { 0.0 };
        let mut found = Vec::new();
        let n_kinds = self.data.history.artefacts.len();
        let u = |s: i64| unit(hash_coord(seed ^ 0xD16, key as i64, s, 9));
        let mut j = 0;
        while n >= 1.0 || (n > 0.0 && u(100 + j) < n) {
            n -= 1.0;
            let kind = (u(j) * n_kinds as f64) as usize % n_kinds;
            let fake = u(10 + j) < p_fake;
            let now = year_at(self.time);
            let made = if fake {
                // Forged 150 to 400 years ago, with old materials.
                (now - 150.0 - 250.0 * u(20 + j)).floor() as i32
            } else {
                prov.built
            };
            let a = Artefact {
                id: self.archive.next_id,
                kind,
                cell,
                builder: prov.builder,
                built: prov.built,
                fallen: prov.fallen,
                cause: prov.cause,
                fake,
                made,
                documented: careful,
                measured: None,
                progress: 0.0,
                translated: self.data.history.artefacts[kind].difficulty <= 0.0,
            };
            self.archive.next_id += 1;
            self.archive.artefacts.push(a);
            found.push(self.archive.artefacts.len() - 1);
            j += 1;
        }
        if !found.is_empty() {
            // A discovery is news: where, whose, since when (the provenance of the place).
            let place = self.place_name(self.here_pos);
            let what = match theme {
                "ruine" => "Ruines",
                "epave" => "Epave",
                _ => "Station ancienne",
            };
            let text = format!(
                "{what} explorees pres de {place} : {} piece(s) remontent a l'an {}. {}",
                found.len(),
                prov.built,
                history::provenance_line(&self.data, &prov)
            );
            self.news_push(self.time, 5, 0.0, text);
        }
        if !careful && !found.is_empty() {
            // Pillage destroys the context and the people of the place resent it.
            self.archive.pillaged += found.len() as u32;
            self.change_reputation(holder, -0.5 * found.len() as f32);
        }
        found
    }

    /// Date a find: the error bar depends on the analysis budget; a forgery older than its error
    /// bars gives itself away.
    pub fn date_artefact(&mut self, i: usize) -> Result<String, &'static str> {
        if i >= self.archive.artefacts.len() {
            return Err("artefact inconnu");
        }
        if self.credits < DATING_COST {
            return Err("credits insuffisants");
        }
        self.credits -= DATING_COST;
        let now = year_at(self.time);
        let rate = self.mind_rate(T_ANALYSE);
        let a = &mut self.archive.artefacts[i];
        let sigma = a.dating_sigma(now, rate);
        // Only a better dating replaces the previous one.
        if a.measured.is_none_or(|(_, s)| sigma < s) {
            a.measured = Some((a.true_age(now) + sigma * a.noise(), sigma));
        }
        let (m, s) = a.measured.expect("set above");
        let suspect = a.suspect(now);
        Ok(format!(
            "Datation : {m:.0} ans (+/- {s:.0}). {}",
            if suspect {
                "CONTREFACON PROBABLE."
            } else {
                "Rien d'anormal."
            }
        ))
    }

    /// Translation work, once per day-chunk of the mind: texts, maps, lost technologies.
    pub(crate) fn archive_tick(&mut self, days: f64) {
        let rate = self.mind_rate(T_TRADUCTION);
        let Some(i) = self.archive.artefacts.iter().position(|a| !a.translated) else {
            return;
        };
        let def = self.data.history.artefacts[self.archive.artefacts[i].kind].clone();
        let a = &mut self.archive.artefacts[i];
        a.progress += rate * days / def.difficulty.max(0.1);
        if a.progress < 1.0 {
            return;
        }
        a.translated = true;
        // Taking a find apart is the work of engineers: with the technology, the lessons double.
        let reverse =
            self.capacity_mask() & crate::colony_works::cap_bit("reverse_engineering") != 0;
        if !self.archive.artefacts[i].fake {
            self.findings.insert(format!("artefact:{}", def.id));
        }
        let a = &mut self.archive.artefacts[i];
        let (fake, fallen, cause, builder, cell) = (a.fake, a.fallen, a.cause, a.builder, a.cell);
        let t = self.time;
        let who = builder.map_or("un peuple oublie", |f| self.data.factions[f].name.as_str());
        let text = if fake {
            // A forgery says what the forger wanted: it is rich and has no truth to offer.
            format!(
                "{} traduit : un texte fluide, trop beau pour etre vrai.",
                def.name
            )
        } else {
            match def.id.as_str() {
                "technologie" | "outil" => {
                    let pts = def.value / 10.0 * if reverse { 2.0 } else { 1.0 };
                    self.research_pool += pts;
                    format!(
                        "{} etudiee : {pts:.0} points de recherche (ouvrage de {who}).",
                        def.name
                    )
                }
                "carte" => {
                    self.credits += def.value / 10.0;
                    format!(
                        "{} recalee : routes de {who}, une prime de cartographie est versee.",
                        def.name
                    )
                }
                _ => format!(
                    "{} traduit : en l'an {fallen}, {}.",
                    def.name,
                    cause.map_or(
                        "le peuple de ce lieu s'en est alle".to_string(),
                        |c| format!(
                            "{} a eu raison de {who}",
                            self.data.history.causes[c].name.to_lowercase()
                        )
                    )
                ),
            }
        };
        let _ = cell;
        self.crew.log_event(t, text);
    }

    /// Get rid of a find. Collectors pay most and ask no questions (a forgery may pass), museums
    /// only take documented finds, giving back to the heirs earns their goodwill.
    pub fn dispose_artefact(&mut self, i: usize, dest: Dest) -> Result<String, &'static str> {
        if i >= self.archive.artefacts.len() {
            return Err("artefact inconnu");
        }
        let a = self.archive.artefacts[i].clone();
        let heirs = crate::politics::faction_of(&self.data, &self.galaxy, a.cell);
        let now = year_at(self.time);
        let name = self.data.history.artefacts[a.kind].name.clone();
        let msg = match dest {
            Dest::Collector => {
                let price = self.artefact_price(i) * COLLECTOR_SHARE;
                if a.fake {
                    // The buyer has its own instruments.
                    let sees = unit(hash_coord(0xFA4E, i64::from(a.id), 0, 3)) < 0.5;
                    if sees || a.suspect(now) {
                        self.change_reputation(heirs, -2.0);
                        return Err("le collectionneur flaire la contrefacon : fraude denoncee");
                    }
                    self.credits += price;
                    self.archive.artefacts.remove(i);
                    return Ok(format!(
                        "{name} vendu {price:.0} CR a un collectionneur trop pressé."
                    ));
                }
                self.credits += price;
                if !a.documented {
                    self.change_reputation(heirs, -(price / 4000.0).clamp(0.2, 3.0) as f32);
                }
                format!("{name} vendu {price:.0} CR a un collectionneur.")
            }
            Dest::Museum => {
                if !a.documented {
                    return Err("le musee refuse ce qui n'a pas de contexte");
                }
                if a.fake || a.suspect(now) {
                    return Err("le musee ne prend pas les contrefacons");
                }
                let price = self.artefact_price(i) * MUSEUM_SHARE;
                self.credits += price;
                self.change_reputation(heirs, 0.3);
                format!("{name} verse au musee : {price:.0} CR et la gratitude des savants.")
            }
            Dest::Heirs => {
                if a.fake {
                    self.archive.artefacts.remove(i);
                    return Ok(format!(
                        "{name} rendu : les heritiers rient de la contrefacon."
                    ));
                }
                let gain = (self.artefact_price(i) / 1500.0).clamp(0.2, 3.0) as f32;
                self.change_reputation(heirs, gain);
                self.archive.restituted += 1;
                format!("{name} restitue aux heritiers : leur reconnaissance ({gain:+.1}).")
            }
        };
        self.archive.artefacts.remove(i);
        Ok(msg)
    }

    /// The lines of the archives of the sector the ship is in.
    pub fn sector_history(&self) -> Vec<String> {
        history::lines(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            self.here_cell(),
            self.time,
        )
    }

    pub fn toggle_careful_dig(&mut self) {
        self.archive.careful = !self.archive.careful;
    }

    /// Days since the find was made, for display.
    pub fn archive_years_now(&self) -> f64 {
        year_at(self.time)
    }
}

/// One line per find for the screen.
pub fn artefact_line(sim: &Sim, i: usize) -> String {
    let a = &sim.archive.artefacts[i];
    let def = &sim.data.history.artefacts[a.kind];
    let now = year_at(sim.time);
    let who = a
        .builder
        .map_or("PEUPLE OUBLIE", |f| sim.data.factions[f].name.as_str());
    let dating = match a.measured {
        Some((m, s)) => format!("age mesure {m:.0}+/-{s:.0}"),
        None => "non date".to_string(),
    };
    let flag = if a.suspect(now) { " CONTREFACON?" } else { "" };
    let tr = if a.translated {
        "".to_string()
    } else {
        format!(" traduction {:.0} %", a.progress * 100.0)
    };
    format!(
        "{} - {who} an {} - {dating}{flag}{tr} - {} - {:.0} CR",
        def.name.to_uppercase(),
        a.built,
        if a.documented {
            "DOCUMENTE"
        } else {
            "SANS CONTEXTE"
        },
        sim.artefact_price(i)
    )
}

#[allow(dead_code)]
const _: f64 = DAY;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        s.ship.sys.temp_k = 290.0;
        s
    }

    fn find(s: &mut Sim, fake: bool, kind: &str) -> usize {
        let now = year_at(s.time);
        let k = s.data.artefact_index(kind).unwrap();
        let a = Artefact {
            id: s.archive.next_id,
            kind: k,
            cell: [0, 0, 0],
            builder: Some(0),
            built: (now - 2400.0) as i32,
            fallen: (now - 1500.0) as i32,
            cause: Some(0),
            fake,
            made: if fake {
                (now - 2100.0) as i32
            } else {
                (now - 2400.0) as i32
            },
            documented: true,
            measured: None,
            progress: 0.0,
            translated: s.data.history.artefacts[k].difficulty <= 0.0,
        };
        s.archive.next_id += 1;
        s.archive.artefacts.push(a);
        s.archive.artefacts.len() - 1
    }

    #[test]
    fn a_better_analysis_budget_catches_the_forgery_a_poor_one_lets_pass() {
        let mut s = sim();
        s.credits = 1.0e5;
        let i = find(&mut s, true, "relique");
        let j = find(&mut s, false, "relique");
        // Nothing for analysis: the error bar swallows the 300 years of the forgery.
        s.mind.alloc = [10, 0, 5, 5, 5];
        s.date_artefact(i).unwrap();
        let now = year_at(s.time);
        assert!(
            !s.archive.artefacts[i].suspect(now),
            "a poor dating is fooled"
        );
        // Everything for analysis: the forgery gives itself away; the real one never does.
        s.mind.alloc = [0, 10, 0, 0, 0];
        s.date_artefact(i).unwrap();
        s.date_artefact(j).unwrap();
        assert!(
            s.archive.artefacts[i].suspect(now),
            "{:?}",
            s.archive.artefacts[i]
        );
        assert!(
            !s.archive.artefacts[j].suspect(now),
            "an authentic find is never accused"
        );
        let (_, sigma_good) = s.archive.artefacts[i].measured.unwrap();
        assert!(sigma_good < 150.0);
    }

    #[test]
    fn digs_are_once_only_deterministic_and_carry_the_chronicle() {
        let mut s = sim();
        let a = s.dig_site("ruine", 77);
        assert!(!a.is_empty());
        assert!(s.dig_site("ruine", 77).is_empty(), "a site is dug once");
        assert!(
            s.dig_site("source", 78).is_empty(),
            "not an archaeological site"
        );
        let art = &s.archive.artefacts[a[0]];
        assert!(art.built < art.fallen && f64::from(art.fallen) < year_at(s.time));
        let mut t = sim();
        let b = t.dig_site("ruine", 77);
        assert_eq!(a.len(), b.len());
        assert_eq!(
            s.archive.artefacts, t.archive.artefacts,
            "same find for the same site"
        );
        // A careful dig documents, a pillage does not and costs goodwill.
        let mut c = sim();
        c.archive.careful = true;
        c.dig_site("ruine", 77);
        assert!(c.archive.artefacts.iter().all(|a| a.documented));
        assert!(s.archive.artefacts.iter().all(|a| !a.documented));
        assert!(s.archive.pillaged > 0 && c.archive.pillaged == 0);
    }

    #[test]
    fn translation_takes_the_translation_budget_and_a_lost_technology_feeds_research() {
        let days = |alloc: [u8; 5]| {
            let mut s = sim();
            s.mind.alloc = alloc;
            find(&mut s, false, "technologie");
            let pool = s.research_pool;
            let mut d = 0;
            while !s.archive.artefacts[0].translated && d < 2000 {
                s.time += DAY;
                s.mind_tick();
                d += 1;
            }
            assert!(s.archive.artefacts[0].translated);
            assert!(s.research_pool > pool, "the technology was studied");
            d
        };
        assert!(days([1, 1, 10, 1, 1]) < days([4, 4, 1, 4, 4]));
    }

    #[test]
    fn selling_giving_and_returning_have_different_consequences() {
        let mut s = sim();
        for f in 0..s.data.factions.len() {
            s.set_reputation(f, 0.0);
        }
        let heirs = crate::politics::faction_of(&s.data, &s.galaxy, [0, 0, 0]);
        // A pillaged find sells for money and costs goodwill; the museum refuses it.
        let i = find(&mut s, false, "relique");
        s.archive.artefacts[i].documented = false;
        assert!(s.dispose_artefact(i, Dest::Museum).is_err());
        let (cr, rep) = (s.credits, s.reputation_with(heirs));
        s.dispose_artefact(i, Dest::Collector).unwrap();
        assert!(s.credits > cr);
        if heirs.is_some() {
            assert!(s.reputation_with(heirs) < rep, "pillage is remembered");
        }
        // A documented one goes to the museum for less money and some respect.
        let j = find(&mut s, false, "relique");
        let price = s.artefact_price(j);
        let cr = s.credits;
        s.dispose_artefact(j, Dest::Museum).unwrap();
        assert!((s.credits - cr - price * MUSEUM_SHARE).abs() < 1e-6);
        // Giving back earns goodwill and no money.
        let k = find(&mut s, false, "relique");
        let (cr, rep) = (s.credits, s.reputation_with(heirs));
        s.dispose_artefact(k, Dest::Heirs).unwrap();
        assert_eq!(s.credits, cr);
        if heirs.is_some() {
            assert!(s.reputation_with(heirs) > rep);
        }
        assert_eq!(s.archive.restituted, 1);
        assert!(s.archive.artefacts.is_empty());
    }

    #[test]
    fn a_known_forgery_cannot_be_sold_honestly_and_is_worth_nothing() {
        let mut s = sim();
        s.credits = 1.0e5;
        let i = find(&mut s, true, "relique");
        assert!(s.artefact_true_value(i) < s.artefact_price(i) * 0.05);
        s.mind.alloc = [0, 10, 0, 0, 0];
        s.date_artefact(i).unwrap();
        let cr = s.credits;
        assert!(
            s.dispose_artefact(i, Dest::Collector).is_err(),
            "fraud is denounced"
        );
        assert_eq!(s.credits, cr);
        assert!(s.dispose_artefact(i, Dest::Museum).is_err());
    }
}
