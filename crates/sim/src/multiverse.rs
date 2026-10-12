//! High science fiction (E8): portals, other dimensions and time travel.
//!
//! A portal is a rare anomaly. Natural ones lead somewhere far away (a wormhole, one way) or into
//! a dimension: another version of the same star with its own rules (see `data/dimensions.ron`),
//! where a portal always brings you back. Time is kept in anchors: a snapshot of the whole game
//! that can be rewound to. You then remember what happened in the abandoned timeline: the world's
//! events are a pure function of the seed, so what you remember is true, and shows up as certain
//! rumours. Every rewind leaves a paradox that makes the crew unstable. Stasis skips ahead in time
//! without ageing anything aboard.
use crate::bourse::DAY;
use crate::voyage::{JumpState, SystemDelta};
use crate::{Event, ShipInput, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng, Vec3};
use vector_data::Dimension;
use vector_world::galaxy::StarEntry;
use vector_world::{generate_system, generate_system_in, portal_index};

/// Distance within which a portal can be used (m).
pub const PORTAL_RANGE: f64 = 50_000.0;
/// Share of the tank a crossing burns.
pub const PORTAL_FUEL: f64 = 0.05;
/// Share of natural portals that are wormholes (the rest open on a dimension).
pub const WORMHOLE_SHARE: f64 = 0.35;
/// Distance of a wormhole's other end (ly).
pub const WORMHOLE_LY: (f64, f64) = (300.0, 3000.0);
/// Anchors held at most, what one costs, what a rewind costs.
pub const MAX_ANCHORS: usize = 3;
pub const ANCHOR_FEE: f64 = 1500.0;
pub const REWIND_FEE: f64 = 2500.0;
/// How far back the memory of an abandoned timeline reaches ahead of now (days).
pub const MEMORY_DAYS: f64 = 90.0;
/// Daily odds of a temporal glitch, per paradox point, and the instability it adds to a member.
pub const GLITCH_ODDS: f64 = 0.02;
pub const GLITCH_INSTABILITY: f32 = 0.15;
/// Days of calm that wash away one paradox point.
pub const PARADOX_DECAY_DAYS: f64 = 30.0;
/// Relay portals built in colonies: the credits asked per light year, the building's id.
pub const RELAY_ID: &str = "portal_relay";
pub const RELAY_CR_PER_LY: f64 = 2.0;
/// Time loops: share of systems that hold one, days a loop lasts, what breaking it costs.
pub const LOOP_CHANCE: f64 = 0.04;
pub const LOOP_DAYS: f64 = 3.0;
pub const LOOP_ARTIFACTS: f64 = 5.0;
/// Echoes: smallest gain that leaves one, days after the rewind, the odds of winning the fight.
pub const ECHO_MIN: f64 = 500.0;
pub const ECHO_DELAY_DAYS: f64 = 2.0;
pub const ECHO_FIGHT_ODDS: f64 = 0.6;
pub const ECHO_HELP_FEE: f64 = 800.0;
/// Outposts in dimensions: price, most held, kg produced per day at ore x1.
pub const OUTPOST_COST: f64 = 50_000.0;
pub const MAX_OUTPOSTS: usize = 5;
pub const OUTPOST_KG_DAY: f64 = 40.0;
/// Stasis: cost per day, longest stay (days).
pub const STASIS_COST: f64 = 100.0;
pub const STASIS_MAX: f64 = 365.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Anchor {
    pub time: f64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Multiverse {
    /// 0 is our dimension, otherwise 1 + index in `GameData::dimensions`.
    pub dimension: u32,
    /// Date (our clock) the current dimension was entered.
    pub entered: f64,
    pub last_day: i64,
    pub anchors: Vec<Anchor>,
    pub paradox: u32,
    /// Latest date reached in an abandoned timeline: what happens before it is remembered.
    pub memory_until: f64,
    pub last_rewind: f64,
    /// Loops of time: the date the current one began (if in a looping system) and those broken.
    pub loop_start: Option<f64>,
    pub loops_done: u32,
    pub broken: Vec<vector_world::galaxy::StarRef>,
    /// An echo of the abandoned timeline waiting to be met: (credits it carries, date it shows).
    pub echo: Option<(f64, f64)>,
    /// Outposts in dimensions: (star, dimension, kg waiting).
    pub outposts: Vec<(vector_world::galaxy::StarRef, u32, f64)>,
    /// Clues found in precursor ruins about the Fermi paradox.
    #[serde(default)]
    pub exotic_clues: u32,
}

/// Where a portal leads.
#[derive(Clone, Debug)]
pub enum Gate {
    /// One way to a distant star.
    Wormhole(StarEntry),
    /// Into dimension `d` (1-based).
    Dimension(u32),
    /// Back to our own dimension.
    Home,
}

impl Sim {
    pub fn dimension_def(&self) -> Option<&Dimension> {
        match self.multiverse.dimension {
            0 => None,
            d => self.data.dimensions.get(d as usize - 1),
        }
    }

    pub fn ore_scale(&self) -> f64 {
        self.dimension_def().map_or(1.0, |d| d.ore_scale)
    }

    pub fn hazard_scale(&self) -> f64 {
        self.dimension_def().map_or(1.0, |d| d.hazard_scale)
    }

    pub fn research_scale(&self) -> f64 {
        self.dimension_def().map_or(1.0, |d| d.research_scale)
    }

    /// The portal within reach, if any.
    pub fn portal_in_range(&self) -> Option<usize> {
        let i = portal_index(&self.system)?;
        ((self.bodies[i].pos - self.ship.pos).length() <= PORTAL_RANGE).then_some(i)
    }

    /// Where portal `i` of this system leads.
    pub fn gate_of(&self, i: usize) -> Gate {
        if self.multiverse.dimension > 0 {
            return Gate::Home;
        }
        let h = |k: i64| hash_coord(self.system.seed ^ 0x6A7E, i as i64, k, 3);
        let unit = |k: i64| (h(k) >> 11) as f64 / (1u64 << 53) as f64;
        if unit(0) < WORMHOLE_SHARE {
            // A point far away in a direction drawn from the portal; the nearest star there.
            let dist = WORMHOLE_LY.0 + (WORMHOLE_LY.1 - WORMHOLE_LY.0) * unit(1);
            let (a, z) = (unit(2) * std::f64::consts::TAU, unit(3) * 2.0 - 1.0);
            let r = (1.0 - z * z).sqrt();
            let p = [
                self.here_pos[0] + dist * r * a.cos(),
                self.here_pos[1] + dist * z * 0.3,
                self.here_pos[2] + dist * r * a.sin(),
            ];
            let mut stars = self.galaxy.nearby(p, 250.0);
            stars.sort_by(|a, b| a.distance_to(p).total_cmp(&b.distance_to(p)));
            if let Some(s) = stars.into_iter().next() {
                return Gate::Wormhole(s);
            }
        }
        let n = self.data.dimensions.len().max(1) as u64;
        Gate::Dimension(1 + (h(4) % n) as u32)
    }

    fn stash_here(&mut self) {
        let old = SystemDelta {
            mined: std::mem::take(&mut self.mined),
            markets: std::mem::take(&mut self.markets),
            scan: std::mem::take(&mut self.scan),
            scan_progress: std::mem::take(&mut self.scan_progress),
            explored: std::mem::take(&mut self.explored),
        };
        self.deltas.insert(self.here, old);
    }

    fn star_seed(&self) -> Result<u64, &'static str> {
        self.galaxy
            .star(self.here)
            .map(|e| e.seed)
            .ok_or("etoile inconnue")
    }

    /// Place the ship next to the portal of the system just loaded.
    fn arrive_at_portal(&mut self) {
        if let Some(i) = portal_index(&self.system) {
            self.refresh_bodies();
            self.ship.pos = self.bodies[i].pos + Vec3::new(0.0, 0.0, -3000.0);
            self.ship.vel = self.bodies[i].vel;
            self.ship.ang = Vec3::ZERO;
        }
        self.target = None;
        self.autopilot = None;
        self.parked = None;
        self.ride = None;
        self.update_ref();
    }

    /// Cross the portal within reach. Returns a line for the log.
    pub fn use_portal(&mut self) -> Result<String, &'static str> {
        if self.docked.is_some() || self.jump.is_some() {
            return Err("impossible pour l'instant");
        }
        let i = self.portal_in_range().ok_or("aucun portail a portee")?;
        let fuel = self.ship.stats.fuel_cap * PORTAL_FUEL;
        if self.ship.fuel < fuel {
            return Err("carburant insuffisant pour franchir le portail");
        }
        self.ship.fuel -= fuel;
        match self.gate_of(i) {
            Gate::Wormhole(star) => {
                let mishap = hash_coord(star.seed, self.time as i64, 4, 4).is_multiple_of(10);
                self.jump = Some(JumpState {
                    target: star.id,
                    target_pos: star.pos,
                    target_seed: star.seed,
                    dist_ly: star.distance_to(self.here_pos),
                    charge_left: 0.0,
                    transit_left: 0.0,
                    total_transit: 0.0,
                    fuel_cost: 0.0,
                    in_transit: true,
                    mishap,
                });
                let name = star.header.name.clone();
                self.arrive();
                self.target = None;
                Ok(format!("TROU DE VER : VOUS EMERGEZ PRES DE {name}"))
            }
            Gate::Dimension(d) => {
                let seed = self.star_seed()?;
                self.stash_here();
                self.multiverse.dimension = d;
                self.multiverse.entered = self.time;
                self.load_system(generate_system_in(seed, d));
                self.compute_faction();
                self.spawn_traffic();
                self.arrive_at_portal();
                let name = self
                    .dimension_def()
                    .map_or("?", |x| x.name.as_str())
                    .to_string();
                Ok(format!("VOUS ENTREZ DANS : {}", name.to_uppercase()))
            }
            Gate::Home => {
                let seed = self.star_seed()?;
                let def = self.dimension_def().cloned();
                let outpost_kg = self.collect_outpost();
                let inside = (self.time - self.multiverse.entered).max(0.0);
                self.mined.clear();
                self.markets.clear();
                self.multiverse.dimension = 0;
                self.load_system(generate_system(seed));
                self.compute_faction();
                self.spawn_traffic();
                self.target = None;
                let mut msg = "RETOUR DANS NOTRE DIMENSION".to_string();
                if let Some(kg) = outpost_kg.filter(|k| *k > 0.5) {
                    msg = format!("{msg} : {kg:.0} KG DE TERRES RARES DE L'AVANT-POSTE");
                }
                if let Some(d) = def {
                    // The outside clock ran at another pace.
                    let shift = (inside * (d.time_ratio - 1.0)).max(-30.0 * DAY);
                    self.time += shift;
                    if shift.abs() > DAY {
                        msg = format!(
                            "{msg} : {:.0} JOURS {} DEHORS",
                            shift.abs() / DAY,
                            if shift > 0.0 { "DE PLUS" } else { "DE MOINS" }
                        );
                    }
                }
                Ok(msg)
            }
        }
    }

    /// Once a day: the crew feels the other dimension, paradoxes glitch.
    pub(crate) fn multiverse_tick(&mut self) {
        self.loop_tick();
        let day = (self.time / DAY).floor() as i64;
        let last = self.multiverse.last_day;
        self.multiverse.last_day = day;
        if last == 0 || day <= last {
            return;
        }
        let days = (day - last).min(3) as f32;
        self.outpost_tick(f64::from(days));
        if let Some(d) = self.dimension_def().cloned() {
            for p in &mut self.crew.members {
                p.instability = (p.instability + d.instability as f32 * days).min(1.0);
            }
        }
        if self.multiverse.paradox == 0 {
            return;
        }
        let mut rng = Rng::new(hash_coord(
            self.galaxy.seed ^ 0x9A2A,
            day,
            self.multiverse.paradox as i64,
            1,
        ));
        if rng.f64() < GLITCH_ODDS * f64::from(self.multiverse.paradox)
            && !self.crew.members.is_empty()
        {
            let k = (rng.f64() * self.crew.members.len() as f64) as usize;
            let p = &mut self.crew.members[k];
            p.instability = (p.instability + GLITCH_INSTABILITY).min(1.0);
            let name = p.name.clone();
            self.events.push(Event::MarketNews(format!(
                "Echo temporel : {name} se voit lui-meme passer dans le couloir."
            )));
        }
        let calm = self.time - self.multiverse.last_rewind > PARADOX_DECAY_DAYS * DAY;
        if calm && day % PARADOX_DECAY_DAYS as i64 == 0 {
            self.multiverse.paradox -= 1;
        }
    }

    // ---- time -------------------------------------------------------------------

    /// Fix the present in an anchor that can be rewound to.
    pub fn set_anchor(&mut self) -> Result<f64, &'static str> {
        if self.jump.as_ref().is_some_and(|j| j.in_transit) {
            return Err("impossible en plein saut");
        }
        if self.multiverse.anchors.len() >= MAX_ANCHORS {
            return Err("trois ancres au plus : levez-en une");
        }
        if self.credits < ANCHOR_FEE {
            return Err("fonds insuffisants");
        }
        self.credits -= ANCHOR_FEE;
        let anchors = std::mem::take(&mut self.multiverse.anchors);
        let saved = self.save_bytes();
        self.multiverse.anchors = anchors;
        let bytes = saved.map_err(|_| "ancre impossible")?;
        self.multiverse.anchors.push(Anchor {
            time: self.time,
            bytes,
        });
        Ok(ANCHOR_FEE)
    }

    pub fn drop_anchor(&mut self, i: usize) -> Result<(), &'static str> {
        if i >= self.multiverse.anchors.len() {
            return Err("ancre inconnue");
        }
        self.multiverse.anchors.remove(i);
        Ok(())
    }

    /// Rewind to anchor `i`: everything is as it was then, except that you remember the rest of
    /// the timeline you abandon, and carry one more paradox.
    pub fn rewind(&mut self, i: usize) -> Result<String, &'static str> {
        let anchor = self
            .multiverse
            .anchors
            .get(i)
            .ok_or("ancre inconnue")?
            .clone();
        if self.credits < REWIND_FEE {
            return Err("fonds insuffisants pour remonter le temps");
        }
        let now = self.time;
        let mut kept: Vec<Anchor> = self
            .multiverse
            .anchors
            .iter()
            .filter(|a| a.time <= anchor.time)
            .cloned()
            .collect();
        kept.sort_by(|a, b| a.time.total_cmp(&b.time));
        let paradox = self.multiverse.paradox + 1;
        let memory = self.multiverse.memory_until.max(now);
        let data = std::sync::Arc::clone(&self.data);
        let mut restored = Sim::load_bytes(&anchor.bytes, data).map_err(|_| "ancre corrompue")?;
        restored.multiverse.anchors = kept;
        restored.multiverse.paradox = paradox;
        restored.multiverse.memory_until = memory;
        restored.multiverse.last_rewind = restored.time;
        let gain = (self.credits - restored.credits).max(0.0);
        restored.multiverse.echo =
            (gain >= ECHO_MIN).then_some((gain, restored.time + ECHO_DELAY_DAYS * DAY));
        restored.credits = (restored.credits - REWIND_FEE).max(0.0);
        let back = (now - anchor.time) / DAY;
        *self = restored;
        self.events.push(Event::MarketNews(format!(
            "Temps remonte de {back:.1} jours. Vous vous souvenez de ce qui n'a pas eu lieu."
        )));
        Ok(format!("TEMPS REMONTE DE {back:.1} JOURS"))
    }

    /// What is remembered of the abandoned timeline (events still to come for this world).
    pub fn remembered(&self) -> Vec<crate::trading::Rumour> {
        let until = self
            .multiverse
            .memory_until
            .min(self.time + MEMORY_DAYS * DAY);
        if until <= self.time {
            return Vec::new();
        }
        crate::bourse::events_between(
            &self.data,
            self.galaxy.seed,
            self.here_pos,
            self.time,
            until,
        )
        .into_iter()
        .filter(|e| e.start > self.time)
        .map(|e| crate::trading::Rumour {
            def: e.def,
            start: e.start,
            confidence: 1.0,
            real: true,
            remembered: true,
        })
        .collect()
    }

    /// Sleep through `days` days docked: the world goes on, nothing aboard ages or eats.
    pub fn stasis(&mut self, days: f64) -> Result<f64, &'static str> {
        self.docked.ok_or("la stase se fait a quai")?;
        if self.multiverse.dimension > 0 {
            return Err("pas de stase hors de notre dimension");
        }
        if !(1.0..=STASIS_MAX).contains(&days) {
            return Err("de 1 a 365 jours");
        }
        let cost = STASIS_COST * days;
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        let crew = self.crew.clone();
        let cargo = self.ship.cargo.clone();
        let herd = self.herd.clone();
        let (fuel, battery) = (self.ship.fuel, self.ship.sys.battery_mj);
        let end = self.time + days * DAY;
        let mut guard = 0;
        while self.time < end && guard < 20_000 {
            let dt = (end - self.time).min(DAY / 4.0);
            self.step_dt(&ShipInput::default(), dt);
            guard += 1;
        }
        // The sleepers did not age, eat or starve; the paperwork did not wait.
        self.crew.members = crew.members;
        self.crew.air = crew.air;
        self.crew.unpaid = crew.unpaid;
        self.ship.cargo = cargo;
        self.herd = herd;
        self.ship.fuel = fuel;
        self.ship.sys.battery_mj = battery;
        Ok(cost)
    }
}

