//! The history of the sectors. Every sector of the infinite galaxy has a past: a chain of ages of
//! 400 years (rise, apogee, collapse, dark age, migrations, reconquest) that is a pure function of
//! the galaxy seed and the sector, computed on demand (nothing is stored). The last age agrees with
//! the present (the people that holds the sector today, see `politics`), and the recent years are
//! told by `politics` itself (leaders, wars), so there is one chronology, not two. Ruins, wrecks and
//! stations get their provenance from it (who built them, when, why they fell): see `provenance`.
use crate::bourse::DAY;
use crate::genealogy::{self, Kin};
use crate::planet::unit;
use crate::politics::{self, Cell};
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Year of the game epoch (time 0), length of an age in years and number of ages told.
pub const EPOCH_YEAR: i32 = 2400;
pub const AGE_YEARS: i32 = 400;
pub const AGES: usize = 8;
pub const YEAR: f64 = 365.0 * DAY;

/// The calendar year at game time `t` (seconds).
pub fn year_at(t: f64) -> f64 {
    f64::from(EPOCH_YEAR) + t / YEAR
}

// Indices of the ages in `data/history.ron` (validated against `vector_data::AGE_IDS`).
const ESSOR: usize = 0;
const APOGEE: usize = 1;
const EFFONDREMENT: usize = 2;
const AGE_SOMBRE: usize = 3;
const MIGRATION: usize = 4;
const RECONQUETE: usize = 5;

/// Whether age kind `to` may follow age kind `from` in a chronicle.
pub fn can_follow(from: usize, to: usize) -> bool {
    NEXT.get(from)
        .and_then(|r| r.get(to))
        .is_some_and(|w| *w > 0.0)
}

/// Chance weights of the age that follows an age (rows: from, columns: to).
const NEXT: [[f64; 6]; 6] = [
    [0.0, 6.0, 2.0, 0.0, 0.0, 0.0],
    [0.0, 3.0, 7.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 6.0, 4.0, 0.0],
    [0.0, 0.0, 0.0, 2.0, 3.0, 5.0],
    [6.0, 0.0, 0.0, 0.0, 0.0, 4.0],
    [1.0, 5.0, 2.0, 0.0, 0.0, 0.0],
];

#[derive(Clone, Debug, PartialEq)]
pub struct Age {
    /// Index in `data.history.ages`.
    pub kind: usize,
    pub start: i32,
    pub end: i32,
    /// The people that holds the sector (None: a people nobody remembers, or nobody).
    pub holder: Option<usize>,
    /// Index in `data.history.causes` when the age is a collapse.
    pub cause: Option<usize>,
    pub figure: String,
    pub role: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chronicle {
    pub cell: Cell,
    pub ages: Vec<Age>,
    /// The place name that comes from its history ("Marche de ...").
    pub name: String,
}

fn ckey(c: Cell) -> i64 {
    hash_coord(0x4157, c[0], c[1], c[2]) as i64
}

fn pick_weighted(weights: &[f64], u: f64) -> usize {
    let total: f64 = weights.iter().sum();
    let mut x = u * total;
    for (i, w) in weights.iter().enumerate() {
        if x < *w {
            return i;
        }
        x -= w;
    }
    weights.iter().rposition(|w| *w > 0.0).unwrap_or(0)
}

fn person(data: &GameData, h: u64) -> String {
    let kin = Kin {
        fam: genealogy::make_fam(genealogy::ADULT_GENERATION, h),
        k: 0,
    };
    genealogy::full_name(&genealogy::Families::new(), data, kin)
}

fn role_of(kind: usize) -> &'static str {
    match kind {
        ESSOR => "fondateur",
        APOGEE => "empereur",
        EFFONDREMENT => "dernier souverain",
        AGE_SOMBRE => "prophete",
        MIGRATION => "chef de clan",
        _ => "restaurateur",
    }
}

