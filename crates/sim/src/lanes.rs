//! Regular lines of cargo ships between neighbouring systems. A line is a fact of the galaxy
//! (a function of the seed and of its two ends): a good that the origin makes is carried to the
//! destination by a convoy that leaves on a schedule, flies for days, may be held up (war, a
//! blockade, a closed port in an epidemic), may be robbed on the way (the same risk as the
//! insurers' premiums, see `trading::risk_at`) and pays port fees and the customs of the
//! destination. The goods are real: they leave the origin at the departure, are in transit
//! (neither here nor there) and reach the destination at the arrival, minus the loss. The
//! prices of both ends follow those stocks, so a late convoy makes the destination dear.
//! The player may finance a convoy and share its margin.
use crate::bourse::DAY;
use crate::planet::unit;
use crate::trading::{risk_at, Risk};
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Radius within which two systems may be joined by a line, ly.
pub const REACH_LY: f64 = 30.0;
/// Shortest line, ly (nearer systems share a market already).
pub const MIN_LY: f64 = 4.0;
/// Share of the pairs of hubs that are joined by a line.
pub const P_LINE: f64 = 0.04;
/// Share of the stars that are trade hubs.
pub const P_HUB: f64 = 0.3;
/// Days a convoy's stock weighs on the market of its end, and the stock a market absorbs.
pub const RELAX_DAYS: f64 = 12.0;
pub const MARKET_KG: f64 = 150_000.0;
/// Fee of each port, share of the cargo value; and of a financed convoy for the broker.
pub const PORT_FEE: f64 = 0.012;
pub const BROKER_FEE: f64 = 0.02;
pub const MAX_STAKES: usize = 5;
/// Days a convoy is held up by an epidemic, a war and a blockade.
pub const SICK_DELAY: f64 = 6.0;
pub const WAR_DELAY: f64 = 3.0;
pub const SIEGE_DELAY: f64 = 8.0;

/// One regular line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub id: u64,
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub a_name: String,
    pub b_name: String,
    pub res: usize,
    pub dist_ly: f64,
    /// Days between two departures, and the day of the first.
    pub period: f64,
    pub offset: f64,
    /// Light years per day.
    pub speed: f64,
    pub lot_kg: f64,
}

/// One departure of a line: what it carried, when it arrived, what became of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Convoy {
    pub line: u64,
    pub k: i64,
    pub depart: f64,
    /// The timetable's arrival, and the real one.
    pub sched_eta: f64,
    pub arrive: f64,
    pub loaded: f64,
    /// Robbed on the way (at `lost_at`).
    pub lost: f64,
    pub lost_at: f64,
    pub delay_days: f64,
    pub cause: Delay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delay {
    None,
    Quarantine,
    War,
    Blockade,
}

impl Delay {
    pub fn label(self) -> &'static str {
        match self {
            Delay::None => "A L'HEURE",
            Delay::Quarantine => "QUARANTAINE",
            Delay::War => "GUERRE",
            Delay::Blockade => "BLOCUS",
        }
    }
}

/// Where a convoy is at a date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    Transit,
    Arrived,
}

impl Convoy {
    pub fn phase(&self, t: f64) -> Phase {
        if t < self.depart {
            Phase::Waiting
        } else if t < self.arrive {
            Phase::Transit
        } else {
            Phase::Arrived
        }
    }

    pub fn delivered(&self) -> f64 {
        self.loaded - self.lost
    }

    /// Goods in the holds of the convoy at `t` (nothing before it leaves or after it arrives).
    pub fn in_transit(&self, t: f64) -> f64 {
        match self.phase(t) {
            Phase::Transit => self.loaded - if t >= self.lost_at { self.lost } else { 0.0 },
            _ => 0.0,
        }
    }

    /// Fraction of the way flown at `t`.
    pub fn progress(&self, t: f64) -> f64 {
        ((t - self.depart) / (self.arrive - self.depart).max(1.0)).clamp(0.0, 1.0)
    }
}

