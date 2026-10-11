//! The registry of relations: ONE place that says how every entity of the world feels about the
//! player and about the others. Factions, native civilisations, cartels, black-market contacts and
//! companies are all `Entity` values; their goodwill toward the player lives in `Relations`
//! (-1 hostile .. +1 devoted), and the ties between them (alliances, rivalries) are read from the
//! data (`factions.ron` allies and enemies, the headquarters of cartels and companies) plus the
//! `Bond`s the world forms during play (see `surprises`). What you do to one is felt by its
//! neighbours, attenuated and over one hop only: a friend of my enemy is a little less my friend.
//! Prices, docking, contract rewards, dialogues, the black market and the surprises all read it.
use crate::Sim;
use serde::{Deserialize, Serialize};

/// Share of a change that reaches the neighbours of the entity it was made to.
pub const ATTENUATION: f32 = 0.3;
/// Changes remembered, newest last.
pub const HISTORY_MAX: usize = 40;
/// Scale of the old faction reputation (-100 .. 100) over the registry's -1 .. 1.
pub const FACTION_SCALE: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Kind {
    Faction,
    Civ,
    Cartel,
    Contact,
    Company,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Faction => "FACTION",
            Kind::Civ => "CIVILISATION",
            Kind::Cartel => "CARTEL",
            Kind::Contact => "CONTACT",
            Kind::Company => "ENTREPRISE",
        }
    }
}

/// Who: the kind, and the index (faction, cartel, company, the faction of a contact) or the world
/// key (civilisation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Entity {
    pub kind: Kind,
    pub id: u64,
}

impl Entity {
    pub fn faction(i: usize) -> Self {
        Entity {
            kind: Kind::Faction,
            id: i as u64,
        }
    }
    pub fn cartel(i: usize) -> Self {
        Entity {
            kind: Kind::Cartel,
            id: i as u64,
        }
    }
    pub fn company(i: usize) -> Self {
        Entity {
            kind: Kind::Company,
            id: i as u64,
        }
    }
    pub fn contact(faction: usize) -> Self {
        Entity {
            kind: Kind::Contact,
            id: faction as u64,
        }
    }
    pub fn civ(key: u64) -> Self {
        Entity {
            kind: Kind::Civ,
            id: key,
        }
    }
}

/// A tie formed during play between two entities: positive is an alliance, negative a rivalry.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    pub a: Entity,
    pub b: Entity,
    pub w: f32,
    /// Game time at which it dissolves.
    pub until: f64,
}

/// One remembered change of the registry.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub time: f64,
    pub who: Entity,
    pub delta: f32,
    /// True when it is the echo of a change made to a neighbour.
    pub echo: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Relations {
    /// Goodwill toward the player, by entity (absent = 0).
    pub values: Vec<(Entity, f32)>,
    pub bonds: Vec<Bond>,
    pub history: Vec<Change>,
    /// Names of the entities the data cannot name (civilisations are generated).
    pub labels: Vec<(Entity, String)>,
}

impl Relations {
    pub fn get(&self, e: Entity) -> f32 {
        self.values
            .iter()
            .find(|(k, _)| *k == e)
            .map_or(0.0, |(_, v)| *v)
    }

    /// The lowest value an entity of this kind can fall to: trust of a contact has no negative.
    fn floor(e: Entity) -> f32 {
        if e.kind == Kind::Contact {
            0.0
        } else {
            -1.0
        }
    }

    pub fn set(&mut self, e: Entity, v: f32) {
        let v = v.clamp(Self::floor(e), 1.0);
        match self.values.iter_mut().find(|(k, _)| *k == e) {
            Some(slot) => slot.1 = v,
            None => {
                self.values.push((e, v));
                self.values.sort_by_key(|(k, _)| *k);
            }
        }
    }

    pub fn add(&mut self, e: Entity, delta: f32) {
        let v = self.get(e) + delta;
        self.set(e, v);
    }

    pub fn label(&mut self, e: Entity, name: &str) {
        if !self.labels.iter().any(|(k, _)| *k == e) {
            self.labels.push((e, name.to_string()));
        }
    }

    fn remember(&mut self, c: Change) {
        self.history.push(c);
        let n = self.history.len();
        if n > HISTORY_MAX {
            self.history.drain(0..n - HISTORY_MAX);
        }
    }
}

