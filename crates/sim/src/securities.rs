//! Securities beyond the shares and the futures of the exchange: sovereign bonds (zero-coupon,
//! in the currency of the sector that issues them: the yield is its policy rate plus a risk
//! premium, an early sale is at the market), index funds (a basket of the listed companies or of
//! the goods prices, with a fee) and options on goods (a premium buys the right to the gain of a
//! rise or a fall, never a loss). All values are functions of the date, so the net worth of the
//! player counts them (futures too) at market.
use crate::bourse::{smooth_noise, world_factors, DAY};
use crate::macroeco::{CASCADE_DEPTH, FX_FEE};
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};

/// Days to maturity of an issue of bonds.
pub const BOND_DAYS: f64 = 180.0;
/// Fee of a sale or a purchase of bonds, funds.
pub const FEE: f64 = 0.005;
/// What a defaulted bond still pays.
pub const RECOVERY: f64 = 0.4;
/// Yearly management fee of a fund, charged daily in units.
pub const FUND_FEE: f64 = 0.008;
pub const MAX_OPTIONS: usize = 6;
pub const MAX_BONDS: usize = 6;
pub const OPTION_MIN_DAYS: f64 = 5.0;
pub const OPTION_MAX_DAYS: f64 = 120.0;
/// Funds on offer: (name, what it holds).
pub const FUNDS: [(&str, &str); 2] = [
    (
        "FONDS ACTIONS",
        "TOUTES LES ENTREPRISES COTEES, A POIDS EGAL",
    ),
    (
        "FONDS MATIERES",
        "LES PRIX DES MARCHANDISES DE LA REGION D'ORIGINE",
    ),
];

