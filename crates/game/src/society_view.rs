//! SOCIETE page of the factions screen (S): the local law, the language and what the crew and
//! the AI understand of it, the culture of the place (calendar, festival, cuisine), the legal
//! situation, the school, the works of the crew and the races to bet on.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;

/// Codes of the buttons (see `Game::click_society`).
pub const ENROLL: usize = 0;
pub const COMPOSE: usize = 1;
pub const SELL: usize = 2;
pub const DONATE: usize = 3;
pub const BET: usize = 4;
pub const NEXT_ART: usize = 5;
pub const NEXT_SPORT: usize = 6;

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let d = &g.data;
    let f = g.fac_sel.min(d.factions.len() - 1);
    let fac = &d.factions[f];
    fb.text(
        1,
        1,
        &format!("SOCIETE : {}", fac.name.to_uppercase()),
        AMBER,
    );
    fb.text(60, 1, "[S] RETOUR AUX FACTIONS", DIM);
    // Law.
    let laws = s.laws_in_force(f);
    fb.text(1, 3, "LOI LOCALE", AMBER);
    let names: Vec<String> = laws
        .iter()
        .map(|&l| d.laws[l].name.to_uppercase())
        .collect();
    fb.text(
        1,
        4,
        &format!(
            "{} LOIS : {}...",
            laws.len(),
            names.join(", ").chars().take(36).collect::<String>()
        ),
        GREEN,
    );
    let (owed, noto) = (s.fines_owed(f), s.notoriety_in(f));
    let state = if s.has_warrant(f) {
        "MANDAT D'ARRET"
    } else if noto > 0.0 {
        "FICHE"
    } else {
        "CASIER VIERGE"
    };
    fb.text(
        1,
        5,
        &format!("SITUATION : {state}  NOTORIETE {noto:.1}  AMENDES {owed:.0} CR"),
        if noto > 0.0 || owed > 0.0 { RED } else { GREEN },
    );
    // Language.
    let lang = s.language_of_faction(f);
    let l = &d.society.languages[lang];
    fb.text(1, 7, "LANGUE", AMBER);
    fb.text(
        1,
        8,
        &format!(
            "{} (FAMILLE {})  ERE {}",
            l.name.to_uppercase(),
            l.family.to_uppercase(),
            s.language_era(lang)
        ),
        GREEN,
    );
    let words: Vec<String> = (0..3).map(|k| s.language_word(lang, k)).collect();
    fb.text(
        1,
        9,
        &format!("MOTS : {}", words.join(" / ").to_uppercase()),
        DIM,
    );
    fb.text(
        1,
        10,
        &format!(
            "COMPRIS PAR L'EQUIPAGE {:.0} %   PAR L'IA {:.0} %   PRIX {:+.1} %",
            s.crew_fluency(lang) * 100.0,
            s.ai_fluency() * 100.0,
            (s.language_edge_for(lang) - 1.0) * 100.0
        ),
        if s.understanding(lang) < 0.5 {
            RED
        } else {
            GREEN
        },
    );
    // Culture of the place.
    let body = s.docked.unwrap_or(0);
    let (year, frac) = s.local_date(body);
    fb.text(1, 12, "CULTURE DU LIEU", AMBER);
    fb.text(
        1,
        13,
        &format!(
            "AN {} DE LA PLANETE ({:.0} JOURS)  {}",
            year + 1,
            s.local_year(body) / 86_400.0,
            vector_sim::Sim::season_name(frac)
        ),
        GREEN,
    );
    let dish = &d.society.dishes[s.local_dish(f)];
    fb.text(
        1,
        14,
        &format!("CUISINE : {}", dish.name.to_uppercase()),
        GREEN,
    );
    match s.festival_now(f, body) {
        Some(i) => fb.text(
            1,
            15,
            &format!(
                "FETE EN COURS : {}",
                d.society.festivals[i].name.to_uppercase()
            ),
            AMBER,
        ),
        None => fb.text(1, 15, "PAS DE FETE EN CE MOMENT", DIM),
    }
    // Crew and school.
    fb.text(
        1,
        17,
        "EQUIPAGE (HAUT/BAS) : LANGUE LOCALE ET ETUDES",
        AMBER,
    );
    let crew: Vec<_> = s.crew.alive().collect();
    let who = g.soc_who.min(crew.len().saturating_sub(1));
    for (k, p) in crew.iter().enumerate().take(8) {
        let sel = k == who;
        let lesson = s
            .society
            .lessons
            .iter()
            .find(|x| x.person == p.id)
            .map(|x| d.society.studies[usize::from(x.study)].name.to_uppercase());
        fb.text(
            1,
            18 + k,
            &format!(
                "{}{:<18.18} {:>3.0} %  {}",
                if sel { ">" } else { " " },
                p.name.to_uppercase(),
                s.fluency(p.id, lang) * 100.0,
                lesson.unwrap_or_default()
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    let study = g.soc_study.min(d.society.studies.len() - 1);
    let st = &d.society.studies[study];
    let art = g.soc_art.min(d.society.arts.len() - 1);
    let sport = g.soc_sport.min(d.society.sports.len() - 1);
    let x = 62;
    fb.text(x, 3, "ECOLE (PGUP/PGDN)", AMBER);
    fb.text(
        x,
        4,
        &format!(
            "{}  {:.0} CR  {:.0} J",
            st.name.to_uppercase(),
            s.study_fee(study),
            st.days
        ),
        GREEN,
    );
    fb.text(x, 5, &st.text.to_uppercase(), DIM);
    fb.text(
        x,
        6,
        &format!("BREVETS : {}", s.society.diplomas.len()),
        DIM,
    );
    fb.text(x, 8, "ARTS", AMBER);
    let form = &d.society.arts[art];
    let skill = crew.get(who).map_or(0.0, |p| s.craft_skill(p.id, art));
    fb.text(
        x,
        9,
        &format!(
            "{}  METIER {:.0} %  RENOM {:.1}",
            form.name.to_uppercase(),
            skill * 100.0,
            s.society.fame
        ),
        GREEN,
    );
    for (k, w) in s.society.works.iter().enumerate().take(4) {
        fb.text(
            x,
            10 + k,
            &format!(
                "{:<26.26} Q{:>3.0}  {:>6.0} CR",
                w.title.to_uppercase(),
                w.quality * 100.0,
                s.work_value(k)
            ),
            GREEN,
        );
    }
    fb.text(x, 15, "SPORTS ET PARIS", AMBER);
    let sp = &d.society.sports[sport];
    fb.text(x, 16, &sp.name.to_uppercase(), GREEN);
    let odds = s.field_odds(sport);
    let fav = odds
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
        .map_or(0, |(i, _)| i);
    for (i, (p, pay)) in odds.iter().enumerate().take(6) {
        fb.text(
            x,
            17 + i,
            &format!(
                "{}{:<8} {:>4.0} %  COTE {:.1}",
                if i == fav { "*" } else { " " },
                s.runner_name(sport, i).to_uppercase(),
                p * 100.0,
                pay
            ),
            if i == fav { AMBER } else { GREEN },
        );
    }
    let items = [
        ("ETUDIER (E)", ENROLL),
        ("COMPOSER (C)", COMPOSE),
        ("VENDRE (V)", SELL),
        ("DONNER AU MUSEE (D)", DONATE),
        ("PARIER 100 CR SUR LE FAVORI (P)", BET),
        ("ART SUIVANT (A)", NEXT_ART),
        ("SPORT SUIVANT (J)", NEXT_SPORT),
    ];
    for (k, (label, code)) in items.iter().enumerate() {
        let y = 28 * 10 + k as i32 * 12;
        let rc = (8, y, 8 + 36 * 8, y + 12);
        crate::proto_view::button(fb, g, rc, label, Act::Call(Game::click_society, *code));
    }
}
