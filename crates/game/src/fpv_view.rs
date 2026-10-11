//! First-person view of the exploration (F in the exploration screen): the same floors, the same
//! squad and the same turns as the view in characters, seen from the squad's eyes with a
//! raycaster (walls, floor and ceiling) and billboards made of ASCII art for everything that
//! stands in it: people, creatures, machines, trees and plants, loot, terminals and stairs. Left
//! and right turn by an eighth of a circle, up and down step forward and back. A prototype of an
//! exploration in the manner of Daggerfall: one engine, two representations.
use crate::hud::{AMBER, DIM, GREEN};
use vector_data::ROGUE_TERMINALS;
use vector_render::Framebuffer;
use vector_sim::rogue::{Mon, Rogue, T_DOWN, T_FEATURE, T_SITE, T_TERMINAL, T_UP, T_WALL};

/// Window of the view on the screen (pixels).
pub const X0: i32 = 8;
pub const Y0: i32 = 30;
pub const W: i32 = 880;
pub const H: i32 = 520;
/// Half the field of view as the length of the camera plane (0.66 is about 66 degrees).
const PLANE: f64 = 0.66;
const MAX_DIST: f64 = 16.0;

type Art = &'static [&'static str];

// ---- the billboards: ASCII drawings, one string per row (spaces are transparent) ---------------

const HUMAN: Art = &["  _  ", " (o) ", "/|#|\\", " | | ", " / \\ "];
const HUMAN_ARMED: Art = &["  _   ", " (o)  ", "/|#|-+", " | |  ", " / \\  "];
const ROBOT: Art = &[" [=] ", "[o_o]", " |#| ", "_| |_"];
const RAT: Art = &["      ", " ,--.o", "(____>", " ^  ^ "];
const INSECT: Art = &["\\ _ /", " (o) ", "/|#|\\", " ' ' "];
const QUEEN: Art = &[
    "  \\ | /  ",
    " .-(O)-. ",
    "(  |#|  )",
    " '-|#|-' ",
    "   / \\   ",
];
const GOLEM: Art = &[" ____ ", "|o  o|", "|_##_|", " |  | ", "_|__|_"];
const GHOUL: Art = &[" .-. ", "(o_o)", "/|~|\\", " | | ", " ' ' "];
const SENTINEL: Art = &["  /\\  ", " /OO\\ ", "|=##=|", " |  | ", " |__| "];
const MUTANT: Art = &[" ,-. ", "(o.O)", "/|#|\\\\", " / \\ "];
const WISP: Art = &["  .  ", " (*) ", "  '  "];
const CHEST: Art = &[" _____ ", "|=====|", "|_[#]_|"];
const CRAWLER: Art = &["\\\\ // ", " (oo) ", "/ || \\", "  /\\  "];
const WOLF: Art = &[" /\\_/\\  ", "( o.o ) ", " > ^ <__", " |_||_| "];
const TURRET: Art = &["  _|_  ", " [O-O] ", "  |#|  ", " _|_|_ "];
const HORROR: Art = &["  \\|/  ", " (@@@) ", "\\/|#|\\/", "  / \\  ", " ~/ \\~ "];
const AI_CORE: Art = &[" _____ ", "|[o o]|", "|=###=|", "|_____|"];
const BEAST: Art = &["  /\\_/\\_", " ( o  O>", " /|##|\\ ", " ^^  ^^ "];
const PREY: Art = &["   ,_ ", " _/o\\>", "(____)", " ^  ^ "];
const TREE: Art = &[
    "  ###  ", " ##### ", "#######", " ##### ", "   |   ", "   |   ",
];
const PINE: Art = &["   ^   ", "  /^\\  ", " /^^^\\ ", "/^^^^^\\", "   |   "];
const BUSH: Art = &[" .~~. ", "(~~~~)", " '||' "];
const FLOWER: Art = &[" \\|/ ", " -*- ", "  |  "];
const REED: Art = &["| | |", "|\\|/|", " \\|/ "];
const PILE: Art = &["  *  ", " *** ", "*****"];
const CONSOLE: Art = &[" _____ ", "|[X X]|", "|_____|", "  | |  ", "_|___|_"];
const STAIRS_UP: Art = &["  /\\  ", " /  \\ ", "/_  _\\", "  ||  "];
const STAIRS_DOWN: Art = &["  ||  ", "\\_  _/", " \\  / ", "  \\/  "];
const RUIN: Art = &[" _  _ ", "| \\/ |", "|    |", "|_/\\_|"];
const ALTAR: Art = &["  *  ", " /_\\ ", "|___|"];
const FEATURE: Art = &[" .-. ", "( & )", " '-' "];

/// The drawing of a creature, a machine or a person by what it is.
fn art_of(m: &Mon) -> Art {
    if m.friendly {
        return HUMAN;
    }
    match m.glyph {
        'r' => RAT,
        'g' => ROBOT,
        'p' | 'P' => HUMAN_ARMED,
        'd' => INSECT,
        'Q' => QUEEN,
        'G' => GOLEM,
        'h' => GHOUL,
        'S' => SENTINEL,
        'm' => MUTANT,
        'w' => WISP,
        '$' => CHEST,
        'c' => CRAWLER,
        'l' => WOLF,
        'T' => TURRET,
        '@' => HUMAN,
        'X' => HORROR,
        'A' => AI_CORE,
        c if c.is_ascii_uppercase() => BEAST,
        _ => PREY,
    }
}

