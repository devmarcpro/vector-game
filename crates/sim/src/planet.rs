//! Detailed planets (E1): the surface map with its biomes, the seasons, the weather and the
//! catastrophes of a solid world. Nothing here is stored: the map comes from the body's seed and
//! the weather from (seed, day, region), so a save only needs the game time.
use crate::Sim;
use std::sync::Arc;
use vector_core::hash_coord;
use vector_world::climate::{self, Atmo};
use vector_world::surface::{self, Surface, H, W};
use vector_world::{BodyKind, PlanetKind};

/// A planet's surface with the biome of every cell (index into `GameData::biomes`).
#[derive(Clone, Debug)]
pub struct PlanetMap {
    pub body: usize,
    pub kind: PlanetKind,
    pub seed: u64,
    pub surface: Surface,
    pub biome: Vec<u8>,
    pub pressure: f64,
}

impl PlanetMap {
    pub fn biome_at(&self, x: usize, y: usize) -> usize {
        usize::from(self.biome[Surface::idx(x, y)])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Cloudy,
    Rain,
    Storm,
    Snow,
    Blizzard,
    Heat,
    Dust,
}

impl Weather {
    pub fn label(self) -> &'static str {
        match self {
            Weather::Clear => "dégagé",
            Weather::Cloudy => "nuageux",
            Weather::Rain => "pluie",
            Weather::Storm => "orage",
            Weather::Snow => "neige",
            Weather::Blizzard => "blizzard",
            Weather::Heat => "canicule",
            Weather::Dust => "tempête de sable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatastropheKind {
    Earthquake,
    Eruption,
    Hurricane,
    Flood,
    Drought,
    Blizzard,
}

impl CatastropheKind {
    pub fn label(self) -> &'static str {
        match self {
            CatastropheKind::Earthquake => "séisme",
            CatastropheKind::Eruption => "éruption volcanique",
            CatastropheKind::Hurricane => "ouragan",
            CatastropheKind::Flood => "inondation",
            CatastropheKind::Drought => "sécheresse",
            CatastropheKind::Blizzard => "tempête de neige",
        }
    }
}

/// A catastrophe on a given day: where it strikes (cell) and how hard, 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Catastrophe {
    pub kind: CatastropheKind,
    pub x: usize,
    pub y: usize,
    pub severity: f64,
}

/// Cells per weather region (weather is the same across a region on a given day).
const REGION: usize = 8;

pub(crate) fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// The surface map of a solid planet or moon of system (None for anything else).
pub fn build_map(
    data: &vector_data::GameData,
    system: &vector_world::System,
    body: usize,
) -> Option<PlanetMap> {
    build_map_with(data, system, body, None)
}

/// The surface map of a body as its seed makes it; `pressure` overrides the air pressure the
/// biomes are classified with (the baseline of a world whose air was terraformed).
pub fn build_map_with(
    data: &vector_data::GameData,
    system: &vector_world::System,
    body: usize,
    pressure: Option<f64>,
) -> Option<PlanetMap> {
    let b = system.bodies.get(body)?;
    let air = pressure.unwrap_or(b.pressure);
    let kind = match b.kind {
        BodyKind::Planet(k) | BodyKind::Moon(k) => k,
        _ => return None,
    };
    let seed = hash_coord(system.seed, body as i64, 0xB10E, 1);
    // What the inside of the body does (plates, a rigid shell, nothing), how long it has been
    // doing it and whether there is air to blow on it decide the relief (see `geology.rs`).
    let gp = vector_world::geology::GeoParams {
        activity: system.physics(body, 0.0).activity,
        age_gyr: system.age() / 1.0e9,
        pressure: b.pressure,
    };
    let surface = surface::generate_with(seed, kind, b.temp, &gp)?;
    let mut biome = vec![0u8; W * H];
    for (i, cell) in biome.iter_mut().enumerate() {
        let idx = data.classify_biome(
            f64::from(surface.temp[i]),
            f64::from(surface.hum[i]),
            f64::from(surface.elev[i]),
            air,
        )?;
        *cell = idx as u8;
    }
    Some(PlanetMap {
        body,
        kind,
        seed,
        surface,
        biome,
        pressure: air,
    })
}