/// A zero-coupon bond of a sector: `units` of its currency due at maturity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    pub cell: [i64; 3],
    pub units: f64,
    pub issued: f64,
    pub maturity: f64,
    pub paid: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionC {
    pub id: u32,
    pub res: usize,
    pub kg: f64,
    pub call: bool,
    pub strike: f64,
    pub pos: [f64; 3],
    pub expiry: f64,
    pub premium: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Portfolio {
    pub bonds: Vec<Bond>,
    /// Units of each fund held (index into `FUNDS`).
    pub funds: Vec<f64>,
    pub options: Vec<OptionC>,
    pub next_id: u32,
    pub last_day: i64,
}

/// Value in credits of `units` of a bond with `rem_days` to run, at yield `y` (annual): the
/// debt discounted at the market yield.
pub fn bond_value(units: f64, fx: f64, y: f64, rem_days: f64) -> f64 {
    units * fx / (1.0 + y).powf((rem_days / 365.0).max(0.0))
}

/// Monthly volatility of a good (the exchange's own figure).
pub fn volatility(value: f64) -> f64 {
    0.03 + 0.012 * (1.0 + value).log10()
}

/// Premium per kg of an at-the-money option on a good of reference price `price` and `days`.
pub fn option_premium_per_kg(value: f64, price: f64, days: f64) -> f64 {
    0.8 * volatility(value) * price * (days / 30.0).sqrt()
}

impl Sim {
    // ---- bonds ----------------------------------------------------------------

    /// Buy bonds of the sector the ship is in with `credits` credits: they come due in six
    /// months at par in the local currency, bought at a discount that is the yield.
    pub fn buy_bond(&mut self, credits: f64) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if self.trade.folio.bonds.len() >= MAX_BONDS {
            return Err("trop d'obligations");
        }
        if !(100.0..=self.credits).contains(&credits) {
            return Err("montant invalide ou fonds insuffisants");
        }
        let s = self.local_sector();
        let cell = crate::politics::cell_of(self.here_pos);
        let disc = (1.0 + s.sovereign_yield()).powf(-BOND_DAYS / 365.0);
        let units = credits * (1.0 - FEE) / (s.fx * disc);
        self.credits -= credits;
        self.trade.folio.bonds.push(Bond {
            cell,
            units,
            issued: self.time,
            maturity: self.time + BOND_DAYS * DAY,
            paid: credits,
        });
        Ok(units)
    }

    /// What a bond is worth now at market.
    pub fn bond_market_value(&self, b: &Bond) -> f64 {
        let s = self.sector_at(b.cell);
        let mut v = bond_value(
            b.units,
            s.fx,
            s.sovereign_yield(),
            (b.maturity - self.time) / DAY,
        );
        if self.bond_defaults(b, self.time) {
            v *= RECOVERY;
        }
        v
    }

    /// Whether the sector that issued the bond cannot pay at `t`: a deep crash on top of a
    /// debt bigger than a year of output.
    pub fn bond_defaults(&self, b: &Bond, t: f64) -> bool {
        let s = crate::macroeco::sector(&self.data, &self.galaxy, self.galaxy.seed, b.cell, t);
        s.crash > CASCADE_DEPTH && s.debt_ratio > 1.0
    }

    /// Sell bond number `idx` at the market (with a fee).
    pub fn sell_bond(&mut self, idx: usize) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if idx >= self.trade.folio.bonds.len() {
            return Err("obligation inconnue");
        }
        let b = self.trade.folio.bonds.remove(idx);
        let got = self.bond_market_value(&b) * (1.0 - FEE);
        self.credits += got;
        Ok(got)
    }

    // ---- funds ----------------------------------------------------------------

    /// Price of one unit of fund `kind` at `t` (100 at the beginning of time).
    pub fn fund_price(&self, kind: usize, t: f64) -> f64 {
        match kind {
            0 => {
                let n = self.data.companies.len().max(1);
                100.0
                    * (0..self.data.companies.len())
                        .map(|c| self.company_price(c, t) / self.company_price(c, 0.0).max(1e-9))
                        .sum::<f64>()
                    / n as f64
            }
            _ => {
                let f = world_factors(&self.data, self.galaxy.seed, [0.0; 3], t);
                let mut ln = 0.0;
                let mut n: f64 = 0.0;
                for (r, v) in f.iter().enumerate() {
                    if self.data.resources[r].value >= 1.0 {
                        ln += v.max(0.05).ln();
                        n += 1.0;
                    }
                }
                100.0
                    * (ln / n.max(1.0)).exp()
                    * (1.0 + 0.02 * smooth_noise(self.galaxy.seed, 7, 45.0, t))
            }
        }
    }

    pub fn fund_units(&self, kind: usize) -> f64 {
        self.trade.folio.funds.get(kind).copied().unwrap_or(0.0)
    }

    /// Buy `credits` worth of fund `kind`; returns the units.
    pub fn buy_fund(&mut self, kind: usize, credits: f64) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if kind >= FUNDS.len() {
            return Err("fonds inconnu");
        }
        if !(100.0..=self.credits).contains(&credits) {
            return Err("montant invalide ou fonds insuffisants");
        }
        let units = credits * (1.0 - FEE) / self.fund_price(kind, self.time);
        self.credits -= credits;
        let f = &mut self.trade.folio.funds;
        f.resize(FUNDS.len(), 0.0);
        f[kind] += units;
        Ok(units)
    }

    /// Sell all the units of fund `kind`.
    pub fn sell_fund(&mut self, kind: usize) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        let u = self.fund_units(kind);
        if u <= 0.0 {
            return Err("aucune part de ce fonds");
        }
        let got = u * self.fund_price(kind, self.time) * (1.0 - FEE);
        self.trade.folio.funds[kind] = 0.0;
        self.credits += got;
        Ok(got)
    }

    // ---- options ----------------------------------------------------------------

    /// Buy an option on `kg` of `res`, struck at today's price, valid `days` days: a call
    /// gains when the price rises, a put when it falls; the premium is all that can be lost.
    pub fn buy_option(
        &mut self,
        res: usize,
        kg: f64,
        days: f64,
        call: bool,
    ) -> Result<u32, &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if res >= self.data.resources.len() || !(1.0..=1.0e6).contains(&kg) {
            return Err("quantite invalide");
        }
        if !(OPTION_MIN_DAYS..=OPTION_MAX_DAYS).contains(&days) {
            return Err("echeance invalide");
        }
        if self.trade.folio.options.len() >= MAX_OPTIONS {
            return Err("trop d'options ouvertes");
        }
        let price = self.reference_price(self.here_pos, self.time, res);
        let premium = option_premium_per_kg(self.data.resources[res].value, price, days) * kg;
        if premium > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= premium;
        self.trade.folio.next_id += 1;
        let id = self.trade.folio.next_id;
        let pos = self.here_pos;
        self.trade.folio.options.push(OptionC {
            id,
            res,
            kg,
            call,
            strike: price,
            pos,
            expiry: self.time + days * DAY,
            premium,
        });
        Ok(id)
    }

    /// What the option pays at its end for a price of `price`.
    pub fn option_payoff(o: &OptionC, price: f64) -> f64 {
        let d = if o.call {
            price - o.strike
        } else {
            o.strike - price
        };
        d.max(0.0) * o.kg
    }

    /// Value of an option now: what it would pay at today's price plus the chance left.
    pub fn option_value(&self, o: &OptionC) -> f64 {
        let price = self.reference_price(o.pos, self.time.min(o.expiry), o.res);
        let intrinsic = Self::option_payoff(o, price);
        let rem = ((o.expiry - self.time) / DAY).max(0.0);
        intrinsic + 0.9 * option_premium_per_kg(self.data.resources[o.res].value, price, rem) * o.kg
    }

    // ---- the whole folio ------------------------------------------------------

    /// What the bonds, funds, options and futures are worth now at market.
    pub fn portfolio_value(&self) -> f64 {
        let bonds: f64 = self
            .trade
            .folio
            .bonds
            .iter()
            .map(|b| self.bond_market_value(b))
            .sum();
        let funds: f64 = (0..FUNDS.len())
            .map(|k| self.fund_units(k) * self.fund_price(k, self.time))
            .sum();
        let options: f64 = self
            .trade
            .folio
            .options
            .iter()
            .map(|o| self.option_value(o))
            .sum();
        let futures: f64 = self.futures.iter().map(|f| self.future_payout(f)).sum();
        bonds + funds + options + futures
    }

    /// Once a day: the funds charge their fee, bonds and options that reached their date pay.
    pub(crate) fn securities_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.trade.folio.last_day;
        self.trade.folio.last_day = day;
        if last == 0 || day <= last {
            return;
        }
        let days = (day - last).min(3650);
        for u in &mut self.trade.folio.funds {
            *u *= (1.0 - FUND_FEE / 365.0).powi(days as i32);
        }
        if self
            .trade
            .folio
            .bonds
            .iter()
            .any(|b| b.maturity <= self.time)
        {
            let (due, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.trade.folio.bonds)
                .into_iter()
                .partition(|b| b.maturity <= self.time);
            self.trade.folio.bonds = kept;
            for b in due {
                let s = crate::macroeco::sector(
                    &self.data,
                    &self.galaxy,
                    self.galaxy.seed,
                    b.cell,
                    b.maturity,
                );
                let default = self.bond_defaults(&b, b.maturity);
                let pay = b.units * s.fx * if default { RECOVERY } else { 1.0 };
                self.credits += pay;
                self.events.push(Event::MarketNews(format!(
                    "Obligation echue : {pay:.0} CR ({}).",
                    if default { "defaut partiel" } else { "payee" }
                )));
            }
        }
        if self
            .trade
            .folio
            .options
            .iter()
            .any(|o| o.expiry <= self.time)
        {
            let (due, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.trade.folio.options)
                .into_iter()
                .partition(|o| o.expiry <= self.time);
            self.trade.folio.options = kept;
            for o in due {
                let price = self.reference_price(o.pos, o.expiry, o.res);
                let pay = Self::option_payoff(&o, price);
                self.credits += pay;
                self.events.push(Event::MarketNews(format!(
                    "Option {} sur {} : {pay:.0} CR (prime {:.0}).",
                    if o.call { "d'achat" } else { "de vente" },
                    self.data.resources[o.res].name.to_lowercase(),
                    o.premium
                )));
            }
        }
    }
}

