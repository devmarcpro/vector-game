//! The news: a feed of what really happens in the world, shown in the ticker and the journal.
//! Nothing is invented for the feed: each item tells a real change of state (a war that started
//! and moved the price of arms, a head of state who fell, a company hit by an incident that moved
//! its prices, an event of the exchange). The world is infinite, so news is a matter of distance:
//! only the sectors around the ship are listened to, and what happens in a faraway sector is
//! heard late (the couriers carry it at a few light years per day), so a distant event is old when
//! it arrives. Each item says how far away it happened.
use crate::bourse::DAY;
use crate::epidemic;
use crate::incidents::{self, KINDS};
use crate::politics::{self, cell_center, cell_of, cells_around, EndKind, PolKind, SECTOR_LY};
use crate::Sim;
use serde::{Deserialize, Serialize};

pub const NEWS_MAX: usize = 80;
/// How far back the feed looks when a game starts (so the ticker is never empty).
pub const BACKFILL_DAYS: f64 = 25.0;
/// Sectors listened to around the ship, and the speed of the couriers (ly per day) beyond the
/// ship's own sector.
pub const HEARING_CELLS: i64 = 2;
pub const COURIER_LY_DAY: f64 = 4.0;
pub const TAGS: [&str; 8] = [
    "POLITIQUE",
    "GUERRE",
    "ENTREPRISES",
    "BOURSE",
    "COLONIES",
    "SCIENCE",
    "CRIME",
    "SANTE",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewsItem {
    /// When it was heard.
    pub time: f64,
    pub tag: u8,
    /// How far away it happened, ly (0 for what happens aboard or in the system).
    pub dist_ly: f32,
    pub text: String,
}

impl NewsItem {
    /// "ICI", "PROCHE", "REGION" or "LOINTAIN".
    pub fn scale(&self) -> &'static str {
        match self.dist_ly {
            d if d < 1.0 => "ICI",
            d if d < SECTOR_LY as f32 / 2.0 => "SECTEUR",
            d if d < SECTOR_LY as f32 * 1.5 => "REGION",
            _ => "LOINTAIN",
        }
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Sim {
    pub fn news_push(&mut self, time: f64, tag: u8, dist_ly: f64, text: String) {
        self.news.push(NewsItem {
            time,
            tag,
            dist_ly: dist_ly as f32,
            text,
        });
        self.news.sort_by(|a, b| a.time.total_cmp(&b.time));
        let n = self.news.len();
        if n > NEWS_MAX {
            self.news.drain(0..n - NEWS_MAX);
        }
    }

    /// Name of the place of a sector: the nearest star to its middle.
    fn place_name(&self, pos: [f64; 3]) -> String {
        // The nearest star: look close first (the search generates every sector it touches).
        let mut stars = self.galaxy.nearby(pos, 50.0);
        if stars.is_empty() {
            stars = self.galaxy.nearby(pos, SECTOR_LY * 0.8);
        }
        stars.sort_by(|a, b| a.distance_to(pos).total_cmp(&b.distance_to(pos)));
        stars
            .first()
            .map_or_else(|| "l'espace profond".to_string(), |s| s.header.name.clone())
    }

    /// Once a day: tell what the sectors around the ship have seen happen, as it reaches us.
    pub(crate) fn news_tick(&mut self) {
        let to = self.news_scanned_to;
        if self.time - to < DAY {
            return;
        }
        let from = if to == f64::NEG_INFINITY {
            self.time - BACKFILL_DAYS * DAY
        } else {
            to.max(self.time - 60.0 * DAY)
        };
        self.news_scanned_to = self.time;
        let (h0, h1) = (from, self.time);
        // The journals of your own colonies make the news, wherever they are; like any news
        // they travel, so a colony far from the ship is heard late (and says how far it is).
        let mut own: Vec<(f64, f64, String)> = Vec::new();
        for col in &self.colonies {
            let d = self
                .galaxy
                .star(col.star)
                .map_or(0.0, |e| e.distance_to(self.here_pos));
            let delay = (d - SECTOR_LY / 2.0).max(0.0) / COURIER_LY_DAY * DAY;
            for (t, line) in &col.log {
                if *t > h0 - delay && *t <= h1 - delay {
                    own.push((*t + delay, d, format!("{} : {line}", col.name)));
                }
            }
        }
        for (t, d, text) in own {
            self.news_push(t, if text.contains("pidemie") { 7 } else { 4 }, d, text);
        }
        let data = std::sync::Arc::clone(&self.data);
        let seed = self.galaxy.seed;
        let me = self.here_pos;
        let galaxy = vector_world::galaxy::Galaxy::new(seed);
        for c in cells_around(cell_of(me), HEARING_CELLS) {
            let d = dist(cell_center(c), me);
            // News of the ship's own sector is immediate; the rest travels.
            let delay = (d - SECTOR_LY / 2.0).max(0.0) / COURIER_LY_DAY * DAY;
            let (e0, e1) = (h0 - delay, h1 - delay);
            let name = |f: Option<usize>| {
                f.map_or("un monde libre".to_string(), |f| {
                    data.factions[f].name.clone()
                })
            };
            for e in politics::events_in_cell(&data, &galaxy, seed, c, e0, e1) {
                let place = self.place_name(e.pos);
                let (tag, text) = match e.kind {
                    PolKind::War { fa, fb, .. } => (
                        1,
                        format!(
                            "Guerre pres de {place} : {} et {} s'affrontent ; les armes et le carburant s'envolent dans la region.",
                            name(fa),
                            name(fb)
                        ),
                    ),
                    PolKind::Peace { fa, fb, .. } => (
                        1,
                        format!(
                            "Cessez-le-feu pres de {place} entre {} et {} : les marches se detendent.",
                            name(fa),
                            name(fb)
                        ),
                    ),
                    PolKind::Succession {
                        faction, old, new, how, ..
                    } => {
                        let f = name(faction);
                        (
                            0,
                            match how {
                                EndKind::Election => format!(
                                    "{place} : {old} quitte le pouvoir, {new} est elu(e) a la tete du secteur ({f})."
                                ),
                                EndKind::Assassination => format!(
                                    "{place} : {old} a ete assassine ! {new} prend la tete du secteur ({f}) dans le chaos."
                                ),
                                EndKind::Coup => format!(
                                    "Coup d'Etat pres de {place} : {old} est renverse, {new} s'empare du pouvoir ({f})."
                                ),
                                EndKind::Resignation => format!(
                                    "{place} : {old} demissionne ; {new} lui succede ({f})."
                                ),
                            },
                        )
                    }
                };
                self.news_push(e.time + delay, tag, d, text);
            }
            for (t, k, at) in epidemic::declarations_in_cell(&data, seed, c, e0, e1) {
                let place = self.place_name(at);
                let pa = &data.pathogens[k];
                self.news_push(
                    t + delay,
                    7,
                    d,
                    format!(
                        "Epidemie de {} declaree pres de {place} : {} Les medicaments s'envolent dans la region.",
                        pa.name, pa.text
                    ),
                );
            }
            for inc in incidents::incidents_in_cell(&data, seed, c, e0, e1) {
                if inc.start > e0 && inc.start <= e1 {
                    let place = self.place_name(incidents::place(&inc));
                    let text = KINDS[inc.kind]
                        .1
                        .replace("{c}", &data.companies[inc.company].name)
                        .replace("{p}", &place);
                    self.news_push(inc.start + delay, 2, d, text);
                }
            }
        }
    }

    /// The newest `n` items, newest first.
    pub fn headlines(&self, n: usize) -> Vec<&NewsItem> {
        self.news.iter().rev().take(n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    #[test]
    fn the_feed_tells_real_events_once_oldest_first_and_stays_bounded() {
        let mut s = new_sim(5, data());
        s.time = 300.0 * DAY;
        s.news_tick();
        let first = s.news.len();
        s.news_tick();
        assert_eq!(s.news.len(), first, "the same day tells nothing twice");
        for d in 301..2500 {
            s.time = f64::from(d) * DAY;
            s.news_tick();
        }
        assert!(s.news.len() == NEWS_MAX, "{} items", s.news.len());
        assert!(s.news.windows(2).all(|w| w[0].time <= w[1].time));
        let mut texts: Vec<_> = s.news.iter().map(|n| n.text.clone()).collect();
        texts.sort();
        texts.dedup();
        let mut all: Vec<_> = s.news.iter().map(|n| n.text.clone()).collect();
        all.sort();
        let dup: Vec<_> = all.windows(2).filter(|w| w[0] == w[1]).collect();
        assert_eq!(texts.len(), s.news.len(), "no item twice: {dup:?}");
        assert!(s.news.iter().all(|n| usize::from(n.tag) < TAGS.len()));
        let tags: std::collections::BTreeSet<u8> = s.news.iter().map(|n| n.tag).collect();
        assert!(
            tags.len() >= 3,
            "politics, wars and companies all make the news: {tags:?}"
        );
        assert_eq!(s.headlines(5).len(), 5);
        assert!(
            s.headlines(5)[0].time >= s.headlines(5)[4].time,
            "newest first"
        );
    }

    #[test]
    fn distant_news_arrives_late_and_says_how_far_it_is() {
        let mut s = new_sim(5, data());
        let me = s.here_pos;
        // The nearest sector at war with a neighbour whose declaration lies inside the first
        // 70 years; the news is heard when the couriers have carried it.
        let d = Arc::clone(&s.data);
        let galaxy = vector_world::galaxy::Galaxy::new(s.galaxy.seed);
        let mut found = None;
        'seek: for c in cells_around(cell_of(me), HEARING_CELLS) {
            for m in 1..300 {
                let t = m as f64 * politics::WAR_PERIOD;
                for e in politics::events_in_cell(&d, &galaxy, s.galaxy.seed, c, t - DAY, t) {
                    if matches!(e.kind, PolKind::War { .. }) {
                        found = Some((c, e));
                        break 'seek;
                    }
                }
            }
        }
        let (c, e) = found.expect("a war within hearing in 50 years");
        let dist_ly = dist(cell_center(c), me);
        let delay = (dist_ly - SECTOR_LY / 2.0).max(0.0) / COURIER_LY_DAY * DAY;
        // Heard just before the couriers arrive: nothing. After: the item, dated at arrival.
        s.time = e.time - 3.0 * DAY;
        s.news_tick();
        s.news.clear();
        s.time = e.time + delay - 0.5 * DAY;
        s.news_tick();
        let before = s
            .news
            .iter()
            .any(|n| n.tag == 1 && (n.dist_ly - dist_ly as f32).abs() < 1.0);
        s.time = e.time + delay + 1.5 * DAY;
        s.news_tick();
        let item = s
            .news
            .iter()
            .find(|n| n.tag == 1 && (n.dist_ly - dist_ly as f32).abs() < 1.0)
            .expect("heard once the couriers arrive");
        assert!(!before || delay < DAY, "a distant war is not heard at once");
        assert!((item.time - (e.time + delay)).abs() < 1.0);
        assert!(!item.scale().is_empty());
        // The war in the news is a war in the prices of that sector.
        let arms = d.resource_index("ammo_slugs").unwrap();
        let tm = politics::turmoil(&d, &galaxy, s.galaxy.seed, cell_center(c), e.time + DAY);
        assert!(tm.at_war && tm.factor(&d, arms) >= 1.25 - 1e-9);
    }

    #[test]
    fn what_happens_in_your_colonies_makes_the_news_at_once() {
        let (mut s, ci) = crate::colony::tests::founded();
        let name = s.colonies[ci].name.clone();
        let colony_news = |s: &Sim| s.news.iter().filter(|n| n.text.starts_with(&name)).count();
        s.time = 400.0 * DAY;
        s.news_tick();
        let before = colony_news(&s);
        s.time += 2.0 * DAY;
        s.colonies[ci].log_line(s.time - DAY, "Une epidemie se declare.");
        s.news_tick();
        assert_eq!(colony_news(&s), before + 1);
        let n = s
            .news
            .iter()
            .rev()
            .find(|n| n.text.contains("epidemie"))
            .unwrap();
        assert_eq!(n.tag, 7, "an epidemic is health news");
        assert!(n.text.contains("epidemie") && n.text.contains(&s.colonies[ci].name));
        s.time += 0.5 * DAY;
        s.news_tick();
        assert_eq!(colony_news(&s), before + 1, "told once");
    }
}
