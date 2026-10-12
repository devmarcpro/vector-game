//! Links between systems added by the second interaction audit (see docs/MATRICE_INTERACTIONS.md):
//! small functions that read the state of one system to act on another. Each one is bounded and
//! tested in `links_tests.rs`.
use crate::Sim;

/// Old-scale points of goodwill (see `relations::FACTION_SCALE`) per day that a colony of
/// 1 500 people (half weight) gives or takes at full score, and the standing at which a faction
/// stops being won over (or turned against) by colonies alone.
pub const COLONY_REP_RATE: f32 = 0.05;
pub const COLONY_REP_CAP: f32 = 60.0;
/// Population at which the host faction pays full attention to a colony.
pub const COLONY_REP_POP: f64 = 1500.0;
/// Goodwill is handed over in lumps of at least this many points (the history of the relations
/// is not flooded by daily crumbs).
pub const COLONY_REP_LUMP: f32 = 0.5;
/// How many times its usual stock a station market holds at most when a colony fills it.
pub const MARKET_ROOM: f64 = 6.0;

impl Sim {
    /// The station whose shelves serve the port of colony `ci` (the nearest one of the system
    /// the ship is in), if any.
    fn colony_market_station(&self, ci: usize) -> Option<usize> {
        let c = self.colonies.get(ci)?;
        if c.star != self.here {
            return None;
        }
        let at = self.bodies.get(c.body)?.pos;
        (0..self.bodies.len())
            .filter(|&i| self.bodies[i].kind == vector_world::BodyKind::Station)
            .min_by(|&a, &b| {
                (self.bodies[a].pos - at)
                    .length()
                    .total_cmp(&(self.bodies[b].pos - at).length())
            })
    }

    /// After the colonies advanced: what went through their ports moves the stock of the market
    /// of the station nearest to them (imports take from its shelves, exports fill them, the
    /// shelves hold between nothing and `MARKET_ROOM` times their usual stock). What the shelves
    /// cannot give or take comes from, or goes to, the rest of the sector by the regular lines.
    pub(crate) fn settle_station_food(&mut self) {
        let Some(fi) = self.data.resource_index("food") else {
            return;
        };
        for ci in 0..self.colonies.len() {
            let flow = std::mem::take(&mut self.colonies[ci].works.trade_flow);
            if flow == 0.0 {
                continue;
            }
            if let Some(st) = self.colony_market_station(ci) {
                let m = self.market(st);
                let cap = MARKET_ROOM * m.reference[fi];
                m.stock[fi] = (m.stock[fi] - flow).clamp(0.0, cap.max(m.stock[fi]));
            }
        }
    }

    /// The faction that holds the sector colony `ci` lies in.
    pub fn colony_faction(&self, ci: usize) -> Option<usize> {
        let c = self.colonies.get(ci)?;
        let pos = self
            .visited
            .get(&c.star)
            .copied()
            .or_else(|| self.galaxy.star(c.star).map(|s| s.pos))?;
        crate::politics::faction_of(&self.data, &self.galaxy, crate::politics::cell_of(pos))
    }

    /// How much colony `ci` pleases the people whose space it lies in, -1..1: prosperity and
    /// taxes paid on one side; crime, unrest, a bankrupt treasury and a sickness that spreads
    /// into the sector on the other. A war in the sector makes the host pay more attention.
    pub fn colony_standing(&self, ci: usize) -> f64 {
        let Some(c) = self.colonies.get(ci) else {
            return 0.0;
        };
        if c.pop < 1.0 {
            return 0.0;
        }
        let prosperity = 0.5 * (c.happiness - 0.5) * 2.0;
        let taxes = if c.stats.income - c.stats.upkeep > 0.0 {
            0.2
        } else {
            -0.1
        } - if c.debt_days > 0.0 { 0.3 } else { 0.0 };
        let crime = -(c.works.crime - 0.2).max(0.0) * 1.5;
        let unrest = -(c.unrest - 0.4).max(0.0);
        let sick = -(c.sick() / c.pop * 5.0).min(1.0) * 0.6;
        let war = self.colony_in_war(ci);
        let s = prosperity + taxes + crime + unrest + sick;
        (s * if war { 1.5 } else { 1.0 }).clamp(-1.0, 1.0)
    }

