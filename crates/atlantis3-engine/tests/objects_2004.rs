//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): chapter 1 in
//! the ship's cabin played by clicks — the trunk's place sprites
//! (`cabine.wam` view 2: `MALLE_FERME` A1 and `MALLE_OUVERTE` A3 have
//! click kind 3, view 1's have 0), the bag (FUN_0042a980, FUN_0042aaf0),
//! the inventory (FUN_00428690, FUN_00428940, FUN_0042b170), the object
//! in the hand (FUN_00429600, FUN_00429780) and a zone clicked with it,
//! the compositions (FUN_0042ab80) and the followers (FUN_0043d240,
//! FUN_0043d000).
//!
//! chapi001.con: `/INIT` puts `ALLUMETTES` in the bag; /002 the closed
//! trunk clicked (flag3 = 0) opens it (flag3 = 1); /003 the open trunk
//! clicked: `DIAL=4,off`, closed again (flag3 = 2); /004 the closed trunk
//! clicked again: `SACADOS=MALLE`, `OBJETENMAIN=MALLE`, the trunk hidden,
//! the zone `POSE_MALLE` enabled; /005 that zone clicked with the trunk in
//! the hand: `SACADOS=NOT_MALLE`, the trunk shown again.
//!
//! `OBJECTS_2004_PPM=dir` writes the screens (bag, trunk) as PPM files.

use std::sync::Arc;

use atlantis2_data::defnames;
use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::inventory2004::Layout;
use atlantis3_engine::px::Surface;

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    Some(Game::new(A3Files::evolution_2004(src)).expect("the 2004 program's game"))
}

/// The cabin's view 1 after the intro.
fn cabin() -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    assert_eq!(g.mode(), Mode::Warp);
    Some((g, 0.01))
}

fn object(g: &Game, name: &str) -> i32 {
    g.names()
        .id(defnames::OBJETS, name.as_bytes())
        .unwrap_or_else(|_| panic!("{name}")) as i32
}

fn bag(g: &Game) -> Vec<i32> {
    g.script().unwrap().bag.iter().map(|e| e.id).collect()
}

fn hand(g: &Game) -> i32 {
    g.object_in_hand()
}

fn flag3(g: &Game) -> i32 {
    g.script().unwrap().flags[3]
}

/// 25 frames a second for `secs`.
fn run(g: &mut Game, t: &mut f64, secs: f64) {
    let end = *t + secs;
    while *t < end {
        *t += 0.04;
        g.step(*t);
    }
}

/// Steps until `done`, at most `secs`.
fn run_until(g: &mut Game, t: &mut f64, secs: f64, mut done: impl FnMut(&Game) -> bool) -> bool {
    let end = *t + secs;
    while *t < end {
        *t += 0.04;
        g.step(*t);
        if done(g) {
            return true;
        }
    }
    false
}

fn shown(g: &Game, spw: &str) -> bool {
    g.slots()
        .slots
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(spw))
        .unwrap_or_else(|| panic!("{spw}"))
        .shown
}

