//! The new-game flow: seven plain screens (what the game is, the scenario, the ship, the crew,
//! the galaxy, the options, the summary), reached from the main menu. Everything chosen here
//! builds the real starting `Sim` (`vector_sim::start`): nothing is cosmetic.
use crate::game::{parse_seed, Game, Screen, DEFAULT_SEED};
use crate::hub::{button, wrap, HOVER};
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use std::cell::RefCell;
use std::path::PathBuf;
use vector_render::Framebuffer;
use vector_sim::crew::Person;
use vector_sim::shipwright::{Blueprint, Report};
use vector_sim::start::{self, CrewPick, HomePreview, StartConfig};
use vector_sim::start_custom::{CustomStart, Review};
use winit::keyboard::KeyCode;

pub const STEPS: [&str; 9] = [
    "EXPLICATIONS",
    "SCENARIO",
    "VAISSEAU",
    "EQUIPAGE",
    "IA",
    "GALAXIE",
    "MONDE",
    "OPTIONS",
    "RECAPITULATIF",
];
const LAST: usize = STEPS.len() - 1;
use crate::newgame_custom::{S_AI, S_CREW, S_OPTIONS, S_RECAP, S_SEED, S_SHIP, S_WORLD};

/// The last analysed plan of the design mode, by its text.
pub type PlanCache = Option<(String, Result<(Blueprint, Report), String>)>;

#[derive(Default)]
pub struct NewGameUi {
    pub step: usize,
    pub scenario: usize,
    pub ship: usize,
    /// The people the seed offers, who is picked (with a post each) and the cursor.
    pub pool: Vec<Person>,
    pub picks: Vec<CrewPick>,
    pub custom_crew: bool,
    pub cursor: usize,
    pub seed_text: String,
    /// What the typed seed gives at home, for the seed it was computed for.
    pub home: Option<(u64, HomePreview)>,
    /// Seeds drawn by the random button (so that two draws differ).
    pub draws: u64,
    /// Everything chosen beyond the scenario (see `vector_sim::start_custom`).
    pub custom: CustomStart,
    /// The selected row of each step, and where the keys go on the crew step.
    pub row: [usize; 9],
    pub crew_focus: bool,
    pub crew_tab: u8,
    pub trait_pick: usize,
    pub note: String,
    pub plan_files: Vec<String>,
    pub plan_pick: usize,
    pub preset_idx: Option<usize>,
    pub preset_name: String,
    pub preset_pick: usize,
    pub designing: bool,
    /// (seed, salt) the pool was drawn for.
    pub pool_key: Option<(u64, u32)>,
    pub world_key: Option<(u64, usize)>,
    pub world_preview: Option<(HomePreview, bool)>,
    pub plan_cache: RefCell<PlanCache>,
    pub review: RefCell<Option<(String, Review)>>,
}

impl NewGameUi {
    pub fn seed(&self) -> u64 {
        parse_seed(&self.seed_text)
    }
}

impl Game {
    /// Open the flow on its first screen (the seed typed in the main menu is kept).
    pub fn open_new_game(&mut self) {
        self.ng = NewGameUi {
            seed_text: self.seed_text.clone(),
            ..Default::default()
        };
        self.ng_refresh();
        self.screen = Screen::NewGame;
    }

    /// The seed, scenario or ship changed: recompute what depends on them.
    pub(crate) fn ng_refresh(&mut self) {
        let seed = self.ng.seed();
        let key = (seed, self.ng.custom.crew.salt);
        if self.ng.pool.is_empty() || self.ng.pool_key != Some(key) {
            self.ng.pool = start::crew_pool_for(&self.data, seed, key.1);
            self.ng.pool_key = Some(key);
            self.ng.custom_crew = false;
            self.ng.custom.crew.edits.clear();
            self.ng.custom.crew.locked.clear();
        }
        if self.ng.home.as_ref().map(|h| h.0) != Some(seed) {
            self.ng.home = Some((seed, start::home_preview(&self.data, seed)));
        }
        let sc = self.ng.scenario.min(self.data.scenarios.len() - 1);
        self.ng.scenario = sc;
        self.ng.ship = self.ng.ship.min(self.data.scenarios[sc].ships.len() - 1);
        if !self.ng.custom_crew {
            self.ng.picks =
                start::default_crew(&self.data, &self.data.scenarios[sc], &self.ng.pool);
        }
        self.ng.cursor = self.ng.cursor.min(self.ng.pool.len().saturating_sub(1));
    }

