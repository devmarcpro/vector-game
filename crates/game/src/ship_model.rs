//! Solid model of a ship: the real hull of a design with every installed module drawn by its own
//! procedural shape, lit by the star, and a small z-buffered triangle rasteriser to paint it.
//! One builder serves the third-person view of the player's ship, the NPC ships, the shipyard
//! screen and the codex: the model is a pure function of (data, design, state), so it is
//! deterministic and changes whenever a module, a health value, the thrust or a deployment does.
use vector_core::{hash_coord, Mat3, Vec3};
use vector_data::{GameData, Module};
use vector_render::{mix_color as mix_col, scale_color as scale_col, Camera, Framebuffer, NEAR};
use vector_sim::design::{ShipDesign, T_DAMAGE};
use vector_sim::Sim;

pub type V = [f32; 3];

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V, k: f32) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn nrm(a: V) -> V {
    let l = dot(a, a).sqrt();
    if l < 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        mul(a, 1.0 / l)
    }
}
/// Two unit vectors perpendicular to `axis` and to each other.
fn perp(axis: V) -> (V, V) {
    let a = nrm(axis);
    let h = if a[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = nrm(cross(h, a));
    (u, cross(a, u))
}

/// One triangle of the model, in metres in the frame of the ship (x right, y up, z forward).
#[derive(Clone, Debug)]
pub struct Tri {
    pub v: [V; 3],
    pub color: u32,
    /// Share of the colour that is emitted rather than lit (0..1); the alpha of an additive one.
    pub glow: f32,
    /// Additive light (flames, plasma, flashes): not lit, not written in the depth buffer.
    pub add: bool,
    /// Bare metal catches the light with a highlight.
    pub metal: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub tris: Vec<Tri>,
    /// Wire details: shield bubble, antennas, panel grids.
    pub lines: Vec<(V, V, u32)>,
    /// Radius (m) of the sphere that holds everything but the flames.
    pub radius: f32,
    /// Height of the lowest point of the model (negative): the feet when the gear is out.
    pub foot_y: f32,
    /// Length (m) of the longest exhaust flame behind the nozzles.
    pub flame_len: f32,
}

/// What the ship is doing and how it is: everything the shape depends on besides the design.
#[derive(Clone, Debug)]
pub struct State {
    /// Health (0..1) and power state of each slot of the design.
    pub health: Vec<f64>,
    pub powered: Vec<bool>,
    pub temp_k: f64,
    /// Main engine throttle (0 off, 1 full, up to 2.5 under boost).
    pub thrust: f64,
    /// Shield charge (0..1) and seconds since the last hit.
    pub shield: f64,
    pub hit_age: f64,
    /// Hull integrity (0..1).
    pub hull: f64,
    /// Landing gear (0 stowed .. 1 down) and radiators / solar wings (0 folded .. 1 open).
    pub gear: f64,
    pub deploy: f64,
    pub firing: bool,
    pub laser: bool,
    /// Heat of an atmospheric entry on the nose (0..1).
    pub entry: f64,
    /// Share of RCS in use (0..1).
    pub rcs: f64,
    /// Seconds: drives the flicker of flames and the strobe.
    pub time: f64,
}

impl State {
    /// A healthy ship at rest with its panels out.
    pub fn idle(design: &ShipDesign) -> State {
        State {
            health: vec![1.0; design.slots.len()],
            powered: vec![true; design.slots.len()],
            temp_k: 300.0,
            thrust: 0.0,
            shield: 0.0,
            hit_age: 1.0e9,
            hull: 1.0,
            gear: 0.0,
            deploy: 1.0,
            firing: false,
            laser: false,
            entry: 0.0,
            rcs: 0.0,
            time: 0.0,
        }
    }

    /// The player's ship as the simulation has it right now; `gear` comes from the ground below.
    pub fn of_player(sim: &Sim, gear: f64) -> State {
        let s = &sim.ship;
        let heat = ((s.sys.temp_k - 300.0) / (T_DAMAGE - 300.0)).clamp(0.0, 1.0);
        let entry = (sim.aero_heat_kw / 400.0).clamp(0.0, 1.0);
        let deploy = if sim.docked.is_some() || entry > 0.02 || gear > 0.5 {
            0.0
        } else {
            (0.35 + 1.2 * heat).clamp(0.0, 1.0)
        };
        let cap = sim.effective.shield_cap.max(1.0);
        State {
            health: s.sys.health.clone(),
            powered: s.sys.powered.clone(),
            temp_k: s.sys.temp_k,
            thrust: sim.main_throttle,
            shield: (s.sys.shield_pts / cap).clamp(0.0, 1.0),
            hit_age: sim.time - s.sys.last_hit,
            hull: (s.hull / s.stats.hull_max.max(1.0)).clamp(0.0, 1.0),
            gear,
            deploy,
            firing: !sim.beams.is_empty() || sim.laser_on,
            laser: sim.laser_on,
            entry,
            rcs: 0.0,
            time: sim.time,
        }
    }

    fn hp(&self, slot: usize) -> f32 {
        self.health.get(slot).copied().unwrap_or(1.0) as f32
    }
    fn on(&self, slot: usize) -> bool {
        self.powered.get(slot).copied().unwrap_or(true)
    }
}

/// Size class of a hull: the shape of its fuselage.
struct Look {
    /// Half width over half length.
    aspect: f32,
    /// Superellipse exponent of the sections (2 round, 4 boxy).
    boxy: f32,
    /// (z / half length, width, height) of the sections, rear to nose.
    secs: &'static [(f32, f32, f32)],
    color: u32,
}

/// How slim the fuselage of a hull class is (half width over half length): the one table of the
/// proportions of the classes, read by the 3D model and by the wireframe of the codex.
pub fn class_aspect(class: &str) -> f32 {
    look(class).aspect
}

fn look(class: &str) -> Look {
    const ROUND: &[(f32, f32, f32)] = &[
        (-1.0, 0.55, 0.55),
        (-0.8, 0.85, 0.85),
        (-0.2, 1.0, 1.0),
        (0.45, 0.9, 0.85),
        (0.8, 0.5, 0.4),
        (1.0, 0.08, 0.08),
    ];
    const DART: &[(f32, f32, f32)] = &[
        (-1.0, 0.6, 0.5),
        (-0.7, 0.9, 0.7),
        (0.0, 1.0, 0.8),
        (0.5, 0.55, 0.4),
        (0.85, 0.25, 0.2),
        (1.0, 0.04, 0.04),
    ];
    const BOX: &[(f32, f32, f32)] = &[
        (-1.0, 0.9, 0.9),
        (-0.9, 1.0, 1.0),
        (0.7, 1.0, 1.0),
        (0.9, 0.8, 0.8),
        (1.0, 0.5, 0.5),
    ];
    const STOCKY: &[(f32, f32, f32)] = &[
        (-1.0, 0.7, 0.7),
        (-0.5, 1.0, 1.0),
        (0.3, 1.0, 1.0),
        (0.7, 0.7, 0.6),
        (1.0, 0.35, 0.3),
    ];
    match class {
        "combat" => Look {
            aspect: 0.22,
            boxy: 2.6,
            secs: DART,
            color: 0x0078_8294,
        },
        "rapide" => Look {
            aspect: 0.18,
            boxy: 2.2,
            secs: DART,
            color: 0x00C8_D0DC,
        },
        "cargo" => Look {
            aspect: 0.3,
            boxy: 4.0,
            secs: BOX,
            color: 0x00A8_A090,
        },
        "colonial" | "station" => Look {
            aspect: 0.34,
            boxy: 3.0,
            secs: BOX,
            color: 0x00B0_B4B8,
        },
        "mineur" => Look {
            aspect: 0.36,
            boxy: 3.2,
            secs: STOCKY,
            color: 0x00C0_9A48,
        },
        "science" | "explorateur" => Look {
            aspect: 0.26,
            boxy: 2.4,
            secs: ROUND,
            color: 0x00D0_D8E0,
        },
        _ => Look {
            aspect: 0.28,
            boxy: 2.6,
            secs: ROUND,
            color: 0x00B8_C0C8,
        },
    }
}

/// The fuselage: sections along z, each a superellipse.
struct Hull {
    secs: Vec<(f32, f32, f32)>,
    boxy: f32,
    lh: f32,
}

fn spow(x: f32, p: f32) -> f32 {
    x.signum() * x.abs().powf(p)
}

impl Hull {
    fn new(h: &vector_data::Hull) -> (Hull, Look) {
        let l = look(&h.class);
        let length = 7.0 * ((h.mass / 1000.0) as f32).powf(0.4);
        let lh = length * 0.5;
        let r = l.aspect * lh;
        let secs = l
            .secs
            .iter()
            .map(|&(z, w, hh)| (z * lh, w * r, hh * r * 0.85))
            .collect();
        (
            Hull {
                secs,
                boxy: l.boxy,
                lh,
            },
            l,
        )
    }

    /// Half width and half height of the fuselage at z.
    fn dim(&self, z: f32) -> (f32, f32) {
        let s = &self.secs;
        if z <= s[0].0 {
            return (s[0].1, s[0].2);
        }
        for w in s.windows(2) {
            if z <= w[1].0 {
                let t = (z - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                return (
                    w[0].1 + (w[1].1 - w[0].1) * t,
                    w[0].2 + (w[1].2 - w[0].2) * t,
                );
            }
        }
        let l = s[s.len() - 1];
        (l.1, l.2)
    }

    /// The point of the skin at z and angle (0 right, pi/2 up) and the outward normal.
    fn skin(&self, z: f32, ang: f32) -> (V, V) {
        let (rx, ry) = self.dim(z);
        let (c, s) = (ang.cos(), ang.sin());
        let e = 2.0 / self.boxy;
        let p = [rx * spow(c, e), ry * spow(s, e), z];
        let n = nrm([p[0] / (rx * rx).max(1e-4), p[1] / (ry * ry).max(1e-4), 0.0]);
        (p, n)
    }

    fn max_dim(&self) -> (f32, f32) {
        self.secs
            .iter()
            .fold((0.0, 0.0), |a, s| (f32::max(a.0, s.1), f32::max(a.1, s.2)))
    }
}

struct B {
    tris: Vec<Tri>,
    lines: Vec<(V, V, u32)>,
    flame: f32,
}

impl B {
    fn tri(&mut self, a: V, b: V, c: V, color: u32, glow: f32) {
        self.tris.push(Tri {
            v: [a, b, c],
            color,
            glow,
            add: false,
            metal: false,
        });
    }
    fn metal(&mut self, a: V, b: V, c: V, color: u32) {
        self.tris.push(Tri {
            v: [a, b, c],
            color,
            glow: 0.0,
            add: false,
            metal: true,
        });
    }
    fn light(&mut self, a: V, b: V, c: V, color: u32, alpha: f32) {
        self.tris.push(Tri {
            v: [a, b, c],
            color,
            glow: alpha,
            add: true,
            metal: false,
        });
    }
    fn quad(&mut self, a: V, b: V, c: V, d: V, color: u32, glow: f32) {
        self.tri(a, b, c, color, glow);
        self.tri(a, c, d, color, glow);
    }

    /// A parallelepiped of centre `c` and half vectors `e`.
    fn obox(&mut self, c: V, e: [V; 3], color: u32, glow: f32) {
        let p = |sx: f32, sy: f32, sz: f32| {
            add(c, add(add(mul(e[0], sx), mul(e[1], sy)), mul(e[2], sz)))
        };
        let faces: [[V; 4]; 6] = [
            [
                p(1., -1., -1.),
                p(1., 1., -1.),
                p(1., 1., 1.),
                p(1., -1., 1.),
            ],
            [
                p(-1., -1., -1.),
                p(-1., -1., 1.),
                p(-1., 1., 1.),
                p(-1., 1., -1.),
            ],
            [
                p(-1., 1., -1.),
                p(-1., 1., 1.),
                p(1., 1., 1.),
                p(1., 1., -1.),
            ],
            [
                p(-1., -1., -1.),
                p(1., -1., -1.),
                p(1., -1., 1.),
                p(-1., -1., 1.),
            ],
            [
                p(-1., -1., 1.),
                p(1., -1., 1.),
                p(1., 1., 1.),
                p(-1., 1., 1.),
            ],
            [
                p(-1., -1., -1.),
                p(-1., 1., -1.),
                p(1., 1., -1.),
                p(1., -1., -1.),
            ],
        ];
        for f in faces {
            self.quad(f[0], f[1], f[2], f[3], color, glow);
        }
    }

    fn abox(&mut self, c: V, h: V, color: u32, glow: f32) {
        self.obox(
            c,
            [[h[0], 0.0, 0.0], [0.0, h[1], 0.0], [0.0, 0.0, h[2]]],
            color,
            glow,
        );
    }

    /// A cylinder or cone frustum from `a` (radius r0) to `b` (radius r1).
    #[allow(clippy::too_many_arguments)]
    fn cyl(&mut self, a: V, b: V, r0: f32, r1: f32, n: usize, color: u32, glow: f32, metal: bool) {
        let (u, w) = perp(sub(b, a));
        let ring = |o: V, r: f32, k: usize| {
            let t = std::f32::consts::TAU * k as f32 / n as f32;
            add(o, add(mul(u, r * t.cos()), mul(w, r * t.sin())))
        };
        for k in 0..n {
            let (p0, p1) = (ring(a, r0, k), ring(a, r0, k + 1));
            let (q0, q1) = (ring(b, r1, k), ring(b, r1, k + 1));
            let shade = if k % 2 == 0 { 1.0 } else { 0.94 };
            let col = scale_col(color, shade);
            if metal {
                self.metal(p0, p1, q1, col);
                self.metal(p0, q1, q0, col);
            } else {
                self.tri(p0, p1, q1, col, glow);
                self.tri(p0, q1, q0, col, glow);
            }
            if r1 > 1e-4 {
                self.tri(b, q0, q1, col, glow);
            }
            if r0 > 1e-4 {
                self.tri(a, p1, p0, col, glow);
            }
        }
    }

    fn line(&mut self, a: V, b: V, color: u32) {
        self.lines.push((a, b, color));
    }

    /// A ring of thin tubes around the hull at z.
    fn ring(&mut self, h: &Hull, z: f32, grow: f32, tube: f32, color: u32, glow: f32) {
        let n = 14;
        let pt = |k: usize| {
            let (p, nn) = h.skin(z, std::f32::consts::TAU * k as f32 / n as f32);
            add(p, mul(nn, grow))
        };
        for k in 0..n {
            self.cyl(pt(k), pt(k + 1), tube, tube, 5, color, glow, false);
        }
    }
}

fn hurt(color: u32, hp: f32) -> u32 {
    mix_col(color, 0x0018_1412, ((1.0 - hp) * 0.75).clamp(0.0, 0.85))
}

fn hash01(seed: u64, a: i64, b: i64) -> f32 {
    vector_core::unit(hash_coord(seed, a, b, 7)) as f32
}

/// Angles (rad) where outside fittings sit, in the order they are used.
const ANGS: [f32; 6] = [std::f32::consts::FRAC_PI_2, 4.7124, 0.35, 2.79, 5.93, 3.49];

fn lane(k: usize, lh: f32) -> f32 {
    lh * (-0.5 + (k as f32 * 0.618_034).fract() * 0.95)
}

/// Length (m) of the hull number `hull` as the model draws it.
pub fn hull_length(data: &GameData, hull: usize) -> f64 {
    f64::from(7.0 * ((data.hulls[hull].mass / 1000.0) as f32).powf(0.4))
}

/// Build the model of `design` in state `st`.
pub fn build(data: &GameData, design: &ShipDesign, st: &State) -> Model {
    build_hull(data, &data.hulls[design.hull], design, st)
}

/// The model of `design` on the given hull: the catalogue's hulls and the one a blueprint makes
/// (`shipwright::hull_of`) go through the same builder, so the designer shows the same ship as the
/// outside view.
pub fn build_hull(
    data: &GameData,
    h: &vector_data::Hull,
    design: &ShipDesign,
    st: &State,
) -> Model {
    let (hull, lk) = Hull::new(h);
    let lh = hull.lh;
    let u = (lh / 7.0).powf(0.55);
    let mut b = B {
        tris: Vec::new(),
        lines: Vec::new(),
        flame: 0.0,
    };
    // Fuselage.
    let n = 12;
    let mut body = Vec::new();
    for w in hull.secs.windows(2) {
        for j in 0..n {
            let (a0, a1) = (
                std::f32::consts::TAU * j as f32 / n as f32,
                std::f32::consts::TAU * (j + 1) as f32 / n as f32,
            );
            let (p00, p01) = (hull.skin(w[0].0, a0).0, hull.skin(w[0].0, a1).0);
            let (p10, p11) = (hull.skin(w[1].0, a0).0, hull.skin(w[1].0, a1).0);
            let k = 0.9 + 0.2 * hash01(0x5417, (w[0].0 * 10.0) as i64, j as i64);
            let col = scale_col(lk.color, k);
            body.push(b.tris.len());
            b.metal(p00, p01, p11, col);
            body.push(b.tris.len());
            b.metal(p00, p11, p10, col);
        }
    }
    for (z, nose) in [
        (hull.secs[0].0, false),
        (hull.secs[hull.secs.len() - 1].0, true),
    ] {
        for j in 0..n {
            let (a0, a1) = (
                std::f32::consts::TAU * j as f32 / n as f32,
                std::f32::consts::TAU * (j + 1) as f32 / n as f32,
            );
            let c = [0.0, 0.0, z + if nose { 0.05 * lh } else { -0.02 * lh }];
            let col = scale_col(lk.color, 0.7);
            if nose {
                b.tri(hull.skin(z, a0).0, hull.skin(z, a1).0, c, col, 0.0);
            } else {
                b.tri(hull.skin(z, a1).0, hull.skin(z, a0).0, c, col, 0.0);
            }
        }
    }
    let body_end = b.tris.len();
    // Atmospheric entry: the nose-facing skin glows.
    if st.entry > 0.02 {
        for t in &mut b.tris[..body_end] {
            let nn = nrm(cross(sub(t.v[1], t.v[0]), sub(t.v[2], t.v[0])));
            let face = nn[2].abs().max(0.0);
            let near_nose = t.v[0][2] > lh * 0.2;
            if near_nose || face > 0.5 {
                let k = (st.entry as f32 * if near_nose { 1.0 } else { 0.6 }).min(1.0);
                t.color = mix_col(t.color, 0x00FF_6A20, k);
                t.glow = k * 0.8;
            }
        }
    }
    // Wings and class details.
    let (mx, my) = hull.max_dim();
    match look_name(&h.class) {
        "combat" | "rapide" | "leger" => {
            for s in [-1.0f32, 1.0] {
                let root = -0.15 * lh;
                let (rx, _) = hull.dim(root);
                let a = [s * rx * 0.9, 0.0, root + lh * 0.45];
                let c = [s * rx * 0.9, 0.0, root - lh * 0.35];
                let tip = [s * (rx + lh * 0.42), -0.02 * lh, root - lh * 0.45];
                let tip2 = [s * (rx + lh * 0.38), -0.02 * lh, root - lh * 0.25];
                let col = scale_col(lk.color, 0.8);
                b.metal(a, c, tip, col);
                b.metal(a, tip, tip2, scale_col(lk.color, 0.9));
                b.metal(c, a, tip, col);
            }
        }
        "mineur" => {
            for s in [-1.0f32, 1.0] {
                let c = [s * mx * 0.95, -my * 0.2, lh * 0.55];
                b.abox(c, [0.08 * u, 0.1 * u, lh * 0.38], 0x0070_6860, 0.0);
                b.cyl(
                    [c[0], c[1], c[2] + lh * 0.38],
                    [c[0] - s * 0.25 * u, c[1], c[2] + lh * 0.52],
                    0.1 * u,
                    0.04 * u,
                    6,
                    0x00D0_B040,
                    0.0,
                    true,
                );
            }
        }
        "colonial" | "station" => {
            let r = mx * 2.0;
            let n = 20;
            let ringp = |k: usize| {
                let t = std::f32::consts::TAU * k as f32 / n as f32;
                [r * t.cos(), r * t.sin(), -0.1 * lh]
            };
            for k in 0..n {
                b.cyl(
                    ringp(k),
                    ringp(k + 1),
                    0.28 * u,
                    0.28 * u,
                    5,
                    0x0090_98A0,
                    0.0,
                    true,
                );
            }
            for k in 0..4 {
                let t = std::f32::consts::FRAC_PI_2 * k as f32;
                b.cyl(
                    [mx * 0.9 * t.cos(), my * 0.9 * t.sin(), -0.1 * lh],
                    [r * t.cos(), r * t.sin(), -0.1 * lh],
                    0.1 * u,
                    0.1 * u,
                    5,
                    0x0070_7880,
                    0.0,
                    true,
                );
            }
        }
        "cargo" => {
            b.abox(
                [0.0, my * 1.02, 0.0],
                [0.04 * u, 0.04 * u, lh * 0.8],
                0x0060_6468,
                0.0,
            );
        }
        "science" | "explorateur" => {
            b.cyl(
                [0.0, my * 0.9, lh * 0.6],
                [0.0, my + lh * 0.35, lh * 0.6],
                0.03 * u,
                0.03 * u,
                4,
                0x00A0_A0A8,
                0.0,
                true,
            );
        }
        _ => {}
    }

    // Modules.
    let engines: Vec<usize> = design
        .modules(data)
        .filter(|(_, m)| m.category == "engine")
        .map(|(i, _)| i)
        .collect();
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut greeble = 0usize;
    for (i, m) in design.modules(data) {
        let cat = m.category.as_str();
        let k = {
            let c = counts.entry(cat).or_insert(0);
            *c += 1;
            *c - 1
        };
        let hp = st.hp(i);
        let on = st.on(i) && hp > 0.0;
        let sz = f32::from(m.size).max(1.0);
        let heat = ((st.temp_k - 300.0) / (T_DAMAGE - 300.0)).clamp(0.0, 1.0) as f32;
        let ang = ANGS[k % ANGS.len()];
        match cat {
            "engine" => {
                let slot = engines.iter().position(|&e| e == i).unwrap_or(0);
                engine(&mut b, &hull, m, slot, engines.len(), st, hp, u, sz, i);
            }
            "ftl" => {
                let z = -0.3 * lh + k as f32 * 0.42 * lh;
                let col = hurt(0x0090_98C0, hp);
                b.ring(
                    &hull,
                    z.min(0.7 * lh),
                    0.12 * u * sz.sqrt(),
                    0.1 * u * sz.sqrt(),
                    col,
                    0.0,
                );
                if on {
                    b.ring(
                        &hull,
                        z.min(0.7 * lh),
                        0.12 * u,
                        0.045 * u,
                        0x0060_A0FF,
                        0.9,
                    );
                }
            }
            "gyro" => {
                let z = lh * (0.1 + 0.15 * k as f32);
                b.ring(&hull, z, 0.04 * u, 0.05 * u, hurt(0x0070_7880, hp), 0.0);
            }
            "reactor" => {
                let z = lane(k + 3, lh);
                let (p, nn) = hull.skin(z, std::f32::consts::FRAC_PI_2);
                let c = if m.id.contains("fusion") {
                    0x0080_D0FF
                } else if m.id.starts_with("exo") {
                    0x00C0_60FF
                } else {
                    0x00FF_A030
                };
                let (t, bt) = perp(nn);
                let s = 0.35 * u * sz.sqrt();
                b.obox(
                    add(p, mul(nn, 0.12 * u)),
                    [mul(t, s), mul(nn, 0.14 * u), mul(bt, s * 1.4)],
                    hurt(0x0058_5C64, hp),
                    0.0,
                );
                if on {
                    let glow = 0.45 + 0.4 * heat;
                    b.obox(
                        add(p, mul(nn, 0.27 * u)),
                        [mul(t, s * 0.55), mul(nn, 0.02 * u), mul(bt, s * 1.1)],
                        c,
                        glow,
                    );
                }
            }
            "shield" => {
                let (p, nn) = hull.skin(lh * 0.55, std::f32::consts::FRAC_PI_2 - 0.15 * k as f32);
                let tip = add(p, add(mul(nn, 0.5 * u * sz.sqrt()), [0.0, 0.0, 0.25 * u]));
                b.cyl(
                    p,
                    tip,
                    0.1 * u,
                    0.4 * u * sz.sqrt(),
                    10,
                    hurt(0x00B0_B8C8, hp),
                    0.0,
                    true,
                );
                if on && st.shield > 0.05 {
                    b.cyl(
                        tip,
                        add(tip, mul(nn, 0.03 * u)),
                        0.34 * u * sz.sqrt(),
                        0.34 * u * sz.sqrt(),
                        10,
                        0x0060_E0FF,
                        0.9,
                        false,
                    );
                }
            }
            "radiator" => radiator(&mut b, &hull, k, st, hp, u, sz, i),
            "solar" => solar(&mut b, &hull, k, st, hp, u, sz),
            "weapon" | "defense" | "mining" => {
                weapon(&mut b, &hull, m, k, st, hp, on, u, sz, i);
            }
            "scanner" => {
                let (p, nn) = hull.skin(lane(k + 5, lh), ang.min(2.6));
                let top = add(p, mul(nn, 0.5 * u * sz));
                b.cyl(
                    p,
                    top,
                    0.04 * u,
                    0.04 * u,
                    4,
                    hurt(0x00A0_A4AC, hp),
                    0.0,
                    true,
                );
                let (u1, _) = perp(nn);
                let face = nrm(add(nn, mul(u1, 0.35)));
                b.cyl(
                    top,
                    add(top, mul(face, 0.18 * u)),
                    0.04 * u,
                    0.38 * u * sz.sqrt(),
                    10,
                    hurt(0x00D8_DCE0, hp),
                    0.0,
                    true,
                );
                if on {
                    b.line(top, add(top, mul(face, 0.5 * u)), 0x0040_C080);
                }
            }
            "armor" => {
                let (p, nn) = hull.skin(lane(k + 1, lh) * 0.6, ang);
                let (t, bt) = perp(nn);
                let s = 0.55 * u * sz.sqrt();
                // A cracked plate sits crooked and burnt.
                let tilt = (1.0 - hp) * 0.25;
                b.obox(
                    add(p, mul(nn, 0.05 * u)),
                    [
                        mul(add(t, mul(nn, tilt)), s),
                        mul(nn, 0.05 * u),
                        mul(bt, s * 1.3),
                    ],
                    hurt(0x0070_747C, hp),
                    0.0,
                );
            }
            "rcs" => {
                for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    let z = if k % 2 == 0 { 0.72 * lh } else { -0.62 * lh };
                    let (rx, ry) = hull.dim(z);
                    let p = [sx * rx * 0.78, sy * ry * 0.78, z];
                    let out = nrm([sx, sy, 0.0]);
                    b.cyl(
                        p,
                        add(p, mul(out, 0.14 * u)),
                        0.07 * u,
                        0.1 * u,
                        6,
                        0x0050_545C,
                        0.0,
                        true,
                    );
                    if st.rcs > 0.1 && on {
                        let tip = add(p, mul(out, (0.14 + 0.9 * st.rcs as f32) * u));
                        let (a1, a2) = perp(out);
                        let base = add(p, mul(out, 0.14 * u));
                        b.light(
                            add(base, mul(a1, 0.08 * u)),
                            add(base, mul(a1, -0.08 * u)),
                            tip,
                            0x00A0_C0FF,
                            0.7,
                        );
                        b.light(
                            add(base, mul(a2, 0.08 * u)),
                            add(base, mul(a2, -0.08 * u)),
                            tip,
                            0x00A0_C0FF,
                            0.7,
                        );
                    }
                }
            }
            "dock" => {
                let z0 = lh * 0.98;
                b.cyl(
                    [0.0, 0.0, z0 - 0.05 * lh],
                    [0.0, 0.0, z0 + 0.12 * u],
                    0.38 * u,
                    0.34 * u,
                    12,
                    hurt(0x00C8_B040, hp),
                    0.0,
                    true,
                );
            }
            _ => {
                greebles(&mut b, &hull, m, greeble, st, hp, on, u, sz);
                greeble += 1;
            }
        }
    }

    // Landing gear.
    let mut foot_y = -my;
    if st.gear > 0.02 {
        let g = st.gear as f32;
        let len = 0.32 * lh.max(2.0) * g.max(0.05);
        let legs: &[(f32, f32)] = &[(-0.55, -0.4), (0.55, -0.4), (-0.55, 0.4), (0.55, 0.4)];
        for &(sx, sz) in legs {
            let z = sz * lh;
            let (rx, ry) = hull.dim(z);
            let top = [sx * rx * 0.7, -ry * 0.85, z];
            let foot = [sx * (rx * 1.15 + len * 0.25), -ry * 0.85 - len, z];
            b.cyl(top, foot, 0.07 * u, 0.05 * u, 6, 0x0070_7478, 0.0, true);
            // Brace and pad.
            let mid = add(top, mul(sub(foot, top), 0.45));
            b.cyl(
                [sx * rx * 0.25, -ry * 0.6, z],
                mid,
                0.03 * u,
                0.03 * u,
                4,
                0x0058_5C60,
                0.0,
                true,
            );
            b.cyl(
                add(foot, [0.0, 0.03 * u, 0.0]),
                add(foot, [0.0, -0.03 * u, 0.0]),
                0.2 * u,
                0.2 * u,
                8,
                0x0048_4C50,
                0.0,
                true,
            );
            foot_y = foot_y.min(foot[1] - 0.03 * u);
        }
    }
    // Navigation lights and the strobe.
    for (s, c) in [(-1.0f32, 0x00FF_2020u32), (1.0, 0x0020_FF40)] {
        let (rx, _) = hull.dim(0.0);
        b.abox(
            [s * (rx + 0.05 * u), 0.0, 0.0],
            [0.05 * u, 0.05 * u, 0.05 * u],
            c,
            1.0,
        );
    }
    if (st.time * 1.2).fract() < 0.12 {
        let (_, ry) = hull.dim(-0.7 * lh);
        b.abox(
            [0.0, ry + 0.05 * u, -0.7 * lh],
            [0.06 * u, 0.06 * u, 0.06 * u],
            0x00FF_FFFF,
            1.0,
        );
    }
    // Burns: dark scorch marks and a few embers where the hull has taken damage.
    let wounds = ((1.0 - st.hull) * 14.0).ceil() as usize;
    for d in 0..wounds.min(18) {
        let pick = (hash_coord(0xB0_17, d as i64, design.hull as i64, 3) as usize) % body.len();
        let t = b.tris[body[pick]].clone();
        let c = mul(add(add(t.v[0], t.v[1]), t.v[2]), 1.0 / 3.0);
        let mut nn = nrm(cross(sub(t.v[1], t.v[0]), sub(t.v[2], t.v[0])));
        if dot(nn, [c[0], c[1], c[2] * 0.2]) < 0.0 {
            nn = mul(nn, -1.0);
        }
        let sh = |p: V| add(add(c, mul(sub(p, c), 0.75)), mul(nn, 0.03 * u));
        b.tri(sh(t.v[0]), sh(t.v[1]), sh(t.v[2]), 0x0012_100E, 0.0);
        if st.hull < 0.5 && d % 3 == 0 {
            let sh2 = |p: V| add(add(c, mul(sub(p, c), 0.3)), mul(nn, 0.05 * u));
            b.tri(sh2(t.v[0]), sh2(t.v[1]), sh2(t.v[2]), 0x00FF_4010, 0.9);
        }
    }
    // The shield bubble.
    if st.shield > 0.03 {
        let flash = (1.0 - (st.hit_age / 0.6).clamp(0.0, 1.0)) as f32;
        let k = (0.25 + 0.5 * st.shield as f32 + 0.6 * flash).min(1.0);
        let col = mix_col(scale_col(0x0030_90FF, k), 0x00FF_FFFF, flash * 0.5);
        let (rx, ry, rz) = (mx * 1.5 + 0.5 * u, my * 1.5 + 0.5 * u, lh * 1.12);
        let pt = |lat: f32, lon: f32| {
            [
                rx * lat.cos() * lon.cos(),
                ry * lat.sin(),
                rz * lat.cos() * lon.sin(),
            ]
        };
        let steps = 24;
        for lat_k in -2..=2 {
            let lat = lat_k as f32 * 0.5;
            for s in 0..steps {
                let (a, c) = (
                    std::f32::consts::TAU * s as f32 / steps as f32,
                    std::f32::consts::TAU * (s + 1) as f32 / steps as f32,
                );
                b.line(pt(lat, a), pt(lat, c), col);
            }
        }
        for lon_k in 0..6 {
            let lon = lon_k as f32 * std::f32::consts::PI / 3.0;
            for s in -6..6 {
                let (a, c) = (s as f32 * 0.26, (s + 1) as f32 * 0.26);
                b.line(pt(a, lon), pt(c, lon), col);
            }
        }
    }
    let radius = b
        .tris
        .iter()
        .filter(|t| !t.add)
        .flat_map(|t| t.v.iter())
        .map(|p| dot(*p, *p).sqrt())
        .fold(lh, f32::max);
    Model {
        tris: b.tris,
        lines: b.lines,
        radius,
        foot_y,
        flame_len: b.flame,
    }
}

fn look_name(class: &str) -> &'static str {
    match class {
        "combat" => "combat",
        "rapide" => "rapide",
        "leger" => "leger",
        "mineur" => "mineur",
        "colonial" => "colonial",
        "station" => "station",
        "cargo" => "cargo",
        "science" => "science",
        "explorateur" => "explorateur",
        _ => "other",
    }
}