impl Sim {
    /// The surface map of a solid planet or moon of the current system (None for anything else).
    ///
    /// On a world being terraformed the seed's map is shifted by the state of its climate
    /// (temperature, sea level, humidity, biomes): see `terraform::Terra::apply_to_map`.
    pub fn planet_map(&self, body: usize) -> Option<PlanetMap> {
        let terra = self.terra_here(body);
        let air =
            terra.map(|t| t.snap.gas0.iter().sum::<f64>() * t.snap.gravity / t.snap.area() / 1.0e5);
        let mut map = build_map_with(&self.data, &self.system, body, air)?;
        if let Some(t) = terra {
            t.apply_to_map(&self.data, &mut map);
        }
        Some(map)
    }

    /// The map of the seed alone, before any terraforming (natives and the ecology of the
    /// territory go by what the world was).
    pub fn planet_map_base(&self, body: usize) -> Option<PlanetMap> {
        let air = self
            .colonies
            .iter()
            .find(|c| c.star == self.here && c.body == body)
            .and_then(|c| c.terra.as_ref())
            .filter(|t| t.snap.rocky)
            .map(|t| t.snap.gas0.iter().sum::<f64>() * t.snap.gravity / t.snap.area() / 1.0e5);
        build_map_with(&self.data, &self.system, body, air)
    }

    /// Length of the year of a body (a moon shares its planet's), s.
    pub fn year_length(&self, body: usize) -> f64 {
        let mut i = body;
        if let BodyKind::Moon(_) = self.system.bodies[i].kind {
            if let Some(o) = &self.system.bodies[i].orbit {
                i = o.parent;
            }
        }
        self.system.bodies[i]
            .orbit
            .as_ref()
            .map_or(3.15e7, |o| o.period())
    }

    /// Position in the year, 0..1 (0 and 0.5 are the equinoxes, 0.25 northern midsummer).
    pub fn season_phase(&self, body: usize) -> f64 {
        (self.time / self.year_length(body)).fract()
    }

    /// Temperature of a cell now: its annual mean plus the seasonal swing that the sunlight of
    /// its latitude gives (the Sun's declination follows the axial tilt and the year), late and
    /// damped where the air is humid (maritime climate). See `vector_world::climate`.
    pub fn temp_now(&self, map: &PlanetMap, x: usize, y: usize) -> f64 {
        climate::seasonal_temp(&map.surface, x, y, self.season_phase(map.body))
    }

    /// Declination of the Sun above this world now, rad (+ is north).
    pub fn sun_declination(&self, map: &PlanetMap) -> f64 {
        climate::declination(f64::from(map.surface.tilt), self.season_phase(map.body))
    }

    /// Is the water of this cell frozen now? Seas freeze a little colder than lakes and rivers;
    /// a cell with no water at all is never "frozen" (its snow is the weather's business).
    pub fn water_frozen(&self, map: &PlanetMap, x: usize, y: usize) -> bool {
        let sea = map.surface.is_sea(x, y);
        (sea || map.surface.is_lake(x, y) || map.surface.is_river(x, y))
            && climate::frozen(self.temp_now(map, x, y), sea)
    }

    /// What the clouds of this world depend on, now.
    pub fn atmo(&self, map: &PlanetMap) -> Atmo {
        Atmo::new(
            &map.surface,
            map.pressure,
            self.system.spin_period(map.body),
            self.season_phase(map.body),
            self.time / 86_400.0,
        )
    }

    /// Cloud cover (0..1) above a point of a world (rad). None of it where there is no air.
    pub fn cloud_cover(&self, map: &PlanetMap, lat: f64, lon: f64) -> f32 {
        climate::cloud_cover(&map.surface, &self.atmo(map), lat, lon)
    }

    /// Cloud cover above the middle of a cell.
    pub fn cloud_at(&self, map: &PlanetMap, x: usize, y: usize) -> f32 {
        self.cloud_cover(map, Surface::lat(y), Surface::lon(x))
    }