/// One line of the relations screen.
#[derive(Clone, Debug, PartialEq)]
pub struct RelRow {
    pub who: Entity,
    pub name: String,
    pub value: f32,
}

impl Sim {
    /// Goodwill of `e` toward the player, -1 .. 1.
    pub fn rel(&self, e: Entity) -> f32 {
        self.relations.get(e)
    }

    /// Display name of an entity.
    pub fn entity_name(&self, e: Entity) -> String {
        let d = &self.data;
        let named = match e.kind {
            Kind::Faction => d.factions.get(e.id as usize).map(|f| f.name.clone()),
            Kind::Cartel => d.cartels.get(e.id as usize).map(|c| c.name.clone()),
            Kind::Company => d.companies.get(e.id as usize).map(|c| c.name.clone()),
            Kind::Contact => d
                .factions
                .get(e.id as usize)
                .map(|f| format!("Contact de {}", f.name)),
            Kind::Civ => self
                .relations
                .labels
                .iter()
                .find(|(k, _)| *k == e)
                .map(|(_, n)| n.clone()),
        };
        named.unwrap_or_else(|| format!("{} #{:04X}", e.kind.label(), e.id & 0xFFFF))
    }

    /// The ties of `e` and their weight (+1 allied, -1 rival): the data, plus the live bonds.
    pub fn rel_neighbours(&self, e: Entity) -> Vec<(Entity, f32)> {
        let d = &self.data;
        let mut out: Vec<(Entity, f32)> = Vec::new();
        let fac_id = |i: usize| d.factions.get(i).map(|f| f.id.as_str());
        let fac_idx = |id: &str| d.factions.iter().position(|f| f.id == id);
        match e.kind {
            Kind::Faction => {
                let i = e.id as usize;
                if let Some(me) = d.factions.get(i) {
                    for (j, other) in d.factions.iter().enumerate().filter(|(j, _)| *j != i) {
                        let mut w = 0.0;
                        for (list_a, list_b, sign) in [
                            (&me.allies, &other.allies, 1.0),
                            (&me.enemies, &other.enemies, -1.0),
                        ] {
                            if list_a.contains(&other.id) || list_b.contains(&me.id) {
                                w += sign;
                            }
                        }
                        if w != 0.0 {
                            out.push((Entity::faction(j), f32::clamp(w, -1.0, 1.0)));
                        }
                    }
                    for (c, k) in d.cartels.iter().enumerate() {
                        if k.hq == me.id {
                            out.push((Entity::cartel(c), 1.0));
                        }
                    }
                    for (c, k) in d.companies.iter().enumerate() {
                        if k.hq == me.id {
                            out.push((Entity::company(c), 1.0));
                        }
                    }
                }
            }
            Kind::Cartel => {
                if let Some(h) = d.cartels.get(e.id as usize).and_then(|k| fac_idx(&k.hq)) {
                    out.push((Entity::faction(h), 1.0));
                }
            }
            Kind::Company => {
                if let Some(h) = d.companies.get(e.id as usize).and_then(|k| fac_idx(&k.hq)) {
                    out.push((Entity::faction(h), 1.0));
                }
            }
            // The people of a world answer to the territory it lies in.
            Kind::Civ => {
                if let Some(f) = self.faction.filter(|f| fac_id(*f).is_some()) {
                    out.push((Entity::faction(f), 0.5));
                }
            }
            Kind::Contact => {}
        }
        for b in self.relations.bonds.iter().filter(|b| b.until > self.time) {
            if b.a == e {
                out.push((b.b, b.w));
            } else if b.b == e {
                out.push((b.a, b.w));
            }
        }
        out
    }

    /// Change the goodwill of `e` by `delta` and let it echo, attenuated, on its neighbours
    /// (their allies feel it the same way, their rivals the opposite way). One hop only.
    pub fn relate(&mut self, e: Entity, delta: f32) {
        self.relate_alone(e, delta, false);
        for (n, w) in self.rel_neighbours(e) {
            self.relate_alone(n, delta * w * ATTENUATION, true);
        }
    }

    /// Change the goodwill of `e` only (no echo).
    pub fn relate_alone(&mut self, e: Entity, delta: f32, echo: bool) {
        if delta == 0.0 {
            return;
        }
        let before = self.relations.get(e);
        self.relations.add(e, delta);
        let applied = self.relations.get(e) - before;
        if applied != 0.0 {
            let time = self.time;
            self.relations.remember(Change {
                time,
                who: e,
                delta: applied,
                echo,
            });
        }
    }

