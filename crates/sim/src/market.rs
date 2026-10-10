//! Station markets: stock-driven prices with a spread; stock relaxes toward a target.
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::GameData;

pub const SPREAD: f64 = 0.10;
/// Stock relaxation time constant (seconds): 3 game days.
const RELAX_TAU: f64 = 3.0 * 86_400.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Market {
    pub stock: Vec<f64>,
    pub target: Vec<f64>,
    pub modifier: Vec<f64>,
    /// Game time of the last relaxation update.
    pub updated: f64,
}

impl Market {
    pub fn new(seed: u64, station: usize, data: &GameData, time: f64) -> Market {
        let mut rng = Rng::new(hash_coord(seed, station as i64, 7, 13));
        let n = data.resources.len();
        let mut m = Market {
            stock: vec![0.0; n],
            target: vec![0.0; n],
            modifier: vec![1.0; n],
            updated: time,
        };
        for i in 0..n {
            // Cheap bulk goods are stocked in larger quantities.
            let value = data.resources[i].value.max(0.01);
            let base = 2.0e4 / (1.0 + value).sqrt();
            m.target[i] = base * rng.range(0.4, 1.6);
            m.modifier[i] = rng.range(0.75, 1.35);
            m.stock[i] = m.target[i] * rng.range(0.3, 1.7);
        }
        m
    }

    /// Analytic catch-up: stock decays toward target regardless of elapsed time.
    pub fn relax_to(&mut self, time: f64) {
        let dt = (time - self.updated).max(0.0);
        let k = 1.0 - (-dt / RELAX_TAU).exp();
        for (s, t) in self.stock.iter_mut().zip(&self.target) {
            *s += (t - *s) * k;
        }
        self.updated = time;
    }

    /// Mid price (credits/kg).
    pub fn price(&self, data: &GameData, res: usize) -> f64 {
        let scarcity = (self.target[res] / self.stock[res].max(0.05 * self.target[res])).powf(0.35);
        data.resources[res].value * self.modifier[res] * scarcity.clamp(0.25, 4.0)
    }
    pub fn bid(&self, data: &GameData, res: usize) -> f64 {
        self.price(data, res) * (1.0 - SPREAD / 2.0)
    }
    pub fn ask(&self, data: &GameData, res: usize) -> f64 {
        self.price(data, res) * (1.0 + SPREAD / 2.0)
    }

    /// Player sells `kg`; price slides as stock grows (integrated in chunks). Returns credits earned.
    pub fn sell(&mut self, data: &GameData, res: usize, kg: f64) -> f64 {
        let chunks = 20;
        let part = kg / chunks as f64;
        let mut total = 0.0;
        for _ in 0..chunks {
            total += self.bid(data, res) * part;
            self.stock[res] += part;
        }
        total
    }

    /// Player buys up to `kg` (limited by stock). Returns (kg bought, credits cost).
    pub fn buy(&mut self, data: &GameData, res: usize, kg: f64) -> (f64, f64) {
        let kg = kg.min(self.stock[res]).max(0.0);
        let chunks = 20;
        let part = kg / chunks as f64;
        let mut total = 0.0;
        for _ in 0..chunks {
            total += self.ask(data, res) * part;
            self.stock[res] -= part;
        }
        (kg, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn data() -> GameData {
        GameData::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).expect("data")
    }

    #[test]
    fn buy_then_sell_loses_money() {
        let d = data();
        let mut m = Market::new(1, 5, &d, 0.0);
        let iron = d.resource_index("iron").expect("iron");
        let (kg, cost) = m.buy(&d, iron, 500.0);
        let back = m.sell(&d, iron, kg);
        assert!(
            back < cost,
            "round trip must lose to the spread: {back} vs {cost}"
        );
    }

    #[test]
    fn price_falls_as_stock_rises_and_relaxes_back() {
        let d = data();
        let mut m = Market::new(2, 3, &d, 0.0);
        let r = d.resource_index("copper").expect("copper");
        let p0 = m.price(&d, r);
        m.stock[r] += m.target[r] * 5.0;
        assert!(m.price(&d, r) < p0);
        m.relax_to(60.0 * 86_400.0);
        assert!((m.stock[r] - m.target[r]).abs() / m.target[r] < 0.01);
    }

    #[test]
    fn prices_are_sane_across_resources() {
        let d = data();
        let m = Market::new(3, 9, &d, 0.0);
        for i in 0..d.resources.len() {
            let p = m.price(&d, i);
            assert!(p.is_finite() && p > 0.0 && p < d.resources[i].value * 6.0);
        }
    }
}
