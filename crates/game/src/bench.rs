//! Reproducible headless benchmarks: `--bench-planet [frames]` (a world filling more or less of
//! the screen, in orbit and low over the ground, day and night) and `--bench-all [frames]` (start-up,
//! the simulation per step, save and load, every screen). Single-threaded renders are timed with
//! the profiler of `perf.rs` for the stages.
use crate::game::{Game, DEFAULT_SEED};
use std::sync::Arc;
use std::time::Instant;
use vector_core::{Mat3, Vec3};
use vector_data::GameData;
use vector_render::Framebuffer;
use vector_sim::ShipInput;
use vector_world::{BodyKind, PlanetKind};

const BUDGET_MS: f64 = 16.7;

fn frames_arg(args: &[String], flag: &str, default: usize) -> usize {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Time `n` frames of the current screen after one warm-up image (which builds the caches):
/// (best ms, median ms, CPU ms per image over all threads, worst warm-up image ms). The best is the figure closest to a quiet machine.
fn time_frames(fb: &mut Framebuffer, g: &Game, n: usize) -> (f64, f64, f64, f64) {
    // Warm-up: the caches fill over several images; the worst of them is the hitch.
    let mut hitch = 0.0f64;
    for _ in 0..60 {
        let t = Instant::now();
        crate::draw_frame(fb, g);
        hitch = hitch.max(t.elapsed().as_secs_f64() * 1000.0);
        let spans = crate::perf::take();
        if !spans.iter().any(|s| s.0 == "ball_bake") {
            break;
        }
    }
    let mut v = Vec::with_capacity(n);
    let cpu0 = crate::perf::cpu_ms();
    for _ in 0..n {
        let t = Instant::now();
        crate::draw_frame(fb, g);
        v.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let cpu = (crate::perf::cpu_ms() - cpu0) / n as f64;
    v.sort_by(f64::total_cmp);
    (v[0], v[v.len() / 2], cpu, hitch)
}

fn stages(n: usize) -> String {
    let mut v = crate::perf::take();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v.iter()
        .take(5)
        .map(|(name, ms, _)| format!("{name}={:.2}", ms / n as f64))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Put the ship at `frac` of the screen height for the apparent diameter of body `i`, looking at
/// it from the lit side (`day`) or the dark one.
fn place(g: &mut Game, i: usize, frac: f64, day: bool, h: f64) {
    let b = g.sim.bodies[i].clone();
    let cam: f64 = 1.2;
    let focal = h * 0.5 / (cam * 0.5).tan();
    let r_px = (frac * h * 0.5).max(1.0);
    let dist = b.radius * (1.0 + (focal / r_px).powi(2)).sqrt();
    let sun = (b.pos * -1.0).normalized();
    let u = if day { sun } else { sun * -1.0 };
    g.sim.ship.pos = b.pos + u * dist;
    g.sim.ship.vel = b.vel;
    g.sim.ship.rot = Mat3::look_at(u * -1.0, Vec3::new(0.0, 1.0, 0.0));
    g.sim.target = Some(i);
    g.sim.update_ref_pub();
}

/// Put the ship `alt` metres over the ground of body `i`, looking along the ground.
fn place_low(g: &mut Game, i: usize, alt: f64) {
    let b = g.sim.bodies[i].clone();
    let sun = (b.pos * -1.0).normalized();
    let side = sun.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
    let up = (sun + side * 0.45).normalized();
    g.sim.ship.pos = b.pos + up * (b.radius + alt);
    g.sim.ship.vel = b.vel;
    let fwd = up.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
    g.sim.ship.rot = Mat3::look_at(fwd * 0.97 + up * -0.12, up);
    g.sim.target = Some(i);
    g.sim.update_ref_pub();
}

pub fn planet(data: Arc<GameData>, args: &[String]) {
    if args.iter().any(|a| a == "--micro") {
        terrain_micro(data);
        return;
    }
    let n = frames_arg(args, "--bench-planet", 9);
    let (w, h) = (1280usize, 720usize);
    crate::perf::set(true);
    let mut fb = Framebuffer::new(w, h);
    let g_data = Arc::clone(&data);
    let mut g = Game::new(data, DEFAULT_SEED);
    // A system with a living world, a giant and a moon, to cover every kind of ball.
    let mut best = (0, DEFAULT_SEED);
    for seed in DEFAULT_SEED..DEFAULT_SEED + 300 {
        g.new_game(seed);
        let has = |f: &dyn Fn(BodyKind) -> bool| g.sim.bodies.iter().any(|b| f(b.kind));
        let score = usize::from(has(&|k| {
            matches!(k, BodyKind::Planet(PlanetKind::Terran | PlanetKind::Ocean))
        })) + usize::from(has(&|k| matches!(k, BodyKind::Planet(p) if p.is_giant())))
            + usize::from(has(&|k| matches!(k, BodyKind::Moon(_))))
            + usize::from(has(&|k| {
                matches!(k, BodyKind::Planet(PlanetKind::Barren | PlanetKind::Dwarf))
            }));
        if score > best.0 {
            best = (score, seed);
        }
        if score == 4 {
            break;
        }
    }
    g.new_game(best.1);
    println!("graine {}", best.1);
    g.sim.reveal_all();
    let find = |g: &Game, f: &dyn Fn(BodyKind) -> bool| g.sim.bodies.iter().position(|b| f(b.kind));
    let cases: Vec<(&str, Option<usize>)> = vec![
        (
            "vivante+air+nuages",
            find(&g, &|k| {
                matches!(k, BodyKind::Planet(PlanetKind::Terran | PlanetKind::Ocean))
            }),
        ),
        (
            "rocheuse sans air",
            find(&g, &|k| {
                matches!(k, BodyKind::Planet(PlanetKind::Barren | PlanetKind::Dwarf))
            }),
        ),
        (
            "geante",
            find(&g, &|k| matches!(k, BodyKind::Planet(p) if p.is_giant())),
        ),
        ("lune", find(&g, &|k| matches!(k, BodyKind::Moon(_)))),
    ];
    println!("cas (min = meilleure image, med = mediane, cpu = ms de processeur par image, tous fils confondus (insensible a la charge de la machine))");
    if let Some(i) = args.iter().position(|a| a == "--threads") {
        crate::ball::set_threads(args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0));
    }
    for (label, idx) in cases {
        let Some(i) = idx else { continue };
        for frac in [0.1, 0.5, 1.0, 2.0] {
            for day in [true, false] {
                place(&mut g, i, frac, day, h as f64);
                crate::perf::take();
                let (ms, med, first, hitch) = time_frames(&mut fb, &g, n);
                println!(
                    "{label:<24} {:>4.0}%  {:<5} min {ms:>7.2} med {med:>7.2} cpu {first:>7.1} pic {hitch:>6.0}  {}{}",
                    frac * 100.0,
                    if day { "jour" } else { "nuit" },
                    stages(n + 1),
                    if ms > BUDGET_MS { "  <-- lent" } else { "" }
                );
            }
        }
        // Low over the ground: the heightfield is marched.
        let r = g.sim.bodies[i].radius;
        for alt in [r * 0.02, r * 0.002, 200.0] {
            if matches!(g.sim.bodies[i].kind, BodyKind::Planet(p) if p.is_giant()) {
                break;
            }
            place_low(&mut g, i, alt.min(190_000.0));
            crate::perf::take();
            let (ms, med, first, hitch) = time_frames(&mut fb, &g, n);
            println!(
                "{label:<24} alt {:>8.0} m  min {ms:>7.2} med {med:>7.2} cpu {first:>7.1} pic {hitch:>6.0}  {}{}",
                alt.min(190_000.0),
                stages(n + 1),
                if ms > BUDGET_MS { "  <-- lent" } else { "" }
            );
        }
    }
    crate::perf::set(false);
    terrain_micro(Arc::clone(&g_data));
}

/// Screens to time (scene names of `--snapshot`).
const SCENES: &[&str] = &[
    "cockpit",
    "menu",
    "map",
    "nav_plan",
    "world",
    "world_near",
    "world_low",
    "worldgiant",
    "ext_world",
    "land_descent",
    "land_final",
    "planet",
    "giant",
    "cosmos",
    "surface",
    "surface_geo",
    "surface_clouds",
    "biosphere",
    "globe",
    "station",
    "stationview",
    "ship",
    "ship_inside",
    "designer",
    "inventory",
    "hub",
    "dash",
    "dash_foot",
    "dialogue",
    "newgame",
    "empire",
    "colony",
    "fleetmap",
    "flow",
    "industry",
    "farm",
    "city",
    "logistics",
    "galaxy",
    "crew",
    "medicine",
    "tech",
    "codex",
    "combat",
    "war",
    "societe",
    "bourse",
    "sky_ring",
    "galaxy_ism",
    "pause",
    "options",
    "help",
    "rogue_0",
    "castaway",
    "traffic",
    "projects",
    "walk",
    "walk_fpv",
    "expedition",
    "speed",
    "mine",
    "space",
    "anomaly",
    "haute_sf",
    "abordage",
    "archives",
    "esprit",
    "factions",
    "family",
];

fn quiet<T>(f: impl FnOnce() -> T) -> Option<T> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok();
    std::panic::set_hook(hook);
    r
}

pub fn all(data: Arc<GameData>, args: &[String]) {
    let n = frames_arg(args, "--bench-all", 7);
    if let Some(i) = args.iter().position(|a| a == "--threads") {
        crate::ball::set_threads(args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0));
    }
    crate::perf::set(true);
    if args.iter().any(|a| a == "--only") {
        // Screens only.
    }
    println!("== demarrage ==");
    let t = Instant::now();
    let mut g = Game::new(Arc::clone(&data), DEFAULT_SEED);
    println!("Game::new           {:>9.1} ms", ms(t));
    let t = Instant::now();
    g.new_game(DEFAULT_SEED);
    println!("new_game (systeme)  {:>9.1} ms", ms(t));
    let t = Instant::now();
    let i = g
        .sim
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Planet(p) if !p.is_giant()))
        .unwrap_or(1);
    let _ = g.sim.planet_map(i);
    println!("planet_map          {:>9.1} ms", ms(t));
    let t = Instant::now();
    let _ = g.flight_map(i);
    println!("flight_map (1ere)   {:>9.1} ms", ms(t));
    println!("== simulation ==");
    let steps = 20_000;
    let t = Instant::now();
    let mut worst = (0.0f64, 0usize);
    let mut spikes = 0;
    for k in 0..steps {
        let s = Instant::now();
        g.sim.step(&ShipInput::default());
        let d = s.elapsed().as_secs_f64() * 1000.0;
        if d > 5.0 {
            spikes += 1;
        }
        if d > worst.0 {
            worst = (d, k);
        }
    }
    println!(
        "step moyen {:.4} ms (pire {:.2} ms au pas {}, {spikes} pas > 5 ms) sur {steps}",
        ms(t) / steps as f64,
        worst.0,
        worst.1
    );
    let t = Instant::now();
    let bytes = g.sim.save_bytes().map(|b| b.len()).unwrap_or(0);
    println!("save {bytes} octets  {:>9.1} ms", ms(t));
    if let Ok(b) = g.sim.save_bytes() {
        let t = Instant::now();
        let loaded = vector_sim::Sim::load_bytes(&b, Arc::clone(&data));
        println!("load {:>9.1} ms ({})", ms(t), loaded.is_ok());
    }
    println!("== ecrans (1280x720, {n} images) ==");
    let mut rows: Vec<(f64, f64, &str)> = Vec::new();
    let only: Vec<&str> = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .map_or(Vec::new(), |l| l.split(',').collect());
    for &s in SCENES {
        if !only.is_empty() && !only.contains(&s) {
            continue;
        }
        // Each screen in its own thread: one that takes too long to build (some scenes play days
        // of game time) is given up on, the others go on.
        let (tx, rx) = std::sync::mpsc::channel();
        let d = Arc::clone(&data);
        let _ = std::thread::Builder::new()
            .stack_size(32 << 20)
            .spawn(move || {
                let t = Instant::now();
                let Some(g) = quiet(|| crate::scene_game(s, d)) else {
                    let _ = tx.send(None);
                    return;
                };
                let build = ms(t);
                let mut fb = Framebuffer::new(1280, 720);
                crate::perf::take();
                let (mean, med, cpu, hitch) = time_frames(&mut fb, &g, n);
                let _ = tx.send(Some((mean, med, cpu, hitch, build, stages(n + 1))));
            });
        match rx.recv_timeout(std::time::Duration::from_secs(40)) {
            Ok(Some((mean, med, cpu, hitch, build, st))) => {
                println!(
                    "{s:<16} min {mean:>7.2} med {med:>7.2} cpu {cpu:>7.1} pic {hitch:>6.0} (scene {build:>5.0}) {st}{}",
                    if mean > BUDGET_MS { "  <-- lent" } else { "" }
                );
                rows.push((mean, med, s));
            }
            Ok(None) => println!("{s:<16} (scene indisponible)"),
            Err(_) => println!("{s:<16} (scene trop longue a construire: abandon)"),
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("== les plus lents ==");
    for (m, w, s) in rows.iter().take(8) {
        println!("{s:<16} min {m:>8.2} ms (mediane ou 1ere {w:.2})");
    }
    crate::perf::set(false);
    // Abandoned scenes may still be running: leave without waiting for them.
    std::process::exit(0);
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Cost of the pieces of the march over the same 200 rays, in ns per step (relative figures).
pub fn terrain_micro(data: Arc<GameData>) {
    use crate::terrain::{SampleCache, Terrain};
    let mut g = Game::new(data, DEFAULT_SEED);
    g.new_game(DEFAULT_SEED);
    let Some(i) = (0..g.sim.bodies.len()).find(|&i| g.sim.landable(i)) else {
        return;
    };
    let map = g.sim.planet_map(i).expect("map");
    let t = Terrain::new(&map, g.sim.bodies[i].radius);
    let focal = 525.0;
    let rays = 200;
    let mut steps = 0usize;
    let (mut t_sample, mut t_inland) = (0.0f64, 0.0f64);
    let mut sink = 0.0f64;
    for r in 0..rays {
        let yaw = f64::from(r) * 0.0314;
        let (de, dn) = (yaw.sin(), yaw.cos());
        let mut cache = SampleCache::default();
        let mut d = 1.0f64;
        while d < 300_000.0 {
            let lat = 0.3 + dn * d / t.radius;
            let lon = 1.1 + de * d / (t.radius * 0.95);
            let lod = d / focal * 2.0;
            let a = Instant::now();
            let (h, e) = t.sample_with(lat, lon, lod, &mut cache);
            t_sample += a.elapsed().as_secs_f64();
            let b = Instant::now();
            let w = t.inland_water(lat, lon, lod);
            t_inland += b.elapsed().as_secs_f64();
            sink += f64::from(h) + f64::from(e) + w.map_or(0.0, |w| f64::from(w.level.max(0.0)));
            d += d * if d < 8000.0 { 0.03 } else { 0.045 } + 0.5;
            steps += 1;
        }
    }
    // The same points with no octave at all (the map alone), and with a miss every time.
    let mut t_map = 0.0f64;
    let mut cache = SampleCache::default();
    for r in 0..rays {
        let (de, dn) = (f64::from(r).sin() * 0.3, 1.0);
        let mut d = 1.0f64;
        while d < 300_000.0 {
            let lat = 0.3 + dn * d / t.radius;
            let lon = 1.1 + de * d / (t.radius * 0.95);
            let a = Instant::now();
            let (h, _) = t.sample_with(lat, lon, 1.0e12, &mut cache);
            t_map += a.elapsed().as_secs_f64();
            sink += f64::from(h);
            d += d * if d < 8000.0 { 0.03 } else { 0.045 } + 0.5;
        }
    }
    println!(
        "  carte seule (sans octave): {:.0} ns",
        t_map / steps as f64 * 1e9
    );
    println!(
        "march: {steps} pas ({} par rayon), sample {:.0} ns, inland_water {:.0} ns (somme {sink:.0})",
        steps / rays as usize,
        t_sample / steps as f64 * 1e9,
        t_inland / steps as f64 * 1e9
    );
}
