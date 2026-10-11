//! The topographic globe of a world: a dot per cell of its map, a graticule and contour lines of
//! the elevation (marching squares), with the coast in light blue. One drawing for the surface
//! screen (F8) and for the planets seen from the cockpit: the caller says how a point of the globe
//! becomes a point of the screen and which ones face the viewer.
use vector_core::Vec3;
use vector_render::Framebuffer;
use vector_sim::planet::PlanetMap;
use vector_world::surface::{Surface, H, W};

pub struct Globe<'a> {
    pub map: &'a PlanetMap,
    /// (latitude, longitude) in radians to a point of the globe, in the frame of the viewer.
    pub to_view: &'a dyn Fn(f64, f64) -> Vec3,
    /// Whether a point of the globe faces the viewer.
    pub visible: &'a dyn Fn(Vec3) -> bool,
    /// A point of the globe on the screen (None when it cannot be seen).
    pub project: &'a dyn Fn(Vec3) -> Option<(f32, f32)>,
}

/// Colours of the contour lines: ocean depth, the coast, lowland, highland, peaks.
pub const LEVELS: [(f32, u32); 5] = [
    (-0.5, 0x0020_50A0),
    (0.0, 0x0090_E0FF),
    (0.25, 0x0060_C060),
    (0.5, 0x00C0_A050),
    (0.75, 0x00F0_F0F0),
];

impl Globe<'_> {
    /// One dot (`size` pixels wide) for every `step`-th cell facing the viewer; `color` gives
    /// its colour from the cell and its point on the globe.
    pub fn dots(
        &self,
        fb: &mut Framebuffer,
        step: usize,
        size: i32,
        color: &dyn Fn(usize, usize, Vec3) -> u32,
    ) {
        for y in (0..H).step_by(step) {
            for x in (0..W).step_by(step) {
                let p = (self.to_view)(Surface::lat(y), Surface::lon(x));
                if !(self.visible)(p) {
                    continue;
                }
                if let Some((sx, sy)) = (self.project)(p) {
                    let c = color(x, y, p);
                    for dy in 0..size {
                        for dx in 0..size {
                            fb.plot(sx as i32 + dx, sy as i32 + dy, c);
                        }
                    }
                }
            }
        }
    }

    fn seg(&self, fb: &mut Framebuffer, a: Vec3, b: Vec3, c: u32) {
        if !((self.visible)(a) || (self.visible)(b)) {
            return;
        }
        if let (Some(p), Some(q)) = ((self.project)(a), (self.project)(b)) {
            fb.line(p.0, p.1, q.0, q.1, c);
        }
    }

    /// Parallels and meridians every 30 degrees.
    pub fn graticule(&self, fb: &mut Framebuffer, color: u32) {
        for k_lat in -2..=2 {
            let lat = f64::from(k_lat) * 30f64.to_radians();
            for s in 0..72 {
                let (a, b) = (
                    f64::from(s) / 72.0 * std::f64::consts::TAU,
                    f64::from(s + 1) / 72.0 * std::f64::consts::TAU,
                );
                self.seg(fb, (self.to_view)(lat, a), (self.to_view)(lat, b), color);
            }
        }
        for k_lon in 0..12 {
            let lon = f64::from(k_lon) * 30f64.to_radians();
            for s in -30..30 {
                let (a, b) = (
                    f64::from(s) / 60.0 * std::f64::consts::PI,
                    f64::from(s + 1) / 60.0 * std::f64::consts::PI,
                );
                self.seg(fb, (self.to_view)(a, lon), (self.to_view)(b, lon), color);
            }
        }
    }

    /// Contour lines of the elevation, read on a grid of `step` cells.
    pub fn contours(&self, fb: &mut Framebuffer, step: usize) {
        let at = |x: usize, y: usize| self.map.surface.elev[Surface::idx(x % W, y.min(H - 1))];
        for y in (0..H - step).step_by(step) {
            for x in (0..W).step_by(step) {
                let corners = [
                    at(x, y),
                    at(x + step, y),
                    at(x + step, y + step),
                    at(x, y + step),
                ];
                let pos = [(x, y), (x + step, y), (x + step, y + step), (x, y + step)];
                for &(level, color) in &LEVELS {
                    let mut pts: Vec<Vec3> = Vec::new();
                    for e in 0..4 {
                        let (a, b) = (corners[e], corners[(e + 1) % 4]);
                        if (a < level) != (b < level) {
                            let t = f64::from((level - a) / (b - a));
                            let ((ax, ay), (bx, by)) = (pos[e], pos[(e + 1) % 4]);
                            let fx = ax as f64 + (bx as f64 - ax as f64) * t + 0.5;
                            let fy = ay as f64 + (by as f64 - ay as f64) * t + 0.5;
                            let lon = fx / W as f64 * std::f64::consts::TAU;
                            let lat = (0.5 - fy / H as f64) * std::f64::consts::PI;
                            pts.push((self.to_view)(lat, lon));
                        }
                    }
                    for pair in pts.as_chunks::<2>().0 {
                        self.seg(fb, pair[0], pair[1], color);
                    }
                }
            }
        }
    }
}

