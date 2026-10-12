//! Family screen (G in the crew screen): the family tree of a crew member drawn as boxes joined
//! by lines (grandparents as couples, parents, brothers and sisters, uncles and aunts with their
//! children below them), and a second page (Tab) with the genes.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED, WHITE, WIRE};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::genealogy::{Kin, Relative};
use winit::keyboard::KeyCode;

const DEAD: u32 = 0x0060_806A;

fn fit(s: &str, w: usize) -> String {
    s.chars().take(w).collect()
}

/// A three-line box at text cell (x, y), `w` cells wide; the person themselves get a double frame.
fn draw_box(
    fb: &mut Framebuffer,
    x: usize,
    y: usize,
    w: usize,
    lines: [&str; 3],
    c: u32,
    me: bool,
) {
    let (l, t) = ((x * 8) as f32 - 3.0, (y * 10) as f32 - 3.0);
    let (r, b) = (((x + w) * 8) as f32 + 1.0, ((y + 3) * 10) as f32 + 1.0);
    fb.outline(l, t, r, b, c);
    if me {
        fb.outline(l + 2.0, t + 2.0, r - 2.0, b - 2.0, c);
    }
    fb.text(x, y, &fit(lines[0], w), c);
    fb.text(x, y + 1, &fit(lines[1], w), if me { c } else { DIM });
    fb.text(x, y + 2, &fit(lines[2], w), if me { c } else { DIM });
}

/// Pixel centre of the top edge of a box at (x, y) of width `w`.
fn top_mid(x: usize, y: usize, w: usize) -> (f32, f32) {
    (((x * 8) + (w * 8) / 2) as f32, (y * 10) as f32 - 3.0)
}

/// Pixel centre of the bottom edge.
fn bottom_mid(x: usize, y: usize, w: usize) -> (f32, f32) {
    (((x * 8) + (w * 8) / 2) as f32, ((y + 3) * 10) as f32 + 1.0)
}

/// A bus line from `from` (a point under a couple) down to `bus_y`, across the children's
/// x positions, and down to each child's top.
fn bus(fb: &mut Framebuffer, from: (f32, f32), bus_y: f32, kids: &[(f32, f32)]) {
    if kids.is_empty() {
        return;
    }
    fb.line(from.0, from.1, from.0, bus_y, WIRE);
    let lo = kids.iter().map(|k| k.0).fold(from.0, f32::min);
    let hi = kids.iter().map(|k| k.0).fold(from.0, f32::max);
    fb.line(lo, bus_y, hi, bus_y, WIRE);
    for k in kids {
        fb.line(k.0, bus_y, k.0, k.1, WIRE);
    }
}

fn status(r: &Relative) -> String {
    match r.died_at {
        Some(span) => format!("+{span:.0} ANS {:.0}%", r.similarity * 100.0),
        None => format!("{:.0} ANS {:.0}%", r.age, r.similarity * 100.0),
    }
}

