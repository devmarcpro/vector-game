//! Headless scripted play: `vector-game --script file.txt` drives the real game logic with
//! key events and checks outcomes, so whole sessions can be regression-tested without a window.
//!
//! One command per line (`#` starts a comment):
//!   new <seed>                start a new game          tutorial off     disable the tutorial
//!   key <Name>                press and release         hold <Name> <s>  hold while time passes
//!   wait <s>                  let <s> real seconds pass scale <x>        set the time scale
//!   snapshot <file.png>       render the current frame  print            show a state summary
//!   expect screen <Name>      expect docked yes|no      expect credits > <n>   expect speed < <n>
//!   expect message <text>     expect target yes|no      expect fuel > <n>
//!   expect market <tab>       expect crew <tab>         (tabs of the market and crew screens)
//! Test set-up (not reachable by a player): `near asteroid|station|planet|moon` puts the ship
//! next to the closest such body with matched velocity and targets it; `give credits <n>`,
//! `give cargo <resource id> <kg>`, `give module <module id>`; `mark` remembers the current
//! star, so `expect jumped yes|no` can check a jump happened.
//! More expectations: `colonies > <n>`, `cargo <id> > <n>` (or `<`), `module <id>` (installed),
//! `ship_mass > <n>`, `credits < <n>`.
//! Player-style input and reading (see docs/JOURNAL_DE_TEST.md): `key shift+tab` (modifiers),
//! `click X Y` / `rclick X Y` (pixels of the 1280x720 frame), `click_text WORD` / `rclick_text WORD`
//! (the first drawn text that contains WORD), `type TEXT`, `text` (print what is on screen),
//! `expect_text "WORD"` / `expect_no_text "WORD"`, `shot dir/name.png`, `lint` (texts drawn over
//! one another), `post TEXT` / `messages` (the message strip), `azerty on|off`.
use crate::game::{Game, DEFAULT_SEED};
use std::sync::Arc;
use vector_data::GameData;
use vector_render::Framebuffer;
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

/// Draw the current frame with text recording on; returns the texts as (x, y, scale, text).
pub fn screen_texts(
    g: &Game,
    draw: fn(&mut Framebuffer, &Game),
) -> (Framebuffer, Vec<(i32, i32, i32, String)>) {
    let mut fb = Framebuffer::new(1280, 720);
    fb.rec = Some(Vec::new());
    draw(&mut fb, g);
    let rec = fb.rec.take().unwrap_or_default();
    (fb, rec)
}

/// The visible text, one line per text row, left to right (what a player reads).
pub fn visible_text(rec: &[(i32, i32, i32, String)]) -> String {
    let mut v: Vec<&(i32, i32, i32, String)> = rec.iter().collect();
    v.sort_by_key(|t| (t.1 / 10, t.0));
    let mut out = String::new();
    let mut row = i32::MIN;
    for t in v {
        if t.1 / 10 != row {
            if row != i32::MIN {
                out.push('\n');
            }
            row = t.1 / 10;
        } else {
            out.push_str("  ");
        }
        out.push_str(t.3.trim_end());
    }
    out
}

/// Centre in pixels of the first drawn text containing `needle` (case-insensitive).
pub fn find_text(rec: &[(i32, i32, i32, String)], needle: &str) -> Option<(i32, i32)> {
    let n = needle.to_uppercase();
    rec.iter()
        .filter(|t| t.3.to_uppercase().contains(&n))
        .min_by_key(|t| t.3.len())
        .map(|t| {
            let at = t.3.to_uppercase().find(&n).unwrap_or(0);
            let w = n.chars().count() as i32 * 8 * t.2;
            (t.0 + at as i32 * 8 * t.2 + w / 2, t.1 + 4 * t.2)
        })
}

/// Pairs of texts on the same cell row whose columns intersect (one is written over the other).
pub fn overlapping_texts(rec: &[(i32, i32, i32, String)]) -> Vec<String> {
    let cells: Vec<(i32, i32, i32, &String)> = rec
        .iter()
        .filter(|t| t.2 == 1 && t.0 % 8 == 0 && t.1 % 10 == 0 && !t.3.trim().is_empty())
        .map(|t| {
            (
                t.1 / 10,
                t.0 / 8,
                t.0 / 8 + t.3.trim_end().chars().count() as i32,
                &t.3,
            )
        })
        .collect();
    let mut out = Vec::new();
    for (i, a) in cells.iter().enumerate() {
        for b in &cells[i + 1..] {
            if a.0 == b.0 && a.1 < b.2 && b.1 < a.2 {
                // Same text drawn twice (a highlight over its own row) is not a collision.
                let la = a.3.trim_start().chars().next();
                let lb = b.3.trim_start().chars().next();
                if (a.1 == b.1
                    && (a.3.starts_with(b.3.trim_end()) || b.3.starts_with(a.3.trim_end())))
                    || la.is_none()
                    || lb.is_none()
                {
                    continue;
                }
                out.push(format!(
                    "row {}: '{}' x '{}'",
                    a.0,
                    a.3.trim_end(),
                    b.3.trim_end()
                ));
            }
        }
    }
    out
}