fn write_ppm(g: &Game, name: &str) {
    let Ok(dir) = std::env::var("OBJECTS_2004_PPM") else {
        return;
    };
    let (w, h) = (g.width(), g.height());
    let mut out = format!("P6 {w} {h} 255\n").into_bytes();
    match g.surface() {
        Surface::Rgb565(p) => {
            for &c in p {
                let c = u32::from(c);
                out.extend([
                    ((c >> 11) << 3) as u8,
                    (((c >> 5) & 63) << 2) as u8,
                    ((c & 31) << 3) as u8,
                ]);
            }
        }
        Surface::Xrgb8888(p) => {
            for &c in p {
                out.extend([(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    std::fs::write(std::path::Path::new(&dir).join(name), out).unwrap();
}

fn zone_kind(g: &Game, name: &str) -> i32 {
    g.wam()
        .view(2)
        .unwrap()
        .zones
        .iter()
        .find(|z| z.name.as_deref() == Some(name))
        .unwrap()
        .kind
}

/// The pointer on the bag's slot `i` (scroll 0).
fn point_slot(g: &mut Game, i: i32) {
    let lay = Layout::for_width(g.width());
    let (x, y) = lay.item_centre(i, g.height());
    g.set_pointer(x, y);
}

/// Turns to the zone `name` and clicks it. The aim sets the camera
/// alone: the ship's node keeps the camera's orientation inside it, which
/// the next 25 Hz look tick puts back; so, as `cursor_2004.rs` does, the
/// click's frame comes 1 ms later (no tick).
fn click_zone(g: &mut Game, t: &mut f64, name: &str) {
    assert!(
        g.aim_at_zone(|z| z.name.as_deref() == Some(name)),
        "{name} not under the centre"
    );
    g.click();
    *t += 0.001;
    g.step(*t);
}

/// View 1 → view 2 by its move zone.
fn to_view_2(g: &mut Game, t: &mut f64) {
    assert!(g.aim_at_zone(|z| z.kind == 1 && z.value == 2));
    g.click();
    assert!(
        run_until(g, t, 10.0, |g| g.place().1 == Some(2)
            && g.mode() == Mode::Warp),
        "view 2 not reached"
    );
    run(g, t, 0.5);
}

#[test]
fn chapter_1_by_clicks() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let matches = object(&g, "ALLUMETTES");
    let malle = object(&g, "MALLE");
    // `/INIT`: the matches in the bag.
    assert_eq!(bag(&g), [matches]);
    assert_eq!(hand(&g), 0);

    // The inventory: the right button opens it, the fade runs, the item
    // is drawn in slot 0.
    g.right_click();
    assert!(g.inventory_open());
    assert_eq!(g.mode(), Mode::Inventory);
    run(&mut g, &mut t, 0.5);
    point_slot(&mut g, 3);
    t += 0.04;
    g.step(t);
    write_ppm(&g, "bag.ppm");
    // The left button on the matches: in the hand, the inventory open.
    point_slot(&mut g, 0);
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(hand(&g), matches);
    assert!(g.inventory_open());
    // Again on the same item: the hand emptied.
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(hand(&g), 0);
    // The right button above the bar closes it.
    g.set_pointer(500, 300);
    g.right_click();
    t += 0.04;
    g.step(t);
    assert!(!g.inventory_open());
    assert_eq!(g.mode(), Mode::Warp);
    run(&mut g, &mut t, 0.5);

    // View 1's trunk is not clickable (click kind 0); view 2's is.
    to_view_2(&mut g, &mut t);
    let ferme = object(&g, "MALLE_FERME");
    // `/INIT`'s `DISABLEZONE=POSE_MALLE` (FUN_004420f0): type 3 + 88888.
    assert_eq!(zone_kind(&g, "POSE_MALLE"), 3 + 88888);
    let ouverte = object(&g, "MALLE_OUVERTE");
    assert!(shown(&g, "mlA1002002"));
    // /002: the closed trunk clicked opens it.
    assert!(
        g.click_sprite(ferme, 2),
        "the closed trunk is not under the centre"
    );
    run(&mut g, &mut t, 0.2);
    assert_eq!(flag3(&g), 1);
    assert!(!shown(&g, "mlA1002002"));
    run(&mut g, &mut t, 2.0);
    assert!(shown(&g, "mlA3002002"), "the open trunk after its play");
    write_ppm(&g, "trunk_open.ppm");
    // /003: the open trunk clicked: the narrator's dialog 4, then closed.
    assert!(
        g.click_sprite(ouverte, 2),
        "the open trunk is not under the centre"
    );
    run(&mut g, &mut t, 0.1);
    assert_eq!(g.dialog_2004().map(|d| d.0), Some(4));
    assert!(
        run_until(&mut g, &mut t, 30.0, |g| g.dialog_2004().is_none()),
        "dialog 4 did not end"
    );
    run(&mut g, &mut t, 2.0);
    assert_eq!(flag3(&g), 2);
    // /004: the closed trunk clicked: in the bag and in the hand, hidden.
    assert!(g.click_sprite(ferme, 2));
    run(&mut g, &mut t, 0.1);
    assert_eq!(bag(&g), [matches, malle]);
    assert_eq!(hand(&g), malle);
    assert_eq!(flag3(&g), 0);
    assert!(!shown(&g, "mlA1002002") && !shown(&g, "mlA4002002"));
    assert_eq!(zone_kind(&g, "POSE_MALLE"), 3, "ENABLEZONE=POSE_MALLE");
    write_ppm(&g, "trunk_in_hand.ppm");

    // The inventory: opening it empties the hand; the trunk taken back
    // from the bag, the right button above the bar closes it with the
    // trunk in the hand (@ 0x00428b80..0x00428b8f).
    g.right_click();
    t += 0.04;
    g.step(t);
    assert_eq!(hand(&g), 0);
    run(&mut g, &mut t, 0.3);
    point_slot(&mut g, 1);
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(hand(&g), malle);
    g.set_pointer(500, 300);
    t += 0.04;
    g.step(t);
    write_ppm(&g, "bag_trunk_in_hand.ppm");
    g.right_click();
    t += 0.04;
    g.step(t);
    assert!(!g.inventory_open());
    assert_eq!(hand(&g), malle);
    run(&mut g, &mut t, 0.5);

    // /005: the zone `POSE_MALLE` clicked with the trunk in the hand.
    click_zone(&mut g, &mut t, "POSE_MALLE");
    run(&mut g, &mut t, 0.2);
    assert_eq!(bag(&g), [matches], "SACADOS=NOT_MALLE");
    assert!(shown(&g, "mlA1002002"), "the trunk shown again");
    assert_eq!(zone_kind(&g, "POSE_MALLE"), 3 + 88888);
    assert_eq!(hand(&g), 0, "the trunk left the bag: the hand emptied");
    t += 0.04;
    g.step(t);
    write_ppm(&g, "trunk_back.ppm");
}

#[test]
fn compositions_and_the_big_view() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let matches = object(&g, "ALLUMETTES");
    let lamp = object(&g, "LANTERNE");
    let lit = object(&g, "LANTERNE_ALLUMEE");
    g.run_command("(SACADOS=LANTERNE)");
    assert_eq!(bag(&g), [matches, lamp]);
    g.right_click();
    run(&mut g, &mut t, 0.3);
    // The right button on an item: shown big; the left button ends it.
    point_slot(&mut g, 1);
    g.right_click();
    t += 0.04;
    g.step(t);
    assert_eq!(g.examined_2004(), Some(lamp));
    assert!(g.inventory_open());
    t += 0.04;
    g.step(t);
    write_ppm(&g, "big_lamp.ppm");
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(g.examined_2004(), None);
    // The matches in the hand, used on the lamp: `ALLUMETTES,LANTERNE,
    // ALLUMETTES,LANTERNE_ALLUMEE`.
    point_slot(&mut g, 0);
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(hand(&g), matches);
    point_slot(&mut g, 1);
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(bag(&g), [matches, lit]);
    assert_eq!(hand(&g), 0);
    // No composition: the hand stays.
    point_slot(&mut g, 0);
    g.click();
    t += 0.04;
    g.step(t);
    point_slot(&mut g, 1);
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(bag(&g), [matches, lit]);
    assert_eq!(hand(&g), matches);
}

#[test]
fn followers() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    // chapi015 / chapi016's `addsuiveur=MIRANDA`: `MIRANDA,0` of
    // `#PERSONNAGES#`: frame 0 disabled, 0 enabled (no parameter 2).
    g.run_command("(ADDSUIVEUR=MIRANDA)");
    let f = g.followers_2004().to_vec();
    assert_eq!(f.len(), 1);
    assert_eq!(
        (
            f[0].name.as_str(),
            f[0].frame_off,
            f[0].frame_on,
            f[0].enabled
        ),
        ("MIRANDA", 0, 0, false)
    );
    t += 0.04;
    g.step(t);
    write_ppm(&g, "follower.ppm");
    g.run_command("(ENABLESUIVEUR=MIRANDA)");
    assert!(g.followers_2004()[0].enabled);
    g.run_command("(KILLSUIVEUR=MIRANDA)");
    assert!(g.followers_2004().is_empty());
    g.run_command("(ADDSUIVEUR=MIRANDA)");
    g.run_command("(ADDSUIVEUR=MIRANDA)");
    assert_eq!(g.followers_2004().len(), 2);
    g.run_command("(CLEARSUIVEURS=0)");
    assert_eq!(g.followers_2004().len(), 2);
    g.run_command("(CLEARSUIVEURS=1)");
    assert!(g.followers_2004().is_empty());
}

/// The hover label (FUN_0042b170 → FUN_0042bed0): the pointer on the
/// matches' slot prints `A box of matches` (`Objects.txt`) in white at
/// (px + 50, py − 20) and up; on an empty slot, or with the matches in
/// the hand, no label.
#[test]
fn the_hover_label() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    g.right_click();
    run(&mut g, &mut t, 0.5);
    let white = |g: &Game| {
        let (w, h) = (g.width(), g.height());
        let lay = Layout::for_width(w);
        let (px, py) = lay.item_centre(0, h);
        let (x0, y0) = ((px + 50) as usize, (py - 60).max(0) as usize);
        let mut n = 0;
        for y in y0..(py as usize).min(h) {
            for x in x0..(x0 + 300).min(w) {
                let c = match g.surface() {
                    Surface::Rgb565(p) => u32::from(p[y * w + x] == 0xffff),
                    Surface::Xrgb8888(p) => u32::from(p[y * w + x] & 0xff_ffff == 0xff_ffff),
                };
                n += c;
            }
        }
        n
    };
    point_slot(&mut g, 3);
    t += 0.04;
    g.step(t);
    let none = white(&g);
    point_slot(&mut g, 0);
    t += 0.04;
    g.step(t);
    write_ppm(&g, "label.ppm");
    let label = white(&g);
    assert!(label > none + 50, "{none} {label}");
    // In the hand: no label.
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(hand(&g), object(&g, "ALLUMETTES"));
    t += 0.04;
    g.step(t);
    assert!(white(&g) < label, "the hand's object has no label");
}

/// `DISABLEWARP=1` / `=0` (FUN_00442030): every zone of the current view
/// disabled (type + 88888) / enabled again, each change a zone table row
/// (FUN_00422950: the same row updated).
#[test]
fn disable_warp() {
    let Some((mut g, _)) = cabin() else {
        return;
    };
    let kinds = |g: &Game| -> Vec<i32> {
        g.wam()
            .view(1)
            .unwrap()
            .zones
            .iter()
            .map(|z| z.kind)
            .collect()
    };
    let before = kinds(&g);
    let rows0 = g.zone_table_2004().len();
    g.run_command("(disablewarp=1)");
    let off = kinds(&g);
    assert!(off.iter().all(|&k| k >= 88888), "{off:?}");
    let added = before.iter().filter(|&&k| k < 88888).count();
    assert_eq!(g.zone_table_2004().len(), rows0 + added);
    g.run_command("(disablewarp=0)");
    let on = kinds(&g);
    assert!(on.iter().all(|&k| k < 88888), "{on:?}");
    assert_eq!(g.zone_table_2004().len(), rows0 + added);
    assert!(g.zone_table_2004()[rows0..]
        .iter()
        .all(|r| r.etat == 1 && r.vue == 1));
}
