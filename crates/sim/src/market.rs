//! Station markets: stock-driven prices with a spread. Each station has an industry that makes some
//! goods plentiful (cheap) and others scarce (expensive); stock relaxes toward that equilibrium.
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::GameData;

pub const SPREAD: f64 = 0.10;
/// Stock relaxation time constant (seconds): 3 game days.
const RELAX_TAU: f64 = 3.0 * 86_400.0;
/// Equilibrium stock multipliers (relative to the reference level) for produced / consumed goods.
pub const PRODUCE_FACTOR: f64 = 3.0;
pub const CONSUME_FACTOR: f64 = 0.15;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Market {
    pub stock: Vec<f64>,
    /// Equilibrium stock (what the station drifts toward).
    pub target: Vec<f64>,
    /// "Normal" stock used as the price reference: price = value x (reference / stock)^0.35.
    pub reference: Vec<f64>,
    pub modifier: Vec<f64>,
    /// Index into `GameData::industries`.
    pub industry: usize,
    /// Game time of the last relaxation update.
    pub updated: f64,
    /// What the world is doing to each price right now (events, drift); 1 = nothing. Derived
    /// from the date and the place (see `bourse`), so it is never saved.
    #[serde(skip)]
    pub world: Vec<f64>,
    #[serde(skip)]
    pub world_day: i64,
}

