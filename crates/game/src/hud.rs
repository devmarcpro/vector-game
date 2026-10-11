//! Cockpit view and system map: 3D scene + ASCII HUD.
use crate::console::Console;
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer, Wireframe};
use vector_sim::design::{T_DAMAGE, T_SCRAM};
use vector_sim::npc::NpcRole;
use vector_sim::voyage::{SCAN_FULL, SCAN_SURVEYED};
use vector_sim::Sim;
use vector_world::BodyKind;

pub const GREEN: u32 = 0x0030_E050;
pub const DIM: u32 = 0x0010_6020;
pub const AMBER: u32 = 0x00E0_A020;
pub const RED: u32 = 0x00FF_3030;
pub const STAR: u32 = 0x0040_5060;
pub const SUN: u32 = 0x00E0_D060;
pub const MINER: u32 = 0x00C0_C040;
pub const PATROL: u32 = 0x0040_80FF;

pub fn npc_color(r: NpcRole) -> u32 {
    match r {
        NpcRole::Trader => GREEN,
        NpcRole::Miner => MINER,
        NpcRole::Pirate => RED,
        NpcRole::Patrol => PATROL,
    }
}

pub struct View<'a> {
    pub sim: &'a Sim,
    pub stars: &'a [Vec3],
    pub timescale: f64,
    pub paused: bool,
    pub messages: &'a [String],
    pub console: &'a Console,
    pub map: bool,
    /// Metres per pixel on the system map.
    pub map_scale: f64,
    pub map_on_ship: bool,
}

pub fn fmt_dist(m: f64) -> String {
    let a = m.abs();
    if a < 10_000.0 {
        format!("{m:.0} m")
    } else if a < 1.0e9 {
        format!("{:.1} km", m / 1e3)
    } else if a < 1.0e12 {
        format!("{:.2} Gm", m / 1e9)
    } else {
        format!("{:.3} AU", m / 1.496e11)
    }
}

pub fn fmt_time(s: f64) -> String {
    if s < 3600.0 {
        format!("{:.0}s", s)
    } else if s < 86400.0 {
        format!("{:.1}h", s / 3600.0)
    } else {
        format!("{:.1}j", s / 86400.0)
    }
}

pub fn draw(fb: &mut Framebuffer, v: &View) {
    fb.clear();
    if v.map {
        draw_map(fb, v);
    } else {
        draw_cockpit(fb, v);
    }
    draw_status(fb, v);
    draw_console(fb, v.console);
}

fn body_color(v: &View, i: usize) -> u32 {
    if Some(i) == v.sim.target {
        AMBER
    } else {
        GREEN
    }
}