/// Everything after the command word, quotes stripped.
fn rest(line: &str) -> String {
    let r = line
        .split_once(char::is_whitespace)
        .map_or("", |x| x.1)
        .trim();
    r.trim_matches('"').to_string()
}

fn kind_matches(name: &str, k: &BodyKind) -> bool {
    match name {
        "asteroid" => matches!(k, BodyKind::Asteroid(_)),
        "station" => matches!(k, BodyKind::Station),
        "planet" => matches!(k, BodyKind::Planet(p) if !p.is_giant()),
        "moon" => matches!(k, BodyKind::Moon(_)),
        _ => false,
    }
}

/// Point the nose at the targeted ship.
fn aim(g: &mut Game) -> Result<(), String> {
    let t = g
        .sim
        .target_npc
        .and_then(|i| g.sim.npcs.get(i))
        .ok_or("no ship targeted")?;
    let fwd = (t.pos - g.sim.ship.pos).normalized();
    g.sim.ship.rot = vector_core::Mat3::look_at(fwd, vector_core::Vec3::new(0.0, 1.0, 0.0));
    g.sim.ship.ang = vector_core::Vec3::ZERO;
    Ok(())
}

/// Teleport next to the closest body of this kind, velocity matched, and target it.
fn near(g: &mut Game, name: &str) -> Result<(), String> {
    g.sim.target = None;
    g.cycle_target(|k| kind_matches(name, k));
    let t = g
        .sim
        .target
        .ok_or_else(|| format!("no {name} in this system"))?;
    let b = &g.sim.bodies[t];
    let stand = match name {
        "asteroid" => b.radius + 600.0,
        "station" => b.radius + 200.0,
        _ => b.radius * 1.5,
    };
    g.sim.ship.pos = b.pos - vector_core::Vec3::new(0.0, 0.0, stand);
    g.sim.ship.vel = b.vel;
    g.sim.ship.ang = vector_core::Vec3::ZERO;
    g.sim.update_ref_pub();
    Ok(())
}

pub fn key_code(name: &str) -> Option<KeyCode> {
    let n = name.to_ascii_lowercase();
    if n.len() == 1 {
        let c = n.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(
                [
                    KeyCode::KeyA,
                    KeyCode::KeyB,
                    KeyCode::KeyC,
                    KeyCode::KeyD,
                    KeyCode::KeyE,
                    KeyCode::KeyF,
                    KeyCode::KeyG,
                    KeyCode::KeyH,
                    KeyCode::KeyI,
                    KeyCode::KeyJ,
                    KeyCode::KeyK,
                    KeyCode::KeyL,
                    KeyCode::KeyM,
                    KeyCode::KeyN,
                    KeyCode::KeyO,
                    KeyCode::KeyP,
                    KeyCode::KeyQ,
                    KeyCode::KeyR,
                    KeyCode::KeyS,
                    KeyCode::KeyT,
                    KeyCode::KeyU,
                    KeyCode::KeyV,
                    KeyCode::KeyW,
                    KeyCode::KeyX,
                    KeyCode::KeyY,
                    KeyCode::KeyZ,
                ][(c as u8 - b'a') as usize],
            );
        }
    }
    Some(match n.as_str() {
        "space" => KeyCode::Space,
        "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "plus" => KeyCode::Equal,
        "minus" => KeyCode::Minus,
        "ctrl" => KeyCode::ControlLeft,
        "shift" => KeyCode::ShiftLeft,
        "f7" => KeyCode::F7,
        "delete" => KeyCode::Delete,
        "f8" => KeyCode::F8,
        "backquote" => KeyCode::Backquote,
        "f1" => KeyCode::F1,
        "f2" => KeyCode::F2,
        "f3" => KeyCode::F3,
        "f4" => KeyCode::F4,
        "f5" => KeyCode::F5,
        "f6" => KeyCode::F6,
        "f9" => KeyCode::F9,
        "f10" => KeyCode::F10,
        "f11" => KeyCode::F11,
        "f12" => KeyCode::F12,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "insert" => KeyCode::Insert,
        "backslash" => KeyCode::Backslash,
        "comma" => KeyCode::Comma,
        "period" => KeyCode::Period,
        "slash" => KeyCode::Slash,
        "numpadmultiply" => KeyCode::NumpadMultiply,
        "0" => KeyCode::Digit0,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4,
        "5" => KeyCode::Digit5,
        "6" => KeyCode::Digit6,
        "7" => KeyCode::Digit7,
        "8" => KeyCode::Digit8,
        "9" => KeyCode::Digit9,
        "numpad0" => KeyCode::Numpad0,
        "numpad1" => KeyCode::Numpad1,
        "numpad2" => KeyCode::Numpad2,
        "numpad3" => KeyCode::Numpad3,
        "numpad4" => KeyCode::Numpad4,
        "numpad5" => KeyCode::Numpad5,
        "numpad7" => KeyCode::Numpad7,
        "numpad8" => KeyCode::Numpad8,
        "numpad9" => KeyCode::Numpad9,
        _ => return None,
    })
}

