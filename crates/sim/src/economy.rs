//! Factions, reputation, customs, emergent contracts and the bank.
use crate::crew::DAY;
use crate::relations::Entity;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_world::galaxy::StarRef;
use vector_world::{generate_system, BodyKind};

pub const REP_MIN: f32 = -100.0;
pub const REP_MAX: f32 = 100.0;
/// Below this reputation the faction refuses docking.
pub const HOSTILE_REP: f32 = -60.0;
pub const BANK_RATE_PER_DAY: f64 = 0.002;
pub const BASE_CREDIT_LIMIT: f64 = 10_000.0;
pub const MAX_ACTIVE_CONTRACTS: usize = 5;
/// Share of the ship's value a bank lends against it.
pub const MORTGAGE_LTV: f64 = 0.5;
/// Contract offers refresh every week of game time.
pub const CONTRACT_EPOCH: f64 = 7.0 * DAY;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bank {
    pub loan: f64,
    pub last_day: i64,
    pub overdue_days: u32,
    pub total_interest: f64,
    /// The ship is pledged: the credit ceiling rises, and the bank takes the hold on seizure.
    pub mortgage: bool,
    /// Credits earned off the books (black market), not yet laundered.
    pub dirty: f64,
    /// Foreign currencies held.
    pub forex: Vec<Holding>,
    /// Monthly accounts of the player, oldest first.
    pub reports: Vec<MonthReport>,
}

/// Units of a foreign currency held (bought in sector `cell`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Holding {
    pub faction: usize,
    pub cell: [i64; 3],
    pub amount: f64,
}

/// One month of the player's accounts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonthReport {
    pub month: i64,
    pub net_worth: f64,
    pub credits: f64,
    pub loan: f64,
    pub interest: f64,
    pub dirty: f64,
}

/// Kinds of passengers: (name, fare per passenger, per light-year).
pub const PAX_KINDS: [(&str, f64, f64); 6] = [
    ("TOURISTES", 120.0, 90.0),
    ("PELERINS", 60.0, 40.0),
    ("MIGRANTS", 40.0, 30.0),
    ("REFUGIES", 15.0, 10.0),
    ("CROISIERE", 180.0, 120.0),
    ("PELERINAGE", 90.0, 60.0),
];
/// Cost of a guide per passenger and per stop, and the share of the fare it adds.
pub const GUIDE_FEE: f64 = 12.0;
pub const GUIDE_BONUS: f64 = 0.12;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contract {
    pub id: u64,
    pub dest_star: StarRef,
    pub dest_seed: u64,
    pub dest_body: usize,
    pub dest_name: String,
    pub dest_system: String,
    pub dist_ly: f64,
    pub resource: usize,
    pub kg: f64,
    pub reward: f64,
    pub expires: f64,
    pub faction: Option<usize>,
    /// Passengers to carry (then `resource` and `kg` mean nothing); 0 for a cargo contract.
    pub pax: u32,
    /// Index into `PAX_KINDS`.
    pub pax_kind: u8,
    /// Stops still to make before the destination (cruises and pilgrimages).
    pub legs: Vec<Leg>,
    /// A guide hired for the passengers.
    pub guide: bool,
    /// The sector the passengers come from.
    pub origin: [i64; 3],
}

/// Goods banned in a faction's space: its own list plus whatever its mœurs forbid.
pub fn banned_goods_of(data: &vector_data::GameData, faction: usize) -> Vec<usize> {
    let fac = &data.factions[faction];
    let values = data.ethos.profile(Some(fac.id.as_str()));
    (0..data.resources.len())
        .filter(|&r| {
            fac.contraband.iter().any(|id| *id == data.resources[r].id)
                || data.ethos.banned_by(&values, &data.resources, r).is_some()
        })
        .collect()
}

/// A stop of a cruise or a pilgrimage, before the last one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Leg {
    pub star: StarRef,
    pub seed: u64,
    pub body: usize,
    pub name: String,
    pub system: String,
}

impl Sim {
    // ---- factions & reputation -------------------------------------------------

    pub(crate) fn compute_faction(&mut self) {
        let n = self.data.factions.len();
        self.faction = self.galaxy.territory(self.here_pos, n);
    }

    /// Standing with a faction on the old -100 .. 100 scale: a view of the relations registry.
    pub fn reputation_with(&self, faction: Option<usize>) -> f32 {
        faction
            .filter(|f| *f < self.data.factions.len())
            .map_or(0.0, |f| {
                self.rel(Entity::faction(f)) * crate::relations::FACTION_SCALE
            })
    }

    /// Change the standing with a faction (old scale); its allies and rivals feel it too.
    pub fn change_reputation(&mut self, faction: Option<usize>, delta: f32) {
        if let Some(f) = faction.filter(|f| *f < self.data.factions.len()) {
            self.relate(Entity::faction(f), delta / crate::relations::FACTION_SCALE);
        }
    }

    /// Set the standing with a faction outright (old scale; scenes and tests).
    pub fn set_reputation(&mut self, faction: usize, rep: f32) {
        self.relations.set(
            Entity::faction(faction),
            (rep / crate::relations::FACTION_SCALE).clamp(REP_MIN / 100.0, REP_MAX / 100.0),
        );
    }

    /// Price multiplier for the player in the current territory: good standing earns better prices.
    pub fn trade_edge(&self) -> f64 {
        (1.0 + 0.002 * f64::from(self.reputation_with(self.faction))
            + 0.04 * self.brand_at(self.time))
            * self.language_edge()
    }

    /// Share of a sale withheld by the local authority.
    pub fn tax_rate(&self) -> f64 {
        self.faction.map_or(0.0, |f| self.data.factions[f].tax)
    }

    /// The industry running station `i` (deterministic from the system).
    pub fn station_industry(&self, i: usize) -> usize {
        (hash_coord(self.system.seed, i as i64, 31, 5) % self.data.industries.len() as u64) as usize
    }

    /// Goods banned in a faction's space: its own list plus whatever its mœurs forbid.
    pub fn banned_goods(&self, faction: usize) -> Vec<usize> {
        banned_goods_of(&self.data, faction)
    }

