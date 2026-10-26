//! The menus' slides, the widgets' tooltips and the bag's object names
//! against the original (`tests/golden/input_ui.txt`, its header there):
//! the menu frame FUN_00486eb0 run with each frame's action given and its
//! page calls recorded, the tooltip FUN_0046d750 on a widget, and the
//! loader FUN_00474810 on the installed `loc\Messages.txt` and on a
//! synthetic file.
//!
//! The slides are replayed on the engine's main menu
//! ([`Engine::menu_step_action`]): the golden's page calls are applied to
//! a copy of the page's widgets (moves, enables, shows, hides; a page
//! built), which must then be the engine's page, frame by frame, with the
//! state and the counter.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use atlantis2_data::messages::ObjectNames;
use atlantis2_data::save::{entry, SystemTime};
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::{PageKind, Rect};
use atlantis2_engine::{CursorBank, Engine, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/input_ui.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// The golden's sections whose header starts with `prefix`: the header's
/// rest and the rows.
fn sections(prefix: &str) -> Vec<(String, Vec<&'static str>)> {
    let mut out: Vec<(String, Vec<&str>)> = Vec::new();
    let mut on = false;
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(h) = line.strip_prefix("== ") {
            on = h.starts_with(prefix);
            if on {
                out.push((h[prefix.len()..].to_string(), Vec::new()));
            }
        } else if on {
            out.last_mut().unwrap().1.push(line);
        }
    }
    out
}

/// A widget as the golden's calls leave it: x, y, enabled, visible.
type Shadow = BTreeMap<String, (i32, i32, bool, bool)>;

fn shadow_of(e: &Engine) -> Shadow {
    let p = e.menu().unwrap().page.as_ref().unwrap();
    p.widgets
        .iter()
        .map(|w| (w.name.to_lowercase(), (w.x, w.y, w.enabled, w.visible)))
        .collect()
}

fn kind(e: &Engine) -> Option<PageKind> {
    e.menu().and_then(|m| m.kind())
}

/// FUN_00486eb0 frame by frame: the options, sound and disc panels'
/// slides (one frame after the last move changes the state), the save
/// question answered NO (every widget enabled again, the slide back) and
/// the click elsewhere.
#[test]
fn the_menu_slides_match_the_original() {
    let Some(_) = game() else { return };
    let cases = sections("slides: ");
    assert_eq!(cases.len(), 4);
    for (head, rows) in cases {
        let mut e = Engine::new_game(game().unwrap(), Variant::PII, Fpu::Chop, 640, 480).unwrap();
        e.end_video();
        e.open_main_menu();
        assert_eq!(kind(&e), Some(PageKind::Main));
        let mut shadow = shadow_of(&e);
        for row in rows {
            let (lhs, calls) = row.split_once(": ").unwrap();
            let act: i32 = lhs.split_whitespace().nth(2).unwrap().parse().unwrap();
            e.menu_step_action(act);
            let (st, calls) = calls.split_once(" | ").unwrap_or((calls, ""));
            let w: Vec<i32> = st
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            let m = e.menu().unwrap();
            assert_eq!((m.state, m.counter), (w[0], w[1]), "{head}: {row}");
            for call in calls.split("; ").filter(|c| !c.is_empty()) {
                let (name, args) = call.split_once('(').unwrap_or((call, ""));
                let args: Vec<&str> = args.trim_end_matches(')').split(',').collect();
                let target = args[0].to_lowercase();
                match name {
                    "move" => {
                        let s = shadow.get_mut(&target).expect(call);
                        s.0 += args[1].parse::<i32>().unwrap();
                        s.1 += args[2].parse::<i32>().unwrap();
                    }
                    "enable" => shadow.get_mut(&target).expect(call).2 = true,
                    "show" => shadow.get_mut(&target).expect(call).3 = true,
                    "hide" => shadow.get_mut(&target).expect(call).3 = false,
                    "disable_all" | "enable_all" => {
                        for s in shadow.values_mut() {
                            s.2 = name == "enable_all";
                        }
                    }
                    "text" => {}
                    "build_options" | "build_sound" => {
                        let want = if name == "build_options" {
                            PageKind::Options
                        } else {
                            PageKind::Sound
                        };
                        assert_eq!(kind(&e), Some(want), "{head}: {row}");
                        shadow = shadow_of(&e);
                    }
                    other => panic!("{head}: {row}: call {other} not expected"),
                }
            }
            assert_eq!(shadow_of(&e), shadow, "{head}: {row}");
        }
    }
}

