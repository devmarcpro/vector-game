//! Surface screen (F8): a coloured character map of a solid planet with five layers, an info
//! panel for the cursor cell, and a wireframe globe with contour lines.
use crate::game::Game;
use crate::globe::Globe;
use crate::hub::wrap;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_core::{Mat3, Vec3};
use vector_render::Framebuffer;
use vector_sim::planet::{PlanetMap, Weather};
use vector_world::surface::{Surface, H, W};

pub const LAYERS: [&str; 9] = [
    "BIOMES",
    "ALTITUDE",
    "TEMPERATURE",
    "HUMIDITE",
    "METEO",
    "VIE",
    "NUAGES",
    "BIOSPHERE",
    "GEOLOGIE",
];

/// Water colours on the maps: lakes and rivers, and the ice that covers them (and the seas) when
/// it is cold enough now.
const LAKE: u32 = 0x0030_70C0;
const ICE: u32 = 0x00D0_E8F8;

fn mix_rgb(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| (((a >> s) & 0xFF) as f32 * (1.0 - t) + ((b >> s) & 0xFF) as f32 * t) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// What a cell looks like from far away: the colour of its biome, with the lakes, the big rivers
/// and the ice of the season on top. One place decides it, for the globe of the cockpit and the
/// map of the surface alike.
pub fn ground_color(sim: &vector_sim::Sim, map: &PlanetMap, x: usize, y: usize) -> u32 {
    let base = sim.data.biomes[map.biome_at(x, y)].color;
    let s = &map.surface;
    if s.is_lake(x, y) {
        return if sim.water_frozen(map, x, y) {
            ICE
        } else {
            LAKE
        };
    }
    if s.is_sea(x, y) {
        return if sim.water_frozen(map, x, y) {
            ICE
        } else {
            base
        };
    }
    if s.is_river(x, y) {
        let rel = s.hydro.rel(Surface::idx(x, y));
        if rel > 0.3 {
            let water = if sim.water_frozen(map, x, y) {
                ICE
            } else {
                LAKE
            };
            return mix_rgb(base, water, 0.35 + 0.4 * rel);
        }
    }
    base
}

/// Index of the BIOSPHERE layer: density of one species on the map, its food web, its variants.
pub const BIOSPHERE: usize = 7;

/// Index of the GEOLOGIE layer: plates, boundaries, volcanoes, rock and ore bodies.
pub const GEOLOGIE: usize = 8;

/// Metres per unit of relative elevation (peaks and trenches of a living world).
const RELIEF_M: f64 = 8000.0;

fn rgb(r: f64, g: f64, b: f64) -> u32 {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}

fn lerp3(stops: &[(f64, [f64; 3])], v: f64) -> u32 {
    let v = v.clamp(stops[0].0, stops[stops.len() - 1].0);
    for w in stops.windows(2) {
        if v <= w[1].0 {
            let t = (v - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
            let (a, b) = (w[0].1, w[1].1);
            return rgb(
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            );
        }
    }
    0
}

pub fn elev_color(e: f64) -> u32 {
    if e < 0.0 {
        lerp3(&[(-1.0, [0.02, 0.06, 0.35]), (0.0, [0.10, 0.45, 0.90])], e)
    } else {
        lerp3(
            &[
                (0.0, [0.20, 0.70, 0.25]),
                (0.3, [0.80, 0.75, 0.25]),
                (0.6, [0.60, 0.40, 0.20]),
                (1.0, [1.0, 1.0, 1.0]),
            ],
            e,
        )
    }
}

fn temp_color(t: f64) -> u32 {
    lerp3(
        &[
            (180.0, [0.30, 0.30, 1.0]),
            (250.0, [0.20, 0.80, 1.0]),
            (285.0, [0.30, 0.90, 0.30]),
            (320.0, [1.0, 0.85, 0.20]),
            (420.0, [1.0, 0.20, 0.10]),
        ],
        t,
    )
}

fn hum_color(h: f64) -> u32 {
    lerp3(
        &[
            (0.0, [0.75, 0.50, 0.20]),
            (0.5, [0.30, 0.80, 0.40]),
            (1.0, [0.20, 0.60, 1.0]),
        ],
        h,
    )
}

pub fn weather_glyph(w: Weather) -> (char, u32) {
    match w {
        Weather::Clear => ('.', 0x0040_5060),
        Weather::Cloudy => ('c', 0x00A0_A8B0),
        Weather::Rain => ('/', 0x0040_90FF),
        Weather::Storm => ('Z', 0x00FF_E040),
        Weather::Snow => ('*', 0x00E0_F0FF),
        Weather::Blizzard => ('X', 0x00FF_FFFF),
        Weather::Heat => ('H', 0x00FF_6020),
        Weather::Dust => ('d', 0x00D0_A050),
    }
}

/// Glyph and colour of a cell in the current layer.
fn cell(g: &Game, map: &PlanetMap, x: usize, y: usize) -> (char, u32) {
    let i = Surface::idx(x, y);
    let s = &map.surface;
    match g.surf_layer {
        0 => {
            let b = &g.data.biomes[map.biome_at(x, y)];
            let color = ground_color(&g.sim, map, x, y);
            let ch = if s.is_water(x, y) && g.sim.water_frozen(map, x, y) {
                '='
            } else if s.is_lake(x, y) || s.is_river(x, y) {
                '~'
            } else {
                b.glyph.chars().next().unwrap_or('?')
            };
            (ch, color)
        }
        1 => {
            let e = f64::from(s.elev[i]);
            let ch = match e {
                e if e < -0.5 => '~',
                e if e < 0.0 => '-',
                e if e < 0.2 => '.',
                e if e < 0.45 => ':',
                e if e < 0.7 => '+',
                _ => '^',
            };
            (ch, elev_color(e))
        }
        2 => ('#', temp_color(g.sim.temp_now(map, x, y))),
        3 => ('#', hum_color(f64::from(s.hum[i]))),
        4 => weather_glyph(g.sim.weather_at(map, x, y).0),
        5 => ('#', life_color(g, map.biome_at(x, y))),
        6 => {
            let c = f64::from(g.sim.cloud_at(map, x, y));
            (
                '#',
                lerp3(&[(0.0, [0.05, 0.08, 0.18]), (1.0, [0.95, 0.96, 1.0])], c),
            )
        }
        7 => species_cell(g, map.biome_at(x, y)),
        _ => geology_cell(map, x, y),
    }
}

/// Species alive somewhere on the world, the most abundant first (what V cycles through).
pub fn alive_species(eco: &vector_sim::ecology::Ecosystem) -> Vec<usize> {
    let mut v: Vec<(usize, f64)> = (0..eco.ns)
        .map(|s| (s, eco.total(s)))
        .filter(|&(_, m)| m > 0.0)
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v.into_iter().map(|(s, _)| s).collect()
}

/// Colour of a trophic level: producers green, grazers yellow, omnivores orange, hunters red.
fn trophic_color(t: &str) -> [f64; 3] {
    match t {
        "producer" => [0.30, 0.90, 0.35],
        "herbivore" => [0.95, 0.85, 0.25],
        "omnivore" => [1.0, 0.60, 0.20],
        _ => [1.0, 0.25, 0.20],
    }
}

/// The species the BIOSPHERE layer shows.
fn shown_species(g: &Game) -> Option<usize> {
    let list = alive_species(g.surf_eco.as_ref()?);
    list.get(g.surf_species.min(list.len().saturating_sub(1)))
        .copied()
}

/// Density of the shown species in the biome of a cell, on a log scale, in the colour of its
/// trophic level (grey where it does not live).
fn species_cell(g: &Game, biome: usize) -> (char, u32) {
    let (Some(eco), Some(s)) = (g.surf_eco.as_ref(), shown_species(g)) else {
        return ('.', 0x0020_2428);
    };
    let dens = if eco.area[biome] > 0.0 {
        eco.at(s, biome) / eco.area[biome]
    } else {
        0.0
    };
    if dens <= 0.0 {
        return ('.', 0x0020_2428);
    }
    let v = ((1.0 + dens).log10() / 6.0).clamp(0.15, 1.0);
    let c = trophic_color(&g.data.species[s].trophic);
    let ch = if v < 0.35 {
        ':'
    } else if v < 0.65 {
        '+'
    } else {
        '#'
    };
    (ch, rgb(c[0] * v, c[1] * v, c[2] * v))
}

/// Volcano, ore body, plate boundary or rock of a cell, in that order of importance; the tint of
/// the ground is that of its plate.
fn geology_cell(map: &PlanetMap, x: usize, y: usize) -> (char, u32) {
    use vector_world::geology::{Boundary, DepositKind, Rock};
    let geo = &map.surface.geo;
    if let Some(v) = geo.volcanoes.iter().find(|v| v.x == x && v.y == y) {
        return if v.active {
            ('A', 0x00FF_5020)
        } else {
            ('a', 0x0090_5030)
        };
    }
    if let Some(d) = geo.deposits.iter().find(|d| d.x == x && d.y == y) {
        let ch = match d.kind {
            DepositKind::MetalVein => 'M',
            DepositKind::Hydrothermal => 'H',
            DepositKind::Kimberlite => 'K',
            DepositKind::Evaporite => 'E',
            DepositKind::Petroleum => 'P',
            DepositKind::Pegmatite => 'G',
            DepositKind::IceField => 'I',
        };
        return (ch, 0x00FF_E060);
    }
    match geo.boundary_at(x, y) {
        Some(Boundary::Convergent) => return ('^', 0x00FF_6060),
        Some(Boundary::Divergent) => return ('=', 0x0060_E0FF),
        Some(Boundary::Transform) => return ('/', 0x00E0_E060),
        None => {}
    }
    let ch = match geo.rock_at(x, y) {
        Rock::Igneous => '.',
        Rock::Sedimentary => ':',
        Rock::Metamorphic => '%',
    };
    // A dim colour per plate, lighter on land than under the sea.
    let p = geo.plate_at(x, y) as u32;
    let base = [
        0x0040_6080u32,
        0x0060_8040,
        0x0080_5060,
        0x0070_7040,
        0x0040_7070,
        0x0070_5090,
    ];
    let c = base[(p as usize) % base.len()];
    let sea = map.surface.is_sea(x, y);
    let k = if sea { 0.55 } else { 1.0 };
    let ch2 = |s: u32| (((c >> s) & 0xFF) as f32 * k) as u32;
    (ch, (ch2(16) << 16) | (ch2(8) << 8) | ch2(0))
}

/// Density of living biomass of a biome (kg per km2), on a log scale from dead rock to jungle.
fn life_color(g: &Game, biome: usize) -> u32 {
    let density = g.surf_eco.as_ref().map_or(0.0, |e| {
        if e.area[biome] <= 0.0 {
            0.0
        } else {
            (0..e.ns).map(|s| e.at(s, biome)).sum::<f64>() / e.area[biome]
        }
    });
    let v = (1.0 + density).log10() / 6.5;
    lerp3(
        &[
            (0.0, [0.12, 0.12, 0.14]),
            (0.4, [0.55, 0.45, 0.20]),
            (0.7, [0.30, 0.75, 0.30]),
            (1.0, [0.60, 1.0, 0.40]),
        ],
        v,
    )
}

/// A mass in kg as a short string (kg, t, kt, Mt, Gt).
pub fn fmt_mass(kg: f64) -> String {
    if kg < 1.0e3 {
        format!("{kg:.0} kg")
    } else if kg < 1.0e6 {
        format!("{:.0} t", kg / 1.0e3)
    } else if kg < 1.0e9 {
        format!("{:.0} kt", kg / 1.0e6)
    } else if kg < 1.0e12 {
        format!("{:.0} Mt", kg / 1.0e9)
    } else {
        format!("{:.0} Gt", kg / 1.0e12)
    }
}

fn fill_cell(fb: &mut Framebuffer, col: usize, row: usize, c: u32) {
    for dy in 0..10 {
        for dx in 0..8 {
            fb.px_set((col * 8 + dx) as i32, (row * 10 + dy) as i32, c);
        }
    }
}

fn season_name(map_phase: f64, south: bool) -> &'static str {
    let q = ((map_phase * 4.0).floor() as usize + if south { 2 } else { 0 }) % 4;
    ["printemps", "été", "automne", "hiver"][q]
}

fn lat_lon(x: usize, y: usize) -> String {
    let lat = Surface::lat(y).to_degrees();
    let lon = Surface::lon(x).to_degrees();
    format!(
        "LAT {:.0} {}  LON {:.0} {}",
        lat.abs(),
        if lat >= 0.0 { "N" } else { "S" },
        if lon > 180.0 { 360.0 - lon } else { lon },
        if lon > 180.0 { "O" } else { "E" }
    )
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let Some(map) = g.surf.as_ref() else {
        fb.text(2, 2, "PAS DE CARTE", RED);
        return;
    };
    let body = &g.sim.system.bodies[map.body];
    fb.text(2, 1, &format!("== SURFACE : {} ==", body.name), AMBER);
    if let Some(c) = &g.surf_civ {
        fb.text(
            50,
            1,
            &format!(
                "CIVILISATION : {} ({}) - TECHNOLOGIE {}/4 - {:.0} HABITANTS - KARDASHEV {:.2}  [ENTREE EN VOL : PARLER]",
                c.name.to_uppercase(),
                g.data.civilisations[c.kind].name.to_uppercase(),
                c.tech,
                c.population,
                c.kardashev()
            ),
            GREEN,
        );
    }
    // Layer tabs.
    let mut x = 2;
    for (i, l) in LAYERS.iter().enumerate() {
        let hover = g.hit_cells(
            x,
            2,
            l.len(),
            1,
            Act::Call(Game::click_surface_layer, i),
            None,
        );
        fb.text(
            x,
            2,
            l,
            if i == g.surf_layer || hover {
                AMBER
            } else {
                DIM
            },
        );
        x += l.len() + 2;
    }
    let panel_w = 38;
    let (x0, y0) = (1usize, 4usize);
    let cols_map = fb.cols().saturating_sub(panel_w + 3).clamp(20, W);
    let rows_map = fb.rows().saturating_sub(y0 + 3).clamp(10, H);
    g.surf_geom.set((x0, y0, cols_map, rows_map));
    if g.surf_globe {
        draw_globe(fb, g, map, x0, y0, cols_map, rows_map);
    } else {
        g.hit_cells(
            x0,
            y0,
            cols_map,
            rows_map,
            Act::At(Game::click_surface_map),
            None,
        );
        for r in 0..rows_map {
            let my = r * H / rows_map;
            for c in 0..cols_map {
                let mx = c * W / cols_map;
                let (ch, col) = cell(g, map, mx, my);
                fb.text(x0 + c, y0 + r, &ch.to_string(), col);
            }
        }
        // Cursor: the cell under it gets a bright background.
        let (cc, cr) = (
            (g.surf_cx * cols_map / W).min(cols_map - 1),
            (g.surf_cy * rows_map / H).min(rows_map - 1),
        );
        let (ch, col) = cell(g, map, g.surf_cx, g.surf_cy);
        fill_cell(fb, x0 + cc, y0 + cr, 0x0060_4010);
        fb.text(
            x0 + cc,
            y0 + cr,
            &ch.to_string(),
            if col == 0 { AMBER } else { 0x00FF_FFFF },
        );
        // Cities of the colonies on this world.
        for (name, cx, cy) in &g.surf_cities {
            let (c, r) = (
                (*cx * cols_map / W).min(cols_map - 1),
                (*cy * rows_map / H).min(rows_map - 1),
            );
            fb.text(x0 + c, y0 + r, "@", 0x00FF_FFFF);
            fb.text(x0 + c + 2, y0 + r, &name.to_uppercase(), AMBER);
        }
    }
    if g.surf_layer == BIOSPHERE {
        draw_biosphere_panel(fb, g, map, x0 + cols_map + 2, y0);
    } else {
        draw_panel(fb, g, map, x0 + cols_map + 2, y0);
    }
}

/// The BIOSPHERE panel: the life of the world (mass, air, chemistries), the shown species with
/// its biology, its place in the web at the cursor and its local variant, and the populations
/// that have diverged the most. Enter opens the full card of the variant.
fn draw_biosphere_panel(fb: &mut Framebuffer, g: &Game, map: &PlanetMap, col: usize, row: usize) {
    use vector_sim::biosphere::{behavior_of, life_of, web_of, world_variants};
    let body = &g.sim.system.bodies[map.body];
    let r = std::cell::Cell::new(row);
    let line = |fb: &mut Framebuffer, s: String, c: u32| {
        let room = fb.cols().saturating_sub(col);
        fb.text(col, r.get(), &s.chars().take(room).collect::<String>(), c);
        r.set(r.get() + 1);
    };
    line(
        fb,
        format!("BIOSPHERE : {}", body.name.to_uppercase()),
        AMBER,
    );
    let Some(eco) = g.surf_eco.as_ref().filter(|e| e.life) else {
        line(fb, "MONDE STERILE : AUCUNE VIE".into(), DIM);
        return;
    };
    let (cx, cy) = (g.surf_cx, g.surf_cy);
    let here = map.biome_at(cx, cy);
    let list = alive_species(eco);
    // The population of the shown species at the cursor, and every population of the world
    // (recomputed once a day: the drift of the traits is slow).
    let variants = {
        let mut c = g.surf_variants.borrow_mut();
        if c.0 != g.sim.day_number() || c.1.is_empty() {
            *c = (
                g.sim.day_number(),
                world_variants(&g.data, eco, map, g.sim.time),
            );
        }
        c.1.clone()
    };
    if let (true, Some(s)) = (g.surf_card, shown_species(g)) {
        let v = variants
            .iter()
            .find(|v| v.species == s && v.biome == here)
            .or_else(|| variants.iter().find(|v| v.species == s));
        line(fb, "FICHE (ENTREE : RETOUR)".into(), AMBER);
        if let Some(v) = v {
            for l in vector_card(g, v, &body.name) {
                for w in wrap(&l, 38) {
                    line(fb, w.to_uppercase(), GREEN);
                }
            }
        }
        return;
    }
    line(
        fb,
        format!(
            "BIOMASSE {}   {} ESPECES",
            fmt_mass(eco.living()),
            list.len()
        ),
        GREEN,
    );
    // The air as life has made it.
    let now = g.sim.atmosphere_of(map.body);
    let pct = |gas: &str, v: &[(String, f32)]| {
        v.iter()
            .find(|(n, _)| n == gas)
            .map_or(0.0, |(_, f)| f64::from(*f) * 100.0)
    };
    let base: Vec<(String, f32)> = body
        .atmosphere
        .iter()
        .map(|(n, f)| ((*n).to_string(), *f))
        .collect();
    line(
        fb,
        format!(
            "AIR : O2 {:.1} %  CO2 {:.1} %",
            pct("O2", &now),
            pct("CO2", &now)
        ),
        GREEN,
    );
    line(
        fb,
        format!(
            "PHOTOSYNTHESE : +{:.1} % D'O2",
            pct("O2", &now) - pct("O2", &base)
        ),
        GREEN,
    );
    for (ci, c) in g.data.chemistries.iter().enumerate() {
        let n = list
            .iter()
            .filter(|&&s| g.data.chemistry_of(s) == Some(ci))
            .count();
        if n > 0 {
            line(
                fb,
                format!("XENOBIOLOGIE : {} ({n})", c.name.to_uppercase()),
                AMBER,
            );
        }
    }
    line(fb, String::new(), DIM);
    let Some(s) = shown_species(g) else {
        return;
    };
    let sp = &g.data.species[s];
    let tc = trophic_color(&sp.trophic);
    line(
        fb,
        format!("[V] {}", sp.name.to_uppercase()),
        rgb(tc[0], tc[1], tc[2]),
    );
    let m_here = eco.at(s, here);
    line(
        fb,
        format!(
            "MONDE {}  ICI {}",
            fmt_mass(eco.total(s)),
            if m_here > 0.0 {
                format!(
                    "{} ({:.0}/KM2)",
                    fmt_mass(m_here),
                    m_here / eco.area[here].max(1.0)
                )
            } else {
                "ABSENTE".to_string()
            }
        ),
        GREEN,
    );
    let (l, b) = (life_of(sp), behavior_of(&g.data, sp));
    line(
        fb,
        format!(
            "METAB {:.1} W  VIE {:.1} ANS  PORTEE {}  {}",
            l.watts,
            l.lifespan_days / 365.25,
            l.litter,
            if l.sexual { "SEXUEE" } else { "CLONALE" }
        ),
        GREEN,
    );
    let mut ways: Vec<String> = Vec::new();
    if b.herd > 1 {
        ways.push(format!(
            "{} DE {}",
            if b.pack { "MEUTE" } else { "TROUPEAU" },
            b.herd
        ));
    }
    if b.territory_km2 > 0.0 {
        ways.push(format!("TERRITOIRE {:.1} KM2", b.territory_km2));
    }
    if b.nocturnal {
        ways.push("NOCTURNE".into());
    }
    if b.hibernates {
        ways.push("HIBERNE".into());
    }
    if !ways.is_empty() {
        line(fb, ways.join("  "), GREEN);
    }
    if let Some(v) = variants.iter().find(|v| v.species == s && v.biome == here) {
        line(
            fb,
            format!(
                "{}  DIVERGENCE {:.0} %",
                if v.speciated() {
                    "VARIANTE"
                } else {
                    "POPULATION"
                },
                v.divergence * 100.0
            ),
            if v.speciated() { AMBER } else { GREEN },
        );
        line(
            fb,
            format!(
                "MASSE x{:.2}  PELAGE {:+.0}  RAYONS {:.0} %",
                v.mass_factor(),
                v.tint * 100.0,
                v.resist * 100.0
            ),
            GREEN,
        );
    }
    let (eats, eaten) = web_of(&g.data, eco, here, s);
    for (title, set) in [("SE NOURRIT DE", &eats), ("EST MANGE PAR", &eaten)] {
        if set.is_empty() {
            continue;
        }
        line(fb, format!("{title} :"), DIM);
        for &(o, m) in set.iter().take(3) {
            line(
                fb,
                format!(
                    "  {:<24}{:>9}",
                    g.data.species[o].name.to_uppercase(),
                    fmt_mass(m)
                ),
                GREEN,
            );
        }
    }
    line(fb, String::new(), DIM);
    // Species at the cursor: a click shows one on the map.
    line(fb, "ICI (CLIC : AFFICHER)".into(), AMBER);
    for (o, m) in eco.present_in(here).into_iter().take(7) {
        let pos = list.iter().position(|&x| x == o).unwrap_or(0);
        let c = trophic_color(&g.data.species[o].trophic);
        g.hit_cells(
            col,
            r.get(),
            36,
            1,
            Act::Call(Game::click_surface_species, pos),
            None,
        );
        line(
            fb,
            format!(
                "{} {:<24}{:>9}",
                if o == s { ">" } else { " " },
                g.data.species[o].name.to_uppercase(),
                fmt_mass(m)
            ),
            rgb(c[0], c[1], c[2]),
        );
    }
    line(fb, String::new(), DIM);
    let diverged: Vec<_> = variants.iter().filter(|v| v.speciated()).collect();
    line(
        fb,
        format!("VARIANTES DISTINCTES : {}", diverged.len()),
        AMBER,
    );
    for v in diverged.iter().take(4) {
        line(fb, format!("  {}", v.name(&g.data).to_uppercase()), GREEN);
    }
}

/// The card of a variant, from the codex.
fn vector_card(g: &Game, v: &vector_sim::biosphere::Variant, world: &str) -> Vec<String> {
    crate::codex::variant_card(&g.data, v, world)
}

fn draw_panel(fb: &mut Framebuffer, g: &Game, map: &PlanetMap, col: usize, row: usize) {
    let body = &g.sim.system.bodies[map.body];
    let sim = &g.sim;
    let mut r = row;
    let mut line = |fb: &mut Framebuffer, s: String, c: u32| {
        fb.text(col, r, &s, c);
        r += 1;
    };
    line(fb, crate::hud::kind_label(body.kind), AMBER);
    line(
        fb,
        format!(
            "GRAVITE {:.1} M/S2  PRESSION {:.2} BAR",
            body.gravity, body.pressure
        ),
        GREEN,
    );
    line(
        fb,
        format!(
            "T EQUILIBRE {:.0} K  ({:.0} C)",
            body.temp,
            body.temp - 273.15
        ),
        GREEN,
    );
    let year_days = sim.year_length(map.body) / 86_400.0;
    line(
        fb,
        format!(
            "ANNEE {year_days:.0} J  INCLINAISON {:.0} DEG",
            f64::from(map.surface.tilt).to_degrees()
        ),
        GREEN,
    );
    line(
        fb,
        format!("MERS {:.0} % DE LA SURFACE", map.surface.sea_frac * 100.0),
        GREEN,
    );
    let ph = sim.system.physics(map.body, f64::from(map.surface.tilt));
    let day = match (ph.locked, ph.solar_day) {
        (Some((1, 1)), _) | (_, None) => "VERROUILLEE : UNE FACE VERS L'ASTRE".to_string(),
        (_, Some(d)) => format!("JOUR {:.1} H", d / 3600.0),
    };
    for (text, c) in [
        (day, GREEN),
        (format!("DENSITE {:.0} KG/M3", ph.density), GREEN),
        (format!("LIBERATION {:.1} KM/S", ph.escape / 1000.0), GREEN),
        (
            format!(
                "EQUATEUR {:.0} M/S  APLAT. {:.2} %",
                ph.equator_speed,
                ph.flattening * 100.0
            ),
            GREEN,
        ),
        (format!("INERTIE {:.2} MR2", ph.moment_factor), GREEN),
        (
            format!("MOMENT CINETIQUE {:.1E}", ph.angular_momentum),
            GREEN,
        ),
        (format!("INTERIEUR : {}", ph.activity.label()), GREEN),
        (
            format!(
                "NOYAU {:.0} %  FLUX {:.0} MW/M2",
                ph.core_fraction * 100.0,
                ph.heat_flux * 1000.0
            ),
            GREEN,
        ),
        (format!("MAGNETISME {:.2} (TERRE=1)", ph.magnetic), GREEN),
        (format!("MAREE {:.2} M", ph.tide), GREEN),
    ] {
        line(fb, text, c);
    }
    line(fb, String::new(), DIM);
    let (cx, cy) = (g.surf_cx, g.surf_cy);
    let i = Surface::idx(cx, cy);
    let b = &g.data.biomes[map.biome_at(cx, cy)];
    line(fb, lat_lon(cx, cy), AMBER);
    let season = season_name(sim.season_phase(map.body), cy >= H / 2);
    line(
        fb,
        format!("SAISON LOCALE : {}", season.to_uppercase()),
        GREEN,
    );
    line(
        fb,
        format!("{}  [{}]", b.name.to_uppercase(), b.glyph),
        b.color,
    );
    let e = f64::from(map.surface.elev[i]);
    line(
        fb,
        if e < 0.0 {
            format!("PROFONDEUR {:.0} M", -e * RELIEF_M)
        } else {
            format!("ALTITUDE {:.0} M", e * RELIEF_M)
        },
        GREEN,
    );
    {
        use vector_world::geology::Boundary;
        let geo = &map.surface.geo;
        let plate = &geo.plates[geo.plate_at(cx, cy)];
        line(
            fb,
            format!(
                "PLAQUE {} ({})",
                geo.plate_at(cx, cy) + 1,
                if plate.oceanic {
                    "OCEANIQUE"
                } else {
                    "CONTINENTALE"
                }
            ),
            GREEN,
        );
        line(
            fb,
            format!(
                "ROCHE {}",
                match geo.rock_at(cx, cy) {
                    vector_world::geology::Rock::Igneous => "IGNEE",
                    vector_world::geology::Rock::Sedimentary => "SEDIMENTAIRE",
                    vector_world::geology::Rock::Metamorphic => "METAMORPHIQUE",
                }
            ),
            GREEN,
        );
        if let Some(b) = geo.boundary_at(cx, cy) {
            line(
                fb,
                format!(
                    "FRONTIERE {}",
                    match b {
                        Boundary::Convergent => "CONVERGENTE (MONTAGNES, FOSSES, VOLCANS)",
                        Boundary::Divergent => "DIVERGENTE (DORSALE, RIFT)",
                        Boundary::Transform => "TRANSFORMANTE (FAILLE, SEISMES)",
                    }
                ),
                AMBER,
            );
        }
        line(
            fb,
            format!("CHALEUR DU SOL {:.0} %", geo.heat_at(cx, cy) * 100.0),
            GREEN,
        );
        for d in geo.deposits_near((cx, cy), 6.0).iter().take(2) {
            let name = g
                .data
                .geo_def(d.kind.id())
                .map_or(d.kind.id(), |def| def.name.as_str())
                .to_uppercase();
            line(
                fb,
                format!("GISEMENT : {name} ({:.0} %)", d.richness * 100.0),
                0x00FF_E060,
            );
        }
    }
    let (tm, tn) = (f64::from(map.surface.temp[i]), sim.temp_now(map, cx, cy));
    line(
        fb,
        format!("T MOYENNE {:.0} K  ({:.0} C)", tm, tm - 273.15),
        GREEN,
    );
    line(
        fb,
        format!("T ACTUELLE {:.0} K  ({:.0} C)", tn, tn - 273.15),
        GREEN,
    );
    line(
        fb,
        format!("HUMIDITE {:.0} %", f64::from(map.surface.hum[i]) * 100.0),
        GREEN,
    );
    let sky = f64::from(sim.cloud_at(map, cx, cy));
    line(
        fb,
        if map.pressure < 0.01 {
            "CIEL : PAS D'AIR, PAS DE NUAGES".to_string()
        } else {
            format!("NUAGES {:.0} %", sky * 100.0)
        },
        GREEN,
    );
    let s = &map.surface;
    if s.is_water(cx, cy) {
        let kind = if s.is_sea(cx, cy) {
            "MER"
        } else if s.is_lake(cx, cy) {
            "LAC"
        } else {
            "FLEUVE"
        };
        let frozen = sim.water_frozen(map, cx, cy);
        let flow = if s.is_river(cx, cy) {
            format!("  DEBIT {:.0} %", s.hydro.rel(i) * 100.0)
        } else {
            String::new()
        };
        line(
            fb,
            format!("{kind}{}{flow}", if frozen { " : GELE" } else { "" }),
            if frozen { 0x00D0_E8F8 } else { 0x0030_70C0 },
        );
    }
    let (w, sev) = sim.weather_at(map, cx, cy);
    let (_, wc) = weather_glyph(w);
    line(
        fb,
        format!(
            "METEO : {} ({:.0} %)",
            w.label().to_uppercase(),
            sev * 100.0
        ),
        wc,
    );
    line(
        fb,
        format!(
            "ATTRAIT {:.0} %  FERTILITE {:.0} %",
            b.habit * 100.0,
            b.fertility * 100.0
        ),
        GREEN,
    );
    line(fb, String::new(), DIM);
    match &g.surf_eco {
        Some(eco) if !eco.life => line(fb, "MONDE STÉRILE : AUCUNE VIE".into(), DIM),
        Some(eco) => {
            let here = eco.present_in(map.biome_at(cx, cy));
            line(fb, format!("VIE ICI : {} ESPÈCES", here.len()), AMBER);
            for &(s, m) in here.iter().take(5) {
                let sp = &g.data.species[s];
                let tag = match sp.trophic.as_str() {
                    "producer" => 'P',
                    "herbivore" => 'H',
                    "carnivore" => 'C',
                    _ => 'O',
                };
                line(
                    fb,
                    format!("[{tag}] {:<22}{:>9}", sp.name.to_uppercase(), fmt_mass(m)),
                    GREEN,
                );
            }
        }
        None => {}
    }
    line(fb, String::new(), DIM);
    match sim.catastrophe_on(map, sim.day_number()) {
        Some(c) => line(
            fb,
            format!(
                "ALERTE : {} ({:.0} %) EN {}",
                c.kind.label().to_uppercase(),
                c.severity * 100.0,
                lat_lon(c.x, c.y)
            ),
            RED,
        ),
        None => line(fb, "AUCUNE CATASTROPHE AUJOURD'HUI".into(), DIM),
    }
    line(fb, String::new(), DIM);
    // Biome shares, largest first.
    let mut count = vec![0usize; g.data.biomes.len()];
    for &b in &map.biome {
        count[usize::from(b)] += 1;
    }
    let mut order: Vec<usize> = (0..count.len()).filter(|&i| count[i] > 0).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(count[i]));
    line(fb, "BIOMES".into(), AMBER);
    for &bi in order.iter().take(6) {
        let b = &g.data.biomes[bi];
        line(
            fb,
            format!(
                "[{}] {:<18}{:>4.0} %",
                b.glyph,
                b.name.to_uppercase(),
                count[bi] as f64 * 100.0 / (W * H) as f64
            ),
            b.color,
        );
    }
}

