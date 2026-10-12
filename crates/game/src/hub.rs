//! The one menu: eight spaces (piloting, navigation, ship, crew, trade, exploration on foot,
//! colonies, codex and journal) that gather every screen of the game without duplicating any of
//! them. A space opens the existing screen by the very function its own key uses; Escape goes
//! back up one level (screen, then the space page, then the menu, then where the menu was
//! opened). The menu opens with F10 or a click on MENU at the top right of every screen, and it
//! begins with an orientation: where the AI is, what is wrong, what it can do.
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

/// Colour of a line or button under the pointer.
pub const HOVER: u32 = 0x00D0_FFD0;

/// The first entry of the menu that opens `screen` (space, line).
pub fn find_entry(screen: Screen) -> Option<(usize, usize)> {
    SPACES.iter().enumerate().find_map(|(s, sp)| {
        sp.entries
            .iter()
            .position(|e| e.target == Some(screen))
            .map(|i| (s, i))
    })
}

/// What the ship must be doing for an entry to work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Any,
    Docked,
    Flying,
}

pub struct Entry {
    pub label: &'static str,
    /// The key that does the same from the game, shown to teach it.
    pub key: &'static str,
    pub desc: &'static str,
    /// The screen it opens (None for an action or a mode of the cockpit).
    pub target: Option<Screen>,
    pub need: Need,
    /// Works while the squad walks the ground (information screens).
    pub on_foot: bool,
    pub go: fn(&mut Game),
}

pub struct Space {
    pub name: &'static str,
    pub blurb: &'static str,
    pub entries: &'static [Entry],
}

/// Where a screen lives in the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Space(usize),
    /// Menus of the game itself (not a place of the ship's world).
    System,
}

pub const PILOTAGE: usize = 0;
pub const NAVIGATION: usize = 1;
pub const VAISSEAU: usize = 2;
pub const EQUIPAGE: usize = 3;
pub const COMMERCE: usize = 4;
pub const EXPLORATION: usize = 5;
pub const COLONIES: usize = 6;
pub const CODEX: usize = 7;

/// The space every screen belongs to. Exhaustive on purpose: a new screen must pick a place.
pub fn place_of(s: Screen) -> Place {
    use Screen::*;
    match s {
        Flight | Dialogue | Cosmos => Place::Space(PILOTAGE),
        Galaxy | Surface | NavAuto => Place::Space(NAVIGATION),
        Ship | Inventory | Shipyard => Place::Space(VAISSEAU),
        Crew | Recruit | Family => Place::Space(EQUIPAGE),
        Station | Contracts | Bank | Trading | Bourse | Livestock | Casino => {
            Place::Space(COMMERCE)
        }
        Rogue | Expedition => Place::Space(EXPLORATION),
        Empire | Colony | Fleet | Factions => Place::Space(COLONIES),
        Journal | Codex | Tech | Proto => Place::Space(CODEX),
        MainMenu | NewGame | Hub | Dashboard | Pause | Options | Rules | Destroyed | Designer => {
            Place::System
        }
    }
}

/// Every screen of the game (the menu's test walks it; the help of a screen is found by its place).
pub const ALL_SCREENS: [Screen; 38] = [
    Screen::Dashboard,
    Screen::Designer,
    Screen::MainMenu,
    Screen::NewGame,
    Screen::Hub,
    Screen::Flight,
    Screen::Station,
    Screen::Inventory,
    Screen::Pause,
    Screen::Ship,
    Screen::Shipyard,
    Screen::Crew,
    Screen::Recruit,
    Screen::Journal,
    Screen::Galaxy,
    Screen::Contracts,
    Screen::Bank,
    Screen::Livestock,
    Screen::Casino,
    Screen::Trading,
    Screen::Cosmos,
    Screen::Rogue,
    Screen::Empire,
    Screen::Colony,
    Screen::Fleet,
    Screen::Destroyed,
    Screen::Tech,
    Screen::Rules,
    Screen::Options,
    Screen::Codex,
    Screen::Surface,
    Screen::Bourse,
    Screen::Family,
    Screen::Factions,
    Screen::Proto,
    Screen::Dialogue,
    Screen::Expedition,
    Screen::NavAuto,
];

