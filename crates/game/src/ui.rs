//! Text-grid screens: main menu, pause, station market, inventory.
use crate::game::{Game, Screen, MAIN_MENU, PAUSE_MENU};
use crate::hud::{AMBER, DIM, GREEN, RED};
use vector_render::Framebuffer;
use vector_sim::{G0, RESALE};

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
        Screen::Ship => draw_ship(fb, g),
        Screen::Shipyard => draw_shipyard(fb, g),
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
    fb.gauge(
        1,
        3,
        "CARBURANT",
        s.ship.fuel / s.ship.stats.fuel_cap.max(1.0),
        20,
        GREEN,
    );
    fb.gauge(
        1,
        4,
        "COQUE    ",
        s.ship.hull / s.ship.stats.hull_max.max(1.0),
        20,
        GREEN,
    );
    fb.gauge(
        1,
        5,
        "SOUTE MAS",
        s.ship.cargo.mass / s.ship.cargo.cap_mass.max(1.0),
        20,
        GREEN,
    );
    fb.gauge(
        1,
        6,
        "SOUTE VOL",
        s.ship.cargo.volume / s.ship.cargo.cap_vol.max(1.0),
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
        "F:CARBURANT R:REPARER H:VAISSEAU Y:CHANTIER U:DECOLLER HAUT/BAS:CHOISIR",
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

fn bar(frac: f64, width: usize) -> String {
    let f = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
    format!("[{}{}]", "#".repeat(f), "-".repeat(width - f))
}

fn draw_ship(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let ship = &s.ship;
    let hull = &g.data.hulls[ship.design.hull];
    header(fb, g, &format!("VAISSEAU {}  ({})", hull.name, hull.class));
    let top = 8;
    fb.text(
        1,
        top,
        "N  TYPE      MODULE                       SANTE       ALIM",
        DIM,
    );
    for (i, &(ty, size)) in hull.slots.iter().enumerate() {
        let sel = i == g.ship_sel;
        let c = if sel { AMBER } else { GREEN };
        let (name, health, powered) = match ship.design.slots[i] {
            Some(m) => (
                g.data.modules[m].name.clone(),
                ship.sys.health[i],
                ship.sys.powered[i],
            ),
            None => ("(vide)".to_string(), 0.0, false),
        };
        let filled = ship.design.slots[i].is_some();
        let hc = if !filled {
            DIM
        } else if health < 0.3 {
            RED
        } else {
            c
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<2} {:<9}{} {:<28.28}",
                if sel { ">" } else { " " },
                i + 1,
                ty.label(),
                size,
                name
            ),
            c,
        );
        if filled {
            fb.text(
                46,
                top + 1 + i,
                &format!("{} {:>3.0}%", bar(health, 8), health * 100.0),
                hc,
            );
            fb.text(
                62,
                top + 1 + i,
                if powered { "ON" } else { "OFF" },
                if powered { GREEN } else { RED },
            );
        }
    }
    // Right panel: derived numbers.
    let st = &ship.stats;
    let x = 90;
    let weight = ship.mass() * G0;
    let lines = [
        format!("MASSE     {:>9.0} KG", ship.mass()),
        format!("POUSSEE   {:>9.0} KN", s.effective.thrust / 1e3),
        format!("TWR       {:>9.2}", st.thrust / weight),
        format!("DELTA-V   {:>9.1} KM/S", ship.delta_v() / 1e3),
        format!("ISP       {:>9.0} S", st.isp),
        format!("CARBURANT {:>9.0}/{:.0}", ship.fuel, st.fuel_cap),
        format!("PRODUCT.  {:>9.1} MW", s.ship.sys.supply_kw / 1e3),
        format!("DEMANDE   {:>9.1} MW", s.ship.sys.demand_kw / 1e3),
        format!("CAPACITE  {:>9.1} MW", st.power_capacity / 1e3),
        format!(
            "BATTERIE  {:>9.0}/{:.0} MJ",
            ship.sys.battery_mj, st.battery_cap
        ),
        format!(
            "RADIATEUR {:>9.1} MW",
            (st.radiate_kw + 30.0 + 0.004 * st.mass_dry) / 1e3
        ),
        format!("TEMP      {:>9.0} K", ship.sys.temp_k),
        format!("COQUE     {:>9.0}/{:.0}", ship.hull, st.hull_max),
        format!("MINAGE    {:>9.0} KG/S", st.mining_rate),
        format!("SOUTE     {:>9.0} KG", st.cargo_kg),
        format!("EQUIPAGE  {:>9.0} PLACES", st.crew_cap),
    ];
    for (i, l) in lines.iter().enumerate() {
        fb.text(x, top + i, l, GREEN);
    }
    if g.picker {
        let list = g.compatible_modules(g.ship_sel);
        let y0 = top + hull.slots.len() + 3;
        fb.text(
            1,
            y0 - 1,
            "CHOISIR UN MODULE (ENTREE = INSTALLER / ACHETER, ECHAP = ANNULER)",
            AMBER,
        );
        let rows = fb.rows().saturating_sub(y0 + 4);
        let start = g
            .pick_sel
            .saturating_sub(rows / 2)
            .min(list.len().saturating_sub(rows));
        for (line, k) in (start..(start + rows).min(list.len())).enumerate() {
            let m = &g.data.modules[list[k]];
            let sel = k == g.pick_sel;
            fb.text(
                1,
                y0 + line,
                &format!(
                    "{}{:<26.26} {:<12.12} T{} {:>8.0} KG {:>9.0} CR  STOCK {}",
                    if sel { ">" } else { " " },
                    m.name,
                    m.category,
                    m.size,
                    m.mass,
                    m.cost,
                    s.module_stock[list[k]]
                ),
                if sel { AMBER } else { GREEN },
            );
        }
    }
    let b = fb.rows() - 4;
    let help = if s.docked.is_some() {
        "HAUT/BAS:EMPLACEMENT ENTREE:INSTALLER X:RETIRER Y:CHANTIER NAVAL H/ECHAP:RETOUR"
    } else {
        "HAUT/BAS:EMPLACEMENT   (AMARREZ-VOUS A UNE STATION POUR MODIFIER)   H/ECHAP:RETOUR"
    };
    fb.text(1, b, help, DIM);
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
    if ship.sys.overheating {
        fb.text(x, top + lines.len() + 1, "*** SURCHAUFFE ***", RED);
    }
}

fn draw_shipyard(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "CHANTIER NAVAL");
    let cur = g.sim.ship.design.hull;
    let trade = g.data.hulls[cur].cost * RESALE;
    let top = 8;
    fb.text(
        1,
        top,
        "COQUE                    CLASSE        MASSE    EMPL.    PRIX (REPRISE DEDUITE)",
        DIM,
    );
    for (i, h) in g.data.hulls.iter().enumerate() {
        let sel = i == g.yard_sel;
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<24.24} {:<12.12} {:>7.0} {:>6} {:>12.0} CR{}",
                if sel { ">" } else { " " },
                h.name,
                h.class,
                h.mass,
                h.slots.len(),
                h.cost - trade,
                if i == cur { "  (ACTUELLE)" } else { "" }
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    let b = fb.rows() - 4;
    fb.text(
        1,
        b,
        "ENTREE:ACHETER (MODULES -> STOCK, SOUTE VIDE REQUISE)  ECHAP:RETOUR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}
