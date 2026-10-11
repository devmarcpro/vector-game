//! First-person view of the exploration (F in the exploration screen): the same floors, the same
//! squad and the same turns as the view in characters, seen from the squad's eyes. Indoors a
//! raycaster draws the walls, floor and ceiling; on the surface of a world the same ground as the
//! cockpit sees from orbit (see `terrain`) is marched from the squad's feet to the horizon. In
//! both, everything that stands in it is a billboard made of ASCII art: people, creatures,
//! machines, trees and plants, loot, terminals, stairs, rocks. Left and right turn by an eighth
//! of a circle, up and down step forward and back. One engine, two representations.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN};
use vector_data::ROGUE_TERMINALS;
use vector_render::Framebuffer;
use vector_sim::rogue::{Mon, Rogue, T_DOWN, T_FEATURE, T_SITE, T_TERMINAL, T_UP, T_WALL, T_WATER};

/// Window of the view on the screen (pixels).
pub const X0: i32 = 8;
pub const Y0: i32 = 30;
pub const W: i32 = 880;
pub const H: i32 = 520;
/// Half the field of view as the length of the camera plane (0.66 is about 66 degrees).
const PLANE: f64 = 0.66;
const MAX_DIST: f64 = 16.0;
/// Metres of one tile of a surface, and the height a billboard of size 1 stands (m).
pub const TILE_M: f64 = 3.0;
const UNIT_M: f64 = 2.4;
const EYE_M: f64 = 1.7;

use crate::sprites::{self, Kind};

/// The drawing of a creature, a machine or a person by what it is.
fn art_of(m: &Mon) -> Kind {
    if m.friendly {
        return Kind::Human;
    }
    match m.glyph {
        'r' => Kind::Rat,
        'g' => Kind::Robot,
        'p' | 'P' => Kind::Armed,
        'd' => Kind::Insect,
        'Q' => Kind::Queen,
        'G' => Kind::Golem,
        'h' => Kind::Ghoul,
        'S' => Kind::Sentinel,
        'm' => Kind::Mutant,
        'w' => Kind::Wisp,
        '$' => Kind::Chest,
        'c' => Kind::Crawler,
        'l' => Kind::Wolf,
        'T' => Kind::Turret,
        '@' => Kind::Human,
        'X' => Kind::Horror,
        'A' => Kind::AiCore,
        c if c.is_ascii_uppercase() => Kind::Beast,
        _ => Kind::Prey,
    }
}

/// A plant of the ground (a tree, a bush, a flower, a reed) by where it grows.
fn plant_art(x: i32, y: i32) -> Kind {
    let h = vector_core::hash_coord(5, i64::from(x), i64::from(y), 2) % 8;
    match h {
        0..=2 => Kind::Tree,
        3 => Kind::Pine,
        4 | 5 => Kind::Bush,
        6 => Kind::Flower,
        _ => Kind::Reed,
    }
}

/// Facing `dir` (0..8, in eighths of a circle, 0 east, 2 south) as a step on the grid.
pub fn step_of(dir: i32) -> (i32, i32) {
    const S: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    S[dir.rem_euclid(8) as usize]
}

fn shade(c: u32, k: f64) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f64 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// The direction (0..8) with the longest open run: where to look first.
pub fn open_direction(r: &Rogue) -> i32 {
    (0..8)
        .max_by_key(|&d| {
            let (dx, dy) = step_of(d);
            (1..30)
                .take_while(|k| r.walkable(r.x + dx * k, r.y + dy * k))
                .count()
        })
        .unwrap_or(0)
}

struct Sprite {
    x: f64,
    y: f64,
    kind: Kind,
    color: u32,
    /// Height of the drawing as a share of the height of a wall (a unit of 2.4 m outdoors).
    size: f64,
    /// A letter to put on the drawing (the label of a terminal).
    mark: Option<char>,
}

