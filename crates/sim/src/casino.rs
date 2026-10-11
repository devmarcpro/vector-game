//! The casino of the stations: roulette (red, black or a number) and a three-reel machine. The
//! house always keeps an edge (roulette 1/37, machine about 7 %), so playing is a way to lose
//! credits for fun; the draws come from the seed and a round counter, never from the clock.
use crate::Sim;
use vector_core::{hash_coord, Rng};

pub const MIN_STAKE: f64 = 10.0;
pub const MAX_STAKE: f64 = 5000.0;
/// Number of slots of the wheel (0 is green and belongs to the house).
pub const SLOTS: u32 = 37;
/// Symbols on a reel of the machine.
pub const SYMBOLS: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Game {
    Red,
    Black,
    Number(u32),
    Machine,
}

impl Game {
    pub fn name(self) -> &'static str {
        match self {
            Game::Red => "ROULETTE : ROUGE",
            Game::Black => "ROULETTE : NOIR",
            Game::Number(_) => "ROULETTE : NUMERO",
            Game::Machine => "MACHINE A SOUS",
        }
    }
}

/// Red slots of the wheel (the usual European layout).
pub fn is_red(n: u32) -> bool {
    matches!(
        n,
        1 | 3 | 5 | 7 | 9 | 12 | 14 | 16 | 18 | 19 | 21 | 23 | 25 | 27 | 30 | 32 | 34 | 36
    )
}

/// What the machine returns, as a multiple of the stake, for three reels.
pub fn machine_return(r: [u32; 3]) -> f64 {
    if r[0] == r[1] && r[1] == r[2] {
        20.0
    } else if r[0] == r[1] || r[1] == r[2] || r[0] == r[2] {
        0.9
    } else {
        0.0
    }
}

/// What a roulette bet returns (multiple of the stake) when the ball stops on `n`.
pub fn roulette_return(game: Game, n: u32) -> f64 {
    match game {
        Game::Red if n != 0 && is_red(n) => 2.0,
        Game::Black if n != 0 && !is_red(n) => 2.0,
        Game::Number(k) if k == n => 36.0,
        _ => 0.0,
    }
}

/// The result of one round.
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub game: Game,
    pub stake: f64,
    /// Wheel slot or the three reels.
    pub wheel: u32,
    pub reels: [u32; 3],
    /// What came back (multiple of the stake times the stake).
    pub payout: f64,
}

impl Sim {
    /// Play one round at the station's casino. The stake is taken, the payout paid.
    pub fn gamble(&mut self, game: Game, stake: f64) -> Result<Round, &'static str> {
        self.docked.ok_or("le casino est a la station")?;
        if let Game::Number(k) = game {
            if k >= SLOTS {
                return Err("numero de 0 a 36");
            }
        }
        if !(MIN_STAKE..=MAX_STAKE).contains(&stake) {
            return Err("mise de 10 a 5000 CR");
        }
        if stake > self.credits {
            return Err("fonds insuffisants");
        }
        let mut rng = Rng::new(hash_coord(
            self.galaxy.seed ^ 0xCA51,
            self.casino_round,
            self.time as i64,
            0,
        ));
        self.casino_round += 1;
        let wheel = (rng.f64() * f64::from(SLOTS)) as u32;
        let reels = [0; 3].map(|_| (rng.f64() * f64::from(SYMBOLS)) as u32);
        let mult = match game {
            Game::Machine => machine_return(reels),
            g => roulette_return(g, wheel),
        };
        let payout = stake * mult;
        self.credits += payout - stake;
        Ok(Round {
            game,
            stake,
            wheel,
            reels,
            payout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn docked() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e7;
        s
    }

    #[test]
    fn the_house_keeps_an_edge_on_every_game() {
        // Exact return to player, by enumeration.
        let wheel =
            |g: Game| (0..SLOTS).map(|n| roulette_return(g, n)).sum::<f64>() / f64::from(SLOTS);
        assert!((wheel(Game::Red) - 36.0 / 37.0).abs() < 1e-12);
        assert!((wheel(Game::Black) - 36.0 / 37.0).abs() < 1e-12);
        assert!((wheel(Game::Number(17)) - 36.0 / 37.0).abs() < 1e-12);
        assert_eq!((0..SLOTS).filter(|n| is_red(*n)).count(), 18);
        let mut total = 0.0;
        for a in 0..SYMBOLS {
            for b in 0..SYMBOLS {
                for c in 0..SYMBOLS {
                    total += machine_return([a, b, c]);
                }
            }
        }
        let rtp = total / f64::from(SYMBOLS.pow(3));
        assert!(rtp > 0.9 && rtp < 0.95, "machine returns {rtp}");
    }

    #[test]
    fn a_round_moves_credits_by_payout_minus_stake_and_is_reproducible() {
        let mut s = docked();
        let c = s.credits;
        let r = s.gamble(Game::Machine, 100.0).expect("plays");
        assert!((s.credits - (c - 100.0 + r.payout)).abs() < 1e-9);
        let mut t = docked();
        assert_eq!(
            t.gamble(Game::Machine, 100.0).unwrap(),
            r,
            "same seed, same draw"
        );
        let again = s.gamble(Game::Machine, 100.0).unwrap();
        assert_eq!(s.casino_round, 2);
        let _ = again;
        assert!(s.gamble(Game::Machine, 5.0).is_err(), "stake too small");
        assert!(s.gamble(Game::Machine, 6000.0).is_err(), "stake too large");
        assert!(s.gamble(Game::Number(37), 50.0).is_err(), "no such number");
        s.credits = 20.0;
        assert!(s.gamble(Game::Red, 50.0).is_err(), "no money");
        s.undock();
        assert!(
            s.gamble(Game::Red, 10.0).is_err(),
            "casinos are in stations"
        );
    }

    #[test]
    fn many_rounds_lose_money_on_average_near_the_edge() {
        let mut s = docked();
        let n = 20_000;
        let mut paid = 0.0;
        for i in 0..n {
            s.time = f64::from(i);
            paid += s.gamble(Game::Red, 100.0).unwrap().payout;
        }
        let rtp = paid / (f64::from(n) * 100.0);
        assert!((rtp - 36.0 / 37.0).abs() < 0.03, "measured return {rtp}");
        let mut paid = 0.0;
        for i in 0..n {
            s.time = f64::from(i);
            paid += s.gamble(Game::Machine, 100.0).unwrap().payout;
        }
        let rtp = paid / (f64::from(n) * 100.0);
        assert!(rtp > 0.8 && rtp < 1.05, "machine measured return {rtp}");
    }
}
