//! Delegated fleet: owned freighters that run trade routes between colonies and stations by themselves.
use crate::crew::DAY;
use crate::market::Market;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_world::galaxy::StarRef;
use vector_world::BodyKind;

/// Cruise speed inside a system (m/s).
pub const FLEET_SPEED: f64 = 1.2e5;
pub const LOADING_TIME: f64 = 6.0 * 3600.0;
pub const UNLOADING_TIME: f64 = 4.0 * 3600.0;
/// Hours of travel per light year between systems, plus a fixed hop overhead.
pub const FTL_HOURS_PER_LY: f64 = crate::voyage::HOURS_PER_LY;
pub const HOP_OVERHEAD: f64 = 12.0 * 3600.0;
/// Credits burnt per light year for each tonne of ship capacity (drive fuel, tolls).
pub const FTL_COST_PER_LY_TONNE: f64 = 2.5;
/// Daily upkeep as a share of the purchase price (crew, maintenance).
pub const UPKEEP_RATE: f64 = 0.0004;
pub const PURCHASE_FACTOR: f64 = 0.7;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Endpoint {
    Colony(u32),
    Station {
        star: StarRef,
        system_seed: u64,
        body: usize,
        name: String,
        system_name: String,
    },
}

impl Endpoint {
    pub fn label(&self, sim: &Sim) -> String {
        match self {
            Endpoint::Colony(id) => sim
                .colonies
                .iter()
                .find(|c| c.id == *id)
                .map_or("?".into(), |c| c.name.clone()),
            Endpoint::Station {
                name, system_name, ..
            } => format!("{name} ({system_name})"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FleetState {
    Idle,
    Loading { until: f64 },
    Outbound { t1: f64 },
    Unloading { until: f64 },
    Returning { t1: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FleetShip {
    pub id: u32,
    pub name: String,
    pub hull: usize,
    pub capacity_kg: f64,
    pub upkeep: f64,
    pub route: Option<u32>,
    pub state: FleetState,
    /// What it carries right now: (resource, kg, purchase cost paid).
    pub cargo: Option<(usize, f64, f64)>,
    pub trips: u32,
    pub earnings: f64,
    /// Trade, or a post in the player's squadron (see `FleetOrder`).
    pub order: FleetOrder,
    /// 1 = intact, 0 = lost; what the guns of a fight take away.
    pub condition: f64,
    /// Ammunition left (0..1) of a warship; refilled from the hold.
    pub ammo: f64,
    /// A ship taken in a boarding: its crew's loyalty, hands aboard and prisoners to decide on.
    pub prize: bool,
    pub loyalty: f64,
    pub crew: u32,
    pub prisoners: u32,
    /// The people a prize belonged to (who hears of what is done to its crew).
    pub origin: Option<usize>,
}

impl FleetShip {
    /// A freighter fresh from the yard: intact, supplied, no prize crew.
    pub fn base(id: u32, name: String, hull: usize, capacity_kg: f64, upkeep: f64) -> FleetShip {
        FleetShip {
            id,
            name,
            hull,
            capacity_kg,
            upkeep,
            route: None,
            state: FleetState::Idle,
            cargo: None,
            trips: 0,
            earnings: 0.0,
            order: FleetOrder::Trade,
            condition: 1.0,
            ammo: 1.0,
            prize: false,
            loyalty: 1.0,
            crew: 0,
            prisoners: 0,
            origin: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub id: u32,
    pub from: Endpoint,
    pub to: Endpoint,
    pub resource: usize,
    /// Wanted lot per trip (capped by the ship's capacity).
    pub kg: f64,
    pub active: bool,
    pub trips: u32,
    pub profit: f64,
}

fn same_system(a: &Endpoint, b: &Endpoint, sim: &Sim) -> bool {
    sim.endpoint_star(a) == sim.endpoint_star(b)
}

impl Sim {
    pub fn endpoint_star(&self, e: &Endpoint) -> Option<StarRef> {
        match e {
            Endpoint::Colony(id) => self.colonies.iter().find(|c| c.id == *id).map(|c| c.star),
            Endpoint::Station { star, .. } => Some(*star),
        }
    }

    fn endpoint_pos_ly(&self, e: &Endpoint) -> [f64; 3] {
        match self.endpoint_star(e) {
            Some(s) if s == self.here => self.here_pos,
            Some(s) => self
                .visited
                .get(&s)
                .copied()
                .or_else(|| self.galaxy.star(s).map(|x| x.pos))
                .unwrap_or(self.here_pos),
            None => self.here_pos,
        }
    }

    /// Stations and colonies the player knows about (current and visited systems).
    pub fn known_endpoints(&self) -> Vec<Endpoint> {
        let mut v: Vec<Endpoint> = self
            .colonies
            .iter()
            .map(|c| Endpoint::Colony(c.id))
            .collect();
        let mut systems: Vec<(StarRef, u64, String)> =
            vec![(self.here, self.system.seed, self.system.name.clone())];
        for id in self.visited.keys().filter(|&&k| k != self.here).take(40) {
            if let Some(star) = self.galaxy.star(*id) {
                systems.push((*id, star.seed, star.header.name.clone()));
            }
        }
        for (star, seed, name) in systems {
            let sys = if seed == self.system.seed {
                self.system.clone()
            } else {
                vector_world::generate_system(seed)
            };
            for (i, b) in sys.bodies.iter().enumerate() {
                if b.kind == BodyKind::Station {
                    v.push(Endpoint::Station {
                        star,
                        system_seed: seed,
                        body: i,
                        name: b.name.clone(),
                        system_name: name.clone(),
                    });
                }
            }
        }
        v
    }

    /// Buy a freighter (needs a station shipyard).
    pub fn buy_fleet_ship(&mut self, hull: usize) -> Result<u32, &'static str> {
        self.docked_station()?;
        let h = self.data.hulls.get(hull).ok_or("coque inconnue")?;
        if !self.is_unlocked("hull", &h.id) {
            return Err("technologie non recherchee (F4, ou regle: technologies)");
        }
        let price = if self.rules.infinite_resources {
            0.0
        } else {
            h.cost * PURCHASE_FACTOR * self.rules.build_cost
        };
        if price > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= price;
        let mult = match h.class.as_str() {
            "cargo" | "colonial" | "support" => 2.0,
            "mineur" => 1.2,
            _ => 0.5,
        };
        let id = self.next_fleet;
        self.next_fleet += 1;
        let name = {
            let nh = hash_coord(self.system.seed, i64::from(id), 0x5417, 0);
            let culture = self.faction.map(|f| self.data.factions[f].culture.as_str());
            vector_world::names::Spec::new(if nh & 1 == 0 {
                "ship_civil_m"
            } else {
                "ship_civil_f"
            })
            .culture(culture)
            .name(nh >> 1)
        };
        self.fleet.push(FleetShip::base(
            id,
            name,
            hull,
            (h.mass * mult).round(),
            h.cost * PURCHASE_FACTOR * UPKEEP_RATE,
        ));
        Ok(id)
    }

    pub fn sell_fleet_ship(&mut self, idx: usize) -> Result<f64, &'static str> {
        self.docked_station()?;
        let s = self.fleet.get(idx).ok_or("vaisseau inconnu")?;
        let v = self.data.hulls[s.hull].cost * PURCHASE_FACTOR * 0.6;
        let id = s.id;
        self.fleet.remove(idx);
        for r in &mut self.routes {
            if r.active && self.fleet.iter().all(|f| f.route != Some(r.id)) {
                r.active = false;
            }
        }
        let _ = id;
        self.credits += v;
        Ok(v)
    }

    pub fn create_route(
        &mut self,
        from: Endpoint,
        to: Endpoint,
        resource: usize,
        kg: f64,
    ) -> Result<u32, &'static str> {
        if from == to {
            return Err("origine et destination identiques");
        }
        if resource >= self.data.resources.len() || kg <= 0.0 {
            return Err("route invalide");
        }
        let id = self.next_route;
        self.next_route += 1;
        self.routes.push(Route {
            id,
            from,
            to,
            resource,
            kg,
            active: false,
            trips: 0,
            profit: 0.0,
        });
        Ok(id)
    }

    /// Put freighter `ship_idx` on route `route_idx` (a ship serves one route, a route has one ship).
    pub fn assign_route(&mut self, ship_idx: usize, route_idx: usize) -> Result<(), &'static str> {
        let rid = self.routes.get(route_idx).ok_or("route inconnue")?.id;
        if self.fleet.iter().any(|f| f.route == Some(rid)) {
            return Err("route deja desservie");
        }
        let s = self.fleet.get_mut(ship_idx).ok_or("vaisseau inconnu")?;
        if !matches!(s.state, FleetState::Idle) {
            return Err("le vaisseau est en mission");
        }
        if s.order != FleetOrder::Trade {
            return Err("le vaisseau sert dans l'escadre : rendez-lui d'abord le commerce");
        }
        s.route = Some(rid);
        self.routes[route_idx].active = true;
        Ok(())
    }

    pub fn unassign_ship(&mut self, ship_idx: usize) {
        if let Some(s) = self.fleet.get_mut(ship_idx) {
            if matches!(s.state, FleetState::Idle) {
                if let Some(rid) = s.route.take() {
                    if let Some(r) = self.routes.iter_mut().find(|r| r.id == rid) {
                        r.active = false;
                    }
                }
            }
        }
    }

    pub fn delete_route(&mut self, route_idx: usize) -> Result<(), &'static str> {
        let rid = self.routes.get(route_idx).ok_or("route inconnue")?.id;
        if self
            .fleet
            .iter()
            .any(|f| f.route == Some(rid) && !matches!(f.state, FleetState::Idle))
        {
            return Err("un vaisseau est en mission sur cette route");
        }
        for f in &mut self.fleet {
            if f.route == Some(rid) {
                f.route = None;
            }
        }
        self.routes.remove(route_idx);
        Ok(())
    }

    /// Mid-prices of a station market (live market if it is here, otherwise its untouched equilibrium).
    fn station_prices(
        &mut self,
        star: StarRef,
        system_seed: u64,
        body: usize,
        res: usize,
    ) -> (f64, f64) {
        let data = std::sync::Arc::clone(&self.data);
        if star == self.here && system_seed == self.system.seed {
            let m = self.market(body);
            return (m.bid(&data, res), m.ask(&data, res));
        }
        let industry =
            (hash_coord(system_seed, body as i64, 31, 5) % data.industries.len() as u64) as usize;
        let mut m = Market::new(
            hash_coord(system_seed, body as i64, 0, 0),
            body,
            &data,
            0.0,
            industry,
        );
        m.relax_to(1.0e9);
        let pos = self.visited.get(&star).copied().unwrap_or(self.here_pos);
        m.world = crate::bourse::world_factors(&data, self.galaxy.seed, pos, self.time);
        (m.bid(&data, res), m.ask(&data, res))
    }

    fn leg_duration(&self, a: &Endpoint, b: &Endpoint) -> f64 {
        if same_system(a, b, self) {
            let pos = |e: &Endpoint| match e {
                Endpoint::Colony(id) => self
                    .colonies
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| (c.star, c.body)),
                Endpoint::Station { star, body, .. } => Some((*star, *body)),
            };
            if let (Some((s1, b1)), Some((s2, b2))) = (pos(a), pos(b)) {
                if s1 == self.here && s2 == self.here {
                    let d = (self.bodies[b1].pos - self.bodies[b2].pos).length();
                    return (d / FLEET_SPEED).max(3600.0);
                }
            }
            return 2.0 * 86_400.0;
        }
        let (pa, pb) = (self.endpoint_pos_ly(a), self.endpoint_pos_ly(b));
        let d =
            ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt();
        d * FTL_HOURS_PER_LY * 3600.0 + HOP_OVERHEAD
    }

    fn leg_cost(&self, a: &Endpoint, b: &Endpoint, capacity: f64) -> f64 {
        if same_system(a, b, self) {
            return 0.0;
        }
        let (pa, pb) = (self.endpoint_pos_ly(a), self.endpoint_pos_ly(b));
        let d =
            ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt();
        d * FTL_COST_PER_LY_TONNE * capacity / 1000.0
    }

    /// Load the lot at the origin. Returns false if nothing could be loaded now.
    fn fleet_load(&mut self, si: usize, ri: usize) -> bool {
        let route = self.routes[ri].clone();
        let cap = self.fleet[si].capacity_kg;
        let want = route.kg.min(cap);
        let data = std::sync::Arc::clone(&self.data);
        match &route.from {
            Endpoint::Colony(id) => {
                let Some(c) = self.colonies.iter_mut().find(|c| c.id == *id) else {
                    return false;
                };
                let kg = want.min(c.storage[route.resource]);
                if kg < (want * 0.25).min(200.0) {
                    return false;
                }
                c.storage[route.resource] -= kg;
                self.fleet[si].cargo = Some((route.resource, kg, 0.0));
                true
            }
            Endpoint::Station {
                star,
                system_seed,
                body,
                ..
            } => {
                let (_, ask) = self.station_prices(*star, *system_seed, *body, route.resource);
                let kg = want.min(self.credits / ask.max(1e-9));
                if kg < (want * 0.25).min(200.0) {
                    return false;
                }
                if *star == self.here && *system_seed == self.system.seed {
                    let m = self.market(*body);
                    let (got, cost) = m.buy(&data, route.resource, kg, 1.0);
                    if got < (want * 0.25).min(200.0) {
                        m.stock[route.resource] += got;
                        return false;
                    }
                    self.credits -= cost;
                    self.fleet[si].cargo = Some((route.resource, got, cost));
                } else {
                    let cost = kg * ask;
                    self.credits -= cost;
                    self.fleet[si].cargo = Some((route.resource, kg, cost));
                }
                true
            }
        }
    }

    /// Deliver the cargo at the destination; returns revenue.
    fn fleet_unload(&mut self, si: usize, ri: usize) -> f64 {
        let route = self.routes[ri].clone();
        let Some((res, kg, cost)) = self.fleet[si].cargo.take() else {
            return 0.0;
        };
        let data = std::sync::Arc::clone(&self.data);
        let mut revenue = 0.0;
        match &route.to {
            Endpoint::Colony(id) => {
                if let Some(c) = self.colonies.iter_mut().find(|c| c.id == *id) {
                    let room = (c.stats.storage_cap.max(5_000.0) - c.total_stored()).max(0.0);
                    c.storage[res] += kg.min(room);
                    // The colony pays the shipper, out of its own treasury, half the base value of
                    // what arrived: a transfer between the two purses of the player, so that
                    // shuttling goods between colonies no longer prints money.
                    revenue =
                        (kg.min(room) * data.resources[res].value * 0.5).min(c.treasury.max(0.0));
                    c.treasury -= revenue;
                }
            }
            Endpoint::Station {
                star,
                system_seed,
                body,
                ..
            } => {
                let tax = self
                    .galaxy
                    .territory(self.endpoint_pos_ly(&route.to), data.factions.len())
                    .map_or(0.0, |f| data.factions[f].tax);
                if *star == self.here && *system_seed == self.system.seed {
                    revenue = self.market(*body).sell(&data, res, kg, 1.0) * (1.0 - tax);
                } else {
                    let (bid, _) = self.station_prices(*star, *system_seed, *body, res);
                    revenue = kg * bid * (1.0 - tax);
                }
            }
        }
        self.credits += revenue;
        let trip_cost =
            cost + self.leg_cost(&route.from, &route.to, self.fleet[si].capacity_kg) * 2.0;
        self.credits -= self.leg_cost(&route.from, &route.to, self.fleet[si].capacity_kg) * 2.0;
        let profit = revenue - trip_cost;
        self.routes[ri].profit += profit;
        self.routes[ri].trips += 1;
        self.fleet[si].trips += 1;
        self.fleet[si].earnings += profit;
        revenue
    }

    /// Advance every freighter's state machine to the current time; pay upkeep daily.
    pub(crate) fn update_fleet(&mut self) {
        if self.fleet.is_empty() {
            return;
        }
        let now = self.time;
        for si in 0..self.fleet.len() {
            let Some(rid) = self.fleet[si].route else {
                continue;
            };
            let Some(ri) = self.routes.iter().position(|r| r.id == rid) else {
                continue;
            };
            if !self.routes[ri].active {
                continue;
            }
            let (from, to) = (self.routes[ri].from.clone(), self.routes[ri].to.clone());
            let leg = self.leg_duration(&from, &to);
            match self.fleet[si].state.clone() {
                FleetState::Idle => {
                    if self.fleet_load(si, ri) {
                        self.fleet[si].state = FleetState::Loading {
                            until: now + LOADING_TIME,
                        };
                    } else {
                        // Nothing to carry yet: look again in six hours.
                        self.fleet[si].state = FleetState::Loading {
                            until: now + LOADING_TIME,
                        };
                        self.fleet[si].cargo = None;
                    }
                }
                FleetState::Loading { until } if now >= until => {
                    if self.fleet[si].cargo.is_some() {
                        self.fleet[si].state = FleetState::Outbound { t1: now + leg };
                    } else {
                        self.fleet[si].state = FleetState::Idle;
                    }
                }
                FleetState::Outbound { t1 } if now >= t1 => {
                    let rev = self.fleet_unload(si, ri);
                    self.fleet[si].state = FleetState::Unloading {
                        until: now + UNLOADING_TIME,
                    };
                    if self.here == self.endpoint_star(&to).unwrap_or(self.here) {
                        self.events.push(Event::FleetDelivery {
                            ship: si,
                            revenue: rev,
                        });
                    }
                }
                FleetState::Unloading { until } if now >= until => {
                    self.fleet[si].state = FleetState::Returning { t1: now + leg };
                }
                FleetState::Returning { t1 } if now >= t1 => {
                    self.fleet[si].state = FleetState::Idle;
                }
                _ => {}
            }
        }
        // Daily upkeep.
        let day = (now / DAY).floor() as i64;
        if day > self.fleet_paid_day {
            let days = (day - self.fleet_paid_day).min(3650) as f64;
            self.fleet_paid_day = day;
            let upkeep: f64 = self
                .fleet
                .iter()
                .map(|f| f.upkeep * f.order.upkeep_mult())
                .sum::<f64>()
                * days;
            self.credits -= upkeep;
        }
    }
}

// ---- the player's squadron: orders, escorts, prizes -------------------------------------------

/// What an owned ship does. `Trade` is the freighter's own routine (`routes`); the rest make
/// it a warship of the squadron that goes where the player goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FleetOrder {
    #[default]
    Trade,
    /// Keeps formation: screens the ship a little, fires on nobody.
    Follow,
    /// Fires on the target of the player.
    Attack,
    /// Takes hits for the player's ship.
    Defend,
    /// Engages hostile ships that come near.
    Patrol,
    /// Hired out to a people at war (see `arms::Merc`).
    Mercenary,
}

pub const ORDERS: [FleetOrder; 5] = [
    FleetOrder::Trade,
    FleetOrder::Follow,
    FleetOrder::Attack,
    FleetOrder::Defend,
    FleetOrder::Patrol,
];

impl FleetOrder {
    pub fn label(self) -> &'static str {
        match self {
            FleetOrder::Trade => "COMMERCE",
            FleetOrder::Follow => "SUIVRE",
            FleetOrder::Attack => "ATTAQUER",
            FleetOrder::Defend => "DEFENDRE",
            FleetOrder::Patrol => "PATROUILLER",
            FleetOrder::Mercenary => "MERCENAIRE",
        }
    }
    /// What the order means, with its numbers.
    pub fn text(self) -> String {
        match self {
            FleetOrder::Trade => "Le vaisseau suit ses routes de commerce (entretien x1 ; un vaisseau de guerre coute x2).".to_string(),
            FleetOrder::Follow => format!(
                "Reste en formation : prend {:.0} % des coups visant votre vaisseau, ne tire sur personne.",
                FOLLOW_SHARE * 100.0
            ),
            FleetOrder::Attack => format!(
                "Tire sur la cible que vous avez designee, avec {:.0} % des armes de ses points d'emport ; les munitions s'epuisent.",
                ESCORT_FIRE * 100.0
            ),
            FleetOrder::Defend => format!(
                "Se place devant vous : prend {:.0} % des coups visant votre vaisseau (plafond {:.0} % pour l'escadre) et peut etre perdu.",
                DEFEND_SHARE * 100.0,
                SCREEN_CAP * 100.0
            ),
            FleetOrder::Patrol => format!(
                "Engage seul les hostiles a moins de {:.0} km, a demi-cadence.",
                PATROL_RANGE / 1000.0
            ),
            FleetOrder::Mercenary => format!("Loue a un secteur en guerre pour {:.0} jours : ses coques comptent dans sa ligne de front, paie a la fin (x1,5 si victoire, x0,5 sinon).", crate::arms::MERC_DAYS),
        }
    }

