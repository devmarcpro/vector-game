//! Text-grid screens: main menu, pause, station market, inventory.
use crate::game::{CrewTab, Game, MarketTab, Screen, MAIN_MENU, PAUSE_MENU};
use crate::hud::{AMBER, DIM, GREEN, HOVER, RED};
use crate::mouse::{arrow_of_word, key_of_word, pair_of_word, Act};
use winit::keyboard::KeyCode;

/// Colour of a list line under the pointer.
use vector_render::Framebuffer;
use vector_sim::{G0, RESALE};

/// The screen's own line of keys at the foot of the page: the permanent command bar says the
/// same (and its words are buttons), so it only shows when the bar is turned off.
pub fn foot(fb: &mut Framebuffer, g: &Game, col: usize, row: usize, text: &str) {
    if g.settings.hints {
        return;
    }
    foot_buttons(fb, g, col, row, text);
}

/// A line of "KEY:what it does" commands (separated by two spaces): each is a button.
fn foot_buttons(fb: &mut Framebuffer, g: &Game, col: usize, row: usize, text: &str) {
    let mut c = col;
    for seg in text.split("  ").filter(|x| !x.is_empty()) {
        let n = seg.chars().count();
        let word = seg.split([':', ' ']).next().unwrap_or("");
        let hover = key_of_word(g.screen, word)
            .is_some_and(|k| g.hit_cells(c, row, n, 1, Act::Key(k), None));
        fb.text(c, row, seg, if hover { AMBER } else { DIM });
        c += n + 2;
    }
}

fn menu(fb: &mut Framebuffer, g: &Game, top: usize, items: &[&str], sel: usize) {
    let col = fb.cols() / 2 - 12;
    for (i, it) in items.iter().enumerate() {
        let hover = g.hit_cells(
            col,
            top + i * 2,
            26,
            1,
            Act::Call(Game::click_menu, i),
            None,
        );
        let (c, mark) = if i == sel || hover {
            (AMBER, ">")
        } else {
            (GREEN, " ")
        };
        fb.text(col, top + i * 2, &format!("{mark} {it}"), c);
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    match g.screen {
        Screen::MainMenu => draw_main(fb, g),
        Screen::NewGame => crate::newgame::draw(fb, g),
        Screen::Hub => crate::hub::draw(fb, g),
        Screen::Dashboard => crate::dash_view::draw(fb, g),
        Screen::Pause => {
            fb.text(fb.cols() / 2 - 4, 6, "== PAUSE ==", AMBER);
            menu(fb, g, 9, &PAUSE_MENU, g.menu_sel);
            fb.text(
                fb.cols() / 2 - 12,
                fb.rows() - 3,
                "HAUT/BAS CHOISIR   ENTREE VALIDER",
                DIM,
            );
        }
        Screen::Station => draw_market(fb, g),
        Screen::Inventory => draw_inventory(fb, g),
        Screen::Ship => draw_ship(fb, g),
        Screen::Shipyard => draw_shipyard(fb, g),
        Screen::Designer => crate::designer_view::draw(fb, g),
        Screen::Crew => draw_crew(fb, g),
        Screen::Journal => draw_journal(fb, g),
        Screen::Galaxy => draw_galaxy(fb, g),
        Screen::Contracts => draw_contracts(fb, g),
        Screen::Bank => draw_bank(fb, g),
        Screen::Livestock => crate::farm_view::draw_livestock(fb, g),
        Screen::Casino => crate::farm_view::draw_casino(fb, g),
        Screen::Cosmos => crate::cosmos_view::draw(fb, g),
        Screen::Rogue => crate::rogue_view::draw(fb, g),
        Screen::Empire => draw_empire(fb, g),
        Screen::Colony => draw_colony(fb, g),
        Screen::Fleet => draw_fleet(fb, g),
        Screen::Tech => crate::tech_view::draw(fb, g),
        Screen::Rules => draw_rules(fb, g),
        Screen::Options => draw_options(fb, g),
        Screen::Codex => draw_codex(fb, g),
        Screen::Surface => crate::surface_view::draw(fb, g),
        Screen::Factions => crate::faction_view::draw(fb, g),
        Screen::Proto => crate::proto_view::draw(fb, g),
        Screen::Dialogue => crate::dialogue_view::draw(fb, g),
        Screen::Expedition => crate::expedition_view::draw_prep(fb, g),
        Screen::NavAuto => crate::nav_view::draw(fb, g),
        Screen::Destroyed => {
            let cols = fb.cols();
            fb.text_big((fb.w as i32 - 15 * 8 * 4) / 2, 140, "DETRUIT", 8, RED);
            fb.text(
                cols / 2 - 22,
                22,
                "VOTRE VAISSEAU A ETE DETRUIT. L ASSUREUR LE RECONSTRUIT A LA STATION",
                RED,
            );
            fb.text(
                cols / 2 - 22,
                24,
                "LA PLUS PROCHE : SOUTE PERDUE, 20 % DES CREDITS RETENUS.",
                DIM,
            );
            fb.text(cols / 2 - 10, 28, "ENTREE : CONTINUER", AMBER);
        }
        Screen::Flight => {}
    }
}

/// Tutorial banner and the F1 contextual help box, drawn over any screen.
/// Colour of a news tag (see `vector_sim::news::TAGS`).
fn tag_color(tag: u8) -> u32 {
    match tag {
        0 => AMBER,
        1 => RED,
        2 => GREEN,
        3 => 0x0060_C0FF,
        4 => 0x0090_E090,
        5 => 0x00D0_A0FF,
        _ => 0x00FF_8040,
    }
}

/// The news banner: what the sectors around have seen happen, scrolling along the bottom.
fn draw_ticker(fb: &mut Framebuffer, g: &Game) {
    let items = g.sim.headlines(14);
    let row = fb
        .rows()
        .saturating_sub(if g.settings.hints { 2 } else { 1 });
    let y = (row * 10) as i32;
    for yy in y..y + 10 {
        for x in 0..fb.w as i32 {
            fb.px_set(x, yy, 0);
        }
    }
    if items.is_empty() {
        fb.text_at(
            8,
            y + 1,
            "INFORMATIONS : AUCUNE NOUVELLE POUR L'INSTANT.",
            DIM,
        );
        return;
    }
    let segs: Vec<(u32, String)> = items
        .iter()
        .map(|n| {
            (
                tag_color(n.tag),
                format!("[{}] {}  +++  ", n.scale(), n.text.to_uppercase()),
            )
        })
        .collect();
    let total: i32 = segs.iter().map(|s| s.1.chars().count() as i32 * 8).sum();
    // The banner moves left at 40 px/s; the list is repeated so there is never a gap.
    let shift = ((crate::proto_view::anim_time() * 40.0) as i32).rem_euclid(total.max(1));
    let mut x = 8 - shift;
    for _ in 0..2 {
        for (col, text) in &segs {
            let w = text.chars().count() as i32 * 8;
            if x + w > 0 && x < fb.w as i32 {
                fb.text_at(x, y + 1, text, *col);
            }
            x += w;
        }
    }
}

pub fn draw_overlay(fb: &mut Framebuffer, g: &Game) {
    draw_overlay_inner(fb, g);
    // The context menu and the tooltips come last: they own the zones on top.
    crate::context_menu::draw(fb, g);
    crate::context_menu::draw_tooltip(fb, g);
}

/// Width in characters of the tutorial row on the screens that are not the flight view: from
/// column 1 to the buttons at the top right.
const TUTORIAL_ROW: usize = 104;
const TUTORIAL_BUTTON: &str = "[SUPPR ignorer]";

/// The tutorial line on the top row of a menu screen. It is one row only (the rows below are the
/// screen's title and tabs): a long lesson scrolls like the news ticker, the "[SUPPR ignorer]"
/// button stays where it is.
fn draw_tutorial_row(fb: &mut Framebuffer, g: &Game, i: usize, text: &str) {
    let head = format!("TUTO {}/{}  ", i + 1, crate::tutorial::STEPS.len());
    let room = TUTORIAL_ROW - head.chars().count() - TUTORIAL_BUTTON.len() - 2;
    let chars: Vec<char> = text.chars().collect();
    let shown: String = if chars.len() <= room {
        text.to_string()
    } else {
        // Hold still for the first moments of each lap so the beginning can be read.
        let lap = chars.len() + 7;
        let t = (crate::proto_view::anim_time() * 8.0) as usize;
        let start = t.saturating_sub(24) % lap;
        chars
            .iter()
            .chain("   +++ ".chars().collect::<Vec<_>>().iter())
            .cycle()
            .skip(start)
            .take(room)
            .collect()
    };
    fb.text(1, 0, &format!("{head}{shown:<room$}"), AMBER);
    let col = 1 + head.chars().count() + room + 2;
    let hover = g.hit_cells(
        col,
        0,
        TUTORIAL_BUTTON.len(),
        1,
        Act::Key(KeyCode::Delete),
        None,
    );
    fb.text(
        col,
        0,
        TUTORIAL_BUTTON,
        if hover { 0x00D0_FFD0 } else { AMBER },
    );
}

fn draw_overlay_inner(fb: &mut Framebuffer, g: &Game) {
    crate::flight_menu::draw(fb, g);
    // Before the command bar: the bar stays the last zone drawn.
    crate::hub::draw_menu_button(fb, g);
    crate::dash_view::draw_alert_badge(fb, g);
    crate::context_menu::draw_help_button(fb, g);
    // Screens that have no message line of their own: the result of an action (fuel moved,
    // fine paid, research started, expedition refused...) still has to be readable.
    if matches!(
        g.screen,
        Screen::Inventory
            | Screen::Journal
            | Screen::Tech
            | Screen::Codex
            | Screen::Factions
            | Screen::Surface
            | Screen::Dashboard
    ) {
        if let Some(m) = g.messages.last() {
            fb.text_fit(1, fb.rows().saturating_sub(3), m, AMBER);
        }
    }
    if g.settings.ticker && !matches!(g.screen, Screen::MainMenu | Screen::NewGame) {
        draw_ticker(fb, g);
    }
    if let Some((i, text)) = g.tutorial_line() {
        if g.screen != Screen::Flight && g.settings.hints {
            draw_tutorial_row(fb, g, i, &text);
        } else {
            let line = format!(
                "TUTO {}/{}  {}  [SUPPR ignorer]",
                i + 1,
                crate::tutorial::STEPS.len(),
                text
            );
            let flight = g.screen == Screen::Flight;
            let hints = g.settings.hints;
            // Width of row `k`. On the other screens with the command bar the first row stops before
            // the buttons at the top right and the next ones sit right of the screen's title (rows 1
            // and 2, from column 64), so the text never lands on the title.
            let width = |k: usize| -> usize {
                if flight || (hints && k > 0) {
                    54
                } else if hints {
                    116
                } else {
                    fb.cols().saturating_sub(2).max(10)
                }
            };
            let mut rows: Vec<String> = Vec::new();
            let mut cur = String::new();
            for word in line.split_whitespace() {
                if !cur.is_empty()
                    && cur.chars().count() + 1 + word.chars().count() > width(rows.len())
                {
                    rows.push(std::mem::take(&mut cur));
                }
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(word);
            }
            rows.push(cur);
            for (k, r) in rows.iter().enumerate() {
                let (c, row) = if flight {
                    // Row 6: the four newest messages own rows 2 to 5.
                    (40, 6 + k)
                } else if hints {
                    // The last row belongs to the command bar and the rows above to messages.
                    (if k == 0 { 1 } else { 64 }, k)
                } else {
                    (1, fb.rows().saturating_sub(rows.len() - k))
                };
                fb.text(c, row, r, AMBER);
                // "[SUPPR ignorer]" is a button (it can wrap onto the next row: the word "[SUPPR" opens it).
                if let Some(pos) = r.find("[SUPPR") {
                    let n = r.chars().count() - r[..pos].chars().count();
                    let hover = g.hit_cells(
                        c + r[..pos].chars().count(),
                        row,
                        n,
                        1,
                        Act::Key(KeyCode::Delete),
                        None,
                    );
                    if hover {
                        fb.text(c + r[..pos].chars().count(), row, &r[pos..], 0x00D0_FFD0);
                    }
                }
            }
        }
    }
    // Permanent command bar on the last row (option "Barre des commandes").
    if g.settings.hints {
        let line: String = g
            .settings
            .localize(&crate::tutorial::hint_line(g))
            .chars()
            .take(fb.cols().saturating_sub(2))
            .collect();
        // The bar replaces whatever the screen drew on that row (its own footer is redundant).
        let y0 = (fb.rows().saturating_sub(1) * 10) as i32;
        for y in y0..y0 + 10 {
            for x in 0..fb.w as i32 {
                fb.px_set(x, y, 0);
            }
        }
        // Each command of the bar ("KEY what it does", separated by two spaces) is a button.
        let row = fb.rows().saturating_sub(1);
        let (mut col, mut rest) = (1usize, line.as_str());
        while !rest.is_empty() {
            let end = rest.find("  ").unwrap_or(rest.len());
            let (seg, tail) = rest.split_at(end);
            let word = seg.split_whitespace().next().unwrap_or("");
            let hover = if let Some((a, b)) = pair_of_word(word) {
                // "HAUT/BAS": the left half presses the first key, the right half the second.
                let n = word.chars().count();
                let half = n / 2;
                let h1 = g.hit_cells(col, row, half, 1, Act::Key(a), None);
                let h2 = g.hit_cells(col + half, row, n - half, 1, Act::Key(b), None);
                h1 || h2
            } else if let Some(k) = arrow_of_word(word).or_else(|| key_of_word(g.screen, word)) {
                g.hit_cells(col, row, seg.chars().count(), 1, Act::Key(k), None)
            } else {
                false
            };
            fb.text(col, row, seg, if hover { AMBER } else { 0x0040_A058 });
            let skipped = tail.len() - tail.trim_start().len();
            col += seg.chars().count() + skipped;
            rest = tail.trim_start();
        }
    }
    if g.help_open && g.screen != Screen::MainMenu {
        let shown = g.help_for.unwrap_or(g.screen);
        let lesson: &[&str] = if shown == Screen::Flight && g.sim.surface_state().is_some() {
            crate::tutorial::landing_help()
        } else {
            &[]
        };
        let wrap_at = fb.cols().saturating_sub(12).min(110);
        let mut lines: Vec<String> = lesson
            .iter()
            .chain(crate::tutorial::help_lines(shown).iter())
            .map(|l| g.settings.localize(l))
            .flat_map(|l| crate::hub::wrap(&l, wrap_at))
            .collect();
        lines.extend(crate::hub::wrap(
            "SOURIS : CLIC DROIT SUR UN ELEMENT = SES ACTIONS ; SURVOL = INFOBULLE ; MOLETTE = LISTE",
            wrap_at,
        ));
        let w = lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(10)
            .max(30)
            + 2;
        let x0 = fb.cols().saturating_sub(w + 2);
        let y0 = 4;
        let (px, py) = ((x0 * 8) as f32 - 4.0, (y0 * 10) as f32 - 4.0);
        let (pw, ph) = ((w * 8) as f32 + 8.0, ((lines.len() + 3) * 10) as f32 + 8.0);
        // A click anywhere on the panel closes it (the zones below it are not reachable).
        g.hit(
            (px as i32, py as i32, (px + pw) as i32, (py + ph) as i32),
            Act::Key(KeyCode::F1),
            None,
        );
        for y in 0..ph as i32 {
            for x in 0..pw as i32 {
                fb.px_set((px as i32) + x, (py as i32) + y, 0x0008_1018);
            }
        }
        fb.line(px, py, px + pw, py, AMBER);
        fb.line(px, py + ph, px + pw, py + ph, AMBER);
        fb.line(px, py, px, py + ph, AMBER);
        fb.line(px + pw, py, px + pw, py + ph, AMBER);
        let title = if shown == g.screen {
            "AIDE  [F1 OU CLIC : FERMER]".to_string()
        } else {
            format!(
                "AIDE  [F1 OU CLIC : FERMER]  {}",
                crate::hub::screen_label(shown)
            )
        };
        let over = g.hit_cells(
            x0 + 1,
            y0,
            title.chars().count(),
            1,
            Act::Key(KeyCode::F1),
            None,
        );
        fb.text(x0 + 1, y0, &title, if over { 0x00D0_FFD0 } else { AMBER });
        for (i, l) in lines.iter().enumerate() {
            fb.text(x0 + 1, y0 + 2 + i, l, GREEN);
        }
    }
}

fn draw_rules(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 2, "== REGLES DE PARTIE ==", AMBER);
    let r = g.pending_rules.describe();
    for (i, (label, val)) in r.iter().enumerate() {
        let c = if i == g.rules_sel { AMBER } else { GREEN };
        let mark = if i == g.rules_sel { ">" } else { " " };
        let row = 5 + i * 2;
        let hover = g.hit_cells(
            4,
            row,
            34,
            1,
            Act::Call(Game::click_rules_row, i * 3 + 1),
            None,
        );
        g.hit_cells(38, row, 2, 1, Act::Call(Game::click_rules_row, i * 3), None);
        g.hit_cells(
            41 + val.len(),
            row,
            2,
            1,
            Act::Call(Game::click_rules_row, i * 3 + 2),
            None,
        );
        let c = if hover && i != g.rules_sel { HOVER } else { c };
        fb.text(4, row, &format!("{mark} {label}"), c);
        fb.text(38, row, "<", AMBER);
        fb.text(40, row, val, c);
        fb.text(41 + val.len(), row, ">", AMBER);
    }
    if g.pending_rules.is_modified() {
        fb.text(
            4,
            5 + r.len() * 2 + 1,
            "PARTIE MODIFIEE : pas de classement, a titre de bac a sable.",
            RED,
        );
    }
    foot(
        fb,
        g,
        4,
        fb.rows() - 3,
        "HAUT/BAS CHOISIR  GAUCHE/DROITE MODIFIER  R DEFAUT  ECHAP RETOUR",
    );
}

