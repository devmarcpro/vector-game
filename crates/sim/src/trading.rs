//! More ways to play the exchange (E7): rumours that announce events before they happen (some
//! are false), arbitrage between nearby systems, cargo insurance, cartels that hold prices up (and
//! can be joined, at a risk) and tenders where you bid against rivals to supply a good.
use crate::bourse::{events_between, DAY, WEEK};
use crate::planet::unit;
use crate::Event;
use crate::Sim;
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_data::GameData;

/// How far ahead rumours reach.
pub const RUMOUR_DAYS: f64 = 14.0;
/// Share of weeks that spread a false rumour.
pub const P_FALSE: f64 = 0.3;
/// Share of real events that are rumoured in advance.
pub const P_LEAK: f64 = 0.8;
/// Insurance: premium share of the covered value, cover share, duration.
pub const PREMIUM: f64 = 0.02;
pub const COVER: f64 = 0.7;
pub const INSURANCE_DAYS: f64 = 30.0;
/// Cartel seat price, monthly payout at the base markup (0.25), antitrust fine and its monthly odds.
pub const SEAT_PRICE: f64 = 4000.0;
pub const SEAT_PAYOUT: f64 = 2500.0;
pub const ANTITRUST_FINE: f64 = 3000.0;
pub const ANTITRUST_ODDS: f64 = 0.10;
pub const MAX_SEATS: usize = 3;
/// Share of months a cartel is active in a region.
pub const P_CARTEL: f64 = 0.4;
pub const CARTEL_MONTH: f64 = 30.0 * DAY;
/// Tenders: days to deliver, how many are offered at once, how many can be held.
pub const TENDER_DAYS: f64 = 20.0;
pub const TENDERS_OFFERED: usize = 3;
pub const MAX_TENDERS: usize = 4;
const TENDER_EPOCH: f64 = 10.0 * DAY;

#[derive(Clone, Debug, PartialEq)]
pub struct Rumour {
    pub def: usize,
    pub start: f64,
    /// How reliable the source looks, 0..1 (it is not a promise).
    pub confidence: f64,
    /// Whether it will really happen (hidden from the player).
    pub real: bool,
}

