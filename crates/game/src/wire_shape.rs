//! Wireframe renderings of the things of the game, drawn turning in the codex: every resource,
//! module, hull, building, species, organ and implant gets a small 3D shape built from lines,
//! chosen by its category (the data says what it is, the shape follows).
use vector_data::GameData;
use vector_render::Framebuffer;

pub type P = [f32; 3];
pub type Seg = (P, P);

#[derive(Default)]
struct Shape(Vec<Seg>);

impl Shape {
    fn line(&mut self, a: P, b: P) {
        self.0.push((a, b));
    }

    /// A box centred at `c` with half sizes `h`.
    fn cuboid(&mut self, c: P, h: P) {
        let v = |sx: f32, sy: f32, sz: f32| [c[0] + sx * h[0], c[1] + sy * h[1], c[2] + sz * h[2]];
        for &(sy, sz) in &[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.line(v(-1.0, sy, sz), v(1.0, sy, sz));
        }
        for &(sx, sz) in &[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.line(v(sx, -1.0, sz), v(sx, 1.0, sz));
        }
        for &(sx, sy) in &[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.line(v(sx, sy, -1.0), v(sx, sy, 1.0));
        }
    }

    /// A cylinder along Y.
    fn cylinder(&mut self, c: P, r: f32, half: f32, n: usize) {
        for k in 0..n {
            let (a0, a1) = (
                std::f32::consts::TAU * k as f32 / n as f32,
                std::f32::consts::TAU * (k + 1) as f32 / n as f32,
            );
            let p = |a: f32, y: f32| [c[0] + r * a.cos(), c[1] + y, c[2] + r * a.sin()];
            self.line(p(a0, half), p(a1, half));
            self.line(p(a0, -half), p(a1, -half));
            if k % 2 == 0 {
                self.line(p(a0, -half), p(a0, half));
            }
        }
    }

    /// A cone along Y: base radius `r` at y = -half, apex at y = +half.
    fn cone(&mut self, c: P, r: f32, half: f32, n: usize) {
        for k in 0..n {
            let (a0, a1) = (
                std::f32::consts::TAU * k as f32 / n as f32,
                std::f32::consts::TAU * (k + 1) as f32 / n as f32,
            );
            let b = |a: f32| [c[0] + r * a.cos(), c[1] - half, c[2] + r * a.sin()];
            self.line(b(a0), b(a1));
            if k % 2 == 0 {
                self.line(b(a0), [c[0], c[1] + half, c[2]]);
            }
        }
    }

    fn ring(&mut self, c: P, r: f32, n: usize) {
        for k in 0..n {
            let (a0, a1) = (
                std::f32::consts::TAU * k as f32 / n as f32,
                std::f32::consts::TAU * (k + 1) as f32 / n as f32,
            );
            self.line(
                [c[0] + r * a0.cos(), c[1], c[2] + r * a0.sin()],
                [c[0] + r * a1.cos(), c[1], c[2] + r * a1.sin()],
            );
        }
    }

    fn sphere(&mut self, c: P, r: f32) {
        // Three great circles and two parallels.
        let n = 16;
        for k in 0..n {
            let (a0, a1) = (
                std::f32::consts::TAU * k as f32 / n as f32,
                std::f32::consts::TAU * (k + 1) as f32 / n as f32,
            );
            self.line(
                [c[0] + r * a0.cos(), c[1] + r * a0.sin(), c[2]],
                [c[0] + r * a1.cos(), c[1] + r * a1.sin(), c[2]],
            );
            self.line(
                [c[0], c[1] + r * a0.cos(), c[2] + r * a0.sin()],
                [c[0], c[1] + r * a1.cos(), c[2] + r * a1.sin()],
            );
        }
        self.ring(c, r, n);
        self.ring([c[0], c[1] + r * 0.6, c[2]], r * 0.8, n);
        self.ring([c[0], c[1] - r * 0.6, c[2]], r * 0.8, n);
    }

    /// An octahedron (a crystal).
    fn crystal(&mut self, c: P, r: f32) {
        let top = [c[0], c[1] + r * 1.2, c[2]];
        let bot = [c[0], c[1] - r * 1.2, c[2]];
        let eq = [
            [c[0] + r, c[1], c[2]],
            [c[0], c[1], c[2] + r],
            [c[0] - r, c[1], c[2]],
            [c[0], c[1], c[2] - r],
        ];
        for k in 0..4 {
            self.line(eq[k], eq[(k + 1) % 4]);
            self.line(top, eq[k]);
            self.line(bot, eq[k]);
        }
    }
}

