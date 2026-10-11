//! The commodity exchange: world events (wars, famines, strikes, booms...) and market noise move
//! prices. Everything is a pure function of the galaxy seed, the place and the date, so nothing is
//! stored: the same event happens at the same time for every visitor, a chart of the last 90 days
//! can be drawn at any moment, and a saved game needs nothing more.
use crate::planet::unit;
use crate::{Event, Sim};
use vector_core::hash_coord;
use vector_data::GameData;

pub const DAY: f64 = 86_400.0;
pub(crate) const WEEK: f64 = 7.0 * DAY;
/// Events last at most 90 days, so only the previous 14 weeks can still be running.
const LOOKBACK_WEEKS: i64 = 14;
/// Chance that a week brings an event to a region / to the whole galaxy.
const P_REGION: f64 = 0.55;
const P_GLOBAL: f64 = 0.18;
/// Side of a region of space, ly (same as the faction territories).
const REGION_LY: f64 = 60.0;

/// An event on the calendar of a place: index into `GameData::world_events`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveEvent {
    pub def: usize,
    pub start: f64,
    pub end: f64,
    pub global: bool,
    /// 0.6 .. 1.3, how hard this occurrence hits.
    pub strength: f64,
}

impl ActiveEvent {
    /// Share of the peak effect at time `t`: 0 outside, rising over the first fifth of the event,
    /// full, then falling over the last third.
    pub fn profile(&self, t: f64) -> f64 {
        if t < self.start || t > self.end {
            return 0.0;
        }
        let x = (t - self.start) / (self.end - self.start).max(1.0);
        let smooth = |v: f64| v * v * (3.0 - 2.0 * v);
        if x < 0.2 {
            smooth(x / 0.2)
        } else if x > 0.67 {
            smooth((1.0 - x) / 0.33)
        } else {
            1.0
        }
    }
}

fn region_cell(pos: [f64; 3]) -> [i64; 3] {
    pos.map(|v| (v / REGION_LY).floor() as i64)
}

/// Events (of the region around `pos` and of the whole galaxy) whose life overlaps [t0, t1].
pub fn events_between(
    data: &GameData,
    galaxy_seed: u64,
    pos: [f64; 3],
    t0: f64,
    t1: f64,
) -> Vec<ActiveEvent> {
    let cell = region_cell(pos);
    let mut out = Vec::new();
    let (w0, w1) = (
        (t0 / WEEK).floor() as i64 - LOOKBACK_WEEKS,
        (t1 / WEEK).floor() as i64,
    );
    for global in [false, true] {
        let scope = if global { "global" } else { "region" };
        let defs: Vec<usize> = (0..data.world_events.len())
            .filter(|&i| data.world_events[i].scope == scope)
            .collect();
        let total: f64 = defs.iter().map(|&i| data.world_events[i].weight).sum();
        if defs.is_empty() || total <= 0.0 {
            continue;
        }
        let base = if global {
            hash_coord(galaxy_seed ^ 0x6106, 1, 2, 3)
        } else {
            hash_coord(galaxy_seed ^ 0xB0B5, cell[0], cell[1], cell[2])
        };
        for w in w0..=w1 {
            let h = |k: i64| hash_coord(base, w, k, 0);
            if unit(h(0)) >= if global { P_GLOBAL } else { P_REGION } {
                continue;
            }
            let mut pick = unit(h(1)) * total;
            let mut def = defs[0];
            for &i in &defs {
                let wt = data.world_events[i].weight;
                if pick < wt {
                    def = i;
                    break;
                }
                pick -= wt;
            }
            let e = &data.world_events[def];
            let start = w as f64 * WEEK + unit(h(2)) * WEEK;
            let end = start + (e.days.0 + (e.days.1 - e.days.0) * unit(h(3))) * DAY;
            if end >= t0 && start <= t1 {
                out.push(ActiveEvent {
                    def,
                    start,
                    end,
                    global,
                    strength: 0.6 + 0.7 * unit(h(4)),
                });
            }
        }
    }
    out
}

