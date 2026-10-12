//! The customisation screens of the new-game flow: the ship (a scenario model, a catalogue hull
//! with free fittings, or a full design within a budget), the crew member by member, the ship's
//! AI, the world, the rule categories and the presets. Every screen is a list of rows (a label,
//! a value, `<` `>` and a random button); what a row changes is the `CustomStart` of
//! `vector_sim::start_custom`, which builds the real starting `Sim`. Nothing is cosmetic.
use crate::game::{Game, Screen};
use crate::hub::wrap;
use crate::hud::HOVER;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use std::path::PathBuf;
use std::sync::Arc;
use vector_core::mix64;
use vector_render::Framebuffer;
use vector_sim::start;
use vector_sim::start_custom::{self as sc, ShipMode};

pub const S_SHIP: usize = 2;
pub const S_CREW: usize = 3;
pub const S_AI: usize = 4;
pub const S_SEED: usize = 5;
pub const S_WORLD: usize = 6;
pub const S_OPTIONS: usize = 7;
pub const S_RECAP: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `<` and `>` change the value.
    Pick,
    /// Typing edits the value.
    Text,
    /// Activating does something.
    Action,
    /// Only information.
    Info,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub id: u32,
    pub label: String,
    pub value: String,
    pub hint: String,
    pub kind: Kind,
}

fn row(id: u32, label: impl Into<String>, value: impl Into<String>, hint: &str, kind: Kind) -> Row {
    Row {
        id,
        label: label.into(),
        value: value.into(),
        hint: hint.to_string(),
        kind,
    }
}

// Row ids.
const S_MODE: u32 = 0;
const S_MODEL: u32 = 1;
const S_HULL: u32 = 2;
const S_PLAN: u32 = 3;
const S_EDIT: u32 = 4;
const S_EXPORT: u32 = 5;
const S_NAME: u32 = 10;
const S_REG: u32 = 11;
const S_COMPANY: u32 = 12;
const S_BUDGET: u32 = 13;
const S_SLOT: u32 = 100;

const C_TAB: u32 = 0;
const C_POINTS: u32 = 1;
const C_NEWPOOL: u32 = 2;
const C_REROLL_ALL: u32 = 3;
const C_NAME: u32 = 10;
const C_CULTURE: u32 = 11;
const C_AGE: u32 = 12;
const C_ROLE: u32 = 13;
const C_BG: u32 = 14;
const C_SALARY: u32 = 15;
const C_IMPLANT: u32 = 16;
const C_REROLL: u32 = 17;
const C_LOCK: u32 = 18;
const C_RESET: u32 = 19;
const C_CRYO: u32 = 40;
const C_WAKE_ARR: u32 = 41;
const C_WAKE_ALERT: u32 = 42;
const C_WAKE_DAY: u32 = 43;
const C_TRAIT_ADD: u32 = 20;
const C_SKILL: u32 = 100;
const C_TRAIT: u32 = 200;
const C_CT_ADD: u32 = 30;
const C_CT_DEL: u32 = 31;
const C_CT: u32 = 300;

const A_NAME: u32 = 0;
const A_PERS: u32 = 1;
const A_CORE: u32 = 2;
const A_POINTS: u32 = 3;
const A_ALLOC_RESET: u32 = 4;
const A_ALLOC: u32 = 10;
const A_SW: u32 = 100;

const W_SYSTEM: u32 = 0;
const W_PLACE: u32 = 1;
const W_EPI: u32 = 2;
const W_SUR: u32 = 3;
const W_PIR: u32 = 4;
const W_EVT: u32 = 5;
const W_PROV: u32 = 6;

const O_LANG: u32 = 0;
const O_KB: u32 = 1;
const O_TUTO: u32 = 2;
const O_DIFF: u32 = 3;
const O_PRESET: u32 = 4;
const O_SAVE: u32 = 20;
const O_ADV: u32 = 21;
const O_RULE: u32 = 100;

const R_RANDOM: u32 = 0;
const R_NAME: u32 = 1;
const R_SAVE: u32 = 2;
const R_LOAD: u32 = 3;
const R_EXPORT: u32 = 4;
const R_IMPORT: u32 = 5;

/// Rules in the order the options screen shows them, by category.
const RULE_ORDER: [usize; 14] = [1, 0, 2, 3, 6, 4, 5, 12, 7, 8, 9, 10, 11, 13];
const RULE_HINTS: [&str; 14] = [
    "0 = LE VAISSEAU NE BRULE RIEN ; 3 = TRES GOURMAND. AGIT SUR CHAQUE POUSSEE.",
    "OUI : FAIM, SOIF ET AIR COMPTENT. NON : L'EQUIPAGE N'A BESOIN DE RIEN.",
    "OUI : UN VAISSEAU DETRUIT MET FIN A LA PARTIE.",
    "MULTIPLICATEUR DES DEGATS QUE VOUS SUBISSEZ (COMBAT, IMPACTS, FEU).",
    "MULTIPLICATEUR DU PRIX DES VAISSEAUX, MODULES ET MATERIAUX DE CONSTRUCTION.",
    "MULTIPLICATEUR DES POINTS DE RECHERCHE.",
    "0 = GALAXIE PACIFIQUE ; 3 = LES PIRATES ET LES PATROUILLES VOUS CHERCHENT.",
    "FREQUENCE DES EVENEMENTS (EQUIPAGE, ERUPTIONS, METEORITES, INTRUSIONS).",
    "OUI : TOUTE LA GALAXIE EST CONNUE DES LE DEPART.",
    "OUI : CARBURANT, MUNITIONS ET ACHATS NE COUTENT RIEN.",
    "OUI : RIEN NE PEUT ENDOMMAGER LE VAISSEAU.",
    "OUI : LA CONSOLE (TELEPORT, CREATION, ACCELERATION) EST PERMISE.",
    "PLUS HAUT FACTEUR DE TEMPS DISPONIBLE.",
    "OUI : COQUES, MODULES ET BATIMENTS DEMANDENT LEUR TECHNOLOGIE.",
];
const SAVE_HINTS: [&str; 3] = [
    "SAUVEGARDE LIBRE, A TOUT MOMENT.",
    "ON NE SAUVEGARDE QU'AMARRE A UNE STATION.",
    "UN SEUL EMPLACEMENT, ET SEULEMENT AMARRE : CHAQUE CHOIX COMPTE.",
];
const LADDER: [f64; 8] = [0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0];

fn ladder(v: f64, up: bool) -> f64 {
    let i = LADDER
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - v).abs().total_cmp(&(b.1 - v).abs()))
        .map_or(3, |(i, _)| i);
    LADDER[if up {
        (i + 1).min(7)
    } else {
        i.saturating_sub(1)
    }]
}

fn cycle(i: usize, n: usize, part: usize, h: u64) -> usize {
    if n == 0 {
        return 0;
    }
    match part {
        0 => (i + n - 1) % n,
        3 => (mix64(h) % n as u64) as usize,
        _ => (i + 1) % n,
    }
}

fn x_text(x: f64) -> String {
    format!("x{x:.2}")
}

fn presets_dir() -> PathBuf {
    if cfg!(test) {
        std::env::temp_dir().join("vector_test_presets")
    } else {
        PathBuf::from("saves").join("presets")
    }
}

fn list_presets() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(presets_dir())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".ron"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// One entry of the list of plans a design can start from.
#[derive(Clone, Debug)]
pub enum PlanSrc {
    Sale(usize),
    File(String),
}

// ---- rows of each step ------------------------------------------------------------------

impl Game {
    pub fn ng_rows(&self) -> Vec<Row> {
        match self.ng.step {
            S_SHIP => self.ship_rows(),
            S_CREW => self.crew_rows(),
            S_AI => self.ai_rows(),
            S_WORLD => self.world_rows(),
            S_OPTIONS => self.options_rows(),
            S_RECAP => self.recap_rows(),
            _ => Vec::new(),
        }
    }

