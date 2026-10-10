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
        Screen::Crew => draw_crew(fb, g),
        Screen::Recruit => draw_recruit(fb, g),
        Screen::Journal => draw_journal(fb, g),
        Screen::Galaxy => draw_galaxy(fb, g),
        Screen::Contracts => draw_contracts(fb, g),
        Screen::Bank => draw_bank(fb, g),
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
    let owner = match s.faction {
        Some(f) => format!(
            "{}  REP {:+.0}  TAXE {:.0}%",
            s.data.factions[f].name.to_uppercase(),
            s.reputation_with(Some(f)),
            s.tax_rate() * 100.0
        ),
        None => "STATION INDEPENDANTE".to_string(),
    };
    fb.text(
        1,
        7,
        &format!(
            "{}  |  {}",
            owner,
            s.data.industries[m.industry].name.to_uppercase()
        ),
        GREEN,
    );
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
        "ENTREE:VENDRE TOUT  S:VENDRE 100KG  B:ACHETER 100KG  C:CONTRATS  E:BANQUE",
        DIM,
    );
    fb.text(
        1,
        b + 1,
        "F:CARBURANT R:REPARER H:VAISSEAU Y:CHANTIER K:EQUIPAGE N:RECRUTER U:DECOLLER",
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

fn stamp(t: f64) -> String {
    let day = (t / 86_400.0).floor();
    let hour = ((t % 86_400.0) / 3600.0).floor();
    format!("J{:<4}{:02}h", day as i64, hour as i64)
}

fn draw_crew(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    header(fb, g, "EQUIPAGE");
    let crew = &s.crew;
    fb.text(
        28,
        2,
        &format!(
            "MEMBRES {}/{:.0}  AIR {:.0}%  PAIE {:.0} CR/JOUR",
            crew.count(),
            s.ship.stats.crew_cap,
            crew.air * 100.0,
            crew.daily_payroll()
        ),
        GREEN,
    );
    let top = 8;
    fb.text(
        1,
        top,
        "NOM                  POSTE          Q  SANTE    MORAL    FATIGUE  FAIM     ETAT",
        DIM,
    );
    for (i, p) in crew.members.iter().enumerate() {
        let sel = i == g.crew_sel;
        let c = if !p.alive {
            DIM
        } else if sel {
            AMBER
        } else {
            GREEN
        };
        let state = if !p.alive {
            "MORT".to_string()
        } else if let Some((d, left)) = p.disease {
            format!("{} {:.0}J", g.data.diseases[d].name, left)
        } else if p.on_duty(s.time) {
            "AU TRAVAIL".to_string()
        } else {
            "REPOS".to_string()
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<19.19} {:<14.14} {}  {} {} {} {} {}",
                if sel { ">" } else { " " },
                p.name,
                g.data.roles[p.role].name,
                p.shift + 1,
                bar(f64::from(p.health), 6),
                bar(f64::from(p.morale), 6),
                bar(f64::from(p.fatigue), 6),
                bar(f64::from(p.hunger), 6),
                state
            ),
            c,
        );
    }
    if let Some(p) = crew.members.get(g.crew_sel) {
        let x = 100;
        fb.text(
            x,
            top,
            &format!("{} - {}", p.name, g.data.cultures[p.culture].name),
            AMBER,
        );
        fb.text(
            x,
            top + 1,
            &format!(
                "AGE {:.0}  SALAIRE {:.0} CR/J  LOYAUTE {:.0}%",
                p.age,
                p.salary,
                p.loyalty * 100.0
            ),
            GREEN,
        );
        let traits: Vec<&str> = p
            .traits
            .iter()
            .map(|&t| g.data.traits[t].name.as_str())
            .collect();
        fb.text(x, top + 2, &format!("TRAITS: {}", traits.join(", ")), GREEN);
        let mut sk: Vec<(usize, f32)> = p.skills.iter().copied().enumerate().collect();
        sk.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (k, (si, v)) in sk.iter().take(6).enumerate() {
            fb.text(
                x,
                top + 4 + k,
                &format!(
                    "{:<13} {} {:.1}",
                    g.data.skills[*si].name,
                    bar(f64::from(*v) / 10.0, 10),
                    v
                ),
                GREEN,
            );
        }
        let mut y = top + 11;
        fb.text(x, y, "RELATIONS:", DIM);
        for (oid, r) in p.relations.iter().take(6) {
            if let Some(o) = crew.members.iter().find(|m| m.id == *oid) {
                y += 1;
                fb.text(
                    x,
                    y,
                    &format!("{:<19.19} {:+}", o.name, r),
                    if *r < 0 { RED } else { GREEN },
                );
            }
        }
    } else {
        fb.text(
            1,
            top + 2,
            "AUCUN MEMBRE D EQUIPAGE - RECRUTEZ A UNE STATION [N]",
            DIM,
        );
    }
    let b = fb.rows() - 4;
    fb.text(1, b, "HAUT/BAS:CHOISIR  GAUCHE/DROITE:POSTE  T:QUART  X:DEBARQUER (A QUAI)  J:JOURNAL  N:RECRUTER  K/ECHAP:RETOUR", DIM);
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_recruit(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "RECRUTEMENT");
    let cands = g.sim.candidates();
    let top = 8;
    fb.text(1, top, "NOM                  CULTURE       AGE  MEILLEURE COMPETENCE     TRAITS                      EMBAUCHE", DIM);
    for (i, p) in cands.iter().enumerate() {
        let sel = i == g.recruit_sel;
        let (bi, bv) = p.best_skill();
        let traits: Vec<&str> = p
            .traits
            .iter()
            .map(|&t| g.data.traits[t].name.as_str())
            .collect();
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<19.19} {:<13.13} {:>3.0}  {:<14.14} {:>4.1}   {:<27.27} {:>6.0} CR",
                if sel { ">" } else { " " },
                p.name,
                g.data.cultures[p.culture].name,
                p.age,
                g.data.skills[bi].name,
                bv,
                traits.join(", "),
                vector_sim::crew::hire_cost(p)
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    fb.text(
        1,
        top + 9,
        &format!(
            "COUCHETTES: {}/{:.0}   (LE NOUVEAU MEMBRE PREND LE POSTE DE SA MEILLEURE COMPETENCE)",
            g.sim.crew.count(),
            g.sim.ship.stats.crew_cap
        ),
        DIM,
    );
    let b = fb.rows() - 4;
    fb.text(
        1,
        b,
        "HAUT/BAS:CHOISIR  ENTREE:EMBAUCHER  K:EQUIPAGE  ECHAP:RETOUR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_journal(fb: &mut Framebuffer, g: &Game) {
    fb.text(1, 1, "JOURNAL DE BORD", AMBER);
    let log = &g.sim.crew.log;
    let rows = fb.rows().saturating_sub(6);
    let end = log.len().saturating_sub(g.journal_scroll.min(log.len()));
    let start = end.saturating_sub(rows);
    for (i, e) in log[start..end].iter().enumerate() {
        fb.text(1, 3 + i, &format!("{}  {}", stamp(e.time), e.text), GREEN);
    }
    if log.is_empty() {
        fb.text(1, 3, "(RIEN A SIGNALER)", DIM);
    }
    fb.text(
        1,
        fb.rows() - 3,
        "HAUT/BAS:DEFILER  K:EQUIPAGE  J/ECHAP:RETOUR",
        DIM,
    );
}

