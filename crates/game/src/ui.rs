//! Text-grid screens: main menu, pause, station market, inventory.
use crate::game::{Game, Screen, MAIN_MENU, PAUSE_MENU};
use crate::hud::{AMBER, DIM, GREEN, RED};
use vector_render::Framebuffer;
use vector_sim::cargo::{CARGO_MASS_CAP, CARGO_VOL_CAP};
use vector_sim::{FUEL_CAP, HULL_MAX};

fn menu(fb: &mut Framebuffer, top: usize, items: &[&str], sel: usize) {
    let col = fb.cols() / 2 - 12;
    for (i, it) in items.iter().enumerate() {
        let (c, mark) = if i == sel { (AMBER, ">") } else { (GREEN, " ") };
        fb.text(col, top + i * 2, &format!("{mark} {it}"), c);
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    match g.screen {
        Screen::MainMenu => draw_main(fb, g),
        Screen::Pause => {
            fb.text(fb.cols() / 2 - 4, 6, "== PAUSE ==", AMBER);
            menu(fb, 9, &PAUSE_MENU, g.menu_sel);
            fb.text(
                fb.cols() / 2 - 12,
                fb.rows() - 3,
                "HAUT/BAS CHOISIR   ENTREE VALIDER",
                DIM,
            );
        }
        Screen::Station => draw_station(fb, g),
        Screen::Inventory => draw_inventory(fb, g),
        Screen::Flight => {}
    }
}

fn draw_main(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    let scale = 6;
    let w = "VECTOR".len() as i32 * 8 * scale;
    fb.text_big((fb.w as i32 - w) / 2, 30, "VECTOR", scale, GREEN);
    fb.text(
        cols / 2 - 18,
        9,
        "SANDBOX SPATIAL EN LIGNES VECTORIELLES",
        DIM,
    );
    menu(fb, 13, &MAIN_MENU, g.menu_sel);
    let seed = if g.seed_text.is_empty() {
        "(DEFAUT)"
    } else {
        &g.seed_text
    };
    let c = if g.menu_sel == 0 { AMBER } else { DIM };
    fb.text(
        cols / 2 - 12,
        20,
        &format!("GRAINE: {seed}{}", if g.menu_sel == 0 { "_" } else { "" }),
        c,
    );
    fb.text(
        cols / 2 - 22,
        fb.rows() - 3,
        "HAUT/BAS CHOISIR  ENTREE VALIDER  TAPER UNE GRAINE",
        DIM,
    );
    if let Some(m) = g.messages.last().filter(|m| m.starts_with("ECHEC")) {
        fb.text(cols / 2 - m.len() / 2, fb.rows() - 5, m, RED);
    }
}

fn header(fb: &mut Framebuffer, g: &Game, title: &str) {
    let s = &g.sim;
    fb.text(1, 1, title, AMBER);
    fb.text(1, 2, &format!("CREDITS {:>10.0}", s.credits), GREEN);
    fb.gauge(1, 3, "CARBURANT", s.ship.fuel / FUEL_CAP, 20, GREEN);
    fb.gauge(1, 4, "COQUE    ", s.ship.hull / HULL_MAX, 20, GREEN);
    fb.gauge(
        1,
        5,
        "SOUTE MAS",
        s.ship.cargo.mass / CARGO_MASS_CAP,
        20,
        GREEN,
    );
    fb.gauge(
        1,
        6,
        "SOUTE VOL",
        s.ship.cargo.volume / CARGO_VOL_CAP,
        20,
        GREEN,
    );
}

fn window(sel: usize, n: usize, rows: usize) -> std::ops::Range<usize> {
    let start = sel.saturating_sub(rows / 2).min(n.saturating_sub(rows));
    start..(start + rows).min(n)
}

fn draw_station(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let Some(st) = s.docked else { return };
    header(fb, g, &format!("STATION {}", s.bodies[st].name));
    let Some(m) = s.markets.get(&st) else { return };
    let (x, top) = (40, 3);
    fb.text(
        x,
        top,
        "RESSOURCE            STOCK KG     ACHAT     VENTE    SOUTE KG",
        DIM,
    );
    let rows = fb.rows().saturating_sub(top + 6);
    for (line, i) in window(g.sel_res, g.data.resources.len(), rows).enumerate() {
        let r = &g.data.resources[i];
        let c = if i == g.sel_res { AMBER } else { GREEN };
        fb.text(
            x,
            top + 1 + line,
            &format!(
                "{}{:<18} {:>10.0} {:>9.2} {:>9.2} {:>10.0}",
                if i == g.sel_res { ">" } else { " " },
                r.name.chars().take(18).collect::<String>(),
                m.stock[i],
                m.ask(&g.data, i),
                m.bid(&g.data, i),
                s.ship.cargo.kg[i]
            ),
            c,
        );
    }
    let b = fb.rows() - 4;
    fb.text(
        1,
        b,
        "ENTREE:VENDRE TOUT  S:VENDRE 100KG  B:ACHETER 100KG",
        DIM,
    );
    fb.text(
        1,
        b + 1,
        "F:PLEIN CARBURANT  R:REPARER  U:DECOLLER  HAUT/BAS:CHOISIR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 2 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_inventory(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "SOUTE / INVENTAIRE");
    let c = &g.sim.ship.cargo;
    let x = 40;
    fb.text(
        x,
        3,
        "RESSOURCE            MASSE KG    VOLUME M3   VALEUR BASE",
        DIM,
    );
    let items: Vec<usize> = (0..g.data.resources.len())
        .filter(|&i| c.kg[i] > 0.01)
        .collect();
    if items.is_empty() {
        fb.text(x, 5, "SOUTE VIDE - MINEZ UN ASTEROIDE [V]", DIM);
    }
    let sel = g.sel_res;
    for (line, &i) in items.iter().take(fb.rows().saturating_sub(10)).enumerate() {
        let r = &g.data.resources[i];
        let col = if i == sel { AMBER } else { GREEN };
        fb.text(
            x,
            4 + line,
            &format!(
                "{}{:<18} {:>10.1} {:>11.2} {:>12.0}",
                if i == sel { ">" } else { " " },
                r.name,
                c.kg[i],
                c.kg[i] / r.density,
                c.kg[i] * r.value
            ),
            col,
        );
    }
    fb.text(
        1,
        fb.rows() - 3,
        "HAUT/BAS:CHOISIR  J:JETER LA RESSOURCE  I/ECHAP:RETOUR",
        DIM,
    );
    if c.is_full() {
        fb.text(1, fb.rows() - 5, "SOUTE PLEINE", RED);
    }
}
