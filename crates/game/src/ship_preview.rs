//! Filled preview of a ship made of cells, turning under the mouse: every cell is a small solid
//! (flat shaded, sorted far to near, opaque so it does not blow out like the additive lines).
//! Used by the ship designer; it only needs a list of coloured solids, so any other view of a
//! ship (a third-person camera, a codex card) can reuse `draw_solids`.
use vector_data::ship::CELL_M;
use vector_data::GameData;
use vector_render::Framebuffer;
use vector_sim::shipwright::Blueprint;

/// One solid: centre in cells (x forward, y starboard, z up), half sizes in cells, colour, and
/// how much the front face shrinks (1 = a box, 0.3 = a nose cone).
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub c: [f32; 3],
    pub half: [f32; 3],
    pub color: u32,
    pub taper: f32,
}

/// Colour and shape of a part by its kind (the data says which kind it is).
pub fn solid_of(kind: &str, id: &str, c: [f32; 3]) -> Solid {
    let (color, half, taper) = match kind {
        "cockpit" => (0x00E0_A020, [0.46, 0.4, 0.4], 1.0),
        "hull" if id.contains("nose") => (0x0090_B0D0, [0.46, 0.4, 0.4], 0.3),
        "hull" if id.contains("truss") => (0x0070_8090, [0.4, 0.4, 0.4], 1.0),
        "hull" if id.contains("armor") => (0x0060_7080, [0.5, 0.5, 0.5], 1.0),
        "hull" => (0x0070_A0C0, [0.46, 0.4, 0.4], 1.0),
        "deck" => (0x0040_C060, [0.46, 0.4, 0.4], 1.0),
        "hold" => (0x00B0_8040, [0.46, 0.42, 0.42], 1.0),
        "bay" => (0x0090_60C0, [0.46, 0.42, 0.42], 1.0),
        "engine" => (0x00D0_5040, [0.4, 0.34, 0.34], 0.6),
        "nacelle" => (0x00E0_7030, [0.46, 0.22, 0.22], 0.5),
        "wing" => (0x0050_C0D0, [0.5, 0.5, 0.06], 1.0),
        "gear" => (0x00C0_C0C0, [0.1, 0.1, 0.46], 1.0),
        _ => (0x0080_80A0, [0.46, 0.46, 0.14], 1.0),
    };
    Solid {
        c,
        half,
        color,
        taper,
    }
}

/// The solids of a blueprint, centred on its middle.
pub fn solids_of(data: &GameData, bp: &Blueprint) -> Vec<Solid> {
    if bp.cells.is_empty() {
        return Vec::new();
    }
    let n = bp.cells.len() as f32;
    let mid = bp.cells.iter().fold([0.0f32; 3], |a, c| {
        [
            a[0] + f32::from(c.x) / n,
            a[1] + f32::from(c.y) / n,
            a[2] + f32::from(c.z) / n,
        ]
    });
    bp.cells
        .iter()
        .filter_map(|c| {
            let p = &data.ship_parts[data.ship_part_index(&c.part)?];
            Some(solid_of(
                &p.kind,
                &p.id,
                [
                    f32::from(c.x) - mid[0],
                    f32::from(c.y) - mid[1],
                    f32::from(c.z) - mid[2],
                ],
            ))
        })
        .collect()
}