    /// A kill during an exploration: clearing threats earns the thanks of the territory (a boss
    /// much more) and of the people of the world; killing peaceful wildlife on a world whose
    /// people live there is held against you.
    pub(crate) fn kill_relations(&mut self, body: Option<usize>, hunts: bool, boss: bool) {
        let civ = body.filter(|b| self.civilisation(*b).is_some());
        if hunts || boss {
            self.change_reputation(self.faction, if boss { 4.0 } else { 0.15 });
            if let Some(b) = civ {
                self.change_civ_reputation(b, 0.01);
            }
        } else if let Some(b) = civ {
            self.change_civ_reputation(b, -0.02);
        }
    }

    /// Dissolve the bonds that ended.
    pub(crate) fn relations_tick(&mut self) {
        let now = self.time;
        self.relations.bonds.retain(|b| b.until > now);
    }

    /// Form a tie between two entities for `days` days (an alliance, or a rivalry if `w` < 0).
    pub fn form_bond(&mut self, a: Entity, b: Entity, w: f32, days: f64) {
        let until = self.time + days * crate::bourse::DAY;
        self.relations.bonds.push(Bond {
            a,
            b,
            w: w.clamp(-1.0, 1.0),
            until,
        });
    }

    /// Every faction, then every other entity with a non-zero feeling, most friendly first.
    pub fn relation_rows(&self) -> Vec<RelRow> {
        let mut rows: Vec<RelRow> = (0..self.data.factions.len())
            .map(Entity::faction)
            .map(|e| RelRow {
                who: e,
                name: self.entity_name(e),
                value: self.rel(e),
            })
            .collect();
        let mut rest: Vec<RelRow> = self
            .relations
            .values
            .iter()
            .filter(|(e, v)| e.kind != Kind::Faction && v.abs() > 0.004)
            .map(|(e, v)| RelRow {
                who: *e,
                name: self.entity_name(*e),
                value: *v,
            })
            .collect();
        rest.sort_by(|a, b| b.value.total_cmp(&a.value));
        rows.append(&mut rest);
        rows
    }

    /// Whom `e` is allied with and whom it opposes, by name.
    pub fn ties_of(&self, e: Entity) -> (Vec<String>, Vec<String>) {
        let (mut friends, mut foes) = (Vec::new(), Vec::new());
        for (n, w) in self.rel_neighbours(e) {
            if n.kind == Kind::Company {
                continue;
            }
            if w > 0.0 {
                friends.push(self.entity_name(n));
            } else if w < 0.0 {
                foes.push(self.entity_name(n));
            }
        }
        (friends, foes)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    pub(crate) fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")))
    }

    fn fac(s: &Sim, id: &str) -> usize {
        s.data.factions.iter().position(|f| f.id == id).expect(id)
    }

    #[test]
    fn a_favour_echoes_on_allies_and_against_enemies_over_one_hop_only() {
        let mut s = sim();
        let (con, hel, heg) = (fac(&s, "concorde"), fac(&s, "helios"), fac(&s, "hegemonie"));
        s.relate(Entity::faction(con), 0.5);
        assert_eq!(s.rel(Entity::faction(con)), 0.5);
        let ally = s.rel(Entity::faction(hel));
        let foe = s.rel(Entity::faction(heg));
        assert!(
            (ally - 0.5 * ATTENUATION).abs() < 1e-5,
            "ally shares {ally}"
        );
        assert!(
            (foe + 0.5 * ATTENUATION).abs() < 1e-5,
            "enemy resents {foe}"
        );
        // One hop: the ally's own allies are not touched by the echo (unless they are also ours).
        let echo_only: Vec<_> = s.relations.history.iter().filter(|c| c.echo).collect();
        assert!(!echo_only.is_empty());
        assert!(s.relations.history.iter().filter(|c| !c.echo).count() == 1);
    }

    #[test]
    fn values_are_clamped_and_a_contact_never_distrusts_below_zero() {
        let mut s = sim();
        s.relate_alone(Entity::faction(0), 5.0, false);
        assert_eq!(s.rel(Entity::faction(0)), 1.0);
        s.relate_alone(Entity::contact(0), -0.7, false);
        assert_eq!(s.rel(Entity::contact(0)), 0.0);
        s.relate_alone(Entity::civ(9), -3.0, false);
        assert_eq!(s.rel(Entity::civ(9)), -1.0);
    }