#[allow(clippy::too_many_arguments)]
fn engine(
    b: &mut B,
    h: &Hull,
    m: &Module,
    k: usize,
    n: usize,
    st: &State,
    hp: f32,
    u: f32,
    sz: f32,
    slot: usize,
) {
    let lh = h.lh;
    let z0 = -lh;
    let (rxr, ryr) = h.dim(z0);
    let rm = rxr.min(ryr);
    let spread = if n > 1 { 1.7 } else { 1.0 };
    let rn = ((0.3 + 0.18 * sz) * u).min(0.85 * rm / spread).max(0.12);
    let rho = 0.58 * rm;
    let (ox, oy) = match n {
        1 => (0.0, 0.0),
        2 => ((k as f32 * 2.0 - 1.0) * rho, 0.0),
        4 => (
            (k % 2) as f32 * 2.0 * rho * 0.7 - rho * 0.7,
            (k / 2) as f32 * 2.0 * rho * 0.7 - rho * 0.7,
        ),
        _ => {
            let t = std::f32::consts::TAU * k as f32 / n as f32 + std::f32::consts::FRAC_PI_2;
            (rho * t.cos(), rho * t.sin())
        }
    };
    let thr = (st.thrust as f32 * hp).max(0.0);
    let dead = hp <= 0.0;
    let body = hurt(0x0050_545C, hp);
    let hot = mix_col(body, 0x00C0_4818, (thr * 0.5).min(0.8));
    let z_e = z0 - 1.7 * rn;
    b.cyl(
        [ox, oy, z0 + 0.6 * rn],
        [ox, oy, z0 - 0.2 * rn],
        rn * 0.85,
        rn * 0.6,
        10,
        body,
        0.0,
        true,
    );
    b.cyl(
        [ox, oy, z0 - 0.2 * rn],
        [ox, oy, z_e],
        rn * 0.55,
        rn * 1.1,
        10,
        hot,
        thr.min(1.0) * 0.35,
        true,
    );
    if dead {
        return;
    }
    if thr > 0.01 {
        let flick = 1.0 + 0.07 * (st.time as f32 * 45.0 + k as f32 * 1.7).sin();
        let len = rn * 7.0 * thr * flick;
        b.flame = b.flame.max(len);
        let blue = m.isp > 8000.0;
        let (outer, mid) = if blue {
            (0x0030_70FF, 0x0080_C0FF)
        } else {
            (0x00FF_7010, 0x00FF_B040)
        };
        let tip = |l: f32| [ox, oy, z_e - l];
        for (r, l, c, a) in [
            (1.0, 1.0, outer, 0.45),
            (0.7, 0.7, mid, 0.6),
            (0.4, 0.4, 0x00FF_FFE0, 0.85),
        ] {
            let nn = 8;
            for s in 0..nn {
                let (t0, t1) = (
                    std::f32::consts::TAU * s as f32 / nn as f32,
                    std::f32::consts::TAU * (s + 1) as f32 / nn as f32,
                );
                let p = |t: f32| [ox + rn * r * t.cos(), oy + rn * r * t.sin(), z_e];
                b.light(p(t0), p(t1), tip(len * l), c, a);
            }
        }
        // The glow of the throat.
        let q = rn * 1.05;
        b.light(
            [ox - q, oy - q, z_e],
            [ox + q, oy - q, z_e],
            [ox, oy + q, z_e - 0.1 * rn],
            0x00FF_C080,
            0.5 * thr.min(1.0),
        );
    }
    let _ = slot;
}

