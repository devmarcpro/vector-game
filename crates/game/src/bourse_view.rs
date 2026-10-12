//! Exchange screen (T at a station): the reference price of every resource with a sparkline,
//! a big chart of the selected one (price, 7-day average, the world events that touched it) and
//! the news of the period.
use crate::game::{Game, BOURSE_RANGES};
use crate::hud::{AMBER, DIM, DOWN, GREEN, RED, UP, WHITE};
use crate::mouse::Act;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

fn sparkline(series: &[f64], width: usize) -> String {
    const LEVELS: &[u8] = b" .:-=+*#";
    let tail = &series[series.len().saturating_sub(30)..];
    let (lo, hi) = tail
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    (0..width)
        .map(|i| {
            let v = tail[i * tail.len() / width];
            let k = if hi > lo {
                ((v - lo) / (hi - lo) * (LEVELS.len() - 1) as f64).round() as usize
            } else {
                3
            };
            LEVELS[k.min(LEVELS.len() - 1)] as char
        })
        .collect()
}

fn fmt_price(p: f64) -> String {
    if p >= 1000.0 {
        format!("{p:.0}")
    } else if p >= 10.0 {
        format!("{p:.1}")
    } else {
        format!("{p:.2}")
    }
}

/// Change over the last `back` samples, as a percentage.
fn change(series: &[f64], back: usize) -> f64 {
    let last = *series.last().unwrap_or(&1.0);
    let first = series[series.len().saturating_sub(back + 1)];
    (last / first.max(1e-9) - 1.0) * 100.0
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    if g.bourse_tab == 1 {
        draw_companies(fb, g);
        return;
    }
    let data = &g.data;
    if g.bourse_series.is_empty() {
        fb.text(2, 2, "BOURSE : PAS DE DONNEES", RED);
        return;
    }
    let sel = g.bourse_sel.min(data.resources.len() - 1);
    let days = BOURSE_RANGES[g.bourse_range];
    fb.text(2, 1, "== BOURSE ==", AMBER);
    fb.text(
        16,
        1,
        &format!("PRIX DE REFERENCE   CREDITS {:.0}", g.sim.credits),
        DIM,
    );
    // Period tabs.
    let mut tx = 62usize;
    for (i, &d) in BOURSE_RANGES.iter().enumerate() {
        let label = format!("[{d} J]");
        let hover = g.hit_cells(
            tx,
            1,
            label.len(),
            1,
            Act::Call(Game::click_bourse_range, i),
            None,
        );
        fb.text(tx, 1, &label, if d == days || hover { AMBER } else { DIM });
        tx += label.len() + 2;
    }
    // The people of this territory: their mœurs and what they forbid.
    let faction = g.sim.faction;
    let values = data
        .ethos
        .profile(faction.map(|f| data.factions[f].id.as_str()));
    let banned: Vec<bool> = (0..data.resources.len())
        .map(|r| {
            data.ethos.banned_by(&values, &data.resources, r).is_some()
                || faction
                    .is_some_and(|f| data.factions[f].contraband.contains(&data.resources[r].id))
        })
        .collect();
    let people = faction.map_or("ESPACE LIBRE".to_string(), |f| {
        data.factions[f].name.to_uppercase()
    });
    fb.text(
        2,
        2,
        &format!(
            "MOEURS : {people} - {}",
            data.ethos.traits(&values, 4).join(", ").to_uppercase()
        ),
        DIM,
    );
    let rows = fb.rows().saturating_sub(7);
    // Resource list with 7-day change and a sparkline.
    fb.text(2, 3, "RESSOURCE            PRIX   7J      30J", DIM);
    let start = sel
        .saturating_sub(rows / 2)
        .min(data.resources.len().saturating_sub(rows));
    for (k, r) in data.resources.iter().enumerate().skip(start).take(rows) {
        let series = &g.bourse_series[k];
        let (ch7, ch30) = (change(series, 7), change(series, 30));
        let selected = k == sel;
        let col = if banned[k] {
            RED
        } else if selected {
            AMBER
        } else {
            GREEN
        };
        let mut name = r.name.to_uppercase();
        name.truncate(18);
        let row = 4 + k - start;
        let hover = g.hit_cells(2, row, 44, 1, Act::Call(Game::click_bourse_res, k), None);
        let col = if hover && !selected { WHITE } else { col };
        fb.text(
            2,
            row,
            &format!(
                "{}{:<19}{:>7}",
                if selected {
                    ">"
                } else if banned[k] {
                    "!"
                } else {
                    " "
                },
                name,
                fmt_price(series[series.len() - 1])
            ),
            col,
        );
        fb.text(
            30,
            row,
            &format!("{ch7:+5.0}%"),
            if ch7 >= 0.0 { UP } else { DOWN },
        );
        fb.text(
            37,
            row,
            &sparkline(series, 8),
            if ch30 >= 0.0 { UP } else { DOWN },
        );
    }
    let x0 = 48usize;
    if fb.cols() < x0 + 30 {
        return;
    }
    if banned[sel] {
        fb.text(
            x0,
            2,
            "INTERDIT ICI : CONTREBANDE (SAISIE ET AMENDE A LA DOUANE)",
            RED,
        );
    }
    draw_chart(fb, g, sel, x0);
    draw_futures(fb, g, sel, x0);
    draw_news(fb, g, sel, x0);
}

