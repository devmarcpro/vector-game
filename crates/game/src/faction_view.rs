//! Factions screen (X in flight): the factions as boxes on a ring, joined by green lines for
//! alliances and red ones for enmities, and for the selected one its mores (the eight axes of
//! `data/ethos.ron`), what it bans, your reputation and its taxes.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;

const CX: f32 = 330.0;
const CY: f32 = 340.0;
const RX: f32 = 250.0;
const RY: f32 = 230.0;
const BOX_W: i32 = 112;
const BOX_H: i32 = 26;

fn pos(i: usize, n: usize) -> (f32, f32) {
    let a = std::f32::consts::TAU * i as f32 / n.max(1) as f32 - std::f32::consts::FRAC_PI_2;
    (CX + RX * a.cos(), CY + RY * a.sin())
}

/// Where the line from the centre of a box towards (tx, ty) leaves the box.
fn edge(cx: f32, cy: f32, tx: f32, ty: f32) -> (f32, f32) {
    let (dx, dy) = (tx - cx, ty - cy);
    let (hw, hh) = (BOX_W as f32 / 2.0, BOX_H as f32 / 2.0);
    let t = (hw / dx.abs().max(1e-3)).min(hh / dy.abs().max(1e-3));
    (cx + dx * t, cy + dy * t)
}

