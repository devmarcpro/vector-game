//! Seasons and clouds of a solid world. The Sun's declination follows from the axial tilt and the
//! position in the year; the temperature of a cell follows from the sunlight its latitude gets
//! (so a pole is hot in its summer and dark in its winter) with a delay and a damping that the
//! humid air gives; the clouds are made where the air is humid, warm and converging, carried by
//! the winds of the world (which follow its spin and the temperature contrast between equator
//! and poles), and there are none where there is no air.
use std::f64::consts::{PI, TAU};

use crate::surface::{fbm, Surface, H, W};

/// Temperature swing, in K, for one unit of daily sunlight (the solar constant at the top of the
/// air): a pole of a world tilted by 0.4 rad swings by roughly 22 K.
const SEASON_K: f64 = 80.0;
/// Freezing point of sea water, K (a little under the fresh water one).
pub const SEA_FREEZE: f64 = 271.4;
/// Freezing point of fresh water, K.
pub const FRESH_FREEZE: f64 = 273.15;

/// Declination of the Sun (latitude of the point where it is overhead at noon), rad. `phase` is the
/// position in the year, 0..1: 0 and 0.5 are the equinoxes, 0.25 northern midsummer.
pub fn declination(tilt: f64, phase: f64) -> f64 {
    (tilt.sin() * (TAU * phase).sin()).clamp(-1.0, 1.0).asin()
}

/// Sunlight a latitude gets in a day, averaged over the day, as a share of the solar constant
/// facing the Sun (the classic daily-mean insolation: polar day and night included).
pub fn daily_insolation(lat: f64, decl: f64) -> f64 {
    let a = -lat.tan() * decl.tan();
    let h0 = if a >= 1.0 {
        0.0
    } else if a <= -1.0 {
        PI
    } else {
        a.acos()
    };
    (h0 * lat.sin() * decl.sin() + lat.cos() * decl.cos() * h0.sin()) / PI
}

/// Mean over the year of the sunlight at a latitude.
pub fn mean_insolation(lat: f64, tilt: f64) -> f64 {
    (0..8)
        .map(|k| daily_insolation(lat, declination(tilt, f64::from(k) / 8.0)))
        .sum::<f64>()
        / 8.0
}

/// Temperature of a cell at a point of the year: its annual mean plus the swing the sunlight of
/// its latitude gives, late by a fraction of a year (the more humid, the later and the smaller
/// the swing: a maritime climate).
pub fn seasonal_temp(s: &Surface, x: usize, y: usize, phase: f64) -> f64 {
    let qmean = mean_insolation(Surface::lat(y), f64::from(s.tilt));
    temp_with(s, x, y, phase, qmean)
}

fn temp_with(s: &Surface, x: usize, y: usize, phase: f64, qmean: f64) -> f64 {
    let i = Surface::idx(x, y);
    let hum = f64::from(s.hum[i]);
    let lag = 0.04 + 0.08 * hum;
    let q = daily_insolation(Surface::lat(y), declination(f64::from(s.tilt), phase - lag));
    f64::from(s.temp[i]) + SEASON_K * (q - qmean) * (1.0 - 0.5 * hum)
}

/// The temperature of every cell at a point of the year (the same figures as `seasonal_temp`,
/// the yearly mean of each latitude computed once per row).
pub fn season_map(s: &Surface, phase: f64) -> Vec<f32> {
    let mut out = vec![0.0f32; W * H];
    for y in 0..H {
        let qmean = mean_insolation(Surface::lat(y), f64::from(s.tilt));
        for x in 0..W {
            out[Surface::idx(x, y)] = temp_with(s, x, y, phase, qmean) as f32;
        }
    }
    out
}

/// Is the water of this cell frozen at this temperature? Sea water freezes a little colder than
/// the water of rivers and lakes.
pub fn frozen(temp: f64, sea: bool) -> bool {
    temp < if sea { SEA_FREEZE } else { FRESH_FREEZE }
}