fn draw_cockpit(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    let cam = Camera {
        pos: ship.pos,
        rot: ship.rot.transpose(),
        fov_y: 1.2,
    };
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();

    for s in v.stars {
        if let Some((x, y)) = cam.project_dir(*s, fb) {
            fb.plot(x as i32, y as i32, STAR);
        }
    }
    for (i, b) in v.sim.bodies.iter().enumerate() {
        if v.sim.scan[i] == 0 {
            continue;
        }
        let col = body_color(v, i);
        let dist = (b.pos - ship.pos).length();
        let px = b.radius / dist.max(1.0) * focal;
        let spin = Mat3::rot_y(v.sim.time * 2.0e-5 + i as f64);
        match b.kind {
            BodyKind::Star(_) => cam.draw_sphere(fb, b.pos, b.radius, &spin, 4, 8, SUN),
            BodyKind::Planet(k) if k.is_giant() => {
                cam.draw_sphere(fb, b.pos, b.radius, &spin, 12, 6, col)
            }
            BodyKind::Planet(_) => cam.draw_sphere(fb, b.pos, b.radius, &spin, 8, 14, col),
            BodyKind::Moon(_) => cam.draw_sphere(fb, b.pos, b.radius, &spin, 5, 8, col),
            BodyKind::Anomaly(_) => {
                // Anomalies: a lattice sphere that pulses, so they do not look like rocks.
                let pulse = 1.0 + 0.08 * (v.sim.time * 0.7).sin();
                cam.draw_sphere(fb, b.pos, b.radius * pulse, &spin, 3, 5, 0x00C0_60FF);
            }
            BodyKind::Asteroid(_) => cam.draw_sphere(fb, b.pos, b.radius, &spin, 4, 6, col),
            BodyKind::Station => {
                let r = Mat3::rot_y(v.sim.time * 0.2);
                Wireframe::cube(b.radius * 0.5).draw(&cam, fb, &r, b.pos, col);
                let ring = Mat3::rot_x(std::f64::consts::FRAC_PI_2).mul(&r);
                cam.draw_sphere(fb, b.pos, b.radius * 0.9, &ring, 2, 0, col);
            }
        }
        // Marker + label for objects too small to see as shapes.
        if let Some((x, y)) = cam.project_world(b.pos, fb) {
            if px < 6.0 && !matches!(b.kind, BodyKind::Star(_)) {
                fb.line(x - 3.0, y, x + 3.0, y, col);
                fb.line(x, y - 3.0, x, y + 3.0, col);
            }
            if Some(i) == v.sim.target
                || (matches!(b.kind, BodyKind::Planet(_) | BodyKind::Station) && px < 40.0)
            {
                fb.text_at(x as i32 + 8, y as i32 - 12, body_label(v, i), col);
            }
        }
    }

    let down = ship.rot.col(1) * -6.0;
    for (a, b, mine) in &v.sim.beams {
        cam.draw_line(fb, *a + down, *b, if *mine { RED } else { AMBER });
    }
    for (a, b) in &v.sim.enemy_beams {
        cam.draw_line(fb, *a, *b, RED);
    }
    for p in &v.sim.projectiles {
        let tail = p.pos - p.vel.normalized() * 40.0;
        let col = match p.kind {
            vector_sim::combat::ProjKind::Kinetic => AMBER,
            vector_sim::combat::ProjKind::Missile { .. } => 0x00FF_8040,
            vector_sim::combat::ProjKind::Mine { .. } => 0x00A0_A0A0,
        };
        cam.draw_line(fb, tail, p.pos, col);
    }
    for w in &v.sim.wrecks {
        if let Some((x, y)) = cam.project_world(w.pos, fb) {
            fb.line(x - 4.0, y - 4.0, x + 4.0, y + 4.0, MINER);
            fb.line(x - 4.0, y + 4.0, x + 4.0, y - 4.0, MINER);
            if (w.pos - ship.pos).length() < 2.0e5 {
                fb.text_at(x as i32 + 8, y as i32 + 6, &w.name, MINER);
            }
        }
    }
    let see = 1.0e6 + 0.01 * v.sim.effective.scan_range;
    let ship_wire = Wireframe::ship();
    for (k, n) in v.sim.npcs.iter().enumerate() {
        if !n.visible() {
            continue;
        }
        let d = (n.pos - ship.pos).length();
        if d > see {
            continue;
        }
        let col = if Some(k) == v.sim.target_npc {
            AMBER
        } else {
            npc_color(n.role)
        };
        if d < 6.0e4 {
            let fwd = if n.vel.length() > 1.0 {
                n.vel
            } else {
                Vec3::new(0.0, 0.0, 1.0)
            };
            let rot = Mat3::look_at(fwd, Vec3::new(0.0, 1.0, 0.0));
            let scaled = Wireframe {
                verts: ship_wire.verts.iter().map(|p| *p * 80.0).collect(),
                edges: ship_wire.edges.clone(),
            };
            scaled.draw(&cam, fb, &rot, n.pos, col);
        }
        if let Some((x, y)) = cam.project_world(n.pos, fb) {
            fb.line(x - 3.0, y, x, y - 3.0, col);
            fb.line(x, y - 3.0, x + 3.0, y, col);
            fb.line(x + 3.0, y, x, y + 3.0, col);
            fb.line(x, y + 3.0, x - 3.0, y, col);
            if Some(k) == v.sim.target_npc || d < 3.0e5 {
                fb.text_at(x as i32 + 6, y as i32 + 6, &n.name, col);
            }
            if Some(k) == v.sim.target_npc {
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    fb.line(
                        x + sx * 14.0,
                        y + sy * 14.0,
                        x + sx * 9.0,
                        y + sy * 14.0,
                        AMBER,
                    );
                    fb.line(
                        x + sx * 14.0,
                        y + sy * 14.0,
                        x + sx * 14.0,
                        y + sy * 9.0,
                        AMBER,
                    );
                }
            }
        }
    }

    let (w, h) = (fb.w as f32, fb.h as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    for (dx, dy, ex, ey) in [
        (-14.0, 0.0, 6.0, 0.0),
        (8.0, 0.0, 6.0, 0.0),
        (0.0, -14.0, 0.0, 6.0),
        (0.0, 8.0, 0.0, 6.0),
    ] {
        fb.line(cx + dx, cy + dy, cx + dx + ex, cy + dy + ey, GREEN);
    }
    // Prograde marker (relative to the reference frame).
    let rel = v.sim.rel_velocity();
    if rel.length() > 0.5 {
        if let Some((x, y)) = cam.project_dir(rel.normalized(), fb) {
            fb.circle(x, y, 6.0, GREEN);
            fb.line(x - 10.0, y, x - 6.0, y, GREEN);
            fb.line(x + 6.0, y, x + 10.0, y, GREEN);
            fb.line(x, y - 10.0, x, y - 6.0, GREEN);
        }
    }
    if let Some(t) = v.sim.target.and_then(|i| v.sim.bodies.get(i)) {
        if let Some((x, y)) = cam.project_world(t.pos, fb) {
            let r = 16.0;
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                fb.line(
                    x + sx * r,
                    y + sy * r,
                    x + sx * (r - 6.0),
                    y + sy * r,
                    AMBER,
                );
                fb.line(
                    x + sx * r,
                    y + sy * r,
                    x + sx * r,
                    y + sy * (r - 6.0),
                    AMBER,
                );
            }
        }
    }
    if v.sim.laser_on {
        if let Some((x, y)) = v
            .sim
            .target
            .and_then(|i| cam.project_world(v.sim.bodies[i].pos, fb))
        {
            let flick = (v.sim.time * 40.0).sin() as f32 * 2.0;
            fb.line(cx - 30.0, h - 90.0, x + flick, y, RED);
            fb.line(cx + 30.0, h - 90.0, x - flick, y, RED);
            fb.circle(x, y, 4.0 + flick.abs(), AMBER);
        }
    }
    draw_radar(fb, v);
}