/// The companies tab: the list with price, change and shares held; the selected one's chart,
/// what it sells and where it is from; clickable words to buy and sell.
fn draw_companies(fb: &mut Framebuffer, g: &Game) {
    let data = &g.data;
    fb.text(2, 1, "== BOURSE : ENTREPRISES ==", AMBER);
    fb.text(30, 1, &format!("CREDITS {:.0}", g.sim.credits), DIM);
    let hover = g.hit_cells(48, 1, 18, 1, Act::Key(KeyCode::KeyE), None);
    fb.text(48, 1, "[E] MARCHANDISES", if hover { AMBER } else { GREEN });
    fb.text(2, 3, "ENTREPRISE                PRIX    7J  ACTIONS", DIM);
    for (c, k) in data.companies.iter().enumerate() {
        let series = &g.comp_series[c];
        let now = series.last().copied().unwrap_or(0.0);
        let ch7 = change(series, 7);
        let held = g.sim.shares_of(c);
        let sel = c == g.comp_sel;
        let hov = g.hit_cells(2, 4 + c, 56, 1, Act::Call(Game::click_company, c), None);
        fb.text(
            2,
            4 + c,
            &format!(
                "{}{:<24.24}{:>8.2}",
                if sel { ">" } else { " " },
                k.name.to_uppercase(),
                now
            ),
            if sel {
                AMBER
            } else if hov {
                WHITE
            } else {
                GREEN
            },
        );
        fb.text(
            36,
            4 + c,
            &format!("{ch7:+5.0}%"),
            if ch7 >= 0.0 { UP } else { DOWN },
        );
        fb.text(
            44,
            4 + c,
            &format!("{held:>6.0}"),
            if held > 0.0 { AMBER } else { DIM },
        );
    }
    let Some(k) = data.companies.get(g.comp_sel) else {
        return;
    };
    let series = &g.comp_series[g.comp_sel];
    let n = series.len();
    let (lo, hi) = series
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let now = series[n - 1];
    let col = 62usize;
    fb.text(col, 3, &k.name.to_uppercase(), AMBER);
    let hq = data
        .factions
        .iter()
        .find(|f| f.id == k.hq)
        .map_or(k.hq.clone(), |f| f.name.to_uppercase());
    fb.text(
        col,
        4,
        &format!(
            "SIEGE : {hq}   DIVIDENDE {:.1} %/AN   {now:.2} CR   BAS {lo:.2}  HAUT {hi:.2}",
            k.yield_year * 100.0
        ),
        GREEN,
    );
    let mut sells: Vec<String> = k
        .goods
        .iter()
        .filter_map(|(r, w)| {
            data.resource_index(r).map(|i| {
                format!(
                    "{} {:.0}%",
                    data.resources[i].name.to_uppercase(),
                    w * 100.0
                )
            })
        })
        .collect();
    sells.truncate(4);
    fb.text(col, 5, &format!("VEND : {}", sells.join(", ")), DIM);
    // The chart.
    let (px0, py0) = (((col + 7) * 8) as i32, 8 * 10);
    let w = ((fb.cols() - col - 9) * 8) as i32;
    let h = 14 * 10;
    let pad = ((hi - lo) * 0.1).max(hi * 0.01);
    let (span_lo, span_hi) = (lo - pad, hi + pad);
    let x_of = |i: usize| px0 + (i as f64 / (n - 1).max(1) as f64 * f64::from(w)) as i32;
    let y_of = |v: f64| py0 + h - ((v - span_lo) / (span_hi - span_lo) * f64::from(h)) as i32;
    for q in 0..=4 {
        let v = span_lo + (span_hi - span_lo) * f64::from(q) / 4.0;
        let y = y_of(v);
        for x in (px0..px0 + w).step_by(6) {
            fb.plot(x, y, 0x0018_3028);
        }
        fb.text(col, (y / 10).max(0) as usize, &format!("{v:>6.1}"), DIM);
    }
    for i in 1..n {
        let up = series[i] >= series[i - 1];
        fb.line(
            x_of(i - 1) as f32,
            y_of(series[i - 1]) as f32,
            x_of(i) as f32,
            y_of(series[i]) as f32,
            if up { UP } else { DOWN },
        );
    }
    // Trading words and what the position is worth.
    let held = g.sim.shares_of(g.comp_sel);
    let acts: [(&str, KeyCode); 2] = [
        ("[B] ACHETER 10 ACTIONS (FRAIS 1 %)", KeyCode::KeyB),
        ("[N] VENDRE 10 ACTIONS", KeyCode::KeyN),
    ];
    let mut x = col;
    for (label, key) in acts {
        let hov = g.hit_cells(x, 24, label.len(), 1, Act::Key(key), None);
        fb.text(x, 24, label, if hov { AMBER } else { GREEN });
        x += label.len() + 3;
    }
    fb.text(
        col,
        26,
        &format!(
            "VOUS DETENEZ {held:.0} ACTIONS : {:.0} CR  (DIVIDENDE MENSUEL {:.0} CR)",
            held * now,
            held * now * k.yield_year / 12.0
        ),
        if held > 0.0 { AMBER } else { DIM },
    );
    fb.text(col, 28, &k.text, DIM);
    for (i, w) in g.messages.iter().rev().take(1).enumerate() {
        fb.text(col, 30 + i, w, AMBER);
    }
}

