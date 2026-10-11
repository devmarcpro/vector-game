//! The ground of a world at every scale. One height function (the map of the world, 400 km per
//! cell, plus octaves of noise down to a few metres) and one renderer (a heightfield marched
//! column by column, the steps growing with the distance) serve the cockpit when a ship flies low
//! over a world and the first-person view of an expedition on foot: the nearer the ground, the
//! finer the detail, and from orbit down to the soles of the squad it is the same ground.
use crate::globe::elev_at;
use vector_render::Framebuffer;
use vector_sim::planet::PlanetMap;
use vector_world::surface::{H, W};

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
}

fn noise2(x: f64, y: f64) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
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

impl<'a> Terrain<'a> {
    pub fn new(map: &'a PlanetMap, radius: f64) -> Self {
        Terrain {
            map,
            radius,
            relief: (radius * 0.0007).clamp(1500.0, 9000.0),
            craters: map.pressure < 0.02,
            seed: map.seed,
        }
    }

    /// The cell of the map a point belongs to.
    pub fn cell(lat: f64, lon: f64) -> (usize, usize) {
        let lon = lon.rem_euclid(std::f64::consts::TAU);
        (
            ((lon / std::f64::consts::TAU * W as f64) as usize).min(W - 1),
            (((0.5 - lat / std::f64::consts::PI) * H as f64).max(0.0) as usize).min(H - 1),
        )
    }

    /// Height above the sea at a point (m; the sea itself is 0) and the elevation of the map
    /// there (negative under the sea), with the octaves of detail the distance `lod` (the size
    /// of what can still be told apart, m) deserves.
    pub fn sample(&self, lat: f64, lon: f64, lod: f64) -> (f32, f32) {
        let fx = lon.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU * W as f64 - 0.5;
        let fy = (0.5 - lat / std::f64::consts::PI) * H as f64 - 0.5;
        let e = elev_at(self.map, fx, fy);
        let base = f64::from(e) * self.relief;
        let rugged = 0.3 + 0.7 * f64::from(e.clamp(0.0, 1.0));
        let (x, y) = (lon * self.radius, lat * self.radius);
        let mut h = base;
        let mut lambda = LAMBDA0;
        let mut amp = 0.22 * self.relief * rugged;
        for _ in 0..OCTAVES {
            if lambda < lod * 2.5 {
                break;
            }
            h += f64::from(noise2(x / lambda, y / lambda)) * amp;
            lambda /= RATIO;
            amp *= 0.45;
        }
        if self.craters {
            h += self.crater_height(x, y, lod);
        }
        (h as f32, e)
    }