fn draw_map(fb: &mut Framebuffer, v: &View) {
    let sim = v.sim;
    let center = if v.map_on_ship {
        sim.ship.pos
    } else {
        Vec3::ZERO
    };
    let (cx, cy) = (fb.w as f64 / 2.0, fb.h as f64 / 2.0);
    let to_px = |p: Vec3| -> (f32, f32) {
        (
            (cx + (p.x - center.x) / v.map_scale) as f32,
            (cy + (p.z - center.z) / v.map_scale) as f32,
        )
    };
    for (i, cb) in sim.system.bodies.iter().enumerate() {
        let b = &sim.bodies[i];
        let col = body_color(v, i);
        if let Some(o) = &cb.orbit {
            if o.a / v.map_scale > 3.0 {
                let parent = sim.bodies[o.parent].pos;
                let period = o.period();
                let n = 120;
                let mut prev: Option<(f32, f32)> = None;
                for k in 0..=n {
                    let t = sim.time + period * k as f64 / n as f64;
                    let (rp, _) = o.state(t);
                    let p = to_px(parent + rp);
                    if let Some(q) = prev {
                        fb.line(q.0, q.1, p.0, p.1, DIM);
                    }
                    prev = Some(p);
                }
            }
        }
        let (x, y) = to_px(b.pos);
        let r = (b.radius / v.map_scale).clamp(2.0, 14.0) as f32;
        match cb.kind {
            BodyKind::Star(_) => fb.circle(x, y, r.max(4.0), SUN),
            BodyKind::Asteroid(_) => fb.plot(x as i32, y as i32, col),
            BodyKind::Anomaly(_) => {
                fb.circle(x, y, 4.0, 0x00C0_60FF);
                fb.plot(x as i32, y as i32, 0x00C0_60FF);
            }
            BodyKind::Station => {
                fb.line(x - 3.0, y - 3.0, x + 3.0, y + 3.0, col);
                fb.line(x - 3.0, y + 3.0, x + 3.0, y - 3.0, col);
            }
            _ => fb.circle(x, y, r.min(8.0), col),
        }
        let shown = match cb.kind {
            BodyKind::Asteroid(_) => false,
            BodyKind::Anomaly(_) => true,
            BodyKind::Moon(_) | BodyKind::Station => {
                cb.orbit.as_ref().is_some_and(|o| o.a / v.map_scale > 12.0)
            }
            _ => true,
        };
        if shown || Some(i) == v.sim.target {
            fb.text_at(x as i32 + 6, y as i32 - 12, &cb.name, col);
        }
    }
    let (sx, sy) = to_px(sim.ship.pos);
    fb.line(sx - 5.0, sy, sx + 5.0, sy, RED);
    fb.line(sx, sy - 5.0, sx, sy + 5.0, RED);
    fb.text_at(sx as i32 + 8, sy as i32 + 4, "VAISSEAU", RED);
    let rows = fb.rows();
    fb.text(
        1,
        rows - 5,
        &format!(
            "CARTE SYSTEME  ECHELLE {}/PX  [+/-] ZOOM  [C] CENTRE:{}",
            fmt_dist(v.map_scale),
            if v.map_on_ship { "VAISSEAU" } else { "ETOILE" }
        ),
        DIM,
    );
}

