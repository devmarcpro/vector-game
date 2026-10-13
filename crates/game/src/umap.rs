//! The map of the universe: one continuous zoom from the stellar neighbourhood (50 ly) to the
//! region (1000 ly), the whole galaxy (disc, bulge, arms), the local group and the universe of
//! neighbouring galaxies (`vector_world::universe`). It is an orbit camera (drag to turn, wheel
//! to zoom) over caches built once: a cloud of far stars for the galaxy, a local cloud around the
//! ship, the neighbouring galaxies. Every frame is O(points) with no allocation.
use crate::game::Game;
use crate::mouse::Act;
use std::cell::RefCell;
use std::f64::consts::{PI, TAU};
use vector_core::{hash_coord, Rng};
use vector_render::Framebuffer;
use vector_world::galaxy::{Galaxy, SectorCoord, R_SUN_LY};
use vector_world::universe::{GalaxyInfo, GalaxyKind, Universe};

pub const ZOOM_MIN: f64 = 5.0e-6;
pub const ZOOM_MAX: f64 = 80.0;
/// Pixels per light year of the five scales (keys 1 to 5).
pub const PRESETS: [f64; 5] = [14.0, 0.6, 0.008, 6.0e-5, 2.0e-5];
/// Radius of the universe cache (millions of light years).
const UNIVERSE_MLY: f64 = 18.0;
const CLOUD_N: usize = 9000;
const LOCAL_N: usize = 8000;

/// Names of the scales, from the zoom (pixels per light year).
pub fn scale_name(zoom: f64) -> &'static str {
    if zoom >= 3.0 {
        "VOISINAGE STELLAIRE (50 AL)"
    } else if zoom >= 0.15 {
        "REGION (1000 AL)"
    } else if zoom >= 6.0e-4 {
        "GALAXIE ENTIERE"
    } else if zoom >= 4.0e-5 {
        "GROUPE LOCAL"
    } else {
        "UNIVERS"
    }
}

pub const FILTERS: [&str; 6] = [
    "TOUTES",
    "EXPLOREES",
    "TYPE SOLAIRE (G K)",
    "NAINES ROUGES (M)",
    "CHAUDES (O B A F)",
    "RESTES (D N X)",
];

/// Does star `e` pass filter `f`?
pub fn passes(f: u8, e: &vector_world::galaxy::StarEntry, visited: bool) -> bool {
    use vector_world::StarClass::*;
    match f {
        1 => visited,
        2 => matches!(e.class(), G | K),
        3 => matches!(e.class(), M),
        4 => matches!(e.class(), O | B | A | F),
        5 => matches!(e.class(), D | N | X),
        _ => true,
    }
}

/// An orbit camera looking at `target`; orthographic, `zoom` pixels per light year.
#[derive(Clone, Copy, Debug)]
pub struct Cam {
    pub target: [f64; 3],
    pub zoom: f64,
    pub cx: f32,
    pub cy: f32,
    pub pitch: f64,
    cy_: f64,
    sy_: f64,
    cp: f64,
    sp: f64,
}

impl Cam {
    pub fn new(target: [f64; 3], yaw: f64, pitch: f64, zoom: f64, cx: f32, cy: f32) -> Cam {
        Cam {
            target,
            zoom,
            cx,
            cy,
            pitch,
            cy_: yaw.cos(),
            sy_: yaw.sin(),
            cp: pitch.cos(),
            sp: pitch.sin(),
        }
    }

    /// Screen position of the world point `p` (ly). Pitch pi/2 looks straight down the Y axis.
    pub fn project(&self, p: [f64; 3]) -> (f32, f32) {
        let d = [
            p[0] - self.target[0],
            p[1] - self.target[1],
            p[2] - self.target[2],
        ];
        let x1 = d[0] * self.cy_ + d[2] * self.sy_;
        let z1 = -d[0] * self.sy_ + d[2] * self.cy_;
        let sy = -d[1] * self.cp + z1 * self.sp;
        (
            self.cx + (x1 * self.zoom) as f32,
            self.cy + (sy * self.zoom) as f32,
        )
    }
}