/// Rumours heard at `pos` at time `now`: events to come within `RUMOUR_DAYS`.
pub fn rumours(data: &GameData, seed: u64, pos: [f64; 3], now: f64) -> Vec<Rumour> {
    let end = now + RUMOUR_DAYS * DAY;
    let mut out = Vec::new();
    for ev in events_between(data, seed, pos, now, end) {
        if ev.start > now
            && unit(hash_coord(seed ^ 0x2AB0, ev.start as i64, ev.def as i64, 1)) < P_LEAK
        {
            let u = unit(hash_coord(seed ^ 0x2AB1, ev.start as i64, ev.def as i64, 2));
            out.push(Rumour {
                def: ev.def,
                start: ev.start,
                confidence: 0.55 + 0.35 * u,
                real: true,
            });
        }
    }
    let cell = pos.map(|v| (v / 60.0).floor() as i64);
    let (w0, w1) = ((now / WEEK).floor() as i64, (end / WEEK).floor() as i64);
    for w in w0..=w1 {
        let h = |k: i64| hash_coord(seed ^ 0xFA15, w, cell[0] * 31 + cell[2], k + cell[1]);
        if unit(h(0)) >= P_FALSE || data.world_events.is_empty() {
            continue;
        }
        let def = (h(1) % data.world_events.len() as u64) as usize;
        let start = w as f64 * WEEK + unit(h(2)) * WEEK;
        if start > now && start <= end {
            out.push(Rumour {
                def,
                start,
                confidence: 0.25 + 0.45 * unit(h(3)),
                real: false,
            });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// Whether cartel `c` holds prices up in the region around `pos` at time `t`.
pub fn cartel_active(seed: u64, c: usize, pos: [f64; 3], t: f64) -> bool {
    let cell = pos.map(|v| (v / 60.0).floor() as i64);
    let month = (t / CARTEL_MONTH).floor() as i64;
    unit(hash_coord(
        seed ^ 0xCA27 ^ (c as u64).wrapping_mul(0x9E37_79B9),
        cell[0] + month * 7,
        cell[1] - month,
        cell[2] + c as i64,
    )) < P_CARTEL
}

/// Price multiplier of the cartels on resource `res` at `pos`, `t`.
pub fn cartel_factor(data: &GameData, seed: u64, pos: [f64; 3], t: f64, res: usize) -> f64 {
    let mut f = 1.0;
    for (c, k) in data.cartels.iter().enumerate() {
        if k.goods.iter().any(|g| data.resource_index(g) == Some(res))
            && cartel_active(seed, c, pos, t)
        {
            f *= 1.0 + k.markup;
        }
    }
    f
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tender {
    pub id: u64,
    pub system_seed: u64,
    pub station: usize,
    pub res: usize,
    pub kg: f64,
    pub price: f64,
    pub deadline: f64,
}

/// A tender on offer (not yet won).
#[derive(Clone, Debug, PartialEq)]
pub struct TenderOffer {
    pub id: u64,
    pub res: usize,
    pub kg: f64,
    /// The most the buyer will pay per kg.
    pub ceiling: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TradeState {
    /// Insurance: end date and covered value.
    pub insured_until: f64,
    pub covered: f64,
    /// Cartel seats held, and the month last settled.
    pub seats: Vec<usize>,
    pub last_cartel_month: i64,
    pub tenders: Vec<Tender>,
}

/// One line of the arbitrage board.
#[derive(Clone, Debug)]
pub struct Arb {
    pub res: usize,
    pub star: String,
    pub dist_ly: f64,
    pub here: f64,
    pub there: f64,
}

impl Arb {
    pub fn margin(&self) -> f64 {
        self.there / self.here.max(1e-9) - 1.0
    }
}

impl Sim {
    pub fn rumours(&self) -> Vec<Rumour> {
        rumours(&self.data, self.galaxy.seed, self.here_pos, self.time)
    }

    /// The best price gaps between here and the systems within `radius` ly: buy here, sell there.
    pub fn arbitrage(&self, radius: f64, top: usize) -> Vec<Arb> {
        let mut stars = self.galaxy.nearby(self.here_pos, radius);
        stars.retain(|s| s.distance_to(self.here_pos) > 0.5);
        stars.sort_by(|a, b| {
            a.distance_to(self.here_pos)
                .total_cmp(&b.distance_to(self.here_pos))
        });
        stars.truncate(12);
        let mut out = Vec::new();
        for s in stars {
            for r in 0..self.data.resources.len() {
                let here = self.reference_price(self.here_pos, self.time, r);
                let there = self.reference_price(s.pos, self.time, r);
                if here > 0.0 && there > here * 1.05 {
                    out.push(Arb {
                        res: r,
                        star: s.header.name.clone(),
                        dist_ly: s.distance_to(self.here_pos),
                        here,
                        there,
                    });
                }
            }
        }
        out.sort_by(|a, b| b.margin().total_cmp(&a.margin()));
        out.truncate(top);
        out
    }

    fn cargo_value(&self) -> f64 {
        (0..self.data.resources.len())
            .map(|r| self.ship.cargo.kg[r] * self.data.resources[r].value)
            .sum()
    }

    /// Insure the cargo now aboard for a month; returns the premium.
    pub fn buy_insurance(&mut self) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : l'assureur est a la station")?;
        let value = self.cargo_value();
        if value < 100.0 {
            return Err("soute trop peu chargee pour une police");
        }
        let premium = (value * PREMIUM).max(20.0);
        if premium > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= premium;
        self.trade.insured_until = self.time + INSURANCE_DAYS * DAY;
        self.trade.covered = value;
        Ok(premium)
    }

    pub fn insured(&self) -> bool {
        self.trade.insured_until > self.time
    }

    /// Pay the insured part of a cargo of `value` lost (the ship was destroyed).
    pub(crate) fn cargo_claim(&mut self, value: f64) {
        if !self.insured() {
            return;
        }
        let pay = value.min(self.trade.covered) * COVER;
        self.trade.insured_until = 0.0;
        if pay > 0.0 {
            self.credits += pay;
            self.events.push(Event::MarketNews(format!(
                "Assurance : {pay:.0} CR verses pour la cargaison perdue."
            )));
        }
    }

    /// Value of the cargo, for the insurance claim at respawn.
    pub(crate) fn claim_before_wipe(&mut self) {
        let v = self.cargo_value();
        self.cargo_claim(v);
    }

    // ---- cartels ----------------------------------------------------------------

    pub fn cartel_here(&self, c: usize) -> bool {
        cartel_active(self.galaxy.seed, c, self.here_pos, self.time)
    }

    pub fn join_cartel(&mut self, c: usize) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if c >= self.data.cartels.len() {
            return Err("cartel inconnu");
        }
        if self.trade.seats.contains(&c) {
            return Err("vous avez deja un siege");
        }
        if self.trade.seats.len() >= MAX_SEATS {
            return Err("trois sieges au plus");
        }
        if SEAT_PRICE > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= SEAT_PRICE;
        self.trade.seats.push(c);
        Ok(())
    }

    pub fn leave_cartel(&mut self, c: usize) -> Result<(), &'static str> {
        let i = self
            .trade
            .seats
            .iter()
            .position(|x| *x == c)
            .ok_or("pas de siege")?;
        self.trade.seats.remove(i);
        Ok(())
    }

    /// Once a month: seats pay when their cartel holds prices around you; the antitrust
    /// authority may find you out.
    pub(crate) fn cartel_tick(&mut self) {
        let month = (self.time / CARTEL_MONTH).floor() as i64;
        let last = self.trade.last_cartel_month;
        self.trade.last_cartel_month = month;
        if last == 0 || month <= last || self.trade.seats.is_empty() {
            return;
        }
        let seats = self.trade.seats.clone();
        let mut total = 0.0;
        let mut caught = None;
        for c in seats {
            if !self.cartel_here(c) {
                continue;
            }
            total += SEAT_PAYOUT * self.data.cartels[c].markup / 0.25;
            if unit(hash_coord(self.galaxy.seed ^ 0xA77, month, c as i64, 9)) < ANTITRUST_ODDS {
                caught = Some(c);
            }
        }
        if total > 0.0 {
            self.credits += total;
            self.events.push(Event::MarketNews(format!(
                "Cartels : {total:.0} CR de profits partages."
            )));
        }
        if let Some(c) = caught {
            let fine = self.credits.clamp(0.0, ANTITRUST_FINE);
            self.credits -= fine;
            self.trade.seats.retain(|x| *x != c);
            let hq = self
                .data
                .factions
                .iter()
                .position(|f| f.id == self.data.cartels[c].hq);
            self.change_reputation(self.faction, -0.1);
            let _ = hq;
            self.events.push(Event::MarketNews(format!(
                "Enquete antitrust : amende de {fine:.0} CR, siege perdu ({}).",
                self.data.cartels[c].name
            )));
        }
    }

    // ---- tenders ----------------------------------------------------------------

    /// Tenders posted by the station you are docked at.
    pub fn offered_tenders(&self) -> Vec<TenderOffer> {
        let Ok(st) = self.docked_station() else {
            return Vec::new();
        };
        let epoch = (self.time / TENDER_EPOCH).floor() as i64;
        let n = self.data.resources.len() as u64;
        let mut out = Vec::new();
        for k in 0..TENDERS_OFFERED as i64 {
            let h = |j: i64| hash_coord(self.system.seed ^ 0x7E4D, st as i64, epoch, k * 8 + j);
            let res = (h(0) % n) as usize;
            let reference = self.reference_price(self.here_pos, self.time, res);
            if reference <= 0.0 {
                continue;
            }
            let id = h(9);
            if self.trade.tenders.iter().any(|t| t.id == id) {
                continue;
            }
            out.push(TenderOffer {
                id,
                res,
                kg: (200.0 + 1800.0 * unit(h(1))).round(),
                ceiling: reference * 1.6,
            });
        }
        out
    }

    /// Bid `price` per kg on tender `id`. The rivals' best bid is a fixed draw below the ceiling:
    /// the lowest bid wins, but a bid below cost is yours to honour.
    pub fn bid_tender(&mut self, id: u64, price: f64) -> Result<(), &'static str> {
        let st = self.docked_station()?;
        let offer = self
            .offered_tenders()
            .into_iter()
            .find(|o| o.id == id)
            .ok_or("appel d'offres clos")?;
        if self.trade.tenders.len() >= MAX_TENDERS {
            return Err("trop de marches en cours");
        }
        if !(price > 0.0 && price <= offer.ceiling) {
            return Err("offre au-dessus du plafond");
        }
        let rival = offer.ceiling * (0.55 + 0.35 * unit(hash_coord(id, 5, 6, 7)));
        if price >= rival {
            return Err("offre battue : un concurrent est moins cher");
        }
        self.trade.tenders.push(Tender {
            id,
            system_seed: self.system.seed,
            station: st,
            res: offer.res,
            kg: offer.kg,
            price,
            deadline: self.time + TENDER_DAYS * DAY,
        });
        Ok(())
    }

    /// Deliver tender `i` at its station; returns the payment.
    pub fn deliver_tender(&mut self, i: usize) -> Result<f64, &'static str> {
        let st = self.docked_station()?;
        let t = self.trade.tenders.get(i).ok_or("marche inconnu")?.clone();
        if t.system_seed != self.system.seed || t.station != st {
            return Err("ce marche se livre a une autre station");
        }
        if self.ship.cargo.kg[t.res] + 1e-9 < t.kg {
            return Err("cargaison insuffisante");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, t.res, t.kg);
        let pay = t.kg * t.price;
        self.credits += pay;
        self.trade.tenders.remove(i);
        self.change_reputation(self.faction, 0.03);
        Ok(pay)
    }

    /// Fine the tenders that ran out of time.
    pub(crate) fn tender_tick(&mut self) {
        let now = self.time;
        let late: Vec<Tender> = self
            .trade
            .tenders
            .iter()
            .filter(|t| t.deadline < now)
            .cloned()
            .collect();
        if late.is_empty() {
            return;
        }
        self.trade.tenders.retain(|t| t.deadline >= now);
        for t in late {
            let fine = (t.kg * t.price * 0.15).min(self.credits.max(0.0));
            self.credits -= fine;
            self.change_reputation(self.faction, -0.1);
            self.events.push(Event::MarketNews(format!(
                "Marche non livre : penalite de {fine:.0} CR."
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn docked() -> Sim {
        let mut s = new_sim(5, data());
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
    fn rumours_announce_real_events_and_some_are_false() {
        let d = data();
        let pos = [10.0, 5.0, -20.0];
        let (mut real, mut fake) = (0, 0);
        for w in 0..200 {
            let now = f64::from(w) * 3.0 * DAY;
            let list = rumours(&d, 9, pos, now);
            assert_eq!(list, rumours(&d, 9, pos, now), "deterministic");
            let events = events_between(&d, 9, pos, now, now + RUMOUR_DAYS * DAY);
            for r in &list {
                assert!(r.start > now && r.start <= now + RUMOUR_DAYS * DAY);
                let exists = events
                    .iter()
                    .any(|e| e.def == r.def && (e.start - r.start).abs() < 1.0);
                if r.real {
                    real += 1;
                    assert!(exists, "a real rumour matches an event");
                } else {
                    fake += 1;
                }
                assert!((0.2..=0.95).contains(&r.confidence));
            }
        }
        assert!(real > 50 && fake > 20, "{real} real, {fake} false");
        let share = f64::from(fake) / f64::from(real + fake);
        assert!(share > 0.1 && share < 0.6, "false share {share}");
    }

    #[test]
    fn cartels_raise_prices_only_where_and_when_active() {
        let d = data();
        let seed = 4;
        let pos = [0.0; 3];
        let (mut active, mut total) = (0, 0);
        for m in 0..120 {
            let t = f64::from(m) * CARTEL_MONTH + 1.0;
            for c in 0..d.cartels.len() {
                total += 1;
                if cartel_active(seed, c, pos, t) {
                    active += 1;
                    let res = d.resource_index(&d.cartels[c].goods[0]).unwrap();
                    let f = cartel_factor(&d, seed, pos, t, res);
                    assert!(f >= 1.0 + d.cartels[c].markup - 1e-9);
                }
            }
        }
        let share = f64::from(active) / f64::from(total);
        assert!((share - P_CARTEL).abs() < 0.1, "active share {share}");
        // A good no cartel controls is untouched.
        let free = (0..d.resources.len())
            .find(|r| {
                d.cartels
                    .iter()
                    .all(|k| k.goods.iter().all(|g| d.resource_index(g) != Some(*r)))
            })
            .unwrap();
        assert_eq!(cartel_factor(&d, seed, pos, 5.0 * DAY, free), 1.0);
    }

    #[test]
    fn a_cartel_seat_costs_pays_when_active_and_can_end_in_a_fine() {
        let mut s = docked();
        assert!(s.join_cartel(99).is_err());
        s.join_cartel(0).unwrap();
        assert!(s.join_cartel(0).is_err(), "one seat per cartel");
        s.join_cartel(1).unwrap();
        s.join_cartel(2).unwrap();
        assert!(s.join_cartel(3).is_err(), "three seats at most");
        s.leave_cartel(2).unwrap();
        assert_eq!(s.trade.seats, vec![0, 1]);
        s.trade.last_cartel_month = 1;
        let (mut paid, mut fined) = (false, false);
        for m in 2..400 {
            s.time = f64::from(m) * CARTEL_MONTH + 1.0;
            let credits = s.credits;
            if s.trade.seats.is_empty() {
                s.trade.seats = vec![0, 1];
            }
            s.cartel_tick();
            if s.credits > credits {
                paid = true;
            }
            if s.events
                .iter()
                .any(|e| matches!(e, Event::MarketNews(t) if t.starts_with("Enquete")))
            {
                fined = true;
            }
        }
        assert!(paid, "seats pay when their cartel is active");
        assert!(fined, "the antitrust authority finds you out sometimes");
    }

    #[test]
    fn insurance_pays_part_of_a_cargo_lost_only_while_it_runs() {
        let mut s = docked();
        s.ship.cargo.kg.fill(0.0);
        assert!(s.buy_insurance().is_err(), "nothing to insure");
        let gold = s.data.resource_index("gold").unwrap();
        s.ship.cargo.kg[gold] = 100.0;
        let value = s.cargo_value();
        let c = s.credits;
        let premium = s.buy_insurance().expect("insured");
        assert!((premium - value * PREMIUM).abs() < 1e-6);
        assert!(s.insured());
        s.credits = c - premium;
        s.claim_before_wipe();
        assert!((s.credits - (c - premium + value * COVER)).abs() < 1e-6);
        assert!(!s.insured(), "one claim per policy");
        s.claim_before_wipe();
        assert!((s.credits - (c - premium + value * COVER)).abs() < 1e-6);
        // Expired policy pays nothing.
        s.buy_insurance().unwrap();
        s.time += (INSURANCE_DAYS + 1.0) * DAY;
        let c = s.credits;
        s.claim_before_wipe();
        assert_eq!(s.credits, c);
    }

    #[test]
    fn tenders_are_won_by_underbidding_rivals_and_must_be_delivered() {
        let mut s = docked();
        let offers = s.offered_tenders();
        assert!(!offers.is_empty());
        let o = offers[0].clone();
        assert_eq!(offers, s.offered_tenders(), "same offers all epoch");
        assert!(
            s.bid_tender(o.id, o.ceiling * 2.0).is_err(),
            "above the ceiling"
        );
        assert!(
            s.bid_tender(o.id, o.ceiling * 0.95).is_err(),
            "beaten by a rival"
        );
        assert!(s.bid_tender(1, 1.0).is_err(), "unknown tender");
        s.bid_tender(o.id, o.ceiling * 0.5).expect("a low bid wins");
        assert!(!s.offered_tenders().iter().any(|x| x.id == o.id));
        assert!(s.deliver_tender(0).is_err(), "no cargo yet");
        s.ship.cargo.kg[o.res] = o.kg;
        let c = s.credits;
        let pay = s.deliver_tender(0).expect("delivered");
        assert!((pay - o.kg * o.ceiling * 0.5).abs() < 1e-6);
        assert!((s.credits - c - pay).abs() < 1e-6);
        assert!(s.trade.tenders.is_empty());
        // A won tender left undelivered costs a penalty.
        let o2 = s.offered_tenders()[0].clone();
        s.bid_tender(o2.id, o2.ceiling * 0.5).unwrap();
        s.time += (TENDER_DAYS + 1.0) * DAY;
        let c = s.credits;
        s.tender_tick();
        assert!(s.credits < c && s.trade.tenders.is_empty());
    }

    #[test]
    fn arbitrage_lists_gaps_with_a_positive_margin() {
        let s = docked();
        let list = s.arbitrage(60.0, 8);
        assert!(!list.is_empty(), "somewhere is more expensive");
        for a in &list {
            assert!(a.margin() > 0.05 && a.dist_ly > 0.5 && !a.star.is_empty());
        }
        assert!(list.windows(2).all(|w| w[0].margin() >= w[1].margin()));
    }
}
