//! Futures on the exchange: bet on where the reference price of a good will stand in a few
//! weeks. You lock a margin (a quarter of the value), the contract settles in cash at its date
//! against the reference price of the region where you signed it; losses stop at the margin.
use crate::bourse::{world_factors, DAY};
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};

/// Share of the notional value locked as margin.
pub const MARGIN_RATE: f64 = 0.25;
pub const MAX_OPEN: usize = 6;
pub const MIN_DAYS: f64 = 5.0;
pub const MAX_DAYS: f64 = 120.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Future {
    pub id: u32,
    pub res: usize,
    pub kg: f64,
    /// Bet on a rise (long) or on a fall (short).
    pub long: bool,
    /// Reference price when signed, credits per kg.
    pub strike: f64,
    /// Where the contract is signed (the price is the one of that region).
    pub pos: [f64; 3],
    pub expiry: f64,
    pub margin: f64,
}

impl Sim {
    /// Reference price of a good in the region around `pos` at time `t`, credits per kg: the
    /// usual level of the place, with the effect of the timetable of the regular lines (the
    /// stock-free reading of the convoys; the live price of a station reads its real stock,
    /// on which the convoys act through `lanes::flows_between`).
    pub fn reference_price(&self, pos: [f64; 3], t: f64, res: usize) -> f64 {
        self.data.resources[res].value
            * world_factors(&self.data, self.galaxy.seed, pos, t)[res]
            * self.shock_factor(pos, t, res)
            * crate::lanes::good_factor(&self.data, &self.galaxy, pos, t, res)
    }

    /// What closing the contract now would pay out (margin plus gain or loss, never below 0).
    pub fn future_payout(&self, f: &Future) -> f64 {
        let now = self.reference_price(f.pos, self.time.min(f.expiry), f.res);
        let pnl = (now - f.strike) * f.kg * if f.long { 1.0 } else { -1.0 };
        (f.margin + pnl).max(0.0)
    }

    /// Sign a future on `kg` of `res` ending in `days` days. Needs a trading desk: dock first.
    pub fn open_future(
        &mut self,
        res: usize,
        kg: f64,
        days: f64,
        long: bool,
    ) -> Result<u32, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if res >= self.data.resources.len() || !(1.0..=1.0e6).contains(&kg) {
            return Err("quantite invalide");
        }
        if !(MIN_DAYS..=MAX_DAYS).contains(&days) {
            return Err("echeance invalide");
        }
        if self.futures.len() >= MAX_OPEN {
            return Err("trop de contrats ouverts");
        }
        let strike = self.reference_price(self.here_pos, self.time, res);
        let margin = MARGIN_RATE * strike * kg;
        if margin > self.credits {
            return Err("fonds insuffisants pour la marge");
        }
        self.credits -= margin;
        self.next_future += 1;
        let id = self.next_future;
        self.futures.push(Future {
            id,
            res,
            kg,
            long,
            strike,
            pos: self.here_pos,
            expiry: self.time + days * DAY,
            margin,
        });
        Ok(id)
    }

    /// Close contract number `idx` now at the current reference price; returns what it paid.
    pub fn close_future(&mut self, idx: usize) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if idx >= self.futures.len() {
            return Err("contrat inconnu");
        }
        let f = self.futures.remove(idx);
        let pay = self.future_payout(&f);
        self.credits += pay;
        Ok(pay)
    }

    /// Settle the contracts that reached their date.
    pub(crate) fn futures_tick(&mut self) {
        if !self.futures.iter().any(|f| f.expiry <= self.time) {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        let mut kept = Vec::new();
        for f in std::mem::take(&mut self.futures) {
            if f.expiry > self.time {
                kept.push(f);
                continue;
            }
            let price = self.reference_price(f.pos, f.expiry, f.res);
            let pnl = (price - f.strike) * f.kg * if f.long { 1.0 } else { -1.0 };
            let pay = (f.margin + pnl).max(0.0);
            self.credits += pay;
            self.events.push(Event::MarketNews(format!(
                "Contrat a terme {} {:.0} kg de {} : {} {:.0} CR",
                if f.long { "a la hausse" } else { "a la baisse" },
                f.kg,
                data.resources[f.res].name.to_lowercase(),
                if pnl >= 0.0 { "gain" } else { "perte" },
                pnl.abs().min(f.margin.max(pnl.abs()))
            )));
        }
        self.futures = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docked() -> Sim {
        crate::test_support::docked_sim()
    }

    #[test]
    fn a_future_locks_a_margin_and_settles_in_cash_at_its_date() {
        let mut s = docked();
        let iron = s.data.resource_index("iron").unwrap();
        let credits = s.credits;
        let id = s.open_future(iron, 1000.0, 30.0, true).expect("open");
        let f = s.futures[0].clone();
        assert_eq!(f.id, id);
        let margin = MARGIN_RATE * f.strike * 1000.0;
        assert!(
            (credits - s.credits - margin).abs() < 1e-6,
            "the margin is locked"
        );
        // The payout at any moment is the margin plus the gain, never negative.
        assert!(s.future_payout(&f) >= 0.0);
        // Move past the date: it settles by itself and the contract disappears.
        let before = s.credits;
        s.time = f.expiry + 10.0;
        s.futures_tick();
        assert!(s.futures.is_empty());
        let gain = (s.reference_price(f.pos, f.expiry, f.res) - f.strike) * 1000.0;
        let expected = (margin + gain).max(0.0);
        assert!(
            (s.credits - before - expected).abs() < 1e-6,
            "{} vs {expected}",
            s.credits - before
        );
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, Event::MarketNews(t) if t.contains("Contrat a terme"))));
    }

    #[test]
    fn longs_and_shorts_are_opposite_bets_and_limits_hold() {
        let mut s = docked();
        let gas = s.data.resource_index("hydrogen").unwrap();
        s.open_future(gas, 500.0, 40.0, true).unwrap();
        s.open_future(gas, 500.0, 40.0, false).unwrap();
        let (a, b) = (s.futures[0].clone(), s.futures[1].clone());
        s.time += 20.0 * DAY;
        // Together the two bets pay back exactly the two margins: what one wins the other loses
        // (unless a loss was cut at the margin).
        let (pa, pb) = (s.future_payout(&a), s.future_payout(&b));
        let pnl = (s.reference_price(a.pos, s.time, a.res) - a.strike) * 500.0;
        if pnl.abs() < a.margin {
            assert!((pa + pb - a.margin - b.margin).abs() < 1e-6);
        }
        assert!(pa >= 0.0 && pb >= 0.0);
        // Limits.
        assert!(s.open_future(gas, 500.0, 2.0, true).is_err(), "too short");
        assert!(s.open_future(gas, 500.0, 500.0, true).is_err(), "too long");
        assert!(s.open_future(gas, 0.0, 30.0, true).is_err(), "no quantity");
        for _ in 0..8 {
            let _ = s.open_future(gas, 10.0, 30.0, true);
        }
        assert!(s.futures.len() <= MAX_OPEN, "at most {MAX_OPEN} open");
        s.credits = 1.0;
        assert!(s.open_future(gas, 500.0, 30.0, true).is_err() || s.futures.len() < MAX_OPEN);
        // Closing pays the margin plus the gain at once.
        s.credits = 0.0;
        let pay = s.close_future(0).expect("close");
        assert!((s.credits - pay).abs() < 1e-9);
    }

    #[test]
    fn a_desk_is_needed() {
        let mut s = docked();
        s.undock();
        let iron = s.data.resource_index("iron").unwrap();
        assert!(s.open_future(iron, 100.0, 30.0, true).is_err());
    }
}
