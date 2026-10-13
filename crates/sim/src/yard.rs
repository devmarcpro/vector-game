//! The shipyard side of the designer: blueprints the player owns, ships built from them at a
//! station yard or in one of the player's own colonies (materials, work, time), refits,
//! recycling, boarding a ship of the fleet and the plans on sale.
use crate::fleet::{FleetShip, FleetState, PURCHASE_FACTOR, UPKEEP_RATE};
use crate::shipwright::{analyze, design_of, hull_of, Blueprint, Report, RECYCLE};
use crate::{Event, Sim, RESALE};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vector_data::Hull;
use vector_world::galaxy::StarRef;

use vector_core::units::DAY;
/// What a yard asks over the market price for the materials it has to fetch.
pub const YARD_MARKUP: f64 = 1.3;
/// A refit takes this share of the work of a new ship.
pub const REFIT_WORK: f64 = 0.7;
/// A colony's own yard is slower than a station's, but costs no wages.
pub const COLONY_SLOWDOWN: f64 = 1.3;
/// Each team after the first costs a bit more per day of work (overtime, transport).
pub const TEAM_PREMIUM: f64 = 0.1;
pub const MAX_TEAMS: u8 = 4;
/// People in a team: the work of a plan is in person-days.
pub const TEAM_SIZE: f64 = 8.0;

/// A ship that exists in this game: its hull as registered in the data (index
/// `base_hulls + k`) and the plan it was built from (its inside, its loadout).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomShip {
    pub hull: Hull,
    pub bp: Blueprint,
}

/// A ship being built.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct YardOrder {
    pub id: u32,
    pub name: String,
    pub bp: Blueprint,
    pub ready: f64,
    pub teams: u8,
    pub star: StarRef,
    pub place: String,
    pub refit: bool,
}

/// Where a ship can be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    Station(usize),
    Colony(usize),
}

/// What an order would cost here and now.
#[derive(Clone, Debug)]
pub struct Quote {
    pub report: Report,
    /// (resource, kg needed, kg available, kg to buy)
    pub lines: Vec<(usize, f64, f64, f64)>,
    pub credits: f64,
    pub days: f64,
    pub site: Option<Site>,
    pub problem: Option<String>,
}

