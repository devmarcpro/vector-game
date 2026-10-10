mod console;
mod hud;
mod scene;

use std::{collections::HashSet, num::NonZeroU32, rc::Rc, time::Instant};
use vector_render::Framebuffer;
use vector_sim::{autopilot::Autopilot, Event, ShipInput, Sim, SIM_DT};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const SEED: u64 = 0x5EC7;
const MAX_STEPS_PER_FRAME: usize = 64;

struct Game {
    sim: Sim,
    stars: Vec<vector_core::Vec3>,
    keys: HashSet<KeyCode>,
    target: Option<usize>,
    timescale: f64,
    paused: bool,
    messages: Vec<String>,
    console: console::Console,
    map: bool,
    map_scale: f64,
    map_on_ship: bool,
}

const TIME_LADDER: [f64; 10] = [
    0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0, 1000.0, 10_000.0, 100_000.0,
];

impl Game {
    fn new() -> Self {
        let mut g = Self::raw();
        g.fit_map();
        g
    }

    fn fit_map(&mut self) {
        let max_a = self
            .sim
            .system
            .bodies
            .iter()
            .filter_map(|b| {
                b.orbit
                    .as_ref()
                    .filter(|o| o.parent == 0)
                    .map(|o| o.a * (1.0 + o.e))
            })
            .fold(1.0e9, f64::max);
        self.map_scale = max_a * 1.15 / 270.0;
    }

    fn raw() -> Self {
        Game {
            sim: scene::new_sim(SEED),
            stars: scene::star_field(SEED, 900),
            keys: HashSet::new(),
            target: None,
            timescale: 1.0,
            paused: false,
            messages: vec!["BIENVENUE A BORD - [F1] AIDE".into()],
            console: console::Console::default(),
            map: false,
            map_scale: 1.0,
            map_on_ship: false,
        }
    }

    fn input(&self) -> ShipInput {
        if self.console.open {
            return ShipInput::default();
        }
        let ax = |pos: KeyCode, neg: KeyCode| {
            f64::from(self.keys.contains(&pos) as i8) - f64::from(self.keys.contains(&neg) as i8)
        };
        ShipInput {
            pitch: ax(KeyCode::KeyS, KeyCode::KeyW),
            yaw: ax(KeyCode::KeyD, KeyCode::KeyA),
            roll: ax(KeyCode::KeyE, KeyCode::KeyQ),
            fwd: ax(KeyCode::Space, KeyCode::KeyX),
            lat: ax(KeyCode::ArrowRight, KeyCode::ArrowLeft),
            vert: ax(KeyCode::ArrowUp, KeyCode::ArrowDown),
        }
    }

    fn say(&mut self, s: impl Into<String>) {
        self.messages.push(s.into());
        if self.messages.len() > 8 {
            self.messages.remove(0);
        }
    }