/// The registry of relations: every faction, people, cartel, contact and company that has an
/// opinion of you, the ties between them, and what changed lately.
fn draw_relations(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let rows = s.relation_rows();
    fb.text(1, 1, "RELATIONS", AMBER);
    fb.text(20, 1, "[TAB] FACTIONS ET MOEURS", DIM);
    fb.text(
        1,
        2,
        "CE QUE CHACUN PENSE DE VOUS ; CE QUE VOUS FAITES A L'UN SE REPERCUTE, ATTENUE, SUR SES ALLIES ET SES RIVAUX",
        DIM,
    );
    let sel = g.rel_sel.min(rows.len().saturating_sub(1));
    let visible = fb.rows().saturating_sub(10).max(1);
    let start = sel
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible));
    for (i, r) in rows.iter().enumerate().skip(start).take(visible) {
        let y = 4 + i - start;
        let col = if i == sel {
            AMBER
        } else if r.value < -0.2 {
            RED
        } else if r.value > 0.2 {
            GREEN
        } else {
            DIM
        };
        let cell = (((r.value.clamp(-1.0, 1.0) + 1.0) * 10.0).round()) as usize;
        let mut bar = ['-'; 21];
        bar[10] = '|';
        bar[cell.min(20)] = '#';
        let name: String = r.name.to_uppercase().chars().take(30).collect();
        fb.text(
            1,
            y,
            &format!(
                "{}{:<13}{:<31}{}  {:+.2}",
                if i == sel { ">" } else { " " },
                r.who.kind.label(),
                name,
                bar.iter().collect::<String>(),
                r.value
            ),
            col,
        );
    }
    // The selected entity: its ties, then what changed lately (echoes in dim).
    let x = 90;
    if let Some(r) = rows.get(sel) {
        fb.text(x, 4, &r.name.to_uppercase(), AMBER);
        let (friends, foes) = s.ties_of(r.who);
        fb.text(x, 6, "ALLIES", GREEN);
        for (k, n) in friends.iter().take(6).enumerate() {
            fb.text(x + 2, 7 + k, &n.to_uppercase(), GREEN);
        }
        fb.text(x, 14, "RIVAUX", RED);
        for (k, n) in foes.iter().take(6).enumerate() {
            fb.text(x + 2, 15 + k, &n.to_uppercase(), RED);
        }
    }
    let y0 = fb.rows().saturating_sub(5);
    fb.text(
        1,
        y0.saturating_sub(7),
        "DERNIERS CHANGEMENTS (ECHO = REPERCUSSION D'UN AUTRE)",
        AMBER,
    );
    for (k, c) in s.relations.history.iter().rev().take(5).enumerate() {
        fb.text(
            1,
            y0.saturating_sub(6) + k,
            &format!(
                "{:<30}{:+.3}{}",
                s.entity_name(c.who)
                    .to_uppercase()
                    .chars()
                    .take(29)
                    .collect::<String>(),
                c.delta,
                if c.echo { "  (ECHO)" } else { "" }
            ),
            if c.echo {
                DIM
            } else if c.delta > 0.0 {
                GREEN
            } else {
                RED
            },
        );
    }
    fb.text(
        1,
        fb.rows() - 3,
        "HAUT/BAS:CHOISIR  PGUP/PGDN:SAUTER  TAB:FACTIONS  X/ECHAP:RETOUR",
        DIM,
    );
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    if g.war_page {
        crate::war_view::draw_war(fb, g);
        return;
    }
    if g.rel_page {
        draw_relations(fb, g);
        return;
    }
    if g.soc_page {
        crate::society_view::draw(fb, g);
        return;
    }
    let s = &g.sim;
    let fs = &g.data.factions;
    let n = fs.len();
    fb.text(1, 1, "FACTIONS, MOEURS ET LOIS", AMBER);
    fb.text(40, 1, "[TAB] RELATIONS   [S] SOCIETE   [G] GUERRE", DIM);
    fb.text(
        1,
        2,
        "VERT : ALLIES   ROUGE : ENNEMIS   CADRE AMBRE : SELECTION   SOULIGNE : TERRITOIRE ACTUEL",
        DIM,
    );
    let index = |id: &str| fs.iter().position(|f| f.id == id);
    for (i, f) in fs.iter().enumerate() {
        let (x0, y0) = pos(i, n);
        for (list, col) in [(&f.allies, 0x0040_C060u32), (&f.enemies, 0x00C0_4040)] {
            for id in list {
                if let Some(j) = index(id) {
                    // Draw each pair once.
                    if j > i {
                        let (x1, y1) = pos(j, n);
                        let (ax, ay) = edge(x0, y0, x1, y1);
                        let (bx, by) = edge(x1, y1, x0, y0);
                        fb.line(ax, ay, bx, by, col);
                    }
                }
            }
        }
    }
    for (i, f) in fs.iter().enumerate() {
        let (x, y) = pos(i, n);
        let (l, t) = (x as i32 - BOX_W / 2, y as i32 - BOX_H / 2);
        let (r, b) = (l + BOX_W, t + BOX_H);
        let sel = i == g.fac_sel;
        let hover = g.hit((l, t, r, b), Act::Call(Game::click_faction, i), None);
        let col = if sel {
            AMBER
        } else if hover {
            0x00D0_FFD0
        } else {
            GREEN
        };
        let (lf, tf, rf, bf) = (l as f32, t as f32, r as f32, b as f32);
        fb.line(lf, tf, rf, tf, col);
        fb.line(rf, tf, rf, bf, col);
        fb.line(rf, bf, lf, bf, col);
        fb.line(lf, bf, lf, tf, col);
        let name: String = f.name.to_uppercase().chars().take(13).collect();
        fb.text_at(l + 4, t + 4, &name, col);
        let rep = s.reputation_with(Some(i));
        let rc = if rep < -0.2 {
            RED
        } else if rep > 0.2 {
            GREEN
        } else {
            DIM
        };
        fb.text_at(l + 4, t + 15, &format!("REP {rep:+.1}"), rc);
        if s.faction == Some(i) {
            fb.line(lf + 2.0, bf + 3.0, rf - 2.0, bf + 3.0, f.color);
        }
    }
    // Detail of the selected faction.
    let Some(f) = fs.get(g.fac_sel) else { return };
    let x = 80;
    fb.text(x, 5, &f.name.to_uppercase(), f.color);
    fb.text(
        x,
        6,
        &format!("{}  -  {}", f.culture, f.ideology).to_uppercase(),
        DIM,
    );
    fb.text(
        x,
        8,
        &format!(
            "REPUTATION {:+.2}   TAXE {:.0}%   AGRESSIVITE {:.0}%",
            s.reputation_with(Some(g.fac_sel)),
            f.tax * 100.0,
            f.aggression * 100.0
        ),
        GREEN,
    );
    let profile = g.data.ethos.profile(Some(&f.id));
    fb.text(x, 10, "MOEURS", AMBER);
    for (k, axis) in g.data.ethos.axes.iter().enumerate() {
        let v = profile.get(k).copied().unwrap_or(0.0);
        let y = 11 + k;
        fb.text(x, y, &format!("{:<11.11}", axis.name.to_uppercase()), GREEN);
        fb.text(x + 12, y, &format!("{:>13.13}", axis.low), DIM);
        // A scale of 21 cells with the value as a marker.
        let cell = (((v.clamp(-1.0, 1.0) + 1.0) * 10.0).round()) as usize;
        let mut bar = ['-'; 21];
        bar[10] = '|';
        bar[cell.min(20)] = '#';
        fb.text(x + 26, y, &bar.iter().collect::<String>(), GREEN);
        fb.text(x + 48, y, &axis.high, DIM);
    }
    // What it bans: the moral tags whose ban condition holds for this faction.
    fb.text(x, 21, "INTERDIT PAR LA MORALE", AMBER);
    let banned: Vec<&str> = g
        .data
        .ethos
        .tags
        .iter()
        .filter(|t| {
            t.ban.iter().any(|(axis, op, th)| {
                let v = g
                    .data
                    .ethos
                    .axis(axis)
                    .and_then(|a| profile.get(a))
                    .copied()
                    .unwrap_or(0.0);
                if op == "<" {
                    v < *th
                } else {
                    v > *th
                }
            })
        })
        .map(|t| t.name.as_str())
        .collect();
    fb.text(
        x,
        22,
        &if banned.is_empty() {
            "RIEN".to_string()
        } else {
            banned.join(", ").to_uppercase()
        },
        if banned.is_empty() { DIM } else { RED },
    );
    fb.text(x, 24, "CONTREBANDE", AMBER);
    fb.text(
        x,
        25,
        &if f.contraband.is_empty() {
            "AUCUNE".to_string()
        } else {
            f.contraband.join(", ").to_uppercase()
        },
        if f.contraband.is_empty() { DIM } else { RED },
    );
    justice(fb, g, x);
}

