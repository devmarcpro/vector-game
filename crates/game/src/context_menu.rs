//! The one context menu of the game. A right click on any element (a row of a list, a body of the
//! cockpit, a cell of the walk, a word, a gauge...) opens a small list of the ACTIONS POSSIBLE in
//! that context: the key that does the same is shown dimmed, an action that cannot be done now is
//! greyed with its reason. A left click chooses, a click elsewhere or Escape closes; the arrows and
//! Enter work too. A right click on an empty zone goes back, as before.
//!
//! One action = one identifier: an item is a list of `Act`, the very same acts the zones and the
//! command bar run (a key press or a function with a number), so the menu never has a second
//! implementation of anything. The context is found from the zone under the pointer (`ctx_of`):
//! the screens declare nothing more than they already do for the left click.
//!
//! Also here: the tooltips of the zones (after a short rest of the pointer), the "what can I do
//! here" suggestions of the menu and of the dashboard, and the table of the screens' help.
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use std::cell::Cell;
use vector_render::Framebuffer;
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

const HOVER: u32 = 0x00D0_FFD0;
const BACK: u32 = 0x0008_1018;
/// Frames the pointer must rest on a zone before its tooltip shows.
pub const TIP_FRAMES: u32 = 24;

/// One line of the menu.
pub struct Item {
    pub label: String,
    /// The key that does the same, shown dimmed.
    pub key: String,
    /// What it runs, in order (select the row, then the action's own key...).
    pub run: Vec<Act>,
    /// Why it cannot be done now (the line is greyed and says it).
    pub off: Option<String>,
}

impl Item {
    pub fn new(label: impl Into<String>, key: &str, run: Vec<Act>) -> Item {
        Item {
            label: label.into(),
            key: key.into(),
            run,
            off: None,
        }
    }

    pub fn off_if(mut self, cond: bool, why: &str) -> Item {
        if cond && self.off.is_none() {
            self.off = Some(why.into());
        }
        self
    }
}

/// The menu being shown.
pub struct Menu {
    /// Where it was opened (pixels).
    pub at: (i32, i32),
    pub title: String,
    pub items: Vec<Item>,
    pub sel: usize,
    /// Where it was drawn last frame: column, row, width, height in cells.
    pub rect: Cell<(usize, usize, usize, usize)>,
}

/// What a zone stands for.
#[derive(Clone, Copy)]
pub enum Ctx {
    Body(usize),
    Npc(usize),
    Star(usize),
    Crew(usize),
    Contract(usize),
    Market(usize),
    BourseRes(usize),
    Fleet(usize),
    Policy(usize),
    Storage(usize),
    BuildOpt(usize),
    Empire(usize),
    Tech(usize),
    Yard(usize),
    Recruit(usize),
    Slot(usize),
    Faction(usize),
    HubSpace(usize),
    HubEntry(usize),
    DashPanel(usize),
    DashLink(usize),
    /// A cell of the walk (tile x, tile y).
    Cell(i32, i32),
    /// A word or a button that presses a key.
    Key(KeyCode),
    /// Anything else that can be clicked.
    Other(Act),
}

/// The function a zone runs on a left click, with the number of its row.
type Click = fn(&mut Game, usize);

/// Builds the context of a row.
type Make = fn(usize) -> Ctx;

fn is(f: Click, g: Click) -> bool {
    std::ptr::fn_addr_eq(f, g)
}

/// The context of the zone whose left-click action is `act`; `pos` is where the pointer is.
pub fn ctx_of(g: &Game, act: Act, pos: (i32, i32)) -> Ctx {
    match act {
        Act::Call(f, n) => {
            let table: [(Click, Make); 20] = [
                (Game::click_body, Ctx::Body),
                (Game::dbl_body, Ctx::Body),
                (Game::click_npc, Ctx::Npc),
                (Game::click_star, Ctx::Star),
                (Game::click_crew, Ctx::Crew),
                (Game::click_contract, Ctx::Contract),
                (Game::click_sel_res, Ctx::Market),
                (Game::click_bourse_res, Ctx::BourseRes),
                (Game::click_fleet_ship, Ctx::Fleet),
                (Game::click_policy, Ctx::Policy),
                (Game::click_storage, Ctx::Storage),
                (Game::click_build, Ctx::BuildOpt),
                (Game::click_empire, Ctx::Empire),
                (Game::click_tech, Ctx::Tech),
                (Game::click_yard, Ctx::Yard),
                (Game::click_recruit, Ctx::Recruit),
                (Game::click_ship_slot, Ctx::Slot),
                (Game::click_faction, Ctx::Faction),
                (Game::click_hub_entry, Ctx::HubEntry),
                (Game::click_dash_panel, Ctx::DashPanel),
            ];
            for (h, make) in table {
                if is(f, h) {
                    return make(n);
                }
            }
            if is(f, Game::click_hub_space) {
                return Ctx::HubSpace(n);
            }
            if is(f, Game::click_dash_link) {
                return Ctx::DashLink(n);
            }
            Ctx::Other(act)
        }
        Act::At(f) if std::ptr::fn_addr_eq(f, Game::rogue_click as fn(&mut Game, (i32, i32))) => {
            match g.rogue_cell_at(pos.0, pos.1) {
                Some((x, y)) => Ctx::Cell(x, y),
                None => Ctx::Other(act),
            }
        }
        Act::Key(k) => Ctx::Key(k),
        _ => Ctx::Other(act),
    }
}

// ---- the tail every menu ends with ----------------------------------------------------

fn tail(g: &Game, items: &mut Vec<Item>) {
    items.push(Item::new(
        if g.help_open {
            "FERMER L'AIDE"
        } else {
            "AIDE DE CET ECRAN"
        },
        "F1",
        vec![Act::Key(KeyCode::F1)],
    ));
    let free = !matches!(
        g.screen,
        Screen::MainMenu | Screen::NewGame | Screen::Destroyed | Screen::Hub
    );
    if free {
        items.push(Item::new(
            "MENU : TOUS LES ECRANS",
            "F10",
            vec![Act::Key(KeyCode::F10)],
        ));
    }
    let root = matches!(
        g.screen,
        Screen::Flight | Screen::MainMenu | Screen::Destroyed
    );
    if !root {
        items.push(Item::new(
            "RETOUR",
            "ECHAP",
            vec![Act::Key(KeyCode::Escape)],
        ));
    }
}

fn k(code: KeyCode) -> Act {
    Act::Key(code)
}

fn call(f: fn(&mut Game, usize), n: usize) -> Act {
    Act::Call(f, n)
}

/// Rows of the fleet / crew... named for the title.
fn up(s: &str) -> String {
    s.to_uppercase()
}