    /// Advance the simulation by `seconds` of real time at the current time scale.
    fn advance(&mut self, seconds: f64) {
        let input = if self.timescale > 2.0 {
            ShipInput::default()
        } else {
            self.input()
        };
        self.sim.events.clear();
        if self.timescale <= 1.0 {
            let steps = (seconds / SIM_DT).round() as usize;
            for _ in 0..steps {
                self.sim.step_dt(&input, SIM_DT);
            }
        } else {
            self.sim.advance(seconds * self.timescale, &input);
        }
        let evs = std::mem::take(&mut self.sim.events);
        for e in evs {
            match e {
                Event::Impact {
                    body,
                    speed,
                    damage,
                } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("IMPACT {name} {speed:.0} M/S  DEGATS {damage:.0}"));
                    self.timescale = 1.0;
                }
                Event::OutOfFuel => self.say("CARBURANT EPUISE"),
                Event::Arrived { body } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("ARRIVE A {name}"));
                    self.timescale = 1.0;
                }
            }
        }
    }

    fn toggle_autopilot(&mut self) {
        if self.sim.autopilot.take().is_some() {
            self.say("PILOTE AUTO DESACTIVE");
            return;
        }
        let Some(t) = self.target else {
            self.say("PILOTE AUTO: AUCUNE CIBLE");
            return;
        };
        match Autopilot::new(&self.sim, t) {
            Ok(ap) => {
                self.sim.autopilot = Some(ap);
                self.say("PILOTE AUTO ENGAGE");
            }
            Err(e) => self.say(format!("PILOTE AUTO REFUSE: {e}")),
        }
    }

    fn step_time(&mut self, up: bool) {
        let i = TIME_LADDER
            .iter()
            .position(|&t| t >= self.timescale)
            .unwrap_or(2);
        let j = if up {
            (i + 1).min(TIME_LADDER.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        self.timescale = TIME_LADDER[j];
    }

    fn cycle_target(&mut self) {
        // Nearest-first ordering, excluding the star; Tab advances to the next-nearest.
        let p = self.sim.ship.pos;
        let mut idx: Vec<usize> = (0..self.sim.bodies.len()).collect();
        idx.sort_by(|&a, &b| {
            let da = (self.sim.bodies[a].pos - p).length();
            let db = (self.sim.bodies[b].pos - p).length();
            da.total_cmp(&db)
        });
        let next = match self.target.and_then(|t| idx.iter().position(|&i| i == t)) {
            Some(k) => idx[(k + 1) % idx.len()],
            None => idx[0],
        };
        self.target = Some(next);
    }

    fn key(&mut self, code: KeyCode, text: Option<&str>, pressed: bool) {
        if pressed && code == KeyCode::Backquote {
            self.console.open = !self.console.open;
            return;
        }
        if self.console.open {
            if !pressed {
                return;
            }
            match code {
                KeyCode::Enter => {
                    let line = std::mem::take(&mut self.console.line);
                    self.console.print(format!("> {line}"));
                    for l in console::run(&line, &mut self.sim, &mut self.timescale) {
                        self.console.print(l);
                    }
                }
                KeyCode::Backspace => {
                    self.console.line.pop();
                }
                KeyCode::Escape => self.console.open = false,
                _ => {
                    if let Some(t) = text {
                        self.console
                            .line
                            .extend(t.chars().filter(|c| !c.is_control()));
                    }
                }
            }
            return;
        }
        if !pressed {
            self.keys.remove(&code);
            return;
        }
        self.keys.insert(code);
        match code {
            KeyCode::KeyF => {
                self.sim.ship.assist = !self.sim.ship.assist;
                let a = self.sim.ship.assist;
                self.say(if a {
                    "ASSISTANCE DE VOL ACTIVEE"
                } else {
                    "ASSISTANCE DE VOL COUPEE"
                });
            }
            KeyCode::Tab => self.cycle_target(),
            KeyCode::KeyP => self.paused = !self.paused,
            KeyCode::PageUp => self.step_time(true),
            KeyCode::PageDown => self.step_time(false),
            KeyCode::KeyG => self.toggle_autopilot(),
            KeyCode::KeyM => self.map = !self.map,
            KeyCode::KeyC => self.map_on_ship = !self.map_on_ship,
            KeyCode::Equal | KeyCode::NumpadAdd => self.map_scale /= 1.5,
            KeyCode::Minus | KeyCode::NumpadSubtract => self.map_scale *= 1.5,
            KeyCode::F1 => {
                self.say("WASD/QE:ROT  ESPACE/X:POUSSEE  FLECHES:RCS  F:ASSIST");
                self.say("TAB:CIBLE G:PILOTE AUTO M:CARTE PGUP/PGDN:TEMPS ~:CONSOLE");
            }
            _ => {}
        }
    }

    fn view(&self) -> hud::View<'_> {
        hud::View {
            sim: &self.sim,
            stars: &self.stars,
            target: self.target,
            timescale: self.timescale,
            paused: self.paused,
            messages: &self.messages,
            console: &self.console,
            map: self.map,
            map_scale: self.map_scale,
            map_on_ship: self.map_on_ship,
        }
    }
}

