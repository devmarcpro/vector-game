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
use crate::game::{Game, DEFAULT_SEED};
use std::sync::Arc;
use vector_data::GameData;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

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
            "print" => println!(
                "[{at}] screen {:?} t={:.0}s credits {:.0} fuel {:.0}/{:.0} speed {:.1} m/s docked {} target {}",
                g.screen,
                g.sim.time,
                g.sim.credits,
                g.sim.ship.fuel,
                g.sim.ship.stats.fuel_cap,
                g.sim.rel_velocity().length(),
                g.sim.docked.is_some(),
                g.sim.target.is_some()
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
