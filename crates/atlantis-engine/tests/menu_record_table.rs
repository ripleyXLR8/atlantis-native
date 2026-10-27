//! The menus' record table, one for the session (15 blocks of a count byte
//! and the build's records per menu, with no bound on a menu's count): the
//! port's `MenuRecordTable`, `walk_menu_text_in`, `menu_walk_hit` and the
//! LOAD picker's knob record, against the original's walk, sprite records,
//! hit test and LOAD picker entry stores run under emulation over the
//! build's menu files (golden files `ui_menu_record_table_*.txt`: the
//! records each menu writes, a SUREMENU / SONMENU / SUREMENU sequence, the
//! hit test of SUREMENU before and after its walk, two LOAD picker visits).
//! The golden values are numbers only.
//!
//! Needs the game folders: `ATLANTIS_DATA` (the 2001 English build),
//! `ATLANTIS_DATA_1997` (the May 1997 English build) and
//! `ATLANTIS_DATA_FR1997_MAY` (the May 1997 French build); without them the
//! tests are skipped.

use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::enhancements::BugFixes;
use atlantis_engine::ui::layout::{
    load_picker_knob_record, menu_walk_hit, walk_menu_text_in, LoadPickerScroll, MenuRecord,
    MenuRecordTable, MenuRules, MenuWalk,
};

const MENUS: [&str; MenuRecordTable::MENUS] = [
    "MAINMENU", "LOADMENU", "OPTMENU", "NEWMENU", "SELEMENU", "SONMENU", "SUREMENU", "CD1MENU",
    "CD2MENU", "CD3MENU", "CD4MENU", "WAIT", "ERROR1", "ERROR2", "ERROR3",
];

/// SONMENU.TXT's knob records' x, the sound menu's first knob positions.
const SON_KNOB_X: [i16; 4] = [460, 465, 470, 490];

