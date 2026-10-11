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
}

/// Price level from the quantity of money and the real output (the quantity theory of money).
pub fn price_level(money: f64, output: f64) -> f64 {
    money / output.max(1e-9)
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

/// Real output of a sector: the stars it holds make the base, the cycle, war, unrest and crash
/// bend it. Returns (gdp, potential).
fn output(seed: u64, c: Cell, t: f64, war: bool, unrest: f64, crash: f64) -> (f64, f64) {
    let stars = Galaxy::density(politics::cell_center(c));
    let potential = (3_000.0 + 900.0 * stars) * (0.7 + 0.6 * h(seed, c, 1));
    let period = 700.0 + 800.0 * h(seed, c, 2);
    let phase = std::f64::consts::TAU * h(seed, c, 3);
    let gap = 0.08 * (std::f64::consts::TAU * t / DAY / period + phase).sin();
    let gdp = potential
        * (1.0 + gap)
        * (1.0 - crash)
        * (1.0 - if war { 0.12 } else { 0.0 })
        * (1.0 - 0.08 * unrest);
    (gdp, potential)
}

/// The economy of sector `c` at time `t`.
pub fn sector(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> Sector {
    let war = politics::sector_at_war(data, galaxy, seed, c, t);
    let unrest = politics::unrest(seed, c, t);
    let (crash, bubble) = crisis(seed, c, t);
    let (gdp, potential) = output(seed, c, t, war, unrest, crash);
    // Output growth over the last month, per day (war and crash terms follow the date).
    let (crash0, _) = crisis(seed, c, t - 30.0 * DAY);
    let (gdp0, _) = output(seed, c, t - 30.0 * DAY, war, unrest, crash0);
    let g_q = (gdp / gdp0.max(1e-9)).ln() / 30.0;
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
    fn doubling_the_money_supply_over_the_same_output_doubles_the_price_level() {
        let p = price_level(1.0e9, 5.0e6);
        assert!((price_level(2.0e9, 5.0e6) / p - 2.0).abs() < 1e-12);
        // And inflation rises with money growth at constant output.
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
}