    /// Warships on a war footing cost twice the upkeep of a freighter.
    pub fn upkeep_mult(self) -> f64 {
        if self == FleetOrder::Trade {
            1.0
        } else {
            2.0
        }
    }
}

/// Share of the damage aimed at the player that a screening ship takes, per ship.
pub const DEFEND_SHARE: f64 = 0.12;
pub const FOLLOW_SHARE: f64 = 0.05;
pub const SCREEN_CAP: f64 = 0.5;
/// Fraction of the weapons of the data an escort brings to bear.
pub const ESCORT_FIRE: f64 = 0.35;
/// Range (m) at which a patrol engages.
pub const PATROL_RANGE: f64 = 40_000.0;
/// Kilograms of ammunition that refill one ship from empty.
pub const RESUPPLY_KG: f64 = 500.0;
/// Loyalty of a prize, and what the daily round does to it.
pub const PRIZE_LOYALTY: f64 = 0.3;

impl Sim {
    /// Give an order to ship `idx`. Joining or leaving the squadron is done at a station.
    pub fn set_order(&mut self, idx: usize, order: FleetOrder) -> Result<(), &'static str> {
        if order == FleetOrder::Mercenary {
            return Err("un contrat se signe aupres d'un secteur en guerre");
        }
        let cur = self.fleet.get(idx).ok_or("vaisseau inconnu")?.order;
        if cur == FleetOrder::Mercenary {
            return Err("le vaisseau est sous contrat");
        }
        if (cur == FleetOrder::Trade) != (order == FleetOrder::Trade) {
            self.docked_station()?;
        }
        let s = &mut self.fleet[idx];
        if order != FleetOrder::Trade && (s.route.is_some() || !matches!(s.state, FleetState::Idle))
        {
            return Err("le vaisseau est en mission de commerce");
        }
        s.order = order;
        Ok(())
    }

    /// Ships of the squadron with this order.
    pub fn squadron(&self, order: FleetOrder) -> impl Iterator<Item = &FleetShip> {
        self.fleet.iter().filter(move |s| s.order == order)
    }

    /// Share of the hits aimed at the player that the escorts take.
    pub fn escort_screen(&self) -> f64 {
        let n = |o| self.squadron(o).filter(|s| s.condition > 0.0).count() as f64;
        (n(FleetOrder::Defend) * DEFEND_SHARE + n(FleetOrder::Follow) * FOLLOW_SHARE)
            .min(SCREEN_CAP)
    }

    /// Escorts take `amount` of damage points, shared between the screening ships; a ship
    /// whose condition falls to zero is lost.
    pub(crate) fn escort_absorb(&mut self, amount: f64) {
        let ids: Vec<usize> = (0..self.fleet.len())
            .filter(|&i| {
                matches!(self.fleet[i].order, FleetOrder::Defend | FleetOrder::Follow)
                    && self.fleet[i].condition > 0.0
            })
            .collect();
        if ids.is_empty() || amount <= 0.0 {
            return;
        }
        let each = amount / ids.len() as f64;
        let mut lost = Vec::new();
        for &i in &ids {
            let hp = self.data.hulls[self.fleet[i].hull].hp.max(1.0);
            let s = &mut self.fleet[i];
            s.condition = (s.condition - each / hp).max(0.0);
            if s.condition <= 0.0 {
                lost.push(i);
            }
        }
        for i in lost.into_iter().rev() {
            let s = self.fleet.remove(i);
            let t = self.time;
            self.news_push(
                t,
                1,
                0.0,
                format!("{} est perdu en couvrant votre vaisseau.", s.name),
            );
        }
    }

    /// Fire of the squadron for one step: attackers on the target of the player, patrols on
    /// the nearest hostile ship. Ammunition runs down while they fire.
    pub(crate) fn escort_tick(&mut self, dt: f64) {
        if self.docked.is_some() || self.fleet.is_empty() {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        let base = crate::arms::mean_weapon_dps(&data) * ESCORT_FIRE;
        let target_hostile = || -> Option<usize> {
            (0..self.npcs.len())
                .filter(|&k| {
                    let n = &self.npcs[k];
                    n.hostile
                        && !n.disabled
                        && n.visible()
                        && (n.pos - self.ship.pos).length() <= PATROL_RANGE
                })
                .min_by(|&a, &b| {
                    (self.npcs[a].pos - self.ship.pos)
                        .length()
                        .total_cmp(&(self.npcs[b].pos - self.ship.pos).length())
                })
        };
        let patrol_target = target_hostile();
        let mut shots: Vec<(usize, f64)> = Vec::new();
        for s in self.fleet.iter_mut() {
            let (target, mult) = match s.order {
                FleetOrder::Attack => (self.target_npc, 1.0),
                FleetOrder::Patrol => (patrol_target, 0.5),
                _ => continue,
            };
            let Some(k) = target else { continue };
            if s.ammo <= 0.0 || s.condition <= 0.0 {
                continue;
            }
            let dps = base * crate::arms::hardpoints(&data, s.hull) * s.condition * mult;
            if dps <= 0.0 {
                continue;
            }
            s.ammo = (s.ammo - 0.005 * dt).max(0.0);
            shots.push((k, dps * dt));
        }
        shots.sort_by_key(|x| std::cmp::Reverse(x.0));
        let mut done: Vec<usize> = Vec::new();
        for (k, d) in shots {
            // A kill earlier in the pass shifts the indices above it: skip stale ones.
            if done.iter().any(|&x| x < k) || k >= self.npcs.len() {
                continue;
            }
            if self.damage_npc(k, d) {
                done.push(k);
            }
        }
    }

    /// Refill the ammunition of the squadron from the hold (ammunition slugs).
    pub fn resupply_escorts(&mut self) -> Result<f64, &'static str> {
        let slugs = self
            .data
            .resource_index("ammo_slugs")
            .ok_or("pas de munitions")?;
        let need: f64 = self
            .fleet
            .iter()
            .filter(|s| s.order != FleetOrder::Trade)
            .map(|s| (1.0 - s.ammo) * RESUPPLY_KG)
            .sum();
        if need <= 0.0 {
            return Err("l'escadre est ravitaillee");
        }
        let have = self.ship.cargo.kg[slugs];
        if have <= 0.0 {
            return Err("pas de munitions en soute");
        }
        let frac = (have / need).min(1.0);
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, slugs, need * frac);
        for s in self
            .fleet
            .iter_mut()
            .filter(|s| s.order != FleetOrder::Trade)
        {
            s.ammo = (s.ammo + (1.0 - s.ammo) * frac).min(1.0);
        }
        Ok(need * frac)
    }

    /// Repair the squadron at a station for a fee proportional to the damage.
    pub fn repair_squadron(&mut self) -> Result<f64, &'static str> {
        self.docked_station()?;
        let cost: f64 = self
            .fleet
            .iter()
            .map(|s| (1.0 - s.condition) * self.data.hulls[s.hull].cost * 0.2)
            .sum();
        if cost <= 0.0 {
            return Err("rien a reparer");
        }
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        for s in &mut self.fleet {
            s.condition = 1.0;
        }
        Ok(cost)
    }

    // ---- prizes ---------------------------------------------------------------------------

    /// A ship taken in a boarding becomes the player's: a hull of the class its owners flew,
    /// with its crew still aboard as prisoners.
    pub(crate) fn capture_prize(&mut self, b: &crate::arms::Boarding) -> u32 {
        let data = std::sync::Arc::clone(&self.data);
        let want = match b.role {
            crate::npc::NpcRole::Trader => "cargo",
            crate::npc::NpcRole::Miner => "mineur",
            _ => "combat",
        };
        let mut pool: Vec<usize> = (0..data.hulls.len())
            .filter(|&i| data.hulls[i].class == want)
            .collect();
        if pool.is_empty() {
            pool = (0..data.hulls.len()).collect();
        }
        let h = hash_coord(
            self.system.seed,
            i64::from(b.npc_id),
            0xB0A8D,
            i64::from(b.level),
        );
        let hull = pool[(h % pool.len() as u64) as usize];
        let hd = &data.hulls[hull];
        let id = self.next_fleet;
        self.next_fleet += 1;
        let mut s = FleetShip::base(
            id,
            b.name.clone(),
            hull,
            (hd.mass * 0.5).round(),
            hd.cost * PURCHASE_FACTOR * UPKEEP_RATE,
        );
        s.prize = true;
        s.origin = b.faction;
        s.loyalty = PRIZE_LOYALTY;
        s.crew = 3 + 2 * u32::from(b.level);
        s.prisoners = s.crew;
        // A ship won in battle is a wreck: the guns of the fight left their mark.
        s.condition = 0.5;
        s.ammo = 0.3;
        s.cargo = b.cargo.map(|(r, kg)| (r, kg, 0.0));
        self.fleet.push(s);
        id
    }

    fn prize_index(&self, id: u32) -> Result<usize, &'static str> {
        self.fleet
            .iter()
            .position(|s| s.id == id && s.prize && s.prisoners > 0)
            .ok_or("pas de prisonniers sur ce vaisseau")
    }

    /// Take the crew of a prize into service: loyal sailors, but a bigger payroll.
    pub fn prize_enlist(&mut self, id: u32) -> Result<(), &'static str> {
        let i = self.prize_index(id)?;
        let s = &mut self.fleet[i];
        s.prisoners = 0;
        s.loyalty = (s.loyalty + 0.5).min(1.0);
        s.upkeep *= 1.3;
        Ok(())
    }

    /// Let the prisoners go: the people they belong to remembers it.
    pub fn prize_release(&mut self, id: u32) -> Result<(), &'static str> {
        let i = self.prize_index(id)?;
        self.fleet[i].prisoners = 0;
        let faction = self.fleet[i].origin;
        self.change_reputation(faction, 4.0);
        Ok(())
    }

    /// Sell the prisoners back: credits, and the displeasure of their people.
    pub fn prize_ransom(&mut self, id: u32) -> Result<f64, &'static str> {
        let i = self.prize_index(id)?;
        let faction = self.fleet[i].origin;
        let n = self.fleet[i].prisoners;
        self.fleet[i].prisoners = 0;
        let sum = 600.0 * f64::from(n);
        self.credits += sum;
        self.change_reputation(faction, -5.0);
        Ok(sum)
    }

    /// Kill the prisoners: a crime of war, recorded and punished.
    pub fn prize_execute(&mut self, id: u32) -> Result<(), &'static str> {
        let i = self.prize_index(id)?;
        self.fleet[i].prisoners = 0;
        let faction = self.fleet[i].origin;
        self.record_war_crime(faction, "execution de prisonniers apres un abordage");
        Ok(())
    }

    /// Daily round of the prizes: prisoners make a crew restless, an unpaid payroll loses it.
    pub(crate) fn prize_tick(&mut self, days: f64) {
        let broke = self.credits < 0.0;
        let mut mutinies = Vec::new();
        for (i, s) in self.fleet.iter_mut().enumerate().filter(|(_, s)| s.prize) {
            if s.prisoners > 0 {
                s.loyalty -= 0.02 * days;
            } else if broke {
                s.loyalty -= 0.05 * days;
            } else {
                s.loyalty = (s.loyalty + 0.01 * days).min(1.0);
            }
            if s.loyalty <= 0.0 {
                mutinies.push(i);
            }
        }
        for i in mutinies.into_iter().rev() {
            let s = self.fleet.remove(i);
            let t = self.time;
            self.news_push(
                t,
                6,
                0.0,
                format!(
                    "Mutinerie : l'equipage de {} s'est enfui avec le vaisseau.",
                    s.name
                ),
            );
        }
    }
}

