//! Expedition preparation (E on the surface screen): choose who goes down and the rover, then land
//! on the chosen cell. The exploration itself is the common one (see `rogue_view`).
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN, HOVER};
use crate::mouse::Act;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

// ---- preparing the expedition ------------------------------------------------------------

pub fn draw_prep(fb: &mut Framebuffer, g: &Game) {
    fb.text(1, 1, "PREPARER UNE EXPEDITION", AMBER);
    let Some(map) = &g.surf else {
        return;
    };
    let b = &g.data.biomes[map.biome_at(g.surf_cx, g.surf_cy)];
    fb.text(
        1,
        3,
        &format!(
            "Lieu : {} ({}, {})   Biome : {}",
            g.sim.bodies[map.body].name.to_uppercase(),
            g.surf_cx,
            g.surf_cy,
            b.name.to_uppercase()
        ),
        GREEN,
    );
    // The air and the climate of the place: a suit is advised when they hurt.
    let temp = g.sim.temp_now(map, g.surf_cx, g.surf_cy);
    let (hurt, why) = vector_sim::rogue::environment_damage(map.pressure, temp, 0.0);
    fb.text(
        1,
        4,
        &format!(
            "AIR {:.2} BAR   TEMPERATURE {:.0} K   {}",
            map.pressure,
            temp,
            if hurt > 0.0 {
                format!(
                    "{} : COMBINAISON CONSEILLEE (ARMURE EN EQUIPEMENT)",
                    why.to_uppercase()
                )
            } else {
                "CLIMAT SUPPORTABLE A MAINS NUES".to_string()
            }
        ),
        if hurt > 0.0 { AMBER } else { GREEN },
    );
    fb.text(1, 5, "QUI DESCEND (clic ou Espace pour cocher) :", DIM);
    let n = g.sim.crew.members.len();
    for (i, p) in g.sim.crew.members.iter().enumerate() {
        let on = g.exp_party.get(i).copied().unwrap_or(false);
        let able = p.alive && p.health > 0.0;
        let row = 7 + i;
        let hover = able
            && g.hit_cells(
                3,
                row,
                50,
                1,
                Act::Call(Game::exp_toggle, i),
                Some(Act::Call(Game::exp_toggle, i)),
            );
        let sel = g.exp_sel == i;
        let c = if !able {
            DIM
        } else if hover || sel {
            AMBER
        } else {
            GREEN
        };
        fb.text(
            3,
            row,
            &format!(
                "{} [{}] {:<22} {:<12} SANTE {:>3.0}%",
                if sel { ">" } else { " " },
                if on { "x" } else { " " },
                p.name.to_uppercase(),
                g.data.roles[p.role].name.to_uppercase(),
                p.health * 100.0
            ),
            c,
        );
    }
    let row = 8 + n;
    let hover = g.hit_cells(3, row, 40, 1, Act::Call(Game::exp_toggle, n), None);
    let sel = g.exp_sel == n;
    fb.text(
        3,
        row,
        &format!(
            "{} [{}] ROVER D'EXPLORATION (x2 vitesse, +250 kg de charge)",
            if sel { ">" } else { " " },
            if g.exp_vehicle { "x" } else { " " }
        ),
        if hover || sel { AMBER } else { GREEN },
    );
    let go = g.hit_cells(3, row + 3, 12, 1, Act::Key(KeyCode::Enter), None);
    fb.text(3, row + 3, "> PARTIR", if go { HOVER } else { AMBER });
    let back = g.hit_cells(3, row + 5, 8, 1, Act::Key(KeyCode::Escape), None);
    fb.text(3, row + 5, "RETOUR", if back { HOVER } else { DIM });
    fb.text(
        1,
        fb.rows() - 3,
        "HAUT/BAS CHOISIR  ESPACE COCHER  ENTREE PARTIR  ECHAP RETOUR",
        DIM,
    );
}

impl Game {
    pub fn open_expedition(&mut self) {
        let Some(map) = &self.surf else {
            return;
        };
        let body = map.body;
        if let Err(e) = self.sim.expedition_possible(body) {
            self.say(e.to_uppercase());
            return;
        }
        if map.surface.is_sea(self.surf_cx, self.surf_cy) {
            self.say("PAS DE TERRE FERME ICI : CHOISISSEZ UNE CASE DE TERRE");
            return;
        }
        let n = self.sim.crew.members.len();
        self.exp_party = (0..n)
            .map(|i| {
                let p = &self.sim.crew.members[i];
                p.alive && p.health > 0.0
            })
            .collect();
        self.exp_vehicle = false;
        self.exp_sel = 0;
        self.screen = Screen::Expedition;
    }

    pub fn exp_toggle(&mut self, i: usize) {
        let n = self.sim.crew.members.len();
        if i == n {
            self.exp_vehicle = !self.exp_vehicle;
        } else if let Some(v) = self.exp_party.get_mut(i) {
            *v = !*v;
        }
        self.exp_sel = i;
    }

    pub fn expedition_key(&mut self, code: KeyCode) {
        let n = self.sim.crew.members.len();
        match code {
            KeyCode::ArrowUp => self.exp_sel = (self.exp_sel + n) % (n + 1),
            KeyCode::ArrowDown => self.exp_sel = (self.exp_sel + 1) % (n + 1),
            KeyCode::Space => self.exp_toggle(self.exp_sel),
            KeyCode::Enter => {
                let party: Vec<usize> = (0..n).filter(|&i| self.exp_party[i]).collect();
                let Some(map) = &self.surf else {
                    return;
                };
                let body = map.body;
                match self
                    .sim
                    .rogue_land(body, self.surf_cx, self.surf_cy, party, self.exp_vehicle)
                {
                    Ok(()) => {
                        self.rogue_from_surface = true;
                        self.screen = Screen::Rogue;
                    }
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::Escape => self.screen = Screen::Surface,
            _ => {}
        }
    }
}
