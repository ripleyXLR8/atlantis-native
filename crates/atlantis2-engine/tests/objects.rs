//! The objects, the bag and the object in hand against the original
//! (`tests/golden/objects.txt`, its header there), then the bag bar's
//! frame by the rules read in the original's code (no golden of the bar
//! exists yet).
//!
//! The engine runs the scripts as a new game does (Tibet); the golden's
//! steps are replayed in its order: the worlds' banks, the takes of the
//! three boats in views 5, 8 and 9, the bag and hand keys of the scripts,
//! the visible counts, the centre draw of hand and cursor, the scroll.
//! Rows of the original's view-10 hook (the mandala puzzle, not ported)
//! and the stubbed calls are skipped; a `changes` row is checked on the
//! state after the step (each change listed holds).
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped.

use std::path::PathBuf;

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::A2Files;
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::engine::objects_bank_name;
use atlantis2_engine::script::{Host, Script};
use atlantis2_engine::{CentreDraw, CursorBank, Engine, Fpu, Variant};

const GOLDEN: &str = include_str!("golden/objects.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// The golden's `{S:I}` tokens replaced by the names.
fn fill(names: &DefNames, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let j = rest[i..].find('}').unwrap() + i;
        let f: Vec<&str> = rest[i + 1..j].split(':').collect();
        let name = names
            .name(f[0].parse().unwrap(), f[1].parse().unwrap())
            .unwrap();
        out.extend(name.iter().map(|&c| char::from(c)));
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out
}

/// A host for conditions evaluated on a copy of the scripts.
struct NoHost;

impl Host for NoHost {
    fn read_chapter(&mut self, _chapter: i32) -> Option<Vec<u8>> {
        None
    }
}

/// The game and the golden's rows, walked in order.
struct Walk {
    e: Engine,
    names: DefNames,
    rows: Vec<String>,
    at: usize,
    failures: Vec<String>,
}

impl Walk {
    fn s(&self) -> &Script {
        self.e.script_state().unwrap()
    }

    fn s_mut(&mut self) -> &mut Script {
        self.e.script_state_mut().unwrap()
    }

    fn fill(&self, s: &str) -> String {
        fill(&self.names, s)
    }

    /// The next row, past those of the puzzle hook and the stubbed calls.
    fn next(&mut self) -> String {
        loop {
            let r = self.rows.get(self.at).cloned().unwrap_or_default();
            self.at += 1;
            if !(r.starts_with("hook ") || r.contains(" calls ")) {
                return r;
            }
        }
    }

    fn peek(&mut self) -> String {
        let at = self.at;
        let r = self.next();
        self.at = at;
        r
    }

    fn expect(&mut self, produced: &str) {
        let produced = self.fill(produced);
        let want = self.next();
        if want != produced {
            self.failures.push(format!(
                "row {}:\n  want {want}\n  got  {produced}",
                self.at
            ));
        }
    }

    fn objects_bank(&self) -> String {
        let monde = self.s().var("MONDE").unwrap();
        objects_bank_name(monde)
            .map_or("none", |n| n.trim_end_matches(".spr"))
            .to_string()
    }

    fn cursor_bank(&self, b: CursorBank) -> String {
        match b {
            CursorBank::Interface => "interfac".into(),
            CursorBank::Objects => self.objects_bank(),
        }
    }

    /// The hand pair, the cursor pair and `OBJETENMAIN`, as the engine
    /// holds them (the hand's frame without a bank is the scripts').
    fn pairs(&self) -> String {
        let s = self.s();
        let hand = match self.e.hand {
            Some((_, f)) => format!("{},{f}", self.objects_bank()),
            None => format!("none,{}", s.cursor_sprite),
        };
        let cursor = match self.e.cursor.shown {
            Some(f) => format!("{},{f}", self.cursor_bank(self.e.cursor.bank)),
            None => "none,-1".into(),
        };
        let held = self.e.vars.objet_en_main;
        assert_eq!(held, s.var("OBJETENMAIN").unwrap(), "OBJETENMAIN");
        format!("hand={hand};cursor={cursor};OBJETENMAIN={held}")
    }

    fn bag(&self) -> String {
        let b: Vec<String> = self
            .e
            .bag_entries()
            .iter()
            .map(|e| format!("{}/spr{}/w{}", e.id, e.sprite, e.world))
            .collect();
        if b.is_empty() {
            "-".into()
        } else {
            b.join(";")
        }
    }

    fn shown_table(&self) -> String {
        let v: Vec<String> = self
            .s()
            .shown
            .iter()
            .map(|r| format!("{}/{}/{}", r.id, u8::from(r.shown), r.kind))
            .collect();
        v.join(";")
    }

    /// The next row is `<prefix> changes <list>`: each change holds now.
    fn changes(&mut self, prefix: &str) {
        let prefix = self.fill(prefix);
        let row = self.next();
        let Some(list) = row.strip_prefix(&format!("{prefix} changes ")) else {
            self.failures
                .push(format!("row {}: {row} is not {prefix} changes", self.at));
            return;
        };
        if list == "-" {
            return;
        }
        // Items are `key=value`; a part without `=` continues the list
        // value before it (the bag and the show table).
        let mut items: Vec<(String, String)> = Vec::new();
        for part in list.split(';') {
            match part.split_once('=') {
                Some((k, v)) if !k.contains('/') => items.push((k.into(), v.into())),
                _ => {
                    let last = items.last_mut().unwrap();
                    last.1 = format!("{};{part}", last.1);
                }
            }
        }
        for (k, v) in items {
            let s = self.s();
            let have = if let Some(i) = k.strip_prefix("var[").and_then(|k| k.strip_suffix(']')) {
                s.vars[i.parse::<usize>().unwrap()].to_string()
            } else {
                match k.as_str() {
                    "bag" => self.bag(),
                    // The table also holds the rows of the new game's
                    // `/INIT`, before those of the producer's steps.
                    "vis" => {
                        let t = self.shown_table();
                        if t.ends_with(&v) {
                            v.clone()
                        } else {
                            t
                        }
                    }
                    // The producer's names: cursor_spr / cursor_bank are
                    // the hand's frame and bank, hand_flag / hand_x the
                    // cursor's bank and frame.
                    "cursor_spr" => s.cursor_sprite.to_string(),
                    "cursor_bank" if s.cursor_objects => self.objects_bank(),
                    "cursor_bank" => "none".into(),
                    "hand_flag" => match (s.hand_drawn, self.e.cursor.shown) {
                        (true, Some(_)) => self.cursor_bank(self.e.cursor.bank),
                        (false, None) => "none".into(),
                        _ => "mismatch".into(),
                    },
                    "hand_x" => {
                        let engine = self.e.cursor.shown.unwrap_or(-1);
                        if engine == s.hand_x {
                            engine.to_string()
                        } else {
                            format!("engine {engine} scripts {}", s.hand_x)
                        }
                    }
                    _ => format!("unknown key {k}"),
                }
            };
            if have != v {
                self.failures
                    .push(format!("{prefix}: {k} = {have}, the original {v}"));
            }
        }
    }

    /// `/set(KEY=value)` as the scripts' handler runs it.
    fn set(&mut self, line: &str) {
        let line = self.fill(line);
        let cmd = line.strip_prefix("/set").unwrap_or(&line);
        self.e.run_command(cmd.as_bytes());
    }

    /// The next row is `cond <condition> <0|1>`: the condition evaluated
    /// on a copy of the scripts, `CLICZONE` set to the zone it names.
    fn cond(&mut self) {
        let row = self.peek();
        let (c, _) = row
            .strip_prefix("cond ")
            .and_then(|r| r.rsplit_once(' '))
            .unwrap();
        let mut s = self.s().clone();
        if let Some(z) = c
            .strip_prefix("(cliczone=")
            .and_then(|r| r.split(')').next())
        {
            let zone = self.names.id(defnames::ZONES_CLIC, z.as_bytes()).unwrap();
            s.set_var("CLICZONE", zone as i32);
        }
        s.set_chapter_text(format!("/con{c}\r\n").as_bytes());
        let ok = s.eval_condition(&mut NoHost, 4).is_some();
        let c = c.to_string();
        self.expect(&format!("cond {c} {}", u8::from(ok)));
    }

    /// The `<prefix> /set(...) changes` rows that follow: each line run,
    /// its changes checked.
    fn set_rows(&mut self, prefix: &str) {
        loop {
            let row = self.peek();
            let Some(rest) = row.strip_prefix(&format!("{prefix} /set(")) else {
                return;
            };
            let line = format!("/set({}", rest.split(" changes ").next().unwrap());
            self.set(&line);
            self.changes(&format!("{prefix} {line}"));
        }
    }

    fn set_var(&mut self, name: &str, v: i32) {
        self.s_mut().set_var(name, v);
    }

    fn warnings(&self) -> usize {
        self.s().warnings.len()
    }

    /// The warnings since `from`, as the producer prints them.
    fn errors(&self, from: usize) -> String {
        let w: Vec<String> = self.s().warnings[from..]
            .iter()
            .map(|w| format!("warning:{}", w.replace(' ', "")))
            .collect();
        if w.is_empty() {
            "-".into()
        } else {
            w.join(";")
        }
    }

    fn slot_of(&self, id: i32) -> usize {
        self.e
            .anim
            .slots
            .iter()
            .position(|s| s.id == id && s.kind == 2 && s.click == 1)
            .unwrap_or_else(|| panic!("no slot for object {id}"))
    }

    fn reset(&mut self) {
        self.e.reset_view();
    }
}

/// A new game, the opening film ended: Tibet, chapter 0's `/INIT` done.
fn new_game(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    e
}

#[test]
fn objects_against_the_original() {
    let Some(files) = game() else { return };
    let names = DefNames::parse(
        &files
            .read(atlantis2_data::Folder::Scenar, "defnames.con")
            .unwrap(),
    )
    .unwrap();
    let rows: Vec<String> = GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| fill(&names, l))
        .collect();
    let e = new_game(files);
    let mut w = Walk {
        e,
        names,
        rows,
        at: 0,
        failures: Vec::new(),
    };

    // 1. The object table: the sprite of each object.
    let count = w.names.len(defnames::OBJETS);
    for i in 0..count {
        let sprite = w.names.param_of(defnames::OBJETS, i);
        w.expect(&format!("obj {i} sprite {sprite}"));
    }
    w.expect(&format!("obj_count {count}"));

    // 2. The world change: its objects bank; the hand pair emptied (here
    // after an object was put in it), `OBJETENMAIN` kept.
    let crayon = "{1:6}";
    for m in 0..8 {
        w.set(&format!("/set(objetenmain={crayon})"));
        w.set_var("OBJETENMAIN", 0);
        w.set(&format!("/set(monde={m})"));
        let bank = objects_bank_name(m).map_or("none", |n| n.trim_end_matches(".spr"));
        w.expect(&format!("monde {m} objbank {bank}"));
        let p = w.pairs();
        w.expect(&format!("monde {m} after {p}"));
    }
    w.set("/set(monde=5)");

    // 3. The takes of the three boats (the producer's bag cells: 8).
    w.e.bar.slots = 8;
    for (view, id) in [(5u32, 61), (8, 53), (9, 57)] {
        w.e.enter_at(view, 0.0, 0.0).unwrap();
        let n = w.slot_of(id);
        let before = w.warnings();
        w.e.take_slot(n);
        let name = format!("{{1:{id}}}");
        let sprite = w.e.anim.slots[n].object_sprite;
        w.expect(&format!("take v{view} {name} slot_sprite {sprite}"));
        w.changes(&format!("take v{view} {name}"));
        let p = w.pairs();
        w.expect(&format!("take v{view} {name} pairs {p}"));
        let v = w.e.bar.visible;
        w.expect(&format!("take v{view} {name} visible {v}"));
        let c = w.s().var("CLICOBJET").unwrap();
        w.expect(&format!("take v{view} {name} CLICOBJET {c}"));
        let slot = &w.e.anim.slots[n];
        let (frame, active) = (slot.frame, u8::from(slot.active));
        let errors = w.errors(before);
        w.expect(&format!(
            "take v{view} {name} slot +0x10 {frame} +0x30 {active} errors {errors}"
        ));
        w.reset();
        let p = w.pairs();
        w.expect(&format!("reset v{view} after_take pairs {p}"));
    }
    // The same click with the bar open: nothing.
    w.e.enter_at(5, 0.0, 0.0).unwrap();
    let n = w.slot_of(61);
    w.e.anim.slots[n].active = true;
    w.e.bar.open = true;
    let before = (w.bag(), w.pairs(), w.s().vars.clone());
    w.e.take_slot(n);
    let same = before == (w.bag(), w.pairs(), w.s().vars.clone());
    w.expect(if same {
        "take bag_open changes -"
    } else {
        "take bag_open changed"
    });
    w.e.bar.open = false;
    // An object already in the bag, its slot shown again.
    w.reset();
    let before = w.warnings();
    w.e.take_slot(n);
    w.changes("take again_shown");
    let errors = w.errors(before);
    let active = u8::from(w.e.anim.slots[n].active);
    w.expect(&format!(
        "take again_shown errors {errors} slot +0x30 after {active}"
    ));
    w.reset();
    // Click kinds 3 and 4 on an object not in the bag.
    for ck in [3, 4] {
        let slot = &mut w.e.anim.slots[n];
        (slot.kind, slot.id, slot.click, slot.active) = (2, 6, ck, true);
        w.e.take_slot(n);
        w.changes(&format!("take click{ck} {crayon}"));
        let (o, p) = (
            w.s().var("CLICOBJET").unwrap(),
            w.s().var("CLICPERSO").unwrap(),
        );
        let (bag, pairs) = (w.bag(), w.pairs());
        w.expect(&format!(
            "take click{ck} {crayon} vars ClicObjet={o} ClicPerso={p} bag {bag} pairs {pairs}"
        ));
        w.set_var("CLICOBJET", 0);
        w.set_var("CLICPERSO", 0);
        w.e.vars.clicobjet = 0;
        w.e.vars.clicperso = 0;
    }

    // 4. Chapter 75's bag conditions.
    for _ in 0..4 {
        w.cond();
    }

    // 5. Two boats placed in view 10 (the hook's rows skipped), one on a
    // wrong zone.
    for boat in [53, 57, 57] {
        w.set(&format!("/set(objetenmain={{1:{boat}}})"));
        let p = w.pairs();
        w.expect(&format!("hand {{1:{boat}}} pairs {p}"));
        w.cond();
        w.set_rows("set");
        let zone = w.peek().split(' ').nth(2).unwrap().to_string();
        let (bag, p) = (w.bag(), w.pairs());
        w.expect(&format!("after {{1:{boat}}} {zone} bag {bag}"));
        w.expect(&format!("after {{1:{boat}}} {zone} pairs {p}"));
        w.reset();
    }

    // 6. A world's mandala brought back in chapter 16's order.
    w.set_var("MONDE", 1);
    w.set_rows("return");
    let p = w.pairs();
    w.expect(&format!("return pairs {p}"));
    w.reset();
    let p = w.pairs();
    w.expect(&format!("return after_arrival_reset pairs {p}"));
    let b = w.bag();
    w.expect(&format!("return bag {b}"));
    // An object in hand that is not in the bag.
    w.set(&format!("/set(objetenmain={crayon})"));
    let (b, p) = (w.bag(), w.pairs());
    w.expect(&format!(
        "notinbag /set(objetenmain={crayon}) bag {b} pairs {p}"
    ));
    w.set("/set(objetenmain=0)");

    // 7. The shaman's gift, the entries shown in each world.
    w.set_rows("gift");
    let (b, p) = (w.bag(), w.pairs());
    w.expect(&format!("gift bag {b}"));
    w.expect(&format!("gift pairs {p}"));
    for m in 1..=6 {
        w.set_var("MONDE", m);
        let v = w.s().visible_objects();
        w.expect(&format!("visible monde {m} {v}"));
    }
    w.set_var("MONDE", 5);
    // The balls become crystals.
    w.set_rows("crystal");
    let (b, p) = (w.bag(), w.pairs());
    w.expect(&format!("crystal bag {b}"));
    w.expect(&format!("crystal pairs {p}"));
    w.set("/set(objetenmain={1:167})");
    w.cond();
    w.reset();
    for m in [1, 2, 3, 5] {
        w.set_var("MONDE", m);
        let v = w.s().visible_objects();
        w.expect(&format!("crystal visible monde {m} {v}"));
    }
    w.set_var("MONDE", 5);

    // 8. The centre draw of the hand and the cursor (FUN_0047bfd0).
    let objects = Some(CursorBank::Objects);
    let interface = Some(CursorBank::Interface);
    for (hand, hand_frame, cursor, cursor_frame) in [
        (false, 0, None, -1),
        (true, 5, None, -1),
        (true, 0, None, -1),
        (true, -1, None, -1),
        (false, 5, None, -1),
        (true, 5, interface, 13),
        (false, 0, interface, 25),
        (true, 5, objects, 5),
        (true, 0, interface, 0),
    ] {
        w.e.hand = hand.then_some((0, hand_frame));
        w.e.cursor.shown = cursor.map(|_| cursor_frame);
        if let Some(b) = cursor {
            w.e.cursor.bank = b;
        }
        let tibet = w.objects_bank();
        let hb = if hand { tibet.as_str() } else { "none" };
        let cb = cursor.map_or("none".to_string(), |b| w.cursor_bank(b));
        let draws: Vec<String> =
            w.e.plan_centre()
                .iter()
                .filter_map(|d| match d {
                    CentreDraw::Sprite { bank, frame, x, y } => Some(format!(
                        "AtlSpr_Aff({},{frame},{x},{y},0)",
                        w.cursor_bank(*bank)
                    )),
                    CentreDraw::Line { .. } => None,
                })
                .collect();
        let draws = if draws.is_empty() {
            "-".into()
        } else {
            draws.join(";")
        };
        let ret = u8::from(cursor.is_some());
        w.expect(&format!(
            "draw hand={hb},{hand_frame};cursor={cb},{cursor_frame} ret{ret} {draws}"
        ));
    }

    // 9. Many objects of the world: the bar scrolled to its end.
    for star in 67..=73 {
        w.set(&format!("/set(sacados={{1:{star}}})"));
    }
    let b = w.bag();
    w.expect(&format!("scroll bag {b}"));
    let bar = &w.e.bar;
    let row = format!(
        "scroll visible {} first {} slots {}",
        bar.visible, bar.first, bar.slots
    );
    w.expect(&row);
    w.e.bar.first = 0;
    let v = w.s().visible_objects();
    w.e.bar.layout(640, v);
    let bar = &w.e.bar;
    let row = format!(
        "layout640 visible {} first {} slots {}",
        bar.visible, bar.first, bar.slots
    );
    w.expect(&row);

    assert_eq!(w.next(), "", "golden rows left");
    assert!(
        w.failures.is_empty(),
        "{} differences:\n{}",
        w.failures.len(),
        w.failures.join("\n")
    );
}