fn draw_status(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    let sim = v.sim;
    let rows = fb.rows();
    let cols = fb.cols();
    let rel = sim.rel_velocity();
    let frame = sim
        .ref_body
        .map_or("ETOILE".to_string(), |i| sim.bodies[i].name.clone());
    fb.text(
        1,
        1,
        &format!(
            "{} ({})  CLASSE {}",
            sim.system.name,
            sim.system.designation,
            sim.system.star_class.letter()
        ),
        GREEN,
    );
    fb.text(
        1,
        2,
        &format!("VITESSE {}/S  REF {}", fmt_dist(rel.length()), frame),
        GREEN,
    );
    fb.text(
        1,
        3,
        &format!(
            "ASSIST {}  TEMPS x{}{}  T+{}",
            if ship.assist { "ON" } else { "OFF" },
            v.timescale,
            if v.paused { " PAUSE" } else { "" },
            fmt_time(sim.time)
        ),
        if ship.assist { GREEN } else { AMBER },
    );
    if let Some(ap) = &sim.autopilot {
        fb.text(
            1,
            4,
            &format!(
                "PILOTE AUTO -> {}  A={:.2} M/S2",
                sim.bodies[ap.target].name, ap.a_cruise
            ),
            AMBER,
        );
    }
    fb.gauge(
        1,
        rows - 4,
        "CARBURANT",
        ship.fuel / ship.stats.fuel_cap.max(1.0),
        20,
        if ship.fuel < ship.stats.fuel_cap * 0.1 {
            RED
        } else {
            GREEN
        },
    );
    fb.gauge(
        1,
        rows - 3,
        "COQUE    ",
        ship.hull / ship.stats.hull_max.max(1.0),
        20,
        if ship.hull < ship.stats.hull_max * 0.3 {
            RED
        } else {
            GREEN
        },
    );
    if v.sim.effective.shield_cap > 0.0 {
        let frac = ship.sys.shield_pts / v.sim.effective.shield_cap;
        fb.gauge(
            1,
            rows - 7,
            "BOUCLIER ",
            frac,
            20,
            if frac < 0.25 { RED } else { PATROL },
        );
    }
    let batt = ship.sys.battery_mj / ship.stats.battery_cap.max(1.0);
    fb.gauge(
        1,
        rows - 6,
        "BATTERIE ",
        batt,
        20,
        if batt < 0.15 { RED } else { GREEN },
    );
    let temp_frac = ((ship.sys.temp_k - 150.0) / (T_SCRAM - 150.0)).clamp(0.0, 1.0);
    let temp_col = if ship.sys.temp_k > T_DAMAGE {
        RED
    } else if ship.sys.temp_k > 500.0 {
        AMBER
    } else {
        GREEN
    };
    fb.gauge(1, rows - 5, "TEMP     ", temp_frac, 20, temp_col);
    fb.text(
        28,
        rows - 5,
        &format!(
            "{:.0} K  {:.1}/{:.1} MW",
            ship.sys.temp_k,
            ship.sys.demand_kw / 1e3,
            ship.sys.supply_kw / 1e3
        ),
        DIM,
    );
    fb.text(
        1,
        rows - 2,
        &format!("DELTA-V {:.1} KM/S", ship.delta_v() / 1e3),
        DIM,
    );

    fb.text(1, 5, &format!("CREDITS {:.0}", sim.credits), GREEN);
    fb.text(
        1,
        6,
        &format!("SOUTE {:.0}/{:.0} KG", ship.cargo.mass, ship.cargo.cap_mass),
        if ship.cargo.is_full() { RED } else { GREEN },
    );
    fb.text(
        1,
        8,
        &format!(
            "EQUIPAGE {}/{:.0}  AIR {:.0}%",
            sim.crew.count(),
            ship.stats.crew_cap,
            sim.crew.air * 100.0
        ),
        if sim.crew.air < 0.5 { RED } else { GREEN },
    );
    if let Some((_, range)) = sim.nearest_station() {
        if range < vector_sim::DOCK_RANGE * 4.0 {
            fb.text(
                1,
                7,
                &format!("STATION A {} - [D] AMARRER", fmt_dist(range)),
                AMBER,
            );
        }
    }
    let terr = match sim.faction {
        Some(f) => format!(
            "TERRITOIRE {}  REP {:+.0}",
            sim.data.factions[f].name.to_uppercase(),
            sim.reputation_with(Some(f))
        ),
        None => "ESPACE NON REVENDIQUE".to_string(),
    };
    fb.text(
        1,
        10,
        &terr,
        sim.faction.map_or(DIM, |f| sim.data.factions[f].color),
    );
    {
        let env = sim.environment_at(ship.pos);
        let mut danger = Vec::new();
        if env.radiation > 0.0 {
            danger.push(format!("RADIATIONS {:.1} GY/H", env.radiation));
        }
        if env.ion > 0.0 {
            danger.push("TEMPETE IONIQUE".to_string());
        }
        if env.gravity.length() > 0.5 {
            danger.push(format!(
                "PUITS GRAVITATIONNEL {:.1} M/S2",
                env.gravity.length()
            ));
        }
        if env.sensor_mult < 0.9 {
            danger.push("CAPTEURS BRUITES".to_string());
        }
        if env.agility_mult < 0.9 {
            danger.push("INTERFERENCES MAGNETIQUES".to_string());
        }
        if env.rift > 0.0 {
            danger.push("FAILLE SPATIALE".to_string());
        }
        if !danger.is_empty() {
            fb.text(1, 13, &format!("DANGER : {}", danger.join(" / ")), RED);
        }
        let cur = sim.current_research.map_or("AUCUNE".to_string(), |t| {
            format!(
                "{} {:.0}%",
                sim.data.techs[t].name.to_uppercase(),
                100.0 * sim.research_progress / sim.data.techs[t].cost.max(1.0)
            )
        });
        fb.text(
            1,
            14,
            &format!(
                "RECHERCHE : {cur}   RESERVE {:.0} PTS  [F4]",
                sim.research_pool
            ),
            DIM,
        );
    }
    if sim.is_wanted() {
        fb.text(1, 11, "RECHERCHE DANS CE TERRITOIRE !", RED);
    }
    let ammo = |id: &str| {
        sim.data
            .resource_index(id)
            .map_or(0.0, |r| ship.cargo.kg[r])
    };
    if ship
        .design
        .modules(&sim.data)
        .any(|(_, m)| !m.weapon.is_empty())
    {
        fb.text(
            1,
            12,
            &format!(
                "MUNITIONS  OBUS {:.0}  MISSILES {:.0}  MINES {:.0}",
                ammo("ammo_slugs"),
                ammo("missiles"),
                ammo("mines")
            ),
            DIM,
        );
    }
    draw_target_panel(fb, v, rows, cols);
    if let Some(j) = &sim.jump {
        let line = if j.in_transit {
            let done = 1.0 - j.transit_left / j.total_transit.max(1.0);
            format!(
                "TRANSIT FTL {:.0}%  RESTE {}",
                done * 100.0,
                fmt_time(j.transit_left)
            )
        } else {
            format!(
                "CHARGE FTL {:.0}S  [RETOUR ARR.] ANNULER",
                j.charge_left.max(0.0)
            )
        };
        fb.text(1, 9, &line, AMBER);
    }
    for (i, m) in v.messages.iter().rev().take(4).enumerate() {
        fb.text(
            cols / 2 - m.len().min(cols) / 2,
            2 + i,
            m,
            if i == 0 { AMBER } else { DIM },
        );
    }
    if ship.hull <= 0.0 {
        fb.text(cols / 2 - 10, rows / 2 + 4, "*** COQUE DETRUITE ***", RED);
    }
}