/// The goods of a line seen at date `t`: shipped, still flying, delivered, lost.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ledger {
    pub shipped: f64,
    pub in_transit: f64,
    pub delivered: f64,
    pub lost: f64,
}

type LineKey = (u64, [u64; 3]);

thread_local! {
    static LINES: RefCell<HashMap<LineKey, Rc<Vec<Line>>>> = RefCell::new(HashMap::new());
    static CONVOYS: RefCell<HashMap<(u64, u64, i64), Convoy>> = RefCell::new(HashMap::new());
}

fn pos_key(p: [f64; 3]) -> [u64; 3] {
    p.map(f64::to_bits)
}

fn hu(seed: u64, a: u64, k: i64) -> f64 {
    unit(hash_coord(seed ^ 0x11E5, a as i64, (a >> 32) as i64 ^ k, k))
}

fn star_hash(s: &vector_world::galaxy::StarEntry) -> u64 {
    s.seed
}

fn is_hub(seed: u64, s: &vector_world::galaxy::StarEntry) -> bool {
    hu(seed, star_hash(s), 1) < P_HUB
}

/// Whether good `res` may enter the space of `faction` (customs forbid it otherwise).
pub fn admitted(data: &GameData, faction: Option<usize>, res: usize) -> bool {
    faction.is_none_or(|f| !crate::economy::banned_goods_of(data, f).contains(&res))
}

/// Customs duty of the people who hold a place: a share of the cargo value (the `tax` of the
/// faction; free space has none).
pub fn tariff(data: &GameData, faction: Option<usize>) -> f64 {
    faction.map_or(0.0, |f| 0.5 * data.factions[f].tax)
}

/// Fee of a port: the more aggressive the people, the heavier the fees.
pub fn port_fee(data: &GameData, faction: Option<usize>) -> f64 {
    PORT_FEE * (1.0 + faction.map_or(0.0, |f| data.factions[f].aggression))
}

/// The lines that touch the system at `pos` (none if no star is there).
pub fn lines_at(data: &GameData, galaxy: &Galaxy, pos: [f64; 3]) -> Rc<Vec<Line>> {
    let key = (galaxy.seed, pos_key(pos));
    if let Some(v) = LINES.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    let sector = Galaxy::sector_of(pos);
    let here = galaxy
        .sector(sector)
        .into_iter()
        .find(|s| s.distance_to(pos) < 1e-6);
    let mut out = Vec::new();
    if let Some(me) = here.filter(|s| is_hub(galaxy.seed, s)) {
        for other in galaxy.nearby(me.pos, REACH_LY) {
            let d = other.distance_to(me.pos);
            if d < MIN_LY || other.id == me.id || !is_hub(galaxy.seed, &other) {
                continue;
            }
            let (lo, hi) = if me.seed < other.seed {
                (me.seed, other.seed)
            } else {
                (other.seed, me.seed)
            };
            let pair = unit(hash_coord(galaxy.seed ^ 0x11FE, lo as i64, hi as i64, 2));
            if pair >= P_LINE {
                continue;
            }
            // The direction and the good follow the pair, not who asks.
            let forward = unit(hash_coord(galaxy.seed ^ 0x11FE, lo as i64, hi as i64, 3)) < 0.5;
            let (a, b) = if forward == (me.seed == lo) {
                (&me, &other)
            } else {
                (&other, &me)
            };
            let n = data.industries.len();
            let ind = &data.industries
                [(hash_coord(galaxy.seed ^ 0x11FE, a.seed as i64, 5, 6) % n as u64) as usize];
            let bf = galaxy.territory(b.pos, data.factions.len());
            // Of what the origin makes, the shippers carry what sells dearest at the other end
            // (prices of the day zero) among the goods that the customs let in.
            let fa = crate::bourse::world_factors(data, galaxy.seed, a.pos, 0.0);
            let fb = crate::bourse::world_factors(data, galaxy.seed, b.pos, 0.0);
            let Some(res) = ind
                .produces
                .iter()
                .filter_map(|r| data.resource_index(r))
                .filter(|r| admitted(data, bf, *r))
                .max_by(|x, y| (fb[*x] / fa[*x]).total_cmp(&(fb[*y] / fa[*y])))
            else {
                continue;
            };
            let id = hash_coord(galaxy.seed ^ 0x11FE, lo as i64, hi as i64, 8);
            let h = |k: i64| unit(hash_coord(galaxy.seed ^ 0x11FF, lo as i64, hi as i64, k));
            out.push(Line {
                id,
                a: a.pos,
                b: b.pos,
                a_name: a.header.name.clone(),
                b_name: b.header.name.clone(),
                res,
                dist_ly: a.distance_to(b.pos),
                period: 6.0 + 14.0 * h(1),
                offset: 20.0 * h(2),
                speed: 0.35 + 0.5 * h(3),
                lot_kg: 15_000.0 + 45_000.0 * h(4),
            });
        }
    }
    out.sort_by_key(|l| l.id);
    let rc = Rc::new(out);
    LINES.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 4000 {
            c.clear();
        }
        c.insert(key, Rc::clone(&rc));
    });
    rc
}

