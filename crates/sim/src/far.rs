//! Long voyages: drive classes, route planning over hundreds of jumps, automatic execution
//! (refuelling halts, cryosleep, encounters, repairs) and the crossing between galaxies.
//!
//! One command, `voyage_plan` then `voyage_begin`, replaces the micro-management of every jump:
//! the voyage state lives in `Sim::voyage` (saved), is stepped by `voyage_tick` and
//! `voyage_arrived` (called when a jump ends), and stops by itself on danger.
use crate::relations::Relations;
use crate::voyage::SystemDelta;
use crate::Sim;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vector_core::{hash_coord, Rng};
use vector_world::galaxy::{Galaxy, StarEntry, StarRef, SECTOR_LY};
use vector_world::universe::Universe;
use vector_world::StarClass;

/// Time acceleration the voyage asks for while it runs without trouble.
pub const VOYAGE_WARP: f64 = 100_000.0;
/// A full tank is scooped from a star in this many seconds.
pub const SCOOP_FULL_S: f64 = 86_400.0;
/// Voyages longer than this put the crew to sleep (s).
pub const CRYO_THRESHOLD_S: f64 = 30.0 * 86_400.0;
const LOG_MAX: usize = 300;

/// What the best working FTL drive of the ship does (the classic drive when none is fitted).
#[derive(Clone, Copy, Debug)]
pub struct DriveProfile {
    pub range: f64,
    pub hours_per_ly: f64,
    pub fuel_mult: f64,
    pub risk: f64,
    pub charge: f64,
    pub antimatter_per_ly: f64,
    pub class: &'static str,
}

impl DriveProfile {
    pub const CLASSIC: DriveProfile = DriveProfile {
        range: 0.0,
        hours_per_ly: crate::voyage::HOURS_PER_LY,
        fuel_mult: 1.0,
        risk: 0.01,
        charge: crate::voyage::CHARGE_TIME,
        antimatter_per_ly: 0.0,
        class: "courte",
    };

    /// Profile of one module definition.
    pub fn of(m: &vector_data::Module) -> DriveProfile {
        let or = |v: f64, d: f64| if v > 0.0 { v } else { d };
        DriveProfile {
            range: m.jump_range,
            hours_per_ly: or(m.jump_hours_per_ly, Self::CLASSIC.hours_per_ly),
            fuel_mult: or(m.jump_fuel_mult, 1.0),
            risk: or(m.jump_risk, Self::CLASSIC.risk),
            charge: or(m.jump_charge, Self::CLASSIC.charge),
            antimatter_per_ly: m.jump_antimatter,
            class: match m.jump_class.as_str() {
                "long" => "longue",
                "galactic" => "galactique",
                "intergalactic" => "intergalactique",
                _ => "courte",
            },
        }
    }
}