/// Smooth noise in [-1, 1] with the given period (days), different for every key.
pub(crate) fn smooth_noise(seed: u64, key: i64, days: f64, t: f64) -> f64 {
    let x = t / DAY / days;
    let (i, f) = (x.floor() as i64, x - x.floor());
    let s = f * f * (3.0 - 2.0 * f);
    let v = |k: i64| unit(hash_coord(seed, key, k, 0x5EED)) * 2.0 - 1.0;
    v(i) * (1.0 - s) + v(i + 1) * s
}

/// Combined effect of events on one resource at time `t` (1 when none applies).
pub fn event_factor(data: &GameData, events: &[ActiveEvent], res: usize, t: f64) -> f64 {
    events
        .iter()
        .map(|ev| {
            let m = data.world_events[ev.def].effect_on(&data.resources, res);
            1.0 + (m - 1.0) * ev.strength * ev.profile(t)
        })
        .product()
}

/// Price multiplier of every resource at `pos` and time `t`: the active events' effects plus a
/// slow random drift (more volatile for dear goods), around 1.
pub fn world_factors(data: &GameData, galaxy_seed: u64, pos: [f64; 3], t: f64) -> Vec<f64> {
    let events = events_between(data, galaxy_seed, pos, t, t);
    let cell = region_cell(pos);
    // What the people of this territory want and shun (their mœurs, see `data/ethos.ron`).
    let faction =
        vector_world::galaxy::Galaxy::new(galaxy_seed).territory(pos, data.factions.len());
    let values = data
        .ethos
        .profile(faction.map(|f| data.factions[f].id.as_str()));
    let region_key = hash_coord(galaxy_seed ^ 0xF1A7, cell[0], cell[1], cell[2]) as i64;
    (0..data.resources.len())
        .map(|r| {
            let f = event_factor(data, &events, r, t)
                * data.ethos.demand_factor(&values, &data.resources, r);
            let vol = 0.03 + 0.012 * (1.0 + data.resources[r].value).log10();
            let key = region_key ^ (r as i64).wrapping_mul(0x9E37_79B9);
            let n = 0.65 * smooth_noise(galaxy_seed, key, 9.0, t)
                + 0.35 * smooth_noise(galaxy_seed ^ 1, key, 31.0, t);
            f * (1.0 + vol * 2.0 * n) * crate::trading::cartel_factor(data, galaxy_seed, pos, t, r)
        })
        .collect()
}

impl Sim {
    /// Reference prices of every resource on station `station`'s market, day by day over the last
    /// `days` days ending now (`[resource][day]`, oldest first): the market's usual level (its
    /// equilibrium stock) times what the world was doing that day. This is the "reference curve"
    /// of the exchange; the live price also depends on the stock the player left behind.
    pub fn price_series(&mut self, station: usize, days: usize) -> Vec<Vec<f64>> {
        let data = std::sync::Arc::clone(&self.data);
        let (seed, pos, now) = (self.galaxy.seed, self.here_pos, self.time);
        let m = self.market(station).clone();
        let n = data.resources.len();
        let mut out = vec![Vec::with_capacity(days); n];
        for d in 0..days {
            let t = now - (days - 1 - d) as f64 * DAY;
            let f = world_factors(&data, seed, pos, t);
            for (r, series) in out.iter_mut().enumerate() {
                let scarcity = (m.reference[r] / m.target[r].max(0.03 * m.reference[r]))
                    .powf(0.35)
                    .clamp(0.25, 4.0);
                series.push(data.resources[r].value * m.modifier[r] * scarcity * f[r]);
            }
        }
        out
    }

    /// Events running around the player now, and those that ended in the last `back_days` days.
    pub fn market_news(&self, back_days: f64) -> Vec<ActiveEvent> {
        let mut v = events_between(
            &self.data,
            self.galaxy.seed,
            self.here_pos,
            self.time - back_days * DAY,
            self.time,
        );
        v.sort_by(|a, b| b.start.total_cmp(&a.start));
        v
    }

