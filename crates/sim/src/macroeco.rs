//! Macroeconomy of the sectors. Like politics, it is local and pure: every sector of 240 ly has a
//! currency (that of the people who hold it), a real output (GDP) that follows the stars it
//! holds, a business cycle, wars and unrest; a money supply that grows faster in war; and from
//! these a price level (quantity theory: price level = money / output), an inflation, a policy
//! rate, unemployment, a debt ratio, a floating exchange rate against the credit and, now and
//! then, a bubble followed by a crash. All of it is a function of (seed, sector, date): the same
//! crash hits everybody on the same day. The player meets it through the bank (loan rate, credit
//! ceiling), the stock exchange (bubbles and crashes move the shares), foreign currencies and
//! the monthly report.
use crate::bourse::DAY;
use crate::planet::unit;
use crate::politics::{self, Cell};
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Length of the window in which a crash may start, days.
pub const CRASH_PERIOD: f64 = 400.0;
/// Chance that a window holds a crash.
pub const CRASH_CHANCE: f64 = 0.12;
/// Days of rising bubble before a crash.
pub const BUBBLE_DAYS: f64 = 150.0;
/// Fee of a currency exchange.
pub const FX_FEE: f64 = 0.005;
/// Crash depth above which listed companies fail (cascade).
pub const CASCADE_DEPTH: f64 = 0.22;

/// The state of a sector's economy at one date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    pub faction: Option<usize>,
    /// Real output, credits per day.
    pub gdp: f64,
    /// Output the sector would make with no cycle, war or crash.
    pub potential: f64,
    /// Growth of the money supply per day.
    pub money_growth: f64,
    /// Annual inflation (price level growth).
    pub inflation: f64,
    /// Annual policy rate of the central bank.
    pub policy_rate: f64,
    pub unemployment: f64,
    /// Public debt as a share of a year of output.
    pub debt_ratio: f64,
    /// Value of one unit of the local currency in credits.
    pub fx: f64,
    /// 0..0.35: depth of the running crash.
    pub crash: f64,
    /// 0..0.4: share prices above their fundamentals.
    pub bubble: f64,
    pub at_war: bool,
    pub unrest: f64,
    /// Share of `gdp` that comes from the production of the colonies and industries (credits/day).
    pub production: f64,
    /// Harvest of the year: 1 is a normal one (see `harvest`).
    pub harvest: f64,
}

/// Annual inflation from the daily growth of money and of output.
pub fn inflation_of(money_growth: f64, output_growth: f64) -> f64 {
    ((money_growth - output_growth) * 365.0).clamp(-0.3, 3.0)
}

/// Daily money growth: a calm base plus the printing press of war, unrest and crisis relief.
pub fn money_growth_of(base: f64, at_war: bool, unrest: f64, crash: f64) -> f64 {
    base + if at_war { 4.0e-4 } else { 0.0 } + 2.0e-4 * unrest + 5.0e-4 * crash
}

/// Annual policy rate from inflation and the output gap (a Taylor rule).
pub fn policy_rate_of(inflation: f64, gap: f64) -> f64 {
    (0.02 + 1.5 * (inflation - 0.02) + 1.0 * gap).clamp(0.0, 0.6)
}

/// `principal` grown by compound interest at `daily` over `days` days.
pub fn compound(principal: f64, daily: f64, days: i64) -> f64 {
    principal * (1.0 + daily).powi(days.clamp(0, 36_500) as i32)
}

fn h(seed: u64, c: Cell, k: i64) -> f64 {
    unit(hash_coord(
        seed ^ 0xEC0_0001,
        c[0] ^ (c[1] << 20),
        c[2] ^ (c[1] << 40),
        k,
    ))
}

/// (crash depth, bubble) of a sector at time `t`.
pub fn crisis(seed: u64, c: Cell, t: f64) -> (f64, f64) {
    let day = t / DAY;
    let n = (day / CRASH_PERIOD).floor() as i64;
    let (mut crash, mut bubble) = (0.0_f64, 0.0_f64);
    for w in [n - 1, n, n + 1] {
        if h(seed, c, 1000 + w) >= CRASH_CHANCE {
            continue;
        }
        let start = w as f64 * CRASH_PERIOD + BUBBLE_DAYS + 50.0 + 150.0 * h(seed, c, 2000 + w);
        let depth = 0.12 + 0.23 * h(seed, c, 3000 + w);
        if day >= start {
            crash = crash.max(depth * (-(day - start) / 90.0).exp());
        } else if day >= start - BUBBLE_DAYS {
            bubble = bubble.max(0.4 * (depth / 0.35) * (day - (start - BUBBLE_DAYS)) / BUBBLE_DAYS);
        }
    }
    (crash, bubble)
}

