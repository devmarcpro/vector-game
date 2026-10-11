//! Billboards made of characters, full ones: every creature, person, machine, plant and thing is
//! a silhouette drawn by shapes (ellipses, rectangles, triangles) in a grid whose resolution
//! follows the size on the screen, and each cell is filled in layers: a dark `@` underlay, a
//! character picked from a ramp by the light on that cell, a brighter outline, then the details
//! (eyes, screens, weapons). The same thing at any size, with no hole in it.
use vector_render::Framebuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Human,
    Armed,
    Robot,
    Rat,
    Insect,
    Queen,
    Golem,
    Ghoul,
    Sentinel,
    Mutant,
    Wisp,
    Chest,
    Crawler,
    Wolf,
    Turret,
    Horror,
    AiCore,
    Beast,
    Prey,
    Tree,
    Pine,
    Bush,
    Flower,
    Reed,
    Pile,
    Console,
    StairsUp,
    StairsDown,
    Ruin,
    Altar,
    Feature,
    Rock,
    Pond,
}

/// What a cell of a silhouette is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Empty,
    Body,
    /// Limbs, undersides: a little darker.
    Dark,
    /// A detail that shines (eyes, screens).
    Light,
    /// An accent in another colour (a weapon, flowers, a lid).
    Accent,
}

fn ell(u: f64, v: f64, cx: f64, cy: f64, rx: f64, ry: f64) -> bool {
    ((u - cx) / rx).powi(2) + ((v - cy) / ry).powi(2) <= 1.0
}

fn rect(u: f64, v: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    u >= x0 && u <= x1 && v >= y0 && v <= y1
}

/// A triangle with its apex at (ax, top) and its base on the row `base`, half a width `hw`.
fn tri(u: f64, v: f64, ax: f64, top: f64, base: f64, hw: f64) -> bool {
    v >= top && v <= base && (u - ax).abs() <= hw * (v - top) / (base - top).max(1e-6)
}

/// Width over height of the silhouette.
fn aspect(k: Kind) -> f64 {
    match k {
        Kind::Human | Kind::Armed | Kind::Mutant | Kind::Ghoul => 0.5,
        Kind::Robot | Kind::Sentinel | Kind::Golem | Kind::Turret | Kind::AiCore => 0.6,
        Kind::Rat | Kind::Prey | Kind::Crawler => 1.4,
        Kind::Wolf | Kind::Beast => 1.5,
        Kind::Insect | Kind::Horror | Kind::Queen => 0.9,
        Kind::Wisp | Kind::Feature => 0.8,
        Kind::Chest | Kind::Console | Kind::Altar => 0.9,
        Kind::Tree | Kind::Bush => 0.9,
        Kind::Pine | Kind::Flower | Kind::Stairs | Kind::Ruin => 0.7,
        Kind::Reed => 0.6,
        Kind::Pile => 1.2,
        Kind::Rock => 1.6,
        Kind::Pond => 3.0,
        _ => 0.8,
    }
}

impl Kind {
    #[allow(non_upper_case_globals)]
    const Stairs: Kind = Kind::StairsUp;
}

