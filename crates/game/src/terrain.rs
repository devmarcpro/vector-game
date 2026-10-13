//! The ground of a world at every scale. One height function (the map of the world, 400 km per
//! cell, plus octaves of noise down to a few metres) and one renderer (a heightfield marched
//! column by column, the steps growing with the distance) serve the cockpit when a ship flies low
//! over a world and the first-person view of an expedition on foot: the nearer the ground, the
//! finer the detail, and from orbit down to the soles of the squad it is the same ground.
use crate::globe::elev_at;
use vector_render::{mix_color as mix, scale_color as scale, Framebuffer};
use vector_sim::planet::PlanetMap;
use vector_world::surface::{Surface, H, W};

/// `f64::floor` without the call into the maths library (the march calls it millions of times):
/// right for |x| below 9e18.
#[inline]
pub(crate) fn ffloor(x: f64) -> f64 {
    let t = x as i64 as f64;
    if t > x {
        t - 1.0
    } else {
        t
    }
}

/// `x` wrapped to [0, TAU), without `fmod`.
#[inline]
pub(crate) fn wrap_tau(x: f64) -> f64 {
    let r = x - std::f64::consts::TAU * ffloor(x * (1.0 / std::f64::consts::TAU));
    if r >= std::f64::consts::TAU {
        r - std::f64::consts::TAU
    } else if r < 0.0 {
        r + std::f64::consts::TAU
    } else {
        r
    }
}

/// Wavelength of the coarsest octave added to the map, and the ratio between octaves.
const LAMBDA0: f64 = 800_000.0;
const RATIO: f64 = 3.0;
const OCTAVES: usize = 16;

pub struct Terrain<'a> {
    pub map: &'a PlanetMap,
    pub radius: f64,
    /// Metres of height for one unit of elevation of the map.
    pub relief: f64,
    /// Craters (airless worlds: nothing wears them away), and the seed of their placing.
    pub craters: bool,
    pub seed: u64,
    /// Which cells of the map have their water frozen now (empty: none do).
    pub ice: Vec<bool>,
}

/// Water on the land: a lake (flat, at the level its basin would fill to) or a river (a line
/// that follows the ground), with the height of its surface (m above the sea).
#[derive(Clone, Copy, Debug)]
pub struct Inland {
    pub river: bool,
    pub level: f32,
}

/// Colours of the water, and of its ice.
pub const SEA_ICE: u32 = 0x00D8_ECF8;
pub const LAKE_ICE: u32 = 0x00C0_E0F0;
pub const LAKE_COLOR: u32 = 0x0020_60A8;
pub const RIVER_COLOR: u32 = 0x002A_6CB4;