    fn colony_in_war(&self, ci: usize) -> bool {
        let Some(c) = self.colonies.get(ci) else {
            return false;
        };
        let Some(pos) = self
            .visited
            .get(&c.star)
            .copied()
            .or_else(|| self.galaxy.star(c.star).map(|s| s.pos))
        else {
            return false;
        };
        crate::politics::sector_at_war(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            crate::politics::cell_of(pos),
            self.time,
        )
    }

    /// Colonies and the faction of their sector: for `days` days, goodwill drifts with the
    /// standing of the colony, in lumps, and never past `COLONY_REP_CAP` either way (a thriving
    /// colony alone does not make a friend of anyone for good).
    pub(crate) fn colony_relations_tick(&mut self, ci: usize, days: f64) {
        if days <= 0.0 {
            return;
        }
        let Some(f) = self.colony_faction(ci) else {
            return;
        };
        let s = self.colony_standing(ci);
        let pop = self.colonies[ci].pop;
        let weight = pop / (pop + COLONY_REP_POP);
        let acc = &mut self.colonies[ci].works.rel_acc;
        *acc += (s * weight * f64::from(COLONY_REP_RATE) * days) as f32;
        if acc.abs() < COLONY_REP_LUMP {
            return;
        }
        let lump = std::mem::take(acc);
        let rep = self.reputation_with(Some(f));
        if (lump > 0.0 && rep >= COLONY_REP_CAP) || (lump < 0.0 && rep <= -COLONY_REP_CAP) {
            return;
        }
        self.change_reputation(Some(f), lump);
    }
}

// ---- the weather of the site over a landing -----------------------------------------------------

use crate::planet::Weather;
use vector_core::Vec3;

/// Altitude (m) above which the wind of the ground no longer blows on the ship.
pub const WIND_TOP: f64 = 8_000.0;

/// What the weather of the ground under the ship does to the approach and the touch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteWeather {
    pub weather: Weather,
    /// 0..1 how hard it is.
    pub severity: f64,
    /// Speed of the wind at the ground (m/s) and the direction it blows to, a unit vector of the
    /// world frame (tangent to the ground where it was drawn).
    pub wind: f64,
    pub dir: Vec3,
    /// 1 = clear view, 0 = blind (the ship reads the ground late in dust, snow and storm).
    pub visibility: f64,
    /// 1 = dry ground; rain, snow and ice take the grip that a landing leans on (the share of the
    /// sliding speed the gear absorbs).
    pub grip: f64,
}

impl SiteWeather {
    /// The effects of a weather of severity `sev` (0..1) on a world whose air weighs `pressure`
    /// bar; `up` is the vertical of the place and `h` a hash that draws the direction of the wind.
    pub fn of(weather: Weather, sev: f64, pressure: f64, up: Vec3, h: u64) -> SiteWeather {
        let sev = sev.clamp(0.0, 1.0);
        let (wind, vis, grip) = match weather {
            Weather::Clear => (1.0 + 3.0 * sev, 1.0, 1.0),
            Weather::Cloudy => (3.0 + 5.0 * sev, 0.9, 1.0),
            Weather::Rain => (6.0 + 10.0 * sev, 0.6 - 0.3 * sev, 0.75 - 0.15 * sev),
            Weather::Storm => (22.0 + 28.0 * sev, 0.3 - 0.2 * sev, 0.6),
            Weather::Snow => (4.0 + 8.0 * sev, 0.5 - 0.3 * sev, 0.55),
            Weather::Blizzard => (18.0 + 27.0 * sev, 0.15, 0.35),
            Weather::Heat => (3.0 + 5.0 * sev, 0.9, 1.0),
            Weather::Dust => (14.0 + 20.0 * sev, 0.25 - 0.1 * sev, 0.9),
        };
        let thin = if pressure < 0.01 { 0.0 } else { 1.0 };
        let a = crate::planet::unit(h) * std::f64::consts::TAU;
        let east = {
            let e = up.cross(Vec3::new(0.0, 1.0, 0.0));
            if e.length() < 1e-6 {
                up.cross(Vec3::new(1.0, 0.0, 0.0)).normalized()
            } else {
                e.normalized()
            }
        };
        let north = up.cross(east).normalized();
        SiteWeather {
            weather,
            severity: sev,
            wind: wind * thin,
            dir: east * a.cos() + north * a.sin(),
            visibility: vis,
            grip,
        }
    }