// ---- the solid world ---------------------------------------------------------------------------

use crate::terrain::Terrain;
use vector_core::Mat3;
use vector_render::Camera;

fn mix_color(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f32;
    let m = |s: u32| (ch(a, s) * (1.0 - t) + ch(b, s) * t).clamp(0.0, 255.0) as u32;
    (m(16) << 16) | (m(8) << 8) | m(0)
}

fn scale_color(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// What to paint on a world: its map and the colour of a cell.
pub struct Paint<'a> {
    /// The relief of the world (a gas giant has none: bands only).
    pub map: Option<&'a PlanetMap>,
    pub color: &'a dyn Fn(usize, usize) -> u32,
    /// Air pressure (bar): above a trace, a bright rim of atmosphere is drawn.
    pub air: f64,
}

/// Elevation of the map at a fractional cell, bilinear, wrapping in longitude.
pub(crate) fn elev_at(map: &PlanetMap, fx: f64, fy: f64) -> f32 {
    let x0 = fx.floor();
    let y0 = fy.floor();
    let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
    let at = |x: f64, y: f64| {
        let xi = (x as i64).rem_euclid(W as i64) as usize;
        let yi = (y as i64).clamp(0, H as i64 - 1) as usize;
        map.surface.elev[Surface::idx(xi, yi)]
    };
    let a = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
    let b = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
    a + (b - a) * ty
}