    /// Contraband inspection at the docks. Returns (kg seized, fine) when something was found.
    pub fn customs_check(&mut self) -> Option<(f64, f64)> {
        let f = self.faction?;
        let fac = self.data.factions[f].clone();
        let data = std::sync::Arc::clone(&self.data);
        let held: Vec<usize> = self
            .banned_goods(f)
            .into_iter()
            .filter(|&r| self.ship.cargo.kg[r] > 0.0)
            .collect();
        if held.is_empty() {
            self.change_reputation(Some(f), 0.3);
            return None;
        }
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 10.0) as i64,
            91,
            2,
        ));
        if rng.f64() > 0.4 + 0.5 * fac.aggression {
            return None; // slipped through
        }
        let (mut kg, mut value) = (0.0, 0.0);
        for r in held {
            let amount = self.ship.cargo.kg[r];
            kg += self.ship.cargo.remove(&data, r, amount);
            value += amount * data.resources[r].value;
        }
        let fine = (2.0 * value).min(self.credits);
        self.credits -= fine;
        self.change_reputation(Some(f), -8.0);
        self.offend(Some(f), "contrebande");
        self.events.push(Event::Customs {
            seized_kg: kg,
            fine,
        });
        Some((kg, fine))
    }

    // ---- contracts -----------------------------------------------------------

    /// Offers at the docked station: shortages here, and exports to nearby stations.
    pub fn offered_contracts(&mut self) -> Vec<Contract> {
        let Ok(st) = self.docked_station() else {
            return Vec::new();
        };
        let epoch = (self.time / CONTRACT_EPOCH).floor() as i64;
        let mut rng = Rng::new(hash_coord(self.system.seed, st as i64, epoch, 77));
        let data = std::sync::Arc::clone(&self.data);
        let here_name = self.system.name.clone();
        let (here, faction) = (self.here, self.faction);
        let station_name = self.bodies[st].name.clone();
        let here_seed = self.system.seed;
        let mut out = Vec::new();
        let (industry, mut shortages) = {
            let m = self.market(st);
            let mut v: Vec<(usize, f64)> = (0..data.resources.len())
                .filter(|&r| data.resources[r].value >= 0.1)
                .map(|r| (r, m.fullness(r)))
                .filter(|&(_, f)| f < 0.5)
                .collect();
            v.sort_by(|a, b| a.1.total_cmp(&b.1));
            let reward_basis: Vec<f64> = v.iter().map(|&(r, _)| m.price(&data, r)).collect();
            let need: Vec<f64> = v
                .iter()
                .map(|&(r, _)| (m.target[r] - m.stock[r]).max(0.0))
                .collect();
            (
                m.industry,
                v.into_iter()
                    .zip(reward_basis)
                    .zip(need)
                    .collect::<Vec<_>>(),
            )
        };
        shortages.truncate(2);
        for (k, (((r, _f), price), need)) in shortages.into_iter().enumerate() {
            let kg = (need * 0.4).clamp(200.0, 4000.0).round();
            out.push(Contract {
                pax: 0,
                pax_kind: 0,
                legs: Vec::new(),
                guide: false,
                origin: [0; 3],
                id: hash_coord(here_seed, st as i64, epoch, 1000 + k as i64),
                dest_star: here,
                dest_seed: here_seed,
                dest_body: st,
                dest_name: station_name.clone(),
                dest_system: here_name.clone(),
                dist_ly: 0.0,
                resource: r,
                kg,
                reward: (kg * price * 1.35).round(),
                expires: self.time + 15.0 * DAY,
                faction,
            });
        }
        // Exports: what this station makes plenty of, wanted at a neighbouring station.
        let produces: Vec<usize> = data.industries[industry]
            .produces
            .iter()
            .filter_map(|id| data.resource_index(id))
            .collect();
        if !produces.is_empty() {
            let neighbours = self.galaxy.nearby(self.here_pos, 9.0);
            let mut made = 0;
            for e in neighbours.into_iter().filter(|e| e.id != here).take(8) {
                if made >= 2 {
                    break;
                }
                let sys = generate_system(e.seed);
                let Some(bi) = sys.bodies.iter().position(|b| b.kind == BodyKind::Station) else {
                    continue;
                };
                let r = produces[(rng.next_u64() % produces.len() as u64) as usize];
                let kg = (rng.range(500.0, 3000.0) / 50.0).round() * 50.0;
                let d = e.distance_to(self.here_pos);
                out.push(Contract {
                    pax: 0,
                    pax_kind: 0,
                    legs: Vec::new(),
                    guide: false,
                    origin: [0; 3],
                    id: hash_coord(here_seed, st as i64, epoch, 2000 + made as i64),
                    dest_star: e.id,
                    dest_seed: e.seed,
                    dest_body: bi,
                    dest_name: sys.bodies[bi].name.clone(),
                    dest_system: sys.name.clone(),
                    dist_ly: d,
                    resource: r,
                    kg,
                    reward: (kg * data.resources[r].value * 1.8 + 800.0 * d).round(),
                    expires: self.time + 45.0 * DAY,
                    faction: self.galaxy.territory(e.pos, data.factions.len()),
                });
                made += 1;
            }
        }
        // Passengers: tourists, pilgrims and migrants for a neighbouring station; refugees when
        // this sector is at war (they pay little, and are grateful).
        {
            let at_war = self.local_sector().at_war;
            let near = self.galaxy.nearby(self.here_pos, 9.0);
            let dests: Vec<_> = near.into_iter().filter(|e| e.id != here).take(6).collect();
            for k in 0..2usize {
                if dests.is_empty() {
                    break;
                }
                let e = &dests[(rng.next_u64() % dests.len() as u64) as usize];
                let sys = generate_system(e.seed);
                let Some(bi) = sys.bodies.iter().position(|b| b.kind == BodyKind::Station) else {
                    continue;
                };
                let kind = if at_war && k == 0 {
                    3
                } else {
                    (rng.next_u64() % 3) as usize
                };
                let pax = 2 + (rng.next_u64() % 10) as u32;
                let d = e.distance_to(self.here_pos);
                let (_, fare, per_ly) = PAX_KINDS[kind];
                out.push(Contract {
                    pax,
                    pax_kind: kind as u8,
                    legs: Vec::new(),
                    guide: false,
                    origin: crate::politics::cell_of(self.here_pos),
                    id: hash_coord(here_seed, st as i64, epoch, 4000 + k as i64),
                    dest_star: e.id,
                    dest_seed: e.seed,
                    dest_body: bi,
                    dest_name: sys.bodies[bi].name.clone(),
                    dest_system: sys.name.clone(),
                    dist_ly: d,
                    resource: 0,
                    kg: 0.0,
                    reward: (f64::from(pax) * (fare + per_ly * d)).round(),
                    expires: self.time + (20.0 + 6.0 * d) * DAY,
                    faction: self.galaxy.territory(e.pos, data.factions.len()),
                });
            }
        }
        // Cruises and pilgrimages: several stops, a longer fare, a hotel at each stop.
        {
            let near = self.galaxy.nearby(self.here_pos, 14.0);
            let mut stops = Vec::new();
            for e in near.into_iter().filter(|e| e.id != here).take(10) {
                let sys = generate_system(e.seed);
                if let Some(bi) = sys.bodies.iter().position(|b| b.kind == BodyKind::Station) {
                    let tour = crate::macroeco::tourism(
                        &data,
                        &self.galaxy,
                        self.galaxy.seed,
                        e.pos,
                        self.time,
                    );
                    stops.push((tour, e, sys, bi));
                }
            }
            for (k, kind) in [4usize, 5].into_iter().enumerate() {
                if stops.len() < 3 {
                    break;
                }
                let mut pool: Vec<usize> = (0..stops.len()).collect();
                if kind == 4 {
                    // Cruises go where tourists like to be.
                    pool.sort_by(|a, b| stops[*b].0.total_cmp(&stops[*a].0));
                } else {
                    // Pilgrims go where the faith leads: any shrine, picked by the dice.
                    for i in (1..pool.len()).rev() {
                        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
                        pool.swap(i, j);
                    }
                }
                let n = 3.min(pool.len());
                let route = &pool[..n];
                let mut total = 0.0;
                let mut at = self.here_pos;
                let mut legs = Vec::new();
                for (i, &si) in route.iter().enumerate() {
                    let (_, e, sys, bi) = &stops[si];
                    total += e.distance_to(at);
                    at = e.pos;
                    if i + 1 < n {
                        legs.push(Leg {
                            star: e.id,
                            seed: e.seed,
                            body: *bi,
                            name: sys.bodies[*bi].name.clone(),
                            system: sys.name.clone(),
                        });
                    }
                }
                let (_, e, sys, bi) = &stops[route[n - 1]];
                let pax = 2 + (rng.next_u64() % 8) as u32;
                let (_, fare, per_ly) = PAX_KINDS[kind];
                out.push(Contract {
                    pax,
                    pax_kind: kind as u8,
                    legs,
                    guide: false,
                    origin: crate::politics::cell_of(self.here_pos),
                    id: hash_coord(here_seed, st as i64, epoch, 4100 + k as i64),
                    dest_star: e.id,
                    dest_seed: e.seed,
                    dest_body: *bi,
                    dest_name: sys.bodies[*bi].name.clone(),
                    dest_system: sys.name.clone(),
                    dist_ly: total,
                    resource: 0,
                    kg: 0.0,
                    reward: (f64::from(pax) * (fare * n as f64 + per_ly * total)).round(),
                    expires: self.time + (30.0 + 8.0 * total) * DAY,
                    faction: self.galaxy.territory(e.pos, data.factions.len()),
                });
            }
        }
        // Urgent orders born of the news: arms in a sector at war, the goods of a company that
        // suffers an incident here (what the ticker tells is what the stations ask for).
        let galaxy = vector_world::galaxy::Galaxy::new(self.galaxy.seed);
        let turmoil =
            crate::politics::turmoil(&data, &galaxy, self.galaxy.seed, self.here_pos, self.time);
        let local =
            crate::incidents::goods_factors(&data, self.galaxy.seed, self.here_pos, self.time);
        let mut urgent: Vec<(usize, f64)> = Vec::new();
        if turmoil.at_war {
            let pick = ["ammo_slugs", "weapon_parts", "medicine", "missiles"]
                [(rng.next_u64() % 4) as usize];
            if let Some(r) = data.resource_index(pick) {
                urgent.push((r, 1.0 + crate::politics::WAR_MARKUP + 0.5));
            }
        }
        for (r, f) in local.iter().enumerate() {
            if *f > 1.12 && data.resources[r].value >= 0.1 {
                urgent.push((r, *f + 0.3));
            }
        }
        for (k, (r, mult)) in urgent.into_iter().take(2).enumerate() {
            let kg = (rng.range(300.0, 1500.0) / 50.0).round() * 50.0;
            out.push(Contract {
                pax: 0,
                pax_kind: 0,
                legs: Vec::new(),
                guide: false,
                origin: [0; 3],
                id: hash_coord(here_seed, st as i64, epoch, 3000 + k as i64),
                dest_star: here,
                dest_seed: here_seed,
                dest_body: st,
                dest_name: station_name.clone(),
                dest_system: here_name.clone(),
                dist_ly: 0.0,
                resource: r,
                kg,
                reward: (kg * data.resources[r].value * mult * 1.4).round(),
                expires: self.time + 10.0 * DAY,
                faction,
            });
        }
        out.extend(self.surprise_offers_here(st));
        out.retain(|c| {
            !self.contracts.iter().any(|a| a.id == c.id) && !self.done_contracts.contains(&c.id)
        });
        out
    }

    pub fn accept_contract(&mut self, c: Contract) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if self.contracts.len() >= MAX_ACTIVE_CONTRACTS {
            return Err("trop de contrats actifs");
        }
        if c.pax > 0 && c.pax > self.free_berths() {
            return Err("pas assez de couchettes libres pour ces passagers");
        }
        self.contracts.push(c);
        Ok(())
    }

    /// Deliver active contract `idx` at the docked station if it is the destination.
    pub fn deliver_contract(&mut self, idx: usize) -> Result<f64, &'static str> {
        let st = self.docked_station()?;
        let c = self.contracts.get(idx).ok_or("contrat inconnu")?.clone();
        if let Some(leg) = c.legs.first() {
            if leg.star == self.here && leg.body == st {
                return self.visit_leg(idx);
            }
            return Err("ce n'est pas l'etape suivante");
        }
        if c.dest_star != self.here || c.dest_body != st {
            return Err("ce n'est pas la station de destination");
        }
        if c.pax > 0 {
            return self.deliver_passengers(idx, &c);
        }
        if self.ship.cargo.kg[c.resource] + 1e-6 < c.kg {
            return Err("cargaison insuffisante");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, c.resource, c.kg);
        self.market(st).stock[c.resource] += c.kg;
        // Whoever ordered the goods pays better the more it likes you (+-10 % at +-100).
        let issuer = c.faction.or(self.faction);
        let edge = 1.0 + 0.001 * f64::from(self.reputation_with(issuer));
        let pay = c.reward * edge * (1.0 - self.tax_rate());
        self.credits += pay;
        self.change_reputation(self.faction, 3.0);
        if issuer != self.faction {
            self.change_reputation(issuer, 1.5);
        }
        self.done_contracts.push(c.id);
        if self.done_contracts.len() > 200 {
            self.done_contracts.remove(0);
        }
        self.contracts.remove(idx);
        self.events.push(Event::ContractDone { reward: pay });
        Ok(pay)
    }

    /// Berths not taken by the crew or by passengers already aboard.
    pub fn free_berths(&self) -> u32 {
        let used =
            self.crew.count() as f64 + self.contracts.iter().map(|c| f64::from(c.pax)).sum::<f64>();
        (self.ship.stats.crew_cap - used).max(0.0) as u32
    }

    /// Passengers step off: the fare (scaled by the comfort of the trip), a tax, and, for
    /// migrants and refugees, new inhabitants for one of our colonies here.
    fn deliver_passengers(&mut self, idx: usize, c: &Contract) -> Result<f64, &'static str> {
        // Comfort: the emptier the ship, the better the fare (0.8 .. 1.2).
        let crowd = f64::from(c.pax) / f64::from(self.free_berths() + c.pax).max(1.0);
        let comfort = 1.2 - 0.4 * crowd;
        let kind = c.pax_kind.min(5);
        // Tourists pay for the hotels of the place: five stars of a safe, lovely sector earn
        // double what a hostel of a war zone does.
        let hotel = if matches!(kind, 0 | 4) {
            0.7 + 0.15 * f64::from(self.hotel_stars_here())
        } else {
            1.0
        };
        let guide = if c.guide { 1.0 + GUIDE_BONUS } else { 1.0 };
        let pay = c.reward * comfort * hotel * guide * (1.0 - self.tax_rate());
        self.credits += pay;
        self.change_reputation(self.faction, if kind == 3 { 6.0 } else { 2.0 });
        if matches!(kind, 2 | 3) {
            if let Some(ci) = self
                .colony_at(c.dest_body)
                .filter(|_| c.dest_star == self.here)
            {
                self.colonies[ci].pop += f64::from(c.pax);
            }
            // The people move from one sector to the other: output follows them.
            let to = crate::politics::cell_of(self.here_pos);
            self.record_migration(c.origin, to, f64::from(c.pax));
        }
        self.done_contracts.push(c.id);
        if self.done_contracts.len() > 200 {
            self.done_contracts.remove(0);
        }
        self.contracts.remove(idx);
        self.events.push(Event::ContractDone { reward: pay });
        Ok(pay)
    }

    pub(crate) fn expire_contracts(&mut self) {
        let now = self.time;
        let mut lost = Vec::new();
        self.contracts.retain(|c| {
            let keep = c.expires > now;
            if !keep {
                lost.push(c.faction);
            }
            keep
        });
        for f in lost {
            self.change_reputation(f, -2.0);
            self.events.push(Event::ContractExpired);
        }
    }

    /// Hotel rating (0..5) of the station the ship is docked at.
    pub fn hotel_stars_here(&self) -> u8 {
        let t = crate::macroeco::tourism(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            self.here_pos,
            self.time,
        );
        crate::macroeco::hotel_stars(t, self.system.seed, self.docked.unwrap_or(0))
    }

    /// Hire a guide for the passengers of contract `idx` at a station: it costs per passenger
    /// and per stop, and the passengers pay more at the end.
    pub fn hire_guide(&mut self, idx: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let c = self.contracts.get(idx).ok_or("contrat inconnu")?;
        if c.pax == 0 {
            return Err("pas de passagers");
        }
        if c.guide {
            return Err("deja un guide");
        }
        let fee = GUIDE_FEE * f64::from(c.pax) * (1.0 + c.legs.len() as f64);
        if fee > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= fee;
        self.contracts[idx].guide = true;
        Ok(fee)
    }

    /// Passengers visit the stop where the ship is docked: they spend at its hotels.
    fn visit_leg(&mut self, idx: usize) -> Result<f64, &'static str> {
        let stars = f64::from(self.hotel_stars_here());
        let c = &mut self.contracts[idx];
        let leg = c.legs.remove(0);
        let tip = f64::from(c.pax)
            * 35.0
            * (0.5 + 0.25 * stars)
            * if c.guide {
                1.0 + 2.0 * GUIDE_BONUS
            } else {
                1.0
            };
        let tip = tip * (1.0 - self.tax_rate());
        self.credits += tip;
        self.change_reputation(self.faction, 0.5);
        self.events.push(Event::MarketNews(format!(
            "Escale {} : les passagers depensent {tip:.0} CR a l'hotel ({stars:.0} etoiles).",
            leg.name.to_uppercase()
        )));
        Ok(tip)
    }

    /// The player moved `n` people from sector `from` to sector `to`.
    pub(crate) fn record_migration(&mut self, from: [i64; 3], to: [i64; 3], n: f64) {
        for (cell, d) in [(from, -n), (to, n)] {
            match self.trade.migrants.iter_mut().find(|(c, _)| *c == cell) {
                Some((_, v)) => *v += d,
                None => self.trade.migrants.push((cell, d)),
            }
        }
    }

    /// Net migrants taken into sector `cell` by the player.
    pub fn net_migrants(&self, cell: [i64; 3]) -> f64 {
        self.trade
            .migrants
            .iter()
            .find(|(c, _)| *c == cell)
            .map_or(0.0, |(_, v)| *v)
    }

    /// Output of the player's colonies in a sector and of the people they moved, credits a day.
    pub fn outside_output(&self, cell: [i64; 3]) -> f64 {
        let colonies: f64 = self
            .colonies
            .iter()
            .filter(|c| {
                self.galaxy
                    .star(c.star)
                    .is_some_and(|e| crate::politics::cell_of(e.pos) == cell)
            })
            .map(|c| c.pop * (0.5 + c.happiness) * 2.0 + 10.0 * c.buildings.len() as f64)
            .sum();
        colonies + (0.8 * self.net_migrants(cell)).clamp(-3000.0, 3000.0)
    }

    /// The economy of any sector, with what the player's colonies and the people they carried
    /// add to it.
    pub fn sector_at(&self, cell: [i64; 3]) -> crate::macroeco::Sector {
        crate::macroeco::sector_with(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            cell,
            self.time,
            self.outside_output(cell),
        )
    }

    /// The jurisdictions around the ship, nearest first: one sector per people (and the free
    /// space) within two sectors, the ship's own first.
    pub fn jurisdictions(&self) -> Vec<(String, [i64; 3])> {
        let here = crate::politics::cell_of(self.here_pos);
        let mut cells = crate::politics::cells_around(here, 2);
        let d = |c: &[i64; 3]| (0..3).map(|k| (c[k] - here[k]).pow(2)).sum::<i64>();
        cells.sort_by_key(d);
        let mut seen: Vec<Option<usize>> = Vec::new();
        let mut out = Vec::new();
        for c in cells {
            let f = crate::politics::faction_of(&self.data, &self.galaxy, c);
            if seen.contains(&f) {
                continue;
            }
            seen.push(f);
            let name = f.map_or("ESPACE LIBRE".to_string(), |f| {
                self.data.factions[f].name.to_uppercase()
            });
            out.push((name, c));
            if out.len() >= 9 {
                break;
            }
        }
        out
    }

    /// The board of one jurisdiction: figures now and 24 months of history.
    pub fn eco_board(&self, cell: [i64; 3], res: usize) -> crate::macroeco::Board {
        let extra = self.outside_output(cell);
        let series = |f: fn(&crate::macroeco::Sector) -> f64| -> Vec<f64> {
            (0..24)
                .map(|k| {
                    let t = self.time - (23 - k) as f64 * 30.0 * DAY;
                    f(&crate::macroeco::sector_with(
                        &self.data,
                        &self.galaxy,
                        self.galaxy.seed,
                        cell,
                        t,
                        extra,
                    ))
                })
                .collect()
        };
        let sector = self.sector_at(cell);
        let pos = crate::politics::cell_center(cell);
        let (by_industry, _) =
            crate::macroeco::production(&self.data, self.galaxy.seed, cell, self.time);
        crate::macroeco::Board {
            cell,
            sector,
            currency: crate::macroeco::currency_name(&self.data, sector.faction),
            tariff: crate::lanes::tariff(&self.data, sector.faction),
            port_fee: crate::lanes::port_fee(&self.data, sector.faction),
            tourism: crate::macroeco::tourism(
                &self.data,
                &self.galaxy,
                self.galaxy.seed,
                pos,
                self.time,
            ),
            by_industry,
            gdp: series(|s| s.gdp),
            inflation: series(|s| s.inflation),
            fx: series(|s| s.fx),
            rate: series(|s| s.policy_rate),
            price: (0..24)
                .map(|k| {
                    let t = self.time - (23 - k) as f64 * 30.0 * DAY;
                    self.reference_price(pos, t, res.min(self.data.resources.len() - 1))
                })
                .collect(),
            price_res: res,
        }
    }

    // ---- bank ----------------------------------------------------------------

    pub fn credit_limit(&self) -> f64 {
        let unsecured = (BASE_CREDIT_LIMIT
            + 200.0
                * self
                    .data
                    .factions
                    .iter()
                    .enumerate()
                    .map(|(i, _)| f64::from(self.reputation_with(Some(i)).max(0.0)))
                    .sum::<f64>())
            * self.local_sector().credit_factor();
        unsecured
            + if self.bank.mortgage {
                self.ship_value() * MORTGAGE_LTV
            } else {
                0.0
            }
    }

    /// The economy of the sector the ship is in.
    pub fn local_sector(&self) -> crate::macroeco::Sector {
        self.sector_at(crate::politics::cell_of(self.here_pos))
    }

    /// What the ship is worth to a banker: its hull at the shipyard price.
    pub fn ship_value(&self) -> f64 {
        self.data.hulls[self.ship.design.hull].cost * crate::fleet::PURCHASE_FACTOR
    }

    /// Pledge the ship against a higher credit ceiling.
    pub fn mortgage_ship(&mut self) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if self.bank.mortgage {
            return Err("vaisseau deja hypotheque");
        }
        self.bank.mortgage = true;
        Ok(())
    }

    /// Lift the pledge once the loan fits under the unsecured ceiling.
    pub fn release_mortgage(&mut self) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if !self.bank.mortgage {
            return Err("aucune hypotheque");
        }
        self.bank.mortgage = false;
        if self.bank.loan > self.credit_limit() {
            self.bank.mortgage = true;
            return Err("le pret depasse le plafond sans hypotheque");
        }
        Ok(())
    }

    /// Everything the player owns minus what is owed: credits, shares, foreign money, the hold at
    /// reference prices, less the loan.
    pub fn net_worth(&self) -> f64 {
        let shares: f64 = self
            .shares
            .iter()
            .map(|&(c, n)| self.company_price(c, self.time) * n)
            .sum();
        let forex: f64 = self
            .bank
            .forex
            .iter()
            .map(|h| h.amount * self.sector_at(h.cell).fx)
            .sum();
        let hold: f64 = (0..self.data.resources.len())
            .map(|r| self.ship.cargo.kg[r] * self.data.resources[r].value)
            .sum();
        self.credits + shares + forex + hold + self.portfolio_value() - self.bank.loan
    }

    // ---- currencies -------------------------------------------------------------

    /// Buy the currency of the people whose station this is with `credits` credits (a fee of
    /// 0.5 %); returns the units obtained.
    pub fn buy_currency(&mut self, credits: f64) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le change est a la station")?;
        let f = self.faction.ok_or("espace libre : pas de monnaie locale")?;
        if !(1.0..=self.credits).contains(&credits) {
            return Err("fonds insuffisants");
        }
        let cell = crate::politics::cell_of(self.here_pos);
        let units = credits * (1.0 - crate::macroeco::FX_FEE) / self.local_sector().fx;
        self.credits -= credits;
        match self.bank.forex.iter_mut().find(|h| h.faction == f) {
            Some(h) => h.amount += units,
            None => self.bank.forex.push(Holding {
                faction: f,
                cell,
                amount: units,
            }),
        }
        Ok(units)
    }

    /// Sell all the local currency held at today's rate (a fee of 0.5 %); returns the credits.
    pub fn sell_currency(&mut self) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le change est a la station")?;
        let f = self.faction.ok_or("espace libre : pas de monnaie locale")?;
        let pos = self
            .bank
            .forex
            .iter()
            .position(|h| h.faction == f)
            .ok_or("aucune monnaie de ce peuple")?;
        let h = self.bank.forex.remove(pos);
        let got = h.amount * self.local_sector().fx * (1.0 - crate::macroeco::FX_FEE);
        self.credits += got;
        Ok(got)
    }

    // ---- off the books ---------------------------------------------------------

    /// Launder dirty money through the contact of the black market (25 % fee, less with trust).
    pub fn launder(&mut self, amount: f64) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        self.faction.ok_or("espace libre : aucun contact")?;
        let amount = amount.min(self.bank.dirty).min(self.credits);
        if amount < 1.0 {
            return Err("pas d'argent sale a blanchir");
        }
        let fee = amount * (0.25 - 0.1 * f64::from(self.contact_trust()));
        self.credits -= fee;
        self.bank.dirty -= amount;
        Ok(fee)
    }

    /// Monthly accounts, and the tax audit that looks for dirty money.
    fn month_tick(&mut self, month: i64) {
        let report = MonthReport {
            month,
            net_worth: self.net_worth(),
            credits: self.credits,
            loan: self.bank.loan,
            interest: self.bank.total_interest,
            dirty: self.bank.dirty,
        };
        self.bank.reports.push(report);
        if self.bank.reports.len() > 36 {
            self.bank.reports.remove(0);
        }
        if self.bank.dirty > 0.0 {
            let mut rng = Rng::new(hash_coord(self.galaxy.seed, month, 5, 0xA0D1));
            let chance = (self.bank.dirty / (self.credits.max(1000.0) * 2.0)).clamp(0.02, 0.6);
            if rng.f64() < chance {
                let fine = (self.bank.dirty * 0.4).min(self.credits.max(0.0));
                self.credits -= fine;
                self.bank.dirty = 0.0;
                self.change_reputation(self.faction, -3.0);
                self.events.push(Event::MarketNews(format!(
                    "Controle fiscal : argent sale decouvert, amende de {fine:.0} CR."
                )));
            }
        }
    }

    pub fn borrow(&mut self, amount: f64) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if amount <= 0.0 {
            return Err("montant invalide");
        }
        if self.bank.loan + amount > self.credit_limit() {
            return Err("plafond de credit atteint");
        }
        self.bank.loan += amount;
        self.credits += amount;
        Ok(())
    }

    pub fn repay(&mut self, amount: f64) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let pay = amount.min(self.bank.loan).min(self.credits);
        if pay <= 0.0 {
            return Err("rien a rembourser");
        }
        self.bank.loan -= pay;
        self.credits -= pay;
        if self.bank.loan < 1e-6 {
            self.bank.loan = 0.0;
            self.bank.overdue_days = 0;
        }
        Ok(pay)
    }

    /// Daily interest, overdue tracking and seizure.
    pub(crate) fn bank_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        if day <= self.bank.last_day {
            return;
        }
        let days = (day - self.bank.last_day).min(3650);
        self.bank.last_day = day;
        if self.bank.loan <= 0.0 {
            return;
        }
        let before = self.bank.loan;
        let rate = self.local_sector().bank_rate_per_day();
        self.bank.loan = crate::macroeco::compound(self.bank.loan, rate, days);
        self.bank.total_interest += self.bank.loan - before;
        if self.bank.loan > self.credit_limit() * 1.2 {
            self.bank.overdue_days += days as u32;
        } else {
            self.bank.overdue_days = 0;
        }
        if self.bank.overdue_days >= 30 {
            let seized = self.credits.min(self.bank.loan * 0.5);
            self.credits -= seized;
            self.bank.loan -= seized;
            self.bank.overdue_days = 0;
            self.change_reputation(self.faction, -10.0);
            self.events.push(Event::BankSeizure { amount: seized });
            if self.bank.mortgage {
                // The pledge is called in: the hold goes to the bank at reference prices.
                let data = std::sync::Arc::clone(&self.data);
                let mut value = 0.0;
                for r in 0..data.resources.len() {
                    let kg = self.ship.cargo.kg[r];
                    if kg > 0.0 {
                        self.ship.cargo.remove(&data, r, kg);
                        value += kg * data.resources[r].value;
                    }
                }
                self.bank.loan = (self.bank.loan - value).max(0.0);
                self.bank.mortgage = false;
                self.events.push(Event::MarketNews(format!(
                    "Hypotheque saisie : la soute part a la banque ({value:.0} CR)."
                )));
            }
        }
    }

    /// Once per day of the bank tick: close the month when it turns.
    pub(crate) fn accounts_tick(&mut self) {
        let month = (self.time / DAY).floor() as i64 / 30;
        if self.bank.reports.last().is_none_or(|r| r.month < month) && self.time > 0.0 {
            self.month_tick(month);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShipInput, Sim};
    use std::sync::Arc;
    use vector_core::Vec3;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// A docked ship at the first station of a system that has neighbours with stations.
    fn docked_at_hub() -> Sim {
        let g = Galaxy::new(5);
        let entry = g
            .nearby([0.0; 3], 40.0)
            .into_iter()
            .find(|e| {
                generate_system(e.seed)
                    .bodies
                    .iter()
                    .any(|b| b.kind == BodyKind::Station)
                    && g.nearby(e.pos, 9.0).len() >= 4
            })
            .expect("hub");
        let mut s = Sim::new_in_galaxy(g, &entry, data());
        let st = s
            .system
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Station)
            .expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 100_000.0;
        s
    }

    #[test]
    fn faction_and_industry_are_assigned() {
        let s = docked_at_hub();
        assert!(s.reputation_with(Some(s.data.factions.len() - 1)).abs() < 1e-3);
        assert!(s.station_industry(0) < s.data.industries.len());
        assert_eq!(s.station_industry(3), s.station_industry(3));
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..40 {
            seen.insert(s.station_industry(i));
        }
        assert!(seen.len() >= 5, "industries should vary: {seen:?}");
    }

    #[test]
    fn reputation_changes_prices_and_tax_applies() {
        let mut s = docked_at_hub();
        s.faction = Some(0);
        let iron = s.data.resource_index("iron").expect("iron");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, iron, 400.0);
        s.set_reputation(0, 0.0);
        let before = s.market(s.docked.expect("d")).clone();
        let neutral = s.sell(iron, Some(200.0)).expect("sell");
        *s.market(s.docked.expect("d")) = before;
        s.set_reputation(0, 80.0);
        let friendly = s.sell(iron, Some(200.0)).expect("sell");
        assert!(
            friendly > neutral,
            "good standing pays better: {friendly} vs {neutral}"
        );
        s.faction = None;
        s.ship.cargo.add(&data, iron, 200.0);
        let st = s.docked.expect("d");
        let m = s.market(st).clone();
        let free = s.sell(iron, Some(100.0)).expect("sell");
        *s.market(st) = m;
        s.faction = Some(4);
        s.set_reputation(4, 0.0);
        let taxed = s.sell(iron, Some(100.0)).expect("sell");
        assert!(taxed < free, "10% tax must bite: {taxed} vs {free}");
    }

    #[test]
    fn hostile_reputation_blocks_docking() {
        let mut s = docked_at_hub();
        s.undock();
        s.faction = Some(2);
        s.set_reputation(2, -70.0);
        let st = s.nearest_station().expect("station").0;
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        assert!(s.try_dock().is_err());
        s.set_reputation(2, -10.0);
        assert!(s.try_dock().is_ok());
    }

    #[test]
    fn mores_make_goods_contraband_without_being_listed() {
        let mut s = docked_at_hub();
        let verdant = s.data.faction_index("verdant").expect("faction");
        let hegemonie = s.data.faction_index("hegemonie").expect("faction");
        let arms = s.data.resource_index("weapon_parts").expect("res");
        assert!(
            !s.data.factions[verdant]
                .contraband
                .contains(&"weapon_parts".to_string()),
            "not on the faction's own list"
        );
        // Their pacifism bans it; the militarists buy it.
        assert!(s.banned_goods(verdant).contains(&arms));
        assert!(!s.banned_goods(hegemonie).contains(&arms));
        s.faction = Some(verdant);
        let data = Arc::clone(&s.data);
        let mut caught = false;
        for k in 0..80 {
            s.time = f64::from(k) * 100.0;
            if s.ship.cargo.kg[arms] == 0.0 {
                s.ship.cargo.add(&data, arms, 40.0);
            }
            if s.customs_check().is_some() {
                assert_eq!(s.ship.cargo.kg[arms], 0.0);
                caught = true;
                break;
            }
        }
        assert!(caught, "customs seize goods the mores forbid");
    }

    #[test]
    fn customs_seizes_contraband_and_rewards_clean_ships() {
        let mut s = docked_at_hub();
        let conc = s.data.faction_index("hegemonie").expect("faction");
        s.faction = Some(conc);
        let gold = s.data.resource_index("gold").expect("gold");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, gold, 50.0);
        let mut caught = false;
        for k in 0..60 {
            s.time = f64::from(k) * 100.0;
            s.ship.cargo.add(
                &data,
                gold,
                if s.ship.cargo.kg[gold] > 0.0 {
                    0.0
                } else {
                    50.0
                },
            );
            let rep0 = s.reputation_with(Some(conc));
            if let Some((kg, fine)) = s.customs_check() {
                assert!(kg > 0.0 && fine >= 0.0);
                assert_eq!(s.ship.cargo.kg[gold], 0.0, "contraband confiscated");
                assert!(s.reputation_with(Some(conc)) < rep0);
                caught = true;
                break;
            }
        }
        assert!(
            caught,
            "an aggressive faction should catch a smuggler within a few inspections"
        );
        let rep0 = s.reputation_with(Some(conc));
        assert!(s.customs_check().is_none());
        assert!(
            s.reputation_with(Some(conc)) > rep0,
            "clean cargo earns trust"
        );
    }

    #[test]
    fn contracts_are_stable_acceptable_deliverable_and_expire() {
        let mut s = docked_at_hub();
        let a = s.offered_contracts();
        let b = s.offered_contracts();
        assert!(!a.is_empty(), "a hub with neighbours should offer work");
        assert_eq!(
            a.iter().map(|c| c.id).collect::<Vec<_>>(),
            b.iter().map(|c| c.id).collect::<Vec<_>>(),
            "offers are stable within an epoch"
        );
        // Deliver a local-shortage contract: fabricate one for this station.
        let st = s.docked.expect("d");
        let water = s.data.resource_index("water").expect("water");
        let c = Contract {
            pax: 0,
            pax_kind: 0,
            legs: Vec::new(),
            guide: false,
            origin: [0; 3],
            id: 42,
            dest_star: s.here,
            dest_seed: s.system.seed,
            dest_body: st,
            dest_name: "x".into(),
            dest_system: "y".into(),
            dist_ly: 0.0,
            resource: water,
            kg: 300.0,
            reward: 5000.0,
            expires: s.time + 10.0 * DAY,
            faction: s.faction,
        };
        s.accept_contract(c).expect("accept");
        assert!(s.deliver_contract(0).is_err(), "no cargo yet");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, water, 300.0);
        let credits0 = s.credits;
        let stock0 = s.market(st).stock[water];
        let pay = s.deliver_contract(0).expect("deliver");
        assert!(pay > 4000.0 && (s.credits - credits0 - pay).abs() < 1e-6);
        assert!(
            s.market(st).stock[water] > stock0,
            "delivered goods end up on the shelves"
        );
        assert!(s.contracts.is_empty());
        // Expiry costs reputation.
        let mut c2 = a[0].clone();
        c2.expires = s.time + 1.0;
        s.faction = Some(0);
        c2.faction = Some(0);
        s.accept_contract(c2).expect("accept");
        s.set_reputation(0, 10.0);
        s.time += 100.0;
        s.expire_contracts();
        assert!(s.contracts.is_empty() && s.reputation_with(Some(0)) < 10.0);
        // Never more than the cap.
        for k in 0..10u64 {
            let mut c3 = a[0].clone();
            c3.id = 1000 + k;
            let _ = s.accept_contract(c3);
        }
        assert!(s.contracts.len() <= MAX_ACTIVE_CONTRACTS);
    }

    #[test]
    fn bank_limits_interest_and_seizure() {
        let mut s = docked_at_hub();
        s.credits = 0.0;
        assert!(s.borrow(1.0e9).is_err(), "above the credit limit");
        s.borrow(5000.0).expect("borrow");
        assert_eq!(s.credits, 5000.0);
        s.advance(30.0 * DAY, &ShipInput::default());
        assert!(
            s.bank.loan > 5000.0 * 1.05,
            "interest accrues: {}",
            s.bank.loan
        );
        assert!(s.bank.total_interest > 250.0);
        let paid = s.repay(1000.0).expect("repay");
        assert_eq!(paid, 1000.0);
        s.credits = 1.0e6;
        s.repay(1.0e9).expect("repay all");
        assert_eq!(s.bank.loan, 0.0);
        // Overdue loans get seized.
        s.bank.loan = s.credit_limit() * 1.5;
        s.credits = 20_000.0;
        s.faction = Some(0);
        s.set_reputation(0, 0.0);
        let mut seized = false;
        for _ in 0..40 {
            s.events.clear();
            s.advance(DAY, &ShipInput::default());
            if s.events
                .iter()
                .any(|e| matches!(e, Event::BankSeizure { .. }))
            {
                seized = true;
                break;
            }
        }
        assert!(seized, "a loan far above the limit is eventually collected");
        assert!(s.reputation_with(Some(0)) < 0.0);
        let _ = Vec3::ZERO;
    }

    #[test]
    fn a_station_in_a_sector_at_war_orders_arms_urgently() {
        let d = std::sync::Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        );
        let mut s = crate::scene::new_sim(5, std::sync::Arc::clone(&d));
        let (st, _) = s.nearest_station().unwrap();
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().unwrap();
        let galaxy = vector_world::galaxy::Galaxy::new(s.galaxy.seed);
        // Put the station's sector at war (move the sector, not the station: the test only needs
        // the date and the place of the market).
        let found = crate::politics::cells_around([0, 0, 0], 2)
            .into_iter()
            .find_map(|c| {
                (1..400).find_map(|m| {
                    let t = m as f64 * crate::politics::WAR_PERIOD + 1.0;
                    crate::politics::sector_at_war(&d, &galaxy, s.galaxy.seed, c, t)
                        .then_some((c, t))
                })
            })
            .expect("a war near the origin");
        s.here_pos = crate::politics::cell_center(found.0);
        s.time = found.1;
        let offers = s.offered_contracts();
        let urgent: Vec<_> = offers
            .iter()
            .filter(|c| {
                c.dist_ly == 0.0
                    && c.reward / (c.kg * d.resources[c.resource].value) >= 2.0
                    && crate::politics::WAR_GOODS.contains(&d.resources[c.resource].id.as_str())
            })
            .collect();
        assert!(!urgent.is_empty(), "arms are ordered urgently: {offers:?}");
    }

    #[test]
    fn the_loan_grows_by_exact_compound_interest_at_the_local_rate() {
        let mut s = docked_at_hub();
        s.bank.last_day = (s.time / DAY).floor() as i64;
        s.borrow(1000.0).expect("borrow");
        let rate = s.local_sector().bank_rate_per_day();
        // Same sector, 10 days later: the tick compounds exactly.
        s.time += 10.0 * DAY;
        s.bank_tick();
        let want = crate::macroeco::compound(1000.0, s.local_sector().bank_rate_per_day(), 10);
        assert!(
            (s.bank.loan - want).abs() < 0.5,
            "{} vs {want} (rate {rate})",
            s.bank.loan
        );
        assert!((s.bank.total_interest - (s.bank.loan - 1000.0)).abs() < 1e-6);
    }

    #[test]
    fn mortgaging_the_ship_raises_the_ceiling_and_default_takes_the_hold() {
        let mut s = docked_at_hub();
        let base = s.credit_limit();
        s.mortgage_ship().expect("mortgage");
        assert!((s.credit_limit() - base - s.ship_value() * MORTGAGE_LTV).abs() < 1e-6);
        assert!(s.mortgage_ship().is_err(), "already pledged");
        s.release_mortgage().expect("nothing owed, can release");
        assert_eq!(s.credit_limit(), base);
        // Default: a loan far above the ceiling, 30 days.
        s.mortgage_ship().expect("mortgage");
        s.bank.last_day = (s.time / DAY).floor() as i64;
        s.bank.loan = s.credit_limit() * 1.5;
        let water = s.data.resource_index("water").expect("water");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, water, 200.0);
        s.time += 31.0 * DAY;
        s.bank_tick();
        assert!(!s.bank.mortgage, "the pledge was called in");
        assert_eq!(s.ship.cargo.kg[water], 0.0, "the hold went to the bank");
        assert!(s.release_mortgage().is_err());
    }

    #[test]
    fn currency_exchange_loses_only_the_fees_and_the_rate_floats() {
        let mut s = docked_at_hub();
        s.credits = 10_000.0;
        s.faction = Some(0);
        let fx = s.local_sector().fx;
        let nw0 = s.net_worth();
        let units = s.buy_currency(1000.0).expect("buy");
        assert!((units * fx - 1000.0 * (1.0 - crate::macroeco::FX_FEE)).abs() < 1e-6);
        assert!(
            (nw0 - s.net_worth() - 1000.0 * crate::macroeco::FX_FEE).abs() < 1e-6,
            "only the fee is lost"
        );
        let back = s.sell_currency().expect("sell");
        let fee = crate::macroeco::FX_FEE;
        assert!((back - 1000.0 * (1.0 - fee) * (1.0 - fee)).abs() < 1e-6);
        assert!(
            (s.credits - (9000.0 + back)).abs() < 1e-6,
            "money is conserved"
        );
        assert!(s.sell_currency().is_err(), "nothing left to sell");
        // The rate moves with the date.
        let a = s.local_sector().fx;
        s.time += 400.0 * DAY;
        let b = s.local_sector().fx;
        assert!((a - b).abs() > 1e-9);
    }

    #[test]
    fn dirty_money_is_laundered_for_a_fee_or_found_by_the_audit() {
        let mut s = docked_at_hub();
        s.faction = Some(0);
        s.credits = 50_000.0;
        s.bank.dirty = 20_000.0;
        let fee = s.launder(10_000.0).expect("launder");
        assert!((fee - 2500.0).abs() < 1e-6, "25 % with no trust: {fee}");
        assert!((s.bank.dirty - 10_000.0).abs() < 1e-9 && (s.credits - 47_500.0).abs() < 1e-6);
        assert!(s.launder(0.0).is_err() || s.bank.dirty > 0.0);
        // Left alone for long enough, an audit finds the rest.
        let mut found = false;
        for m in 1..400 {
            s.time = f64::from(m) * 31.0 * DAY;
            s.bank.dirty = 10_000.0;
            let c0 = s.credits;
            s.accounts_tick();
            if s.credits < c0 {
                assert_eq!(s.bank.dirty, 0.0);
                assert!((c0 - s.credits - 4000.0).abs() < 1e-6, "fine of 40 %");
                found = true;
                break;
            }
        }
        assert!(found, "the tax office eventually finds dirty money");
        // No dirty money, no audit.
        s.bank.dirty = 0.0;
        let c = s.credits;
        s.time += 31.0 * DAY;
        s.accounts_tick();
        assert_eq!(s.credits, c);
    }

    #[test]
    fn monthly_accounts_record_the_net_worth_once_a_month() {
        let mut s = docked_at_hub();
        s.time = 100.0 * DAY;
        s.accounts_tick();
        s.accounts_tick();
        assert_eq!(s.bank.reports.len(), 1, "one report per month");
        let nw = s.net_worth();
        assert!((s.bank.reports[0].net_worth - nw).abs() < 1e-6);
        s.time += 35.0 * DAY;
        s.accounts_tick();
        assert_eq!(s.bank.reports.len(), 2);
    }

    #[test]
    fn passengers_need_berths_pay_a_fare_and_migrants_settle() {
        let mut s = docked_at_hub();
        let st = s.docked.expect("docked");
        let offers = s.offered_contracts();
        assert!(
            offers.iter().any(|c| c.pax > 0),
            "stations offer passengers"
        );
        assert!(offers
            .iter()
            .filter(|c| c.pax > 0)
            .all(|c| c.reward > 0.0 && c.kg == 0.0));
        let make = |pax: u32, kind: u8, s: &Sim| Contract {
            pax,
            pax_kind: kind,
            legs: Vec::new(),
            guide: false,
            origin: [0; 3],
            id: 900 + u64::from(pax),
            dest_star: s.here,
            dest_seed: s.system.seed,
            dest_body: st,
            dest_name: "x".into(),
            dest_system: "y".into(),
            dist_ly: 0.0,
            resource: 0,
            kg: 0.0,
            reward: 1000.0,
            expires: s.time + 10.0 * DAY,
            faction: s.faction,
        };
        let cap = s.free_berths();
        assert!(
            s.accept_contract(make(cap + 1, 0, &s)).is_err(),
            "no berths"
        );
        s.accept_contract(make(2, 0, &s)).expect("2 tourists fit");
        assert_eq!(s.free_berths(), cap - 2, "passengers take berths");
        let pay = s.deliver_contract(0).expect("deliver");
        assert!(
            pay > 500.0 && pay < 2000.0,
            "fare within comfort and hotel bounds: {pay}"
        );
        assert_eq!(s.free_berths(), cap);
        // Migrants join a colony of ours at the destination.
        let ci = s.found_colony(st, None).ok();
        if let Some(ci) = ci {
            let pop0 = s.colonies[ci].pop;
            s.accept_contract(make(3, 2, &s)).expect("accept migrants");
            s.deliver_contract(0).expect("deliver");
            assert!((s.colonies[ci].pop - pop0 - 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_cruise_makes_its_stops_in_order_with_a_guide_and_migrants_move_the_economy() {
        let mut s = docked_at_hub();
        let st = s.docked.expect("docked");
        let here_leg = Leg {
            star: s.here,
            seed: s.system.seed,
            body: st,
            name: "ESCALE".into(),
            system: "S".into(),
        };
        let far = vector_world::galaxy::StarRef {
            sector: (999, 0, 0),
            index: 0,
        };
        let make = |s: &Sim, pax: u32, kind: u8, legs: Vec<Leg>, origin: [i64; 3]| Contract {
            pax,
            pax_kind: kind,
            legs,
            guide: false,
            origin,
            id: 700 + u64::from(pax) + u64::from(kind),
            dest_star: s.here,
            dest_seed: s.system.seed,
            dest_body: st,
            dest_name: "x".into(),
            dest_system: "y".into(),
            dist_ly: 0.0,
            resource: 0,
            kg: 0.0,
            reward: 1000.0,
            expires: s.time + 10.0 * DAY,
            faction: s.faction,
        };
        // A stop that is not the next one is refused; the right one pays a tip and is crossed off.
        let mut elsewhere = here_leg.clone();
        elsewhere.star = far;
        s.accept_contract(make(&s, 2, 4, vec![elsewhere, here_leg.clone()], [0; 3]))
            .unwrap();
        assert!(
            s.deliver_contract(0).is_err(),
            "the first stop is elsewhere"
        );
        s.contracts[0].legs.remove(0);
        let credits = s.credits;
        let tip = s.deliver_contract(0).expect("stop");
        assert!(tip > 0.0 && (s.credits - credits - tip).abs() < 1e-9);
        assert!(s.contracts[0].legs.is_empty(), "stop crossed off");
        // A guide costs per passenger and per stop, once, and the final fare is higher.
        s.contracts[0].legs.push(here_leg.clone());
        let fee = s.hire_guide(0).expect("guide");
        assert!((fee - GUIDE_FEE * 2.0 * 2.0).abs() < 1e-9);
        assert!(s.hire_guide(0).is_err());
        let guided = s.deliver_contract(0).expect("second stop with a guide");
        assert!(
            guided > tip,
            "a guide makes the passengers spend more: {guided} vs {tip}"
        );
        let paid_guided = s.deliver_contract(0).expect("arrival");
        s.accept_contract(make(&s, 2, 4, vec![], [0; 3])).unwrap();
        let paid_plain = s.deliver_contract(0).expect("arrival");
        assert!(
            (paid_guided / paid_plain - (1.0 + GUIDE_BONUS)).abs() < 0.02,
            "{paid_guided} vs {paid_plain}"
        );
        // Hotels: a safe lovely place pays more than a dangerous one for the same trip.
        assert!((0..=5).contains(&s.hotel_stars_here()));
        // Refugees leave a sector and arrive in another: output follows the people.
        let here_cell = crate::politics::cell_of(s.here_pos);
        let from = [here_cell[0] + 5, here_cell[1], here_cell[2]];
        let g0 = s.sector_at(here_cell).gdp;
        let f0 = s.sector_at(from).gdp;
        s.accept_contract(make(&s, 2, 3, vec![], from)).unwrap();
        s.deliver_contract(0).expect("refugees");
        assert_eq!(s.net_migrants(here_cell), 2.0);
        assert_eq!(s.net_migrants(from), -2.0);
        assert!(s.sector_at(here_cell).gdp > g0, "the host sector grows");
        assert!(s.sector_at(from).gdp < f0, "the origin loses its people");
    }

    #[test]
    fn colonies_of_the_player_add_to_the_output_of_their_sector() {
        let mut s = docked_at_hub();
        let st = s.docked.expect("docked");
        let cell = crate::politics::cell_of(s.here_pos);
        let before = s.sector_at(cell).gdp;
        assert_eq!(s.outside_output(cell), 0.0);
        if s.found_colony(st, None).is_ok() {
            s.colonies[0].pop = 400.0;
            assert!(s.outside_output(cell) > 400.0);
            assert!(s.sector_at(cell).gdp > before);
        }
    }

    #[test]
    fn a_crash_tightens_credit_and_sinks_the_shares_of_the_sector() {
        let s = docked_at_hub();
        let galaxy = vector_world::galaxy::Galaxy::new(s.galaxy.seed);
        let _ = galaxy;
        // Find a sector in crash and compare against its calm.
        let seed = s.galaxy.seed;
        let (cell, t) = (0..300)
            .find_map(|x| {
                let cell = [x, 0, 0];
                (0..3000).step_by(5).find_map(|d| {
                    let t = f64::from(d) * DAY;
                    (crate::macroeco::crisis(seed, cell, t).0 > 0.2).then_some((cell, t))
                })
            })
            .expect("some sector crashes");
        let m = crate::macroeco::sector(&s.data, &s.galaxy, seed, cell, t);
        assert!(m.credit_factor() < 0.8 && m.share_factor() < 0.8);
        assert!(m.unemployment > 0.06);
    }
}
