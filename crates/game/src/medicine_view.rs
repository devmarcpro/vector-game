//! The MEDECINE tab of the crew screen (M): an ASCII silhouette whose parts take the colour of
//! their worst wound, vital signs, wounds, drugs in the blood, disabilities, and the care actions
//! as clickable words. Everything shown comes from the body model of `vector_sim::body`.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use crate::ui::bar;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

/// The silhouette: (text, which part each column range belongs to). Columns 0..9 are the left arm
/// (or leg), 9..17 the trunk and head, 17..26 the right arm (or leg).
#[derive(Clone, Copy)]
enum Zone {
    Head,
    Body,
    Legs,
}

const ART: [(&str, Zone); 17] = [
    ("         _____          ", Zone::Head),
    ("        /     \\         ", Zone::Head),
    ("        | o o |         ", Zone::Head),
    ("        \\__-__/         ", Zone::Head),
    ("   _____/|     |\\_____  ", Zone::Body),
    ("  /  ___/|     |\\___  \\ ", Zone::Body),
    (" /  /    | ::: |    \\  \\", Zone::Body),
    (" |  |    | ::: |    |  |", Zone::Body),
    (" |  |    | ::: |    |  |", Zone::Body),
    (" \\__/    |_____|    \\__/", Zone::Body),
    ("          |  |  |         ", Zone::Legs),
    ("         /   |   \\        ", Zone::Legs),
    ("         |   |   |        ", Zone::Legs),
    ("         |   |   |        ", Zone::Legs),
    ("         |   |   |        ", Zone::Legs),
    ("         |   |   |        ", Zone::Legs),
    ("        _|   |   |_       ", Zone::Legs),
];

/// 0 unhurt .. 1 worst: colour of a part.
fn part_state(p: &vector_sim::crew::Person, part: usize) -> f32 {
    let wounds = p
        .body
        .wounds
        .iter()
        .filter(|w| w.part as usize == part)
        .map(|w| w.sev)
        .fold(0.0f32, f32::max);
    let lame = p.body.disability.get(part).copied().unwrap_or(0.0);
    wounds.max(lame * 0.8)
}

