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

pub const STEPS: [Step; 17] = [
    Step {
        text: "Bienvenue, IA de bord. Pilotez le vaisseau : W/S tangage, A/D lacet, Q/E roulis.",
        done: |g| any_pressed(g, Screen::Flight, &g.settings.move_keys()),
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
        text: "Tab (ou Y) cible l'objet ou la station le plus proche, Maj+Tab une planete. Le panneau de cible affiche distance et vitesse relative.",
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
        text: "L ouvre la carte de la galaxie : Entree lance un saut (le vaisseau s'eloigne seul des corps massifs puis saute ; G annule).",
        done: |g| pressed(g, Screen::Flight, KeyCode::KeyL) || g.screen == Screen::Galaxy,
    },
    Step {
        text: "F4 : technologies, F6 : codex, F2 : empire. F1 donne l'aide du contexte. Bon vol !",
        done: |g| any_pressed(g, Screen::Flight, &[KeyCode::F4, KeyCode::F6, KeyCode::F2]) || matches!(g.screen, Screen::Tech | Screen::Codex | Screen::Empire),
    },
    Step {
        text: "F8 ouvre la surface d'un monde releve (G : globe) ; E y lance une expedition a pied ; Entree en vol sur une planete ciblee parle a son peuple.",
        done: |g| pressed(g, Screen::Flight, KeyCode::F8) || g.screen == Screen::Surface,
    },];