/// The bar's frame by the original's code (FUN_00474c30, FUN_004752a0,
/// FUN_0040c510, FUN_0047c090): the right button, the bar and its cells,
/// the edge scroll, the hover name, the pick and the hand at the centre.
#[test]
fn the_bag_bar() {
    let Some(files) = game() else { return };
    let mut e = new_game(files);
    e.enter_at(10, 0.0, 0.0).unwrap();
    // Ten objects of Tibet: the three boats, the world stones, the stars.
    for id in [61, 53, 57, 52, 56, 60, 67, 68, 69, 70] {
        e.run_command(format!("(sacados={})", obj_name(&e, id)).as_bytes());
    }
    assert_eq!(e.bar.visible, 10);
    // Opening: the pointer frame at the screen centre.
    e.right_click();
    assert!(e.bar.open);
    assert_eq!(e.pointer, (320, 240));
    let mut screen = vec![0u16; 640 * 480];
    e.pass(Some(&mut screen));
    let sprites: Vec<(UiBank, i32, i32, i32)> = e
        .ui_draws
        .iter()
        .filter_map(|d| match d {
            UiDraw::Sprite { bank, frame, x, y } => Some((*bank, *frame, *x, *y)),
            UiDraw::Text { .. } => None,
        })
        .collect();
    // The bar, eight cells from the third object (scrolled to the end),
    // then the pointer at the mouse.
    assert_eq!(sprites[0], (UiBank::Interface, 0, 320, 441));
    let cells: Vec<(i32, i32)> = sprites[1..9].iter().map(|s| (s.1, s.2)).collect();
    let frames: Vec<i32> = [57, 52, 56, 60, 67, 68, 69, 70]
        .iter()
        .map(|&id| sprite_of(&e, id))
        .collect();
    let want: Vec<(i32, i32)> = frames
        .iter()
        .enumerate()
        .map(|(k, &f)| (f, 40 + 80 * k as i32))
        .collect();
    assert_eq!(cells, want);
    assert!(sprites[1..9]
        .iter()
        .all(|s| s.0 == UiBank::Objects && s.3 == 440));
    assert_eq!(sprites[9], (UiBank::Interface, 40, 320, 240));
    assert_eq!(e.bar.shown.len(), 8);
    // The centre draws nothing while the bar is open; the view does not
    // turn.
    assert!(e.centre_draws.is_empty());
    // The left edge scrolls one object per 0.1 s.
    e.mouse_motion(-400, 200);
    assert_eq!(e.pointer, (0, 440));
    e.set_clock(1.0);
    e.pass(None);
    assert_eq!(e.bar.first, 1);
    e.pass(None);
    assert_eq!(e.bar.first, 1);
    e.set_clock(1.2);
    e.pass(None);
    assert_eq!(e.bar.first, 0);
    // Over the second cell: its name, black then white, above the bar.
    e.mouse_motion(120, 0);
    e.pass(None);
    let texts: Vec<(i32, i32, u32)> = e
        .ui_draws
        .iter()
        .filter_map(|d| match d {
            UiDraw::Text { x, y, colour, .. } => Some((*x, *y, *colour)),
            UiDraw::Sprite { .. } => None,
        })
        .collect();
    assert_eq!(texts.len(), 2);
    assert_eq!((texts[0].1, texts[0].2), (411, 0));
    assert_eq!((texts[1].1, texts[1].2), (410, 0xff_ffff));
    assert_eq!(texts[0].0, texts[1].0 + 1);
    // A click on the second cell: the object in the hand, the bar closed.
    e.click(2.0);
    e.pass(Some(&mut screen));
    assert!(!e.bar.open);
    let id = 53;
    assert_eq!(e.vars.objet_en_main, id);
    assert_eq!(e.hand, Some((id, sprite_of(&e, id))));
    // The next frame draws the object in hand at the centre, the cursor
    // over it.
    e.pass(Some(&mut screen));
    match e.centre_draws[..2] {
        [CentreDraw::Sprite {
            bank: CursorBank::Objects,
            frame,
            x: 320,
            y: 240,
        }, CentreDraw::Sprite { .. }] => assert_eq!(frame, sprite_of(&e, id)),
        ref d => panic!("centre draws {d:?}"),
    }
    // A right click opens the bar again, another closes it and empties the
    // hand.
    e.right_click();
    e.right_click();
    assert!(!e.bar.open);
    assert_eq!((e.vars.objet_en_main, e.hand), (0, None));
}

fn defnames_of(e: &Engine) -> DefNames {
    DefNames::parse(
        &e.files()
            .read(atlantis2_data::Folder::Scenar, "defnames.con")
            .unwrap(),
    )
    .unwrap()
}

fn obj_name(e: &Engine, id: i32) -> String {
    let names = defnames_of(e);
    String::from_utf8_lossy(names.name(defnames::OBJETS, id as usize).unwrap()).into_owned()
}

fn sprite_of(e: &Engine, id: i32) -> i32 {
    defnames_of(e).param_of(defnames::OBJETS, id as usize)
}