/// What the clouds need to be known about a world at a moment.
#[derive(Clone, Copy, Debug)]
pub struct Atmo {
    /// Surface pressure, bar.
    pub pressure: f64,
    /// Position in the year, 0..1.
    pub phase: f64,
    /// Days since the start of the game.
    pub days: f64,
    /// Strength of the winds, cells per day at their fastest: the faster the spin and the bigger
    /// the temperature contrast between equator and poles, the stronger.
    pub wind: f64,
}

impl Atmo {
    pub fn new(s: &Surface, pressure: f64, spin_period: f64, phase: f64, days: f64) -> Atmo {
        let row = |y: usize| {
            (0..W)
                .map(|x| f64::from(s.temp[Surface::idx(x, y)]))
                .sum::<f64>()
                / W as f64
        };
        let contrast = (row(H / 2) - (row(0) + row(H - 1)) / 2.0).max(0.0);
        let spin = (86_400.0 / spin_period.max(600.0)).clamp(0.1, 4.0).sqrt();
        let wind = 2.5 * spin * (contrast / 50.0).clamp(0.3, 2.0);
        Atmo {
            pressure,
            phase,
            days,
            wind,
        }
    }
}

/// East-west wind at a latitude, in cells per day (positive eastward): trade winds blowing west
/// in the tropics, westerlies in the mid-latitudes, easterlies again near the poles.
pub fn zonal_wind(a: &Atmo, lat: f64) -> f64 {
    -(3.0 * lat).cos() * a.wind
}

/// Humidity at a point (bilinear, wrapped in longitude).
fn hum_at(s: &Surface, fx: f64, fy: f64) -> f64 {
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let at = |x: f64, y: f64| {
        let xi = (x as i64).rem_euclid(W as i64) as usize;
        let yi = (y as i64).clamp(0, H as i64 - 1) as usize;
        f64::from(s.hum[Surface::idx(xi, yi)])
    };
    let a = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
    let b = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
    a + (b - a) * ty
}

/// Days a pattern of clouds lives before it fades into the next one.
const CLOUD_LIFE: f64 = 24.0;

