//! Places where prices are made by people rather than by the exchange: the weekly auction of a
//! station (lots with a reserve and hidden rival bidders, second-price rule: you give the most
//! you would pay and win at the price of the best rival), the flea market (small lots at wild
//! prices, some of them fakes), the counterfeits that flood a lawless sector and cheapen the
//! goods of its brands, and advertising that lifts the demand for a good around a station and
//! builds the brand of the player's trading house.
use crate::bourse::DAY;
use crate::planet::unit;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Lots of an auction week, days they stay open, days a won lot waits for its buyer.
pub const LOTS: usize = 4;
pub const OPEN_DAYS: f64 = 5.0;
pub const WAIT_DAYS: f64 = 14.0;
pub const MAX_BIDS: usize = 4;
/// Share of the reserve price over the value of the lot.
pub const RESERVE: f64 = 0.55;
/// Lots of the flea market per day, share of the goods that a fake really holds, the price of
/// an appraisal (share of the price) and the share paid when selling there.
pub const FLEA_LOTS: usize = 6;
pub const FAKE_SHARE: f64 = 0.1;
pub const APPRAISAL: f64 = 0.04;
pub const FLEA_SELL: f64 = 0.55;
/// Advertising: days a campaign runs, campaigns at once, radius it reaches (ly).
pub const AD_DAYS: f64 = 20.0;
pub const MAX_ADS: usize = 3;
pub const AD_RADIUS_LY: f64 = 12.0;
/// Days over which the brand of the trading house fades by a factor e.
pub const BRAND_DAYS: f64 = 100.0;

/// One lot of an auction.
#[derive(Clone, Debug, PartialEq)]
pub struct Lot {
    pub week: i64,
    pub index: usize,
    pub res: usize,
    pub kg: f64,
    /// What it is worth at the market of the station.
    pub value: f64,
    pub reserve: f64,
    /// The most each rival bidder would pay.
    pub rivals: Vec<f64>,
    pub close: f64,
}

/// How an auction ends for a player's top bid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// The player wins and pays this price.
    Won(f64),
    Lost,
    /// Under the reserve: nobody buys.
    Unsold,
}

/// Settle a lot against a player's top bid: second-price rule (the winner pays a little over
/// the best rival, never over what he offered, never under the reserve).
pub fn settle(lot: &Lot, my_max: f64) -> Outcome {
    let best = lot.rivals.iter().copied().fold(0.0_f64, f64::max);
    if my_max < lot.reserve {
        return Outcome::Unsold;
    }
    if my_max > best {
        Outcome::Won((best * 1.05).max(lot.reserve).min(my_max))
    } else {
        Outcome::Lost
    }
}

/// A bid the player has placed (the credits are held until the lot closes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bid {
    pub system_seed: u64,
    pub station: usize,
    pub week: i64,
    pub index: usize,
    pub res: usize,
    pub kg: f64,
    pub max_bid: f64,
    pub close: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ad {
    pub res: usize,
    pub pos: [f64; 3],
    pub start: f64,
    pub end: f64,
    pub strength: f64,
}