fn draw_target_panel(fb: &mut Framebuffer, v: &View, rows: usize, cols: usize) {
    let sim = v.sim;
    let ship = &sim.ship;
    let x = cols.saturating_sub(40);
    if let Some(n) = sim.target_npc.and_then(|k| sim.npcs.get(k)) {
        let r = n.pos - ship.pos;
        let closing = (ship.vel - n.vel).dot(r.normalized());
        let faction = n.faction.map_or("INDEPENDANT".to_string(), |f| {
            sim.data.factions[f].name.to_uppercase()
        });
        let status = match &n.state {
            vector_sim::npc::NpcState::Docked { .. } => "AMARRE / EN ATTENTE",
            vector_sim::npc::NpcState::Travel { .. } => "EN TRANSIT",
            vector_sim::npc::NpcState::Working { .. } => "AU TRAVAIL",
            vector_sim::npc::NpcState::Lurk { .. } => "EN EMBUSCADE",
            vector_sim::npc::NpcState::Orbit { .. } => "EN PATROUILLE",
            vector_sim::npc::NpcState::Away { .. } => "ABSENT",
            vector_sim::npc::NpcState::Combat { .. } => {
                if n.disabled {
                    "NEUTRALISE (ABORDABLE)"
                } else if n.hostile {
                    "HOSTILE !"
                } else {
                    "EN ALERTE"
                }
            }
        };
        fb.text(x, rows - 8, &format!("CIBLE {}", n.name), AMBER);
        fb.text(
            x,
            rows - 7,
            &format!("TYPE  {}  {}", n.role.label(), faction),
            AMBER,
        );
        fb.text(
            x,
            rows - 6,
            &format!("DIST  {}", fmt_dist(r.length())),
            AMBER,
        );
        fb.text(x, rows - 5, &format!("APPR  {closing:+.1} M/S"), AMBER);
        fb.text(x, rows - 4, status, if n.hostile { RED } else { DIM });
        fb.text(
            x,
            rows - 2,
            &format!(
                "COQUE {:.0}%  BOUCLIER {:.0}%",
                100.0 * n.hull / n.hull_max.max(1.0),
                100.0 * n.shield / n.shield_max.max(1.0)
            ),
            if n.hostile { RED } else { DIM },
        );
        if let Some((res, kg)) = n.cargo {
            fb.text(
                x,
                rows - 3,
                &format!(
                    "SOUTE {:.0} KG {}",
                    kg,
                    sim.data.resources[res].name.to_uppercase()
                ),
                DIM,
            );
        }
        return;
    }
    let Some(i) = sim.target else {
        fb.text(x, rows - 2, "AUCUNE CIBLE [TAB]", DIM);
        return;
    };
    let t = &sim.bodies[i];
    let cb = &sim.system.bodies[i];
    let level = sim.scan[i];
    let r = t.pos - ship.pos;
    let range = r.length() - t.radius;
    let closing = (ship.vel - t.vel).dot(r.normalized());
    fb.text(x, rows - 8, &format!("CIBLE {}", body_label(v, i)), AMBER);
    fb.text(x, rows - 6, &format!("DIST  {}", fmt_dist(range)), AMBER);
    fb.text(x, rows - 5, &format!("APPR  {:+.1} M/S", closing), AMBER);
    if level < SCAN_SURVEYED {
        fb.text(x, rows - 7, "TYPE  NON IDENTIFIE", AMBER);
        fb.text(x, rows - 3, "APPROCHEZ POUR SCANNER", DIM);
        return;
    }
    fb.text(
        x,
        rows - 7,
        &format!("TYPE  {}", kind_label(cb.kind)),
        AMBER,
    );
    if let BodyKind::Anomaly(a) = cb.kind {
        let def = sim.data.anomaly_def(a.id());
        fb.text(
            x,
            rows - 4,
            &format!(
                "RAYON {}  ZONE {}",
                fmt_dist(t.radius),
                fmt_dist(def.map_or(0.0, |d| d.radius))
            ),
            DIM,
        );
        let text = def.map_or(String::new(), |d| d.text.to_uppercase());
        fb.text(x, rows - 3, &text.chars().take(38).collect::<String>(), DIM);
        fb.text(
            x,
            rows - 2,
            if sim.explored.get(i).copied().unwrap_or(false) {
                "EXPLOREE"
            } else {
                "SCAN COMPLET = RECOMPENSES"
            },
            DIM,
        );
        return;
    }
    fb.text(
        x,
        rows - 4,
        &format!("RAYON {}  G {:.1}", fmt_dist(t.radius), cb.gravity),
        DIM,
    );
    fb.text(
        x,
        rows - 3,
        &format!("TEMP  {:.0} K  PRES {:.2} BAR", cb.temp, cb.pressure),
        DIM,
    );
    if level < SCAN_FULL {
        fb.text(x, rows - 2, "SCAN COMPLET: PLUS PRES", DIM);
        return;
    }
    if let BodyKind::Asteroid(class) = cb.kind {
        let range_laser = ship
            .design
            .modules(&sim.data)
            .map(|(_, m)| m.mining_range)
            .fold(0.0, f64::max);
        let comp: Vec<String> = sim
            .data
            .asteroids
            .iter()
            .find(|a| a.id == class.id())
            .map(|a| {
                let mut c = a.composition.clone();
                c.sort_by(|x, y| y.1.total_cmp(&x.1));
                c.iter()
                    .take(3)
                    .map(|(r, f)| format!("{r} {:.0}%", f * 100.0))
                    .collect()
            })
            .unwrap_or_default();
        fb.text(
            x,
            rows - 2,
            &format!(
                "MASSE {:.0} T  {}  LASER<{}",
                sim.remaining_mass(i) / 1e3,
                comp.join(" "),
                fmt_dist(range_laser)
            ),
            DIM,
        );
    } else if !cb.atmosphere.is_empty() {
        let a: Vec<String> = cb
            .atmosphere
            .iter()
            .map(|(g, f)| format!("{g} {:.0}%", f * 100.0))
            .collect();
        fb.text(x, rows - 2, &a.join(" "), DIM);
    }
}