/// The change of an exchange of `credits` for a fund: units bought less the fee, for tests and
/// the screen.
pub fn after_fee(credits: f64) -> f64 {
    credits * (1.0 - FEE)
}

/// The fee of a currency exchange, re-exported next to the other fees of the folio.
pub const FX: f64 = FX_FEE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;

    fn docked() -> Sim {
        let d = Arc::new(
            vector_data::GameData::load(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            )
            .expect("data"),
        );
        let mut s = new_sim(5, d);
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s.time = 100.0 * DAY;
        s
    }

    #[test]
    fn a_bond_loses_value_when_the_yield_rises_and_pays_par_at_maturity() {
        // The discounting: same units, higher yield, lower price; at maturity no discount.
        assert!(bond_value(1000.0, 1.0, 0.10, 180.0) < bond_value(1000.0, 1.0, 0.05, 180.0));
        assert!((bond_value(1000.0, 1.0, 0.30, 0.0) - 1000.0).abs() < 1e-9);
        assert!((bond_value(1000.0, 2.0, 0.30, 0.0) - 2000.0).abs() < 1e-9);
        let mut s = docked();
        let c0 = s.credits;
        let units = s.buy_bond(10_000.0).expect("bought");
        assert!(units > 0.0 && s.credits == c0 - 10_000.0);
        // Right after the purchase it is worth what was paid less the fee (same date).
        let v = s.bond_market_value(&s.trade.folio.bonds[0].clone());
        assert!((v - 10_000.0 * (1.0 - FEE)).abs() < 1.0, "{v}");
        // It is a position: net worth counts it.
        assert!(s.portfolio_value() > 9_000.0);
        // Sold at once it costs the fee on both sides.
        let got = s.sell_bond(0).expect("sold");
        assert!(got < 10_000.0 && got > 9_800.0, "{got}");
        // Held to maturity it pays units x fx unless the sector defaults.
        s.buy_bond(10_000.0).unwrap();
        let b = s.trade.folio.bonds[0].clone();
        s.trade.folio.last_day = (s.time / DAY) as i64;
        let before = s.credits;
        s.time = b.maturity + DAY;
        s.securities_tick();
        assert!(s.trade.folio.bonds.is_empty());
        let fx = crate::macroeco::sector(&s.data, &s.galaxy, s.galaxy.seed, b.cell, b.maturity).fx;
        let k = if s.bond_defaults(&b, b.maturity) {
            RECOVERY
        } else {
            1.0
        };
        assert!((s.credits - before - b.units * fx * k).abs() < 1e-6);
    }

    #[test]
    fn a_fund_follows_its_basket_and_its_fee_eats_units() {
        let mut s = docked();
        let p0 = s.fund_price(0, 0.0);
        assert!((p0 - 100.0).abs() < 1e-6, "base 100, got {p0}");
        let q = s.fund_price(0, s.time);
        // The index is the mean of the ratios of the companies' prices.
        let n = s.data.companies.len();
        let mean = (0..n)
            .map(|c| s.company_price(c, s.time) / s.company_price(c, 0.0))
            .sum::<f64>()
            / n as f64;
        assert!((q - 100.0 * mean).abs() < 1e-9);
        let units = s.buy_fund(0, 5_000.0).expect("bought");
        assert!((units * q - after_fee(5_000.0)).abs() < 1e-6);
        s.trade.folio.last_day = (s.time / DAY) as i64;
        s.time += 365.0 * DAY;
        s.securities_tick();
        // One year of fees: 0.8 % of the units.
        assert!((s.fund_units(0) / units - (1.0 - FUND_FEE / 365.0).powi(365)).abs() < 1e-9);
        assert!(s.sell_fund(1).is_err(), "nothing held in the other fund");
        assert!(s.sell_fund(0).unwrap() > 0.0 && s.fund_units(0) == 0.0);
    }

    #[test]
    fn an_option_costs_its_premium_pays_the_gain_and_never_a_loss() {
        let mut s = docked();
        let gold = s.data.resource_index("gold").unwrap();
        let (c0, p) = (s.credits, s.reference_price(s.here_pos, s.time, gold));
        let id = s.buy_option(gold, 100.0, 30.0, true).expect("call");
        let o = s.trade.folio.options[0].clone();
        assert_eq!(o.id, id);
        assert!((c0 - s.credits - o.premium).abs() < 1e-6);
        assert!((o.strike - p).abs() < 1e-9);
        // The longer the option, the dearer; a call and a put at the money cost the same.
        let q60 = option_premium_per_kg(s.data.resources[gold].value, p, 60.0);
        let q30 = option_premium_per_kg(s.data.resources[gold].value, p, 30.0);
        assert!(q60 > q30 && (q60 / q30 - 2.0_f64.sqrt()).abs() < 1e-9);
        // Payoff: a call pays the rise only, a put the fall only, never negative.
        assert_eq!(Sim::option_payoff(&o, o.strike * 0.5), 0.0);
        assert!((Sim::option_payoff(&o, o.strike + 1.0) - 100.0).abs() < 1e-9);
        let put = OptionC {
            call: false,
            ..o.clone()
        };
        assert_eq!(Sim::option_payoff(&put, o.strike + 1.0), 0.0);
        assert!((Sim::option_payoff(&put, o.strike - 2.0) - 200.0).abs() < 1e-9);
        // The expiry settles in cash at the price of that day; credits never fall below
        // what was left after the premium.
        s.trade.folio.last_day = (s.time / DAY) as i64;
        let after_premium = s.credits;
        s.time = o.expiry + DAY;
        s.securities_tick();
        assert!(s.trade.folio.options.is_empty());
        let end = s.reference_price(o.pos, o.expiry, gold);
        assert!((s.credits - after_premium - Sim::option_payoff(&o, end)).abs() < 1e-6);
        assert!(s.credits >= after_premium);
        // A valid option counts in the net worth, expired ones do not.
        s.buy_option(gold, 10.0, 30.0, false).unwrap();
        assert!(s.portfolio_value() > 0.0);
    }

    #[test]
    fn futures_count_in_the_net_worth() {
        let mut s = docked();
        let iron = s.data.resource_index("iron").unwrap();
        let w0 = s.net_worth();
        s.open_future(iron, 1000.0, 30.0, true).expect("future");
        // The margin left the credits and came back in the folio: no instant loss or gain.
        let w1 = s.net_worth();
        assert!((w1 - w0).abs() < 1e-6, "{w0} vs {w1}");
        assert!(s.portfolio_value() > 0.0);
    }
}