fn class_color(c: vector_world::StarClass) -> u32 {
    use vector_world::StarClass::*;
    match c {
        O | B => 0x0070_90FF,
        A => 0x00D0_D8FF,
        F => 0x00E8_E8A0,
        G => 0x00F0_D860,
        K => 0x00F0_9040,
        M => 0x00FF_5040,
    }
}

fn draw_galaxy(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        1,
        &format!(
            "CARTE GALACTIQUE - {} ({})",
            s.system.name, s.system.designation
        ),
        AMBER,
    );
    let range = s.jump_range();
    fb.text(
        1,
        2,
        &format!(
            "PORTEE DE SAUT {range:.1} AL   CARBURANT {:.0} KG   ETOILES CONNUES {}",
            s.ship.fuel,
            g.gal_cache.len()
        ),
        GREEN,
    );
    let (cx, cy) = (fb.w as f32 * 0.42, fb.h as f32 * 0.52);
    let zoom = g.gal_zoom as f32;
    let proj = |p: [f64; 3]| -> (f32, f32) {
        (
            cx + (p[0] - s.here_pos[0]) as f32 * zoom,
            cy + (p[2] - s.here_pos[2]) as f32 * zoom,
        )
    };
    // Jump range ring and axes.
    if range > 0.0 {
        fb.circle(cx, cy, range as f32 * zoom, DIM);
    }
    fb.line(cx - 6.0, cy, cx + 6.0, cy, RED);
    fb.line(cx, cy - 6.0, cx, cy + 6.0, RED);
    fb.text_at(cx as i32 + 8, cy as i32 + 6, "ICI", RED);
    for (k, e) in g.gal_cache.iter().enumerate() {
        let (x, y) = proj(e.pos);
        let h = (e.pos[1] - s.here_pos[1]) as f32 * zoom * 0.5;
        let col = class_color(e.class());
        fb.line(x, y, x, y - h, DIM);
        fb.circle(x, y - h, 2.0, col);
        let visited = s.visited.contains_key(&e.id);
        if visited {
            fb.circle(x, y - h, 5.0, GREEN);
        }
        if k == g.gal_sel {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                fb.line(
                    x + sx * 9.0,
                    y - h + sy * 9.0,
                    x + sx * 5.0,
                    y - h + sy * 9.0,
                    AMBER,
                );
                fb.line(
                    x + sx * 9.0,
                    y - h + sy * 9.0,
                    x + sx * 9.0,
                    y - h + sy * 5.0,
                    AMBER,
                );
            }
            fb.text_at(
                x as i32 + 12,
                y as i32 - h as i32 - 4,
                &e.header.name,
                AMBER,
            );
        } else if zoom > 10.0 {
            fb.text_at(x as i32 + 6, y as i32 - h as i32 - 4, &e.header.name, DIM);
        }
    }
    // Info panel.
    let x = (fb.w as f32 * 0.78) as usize / 8;
    if let Some(e) = g.gal_cache.get(g.gal_sel) {
        let d = e.distance_to(s.here_pos);
        let ok = d <= range;
        fb.text(x, 4, &e.header.name, AMBER);
        fb.text(x, 5, &e.header.designation, DIM);
        fb.text(
            x,
            7,
            &format!(
                "CLASSE {}  MASSE {:.2} MSOL",
                e.header.class.letter(),
                e.header.mass_sun
            ),
            GREEN,
        );
        fb.text(
            x,
            8,
            &format!(
                "LUMINOSITE {:.3} LSOL  TEMP {:.0} K",
                e.header.luminosity, e.header.teff
            ),
            GREEN,
        );
        fb.text(x, 9, &format!("DISTANCE {d:.1} AL"), GREEN);
        let owner = s.galaxy.territory(e.pos, s.data.factions.len());
        let (oname, ocol) = owner.map_or(("NON REVENDIQUE".to_string(), DIM), |f| {
            (
                s.data.factions[f].name.to_uppercase(),
                s.data.factions[f].color,
            )
        });
        fb.text(x, 10, &oname, ocol);
        fb.text(
            x,
            11,
            if ok { "A PORTEE" } else { "HORS DE PORTEE" },
            if ok { GREEN } else { RED },
        );
        fb.text(
            x,
            12,
            &format!("CARBURANT {:.0} KG", s.jump_fuel_cost(d)),
            GREEN,
        );
        fb.text(
            x,
            13,
            &format!("TRAJET {:.0} H", d * vector_sim::voyage::HOURS_PER_LY),
            GREEN,
        );
        if s.visited.contains_key(&e.id) {
            fb.text(x, 15, "SYSTEME DEJA VISITE", DIM);
        }
    } else {
        fb.text(x, 4, "AUCUNE ETOILE CONNUE", DIM);
    }
    let b = fb.rows() - 4;
    fb.text(
        1,
        b,
        "HAUT/BAS:ETOILE  +/-:ZOOM  ENTREE:SAUTER (HORS DE TOUT CHAMP DE GRAVITE)  L/ECHAP:FERMER",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_contracts(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "CONTRATS");
    let s = &g.sim;
    let top = 8;
    fb.text(
        1,
        top,
        "OFFRES DE CETTE STATION (BESOINS EMERGENTS ET EXPORTS)",
        AMBER,
    );
    for (i, c) in g.offers.iter().enumerate() {
        let sel = i == g.contract_sel;
        let name = &g.data.resources[c.resource].name;
        let dest = if c.dist_ly == 0.0 {
            "ICI".to_string()
        } else {
            format!("{} ({:.1} AL)", c.dest_system, c.dist_ly)
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}LIVRER {:>5.0} KG {:<18.18} -> {:<24.24} {:>8.0} CR  {:.0} J",
                if sel { ">" } else { " " },
                c.kg,
                name,
                dest,
                c.reward,
                (c.expires - s.time) / 86_400.0
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    if g.offers.is_empty() {
        fb.text(1, top + 1, "(AUCUNE OFFRE POUR LE MOMENT)", DIM);
    }
    let y0 = top + 3 + g.offers.len().max(1);
    fb.text(
        1,
        y0,
        &format!(
            "CONTRATS ACTIFS ({}/{})",
            s.contracts.len(),
            vector_sim::economy::MAX_ACTIVE_CONTRACTS
        ),
        AMBER,
    );
    for (i, c) in s.contracts.iter().enumerate() {
        let sel = g.offers.len() + i == g.contract_sel;
        let have = s.ship.cargo.kg[c.resource];
        let here = c.dest_star == s.here && Some(c.dest_body) == s.docked;
        fb.text(
            1,
            y0 + 1 + i,
            &format!(
                "{}{:>5.0} KG {:<16.16} -> {:<22.22} {:>8.0} CR  {:.0} J  SOUTE {:.0}{}",
                if sel { ">" } else { " " },
                c.kg,
                g.data.resources[c.resource].name,
                c.dest_name,
                c.reward,
                (c.expires - s.time) / 86_400.0,
                have,
                if here { "  [ENTREE: LIVRER]" } else { "" }
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    let b = fb.rows() - 4;
    fb.text(
        1,
        b,
        "HAUT/BAS:CHOISIR  ENTREE:ACCEPTER / LIVRER  X:ABANDONNER (-1 REP)  ECHAP:RETOUR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_bank(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "BANQUE");
    let s = &g.sim;
    let top = 8;
    fb.text(
        1,
        top,
        &format!("PRET EN COURS     {:>12.0} CR", s.bank.loan),
        GREEN,
    );
    fb.text(
        1,
        top + 1,
        &format!("PLAFOND DE CREDIT  {:>12.0} CR", s.credit_limit()),
        GREEN,
    );
    fb.text(
        1,
        top + 2,
        &format!(
            "TAUX JOURNALIER    {:>11.2} %",
            vector_sim::economy::BANK_RATE_PER_DAY * 100.0
        ),
        GREEN,
    );
    fb.text(
        1,
        top + 3,
        &format!("INTERETS PAYES     {:>12.0} CR", s.bank.total_interest),
        GREEN,
    );
    if s.bank.overdue_days > 0 {
        fb.text(
            1,
            top + 5,
            &format!(
                "PRET EN SOUFFRANCE DEPUIS {} J - SAISIE A 30 J !",
                s.bank.overdue_days
            ),
            RED,
        );
    }
    fb.text(
        1,
        top + 7,
        "LE PLAFOND AUGMENTE AVEC VOTRE REPUTATION AUPRES DES FACTIONS.",
        DIM,
    );
    let b = fb.rows() - 4;
    fb.text(
        1,
        b,
        "HAUT:EMPRUNTER 1000  BAS:REMBOURSER 1000  ENTREE:TOUT REMBOURSER  ECHAP:RETOUR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}