/// The silhouette of `k` at (u, v) in [0, 1]^2, v down.
fn tone(k: Kind, u: f64, v: f64) -> Tone {
    use Tone::{Accent, Body, Dark, Empty, Light};
    let any = |b: bool, t: Tone| if b { Some(t) } else { None };
    let t = match k {
        Kind::Human | Kind::Armed | Kind::Mutant | Kind::Ghoul => {
            let eye = ell(u, v, 0.43, 0.11, 0.035, 0.03) || ell(u, v, 0.57, 0.11, 0.035, 0.03);
            let arm_r = if k == Kind::Armed { 0.74 } else { 0.8 };
            any(eye, Light)
                .or(any(
                    k == Kind::Armed && rect(u, v, 0.8, 0.2, 0.9, 0.7),
                    Accent,
                ))
                .or(any(ell(u, v, 0.5, 0.12, 0.13, 0.1), Body))
                .or(any(rect(u, v, 0.3, 0.24, 0.7, 0.58), Body))
                .or(any(
                    rect(u, v, 0.14, 0.26, 0.3, 0.56) || rect(u, v, 0.7, 0.26, arm_r, 0.56),
                    Dark,
                ))
                .or(any(
                    rect(u, v, 0.34, 0.58, 0.48, 0.97) || rect(u, v, 0.52, 0.58, 0.66, 0.97),
                    Dark,
                ))
        }
        Kind::Robot | Kind::Sentinel | Kind::Golem => {
            let eye = rect(u, v, 0.36, 0.1, 0.64, 0.16);
            any(eye, Light)
                .or(any(rect(u, v, 0.3, 0.04, 0.7, 0.24), Body))
                .or(any(rect(u, v, 0.22, 0.26, 0.78, 0.66), Body))
                .or(any(
                    rect(u, v, 0.06, 0.28, 0.22, 0.62) || rect(u, v, 0.78, 0.28, 0.94, 0.62),
                    Dark,
                ))
                .or(any(
                    rect(u, v, 0.28, 0.66, 0.46, 0.97) || rect(u, v, 0.54, 0.66, 0.72, 0.97),
                    Dark,
                ))
        }
        Kind::Turret => any(rect(u, v, 0.08, 0.3, 0.95, 0.42), Accent)
            .or(any(ell(u, v, 0.45, 0.38, 0.28, 0.2), Body))
            .or(any(ell(u, v, 0.4, 0.35, 0.06, 0.05), Light))
            .or(any(rect(u, v, 0.36, 0.55, 0.54, 0.8), Dark))
            .or(any(rect(u, v, 0.15, 0.8, 0.75, 0.97), Body)),
        Kind::AiCore => any(rect(u, v, 0.2, 0.2, 0.8, 0.5), Light)
            .or(any(rect(u, v, 0.08, 0.08, 0.92, 0.9), Body)),
        Kind::Rat | Kind::Prey | Kind::Crawler | Kind::Wolf | Kind::Beast => {
            let big = matches!(k, Kind::Wolf | Kind::Beast);
            let eye = ell(u, v, if big { 0.8 } else { 0.84 }, 0.3, 0.03, 0.04);
            let legs = rect(u, v, 0.22, 0.6, 0.3, 0.96)
                || rect(u, v, 0.34, 0.6, 0.42, 0.96)
                || rect(u, v, 0.6, 0.6, 0.68, 0.96)
                || rect(u, v, 0.72, 0.6, 0.8, 0.96);
            any(eye, Light)
                .or(any(rect(u, v, 0.02, 0.38, 0.2, 0.44), Dark))
                .or(any(ell(u, v, 0.83, 0.38, 0.14, 0.17), Body))
                .or(any(ell(u, v, 0.45, 0.46, 0.38, 0.22), Body))
                .or(any(legs, Dark))
        }
        Kind::Insect | Kind::Queen => {
            let r = if k == Kind::Queen { 1.0 } else { 0.75 };
            any(ell(u, v, 0.5, 0.2, 0.06 * r, 0.05 * r), Light)
                .or(any(ell(u, v, 0.5, 0.2, 0.14 * r, 0.12 * r), Body))
                .or(any(ell(u, v, 0.5, 0.5, 0.18 * r + 0.06, 0.2), Body))
                .or(any(ell(u, v, 0.5, 0.8, 0.14 * r + 0.04, 0.16), Body))
                .or(any(
                    (u - 0.5).abs() > 0.2
                        && (u - 0.5).abs() < 0.45
                        && ((v - 0.35).abs() < 0.04
                            || (v - 0.55).abs() < 0.04
                            || (v - 0.72).abs() < 0.04),
                    Dark,
                ))
                .or(any(
                    rect(u, v, 0.05, 0.05, 0.3, 0.12) || rect(u, v, 0.7, 0.05, 0.95, 0.12),
                    Accent,
                ))
        }
        Kind::Horror => any(ell(u, v, 0.5, 0.35, 0.08, 0.06), Light)
            .or(any(ell(u, v, 0.5, 0.42, 0.26, 0.3), Body))
            .or(any(
                tri(u, v, 0.5, 0.0, 0.2, 0.2)
                    || tri(u, v, 0.15, 0.1, 0.4, 0.1)
                    || tri(u, v, 0.85, 0.1, 0.4, 0.1),
                Dark,
            ))
            .or(any(
                rect(u, v, 0.25, 0.7, 0.35, 1.0)
                    || rect(u, v, 0.45, 0.7, 0.55, 1.0)
                    || rect(u, v, 0.65, 0.7, 0.75, 1.0),
                Dark,
            )),
        Kind::Wisp => any(ell(u, v, 0.5, 0.5, 0.12, 0.12), Light)
            .or(any(ell(u, v, 0.5, 0.5, 0.3, 0.3), Body))
            .or(any(ell(u, v, 0.5, 0.5, 0.5, 0.5), Dark)),
        Kind::Chest => any(rect(u, v, 0.44, 0.45, 0.56, 0.62), Light)
            .or(any(rect(u, v, 0.05, 0.1, 0.95, 0.42), Accent))
            .or(any(rect(u, v, 0.05, 0.42, 0.95, 0.95), Body)),
        Kind::Console => any(rect(u, v, 0.25, 0.1, 0.75, 0.4), Light)
            .or(any(rect(u, v, 0.12, 0.05, 0.88, 0.6), Body))
            .or(any(rect(u, v, 0.4, 0.6, 0.6, 0.9), Dark))
            .or(any(rect(u, v, 0.2, 0.9, 0.8, 1.0), Body)),
        Kind::Tree => any(rect(u, v, 0.44, 0.55, 0.56, 1.0), Dark)
            .or(any(ell(u, v, 0.5, 0.3, 0.38, 0.28), Body))
            .or(any(
                ell(u, v, 0.28, 0.42, 0.24, 0.2) || ell(u, v, 0.72, 0.42, 0.24, 0.2),
                Body,
            ))
            .or(any(
                ell(u, v, 0.4, 0.2, 0.05, 0.04) || ell(u, v, 0.62, 0.34, 0.04, 0.04),
                Accent,
            )),
        Kind::Pine => any(rect(u, v, 0.46, 0.85, 0.54, 1.0), Dark).or(any(
            tri(u, v, 0.5, 0.0, 0.4, 0.22)
                || tri(u, v, 0.5, 0.25, 0.65, 0.32)
                || tri(u, v, 0.5, 0.5, 0.9, 0.42),
            Body,
        )),
        Kind::Bush => any(
            ell(u, v, 0.3, 0.55, 0.3, 0.3)
                || ell(u, v, 0.7, 0.55, 0.3, 0.3)
                || ell(u, v, 0.5, 0.45, 0.35, 0.35),
            Body,
        )
        .or(any(
            ell(u, v, 0.35, 0.4, 0.04, 0.04) || ell(u, v, 0.62, 0.5, 0.04, 0.04),
            Accent,
        ))
        .or(any(rect(u, v, 0.1, 0.85, 0.9, 1.0), Dark)),
        Kind::Flower => any(ell(u, v, 0.5, 0.2, 0.1, 0.1), Light)
            .or(any(ell(u, v, 0.5, 0.2, 0.28, 0.2), Accent))
            .or(any(
                rect(u, v, 0.47, 0.35, 0.53, 1.0)
                    || ell(u, v, 0.35, 0.7, 0.14, 0.07)
                    || ell(u, v, 0.65, 0.62, 0.14, 0.07),
                Body,
            )),
        Kind::Reed => any(
            rect(u, v, 0.1, 0.3, 0.17, 1.0)
                || rect(u, v, 0.28, 0.1, 0.35, 1.0)
                || rect(u, v, 0.46, 0.4, 0.53, 1.0)
                || rect(u, v, 0.64, 0.0, 0.71, 1.0)
                || rect(u, v, 0.82, 0.25, 0.89, 1.0),
            Body,
        )
        .or(any(
            rect(u, v, 0.28, 0.05, 0.35, 0.22) || rect(u, v, 0.64, 0.0, 0.71, 0.18),
            Accent,
        )),
        Kind::Pile => any(ell(u, v, 0.5, 0.35, 0.18, 0.28), Accent).or(any(
            ell(u, v, 0.3, 0.7, 0.28, 0.28)
                || ell(u, v, 0.72, 0.7, 0.26, 0.26)
                || ell(u, v, 0.5, 0.75, 0.3, 0.25),
            Body,
        )),
        Kind::StairsUp | Kind::StairsDown => {
            let up = k == Kind::StairsUp;
            let steps = (0..5).any(|i| {
                let y0 = 0.15 + f64::from(i) * 0.17;
                let inset = if up {
                    f64::from(i) * 0.07
                } else {
                    0.3 - f64::from(i) * 0.07
                };
                rect(u, v, 0.1 + inset * 0.4, y0, 0.9 - inset * 0.4, y0 + 0.14)
            });
            any(rect(u, v, 0.44, 0.0, 0.56, 0.12), Light).or(any(steps, Body))
        }
        Kind::Ruin => any(ell(u, v, 0.5, 0.55, 0.2, 0.3), Empty)
            .filter(|_| false)
            .or(any(
                rect(u, v, 0.15, 0.2, 0.3, 1.0)
                    || rect(u, v, 0.7, 0.2, 0.85, 1.0)
                    || rect(u, v, 0.15, 0.05, 0.85, 0.22),
                Body,
            ))
            .or(any(rect(u, v, 0.38, 0.5, 0.62, 1.0), Dark)),
        Kind::Altar => any(ell(u, v, 0.5, 0.15, 0.1, 0.12), Light).or(any(
            rect(u, v, 0.15, 0.4, 0.85, 0.55)
                || rect(u, v, 0.28, 0.55, 0.72, 0.9)
                || rect(u, v, 0.1, 0.9, 0.9, 1.0),
            Body,
        )),
        Kind::Feature => any(ell(u, v, 0.5, 0.5, 0.08, 0.08), Light)
            .or(any(ell(u, v, 0.5, 0.5, 0.4, 0.4), Accent)),
        Kind::Rock => any(ell(u, v, 0.5, 0.5, 0.3, 0.28), Body)
            .or(any(
                ell(u, v, 0.3, 0.7, 0.28, 0.3) || ell(u, v, 0.72, 0.72, 0.26, 0.28),
                Dark,
            ))
            .or(any(ell(u, v, 0.5, 0.6, 0.5, 0.4), Dark)),
        Kind::Pond => any(ell(u, v, 0.5, 0.5, 0.5, 0.5), Body),
    };
    t.unwrap_or(Empty)
}