/// FUN_0046d750 through the widget's draw: the count restarts when the
/// mouse enters the box of an enabled widget; the in-the-box flag follows
/// the mouse while the widget is disabled, so a widget enabled with the
/// mouse already in its box keeps its count (the tooltip at once when it
/// is old); nothing while hidden.
#[test]
fn the_tooltips_count_as_the_originals() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.end_video();
    e.open_main_menu();
    let base = {
        let p = e.menu().unwrap().page.as_ref().unwrap();
        p.widgets
            .iter()
            .find(|w| !w.text.is_empty())
            .expect("a widget with a tooltip")
            .clone()
    };
    let cases = sections("tooltip: ");
    assert_eq!(cases.len(), 5);
    for (head, rows) in cases {
        let mut w = base.clone();
        w.hit = Rect {
            left: 100,
            top: 100,
            right: 200,
            bottom: 140,
        };
        for row in rows {
            // t<T> ptr (<x>,<y>) enabled <e> visible <v>: drawn <d> start <s> inside <i>
            let n: Vec<i64> = row
                .split(|c: char| !c.is_ascii_digit())
                .filter(|t| !t.is_empty())
                .map(|t| t.parse().unwrap())
                .collect();
            let (t, x, y) = (n[0] as u32, n[1] as i32, n[2] as i32);
            w.enabled = n[3] != 0;
            w.visible = n[4] != 0;
            w.tooltip_step(x, y, t);
            let drawn = w.visible && w.tip_shown(t);
            assert_eq!(drawn, n[5] > 0, "{head}: {row}");
            assert_eq!(w.tip_state(), (n[6] as u32, n[7] != 0), "{head}: {row}");
        }
    }
}

fn md5_12(b: &[u8]) -> String {
    Md5::digest(b)
        .iter()
        .take(6)
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// The bag's names as the original's loader leaves them: the installed
/// file's keys and names (lengths, blanks, MD5s, in the map's order: two
/// names keep a trailing blank) and a synthetic file's.
#[test]
fn the_object_names_are_the_originals() {
    let Some(files) = game() else { return };
    let data = files
        .read(Folder::Loc, "Messages.txt")
        .expect("Messages.txt");
    let (head, rows) = sections("names: ").remove(0);
    let md5: String = Md5::digest(&data)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    assert!(head.contains(&format!("md5 {md5}")), "{head}");
    let names = ObjectNames::parse(&data);
    let mut ents: Vec<(&[u8], &[u8])> = names.entries().collect();
    ents.sort();
    assert!(head.contains(&format!(", {} keys,", ents.len())), "{head}");
    let got: Vec<String> = ents
        .iter()
        .enumerate()
        .map(|(i, (k, v))| {
            let trimmed = |b: &[u8]| {
                let blank = |c: &u8| *c == b' ' || *c == b'\t';
                let t = b.iter().rev().take_while(|c| blank(c)).count();
                let l = b.iter().take_while(|c| blank(c)).count();
                (l, t)
            };
            let (lead, trail) = trimmed(v);
            format!(
                "key {i} len {} md5 {} | name len {} lead {lead} trail {trail} cr {} md5 {}",
                k.len(),
                md5_12(k),
                v.len(),
                v.iter().filter(|&&c| c == b'\r').count(),
                md5_12(v)
            )
        })
        .collect();
    assert_eq!(got, rows);

    let (head, rows) = sections("names, synthetic file ").remove(0);
    let hex = head
        .split("hex ")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let names = ObjectNames::parse(&bytes);
    let mut ents: Vec<(&[u8], &[u8])> = names.entries().collect();
    ents.sort();
    let hexs = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let got: Vec<(String, String)> = ents.iter().map(|(k, v)| (hexs(k), hexs(v))).collect();
    let want: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            let (k, v) = r.split_once(" | name ").unwrap();
            let k = k.strip_prefix("key ").unwrap().split(' ').next().unwrap();
            let v = v.split(' ').next().unwrap();
            (k.to_string(), v.to_string())
        })
        .collect();
    assert_eq!(got, want);
}

/// A new game as the player menu starts it (the scripts loaded), its
/// opening film ended; the saves in `data`.
fn booted(files: A2Files, data: Option<PathBuf>) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.set_data_dir(data);
    e.start_new_game().unwrap();
    e.end_video();
    e
}

/// A pass of the engine at `n` × [`PASS`].
fn pass(e: &mut Engine, n: &mut u32) {
    *n += 1;
    e.set_clock(f64::from(*n) * PASS);
    e.pass(None);
}