impl Ad {
    /// Ramp up over three days, hold, fade over the last five.
    pub fn profile(&self, t: f64) -> f64 {
        if t < self.start || t > self.end {
            return 0.0;
        }
        ((t - self.start) / (3.0 * DAY))
            .min((self.end - t) / (5.0 * DAY))
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dealings {
    pub bids: Vec<Bid>,
    pub ads: Vec<Ad>,
    /// Brand of the trading house (0..1) as of `brand_t`.
    pub brand: f64,
    pub brand_t: f64,
    /// Flea lots taken today: (day, index).
    pub flea_taken: Vec<(i64, usize)>,
    /// Flea lots appraised today: (day, index).
    pub flea_known: Vec<(i64, usize)>,
    pub last_day: i64,
}

/// A lot of the flea market.
#[derive(Clone, Debug, PartialEq)]
pub struct FleaLot {
    pub index: usize,
    pub res: usize,
    pub kg: f64,
    pub price: f64,
    /// Hidden until appraised or bought.
    pub fake: bool,
}

/// Share of the goods of a place that are copies: the lawless make them, the order stops them.
pub fn counterfeit_rate(data: &GameData, galaxy: &Galaxy, seed: u64, pos: [f64; 3], t: f64) -> f64 {
    let c = crate::politics::cell_of(pos);
    let faction = crate::politics::faction_of(data, galaxy, c);
    let base = faction.map_or(0.3, |f| 0.05 + 0.15 * (1.0 - data.factions[f].aggression));
    (base + 0.25 * crate::politics::unrest(seed, c, t)).clamp(0.0, 0.8)
}

/// Price multiplier of the goods that carry brands (the "consumer" ones) where copies flood.
pub fn counterfeit_factor(
    data: &GameData,
    galaxy: &Galaxy,
    seed: u64,
    pos: [f64; 3],
    t: f64,
    res: usize,
) -> f64 {
    if data.resources[res].category != "consumer" {
        return 1.0;
    }
    1.0 - 0.3 * counterfeit_rate(data, galaxy, seed, pos, t)
}

impl Sim {
    // ---- auctions -----------------------------------------------------------------

    /// The lots of the auction of station `station` in the week of `t`.
    pub fn auction_lots(&self, station: usize, t: f64) -> Vec<Lot> {
        let week = (t / (7.0 * DAY)).floor() as i64;
        let seed = self.system.seed;
        let pool: Vec<usize> = (0..self.data.resources.len())
            .filter(|&r| {
                let v = self.data.resources[r].value;
                (0.5..=5_000.0).contains(&v)
            })
            .collect();
        (0..LOTS)
            .map(|i| {
                let h = |k: i64| {
                    unit(hash_coord(
                        seed ^ 0xA0C7,
                        station as i64 * 64 + i as i64,
                        week,
                        k,
                    ))
                };
                let res = pool[(h(1) * pool.len() as f64) as usize % pool.len()];
                let v = self.data.resources[res].value;
                let kg = (20.0 + 3000.0 * h(2))
                    .min(60_000.0 / v.max(1.0))
                    .max(1.0)
                    .round();
                let value = kg * v;
                let n = 2 + (h(3) * 3.0) as usize;
                let rivals = (0..n)
                    .map(|r| value * (0.5 + 0.9 * h(10 + r as i64)))
                    .collect();
                Lot {
                    week,
                    index: i,
                    res,
                    kg,
                    value,
                    reserve: RESERVE * value,
                    rivals,
                    close: week as f64 * 7.0 * DAY + OPEN_DAYS * DAY,
                }
            })
            .collect()
    }

    /// Place the most you would pay on lot `index` of this week's auction. The sum is held.
    pub fn place_bid(&mut self, index: usize, max_bid: f64) -> Result<(), &'static str> {
        let st = self
            .docked_station()
            .map_err(|_| "non amarre : la vente est a la station")?;
        let lots = self.auction_lots(st, self.time);
        let lot = lots.get(index).ok_or("lot inconnu")?;
        if self.time >= lot.close {
            return Err("vente close : prochain lot lundi");
        }
        if self.trade.dealings.bids.len() >= MAX_BIDS {
            return Err("trop d'encheres en cours");
        }
        let seed = self.system.seed;
        if self.trade.dealings.bids.iter().any(|b| {
            b.system_seed == seed && b.station == st && b.week == lot.week && b.index == index
        }) {
            return Err("deja une mise sur ce lot");
        }
        if max_bid < 1.0 || max_bid > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= max_bid;
        self.trade.dealings.bids.push(Bid {
            system_seed: seed,
            station: st,
            week: lot.week,
            index,
            res: lot.res,
            kg: lot.kg,
            max_bid,
            close: lot.close,
        });
        Ok(())
    }

    /// Close the bids whose lots are done. At the station of the sale the winner takes the goods
    /// (what the hold cannot take is refunded at the price paid); a bid that waits more than
    /// two weeks is refunded less a storage fee.
    pub fn collect_auctions(&mut self) {
        let now = self.time;
        let here = self.docked_station().ok();
        let seed = self.system.seed;
        let bids = std::mem::take(&mut self.trade.dealings.bids);
        let mut kept = Vec::new();
        for b in bids {
            if b.close > now {
                kept.push(b);
                continue;
            }
            if here != Some(b.station) || seed != b.system_seed {
                if now > b.close + WAIT_DAYS * DAY {
                    self.credits += 0.9 * b.max_bid;
                    self.events.push(Event::MarketNews(
                        "Vente aux encheres : mise rendue (frais de garde 10 %).".into(),
                    ));
                } else {
                    kept.push(b);
                }
                continue;
            }
            let lots = self.auction_lots(b.station, b.week as f64 * 7.0 * DAY);
            let Some(lot) = lots.get(b.index) else {
                self.credits += b.max_bid;
                continue;
            };
            let name = self.data.resources[lot.res].name.to_lowercase();
            match settle(lot, b.max_bid) {
                Outcome::Won(price) => {
                    let data = std::sync::Arc::clone(&self.data);
                    let taken = self.ship.cargo.add(&data, lot.res, lot.kg);
                    let share = (taken / lot.kg.max(1e-9)).clamp(0.0, 1.0);
                    let paid = price * share;
                    self.credits += b.max_bid - paid;
                    self.events.push(Event::MarketNews(format!(
                        "Encheres : {taken:.0} kg de {name} remportes pour {paid:.0} CR."
                    )));
                }
                Outcome::Lost => {
                    self.credits += b.max_bid;
                    self.events.push(Event::MarketNews(format!(
                        "Encheres : un rival emporte {:.0} kg de {name}.",
                        lot.kg
                    )));
                }
                Outcome::Unsold => {
                    self.credits += b.max_bid;
                }
            }
        }
        self.trade.dealings.bids.extend(kept);
    }

    // ---- flea market --------------------------------------------------------------

    /// The lots of the flea market of the station today.
    pub fn flea_lots(&self) -> Vec<FleaLot> {
        let day = (self.time / DAY).floor() as i64;
        let seed = self.system.seed;
        let st = self.docked.unwrap_or(0);
        let rate = counterfeit_rate(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            self.here_pos,
            self.time,
        );
        let pool: Vec<usize> = (0..self.data.resources.len())
            .filter(|&r| (0.2..=800.0).contains(&self.data.resources[r].value))
            .collect();
        (0..FLEA_LOTS)
            .map(|i| {
                let h = |k: i64| unit(hash_coord(seed ^ 0xF1EA, st as i64 * 64 + i as i64, day, k));
                let res = pool[(h(1) * pool.len() as f64) as usize % pool.len()];
                let kg = (2.0 + 58.0 * h(2)).round();
                let base = self.reference_price(self.here_pos, self.time, res);
                FleaLot {
                    index: i,
                    res,
                    kg,
                    price: base * kg * (0.25 + 0.85 * h(3)),
                    fake: h(4) < rate * (0.5 + h(5)),
                }
            })
            .filter(|l| !self.flea_taken_today(l.index))
            .collect()
    }

    fn flea_taken_today(&self, index: usize) -> bool {
        let day = (self.time / DAY).floor() as i64;
        self.trade
            .dealings
            .flea_taken
            .iter()
            .any(|&(d, i)| d == day && i == index)
    }

    /// Whether the player knows lot `index` is a fake today.
    pub fn flea_known(&self, index: usize) -> bool {
        let day = (self.time / DAY).floor() as i64;
        self.trade
            .dealings
            .flea_known
            .iter()
            .any(|&(d, i)| d == day && i == index)
    }

    /// Have lot `index` appraised: costs a share of its price, and tells whether it is a fake.
    pub fn flea_appraise(&mut self, index: usize) -> Result<bool, &'static str> {
        self.docked_station()
            .map_err(|_| "non amarre : le marche est a la station")?;
        let lot = self
            .flea_lots()
            .into_iter()
            .find(|l| l.index == index)
            .ok_or("lot inconnu")?;
        let fee = (APPRAISAL * lot.price).max(1.0);
        if fee > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= fee;
        let day = (self.time / DAY).floor() as i64;
        self.trade.dealings.flea_known.push((day, index));
        Ok(lot.fake)
    }