    pub fn ng_plan_list(&self) -> Vec<(String, PlanSrc)> {
        let mut v: Vec<(String, PlanSrc)> = self
            .data
            .ship_plans
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    format!("{} (EN VENTE)", p.name.to_uppercase()),
                    PlanSrc::Sale(i),
                )
            })
            .collect();
        for f in &self.ng.plan_files {
            v.push((
                format!("FICHIER {}", f.to_uppercase()),
                PlanSrc::File(f.clone()),
            ));
        }
        v
    }

    fn ship_rows(&self) -> Vec<Row> {
        let c = &self.ng.custom.ship;
        let d = &self.data;
        let sc_ = &d.scenarios[self.ng.scenario];
        let mut v = vec![row(
            S_MODE,
            "TYPE DE VAISSEAU",
            match c.mode {
                ShipMode::Scenario => "MODELE DU SCENARIO",
                ShipMode::Catalog => "COQUE DU CATALOGUE + EQUIPEMENT LIBRE",
                ShipMode::Design => "CONCEPTION COMPLETE",
            },
            "MODELE : LE DEPART CLASSIQUE. CATALOGUE : UNE COQUE ET VOS MODULES. CONCEPTION : VOTRE PLAN, DANS UN BUDGET.",
            Kind::Pick,
        )];
        match c.mode {
            ShipMode::Scenario => {
                let h = d.hull_index(&sc_.ships[self.ng.ship].0).unwrap_or(0);
                v.push(row(
                    S_MODEL,
                    "MODELE",
                    d.hulls[h].name.to_uppercase(),
                    "LES MODELES PROPOSES PAR CE SCENARIO.",
                    Kind::Pick,
                ));
            }
            ShipMode::Catalog => {
                let hi = d.hull_index(&c.hull).unwrap_or(0);
                v.push(row(
                    S_HULL,
                    "COQUE",
                    d.hulls[hi].name.to_uppercase(),
                    "CHANGER DE COQUE REMONTE L'EQUIPEMENT LA OU IL TIENT.",
                    Kind::Pick,
                ));
                for (i, (ty, size)) in d.hulls[hi].slots.iter().enumerate() {
                    let m = c.modules.get(i).filter(|m| !m.is_empty());
                    let (val, hint) = match m.and_then(|m| d.module_index(m)) {
                        Some(mi) => (
                            d.modules[mi].name.to_uppercase(),
                            format!(
                                "{:.0} CREDITS - MASSE {:.0} KG",
                                d.modules[mi].cost, d.modules[mi].mass
                            ),
                        ),
                        None => (
                            "VIDE".to_string(),
                            "UN EMPLACEMENT VIDE NE COUTE RIEN.".to_string(),
                        ),
                    };
                    v.push(row(
                        S_SLOT + i as u32,
                        format!("{:>2} {} T{}", i + 1, ty.label(), size),
                        val,
                        &hint,
                        Kind::Pick,
                    ));
                }
            }
            ShipMode::Design => {
                let list = self.ng_plan_list();
                let cur = list
                    .get(self.ng.plan_pick)
                    .map_or("(AUCUN)".to_string(), |p| p.0.clone());
                v.push(row(
                    S_PLAN,
                    "PLAN A CHARGER",
                    cur,
                    "PLANS EN VENTE ET FICHIERS DE saves/plans : < > CHARGE LE PLAN.",
                    Kind::Pick,
                ));
                v.push(row(
                    S_EDIT,
                    "EDITEUR DE PIECES",
                    "OUVRIR >",
                    "L'ATELIER COMPLET : PIECES, GRILLE 3D, MODULES. ECHAP REVIENT ICI.",
                    Kind::Action,
                ));
                v.push(row(
                    S_EXPORT,
                    "EXPORTER LE PLAN",
                    "ECRIRE >",
                    "ECRIT LE PLAN DANS saves/plans POUR LE PARTAGER.",
                    Kind::Action,
                ));
            }
        }
        let gen = |s: &str, f: &str| {
            if s.is_empty() {
                format!("({f})")
            } else {
                s.to_string()
            }
        };
        v.push(row(
            S_NAME,
            "NOM DU VAISSEAU",
            gen(&c.name, "TIRE DE LA GRAINE"),
            "TAPEZ POUR ECRIRE.",
            Kind::Text,
        ));
        v.push(row(
            S_REG,
            "IMMATRICULATION",
            gen(&c.registration, "TIREE"),
            "TAPEZ POUR ECRIRE.",
            Kind::Text,
        ));
        v.push(row(
            S_COMPANY,
            "COMPAGNIE",
            gen(&c.company, "TIREE"),
            "TAPEZ POUR ECRIRE.",
            Kind::Text,
        ));
        if c.mode != ShipMode::Scenario {
            let auto = sc::ship_budget(
                d,
                sc_,
                &self.pending_rules,
                &sc::ShipCustom {
                    budget: None,
                    ..c.clone()
                },
            );
            v.push(row(
                S_BUDGET,
                "BUDGET DU VAISSEAU",
                match c.budget {
                    None => format!("AUTO {auto:.0} CR"),
                    Some(b) => format!("{b:.0} CR"),
                },
                "BORNE LE COUT REEL (MATERIAUX + TRAVAIL + MODULES). AUTO : SUIT LA DIFFICULTE. ENTREE DE LA LIGNE : REMET AUTO.",
                Kind::Pick,
            ));
        }
        v
    }

    fn crew_rows(&self) -> Vec<Row> {
        let d = &self.data;
        let cc = &self.ng.custom.crew;
        let mut v = vec![row(
            C_TAB,
            "ONGLET",
            if self.ng.crew_tab == 0 {
                "EQUIPAGE"
            } else {
                "CONTACTS"
            },
            "EQUIPAGE : LA PERSONNE SOUS LE CURSEUR. CONTACTS : AMIS ET RIVAUX DE DEPART.",
            Kind::Pick,
        )];
        if self.ng.crew_tab == 1 {
            for (i, c) in cc.contacts.iter().enumerate() {
                let b = C_CT + 3 * i as u32;
                v.push(row(b, format!("CONTACT {} NATURE", i + 1), if c.friend { "AMI" } else { "RIVAL" }, "UN AMI DONNE UNE CONFIANCE ET UN PEU DE REPUTATION ; UN RIVAL ABAISSE LA REPUTATION.", Kind::Pick));
                let f = d
                    .factions
                    .get(c.faction)
                    .map_or("?".into(), |f| f.name.to_uppercase());
                v.push(row(
                    b + 1,
                    "   FACTION",
                    f,
                    "UN SEUL CONTACT PAR FACTION.",
                    Kind::Pick,
                ));
                let n = if c.name.is_empty() {
                    "(SANS NOM)".to_string()
                } else {
                    c.name.to_uppercase()
                };
                v.push(row(b + 2, "   NOM", n, "TAPEZ POUR ECRIRE.", Kind::Text));
            }
            v.push(row(
                C_CT_ADD,
                "AJOUTER UN CONTACT",
                format!("{} / {}", cc.contacts.len(), sc::MAX_CONTACTS),
                "",
                Kind::Action,
            ));
            v.push(row(C_CT_DEL, "RETIRER LE DERNIER", "", "", Kind::Action));
            return v;
        }
        let used = self.ng_points_used();
        let budget = sc::crew_points(d, &self.ng_config());
        v.push(row(
            C_POINTS,
            "POINTS D'EQUIPAGE",
            format!("{used} / {budget}"),
            "LE BUDGET DE POINTS DU SCENARIO ; < > LE CHANGE (BAC A SABLE).",
            Kind::Pick,
        ));
        v.push(row(
            C_NEWPOOL,
            "NOUVEAUX CANDIDATS",
            format!("LOT {}", cc.salt + 1),
            "TIRE UN AUTRE LOT DE 12 CANDIDATS POUR CETTE GRAINE.",
            Kind::Action,
        ));
        v.push(row(
            C_REROLL_ALL,
            "RELANCER L'EQUIPE",
            "TIRER >",
            "REMPLACE CHAQUE MEMBRE NON VERROUILLE PAR UN AUTRE CANDIDAT.",
            Kind::Action,
        ));
        let k = self.ng.cursor;
        let Some(p) = self.ng.pool.get(k) else {
            return v;
        };
        let e = cc.edit_of(k).cloned().unwrap_or_default();
        let pick = self.ng.picks.iter().find(|q| q.candidate == k);
        let (sk_spent, tr_spent) = sc::edit_spent(d, &e);
        v.push(row(
            C_NAME,
            "NOM",
            e.name
                .clone()
                .unwrap_or_else(|| p.name.clone())
                .to_uppercase(),
            "TAPEZ POUR ECRIRE (OU ALEA : GRAMMAIRE DE NOMS DE LA CULTURE).",
            Kind::Text,
        ));
        let cul = e.culture.unwrap_or(p.culture);
        v.push(row(
            C_CULTURE,
            "CULTURE",
            d.cultures[cul.min(d.cultures.len() - 1)].id.to_uppercase(),
            "L'ORIGINE : CHANGER DE CULTURE TIRE UN NOM DE CETTE CULTURE.",
            Kind::Pick,
        ));
        v.push(row(
            C_AGE,
            "AGE",
            format!("{:.0} ANS", e.age.unwrap_or(p.age)),
            "18 A 80 ANS.",
            Kind::Pick,
        ));
        v.push(row(
            C_ROLE,
            "POSTE",
            pick.map_or("(PAS A BORD)".to_string(), |q| {
                d.roles[q.role].name.to_uppercase()
            }),
            "LE POSTE A BORD ; ESPACE (OU CLIC SUR LA LISTE) PREND OU LAISSE.",
            Kind::Pick,
        ));
        for (i, s) in d.skills.iter().enumerate() {
            let delta = e
                .skills
                .iter()
                .find(|(j, _)| *j == i)
                .map_or(0, |(_, x)| *x);
            let fin = (p.skills[i] + f32::from(delta)).clamp(0.0, sc::SKILL_CAP.max(p.skills[i]));
            v.push(row(
                C_SKILL + i as u32,
                format!("  {}", s.name.to_uppercase()),
                format!("{:.1} -> {:.1}  ({:+})", p.skills[i], fin, delta),
                &format!("POINTS DE COMPETENCE : {sk_spent} / {} (RETIRER UN NIVEAU EN REND UN). PLAFOND {:.0}.", sc::SKILL_POINTS, sc::SKILL_CAP),
                Kind::Pick,
            ));
        }
        let traits = e.traits.clone().unwrap_or_else(|| p.traits.clone());
        for (i, &t) in traits.iter().enumerate() {
            let pts = sc::trait_points(&d.traits[t]);
            v.push(row(
                C_TRAIT + i as u32,
                format!("  TRAIT {}", i + 1),
                format!("{} ({:+})", d.traits[t].name.to_uppercase(), pts),
                &format!("EQUILIBRE : {tr_spent} / {} ; UN DEFAUT REND DES POINTS. ENTREE DE LA LIGNE : RETIRER.", sc::TRAIT_BUDGET),
                Kind::Pick,
            ));
        }
        let tp = self.ng.trait_pick.min(d.traits.len() - 1);
        v.push(row(
            C_TRAIT_ADD,
            "  AJOUTER UN TRAIT",
            format!(
                "{} ({:+})",
                d.traits[tp].name.to_uppercase(),
                sc::trait_points(&d.traits[tp])
            ),
            "< > CHOISIT, ENTREE AJOUTE (5 TRAITS AU PLUS).",
            Kind::Pick,
        ));
        let bg = &d.start.backgrounds[e.background.min(d.start.backgrounds.len() - 1)];
        v.push(row(
            C_BG,
            "HISTORIQUE",
            format!("{} ({:+})", bg.name.to_uppercase(), bg.cost),
            &bg.text.to_uppercase(),
            Kind::Pick,
        ));
        v.push(row(
            C_SALARY,
            "SALAIRE PAR JOUR",
            format!("{:.0} CR", e.salary.unwrap_or(p.salary)),
            "PAR DEFAUT SUIVANT LES COMPETENCES.",
            Kind::Pick,
        ));
        v.push(row(
            C_IMPLANT,
            "IMPLANT",
            e.implants.first().map_or("AUCUN".to_string(), |&i| {
                d.cybernetics[i].name.to_uppercase()
            }),
            "UN IMPLANT CYBERNETIQUE DE DEPART COUTE 2 POINTS D'EQUILIBRE.",
            Kind::Pick,
        ));
        v.push(row(
            C_REROLL,
            "RELANCER CE MEMBRE",
            "TIRER >",
            "LE REMPLACE PAR UN AUTRE CANDIDAT DU LOT.",
            Kind::Action,
        ));
        let locked = cc.locked.contains(&k);
        v.push(row(
            C_LOCK,
            "VERROUILLER",
            if locked { "OUI" } else { "NON" },
            "UN MEMBRE VERROUILLE RESTE QUAND ON RELANCE L'EQUIPE.",
            Kind::Pick,
        ));
        v.push(row(
            C_RESET,
            "ANNULER LES CHANGEMENTS",
            "OK >",
            "",
            Kind::Action,
        ));
        let cy = &self.ng.custom.cryo;
        let asleep = cy.members.contains(&k);
        v.push(row(
            C_CRYO,
            "CRYOSOMMEIL AU DEPART",
            if asleep { "OUI" } else { "NON" },
            "LA PERSONNE DORT DANS UN CAISSON DES LE JOUR 0 ET NE CONSOMME RIEN ; LES CAISSONS MANQUANTS SONT INSTALLES ET PAYES.",
            Kind::Pick,
        ));
        v.push(row(
            C_WAKE_ARR,
            "  REVEIL A L'ARRIVEE",
            if cy.wake_on_arrival { "OUI" } else { "NON" },
            "REVEILLE LES DORMEURS A LA PREMIERE ARRIVEE (NOUVELLE ETOILE OU QUAI).",
            Kind::Pick,
        ));
        v.push(row(
            C_WAKE_ALERT,
            "  REVEIL SUR ALERTE",
            if cy.wake_on_alert { "OUI" } else { "NON" },
            "LE MEILLEUR VEILLEUR SE REVEILLE EN CAS D'ALERTE CRITIQUE.",
            Kind::Pick,
        ));
        v.push(row(
            C_WAKE_DAY,
            "  REVEIL A LA DATE",
            cy.wake_day
                .map_or("AUCUN".to_string(), |d| format!("JOUR {d:.0}")),
            "REVEILLE TOUS LES DORMEURS CE JOUR-LA.",
            Kind::Pick,
        ));
        v
    }

    fn ai_rows(&self) -> Vec<Row> {
        let d = &self.data;
        let a = &self.ng.custom.ai;
        let tr = &d.start.ai_traits[a.personality.min(d.start.ai_traits.len() - 1)];
        let core = &d.start.ai_cores[a.core.min(d.start.ai_cores.len() - 1)];
        let nm = if a.name.is_empty() {
            "(TIRE DE LA GRAINE)".to_string()
        } else {
            a.name.to_uppercase()
        };
        let mut v = vec![
            row(
                A_NAME,
                "NOM DE L'IA",
                nm,
                "VOUS ETES L'IA DU VAISSEAU : TAPEZ SON NOM.",
                Kind::Text,
            ),
            row(
                A_PERS,
                "PERSONNALITE",
                tr.name.to_uppercase(),
                &tr.text.to_uppercase(),
                Kind::Pick,
            ),
            row(
                A_CORE,
                "NOYAU",
                format!("{} ({} PT)", core.name.to_uppercase(), core.cost),
                &core.text.to_uppercase(),
                Kind::Pick,
            ),
            row(
                A_POINTS,
                "POINTS D'IA",
                format!("{} / {}", sc::ai_points_used(d, a), sc::AI_POINTS),
                "NOYAU ET LOGICIELS DE DEPART COUTENT DES POINTS.",
                Kind::Info,
            ),
            row(
                A_ALLOC_RESET,
                "BUDGET DE CALCUL",
                if a.alloc.is_some() {
                    "PERSONNALISE (REMETTRE >)"
                } else {
                    "CELUI DE LA PERSONNALITE"
                },
                "REPARTITION INITIALE DU CALCUL ENTRE LES CINQ TACHES.",
                Kind::Action,
            ),
        ];
        let alloc: Vec<u8> = a.alloc.map_or_else(|| tr.alloc.clone(), |x| x.to_vec());
        for (t, def) in d.mind.tasks.iter().enumerate() {
            v.push(row(
                A_ALLOC + t as u32,
                format!("  {}", def.name.to_uppercase()),
                format!("{} / {}", alloc[t], sc::AI_MAX_WEIGHT),
                &def.text.to_uppercase(),
                Kind::Pick,
            ));
        }
        for (i, s) in d
            .mind
            .software
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tech.is_empty())
        {
            let on = a.software.contains(&s.id);
            v.push(row(
                A_SW + i as u32,
                format!(
                    "  {} {}",
                    if on { "[X]" } else { "[ ]" },
                    s.name.to_uppercase()
                ),
                format!("{} PT, MEMOIRE {}", sc::software_cost(s), s.memory),
                &s.text.to_uppercase(),
                Kind::Action,
            ));
        }
        v
    }

    fn world_rows(&self) -> Vec<Row> {
        let d = &self.data;
        let w = &self.ng.custom.world;
        let r = &self.pending_rules;
        let sys = &d.start.systems[w.system.min(d.start.systems.len() - 1)];
        let place = w.place.map_or(
            (
                "PAR DEFAUT DU SCENARIO".to_string(),
                "LA STATION, OU LE NAUFRAGE POUR CE SCENARIO.".to_string(),
            ),
            |i| {
                (
                    d.start.places[i].name.to_uppercase(),
                    d.start.places[i].text.to_uppercase(),
                )
            },
        );
        vec![
            row(
                W_SYSTEM,
                "SYSTEME DE DEPART",
                sys.name.to_uppercase(),
                &sys.text.to_uppercase(),
                Kind::Pick,
            ),
            row(W_PLACE, "POSITION DE DEPART", place.0, &place.1, Kind::Pick),
            row(
                W_EPI,
                "EPIDEMIES",
                x_text(w.epidemics),
                "MULTIPLIE LES FOYERS SPONTANES DANS LES COLONIES.",
                Kind::Pick,
            ),
            row(
                W_SUR,
                "SURPRISES ET ANOMALIES",
                x_text(w.surprises),
                "FREQUENCE DES SURPRISES DU JOUR (RENCONTRES, OFFRES, ACCIDENTS).",
                Kind::Pick,
            ),
            row(
                W_PIR,
                "PIRATES ET PATROUILLES",
                x_text(r.ai_aggression),
                "AGRESSIVITE DES IA HOSTILES : 0 = GALAXIE PACIFIQUE.",
                Kind::Pick,
            ),
            row(
                W_EVT,
                "EVENEMENTS",
                x_text(r.event_freq),
                "EVENEMENTS D'EQUIPAGE, ERUPTIONS, METEORITES, INTRUSIONS.",
                Kind::Pick,
            ),
            row(
                W_PROV,
                "PROVISIONS DE DEPART",
                x_text(w.provisions),
                "MULTIPLIE LA SOUTE DU SCENARIO (VIVRES, PIECES, CARGAISON).",
                Kind::Pick,
            ),
        ]
    }

    fn options_rows(&self) -> Vec<Row> {
        let d = &self.data;
        let lvl = start::difficulty_of(&self.pending_rules);
        let desc = self.pending_rules.describe();
        let mut v = vec![
            row(
                O_LANG,
                "LANGUE",
                self.settings.describe()[7].1.clone(),
                "SAUVEGARDE DANS LES REGLAGES.",
                Kind::Pick,
            ),
            row(
                O_KB,
                "CLAVIER",
                self.settings.describe()[6].1.clone(),
                "AZERTY PAR DEFAUT, QWERTY EN OPTION.",
                Kind::Pick,
            ),
            row(
                O_TUTO,
                "TUTORIEL GUIDE",
                self.settings.describe()[4].1.clone(),
                "",
                Kind::Pick,
            ),
            row(
                O_DIFF,
                "DIFFICULTE (REGLES)",
                lvl.map_or("PERSONNALISEE".to_string(), |l| {
                    start::DIFFICULTY_NAMES[l].to_string()
                }),
                "DOUX / STANDARD / RUDE : DEGATS, AGRESSIVITE, EVENEMENTS, CARBURANT.",
                Kind::Pick,
            ),
            row(
                O_PRESET,
                "PRESET PAR CATEGORIES",
                self.ng.preset_idx.map_or("(AUCUN)".to_string(), |i| {
                    d.start.presets[i].name.to_uppercase()
                }),
                &self
                    .ng
                    .preset_idx
                    .map_or(String::new(), |i| d.start.presets[i].text.to_uppercase()),
                Kind::Pick,
            ),
        ];
        for &i in &RULE_ORDER {
            v.push(row(
                O_RULE + i as u32,
                format!("  {}", desc[i].0.to_uppercase()),
                desc[i].1.clone(),
                RULE_HINTS[i],
                Kind::Pick,
            ));
        }
        let sm = usize::from(self.ng.custom.save_mode.min(2));
        v.push(row(
            O_SAVE,
            "  SAUVEGARDES",
            sc::SAVE_MODES[sm],
            SAVE_HINTS[sm],
            Kind::Pick,
        ));
        v.push(row(
            O_ADV,
            "REGLES AVANCEES",
            "OUVRIR >",
            "L'ECRAN COMPLET DES REGLES.",
            Kind::Action,
        ));
        v
    }

    fn recap_rows(&self) -> Vec<Row> {
        let files = list_presets();
        let cur = files
            .get(self.ng.preset_pick)
            .cloned()
            .unwrap_or_else(|| "(AUCUN FICHIER)".into());
        vec![
            row(
                R_RANDOM,
                "TOUT TIRER AU HASARD",
                "ALEATOIRE >",
                "UNE CONFIGURATION ENTIERE ET VALIDE, GRAINE COMPRISE.",
                Kind::Action,
            ),
            row(
                R_NAME,
                "NOM DE LA CONFIGURATION",
                if self.ng.preset_name.is_empty() {
                    "(TAPEZ UN NOM)".to_string()
                } else {
                    self.ng.preset_name.to_uppercase()
                },
                "POUR LA SAUVER.",
                Kind::Text,
            ),
            row(
                R_SAVE,
                "SAUVER CETTE CONFIGURATION",
                "ECRIRE >",
                "saves/presets/<nom>.ron",
                Kind::Action,
            ),
            row(
                R_LOAD,
                "CHARGER UNE CONFIGURATION",
                cur.to_uppercase(),
                "< > CHOISIT, ENTREE CHARGE.",
                Kind::Pick,
            ),
            row(
                R_EXPORT,
                "PARTAGER PAR CODE",
                "ECRIRE LE CODE >",
                "ECRIT UN CODE VEC1-... DANS saves/presets/<nom>.code.txt",
                Kind::Action,
            ),
            row(
                R_IMPORT,
                "IMPORTER UN CODE",
                "LIRE import.txt >",
                "COLLEZ UN CODE DANS saves/presets/import.txt PUIS ENTREE.",
                Kind::Action,
            ),
        ]
    }
}

