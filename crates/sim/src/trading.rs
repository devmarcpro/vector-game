//! More ways to play the exchange (E7): rumours that announce events before they happen (some
//! are false), arbitrage between nearby systems, cargo insurance, cartels that hold prices up (and
//! can be joined, at a risk) and tenders where you bid against rivals to supply a good.
use crate::bourse::{events_between, DAY, WEEK};
use crate::planet::unit;
use crate::relations::Entity;
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
/// Insurance: cover share of a claim, duration of a policy, the insurer's loading over the loss
/// odds (it must make a profit), the monthly odds of losing a ship nobody has ever heard of
/// in the calmest space, and how long a claim stays in the file.
pub const COVER: f64 = 0.7;
pub const INSURANCE_DAYS: f64 = 30.0;
pub const LOADING: f64 = 1.8;
pub const BASE_ODDS: f64 = 0.004;
pub const CLAIM_MEMORY_DAYS: f64 = 365.0;
/// Chance that a claim of nothing but the player's word is examined, before history.
pub const AUDIT_BASE: f64 = 0.25;
/// Cartel seat price, monthly payout at the base markup (0.25), antitrust fine and its monthly odds.
pub const SEAT_PRICE: f64 = 4000.0;
pub const SEAT_PAYOUT: f64 = 2500.0;
pub const ANTITRUST_FINE: f64 = 3000.0;
/// Goodwill of a cartel won by taking a seat, lost when the antitrust authority catches you.
pub const SEAT_FAVOUR: f32 = 0.15;
pub const ANTITRUST_FAVOUR: f32 = 0.3;
pub const ANTITRUST_ODDS: f64 = 0.10;
pub const MAX_SEATS: usize = 3;
/// Share of months a cartel is active in a region.
pub const P_CARTEL: f64 = 0.4;
pub const CARTEL_MONTH: f64 = 30.0 * DAY;
/// Tenders: days to deliver, how many are offered at once.
pub const TENDER_DAYS: f64 = 20.0;
pub const TENDERS_OFFERED: usize = 3;
const TENDER_EPOCH: f64 = 10.0 * DAY;

#[derive(Clone, Debug, PartialEq)]
pub struct Rumour {
    pub def: usize,
    pub start: f64,
    /// How reliable the source looks, 0..1 (it is not a promise).
    pub confidence: f64,
    /// Whether it will really happen (hidden from the player).
    pub real: bool,
    /// Remembered from an abandoned timeline (time travel): certain.
    pub remembered: bool,
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
                remembered: false,
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
                remembered: false,
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

/// A tender on offer (not yet won).
#[derive(Clone, Debug, PartialEq)]
pub struct TenderOffer {
    pub id: u64,
    pub res: usize,
    pub kg: f64,
    /// The most the buyer will pay per kg.
    pub ceiling: f64,
}

/// What can go wrong with a ship around a place at a date: it feeds the insurers' premiums
/// and the losses of the regular lines (one representation of risk for both).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Risk {
    /// Aggression of the people who hold the space (0.6 for the free space).
    pub zone: f64,
    /// Pirates: thick in the free space, thin under a strong flag.
    pub pirates: f64,
    pub war: bool,
    pub siege: bool,
    /// Epidemic running (share of the people infectious): ports close, ships wait.
    pub sick: f64,
    /// A catastrophe (an industrial incident) is shaking the sector.
    pub catastrophe: bool,
}

pub fn risk_at(
    data: &GameData,
    galaxy: &vector_world::galaxy::Galaxy,
    seed: u64,
    pos: [f64; 3],
    t: f64,
) -> Risk {
    let c = crate::politics::cell_of(pos);
    let faction = crate::politics::faction_of(data, galaxy, c);
    let unrest = crate::politics::unrest(seed, c, t);
    let zone = faction.map_or(0.6, |f| data.factions[f].aggression);
    let pirates = if faction.is_none() {
        0.7 + 0.3 * unrest
    } else {
        0.15 * zone + 0.3 * unrest
    };
    Risk {
        zone,
        pirates: pirates.clamp(0.0, 1.0),
        war: crate::politics::sector_at_war(data, galaxy, seed, c, t),
        siege: crate::arms::is_besieged(data, galaxy, seed, c, t),
        sick: crate::epidemic::prevalence_at(data, seed, pos, t)
            .iter()
            .map(|(_, i)| *i)
            .sum(),
        catastrophe: !crate::incidents::incidents_in_cell(data, seed, c, t, t).is_empty(),
    }
}

