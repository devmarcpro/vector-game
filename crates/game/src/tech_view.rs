//! Technology tree (F4): the technologies as boxes in columns by depth (how many prerequisites
//! separate them from the start), grouped by field inside a column, joined by lines from each
//! prerequisite to what it unlocks. Arrows walk the graph, Enter starts a research.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use vector_data::GameData;
use vector_render::Framebuffer;

const WHITE: u32 = 0x00FF_FFFF;
const BOX_W: usize = 23;
const COL_GAP: usize = 4;
const COL: usize = BOX_W + COL_GAP;
const ROW_H: usize = 3;

/// Where each technology sits: its column (depth) and its row in that column.
pub struct TechLayout {
    pub layer: Vec<usize>,
    pub row: Vec<usize>,
    /// Technologies of each column from top to bottom.
    pub layers: Vec<Vec<usize>>,
}

impl TechLayout {
    pub fn requires(data: &GameData, t: usize) -> Vec<usize> {
        data.techs[t]
            .requires
            .iter()
            .filter_map(|r| data.tech_index(r))
            .collect()
    }

    pub fn dependents(data: &GameData, t: usize) -> Vec<usize> {
        (0..data.techs.len())
            .filter(|&u| Self::requires(data, u).contains(&t))
            .collect()
    }
}