impl Sim {
    // ---- relay portals (a building of the colonies) ----------------------------

    /// Whether a colony of the system holds a finished relay.
    pub fn relay_here(&self) -> bool {
        self.colonies
            .iter()
            .any(|c| c.star == self.here && self.has_relay(c))
    }

    fn has_relay(&self, c: &crate::colony::Colony) -> bool {
        c.buildings
            .iter()
            .any(|b| b.built() && self.data.buildings[b.def].id == RELAY_ID)
    }

    /// The other systems with a relay: (colony id, name, distance in ly).
    pub fn relay_nodes(&self) -> Vec<(u32, String, f64)> {
        self.colonies
            .iter()
            .filter(|c| c.star != self.here && self.has_relay(c))
            .filter_map(|c| {
                let e = self.galaxy.star(c.star)?;
                Some((c.id, c.name.clone(), e.distance_to(self.here_pos)))
            })
            .collect()
    }

    /// Cross to the relay of colony `id`, for credits (no fuel, no charge time).
    pub fn relay_jump(&mut self, id: u32) -> Result<String, &'static str> {
        if self.multiverse.dimension > 0 {
            return Err("pas de relais hors de notre dimension");
        }
        if !self.relay_here() {
            return Err("aucun portail relais dans ce systeme");
        }
        if self.jump.is_some() {
            return Err("saut deja engage");
        }
        let c = self
            .colonies
            .iter()
            .find(|c| c.id == id && c.star != self.here && self.has_relay(c))
            .ok_or("relais inconnu")?;
        let star = self.galaxy.star(c.star).ok_or("etoile inconnue")?;
        let dist = star.distance_to(self.here_pos);
        let cost = dist * RELAY_CR_PER_LY;
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        self.docked = None;
        self.jump = Some(JumpState {
            target: star.id,
            target_pos: star.pos,
            target_seed: star.seed,
            dist_ly: dist,
            charge_left: 0.0,
            transit_left: 0.0,
            total_transit: 0.0,
            fuel_cost: 0.0,
            in_transit: true,
            mishap: false,
        });
        self.arrive();
        Ok(format!(
            "RELAIS : VOUS EMERGEZ PRES DE {}",
            star.header.name.to_uppercase()
        ))
    }

    // ---- loops of time ----------------------------------------------------------

    /// Whether this system is caught in a loop of time (some are; leaving always ends it).
    pub fn in_time_loop(&self) -> bool {
        if self.multiverse.broken.contains(&self.here) || self.multiverse.dimension > 0 {
            return false;
        }
        let h = hash_coord(self.system.seed ^ 0x100F, 7, 7, 7);
        ((h >> 11) as f64 / (1u64 << 53) as f64) < LOOP_CHANCE
    }

    /// Spend artifacts to break the loop of this system for good: it teaches something.
    pub fn break_loop(&mut self) -> Result<String, &'static str> {
        if !self.in_time_loop() {
            return Err("ce systeme n'est pas pris dans une boucle");
        }
        let art = self
            .data
            .resource_index("artifacts")
            .ok_or("artefacts inconnus")?;
        if self.ship.cargo.kg[art] + 1e-9 < LOOP_ARTIFACTS {
            return Err("il faut 5 artefacts pour briser la boucle");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, art, LOOP_ARTIFACTS);
        self.multiverse.broken.push(self.here);
        self.multiverse.loop_start = None;
        self.research_pool += 600.0;
        self.credits += 2000.0;
        Ok("BOUCLE BRISEE : LE TEMPS REPREND SON COURS".to_string())
    }

    pub(crate) fn loop_tick(&mut self) {
        if !self.in_time_loop() {
            self.multiverse.loop_start = None;
            return;
        }
        let start = *self.multiverse.loop_start.get_or_insert(self.time);
        if self.time - start >= LOOP_DAYS * DAY {
            // The days run again, but what happened in them is remembered.
            self.multiverse.memory_until = self.multiverse.memory_until.max(self.time);
            self.time = start;
            self.multiverse.loops_done += 1;
            self.events.push(Event::MarketNews(format!(
                "Boucle temporelle : le jour recommence ({} fois).",
                self.multiverse.loops_done
            )));
        }
    }

    // ---- echoes of an abandoned timeline ------------------------------------------

    /// An echo is waiting, if its time has come: the credits it carries.
    pub fn echo_ready(&self) -> Option<f64> {
        self.multiverse
            .echo
            .filter(|(_, at)| *at <= self.time)
            .map(|(c, _)| c)
    }

    /// Meet the echo: 0 help it (costs a little, calms a paradox, a third of its gains), 1 fight it
    /// (odds, most of its gains), 2 merge with it (all of it, two paradoxes and unstable crew).
    pub fn meet_echo(&mut self, how: u8) -> Result<String, &'static str> {
        let carried = self.echo_ready().ok_or("aucun echo ne vous attend")?;
        let msg = match how {
            0 => {
                if self.credits < ECHO_HELP_FEE {
                    return Err("fonds insuffisants");
                }
                self.credits += carried * 0.3 - ECHO_HELP_FEE;
                self.multiverse.paradox = self.multiverse.paradox.saturating_sub(1);
                "L'ECHO VOUS REMERCIE ET S'EFFACE : UN PARADOXE EN MOINS"
            }
            1 => {
                let mut rng = Rng::new(hash_coord(
                    self.galaxy.seed ^ 0xEC40,
                    self.time as i64,
                    3,
                    3,
                ));
                if rng.f64() < ECHO_FIGHT_ODDS {
                    self.credits += carried * 0.7;
                    "L'ECHO SE DISSOUT ; VOUS EN RECUPEREZ LES RESTES"
                } else {
                    self.credits -= self.credits * 0.05;
                    self.multiverse.paradox += 1;
                    "L'ECHO VOUS REPOUSSE : UN PARADOXE DE PLUS"
                }
            }
            2 => {
                self.credits += carried;
                self.multiverse.paradox += 2;
                for p in &mut self.crew.members {
                    p.instability = (p.instability + 0.2).min(1.0);
                }
                "VOUS FUSIONNEZ AVEC VOTRE ECHO : TOUT SE SOUVIENT, TOUT VIBRE"
            }
            _ => return Err("choix inconnu"),
        };
        self.multiverse.echo = None;
        Ok(msg.to_string())
    }

    // ---- exotic modules --------------------------------------------------------

    /// Indices of the modules only found beyond the portals.
    pub fn exotic_modules(&self) -> Vec<usize> {
        (0..self.data.modules.len())
            .filter(|&m| self.data.modules[m].id.starts_with("exo_"))
            .collect()
    }

    /// Exploring an anomaly in a dimension may turn up an exotic module (one chance in two).
    pub(crate) fn exotic_find(&mut self, body: usize) {
        let list = self.exotic_modules();
        let h = hash_coord(self.system.seed ^ 0xE70, body as i64, 5, 5);
        if list.is_empty() || (h >> 11) as f64 / (1u64 << 53) as f64 >= 0.5 {
            return;
        }
        let m = list[(h % list.len() as u64) as usize];
        self.module_stock[m] += 1;
        self.events.push(Event::MarketNews(format!(
            "Decouverte : {} (module exotique).",
            self.data.modules[m].name
        )));
    }

    // ---- outposts in dimensions ---------------------------------------------------

    /// Build an outpost beside the portal of the dimension you are in.
    pub fn build_outpost(&mut self) -> Result<String, &'static str> {
        let d = self.multiverse.dimension;
        if d == 0 {
            return Err("les avant-postes se batissent dans une dimension");
        }
        self.portal_in_range().ok_or("approchez-vous du portail")?;
        if self.multiverse.outposts.len() >= MAX_OUTPOSTS {
            return Err("cinq avant-postes au plus");
        }
        if self
            .multiverse
            .outposts
            .iter()
            .any(|o| o.0 == self.here && o.1 == d)
        {
            return Err("un avant-poste existe deja ici");
        }
        if OUTPOST_COST > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= OUTPOST_COST;
        self.multiverse.outposts.push((self.here, d, 0.0));
        Ok("AVANT-POSTE FONDE".to_string())
    }

    /// Outposts produce while you are away; their stock is loaded when you come back through
    /// their portal.
    pub(crate) fn outpost_tick(&mut self, days: f64) {
        let scales: Vec<f64> = self
            .multiverse
            .outposts
            .iter()
            .map(|o| {
                self.data
                    .dimensions
                    .get(o.1 as usize - 1)
                    .map_or(1.0, |d| d.ore_scale)
            })
            .collect();
        for (o, s) in self.multiverse.outposts.iter_mut().zip(scales) {
            o.2 = (o.2 + OUTPOST_KG_DAY * s * days).min(50_000.0);
        }
    }

    pub(crate) fn collect_outpost(&mut self) -> Option<f64> {
        let d = self.multiverse.dimension;
        let k = self
            .multiverse
            .outposts
            .iter()
            .position(|o| o.0 == self.here && o.1 == d)?;
        let kg = std::mem::take(&mut self.multiverse.outposts[k].2);
        let res = self.data.resource_index("rare_earths")?;
        let data = std::sync::Arc::clone(&self.data);
        let put = self.ship.cargo.add(&data, res, kg);
        self.multiverse.outposts[k].2 = kg - put;
        Some(put)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;
    use vector_world::{AnomalyKind, BodyKind};

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    /// A sim whose system holds a portal, with the ship next to it and a full tank.
    fn at_portal(wormhole: bool) -> Sim {
        let g = Galaxy::new(8);
        for e in g.nearby([0.0; 3], 200.0) {
            let sys = generate_system(e.seed);
            if portal_index(&sys).is_none() {
                continue;
            }
            let mut s = Sim::new_in_galaxy(Galaxy::new(8), &e, data());
            let i = portal_index(&s.system).unwrap();
            if matches!(s.gate_of(i), Gate::Wormhole(_)) != wormhole {
                continue;
            }
            s.refresh_bodies();
            s.ship.pos = s.bodies[i].pos + Vec3::new(0.0, 0.0, -5000.0);
            s.ship.fuel = s.ship.stats.fuel_cap;
            s.time = 50.0 * DAY;
            return s;
        }
        panic!("no such portal near the origin");
    }

    #[test]
    fn portals_are_rare_stable_and_leave_the_rest_of_the_system_unchanged() {
        let g = Galaxy::new(3);
        let stars = g.nearby([0.0; 3], 120.0);
        assert!(stars.len() > 60);
        let with: Vec<_> = stars
            .iter()
            .filter(|e| portal_index(&generate_system(e.seed)).is_some())
            .collect();
        let share = with.len() as f64 / stars.len() as f64;
        assert!(
            (share - vector_world::PORTAL_CHANCE).abs() < 0.08,
            "share {share}"
        );
        // The portal is always the last body: the other indices are those of the old worlds.
        for e in with.iter().take(5) {
            let a = generate_system(e.seed);
            assert_eq!(portal_index(&a), Some(a.bodies.len() - 1));
            assert_eq!(a.bodies.len(), generate_system(e.seed).bodies.len());
        }
        // Another dimension: another version of the star, always with a way back.
        let e = &stars[0];
        let d1 = generate_system_in(e.seed, 1);
        let d2 = generate_system_in(e.seed, 2);
        assert_ne!(d1.seed, d2.seed);
        assert_ne!(d1.seed, e.seed);
        assert!(portal_index(&d1).is_some() && portal_index(&d2).is_some());
        assert_eq!(generate_system_in(e.seed, 0).seed, e.seed);
        assert!(matches!(
            d1.bodies[portal_index(&d1).unwrap()].kind,
            BodyKind::Anomaly(AnomalyKind::Portal)
        ));
    }

    #[test]
    fn a_dimension_changes_the_rules_and_the_way_back_restores_the_system() {
        let mut s = at_portal(false);
        let home_seed = s.system.seed;
        let home_bodies = s.system.bodies.len();
        s.mined[1] = 123.0;
        assert_eq!(s.ore_scale(), 1.0);
        let fuel = s.ship.fuel;
        let msg = s.use_portal().expect("enters");
        assert!(msg.starts_with("VOUS ENTREZ"), "{msg}");
        let d = s.multiverse.dimension;
        assert!(d >= 1 && s.system.seed != home_seed);
        assert!(s.ship.fuel < fuel);
        assert!(s.portal_in_range().is_some(), "arrive next to the way back");
        let def = s.dimension_def().unwrap().clone();
        assert_eq!(s.ore_scale(), def.ore_scale);
        assert_eq!(s.hazard_scale(), def.hazard_scale);
        // Instability grows day after day for the crew (when the dimension is unstable).
        if def.instability > 0.0 && !s.crew.members.is_empty() {
            let before = s.crew.members[0].instability;
            s.multiverse.last_day = (s.time / DAY) as i64;
            s.time += 2.0 * DAY;
            s.multiverse_tick();
            assert!(s.crew.members[0].instability > before);
        }
        let back = s.use_portal().expect("returns");
        assert!(back.starts_with("RETOUR"), "{back}");
        assert_eq!(s.multiverse.dimension, 0);
        assert_eq!(s.system.seed, home_seed);
        assert_eq!(s.system.bodies.len(), home_bodies);
        assert_eq!(s.mined[1], 123.0, "our system kept its changes");
    }

    #[test]
    fn a_wormhole_throws_the_ship_far_away_and_a_far_portal_is_out_of_reach() {
        let mut s = at_portal(true);
        let from = s.here_pos;
        let fuel = s.ship.fuel;
        s.ship.pos = s.ship.pos + Vec3::new(0.0, 0.0, -1.0e9);
        assert!(s.use_portal().is_err(), "too far from the portal");
        s.ship.pos = s.bodies[portal_index(&s.system).unwrap()].pos + Vec3::new(0.0, 0.0, -5000.0);
        let msg = s.use_portal().expect("crosses");
        assert!(msg.starts_with("TROU DE VER"), "{msg}");
        let d = ((s.here_pos[0] - from[0]).powi(2)
            + (s.here_pos[1] - from[1]).powi(2)
            + (s.here_pos[2] - from[2]).powi(2))
        .sqrt();
        assert!(d > 100.0, "{d} ly away");
        assert!(s.ship.fuel < fuel);
        assert_eq!(s.multiverse.dimension, 0);
    }

    #[test]
    fn dimension_time_ratio_shifts_the_outside_clock_on_return() {
        let mut s = at_portal(false);
        s.use_portal().unwrap();
        let ratio = s.dimension_def().unwrap().time_ratio;
        s.time += 3.0 * DAY;
        let before = s.time;
        s.use_portal().unwrap();
        let expect = (3.0 * DAY * (ratio - 1.0)).max(-30.0 * DAY);
        assert!(
            (s.time - before - expect).abs() < 1.0,
            "{} vs {expect}",
            s.time - before
        );
    }

    #[test]
    fn rewinding_restores_the_past_but_keeps_the_memory_and_a_paradox() {
        let mut s = new_sim(5, data());
        s.time = 40.0 * DAY;
        s.credits = 50_000.0;
        let c0 = s.credits;
        s.set_anchor().expect("anchor");
        assert_eq!(s.credits, c0 - ANCHOR_FEE);
        let at_anchor = s.credits;
        let t0 = s.time;
        // The timeline goes on: money is made and time passes.
        s.credits += 10_000.0;
        s.time += 30.0 * DAY;
        let msg = s.rewind(0).expect("rewinds");
        assert!(msg.starts_with("TEMPS REMONTE"), "{msg}");
        assert!((s.time - t0).abs() < 1e-6, "back to the anchor");
        assert_eq!(
            s.credits,
            at_anchor - REWIND_FEE,
            "the gains of the lost timeline are gone"
        );
        assert_eq!(s.multiverse.paradox, 1);
        assert_eq!(s.multiverse.anchors.len(), 1, "the anchor stays");
        assert!(s.multiverse.memory_until >= t0 + 30.0 * DAY - 1.0);
        // The abandoned timeline's events are remembered with certainty, and match the real ones.
        let mem = s.remembered();
        let real = crate::bourse::events_between(
            &s.data,
            s.galaxy.seed,
            s.here_pos,
            s.time,
            t0 + 30.0 * DAY,
        );
        let future: Vec<_> = real.iter().filter(|e| e.start > s.time).collect();
        assert_eq!(mem.len(), future.len());
        assert!(mem.iter().all(|m| m.confidence == 1.0 && m.remembered));
        // The memory fades once the present catches up with it.
        s.time = t0 + 100.0 * DAY;
        assert!(s.remembered().is_empty());
    }

    #[test]
    fn anchors_are_limited_cost_money_and_rewinding_needs_funds() {
        let mut s = new_sim(5, data());
        s.credits = 100_000.0;
        for _ in 0..MAX_ANCHORS {
            s.set_anchor().unwrap();
        }
        assert!(s.set_anchor().is_err(), "three at most");
        s.drop_anchor(0).unwrap();
        assert!(s.drop_anchor(5).is_err());
        s.set_anchor().unwrap();
        s.credits = 10.0;
        assert!(s.rewind(0).is_err(), "no money to rewind");
        assert!(s.rewind(9).is_err(), "unknown anchor");
        s.credits = 5.0;
        assert!(s.set_anchor().is_err());
        // Anchors do not nest: a saved game does not grow with every anchor.
        let small = s.save_bytes().unwrap().len();
        assert!(small < 12 * 1024 * 1024, "{small}");
    }

    #[test]
    fn paradoxes_destabilise_the_crew_and_fade_with_time() {
        let mut s = new_sim(5, data());
        assert!(!s.crew.members.is_empty());
        s.multiverse.paradox = 4;
        s.multiverse.last_rewind = 0.0;
        s.multiverse.last_day = 1;
        let mut glitches = 0;
        for d in 2..300 {
            s.time = f64::from(d) * DAY;
            s.events.clear();
            s.multiverse_tick();
            if s.events
                .iter()
                .any(|e| matches!(e, Event::MarketNews(t) if t.starts_with("Echo")))
            {
                glitches += 1;
            }
        }
        assert!(glitches >= 2, "{glitches} glitches");
        assert!(s.crew.members.iter().any(|p| p.instability > 0.1));
        assert!(s.multiverse.paradox < 4, "calm washes paradoxes away");
    }

    #[test]
    fn stasis_skips_time_without_ageing_the_crew_or_eating() {
        let mut s = new_sim(5, data());
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        let t0 = s.time;
        let cargo = s.ship.cargo.kg.clone();
        let crew: Vec<_> = s
            .crew
            .members
            .iter()
            .map(|p| (p.id, (p.hunger.to_bits(), p.age.to_bits())))
            .collect();
        assert!(s.stasis(0.5).is_err() && s.stasis(500.0).is_err());
        let cost = s.stasis(60.0).expect("sleeps");
        assert_eq!(cost, 60.0 * STASIS_COST);
        assert!(
            (s.time - t0 - 60.0 * DAY).abs() < 10.0,
            "{}",
            (s.time - t0) / DAY
        );
        assert_eq!(s.ship.cargo.kg, cargo, "nothing was eaten");
        let after: Vec<_> = s
            .crew
            .members
            .iter()
            .map(|p| (p.id, (p.hunger.to_bits(), p.age.to_bits())))
            .collect();
        assert_eq!(crew, after);
        s.undock();
        assert!(s.stasis(10.0).is_err(), "only docked");
    }

    #[test]
    fn exotic_modules_exist_and_are_found_only_beyond_the_portals() {
        let mut s = at_portal(false);
        let list = s.exotic_modules();
        assert!(list.len() >= 5, "{} exotic modules", list.len());
        // Our own dimension never gives them.
        for b in 0..60 {
            s.exotic_find(b);
        }
        // exotic_find itself is unconditional; the condition is in explore_anomaly.
        let found_here: u32 = list.iter().map(|m| s.module_stock[*m]).sum();
        assert!(
            found_here > 5 && found_here < 45,
            "{found_here} of 60 tries"
        );
        let mut s = at_portal(false);
        s.use_portal().unwrap();
        let before: u32 = list.iter().map(|m| s.module_stock[*m]).sum();
        assert_eq!(before, 0);
        let anomaly = s
            .bodies
            .iter()
            .position(|b| matches!(b.kind, vector_world::BodyKind::Anomaly(_)))
            .unwrap();
        s.explored[anomaly] = false;
        for k in 0..40 {
            s.system.seed ^= k;
            s.exotic_find(anomaly + k as usize);
        }
        assert!(list.iter().map(|m| s.module_stock[*m]).sum::<u32>() > 5);
    }

    #[test]
    fn time_loops_repeat_the_days_until_broken_and_ending_them_is_free_by_leaving() {
        let g = Galaxy::new(8);
        let mut found = None;
        'seek: for e in g.nearby([0.0; 3], 400.0) {
            let s = Sim::new_in_galaxy(Galaxy::new(8), &e, data());
            if s.in_time_loop() {
                found = Some(s);
                break 'seek;
            }
        }
        let mut s = found.expect("a looping system");
        s.time = 10.0 * DAY;
        s.multiverse.last_day = 10;
        s.loop_tick();
        let start = s.time;
        assert_eq!(s.multiverse.loop_start, Some(start));
        s.time = start + 2.0 * DAY;
        s.loop_tick();
        assert!((s.time - (start + 2.0 * DAY)).abs() < 1e-9, "not yet");
        s.time = start + LOOP_DAYS * DAY + 5.0;
        let reached = s.time;
        s.loop_tick();
        assert_eq!(s.time, start, "the days run again");
        assert_eq!(s.multiverse.loops_done, 1);
        assert!(s.multiverse.memory_until >= reached, "and are remembered");
        // Breaking it needs artifacts, then the system is free for good.
        assert!(s.break_loop().is_err());
        let art = s.data.resource_index("artifacts").unwrap();
        s.ship.cargo.kg[art] = 10.0;
        let research = s.research_pool;
        s.break_loop().expect("broken");
        assert!(s.research_pool > research);
        assert!(!s.in_time_loop());
        s.time = start + 10.0 * DAY;
        s.loop_tick();
        assert_eq!(s.time, start + 10.0 * DAY);
        assert!(s.break_loop().is_err(), "nothing left to break");
    }

    #[test]
    fn leaving_a_looping_system_ends_the_loop() {
        let g = Galaxy::new(8);
        let mut s = g
            .nearby([0.0; 3], 400.0)
            .into_iter()
            .map(|e| Sim::new_in_galaxy(Galaxy::new(8), &e, data()))
            .find(Sim::in_time_loop)
            .expect("a looping system");
        s.multiverse.loop_start = Some(5.0);
        // A system without a loop clears the mark.
        let other = g
            .nearby([0.0; 3], 400.0)
            .into_iter()
            .map(|e| Sim::new_in_galaxy(Galaxy::new(8), &e, data()))
            .find(|x| !x.in_time_loop())
            .unwrap();
        s.system = other.system.clone();
        s.loop_tick();
        assert_eq!(s.multiverse.loop_start, None);
    }

    #[test]
    fn a_rewind_that_loses_gains_leaves_an_echo_with_three_ways_to_meet_it() {
        let mut s = new_sim(5, data());
        s.credits = 100_000.0;
        s.set_anchor().unwrap();
        s.credits += 20_000.0;
        s.rewind(0).unwrap();
        let (carried, at) = s.multiverse.echo.expect("an echo waits");
        assert!((carried - 20_000.0).abs() < 1.0, "{carried}");
        assert!(
            s.echo_ready().is_none() && s.meet_echo(0).is_err(),
            "not yet"
        );
        s.time = at + 1.0;
        assert_eq!(s.echo_ready(), Some(carried));
        // Help: costs a bit, a third of its gains, one paradox less.
        let mut h = clone_state(&s);
        let c = h.credits;
        h.meet_echo(0).unwrap();
        assert!((h.credits - (c + carried * 0.3 - ECHO_HELP_FEE)).abs() < 1e-6);
        assert_eq!(h.multiverse.paradox, 0);
        assert!(h.multiverse.echo.is_none());
        // Merge: everything, two paradoxes, unstable crew.
        let mut m = clone_state(&s);
        let c = m.credits;
        m.meet_echo(2).unwrap();
        assert!((m.credits - c - carried).abs() < 1e-6);
        assert_eq!(m.multiverse.paradox, 3);
        assert!(m.crew.members.iter().all(|p| p.instability >= 0.2));
        // Fight: odds, either way the echo is gone.
        let mut wins = 0;
        for k in 0..60 {
            let mut f = clone_state(&s);
            f.time += f64::from(k) * 7.0;
            let c = f.credits;
            f.meet_echo(1).unwrap();
            if f.credits > c {
                wins += 1;
            }
            assert!(f.multiverse.echo.is_none());
        }
        assert!((20..=50).contains(&wins), "{wins} wins of 60");
        assert!(s.meet_echo(7).is_err());
        // Small gains leave no echo.
        let mut t = new_sim(5, data());
        t.credits = 10_000.0;
        t.set_anchor().unwrap();
        t.credits += 100.0;
        t.rewind(0).unwrap();
        assert!(t.multiverse.echo.is_none());
    }

    /// A copy of a sim through a saved game.
    fn clone_state(s: &Sim) -> Sim {
        let bytes = s.save_bytes().unwrap();
        Sim::load_bytes(&bytes, Arc::clone(&s.data)).unwrap()
    }

    #[test]
    fn outposts_produce_while_away_and_are_collected_on_the_way_home() {
        let mut s = at_portal(false);
        s.credits = 200_000.0;
        assert!(s.build_outpost().is_err(), "only in a dimension");
        s.use_portal().unwrap();
        s.build_outpost().expect("built");
        assert!(s.build_outpost().is_err(), "one per place");
        assert_eq!(s.credits, 200_000.0 - OUTPOST_COST);
        let ore = s.ore_scale();
        s.multiverse.last_day = (s.time / DAY) as i64;
        s.time += 10.0 * DAY;
        s.multiverse_tick();
        let waiting = s.multiverse.outposts[0].2;
        assert!(
            waiting > 0.0 && waiting <= OUTPOST_KG_DAY * ore * 4.0,
            "{waiting}"
        );
        let re = s.data.resource_index("rare_earths").unwrap();
        let before = s.ship.cargo.kg[re];
        let msg = s.use_portal().unwrap();
        assert!(msg.contains("AVANT-POSTE"), "{msg}");
        assert!((s.ship.cargo.kg[re] - before - waiting).abs() < 1e-6);
        assert_eq!(s.multiverse.outposts[0].2, 0.0);
    }

    #[test]
    fn relay_portals_link_colonies_for_credits() {
        let (mut s, ci) = crate::colony::tests::founded();
        let relay = s
            .data
            .buildings
            .iter()
            .position(|b| b.id == RELAY_ID)
            .expect("the relay is a building");
        let inst = crate::colony::BuildingInst {
            def: relay,
            progress: 1.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
            ops: Default::default(),
        };
        s.colonies[ci].buildings.push(inst.clone());
        assert!(s.relay_here());
        assert!(s.relay_nodes().is_empty());
        assert!(s.relay_jump(99).is_err());
        // A second colony with a relay, in another system.
        let far = Galaxy::new(s.galaxy.seed)
            .nearby(s.here_pos, 60.0)
            .into_iter()
            .find(|e| e.id != s.here)
            .expect("another star");
        let mut other = s.colonies[ci].clone();
        other.buildings.clear();
        other.id = 77;
        other.star = far.id;
        other.name = "Lointaine".into();
        s.colonies.push(other);
        assert!(s.relay_nodes().is_empty(), "an unfinished relay is no node");
        s.colonies.last_mut().unwrap().buildings.push(inst);
        let nodes = s.relay_nodes();
        assert_eq!(nodes.len(), 1);
        let cost = nodes[0].2 * RELAY_CR_PER_LY;
        s.credits = cost - 1.0;
        assert!(s.relay_jump(77).is_err(), "no money");
        s.credits = cost + 10.0;
        let msg = s.relay_jump(77).expect("crosses");
        assert!(msg.starts_with("RELAIS"), "{msg}");
        assert_eq!(s.here, far.id);
        assert!((s.credits - 10.0).abs() < 1e-6);
        assert!(s.docked.is_none());
    }
}