/// One-line summary of the keys that work right now, for the permanent command bar.
/// It follows the screen and its sub-state (map, module picker, jump charge, autopilot...).
pub fn hint_line(g: &Game) -> String {
    if g.console.open {
        return "ENTREE EXECUTER  ~ FERMER LA CONSOLE  (HELP POUR LA LISTE)".into();
    }
    let s = match g.screen {
        Screen::MainMenu => "HAUT/BAS CHOISIR  ENTREE VALIDER  TAPER UNE GRAINE AVANT NOUVELLE PARTIE",
        Screen::Flight if g.map => {
            "M FERMER LA CARTE  +/- ZOOM  C CENTRE ETOILE/VAISSEAU  TAB CIBLE  G AUTO  PGUP/PGDN TEMPS  F1 AIDE"
        }
        Screen::Flight if g.sim.is_charging_jump() => {
            "SAUT EN CHARGE  RETOUR ARRIERE ANNULER  PGUP/PGDN TEMPS  F1 AIDE"
        }
        Screen::Flight if g.sim.in_transit() => "TRANSIT FTL  P PAUSE  F1 AIDE",
        Screen::Flight if g.sim.autopilot.is_some() => {
            "PILOTE AUTO ACTIF  G ANNULER  PGUP/PGDN TEMPS  TAB CIBLE  M CARTE  P PAUSE  F1 AIDE"
        }
        Screen::Flight => {
            "WASD/QE ROT  ESP/X POUSSEE  MAJ BOOST  C FREIN  F ASSIST  TAB CIBLE  G AUTO  D AMARRER  V LASER  CTRL TIR  M CARTE  F1 AIDE"
        }
        Screen::Station => {
            "ENTREE VENDRE  F PLEIN  R REPARER  FIN VISITER LA STATION (TOUS LES SERVICES)  H NAVIRE  T BOURSE  U DECOLLER"
        }
        Screen::Inventory => "HAUT/BAS CHOISIR  J JETER  I/ECHAP RETOUR",
        Screen::Pause => "HAUT/BAS CHOISIR  ENTREE VALIDER  ECHAP REPRENDRE",
        Screen::Ship if g.picker => {
            "HAUT/BAS MODULE  ENTREE INSTALLER OU REMPLACER (ACHAT AUTO)  ECHAP ANNULER"
        }
        Screen::Ship if g.sim.docked.is_some() => {
            "HAUT/BAS EMPLACEMENT  ENTREE INSTALLER/REMPLACER  X RETIRER  Y CHANTIER NAVAL  H/ECHAP RETOUR"
        }
        Screen::Ship => "HAUT/BAS EMPLACEMENT  H/ECHAP RETOUR",
        Screen::Shipyard => "HAUT/BAS COQUE  ENTREE ACHETER (REPRISE 50%)  ECHAP RETOUR",
        Screen::Crew if g.gear_mode => {
            "FLECHES CHOISIR UN EQUIPEMENT  ENTREE EQUIPER (IL FAUT L'OBJET EN SOUTE)  U RETIRER  E/ECHAP RETOUR A LA LISTE"
        }
        Screen::Crew if g.organ_mode => {
            "HAUT/BAS ORGANE  ENTREE GREFFE  P PROTHESE  H PRELEVER  </> IMPLANT  I POSER  U RETIRER  K CLONER  M MUTAGENE  O/ECHAP"
        }
        Screen::Crew => {
            "HAUT/BAS PERSONNE  </> POSTE  T QUART  X DEBARQUER  N RECRUTER  J JOURNAL  G ARBRE  V RESEAU  O ORGANES  E EQUIP.  K RETOUR"
        }
        Screen::Recruit => "HAUT/BAS CANDIDAT  ENTREE EMBAUCHER  K EQUIPAGE  ECHAP RETOUR",
        Screen::Journal => {
            "HAUT/BAS DEFILER  1-3 REPONDRE A UNE SURPRISE  K EQUIPAGE  J/ECHAP RETOUR  TAB NOUVELLES"
        }
        Screen::Expedition => "HAUT/BAS CHOISIR  ESPACE COCHER  ENTREE PARTIR  ECHAP RETOUR",
        Screen::Dialogue => "1-6 OU CLIC REPONDRE  HAUT/BAS + ENTREE  ECHAP COUPER LA COMMUNICATION",
        Screen::Proto => "CLIC SUR LES ELEMENTS  GAUCHE/DROITE OU 1-2 PROTOTYPE  F12/ECHAP RETOUR",
        Screen::Factions => "GAUCHE/DROITE FACTION  CLIC : CHOISIR  TAB RELATIONS  X/ECHAP RETOUR",
        Screen::Galaxy => {
            "HAUT/BAS ETOILE  +/- ZOOM  ENTREE SAUT FTL  L/ECHAP RETOUR  (RETOUR ARRIERE ANNULE UNE CHARGE)"
        }
        Screen::Contracts => "HAUT/BAS CONTRAT  ENTREE ACCEPTER/LIVRER  X ABANDONNER  ECHAP RETOUR",
        Screen::Bank => {
            "HAUT EMPRUNTER 1000  BAS REMBOURSER 1000  ENTREE TOUT REMBOURSER  ECHAP RETOUR"
        }
        Screen::Livestock => {
            "HAUT/BAS ANIMAL  GAUCHE/DROITE ESPECE  B ACHETER  P REPRODUIRE  H ABATTRE  ECHAP RETOUR"
        }
        Screen::Trading => {
            "GAUCHE/DROITE ONGLET  HAUT/BAS CHOIX  +/- MISE  ENTREE AGIR  ECHAP RETOUR"
        }
        Screen::Rogue => {
            "FLECHES/ZQSD DEPLACER (FONCER = ATTAQUER)  CLIC MARCHER  ENTREE ESCALIER/SITE  V DRONE  R RAPPEL  TAB CHEF"
        }
        Screen::Cosmos => {
            "ENTREE PORTAIL  A ANCRE  R REMONTER  X LEVER  HAUT/BAS CHOIX  +/- JOURS  S STASE  ECHAP RETOUR"
        }
        Screen::Casino => {
            "GAUCHE/DROITE JEU  HAUT/BAS MISE  +/- NUMERO  ENTREE JOUER  ECHAP RETOUR"
        }
        Screen::Empire => "HAUT/BAS COLONIE  ENTREE DETAIL  F3 FLOTTE  ECHAP RETOUR",
        Screen::Colony => {
            "TAB ZONE  ESP ON/OFF  R RECETTE  X DEMOLIR  D DEPOSER  W RETIRER  </> POLITIQUE  ENTREE BATIR  T CAISSE  U DECOLLER  F CHAINES"
        }
        Screen::Fleet => {
            "TAB LISTE  N ROUTE  A AFFECTER  U RETIRER  X SUPPRIMER  B ACHETER  M CARTE  ECHAP RETOUR"
        }
        Screen::Destroyed => "ENTREE CONTINUER",
        Screen::Tech => "FLECHES GRAPHE (GAUCHE PREREQUIS, DROITE SUITE)  TAB RECHERCHE OU SUIVANTE  ENTREE RECHERCHER  ECHAP RETOUR",
        Screen::Rules => "HAUT/BAS REGLE  GAUCHE/DROITE MODIFIER  R DEFAUTS  ECHAP RETOUR",
        Screen::Options => "HAUT/BAS REGLAGE  GAUCHE/DROITE CHANGER  ENTREE/ECHAP RETOUR",
        Screen::Codex => "GAUCHE/DROITE CATEGORIE  HAUT/BAS ENTREE  ECHAP RETOUR",
        Screen::Family => "GAUCHE/DROITE PERSONNE SUIVANTE  TAB ARBRE/GENES  G/K/ECHAP RETOUR A L'EQUIPAGE",
        Screen::Bourse => {
            "HAUT/BAS RESSOURCE  TAB PERIODE  B HAUSSE  N BAISSE  C CLORE  E ENTREPRISES (ACTIONS)  T/ECHAP RETOUR"
        }
        Screen::Surface if g.surf_globe => {
            "GLOBE  GAUCHE/DROITE TOURNER  TAB COUCHE  G CARTE  F CODEX  F8/ECHAP RETOUR"
        }
        Screen::Surface => {
            "FLECHES CURSEUR (MAJ x4)  TAB COUCHE  G GLOBE  + ZOOM (A PIED)  F CODEX  F8/ECHAP RETOUR"
        }
    };
    s.to_string()
}