/// Everything that stands on the floor, as billboards. On a surface the rocks and ponds of the
/// ground near the squad stand too.
fn collect_sprites(r: &Rogue, px: f64, py: f64, reach: f64) -> Vec<Sprite> {
    let f = &r.floors[r.cur];
    let mut sprites: Vec<Sprite> = Vec::new();
    for m in &f.monsters {
        sprites.push(Sprite {
            x: f64::from(m.x) + 0.5,
            y: f64::from(m.y) + 0.5,
            kind: art_of(m),
            color: if m.boss {
                0x00FF_40FF
            } else if m.friendly {
                0x0060_E0E0
            } else if !m.hunts {
                0x0090_D090
            } else {
                0x00FF_8070
            },
            size: if m.boss { 1.2 } else { 0.8 },
            mark: None,
        });
    }
    for it in &f.items {
        let (art, color, size) = if it.plant {
            let art = plant_art(it.x, it.y);
            let tall = matches!(art, Kind::Tree | Kind::Pine);
            (art, 0x0050_D070, if tall { 1.15 } else { 0.55 })
        } else {
            (Kind::Pile, AMBER, 0.3)
        };
        sprites.push(Sprite {
            x: f64::from(it.x) + 0.5,
            y: f64::from(it.y) + 0.5,
            kind: art,
            color,
            size,
            mark: None,
        });
    }
    for ty in 0..r.h as i32 {
        for tx in 0..r.w as i32 {
            let (ddx, ddy) = (f64::from(tx) + 0.5 - px, f64::from(ty) + 0.5 - py);
            if ddx * ddx + ddy * ddy >= reach * reach || (ddx == 0.0 && ddy == 0.0) {
                continue;
            }
            let t = r.tile(tx, ty);
            let g: Option<(Kind, u32, f64, Option<char>)> = if t == T_UP {
                Some((Kind::StairsUp, AMBER, 0.9, None))
            } else if t == T_DOWN {
                Some((Kind::StairsDown, AMBER, 0.9, None))
            } else if t == T_SITE {
                Some((Kind::Ruin, AMBER, 1.0, None))
            } else if t >= T_TERMINAL {
                ROGUE_TERMINALS
                    .get((t - T_TERMINAL) as usize)
                    .map(|d| (Kind::Console, AMBER, 0.9, Some(d.2)))
            } else if t >= T_FEATURE {
                Some((
                    if t.is_multiple_of(2) {
                        Kind::Altar
                    } else {
                        Kind::Feature
                    },
                    0x0060_C0FF,
                    0.6,
                    None,
                ))
            } else if r.on_surface() && t == T_WALL {
                Some((Kind::Rock, 0x0090_8880, 0.7, None))
            } else if r.on_surface() && t == T_WATER {
                Some((Kind::Pond, 0x0040_80E0, 0.12, None))
            } else {
                None
            };
            if let Some((art, color, size, mark)) = g {
                if f.seen[r.idx(tx, ty)] {
                    sprites.push(Sprite {
                        x: f64::from(tx) + 0.5,
                        y: f64::from(ty) + 0.5,
                        kind: art,
                        color,
                        size,
                        mark,
                    });
                }
            }
        }
    }
    sprites
}

/// Where a billboard shows on the screen.
struct Placed {
    screen_x: f64,
    /// Distance from the eye (for the order, the fade and the hiding).
    depth: f64,
    /// Pixels of one unit of height at that distance.
    unit_px: f64,
    /// Row of the feet, in the window.
    feet_y: f64,
}

/// Draw the billboards, farthest first. `place` puts one on the screen (None when it is behind
/// or out of range); `hidden` tells whether something nearer covers it.
fn draw_sprites(
    fb: &mut Framebuffer,
    sprites: &[Sprite],
    place: &dyn Fn(&Sprite) -> Option<Placed>,
    hidden: &dyn Fn(i32, &Placed) -> bool,
    far: f64,
) {
    let mut placed: Vec<(&Sprite, Placed)> = sprites
        .iter()
        .filter_map(|s| place(s).map(|p| (s, p)))
        .collect();
    placed.sort_by(|a, b| b.1.depth.total_cmp(&a.1.depth));
    for (s, p) in placed {
        let col = (p.screen_x / 2.0) as i32;
        if !(0..W / 2).contains(&col) || hidden(col, &p) {
            continue;
        }
        let height_px = (p.unit_px * s.size).min(f64::from(H) * 2.0);
        let fade = (1.0 / (1.0 + (p.depth / far).powi(2) * 4.0)).max(0.3);
        let color = shade(s.color, fade);
        sprites::draw(
            fb,
            s.kind,
            X0 + p.screen_x as i32,
            Y0 + p.feet_y as i32,
            height_px,
            color,
            s.mark,
        );
    }
}

fn crosshair(fb: &mut Framebuffer, dir: i32) {
    let (cx, cy) = (X0 + W / 2, Y0 + H / 2);
    for d in [-6, -5, -4, 4, 5, 6] {
        fb.plot(cx + d, cy, GREEN);
        fb.plot(cx, cy + d, GREEN);
    }
    let names = ["E", "SE", "S", "SO", "O", "NO", "N", "NE"];
    fb.text_at(
        X0 + 4,
        Y0 + H - 12,
        &format!("CAP {}", names[dir.rem_euclid(8) as usize]),
        DIM,
    );
}