/// The title and the items of the menu of a context.
pub fn items_for(g: &Game, ctx: Ctx) -> (String, Vec<Item>) {
    let mut it: Vec<Item> = Vec::new();
    let s = &g.sim;
    let mut title = String::from("ACTIONS");
    match ctx {
        Ctx::Body(i) if i < s.bodies.len() => {
            let b = &s.bodies[i];
            title = up(&b.name);
            let station = matches!(b.kind, BodyKind::Station);
            let world = matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_));
            let flying = s.docked.is_none();
            let can_fly = s.readiness().can_fly();
            let go_off = if !flying {
                Some("DECOLLEZ D'ABORD")
            } else if !can_fly {
                Some("LE VAISSEAU NE PEUT PAS VOLER")
            } else {
                None
            };
            it.push(Item::new("CIBLER", "CLIC", vec![call(Game::click_body, i)]));
            let mut goto = Item::new(
                "ALLER VERS (PILOTE AUTO)",
                "G / 2 CLICS",
                vec![call(Game::dbl_body, i)],
            );
            goto.off = go_off.map(Into::into);
            it.push(goto);
            it.push(
                Item::new(
                    "AMARRER",
                    "D",
                    vec![call(Game::click_body, i), k(KeyCode::KeyD)],
                )
                .off_if(!station, "CE N'EST PAS UNE STATION")
                .off_if(!flying, "DEJA AMARRE"),
            );
            let surveyed = s.scan[i] >= vector_sim::voyage::SCAN_SURVEYED;
            let down = !flying || s.landed_on().is_some();
            it.push(
                Item::new(
                    "ATTERRIR",
                    "PAV4",
                    vec![call(Game::click_body, i), call(Game::menu_land, 0)],
                )
                .off_if(!world, "PAS UN MONDE")
                .off_if(world && !flying, "QUITTEZ D'ABORD LA STATION")
                .off_if(world && flying && down, "DEJA AU SOL"),
            );
            it.push(
                Item::new(
                    "SURFACE (CARTE, CHOISIR LE SITE)",
                    "F8",
                    vec![call(Game::click_body, i), k(KeyCode::F8)],
                )
                .off_if(!world, "PAS UN MONDE")
                .off_if(world && !surveyed, "NON RELEVE : S'APPROCHER"),
            );
            let mut scan = Item::new("SCANNER (S'APPROCHER)", "G", vec![call(Game::dbl_body, i)])
                .off_if(surveyed, "DEJA RELEVE");
            if scan.off.is_none() {
                scan.off = go_off.map(Into::into);
            }
            it.push(scan);
            it.push(
                Item::new("ORBITE CIRCULAIRE ICI", "INSER", vec![k(KeyCode::Insert)])
                    .off_if(s.ref_body.is_none(), "AUCUN CORPS PROCHE")
                    .off_if(!flying, "DECOLLEZ D'ABORD"),
            );
            it.push(Item::new("DETAILS (CODEX)", "F6", vec![k(KeyCode::F6)]));
        }
        Ctx::Npc(n) if n < s.npcs.len() => {
            title = up(&s.npcs[n].name);
            it.push(Item::new("CIBLER", "CLIC", vec![call(Game::click_npc, n)]));
            it.push(
                Item::new(
                    "APPELER (RADIO)",
                    "ENTREE",
                    vec![call(Game::click_npc, n), k(KeyCode::Enter)],
                )
                .off_if(s.docked.is_some(), "DECOLLEZ D'ABORD"),
            );
        }
        Ctx::Star(n) => {
            title = "ETOILE".into();
            it.push(Item::new(
                "DETAILS",
                "CLIC",
                vec![call(Game::click_star, n)],
            ));
            it.push(
                Item::new(
                    "ALLER / ITINERAIRE (SAUT FTL)",
                    "ENTREE",
                    vec![call(Game::click_star, n), k(KeyCode::Enter)],
                )
                .off_if(!s.readiness().can_fly(), "LE VAISSEAU NE PEUT PAS VOLER"),
            );
            it.push(
                Item::new(
                    "VOYAGER ICI (ROUTE AUTOMATIQUE)",
                    "V",
                    vec![call(Game::click_star, n), k(KeyCode::KeyV)],
                )
                .off_if(!s.readiness().can_fly(), "LE VAISSEAU NE PEUT PAS VOLER"),
            );
        }
        Ctx::Crew(n) => {
            title = s
                .crew
                .members
                .get(n)
                .map_or("EQUIPAGE".into(), |p| up(&p.name));
            let plain = g.screen == Screen::Crew && g.crew_tab == crate::game::CrewTab::List;
            it.push(Item::new(
                "CHOISIR",
                "CLIC",
                vec![call(Game::click_crew, n)],
            ));
            if plain {
                let sel = call(Game::click_crew, n);
                it.push(Item::new(
                    "SOIGNER (MEDECINE)",
                    "M",
                    vec![sel, k(KeyCode::KeyM)],
                ));
                it.push(Item::new("CRYOGENISER", "C", vec![sel, k(KeyCode::KeyC)]));
                it.push(Item::new(
                    "POSTE SUIVANT",
                    "DROITE",
                    vec![sel, k(KeyCode::ArrowRight)],
                ));
                it.push(Item::new(
                    "POSTE PRECEDENT",
                    "GAUCHE",
                    vec![sel, k(KeyCode::ArrowLeft)],
                ));
                it.push(Item::new(
                    "CHANGER DE QUART",
                    "T",
                    vec![sel, k(KeyCode::KeyT)],
                ));
                it.push(Item::new("EQUIPEMENT", "E", vec![sel, k(KeyCode::KeyE)]));
                it.push(Item::new("ORGANES", "O", vec![sel, k(KeyCode::KeyO)]));
                it.push(Item::new(
                    "ARBRE FAMILIAL",
                    "G",
                    vec![sel, k(KeyCode::KeyG)],
                ));
                it.push(Item::new(
                    "RESEAU DES RELATIONS",
                    "V",
                    vec![sel, k(KeyCode::KeyV)],
                ));
                it.push(
                    Item::new("DEBARQUER", "X", vec![sel, k(KeyCode::KeyX)])
                        .off_if(s.docked.is_none(), "A QUAI SEULEMENT"),
                );
            }
        }
        Ctx::Contract(n) => {
            title = "CONTRAT".into();
            let offers = g.offers.len();
            let sel = call(Game::click_contract, n);
            it.push(Item::new("DETAILS", "CLIC", vec![sel]));
            if n < offers {
                it.push(Item::new(
                    "ACCEPTER",
                    "ENTREE",
                    vec![sel, k(KeyCode::Enter)],
                ));
            } else {
                it.push(Item::new("LIVRER", "ENTREE", vec![sel, k(KeyCode::Enter)]));
                it.push(Item::new("ABANDONNER", "X", vec![sel, k(KeyCode::KeyX)]));
            }
        }
        Ctx::Market(n) => {
            let name = g
                .data
                .resources
                .get(n)
                .map_or(String::new(), |r| up(&r.name));
            title = name;
            let sel = call(Game::click_sel_res, n);
            it.push(Item::new("CHOISIR", "CLIC", vec![sel]));
            match g.screen {
                Screen::Station => {
                    it.push(Item::new(
                        "ACHETER 100 KG",
                        "B",
                        vec![sel, k(KeyCode::KeyB)],
                    ));
                    it.push(Item::new("VENDRE 100 KG", "S", vec![sel, k(KeyCode::KeyS)]));
                    it.push(Item::new(
                        "TOUT VENDRE",
                        "ENTREE",
                        vec![sel, k(KeyCode::Enter)],
                    ));
                    it.push(Item::new("FAIRE LE PLEIN", "F", vec![k(KeyCode::KeyF)]));
                    it.push(Item::new("REPARER", "R", vec![k(KeyCode::KeyR)]));
                    it.push(Item::new("DECOLLER", "U", vec![k(KeyCode::KeyU)]));
                }
                Screen::Inventory => {
                    it.push(Item::new("JETER", "J", vec![sel, k(KeyCode::KeyJ)]));
                    it.push(Item::new(
                        "VERSER LE CARBURANT AU RESERVOIR",
                        "F",
                        vec![k(KeyCode::KeyF)],
                    ));
                }
                _ => {}
            }
        }
        Ctx::BourseRes(n) => {
            title = g
                .data
                .resources
                .get(n)
                .map_or("BOURSE".into(), |r| up(&r.name));
            let sel = call(Game::click_bourse_res, n);
            it.push(Item::new("COURBE", "CLIC", vec![sel]));
            it.push(Item::new(
                "ACHETER A TERME (LONG)",
                "B",
                vec![sel, k(KeyCode::KeyB)],
            ));
            it.push(Item::new(
                "VENDRE A TERME (COURT)",
                "N",
                vec![sel, k(KeyCode::KeyN)],
            ));
            it.push(Item::new("ACTIONS / MATIERES", "E", vec![k(KeyCode::KeyE)]));
        }
        Ctx::Fleet(n) => {
            title = s.fleet.get(n).map_or("FLOTTE".into(), |f| up(&f.name));
            let sel = call(Game::click_fleet_ship, n);
            it.push(Item::new("CHOISIR", "CLIC", vec![sel]));
            it.push(Item::new("ORDRE SUIVANT", "O", vec![sel, k(KeyCode::KeyO)]));
            it.push(Item::new(
                "RAVITAILLER L'ESCADRE",
                "R",
                vec![sel, k(KeyCode::KeyR)],
            ));
            it.push(Item::new(
                "REPARER L'ESCADRE",
                "V",
                vec![sel, k(KeyCode::KeyV)],
            ));
            it.push(
                Item::new("AFFECTER A LA ROUTE", "A", vec![sel, k(KeyCode::KeyA)])
                    .off_if(s.routes.is_empty(), "AUCUNE ROUTE (N : EN CREER UNE)"),
            );
            it.push(Item::new(
                "RETIRER DE LA ROUTE",
                "U",
                vec![sel, k(KeyCode::KeyU)],
            ));
            it.push(Item::new("REVENDRE", "X", vec![sel, k(KeyCode::KeyX)]));
        }
        Ctx::Policy(code) => {
            title = "POLITIQUE".into();
            let row = code / 3;
            it.push(Item::new(
                "CHOISIR",
                "CLIC",
                vec![call(Game::click_policy, row * 3 + 1)],
            ));
            it.push(Item::new(
                "PLUS",
                "DROITE",
                vec![call(Game::click_policy, row * 3 + 2)],
            ));
            it.push(Item::new(
                "MOINS",
                "GAUCHE",
                vec![call(Game::click_policy, row * 3)],
            ));
            colony_tail(&mut it);
        }
        Ctx::Storage(n) => {
            title = g
                .data
                .resources
                .get(n)
                .map_or("STOCK".into(), |r| up(&r.name));
            let sel = call(Game::click_storage, n);
            it.push(Item::new("CHOISIR", "CLIC", vec![sel]));
            let at_dock = s.docked_colony().is_some();
            it.push(
                Item::new(
                    "DEPOSER (SOUTE VERS COLONIE)",
                    "D",
                    vec![sel, k(KeyCode::KeyD)],
                )
                .off_if(!at_dock, "AMARREZ-VOUS A LA COLONIE"),
            );
            it.push(
                Item::new(
                    "RETIRER (COLONIE VERS SOUTE)",
                    "W",
                    vec![sel, k(KeyCode::KeyW)],
                )
                .off_if(!at_dock, "AMARREZ-VOUS A LA COLONIE"),
            );
            it.push(Item::new(
                "RECUPERER LA CAISSE",
                "T",
                vec![k(KeyCode::KeyT)],
            ));
            colony_tail(&mut it);
        }
        Ctx::BuildOpt(n) => {
            title = g
                .data
                .buildings
                .get(n)
                .map_or("BATIMENT".into(), |b| up(&b.name));
            let sel = call(Game::click_build, n);
            it.push(Item::new("CHOISIR", "CLIC", vec![sel]));
            it.push(Item::new(
                "CONSTRUIRE",
                "ENTREE",
                vec![sel, k(KeyCode::Enter)],
            ));
            colony_tail(&mut it);
        }
        Ctx::Empire(n) => {
            title = "COLONIE".into();
            let sel = call(Game::click_empire, n);
            it.push(Item::new("CHOISIR", "CLIC", vec![sel]));
            it.push(Item::new(
                "DETAIL : CONSTRUIRE, POLITIQUE",
                "ENTREE",
                vec![sel, k(KeyCode::Enter)],
            ));
            it.push(Item::new("FLOTTE ET ROUTES", "F3", vec![k(KeyCode::F3)]));
        }
        Ctx::Tech(n) => {
            title = "TECHNOLOGIE".into();
            let sel = call(Game::click_tech, n);
            it.push(Item::new("DETAILS", "CLIC", vec![sel]));
            it.push(Item::new(
                "RECHERCHER",
                "ENTREE / 2 CLICS",
                vec![call(Game::start_tech, n)],
            ));
        }
        Ctx::Yard(n) => {
            title = "COQUE".into();
            let sel = call(Game::click_yard, n);
            it.push(Item::new("DETAILS", "CLIC", vec![sel]));
            it.push(Item::new(
                "ACHETER / TRANSFERER",
                "ENTREE",
                vec![sel, k(KeyCode::Enter)],
            ));
            it.push(Item::new(
                "CONCEVOIR SON VAISSEAU",
                "C",
                vec![k(KeyCode::KeyC)],
            ));
        }
        Ctx::Recruit(n) => {
            title = "CANDIDAT".into();
            let sel = call(Game::click_recruit, n);
            it.push(Item::new("DETAILS", "CLIC", vec![sel]));
            it.push(Item::new(
                "EMBAUCHER",
                "ENTREE",
                vec![sel, k(KeyCode::Enter)],
            ));
        }
        Ctx::Slot(n) => {
            title = format!("EMPLACEMENT {}", n + 1);
            let sel = call(Game::click_ship_slot, n);
            let docked = s.docked.is_some();
            it.push(Item::new("DETAILS", "CLIC", vec![sel]));
            it.push(
                Item::new(
                    "INSTALLER / REMPLACER",
                    "ENTREE",
                    vec![sel, k(KeyCode::Enter)],
                )
                .off_if(!docked, "A QUAI SEULEMENT"),
            );
            it.push(
                Item::new("RETIRER LE MODULE", "X", vec![sel, k(KeyCode::KeyX)])
                    .off_if(!docked, "A QUAI SEULEMENT"),
            );
            it.push(
                Item::new("CHANTIER NAVAL", "Y", vec![k(KeyCode::KeyY)])
                    .off_if(!docked, "A QUAI SEULEMENT"),
            );
        }
        Ctx::Faction(n) => {
            title = g
                .data
                .factions
                .get(n)
                .map_or("FACTION".into(), |f| up(&f.name));
            let sel = call(Game::click_faction, n);
            it.push(Item::new("DETAILS", "CLIC", vec![sel]));
            it.push(Item::new(
                "PAYER LES AMENDES",
                "P",
                vec![sel, k(KeyCode::KeyP)],
            ));
            it.push(Item::new("POT-DE-VIN", "B", vec![sel, k(KeyCode::KeyB)]));
            it.push(Item::new(
                "ENGAGER UN AVOCAT",
                "L",
                vec![sel, k(KeyCode::KeyL)],
            ));
            it.push(Item::new(
                "REGISTRE DES RELATIONS",
                "TAB",
                vec![k(KeyCode::Tab)],
            ));
            it.push(Item::new(
                "GUERRE ET RENSEIGNEMENT",
                "G",
                vec![k(KeyCode::KeyG)],
            ));
        }
        Ctx::HubSpace(n) => {
            title = crate::hub::SPACES
                .get(n)
                .map_or("ESPACE".into(), |sp| sp.name.into());
            it.push(Item::new(
                "OUVRIR L'ESPACE",
                "CLIC / 1-8",
                vec![call(Game::click_hub_space, n)],
            ));
            it.push(Item::new(
                "TABLEAU DE BORD",
                "0",
                vec![call(Game::click_hub_dashboard, 0)],
            ));
        }
        Ctx::HubEntry(code) => {
            let (sp, j) = (code / 100, code % 100);
            if let Some(e) = crate::hub::SPACES.get(sp).and_then(|x| x.entries.get(j)) {
                title = e.label.into();
                let why = crate::hub::unavailable(g, e);
                it.push(
                    Item::new("OUVRIR", e.key, vec![call(Game::click_hub_entry, code)])
                        .off_if(why.is_some(), why.unwrap_or("")),
                );
                if let Some(t) = e.target {
                    it.push(Item::new(
                        "AIDE DE CET ECRAN",
                        "F1",
                        vec![call(Game::open_help_for, crate::hub::screen_id(t))],
                    ));
                }
            }
        }
        Ctx::DashPanel(n) => {
            title = "PANNEAU".into();
            let sel = call(Game::click_dash_panel, n);
            it.push(Item::new("CHOISIR", "CLIC", vec![sel]));
            it.push(Item::new(
                "OUVRIR L'ECRAN DETAILLE",
                "ENTREE",
                vec![sel, k(KeyCode::Enter)],
            ));
        }
        Ctx::DashLink(code) => {
            title = "LIEN".into();
            it.push(Item::new(
                "OUVRIR L'ECRAN DETAILLE",
                "CLIC",
                vec![call(Game::click_dash_link, code)],
            ));
        }
        Ctx::Cell(x, y) => {
            title = cell_items(g, x, y, &mut it);
        }
        Ctx::Key(code) => {
            title = "COMMANDE".into();
            it.push(Item::new("EXECUTER", &key_name(code), vec![k(code)]));
        }
        Ctx::Other(act) => {
            title = "ELEMENT".into();
            match act {
                Act::Hold(c) => it.push(
                    Item::new("ACTIVER", &key_name(c), vec![k(c)])
                        .off_if(true, "MAINTENIR LE CLIC GAUCHE"),
                ),
                a => it.push(Item::new("ACTIVER", "CLIC", vec![a])),
            }
        }
        // A body or a ship that no longer exists.
        Ctx::Body(_) | Ctx::Npc(_) => {}
    }
    crate::creative_view::context_items(g, ctx, &mut it);
    tail(g, &mut it);
    (title, it)
}

