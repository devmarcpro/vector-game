//! Planet surface maps, generated on demand from the body's seed: elevation, mean annual
//! temperature and humidity on an equirectangular grid. Nothing is stored: the same seed always
//! gives the same map. Noise is sampled on the unit sphere, so the map has no seam at the
//! date line and no stretching at the poles.
use crate::PlanetKind;
use vector_core::hash_coord;

pub const W: usize = 128;
pub const H: usize = 64;

/// How a kind of world looks from the ground.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// Fraction of the surface under liquid (or ice) at sea level.
    pub water: f64,
    /// Feature size (octave count) and relief weight.
    pub rough: f64,
    /// Offset in K of the mean temperature (greenhouse for dense atmospheres).
    pub greenhouse: f64,
    /// 0 = no air to carry moisture, 1 = wet.
    pub wet: f64,
}

/// Solid worlds only: giants have no surface.
pub fn profile(kind: PlanetKind) -> Option<Profile> {
    use PlanetKind::*;
    let p = |water, rough, greenhouse, wet| Profile {
        water,
        rough,
        greenhouse,
        wet,
    };
    Some(match kind {
        Barren => p(0.0, 1.0, 0.0, 0.0),
        Dwarf => p(0.0, 0.7, 0.0, 0.0),
        Desert => p(0.06, 0.9, 15.0, 0.25),
        Terran => p(0.62, 1.0, 22.0, 0.9),
        Ocean => p(0.88, 0.6, 25.0, 1.0),
        Tundra => p(0.30, 1.0, 32.0, 0.6),
        Ice => p(0.45, 0.8, 5.0, 0.5),
        Lava => p(0.0, 1.3, 20.0, 0.0),
        Venusian => p(0.0, 0.8, 250.0, 0.1),
        GasGiant | IceGiant | HotJupiter => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Surface {
    /// Elevation relative to sea level in [-1, 1] (negative = sea floor).
    pub elev: Vec<f32>,
    /// Mean annual temperature at sea level reference, K (altitude lapse already applied).
    pub temp: Vec<f32>,
    /// Humidity in [0, 1].
    pub hum: Vec<f32>,
    /// Axial tilt, rad.
    pub tilt: f32,
    /// Fraction of cells below sea level.
    pub sea_frac: f32,
}

impl Surface {
    pub fn idx(x: usize, y: usize) -> usize {
        y * W + x
    }

    /// Latitude of row `y` in radians (+pi/2 at the top row).
    pub fn lat(y: usize) -> f64 {
        (0.5 - (y as f64 + 0.5) / H as f64) * std::f64::consts::PI
    }

    /// Longitude of column `x` in radians, in [0, 2 pi).
    pub fn lon(x: usize) -> f64 {
        (x as f64 + 0.5) / W as f64 * std::f64::consts::TAU
    }

    pub fn is_sea(&self, x: usize, y: usize) -> bool {
        self.elev[Self::idx(x, y)] < 0.0
    }
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in [-1, 1] on a 3D lattice.
fn noise3(seed: u64, x: f64, y: f64, z: f64) -> f64 {
    let (ix, iy, iz) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (smooth(x - ix), smooth(y - iy), smooth(z - iz));
    let corner = |dx: i64, dy: i64, dz: i64| {
        let h = hash_coord(seed, ix as i64 + dx, iy as i64 + dy, iz as i64 + dz);
        (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), fx);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), fx);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), fx);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), fx);
    lerp(lerp(x00, x10, fy), lerp(x01, x11, fy), fz)
}

fn fbm(seed: u64, p: [f64; 3], octaves: u32) -> f64 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp
            * noise3(
                seed ^ (u64::from(o) * 0x9E37),
                p[0] * freq,
                p[1] * freq,
                p[2] * freq,
            );
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Unit-sphere point of a cell.
fn sphere(x: usize, y: usize) -> [f64; 3] {
    let (lat, lon) = (Surface::lat(y), Surface::lon(x));
    [lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()]
}