/// Departure number `k` of `line`: a pure function of the line and the number.
pub fn convoy(data: &GameData, galaxy: &Galaxy, line: &Line, k: i64) -> Convoy {
    let key = (galaxy.seed, line.id, k);
    if let Some(c) = CONVOYS.with(|c| c.borrow().get(&key).cloned()) {
        return c;
    }
    let seed = galaxy.seed;
    let h = |n: i64| unit(hash_coord(seed ^ 0x11C0, line.id as i64, k, n));
    let depart = (line.offset + k as f64 * line.period) * DAY;
    let base_days = line.dist_ly / line.speed;
    let sched_eta = depart + base_days * DAY;
    let ra: Risk = risk_at(data, galaxy, seed, line.a, depart);
    let rb: Risk = risk_at(data, galaxy, seed, line.b, sched_eta);
    let mid = [
        0.5 * (line.a[0] + line.b[0]),
        0.5 * (line.a[1] + line.b[1]),
        0.5 * (line.a[2] + line.b[2]),
    ];
    let rm = risk_at(data, galaxy, seed, mid, 0.5 * (depart + sched_eta));
    // The worst reason of delay is the one named.
    let mut delay = 2.0 * h(1) * base_days.sqrt() * 0.3;
    let mut cause = Delay::None;
    let mut add = |d: f64, c: Delay| {
        if d > 0.0 {
            delay += d;
            if cause == Delay::None {
                cause = c;
            }
        }
    };
    if ra.sick > 0.02 {
        add(0.5 * SICK_DELAY, Delay::Quarantine);
    }
    if rb.sick > 0.02 {
        add(SICK_DELAY, Delay::Quarantine);
    }
    if ra.war || rb.war {
        add(WAR_DELAY, Delay::War);
    }
    if rb.siege || ra.siege {
        add(SIEGE_DELAY, Delay::Blockade);
    }
    let arrive = sched_eta + delay * DAY;
    // Pirates: the odds of a month around the way, taken per voyage with the weight of a trip.
    let odds = ((ra.monthly_loss_odds() + 2.0 * rm.monthly_loss_odds() + rb.monthly_loss_odds())
        / 4.0
        * 6.0)
        .min(0.8);
    let loaded = line.lot_kg * (0.85 + 0.3 * h(2));
    let lost = if h(3) < odds {
        loaded * (0.25 + 0.5 * h(4))
    } else {
        0.0
    };
    let c = Convoy {
        line: line.id,
        k,
        depart,
        sched_eta,
        arrive,
        loaded,
        lost,
        lost_at: depart + 0.5 * (arrive - depart),
        delay_days: delay,
        cause,
    };
    CONVOYS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 60_000 {
            m.clear();
        }
        m.insert(key, c.clone());
    });
    c
}

