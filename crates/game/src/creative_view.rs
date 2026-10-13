//! The creative panel: every action calls the one `Sim::creative_*` API (also used by the
//! console); the panel only picks the action and shows the result. Keys and mouse alike.
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN, HOVER, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::rules::CreativeFlags;
use winit::keyboard::KeyCode;

pub const TABS: [&str; 9] = [
    "MONDE",
    "TEMPS",
    "VAISSEAU",
    "EQUIPAGE",
    "ECONOMIE",
    "SAVOIR",
    "COLONIES",
    "EXPLORATION",
    "DEBUG",
];

#[derive(Default)]
pub struct CxState {
    pub tab: usize,
    pub sel: usize,
    /// Body picked for the teleport row.
    pub arg: usize,
    pub note: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Enable,
    Flag(usize),
    Teleport,
    Reveal,
    Stop,
    Pause,
    Scale(f64),
    Days(f64),
    Credits(f64),
    Service,
    Cargo,
    Revive,
    Reputation(f32),
    AllTech,
    Found,
    Console,
}

pub fn label(it: Item) -> String {
    match it {
        Item::Enable => "MODE CREATIF".into(),
        Item::Flag(i) => CreativeFlags::name(i).to_uppercase(),
        Item::Teleport => "TELEPORTER AU CORPS".into(),
        Item::Reveal => "REVELER TOUTE LA CARTE".into(),
        Item::Stop => "ARRET COMPLET (VITESSE 0)".into(),
        Item::Pause => "PAUSE / REPRISE".into(),
        Item::Scale(v) => format!("VITESSE x{v:.0}"),
        Item::Days(d) => format!("AVANCER DE {d:.0} JOUR(S)"),
        Item::Credits(c) => format!("CREDITS {c:+.0}"),
        Item::Service => "REPARATION TOTALE + CARBURANT + SOINS".into(),
        Item::Cargo => "REMPLIR LA SOUTE (1000 KG DE TOUT)".into(),
        Item::Revive => "RESSUSCITER L'EQUIPAGE".into(),
        Item::Reputation(r) => format!("RELATIONS AVEC TOUS : {r:+.0}"),
        Item::AllTech => "TOUTES LES TECHNOLOGIES".into(),
        Item::Found => "FONDER UNE COLONIE SUR LA CIBLE".into(),
        Item::Console => "OUVRIR LA CONSOLE".into(),
    }
}

pub fn items(tab: usize) -> Vec<Item> {
    use Item::*;
    match tab {
        0 => vec![Teleport, Reveal, Stop],
        1 => vec![
            Pause,
            Scale(1.0),
            Scale(1000.0),
            Scale(1_000_000.0),
            Days(1.0),
            Days(30.0),
            Days(365.0),
        ],
        2 => vec![
            Flag(0),
            Flag(1),
            Flag(2),
            Flag(3),
            Flag(9),
            Service,
            Cargo,
            Credits(1e6),
        ],
        3 => vec![Flag(4), Flag(5), Revive, Service],
        4 => vec![
            Credits(1e6),
            Credits(1e9),
            Credits(-1e6),
            Reputation(100.0),
            Reputation(-100.0),
            Flag(6),
        ],
        5 => vec![AllTech, Flag(7), Flag(8)],
        6 => vec![Found, Flag(1), Flag(2)],
        7 => vec![Flag(10), Reveal, Teleport],
        _ => vec![Flag(0), Flag(11), Console],
    }
}

fn flag_on(g: &Game, i: usize) -> bool {
    g.sim.rules.cx.get(i)
}

pub fn value(g: &Game, it: Item) -> String {
    match it {
        Item::Enable => if g.sim.rules.creative { "OUI" } else { "NON" }.into(),
        Item::Flag(i) => if flag_on(g, i) { "OUI" } else { "NON" }.into(),
        Item::Teleport => g
            .sim
            .bodies
            .get(g.cx.arg)
            .map_or("?".into(), |b| format!("< {} >", b.name)),
        Item::Pause => if g.paused { "EN PAUSE" } else { "" }.into(),
        _ => String::new(),
    }
}