/// The chronicle of sector `cell`; `holder_now` is the people that holds it today.
pub fn chronicle(data: &GameData, seed: u64, cell: Cell, holder_now: Option<usize>) -> Chronicle {
    let n_f = data.factions.len();
    let k = ckey(cell);
    let h = |a: usize, salt: i64| hash_coord(seed ^ 0x415E, k, a as i64, salt);
    let mut ages: Vec<Age> = Vec::with_capacity(AGES);
    let mut holder: Option<usize> = None;
    for i in 0..AGES {
        let kind = if i == 0 {
            if unit(h(0, 1)) < 0.7 {
                ESSOR
            } else {
                MIGRATION
            }
        } else {
            let prev = ages[i - 1].kind;
            pick_weighted(&NEXT[prev], unit(h(i, 1)))
        };
        // A new people comes with a rise, a migration or a reconquest.
        if i == 0 || matches!(kind, ESSOR | MIGRATION | RECONQUETE) {
            holder = if n_f > 0 && unit(h(i, 2)) < 0.8 {
                Some((h(i, 3) % n_f as u64) as usize)
            } else {
                None
            };
        }
        let start = EPOCH_YEAR - AGE_YEARS * (AGES - i) as i32;
        ages.push(Age {
            kind,
            start,
            end: start + AGE_YEARS,
            holder,
            cause: None,
            figure: String::new(),
            role: role_of(kind),
        });
    }
    // The last age agrees with the present.
    let last = AGES - 1;
    if ages[last].holder != holder_now {
        let prev = ages[last - 1].kind;
        ages[last].kind = match (holder_now, prev) {
            (None, _) => AGE_SOMBRE,
            (_, EFFONDREMENT | AGE_SOMBRE) => RECONQUETE,
            _ => MIGRATION,
        };
        ages[last].holder = holder_now;
        ages[last].role = role_of(ages[last].kind);
    }
    for (i, age) in ages.iter_mut().enumerate() {
        // A collapse has a cause (weighted), and a figure; everything is told by its own hash.
        if age.kind == EFFONDREMENT {
            let w: Vec<f64> = data.history.causes.iter().map(|c| c.weight).collect();
            age.cause = Some(pick_weighted(&w, unit(h(i, 4))));
        }
        age.figure = person(data, h(i, 5));
    }
    // The place is named after the greatest figure: the emperor of the apogee, else the founder.
    let star = ages
        .iter()
        .find(|a| a.kind == APOGEE)
        .or_else(|| ages.iter().find(|a| a.kind == ESSOR))
        .unwrap_or(&ages[0]);
    let family = star
        .figure
        .rsplit(' ')
        .next()
        .unwrap_or("l'oubli")
        .to_string();
    Chronicle {
        cell,
        ages,
        name: if family.starts_with(['A', 'E', 'I', 'O', 'U', 'Y']) {
            format!("Marche d'{family}")
        } else {
            format!("Marche de {family}")
        },
    }
}

/// How the age is told in one line.
pub fn age_line(data: &GameData, a: &Age) -> String {
    let def = &data.history.ages[a.kind];
    let people = a
        .holder
        .map_or("un peuple oublie", |f| data.factions[f].name.as_str());
    let mut s = format!(
        "AN {} A {} : {} ({}). {} {}",
        a.start,
        a.end,
        def.name.to_uppercase(),
        people,
        a.role.to_uppercase(),
        a.figure
    );
    if let Some(c) = a.cause {
        s.push_str(&format!(" - cause : {}", data.history.causes[c].name));
    }
    s
}

