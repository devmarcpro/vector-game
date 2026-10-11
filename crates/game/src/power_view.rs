//! Power and heat flow of the ship, drawn under the schematic of the ship screen: producers on
//! the left feed a bus (with the battery), the bus feeds the consumers, and everything ends up as
//! heat that the radiators must get rid of. Line thickness follows the power; red = switched off
//! or overheating.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use vector_render::Framebuffer;
use vector_sim::design::REACTOR_WASTE;

const BOX_W: i32 = 150;
const BOX_H: i32 = 18;

fn thick(fb: &mut Framebuffer, a: (f32, f32), b: (f32, f32), share: f64, c: u32) {
    let (x0, y0, x1, y1) = (a.0, a.1, b.0, b.1);
    // 1 to 3 parallel lines depending on the share of the total power.
    let n = 1 + (share * 3.0).floor().clamp(0.0, 2.0) as i32;
    for k in 0..n {
        fb.line(x0, y0 + k as f32, x1, y1 + k as f32, c);
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game, x0: i32, y0: i32) {
    let ship = &g.sim.ship;
    let hull = &g.data.hulls[ship.design.hull];
    let mut sources: Vec<(String, f64)> = Vec::new();
    let mut loads: Vec<(String, f64, bool)> = Vec::new();
    for i in 0..hull.slots.len() {
        let Some(m) = ship.design.slots[i] else {
            continue;
        };
        let m = &g.data.modules[m];
        if m.power > 0.0 {
            sources.push((m.name.clone(), m.power * ship.sys.health[i]));
        } else if m.power < 0.0 {
            loads.push((m.name.clone(), -m.power, ship.sys.powered[i]));
        }
    }
    fb.text_at(x0, y0 - 12, "FLUX D'ENERGIE ET DE CHALEUR", DIM);
    let total = sources
        .iter()
        .map(|s| s.1)
        .sum::<f64>()
        .max(loads.iter().map(|l| l.1).sum())
        .max(1.0);
    let bus_x = x0 + BOX_W + 50;
    let (lx, rx) = (x0, bus_x + 50);
    let rows = sources.len().max(loads.len()).max(1) as i32;
    let step = 24;
    let bus_h = rows * step;
    let overheat = ship.sys.overheating;
    // The bus and the battery.
    fb.line(
        bus_x as f32,
        y0 as f32,
        bus_x as f32,
        (y0 + bus_h) as f32,
        if overheat { RED } else { GREEN },
    );
    fb.text_at(
        bus_x - 36,
        y0 + bus_h + 4,
        &format!(
            "BATT {:.0}/{:.0} MJ",
            ship.sys.battery_mj, ship.stats.battery_cap
        ),
        AMBER,
    );
    let boxed = |fb: &mut Framebuffer, x: i32, y: i32, c: u32| {
        let (l, t, r, b) = (x as f32, y as f32, (x + BOX_W) as f32, (y + BOX_H) as f32);
        fb.line(l, t, r, t, c);
        fb.line(r, t, r, b, c);
        fb.line(r, b, l, b, c);
        fb.line(l, b, l, t, c);
    };
    for (k, (name, kw)) in sources.iter().enumerate() {
        let y = y0 + k as i32 * step;
        boxed(fb, lx, y, GREEN);
        let s = format!("{} {:.0}KW", name, kw);
        fb.text_at(
            lx + 4,
            y + 5,
            &s.chars().take(18).collect::<String>(),
            GREEN,
        );
        thick(
            fb,
            ((lx + BOX_W) as f32, (y + 9) as f32),
            (bus_x as f32, (y + 9) as f32),
            kw / total,
            GREEN,
        );
    }
    for (k, (name, kw, on)) in loads.iter().enumerate() {
        let y = y0 + k as i32 * step;
        let c = if *on { GREEN } else { RED };
        boxed(fb, rx, y, c);
        let s = format!("{} {:.0}KW", name, kw);
        fb.text_at(rx + 4, y + 5, &s.chars().take(18).collect::<String>(), c);
        thick(
            fb,
            (bus_x as f32, (y + 9) as f32),
            (rx as f32, (y + 9) as f32),
            kw / total,
            c,
        );
    }
    // Heat: everything ends up in the radiators.
    let sys = &ship.sys;
    let heat = sys.supply_kw.min(sys.demand_kw) * REACTOR_WASTE + sys.demand_kw;
    let radiators = ship.stats.radiate_kw + 30.0 + 0.004 * ship.stats.mass_dry;
    let hy = y0 + bus_h + 24;
    let c = if overheat { RED } else { GREEN };
    fb.text_at(
        x0,
        hy,
        &format!(
            "CHALEUR {:.1} MW  ->  RADIATEURS {:.1} MW   TEMPERATURE {:.0} K",
            heat / 1e3,
            radiators / 1e3,
            sys.temp_k
        ),
        c,
    );
    let frac = (heat / radiators.max(1e-9)).clamp(0.0, 1.5);
    let w = 300.0_f32;
    fb.line(
        x0 as f32,
        (hy + 14) as f32,
        x0 as f32 + w,
        (hy + 14) as f32,
        DIM,
    );
    fb.line(
        x0 as f32,
        (hy + 15) as f32,
        x0 as f32 + w * (frac / 1.5) as f32,
        (hy + 15) as f32,
        c,
    );
}