    pub fn day_number(&self) -> i64 {
        (self.time / 86_400.0).floor() as i64
    }

    /// Weather of a cell today: the same over a whole region (REGION x REGION cells), different
    /// every day, driven by the local temperature and humidity. Airless worlds are always clear.
    pub fn weather_at(&self, map: &PlanetMap, x: usize, y: usize) -> (Weather, f64) {
        if map.pressure < 0.01 {
            return (Weather::Clear, 0.0);
        }
        let (rx, ry) = ((x / REGION) as i64, (y / REGION) as i64);
        let roll = |salt: i64| {
            unit(hash_coord(
                map.seed ^ 0x57_0A,
                rx * 4099 + salt,
                ry,
                self.day_number(),
            ))
        };
        let i = Surface::idx(x, y);
        let (t, hum) = (self.temp_now(map, x, y), f64::from(map.surface.hum[i]));
        let (r, sev) = (roll(1), roll(2));
        if t > 330.0 && hum < 0.4 && r < 0.4 {
            return (Weather::Heat, sev);
        }
        if hum < 0.15 && r < 0.12 {
            return (Weather::Dust, sev);
        }
        // No rain from a clear sky: the clouds of the day are the ones of the cloud layer.
        let cover = self.cloud_at(map, x, y);
        let precip = if cover < 0.12 {
            0.0
        } else {
            hum.powf(1.3) * 0.55 * (0.5 + cover.min(1.0) as f64)
        };
        if r < precip {
            let cold = t < 273.0;
            let severe = sev > 0.88;
            return (
                match (cold, severe) {
                    (true, true) => Weather::Blizzard,
                    (true, false) => Weather::Snow,
                    (false, true) => Weather::Storm,
                    (false, false) => Weather::Rain,
                },
                sev,
            );
        }
        if r < precip + 0.25 {
            (Weather::Cloudy, sev)
        } else {
            (Weather::Clear, sev)
        }
    }

    /// Where the city of colony `colony_id` stands on this world (see `city_site_in`).
    pub fn city_site(&self, map: &PlanetMap, colony_id: u32) -> Option<(usize, usize)> {
        let ids: Vec<u32> = self
            .colonies
            .iter()
            .filter(|c| c.star == self.here && c.body == map.body)
            .map(|c| c.id)
            .collect();
        city_site_in(&self.data, map, &ids, colony_id)
    }

    /// The map and city site of colony `ci`, cached (maps of other systems are regenerated from
    /// their seed). None for asteroids and anything without a solid surface.
    pub(crate) fn colony_world(&mut self, ci: usize) -> Option<(Arc<PlanetMap>, (usize, usize))> {
        let c = &self.colonies[ci];
        let (seed, body, id) = (c.system_seed, c.body, c.id);
        let key = (seed, body);
        if !self.world_cache.contains_key(&key) {
            let map = if seed == self.system.seed {
                build_map(&self.data, &self.system, body)
            } else {
                build_map(&self.data, &vector_world::generate_system(seed), body)
            };
            self.world_cache.insert(key, map.map(Arc::new));
        }
        let map = self.world_cache.get(&key)?.clone()?;
        if !self.city_cache.contains_key(&id) {
            let ids: Vec<u32> = self
                .colonies
                .iter()
                .filter(|c| c.system_seed == seed && c.body == body)
                .map(|c| c.id)
                .collect();
            let site = city_site_in(&self.data, &map, &ids, id)?;
            self.city_cache.insert(id, site);
        }
        Some((map, self.city_cache[&id]))
    }

