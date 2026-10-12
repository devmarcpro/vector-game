//! The CRYO tab of the crew screen (C): the caissons, who sleeps in them and since when, the
//! temperature, ice and radiation of the selected sleeper, the scheduled wakes, the automatic
//! plan and the watch rotation, all as clickable words. Everything comes from `vector_sim::cryo`.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use crate::ui::bar;
use vector_render::Framebuffer;
use vector_sim::cryo::{When, Who};
use winit::keyboard::KeyCode;

const ICE: u32 = 0x0050_C0E0;
/// Choices of the delay of a dated wake, days.
pub const DELAYS: [f64; 6] = [7.0, 30.0, 90.0, 365.0, 1000.0, 3650.0];
/// Choices of the watch rotation, days (0 = none).
pub const ROTATIONS: [f32; 4] = [0.0, 7.0, 30.0, 90.0];

/// A span of days as text.
pub fn span(days: f64) -> String {
    if days < 1.0 {
        format!("{:.0} H", days * 24.0)
    } else if days < 400.0 {
        format!("{days:.0} J")
    } else {
        format!("{:.1} ANS", days / 365.25)
    }
}

fn order_text(g: &Game, o: &vector_sim::cryo::WakeOrder) -> String {
    let who = match o.who {
        Who::All => "TOUS".to_string(),
        Who::Best => "LE MEILLEUR VEILLEUR".to_string(),
        Who::One(id) => g
            .sim
            .crew
            .members
            .iter()
            .find(|p| p.id == id)
            .map_or("?".to_string(), |p| p.name.to_uppercase()),
    };
    let when = match o.when {
        When::At(t) => format!("DANS {}", span(((t - g.sim.time) / 86_400.0).max(0.0))),
        When::Arrival => "A L'ARRIVEE".to_string(),
        When::Alert => "SUR ALERTE CRITIQUE".to_string(),
    };
    format!("{who} {when}")
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let crew = &s.crew;
    let top = 8;
    fb.text(1, top, "CRYOGENISATION  (C : RETOUR A LA LISTE)", AMBER);
    // Who sleeps, who watches.
    for (i, p) in crew.members.iter().enumerate() {
        let sel = i == g.crew_sel;
        g.hit_cells(1, top + 2 + i, 50, 1, Act::Call(Game::click_crew, i), None);
        let (state, col) = if !p.alive {
            ("MORT".to_string(), DIM)
        } else if p.body.cryo_wake > 0.0 {
            (format!("REVEIL {:.0} H", p.body.cryo_wake), AMBER)
        } else if p.body.cryo {
            (
                format!(
                    "CRYO {:>4.0} C  {}",
                    p.body.core_temp,
                    span((s.time - p.body.cryo_since) / 86_400.0)
                ),
                ICE,
            )
        } else {
            ("VEILLE".to_string(), GREEN)
        };
        let col = if sel && p.alive { AMBER } else { col };
        fb.text(
            1,
            top + 2 + i,
            &format!(
                "{}{:<19.19} {:>3.0}A {}",
                if sel { ">" } else { " " },
                p.name,
                p.age,
                state
            ),
            col,
        );
    }
    // The caissons.
    let x = 62;
    let st = s.cryo_status();
    fb.text(x, top, "CAISSONS", AMBER);
    let places = if st.hospital {
        "HOPITAL DE LA STATION".to_string()
    } else {
        format!(
            "{} PLACES ALIMENTEES / {} INSTALLEES",
            st.pods, st.pods_installed
        )
    };
    fb.text(
        x,
        top + 1,
        &places,
        if st.pods == 0 && !st.hospital {
            RED
        } else {
            GREEN
        },
    );
    fb.text(
        x,
        top + 2,
        &format!(
            "DORMEURS {}  RETENUS {}  EN REVEIL {}  LIBRES {}",
            st.sleepers,
            st.held,
            st.waking,
            if st.hospital {
                "TOUTES".to_string()
            } else {
                st.free.to_string()
            }
        ),
        if st.sleepers > st.held { RED } else { GREEN },
    );
    fb.text(
        x,
        top + 3,
        &format!(
            "FIABILITE {:.1} %  CONSIGNE {:.0} C  INFIRMERIE {}",
            st.rel * 100.0,
            st.temp,
            if st.infirmary { "ALIMENTEE" } else { "NON" }
        ),
        GREEN,
    );
    let stock = |id: &str| {
        g.data
            .resource_index(id)
            .map_or(0.0, |r| s.ship.cargo.kg[r])
    };
    fb.text(
        x,
        top + 4,
        &format!(
            "CRYOPROTECTEUR {:.1} KG  PIECES {:.1} KG  COLONS ENDORMIS {}",
            stock("cryoprotectant"),
            stock("spare_parts"),
            st.colonists
        ),
        if stock("cryoprotectant") < 1.5 {
            AMBER
        } else {
            GREEN
        },
    );
    // The selected sleeper.
    fb.text(x, top + 6, "DORMEUR CHOISI", AMBER);
    if let Some(p) = crew.members.get(g.crew_sel).filter(|p| p.alive) {
        let b = &p.body;
        fb.text(
            x,
            top + 7,
            &format!(
                "{} - {}  AGE BIOLOGIQUE {:.2}",
                p.name.to_uppercase(),
                g.data.roles[p.role].name.to_uppercase(),
                p.age
            ),
            GREEN,
        );
        if b.cryo {
            let days = (s.time - b.cryo_since) / 86_400.0;
            let rate = vector_sim::body::bio_rate(b.core_temp);
            fb.text(
                x,
                top + 8,
                &format!("ENDORMI DEPUIS {}  VIEILLIT x{:.4}", span(days), rate),
                ICE,
            );
            fb.text(
                x,
                top + 9,
                &format!(
                    "TEMPERATURE {:.0} C   CRYOPROTECTEUR {:.0} %",
                    b.core_temp,
                    b.cryo_cpa * 100.0
                ),
                ICE,
            );
            let integ = (1.0 - b.cryo_ice).clamp(0.0, 1.0);
            fb.text(
                x,
                top + 10,
                &format!(
                    "INTEGRITE {} {:>3.0} %  (CRISTAUX DE GLACE)",
                    bar(f64::from(integ), 14),
                    integ * 100.0
                ),
                if integ < 0.5 {
                    RED
                } else if integ < 0.85 {
                    AMBER
                } else {
                    GREEN
                },
            );
            fb.text(
                x,
                top + 11,
                &format!(
                    "DOSE ACCUMULEE {:.2} SV (NE SE REPARE PAS EN CRYO)",
                    b.acute_dose
                ),
                if b.acute_dose > 1.0 { RED } else { GREEN },
            );
            if let Some(r) = s.cryo_wake_risk(g.crew_sel) {
                fb.text(
                    x,
                    top + 12,
                    &format!("RISQUE D'UN REVEIL RATE : {:.0} %", r * 100.0),
                    if r > 0.15 { RED } else { AMBER },
                );
            }
        } else {
            fb.text(
                x,
                top + 8,
                &format!(
                    "EN VEILLE.  MAL DE CRYO {:.0} %  DERNIER DEGEL {}",
                    b.cryo_sick * 100.0,
                    if b.cryo_last < -1.0e17 {
                        "JAMAIS".to_string()
                    } else {
                        format!("IL Y A {}", span((s.time - b.cryo_last) / 86_400.0))
                    }
                ),
                GREEN,
            );
        }
    }
    // The programme.
    fb.text(x, top + 14, "PROGRAMME", AMBER);
    if crew.cryo.orders.is_empty() {
        fb.text(x, top + 15, "AUCUN REVEIL PROGRAMME", DIM);
    }
    for (k, o) in crew.cryo.orders.iter().take(5).enumerate() {
        fb.text(
            x,
            top + 15 + k,
            &format!("REVEIL : {}", order_text(g, o)),
            GREEN,
        );
    }
    let rot = crew.cryo.rotation_days;
    fb.text(
        x,
        top + 20,
        &format!(
            "ROTATION DES VEILLEURS : {}   PLAN AUTO : TOUS SAUF {}",
            if rot > 0.0 {
                format!("TOUS LES {} J", rot as i32)
            } else {
                "NON".to_string()
            },
            g.cryo_keep
        ),
        GREEN,
    );
    // Actions as clickable words.
    let acts: [(&str, KeyCode); 6] = [
        ("E:ENDORMIR", KeyCode::KeyE),
        ("R:REVEILLER", KeyCode::KeyR),
        ("U:REVEIL D'URGENCE", KeyCode::KeyU),
        ("A:A L'ARRIVEE", KeyCode::KeyA),
        ("L:SUR ALERTE", KeyCode::KeyL),
        ("X:EFFACER", KeyCode::KeyX),
    ];
    let acts2: [(&str, KeyCode); 5] = [
        ("D:A LA DATE", KeyCode::KeyD),
        ("P:PLAN AUTO", KeyCode::KeyP),
        ("T:ROTATION", KeyCode::KeyT),
        ("K:ENDORMIR COLONS", KeyCode::KeyK),
        ("B:REVEILLER COLONS", KeyCode::KeyB),
    ];
    for (row, list) in [(top + 22, &acts[..]), (top + 23, &acts2[..])] {
        let mut cx = x;
        for (label, key) in list {
            g.hit_cells(cx, row, label.len(), 1, Act::Key(*key), None);
            fb.text(cx, row, label, AMBER);
            cx += label.len() + 2;
        }
    }
    g.hit_cells(x, top + 24, 2, 1, Act::Key(KeyCode::ArrowLeft), None);
    g.hit_cells(x + 24, top + 24, 2, 1, Act::Key(KeyCode::ArrowRight), None);
    fb.text(
        x,
        top + 24,
        &format!(
            "< DELAI {} >   PGUP/PGDN VEILLEURS {}",
            span(DELAYS[g.cryo_delay.min(DELAYS.len() - 1)]),
            g.cryo_keep
        ),
        GREEN,
    );
    let bot = fb.rows() - 4;
    fb.text(
        1,
        bot,
        "HAUT/BAS PERSONNE  E ENDORMIR  R REVEILLER  U URGENCE  A/L/D REVEIL PROGRAMME  P PLAN AUTO  T ROTATION  K/B COLONS  C/ECHAP RETOUR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, bot + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}
