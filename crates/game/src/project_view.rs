//! Projects of a colony (P in the colony screen): the works under way (terraforming and
//! megastructures), the catalogue of what can be started, and the state of the world's climate
//! with the plants that crack its gases. Everything shown comes from the simulation.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::terraform::{CO2, O2, PROCESSES};
use winit::keyboard::KeyCode;

/// Units ordered at once, by step of the selector.
pub const UNIT_STEPS: [u32; 6] = [1, 10, 100, 1000, 10_000, 100_000];

/// Most rows the lists show.
const ROWS: usize = 12;
/// Columns of the three zones (first column, width) of the 160-column screen.
const LEFT: (usize, usize) = (1, 58);
const MID: (usize, usize) = (61, 49);
const RIGHT: (usize, usize) = (112, 47);

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for w in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + w.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(w);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn big(v: f64) -> String {
    if v.abs() >= 1.0e4 || (v != 0.0 && v.abs() < 0.01) {
        format!("{v:.1e}").to_uppercase()
    } else {
        format!("{v:.1}")
    }
}

fn producers(g: &Game) -> Vec<usize> {
    (0..g.data.species.len())
        .filter(|&i| g.data.species[i].is_producer())
        .collect()
}

/// Text line in a zone, clipped to its width.
fn put(fb: &mut Framebuffer, zone: (usize, usize), row: usize, text: &str, col: u32) {
    let clipped: String = text.chars().take(zone.1).collect();
    fb.text(zone.0, row, &clipped, col);
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let Some(c) = s.colonies.get(g.col_idx) else {
        return;
    };
    let ci = g.col_idx;
    let focus = |f: u8| if g.proj_focus == f { AMBER } else { GREEN };
    fb.text(
        1,
        5,
        "PROJETS : TERRAFORMATION ET MEGASTRUCTURES  (P : RETOUR AUX LISTES)",
        AMBER,
    );
    fb.text(
        1,
        6,
        &format!(
            "ECHELLE DE KARDASHEV DE LA CIVILISATION : {:.3}   PUISSANCE {} W",
            s.empire_kardashev(),
            big(s.empire_power_w())
        ),
        GREEN,
    );
    // --- works under way ---------------------------------------------------------------
    put(fb, LEFT, 8, "TRAVAUX", focus(0));
    if c.projects.is_empty() {
        put(fb, LEFT, 9, "(AUCUN PROJET)", DIM);
    }
    let sel = g.proj_sel.min(c.projects.len().saturating_sub(1));
    let start = sel
        .saturating_sub(ROWS / 2)
        .min(c.projects.len().saturating_sub(ROWS));
    for (line, pi) in (start..(start + ROWS).min(c.projects.len())).enumerate() {
        let p = &c.projects[pi];
        let d = &g.data.projects[p.def];
        let stage = d.stages.get(p.stage).map_or("", |st| st.name.as_str());
        let done = p.units >= p.target;
        let state = if !p.enabled {
            "ARRET".to_string()
        } else if done {
            format!("{:.0}%", p.health * 100.0)
        } else {
            match p.wait {
                1 => "MATIERE".to_string(),
                2 => "ENERGIE".to_string(),
                3 => "EQUIPE".to_string(),
                _ => format!("{:.0}%", p.progress * 100.0),
            }
        };
        let name = if d.kind == "seeding" {
            format!(
                "{} {}",
                d.name,
                g.data.species.get(p.arg).map_or("?", |sp| sp.name.as_str())
            )
        } else {
            d.name.clone()
        };
        let row = 9 + line;
        let is_sel = g.proj_focus == 0 && pi == sel;
        g.hit_cells(
            LEFT.0,
            row,
            LEFT.1,
            1,
            Act::Call(Game::click_proj, pi),
            None,
        );
        put(
            fb,
            LEFT,
            row,
            &format!(
                "{}{:<24.24} {:>5}/{:<5} {:<10.10} {:<8.8}",
                if is_sel { ">" } else { " " },
                name.to_uppercase(),
                p.units,
                p.target,
                if done { "" } else { stage }.to_uppercase(),
                state
            ),
            if is_sel {
                AMBER
            } else if !p.enabled {
                DIM
            } else if p.wait > 0 && !done {
                RED
            } else {
                GREEN
            },
        );
    }
    // The selected work: what its stage still needs and what it already gives.
    let detail = 9 + ROWS + 2;
    if let Some(p) = c.projects.get(sel) {
        let d = &g.data.projects[p.def];
        put(
            fb,
            LEFT,
            detail,
            &format!("{} - {}", d.name.to_uppercase(), d.category.to_uppercase()),
            AMBER,
        );
        for (i, l) in wrap(&d.text.to_uppercase(), LEFT.1)
            .iter()
            .take(4)
            .enumerate()
        {
            put(fb, LEFT, detail + 1 + i, l, DIM);
        }
        if let Some(st) = d.stages.get(p.stage).filter(|_| p.units < p.target) {
            let list: Vec<String> = st
                .cost
                .iter()
                .map(|(r, kg)| {
                    let scale = match st.scale.as_str() {
                        "taper" => p.taper,
                        "radius" => c.terra.as_ref().map_or(1.0, |t| t.snap.radius / 6.371e6),
                        _ => 1.0,
                    };
                    format!("{} {}", r.to_uppercase(), big(kg * scale))
                })
                .collect();
            put(
                fb,
                LEFT,
                detail + 6,
                &format!(
                    "ETAPE {}/{} : {}",
                    p.stage + 1,
                    d.stages.len(),
                    st.name.to_uppercase()
                ),
                GREEN,
            );
            put(
                fb,
                LEFT,
                detail + 7,
                &format!(
                    "{:.0} J  EQUIPE {:.0}  {} KW",
                    st.days,
                    st.crew,
                    big(st.power)
                ),
                GREEN,
            );
            for (i, l) in wrap(&list.join(", "), LEFT.1).iter().take(2).enumerate() {
                put(fb, LEFT, detail + 8 + i, l, GREEN);
            }
            let why = match p.wait {
                1 => format!(
                    "ATTEND : {}",
                    g.data
                        .resources
                        .get(usize::from(p.short))
                        .map_or("?".into(), |r| r.name.to_uppercase())
                ),
                2 => "ATTEND : ENERGIE (RESEAU INSUFFISANT)".to_string(),
                3 => "ATTEND : MAIN-D'OEUVRE (POPULATION)".to_string(),
                _ => "EN COURS".to_string(),
            };
            put(
                fb,
                LEFT,
                detail + 10,
                &why,
                if p.wait > 0 { RED } else { GREEN },
            );
        }
        let fx = c.mega_effects(&g.data);
        put(
            fb,
            LEFT,
            detail + 12,
            &format!(
                "OUVRAGES : LOGEMENTS {}  STOCKS {} KG",
                big(fx.housing),
                big(fx.storage)
            ),
            DIM,
        );
        put(
            fb,
            LEFT,
            detail + 13,
            &format!(
                "ENERGIE RECUE {} KW  ENTRETIEN {} CR/J",
                big(fx.power),
                big(fx.upkeep)
            ),
            DIM,
        );
    }
    // --- catalogue ---------------------------------------------------------------------
    put(fb, MID, 8, "CATALOGUE", focus(1));
    let n = g.data.projects.len();
    let csel = g.cat_sel.min(n.saturating_sub(1));
    let cstart = csel.saturating_sub(ROWS / 2).min(n.saturating_sub(ROWS));
    let prods = producers(g);
    let species = prods
        .get(g.proj_species % prods.len().max(1))
        .copied()
        .unwrap_or(0);
    for (line, i) in (cstart..(cstart + ROWS).min(n)).enumerate() {
        let d = &g.data.projects[i];
        let arg = if d.kind == "seeding" { species } else { 0 };
        let check = s.project_check(ci, i, arg);
        let tag = match &check {
            Ok(_) => "",
            Err(e) if e.starts_with("technologie") => " [VERROUILLE]",
            Err(_) => " [IMPOSSIBLE]",
        };
        let row = 9 + line;
        let is_sel = g.proj_focus == 1 && i == csel;
        g.hit_cells(
            MID.0,
            row,
            MID.1,
            1,
            Act::Call(Game::click_cat, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        put(
            fb,
            MID,
            row,
            &format!(
                "{}{:<30.30}{}",
                if is_sel { ">" } else { " " },
                d.name.to_uppercase(),
                tag
            ),
            if is_sel {
                AMBER
            } else if check.is_ok() {
                GREEN
            } else {
                DIM
            },
        );
    }
    g.hit_cells(MID.0, 9 + ROWS, 12, 1, Act::Key(KeyCode::Enter), None);
    put(fb, MID, 9 + ROWS, " [COMMANDER]", AMBER);
    if let Some(d) = g.data.projects.get(csel) {
        let arg = if d.kind == "seeding" { species } else { 0 };
        let mut y = detail;
        put(fb, MID, y, &d.name.to_uppercase(), AMBER);
        y += 1;
        for l in wrap(&d.text.to_uppercase(), MID.1).iter().take(5) {
            put(fb, MID, y, l, DIM);
            y += 1;
        }
        let cost: Vec<String> = d
            .total_cost()
            .iter()
            .map(|(r, kg)| format!("{} {}", r.to_uppercase(), big(*kg)))
            .collect();
        for l in wrap(&format!("COUT PAR UNITE : {}", cost.join(", ")), MID.1)
            .iter()
            .take(3)
        {
            put(fb, MID, y, l, GREEN);
            y += 1;
        }
        put(
            fb,
            MID,
            y,
            &format!(
                "DUREE {:.1} ANS/UNITE  ENTRETIEN {} CR/J",
                d.total_days() / 365.25,
                big(d.upkeep)
            ),
            GREEN,
        );
        y += 1;
        match s.project_check(ci, csel, arg) {
            Ok(taper) => {
                if let (Some(t), "elevator") = (&c.terra, d.kind.as_str()) {
                    if let Some(r) = t.snap.geostationary() {
                        put(
                            fb,
                            MID,
                            y,
                            &format!(
                                "CABLE {} KM  SECTION x{:.1} A LA BASE",
                                big((r - t.snap.radius) / 1000.0),
                                taper
                            ),
                            GREEN,
                        );
                        y += 1;
                    }
                }
                if let (Some(t), "dyson") = (&c.terra, d.kind.as_str()) {
                    put(
                        fb,
                        MID,
                        y,
                        &format!(
                            "UNITE : {} W A {} UA DE L'ETOILE",
                            big(vector_sim::megastructure::swarm_power(
                                &t.snap, 1.0, d.param
                            )),
                            big(vector_sim::megastructure::swarm_radius(&t.snap) / 1.496e11)
                        ),
                        GREEN,
                    );
                    y += 1;
                }
            }
            Err(e) => {
                for l in wrap(&format!("IMPOSSIBLE : {}", e.to_uppercase()), MID.1)
                    .iter()
                    .take(2)
                {
                    put(fb, MID, y, l, RED);
                    y += 1;
                }
            }
        }
        if d.kind == "seeding" {
            let sp = &g.data.species[species];
            let suit = c
                .terra
                .as_ref()
                .map_or(0.0, |t| t.suitability(&g.data, species).0);
            put(
                fb,
                MID,
                y,
                &format!(
                    "ESPECE : {} ({:.0} % PROPICE)",
                    sp.name.to_uppercase(),
                    suit * 100.0
                ),
                AMBER,
            );
            y += 1;
            put(fb, MID, y, "(GAUCHE/DROITE POUR CHANGER)", DIM);
            y += 1;
        }
        if d.repeat {
            let units = UNIT_STEPS[g.proj_units.min(UNIT_STEPS.len() - 1)];
            put(
                fb,
                MID,
                y,
                &format!("UNITES COMMANDEES : {units}  ([ ] CHANGER)"),
                AMBER,
            );
        }
    }
    // --- the world ---------------------------------------------------------------------
    put(fb, RIGHT, 8, "MONDE", focus(2));
    let Some(t) = s.terra_of(ci) else {
        put(fb, RIGHT, 9, "(PAS DE CLIMAT A TRANSFORMER)", DIM);
        return;
    };
    let status = t.status();
    let alarm = status.contains("EMBALLEMENT") || status.contains("GLACIATION");
    let mut y = 9;
    let mut line = |fb: &mut Framebuffer, text: String, col: u32| {
        put(fb, RIGHT, y, &text, col);
        y += 1;
    };
    line(
        fb,
        format!("ETAT : {status}"),
        if alarm { RED } else { AMBER },
    );
    line(
        fb,
        format!(
            "T {:.1} K ({:+.1})  DERIVE {:+.2} K/AN",
            t.t,
            t.t - t.snap.t_base,
            t.drift
        ),
        GREEN,
    );
    line(
        fb,
        format!(
            "BILAN {:.1} K  ALBEDO {:.2}  SERRE {:.2}",
            t.target_temperature(),
            t.albedo(),
            t.optical_depth()
        ),
        GREEN,
    );
    line(
        fb,
        format!(
            "PRESSION {:.3} BAR  O2 {:.3}  CO2 {:.4}",
            t.pressure(),
            t.pressure_of(O2),
            t.pressure_of(CO2)
        ),
        GREEN,
    );
    let comp: Vec<String> = t
        .composition()
        .iter()
        .take(4)
        .map(|(id, f)| format!("{id} {:.1}%", f * 100.0))
        .collect();
    line(fb, format!("AIR : {}", comp.join(" ")), DIM);
    line(
        fb,
        format!(
            "MERS {:.0} % ({:+.1} M)  GLACES {} KG",
            t.sea_fraction() * 100.0,
            t.sea_level_rise(),
            big(t.ice)
        ),
        GREEN,
    );
    line(
        fb,
        format!(
            "CO2 GELE {} KG  C ENFOUI {} KG",
            big(t.co2res),
            big(t.carbon)
        ),
        DIM,
    );
    line(
        fb,
        format!("MIROIRS {} M2  SUIE {} KG", big(t.mirror), big(t.soot)),
        GREEN,
    );
    line(
        fb,
        format!(
            "VIE NATIVE {:.0} %  GLACE {:.0} % DU GLOBE",
            t.life * 100.0,
            t.ice_cover() * 100.0
        ),
        GREEN,
    );
    line(
        fb,
        format!("HABITABILITE DEHORS {:.0} %", t.habitability() * 100.0),
        if t.habitability() > 0.6 { AMBER } else { GREEN },
    );
    for sd in t.seeds.iter().take(4) {
        line(
            fb,
            format!(
                "SOUCHE {} : {}",
                g.data.species[usize::from(sd.species)].name.to_uppercase(),
                if sd.cover < 0.01 {
                    format!("{:.1e}", sd.cover).to_uppercase()
                } else {
                    format!("{:.0} %", sd.cover * 100.0)
                }
            ),
            DIM,
        );
    }
    y += 1;
    put(fb, RIGHT, y, "USINES ATMOSPHERIQUES", focus(2));
    y += 1;
    for (i, name) in PROCESSES.iter().enumerate() {
        let on = usize::from(t.process) == i;
        g.hit_cells(RIGHT.0, y, RIGHT.1, 1, Act::Call(Game::click_proc, i), None);
        let cursor = g.proj_focus == 2 && i == g.proc_sel;
        put(
            fb,
            RIGHT,
            y,
            &format!(
                "{}{}{}",
                if on { "*" } else { " " },
                if cursor { ">" } else { " " },
                name
            ),
            if on { AMBER } else { GREEN },
        );
        y += 1;
    }
    put(fb, RIGHT, y, &format!("DEBIT {} KG/S", big(t.rate)), DIM);
}

impl Game {
    pub fn click_proj(&mut self, i: usize) {
        self.proj_focus = 0;
        self.proj_sel = i;
    }

    pub fn click_cat(&mut self, i: usize) {
        self.proj_focus = 1;
        self.cat_sel = i;
    }

    pub fn click_proc(&mut self, i: usize) {
        self.proj_focus = 2;
        self.proc_sel = i;
        self.project_key(KeyCode::Enter);
    }

    /// Keys of the projects screen of a colony.
    pub fn project_key(&mut self, code: KeyCode) {
        let ci = self.col_idx;
        let n_proj = self.sim.colonies.get(ci).map_or(0, |c| c.projects.len());
        let n_cat = self.data.projects.len();
        let n_proc = PROCESSES.len();
        match code {
            KeyCode::KeyP | KeyCode::Escape => self.col_proj = false,
            KeyCode::Tab => self.proj_focus = (self.proj_focus + 1) % 3,
            KeyCode::ArrowUp => match self.proj_focus {
                0 if n_proj > 0 => self.proj_sel = (self.proj_sel + n_proj - 1) % n_proj,
                1 if n_cat > 0 => self.cat_sel = (self.cat_sel + n_cat - 1) % n_cat,
                2 => self.proc_sel = (self.proc_sel + n_proc - 1) % n_proc,
                _ => {}
            },
            KeyCode::ArrowDown => match self.proj_focus {
                0 if n_proj > 0 => self.proj_sel = (self.proj_sel + 1) % n_proj,
                1 if n_cat > 0 => self.cat_sel = (self.cat_sel + 1) % n_cat,
                2 => self.proc_sel = (self.proc_sel + 1) % n_proc,
                _ => {}
            },
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let prods = producers(self);
                if !prods.is_empty() {
                    let len = prods.len();
                    self.proj_species = if code == KeyCode::ArrowRight {
                        (self.proj_species + 1) % len
                    } else {
                        (self.proj_species + len - 1) % len
                    };
                }
            }
            KeyCode::BracketRight => {
                self.proj_units = (self.proj_units + 1).min(UNIT_STEPS.len() - 1);
            }
            KeyCode::BracketLeft => self.proj_units = self.proj_units.saturating_sub(1),
            KeyCode::Space if self.proj_focus == 0 && n_proj > 0 => {
                let _ = self
                    .sim
                    .colony_project_toggle(ci, self.proj_sel.min(n_proj - 1));
            }
            KeyCode::KeyX if self.proj_focus == 0 && n_proj > 0 => {
                match self
                    .sim
                    .colony_project_cancel(ci, self.proj_sel.min(n_proj - 1))
                {
                    Ok(()) => self.say("PROJET ARRETE"),
                    Err(e) => self.say(e.to_uppercase()),
                }
                self.proj_sel = 0;
            }
            KeyCode::Enter => match self.proj_focus {
                2 => match self.sim.colony_set_process(ci, self.proc_sel) {
                    Ok(()) => self.say(format!("USINES : {}", PROCESSES[self.proc_sel])),
                    Err(e) => self.say(e.to_uppercase()),
                },
                1 => {
                    let def = self.cat_sel.min(n_cat.saturating_sub(1));
                    let prods = producers(self);
                    let arg = prods
                        .get(self.proj_species % prods.len().max(1))
                        .copied()
                        .unwrap_or(0);
                    let units = UNIT_STEPS[self.proj_units.min(UNIT_STEPS.len() - 1)];
                    match self.sim.colony_project_start(ci, def, units, arg) {
                        Ok(()) => self.say("PROJET LANCE"),
                        Err(e) => self.say(e.to_uppercase()),
                    }
                }
                _ => {}
            },
            KeyCode::F2 => self.screen = crate::game::Screen::Empire,
            _ => {}
        }
    }
}
