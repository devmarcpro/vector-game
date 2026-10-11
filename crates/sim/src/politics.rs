//! The politics of the sectors (E10). The galaxy is infinite, so power is local: space is cut in
//! sectors of 240 ly, each ruled by a leader who stays in office for a term and leaves it by
//! election, assassination, coup or resignation; neighbouring sectors may go to war (more often
//! when their peoples are enemies). Everything is a pure function of the galaxy seed, the sector
//! and the date, like the world events of the exchange: the same war starts on the same day for
//! everybody, and it has consequences where it happens (the price of arms and fuel in the
//! sectors at war, unrest after a murder or a coup).
use crate::bourse::DAY;
use crate::genealogy::{self, Kin};
use crate::planet::unit;
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Side of a sector of power, ly.
pub const SECTOR_LY: f64 = 240.0;
/// A sector, by integer coordinates.
pub type Cell = [i64; 3];

/// Wars start and end at the turn of a period.
pub const WAR_PERIOD: f64 = 60.0 * DAY;
/// Chance that a period is a war period between two neighbouring sectors: enemy peoples, the
/// same people, other peoples.
pub const P_WAR_ENEMIES: f64 = 0.20;
pub const P_WAR_SAME: f64 = 0.01;
pub const P_WAR_OTHERS: f64 = 0.04;
/// Goods whose price jumps in a sector at war, and by how much.
pub const WAR_GOODS: [&str; 7] = [
    "weapon_parts",
    "ammo_slugs",
    "missiles",
    "mines",
    "fuel",
    "medicine",
    "hull_plates",
];
pub const WAR_MARKUP: f64 = 0.25;
/// Days an assassination or a coup keeps a sector shaken.
pub const UNREST_DAYS: f64 = 30.0;

pub fn cell_of(p: [f64; 3]) -> Cell {
    p.map(|v| (v / SECTOR_LY).floor() as i64)
}

pub fn cell_center(c: Cell) -> [f64; 3] {
    c.map(|v| (v as f64 + 0.5) * SECTOR_LY)
}

/// The sectors within `r` sectors of `c` (a cube).
pub fn cells_around(c: Cell, r: i64) -> Vec<Cell> {
    let mut out = Vec::new();
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                out.push([c[0] + dx, c[1] + dy, c[2] + dz]);
            }
        }
    }
    out
}

pub(crate) fn neighbours(c: Cell) -> [Cell; 6] {
    [
        [c[0] + 1, c[1], c[2]],
        [c[0] - 1, c[1], c[2]],
        [c[0], c[1] + 1, c[2]],
        [c[0], c[1] - 1, c[2]],
        [c[0], c[1], c[2] + 1],
        [c[0], c[1], c[2] - 1],
    ]
}

pub(crate) fn key(c: Cell) -> i64 {
    hash_coord(0x5EC7, c[0], c[1], c[2]) as i64
}

