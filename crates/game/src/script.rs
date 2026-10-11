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
//! Test set-up (not reachable by a player): `near asteroid|station|planet|moon` puts the ship
//! next to the closest such body with matched velocity and targets it; `give credits <n>`,
//! `give cargo <resource id> <kg>`, `give module <module id>`; `mark` remembers the current
//! star, so `expect jumped yes|no` can check a jump happened.
//! More expectations: `colonies > <n>`, `cargo <id> > <n>` (or `<`), `module <id>` (installed),
//! `ship_mass > <n>`, `credits < <n>`.
use crate::game::{Game, DEFAULT_SEED};
use std::sync::Arc;
use vector_data::GameData;
use vector_render::Framebuffer;
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

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
        "backquote" => KeyCode::Backquote,
        "f1" => KeyCode::F1,
        "f2" => KeyCode::F2,
        "f3" => KeyCode::F3,
        "f4" => KeyCode::F4,
        "f5" => KeyCode::F5,
        "f6" => KeyCode::F6,
        "f9" => KeyCode::F9,
        "f10" => KeyCode::F10,
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
                let k = key(1)?;
                g.key(k, None, true);
                g.key(k, None, false);
            }
            "hold" => {
                let k = key(1)?;
                g.key(k, None, true);
                wait(&mut g, num(2)?);
                g.key(k, None, false);
            }
            "wait" => wait(&mut g, num(1)?),
            "scale" => g.timescale = num(1)?,
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
                "[{at}] screen {:?} t={:.0}s credits {:.0} fuel {:.0}/{:.0} speed {:.1} m/s docked {} target {} npc_hull {:?} | {}",
                g.screen,
                g.sim.time,
                g.sim.credits,
                g.sim.ship.fuel,
                g.sim.ship.stats.fuel_cap,
                g.sim.rel_velocity().length(),
                g.sim.docked.is_some(),
                g.sim.target.is_some(),
                g.sim.target_npc.and_then(|i| g.sim.npcs.get(i)).map(|n| n.hull.round()),
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
    fn key_names_parse() {
        assert_eq!(key_code("W"), Some(KeyCode::KeyW));
        assert_eq!(key_code("PageUp"), Some(KeyCode::PageUp));
        assert_eq!(key_code("nope"), None);
    }
}