impl Risk {
    /// Chance per month that a ship loses its cargo or itself here.
    pub fn monthly_loss_odds(&self) -> f64 {
        BASE_ODDS
            + 0.05 * self.pirates
            + if self.war { 0.04 } else { 0.0 }
            + if self.siege { 0.03 } else { 0.0 }
            + if self.catastrophe { 0.015 } else { 0.0 }
            + 0.01 * self.zone
    }
}

/// What a month of cover would cost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quote {
    pub cargo: f64,
    pub hull: f64,
    pub odds: f64,
    pub history: f64,
    pub premium: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TradeState {
    /// Insurance: end date and covered value of the cargo, covered value of the hull.
    pub insured_until: f64,
    pub covered: f64,
    pub hull_covered: f64,
    /// Dates of the claims paid in the last year (the premium follows the file) and the frauds
    /// found out.
    pub claims: Vec<f64>,
    pub frauds: u32,
    /// Bonds, funds and options (see `securities`).
    pub folio: crate::securities::Portfolio,
    /// Auctions, ads and the flea market (see `auction`).
    pub dealings: crate::auction::Dealings,
    /// Regular lines of cargo ships the player watches or has hit (see `lanes`).
    pub lanes: crate::lanes::LaneLog,
    /// Net migrants taken into each sector by the player (see `macroeco`).
    pub migrants: Vec<([i64; 3], f64)>,
    /// Cartel seats held, and the month last settled.
    pub seats: Vec<usize>,
    pub last_cartel_month: i64,
    /// The legal record of the player, per jurisdiction (see `law`).
    pub record: Vec<crate::law::Record>,
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
        let mut list = rumours(&self.data, self.galaxy.seed, self.here_pos, self.time);
        for r in self.remembered() {
            if let Some(k) = list
                .iter()
                .position(|x| x.def == r.def && (x.start - r.start).abs() < 1.0)
            {
                list[k] = r;
            } else {
                list.push(r);
            }
        }
        list.sort_by(|a, b| a.start.total_cmp(&b.start));
        list
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

    /// The risk around the ship now (see `risk_at`).
    pub fn risk_here(&self) -> Risk {
        risk_at(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            self.here_pos,
            self.time,
        )
    }

    /// Claims of the last year still in the file.
    pub fn recent_claims(&self) -> usize {
        self.trade
            .claims
            .iter()
            .filter(|t| self.time - **t < CLAIM_MEMORY_DAYS * DAY)
            .count()
    }

    /// The premium multiplier of the player's file: 1 for a clean one, +30 % per claim of the
    /// year and +60 % per fraud.
    pub fn history_factor(&self) -> f64 {
        1.0 + 0.3 * self.recent_claims() as f64 + 0.6 * f64::from(self.trade.frauds)
    }

    /// What a month of cover costs now: the loss odds around the ship, the insurer's loading and
    /// the player's file applied to the values at risk.
    pub fn insurance_quote(&self, with_hull: bool) -> Quote {
        let cargo = self.cargo_value();
        let hull = if with_hull { self.ship_value() } else { 0.0 };
        let odds = self.risk_here().monthly_loss_odds();
        let rate = odds * LOADING * self.history_factor();
        Quote {
            cargo,
            hull,
            odds,
            history: self.history_factor(),
            premium: ((cargo + hull) * rate).max(if cargo + hull > 0.0 { 20.0 } else { 0.0 }),
        }
    }

    /// Insure the cargo now aboard (and the hull when asked) for a month; returns the premium.
    pub fn buy_insurance(&mut self, with_hull: bool) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : l'assureur est a la station")?;
        let q = self.insurance_quote(with_hull);
        if q.cargo + q.hull < 100.0 {
            return Err("rien d'assez precieux pour une police");
        }
        if q.premium > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= q.premium;
        self.trade.insured_until = self.time + INSURANCE_DAYS * DAY;
        self.trade.covered = q.cargo;
        self.trade.hull_covered = q.hull;
        Ok(q.premium)
    }

    pub fn insured(&self) -> bool {
        self.trade.insured_until > self.time
    }

    fn record_claim(&mut self) {
        self.trade.claims.push(self.time);
        let now = self.time;
        self.trade
            .claims
            .retain(|t| now - *t < CLAIM_MEMORY_DAYS * DAY);
    }

    /// Pay the insured part of a cargo of `value` lost (the ship was destroyed), and of the hull.
    pub(crate) fn cargo_claim(&mut self, value: f64) {
        if !self.insured() {
            return;
        }
        let pay = (value.min(self.trade.covered) + self.trade.hull_covered) * COVER;
        self.trade.insured_until = 0.0;
        self.trade.hull_covered = 0.0;
        if pay > 0.0 {
            self.record_claim();
            self.credits += pay;
            self.events.push(Event::MarketNews(format!(
                "Assurance : {pay:.0} CR verses (cargaison et coque)."
            )));
        }
    }

    /// Declare that `kg` of `res` were lost (taken by pirates, jettisoned, seized). The cargo
    /// leaves the hold; the insurer pays `COVER` of the value, but may audit the claim: the
    /// more claims in the file and the calmer the space where it happened, the likelier. A
    /// false claim cancels the policy, costs three months of premium and is a crime.
    pub fn declare_loss(&mut self, res: usize, kg: f64) -> Result<f64, &'static str> {
        if !self.insured() {
            return Err("aucune police en cours");
        }
        let kg = kg.min(self.ship.cargo.kg[res]);
        if kg < 1.0 {
            return Err("rien de tel en soute");
        }
        let value = (kg * self.data.resources[res].value).min(self.trade.covered);
        self.ship.cargo.kg[res] -= kg;
        self.trade.covered = (self.trade.covered - value).max(0.0);
        let risk = self.risk_here();
        // A real loss leaves traces where it is likely: in dangerous space the claim is believed.
        let audit =
            (AUDIT_BASE + 0.2 * self.recent_claims() as f64 + 0.2 * f64::from(self.trade.frauds)
                - 1.5 * risk.monthly_loss_odds())
            .clamp(0.05, 0.95);
        let roll = unit(hash_coord(
            self.galaxy.seed ^ 0xF4A0D,
            (self.time / 60.0) as i64,
            res as i64,
            self.trade.claims.len() as i64 + 31 * i64::from(self.trade.frauds),
        ));
        // Nobody really took the cargo: only the audit stands between the player and the money.
        if roll < audit {
            let fee = 3.0 * self.insurance_quote(false).premium.max(20.0);
            self.credits -= fee.min(self.credits.max(0.0));
            self.trade.insured_until = 0.0;
            self.trade.frauds += 1;
            let faction = self.faction;
            let _ = self.offend(faction, "fraude");
            self.events.push(Event::MarketNews(
                "Assurance : fraude decouverte, police annulee.".into(),
            ));
            return Err("fraude decouverte");
        }
        let pay = value * COVER;
        self.credits += pay;
        self.record_claim();
        self.events.push(Event::MarketNews(format!(
            "Assurance : sinistre de {:.0} kg de {} indemnise {pay:.0} CR.",
            kg,
            self.data.resources[res].name.to_lowercase()
        )));
        Ok(pay)
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
        let price = self.seat_price(c);
        if price > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= price;
        self.trade.seats.push(c);
        self.relate(Entity::cartel(c), SEAT_FAVOUR);
        Ok(())
    }

    /// What a seat of cartel `c` costs you: its members sell cheaper to those it already likes.
    pub fn seat_price(&self, c: usize) -> f64 {
        SEAT_PRICE * (1.0 - 0.3 * f64::from(self.rel(Entity::cartel(c)).max(0.0)))
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
            let favour = f64::from(self.rel(Entity::cartel(c)).max(0.0));
            total += SEAT_PAYOUT * self.data.cartels[c].markup / 0.25 * (1.0 + 0.5 * favour);
            self.relate(Entity::cartel(c), 0.01);
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
            self.relate(Entity::cartel(c), -ANTITRUST_FAVOUR);
            if let Some(h) = hq {
                self.change_reputation(Some(h), -0.5);
            }
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
            // Already won (running, or done): no longer on offer.
            if self.contracts.iter().any(|c| c.id == id) || self.done_contracts.contains(&id) {
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
    /// the lowest bid wins, and a won tender is an ordinary delivery contract to this station
    /// (same list, same payment, same expiry and reputation as any other).
    pub fn bid_tender(&mut self, id: u64, price: f64) -> Result<(), &'static str> {
        let st = self.docked_station()?;
        let offer = self
            .offered_tenders()
            .into_iter()
            .find(|o| o.id == id)
            .ok_or("appel d'offres clos")?;
        if !(price > 0.0 && price <= offer.ceiling) {
            return Err("offre au-dessus du plafond");
        }
        let rival = offer.ceiling * (0.55 + 0.35 * unit(hash_coord(id, 5, 6, 7)));
        if price >= rival {
            return Err("offre battue : un concurrent est moins cher");
        }
        self.accept_contract(crate::economy::Contract {
            pax: 0,
            pax_kind: 0,
            legs: Vec::new(),
            guide: false,
            origin: [0; 3],
            id,
            dest_star: self.here,
            dest_seed: self.system.seed,
            dest_body: st,
            dest_name: self.bodies[st].name.clone(),
            dest_system: self.system.name.clone(),
            dist_ly: 0.0,
            resource: offer.res,
            kg: offer.kg,
            reward: offer.kg * price,
            expires: self.time + TENDER_DAYS * DAY,
            faction: self.faction,
        })
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
    fn a_seat_wins_the_cartel_and_its_headquarters_and_a_known_cartel_sells_cheaper() {
        let mut s = docked();
        let hq = s
            .data
            .factions
            .iter()
            .position(|f| f.id == s.data.cartels[0].hq)
            .unwrap();
        let (rep, price0) = (s.reputation_with(Some(hq)), s.seat_price(0));
        assert!(price0 <= SEAT_PRICE && price0 > 0.7 * SEAT_PRICE);
        s.join_cartel(0).unwrap();
        assert!(
            s.rel(Entity::cartel(0)) > 0.1,
            "the cartel likes a new member"
        );
        assert!(
            s.reputation_with(Some(hq)) > rep,
            "so does the faction behind it"
        );
        assert!(s.seat_price(0) < price0, "a friend pays less");
        s.leave_cartel(0).unwrap();
        let credits = s.credits;
        s.join_cartel(0).unwrap();
        assert!(
            credits - s.credits < SEAT_PRICE,
            "the second seat was cheaper"
        );
        // Caught by the antitrust authority, the cartel turns on you.
        let favour = s.rel(Entity::cartel(0));
        s.trade.last_cartel_month = 1;
        for m in 2..600 {
            s.time = f64::from(m) * CARTEL_MONTH + 1.0;
            if !s.trade.seats.contains(&0) {
                break;
            }
            s.cartel_tick();
        }
        assert!(!s.trade.seats.contains(&0), "the seat was lost");
        assert!(
            s.rel(Entity::cartel(0)) < favour,
            "and the cartel's goodwill with it"
        );
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
        assert!(s.buy_insurance(false).is_err(), "nothing to insure");
        let gold = s.data.resource_index("gold").unwrap();
        s.ship.cargo.kg[gold] = 100.0;
        let value = s.cargo_value();
        let c = s.credits;
        let quote = s.insurance_quote(false);
        let premium = s.buy_insurance(false).expect("insured");
        assert!((premium - quote.premium).abs() < 1e-9);
        assert!(s.insured());
        s.credits = c - premium;
        s.claim_before_wipe();
        assert!((s.credits - (c - premium + value * COVER)).abs() < 1e-6);
        assert!(!s.insured(), "one claim per policy");
        s.claim_before_wipe();
        assert!((s.credits - (c - premium + value * COVER)).abs() < 1e-6);
        // Expired policy pays nothing.
        s.buy_insurance(false).unwrap();
        s.time += (INSURANCE_DAYS + 1.0) * DAY;
        let c = s.credits;
        s.claim_before_wipe();
        assert_eq!(s.credits, c);
    }

    #[test]
    fn the_premium_is_proportional_to_the_real_risk_and_to_the_file() {
        let d = data();
        let g = vector_world::galaxy::Galaxy::new(5);
        // Free space is riskier than a calm, well-policed faction; war and siege add up.
        let mut calm = f64::MAX;
        let mut wild = 0.0_f64;
        for x in -6..7 {
            for y in -2..3 {
                let r = risk_at(
                    &d,
                    &g,
                    5,
                    crate::politics::cell_center([x, y, 0]),
                    10.0 * DAY,
                );
                calm = calm.min(r.monthly_loss_odds());
                wild = wild.max(r.monthly_loss_odds());
            }
        }
        assert!(wild > 2.0 * calm, "{calm} vs {wild}");
        let base = Risk {
            zone: 0.1,
            pirates: 0.05,
            war: false,
            siege: false,
            sick: 0.0,
            catastrophe: false,
        };
        let war = Risk { war: true, ..base };
        let both = Risk { siege: true, ..war };
        assert!(base.monthly_loss_odds() < war.monthly_loss_odds());
        assert!(war.monthly_loss_odds() < both.monthly_loss_odds());
        // The premium is exactly value x odds x loading x file, and doubles with the value.
        let mut s = docked();
        s.ship.cargo.kg.fill(0.0);
        let gold = s.data.resource_index("gold").unwrap();
        s.ship.cargo.kg[gold] = 1000.0;
        let q1 = s.insurance_quote(false);
        s.ship.cargo.kg[gold] = 2000.0;
        let q2 = s.insurance_quote(false);
        assert!((q2.premium / q1.premium - 2.0).abs() < 1e-9);
        assert!((q1.premium - q1.cargo * q1.odds * LOADING).abs() < 1e-6);
        // The hull is insured too, for a premium of the same rate.
        let qh = s.insurance_quote(true);
        assert!((qh.premium / (qh.cargo + qh.hull) - q2.premium / q2.cargo).abs() < 1e-9);
        // A claim in the file raises the next premium by 30 %.
        s.trade.claims.push(s.time - DAY);
        let q3 = s.insurance_quote(false);
        assert!((q3.premium / q2.premium - 1.3).abs() < 1e-9);
        // Claims fade after a year.
        s.time += 400.0 * DAY;
        assert_eq!(s.recent_claims(), 0);
    }

    #[test]
    fn a_false_claim_is_sometimes_paid_sometimes_caught_and_the_file_remembers() {
        let (mut paid, mut caught) = (0, 0);
        for n in 0..60 {
            let mut s = docked();
            s.time = (100.0 + 3.0 * f64::from(n)) * DAY;
            s.ship.cargo.kg.fill(0.0);
            let gold = s.data.resource_index("gold").unwrap();
            s.ship.cargo.kg[gold] = 100.0;
            s.buy_insurance(false).unwrap();
            let c = s.credits;
            let kg_before = s.ship.cargo.kg[gold];
            match s.declare_loss(gold, 50.0) {
                Ok(pay) => {
                    paid += 1;
                    assert!((s.credits - c - pay).abs() < 1e-6);
                    assert_eq!(s.recent_claims(), 1);
                    assert!((s.ship.cargo.kg[gold] - (kg_before - 50.0)).abs() < 1e-9);
                }
                Err(e) => {
                    assert_eq!(e, "fraude decouverte");
                    caught += 1;
                    assert!(s.credits < c, "the fee is charged");
                    assert!(!s.insured() && s.trade.frauds == 1);
                    assert!(s.history_factor() > 1.5);
                }
            }
        }
        assert!(paid > 10 && caught > 3, "{paid} paid, {caught} caught");
    }

    #[test]
    fn a_won_tender_is_an_ordinary_contract_with_its_payment_and_its_penalty() {
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
        let n = s.contracts.len();
        s.bid_tender(o.id, o.ceiling * 0.5).expect("a low bid wins");
        assert_eq!(s.contracts.len(), n + 1, "it is a contract now");
        assert!(!s.offered_tenders().iter().any(|x| x.id == o.id));
        let idx = s.contracts.iter().position(|c| c.id == o.id).unwrap();
        assert!(s.deliver_contract(idx).is_err(), "no cargo yet");
        s.ship.cargo.kg[o.res] = o.kg;
        let c = s.credits;
        let issuer = s.contracts[idx].faction.or(s.faction);
        let edge = 1.0 + 0.001 * f64::from(s.reputation_with(issuer));
        let pay = s.deliver_contract(idx).expect("delivered");
        assert!((pay - o.kg * o.ceiling * 0.5 * edge * (1.0 - s.tax_rate())).abs() < 1e-6);
        assert!((s.credits - c - pay).abs() < 1e-6);
        assert!(!s.contracts.iter().any(|c| c.id == o.id));
        assert!(
            !s.offered_tenders().iter().any(|x| x.id == o.id),
            "not offered twice"
        );
        // A won tender left undelivered expires like any contract: reputation is lost.
        let o2 = s.offered_tenders()[0].clone();
        s.bid_tender(o2.id, o2.ceiling * 0.5).unwrap();
        let rep0 = s.reputation_with(s.faction);
        s.time += (TENDER_DAYS + 1.0) * DAY;
        s.expire_contracts();
        assert!(!s.contracts.iter().any(|c| c.id == o2.id));
        assert!(s.reputation_with(s.faction) < rep0);
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