/// A plant of the ground (a tree, a bush, a flower, a reed) by where it grows.
fn plant_art(x: i32, y: i32) -> Art {
    let h = vector_core::hash_coord(5, i64::from(x), i64::from(y), 2) % 8;
    match h {
        0..=2 => TREE,
        3 => PINE,
        4 | 5 => BUSH,
        6 => FLOWER,
        _ => REED,
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
    art: Art,
    color: u32,
    /// Height of the drawing as a share of the height of a wall.
    size: f64,
    /// A letter to put on the drawing (the label of a terminal).
    mark: Option<char>,
}

pub fn draw(fb: &mut Framebuffer, r: &Rogue, dir: i32, wall_color: u32, sky: bool) {
    let a = f64::from(dir) * std::f64::consts::FRAC_PI_4;
    let (dx, dy) = (a.cos(), a.sin());
    let (px, py) = (f64::from(r.x) + 0.5, f64::from(r.y) + 0.5);
    let (plx, ply) = (-dy * PLANE, dx * PLANE);
    // Ceiling (or sky) and floor, in bands.
    for y in 0..H {
        let t = f64::from(y) / f64::from(H);
        let c = if t < 0.5 {
            let k = 1.0 - t * 2.0;
            if sky {
                shade(0x0040_70C0, 0.35 + 0.5 * (1.0 - k))
            } else {
                shade(0x0030_3840, 0.15 + 0.5 * (1.0 - k))
            }
        } else {
            let k = (t - 0.5) * 2.0;
            shade(0x0030_4830, 0.12 + 0.55 * k)
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
        // Where along the wall the ray hit, for the pattern.
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
    // Billboards, farthest first, hidden behind the walls that are nearer.
    let f = &r.floors[r.cur];
    let mut sprites: Vec<Sprite> = Vec::new();
    for m in &f.monsters {
        sprites.push(Sprite {
            x: f64::from(m.x) + 0.5,
            y: f64::from(m.y) + 0.5,
            art: art_of(m),
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
            let tall = art.len() >= 5;
            (art, 0x0050_D070, if tall { 1.15 } else { 0.55 })
        } else {
            (PILE, AMBER, 0.3)
        };
        sprites.push(Sprite {
            x: f64::from(it.x) + 0.5,
            y: f64::from(it.y) + 0.5,
            art,
            color,
            size,
            mark: None,
        });
    }
    for ty in 0..r.h as i32 {
        for tx in 0..r.w as i32 {
            let t = r.tile(tx, ty);
            let g: Option<(Art, u32, f64, Option<char>)> = if t == T_UP {
                Some((STAIRS_UP, AMBER, 0.9, None))
            } else if t == T_DOWN {
                Some((STAIRS_DOWN, AMBER, 0.9, None))
            } else if t == T_SITE {
                Some((RUIN, AMBER, 1.0, None))
            } else if t >= T_TERMINAL {
                ROGUE_TERMINALS
                    .get((t - T_TERMINAL) as usize)
                    .map(|d| (CONSOLE, AMBER, 0.9, Some(d.2)))
            } else if t >= T_FEATURE {
                Some((
                    (if t.is_multiple_of(2) { ALTAR } else { FEATURE }),
                    0x0060_C0FF,
                    0.6,
                    None,
                ))
            } else {
                None
            };
            if let Some((art, color, size, mark)) = g {
                let (ddx, ddy) = (f64::from(tx) + 0.5 - px, f64::from(ty) + 0.5 - py);
                if ddx * ddx + ddy * ddy < MAX_DIST * MAX_DIST
                    && (ddx != 0.0 || ddy != 0.0)
                    && f.seen[r.idx(tx, ty)]
                {
                    sprites.push(Sprite {
                        x: f64::from(tx) + 0.5,
                        y: f64::from(ty) + 0.5,
                        art,
                        color,
                        size,
                        mark,
                    });
                }
            }
        }
    }
    sprites.sort_by(|a, b| {
        let da = (a.x - px).powi(2) + (a.y - py).powi(2);
        let db = (b.x - px).powi(2) + (b.y - py).powi(2);
        db.total_cmp(&da)
    });
    let inv = 1.0 / (plx * dy - dx * ply);
    for s in &sprites {
        let (rx, ry) = (s.x - px, s.y - py);
        let tx = inv * (dy * rx - dx * ry);
        let ty = inv * (-ply * rx + plx * ry);
        if !(0.2..=MAX_DIST).contains(&ty) {
            continue;
        }
        let screen_x = f64::from(W) / 2.0 * (1.0 + tx / ty);
        let col = (screen_x / 2.0) as i32;
        if !(0..W / 2).contains(&col) || zbuf[col as usize] < ty {
            continue;
        }
        // The drawing stands on the floor: its height is a share of a wall's, its feet at the
        // bottom of the wall in that direction.
        let wall_px = f64::from(H) / ty;
        let height_px = (wall_px * s.size).min(f64::from(H) * 2.0);
        let rows = s.art.len() as f64;
        let scale = ((height_px / rows / 8.0).round() as i32).clamp(1, 16);
        let cols = s.art.iter().map(|l| l.chars().count()).max().unwrap_or(1) as i32;
        let (w_px, h_px) = (cols * 6 * scale, s.art.len() as i32 * 8 * scale);
        let feet = f64::from(H) / 2.0 + wall_px * 0.5;
        let (gx, gy) = (X0 + screen_x as i32 - w_px / 2, Y0 + feet as i32 - h_px);
        let fade = (1.0 / (1.0 + ty * ty * 0.02)).max(0.3);
        let color = shade(s.color, fade);
        for (k, line) in s.art.iter().enumerate() {
            let line: String = match (s.mark, line.contains('X')) {
                (Some(m), true) => line.replace('X', &m.to_string()),
                _ => (*line).to_string(),
            };
            fb.text_big(gx, gy + k as i32 * 8 * scale, &line, scale, color);
        }
    }
    // A crosshair and the way you face.
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
