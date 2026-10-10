mod console;
mod game;
mod hud;
mod scene;
mod ui;

use game::{Game, Screen, DEFAULT_SEED};
use std::{num::NonZeroU32, rc::Rc, sync::Arc, time::Instant};
use vector_data::GameData;
use vector_render::Framebuffer;
use vector_sim::{ShipInput, SIM_DT};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const MAX_STEPS_PER_FRAME: usize = 64;

fn load_data() -> Option<Arc<GameData>> {
    let Some(dir) = vector_data::find_data_dir() else {
        eprintln!("dossier data/ introuvable");
        return None;
    };
    match GameData::load(&dir) {
        Ok(d) => Some(Arc::new(d)),
        Err(e) => {
            eprintln!("{e}");
            None
        }
    }
}

/// Render whichever screen is active.
fn draw_frame(fb: &mut Framebuffer, g: &Game) {
    match g.screen {
        Screen::Flight => hud::draw(fb, &g.view()),
        Screen::Pause => {
            hud::draw(fb, &g.view());
            ui::draw(fb, g);
        }
        _ => {
            fb.clear();
            ui::draw(fb, g);
            hud::draw_console(fb, &g.console);
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
                    self.game.key(code, event.text.as_deref(), pressed);
                    if self.game.quit {
                        el.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f64().min(0.25);
                self.last = now;
                if self.game.running() {
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
                draw_frame(&mut self.fb, &self.game);
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

/// Headless scripted scenes for visual verification.
fn snapshot(path: &str, scene: &str, data: Arc<GameData>) {
    let mut g = Game::new(data, DEFAULT_SEED);
    if scene != "menu" {
        g.new_game(DEFAULT_SEED);
    }
    match scene {
        "map" => {
            g.map = true;
            g.cycle_target(|_| true);
        }
        "station" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            let iron = g.data.resource_index("iron").expect("iron");
            let gold = g.data.resource_index("gold").expect("gold");
            let data = Arc::clone(&g.data);
            g.sim.ship.cargo.add(&data, iron, 12_000.0);
            g.sim.ship.cargo.add(&data, gold, 40.0);
            g.sel_res = iron;
        }
        "inventory" => {
            let iron = g.data.resource_index("iron").expect("iron");
            let data = Arc::clone(&g.data);
            g.sim.ship.cargo.add(&data, iron, 5_000.0);
            g.sel_res = iron;
            g.screen = Screen::Inventory;
        }
        "mine" => {
            g.cycle_target(|k| matches!(k, vector_world::BodyKind::Asteroid(_)));
            let t = g.sim.target.expect("asteroid");
            let b = &g.sim.bodies[t];
            g.sim.ship.pos = b.pos - vector_core::Vec3::new(0.0, 0.0, 600.0 + b.radius);
            g.sim.ship.vel = b.vel;
            g.keys.insert(KeyCode::KeyV);
            g.advance(0.1);
            g.sim.ship.pos = g.sim.bodies[t].pos
                - vector_core::Vec3::new(0.0, 0.0, 600.0 + g.sim.bodies[t].radius);
            g.sim.ship.vel = g.sim.bodies[t].vel;
            g.advance(0.1);
        }
        "pause" => g.screen = Screen::Pause,
        "contracts" | "bank" | "market" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            g.sim.credits = 20_000.0;
            g.sim.borrow(4000.0).expect("loan");
            g.advance(3.0 * 86_400.0);
            match scene {
                "contracts" => {
                    g.offers = g.sim.offered_contracts();
                    if let Some(c) = g.offers.first().cloned() {
                        let _ = g.sim.accept_contract(c);
                    }
                    g.screen = Screen::Contracts;
                }
                "bank" => g.screen = Screen::Bank,
                _ => g.screen = Screen::Station,
            }
        }
        "traffic" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, 12_000.0);
            g.sim.target_npc = g.sim.visible_npcs().first().copied();
            g.advance(0.2);
        }
        "galaxy" => {
            g.sim.ship.pos = vector_core::Vec3::new(4.0e11, 0.0, 0.0);
            g.sim.update_ref_pub();
            g.open_galaxy();
        }
        "crew" | "recruit" | "journal" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            g.sim.credits = 100_000.0;
            g.sim.hire(1, 4).expect("hire");
            let data = Arc::clone(&g.data);
            for (id, kg) in [("food", 80.0), ("water", 300.0)] {
                g.sim
                    .ship
                    .cargo
                    .add(&data, data.resource_index(id).expect("res"), kg);
            }
            g.sim.advance(40.0 * 86_400.0, &ShipInput::default());
            g.screen = match scene {
                "crew" => Screen::Crew,
                "recruit" => Screen::Recruit,
                _ => Screen::Journal,
            };
        }
        "ship" | "picker" | "yard" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            g.sim.ship.sys.health[1] = 0.55;
            g.sim.ship.sys.health[3] = 0.2;
            g.sim.credits = 250_000.0;
            g.advance(0.2);
            g.screen = Screen::Ship;
            g.ship_sel = 2;
            if scene == "picker" {
                g.ship_sel = 7;
                g.picker = true;
                g.pick_sel = 2;
            }
            if scene == "yard" {
                g.screen = Screen::Shipyard;
                g.yard_sel = 5;
            }
        }
        _ => {
            g.cycle_target(|_| true);
            g.cycle_target(|_| true);
        }
    }
    let mut fb = Framebuffer::new(1280, 720);
    draw_frame(&mut fb, &g);
    match std::fs::write(path, vector_render::png::encode(fb.w, fb.h, &fb.px)) {
        Ok(()) => println!("snapshot -> {path}"),
        Err(e) => eprintln!("write failed: {e}"),
    }
}

fn bench(data: Arc<GameData>) {
    let mut g = Game::new(data, DEFAULT_SEED);
    g.new_game(DEFAULT_SEED);
    g.cycle_target(|_| true);
    let mut fb = Framebuffer::new(1280, 720);
    let t = Instant::now();
    for _ in 0..200 {
        draw_frame(&mut fb, &g);
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
    let t = Instant::now();
    let bytes = g.sim.save_bytes().map(|b| b.len()).unwrap_or(0);
    println!(
        "save: {bytes} bytes in {:.3} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
}

fn main() {
    let Some(data) = load_data() else {
        std::process::exit(1);
    };
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        let path = args
            .get(i + 1)
            .map(String::as_str)
            .unwrap_or("snapshot.png");
        let scene = args.get(i + 2).map(String::as_str).unwrap_or("cockpit");
        snapshot(path, scene, data);
        return;
    }
    if args.iter().any(|a| a == "--bench") {
        bench(data);
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
        game: Game::new(data, DEFAULT_SEED),
        last: Instant::now(),
        acc: 0.0,
    };
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("run failed: {e}");
    }
}
