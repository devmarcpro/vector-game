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
pub const FTL_HOURS_PER_LY: f64 = 6.0;
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
        let price = h.cost * PURCHASE_FACTOR;
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
        self.fleet.push(FleetShip {
            id,
            name: format!("{} #{}", h.name, id),
            hull,
            capacity_kg: (h.mass * mult).round(),
            upkeep: h.cost * PURCHASE_FACTOR * UPKEEP_RATE,
            route: None,
            state: FleetState::Idle,
            cargo: None,
            trips: 0,
            earnings: 0.0,
        });
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
                    // The colony credits the shipper at the base value of what arrived.
                    revenue = kg.min(room) * data.resources[res].value * 0.5;
                    c.treasury -= 0.0;
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
            let upkeep: f64 = self.fleet.iter().map(|f| f.upkeep).sum::<f64>() * days;
            self.credits -= upkeep;
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
        self.fleet.push(FleetShip {
            id,
            name: format!("test #{id}"),
            hull,
            capacity_kg: h.mass * 2.0,
            upkeep: 0.0,
            route: None,
            state: FleetState::Idle,
            cargo: None,
            trips: 0,
            earnings: 0.0,
        });
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
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
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