/// Name, tie and age with resemblance of a relative, drawn in a box.
fn show(fb: &mut Framebuffer, g: &Game, x: usize, y: usize, w: usize, r: &Relative, c: u32) {
    // A relative who is aboard opens their own tree when clicked.
    if let Some(mi) = g
        .sim
        .crew
        .members
        .iter()
        .position(|m| m.kin == r.kin && m.alive)
    {
        g.hit_cells(x, y, w, 3, Act::Call(Game::click_family_member, mi), None);
    }
    let (name, tie, st) = (
        r.name.to_uppercase(),
        r.relation.label(r.kin.male()).to_uppercase(),
        status(r),
    );
    draw_box(fb, x, y, w, [&name, &tie, &st], c, false);
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let Some(ped) = g.family_ped.as_ref() else {
        fb.text(2, 2, "PAS DE FAMILLE A AFFICHER", RED);
        return;
    };
    let Some(p) = g.sim.crew.members.get(g.family_member) else {
        return;
    };
    let data = &g.data;
    fb.text(
        2,
        1,
        &format!("== GENEALOGIE : {} ==", p.name.to_uppercase()),
        AMBER,
    );
    fb.text(
        2,
        2,
        &format!(
            "{:.0} ANS  CULTURE {}  {}   [TAB : {}]",
            p.age,
            data.cultures[p.culture].name.to_uppercase(),
            if p.kin.male() { "HOMME" } else { "FEMME" },
            if g.family_tab == 0 { "GENES" } else { "ARBRE" }
        ),
        DIM,
    );
    for (col, label, key) in [
        (70usize, "[<]", KeyCode::ArrowLeft),
        (74, "[>]", KeyCode::ArrowRight),
        (78, "[ARBRE/GENES]", KeyCode::Tab),
    ] {
        let hover = g.hit_cells(col, 2, label.len(), 1, Act::Key(key), None);
        fb.text(col, 2, label, if hover { AMBER } else { DIM });
    }
    if g.family_tab == 1 {
        draw_genes(fb, g);
        return;
    }
    let aboard = |k: Kin| g.sim.crew.members.iter().any(|m| m.kin == k && m.alive);
    let color = |r: &Relative| {
        if aboard(r.kin) {
            AMBER
        } else if r.alive {
            GREEN
        } else {
            DEAD
        }
    };
    let (father, mother) = (&ped.parents[0], &ped.parents[1]);
    let sorted = |via: Kin| -> Vec<&Relative> {
        let mut v: Vec<&Relative> = ped.uncles.iter().filter(|u| u.via == Some(via)).collect();
        v.sort_by_key(|u| u.kin.k);
        v
    };
    let (pat, mat) = (sorted(father.kin), sorted(mother.kin));
    // My generation: me and my brothers and sisters, by birth order.
    let mut kids: Vec<(Kin, Option<&Relative>)> =
        ped.siblings.iter().map(|s| (s.kin, Some(s))).collect();
    kids.push((p.kin, None));
    kids.sort_by_key(|k| k.0.k);
    // Horizontal layout in box units: aunts and uncles | five slots with the parents on the 2nd
    // and 4th | aunts and uncles.
    let cols = fb.cols();
    let units = pat.len() + 5 + mat.len();
    let w = (cols.saturating_sub(2) / units)
        .saturating_sub(1)
        .clamp(8, 17);
    let unit = w + 1;
    let x0 = cols.saturating_sub(units * unit) / 2 + 1;
    let xc = x0 + pat.len() * unit;
    let (xf, xm) = (xc + unit, xc + 3 * unit);
    let (y0, y1, y2) = (4usize, 12usize, 22usize);
    let bus0 = ((y0 + 3) * 10 + 20) as f32;
    let pat_x: Vec<usize> = (0..pat.len()).map(|i| x0 + i * unit).collect();
    let mat_x: Vec<usize> = (0..mat.len()).map(|i| xc + 5 * unit + i * unit).collect();
    // Level 1: uncles and aunts, then the parents.
    for (r, &x) in pat.iter().zip(&pat_x).chain(mat.iter().zip(&mat_x)) {
        show(fb, g, x, y1, w, r, color(r));
    }
    for (r, x) in [(father, xf), (mother, xm)] {
        show(fb, g, x, y1, w, r, color(r));
    }
    // The couple's union, and below it the children.
    let ymid = (y1 * 10 + 15) as f32;
    fb.line(
        ((xf + w) * 8 + 1) as f32,
        ymid,
        (xm * 8) as f32 - 3.0,
        ymid,
        WIRE,
    );
    let union_x = (((xf + w) * 8 + xm * 8) / 2) as f32;
    let n = kids.len().min(5);
    let start = xc + ((5 - n) * unit) / 2;
    let mut tops = Vec::new();
    for (i, (_, rel)) in kids.iter().take(5).enumerate() {
        let x = start + i * unit;
        match rel {
            Some(r) => show(fb, g, x, y2, w, r, color(r)),
            None => draw_box(
                fb,
                x,
                y2,
                w,
                [
                    &p.name.to_uppercase(),
                    "MOI-MEME",
                    &format!("{:.0} ANS", p.age),
                ],
                WHITE,
                true,
            ),
        }
        tops.push(top_mid(x, y2, w));
    }
    bus(fb, (union_x, ymid), (y2 * 10) as f32 - 12.0, &tops);
    // Level 0: the grandparents, two couples, each above its children.
    let couple =
        |fb: &mut Framebuffer, lo: usize, hi: usize, gps: &[&Relative], kids_x: &[usize]| {
            let left = ((lo + hi) / 2).saturating_sub(w + 1).max(1);
            let right = left + w + 2;
            for (r, x) in gps.iter().zip([left, right]) {
                show(fb, g, x, y0, w, r, color(r));
            }
            let ym = (y0 * 10 + 15) as f32;
            fb.line(
                ((left + w) * 8 + 1) as f32,
                ym,
                (right * 8) as f32 - 3.0,
                ym,
                WIRE,
            );
            let from = (((left + w) * 8 + right * 8) as f32 / 2.0, ym);
            let tops: Vec<(f32, f32)> = kids_x.iter().map(|&x| top_mid(x, y1, w)).collect();
            bus(fb, from, bus0, &tops);
        };
    let of = |parent: Kin| -> Vec<&Relative> {
        ped.grandparents
            .iter()
            .filter(|r| r.via == Some(parent))
            .collect()
    };
    let mut pat_kids = pat_x.clone();
    pat_kids.push(xf);
    let mut mat_kids = vec![xm];
    mat_kids.extend(&mat_x);
    couple(fb, x0, xf + w, &of(father.kin), &pat_kids);
    couple(
        fb,
        xm,
        mat_x.last().map_or(xm + w, |&x| x + w),
        &of(mother.kin),
        &mat_kids,
    );
    // Cousins hang under their uncle or aunt, one line each.
    for (r, &x) in pat.iter().zip(&pat_x).chain(mat.iter().zip(&mat_x)) {
        let cousins: Vec<&Relative> = ped
            .cousins
            .iter()
            .filter(|c| c.via == Some(r.kin))
            .collect();
        if cousins.is_empty() {
            continue;
        }
        let (bx, by) = bottom_mid(x, y1, w);
        let vx = bx - (w * 4) as f32 + 2.0;
        let last_y = ((y1 + 4 + cousins.len()) * 10) as f32 - 4.0;
        fb.line(vx, by, vx, last_y, WIRE);
        for (j, c) in cousins.iter().enumerate() {
            let row = y1 + 4 + j;
            let ty = (row * 10 + 3) as f32;
            fb.line(vx, ty, vx + 6.0, ty, WIRE);
            fb.text(
                x + 1,
                row,
                &fit(&format!("{} {:.0}", c.name.to_uppercase(), c.age), w - 1),
                color(c),
            );
        }
    }
    let foot = fb.rows().saturating_sub(3);
    fb.text(
        2,
        foot,
        "AMBRE : A BORD   VERT : VIVANT   GRIS : DECEDE (+ AGE)   % : RESSEMBLANCE GENETIQUE",
        DIM,
    );
}

