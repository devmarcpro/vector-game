//! The floodlights of the ship, seen from outside: two lamps at the bow, a cone of light ahead
//! and a glow. The switch is `Sim::lights_on` (see `vector_sim::switches`): lit, the ship also
//! shouts to the passive sensors of the pirates and the patrols.
use crate::hud::View;
use vector_render::{Camera, Framebuffer};

/// Warm white of the beams (dim: lines are additive).
const BEAM: u32 = 0x0058_5030;
const GLOW: u32 = 0x00F0_E8B0;

/// Draw the lights of the player's ship in the third-person view.
pub fn draw(fb: &mut Framebuffer, cam: &Camera, v: &View) {
    let sim = v.sim;
    if !sim.lights_on {
        return;
    }
    let ship = &sim.ship;
    let len = crate::ext_view::ship_length(sim);
    let (fwd, right, up) = (ship.forward(), ship.rot.col(0), ship.rot.col(1));
    let reach = (len * 5.0).max(300.0);
    for side in [-1.0, 1.0] {
        let p0 = ship.pos + fwd * (len * 0.46) + right * (side * len * 0.09) - up * (len * 0.04);
        for (du, dr) in [
            (0.0, 0.0),
            (0.22, 0.0),
            (-0.22, 0.0),
            (0.0, 0.22),
            (0.0, -0.22),
        ] {
            let dir = fwd + up * du + right * (dr + side * 0.04);
            cam.draw_line(fb, p0, p0 + dir.normalized() * reach, BEAM);
        }
        if let Some((x, y)) = cam.project_world(p0, fb) {
            fb.circle(x, y, 3.0, GLOW);
            fb.circle(x, y, 6.0, 0x0040_3C20);
        }
    }
}