impl Market {
    pub fn new(seed: u64, station: usize, data: &GameData, time: f64, industry: usize) -> Market {
        let mut rng = Rng::new(hash_coord(seed, station as i64, 7, 13));
        let n = data.resources.len();
        let ind = &data.industries[industry % data.industries.len()];
        let mut m = Market {
            stock: vec![0.0; n],
            target: vec![0.0; n],
            reference: vec![0.0; n],
            modifier: vec![1.0; n],
            industry,
            updated: time,
            world: Vec::new(),
            world_day: i64::MIN,
        };
        for i in 0..n {
            let id = data.resources[i].id.as_str();
            // Cheap bulk goods are stocked in larger quantities.
            let value = data.resources[i].value.max(0.01);
            let mut reference = 2.0e4 / (1.0 + value).sqrt() * rng.range(0.6, 1.4);
            // Grafts and prostheses are counted in pieces, not tonnes.
            if matches!(
                data.resources[i].category.as_str(),
                "organes" | "protheses" | "cybernetique" | "equipement"
            ) {
                reference = rng.range(1.5, 5.0);
            }
            let factor = if ind.produces.iter().any(|p| p == id) {
                PRODUCE_FACTOR
            } else if ind.consumes.iter().any(|p| p == id) {
                CONSUME_FACTOR
            } else {
                1.0
            };
            m.reference[i] = reference;
            m.target[i] = reference * factor;
            m.modifier[i] = rng.range(0.85, 1.2);
            m.stock[i] = m.target[i] * rng.range(0.6, 1.4);
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

    /// Price multiplier of the scarcity of `res` when the stock stands at `stock`: the one law
    /// of the stock-driven price (the market and the exchange's reference curve both use it).
    pub fn scarcity_at(&self, res: usize, stock: f64) -> f64 {
        (self.reference[res] / stock.max(0.03 * self.reference[res]))
            .powf(0.35)
            .clamp(0.25, 4.0)
    }

    /// Mid price (credits/kg).
    pub fn price(&self, data: &GameData, res: usize) -> f64 {
        data.resources[res].value
            * self.modifier[res]
            * self.scarcity_at(res, self.stock[res])
            * self.world.get(res).copied().unwrap_or(1.0)
    }
    pub fn bid(&self, data: &GameData, res: usize) -> f64 {
        self.price(data, res) * (1.0 - SPREAD / 2.0)
    }
    pub fn ask(&self, data: &GameData, res: usize) -> f64 {
        self.price(data, res) * (1.0 + SPREAD / 2.0)
    }

    /// Player sells `kg`; price slides as stock grows (integrated in chunks). `edge` scales the price
    /// (reputation, tax). Returns credits earned.
    pub fn sell(&mut self, data: &GameData, res: usize, kg: f64, edge: f64) -> f64 {
        let chunks = 20;
        let part = kg / chunks as f64;
        let mut total = 0.0;
        for _ in 0..chunks {
            total += self.bid(data, res) * part * edge;
            self.stock[res] += part;
        }
        total
    }

    /// Player buys up to `kg` (limited by stock). `edge` scales the price. Returns (kg bought, cost).
    pub fn buy(&mut self, data: &GameData, res: usize, kg: f64, edge: f64) -> (f64, f64) {
        let kg = kg.min(self.stock[res]).max(0.0);
        let chunks = 20;
        let part = kg / chunks as f64;
        let mut total = 0.0;
        for _ in 0..chunks {
            total += self.ask(data, res) * part / edge.max(0.1);
            self.stock[res] -= part;
        }
        (kg, total)
    }

    /// Relative scarcity of a resource: < 1 means the station is short of it.
    pub fn fullness(&self, res: usize) -> f64 {
        self.stock[res] / self.reference[res].max(1e-9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        crate::test_support::load()
    }

    #[test]
    fn buy_then_sell_loses_money() {
        let d = data();
        let mut m = Market::new(1, 5, &d, 0.0, 0);
        let iron = d.resource_index("iron").expect("iron");
        let (kg, cost) = m.buy(&d, iron, 500.0, 1.0);
        let back = m.sell(&d, iron, kg, 1.0);
        assert!(
            back < cost,
            "round trip must lose to the spread: {back} vs {cost}"
        );
    }

    #[test]
    fn price_falls_as_stock_rises_and_relaxes_back() {
        let d = data();
        let mut m = Market::new(2, 3, &d, 0.0, 0);
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
        for ind in 0..d.industries.len() {
            let m = Market::new(3, 9, &d, 0.0, ind);
            for i in 0..d.resources.len() {
                let p = m.price(&d, i);
                assert!(p.is_finite() && p > 0.0 && p < d.resources[i].value * 6.0);
            }
        }
    }

    #[test]
    fn industries_create_regional_price_gaps() {
        // A mining colony sells iron cheap and food dear; a refinery pays well for iron; an agri dome is rich in food.
        let d = data();
        let find = |id: &str| {
            d.industries
                .iter()
                .position(|i| i.id == id)
                .expect("industry")
        };
        let (mine, refinery, agri) = (find("mining_colony"), find("refinery"), find("agri_dome"));
        let (iron, food) = (
            d.resource_index("iron").expect("iron"),
            d.resource_index("food").expect("food"),
        );
        let (mut gap_iron, mut gap_food) = (0.0, 0.0);
        for seed in 0..20u64 {
            let (mut a, mut r, mut g) = (
                Market::new(seed, 1, &d, 0.0, mine),
                Market::new(seed, 2, &d, 0.0, refinery),
                Market::new(seed, 3, &d, 0.0, agri),
            );
            for m in [&mut a, &mut r, &mut g] {
                m.relax_to(1.0e8);
            }
            gap_iron += a.price(&d, iron) / r.price(&d, iron);
            gap_food += a.price(&d, food) / g.price(&d, food);
        }
        assert!(
            gap_iron / 20.0 < 0.5,
            "iron should be much cheaper at the mine than at the refinery: {}",
            gap_iron / 20.0
        );
        assert!(
            gap_food / 20.0 > 2.0,
            "food should be much dearer at the mine than at the farm: {}",
            gap_food / 20.0
        );
    }

    #[test]
    fn fullness_tracks_shortage() {
        let d = data();
        let mut m = Market::new(4, 1, &d, 0.0, 0);
        let r = d.resource_index("water").expect("water");
        m.stock[r] = m.reference[r] * 0.1;
        assert!(m.fullness(r) < 0.2);
    }
}