fn game(var: &str, test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

struct Menus {
    texts: Vec<Vec<u8>>,
    rules: MenuRules,
}

impl Menus {
    fn walk(&self, m: usize, table: &mut MenuRecordTable, walk: &mut MenuWalk) {
        let t = &self.texts[m];
        walk_menu_text_in(t, m as i32, -1, 0, 0, self.rules, walk, Some(table));
    }
}

fn recs(r: &[MenuRecord]) -> String {
    r.iter()
        .map(|r| format!("({},{},{},{})", r.x, r.y, r.w, r.h))
        .collect::<Vec<_>>()
        .join(" ")
}

fn blits(w: &MenuWalk) -> String {
    w.ops
        .iter()
        .filter(|o| o.sprite && !o.alt_bank)
        .map(|o| format!("({},{},{})", o.x, o.y, o.index))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The golden's lines, as the port gives them.
fn port_lines(menus: &Menus, n: usize, knob_at_bar: bool, clamp: bool) -> Vec<String> {
    let mut out = Vec::new();
    // counts: each menu walked on a zeroed table.
    for (m, name) in MENUS.iter().enumerate() {
        let mut table = MenuRecordTable::new(n);
        menus.walk(m, &mut table, &mut MenuWalk::default());
        let c = usize::from(table.count(m));
        let mut line = format!("count {name:<8} menu {m:>2} records {c:>2}");
        if c > n {
            line += &format!("  overflow {} into menu {}", c - n, m + 1);
        }
        out.push(line);
    }
    // sequence: SUREMENU, SONMENU (its knob records 5..8 at the knobs' x,
    // as the sound menu writes them), SUREMENU.
    let mut table = MenuRecordTable::new(n);
    for (step, m) in [6, 5, 6].into_iter().enumerate() {
        let mut walk = MenuWalk::default();
        if m == 5 {
            for (t, &x) in SON_KNOB_X.iter().enumerate() {
                let mut r = table.record(5, 5 + t);
                r.x = x;
                table.set_record(5, 5 + t, r);
            }
        }
        menus.walk(m, &mut table, &mut walk);
        out.push(format!(
            "step {} {} count {}",
            step + 1,
            MENUS[m],
            table.count(m)
        ));
        if m == 5 {
            let from = n.min(walk.records.len());
            out.push(format!(
                "SONMENU records {n}.. {}",
                recs(&walk.records[from..])
            ));
        }
        let block6: Vec<MenuRecord> = (0..5).map(|k| table.record(6, k)).collect();
        out.push(format!(
            "menu 6 block: count {} records 0-4 {}",
            table.count(6),
            recs(&block6)
        ));
        out.push(format!("SPRMENU blits (x, y, index): {}", blits(&walk)));
    }
    // stale: SUREMENU's hit test before its walk and after it, on a 4-px
    // grid, after SUREMENU and SONMENU (menus 7-14 never walked), then
    // after every menu.
    for every in [false, true] {
        let mut table = MenuRecordTable::new(n);
        let mut prep: Vec<usize> = if every {
            (0..MenuRecordTable::MENUS).collect()
        } else {
            Vec::new()
        };
        prep.extend([6, 5]);
        for m in prep {
            menus.walk(m, &mut table, &mut MenuWalk::default());
        }
        let grid = |table: &MenuRecordTable| -> Vec<u32> {
            let r = table.records(6);
            (0..480)
                .step_by(4)
                .flat_map(|y| (0..640).step_by(4).map(move |x| (x, y)))
                .map(|(x, y)| menu_walk_hit(&r, x, y).map_or(255, |h| h as u32))
                .collect()
        };
        let count_before = table.count(6);
        let before = grid(&table);
        menus.walk(6, &mut table, &mut MenuWalk::default());
        let after = grid(&table);
        let mut keys: Vec<(u32, u32, u32)> = Vec::new();
        for (b, a) in before.into_iter().zip(after) {
            match keys.iter_mut().find(|k| (k.0, k.1) == (b, a)) {
                Some(k) => k.2 += 1,
                None => keys.push((b, a, 1)),
            }
        }
        keys.sort_unstable();
        out.push(format!(
            "stale {}: count before the walk {}, after {}",
            if every {
                "after every menu"
            } else {
                "menus 7-14 empty"
            },
            count_before,
            table.count(6)
        ));
        for (b, a, points) in keys {
            let differs = if b == a { "" } else { "  differs" };
            out.push(format!(
                "before {b:>3} after {a:>3} points {points}{differs}"
            ));
        }
    }
    // loadknob: two LOAD picker visits, 5 then 20 files.
    let mut table = MenuRecordTable::new(n);
    for (v, files) in [5, 20].into_iter().enumerate() {
        let sb = LoadPickerScroll::new(files, knob_at_bar, clamp);
        let r = load_picker_knob_record(table.record(1, 4), &sb);
        table.set_record(1, 4, r);
        let mut walk = MenuWalk::default();
        menus.walk(1, &mut table, &mut walk);
        let r4 = recs(&walk.records[4..5]);
        out.push(if v == 0 {
            format!(
                "visit 1 (11 files or fewer: x 0xffce, y {}): record 4 {r4} blits {}",
                sb.knob_y,
                blits(&walk)
            )
        } else {
            format!(
                "visit 2 (more than 11 files: y {} only): record 4 {r4} blits {}",
                sb.knob_y,
                blits(&walk)
            )
        });
    }
    out
}

fn check(var: &str, test: &str, build: &str, golden: &str) {
    let Some(g) = game(var, test) else { return };
    let b = g.exe.build;
    assert_eq!(b.name, build);
    let menus = Menus {
        texts: MENUS
            .iter()
            .map(|m| g.big.read(&format!("SPRLIST\\{m}.TXT")).unwrap())
            .collect(),
        rules: MenuRules::of_build(b, &BugFixes::NONE),
    };
    let want: Vec<&str> = golden
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(str::trim)
        .collect();
    let n = usize::from(b.menu_records_per_menu);
    let got = port_lines(&menus, n, b.load_knob_starts_at_bar, b.load_picker_clamp);
    for (w, g) in want.iter().zip(&got) {
        assert_eq!(g.trim(), *w, "{build}");
    }
    assert_eq!(got.len(), want.len(), "{build}");
}

#[test]
fn record_table_2001_build() {
    check(
        "ATLANTIS_DATA",
        "record_table_2001_build",
        "atlantis-cd-en-714240",
        include_str!("golden/ui_menu_record_table_en714240.txt"),
    );
}

#[test]
fn record_table_english_may_1997_build() {
    check(
        "ATLANTIS_DATA_1997",
        "record_table_english_may_1997_build",
        "atlantis-cd-en-688128",
        include_str!("golden/ui_menu_record_table_en688128.txt"),
    );
}

#[test]
fn record_table_french_may_1997_build() {
    check(
        "ATLANTIS_DATA_FR1997_MAY",
        "record_table_french_may_1997_build",
        "atlantis-cd-fr-688640",
        include_str!("golden/ui_menu_record_table_fr688640.txt"),
    );
}

/// Without the table (the menus' records fix), each menu walks its own
/// records: SUREMENU after the sound menu keeps its ornament at the text's
/// place, and the LOAD knob of a second visit with more files is drawn
/// again at the file's x.
#[test]
fn record_table_left_out_with_the_fix() {
    let Some(g) = game("ATLANTIS_DATA_1997", "record_table_left_out_with_the_fix") else {
        return;
    };
    let rules = MenuRules::of_build(g.exe.build, &BugFixes::ALL);
    let text = |m: &str| g.big.read(&format!("SPRLIST\\{m}.TXT")).unwrap();
    let mut son = MenuWalk::default();
    walk_menu_text_in(&text("SONMENU"), 5, -1, 0, 0, rules, &mut son, None);
    assert_eq!(son.records.len(), 18);
    let mut sure = MenuWalk::default();
    walk_menu_text_in(&text("SUREMENU"), 6, -1, 0, 0, rules, &mut sure, None);
    assert_eq!(blits(&sure), "(130,220,5) (500,260,6)");
    let load = text("LOADMENU");
    let knob = |walk: &MenuWalk| walk.ops.iter().find(|o| o.index == 13).map(|o| o.x);
    let mut first = MenuWalk::default();
    let sb = LoadPickerScroll::new(5, false, false);
    let r = load_picker_knob_record(MenuRecord::default(), &sb);
    first.records = vec![MenuRecord::default(); 4];
    first.records.push(r);
    walk_menu_text_in(&load, 1, -1, 0, 0, rules, &mut first, None);
    assert_eq!(knob(&first), Some(-50));
    let mut second = MenuWalk::default();
    walk_menu_text_in(&load, 1, -1, 0, 0, rules, &mut second, None);
    assert_eq!(knob(&second), Some(600));
}

/// The LOAD knob, record 4 (`/spr=13`) of LOADMENU, drawn at the x / y the
/// picker stores there: the original walk run under emulation with the
/// record preset (nothing; y 0x7b; x 0xffce and y 0x7b; y 190) blits it at
/// (600, 130), (600, 123), (-50, 123), (600, 190).
#[test]
fn load_knob_drawn_from_its_stored_record() {
    let Some(g) = game("ATLANTIS_DATA", "load_knob_drawn_from_its_stored_record") else {
        return;
    };
    let text = g.big.read("SPRLIST\\LOADMENU.TXT").unwrap();
    for (x, y, want) in [
        (0, 0, (600, 130)),
        (0, 0x7b, (600, 123)),
        (-50, 0x7b, (-50, 123)),
        (0, 190, (600, 190)),
    ] {
        let mut walk = MenuWalk {
            records: vec![MenuRecord::default(); 5],
            ops: Vec::new(),
        };
        walk.records[4] = MenuRecord { x, y, w: 0, h: 0 };
        walk_menu_text_in(&text, 1, -1, 0, 0, MenuRules::default(), &mut walk, None);
        let got: Vec<(i16, i16)> = walk
            .ops
            .iter()
            .filter(|o| o.sprite && o.index == 13)
            .map(|o| (o.x, o.y))
            .collect();
        assert_eq!(got, [want], "preset ({x}, {y})");
    }
}
