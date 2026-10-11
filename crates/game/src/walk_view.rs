//! Expedition screens (E on the surface screen): choose who goes down (and the rover), then walk
//! in the roguelike view where the ship's AI controls one crew member at a time. Arrows or the
//! rotation keys move (bump to fight), G gathers, Tab changes member, R calls everybody back.
//! Everything is also clickable text.
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::walk::{Tile, MAP_H, MAP_W};
use winit::keyboard::KeyCode;

const HOVER: u32 = 0x00D0_FFD0;
const WHITE: u32 = 0x00FF_FFFF;
const WATER: u32 = 0x0040_80E0;
const ROCK: u32 = 0x0080_8890;

fn dim(c: u32) -> u32 {
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    ((r / 2) << 16) | ((g / 2) << 8) | (b / 2)
}

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
    let (hurt, why) = vector_sim::walk::environment_damage(map.pressure, temp, 0.0);
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

// ---- walking -------------------------------------------------------------------------------

pub fn draw_walk(fb: &mut Framebuffer, g: &Game) {
    let Some(w) = &g.walk else {
        return;
    };
    let (x0, y0) = (1usize, 3usize);
    let biome = &g.data.biomes[w.biome];
    let ground = dim(biome.color);
    fb.text(
        1,
        1,
        &format!(
            "EXPEDITION - {} - TOUR {}",
            biome.name.to_uppercase(),
            w.turn
        ),
        AMBER,
    );
    // Tiles.
    for y in 0..MAP_H {
        for x in 0..MAP_W {
            let (glyph, c) = match w.tile(x, y) {
                Tile::Ground => {
                    // A little texture so the ground is not a flat field of dots.
                    let h = (x * 7 + y * 13 + (x * y) % 5) % 9;
                    (
                        if h == 0 {
                            ","
                        } else if h == 1 {
                            "`"
                        } else {
                            "."
                        },
                        ground,
                    )
                }
                Tile::Rock => ("#", ROCK),
                Tile::Water => ("~", WATER),
                Tile::Landing => ("^", AMBER),
            };
            fb.text(x0 + x as usize, y0 + y as usize, glyph, c);
        }
    }
    // Sites of interest (dim once searched).
    for t in &w.sites {
        let d = &g.data.sites[t.def];
        fb.text(
            x0 + t.x as usize,
            y0 + t.y as usize,
            &d.glyph,
            if t.used { dim(d.color) } else { d.color },
        );
    }
    // Plants.
    for p in w.plants.iter().filter(|p| !p.taken) {
        let sp = &g.data.species[p.species];
        let glyph = if sp.body < 2.0 { "\"" } else { "T" };
        fb.text(x0 + p.x as usize, y0 + p.y as usize, glyph, GREEN);
    }
    // Animals.
    for c in w.creatures.iter().filter(|c| c.alive) {
        let sp = &g.data.species[c.species];
        let first = sp.name.chars().find(|ch| ch.is_alphabetic()).unwrap_or('?');
        let hunts = sp.danger > 0.0 && !sp.domestic;
        let (ch, col) = if hunts {
            (first.to_uppercase().to_string(), RED)
        } else {
            (first.to_lowercase().to_string(), AMBER)
        };
        fb.text(x0 + c.x as usize, y0 + c.y as usize, &ch, col);
    }
    // The party.
    for (i, &(px, py)) in w.pos.iter().enumerate() {
        let up = w.up(&g.sim, i);
        let (ch, col) = if !up {
            ("x", RED)
        } else if i == w.active {
            ("@", WHITE)
        } else {
            ("@", GREEN)
        };
        fb.text(x0 + px as usize, y0 + py as usize, ch, col);
        if i == w.active && w.vehicle && up {
            // The rover: a small frame around the active member.
            fb.text(x0 + px as usize + 1, y0 + py as usize, "v", AMBER);
        }
    }
    // Clicking a tile next to the active member steps there (or fights what is there).
    let (ax, ay) = w.pos[w.active];
    g.hit(
        (
            (x0 * 8) as i32,
            (y0 * 10) as i32,
            ((x0 + MAP_W as usize) * 8) as i32,
            ((y0 + MAP_H as usize) * 10) as i32,
        ),
        Act::At(Game::walk_click),
        None,
    );
    let _ = (ax, ay);
    // Sidebar.
    let sx = x0 + MAP_W as usize + 3;
    fb.text(sx, y0, "EQUIPE (clic = prendre le controle)", DIM);
    for (i, &m) in w.party.iter().enumerate() {
        let p = &g.sim.crew.members[m];
        let up = w.up(&g.sim, i);
        let hover = up && g.hit_cells(sx, y0 + 2 + i, 40, 1, Act::Call(Game::walk_pick, i), None);
        let c = if !up {
            RED
        } else if i == w.active {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            sx,
            y0 + 2 + i,
            &format!(
                "{} {:<18.18} {:>3.0}%{}",
                if i == w.active { ">" } else { " " },
                p.name.to_uppercase(),
                p.health * 100.0,
                if up { "" } else { "  HORS COMBAT" }
            ),
            c,
        );
    }
    let mut row = y0 + 3 + w.party.len();
    if let Some(p) = g.sim.crew.members.get(w.active_member()) {
        fb.text(sx, row, "ORGANES", DIM);
        row += 1;
        for (def, v) in g.data.organs.iter().zip(&p.organs) {
            let c = if *v <= 0.0 {
                RED
            } else if *v < 0.6 {
                AMBER
            } else {
                GREEN
            };
            fb.text(
                sx,
                row,
                &format!(
                    "{:<9.9}{} {:>3.0}%",
                    def.name.to_uppercase(),
                    crate::ui::bar(f64::from(*v), 8),
                    v * 100.0
                ),
                c,
            );
            row += 1;
        }
    }
    row += 1;
    fb.text(
        sx,
        row,
        &format!(
            "BUTIN {:.0}/{:.0} KG",
            w.loot_mass().abs(),
            w.carry_capacity()
        ),
        GREEN,
    );
    for (r, kg) in w.loot.iter().take(8) {
        row += 1;
        fb.text(
            sx,
            row,
            &format!("  {:<20.20} {:>6.1}", g.data.resources[*r].name, kg),
            DIM,
        );
    }
    // Actions as clickable text.
    let row = y0 + MAP_H as usize + 1;
    let acts: [(&str, Act); 5] = [
        ("[G] CUEILLIR", Act::Key(KeyCode::KeyG)),
        ("[V] LANCER UN DRONE", Act::Key(KeyCode::KeyV)),
        ("[X] DESCENDRE DANS UN SITE", Act::Key(KeyCode::KeyX)),
        ("[TAB] CHANGER DE MEMBRE", Act::Key(KeyCode::Tab)),
        ("[R] RENTRER AU VAISSEAU", Act::Key(KeyCode::KeyR)),
    ];
    let mut x = x0;
    for (label, act) in acts {
        let hover = g.hit_cells(x, row, label.len(), 1, act, None);
        fb.text(x, row, label, if hover { AMBER } else { GREEN });
        x += label.len() + 3;
    }
    // Log.
    for (i, l) in w.log.iter().rev().take(4).enumerate() {
        fb.text(x0, row + 2 + i, l, if i == 0 { GREEN } else { DIM });
    }
    if w.over {
        fb.text(
            x0,
            row + 7,
            "EXPEDITION TERMINEE - APPUYEZ SUR UNE TOUCHE OU CLIQUEZ POUR REVENIR",
            AMBER,
        );
        g.hit((0, 0, 1280, 720), Act::Call(Game::walk_finish, 0), None);
    }
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
                    .walk_start(body, self.surf_cx, self.surf_cy, party, self.exp_vehicle)
                {
                    Ok(w) => {
                        self.walk = Some(w);
                        self.screen = Screen::Walk;
                    }
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::Escape => self.screen = Screen::Surface,
            _ => {}
        }
    }

    pub fn walk_pick(&mut self, i: usize) {
        if let Some(mut w) = self.walk.take() {
            if w.up(&self.sim, i) && i != w.active {
                w.active = i;
                let name = self.sim.crew.members[w.party[i]].name.clone();
                w.log.push(format!("L'IA prend le controle de {name}."));
            }
            self.walk = Some(w);
        }
    }

    /// A click on the map: a step towards the clicked tile when it is within reach.
    pub fn walk_click(&mut self, (px, py): (i32, i32)) {
        let Some(mut w) = self.walk.take() else {
            return;
        };
        let (tx, ty) = (px / 8 - 1, py / 10 - 3);
        let (ax, ay) = w.pos[w.active];
        let (dx, dy) = ((tx - ax).signum(), (ty - ay).signum());
        if (dx, dy) != (0, 0) && !w.over {
            // One axis at a time: the one that is farther away.
            if (tx - ax).abs() >= (ty - ay).abs() {
                self.sim.walk_step(&mut w, dx, 0);
            } else {
                self.sim.walk_step(&mut w, 0, dy);
            }
        }
        self.walk = Some(w);
    }

    pub fn walk_finish(&mut self, _: usize) {
        if let Some(w) = self.walk.take() {
            let kg = w.loot_mass();
            self.say(format!("EXPEDITION TERMINEE ({} TOURS)", w.turn));
            let _ = kg;
        }
        self.screen = Screen::Surface;
    }

    pub fn walk_key(&mut self, code: KeyCode) {
        let Some(mut w) = self.walk.take() else {
            self.screen = Screen::Surface;
            return;
        };
        if w.over {
            self.walk = Some(w);
            self.walk_finish(0);
            return;
        }
        let [up, down, right, left, _, _] = self.settings.move_keys();
        match code {
            c if c == KeyCode::ArrowUp || c == up => self.sim.walk_step(&mut w, 0, -1),
            c if c == KeyCode::ArrowDown || c == down => self.sim.walk_step(&mut w, 0, 1),
            c if c == KeyCode::ArrowLeft || c == left => self.sim.walk_step(&mut w, -1, 0),
            c if c == KeyCode::ArrowRight || c == right => self.sim.walk_step(&mut w, 1, 0),
            KeyCode::KeyG => self.sim.walk_gather(&mut w),
            KeyCode::Tab => self.sim.walk_switch(&mut w),
            KeyCode::KeyR => self.sim.walk_return(&mut w),
            KeyCode::KeyV => {
                if let Err(e) = self.sim.walk_deploy_drone(&mut w) {
                    self.say(e.to_uppercase());
                }
            }
            KeyCode::KeyX => match self.sim.rogue_from_walk(&w) {
                Ok(()) => {
                    self.rogue_return = Some(Screen::Walk);
                    self.screen = Screen::Rogue;
                }
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Space => {
                // Wait a turn.
                let (x, y) = w.pos[w.active];
                self.sim.walk_step(&mut w, 0, 0);
                let _ = (x, y);
            }
            _ => {}
        }
        self.walk = Some(w);
    }
}