/// The colonies' screens share the same three side doors.
fn colony_tail(it: &mut Vec<Item>) {
    it.push(Item::new(
        "PROJETS DE LA COLONIE",
        "P",
        vec![k(KeyCode::KeyP)],
    ));
    it.push(Item::new("SCHEMA DES CHAINES", "F", vec![k(KeyCode::KeyF)]));
    it.push(Item::new("SUIVI SANITAIRE", "H", vec![k(KeyCode::KeyH)]));
}

/// The name of a key as the command bar writes it.
pub fn key_name(code: KeyCode) -> String {
    let s = match code {
        KeyCode::Enter => "ENTREE",
        KeyCode::Escape => "ECHAP",
        KeyCode::Space => "ESP",
        KeyCode::Tab => "TAB",
        KeyCode::Delete => "SUPPR",
        KeyCode::Insert => "INSER",
        KeyCode::Home => "ACCUEIL",
        KeyCode::End => "FIN",
        KeyCode::PageUp => "PGUP",
        KeyCode::PageDown => "PGDN",
        KeyCode::ArrowUp => "HAUT",
        KeyCode::ArrowDown => "BAS",
        KeyCode::ArrowLeft => "GAUCHE",
        KeyCode::ArrowRight => "DROITE",
        KeyCode::ShiftLeft => "MAJ",
        KeyCode::ControlLeft => "CTRL",
        KeyCode::Backspace => "RETOUR ARRIERE",
        c => {
            let d = format!("{c:?}");
            return d
                .strip_prefix("Key")
                .or_else(|| d.strip_prefix("Digit"))
                .map_or(d.clone(), str::to_string);
        }
    };
    s.into()
}