/// Run one item through the creative API; returns the message shown.
pub fn exec(g: &mut Game, it: Item) -> String {
    let r: Result<String, String> = match it {
        Item::Enable => {
            let on = !g.sim.rules.creative;
            g.sim.creative_enable(on);
            g.pending_rules = g.sim.rules.clone();
            Ok(if on {
                "CREATIF ACTIF : succes et classement desactives, partie marquee".to_string()
            } else {
                "creatif coupe".to_string()
            })
        }
        Item::Flag(i) => {
            let on = !flag_on(g, i);
            g.sim.creative_set_flag(i, on)
        }
        Item::Teleport => g.sim.creative_teleport_body(g.cx.arg),
        Item::Reveal => g.sim.creative_reveal_map(),
        Item::Stop => g.sim.creative_stop(),
        Item::Pause => {
            g.paused = !g.paused;
            Ok("pause".into())
        }
        Item::Scale(v) => g.sim.creative_check().map(|_| {
            g.timescale = v;
            format!("vitesse x{v:.0}")
        }),
        Item::Days(d) => g.sim.creative_advance_days(d),
        Item::Credits(c) => g.sim.creative_give_credits(c),
        Item::Service => g.sim.creative_full_service(),
        Item::Cargo => g.sim.creative_fill_cargo(1000.0),
        Item::Revive => g.sim.creative_revive_crew(),
        Item::Reputation(r) => g.sim.creative_set_all_reputation(r),
        Item::AllTech => g.sim.creative_unlock_all_tech(),
        Item::Found => match g.sim.target {
            Some(t) => g.sim.creative_found_colony(t),
            None => Err("aucune cible (T/Y pour en choisir une)".into()),
        },
        Item::Console => g.sim.creative_check().map(|_| {
            g.console.open = true;
            g.screen = g.cx_back;
            "console".to_string()
        }),
    };
    r.unwrap_or_else(|e| e)
}

impl Game {
    pub fn open_creative(&mut self) {
        if self.screen != Screen::Creative {
            self.cx_back = self.screen;
        }
        self.cx.sel = 0;
        self.screen = Screen::Creative;
    }

    fn cx_items(&self) -> Vec<Item> {
        if self.sim.rules.creative {
            items(self.cx.tab)
        } else {
            vec![Item::Enable]
        }
    }

    fn cx_run(&mut self, i: usize) {
        if let Some(&it) = self.cx_items().get(i) {
            self.cx.sel = i;
            let m = exec(self, it);
            self.say(m.clone());
            self.cx.note = m;
        }
    }

    /// Click: 0..99 picks a tab, 100+n runs row n, 200 / 201 steps the teleport body.
    pub fn click_cx(&mut self, code: usize) {
        match code {
            200 => self.cx_step(false),
            201 => self.cx_step(true),
            c if c >= 100 => self.cx_run(c - 100),
            c => {
                self.cx.tab = c.min(TABS.len() - 1);
                self.cx.sel = 0;
            }
        }
    }

    fn cx_step(&mut self, up: bool) {
        let n = self.sim.bodies.len().max(1);
        self.cx.arg = if up {
            (self.cx.arg + 1) % n
        } else {
            (self.cx.arg + n - 1) % n
        };
    }