impl Sim {
    /// Test helper: add a freighter without paying.
    #[cfg(test)]
    pub(crate) fn buy_fleet_ship_free(&mut self, hull: usize) {
        let id = self.next_fleet;
        self.next_fleet += 1;
        let h = &self.data.hulls[hull];
        self.fleet.push(FleetShip::base(
            id,
            format!("test #{id}"),
            hull,
            h.mass * 2.0,
            0.0,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;
    use vector_world::generate_system;

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    fn two_station_sim() -> Sim {
        let g = Galaxy::new(8);
        let e = g
            .nearby([0.0; 3], 120.0)
            .into_iter()
            .find(|e| {
                generate_system(e.seed)
                    .bodies
                    .iter()
                    .filter(|b| b.kind == BodyKind::Station)
                    .count()
                    >= 2
            })
            .expect("system with two stations");
        let mut s = Sim::new_in_galaxy(g, &e, data());
        s.reveal_all();
        s.researched.fill(true);
        let st = s
            .system
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Station)
            .expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 5.0e6;
        s
    }

    fn station_endpoints(s: &Sim) -> Vec<Endpoint> {
        s.known_endpoints()
            .into_iter()
            .filter(|e| matches!(e, Endpoint::Station { .. }))
            .collect()
    }

    #[test]
    fn buying_and_selling_freighters() {
        let mut s = two_station_sim();
        let hauler = s.data.hull_index("hauler").expect("hull");
        let c0 = s.credits;
        s.buy_fleet_ship(hauler).expect("buy");
        assert_eq!(s.fleet.len(), 1);
        assert!(s.credits < c0 && s.fleet[0].capacity_kg > 10_000.0);
        let back = s.sell_fleet_ship(0).expect("sell");
        assert!(back < c0 - s.credits + back && s.fleet.is_empty());
        s.credits = 10.0;
        assert!(s.buy_fleet_ship(hauler).is_err());
    }

    #[test]
    fn a_trade_route_earns_money_between_two_stations() {
        let mut s = two_station_sim();
        let eps = station_endpoints(&s);
        assert!(eps.len() >= 2);
        // Pick the resource with the widest price gap between the two stations.
        let data = Arc::clone(&s.data);
        let (a, b) = (eps[0].clone(), eps[1].clone());
        let price = |s: &mut Sim, e: &Endpoint, r: usize| match e {
            Endpoint::Station {
                star,
                system_seed,
                body,
                ..
            } => s.station_prices(*star, *system_seed, *body, r),
            Endpoint::Colony(_) => (0.0, 0.0),
        };
        let mut best = (0usize, 0.0f64);
        for r in 0..data.resources.len() {
            let (pa, pb) = (price(&mut s, &a, r), price(&mut s, &b, r));
            let gain = (pb.0 / pa.1.max(1e-9)) - 1.0;
            if gain > best.1 {
                best = (r, gain);
            }
        }
        let (res, gain) = best;
        let (from, to) = (a, b);
        assert!(
            gain > 0.0,
            "two different stations should differ in price somewhere"
        );
        let hauler = s.data.hull_index("freighter").expect("hull");
        s.buy_fleet_ship(hauler).expect("buy");
        let rid = s.create_route(from, to, res, 20_000.0).expect("route");
        s.assign_route(0, 0).expect("assign");
        let credits0 = s.credits;
        s.undock();
        s.advance(60.0 * DAY, &crate::ShipInput::default());
        assert!(
            s.routes[0].trips >= 1,
            "at least one delivery in two months"
        );
        assert_eq!(s.routes[0].id, rid);
        assert!(s.routes[0].profit.is_finite());
        assert!(s.fleet[0].trips >= 1);
        let _ = credits0;
    }

    #[test]
    fn colony_to_station_route_moves_goods_out_of_the_colony() {
        let mut s = two_station_sim();
        // Found a colony on the nearest rocky planet.
        let b = s
            .system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()))
            .expect("planet");
        let data = Arc::clone(&s.data);
        s.undock();
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        for (id, kg) in [
            ("construction_materials", 3000.0),
            ("colonists", 400.0),
            ("food", 300.0),
            ("water", 600.0),
        ] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.ship.pos =
            s.bodies[b].pos - vector_core::Vec3::new(0.0, 0.0, s.bodies[b].radius + 5000.0);
        s.ship.vel = s.bodies[b].vel;
        s.update_ref();
        let ci = s.found_colony(b, None).expect("colony");
        s.colonies[ci].storage.fill(0.0);
        let iron = data.resource_index("iron").expect("iron");
        s.colonies[ci].storage[iron] = 30_000.0;
        let station = station_endpoints(&s).remove(0);
        s.buy_fleet_ship_free(data.hull_index("hauler").expect("hull"));
        s.create_route(Endpoint::Colony(s.colonies[ci].id), station, iron, 8_000.0)
            .expect("route");
        s.assign_route(0, 0).expect("assign");
        s.try_dock_colony()
            .expect("park at the colony port while time passes");
        s.advance(300.0 * DAY, &crate::ShipInput::default());
        assert!(
            s.colonies[ci].storage[iron] < 30_000.0,
            "the freighter must have carried iron away"
        );
        assert!(s.routes[0].trips >= 1);
    }

