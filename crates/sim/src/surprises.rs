//! Surprises: rare events that come out of the crossing of systems, declared in
//! `data/surprises.ron`. Once a day the world around the ship is read (wars, world events,
//! cartels, incidents, your colonies, your relations, anomalies, contracts, the crew...) and every
//! surprise whose conditions all hold rolls a hash of (seed, sector, day, surprise): nothing is
//! scripted and nothing is drawn from a real clock. A surprise that fires is told in the news,
//! applies its effects (relations, credits, price shocks, contracts offered, cargo...) and may
//! offer choices, answered from the journal. It only happens where the ship is: the world is
//! infinite and surprises are a matter of proximity.
use crate::bourse::{events_between, ActiveEvent, DAY};
use crate::economy::Contract;
use crate::news::TAGS;
use crate::planet::unit;
use crate::politics::{cell_of, turmoil, Turmoil};
use crate::relations::{Entity, Kind};
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_data::surprise::{Cond, Effect, Target, DAILY_CHANCE};

/// Days that are replayed at most when time was skipped.
pub const CATCH_UP_DAYS: i64 = 10;
/// Pending choices kept at once.
pub const PENDING_MAX: usize = 3;
/// How far a price shock reaches, ly.
pub const SHOCK_RADIUS_LY: f64 = 60.0;
/// Reach of "your colonies" in conditions that do not say, ly.
pub const COLONY_REACH_LY: f64 = 30.0;
/// Contracts offered by surprises last this many days.
pub const OFFER_DAYS: f64 = 20.0;

/// A choice waiting for an answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub def: usize,
    pub time: f64,
    pub expires: f64,
    /// The text as it was told (placeholders resolved).
    pub text: String,
}

/// A price shock in a region: (resource id or "cat:<category>") times `mult` around `pos`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shock {
    pub pos: [f64; 3],
    pub key: String,
    pub mult: f64,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SurpriseState {
    /// Last day the world was read (0 = never).
    pub last_day: i64,
    /// (surprise index, day) of the last time each one fired.
    pub fired: Vec<(usize, i64)>,
    pub pending: Vec<Pending>,
    pub shocks: Vec<Shock>,
    /// Contracts offered at the station of the system where they were born.
    pub offers: Vec<Contract>,
    /// How many surprises happened in all.
    pub count: u32,
}