// ---- actions ------------------------------------------------------------------------------

impl Game {
    fn ng_h(&mut self) -> u64 {
        self.ng.draws += 1;
        mix64(self.ng.seed() ^ self.ng.draws.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// A click on a row: `code` = row * 4 + part (0 less, 1 the row itself, 2 more, 3 random).
    pub fn click_ng_field(&mut self, code: usize) {
        let rows = self.ng_rows();
        let (r, part) = (code / 4, code % 4);
        let Some(row) = rows.get(r) else { return };
        let step = self.ng.step;
        self.ng.row[step] = r;
        if step == S_CREW {
            self.ng.crew_focus = true;
        }
        self.ng.note.clear();
        let id = row.id;
        match step {
            S_SHIP => self.ship_field(id, part),
            S_CREW => self.crew_field(id, part),
            S_AI => self.ai_field(id, part),
            S_WORLD => {
                self.world_field(id, part);
                self.ng_world_refresh();
            }
            S_OPTIONS => self.options_field(id, part),
            S_RECAP => self.recap_field(id, part),
            _ => {}
        }
    }

    pub fn ng_kind_selected(&self) -> Option<Kind> {
        self.ng_rows()
            .get(self.ng.row[self.ng.step])
            .map(|r| r.kind)
    }

    /// Typing into the text row under the cursor.
    pub fn ng_type(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode;
        let rows = self.ng_rows();
        let Some(row) = rows.get(self.ng.row[self.ng.step]) else {
            return false;
        };
        if row.kind != Kind::Text {
            return false;
        }
        let id = row.id;
        let step = self.ng.step;
        let k = self.ng.cursor;
        let base = self
            .ng
            .pool
            .get(k)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let ci = (id.saturating_sub(C_CT) / 3) as usize;
        let target: Option<&mut String> = match (step, id) {
            (S_SHIP, S_NAME) => Some(&mut self.ng.custom.ship.name),
            (S_SHIP, S_REG) => Some(&mut self.ng.custom.ship.registration),
            (S_SHIP, S_COMPANY) => Some(&mut self.ng.custom.ship.company),
            (S_AI, A_NAME) => Some(&mut self.ng.custom.ai.name),
            (S_RECAP, R_NAME) => Some(&mut self.ng.preset_name),
            (S_CREW, C_NAME) => Some(self.ng.custom.crew.edit_mut(k).name.get_or_insert(base)),
            (S_CREW, i) if i >= C_CT && (i - C_CT) % 3 == 2 => self
                .ng
                .custom
                .crew
                .contacts
                .get_mut(ci)
                .map(|c| &mut c.name),
            _ => None,
        };
        let Some(s) = target else { return false };
        match code {
            KeyCode::Backspace => {
                s.pop();
            }
            KeyCode::Delete => s.clear(),
            _ => {
                let Some(t) = text.filter(|t| t.chars().any(|c| !c.is_control())) else {
                    return false;
                };
                for ch in t.chars().filter(|c| !c.is_control()) {
                    if s.chars().count() < 28 {
                        s.push(ch);
                    }
                }
            }
        }
        true
    }

    // ---- ship ----

    fn ng_ship_default(&mut self) {
        let sc_ = &self.data.scenarios[self.ng.scenario];
        if let Some(mut c) = sc::catalog_from_scenario(&self.data, sc_, self.ng.ship) {
            let old = &self.ng.custom.ship;
            c.name = old.name.clone();
            c.registration = old.registration.clone();
            c.company = old.company.clone();
            c.budget = old.budget;
            self.ng.custom.ship = c;
        }
    }

    pub fn ng_set_ship_mode(&mut self, mode: ShipMode) {
        match mode {
            ShipMode::Scenario => self.ng.custom.ship.mode = ShipMode::Scenario,
            ShipMode::Catalog => {
                if self.ng.custom.ship.hull.is_empty()
                    || self.data.hull_index(&self.ng.custom.ship.hull).is_none()
                {
                    self.ng_ship_default();
                }
                self.ng.custom.ship.mode = ShipMode::Catalog;
            }
            ShipMode::Design => {
                self.ng.plan_files = crate::designer_view::list_plan_files();
                self.ng.custom.ship.mode = ShipMode::Design;
                if self.ng.custom.ship.plan.is_empty() {
                    // The first plan on sale that the budget allows.
                    let list = self.ng_plan_list();
                    let pick = (0..list.len()).find(|&i| {
                        self.ng_load_plan(i);
                        sc::ship_check(
                            &self.data,
                            &self.data.scenarios[self.ng.scenario],
                            &self.pending_rules,
                            &self.ng.custom.ship,
                        )
                        .is_ok()
                    });
                    match pick {
                        Some(i) => self.ng_load_plan(i),
                        None => self.ng.custom.ship.plan.clear(),
                    }
                }
            }
        }
    }

    fn ng_load_plan(&mut self, i: usize) {
        let list = self.ng_plan_list();
        let Some((_, src)) = list.get(i) else { return };
        self.ng.plan_pick = i;
        let text = match src {
            PlanSrc::Sale(k) => sc::plan_text_of_def(&self.data, &self.data.ship_plans[*k]),
            PlanSrc::File(f) => {
                std::fs::read_to_string(crate::designer_view::plans_dir().join(f)).ok()
            }
        };
        match text {
            Some(t) => self.ng.custom.ship.plan = t,
            None => self.ng.note = "PLAN ILLISIBLE".into(),
        }
    }

    fn ship_field(&mut self, id: u32, part: usize) {
        let d = Arc::clone(&self.data);
        let h = self.ng_h();
        match id {
            S_MODE => {
                let cur = match self.ng.custom.ship.mode {
                    ShipMode::Scenario => 0,
                    ShipMode::Catalog => 1,
                    ShipMode::Design => 2,
                };
                let n = cycle(cur, 3, part, h);
                self.ng_set_ship_mode([ShipMode::Scenario, ShipMode::Catalog, ShipMode::Design][n]);
            }
            S_MODEL => {
                let n = d.scenarios[self.ng.scenario].ships.len();
                self.ng.ship = cycle(self.ng.ship, n, part, h);
            }
            S_HULL => {
                let c = &self.ng.custom.ship;
                let cur = d.hull_index(&c.hull).unwrap_or(0);
                let next = cycle(cur, d.hulls.len(), part, h);
                let fit: Vec<String> = c
                    .modules
                    .iter()
                    .filter(|m| !m.is_empty())
                    .cloned()
                    .collect();
                let id = d.hulls[next].id.clone();
                if let Some(m) = sc::catalog_fill(&d, &id, &fit) {
                    self.ng.custom.ship.hull = id;
                    self.ng.custom.ship.modules = m;
                }
            }
            S_PLAN => {
                let n = self.ng_plan_list().len();
                let i = cycle(self.ng.plan_pick, n, part, h);
                self.ng_load_plan(i);
            }
            S_EDIT => self.ng_open_designer(),
            S_EXPORT => {
                if let Ok(bp) =
                    vector_sim::shipwright::Blueprint::from_text(&d, &self.ng.custom.ship.plan)
                {
                    let dir = crate::designer_view::plans_dir();
                    let name = crate::designer_view::file_name_of(&bp.name);
                    let r = std::fs::create_dir_all(&dir)
                        .and_then(|()| std::fs::write(dir.join(&name), bp.to_text()));
                    self.ng.note = match r {
                        Ok(()) => format!("PLAN EXPORTE : {}", dir.join(&name).display()),
                        Err(e) => format!("ECHEC EXPORT : {e}"),
                    };
                    self.ng.plan_files = crate::designer_view::list_plan_files();
                } else {
                    self.ng.note = "PAS DE PLAN A EXPORTER".into();
                }
            }
            S_NAME => {
                if part == 3 {
                    self.ng.custom.ship.name = sc::random_ship_name(h);
                }
            }
            S_REG => {
                if part == 3 {
                    self.ng.custom.ship.registration = sc::random_registration(h);
                }
            }
            S_COMPANY => {
                if part == 3 {
                    self.ng.custom.ship.company = sc::random_company(h);
                }
            }
            S_BUDGET => {
                let sc_ = &d.scenarios[self.ng.scenario];
                let auto = sc::ship_budget(
                    &d,
                    sc_,
                    &self.pending_rules,
                    &sc::ShipCustom {
                        budget: None,
                        ..self.ng.custom.ship.clone()
                    },
                );
                let cur = self.ng.custom.ship.budget.unwrap_or(auto);
                self.ng.custom.ship.budget = match part {
                    0 => Some((cur / 1.25).round()),
                    2 => Some((cur * 1.25).round()),
                    _ => None,
                };
            }
            i if i >= S_SLOT => {
                let slot = (i - S_SLOT) as usize;
                let hi = d.hull_index(&self.ng.custom.ship.hull).unwrap_or(0);
                let cands = sc::slot_candidates(&d, hi, slot);
                // Options: empty, then each module that fits.
                let cur = self
                    .ng
                    .custom
                    .ship
                    .modules
                    .get(slot)
                    .and_then(|m| d.module_index(m))
                    .and_then(|mi| cands.iter().position(|&c| c == mi))
                    .map_or(0, |p| p + 1);
                let n = cycle(cur, cands.len() + 1, part, h);
                if let Some(m) = self.ng.custom.ship.modules.get_mut(slot) {
                    *m = if n == 0 {
                        String::new()
                    } else {
                        d.modules[cands[n - 1]].id.clone()
                    };
                }
            }
            _ => {}
        }
    }

    pub fn ng_open_designer(&mut self) {
        let seed = self.ng.seed();
        self.sim = vector_sim::scene::new_sim(seed, Arc::clone(&self.data));
        self.sim.blueprints.clear();
        let text = self.ng.custom.ship.plan.clone();
        let bi = if text.is_empty() {
            None
        } else {
            self.sim.import_blueprint(&text).ok()
        };
        self.dz.bp = bi.unwrap_or_else(|| self.sim.new_blueprint("MON VAISSEAU"));
        self.ng.designing = true;
        self.dz_open();
    }

    /// Back from the designer: the plan being edited becomes the design of the start.
    pub fn ng_close_designer(&mut self) {
        if let Some(bp) = self.sim.blueprints.get(self.dz.bp) {
            self.ng.custom.ship.plan = bp.to_text();
            self.ng.custom.ship.mode = ShipMode::Design;
        }
        self.ng.designing = false;
        self.screen = Screen::NewGame;
    }

    // ---- crew ----

    fn crew_edit_ok(&mut self) -> bool {
        let k = self.ng.cursor;
        let Some(p) = self.ng.pool.get(k) else {
            return false;
        };
        let e = self.ng.custom.crew.edit_of(k).cloned().unwrap_or_default();
        match sc::crew_edit_valid(&self.data, p, &e) {
            Ok(()) => true,
            Err(m) => {
                self.ng.note = m.to_uppercase();
                false
            }
        }
    }

    /// Apply `f` to the edit of the person under the cursor; undone if the budgets refuse it.
    fn crew_change(&mut self, f: impl FnOnce(&mut sc::CrewEdit)) {
        let k = self.ng.cursor;
        let before = self.ng.custom.crew.edit_of(k).cloned();
        f(self.ng.custom.crew.edit_mut(k));
        if !self.crew_edit_ok() {
            match before {
                Some(b) => *self.ng.custom.crew.edit_mut(k) = b,
                None => self.ng.custom.crew.edits.retain(|e| e.candidate != k),
            }
        }
    }

    fn crew_field(&mut self, id: u32, part: usize) {
        let d = Arc::clone(&self.data);
        let h = self.ng_h();
        let k = self.ng.cursor;
        let up = part != 0;
        match id {
            C_TAB => self.ng.crew_tab = 1 - self.ng.crew_tab,
            C_POINTS => {
                let cur = sc::crew_points(&d, &self.ng_config());
                let n = if part == 0 {
                    cur.saturating_sub(1).max(1)
                } else if part == 3 {
                    1 + (h % 12) as u32
                } else {
                    (cur + 1).min(24)
                };
                self.ng.custom.crew.points = Some(n);
            }
            C_NEWPOOL => {
                self.ng.custom.crew.salt += 1;
                self.ng.custom_crew = false;
                self.ng.custom.crew.edits.clear();
                self.ng.custom.crew.locked.clear();
                self.ng.pool_key = None;
                self.ng_refresh();
            }
            C_REROLL_ALL => self.ng_reroll_all(h),
            C_NAME => {
                if part == 3 {
                    let p = &self.ng.pool[k];
                    let cul = self
                        .ng
                        .custom
                        .crew
                        .edit_of(k)
                        .and_then(|e| e.culture)
                        .unwrap_or(p.culture);
                    let cid = d.cultures[cul].id.clone();
                    let name = format!(
                        "{} {}",
                        vector_world::names::given_name(&cid, h.is_multiple_of(2), h),
                        vector_world::names::surname(&cid, h ^ 0x5A)
                    );
                    self.crew_change(|e| e.name = Some(name));
                }
            }
            C_CULTURE => {
                let cur = self
                    .ng
                    .custom
                    .crew
                    .edit_of(k)
                    .and_then(|e| e.culture)
                    .unwrap_or(self.ng.pool[k].culture);
                let n = cycle(cur, d.cultures.len(), part, h);
                self.crew_change(|e| {
                    e.culture = Some(n);
                    e.name = None;
                });
            }
            C_AGE => {
                let cur = self
                    .ng
                    .custom
                    .crew
                    .edit_of(k)
                    .and_then(|e| e.age)
                    .unwrap_or(self.ng.pool[k].age);
                let n = match part {
                    0 => (cur - 1.0).max(18.0),
                    3 => 21.0 + (h % 40) as f32,
                    _ => (cur + 1.0).min(80.0),
                };
                self.crew_change(|e| e.age = Some(n.round()));
            }
            C_ROLE => {
                if let Some(p) = self.ng.picks.iter_mut().find(|p| p.candidate == k) {
                    p.role = cycle(p.role, d.roles.len(), part, h);
                    self.ng.custom_crew = true;
                } else {
                    self.ng_toggle_pick(k);
                }
            }
            C_BG => {
                let cur = self.ng.custom.crew.edit_of(k).map_or(0, |e| e.background);
                let n = cycle(cur, d.start.backgrounds.len(), part, h);
                self.crew_change(|e| e.background = n);
            }
            C_SALARY => {
                let cur = self
                    .ng
                    .custom
                    .crew
                    .edit_of(k)
                    .and_then(|e| e.salary)
                    .unwrap_or(self.ng.pool[k].salary);
                if part == 3 {
                    self.crew_change(|e| e.salary = None);
                } else {
                    let n = if up { cur + 5.0 } else { (cur - 5.0).max(0.0) };
                    self.crew_change(|e| e.salary = Some(n));
                }
            }
            C_IMPLANT => {
                let cur = self
                    .ng
                    .custom
                    .crew
                    .edit_of(k)
                    .and_then(|e| e.implants.first().copied());
                let n = cycle(cur.map_or(0, |i| i + 1), d.cybernetics.len() + 1, part, h);
                self.crew_change(|e| e.implants = if n == 0 { vec![] } else { vec![n - 1] });
            }
            C_REROLL => self.ng_reroll_member(k, h),
            C_LOCK => {
                let l = &mut self.ng.custom.crew.locked;
                if let Some(i) = l.iter().position(|&c| c == k) {
                    l.remove(i);
                } else {
                    l.push(k);
                }
            }
            C_RESET => self.ng.custom.crew.edits.retain(|e| e.candidate != k),
            C_CRYO => {
                let m = &mut self.ng.custom.cryo.members;
                if let Some(i) = m.iter().position(|&c| c == k) {
                    m.remove(i);
                } else {
                    m.push(k);
                }
            }
            C_WAKE_ARR => self.ng.custom.cryo.wake_on_arrival ^= true,
            C_WAKE_ALERT => self.ng.custom.cryo.wake_on_alert ^= true,
            C_WAKE_DAY => {
                let cur = self.ng.custom.cryo.wake_day.unwrap_or(0.0);
                let n = match part {
                    0 => (cur - 10.0).max(0.0),
                    3 => ((h % 30) as f64 + 1.0) * 10.0,
                    _ => cur + 10.0,
                };
                self.ng.custom.cryo.wake_day = (n > 0.0).then_some(n);
            }
            C_TRAIT_ADD => {
                let n = d.traits.len();
                if part == 1 {
                    let t = self.ng.trait_pick.min(n - 1);
                    let mut cur = self
                        .ng
                        .custom
                        .crew
                        .edit_of(k)
                        .and_then(|e| e.traits.clone())
                        .unwrap_or_else(|| self.ng.pool[k].traits.clone());
                    if cur.contains(&t) {
                        self.ng.note = "DEJA PRESENT".into();
                    } else {
                        cur.push(t);
                        self.crew_change(|e| e.traits = Some(cur));
                    }
                } else {
                    self.ng.trait_pick = cycle(self.ng.trait_pick, n, part, h);
                }
            }
            i if (C_SKILL..C_SKILL + 100).contains(&i) => {
                let s = (i - C_SKILL) as usize;
                let cur = self
                    .ng
                    .custom
                    .crew
                    .edit_of(k)
                    .and_then(|e| e.skills.iter().find(|(j, _)| *j == s).map(|(_, x)| *x))
                    .unwrap_or(0);
                let n = match part {
                    0 => cur - 1,
                    2 | 1 => cur + 1,
                    _ => (h % 5) as i8 - 1,
                };
                let n = n.clamp(-3, 4);
                self.crew_change(|e| {
                    e.skills.retain(|(j, _)| *j != s);
                    if n != 0 {
                        e.skills.push((s, n));
                    }
                });
            }
            i if (C_TRAIT..C_TRAIT + 100).contains(&i) => {
                let t = (i - C_TRAIT) as usize;
                let mut cur = self
                    .ng
                    .custom
                    .crew
                    .edit_of(k)
                    .and_then(|e| e.traits.clone())
                    .unwrap_or_else(|| self.ng.pool[k].traits.clone());
                if part == 1 {
                    cur.remove(t.min(cur.len() - 1));
                } else if let Some(x) = cur.get_mut(t) {
                    *x = cycle(*x, d.traits.len(), part, h);
                }
                let dup = cur.iter().enumerate().any(|(a, v)| cur[..a].contains(v));
                if dup {
                    self.ng.note = "TRAIT EN DOUBLE".into();
                } else {
                    self.crew_change(|e| e.traits = Some(cur));
                }
            }
            C_CT_ADD => {
                let cs = &mut self.ng.custom.crew.contacts;
                if cs.len() < sc::MAX_CONTACTS {
                    if let Some(f) =
                        (0..d.factions.len()).find(|f| !cs.iter().any(|c| c.faction == *f))
                    {
                        cs.push(sc::ContactSpec {
                            faction: f,
                            friend: true,
                            name: String::new(),
                        });
                    }
                }
            }
            C_CT_DEL => {
                self.ng.custom.crew.contacts.pop();
            }
            i if i >= C_CT => {
                let ci = ((i - C_CT) / 3) as usize;
                let which = (i - C_CT) % 3;
                let nf = d.factions.len();
                if let Some(c) = self.ng.custom.crew.contacts.get_mut(ci) {
                    match which {
                        0 => c.friend = !c.friend,
                        1 => {
                            let mut f = cycle(c.faction, nf, part, h);
                            let me = c.faction;
                            let taken = |f: usize, cs: &[sc::ContactSpec]| {
                                cs.iter()
                                    .enumerate()
                                    .any(|(j, o)| j != ci && o.faction == f)
                            };
                            let cs = self.ng.custom.crew.contacts.clone();
                            for _ in 0..nf {
                                if !taken(f, &cs) {
                                    break;
                                }
                                f = cycle(f, nf, if part == 0 { 0 } else { 2 }, h);
                            }
                            let _ = me;
                            self.ng.custom.crew.contacts[ci].faction = f;
                        }
                        _ => {
                            if part == 3 {
                                let cid = d.cultures[(h % d.cultures.len() as u64) as usize]
                                    .id
                                    .clone();
                                c.name = format!(
                                    "{} {}",
                                    vector_world::names::given_name(&cid, h.is_multiple_of(2), h),
                                    vector_world::names::surname(&cid, h ^ 0x33)
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Replace candidate `k` by another one of the pool (not on board, not locked).
    pub fn ng_reroll_member(&mut self, k: usize, h: u64) {
        let free: Vec<usize> = (0..self.ng.pool.len())
            .filter(|c| *c != k && !self.ng.picks.iter().any(|p| p.candidate == *c))
            .collect();
        let Some(&n) = free.get((h % free.len().max(1) as u64) as usize) else {
            self.ng.note = "PAS D'AUTRE CANDIDAT".into();
            return;
        };
        if let Some(p) = self.ng.picks.iter_mut().find(|p| p.candidate == k) {
            let role = p.role;
            p.candidate = n;
            p.role = role;
            self.ng.custom_crew = true;
        }
        self.ng.custom.crew.edits.retain(|e| e.candidate != k);
        self.ng.custom.crew.locked.retain(|c| *c != k);
        for c in &mut self.ng.custom.cryo.members {
            if *c == k {
                *c = n;
            }
        }
        self.ng.cursor = n;
        // The budget of points may no longer hold: say so.
        if self.ng_points_used() > sc::crew_points(&self.data, &self.ng_config()) {
            self.ng.note = "BUDGET DE POINTS DEPASSE : AJUSTEZ L'EQUIPE".into();
        }
    }

    pub fn ng_reroll_all(&mut self, h: u64) {
        let members: Vec<usize> = self
            .ng
            .picks
            .iter()
            .map(|p| p.candidate)
            .filter(|c| !self.ng.custom.crew.locked.contains(c))
            .collect();
        for (i, k) in members.into_iter().enumerate() {
            self.ng_reroll_member(k, mix64(h ^ i as u64));
        }
    }

    // ---- AI ----

    fn ai_field(&mut self, id: u32, part: usize) {
        let d = Arc::clone(&self.data);
        let h = self.ng_h();
        match id {
            A_NAME => {
                if part == 3 {
                    self.ng.custom.ai.name = sc::random_ai_name(h);
                }
            }
            A_PERS => {
                self.ng.custom.ai.personality = cycle(
                    self.ng.custom.ai.personality,
                    d.start.ai_traits.len(),
                    part,
                    h,
                )
            }
            A_CORE => {
                let mut t = self.ng.custom.ai.clone();
                t.core = cycle(t.core, d.start.ai_cores.len(), part, h);
                if sc::ai_points_used(&d, &t) > sc::AI_POINTS {
                    self.ng.note = "TROP DE POINTS D'IA : RETIREZ UN LOGICIEL".into();
                } else {
                    self.ng.custom.ai = t;
                }
            }
            A_ALLOC_RESET => self.ng.custom.ai.alloc = None,
            i if (A_ALLOC..A_ALLOC + 5).contains(&i) => {
                let t = (i - A_ALLOC) as usize;
                let tr = &d.start.ai_traits[self.ng.custom.ai.personality];
                let mut w = [0u8; 5];
                w.copy_from_slice(
                    &self
                        .ng
                        .custom
                        .ai
                        .alloc
                        .map_or_else(|| tr.alloc.clone(), |a| a.to_vec()),
                );
                w[t] = match part {
                    0 => w[t].saturating_sub(1),
                    3 => (h % 11) as u8,
                    _ => (w[t] + 1).min(sc::AI_MAX_WEIGHT),
                };
                if w.iter().any(|&x| x > 0) {
                    self.ng.custom.ai.alloc = Some(w);
                }
            }
            i if i >= A_SW => {
                let s = &d.mind.software[(i - A_SW) as usize];
                let mut t = self.ng.custom.ai.clone();
                if let Some(p) = t.software.iter().position(|x| *x == s.id) {
                    t.software.remove(p);
                } else {
                    t.software.push(s.id.clone());
                }
                let labs = 0;
                match sc::ai_valid(&d, &t, labs) {
                    Ok(()) => self.ng.custom.ai = t,
                    Err(e) => self.ng.note = e.to_uppercase(),
                }
            }
            _ => {}
        }
    }

    // ---- world ----

    fn world_field(&mut self, id: u32, part: usize) {
        let d = Arc::clone(&self.data);
        let h = self.ng_h();
        let up = part != 0;
        let lad = |v: f64| {
            if part == 3 {
                LADDER[(h % 8) as usize]
            } else {
                ladder(v, up)
            }
        };
        match id {
            W_SYSTEM => {
                self.ng.custom.world.system =
                    cycle(self.ng.custom.world.system, d.start.systems.len(), part, h);
                self.ng.world_key = None;
            }
            W_PLACE => {
                let cur = self.ng.custom.world.place.map_or(0, |p| p + 1);
                let n = cycle(cur, d.start.places.len() + 1, part, h);
                self.ng.custom.world.place = if n == 0 { None } else { Some(n - 1) };
            }
            W_EPI => self.ng.custom.world.epidemics = lad(self.ng.custom.world.epidemics),
            W_SUR => self.ng.custom.world.surprises = lad(self.ng.custom.world.surprises),
            W_PIR => self.pending_rules.ai_aggression = lad(self.pending_rules.ai_aggression),
            W_EVT => self.pending_rules.event_freq = lad(self.pending_rules.event_freq),
            W_PROV => {
                self.ng.custom.world.provisions = lad(self.ng.custom.world.provisions).max(0.25)
            }
            _ => {}
        }
    }

    // ---- options ----

    fn options_field(&mut self, id: u32, part: usize) {
        let d = Arc::clone(&self.data);
        let h = self.ng_h();
        let up = part != 0;
        match id {
            O_LANG => self.settings.adjust(7, up),
            O_KB => self.settings.adjust(6, up),
            O_TUTO => self.settings.adjust(4, up),
            O_DIFF => {
                let lvl = start::difficulty_of(&self.pending_rules).unwrap_or(1);
                let next = match part {
                    0 => lvl.saturating_sub(1),
                    3 => (h % 3) as usize,
                    _ => (lvl + 1).min(2),
                };
                self.pending_rules = start::difficulty_rules(next);
                self.ng.preset_idx = None;
            }
            O_PRESET => {
                let cur = self.ng.preset_idx.map_or(0, |i| i + 1);
                let n = cycle(cur, d.start.presets.len() + 1, part, h);
                self.ng.preset_idx = n.checked_sub(1);
                if let Some(i) = self.ng.preset_idx {
                    sc::apply_difficulty_preset(
                        &d,
                        i,
                        &mut self.pending_rules,
                        &mut self.ng.custom,
                    );
                }
            }
            O_SAVE => {
                let m = self.ng.custom.save_mode;
                self.ng.custom.save_mode = match part {
                    0 => m.saturating_sub(1),
                    3 => (h % 3) as u8,
                    _ => (m + 1).min(2),
                };
            }
            O_ADV => self.open_rules(Screen::NewGame),
            i if i >= O_RULE => {
                let r = (i - O_RULE) as usize;
                if part == 3 {
                    let def = vector_sim::rules::Rules::default();
                    let mut x = def.clone();
                    for _ in 0..(h % 4) {
                        x.adjust(r, !h.is_multiple_of(3));
                    }
                    // Only this rule moves: copy it back through the setters of `adjust`.
                    let mut cur = self.pending_rules.clone();
                    cur = apply_one(cur, &x, r);
                    self.pending_rules = cur;
                } else {
                    self.pending_rules.adjust(r, up);
                }
                self.ng.preset_idx = None;
            }
            _ => {}
        }
        self.save_settings();
    }

    // ---- recap / presets ----

    pub fn ng_apply_config(&mut self, cfg: &start::StartConfig) {
        self.ng.scenario = cfg.scenario;
        self.ng.ship = cfg.ship;
        self.ng.custom = cfg.custom.clone();
        self.pending_rules = cfg.rules.clone();
        self.ng.seed_text = cfg.seed.to_string();
        self.ng.pool_key = None;
        self.ng_refresh();
        if let Some(p) = &cfg.crew {
            self.ng.picks = p.clone();
            self.ng.custom_crew = true;
        }
        self.ng.world_key = None;
        self.ng.cursor = self.ng.picks.first().map_or(0, |p| p.candidate);
    }

    pub fn ng_random_all(&mut self) {
        let h = self.ng_h();
        let seed = h % 1_000_000_000;
        let cfg = sc::random_config(&self.data, seed);
        self.ng_apply_config(&cfg);
        self.ng.picks = start::default_crew(
            &self.data,
            &self.data.scenarios[self.ng.scenario],
            &self.ng.pool,
        );
        self.ng.custom_crew = false;
        self.ng.note = "CONFIGURATION TIREE AU HASARD".into();
    }

    fn recap_field(&mut self, id: u32, part: usize) {
        let files = list_presets();
        let name = if self.ng.preset_name.trim().is_empty() {
            "ma_config".to_string()
        } else {
            self.ng.preset_name.trim().to_string()
        };
        let fname = crate::designer_view::file_name_of(&name);
        let h = self.ng_h();
        match id {
            R_RANDOM => self.ng_random_all(),
            R_SAVE => {
                let cfg = self.ng_config();
                let p = sc::preset_of(&self.data, &name, &cfg);
                let dir = presets_dir();
                let r = std::fs::create_dir_all(&dir)
                    .and_then(|()| std::fs::write(dir.join(&fname), sc::preset_to_text(&p)));
                self.ng.note = match r {
                    Ok(()) => format!("CONFIGURATION SAUVEE : {fname}"),
                    Err(e) => format!("ECHEC : {e}"),
                };
            }
            R_LOAD => {
                if part == 1 {
                    let Some(f) = files.get(self.ng.preset_pick) else {
                        self.ng.note = "AUCUN FICHIER DANS saves/presets".into();
                        return;
                    };
                    let loaded = std::fs::read_to_string(presets_dir().join(f))
                        .map_err(|e| e.to_string())
                        .and_then(|t| sc::preset_from_text(&t));
                    self.ng_load_preset(loaded);
                } else {
                    self.ng.preset_pick = cycle(self.ng.preset_pick, files.len(), part, h);
                }
            }
            R_EXPORT => {
                let cfg = self.ng_config();
                let code = sc::preset_code(&sc::preset_of(&self.data, &name, &cfg));
                let dir = presets_dir();
                let file = format!("{}.code.txt", fname.trim_end_matches(".ron"));
                let r = std::fs::create_dir_all(&dir)
                    .and_then(|()| std::fs::write(dir.join(&file), &code));
                self.ng.note = match r {
                    Ok(()) => format!("CODE ECRIT DANS {file} ({} CARACTERES)", code.len()),
                    Err(e) => format!("ECHEC : {e}"),
                };
            }
            R_IMPORT => {
                let loaded = std::fs::read_to_string(presets_dir().join("import.txt"))
                    .map_err(|_| "AUCUN FICHIER saves/presets/import.txt".to_string())
                    .and_then(|t| sc::preset_from_code(&t));
                self.ng_load_preset(loaded);
            }
            _ => {}
        }
    }

    fn ng_load_preset(&mut self, loaded: Result<sc::StartPreset, String>) {
        match loaded.and_then(|p| sc::config_of(&self.data, &p).map(|c| (p, c))) {
            Ok((p, cfg)) => {
                self.ng_apply_config(&cfg);
                self.ng.preset_name = p.name;
                self.ng.note = "CONFIGURATION CHARGEE".into();
            }
            Err(e) => self.ng.note = format!("ECHEC : {}", e.to_uppercase()),
        }
    }

    /// The summary of the configuration (cached while it does not change).
    pub fn ng_review(&self) -> sc::Review {
        let cfg = self.ng_config();
        let key = format!("{cfg:?}");
        if let Some((k, r)) = &*self.ng.review.borrow() {
            if *k == key {
                return r.clone();
            }
        }
        let r = sc::review(&self.data, &cfg);
        *self.ng.review.borrow_mut() = Some((key, r.clone()));
        r
    }

    /// Recompute what the world step previews (the home the start system gives).
    pub fn ng_world_refresh(&mut self) {
        let key = (self.ng.seed(), self.ng.custom.world.system);
        if self.ng.world_key == Some(key) {
            return;
        }
        let kind = self.data.start.systems[key.1.min(self.data.start.systems.len() - 1)]
            .id
            .clone();
        self.ng.world_preview = Some(start::home_preview_kind(&self.data, key.0, &kind));
        self.ng.world_key = Some(key);
    }
}

/// `cur` with only rule `i` taken from `from`.
fn apply_one(
    mut cur: vector_sim::rules::Rules,
    from: &vector_sim::rules::Rules,
    i: usize,
) -> vector_sim::rules::Rules {
    match i {
        0 => cur.fuel_use = from.fuel_use,
        1 => cur.crew_needs = from.crew_needs,
        2 => cur.permadeath = from.permadeath,
        3 => cur.damage = from.damage,
        4 => cur.build_cost = from.build_cost,
        5 => cur.research_speed = from.research_speed,
        6 => cur.ai_aggression = from.ai_aggression,
        7 => cur.event_freq = from.event_freq,
        8 => cur.reveal_map = from.reveal_map,
        9 => cur.infinite_resources = from.infinite_resources,
        10 => cur.invulnerable = from.invulnerable,
        11 => cur.creative = from.creative,
        12 => cur.max_timescale = from.max_timescale,
        13 => cur.tech_locks = from.tech_locks,
        _ => {}
    }
    cur
}

// ---- drawing ------------------------------------------------------------------------------

const LABEL_W: usize = 30;
const VALUE_W: usize = 34;

/// The rows of the step at (x, y), one per line; the one selected is lit when `focus`.
pub fn draw_rows(fb: &mut Framebuffer, g: &Game, rows: &[Row], x: usize, y: usize, focus: bool) {
    let step = g.ng.step;
    let sel = g.ng.row[step].min(rows.len().saturating_sub(1));
    for (i, r) in rows.iter().enumerate() {
        let line = y + i;
        let on = focus && i == sel;
        let hover = g.hit_cells(
            x,
            line,
            LABEL_W,
            1,
            Act::Call(Game::click_ng_field, i * 4 + 1),
            None,
        );
        let c = if on {
            AMBER
        } else if hover {
            HOVER
        } else if r.kind == Kind::Info {
            DIM
        } else {
            GREEN
        };
        fb.text(
            x,
            line,
            &format!(
                "{}{:<w$.w$}",
                if on { ">" } else { " " },
                r.label,
                w = LABEL_W - 1
            ),
            c,
        );
        let vx = x + LABEL_W + 2;
        match r.kind {
            Kind::Pick | Kind::Text => {
                g.hit_cells(
                    x + LABEL_W,
                    line,
                    2,
                    1,
                    Act::Call(Game::click_ng_field, i * 4),
                    None,
                );
                fb.text(x + LABEL_W, line, "<", AMBER);
                let mut v = r.value.clone();
                if on && r.kind == Kind::Text {
                    v.push('_');
                }
                let v: String = v.chars().take(VALUE_W).collect();
                fb.text(vx, line, &v, c);
                let gx = vx + VALUE_W + 1;
                g.hit_cells(
                    gx,
                    line,
                    2,
                    1,
                    Act::Call(Game::click_ng_field, i * 4 + 2),
                    None,
                );
                fb.text(gx, line, ">", AMBER);
                let ax = gx + 3;
                let h2 = g.hit_cells(
                    ax,
                    line,
                    6,
                    1,
                    Act::Call(Game::click_ng_field, i * 4 + 3),
                    None,
                );
                fb.text(ax, line, "[ALEA]", if h2 { HOVER } else { DIM });
            }
            Kind::Action | Kind::Info => {
                let v: String = r.value.chars().take(VALUE_W + 8).collect();
                fb.text(vx, line, &v, c);
            }
        }
    }
    if let Some(r) = rows.get(sel) {
        let hl = fb.rows().saturating_sub(8);
        for (k, l) in wrap(&r.hint, 120).iter().take(2).enumerate() {
            fb.text(2, hl + k, l, DIM);
        }
    }
    if !g.ng.note.is_empty() {
        let hl = fb.rows().saturating_sub(6);
        fb.text(2, hl, &g.ng.note.chars().take(150).collect::<String>(), RED);
    }
}

pub fn draw_ai(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 6, "L'IA DU VAISSEAU : C'EST VOUS", AMBER);
    let rows = g.ng_rows();
    draw_rows(fb, g, &rows, 2, 8, true);
    let d = &g.data;
    let tr = &d.start.ai_traits[g.ng.custom.ai.personality.min(d.start.ai_traits.len() - 1)];
    let x = 112;
    fb.text(x, 8, "EFFETS PERMANENTS", AMBER);
    for (t, def) in d.mind.tasks.iter().enumerate() {
        let b = tr.bias[t];
        let c = if b > 0.0 {
            GREEN
        } else if b < 0.0 {
            RED
        } else {
            DIM
        };
        fb.text(
            x,
            10 + t,
            &format!("{:<14} {:+.0} %", def.name.to_uppercase(), b * 100.0),
            c,
        );
    }
    if tr.credits_mult != 1.0 {
        fb.text(
            x,
            16,
            &format!("CREDITS DE DEPART x{:.2}", tr.credits_mult),
            GREEN,
        );
    }
    if tr.crew_morale != 0.0 {
        fb.text(
            x,
            17,
            &format!("MORAL DE L'EQUIPAGE {:+.0} %", tr.crew_morale * 100.0),
            if tr.crew_morale > 0.0 { GREEN } else { RED },
        );
    }
    let core = &d.start.ai_cores[g.ng.custom.ai.core.min(d.start.ai_cores.len() - 1)];
    fb.text(
        x,
        19,
        &format!("PUISSANCE +{:.0}  MEMOIRE +{}", core.power, core.memory),
        GREEN,
    );
}

pub fn draw_world(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 6, "LE MONDE ET LE DEPART", AMBER);
    let rows = g.ng_rows();
    draw_rows(fb, g, &rows, 2, 8, true);
    fb.text(2, 17, "CE QUE LE SYSTEME DE DEPART DONNE", AMBER);
    if let Some((h, exact)) = &g.ng.world_preview {
        fb.text(
            4,
            19,
            &format!(
                "SYSTEME {}   ETOILE DE CLASSE {}",
                h.system.to_uppercase(),
                h.class
            ),
            GREEN,
        );
        fb.text(
            4,
            20,
            &format!(
                "{} PLANETE(S)  {} LUNE(S)  {} ASTEROIDE(S)  {} STATION(S)",
                h.planets, h.moons, h.asteroids, h.stations
            ),
            GREEN,
        );
        fb.text(
            4,
            21,
            &format!(
                "TERRITOIRE : {}",
                h.faction
                    .as_ref()
                    .map_or("AUCUN (ESPACE LIBRE)".to_string(), |f| f.to_uppercase())
            ),
            GREEN,
        );
        if !exact {
            fb.text(
                4,
                22,
                "AUCUN SYSTEME DE CE TYPE DANS CETTE GALAXIE : LE DEPART HABITUEL SERA UTILISE.",
                RED,
            );
        }
    }
    fb.text(
        2,
        24,
        "LA DENSITE DES ETOILES ET LES GUERRES SE REGLENT PAR LA GRAINE ET LES REGLES AVANCEES.",
        DIM,
    );
}

pub fn draw_recap_extra(fb: &mut Framebuffer, g: &Game, y: usize) {
    let r = g.ng_review();
    let mut row = y;
    let c = if r.error.is_some() { RED } else { GREEN };
    fb.text(4, row, "DIFFICULTE INDICATIVE", DIM);
    fb.text(26, row, &format!("{}/5", r.rating), AMBER);
    row += 1;
    if g.ng.custom.ship.mode != ShipMode::Scenario {
        fb.text(4, row, "COUT DU VAISSEAU", DIM);
        fb.text(
            26,
            row,
            &format!(
                "{:.0} / {:.0} CREDITS DE BUDGET",
                r.ship_cost, r.ship_budget
            ),
            c,
        );
        row += 1;
    }
    fb.text(4, row, "POINTS D'IA", DIM);
    fb.text(
        26,
        row,
        &format!("{} / {}", r.ai_points, sc::AI_POINTS),
        GREEN,
    );
    row += 1;
    for w in &r.warnings {
        fb.text(4, row, &format!("! {w}"), AMBER);
        row += 1;
    }
    if let Some(e) = &r.error {
        for l in wrap(&format!("IMPOSSIBLE DE LANCER : {e}").to_uppercase(), 120) {
            fb.text(4, row, &l, RED);
            row += 1;
        }
    }
}

/// The ship panel of the vessel step: what the design gives, live.
pub fn draw_ship_panel(fb: &mut Framebuffer, g: &Game, x: usize) {
    let d = &g.data;
    let c = &g.ng.custom.ship;
    let sc_ = &d.scenarios[g.ng.scenario];
    let cost_line = |fb: &mut Framebuffer, y: usize, cost: f64| {
        let budget = sc::ship_budget(d, sc_, &g.pending_rules, c);
        let col = if cost > budget { RED } else { GREEN };
        fb.text(
            x,
            y,
            &format!(
                "COUT {cost:.0} CR   BUDGET {budget:.0} CR   RESTE {:.0}",
                budget - cost
            ),
            col,
        );
    };
    match c.mode {
        ShipMode::Design => {
            fb.text(x, 6, "CONCEPTION", AMBER);
            let mut cache = g.ng.plan_cache.borrow_mut();
            if cache.as_ref().map(|c| &c.0) != Some(&c.plan) {
                *cache = Some((c.plan.clone(), sc::plan_report(d, &c.plan)));
            }
            match &cache.as_ref().expect("cached").1 {
                Err(e) => {
                    fb.text(
                        x,
                        8,
                        &format!(
                            "PAS DE PLAN VALIDE : {}",
                            e.chars().take(80).collect::<String>()
                        )
                        .to_uppercase(),
                        RED,
                    );
                }
                Ok((bp, r)) => {
                    let cells = r.cells;
                    let lines = [
                        format!("{} - {cells} CELLULES", bp.name.to_uppercase()),
                        format!(
                            "MASSE {:.1} T (PLEINE {:.1} T)   COQUE {:.0} PV",
                            r.mass_dry / 1000.0,
                            r.mass_wet / 1000.0,
                            r.hp
                        ),
                        format!(
                            "DELTA-V {:.0} M/S (VIDE {:.0})   POUSSEE/POIDS {:.2}",
                            r.dv_loaded, r.dv_empty, r.twr
                        ),
                        format!(
                            "ENERGIE {:.1} MW (PIC {:.1})   COUCHETTES {}",
                            r.power_supply / 1000.0,
                            r.power_peak / 1000.0,
                            r.berths
                        ),
                        format!(
                            "SOUTE {:.1} T   RESERVOIR {:.1} T",
                            r.cargo_kg / 1000.0,
                            r.fuel_kg / 1000.0
                        ),
                    ];
                    for (k, l) in lines.iter().enumerate() {
                        fb.text(x, 8 + k, l, GREEN);
                    }
                    cost_line(fb, 14, r.value);
                    fb.text(
                        x,
                        15,
                        &format!(
                            "  MATERIAUX + TRAVAIL {:.0} + MODULES {:.0}",
                            r.value - r.modules_cost,
                            r.modules_cost
                        ),
                        DIM,
                    );
                    for (k, e) in r.errors.iter().take(4).enumerate() {
                        fb.text(
                            x,
                            17 + k,
                            &format!("! {}", e.to_uppercase())
                                .chars()
                                .take(90)
                                .collect::<String>(),
                            RED,
                        );
                    }
                    for (k, w) in r.warnings.iter().take(3).enumerate() {
                        fb.text(
                            x,
                            21 + k,
                            &format!("? {}", w.to_uppercase())
                                .chars()
                                .take(90)
                                .collect::<String>(),
                            AMBER,
                        );
                    }
                    if r.errors.is_empty() {
                        let solids = crate::ship_preview::solids_of(d, bp);
                        crate::ship_preview::draw_solids(
                            fb,
                            &solids,
                            (x as i32 * 8, 270, x as i32 * 8 + 380, 640),
                            0.6 + crate::proto_view::anim_time() * 0.5,
                            0.5,
                            None,
                        );
                    }
                }
            }
        }
        _ => {
            let design = if c.mode == ShipMode::Catalog {
                sc::catalog_design(d, c).ok()
            } else {
                start::build_design(d, sc_, g.ng.ship).map(|x| x.0)
            };
            let Some(design) = design else {
                fb.text(x, 6, "EMPLACEMENT OU MODULE INVALIDE", RED);
                return;
            };
            let health = vec![1.0; design.slots.len()];
            let st = vector_sim::design::ShipStats::compute(d, &design, &health);
            fb.text(x, 6, &d.hulls[design.hull].name.to_uppercase(), AMBER);
            let lines = [
                format!(
                    "MASSE A VIDE {:.1} T   COQUE {:.0} PV   BOUCLIER {:.0}",
                    st.mass_dry / 1000.0,
                    st.hull_max,
                    st.shield
                ),
                format!("POUSSEE {:.0} KN   ISP {:.0} S", st.thrust / 1000.0, st.isp),
                format!(
                    "ENERGIE {:.1} MW (PIC {:.1})",
                    st.power_capacity / 1000.0,
                    st.power_peak / 1000.0
                ),
                format!(
                    "RESERVOIR {:.1} T   SOUTE {:.1} T",
                    st.fuel_cap / 1000.0,
                    st.cargo_kg / 1000.0
                ),
                format!(
                    "LOGEMENTS {:.0}   SAUT FTL {:.0} AL   SCANNER {}",
                    st.crew_cap,
                    st.jump_range,
                    crate::hud::fmt_dist(st.scan_range)
                ),
            ];
            for (k, l) in lines.iter().enumerate() {
                fb.text(x, 8 + k, l, GREEN);
            }
            if c.mode == ShipMode::Catalog {
                cost_line(fb, 14, sc::catalog_cost(d, c).unwrap_or(0.0));
                if let Err(e) = sc::flyable(d, &design) {
                    fb.text(x, 16, &format!("! {}", e.to_uppercase()), RED);
                }
            }
            let dsn = crate::ship_model::showcase_design(d, design.hull);
            let _ = dsn;
            let st2 = crate::ship_model::State::idle(&design);
            crate::ship_model::turntable(
                fb,
                d,
                &design,
                &st2,
                (x * 8 + 190) as i32,
                450,
                340,
                f64::from(crate::proto_view::anim_time()) * 0.6,
            );
        }
    }
}

#[cfg(test)]
#[path = "newgame_custom_tests.rs"]
mod tests;
