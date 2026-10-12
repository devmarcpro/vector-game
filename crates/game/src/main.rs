mod archive_view;
mod audio;
mod bourse_view;
mod codex;
mod codex_sky;
mod console;
mod cosmos_view;
mod crew_net_view;
mod designer_view;
mod dialogue_view;
mod eco_view;
mod expedition_view;
mod ext_view;
mod faction_view;
mod family_view;
mod farm_view;
mod fleet_map_view;
mod flight_menu;
mod flow_view;
mod fpv_view;
mod game;
mod globe;
mod health_view;
mod hub;
mod hud;
mod medicine_view;
mod mind_view;
mod mouse;
mod newgame;
mod power_view;
mod proto_view;
mod rogue_view;
mod scene;
mod script;
mod settings;
mod ship_model;
mod ship_preview;
mod sky_view;
mod society_view;
mod sprites;
mod surface_view;
mod tech_view;
mod terrain;
mod trade_view;
mod tutorial;
mod ui;
mod wire_shape;

use game::{Game, Screen, DEFAULT_SEED};
use std::{num::NonZeroU32, rc::Rc, sync::Arc, time::Instant};
use vector_data::GameData;
use vector_render::Framebuffer;
use vector_sim::{ShipInput, SIM_DT};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
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
    g.begin_frame();
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
    ui::draw_overlay(fb, g);
}

struct App {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    fb: Framebuffer,
    game: Game,
    last: Instant,
    acc: f64,
    audio: audio::Audio,
    /// Raw mouse motion since the last frame, and whether the cursor is currently captured.
    mouse_acc: (f64, f64),
    grabbed: bool,
    started: Instant,
}

/// Full turn demand at this many pixels of raw mouse motion per second.
const MOUSE_FULL_PX_PER_S: f64 = 500.0;

impl App {
    /// Capture the cursor while mouse look is on in flight; release it otherwise.
    fn sync_mouse(&mut self, dt: f64) {
        let want =
            self.game.mouse_look && self.game.screen == Screen::Flight && !self.game.console.open;
        if want != self.grabbed {
            if let Some(w) = &self.window {
                use winit::window::CursorGrabMode;
                if want {
                    let ok = w.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                        || w.set_cursor_grab(CursorGrabMode::Confined).is_ok();
                    w.set_cursor_visible(!ok);
                } else {
                    w.set_cursor_grab(CursorGrabMode::None).ok();
                    w.set_cursor_visible(true);
                }
            }
            self.grabbed = want;
        }
        let (dx, dy) = std::mem::take(&mut self.mouse_acc);
        self.game.mouse = if want && dt > 0.0 {
            (
                (dx / dt / MOUSE_FULL_PX_PER_S).clamp(-1.0, 1.0),
                (dy / dt / MOUSE_FULL_PX_PER_S).clamp(-1.0, 1.0),
            )
        } else {
            (0.0, 0.0)
        };
    }
}

