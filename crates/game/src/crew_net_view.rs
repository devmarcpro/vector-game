//! Relations network of the crew (V in the crew screen): everybody on a ring, green lines for
//! friendships, red for quarrels (the stronger the thicker the colour), amber dotted lines for
//! kinship. The selected person's ties are listed on the right.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::genealogy;

const CX: f32 = 330.0;
const CY: f32 = 370.0;
const R: f32 = 220.0;
const BW: i32 = 104;
const BH: i32 = 26;

fn pos(i: usize, n: usize) -> (f32, f32) {
    let a = std::f32::consts::TAU * i as f32 / n.max(1) as f32 - std::f32::consts::FRAC_PI_2;
    (CX + R * a.cos(), CY + R * a.sin())
}

fn edge(cx: f32, cy: f32, tx: f32, ty: f32) -> (f32, f32) {
    let (dx, dy) = (tx - cx, ty - cy);
    let t = ((BW as f32 / 2.0) / dx.abs().max(1e-3)).min((BH as f32 / 2.0) / dy.abs().max(1e-3));
    (cx + dx * t, cy + dy * t)
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let crew = &g.sim.crew;
    let n = crew.members.len();
    fb.text(1, 8, "RESEAU DES RELATIONS  (V : RETOUR A LA LISTE)", AMBER);
    fb.text(
        1,
        9,
        "VERT : AMIS   ROUGE : RIVAUX   AMBRE : FAMILLE   CLIC : CHOISIR   DOUBLE CLIC : ARBRE",
        DIM,
    );
    if n == 0 {
        return;
    }
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = (&crew.members[i], &crew.members[j]);
            let v = (a.relation(b.id) + b.relation(a.id)) / 2;
            let kin = genealogy::relationship(&crew.families, a.kin, b.kin).is_some();
            let (x0, y0) = pos(i, n);
            let (x1, y1) = pos(j, n);
            let (ax, ay) = edge(x0, y0, x1, y1);
            let (bx, by) = edge(x1, y1, x0, y0);
            if v >= 15 {
                fb.line(ax, ay, bx, by, GREEN);
            } else if v <= -15 {
                fb.line(ax, ay, bx, by, RED);
            }
            if kin {
                // Dotted amber line a little to the side.
                let steps = 16;
                for s in (0..steps).step_by(2) {
                    let (t0, t1) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
                    fb.line(
                        ax + (bx - ax) * t0 + 2.0,
                        ay + (by - ay) * t0 + 2.0,
                        ax + (bx - ax) * t1 + 2.0,
                        ay + (by - ay) * t1 + 2.0,
                        AMBER,
                    );
                }
            }
        }
    }
    for (i, p) in crew.members.iter().enumerate() {
        let (x, y) = pos(i, n);
        let (l, t) = (x as i32 - BW / 2, y as i32 - BH / 2);
        let sel = i == g.crew_sel;
        let hover = g.hit(
            (l, t, l + BW, t + BH),
            Act::Call(Game::click_crew, i),
            Some(Act::Call(Game::open_family_of, i)),
        );
        let col = if !p.alive {
            DIM
        } else if sel {
            AMBER
        } else if p.morale < 0.3 {
            RED
        } else if hover {
            0x00D0_FFD0
        } else {
            GREEN
        };
        let (lf, tf, rf, bf) = (l as f32, t as f32, (l + BW) as f32, (t + BH) as f32);
        fb.line(lf, tf, rf, tf, col);
        fb.line(rf, tf, rf, bf, col);
        fb.line(rf, bf, lf, bf, col);
        fb.line(lf, bf, lf, tf, col);
        let name: String = p.name.to_uppercase().chars().take(12).collect();
        fb.text_at(l + 4, t + 4, &name, col);
        let role = g.data.roles[p.role].name.to_uppercase();
        fb.text_at(
            l + 4,
            t + 15,
            &role.chars().take(12).collect::<String>(),
            DIM,
        );
    }
    // The ties of the selected person.
    let Some(p) = crew.members.get(g.crew_sel) else {
        return;
    };
    let x = 100;
    fb.text(x, 10, &p.name.to_uppercase(), AMBER);
    fb.text(x, 12, "LIENS", AMBER);
    for (row, q) in (13..).zip(crew.members.iter().filter(|q| q.id != p.id)) {
        let v = p.relation(q.id);
        let kin = genealogy::relationship(&crew.families, p.kin, q.kin)
            .map(|r| r.label(q.kin.male()).to_uppercase())
            .unwrap_or_default();
        let col = if v >= 15 {
            GREEN
        } else if v <= -15 {
            RED
        } else {
            DIM
        };
        fb.text(
            x,
            row,
            &format!("{:<18.18} {:>+4}  {}", q.name.to_uppercase(), v, kin),
            col,
        );
    }
}
