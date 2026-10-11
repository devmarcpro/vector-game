//! Guided tutorial (state-driven, skippable) and contextual help (F1) per screen.
use crate::game::{Game, Screen};
use winit::keyboard::KeyCode;

/// One tutorial step: instruction text and the condition that completes it.
pub struct Step {
    pub text: &'static str,
    pub done: fn(&Game) -> bool,
}

fn pressed(g: &Game, screen: Screen, code: KeyCode) -> bool {
    g.tut_keys.iter().any(|&(s, c)| s == screen && c == code)
}

fn any_pressed(g: &Game, screen: Screen, codes: &[KeyCode]) -> bool {
    codes.iter().any(|&c| pressed(g, screen, c))
}

pub const STEPS: [Step; 16] = [
    Step {
        text: "Bienvenue, capitaine. Tournez le vaisseau : W/S tangage, A/D lacet, Q/E roulis.",
        done: |g| {
            any_pressed(
                g,
                Screen::Flight,
                &[
                    KeyCode::KeyW,
                    KeyCode::KeyA,
                    KeyCode::KeyS,
                    KeyCode::KeyD,
                    KeyCode::KeyQ,
                    KeyCode::KeyE,
                ],
            )
        },
    },
    Step {
        text: "Appuyez sur ESPACE pour pousser. Le carburant est precieux : coupez avant d'arriver.",
        done: |g| pressed(g, Screen::Flight, KeyCode::Space),
    },
    Step {
        text: "F active ou coupe l'assistance de vol. Essayez de la basculer.",
        done: |g| pressed(g, Screen::Flight, KeyCode::KeyF),
    },
    Step {
        text: "MAJ (maintenir) : boost. C (maintenir) : frein, annule la vitesse vers la cible. F7 : visee souris.",
        done: |g| any_pressed(g, Screen::Flight, &[KeyCode::ShiftLeft, KeyCode::KeyC, KeyCode::F7]),
    },
    Step {
        text: "Tab (ou Y) cible la station la plus proche. Le panneau de cible affiche distance et vitesse relative.",
        done: |g| g.sim.target.is_some() || g.sim.docked.is_some(),
    },
    Step {
        text: "M ouvre la carte du systeme ; +/- zoome. Appuyez de nouveau sur M pour la fermer.",
        done: |g| pressed(g, Screen::Flight, KeyCode::KeyM),
    },
    Step {
        text: "G engage le pilote automatique vers la cible. Il freine tout seul et s'arrete a l'arrivee.",
        done: |g| g.sim.autopilot.is_some() || g.sim.docked.is_some() || pressed(g, Screen::Flight, KeyCode::KeyG),
    },
    Step {
        text: "PgUp accelere le temps (jusqu'a x100000). Utile pour les longs trajets ; PgDn ralentit.",
        done: |g| g.timescale > 2.0 || g.sim.docked.is_some(),
    },
    Step {
        text: "Approchez a moins de 500 m et moins de 8 m/s de la station, puis D pour amarrer.",
        done: |g| g.sim.docked.is_some(),
    },
    Step {
        text: "A quai : Haut/Bas choisit une ressource, Entree vend tout, S vend 100 kg, B achete 100 kg.",
        done: |g| {
            any_pressed(g, Screen::Station, &[KeyCode::Enter, KeyCode::KeyS, KeyCode::KeyB])
        },
    },
    Step {
        text: "F fait le plein de carburant, R repare la coque. Pensez-y avant chaque depart.",
        done: |g| any_pressed(g, Screen::Station, &[KeyCode::KeyF, KeyCode::KeyR]),
    },
    Step {
        text: "H ouvre l'ecran du vaisseau : modules, energie, chaleur. Echap pour revenir.",
        done: |g| pressed(g, Screen::Station, KeyCode::KeyH) || g.screen == Screen::Ship,
    },
    Step {
        text: "C affiche les contrats de la station : livraisons emergeant des penuries reelles.",
        done: |g| pressed(g, Screen::Station, KeyCode::KeyC),
    },
    Step {
        text: "U decolle. Ciblez ensuite un asteroide (T) et maintenez V pour miner a moins de 2 km.",
        done: |g| g.sim.docked.is_none() && g.sim.credits.is_finite() && (pressed(g, Screen::Flight, KeyCode::KeyV) || pressed(g, Screen::Flight, KeyCode::KeyT)),
    },
    Step {
        text: "L ouvre la carte de la galaxie : Entree lance un saut si le vaisseau a un moteur FTL.",
        done: |g| pressed(g, Screen::Flight, KeyCode::KeyL) || g.screen == Screen::Galaxy,
    },
    Step {
        text: "F4 : technologies, F6 : codex, F2 : empire. F1 donne l'aide du contexte. Bon vol !",
        done: |g| any_pressed(g, Screen::Flight, &[KeyCode::F4, KeyCode::F6, KeyCode::F2]) || matches!(g.screen, Screen::Tech | Screen::Codex | Screen::Empire),
    },
];