/// The lines of the archives of a sector: the recent years from `politics` (newest first), then
/// the ages, newest first.
pub fn lines(data: &GameData, galaxy: &Galaxy, seed: u64, cell: Cell, t: f64) -> Vec<String> {
    let holder = politics::faction_of(data, galaxy, cell);
    let ch = chronicle(data, seed, cell, holder);
    let mut out = vec![format!(
        "{} - secteur {} {} {}",
        ch.name.to_uppercase(),
        cell[0],
        cell[1],
        cell[2]
    )];
    let ev = politics::events_in_cell(data, galaxy, seed, cell, (t - 8.0 * YEAR).max(0.0), t);
    for e in ev.iter().rev().take(6) {
        let txt = match &e.kind {
            politics::PolKind::Succession { old, new, how, .. } => {
                format!("{old} quitte le pouvoir ({how:?}) ; {new} lui succede")
            }
            politics::PolKind::War { fa, fb, .. } => format!(
                "guerre entre {} et {}",
                fa.map_or("les sans-nom", |f| data.factions[f].name.as_str()),
                fb.map_or("les sans-nom", |f| data.factions[f].name.as_str())
            ),
            politics::PolKind::Peace { fa, fb, .. } => format!(
                "la paix revient entre {} et {}",
                fa.map_or("les sans-nom", |f| data.factions[f].name.as_str()),
                fb.map_or("les sans-nom", |f| data.factions[f].name.as_str())
            ),
        };
        out.push(format!("AN {:.0} : {txt}", year_at(e.time)));
    }
    for a in ch.ages.iter().rev() {
        out.push(age_line(data, a));
    }
    out
}

/// Where a ruin, a wreck or a station comes from, consistent with the chronicle.
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    /// Index of the age the site was built in.
    pub age: usize,
    pub builder: Option<usize>,
    pub built: i32,
    pub fallen: i32,
    pub cause: Option<usize>,
}

/// Provenance of a site (`kind`: "ruine", "station_ancienne" or "epave"; `key`: its own hash).
pub fn provenance(
    data: &GameData,
    seed: u64,
    cell: Cell,
    holder_now: Option<usize>,
    kind: &str,
    key: u64,
) -> Provenance {
    let ch = chronicle(data, seed, cell, holder_now);
    let now = EPOCH_YEAR;
    let u = |salt: i64| unit(hash_coord(seed ^ 0x9B07, ckey(cell), key as i64, salt));
    // Wrecks are recent; ancient stations are mid-age; ruins date from any rise or apogee.
    if kind == "epave" {
        let built = now - 5 - (u(1) * 150.0) as i32;
        let fallen = built + 1 + (u(2) * f64::from(now - built - 1)) as i32;
        let cause = if u(3) < 0.6 { 0 } else { 4 };
        return Provenance {
            age: AGES - 1,
            builder: holder_now,
            built,
            fallen,
            cause: Some(cause.min(data.history.causes.len() - 1)),
        };
    }
    let builders: Vec<usize> = (0..AGES)
        .filter(|&i| matches!(ch.ages[i].kind, ESSOR | APOGEE | RECONQUETE))
        .collect();
    let pool = if kind == "station_ancienne" {
        let recent: Vec<usize> = builders.iter().copied().filter(|&i| i >= 4).collect();
        if recent.is_empty() {
            builders.clone()
        } else {
            recent
        }
    } else {
        builders.clone()
    };
    // A sector that never rose (all migrations): the ruin is of the first people to pass.
    let age = if pool.is_empty() {
        0
    } else {
        pool[(u(4) * pool.len() as f64) as usize % pool.len()]
    };
    let a = &ch.ages[age];
    let built = a.start + (u(5) * f64::from(AGE_YEARS - 1)) as i32;
    // It falls with the first collapse after it, else when the people leaves (at the end of the
    // age that follows), but always before today.
    let fall_age = (age..AGES).find(|&i| ch.ages[i].kind == EFFONDREMENT);
    let (fallen, cause) = match fall_age {
        Some(i) => {
            let f = &ch.ages[i];
            (
                (f.start + (u(6) * f64::from(AGE_YEARS)) as i32).max(built + 1),
                f.cause,
            )
        }
        None => (
            (built + 1 + (u(7) * f64::from(now - built - 1)) as i32).min(now - 1),
            Some(4.min(data.history.causes.len() - 1)),
        ),
    };
    Provenance {
        age,
        builder: a.holder,
        built,
        fallen: fallen.min(now - 1),
        cause,
    }
}