/// Ramp of characters from sparse to dense.
const RAMP: &[u8] = b".:-+=*%#@";

fn shade(c: u32, k: f64) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f64 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn mix(a: u32, b: u32, t: f64) -> u32 {
    let ch = |c: u32, s: u32| f64::from((c >> s) & 0xFF);
    let m = |s: u32| (ch(a, s) * (1.0 - t) + ch(b, s) * t).clamp(0.0, 255.0) as u32;
    (m(16) << 16) | (m(8) << 8) | m(0)
}

/// Draw the billboard of `kind`, its feet at (`x`, `y`) (the middle of its base), `height_px`
/// tall, in `color` (faded as the caller wishes); `mark` is a letter for a terminal's screen.
pub fn draw(
    fb: &mut Framebuffer,
    kind: Kind,
    x: i32,
    y: i32,
    height_px: f64,
    color: u32,
    mark: Option<char>,
) {
    if height_px < 4.0 {
        return;
    }
    let scale = if height_px < 150.0 {
        1
    } else if height_px < 420.0 {
        2
    } else {
        3
    };
    let (cw, ch) = (6 * scale, 8 * scale);
    let rows = ((height_px / f64::from(ch)).round() as i32).clamp(3, 48);
    let cols = ((f64::from(rows) * aspect(kind) * f64::from(ch) / f64::from(cw)).round() as i32)
        .clamp(2, 80);
    let (w_px, h_px) = (cols * cw, rows * ch);
    let (x0, y0) = (x - w_px / 2, y - h_px);
    let accent = mix(color, 0x00FF_E070, 0.65);
    let at = |i: i32, j: i32| {
        tone(
            kind,
            (f64::from(j) + 0.5) / f64::from(cols),
            (f64::from(i) + 0.5) / f64::from(rows),
        )
    };
    for i in 0..rows {
        for j in 0..cols {
            let t = at(i, j);
            if t == Tone::Empty {
                continue;
            }
            let (u, v) = (
                (f64::from(j) + 0.5) / f64::from(cols),
                (f64::from(i) + 0.5) / f64::from(rows),
            );
            let (px, py) = (x0 + j * cw, y0 + i * ch);
            // Light from the upper left, a little grain.
            let g = vector_core::hash_coord(3, i64::from(i), i64::from(j), kind as i64);
            let grain = ((g >> 11) as f64 / (1u64 << 53) as f64) * 0.18;
            let lum = (0.38 + 0.32 * (1.0 - v) + 0.30 * (0.6 - u) + grain).clamp(0.05, 1.0);
            // Edge: a neighbour is empty.
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(di, dj)| {
                at(i + di, j + dj) == Tone::Empty
                    || i + di < 0
                    || i + di >= rows
                    || j + dj < 0
                    || j + dj >= cols
            });
            let base = match t {
                Tone::Dark => shade(color, 0.72),
                Tone::Accent => accent,
                Tone::Light => mix(color, 0x00FF_FFFF, 0.8),
                _ => color,
            };
            // Underlay: the densest character, dark; then the ramp by the light; then the edge.
            fb.rect_over(px, py, cw, ch, shade(base, 0.2));
            match t {
                Tone::Light => {
                    fb.text_big_over(px, py, "O", scale, base);
                }
                _ => {
                    let c = RAMP
                        [((lum * (RAMP.len() - 1) as f64).round() as usize).min(RAMP.len() - 1)]
                        as char;
                    fb.text_big_over(
                        px,
                        py,
                        &c.to_string(),
                        scale,
                        shade(base, 0.55 + 0.45 * lum),
                    );
                    if edge {
                        fb.text_big_over(px, py, "#", scale, shade(base, 0.85));
                    }
                }
            }
        }
    }
    if let Some(m) = mark {
        let (mx, my) = (x0 + (cols / 2) * cw, y0 + (rows / 4) * ch);
        fb.text_big_over(mx, my, &m.to_string(), scale, 0x00FF_FFFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Kind; 33] = [
        Kind::Human,
        Kind::Armed,
        Kind::Robot,
        Kind::Rat,
        Kind::Insect,
        Kind::Queen,
        Kind::Golem,
        Kind::Ghoul,
        Kind::Sentinel,
        Kind::Mutant,
        Kind::Wisp,
        Kind::Chest,
        Kind::Crawler,
        Kind::Wolf,
        Kind::Turret,
        Kind::Horror,
        Kind::AiCore,
        Kind::Beast,
        Kind::Prey,
        Kind::Tree,
        Kind::Pine,
        Kind::Bush,
        Kind::Flower,
        Kind::Reed,
        Kind::Pile,
        Kind::Console,
        Kind::StairsUp,
        Kind::StairsDown,
        Kind::Ruin,
        Kind::Altar,
        Kind::Feature,
        Kind::Rock,
        Kind::Pond,
    ];

    #[test]
    fn every_silhouette_is_a_real_filled_shape_at_any_size() {
        for k in ALL {
            for rows in [6, 12, 24, 40] {
                let cols = ((f64::from(rows) * aspect(k) * 8.0 / 6.0).round() as i32).max(2);
                let mut filled = 0;
                for i in 0..rows {
                    for j in 0..cols {
                        let (u, v) = (
                            (f64::from(j) + 0.5) / f64::from(cols),
                            (f64::from(i) + 0.5) / f64::from(rows),
                        );
                        if tone(k, u, v) != Tone::Empty {
                            filled += 1;
                        }
                    }
                }
                let share = f64::from(filled) / f64::from(rows * cols);
                assert!(
                    share > 0.22 && share < 0.98,
                    "{k:?} at {rows} rows: {share:.2} filled"
                );
            }
        }
    }

    #[test]
    fn drawing_paints_pixels_only_inside_the_box_of_the_silhouette() {
        let mut fb = Framebuffer::new(200, 200);
        draw(&mut fb, Kind::Human, 100, 190, 120.0, 0x00FF_8070, None);
        let lit: Vec<(usize, usize)> = (0..200usize)
            .flat_map(|y| (0..200usize).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.px[y * 200 + x] != 0)
            .collect();
        assert!(lit.len() > 1500, "{} pixels", lit.len());
        assert!(lit
            .iter()
            .all(|&(x, y)| (60..140).contains(&x) && (60..=195).contains(&y)));
    }
}