fn state_colour(v: f32) -> u32 {
    if v <= 0.0 {
        GREEN
    } else if v < 0.5 {
        AMBER
    } else {
        RED
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let crew = &s.crew;
    let top = 8;
    fb.text(1, top, "MEDECINE  (M : RETOUR A LA LISTE)", AMBER);
    // The crew, with a mark for who needs care.
    for (i, p) in crew.members.iter().enumerate() {
        let sel = i == g.crew_sel;
        g.hit_cells(1, top + 2 + i, 34, 1, Act::Call(Game::click_crew, i), None);
        let mark = if !p.alive {
            "MORT"
        } else if p.body.cryo {
            "CRYO"
        } else if p.body.unconscious() {
            "INCONSCIENT"
        } else if p.body.is_hurt() || p.disease.is_some() {
            "SOINS"
        } else {
            ""
        };
        let col = if sel {
            AMBER
        } else if !p.alive {
            DIM
        } else if mark == "SOINS" || mark == "INCONSCIENT" {
            RED
        } else {
            GREEN
        };
        fb.text(
            1,
            top + 2 + i,
            &format!(
                "{}{:<19.19} {:>3.0}% {}",
                if sel { ">" } else { " " },
                p.name,
                p.health * 100.0,
                mark
            ),
            col,
        );
    }
    let Some(p) = crew.members.get(g.crew_sel) else {
        return;
    };
    // Silhouette.
    let ax = 40;
    let ay = top + 2;
    let name_of = |id: &str| g.data.part_index(id);
    let (head, torso) = (name_of("tete"), name_of("torse"));
    let (arm_l, arm_r) = (name_of("bras_g"), name_of("bras_d"));
    let (leg_l, leg_r) = (name_of("jambe_g"), name_of("jambe_d"));
    for (row, (text, zone)) in ART.iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        for (c, ch) in chars.iter().enumerate() {
            if *ch == ' ' {
                continue;
            }
            let part = match zone {
                Zone::Head => head,
                Zone::Body => {
                    if c < 9 {
                        arm_l
                    } else if c < 17 {
                        torso
                    } else {
                        arm_r
                    }
                }
                Zone::Legs => {
                    if c < 13 {
                        leg_l
                    } else {
                        leg_r
                    }
                }
            };
            let v = part.map_or(0.0, |k| part_state(p, k));
            let col = if p.body.cryo {
                0x0050_C0E0
            } else {
                state_colour(v)
            };
            fb.text(ax + c, ay + row, &ch.to_string(), col);
        }
    }
    fb.text(
        ax,
        ay + ART.len() + 1,
        "VERT SAIN  AMBRE BLESSE  ROUGE GRAVE",
        DIM,
    );
    // Vital signs.
    let x = 70;
    fb.text(
        x,
        top,
        &format!(
            "{} - {} ({:.0} ANS)",
            p.name.to_uppercase(),
            g.data.roles[p.role].name.to_uppercase(),
            p.age
        ),
        AMBER,
    );
    let b = &p.body;
    let vitals = [
        ("SANTE", p.health, p.health < 0.4),
        ("SANG", b.blood, b.blood < 0.6),
        ("OXYGENE", b.oxygen, b.oxygen < 0.6),
        ("DOULEUR", b.pain, b.pain > 0.6),
        ("TRAUMA", b.trauma, b.trauma > 0.6),
        ("OS", b.bone, b.bone < 0.8),
        ("FATIGUE", p.fatigue, p.fatigue > 0.8),
        ("FAIM", p.hunger, p.hunger > 0.8),
        ("SOIF", p.thirst, p.thirst > 0.7),
    ];
    for (k, (label, v, bad)) in vitals.iter().enumerate() {
        fb.text(
            x,
            top + 1 + k,
            &format!("{:<8}{} {:>3.0}%", label, bar(f64::from(*v), 14), v * 100.0),
            if *bad { RED } else { GREEN },
        );
    }
    let t = b.core_temp;
    fb.text(
        x,
        top + 10,
        &format!("TEMPERATURE {t:.1} C   DOSE RAYONS {:.2} SV", b.acute_dose),
        if !(35.0..=39.0).contains(&t) || b.acute_dose > 1.0 {
            RED
        } else {
            GREEN
        },
    );
    // Wounds (selectable) and what the carer can see.
    let skill = s.carer_skill(g.crew_sel);
    let (seen, hidden) = b.diagnose(&g.data, skill, s.effective.medbay);
    fb.text(
        x,
        top + 12,
        &format!(
            "BLESSURES  (SOIGNANT {:.0}  INFIRMERIE {})",
            skill,
            if s.effective.medbay > 0.0 {
                "OUI"
            } else {
                "NON"
            }
        ),
        AMBER,
    );
    if seen.is_empty() && hidden == 0 {
        fb.text(x, top + 13, "AUCUNE", GREEN);
    }
    let wsel = g.med_wound.min(seen.len().saturating_sub(1));
    for (k, (wi, sev)) in seen.iter().take(8).enumerate() {
        let w = &b.wounds[*wi];
        let def = &g.data.wounds[w.kind as usize];
        g.hit_cells(
            x,
            top + 13 + k,
            50,
            1,
            Act::Call(Game::click_wound, k),
            None,
        );
        fb.text(
            x,
            top + 13 + k,
            &format!(
                "{}{:<22.22} {:<12.12} {} {:>3.0}%{}{}",
                if k == wsel { ">" } else { " " },
                def.name.to_uppercase(),
                g.data.body_parts[w.part as usize].name.to_uppercase(),
                bar(f64::from(*sev), 8),
                sev * 100.0,
                if w.dressed > 0.0 { " P" } else { "" },
                if w.infection > 0.0 { " INFECTEE" } else { "" }
            ),
            if k == wsel { AMBER } else { state_colour(*sev) },
        );
    }
    if hidden > 0 {
        fb.text(
            x,
            top + 22,
            "LESIONS INTERNES SOUPCONNEES : MEDECIN (3) OU INFIRMERIE REQUIS",
            RED,
        );
    }
    // Drugs in the blood.
    fb.text(x, top + 24, "MEDICAMENTS DANS LE SANG", AMBER);
    for (k, d) in b.drugs.iter().take(5).enumerate() {
        let def = &g.data.drugs[d.drug as usize];
        fb.text(
            x,
            top + 25 + k,
            &format!(
                "{:<16.16} {} TOL {:>3.0}% DEP {:>3.0}%",
                def.name.to_uppercase(),
                bar(f64::from(d.level.min(2.0) / 2.0), 8),
                d.tolerance * 100.0,
                d.dependence * 100.0
            ),
            if d.level > 1.6 { RED } else { GREEN },
        );
    }
    let liver = g
        .data
        .organ_index("foie")
        .and_then(|o| p.organs.get(o).copied())
        .unwrap_or(1.0);
    let pressure = b.overdose_pressure(&g.data, liver);
    if pressure > vector_sim::body::OVERDOSE {
        fb.text(x, top + 30, "SURDOSE EN COURS", RED);
    }
    // Disabilities.
    let lame: Vec<String> = b
        .disability
        .iter()
        .enumerate()
        .filter(|(_, d)| **d > 0.05)
        .map(|(i, d)| {
            format!(
                "{} {:.0}%",
                g.data.body_parts[i].name.to_uppercase(),
                d * 100.0
            )
        })
        .collect();
    fb.text(
        x,
        top + 31,
        &format!(
            "HANDICAPS : {}",
            if lame.is_empty() {
                "AUCUN".to_string()
            } else {
                lame.join(", ")
            }
        ),
        if lame.is_empty() { GREEN } else { AMBER },
    );
    // Care actions: clickable words.
    let ay2 = top + 33;
    let drug = g.med_drug.min(g.data.drugs.len().saturating_sub(1));
    let held = g.data.drugs.get(drug).map_or(0.0, |d| {
        g.data
            .resource_index(&d.resource)
            .map_or(0.0, |r| s.ship.cargo.kg[r])
    });
    let actions: [(&str, KeyCode); 6] = [
        ("A:PREMIERS SOINS", KeyCode::KeyA),
        ("S:OPERER LA PLAIE", KeyCode::KeyS),
        ("ENTREE:DONNER", KeyCode::Enter),
        ("R:CHOC ELECTRIQUE", KeyCode::KeyR),
        ("C:CRYOSTASE", KeyCode::KeyC),
        ("TAB:PLAIE SUIVANTE", KeyCode::Tab),
    ];
    let mut cx = x;
    for (label, key) in actions {
        g.hit_cells(cx, ay2, label.len(), 1, Act::Key(key), None);
        fb.text(cx, ay2, label, AMBER);
        cx += label.len() + 2;
    }
    if let Some(d) = g.data.drugs.get(drug) {
        g.hit_cells(x, ay2 + 1, 2, 1, Act::Key(KeyCode::ArrowLeft), None);
        g.hit_cells(x + 40, ay2 + 1, 2, 1, Act::Key(KeyCode::ArrowRight), None);
        fb.text(
            x,
            ay2 + 1,
            &format!(
                "< {} >   EN SOUTE {:.2} KG   DOSE {:.2} KG",
                d.name.to_uppercase(),
                held,
                d.dose
            ),
            GREEN,
        );
        fb.text(x, ay2 + 2, &d.text.to_uppercase(), DIM);
    }
    let bot = fb.rows() - 4;
    fb.text(
        1,
        bot,
        "HAUT/BAS PERSONNE  GAUCHE/DROITE MEDICAMENT  TAB PLAIE  ENTREE DONNER  A SOINS  S OPERER  R CHOC  C CRYO  M/ECHAP RETOUR",
        DIM,
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, bot + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}