#[allow(clippy::too_many_arguments)]
fn radiator(b: &mut B, h: &Hull, k: usize, st: &State, hp: f32, u: f32, sz: f32, _slot: usize) {
    let lh = h.lh;
    let side = if k.is_multiple_of(2) { 1.0f32 } else { -1.0 };
    let z = lh * (0.0 - 0.28 * (k / 2) as f32 + 0.1);
    let (rx, _) = h.dim(z);
    let hl = (0.5 + 0.28 * sz) * u;
    let w = (0.9 + 0.5 * sz) * u;
    let th = st.deploy as f32 * std::f32::consts::FRAC_PI_2;
    let d = [side * th.sin(), -th.cos(), 0.0];
    let hinge = [side * rx * 0.98, 0.0, z];
    let heat = ((st.temp_k - 300.0) / (T_DAMAGE - 300.0)).clamp(0.0, 1.0) as f32;
    let base = hurt(0x00A0_A8B0, hp);
    let col = mix_col(base, 0x00FF_5A20, heat * 0.85);
    let nrm_ = [-d[1] * side, d[0] * side, 0.0];
    let nrm_ = nrm(nrm_);
    // Three fins separated by gaps, and a thin spar.
    for f in 0..3 {
        let a = w * (0.12 + f as f32 * 0.3);
        let c = add(hinge, mul(d, a + w * 0.13));
        b.obox(
            c,
            [mul(d, w * 0.13), mul(nrm_, 0.025 * u), [0.0, 0.0, hl]],
            col,
            heat * 0.8,
        );
    }
    b.cyl(
        hinge,
        add(hinge, mul(d, w)),
        0.03 * u,
        0.03 * u,
        4,
        0x0060_6468,
        0.0,
        true,
    );
}