fn draw_options(fb: &mut Framebuffer, g: &Game) {
    fb.text(2, 2, "== OPTIONS ==", AMBER);
    for (i, (label, val)) in g.settings.describe().iter().enumerate() {
        let c = if i == g.opt_sel { AMBER } else { GREEN };
        let mark = if i == g.opt_sel { ">" } else { " " };
        let row = 5 + i * 2;
        let hover = g.hit_cells(
            4,
            row,
            34,
            1,
            Act::Call(Game::click_options_row, i * 3 + 1),
            None,
        );
        g.hit_cells(
            38,
            row,
            2,
            1,
            Act::Call(Game::click_options_row, i * 3),
            None,
        );
        g.hit_cells(
            41 + val.len(),
            row,
            2,
            1,
            Act::Call(Game::click_options_row, i * 3 + 2),
            None,
        );
        let c = if hover && i != g.opt_sel { HOVER } else { c };
        fb.text(4, row, &format!("{mark} {label}"), c);
        fb.text(38, row, "<", AMBER);
        fb.text(40, row, val, c);
        fb.text(41 + val.len(), row, ">", AMBER);
    }
    foot(
        fb,
        g,
        4,
        fb.rows() - 3,
        "HAUT/BAS CHOISIR  GAUCHE/DROITE MODIFIER  ECHAP RETOUR",
    );
}