/// Futures: three clickable words and the list of open contracts with what each would pay now.
fn draw_futures(fb: &mut Framebuffer, g: &Game, sel: usize, col: usize) {
    let acts: [(&str, KeyCode); 3] = [
        ("[B] PARIER A LA HAUSSE (100 KG, 30 J)", KeyCode::KeyB),
        ("[N] PARIER A LA BAISSE", KeyCode::KeyN),
        ("[C] CLORE UN CONTRAT", KeyCode::KeyC),
    ];
    let mut x = col;
    for (label, key) in acts {
        let hover = g.hit_cells(x, 20, label.len(), 1, Act::Key(key), None);
        fb.text(x, 20, label, if hover { AMBER } else { GREEN });
        x += label.len() + 3;
    }
    fb.text(
        col,
        21,
        "CONTRATS A TERME (MARGE 25 %, REGLEMENT EN CREDITS)",
        DIM,
    );
    for (i, f) in g.sim.futures.iter().enumerate().take(5) {
        let now = g.sim.reference_price(f.pos, g.sim.time, f.res);
        let pnl = (now - f.strike) * f.kg * if f.long { 1.0 } else { -1.0 };
        let left = (f.expiry - g.sim.time) / vector_sim::bourse::DAY;
        fb.text(
            col,
            22 + i,
            &format!(
                "{}{} {:<14.14} {:>6.0} KG  PRIX {:>8.2}  {:>3.0} J  {:+9.0} CR",
                if f.res == sel { ">" } else { " " },
                if f.long { "HAUSSE" } else { "BAISSE" },
                g.data.resources[f.res].name.to_uppercase(),
                f.kg,
                f.strike,
                left,
                pnl
            ),
            if pnl >= 0.0 { UP } else { DOWN },
        );
    }
}

