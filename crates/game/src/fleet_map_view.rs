//! Map of the delegated fleet (M in the fleet screen): the systems that hold your colonies or
//! the ends of your routes, placed where they are in the galaxy, with the routes drawn between
//! them (green = active, dim = stopped, amber = the selected route, a ring = both ends in the
//! same system).
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN};
use vector_render::Framebuffer;
use vector_world::galaxy::StarRef;

const X0: f32 = 60.0;
const X1: f32 = 760.0;
const Y0: f32 = 110.0;
const Y1: f32 = 600.0;

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 8, "CARTE DE LA FLOTTE  (M : RETOUR AUX LISTES)", AMBER);
    fb.text(
        1,
        9,
        "VERT : ROUTE ACTIVE   GRIS : ROUTE ARRETEE   AMBRE : ROUTE CHOISIE   ANNEAU : MEME SYSTEME",
        DIM,
    );
    // The systems that matter.
    let mut stars: Vec<StarRef> = vec![s.here];
    let add = |st: Option<StarRef>, stars: &mut Vec<StarRef>| {
        if let Some(st) = st {
            if !stars.contains(&st) {
                stars.push(st);
            }
        }
    };
    for c in &s.colonies {
        add(Some(c.star), &mut stars);
    }
    for r in &s.routes {
        add(s.endpoint_star(&r.from), &mut stars);
        add(s.endpoint_star(&r.to), &mut stars);
    }
    let pos_of = |st: StarRef| -> [f64; 3] {
        if st == s.here {
            s.here_pos
        } else {
            g.sim.galaxy.star(st).map_or(s.here_pos, |e| e.pos)
        }
    };
    let pts: Vec<[f64; 3]> = stars.iter().map(|&st| pos_of(st)).collect();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &pts {
        for (k, a) in [0usize, 2].into_iter().enumerate() {
            lo[k] = lo[k].min(p[a]);
            hi[k] = hi[k].max(p[a]);
        }
    }
    // One scale for both axes, so distances are true.
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6);
    let scale = ((X1 - X0).min(Y1 - Y0) as f64 / span).min(400.0);
    let (cx, cy) = ((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
    let to_px = |p: [f64; 3]| -> (f32, f32) {
        (
            (X0 + X1) / 2.0 + ((p[0] - cx) * scale) as f32,
            (Y0 + Y1) / 2.0 + ((p[2] - cy) * scale) as f32,
        )
    };
    let at = |st: Option<StarRef>| -> Option<(f32, f32)> {
        let i = stars.iter().position(|&x| Some(x) == st)?;
        Some(to_px(pts[i]))
    };
    for (i, r) in s.routes.iter().enumerate() {
        let col = if g.fleet_focus == 1 && i == g.route_sel {
            AMBER
        } else if r.active {
            GREEN
        } else {
            DIM
        };
        let (a, b) = (at(s.endpoint_star(&r.from)), at(s.endpoint_star(&r.to)));
        let (Some((x0, y0)), Some((x1, y1))) = (a, b) else {
            continue;
        };
        if s.endpoint_star(&r.from) == s.endpoint_star(&r.to) {
            fb.circle(x0 + 14.0, y0 - 14.0, 12.0, col);
        } else {
            fb.line(x0, y0, x1, y1, col);
            let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let name = &g.data.resources[r.resource].name;
            fb.text_at(
                mx as i32 + 4,
                my as i32 - 10,
                &format!("#{} {}", r.id, name.chars().take(12).collect::<String>()),
                col,
            );
        }
    }
    for (i, &st) in stars.iter().enumerate() {
        let (x, y) = to_px(pts[i]);
        let name = if st == s.here {
            s.system.name.clone()
        } else {
            g.sim.galaxy.star(st).map_or("?".into(), |e| e.header.name)
        };
        fb.circle(x, y, 5.0, if st == s.here { AMBER } else { GREEN });
        let here = if st == s.here { "  (ICI)" } else { "" };
        fb.text_at(
            x as i32 + 9,
            y as i32 - 4,
            &format!("{}{}", name.to_uppercase(), here),
            GREEN,
        );
        for (k, c) in s.colonies.iter().filter(|c| c.star == st).enumerate() {
            fb.text_at(
                x as i32 + 9,
                y as i32 + 6 + 10 * k as i32,
                &format!("{} POP {:.0}", c.name.to_uppercase(), c.pop),
                DIM,
            );
        }
    }
}