/// Name shown for a body, depending on how well it is known.
pub fn body_label<'a>(v: &'a View, i: usize) -> &'a str {
    if v.sim.scan[i] >= SCAN_SURVEYED {
        &v.sim.bodies[i].name
    } else {
        "OBJET INCONNU"
    }
}

pub fn kind_label(k: BodyKind) -> String {
    match k {
        BodyKind::Star(c) => format!("ETOILE {}", c.letter()),
        BodyKind::Planet(p) => format!("PLANETE {}", p.label()),
        BodyKind::Moon(p) => format!("LUNE {}", p.label()),
        BodyKind::Asteroid(t) => format!("ASTEROIDE TYPE {}", t.id()),
        BodyKind::Anomaly(a) => format!("ANOMALIE : {}", a.label().to_uppercase()),
        BodyKind::Station => "STATION".into(),
    }
}

fn draw_radar(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    let (cx, cy) = (fb.w as f32 / 2.0, fb.h as f32 - 34.0);
    let (rx, ry) = (80.0f32, 26.0f32);
    let range = 20_000.0f64;
    for k in [1.0, 0.5] {
        ellipse(fb, cx, cy, rx * k, ry * k, DIM);
    }
    fb.line(cx - rx, cy, cx + rx, cy, DIM);
    fb.line(cx, cy - ry, cx, cy + ry, DIM);
    radar_npcs(fb, v, cx, cy, rx, ry, range);
    let inv = ship.rot.transpose();
    for (i, b) in v.sim.bodies.iter().enumerate() {
        if matches!(b.kind, BodyKind::Star(_)) || v.sim.scan[i] == 0 {
            continue;
        }
        let l = inv.mul_vec(b.pos - ship.pos);
        let flat = (l.x * l.x + l.z * l.z).sqrt();
        let scale = if flat > range { range / flat } else { 1.0 };
        let px = cx + (l.x * scale / range) as f32 * rx;
        let py = cy - (l.z * scale / range) as f32 * ry;
        let top = py - (l.y * scale / range) as f32 * ry * 1.5;
        let col = body_color(v, i);
        fb.line(px, py, px, top, col);
        fb.plot(px as i32, top as i32, col);
        fb.plot(px as i32 + 1, top as i32, col);
    }
}