fn norm(mut s: Shape) -> Vec<Seg> {
    // Fit in the unit sphere.
    let mut r = 1e-6f32;
    for (a, b) in &s.0 {
        for p in [a, b] {
            r = r.max((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt());
        }
    }
    for (a, b) in s.0.iter_mut() {
        for p in [a, b] {
            p[0] /= r;
            p[1] /= r;
            p[2] /= r;
        }
    }
    s.0
}

/// Shape of a resource by its category.
fn resource(category: &str) -> Shape {
    let mut s = Shape::default();
    match category {
        "metal" | "alloy" | "precious" | "material" => {
            s.cuboid([0.0, 0.0, 0.0], [0.9, 0.35, 0.45]);
            s.line([-0.9, 0.35, 0.0], [-0.6, 0.55, 0.0]);
            s.line([0.9, 0.35, 0.0], [0.6, 0.55, 0.0]);
            s.line([-0.6, 0.55, 0.0], [0.6, 0.55, 0.0]);
        }
        "mineral" | "chemical" => s.crystal([0.0, 0.0, 0.0], 0.8),
        "gas" | "volatile" | "fuel" => {
            s.sphere([0.0, 0.0, 0.0], 0.8);
        }
        "nuclear" => {
            s.cylinder([0.0, 0.0, 0.0], 0.5, 0.8, 12);
            s.ring([0.0, 0.0, 0.0], 0.85, 16);
        }
        "organic" | "consumable" | "consumer" | "people" => {
            s.cuboid([0.0, 0.0, 0.0], [0.7, 0.55, 0.7]);
            s.line([-0.7, 0.0, -0.7], [0.7, 0.0, 0.7]);
            s.line([-0.7, 0.0, 0.7], [0.7, 0.0, -0.7]);
        }
        "data" => s.cuboid([0.0, 0.0, 0.0], [0.9, 0.08, 0.6]),
        "artifact" => {
            s.crystal([0.0, 0.0, 0.0], 0.55);
            s.ring([0.0, 0.0, 0.0], 0.95, 20);
        }
        "organes" => {
            s.sphere([-0.2, 0.0, 0.0], 0.55);
            s.sphere([0.35, 0.15, 0.0], 0.4);
        }
        "protheses" => {
            s.cylinder([0.0, 0.5, 0.0], 0.22, 0.45, 8);
            s.sphere([0.0, 0.0, 0.0], 0.22);
            s.cylinder([0.0, -0.5, 0.0], 0.18, 0.45, 8);
        }
        "cybernetique" => {
            s.cuboid([0.0, 0.0, 0.0], [0.55, 0.1, 0.55]);
            for k in -2..=2 {
                let x = k as f32 * 0.25;
                s.line([x, 0.0, 0.55], [x, 0.0, 0.9]);
                s.line([x, 0.0, -0.55], [x, 0.0, -0.9]);
            }
        }
        _ => s.cuboid([0.0, 0.0, 0.0], [0.7, 0.7, 0.7]),
    }
    s
}

/// Shape of a ship module by its category.
fn module(category: &str) -> Shape {
    let mut s = Shape::default();
    match category {
        "engine" | "ftl" | "rcs" => {
            s.cylinder([0.0, 0.5, 0.0], 0.4, 0.4, 10);
            s.cone([0.0, -0.4, 0.0], 0.8, 0.5, 12);
        }
        "reactor" => {
            s.sphere([0.0, 0.0, 0.0], 0.6);
            s.ring([0.0, 0.0, 0.0], 0.95, 20);
        }
        "solar" => {
            for k in 0..3 {
                s.cuboid([-0.6 + k as f32 * 0.6, 0.0, 0.0], [0.28, 0.03, 0.7]);
            }
            s.line([-0.9, 0.0, 0.0], [0.9, 0.0, 0.0]);
        }
        "battery" => {
            for k in 0..3 {
                s.cuboid([0.0, -0.5 + k as f32 * 0.5, 0.0], [0.5, 0.2, 0.4]);
            }
        }
        "tank" => {
            s.cylinder([0.0, 0.0, 0.0], 0.55, 0.8, 12);
            s.ring([0.0, 0.8, 0.0], 0.3, 8);
        }
        "radiator" => {
            for k in 0..4 {
                let z = -0.75 + k as f32 * 0.5;
                s.cuboid([0.0, 0.0, z], [0.8, 0.02, 0.2]);
            }
            s.line([0.0, 0.0, -0.9], [0.0, 0.0, 0.9]);
        }
        "weapon" | "armory" => {
            s.cuboid([-0.5, 0.0, 0.0], [0.4, 0.3, 0.3]);
            s.cylinder([0.0, 0.0, 0.0], 0.1, 0.1, 6);
            s.line([-0.1, 0.0, 0.0], [1.0, 0.0, 0.0]);
            s.line([-0.1, 0.1, 0.0], [1.0, 0.1, 0.0]);
            s.line([-0.1, -0.1, 0.0], [1.0, -0.1, 0.0]);
        }
        "shield" | "defense" => {
            s.sphere([0.0, 0.0, 0.0], 0.45);
            s.ring([0.0, 0.0, 0.0], 0.9, 20);
            s.ring([0.0, 0.3, 0.0], 0.8, 20);
        }
        "armor" => {
            s.cuboid([0.0, 0.0, 0.0], [0.9, 0.7, 0.08]);
            s.line([-0.9, -0.7, 0.08], [0.9, 0.7, 0.08]);
        }
        "scanner" => {
            s.cone([0.0, 0.0, 0.0], 0.8, 0.25, 12);
            s.line([0.0, 0.25, 0.0], [0.0, 0.9, 0.0]);
        }
        "cargo" | "hangar" | "dock" => {
            s.cuboid([0.0, 0.0, 0.0], [0.9, 0.6, 0.6]);
            s.line([0.0, -0.6, 0.6], [0.0, 0.6, 0.6]);
        }
        "habitat" | "life_support" | "medbay" | "greenhouse" | "lab" | "workshop" | "mining"
        | "gyro" => {
            s.cuboid([0.0, 0.0, 0.0], [0.7, 0.5, 0.7]);
            s.cylinder([0.0, 0.7, 0.0], 0.3, 0.2, 8);
        }
        _ => s.cuboid([0.0, 0.0, 0.0], [0.7, 0.7, 0.7]),
    }
    s
}

fn hull(class: &str) -> Shape {
    let mut s = Shape::default();
    let (len, wid) = match class {
        "cargo" | "station" | "colonial" => (1.0, 0.45),
        "combat" | "rapide" => (1.0, 0.25),
        _ => (1.0, 0.33),
    };
    // A long prism with a pointed nose and two fins.
    s.cuboid([-0.1, 0.0, 0.0], [len * 0.7, wid * 0.5, wid * 0.5]);
    s.line([len * 0.6, wid * 0.5, wid * 0.5], [len, 0.0, 0.0]);
    s.line([len * 0.6, -wid * 0.5, wid * 0.5], [len, 0.0, 0.0]);
    s.line([len * 0.6, wid * 0.5, -wid * 0.5], [len, 0.0, 0.0]);
    s.line([len * 0.6, -wid * 0.5, -wid * 0.5], [len, 0.0, 0.0]);
    s.line([-0.5, 0.0, wid * 0.5], [-0.9, 0.0, wid * 1.3]);
    s.line([-0.5, 0.0, -wid * 0.5], [-0.9, 0.0, -wid * 1.3]);
    s.line([-0.9, 0.0, wid * 1.3], [-0.9, 0.0, -wid * 1.3]);
    s
}

fn building(category: &str) -> Shape {
    let mut s = Shape::default();
    match category {
        "housing" => {
            s.sphere([0.0, 0.0, 0.0], 0.8);
            s.cuboid([0.0, -0.8, 0.0], [0.9, 0.05, 0.9]);
        }
        "extraction" => {
            s.cuboid([0.0, -0.6, 0.0], [0.5, 0.2, 0.5]);
            s.line([0.0, -0.4, 0.0], [0.0, 0.9, 0.0]);
            s.line([-0.4, -0.4, 0.0], [0.0, 0.9, 0.0]);
            s.line([0.4, -0.4, 0.0], [0.0, 0.9, 0.0]);
        }
        "industry" | "power" => {
            s.cuboid([-0.2, -0.3, 0.0], [0.8, 0.4, 0.5]);
            s.cylinder([0.5, 0.3, 0.0], 0.15, 0.6, 8);
        }
        "agriculture" => {
            s.cuboid([0.0, 0.0, 0.0], [0.9, 0.25, 0.6]);
            s.ring([0.0, 0.25, 0.0], 0.5, 12);
        }
        "storage" => s.cuboid([0.0, 0.0, 0.0], [0.9, 0.5, 0.7]),
        "spaceport" => {
            s.cuboid([0.0, -0.5, 0.0], [0.95, 0.04, 0.95]);
            s.ring([0.0, -0.46, 0.0], 0.6, 16);
            s.cylinder([0.8, 0.0, 0.8], 0.06, 0.5, 6);
        }
        _ => {
            s.cuboid([0.0, 0.0, 0.0], [0.7, 0.5, 0.7]);
            s.line([-0.7, 0.5, -0.7], [0.0, 0.9, 0.0]);
            s.line([0.7, 0.5, -0.7], [0.0, 0.9, 0.0]);
            s.line([-0.7, 0.5, 0.7], [0.0, 0.9, 0.0]);
            s.line([0.7, 0.5, 0.7], [0.0, 0.9, 0.0]);
        }
    }
    s
}

fn species(producer: bool, body: f64) -> Shape {
    let mut s = Shape::default();
    if producer {
        if body > 2.0 {
            s.cylinder([0.0, -0.3, 0.0], 0.1, 0.6, 6);
            s.sphere([0.0, 0.5, 0.0], 0.45);
        } else {
            for k in 0..5 {
                let a = k as f32 * 1.256;
                s.line([0.0, -0.6, 0.0], [0.45 * a.cos(), 0.5, 0.45 * a.sin()]);
            }
        }
    } else {
        s.cuboid([0.0, 0.1, 0.0], [0.6, 0.25, 0.3]);
        s.sphere([0.75, 0.3, 0.0], 0.2);
        for (x, z) in [(-0.4, -0.2), (-0.4, 0.2), (0.4, -0.2), (0.4, 0.2)] {
            s.line([x, -0.15, z], [x, -0.7, z]);
        }
    }
    s
}

/// The wireframe of entry `k` of codex category `cat`, in the unit sphere.
pub fn shape_for(data: &GameData, cat: usize, k: usize) -> Option<Vec<Seg>> {
    let i = crate::codex::source_index(data, cat, k)?;
    let s = match cat {
        0 => resource(&data.resources.get(i)?.category),
        1 | 2 => module(&data.modules.get(i)?.category),
        3 => hull(&data.hulls.get(i)?.class),
        4 => building(&data.buildings.get(i)?.category),
        12 | 13 => {
            let sp = data.species.get(i)?;
            species(sp.is_producer(), sp.body)
        }
        16 => {
            let mut s = Shape::default();
            s.sphere([0.0, 0.0, 0.0], 0.8);
            s.line([0.0, 0.8, 0.0], [0.2, 1.1, 0.0]);
            s
        }
        17 => resource("cybernetique"),
        _ => return None,
    };
    Some(norm(s))
}

/// Draw `segs` turning with time `t` in the square of side `size` centred at (cx, cy).
pub fn draw(fb: &mut Framebuffer, segs: &[Seg], cx: f32, cy: f32, size: f32, t: f32, col: u32) {
    let (sy, cyw) = (t.sin(), t.cos());
    let tilt = 0.5f32;
    let (st, ct) = (tilt.sin(), tilt.cos());
    let proj = |p: P| -> (f32, f32) {
        // Turn around Y, then tilt around X.
        let x = p[0] * cyw + p[2] * sy;
        let z = -p[0] * sy + p[2] * cyw;
        let y = p[1] * ct - z * st;
        let z2 = p[1] * st + z * ct;
        let persp = 1.0 / (1.0 + 0.25 * z2);
        (cx + x * size * 0.5 * persp, cy - y * size * 0.5 * persp)
    };
    for (a, b) in segs {
        let (x0, y0) = proj(*a);
        let (x1, y1) = proj(*b);
        fb.line(x0, y0, x1, y1, col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        GameData::load(&dir).expect("data loads")
    }

    #[test]
    fn every_drawable_entry_has_a_finite_shape_in_the_unit_sphere() {
        let d = data();
        let n = |cat: usize| crate::codex::count(&d, cat);
        let mut drawn = 0;
        for cat in [0usize, 1, 2, 3, 4, 12, 13, 16, 17] {
            for i in 0..n(cat) {
                let segs = shape_for(&d, cat, i).unwrap_or_else(|| panic!("shape for {cat}/{i}"));
                assert!(segs.len() >= 4, "{cat}/{i} has {} lines", segs.len());
                for (a, b) in &segs {
                    for p in [a, b] {
                        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                        assert!(r.is_finite() && r <= 1.0001, "{cat}/{i} point {p:?}");
                    }
                }
                drawn += 1;
            }
        }
        assert!(drawn > 100, "{drawn}");
        assert!(shape_for(&d, 9, 0).is_none(), "diseases have no shape");
    }

    #[test]
    fn different_kinds_of_things_look_different() {
        let d = data();
        let find = |cat: &str| d.resources.iter().position(|r| r.category == cat).unwrap();
        let metal = shape_for(&d, 0, find("metal")).unwrap();
        let gas = shape_for(&d, 0, find("gas")).unwrap();
        assert_ne!(metal.len(), gas.len(), "an ingot is not a sphere");
        let engine = d
            .modules
            .iter()
            .position(|m| m.category == "engine")
            .unwrap();
        let tank = d.modules.iter().position(|m| m.category == "tank").unwrap();
        assert_ne!(
            shape_for(&d, 1, engine).unwrap().len(),
            shape_for(&d, 1, tank).unwrap().len()
        );
    }
}