impl Sim {
    /// The yard the ship is docked at: a station, or a spaceport of one of our colonies.
    pub fn yard_site(&self) -> Result<Site, &'static str> {
        let b = self.docked.ok_or("non amarre")?;
        if self.bodies[b].kind == crate::BodyKind::Station {
            return Ok(Site::Station(b));
        }
        self.colonies
            .iter()
            .position(|c| c.star == self.here && c.body == b && c.has_port(&self.data))
            .map(Site::Colony)
            .ok_or("pas de chantier ici: un chantier de station ou votre propre colonie")
    }

    pub fn new_blueprint(&mut self, name: &str) -> usize {
        let id = self.next_blueprint;
        self.next_blueprint += 1;
        self.blueprints.push(Blueprint::new(id, name));
        self.blueprints.len() - 1
    }

    pub fn duplicate_blueprint(&mut self, i: usize) -> Option<usize> {
        let mut bp = self.blueprints.get(i)?.clone();
        bp.id = self.next_blueprint;
        self.next_blueprint += 1;
        bp.name = format!("{} (COPIE)", bp.name);
        bp.origin = "joueur".to_string();
        self.blueprints.push(bp);
        Some(self.blueprints.len() - 1)
    }

    pub fn delete_blueprint(&mut self, i: usize) {
        if i < self.blueprints.len() {
            self.blueprints.remove(i);
        }
    }

    /// Keep a plan read from a shared text.
    pub fn import_blueprint(&mut self, text: &str) -> Result<usize, String> {
        let mut bp = Blueprint::from_text(&self.data, text)?;
        bp.id = self.next_blueprint;
        self.next_blueprint += 1;
        bp.origin = "partage".to_string();
        self.blueprints.push(bp);
        Ok(self.blueprints.len() - 1)
    }

    /// Buy a ready-made plan (station yards sell them).
    pub fn buy_plan(&mut self, def: usize) -> Result<usize, &'static str> {
        self.docked_station()?;
        let d = self.data.ship_plans.get(def).ok_or("plan inconnu")?.clone();
        if self.bought_plans.contains(&d.id) {
            return Err("plan deja possede");
        }
        let price = if self.rules.free() { 0.0 } else { d.price };
        if price > self.credits {
            return Err("fonds insuffisants");
        }
        let mut bp = Blueprint::new(self.next_blueprint, &d.name);
        for (part, x, y, z) in &d.cells {
            bp.place(&self.data, part, *x, *y, *z)?;
        }
        for (x, y, z, slot, module) in &d.loadout {
            bp.fit(&self.data, *x, *y, *z, *slot, module)?;
        }
        bp.origin = "achete".to_string();
        self.credits -= price;
        self.next_blueprint += 1;
        self.bought_plans.push(d.id);
        self.blueprints.push(bp);
        Ok(self.blueprints.len() - 1)
    }

    /// The price of a plan's materials here: the market's asking price, plus the yard's cut.
    fn material_price(&self, site: Site, res: usize) -> f64 {
        let ask = match site {
            Site::Station(b) => self
                .markets
                .get(&b)
                .map_or(self.data.resources[res].value, |m| m.ask(&self.data, res)),
            Site::Colony(_) => self.data.resources[res].value,
        };
        ask * YARD_MARKUP * self.rules.build_cost
    }

    /// What building `bp` costs here with `teams` teams, minus `credit` already earned back.
    pub fn quote_build(&self, bp: &Blueprint, teams: u8, refit: bool) -> Quote {
        let report = analyze(&self.data, bp);
        let teams = teams.clamp(1, MAX_TEAMS);
        let site = self.yard_site();
        let mut q = Quote {
            lines: Vec::new(),
            credits: 0.0,
            days: 0.0,
            site: site.ok(),
            problem: None,
            report,
        };
        if let Err(e) = site {
            q.problem = Some(e.to_string());
        }
        if let Some(e) = q.report.errors.first() {
            q.problem = Some(format!("plan inconstructible: {e}"));
        }
        let work = q.report.labor * if refit { REFIT_WORK } else { 1.0 };
        let slow = if matches!(q.site, Some(Site::Colony(_))) {
            COLONY_SLOWDOWN
        } else {
            1.0
        };
        q.days = (work / (f64::from(teams) * TEAM_SIZE) * slow).max(0.5);
        let wage = match q.site {
            Some(Site::Colony(_)) => 0.0,
            _ => crate::shipwright::LABOR_DAY_CR,
        };
        q.credits += work * wage * (1.0 + TEAM_PREMIUM * f64::from(teams - 1));
        for (res, need) in q.report.materials.clone() {
            let have = match q.site {
                Some(Site::Colony(ci)) => {
                    self.colonies[ci].storage.get(res).copied().unwrap_or(0.0)
                }
                _ => self.ship.cargo.kg.get(res).copied().unwrap_or(0.0),
            };
            let from_stock = have.min(need);
            let to_buy = need - from_stock;
            if to_buy > 1e-9 {
                match q.site {
                    Some(Site::Colony(_)) => {
                        if q.problem.is_none() {
                            q.problem = Some(format!(
                                "il manque {:.0} kg de {} dans la colonie",
                                to_buy, self.data.resources[res].name
                            ));
                        }
                    }
                    Some(s) => q.credits += to_buy * self.material_price(s, res),
                    None => {}
                }
            }
            q.lines.push((res, need, from_stock, to_buy));
        }
        // Modules come from the spare stock first, then from the shop.
        let mut stock = self.module_stock.clone();
        for f in &bp.fits {
            if let Some(mi) = self.data.module_index(&f.module) {
                if stock[mi] > 0 {
                    stock[mi] -= 1;
                } else {
                    q.credits += self.data.modules[mi].cost * self.rules.build_cost;
                }
            }
        }
        if self.rules.free() {
            q.credits = 0.0;
            if q.problem
                .as_deref()
                .is_some_and(|p| p.starts_with("il manque"))
            {
                q.problem = None;
            }
        }
        q
    }

    /// Order the construction of blueprint `bi` at the yard where the ship is docked. The ship
    /// is delivered to the fleet when the work is done.
    pub fn order_ship(&mut self, bi: usize, teams: u8) -> Result<u32, String> {
        self.order_inner(bi, teams, None)
    }

    fn order_inner(&mut self, bi: usize, teams: u8, refit: Option<usize>) -> Result<u32, String> {
        let bp = self
            .blueprints
            .get(bi)
            .cloned()
            .ok_or("plan inconnu".to_string())?;
        let teams = teams.clamp(1, MAX_TEAMS);
        let q = self.quote_build(&bp, teams, refit.is_some());
        if let Some(p) = q.problem {
            return Err(p);
        }
        // What the old ship gives back when it is refitted.
        let back = refit.map_or(0.0, |fi| self.recycle_value(fi));
        if q.credits > self.credits + back + 1e-6 {
            return Err("fonds insuffisants".to_string());
        }
        if let Some(fi) = refit {
            self.fleet.remove(fi);
        }
        self.credits += back;
        self.credits -= q.credits;
        let site = q.site.ok_or("pas de chantier")?;
        for (res, _, from_stock, _) in &q.lines {
            match site {
                Site::Colony(ci) => self.colonies[ci].storage[*res] -= from_stock,
                Site::Station(_) => {
                    self.ship.cargo.remove(&self.data, *res, *from_stock);
                }
            }
        }
        let mut stock = std::mem::take(&mut self.module_stock);
        for f in &bp.fits {
            if let Some(mi) = self.data.module_index(&f.module) {
                if stock[mi] > 0 {
                    stock[mi] -= 1;
                }
            }
        }
        self.module_stock = stock;
        let id = self.next_order;
        self.next_order += 1;
        let place = match site {
            Site::Station(b) => self.bodies[b].name.clone(),
            Site::Colony(ci) => self.colonies[ci].name.clone(),
        };
        self.yard_orders.push(YardOrder {
            id,
            name: bp.name.clone(),
            bp,
            ready: self.time + q.days * DAY,
            teams,
            star: self.here,
            place,
            refit: refit.is_some(),
        });
        Ok(id)
    }

    /// Register the hull of a plan in the data (once for the same plan) and return its index.
    pub fn register_custom(&mut self, bp: &Blueprint) -> usize {
        let r = analyze(&self.data, bp);
        let hull = hull_of(&self.data, bp, &r);
        if let Some(k) = self
            .custom_ships
            .iter()
            .position(|c| c.bp.cells == bp.cells && c.bp.fits == bp.fits && c.bp.name == bp.name)
        {
            return self.base_hulls + k;
        }
        self.custom_ships.push(CustomShip {
            hull: hull.clone(),
            bp: bp.clone(),
        });
        Arc::make_mut(&mut self.data).hulls.push(hull);
        self.base_hulls + self.custom_ships.len() - 1
    }

    /// Rebuild the registered hulls in the data after a load.
    pub fn restore_custom(&mut self) {
        let base = self.base_hulls;
        let d = Arc::make_mut(&mut self.data);
        d.hulls.truncate(base);
        for c in &self.custom_ships {
            d.hulls.push(c.hull.clone());
        }
    }

    /// The plan a hull index was built from (None for the stock hulls).
    pub fn plan_of_hull(&self, hull: usize) -> Option<&Blueprint> {
        hull.checked_sub(self.base_hulls)
            .and_then(|k| self.custom_ships.get(k))
            .map(|c| &c.bp)
    }

    /// Complete the orders whose work is done.
    pub(crate) fn update_yard(&mut self) {
        if self.yard_orders.is_empty() {
            return;
        }
        let now = self.time;
        let (done, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.yard_orders)
            .into_iter()
            .partition(|o| o.ready <= now);
        self.yard_orders = rest;
        for o in done {
            let hi = self.register_custom(&o.bp);
            let r = analyze(&self.data, &o.bp);
            let h = &self.data.hulls[hi];
            let upkeep = h.cost * PURCHASE_FACTOR * UPKEEP_RATE;
            let id = self.next_fleet;
            self.next_fleet += 1;
            self.fleet.push(FleetShip {
                id,
                name: o.name.clone(),
                hull: hi,
                capacity_kg: if r.cargo_kg > 0.0 {
                    r.cargo_kg.round()
                } else {
                    (r.mass_wet * 0.5).round()
                },
                upkeep,
                route: None,
                state: FleetState::Idle,
                cargo: None,
                trips: 0,
                earnings: 0.0,
                ..FleetShip::base(id, String::new(), hi, 0.0, 0.0)
            });
            self.events.push(Event::ShipBuilt { name: o.name });
        }
    }

    /// Credits a ship of the fleet is worth as scrap: the materials of its plan, partly.
    pub fn recycle_value(&self, fleet_idx: usize) -> f64 {
        let Some(s) = self.fleet.get(fleet_idx) else {
            return 0.0;
        };
        match self.plan_of_hull(s.hull) {
            Some(bp) => {
                let r = analyze(&self.data, bp);
                let mats: f64 = r
                    .materials
                    .iter()
                    .map(|(i, kg)| self.data.resources[*i].value * kg)
                    .sum();
                (mats + r.modules_cost * RESALE) * RECYCLE
            }
            None => self.data.hulls[s.hull].cost * PURCHASE_FACTOR * 0.6,
        }
    }

    /// Take a ship of the fleet apart at the yard: the materials of a custom ship go to the
    /// hold (what does not fit is paid in credits), the modules to the spare stock.
    pub fn recycle_fleet_ship(&mut self, idx: usize) -> Result<f64, &'static str> {
        self.yard_site()?;
        let s = self.fleet.get(idx).ok_or("vaisseau inconnu")?;
        if s.cargo.is_some() {
            return Err("videz la cargaison d'abord");
        }
        let Some(bp) = self.plan_of_hull(s.hull).cloned() else {
            return Err("seuls les vaisseaux sur mesure se demontent");
        };
        let r = analyze(&self.data, &bp);
        let mut credits = 0.0;
        for (res, kg) in &r.materials {
            let back = kg * RECYCLE;
            let taken = self.ship.cargo.add(&self.data, *res, back);
            credits += (back - taken) * self.data.resources[*res].value;
        }
        for f in &bp.fits {
            if let Some(mi) = self.data.module_index(&f.module) {
                self.module_stock[mi] += 1;
            }
        }
        self.fleet.remove(idx);
        self.credits += credits;
        Ok(credits)
    }

    /// Rebuild a ship of the fleet to another plan: the old one is scrapped for credit and the
    /// new work takes a share of a full build.
    pub fn refit_fleet_ship(&mut self, idx: usize, bi: usize, teams: u8) -> Result<u32, String> {
        let s = self.fleet.get(idx).ok_or("vaisseau inconnu")?;
        if s.cargo.is_some() {
            return Err("videz la cargaison d'abord".to_string());
        }
        self.order_inner(bi, teams, Some(idx))
    }

    /// Board a ship of the fleet: it becomes the ship the player flies, and the current one
    /// takes its place in the fleet (its modules go to the spare stock).
    pub fn board_fleet_ship(&mut self, idx: usize) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if self.ship.cargo.mass > 0.0 {
            return Err("videz la soute d'abord");
        }
        let s = self.fleet.get(idx).ok_or("vaisseau inconnu")?.clone();
        if s.route.is_some() || !matches!(s.state, FleetState::Idle) {
            return Err("ce vaisseau est en mission");
        }
        let old_hull = self.ship.design.hull;
        let modules: Vec<usize> = self.ship.design.slots.iter().flatten().copied().collect();
        let plan = self.plan_of_hull(s.hull).cloned();
        for m in modules {
            self.module_stock[m] += 1;
        }
        let new_hull = s.hull;
        self.ship.design = crate::design::ShipDesign::empty(&self.data, new_hull);
        if let Some(bp) = &plan {
            self.ship.design = design_of(&self.data, bp, new_hull);
        }
        self.ship.sys = crate::design::Systems::new(&self.ship.design, 0.0);
        self.ship.refresh_stats(&self.data);
        self.ship.hull = self.ship.stats.hull_max;
        let h = &self.data.hulls[old_hull];
        self.fleet[idx] = FleetShip {
            id: s.id,
            name: h.name.clone(),
            hull: old_hull,
            capacity_kg: (h.mass * 0.5).round(),
            upkeep: h.cost * PURCHASE_FACTOR * UPKEEP_RATE,
            route: None,
            state: FleetState::Idle,
            cargo: None,
            trips: 0,
            earnings: 0.0,
            ..FleetShip::base(s.id, String::new(), old_hull, 0.0, 0.0)
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rogue::{T_TERMINAL, T_UP};
    use vector_data::GameData;
    use vector_world::generate_system;

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    fn docked() -> Sim {
        for seed in 0..2000u64 {
            let sys = generate_system(seed);
            if let Some(i) = sys
                .bodies
                .iter()
                .position(|b| b.kind == crate::BodyKind::Station)
            {
                let mut s = Sim::new(sys, data());
                s.spawn_near(i, s.bodies[i].radius + 100.0);
                s.try_dock().expect("dock");
                s.credits = 5.0e6;
                s.researched.fill(true);
                return s;
            }
        }
        panic!("no station");
    }

    fn built(s: &mut Sim, plan: usize) -> usize {
        let bi = s.buy_plan(plan).expect("buy plan");
        s.order_ship(bi, 1).expect("order");
        let ready = s.yard_orders[0].ready;
        s.time = ready + 1.0;
        s.update_yard();
        s.fleet.len() - 1
    }

    #[test]
    fn a_plan_is_bought_once_and_costs_its_price() {
        let mut s = docked();
        let c0 = s.credits;
        let price = s.data.ship_plans[0].price;
        let bi = s.buy_plan(0).expect("buy");
        assert!((c0 - s.credits - price).abs() < 1e-9);
        assert_eq!(s.blueprints[bi].origin, "achete");
        assert_eq!(s.buy_plan(0), Err("plan deja possede"));
        s.credits = 0.0;
        assert_eq!(s.buy_plan(1), Err("fonds insuffisants"));
    }

    #[test]
    fn building_takes_time_money_and_materials_then_delivers_a_ship_of_the_fleet() {
        let mut s = docked();
        let bi = s.buy_plan(0).expect("buy plan");
        let steel = s.data.resource_index("steel").expect("steel");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, steel, 2000.0);
        let have = s.ship.cargo.kg[steel];
        let q = s.quote_build(&s.blueprints[bi].clone(), 1, false);
        assert!(q.problem.is_none(), "{:?}", q.problem);
        let line = q.lines.iter().find(|l| l.0 == steel).expect("steel line");
        assert!(
            (line.2 - have.min(line.1)).abs() < 1e-9,
            "the hold's steel is used first"
        );
        let c0 = s.credits;
        s.order_ship(bi, 1).expect("order");
        assert!((c0 - s.credits - q.credits).abs() < 1e-6, "pays the quote");
        assert!((s.ship.cargo.kg[steel] - (have - line.2)).abs() < 1e-9);
        assert_eq!(s.fleet.len(), 0, "not yet");
        assert_eq!(s.yard_orders.len(), 1);
        // Not before the work is done...
        s.time = s.yard_orders[0].ready - 10.0;
        s.update_yard();
        assert!(s.fleet.is_empty());
        // ...and then the ship belongs to the fleet, with its own hull in the data.
        s.time += 20.0;
        s.update_yard();
        assert_eq!(s.fleet.len(), 1);
        assert!(s.yard_orders.is_empty());
        let hi = s.fleet[0].hull;
        assert!(hi >= s.base_hulls && hi < s.data.hulls.len());
        assert_eq!(s.data.hulls[hi].name, "Courrier Zéphyr-X");
        assert!(s.fleet[0].capacity_kg > 0.0);
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, Event::ShipBuilt { .. })));
        assert!(s.plan_of_hull(hi).is_some());
        assert!(s.plan_of_hull(0).is_none());
        // The same plan built again shares its hull.
        let bi = 0;
        s.order_ship(bi, 1).expect("again");
        s.time = s.yard_orders[0].ready + 1.0;
        s.update_yard();
        assert_eq!(s.fleet[1].hull, hi);
        assert_eq!(s.custom_ships.len(), 1);
    }

    #[test]
    fn more_teams_finish_sooner_and_cost_more_wages() {
        let mut s = docked();
        let bi = s.buy_plan(0).expect("buy");
        let bp = s.blueprints[bi].clone();
        let (q1, q3) = (s.quote_build(&bp, 1, false), s.quote_build(&bp, 3, false));
        assert!(
            (q1.days - 3.0 * q3.days).abs() < 1e-9,
            "{} vs {}",
            q1.days,
            q3.days
        );
        assert!(q3.credits > q1.credits);
        // A refit is shorter than a new ship.
        assert!(s.quote_build(&bp, 1, true).days < q1.days);
    }

    #[test]
    fn the_yard_refuses_what_it_cannot_do() {
        let mut s = docked();
        let bi = s.buy_plan(0).expect("buy");
        // Nothing to build in a plan with no cockpit.
        let empty = s.new_blueprint("vide");
        assert!(s.order_ship(empty, 1).is_err());
        let mut nocore = s.blueprints[bi].clone();
        nocore.remove(5, 0, 1);
        s.blueprints.push(nocore);
        let last = s.blueprints.len() - 1;
        assert!(s
            .order_ship(last, 1)
            .unwrap_err()
            .contains("inconstructible"));
        // Not enough money: nothing is taken.
        s.credits = 100.0;
        assert!(s.order_ship(bi, 1).is_err());
        assert_eq!(s.credits, 100.0);
        assert!(s.yard_orders.is_empty());
        // Not docked: no yard.
        s.credits = 5.0e6;
        s.undock();
        assert!(s.order_ship(bi, 1).is_err());
        assert_eq!(s.yard_site(), Err("non amarre"));
    }

    #[test]
    fn boarding_a_built_ship_makes_it_the_ship_and_keeps_the_old_one_in_the_fleet() {
        let mut s = docked();
        let idx = built(&mut s, 0);
        let hi = s.fleet[idx].hull;
        let old = s.ship.design.hull;
        let n_fits = s.plan_of_hull(hi).expect("plan").fits.len();
        s.board_fleet_ship(idx).expect("board");
        assert_eq!(s.ship.design.hull, hi);
        assert_eq!(s.ship.design.slots.len(), s.data.hulls[hi].slots.len());
        assert_eq!(s.ship.design.slots.iter().flatten().count(), n_fits);
        assert_eq!(
            s.fleet[idx].hull, old,
            "the previous ship is in the fleet now"
        );
        assert_eq!(s.ship.sys.health.len(), s.ship.design.slots.len());
        assert!(s.ship.stats.thrust > 0.0 && s.ship.stats.fuel_cap > 0.0);
        // A loaded hold forbids the swap.
        let iron = s.data.resource_index("iron").expect("iron");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, iron, 10.0);
        assert!(s.board_fleet_ship(idx).is_err());
    }

    #[test]
    fn recycling_gives_back_the_share_of_the_materials() {
        let mut s = docked();
        let idx = built(&mut s, 0);
        let bp = s.plan_of_hull(s.fleet[idx].hull).expect("plan").clone();
        let r = analyze(&s.data, &bp);
        let value = |s: &Sim| -> f64 {
            s.ship
                .cargo
                .kg
                .iter()
                .enumerate()
                .map(|(i, kg)| kg * s.data.resources[i].value)
                .sum::<f64>()
                + s.credits
        };
        let v0 = value(&s);
        let stock0: u32 = s.module_stock.iter().sum();
        s.recycle_fleet_ship(idx).expect("recycle");
        let mats: f64 = r
            .materials
            .iter()
            .map(|(i, kg)| kg * s.data.resources[*i].value)
            .sum();
        assert!(
            (value(&s) - v0 - RECYCLE * mats).abs() < 1e-3,
            "materials come back at {RECYCLE}"
        );
        assert!(s.fleet.is_empty());
        assert_eq!(
            s.module_stock.iter().sum::<u32>(),
            stock0 + bp.fits.len() as u32
        );
        // A stock hull is not taken apart.
        let id = s.data.hull_index("hauler").expect("hull");
        s.buy_fleet_ship(id).expect("buy");
        assert!(s.recycle_fleet_ship(0).is_err());
    }

    #[test]
    fn a_refit_scraps_the_old_ship_and_orders_the_new_one() {
        let mut s = docked();
        let idx = built(&mut s, 0);
        let other = s.buy_plan(1).expect("plan");
        s.credits = 5.0e6;
        let n = s.fleet.len();
        s.refit_fleet_ship(idx, other, 2).expect("refit");
        assert_eq!(s.fleet.len(), n - 1);
        assert!(s.yard_orders[0].refit);
        s.time = s.yard_orders[0].ready + 1.0;
        s.update_yard();
        assert_eq!(s.fleet.len(), n);
        assert_eq!(s.data.hulls[s.fleet[0].hull].name, "Atterrisseur Mercure");
    }

    #[test]
    fn custom_ships_survive_a_save_and_a_load() {
        let mut s = docked();
        let idx = built(&mut s, 0);
        s.board_fleet_ship(idx).expect("board");
        let hi = s.ship.design.hull;
        let bytes = s.save_bytes().expect("save");
        let back = Sim::load_bytes(&bytes, data()).expect("load");
        assert_eq!(back.ship.design.hull, hi);
        assert_eq!(back.data.hulls[hi].name, "Courrier Zéphyr-X");
        assert_eq!(back.custom_ships.len(), s.custom_ships.len());
        assert_eq!(back.blueprints.len(), s.blueprints.len());
        assert_eq!(back.bought_plans, s.bought_plans);
        assert_eq!(back.ship.design.slots, s.ship.design.slots);
        assert_eq!(back.fleet[idx].hull, s.fleet[idx].hull);
    }

    #[test]
    fn the_inside_of_the_ship_is_made_from_the_design() {
        let mut s = docked();
        let idx = built(&mut s, 0);
        s.board_fleet_ship(idx).expect("board");
        let d = Arc::clone(&s.data);
        let theme = d
            .rogue_themes
            .iter()
            .position(|t| t.id == "vaisseau")
            .expect("theme");
        let p = crate::crew::generate_person(&d, 900, None);
        s.crew.hire(&d, p, 1, 0.0);
        s.rogue_begin_social(theme, 77, vec![0]);
        let r = s.rogue.take().expect("run");
        let f = &r.floors[0];
        let bp = s.plan_of_hull(s.ship.design.hull).expect("plan").clone();
        let rooms = bp.interior(&d).len();
        assert_eq!(
            r.w * r.h,
            f.tiles.len(),
            "the floor has the size of the plan"
        );
        let xs: Vec<i8> = bp.interior(&d).iter().map(|c| c.0).collect();
        let nx = (xs.iter().max().unwrap_or(&0) - xs.iter().min().unwrap_or(&0) + 1) as usize;
        assert_eq!(r.w, nx * crate::rogue::SHIP_ROOM_W + 1);
        let floors = f
            .tiles
            .iter()
            .filter(|t| **t == crate::rogue::T_FLOOR)
            .count();
        assert!(
            floors >= rooms * 10,
            "{floors} floor tiles for {rooms} rooms"
        );
        assert_eq!(f.tiles.iter().filter(|t| **t == T_UP).count(), 1);
        let terminals = f.tiles.iter().filter(|t| **t >= T_TERMINAL).count();
        assert_eq!(
            terminals,
            s.ship_terminals().len(),
            "one terminal per function"
        );
        // The same seed gives the same ship; another design gives another plan.
        s.rogue_begin_social(theme, 77, vec![0]);
        let again = s.rogue.take().expect("run");
        assert_eq!(again.floors[0].tiles, f.tiles);
        assert_eq!(again.floors[0].start, f.start);
        // Terminals sit in the rooms that suit them: the hold terminal in the hold room.
        let hold_k = vector_data::ROGUE_TERMINALS
            .iter()
            .position(|t| t.0 == "soute")
            .expect("soute");
        let at = f
            .tiles
            .iter()
            .position(|t| *t == T_TERMINAL + hold_k as u8)
            .expect("hold terminal");
        let (tx, ty) = (at % r.w, at / r.w);
        let minx = bp.interior(&d).iter().map(|c| c.0).min().unwrap_or(0);
        let miny = bp.interior(&d).iter().map(|c| c.1).min().unwrap_or(0);
        let (cx, cy) = (
            (tx - 1) / crate::rogue::SHIP_ROOM_W + minx as usize,
            (ty - 1) / crate::rogue::SHIP_ROOM_H + miny as usize,
        );
        let kind = bp
            .interior(&d)
            .into_iter()
            .find(|c| c.0 as usize == cx && c.1 as usize == cy)
            .map(|c| c.2);
        assert_eq!(kind.as_deref(), Some("soute"));
        // Everything is reachable on foot from the entrance.
        let mut seen = vec![false; f.tiles.len()];
        let mut stack = vec![f.start.1 as usize * r.w + f.start.0 as usize];
        while let Some(i) = stack.pop() {
            if seen[i] || f.tiles[i] == crate::rogue::T_WALL {
                continue;
            }
            seen[i] = true;
            let (x, y) = (i % r.w, i / r.w);
            for (nx, ny) in [
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ] {
                if nx < r.w && ny < r.h {
                    stack.push(ny * r.w + nx);
                }
            }
        }
        for (i, t) in f.tiles.iter().enumerate() {
            if *t != crate::rogue::T_WALL {
                assert!(seen[i], "tile {i} is not reachable");
            }
        }
    }

    #[test]
    fn blueprints_can_be_shared_as_text_and_deleted() {
        let mut s = docked();
        let bi = s.buy_plan(2).expect("plan");
        let text = s.blueprints[bi].to_text();
        let ci = s.import_blueprint(&text).expect("import");
        assert_eq!(s.blueprints[ci].cells, s.blueprints[bi].cells);
        assert_eq!(s.blueprints[ci].origin, "partage");
        assert_ne!(s.blueprints[ci].id, s.blueprints[bi].id);
        assert!(s.import_blueprint("n'importe quoi").is_err());
        let di = s.duplicate_blueprint(bi).expect("copy");
        assert!(s.blueprints[di].name.contains("COPIE"));
        let n = s.blueprints.len();
        s.delete_blueprint(di);
        assert_eq!(s.blueprints.len(), n - 1);
    }
}