fn radar_npcs(fb: &mut Framebuffer, v: &View, cx: f32, cy: f32, rx: f32, ry: f32, range: f64) {
    let ship = &v.sim.ship;
    let inv = ship.rot.transpose();
    for (k, n) in v.sim.npcs.iter().enumerate() {
        if !n.visible() {
            continue;
        }
        let l = inv.mul_vec(n.pos - ship.pos);
        let flat = (l.x * l.x + l.z * l.z).sqrt();
        if flat > range * 3.0 {
            continue;
        }
        let scale = if flat > range { range / flat } else { 1.0 };
        let px = cx + (l.x * scale / range) as f32 * rx;
        let py = cy - (l.z * scale / range) as f32 * ry;
        let col = if Some(k) == v.sim.target_npc {
            AMBER
        } else {
            npc_color(n.role)
        };
        fb.plot(px as i32, py as i32, col);
        fb.plot(px as i32 + 1, py as i32, col);
        fb.plot(px as i32, py as i32 + 1, col);
    }
}

fn ellipse(fb: &mut Framebuffer, cx: f32, cy: f32, rx: f32, ry: f32, c: u32) {
    let mut prev = (cx + rx, cy);
    for i in 1..=48 {
        let a = i as f32 / 48.0 * std::f32::consts::TAU;
        let p = (cx + rx * a.cos(), cy + ry * a.sin());
        fb.line(prev.0, prev.1, p.0, p.1, c);
        prev = p;
    }
}

pub fn draw_console(fb: &mut Framebuffer, c: &Console) {
    if !c.open {
        return;
    }
    let lines = c.log.len() + 1;
    for y in 0..(lines * 10 + 4) {
        for x in 0..fb.w {
            fb.plot(x as i32, y as i32, 0x0000_0800);
        }
    }
    for (i, l) in c.log.iter().enumerate() {
        fb.text(1, i, l, DIM);
    }
    fb.text(1, c.log.len(), &format!("> {}_", c.line), GREEN);
}