#[derive(Default)]
struct Caches {
    /// The far stars of the galaxy: x, y, z (ly) and a tint 0..1 (warm centre to blue rim).
    cloud: Vec<[f32; 4]>,
    cloud_seed: Option<u64>,
    /// The stars around the ship, regenerated when it changes sector.
    local: Vec<[f32; 4]>,
    local_key: Option<(u64, SectorCoord)>,
    galaxies: Vec<(GalaxyInfo, [f64; 3])>,
    gal_seed: Option<u64>,
}

pub struct UMap {
    pub yaw: f64,
    pub pitch: f64,
    /// What the camera looks at (None: the ship).
    pub target: Option<[f64; 3]>,
    pub filter: u8,
    /// Name search being typed (None: off).
    pub search: Option<String>,
    /// Selected neighbouring galaxy.
    pub gsel: Option<usize>,
    caches: RefCell<Caches>,
}

impl Default for UMap {
    fn default() -> UMap {
        UMap {
            yaw: 0.0,
            pitch: 1.25,
            target: None,
            filter: 0,
            search: None,
            gsel: None,
            caches: RefCell::default(),
        }
    }
}

impl UMap {
    pub fn cam(&self, here: [f64; 3], zoom: f64, cx: f32, cy: f32) -> Cam {
        // Seen from outside, the galaxy turns about its own centre, not about the ship.
        let far = if zoom < 0.05 {
            [-R_SUN_LY, 0.0, 0.0]
        } else {
            here
        };
        Cam::new(
            self.target.unwrap_or(far),
            self.yaw,
            self.pitch,
            zoom,
            cx,
            cy,
        )
    }

    /// Build the caches the zoom needs; cheap when nothing changed.
    pub fn ensure(&self, galaxy: &Galaxy, here: [f64; 3], zoom: f64) {
        let mut c = self.caches.borrow_mut();
        if zoom < 0.3 && c.cloud_seed != Some(galaxy.seed) {
            c.cloud = gen_cloud(galaxy);
            c.cloud_seed = Some(galaxy.seed);
        }
        if zoom > 0.02 {
            let key = (galaxy.seed, Galaxy::sector_of(here));
            if c.local_key != Some(key) {
                c.local = gen_local(galaxy, here);
                c.local_key = Some(key);
            }
        }
        if zoom < 3.0e-3 && c.gal_seed != Some(galaxy.seed) {
            let u = Universe::new(galaxy.seed);
            c.galaxies = u
                .galaxies_near([0.0; 3], UNIVERSE_MLY)
                .into_iter()
                .map(|g| {
                    let p = [
                        g.pos[0] * 1.0e6 - R_SUN_LY,
                        g.pos[1] * 1.0e6,
                        g.pos[2] * 1.0e6,
                    ];
                    (g, p)
                })
                .collect();
            c.gal_seed = Some(galaxy.seed);
        }
    }

    /// Number of neighbouring galaxies cached (for the readout and the tests).
    #[cfg(test)]
    pub fn galaxy_count(&self) -> usize {
        self.caches.borrow().galaxies.len()
    }

    /// The neighbouring galaxy `i` and its place (ly).
    pub fn galaxy_at(&self, i: usize) -> Option<(GalaxyInfo, [f64; 3])> {
        self.caches.borrow().galaxies.get(i).cloned()
    }

    /// Index of the first neighbouring galaxy whose name contains `q` (upper case).
    pub fn find_galaxy(&self, q: &str) -> Option<usize> {
        self.caches
            .borrow()
            .galaxies
            .iter()
            .position(|(g, _)| g.name.to_uppercase().contains(q))
    }
}

fn gen_cloud(galaxy: &Galaxy) -> Vec<[f32; 4]> {
    let mut rng = Rng::new(galaxy.seed ^ 0xC10D_0001);
    let mut v = Vec::with_capacity(CLOUD_N);
    let mut tries = 0;
    while v.len() < CLOUD_N && tries < 900_000 {
        tries += 1;
        let r = 40_000.0 * rng.f64().sqrt();
        let th = TAU * rng.f64();
        let y = rng.range(-1500.0, 1500.0);
        let p = [r * th.cos() - R_SUN_LY, y, r * th.sin()];
        // The arms are exaggerated so that the spiral reads from afar.
        let arm = (Galaxy::arm_strength(p) / 1.7).powi(2).clamp(0.05, 1.0);
        let keep = if r < 3500.0 { 1.0 } else { arm };
        if rng.f64() * 40.0 < Galaxy::density(p) * keep {
            v.push([p[0] as f32, p[1] as f32, p[2] as f32, (r / 40_000.0) as f32]);
        }
    }
    v
}