fn solar(b: &mut B, h: &Hull, k: usize, st: &State, hp: f32, u: f32, sz: f32) {
    let lh = h.lh;
    let side = if k.is_multiple_of(2) { 1.0f32 } else { -1.0 };
    let z = lh * (0.4 - 0.3 * (k / 2) as f32);
    let (rx, _) = h.dim(z);
    let hl = (0.7 + 0.3 * sz) * u;
    let w = (1.4 + 0.8 * sz) * u;
    let th = st.deploy as f32 * std::f32::consts::FRAC_PI_2;
    // Folded the wing lies along the flank; open it stands out flat.
    let d = [side * th.sin(), -th.cos() * 0.3, th.cos().abs() * 0.0];
    let d = nrm(add(d, [0.0, if th < 0.1 { -0.9 } else { 0.0 }, 0.0]));
    let hinge = [side * rx, 0.0, z];
    let col = hurt(0x0020_3C78, hp);
    for seg in 0..3 {
        let a = w * (0.04 + seg as f32 * 0.33);
        let c = add(hinge, mul(d, a + w * 0.15));
        let e0 = mul(d, w * 0.15);
        b.obox(c, [e0, [0.0, 0.025 * u, 0.0], [0.0, 0.0, hl]], col, 0.0);
        // Cell grid.
        if hp > 0.0 {
            let up = [0.0, 0.03 * u, 0.0];
            for g in 0..4 {
                let zz = -hl + hl * 0.5 * g as f32;
                b.line(
                    add(add(c, mul(e0, -1.0)), add(up, [0.0, 0.0, zz])),
                    add(add(c, e0), add(up, [0.0, 0.0, zz])),
                    0x0060_80C0,
                );
            }
        }
    }
    b.cyl(
        hinge,
        add(hinge, mul(d, w)),
        0.03 * u,
        0.03 * u,
        4,
        0x0070_7478,
        0.0,
        true,
    );
}