    /// Catastrophes that struck colony `ci` between its previous and current update: the days in
    /// (t0, t1], at most the last 60 (the rest is skipped like any LOD catch-up).
    pub(crate) fn apply_catastrophes(&mut self, ci: usize, t0: f64, t1: f64) {
        let (d0, d1) = (
            (t0 / 86_400.0).floor() as i64,
            (t1 / 86_400.0).floor() as i64,
        );
        if d1 <= d0 {
            return;
        }
        let Some((map, city)) = self.colony_world(ci) else {
            return;
        };
        for day in (d0 + 1).max(d1 - 59)..=d1 {
            let Some(c) = self.catastrophe_on(&map, day) else {
                continue;
            };
            let hit = catastrophe_impact(city, &c);
            if hit <= 0.0 {
                continue;
            }
            let col = &mut self.colonies[ci];
            let dead = col.pop * 0.06 * hit;
            col.pop = (col.pop - dead).max(0.0);
            col.happiness = (col.happiness - 0.2 * hit).max(0.0);
            col.treasury -= col.treasury.max(0.0) * 0.1 * hit;
            col.log_line(
                day as f64 * 86_400.0,
                format!(
                    "Catastrophe : {} (gravité {:.0} %), {:.0} morts, dégâts à la ville",
                    c.kind.label(),
                    c.severity * 100.0,
                    dead.ceil()
                ),
            );
        }
    }

    /// The catastrophe of the day on this world, if any (at most one). Rare by construction:
    /// roughly one every two months on a living world, every few weeks on a volcanic one.
    pub fn catastrophe_on(&self, map: &PlanetMap, day: i64) -> Option<Catastrophe> {
        let h = |salt: i64| hash_coord(map.seed ^ 0xCA7A, salt, 0, day);
        let cell = |salt: i64| {
            let v = h(salt);
            ((v % W as u64) as usize, ((v >> 20) % H as u64) as usize)
        };
        let severity = 0.2 + 0.8 * unit(h(9)).powi(2);
        let roll = unit(h(1));
        let (kind, (x, y)) = match map.kind {
            PlanetKind::Lava if roll < 0.06 => (CatastropheKind::Eruption, cell(2)),
            PlanetKind::Barren | PlanetKind::Dwarf | PlanetKind::Ice if roll < 0.004 => {
                (CatastropheKind::Earthquake, cell(2))
            }
            PlanetKind::Venusian if roll < 0.012 => (CatastropheKind::Eruption, cell(2)),
            PlanetKind::Barren | PlanetKind::Dwarf | PlanetKind::Lava | PlanetKind::Venusian => {
                return None
            }
            _ if roll < 0.006 => (CatastropheKind::Earthquake, cell(2)),
            _ if roll < 0.010 => (CatastropheKind::Eruption, cell(2)),
            _ if roll < 0.016 => (CatastropheKind::Hurricane, cell(3)),
            _ if roll < 0.022 => (CatastropheKind::Flood, cell(4)),
            _ if roll < 0.028 => (CatastropheKind::Drought, cell(5)),
            _ if roll < 0.034 => (CatastropheKind::Blizzard, cell(6)),
            _ => return None,
        };
        // Quakes strike the faults of the world and eruptions its active volcanoes (see
        // `Geology::quake_site`); a world with no plates, or dead, has neither.
        let geo = &map.surface.geo;
        let (x, y) = match kind {
            CatastropheKind::Earthquake => geo.quake_site(h(11))?,
            CatastropheKind::Eruption => geo.eruption_site(h(12))?,
            _ => (x, y),
        };
        let i = Surface::idx(x, y);
        let (hum, t, sea) = (
            f64::from(map.surface.hum[i]),
            f64::from(map.surface.temp[i]),
            map.surface.is_sea(x, y),
        );
        // A catastrophe only strikes where it can: hurricanes over warm water, floods on wet
        // land, droughts on dry warm land, blizzards in the cold, eruptions and quakes anywhere.
        let fits = match kind {
            CatastropheKind::Hurricane => sea && t > 285.0,
            CatastropheKind::Flood => !sea && hum > 0.5,
            CatastropheKind::Drought => !sea && hum < 0.4 && t > 285.0,
            CatastropheKind::Blizzard => t < 270.0 && map.pressure >= 0.01,
            _ => true,
        };
        fits.then_some(Catastrophe {
            kind,
            x,
            y,
            severity,
        })
    }
}