/// Genes of the person: expression bars, and carrier status for the predispositions.
fn draw_genes(fb: &mut Framebuffer, g: &Game) {
    let p = &g.sim.crew.members[g.family_member];
    let data = &g.data;
    fb.text(2, 4, "GENES", AMBER);
    for (i, gene) in data.genes.iter().enumerate() {
        let (a, b) = (p.genome[2 * i], p.genome[2 * i + 1]);
        let y = 5 + i;
        if gene.is_predisposition() {
            let (tag, col) = match (a == 0, b == 0) {
                (true, true) => ("ATTEINT (RISQUE x4)", RED),
                (false, false) => ("-", DIM),
                _ => ("PORTEUR (RISQUE x1.5)", AMBER),
            };
            fb.text(3, y, &format!("{:<32}{tag}", gene.name.to_uppercase()), col);
        } else {
            let e = vector_data::Gene::expression(a, b);
            let n = (e * 10.0).round() as usize;
            let bar: String = (0..10).map(|k| if k < n { '#' } else { '-' }).collect();
            fb.text(
                3,
                y,
                &format!(
                    "{:<22}{bar} {:>3.0} %   ALLELES {a} {b}",
                    gene.name.to_uppercase(),
                    e * 100.0
                ),
                GREEN,
            );
        }
    }
    let l = 6 + data.genes.len();
    fb.text(
        3,
        l,
        "DEUX ALLELES PAR GENE (0 A 3), UN DE CHAQUE PARENT ; 0.5 % DE MUTATION",
        DIM,
    );
    fb.text(
        3,
        l + 1,
        "LES PREDISPOSITIONS MULTIPLIENT LE RISQUE DE LA MALADIE LIEE : PORTEUR x1.5, ATTEINT x4",
        DIM,
    );
}
