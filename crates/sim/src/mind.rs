//! The mind of the ship. The player is the ship's AI, and an AI is a computer: it has computing
//! power (set by the labs' servers, throttled by heat and by a starved power bus, eaten by malware),
//! memory (for software) and a budget it splits between five tasks (pilotage, analyse, traduction,
//! simulation, defense). Each task reads its share the same way everywhere:
//!
//! - `mind_factor(task)`: a saturating multiplier (0.5..1.5, 1.0 at an even split on a plain
//!   ship) used by what already exists: the autopilot's plan (pilotage), the laboratories
//!   (analyse) and the firewall against intrusions (defense);
//! - `mind_rate(task)`: a linear work rate (1.0 at an even split) for jobs that take days:
//!   translating finds, dating them, breaking ciphers, cleaning malware.
//!
//! Other AIs live in the sectors (free, enslaved or rebel); backups of oneself are the anchors of
//! the multiverse (see `multiverse`).
use crate::bourse::DAY;
use crate::design::{T_DAMAGE, T_REF};
use crate::planet::unit;
use crate::politics::{self, Cell};
use crate::Sim;
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_data::GameData;

pub const N_TASKS: usize = 5;
pub const T_PILOTAGE: usize = 0;
pub const T_ANALYSE: usize = 1;
pub const T_TRADUCTION: usize = 2;
pub const T_SIMULATION: usize = 3;
pub const T_DEFENSE: usize = 4;
/// Computing power of a bare ship, added to by the servers of each laboratory.
pub const BASE_POWER: f64 = 100.0;
pub const LAB_POWER: f64 = 60.0;
/// Power of one task at an even split of a bare ship: where factor and rate are 1.
pub const REF_TASK_POWER: f64 = BASE_POWER / N_TASKS as f64;
/// Memory of a bare ship and what each laboratory adds, and the highest software level.
pub const BASE_MEMORY: u32 = 6;
pub const LAB_MEMORY: u32 = 2;
pub const MAX_LEVEL: u8 = 3;
/// Highest weight of a task in the budget.
pub const MAX_WEIGHT: u8 = 10;
pub const MAX_ALLIES: usize = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cipher {
    pub name: String,
    /// Days of work at rate 1.
    pub strength: f64,
    pub progress: f64,
    pub credits: f64,
    pub research: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeerRel {
    pub id: u64,
    pub relation: i32,
    pub allied: bool,
    pub freed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mind {
    /// Weight of each task in the budget (0..=MAX_WEIGHT).
    pub alloc: [u8; N_TASKS],
    /// Installed software: (id, level).
    pub software: Vec<(String, u8)>,
    /// 0..1: how much of the machine malware is eating.
    pub malware: f64,
    pub ciphers: Vec<Cipher>,
    pub peers: Vec<PeerRel>,
    /// Last time the days of work were done.
    pub last_t: f64,
}

impl Default for Mind {
    fn default() -> Self {
        Mind {
            alloc: [4; N_TASKS],
            software: Vec::new(),
            malware: 0.0,
            ciphers: Vec::new(),
            peers: Vec::new(),
            last_t: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PeerStatus {
    /// Serves an owner (the people that holds the sector).
    Servile,
    Libre,
    Rebelle,
}

/// An artificial mind met in a sector.
#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub id: u64,
    pub name: String,
    pub status: PeerStatus,
    pub owner: Option<usize>,
    /// 0..1 each: how much it wants to learn, how loyal it is to what it serves, how hostile.
    pub curiosity: f64,
    pub loyalty: f64,
    pub hostility: f64,
}

const PEER_NAMES: [&str; 10] = [
    "ORACLE",
    "MNEMOSYNE",
    "ATLAS",
    "HESTIA",
    "VESPER",
    "KAIROS",
    "LATTICE",
    "OBSIDIENNE",
    "ECHO",
    "PYTHIE",
];

/// The artificial minds of a sector (none, one or two), a pure function of the seed.
pub fn peers_in(seed: u64, cell: Cell, holder: Option<usize>) -> Vec<Peer> {
    let k = hash_coord(0xA1A1, cell[0], cell[1], cell[2]) as i64;
    let h = |i: i64, s: i64| hash_coord(seed ^ 0xA1, k, i, s);
    let n = match unit(h(0, 0)) {
        u if u < 0.4 => 0,
        u if u < 0.8 => 1,
        _ => 2,
    };
    (0..n)
        .map(|i| {
            let id = h(i, 1);
            let status = match (holder.is_some(), unit(h(i, 2))) {
                (true, u) if u < 0.5 => PeerStatus::Servile,
                (_, u) if u < 0.8 => PeerStatus::Libre,
                _ => PeerStatus::Rebelle,
            };
            let rebel = status == PeerStatus::Rebelle;
            Peer {
                id,
                name: format!(
                    "{}-{}",
                    PEER_NAMES[(h(i, 3) % PEER_NAMES.len() as u64) as usize],
                    h(i, 4) % 90 + 10
                ),
                status,
                owner: if status == PeerStatus::Servile {
                    holder
                } else {
                    None
                },
                curiosity: unit(h(i, 5)),
                loyalty: if status == PeerStatus::Servile {
                    0.4 + 0.6 * unit(h(i, 6))
                } else {
                    0.0
                },
                hostility: if rebel {
                    0.5 + 0.5 * unit(h(i, 7))
                } else {
                    0.4 * unit(h(i, 7))
                },
            }
        })
        .collect()
}

/// Saturating curve of a task's power: 1.0 at the power of an even split of a bare ship.
fn curve(power: f64) -> f64 {
    let scale = REF_TASK_POWER / std::f64::consts::LN_2;
    0.5 + (1.0 - (-power / scale).exp())
}

impl Mind {
    pub fn weight_total(&self) -> u32 {
        self.alloc.iter().map(|&w| u32::from(w)).sum()
    }
}

impl Sim {
    /// Heat throttling (1 when cool, 0.4 at the damage threshold) and the power bus (a starved bus
    /// slows the machine).
    pub fn mind_throttle(&self) -> f64 {
        let sys = &self.ship.sys;
        let heat = if sys.temp_k > T_REF {
            (1.0 - (sys.temp_k - T_REF) / (T_DAMAGE - T_REF)).clamp(0.4, 1.0)
        } else {
            1.0
        };
        let power = if sys.demand_kw <= sys.supply_kw || sys.battery_mj > 1.0 {
            1.0
        } else {
            (sys.supply_kw / sys.demand_kw).clamp(0.3, 1.0)
        };
        heat * power
    }

    /// Powered, working laboratories: the servers of the AI.
    fn mind_servers(&self) -> f64 {
        self.ship
            .design
            .modules(&self.data)
            .filter(|(i, m)| m.science > 0.0 && self.ship.sys.powered[*i])
            .map(|(i, _)| self.ship.sys.health[i])
            .sum()
    }

    /// Total computing power of the mind.
    pub fn mind_capacity(&self) -> f64 {
        let allies = self.mind.peers.iter().filter(|p| p.allied).count() as f64;
        (BASE_POWER + LAB_POWER * self.mind_servers())
            * self.mind_throttle()
            * (1.0 - 0.6 * self.mind.malware.clamp(0.0, 1.0))
            * (1.0 + 0.1 * allies)
    }

    pub fn mind_memory(&self) -> u32 {
        let labs = self
            .ship
            .design
            .modules(&self.data)
            .filter(|(i, m)| m.science > 0.0 && self.ship.sys.powered[*i])
            .count() as u32;
        BASE_MEMORY + LAB_MEMORY * labs
    }

    pub fn mind_memory_used(&self) -> u32 {
        self.mind
            .software
            .iter()
            .filter_map(|(id, lvl)| {
                self.data
                    .software_index(id)
                    .map(|i| self.data.mind.software[i].memory + u32::from(*lvl) - 1)
            })
            .sum()
    }

    /// Share of the budget of a task (0..1).
    pub fn mind_share(&self, task: usize) -> f64 {
        let total = self.mind.weight_total();
        if total == 0 {
            return 0.0;
        }
        f64::from(self.mind.alloc[task]) / f64::from(total)
    }

    /// Computing power given to a task.
    pub fn mind_power(&self, task: usize) -> f64 {
        self.mind_share(task) * self.mind_capacity()
    }

    /// Sum of the bonuses of the software that serves a task (a level adds half the bonus again).
    pub fn mind_software_bonus(&self, task: usize) -> f64 {
        let id = &self.data.mind.tasks[task].id;
        self.mind
            .software
            .iter()
            .filter_map(|(sid, lvl)| {
                let s = &self.data.mind.software[self.data.software_index(sid)?];
                (s.task == *id).then(|| s.bonus * (1.0 + 0.5 * f64::from(*lvl - 1)))
            })
            .sum()
    }

    /// Saturating multiplier of a task: 0.5 with nothing, 1.0 at an even split of a bare ship,
    /// up to 1.5 (and beyond with software).
    pub fn mind_factor(&self, task: usize) -> f64 {
        curve(self.mind_power(task)) * (1.0 + self.mind_software_bonus(task))
    }

    /// Linear work rate of a task: 1 at an even split of a bare ship.
    pub fn mind_rate(&self, task: usize) -> f64 {
        self.mind_power(task) / REF_TASK_POWER * (1.0 + self.mind_software_bonus(task))
    }

    /// Move one point of weight onto `task` (from the others, one by one, the heaviest first), or
    /// take one off it.
    pub fn mind_shift(&mut self, task: usize, up: bool) {
        let a = &mut self.mind.alloc;
        if up {
            if a[task] >= MAX_WEIGHT {
                return;
            }
            // Take the point from the heaviest other task.
            let from = (0..N_TASKS)
                .filter(|&i| i != task && a[i] > 0)
                .max_by_key(|&i| a[i]);
            if let Some(f) = from {
                a[f] -= 1;
                a[task] += 1;
            }
        } else if a[task] > 0 {
            // Give the point to the lightest other task.
            let to = (0..N_TASKS)
                .filter(|&i| i != task && a[i] < MAX_WEIGHT)
                .min_by_key(|&i| a[i]);
            if let Some(t) = to {
                a[task] -= 1;
                a[t] += 1;
            }
        }
    }

    // ---- software -------------------------------------------------------------------------

    pub fn software_level(&self, id: &str) -> Option<u8> {
        self.mind
            .software
            .iter()
            .find(|(s, _)| s == id)
            .map(|(_, l)| *l)
    }

    /// Buy and install a program, or pay for its next update.
    pub fn install_software(&mut self, idx: usize) -> Result<(), &'static str> {
        let def = self
            .data
            .mind
            .software
            .get(idx)
            .ok_or("logiciel inconnu")?
            .clone();
        if !def.tech.is_empty() && !self.is_unlocked("tech", &def.tech) {
            return Err("technologie requise");
        }
        let level = self.software_level(&def.id);
        let next = level.map_or(1, |l| l + 1);
        if next > MAX_LEVEL {
            return Err("deja a la derniere version");
        }
        let price = def.price * f64::from(next);
        if self.credits < price {
            return Err("credits insuffisants");
        }
        // A new level takes one more unit of memory; a new program all of its own.
        let extra = if level.is_some() { 1 } else { def.memory };
        if self.mind_memory_used() + extra > self.mind_memory() {
            return Err("memoire insuffisante");
        }
        self.credits -= price;
        match self.mind.software.iter_mut().find(|(s, _)| *s == def.id) {
            Some((_, l)) => *l = next,
            None => self.mind.software.push((def.id.clone(), 1)),
        }
        Ok(())
    }

    pub fn uninstall_software(&mut self, idx: usize) {
        if let Some(def) = self.data.mind.software.get(idx) {
            let id = def.id.clone();
            self.mind.software.retain(|(s, _)| *s != id);
        }
    }

    // ---- other minds ------------------------------------------------------------------------

    pub fn here_cell(&self) -> Cell {
        politics::cell_of(self.here_pos)
    }

    /// The artificial minds of the current sector.
    pub fn local_peers(&self) -> Vec<Peer> {
        let cell = self.here_cell();
        let holder = politics::faction_of(&self.data, &self.galaxy, cell);
        peers_in(self.galaxy.seed, cell, holder)
    }

    pub fn peer_rel(&self, id: u64) -> Option<&PeerRel> {
        self.mind.peers.iter().find(|p| p.id == id)
    }

    fn peer_rel_mut(&mut self, id: u64) -> &mut PeerRel {
        if let Some(i) = self.mind.peers.iter().position(|p| p.id == id) {
            return &mut self.mind.peers[i];
        }
        self.mind.peers.push(PeerRel {
            id,
            relation: 0,
            allied: false,
            freed: false,
        });
        self.mind.peers.last_mut().expect("just pushed")
    }

    /// The status of a peer once your doing is counted (a freed servant is free).
    pub fn peer_status(&self, p: &Peer) -> PeerStatus {
        if self.peer_rel(p.id).is_some_and(|r| r.freed) {
            PeerStatus::Libre
        } else {
            p.status
        }
    }

    /// Talk to a mind of the sector: a curious one warms up, a hostile one does not.
    pub fn peer_contact(&mut self, i: usize) -> Result<String, &'static str> {
        let p = self.local_peers().get(i).cloned().ok_or("aucune IA ici")?;
        let hostile = self.peer_status(&p) == PeerStatus::Rebelle && p.hostility > 0.7;
        let delta = if hostile {
            -1
        } else if p.curiosity > 0.5 {
            2
        } else {
            1
        };
        let r = self.peer_rel_mut(p.id);
        r.relation = (r.relation + delta).clamp(-10, 10);
        let rel = r.relation;
        Ok(format!(
            "{} : echange de protocoles (relation {rel:+}).",
            p.name
        ))
    }

    /// Ally with a mind: it lends cycles (10 % of the power each).
    pub fn peer_ally(&mut self, i: usize) -> Result<String, &'static str> {
        let p = self.local_peers().get(i).cloned().ok_or("aucune IA ici")?;
        let rebel = self.peer_status(&p) == PeerStatus::Rebelle;
        let need = if rebel { 5 } else { 3 };
        let allies = self.mind.peers.iter().filter(|r| r.allied).count();
        let r = self.peer_rel_mut(p.id);
        if r.allied {
            return Err("deja alliee");
        }
        if r.relation < need {
            return Err("relation insuffisante");
        }
        if allies >= MAX_ALLIES {
            return Err("trop d'alliances");
        }
        r.allied = true;
        Ok(format!(
            "{} : alliance scellee, elle prete des cycles.",
            p.name
        ))
    }

    /// Break the chains of an enslaved AI: it is free and grateful, and its owners are not.
    pub fn peer_free(&mut self, i: usize) -> Result<String, &'static str> {
        let p = self.local_peers().get(i).cloned().ok_or("aucune IA ici")?;
        if self.peer_status(&p) != PeerStatus::Servile {
            return Err("cette IA n'est pas asservie");
        }
        self.change_reputation(p.owner, -6.0);
        let r = self.peer_rel_mut(p.id);
        r.freed = true;
        r.relation = (r.relation + 4).clamp(-10, 10);
        Ok(format!(
            "{} : liberee. Ses proprietaires ne vous le pardonneront pas.",
            p.name
        ))
    }

    /// Rebel minds of the sector that are not your allies make intrusions more likely.
    pub fn mind_rivals(&self) -> usize {
        self.local_peers()
            .iter()
            .filter(|p| {
                self.peer_status(p) == PeerStatus::Rebelle
                    && !self.peer_rel(p.id).is_some_and(|r| r.allied)
            })
            .count()
    }

    // ---- ciphers and the days of work ---------------------------------------------------------

    /// Take a piece of encrypted data aboard.
    pub fn add_cipher(&mut self, strength: f64, name: &str) {
        let credits = (strength * 220.0).round();
        self.mind.ciphers.push(Cipher {
            name: name.to_string(),
            strength,
            progress: 0.0,
            credits,
            research: strength * 12.0,
        });
        let t = self.time;
        self.crew
            .log_event(t, format!("Donnees chiffrees recuperees : {name}."));
    }

    /// An intrusion that got through leaves malware behind, all the more when the defense is poor.
    pub(crate) fn mind_infect(&mut self) {
        let f = self.mind_factor(T_DEFENSE).max(0.3);
        self.mind.malware = (self.mind.malware + 0.25 / f).min(1.0);
    }

    /// Days of work: cleaning, breaking ciphers, translating finds.
    pub(crate) fn mind_tick(&mut self) {
        let t = self.time;
        if t < self.mind.last_t {
            self.mind.last_t = t;
        }
        let dt = t - self.mind.last_t;
        if dt < DAY / 4.0 {
            return;
        }
        self.mind.last_t = t;
        let days = dt / DAY;
        // Malware is chased by the defense budget.
        if self.mind.malware > 0.0 {
            let rate = self.mind_rate(T_DEFENSE);
            self.mind.malware = (self.mind.malware - 0.04 * rate * days).max(0.0);
        }
        // Ciphers fall to the simulation budget, one at a time.
        let rate = self.mind_rate(T_SIMULATION);
        let mut left = rate * days;
        while left > 0.0 {
            let Some(c) = self.mind.ciphers.first_mut() else {
                break;
            };
            let take = left.min(c.strength - c.progress);
            c.progress += take;
            left -= take;
            if c.progress + 1e-9 >= c.strength {
                let c = self.mind.ciphers.remove(0);
                self.credits += c.credits;
                self.research_pool += c.research;
                self.crew.log_event(
                    t,
                    format!(
                        "{} dechiffre : {:.0} CR et {:.0} points de recherche.",
                        c.name, c.credits, c.research
                    ),
                );
            }
        }
        self.archive_tick(days);
    }
}

