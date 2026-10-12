//! Society: the languages people speak, the culture of a place, the arts, sports and schools.
//! Every culture and every native people speaks one language (`data/society.ron`); languages of a
//! family are half understood, a language drifts with the eras (its words change, what a
//! person learned of it fades unless it is practised). Each crew member has a fluency per
//! language; the ship's AI translates with the translation budget of the mind (never as well as
//! a person). What the crew and the AI understand of the local language sets the price edge at
//! the docks and how far a favour weighs with a native people. A place also has a calendar
//! (the year of its planet, seasons, festivals held by its mores) and a cuisine. The crew makes
//! works of art (valued by quality, fame and the taste of the buyers), studies at schools
//! (skills, languages, arts, with mentors) and the docks hold races and tournaments to bet on.
use crate::bourse::DAY;
use crate::mind::T_TRADUCTION;
use crate::planet::unit;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_world::BodyKind;

/// Share of a language's fluency that carries over to another language of the same family.
pub const KINSHIP: f32 = 0.5;
/// The AI translator never reaches the fluency of a person.
pub const AI_CAP: f32 = 0.9;
/// Fluency gained a day by living among the people who speak it.
pub const PRACTICE: f32 = 0.004;
/// Ceiling of what practice alone can teach.
pub const PRACTICE_CAP: f32 = 0.7;
/// Fluency lost per era since it was last practised.
pub const ERA_FADE: f32 = 0.2;
/// A mentor speeds lessons up by this factor.
pub const MENTOR: f32 = 1.5;
/// Seconds in a year of the calendar of an unknown place.
const EARTH_YEAR: f64 = 365.25 * DAY;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tongue {
    pub person: u32,
    pub lang: u8,
    pub skill: f32,
    /// Era of the language when it was last practised.
    pub era: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Craft {
    pub person: u32,
    pub art: u8,
    pub skill: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lesson {
    pub person: u32,
    pub study: u8,
    /// Language or art index, or skill index, depending on the study.
    pub target: u8,
    pub left: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Work {
    pub id: u64,
    pub art: u8,
    pub author: u32,
    pub author_name: String,
    pub title: String,
    pub quality: f32,
    pub made: f64,
}

/// What the player's society layer remembers (everything else comes from the data and the seed).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SocietyState {
    pub tongues: Vec<Tongue>,
    pub crafts: Vec<Craft>,
    pub lessons: Vec<Lesson>,
    /// Diplomas: (person id, study index).
    pub diplomas: Vec<(u32, u8)>,
    pub works: Vec<Work>,
    /// Renown of the crew's artists, 0..
    pub fame: f32,
    pub day: i64,
    pub next_work: u64,
    /// Last day each person composed a work.
    pub composed: Vec<(u32, i64)>,
    pub rounds: u64,
}

/// The result of a bet.
#[derive(Clone, Debug, PartialEq)]
pub struct Bet {
    pub winner: usize,
    pub stake: f64,
    pub payout: f64,
    pub odds: f64,
}

fn name_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A word of a language at a given era: the base word drifts one letter per era.
pub fn drift_word(base: &str, lang_hash: u64, era: u32) -> String {
    let mut c: Vec<char> = base.chars().collect();
    if c.is_empty() {
        return String::new();
    }
    const V: [char; 5] = ['a', 'e', 'i', 'o', 'u'];
    const K: [char; 10] = ['b', 'd', 'k', 'l', 'm', 'n', 'r', 's', 't', 'v'];
    for e in 1..=era {
        let h = hash_coord(lang_hash, i64::from(e), 3, 9);
        let i = (h % c.len() as u64) as usize;
        let pool: &[char] = if V.contains(&c[i]) {
            &V
        } else if c[i].is_alphabetic() {
            &K
        } else {
            continue;
        };
        c[i] = pool[((h >> 20) % pool.len() as u64) as usize];
    }
    c.into_iter().collect()
}

impl Sim {
    /// The language a culture of `cultures_names.ron` speaks.
    pub fn language_of_culture(&self, culture: usize) -> usize {
        let id = self
            .data
            .cultures
            .get(culture)
            .map_or("", |c| c.id.as_str());
        self.data
            .society
            .languages
            .iter()
            .position(|l| l.cultures.iter().any(|c| c == id))
            .unwrap_or(0)
    }

    pub fn language_of_faction(&self, faction: usize) -> usize {
        let cid = &self.data.factions[faction].culture;
        let ci = self
            .data
            .cultures
            .iter()
            .position(|c| c.id == *cid)
            .unwrap_or(0);
        self.language_of_culture(ci)
    }

    /// The language of a native people (kind index of `civilisations.ron`).
    pub fn language_of_civ(&self, kind: usize) -> usize {
        let id = self
            .data
            .civilisations
            .get(kind)
            .map_or("", |c| c.id.as_str());
        self.data
            .society
            .languages
            .iter()
            .position(|l| l.civs.iter().any(|c| c == id))
            .unwrap_or(0)
    }

    /// The language spoken where the ship is, if it lies in somebody's space.
    pub fn local_language(&self) -> Option<usize> {
        self.faction.map(|f| self.language_of_faction(f))
    }

    pub fn years_elapsed(&self) -> f64 {
        self.time.max(0.0) / EARTH_YEAR
    }

    /// How many drifts the language has gone through.
    pub fn language_era(&self, lang: usize) -> u32 {
        let l = &self.data.society.languages[lang];
        (self.years_elapsed() / l.era_years) as u32
    }

    /// A word of a language as it is said now (0 greeting, 1 thanks, 2 deal).
    pub fn language_word(&self, lang: usize, k: usize) -> String {
        let l = &self.data.society.languages[lang];
        drift_word(&l.words[k % 3], name_hash(&l.id), self.language_era(lang))
    }

    fn tongue_raw(&self, person: u32, lang: usize) -> Option<&Tongue> {
        self.society
            .tongues
            .iter()
            .find(|t| t.person == person && usize::from(t.lang) == lang)
    }

    /// 0..1 fluency of a crew member in a language: the mother tongue is perfect, a learned one
    /// fades with the eras it drifted, a language of the same family is half understood.
    pub fn fluency(&self, person: u32, lang: usize) -> f32 {
        let Some(p) = self.crew.members.iter().find(|p| p.id == person) else {
            return 0.0;
        };
        let mine = self.language_of_culture(p.culture);
        let direct = |l: usize| -> f32 {
            if l == mine {
                return 1.0;
            }
            self.tongue_raw(person, l).map_or(0.0, |t| {
                let gap = self.language_era(l).saturating_sub(t.era) as f32;
                (t.skill - ERA_FADE * gap).max(0.0)
            })
        };
        let family = &self.data.society.languages[lang].family;
        let kin = self
            .data
            .society
            .languages
            .iter()
            .enumerate()
            .filter(|(i, l)| *i != lang && l.family == *family)
            .map(|(i, _)| direct(i))
            .fold(0.0, f32::max);
        direct(lang).max(KINSHIP * kin)
    }

    /// Best fluency of the living crew.
    pub fn crew_fluency(&self, lang: usize) -> f32 {
        self.crew
            .alive()
            .map(|p| self.fluency(p.id, lang))
            .fold(0.0, f32::max)
    }

    /// What the ship's AI understands with the translation budget of the mind.
    pub fn ai_fluency(&self) -> f32 {
        (0.2 + 0.4 * self.mind_rate(T_TRADUCTION) as f32).clamp(0.0, AI_CAP)
    }

    /// What the ship as a whole understands of a language.
    pub fn understanding(&self, lang: usize) -> f32 {
        self.crew_fluency(lang).max(self.ai_fluency())
    }

    /// Price multiplier of the docks from the language barrier (0.97 .. 1).
    pub fn language_edge(&self) -> f64 {
        self.local_language()
            .map_or(1.0, |l| self.language_edge_for(l))
    }

    /// The same for a given language.
    pub fn language_edge_for(&self, lang: usize) -> f64 {
        0.97 + 0.03 * f64::from(self.understanding(lang))
    }

    // ---- calendar and culture of a place ----

    /// The body whose year is the calendar of `body`: the planet a station or moon orbits.
    fn calendar_body(&self, body: usize) -> usize {
        let mut i = body;
        for _ in 0..4 {
            match self.system.bodies.get(i).and_then(|b| b.orbit.as_ref()) {
                Some(o) if !matches!(self.system.bodies[o.parent].kind, BodyKind::Star(_)) => {
                    i = o.parent
                }
                _ => break,
            }
        }
        i
    }

    /// Length of the local year in seconds (the period around the star).
    pub fn local_year(&self, body: usize) -> f64 {
        let i = self.calendar_body(body);
        self.system
            .bodies
            .get(i)
            .and_then(|b| b.orbit.as_ref())
            .filter(|o| o.mu > 0.0 && o.a > 0.0)
            .map_or(EARTH_YEAR, |o| {
                std::f64::consts::TAU * (o.a.powi(3) / o.mu).sqrt()
            })
    }

    /// (year number, fraction of the year) of a place now.
    pub fn local_date(&self, body: usize) -> (u64, f64) {
        let y = self.local_year(body);
        let t = self.time.max(0.0) / y;
        (t.floor() as u64, t.fract())
    }

    pub fn season_name(frac: f64) -> &'static str {
        ["PRINTEMPS", "ETE", "AUTOMNE", "HIVER"][((frac.fract() * 4.0) as usize).min(3)]
    }

    /// The festival under way in the space of `faction` at `body`, if any.
    pub fn festival_now(&self, faction: usize, body: usize) -> Option<usize> {
        let d = &self.data;
        let values = d.ethos.profile(Some(d.factions[faction].id.as_str()));
        let (_, frac) = self.local_date(body);
        let year = self.local_year(body);
        d.society.festivals.iter().position(|f| {
            let held = f.when.is_empty()
                || f.when.iter().any(|(axis, op, limit)| {
                    let v = d
                        .ethos
                        .axis(axis)
                        .and_then(|a| values.get(a))
                        .copied()
                        .unwrap_or(0.0);
                    if op == "<" {
                        v < *limit
                    } else {
                        v > *limit
                    }
                });
            let len = f.days * DAY / year;
            held && (frac - f.at).rem_euclid(1.0) < len
        })
    }

    /// The dish a people is known for.
    pub fn local_dish(&self, faction: usize) -> usize {
        let f = &self.data.factions[faction];
        (name_hash(&f.id) % self.data.society.dishes.len() as u64) as usize
    }

    /// Once a day at the docks: festival and cuisine lift the crew's mood.
    fn culture_day(&mut self) {
        let (Some(f), Some(b)) = (self.faction, self.docked) else {
            return;
        };
        let dish = self.data.society.dishes[self.local_dish(f)].morale;
        let fest = self.festival_now(f, b);
        let mood = dish + fest.map_or(0.0, |i| self.data.society.festivals[i].morale);
        for p in self.crew.members.iter_mut().filter(|p| p.alive) {
            p.morale = (p.morale + mood).min(1.0);
        }
        if let Some(i) = fest {
            let msg = format!(
                "{} chez {} : l'equipage est de fete.",
                self.data.society.festivals[i].name, self.data.factions[f].name
            );
            self.events.push(Event::MarketNews(msg));
        }
    }

    // ---- the languages in practice ----

    fn learn_tongue(&mut self, person: u32, lang: usize, gain: f32, cap: f32) {
        let era = self.language_era(lang);
        let now = self.fluency(person, lang);
        let base = self.tongue_raw(person, lang).map_or(0.0, |t| {
            (t.skill - ERA_FADE * era.saturating_sub(t.era) as f32).max(0.0)
        });
        let _ = now;
        let skill = (base + gain).min(cap.max(base)).min(1.0);
        match self
            .society
            .tongues
            .iter_mut()
            .find(|t| t.person == person && usize::from(t.lang) == lang)
        {
            Some(t) => {
                t.skill = skill;
                t.era = era;
            }
            None => self.society.tongues.push(Tongue {
                person,
                lang: lang as u8,
                skill,
                era,
            }),
        }
    }

    /// A quarantine law and a sick crew arriving at the docks.
    pub(crate) fn quarantine_check(&mut self) -> Option<crate::law::Offence> {
        let sick = self.crew.alive().any(|p| {
            p.disease
                .is_some_and(|(d, _)| self.data.diseases[d].contagion > 0.0)
        });
        if sick {
            self.offend(self.faction, "quarantaine")
        } else {
            None
        }
    }

    // ---- schools ----

    /// Fee of a study here (the open-minded charge less).
    pub fn study_fee(&self, study: usize) -> f64 {
        let s = &self.data.society.studies[study];
        let open = self.faction.map_or(0.0, |f| {
            let v = self
                .data
                .ethos
                .profile(Some(self.data.factions[f].id.as_str()));
            self.data
                .ethos
                .axis("ouverture")
                .and_then(|a| v.get(a))
                .copied()
                .unwrap_or(0.0)
        });
        s.fee * (1.0 - 0.3 * open)
    }

    /// Whether somebody else of the crew can teach what `target` is about.
    fn has_mentor(&self, person: u32, study: usize, target: usize) -> bool {
        let s = &self.data.society.studies[study];
        self.crew
            .alive()
            .filter(|p| p.id != person)
            .any(|p| match s.kind.as_str() {
                "skill" => p.skills.get(target).is_some_and(|v| *v >= 6.0),
                "langue" => self.fluency(p.id, target) >= 0.8,
                _ => self.craft_skill(p.id, target) >= 0.6,
            })
    }

    /// Enrol a crew member at the school of the docks.
    pub fn enroll(&mut self, person: u32, study: usize) -> Result<String, &'static str> {
        self.docked.ok_or("les ecoles sont a la station")?;
        let s = self
            .data
            .society
            .studies
            .get(study)
            .ok_or("etude inconnue")?
            .clone();
        let p = self
            .crew
            .members
            .iter()
            .find(|p| p.id == person && p.alive)
            .ok_or("personne inconnue")?;
        let name = p.name.clone();
        let mother = self.language_of_culture(p.culture);
        if self.society.lessons.iter().any(|l| l.person == person) {
            return Err("deja en cours d'etudes");
        }
        let target = match s.kind.as_str() {
            "skill" => self
                .data
                .skill_index(&s.target)
                .ok_or("competence inconnue")?,
            "art" => self.data.art_index(&s.target).ok_or("art inconnu")?,
            _ => {
                let l = self.local_language().ok_or("pas de langue locale ici")?;
                if l == mother {
                    return Err("c'est deja sa langue");
                }
                l
            }
        };
        let fee = self.study_fee(study);
        if fee > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= fee;
        self.society.lessons.push(Lesson {
            person,
            study: study as u8,
            target: target as u8,
            left: s.days as f32,
        });
        Ok(format!("{name} s'inscrit : {}.", s.name.to_lowercase()))
    }

    /// Lessons, practice, festivals: once a day.
    pub(crate) fn society_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.society.day;
        self.society.day = day;
        if last == 0 || day <= last {
            return;
        }
        self.culture_day();
        // Living among a people teaches its language.
        if let (Some(l), Some(_)) = (self.local_language(), self.docked) {
            let ids: Vec<u32> = self.crew.alive().map(|p| p.id).collect();
            for id in ids {
                self.learn_tongue(id, l, PRACTICE, PRACTICE_CAP);
            }
        }
        // Lessons.
        let lessons = std::mem::take(&mut self.society.lessons);
        let mut keep = Vec::new();
        for mut les in lessons {
            if !self
                .crew
                .members
                .iter()
                .any(|p| p.id == les.person && p.alive)
            {
                continue;
            }
            let speed =
                if self.has_mentor(les.person, usize::from(les.study), usize::from(les.target)) {
                    MENTOR
                } else {
                    1.0
                };
            les.left -= speed;
            if les.left > 0.0 {
                keep.push(les);
                continue;
            }
            let s = self.data.society.studies[usize::from(les.study)].clone();
            let t = usize::from(les.target);
            match s.kind.as_str() {
                "skill" => {
                    if let Some(p) = self.crew.members.iter_mut().find(|p| p.id == les.person) {
                        p.skills[t] = (p.skills[t] + s.gain as f32).clamp(0.0, 10.0);
                    }
                }
                "art" => self.learn_craft(les.person, t, s.gain as f32),
                _ => self.learn_tongue(les.person, t, s.gain as f32, 1.0),
            }
            self.society.diplomas.push((les.person, les.study));
            if let Some(p) = self.crew.members.iter().find(|p| p.id == les.person) {
                let msg = format!("{} obtient le brevet : {}.", p.name, s.name.to_lowercase());
                self.events.push(Event::MarketNews(msg));
            }
        }
        keep.append(&mut self.society.lessons);
        self.society.lessons = keep;
    }

    // ---- arts ----

    /// 0..1 craft of a person in an art (a little innate talent to start with).
    pub fn craft_skill(&self, person: u32, art: usize) -> f32 {
        self.society
            .crafts
            .iter()
            .find(|c| c.person == person && usize::from(c.art) == art)
            .map_or_else(
                || {
                    0.1 + 0.3
                        * unit(hash_coord(
                            self.galaxy.seed ^ 0xA27,
                            i64::from(person),
                            art as i64,
                            1,
                        )) as f32
                },
                |c| c.skill,
            )
    }

    fn learn_craft(&mut self, person: u32, art: usize, gain: f32) {
        let v = (self.craft_skill(person, art) + gain).min(1.0);
        match self
            .society
            .crafts
            .iter_mut()
            .find(|c| c.person == person && usize::from(c.art) == art)
        {
            Some(c) => c.skill = v,
            None => self.society.crafts.push(Craft {
                person,
                art: art as u8,
                skill: v,
            }),
        }
    }

    /// A crew member makes a work of art (once a day each); returns its index.
    pub fn compose(&mut self, person: u32, art: usize) -> Result<usize, &'static str> {
        let form = self
            .data
            .society
            .arts
            .get(art)
            .ok_or("art inconnu")?
            .clone();
        let p = self
            .crew
            .members
            .iter()
            .find(|p| p.id == person && p.alive)
            .ok_or("personne inconnue")?;
        let author_name = p.name.clone();
        let day = (self.time / DAY).floor() as i64;
        if self
            .society
            .composed
            .iter()
            .any(|(id, d)| *id == person && *d == day)
        {
            return Err("deja au travail aujourd'hui");
        }
        self.society.composed.retain(|(id, _)| *id != person);
        self.society.composed.push((person, day));
        let id = self.society.next_work;
        self.society.next_work += 1;
        let h = hash_coord(
            self.galaxy.seed ^ 0xA27,
            i64::from(person),
            art as i64,
            id as i64,
        );
        let skill = self.craft_skill(person, art);
        let quality = (0.8 * skill + 0.2 * unit(h) as f32).clamp(0.0, 1.0);
        let title = format!(
            "{} {}",
            form.stems[(h % form.stems.len() as u64) as usize],
            form.epithets[((h >> 24) % form.epithets.len() as u64) as usize]
        );
        self.learn_craft(person, art, 0.02);
        self.society.works.push(Work {
            id,
            art: art as u8,
            author: person,
            author_name,
            title,
            quality,
            made: self.time,
        });
        Ok(self.society.works.len() - 1)
    }

    /// Price of a work on the market of the docked place: quality squared, the artist's fame,
    /// and what the local people like (their mores on the taste axis of the art).
    pub fn work_value(&self, i: usize) -> f64 {
        let w = &self.society.works[i];
        let form = &self.data.society.arts[usize::from(w.art)];
        let taste = self.faction.map_or(0.0, |f| {
            let v = self
                .data
                .ethos
                .profile(Some(self.data.factions[f].id.as_str()));
            self.data
                .ethos
                .axis(&form.taste.0)
                .and_then(|a| v.get(a))
                .copied()
                .unwrap_or(0.0)
                * form.taste.1
        });
        let week = (self.time / (7.0 * DAY)).floor() as i64;
        let swing = 0.9
            + 0.2
                * unit(hash_coord(
                    self.galaxy.seed ^ 0xA27,
                    i64::from(w.art),
                    week,
                    5,
                ));
        form.base
            * (0.2 + 3.0 * f64::from(w.quality).powi(2))
            * (1.0 + 0.1 * f64::from(self.society.fame.min(10.0)))
            * (1.0 + 0.4 * taste)
            * swing
    }

    /// Sell a work to the gallery of the docks: credits, and fame for the artist.
    pub fn sell_work(&mut self, i: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("la galerie est a la station")?;
        if i >= self.society.works.len() {
            return Err("oeuvre inconnue");
        }
        let v = self.work_value(i);
        let w = self.society.works.remove(i);
        self.credits += v;
        self.society.fame += w.quality * 0.2;
        Ok(v)
    }

    /// Give a work to the museum of the docks: goodwill with the people and fame, no money.
    pub fn donate_work(&mut self, i: usize) -> Result<String, &'static str> {
        self.docked.ok_or("le musee est a la station")?;
        if i >= self.society.works.len() {
            return Err("oeuvre inconnue");
        }
        let w = self.society.works.remove(i);
        self.society.fame += w.quality * 0.5;
        self.change_reputation(self.faction, w.quality * 1.5);
        Ok(format!("{} entre au musee.", w.title))
    }

    // ---- sports ----

    /// Strengths of the field of a sport today (the favourite changes every day).
    pub fn field_strengths(&self, sport: usize) -> Vec<f64> {
        let n = self.data.society.sports[sport].runners;
        let day = (self.time / DAY).floor() as i64;
        let place = self.docked.unwrap_or(0) as i64;
        (0..n)
            .map(|i| {
                let h = hash_coord(
                    self.galaxy.seed ^ 0x5907,
                    day * 16 + sport as i64,
                    place,
                    i as i64,
                );
                (0.5 + unit(h)).powi(3)
            })
            .collect()
    }

    /// Fair chance of each runner winning and what a stake pays (the house keeps its margin).
    pub fn field_odds(&self, sport: usize) -> Vec<(f64, f64)> {
        let w = self.field_strengths(sport);
        let total: f64 = w.iter().sum();
        let margin = self.data.society.sports[sport].margin;
        w.iter()
            .map(|x| (x / total, (1.0 - margin) * total / x))
            .collect()
    }

    pub fn runner_name(&self, sport: usize, i: usize) -> String {
        const S: [&str; 10] = [
            "ka", "ro", "vel", "ixa", "mo", "dun", "sel", "tor", "nia", "bex",
        ];
        let h = hash_coord(
            self.galaxy.seed ^ 0x5907,
            sport as i64,
            self.docked.unwrap_or(0) as i64,
            i as i64 + 100,
        );
        let (a, b) = (S[(h % 10) as usize], S[((h >> 8) % 10) as usize]);
        let mut n = format!("{a}{b}");
        n[..1].make_ascii_uppercase();
        n
    }

    /// Bet on a runner at the docks (with the same stakes as the casino).
    pub fn bet(&mut self, sport: usize, runner: usize, stake: f64) -> Result<Bet, &'static str> {
        self.docked.ok_or("les paris sont a la station")?;
        let odds = self.field_odds(sport);
        if runner >= odds.len() {
            return Err("coureur inconnu");
        }
        if !(crate::casino::MIN_STAKE..=crate::casino::MAX_STAKE).contains(&stake) {
            return Err("mise de 10 a 5000 CR");
        }
        if stake > self.credits {
            return Err("fonds insuffisants");
        }
        let mut rng = Rng::new(hash_coord(
            self.galaxy.seed ^ 0x5908,
            self.society.rounds as i64,
            self.time as i64,
            sport as i64,
        ));
        self.society.rounds += 1;
        let mut x = rng.f64();
        let mut winner = odds.len() - 1;
        for (i, (p, _)) in odds.iter().enumerate() {
            if x < *p {
                winner = i;
                break;
            }
            x -= p;
        }
        let payout = if winner == runner {
            stake * odds[runner].1
        } else {
            0.0
        };
        self.credits += payout - stake;
        Ok(Bet {
            winner,
            stake,
            payout,
            odds: odds[runner].1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn docked() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s
    }

    /// A language that nobody aboard has as a mother tongue and that is not of their families.
    fn foreign(s: &Sim) -> usize {
        let langs = &s.data.society.languages;
        (0..langs.len())
            .find(|&l| {
                s.crew
                    .alive()
                    .all(|p| langs[s.language_of_culture(p.culture)].family != langs[l].family)
            })
            .expect("a foreign family")
    }

    #[test]
    fn every_culture_and_people_speaks_one_language() {
        let s = docked();
        for c in 0..s.data.cultures.len() {
            let l = s.language_of_culture(c);
            assert!(s.data.society.languages[l]
                .cultures
                .contains(&s.data.cultures[c].id));
        }
        for k in 0..s.data.civilisations.len() {
            let l = s.language_of_civ(k);
            assert!(s.data.society.languages[l]
                .civs
                .contains(&s.data.civilisations[k].id));
        }
    }

    #[test]
    fn fluency_mother_tongue_kin_and_strangers() {
        let s = docked();
        let p = s.crew.members[0].clone();
        let mine = s.language_of_culture(p.culture);
        assert_eq!(s.fluency(p.id, mine), 1.0);
        let fam = &s.data.society.languages[mine].family;
        let cousin = (0..s.data.society.languages.len())
            .find(|&l| l != mine && s.data.society.languages[l].family == *fam)
            .unwrap();
        assert_eq!(s.fluency(p.id, cousin), KINSHIP);
        let far = foreign(&s);
        assert_eq!(s.fluency(p.id, far), 0.0);
    }

    #[test]
    fn the_ai_translates_with_the_translation_budget_but_never_like_a_person() {
        let mut s = docked();
        let far = foreign(&s);
        let before = s.understanding(far);
        assert!((0.2..=AI_CAP).contains(&before));
        s.mind.alloc = [4; crate::mind::N_TASKS];
        s.mind.alloc[T_TRADUCTION] = 0;
        let starved = s.ai_fluency();
        s.mind.alloc[T_TRADUCTION] = crate::mind::MAX_WEIGHT;
        let fed = s.ai_fluency();
        assert!(fed > starved, "{fed} vs {starved}");
        assert!(fed <= AI_CAP);
    }

    #[test]
    fn practice_teaches_the_local_language_and_it_drifts_away_when_unpractised() {
        let mut s = docked();
        let l = s.local_language().expect("station in somebody's space");
        let id = s
            .crew
            .alive()
            .find(|p| s.language_of_culture(p.culture) != l)
            .map(|p| p.id);
        let Some(id) = id else { return };
        let fam = s.data.society.languages[l].family.clone();
        let mine =
            s.language_of_culture(s.crew.members.iter().find(|p| p.id == id).unwrap().culture);
        let kin = s.data.society.languages[mine].family == fam;
        let start = s.fluency(id, l);
        s.society.day = (s.time / DAY) as i64 - 1;
        s.society_tick();
        for _ in 0..30 {
            s.time += DAY;
            s.society_tick();
        }
        let learned = s.fluency(id, l);
        assert!(
            learned > start || kin && learned >= start,
            "{start} -> {learned}"
        );
        // Leave and let eras pass: the language moved on.
        s.docked = None;
        s.faction = None;
        let era_len = s.data.society.languages[l].era_years * 365.25 * DAY;
        s.time += 3.0 * era_len;
        if !kin {
            assert!(s.fluency(id, l) < learned, "unpractised, it fades");
        } else {
            assert!(s.fluency(id, l) <= learned);
        }
    }

    #[test]
    fn words_drift_one_letter_a_era_and_deterministically() {
        assert_eq!(drift_word("marche", 9, 0), "marche");
        let a = drift_word("marche", 9, 6);
        assert_eq!(a, drift_word("marche", 9, 6));
        assert_ne!(a, "marche");
        assert_eq!(a.chars().count(), 6);
    }

    #[test]
    fn language_barrier_costs_at_most_three_percent() {
        let mut s = docked();
        let e = s.language_edge();
        assert!((0.97..=1.0).contains(&e));
        s.faction = None;
        assert_eq!(s.language_edge(), 1.0);
    }

    #[test]
    fn calendar_has_seasons_and_festivals_held_by_mores() {
        let s = docked();
        let b = s.docked.unwrap();
        assert!(s.local_year(b) > 0.0);
        let (_, frac) = s.local_date(b);
        assert!((0.0..1.0).contains(&frac));
        assert_eq!(Sim::season_name(0.1), "PRINTEMPS");
        assert_eq!(Sim::season_name(0.9), "HIVER");
        // Over a whole year every faction holds some festival at some point (solstice: everyone).
        let f = s.faction.expect("station faction");
        let mut t = s;
        let year = t.local_year(b);
        let mut seen = false;
        for k in 0..400 {
            t.time = year * f64::from(k) / 400.0;
            seen |= t.festival_now(f, b).is_some();
        }
        assert!(seen);
        // A festival only runs where its mores hold.
        let veillee = t
            .data
            .society
            .festivals
            .iter()
            .position(|x| x.id == "veillee")
            .unwrap();
        let values = t.data.ethos.profile(Some(&t.data.factions[f].id));
        let faith = values[t.data.ethos.axis("foi").unwrap()];
        t.time = year * (t.data.society.festivals[veillee].at + 0.001);
        let held = t.festival_now(f, b) == Some(veillee);
        assert_eq!(held, faith > 0.3);
    }

    #[test]
    fn schools_teach_with_a_fee_and_a_mentor_speeds_lessons() {
        let mut s = docked();
        let st = s
            .data
            .society
            .studies
            .iter()
            .position(|x| x.id == "science")
            .unwrap();
        let sk = s.data.skill_index("science").unwrap();
        let id = s.crew.members[0].id;
        s.crew.members[0].skills[sk] = 2.0;
        // Nobody else knows science: no mentor.
        for p in s.crew.members.iter_mut().skip(1) {
            p.skills[sk] = 0.0;
        }
        let c0 = s.credits;
        s.enroll(id, st).unwrap();
        assert!(s.credits < c0, "the fee is paid");
        assert!(s.enroll(id, st).is_err(), "one course at a time");
        s.society.day = (s.time / DAY) as i64 - 1;
        let days = s.data.society.studies[st].days as usize;
        for k in 0..days + 1 {
            s.time += DAY;
            s.society_tick();
            if k == 0 {
                assert!(s.society.diplomas.is_empty());
            }
        }
        assert_eq!(s.society.diplomas, vec![(id, st as u8)]);
        assert!(s.crew.members[0].skills[sk] > 2.0);
        // With a mentor it takes fewer days.
        if s.crew.members.len() > 1 {
            s.crew.members[1].skills[sk] = 9.0;
            let other = s.crew.members[0].id;
            s.enroll(other, st).unwrap();
            let before = s.society.lessons[0].left;
            s.time += DAY;
            s.society_tick();
            assert!(
                before - s.society.lessons[0].left > 1.0,
                "mentored lesson goes faster"
            );
        }
        // Not enough money.
        s.credits = 0.0;
        let other = s.crew.members.last().unwrap().id;
        s.society.lessons.clear();
        assert_eq!(s.enroll(other, st), Err("fonds insuffisants"));
    }

    #[test]
    fn works_are_valued_by_quality_fame_and_taste_and_can_be_sold_or_given() {
        let mut s = docked();
        let id = s.crew.members[0].id;
        let i = s.compose(id, 1).unwrap();
        assert!(s.compose(id, 1).is_err(), "once a day");
        let v = s.work_value(i);
        assert!(v > 0.0);
        s.society.works[i].quality = 1.0;
        let great = s.work_value(i);
        s.society.works[i].quality = 0.1;
        let poor = s.work_value(i);
        assert!(great > 5.0 * poor);
        s.society.fame = 10.0;
        assert!(s.work_value(i) > poor);
        let c = s.credits;
        let sold = s.sell_work(i).unwrap();
        assert!((s.credits - c - sold).abs() < 1e-9 && s.society.works.is_empty());
        s.time += DAY;
        let j = s.compose(id, 0).unwrap();
        let rep = s.reputation_with(s.faction);
        s.society.works[j].quality = 1.0;
        s.donate_work(j).unwrap();
        assert!(s.reputation_with(s.faction) > rep);
        assert!(s.society.works.is_empty());
    }

    #[test]
    fn the_house_keeps_its_margin_on_every_sport() {
        let s = docked();
        for (k, sport) in s.data.society.sports.iter().enumerate() {
            let odds = s.field_odds(k);
            assert_eq!(odds.len(), sport.runners);
            let p: f64 = odds.iter().map(|o| o.0).sum();
            assert!((p - 1.0).abs() < 1e-9);
            // Exact return of any bet = p * payout = 1 - margin.
            for (p, pay) in odds {
                assert!((p * pay - (1.0 - sport.margin)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn bets_pay_the_odds_and_are_checked() {
        let mut s = docked();
        assert!(s.bet(0, 99, 100.0).is_err());
        assert!(s.bet(0, 0, 1.0).is_err());
        assert!(s.bet(0, 0, 1.0e9).is_err());
        let (mut wins, mut n) = (0, 0);
        for k in 0..300 {
            s.time = f64::from(k) * 3.0;
            let before = s.credits;
            let r = s.bet(0, 0, 100.0).unwrap();
            n += 1;
            if r.winner == 0 {
                wins += 1;
                assert!((s.credits - before - (r.payout - 100.0)).abs() < 1e-9);
                assert!(r.payout > 100.0);
            } else {
                assert_eq!(s.credits, before - 100.0);
            }
        }
        assert!(wins > 0 && wins < n, "{wins}/{n}");
        s.docked = None;
        assert!(s.bet(0, 0, 100.0).is_err());
    }

    #[test]
    fn sick_arrivals_break_quarantine_laws_where_they_exist() {
        let mut s = docked();
        let d = s
            .data
            .diseases
            .iter()
            .position(|d| d.contagion > 0.0)
            .unwrap();
        let q = s.data.law_index("quarantaine").unwrap();
        let f = (0..s.data.factions.len())
            .find(|&f| s.laws_in_force(f).contains(&q))
            .expect("some people hold it");
        let g = (0..s.data.factions.len())
            .find(|&f| !s.laws_in_force(f).contains(&q))
            .unwrap();
        s.crew.members[0].disease = Some((d, 5.0));
        s.faction = Some(g);
        assert!(s.quarantine_check().is_none());
        s.faction = Some(f);
        let mut caught = false;
        for k in 0..200 {
            s.time = f64::from(k) * 7.0;
            caught |= s.quarantine_check().is_some();
        }
        assert!(caught);
        // Healthy crew: nothing.
        s.crew.members[0].disease = None;
        s.time += 1.0e5;
        assert!(s.quarantine_check().is_none());
    }
}