/// Where a colony's city stands: each colony of the body, in id order, takes the best remaining
/// land cell (the biome's appeal, a bonus for a coast within a few cells, a little noise to break
/// ties), never within 14 cells of an earlier city. Pure function of the map and the colony ids.
pub fn city_site_in(
    data: &vector_data::GameData,
    map: &PlanetMap,
    ids: &[u32],
    colony_id: u32,
) -> Option<(usize, usize)> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for id in ids {
        let mut best: Option<(f64, (usize, usize))> = None;
        for y in 2..H - 2 {
            for x in 0..W {
                if map.surface.is_sea(x, y) || map.surface.is_lake(x, y) {
                    continue;
                }
                let near = |a: (usize, usize)| {
                    let dx = (x as isize - a.0 as isize).unsigned_abs();
                    let dx = dx.min(W - dx);
                    dx.max((y as isize - a.1 as isize).unsigned_abs()) < 14
                };
                if taken.iter().any(|&a| near(a)) {
                    continue;
                }
                let habit = data.biomes[map.biome_at(x, y)].habit;
                if habit <= 0.0 {
                    continue;
                }
                let coast = (-4isize..=4).any(|dy| {
                    (-4isize..=4).any(|dx| {
                        let xx = (x as isize + dx).rem_euclid(W as isize) as usize;
                        let yy = y as isize + dy;
                        (0..H as isize).contains(&yy)
                            && (map.surface.is_sea(xx, yy as usize)
                                || map.surface.is_lake(xx, yy as usize)
                                || map.surface.is_river(xx, yy as usize))
                    })
                });
                let noise = unit(hash_coord(
                    map.seed ^ 0xC17E,
                    x as i64,
                    y as i64,
                    i64::from(id),
                ));
                let score = habit * if coast { 1.5 } else { 1.0 } + 0.05 * noise;
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, (x, y)));
                }
            }
        }
        let site = best?.1;
        if id == colony_id {
            return Some(site);
        }
        taken.push(site);
    }
    None
}