/// Generate the surface of a body. `seed` identifies the body (system seed mixed with its index),
/// `temp_eq` is its equilibrium temperature in K. None for gas and ice giants.
pub fn generate(seed: u64, kind: PlanetKind, temp_eq: f64) -> Option<Surface> {
    let prof = profile(kind)?;
    let scale = 1.6 + 1.4 * prof.rough;
    // Raw relief: continents (low frequency) plus detail, ridged a little for mountain chains.
    let mut raw = vec![0.0f64; W * H];
    for y in 0..H {
        for x in 0..W {
            let p = sphere(x, y);
            let q = [p[0] * scale, p[1] * scale, p[2] * scale];
            let base = fbm(seed, q, 5);
            let ridge =
                1.0 - fbm(seed ^ 0xA11CE, [q[0] * 2.0, q[1] * 2.0, q[2] * 2.0], 3).abs() * 2.0;
            raw[Surface::idx(x, y)] = base + 0.25 * prof.rough * ridge.max(0.0);
        }
    }
    // Sea level: the quantile that puts `water` of the surface under it (area-weighted by cos lat).
    let mut order: Vec<usize> = (0..W * H).collect();
    order.sort_by(|&a, &b| raw[a].total_cmp(&raw[b]));
    let weight = |i: usize| Surface::lat(i / W).cos();
    let total: f64 = (0..W * H).map(weight).sum();
    let mut acc = 0.0;
    let mut sea = f64::NEG_INFINITY;
    if prof.water > 0.0 {
        for &i in &order {
            acc += weight(i);
            if acc / total >= prof.water {
                sea = raw[i];
                break;
            }
        }
    }
    let (lo, hi) = (raw[order[0]], raw[*order.last().expect("non-empty")]);
    let mut elev = vec![0.0f32; W * H];
    let mut sea_cells = 0usize;
    for i in 0..W * H {
        let e = if sea.is_finite() {
            if raw[i] < sea {
                -((sea - raw[i]) / (sea - lo).max(1e-9))
            } else {
                (raw[i] - sea) / (hi - sea).max(1e-9)
            }
        } else {
            // Dry world: elevation relative to the mean, never below sea level.
            ((raw[i] - lo) / (hi - lo).max(1e-9)).clamp(0.0, 1.0)
        };
        elev[i] = e as f32;
        sea_cells += usize::from(e < 0.0);
    }
    // Moisture: how much water lies within a few cells (box blur of the sea mask), wrapped in
    // longitude, scaled by how wet the world is.
    let mut mask = vec![0.0f32; W * H];
    for i in 0..W * H {
        mask[i] = f32::from(elev[i] < 0.0);
    }
    let r = 6isize;
    let mut hum = vec![0.0f32; W * H];
    for y in 0..H {
        for x in 0..W {
            let (mut s, mut n) = (0.0f32, 0.0f32);
            for dy in -r..=r {
                let yy = y as isize + dy;
                if !(0..H as isize).contains(&yy) {
                    continue;
                }
                for dx in -r..=r {
                    let xx = (x as isize + dx).rem_euclid(W as isize) as usize;
                    s += mask[yy as usize * W + xx];
                    n += 1.0;
                }
            }
            let near = f64::from(s / n);
            let wobble = fbm(
                seed ^ 0x57A7,
                [
                    sphere(x, y)[0] * 3.0,
                    sphere(x, y)[1] * 3.0,
                    sphere(x, y)[2] * 3.0,
                ],
                3,
            );
            let h = (0.15 + 0.85 * near) * prof.wet + 0.18 * wobble * prof.wet.max(0.2);
            hum[Surface::idx(x, y)] = h.clamp(0.0, 1.0) as f32;
        }
    }
    // Mean annual temperature: warm equator, cold poles, cooler with altitude, offset by the
    // greenhouse of the atmosphere (the equilibrium temperature already averages the globe).
    let mut temp = vec![0.0f32; W * H];
    for y in 0..H {
        let c = Surface::lat(y).cos();
        let lat_factor = 0.80 + 0.40 * c * c;
        for x in 0..W {
            let i = Surface::idx(x, y);
            let alt = f64::from(elev[i]).max(0.0);
            let t = (temp_eq * lat_factor + prof.greenhouse - 28.0 * alt).max(2.7);
            temp[i] = t as f32;
        }
    }
    let tilt = (hash_coord(seed, 7, 7, 7) % 10_000) as f64 / 10_000.0 * 0.65 + 0.02;
    Some(Surface {
        elev,
        temp,
        hum,
        tilt: tilt as f32,
        sea_frac: sea_cells as f32 / (W * H) as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_world_and_different_seed_different_world() {
        let a = generate(42, PlanetKind::Terran, 285.0).expect("solid");
        let b = generate(42, PlanetKind::Terran, 285.0).expect("solid");
        let c = generate(43, PlanetKind::Terran, 285.0).expect("solid");
        assert_eq!(a.elev, b.elev);
        assert_eq!(a.temp, b.temp);
        assert_ne!(a.elev, c.elev);
    }

    #[test]
    fn giants_have_no_surface() {
        for k in [
            PlanetKind::GasGiant,
            PlanetKind::IceGiant,
            PlanetKind::HotJupiter,
        ] {
            assert!(generate(1, k, 150.0).is_none());
        }
    }

    #[test]
    fn water_fraction_follows_the_kind_of_world() {
        for (kind, want) in [
            (PlanetKind::Terran, 0.62),
            (PlanetKind::Ocean, 0.88),
            (PlanetKind::Desert, 0.06),
            (PlanetKind::Tundra, 0.30),
        ] {
            let s = generate(7, kind, 280.0).expect("solid");
            // Counted by cells, the target is by area: allow the cos(lat) weighting difference.
            assert!(
                (f64::from(s.sea_frac) - want).abs() < 0.12,
                "{kind:?}: {}",
                s.sea_frac
            );
        }
        for k in [
            PlanetKind::Barren,
            PlanetKind::Lava,
            PlanetKind::Venusian,
            PlanetKind::Dwarf,
        ] {
            let s = generate(7, k, 400.0).expect("solid");
            assert_eq!(s.sea_frac, 0.0, "{k:?} has no sea");
        }
    }

    #[test]
    fn the_map_wraps_in_longitude_and_has_no_cliffs_at_the_seam() {
        let s = generate(99, PlanetKind::Terran, 285.0).expect("solid");
        let mut seam = 0.0f32;
        let mut inner = 0.0f32;
        for y in 0..H {
            seam += (s.elev[Surface::idx(0, y)] - s.elev[Surface::idx(W - 1, y)]).abs();
            inner += (s.elev[Surface::idx(W / 2, y)] - s.elev[Surface::idx(W / 2 - 1, y)]).abs();
        }
        assert!(seam < inner * 3.0 + 1.0, "seam {seam} vs inner {inner}");
        assert!(s.elev.iter().all(|e| (-1.0..=1.0).contains(e)));
    }

    #[test]
    fn poles_are_colder_than_the_equator_and_mountains_colder_than_plains() {
        let s = generate(5, PlanetKind::Terran, 285.0).expect("solid");
        let row = |y: usize| (0..W).map(|x| s.temp[Surface::idx(x, y)]).sum::<f32>() / W as f32;
        assert!(row(0) < row(H / 2) - 15.0, "{} vs {}", row(0), row(H / 2));
        let (mut hi, mut lo) = ((0.0f32, 0u32), (0.0f32, 0u32));
        for y in H / 2 - 2..H / 2 + 2 {
            for x in 0..W {
                let i = Surface::idx(x, y);
                if s.elev[i] > 0.7 {
                    hi = (hi.0 + s.temp[i], hi.1 + 1);
                } else if (0.0..0.15).contains(&s.elev[i]) {
                    lo = (lo.0 + s.temp[i], lo.1 + 1);
                }
            }
        }
        if hi.1 > 0 && lo.1 > 0 {
            assert!(
                hi.0 / (hi.1 as f32) < lo.0 / (lo.1 as f32),
                "altitude cools"
            );
        }
    }

    #[test]
    fn humidity_is_higher_near_the_sea_and_zero_on_airless_worlds() {
        let s = generate(11, PlanetKind::Terran, 285.0).expect("solid");
        let (mut coast, mut inland) = ((0.0f32, 0u32), (0.0f32, 0u32));
        for y in 4..H - 4 {
            for x in 0..W {
                let i = Surface::idx(x, y);
                if s.elev[i] < 0.0 {
                    continue;
                }
                // Distance to the sea along the row, wrapped.
                let near = (1..8).any(|d| {
                    s.elev[Surface::idx((x + d) % W, y)] < 0.0
                        || s.elev[Surface::idx((x + W - d) % W, y)] < 0.0
                });
                if near {
                    coast = (coast.0 + s.hum[i], coast.1 + 1);
                } else {
                    inland = (inland.0 + s.hum[i], inland.1 + 1);
                }
            }
        }
        assert!(coast.1 > 0 && inland.1 > 0);
        assert!(coast.0 / coast.1 as f32 > inland.0 / inland.1 as f32);
        let moon = generate(11, PlanetKind::Barren, 200.0).expect("solid");
        assert!(moon.hum.iter().all(|&h| h < 0.2));
    }
}