fn shade(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 255) as f32 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn line_over(fb: &mut Framebuffer, a: (f32, f32), b: (f32, f32), c: u32) {
    let (mut x, mut y) = (a.0.round() as i32, a.1.round() as i32);
    let (xe, ye) = (b.0.round() as i32, b.1.round() as i32);
    let dx = (xe - x).abs();
    let dy = -(ye - y).abs();
    let (sx, sy) = (if x < xe { 1 } else { -1 }, if y < ye { 1 } else { -1 });
    let mut err = dx + dy;
    for _ in 0..4096 {
        fb.px_set(x, y, c);
        if x == xe && y == ye {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Fill a convex polygon (opaque), clipped to `clip` (left, top, right, bottom).
fn fill_poly(fb: &mut Framebuffer, pts: &[(f32, f32)], c: u32, clip: (i32, i32, i32, i32)) {
    let ymin = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32;
    let ymax = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32;
    for y in ymin.max(clip.1)..=ymax.min(clip.3 - 1) {
        let yc = y as f32 + 0.5;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc) {
                let t = (yc - a.1) / (b.1 - a.1);
                let x = a.0 + t * (b.0 - a.0);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        if lo <= hi {
            for x in (lo.floor() as i32).max(clip.0)..=(hi.ceil() as i32).min(clip.2 - 1) {
                fb.px_set(x, y, c);
            }
        }
    }
}

/// The six faces of a solid as corner indices and outward normals; corners are numbered by the
/// bits (x, y, z) of their index: bit 0 = +x, bit 1 = +y, bit 2 = +z.
const FACES: [([usize; 4], [f32; 3]); 6] = [
    ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
    ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
    ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
    ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
    ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
];

/// Draw `solids` inside `rect`, turned by `yaw` (about the vertical) and `pitch` (the camera
/// going up), scaled to fit. `mark` is the index of a solid to outline in white.
pub fn draw_solids(
    fb: &mut Framebuffer,
    solids: &[Solid],
    rect: (i32, i32, i32, i32),
    yaw: f32,
    pitch: f32,
    mark: Option<usize>,
) {
    if solids.is_empty() {
        return;
    }
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // To view space: yaw about z, then pitch about x. The camera looks along +y.
    let rot = |v: [f32; 3]| -> [f32; 3] {
        let (x1, y1) = (v[0] * cy - v[1] * sy, v[0] * sy + v[1] * cy);
        [x1, y1 * cp - v[2] * sp, y1 * sp + v[2] * cp]
    };
    let radius = solids
        .iter()
        .map(|s| {
            let d = (s.c[0].powi(2) + s.c[1].powi(2) + s.c[2].powi(2)).sqrt();
            d + 0.9
        })
        .fold(1.0f32, f32::max);
    let (w, h) = ((rect.2 - rect.0) as f32, (rect.3 - rect.1) as f32);
    let scale = (w.min(h) * 0.5 / radius).min(60.0);
    let (ox, oy) = (
        (rect.0 + rect.2) as f32 / 2.0,
        (rect.1 + rect.3) as f32 / 2.0,
    );
    let to_screen = |v: [f32; 3]| -> (f32, f32) {
        let r = rot(v);
        (ox + r[0] * scale, oy - r[2] * scale)
    };
    let mut order: Vec<usize> = (0..solids.len()).collect();
    // Far to near: a larger y in view space is farther from the camera.
    order.sort_by(|&a, &b| {
        rot(solids[b].c)[1]
            .partial_cmp(&rot(solids[a].c)[1])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let light = [-0.35f32, -0.6, 0.72];
    for &i in &order {
        let s = &solids[i];
        let mut corners = [[0.0f32; 3]; 8];
        for (k, c) in corners.iter_mut().enumerate() {
            let (sx, sy_, sz) = (
                if k & 1 == 1 { 1.0 } else { -1.0 },
                if k & 2 == 2 { 1.0 } else { -1.0 },
                if k & 4 == 4 { 1.0 } else { -1.0 },
            );
            // The front (+x) face of a nose or an engine bell is narrower.
            let t = if k & 1 == 1 { s.taper } else { 1.0 };
            *c = [
                s.c[0] + sx * s.half[0],
                s.c[1] + sy_ * s.half[1] * t,
                s.c[2] + sz * s.half[2] * t,
            ];
        }
        for (idx, n) in FACES {
            let nv = rot(n);
            if nv[1] >= -0.02 {
                continue;
            }
            let pts: Vec<(f32, f32)> = idx.iter().map(|&k| to_screen(corners[k])).collect();
            let lit = (nv[0] * light[0] + nv[1] * light[1] + nv[2] * light[2]).max(0.0);
            fill_poly(fb, &pts, shade(s.color, 0.35 + 0.65 * lit), rect);
            let edge = if mark == Some(i) {
                0x00FF_FFFF
            } else {
                shade(s.color, 0.25)
            };
            for k in 0..4 {
                let (a, b) = (pts[k], pts[(k + 1) % 4]);
                let inside = |p: (f32, f32)| {
                    p.0 >= rect.0 as f32
                        && p.0 < rect.2 as f32
                        && p.1 >= rect.1 as f32
                        && p.1 < rect.3 as f32
                };
                if inside(a) && inside(b) {
                    line_over(fb, a, b, edge);
                }
            }
        }
    }
    let _ = CELL_M;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_is_drawn_inside_its_rectangle_and_nothing_outside() {
        let mut fb = Framebuffer::new(200, 120);
        let s = [solid_of("hull", "hull_frame", [0.0, 0.0, 0.0])];
        draw_solids(&mut fb, &s, (50, 20, 150, 100), 0.5, 0.4, None);
        let lit = |x: usize, y: usize| fb.px[y * fb.w + x] != 0;
        let inside = (20..100).any(|y| (50..150).any(|x| lit(x, y)));
        assert!(inside, "something is drawn");
        for y in 0..120 {
            for x in 0..200 {
                if !(50..150).contains(&x) || !(20..100).contains(&y) {
                    assert!(!lit(x, y), "pixel ({x},{y}) outside the rectangle");
                }
            }
        }
    }

    #[test]
    fn the_nearer_solid_hides_the_farther_one() {
        // Two boxes along the line of sight (the camera looks along +y after a quarter turn).
        let near = Solid {
            c: [0.0, -2.0, 0.0],
            half: [0.4, 0.4, 0.4],
            color: 0x00FF_0000,
            taper: 1.0,
        };
        let far = Solid {
            c: [0.0, 2.0, 0.0],
            half: [0.4, 0.4, 0.4],
            color: 0x0000_00FF,
            taper: 1.0,
        };
        let mut fb = Framebuffer::new(100, 100);
        draw_solids(&mut fb, &[far, near], (0, 0, 100, 100), 0.0, 0.0, None);
        let c = fb.px[50 * 100 + 50];
        assert!(
            c >> 16 > c & 255,
            "red (near) wins over blue (far): {c:06x}"
        );
        let mut fb = Framebuffer::new(100, 100);
        draw_solids(&mut fb, &[near, far], (0, 0, 100, 100), 0.0, 0.0, None);
        assert_eq!(
            c,
            fb.px[50 * 100 + 50],
            "the order of the list does not matter"
        );
    }

    #[test]
    fn the_camera_angle_changes_the_picture() {
        let s = [
            solid_of("hull", "hull_frame", [-1.0, 0.0, 0.0]),
            solid_of("cockpit", "core_cockpit", [1.0, 0.0, 0.0]),
        ];
        let draw = |yaw: f32| {
            let mut fb = Framebuffer::new(100, 100);
            draw_solids(&mut fb, &s, (0, 0, 100, 100), yaw, 0.3, None);
            fb.px
        };
        assert_ne!(draw(0.0), draw(1.2));
        assert_eq!(draw(0.7), draw(0.7));
    }
}
