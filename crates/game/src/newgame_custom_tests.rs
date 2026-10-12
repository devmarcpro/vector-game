//! Tests of the customisation screens of the new-game flow: each choice reaches the real game.
use super::*;
use vector_data::GameData;
use winit::keyboard::KeyCode;

fn data() -> Arc<GameData> {
    Arc::new(
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data"),
    )
}

fn flow() -> Game {
    let mut g = Game::new(data(), 1);
    g.open_new_game();
    g
}

fn click(g: &mut Game, id: u32, part: usize) {
    let rows = g.ng_rows();
    let r = rows
        .iter()
        .position(|r| r.id == id)
        .unwrap_or_else(|| panic!("row {id}"));
    g.click_ng_field(r * 4 + part);
}

fn type_text(g: &mut Game, s: &str) {
    g.newgame_key(KeyCode::KeyA, Some(s));
}

fn select(g: &mut Game, id: u32) {
    let rows = g.ng_rows();
    g.ng.row[g.ng.step] = rows.iter().position(|r| r.id == id).expect("row");
}

fn frame(g: &Game) {
    let mut fb = Framebuffer::new(1280, 720);
    crate::draw_frame(&mut fb, g);
}

#[test]
fn every_step_has_rows_draws_and_survives_every_click() {
    for step in 0..9 {
        let mut g = flow();
        g.ng_goto(step);
        let n = g.ng_rows().len();
        frame(&g);
        for i in 0..n {
            for part in 0..4 {
                let mut h = flow();
                h.ng_goto(step);
                h.click_ng_field(i * 4 + part);
                frame(&h);
            }
        }
    }
}

#[test]
fn a_catalogue_ship_chosen_in_the_flow_is_the_ship_of_the_game() {
    let mut g = flow();
    g.ng_goto(2);
    click(&mut g, S_MODE, 2);
    assert_eq!(g.ng.custom.ship.mode, ShipMode::Catalog);
    let hull = g.ng.custom.ship.hull.clone();
    let slot =
        g.ng.custom
            .ship
            .modules
            .iter()
            .position(|m| !m.is_empty())
            .expect("a module");
    // The weapon or any module taken out stays out.
    let last =
        g.ng.custom
            .ship
            .modules
            .iter()
            .rposition(|m| !m.is_empty())
            .unwrap_or(slot);
    click(&mut g, S_SLOT + last as u32, 3);
    select(&mut g, S_NAME);
    type_text(&mut g, "Aurore");
    let cfg = g.ng_config();
    let r = sc::review(&g.data, &cfg);
    if r.error.is_none() {
        assert!(g.start_game(&cfg));
        assert_eq!(g.data.hulls[g.sim.ship.design.hull].id, hull);
        assert_eq!(g.sim.custom.ship_name, "AURORE");
        let want = &g.ng.custom.ship.modules[last];
        let have = g.sim.ship.design.slots[last].map(|m| g.data.modules[m].id.clone());
        assert_eq!(have.unwrap_or_default(), *want);
    } else {
        assert!(!g.start_game(&cfg), "a refused ship is not started");
    }
}

#[test]
fn a_design_from_the_sale_list_starts_a_flying_ship_and_a_small_budget_refuses() {
    let mut g = flow();
    g.ng_goto(2);
    g.ng.custom.ship.budget = Some(1e9);
    click(&mut g, S_MODE, 2);
    click(&mut g, S_MODE, 2);
    assert_eq!(g.ng.custom.ship.mode, ShipMode::Design);
    let n = g.ng_plan_list().len();
    let mut started = false;
    for _ in 0..n {
        let cfg = g.ng_config();
        if sc::review(&g.data, &cfg).error.is_none() {
            assert!(g.start_game(&cfg));
            assert!(g.sim.data.hulls[g.sim.ship.design.hull]
                .id
                .starts_with("custom_"));
            assert!(g.sim.readiness().engine > 0.0);
            started = true;
            break;
        }
        click(&mut g, S_PLAN, 2);
    }
    assert!(started, "one plan on sale starts a game");
    let mut g = flow();
    g.ng_goto(2);
    g.ng.custom.ship.budget = Some(1e9);
    click(&mut g, S_MODE, 2);
    click(&mut g, S_MODE, 2);
    g.ng.custom.ship.budget = Some(1000.0);
    let cfg = g.ng_config();
    assert!(!g.start_game(&cfg));
    assert!(g.messages.last().is_some_and(|m| m.contains("ECHEC")));
}