struct App {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    fb: Framebuffer,
    game: Game,
    last: Instant,
    acc: f64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("VECTOR")
            .with_inner_size(winit::dpi::LogicalSize::new(1024, 576));
        let Ok(w) = el.create_window(attrs) else {
            el.exit();
            return;
        };
        let w = Rc::new(w);
        let Ok(ctx) = softbuffer::Context::new(w.clone()) else {
            el.exit();
            return;
        };
        self.surface = softbuffer::Surface::new(&ctx, w.clone()).ok();
        w.request_redraw();
        self.window = Some(w);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if pressed && code == KeyCode::Escape && !self.game.console.open {
                        el.exit();
                        return;
                    }
                    self.game.key(code, event.text.as_deref(), pressed);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f64().min(0.25);
                self.last = now;
                if !self.game.paused {
                    if self.game.timescale <= 1.0 {
                        self.acc += dt * self.game.timescale;
                        let steps = ((self.acc / SIM_DT) as usize).min(MAX_STEPS_PER_FRAME);
                        self.acc = (self.acc - steps as f64 * SIM_DT).min(SIM_DT);
                        self.game.advance(steps as f64 * SIM_DT);
                    } else {
                        self.game.advance(dt);
                    }
                }
                let (Some(win), Some(surf)) = (&self.window, &mut self.surface) else {
                    return;
                };
                let size = win.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                if self.fb.w != w.get() as usize || self.fb.h != h.get() as usize {
                    self.fb = Framebuffer::new(w.get() as usize, h.get() as usize);
                }
                hud::draw(&mut self.fb, &self.game.view());
                surf.resize(w, h).ok();
                if let Ok(mut buf) = surf.buffer_mut() {
                    buf.copy_from_slice(&self.fb.px);
                    buf.present().ok();
                }
                win.request_redraw();
            }
            _ => {}
        }
    }
}

fn snapshot(path: &str) {
    let mut g = Game::new();
    g.timescale = 1.0;
    // Scripted: turn toward the station, thrust a bit, target it.
    g.cycle_target();
    g.cycle_target();
    g.say("SNAPSHOT SCRIPTE");
    let mut fb = Framebuffer::new(1280, 720);
    if std::env::args().any(|a| a == "--map") {
        g.map = true;
    }
    hud::draw(&mut fb, &g.view());
    match std::fs::write(path, vector_render::png::encode(fb.w, fb.h, &fb.px)) {
        Ok(()) => println!("snapshot -> {path}"),
        Err(e) => eprintln!("write failed: {e}"),
    }
}

fn bench() {
    let mut g = Game::new();
    g.cycle_target();
    let mut fb = Framebuffer::new(1280, 720);
    let t = Instant::now();
    for _ in 0..200 {
        hud::draw(&mut fb, &g.view());
    }
    println!(
        "render 1280x720: {:.2} ms/frame",
        t.elapsed().as_secs_f64() * 1000.0 / 200.0
    );
    let t = Instant::now();
    for _ in 0..30_000 {
        g.sim.step(&ShipInput::default());
    }
    println!(
        "sim step: {:.4} ms",
        t.elapsed().as_secs_f64() * 1000.0 / 30_000.0
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        snapshot(
            args.get(i + 1)
                .map(String::as_str)
                .unwrap_or("snapshot.png"),
        );
        return;
    }
    if args.iter().any(|a| a == "--bench") {
        bench();
        return;
    }
    let Ok(el) = EventLoop::new() else {
        eprintln!("cannot create event loop");
        return;
    };
    let mut app = App {
        window: None,
        surface: None,
        fb: Framebuffer::new(1024, 576),
        game: Game::new(),
        last: Instant::now(),
        acc: 0.0,
    };
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("run failed: {e}");
    }
}