/// Departures of `line` whose flight touches the window [t0, t1].
pub fn convoys_between(
    data: &GameData,
    galaxy: &Galaxy,
    line: &Line,
    t0: f64,
    t1: f64,
) -> Vec<Convoy> {
    let max_flight = (line.dist_ly / line.speed + 30.0) * DAY;
    let k0 = (((t0 - max_flight) / DAY - line.offset) / line.period).floor() as i64;
    let k1 = (((t1 / DAY) - line.offset) / line.period).floor() as i64;
    (k0.max(0)..=k1)
        .map(|k| convoy(data, galaxy, line, k))
        .filter(|c| c.arrive >= t0 && c.depart <= t1)
        .collect()
}

/// The goods of `line` over all its departures up to `t`.
pub fn ledger(data: &GameData, galaxy: &Galaxy, line: &Line, t: f64) -> Ledger {
    let mut l = Ledger::default();
    let k1 = ((t / DAY - line.offset) / line.period).floor() as i64;
    for k in 0..=k1.max(-1) {
        let c = convoy(data, galaxy, line, k);
        if c.depart > t {
            continue;
        }
        l.shipped += c.loaded;
        l.in_transit += c.in_transit(t);
        if t >= c.arrive {
            l.delivered += c.delivered();
        }
        if t >= c.lost_at {
            l.lost += c.lost;
        }
    }
    l
}

/// Price multiplier of good `res` at the system at `pos`, date `t`, from the convoys that leave
/// it, are late to it or reach it.
pub fn good_factor(data: &GameData, galaxy: &Galaxy, pos: [f64; 3], t: f64, res: usize) -> f64 {
    let lines = lines_at(data, galaxy, pos);
    if lines.is_empty() {
        return 1.0;
    }
    let mut stock = 0.0;
    for line in lines.iter().filter(|l| l.res == res) {
        let origin = (line.a[0] - pos[0]).abs() < 1e-6
            && (line.a[1] - pos[1]).abs() < 1e-6
            && (line.a[2] - pos[2]).abs() < 1e-6;
        for c in convoys_between(data, galaxy, line, t - 5.0 * RELAX_DAYS * DAY, t) {
            if origin {
                if t >= c.depart {
                    stock -= c.loaded * (-(t - c.depart) / (RELAX_DAYS * DAY)).exp();
                }
            } else if t >= c.arrive {
                stock += c.delivered() * (-(t - c.arrive) / (RELAX_DAYS * DAY)).exp();
            } else if t >= c.sched_eta {
                // Late: the shelves are empty and everybody knows it.
                let late = (t - c.sched_eta) / (5.0 * DAY);
                stock -= 0.5 * c.loaded * (1.0 - (-late).exp());
            }
        }
    }
    (-0.3 * stock / MARKET_KG).exp().clamp(0.6, 1.6)
}

/// A line at the player's system, with its next departures.
#[derive(Clone, Debug)]
pub struct LineView {
    pub line: Line,
    /// True when the ship's system is the origin.
    pub outbound: bool,
    pub convoys: Vec<Convoy>,
}

/// A convoy the player has financed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stake {
    pub line: u64,
    pub k: i64,
    pub amount: f64,
    pub label: String,
    /// Where the line starts (to find it again from anywhere).
    pub a: [f64; 3],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LaneLog {
    pub stakes: Vec<Stake>,
    pub last_day: i64,
}

impl Sim {
    /// What a broker keeps of a financed convoy: the base fee, scaled by what the AI computes
    /// for itself (the simulation budget of the mind: models and forecasts). A rich budget halves
    /// it at best, a starved one adds half again.
    pub fn broker_fee(&self) -> f64 {
        BROKER_FEE * (2.0 - self.mind_factor(crate::mind::T_SIMULATION)).clamp(0.5, 1.5)
    }