/// The place of a screen in `ALL_SCREENS` (a number a zone can carry).
pub fn screen_id(s: Screen) -> usize {
    ALL_SCREENS.iter().position(|&x| x == s).unwrap_or(0)
}

pub fn screen_from_id(i: usize) -> Option<Screen> {
    ALL_SCREENS.get(i).copied()
}

/// How the menu names a screen (its Debug name when no line opens it).
pub fn screen_label(s: Screen) -> String {
    SPACES
        .iter()
        .flat_map(|sp| sp.entries.iter())
        .find(|e| e.target == Some(s))
        .map_or_else(|| format!("{s:?}").to_uppercase(), |e| e.label.to_string())
}

const fn entry(
    label: &'static str,
    key: &'static str,
    desc: &'static str,
    target: Option<Screen>,
    need: Need,
    on_foot: bool,
    go: fn(&mut Game),
) -> Entry {
    Entry {
        label,
        key,
        desc,
        target,
        need,
        on_foot,
        go,
    }
}

pub static SPACES: [Space; 8] = [
    Space {
        name: "PILOTAGE",
        blurb: "Le cockpit : piloter, decoller, appeler un autre vaisseau, franchir un portail.",
        entries: &[
            entry(
                "COCKPIT / DECOLLER",
                "U A QUAI",
                "Retour aux commandes de vol ; a quai, decolle.",
                Some(Screen::Flight),
                Need::Any,
                false,
                |g| {
                    if g.sim.docked.is_some() {
                        g.station_key(KeyCode::KeyU);
                    } else {
                        g.screen = Screen::Flight;
                        g.map = false;
                    }
                },
            ),
            entry(
                "RADIO : APPELER LA CIBLE",
                "ENTREE",
                "Parler a un vaisseau ou au peuple d'un monde cible (Tab, R, Maj+Tab pour cibler).",
                Some(Screen::Dialogue),
                Need::Flying,
                false,
                |g| g.open_dialogue(),
            ),
            entry(
                "PORTAIL ET TEMPS",
                "ACCUEIL",
                "Portails vers d'autres dimensions, ancres temporelles, stase.",
                Some(Screen::Cosmos),
                Need::Any,
                false,
                |g| g.screen = Screen::Cosmos,
            ),
            entry(
                "PAUSE, SAUVEGARDE, OPTIONS",
                "ECHAP EN VOL",
                "Sauvegarder, charger, regles de partie, options, menu principal.",
                Some(Screen::Pause),
                Need::Any,
                false,
                |g| {
                    g.screen = Screen::Pause;
                    g.menu_sel = 0;
                },
            ),
        ],
    },
    Space {
        name: "NAVIGATION",
        blurb: "Se reperer : le systeme, la galaxie, la surface des mondes.",
        entries: &[
            entry(
                "CARTE DU SYSTEME",
                "M",
                "Les corps du systeme, la cible, la trajectoire (+/- zoom, C centre).",
                None,
                Need::Flying,
                false,
                |g| {
                    g.screen = Screen::Flight;
                    g.map = true;
                },
            ),
            entry(
                "GALAXIE ET SAUT FTL",
                "L",
                "Les etoiles connues ; Entree lance un saut si le moteur FTL le permet.",
                Some(Screen::Galaxy),
                Need::Any,
                false,
                |g| g.open_galaxy(),
            ),
            entry(
                "NAVIGATION AUTO",
                "PAVE NUM. *",
                "L'ordinateur de bord : file d'ordres, manoeuvres, transferts, orbites, escorte, sauts FTL.",
                Some(Screen::NavAuto),
                Need::Any,
                false,
                |g| g.open_nav_auto(),
            ),
            entry(
                "SURFACE D'UN MONDE",
                "F8",
                "Carte et globe du monde cible, une fois releve (niveau 2).",
                Some(Screen::Surface),
                Need::Any,
                false,
                |g| g.open_surface(),
            ),
        ],
    },
    Space {
        name: "VAISSEAU",
        blurb: "Le vaisseau lui-meme : modules, energie, chaleur, soutes, chantier naval.",
        entries: &[
            entry(
                "MODULES ET ENERGIE",
                "H",
                "Les emplacements, leur sante, le bus d'energie ; a quai, installer ou retirer.",
                Some(Screen::Ship),
                Need::Any,
                true,
                |g| g.open_ship(),
            ),
            entry(
                "SOUTE",
                "I",
                "Ce qui est a bord ; F verse le carburant de la soute dans le reservoir.",
                Some(Screen::Inventory),
                Need::Any,
                true,
                |g| g.screen = Screen::Inventory,
            ),
            entry(
                "CHANTIER NAVAL",
                "Y A QUAI",
                "Changer de coque, transferer les modules.",
                Some(Screen::Shipyard),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyY),
            ),
        ],
    },
    Space {
        name: "EQUIPAGE",
        blurb: "Les gens du bord : postes, besoins, famille, recrutement.",
        entries: &[
            entry(
                "EQUIPAGE",
                "K",
                "Postes, quarts, besoins, equipement, organes, relations.",
                Some(Screen::Crew),
                Need::Any,
                true,
                |g| g.screen = Screen::Crew,
            ),
            entry(
                "ARBRE FAMILIAL",
                "G DANS K",
                "La famille de la personne choisie : parents, fratrie, genes.",
                Some(Screen::Family),
                Need::Any,
                true,
                |g| {
                    g.screen = Screen::Crew;
                    g.crew_key(KeyCode::KeyG);
                },
            ),
            entry(
                "RECRUTEMENT",
                "N A QUAI",
                "Embaucher des candidats de la station.",
                Some(Screen::Recruit),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyN),
            ),
        ],
    },
    Space {
        name: "COMMERCE / STATION",
        blurb: "Tout ce que l'on fait a quai : vendre, acheter, contrats, banque, bourse, elevage.",
        entries: &[
            entry(
                "MARCHE",
                "AMARRER",
                "Vendre la soute, acheter, faire le plein, reparer.",
                Some(Screen::Station),
                Need::Docked,
                false,
                |g| g.screen = g.dock_screen(),
            ),
            entry(
                "BOURSE",
                "T A QUAI",
                "Courbes de prix, contrats a terme, actions des entreprises.",
                Some(Screen::Bourse),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyT),
            ),
            entry(
                "COURTAGE",
                "D A QUAI",
                "Rumeurs, arbitrage, cartels, appels d'offres, assurance.",
                Some(Screen::Trading),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyD),
            ),
            entry(
                "CONTRATS",
                "C A QUAI",
                "Livraisons nees des penuries reelles de la station.",
                Some(Screen::Contracts),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyC),
            ),
            entry(
                "BANQUE",
                "E A QUAI",
                "Emprunter et rembourser.",
                Some(Screen::Bank),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyE),
            ),
            entry(
                "ELEVAGE",
                "L A QUAI",
                "Acheter, reproduire et abattre des animaux.",
                Some(Screen::Livestock),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyL),
            ),
            entry(
                "CASINO",
                "A A QUAI",
                "Jeux de hasard : la maison gagne toujours a la longue.",
                Some(Screen::Casino),
                Need::Docked,
                false,
                |g| g.station_key(KeyCode::KeyA),
            ),
        ],
    },
    Space {
        name: "EXPLORATION A PIED",
        blurb: "Descendre de l'IA a l'equipe : stations, epaves, ruines, grottes, surfaces.",
        entries: &[
            entry(
                "EXPLORER A PIED",
                "FIN",
                "Choisir un lieu a portee et y envoyer l'equipe ; le butin revient en soute.",
                Some(Screen::Rogue),
                Need::Any,
                true,
                |g| g.screen = Screen::Rogue,
            ),
            entry(
                "EXPEDITION SUR UN MONDE",
                "F8 PUIS E",
                "Poser l'equipe sur la case de terre choisie du monde cible.",
                Some(Screen::Expedition),
                Need::Flying,
                false,
                |g| {
                    g.open_surface();
                    if g.screen == Screen::Surface {
                        g.open_expedition();
                    }
                },
            ),
        ],
    },
    Space {
        name: "COLONIES",
        blurb: "Ce que l'on fonde et ce que l'on gouverne : colonies, flotte, factions.",
        entries: &[
            entry(
                "EMPIRE",
                "F2",
                "La liste des colonies et leurs comptes.",
                Some(Screen::Empire),
                Need::Any,
                false,
                |g| g.open_empire(),
            ),
            entry(
                "COLONIE",
                "F2 PUIS ENTREE",
                "Batiments, stocks, politiques, construction de la colonie choisie.",
                Some(Screen::Colony),
                Need::Any,
                false,
                |g| {
                    g.open_empire();
                    if g.sim.colonies.is_empty() {
                        g.say("AUCUNE COLONIE : FONDEZ-EN UNE (CIBLE + O, KIT EN SOUTE)");
                    } else {
                        g.empire_key(KeyCode::Enter);
                    }
                },
            ),
            entry(
                "FLOTTE",
                "F3",
                "Navires, routes commerciales, carte des colonies.",
                Some(Screen::Fleet),
                Need::Any,
                false,
                |g| g.open_fleet(),
            ),
            entry(
                "FACTIONS",
                "X",
                "Les puissances, leurs moeurs, leurs alliances et votre reputation.",
                Some(Screen::Factions),
                Need::Any,
                false,
                |g| {
                    g.fac_sel = g.sim.faction.unwrap_or(g.fac_sel);
                    g.screen = Screen::Factions;
                },
            ),
            entry(
                "FONDER UNE COLONIE",
                "O",
                "Sur le corps cible, avec un kit de colonisation en soute.",
                None,
                Need::Flying,
                false,
                |g| g.found_colony_at_target(),
            ),
        ],
    },
    Space {
        name: "CODEX / JOURNAL",
        blurb: "Savoir : le journal de bord, l'encyclopedie, les technologies.",
        entries: &[
            entry(
                "JOURNAL DE BORD",
                "J",
                "Ce qui s'est passe a bord, et les nouvelles des secteurs.",
                Some(Screen::Journal),
                Need::Any,
                true,
                |g| g.screen = Screen::Journal,
            ),
            entry(
                "CODEX",
                "F6",
                "Une entree par ressource, module, coque, creature, faction... tirees des donnees.",
                Some(Screen::Codex),
                Need::Any,
                true,
                |g| g.open_codex(Screen::Hub),
            ),
            entry(
                "TECHNOLOGIES",
                "F4",
                "L'arbre de recherche ; Entree lance une recherche.",
                Some(Screen::Tech),
                Need::Any,
                true,
                |g| g.open_tech(),
            ),
            entry(
                "PROTOTYPES D'INTERFACE",
                "F12",
                "Maquettes cliquables du corps et du hall de station.",
                Some(Screen::Proto),
                Need::Flying,
                false,
                |g| g.screen = Screen::Proto,
            ),
        ],
    },
];