fn noise2(x: f64, y: f64) -> f32 {
    let (ix, iy) = (ffloor(x), ffloor(y));
    let (fx, fy) = ((x - ix) as f32, (y - iy) as f32);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let v = |dx: i64, dy: i64| {
        let h = vector_core::hash_coord(13, ix as i64 + dx, iy as i64 + dy, 4);
        ((h >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
    };
    let a = v(0, 0) + (v(1, 0) - v(0, 0)) * fx;
    let b = v(0, 1) + (v(1, 1) - v(0, 1)) * fx;
    a + (b - a) * fy
}

/// The four lattice values of the last cell of one octave of noise: along a ray the coarse
/// octaves stay in the same cell for many steps, so their hashes are not made again.
#[derive(Clone, Copy)]
struct Lattice {
    ix: i64,
    iy: i64,
    v: [f32; 4],
}

/// A crater: centre, radius and depth scale.
#[derive(Clone, Copy, Default)]
struct Crater {
    cx: f64,
    cy: f64,
    r: f64,
    dp: f64,
}

#[derive(Clone, Copy)]
struct CraterCell {
    ix: i64,
    iy: i64,
    n: usize,
    list: [Crater; 9],
}

/// What `Terrain::sample_with` remembers from one call to the next (one per ray). The answers are
/// the same with or without it, only faster.
pub struct SampleCache {
    oct: [Lattice; OCTAVES],
    crat: [CraterCell; 4],
    /// The three octaves of variation of the colour of the ground.
    var: [Lattice; 3],
}

impl Default for SampleCache {
    fn default() -> Self {
        SampleCache {
            oct: [Lattice {
                ix: i64::MIN,
                iy: i64::MIN,
                v: [0.0; 4],
            }; OCTAVES],
            crat: [CraterCell {
                ix: i64::MIN,
                iy: i64::MIN,
                n: 0,
                list: [Crater::default(); 9],
            }; 4],
            var: [Lattice {
                ix: i64::MIN,
                iy: i64::MIN,
                v: [0.0; 4],
            }; 3],
        }
    }
}

fn noise2_cached(l: &mut Lattice, x: f64, y: f64) -> f32 {
    let (ix, iy) = (ffloor(x), ffloor(y));
    let (fx, fy) = ((x - ix) as f32, (y - iy) as f32);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ixi, iyi) = (ix as i64, iy as i64);
    if l.ix != ixi || l.iy != iyi {
        let v = |dx: i64, dy: i64| {
            let h = vector_core::hash_coord(13, ixi + dx, iyi + dy, 4);
            ((h >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        };
        *l = Lattice {
            ix: ixi,
            iy: iyi,
            v: [v(0, 0), v(1, 0), v(0, 1), v(1, 1)],
        };
    }
    let a = l.v[0] + (l.v[1] - l.v[0]) * fx;
    let b = l.v[2] + (l.v[3] - l.v[2]) * fx;
    a + (b - a) * fy
}

impl<'a> Terrain<'a> {
    pub fn new(map: &'a PlanetMap, radius: f64) -> Self {
        Terrain {
            map,
            radius,
            relief: (radius * 0.0007).clamp(1500.0, 9000.0),
            craters: map.pressure < 0.02,
            seed: map.seed,
            ice: Vec::new(),
        }
    }

    /// The same ground at a point of the year: the water that is cold enough there is ice.
    pub fn with_season(mut self, phase: f64) -> Self {
        let s = &self.map.surface;
        if s.elev.iter().any(|&e| e < 0.0) {
            let temps = vector_world::climate::season_map(s, phase);
            self.ice = (0..W * H)
                .map(|i| {
                    let (x, y) = (i % W, i / W);
                    s.is_water(x, y)
                        && vector_world::climate::frozen(f64::from(temps[i]), s.is_sea(x, y))
                })
                .collect();
        }
        self
    }

    pub fn is_ice(&self, x: usize, y: usize) -> bool {
        self.ice.get(Surface::idx(x, y)).copied().unwrap_or(false)
    }

    /// The lake or river at a point, if there is one. Rivers are drawn at least as wide as the
    /// pixel (`lod`, m) so that they stay visible from afar, and meander a little.
    pub fn inland_water(&self, lat: f64, lon: f64, lod: f64) -> Option<Inland> {
        let h = &self.map.surface.hydro;
        let fx = wrap_tau(lon) * (W as f64 / std::f64::consts::TAU) - 0.5;
        let fy = (0.5 - lat / std::f64::consts::PI) * H as f64 - 0.5;
        let (wx, wy) = (lon * self.radius, lat * self.radius);
        let lake = h.lake_at(fx, fy);
        if lake > 0.0 && lake > 0.5 + 0.2 * noise2(wx / 40_000.0, wy / 40_000.0) {
            let level = h.filled_at(fx, fy).max(0.0) as f64 * self.relief;
            return Some(Inland {
                river: false,
                level: level as f32,
            });
        }
        let cell_n = self.radius * std::f64::consts::PI / H as f64;
        let (d, rel) = h.river_near(fx, fy)?;
        let meander = f64::from(noise2(wx / 90.0, wy / 90.0)) * 5.0;
        let half = vector_world::hydro::river_half_width_m(rel).max(lod * 1.2);
        (d * cell_n + meander < half).then_some(Inland {
            river: true,
            level: f32::NAN,
        })
    }

    /// The cell of the map a point belongs to.
    pub fn cell(lat: f64, lon: f64) -> (usize, usize) {
        let lon = wrap_tau(lon);
        (
            ((lon / std::f64::consts::TAU * W as f64) as usize).min(W - 1),
            (((0.5 - lat / std::f64::consts::PI) * H as f64).max(0.0) as usize).min(H - 1),
        )
    }

    /// Height above the sea at a point (m; the sea itself is 0) and the elevation of the map
    /// there (negative under the sea), with the octaves of detail the distance `lod` (the size
    /// of what can still be told apart, m) deserves.
    pub fn sample(&self, lat: f64, lon: f64, lod: f64) -> (f32, f32) {
        self.sample_with(lat, lon, lod, &mut SampleCache::default())
    }

    /// `sample`, remembering the lattice cells between calls (see `SampleCache`).
    pub fn sample_with(&self, lat: f64, lon: f64, lod: f64, cache: &mut SampleCache) -> (f32, f32) {
        let fx = wrap_tau(lon) * (W as f64 / std::f64::consts::TAU) - 0.5;
        let fy = (0.5 - lat / std::f64::consts::PI) * H as f64 - 0.5;
        let e = elev_at(self.map, fx, fy);
        let base = f64::from(e) * self.relief;
        let rugged = 0.3 + 0.7 * f64::from(e.clamp(0.0, 1.0));
        let (x, y) = (lon * self.radius, lat * self.radius);
        let mut h = base;
        let mut lambda = LAMBDA0;
        let mut amp = 0.22 * self.relief * rugged;
        for l in cache.oct.iter_mut() {
            if lambda < lod * 2.5 {
                break;
            }
            let inv = 1.0 / lambda;
            h += f64::from(noise2_cached(l, x * inv, y * inv)) * amp;
            lambda /= RATIO;
            amp *= 0.45;
        }
        if self.craters {
            h += self.crater_height(x, y, lod, &mut cache.crat);
        }
        (h as f32, e)
    }

    /// Height of the craters at a point (m, negative in the bowls, positive on the rims): three
    /// sizes of them, each on a grid with at most one crater per square.
    fn crater_height(&self, x: f64, y: f64, lod: f64, cache: &mut [CraterCell; 4]) -> f64 {
        let mut sum = 0.0;
        for (k, (cell, depth)) in [
            (900_000.0, 0.16),
            (220_000.0, 0.14),
            (50_000.0, 0.12),
            (11_000.0, 0.10),
        ]
        .into_iter()
        .enumerate()
        {
            // A crater smaller than what can be told apart at this distance is not drawn.
            if cell * 0.3 < lod * 2.0 {
                continue;
            }
            let (ix, iy) = (ffloor(x / cell), ffloor(y / cell));
            let (ixi, iyi) = (ix as i64, iy as i64);
            let cc = &mut cache[k];
            if cc.ix != ixi || cc.iy != iyi {
                cc.ix = ixi;
                cc.iy = iyi;
                cc.n = 0;
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let (gx, gy) = (ixi + dx, iyi + dy);
                        let h = |k: i64| {
                            (vector_core::hash_coord(
                                self.seed ^ 0xC8A7,
                                gx,
                                gy,
                                k + (cell as i64 % 97),
                            ) >> 11) as f64
                                / (1u64 << 53) as f64
                        };
                        if h(0) > 0.45 {
                            continue;
                        }
                        let r = cell * (0.12 + 0.18 * h(3));
                        cc.list[cc.n] = Crater {
                            cx: (gx as f64 + 0.2 + 0.6 * h(1)) * cell,
                            cy: (gy as f64 + 0.2 + 0.6 * h(2)) * cell,
                            r,
                            dp: depth * r * 0.4,
                        };
                        cc.n += 1;
                    }
                }
            }
            for c in &cc.list[..cc.n] {
                let d = ((x - c.cx).powi(2) + (y - c.cy).powi(2)).sqrt() / c.r;
                sum += if d < 1.0 {
                    -c.dp * (1.0 - d * d)
                } else if d < 1.5 {
                    0.35 * c.dp * (1.0 - (d - 1.0) / 0.5).powi(2)
                } else {
                    0.0
                };
            }
        }
        sum
    }
}

/// Where the viewer is and where it looks: latitude and longitude of the feet, height above the
/// sea, heading (0 north, clockwise) and pitch (up positive), radians.
pub struct Eye {
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
    pub yaw: f64,
    pub pitch: f64,
}

/// The window on the screen and the focal length (pixels).
pub struct Frame {
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
    pub focal: f64,
}

/// The sky: colours at the zenith and the horizon (None in a vacuum: the stars stay), and the
/// distance at which the air hides the ground (m).
pub struct Sky {
    pub air: Option<(u32, u32)>,
    /// Cloud cover over the eye, 0..1: the sky greys and the light with it.
    pub cloud: f32,
    /// Distance at which the air at the ground hides the ground (m), and the scale height of
    /// that air (m): the thinner it gets with height, the less it hazes.
    pub haze: f64,
    pub scale: f64,
}

/// Direction of the light: up component and the east and north ones.
pub struct Light {
    pub up: f64,
    pub east: f64,
    pub north: f64,
}

/// What the renderer left behind: the distance of what was drawn at each pixel of each column
/// (columns are two pixels wide), so that sprites can hide behind a ridge.
pub struct Depth {
    pub cols: i32,
    pub rows: i32,
    pub d: Vec<f32>,
}

impl Depth {
    pub fn at(&self, col: i32, y: i32) -> f32 {
        if (0..self.cols).contains(&col) && (0..self.rows).contains(&y) {
            self.d[(col * self.rows + y) as usize]
        } else {
            f32::MAX
        }
    }
}

/// The unpainted marker of a pixel of the marched columns.
const NONE: u32 = u32::MAX;

/// Everything the march of a column reads.
struct March<'a> {
    t: &'a Terrain<'a>,
    eye: &'a Eye,
    fr: &'a Frame,
    sky: &'a Sky,
    light: &'a Light,
    far: f64,
    horizon: f64,
    cx: f64,
    horizon_color: u32,
    cos_lat: f64,
}

