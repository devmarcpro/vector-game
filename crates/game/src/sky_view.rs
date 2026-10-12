//! Cockpit and map overlays for what astronomy adds to a system (see `vector_world::stellar`):
//! the true rings of the giants (circles in their equatorial plane, where the minable ice blocks
//! orbit), the ion and dust tails of comets blown away from the star, the accretion disc, the
//! lensed halo and the black disc of a black hole, and the lighthouse beams of a pulsar. All of
//! it in lines, like the rest of the cockpit.
use crate::hud::{View, AMBER};
use std::f64::consts::TAU;
use vector_core::Vec3;
use vector_render::{Camera, Framebuffer};
use vector_world::stellar::StarStage;
use vector_world::BodyKind;

const ION: u32 = 0x0060_C0FF;
const DUST: u32 = 0x00E0_E0C0;
const DISC: u32 = 0x00FF_9030;
const HALO: u32 = 0x0060_80FF;
const BEAM: u32 = 0x00C0_E0FF;

fn dimmed(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// A circle of radius `r` in the reference plane around `centre`, in world space.
fn circle3(fb: &mut Framebuffer, cam: &Camera, centre: Vec3, r: f64, col: u32) {
    let n = 64;
    let mut prev: Option<Vec3> = None;
    for k in 0..=n {
        let a = TAU * f64::from(k) / f64::from(n);
        let p = centre + Vec3::new(r * a.cos(), 0.0, r * a.sin());
        if let Some(q) = prev {
            cam.draw_line(fb, q, p, col);
        }
        prev = Some(p);
    }
}

/// Everything astronomy adds to body `i`, drawn after the body itself. `(x, y)` is its place on
/// the screen and `px` its apparent radius.
pub fn overlay(fb: &mut Framebuffer, cam: &Camera, v: &View, i: usize, x: f32, y: f32, px: f64) {
    let (sys, sim) = (&v.sim.system, v.sim);
    let b = &sim.bodies[i];
    // Rings: three circles (the edges and the middle) of the real ring, round the equator.
    if let Some(r) = sys.ring_of(i) {
        if sim.scan[i] > 0 {
            for (k, f) in [(1.0, 0.8), (0.5, 0.5), (0.0, 0.8)] {
                let rad = r.inner + (r.outer - r.inner) * k;
                circle3(fb, cam, b.pos, rad, dimmed(0x00C8_B8A0, f));
            }
        }
    }
    // Comets: the ion tail runs straight away from the star, the dust tail curves behind.
    if let Some(tail) = sys.comet_tail(i, sim.time) {
        let head = b.pos;
        cam.draw_line(fb, head, head + tail, ION);
        let side = tail.cross(sim.bodies[i].vel).normalized() * (tail.length() * 0.12);
        let mut prev = head;
        for k in 1..=8 {
            let s = f64::from(k) / 8.0;
            let p = head + tail * (0.8 * s) + side * (s * s);
            cam.draw_line(fb, prev, p, DUST);
            prev = p;
        }
    }
    // Compact stars and rogue worlds are far too small to see at their true size: a disc of at
    // least a few pixels carries their signs.
    if let BodyKind::Star(_) = b.kind {
        let stage = if i == 0 { Some(sys.stage) } else { None };
        let r = px.max(8.0) as f32;
        match stage {
            Some(StarStage::BlackHole) => {
                fb.circle(x, y, r, 0x0000_0000);
                fb.circle(x, y, r * 0.8, 0x0008_0808);
                // The photon ring and the halo of light bent around it.
                fb.ellipse(x, y, r * 1.3, r * 1.3, HALO);
                fb.ellipse(x, y, r * 2.2, r * 2.2, dimmed(HALO, 0.5));
                if sys.accreting() {
                    for (k, f) in [(2.4, 1.0), (3.2, 0.7), (4.2, 0.4)] {
                        fb.ellipse(x, y, r * k, r * k * 0.28, dimmed(DISC, f));
                    }
                }
            }
            Some(StarStage::NeutronStar) => {
                if let Some(p) = sys.pulsar() {
                    // The beams sweep: shown no faster than one turn in 0.7 s, or they would blur.
                    let a = TAU * sim.time / p.period_s.max(0.7);
                    let l = 90.0 + if p.magnetar { 60.0 } else { 0.0 };
                    for s in [1.0f64, -1.0] {
                        let (dx, dy) = ((a.cos() * s) as f32, (a.sin() * s * 0.4) as f32);
                        fb.line(x, y, x + dx * l as f32, y + dy * l as f32, BEAM);
                        fb.line(
                            x,
                            y,
                            x + dx * l as f32 * 0.6,
                            y + dy * l as f32 * 0.6 + 1.0,
                            dimmed(BEAM, 0.6),
                        );
                    }
                    fb.circle(x, y, r, BEAM);
                }
            }
            Some(StarStage::Rogue) => fb.ellipse(x, y, r * 1.5, r * 1.5, dimmed(0x00A0_3020, 0.7)),
            _ => {}
        }
    }
}

/// Where a ring, a comet tail or a star's sign appears on the system map: the tail as a line.
pub fn map_overlay(fb: &mut Framebuffer, v: &View, i: usize, to_px: &dyn Fn(Vec3) -> (f32, f32)) {
    let sim = v.sim;
    if let Some(tail) = sim.system.comet_tail(i, sim.time) {
        let head = sim.bodies[i].pos;
        let (a, b) = (to_px(head), to_px(head + tail));
        fb.line(a.0, a.1, b.0, b.1, ION);
    }
    if sim.system.ring_of(i).is_some() {
        let (x, y) = to_px(sim.bodies[i].pos);
        fb.ellipse(x, y, 6.0, 6.0, dimmed(AMBER, 0.5));
    }
}