    /// The velocity of the air at altitude `alt` over a place with vertical `up`: the wind of the
    /// ground, fading to nothing at `WIND_TOP`, along the ground there.
    pub fn air_velocity(&self, up: Vec3, alt: f64) -> Vec3 {
        let k = (1.0 - alt / WIND_TOP).clamp(0.0, 1.0);
        let d = self.dir - up * self.dir.dot(up);
        if d.length() < 1e-9 {
            return Vec3::ZERO;
        }
        d.normalized() * (self.wind * k)
    }

    /// Whether it is bad enough to warn the pilot (alerts).
    pub fn rough(&self) -> bool {
        self.wind >= 15.0 || self.grip < 0.7 || self.visibility < 0.4
    }
}

type WeatherKey = (i64, usize, usize, usize);

impl Sim {
    /// The weather over the point of body `body` straight under `up` (a world-frame unit
    /// vector), drawn once per day and region and kept (so the physics of a descent does not ask
    /// the surface map again every step). None for a world with no air or no ground.
    pub(crate) fn site_weather(&mut self, body: usize, up: Vec3) -> Option<SiteWeather> {
        if !self.landable(body) || self.air_of(body).is_none() {
            return None;
        }
        let nb = self.body_orient(body).transpose().mul_vec(up);
        let (cx, cy) = vector_world::geology::cell_of([nb.x, nb.y, nb.z]);
        let (rx, ry) = crate::planet::region_of(cx, cy);
        let key: WeatherKey = (self.day_number(), body, rx, ry);
        if let Some((k, w)) = self.site_weather_cache {
            if k == key {
                return Some(w);
            }
        }
        let ck = (self.system.seed, body);
        if !self.world_cache.contains_key(&ck) {
            let map = crate::planet::build_map(&self.data, &self.system, body);
            self.world_cache.insert(ck, map.map(std::sync::Arc::new));
        }
        let map = self.world_cache.get(&ck)?.clone()?;
        let (w, sev) = self.weather_at(&map, cx, cy);
        let h = vector_core::hash_coord(map.seed ^ 0x5171, self.day_number(), rx as i64, ry as i64);
        let sw = SiteWeather::of(w, sev, map.pressure, up, h);
        self.site_weather_cache = Some((key, sw));
        Some(sw)
    }

    /// The weather last drawn for `body`, today (read-only: the physics fills it in flight).
    pub fn site_weather_now(&self, body: usize) -> Option<SiteWeather> {
        let ((day, b, _, _), w) = self.site_weather_cache?;
        (day == self.day_number() && b == body).then_some(w)
    }

    /// The wind over the ship, if it is in the air of a world that has weather; the ship rests
    /// against the ground when it is landed (no wind then).
    pub(crate) fn wind_over_ship(&mut self, body: usize) -> Vec3 {
        if self.parked.is_some_and(|p| p.0 == body) || self.docked.is_some() {
            return Vec3::ZERO;
        }
        let rel = self.ship.pos - self.bodies[body].pos;
        let d = rel.length();
        if d < 1.0 {
            return Vec3::ZERO;
        }
        let up = rel * (1.0 / d);
        let alt = d - self.bodies[body].radius;
        match self.site_weather(body, up) {
            Some(w) => w.air_velocity(up, alt),
            None => Vec3::ZERO,
        }
    }

    /// Grip of the ground under the ship on `body` for a touch down now (1 = dry).
    pub(crate) fn ground_grip(&mut self, body: usize) -> f64 {
        let rel = self.ship.pos - self.bodies[body].pos;
        let d = rel.length();
        if d < 1.0 {
            return 1.0;
        }
        self.site_weather(body, rel * (1.0 / d))
            .map_or(1.0, |w| w.grip)
    }
}