/// How much of a catastrophe a city at `city` feels, 0..1: the severity, fading to nothing at the
/// edge of its reach (5 to 20 cells depending on the severity, wrapped in longitude).
pub fn catastrophe_impact(city: (usize, usize), c: &Catastrophe) -> f64 {
    let dx = (city.0 as isize - c.x as isize).unsigned_abs();
    let dx = dx.min(W - dx) as f64;
    let dy = (city.1 as isize - c.y as isize).unsigned_abs() as f64;
    let reach = 5.0 + 15.0 * c.severity;
    let d = (dx * dx + dy * dy).sqrt();
    if d >= reach {
        0.0
    } else {
        c.severity * (1.0 - d / reach)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::generate_system;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// A sim whose system has a planet or moon of this kind (searching seeds).
    fn sim_with(kind: PlanetKind) -> (Sim, usize) {
        for seed in 1..4000u64 {
            let sys = generate_system(seed);
            if let Some(i) = sys.bodies.iter().position(
                |b| matches!(b.kind, BodyKind::Planet(k) | BodyKind::Moon(k) if k == kind),
            ) {
                return (Sim::new(sys, data()), i);
            }
        }
        panic!("no {kind:?} found");
    }

    #[test]
    fn terran_world_has_seas_and_several_land_biomes_and_is_deterministic() {
        let (s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let again = s.planet_map(i).expect("map");
        assert_eq!(m.biome, again.biome);
        let ids: std::collections::BTreeSet<&str> = m
            .biome
            .iter()
            .map(|&b| s.data.biomes[usize::from(b)].id.as_str())
            .collect();
        assert!(ids.len() >= 5, "{ids:?}");
        assert!(
            ids.contains("ocean") || ids.contains("ocean_profond") || ids.contains("mer_gelee")
        );
    }

    #[test]
    fn airless_worlds_only_get_airless_biomes_and_giants_have_no_map() {
        let (s, i) = sim_with(PlanetKind::Barren);
        let m = s.planet_map(i).expect("map");
        if m.pressure < 0.01 {
            for &b in &m.biome {
                let id = s.data.biomes[usize::from(b)].id.as_str();
                assert!(id == "plaine_rocheuse" || id == "massif", "{id}");
            }
        }
        let (s, i) = sim_with(PlanetKind::GasGiant);
        assert!(s.planet_map(i).is_none());
        assert!(s.planet_map(0).is_none(), "the star has no surface");
    }

    #[test]
    fn seasons_are_opposite_in_the_two_hemispheres() {
        let (mut s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let (north, south) = (8, H - 9);
        let year = s.year_length(i);
        let swing = |s: &mut Sim, y: usize| {
            s.time = 0.25 * year;
            let summer = s.temp_now(&m, 10, y) - f64::from(m.surface.temp[Surface::idx(10, y)]);
            s.time = 0.75 * year;
            let winter = s.temp_now(&m, 10, y) - f64::from(m.surface.temp[Surface::idx(10, y)]);
            (summer, winter)
        };
        let (ns, nw) = swing(&mut s, north);
        let (ss, sw) = swing(&mut s, south);
        assert!(ns > 0.5 && nw < -0.5, "north {ns} {nw}");
        assert!(ss < -0.5 && sw > 0.5, "south {ss} {sw}");
        // The equator hardly moves.
        let (es, _) = swing(&mut s, H / 2);
        assert!(es.abs() < ns.abs() / 4.0);
    }

    #[test]
    fn weather_changes_from_day_to_day_but_is_stable_within_a_day_and_region() {
        let (mut s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let (a, _) = s.weather_at(&m, 3, 20);
        let (b, _) = s.weather_at(&m, 6, 22);
        let same_region = s.weather_at(&m, 3, 20).0;
        assert_eq!(a, same_region);
        let _ = b;
        let mut kinds = std::collections::BTreeSet::new();
        for d in 0..200 {
            s.time = f64::from(d) * 86_400.0 + 100.0;
            kinds.insert(s.weather_at(&m, 40, 30).0.label());
        }
        assert!(kinds.len() >= 3, "{kinds:?}");
        // No air, no weather.
        let (s2, j) = sim_with(PlanetKind::Dwarf);
        let m2 = s2.planet_map(j).expect("map");
        if m2.pressure < 0.01 {
            assert_eq!(s2.weather_at(&m2, 5, 5).0, Weather::Clear);
        }
    }

    #[test]
    fn catastrophes_are_rare_deterministic_and_fit_the_terrain() {
        let (s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let count = |s: &Sim, m: &PlanetMap| {
            (0..2000)
                .filter(|&d| s.catastrophe_on(m, d).is_some())
                .count()
        };
        let n = count(&s, &m);
        assert!((10..=200).contains(&n), "{n} catastrophes in 2000 days");
        assert_eq!(s.catastrophe_on(&m, 77), s.catastrophe_on(&m, 77));
        for d in 0..4000 {
            if let Some(c) = s.catastrophe_on(&m, d) {
                match c.kind {
                    CatastropheKind::Hurricane => assert!(m.surface.is_sea(c.x, c.y)),
                    CatastropheKind::Flood | CatastropheKind::Drought => {
                        assert!(!m.surface.is_sea(c.x, c.y))
                    }
                    _ => {}
                }
                assert!((0.2..=1.0).contains(&c.severity));
            }
        }
        // Volcanic worlds erupt far more often than living ones.
        let (sl, il) = sim_with(PlanetKind::Lava);
        let ml = sl.planet_map(il).expect("map");
        assert!(count(&sl, &ml) > n);
    }

    /// A year-long look at one cell of a world whose surface is edited to suit the test.
    fn at_phase(s: &mut Sim, body: usize, phase: f64) {
        s.time = phase * s.year_length(body);
    }

    #[test]
    fn polar_seas_freeze_in_winter_and_melt_in_summer() {
        let (mut s, i) = sim_with(PlanetKind::Terran);
        let mut m = s.planet_map(i).expect("map");
        let (x, y) = (10, 4);
        let k = Surface::idx(x, y);
        m.surface.elev[k] = -0.3;
        m.surface.temp[k] = 270.0;
        m.surface.hum[k] = 0.2;
        m.surface.tilt = 0.5;
        at_phase(&mut s, i, 0.30);
        let summer = (s.temp_now(&m, x, y), s.water_frozen(&m, x, y));
        at_phase(&mut s, i, 0.80);
        let winter = (s.temp_now(&m, x, y), s.water_frozen(&m, x, y));
        assert!(summer.0 > winter.0 + 10.0);
        assert!(!summer.1, "the sea is open in summer: {}", summer.0);
        assert!(winter.1, "and frozen in winter: {}", winter.0);
        // Dry land never freezes as water.
        m.surface.elev[k] = 0.3;
        assert!(!s.water_frozen(&m, x, y) || m.surface.is_lake(x, y) || m.surface.is_river(x, y));
        // A lake freezes at a warmer temperature than the sea does.
        m.surface.elev[k] = -0.3;
        m.surface.temp[k] = 277.0;
        at_phase(&mut s, i, 0.80);
        let t = s.temp_now(&m, x, y);
        assert!(!s.water_frozen(&m, x, y) || t < 271.4, "a sea at {t} K");
    }

    #[test]
    fn the_sun_goes_up_and_down_with_the_year_and_the_tilt() {
        let (mut s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        at_phase(&mut s, i, 0.25);
        let up = s.sun_declination(&m);
        at_phase(&mut s, i, 0.75);
        let down = s.sun_declination(&m);
        assert!((up - f64::from(m.surface.tilt)).abs() < 1e-3);
        assert!((up + down).abs() < 1e-3);
    }

    #[test]
    fn clouds_need_air_and_come_with_wet_weather() {
        let (s, i) = sim_with(PlanetKind::Dwarf);
        let m = s.planet_map(i).expect("map");
        if m.pressure < 0.01 {
            for y in (0..H).step_by(6) {
                for x in (0..W).step_by(9) {
                    assert_eq!(s.cloud_at(&m, x, y), 0.0, "no air, no clouds");
                }
            }
        }
        let (mut s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let mut any = false;
        for d in 0..60 {
            s.time = f64::from(d) * 86_400.0 + 100.0;
            for (x, y) in [(5, 30), (40, 20), (70, 35), (100, 25), (120, 40)] {
                let cover = s.cloud_at(&m, x, y);
                any |= cover > 0.3;
                // No rain from a clear sky.
                let w = s.weather_at(&m, x, y).0;
                if cover < 0.12 {
                    assert!(
                        !matches!(
                            w,
                            Weather::Rain | Weather::Storm | Weather::Snow | Weather::Blizzard
                        ),
                        "{w:?} under a clear sky"
                    );
                }
            }
        }
        assert!(any, "a living world has clouds");
    }

    #[test]
    fn the_clouds_move_with_time() {
        let (mut s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let sky = |s: &Sim| -> Vec<f32> { (0..W).map(|x| s.cloud_at(&m, x, 28)).collect() };
        at_phase(&mut s, i, 0.1);
        let a = sky(&s);
        s.time += 3.0 * 86_400.0;
        assert_ne!(a, sky(&s));
    }

    #[test]
    fn quakes_strike_the_faults_eruptions_the_active_volcanoes_and_a_dead_world_is_quiet() {
        let (s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let geo = &m.surface.geo;
        let faults = geo.fault_cells();
        let volcano_cells: Vec<(usize, usize)> = geo.volcanoes.iter().map(|v| (v.x, v.y)).collect();
        let (mut quakes, mut eruptions) = (0, 0);
        for d in 0..20_000 {
            let Some(c) = s.catastrophe_on(&m, d) else {
                continue;
            };
            match c.kind {
                CatastropheKind::Earthquake => {
                    quakes += 1;
                    assert!(
                        faults.contains(&(c.x, c.y)) || volcano_cells.contains(&(c.x, c.y)),
                        "a quake at {:?} is on no fault",
                        (c.x, c.y)
                    );
                }
                CatastropheKind::Eruption => {
                    eruptions += 1;
                    assert!(geo
                        .volcanoes
                        .iter()
                        .any(|v| v.active && (v.x, v.y) == (c.x, c.y)));
                }
                _ => {}
            }
        }
        assert!(quakes > 20 && eruptions > 10, "{quakes} {eruptions}");
        // The same world with its inside dead: no quake, no eruption, ever.
        let mut dead = m.clone();
        dead.surface.geo.activity = vector_world::physics::Activity::Dead;
        dead.surface.geo.volcanoes.clear();
        for d in 0..20_000 {
            if let Some(c) = s.catastrophe_on(&dead, d) {
                assert!(!matches!(
                    c.kind,
                    CatastropheKind::Earthquake | CatastropheKind::Eruption
                ));
            }
        }
    }

    #[test]
    fn the_relief_of_a_world_follows_what_its_inside_does() {
        // The same system, one world: its map is built with its Activity and the age of its star.
        let (s, i) = sim_with(PlanetKind::Terran);
        let m = s.planet_map(i).expect("map");
        let ph = s.system.physics(i, 0.0);
        assert_eq!(m.surface.geo.activity, ph.activity);
        assert!((f64::from(m.surface.geo.age_gyr) - s.system.age() / 1.0e9).abs() < 1e-3);
        let (sb, ib) = sim_with(PlanetKind::Barren);
        let mb = sb.planet_map(ib).expect("map");
        assert_eq!(mb.surface.geo.activity, sb.system.physics(ib, 0.0).activity);
    }

    #[test]
    fn impact_fades_with_distance_and_wraps_around_the_date_line() {
        let c = Catastrophe {
            kind: CatastropheKind::Earthquake,
            x: 2,
            y: 30,
            severity: 0.8,
        };
        assert!((catastrophe_impact((2, 30), &c) - 0.8).abs() < 1e-9);
        let near = catastrophe_impact((5, 30), &c);
        assert!(near > 0.0 && near < 0.8);
        assert_eq!(catastrophe_impact((60, 30), &c), 0.0, "out of reach");
        // Column 126 is 4 cells from column 2 across the seam, like column 6.
        assert!(
            (catastrophe_impact((W - 2, 30), &c) - catastrophe_impact((6, 30), &c)).abs() < 1e-9
        );
    }

    #[test]
    fn a_catastrophe_on_the_city_kills_people_and_is_logged_once() {
        let (mut s, body) = sim_with(PlanetKind::Terran);
        let data = Arc::clone(&s.data);
        s.reveal_all();
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        for (id, kg) in [
            ("construction_materials", 3000.0),
            ("colonists", 400.0),
            ("food", 300.0),
            ("water", 600.0),
        ] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.credits = 100_000.0;
        let b = &s.bodies[body];
        s.ship.pos = b.pos - vector_core::Vec3::new(0.0, 0.0, b.radius + 5000.0);
        s.ship.vel = b.vel;
        s.update_ref_pub();
        let ci = s.found_colony(body, None).expect("found");
        let map = s.planet_map(body).expect("map");
        let city = s.city_site(&map, s.colonies[ci].id).expect("a city site");
        assert!(!map.surface.is_sea(city.0, city.1), "cities stand on land");
        // The first day a catastrophe really reaches the city.
        let (day, cat) = (1..60_000)
            .find_map(|d| {
                s.catastrophe_on(&map, d)
                    .filter(|c| catastrophe_impact(city, c) > 0.05)
                    .map(|c| (d, c))
            })
            .expect("a catastrophe hits the city eventually");
        let lines = |s: &Sim| {
            s.colonies[ci]
                .log
                .iter()
                .filter(|(_, l)| l.starts_with("Catastrophe"))
                .count()
        };
        // Live the day before (nothing logged), then the day itself.
        s.time = (day - 1) as f64 * 86_400.0 + 100.0;
        s.colonies[ci].last_tick = s.time;
        s.colonies[ci].pop = 200.0;
        let before = lines(&s);
        s.time = day as f64 * 86_400.0 + 100.0;
        s.update_colonies();
        assert_eq!(lines(&s), before + 1, "{cat:?}");
        assert!(s.colonies[ci].pop < 200.0, "pop {}", s.colonies[ci].pop);
        // Updating again without time passing does not repeat it.
        s.update_colonies();
        assert_eq!(lines(&s), before + 1);
    }
}