/// Contextual help lines for a screen (what the player can do here).
pub fn help_lines(screen: Screen) -> &'static [&'static str] {
    match screen {
        Screen::Flight => &[
            "W/S A/D Q/E : tangage, lacet, roulis",
            "Espace / X : poussee avant / recul",
            "Fleches : RCS lateral et vertical",
            "F : assistance de vol   Maj : boost   C : frein sur la cible",
            "F7 : visee souris (tangage et lacet)",
            "F11 : plier ou deplier le menu de pilotage (clic : menus et commandes)",
            "Tab T Y R : cibles (proche, asteroide, station, PNJ)",
            "V : laser de minage   Ctrl : tir   B : missiles",
            "G : pilote auto   D : amarrer   M : carte systeme",
            "PgUp/PgDn : temps   P : pause   Echap : menu",
            "H vaisseau  K equipage  I soute  J journal  X factions",
            "L galaxie  F2 empire  F3 flotte  F4 techno  F6 codex  Entree : appeler le vaisseau cible",
            "O : fonder une colonie   Z tribut   N epave   U abordage",
            "F8 : surface de la planete ou lune ciblee (releve niveau 2)",
            "Souris : clic = cibler un objet, double clic = y aller ; molette = temps ; clic droit = retour",
        ],
        Screen::Station => &[
            "Haut/Bas : ressource   Entree : tout vendre",
            "S : vendre 100 kg   B : acheter 100 kg",
            "F : plein de carburant   R : reparer",
            "H : vaisseau   Y : chantier naval   N : recruter",
            "K : equipage   C : contrats   E : banque   T : bourse (courbes de prix)",
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
            "V : reseau des relations (amis, rivaux, famille)",
            "E : equipement (arme, armure, outil pris en soute ; Entree equipe, U retire)",
            "O : organes (Entree greffe, P prothese, H prelever, </> choisir un implant, I poser, U retirer, K cloner l'organe a l'hopital, M mutagene ; piece en soute)",
        ],
        Screen::Recruit => &["Haut/Bas : candidat   Entree : embaucher"],
        Screen::Journal => &[
            "Haut/Bas ou PgUp/PgDn : defiler   Echap : retour",
            "Une surprise qui attend un choix s'affiche en bas : touches 1 a 3 pour repondre",
            "Elles naissent du croisement des systemes (guerre, cartels, relations, anomalies...)",
        ],
        Screen::Expedition => &["Cochez qui descend et le rover, Entree pour partir", "Il faut une case de terre et d'etre pres de la planete"],
        Screen::Dialogue => &[
            "Entree en vol : appeler le vaisseau cible (Tab/T/Y/R pour cibler)",
            "Leur ton depend des moeurs de leur faction ; les reponses aussi",
            "Cadeaux, nouvelles, tuyaux, corruption, bluff... Echap coupe",
        ],
        Screen::Proto => &["Prototypes d'interface : le corps (1) et le hall de la station (2)", "Tout est clicable ; rien n'est encore branche"],
        Screen::Factions => &[
            "Gauche/Droite ou clic : choisir une faction",
            "Lignes vertes : allies   lignes rouges : ennemis",
            "A droite : moeurs (8 axes), interdits moraux, contrebande, reputation",
            "Tab : registre des relations (factions, peuples, cartels, contacts, entreprises)",
            "Ce que vous faites a l'un se repercute, attenue, sur ses allies et ses rivaux",
            "X ou Echap : retour",
        ],
        Screen::Galaxy => &[
            "Haut/Bas : etoile   +/- : zoom   Entree : saut FTL",
            "Retour arriere : annuler une charge de saut",
            "La lettre est la classe stellaire (O B A F G K M D).",
        ],
        Screen::Contracts => &["Entree : accepter / livrer   X : abandonner"],
        Screen::Bank => &["Haut : emprunter 1000   Bas : rembourser 1000   Entree : tout"],
        Screen::Livestock => &[
            "B : acheter un animal   P : reproduire avec le meilleur partenaire",
            "H : abattre (rendement selon les genes)   les animaux mangent des rations",
        ],
        Screen::Trading => &[
            "Gauche/Droite : onglet (rumeurs, arbitrage, cartels, appels d'offres, assurance)",
            "Entree : agir   +/- : mise de l'offre",
        ],
        Screen::Rogue => &[
            "Fin : explorer a pied un lieu a portee (station, epave, ruine, grotte, portail...)",
            "Fleches ou pave numerique : deplacer ; foncer sur un monstre l'attaque ; clic : marcher jusque-la",
            "Entree sur < sort (butin a bord) ou sur > descend ; sur un site (ruine, grotte, epave) elle y descend",
            "V : lance un drone (robotique) qui ramasse a proximite ; R : rappelle l'equipe depuis la surface",
        ],
        Screen::Cosmos => &[
            "Entree : franchir le portail   A : poser une ancre temporelle   R : remonter",
            "S : stase a quai (le monde avance, pas le bord)   Accueil : ouvrir cet ecran",
        ],
        Screen::Casino => &["Entree : jouer   la maison garde toujours un avantage"],
        Screen::Empire => &["Entree : detail de la colonie   F3 : flotte"],
        Screen::Colony => &[
            "Tab : zone   Haut/Bas : choix   Espace : on/off",
            "R recette   X demolir   Entree construire",
            "D deposer / W retirer (amarre)   T caisse   U decoller",
            "F : schema des chaines de production",
        ],
        Screen::Fleet => &[
            "Tab : liste   N : nouvelle route   A : affecter",
            "U : retirer   X : supprimer   B : acheter un navire",
            "M : carte des colonies et des routes",
        ],
        Screen::Tech => &[
            "Graphe : une case par technologie, colonnes = profondeur des prerequis",
            "Fleches : Gauche = un prerequis, Droite = ce qu'elle ouvre, Haut/Bas = voisin",
            "Tab : recherche en cours ou technologie suivante a portee",
            "Entree : lancer la recherche   Echap ou F4 : retour",
            "Vert : a portee  Ambre : en cours  Sombre : acquise  Rouge : verrouillee",
        ],
        Screen::Rules => &[
            "Gauche/Droite : modifier   R : valeurs par defaut",
            "Depuis la pause, les changements sont immediats.",
        ],
        Screen::Options => &["Gauche/Droite : modifier   Les choix sont sauvegardes."],
        Screen::Codex => &[
            "Gauche/Droite : categorie   Haut/Bas : entree",
            "PgUp/PgDn : sauter de 10   Echap ou F6 : retour",
        ],
        Screen::Family => &[
            "Arbre : grands-parents (couples), parents, fratrie, oncles et tantes avec leurs enfants",
            "Pourcentage : ressemblance genetique avec la personne",
            "Ligne ambre : parent a bord ; + age : decede",
            "Tab : bascule entre l'arbre et les genes",
            "Gauche/Droite : personne suivante   G, K ou Echap : retour",
            "Page des genes : porteur (x1.5) ou atteint (x4) pour les predispositions",
        ],
        Screen::Bourse => &[
            "Haut/Bas : ressource   PgUp/PgDn : sauter de 10",
            "B / N : contrat a terme a la hausse / baisse (100 kg, 30 j, marge 25 %)",
            "C : clore un contrat (gain ou perte au prix de reference du jour)",
            "E : entreprises cotees ; B achete 10 actions, N en vend 10 ; dividende chaque mois",
            "Tab ou Gauche/Droite : periode du graphique (30, 90, 180 jours)",
            "Courbe pleine : prix de reference ; ligne ambre : moyenne sur 7 jours",
            "Bandes colorees : evenements du monde qui touchent cette ressource",
            "T ou Echap : retour a la station",
        ],
        Screen::Surface => &[
            "Fleches : deplacer le curseur (Maj : x4)",
            "Tab : couche (biomes, altitude, temperature, humidite, meteo)",
            "G : globe fil de fer avec courbes de niveau (Gauche/Droite : tourner)",
            "F : codex   F8 ou Echap : retour au vol",
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
    pub fn tutorial_line(&self) -> Option<(usize, String)> {
        self.tut_step.map(|i| {
            (
                i,
                self.settings.localize(STEPS[i.min(STEPS.len() - 1)].text),
            )
        })
    }
}