impl March<'_> {
    /// March one column: the colour (NONE: sky) and the distance of each pixel from the top.
    fn column(
        &self,
        col: i32,
        out: &mut [(u32, f32)],
        color_of: &dyn Fn(usize, usize) -> u32,
        local: &dyn Fn(f64, f64) -> Option<u32>,
    ) {
        let (t, eye, fr, sky, light) = (self.t, self.eye, self.fr, self.sky, self.light);
        let theta = ((f64::from(col) * 2.0 + 1.0 - self.cx) / fr.focal).atan();
        let yaw = eye.yaw + theta;
        let (de, dn) = (yaw.sin(), yaw.cos());
        let cos_t = theta.cos();
        // Slope against the light is read along the ray.
        let along = de * light.east + dn * light.north;
        let mut ymin = fr.h;
        let mut d = 1.0f64;
        let mut prev: Option<(f64, f32)> = None;
        let mut cache = SampleCache::default();
        while d < self.far && ymin > 0 {
            let lat = eye.lat + dn * d / t.radius;
            let lon = eye.lon + de * d / (t.radius * self.cos_lat);
            let (h_raw, e) = t.sample_with(lat, lon, d / fr.focal * 2.0, &mut cache);
            let sea = e <= 0.0 && h_raw <= 0.0;
            let inland = if sea {
                None
            } else {
                t.inland_water(lat, lon, d / fr.focal * 2.0)
            };
            let h = match inland {
                _ if sea => 0.0,
                Some(w) if !w.river => f64::from(w.level),
                // A river lies a little below the ground it cuts through.
                Some(_) => f64::from(h_raw) - 1.5,
                None => f64::from(h_raw),
            };
            let drop = d * d / (2.0 * t.radius);
            let dz = h - eye.height - drop;
            let ytop = ffloor(self.horizon - fr.focal * dz / (d * cos_t)) as i32;
            if ytop < ymin {
                let (ci, ri) = Terrain::cell(lat, lon);
                // Near the squad the ground is what the cells of the grid are made of.
                let mut base = local(lat, lon).unwrap_or_else(|| color_of(ci, ri));
                // Variation from place to place, snow and bare rock on the heights, water.
                let (wx, wy) = (lon * t.radius, lat * t.radius);
                let near = 1.0 / (1.0 + d / 600.0) as f32;
                let [v0, v1, v2] = &mut cache.var;
                let mut var = 1.0 + 0.10 * noise2_cached(v0, wx / 400.0, wy / 400.0);
                // Far away the fine variation is below what a pixel can show.
                if near > 0.002 {
                    var += near
                        * (0.10 * noise2_cached(v1, wx / 3.0, wy / 3.0)
                            + 0.07 * noise2_cached(v2, wx / 30.0, wy / 30.0));
                }
                let iced = t.is_ice(ci, ri);
                let k = if sea {
                    base = if iced {
                        SEA_ICE
                    } else {
                        mix(
                            0x0018_4A8C,
                            0x0030_90B0,
                            (1.0 - (-e * 3.0).exp()).clamp(0.0, 0.7),
                        )
                    };
                    1.0
                } else if let Some(w) = inland {
                    base = match (iced, w.river) {
                        (true, _) => LAKE_ICE,
                        (false, true) => RIVER_COLOR,
                        (false, false) => LAKE_COLOR,
                    };
                    1.0
                } else {
                    let hh = h as f32 / t.relief as f32;
                    if hh > 0.85 {
                        base = mix(base, 0x00F0_F0F8, ((hh - 0.85) * 5.0).clamp(0.0, 1.0));
                    } else if hh > 0.55 {
                        base = mix(base, 0x0090_8880, ((hh - 0.55) * 2.5).clamp(0.0, 0.6));
                    }
                    var
                };
                let mut lit = 0.55;
                if let Some((pd, ph)) = prev {
                    let slope = (h - f64::from(ph)) / (d - pd).max(0.01);
                    let n_dot_l = (-slope * along + light.up) / (1.0 + slope * slope).sqrt();
                    lit = 0.2 + 0.62 * n_dot_l.clamp(0.0, 1.0);
                }
                let mut color = scale(base, (lit as f32 * k * 0.6).clamp(0.05, 1.0));
                if sky.air.is_some() {
                    // The air thins with height (scale height 8 km): high above the ground
                    // there is little between the eye and what it looks at.
                    let mean = (eye.height.max(0.0) + h.max(0.0)) / 2.0;
                    let density = (-mean / sky.scale.max(100.0)).exp();
                    let fog = (1.0 - (-d * density / sky.haze).exp()) as f32;
                    color = mix(color, self.horizon_color, fog);
                }
                for yy in ytop.max(0)..ymin {
                    out[yy as usize] = (color, d as f32);
                }
                ymin = ytop.max(0);
            }
            prev = Some((d, h as f32));
            // Far from the eye the steps are longer than a pixel anyway.
            d += d * if d < 8000.0 { 0.03 } else { 0.045 } + 0.5;
        }
    }
}