/// Why an entry cannot be used right now (None = it can).
pub fn unavailable(g: &Game, e: &Entry) -> Option<&'static str> {
    if g.sim.rogue.is_some() && !e.on_foot && e.target != Some(Screen::Rogue) {
        return Some("A PIED : REJOIGNEZ LE VAISSEAU D'ABORD (RAPPEL OU SORTIE)");
    }
    match e.need {
        Need::Docked if g.sim.docked.is_none() => {
            Some("AMARREZ-VOUS A UNE STATION (D, PRES D'ELLE)")
        }
        Need::Flying if g.sim.docked.is_some() => Some("DECOLLEZ D'ABORD (U A QUAI)"),
        _ => None,
    }
}

/// State of the menu: which space, whether its page is open, which line.
pub struct HubUi {
    pub sel: usize,
    pub page: bool,
    pub item: usize,
    /// The screen the menu was opened from (where Escape goes back to).
    pub from: Screen,
}

impl Default for HubUi {
    fn default() -> Self {
        HubUi {
            sel: 0,
            page: false,
            item: 0,
            from: Screen::Flight,
        }
    }
}

/// Greedy word wrap on `w` columns.
pub fn wrap(text: &str, w: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > w {
            rows.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows
}

/// A button: plain clickable text. Returns its width in cells.
pub fn button(
    fb: &mut Framebuffer,
    g: &Game,
    col: usize,
    row: usize,
    label: &str,
    act: Act,
) -> usize {
    let w = label.chars().count();
    let hover = g.hit_cells(col, row, w, 1, act, None);
    fb.text(col, row, label, if hover { HOVER } else { GREEN });
    w
}

/// Where the AI is and what it can do: the first thing the menu says.
pub struct Orientation {
    pub place: String,
    pub status: String,
    pub todo: Vec<String>,
}

pub fn orientation(g: &Game) -> Orientation {
    let s = &g.sim;
    let place = if let Some(r) = &s.rogue {
        format!(
            "A PIED : {} ({} PERSONNE(S) DEHORS)",
            r.place.to_uppercase(),
            r.party.len()
        )
    } else if let Some(b) = s.docked {
        format!(
            "AMARRE A {} ({})",
            s.bodies[b].name.to_uppercase(),
            s.faction
                .map_or("TERRITOIRE LIBRE".to_string(), |f| s.data.factions[f]
                    .name
                    .to_uppercase())
        )
    } else if let Some((b, _)) = s.parked {
        format!("POSE SUR {}", s.bodies[b].name.to_uppercase())
    } else if let Some(b) = s.ref_body {
        let alt = ((s.ship.pos - s.bodies[b].pos).length() - s.bodies[b].radius).max(0.0);
        format!(
            "EN VOL PRES DE {} (ALTITUDE {})",
            s.bodies[b].name.to_uppercase(),
            crate::hud::fmt_dist(alt)
        )
    } else {
        format!("EN VOL DANS LE SYSTEME {}", s.system.name.to_uppercase())
    };
    let st = &s.ship;
    let status = format!(
        "CREDITS {:.0}   CARBURANT {:.0} %   COQUE {:.0} %   SOUTE {:.1}/{:.1} T   EQUIPAGE {}",
        s.credits,
        100.0 * st.fuel / st.stats.fuel_cap.max(1.0),
        100.0 * st.hull / st.stats.hull_max.max(1.0),
        st.cargo.mass / 1000.0,
        st.cargo.cap_mass / 1000.0,
        s.crew.alive().count()
    );
    let mut todo: Vec<String> = Vec::new();
    let ready = s.readiness();
    if s.rogue.is_some() {
        todo.push("FLECHES : MARCHER ; FONCER SUR UN ETRE L'ATTAQUE ; ENTREE SUR < : REJOINDRE LE VAISSEAU.".into());
        todo.push("LE BUTIN ET LES RESSOURCES RAMASSEES PARTENT EN SOUTE AU RETOUR (PIECES DETACHEES, CARBURANT...).".into());
    }
    let grounded = s.parked.is_some() || ready.engine < 0.05 || ready.fuel <= 0.0;
    if s.docked.is_none() && !ready.can_fly() && grounded {
        todo.push(format!(
            "LE VAISSEAU NE PEUT PAS VOLER : MOTEUR {:.0} %, ENERGIE {:.0} %, CARBURANT {:.0} KG.",
            ready.engine * 100.0,
            ready.power * 100.0,
            ready.fuel
        ));
        todo.push(format!(
            "L'INGENIEUR REPARE AVEC LES PIECES DETACHEES DE LA SOUTE ({:.0} KG) ; LE CARBURANT DE LA SOUTE ({:.0} KG) SE VERSE AU RESERVOIR (I, F).",
            ready.parts_kg, ready.hold_fuel_kg
        ));
        todo.push("LES REPARATIONS PRENNENT DU TEMPS (L'INGENIEUR TRAVAILLE A SON QUART) : PGUP ACCELERE LE TEMPS EN VOL, OU CONTINUEZ A EXPLORER.".into());
        todo.push("PUIS ESPACE POUR DECOLLER. RIEN N'EST OBLIGATOIRE.".into());
    }
    if s.docked.is_some() {
        todo.push("A QUAI : ENTREE VENDRE LA SOUTE, F LE PLEIN, R REPARER, U DECOLLER.".into());
        if st.fuel < 0.9 * st.stats.fuel_cap {
            todo.push("LE RESERVOIR N'EST PAS PLEIN : F AU MARCHE.".into());
        }
        if st.hull < 0.95 * st.stats.hull_max {
            todo.push("LA COQUE EST ABIMEE : R AU MARCHE.".into());
        }
    } else if s.rogue.is_none() && ready.can_fly() {
        if s.target.is_none() {
            todo.push(
                "TAB CIBLE LE CORPS LE PLUS PROCHE ; M OUVRE LA CARTE ; G ENGAGE LE PILOTE AUTO."
                    .into(),
            );
        } else {
            todo.push("G : PILOTE AUTO VERS LA CIBLE ; D : AMARRER A UNE STATION PROCHE ; F8 : SURFACE D'UN MONDE.".into());
        }
        todo.push("L : GALAXIE ET SAUT FTL ; END : EXPLORER A PIED ; F2 : EMPIRE.".into());
    }
    todo.push(
        "1 A 8 : OUVRIR UN ESPACE ; 0 OU \\ : TABLEAU DE BORD ; ECHAP : REMONTER D'UN NIVEAU ; F1 : AIDE.".into(),
    );
    Orientation {
        place,
        status,
        todo,
    }
}

/// One line of live state next to the name of a space.
pub fn space_status(g: &Game, i: usize) -> String {
    let s = &g.sim;
    match i {
        PILOTAGE => {
            if s.docked.is_some() {
                "AMARRE".into()
            } else if s.rogue.is_some() {
                "EQUIPE A PIED".into()
            } else {
                let rv = s
                    .ref_body
                    .map_or(vector_core::Vec3::ZERO, |b| s.bodies[b].vel);
                format!("{:.0} M/S", (s.ship.vel - rv).length())
            }
        }
        NAVIGATION => format!("{} ETOILE(S) CONNUE(S)", s.visited.len()),
        VAISSEAU => format!(
            "COQUE {:.0} %  CARB {:.0} %",
            100.0 * s.ship.hull / s.ship.stats.hull_max.max(1.0),
            100.0 * s.ship.fuel / s.ship.stats.fuel_cap.max(1.0)
        ),
        EQUIPAGE => format!("{} PERSONNE(S)", s.crew.alive().count()),
        COMMERCE => if s.docked.is_some() {
            "OUVERT"
        } else {
            "A QUAI SEULEMENT"
        }
        .into(),
        EXPLORATION => if s.rogue.is_some() {
            "EN COURS"
        } else {
            "LIEUX A PORTEE"
        }
        .into(),
        COLONIES => format!("{} COLONIE(S)", s.colonies.len()),
        CODEX => format!(
            "{} TECHNOLOGIE(S)",
            s.researched.iter().filter(|r| **r).count()
        ),
        _ => String::new(),
    }
}

impl Game {
    /// Open the menu from the current screen (or close it again).
    pub fn open_hub(&mut self) {
        if self.screen == Screen::Hub {
            self.close_hub();
            return;
        }
        if matches!(
            self.screen,
            Screen::MainMenu | Screen::NewGame | Screen::Destroyed
        ) {
            return;
        }
        self.hub.from = self.screen;
        self.hub.page = false;
        // Opened from a screen of a space: that space is already selected.
        if let Place::Space(i) = place_of(self.screen) {
            self.hub.sel = i;
        }
        self.screen = Screen::Hub;
    }

    /// Leave the menu for the screen it was opened from.
    pub fn close_hub(&mut self) {
        let from = self.hub.from;
        self.hub_origin = None;
        self.screen = match from {
            Screen::Station if self.sim.docked.is_none() => Screen::Flight,
            Screen::Flight if self.sim.docked.is_some() => Screen::Station,
            Screen::Rogue if self.sim.rogue.is_none() => Screen::Flight,
            Screen::Hub => Screen::Flight,
            s => s,
        };
    }

    /// The menu is shown again as the page of `space` (Escape from a screen it opened).
    pub fn hub_return(&mut self, space: usize) {
        self.hub_origin = None;
        self.hub.sel = space;
        self.hub.page = true;
        self.screen = Screen::Hub;
    }

    fn hub_first_free(&self, space: usize) -> usize {
        SPACES[space]
            .entries
            .iter()
            .position(|e| unavailable(self, e).is_none())
            .unwrap_or(0)
    }

    pub fn hub_select(&mut self, space: usize) {
        self.hub.sel = space.min(SPACES.len() - 1);
        self.hub.page = true;
        self.hub.item = self.hub_first_free(self.hub.sel);
    }

    /// Run entry `item` of space `space`: the existing screen opens, or the reason is said.
    pub fn hub_activate(&mut self, space: usize, item: usize) {
        let Some(e) = SPACES.get(space).and_then(|s| s.entries.get(item)) else {
            return;
        };
        self.hub.sel = space;
        self.hub.item = item;
        if let Some(why) = unavailable(self, e) {
            self.say(why);
            return;
        }
        if self.sim.rogue.is_some() && e.target != Some(Screen::Rogue) {
            // Information screens opened while the squad walks come back to the walk.
            self.rogue_visit = true;
        }
        self.hub_origin = Some(space);
        (e.go)(self);
        if matches!(self.screen, Screen::Hub | Screen::Flight | Screen::Station) {
            // An action, or a refusal: nothing to come back from.
            self.hub_origin = None;
        }
        if self.screen == Screen::Flight || self.screen == Screen::Station {
            // The cockpit and the dock are roots: the menu closes.
        }
    }

    /// The dashboard, from the menu: the menu closes into it, and Escape goes back to the screen
    /// the menu was opened from.
    pub fn click_hub_dashboard(&mut self, _: usize) {
        let from = self.hub.from;
        self.screen = from;
        self.hub_origin = None;
        self.open_dashboard();
    }

    pub fn click_hub_space(&mut self, i: usize) {
        self.hub_select(i);
    }

    pub fn click_hub_entry(&mut self, code: usize) {
        self.hub_activate(code / 100, code % 100);
    }

    pub fn hub_key(&mut self, code: KeyCode) {
        let n = SPACES.len();
        let digit = match code {
            KeyCode::Digit1 | KeyCode::Numpad1 => Some(0),
            KeyCode::Digit2 | KeyCode::Numpad2 => Some(1),
            KeyCode::Digit3 | KeyCode::Numpad3 => Some(2),
            KeyCode::Digit4 | KeyCode::Numpad4 => Some(3),
            KeyCode::Digit5 | KeyCode::Numpad5 => Some(4),
            KeyCode::Digit6 | KeyCode::Numpad6 => Some(5),
            KeyCode::Digit7 | KeyCode::Numpad7 => Some(6),
            KeyCode::Digit8 | KeyCode::Numpad8 => Some(7),
            _ => None,
        };
        if let Some(d) = digit {
            self.hub_select(d);
            return;
        }
        if matches!(code, KeyCode::Digit0 | KeyCode::Numpad0) {
            self.click_hub_dashboard(0);
            return;
        }
        let m = SPACES[self.hub.sel].entries.len();
        match code {
            KeyCode::ArrowUp if self.hub.page => self.hub.item = (self.hub.item + m - 1) % m,
            KeyCode::ArrowDown if self.hub.page => self.hub.item = (self.hub.item + 1) % m,
            KeyCode::ArrowUp => self.hub.sel = (self.hub.sel + n - 1) % n,
            KeyCode::Tab => self.hub_select((self.hub.sel + 1) % n),
            KeyCode::ArrowDown => self.hub.sel = (self.hub.sel + 1) % n,
            KeyCode::ArrowRight if !self.hub.page => self.hub_select(self.hub.sel),
            KeyCode::Enter if !self.hub.page => self.hub_select(self.hub.sel),
            KeyCode::Enter | KeyCode::Space if self.hub.page => {
                self.hub_activate(self.hub.sel, self.hub.item)
            }
            KeyCode::ArrowLeft if self.hub.page => self.hub.page = false,
            KeyCode::Escape if self.hub.page => self.hub.page = false,
            KeyCode::Escape => self.close_hub(),
            _ => {}
        }
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    fb.text(2, 1, "== MENU PRINCIPAL ==", AMBER);
    let label = if g.hub.page {
        "RETOUR (ECHAP)"
    } else {
        "FERMER (ECHAP)"
    };
    button(fb, g, cols - 22, 1, label, Act::Key(KeyCode::Escape));
    // Orientation.
    let o = orientation(g);
    fb.text(2, 3, "VOUS ETES :", DIM);
    fb.text(14, 3, &o.place, AMBER);
    fb.text(2, 4, &o.status, GREEN);
    // What makes sense right now: clickable. Then the reminders of the keys.
    let mut row = 6;
    let banner_row = row;
    row += crate::context_menu::suggestions(g).len() + 2;
    fb.text(2, row, "RAPPELS :", DIM);
    for line in &o.todo {
        for (k, l) in wrap(line, cols - 24).iter().enumerate() {
            if k == 0 {
                fb.text(12, row, "-", DIM);
            }
            fb.text(14, row, l, DIM);
            row += 1;
        }
    }
    row += 1;
    // The eight spaces, and the page of the selected one.
    let top = row + 1;
    fb.text(2, top - 1, "ESPACES", DIM);
    for (i, sp) in SPACES.iter().enumerate() {
        let r = top + i * 2;
        let hover = g.hit_cells(2, r, 40, 1, Act::Call(Game::click_hub_space, i), None);
        let sel = i == g.hub.sel;
        let c = if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            2,
            r,
            &format!("{} {} {}", if sel { ">" } else { " " }, i + 1, sp.name),
            c,
        );
        fb.text(5, r + 1, &space_status(g, i), DIM);
    }
    let sp = &SPACES[g.hub.sel];
    let x = 46;
    fb.text(x, top - 1, sp.name, AMBER);
    for (k, l) in wrap(sp.blurb, cols - x - 3).iter().enumerate() {
        fb.text(x, top + k, l, DIM);
    }
    let first = top + 3;
    for (j, e) in sp.entries.iter().enumerate() {
        let r = first + j * 3;
        let why = unavailable(g, e);
        let sel = g.hub.page && j == g.hub.item;
        let hover = g.hit_cells(
            x,
            r,
            cols - x - 3,
            2,
            Act::Call(Game::click_hub_entry, g.hub.sel * 100 + j),
            None,
        );
        let c = if why.is_some() {
            DIM
        } else if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            x,
            r,
            &format!("{} {}", if sel { ">" } else { " " }, e.label),
            c,
        );
        fb.text(cols - 3 - e.key.chars().count(), r, e.key, DIM);
        match why {
            Some(w) => fb.text(x + 2, r + 1, w, RED),
            None => fb.text(x + 2, r + 1, e.desc, DIM),
        }
    }
    // Last, so that the zones of the spaces and of their entries keep their order.
    crate::context_menu::draw_banner(fb, g, 2, banner_row, cols - 24);
    button(
        fb,
        g,
        cols - 40,
        3,
        "TABLEAU DE BORD (0 OU \\)",
        Act::Call(Game::click_hub_dashboard, 0),
    );
}