/// The menu of a cell of the walk; returns the title.
fn cell_items(g: &Game, x: i32, y: i32, it: &mut Vec<Item>) -> String {
    use vector_sim::rogue::{T_DOWN, T_FEATURE, T_SITE, T_TERMINAL, T_UP, T_WALL, T_WATER};
    let Some(r) = g.sim.rogue.as_ref() else {
        return "CASE".into();
    };
    if !r.inside(x, y) {
        return "HORS CARTE".into();
    }
    let f = r.floor();
    let i = r.idx(x, y);
    let seen = f.seen[i];
    let t = f.tiles[i];
    let mon = f
        .monsters
        .iter()
        .find(|m| (m.x, m.y) == (x, y) && r.visible()[i]);
    let item = f.items.iter().find(|m| (m.x, m.y) == (x, y) && seen);
    let (dx, dy) = (x - r.x, y - r.y);
    let near = dx.abs().max(dy.abs()) == 1;
    let me = (x, y) == (r.x, r.y);
    let code = crate::rogue_view::pack_cell(x, y);
    let walk = call(Game::rogue_walk_cell, code);
    let face = call(Game::rogue_face_cell, code);
    let kind = if let Some(m) = mon {
        up(&m.name)
    } else if !seen {
        "CASE INCONNUE".into()
    } else if t == T_WALL {
        "MUR".into()
    } else if t == T_WATER {
        "EAU".into()
    } else if t == T_UP {
        "ESCALIER MONTANT".into()
    } else if t == T_DOWN {
        "ESCALIER DESCENDANT".into()
    } else if t == T_SITE {
        "SITE".into()
    } else if t >= T_TERMINAL {
        "BORNE".into()
    } else if t >= T_FEATURE {
        "INSTALLATION".into()
    } else {
        "SOL".into()
    };
    let title = format!("{kind} ({x},{y})");
    if let Some(m) = mon {
        if m.friendly {
            it.push(Item::new("PARLER", "FONCER DESSUS", vec![walk]));
        } else {
            it.push(Item::new(
                if near {
                    "ATTAQUER"
                } else {
                    "S'APPROCHER ET ATTAQUER"
                },
                "FONCER DESSUS",
                vec![walk],
            ));
            it.push(Item::new("TIRER (ARME A FEU)", "T", vec![k(KeyCode::KeyT)]));
            it.push(Item::new("LANCER UNE GRENADE", "G", vec![k(KeyCode::KeyG)]));
            if m.species.is_some() {
                it.push(
                    Item::new("CAPTURER", "C", vec![k(KeyCode::KeyC)]).off_if(!near, "TROP LOIN"),
                );
            }
        }
    } else {
        it.push(
            Item::new("MARCHER JUSQU'ICI", "CLIC", vec![walk])
                .off_if(!seen, "CASE INCONNUE")
                .off_if(t == T_WALL, "C'EST UN MUR")
                .off_if(me, "VOUS Y ETES"),
        );
    }
    if let Some(m) = item {
        let _ = m;
        it.push(Item::new(
            if item.is_some_and(|m| m.plant) {
                "CUEILLIR"
            } else {
                "RAMASSER"
            },
            "CLIC",
            vec![walk],
        ));
    }
    if t == T_UP || t == T_DOWN || t == T_SITE || t >= T_TERMINAL {
        let what = if t == T_SITE {
            "VISITER LE SITE"
        } else if t >= T_TERMINAL {
            "UTILISER LA BORNE"
        } else {
            "PRENDRE L'ESCALIER"
        };
        it.push(
            Item::new(what, "ENTREE", vec![walk, k(KeyCode::Enter)]).off_if(!seen, "CASE INCONNUE"),
        );
    }
    let adj = "CASE ADJACENTE REQUISE";
    it.push(Item::new("CREUSER / CASSER", "X", vec![face, k(KeyCode::KeyX)]).off_if(!near, adj));
    it.push(Item::new("BATIR UN MUR", "B", vec![face, k(KeyCode::KeyB)]).off_if(!near, adj));
    it.push(Item::new("POSER UN SOL", "N", vec![face, k(KeyCode::KeyN)]).off_if(!near, adj));
    it.push(Item::new("LANCER LE DRONE", "V", vec![k(KeyCode::KeyV)]));
    it.push(
        Item::new("RAPPELER L'EQUIPE", "R", vec![k(KeyCode::KeyR)]).off_if(
            !(r.on_surface() && r.parent.is_none()),
            "SURFACE D'UN MONDE SEULEMENT",
        ),
    );
    it.push(Item::new(
        "FICHE DE LA CASE",
        "-",
        vec![call(Game::rogue_inspect_cell, code)],
    ));
    title
}

// ---- opening, choosing, closing -------------------------------------------------------

impl Game {
    /// A right click at (x, y): the menu of the zone under it; on an empty zone, back (the cockpit
    /// has no "back": it gets the menu of the screen).
    pub fn pointer_context(&mut self, x: i32, y: i32) {
        if self.mouse_look && self.screen == Screen::Flight {
            return;
        }
        if self.console.open {
            return;
        }
        self.pointer.pos = (x, y);
        let was_open = self.ctx_menu.take().is_some();
        let hit = self
            .pointer
            .hits
            .borrow()
            .iter()
            .rev()
            .find(|h| x >= h.rect.0 && x < h.rect.2 && y >= h.rect.1 && y < h.rect.3)
            .copied();
        let (title, items) = match hit {
            Some(h) => {
                let c = ctx_of(self, h.act, (x, y));
                items_for(self, c)
            }
            None if was_open => return,
            None if self.screen == Screen::Flight => screen_items(self),
            None => {
                self.pointer_back();
                return;
            }
        };
        if items.is_empty() {
            return;
        }
        let sel = items.iter().position(|i| i.off.is_none()).unwrap_or(0);
        self.ctx_menu = Some(Menu {
            at: (x, y),
            title,
            items,
            sel,
            rect: Cell::new((0, 0, 0, 0)),
        });
    }

    /// Run item `i` of the open menu (a greyed one says why and stays open).
    pub fn ctx_pick(&mut self, i: usize) {
        let Some(m) = self.ctx_menu.as_ref() else {
            return;
        };
        let Some(item) = m.items.get(i) else {
            return;
        };
        if let Some(why) = item.off.clone() {
            self.say(why);
            return;
        }
        let run = item.run.clone();
        self.ctx_menu = None;
        for a in run {
            self.run_act(a);
        }
    }

    /// Keys while the menu is open. True when the key was used (all of them are).
    pub fn ctx_key(&mut self, code: KeyCode) -> bool {
        let Some(m) = self.ctx_menu.as_mut() else {
            return false;
        };
        let n = m.items.len().max(1);
        match code {
            KeyCode::ArrowUp => m.sel = (m.sel + n - 1) % n,
            KeyCode::ArrowDown | KeyCode::Tab => m.sel = (m.sel + 1) % n,
            KeyCode::Enter | KeyCode::Space => {
                let i = m.sel;
                self.ctx_pick(i);
            }
            _ => self.ctx_menu = None,
        }
        true
    }

