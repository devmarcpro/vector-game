//! Companies listed on the exchange: their share price follows the world prices of what they
//! make and sell (events, region drift) plus a slow movement of their own; the player buys and
//! sells shares at a station (a 1 % fee each way) and gets a dividend every month.
use crate::bourse::{smooth_noise, world_factors, DAY};
use crate::{Event, Sim};
use vector_core::hash_coord;

/// Fee on every purchase and sale of shares.
pub const FEE: f64 = 0.01;
/// Days between two dividends.
pub const DIVIDEND_DAYS: i64 = 30;

impl Sim {
    /// A fixed place in the galaxy for company `c`: the region whose prices matter to it.
    fn company_pos(&self, c: usize) -> [f64; 3] {
        let h = |k: i64| {
            (hash_coord(self.galaxy.seed ^ 0xC0A1, c as i64, k, 5) >> 11) as f64
                / (1u64 << 53) as f64
        };
        [
            (h(0) - 0.5) * 400.0,
            (h(1) - 0.5) * 100.0,
            (h(2) - 0.5) * 400.0,
        ]
    }

    /// Price of one share of company `c` at time `t`, credits.
    pub fn company_price(&self, c: usize, t: f64) -> f64 {
        let k = &self.data.companies[c];
        let f = world_factors(&self.data, self.galaxy.seed, self.company_pos(c), t);
        let mut ln = 0.0;
        for (res, w) in &k.goods {
            if let Some(r) = self.data.resource_index(res) {
                ln += w * f[r].max(0.05).ln();
            }
        }
        let own = 0.6 * smooth_noise(self.galaxy.seed ^ 0xC0, c as i64, 17.0, t)
            + 0.4 * smooth_noise(self.galaxy.seed ^ 0xC1, c as i64, 61.0, t);
        // What befalls the company in the sectors around its headquarters.
        let hq =
            crate::incidents::share_factor(&self.data, self.galaxy.seed, c, self.company_pos(c), t);
        k.base * ln.exp() * (1.0 + k.vol * own).max(0.2) * hq
    }

    /// Daily prices of company `c` over the last `days` days, oldest first.
    pub fn company_series(&self, c: usize, days: usize) -> Vec<f64> {
        (0..days)
            .map(|d| self.company_price(c, self.time - (days - 1 - d) as f64 * DAY))
            .collect()
    }

    /// Shares of company `c` held.
    pub fn shares_of(&self, c: usize) -> f64 {
        self.shares
            .iter()
            .find(|(i, _)| *i == c)
            .map_or(0.0, |(_, n)| *n)
    }

    /// Buy `n` shares of company `c` (needs a station, a broker).
    pub fn buy_shares(&mut self, c: usize, n: f64) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if c >= self.data.companies.len() || !(1.0..=1.0e6).contains(&n) {
            return Err("quantite invalide");
        }
        let cost = self.company_price(c, self.time) * n * (1.0 + FEE);
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        match self.shares.iter_mut().find(|(i, _)| *i == c) {
            Some((_, h)) => *h += n,
            None => self.shares.push((c, n)),
        }
        Ok(cost)
    }

    /// Sell up to `n` shares of company `c`; returns what was received.
    pub fn sell_shares(&mut self, c: usize, n: f64) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        let held = self.shares_of(c);
        if held < 1.0 {
            return Err("aucune action de cette entreprise");
        }
        let n = n.min(held);
        let got = self.company_price(c, self.time) * n * (1.0 - FEE);
        self.credits += got;
        if let Some(pos) = self.shares.iter().position(|(i, _)| *i == c) {
            self.shares[pos].1 -= n;
            if self.shares[pos].1 < 1e-9 {
                self.shares.remove(pos);
            }
        }
        Ok(got)
    }

    /// Pay the monthly dividends of the shares held.
    pub(crate) fn dividend_tick(&mut self) {
        let month = (self.time / DAY).floor() as i64 / DIVIDEND_DAYS;
        let last = self.last_dividend_month;
        self.last_dividend_month = month;
        if last == i64::MIN || month <= last || self.shares.is_empty() {
            return;
        }
        let mut total = 0.0;
        for &(c, n) in &self.shares {
            let k = &self.data.companies[c];
            // Paid on the price of the month: a company doing well pays more.
            let price = self.company_price(c, self.time);
            total += n * price * k.yield_year / 12.0 * (month - last).min(3) as f64;
        }
        if total > 0.0 {
            self.credits += total;
            self.events.push(Event::MarketNews(format!(
                "Dividendes : {total:.0} CR verses par vos entreprises."
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn docked() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s.time = 100.0 * DAY;
        s
    }

    #[test]
    fn prices_are_deterministic_positive_and_move_with_what_the_company_sells() {
        let s = docked();
        for c in 0..s.data.companies.len() {
            let series = s.company_series(c, 120);
            assert_eq!(
                series,
                s.company_series(c, 120),
                "pure function of the date"
            );
            assert!(series.iter().all(|p| p.is_finite() && *p > 0.0));
            let (lo, hi) = series
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
            assert!(
                hi / lo > 1.02,
                "{} never moves: {lo}..{hi}",
                s.data.companies[c].name
            );
            assert!(
                hi / lo < 8.0,
                "{} is wild: {lo}..{hi}",
                s.data.companies[c].name
            );
        }
        // Different companies do not move together.
        let a = s.company_series(0, 60);
        let b = s.company_series(3, 60);
        assert_ne!(
            a.iter().map(|p| (*p * 10.0) as i64).collect::<Vec<_>>(),
            b.iter().map(|p| (*p * 10.0) as i64).collect::<Vec<_>>()
        );
    }

    #[test]
    fn buying_and_selling_costs_the_fees_and_needs_a_broker() {
        let mut s = docked();
        let credits = s.credits;
        let spent = s.buy_shares(2, 50.0).expect("buy");
        assert!((credits - s.credits - spent).abs() < 1e-6);
        assert_eq!(s.shares_of(2), 50.0);
        let got = s.sell_shares(2, 50.0).expect("sell");
        assert_eq!(s.shares_of(2), 0.0);
        // Same price, so a round trip loses the two fees.
        assert!(
            ((spent - got) / spent - 2.0 * FEE).abs() < 0.005,
            "{spent} vs {got}"
        );
        assert!(s.sell_shares(2, 1.0).is_err(), "nothing left");
        s.credits = 10.0;
        assert!(s.buy_shares(2, 50.0).is_err(), "no money");
        s.undock();
        assert!(s.buy_shares(2, 1.0).is_err(), "no broker in space");
    }

    #[test]
    fn dividends_are_paid_once_a_month_on_the_value_held() {
        let mut s = docked();
        s.buy_shares(0, 100.0).unwrap();
        s.dividend_tick();
        let credits = s.credits;
        // The same month: nothing.
        s.dividend_tick();
        assert_eq!(s.credits, credits);
        // A month later: the yield of a month on the price.
        s.time += (DIVIDEND_DAYS as f64 + 1.0) * DAY;
        s.dividend_tick();
        let expected = 100.0 * s.company_price(0, s.time) * s.data.companies[0].yield_year / 12.0;
        assert!(
            (s.credits - credits - expected).abs() < 1e-6,
            "{} vs {expected}",
            s.credits - credits
        );
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, Event::MarketNews(t) if t.starts_with("Dividendes"))));
    }
}