/// The people that holds the sector (none in empty space).
pub fn faction_of(data: &GameData, galaxy: &Galaxy, c: Cell) -> Option<usize> {
    galaxy.territory(cell_center(c), data.factions.len())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EndKind {
    Election,
    Assassination,
    Coup,
    Resignation,
}

#[derive(Clone, Debug)]
pub struct Leader {
    pub name: String,
    pub title: &'static str,
    pub since: f64,
    pub until: f64,
    /// How this leader's term ends.
    pub end: EndKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PolKind {
    War {
        a: Cell,
        b: Cell,
        fa: Option<usize>,
        fb: Option<usize>,
    },
    Peace {
        a: Cell,
        b: Cell,
        fa: Option<usize>,
        fb: Option<usize>,
    },
    Succession {
        cell: Cell,
        faction: Option<usize>,
        old: String,
        new: String,
        how: EndKind,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolEvent {
    pub time: f64,
    /// Where it happened (ly): the middle of the sector, or of the front.
    pub pos: [f64; 3],
    pub kind: PolKind,
}

/// The title of the head of each people.
fn title(faction_id: Option<&str>) -> &'static str {
    match faction_id {
        Some("concorde") => "Premier consul",
        Some("ceintures") => "Doyen des ceintures",
        Some("mars") => "Gouverneur",
        Some("ligue") => "Doge",
        Some("hegemonie") => "Marechal",
        Some("errants") => "Capitaine-amiral",
        Some("helios") => "Grand pretre",
        Some("verdant") => "Regente",
        _ => "Seigneur local",
    }
}

fn term_len(seed: u64, c: Cell, k: u32) -> f64 {
    (4.0 + 6.0 * unit(hash_coord(seed ^ 0x1EAD, key(c), i64::from(k), 1))) * 365.0 * DAY
}

fn end_kind(seed: u64, c: Cell, k: u32) -> EndKind {
    let u = unit(hash_coord(seed ^ 0x1EAD, key(c), i64::from(k), 2));
    if u < 0.6 {
        EndKind::Election
    } else if u < 0.8 {
        EndKind::Assassination
    } else if u < 0.9 {
        EndKind::Coup
    } else {
        EndKind::Resignation
    }
}

/// When term `k` of sector `c` starts and ends (term 0 starts at date 0).
fn term_bounds(seed: u64, c: Cell, k: u32) -> (f64, f64) {
    let mut since = 0.0;
    for j in 0..k {
        since += term_len(seed, c, j);
    }
    (since, since + term_len(seed, c, k))
}

/// The leader of sector `c` (held by people `faction`) during term `k`.
pub fn leader_of_term(
    data: &GameData,
    seed: u64,
    c: Cell,
    faction: Option<usize>,
    k: u32,
) -> Leader {
    let (since, until) = term_bounds(seed, c, k);
    let h = hash_coord(seed ^ 0x1EAE, key(c), i64::from(k), 3);
    let kin = Kin {
        fam: genealogy::make_fam(genealogy::ADULT_GENERATION, h),
        k: 0,
    };
    Leader {
        name: genealogy::full_name(&genealogy::Families::new(), data, kin),
        title: title(faction.map(|f| data.factions[f].id.as_str())),
        since,
        until,
        end: end_kind(seed, c, k),
    }
}

/// The leader of sector `c` at time `t`.
pub fn leader(data: &GameData, seed: u64, c: Cell, faction: Option<usize>, t: f64) -> Leader {
    let mut k = 0;
    while term_bounds(seed, c, k).1 <= t && k < 400 {
        k += 1;
    }
    leader_of_term(data, seed, c, faction, k)
}

fn enemies(data: &GameData, a: Option<usize>, b: Option<usize>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    let (fa, fb) = (&data.factions[a], &data.factions[b]);
    fa.enemies.contains(&fb.id) || fb.enemies.contains(&fa.id)
}

/// Whether the two neighbouring sectors are at war during period `m`.
pub(crate) fn war_in_period(
    data: &GameData,
    seed: u64,
    a: Cell,
    b: Cell,
    fa: Option<usize>,
    fb: Option<usize>,
    m: i64,
) -> bool {
    let p = if fa.is_some() && fa == fb {
        P_WAR_SAME
    } else if enemies(data, fa, fb) {
        P_WAR_ENEMIES
    } else {
        P_WAR_OTHERS
    };
    // Symmetric in (a, b).
    let (ka, kb) = (key(a), key(b));
    let (lo, hi) = (ka.min(kb), ka.max(kb));
    unit(hash_coord(seed ^ 0x7A2, lo ^ hi.rotate_left(17), m, 1)) < p
}

/// What happened in sector `c` between `t0` (excluded) and `t1` (included): successions of its
/// leader, and the wars it has with its neighbours (told once, by the sector with the smaller
/// key). Oldest first.
pub fn events_in_cell(
    data: &GameData,
    galaxy: &Galaxy,
    seed: u64,
    c: Cell,
    t0: f64,
    t1: f64,
) -> Vec<PolEvent> {
    let mut out = Vec::new();
    let (m0, m1) = (
        (t0 / WAR_PERIOD).floor() as i64 + 1,
        (t1 / WAR_PERIOD).floor() as i64,
    );
    if m1 >= m0 {
        let fc = faction_of(data, galaxy, c);
        for nb in neighbours(c) {
            if key(nb) < key(c) {
                continue;
            }
            let fnb = faction_of(data, galaxy, nb);
            let (ca, cb) = (cell_center(c), cell_center(nb));
            let pos = [
                (ca[0] + cb[0]) / 2.0,
                (ca[1] + cb[1]) / 2.0,
                (ca[2] + cb[2]) / 2.0,
            ];
            for m in m0..=m1 {
                let (before, now) = (
                    war_in_period(data, seed, c, nb, fc, fnb, m - 1),
                    war_in_period(data, seed, c, nb, fc, fnb, m),
                );
                let time = m as f64 * WAR_PERIOD;
                if now && !before {
                    out.push(PolEvent {
                        time,
                        pos,
                        kind: PolKind::War {
                            a: c,
                            b: nb,
                            fa: fc,
                            fb: fnb,
                        },
                    });
                } else if before && !now {
                    out.push(PolEvent {
                        time,
                        pos,
                        kind: PolKind::Peace {
                            a: c,
                            b: nb,
                            fa: fc,
                            fb: fnb,
                        },
                    });
                }
            }
        }
    }
    let mut k = 0;
    loop {
        let (_, until) = term_bounds(seed, c, k);
        if until > t1 || k > 400 {
            break;
        }
        if until > t0 {
            let fc = faction_of(data, galaxy, c);
            let l = leader_of_term(data, seed, c, fc, k);
            let next = leader_of_term(data, seed, c, fc, k + 1);
            out.push(PolEvent {
                time: until,
                pos: cell_center(c),
                kind: PolKind::Succession {
                    cell: c,
                    faction: fc,
                    old: format!("{} {}", l.title, l.name),
                    new: next.name,
                    how: l.end,
                },
            });
        }
        k += 1;
    }
    out.sort_by(|x, y| x.time.total_cmp(&y.time));
    out
}

/// How shaken sector `c` is at time `t` after a murder or a coup (0 calm, 1 just after).
pub fn unrest(seed: u64, c: Cell, t: f64) -> f64 {
    let mut k = 0;
    let mut u = 0.0;
    loop {
        let (since, until) = term_bounds(seed, c, k);
        if since > t || k > 400 {
            break;
        }
        let ago = (t - until) / DAY;
        if (0.0..UNREST_DAYS).contains(&ago)
            && matches!(end_kind(seed, c, k), EndKind::Assassination | EndKind::Coup)
        {
            u = 1.0 - ago / UNREST_DAYS;
        }
        k += 1;
    }
    u
}

/// Whether sector `c` is at war with a neighbour at time `t`.
pub fn sector_at_war(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> bool {
    let m = (t / WAR_PERIOD).floor() as i64;
    let fc = faction_of(data, galaxy, c);
    neighbours(c).into_iter().any(|nb| {
        let fnb = faction_of(data, galaxy, nb);
        war_in_period(data, seed, c, nb, fc, fnb, m)
    })
}

/// What a place goes through at a given time: war and unrest, worked out once for all prices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turmoil {
    pub at_war: bool,
    pub unrest: f64,
    /// Blockaded by a victorious fleet (see `arms::fronts`).
    pub besieged: bool,
}

pub fn turmoil(data: &GameData, galaxy: &Galaxy, seed: u64, pos: [f64; 3], t: f64) -> Turmoil {
    let c = cell_of(pos);
    Turmoil {
        at_war: sector_at_war(data, galaxy, seed, c, t),
        unrest: unrest(seed, c, t),
        besieged: crate::arms::is_besieged(data, galaxy, seed, c, t),
    }
}

impl Turmoil {
    /// Price multiplier of resource `res`: arms, fuel and medicine are dear in a sector at war,
    /// and everything wobbles a little after a murder or a coup.
    pub fn factor(&self, data: &GameData, res: usize) -> f64 {
        let war_good = WAR_GOODS.contains(&data.resources[res].id.as_str());
        let mut mult = 1.0;
        if war_good && self.at_war {
            mult *= 1.0 + WAR_MARKUP;
        }
        if self.besieged {
            mult *= 1.0 + crate::arms::BLOCKADE_MARKUP;
        }
        if self.unrest > 0.0 {
            mult *= 1.0 + 0.08 * self.unrest * if war_good { 1.0 } else { 0.4 };
        }
        mult
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
    fn every_sector_has_its_own_leaders_with_terms_and_an_end() {
        let d = data();
        let g = Galaxy::new(7);
        let mut names = std::collections::BTreeSet::new();
        for c in cells_around([0, 0, 0], 2) {
            let f = faction_of(&d, &g, c);
            let a = leader(&d, 7, c, f, 100.0 * DAY);
            assert_eq!(
                a.name,
                leader(&d, 7, c, f, 101.0 * DAY).name,
                "same term, same leader"
            );
            let span = (a.until - a.since) / (365.0 * DAY);
            assert!((4.0..=10.0).contains(&span), "{span} years");
            let b = leader(&d, 7, c, f, a.until + DAY);
            assert!((b.since - a.until).abs() < 1.0);
            assert_ne!(a.name, b.name);
            names.insert(a.name);
        }
        assert!(
            names.len() > 100,
            "{} different leaders in 125 sectors",
            names.len()
        );
        // The galaxy is infinite: far sectors have leaders too.
        let far = [123_456, -98_765, 4_321];
        assert!(!leader(&d, 7, far, None, 0.0).name.is_empty());
        let mut kinds = std::collections::BTreeSet::new();
        for c in cells_around([0, 0, 0], 1) {
            for k in 0..30 {
                kinds.insert(format!("{:?}", end_kind(7, c, k)));
            }
        }
        assert_eq!(kinds.len(), 4, "{kinds:?}");
    }

    #[test]
    fn wars_are_between_neighbours_likelier_between_enemy_peoples_and_events_match_the_state() {
        let d = data();
        let g = Galaxy::new(7);
        let (mut enemy_p, mut enemy_w, mut other_p, mut other_w) = (0, 0, 0, 0);
        for c in cells_around([0, 0, 0], 3) {
            let fc = faction_of(&d, &g, c);
            for nb in neighbours(c) {
                let fnb = faction_of(&d, &g, nb);
                for m in 0..60 {
                    let w = war_in_period(&d, 7, c, nb, fc, fnb, m);
                    assert_eq!(w, war_in_period(&d, 7, nb, c, fnb, fc, m), "symmetric");
                    if fc != fnb && enemies(&d, fc, fnb) {
                        enemy_p += 1;
                        enemy_w += i32::from(w);
                    } else if fc != fnb {
                        other_p += 1;
                        other_w += i32::from(w);
                    }
                }
            }
        }
        assert!(enemy_p > 100 && other_p > 100, "{enemy_p} {other_p}");
        let (pe, po) = (
            f64::from(enemy_w) / f64::from(enemy_p),
            f64::from(other_w) / f64::from(other_p),
        );
        assert!(pe > po * 2.0, "{pe} vs {po}");
        // The events of a sector, replayed, give the change of its wars with its larger-key
        // neighbours.
        let c = [0, 0, 0];
        let (t0, t1) = (500.0 * DAY, 3000.0 * DAY);
        let fc = faction_of(&d, &g, c);
        let at = |t: f64| -> Vec<Cell> {
            let mut v: Vec<Cell> = neighbours(c)
                .into_iter()
                .filter(|nb| key(*nb) >= key(c))
                .filter(|nb| {
                    war_in_period(
                        &d,
                        7,
                        c,
                        *nb,
                        fc,
                        faction_of(&d, &g, *nb),
                        (t / WAR_PERIOD).floor() as i64,
                    )
                })
                .collect();
            v.sort_unstable();
            v
        };
        let mut state = at(t0);
        for e in events_in_cell(&d, &g, 7, c, t0, t1) {
            match e.kind {
                PolKind::War { b, .. } => state.push(b),
                PolKind::Peace { b, .. } => state.retain(|x| *x != b),
                PolKind::Succession { .. } => {}
            }
        }
        state.sort_unstable();
        assert_eq!(state, at(t1), "events replay the change of state");
    }

    #[test]
    fn a_murder_shakes_the_sector_and_war_raises_the_price_of_arms_there() {
        let d = data();
        let g = Galaxy::new(7);
        let arms = d.resource_index("ammo_slugs").unwrap();
        let bread = d.resource_index("food").unwrap();
        let (c, t) = cells_around([0, 0, 0], 2)
            .into_iter()
            .find_map(|c| {
                (0..6).find_map(|k| {
                    matches!(end_kind(7, c, k), EndKind::Assassination | EndKind::Coup)
                        .then(|| (c, term_bounds(7, c, k).1))
                })
            })
            .expect("a murder or a coup");
        assert!(unrest(7, c, t + 2.0 * DAY) > 0.8);
        assert!(unrest(7, c, t + 40.0 * DAY) < 0.01);
        let calm = Turmoil {
            at_war: false,
            unrest: 0.0,
            besieged: false,
        };
        let shaken = Turmoil {
            at_war: false,
            unrest: 1.0,
            besieged: false,
        };
        assert!(shaken.factor(&d, arms) > calm.factor(&d, arms));
        let found = cells_around([0, 0, 0], 2).into_iter().find_map(|c| {
            (1..400).find_map(|m| {
                let t = m as f64 * WAR_PERIOD + DAY;
                sector_at_war(&d, &g, 7, c, t).then_some((c, t))
            })
        });
        let (c, t) = found.expect("a war near the origin within 70 years");
        let tm = turmoil(&d, &g, 7, cell_center(c), t);
        assert!(tm.at_war);
        assert!(tm.factor(&d, arms) >= 1.0 + WAR_MARKUP - 1e-9);
        assert!(tm.factor(&d, bread) < tm.factor(&d, arms));
    }
}