    #[test]
    fn the_old_reputation_scale_is_a_view_of_the_registry() {
        let mut s = sim();
        s.change_reputation(Some(0), 12.0);
        assert!((s.reputation_with(Some(0)) - 12.0).abs() < 1e-3);
        assert!((s.rel(Entity::faction(0)) - 0.12).abs() < 1e-5);
        s.change_reputation(Some(0), -500.0);
        assert_eq!(s.reputation_with(Some(0)), -100.0);
    }

    #[test]
    fn a_bond_makes_rivals_of_friends_and_ends_on_time() {
        let mut s = sim();
        let (a, b) = (
            Entity::faction(fac(&s, "ligue")),
            Entity::faction(fac(&s, "ceintures")),
        );
        s.form_bond(a, b, -1.0, 10.0);
        s.relate(a, 0.4);
        let echo = s.rel(b);
        assert!(echo < 0.0, "a rival of my friend is cooler to me: {echo}");
        s.time += 11.0 * crate::bourse::DAY;
        s.relations_tick();
        assert!(s.relations.bonds.is_empty());
    }

    #[test]
    fn killing_in_exploration_pleases_or_angers_the_territory_and_the_natives() {
        let mut s = sim();
        s.faction = Some(0);
        let rep = s.reputation_with(Some(0));
        s.kill_relations(None, true, false);
        let after_beast = s.reputation_with(Some(0));
        assert!(after_beast > rep, "a threat cleared is thanked");
        s.kill_relations(None, false, false);
        assert_eq!(
            s.reputation_with(Some(0)),
            after_beast,
            "wildlife alone: nothing"
        );
        s.kill_relations(None, true, true);
        assert!(
            s.reputation_with(Some(0)) > after_beast + 3.0,
            "a boss is worth much more"
        );
        // On a world with natives.
        let (mut t, b) = (1..60u64)
            .find_map(|seed| {
                let t = new_sim(seed, Arc::clone(&s.data));
                t.first_civilised_world().map(|b| (t, b))
            })
            .expect("a civilised world somewhere");
        t.faction = Some(0);
        let civ0 = t.civ_reputation(b);
        t.kill_relations(Some(b), false, false);
        assert!(
            t.civ_reputation(b) < civ0,
            "peaceful wildlife killed in their land"
        );
        let low = t.civ_reputation(b);
        t.kill_relations(Some(b), true, false);
        assert!(t.civ_reputation(b) > low, "a beast that hunted is no loss");
        assert!(
            t.relations.labels.iter().any(|(e, _)| e.kind == Kind::Civ),
            "the people are named"
        );
    }

    #[test]
    fn history_is_capped_and_every_entity_has_a_name() {
        let mut s = sim();
        for k in 0..(HISTORY_MAX + 10) {
            s.relate_alone(Entity::faction(k % 3), 0.001, false);
        }
        assert_eq!(s.relations.history.len(), HISTORY_MAX);
        for r in s.relation_rows() {
            assert!(!r.name.is_empty());
        }
        let (friends, foes) = s.ties_of(Entity::faction(fac(&s, "concorde")));
        assert!(!friends.is_empty() && !foes.is_empty());
    }

    #[test]
    fn delivering_a_contract_pleases_its_people_and_good_standing_pays_better() {
        let mut s = sim();
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        let f = s.faction.expect("faction");
        let offer = {
            let mut o = s.offered_contracts();
            o.retain(|c| c.dest_body == st && c.dest_star == s.here);
            o.remove(0)
        };
        let data = Arc::clone(&s.data);
        let mut pays = Vec::new();
        for (k, rep) in [-40.0f32, 60.0].into_iter().enumerate() {
            s.relations.set(Entity::faction(f), rep / FACTION_SCALE);
            let mut c = offer.clone();
            c.id += 1 + k as u64;
            s.contracts.push(c.clone());
            s.ship.cargo.add(&data, c.resource, c.kg);
            let before = s.rel(Entity::faction(f));
            pays.push(s.deliver_contract(0).expect("delivers"));
            assert!(s.rel(Entity::faction(f)) > before, "delivery pleases");
        }
        assert!(pays[1] > pays[0], "standing pays: {pays:?}");
    }
}