    /// The configuration the flow has composed so far.
    pub fn ng_config(&self) -> StartConfig {
        StartConfig {
            seed: self.ng.seed(),
            scenario: self.ng.scenario,
            ship: self.ng.ship,
            crew: Some(self.ng.picks.clone()),
            rules: self.pending_rules.clone(),
            custom: self.ng.custom.clone(),
        }
    }

    pub fn ng_goto(&mut self, step: usize) {
        self.ng.step = step.min(LAST);
        self.ng.note.clear();
        if self.ng.step == S_WORLD {
            self.ng_world_refresh();
        }
    }

    fn ng_next(&mut self) {
        if self.ng.step >= LAST {
            let cfg = self.ng_config();
            self.start_game(&cfg);
        } else {
            self.ng_goto(self.ng.step + 1);
        }
    }

    fn ng_prev(&mut self) {
        if self.ng.step == 0 {
            self.screen = Screen::MainMenu;
        } else {
            self.ng_goto(self.ng.step - 1);
        }
    }

    pub fn click_ng_step(&mut self, i: usize) {
        self.ng_goto(i);
    }

    pub fn click_ng_scenario(&mut self, i: usize) {
        if self.ng.scenario != i {
            self.ng.scenario = i;
            self.ng.ship = 0;
            self.ng.custom_crew = false;
            self.ng_refresh();
        }
    }

    pub fn click_ng_candidate(&mut self, i: usize) {
        self.ng.cursor = i;
        self.ng.crew_focus = false;
        self.ng_toggle_pick(i);
    }

    pub fn click_ng_random(&mut self, _: usize) {
        self.ng_random_seed();
    }

    pub fn click_ng_default_seed(&mut self, _: usize) {
        self.ng.seed_text.clear();
        self.ng_refresh();
    }

    pub fn click_ng_random_all(&mut self, _: usize) {
        self.ng_random_all();
    }