/// What the world around the ship is doing, read once per day.
pub(crate) struct Ctx {
    events: Vec<usize>,
    turmoil: Turmoil,
    incident: bool,
    /// Cartels (by index) that hold prices here this month.
    cartels: Vec<bool>,
    colony_ly: f64,
    epidemic_ly: f64,
    colony_unrest: f64,
    anomalies: Vec<&'static str>,
    civ: Option<Option<u64>>,
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Sim {
    /// Price multiplier that surprises put on resource `res` at `pos`, time `t`.
    pub fn shock_factor(&self, pos: [f64; 3], t: f64, res: usize) -> f64 {
        let r = &self.data.resources[res];
        self.surprises
            .shocks
            .iter()
            .filter(|s| t >= s.start && t <= s.end && dist(s.pos, pos) < SHOCK_RADIUS_LY)
            .filter(|s| {
                s.key
                    .strip_prefix("cat:")
                    .map_or(s.key == r.id, |c| c == r.category)
            })
            .map(|s| {
                let p = ActiveEvent {
                    def: 0,
                    start: s.start,
                    end: s.end,
                    global: false,
                    strength: 1.0,
                }
                .profile(t);
                1.0 + (s.mult - 1.0) * p
            })
            .product()
    }

    fn surprise_ctx(&self) -> Ctx {
        let data = &self.data;
        let (seed, pos, t) = (self.galaxy.seed, self.here_pos, self.time);
        let events = events_between(data, seed, pos, t, t)
            .into_iter()
            .filter(|e| e.profile(t) * e.strength > 0.3)
            .map(|e| e.def)
            .collect();
        let galaxy = vector_world::galaxy::Galaxy::new(seed);
        let incident =
            !crate::incidents::incidents_in_cell(data, seed, cell_of(pos), t, t).is_empty();
        let (mut colony_ly, mut epidemic_ly, mut colony_unrest) = (f64::MAX, f64::MAX, 0.0f64);
        for col in &self.colonies {
            let d = self
                .galaxy
                .star(col.star)
                .map_or(f64::MAX, |e| e.distance_to(pos));
            colony_ly = colony_ly.min(d);
            if !col.outbreaks.is_empty() {
                epidemic_ly = epidemic_ly.min(d);
            }
            if d <= COLONY_REACH_LY {
                colony_unrest = colony_unrest.max(col.unrest);
            }
        }
        let anomalies = (0..self.bodies.len())
            .filter_map(|i| self.anomaly_kind(i).map(|k| k.id()))
            .collect();
        Ctx {
            events,
            turmoil: turmoil(data, &galaxy, seed, pos, t),
            incident,
            cartels: (0..data.cartels.len())
                .map(|c| self.cartel_here(c))
                .collect(),
            colony_ly,
            epidemic_ly,
            colony_unrest,
            anomalies,
            civ: None,
        }
    }

    /// Key of the civilisation of the system (worked out only when a surprise asks).
    fn ctx_civ(&self, ctx: &mut Ctx) -> Option<u64> {
        *ctx.civ
            .get_or_insert_with(|| self.first_civilised_world().map(|b| self.civ_key(b)))
    }

    fn faction_named(&self, id: &str) -> Option<usize> {
        self.data.factions.iter().position(|f| f.id == id)
    }

    /// The first ally (or enemy) of the territory the ship is in.
    fn neighbour_faction(&self, ally: bool) -> Option<usize> {
        let f = &self.data.factions[self.faction?];
        let list = if ally { &f.allies } else { &f.enemies };
        list.iter().find_map(|id| self.faction_named(id))
    }

    /// Who a target of a surprise is, if it can be told.
    fn resolve_target(&self, t: &Target, ctx: &mut Ctx) -> Option<Entity> {
        let d = &self.data;
        match t {
            Target::Faction(id) => self.faction_named(id).map(Entity::faction),
            Target::Here => self.faction.map(Entity::faction),
            Target::Ally => self.neighbour_faction(true).map(Entity::faction),
            Target::Rival => self.neighbour_faction(false).map(Entity::faction),
            Target::Cartel(id) => d
                .cartels
                .iter()
                .position(|c| c.id == *id)
                .map(Entity::cartel),
            Target::Company(id) => d
                .companies
                .iter()
                .position(|c| c.id == *id)
                .map(Entity::company),
            Target::Contact => self.faction.map(Entity::contact),
            Target::Civ => self.ctx_civ(ctx).map(Entity::civ),
        }
    }

    fn surprise_cond(&self, c: &Cond, ctx: &mut Ctx) -> bool {
        let d = &self.data;
        let alive = || self.crew.members.iter().filter(|p| p.health > 0.0);
        match c {
            Cond::WorldEvent(id) => d
                .world_events
                .iter()
                .position(|w| w.id == *id)
                .is_some_and(|i| ctx.events.contains(&i)),
            Cond::AtWar => ctx.turmoil.at_war,
            Cond::Unrest(v) => ctx.turmoil.unrest >= *v,
            Cond::CartelHere(id) => d
                .cartels
                .iter()
                .position(|k| k.id == *id)
                .is_some_and(|i| ctx.cartels[i]),
            Cond::Incident => ctx.incident,
            Cond::Epidemic(ly) => ctx.epidemic_ly <= *ly,
            Cond::ColonyNear(ly) => ctx.colony_ly <= *ly,
            Cond::ColonyUnrest(v) => ctx.colony_unrest >= *v,
            Cond::Territory(id) => self.faction.is_some() && self.faction == self.faction_named(id),
            Cond::Claimed => self.faction.is_some(),
            Cond::Civ => self.ctx_civ(ctx).is_some(),
            Cond::Anomaly => !ctx.anomalies.is_empty(),
            Cond::AnomalyOf(id) => ctx.anomalies.iter().any(|a| a == id),
            Cond::Docked => self.docked.is_some(),
            Cond::InFlight => self.docked.is_none(),
            Cond::Credits(cmp, v) => cmp.test(self.credits, *v),
            Cond::Contracts(n) => self.contracts.len() >= *n as usize,
            Cond::Rel(t, cmp, v) => self.resolve_target(t, ctx).is_some_and(|e| {
                let r = if e.kind == Kind::Contact {
                    self.rel(e).clamp(0.0, 1.0)
                } else {
                    self.rel(e)
                };
                cmp.test(f64::from(r), *v)
            }),
            Cond::Morale(cmp, v) => {
                let (n, sum) =
                    alive().fold((0.0, 0.0), |(n, s), p| (n + 1.0, s + f64::from(p.morale)));
                n > 0.0 && cmp.test(sum / n, *v)
            }
            Cond::Crew(n) => alive().count() >= *n as usize,
            Cond::Hold(id, kg) => d
                .resource_index(id)
                .is_some_and(|r| self.ship.cargo.kg[r] >= *kg),
            Cond::Seat(id) => d
                .cartels
                .iter()
                .position(|k| k.id == *id)
                .is_some_and(|i| self.trade.seats.contains(&i)),
            Cond::Wanted => self.is_wanted(),
            Cond::InDebt => self.bank.loan > 0.0,
            Cond::Shares => self.shares.iter().any(|(_, n)| *n > 0.0),
            Cond::Herd => !self.herd.is_empty(),
        }
    }

    /// Whether every condition of surprise `i` holds now.
    fn surprise_ready(&self, i: usize, ctx: &mut Ctx) -> bool {
        let data = std::sync::Arc::clone(&self.data);
        data.surprises[i]
            .when
            .iter()
            .all(|c| self.surprise_cond(c, ctx))
    }

    /// Words of a surprise with its placeholders resolved.
    fn surprise_words(&self, s: &str) -> String {
        let name = |f: Option<usize>, fallback: &str| {
            f.map_or_else(
                || fallback.to_string(),
                |i| self.data.factions[i].name.clone(),
            )
        };
        s.replace("{here}", &name(self.faction, "les espaces libres"))
            .replace("{ally}", &name(self.neighbour_faction(true), "un voisin"))
            .replace("{rival}", &name(self.neighbour_faction(false), "un rival"))
            .replace("{place}", &self.system.name)
    }

    fn apply_surprise_effects(&mut self, effects: &[Effect], ctx: &mut Ctx) {
        let data = std::sync::Arc::clone(&self.data);
        for e in effects {
            match e {
                Effect::Credits(v) => self.credits = (self.credits + v).max(0.0),
                Effect::Rel(t, delta) => {
                    if let Some(who) = self.resolve_target(t, ctx) {
                        if who.kind == Kind::Contact {
                            self.relate_alone(who, *delta as f32, false);
                        } else {
                            self.relate(who, *delta as f32);
                        }
                    }
                }
                Effect::Bond(a, b, w, days) => {
                    if let (Some(a), Some(b)) =
                        (self.resolve_target(a, ctx), self.resolve_target(b, ctx))
                    {
                        self.form_bond(a, b, *w as f32, *days);
                    }
                }
                Effect::Shock(key, mult, days) => {
                    let (pos, now) = (self.here_pos, self.time);
                    self.surprises.shocks.push(Shock {
                        pos,
                        key: key.clone(),
                        mult: *mult,
                        start: now,
                        end: now + days * DAY,
                    });
                    // The markets read the world once a day: make them read it again.
                    self.markets
                        .values_mut()
                        .for_each(|m| m.world_day = i64::MIN);
                }
                Effect::Cargo(id, kg) => {
                    if let Some(r) = data.resource_index(id) {
                        if *kg > 0.0 {
                            self.ship.cargo.add(&data, r, *kg);
                        } else {
                            let have = self.ship.cargo.kg[r];
                            self.ship.cargo.remove(&data, r, (-kg).min(have));
                        }
                    }
                }
                Effect::Morale(d) => {
                    for p in self.crew.members.iter_mut().filter(|p| p.health > 0.0) {
                        p.morale = (p.morale + *d as f32).clamp(0.0, 1.0);
                    }
                }
                Effect::Contract(id, kg, mult) => self.offer_surprise_contract(id, *kg, *mult),
                Effect::Research(v) => self.research_pool += v,
                Effect::ColonyHappiness(d) => {
                    let pos = self.here_pos;
                    let near: Vec<usize> = (0..self.colonies.len())
                        .filter(|&i| {
                            self.galaxy
                                .star(self.colonies[i].star)
                                .is_some_and(|e| e.distance_to(pos) <= COLONY_REACH_LY)
                        })
                        .collect();
                    for i in near {
                        let c = &mut self.colonies[i];
                        c.happiness = (c.happiness + d).clamp(0.0, 1.0);
                    }
                }
            }
        }
    }

    /// A contract offered at the nearest station of the system.
    fn offer_surprise_contract(&mut self, id: &str, kg: f64, mult: f64) {
        let (Some(r), Some((st, _))) = (self.data.resource_index(id), self.nearest_station())
        else {
            return;
        };
        let price = self.reference_price(self.here_pos, self.time, r);
        let day = (self.time / DAY).floor() as i64;
        let c = Contract {
            id: hash_coord(self.system.seed ^ 0x5075, st as i64, day, r as i64),
            dest_star: self.here,
            dest_seed: self.system.seed,
            dest_body: st,
            dest_name: self.bodies[st].name.clone(),
            dest_system: self.system.name.clone(),
            dist_ly: 0.0,
            resource: r,
            kg,
            reward: (kg * price * mult).round(),
            expires: self.time + OFFER_DAYS * DAY,
            faction: self.faction,
            pax: 0,
            pax_kind: 0,
        };
        if !self.surprises.offers.iter().any(|o| o.id == c.id) {
            self.surprises.offers.push(c);
        }
    }

    /// Contracts that surprises put on the table of the station the ship is docked at.
    pub(crate) fn surprise_offers_here(&self, st: usize) -> Vec<Contract> {
        self.surprises
            .offers
            .iter()
            .filter(|c| c.dest_star == self.here && c.dest_body == st && c.expires > self.time)
            .cloned()
            .collect()
    }

    fn news_tag(tag: &str) -> u8 {
        TAGS.iter().position(|t| *t == tag).unwrap_or(0) as u8
    }

    /// Once a day: read the world around the ship and let surprises happen.
    pub(crate) fn surprises_tick(&mut self) {
        let today = (self.time / DAY).floor() as i64;
        let now = self.time;
        self.surprises.pending.retain(|p| p.expires > now);
        self.surprises.offers.retain(|c| c.expires > now);
        self.surprises.shocks.retain(|s| s.end > now);
        if self.surprises.last_day == 0 {
            self.surprises.last_day = today;
            return;
        }
        if today <= self.surprises.last_day || self.data.surprises.is_empty() {
            return;
        }
        let from = self.surprises.last_day.max(today - CATCH_UP_DAYS);
        self.surprises.last_day = today;
        let mut ctx = self.surprise_ctx();
        for day in from + 1..=today {
            if let Some(i) = self.surprise_of_day(day, &mut ctx) {
                self.fire_surprise(i, day, &mut ctx);
            }
        }
    }

    /// The surprise that happens on `day` here, if any: of those whose conditions hold and whose
    /// cooldown is over, the one whose roll beat its chance by the widest margin.
    pub(crate) fn surprise_of_day(&self, day: i64, ctx: &mut Ctx) -> Option<usize> {
        let data = std::sync::Arc::clone(&self.data);
        let cell = cell_of(self.here_pos);
        let place = hash_coord(self.galaxy.seed ^ 0x5B5E, cell[0], cell[1], cell[2]) as i64;
        let mut best: Option<(usize, f64)> = None;
        for (i, s) in data.surprises.iter().enumerate() {
            let last = self
                .surprises
                .fired
                .iter()
                .find(|(k, _)| *k == i)
                .map(|(_, d)| *d);
            if last.is_some_and(|d| (day - d) as f64 <= s.cooldown) {
                continue;
            }
            let p = (DAILY_CHANCE * s.weight).min(1.0);
            let roll = unit(hash_coord(self.galaxy.seed ^ 0x5B5E, place, day, i as i64));
            if roll >= p {
                continue;
            }
            if !self.surprise_ready(i, ctx) {
                continue;
            }
            let margin = roll / p;
            if best.is_none_or(|(_, m)| margin < m) {
                best = Some((i, margin));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Make surprise `i` happen now (on `day`).
    pub(crate) fn fire_surprise(&mut self, i: usize, day: i64, ctx: &mut Ctx) {
        let data = std::sync::Arc::clone(&self.data);
        let s = &data.surprises[i];
        match self.surprises.fired.iter_mut().find(|(k, _)| *k == i) {
            Some(slot) => slot.1 = day,
            None => self.surprises.fired.push((i, day)),
        }
        self.surprises.count += 1;
        let text = self.surprise_words(&s.text);
        let tag = Self::news_tag(&s.tag);
        let now = self.time;
        self.news_push(now, tag, 0.0, text.clone());
        self.apply_surprise_effects(&s.effects, ctx);
        if s.choices.is_empty() {
            self.events.push(Event::Surprise(text));
        } else {
            self.surprises.pending.push(Pending {
                def: i,
                time: now,
                expires: now + s.window * DAY,
                text: text.clone(),
            });
            let n = self.surprises.pending.len();
            if n > PENDING_MAX {
                self.surprises.pending.drain(0..n - PENDING_MAX);
            }
            self.events
                .push(Event::Surprise(format!("{text} (J : CHOISIR)")));
        }
    }

    /// Answer the `k`-th waiting surprise with its `choice`-th choice.
    pub fn surprise_choose(&mut self, k: usize, choice: usize) -> Result<String, &'static str> {
        let p = self
            .surprises
            .pending
            .get(k)
            .ok_or("rien a decider")?
            .clone();
        let data = std::sync::Arc::clone(&self.data);
        let c = data.surprises[p.def]
            .choices
            .get(choice)
            .ok_or("choix inconnu")?;
        self.surprises.pending.remove(k);
        let mut ctx = self.surprise_ctx();
        self.apply_surprise_effects(&c.effects, &mut ctx);
        let words = self.surprise_words(&c.result);
        let tag = Self::news_tag(&data.surprises[p.def].tag);
        let now = self.time;
        self.news_push(now, tag, 0.0, words.clone());
        Ok(words)
    }

    /// Words of the waiting surprise `k` and the labels of its choices.
    pub fn pending_surprise(&self, k: usize) -> Option<(String, Vec<String>)> {
        let p = self.surprises.pending.get(k)?;
        let s = &self.data.surprises[p.def];
        Some((
            p.text.clone(),
            s.choices
                .iter()
                .map(|c| self.surprise_words(&c.label))
                .collect(),
        ))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::relations::tests::sim;

    fn index(s: &Sim, id: &str) -> usize {
        s.data.surprises.iter().position(|x| x.id == id).expect(id)
    }

    #[test]
    fn there_are_at_least_thirty_varied_surprises_with_known_tags() {
        let s = sim();
        let d = &s.data.surprises;
        assert!(d.len() >= 30, "{} surprises", d.len());
        for x in d {
            assert!(TAGS.contains(&x.tag.as_str()), "{}: tag {}", x.id, x.tag);
        }
        // Variety: many different kinds of conditions and of effects are used.
        let conds: std::collections::HashSet<String> = d
            .iter()
            .flat_map(|x| x.when.iter().map(|c| format!("{c:?}")))
            .map(|c| c.split('(').next().unwrap_or("").to_string())
            .collect();
        let effects: std::collections::HashSet<String> = d
            .iter()
            .flat_map(|x| {
                x.effects
                    .iter()
                    .chain(x.choices.iter().flat_map(|c| c.effects.iter()))
            })
            .map(|e| format!("{e:?}").split('(').next().unwrap_or("").to_string())
            .collect();
        assert!(conds.len() >= 18, "conditions used: {conds:?}");
        assert!(effects.len() >= 8, "effects used: {effects:?}");
        assert!(d.iter().filter(|x| !x.choices.is_empty()).count() >= 8);
    }

    /// Make condition `c` true in the world (and in the reading of it).
    fn satisfy(s: &mut Sim, ctx: &mut Ctx, c: &Cond) {
        let d = std::sync::Arc::clone(&s.data);
        match c {
            Cond::WorldEvent(id) => ctx
                .events
                .push(d.world_events.iter().position(|w| w.id == *id).unwrap()),
            Cond::AtWar => ctx.turmoil.at_war = true,
            Cond::Unrest(v) => ctx.turmoil.unrest = *v + 0.01,
            Cond::CartelHere(id) => {
                ctx.cartels[d.cartels.iter().position(|k| k.id == *id).unwrap()] = true
            }
            Cond::Incident => ctx.incident = true,
            Cond::Epidemic(ly) => {
                ctx.epidemic_ly = ly / 2.0;
                ctx.colony_ly = ly / 2.0;
            }
            Cond::ColonyNear(ly) => ctx.colony_ly = ly / 2.0,
            Cond::ColonyUnrest(v) => ctx.colony_unrest = *v + 0.01,
            Cond::Territory(id) => s.faction = d.factions.iter().position(|f| f.id == *id),
            Cond::Claimed => {}
            Cond::Civ => ctx.civ = Some(Some(77)),
            Cond::Anomaly => ctx.anomalies.push("comet"),
            Cond::AnomalyOf(id) => ctx.anomalies.push(
                d.anomalies_def
                    .iter()
                    .find(|a| a.id == *id)
                    .unwrap()
                    .id
                    .clone()
                    .leak(),
            ),
            Cond::Docked => s.docked = Some(0),
            Cond::InFlight => s.docked = None,
            Cond::Credits(cmp, v) => {
                s.credits = if *cmp == vector_data::surprise::Cmp::Gt {
                    v + 1.0
                } else {
                    v - 1.0
                }
            }
            Cond::Contracts(n) => {
                let (st, _) = s.nearest_station().expect("station");
                let offer = Contract {
                    id: 1,
                    dest_star: s.here,
                    dest_seed: s.system.seed,
                    dest_body: st,
                    dest_name: String::new(),
                    dest_system: String::new(),
                    dist_ly: 0.0,
                    resource: 0,
                    kg: 1.0,
                    reward: 1.0,
                    expires: 1e18,
                    faction: None,
                    pax: 0,
                    pax_kind: 0,
                };
                for k in 0..*n {
                    let mut o = offer.clone();
                    o.id += u64::from(k);
                    s.contracts.push(o);
                }
            }
            Cond::Rel(t, cmp, v) => {
                let e = s.resolve_target(t, ctx).expect("target resolves");
                let x = if *cmp == vector_data::surprise::Cmp::Gt {
                    (*v as f32 + 0.15).min(1.0)
                } else {
                    (*v as f32 - 0.15).max(-1.0)
                };
                s.relations.set(e, x);
            }
            Cond::Morale(cmp, v) => {
                let x = if *cmp == vector_data::surprise::Cmp::Gt {
                    (*v as f32 + 0.1).min(1.0)
                } else {
                    (*v as f32 - 0.1).max(0.0)
                };
                s.crew.members.iter_mut().for_each(|p| p.morale = x);
            }
            Cond::Crew(n) => {
                let role = crate::crew::Crew::role_index(&d, "engineer");
                let mut k = 0;
                while s.crew.members.iter().filter(|p| p.health > 0.0).count() < *n as usize {
                    k += 1;
                    let p = crate::crew::generate_person(&d, 900 + k, None);
                    s.crew.hire(&d, p, role, 0.0);
                }
            }
            Cond::Hold(r, kg) => {
                s.ship.cargo.cap_mass = 1.0e7;
                s.ship.cargo.cap_vol = 1.0e7;
                let r = d.resource_index(r).unwrap();
                s.ship.cargo.add(&d, r, *kg);
            }
            Cond::Seat(id) => s
                .trade
                .seats
                .push(d.cartels.iter().position(|k| k.id == *id).unwrap()),
            Cond::Wanted => {
                let f = s.faction.expect("territory");
                s.wanted_until.insert(f, s.time + 1.0e6);
            }
            Cond::InDebt => s.bank.loan = 1000.0,
            Cond::Shares => s.shares.push((0, 1.0)),
            Cond::Herd => s.herd.push(crate::livestock::Animal {
                species: 0,
                male: true,
                born: 0.0,
                genes: [[0; 2]; 4],
                hunger: 0.0,
                last_birth: 0.0,
                gen: 0,
            }),
        }
    }

    /// A number that moves when anything a surprise can touch moves.
    fn fingerprint(s: &Sim) -> Vec<f64> {
        let mut v = vec![
            s.credits,
            s.research_pool,
            s.surprises.shocks.len() as f64,
            s.surprises.offers.len() as f64,
            s.surprises.pending.len() as f64,
            s.relations.bonds.len() as f64,
            s.ship.cargo.mass,
        ];
        v.push(s.crew.members.iter().map(|p| f64::from(p.morale)).sum());
        v.extend(s.relations.values.iter().map(|(_, x)| f64::from(*x)));
        v
    }

    #[test]
    fn every_surprise_can_happen_once_its_conditions_hold_and_changes_the_world() {
        let base = sim();
        let conc = base
            .data
            .factions
            .iter()
            .position(|f| f.id == "concorde")
            .unwrap();
        for (i, def) in base.data.surprises.iter().enumerate() {
            let mut s = sim();
            s.faction = Some(conc);
            let mut ctx = s.surprise_ctx();
            // A fresh world gives nothing for free.
            let fresh = s.surprise_ready(i, &mut ctx);
            let mut conds: Vec<&Cond> = def.when.iter().collect();
            conds.sort_by_key(|c| !matches!(c, Cond::Crew(_)));
            for c in conds {
                satisfy(&mut s, &mut ctx, c);
            }
            assert!(
                s.surprise_ready(i, &mut ctx),
                "{}: conditions not reachable",
                def.id
            );
            let (news, before) = (s.news.len(), fingerprint(&s));
            s.fire_surprise(i, 50, &mut ctx);
            assert_eq!(s.news.len(), news + 1, "{}: told in the news", def.id);
            let only_colonies = def
                .effects
                .iter()
                .all(|e| matches!(e, Effect::ColonyHappiness(_)))
                && def.choices.is_empty();
            let changed = fingerprint(&s) != before;
            assert!(
                changed || only_colonies,
                "{}: fired but changed nothing (fresh ready: {fresh})",
                def.id
            );
            // Every choice can be taken, ends the wait and is told.
            for k in 0..def.choices.len() {
                let mut t = sim();
                t.faction = Some(conc);
                let mut c2 = t.surprise_ctx();
                let mut conds: Vec<&Cond> = def.when.iter().collect();
                conds.sort_by_key(|c| !matches!(c, Cond::Crew(_)));
                for c in conds {
                    satisfy(&mut t, &mut c2, c);
                }
                t.fire_surprise(i, 50, &mut c2);
                let n = t.news.len();
                t.surprise_choose(0, k).expect("choice works");
                assert_eq!(t.news.len(), n + 1, "{} choice {k} is told", def.id);
                assert!(t.surprises.pending.is_empty());
            }
        }
    }

    #[test]
    fn a_fresh_world_is_calm_most_surprises_need_something_to_have_happened() {
        let mut s = sim();
        s.faction = None;
        s.docked = None;
        let mut ctx = s.surprise_ctx();
        let n = (0..s.data.surprises.len())
            .filter(|&i| s.surprise_ready(i, &mut ctx))
            .count();
        assert!(
            n * 4 < s.data.surprises.len(),
            "{n} surprises ready from nothing"
        );
    }

    #[test]
    fn conditions_read_the_real_world_and_a_surprise_needs_all_of_them() {
        let mut s = sim();
        let i = index(&s, "cadeau_allie");
        let mut ctx = s.surprise_ctx();
        let f = s.faction.expect("faction");
        s.docked = None;
        s.relations.set(Entity::faction(f), 0.9);
        assert!(!s.surprise_ready(i, &mut ctx), "not docked: no gift");
        s.docked = Some(0);
        assert!(s.surprise_ready(i, &mut ctx));
        s.relations.set(Entity::faction(f), 0.2);
        assert!(
            !s.surprise_ready(i, &mut ctx),
            "a lukewarm friend gives nothing"
        );
    }

    #[test]
    fn rolls_are_deterministic_local_and_rare() {
        let mut s = sim();
        let f = s.faction.expect("faction");
        s.docked = Some(0);
        s.relations.set(Entity::faction(f), 0.9);
        let mut ctx = s.surprise_ctx();
        let days: Vec<Option<usize>> = (1..400).map(|d| s.surprise_of_day(d, &mut ctx)).collect();
        let again: Vec<Option<usize>> = (1..400).map(|d| s.surprise_of_day(d, &mut ctx)).collect();
        assert_eq!(days, again, "same world, same days, same surprises");
        let n = days.iter().flatten().count();
        assert!(n > 0, "a very warm relation brings surprises in a year");
        assert!(n < 200, "but they stay rare ({n} of 399 days)");
        // Elsewhere in the galaxy the rolls differ (they are a matter of place).
        let here = s.here_pos;
        s.here_pos = [here[0] + 600.0, here[1], here[2]];
        let far: Vec<Option<usize>> = (1..400).map(|d| s.surprise_of_day(d, &mut ctx)).collect();
        assert_ne!(days, far, "another sector rolls other days");
    }

    #[test]
    fn firing_tells_the_news_applies_effects_and_respects_the_cooldown() {
        let mut s = sim();
        let f = s.faction.expect("faction");
        s.docked = Some(0);
        s.relations.set(Entity::faction(f), 0.9);
        let i = index(&s, "cadeau_allie");
        let (credits, news) = (s.credits, s.news.len());
        let mut ctx = s.surprise_ctx();
        s.fire_surprise(i, 100, &mut ctx);
        assert!(s.credits > credits + 2000.0, "the gift arrived");
        assert_eq!(s.news.len(), news + 1, "and was told in the news");
        assert!(s
            .news
            .last()
            .unwrap()
            .text
            .contains(&s.data.factions[f].name));
        assert!(s.events.iter().any(|e| matches!(e, Event::Surprise(_))));
        // Within its cooldown it cannot be picked again, whatever the roll.
        let cd = s.data.surprises[i].cooldown as i64;
        for day in 101..100 + cd {
            assert_ne!(
                s.surprise_of_day(day, &mut ctx),
                Some(i),
                "cooldown day {day}"
            );
        }
    }

    #[test]
    fn a_war_and_a_cartel_make_a_shortage_that_moves_the_prices() {
        let mut s = sim();
        let i = index(&s, "penurie_guerre_metaux");
        let iron = s.data.resource_index("iron").expect("iron");
        let before = s.shock_factor(s.here_pos, s.time, iron);
        assert!((before - 1.0).abs() < 1e-12);
        let cart = s
            .data
            .cartels
            .iter()
            .position(|c| c.id == "metaux_unis")
            .unwrap();
        let rel = s.rel(Entity::cartel(cart));
        let mut ctx = s.surprise_ctx();
        s.fire_surprise(i, 5, &mut ctx);
        let mid = s.time + 10.0 * DAY;
        let after = s.shock_factor(s.here_pos, mid, iron);
        assert!(after > 1.3, "metals are dear: {after}");
        assert!(
            s.rel(Entity::cartel(cart)) < rel,
            "the cartel resents the leak"
        );
        // Local in space and in time.
        let far = [s.here_pos[0] + 500.0, s.here_pos[1], s.here_pos[2]];
        assert!((s.shock_factor(far, mid, iron) - 1.0).abs() < 1e-12);
        assert!((s.shock_factor(s.here_pos, s.time + 40.0 * DAY, iron) - 1.0).abs() < 1e-12);
        // The market of the station reads it.
        let gold = s.data.resource_index("gold").expect("gold");
        assert!(
            (s.shock_factor(s.here_pos, mid, gold) - 1.0).abs() < 1e-12,
            "gold is not a metal here"
        );
    }

    #[test]
    fn a_surprise_with_choices_waits_for_an_answer_and_lapses() {
        let mut s = sim();
        let f = s.faction.expect("faction");
        let i = index(&s, "invitation_gala");
        s.credits = 5000.0;
        let mut ctx = s.surprise_ctx();
        s.fire_surprise(i, 9, &mut ctx);
        assert_eq!(s.surprises.pending.len(), 1);
        let (text, labels) = s.pending_surprise(0).expect("pending");
        assert!(!text.contains('{') && labels.len() == 2);
        assert!(s.surprise_choose(0, 7).is_err(), "no such choice");
        assert!(
            s.surprises.pending.len() == 1,
            "a bad answer leaves it waiting"
        );
        let rel = s.rel(Entity::faction(f));
        let news = s.news.len();
        s.surprise_choose(0, 0).expect("accept");
        assert!(s.rel(Entity::faction(f)) > rel + 0.05);
        assert!(s.credits < 5000.0);
        assert!(s.surprises.pending.is_empty() && s.news.len() == news + 1);
        // Unanswered, it lapses.
        s.fire_surprise(i, 99, &mut ctx);
        s.time += 20.0 * DAY;
        s.surprises_tick();
        assert!(s.surprises.pending.is_empty(), "the occasion passed");
    }

    #[test]
    fn a_surprise_contract_is_offered_at_the_station_and_taken_like_any_other() {
        let mut s = sim();
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        let i = index(&s, "epidemie_contrat");
        let mut ctx = s.surprise_ctx();
        s.fire_surprise(i, 3, &mut ctx);
        assert_eq!(s.surprises.offers.len(), 1);
        let offer = s
            .offered_contracts()
            .into_iter()
            .find(|c| c.id == s.surprises.offers[0].id)
            .expect("on the board");
        assert!(offer.reward > 0.0 && offer.kg == 400.0);
        s.accept_contract(offer.clone()).expect("accept");
        assert!(
            !s.offered_contracts().iter().any(|c| c.id == offer.id),
            "taken: no longer offered"
        );
    }

    #[test]
    fn an_effect_with_an_unknown_target_is_skipped_not_applied_elsewhere() {
        let mut s = sim();
        s.faction = None;
        let i = index(&s, "cadeau_allie");
        let mut ctx = s.surprise_ctx();
        assert!(
            !s.surprise_ready(i, &mut ctx),
            "no territory, no ally to give"
        );
        let before = s.relations.clone();
        s.apply_surprise_effects(&[Effect::Rel(Target::Here, 0.3)], &mut ctx);
        assert_eq!(s.relations, before);
    }

    #[test]
    fn surprise_state_survives_a_save() {
        let mut s = sim();
        let i = index(&s, "invitation_gala");
        let mut ctx = s.surprise_ctx();
        s.fire_surprise(i, 9, &mut ctx);
        s.relate(Entity::faction(0), 0.2);
        let bytes = s.save_bytes().expect("save");
        let data = std::sync::Arc::clone(&s.data);
        let t = Sim::load_bytes(&bytes, data).expect("load");
        assert_eq!(t.surprises.pending, s.surprises.pending);
        assert_eq!(t.surprises.fired, s.surprises.fired);
        assert_eq!(t.surprises.count, s.surprises.count);
        assert_eq!(t.relations, s.relations);
    }
}