    /// A left click while the menu is open that is not on one of its lines: it closes.
    /// True when the click was eaten.
    pub fn ctx_click_outside(&mut self, x: i32, y: i32) -> bool {
        let Some(m) = self.ctx_menu.as_ref() else {
            return false;
        };
        let (c, r, w, h) = m.rect.get();
        let inside = x >= (c * 8) as i32 - 4
            && x < ((c + w) * 8) as i32 + 4
            && y >= (r * 10) as i32 - 4
            && y < ((r + h) * 10) as i32 + 4;
        if inside {
            return false;
        }
        self.ctx_menu = None;
        true
    }

    /// The help of another screen than the current one (from a menu line).
    pub fn open_help_for(&mut self, id: usize) {
        self.help_for = crate::hub::screen_from_id(id);
        self.help_open = true;
    }
}

/// The menu of the cockpit itself (a right click in empty space).
fn screen_items(g: &Game) -> (String, Vec<Item>) {
    let mut it = vec![
        Item::new("MENU : TOUS LES ECRANS", "F10", vec![k(KeyCode::F10)]),
        Item::new("TABLEAU DE BORD", "\\", vec![k(KeyCode::Backslash)]),
        Item::new("CARTE DU SYSTEME", "M", vec![k(KeyCode::KeyM)]),
        Item::new("CIBLE LA PLUS PROCHE", "TAB", vec![k(KeyCode::Tab)]),
        Item::new(
            if g.sim.autopilot.is_some() {
                "COUPER LE PILOTE AUTO"
            } else {
                "PILOTE AUTO VERS LA CIBLE"
            },
            "G",
            vec![k(KeyCode::KeyG)],
        )
        .off_if(
            g.sim.target.is_none() && g.sim.autopilot.is_none(),
            "AUCUNE CIBLE",
        ),
        Item::new("PAUSE", "P", vec![k(KeyCode::KeyP)]),
        Item::new("SAUVER", "F5", vec![k(KeyCode::F5)]),
    ];
    tail(g, &mut it);
    ("COCKPIT".into(), it)
}

// ---- drawing -----------------------------------------------------------------------------

/// Where a box of `w` x `h` cells opened at pixel `at` goes so that it stays on the screen (and
/// off the command bar of the last row).
pub fn place(at: (i32, i32), w: usize, h: usize, cols: usize, rows: usize) -> (usize, usize) {
    let c = (at.0.max(0) as usize) / 8;
    let r = (at.1.max(0) as usize) / 10;
    let max_c = cols.saturating_sub(w);
    let max_r = rows.saturating_sub(1).saturating_sub(h);
    (c.min(max_c), r.min(max_r))
}

/// The size in cells of a menu.
pub fn size_of(m: &Menu) -> (usize, usize) {
    let mut w = m.title.chars().count() + 2;
    for i in &m.items {
        let right = i
            .off
            .as_ref()
            .map_or(i.key.chars().count(), |r| r.chars().count());
        w = w.max(i.label.chars().count() + 3 + right);
    }
    (w + 2, m.items.len() + 3)
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let Some(m) = g.ctx_menu.as_ref() else {
        return;
    };
    let (w, h) = size_of(m);
    let (c, r) = place(m.at, w, h, fb.cols(), fb.rows());
    m.rect.set((c, r, w, h));
    let (px, py) = ((c * 8) as i32 - 4, (r * 10) as i32 - 4);
    let (pw, ph) = ((w * 8) as i32 + 8, (h * 10) as i32 + 8);
    fb.rect_over(px, py, pw, ph, BACK);
    for (x0, y0, x1, y1) in [
        (px, py, px + pw, py),
        (px, py + ph, px + pw, py + ph),
        (px, py, px, py + ph),
        (px + pw, py, px + pw, py + ph),
    ] {
        fb.line(x0 as f32, y0 as f32, x1 as f32, y1 as f32, AMBER);
    }
    fb.text(c + 1, r, &m.title, AMBER);
    let sep = (r * 10 + 12) as f32;
    fb.line(px as f32, sep, (px + pw) as f32, sep, DIM);
    for (i, item) in m.items.iter().enumerate() {
        let row = r + 2 + i;
        let hover = g.hit_cells(c + 1, row, w - 2, 1, Act::Call(Game::ctx_pick, i), None);
        let live = item.off.is_none();
        let col = if !live {
            DIM
        } else if i == m.sel || hover {
            if hover {
                HOVER
            } else {
                AMBER
            }
        } else {
            GREEN
        };
        let mark = if i == m.sel && live { ">" } else { " " };
        fb.text(c + 1, row, &format!("{mark}{}", item.label), col);
        match &item.off {
            Some(why) => {
                let n = why.chars().count();
                fb.text(c + w - 1 - n, row, why, RED);
            }
            None => {
                let n = item.key.chars().count();
                fb.text(c + w - 1 - n, row, &item.key, DIM);
            }
        }
    }
}

/// The "? AIDE" button of the top bar: the help of this screen, one click away.
pub fn draw_help_button(fb: &mut Framebuffer, g: &Game) {
    if matches!(g.screen, Screen::MainMenu | Screen::Destroyed) {
        return;
    }
    let label = if g.help_open {
        "? FERMER L'AIDE"
    } else {
        "? AIDE (F1)"
    };
    let n = label.chars().count();
    let menu = 11 + 1; // "MENU (F10)" and its margin
    let used = match g.screen {
        Screen::NewGame => 1,
        Screen::Dashboard => menu + 1,
        _ => menu + 1 + crate::dash_view::badge_width(g) + 1,
    };
    let col = fb.cols().saturating_sub(used + n + 1);
    let hover = g.hit_cells(col, 0, n, 1, Act::Key(KeyCode::F1), None);
    for y in 0..10 {
        for x in (col * 8) as i32 - 2..((col + n) * 8) as i32 + 2 {
            fb.px_set(x, y, 0);
        }
    }
    fb.text(col, 0, label, if hover { HOVER } else { AMBER });
}

// ---- tooltips -----------------------------------------------------------------------------

/// What the zone under the pointer says about itself, after a short rest of the pointer.
pub fn tooltip(g: &Game) -> Option<Vec<(String, u32)>> {
    if g.ctx_menu.is_some() {
        return None;
    }
    let (x, y) = g.pointer.pos;
    let act = g
        .pointer
        .hits
        .borrow()
        .iter()
        .rev()
        .find(|h| x >= h.rect.0 && x < h.rect.2 && y >= h.rect.1 && y < h.rect.3)
        .map(|h| (h.act, h.double.is_some()))?;
    let ctx = ctx_of(g, act.0, (x, y));
    if matches!(ctx, Ctx::Other(_)) {
        return None;
    }
    let (title, items) = items_for(g, ctx);
    let first = items.first()?;
    let mut out: Vec<(String, u32)> = Vec::new();
    match ctx {
        Ctx::Key(code) => {
            out.push((format!("{} : {}", key_name(code), key_desc(code)), GREEN));
        }
        Ctx::HubEntry(_) => {
            out.push((title, AMBER));
            match &first.off {
                Some(why) => out.push((format!("INDISPONIBLE : {why}"), RED)),
                None => out.push(("CLIC : OUVRIR".into(), GREEN)),
            }
        }
        _ => {
            out.push((title, AMBER));
            let second = items.get(1).filter(|_| !matches!(ctx, Ctx::Cell(..)));
            out.push((format!("CLIC : {}", first.label), GREEN));
            if act.1 {
                if let Some(s) = second {
                    out.push((format!("DOUBLE CLIC : {}", s.label), GREEN));
                }
            }
        }
    }
    out.push(("CLIC DROIT : ACTIONS POSSIBLES".into(), DIM));
    Some(out)
}

