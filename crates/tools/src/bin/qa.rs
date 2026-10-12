//! `vector-qa`: a test robot that plays long simulated games with several policies and checks
//! invariants after every step (finite numbers, bounds, save round-trip, determinism, panics,
//! hangs), and measures speed. Usage:
//!   qa [--days N] [--seeds 1,2,3] [--scenario id] [--policy name] [--no-det] [--timeout SECS]
//! Policies: idle, random, trader, explorer, chaos. Exit code 1 if any finding.
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use vector_data::{find_data_dir, GameData};
use vector_sim::casino::Game;
use vector_sim::crew::DAY;
use vector_sim::start::{new_sim_with, StartConfig};
use vector_sim::{Sim, ShipInput};

static CURRENT: Mutex<String> = Mutex::new(String::new());
static LAST_PANIC: Mutex<String> = Mutex::new(String::new());

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn sym(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn fin(v: f64) -> bool {
    v.is_finite()
}

/// Every broken invariant of the current state, as short messages.
fn invariants(s: &Sim) -> Vec<String> {
    let mut o = Vec::new();
    let mut bad = |c: bool, m: &str| {
        if !c {
            o.push(m.to_string());
        }
    };
    bad(fin(s.time), "time non fini");
    bad(fin(s.credits), "credits non finis");
    bad(
        fin(s.ship.pos.length()) && fin(s.ship.vel.length()) && fin(s.ship.ang.length()),
        "physique du vaisseau non finie",
    );
    bad(
        s.ship.rot.0.iter().flatten().all(|x| fin(*x)),
        "rotation non finie",
    );
    bad(fin(s.ship.fuel) && s.ship.fuel >= -1e-6, "carburant NaN ou negatif");
    bad(
        s.ship.fuel <= s.ship.stats.fuel_cap * 1.001 + 1e-6,
        "carburant au dessus du reservoir",
    );
    bad(fin(s.ship.hull) && s.ship.hull >= -1e-6, "coque NaN ou negative");
    bad(
        s.ship.hull <= s.ship.stats.hull_max * 1.001 + 1e-6,
        "coque au dessus du max",
    );
    bad(fin(s.ship.sys.battery_mj) && s.ship.sys.battery_mj >= -1e-6, "batterie invalide");
    bad(
        fin(s.ship.sys.temp_k) && s.ship.sys.temp_k >= 0.0,
        "temperature invalide",
    );
    bad(fin(s.ship.sys.dose_gy) && s.ship.sys.dose_gy >= 0.0, "dose invalide");
    bad(
        s.ship.sys.health.iter().all(|h| fin(*h) && *h >= -1e-9),
        "sante de module invalide",
    );
    let c = &s.ship.cargo;
    bad(c.kg.iter().all(|k| fin(*k) && *k >= -1e-6), "soute: masse negative/NaN");
    bad(fin(c.mass) && fin(c.volume), "soute: total non fini");
    bad(
        c.cap_mass <= 0.0 || c.mass <= c.cap_mass * 1.001 + 1e-3,
        "soute au dessus de la capacite (masse)",
    );
    let sum: f64 = c.kg.iter().sum();
    bad(
        (sum - c.mass).abs() <= 1e-3 + 1e-6 * sum.abs(),
        "soute: total desynchronise de la somme",
    );
    bad(
        fin(s.bank.loan) && s.bank.loan >= -1e-6 && fin(s.bank.dirty) && s.bank.dirty >= -1e-6,
        "banque invalide",
    );
    for (st, m) in &s.markets {
        let ok = (0..m.stock.len()).all(|r| {
            let p = m.price(&s.data, r);
            fin(m.stock[r]) && m.stock[r] >= -1e-6 && fin(p) && p > 0.0
        });
        if !ok {
            o.push(format!("marche {st}: stock/prix invalide"));
        }
    }
    for (i, p) in s.crew.members.iter().enumerate() {
        let vals = [
            p.health, p.fatigue, p.hunger, p.thirst, p.social, p.stress, p.morale, p.loyalty,
            p.age,
        ];
        if !vals.iter().all(|v| v.is_finite()) {
            o.push(format!("equipage {i}: valeur non finie"));
        }
        if !p.organs.iter().all(|v| v.is_finite()) {
            o.push(format!("equipage {i}: organes non finis"));
        }
    }
    for c in &s.colonies {
        let ok = fin(c.pop)
            && c.pop >= -1e-9
            && fin(c.happiness)
            && fin(c.unrest)
            && fin(c.treasury)
            && fin(c.terraform)
            && c.storage.iter().all(|v| fin(*v) && *v >= -1e-6);
        if !ok {
            o.push(format!("colonie {}: pop/stock/tresor invalide", c.name));
        }
    }
    for (i, f) in s.fleet.iter().enumerate() {
        let _ = (i, f);
    }
    o
}

/// Resource indices by rough category used by the bots.
fn pick_res(s: &Sim, r: &mut Rng) -> usize {
    r.below(s.data.resources.len())
}

type Act = fn(&mut Sim, &mut Rng);

fn nstars(s: &Sim) -> Vec<vector_world::galaxy::StarEntry> {
    s.galaxy.nearby(s.here_pos, 40.0)
}

fn actions() -> Vec<(&'static str, Act)> {
    vec![
        ("dock", |s, _| {
            let _ = s.try_dock();
        }),
        ("undock", |s, _| s.undock()),
        ("buy", |s, r| {
            let x = pick_res(s, r);
            let _ = s.buy(x, r.unit() * 500.0);
        }),
        ("sell", |s, r| {
            let x = pick_res(s, r);
            let _ = s.sell(x, if r.below(2) == 0 { None } else { Some(r.unit() * 500.0) });
        }),
        ("refuel", |s, _| {
            let _ = s.refuel();
        }),
        ("repair", |s, _| {
            let _ = s.repair();
        }),
        ("hire", |s, r| {
            let k = r.below(6);
            let role = r.below(s.data.roles.len());
            let _ = s.hire(k, role);
        }),
        ("dismiss", |s, r| {
            let i = r.below(s.crew.members.len() + 1);
            let _ = s.dismiss(i);
        }),
        ("contract", |s, r| {
            let o = s.offered_contracts();
            if !o.is_empty() {
                let c = o[r.below(o.len())].clone();
                let _ = s.accept_contract(c);
            }
            let n = s.contracts.len();
            let _ = s.deliver_contract(r.below(n + 1));
        }),
        ("loan", |s, r| {
            if r.below(2) == 0 {
                let _ = s.borrow(r.unit() * 20000.0);
            } else {
                let _ = s.repay(r.unit() * 20000.0);
            }
        }),
        ("currency", |s, r| {
            let _ = s.buy_currency(r.unit() * 3000.0);
            let _ = s.sell_currency();
            let _ = s.launder(r.unit() * 3000.0);
        }),
        ("securities", |s, r| {
            let _ = s.buy_bond(r.unit() * 5000.0);
            let _ = s.sell_bond(r.below(3));
            let _ = s.buy_fund(r.below(3), r.unit() * 5000.0);
            let _ = s.sell_fund(r.below(3));
        }),
        ("shares", |s, r| {
            let c = r.below(s.data.companies.len().max(1));
            if r.below(2) == 0 {
                let _ = s.buy_shares(c, r.unit() * 20.0);
            } else {
                let _ = s.sell_shares(c, r.unit() * 20.0);
            }
        }),
        ("casino", |s, r| {
            let g = match r.below(4) {
                0 => Game::Red,
                1 => Game::Black,
                2 => Game::Number(r.below(37) as u32),
                _ => Game::Machine,
            };
            let _ = s.gamble(g, r.unit() * 500.0);
        }),
        ("black", |s, r| {
            let x = pick_res(s, r);
            let _ = s.black_sell(x, r.unit() * 100.0);
            let _ = s.black_buy(x, r.unit() * 100.0);
        }),
        ("flea", |s, r| {
            let x = pick_res(s, r);
            let _ = s.flea_buy(r.below(6));
            let _ = s.flea_sell(x, r.unit() * 50.0);
            let _ = s.place_bid(r.below(4), r.unit() * 5000.0);
        }),
        ("insurance", |s, r| {
            let _ = s.buy_insurance(r.below(2) == 0);
            let _ = s.join_cartel(r.below(3));
            let _ = s.bid_tender(r.next() % 8, r.unit() * 1000.0);
        }),
        ("law", |s, r| {
            let f = r.below(s.data.factions.len().max(1));
            let _ = s.offend(Some(f), "smuggling");
            let _ = s.pay_fines(f);
            let _ = s.bribe_officials(f);
            let _ = s.hire_lawyer(f);
        }),
        ("colony", |s, r| {
            let n = s.bodies.len();
            let _ = s.found_colony(r.below(n), None);
            let nc = s.colonies.len();
            if nc > 0 {
                let ci = r.below(nc);
                let _ = s.colony_build(ci, r.below(s.data.buildings.len().max(1)));
                let _ = s.colony_demolish(ci, r.below(8));
                let _ = s.colony_withdraw_treasury(ci);
                let _ = s.colony_set_process(ci, r.below(4));
                let _ = s.colony_project_toggle(ci, r.below(s.data.projects.len().max(1)));
            }
        }),
        ("fleet", |s, r| {
            let _ = s.buy_fleet_ship(r.below(s.data.hulls.len().max(1)));
            let nf = s.fleet.len();
            let _ = s.assign_route(r.below(nf + 1), r.below(s.routes.len() + 1));
            if r.below(4) == 0 {
                let _ = s.sell_fleet_ship(r.below(nf + 1));
            }
        }),
        ("yard", |s, r| {
            let b = s.new_blueprint("QA");
            let _ = s.order_ship(b.min(s.blueprints.len().saturating_sub(1)), 1);
            let _ = s.buy_plan(r.below(s.data.ship_plans.len().max(1)));
            if r.below(3) == 0 {
                let nb = s.blueprints.len();
                s.delete_blueprint(r.below(nb + 1));
            }
            let nf = s.fleet.len();
            let _ = s.board_fleet_ship(r.below(nf + 1));
        }),
        ("terraform", |s, r| {
            let _ = s.orbit_here();
            let nc = s.colonies.len();
            if nc > 0 {
                let ci = r.below(nc);
                let _ = s.colony_set_process(ci, r.below(4));
            }
        }),
        ("medical", |s, r| {
            let n = s.crew.members.len() + 1;
            let m = r.below(n);
            let _ = s.first_aid(m);
            let _ = s.give_drug(m, r.below(s.data.drugs.len().max(1)));
            let _ = s.operate_wound(m, r.below(3));
            let _ = s.defibrillate(m);
            let _ = s.cryo_start(m);
            let _ = s.cryo_wake(m);
            if r.below(3) == 0 {
                s.wound_member(m, "blast", r.unit() as f32, r.next());
            }
        }),
        ("surgery", |s, r| {
            let m = r.below(s.crew.members.len() + 1);
            let _ = s.grow_clone(r.below(s.data.organs.len().max(1)));
            let _ = s.install_cyber(m, r.below(s.data.cybernetics.len().max(1)));
            let _ = s.remove_cyber(m, r.below(4));
            let _ = s.inject_mutagen(m);
        }),
        ("society", |s, r| {
            let id = s.crew.members.get(r.below(s.crew.members.len().max(1))).map_or(0, |p| p.id);
            let _ = s.enroll(id, r.below(6));
            let _ = s.compose(id, r.below(4));
            let _ = s.bet(r.below(3), r.below(6), r.unit() * 300.0);
            let _ = s.sell_work(r.below(4));
            let _ = s.donate_work(r.below(4));
        }),
        ("exotic", |s, r| {
            let _ = s.clone_crew(r.below(s.crew.members.len() + 1));
            let _ = s.use_portal();
            let _ = s.set_anchor();
            let _ = s.rewind(r.below(3));
            let _ = s.stasis(r.unit() * 30.0);
            let _ = s.break_loop();
            let _ = s.build_outpost();
        }),
        ("science", |s, r| {
            let _ = s.start_research(r.below(s.data.techs.len().max(1)));
            s.irradiate(r.unit() * 2.0);
        }),
        ("surprise", |s, r| {
            let _ = s.surprise_choose(r.below(3), r.below(3));
        }),
        ("mind", |s, r| {
            s.mind_shift(r.below(4), r.below(2) == 0);
        }),
        ("rogue", |s, r| {
            if s.rogue.is_none() {
                let party: Vec<usize> = (0..s.crew.members.len().min(3)).collect();
                s.rogue_begin(r.below(s.data.rogue_themes.len().max(1)), r.next(), party);
            }
            for _ in 0..30 {
                let _ = s.rogue_step(r.below(3) as i32 - 1, r.below(3) as i32 - 1);
                match r.below(12) {
                    0 => {
                        let _ = s.rogue_wait();
                    }
                    1 => {
                        let _ = s.rogue_stairs();
                    }
                    2 => {
                        let _ = s.rogue_capture();
                    }
                    3 => s.rogue_switch(),
                    4 => {
                        let _ = s.rogue_deploy_drone();
                    }
                    5 => {
                        let _ = s.rogue_dig(r.below(3) as i32 - 1, r.below(3) as i32 - 1);
                    }
                    6 => {
                        let _ = s.rogue_travel(r.below(20) as i32, r.below(20) as i32, 40);
                    }
                    _ => {}
                }
            }
            if r.below(3) == 0 {
                s.rogue_close();
            }
        }),
        ("jump", |s, r| {
            let st = nstars(s);
            if !st.is_empty() {
                let t = st[r.below(st.len())].clone();
                let _ = s.start_jump(&t);
            }
        }),
        ("autopilot", |s, r| {
            let n = s.bodies.len();
            let _ = s.engage_autopilot(r.below(n));
        }),
        ("archive", |s, r| {
            let _ = s.dig_site("ruine", r.next());
            let _ = s.date_artefact(r.below(4));
            let _ = s.dispose_artefact(r.below(4), vector_sim::archive::Dest::Museum);
            s.toggle_careful_dig();
        }),
        ("livestock", |s, r| {
            let _ = s.buy_animal(r.below(4));
            let _ = s.breed(r.below(4), r.below(4));
            let _ = s.harvest_animal(r.below(4));
            let _ = s.sell_animal(r.below(4));
        }),
        ("combat", |s, r| {
            let _ = s.pay_tribute();
            let _ = s.salvage();
            let _ = s.board();
            if r.below(5) == 0 {
                s.damage_player(r.unit() * 50.0);
            }
        }),
        ("save", |s, _| {
            let _ = s.save_bytes();
        }),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Policy {
    Idle,
    Pacifist,
    Random,
    Trader,
    Explorer,
    Chaos,
}

impl Policy {
    fn name(self) -> &'static str {
        match self {
            Policy::Idle => "idle",
            Policy::Pacifist => "pacifiste",
            Policy::Random => "random",
            Policy::Trader => "trader",
            Policy::Explorer => "explorer",
            Policy::Chaos => "chaos",
        }
    }
}

struct Finding {
    kind: String,
    detail: String,
    day: u64,
}

struct Outcome {
    findings: Vec<Finding>,
    days_done: u64,
    secs: f64,
    steps: u64,
    worst_day_ms: f64,
    worst_day: u64,
    hash: u64,
    summary: String,
    action_ms: BTreeMap<&'static str, (u32, f64)>,
}

fn rand_input(r: &mut Rng) -> ShipInput {
    ShipInput {
        pitch: r.sym(),
        yaw: r.sym(),
        roll: r.sym(),
        fwd: r.unit(),
        lat: r.sym(),
        vert: r.sym(),
        mine: r.below(3) == 0,
        fire: r.below(6) == 0,
        fire_missile: r.below(10) == 0,
        boost: r.below(8) == 0,
        brake: r.below(4) == 0,
    }
}

fn play(
    data: &Arc<GameData>,
    seed: u64,
    scenario: usize,
    policy: Policy,
    days: u64,
    roundtrip: bool,
) -> Outcome {
    let mut out = Outcome {
        findings: Vec::new(),
        days_done: 0,
        secs: 0.0,
        steps: 0,
        worst_day_ms: 0.0,
        worst_day: 0,
        hash: 0,
        summary: String::new(),
        action_ms: BTreeMap::new(),
    };
    let mut cfg = StartConfig::new(seed);
    cfg.scenario = scenario;
    let mut s = match new_sim_with(&cfg, Arc::clone(data)) {
        Ok(s) => s,
        Err(e) => {
            out.findings.push(Finding {
                kind: "creation".into(),
                detail: e.to_string(),
                day: 0,
            });
            return out;
        }
    };
    let mut rng = Rng(seed ^ 0xA5A5 ^ (policy as u64) << 32);
    let table = actions();
    let t0 = Instant::now();
    let trace = std::env::var("QA_TRACE").is_ok();
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    let mut note = |out: &mut Outcome, kind: &str, detail: String, day: u64| {
        let key = format!("{kind}|{detail}");
        let n = seen.entry(key).or_insert(0);
        *n += 1;
        if *n == 1 {
            out.findings.push(Finding {
                kind: kind.to_string(),
                detail,
                day,
            });
        }
    };
    let pacify = |s: &mut Sim| {
        if s.docked.is_none() && s.try_dock().is_err() {
            if let Some(st) = s.bodies.iter().position(|b| b.kind == vector_world::BodyKind::Station) {
                if s.autopilot.is_none() {
                    let _ = s.engage_autopilot(st);
                }
            }
        }
    };
    let mut last_action = "";
    let mut crew_dead = false;
    let mut prev_ghosts: Vec<u32> = Vec::new();
    let mut min_alive = usize::MAX;
    for day in 0..days {
        if s.steps > 1_200_000 {
            break; // step budget: free flight near worlds is expensive, the day count is reported
        }
        let td = Instant::now();
        if policy == Policy::Pacifist {
            // Dock as fast as a player would: poll every 5 minutes until docked (at most 6 h).
            for poll in 0..72 {
                pacify(&mut s);
                if s.docked.is_some() {
                    if std::env::var("QA_CREW").is_ok() {
                        eprintln!("  amarre apres {poll} sondages de 5 min");
                    }
                    break;
                }
                s.advance(300.0, &ShipInput::default());
            }
        }
        // Actions of the day.
        let n_actions = match policy {
            Policy::Idle | Policy::Pacifist => 0,
            Policy::Random => 2,
            Policy::Trader => 3,
            Policy::Explorer => 2,
            Policy::Chaos => 6,
        };
        for _ in 0..n_actions {
            let (name, f): (&'static str, Act) = match policy {
                Policy::Trader => {
                    const T: [&str; 9] = [
                        "dock", "sell", "buy", "refuel", "repair", "contract", "autopilot",
                        "loan", "undock",
                    ];
                    let n = T[rng.below(T.len())];
                    table.iter().find(|(a, _)| *a == n).copied().unwrap()
                }
                Policy::Explorer => {
                    const T: [&str; 6] = ["jump", "autopilot", "rogue", "archive", "refuel", "dock"];
                    let n = T[rng.below(T.len())];
                    table.iter().find(|(a, _)| *a == n).copied().unwrap()
                }
                Policy::Random => {
                    const T: [&str; 8] = [
                        "dock", "undock", "buy", "sell", "refuel", "repair", "hire", "autopilot",
                    ];
                    let n = T[rng.below(T.len())];
                    table.iter().find(|(a, _)| *a == n).copied().unwrap()
                }
                _ => table[rng.below(table.len())],
            };
            if let Ok(mut g) = CURRENT.lock() {
                *g = format!("jour {day} action {name} (seed {seed})");
            }
            if trace {
                eprintln!("  act jour {day} {name}");
            }
            let ta = Instant::now();
            let r = catch_unwind(AssertUnwindSafe(|| f(&mut s, &mut rng)));
            let ms = ta.elapsed().as_secs_f64() * 1000.0;
            let e = out.action_ms.entry(name).or_insert((0, 0.0));
            e.0 += 1;
            e.1 = e.1.max(ms);
            last_action = name;
            if r.is_err() {
                let msg = LAST_PANIC.lock().map(|g| g.clone()).unwrap_or_default();
                note(&mut out, "panique(action)", format!("{name}: {msg}"), day);
            }
        }
        // Time passes: a day in a few chunks, with inputs when the ship is free.
        if let Ok(mut g) = CURRENT.lock() {
            *g = format!("jour {day} advance");
        }
        let mut left = DAY;
        let mut input = ShipInput::default();
        let mut k = 0;
        while left > 1e-9 {
            if k % 6 == 0 && !matches!(policy, Policy::Idle | Policy::Pacifist | Policy::Trader) {
                input = rand_input(&mut rng);
            }
            k += 1;
            let chunk = left.min(3600.0);
            if trace {
                eprintln!(
                    "  chunk jour {day} k {k} steps {} docked {:?} auto {} jump {} transit {} rogue {} here {:?} t {:.0}",
                    s.steps,
                    s.docked,
                    s.autopilot.is_some(),
                    s.jump.is_some(),
                    s.in_transit(),
                    s.rogue.is_some(),
                    s.here,
                    s.time
                );
            }
            let mut capped = false;
            let r = catch_unwind(AssertUnwindSafe(|| {
                let mut left = chunk;
                let mut g = 0;
                while left > 1e-9 {
                    if g >= 30_000 {
                        capped = true;
                        break;
                    }
                    g += 1;
                    let dt = if s.docked.is_some() {
                        left.min(3600.0)
                    } else {
                        left.min(s.safe_dt())
                    };
                    s.step_dt(&input, dt);
                    left -= dt;
                }
            }));
            if r.is_err() {
                let msg = LAST_PANIC.lock().map(|g| g.clone()).unwrap_or_default();
                note(&mut out, "panique(advance)", msg, day);
                break;
            }
            left -= chunk;
            if capped {
                let near = s
                    .bodies
                    .iter()
                    .map(|b| ((s.ship.pos - b.pos).length() - b.radius, b.name.clone()))
                    .fold((f64::MAX, String::new()), |a, b| if b.0 < a.0 { b } else { a });
                note(
                    &mut out,
                    "lenteur",
                    format!(
                        "pas plafonnes: 30000 pas par heure simulee sans finir (safe_dt {:.4} s, vitesse {:.0} m/s, corps le plus proche {} a {:.0} m, rayon {:.0}, autopilote {}, derniere action {})",
                        s.safe_dt(),
                        s.ship.vel.length(),
                        near.1,
                        near.0,
                        s.bodies.iter().find(|b| b.name == near.1).map_or(0.0, |b| b.radius),
                        s.autopilot.is_some(),
                        last_action
                    ),
                    day,
                );
                s.autopilot = None;
                if let Some(st) = s.bodies.iter().position(|b| b.kind == vector_world::BodyKind::Station) {
                    s.spawn_near(st, s.bodies[st].radius + 500.0);
                }
                break;
            }
        }
        let ms = td.elapsed().as_secs_f64() * 1000.0;
        if ms > out.worst_day_ms {
            out.worst_day_ms = ms;
            out.worst_day = day;
        }
        // A "ghost": alive with no health at two day ends in a row (a single one is just the
        // crew tick that has not yet counted the death).
        let ghosts: Vec<u32> = s
            .crew
            .members
            .iter()
            .filter(|p| p.alive && p.health <= 0.0)
            .map(|p| p.id)
            .collect();
        if ghosts.iter().any(|g| prev_ghosts.contains(g)) {
            note(
                &mut out,
                "invariant",
                "membre d'equipage vivant avec sante 0 deux jours de suite (fantome)".into(),
                day,
            );
        }
        prev_ghosts = ghosts;
        for m in invariants(&s) {
            note(&mut out, "invariant", m, day);
        }
        if roundtrip && day % 40 == 39 {
            match s.save_bytes() {
                Err(e) => note(&mut out, "sauvegarde", format!("save: {e:?}"), day),
                Ok(b1) => match Sim::load_bytes(&b1, Arc::clone(data)) {
                    Err(e) => note(&mut out, "sauvegarde", format!("reload: {e:?}"), day),
                    Ok(l) => match l.save_bytes() {
                        Ok(b2) if b1 == b2 => {}
                        Ok(b2) => {
                            let at = b1.iter().zip(&b2).position(|(a, b)| a != b);
                            note(
                                &mut out,
                                "sauvegarde",
                                format!(
                                    "aller-retour different (len {} vs {}, octet {:?}, champs {:?})",
                                    b1.len(),
                                    b2.len(),
                                    at,
                                    vector_sim::save::diff_fields(&b1, &b2)
                                ),
                                day,
                            );
                        }
                        Err(e) => note(&mut out, "sauvegarde", format!("resave: {e:?}"), day),
                    },
                },
            }
        }
        if std::env::var("QA_CREW").is_ok() {
            for p in &s.crew.members {
                eprintln!(
                    "  crew j{day} {} alive {} sante {:.2} faim {:.2} soif {:.2} fatigue {:.2} stress {:.2} moral {:.2} maladie {:?} air {:.2} dose {:.3} t {:.0}K",
                    p.name, p.alive, p.health, p.hunger, p.thirst, p.fatigue, p.stress, p.morale, p.disease, s.crew.air, s.ship.sys.dose_gy, s.ship.sys.temp_k
                );
            }
        }
        if std::env::var("QA_CREW").is_ok() {
            eprintln!(
                "  sys j{day} log {:?} | surchauffe {} sante_min {:.2} offre {:.1} demande {:.1} batterie {:.1} life_ok {} solaire {:.1}kW docked {:?}",
                s.crew.log.iter().rev().take(3).map(|l| l.text.clone()).collect::<Vec<_>>(),
                s.ship.sys.overheating,
                s.ship.sys.health.iter().cloned().fold(9.0, f64::min),
                s.ship.sys.supply_kw,
                s.ship.sys.demand_kw,
                s.ship.sys.battery_mj,
                s.effective.life_ok,
                s.solar_heat_kw(),
                s.docked
            );
        }
        let alive = s.crew.members.iter().filter(|p| p.alive).count();
        if alive == 0 && !s.crew.members.is_empty() && !crew_dead && day < 40 {
            crew_dead = true;
            note(
                &mut out,
                "equilibrage",
                format!("equipage entierement mort au jour {day} ({} morts)", s.crew.members.len()),
                day,
            );
        }
        if alive < min_alive {
            min_alive = alive;
        }
        out.days_done = day + 1;
        if s.destroyed && out.findings.iter().all(|f| f.kind != "mort") {
            // The player died: the world must still run (insurance respawn etc.). Just note it.
            let _ = &mut note;
        }
    }
    out.secs = t0.elapsed().as_secs_f64();
    out.steps = s.steps;
    out.summary = format!(
        "credits {:.0}, carburant {:.0}/{:.0}, coque {:.0}, equipage vivant {}/{} (min {}), colonies {}, flotte {}, detruit {}",
        s.credits,
        s.ship.fuel,
        s.ship.stats.fuel_cap,
        s.ship.hull,
        s.crew.members.iter().filter(|p| p.alive).count(),
        s.crew.members.len(),
        min_alive,
        s.colonies.len(),
        s.fleet.len(),
        s.destroyed
    );
    out.hash = s.save_bytes().map(|b| fnv(&b)).unwrap_or(0);
    out
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let msg = if let Some(m) = info.payload().downcast_ref::<&str>() {
            (*m).to_string()
        } else if let Some(m) = info.payload().downcast_ref::<String>() {
            m.clone()
        } else {
            String::new()
        };
        if let Ok(mut g) = LAST_PANIC.lock() {
            *g = format!("{msg} @ {loc}");
        }
    }));
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let days: u64 = get("--days").and_then(|v| v.parse().ok()).unwrap_or(200);
    let seeds: Vec<u64> = get("--seeds")
        .map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![1, 2, 3]);
    let only_sc = get("--scenario");
    let only_pol = get("--policy");
    if args.iter().any(|a| a == "--probe") {
        let dir = find_data_dir().expect("data/ not found");
        let data = Arc::new(GameData::load(&dir).expect("data"));
        for (si, sc) in data.scenarios.iter().enumerate() {
            for seed in 1..=8u64 {
                let mut cfg = StartConfig::new(seed);
                cfg.scenario = si;
                if let Ok(s) = new_sim_with(&cfg, Arc::clone(&data)) {
                    let d = s.natural_dose_at(s.ship.pos);
                    println!(
                        "{}/seed{seed}: dose a bord {:.4} Gy/h ({:.1} jours pour 6 Gy), cosmique {:.4} stellaire {:.4} ceinture {:.4}, amarre {:?}",
                        sc.id,
                        s.dose_rate_aboard(),
                        6.0 / (s.dose_rate_aboard() * 24.0).max(1e-9),
                        d.cosmic,
                        d.stellar,
                        d.belt,
                        s.docked
                    );
                }
            }
        }
        return;
    }
    let timeout: u64 = get("--timeout").and_then(|v| v.parse().ok()).unwrap_or(600);
    let det = !args.iter().any(|a| a == "--no-det");
    let dir = find_data_dir().expect("data/ not found");
    let data = Arc::new(GameData::load(&dir).expect("data"));
    let policies = [
        Policy::Idle,
        Policy::Pacifist,
        Policy::Random,
        Policy::Trader,
        Policy::Explorer,
        Policy::Chaos,
    ];
    let mut total = 0usize;
    let mut perf: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut act_worst: BTreeMap<&'static str, f64> = BTreeMap::new();
    for (si, sc) in data.scenarios.iter().enumerate() {
        if only_sc.as_ref().is_some_and(|o| *o != sc.id) {
            continue;
        }
        for &seed in &seeds {
            for &pol in &policies {
                if only_pol.as_ref().is_some_and(|o| o != pol.name()) {
                    continue;
                }
                let label = format!("{}/seed{}/{}", sc.id, seed, pol.name());
                let runs = if det { 2 } else { 1 };
                let mut outs = Vec::new();
                for k in 0..runs {
                    let (tx, rx) = mpsc::channel();
                    let d = Arc::clone(&data);
                    // The second run (determinism) is shorter-lived: same actions, no round-trip.
                    std::thread::Builder::new()
                        .stack_size(64 << 20)
                        .spawn(move || {
                            let _ = tx.send(play(&d, seed, si, pol, days, k == 0));
                        })
                        .unwrap();
                    match rx.recv_timeout(Duration::from_secs(timeout)) {
                        Ok(o) => outs.push(o),
                        Err(_) => {
                            println!("BLOCAGE {label}: pas fini apres {timeout} s ({})", CURRENT.lock().map(|g| g.clone()).unwrap_or_default());
                            std::process::exit(2);
                        }
                    }
                }
                let o = &outs[0];
                let mut line = format!(
                    "{label}: {} j, {:.1} s ({:.1} ms/j, pire {:.0} ms au jour {}), {} pas",
                    o.days_done,
                    o.secs,
                    o.secs * 1000.0 / o.days_done.max(1) as f64,
                    o.worst_day_ms,
                    o.worst_day,
                    o.steps
                );
                if outs.len() == 2 && outs[0].hash != outs[1].hash {
                    line.push_str("  NON DETERMINISTE");
                    total += 1;
                }
                println!("{line}");
                println!("    fin: {}", o.summary);
                for f in &o.findings {
                    println!("  [{}] jour {}: {}", f.kind, f.day, f.detail);
                    total += 1;
                }
                perf.push((
                    label,
                    o.secs * 1000.0 / o.days_done.max(1) as f64,
                    o.worst_day_ms,
                    o.secs * 1e6 / o.steps.max(1) as f64,
                ));
                for (k, v) in &o.action_ms {
                    let e = act_worst.entry(k).or_insert(0.0);
                    *e = e.max(v.1);
                }
            }
        }
    }
    perf.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("--- performance: 5 pires (ms/jour moyen, pire jour, us/pas)");
    for p in perf.iter().take(5) {
        println!("  {}: {:.1} ms/j, pire jour {:.0} ms, {:.1} us/pas", p.0, p.1, p.2, p.3);
    }
    let mut aw: Vec<_> = act_worst.into_iter().collect();
    aw.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("--- actions les plus lentes (ms, pire cas)");
    for (k, v) in aw.iter().take(6) {
        println!("  {k}: {v:.1} ms");
    }
    println!("--- {total} constat(s)");
    if total > 0 {
        std::process::exit(1);
    }
}
