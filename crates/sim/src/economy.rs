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
/// Contract offers refresh every week of game time.
pub const CONTRACT_EPOCH: f64 = 7.0 * DAY;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bank {
    pub loan: f64,
    pub last_day: i64,
    pub overdue_days: u32,
    pub total_interest: f64,
}

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

    /// Contraband inspection at the docks. Returns (kg seized, fine) when something was found.
    pub fn customs_check(&mut self) -> Option<(f64, f64)> {
        let f = self.faction?;
        let fac = self.data.factions[f].clone();
        let data = std::sync::Arc::clone(&self.data);
        let held: Vec<usize> = fac
            .contraband
            .iter()
            .filter_map(|id| data.resource_index(id))
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
        BASE_CREDIT_LIMIT
            + 200.0
                * self
                    .reputation
                    .iter()
                    .map(|r| f64::from(r.max(0.0)))
                    .sum::<f64>()
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
        self.bank.loan *= (1.0 + BANK_RATE_PER_DAY).powi(days as i32);
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
}