fn draw_chart(fb: &mut Framebuffer, g: &Game, sel: usize, col: usize) {
    let series = &g.bourse_series[sel];
    let res = &g.data.resources[sel];
    let n = series.len();
    let (lo, hi) = series
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let avg = series.iter().sum::<f64>() / n as f64;
    let now = series[n - 1];
    fb.text(col, 3, &res.name.to_uppercase(), AMBER);
    fb.text(
        col + 22,
        3,
        &format!(
            "{} CR/KG   7J {:+.0}%   BAS {}  HAUT {}  MOY {}",
            fmt_price(now),
            change(series, 7),
            fmt_price(lo),
            fmt_price(hi),
            fmt_price(avg)
        ),
        GREEN,
    );
    // Plot area in pixels.
    let (px0, py0) = (((col + 7) * 8) as i32, 5 * 10);
    let w = ((fb.cols() - col - 9) * 8) as i32;
    let h = 14 * 10;
    let (span_lo, span_hi) = {
        let pad = ((hi - lo) * 0.1).max(hi * 0.01);
        (lo - pad, hi + pad)
    };
    let x_of = |i: usize| px0 + (i as f64 / (n - 1).max(1) as f64 * f64::from(w)) as i32;
    let y_of = |v: f64| py0 + h - ((v - span_lo) / (span_hi - span_lo) * f64::from(h)) as i32;
    // Bands of world events that touch this resource (green: price up, red: down).
    let t_now = g.sim.time;
    let t0 = t_now - (n - 1) as f64 * vector_sim::bourse::DAY;
    for ev in &g.bourse_news {
        let m = g.data.world_events[ev.def].effect_on(&g.data.resources, sel);
        if (m - 1.0).abs() < 1e-9 {
            continue;
        }
        let day = |t: f64| ((t - t0) / vector_sim::bourse::DAY).clamp(0.0, (n - 1) as f64);
        let (xa, xb) = (
            x_of(day(ev.start) as usize),
            x_of(day(ev.end).ceil() as usize),
        );
        let tint = if m > 1.0 { 0x0008_2810 } else { 0x0030_0C0C };
        for x in xa..=xb {
            for y in py0..py0 + h {
                fb.px_set(x, y, tint);
            }
        }
    }
    // Grid and price labels.
    for k in 0..=4 {
        let v = span_lo + (span_hi - span_lo) * f64::from(k) / 4.0;
        let y = y_of(v);
        for x in (px0..px0 + w).step_by(6) {
            fb.plot(x, y, 0x0018_3028);
        }
        fb.text(
            col,
            (y / 10).max(0) as usize,
            &format!("{:>6}", fmt_price(v)),
            DIM,
        );
    }
    for (label, frac) in [
        (format!("-{}J", n - 1), 0.0),
        (format!("-{}J", (n - 1) / 2), 0.5),
        ("AUJ.".into(), 1.0),
    ] {
        let x = px0 + (frac * f64::from(w)) as i32;
        fb.text_at((x - 12).max(px0 - 20), py0 + h + 4, &label, DIM);
    }
    // Price line, then its 7-day average.
    for i in 1..n {
        let up = series[i] >= series[i - 1];
        fb.line(
            x_of(i - 1) as f32,
            y_of(series[i - 1]) as f32,
            x_of(i) as f32,
            y_of(series[i]) as f32,
            if up { UP } else { DOWN },
        );
    }
    let ma: Vec<f64> = (0..n)
        .map(|i| {
            let from = i.saturating_sub(6);
            series[from..=i].iter().sum::<f64>() / (i - from + 1) as f64
        })
        .collect();
    for i in 1..n {
        fb.line(
            x_of(i - 1) as f32,
            y_of(ma[i - 1]) as f32,
            x_of(i) as f32,
            y_of(ma[i]) as f32,
            AMBER,
        );
    }
    // The live price at the station, with the stock the player left behind, as a marker.
    let live = g.bourse_live[sel];
    let (lx, ly) = (x_of(n - 1), y_of(live.clamp(span_lo, span_hi)));
    for d in [
        (-3, 0),
        (3, 0),
        (0, -3),
        (0, 3),
        (-2, -2),
        (2, 2),
        (-2, 2),
        (2, -2),
    ] {
        fb.plot(lx + d.0, ly + d.1, 0x00FF_FFFF);
    }
    fb.text_at(
        (px0 + w - 8 * 28).max(px0),
        py0 - 11,
        &format!(
            "PRIX ICI {} (ACHAT {})",
            fmt_price(live),
            fmt_price(live * 1.05)
        ),
        0x00FF_FFFF,
    );
}