/// What a global key does, for the tooltip of a button that presses it.
pub fn key_desc(code: KeyCode) -> &'static str {
    match code {
        KeyCode::F1 => "AIDE DE CET ECRAN",
        KeyCode::F10 => "MENU : LES HUIT ESPACES, TOUS LES ECRANS",
        KeyCode::Escape => "RETOUR A L'ECRAN PRECEDENT",
        KeyCode::Enter => "VALIDER / AGIR SUR LA LIGNE CHOISIE",
        KeyCode::Tab => "CHANGER DE CIBLE OU D'ONGLET",
        KeyCode::KeyM => "CARTE, OU MEDECINE SELON L'ECRAN",
        KeyCode::KeyG => "PILOTE AUTO VERS LA CIBLE (OU ARBRE, GUERRE SELON L'ECRAN)",
        KeyCode::KeyD => "AMARRER A LA STATION PROCHE",
        KeyCode::KeyL => "GALAXIE ET SAUT FTL",
        KeyCode::KeyH => "LE VAISSEAU : MODULES, ENERGIE",
        KeyCode::KeyK => "L'EQUIPAGE",
        KeyCode::KeyI => "LA SOUTE",
        KeyCode::KeyJ => "LE JOURNAL",
        KeyCode::KeyP => "PAUSE",
        KeyCode::F5 => "SAUVER LA PARTIE",
        KeyCode::F9 => "CHARGER LA PARTIE",
        KeyCode::F2 => "L'EMPIRE : COLONIES",
        KeyCode::F3 => "LA FLOTTE",
        KeyCode::F4 => "LES TECHNOLOGIES",
        KeyCode::F6 => "LE CODEX",
        KeyCode::F8 => "LA SURFACE DU MONDE CIBLE",
        KeyCode::End => "EXPLORER A PIED",
        KeyCode::Home => "PORTAIL ET TEMPS",
        KeyCode::Insert => "ORBITE CIRCULAIRE AUTOUR DU CORPS PROCHE",
        _ => "CLIC = APPUYER SUR CETTE TOUCHE",
    }
}

pub fn draw_tooltip(fb: &mut Framebuffer, g: &Game) {
    let (pos, n) = g.pointer.hover.get();
    let n = if pos == g.pointer.pos { n + 1 } else { 0 };
    g.pointer.hover.set((g.pointer.pos, n));
    if n < TIP_FRAMES {
        return;
    }
    let Some(lines) = tooltip(g) else {
        return;
    };
    let w = lines
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    let h = lines.len();
    // Below and right of the pointer, or above and left of it near the edges.
    let (mut c, mut r) = (
        (g.pointer.pos.0.max(0) as usize) / 8 + 2,
        (g.pointer.pos.1.max(0) as usize) / 10 + 2,
    );
    if c + w + 1 > fb.cols() {
        c = (g.pointer.pos.0.max(0) as usize / 8).saturating_sub(w + 1);
    }
    if r + h + 2 > fb.rows().saturating_sub(1) {
        r = (g.pointer.pos.1.max(0) as usize / 10).saturating_sub(h + 1);
    }
    c = c.min(fb.cols().saturating_sub(w + 1));
    fb.rect_over(
        (c * 8) as i32 - 4,
        (r * 10) as i32 - 3,
        (w * 8) as i32 + 8,
        (h * 10) as i32 + 6,
        BACK,
    );
    for (i, (l, col)) in lines.iter().enumerate() {
        fb.text(c + 1, r + i, l, *col);
    }
}

// ---- what can I do here --------------------------------------------------------------------

/// One suggestion: a clickable line of the menu and of the dashboard.
pub struct Sugg {
    pub label: String,
    pub short: String,
    pub key: String,
    pub act: Act,
    pub off: Option<String>,
}

fn entry_sugg(g: &Game, entry_label: &str, label: &str, short: &str) -> Option<Sugg> {
    for (si, sp) in crate::hub::SPACES.iter().enumerate() {
        for (j, e) in sp.entries.iter().enumerate() {
            if e.label == entry_label {
                return Some(Sugg {
                    label: label.into(),
                    short: short.into(),
                    key: e.key.into(),
                    act: Act::Call(Game::click_hub_entry, si * 100 + j),
                    off: crate::hub::unavailable(g, e).map(Into::into),
                });
            }
        }
    }
    None
}

/// Three to five things that make sense right now, from the state of the game.
pub fn suggestions(g: &Game) -> Vec<Sugg> {
    let s = &g.sim;
    let mut v: Vec<Sugg> = Vec::new();
    let asleep = s.crew.alive().filter(|p| p.body.cryo).count();
    if s.rogue.is_some() {
        v.extend(entry_sugg(
            g,
            "EXPLORER A PIED",
            "RETOURNER A LA MARCHE : CLIQUER UNE CASE POUR Y ALLER",
            "MARCHER",
        ));
        v.extend(entry_sugg(
            g,
            "EQUIPAGE",
            "VOIR L'EQUIPE : SANTE, EQUIPEMENT",
            "EQUIPE",
        ));
        v.extend(entry_sugg(
            g,
            "SOUTE",
            "VOIR CE QUE L'EQUIPE PORTE",
            "SOUTE",
        ));
        v.push(Sugg {
            label: "RAPPELER L'EQUIPE (SURFACE) OU ENTREE SUR < POUR REMONTER".into(),
            short: "RAPPEL".into(),
            key: "R".into(),
            act: Act::Call(Game::sugg_key, 1),
            off: None,
        });
    } else if s.docked.is_some() {
        v.extend(entry_sugg(
            g,
            "MARCHE",
            "VENDRE LA SOUTE, FAIRE LE PLEIN, REPARER",
            "MARCHE",
        ));
        v.extend(entry_sugg(
            g,
            "CONTRATS",
            "PRENDRE UN CONTRAT DE LIVRAISON",
            "CONTRATS",
        ));
        v.extend(entry_sugg(
            g,
            "RECRUTEMENT",
            "EMBAUCHER UN CANDIDAT",
            "RECRUTER",
        ));
        v.extend(entry_sugg(g, "COCKPIT / DECOLLER", "DECOLLER", "DECOLLER"));
    } else if s.parked.is_some() {
        v.extend(entry_sugg(
            g,
            "EXPLORER A PIED",
            "EXPLORER A PIED LES ENVIRONS",
            "EXPLORER",
        ));
        v.extend(entry_sugg(
            g,
            "SOUTE",
            "VOIR LA SOUTE (CARBURANT, PIECES)",
            "SOUTE",
        ));
        v.extend(entry_sugg(
            g,
            "COCKPIT / DECOLLER",
            "REGAGNER LE COCKPIT",
            "COCKPIT",
        ));
    } else {
        if s.target.is_none() {
            v.extend(entry_sugg(
                g,
                "CARTE DU SYSTEME",
                "OUVRIR LA CARTE ET CHOISIR UNE CIBLE",
                "CARTE",
            ));
            v.push(Sugg {
                label: "CIBLER LE CORPS LE PLUS PROCHE".into(),
                short: "CIBLER".into(),
                key: "TAB".into(),
                act: Act::Call(Game::sugg_key, 0),
                off: None,
            });
        } else {
            v.push(Sugg {
                label: "PILOTE AUTO VERS LA CIBLE".into(),
                short: "PILOTE AUTO".into(),
                key: "G".into(),
                act: Act::Call(Game::sugg_key, 2),
                off: None,
            });
            v.push(Sugg {
                label: "AMARRER A LA STATION CIBLEE".into(),
                short: "AMARRER".into(),
                key: "D".into(),
                act: Act::Call(Game::sugg_key, 3),
                off: None,
            });
        }
        v.extend(entry_sugg(
            g,
            "GALAXIE ET SAUT FTL",
            "VOIR LES ETOILES, PREPARER UN SAUT",
            "GALAXIE",
        ));
        v.extend(entry_sugg(
            g,
            "NAVIGATION AUTO",
            "PROGRAMMER L'ORDINATEUR DE BORD",
            "NAV AUTO",
        ));
    }
    if asleep > 0 {
        v.extend(entry_sugg(
            g,
            "EQUIPAGE",
            &format!("{asleep} DORMEUR(S) EN CRYO : EQUIPAGE, C POUR LES REVEILLER"),
            "CRYO",
        ));
    }
    v.truncate(5);
    v
}

impl Game {
    /// A suggestion that presses a key of the cockpit: the menu closes into it first.
    pub fn sugg_key(&mut self, i: usize) {
        let code = [KeyCode::Tab, KeyCode::KeyR, KeyCode::KeyG, KeyCode::KeyD]
            .get(i)
            .copied();
        let Some(code) = code else {
            return;
        };
        if code == KeyCode::KeyR && self.sim.rogue.is_some() {
            self.screen = Screen::Rogue;
        } else if self.sim.rogue.is_none() && self.sim.docked.is_none() {
            self.screen = Screen::Flight;
            self.map = false;
        }
        self.hub_origin = None;
        self.dash_origin = false;
        self.run_act(Act::Key(code));
    }
}