    /// The regular lines of the ship's system with the convoys of the last and next weeks.
    pub fn lines_here(&self) -> Vec<LineView> {
        let lines = lines_at(&self.data, &self.galaxy, self.here_pos);
        lines
            .iter()
            .map(|l| {
                let outbound = (l.a[0] - self.here_pos[0]).abs() < 1e-6
                    && (l.a[1] - self.here_pos[1]).abs() < 1e-6
                    && (l.a[2] - self.here_pos[2]).abs() < 1e-6;
                let mut convoys = convoys_between(
                    &self.data,
                    &self.galaxy,
                    l,
                    self.time - 3.0 * DAY,
                    self.time + 2.5 * l.period * DAY,
                );
                convoys.truncate(5);
                LineView {
                    line: l.clone(),
                    outbound,
                    convoys,
                }
            })
            .collect()
    }

    /// Expected return of a convoy now: the destination price against the origin's, less both
    /// ports and the customs. Negative when the line does not pay.
    pub fn convoy_margin(&self, line: &Line) -> f64 {
        let pa = self.reference_price(line.a, self.time, line.res);
        let pb = self.reference_price(line.b, self.time, line.res);
        let fa = self.galaxy.territory(line.a, self.data.factions.len());
        let fb = self.galaxy.territory(line.b, self.data.factions.len());
        pb / pa.max(1e-9)
            - 1.0
            - port_fee(&self.data, fa)
            - port_fee(&self.data, fb)
            - tariff(&self.data, fb)
            - self.broker_fee()
    }