    #[test]
    fn a_colony_pays_the_shipper_out_of_its_treasury_nothing_is_printed() {
        let mut s = two_station_sim();
        let b = s
            .system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()))
            .expect("planet");
        let data = Arc::clone(&s.data);
        s.undock();
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        for (id, kg) in [
            ("construction_materials", 3000.0),
            ("colonists", 400.0),
            ("food", 300.0),
            ("water", 600.0),
        ] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.ship.pos =
            s.bodies[b].pos - vector_core::Vec3::new(0.0, 0.0, s.bodies[b].radius + 5000.0);
        s.ship.vel = s.bodies[b].vel;
        s.update_ref();
        let ci = s.found_colony(b, None).expect("colony");
        s.colonies[ci].storage.fill(0.0);
        let iron = data.resource_index("iron").expect("iron");
        let station = station_endpoints(&s).remove(0);
        s.buy_fleet_ship_free(data.hull_index("hauler").expect("hull"));
        s.create_route(station, Endpoint::Colony(s.colonies[ci].id), iron, 8_000.0)
            .expect("route");
        s.fleet[0].route = Some(s.routes[0].id);
        let worth = 1_000.0 * data.resources[iron].value * 0.5;
        for (treasury, expected) in [
            (worth * 10.0, worth),
            (worth / 4.0, worth / 4.0),
            (-5.0, 0.0),
        ] {
            s.colonies[ci].treasury = treasury;
            s.fleet[0].cargo = Some((iron, 1_000.0, 0.0));
            let (before, stock) = (s.colonies[ci].treasury, s.colonies[ci].storage[iron]);
            let paid = s.fleet_unload(0, 0);
            assert!((paid - expected).abs() < 1e-6, "{paid} vs {expected}");
            assert!((before - s.colonies[ci].treasury - paid).abs() < 1e-6);
            assert!(
                s.colonies[ci].storage[iron] > stock,
                "the goods arrived anyway"
            );
        }
    }

    #[test]
    fn route_rules_and_upkeep() {
        let mut s = two_station_sim();
        let eps = station_endpoints(&s);
        let r = s.data.resource_index("iron").expect("iron");
        assert!(s
            .create_route(eps[0].clone(), eps[0].clone(), r, 100.0)
            .is_err());
        s.create_route(eps[0].clone(), eps[1].clone(), r, 1000.0)
            .expect("route");
        let hauler = s.data.hull_index("hauler").expect("hull");
        s.buy_fleet_ship(hauler).expect("buy");
        s.buy_fleet_ship(hauler).expect("buy");
        s.assign_route(0, 0).expect("assign");
        assert!(s.assign_route(1, 0).is_err(), "one ship per route");
        let credits = s.credits;
        s.undock();
        s.advance(10.0 * DAY, &crate::ShipInput::default());
        // Two ships' upkeep is charged daily on top of trade results.
        let upkeep: f64 = s.fleet.iter().map(|f| f.upkeep).sum::<f64>() * 10.0;
        assert!(upkeep > 0.0);
        assert!(s.credits.is_finite() && (s.credits - credits).abs() < 5.0e6);
        assert!(s.delete_route(0).is_err() || s.fleet[0].route.is_none());
    }

    #[test]
    fn remote_station_prices_match_the_market_found_on_arrival() {
        let mut s = two_station_sim();
        let eps = station_endpoints(&s);
        let r = s.data.resource_index("copper").expect("copper");
        if let Endpoint::Station {
            star,
            system_seed,
            body,
            ..
        } = eps[1].clone()
        {
            s.market(body); // created at t = 0, like the remote estimate assumes
                            // Estimate as if it were remote...
            let mut remote = Market::new(
                hash_coord(system_seed, body as i64, 0, 0),
                body,
                &s.data,
                0.0,
                (hash_coord(system_seed, body as i64, 31, 5) % s.data.industries.len() as u64)
                    as usize,
            );
            remote.relax_to(1.0e9);
            remote.world = crate::bourse::world_factors(&s.data, s.galaxy.seed, s.here_pos, 1.0e9);
            // ...and compare with the live market after a very long wait.
            s.time = 1.0e9;
            let d = Arc::clone(&s.data);
            let live = s.market(body).bid(&d, r);
            assert!(
                (live - remote.bid(&s.data, r)).abs() < 1e-6,
                "{live} vs {}",
                remote.bid(&s.data, r)
            );
            let _ = star;
        } else {
            panic!("expected a station");
        }
    }
}