#[test]
fn the_designer_comes_back_to_the_flow_with_the_plan() {
    let mut g = flow();
    g.ng_goto(2);
    click(&mut g, S_MODE, 2);
    click(&mut g, S_MODE, 2);
    click(&mut g, S_EDIT, 1);
    assert_eq!(g.screen, Screen::Designer);
    assert!(g.ng.designing);
    frame(&g);
    g.key(KeyCode::Escape, None, true);
    assert_eq!(g.screen, Screen::NewGame);
    assert!(!g.ng.designing);
    assert!(!g.ng.custom.ship.plan.is_empty());
}

#[test]
fn crew_rows_edit_the_person_within_the_budgets() {
    let mut g = flow();
    g.ng_goto(3);
    let k = g.ng.picks[0].candidate;
    g.ng.cursor = k;
    g.ng.crew_focus = true;
    select(&mut g, C_NAME);
    type_text(&mut g, "Z");
    assert!(g
        .ng
        .custom
        .crew
        .edit_of(k)
        .and_then(|e| e.name.clone())
        .is_some_and(|n| n.ends_with('Z')));
    for _ in 0..30 {
        click(&mut g, C_SKILL, 2);
        click(&mut g, C_SKILL + 1, 2);
    }
    let e = g.ng.custom.crew.edit_of(k).cloned().unwrap_or_default();
    assert!(sc::edit_spent(&g.data, &e).0 <= sc::SKILL_POINTS);
    assert!(!g.ng.note.is_empty(), "the refusal is said");
    let cfg = g.ng_config();
    assert!(g.start_game(&cfg));
    assert!(g.sim.crew.members.iter().any(|p| p.name.ends_with('Z')));
}

#[test]
fn rerolls_replace_members_and_locks_keep_them() {
    let mut g = flow();
    g.ng_goto(3);
    let before: Vec<usize> = g.ng.picks.iter().map(|p| p.candidate).collect();
    let first = before[0];
    g.ng.custom.crew.locked.push(first);
    g.ng_reroll_all(7);
    let after: Vec<usize> = g.ng.picks.iter().map(|p| p.candidate).collect();
    assert_eq!(after.len(), before.len());
    assert!(after.contains(&first), "the locked one stays");
    let names: Vec<String> = g.ng.pool.iter().map(|p| p.name.clone()).collect();
    click(&mut g, C_NEWPOOL, 1);
    let other: Vec<String> = g.ng.pool.iter().map(|p| p.name.clone()).collect();
    assert_ne!(names, other);
}

#[test]
fn the_ai_step_sets_the_ai_of_the_game() {
    let mut g = flow();
    g.ng_goto(4);
    select(&mut g, A_NAME);
    type_text(&mut g, "Pythie");
    click(&mut g, A_PERS, 2);
    assert_eq!(g.ng.custom.ai.personality, 1);
    click(&mut g, A_CORE, 2);
    let cfg = g.ng_config();
    assert!(g.start_game(&cfg));
    assert_eq!(g.sim.custom.ai_name, "PYTHIE");
    assert!(g.sim.mind.bias.iter().any(|b| *b != 0.0));
    assert!(g.sim.mind.power_bonus > 0.0);
}

#[test]
fn the_world_step_changes_the_start() {
    let mut g = flow();
    g.ng_goto(6);
    click(&mut g, W_PLACE, 2);
    click(&mut g, W_PLACE, 2);
    click(&mut g, W_EPI, 2);
    click(&mut g, W_SYSTEM, 2);
    assert!(g.ng.world_preview.is_some());
    let cfg = g.ng_config();
    assert_eq!(cfg.custom.world.place, Some(1));
    assert!(cfg.custom.world.epidemics > 1.0);
    assert!(g.start_game(&cfg));
    assert!(g.sim.custom.epidemics > 1.0);
}

#[test]
fn rule_categories_and_save_modes_apply() {
    let mut g = flow();
    g.ng_goto(7);
    click(&mut g, O_RULE + 2, 2);
    assert!(g.pending_rules.permadeath);
    click(&mut g, O_SAVE, 2);
    click(&mut g, O_SAVE, 2);
    assert_eq!(g.ng.custom.save_mode, 2);
    click(&mut g, O_PRESET, 2);
    let p = g.ng.preset_idx.expect("a preset");
    let want = g.data.start.presets[p]
        .settings
        .iter()
        .find(|(k, _)| k == "damage")
        .map_or(1.0, |x| x.1);
    assert_eq!(g.pending_rules.damage, want);
    let cfg = g.ng_config();
    assert!(g.start_game(&cfg));
    assert_eq!(g.sim.custom.save_mode, cfg.custom.save_mode);
    if cfg.custom.save_mode > 0 {
        g.sim.docked = None;
        g.save_game();
        assert!(g.messages.last().is_some_and(|m| m.contains("REFUSEE")));
    }
}