/// Wireframe globe: a graticule, contour lines of the elevation (marching squares), the coast
/// in light blue and a coloured dot per visible cell. Only the facing hemisphere is drawn.
fn draw_globe(
    fb: &mut Framebuffer,
    g: &Game,
    map: &PlanetMap,
    x0: usize,
    y0: usize,
    cols: usize,
    rows: usize,
) {
    let (w_px, h_px) = ((cols * 8) as f32, (rows * 10) as f32);
    // Perspective camera on the -z axis looking at the origin; the globe is centred in the map
    // rectangle and sized to fit it. Own projection (no framebuffer borrow while drawing).
    let cam_dist = 3.2f64;
    let limb_r = (1.0 - 1.0 / (cam_dist * cam_dist)).sqrt();
    let (cx_px, cy_px) = ((x0 * 8) as f32 + w_px / 2.0, (y0 * 10) as f32 + h_px / 2.0);
    let focal = f64::from(0.47 * w_px.min(h_px)) * (cam_dist - 1.0 / cam_dist) / limb_r;
    let spin = Mat3::rot_y(g.surf_spin);
    let tilt = Mat3::rot_x(0.35);
    let to_view = |lat: f64, lon: f64| -> Vec3 {
        let p = Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        tilt.mul_vec(spin.mul_vec(p))
    };
    let visible = |p: Vec3| p.z < -1.0 / cam_dist + 0.01;
    let project = |p: Vec3| -> Option<(f32, f32)> {
        let z = p.z + cam_dist;
        (z > 0.1).then(|| {
            (
                cx_px + (p.x / z * focal) as f32,
                cy_px - (p.y / z * focal) as f32,
            )
        })
    };
    let globe = Globe {
        map,
        to_view: &to_view,
        visible: &visible,
        project: &project,
    };
    // One dot per visible cell, coloured by the current layer, then the graticule and contours.
    globe.dots(fb, 1, 2, &|x, y, _| cell(g, map, x, y).1);
    globe.graticule(fb, 0x0018_3040);
    globe.contours(fb, 1);
    // Limb.
    let r = limb_r;
    let z = -1.0 / cam_dist;
    for s in 0..96 {
        let (a, b) = (
            f64::from(s) / 96.0 * std::f64::consts::TAU,
            f64::from(s + 1) / 96.0 * std::f64::consts::TAU,
        );
        let (p, q) = (
            Vec3::new(r * a.cos(), r * a.sin(), z),
            Vec3::new(r * b.cos(), r * b.sin(), z),
        );
        if let (Some(u), Some(v)) = (project(p), project(q)) {
            fb.line(u.0, u.1, v.0, v.1, 0x0030_6080);
        }
    }
    // Cities as bright points.
    for (name, cx, cy) in &g.surf_cities {
        let p = to_view(Surface::lat(*cy), Surface::lon(*cx));
        if visible(p) {
            if let Some((sx, sy)) = project(p) {
                for d in [(-2, 0), (2, 0), (0, -2), (0, 2), (0, 0)] {
                    fb.plot(sx as i32 + d.0, sy as i32 + d.1, 0x00FF_FFFF);
                }
                fb.text_at(sx as i32 + 6, sy as i32 - 4, &name.to_uppercase(), AMBER);
            }
        }
    }
}