fn passes(op: &str, have: f64, want: f64) -> bool {
    match op {
        ">" => have > want,
        "<" => have < want,
        _ => false,
    }
}

fn wait(g: &mut Game, seconds: f64) {
    let mut left = seconds;
    while left > 1e-9 {
        let dt = left.min(1.0 / 30.0);
        if g.running() {
            g.advance(dt);
        }
        left -= dt;
    }
}

/// Execute a script; returns the number of failed expectations (or a hard error).
pub fn run(
    path: &str,
    data: Arc<GameData>,
    draw: fn(&mut Framebuffer, &Game),
) -> Result<usize, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut g = Game::new(data, DEFAULT_SEED);
    g.settings.tutorial = false;
    // Scripts name keys the QWERTY way (W/A/S/D/Q/E).
    g.settings.azerty = false;
    let mut failures = 0;
    let mut marked_star = None;
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        let at = format!("{path}:{}", ln + 1);
        let num = |i: usize| -> Result<f64, String> {
            w.get(i)
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("{at}: number expected in '{line}'"))
        };
        let key = |i: usize| -> Result<KeyCode, String> {
            w.get(i)
                .and_then(|s| key_code(s))
                .ok_or_else(|| format!("{at}: unknown key in '{line}'"))
        };
        match w[0] {
            "new" => {
                let seed = w.get(1).map_or(DEFAULT_SEED, |s| crate::game::parse_seed(s));
                g.new_game(seed);
            }
            "tutorial" => g.settings.tutorial = w.get(1) == Some(&"on"),
            "key" => {
                // `key shift+tab`: modifiers (shift, ctrl) held around the key.
                let mut parts: Vec<&str> = w.get(1).copied().unwrap_or("").split('+').collect();
                let main = parts.pop().unwrap_or("");
                let k = key_code(main).ok_or_else(|| format!("{at}: unknown key in '{line}'"))?;
                let mods: Vec<KeyCode> = parts.iter().filter_map(|m| key_code(m)).collect();
                for &m in &mods {
                    g.key(m, None, true);
                }
                g.key(k, None, true);
                g.key(k, None, false);
                for &m in mods.iter().rev() {
                    g.key(m, None, false);
                }
            }
            "hold" => {
                let k = key(1)?;
                g.key(k, None, true);
                wait(&mut g, num(2)?);
                g.key(k, None, false);
            }
            "wait" => wait(&mut g, num(1)?),
            "scale" => g.timescale = num(1)?,
            // Player-style input: pixels, words, typed text, screen reading, screenshots.
            "click" | "rclick" => {
                let (x, y) = (num(1)? as i32, num(2)? as i32);
                let _ = screen_texts(&g, draw);
                if w[0] == "click" {
                    g.pointer_click(x, y, 0);
                } else {
                    g.pointer_move(x, y);
                    g.pointer_context(x, y);
                }
            }
            "click_text" | "rclick_text" => {
                let needle = rest(line);
                let (_, rec) = screen_texts(&g, draw);
                let Some((x, y)) = find_text(&rec, &needle) else {
                    failures += 1;
                    println!("[{at}] FAILED: no text '{needle}' to click on screen {:?}", g.screen);
                    continue;
                };
                g.pointer_move(x, y);
                if w[0] == "click_text" {
                    g.pointer_click(x, y, 0);
                } else {
                    g.pointer_context(x, y);
                }
            }
            "type" => {
                for c in rest(line).chars() {
                    let s = c.to_string();
                    let k = key_code(&s).unwrap_or(KeyCode::Space);
                    g.key(k, Some(&s), true);
                    g.key(k, Some(&s), false);
                }
            }
            "text" => {
                let (_, rec) = screen_texts(&g, draw);
                println!("[{at}] --- screen {:?} ---\n{}\n---", g.screen, visible_text(&rec));
            }
            "messages" => {
                for m in &g.messages {
                    println!("[{at}] msg: {m}");
                }
            }
            "lint" => {
                let (_, rec) = screen_texts(&g, draw);
                let bad = overlapping_texts(&rec);
                if bad.is_empty() {
                    println!("[{at}] lint ok on {:?}", g.screen);
                }
                for m in bad {
                    println!("[{at}] OVERLAP on {:?}: {m}", g.screen);
                }
            }
            "post" => g.say(rest(line)),
            "azerty" => g.settings.azerty = w.get(1) == Some(&"on"),
            "expect_text" | "expect_no_text" => {
                let needle = rest(line);
                let (_, rec) = screen_texts(&g, draw);
                let found = find_text(&rec, &needle).is_some();
                if found == (w[0] == "expect_text") {
                    println!("[{at}] ok: {line}");
                } else {
                    failures += 1;
                    println!("[{at}] FAILED: {line}  (screen {:?})", g.screen);
                }
            }
            "shot" => {
                let p = w.get(1).ok_or_else(|| format!("{at}: path expected"))?;
                if let Some(d) = std::path::Path::new(p).parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let (fb, _) = screen_texts(&g, draw);
                std::fs::write(p, vector_render::png::encode(fb.w, fb.h, &fb.px))
                    .map_err(|e| format!("{at}: {e}"))?;
                println!("shot -> {p}");
            }
            "say" => println!("{}", w[1..].join(" ")),
            "near" => near(&mut g, w.get(1).copied().unwrap_or("")).map_err(|e| format!("{at}: {e}"))?,
            // Beyond every orbit: no reference body, so FTL is allowed.
            "deepspace" => {
                let reach = g
                    .sim
                    .system
                    .bodies
                    .iter()
                    .filter_map(|b| b.orbit.as_ref().map(|o| o.a * (1.0 + o.e)))
                    .fold(1.0e10, f64::max);
                g.sim.ship.pos = vector_core::Vec3::new(0.0, 0.0, reach * 1.5);
                g.sim.ship.vel = vector_core::Vec3::ZERO;
                g.sim.update_ref_pub();
            }
            // A hostile pirate 2.5 km ahead, targeted, for a warship (creative-mode console cheat).
            "pirate" => {
                g.sim.rules.creative = true;
                let r = crate::console::run("warship", &mut g.sim, &mut g.timescale);
                g.sim.rules.creative = false;
                if r != ["warship ready"] {
                    return Err(format!("{at}: warship refused: {r:?}"));
                }
                g.sim.ship.pos = vector_core::Vec3::new(4.0e11, 0.0, 0.0);
                g.sim.ship.vel = vector_core::Vec3::ZERO;
                g.sim.ship.rot = vector_core::Mat3::IDENTITY;
                g.sim.ship.assist = false;
                g.sim.update_ref_pub();
                g.sim.reveal_all();
                g.sim.advance(0.5, &vector_sim::ShipInput::default());
                let pir = g
                    .sim
                    .npcs
                    .iter()
                    .position(|n| n.role == vector_sim::npc::NpcRole::Pirate)
                    .ok_or_else(|| format!("{at}: no pirate in this system"))?;
                let pos = g.sim.ship.pos;
                let vel = g.sim.ship.vel;
                let n = &mut g.sim.npcs[pir];
                n.pos = pos + vector_core::Vec3::new(120.0, 30.0, 2500.0);
                n.vel = vel;
                n.hostile = true;
                n.state = vector_sim::npc::NpcState::Combat {
                    mode: vector_sim::npc::AiMode::Pursue,
                };
                g.sim.target_npc = Some(pir);
            }
            // Firing is manual (no auto-aim): `aim` points the nose at the targeted ship once,
            // `engage <s>` holds the beam trigger while re-aiming every frame, until the ship dies.
            "aim" => aim(&mut g).map_err(|e| format!("{at}: {e}"))?,
            "engage" => {
                let secs = num(1)?;
                g.key(KeyCode::ControlLeft, None, true);
                let mut left = secs;
                while left > 1e-9 && g.sim.wrecks.is_empty() {
                    aim(&mut g).map_err(|e| format!("{at}: {e}"))?;
                    let dt = left.min(1.0 / 30.0);
                    g.advance(dt);
                    left -= dt;
                }
                g.key(KeyCode::ControlLeft, None, false);
            }
            "reveal" => g.sim.reveal_all(),
            "mark" => marked_star = Some(g.sim.here),
            "give" => match w.get(1).copied().unwrap_or("") {
                "credits" => g.sim.credits += num(2)?,
                "cargo" => {
                    let id = w.get(2).copied().unwrap_or("");
                    let r = g
                        .data
                        .resource_index(id)
                        .ok_or_else(|| format!("{at}: unknown resource '{id}'"))?;
                    let data = Arc::clone(&g.data);
                    g.sim.ship.cargo.add(&data, r, num(3)?);
                }
                "module" => {
                    let id = w.get(2).copied().unwrap_or("");
                    let m = g
                        .data
                        .modules
                        .iter()
                        .position(|m| m.id == id)
                        .ok_or_else(|| format!("{at}: unknown module '{id}'"))?;
                    g.sim.module_stock[m] += 1;
                }
                _ => return Err(format!("{at}: unknown give in '{line}'")),
            },
            "print" => println!(
                "[{at}] screen {:?} t={:.0}s credits {:.0} fuel {:.0}/{:.0} speed {:.1} m/s docked {} target {} npc_hull {:?} crew {} | {}",
                g.screen,
                g.sim.time,
                g.sim.credits,
                g.sim.ship.fuel,
                g.sim.ship.stats.fuel_cap,
                g.sim.rel_velocity().length(),
                g.sim.docked.is_some(),
                g.sim.target.is_some(),
                g.sim.target_npc.and_then(|i| g.sim.npcs.get(i)).map(|n| n.hull.round()),
                g.sim.crew.alive().count(),
                g.messages.last().map_or("", String::as_str)
            ),
            "snapshot" => {
                let p = w.get(1).ok_or_else(|| format!("{at}: path expected"))?;
                let mut fb = Framebuffer::new(1280, 720);
                draw(&mut fb, &g);
                std::fs::write(p, vector_render::png::encode(fb.w, fb.h, &fb.px))
                    .map_err(|e| format!("{at}: {e}"))?;
                println!("snapshot -> {p}");
            }
            "expect" => {
                let what = w.get(1).copied().unwrap_or("");
                let ok = match what {
                    "screen" => {
                        let want = w.get(2).copied().unwrap_or("").to_ascii_lowercase();
                        format!("{:?}", g.screen).to_ascii_lowercase() == want
                    }
                    // The tab of the market screen (open, black, exchange, brokerage) or of the
                    // crew screen (list, medicine, cryo, organs, gear, network, family, recruit).
                    "market" => {
                        let want = w.get(2).copied().unwrap_or("").to_ascii_lowercase();
                        format!("{:?}", g.market_tab).to_ascii_lowercase() == want
                    }
                    "crew" => {
                        let want = w.get(2).copied().unwrap_or("").to_ascii_lowercase();
                        format!("{:?}", g.crew_tab).to_ascii_lowercase() == want
                    }
                    "docked" => g.sim.docked.is_some() == (w.get(2) == Some(&"yes")),
                    "target" => g.sim.target.is_some() == (w.get(2) == Some(&"yes")),
                    "credits" => passes(w.get(2).copied().unwrap_or(""), g.sim.credits, num(3)?),
                    "fuel" => passes(w.get(2).copied().unwrap_or(""), g.sim.ship.fuel, num(3)?),
                    "speed" => passes(
                        w.get(2).copied().unwrap_or(""),
                        g.sim.rel_velocity().length(),
                        num(3)?,
                    ),
                    "colonies" => passes(
                        w.get(2).copied().unwrap_or(""),
                        g.sim.colonies.len() as f64,
                        num(3)?,
                    ),
                    "cargo" => {
                        let id = w.get(2).copied().unwrap_or("");
                        let r = g
                            .data
                            .resource_index(id)
                            .ok_or_else(|| format!("{at}: unknown resource '{id}'"))?;
                        passes(
                            w.get(3).copied().unwrap_or(""),
                            g.sim.ship.cargo.kg[r],
                            num(4)?,
                        )
                    }
                    "module" => {
                        let id = w.get(2).copied().unwrap_or("");
                        g.sim
                            .ship
                            .design
                            .modules(&g.data)
                            .any(|(_, m)| m.id == id)
                    }
                    "wrecks" => passes(
                        w.get(2).copied().unwrap_or(""),
                        g.sim.wrecks.len() as f64,
                        num(3)?,
                    ),
                    "hull" => passes(
                        w.get(2).copied().unwrap_or(""),
                        g.sim.ship.hull,
                        num(3)?,
                    ),
                    "ship_mass" => passes(
                        w.get(2).copied().unwrap_or(""),
                        g.sim.ship.mass(),
                        num(3)?,
                    ),
                    "jumped" => {
                        let m = marked_star.ok_or_else(|| format!("{at}: no mark"))?;
                        (g.sim.here != m) == (w.get(2) == Some(&"yes"))
                    }
                    "message" => {
                        let needle = w[2..].join(" ").to_uppercase();
                        g.messages.iter().any(|m| m.to_uppercase().contains(&needle))
                    }
                    _ => return Err(format!("{at}: unknown expectation '{line}'")),
                };
                if ok {
                    println!("[{at}] ok: {line}");
                } else {
                    failures += 1;
                    println!(
                        "[{at}] FAILED: {line}  (screen {:?}, credits {:.0}, docked {})",
                        g.screen,
                        g.sim.credits,
                        g.sim.docked.is_some()
                    );
                }
            }
            other => return Err(format!("{at}: unknown command '{other}'")),
        }
    }
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    #[test]
    fn shipped_scripts_pass() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
        let mut n = 0;
        for e in std::fs::read_dir(dir).expect("scripts dir") {
            let p = e.expect("entry").path();
            if p.extension().is_some_and(|x| x == "txt") {
                let failed = run(p.to_str().expect("utf8"), data(), crate::draw_frame)
                    .unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(failed, 0, "{}", p.display());
                n += 1;
            }
        }
        assert!(n >= 2, "{n} scripts");
    }

    #[test]
    fn script_reads_and_clicks_words() {
        let dir = std::env::temp_dir().join("vector_play_script_test.txt");
        std::fs::write(
            &dir,
            "expect_text \"NOUVELLE\"
click_text \"NOUVELLE\"
expect screen newgame
expect_no_text \"zzzqqq\"
",
        )
        .expect("write");
        let failed = run(dir.to_str().expect("utf8"), data(), crate::draw_frame)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(failed, 0);
    }

    #[test]
    fn the_tutorial_line_never_lands_on_a_screen_title() {
        let mut g = Game::new(data(), 24263);
        g.settings.azerty = false;
        g.settings.tutorial = true;
        g.new_game(24263);
        for k in [
            KeyCode::KeyI,
            KeyCode::KeyK,
            KeyCode::KeyH,
            KeyCode::F8,
            KeyCode::KeyL,
        ] {
            g.key(k, None, true);
            g.key(k, None, false);
            let (_, rec) = screen_texts(&g, crate::draw_frame);
            let bad: Vec<String> = overlapping_texts(&rec)
                .into_iter()
                .filter(|m| {
                    m.starts_with("row 0") || m.starts_with("row 1") || m.starts_with("row 2")
                })
                .collect();
            assert!(bad.is_empty(), "{:?}: {bad:?}", g.screen);
            g.key(KeyCode::Escape, None, true);
        }
    }

    #[test]
    fn the_result_of_an_action_is_readable_on_every_screen_that_has_actions() {
        let mut g = Game::new(data(), 24263);
        g.settings.azerty = false;
        g.new_game(24263);
        for k in [
            KeyCode::KeyI,
            KeyCode::KeyJ,
            KeyCode::F4,
            KeyCode::F6,
            KeyCode::KeyX,
            KeyCode::Backslash,
            KeyCode::KeyK,
            KeyCode::KeyH,
            KeyCode::F2,
        ] {
            g.key(k, None, true);
            g.key(k, None, false);
            g.say("MESSAGE DE TEST");
            let (_, rec) = screen_texts(&g, crate::draw_frame);
            assert!(
                find_text(&rec, "MESSAGE DE TEST").is_some(),
                "{:?}: the last message is not on screen",
                g.screen
            );
            g.key(KeyCode::Escape, None, true);
        }
    }

    #[test]
    fn key_names_parse() {
        assert_eq!(key_code("W"), Some(KeyCode::KeyW));
        assert_eq!(key_code("PageUp"), Some(KeyCode::PageUp));
        assert_eq!(key_code("nope"), None);
    }
}