/// Depth of each technology, then an order inside each column that keeps the fields together and
/// puts a technology near the average position of its prerequisites (fewer crossing lines).
pub fn layout(data: &GameData) -> TechLayout {
    let n = data.techs.len();
    let reqs: Vec<Vec<usize>> = (0..n).map(|t| TechLayout::requires(data, t)).collect();
    let mut layer = vec![usize::MAX; n];
    fn depth(t: usize, reqs: &[Vec<usize>], layer: &mut [usize]) -> usize {
        if layer[t] != usize::MAX {
            return layer[t];
        }
        // The data crate guarantees a graph without cycles.
        let d = reqs[t]
            .iter()
            .map(|&r| depth(r, reqs, layer) + 1)
            .max()
            .unwrap_or(0);
        layer[t] = d;
        d
    }
    for t in 0..n {
        depth(t, &reqs, &mut layer);
    }
    let columns = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut fields: Vec<&str> = data.techs.iter().map(|t| t.field.as_str()).collect();
    fields.sort_unstable();
    fields.dedup();
    let field_of = |t: usize| {
        fields
            .iter()
            .position(|f| *f == data.techs[t].field)
            .unwrap_or(0)
    };
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); columns];
    for t in 0..n {
        layers[layer[t]].push(t);
    }
    for col in &mut layers {
        col.sort_by(|&a, &b| {
            field_of(a)
                .cmp(&field_of(b))
                .then(data.techs[a].cost.total_cmp(&data.techs[b].cost))
                .then(data.techs[a].id.cmp(&data.techs[b].id))
        });
    }
    let mut row = vec![0usize; n];
    for col in &layers {
        for (i, &t) in col.iter().enumerate() {
            row[t] = i;
        }
    }
    for _ in 0..3 {
        for col in layers.iter_mut().skip(1) {
            let mut keyed: Vec<(usize, f64, usize)> = col
                .iter()
                .map(|&t| {
                    let avg = if reqs[t].is_empty() {
                        row[t] as f64
                    } else {
                        reqs[t].iter().map(|&r| row[r] as f64).sum::<f64>() / reqs[t].len() as f64
                    };
                    (field_of(t), avg, t)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
            *col = keyed.into_iter().map(|k| k.2).collect();
            for (i, &t) in col.iter().enumerate() {
                row[t] = i;
            }
        }
    }
    TechLayout { layer, row, layers }
}

fn rect(fb: &mut Framebuffer, l: f32, t: f32, r: f32, b: f32, c: u32) {
    fb.line(l, t, r, t, c);
    fb.line(l, b, r, b, c);
    fb.line(l, t, l, b, c);
    fb.line(r, t, r, b, c);
}

fn field_color(field: &str) -> u32 {
    match field {
        "propulsion" => 0x0050_B0FF,
        "energie" => 0x00FF_D040,
        "armement" => 0x00FF_5050,
        "structure" => 0x00B0B0B0,
        "sciences_vie" | "sciences" => 0x0060_E080,
        "industrie" => 0x00E0_8040,
        _ => 0x00A0_A0FF,
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let data = &g.data;
    let lay = layout(data);
    let sel = g.tech_sel.min(data.techs.len() - 1);
    let (cols, rows) = (fb.cols(), fb.rows());
    let (top, bottom) = (5usize, rows.saturating_sub(9));
    let (view_w, view_h) = (cols.saturating_sub(2), bottom.saturating_sub(top));
    // The camera follows the selection.
    let pos = |t: usize| (lay.layer[t] * COL, lay.row[t] * ROW_H);
    let total_w = lay.layers.len() * COL;
    let total_h = lay.layers.iter().map(Vec::len).max().unwrap_or(0) * ROW_H;
    let (sx, sy) = pos(sel);
    let cam_x = (sx + BOX_W / 2)
        .saturating_sub(view_w / 2)
        .min(total_w.saturating_sub(view_w));
    let cam_y = (sy + 1)
        .saturating_sub(view_h / 2)
        .min(total_h.saturating_sub(view_h));
    let screen = |t: usize| -> (i64, i64) {
        let (x, y) = pos(t);
        (
            1 + x as i64 - cam_x as i64,
            top as i64 + y as i64 - cam_y as i64,
        )
    };
    let state_color = |t: usize| -> u32 {
        if s.researched[t] {
            0x0040_A060
        } else if s.current_research == Some(t) {
            AMBER
        } else if s.tech_available(t) {
            GREEN
        } else {
            0x0070_3030
        }
    };
    // Lines first, so the boxes sit on top. The ones touching the selection are bright.
    let related_up = TechLayout::requires(data, sel);
    let related_down = TechLayout::dependents(data, sel);
    for t in 0..data.techs.len() {
        for r in TechLayout::requires(data, t) {
            let ((x1, y1), (x2, y2)) = (screen(r), screen(t));
            let (px1, py1) = (((x1 + BOX_W as i64) * 8) as f32 + 1.0, (y1 * 10 + 5) as f32);
            let (px2, py2) = ((x2 * 8) as f32 - 3.0, (y2 * 10 + 5) as f32);
            let hot =
                (t == sel && related_up.contains(&r)) || (r == sel && related_down.contains(&t));
            let c = if hot {
                AMBER
            } else if s.researched[r] {
                0x0030_6040
            } else {
                0x0040_2020
            };
            // From the right of the prerequisite, a jog in the gap before the target.
            let mid = px2 - (COL_GAP * 8 / 2) as f32;
            fb.line(px1, py1, mid, py1, c);
            fb.line(mid, py1, mid, py2, c);
            fb.line(mid, py2, px2, py2, c);
        }
    }
    for t in 0..data.techs.len() {
        let (x, y) = screen(t);
        if x < 1 || y < top as i64 || x + BOX_W as i64 > cols as i64 - 1 || y + 2 > bottom as i64 {
            continue;
        }
        let (x, y) = (x as usize, y as usize);
        let tech = &data.techs[t];
        let c = state_color(t);
        let (l, tp) = ((x * 8) as f32 - 3.0, (y * 10) as f32 - 3.0);
        let (r, b) = (((x + BOX_W) * 8) as f32 + 1.0, ((y + 2) * 10) as f32 + 1.0);
        // Opaque: lines of other links pass behind the box.
        for py in tp as i32..=b as i32 {
            for px in l as i32..=r as i32 {
                fb.px_set(px, py, 0);
            }
        }
        rect(fb, l, tp, r, b, c);
        if t == sel {
            rect(fb, l - 2.0, tp - 2.0, r + 2.0, b + 2.0, WHITE);
        }
        // A bar in the colour of the field along the left edge.
        let fc = field_color(&tech.field);
        for k in 0..20 {
            fb.px_set((x * 8) as i32 - 5, (y * 10) as i32 - 2 + k, fc);
        }
        fb.text(
            x,
            y,
            &tech
                .name
                .to_uppercase()
                .chars()
                .take(BOX_W)
                .collect::<String>(),
            if t == sel { WHITE } else { c },
        );
        let line = if s.researched[t] {
            "ACQUISE".to_string()
        } else if s.current_research == Some(t) {
            format!(
                "{:.0} % EN COURS",
                100.0 * s.research_progress / tech.cost.max(1.0)
            )
        } else {
            format!("{:.0} PTS", tech.cost)
        };
        fb.text(x, y + 1, &line.chars().take(BOX_W).collect::<String>(), DIM);
    }
    // Clear the margins the lines may have crossed, then the panels.
    for y in 0..(top * 10) as i32 {
        for x in 0..fb.w as i32 {
            fb.px_set(x, y, 0);
        }
    }
    for y in (bottom * 10) as i32..(rows * 10) as i32 {
        for x in 0..fb.w as i32 {
            fb.px_set(x, y, 0);
        }
    }
    fb.text(1, 1, "ARBRE TECHNOLOGIQUE", AMBER);
    fb.text(
        1,
        2,
        &format!(
            "RESERVE {:.0} PTS   LABOS DE BORD {:.1} PTS/J   CONNUES {}/{}   COLONNES : PROFONDEUR DES PREREQUIS",
            s.research_pool,
            s.ship_research_rate().max(0.0),
            s.researched.iter().filter(|&&r| r).count(),
            s.researched.len()
        ),
        GREEN,
    );
    if let Some(t) = s.current_research {
        let tech = &data.techs[t];
        fb.text(
            1,
            3,
            &format!(
                "EN COURS : {}  {:.0}/{:.0}",
                tech.name.to_uppercase(),
                s.research_progress,
                tech.cost
            ),
            AMBER,
        );
    }
    // Legend of the fields.
    let mut fields: Vec<&str> = data.techs.iter().map(|t| t.field.as_str()).collect();
    fields.sort_unstable();
    fields.dedup();
    let mut lx = 60usize;
    for f in fields {
        fb.text(lx, 3, &f.replace('_', " ").to_uppercase(), field_color(f));
        lx += f.len() + 2;
    }
    // Detail of the selection.
    let tech = &data.techs[sel];
    let d = bottom + 1;
    let state = if s.researched[sel] {
        "ACQUISE"
    } else if s.current_research == Some(sel) {
        "EN COURS"
    } else if s.tech_available(sel) {
        "DISPONIBLE : ENTREE POUR LANCER"
    } else {
        "VERROUILLEE"
    };
    fb.text(
        1,
        d,
        &format!(
            "{}  [{}]  COUT {:.0} PTS  {state}",
            tech.name.to_uppercase(),
            tech.field.to_uppercase(),
            tech.cost
        ),
        state_color(sel),
    );
    let reqs: Vec<String> = related_up
        .iter()
        .map(|&i| {
            format!(
                "{}{}",
                data.techs[i].name.to_uppercase(),
                if s.researched[i] { " (OK)" } else { " (!)" }
            )
        })
        .collect();
    fb.text(
        1,
        d + 1,
        &format!(
            "PREREQUIS : {}",
            if reqs.is_empty() {
                "AUCUN".to_string()
            } else {
                reqs.join(", ")
            }
        ),
        if related_up.iter().all(|&i| s.researched[i]) {
            GREEN
        } else {
            RED
        },
    );
    let deps: Vec<String> = related_down
        .iter()
        .map(|&i| data.techs[i].name.to_uppercase())
        .collect();
    fb.text(
        1,
        d + 2,
        &format!(
            "OUVRE : {}",
            if deps.is_empty() {
                "RIEN (FIN DE BRANCHE)".to_string()
            } else {
                deps.join(", ")
            }
        ),
        GREEN,
    );
    let unlocks: Vec<String> = tech
        .unlocks
        .iter()
        .map(|(k, id)| {
            let name = match k.as_str() {
                "module" => data.module_index(id).map(|i| data.modules[i].name.clone()),
                "hull" => data.hull_index(id).map(|i| data.hulls[i].name.clone()),
                "building" => data
                    .building_index(id)
                    .map(|i| data.buildings[i].name.clone()),
                _ => None,
            };
            format!(
                "{} {}",
                k.to_uppercase(),
                name.unwrap_or_else(|| id.clone()).to_uppercase()
            )
        })
        .collect();
    let text = format!(
        "DEBLOQUE : {}",
        if unlocks.is_empty() {
            "RIEN".to_string()
        } else {
            unlocks.join(" ; ")
        }
    );
    fb.text(
        1,
        d + 3,
        &text
            .chars()
            .take(cols.saturating_sub(2))
            .collect::<String>(),
        GREEN,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data")
    }

    #[test]
    fn every_prerequisite_sits_in_an_earlier_column_and_rows_are_unique() {
        let d = data();
        let lay = layout(&d);
        for t in 0..d.techs.len() {
            for r in TechLayout::requires(&d, t) {
                assert!(
                    lay.layer[r] < lay.layer[t],
                    "{} before {}",
                    d.techs[r].id,
                    d.techs[t].id
                );
            }
            assert_eq!(
                lay.layers[lay.layer[t]][lay.row[t]], t,
                "row index matches the column"
            );
        }
        let placed: usize = lay.layers.iter().map(Vec::len).sum();
        assert_eq!(
            placed,
            d.techs.len(),
            "everything is on the graph exactly once"
        );
        // The start technologies are in the first column.
        for (t, tech) in d.techs.iter().enumerate() {
            if tech.requires.is_empty() {
                assert_eq!(lay.layer[t], 0);
            }
        }
    }

    #[test]
    fn dependents_and_requirements_mirror_each_other() {
        let d = data();
        for t in 0..d.techs.len() {
            for r in TechLayout::requires(&d, t) {
                assert!(TechLayout::dependents(&d, r).contains(&t));
            }
        }
    }
}
