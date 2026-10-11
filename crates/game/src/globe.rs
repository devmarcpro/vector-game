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