/// One-line account of a provenance.
pub fn provenance_line(data: &GameData, p: &Provenance) -> String {
    let who = p
        .builder
        .map_or("un peuple oublie", |f| data.factions[f].name.as_str());
    let why = p
        .cause
        .map_or("abandon", |c| data.history.causes[c].name.as_str());
    format!(
        "Edifie par {who} en l'an {}, tombe en l'an {} ({why}).",
        p.built, p.fallen
    )
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
    fn a_sector_has_one_history_whatever_the_call_and_it_depends_on_the_seed() {
        let d = data();
        let a = chronicle(&d, 7, [3, -2, 5], Some(1));
        assert_eq!(a, chronicle(&d, 7, [3, -2, 5], Some(1)), "deterministic");
        let other = (0..6).any(|s| chronicle(&d, 8 + s, [3, -2, 5], Some(1)) != a);
        assert!(other, "the seed changes the past");
        assert!(
            (0..12).any(|c| chronicle(&d, 7, [c, 0, 0], Some(1)).ages != a.ages),
            "sectors differ"
        );
    }

    #[test]
    fn ages_are_contiguous_the_last_agrees_with_the_present_and_collapses_have_causes() {
        let d = data();
        for s in 0..40u64 {
            for now in [None, Some(0), Some(3)] {
                let c = chronicle(&d, s, [s as i64 % 5, 1, -1], now);
                assert_eq!(c.ages.len(), AGES);
                assert_eq!(c.ages[AGES - 1].end, EPOCH_YEAR, "ends today");
                assert_eq!(c.ages[AGES - 1].holder, now, "present holder");
                for w in c.ages.windows(2) {
                    assert_eq!(w[0].end, w[1].start, "contiguous");
                }
                for a in &c.ages {
                    assert_eq!(
                        a.cause.is_some(),
                        a.kind == EFFONDREMENT,
                        "a cause for each collapse and only for them"
                    );
                }
                // No collapse without a rise before it somewhere in the chain.
                if let Some(i) = c.ages.iter().position(|a| a.kind == EFFONDREMENT) {
                    assert!(c.ages[..i]
                        .iter()
                        .any(|a| matches!(a.kind, ESSOR | APOGEE | RECONQUETE)));
                }
            }
        }
    }

    #[test]
    fn ruins_are_dated_inside_the_chronicle_of_their_sector() {
        let d = data();
        for s in 0..60u64 {
            let cell = [s as i64 % 7 - 3, 2, s as i64 / 7];
            let holder = Some((s % 8) as usize % d.factions.len());
            let ch = chronicle(&d, s, cell, holder);
            for kind in ["ruine", "station_ancienne", "epave"] {
                let p = provenance(&d, s, cell, holder, kind, s * 31 + 5);
                assert!(p.built < p.fallen, "{kind}: built before it fell");
                assert!(p.fallen < EPOCH_YEAR, "{kind}: fell in the past");
                if kind != "epave" {
                    let a = &ch.ages[p.age];
                    assert!((a.start..a.end).contains(&p.built), "built within its age");
                    assert_eq!(p.builder, a.holder, "builder is the holder of that age");
                    if let Some(i) = (p.age..AGES).find(|&i| ch.ages[i].kind == EFFONDREMENT) {
                        assert_eq!(p.cause, ch.ages[i].cause, "same cause as the chronicle");
                    }
                }
            }
        }
    }

    #[test]
    fn the_present_is_told_by_politics_and_a_place_is_named_after_its_history() {
        let d = data();
        let g = Galaxy::new(5);
        let l = lines(&d, &g, 5, [0, 0, 0], 30.0 * YEAR);
        assert!(l[0].starts_with("MARCHE D"), "{l:?}");
        assert!(l.len() >= AGES, "ages are listed");
        assert!(
            l.iter()
                .any(|x| x.starts_with("AN 24") && !x.contains(" A ")),
            "the recent years are told by politics: {l:?}"
        );
        assert_eq!(l, lines(&d, &g, 5, [0, 0, 0], 30.0 * YEAR));
    }
}