enum Colors<'a> {
    Serial(
        &'a dyn Fn(usize, usize) -> u32,
        &'a dyn Fn(f64, f64) -> Option<u32>,
    ),
    Table(&'a [u32]),
}

/// March the ground from the nearest to the farthest and paint it. `color_of` gives the colour
/// of a cell of the map; the steps grow with the distance, so that the detail and the cost fall
/// with it. One thread: the closures need not be `Sync`.
#[allow(clippy::too_many_arguments)]
pub fn render(
    fb: &mut Framebuffer,
    t: &Terrain,
    color_of: &dyn Fn(usize, usize) -> u32,
    local: &dyn Fn(f64, f64) -> Option<u32>,
    eye: &Eye,
    fr: &Frame,
    sky: &Sky,
    light: &Light,
    far: f64,
) -> Depth {
    render_with(
        fb,
        t,
        Colors::Serial(color_of, local),
        eye,
        fr,
        sky,
        light,
        far,
    )
}

/// The same on all the cores, for a ground coloured by a table of the cells of the map (row by
/// row, `W * H` colours) and nothing local. The picture is the same as `render`'s.
#[allow(clippy::too_many_arguments)]
pub fn render_par(
    fb: &mut Framebuffer,
    t: &Terrain,
    cell_colors: &[u32],
    eye: &Eye,
    fr: &Frame,
    sky: &Sky,
    light: &Light,
    far: f64,
) -> Depth {
    render_with(fb, t, Colors::Table(cell_colors), eye, fr, sky, light, far)
}