fn draw_news(fb: &mut Framebuffer, g: &Game, sel: usize, col: usize) {
    let top = 28usize;
    fb.text(col, top, "ACTUALITES DU MONDE", AMBER);
    let now = g.sim.time;
    let max_rows = fb.rows().saturating_sub(top + 4);
    let mut row = top + 1;
    let width = fb.cols().saturating_sub(col + 2);
    for ev in &g.bourse_news {
        if row >= top + 1 + max_rows {
            break;
        }
        let def = &g.data.world_events[ev.def];
        let m = def.effect_on(&g.data.resources, sel);
        let status = if ev.start > now {
            "A VENIR".to_string()
        } else if ev.end < now {
            format!(
                "FINI IL Y A {:.0} J",
                (now - ev.end) / vector_sim::bourse::DAY
            )
        } else {
            format!(
                "EN COURS, FIN DANS {:.0} J",
                (ev.end - now) / vector_sim::bourse::DAY
            )
        };
        let scope = if ev.global { "GALAXIE" } else { "REGION" };
        let colr = if (m - 1.0).abs() > 1e-9 { AMBER } else { GREEN };
        let mut head = format!("{} [{scope}] {status}", def.name.to_uppercase());
        head.truncate(width);
        fb.text(col, row, &head, colr);
        row += 1;
        // The three strongest effects, as percentages at this occurrence's strength.
        let mut fx: Vec<(String, f64)> = def
            .effects
            .iter()
            .map(|(k, mult)| {
                let name = match k.strip_prefix("cat:") {
                    Some(c) => format!("TOUS {}", c.to_uppercase()),
                    None => g
                        .data
                        .resource_index(k)
                        .map_or(k.clone(), |r| g.data.resources[r].name.to_uppercase()),
                };
                (name, (mult - 1.0) * ev.strength * 100.0)
            })
            .collect();
        fx.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        let line: Vec<String> = fx
            .iter()
            .take(4)
            .map(|(n, p)| format!("{n} {p:+.0}%"))
            .collect();
        let mut line = format!("  {}", line.join("  "));
        line.truncate(width);
        if row < top + 1 + max_rows {
            fb.text(col, row, &line, DIM);
            row += 1;
        }
    }
    if g.bourse_news.is_empty() {
        fb.text(col, row, "RIEN A SIGNALER : LES MARCHES SONT CALMES.", DIM);
    }
}