/// The banner "WHAT CAN I DO HERE": one line per suggestion, clickable; returns the rows used.
pub fn draw_banner(fb: &mut Framebuffer, g: &Game, col: usize, row: usize, width: usize) -> usize {
    fb.text(col, row, "QUE PUIS-JE FAIRE ICI ? (CLIQUEZ)", AMBER);
    let list = suggestions(g);
    for (i, sg) in list.iter().enumerate() {
        let r = row + 1 + i;
        let text: String = format!("- {}", sg.label)
            .chars()
            .take(width.saturating_sub(12))
            .collect();
        let hover = g.hit_cells(col, r, width, 1, sg.act, None);
        let c = if sg.off.is_some() {
            DIM
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(col, r, &text, c);
        let key = sg.key.chars().take(10).collect::<String>();
        fb.text(
            col + width.saturating_sub(key.chars().count()),
            r,
            &key,
            DIM,
        );
    }
    list.len() + 1
}

/// The same on one line (the dashboard has no room for more).
pub fn draw_banner_line(fb: &mut Framebuffer, g: &Game, col: usize, row: usize, width: usize) {
    fb.text(col, row, "ICI :", DIM);
    let mut c = col + 6;
    for sg in suggestions(g) {
        let n = sg.short.chars().count() + 2;
        if c + n > col + width {
            break;
        }
        let hover = g.hit_cells(c, row, n, 1, sg.act, None);
        let colr = if sg.off.is_some() {
            DIM
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(c, row, &format!("[{}]", sg.short), colr);
        c += n + 1;
    }
}

/// The scenes of the snapshots that show a context menu or a tooltip (`--snapshot x.png ctx_body`).
pub fn snapshot_scene(g: &mut Game, scene: &str) {
    let draw = |g: &Game| {
        let mut fb = Framebuffer::new(1280, 720);
        crate::draw_frame(&mut fb, g);
    };
    g.tut_step = None;
    let find = |g: &Game, want: &dyn Fn(Ctx) -> bool| -> Option<(i32, i32)> {
        let hits = g.pointer.hits.borrow();
        hits.iter()
            .filter(|h| {
                want(ctx_of(
                    g,
                    h.act,
                    ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2),
                ))
            })
            .filter(|h| h.rect.2 < 1200 && h.rect.1 > 40)
            .max_by_key(|h| (h.rect.2 - h.rect.0) * (h.rect.3 - h.rect.1))
            .map(|h| ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2))
    };
    let tip = scene.starts_with("tip_");
    let at = match scene {
        "ctx_body" | "tip_body" => {
            g.sim.undock();
            g.screen = Screen::Flight;
            g.advance(0.5);
            draw(g);
            find(g, &|c| matches!(c, Ctx::Body(_)))
        }
        "ctx_crew" => {
            g.screen = Screen::Crew;
            draw(g);
            find(g, &|c| matches!(c, Ctx::Crew(1)))
        }
        "ctx_market" => {
            if let Some((st, _)) = g.sim.nearest_station() {
                g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
                g.key(KeyCode::KeyD, None, true);
            }
            g.screen = Screen::Station;
            draw(g);
            find(g, &|c| matches!(c, Ctx::Market(2)))
        }
        "ctx_cell" => {
            let _ = g.sim.rogue_enter(0, vec![]);
            g.screen = Screen::Rogue;
            draw(g);
            g.sim
                .rogue
                .as_ref()
                .map(|r| crate::rogue_view::cell_px(r, r.x + 1, r.y))
        }
        "ctx_hub" | "tip_hub" => {
            g.open_hub();
            g.hub_select(if tip { 4 } else { 0 });
            draw(g);
            find(
                g,
                &|c| matches!(c, Ctx::HubEntry(n) if n % 100 < 20 && n / 100 == g.hub.sel),
            )
        }
        "ctx_dash" => {
            g.open_dashboard();
            draw(g);
            find(g, &|c| matches!(c, Ctx::DashLink(_)))
        }
        "ctx_help" => {
            g.open_hub();
            g.hub_select(3);
            g.open_help_for(crate::hub::screen_id(Screen::Crew));
            draw(g);
            None
        }
        // The cockpit, in empty space.
        _ => {
            g.sim.undock();
            g.screen = Screen::Flight;
            draw(g);
            Some((420, 300))
        }
    };
    if let Some((x, y)) = at {
        g.pointer.pos = (x, y);
        if tip {
            g.pointer.hover.set(((x, y), 99));
        } else {
            g.pointer_context(x, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::DEFAULT_SEED;
    use std::sync::Arc;

    fn game() -> Game {
        let dir = vector_data::find_data_dir().expect("data");
        let data = Arc::new(vector_data::GameData::load(&dir).expect("data"));
        let mut g = Game::new(data, DEFAULT_SEED);
        g.new_game(DEFAULT_SEED);
        g
    }

    fn frame(g: &Game) -> Framebuffer {
        let mut fb = Framebuffer::new(1280, 720);
        crate::draw_frame(&mut fb, g);
        fb
    }

    #[test]
    fn the_menu_stays_on_the_screen() {
        let (cols, rows) = (160, 72);
        for at in [
            (0, 0),
            (1279, 719),
            (1279, 0),
            (0, 719),
            (640, 360),
            (-5, -5),
        ] {
            let (c, r) = place(at, 30, 12, cols, rows);
            assert!(c + 30 <= cols, "{at:?}");
            assert!(r + 12 < rows, "{at:?}: the last row is the command bar");
        }
    }

    #[test]
    fn a_right_click_on_a_body_lists_the_cockpit_actions_and_the_pick_runs_the_same_act() {
        let mut g = game();
        g.screen = Screen::Flight;
        g.sim.undock();
        let _ = frame(&g);
        // The nearest declared zone of a body.
        let hit = g
            .pointer
            .hits
            .borrow()
            .iter()
            .copied()
            .find(|h| matches!(ctx_of(&g, h.act, (0, 0)), Ctx::Body(_)));
        let h = hit.expect("a body of the cockpit is clickable");
        let (x, y) = ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2);
        g.pointer_context(x, y);
        let m = g.ctx_menu.as_ref().expect("a menu opened");
        let labels: Vec<&str> = m.items.iter().map(|i| i.label.as_str()).collect();
        for want in [
            "CIBLER",
            "AMARRER",
            "ORBITE CIRCULAIRE ICI",
            "AIDE DE CET ECRAN",
        ] {
            assert!(labels.contains(&want), "{want} in {labels:?}");
        }
        let Ctx::Body(b) = ctx_of(&g, h.act, (x, y)) else {
            unreachable!()
        };
        g.sim.target = None;
        g.ctx_pick(0);
        assert!(g.ctx_menu.is_none());
        assert_eq!(
            g.sim.target,
            Some(b),
            "the item runs the click's own function"
        );
    }

    #[test]
    fn an_unavailable_action_is_greyed_with_its_reason_and_does_not_run() {
        let mut g = game();
        g.screen = Screen::Flight;
        g.sim.undock();
        let planet = (0..g.sim.bodies.len())
            .find(|&i| matches!(g.sim.bodies[i].kind, BodyKind::Planet(_)))
            .expect("a planet");
        let (_, items) = items_for(&g, Ctx::Body(planet));
        let dock = items.iter().find(|i| i.label == "AMARRER").unwrap();
        assert_eq!(dock.off.as_deref(), Some("CE N'EST PAS UNE STATION"));
        g.pointer_context(0, 0);
        g.ctx_menu = Some(Menu {
            at: (100, 100),
            title: "T".into(),
            items,
            sel: 0,
            rect: Cell::new((0, 0, 0, 0)),
        });
        let i = g
            .ctx_menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|i| i.label == "AMARRER")
            .unwrap();
        g.ctx_pick(i);
        assert!(g.ctx_menu.is_some(), "a greyed line keeps the menu open");
        assert_eq!(
            g.messages.last().map(String::as_str),
            Some("CE N'EST PAS UNE STATION")
        );
    }

    #[test]
    fn the_crew_menu_runs_the_key_of_each_action() {
        let mut g = game();
        g.screen = Screen::Crew;
        let (_, items) = items_for(&g, Ctx::Crew(0));
        let cryo = items.iter().find(|i| i.label == "CRYOGENISER").unwrap();
        assert_eq!(cryo.key, "C");
        g.ctx_menu = Some(Menu {
            at: (50, 50),
            title: String::new(),
            sel: 0,
            rect: Cell::new((0, 0, 0, 0)),
            items,
        });
        let i = g
            .ctx_menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|i| i.label == "CRYOGENISER")
            .unwrap();
        g.ctx_pick(i);
        assert_eq!(
            g.crew_tab,
            crate::game::CrewTab::Cryo,
            "the menu pressed C on the chosen person"
        );
    }

    #[test]
    fn escape_and_a_click_elsewhere_close_the_menu_and_arrows_choose() {
        let mut g = game();
        g.screen = Screen::Flight;
        g.pointer_context(300, 300);
        assert!(g.ctx_menu.is_some(), "the cockpit menu");
        g.key(KeyCode::ArrowDown, None, true);
        g.key(KeyCode::Escape, None, true);
        assert!(g.ctx_menu.is_none());
        assert_eq!(g.screen, Screen::Flight);
        g.pointer_context(300, 300);
        let _ = frame(&g);
        g.pointer_click(5, 700, 0);
        assert!(g.ctx_menu.is_none(), "a click outside closes it");
    }

    #[test]
    fn a_right_click_on_an_empty_zone_goes_back_as_before() {
        let mut g = game();
        g.screen = Screen::Journal;
        let _ = frame(&g);
        g.pointer_context(700, 500);
        assert_ne!(g.screen, Screen::Journal);
    }

    #[test]
    fn the_cell_menu_of_the_walk_offers_the_walk_actions() {
        let mut g = game();
        g.sim.rogue_enter(0, vec![]).expect("a place to walk");
        g.screen = Screen::Rogue;
        let (x, y) = {
            let r = g.sim.rogue.as_ref().unwrap();
            (r.x + 1, r.y)
        };
        let (_, items) = items_for(&g, Ctx::Cell(x, y));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        for want in [
            "CREUSER / CASSER",
            "BATIR UN MUR",
            "POSER UN SOL",
            "RAPPELER L'EQUIPE",
            "FICHE DE LA CASE",
        ] {
            assert!(labels.contains(&want), "{want} in {labels:?}");
        }
        let far = items_for(&g, Ctx::Cell(x + 9, y)).1;
        let dig = far.iter().find(|i| i.label == "CREUSER / CASSER").unwrap();
        assert!(dig.off.is_some(), "too far to dig");
    }

    #[test]
    fn every_screen_has_a_clickable_way_back() {
        let mut g = game();
        for s in crate::hub::ALL_SCREENS {
            if matches!(
                s,
                Screen::MainMenu | Screen::NewGame | Screen::Destroyed | Screen::Flight
            ) {
                continue;
            }
            g.new_game(DEFAULT_SEED);
            if s != Screen::Station {
                // Most screens are reachable from the cockpit as well as from the dock.
                g.sim.undock();
            }
            g.screen = s;
            let _ = frame(&g);
            let back = g.pointer.hits.borrow().iter().any(|h| {
                matches!(
                    h.act,
                    Act::Key(KeyCode::Escape) | Act::Key(KeyCode::F10) | Act::Key(KeyCode::F12)
                )
            });
            assert!(back, "{s:?} has no clickable way back or out");
        }
    }

    /// One screen drawn: (text that ran off the screen, zones off the screen, pairs of word zones
    /// that overlap each other).
    fn layout_of(g: &Game) -> (usize, Vec<String>, Vec<String>, Option<String>) {
        let fb = frame(g);
        let hits: Vec<crate::mouse::Hit> = g.pointer.hits.borrow().clone();
        let mut off = Vec::new();
        for h in &hits {
            let r = h.rect;
            if r.0 >= 1280 || r.2 <= 0 || r.1 >= 720 || r.3 <= 0 || r.2 > 1288 || r.3 > 730 {
                off.push(format!("{r:?}"));
            }
        }
        let mut overlap = Vec::new();
        for (i, a) in hits.iter().enumerate() {
            for (j, b) in hits.iter().enumerate().skip(i + 1) {
                let (ra, rb) = (a.rect, b.rect);
                let small = |r: (i32, i32, i32, i32)| r.3 - r.1 <= 12;
                if !(small(ra) && small(rb)) {
                    continue;
                }
                let w = ra.2.min(rb.2) - ra.0.max(rb.0);
                let h = ra.3.min(rb.3) - ra.1.max(rb.1);
                let inside = |p: (i32, i32, i32, i32), q: (i32, i32, i32, i32)| {
                    p.0 >= q.0 && p.2 <= q.2 && p.1 >= q.1 && p.3 <= q.3
                };
                // Where a zone drawn between them covers both, the first is simply hidden.
                let (x0, y0) = (ra.0.max(rb.0), ra.1.max(rb.1));
                let hidden = hits[i + 1..j]
                    .iter()
                    .any(|c| inside((x0, y0, x0 + w, y0 + h), c.rect));
                if w > 4 && h > 4 && !hidden && !inside(ra, rb) && !inside(rb, ra) {
                    overlap.push(format!("{ra:?} / {rb:?}"));
                }
            }
        }
        (fb.clipped, off, overlap, fb.clipped_first.clone())
    }

    #[test]
    fn layout_report_of_every_screen() {
        let mut g = game();
        let mut bad = Vec::new();
        // Each screen in its plain state, with the help open, with the tutorial line, and with the
        // pointer resting on a zone (the tooltip).
        for variant in 0..8 {
            g.settings.ui_new = variant < 4;
            let variant = variant % 4;
            for s in crate::hub::ALL_SCREENS {
                if matches!(s, Screen::NewGame | Screen::Destroyed) {
                    continue;
                }
                g.new_game(DEFAULT_SEED);
                if !matches!(s, Screen::Station | Screen::Colony) {
                    g.sim.undock();
                }
                g.screen = s;
                g.help_open = variant == 1;
                if variant == 2 {
                    g.tut_step = Some(0);
                }
                for _ in 0..4 {
                    g.say("MESSAGE DE TEST ASSEZ LONG POUR VOIR OU IL SE COUPE EN BORD D'ECRAN");
                }
                let (clipped, off, overlap, first) = layout_of(&g);
                if variant == 3 {
                    // Rest on the first zone drawn and draw again.
                    let first_hit = g.pointer.hits.borrow().first().copied();
                    if let Some(h) = first_hit {
                        g.pointer.pos = ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2);
                        g.pointer.hover.set((g.pointer.pos, 99));
                    }
                }
                eprintln!(
                    "{s:?}/{variant}: clipped {clipped}, off {}, overlaps {}",
                    off.len(),
                    overlap.len()
                );
                if let Some(f) = first {
                    eprintln!("    clipped: {f}");
                }
                for o in off.iter().take(3) {
                    eprintln!("    off {o}");
                }
                for o in overlap.iter().take(3) {
                    eprintln!("    overlap {o}");
                }
                if clipped > 0 || !off.is_empty() || !overlap.is_empty() {
                    bad.push(format!(
                        "{s:?}/{variant}: {clipped} clipped, {} off, {} overlaps",
                        off.len(),
                        overlap.len()
                    ));
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// Prints, for every screen, how many zones it declares and which words of its command bar
    /// cannot be clicked (arrows, "1-9"...): `cargo test audit_mouse_dump -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn audit_mouse_dump() {
        let mut g = game();
        for s in crate::hub::ALL_SCREENS {
            g.new_game(DEFAULT_SEED);
            if !matches!(s, Screen::Station | Screen::Colony) {
                g.sim.undock();
            }
            g.screen = s;
            let _ = frame(&g);
            let zones = g.pointer.hits.borrow().len();
            let line = crate::tutorial::hint_line(&g);
            let mut dead = Vec::new();
            for seg in line.split("  ").filter(|x| !x.is_empty()) {
                let w = seg.split_whitespace().next().unwrap_or("");
                if crate::mouse::key_of_word(s, w).is_none()
                    && crate::mouse::pair_of_word(w).is_none()
                    && crate::mouse::arrow_of_word(w).is_none()
                {
                    dead.push(w.to_string());
                }
            }
            println!("AUDIT|{s:?}|{zones}|{}", dead.join(" "));
        }
    }
}