/// Indoors: walls by raycasting, billboards for everything else.
pub fn draw(fb: &mut Framebuffer, r: &Rogue, dir: i32, wall_color: u32) {
    let a = f64::from(dir) * std::f64::consts::FRAC_PI_4;
    let (dx, dy) = (a.cos(), a.sin());
    let (px, py) = (f64::from(r.x) + 0.5, f64::from(r.y) + 0.5);
    let (plx, ply) = (-dy * PLANE, dx * PLANE);
    // Ceiling and floor, in bands.
    for y in 0..H {
        let t = f64::from(y) / f64::from(H);
        let c = if t < 0.5 {
            shade(0x0030_3840, 0.15 + 0.5 * (t * 2.0))
        } else {
            shade(0x0030_4830, 0.12 + 0.55 * ((t - 0.5) * 2.0))
        };
        for x in 0..W {
            fb.plot(X0 + x, Y0 + y, c);
        }
    }
    // Walls, a column pair at a time.
    let mut zbuf = vec![MAX_DIST; (W / 2 + 1) as usize];
    for col in 0..W / 2 {
        let camx = 2.0 * f64::from(col) / f64::from(W / 2) - 1.0;
        let (rx, ry) = (dx + plx * camx, dy + ply * camx);
        let (mut mx, mut my) = (r.x, r.y);
        let (ddx, ddy) = (
            if rx == 0.0 { 1e30 } else { (1.0 / rx).abs() },
            if ry == 0.0 { 1e30 } else { (1.0 / ry).abs() },
        );
        let (sx, mut sdx) = if rx < 0.0 {
            (-1, (px - f64::from(mx)) * ddx)
        } else {
            (1, (f64::from(mx) + 1.0 - px) * ddx)
        };
        let (sy, mut sdy) = if ry < 0.0 {
            (-1, (py - f64::from(my)) * ddy)
        } else {
            (1, (f64::from(my) + 1.0 - py) * ddy)
        };
        let mut side = 0;
        let mut hit = false;
        for _ in 0..40 {
            if sdx < sdy {
                sdx += ddx;
                mx += sx;
                side = 0;
            } else {
                sdy += ddy;
                my += sy;
                side = 1;
            }
            if !r.inside(mx, my) || r.tile(mx, my) == T_WALL {
                hit = true;
                break;
            }
        }
        if !hit {
            continue;
        }
        let dist = if side == 0 { sdx - ddx } else { sdy - ddy }.max(0.05);
        zbuf[col as usize] = dist;
        if dist >= MAX_DIST {
            continue;
        }
        let height = (f64::from(H) / dist).min(f64::from(H) * 3.0);
        let top = (f64::from(H) / 2.0 - height / 2.0) as i32;
        let wall_x = if side == 0 {
            py + dist * ry
        } else {
            px + dist * rx
        };
        let u = wall_x - wall_x.floor();
        let fade = 1.0 / (1.0 + dist * dist * 0.03);
        let face = if side == 0 { 1.0 } else { 0.72 };
        for yy in top.max(0)..(top + height as i32).min(H) {
            let v = f64::from(yy - top) / height;
            // Courses of bricks: dark joints every eighth of the wall, staggered across.
            let stagger = if (v * 8.0) as i32 % 2 == 0 {
                0.0
            } else {
                0.125
            };
            let joint = (v * 8.0).fract() < 0.08 || ((u + stagger) * 4.0).fract() < 0.05;
            let k = fade * face * if joint { 0.55 } else { 1.0 };
            let c = shade(wall_color, k);
            let x = X0 + col * 2;
            fb.plot(x, Y0 + yy, c);
            fb.plot(x + 1, Y0 + yy, c);
        }
    }
    let sprites = collect_sprites(r, px, py, MAX_DIST);
    let inv = 1.0 / (plx * dy - dx * ply);
    draw_sprites(
        fb,
        &sprites,
        &|s| {
            let (rx, ry) = (s.x - px, s.y - py);
            let tx = inv * (dy * rx - dx * ry);
            let ty = inv * (-ply * rx + plx * ry);
            if !(0.2..=MAX_DIST).contains(&ty) {
                return None;
            }
            let unit_px = f64::from(H) / ty;
            Some(Placed {
                screen_x: f64::from(W) / 2.0 * (1.0 + tx / ty),
                depth: ty,
                unit_px,
                feet_y: f64::from(H) / 2.0 + unit_px * 0.5,
            })
        },
        &|col, p| zbuf[col as usize] < p.depth,
        MAX_DIST,
    );
    crosshair(fb, dir);
}