    pub fn creative_key(&mut self, code: KeyCode) {
        let n = self.cx_items().len().max(1);
        match code {
            KeyCode::ArrowUp => self.cx.sel = (self.cx.sel + n - 1) % n,
            KeyCode::ArrowDown => self.cx.sel = (self.cx.sel + 1) % n,
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let up = code == KeyCode::ArrowRight;
                if self.cx_items().get(self.cx.sel) == Some(&Item::Teleport) {
                    self.cx_step(up);
                } else if self.sim.rules.creative {
                    let t = TABS.len();
                    self.cx.tab = if up {
                        (self.cx.tab + 1) % t
                    } else {
                        (self.cx.tab + t - 1) % t
                    };
                    self.cx.sel = 0;
                }
            }
            KeyCode::Tab => {
                self.cx.tab = (self.cx.tab + 1) % TABS.len();
                self.cx.sel = 0;
            }
            KeyCode::Enter => self.cx_run(self.cx.sel),
            KeyCode::Escape | KeyCode::Insert => self.screen = self.cx_back,
            _ => {}
        }
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 2, "== PANNEAU CREATIF ==", AMBER);
    if !g.sim.rules.creative {
        fb.text(4, 5, "LE MODE CREATIF EST DESACTIVE.", RED);
        fb.text(
            4,
            7,
            "L'ACTIVER DESACTIVE LES SUCCES ET LE CLASSEMENT ; LA PARTIE RESTE SAUVEGARDABLE (MARQUEE CREATIVE).",
            DIM,
        );
    } else {
        let mut col = 4;
        for (i, t) in TABS.iter().enumerate() {
            let on = i == g.cx.tab;
            let hov = g.hit_cells(col, 4, t.len(), 1, Act::Call(Game::click_cx, i), None);
            let c = if on {
                AMBER
            } else if hov {
                HOVER
            } else {
                GREEN
            };
            fb.text(col, 4, t, c);
            col += t.len() + 2;
        }
    }
    for (i, it) in g.cx_items().iter().enumerate() {
        let row = 10 + i * 2;
        let sel = i == g.cx.sel;
        let hov = g.hit_cells(4, row, 46, 1, Act::Call(Game::click_cx, 100 + i), None);
        let c = if sel {
            AMBER
        } else if hov {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            4,
            row,
            &format!("{} {}", if sel { ">" } else { " " }, label(*it)),
            c,
        );
        let v = value(g, *it);
        if *it == Item::Teleport {
            g.hit_cells(52, row, 2, 1, Act::Call(Game::click_cx, 200), None);
            g.hit_cells(54, row, v.len(), 1, Act::Call(Game::click_cx, 201), None);
        }
        fb.text(52, row, &v, c);
    }
    if !g.cx.note.is_empty() {
        fb.text(4, fb.rows() - 5, &g.cx.note.to_uppercase(), AMBER);
    }
    crate::ui::foot(
        fb,
        g,
        4,
        fb.rows() - 3,
        "HAUT/BAS CHOISIR  GAUCHE/DROITE ONGLET  ENTREE EXECUTER  ECHAP RETOUR",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::DEFAULT_SEED;

    fn game() -> Game {
        let data = std::sync::Arc::new(
            vector_data::GameData::load(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
            )
            .expect("data"),
        );
        let mut g = Game::new(data, 1);
        g.new_game(DEFAULT_SEED);
        g
    }

    #[test]
    fn panel_opens_enables_runs_and_draws_every_tab() {
        let mut g = game();
        g.key(KeyCode::Insert, None, true);
        assert_eq!(g.screen, Screen::Creative);
        let mut fb = Framebuffer::new(1280, 720);
        crate::draw_frame(&mut fb, &g);
        // Off: only the activation row, and actions are refused.
        assert_eq!(g.cx_items(), vec![Item::Enable]);
        assert!(exec(&mut g, Item::Credits(5.0)).contains("CREATIF"));
        exec(&mut g, Item::Enable);
        assert!(g.sim.rules.creative);
        let c0 = g.sim.credits;
        exec(&mut g, Item::Credits(1e6));
        assert_eq!(g.sim.credits, c0 + 1e6);
        for tab in 0..TABS.len() {
            g.cx.tab = tab;
            for it in items(tab) {
                assert!(!label(it).is_empty());
            }
            crate::draw_frame(&mut fb, &g);
        }
        g.key(KeyCode::Insert, None, true);
        assert_ne!(g.screen, Screen::Creative);
    }

    #[test]
    fn creative_scenario_starts_creative() {
        let g = game();
        let sc = g.data.scenario_index("creatif").expect("scenario");
        assert!(g.data.scenarios[sc].creative);
        let cfg = vector_sim::start::StartConfig {
            scenario: sc,
            ..vector_sim::start::StartConfig::new(DEFAULT_SEED)
        };
        let sim =
            vector_sim::start::new_sim_with(&cfg, std::sync::Arc::clone(&g.data)).expect("start");
        assert!(sim.rules.creative);
    }
}