/// The main, CD and player menus' start sets the game's cursor pair to
/// the menus' pointer, frame 0 (FUN_0047bef0(curseur, 0) @ 0x004869d5),
/// and their stop (FUN_00489040) leaves it: a save from the menus records
/// frame 0 (FUN_0047bf20 @ the header's refresh); with the bag bar open
/// across the menu, the bar's pointer is the menus' until the bar closes
/// and the panorama's cursor choice runs again.
#[test]
fn the_menus_leave_their_pointer_in_the_cursor_pair() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.end_video();
    let mut n = 0;
    for _ in 0..8 {
        pass(&mut e, &mut n);
    }
    e.right_click();
    pass(&mut e, &mut n);
    assert!(e.bar.open);
    assert_eq!(
        (e.cursor.bank, e.cursor.shown),
        (CursorBank::Interface, Some(40))
    );
    e.escape();
    assert_eq!(e.loop_type, LoopType::Menu);
    assert_eq!((e.cursor.bank, e.cursor.shown), (CursorBank::Menu, Some(0)));
    assert_eq!(e.save_data().i32(entry::CURSOR), 0);
    e.escape();
    assert_eq!(e.loop_type, LoopType::Warp);
    pass(&mut e, &mut n);
    assert!(e.bar.open);
    assert_eq!((e.cursor.bank, e.cursor.shown), (CursorBank::Menu, Some(0)));
    assert!(e.ui_draws.iter().any(|d| matches!(
        d,
        UiDraw::Sprite {
            bank: UiBank::Menu,
            frame: 0,
            ..
        }
    )));
    // The bar closed, the panorama chooses its cursor again.
    e.right_click();
    pass(&mut e, &mut n);
    pass(&mut e, &mut n);
    assert_ne!(e.cursor.bank, CursorBank::Menu);
}

/// The load's puzzle reset (FUN_0049bf70) runs the turtle ring's, the
/// canoe's and the souls' quits too: the modes 2 and 3 go back to 0
/// before the save's entry 26 is copied, and the canoe's quit, which has
/// no test of its own, empties the cursor pair (FUN_0047bef0(0, 0)
/// @ 0x0049a575), so the cursor frame's restore finds no bank and the
/// pair stays empty until the next cursor choice.
#[test]
fn a_load_runs_every_quit_of_the_reset() {
    let Some(files) = game() else { return };
    let mut e = booted(files, None);
    let mut n = 0;
    for _ in 0..8 {
        pass(&mut e, &mut n);
    }
    for mode in [2, 3] {
        e.set_puzzle_mode(mode);
        e.puzzles_reset();
        assert_eq!(e.puzzle_mode(), 0, "mode {mode}");
    }
    e.set_puzzle_mode(5);
    e.puzzles_reset();
    assert_eq!(e.puzzle_mode(), 5, "the reset keeps the other modes");
    e.set_puzzle_mode(0);
    e.escape();
    assert_eq!(e.cursor.shown, Some(0));
    let d = e.save_data();
    e.escape();
    e.load_data(&d).unwrap();
    assert_eq!(e.cursor.shown, None);
    pass(&mut e, &mut n);
}

/// Save entry 15 (@ 0x006d84f4) is the dowsing rod's anchor y: 480 from
/// the start-up's static initialiser (@ 0x0049c6d0), then the logical
/// height at every call of the rod's draw FUN_0049c6e0, shown or not: 600
/// in the panorama of the 800 x 600 mode.
#[test]
fn the_rods_anchor_is_saved() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    assert_eq!(e.save_data().i32(15), 480);
    e.end_video();
    let mut n = 0;
    pass(&mut e, &mut n);
    assert_eq!(e.save_data().i32(15), 480);
    e.set_screen_800(true);
    pass(&mut e, &mut n);
    pass(&mut e, &mut n);
    assert_eq!(e.save_data().i32(15), 600);
}

static DATES: AtomicUsize = AtomicUsize::new(0);

fn probe_date(_: &SystemTime) -> Vec<u8> {
    DATES.fetch_add(1, Ordering::Relaxed);
    b"01.02.03".to_vec()
}

/// The load page writes each save's time with the front end's formats
/// (the original's `GetDateFormatA` / `GetTimeFormatA` of the user's
/// locale, FUN_0046f830): the engine's `date_text`.
#[test]
fn the_load_page_dates_come_from_the_front_end() {
    let Some(files) = game() else { return };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a2-input-ui-dates");
    let _ = std::fs::remove_dir_all(&dir);
    let mut e = booted(files, Some(dir.clone()));
    // The music copy of a new data folder's first start, first.
    let mut n = 0;
    while e.copying() {
        pass(&mut e, &mut n);
    }
    assert!(e.save_slot(0));
    e.date_text = probe_date;
    e.open_menu_page(PageKind::Load);
    assert_eq!(kind(&e), Some(PageKind::Load));
    let mut screen = vec![0u16; 640 * 480];
    pass(&mut e, &mut n);
    e.pass(Some(&mut screen));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(DATES.load(Ordering::Relaxed) >= 1);
}