#[allow(clippy::too_many_arguments)]
fn render_with(
    fb: &mut Framebuffer,
    t: &Terrain,
    colors: Colors,
    eye: &Eye,
    fr: &Frame,
    sky: &Sky,
    light: &Light,
    far: f64,
) -> Depth {
    let _p = crate::perf::span("terrain_render");
    let (cx, cy) = (f64::from(fr.w) / 2.0, f64::from(fr.h) / 2.0);
    let horizon = cy + fr.focal * eye.pitch.tan().clamp(-1.4, 1.4);
    let cols = fr.w / 2;
    let ncols = cols.max(0) as usize;
    let rows = fr.h.max(1) as usize;
    let overcast = |c: u32| mix(c, 0x0098_A0A8, (sky.cloud * 0.75).clamp(0.0, 0.75));
    let horizon_color = sky.air.map_or(0, |a| overcast(a.1));
    let m = March {
        t,
        eye,
        fr,
        sky,
        light,
        far,
        horizon,
        cx,
        horizon_color,
        cos_lat: eye.lat.cos().abs().max(0.05),
    };
    // March every column into its own strip of `rows` pixels.
    let mut strips = vec![(NONE, f32::MAX); ncols * rows];
    let march = |c0: usize,
                 band: &mut [(u32, f32)],
                 cf: &dyn Fn(usize, usize) -> u32,
                 lf: &dyn Fn(f64, f64) -> Option<u32>| {
        for (k, strip) in band.chunks_mut(rows).enumerate() {
            m.column((c0 + k) as i32, strip, cf, lf);
        }
    };
    {
        let _p = crate::perf::span("terrain_march");
        match colors {
            Colors::Serial(cf, lf) => march(0, &mut strips, cf, lf),
            Colors::Table(table) => {
                crate::ball::bands(&mut strips, rows, ncols, |c0, band| {
                    march(c0, band, &|x, y| table[y * W + x], &|_, _| None);
                });
            }
        }
    }
    // The sky above the horizon, then the ground over it, row by row.
    let _p = crate::perf::span("terrain_compose");
    let sky_cols = sky.air.map(|(t, l)| (overcast(t), overcast(l)));
    let (fbw, fbh) = (fb.w as i32, fb.h as i32);
    let (y_a, y_b) = (fr.y0.max(0), (fr.y0 + fr.h).min(fbh));
    if y_a < y_b {
        let strips = &strips;
        let rows_n = (y_b - y_a) as usize;
        let w_line = fb.w;
        let start = y_a as usize * w_line;
        let region = &mut fb.px[start..start + rows_n * w_line];
        crate::ball::bands(region, w_line, rows_n, |r0, band| {
            for (k, line) in band.chunks_mut(w_line).enumerate() {
                let yy = y_a + (r0 + k) as i32 - fr.y0;
                let sky_c = sky_cols.map(|(top, low)| {
                    let up = ((horizon - f64::from(yy)) / (fr.focal * 0.9)).clamp(0.0, 1.0) as f32;
                    mix(low, top, up.sqrt())
                });
                for x in 0..fr.w {
                    let sx = fr.x0 + x;
                    if sx < 0 || sx >= fbw {
                        continue;
                    }
                    let p = &mut line[sx as usize];
                    if let Some(c) = sky_c {
                        add_px(p, c);
                    }
                    if x / 2 < cols {
                        let (c, _) = strips[(x / 2) as usize * rows + yy as usize];
                        if c != NONE {
                            add_px(p, c);
                        }
                    }
                }
            }
        });
    }
    let mut depth = Depth {
        cols,
        rows: fr.h,
        d: Vec::with_capacity(strips.len()),
    };
    depth.d.extend(strips.iter().map(|s| s.1));
    depth
}