/// The "MENU (F10)" button at the top right of every screen but the main menus.
pub fn draw_menu_button(fb: &mut Framebuffer, g: &Game) {
    if matches!(
        g.screen,
        Screen::MainMenu | Screen::NewGame | Screen::Hub | Screen::Destroyed
    ) {
        return;
    }
    let label = "MENU (F10)";
    let col = fb.cols().saturating_sub(label.len() + 1);
    let hover = g.hit_cells(col, 0, label.len(), 1, Act::Key(KeyCode::F10), None);
    // The button owns its cells: whatever the screen drew there is cleared first.
    let (x0, y0) = ((col * 8) as i32 - 2, 0);
    for y in y0..y0 + 10 {
        for x in x0..x0 + (label.len() * 8) as i32 + 4 {
            fb.px_set(x, y, 0);
        }
    }
    fb.text(col, 0, label, if hover { HOVER } else { AMBER });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_has_a_place_and_every_space_screen_has_an_entry() {
        for s in ALL_SCREENS {
            if let Place::Space(i) = place_of(s) {
                let listed = SPACES[i].entries.iter().any(|e| e.target == Some(s));
                assert!(
                    listed,
                    "{s:?} belongs to {} but has no entry",
                    SPACES[i].name
                );
            }
        }
        for (i, sp) in SPACES.iter().enumerate() {
            for e in sp.entries {
                if let Some(t) = e.target {
                    // The game's own menus (pause, options, rules) hang off one line.
                    let want = if t == Screen::Pause {
                        Place::System
                    } else {
                        Place::Space(i)
                    };
                    assert_eq!(place_of(t), want, "{} in {}", e.label, sp.name);
                }
            }
        }
        // No screen is offered twice: one system = one representation.
        let mut seen: Vec<Screen> = Vec::new();
        for sp in &SPACES {
            for e in sp.entries {
                if let Some(t) = e.target {
                    assert!(!seen.contains(&t), "{t:?} is listed twice");
                    seen.push(t);
                }
            }
        }
        assert_eq!(SPACES.len(), 8);
    }

    #[test]
    fn wrap_breaks_on_words() {
        assert_eq!(
            wrap("un deux trois quatre", 9),
            vec!["un deux", "trois", "quatre"]
        );
        assert!(wrap("", 10).is_empty());
    }
}
