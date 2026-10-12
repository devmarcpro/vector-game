//! Production chains of a colony (F in the colony screen): resources consumed on the left, the
//! buildings that turn them into other resources in the middle, resources produced on the right,
//! joined by lines. A red input is out of stock, a dim building is switched off.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED, WIRE};
use crate::mouse::Act;
use vector_render::Framebuffer;

const TOP: i32 = 84;
const ROW_MAX: i32 = 560;

/// A building that runs a recipe: index in the colony, its inputs and outputs (resource, kg).
struct Node {
    k: usize,
    name: String,
    recipe: String,
    on: bool,
    inputs: Vec<usize>,
    outputs: Vec<usize>,
}

fn nodes(g: &Game) -> Vec<Node> {
    let Some(c) = g.sim.colonies.get(g.col_idx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, b) in c.buildings.iter().enumerate() {
        let d = &g.data.buildings[b.def];
        if !b.built() || d.recipes.is_empty() {
            continue;
        }
        let Some(r) = g
            .data
            .recipe_index(&d.recipes[b.recipe % d.recipes.len()])
            .map(|i| &g.data.recipes[i])
        else {
            continue;
        };
        let res = |list: &[(String, f64)]| -> Vec<usize> {
            list.iter()
                .filter_map(|(id, _)| g.data.resource_index(id))
                .collect()
        };
        out.push(Node {
            k,
            name: d.name.clone(),
            recipe: r.name.clone(),
            on: b.enabled,
            inputs: res(&r.inputs),
            outputs: res(&r.outputs),
        });
    }
    out
}

/// Distinct resources in order of first appearance.
fn distinct(lists: impl Iterator<Item = Vec<usize>>) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for l in lists {
        for r in l {
            if !v.contains(&r) {
                v.push(r);
            }
        }
    }
    v
}

fn spread(n: usize) -> (i32, i32) {
    // (first y, step) for n boxes of 26 px in the free height.
    let step = (ROW_MAX / n.max(1) as i32).clamp(18, 34);
    (TOP, step)
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let Some(c) = g.sim.colonies.get(g.col_idx) else {
        return;
    };
    let nodes = nodes(g);
    fb.text(
        1,
        5,
        "CHAINES DE PRODUCTION  (F : RETOUR AUX LISTES)",
        AMBER,
    );
    if nodes.is_empty() {
        fb.text(1, 7, "(AUCUN BATIMENT DE PRODUCTION CONSTRUIT)", DIM);
        return;
    }
    let ins = distinct(nodes.iter().map(|n| n.inputs.clone()));
    let outs = distinct(nodes.iter().map(|n| n.outputs.clone()));
    let (xi, xb, xo) = (24, 470, 960);
    let (wi, wb, wo) = (260, 330, 260);
    let place = |n: usize, i: usize| -> (i32, i32) {
        let (y0, step) = spread(n);
        (y0, y0 + step * i as i32)
    };
    let yb = |i: usize| place(nodes.len(), i).1;
    let yi = |r: usize| place(ins.len(), ins.iter().position(|&x| x == r).unwrap_or(0)).1;
    let yo = |r: usize| place(outs.len(), outs.iter().position(|&x| x == r).unwrap_or(0)).1;
    let mid = |y: i32| (y + 11) as f32;
    // Wires first, boxes on top.
    for (i, n) in nodes.iter().enumerate() {
        for &r in &n.inputs {
            let col = if !n.on {
                DIM
            } else if c.storage[r] < 1.0 {
                RED
            } else {
                WIRE
            };
            fb.line((xi + wi) as f32, mid(yi(r)), xb as f32, mid(yb(i)), col);
        }
        for &r in &n.outputs {
            let col = if n.on { WIRE } else { DIM };
            fb.line((xb + wb) as f32, mid(yb(i)), xo as f32, mid(yo(r)), col);
        }
    }
    let boxed = |fb: &mut Framebuffer, x: i32, y: i32, w: i32, col: u32| {
        let (l, t, r, b) = (x as f32, y as f32, (x + w) as f32, (y + 22) as f32);
        fb.line(l, t, r, t, col);
        fb.line(r, t, r, b, col);
        fb.line(r, b, l, b, col);
        fb.line(l, b, l, t, col);
    };
    fb.text_at(xi, TOP - 13, "CONSOMME (STOCK)", DIM);
    fb.text_at(xb, TOP - 13, "BATIMENT / RECETTE", DIM);
    fb.text_at(xo, TOP - 13, "PRODUIT (STOCK)", DIM);
    for &r in &ins {
        let y = yi(r);
        let col = if c.storage[r] < 1.0 { RED } else { GREEN };
        boxed(fb, xi, y, wi, col);
        let s = format!("{} {:.0}", g.data.resources[r].name, c.storage[r]);
        fb.text_at(xi + 4, y + 7, &s.chars().take(31).collect::<String>(), col);
    }
    for &r in &outs {
        let y = yo(r);
        boxed(fb, xo, y, wo, GREEN);
        let s = format!("{} {:.0}", g.data.resources[r].name, c.storage[r]);
        fb.text_at(
            xo + 4,
            y + 7,
            &s.chars().take(31).collect::<String>(),
            GREEN,
        );
    }
    for (i, n) in nodes.iter().enumerate() {
        let y = yb(i);
        let sel = g.b_sel == n.k;
        let col = if sel {
            AMBER
        } else if n.on {
            GREEN
        } else {
            DIM
        };
        g.hit(
            (xb, y, xb + wb, y + 22),
            Act::Call(Game::click_building, n.k),
            None,
        );
        boxed(fb, xb, y, wb, col);
        let s = format!("{} : {}", n.name, n.recipe);
        fb.text_at(xb + 4, y + 7, &s.chars().take(40).collect::<String>(), col);
    }
}