/// The local law of the selected people and the legal situation of the player there.
fn justice(fb: &mut Framebuffer, g: &Game, x: usize) {
    let s = &g.sim;
    let sel = g.fac_sel;
    fb.text(x, 27, "LOIS EN VIGUEUR (CHEZ EUX SEULEMENT)", AMBER);
    let laws = s.laws_in_force(sel);
    for (k, &l) in laws.iter().enumerate() {
        let law = &g.data.laws[l];
        let col = if law.severity >= 2 { RED } else { GREEN };
        fb.text(
            x,
            28 + k,
            &format!(
                "{:<26.26} {:>6.0} CR  GRAVITE {}",
                law.name.to_uppercase(),
                law.fine,
                law.severity
            ),
            col,
        );
    }
    let y = 29 + laws.len();
    let (owed, noto) = (s.fines_owed(sel), s.notoriety_in(sel));
    fb.text(x, y, "MA SITUATION JURIDIQUE", AMBER);
    let state = if s.has_warrant(sel) {
        "MANDAT D'ARRET : PORTS FERMES"
    } else if noto > 0.0 {
        "FICHE"
    } else {
        "CASIER VIERGE"
    };
    fb.text(
        x,
        y + 1,
        &format!("{state}   NOTORIETE {noto:.1}   AMENDES DUES {owed:.0} CR"),
        if noto > 0.0 || owed > 0.0 { RED } else { GREEN },
    );
    let (cost, chance) = s.bribe_terms(sel);
    let by = (y as i32 + 3) * 10;
    let xs = x as i32 * 8;
    let items = [
        ("PAYER LES AMENDES (P)".to_string(), 0usize),
        (
            format!("POT-DE-VIN {cost:.0} CR, {:.0}% (B)", chance * 100.0),
            1,
        ),
        (format!("AVOCAT {:.0} CR (L)", s.lawyer_fee(sel)), 2),
    ];
    for (k, (label, code)) in items.iter().enumerate() {
        let rc = (xs, by + k as i32 * 12, xs + 40 * 8, by + k as i32 * 12 + 12);
        crate::proto_view::button(fb, g, rc, label, Act::Call(Game::click_justice, *code));
    }
}