/// Worlds settled in a sector: they follow the stars it holds.
pub fn settled_worlds(seed: u64, c: Cell) -> f64 {
    let stars = Galaxy::density(politics::cell_center(c));
    stars * 1.6 * (0.6 + 0.8 * h(seed, c, 7))
}

/// Harvest of the year in sector `c`: a seasonal wave around 1, with drought and flood years
/// (a function of the sector and the year, so the whole region eats the same bread).
pub fn harvest(seed: u64, c: Cell, t: f64) -> f64 {
    let day = t / DAY;
    let year = (day / 365.0).floor() as i64;
    let season = 0.12 * (std::f64::consts::TAU * (day / 365.0 + h(seed, c, 8))).sin();
    let r = h(seed, c, 4000 + year);
    let year_mult = if r < 0.10 {
        0.6 // drought or blight
    } else if r < 0.18 {
        1.2 // bumper year
    } else {
        1.0
    };
    ((1.0 + season) * year_mult).clamp(0.4, 1.4)
}

/// Whether an industry feeds the people (its output follows the harvest).
pub fn is_agricultural(data: &GameData, i: usize) -> bool {
    data.industries[i]
        .produces
        .iter()
        .any(|p| p == "food" || p == "organics")
}

/// Output of the colonies and industries of a sector, credits per day, by industry: the settled
/// worlds are shared between the industries of `industries.ron`, each world makes a value that
/// follows what its industry produces, farms follow the harvest and an epidemic keeps workers
/// home. Returns (per industry, potential without harvest or sickness).
pub fn production(data: &GameData, seed: u64, c: Cell, t: f64) -> (Vec<f64>, f64) {
    let worlds = settled_worlds(seed, c);
    let n = data.industries.len().max(1);
    let weights: Vec<f64> = (0..n).map(|i| 0.4 + h(seed, c, 10 + i as i64)).collect();
    let total: f64 = weights.iter().sum();
    let harvest = harvest(seed, c, t);
    let sick = crate::epidemic::prevalence_at(data, seed, politics::cell_center(c), t)
        .iter()
        .map(|(_, i)| *i)
        .sum::<f64>();
    let labour = 1.0 - 0.5 * (sick / 0.2).min(1.0);
    let mut potential = 0.0;
    let out: Vec<f64> = (0..data.industries.len())
        .map(|i| {
            let ind = &data.industries[i];
            let vals: Vec<f64> = ind
                .produces
                .iter()
                .filter_map(|p| data.resource_index(p))
                .map(|r| data.resources[r].value)
                .collect();
            let mean = vals.iter().sum::<f64>() / vals.len().max(1) as f64;
            let per_world = 140.0 * (1.0 + 0.5 * (1.0 + mean).ln());
            let base = worlds * weights[i] / total * per_world;
            potential += base;
            base * labour
                * if is_agricultural(data, i) {
                    harvest
                } else {
                    1.0
                }
        })
        .collect();
    (out, potential)
}

/// Real output of a sector: its services and its production make the base, the cycle, war,
/// unrest and crash bend it. Returns (gdp, potential, production share of gdp).
#[allow(clippy::too_many_arguments)]
fn output(
    data: &GameData,
    seed: u64,
    c: Cell,
    t: f64,
    war: bool,
    unrest: f64,
    crash: f64,
    extra: f64,
) -> (f64, f64, f64) {
    let stars = Galaxy::density(politics::cell_center(c));
    let services = (1_500.0 + 300.0 * stars) * (0.7 + 0.6 * h(seed, c, 1));
    let (prod, prod_potential) = production(data, seed, c, t);
    let prod: f64 = prod.iter().sum();
    let period = 700.0 + 800.0 * h(seed, c, 2);
    let phase = std::f64::consts::TAU * h(seed, c, 3);
    let gap = 0.08 * (std::f64::consts::TAU * t / DAY / period + phase).sin();
    let bend =
        (1.0 + gap) * (1.0 - crash) * (1.0 - if war { 0.12 } else { 0.0 }) * (1.0 - 0.08 * unrest);
    let gdp = (services + prod + extra) * bend;
    (gdp, services + prod_potential + extra, prod * bend)
}