    fn ng_random_seed(&mut self) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        self.ng.draws += 1;
        let n = vector_core::mix64(nanos ^ self.ng.draws.wrapping_mul(0x9E37_79B9)) % 1_000_000_000;
        self.ng.seed_text = n.to_string();
        self.ng_refresh();
    }

    pub fn ng_points_used(&self) -> u32 {
        self.ng
            .picks
            .iter()
            .map(|p| start::pick_cost(&self.ng.pool[p.candidate]))
            .sum()
    }

    pub(crate) fn ng_toggle_pick(&mut self, k: usize) {
        if k >= self.ng.pool.len() {
            return;
        }
        if let Some(at) = self.ng.picks.iter().position(|p| p.candidate == k) {
            if self.ng.picks.len() == 1 {
                self.say("IL FAUT AU MOINS UNE PERSONNE A BORD");
                return;
            }
            self.ng.picks.remove(at);
        } else {
            let cost = start::pick_cost(&self.ng.pool[k]);
            let budget = self.data.scenarios[self.ng.scenario].crew_points;
            if self.ng.picks.len() >= start::MAX_START_CREW {
                self.say("EQUIPAGE DE DEPART AU MAXIMUM");
                return;
            }
            if self.ng_points_used() + cost > budget {
                self.say("BUDGET DE POINTS DEPASSE : RETIREZ QUELQU'UN D'ABORD");
                return;
            }
            let role = start::natural_role(&self.data, &self.ng.pool[k]);
            self.ng.picks.push(CrewPick { candidate: k, role });
        }
        self.ng.custom_crew = true;
    }

    fn ng_role_change(&mut self, up: bool) {
        let k = self.ng.cursor;
        let n = self.data.roles.len();
        if let Some(p) = self.ng.picks.iter_mut().find(|p| p.candidate == k) {
            p.role = if up {
                (p.role + 1) % n
            } else {
                (p.role + n - 1) % n
            };
            self.ng.custom_crew = true;
        }
    }

    pub(crate) fn save_settings(&self) {
        let path = if cfg!(test) {
            std::env::temp_dir().join("vector_test_settings.cfg")
        } else {
            PathBuf::from("saves").join("settings.cfg")
        };
        self.settings.save(&path);
    }

    pub fn newgame_key(&mut self, code: KeyCode, text: Option<&str>) {
        let step = self.ng.step;
        let (up, down) = (code == KeyCode::ArrowUp, code == KeyCode::ArrowDown);
        match code {
            KeyCode::Escape => return self.ng_prev(),
            KeyCode::Enter => {
                // On an action row, Enter does the action; elsewhere it goes on.
                let row = self.ng.row[step];
                let acts = matches!(
                    self.ng_kind_selected(),
                    Some(crate::newgame_custom::Kind::Action)
                ) && step != S_RECAP
                    && (step != S_CREW || self.ng.crew_focus);
                if acts {
                    self.click_ng_field(row * 4 + 1);
                } else {
                    self.ng_next();
                }
                return;
            }
            _ => {}
        }
        let row_step = matches!(step, S_SHIP | S_AI | S_WORLD | S_OPTIONS | S_RECAP)
            || (step == S_CREW && self.ng.crew_focus);
        if step == S_CREW && code == KeyCode::F3 {
            self.ng.crew_focus = !self.ng.crew_focus;
            return;
        }
        if row_step {
            let n = self.ng_rows().len().max(1);
            let r = self.ng.row[step].min(n - 1);
            match code {
                KeyCode::Tab => self.click_ng_field(r * 4 + 3),
                KeyCode::ArrowUp => self.ng.row[step] = (r + n - 1) % n,
                KeyCode::ArrowDown => self.ng.row[step] = (r + 1) % n,
                KeyCode::ArrowLeft => self.click_ng_field(r * 4),
                KeyCode::ArrowRight => self.click_ng_field(r * 4 + 2),
                _ => {
                    if !self.ng_type(code, text) && code == KeyCode::Space {
                        self.click_ng_field(r * 4 + 1);
                    }
                }
            }
            return;
        }
        match step {
            1 => {
                let n = self.data.scenarios.len();
                if up {
                    self.click_ng_scenario((self.ng.scenario + n - 1) % n);
                } else if down {
                    self.click_ng_scenario((self.ng.scenario + 1) % n);
                }
            }
            S_CREW => {
                let n = self.ng.pool.len().max(1);
                match code {
                    KeyCode::ArrowUp => self.ng.cursor = (self.ng.cursor + n - 1) % n,
                    KeyCode::ArrowDown => self.ng.cursor = (self.ng.cursor + 1) % n,
                    KeyCode::Space => self.ng_toggle_pick(self.ng.cursor),
                    KeyCode::ArrowLeft => self.ng_role_change(false),
                    KeyCode::ArrowRight => self.ng_role_change(true),
                    _ => {}
                }
                return;
            }
            S_SEED => {
                match code {
                    KeyCode::Backspace => {
                        self.ng.seed_text.pop();
                    }
                    KeyCode::Tab => self.ng_random_seed(),
                    KeyCode::Delete => self.ng.seed_text.clear(),
                    _ => {
                        if let Some(t) = text {
                            if self.ng.seed_text.len() < 20 {
                                self.ng
                                    .seed_text
                                    .extend(t.chars().filter(|c| c.is_alphanumeric()));
                            }
                        }
                    }
                }
                self.ng_refresh();
                return;
            }
            _ => {}
        }
        // Left and Right walk the steps where they have no other use.
        match code {
            KeyCode::ArrowRight => self.ng_next(),
            KeyCode::ArrowLeft => self.ng_prev(),
            _ => {}
        }
    }
}

// ---- drawing ---------------------------------------------------------------------------

