//! Factions, reputation, customs, emergent contracts and the bank.
use crate::crew::DAY;
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
pub const PAX_KINDS: [(&str, f64, f64); 4] = [
    ("TOURISTES", 120.0, 90.0),
    ("PELERINS", 60.0, 40.0),
    ("MIGRANTS", 40.0, 30.0),
    ("REFUGIES", 15.0, 10.0),
];

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
}

impl Sim {
    // ---- factions & reputation -------------------------------------------------

    pub(crate) fn compute_faction(&mut self) {
        let n = self.data.factions.len();
        self.faction = self.galaxy.territory(self.here_pos, n);
    }

    pub fn reputation_with(&self, faction: Option<usize>) -> f32 {
        faction
            .and_then(|f| self.reputation.get(f))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn change_reputation(&mut self, faction: Option<usize>, delta: f32) {
        if let Some(r) = faction.and_then(|f| self.reputation.get_mut(f)) {
            *r = (*r + delta).clamp(REP_MIN, REP_MAX);
        }
    }

    /// Price multiplier for the player in the current territory: good standing earns better prices.
    pub fn trade_edge(&self) -> f64 {
        1.0 + 0.002 * f64::from(self.reputation_with(self.faction))
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
        let data = &self.data;
        let fac = &data.factions[faction];
        let values = data.ethos.profile(Some(fac.id.as_str()));
        (0..data.resources.len())
            .filter(|&r| {
                fac.contraband.iter().any(|id| *id == data.resources[r].id)
                    || data.ethos.banned_by(&values, &data.resources, r).is_some()
            })
            .collect()
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
        let pay = c.reward * (1.0 - self.tax_rate());
        self.credits += pay;
        self.change_reputation(self.faction, 3.0);
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
        let pay = c.reward * comfort * (1.0 - self.tax_rate());
        self.credits += pay;
        let kind = c.pax_kind.min(3);
        self.change_reputation(self.faction, if kind == 3 { 6.0 } else { 2.0 });
        if kind >= 2 {
            if let Some(ci) = self
                .colony_at(c.dest_body)
                .filter(|_| c.dest_star == self.here)
            {
                self.colonies[ci].pop += f64::from(c.pax);
            }
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

    // ---- bank ----------------------------------------------------------------

    pub fn credit_limit(&self) -> f64 {
        let unsecured = (BASE_CREDIT_LIMIT
            + 200.0
                * self
                    .reputation
                    .iter()
                    .map(|r| f64::from(r.max(0.0)))
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
        crate::macroeco::sector(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            crate::politics::cell_of(self.here_pos),
            self.time,
        )
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
            .map(|h| {
                h.amount
                    * crate::macroeco::sector(
                        &self.data,
                        &self.galaxy,
                        self.galaxy.seed,
                        h.cell,
                        self.time,
                    )
                    .fx
            })
            .sum();
        let hold: f64 = (0..self.data.resources.len())
            .map(|r| self.ship.cargo.kg[r] * self.data.resources[r].value)
            .sum();
        self.credits + shares + forex + hold - self.bank.loan
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
        assert_eq!(s.reputation.len(), s.data.factions.len());
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
        s.reputation[0] = 0.0;
        let before = s.market(s.docked.expect("d")).clone();
        let neutral = s.sell(iron, Some(200.0)).expect("sell");
        *s.market(s.docked.expect("d")) = before;
        s.reputation[0] = 80.0;
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
        s.reputation[4] = 0.0;
        let taxed = s.sell(iron, Some(100.0)).expect("sell");
        assert!(taxed < free, "10% tax must bite: {taxed} vs {free}");
    }

    #[test]
    fn hostile_reputation_blocks_docking() {
        let mut s = docked_at_hub();
        s.undock();
        s.faction = Some(2);
        s.reputation[2] = -70.0;
        let st = s.nearest_station().expect("station").0;
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        assert!(s.try_dock().is_err());
        s.reputation[2] = -10.0;
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
            let rep0 = s.reputation[conc];
            if let Some((kg, fine)) = s.customs_check() {
                assert!(kg > 0.0 && fine >= 0.0);
                assert_eq!(s.ship.cargo.kg[gold], 0.0, "contraband confiscated");
                assert!(s.reputation[conc] < rep0);
                caught = true;
                break;
            }
        }
        assert!(
            caught,
            "an aggressive faction should catch a smuggler within a few inspections"
        );
        let rep0 = s.reputation[conc];
        assert!(s.customs_check().is_none());
        assert!(s.reputation[conc] > rep0, "clean cargo earns trust");
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
        s.reputation[0] = 10.0;
        s.time += 100.0;
        s.expire_contracts();
        assert!(s.contracts.is_empty() && s.reputation[0] < 10.0);
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
        s.reputation[0] = 0.0;
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
        assert!(s.reputation[0] < 0.0);
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
            pay > 700.0 && pay < 1300.0,
            "fare within comfort bounds: {pay}"
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