    /// Buy lot `index`. A fake holds only a tenth of the goods (and tells so on arrival).
    /// Returns (kg received, was a fake).
    pub fn flea_buy(&mut self, index: usize) -> Result<(f64, bool), &'static str> {
        self.docked_station()
            .map_err(|_| "non amarre : le marche est a la station")?;
        let lot = self
            .flea_lots()
            .into_iter()
            .find(|l| l.index == index)
            .ok_or("lot inconnu")?;
        if lot.price > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= lot.price;
        let day = (self.time / DAY).floor() as i64;
        self.trade.dealings.flea_taken.push((day, index));
        self.trade.dealings.flea_taken.retain(|&(d, _)| d == day);
        let kg = if lot.fake {
            lot.kg * FAKE_SHARE
        } else {
            lot.kg
        };
        let data = std::sync::Arc::clone(&self.data);
        let got = self.ship.cargo.add(&data, lot.res, kg);
        if lot.fake {
            self.events.push(Event::MarketNews(format!(
                "Contrefacon ! Le lot de {} ne contenait que {got:.1} kg de vrai.",
                data.resources[lot.res].name.to_lowercase()
            )));
        }
        Ok((got, lot.fake))
    }

    /// Sell goods from the hold at the flea market: a poor price, no tax, no questions (but
    /// what the law bans is still banned).
    pub fn flea_sell(&mut self, res: usize, kg: f64) -> Result<f64, &'static str> {
        self.docked_station()
            .map_err(|_| "non amarre : le marche est a la station")?;
        let kg = kg.min(self.ship.cargo.kg[res]);
        if kg < 0.1 {
            return Err("rien de tel en soute");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, res, kg);
        let pay = FLEA_SELL * kg * self.reference_price(self.here_pos, self.time, res);
        self.credits += pay;
        if let Some(f) = self.faction {
            if self.banned_goods(f).contains(&res) {
                let _ = self.offend(Some(f), "contrebande");
            }
        }
        Ok(pay)
    }

    // ---- advertising and brand -------------------------------------------------

    /// Run a campaign for `res` around this station: it costs `budget`, lifts the demand (and so
    /// the price) of the good here for twenty days, and builds the brand of the trading house.
    pub fn run_ad(&mut self, res: usize, budget: f64) -> Result<f64, &'static str> {
        self.docked_station()
            .map_err(|_| "non amarre : l'agence est a la station")?;
        if res >= self.data.resources.len() {
            return Err("marchandise inconnue");
        }
        if budget < 200.0 || budget > self.credits {
            return Err("budget insuffisant");
        }
        let now = self.time;
        self.trade.dealings.ads.retain(|a| a.end > now);
        if self.trade.dealings.ads.len() >= MAX_ADS {
            return Err("trop de campagnes en cours");
        }
        let value = self.data.resources[res].value;
        let strength = 0.15 * (1.0 - (-budget / (value * 1500.0 + 2000.0)).exp());
        self.credits -= budget;
        self.trade.dealings.ads.push(Ad {
            res,
            pos: self.here_pos,
            start: now,
            end: now + AD_DAYS * DAY,
            strength,
        });
        let b = self.brand_at(now) + budget / 40_000.0;
        self.trade.dealings.brand = b.min(1.0);
        self.trade.dealings.brand_t = now;
        Ok(strength)
    }

    /// Price multiplier of the campaigns around `pos` for `res` at `t`.
    pub fn ad_factor(&self, pos: [f64; 3], t: f64, res: usize) -> f64 {
        self.trade
            .dealings
            .ads
            .iter()
            .filter(|a| a.res == res)
            .filter(|a| {
                let d: f64 = (0..3).map(|k| (a.pos[k] - pos[k]).powi(2)).sum::<f64>();
                d.sqrt() < AD_RADIUS_LY
            })
            .map(|a| 1.0 + a.strength * a.profile(t))
            .product()
    }

    /// Brand of the trading house at `t` (0..1): it fades unless fed.
    pub fn brand_at(&self, t: f64) -> f64 {
        let d = &self.trade.dealings;
        d.brand * (-((t - d.brand_t).max(0.0)) / (BRAND_DAYS * DAY)).exp()
    }

    /// Once a day: close the auctions that ended, forget old campaigns.
    pub(crate) fn dealings_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.trade.dealings.last_day;
        self.trade.dealings.last_day = day;
        if last == 0 || day <= last {
            return;
        }
        let now = self.time;
        self.trade.dealings.ads.retain(|a| a.end > now - 5.0 * DAY);
        if !self.trade.dealings.bids.is_empty() {
            self.collect_auctions();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;

    fn docked() -> Sim {
        let d = Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        );
        let mut s = new_sim(5, d);
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s.time = 100.0 * DAY;
        s.ship.cargo.cap_mass = 1.0e7;
        s.ship.cargo.cap_vol = 1.0e7;
        s
    }

    #[test]
    fn the_second_price_rule_wins_over_the_best_rival_and_pays_a_little_more() {
        let lot = Lot {
            week: 1,
            index: 0,
            res: 0,
            kg: 10.0,
            value: 1000.0,
            reserve: 550.0,
            rivals: vec![700.0, 900.0, 400.0],
            close: 0.0,
        };
        assert_eq!(settle(&lot, 500.0), Outcome::Unsold, "under the reserve");
        assert_eq!(settle(&lot, 800.0), Outcome::Lost, "a rival goes higher");
        match settle(&lot, 2000.0) {
            Outcome::Won(p) => assert!((p - 945.0).abs() < 1e-9, "5 % over the best rival: {p}"),
            o => panic!("{o:?}"),
        }
        // Never pays more than the bid, however thin the margin.
        match settle(&lot, 920.0) {
            Outcome::Won(p) => assert!((p - 920.0).abs() < 1e-9),
            o => panic!("{o:?}"),
        }
        // Without rivals the reserve is the price.
        let alone = Lot {
            rivals: vec![],
            ..lot.clone()
        };
        assert_eq!(settle(&alone, 600.0), Outcome::Won(550.0));
    }

    #[test]
    fn an_auction_is_deterministic_holds_the_credits_and_delivers_the_goods() {
        let mut s = docked();
        let st = s.docked_station().unwrap();
        let lots = s.auction_lots(st, s.time);
        assert_eq!(lots, s.auction_lots(st, s.time));
        assert_eq!(lots.len(), LOTS);
        assert!(lots
            .iter()
            .all(|l| l.reserve < l.value && l.rivals.len() >= 2));
        let (i, lot) = lots
            .iter()
            .enumerate()
            .next()
            .map(|(i, l)| (i, l.clone()))
            .unwrap();
        let best = lot.rivals.iter().copied().fold(0.0, f64::max);
        let c0 = s.credits;
        let bid = best * 2.0 + 1000.0;
        s.place_bid(i, bid).expect("bid");
        assert_eq!(s.credits, c0 - bid, "the sum is held");
        assert!(s.place_bid(i, bid).is_err(), "one bid per lot");
        s.ship.cargo.kg[lot.res] = 0.0;
        // Before the close nothing happens.
        s.collect_auctions();
        assert_eq!(s.trade.dealings.bids.len(), 1);
        // After it, at the station: the goods are aboard, the change comes back.
        s.time = lot.close + DAY;
        s.collect_auctions();
        assert!(s.trade.dealings.bids.is_empty());
        assert!((s.ship.cargo.kg[lot.res] - lot.kg).abs() < 1e-6);
        let price = (best * 1.05).max(lot.reserve);
        assert!(
            (s.credits - (c0 - price)).abs() < 1e-6,
            "{} vs {}",
            s.credits,
            c0 - price
        );
        // A bid under the best rival loses everything held back (refunded in full).
        s.time = 100.0 * DAY + 14.0 * DAY;
        let lots = s.auction_lots(st, s.time);
        let l = lots[0].clone();
        let rival = l.rivals.iter().copied().fold(0.0, f64::max);
        let c1 = s.credits;
        s.place_bid(0, (rival * 0.9).max(l.reserve * 1.01)).unwrap();
        s.time = l.close + DAY;
        s.collect_auctions();
        assert!(
            (s.credits - c1).abs() < 1e-6
                || (s.credits < c1 && s.ship.cargo.kg[l.res] >= l.kg - 1e-6)
        );
    }

    #[test]
    fn the_flea_market_sells_fakes_that_only_an_appraisal_reveals() {
        let mut s = docked();
        let (mut fakes, mut real) = (0, 0);
        for day in 0..200 {
            s.time = (100.0 + f64::from(day)) * DAY;
            for l in s.flea_lots() {
                if l.fake {
                    fakes += 1;
                } else {
                    real += 1;
                }
            }
        }
        assert!(fakes > 0 && real > fakes, "{fakes} fakes, {real} real");
        // Find a day with a fake and a day with a real lot; buy both.
        let mut seen = (false, false);
        for day in 0..400 {
            s.time = (400.0 + f64::from(day)) * DAY;
            for l in s.flea_lots() {
                if l.fake && !seen.0 {
                    let fee_before = s.credits;
                    assert!(s.flea_appraise(l.index).unwrap(), "appraisal says fake");
                    assert!(s.flea_known(l.index));
                    assert!(
                        fee_before - s.credits > 0.0
                            && fee_before - s.credits <= APPRAISAL * l.price + 1.0
                    );
                    s.ship.cargo.kg[l.res] = 0.0;
                    let (kg, fake) = s.flea_buy(l.index).unwrap();
                    assert!(fake && (kg - l.kg * FAKE_SHARE).abs() < 1e-6);
                    assert!(s.flea_buy(l.index).is_err(), "sold");
                    seen.0 = true;
                } else if !l.fake && !seen.1 {
                    s.ship.cargo.kg[l.res] = 0.0;
                    assert!(!s.flea_appraise(l.index).unwrap());
                    let (kg, fake) = s.flea_buy(l.index).unwrap();
                    assert!(!fake && (kg - l.kg).abs() < 1e-6);
                    seen.1 = true;
                }
            }
            if seen.0 && seen.1 {
                break;
            }
        }
        assert!(seen.0 && seen.1);
        // Selling there pays well under the market price and no tax.
        let gold = s.data.resource_index("iron").unwrap();
        s.ship.cargo.kg[gold] = 100.0;
        let ref_pay = 100.0 * s.reference_price(s.here_pos, s.time, gold);
        let c = s.credits;
        let pay = s.flea_sell(gold, 100.0).unwrap();
        assert!((pay - FLEA_SELL * ref_pay).abs() < 1e-6 && (s.credits - c - pay).abs() < 1e-9);
    }

    #[test]
    fn copies_flood_the_lawless_and_cheapen_branded_goods() {
        let d =
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data");
        let g = Galaxy::new(5);
        let consumer = (0..d.resources.len())
            .find(|&r| d.resources[r].category == "consumer")
            .unwrap();
        let metal = (0..d.resources.len())
            .find(|&r| d.resources[r].category == "metal")
            .unwrap();
        let (mut lo, mut hi) = (1.0_f64, 0.0_f64);
        for x in -6..7 {
            let p = crate::politics::cell_center([x, 0, 0]);
            let r = counterfeit_rate(&d, &g, 5, p, 10.0 * DAY);
            assert!((0.0..=0.8).contains(&r));
            lo = lo.min(r);
            hi = hi.max(r);
            assert!(
                counterfeit_factor(&d, &g, 5, p, 10.0 * DAY, consumer) <= 1.0 - 0.3 * r + 1e-12
            );
            assert_eq!(counterfeit_factor(&d, &g, 5, p, 10.0 * DAY, metal), 1.0);
        }
        assert!(hi > lo + 0.05, "{lo}..{hi}");
    }

    #[test]
    fn an_ad_lifts_the_demand_of_its_good_around_the_station_then_fades_and_builds_the_brand() {
        let mut s = docked();
        let iron = s.data.resource_index("iron").unwrap();
        let gold = s.data.resource_index("gold").unwrap();
        let t0 = s.time;
        let edge0 = s.trade_edge();
        assert_eq!(s.ad_factor(s.here_pos, t0, iron), 1.0);
        let c = s.credits;
        let strength = s.run_ad(iron, 5_000.0).expect("ad");
        assert_eq!(s.credits, c - 5_000.0);
        assert!(strength > 0.0 && strength < 0.15);
        let mid = s.ad_factor(s.here_pos, t0 + 10.0 * DAY, iron);
        assert!((mid - (1.0 + strength)).abs() < 1e-9, "{mid}");
        assert_eq!(
            s.ad_factor(s.here_pos, t0 + 10.0 * DAY, gold),
            1.0,
            "only its good"
        );
        assert_eq!(
            s.ad_factor([500.0, 0.0, 0.0], t0 + 10.0 * DAY, iron),
            1.0,
            "only around here"
        );
        assert_eq!(s.ad_factor(s.here_pos, t0 + 30.0 * DAY, iron), 1.0, "faded");
        // A bigger budget has a bigger effect, with diminishing returns.
        let s2 = {
            let mut s = docked();
            s.run_ad(iron, 20_000.0).unwrap()
        };
        assert!(s2 > strength && s2 < 4.0 * strength);
        // The brand lifts the prices of the house, and fades.
        assert!(s.trade_edge() > edge0);
        assert!(s.brand_at(t0 + 400.0 * DAY) < 0.02 * s.brand_at(t0) + 0.02);
        // The shock factor the markets read carries the ad.
        assert!((s.shock_factor(s.here_pos, t0 + 10.0 * DAY, iron) - mid).abs() < 1e-9);
    }
}