    /// Height of the craters at a point (m, negative in the bowls, positive on the rims): three
    /// sizes of them, each on a grid with at most one crater per square.
    fn crater_height(&self, x: f64, y: f64, lod: f64) -> f64 {
        let mut sum = 0.0;
        for (cell, depth) in [
            (900_000.0, 0.16),
            (220_000.0, 0.14),
            (50_000.0, 0.12),
            (11_000.0, 0.10),
        ] {
            // A crater smaller than what can be told apart at this distance is not drawn.
            if cell * 0.3 < lod * 2.0 {
                continue;
            }
            let (ix, iy) = ((x / cell).floor(), (y / cell).floor());
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let (gx, gy) = (ix as i64 + dx, iy as i64 + dy);
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
                    let (cx, cy) = (
                        (gx as f64 + 0.2 + 0.6 * h(1)) * cell,
                        (gy as f64 + 0.2 + 0.6 * h(2)) * cell,
                    );
                    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r;
                    let dp = depth * r * 0.4;
                    sum += if d < 1.0 {
                        -dp * (1.0 - d * d)
                    } else if d < 1.5 {
                        0.35 * dp * (1.0 - (d - 1.0) / 0.5).powi(2)
                    } else {
                        0.0
                    };
                }
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

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f32;
    let m = |s: u32| (ch(a, s) * (1.0 - t) + ch(b, s) * t).clamp(0.0, 255.0) as u32;
    (m(16) << 16) | (m(8) << 8) | m(0)
}

fn scale(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
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

/// March the ground from the nearest to the farthest and paint it. `color_of` gives the colour
/// of a cell of the map; the steps grow with the distance, so that the detail and the cost fall
/// with it.
#[allow(clippy::too_many_arguments)]
pub fn render(
    fb: &mut Framebuffer,
    t: &Terrain,
    color_of: &dyn Fn(usize, usize) -> u32,
    eye: &Eye,
    fr: &Frame,
    sky: &Sky,
    light: &Light,
    far: f64,
) -> Depth {
    let (cx, cy) = (f64::from(fr.w) / 2.0, f64::from(fr.h) / 2.0);
    let horizon = cy + fr.focal * eye.pitch.tan().clamp(-1.4, 1.4);
    let cols = fr.w / 2;
    let mut depth = Depth {
        cols,
        rows: fr.h,
        d: vec![f32::MAX; (cols * fr.h) as usize],
    };
    let horizon_color = sky.air.map_or(0, |a| a.1);
    // The sky above the horizon.
    if let Some((top, low)) = sky.air {
        for y in 0..fr.h {
            let up = ((horizon - f64::from(y)) / (fr.focal * 0.9)).clamp(0.0, 1.0) as f32;
            let c = mix(low, top, up.sqrt());
            for x in 0..fr.w {
                fb.plot(fr.x0 + x, fr.y0 + y, c);
            }
        }
    }
    let cos_lat = eye.lat.cos().abs().max(0.05);
    for col in 0..cols {
        let theta = ((f64::from(col) * 2.0 + 1.0 - cx) / fr.focal).atan();
        let yaw = eye.yaw + theta;
        let (de, dn) = (yaw.sin(), yaw.cos());
        let cos_t = theta.cos();
        // Slope against the light is read along the ray.
        let along = de * light.east + dn * light.north;
        let mut ymin = fr.h;
        let mut d = 1.0f64;
        let mut prev: Option<(f64, f32)> = None;
        while d < far && ymin > 0 {
            let lat = eye.lat + dn * d / t.radius;
            let lon = eye.lon + de * d / (t.radius * cos_lat);
            let (h_raw, e) = t.sample(lat, lon, d / fr.focal * 2.0);
            let sea = e <= 0.0 && h_raw <= 0.0;
            let h = if sea { 0.0 } else { f64::from(h_raw) };
            let drop = d * d / (2.0 * t.radius);
            let dz = h - eye.height - drop;
            let ytop = (horizon - fr.focal * dz / (d * cos_t)).floor() as i32;
            if ytop < ymin {
                let (ci, ri) = Terrain::cell(lat, lon);
                let mut base = color_of(ci, ri);
                // Variation from place to place, snow and bare rock on the heights, water.
                let (wx, wy) = (lon * t.radius, lat * t.radius);
                let near = 1.0 / (1.0 + d / 600.0) as f32;
                let var = 1.0
                    + 0.10 * noise2(wx / 400.0, wy / 400.0)
                    + near
                        * (0.10 * noise2(wx / 3.0, wy / 3.0) + 0.07 * noise2(wx / 30.0, wy / 30.0));
                let k = if sea {
                    base = mix(
                        0x0018_4A8C,
                        0x0030_90B0,
                        (1.0 - (-e * 3.0).exp()).clamp(0.0, 0.7),
                    );
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
                    color = mix(color, horizon_color, fog);
                }
                for yy in ytop.max(0)..ymin {
                    let (x, y) = (fr.x0 + col * 2, fr.y0 + yy);
                    fb.plot(x, y, color);
                    fb.plot(x + 1, y, color);
                    depth.d[(col * fr.h + yy) as usize] = d as f32;
                }
                ymin = ytop.max(0);
            }
            prev = Some((d, h as f32));
            d += d * 0.03 + 0.5;
        }
    }
    depth
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
}