fn kg_list(g: &Game, items: &[(String, f64)]) -> String {
    items
        .iter()
        .map(|(r, kg)| {
            let name = g
                .data
                .resource_index(r)
                .map_or(r.clone(), |i| g.data.resources[i].name.clone());
            format!("{kg:.0} KG {name}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn lines(fb: &mut Framebuffer, col: usize, mut row: usize, w: usize, text: &str, c: u32) -> usize {
    for l in wrap(text, w) {
        fb.text(col, row, &l, c);
        row += 1;
    }
    row
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    let step = g.ng.step;
    fb.text(2, 1, "== NOUVELLE PARTIE ==", AMBER);
    button(
        fb,
        g,
        cols.saturating_sub(34),
        1,
        "TOUT ALEATOIRE (CONFIG ENTIERE)",
        Act::Call(Game::click_ng_random_all, 0),
    );
    // The steps, each a button.
    let mut x = 2;
    for (i, s) in STEPS.iter().enumerate() {
        let label = format!("{} {}", i + 1, s);
        let hover = g.hit_cells(
            x,
            3,
            label.chars().count(),
            1,
            Act::Call(Game::click_ng_step, i),
            None,
        );
        let c = if i == step {
            AMBER
        } else if hover {
            HOVER
        } else {
            DIM
        };
        fb.text(x, 3, &label, c);
        x += label.chars().count() + 3;
    }
    match step {
        0 => draw_explain(fb, g),
        1 => draw_scenario(fb, g),
        S_SHIP => draw_ship(fb, g),
        S_CREW => draw_crew(fb, g),
        S_AI => crate::newgame_custom::draw_ai(fb, g),
        S_SEED => draw_seed(fb, g),
        S_WORLD => crate::newgame_custom::draw_world(fb, g),
        S_OPTIONS => draw_options(fb, g),
        _ => draw_recap(fb, g),
    }
    // Footer: back and forward, plain text.
    let row = fb.rows().saturating_sub(4);
    let back = if step == 0 {
        "< MENU PRINCIPAL (ECHAP)"
    } else {
        "< PRECEDENT (ECHAP)"
    };
    button(fb, g, 2, row, back, Act::Key(KeyCode::Escape));
    let next = if step == LAST {
        "LANCER LA PARTIE (ENTREE) >"
    } else {
        "SUIVANT (ENTREE) >"
    };
    let col = cols.saturating_sub(next.chars().count() + 3);
    let hover = g.hit_cells(
        col,
        row,
        next.chars().count(),
        1,
        Act::Key(KeyCode::Enter),
        None,
    );
    fb.text(col, row, next, if hover { HOVER } else { AMBER });
    if let Some(m) = g.messages.last().filter(|m| {
        m.starts_with("BUDGET")
            || m.starts_with("IL FAUT")
            || m.starts_with("EQUIPAGE")
            || m.starts_with("ECHEC")
    }) {
        fb.text(2, row.saturating_sub(2), m, RED);
    }
}

fn draw_explain(fb: &mut Framebuffer, g: &Game) {
    let w = 110;
    let mut row = 6;
    fb.text(2, row, "CE QU'EST VECTOR", AMBER);
    row += 2;
    for (i, t) in [
        "VECTOR EST UN BAC A SABLE SPATIAL : AUCUNE HISTOIRE NI AUCUN OBJECTIF NE VOUS EST IMPOSE. RIEN N'EST OBLIGATOIRE.",
        "VOUS ETES L'IA DU VAISSEAU. VOUS PILOTEZ, VOUS CHOISISSEZ LES ESCALES, LES ECHANGES, LES RECHERCHES ; L'EQUIPAGE, LUI, VIT SA VIE A BORD : IL MANGE, DORT, S'ATTACHE, SE FACHE, FAIT DES ENFANTS.",
        "LA GALAXIE EST GENEREE A PARTIR D'UNE GRAINE ET LE MONDE CONTINUE SANS VOUS : PRIX, FACTIONS, COLONIES, EVENEMENTS.",
        "ON PEUT MINER, COMMERCER, EXPLORER LES ASTRES ET LES RUINES A PIED AVEC L'EQUIPE, FONDER ET GOUVERNER DES COLONIES, ETUDIER DES TECHNOLOGIES, OU SIMPLEMENT VOYAGER.",
    ]
    .iter()
    .enumerate()
    {
        let _ = i;
        row = lines(fb, 4, row, w, t, GREEN) + 1;
    }
    row += 1;
    fb.text(2, row, "LES TOUCHES A CONNAITRE", AMBER);
    row += 2;
    let azerty = g.settings.azerty;
    let keys = [
        (
            if azerty { "ZQSD / A E" } else { "WASD / Q E" },
            "TANGAGE, LACET, ROULIS",
        ),
        ("ESPACE / X", "POUSSEE AVANT / RECUL"),
        ("TAB", "CIBLER L'OBJET LE PLUS PROCHE"),
        ("M  L", "CARTE DU SYSTEME  GALAXIE"),
        (
            "F10",
            "LE MENU : HUIT ESPACES QUI REGROUPENT TOUS LES ECRANS",
        ),
        ("ECHAP", "REMONTER D'UN NIVEAU PARTOUT (CLIC DROIT AUSSI)"),
        ("F1", "AIDE DE L'ECRAN COURANT"),
        (
            "SOURIS",
            "TOUT SE FAIT AUSSI A LA SOURIS ; LES TEXTES SOULIGNES DE VERT SONT DES BOUTONS",
        ),
    ];
    for (k, d) in keys {
        fb.text(4, row, k, AMBER);
        fb.text(20, row, d, GREEN);
        row += 1;
    }
    row += 1;
    lines(
        fb,
        2,
        row,
        w,
        "LES ETAPES SUIVANTES : SCENARIO DE DEPART, VAISSEAU, EQUIPAGE, GALAXIE, OPTIONS. CHAQUE CHOIX CHANGE REELLEMENT L'ETAT DE DEPART.",
        DIM,
    );
}

fn draw_scenario(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    fb.text(2, 6, "CHOISISSEZ COMMENT COMMENCER", AMBER);
    for (i, sc) in g.data.scenarios.iter().enumerate() {
        let row = 9 + i * 2;
        let sel = i == g.ng.scenario;
        let hover = g.hit_cells(2, row, 34, 1, Act::Call(Game::click_ng_scenario, i), None);
        let c = if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            2,
            row,
            &format!("{} {}", if sel { ">" } else { " " }, sc.name.to_uppercase()),
            c,
        );
        fb.text(26, row, &format!("{}/5", sc.difficulty), DIM);
    }
    let sc = &g.data.scenarios[g.ng.scenario];
    let x = 42;
    let w = cols - x - 3;
    fb.text(x, 6, &sc.name.to_uppercase(), AMBER);
    let mut row = lines(fb, x, 8, w, &sc.text, GREEN) + 1;
    let hull = &g.data.hulls[g.data.hull_index(&sc.ships[0].0).unwrap_or(0)];
    let rating = start::difficulty_rating(sc, &g.pending_rules);
    let info = [
        format!("DIFFICULTE INDICATIVE : {rating}/5 (SELON LE SCENARIO ET LES REGLES)"),
        format!(
            "DEPART : {}",
            if sc.wreck {
                "AU SOL, A PIED, SUR UN VAISSEAU HORS D'ETAT DE VOLER"
            } else {
                "EN VOL, A COTE D'UNE STATION"
            }
        ),
        format!(
            "VAISSEAU : {} (+ {} AUTRE(S) CHOIX)",
            hull.name.to_uppercase(),
            sc.ships.len() - 1
        ),
        format!("CREDITS : {:.0}", sc.credits),
        format!("SOUTE : {}", kg_list(g, &sc.cargo)),
        format!(
            "EQUIPAGE : {} ({} POINTS A DEPENSER)",
            sc.crew
                .iter()
                .map(|r| g
                    .data
                    .roles
                    .iter()
                    .find(|ro| ro.id == *r)
                    .map_or(r.clone(), |ro| ro.name.to_uppercase()))
                .collect::<Vec<_>>()
                .join(", "),
            sc.crew_points
        ),
    ];
    for l in info {
        row = lines(fb, x, row, w, &l, DIM);
    }
    if !sc.techs.is_empty() {
        let names: Vec<String> = sc
            .techs
            .iter()
            .filter_map(|t| {
                g.data
                    .tech_index(t)
                    .map(|i| g.data.techs[i].name.to_uppercase())
            })
            .collect();
        row = lines(
            fb,
            x,
            row,
            w,
            &format!("TECHNOLOGIES CONNUES : {}", names.join(", ")),
            DIM,
        );
    }
    for (f, v) in &sc.reputation {
        let name = if f == "home" {
            "LE TERRITOIRE DE DEPART".to_string()
        } else {
            g.data
                .faction_index(f)
                .map_or(f.clone(), |i| g.data.factions[i].name.to_uppercase())
        };
        row = lines(
            fb,
            x,
            row,
            w,
            &format!("RELATION AVEC {name} : {v:+.0}"),
            DIM,
        );
    }
    if sc.wanted_days > 0.0 {
        lines(
            fb,
            x,
            row,
            w,
            &format!(
                "RECHERCHE PAR LE TERRITOIRE DE DEPART PENDANT {:.0} JOURS",
                sc.wanted_days
            ),
            RED,
        );
    }
}

fn draw_ship(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 6, "CONSTRUISEZ VOTRE VAISSEAU", AMBER);
    let rows = g.ng_rows();
    crate::newgame_custom::draw_rows(fb, g, &rows, 2, 8, true);
    crate::newgame_custom::draw_ship_panel(fb, g, 84);
}

fn draw_crew(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    let sc = &g.data.scenarios[g.ng.scenario];
    let used = g.ng_points_used();
    fb.text(2, 6, "COMPOSEZ L'EQUIPAGE DE DEPART", AMBER);
    let c = if used > sc.crew_points { RED } else { GREEN };
    fb.text(
        2,
        7,
        &format!(
            "POINTS : {used} / {}   (1 PAR PERSONNE, 2 SI SPECIALISTE, 3 SI EXPERT)",
            sc.crew_points
        ),
        c,
    );
    fb.text(
        2,
        8,
        "ESPACE OU CLIC : PRENDRE / LAISSER   GAUCHE/DROITE : CHANGER SON POSTE",
        DIM,
    );
    fb.text(
        2,
        10,
        "     NOM                        AGE  POSTE            MEILLEURE COMPETENCE        COUT",
        DIM,
    );
    for (k, p) in g.ng.pool.iter().enumerate() {
        let row = 11 + k;
        let pick = g.ng.picks.iter().find(|q| q.candidate == k);
        let hover = g.hit_cells(2, row, 96, 1, Act::Call(Game::click_ng_candidate, k), None);
        let cur = k == g.ng.cursor;
        let (bs, lvl) = p.best_skill();
        let role = pick.map_or("-".to_string(), |q| {
            g.data.roles[q.role].name.to_uppercase()
        });
        let col = if cur {
            AMBER
        } else if hover {
            HOVER
        } else if pick.is_some() {
            GREEN
        } else {
            DIM
        };
        fb.text(
            2,
            row,
            &format!(
                "{}{} {:<26.26} {:>3.0}  {:<16.16} {:<18.18} {:>4.1}   {}",
                if cur { ">" } else { " " },
                if pick.is_some() { "[X]" } else { "[ ]" },
                p.name.to_uppercase(),
                p.age,
                role,
                g.data.skills[bs].name.to_uppercase(),
                lvl,
                start::pick_cost(p)
            ),
            col,
        );
    }
    // The person under the cursor.
    if let Some(p) = g.ng.pool.get(g.ng.cursor) {
        let x = 104.min(cols.saturating_sub(50));
        fb.text(x, 10, &p.name.to_uppercase(), AMBER);
        let mut sk: Vec<(usize, f32)> = p.skills.iter().copied().enumerate().collect();
        sk.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (r, (i, v)) in sk.iter().take(5).enumerate() {
            fb.text(
                x,
                12 + r,
                &format!(
                    "{:<16.16} {:>4.1}",
                    g.data.skills[*i].name.to_uppercase(),
                    v
                ),
                GREEN,
            );
        }
        let traits: Vec<String> = p
            .traits
            .iter()
            .map(|&t| g.data.traits[t].name.to_uppercase())
            .collect();
        let mut row = 18;
        fb.text(x, row, "TRAITS", DIM);
        row += 1;
        row = lines(
            fb,
            x,
            row,
            50,
            &if traits.is_empty() {
                "AUCUN TRAIT MARQUANT".to_string()
            } else {
                traits.join(", ")
            },
            GREEN,
        );
        let _ = row;
    }
    // The ship's room for them.
    if let Some(st) = start::ship_stats(&g.data, sc, g.ng.ship) {
        let n = g.ng.picks.len() as f64;
        let (txt, c) = if n > st.crew_cap {
            (
                format!(
                    "LOGEMENTS : {:.0} POUR {n:.0} PERSONNES - L'EQUIPAGE SERA A L'ETROIT",
                    st.crew_cap
                ),
                RED,
            )
        } else {
            (
                format!(
                    "LOGEMENTS DU VAISSEAU : {:.0} (OK POUR {n:.0})",
                    st.crew_cap
                ),
                GREEN,
            )
        };
        fb.text(2, 11 + g.ng.pool.len() + 1, &txt, c);
    }
    let rows = g.ng_rows();
    crate::newgame_custom::draw_rows(fb, g, &rows, 2, 11 + g.ng.pool.len() + 3, g.ng.crew_focus);
}

fn draw_seed(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 6, "LA GALAXIE", AMBER);
    lines(
        fb,
        2,
        8,
        110,
        "UNE GRAINE (UN NOMBRE OU UN MOT) DECIDE DE TOUT : ETOILES, MONDES, FACTIONS, PEUPLES. LA MEME GRAINE DONNE TOUJOURS LA MEME GALAXIE. TAPEZ-EN UNE, OU TIREZ-EN UNE AU HASARD.",
        GREEN,
    );
    let shown = if g.ng.seed_text.is_empty() {
        format!("(PAR DEFAUT : {DEFAULT_SEED})_")
    } else {
        format!("{}_", g.ng.seed_text)
    };
    fb.text(2, 12, "GRAINE :", DIM);
    fb.text(12, 12, &shown, AMBER);
    let mut x = 2;
    x += button(
        fb,
        g,
        x,
        14,
        "ALEATOIRE (TAB)",
        Act::Call(Game::click_ng_random, 0),
    ) + 3;
    button(
        fb,
        g,
        x,
        14,
        "PAR DEFAUT (SUPPR)",
        Act::Call(Game::click_ng_default_seed, 0),
    );
    if let Some((_, h)) = &g.ng.home {
        fb.text(2, 17, "CE QUE LA GRAINE DONNE AU DEPART", AMBER);
        let rows = [
            format!(
                "SYSTEME {}   ETOILE DE CLASSE {}",
                h.system.to_uppercase(),
                h.class
            ),
            format!(
                "{} PLANETE(S)  {} LUNE(S)  {} ASTEROIDE(S)  {} STATION(S)",
                h.planets, h.moons, h.asteroids, h.stations
            ),
            format!(
                "TERRITOIRE : {}",
                h.faction
                    .as_ref()
                    .map_or("AUCUN (ESPACE LIBRE)".to_string(), |f| f.to_uppercase())
            ),
        ];
        for (k, l) in rows.iter().enumerate() {
            fb.text(4, 19 + k, l, GREEN);
        }
    }
    fb.text(
        2,
        24,
        "LES CANDIDATS DE L'ETAPE EQUIPAGE DEPENDENT AUSSI DE LA GRAINE.",
        DIM,
    );
}