/// On the surface of a world: the ground of the planet marched from the squad's feet, lit by
/// the star under the air of the world, with the billboards standing on it.
pub fn draw_surface(fb: &mut Framebuffer, g: &Game, r: &Rogue, dir: i32) {
    let (Some(body), Some((cx, cy))) = (r.body, r.land) else {
        return;
    };
    let Some(map) = g.flight_map(body) else {
        return;
    };
    let b = &g.sim.bodies[body];
    let t = crate::terrain::Terrain::new(&map, b.radius);
    let (lat0, lon0) = (
        vector_world::surface::Surface::lat(cy),
        vector_world::surface::Surface::lon(cx),
    );
    let cos_lat = lat0.cos().abs().max(0.05);
    // The squad stands on the grid; the grid is laid on the ground around the landing.
    let (landing_x, landing_y) = (
        f64::from(r.floors[0].start.0) + 0.5,
        f64::from(r.floors[0].start.1) + 0.5,
    );
    let to_ll = |gx: f64, gy: f64| -> (f64, f64) {
        let (east, north) = ((gx - landing_x) * TILE_M, -(gy - landing_y) * TILE_M);
        (lat0 + north / b.radius, lon0 + east / (b.radius * cos_lat))
    };
    let (px, py) = (f64::from(r.x) + 0.5, f64::from(r.y) + 0.5);
    let (lat, lon) = to_ll(px, py);
    let ground = f64::from(t.sample(lat, lon, 1.0).0.max(0.0));
    let (sx, sy) = step_of(dir);
    let yaw = f64::from(sx).atan2(-f64::from(sy));
    let focal = f64::from(W) / 2.0 / PLANE;
    // The light of the star in the frame of this place.
    let angle = std::f64::consts::TAU * g.sim.time / g.sim.system.spin_period(body).max(60.0);
    let orient =
        vector_core::Mat3::rot_z(f64::from(map.surface.tilt)).mul(&vector_core::Mat3::rot_y(angle));
    let nb = |lat: f64, lon: f64| {
        orient.mul_vec(vector_core::Vec3::new(
            lat.cos() * lon.cos(),
            lat.sin(),
            lat.cos() * lon.sin(),
        ))
    };
    let up = nb(lat0, lon0);
    let east = orient.mul_vec(vector_core::Vec3::new(-lon0.sin(), 0.0, lon0.cos()));
    let north = orient.mul_vec(vector_core::Vec3::new(
        -lat0.sin() * lon0.cos(),
        lat0.cos(),
        -lat0.sin() * lon0.sin(),
    ));
    let sun = (b.pos * -1.0).normalized();
    let air = g.sim.air_of(body);
    let data = &g.sim.data;
    let far = 20_000.0;
    let depth = crate::terrain::render(
        fb,
        &t,
        &|x, y| data.biomes[map.biome_at(x, y)].color,
        &crate::terrain::Eye {
            lat,
            lon,
            height: ground + EYE_M,
            yaw,
            pitch: 0.0,
        },
        &crate::terrain::Frame {
            x0: X0,
            y0: Y0,
            w: W,
            h: H,
            focal,
        },
        &crate::terrain::Sky {
            air: air
                .as_ref()
                .filter(|a| a.ground > 0.05)
                .map(|_| (0x0030_60B0, 0x00A8_C8E8)),
            haze: 40_000.0 / map.pressure.max(0.1),
            scale: air.as_ref().map_or(8000.0, |a| a.height),
        },
        &crate::terrain::Light {
            up: sun.dot(up),
            east: sun.dot(east),
            north: sun.dot(north),
        },
        far,
    );
    let sprites = collect_sprites(r, px, py, 14.0);
    let horizon = f64::from(H) / 2.0;
    draw_sprites(
        fb,
        &sprites,
        &|s| {
            let (e, n) = ((s.x - px) * TILE_M, -(s.y - py) * TILE_M);
            let fwd = e * yaw.sin() + n * yaw.cos();
            let right = e * yaw.cos() - n * yaw.sin();
            if fwd < 0.6 {
                return None;
            }
            let (sla, slo) = to_ll(s.x, s.y);
            let gh = f64::from(t.sample(sla, slo, 1.0).0.max(0.0));
            let unit_px = focal * UNIT_M / fwd;
            Some(Placed {
                screen_x: f64::from(W) / 2.0 + focal * right / fwd,
                depth: e.hypot(n),
                unit_px,
                feet_y: horizon + focal * (ground + EYE_M - gh) / fwd,
            })
        },
        &|col, p| {
            let y = (p.feet_y - 2.0).clamp(0.0, f64::from(H) - 1.0) as i32;
            f64::from(depth.at(col, y)) < p.depth * 0.97
        },
        far.min(60.0),
    );
    crosshair(fb, dir);
}