    /// Announce the events that began since the last day boundary (a few days at most: under time
    /// acceleration the news would only be noise).
    pub(crate) fn market_news_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.last_news_day;
        self.last_news_day = day;
        if last == i64::MIN || day <= last || day - last > 3 {
            return;
        }
        let (from, to) = ((last + 1) as f64 * DAY, (day + 1) as f64 * DAY);
        let events = events_between(&self.data, self.galaxy.seed, self.here_pos, from, to);
        for ev in events
            .into_iter()
            .filter(|e| e.start >= from - DAY && e.start < to)
        {
            let def = &self.data.world_events[ev.def];
            if ev.start > last as f64 * DAY && ev.start <= self.time {
                self.events.push(Event::MarketNews(format!(
                    "BOURSE : {} - {}",
                    def.name, def.text
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn all_events(d: &GameData, pos: [f64; 3], years: f64) -> Vec<ActiveEvent> {
        let mut v = events_between(d, 7, pos, 0.0, years * 365.0 * DAY);
        v.sort_by(|a, b| a.start.total_cmp(&b.start));
        v.dedup();
        v
    }

    #[test]
    fn events_are_deterministic_frequent_and_differ_between_regions() {
        let d = data();
        let a = all_events(&d, [5.0, 5.0, 5.0], 2.0);
        assert_eq!(a, all_events(&d, [5.0, 5.0, 5.0], 2.0));
        // About 0.55 region events a week plus 0.18 global ones over two years.
        assert!(
            (50..=110).contains(&a.len()),
            "{} events in two years",
            a.len()
        );
        assert!(a.iter().any(|e| e.global) && a.iter().any(|e| !e.global));
        let b = all_events(&d, [500.0, -300.0, 40.0], 2.0);
        let regional = |v: &[ActiveEvent]| -> Vec<(usize, i64)> {
            v.iter()
                .filter(|e| !e.global)
                .map(|e| (e.def, e.start as i64))
                .collect()
        };
        assert_ne!(
            regional(&a),
            regional(&b),
            "regions have their own histories"
        );
        let global = |v: &[ActiveEvent]| -> Vec<(usize, i64)> {
            v.iter()
                .filter(|e| e.global)
                .map(|e| (e.def, e.start as i64))
                .collect()
        };
        assert_eq!(
            global(&a),
            global(&b),
            "global events are the same everywhere"
        );
    }

    #[test]
    fn an_event_ramps_up_holds_and_fades_and_moves_the_right_prices() {
        let d = data();
        let pos = [5.0, 5.0, 5.0];
        let all = all_events(&d, pos, 3.0);
        // A regional event with a named resource in its effects, somewhere in three years.
        let ev = all
            .iter()
            .find(|e| {
                !e.global
                    && d.world_events[e.def]
                        .effects
                        .iter()
                        .any(|(k, _)| !k.starts_with("cat:"))
            })
            .expect("such an event in three years")
            .clone();
        let def = &d.world_events[ev.def];
        let (target, mult) = def
            .effects
            .iter()
            .find(|(k, _)| !k.starts_with("cat:"))
            .cloned()
            .expect("effect");
        let res = d.resource_index(&target).expect("res");
        let peak = (ev.start + ev.end) / 2.0;
        let only = [ev.clone()];
        // Nothing before or after, the full effect (scaled by its strength) in the middle.
        assert_eq!(event_factor(&d, &only, res, ev.start - 3.0 * DAY), 1.0);
        assert_eq!(event_factor(&d, &only, res, ev.end + 3.0 * DAY), 1.0);
        let at_peak = event_factor(&d, &only, res, peak);
        assert!(
            (at_peak - (1.0 + (mult - 1.0) * ev.strength)).abs() < 1e-12,
            "{at_peak}"
        );
        // The ramp is monotonic towards the peak.
        let early = event_factor(&d, &only, res, ev.start + 0.05 * (ev.end - ev.start));
        let later = event_factor(&d, &only, res, ev.start + 0.15 * (ev.end - ev.start));
        if mult > 1.0 {
            assert!(1.0 < early && early < later && later <= at_peak + 1e-12);
        } else {
            assert!(1.0 > early && early > later && later >= at_peak - 1e-12);
        }
        // A resource the event does not mention is untouched by it.
        let untouched = (0..d.resources.len())
            .find(|&r| def.effect_on(&d.resources, r) == 1.0)
            .expect("an unaffected resource");
        assert_eq!(event_factor(&d, &only, untouched, peak), 1.0);
        // And the full price factor follows the event, within the noise (<= 8 %).
        let other: Vec<ActiveEvent> = all
            .iter()
            .filter(|e| (e.start..=e.end).contains(&peak) && **e != ev)
            .cloned()
            .collect();
        let expect = at_peak * event_factor(&d, &other, res, peak);
        let got = world_factors(&d, 7, pos, peak)[res]
            / crate::trading::cartel_factor(&d, 7, pos, peak, res);
        assert!((got / expect - 1.0).abs() < 0.12, "{got} vs {expect}");
    }
    /// A point of space inside a faction's territory.
    fn position_in(d: &GameData, seed: u64, faction: &str) -> [f64; 3] {
        let g = vector_world::galaxy::Galaxy::new(seed);
        let idx = d.faction_index(faction).expect("faction");
        for x in (-600..600).step_by(40) {
            for y in (-300..300).step_by(40) {
                for z in (-300..300).step_by(40) {
                    let p = [f64::from(x), f64::from(y), f64::from(z)];
                    if g.territory(p, d.factions.len()) == Some(idx) {
                        return p;
                    }
                }
            }
        }
        panic!("no territory of {faction}");
    }

    #[test]
    fn the_mores_of_a_people_set_what_its_markets_pay() {
        let d = data();
        let arms = d.resource_index("weapon_parts").expect("res");
        let tech = d.resource_index("electronics").expect("res");
        let (war, green) = (
            position_in(&d, 7, "hegemonie"),
            position_in(&d, 7, "verdant"),
        );
        let mean = |pos: [f64; 3], res: usize| -> f64 {
            (0..300)
                .map(|day| world_factors(&d, 7, pos, f64::from(day) * 3.0 * DAY)[res])
                .sum::<f64>()
                / 300.0
        };
        // Militarists pay far more for weapons than pacifists, whatever the events.
        assert!(
            mean(war, arms) > 1.25 * mean(green, arms),
            "{} vs {}",
            mean(war, arms),
            mean(green, arms)
        );
        // Progressives want technology more than traditionalists.
        let mars = position_in(&d, 7, "mars");
        let errants = position_in(&d, 7, "errants");
        assert!(
            mean(mars, tech) > mean(errants, tech),
            "{} vs {}",
            mean(mars, tech),
            mean(errants, tech)
        );
        // The factor of a neutral people is 1.
        let neutral = vec![0.0; d.ethos.axes.len()];
        assert_eq!(d.ethos.demand_factor(&neutral, &d.resources, arms), 1.0);
    }

    #[test]
    fn noise_stays_small_and_events_never_make_prices_negative() {
        let d = data();
        for res in 0..d.resources.len() {
            for day in (0..800).step_by(7) {
                let f = world_factors(&d, 3, [1.0, 2.0, 3.0], f64::from(day) * DAY)[res];
                assert!(f.is_finite() && f > 0.15 && f < 6.0, "{res} day {day}: {f}");
            }
        }
    }

    #[test]
    fn market_prices_follow_the_world_and_the_spread_still_costs_money() {
        let d = data();
        let mut m = crate::market::Market::new(1, 5, &d, 0.0, 0);
        let iron = d.resource_index("iron").expect("iron");
        let base = m.price(&d, iron);
        m.world = vec![1.5; d.resources.len()];
        assert!((m.price(&d, iron) / base - 1.5).abs() < 1e-9);
        let (kg, cost) = m.buy(&d, iron, 500.0, 1.0);
        let back = m.sell(&d, iron, kg, 1.0);
        assert!(back < cost, "{back} vs {cost}");
        assert!(m.bid(&d, iron) < m.ask(&d, iron));
    }
}