/// Additive plot of one colour on a pixel (saturating per channel).
#[inline]
fn add_px(p: &mut u32, c: u32) {
    let add = |a: u32, b: u32| (a + b).min(255);
    let r = add((*p >> 16) & 255, (c >> 16) & 255);
    let g = add((*p >> 8) & 255, (c >> 8) & 255);
    let b = add(*p & 255, c & 255);
    *p = (r << 16) | (g << 8) | b;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_sim::scene::new_sim;

    fn world() -> (vector_sim::Sim, usize) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let s = new_sim(5, Arc::new(GameData::load(&dir).expect("data")));
        let i = (0..s.bodies.len())
            .find(|&i| s.landable(i))
            .expect("a solid world");
        (s, i)
    }

    #[test]
    fn rivers_run_along_their_line_lakes_are_flat_and_water_freezes_with_the_season() {
        let (s, i) = world();
        let mut map = s.planet_map(i).unwrap();
        let (x, y) = (60usize, 30usize);
        let k = Surface::idx(x, y);
        // One river of one cell flowing east, no other water but a lake far away.
        let h = &mut map.surface.hydro;
        h.river.iter_mut().for_each(|r| *r = false);
        h.lake.iter_mut().for_each(|l| *l = false);
        h.river[k] = true;
        h.down[k] = 0;
        h.flow[k] = 10.0;
        h.max_flow = 10.0;
        h.lake[Surface::idx(20, 10)] = true;
        let t = Terrain::new(&map, s.bodies[i].radius);
        let on = t.inland_water(Surface::lat(y), Surface::lon(x), 1.0);
        assert!(
            on.is_some_and(|w| w.river),
            "the line runs through its cell"
        );
        let east = t.inland_water(
            Surface::lat(y),
            Surface::lon(x) + 0.5 * std::f64::consts::TAU / W as f64,
            1.0,
        );
        assert!(east.is_some_and(|w| w.river), "and on to the next one");
        assert!(t
            .inland_water(Surface::lat(y + 6), Surface::lon(x), 1.0)
            .is_none());
        // Far away a river is as wide as a pixel, up close it is a few metres.
        let near = Surface::lat(y) + 40.0 / t.radius;
        assert!(
            t.inland_water(near, Surface::lon(x), 1.0).is_none(),
            "40 m off the line is dry"
        );
        assert!(
            t.inland_water(near, Surface::lon(x), 100.0).is_some(),
            "but not seen from afar"
        );
        // The lake is flat and at its own level.
        let lake = t.inland_water(Surface::lat(10), Surface::lon(20), 1.0);
        assert!(lake.is_some_and(|w| !w.river && w.level >= 0.0));
        // Ice follows the temperature of the season.
        let mut cold = map.clone();
        cold.surface.elev[Surface::idx(5, 5)] = -0.2;
        cold.surface.temp[Surface::idx(5, 5)] = 240.0;
        cold.surface.elev[Surface::idx(6, 5)] = -0.2;
        cold.surface.temp[Surface::idx(6, 5)] = 300.0;
        let t = Terrain::new(&cold, s.bodies[i].radius).with_season(0.3);
        assert!(t.is_ice(5, 5) && !t.is_ice(6, 5));
    }

    #[test]
    fn the_ground_is_continuous_deterministic_and_finer_the_closer_you_look() {
        let (s, i) = world();
        let map = s.planet_map(i).unwrap();
        let t = Terrain::new(&map, s.bodies[i].radius);
        let (lat, lon) = (0.3, 1.1);
        assert_eq!(t.sample(lat, lon, 10.0), t.sample(lat, lon, 10.0));
        // Continuous: a step of a metre changes the height by a fraction of a metre at most...
        let (a, _) = t.sample(lat, lon, 1.0);
        let (b, _) = t.sample(lat + 1.0 / t.radius, lon, 1.0);
        assert!((a - b).abs() < 5.0, "{a} vs {b}");
        // ...and fine detail exists only when it is asked for: far away the ground is smooth.
        // Roughness as the second difference (the large slope of the ground drops out).
        let rough = |lod: f64| -> f64 {
            let mut sum = 0.0;
            for k in 0..200 {
                let la = lat + f64::from(k) * 3.0 / t.radius;
                let (p, _) = t.sample(la, lon, lod);
                let (q, _) = t.sample(la + 3.0 / t.radius, lon, lod);
                let (r, _) = t.sample(la + 6.0 / t.radius, lon, lod);
                sum += f64::from((p - 2.0 * q + r).abs());
            }
            sum
        };
        assert!(
            rough(1.0) > rough(5000.0) * 1.3,
            "near ground {} vs far ground {}",
            rough(1.0),
            rough(5000.0)
        );
    }

    #[test]
    fn the_ground_is_painted_from_the_feet_to_the_horizon_with_depth() {
        let (s, i) = world();
        let map = s.planet_map(i).unwrap();
        let t = Terrain::new(&map, s.bodies[i].radius);
        let mut fb = Framebuffer::new(320, 200);
        let eye = Eye {
            lat: 0.2,
            lon: 2.0,
            height: t.sample(0.2, 2.0, 1.0).0.max(0.0) as f64 + 2.0,
            yaw: 0.5,
            pitch: 0.0,
        };
        let fr = Frame {
            x0: 0,
            y0: 0,
            w: 320,
            h: 200,
            focal: 200.0,
        };
        let sky = Sky {
            cloud: 0.0,
            air: Some((0x0040_70C0, 0x00B0_D0F0)),
            haze: 40_000.0,
            scale: 8000.0,
        };
        let light = Light {
            up: 0.7,
            east: 0.5,
            north: 0.5,
        };
        let depth = render(
            &mut fb,
            &t,
            &|_, _| 0x0060_A040,
            &|_, _| None,
            &eye,
            &fr,
            &sky,
            &light,
            200_000.0,
        );
        // The bottom of the screen is the ground at our feet (near), higher up it is far.
        let near = depth.at(80, 195);
        let mid = depth.at(80, 120);
        assert!(near < 200.0, "{near}");
        assert!(mid > near, "{mid} vs {near}");
        // Not everything is sky: the lower half is painted ground.
        let painted = (0..320)
            .filter(|x| fb.px[(190 * 320 + *x) as usize] != 0)
            .count();
        assert!(painted > 300);
    }

    #[test]
    fn the_cached_sample_is_the_same_and_the_parallel_march_paints_the_same() {
        let (s, i) = world();
        let mut map = s.planet_map(i).unwrap();
        // With and without craters (an airless world), the cache changes nothing.
        for airless in [false, true] {
            if airless {
                map.pressure = 0.0;
            }
            let t = Terrain::new(&map, s.bodies[i].radius);
            let mut cache = SampleCache::default();
            let mut d = 1.0f64;
            while d < 400_000.0 {
                let (lat, lon) = (0.3 + d * 0.6 / t.radius, 1.1 + d * 0.8 / t.radius);
                let lod = d / 500.0 * 2.0;
                assert_eq!(
                    t.sample(lat, lon, lod),
                    t.sample_with(lat, lon, lod, &mut cache)
                );
                d += d * 0.03 + 0.5;
            }
        }
        // One thread or many, serial closures or a table of colours: the same picture.
        let t = Terrain::new(&map, s.bodies[i].radius);
        let table: Vec<u32> = (0..W * H)
            .map(|k| 0x0010_0000 * (1 + (k % 7) as u32))
            .collect();
        let eye = Eye {
            lat: 0.2,
            lon: 2.0,
            height: 800.0,
            yaw: 0.5,
            pitch: -0.1,
        };
        let fr = Frame {
            x0: 3,
            y0: 2,
            w: 300,
            h: 180,
            focal: 200.0,
        };
        let sky = Sky {
            cloud: 0.2,
            air: Some((0x0040_70C0, 0x00B0_D0F0)),
            haze: 40_000.0,
            scale: 8000.0,
        };
        let light = Light {
            up: 0.7,
            east: 0.5,
            north: 0.5,
        };
        let mut a = Framebuffer::new(320, 190);
        let da = render(
            &mut a,
            &t,
            &|x, y| table[y * W + x],
            &|_, _| None,
            &eye,
            &fr,
            &sky,
            &light,
            150_000.0,
        );
        for threads in [1, 4] {
            crate::ball::set_local_threads(threads);
            let mut b = Framebuffer::new(320, 190);
            let db = render_par(&mut b, &t, &table, &eye, &fr, &sky, &light, 150_000.0);
            assert!(a.px == b.px, "{threads} threads change the picture");
            assert!(da.d == db.d, "{threads} threads change the depth");
        }
        crate::ball::set_local_threads(0);
    }
}