/// Cloud cover (0 clear sky, 1 overcast) above latitude `lat`, longitude `lon` (rad). Humid, warm
/// air makes clouds, and so does the line where the winds of the two hemispheres meet (it follows
/// the Sun through the year); a thick atmosphere is overcast; a world with no air has none. The
/// pattern is carried east or west by the wind of its latitude: two layers, each shifted by the
/// wind since its birth, fade into each other so that the shear never stretches them for ever.
pub fn cloud_cover(s: &Surface, a: &Atmo, lat: f64, lon: f64) -> f32 {
    if a.pressure < 0.01 {
        return 0.0;
    }
    let lon = lon.rem_euclid(TAU);
    let fx = lon / TAU * W as f64 - 0.5;
    let fy = (0.5 - lat / PI) * H as f64 - 0.5;
    let m = hum_at(s, fx, fy);
    let (cx, cy) = (
        (fx.round() as i64).rem_euclid(W as i64) as usize,
        (fy.round() as i64).clamp(0, H as i64 - 1) as usize,
    );
    let temp = seasonal_temp(s, cx, cy, a.phase);
    let warm = ((temp - 235.0) / 60.0).clamp(0.0, 1.0);
    let decl = declination(f64::from(s.tilt), a.phase);
    let itcz = (-((lat - 0.8 * decl) / 0.22).powi(2)).exp();
    let drift = zonal_wind(a, lat) * TAU / W as f64;
    let mut pat = 0.0;
    for k in 0..2 {
        let age = a.days / CLOUD_LIFE + 0.5 * f64::from(k);
        let t = age.fract();
        let weight = 1.0 - (2.0 * t - 1.0).abs();
        let gen = age.floor() as i64;
        let l = lon - drift * t * CLOUD_LIFE;
        let p = [lat.cos() * l.cos(), lat.sin(), lat.cos() * l.sin()];
        let seed = s.seed ^ (gen as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xC10D;
        pat += weight * fbm(seed, [p[0] * 3.2, p[1] * 3.2, p[2] * 3.2], 4);
    }
    let thick = (a.pressure / 8.0).clamp(0.0, 1.0);
    let raw =
        0.95 * m * (0.35 + 0.65 * warm) + 0.5 * pat + 0.3 * itcz * (0.4 + 0.6 * m) + 0.8 * thick
            - 0.3;
    let t = (raw / 0.6).clamp(0.0, 1.0);
    let air = 1.0 - (-a.pressure * 3.0).exp();
    (t * t * (3.0 - 2.0 * t) * air) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{surface, PlanetKind};

    fn terran() -> Surface {
        surface::generate(5, PlanetKind::Terran, 285.0).expect("solid")
    }

    #[test]
    fn the_sun_goes_as_far_as_the_tilt_and_comes_back() {
        let tilt = 0.4;
        assert!(declination(tilt, 0.0).abs() < 1e-9);
        assert!((declination(tilt, 0.25) - tilt).abs() < 1e-9);
        assert!((declination(tilt, 0.75) + tilt).abs() < 1e-9);
        assert!(declination(0.0, 0.25).abs() < 1e-9, "no tilt, no seasons");
    }

    #[test]
    fn the_pole_gets_light_all_summer_and_none_in_winter() {
        let pole = 89.0f64.to_radians();
        assert!(daily_insolation(pole, 0.4) > 0.3);
        assert_eq!(daily_insolation(pole, -0.4), 0.0);
        // At the equator the day is the same all year, give or take cos(declination).
        let eq = (daily_insolation(0.0, 0.4) - daily_insolation(0.0, 0.0)).abs();
        assert!(eq < 0.05);
    }

    #[test]
    fn a_tilted_pole_is_warmer_in_its_summer_than_in_its_winter() {
        let mut s = terran();
        s.tilt = 0.5;
        let y = 3; // far north
        let summer = seasonal_temp(&s, 10, y, 0.30);
        let winter = seasonal_temp(&s, 10, y, 0.80);
        assert!(summer > winter + 15.0, "{summer} vs {winter}");
        // The other hemisphere is the other way round.
        let ys = H - 4;
        assert!(seasonal_temp(&s, 10, ys, 0.30) < seasonal_temp(&s, 10, ys, 0.80) - 15.0);
        // An upright world has no seasons.
        s.tilt = 0.0;
        assert!((seasonal_temp(&s, 10, y, 0.30) - seasonal_temp(&s, 10, y, 0.80)).abs() < 1e-6);
    }

    #[test]
    fn the_year_averages_out_to_the_annual_mean() {
        let s = terran();
        for y in [3, H / 3, H / 2, H - 5] {
            let mean: f64 = (0..48)
                .map(|k| seasonal_temp(&s, 7, y, f64::from(k) / 48.0))
                .sum::<f64>()
                / 48.0;
            let want = f64::from(s.temp[Surface::idx(7, y)]);
            assert!((mean - want).abs() < 2.5, "row {y}: {mean} vs {want}");
        }
    }

    #[test]
    fn the_season_map_is_the_cell_by_cell_temperature() {
        let s = terran();
        let m = season_map(&s, 0.37);
        for (x, y) in [(0, 0), (50, 20), (100, 63), (7, 32)] {
            assert!(
                (f64::from(m[Surface::idx(x, y)]) - seasonal_temp(&s, x, y, 0.37)).abs() < 0.01
            );
        }
    }

    #[test]
    fn humid_air_damps_and_delays_the_seasons() {
        let mut s = terran();
        s.tilt = 0.5;
        let i = Surface::idx(10, 6);
        let swing = |s: &Surface| {
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for k in 0..40 {
                let t = seasonal_temp(s, 10, 6, f64::from(k) / 40.0);
                lo = lo.min(t);
                hi = hi.max(t);
            }
            hi - lo
        };
        s.hum[i] = 0.0;
        let dry = swing(&s);
        s.hum[i] = 1.0;
        assert!(swing(&s) < dry * 0.8);
    }

    #[test]
    fn sea_freezes_colder_than_a_lake() {
        assert!(frozen(272.0, false));
        assert!(!frozen(272.0, true));
        assert!(frozen(270.0, true));
    }

    fn atmo(s: &Surface, pressure: f64, days: f64) -> Atmo {
        Atmo::new(s, pressure, 86_400.0, 0.1, days)
    }

    #[test]
    fn there_are_no_clouds_without_air() {
        let s = terran();
        for y in (0..H).step_by(5) {
            for x in (0..W).step_by(7) {
                let c = cloud_cover(&s, &atmo(&s, 0.0, 3.0), Surface::lat(y), Surface::lon(x));
                assert_eq!(c, 0.0);
            }
        }
        // And with air there are some, but not everywhere.
        let (mut n, mut tot) = (0.0f32, 0u32);
        let mut clear = 0u32;
        for y in 0..H {
            for x in 0..W {
                let c = cloud_cover(&s, &atmo(&s, 1.0, 3.0), Surface::lat(y), Surface::lon(x));
                assert!((0.0..=1.0).contains(&c));
                n += c;
                tot += 1;
                clear += u32::from(c < 0.05);
            }
        }
        let mean = n / tot as f32;
        assert!((0.1..0.85).contains(&mean), "mean cover {mean}");
        assert!(clear > 100, "some sky is clear");
    }

    #[test]
    fn wet_places_are_cloudier_than_dry_ones_and_thick_air_is_overcast() {
        let s = terran();
        let a = atmo(&s, 1.0, 10.0);
        let (mut wet, mut dry) = ((0.0f32, 0u32), (0.0f32, 0u32));
        for y in 8..H - 8 {
            for x in 0..W {
                let c = cloud_cover(&s, &a, Surface::lat(y), Surface::lon(x));
                if s.hum[Surface::idx(x, y)] > 0.6 {
                    wet = (wet.0 + c, wet.1 + 1);
                } else if s.hum[Surface::idx(x, y)] < 0.3 {
                    dry = (dry.0 + c, dry.1 + 1);
                }
            }
        }
        assert!(wet.1 > 0 && dry.1 > 0);
        assert!(wet.0 / wet.1 as f32 > dry.0 / dry.1 as f32);
        let thick = atmo(&s, 90.0, 10.0);
        let c = cloud_cover(&s, &thick, 0.3, 1.0);
        assert!(c > 0.9, "a thick atmosphere is overcast: {c}");
    }

    #[test]
    fn clouds_are_carried_by_the_wind_and_the_wind_follows_the_spin() {
        let s = terran();
        let at = |days: f64| {
            (0..W)
                .map(|x| cloud_cover(&s, &atmo(&s, 1.0, days), 0.2, Surface::lon(x)))
                .collect::<Vec<_>>()
        };
        let (a, b) = (at(2.0), at(2.5));
        assert_ne!(a, b, "the sky moves");
        assert_eq!(a, at(2.0), "and is deterministic");
        // Trades blow west near the equator, westerlies east at mid latitudes.
        let w = atmo(&s, 1.0, 0.0);
        assert!(zonal_wind(&w, 0.0) < 0.0 && zonal_wind(&w, 0.8) > 0.0);
        // A slow world has weaker winds than a fast one.
        let slow = Atmo::new(&s, 1.0, 86_400.0 * 40.0, 0.0, 0.0);
        let fast = Atmo::new(&s, 1.0, 6.0 * 3600.0, 0.0, 0.0);
        assert!(slow.wind < fast.wind);
    }

    #[test]
    fn the_cloud_belt_follows_the_sun_through_the_year() {
        let mut s = terran();
        s.tilt = 0.5;
        // Mean cover of a latitude band, the same sky pattern but another season.
        let band = |lat0: f64, phase: f64| {
            let a = Atmo {
                phase,
                ..atmo(&s, 1.0, 5.0)
            };
            (0..W)
                .map(|x| f64::from(cloud_cover(&s, &a, lat0, Surface::lon(x))))
                .sum::<f64>()
        };
        let north = 0.35;
        assert!(
            band(north, 0.25) > band(north, 0.75),
            "the belt goes north in northern summer"
        );
    }
}