/// The economy of sector `c` at time `t`.
pub fn sector(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> Sector {
    sector_with(data, galaxy, seed, c, t, 0.0)
}

/// The economy of sector `c` at time `t` with `extra` credits a day of output from outside the
/// generated world (the player's colonies and the migrants they took in).
pub fn sector_with(
    data: &GameData,
    galaxy: &Galaxy,
    seed: u64,
    c: Cell,
    t: f64,
    extra: f64,
) -> Sector {
    let war = politics::sector_at_war(data, galaxy, seed, c, t);
    let unrest = politics::unrest(seed, c, t);
    let (crash, bubble) = crisis(seed, c, t);
    let (gdp, potential, production) = output(data, seed, c, t, war, unrest, crash, extra);
    // Output growth over the last year, per day (the harvest has its seasons: a month would
    // read them as inflation).
    let (crash0, _) = crisis(seed, c, t - 365.0 * DAY);
    let (gdp0, _, _) = output(data, seed, c, t - 365.0 * DAY, war, unrest, crash0, extra);
    let g_q = (gdp / gdp0.max(1e-9)).ln() / 365.0;
    let base = 2.0e-5 + 1.0e-4 * h(seed, c, 4);
    let money_growth = money_growth_of(base, war, unrest, crash);
    let inflation = inflation_of(money_growth, g_q);
    let gap = gdp / potential - 1.0;
    let faction = politics::faction_of(data, galaxy, c);
    let fx = if faction.is_some() {
        ((0.7 + 0.6 * h(seed, c, 5))
            * (-0.5 * inflation).exp()
            * (1.0 - if war { 0.25 } else { 0.0 })
            * (1.0 - 0.1 * unrest)
            * (1.0 - 0.6 * crash))
            .clamp(0.1, 3.0)
    } else {
        1.0
    };
    Sector {
        faction,
        gdp,
        potential,
        money_growth,
        inflation,
        policy_rate: policy_rate_of(inflation, gap),
        unemployment: (0.06 - gap + 0.4 * crash).clamp(0.02, 0.6),
        debt_ratio: (0.3 + 0.5 * h(seed, c, 6) + if war { 0.15 } else { 0.0 } + 0.6 * crash)
            .clamp(0.0, 2.5),
        fx,
        crash,
        bubble,
        at_war: war,
        unrest,
        production,
        harvest: harvest(seed, c, t),
    }
}

impl Sector {
    /// Daily rate the bank charges here: the policy rate over a floor, scaled around the
    /// historical flat rate of the game.
    pub fn bank_rate_per_day(&self) -> f64 {
        crate::economy::BANK_RATE_PER_DAY * (0.6 + 4.0 * self.policy_rate)
    }

    /// Multiplier on the credit ceiling: banks lend less in a crash.
    pub fn credit_factor(&self) -> f64 {
        (1.0 - 1.5 * self.crash).max(0.3)
    }

    /// Multiplier on the share prices of companies headquartered here: bubbles inflate them,
    /// crashes sink them, and a deep crash brings failures in cascade.
    pub fn share_factor(&self) -> f64 {
        let f = (1.0 + self.bubble) * (1.0 - 1.2 * self.crash);
        if self.crash > CASCADE_DEPTH {
            f * 0.5
        } else {
            f
        }
    }
}

impl Sector {
    /// Annual yield of the sovereign bonds: the policy rate plus a risk premium that grows with
    /// debt, war and crash.
    pub fn sovereign_yield(&self) -> f64 {
        (self.policy_rate
            + 0.02 * self.debt_ratio
            + if self.at_war { 0.03 } else { 0.0 }
            + 0.3 * self.crash)
            .clamp(0.005, 0.9)
    }
}

/// The history of a sector figure: `n` samples ending at `t`, `step_days` apart, oldest first.
/// Every figure of a sector is a function of the date, so the past is computed, not stored.
#[allow(clippy::too_many_arguments)]
pub fn history(
    data: &GameData,
    galaxy: &Galaxy,
    seed: u64,
    c: Cell,
    t: f64,
    step_days: f64,
    n: usize,
    f: impl Fn(&Sector) -> f64,
) -> Vec<f64> {
    (0..n)
        .map(|k| {
            let when = t - (n - 1 - k) as f64 * step_days * DAY;
            f(&sector(data, galaxy, seed, c, when))
        })
        .collect()
}

/// How much tourists like the place around `pos` (0..1): beauty of the sector, safety (war,
/// unrest, the aggression of its people) and health (no epidemic running).
pub fn tourism(data: &GameData, galaxy: &Galaxy, seed: u64, pos: [f64; 3], t: f64) -> f64 {
    let c = politics::cell_of(pos);
    let beauty = 0.3 + 0.7 * h(seed, c, 20);
    let aggression =
        politics::faction_of(data, galaxy, c).map_or(0.5, |f| data.factions[f].aggression);
    let safety = (1.0
        - 0.6 * aggression
        - if politics::sector_at_war(data, galaxy, seed, c, t) {
            0.4
        } else {
            0.0
        }
        - 0.3 * politics::unrest(seed, c, t))
    .clamp(0.0, 1.0);
    let sick = crate::epidemic::prevalence_at(data, seed, pos, t)
        .iter()
        .map(|(_, i)| *i)
        .sum::<f64>();
    let health = 1.0 - (sick / 0.1).min(1.0);
    (beauty * safety * health).clamp(0.0, 1.0)
}

/// Hotel rating (0..5 stars) of station `station` of a system, from the tourism of its region.
pub fn hotel_stars(tourism: f64, system_seed: u64, station: usize) -> u8 {
    let j = unit(hash_coord(system_seed ^ 0x407E1, station as i64, 3, 1));
    (5.0 * tourism * (0.5 + 0.5 * j)).round().clamp(0.0, 5.0) as u8
}

/// Price multiplier of food and water in a sector from the harvest of its year: a bad year
/// makes bread dear, a good one cheap.
pub fn harvest_goods_factor(data: &GameData, seed: u64, pos: [f64; 3], t: f64, res: usize) -> f64 {
    let id = data.resources[res].id.as_str();
    if id == "food" || id == "organics" {
        harvest(seed, politics::cell_of(pos), t).powf(-0.8)
    } else {
        1.0
    }
}

/// What the economic board of a jurisdiction shows: the sector now and its history.
#[derive(Clone, Debug)]
pub struct Board {
    pub cell: Cell,
    pub sector: Sector,
    pub currency: String,
    /// Customs tariff and port fee of the people who hold the place.
    pub tariff: f64,
    pub port_fee: f64,
    pub tourism: f64,
    /// Output of each industry now, credits a day.
    pub by_industry: Vec<f64>,
    /// 24 months, oldest first.
    pub gdp: Vec<f64>,
    pub inflation: Vec<f64>,
    pub fx: Vec<f64>,
    pub rate: Vec<f64>,
    /// Reference price of one good in the jurisdiction (24 months), and which good.
    pub price: Vec<f64>,
    pub price_res: usize,
}

/// Name of the currency of a people.
pub fn currency_name(data: &GameData, faction: Option<usize>) -> String {
    match faction {
        Some(f) => format!("CREDIT {}", data.factions[f].name.to_uppercase()),
        None => "CREDIT LIBRE".to_string(),
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

    #[test]
    fn inflation_follows_the_growth_of_money_over_the_growth_of_output() {
        // Inflation rises with money growth at constant output.
        assert!(inflation_of(2.0e-4, 0.0) > inflation_of(1.0e-4, 0.0));
        // Money growing as fast as output means stable prices.
        assert!(inflation_of(1.0e-4, 1.0e-4).abs() < 1e-12);
    }

    #[test]
    fn war_prints_money_and_inflation_follows() {
        let calm = money_growth_of(5.0e-5, false, 0.0, 0.0);
        let war = money_growth_of(5.0e-5, true, 0.0, 0.0);
        assert!(war > calm);
        assert!(inflation_of(war, 0.0) > inflation_of(calm, 0.0) + 0.1);
        assert!(
            policy_rate_of(inflation_of(war, 0.0), 0.0)
                > policy_rate_of(inflation_of(calm, 0.0), 0.0)
        );
    }

    #[test]
    fn compound_interest_is_exact() {
        let loop_value = (0..30).fold(1000.0, |v, _| v * 1.002);
        assert!((compound(1000.0, 0.002, 30) - loop_value).abs() < 1e-9);
        assert_eq!(compound(1000.0, 0.002, 0), 1000.0);
    }

    #[test]
    fn sectors_are_deterministic_and_sane() {
        let d = data();
        let g = Galaxy::new(7);
        let mut crashed = false;
        for x in -2..3 {
            for k in 0..60 {
                let t = k as f64 * 30.0 * DAY;
                let a = sector(&d, &g, 7, [x, 0, 0], t);
                let b = sector(&d, &g, 7, [x, 0, 0], t);
                assert_eq!(a, b);
                assert!(a.gdp > 0.0 && a.fx > 0.0 && (0.02..=0.6).contains(&a.unemployment));
                assert!((0.0..=0.6).contains(&a.policy_rate));
                crashed |= a.crash > 0.05;
            }
        }
        assert!(crashed, "over 15 sector-decades some crash happens");
    }

    #[test]
    fn a_crash_sinks_shares_cuts_credit_and_a_bubble_precedes_it() {
        let calm = Sector {
            faction: None,
            gdp: 1.0,
            potential: 1.0,
            money_growth: 0.0,
            inflation: 0.0,
            policy_rate: 0.02,
            unemployment: 0.05,
            debt_ratio: 0.5,
            fx: 1.0,
            crash: 0.0,
            bubble: 0.0,
            at_war: false,
            unrest: 0.0,
            production: 0.0,
            harvest: 1.0,
        };
        let crash = Sector { crash: 0.3, ..calm };
        let bubble = Sector {
            bubble: 0.3,
            ..calm
        };
        assert!(crash.share_factor() < 0.5 * calm.share_factor());
        assert!(bubble.share_factor() > calm.share_factor());
        assert!(crash.credit_factor() < calm.credit_factor());
        // The generated crisis timeline: a bubble is always followed by a crash.
        let mut seen = false;
        'outer: for c in 0..200 {
            let cell = [c, 0, 0];
            for d in 0..(CRASH_PERIOD as i64 * 6) {
                let t = d as f64 * DAY;
                let (cr, bu) = crisis(3, cell, t);
                if bu > 0.2 {
                    let (cr2, _) = crisis(3, cell, t + BUBBLE_DAYS * DAY);
                    assert!(cr2 > 0.0 && cr == 0.0);
                    seen = true;
                    break 'outer;
                }
            }
        }
        assert!(seen);
    }
    #[test]
    fn gdp_follows_the_production_of_the_industries() {
        let d = data();
        let g = Galaxy::new(7);
        // Production adds up per industry and is bounded by its potential.
        let c = [0, 0, 0];
        let (by, potential) = production(&d, 7, c, 100.0 * DAY);
        assert_eq!(by.len(), d.industries.len());
        assert!(by.iter().all(|v| *v >= 0.0));
        assert!(by.iter().sum::<f64>() <= potential * 1.4 + 1e-6);
        // A sector with more settled worlds makes more: scale the worlds, not the formula.
        let rich = (-3..4)
            .map(|x| settled_worlds(7, [x, 0, 0]))
            .fold(0.0_f64, f64::max);
        let poor = (-3..4)
            .map(|x| settled_worlds(7, [x, 0, 0]))
            .fold(f64::MAX, f64::min);
        assert!(rich > poor);
        // Colonies of the player add output: the same date, more GDP.
        let a = sector_with(&d, &g, 7, c, 100.0 * DAY, 0.0);
        let b = sector_with(&d, &g, 7, c, 100.0 * DAY, 500.0);
        assert!(b.gdp > a.gdp + 400.0);
        // The farms follow the harvest: a drought year cuts the farm output only.
        let farm = (0..d.industries.len())
            .find(|&i| is_agricultural(&d, i))
            .expect("a farm");
        let mut dry = None;
        'o: for x in 0..40 {
            for y in 0..30 {
                let t = y as f64 * 365.0 * DAY + 10.0 * DAY;
                if harvest(7, [x, 0, 0], t) < 0.7 {
                    dry = Some(([x, 0, 0], y));
                    break 'o;
                }
            }
        }
        let (cell, y) = dry.expect("some drought in 40 sectors x 30 years");
        let t_dry = y as f64 * 365.0 * DAY + 10.0 * DAY;
        let t_ok = t_dry + 0.5 * 365.0 * DAY;
        let (p_dry, _) = production(&d, 7, cell, t_dry);
        let (p_ok, _) = production(&d, 7, cell, t_ok);
        // Farms against the rest scales with the harvest (the sickness hits every industry alike).
        let ratio = |p: &Vec<f64>| p[farm] / (p.iter().sum::<f64>() - p[farm]);
        let want = harvest(7, cell, t_dry) / harvest(7, cell, t_ok);
        assert!((ratio(&p_dry) / ratio(&p_ok) - want).abs() < 1e-9);
        assert!(
            harvest_goods_factor(
                &d,
                7,
                politics::cell_center(cell),
                t_dry,
                d.resource_index("food").unwrap()
            ) > 1.0
        );
    }

    #[test]
    fn tourism_follows_safety_and_history_is_deterministic() {
        let d = data();
        let g = Galaxy::new(7);
        let mut lo = 1.0_f64;
        let mut hi = 0.0_f64;
        for x in -4..5 {
            let v = tourism(&d, &g, 7, politics::cell_center([x, 0, 0]), 50.0 * DAY);
            assert!((0.0..=1.0).contains(&v));
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert!(hi > lo);
        let a = history(&d, &g, 7, [1, 0, 0], 500.0 * DAY, 30.0, 12, |s| s.inflation);
        let b = history(&d, &g, 7, [1, 0, 0], 500.0 * DAY, 30.0, 12, |s| s.inflation);
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }
}