/// Smooth value noise in [-1, 1] on a grid of hashed values.
fn noise2(x: f64, y: f64) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = ((x - ix) as f32, (y - iy) as f32);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let v = |dx: i64, dy: i64| {
        let h = vector_core::hash_coord(11, ix as i64 + dx, iy as i64 + dy, 9);
        ((h >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
    };
    let a = v(0, 0) + (v(1, 0) - v(0, 0)) * fx;
    let b = v(0, 1) + (v(1, 1) - v(0, 1)) * fx;
    a + (b - a) * fy
}

/// Paint a solid world as a lit ball: the colour of each cell, sunlight and a terminator, relief
/// shaded from the slope of the ground, a little grain when it is close, a rim of air. Each
/// pixel casts a ray at the sphere (in blocks when the world fills the screen).
pub fn fill_world(
    fb: &mut Framebuffer,
    cam: &Camera,
    center: Vec3,
    radius: f64,
    orient: &Mat3,
    sun: Vec3,
    paint: &Paint,
) {
    let oc = cam.pos - center;
    let dist = oc.length();
    if dist <= radius * 1.001 {
        return;
    }
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
    let (cx, cy) = (fb.w as f64 * 0.5, fb.h as f64 * 0.5);
    let view_c = cam.to_view(center);
    if view_c.z <= 0.0 {
        return;
    }
    let (sx, sy) = (
        cx + view_c.x / view_c.z * focal,
        cy - view_c.y / view_c.z * focal,
    );
    let r_px = radius / (dist * dist - radius * radius).sqrt() * focal;
    let half = (r_px * 1.08) as i32 + 2;
    let step = ((r_px / 170.0).ceil() as i32).max(1);
    let (x_lo, x_hi) = (
        (sx as i32 - half).max(0),
        (sx as i32 + half).min(fb.w as i32 - 1),
    );
    let (y_lo, y_hi) = (
        (sy as i32 - half).max(0),
        (sy as i32 + half).min(fb.h as i32 - 1),
    );
    let to_world = cam.rot.transpose();
    let inv_orient = orient.transpose();
    let sun_b = inv_orient.mul_vec(sun);
    let atmosphere = paint.air > 0.02;
    let terrain = paint.map.map(|m| Terrain::new(m, radius));
    let mut y = y_lo;
    while y <= y_hi {
        let mut x = x_lo;
        while x <= x_hi {
            let px = f64::from(x) + f64::from(step) / 2.0;
            let py = f64::from(y) + f64::from(step) / 2.0;
            let d = to_world
                .mul_vec(Vec3::new((px - cx) / focal, (cy - py) / focal, 1.0))
                .normalized();
            let b = oc.dot(d);
            let disc = b * b - (oc.dot(oc) - radius * radius);
            let color = if disc >= 0.0 {
                let t = -b - disc.sqrt();
                let n = (oc + d * t) * (1.0 / radius);
                let nb = inv_orient.mul_vec(n);
                let lat = nb.y.clamp(-1.0, 1.0).asin();
                let lon = nb.z.atan2(nb.x).rem_euclid(std::f64::consts::TAU);
                let fx = lon / std::f64::consts::TAU * W as f64 - 0.5;
                let fy = (0.5 - lat / std::f64::consts::PI) * H as f64 - 0.5;
                // Borders between biomes are not squares: the lookup is warped by noise.
                let warp = (
                    noise2(fx * 2.3, fy * 2.3) * 0.9,
                    noise2(fx * 2.3 + 40.0, fy * 2.3) * 0.9,
                );
                let (wxi, wyi) = (
                    ((fx + f64::from(warp.0)).round() as i64).rem_euclid(W as i64) as usize,
                    ((fy + f64::from(warp.1)).round() as i64).clamp(0, H as i64 - 1) as usize,
                );
                let mut base = (paint.color)(wxi, wyi);
                let mut shade = 1.0f32;
                if let Some(terr) = &terrain {
                    // The same ground as seen from low: the height at this point, to the
                    // resolution of a pixel, and its slope against the light.
                    let px_m = radius / r_px.max(1.0) * f64::from(step);
                    let (hc, e) = terr.sample(lat, lon, px_m);
                    let delta = px_m * 1.2 / radius;
                    let (he, _) = terr.sample(lat, lon + delta / lat.cos().abs().max(0.05), px_m);
                    let (hn, _) = terr.sample(lat + delta, lon, px_m);
                    let slope = (
                        f64::from(he - hc) / (px_m * 1.2),
                        f64::from(hn - hc) / (px_m * 1.2),
                    );
                    let east = Vec3::new(-lon.sin(), 0.0, lon.cos());
                    let north =
                        Vec3::new(-lat.sin() * lon.cos(), lat.cos(), -lat.sin() * lon.sin());
                    let toward = (sun_b.dot(east) * slope.0 + sun_b.dot(north) * slope.1) as f32;
                    // Seas and lakes follow the detailed height, not the cell.
                    if hc <= 0.0 && e > 0.0 && paint.air > 0.05 {
                        base = 0x0020_60A0;
                    } else if hc > 0.0 && e <= 0.0 && paint.air > 0.05 {
                        base = scale_color(0x00C8_B880, 0.9);
                    }
                    if hc > 0.0 {
                        shade += (toward * 9.0).clamp(-0.6, 0.6);
                        shade += (hc as f32 / terr.relief as f32).clamp(0.0, 1.0) * 0.3;
                        // Variation of the ground from place to place.
                        shade *=
                            1.0 + 0.12 * noise2(lon * radius / 30_000.0, lat * radius / 30_000.0);
                    } else {
                        shade *= 1.0 + 0.06 * e.clamp(-1.0, 0.0);
                    }
                }
                let light = n.dot(sun).max(-0.2);
                let lit = ((light + 0.08) / 0.25).clamp(0.0, 1.0) as f32;
                let day = 0.10 + 0.90 * (light.max(0.0) as f32).powf(0.8);
                let k = if lit >= 1.0 {
                    day
                } else {
                    0.10 + (day - 0.10) * lit
                };
                Some(scale_color(base, (k * shade).max(0.05)))
            } else if atmosphere {
                // The air above the limb: a thin bright rim on the lit side.
                let closest = (oc - d * b).length();
                let edge = (closest / radius - 1.0) / 0.045;
                if edge < 1.0 && b < 0.0 {
                    let p = (oc + d * (-b)) * (1.0 / closest);
                    let lit = (p.dot(sun) as f32 * 0.7 + 0.5).clamp(0.0, 1.0);
                    let a = (1.0 - edge as f32).powi(2) * lit * (paint.air as f32).min(1.0).sqrt();
                    let at = (fb.w as i32 * y.min(fb.h as i32 - 1) + x) as usize;
                    let under = fb.px.get(at).copied().unwrap_or(0);
                    Some(mix_color(under, 0x0070_B0FF, a))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(c) = color {
                for dy in 0..step {
                    for dx in 0..step {
                        fb.plot(x + dx, y + dy, c);
                    }
                }
            }
            x += step;
        }
        y += step;
    }
}
