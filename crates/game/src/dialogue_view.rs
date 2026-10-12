//! Radio conversation screen (Enter in flight with a ship targeted): the other ship drawn in
//! wireframe, who they are and how they speak, what they say, and numbered buttons for what the
//! ship's AI can answer. 1-6, Up/Down + Enter or a click choose; Escape closes the channel.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;

const WIRE: u32 = 0x0040_7050;
const HOVER: u32 = 0x00D0_FFD0;

fn rect(fb: &mut Framebuffer, l: i32, t: i32, r: i32, b: i32, c: u32) {
    let (l, t, r, b) = (l as f32, t as f32, r as f32, b as f32);
    fb.line(l, t, r, t, c);
    fb.line(r, t, r, b, c);
    fb.line(r, b, l, b, c);
    fb.line(l, b, l, t, c);
}

/// Words wrapped to `w` characters.
fn wrap(text: &str, w: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > w {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    fb.text(1, 1, "COMMUNICATIONS", AMBER);
    let Some(view) = g.sim.dialogue_view(g.dlg_npc, &g.dlg_node, &g.dlg_result) else {
        fb.text(1, 3, "(PLUS DE CONTACT)", DIM);
        return;
    };
    // Portrait: the ship of the other party, in wireframe, with a little signal animation.
    let (cx, cy) = (150.0f32, 190.0f32);
    rect(fb, 30, 70, 270, 310, WIRE);
    match view.role {
        "CIVILISATION" => {
            // A settlement: a dome, towers and a few lights.
            fb.circle(cx, cy + 20.0, 45.0, GREEN);
            fb.line(cx - 45.0, cy + 20.0, cx + 45.0, cy + 20.0, GREEN);
            for dx in [-70.0f32, 62.0] {
                fb.line(cx + dx, cy + 20.0, cx + dx, cy - 30.0, GREEN);
                fb.line(cx + dx - 6.0, cy - 30.0, cx + dx + 6.0, cy - 30.0, GREEN);
                fb.circle(cx + dx, cy - 36.0, 3.0, AMBER);
            }
            fb.line(cx - 90.0, cy + 20.0, cx + 90.0, cy + 20.0, DIM);
        }
        "CARGO" => {
            fb.line(cx - 70.0, cy - 25.0, cx + 50.0, cy - 25.0, GREEN);
            fb.line(cx + 50.0, cy - 25.0, cx + 80.0, cy, GREEN);
            fb.line(cx + 80.0, cy, cx + 50.0, cy + 25.0, GREEN);
            fb.line(cx + 50.0, cy + 25.0, cx - 70.0, cy + 25.0, GREEN);
            fb.line(cx - 70.0, cy + 25.0, cx - 70.0, cy - 25.0, GREEN);
            for k in 0..4 {
                let x = cx - 50.0 + k as f32 * 25.0;
                fb.line(x, cy - 25.0, x, cy + 25.0, DIM);
            }
        }
        "MINEUR" => {
            fb.circle(cx - 20.0, cy, 28.0, GREEN);
            fb.line(cx + 8.0, cy - 6.0, cx + 70.0, cy - 14.0, GREEN);
            fb.line(cx + 8.0, cy + 6.0, cx + 70.0, cy + 14.0, GREEN);
            fb.line(cx + 70.0, cy - 14.0, cx + 70.0, cy + 14.0, GREEN);
        }
        "PIRATE" => {
            fb.line(cx - 70.0, cy, cx + 20.0, cy - 30.0, RED);
            fb.line(cx + 20.0, cy - 30.0, cx + 80.0, cy, RED);
            fb.line(cx + 80.0, cy, cx + 20.0, cy + 30.0, RED);
            fb.line(cx + 20.0, cy + 30.0, cx - 70.0, cy, RED);
            fb.line(cx - 30.0, cy, cx + 40.0, cy, RED);
        }
        _ => {
            fb.line(cx - 60.0, cy - 20.0, cx + 70.0, cy, GREEN);
            fb.line(cx + 70.0, cy, cx - 60.0, cy + 20.0, GREEN);
            fb.line(cx - 60.0, cy + 20.0, cx - 40.0, cy, GREEN);
            fb.line(cx - 40.0, cy, cx - 60.0, cy - 20.0, GREEN);
            fb.circle(cx + 20.0, cy, 8.0, GREEN);
        }
    }
    // Signal bars that move while they speak.
    let t = crate::proto_view::anim_time();
    for k in 0..12 {
        let h = 4.0 + 10.0 * ((t * 6.0 + k as f32 * 0.9).sin() * 0.5 + 0.5);
        let x = 50.0 + k as f32 * 16.0;
        fb.line(x, 296.0, x, 296.0 - h, GREEN);
    }
    fb.text_at(34, 56, &format!("{} - {}", view.role, view.speaker), AMBER);
    fb.text(
        4,
        33,
        &format!("FACTION : {}", view.faction.to_uppercase()),
        GREEN,
    );
    fb.text(4, 34, &format!("TON : {}", view.tone.to_uppercase()), DIM);
    fb.text(4, 35, &format!("REPUTATION : {:+.2}", view.rep), GREEN);
    // What they say.
    let x = 36;
    let mut row = 8;
    fb.text(
        x,
        row - 1,
        &format!("{} :", view.speaker.to_uppercase()),
        AMBER,
    );
    for l in wrap(&view.text, 100) {
        fb.text(x, row, &l, GREEN);
        row += 1;
    }
    // The log of the conversation so far.
    row += 1;
    for l in g.dlg_log.iter().rev().take(5).rev() {
        for w in wrap(l, 110) {
            fb.text(x, row, &w, DIM);
            row += 1;
        }
    }
    // The answers: clickable text, lit when the pointer is over them.
    let row0 = 26;
    for (i, (label, ok)) in view.options.iter().enumerate() {
        let row = row0 + i * 2;
        let hover = *ok
            && g.hit_cells(
                x,
                row,
                label.chars().count() + 6,
                1,
                Act::Call(Game::dlg_pick, i),
                None,
            );
        let sel = i == g.dlg_sel;
        let c = if !ok {
            DIM
        } else if hover {
            HOVER
        } else if sel {
            AMBER
        } else {
            GREEN
        };
        let mark = if sel || hover { ">" } else { " " };
        fb.text(x, row, &format!("{mark} {}  {}", i + 1, label), c);
        if !ok {
            fb.text(x + label.chars().count() + 8, row, "(indisponible)", DIM);
        }
    }
    crate::ui::foot(
        fb,
        g,
        1,
        fb.rows() - 3,
        "1-6 OU CLIC : REPONDRE   HAUT/BAS + ENTREE   ECHAP : COUPER LA COMMUNICATION",
    );
}

impl Game {
    /// Start a conversation with the targeted ship.
    pub fn open_dialogue(&mut self) {
        // A ship targeted wins; otherwise the people of the targeted world, when it has some.
        let k = match (self.sim.target_npc, self.sim.target) {
            (Some(k), _) => k,
            (None, Some(b)) => match self.sim.can_hail_world(b) {
                Ok(()) => vector_sim::civ::CIV_SUBJECT + b,
                Err(e) => {
                    self.say(e.to_uppercase());
                    return;
                }
            },
            (None, None) => {
                self.say("AUCUNE CIBLE : VAISSEAU (R) OU PLANETE (MAJ+TAB)");
                return;
            }
        };
        let Some(start) = self.sim.dialogue_start(k) else {
            self.say("PAS DE REPONSE");
            return;
        };
        self.dlg_npc = k;
        self.dlg_node = start;
        self.dlg_result.clear();
        self.dlg_log.clear();
        self.dlg_sel = 0;
        self.screen = crate::game::Screen::Dialogue;
    }

    pub fn dlg_pick(&mut self, i: usize) {
        let Some(view) = self
            .sim
            .dialogue_view(self.dlg_npc, &self.dlg_node, &self.dlg_result)
        else {
            return;
        };
        let label = view.options.get(i).map(|o| o.0.clone()).unwrap_or_default();
        let (npc, node) = (self.dlg_npc, self.dlg_node.clone());
        match self.sim.dialogue_choose(npc, &node, i) {
            Ok((next, result)) => {
                self.dlg_log.push(format!("VOUS : {label}"));
                if !view.text.is_empty() {
                    self.dlg_log
                        .push(format!("{} : {}", view.speaker, view.text));
                }
                if next == "end" {
                    if !result.is_empty() {
                        self.say(result.to_uppercase());
                    }
                    self.screen = crate::game::Screen::Flight;
                } else {
                    self.dlg_node = next;
                    self.dlg_result = result;
                    self.dlg_sel = 0;
                }
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn dialogue_key(&mut self, code: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode as K;
        let n = self
            .sim
            .dialogue_view(self.dlg_npc, &self.dlg_node, &self.dlg_result)
            .map_or(0, |v| v.options.len());
        match code {
            K::Escape => self.screen = crate::game::Screen::Flight,
            K::ArrowUp if n > 0 => self.dlg_sel = (self.dlg_sel + n - 1) % n,
            K::ArrowDown if n > 0 => self.dlg_sel = (self.dlg_sel + 1) % n,
            K::Enter => self.dlg_pick(self.dlg_sel),
            K::Digit1 => self.dlg_pick(0),
            K::Digit2 => self.dlg_pick(1),
            K::Digit3 => self.dlg_pick(2),
            K::Digit4 => self.dlg_pick(3),
            K::Digit5 => self.dlg_pick(4),
            K::Digit6 => self.dlg_pick(5),
            _ => {}
        }
    }
}