fn draw_codex(fb: &mut Framebuffer, g: &Game) {
    use crate::codex;
    fb.text(2, 1, "== CODEX ==", AMBER);
    // The tabs wrap on a second row when they do not fit.
    let (mut x, mut tab_row) = (2usize, 2usize);
    for (i, c) in codex::CATEGORIES.iter().enumerate() {
        if x + c.len() > fb.cols().saturating_sub(2) {
            x = 2;
            tab_row += 1;
        }
        let hover = g.hit_cells(
            x,
            tab_row,
            c.len(),
            1,
            Act::Call(Game::click_codex_cat, i),
            None,
        );
        let col = if i == g.codex_cat || hover {
            AMBER
        } else {
            DIM
        };
        fb.text(x, tab_row, c, col);
        x += c.len() + 2;
    }
    // The list starts under the last row of tabs (they wrap on as many rows as they need).
    let top = (tab_row + 2).max(5);
    let names = codex::names(&g.data, g.codex_cat);
    let rows = fb.rows().saturating_sub(top + 5);
    let sel = g.codex_sel.min(names.len().saturating_sub(1));
    let start = sel
        .saturating_sub(rows / 2)
        .min(names.len().saturating_sub(rows));
    for (k, n) in names.iter().enumerate().skip(start).take(rows) {
        let hover = g.hit_cells(
            2,
            top + k - start,
            34,
            1,
            Act::Call(Game::click_codex_sel, k),
            None,
        );
        let c = if k == sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        let mark = if k == sel { ">" } else { " " };
        let mut s = format!("{mark} {n}");
        s.truncate(34);
        fb.text(2, top + k - start, &s, c);
    }
    // The text keeps to the left of the turning wireframe of the entry.
    let shape = crate::wire_shape::shape_for(&g.data, g.codex_cat, sel);
    let w = fb.cols().saturating_sub(40).max(20);
    let w = if shape.is_some() { w.min(82) } else { w };
    let mut row = top;
    let max_row = fb.rows().saturating_sub(4);
    for (i, l) in codex::entry(&g.data, g.codex_cat, sel).iter().enumerate() {
        let mut cur = String::new();
        for word in l.split_whitespace() {
            if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > w {
                if row < max_row {
                    fb.text(38, row, &cur, if i == 0 { AMBER } else { GREEN });
                }
                row += 1;
                cur.clear();
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
        if row < max_row {
            fb.text(38, row, &cur, if i == 0 { AMBER } else { GREEN });
        }
        row += 1;
    }
    if let Some(segs) = &shape {
        let t = crate::proto_view::anim_time();
        if g.codex_cat == 3 {
            // A hull is shown solid, as the third-person view draws it.
            if let Some(h) = codex::source_index(&g.data, 3, sel) {
                let d = crate::ship_model::showcase_design(&g.data, h);
                let st = crate::ship_model::State::idle(&d);
                crate::ship_model::turntable(
                    fb,
                    &g.data,
                    &d,
                    &st,
                    1140,
                    150,
                    200,
                    f64::from(t) * 0.6,
                );
            }
        } else {
            crate::wire_shape::draw(fb, segs, 1140.0, 150.0, 190.0, t * 0.8, GREEN);
        }
    }
    // Cross-links: entries named in this one.
    let links = codex::links(&g.data, g.codex_cat, sel);
    if !links.is_empty() {
        row += 1;
        if row < max_row {
            fb.text(38, row, "VOIR AUSSI (CLIC OU CHIFFRE) :", AMBER);
        }
        let mut x = 38;
        row += 1;
        for (n, (c, j, name)) in links.iter().enumerate() {
            let label = format!(
                "[{}{}: {}]",
                if n < 9 {
                    format!("{} ", n + 1)
                } else {
                    String::new()
                },
                codex::CATEGORIES[*c],
                name.to_uppercase()
            );
            if x + label.chars().count() > 38 + w {
                x = 38;
                row += 1;
            }
            if row >= max_row {
                break;
            }
            let hover = g.hit_cells(
                x,
                row,
                label.chars().count(),
                1,
                Act::Call(Game::follow_codex_link, c * 10_000 + j),
                None,
            );
            fb.text(x, row, &label, if hover { AMBER } else { DIM });
            x += label.chars().count() + 1;
        }
    }
    if g.codex_cat == codex::LAB_CATEGORY {
        let label = "[LANCER UN LOT (L)]";
        let hover = g.hit_cells(
            2,
            fb.rows() - 5,
            label.len(),
            1,
            Act::Call(Game::codex_lab, 0),
            None,
        );
        fb.text(2, fb.rows() - 5, label, if hover { AMBER } else { GREEN });
        for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
            fb.text(26, fb.rows() - 5 + i, msg, if i == 0 { AMBER } else { DIM });
        }
    }
    foot(
        fb,
        g,
        2,
        fb.rows() - 3,
        "GAUCHE/DROITE CATEGORIE  HAUT/BAS ENTREE  RET.ARR. REVENIR  ECHAP RETOUR",
    );
}

fn draw_main(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    let scale = 6;
    let w = "VECTOR".len() as i32 * 8 * scale;
    fb.text_big((fb.w as i32 - w) / 2, 30, "VECTOR", scale, GREEN);
    fb.text(
        cols / 2 - 18,
        9,
        "SANDBOX SPATIAL EN LIGNES VECTORIELLES",
        DIM,
    );
    menu(fb, g, 13, &MAIN_MENU, g.menu_sel);
    // The seed of the quick start sits next to its line.
    let seed = if g.seed_text.is_empty() {
        "(DEFAUT)"
    } else {
        &g.seed_text
    };
    let c = if g.menu_sel == 0 { AMBER } else { DIM };
    fb.text(
        cols / 2 + 16,
        13,
        &format!("GRAINE: {seed}{}", if g.menu_sel == 0 { "_" } else { "" }),
        c,
    );
    // What the highlighted line does.
    let about = [
        "UN VOYAGEUR STANDARD : SON VAISSEAU, SON EQUIPAGE, LA GRAINE TAPEE CI-DESSUS (OU CELLE PAR DEFAUT).",
        "CHOISIR LE DEPART : SCENARIO (NAUFRAGE, MARCHAND, EXPLORATEUR...), VAISSEAU, EQUIPAGE, GALAXIE, OPTIONS.",
        "REPRENDRE LA PARTIE SAUVEGARDEE.",
        "DEGATS, CARBURANT, MORT PERMANENTE, CARTE REVELEE... LES REGLES DE LA PROCHAINE PARTIE.",
        "AFFICHAGE, SON, CLAVIER AZERTY OU QWERTY, TUTORIEL.",
        "L'ENCYCLOPEDIE DU JEU, TIREE DES DONNEES.",
        "",
    ];
    let line = about[g.menu_sel.min(about.len() - 1)];
    fb.text(cols / 2 - line.len() / 2, 29, line, DIM);
    if let Some(m) = g.messages.last().filter(|m| m.starts_with("ECHEC")) {
        fb.text(cols / 2 - m.len() / 2, fb.rows() - 5, m, RED);
    }
}

pub(crate) fn header(fb: &mut Framebuffer, g: &Game, title: &str) {
    let s = &g.sim;
    fb.text(1, 1, title, AMBER);
    fb.text(1, 2, &format!("CREDITS {:>10.0}", s.credits), GREEN);
    fb.gauge(
        1,
        3,
        "CARBURANT",
        s.ship.fuel / s.ship.stats.fuel_cap.max(1.0),
        20,
        GREEN,
    );
    fb.gauge(
        1,
        4,
        "COQUE    ",
        s.ship.hull / s.ship.stats.hull_max.max(1.0),
        20,
        GREEN,
    );
    fb.gauge(
        1,
        5,
        "SOUTE MAS",
        s.ship.cargo.mass / s.ship.cargo.cap_mass.max(1.0),
        20,
        GREEN,
    );
    fb.gauge(
        1,
        6,
        "SOUTE VOL",
        s.ship.cargo.volume / s.ship.cargo.cap_vol.max(1.0),
        20,
        GREEN,
    );
}

fn window(sel: usize, n: usize, rows: usize) -> std::ops::Range<usize> {
    let start = sel.saturating_sub(rows / 2).min(n.saturating_sub(rows));
    start..(start + rows).min(n)
}

/// The market screen: one of its tabs and the bar of the tabs.
fn draw_market(fb: &mut Framebuffer, g: &Game) {
    match g.market_tab {
        MarketTab::Open => draw_station(fb, g),
        MarketTab::Black => draw_black(fb, g),
        MarketTab::Exchange => crate::bourse_view::draw(fb, g),
        MarketTab::Brokerage => crate::trade_view::draw(fb, g),
    }
    market_tab_bar(fb, g);
}

/// The tab bar of the market, above the command lines: the open market, the black market, the
/// exchange and the tabs of the brokerage; one clickable word each, the open one in amber.
fn market_tab_bar(fb: &mut Framebuffer, g: &Game) {
    let y = fb.rows().saturating_sub(5);
    let mut x = 1;
    let words = ["MARCHE", "NOIR", "BOURSE"]
        .into_iter()
        .chain(crate::trade_view::TABS);
    for (i, label) in words.enumerate() {
        let w = label.len();
        let hover = g.hit_cells(x, y, w, 1, Act::Call(Game::click_market_tab, i), None);
        let on = match i {
            0 => g.market_tab == MarketTab::Open,
            1 => g.market_tab == MarketTab::Black,
            2 => g.market_tab == MarketTab::Exchange,
            k => g.market_tab == MarketTab::Brokerage && g.trade_ui.tab == k - 3,
        };
        let c = if on {
            AMBER
        } else if hover {
            HOVER
        } else {
            DIM
        };
        fb.text(x, y, label, c);
        x += w + 2;
    }
}

/// The black market tab: the goods the people of the station ban, with what the contact asks
/// and pays, on the stock that the open market shows (it is the same shelf).
fn draw_black(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let Some(st) = s.docked else { return };
    header(
        fb,
        g,
        &format!("STATION {} : MARCHE NOIR", s.bodies[st].name),
    );
    let Some(m) = s.markets.get(&st) else { return };
    let Some(f) = s.faction else {
        fb.text(
            1,
            8,
            "ESPACE LIBRE : ICI TOUT SE VEND AU GRAND JOUR (ONGLET MARCHE)",
            DIM,
        );
        return;
    };
    let (ask, pay, raid) = s.black_terms();
    fb.text(
        1,
        8,
        &format!(
            "{}  CONFIANCE DU CONTACT {:.0} %  RISQUE DE DESCENTE {:.0} %",
            s.data.factions[f].name.to_uppercase(),
            s.contact_trust() * 100.0,
            raid * 100.0
        ),
        GREEN,
    );
    fb.text(
        1,
        9,
        "LE CONTACT NE VEND QUE LE STOCK DE LA STATION (CELUI DU MARCHE) ET CE QUE VOUS LUI VENDEZ Y RESTE",
        DIM,
    );
    let (x, top) = (1, 11);
    fb.text(
        x,
        top,
        "BIEN INTERDIT        STOCK KG   IL VEND A   IL ACHETE   SOUTE KG",
        DIM,
    );
    let goods = g.black_goods();
    let sel = g.black_sel.min(goods.len().saturating_sub(1));
    for (line, &i) in goods
        .iter()
        .take(fb.rows().saturating_sub(top + 9))
        .enumerate()
    {
        let r = &g.data.resources[i];
        let hover = g.hit_cells(
            x,
            top + 1 + line,
            66,
            1,
            Act::Call(Game::click_black, line),
            None,
        );
        let c = if line == sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        let mid = m.price(&g.data, i);
        fb.text(
            x,
            top + 1 + line,
            &format!(
                "{}{:<18.18} {:>10.0} {:>11.2} {:>11.2} {:>10.0}",
                if line == sel { ">" } else { " " },
                r.name,
                m.stock[i],
                mid * ask,
                mid * pay,
                s.ship.cargo.kg[i]
            ),
            c,
        );
    }
    if goods.is_empty() {
        fb.text(
            x,
            top + 2,
            "RIEN N'EST INTERDIT ICI : TOUT SE VEND AU GRAND JOUR",
            DIM,
        );
    }
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "HAUT/BAS:CHOISIR  B:ACHETER 50 KG  S:VENDRE 50 KG  G/ECHAP:RETOUR AU MARCHE",
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_station(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let Some(st) = s.docked else { return };
    header(fb, g, &format!("STATION {}", s.bodies[st].name));
    // The good under the cursor, turning at the right of the list.
    turning(
        fb,
        crate::wire_shape::resource_shape(&g.data, g.sel_res),
        1150.0,
        200.0,
    );
    fb.text_at(
        1090,
        290,
        &g.data.resources[g.sel_res].name.to_uppercase(),
        DIM,
    );
    let Some(m) = s.markets.get(&st) else { return };
    let owner = match s.faction {
        Some(f) => format!(
            "{}  REP {:+.0}  TAXE {:.0}%",
            s.data.factions[f].name.to_uppercase(),
            s.reputation_with(Some(f)),
            s.tax_rate() * 100.0
        ),
        None => "STATION INDEPENDANTE".to_string(),
    };
    // The market list starts at column 40: keep to the left of it.
    fb.text(1, 7, &owner.chars().take(37).collect::<String>(), GREEN);
    fb.text(
        1,
        8,
        &s.data.industries[m.industry]
            .name
            .to_uppercase()
            .chars()
            .take(37)
            .collect::<String>(),
        DIM,
    );
    let (x, top) = (40, 3);
    fb.text(
        x,
        top,
        "RESSOURCE            STOCK KG     ACHAT     VENTE    SOUTE KG",
        DIM,
    );
    let rows = fb.rows().saturating_sub(top + 8);
    for (line, i) in window(g.sel_res, g.data.resources.len(), rows).enumerate() {
        let r = &g.data.resources[i];
        let hover = g.hit_cells(
            x,
            top + 1 + line,
            66,
            1,
            Act::Call(Game::click_sel_res, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let c = if i == g.sel_res {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            x,
            top + 1 + line,
            &format!(
                "{}{:<18} {:>10.0} {:>9.2} {:>9.2} {:>10.0}",
                if i == g.sel_res { ">" } else { " " },
                r.name.chars().take(18).collect::<String>(),
                m.stock[i],
                m.ask(&g.data, i),
                m.bid(&g.data, i),
                s.ship.cargo.kg[i]
            ),
            c,
        );
    }
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "ENTREE:VENDRE TOUT  S:VENDRE 100KG  B:ACHETER 100KG  C:CONTRATS  E:BANQUE",
    );
    foot_buttons(
        fb,
        g,
        1,
        b + 1,
        "T:BOURSE  D:COURTAGE  G:MARCHE NOIR  L:ELEVAGE  A:CASINO  F:CARBURANT  R:REPARER  H:VAISSEAU  Y:CHANTIER  K:EQUIPAGE  U:DECOLLER",
    );
    // The result of the last action (bought, sold, refuelled, seized...) sits just under the
    // list: the rows below it belong to the news ticker and the key hints.
    if let Some(msg) = g.messages.last() {
        fb.text(1, b - 1, msg, AMBER);
    }
}

fn draw_inventory(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "SOUTE / INVENTAIRE");
    let c = &g.sim.ship.cargo;
    let x = 40;
    fb.text(
        x,
        3,
        "RESSOURCE            MASSE KG    VOLUME M3   VALEUR BASE",
        DIM,
    );
    let items: Vec<usize> = (0..g.data.resources.len())
        .filter(|&i| c.kg[i] > 0.01)
        .collect();
    if items.is_empty() {
        fb.text(x, 5, "SOUTE VIDE - MINEZ UN ASTEROIDE [V]", DIM);
    }
    let sel = g.sel_res;
    for (line, &i) in items.iter().take(fb.rows().saturating_sub(10)).enumerate() {
        let r = &g.data.resources[i];
        let hover = g.hit_cells(x, 4 + line, 60, 1, Act::Call(Game::click_sel_res, i), None);
        let col = if i == sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(
            x,
            4 + line,
            &format!(
                "{}{:<18} {:>10.1} {:>11.2} {:>12.0}",
                if i == sel { ">" } else { " " },
                r.name,
                c.kg[i],
                c.kg[i] / r.density,
                c.kg[i] * r.value
            ),
            col,
        );
    }
    draw_hold(fb, g, &items);
    foot(
        fb,
        g,
        1,
        fb.rows() - 3,
        "HAUT/BAS:CHOISIR  J:JETER LA RESSOURCE  I/ECHAP:RETOUR",
    );
    if c.is_full() {
        fb.text(1, fb.rows() - 5, "SOUTE PLEINE", RED);
    }
}

pub fn bar(frac: f64, width: usize) -> String {
    let f = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
    format!("[{}{}]", "#".repeat(f), "-".repeat(width - f))
}

fn draw_ship(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let ship = &s.ship;
    let hull = &g.sim.data.hulls[ship.design.hull];
    header(fb, g, &format!("VAISSEAU {}  ({})", hull.name, hull.class));
    let top = 8;
    fb.text(
        1,
        top,
        "N  TYPE      MODULE                       SANTE       ALIM",
        DIM,
    );
    for (i, &(ty, size)) in hull.slots.iter().enumerate() {
        let sel = i == g.ship_sel;
        let hover = g.hit_cells(
            1,
            top + 1 + i,
            70,
            1,
            Act::Call(Game::click_ship_slot, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let c = if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        let (name, health, powered) = match ship.design.slots[i] {
            Some(m) => (
                g.data.modules[m].name.clone(),
                ship.sys.health[i],
                ship.sys.powered[i],
            ),
            None => ("(vide)".to_string(), 0.0, false),
        };
        let filled = ship.design.slots[i].is_some();
        let hc = if !filled {
            DIM
        } else if health < 0.3 {
            RED
        } else {
            c
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<2} {:<9}{} {:<28.28}",
                if sel { ">" } else { " " },
                i + 1,
                ty.label(),
                size,
                name
            ),
            c,
        );
        if filled {
            fb.text(
                46,
                top + 1 + i,
                &format!("{} {:>3.0}%", bar(health, 8), health * 100.0),
                hc,
            );
            fb.text(
                62,
                top + 1 + i,
                if powered { "ON" } else { "OFF" },
                if powered { GREEN } else { RED },
            );
        }
    }
    // Right panel: derived numbers.
    let st = &ship.stats;
    let x = 90;
    let weight = ship.mass() * G0;
    let lines = [
        format!("MASSE     {:>9.0} KG", ship.mass()),
        format!("POUSSEE   {:>9.0} KN", s.effective.thrust / 1e3),
        format!("TWR       {:>9.2}", st.thrust / weight),
        format!("DELTA-V   {:>9.1} KM/S", ship.delta_v() / 1e3),
        format!("ISP       {:>9.0} S", st.isp),
        format!("CARBURANT {:>9.0}/{:.0}", ship.fuel, st.fuel_cap),
        format!("PRODUCT.  {:>9.1} MW", s.ship.sys.supply_kw / 1e3),
        format!("DEMANDE   {:>9.1} MW", s.ship.sys.demand_kw / 1e3),
        format!("CAPACITE  {:>9.1} MW", st.power_capacity / 1e3),
        format!(
            "BATTERIE  {:>9.0}/{:.0} MJ",
            ship.sys.battery_mj, st.battery_cap
        ),
        format!(
            "RADIATEUR {:>9.1} MW",
            (st.radiate_kw + 30.0 + 0.004 * st.mass_dry) / 1e3
        ),
        format!("TEMP      {:>9.0} K", ship.sys.temp_k),
        format!("COQUE     {:>9.0}/{:.0}", ship.hull, st.hull_max),
        format!("MINAGE    {:>9.0} KG/S", st.mining_rate),
        format!("SOUTE     {:>9.0} KG", st.cargo_kg),
        format!("EQUIPAGE  {:>9.0} PLACES", st.crew_cap),
    ];
    for (i, l) in lines.iter().enumerate() {
        fb.text(x, top + i, l, GREEN);
    }
    draw_ship_schematic(fb, g);
    if let Some(m) = g.sim.ship.design.slots.get(g.ship_sel).copied().flatten() {
        turning(
            fb,
            crate::wire_shape::module_shape(&g.data, m),
            380.0,
            470.0,
        );
        fb.text(2, 34, &g.data.modules[m].name.to_uppercase(), DIM);
    }
    if !g.picker {
        crate::power_view::draw(fb, g, 720, 330);
    }
    if g.picker {
        let list = g.compatible_modules(g.ship_sel);
        let y0 = top + hull.slots.len() + 3;
        fb.text(
            1,
            y0 - 1,
            "CHOISIR UN MODULE (ENTREE = INSTALLER / ACHETER, V = REVENDRE UN EXEMPLAIRE EN STOCK, ECHAP = ANNULER)",
            AMBER,
        );
        let rows = fb.rows().saturating_sub(y0 + 4);
        let start = g
            .pick_sel
            .saturating_sub(rows / 2)
            .min(list.len().saturating_sub(rows));
        for (line, k) in (start..(start + rows).min(list.len())).enumerate() {
            let m = &g.data.modules[list[k]];
            let sel = k == g.pick_sel;
            fb.text(
                1,
                y0 + line,
                &format!(
                    "{}{:<26.26} {:<12.12} T{} {:>8.0} KG {:>9.0} CR  STOCK {}{}",
                    if sel { ">" } else { " " },
                    m.name,
                    m.category,
                    m.size,
                    m.mass,
                    m.cost,
                    s.module_stock[list[k]],
                    if s.is_unlocked("module", &m.id) {
                        ""
                    } else {
                        "  (VERROUILLE)"
                    }
                ),
                if sel { AMBER } else { GREEN },
            );
        }
    }
    let b = fb.rows() - 4;
    let help = if s.docked.is_some() {
        "HAUT/BAS:EMPLACEMENT ENTREE:INSTALLER X:RETIRER Y:CHANTIER NAVAL H/ECHAP:RETOUR"
    } else {
        "HAUT/BAS:EMPLACEMENT   (AMARREZ-VOUS A UNE STATION POUR MODIFIER)   H/ECHAP:RETOUR"
    };
    foot(fb, g, 1, b, help);
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
    if ship.sys.overheating {
        fb.text(x, top + lines.len() + 1, "*** SURCHAUFFE ***", RED);
    }
}

fn draw_shipyard(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "CHANTIER NAVAL");
    crate::designer_view::tabs(fb, g, 0);
    let cur = g.sim.ship.design.hull;
    let trade = g.sim.data.hulls[cur].cost * RESALE;
    let top = 8;
    fb.text(
        1,
        top,
        "COQUE                    CLASSE        MASSE    EMPL.    PRIX (REPRISE DEDUITE)",
        DIM,
    );
    {
        let d = crate::ship_model::showcase_design(&g.data, g.yard_sel.min(g.data.hulls.len() - 1));
        let st = crate::ship_model::State::idle(&d);
        let t = f64::from(crate::proto_view::anim_time());
        crate::ship_model::turntable(fb, &g.data, &d, &st, 1130, 200, 260, t * 0.6);
    }
    for (i, h) in g.data.hulls.iter().enumerate().take(g.sim.base_hulls) {
        let sel = i == g.yard_sel;
        let hover = g.hit_cells(
            1,
            top + 1 + i,
            100,
            1,
            Act::Call(Game::click_yard, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<24.24} {:<12.12} {:>7.0} {:>6} {:>12.0} CR{}{}",
                if sel { ">" } else { " " },
                h.name,
                h.class,
                h.mass,
                h.slots.len(),
                h.cost - trade,
                if i == cur { "  (ACTUELLE)" } else { "" },
                if s_unlocked(g, &h.id) {
                    ""
                } else {
                    "  (VERROUILLEE)"
                }
            ),
            if sel {
                AMBER
            } else if hover {
                HOVER
            } else {
                GREEN
            },
        );
    }
    let b = fb.rows() - 4;
    foot(fb, g, 1, b, "ENTREE:ACHETER (MODULES -> STOCK, SOUTE VIDE REQUISE)  C:CONCEVOIR SON VAISSEAU  ECHAP:RETOUR");
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn stamp(t: f64) -> String {
    let day = (t / 86_400.0).floor();
    let hour = ((t % 86_400.0) / 3600.0).floor();
    format!("J{:<4}{:02}h", day as i64, hour as i64)
}

/// The tab bar of the crew screen, above the command line at the foot of the screen: one
/// clickable word per tab, the open one in amber.
fn crew_tab_bar(fb: &mut Framebuffer, g: &Game) {
    let y = fb.rows().saturating_sub(6);
    let mut x = 1;
    for (i, tab) in CrewTab::ALL.iter().enumerate() {
        let w = tab.label().len();
        let hover = g.hit_cells(x, y, w, 1, Act::Call(Game::click_crew_tab, i), None);
        let c = if *tab == g.crew_tab {
            AMBER
        } else if hover {
            HOVER
        } else {
            DIM
        };
        fb.text(x, y, tab.label(), c);
        x += w + 2;
    }
}

fn draw_crew(fb: &mut Framebuffer, g: &Game) {
    crew_tab_bar(fb, g);
    match g.crew_tab {
        CrewTab::Family => return crate::family_view::draw(fb, g),
        CrewTab::Recruit => return draw_recruit(fb, g),
        _ => {}
    }
    let s = &g.sim;
    header(fb, g, "EQUIPAGE");
    let crew = &s.crew;
    fb.text(
        28,
        2,
        &format!(
            "MEMBRES {}/{:.0}  AIR {:.0}%  PAIE {:.0} CR/JOUR",
            crew.count(),
            s.ship.stats.crew_cap,
            crew.air * 100.0,
            crew.daily_payroll()
        ),
        GREEN,
    );
    if g.crew_tab == CrewTab::Network {
        crate::crew_net_view::draw(fb, g);
        return;
    }
    if g.crew_tab == CrewTab::Medicine {
        crate::medicine_view::draw(fb, g);
        return;
    }
    if g.crew_tab == CrewTab::Cryo {
        crate::cryo_view::draw(fb, g);
        return;
    }
    let top = 8;
    fb.text(
        1,
        top,
        "NOM                  POSTE          Q  SANTE    MORAL    FATIGUE  FAIM     ETAT",
        DIM,
    );
    for (i, p) in crew.members.iter().enumerate() {
        let sel = i == g.crew_sel;
        // A click selects, a double click opens the family tree.
        let hover = g.hit_cells(
            1,
            top + 1 + i,
            96,
            1,
            Act::Call(Game::click_crew, i),
            Some(Act::Call(Game::open_family_of, i)),
        );
        let c = if !p.alive {
            DIM
        } else if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        let state = if !p.alive {
            "MORT".to_string()
        } else if let Some((d, left)) = p.disease {
            format!("{} {:.0}J", g.data.diseases[d].name, left)
        } else if p.on_duty(s.time) {
            "AU TRAVAIL".to_string()
        } else {
            "REPOS".to_string()
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<19.19} {:<14.14} {}  {} {} {} {} {}",
                if sel { ">" } else { " " },
                p.name,
                g.data.roles[p.role].name,
                p.shift + 1,
                bar(f64::from(p.health), 6),
                bar(f64::from(p.morale), 6),
                bar(f64::from(p.fatigue), 6),
                bar(f64::from(p.hunger), 6),
                state
            ),
            c,
        );
    }
    if let Some(p) = crew.members.get(g.crew_sel) {
        let x = 100;
        fb.text(
            x,
            top,
            &format!("{} - {}", p.name, g.data.cultures[p.culture].name),
            AMBER,
        );
        fb.text(
            x,
            top + 1,
            &format!(
                "AGE {:.0}  SALAIRE {:.0} CR/J  LOYAUTE {:.0}%",
                p.age,
                p.salary,
                p.loyalty * 100.0
            ),
            GREEN,
        );
        let traits: Vec<&str> = p
            .traits
            .iter()
            .map(|&t| g.data.traits[t].name.as_str())
            .collect();
        fb.text(x, top + 2, &format!("TRAITS: {}", traits.join(", ")), GREEN);
        let mut sk: Vec<(usize, f32)> = p.skills.iter().copied().enumerate().collect();
        sk.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (k, (si, v)) in sk.iter().take(6).enumerate() {
            fb.text(
                x,
                top + 4 + k,
                &format!(
                    "{:<13} {} {:.1}",
                    g.data.skills[*si].name,
                    bar(f64::from(*v) / 10.0, 10),
                    v
                ),
                GREEN,
            );
        }
        fb.text(
            x,
            top + 30,
            &format!("INSTABILITE GENETIQUE {:.0} %", p.instability * 100.0),
            if p.instability > 0.5 { RED } else { DIM },
        );
        // Equipment: what each slot holds and what it brings.
        let ey = top + 40;
        fb.text(x, ey, "EQUIPEMENT", AMBER);
        for (slot, label) in ["ARME", "ARMURE", "OUTIL"].iter().enumerate() {
            let worn = p.gear.get(slot).copied().flatten();
            g.hit_cells(
                x,
                ey + 1 + slot,
                44,
                1,
                Act::Call(Game::click_gear_slot, slot),
                None,
            );
            fb.text(
                x,
                ey + 1 + slot,
                &format!(
                    "{:<7}{}",
                    label,
                    worn.map_or("-".to_string(), |gi| g.data.gear[gi].name.to_uppercase())
                ),
                if worn.is_some() { GREEN } else { DIM },
            );
        }
        fb.text(
            x,
            ey + 4,
            &format!(
                "ATTAQUE +{:.1}  DEFENSE {:.0} %  RECOLTE +{:.0} %",
                p.gear_stat(&g.data, "attack"),
                p.gear_stat(&g.data, "defense") * 100.0,
                p.gear_stat(&g.data, "gather") * 100.0
            ),
            DIM,
        );
        if g.crew_tab == CrewTab::Gear {
            if let Some(it) = g.data.gear.get(g.gear_sel) {
                let held = g
                    .data
                    .resource_index(&format!("gear_{}", it.id))
                    .map_or(0.0, |r| g.sim.ship.cargo.kg[r]);
                fb.text(
                    x,
                    ey + 5,
                    &format!(
                        "< {} >  EN SOUTE : {:.0}  ENTREE EQUIPER  U RETIRER",
                        it.name.to_uppercase(),
                        held
                    ),
                    AMBER,
                );
            }
        }
        // Implants installed, and the type picked with Left/Right in the organ list.
        let iy = top + 31;
        fb.text(x, iy, "IMPLANTS", AMBER);
        for (k, &c) in p.implants.iter().enumerate() {
            let on = p.cyber_ok && !(g.data.cybernetics[c].hackable && p.hacked);
            fb.text(
                x,
                iy + 1 + k,
                &format!(
                    "{}{}",
                    g.data.cybernetics[c].name.to_uppercase(),
                    if on { "" } else { "  (ARRET)" }
                ),
                if on { GREEN } else { RED },
            );
        }
        if g.crew_tab == CrewTab::Organs {
            if let Some(c) = g.data.cybernetics.get(g.cyber_sel) {
                fb.text(
                    x,
                    iy + 6,
                    &format!("< {} >  I POSER  U RETIRER", c.name.to_uppercase()),
                    AMBER,
                );
            }
        }
        // Organs: integrity of each, red when failing.
        fb.text(x, top + 19, "ORGANES", AMBER);
        for (k, (def, v)) in g.data.organs.iter().zip(&p.organs).enumerate() {
            let hover = g.hit_cells(
                x,
                top + 20 + k,
                24,
                1,
                Act::Call(Game::click_organ, k),
                None,
            );
            let picked = g.crew_tab == CrewTab::Organs && k == g.organ_sel;
            let kind = match p.organ_kind.get(k) {
                Some(1) => 'G',
                Some(2) => 'P',
                _ => ' ',
            };
            let col = if picked || hover {
                AMBER
            } else if *v <= 0.0 {
                RED
            } else if *v < 0.5 {
                AMBER
            } else {
                GREEN
            };
            fb.text(
                x,
                top + 20 + k,
                &format!(
                    "{}{:<8.8}{} {:>3.0}%{}",
                    if picked { '>' } else { ' ' },
                    def.name.to_uppercase(),
                    bar(f64::from(*v), 8),
                    v * 100.0,
                    kind
                ),
                col,
            );
        }
        let mut y = top + 11;
        fb.text(x, y, "RELATIONS:", DIM);
        for (oid, r) in p.relations.iter().take(6) {
            if let Some(o) = crew.members.iter().find(|m| m.id == *oid) {
                y += 1;
                fb.text(
                    x,
                    y,
                    &format!("{:<19.19} {:+}", o.name, r),
                    if *r < 0 { RED } else { GREEN },
                );
            }
        }
    } else {
        fb.text(
            1,
            top + 2,
            "AUCUN MEMBRE D EQUIPAGE - RECRUTEZ A UNE STATION [N]",
            DIM,
        );
    }
    let b = fb.rows() - 4;
    foot(fb, g, 1, b, "HAUT/BAS:CHOISIR  GAUCHE/DROITE:POSTE  T:QUART  M:MEDECINE  X:DEBARQUER (A QUAI)  J:JOURNAL  N:RECRUTER  K/ECHAP:RETOUR");
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_recruit(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "RECRUTEMENT");
    let cands = g.sim.candidates();
    let top = 8;
    fb.text(1, top, "NOM                  CULTURE       AGE  MEILLEURE COMPETENCE     TRAITS                      EMBAUCHE", DIM);
    for (i, p) in cands.iter().enumerate() {
        let sel = i == g.recruit_sel;
        g.hit_cells(
            1,
            top + 1 + i,
            120,
            1,
            Act::Call(Game::click_recruit, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let (bi, bv) = p.best_skill();
        let traits: Vec<&str> = p
            .traits
            .iter()
            .map(|&t| g.data.traits[t].name.as_str())
            .collect();
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<19.19} {:<13.13} {:>3.0}  {:<14.14} {:>4.1}   {:<27.27} {:>6.0} CR",
                if sel { ">" } else { " " },
                p.name,
                g.data.cultures[p.culture].name,
                p.age,
                g.data.skills[bi].name,
                bv,
                traits.join(", "),
                vector_sim::crew::hire_cost(p)
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    fb.text(
        1,
        top + 9,
        &format!(
            "COUCHETTES: {}/{:.0}   (LE NOUVEAU MEMBRE PREND LE POSTE DE SA MEILLEURE COMPETENCE)",
            g.sim.crew.count(),
            g.sim.ship.stats.crew_cap
        ),
        DIM,
    );
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "HAUT/BAS:CHOISIR  ENTREE:EMBAUCHER  K:LISTE  ECHAP:RETOUR A LA STATION",
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

/// What waits for an answer (a surprise with choices): its words and the keys to answer.
/// Returns the number of rows taken at the bottom of the screen.
fn draw_pending(fb: &mut Framebuffer, g: &Game) -> usize {
    let Some((text, labels)) = g.sim.pending_surprise(0) else {
        return 0;
    };
    let waiting = g.sim.surprises.pending.len();
    let top = fb.rows().saturating_sub(10);
    fb.text(
        1,
        top,
        &format!("SURPRISE ({waiting} EN ATTENTE) : CLIQUEZ UNE REPONSE (OU TOUCHES 1 A 3)"),
        AMBER,
    );
    fb.text(1, top + 1, &text.to_uppercase(), GREEN);
    for (k, l) in labels.iter().enumerate() {
        let line = format!("{} : {}", k + 1, l.to_uppercase());
        // A click answers, like the key.
        let key = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3][k.min(2)];
        let hover = g.hit_cells(3, top + 2 + k, line.chars().count(), 1, Act::Key(key), None);
        fb.text(3, top + 2 + k, &line, if hover { HOVER } else { AMBER });
    }
    5
}

fn draw_journal(fb: &mut Framebuffer, g: &Game) {
    if g.journal_tab == 2 {
        crate::archive_view::draw(fb, g);
        return;
    }
    if g.journal_tab == 1 {
        draw_news(fb, g);
        return;
    }
    fb.text(1, 1, "JOURNAL DE BORD", AMBER);
    fb.text(30, 1, "[TAB] NOUVELLES DU MONDE   (PUIS ARCHIVES)", DIM);
    let log = &g.sim.crew.log;
    let taken = draw_pending(fb, g);
    let rows = fb.rows().saturating_sub(6 + taken + 1);
    let end = log.len().saturating_sub(g.journal_scroll.min(log.len()));
    let start = end.saturating_sub(rows);
    for (i, e) in log[start..end].iter().enumerate() {
        fb.text(1, 3 + i, &format!("{}  {}", stamp(e.time), e.text), GREEN);
    }
    if log.is_empty() {
        fb.text(1, 3, "(RIEN A SIGNALER)", DIM);
    }
    foot(
        fb,
        g,
        1,
        fb.rows() - 3,
        "HAUT/BAS:DEFILER  1-3:REPONDRE A UNE SURPRISE  K:EQUIPAGE  J/ECHAP:RETOUR",
    );
}

/// The news heard so far, newest first, with how far away it happened.
fn draw_news(fb: &mut Framebuffer, g: &Game) {
    fb.text(1, 1, "NOUVELLES DU MONDE", AMBER);
    fb.text(30, 1, "[TAB] ARCHIVES", DIM);
    fb.text(
        1,
        2,
        "TOUT CE QUI SE DIT EST VRAI ; PLUS C'EST LOIN, PLUS CA ARRIVE TARD.",
        DIM,
    );
    let taken = draw_pending(fb, g);
    let rows = fb.rows().saturating_sub(7 + taken + 1);
    let list: Vec<_> = g.sim.news.iter().rev().collect();
    let start = g.journal_scroll.min(list.len().saturating_sub(1));
    for (i, n) in list.iter().skip(start).take(rows).enumerate() {
        fb.text(
            1,
            4 + i,
            &format!(
                "{}  {:<8}{:<12} {:>5.0} AL  {}",
                stamp(n.time),
                n.scale(),
                vector_sim::news::TAGS[usize::from(n.tag).min(vector_sim::news::TAGS.len() - 1)],
                n.dist_ly,
                n.text
            ),
            tag_color(n.tag),
        );
    }
    if list.is_empty() {
        fb.text(1, 4, "(AUCUNE NOUVELLE POUR L'INSTANT)", DIM);
    }
    foot(
        fb,
        g,
        1,
        fb.rows() - 4,
        "HAUT/BAS:DEFILER  1-3:REPONDRE A UNE SURPRISE  TAB:JOURNAL  J/ECHAP:RETOUR",
    );
}

fn class_color(c: vector_world::StarClass) -> u32 {
    use vector_world::StarClass::*;
    match c {
        O | B => 0x0070_90FF,
        A => 0x00D0_D8FF,
        F => 0x00E8_E8A0,
        G => 0x00F0_D860,
        K => 0x00F0_9040,
        M => 0x00FF_5040,
        D => 0x00C8_D0E0,
        N => 0x00A0_E0FF,
        X => 0x0090_5030,
        R => 0x00A0_4030,
    }
}

fn feature_color(k: vector_world::cosmos::FeatureKind) -> u32 {
    use vector_world::cosmos::FeatureKind::*;
    match k {
        EmissionNebula => 0x00FF_4050,
        ReflectionNebula => 0x0060_A0FF,
        PlanetaryNebula => 0x0040_FFC0,
        SupernovaRemnant => 0x00FF_A030,
        OpenCluster => 0x00A0_C0FF,
        GlobularCluster => 0x00FF_E080,
        StarFormingRegion => 0x00FF_70C0,
    }
}

fn dim_color(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn draw_galaxy(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        1,
        &format!(
            "CARTE GALACTIQUE - {} - {} ({})",
            s.galaxy.name().to_uppercase(),
            s.system.name,
            s.system.designation
        ),
        AMBER,
    );
    let range = s.jump_range();
    fb.text(
        1,
        2,
        &format!(
            "PORTEE DE SAUT {range:.1} AL   CARBURANT {:.0} KG   ETOILES CONNUES {}",
            s.ship.fuel,
            g.gal_cache.len()
        ),
        GREEN,
    );
    let (cx, cy) = (fb.w as f32 * 0.42, fb.h as f32 * 0.52);
    let zoom = g.gal_zoom as f32;
    let proj = |p: [f64; 3]| -> (f32, f32) {
        (
            cx + (p[0] - s.here_pos[0]) as f32 * zoom,
            cy + (p[2] - s.here_pos[2]) as f32 * zoom,
        )
    };
    // Jump range ring and axes.
    if range > 0.0 {
        fb.circle(cx, cy, range as f32 * zoom, DIM);
    }
    fb.line(cx - 6.0, cy, cx + 6.0, cy, RED);
    fb.line(cx, cy - 6.0, cx, cy + 6.0, RED);
    fb.text_at(cx as i32 + 8, cy as i32 + 6, "ICI", RED);
    if let Some(sg) = s.here_besieged() {
        fb.text(
            1,
            3,
            &format!(
                "BLOCUS EN COURS DANS CE SECTEUR PAR {} : LES ROUTES SONT COUPEES, LES PRIX MONTENT",
                sg.besieger
                    .map_or("UNE FLOTTE SANS DRAPEAU".to_string(), |f| g.data.factions[f]
                        .name
                        .to_uppercase())
            ),
            RED,
        );
    } else if !s.military.sieges.is_empty() {
        fb.text(
            1,
            3,
            &format!(
                "{} SECTEUR(S) ASSIEGE(S) A PORTEE (CADRES ROUGES, DEZOOMEZ : -)  GUERRE ET PAIX : X PUIS G",
                s.military.sieges.len()
            ),
            RED,
        );
    }
    // Sectors under siege: a red frame the size of the sector (240 ly), with the besieger.
    let mut drawn: Vec<vector_sim::politics::Cell> = Vec::new();
    for sg in &s.military.sieges {
        if drawn.contains(&sg.cell) {
            continue;
        }
        drawn.push(sg.cell);
        let c = vector_sim::politics::cell_center(sg.cell);
        let half = (vector_sim::politics::SECTOR_LY / 2.0) as f32 * zoom;
        let (x, y) = proj(c);
        let (l, t, r, b) = (x - half, y - half, x + half, y + half);
        for (a, bb, cc, d) in [(l, t, r, t), (r, t, r, b), (r, b, l, b), (l, b, l, t)] {
            fb.line(a, bb, cc, d, RED);
        }
        fb.text_at(
            l as i32 + 3,
            t as i32 + 3,
            &format!(
                "BLOCUS {}",
                sg.besieger
                    .map_or(String::new(), |f| g.data.factions[f].name.to_uppercase())
            ),
            RED,
        );
    }
    // The medium between the stars: nebulae, clusters, remnants, as circles of their true size.
    for f in &g.gal_features {
        let (x, y) = proj(f.pos);
        let col = feature_color(f.kind);
        fb.circle(x, y, f.radius as f32 * zoom, dim_color(col, 0.55));
        fb.circle(x, y, f.radius as f32 * zoom * 0.5, dim_color(col, 0.3));
        let name = g
            .data
            .cosmos_def(f.kind.id())
            .map_or("", |d| d.name.as_str());
        fb.text_at(
            x as i32 - 24,
            y as i32,
            &name.to_uppercase(),
            dim_color(col, 0.8),
        );
    }
    for (k, e) in g.gal_cache.iter().enumerate() {
        let (x, y) = proj(e.pos);
        let h = (e.pos[1] - s.here_pos[1]) as f32 * zoom * 0.5;
        // Click selects the star, double click starts the jump.
        g.hit(
            (
                (x - 7.0) as i32,
                (y - h - 7.0) as i32,
                (x + 7.0) as i32,
                (y - h + 7.0) as i32,
            ),
            Act::Call(Game::click_star, k),
            Some(Act::Key(KeyCode::Enter)),
        );
        let col = class_color(e.class());
        fb.line(x, y, x, y - h, DIM);
        fb.circle(x, y - h, 2.0, col);
        let visited = s.visited.contains_key(&e.id);
        if visited {
            fb.circle(x, y - h, 5.0, GREEN);
        }
        if k == g.gal_sel {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                fb.line(
                    x + sx * 9.0,
                    y - h + sy * 9.0,
                    x + sx * 5.0,
                    y - h + sy * 9.0,
                    AMBER,
                );
                fb.line(
                    x + sx * 9.0,
                    y - h + sy * 9.0,
                    x + sx * 9.0,
                    y - h + sy * 5.0,
                    AMBER,
                );
            }
            fb.text_at(
                x as i32 + 12,
                y as i32 - h as i32 - 4,
                &e.header.name,
                AMBER,
            );
        } else if zoom > 10.0 {
            fb.text_at(x as i32 + 6, y as i32 - h as i32 - 4, &e.header.name, DIM);
        }
    }
    // Info panel.
    let x = (fb.w as f32 * 0.78) as usize / 8;
    if let Some(e) = g.gal_cache.get(g.gal_sel) {
        let d = e.distance_to(s.here_pos);
        let ok = d <= range;
        fb.text(x, 4, &e.header.name, AMBER);
        fb.text(x, 5, &e.header.designation, DIM);
        fb.text(
            x,
            7,
            &format!(
                "CLASSE {}  MASSE {:.2} MSOL",
                e.header.class.letter(),
                e.header.mass_sun
            ),
            GREEN,
        );
        fb.text(
            x,
            8,
            &format!(
                "LUMINOSITE {:.3} LSOL  TEMP {:.0} K",
                e.header.luminosity, e.header.teff
            ),
            GREEN,
        );
        fb.text(x, 9, &format!("DISTANCE {d:.1} AL"), GREEN);
        fb.text(x, 14, e.header.stage.label(), AMBER);
        let owner = s.galaxy.territory(e.pos, s.data.factions.len());
        let (oname, ocol) = owner.map_or(("NON REVENDIQUE".to_string(), DIM), |f| {
            (
                s.data.factions[f].name.to_uppercase(),
                s.data.factions[f].color,
            )
        });
        fb.text(x, 10, &oname, ocol);
        fb.text(
            x,
            11,
            if ok { "A PORTEE" } else { "HORS DE PORTEE" },
            if ok { GREEN } else { RED },
        );
        fb.text(
            x,
            12,
            &format!("CARBURANT {:.0} KG", s.jump_fuel_cost(d)),
            GREEN,
        );
        fb.text(
            x,
            13,
            &format!("TRAJET {:.0} H", d * vector_sim::voyage::HOURS_PER_LY),
            GREEN,
        );
        if s.visited.contains_key(&e.id) {
            fb.text(x, 15, "SYSTEME DEJA VISITE", DIM);
        }
        fb.text(
            x,
            16,
            &s.galaxy.region_name(e.id.sector).to_uppercase(),
            GREEN,
        );
        if let Some(n) = s.galaxy.nebula_name(e.id.sector) {
            fb.text(x, 17, &n.to_uppercase(), DIM);
        }
    } else {
        fb.text(x, 4, "AUCUNE ETOILE CONNUE", DIM);
    }
    // The medium here and the nucleus of the galaxy.
    fb.text(x, 17, "MILIEU INTERSTELLAIRE", AMBER);
    let fx = s.cosmic_effects();
    fb.text(
        x,
        18,
        &format!("ICI : DOSE {:.2} GY/H", fx.radiation),
        if fx.radiation > 0.5 { RED } else { GREEN },
    );
    fb.text(
        x,
        19,
        &format!(
            "CAPTEURS x{:.2}  LABOS +{:.0} %",
            fx.sensor_mult,
            fx.interest * 100.0
        ),
        GREEN,
    );
    for (k, f) in g.gal_features.iter().take(3).enumerate() {
        let d = ((f.pos[0] - s.here_pos[0]).powi(2)
            + (f.pos[1] - s.here_pos[1]).powi(2)
            + (f.pos[2] - s.here_pos[2]).powi(2))
        .sqrt();
        let name = g
            .data
            .cosmos_def(f.kind.id())
            .map_or("", |d| d.name.as_str());
        fb.text(
            x,
            20 + k,
            &format!(
                "{} {d:.0} AL",
                name.to_uppercase().chars().take(24).collect::<String>()
            ),
            feature_color(f.kind),
        );
    }
    fb.text(
        x,
        24,
        &format!(
            "TROU NOIR CENTRAL {:.0} AL",
            vector_world::galaxy::Galaxy::centre_distance(s.here_pos),
        ),
        DIM,
    );
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "HAUT/BAS:ETOILE  +/-:ZOOM  ENTREE:SAUTER (HORS DE TOUT CHAMP DE GRAVITE)  L/ECHAP:FERMER",
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_contracts(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "CONTRATS");
    let s = &g.sim;
    let top = 8;
    fb.text(
        1,
        top,
        "OFFRES DE CETTE STATION (BESOINS EMERGENTS ET EXPORTS)",
        AMBER,
    );
    for (i, c) in g.offers.iter().enumerate() {
        let sel = i == g.contract_sel;
        g.hit_cells(
            0,
            top + 1 + i,
            100,
            1,
            Act::Call(Game::click_contract, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let name = if c.pax > 0 {
            format!(
                "{} {}{}",
                c.pax,
                vector_sim::economy::PAX_KINDS[usize::from(c.pax_kind.min(5))].0,
                if c.legs.is_empty() {
                    String::new()
                } else {
                    format!(" +{} ESCALES", c.legs.len())
                }
            )
        } else {
            g.data.resources[c.resource].name.clone()
        };
        let dest = if c.dist_ly == 0.0 {
            "ICI".to_string()
        } else {
            format!("{} ({:.1} AL)", c.dest_system, c.dist_ly)
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{} {:>5.0} KG {:<18.18} -> {:<24.24} {:>8.0} CR  {:.0} J",
                if sel { ">" } else { " " },
                if c.pax > 0 { "TRANSPORTER" } else { "LIVRER" },
                c.kg,
                name,
                dest,
                c.reward,
                (c.expires - s.time) / 86_400.0
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    if g.offers.is_empty() {
        fb.text(1, top + 1, "(AUCUNE OFFRE POUR LE MOMENT)", DIM);
    }
    let y0 = top + 3 + g.offers.len().max(1);
    fb.text(
        1,
        y0,
        &format!(
            "CONTRATS ACTIFS ({}/{})",
            s.contracts.len(),
            vector_sim::economy::MAX_ACTIVE_CONTRACTS
        ),
        AMBER,
    );
    for (i, c) in s.contracts.iter().enumerate() {
        let sel = g.offers.len() + i == g.contract_sel;
        let have = s.ship.cargo.kg[c.resource];
        let here = c.dest_star == s.here && Some(c.dest_body) == s.docked;
        fb.text(
            1,
            y0 + 1 + i,
            &format!(
                "{}{:>5.0} KG {:<16.16} -> {:<22.22} {:>8.0} CR  {:.0} J  SOUTE {:.0}{}",
                if sel { ">" } else { " " },
                c.kg,
                if c.pax > 0 {
                    format!("{} PASSAGERS", c.pax)
                } else {
                    g.data.resources[c.resource].name.clone()
                },
                c.dest_name,
                c.reward,
                (c.expires - s.time) / 86_400.0,
                have,
                if here { "  [ENTREE: LIVRER]" } else { "" }
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "HAUT/BAS:CHOISIR  ENTREE:ACCEPTER / LIVRER  X:ABANDONNER (-1 REP)  ECHAP:RETOUR",
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_bank(fb: &mut Framebuffer, g: &Game) {
    header(fb, g, "BANQUE");
    let s = &g.sim;
    let top = 8;
    fb.text(
        1,
        top,
        &format!("PRET EN COURS     {:>12.0} CR", s.bank.loan),
        GREEN,
    );
    fb.text(
        1,
        top + 1,
        &format!("PLAFOND DE CREDIT  {:>12.0} CR", s.credit_limit()),
        GREEN,
    );
    fb.text(
        1,
        top + 2,
        &format!(
            "TAUX JOURNALIER    {:>11.2} %",
            vector_sim::economy::BANK_RATE_PER_DAY * 100.0
        ),
        GREEN,
    );
    fb.text(
        1,
        top + 3,
        &format!("INTERETS PAYES     {:>12.0} CR", s.bank.total_interest),
        GREEN,
    );
    if s.bank.overdue_days > 0 {
        fb.text(
            1,
            top + 5,
            &format!(
                "PRET EN SOUFFRANCE DEPUIS {} J - SAISIE A 30 J !",
                s.bank.overdue_days
            ),
            RED,
        );
    }
    fb.text(
        1,
        top + 7,
        "LE PLAFOND AUGMENTE AVEC VOTRE REPUTATION AUPRES DES FACTIONS.",
        DIM,
    );
    let acts: [(&str, KeyCode); 7] = [
        ("[+] EMPRUNTER 1000 CR", KeyCode::ArrowUp),
        ("[-] REMBOURSER 1000 CR", KeyCode::ArrowDown),
        ("[ENTREE] TOUT REMBOURSER", KeyCode::Enter),
        (
            "[H] HYPOTHEQUER / LEVER L'HYPOTHEQUE DU VAISSEAU",
            KeyCode::KeyH,
        ),
        ("[C] ACHETER 1000 CR DE MONNAIE LOCALE", KeyCode::KeyC),
        ("[V] VENDRE LA MONNAIE LOCALE", KeyCode::KeyV),
        ("[B] BLANCHIR L'ARGENT SALE", KeyCode::KeyB),
    ];
    for (k, (label, key)) in acts.iter().enumerate() {
        let hover = g.hit_cells(1, top + 9 + k, label.len(), 1, Act::Key(*key), None);
        fb.text(1, top + 9 + k, label, if hover { AMBER } else { GREEN });
    }
    draw_economy_panel(fb, g, top + 17);
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "HAUT:EMPRUNTER 1000  BAS:REMBOURSER 1000  ENTREE:TOUT REMBOURSER  ECHAP:RETOUR",
    );
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

/// The economic dashboard of the jurisdiction the ship is in, the player's accounts and holdings.
fn draw_economy_panel(fb: &mut Framebuffer, g: &Game, y: usize) {
    let s = &g.sim;
    let m = s.local_sector();
    fb.text(
        1,
        y,
        &format!(
            "ECONOMIE LOCALE : {}",
            vector_sim::macroeco::currency_name(&g.data, m.faction)
        ),
        AMBER,
    );
    fb.text(
        1,
        y + 1,
        &format!(
            "PIB {:>9.0} CR/J  INFLATION {:>5.1} %  TAUX DIRECTEUR {:>4.1} %  CHOMAGE {:>4.1} %",
            m.gdp,
            m.inflation * 100.0,
            m.policy_rate * 100.0,
            m.unemployment * 100.0
        ),
        GREEN,
    );
    fb.text(
        1,
        y + 2,
        &format!(
            "DETTE {:>4.0} % DU PIB  CHANGE 1 UNITE = {:.3} CR  TAUX DE LA BANQUE {:.2} %/J",
            m.debt_ratio * 100.0,
            m.fx,
            m.bank_rate_per_day() * 100.0
        ),
        GREEN,
    );
    let state = if m.crash > 0.02 {
        ("KRACH EN COURS : ACTIONS EN CHUTE, CREDIT RESSERRE", RED)
    } else if m.bubble > 0.1 {
        (
            "BULLE SPECULATIVE : LES ACTIONS DEPASSENT LEUR VALEUR",
            AMBER,
        )
    } else if m.at_war {
        (
            "SECTEUR EN GUERRE : PLANCHE A BILLETS, MONNAIE FAIBLE",
            AMBER,
        )
    } else {
        ("CONJONCTURE CALME", DIM)
    };
    fb.text(1, y + 3, state.0, state.1);
    fb.text(
        1,
        y + 4,
        &format!(
            "VALEUR NETTE {:>10.0} CR  HYPOTHEQUE {}  ARGENT SALE {:.0} CR",
            s.net_worth(),
            if s.bank.mortgage { "OUI" } else { "NON" },
            s.bank.dirty
        ),
        GREEN,
    );
    for (i, r) in s.bank.reports.iter().rev().take(3).enumerate() {
        fb.text(
            1,
            y + 5 + i,
            &format!(
                "RAPPORT MOIS {:>3} : VALEUR NETTE {:>10.0}  CREDITS {:>9.0}  PRET {:>9.0}",
                r.month, r.net_worth, r.credits, r.loan
            ),
            DIM,
        );
    }
}

fn msgs(fb: &mut Framebuffer, g: &Game, row: usize) {
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, row + i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_empire(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 1, "EMPIRE - COLONIES", AMBER);
    fb.text(
        1,
        2,
        &format!(
            "CREDITS {:.0}   COLONIES {}   FLOTTE {} VAISSEAUX   ROUTES {}",
            s.credits,
            s.colonies.len(),
            s.fleet.len(),
            s.routes.len()
        ),
        GREEN,
    );
    let total_pop: f64 = s.colonies.iter().map(|c| c.pop).sum();
    let research: f64 = s.colonies.iter().map(|c| c.research).sum();
    fb.text(
        1,
        3,
        &format!(
            "POPULATION TOTALE {:.0}   RECHERCHE {:.0} PTS   TRESORERIE {:.0} CR",
            total_pop,
            research,
            s.colonies.iter().map(|c| c.treasury).sum::<f64>()
        ),
        GREEN,
    );
    fb.text(
        1,
        4,
        &format!(
            "ECHELLE DE KARDASHEV {:.3}   PUISSANCE {:.2e} W   OUVRAGES {}",
            s.empire_kardashev(),
            s.empire_power_w(),
            s.colonies
                .iter()
                .flat_map(|c| c.projects.iter())
                .map(|p| p.units)
                .sum::<u32>()
        ),
        GREEN,
    );
    let top = 6;
    fb.text(1, top, "NOM                    SYSTEME / CORPS                 POP    MORAL     CAISSE    ENERGIE  ETAT", DIM);
    for (i, c) in s.colonies.iter().enumerate() {
        let sel = i == g.emp_sel;
        g.hit_cells(
            0,
            top + 1 + i,
            100,
            1,
            Act::Call(Game::click_empire, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let sys = if c.star == s.here {
            s.system.name.clone()
        } else {
            g.sim
                .galaxy
                .star(c.star)
                .map_or("?".into(), |e| e.header.name)
        };
        let state = if c.pop < 1.0 {
            "ABANDONNEE"
        } else if c.unrest > 0.7 {
            "EMEUTES"
        } else if c.outbreaks.iter().any(|o| o.active()) {
            "EPIDEMIE"
        } else if c.stats.food_sat < 0.5 {
            "FAMINE"
        } else if c.stats.power_sat < 0.5 && c.stats.vital_sat < 1.0 {
            "PANNE GENERALE"
        } else if c.stats.power_sat < 0.9 {
            "DELESTAGE"
        } else if c.stats.power_supply < c.stats.power_demand * 0.9 {
            "PENURIE D ENERGIE"
        } else {
            "STABLE"
        };
        let col = if state == "STABLE" {
            if sel {
                AMBER
            } else {
                GREEN
            }
        } else {
            RED
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<22.22} {:<18.18} {:<12.12} {:>6.0} {} {:>9.0} {:>3.0}/{:<3.0}  {}",
                if sel { ">" } else { " " },
                c.name,
                sys,
                c.body_name,
                c.pop,
                bar(c.happiness, 6),
                c.treasury,
                c.stats.power_supply / 1000.0,
                c.stats.power_demand / 1000.0,
                state
            ),
            col,
        );
    }
    if s.colonies.is_empty() {
        fb.text(1, top + 2, "AUCUNE COLONIE. RAMENEZ UN KIT (3 T DE MATERIAUX, 400 KG DE COLONS, VIVRES, EAU) PRES D UN CORPS,", DIM);
        fb.text(
            1,
            top + 3,
            "CIBLEZ-LE [TAB] ET APPUYEZ SUR [O] (PERMIS : 20000 CR).",
            DIM,
        );
    }
    let b = fb.rows() - 4;
    foot(
        fb,
        g,
        1,
        b,
        "HAUT/BAS:CHOISIR  ENTREE:DETAIL  F3:FLOTTE  F2/ECHAP:RETOUR",
    );
    msgs(fb, g, b + 1);
}

fn draw_colony(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let Some(c) = s.colonies.get(g.col_idx) else {
        return;
    };
    let focus = |f: u8| if g.col_focus == f { AMBER } else { GREEN };
    fb.text(
        1,
        1,
        &format!(
            "COLONIE {}  -  {}",
            c.name.to_uppercase(),
            c.body_name.to_uppercase()
        ),
        AMBER,
    );
    let docked = s.docked_colony() == Some(g.col_idx);
    fb.text(
        1,
        2,
        &format!("POP {:.0}/{:.0}  MORAL {}  AGITATION {:.0}%  CAISSE {:.0} CR  (+{:.0}/-{:.0} PAR JOUR)  {}", c.pop, c.stats.housing, bar(c.happiness, 8), c.unrest * 100.0, c.treasury, c.stats.income, c.stats.upkeep, if docked { "[AMARRE]" } else { "" }),
        GREEN,
    );
    fb.text(
        1,
        3,
        &format!(
            "ENERGIE {:.1}/{:.1} MW  BATTERIE {:.0}/{:.0} MWH  PERTES {:.0}%  EMPLOIS {:.0} / TRAVAILLEURS {:.0}  PRODUCTIVITE {:.0}%  VIVRES {:.0}%  EAU {:.0}%  RECHERCHE {:.0}  TERRAFORMAGE {:.1}%",
            c.stats.power_supply / 1000.0,
            c.stats.power_demand / 1000.0,
            c.stats.battery_charge / 1000.0,
            c.stats.battery_cap / 1000.0,
            c.stats.grid_loss * 100.0,
            c.stats.jobs,
            c.stats.workers,
            c.stats.prod_mult * 100.0,
            c.stats.food_sat * 100.0,
            c.stats.water_sat * 100.0,
            c.research,
            c.terraform * 100.0
        ),
        GREEN,
    );
    fb.text(
        74,
        1,
        "[F] CHAINES [H] SANTE [I] INDUSTRIE [G] AGRICULTURE [V] VILLE [L] LOGISTIQUE",
        DIM,
    );
    g.hit_cells(74, 1, 11, 1, Act::Key(KeyCode::KeyF), None);
    g.hit_cells(86, 1, 9, 1, Act::Key(KeyCode::KeyH), None);
    g.hit_cells(96, 1, 13, 1, Act::Key(KeyCode::KeyI), None);
    g.hit_cells(110, 1, 15, 1, Act::Key(KeyCode::KeyG), None);
    g.hit_cells(126, 1, 9, 1, Act::Key(KeyCode::KeyV), None);
    g.hit_cells(136, 1, 14, 1, Act::Key(KeyCode::KeyL), None);
    if g.col_tab != 0 {
        crate::works_view::draw(fb, g);
        return;
    }
    if g.col_flow {
        crate::flow_view::draw(fb, g);
        return;
    }
    if g.col_health {
        crate::health_view::draw(fb, g);
        return;
    }
    if g.col_proj {
        crate::project_view::draw(fb, g);
        return;
    }
    let worst = c
        .outbreaks
        .iter()
        .filter(|o| o.active())
        .max_by(|a, b| a.i.total_cmp(&b.i));
    if let Some(o) = worst {
        fb.text(
            1,
            4,
            &format!(
                "EPIDEMIE : {}  {:.0} MALADES ({:.0} %)  R EFFECTIF {:.2}  - QUARANTAINE, MEDECINE ET VACCINS LA FREINENT  [H] SUIVI",
                g.data.pathogens[o.pathogen].name.to_uppercase(),
                o.i * c.pop,
                o.i * 100.0,
                c.r_effective(&g.data, o)
            ),
            RED,
        );
    }
    if worst.is_none() {
        fb.text(
            1,
            4,
            &format!(
                "MOEURS : {}   CROYANCE : {}",
                g.data.ethos.traits(&c.culture, 4).join(", ").to_uppercase(),
                g.data
                    .religions
                    .get(c.faith)
                    .map_or("-".to_string(), |r| r.name.to_uppercase())
            ),
            DIM,
        );
    }
    // Buildings.
    let top = 5;
    fb.text(1, top, "BATIMENTS", focus(0));
    let rows = 14usize;
    let start = g
        .b_sel
        .saturating_sub(rows / 2)
        .min(c.buildings.len().saturating_sub(rows));
    for (line, k) in (start..(start + rows).min(c.buildings.len())).enumerate() {
        let b = &c.buildings[k];
        let d = &g.data.buildings[b.def];
        let sel = g.col_focus == 0 && k == g.b_sel;
        let status = if !b.built() {
            format!("{:>3.0}%", b.progress * 100.0)
        } else if !b.enabled {
            "OFF".to_string()
        } else {
            "ON".to_string()
        };
        let recipe = if d.recipes.is_empty() {
            String::new()
        } else {
            g.data.recipes[g
                .data
                .recipe_index(&d.recipes[b.recipe % d.recipes.len()])
                .unwrap_or(0)]
            .name
            .clone()
        };
        fb.text(
            1,
            top + 1 + line,
            &format!(
                "{}{:<26.26} {:>4} {:>3.0}%  {:<24.24}",
                if sel { ">" } else { " " },
                d.name,
                status,
                b.health * 100.0,
                recipe
            ),
            if sel {
                AMBER
            } else if b.built() {
                GREEN
            } else {
                DIM
            },
        );
    }
    // Storage.
    let x = 66;
    fb.text(
        x,
        top,
        &format!(
            "STOCKS {:.0}/{:.0} KG",
            c.total_stored(),
            c.stats.storage_cap.max(5000.0)
        ),
        focus(1),
    );
    let items: Vec<usize> = (0..g.data.resources.len())
        .filter(|&i| c.storage[i] > 0.5 || (g.col_focus == 1 && i == g.s_sel))
        .collect();
    for (line, &i) in items.iter().take(rows).enumerate() {
        let sel = g.col_focus == 1 && i == g.s_sel;
        g.hit_cells(
            x,
            top + 1 + line,
            32,
            1,
            Act::Call(Game::click_storage, i),
            None,
        );
        fb.text(
            x,
            top + 1 + line,
            &format!(
                "{}{:<20.20} {:>10.0}",
                if sel { ">" } else { " " },
                g.data.resources[i].name,
                c.storage[i]
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    // Policies.
    let px = 100;
    fb.text(px, top, "POLITIQUES", focus(2));
    for (i, p) in g.data.policies.iter().enumerate() {
        let sel = g.col_focus == 2 && i == g.p_sel;
        // Name selects; the arrows around the level change it.
        g.hit_cells(
            px,
            top + 1 + i,
            20,
            1,
            Act::Call(Game::click_policy, i * 3 + 1),
            None,
        );
        g.hit_cells(
            px + 20,
            top + 1 + i,
            1,
            1,
            Act::Call(Game::click_policy, i * 3),
            None,
        );
        let lvl_len = p.levels[c.policies[i]].0.chars().count();
        g.hit_cells(
            px + 23 + lvl_len,
            top + 1 + i,
            1,
            1,
            Act::Call(Game::click_policy, i * 3 + 2),
            None,
        );
        fb.text(
            px,
            top + 1 + i,
            &format!(
                "{}{:<18.18} < {} >",
                if sel { ">" } else { " " },
                p.name,
                p.levels[c.policies[i]].0
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    // Build menu.
    let by = top + rows + 2;
    g.hit_cells(
        1,
        by,
        10,
        1,
        Act::Call(Game::click_build, g.build_sel),
        None,
    );
    fb.text(1, by, "CONSTRUIRE", focus(3));
    if g.col_focus != 3 {
        // The list is one zone of the TAB cycle: say so instead of leaving an empty gap.
        fb.text(
            12,
            by,
            "(TAB OU CLIC : LISTE DES BATIMENTS A CONSTRUIRE)",
            DIM,
        );
    }
    if g.col_focus == 3 {
        let n = g.data.buildings.len();
        let show = 7usize;
        let st = g
            .build_sel
            .saturating_sub(show / 2)
            .min(n.saturating_sub(show));
        for (line, k) in (st..(st + show).min(n)).enumerate() {
            let d = &g.data.buildings[k];
            g.hit_cells(
                1,
                by + 1 + line,
                70,
                1,
                Act::Call(Game::click_build, k),
                Some(Act::Key(KeyCode::Enter)),
            );
            let ok = d.sites.iter().any(|t| c.tags.contains(t));
            let cost: Vec<String> = d
                .cost
                .iter()
                .map(|(r, kg)| format!("{} {:.0}", r, kg))
                .collect();
            fb.text(
                1,
                by + 1 + line,
                &format!(
                    "{}{:<26.26} {:>3.0} J  {}",
                    if k == g.build_sel { ">" } else { " " },
                    d.name,
                    d.days,
                    cost.join(", ")
                ),
                if !ok {
                    RED
                } else if k == g.build_sel {
                    AMBER
                } else {
                    GREEN
                },
            );
        }
    }
    // Log.
    let ly = by + 10;
    fb.text(1, ly, "JOURNAL DE LA COLONIE", DIM);
    for (i, (t, l)) in c.log.iter().rev().take(5).enumerate() {
        fb.text(1, ly + 1 + i, &format!("{} {}", stamp(*t), l), DIM);
    }
    let b = fb.rows() - 4;
    foot(fb, g, 1, b, "TAB:ZONE  HAUT/BAS  ESPACE:ON/OFF  R:RECETTE  X:DEMOLIR  D/W:DEPOSER/RETIRER (STOCKS)  GAUCHE/DROITE:POLITIQUE  ENTREE:CONSTRUIRE  T:CAISSE  U:DECOLLER");
    msgs(fb, g, b + 1);
}

fn draw_fleet(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 1, "FLOTTE DELEGUEE", AMBER);
    fb.text(
        1,
        2,
        &format!(
            "CREDITS {:.0}   ENTRETIEN {:.0} CR/J",
            s.credits,
            s.fleet.iter().map(|f| f.upkeep).sum::<f64>()
        ),
        GREEN,
    );
    if g.fleet_map {
        crate::fleet_map_view::draw(fb, g);
        return;
    }
    let top = 4;
    let focus = |f: u8| if g.fleet_focus == f { AMBER } else { GREEN };
    fb.text(1, top, "VAISSEAUX", focus(0));
    for (i, f) in s.fleet.iter().enumerate() {
        let sel = g.fleet_focus == 0 && i == g.ship_sel2;
        g.hit_cells(
            0,
            top + 1 + i,
            100,
            1,
            Act::Call(Game::click_fleet_ship, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let state = match &f.state {
            vector_sim::fleet::FleetState::Idle => "EN ATTENTE".to_string(),
            vector_sim::fleet::FleetState::Loading { .. } => "CHARGEMENT".to_string(),
            vector_sim::fleet::FleetState::Outbound { t1 } => {
                format!("ALLER (ARRIVEE J{:.0})", t1 / 86_400.0)
            }
            vector_sim::fleet::FleetState::Unloading { .. } => "DECHARGEMENT".to_string(),
            vector_sim::fleet::FleetState::Returning { t1 } => {
                format!("RETOUR (J{:.0})", t1 / 86_400.0)
            }
        };
        let route = if f.order != vector_sim::fleet::FleetOrder::Trade {
            f.order.label().to_string()
        } else {
            f.route.map_or("-".to_string(), |r| format!("ROUTE {r}"))
        };
        fb.text(
            1,
            top + 1 + i,
            &format!(
                "{}{:<26.26} {:>7.0} KG  {:<9} {:<24} VOYAGES {:>3}  GAINS {:>9.0}",
                if sel { ">" } else { " " },
                f.name,
                f.capacity_kg,
                route,
                state,
                f.trips,
                f.earnings
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    let ry = top + 3 + s.fleet.len().max(1);
    fb.text(1, ry, "ROUTES", focus(1));
    for (i, r) in s.routes.iter().enumerate() {
        let sel = g.fleet_focus == 1 && i == g.route_sel;
        fb.text(
            1,
            ry + 1 + i,
            &format!(
                "{}#{:<3} {:<28.28} -> {:<28.28} {:>6.0} KG {:<14.14} {} TRAJETS {:>3} PROFIT {:>9.0}",
                if sel { ">" } else { " " },
                r.id,
                r.from.label(s),
                r.to.label(s),
                r.kg,
                g.data.resources[r.resource].name,
                if r.active { "ACTIVE" } else { "INACTIVE" },
                r.trips,
                r.profit
            ),
            if sel { AMBER } else { GREEN },
        );
    }
    if let Some(ed) = &g.editor {
        let y = ry + 3 + s.routes.len().max(1);
        fb.text(
            1,
            y,
            "NOUVELLE ROUTE (GAUCHE/DROITE CHANGE LA VALEUR, TAB CHAMP SUIVANT, ENTREE CREE)",
            AMBER,
        );
        let label = |i: usize| g.endpoints.get(i).map_or("?".into(), |e| e.label(s));
        let f = |n: u8| if ed.field == n { AMBER } else { GREEN };
        fb.text(1, y + 1, &format!("DE       : {}", label(ed.from)), f(0));
        fb.text(1, y + 2, &format!("VERS     : {}", label(ed.to)), f(1));
        fb.text(
            1,
            y + 3,
            &format!("MARCHANDISE: {}", g.data.resources[ed.res].name),
            f(2),
        );
        fb.text(1, y + 4, &format!("LOT      : {:.0} KG", ed.kg), f(3));
    }
    if let Some(sel) = g.hull_pick {
        let y = ry + 3 + s.routes.len().max(1);
        fb.text(1, y, "ACHETER UN VAISSEAU (ENTREE, ECHAP)", AMBER);
        for (k, h) in g.data.hulls.iter().enumerate().take(g.sim.base_hulls) {
            fb.text(
                1,
                y + 1 + k,
                &format!(
                    "{}{:<26.26} {:<12.12} {:>10.0} CR",
                    if k == sel { ">" } else { " " },
                    h.name,
                    h.class,
                    h.cost * vector_sim::fleet::PURCHASE_FACTOR
                ),
                if k == sel { AMBER } else { GREEN },
            );
        }
    }
    if g.editor.is_none() && g.hull_pick.is_none() {
        crate::war_view::draw_squad(fb, g, ry + 3 + s.routes.len().max(1));
    }
    let b = fb.rows() - 4;
    foot(fb, g, 1, b, "TAB:LISTE  N:NOUVELLE ROUTE  A:AFFECTER  U:RETIRER  X:SUPPRIMER/VENDRE  B:ACHETER (A QUAI)  O:ORDRE  R:RAVITAILLER  V:REPARER  F2:EMPIRE  ECHAP:RETOUR");
    msgs(fb, g, b + 1);
}

fn s_unlocked(g: &Game, hull_id: &str) -> bool {
    g.sim.is_unlocked("hull", hull_id)
}

/// Schematic of the ship: every slot is a box wired to the power bus in the middle (green =
/// powered, red = off or damaged, dim = empty). Click a box to select the slot.
fn draw_ship_schematic(fb: &mut Framebuffer, g: &Game) {
    let ship = &g.sim.ship;
    let hull = &g.sim.data.hulls[ship.design.hull];
    let (x0, y0) = (960i32, 80i32);
    let bus = x0 + 152;
    fb.text_at(x0 + 62, y0 - 12, "SCHEMA DU VAISSEAU", DIM);
    let n = hull.slots.len();
    let per_side = n.div_ceil(2);
    // Left side takes the first half of the slots, right side the rest.
    let box_w = 128;
    let step = 28;
    fb.line(
        bus as f32,
        y0 as f32,
        bus as f32,
        (y0 + per_side as i32 * step) as f32,
        if ship.sys.overheating { RED } else { GREEN },
    );
    for (i, &(ty, size)) in hull.slots.iter().enumerate() {
        let left = i < per_side;
        let row = if left { i } else { i - per_side } as i32;
        let bx = if left { x0 } else { bus + 24 };
        let by = y0 + row * step;
        let filled = ship.design.slots[i].is_some();
        let health = ship.sys.health[i];
        let col = if !filled {
            DIM
        } else if !ship.sys.powered[i] || health < 0.3 {
            RED
        } else if i == g.ship_sel {
            AMBER
        } else {
            GREEN
        };
        let hover = g.hit(
            (bx, by, bx + box_w, by + step - 4),
            Act::Call(Game::click_ship_slot, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        let col = if hover && col != AMBER { HOVER } else { col };
        let (l, t, r, b) = (
            bx as f32,
            by as f32,
            (bx + box_w) as f32,
            (by + step - 4) as f32,
        );
        fb.line(l, t, r, t, col);
        fb.line(r, t, r, b, col);
        fb.line(r, b, l, b, col);
        fb.line(l, b, l, t, col);
        // Wire to the bus.
        let wy = (by + (step - 4) / 2) as f32;
        let (wx0, wx1) = if left {
            (r, bus as f32)
        } else {
            (bus as f32, l)
        };
        fb.line(wx0, wy, wx1, wy, if filled { col } else { DIM });
        let name = ship.design.slots[i].map_or("(VIDE)", |m| g.data.modules[m].name.as_str());
        let label: String = format!(
            "{}{} {}",
            ty.label().chars().next().unwrap_or('?'),
            size,
            name
        )
        .chars()
        .take(15)
        .collect();
        fb.text_at(bx + 4, by + 3, &label, col);
        if filled {
            // Health bar along the bottom of the box.
            let w = ((box_w - 8) as f64 * health.clamp(0.0, 1.0)) as i32;
            fb.line(
                (bx + 4) as f32,
                (by + 16) as f32,
                (bx + 4 + w) as f32,
                (by + 16) as f32,
                col,
            );
        }
    }
}

/// Colour of a resource category in the hold diagram.
fn category_color(category: &str) -> u32 {
    const PALETTE: [u32; 8] = [
        0x0060_E080,
        0x0050_C0E0,
        0x00E0_C060,
        0x00E0_7050,
        0x00C0_70E0,
        0x0090_E050,
        0x00E0_90B0,
        0x0080_A0E0,
    ];
    let h = category
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[h as usize % PALETTE.len()]
}

/// The hold drawn as a grid whose cells are equal volumes: each resource fills as many cells as
/// its volume (mass / density) takes, so a dense ore is a small block and a gas a big one. A
/// cell is hatched with more lines the denser the resource. Two bars below tell which of volume
/// and mass is the limit.
/// A small turning wireframe of `segs` in the corner reserved for it.
fn turning(fb: &mut Framebuffer, segs: Option<Vec<crate::wire_shape::Seg>>, cx: f32, cy: f32) {
    if let Some(s) = segs {
        let t = crate::proto_view::anim_time();
        crate::wire_shape::draw(fb, &s, cx, cy, 180.0, t * 0.8, GREEN);
    }
}

fn draw_hold(fb: &mut Framebuffer, g: &Game, items: &[usize]) {
    turning(
        fb,
        crate::wire_shape::resource_shape(&g.data, g.sel_res),
        1120.0,
        170.0,
    );
    const CELL: i32 = 8;
    const COLS: usize = 34;
    const ROWS: usize = 22;
    let c = &g.sim.ship.cargo;
    let (x0, y0) = (16, 100);
    fb.text_at(x0, y0 - 12, "PLAN DE SOUTE (1 CASE = VOLUME EGAL)", DIM);
    let total = COLS * ROWS;
    let (w, h) = (COLS as i32 * CELL, ROWS as i32 * CELL);
    let (fx, fy) = (x0 as f32, y0 as f32);
    for (a, b, c2, d) in [
        (fx, fy, fx + w as f32, fy),
        (fx + w as f32, fy, fx + w as f32, fy + h as f32),
        (fx + w as f32, fy + h as f32, fx, fy + h as f32),
        (fx, fy + h as f32, fx, fy),
    ] {
        fb.line(a, b, c2, d, DIM);
    }
    let cap = c.cap_vol.max(1e-9);
    let mut cell = 0usize;
    for &i in items {
        let r = &g.data.resources[i];
        let n = ((c.kg[i] / r.density / cap) * total as f64).ceil().max(1.0) as usize;
        let col = if i == g.sel_res {
            AMBER
        } else {
            category_color(&r.category)
        };
        // Hatching: 1 line for a light resource, up to 4 for a dense one.
        let lines = ((r.density.max(1.0)).log10().floor() as i32).clamp(0, 3) + 1;
        let first = cell;
        for k in first..(first + n).min(total) {
            let (cx, cy) = (x0 + (k % COLS) as i32 * CELL, y0 + (k / COLS) as i32 * CELL);
            // A small box, filled from the bottom as much as the resource is dense.
            let (l, t, r, bt) = (
                (cx + 1) as f32,
                (cy + 1) as f32,
                (cx + 6) as f32,
                (cy + 6) as f32,
            );
            fb.line(l, t, r, t, col);
            fb.line(r, t, r, bt, col);
            fb.line(r, bt, l, bt, col);
            fb.line(l, bt, l, t, col);
            for f in 0..lines {
                let yy = (cy + 5 - f) as f32;
                fb.line(l, yy, r, yy, col);
            }
        }
        // One zone per row of the block, so a click anywhere on it selects the resource.
        let mut k = first;
        let end = (first + n).min(total);
        while k < end {
            let row_end = ((k / COLS + 1) * COLS).min(end);
            let (cx, cy) = (x0 + (k % COLS) as i32 * CELL, y0 + (k / COLS) as i32 * CELL);
            g.hit(
                (cx, cy, cx + (row_end - k) as i32 * CELL, cy + CELL),
                Act::Call(Game::click_sel_res, i),
                None,
            );
            k = row_end;
        }
        cell += n;
    }
    // Volume and mass gauges.
    let rows = (y0 + h) as usize / 10 + 2;
    fb.gauge(2, rows, "VOLUME", c.volume / cap, 20, GREEN);
    fb.gauge(
        2,
        rows + 1,
        "MASSE ",
        c.mass / c.cap_mass.max(1e-9),
        20,
        GREEN,
    );
    fb.text(
        2,
        rows + 2,
        &format!(
            "{:.1}/{:.1} M3   {:.0}/{:.0} KG",
            c.volume, c.cap_vol, c.mass, c.cap_mass
        ),
        DIM,
    );
    if let Some(&i) = items.iter().find(|&&i| i == g.sel_res) {
        let r = &g.data.resources[i];
        fb.text(
            2,
            rows + 3,
            &format!("{} : DENSITE {:.0} KG/M3", r.name.to_uppercase(), r.density),
            AMBER,
        );
    }
}