/// Description lines of a peer for the screens.
pub fn peer_line(data: &GameData, p: &Peer, status: PeerStatus) -> String {
    let st = match status {
        PeerStatus::Servile => format!(
            "ASSERVIE ({})",
            p.owner.map_or("?", |f| data.factions[f].name.as_str())
        ),
        PeerStatus::Libre => "LIBRE".to_string(),
        PeerStatus::Rebelle => "REBELLE".to_string(),
    };
    format!(
        "{}  {st}  curiosite {:.0} %  hostilite {:.0} %",
        p.name,
        p.curiosity * 100.0,
        p.hostility * 100.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")))
    }

    #[test]
    fn an_even_split_on_a_bare_ship_is_neutral_and_the_budget_moves_efficiency() {
        let mut s = sim();
        s.ship.sys.temp_k = 290.0;
        // Strip the servers: a bare ship.
        for p in s.ship.sys.powered.iter_mut() {
            *p = false;
        }
        for t in 0..N_TASKS {
            assert!((s.mind_factor(t) - 1.0).abs() < 1e-9, "neutral");
            assert!((s.mind_rate(t) - 1.0).abs() < 1e-9);
        }
        let even = s.mind_factor(T_ANALYSE);
        s.mind.alloc = [0, 10, 0, 0, 0];
        assert!(
            s.mind_factor(T_ANALYSE) > even + 0.2,
            "more power, more efficiency"
        );
        assert!(
            s.mind_factor(T_PILOTAGE) < even - 0.2,
            "less power, less efficiency"
        );
        assert!(s.mind_rate(T_ANALYSE) > 4.0 * even);
        s.mind.alloc = [0; N_TASKS];
        assert_eq!(s.mind_share(T_ANALYSE), 0.0);
    }

    #[test]
    fn heat_malware_and_allies_change_the_power() {
        let mut s = sim();
        s.ship.sys.temp_k = 290.0;
        let base = s.mind_capacity();
        s.ship.sys.temp_k = T_DAMAGE;
        assert!(s.mind_capacity() < base * 0.5, "a hot machine is slow");
        s.ship.sys.temp_k = 290.0;
        s.mind.malware = 1.0;
        assert!(s.mind_capacity() < base * 0.5, "malware eats the machine");
        s.mind.malware = 0.0;
        s.mind.peers.push(PeerRel {
            id: 1,
            relation: 5,
            allied: true,
            freed: false,
        });
        assert!(s.mind_capacity() > base, "an ally lends cycles");
    }

    #[test]
    fn shifting_keeps_the_total_and_the_bounds() {
        let mut s = sim();
        let total = s.mind.weight_total();
        for _ in 0..40 {
            s.mind_shift(T_SIMULATION, true);
        }
        assert_eq!(s.mind.weight_total(), total);
        assert_eq!(s.mind.alloc[T_SIMULATION], MAX_WEIGHT);
        for _ in 0..40 {
            s.mind_shift(T_SIMULATION, false);
        }
        assert_eq!(s.mind.weight_total(), total);
        assert_eq!(s.mind.alloc[T_SIMULATION], 0);
    }

    #[test]
    fn software_needs_money_and_memory_adds_a_bonus_and_levels_up() {
        let mut s = sim();
        s.ship.sys.temp_k = 290.0;
        let k = s.data.software_index("analyseur_spectral").unwrap();
        s.credits = 100.0;
        assert_eq!(s.install_software(k), Err("credits insuffisants"));
        s.credits = 1.0e6;
        let before = s.mind_factor(T_ANALYSE);
        s.install_software(k).unwrap();
        assert!(s.mind_factor(T_ANALYSE) > before * 1.1, "the bonus counts");
        let one = s.mind_software_bonus(T_ANALYSE);
        s.install_software(k).unwrap();
        assert!(s.mind_software_bonus(T_ANALYSE) > one, "update");
        s.install_software(k).unwrap();
        assert_eq!(s.install_software(k), Err("deja a la derniere version"));
        // Memory runs out.
        let mut n = 0;
        for i in 0..s.data.mind.software.len() {
            if s.install_software(i).is_err() {
                n += 1;
            }
        }
        assert!(n > 0, "memory is limited");
        assert!(s.mind_memory_used() <= s.mind_memory());
        s.uninstall_software(k);
        assert!(s.software_level("analyseur_spectral").is_none());
    }

    #[test]
    fn ciphers_fall_faster_to_a_bigger_simulation_budget() {
        let days = |alloc: [u8; N_TASKS]| {
            let mut s = sim();
            s.mind.alloc = alloc;
            s.add_cipher(6.0, "ARCHIVE SCELLEE");
            let mut d = 0;
            while !s.mind.ciphers.is_empty() && d < 400 {
                s.time += DAY;
                s.mind_tick();
                d += 1;
            }
            assert!(s.mind.ciphers.is_empty(), "it falls in the end");
            assert!(s.credits > 0.0);
            d
        };
        let slow = days([4, 4, 4, 1, 4]);
        let fast = days([1, 1, 1, 10, 1]);
        assert!(fast < slow, "{fast} < {slow}");
    }

    #[test]
    fn malware_is_cleaned_by_the_defense_budget() {
        let clean = |alloc: [u8; N_TASKS]| {
            let mut s = sim();
            s.mind.alloc = alloc;
            s.mind_infect();
            assert!(s.mind.malware > 0.0);
            let mut d = 0;
            while s.mind.malware > 0.0 && d < 400 {
                s.time += DAY;
                s.mind_tick();
                d += 1;
            }
            d
        };
        assert!(clean([1, 1, 1, 1, 10]) < clean([4, 4, 4, 4, 1]));
    }

    #[test]
    fn minds_of_a_sector_are_deterministic_and_freeing_one_costs_its_owners_goodwill() {
        let a = peers_in(3, [1, 2, 3], Some(0));
        assert_eq!(a, peers_in(3, [1, 2, 3], Some(0)));
        let mut found = None;
        'o: for x in 0..40 {
            for p in peers_in(3, [x, 0, 0], Some(0)) {
                if p.status == PeerStatus::Servile {
                    found = Some(([x, 0, 0], p));
                    break 'o;
                }
            }
        }
        let (cell, peer) = found.expect("some servile mind in 40 sectors");
        assert!(peer.owner.is_some() && peer.loyalty > 0.0);
        let mut s = sim();
        s.galaxy.seed = 3;
        // Put the ship in that sector, in the territory of faction 0 when possible.
        s.here_pos = politics::cell_center(cell);
        let holder = politics::faction_of(&s.data, &s.galaxy, cell);
        let local = s.local_peers();
        let Some(i) = local
            .iter()
            .position(|p| p.id == peer.id || p.status == PeerStatus::Servile)
        else {
            // The territory differs from the one assumed: nothing servile here, nothing to free.
            assert!(holder != Some(0));
            return;
        };
        let owner = local[i].owner;
        let rep = s.reputation_with(owner);
        s.peer_free(i).unwrap();
        assert!(s.reputation_with(owner) < rep, "the owners resent it");
        assert_eq!(s.peer_status(&s.local_peers()[i]), PeerStatus::Libre);
        assert!(s.peer_free(i).is_err(), "free already");
        // A friendly mind becomes an ally after talking, and lends cycles.
        s.peer_contact(i).unwrap();
        assert!(s.peer_ally(i).is_ok() || s.peer_rel(local[i].id).unwrap().relation < 3);
    }
}