    /// Finance departure `k` of line `id` (still at the dock): its profit is shared in
    /// proportion, its losses too.
    pub fn finance_convoy(&mut self, id: u64, k: i64, amount: f64) -> Result<(), &'static str> {
        self.docked
            .ok_or("non amarre : le courtier est a la station")?;
        if self.trade.lanes.stakes.len() >= MAX_STAKES {
            return Err("trop de convois finances");
        }
        let line = lines_at(&self.data, &self.galaxy, self.here_pos)
            .iter()
            .find(|l| l.id == id)
            .cloned()
            .ok_or("ligne inconnue ici")?;
        let c = convoy(&self.data, &self.galaxy, &line, k);
        if c.depart <= self.time {
            return Err("ce convoi est deja parti");
        }
        if self
            .trade
            .lanes
            .stakes
            .iter()
            .any(|s| s.line == id && s.k == k)
        {
            return Err("deja finance");
        }
        if !(100.0..=self.credits).contains(&amount) {
            return Err("fonds insuffisants");
        }
        self.credits -= amount;
        self.trade.lanes.stakes.push(Stake {
            line: id,
            k,
            amount,
            label: format!(
                "{} > {} ({})",
                line.a_name, line.b_name, self.data.resources[line.res].name
            ),
            a: line.a,
        });
        Ok(())
    }

    /// Pay what a financed convoy brings, as the line stands at its arrival.
    fn settle_stake(&self, s: &Stake, line: &Line) -> f64 {
        let c = convoy(&self.data, &self.galaxy, line, s.k);
        let pa = self.reference_price(line.a, c.depart, line.res);
        let pb = self.reference_price(line.b, c.arrive, line.res);
        let fa = self.galaxy.territory(line.a, self.data.factions.len());
        let fb = self.galaxy.territory(line.b, self.data.factions.len());
        let margin = (pb / pa.max(1e-9)
            - 1.0
            - port_fee(&self.data, fa)
            - port_fee(&self.data, fb)
            - tariff(&self.data, fb)
            - self.broker_fee())
        .max(-0.3);
        (s.amount * (1.0 + margin) * (c.delivered() / c.loaded.max(1.0))).max(0.0)
    }

    /// Once a day: financed convoys that arrived pay, and the convoys of the system that were
    /// robbed or held up make the news.
    pub(crate) fn lanes_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.trade.lanes.last_day;
        self.trade.lanes.last_day = day;
        if last == 0 || day <= last {
            return;
        }
        let stakes = std::mem::take(&mut self.trade.lanes.stakes);
        let mut kept = Vec::new();
        for s in stakes {
            // The stake remembers where its line starts: settlement may be far from the dock.
            let Some(line) = lines_at(&self.data, &self.galaxy, s.a)
                .iter()
                .find(|l| l.id == s.line)
                .cloned()
            else {
                kept.push(s);
                continue;
            };
            let c = convoy(&self.data, &self.galaxy, &line, s.k);
            if c.arrive > self.time {
                kept.push(s);
                continue;
            }
            let pay = self.settle_stake(&s, &line);
            self.credits += pay;
            self.events.push(Event::MarketNews(format!(
                "Convoi finance {} : {:+.0} CR ({}).",
                s.label,
                pay - s.amount,
                c.cause.label().to_lowercase()
            )));
        }
        self.trade.lanes.stakes = kept;
        if day - last <= 2 {
            for lv in self.lines_here() {
                for c in &lv.convoys {
                    let t0 = (last as f64) * DAY;
                    if c.lost > 0.0 && c.lost_at > t0 && c.lost_at <= self.time {
                        self.events.push(Event::MarketNews(format!(
                            "LIGNE {} > {} : pirates, {:.0} t de {} perdues.",
                            lv.line.a_name.to_uppercase(),
                            lv.line.b_name.to_uppercase(),
                            c.lost / 1000.0,
                            self.data.resources[lv.line.res].name.to_lowercase()
                        )));
                    }
                    if c.delay_days > 6.0 && c.sched_eta > t0 && c.sched_eta <= self.time {
                        self.events.push(Event::MarketNews(format!(
                            "LIGNE {} > {} : convoi en retard ({}).",
                            lv.line.a_name.to_uppercase(),
                            lv.line.b_name.to_uppercase(),
                            c.cause.label().to_lowercase()
                        )));
                    }
                }
            }
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

    /// Some lines of the galaxy around the origin.
    fn some_lines(d: &GameData, g: &Galaxy, n: usize) -> Vec<Line> {
        let mut out = Vec::new();
        for s in g.nearby([0.0, 0.0, 0.0], 25.0) {
            for l in lines_at(d, g, s.pos).iter() {
                if !out.iter().any(|x: &Line| x.id == l.id) {
                    out.push(l.clone());
                }
            }
            if out.len() >= n {
                break;
            }
        }
        out
    }

    #[test]
    fn lines_exist_are_symmetric_and_deterministic() {
        let d = data();
        let g = Galaxy::new(3);
        let lines = some_lines(&d, &g, 3);
        assert!(!lines.is_empty(), "the neighbourhood holds regular lines");
        for l in &lines {
            assert!(l.dist_ly >= MIN_LY && l.dist_ly <= REACH_LY + 1e-9);
            // Both ends see the same line (same id, same good, same direction).
            let from_b = lines_at(&d, &g, l.b);
            let twin = from_b.iter().find(|x| x.id == l.id).expect("seen from b");
            assert_eq!(twin, l);
            let again = lines_at(&d, &g, l.a);
            assert!(again.iter().any(|x| x == l));
            // The destination people let the good in.
            assert!(admitted(&d, g.territory(l.b, d.factions.len()), l.res));
        }
        assert_eq!(some_lines(&d, &g, 3), lines);
    }

    #[test]
    fn goods_are_conserved_in_transit_never_teleported() {
        let d = data();
        let g = Galaxy::new(3);
        let line = some_lines(&d, &g, 1).remove(0);
        for step in 0..120 {
            let t = (5.0 + step as f64 * 3.7) * DAY;
            let l = ledger(&d, &g, &line, t);
            // Everything shipped is flying, delivered or robbed (conservation).
            assert!(
                (l.shipped - (l.in_transit + l.delivered + l.lost)).abs()
                    < 1e-6 * l.shipped.max(1.0),
                "t={t}: {l:?}"
            );
            assert!(l.in_transit >= -1e-9 && l.delivered >= 0.0 && l.lost >= 0.0);
        }
        // A convoy is in transit between its departure and its arrival, nowhere else.
        let c = convoy(&d, &g, &line, 4);
        assert_eq!(c.in_transit(c.depart - 1.0), 0.0);
        assert!(c.in_transit(0.5 * (c.depart + c.arrive)) > 0.0);
        assert_eq!(c.in_transit(c.arrive + 1.0), 0.0);
        assert!(c.arrive >= c.sched_eta && c.arrive > c.depart);
        assert!((c.delivered() + c.lost - c.loaded).abs() < 1e-9);
    }

    #[test]
    fn a_late_convoy_makes_the_destination_dear_and_arrivals_make_it_cheap() {
        let d = data();
        let g = Galaxy::new(3);
        let line = some_lines(&d, &g, 1).remove(0);
        // Find a late and a robbed convoy among many departures.
        let late = (0..400)
            .map(|k| convoy(&d, &g, &line, k))
            .find(|c| c.delay_days > 4.0);
        // Origin loses stock when a convoy leaves.
        let c = convoy(&d, &g, &line, 10);
        let before = good_factor(&d, &g, line.a, c.depart - 1.0, line.res);
        let after = good_factor(&d, &g, line.a, c.depart + 0.5 * DAY, line.res);
        assert!(
            after > before - 1e-12 || before > 1.0,
            "departure drains the origin"
        );
        // The destination is cheaper once goods arrive than just before.
        let b0 = good_factor(&d, &g, line.b, c.arrive - 0.01 * DAY, line.res);
        let b1 = good_factor(&d, &g, line.b, c.arrive + 0.01 * DAY, line.res);
        assert!(b1 < b0, "arrival lowers the price: {b0} -> {b1}");
        if let Some(c) = late {
            // Past the timetable and before the arrival the shelves are empty: dearer than
            // at the timetable's hour.
            let p0 = good_factor(&d, &g, line.b, c.sched_eta, line.res);
            let p1 = good_factor(&d, &g, line.b, c.arrive - 0.01 * DAY, line.res);
            assert!(p1 >= p0 - 1e-9, "{p0} -> {p1}");
        }
        // Other goods are untouched.
        let other = (0..d.resources.len()).find(|r| *r != line.res).unwrap();
        assert!((good_factor(&d, &g, line.b, c.arrive, other) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn delays_follow_epidemic_war_and_blockade_and_pirates_follow_the_zone() {
        let d = data();
        let mut delayed = [0usize; 4];
        let (mut robbed, mut total) = (0, 0);
        for seed in 1..5u64 {
            let g = Galaxy::new(seed);
            for line in some_lines(&d, &g, 6) {
                for k in 0..120 {
                    let c = convoy(&d, &g, &line, k);
                    total += 1;
                    if c.lost > 0.0 {
                        robbed += 1;
                        assert!(c.lost < c.loaded);
                    }
                    let i = match c.cause {
                        Delay::None => 0,
                        Delay::Quarantine => 1,
                        Delay::War => 2,
                        Delay::Blockade => 3,
                    };
                    delayed[i] += 1;
                    if c.cause != Delay::None {
                        assert!(c.delay_days >= 0.5 * SICK_DELAY.min(WAR_DELAY));
                    }
                }
            }
        }
        assert!(
            delayed[0] > 0 && delayed[1] + delayed[2] + delayed[3] > 0,
            "{delayed:?}"
        );
        assert!(robbed > 0 && robbed < total, "{robbed}/{total}");
    }

    #[test]
    fn the_named_cause_of_a_delay_is_really_there_at_an_end_of_the_line() {
        use crate::trading::risk_at;
        let d = data();
        let (mut sick, mut war, mut siege) = (0, 0, 0);
        for seed in 1..6u64 {
            let g = Galaxy::new(seed);
            for line in some_lines(&d, &g, 6) {
                for k in 0..150 {
                    let c = convoy(&d, &g, &line, k);
                    let ra = risk_at(&d, &g, seed, line.a, c.depart);
                    let rb = risk_at(&d, &g, seed, line.b, c.sched_eta);
                    match c.cause {
                        Delay::None => {
                            assert!(
                                !(ra.sick > 0.02
                                    || rb.sick > 0.02
                                    || ra.war
                                    || rb.war
                                    || ra.siege
                                    || rb.siege),
                                "a convoy through trouble is not on time by magic"
                            );
                        }
                        Delay::Quarantine => {
                            sick += 1;
                            assert!(ra.sick > 0.02 || rb.sick > 0.02);
                        }
                        Delay::War => {
                            war += 1;
                            assert!(ra.war || rb.war);
                        }
                        Delay::Blockade => {
                            siege += 1;
                            assert!(ra.siege || rb.siege);
                        }
                    }
                }
            }
        }
        assert!(sick + war + siege > 0, "{sick} {war} {siege}");
    }

    #[test]
    fn customs_and_ports_are_read_from_the_people_who_hold_the_place() {
        let d = data();
        assert_eq!(tariff(&d, None), 0.0);
        let rich = (0..d.factions.len())
            .max_by(|a, b| d.factions[*a].tax.total_cmp(&d.factions[*b].tax))
            .unwrap();
        let poor = (0..d.factions.len())
            .min_by(|a, b| d.factions[*a].tax.total_cmp(&d.factions[*b].tax))
            .unwrap();
        assert!(tariff(&d, Some(rich)) >= tariff(&d, Some(poor)));
        assert!(port_fee(&d, None) <= port_fee(&d, Some(rich)));
        // Contraband of a faction never rides a regular line into its space.
        for f in 0..d.factions.len() {
            for r in crate::economy::banned_goods_of(&d, f) {
                assert!(!admitted(&d, Some(f), r));
            }
        }
    }

    #[test]
    fn a_financed_convoy_pays_back_with_the_margin_and_the_losses() {
        let d = data();
        let mut s = new_sim(5, d.clone());
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        let g = s.galaxy;
        // Search a start with a line at the ship's own system: move the ship to a hub.
        let hub = g
            .nearby([0.0, 0.0, 0.0], 25.0)
            .into_iter()
            .find(|st| !lines_at(&d, &g, st.pos).is_empty())
            .expect("a hub with a line");
        s.here_pos = hub.pos;
        s.time = 40.0 * DAY;
        let view = s.lines_here();
        let lv = view
            .iter()
            .find(|v| v.outbound)
            .or(view.first())
            .unwrap()
            .clone();
        let upcoming = lv
            .convoys
            .iter()
            .find(|c| c.depart > s.time)
            .expect("a convoy still at the dock")
            .clone();
        let credits = s.credits;
        s.finance_convoy(lv.line.id, upcoming.k, 1000.0)
            .expect("financed");
        assert_eq!(s.credits, credits - 1000.0);
        assert_eq!(s.trade.lanes.stakes.len(), 1);
        assert!(
            s.finance_convoy(lv.line.id, upcoming.k, 1000.0).is_err(),
            "once"
        );
        // Until it arrives nothing is paid.
        s.trade.lanes.last_day = (s.time / DAY) as i64;
        s.time = upcoming.depart + 0.1 * DAY;
        s.lanes_tick();
        assert_eq!(s.trade.lanes.stakes.len(), 1);
        // After it arrives the stake is settled for the margin on the goods that got through.
        s.time = upcoming.arrive + 2.0 * DAY;
        let want = {
            let st = s.trade.lanes.stakes[0].clone();
            s.settle_stake(&st, &lv.line)
        };
        let before = s.credits;
        s.lanes_tick();
        assert!(s.trade.lanes.stakes.is_empty());
        assert!(
            (s.credits - before - want).abs() < 1e-6,
            "{} vs {want}",
            s.credits - before
        );
        // The payout is proportional: delivered share bounds it.
        assert!(want <= 1000.0 * (1.0 + 3.0) * upcoming.delivered() / upcoming.loaded + 1e-6);
    }
}