fn gen_local(galaxy: &Galaxy, here: [f64; 3]) -> Vec<[f32; 4]> {
    let key = Galaxy::sector_of(here);
    let mut rng = Rng::new(hash_coord(galaxy.seed ^ 0x10CA_1000, key.0, key.1, key.2));
    let dh = Galaxy::density(here).max(0.5);
    let mut v = Vec::with_capacity(LOCAL_N);
    let mut tries = 0;
    while v.len() < LOCAL_N && tries < 120_000 {
        tries += 1;
        let p = [
            here[0] + rng.range(-1500.0, 1500.0),
            here[1] + rng.range(-1500.0, 1500.0),
            here[2] + rng.range(-1500.0, 1500.0),
        ];
        if rng.f64() * dh * 2.0 < Galaxy::density(p) {
            v.push([p[0] as f32, p[1] as f32, p[2] as f32, rng.f64() as f32]);
        }
    }
    v
}

fn fade(zoom: f64, lo: f64, hi: f64) -> f32 {
    // 1 between lo and hi, ramping over one decade outside.
    let a = (zoom / lo).log10().clamp(0.0, 1.0);
    let b = (hi / zoom).log10().clamp(0.0, 1.0);
    (a.min(b)) as f32
}

fn tint(rgb: u32, k: f32) -> u32 {
    let c = |s: u32| (((rgb >> s) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    (c(16) << 16) | (c(8) << 8) | c(0)
}

fn kind_color(k: GalaxyKind) -> u32 {
    match k {
        GalaxyKind::Spiral => 0x0090_B0FF,
        GalaxyKind::Barred => 0x00A0_90FF,
        GalaxyKind::Elliptical => 0x00FF_D090,
        GalaxyKind::Irregular => 0x0090_FFC0,
    }
}

/// The layers behind the stars: far stars, local stars, neighbouring galaxies, scale bar.
pub fn draw_backdrop(fb: &mut Framebuffer, g: &Game, cam: &Cam) {
    let s = &g.sim;
    let zoom = cam.zoom;
    g.umap.ensure(&s.galaxy, s.here_pos, zoom);
    let c = g.umap.caches.borrow();
    // Far stars of the galaxy: warm yellow in the middle, blue-white on the rim.
    let a = fade(zoom, 1.0e-5, 0.3);
    let a = if zoom < 1.0e-5 { 0.6 } else { a };
    if a > 0.0 {
        for p in &c.cloud {
            let (x, y) = cam.project([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
            let r = p[3];
            let base = 0x00FF_D8A0u32;
            let rim = 0x0090_B0FFu32;
            let col = if r < 0.25 { base } else { rim };
            fb.plot(x as i32, y as i32, tint(col, 0.22 * a * (1.2 - r)));
        }
    }
    // Local stars: the density of the disc around the ship.
    let a = fade(zoom, 0.02, 40.0);
    if a > 0.0 {
        for p in &c.local {
            let (x, y) = cam.project([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
            let col = if p[3] < 0.3 { 0x00FF_C890 } else { 0x00C0_D0FF };
            fb.plot(x as i32, y as i32, tint(col, 0.7 * a));
        }
    }
    // Neighbouring galaxies.
    let a = fade(zoom, 2.0e-6, 3.0e-3);
    if a > 0.0 {
        let sel = g.umap.gsel;
        for (i, (gi, p)) in c.galaxies.iter().enumerate() {
            if gi.id == vector_world::universe::GalaxyId::HOME {
                continue;
            }
            let (x, y) = cam.project(*p);
            if x < -40.0 || y < -40.0 || x > fb.w as f32 + 40.0 || y > fb.h as f32 + 40.0 {
                continue;
            }
            let rad = (gi.radius_kly * 1000.0 * zoom) as f32;
            let col = kind_color(gi.kind);
            if rad < 3.0 {
                let k = (0.6 + gi.radius_kly as f32 / 100.0).min(1.4) * a;
                for (dx, dy, f) in [
                    (0, 0, 1.0),
                    (1, 0, 0.5),
                    (-1, 0, 0.5),
                    (0, 1, 0.5),
                    (0, -1, 0.5),
                ] {
                    fb.plot(x as i32 + dx, y as i32 + dy, tint(col, k * f));
                }
            } else {
                glyph(fb, cam, gi, *p, tint(col, 0.8 * a));
            }
            let (hw, hh) = (rad.max(5.0) as i32, rad.max(5.0) as i32);
            g.hit(
                (x as i32 - hw, y as i32 - hh, x as i32 + hw, y as i32 + hh),
                Act::Call(Game::click_galaxy, i),
                None,
            );
            if sel == Some(i) {
                fb.circle(x, y, rad.max(7.0) + 3.0, 0x00FF_C040);
                fb.text_at(x as i32 + 10, y as i32 - 4, &gi.name, 0x00FF_C040);
            } else if rad > 14.0 {
                fb.text_at(
                    x as i32 + 6,
                    y as i32 + rad as i32 / 2,
                    &gi.name,
                    tint(col, 0.5),
                );
            }
        }
        // Our own galaxy keeps its name when seen from outside.
        if zoom < 6.0e-4 {
            let (x, y) = cam.project([-R_SUN_LY, 0.0, 0.0]);
            fb.text_at(
                x as i32 + 6,
                y as i32 + 6,
                &s.galaxy.name().to_uppercase(),
                0x00FF_C040,
            );
        }
    }
    // Scale bar: a round number of light years about 100 pixels long.
    let want = 100.0 / zoom;
    let decade = 10f64.powf(want.log10().floor());
    let len = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * decade)
        .find(|l| *l >= want * 0.5)
        .unwrap_or(want);
    let (x0, y0) = (12.0, fb.h as f32 - 110.0);
    fb.line(x0, y0, x0 + (len * zoom) as f32, y0, 0x0090_A0A0);
    let label = if len >= 1.0e6 {
        format!("{} MAL", fmt_num(len / 1.0e6))
    } else if len >= 1.0e3 {
        format!("{} KAL", fmt_num(len / 1.0e3))
    } else {
        format!("{} AL", fmt_num(len))
    };
    fb.text_at(x0 as i32, y0 as i32 + 4, &label, 0x0090_A0A0);
}

fn fmt_num(v: f64) -> String {
    if v >= 10.0 || (v - v.round()).abs() < 1e-9 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// A galaxy as a handful of points in its own tilted plane.
fn glyph(fb: &mut Framebuffer, cam: &Cam, gi: &GalaxyInfo, p: [f64; 3], col: u32) {
    let h = hash_coord(gi.seed, 1, 2, 3);
    let tilt = (h & 0xFF) as f64 / 255.0 * PI;
    let phase = ((h >> 8) & 0xFF) as f64 / 255.0 * TAU;
    let rad = gi.radius_kly * 1000.0;
    let n = 36;
    for i in 0..n {
        let f = (i as f64 + 1.0) / n as f64;
        let (r, a, th) = match gi.kind {
            GalaxyKind::Spiral | GalaxyKind::Barred => {
                let arm = (i % 2) as f64 * PI;
                (f * rad, phase + arm + 3.2 * f, 0.0)
            }
            GalaxyKind::Elliptical => {
                let q = hash_coord(gi.seed, i, 7, 7);
                let a = (q & 0xFFFF) as f64 / 65535.0 * TAU;
                (f.sqrt() * rad * 0.8, a, 0.0)
            }
            GalaxyKind::Irregular => {
                let q = hash_coord(gi.seed, i, 9, 9);
                let a = (q & 0xFFFF) as f64 / 65535.0 * TAU;
                (((q >> 16) & 0xFF) as f64 / 255.0 * rad, a, 0.4)
            }
        };
        let (lx, lz) = (r * a.cos(), r * a.sin());
        // Tilt the plane about the X axis.
        let (ly, lz) = (lz * tilt.sin() + th * rad * 0.1, lz * tilt.cos());
        let (x, y) = cam.project([p[0] + lx, p[1] + ly, p[2] + lz]);
        fb.plot(x as i32, y as i32, col);
    }
    let (x, y) = cam.project(p);
    fb.plot(x as i32, y as i32, tint(col, 1.4));
}

/// The info panel of a selected neighbouring galaxy; false when none is selected or the zoom is
/// too close for galaxies to matter.
pub fn draw_galaxy_panel(fb: &mut Framebuffer, g: &Game, x: usize) -> bool {
    if g.gal_zoom >= 3.0e-3 {
        return false;
    }
    let Some((gi, _)) = g.umap.gsel.and_then(|i| g.umap.galaxy_at(i)) else {
        return false;
    };
    let amber = 0x00FF_C040;
    let green = 0x0060_E080;
    fb.text(x, 4, &gi.name, amber);
    fb.text(x, 5, gi.kind.label(), 0x0080_8080);
    fb.text(
        x,
        7,
        &format!("DIAMETRE {:.0} KAL", gi.radius_kly * 2.0),
        green,
    );
    fb.text(
        x,
        8,
        &format!("ETOILES {:.0} MILLIARDS", gi.stars_billions),
        green,
    );
    let d = (gi.pos[0].powi(2) + gi.pos[1].powi(2) + gi.pos[2].powi(2)).sqrt();
    fb.text(x, 9, &format!("DISTANCE {d:.2} MAL"), green);
    fb.text(
        x,
        10,
        "VOYAGE INTERGALACTIQUE : EN PREPARATION",
        0x0080_8080,
    );
    true
}

/// The lines under the map: scale, filter, search.
pub fn draw_status(fb: &mut Framebuffer, g: &Game, row: usize) {
    let u = &g.umap;
    fb.text(
        1,
        row,
        &format!(
            "ECHELLE : {}   FILTRE : {}   CAMERA : {}",
            scale_name(g.gal_zoom),
            FILTERS[usize::from(u.filter) % FILTERS.len()],
            if u.target.is_some() {
                "LIBRE"
            } else {
                "SUR LE VAISSEAU"
            }
        ),
        0x0060_E080,
    );
    if let Some(q) = &u.search {
        fb.text(
            1,
            row - 1,
            &format!("RECHERCHE : {q}_  (ENTREE: ALLER, ECHAP: ANNULER)"),
            0x00FF_C040,
        );
    }
}

impl Game {
    pub fn click_galaxy(&mut self, i: usize) {
        self.umap.gsel = Some(i);
    }

    pub fn umap_zoom_by(&mut self, f: f64) {
        self.gal_zoom = (self.gal_zoom * f).clamp(ZOOM_MIN, ZOOM_MAX);
    }

    pub fn umap_drag(&mut self, dx: i32, dy: i32) {
        self.umap.yaw += f64::from(dx) * 0.01;
        self.umap.pitch =
            (self.umap.pitch + f64::from(dy) * 0.01).clamp(0.05, std::f64::consts::FRAC_PI_2);
    }

    /// Centre the camera on the selected star (or galaxy at the far scales).
    pub fn umap_centre_selection(&mut self) {
        if self.gal_zoom < 3.0e-3 {
            if let Some((_, p)) = self.umap.gsel.and_then(|i| self.umap.galaxy_at(i)) {
                self.umap.target = Some(p);
                return;
            }
        }
        if let Some(e) = self.gal_cache.get(self.gal_sel) {
            self.umap.target = Some(e.pos);
        }
    }

    /// Jump the search to the first match of `q` among the known stars and the galaxies.
    pub fn umap_search_apply(&mut self) {
        let q = self.umap.search.take().unwrap_or_default();
        if q.is_empty() {
            return;
        }
        if let Some(k) = self
            .gal_cache
            .iter()
            .position(|e| e.header.name.to_uppercase().contains(&q))
        {
            self.gal_sel = k;
            self.umap.target = Some(self.gal_cache[k].pos);
            return;
        }
        self.umap
            .ensure(&self.sim.galaxy, self.sim.here_pos, 1.0e-4);
        if let Some(i) = self.umap.find_galaxy(&q) {
            self.umap.gsel = Some(i);
            if let Some((_, p)) = self.umap.galaxy_at(i) {
                self.umap.target = Some(p);
            }
            self.gal_zoom = 6.0e-5;
            return;
        }
        self.say(format!("AUCUN NOM NE CONTIENT {q}"));
    }

    /// Keys of the map; true when it used the key.
    pub fn umap_key(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode as K;
        if self.umap.search.is_some() {
            match code {
                K::Escape => self.umap.search = None,
                K::Enter | K::NumpadEnter => self.umap_search_apply(),
                K::Backspace => {
                    if let Some(s) = self.umap.search.as_mut() {
                        s.pop();
                    }
                }
                _ => {
                    if let (Some(t), Some(s)) = (text, self.umap.search.as_mut()) {
                        for ch in t
                            .chars()
                            .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
                        {
                            if s.len() < 24 {
                                s.extend(ch.to_uppercase());
                            }
                        }
                    }
                }
            }
            return true;
        }
        let preset = match code {
            K::Digit1 => Some(0),
            K::Digit2 => Some(1),
            K::Digit3 => Some(2),
            K::Digit4 => Some(3),
            K::Digit5 => Some(4),
            _ => None,
        };
        if let Some(i) = preset {
            self.gal_zoom = PRESETS[i];
            return true;
        }
        match code {
            K::KeyF => self.umap.filter = (self.umap.filter + 1) % FILTERS.len() as u8,
            K::KeyC => self.umap_centre_selection(),
            K::KeyR | K::Home => {
                self.umap.target = None;
                self.umap.gsel = None;
            }
            K::Slash | K::KeyS => self.umap.search = Some(String::new()),
            K::ArrowLeft => self.umap.yaw -= 0.15,
            K::ArrowRight => self.umap.yaw += 0.15,
            K::PageUp => self.umap.pitch = (self.umap.pitch + 0.1).min(std::f64::consts::FRAC_PI_2),
            K::PageDown => self.umap.pitch = (self.umap.pitch - 0.1).max(0.05),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_stays_at_the_centre_at_every_scale_and_angle() {
        let t = [123.0, -45.0, 6789.0];
        for z in PRESETS.iter().chain([ZOOM_MIN, ZOOM_MAX].iter()) {
            for (yaw, pitch) in [
                (0.0, 1.25),
                (2.0, 0.05),
                (-3.0, std::f64::consts::FRAC_PI_2),
            ] {
                let cam = Cam::new(t, yaw, pitch, *z, 320.0, 200.0);
                let (x, y) = cam.project(t);
                assert!((x - 320.0).abs() < 1e-3 && (y - 200.0).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn looking_straight_down_is_the_plane_of_the_disc() {
        let cam = Cam::new([0.0; 3], 0.0, std::f64::consts::FRAC_PI_2, 2.0, 0.0, 0.0);
        let (x, y) = cam.project([10.0, 99.0, 5.0]);
        assert!((x - 20.0).abs() < 1e-2 && (y - 10.0).abs() < 1e-2);
    }

    #[test]
    fn the_five_scales_have_their_names_and_zoom_is_monotonic() {
        let names: Vec<_> = PRESETS.iter().map(|z| scale_name(*z)).collect();
        assert_eq!(names[0], "VOISINAGE STELLAIRE (50 AL)");
        assert_eq!(names[1], "REGION (1000 AL)");
        assert_eq!(names[2], "GALAXIE ENTIERE");
        assert_eq!(names[3], "GROUPE LOCAL");
        assert_eq!(names[4], "UNIVERS");
        assert!(PRESETS.windows(2).all(|w| w[0] > w[1]));
    }

    #[test]
    fn the_caches_build_at_every_scale_without_panic() {
        let g = Galaxy::new(11);
        let u = UMap::default();
        for z in PRESETS {
            u.ensure(&g, [5.0, 0.0, 5.0], z);
        }
        assert!(u.galaxy_count() > 100);
        assert!(!u.caches.borrow().cloud.is_empty());
        assert!(!u.caches.borrow().local.is_empty());
        // The cloud follows the disc: most points are near the plane.
        let c = u.caches.borrow();
        let flat = c.cloud.iter().filter(|p| p[1].abs() < 600.0).count();
        assert!(flat * 2 > c.cloud.len());
    }
}