/// Contextual help lines for a screen (what the player can do here).
pub fn help_lines(screen: Screen) -> &'static [&'static str] {
    match screen {
        Screen::Flight => &[
            "W/S A/D Q/E : tangage, lacet, roulis",
            "Espace / X : poussee avant / recul",
            "Fleches : RCS lateral et vertical",
            "F : assistance de vol   Maj : boost   C : frein sur la cible",
            "F7 : visee souris (tangage et lacet)",
            "Tab T Y R : cibles (proche, asteroide, station, PNJ)",
            "V : laser de minage   Ctrl : tir   B : missiles",
            "G : pilote auto   D : amarrer   M : carte systeme",
            "PgUp/PgDn : temps   P : pause   Echap : menu",
            "H vaisseau  K equipage  I soute  J journal",
            "L galaxie  F2 empire  F3 flotte  F4 techno  F6 codex",
            "O : fonder une colonie   Z tribut   N epave   U abordage",
        ],
        Screen::Station => &[
            "Haut/Bas : ressource   Entree : tout vendre",
            "S : vendre 100 kg   B : acheter 100 kg",
            "F : plein de carburant   R : reparer",
            "H : vaisseau   Y : chantier naval   N : recruter",
            "K : equipage   C : contrats   E : banque",
            "F2 : empire   F3 : flotte   U : decoller",
        ],
        Screen::Inventory => &["Haut/Bas : ressource   J : jeter   Echap : retour"],
        Screen::Ship => &[
            "Haut/Bas : emplacement   Entree : installer",
            "X : retirer le module   Y : chantier naval",
            "Surveillez energie, chaleur et sante des modules.",
        ],
        Screen::Shipyard => &[
            "Haut/Bas : coque   Entree : acheter / transferer",
            "Les coques verrouillees demandent une technologie.",
        ],
        Screen::Crew => &[
            "Gauche/Droite : poste   T : quart   X : debarquer",
            "J : journal   Les besoins (faim, soif, air) comptent.",
        ],
        Screen::Recruit => &["Haut/Bas : candidat   Entree : embaucher"],
        Screen::Journal => &["Haut/Bas ou PgUp/PgDn : defiler   Echap : retour"],
        Screen::Galaxy => &[
            "Haut/Bas : etoile   +/- : zoom   Entree : saut FTL",
            "Retour arriere : annuler une charge de saut",
            "La lettre est la classe stellaire (O B A F G K M D).",
        ],
        Screen::Contracts => &["Entree : accepter / livrer   X : abandonner"],
        Screen::Bank => &["Haut : emprunter 1000   Bas : rembourser 1000   Entree : tout"],
        Screen::Empire => &["Entree : detail de la colonie   F3 : flotte"],
        Screen::Colony => &[
            "Tab : zone   Haut/Bas : choix   Espace : on/off",
            "R recette   X demolir   Entree construire",
            "D deposer / W retirer (amarre)   T caisse   U decoller",
        ],
        Screen::Fleet => &[
            "Tab : liste   N : nouvelle route   A : affecter",
            "U : retirer   X : supprimer   B : acheter un navire",
        ],
        Screen::Tech => &["Haut/Bas : technologie   Entree : lancer la recherche"],
        Screen::Rules => &[
            "Gauche/Droite : modifier   R : valeurs par defaut",
            "Depuis la pause, les changements sont immediats.",
        ],
        Screen::Options => &["Gauche/Droite : modifier   Les choix sont sauvegardes."],
        Screen::Codex => &[
            "Gauche/Droite : categorie   Haut/Bas : entree",
            "PgUp/PgDn : sauter de 10   Echap ou F6 : retour",
        ],
        Screen::Pause => &["Haut/Bas : choisir   Entree : valider   Echap : reprendre"],
        Screen::MainMenu => &["Haut/Bas : choisir   Entree : valider   Taper : graine"],
        Screen::Destroyed => &["Entree : continuer (selon les regles de mort permanente)"],
    }
}

impl Game {
    /// Start the guided tutorial from step 0.
    pub fn start_tutorial(&mut self) {
        self.tut_step = Some(0);
        self.tut_keys.clear();
    }

    /// Record a key press for the tutorial and advance when the current step is complete.
    pub fn tutorial_tick(&mut self, key: Option<(Screen, KeyCode)>) {
        let Some(step) = self.tut_step else { return };
        if let Some(k) = key {
            self.tut_keys.push(k);
        }
        if step >= STEPS.len() {
            self.tut_step = None;
            return;
        }
        if (STEPS[step].done)(self) {
            self.tut_keys.clear();
            self.tut_step = if step + 1 < STEPS.len() {
                Some(step + 1)
            } else {
                self.say("TUTORIEL TERMINE");
                None
            };
        }
    }

    /// Text of the active tutorial step, with its number.
    pub fn tutorial_line(&self) -> Option<(usize, &'static str)> {
        self.tut_step
            .map(|i| (i, STEPS[i.min(STEPS.len() - 1)].text))
    }
}