impl ApplicationHandler for App {
    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, ev: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = ev {
            self.mouse_acc.0 += delta.0;
            self.mouse_acc.1 += delta.1;
        }
    }

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
                    let code = self.game.settings.translate(code);
                    self.game.key(code, event.text.as_deref(), pressed);
                    if self.game.quit {
                        el.exit();
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.game.pointer_move(position.x as i32, position.y as i32);
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.game.pointer.down = false;
                self.game.pointer_release();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => match button {
                MouseButton::Left => {
                    self.game.pointer.down = true;
                    let (x, y) = self.game.pointer.pos;
                    let ms = self.started.elapsed().as_millis() as u64;
                    self.game.pointer_click(x, y, ms);
                    if self.game.quit {
                        el.exit();
                    }
                }
                MouseButton::Right => self.game.pointer_back(),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y / 24.0,
                };
                if lines != 0.0 {
                    self.game.pointer_wheel(lines);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f64().min(0.25);
                self.last = now;
                self.sync_mouse(dt);
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
                self.audio.update(&self.game);
                draw_frame(&mut self.fb, &self.game);
                let st = &self.game.settings;
                if st.scanlines || st.glow || st.scheme > 0 {
                    let tint = match st.scheme {
                        1 => Some((60, 255, 90)),
                        2 => Some((255, 180, 40)),
                        3 => Some((255, 255, 255)),
                        4 => Some((90, 160, 255)),
                        _ => None,
                    };
                    self.fb.postfx(st.scanlines, st.glow, tint);
                }
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
    // "ext_<scene>[@yaw,pitch,zoom]": the same scene seen from the third-person camera.
    let (scene, cam) = scene.split_once('@').unwrap_or((scene, ""));
    let (scene, ext) = match scene.strip_prefix("ext_") {
        Some(rest) => (rest, true),
        None => (scene, false),
    };
    let cam: Vec<f64> = cam.split(',').filter_map(|x| x.parse().ok()).collect();
    if let [yaw, pitch, zoom] = cam[..] {
        (g.ext_yaw, g.ext_pitch, g.ext_zoom) = (yaw, pitch, zoom);
    }
    if scene != "menu" {
        g.new_game(DEFAULT_SEED);
    }
    g.ext_view = ext;
    // A world with life, for the views of planets: look for a seed that has a Terran or Ocean one.
    if scene.starts_with("world_life") {
        for seed in 1..400u64 {
            g.new_game(seed);
            if g.sim.bodies.iter().any(|b| {
                matches!(
                    b.kind,
                    vector_world::BodyKind::Planet(
                        vector_world::PlanetKind::Terran | vector_world::PlanetKind::Ocean
                    )
                )
            }) {
                break;
            }
        }
    }
    match scene {
        "map" => {
            g.map = true;
            g.cycle_target(|_| true);
        }
        "dialogue" => {
            g.sim.spawn_traffic();
            let k = g
                .sim
                .npcs
                .iter()
                .position(|n| n.role == vector_sim::npc::NpcRole::Patrol)
                .unwrap_or(0);
            g.sim.npcs[k].faction = g.data.factions.iter().position(|f| f.id == "hegemonie");
            g.sim.target_npc = Some(k);
            g.open_dialogue();
            g.dlg_sel = 1;
        }
        s if s.starts_with("world") => {
            let near = s.ends_with("near");
            let giant = s.ends_with("giant");
            g.sim.reveal_all();
            let i = g
                .sim
                .bodies
                .iter()
                .position(|b| {
                    giant && matches!(b.kind, vector_world::BodyKind::Planet(p) if p.is_giant())
                })
                .or_else(|| {
                    g.sim.bodies.iter().position(|b| {
                        matches!(
                            b.kind,
                            vector_world::BodyKind::Planet(
                                vector_world::PlanetKind::Terran | vector_world::PlanetKind::Ocean
                            )
                        )
                    })
                })
                .or_else(|| {
                    g.sim.bodies.iter().position(
                        |b| matches!(b.kind, vector_world::BodyKind::Planet(p) if !p.is_giant()),
                    )
                })
                .expect("a solid planet");
            let b = &g.sim.bodies[i];
            let dir = vector_core::Vec3::new(0.3, 0.15, -1.0).normalized();
            let land = s.ends_with("land");
            let low = s.ends_with("low") || land || s.ends_with("touch");
            g.sim.ship.pos = b.pos
                - dir
                    * (b.radius
                        * if land || s.ends_with("touch") {
                            1.0 + 3000.0 / b.radius
                        } else if low {
                            1.012
                        } else if near {
                            1.25
                        } else {
                            3.2
                        });
            g.sim.ship.vel = b.vel;
            if land || s.ends_with("touch") {
                // Over the real ground: 60 m up on approach, on its gear when touching.
                // Under the star at noon (a little off the zenith so the shadow shows).
                let sun = (b.pos * -1.0).normalized();
                let side = sun
                    .cross(vector_core::Vec3::new(0.0, 1.0, 0.0))
                    .normalized();
                let up = (sun + side * 0.45).normalized();
                let (c, r) = (b.pos, b.radius);
                let gr = crate::ext_view::ground(&g, &g.sim).map_or(3000.0, |x| x.h);
                let want = if land { 60.0 } else { 0.0 };
                g.sim.ship.pos = c + up * (r + 3000.0 - gr + want);
            }
            g.sim.ship.rot = if low {
                // Flying along the ground, nose a little down.
                let forward = (vector_core::Vec3::new(1.0, -0.12, 0.0)).normalized();
                vector_core::Mat3::look_at(forward, vector_core::Vec3::new(0.0, 1.0, 0.0))
            } else {
                vector_core::Mat3::look_at(dir, vector_core::Vec3::new(0.0, 1.0, 0.0))
            };
            g.sim.target = Some(i);
            g.sim.update_ref_pub();
        }
        "visit_ship" | "visit_station" => {
            let d = Arc::clone(&g.data);
            for k in 0..4 {
                let p = vector_sim::crew::generate_person(&d, 700 + k, None);
                g.sim.crew.hire(&d, p, 1, 0.0);
            }
            g.sim.crew.members[2].hunger = 0.9;
            let station = scene == "visit_station";
            if station {
                let (st, _) = g.sim.nearest_station().expect("station");
                g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
                g.sim.try_dock().expect("dock");
            }
            let i = g
                .sim
                .rogue_targets()
                .iter()
                .position(|t| {
                    if station {
                        g.data.rogue_themes[t.theme].id == "station"
                    } else {
                        t.kind == vector_sim::rogue::TargetKind::Ship
                    }
                })
                .expect("a place");
            g.sim.rogue_enter(i, vec![]).expect("goes");
            for _ in 0..12 {
                let target = {
                    let r = g.sim.rogue.as_ref().expect("a run");
                    let mut best = (r.x, r.y);
                    let mut d = 0;
                    for y in 0..r.h as i32 {
                        for x in 0..r.w as i32 {
                            if r.floor().seen[r.idx(x, y)] && r.walkable(x, y) {
                                let dd = (x - r.x).abs().max((y - r.y).abs());
                                if dd > d {
                                    d = dd;
                                    best = (x, y);
                                }
                            }
                        }
                    }
                    best
                };
                let _ = g.sim.rogue_travel(target.0, target.1, 30);
            }
            g.screen = Screen::Rogue;
        }
        // The new-game flow, one scene per step: newgame0 .. newgame6 (newgame1_naufrage, ...).
        s if s.starts_with("newgame") => {
            let step: usize = s[7..8].parse().unwrap_or(0);
            g.open_new_game();
            if s.ends_with("naufrage") {
                g.click_ng_scenario(g.data.scenario_index("naufrage").expect("scenario"));
            } else if s.ends_with("marchand") {
                g.click_ng_scenario(g.data.scenario_index("marchand").expect("scenario"));
            }
            g.tut_step = None;
            g.ng.seed_text = if step >= 4 {
                "ANDROMEDE".into()
            } else {
                String::new()
            };
            if step >= 4 {
                g.ng.cursor = 2;
            }
            g.ng_goto(step);
        }
        // The one menu: hub (the eight spaces), hub_vaisseau (a page), hub_dock (docked),
        // hub_castaway (the castaway's orientation).
        "hub" | "hub_vaisseau" | "hub_dock" | "hub_castaway" => {
            if scene == "hub_dock" {
                let (st, _) = g.sim.nearest_station().expect("station");
                g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
                g.key(KeyCode::KeyD, None, true);
            }
            if scene == "hub_castaway" {
                let mut cfg = vector_sim::start::StartConfig::new(DEFAULT_SEED);
                cfg.scenario = g.data.scenario_index("naufrage").expect("scenario");
                g.start_game(&cfg);
            }
            g.tut_step = None;
            g.open_hub();
            if scene == "hub_vaisseau" {
                g.hub_select(2);
            }
            if scene == "hub_dock" {
                g.hub_select(4);
            }
        }
        // A castaway on foot, right after the start.
        "castaway" => {
            let mut cfg = vector_sim::start::StartConfig::new(DEFAULT_SEED);
            cfg.scenario = g.data.scenario_index("naufrage").expect("scenario");
            g.start_game(&cfg);
        }
        "rogue0" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.screen = Screen::Rogue;
        }
        s if s.starts_with("rogue_") => {
            let fpv = s.ends_with("_fpv");
            let id = s[6..].trim_end_matches("_fpv");
            let theme = g.data.rogue_theme_index(id).expect("a known place");
            for p in &mut g.sim.crew.members {
                p.health = 1.0;
            }
            g.sim.ship.cargo.cap_mass = 1.0e7;
            let party = g.sim.rogue_squad();
            g.sim.rogue_begin(theme, 12345, party);
            // Explore: walk to the farthest known cell, again and again.
            for _ in 0..9 {
                let target = {
                    let r = g.sim.rogue.as_ref().expect("a run");
                    let mut best = (r.x, r.y);
                    let mut d = 0;
                    for y in 0..r.h as i32 {
                        for x in 0..r.w as i32 {
                            if r.floor().seen[r.idx(x, y)] && r.tile(x, y) != 0 {
                                let dd = (x - r.x).abs().max((y - r.y).abs());
                                if dd > d {
                                    d = dd;
                                    best = (x, y);
                                }
                            }
                        }
                    }
                    best
                };
                let _ = g.sim.rogue_travel(target.0, target.1, 30);
                if g.sim.rogue.as_ref().is_some_and(|r| r.over) {
                    break;
                }
            }
            g.screen = Screen::Rogue;
            if fpv {
                g.fpv = true;
                g.fpv_dir = fpv_view::open_direction(g.sim.rogue.as_ref().expect("a run"));
            }
        }
        "cosmos" => {
            g.sim.credits = 100_000.0;
            let _ = g.sim.set_anchor();
            g.sim.time += 5.0 * 86_400.0;
            let _ = g.sim.rewind(0);
            g.screen = Screen::Cosmos;
        }
        "haute_sf" => {
            g.sim.credits = 500_000.0;
            let d = std::sync::Arc::clone(&g.sim.data);
            g.sim.ship.cargo.cap_mass = 1.0e7;
            g.sim.ship.cargo.cap_vol = 1.0e7;
            for (id, kg) in [("antimatter", 0.02), ("nanites", 3.0)] {
                if let Some(r) = d.resource_index(id) {
                    g.sim.ship.cargo.add(&d, r, kg);
                }
            }
            let p = vector_sim::crew::generate_person(&d, 4, None);
            g.sim.crew.hire(&d, p, 1, 0.0);
            let slot = g
                .sim
                .ship
                .design
                .modules(&d)
                .find(|(_, m)| m.jump_range > 0.0)
                .map(|(i, _)| i);
            if let Some(i) = slot {
                g.sim.ship.design.slots[i] = d.module_index("exo_warp_drive");
            }
            g.sim.ship.refresh_stats(&d);
            let _ = g.sim.clone_crew(0);
            g.screen = Screen::Cosmos;
        }
        "courtage" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.sim.try_dock().expect("dock");
            g.sim.time = 120.0 * 86_400.0;
            g.screen = Screen::Trading;
            g.trade_ui.tab = 0;
        }
        "assurance" | "titres" | "ventes" | "lignes" | "economie" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.sim.try_dock().expect("dock");
            g.sim.credits = 300_000.0;
            g.sim.time = 400.0 * 86_400.0;
            let data = std::sync::Arc::clone(&g.data);
            for (id, kg) in [("gold", 150.0), ("iron", 4000.0), ("food", 900.0)] {
                if let Some(r) = data.resource_index(id) {
                    g.sim.ship.cargo.add(&data, r, kg);
                }
            }
            g.screen = Screen::Trading;
            g.trade_ui.tab = match scene {
                "assurance" => 4,
                "titres" => 5,
                "ventes" => 6,
                "lignes" => 7,
                _ => 8,
            };
            let gold = data.resource_index("gold").unwrap_or(0);
            match scene {
                "assurance" => {
                    let _ = g.sim.buy_insurance(true);
                    let _ = g.sim.declare_loss(gold, 20.0);
                }
                "titres" => {
                    let iron = data.resource_index("iron").unwrap_or(0);
                    g.click_eco(100);
                    g.click_eco(120);
                    g.trade_ui.sel = iron;
                    g.click_eco(130);
                    g.click_eco(131);
                }
                "ventes" => {
                    g.trade_ui.sel = gold;
                    g.click_eco(200);
                    g.click_eco(240);
                }
                "lignes" => {
                    let galaxy = g.sim.galaxy;
                    if let Some(hub) = galaxy
                        .nearby([0.0; 3], 25.0)
                        .into_iter()
                        .find(|s| !vector_sim::lanes::lines_at(&data, &galaxy, s.pos).is_empty())
                    {
                        g.sim.here_pos = hub.pos;
                    }
                    let lines = g.sim.lines_here();
                    if let Some(lv) = lines.first() {
                        for k in 0..lv.convoys.len() {
                            if g.sim
                                .finance_convoy(lv.line.id, lv.convoys[k].k, 1000.0)
                                .is_ok()
                            {
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        "elevage" | "casino" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.sim.try_dock().expect("dock");
            g.sim.credits = 100_000.0;
            if scene == "elevage" {
                g.screen = Screen::Livestock;
                for _ in 0..4 {
                    g.key(KeyCode::KeyB, None, true);
                }
                g.key(KeyCode::ArrowRight, None, true);
                for _ in 0..4 {
                    g.key(KeyCode::KeyB, None, true);
                }
            } else {
                g.screen = Screen::Casino;
                g.key(KeyCode::Enter, None, true);
                g.key(KeyCode::Enter, None, true);
            }
        }
        "proto0" | "proto1" => {
            g.proto = if scene == "proto0" { 0 } else { 1 };
            g.organ_sel = 2;
            g.proto_slot = 3;
            g.screen = Screen::Proto;
        }
        "factions" => {
            g.sim.set_reputation(1, 60.0);
            g.sim.set_reputation(4, -70.0);
            g.key(KeyCode::KeyX, None, true);
            g.fac_sel = 4;
        }
        "societe" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.sim.try_dock().ok();
            g.sim.credits = 50_000.0;
            let id = g.sim.crew.members[0].id;
            for k in 0..3 {
                g.sim.time += 86_400.0;
                g.sim.compose(id, k).ok();
            }
            g.key(KeyCode::KeyX, None, true);
            g.fac_sel = g.sim.faction.unwrap_or(0);
            g.soc_page = true;
        }
        "bourse" | "bourse2" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            // Several months of history, and a resource that an event moved if there is one.
            g.sim.time = 400.0 * 86_400.0;
            g.sim.reveal_all();
            let ev = g.sim.market_news(180.0);
            let pick = ev
                .iter()
                .flat_map(|e| {
                    g.data.world_events[e.def]
                        .effects
                        .iter()
                        .filter_map(|(k, _)| g.data.resource_index(k))
                        .collect::<Vec<_>>()
                })
                .next()
                .unwrap_or(0);
            g.bourse_sel = pick;
            g.bourse_range = 1;
            g.key(KeyCode::KeyT, None, true);
            if scene == "bourse2" {
                g.sim.credits = 90_000.0;
                g.comp_sel = 2;
                let _ = g.sim.buy_shares(2, 40.0);
                g.key(KeyCode::KeyE, None, true);
            }
        }
        "family" => {
            use vector_sim::{crew, genealogy};
            // A crew with a brother and a cousin of the first member aboard.
            let data = Arc::clone(&g.data);
            let me = g.sim.crew.members[0].kin;
            let fams = g.sim.crew.families.clone();
            let sib = genealogy::siblings(&fams, me)[0];
            let p = crew::person_from_kin(&data, &fams, sib, 4242, None, None);
            g.sim.crew.hire(&data, p, 2, 0.0);
            if let Some((kin, reg)) = genealogy::relative_of(&fams, me, 7) {
                let mut f2 = fams.clone();
                let def = reg.map(|(fam, f, m, n)| {
                    f2.insert(fam, (f, m, n));
                    (f, m, n)
                });
                let p = crew::person_from_kin(&data, &f2, kin, 4243, None, def);
                g.sim.crew.hire(&data, p, 3, 0.0);
            }
            g.screen = Screen::Crew;
            g.key(KeyCode::KeyG, None, true);
        }
        "stationview" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius * 2.5);
            g.sim.reveal_all();
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
            for (id, kg) in [
                ("water", 3_000.0),
                ("hydrogen", 400.0),
                ("electronics", 300.0),
            ] {
                if let Some(r) = data.resource_index(id) {
                    g.sim.ship.cargo.add(&data, r, kg);
                }
            }
            g.sel_res = iron;
            g.screen = Screen::Inventory;
        }
        "planet" | "giant" => {
            g.sim.reveal_all();
            let giant = scene == "giant";
            let i = g
                .sim
                .bodies
                .iter()
                .position(|b| matches!(b.kind, vector_world::BodyKind::Planet(p) if p.is_giant() == giant))
                .expect("a planet of that class");
            let b = &g.sim.bodies[i];
            g.sim.ship.pos = b.pos - vector_core::Vec3::new(0.0, 0.0, b.radius * 3.5);
            g.sim.ship.vel = b.vel;
            g.sim.target = Some(i);
            g.sim.update_ref_pub();
        }
        "sky_ring" | "sky_bh" | "sky_pulsar" | "sky_comet" => {
            // A system chosen for what it holds (rings, a black hole, a pulsar, a comet), the
            // ship parked in front of it with the object targeted.
            use vector_world::stellar::StarStage;
            let hit = |s: &vector_world::System| match scene {
                "sky_ring" => !s.rings().is_empty(),
                "sky_bh" => s.stage == StarStage::BlackHole && s.accreting(),
                "sky_pulsar" => s.stage == StarStage::NeutronStar,
                _ => s.bodies.iter().any(|b| {
                    b.kind == vector_world::BodyKind::Anomaly(vector_world::AnomalyKind::Comet)
                }),
            };
            let sys = (0..60_000u64)
                .map(|k| k.wrapping_mul(0x9E37_79B9))
                .map(vector_world::generate_system)
                .find(|s| hit(s))
                .expect("such a system");
            let (i, back) = match scene {
                "sky_ring" => {
                    let r = sys.rings()[0];
                    (r.planet, sys.bodies[r.planet].radius * 7.0)
                }
                "sky_bh" | "sky_pulsar" => (0, 4.0e7),
                _ => (
                    sys.bodies
                        .iter()
                        .position(|b| {
                            b.kind
                                == vector_world::BodyKind::Anomaly(vector_world::AnomalyKind::Comet)
                        })
                        .expect("a comet"),
                    6.0e5,
                ),
            };
            let data = Arc::clone(&g.data);
            let mut sim = vector_sim::Sim::new(sys, data);
            if scene == "sky_comet" {
                let o = sim.system.bodies[i].orbit.clone().expect("orbit");
                sim.time = (0..2000)
                    .map(|k| f64::from(k) * o.period() / 2000.0)
                    .rfind(|&t| sim.system.comet_tail(i, t).is_some())
                    .expect("a tail");
                sim.refresh_bodies();
            }
            sim.reveal_all();
            let b = &sim.bodies[i];
            // Seen from a little above the plane of the orbits, so that rings open into ellipses.
            let lift = if scene == "sky_ring" {
                back * 0.45
            } else {
                0.0
            };
            sim.ship.pos = b.pos + vector_core::Vec3::new(0.0, lift, -back);
            sim.ship.vel = b.vel;
            sim.target = Some(i);
            sim.update_ref_pub();
            g.sim = sim;
        }
        "surface" | "surface_geo" | "surface_clouds" | "biosphere" | "biosphere_card" | "globe"
        | "walk" | "walk_fpv" | "expedition" => {
            g.sim.reveal_all();
            let i = g
                .sim
                .bodies
                .iter()
                .position(|b| {
                    matches!(
                        b.kind,
                        vector_world::BodyKind::Planet(
                            vector_world::PlanetKind::Terran
                                | vector_world::PlanetKind::Ocean
                                | vector_world::PlanetKind::Tundra
                                | vector_world::PlanetKind::Desert
                        )
                    )
                })
                .or_else(|| {
                    g.sim.bodies.iter().position(
                        |b| matches!(b.kind, vector_world::BodyKind::Planet(p) if !p.is_giant()),
                    )
                })
                .expect("a solid planet");
            g.sim.target = Some(i);
            g.open_surface();
            g.surf_globe = scene == "globe";
            if scene.starts_with("biosphere") {
                g.surf_layer = surface_view::BIOSPHERE;
                g.surf_card = scene == "biosphere_card";
            } else if scene == "surface_clouds" {
                g.surf_layer = 6;
            } else if scene == "surface_geo" {
                g.surf_layer = surface_view::GEOLOGIE;
            }
            g.surf_spin = 0.9;
            if scene.starts_with("walk") || scene == "expedition" {
                let body = g.surf.as_ref().map(|m| m.body).expect("map");
                g.sim.ship.pos = g.sim.bodies[body].pos
                    - vector_core::Vec3::new(0.0, 0.0, g.sim.bodies[body].radius * 3.0);
                g.sim.ship.vel = g.sim.bodies[body].vel;
                g.sim.update_ref_pub();
                // A land cell with plants and animals.
                'find: for cy in (6..vector_world::surface::H - 6).step_by(5) {
                    for cx in (0..vector_world::surface::W).step_by(7) {
                        if g.sim.rogue_land(body, cx, cy, vec![0], false).is_ok() {
                            let r = g.sim.rogue.take().expect("a run");
                            let plants = r.floor().items.iter().filter(|i| i.plant).count();
                            if plants > 20 && r.floor().monsters.len() > 6 {
                                g.surf_cx = cx;
                                g.surf_cy = cy;
                                break 'find;
                            }
                        }
                    }
                }
                g.open_expedition();
                if scene.starts_with("walk") {
                    g.exp_vehicle = true;
                    g.key(KeyCode::Enter, None, true);
                    for k in [KeyCode::ArrowRight, KeyCode::ArrowRight, KeyCode::ArrowDown] {
                        g.key(k, None, true);
                    }
                    if scene == "walk_fpv" {
                        g.fpv = true;
                        g.fpv_dir = fpv_view::open_direction(g.sim.rogue.as_ref().expect("a run"));
                    }
                }
            }
        }
        "speed" => {
            g.sim.ship.vel = g
                .sim
                .ship
                .rot
                .mul_vec(vector_core::Vec3::new(0.0, 0.0, 3000.0));
            g.keys.insert(KeyCode::ShiftLeft);
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
        "rules" => {
            g.pending_rules.adjust(3, true);
            g.pending_rules.adjust(11, true);
            g.rules_sel = 3;
            g.screen = Screen::Rules;
        }
        "options" => g.screen = Screen::Options,
        "codex" => {
            g.codex_cat = 1;
            g.codex_sel = 5;
            g.screen = Screen::Codex;
        }
        "codex_geo" | "codex_sky" => {
            g.codex_cat = if scene == "codex_geo" { 29 } else { 30 };
            g.codex_sel = 4;
            g.screen = Screen::Codex;
        }
        "galaxy_ism" => {
            // Inside a supernova remnant or a nebula: circles of the medium on the galactic map.
            use vector_world::cosmos::FeatureKind;
            let galaxy = g.sim.galaxy;
            let f = (-60..60)
                .flat_map(|x| (-60..60).map(move |z| (x, z)))
                .flat_map(|(x, z)| galaxy.features_in_cell((x, 0, z)))
                .find(|f| f.kind == FeatureKind::SupernovaRemnant)
                .expect("a remnant");
            g.sim.here_pos = f.pos;
            g.sim.reveal_all();
            g.open_galaxy();
            g.gal_zoom = 6.0;
        }
        "help" => {
            g.help_open = true;
            g.start_tutorial();
        }
        "tech" => {
            g.sim.research_pool = 340.0;
            let pp = g.data.tech_index("plasma_physics").expect("tech");
            g.sim.start_research(pp).expect("start");
            g.sim.tick_research(1.0);
            g.tech_sel = 7;
            g.screen = Screen::Tech;
        }
        "archives" => {
            g.sim.credits = 5_000.0;
            g.sim.archive.careful = true;
            for key in [3_u64, 11, 29] {
                g.sim.dig_site("ruine", key);
            }
            let _ = g.sim.date_artefact(0);
            g.journal_tab = 2;
            g.screen = Screen::Journal;
        }
        "esprit" => {
            g.sim.credits = 20_000.0;
            let k = g
                .data
                .software_index("datation_isotopique")
                .expect("software");
            g.sim.install_software(k).expect("install");
            g.sim.add_cipher(6.0, "ARCHIVE CHIFFREE");
            g.sim.mind.alloc = [3, 6, 3, 5, 3];
            g.tech_mind = true;
            g.screen = Screen::Tech;
        }
        "anomaly" => {
            let galaxy = vector_world::galaxy::Galaxy::new(12);
            let entry = galaxy
                .nearby([0.0; 3], 400.0)
                .into_iter()
                .find(|e| {
                    vector_world::generate_system(e.seed)
                        .bodies
                        .iter()
                        .any(|b| {
                            matches!(
                                b.kind,
                                vector_world::BodyKind::Anomaly(
                                    vector_world::AnomalyKind::RadiationBelt
                                )
                            )
                        })
                })
                .expect("a system with a radiation belt");
            g.sim = vector_sim::Sim::new_in_galaxy(galaxy, &entry, Arc::clone(&g.data));
            g.sim.reveal_all();
            let i = g
                .sim
                .system
                .bodies
                .iter()
                .position(|b| matches!(b.kind, vector_world::BodyKind::Anomaly(_)))
                .expect("anomaly");
            g.sim.ship.pos = g.sim.bodies[i].pos - vector_core::Vec3::new(0.0, 0.0, 9000.0);
            g.sim.ship.vel = g.sim.bodies[i].vel;
            g.sim.ship.assist = false;
            g.sim.update_ref_pub();
            g.sim.target = Some(i);
            g.advance(0.2);
        }
        "space" => {
            // Under thrust, to see the flames.
            g.keys.insert(KeyCode::Space);
            for _ in 0..30 {
                g.advance(1.0 / 30.0);
            }
        }
        "combat" => {
            g.sim.rules.creative = true;
            let k = console::run("warship", &mut g.sim, &mut g.timescale);
            assert_eq!(k, vec!["warship ready".to_string()]);
            g.sim.ship.pos = vector_core::Vec3::new(4.0e11, 0.0, 0.0);
            g.sim.ship.vel = vector_core::Vec3::ZERO;
            g.sim.ship.rot = vector_core::Mat3::IDENTITY;
            g.sim.ship.assist = false;
            g.sim.update_ref_pub();
            g.sim.reveal_all();
            g.sim.advance(0.5, &ShipInput::default());
            let pir = g
                .sim
                .npcs
                .iter()
                .position(|n| n.role == vector_sim::npc::NpcRole::Pirate)
                .expect("pirate");
            let pos = g.sim.ship.pos;
            let n = &mut g.sim.npcs[pir];
            n.pos = pos + vector_core::Vec3::new(120.0, 30.0, 2500.0);
            n.vel = g.sim.ship.vel;
            n.hostile = true;
            n.state = vector_sim::npc::NpcState::Combat {
                mode: vector_sim::npc::AiMode::Pursue,
            };
            g.sim.target_npc = Some(pir);
            g.keys.insert(KeyCode::ControlLeft);
            g.keys.insert(KeyCode::KeyB);
            for _ in 0..40 {
                g.advance(1.0 / 30.0);
            }
        }
        "empire" | "colony" | "fleet" | "fleetmap" | "flow" => {
            let data = Arc::clone(&g.data);
            let planet = g
                .sim
                .system
                .bodies
                .iter()
                .position(|b| matches!(b.kind, vector_world::BodyKind::Planet(k) if !k.is_giant()))
                .expect("rocky planet");
            g.sim.credits = 2_000_000.0;
            g.sim.ship.cargo.cap_mass = 1.0e6;
            g.sim.ship.cargo.cap_vol = 1.0e6;
            for (id, kg) in [
                ("construction_materials", 3000.0),
                ("colonists", 400.0),
                ("food", 300.0),
                ("water", 600.0),
            ] {
                g.sim
                    .ship
                    .cargo
                    .add(&data, data.resource_index(id).expect("res"), kg);
            }
            g.sim.ship.pos = g.sim.bodies[planet].pos
                - vector_core::Vec3::new(0.0, 0.0, g.sim.bodies[planet].radius + 5000.0);
            g.sim.ship.vel = g.sim.bodies[planet].vel;
            g.sim.update_ref_pub();
            let ci = g
                .sim
                .found_colony(planet, Some("Nouvel Eden".into()))
                .expect("colony");
            for id in [
                "habitat_dome",
                "habitat_dome",
                "surface_mine",
                "hydroponics",
                "water_plant",
                "medical_center",
                "smelter",
                "research_lab",
            ] {
                let d = data.building_index(id).expect("building");
                g.sim.colonies[ci]
                    .buildings
                    .push(vector_sim::colony::BuildingInst {
                        def: d,
                        progress: 1.0,
                        enabled: true,
                        recipe: 0,
                        health: 1.0,
                    });
            }
            let d = data.building_index("fission_plant").expect("building");
            g.sim.colonies[ci]
                .buildings
                .push(vector_sim::colony::BuildingInst {
                    def: d,
                    progress: 0.4,
                    enabled: true,
                    recipe: 0,
                    health: 1.0,
                });
            for (id, kg) in [
                ("food", 4000.0),
                ("water", 2000.0),
                ("iron", 2500.0),
                ("steel", 800.0),
                ("construction_materials", 1500.0),
                ("electronics", 300.0),
            ] {
                g.sim.colonies[ci].storage[data.resource_index(id).expect("res")] += kg;
            }
            g.sim.colonies[ci].pop = 90.0;
            g.sim.time += 20.0 * 86_400.0;
            g.sim.update_colonies_pub();
            g.col_idx = ci;
            g.col_focus = 3;
            g.build_sel = 3;
            g.screen = Screen::Colony;
            if scene == "flow" {
                g.col_flow = true;
            }
            if scene == "empire" {
                g.screen = Screen::Empire;
            }
            if scene == "fleet" || scene == "fleetmap" {
                let (st, _) = g.sim.nearest_station().expect("station");
                g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
                g.sim.try_dock().expect("dock");
                let hauler = data.hull_index("hauler").expect("hull");
                g.sim.researched.iter_mut().for_each(|r| *r = true);
                g.sim.buy_fleet_ship(hauler).expect("ship");
                g.sim
                    .buy_fleet_ship(data.hull_index("freighter").expect("hull"))
                    .expect("ship");
                g.endpoints = g.sim.known_endpoints();
                let iron = data.resource_index("iron").expect("iron");
                let from = g.endpoints[0].clone();
                let to = g.endpoints.last().cloned().expect("endpoint");
                g.sim.create_route(from, to, iron, 8000.0).expect("route");
                g.sim.assign_route(0, 0).expect("assign");
                g.sim.advance(3.0 * 86_400.0, &ShipInput::default());
                g.screen = Screen::Fleet;
                g.fleet_map = scene == "fleetmap";
                g.fleet_focus = 1;
            }
        }
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
        "crew" | "crewnet" | "recruit" | "journal" | "medicine" => {
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
            let days = if scene == "medicine" { 2.0 } else { 40.0 };
            g.sim.advance(days * 86_400.0, &ShipInput::default());
            g.screen = match scene {
                "crew" | "crewnet" | "medicine" => {
                    g.crew_net = scene == "crewnet";
                    g.med_mode = scene == "medicine";
                    if g.med_mode {
                        // A few injuries to look at.
                        let d = Arc::clone(&g.data);
                        let b = &mut g.sim.crew.members[0].body;
                        b.add_wound(&d, "fracture_os", 4, 0.7);
                        b.add_wound(&d, "coupure", 2, 0.4);
                        b.add_wound(&d, "hemorragie_interne", 1, 0.3);
                        b.dress_wounds(&d, 3.0);
                        b.take_drug(&d, 1);
                        b.disability[5] = 0.3;
                    }
                    Screen::Crew
                }
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
        "ship_inside" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            g.sim.credits = 3_000_000.0;
            g.advance(0.2);
            let bi = g.sim.buy_plan(2).expect("plan");
            g.sim.order_ship(bi, 4).expect("order");
            if let Some(o) = g.sim.yard_orders.first() {
                g.sim.time = o.ready + 1.0;
            }
            g.advance(0.2);
            g.sim.ship.cargo.mass = 0.0;
            g.sim.board_fleet_ship(0).expect("board");
            let d = Arc::clone(&g.data);
            for k in 0..3 {
                let p = vector_sim::crew::generate_person(&d, 900 + k, None);
                g.sim.crew.hire(&d, p, 1, 0.0);
            }
            let theme = d
                .rogue_themes
                .iter()
                .position(|t| t.id == "vaisseau")
                .expect("theme");
            g.sim.rogue_begin_social(theme, 5, vec![0]);
            g.screen = Screen::Rogue;
        }
        "designer" | "designer_fit" | "designer_plans" => {
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            g.sim.credits = 3_000_000.0;
            g.advance(0.2);
            for i in [0usize, 1, 3] {
                let _ = g.sim.buy_plan(i);
            }
            g.dz_open();
            g.dz.bp = 0;
            g.dz.cursor = (4, 0);
            g.dz.layer = 1;
            g.dz.part = 6;
            g.dz.yaw = 0.7;
            g.dz.tab = match scene {
                "designer_fit" => 1,
                "designer_plans" => 2,
                _ => 0,
            };
            if scene == "designer_plans" {
                let _ = g.sim.order_ship(0, 2);
                if let Some(o) = g.sim.yard_orders.first() {
                    g.sim.time = o.ready + 1.0;
                }
                g.advance(0.2);
                let _ = g.sim.order_ship(2, 3);
                g.dz.column = 2;
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
    if let Some(i) = args.iter().position(|a| a == "--script") {
        let Some(path) = args.get(i + 1) else {
            eprintln!("usage: vector-game --script <file>");
            std::process::exit(2);
        };
        match script::run(path, data, draw_frame) {
            Ok(0) => println!("SCRIPT OK"),
            Ok(n) => {
                println!("SCRIPT FAILED: {n} expectation(s)");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("SCRIPT ERROR: {e}");
                std::process::exit(2);
            }
        }
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
    let audio = audio::Audio::new();
    if !audio.available() {
        eprintln!("audio: no output device, running silent");
    }
    let mut app = App {
        window: None,
        surface: None,
        fb: Framebuffer::new(1024, 576),
        game: Game::new(data, DEFAULT_SEED),
        audio,
        last: Instant::now(),
        acc: 0.0,
        mouse_acc: (0.0, 0.0),
        grabbed: false,
        started: Instant::now(),
    };
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("run failed: {e}");
    }
}