#[test]
fn presets_go_to_files_and_back_and_the_random_button_is_valid() {
    let mut g = flow();
    g.ng_goto(8);
    click(&mut g, R_RANDOM, 1);
    let cfg = g.ng_config();
    assert!(sc::review(&g.data, &cfg).error.is_none());
    select(&mut g, R_NAME);
    g.ng.preset_name.clear();
    type_text(&mut g, "essai ui");
    click(&mut g, R_SAVE, 1);
    assert!(g.ng.note.contains("SAUVEE"), "{}", g.ng.note);
    g.ng.custom = sc::CustomStart::default();
    g.ng.custom.ai.name = "AUTRE".into();
    let files = list_presets();
    g.ng.preset_pick = files
        .iter()
        .position(|f| f == "essai_ui.ron")
        .expect("file");
    click(&mut g, R_LOAD, 1);
    assert!(g.ng.note.contains("CHARGEE"), "{}", g.ng.note);
    let back = g.ng_config();
    assert_eq!(back.custom, cfg.custom);
    assert_eq!(back.seed, cfg.seed);
    assert_eq!(back.scenario, cfg.scenario);
    click(&mut g, R_EXPORT, 1);
    let code = std::fs::read_to_string(presets_dir().join("essai_ui.code.txt")).expect("code");
    std::fs::write(presets_dir().join("import.txt"), &code).expect("write");
    g.ng.custom = sc::CustomStart::default();
    click(&mut g, R_IMPORT, 1);
    assert_eq!(g.ng_config().custom, cfg.custom);
    let again = g.ng_config();
    assert!(g.start_game(&again));
}

#[test]
fn contacts_are_added_edited_and_start_in_the_game() {
    let mut g = flow();
    g.ng_goto(3);
    click(&mut g, C_TAB, 2);
    click(&mut g, C_CT_ADD, 1);
    click(&mut g, C_CT_ADD, 1);
    assert_eq!(g.ng.custom.crew.contacts.len(), 2);
    click(&mut g, C_CT + 1, 2);
    let c = &g.ng.custom.crew.contacts;
    assert_ne!(c[0].faction, c[1].faction, "one per faction");
    click(&mut g, C_CT + 3, 1);
    assert!(!g.ng.custom.crew.contacts[1].friend);
    let cfg = g.ng_config();
    assert!(g.start_game(&cfg));
}

#[test]
fn typing_goes_to_text_rows_and_arrows_still_move() {
    let mut g = flow();
    g.ng_goto(2);
    select(&mut g, S_NAME);
    g.newgame_key(KeyCode::KeyA, Some("a"));
    g.newgame_key(KeyCode::KeyB, Some("b"));
    assert_eq!(g.ng.custom.ship.name, "ab");
    g.newgame_key(KeyCode::Backspace, None);
    assert_eq!(g.ng.custom.ship.name, "a");
    let r = g.ng.row[2];
    g.newgame_key(KeyCode::ArrowUp, None);
    assert_ne!(g.ng.row[2], r, "arrows move between rows");
    select(&mut g, S_NAME);
    g.ng.custom.ship.name.clear();
    g.newgame_key(KeyCode::Tab, None);
    assert!(g.ng.custom.ship.name.len() > 2, "Tab draws a name");
}

#[test]
fn the_cryosleep_rows_put_the_chosen_member_to_sleep_or_say_why_not() {
    let mut g = flow();
    g.ng_goto(3);
    g.ng.crew_focus = true;
    let k = g.ng.picks[0].candidate;
    g.ng.cursor = k;
    click(&mut g, C_CRYO, 1);
    click(&mut g, C_WAKE_ARR, 1);
    click(&mut g, C_WAKE_DAY, 2);
    assert_eq!(g.ng.custom.cryo.members, vec![k]);
    assert_eq!(g.ng.custom.cryo.wake_day, Some(10.0));
    let cfg = g.ng_config();
    if g.start_game(&cfg) {
        assert!(g.sim.crew.members[0].body.cryo);
        assert_eq!(g.sim.crew.cryo.orders.len(), 2);
    } else {
        assert!(g.messages.last().is_some_and(|m| m.contains("ECHEC")));
    }
}