#[allow(clippy::too_many_arguments)]
fn weapon(
    b: &mut B,
    h: &Hull,
    m: &Module,
    k: usize,
    st: &State,
    hp: f32,
    on: bool,
    u: f32,
    sz: f32,
    _slot: usize,
) {
    let lh = h.lh;
    let hp_ang = [std::f32::consts::FRAC_PI_2, 4.7124, 0.5, 2.64, 5.78, 3.8];
    let zs = [0.35, 0.2, 0.5, 0.05, -0.1, 0.62];
    let (p, nn) = h.skin(lh * zs[k % 6], hp_ang[k % 6]);
    let s = u * (0.5 + 0.28 * sz);
    let body = hurt(0x0068_6C74, hp);
    let base_top = add(p, mul(nn, 0.28 * s));
    b.cyl(p, base_top, 0.34 * s, 0.28 * s, 10, body, 0.0, true);
    let fwd = [0.0f32, 0.0, 1.0];
    let start = add(base_top, mul(nn, 0.1 * s));
    let weapon = m.weapon.as_str();
    let cat = m.category.as_str();
    let tip;
    let flash;
    if cat == "mining" {
        tip = add(start, add(mul(fwd, 1.4 * s), mul(nn, -0.1 * s)));
        b.cyl(start, tip, 0.1 * s, 0.07 * s, 6, body, 0.0, true);
        b.cyl(
            tip,
            add(tip, mul(fwd, 0.18 * s)),
            0.16 * s,
            0.2 * s,
            8,
            hurt(0x00C0_8030, hp),
            0.0,
            true,
        );
        flash = if st.laser { 0x00FF_5030 } else { 0 };
    } else if cat == "defense" {
        tip = add(start, mul(fwd, 0.9 * s));
        for off in [-0.09f32, 0.09] {
            let o = [off * s * 1.5, 0.0, 0.0];
            b.cyl(
                add(start, o),
                add(tip, o),
                0.04 * s,
                0.035 * s,
                6,
                body,
                0.0,
                true,
            );
        }
        flash = 0;
    } else {
        match weapon {
            "beam" => {
                tip = add(start, mul(fwd, 1.3 * s));
                b.cyl(start, tip, 0.1 * s, 0.07 * s, 8, body, 0.0, true);
                b.cyl(
                    tip,
                    add(tip, mul(fwd, 0.1 * s)),
                    0.05 * s,
                    0.05 * s,
                    6,
                    0x00FF_6060,
                    if on { 0.9 } else { 0.2 },
                    false,
                );
                flash = 0x00FF_5050;
            }
            "missile" | "mine" => {
                tip = add(start, mul(fwd, 0.9 * s));
                for off in [-0.28f32, 0.28] {
                    let o = [off * s, 0.0, 0.0];
                    b.abox(
                        add(add(start, o), mul(fwd, 0.4 * s)),
                        [0.2 * s, 0.2 * s, 0.55 * s],
                        body,
                        0.0,
                    );
                    b.cyl(
                        add(add(start, o), mul(fwd, 0.95 * s)),
                        add(add(start, o), mul(fwd, 1.25 * s)),
                        0.15 * s,
                        0.0,
                        6,
                        0x00C8_C0A8,
                        0.0,
                        false,
                    );
                }
                flash = 0x00FF_A040;
            }
            "ecm" => {
                tip = add(start, mul(nn, 0.5 * s));
                b.cyl(start, tip, 0.04 * s, 0.4 * s, 10, body, 0.0, true);
                flash = 0;
            }
            _ => {
                // Kinetic: a heavy barrel with a muzzle brake.
                tip = add(start, mul(fwd, (1.2 + 0.25 * sz) * s));
                b.cyl(start, tip, 0.13 * s, 0.1 * s, 8, body, 0.0, true);
                b.cyl(
                    add(tip, mul(fwd, -0.12 * s)),
                    add(tip, mul(fwd, 0.12 * s)),
                    0.16 * s,
                    0.16 * s,
                    8,
                    hurt(0x0048_4C54, hp),
                    0.0,
                    true,
                );
                flash = 0x00FF_C060;
            }
        }
    }
    if st.firing && on && flash != 0 && hp > 0.0 {
        let len = 0.9 * s * (0.8 + 0.4 * ((st.time * 60.0).sin() as f32).abs());
        for (r, c, a) in [(0.2f32, flash, 0.7f32), (0.1, 0x00FF_FFFF, 0.9)] {
            let (a1, _) = perp(fwd);
            let (_, a2) = perp(fwd);
            b.light(
                add(tip, mul(a1, r * s)),
                add(tip, mul(a2, r * s)),
                add(tip, mul(fwd, len)),
                c,
                a,
            );
            b.light(
                add(tip, mul(a1, -r * s)),
                add(tip, mul(a2, -r * s)),
                add(tip, mul(fwd, len)),
                c,
                a,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn greebles(
    b: &mut B,
    h: &Hull,
    m: &Module,
    k: usize,
    st: &State,
    hp: f32,
    on: bool,
    u: f32,
    sz: f32,
) {
    let lh = h.lh;
    let z = lane(k, lh);
    let ang = ANGS[k % ANGS.len()];
    let (p, nn) = h.skin(z, ang);
    let (t, bt) = perp(nn);
    let s = 0.3 * u * (0.7 + 0.4 * sz);
    let cat = m.category.as_str();
    match cat {
        "tank" => {
            // A conformal tank along the flank with domed ends.
            let c = add(p, mul(nn, 0.25 * s));
            let l = s * (1.4 + 0.4 * sz);
            let a = add(c, [0.0, 0.0, -l]);
            let e = add(c, [0.0, 0.0, l]);
            let col = hurt(0x00B8_B0A0, hp);
            b.cyl(a, e, 0.5 * s, 0.5 * s, 10, col, 0.0, true);
            b.cyl(
                e,
                add(e, [0.0, 0.0, 0.3 * s]),
                0.5 * s,
                0.1 * s,
                10,
                col,
                0.0,
                true,
            );
            b.cyl(
                a,
                add(a, [0.0, 0.0, -0.3 * s]),
                0.5 * s,
                0.1 * s,
                10,
                col,
                0.0,
                true,
            );
        }
        "cargo" | "hangar" | "containment" | "armory" => {
            let c = add(p, mul(nn, 0.45 * s));
            let hue = [0x00B0_5030u32, 0x0040_7090, 0x0090_8030, 0x0050_8050]
                [(hash_coord(7, k as i64, i64::from(m.size), 1) % 4) as usize];
            let col = if cat == "armory" { 0x0058_5C48 } else { hue };
            b.obox(
                c,
                [mul(t, s * 1.1), mul(nn, 0.45 * s), mul(bt, s * 1.6)],
                hurt(col, hp),
                0.0,
            );
            if cat == "hangar" {
                b.obox(
                    add(c, [0.0, 0.0, s * 1.62]),
                    [mul(t, s * 0.9), mul(nn, 0.3 * s), [0.0, 0.0, 0.02 * s]],
                    0x0010_1418,
                    0.0,
                );
            }
        }
        "habitat" | "greenhouse" => {
            let c = add(p, mul(nn, 0.1 * s));
            let col = if cat == "greenhouse" {
                0x0040_C060
            } else {
                0x00C8_D0D8
            };
            let top = add(c, mul(nn, 0.7 * s));
            b.cyl(
                c,
                add(c, mul(nn, 0.25 * s)),
                0.9 * s,
                0.9 * s,
                12,
                hurt(0x0090_9498, hp),
                0.0,
                true,
            );
            b.cyl(
                add(c, mul(nn, 0.25 * s)),
                top,
                0.9 * s,
                0.35 * s,
                12,
                hurt(col, hp),
                if on { 0.35 } else { 0.0 },
                false,
            );
            // Windows.
            for w in 0..4 {
                let a = std::f32::consts::FRAC_PI_2 * w as f32 + 0.4;
                let q = add(
                    add(c, mul(t, a.cos() * 0.8 * s)),
                    mul(bt, a.sin() * 0.8 * s),
                );
                b.abox(
                    add(q, mul(nn, 0.28 * s)),
                    [0.07 * s, 0.07 * s, 0.07 * s],
                    0x00FF_E8A0,
                    if on { 0.9 } else { 0.1 },
                );
            }
        }
        "battery" => {
            let c = add(p, mul(nn, 0.2 * s));
            b.obox(
                c,
                [mul(t, s), mul(nn, 0.2 * s), mul(bt, s * 0.7)],
                hurt(0x0050_6890, hp),
                0.0,
            );
            b.obox(
                add(c, mul(nn, 0.21 * s)),
                [mul(t, 0.7 * s), mul(nn, 0.02 * s), mul(bt, 0.12 * s)],
                0x0040_FF80,
                if on { 0.8 } else { 0.1 },
            );
        }
        "lab" | "workshop" | "medbay" | "life_support" | "reactor" | "scanner" => {
            let c = add(p, mul(nn, 0.3 * s));
            let col = match cat {
                "medbay" => 0x00E0_E0E0,
                "lab" => 0x00A8_C0D0,
                "workshop" => 0x00C0_8040,
                _ => 0x0088_90A0,
            };
            b.obox(
                c,
                [mul(t, s), mul(nn, 0.3 * s), mul(bt, s * 1.2)],
                hurt(col, hp),
                0.0,
            );
            if cat == "medbay" {
                b.obox(
                    add(c, mul(nn, 0.31 * s)),
                    [mul(t, 0.4 * s), mul(nn, 0.01 * s), mul(bt, 0.1 * s)],
                    0x00E0_2020,
                    0.8,
                );
                b.obox(
                    add(c, mul(nn, 0.31 * s)),
                    [mul(t, 0.1 * s), mul(nn, 0.01 * s), mul(bt, 0.4 * s)],
                    0x00E0_2020,
                    0.8,
                );
            } else if cat == "life_support" {
                b.cyl(
                    add(c, mul(nn, 0.3 * s)),
                    add(c, mul(nn, 0.36 * s)),
                    0.5 * s,
                    0.5 * s,
                    10,
                    0x0030_3438,
                    0.0,
                    false,
                );
            } else if cat == "lab" || cat == "scanner" {
                b.cyl(
                    add(c, mul(nn, 0.3 * s)),
                    add(c, mul(nn, 1.1 * s)),
                    0.02 * s,
                    0.02 * s,
                    4,
                    0x00A0_A0A8,
                    0.0,
                    true,
                );
            } else if cat == "workshop" {
                b.cyl(
                    add(c, mul(nn, 0.3 * s)),
                    add(c, add(mul(nn, 0.9 * s), [0.0, 0.0, 0.8 * s])),
                    0.05 * s,
                    0.04 * s,
                    5,
                    0x00D0_B040,
                    0.0,
                    true,
                );
            }
        }
        _ => {
            let c = add(p, mul(nn, 0.2 * s));
            b.obox(
                c,
                [mul(t, s * 0.8), mul(nn, 0.2 * s), mul(bt, s * 0.8)],
                hurt(0x0080_8488, hp),
                0.0,
            );
        }
    }
    let _ = st;
}

/// A world-space placement of a model.
pub struct Pose {
    pub pos: Vec3,
    /// Model to world.
    pub rot: Mat3,
    pub scale: f64,
}

/// The light on the ship: the star's direction, the ambient share and the intensity (0..1).
pub struct Light {
    pub sun: Vec3,
    pub ambient: f32,
    pub intensity: f32,
}

/// Inverse-depth buffer shared by every model drawn in a frame (0 = nothing yet).
pub struct Zbuf {
    pub w: usize,
    pub inv: Vec<f32>,
}

impl Zbuf {
    pub fn new(w: usize, h: usize) -> Zbuf {
        Zbuf {
            w,
            inv: vec![0.0; w * h],
        }
    }

    /// Empty for a frame of `w` x `h`, reusing the memory.
    pub fn reset(&mut self, w: usize, h: usize) {
        self.w = w;
        self.inv.clear();
        self.inv.resize(w * h, 0.0);
    }
}

fn shade(base: u32, k: f32) -> u32 {
    scale_col(base, k)
}

/// Paint `m` placed by `pose` as seen by `cam`: z-buffered, flat shaded by `light`; additive
/// parts (flames) are depth tested but not written; wire details are drawn last.
pub fn draw(
    fb: &mut Framebuffer,
    zb: &mut Zbuf,
    cam: &Camera,
    m: &Model,
    pose: &Pose,
    light: &Light,
) {
    let s = pose.scale;
    let tw = |p: V| -> Vec3 {
        pose.pos
            + pose.rot.mul_vec(Vec3::new(
                f64::from(p[0]) * s,
                f64::from(p[1]) * s,
                f64::from(p[2]) * s,
            ))
    };
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
    let (cx, cy) = (fb.w as f64 * 0.5, fb.h as f64 * 0.5);
    let sun = light.sun.normalized();
    // Opaque first, additive second.
    for pass in 0..2 {
        for t in m.tris.iter().filter(|t| t.add == (pass == 1)) {
            let w = [tw(t.v[0]), tw(t.v[1]), tw(t.v[2])];
            let col = if t.add {
                scale_col(t.color, t.glow)
            } else {
                let mut n = (w[1] - w[0]).cross(w[2] - w[0]);
                if n.length() < 1e-12 {
                    continue;
                }
                n = n.normalized();
                let to_cam = (cam.pos - w[0]).normalized();
                if n.dot(to_cam) < 0.0 {
                    n = -n;
                }
                let diff = n.dot(sun).max(0.0) as f32;
                // A little earthshine from the side the star does not reach.
                let back = (-n.dot(sun)).max(0.0) as f32 * 0.1;
                let mut lum = light.ambient + (1.0 - light.ambient) * diff * light.intensity + back;
                if t.metal {
                    let hv = (sun + to_cam).normalized();
                    let sp = n.dot(hv).max(0.0).powi(18) as f32;
                    lum += sp * 0.45 * light.intensity;
                }
                let lit = shade(t.color, lum);
                if t.glow > 0.0 {
                    mix_col(lit, t.color, t.glow)
                } else {
                    lit
                }
            };
            // To view space and clip against the near plane (at most four corners).
            let vs = [cam.to_view(w[0]), cam.to_view(w[1]), cam.to_view(w[2])];
            let mut poly = [Vec3::ZERO; 4];
            let mut n = 0;
            for i in 0..3 {
                let (a, b) = (vs[i], vs[(i + 1) % 3]);
                if a.z >= NEAR {
                    poly[n] = a;
                    n += 1;
                }
                if (a.z >= NEAR) != (b.z >= NEAR) {
                    let k = (NEAR - a.z) / (b.z - a.z);
                    poly[n] = a + (b - a) * k;
                    n += 1;
                }
            }
            if n < 3 {
                continue;
            }
            let mut pr = [(0.0f32, 0.0f32, 0.0f32); 4];
            for (o, v) in pr.iter_mut().zip(&poly[..n]) {
                *o = (
                    (cx + v.x / v.z * focal) as f32,
                    (cy - v.y / v.z * focal) as f32,
                    (1.0 / v.z) as f32,
                );
            }
            for i in 1..n - 1 {
                fill(fb, zb, [pr[0], pr[i], pr[i + 1]], col, t.add);
            }
        }
    }
    for (a, b, c) in &m.lines {
        cam.draw_line(fb, tw(*a), tw(*b), *c);
    }
}

fn fill(fb: &mut Framebuffer, zb: &mut Zbuf, p: [(f32, f32, f32); 3], col: u32, additive: bool) {
    let (w, h) = (fb.w, fb.h);
    raster_tri(p, w, h, |x, y, z| {
        let at = y as usize * zb.w + x as usize;
        if z <= zb.inv[at] {
            return;
        }
        if additive {
            fb.plot(x, y, col);
        } else {
            zb.inv[at] = z;
            fb.px_set(x, y, col);
        }
    });
}

/// Visit every pixel of a screen triangle (x, y, 1/z per corner) with its interpolated 1/z: one
/// span per row between the two edges that cross it, 1/z stepped along the span.
pub fn raster_tri(p: [(f32, f32, f32); 3], w: usize, h: usize, mut f: impl FnMut(i32, i32, f32)) {
    let [mut a, mut b, mut c] = p;
    if a.1 > b.1 {
        std::mem::swap(&mut a, &mut b);
    }
    if b.1 > c.1 {
        std::mem::swap(&mut b, &mut c);
    }
    if a.1 > b.1 {
        std::mem::swap(&mut a, &mut b);
    }
    if c.1 - a.1 < 1e-6 || !(c.1 - a.1).is_finite() {
        return;
    }
    let y0 = (a.1 - 0.5).ceil().max(0.0) as i32;
    let y1 = (c.1 - 0.5).floor().min(h as f32 - 1.0) as i32;
    let long = 1.0 / (c.1 - a.1);
    let upper = 1.0 / (b.1 - a.1).max(1e-9);
    let lower = 1.0 / (c.1 - b.1).max(1e-9);
    let wmax = w as f32 - 1.0;
    for y in y0..=y1 {
        let py = y as f32 + 0.5;
        let t = (py - a.1) * long;
        let (xl, zl) = (a.0 + (c.0 - a.0) * t, a.2 + (c.2 - a.2) * t);
        let (xs, zs) = if py < b.1 {
            let t = (py - a.1) * upper;
            (a.0 + (b.0 - a.0) * t, a.2 + (b.2 - a.2) * t)
        } else {
            let t = (py - b.1) * lower;
            (b.0 + (c.0 - b.0) * t, b.2 + (c.2 - b.2) * t)
        };
        let (x0, z0, x1, z1) = if xl <= xs {
            (xl, zl, xs, zs)
        } else {
            (xs, zs, xl, zl)
        };
        let xa = (x0 - 0.5).ceil().max(0.0);
        let xb = (x1 - 0.5).floor().min(wmax);
        if xa > xb {
            continue;
        }
        let dz = if x1 - x0 > 1e-6 {
            (z1 - z0) / (x1 - x0)
        } else {
            0.0
        };
        let mut z = z0 + dz * (xa + 0.5 - x0);
        for x in xa as i32..=xb as i32 {
            f(x, y, z);
            z += dz;
        }
    }
}

/// A generic fit of a hull, to show what it looks like with its parts on (codex, shipyard).
pub fn showcase_design(data: &GameData, hull: usize) -> ShipDesign {
    let mut d = ShipDesign::empty(data, hull);
    for m in [
        "torch_mk1",
        "fusion_mk1",
        "tank_mk1",
        "cargo_mk1",
        "radiator_mk1",
        "solar_mk1",
        "pulselaser_mk1",
        "armor_mk1",
        "scanner_mk1",
    ] {
        let _ = d.install_first_fit(data, m);
    }
    d
}

/// Draw `design` turning (angle `t`) in the square of side `size` centred at (cx, cy): the
/// same solid model as the third-person view, for the screens that show a ship.
#[allow(clippy::too_many_arguments)]
pub fn turntable(
    fb: &mut Framebuffer,
    data: &GameData,
    design: &ShipDesign,
    st: &State,
    cx: i32,
    cy: i32,
    size: usize,
    t: f64,
) {
    let m = build(data, design, st);
    let mut tmp = Framebuffer::new(size, size);
    let mut zb = Zbuf::new(size, size);
    let fov = 0.6f64;
    let focal = size as f64 * 0.5 / (fov * 0.5).tan();
    let d = f64::from(m.radius) * focal / (size as f64 * 0.46);
    let eye = Vec3::new(0.0, d * 0.28, -d);
    let cam = Camera {
        pos: eye,
        rot: Mat3::look_at(eye * -1.0, Vec3::new(0.0, 1.0, 0.0)).transpose(),
        fov_y: fov,
    };
    let pose = Pose {
        pos: Vec3::ZERO,
        rot: Mat3::rot_y(t),
        scale: 1.0,
    };
    let light = Light {
        sun: Vec3::new(-0.5, 0.7, -0.6),
        ambient: 0.22,
        intensity: 1.0,
    };
    draw(&mut tmp, &mut zb, &cam, &m, &pose, &light);
    let half = size as i32 / 2;
    for y in 0..size {
        for x in 0..size {
            let p = tmp.px[y * size + x];
            if p != 0 {
                fb.px_set(cx - half + x as i32, cy - half + y as i32, p);
            }
        }
    }
}

/// The loadout of a blueprint as a design (slot by slot, in the order of its hull's slots).
pub fn blueprint_design(data: &GameData, bp: &vector_sim::shipwright::Blueprint) -> ShipDesign {
    let slots = bp.slots(data);
    ShipDesign {
        hull: 0,
        slots: slots
            .iter()
            .map(|s| {
                bp.fit_at(s.x, s.y, s.z, s.slot)
                    .and_then(|f| data.module_index(&f.module))
            })
            .collect(),
    }
}

/// Draw the ship a blueprint makes (its own hull, its fitted modules) in `rect`, turned by `yaw`
/// about the vertical and `pitch` (the camera going up): the same model as the outside view.
#[allow(clippy::too_many_arguments)]
pub fn blueprint_view(
    fb: &mut Framebuffer,
    data: &GameData,
    hull: &vector_data::Hull,
    design: &ShipDesign,
    rect: (i32, i32, i32, i32),
    yaw: f64,
    pitch: f64,
    t: f64,
) {
    let (w, h) = (
        (rect.2 - rect.0).max(8) as usize,
        (rect.3 - rect.1).max(8) as usize,
    );
    let mut st = State::idle(design);
    st.time = t;
    let m = build_hull(data, hull, design, &st);
    let mut tmp = Framebuffer::new(w, h);
    let mut zb = Zbuf::new(w, h);
    let fov = 0.6f64;
    let focal = w.min(h) as f64 * 0.5 / (fov * 0.5).tan();
    let d = f64::from(m.radius) * focal / (w.min(h) as f64 * 0.46);
    let eye = Vec3::new(0.0, d * 0.28, -d);
    let cam = Camera {
        pos: eye,
        rot: Mat3::look_at(eye * -1.0, Vec3::new(0.0, 1.0, 0.0)).transpose(),
        fov_y: fov,
    };
    let pose = Pose {
        pos: Vec3::ZERO,
        rot: Mat3::rot_x(-pitch).mul(&Mat3::rot_y(yaw)),
        scale: 1.0,
    };
    let light = Light {
        sun: Vec3::new(-0.5, 0.7, -0.6),
        ambient: 0.22,
        intensity: 1.0,
    };
    draw(&mut tmp, &mut zb, &cam, &m, &pose, &light);
    for y in 0..h {
        for x in 0..w {
            let p = tmp.px[y * w + x];
            if p != 0 {
                fb.px_set(rect.0 + x as i32, rect.1 + y as i32, p);
            }
        }
    }
}

/// A solid space station of radius `r` in two parts, built once: the fixed hub with its solar
/// vanes, and the ring on its spokes (turn it about the z axis of the model to spin it).
pub fn station_parts(r: f64) -> (Model, Model) {
    let r = r as f32;
    let fixed = {
        let mut b = B {
            tris: Vec::new(),
            lines: Vec::new(),
            flame: 0.0,
        };
        b.cyl(
            [0.0, 0.0, -0.9 * r],
            [0.0, 0.0, 0.9 * r],
            0.22 * r,
            0.22 * r,
            12,
            0x00A8_B0B8,
            0.0,
            true,
        );
        b.cyl(
            [0.0, 0.0, 0.9 * r],
            [0.0, 0.0, 1.05 * r],
            0.26 * r,
            0.14 * r,
            12,
            0x00C8_B040,
            0.0,
            true,
        );
        for s in [-1.0f32, 1.0] {
            for f in 0..3 {
                b.abox(
                    [s * (0.4 + 0.3 * f as f32) * r, 0.0, -0.75 * r],
                    [0.13 * r, 0.01 * r, 0.14 * r],
                    0x0020_3C78,
                    0.0,
                );
            }
        }
        b
    };
    let ring = {
        let mut b = B {
            tris: Vec::new(),
            lines: Vec::new(),
            flame: 0.0,
        };
        let n = 24;
        let at = |k: usize| {
            let t = std::f32::consts::TAU * k as f32 / n as f32;
            [0.8 * r * t.cos(), 0.8 * r * t.sin(), 0.0]
        };
        for k in 0..n {
            b.cyl(
                at(k),
                at(k + 1),
                0.09 * r,
                0.09 * r,
                6,
                0x0090_98A0,
                0.0,
                true,
            );
            if k % 2 == 0 {
                b.abox(at(k), [0.07 * r, 0.07 * r, 0.12 * r], 0x00D8_E0A0, 0.8);
            }
        }
        for k in 0..4 {
            let t = std::f32::consts::FRAC_PI_2 * k as f32;
            b.cyl(
                [0.0, 0.0, 0.0],
                [0.8 * r * t.cos(), 0.8 * r * t.sin(), 0.0],
                0.04 * r,
                0.04 * r,
                5,
                0x0070_7880,
                0.0,
                true,
            );
        }
        b
    };
    let part = |b: B| Model {
        tris: b.tris,
        lines: b.lines,
        radius: r * 1.1,
        foot_y: -r,
        flame_len: 0.0,
    };
    (part(fixed), part(ring))
}

/// A rock: an icosphere whose corners are pushed in and out by `seed`, flat shaded in `col`,
/// of radius 1 (scale it by the pose). One per (seed, colour), built once.
pub fn rock_model(seed: u64, col: u32) -> Model {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;
    let mut v: Vec<V> = vec![
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let mut f: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    // One subdivision: 80 faces.
    let mut mid = std::collections::HashMap::new();
    let mut half = |v: &mut Vec<V>, a: usize, b: usize| -> usize {
        let key = (a.min(b), a.max(b));
        *mid.entry(key).or_insert_with(|| {
            v.push(add(v[a], v[b]));
            v.len() - 1
        })
    };
    let mut g = Vec::new();
    for [a, b, c] in f.drain(..) {
        let (ab, bc, ca) = (half(&mut v, a, b), half(&mut v, b, c), half(&mut v, c, a));
        g.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
    }
    let pts: Vec<V> = v
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let d = nrm(*p);
            let k = 0.72 + 0.4 * hash01(seed, i as i64, 5);
            mul(d, k)
        })
        .collect();
    let mut b = B {
        tris: Vec::new(),
        lines: Vec::new(),
        flame: 0.0,
    };
    for (n, [x, y, z]) in g.iter().enumerate() {
        let shade = 0.85 + 0.3 * hash01(seed, n as i64, 9);
        b.tri(pts[*x], pts[*y], pts[*z], scale_col(col, shade), 0.0);
    }
    Model {
        tris: b.tris,
        lines: Vec::new(),
        radius: 1.15,
        foot_y: -1.0,
        flame_len: 0.0,
    }
}

/// A plausible fit for a ship that has no stored design (the others of the sky): the hull the
/// role and the id call for, an engine, a reactor and the tools of its trade.
pub fn npc_design(data: &GameData, role: &str, id: u32) -> ShipDesign {
    let pick = |opts: &[&str]| -> usize {
        let o = opts[(hash_coord(0x4E_9C, i64::from(id), 1, 1) % opts.len() as u64) as usize];
        data.hull_index(o).unwrap_or(0)
    };
    let (hull, fit): (usize, &[&str]) = match role {
        "trader" => (
            pick(&["hauler", "freighter", "courier", "shuttle"]),
            &[
                "torch_mk2",
                "fusion_mk2",
                "cargo_mk2",
                "cargo_mk2",
                "tank_mk2",
                "radiator_mk2",
                "habitat_mk1",
                "pulselaser_mk1",
            ],
        ),
        "miner" => (
            pick(&["prospector", "driller"]),
            &[
                "torch_mk2",
                "fusion_mk2",
                "tank_mk2",
                "cargo_mk2",
                "radiator_mk2",
                "laser_mk2",
                "habitat_mk1",
            ],
        ),
        "pirate" => (
            pick(&["fighter", "skiff", "corvette"]),
            &[
                "torch_mk2",
                "fusion_mk2",
                "autocannon_mk1",
                "pulselaser_mk2",
                "missilerack_mk1",
                "armor_mk1",
                "radiator_mk1",
                "tank_mk1",
            ],
        ),
        _ => (
            pick(&["corvette", "fighter", "frigate"]),
            &[
                "torch_mk3",
                "fusion_mk3",
                "pulselaser_mk2",
                "pulselaser_mk2",
                "missilerack_mk1",
                "shield_mk2",
                "armor_mk2",
                "radiator_mk2",
                "scanner_mk1",
                "tank_mk2",
            ],
        ),
    };
    let mut d = ShipDesign::empty(data, hull);
    for m in fit {
        let _ = d.install_first_fit(data, m);
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        GameData::load(&dir).expect("data loads")
    }

    fn starter(d: &GameData) -> ShipDesign {
        ShipDesign::starter(d)
    }

    #[test]
    fn the_model_is_deterministic_and_not_empty() {
        let d = data();
        let des = starter(&d);
        let st = State::idle(&des);
        let (a, b) = (build(&d, &des, &st), build(&d, &des, &st));
        assert!(a.tris.len() > 200, "{}", a.tris.len());
        assert_eq!(a.tris.len(), b.tris.len());
        for (x, y) in a.tris.iter().zip(&b.tris) {
            assert_eq!(x.v, y.v);
            assert_eq!(x.color, y.color);
        }
        for t in &a.tris {
            for p in t.v {
                assert!(p.iter().all(|c| c.is_finite()));
            }
        }
        assert!(a.radius >= hull_length(&d, des.hull) as f32 * 0.5 - 1e-3);
    }

    #[test]
    fn every_hull_builds_a_model_and_modules_change_it() {
        let d = data();
        for h in 0..d.hulls.len() {
            let empty = ShipDesign::empty(&d, h);
            let bare = build(&d, &empty, &State::idle(&empty));
            assert!(bare.tris.len() > 50, "hull {h}");
            let mut full = empty.clone();
            for m in [
                "torch_mk1",
                "fusion_mk1",
                "tank_mk1",
                "cargo_mk1",
                "radiator_mk1",
                "solar_mk1",
                "pulselaser_mk1",
                "armor_mk1",
                "scanner_mk1",
                "shield_mk1",
            ] {
                let _ = full.install_first_fit(&d, m);
            }
            let m = build(&d, &full, &State::idle(&full));
            assert!(
                m.tris.len() > bare.tris.len(),
                "hull {h}: modules add shapes"
            );
        }
        // Two different modules in the same slot do not give the same model.
        let mut a = ShipDesign::empty(&d, d.hull_index("prospector").unwrap());
        let mut b = a.clone();
        a.install_first_fit(&d, "radiator_mk1").unwrap();
        b.install_first_fit(&d, "solar_mk1").unwrap();
        let (ma, mb) = (
            build(&d, &a, &State::idle(&a)),
            build(&d, &b, &State::idle(&b)),
        );
        let sig = |m: &Model| {
            m.tris
                .iter()
                .map(|t| t.color as u64 ^ t.v[0][0].to_bits() as u64)
                .sum::<u64>()
        };
        assert_ne!(sig(&ma), sig(&mb));
    }

    #[test]
    fn the_flame_is_proportional_to_the_thrust() {
        let d = data();
        let des = starter(&d);
        let mut st = State::idle(&des);
        assert_eq!(build(&d, &des, &st).flame_len, 0.0, "no thrust, no flame");
        st.thrust = 0.5;
        let half = build(&d, &des, &st).flame_len;
        st.thrust = 1.0;
        let full = build(&d, &des, &st).flame_len;
        assert!(
            half > 0.0 && (full / half - 2.0).abs() < 0.01,
            "{half} {full}"
        );
        // A wrecked engine burns nothing.
        st.health.fill(0.0);
        assert_eq!(build(&d, &des, &st).flame_len, 0.0);
    }

    #[test]
    fn damage_heat_gear_and_deployment_are_visible() {
        let d = data();
        let des = starter(&d);
        let base = State::idle(&des);
        let m0 = build(&d, &des, &base);
        let mut hurt = base.clone();
        hurt.hull = 0.3;
        assert!(
            build(&d, &des, &hurt).tris.len() > m0.tris.len(),
            "scorch marks"
        );
        let mut gear = base.clone();
        gear.gear = 1.0;
        let mg = build(&d, &des, &gear);
        assert!(mg.foot_y < m0.foot_y - 0.5, "{} {}", mg.foot_y, m0.foot_y);
        let mut folded = base.clone();
        folded.deploy = 0.0;
        let mf = build(&d, &des, &folded);
        let sig = |m: &Model| m.tris.iter().map(|t| t.v[0][0] as f64).sum::<f64>();
        assert!((sig(&mf) - sig(&m0)).abs() > 1e-3, "panels fold");
        let mut sh = base.clone();
        sh.shield = 1.0;
        assert!(
            build(&d, &des, &sh).lines.len() > m0.lines.len(),
            "shield bubble"
        );
        let mut hot = base.clone();
        hot.temp_k = 600.0;
        let col = |m: &Model| m.tris.iter().map(|t| u64::from(t.color)).sum::<u64>();
        assert_ne!(col(&build(&d, &des, &hot)), col(&m0), "radiators glow");
    }

    #[test]
    fn drawing_paints_pixels_in_front_and_hides_what_is_behind() {
        let d = data();
        let des = starter(&d);
        let m = build(&d, &des, &State::idle(&des));
        let cam = Camera {
            pos: Vec3::new(0.0, 5.0, -22.0),
            rot: Mat3::look_at(Vec3::new(0.0, -0.1, 1.0), Vec3::new(0.0, 1.0, 0.0)).transpose(),
            fov_y: 1.0,
        };
        let mut fb = Framebuffer::new(320, 200);
        let mut zb = Zbuf::new(320, 200);
        let pose = Pose {
            pos: Vec3::ZERO,
            rot: Mat3::IDENTITY,
            scale: 1.0,
        };
        let light = Light {
            sun: Vec3::new(0.4, 0.8, -0.3),
            ambient: 0.15,
            intensity: 1.0,
        };
        draw(&mut fb, &mut zb, &cam, &m, &pose, &light);
        let lit = fb.px.iter().filter(|&&p| p != 0).count();
        assert!(lit > 800, "{lit}");
        assert!(zb.inv.iter().any(|&z| z > 0.0));
        // The unlit side is darker than the lit one.
        let dark = Light {
            sun: Vec3::new(0.0, -1.0, 0.0),
            ambient: 0.15,
            intensity: 1.0,
        };
        let mut fb2 = Framebuffer::new(320, 200);
        let mut zb2 = Zbuf::new(320, 200);
        draw(&mut fb2, &mut zb2, &cam, &m, &pose, &dark);
        let sum = |f: &Framebuffer| f.px.iter().map(|&p| u64::from(p >> 16 & 255)).sum::<u64>();
        assert!(sum(&fb2) < sum(&fb), "sun below makes the top dark");
    }

    #[test]
    fn the_turntable_paints_a_ship_inside_its_square() {
        let d = data();
        let des = showcase_design(&d, d.hull_index("corvette").unwrap());
        let mut fb = Framebuffer::new(400, 300);
        turntable(&mut fb, &d, &des, &State::idle(&des), 200, 150, 160, 0.7);
        let lit: Vec<usize> = (0..fb.px.len()).filter(|&i| fb.px[i] != 0).collect();
        assert!(lit.len() > 1500, "{}", lit.len());
        for i in lit {
            let (x, y) = ((i % 400) as i32, (i / 400) as i32);
            assert!((x - 200).abs() <= 80 && (y - 150).abs() <= 80, "{x},{y}");
        }
    }

    #[test]
    fn stations_and_npcs_have_models() {
        let d = data();
        let (hub, ring) = station_parts(500.0);
        assert!(hub.tris.len() + ring.tris.len() > 100);
        assert!(rock_model(3, 0x00A0_9078).tris.len() == 80);
        for role in ["trader", "miner", "pirate", "patrol"] {
            let des = npc_design(&d, role, 3);
            let m = build(&d, &des, &State::idle(&des));
            assert!(m.tris.len() > 100, "{role}");
        }
    }
}