fn draw_options(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 6, "OPTIONS ET REGLES PAR CATEGORIES", AMBER);
    let rows = g.ng_rows();
    crate::newgame_custom::draw_rows(fb, g, &rows, 2, 8, true);
    let rating = g.ng_review().rating;
    fb.text(
        2,
        8 + rows.len() + 1,
        &format!("DIFFICULTE INDICATIVE DE CE DEPART : {rating}/5"),
        AMBER,
    );
}

fn draw_recap(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    let w = cols - 8;
    let d = &g.data;
    let sc = &d.scenarios[g.ng.scenario];
    let c = &g.ng.custom;
    fb.text(2, 6, "RECAPITULATIF", AMBER);
    let ship = match c.ship.mode {
        vector_sim::start_custom::ShipMode::Scenario => d.hulls
            [d.hull_index(&sc.ships[g.ng.ship].0).unwrap_or(0)]
        .name
        .to_uppercase(),
        vector_sim::start_custom::ShipMode::Catalog => format!(
            "{} SUR MESURE (CATALOGUE, {} MODULES)",
            d.hulls[d.hull_index(&c.ship.hull).unwrap_or(0)]
                .name
                .to_uppercase(),
            c.ship.modules.iter().filter(|m| !m.is_empty()).count()
        ),
        vector_sim::start_custom::ShipMode::Design => {
            match vector_sim::start_custom::plan_report(d, &c.ship.plan) {
                Ok((bp, r)) => format!(
                    "CONCEPTION {} ({} CELLULES)",
                    bp.name.to_uppercase(),
                    r.cells
                ),
                Err(_) => "CONCEPTION (PLAN INVALIDE)".to_string(),
            }
        }
    };
    let crew: Vec<String> =
        g.ng.picks
            .iter()
            .map(|p| {
                let e = c.crew.edit_of(p.candidate);
                let name = e
                    .and_then(|e| e.name.clone())
                    .unwrap_or_else(|| g.ng.pool[p.candidate].name.clone());
                format!(
                    "{}{} ({})",
                    name.to_uppercase(),
                    if e.is_some() { "*" } else { "" },
                    d.roles[p.role].name.to_uppercase()
                )
            })
            .collect();
    let home = g.ng.home.as_ref().map(|h| h.1.clone());
    let lvl = start::difficulty_of(&g.pending_rules);
    let tr = &d.start.ai_traits[c.ai.personality.min(d.start.ai_traits.len() - 1)];
    let place = c.world.place.map_or("PAR DEFAUT".to_string(), |i| {
        d.start.places[i].name.to_uppercase()
    });
    let contacts: Vec<String> = c
        .crew
        .contacts
        .iter()
        .map(|k| {
            format!(
                "{} {}",
                if k.friend { "AMI" } else { "RIVAL" },
                d.factions
                    .get(k.faction)
                    .map_or(String::new(), |f| f.name.to_uppercase())
            )
        })
        .collect();
    let items = [
        (
            "SCENARIO",
            format!(
                "{} - DIFFICULTE {}/5",
                sc.name.to_uppercase(),
                g.ng_review().rating
            ),
        ),
        ("VAISSEAU", ship),
        (
            "IDENTITE",
            format!(
                "{} {} - {}",
                if c.ship.name.is_empty() {
                    "(NOM TIRE)"
                } else {
                    &c.ship.name
                },
                if c.ship.registration.is_empty() {
                    ""
                } else {
                    &c.ship.registration
                },
                if c.ship.company.is_empty() {
                    "(COMPAGNIE TIREE)"
                } else {
                    &c.ship.company
                }
            )
            .to_uppercase(),
        ),
        ("CREDITS", format!("{:.0}", sc.credits)),
        ("SOUTE", kg_list(g, &sc.cargo)),
        ("EQUIPAGE", crew.join(", ")),
        (
            "CONTACTS",
            if contacts.is_empty() {
                "AUCUN".to_string()
            } else {
                contacts.join(", ")
            },
        ),
        (
            "IA",
            format!(
                "{} - {} - NOYAU {}",
                if c.ai.name.is_empty() {
                    "(NOM TIRE)".to_string()
                } else {
                    c.ai.name.to_uppercase()
                },
                tr.name.to_uppercase(),
                d.start.ai_cores[c.ai.core.min(d.start.ai_cores.len() - 1)]
                    .name
                    .to_uppercase()
            ),
        ),
        (
            "GALAXIE",
            format!(
                "GRAINE {} - SYSTEME {} (CLASSE {}) - DEPART : {place}",
                g.ng.seed(),
                home.as_ref()
                    .map_or(String::new(), |h| h.system.to_uppercase()),
                home.as_ref().map_or("?", |h| h.class)
            ),
        ),
        (
            "CLAVIER",
            if g.settings.azerty {
                "AZERTY"
            } else {
                "QWERTY"
            }
            .to_string(),
        ),
        (
            "REGLES",
            format!(
                "{} - SAUVEGARDE : {}",
                lvl.map_or("PERSONNALISEES".to_string(), |l| start::DIFFICULTY_NAMES[l]
                    .to_string()),
                vector_sim::start_custom::SAVE_MODES[usize::from(c.save_mode.min(2))]
            ),
        ),
    ];
    let mut row = 8;
    for (k, v) in items {
        fb.text(4, row, k, DIM);
        row = lines(fb, 16, row, w - 16, &v, GREEN).max(row + 1);
    }
    row += 1;
    crate::newgame_custom::draw_recap_extra(fb, g, row);
    let wreck = start::is_wreck(d, &g.ng_config());
    let y = row + 7;
    let msg = if wreck {
        "VOUS COMMENCEREZ A PIED, SUR UN MONDE NU : LE VAISSEAU EST EPAVE."
    } else {
        "VOUS COMMENCEREZ EN VOL OU AU LIEU CHOISI. F10 OUVRE LE MENU."
    };
    fb.text(4, y, msg, AMBER);
    let rows = g.ng_rows();
    crate::newgame_custom::draw_rows(fb, g, &rows, 2, y + 2, true);
}