/// What a galaxy we left remembers while we are away.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GalaxyStash {
    pub visited: BTreeMap<StarRef, [f64; 3]>,
    pub deltas: BTreeMap<StarRef, SystemDelta>,
    pub relations: Relations,
    pub here: Option<StarRef>,
    pub here_pos: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dest {
    Star(StarRef),
    Galaxy(i64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Leg {
    pub star: StarRef,
    pub pos: [f64; 3],
    pub seed: u64,
    pub name: String,
    pub dist: f64,
    /// Set for an intergalactic jump: the galaxy this leg arrives in.
    pub galaxy: Option<i64>,
    /// The ship scoops fuel from this star on arrival.
    pub scoop: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VoyagePlan {
    pub dest_name: String,
    pub dest: Option<Dest>,
    pub legs: Vec<Leg>,
    pub total_ly: f64,
    pub fuel: f64,
    pub antimatter: f64,
    /// Seconds of travel (charge and transit) and of halts (refuelling).
    pub travel_s: f64,
    pub halt_s: f64,
    /// What the propellant and antimatter cost at base prices.
    pub credits: f64,
    pub max_hop: f64,
    pub refuels: usize,
    pub intergalactic: bool,
    pub ok: bool,
    pub why: Option<String>,
}

impl VoyagePlan {
    pub fn total_s(&self) -> f64 {
        self.travel_s + self.halt_s
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Voyage {
    pub plan: VoyagePlan,
    pub next: usize,
    pub paused: Option<String>,
    pub done: bool,
    pub refueling: bool,
    pub repairing: bool,
    pub skip_alert: bool,
    /// Leg being started and how many times its jump began (a drive that keeps aborting stops
    /// the voyage instead of looping).
    pub tries_leg: usize,
    pub tries: u8,
    /// Critical alerts already known: only a new one stops the voyage.
    pub alerts_seen: Vec<String>,
    pub slept: bool,
    pub started: f64,
    pub fuel_start: f64,
    pub log: Vec<(f64, String)>,
}

impl Voyage {
    pub fn running(&self) -> bool {
        !self.done && self.paused.is_none()
    }
    fn note(&mut self, t: f64, s: String) {
        if self.log.len() >= LOG_MAX {
            self.log.remove(0);
        }
        self.log.push((t, s));
    }
}

fn scoopable(c: StarClass) -> bool {
    matches!(
        c,
        StarClass::A | StarClass::F | StarClass::G | StarClass::K | StarClass::M
    )
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// How long a duration reads: "3 J", "2.4 A".
pub fn span_text(s: f64) -> String {
    let d = s / 86_400.0;
    if d < 1.0 {
        format!("{:.0} H", s / 3600.0)
    } else if d < 365.0 {
        format!("{d:.0} J")
    } else {
        format!("{:.1} A", d / 365.25)
    }
}

impl Sim {
    // ---- drives ------------------------------------------------------------------------------

    pub fn drive_profile(&self) -> DriveProfile {
        let mut best: Option<DriveProfile> = None;
        for (i, m) in self.ship.design.modules(&self.data) {
            if m.jump_range > 0.0
                && self.ship.sys.health.get(i).copied().unwrap_or(0.0) > 0.0
                && best.is_none_or(|b| m.jump_range > b.range)
            {
                best = Some(DriveProfile::of(m));
            }
        }
        best.unwrap_or(DriveProfile::CLASSIC)
    }

    /// Mishap multiplier from the best awake pilot (0.6 .. 1.25); the bare AI is neutral.
    pub fn pilot_risk_factor(&self) -> f64 {
        let Some(sk) = self.data.skill_index("pilotage") else {
            return 1.0;
        };
        let best = self
            .crew
            .members
            .iter()
            .filter(|p| p.alive && !p.body.cryo)
            .filter_map(|p| p.skills.get(sk).copied())
            .fold(None, |a: Option<f32>, v| Some(a.map_or(v, |x| x.max(v))));
        best.map_or(1.0, |s| (1.25 - 0.065 * f64::from(s)).clamp(0.6, 1.25))
    }

    pub fn universe(&self) -> Universe {
        Universe::new(self.universe_seed)
    }

    // ---- galaxies ----------------------------------------------------------------------------

    /// The star a ship arrives at when it enters galaxy `g`: where we left it if we were there
    /// before, otherwise a quiet star picked from the galaxy's seed.
    pub fn galaxy_entry_star(&self, g: i64) -> Option<StarEntry> {
        let gal = Galaxy::new(self.universe().galaxy_seed(g));
        if let Some(h) = self.galaxy_stash.get(&g).and_then(|s| s.here) {
            return gal.star(h);
        }
        let mut r = Rng::new(hash_coord(gal.seed, 0xE47, 7, 7));
        for k in 0..8 {
            let spread = 1500.0 + 500.0 * f64::from(k);
            let p = [
                r.range(-spread, spread),
                r.range(-150.0, 150.0),
                r.range(-spread, spread),
            ];
            let radius = if k < 3 { 40.0 } else { 90.0 };
            let mut best: Option<StarEntry> = None;
            let c = Galaxy::sector_of(p);
            let reach = (radius / SECTOR_LY).ceil() as i64;
            for dx in -reach..=reach {
                for dy in -reach..=reach {
                    for dz in -reach..=reach {
                        for s in gal.sector_cached((c.0 + dx, c.1 + dy, c.2 + dz)).iter() {
                            let ok = matches!(
                                s.class(),
                                StarClass::F | StarClass::G | StarClass::K | StarClass::M
                            );
                            if ok
                                && s.distance_to(p) <= radius
                                && best.as_ref().is_none_or(|b| {
                                    s.distance_to(p) < b.distance_to(p)
                                        || (s.distance_to(p) == b.distance_to(p) && s.id < b.id)
                                })
                            {
                                best = Some(s.clone());
                            }
                        }
                    }
                }
            }
            if best.is_some() {
                return best;
            }
        }
        gal.nearby([0.0; 3], 120.0).into_iter().next()
    }

    /// Leave the current galaxy for `g`: stash what this one remembers, load what `g` remembered
    /// (nothing the first time: the ship is a stranger there, with no standing).
    pub(crate) fn switch_galaxy(&mut self, g: i64) {
        if g == self.galaxy_id {
            return;
        }
        let old = self.galaxy_id;
        let stash = GalaxyStash {
            visited: std::mem::take(&mut self.visited),
            deltas: std::mem::take(&mut self.deltas),
            relations: std::mem::take(&mut self.relations),
            here: Some(self.here),
            here_pos: self.here_pos,
        };
        self.galaxy_stash.insert(old, stash);
        self.galaxy_id = g;
        self.galaxy = Galaxy::new(self.universe().galaxy_seed(g));
        if let Some(s) = self.galaxy_stash.remove(&g) {
            self.visited = s.visited;
            self.deltas = s.deltas;
            self.relations = s.relations;
        }
    }

    pub fn galaxy_name(&self) -> String {
        self.galaxy.name()
    }

    // ---- planning ----------------------------------------------------------------------------

    /// Plan the whole trip to `dest` with what the ship has now.
    pub fn voyage_plan(&self, dest: Dest) -> Result<VoyagePlan, String> {
        let prof = self.drive_profile();
        if prof.range <= 0.0 {
            return Err("AUCUN PROPULSEUR FTL INSTALLE".into());
        }
        let mut plan = VoyagePlan {
            dest: Some(dest),
            ..Default::default()
        };
        match dest {
            Dest::Galaxy(g) => self.plan_galaxies(g, &prof, &mut plan)?,
            Dest::Star(id) => self.plan_stars(id, &prof, &mut plan)?,
        }
        self.price_plan(&prof, &mut plan);
        Ok(plan)
    }

    fn price_plan(&self, prof: &DriveProfile, plan: &mut VoyagePlan) {
        let cap = self.ship.stats.fuel_cap.max(1.0);
        let mut fuel = self.ship.fuel;
        let mut prev = self.here_pos;
        let mut scooped = 0.0;
        let mut first = true;
        for l in &mut plan.legs {
            let d = if first || l.galaxy.is_none() {
                dist3(prev, l.pos)
            } else {
                l.dist
            };
            if let Some(_g) = l.galaxy {
                // The distance between galaxies was fixed by the planner.
            } else {
                l.dist = d;
            }
            first = false;
            let c = self.jump_fuel_cost(l.dist);
            plan.fuel += c;
            plan.antimatter += prof.antimatter_per_ly * l.dist;
            plan.total_ly += l.dist;
            plan.travel_s += l.dist * prof.hours_per_ly * 3600.0 + prof.charge;
            plan.max_hop = plan.max_hop.max(l.dist);
            fuel -= c;
            if l.galaxy.is_none() && l.scoop && fuel < 0.6 * cap {
                plan.halt_s += (cap - fuel).max(0.0) / cap * SCOOP_FULL_S;
                plan.refuels += 1;
                scooped += (cap - fuel).max(0.0);
                fuel = cap;
            }
            prev = l.pos;
        }
        let fuel_value = self
            .data
            .resource_index("fuel")
            .map_or(1.0, |i| self.data.resources[i].value);
        let am_value = self
            .data
            .resource_index("antimatter")
            .map_or(0.0, |i| self.data.resources[i].value);
        // Scooped propellant is free: only what the tank itself gives up (and the antimatter) is
        // counted, at base value.
        plan.credits = (plan.fuel - scooped).max(0.0) * fuel_value + plan.antimatter * am_value;
        plan.intergalactic = plan.legs.iter().any(|l| l.galaxy.is_some());
        let first_cost = plan
            .legs
            .first()
            .map_or(0.0, |l| self.jump_fuel_cost(l.dist));
        plan.ok = true;
        if first_cost > self.ship.fuel {
            plan.ok = false;
            plan.why = Some(format!(
                "CARBURANT INSUFFISANT POUR LE PREMIER SAUT : {first_cost:.0} KG NECESSAIRES, {:.0} A BORD",
                self.ship.fuel
            ));
        } else if plan.legs.iter().any(|l| l.galaxy.is_some())
            && plan.antimatter > self.antimatter_stored()
        {
            plan.ok = false;
            plan.why = Some(format!(
                "ANTIMATIERE INSUFFISANTE : {:.1} KG NECESSAIRES, {:.1} A BORD",
                plan.antimatter,
                self.antimatter_stored()
            ));
        } else if plan.fuel > 0.0 && plan.refuels == 0 && plan.fuel > self.ship.fuel {
            plan.ok = false;
            plan.why = Some(format!(
                "CARBURANT INSUFFISANT : {:.0} KG NECESSAIRES, {:.0} A BORD, AUCUN RAVITAILLEMENT SUR LA ROUTE",
                plan.fuel, self.ship.fuel
            ));
        }
    }

    fn plan_galaxies(
        &self,
        g: i64,
        prof: &DriveProfile,
        plan: &mut VoyagePlan,
    ) -> Result<(), String> {
        if g == self.galaxy_id {
            return Err("DEJA DANS CETTE GALAXIE".into());
        }
        if prof.class != "intergalactique" {
            return Err("IL FAUT UN PROPULSEUR INTERGALACTIQUE".into());
        }
        let uni = self.universe();
        let goal = uni.galaxy(g);
        plan.dest_name = goal.name();
        let mut cur = uni.galaxy(self.galaxy_id);
        let range = prof.range * 0.98;
        for _ in 0..64 {
            let dg = cur.distance_to(goal.pos);
            let next = if dg <= range {
                goal
            } else {
                // The neighbour in reach that gets closest to the goal.
                uni.galaxies_near(cur.pos, range)
                    .into_iter()
                    .filter(|n| n.id != cur.id && n.distance_to(goal.pos) < dg - range * 0.2)
                    .min_by(|a, b| {
                        a.distance_to(goal.pos)
                            .total_cmp(&b.distance_to(goal.pos))
                            .then(a.id.cmp(&b.id))
                    })
                    .ok_or("AUCUNE GALAXIE A PORTEE SUR LA ROUTE")?
            };
            let entry = self
                .galaxy_entry_star(next.id)
                .ok_or("AUCUNE ETOILE D'ARRIVEE DANS CETTE GALAXIE")?;
            plan.legs.push(Leg {
                star: entry.id,
                pos: entry.pos,
                seed: entry.seed,
                name: format!("{} / {}", next.name(), entry.header.name),
                dist: cur.distance_to(next.pos),
                galaxy: Some(next.id),
                scoop: false,
            });
            if next.id == goal.id {
                return Ok(());
            }
            cur = next;
        }
        Err("ROUTE INTERGALACTIQUE TROP LONGUE".into())
    }

    fn plan_stars(
        &self,
        id: StarRef,
        prof: &DriveProfile,
        plan: &mut VoyagePlan,
    ) -> Result<(), String> {
        let goal = self.galaxy.star(id).ok_or("ETOILE INCONNUE")?;
        if id == self.here {
            return Err("DEJA DANS CE SYSTEME".into());
        }
        plan.dest_name = goal.header.name.clone();
        let range = prof.range * 0.98;
        let cap = self.ship.stats.fuel_cap.max(1.0);
        let nfac = self.data.factions.len();
        let mut fuel = self.ship.fuel;
        let mut cur_pos = self.here_pos;
        let mut cur_id = self.here;
        let sr0 = (range * 0.08).clamp(4.0, 20.0);
        // Fuel is linear in the distance: one figure per light year serves the whole search.
        let kfuel = self.jump_fuel_cost(1.0);
        let danger: Vec<f64> = StarClass::ALL
            .iter()
            .map(|c| crate::nav_plan::star_danger(*c) * range * 0.05)
            .collect();
        for _ in 0..6000 {
            let d_goal = dist3(cur_pos, goal.pos);
            if d_goal <= range {
                let d = d_goal;
                plan.legs.push(Leg {
                    star: goal.id,
                    pos: goal.pos,
                    seed: goal.seed,
                    name: goal.header.name.clone(),
                    dist: d,
                    galaxy: None,
                    scoop: scoopable(goal.class()),
                });
                return Ok(());
            }
            let dir = [
                (goal.pos[0] - cur_pos[0]) / d_goal,
                (goal.pos[1] - cur_pos[1]) / d_goal,
                (goal.pos[2] - cur_pos[2]) / d_goal,
            ];
            let aim = [
                cur_pos[0] + dir[0] * range * 0.9,
                cur_pos[1] + dir[1] * range * 0.9,
                cur_pos[2] + dir[2] * range * 0.9,
            ];
            let hop_full = kfuel * range;
            let mut chosen: Option<StarEntry> = None;
            for mult in [1.0, 2.0, 3.0] {
                let sr = (sr0 * mult).min(if range < 60.0 { range } else { 90.0 });
                let mut best: Option<(f64, StarEntry)> = None;
                let c = Galaxy::sector_of(aim);
                let reach = (sr / SECTOR_LY).ceil() as i64;
                // Distance from the aim point to the box of a sector (skip the sectors out of reach).
                let gap = |c: i64, a: f64| {
                    let (lo, hi) = (c as f64 * SECTOR_LY, (c + 1) as f64 * SECTOR_LY);
                    (lo - a).max(a - hi).max(0.0)
                };
                for dx in -reach..=reach {
                    for dy in -reach..=reach {
                        for dz in -reach..=reach {
                            let g = (gap(c.0 + dx, aim[0]).powi(2)
                                + gap(c.1 + dy, aim[1]).powi(2)
                                + gap(c.2 + dz, aim[2]).powi(2))
                            .sqrt();
                            if g > sr {
                                continue;
                            }
                            for s in self
                                .galaxy
                                .sector_cached((c.0 + dx, c.1 + dy, c.2 + dz))
                                .iter()
                            {
                                if s.id == cur_id || s.distance_to(aim) > sr {
                                    continue;
                                }
                                let d = s.distance_to(cur_pos);
                                let dg = s.distance_to(goal.pos);
                                if d > range || dg > d_goal - range * 0.2 && s.id != goal.id {
                                    continue;
                                }
                                // Every term below only adds to `dg`: stop early when it cannot win.
                                if best.as_ref().is_some_and(|(b, _)| dg > *b) {
                                    continue;
                                }
                                let class = s.class();
                                let mut score = dg + danger[class as usize];
                                if fuel - kfuel * d < 1.2 * hop_full && !scoopable(class) {
                                    score += range;
                                }
                                if best.as_ref().is_some_and(|(b, _)| score > *b) {
                                    continue;
                                }
                                if nfac > 0
                                    && self.galaxy.territory(s.pos, nfac).is_some_and(|f| {
                                        self.reputation_with(Some(f)) <= crate::economy::HOSTILE_REP
                                    })
                                {
                                    score += range * 0.1;
                                }
                                if best
                                    .as_ref()
                                    .is_none_or(|(b, e)| score < *b || (score == *b && s.id < e.id))
                                {
                                    best = Some((score, s.clone()));
                                }
                            }
                        }
                    }
                }
                if let Some((_, s)) = best {
                    chosen = Some(s);
                    break;
                }
            }
            let Some(s) = chosen else {
                return Err(format!(
                    "AUCUNE ETOILE A PORTEE ({range:.0} AL) : LE VIDE EST TROP LARGE"
                ));
            };
            let d = s.distance_to(cur_pos);
            fuel -= self.jump_fuel_cost(d);
            let scoop = scoopable(s.class());
            if scoop && fuel < 0.6 * cap {
                fuel = cap;
            }
            if fuel < 0.0 {
                return Err(
                    "CARBURANT INSUFFISANT : AUCUNE ETOILE DE RAVITAILLEMENT SUR LA ROUTE".into(),
                );
            }
            cur_pos = s.pos;
            cur_id = s.id;
            plan.legs.push(Leg {
                star: s.id,
                pos: s.pos,
                seed: s.seed,
                name: s.header.name.clone(),
                dist: d,
                galaxy: None,
                scoop,
            });
        }
        Err("ROUTE TROP LONGUE (PLUS DE 6000 SAUTS)".into())
    }

    // ---- execution ---------------------------------------------------------------------------

    pub fn voyage_running(&self) -> bool {
        self.voyage.as_ref().is_some_and(Voyage::running)
    }

    /// The time acceleration the voyage wants (None when it is stopped or over).
    pub fn voyage_warp(&self) -> Option<f64> {
        self.voyage_running().then_some(VOYAGE_WARP)
    }

    /// Start (or restart) executing `plan`. The crew sleeps when the trip is long.
    pub fn voyage_begin(&mut self, plan: VoyagePlan) -> Result<String, String> {
        if !plan.ok {
            return Err(plan.why.clone().unwrap_or_else(|| "PLAN INVALIDE".into()));
        }
        if plan.legs.is_empty() {
            return Err("PLAN VIDE".into());
        }
        if self.docked.is_some() {
            self.undock();
        }
        self.autopilot = None;
        let now = self.time;
        let mut v = Voyage {
            alerts_seen: self.critical_alert_texts(),
            started: now,
            fuel_start: self.ship.fuel,
            ..Default::default()
        };
        v.note(
            now,
            format!(
                "DEPART VERS {} : {} SAUTS, {:.0} AL, {} DE ROUTE",
                plan.dest_name.to_uppercase(),
                plan.legs.len(),
                plan.total_ly,
                span_text(plan.total_s())
            ),
        );
        if plan.total_s() >= CRYO_THRESHOLD_S {
            match self.cryo_auto_plan(1) {
                Ok(r) => {
                    v.slept = true;
                    v.note(now, format!("CRYOSOMMEIL : {r}"));
                }
                Err(e) => v.note(now, format!("PAS DE CRYOSOMMEIL ({e})")),
            }
        }
        v.plan = plan;
        let msg = v.log.first().map(|l| l.1.clone()).unwrap_or_default();
        self.voyage = Some(v);
        Ok(msg)
    }

    /// Plan and start in one go: the "ALLER A" command.
    pub fn voyage_go(&mut self, dest: Dest) -> Result<String, String> {
        let plan = self.voyage_plan(dest)?;
        self.voyage_begin(plan)
    }

    pub fn voyage_cancel(&mut self) {
        if let Some(v) = self.voyage.as_mut() {
            let t = self.time;
            v.done = true;
            v.note(t, "VOYAGE ANNULE".into());
        }
        self.cancel_jump();
    }

    /// Continue after a stop: the route is planned again from where the ship is.
    pub fn voyage_resume(&mut self) -> Result<String, String> {
        let Some(v) = self.voyage.as_ref() else {
            return Err("AUCUN VOYAGE EN COURS".into());
        };
        if v.done {
            return Err("VOYAGE TERMINE".into());
        }
        let dest = v.plan.dest.ok_or("DESTINATION INCONNUE")?;
        let galaxy_leg = v.plan.legs.get(v.next).is_some_and(|l| l.galaxy.is_some());
        if !galaxy_leg {
            let plan = self.voyage_plan(dest)?;
            if let Some(v) = self.voyage.as_mut() {
                v.plan = plan;
                v.next = 0;
            }
        }
        let t = self.time;
        let v = self.voyage.as_mut().ok_or("AUCUN VOYAGE EN COURS")?;
        v.paused = None;
        v.skip_alert = true;
        v.note(t, "REPRISE DU VOYAGE".into());
        Ok("REPRISE".into())
    }

    fn voyage_pause(&mut self, why: String) {
        let t = self.time;
        if let Some(v) = self.voyage.as_mut() {
            v.note(t, format!("ARRET : {why}"));
            v.paused = Some(why);
        }
    }

    fn critical_alert_texts(&self) -> Vec<String> {
        self.alerts()
            .into_iter()
            .filter(|a| a.sev == crate::alerts::Severity::Critical)
            .map(|a| format!("{:?}", a.topic))
            .collect()
    }

    fn drive_damaged(&self) -> bool {
        self.ship
            .design
            .modules(&self.data)
            .any(|(i, m)| m.jump_range > 0.0 && self.ship.sys.health[i] < 0.999)
    }

    /// Called every step while a voyage exists and no jump is under way.
    pub(crate) fn voyage_tick(&mut self, dt: f64) {
        let Some(v) = self.voyage.as_ref() else {
            return;
        };
        if !v.running() || self.jump.is_some() || self.jump_run.is_some() {
            return;
        }
        if self.docked.is_some() {
            self.voyage_pause("NAVIRE AMARRE".into());
            return;
        }
        let (next, refueling, repairing, skip) = (v.next, v.refueling, v.repairing, v.skip_alert);
        if next >= v.plan.legs.len() {
            if let Some(v) = self.voyage.as_mut() {
                v.done = true;
            }
            return;
        }
        if repairing {
            // The yard and crew mend the drive in flight (no spare parts needed for patching).
            let w = self.effective.workshop;
            let rate = 0.02 * (1.0 + w) * dt / 3600.0;
            let mut changed = false;
            let ids: Vec<usize> = self
                .ship
                .design
                .modules(&self.data)
                .filter(|(_, m)| m.jump_range > 0.0)
                .map(|(i, _)| i)
                .collect();
            for i in ids {
                let h = &mut self.ship.sys.health[i];
                if *h < 1.0 {
                    *h = (*h + rate).min(1.0);
                    changed = true;
                }
            }
            if changed {
                self.ship.refresh_stats(&self.data.clone());
            }
            if !self.drive_damaged() {
                let t = self.time;
                if let Some(v) = self.voyage.as_mut() {
                    v.repairing = false;
                    v.note(t, "PROPULSEUR REPARE EN ROUTE".into());
                }
            }
            return;
        }
        let leg = self.voyage.as_ref().map(|v| v.plan.legs[next].clone());
        let Some(leg) = leg else { return };
        let need = self.jump_fuel_cost(leg.dist);
        if refueling {
            let cap = self.ship.stats.fuel_cap;
            self.ship.fuel = (self.ship.fuel + cap * dt / SCOOP_FULL_S).min(cap);
            let target = (need * 1.3).min(cap).max(0.9 * cap);
            if self.ship.fuel >= target - 1e-6 {
                let t = self.time;
                if let Some(v) = self.voyage.as_mut() {
                    v.refueling = false;
                    v.note(
                        t,
                        format!("RAVITAILLEMENT TERMINE : {:.0} KG", self.ship.fuel),
                    );
                }
            }
            return;
        }
        if !skip && self.combat_active() {
            self.voyage_pause("COMBAT EN COURS".into());
            return;
        }
        let crit = self.critical_alert_texts();
        let fresh = self
            .voyage
            .as_ref()
            .is_some_and(|v| crit.iter().any(|t| !v.alerts_seen.contains(t)));
        if let Some(v) = self.voyage.as_mut() {
            v.alerts_seen = crit;
        }
        if !skip && fresh {
            self.voyage_pause("NOUVELLE ALERTE CRITIQUE A BORD".into());
            return;
        }
        if self.ship.fuel < need {
            if scoopable(self.system.star_class) && self.ship.stats.fuel_cap >= need {
                let t = self.time;
                if let Some(v) = self.voyage.as_mut() {
                    v.refueling = true;
                    v.note(t, "RAVITAILLEMENT A L'ETOILE".into());
                }
            } else {
                self.voyage_pause(format!(
                    "CARBURANT INSUFFISANT ({:.0} KG POUR {:.0} KG NECESSAIRES)",
                    self.ship.fuel, need
                ));
            }
            return;
        }
        if let Some(v) = self.voyage.as_mut() {
            if v.tries_leg == next {
                v.tries += 1;
            } else {
                v.tries_leg = next;
                v.tries = 1;
            }
            if v.tries > 3 {
                v.tries = 0;
                self.voyage_pause(
                    "LE PROPULSEUR NE DELIVRE PAS L'ENERGIE (REACTEUR OU ETAT INSUFFISANT)".into(),
                );
                return;
            }
        }
        let entry = StarEntry {
            id: leg.star,
            pos: leg.pos,
            seed: leg.seed,
            header: vector_world::peek_star(leg.seed),
        };
        let res = match leg.galaxy {
            Some(g) => self
                .start_jump_to(&entry, leg.dist, Some(g))
                .map(|()| false),
            None => self.jump_or_run(&entry),
        };
        match res {
            Ok(_) => {
                let t = self.time;
                if let Some(v) = self.voyage.as_mut() {
                    v.skip_alert = false;
                    v.note(
                        t,
                        format!(
                            "SAUT {}/{} VERS {} ({:.1} AL)",
                            next + 1,
                            v.plan.legs.len(),
                            leg.name.to_uppercase(),
                            leg.dist
                        ),
                    );
                }
            }
            Err(e) => {
                self.voyage_pause(e.to_uppercase());
            }
        }
    }

    /// Called by `arrive` when a jump ends.
    pub(crate) fn voyage_arrived(&mut self, mishap: bool, dist_ly: f64) {
        let t = self.time;
        let here_name = self.system.name.clone();
        let Some(v) = self.voyage.as_mut() else {
            return;
        };
        if v.done {
            return;
        }
        v.next += 1;
        let (n, total) = (v.next, v.plan.legs.len());
        v.note(
            t,
            format!("ARRIVEE {n}/{total} : {here_name} ({dist_ly:.1} AL)"),
        );
        if mishap {
            v.note(t, "INCIDENT DE SAUT : PROPULSEUR ENDOMMAGE".into());
            // With a workshop the repair starts at once and the voyage goes on after it.
            if self.effective.workshop > 0.0 {
                v.repairing = true;
                v.note(t, "REPARATION DU PROPULSEUR EN COURS".into());
            } else {
                self.voyage_pause("INCIDENT DE SAUT".into());
            }
            return;
        }
        if let Some((text, stop)) = self.voyage_encounter(n) {
            let t2 = self.time;
            if let Some(v) = self.voyage.as_mut() {
                v.note(t2, text.clone());
            }
            if stop {
                self.voyage_pause(text);
                return;
            }
        }
        if n >= total {
            let sleepers = self
                .crew
                .members
                .iter()
                .filter(|p| p.alive && p.body.cryo)
                .count();
            let t2 = self.time;
            if let Some(v) = self.voyage.as_mut() {
                v.done = true;
                let fuel_used = v.fuel_start - self.ship.fuel;
                v.note(
                    t2,
                    format!(
                        "DESTINATION ATTEINTE EN {} ({sleepers} DORMEURS A REVEILLER, {fuel_used:.0} KG BRULES)",
                        span_text(t2 - v.started)
                    ),
                );
            }
            return;
        }
        // Scoop fuel on the way when the star allows it and the tank is low.
        let cap = self.ship.stats.fuel_cap;
        let next_need = self
            .voyage
            .as_ref()
            .map_or(0.0, |v| self.jump_fuel_cost(v.plan.legs[v.next].dist));
        if scoopable(self.system.star_class)
            && (self.ship.fuel < 0.6 * cap || self.ship.fuel < next_need * 1.1)
        {
            if let Some(v) = self.voyage.as_mut() {
                v.refueling = true;
                v.note(t, "RAVITAILLEMENT A L'ETOILE".into());
            }
        }
    }

    /// Deterministic encounter on arrival at leg `n`: (text, stops the voyage).
    fn voyage_encounter(&mut self, n: usize) -> Option<(String, bool)> {
        let s = self.here.sector;
        let h = hash_coord(
            self.galaxy.seed ^ 0xE4C0_17E5,
            s.0,
            s.1,
            s.2 + i64::from(self.here.index),
        );
        let sparse = Galaxy::density(self.here_pos) < 0.6;
        let p = if sparse { 0.22 } else { 0.10 };
        if f64::from((h % 10_000) as u32) / 10_000.0 >= p {
            return None;
        }
        let _ = n;
        match (h >> 20) % 6 {
            0 => {
                let c = 500.0 + f64::from(((h >> 30) % 4500) as u32);
                self.credits += c;
                Some((format!("EPAVE RECUPEREE : +{c:.0} CREDITS"), false))
            }
            1 => {
                self.research_pool += 20.0;
                Some((
                    "ANOMALIE ETUDIEE EN PASSANT : DONNEES DE RECHERCHE".into(),
                    false,
                ))
            }
            2 => {
                let dmg = 0.04 * self.ship.stats.hull_max;
                self.ship.hull = (self.ship.hull - dmg).max(1.0);
                Some((
                    "PIRATES AU POINT D'ARRIVEE : COQUE ENDOMMAGEE, FUITE".into(),
                    true,
                ))
            }
            3 => Some((
                "CIVILISATION ERRANTE CROISEE : ECHANGE DE SIGNAUX".into(),
                false,
            )),
            4 => {
                if sparse {
                    let cap = self.ship.stats.fuel_cap;
                    let low = self.ship.fuel < 0.3 * cap;
                    Some((
                        "VIDE SANS ETOILES : RAVITAILLEMENT IMPOSSIBLE ICI".into(),
                        low,
                    ))
                } else {
                    Some(("COURANT FAVORABLE : CARBURANT ECONOMISE".into(), false))
                }
            }
            _ => {
                if self.system.star_class == StarClass::X {
                    Some(("TROU NOIR A PROXIMITE : NAVIGATION PRUDENTE".into(), true))
                } else {
                    None
                }
            }
        }
    }
}
