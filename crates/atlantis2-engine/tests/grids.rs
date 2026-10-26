//! The Maya counting grids of Yucatán's pyramid (`pyratez.wam` views 55,
//! 64 and 75) against the original's (`tests/golden/grids.txt`, its
//! header there): the data, the entries, the step on every cell for each
//! hand and neighbourhood, the other zones and the two tests, the clicks
//! along played sequences to the wins, the leaves, the hand's cursor, the
//! dispatchers and the reset, the clicks with chapters 31, 32 and 34 and
//! the scans of their frames, the hand of marbles at the centre and the
//! grids drawn; then each grid played with the mouse to its win and the
//! lines of its chapter it unlocks.
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped. With
//! `ATLANTIS2_PNG_DIR` set to a folder, the played test writes
//! `pz-grids-*.png` there.

use std::path::PathBuf;

use atlantis2_data::spr::{SprBank, Target};
use atlantis2_data::spw::{Spw, FACE};
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{
    grid_cursor_frame, grid_list_draw, grid_step, may_lay, may_take, yucatan_click_hooks,
    yucatan_entry_hooks, GridList, GridOp, GRID_SOLUTIONS, GRID_START, GRID_VIEWS,
};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{CentreDraw, CursorBank, Engine, Event, Fpu, HandBank, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/grids.txt");

const W: usize = 640;
const H: usize = 480;

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

fn rows(prefix: &str) -> impl Iterator<Item = &'static str> + '_ {
    GOLDEN.lines().filter(move |l| l.starts_with(prefix))
}

fn row(prefix: &str) -> &'static str {
    rows(prefix)
        .next()
        .unwrap_or_else(|| panic!("no row {prefix}"))
}

/// The value after `key` in a golden row, up to a space or `|`.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let i = line.find(key).unwrap_or_else(|| panic!("{key} in {line}")) + key.len();
    line[i..].split([' ', '|']).next().unwrap()
}

/// The 17 bytes of a 16-digit string.
fn digits(s: &str) -> [u8; 17] {
    let mut d = [0u8; 17];
    d[..16].copy_from_slice(&s.as_bytes()[..16]);
    d
}

/// The grids' sprites' ids (`A ids`).
const SPRITE_IDS: [i32; 3] = [24, 25, 26];

/// The frame list of every slot of an id, for the pure step.
struct List<'a> {
    anim: &'a mut Anim,
    id: i32,
}

impl GridList for List<'_> {
    fn has(&self, f: i32) -> bool {
        self.anim.has_frame(self.id, f)
    }
    fn sub(&mut self, f: i32) {
        self.anim.sub_frame(self.id, f);
    }
    fn add(&mut self, f: i32) {
        self.anim.add_frame(self.id, f);
    }
}

/// A game in the pyramid: a new game's film ended, then the world, the
/// place, view 54 and chapter 31 (as chapter 30's departure leaves it).
fn in_pyramid(files: A2Files) -> Engine {
    let mut e = Engine::boot(
        files,
        Variant::default(),
        Fpu::default(),
        W as u32,
        H as u32,
    )
    .unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=2)"[..],
        b"(changewam=pyratez.wam)",
        b"(vue=54)",
        b"(chapter=31)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("pyratez.wam", 54));
    e.trace = true;
    e
}

/// Films and dialogs ended until the panorama is back, the passes run.
fn settle(e: &mut Engine) {
    for _ in 0..400 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn set_var(e: &mut Engine, name: &str, v: i32) {
    e.script_state_mut().unwrap().set_var(name, v);
}

/// The hand pair as the golden writes it: frame / bank.
fn pair(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let bank = match s.cursor_bank() {
        HandBank::None => "0",
        HandBank::Objects => "OBJ",
        HandBank::Puzzle => "boul",
    };
    format!("{}/{bank}", s.cursor_sprite)
}

/// The golden's state: the three grids' digits and states, the hand, the
/// bank, the sound handles, the mode, the hand pair, `finmaya1..3`,
/// `resetmaya1..3`, `ObjetEnMain`.
fn state(e: &Engine) -> String {
    let d: Vec<String> = (0..3)
        .map(|g| {
            let d = e.grid_digits(g);
            let n = d.iter().position(|&c| c == 0).unwrap_or(17);
            String::from_utf8_lossy(&d[..n]).into_owned()
        })
        .collect();
    let st: Vec<String> = (0..3).map(|g| e.grid_state(g).to_string()).collect();
    let h: Vec<String> = e.grids.handles.iter().map(|h| h.to_string()).collect();
    let fin: Vec<String> = (1..=3)
        .map(|k| var(e, &format!("finmaya{k}")).to_string())
        .collect();
    let rst: Vec<String> = (1..=3)
        .map(|k| var(e, &format!("resetmaya{k}")).to_string())
        .collect();
    format!(
        "digits {} states {} hand {} bank {} handles {} mode {} pair {} fin {} reset {} objet {}",
        d.join("/"),
        st.join(","),
        e.grids.hand,
        if e.grids.bank_loaded { "set" } else { "0" },
        h.join(","),
        e.puzzle_mode(),
        pair(e),
        fin.join(","),
        rst.join(","),
        var(e, "ObjetEnMain")
    )
}

/// The golden's grid slot: its first slot of the id, index and list.
fn slot(e: &Engine, g: usize) -> String {
    let id = SPRITE_IDS[g];
    match e.anim.slots.iter().position(|s| s.id == id) {
        Some(n) => {
            let l: Vec<String> = e
                .anim
                .frame_list(n)
                .iter()
                .map(|&f| format!("{:02x}", f as u8))
                .collect();
            let l = l.join("");
            format!("s{n}:{}", if l.is_empty() { "-" } else { &l })
        }
        None => "-".into(),
    }
}

/// A golden row's state and slot (`-> <state> | <slot>`).
fn row_state(row: &str) -> (&str, &str) {
    let after = row.split_once(" -> ").unwrap().1;
    let mut parts = after.split(" | ");
    (parts.next().unwrap(), parts.next().unwrap_or("-"))
}

/// The three grids' digits and states as a golden row has them.
fn set_from(e: &mut Engine, row: &str) {
    let (st, _) = row_state(row);
    let ds = field(st, "digits ");
    for (g, d) in ds.split('/').enumerate() {
        e.set_grid_digits(g, &digits(d));
    }
    for (g, s) in field(st, "states ").split(',').enumerate() {
        e.set_grid_state(g, s.parse().unwrap());
    }
}

/// The sound calls of a golden row's sounds part (`load NAME>n`, `play
/// NAME>v2 ...`), the stops left out (the port's play stops the voice
/// first, unrecorded), and the engine's events the same way.
fn golden_sounds(part: &str) -> Vec<String> {
    part.split("; ")
        .filter_map(|c| {
            if let Some(r) = c.strip_prefix("load ") {
                Some(format!("load {r}"))
            } else {
                c.strip_prefix("play ")
                    .map(|r| format!("play {}", r.split('>').next().unwrap()))
            }
        })
        .collect()
}

fn engine_sounds(evs: &[Event]) -> Vec<String> {
    evs.iter()
        .filter_map(|x| match x {
            Event::SoundLoaded { name, entry } => Some(format!("load {name}>{entry}")),
            Event::Sound(name) => Some(format!("play {name}")),
            _ => None,
        })
        .collect()
}

/// The sound table empty and the three handles −1.
fn clear_sounds(e: &mut Engine) {
    e.sound_table_clear();
    e.grids.handles = [-1; 3];
}

#[test]
fn the_data_are_the_originals() {
    assert_eq!(
        row("A ids "),
        "A ids {1:24} {1:25} {1:26} {3:94} {3:95} {3:96} {3:146} {3:147} {3:148} {3:28}"
    );
    let a = row("A initial 0");
    let w: Vec<&str> = a.split(' ').collect();
    for g in 0..3 {
        assert_eq!(w[2 + g].as_bytes(), &GRID_START[..16]);
        assert_eq!(w[6 + g].as_bytes(), &GRID_SOLUTIONS[g][..]);
    }
    assert_eq!(w[10].as_bytes(), &GRID_START[..16]);
    assert!(row("A initial states")
        .starts_with("A initial states 0,0,0 hand 0 bank 0 handles -1,-1,-1"));
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    // The start-up state: the save's entries hold the digits.
    for g in 0..3 {
        assert_eq!(e.grid_digits(g), GRID_START);
        assert_eq!(e.grid_state(g), 0);
    }
    assert_eq!((e.grids.hand, e.grids.handles), (0, [-1; 3]));
    for (g, view) in GRID_VIEWS.into_iter().enumerate() {
        let zones: Vec<String> = e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .enumerate()
            .map(|(i, z)| format!("{i}:{} {}", z.kind, z.value))
            .collect();
        let r = rows(&format!("A view {view} (")).next().unwrap();
        assert!(r.ends_with(&format!("zones {}", zones.join(" "))), "{r}");
        e.enter_at(view, 0.0, 0.0).unwrap();
        let r = row(&format!("A view {view} slots "));
        for (n, s) in e.anim.slots.iter().enumerate() {
            let want = format!("s{n}:{}/{}/a{} f", s.id, s.kind, s.state.anim);
            assert!(r.contains(&want), "{r}: {want}");
            assert!(r.contains(&format!("c{} act", s.count)), "{r}");
        }
        assert!(e.anim.slots.iter().any(|s| s.id == SPRITE_IDS[g]));
    }
}

#[test]
fn the_entries_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    let mut n = 0;
    let cases =
        rows("B view ").filter(|r| !r.contains(" again -> ") && !r.contains("resetmaya3 1"));
    let mut lines: Vec<&str> = cases.collect();
    lines.dedup();
    for r in lines {
        let view: u32 = field(r, "B view ").parse().unwrap();
        let g = GRID_VIEWS.iter().position(|&v| v == view).unwrap();
        let label = r["B view 55 ".len()..].split(" digits ").next().unwrap();
        let before = [54, 63, 69][g];
        e.enter_at(before, 0.0, 0.0).unwrap();
        // The state before the entry: the row's digits for the grid, the
        // other grids solved; its state and reset; the mode 0, the bank
        // freed.
        let dg = &r[r.find(" digits ").unwrap() + 8..][..16];
        let reset: i32 = label.split("reset ").nth(1).unwrap()[..1].parse().unwrap();
        let st: i32 = label.split("state ").nth(1).unwrap()[..1].parse().unwrap();
        for (k, solution) in GRID_SOLUTIONS.iter().enumerate() {
            let d = if k == g {
                digits(dg)
            } else {
                digits(std::str::from_utf8(solution).unwrap())
            };
            e.set_grid_digits(k, &d);
            e.set_grid_state(k, if k == g { st } else { 0 });
            set_var(
                &mut e,
                &format!("resetmaya{}", k + 1),
                if k == g { reset } else { 0 },
            );
        }
        e.set_puzzle_mode(0);
        e.grid_bank_free();
        e.enter_at(view, 0.0, 0.0).unwrap();
        let (want, want_slot) = row_state(r);
        assert_eq!(state(&e), want, "{r}");
        assert_eq!(slot(&e, g), want_slot, "{r}");
        assert!(r.contains("sprload sprboul.spr"));
        // Again: the bank already loaded, the same state (the grid's list
        // drawn again).
        let again = rows(&format!("B view {view} {label} again -> "))
            .next()
            .unwrap();
        e.grid_entry(g);
        let (want, want_slot) = row_state(again);
        assert_eq!(state(&e), want, "{again}");
        assert_eq!(slot(&e, g), want_slot, "{again}");
        assert!(again.contains("bank spr déja init"));
        n += 1;
    }
    assert_eq!(n, 21);
    // resetmaya3 resets grid 2.
    let r = row("B view 75 resetmaya3 1");
    e.enter_at(69, 0.0, 0.0).unwrap();
    e.set_grid_digits(1, &digits("0000000000040001"));
    e.set_grid_digits(2, &digits("0000155500350001"));
    set_var(&mut e, "resetmaya3", 1);
    e.enter_at(75, 0.0, 0.0).unwrap();
    let (want, _) = row_state(r);
    assert_eq!(field(&state(&e), "digits "), field(want, "digits "));
}

/// The golden's sound token of a cell click: the step's sounds with an
/// empty table and the three handles −1.
fn cell_sounds(ops: &[GridOp]) -> String {
    ops.iter()
        .filter_map(|op| match op {
            GridOp::Take => Some("T"),
            GridOp::Refuse => Some("R"),
            GridOp::Replay => Some("!"),
            GridOp::Cursor => None,
        })
        .collect()
}

#[test]
fn every_cell_click_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    e.enter_at(55, 0.0, 0.0).unwrap();
    let n = e
        .anim
        .slots
        .iter()
        .position(|s| s.id == SPRITE_IDS[0])
        .unwrap();
    let mut tokens = 0;
    for v in 1..=16i32 {
        let i = (v - 1) as usize;
        for h in 0..6 {
            let r = row(&format!("C v{v} h{h} "));
            let want: Vec<&str> = r.split(' ').skip(3).collect();
            let befores: Vec<Option<u8>> = if i == 0 {
                vec![None]
            } else {
                (0..6).map(Some).collect()
            };
            let afters: Vec<Option<u8>> = if i == 15 {
                vec![None]
            } else {
                (0..6).map(Some).collect()
            };
            let mut k = 0;
            for &b in &befores {
                for c in 0..6u8 {
                    for &a in &afters {
                        let mut d = digits("0000000000000000");
                        d[15 - i] = b'0' + c;
                        if let Some(b) = b {
                            d[16 - i] = b'0' + b;
                        }
                        if let Some(a) = a {
                            d[14 - i] = b'0' + a;
                        }
                        let before = d;
                        let mut list = List {
                            anim: &mut e.anim,
                            id: SPRITE_IDS[0],
                        };
                        grid_list_draw(&mut list, &d);
                        let mut hand = h;
                        let (changed, ops) = grid_step(v, 0, &mut hand, &mut d, &mut list);
                        let other = (0..16).any(|j| j != 15 - i && d[j] != before[j]);
                        let l: String = e
                            .anim
                            .frame_list(n)
                            .iter()
                            .map(|&f| format!("{:02x}", f as u8))
                            .collect();
                        let got = format!(
                            "{}{}{}{}/{}{}",
                            d[15 - i] as char,
                            hand,
                            u8::from(changed),
                            cell_sounds(&ops),
                            if l.is_empty() { "-" } else { &l },
                            if other { "*" } else { "" }
                        );
                        assert_eq!(got, want[k], "v{v} h{h} {:?} {c} {:?}", b, a);
                        k += 1;
                        tokens += 1;
                    }
                }
            }
            assert_eq!(k, want.len());
        }
    }
    assert_eq!(tokens, 14 * 216 * 6 + 2 * 36 * 6);
}

#[test]
fn the_other_zones_and_the_tests_as_the_original() {
    // The two tests on the golden's strings.
    for r in rows("D tests ") {
        let s = field(r, "D tests ").trim_end_matches(':');
        let d = digits(s);
        let lay: Vec<String> = (0..6)
            .map(|h| {
                (0..16)
                    .map(|i| if may_lay(i, h, &d) { '1' } else { '0' })
                    .collect()
            })
            .collect();
        let take: String = (0..16)
            .map(|i| if may_take(i, &d) { '1' } else { '0' })
            .collect();
        assert_eq!(
            r,
            format!("D tests {s}: lay h0..5 {} take {take}", lay.join(" "))
        );
    }
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    e.enter_at(55, 0.0, 0.0).unwrap();
    // Each zone value, hand and object in hand, on the empty grid.
    for objet in [0, 5] {
        for v in [-1, 0, 17, 18, 19, 20, 21, 50] {
            let r = row(&format!("D v{v} objet {objet}: "));
            let mut out = Vec::new();
            for h in -1..=6 {
                let mut d = digits("0000000000000000");
                let mut list = List {
                    anim: &mut e.anim,
                    id: SPRITE_IDS[0],
                };
                grid_list_draw(&mut list, &d);
                let mut hand = h;
                let (changed, ops) = grid_step(v, objet, &mut hand, &mut d, &mut list);
                let mut p = "77/0x1234".to_string();
                let mut snd = cell_sounds(&ops);
                if ops.contains(&GridOp::Cursor) {
                    match grid_cursor_frame(hand) {
                        Ok(None) => p = "0/0".into(),
                        Ok(Some(f)) => p = format!("{f}/boul"),
                        Err(_) => {
                            snd = format!("E:set_curseur_billes_:_nb_billes_en_main=_%d_!_[{hand}]")
                        }
                    }
                }
                out.push(format!(
                    "h{h}>{hand} AL{} pair {p} {snd}",
                    u8::from(changed)
                ));
            }
            assert_eq!(r, format!("D v{v} objet {objet}: {}", out.join(" | ")));
        }
    }
    // A sequence from the empty grid with the sound table kept: the
    // engine's step (loads, plays, handles).
    let r = row("D sequence ");
    let parts: Vec<&str> = r.split(": ").nth(1).unwrap().split(" | ").collect();
    e.set_grid_digits(0, &digits("0000000000000000"));
    e.grid_entry(0);
    e.grids.hand = 0;
    clear_sounds(&mut e);
    for p in parts {
        let v: i32 = p[1..].split('>').next().unwrap().parse().unwrap();
        e.events.clear();
        let changed = e.grid_step(0, v);
        let mut snd: Vec<String> = engine_sounds(&e.events)
            .iter()
            .map(|s| {
                let n = &s[s.find("sound").unwrap() + 5..][..3];
                match s.split_once('>') {
                    Some((_, ent)) => format!("L{n}={ent}"),
                    None => format!("P{n}"),
                }
            })
            .collect();
        if snd.is_empty() {
            snd.push(String::new());
        }
        let want_snd: Vec<&str> = field(p, &format!("AL{} ", u8::from(changed)))
            .split('.')
            .filter(|t| !t.starts_with('S'))
            .collect();
        let want_snd: Vec<String> = match want_snd.as_slice() {
            ["T"] => vec!["L100=0".into(), "P100".into()],
            ["R"] => vec!["L102=0".into(), "P102".into()],
            w => w.iter().map(|s| s.to_string()).collect(),
        };
        assert_eq!(snd, want_snd, "{p}");
        let d = e.grid_digits(0);
        assert_eq!(&d[..16], field(p, ">").as_bytes(), "{p}");
        assert_eq!(e.grids.hand.to_string(), field(p, " h"), "{p}");
        let h: Vec<String> = e.grids.handles.iter().map(|h| h.to_string()).collect();
        assert_eq!(h.join(","), field(p, "handles "), "{p}");
    }
}

/// E: the clicks along played sequences, from the entry.
#[test]
fn the_wins_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    e.enter_at(55, 0.0, 0.0).unwrap();
    let mut wins = 0;
    for (g, view) in GRID_VIEWS.into_iter().enumerate() {
        for label in ["shortest", "played"] {
            let head = format!("E grid {} {label} ", g + 1);
            let seq: Vec<&str> = rows(&head).collect();
            if seq.is_empty() {
                continue;
            }
            let entry = seq[0];
            e.enter_at([54, 63, 69][g], 0.0, 0.0).unwrap();
            set_from(&mut e, entry);
            for k in 0..3 {
                e.set_grid_state(k, 0);
                set_var(&mut e, &format!("finmaya{}", k + 1), 0);
            }
            if g < 2 {
                set_var(&mut e, &format!("resetmaya{}", g + 1), 1);
            }
            e.grids.hand = 0;
            clear_sounds(&mut e);
            e.enter_at(view, 0.0, 0.0).unwrap();
            let (want, want_slot) = row_state(entry);
            assert_eq!(state(&e), want, "{entry}");
            assert_eq!(
                slot(&e, g),
                want_slot.split(" | ").next().unwrap(),
                "{entry}"
            );
            for r in &seq[1..] {
                let v: i32 = field(r, " v").parse().unwrap();
                e.events.clear();
                e.grid_click(g, v);
                check_click(&e, g, r);
                wins += usize::from(r.contains("load sound101.apc"));
            }
            if label == "shortest" {
                for r in rows(&format!("E grid {} after the win, ", g + 1)) {
                    let v: i32 = field(r, " v").parse().unwrap();
                    e.events.clear();
                    e.grid_click(g, v);
                    check_click(&e, g, r);
                }
                let r = row(&format!("E grid {} leave -> ", g + 1));
                e.events.clear();
                e.grid_leave(g);
                assert_eq!(state(&e), row_state(r).0, "{r}");
                let r = row(&format!("E grid {} entered again -> ", g + 1));
                e.enter_at(view, 0.0, 0.0).unwrap();
                let (want, want_slot) = row_state(r);
                assert_eq!(state(&e), want, "{r}");
                assert_eq!(slot(&e, g), want_slot, "{r}");
            }
        }
    }
    assert_eq!(wins, 4);
}

fn check_click(e: &Engine, g: usize, r: &str) {
    let after = r.split_once(" -> ").unwrap().1;
    let parts: Vec<&str> = after.split(" | ").collect();
    assert_eq!(state(e), parts[0], "{r}");
    assert_eq!(slot(e, g), parts[1], "{r}");
    assert_eq!(engine_sounds(&e.events), golden_sounds(parts[2]), "{r}");
}

#[test]
fn the_leaves_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    let mut n = 0;
    for (g, view) in GRID_VIEWS.into_iter().enumerate() {
        for wher in ["its view", "view 54"] {
            for solved in [0, 1] {
                for st in [0, 1, 2] {
                    let r = row(&format!(
                        "F grid {} {wher} solved {solved} state {st} -> ",
                        g + 1
                    ));
                    e.enter_at(view, 0.0, 0.0).unwrap();
                    clear_sounds(&mut e);
                    e.grid_step(g, 17);
                    let d = if solved == 1 {
                        let mut d = [0u8; 17];
                        d[..16].copy_from_slice(&GRID_SOLUTIONS[g]);
                        d
                    } else {
                        digits("0000000000000001")
                    };
                    // The other grids as the row has them.
                    set_from(&mut e, r);
                    e.set_grid_digits(g, &d);
                    e.set_grid_state(g, st);
                    set_var(&mut e, &format!("finmaya{}", g + 1), 7);
                    for k in 0..3 {
                        if k != g {
                            let f: i32 =
                                field(r, "fin ").split(',').nth(k).unwrap().parse().unwrap();
                            set_var(&mut e, &format!("finmaya{}", k + 1), f);
                        }
                    }
                    if wher != "its view" {
                        e.enter_at(54, 0.0, 0.0).unwrap();
                    }
                    let hand = e.grids.hand;
                    e.grid_leave(g);
                    let (want, _) = row_state(r);
                    // The hand as it is: dropped to 0 in the grid's own
                    // view, kept elsewhere (the rows carry one count, one
                    // more 0x11 click each, as this loop does).
                    if wher == "its view" {
                        assert_eq!(e.grids.hand, 0, "{r}");
                    } else {
                        assert_eq!(e.grids.hand, hand, "{r}");
                    }
                    assert_eq!(state(&e), want, "{r}");
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 36);
}

#[test]
fn the_hands_cursor_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    e.enter_at(55, 0.0, 0.0).unwrap();
    for loaded in [true, false] {
        for objet in [0, 5] {
            let r = row(&format!(
                "G bank {} objet {objet}: ",
                if loaded { "loaded" } else { "none" }
            ));
            let mut out = Vec::new();
            for h in -1..=6 {
                let one = |e: &mut Engine, tick: bool| {
                    e.grids.bank_loaded = loaded;
                    {
                        let s = e.script_state_mut().unwrap();
                        s.cursor_sprite = 77;
                        s.cursor_objects = true;
                        s.cursor_puzzle = false;
                    }
                    e.grids.hand = h;
                    set_var(e, "ObjetEnMain", objet);
                    if tick {
                        e.set_puzzle_mode(7);
                        e.puzzle_tick();
                    } else {
                        e.grids_cursor();
                    }
                    let fatal = e.script_state_mut().unwrap().fatal.take();
                    let p = pair(e).replace("77/OBJ", "77/0x1234");
                    (p, fatal)
                };
                let (a, fa) = one(&mut e, false);
                let (b, fb) = one(&mut e, true);
                let errs = match (fa, fb) {
                    (Some(_), Some(_)) => format!(
                        "fatal:set_curseur_billes : nb_billes_en_main= %d ! [{h}];fatal:set_curseur_billes : nb_billes_en_main= %d ! [{h}]"
                    ),
                    (None, None) => "-".into(),
                    x => panic!("{x:?}"),
                };
                out.push(format!("h{h}>{a} tick {b} {errs}"));
            }
            assert_eq!(
                r,
                format!(
                    "G bank {} objet {objet}: {}",
                    if loaded { "loaded" } else { "none" },
                    out.join(" | ")
                )
            );
        }
    }
}

#[test]
fn the_dispatchers_and_the_reset_as_the_original() {
    for r in rows("H click ") {
        let w: Vec<&str> = r.split(' ').collect();
        let (name, number, kind): (&str, u32, i32) =
            (w[2], w[3].parse().unwrap(), w[5].parse().unwrap());
        let (hooks, consumed) = yucatan_click_hooks(name.as_bytes(), number, kind, 0, false);
        assert!(!consumed);
        let mut got = Vec::new();
        for f in hooks {
            got.push(format!("{f:08x}"));
            // The step (@ 0x00493220) and the drop (@ 0x00492dd0) they call.
            match f {
                0x0049_31a0 | 0x0049_3490 | 0x0049_3510 => got.push("00493220".into()),
                0x0049_2d60 | 0x0049_2e80 | 0x0049_2ef0 => got.push("00492dd0".into()),
                _ => {}
            }
        }
        let got = if got.is_empty() {
            "-".to_string()
        } else {
            got.join(",")
        };
        assert_eq!(field(r, " -> "), got, "{r}");
    }
    for r in rows("H entry ") {
        let w: Vec<&str> = r.split(' ').collect();
        let (name, number): (&str, u32) = (w[2], w[3].parse().unwrap());
        let mut got: Vec<String> = Vec::new();
        for f in yucatan_entry_hooks(name.as_bytes(), number, 0) {
            got.push(format!("{f:08x}"));
            got.push("00492ff0".into());
        }
        let got = if got.is_empty() {
            "-".to_string()
        } else {
            got.join(",")
        };
        assert_eq!(field(r, " -> "), got, "{r}");
    }
    assert_eq!(row("H tick mode 7 "), "H tick mode 7 -> 00493660");
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    // The tick of the mode 7 is the marbles' (its thunk @ 0x00493190).
    e.enter_at(55, 0.0, 0.0).unwrap();
    assert_eq!(e.puzzle_tick(), Some(0x0049_3190));
    for view in [55, 54] {
        let r = row(&format!("H reset in view {view} -> "));
        let st = r.split(" | ").nth(1).unwrap();
        e.enter_at(55, 0.0, 0.0).unwrap();
        e.grids.hand = 4;
        e.set_puzzle_mode(7);
        let fake = format!("x -> {st}");
        set_from(&mut e, &fake);
        for k in 1..=3 {
            set_var(&mut e, &format!("finmaya{k}"), 7);
        }
        set_var(&mut e, "ObjetEnMain", 5);
        clear_sounds(&mut e);
        if view == 54 {
            e.enter_at(54, 0.0, 0.0).unwrap();
        }
        e.grids_reset();
        let got = state(&e);
        // The hand pair is not the reset's (the golden's is the
        // harness's own value there).
        let strip = |s: &str| {
            s.split(" pair ").next().unwrap().to_string() + " " + s.split(" fin ").nth(1).unwrap()
        };
        assert_eq!(strip(&got), strip(st), "{r}");
    }
}

/// The view turned to put the centre on zone `zone`.
fn aim(e: &mut Engine, zone: usize) {
    let view = e.place().1;
    let (yaw, pitch) = e.zone_aim(view, zone).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone), "view {view}");
}

/// I: the clicks with each grid's chapter, through the engine's pass (the
/// zone click, the click scan of the same frame, the tick).
#[test]
fn the_clicks_with_the_chapters_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    let mut t = 10.0;
    let mut n = 0;
    for (g, view) in GRID_VIEWS.into_iter().enumerate() {
        let chapter = [31, 32, 34][g];
        e.run_command(format!("(chapter={chapter})").as_bytes());
        settle(&mut e);
        e.enter_at([54, 63, 69][g], 0.0, 0.0).unwrap();
        let entered = row(&format!("I grid {} entered view {view} -> ", g + 1));
        set_from(&mut e, entered);
        for k in 1..=3 {
            set_var(&mut e, &format!("finmaya{k}"), 0);
            set_var(&mut e, &format!("resetmaya{k}"), 0);
        }
        set_var(&mut e, &format!("resetmaya{}", g + 1), i32::from(g < 2));
        for k in ["croco_ouvert", "pied_pris", "flag101"] {
            if e.script_state().unwrap().var(k).is_some() {
                set_var(&mut e, k, 0);
            }
        }
        e.grids.hand = 0;
        clear_sounds(&mut e);
        e.enter_at(view, 0.0, 0.0).unwrap();
        set_var(&mut e, "VUE", view as i32);
        assert_eq!(state(&e), row_state(entered).0, "{entered}");
        for r in rows(&format!("I grid {} zone ", g + 1)) {
            let w: Vec<&str> = r.split(' ').collect();
            let (kind, value): (i32, i32) = (w[4].parse().unwrap(), w[5].parse().unwrap());
            let z = e
                .wam()
                .view(view)
                .unwrap()
                .zones
                .iter()
                .position(|z| z.kind == kind && z.value == value)
                .unwrap();
            aim(&mut e, z);
            e.events.clear();
            t += 10.0;
            e.click(t);
            e.pass(None);
            let parts: Vec<&str> = r.split(" | ").collect();
            // The hooks run.
            let mut hooks: Vec<String> = Vec::new();
            for x in &e.events {
                if let Event::PuzzleRan { function } = x {
                    if *function == 0x0049_3190 {
                        continue;
                    }
                    hooks.push(format!("{function:08x}"));
                    match function {
                        0x0049_31a0 | 0x0049_3490 | 0x0049_3510 => {
                            hooks.push("00493220".into());
                        }
                        0x0049_2d60 | 0x0049_2e80 | 0x0049_2ef0 => hooks.push("00492dd0".into()),
                        _ => {}
                    }
                }
            }
            if e.events
                .iter()
                .any(|x| matches!(x, Event::SoundLoaded { name, .. } if name == "sound101.apc"))
            {
                hooks.push("00493470".into());
            }
            let hooks = if hooks.is_empty() {
                "-".into()
            } else {
                hooks.join(",")
            };
            assert_eq!(
                format!("hooks {hooks}"),
                parts[0].split(" -> ").nth(1).unwrap(),
                "{r}"
            );
            // CLICZONE at the click scan, the chapter's lines.
            let cz = e
                .events
                .iter()
                .find_map(|x| match x {
                    Event::ClickScan(v) => Some(v.cliczone),
                    _ => None,
                })
                .unwrap();
            assert_eq!(format!("ClicZone {cz}"), parts[1], "{r}");
            let lines: Vec<String> = e
                .events
                .iter()
                .filter_map(|x| match x {
                    Event::Set { chapter: c, line } if *c == chapter => Some(line.to_string()),
                    _ => None,
                })
                .collect();
            let lines = if lines.is_empty() {
                "-".into()
            } else {
                lines.join(",")
            };
            assert_eq!(format!("lines {lines} resumes 0"), parts[2], "{r}");
            // The scripts' sounds (NewSound) and the grid's.
            let news: Vec<String> = parts[3]
                .split("; ")
                .filter_map(|c| {
                    c.strip_prefix("AtlSound_NewSound('")
                        .map(|s| s.trim_end_matches("')").to_string())
                })
                .collect();
            let mut want = golden_sounds(parts[4]);
            want.extend(news.iter().map(|s| format!("play {s}")));
            let got = engine_sounds(&e.events);
            let mut got_sorted = got.clone();
            got_sorted.sort();
            want.sort();
            assert_eq!(got_sorted, want, "{r}");
            if kind == 1 {
                // A move: the leave ran before it; the state then (but
                // `OBJETENMAIN`: the move's start is the harness's stub,
                // the port's move runs on and its end empties the hand).
                let strip = |s: &str| s.split(" objet ").next().unwrap().to_string();
                assert_eq!(strip(&state(&e)), strip(parts[5]), "{r}");
                settle(&mut e);
                break;
            }
            assert_eq!(state(&e), parts[5], "{r}");
            assert_eq!(slot(&e, g), parts[6], "{r}");
            n += 1;
        }
        let after = row(&format!("I grid {} vars after: ", g + 1));
        for k in ["croco_ouvert", "pied_pris"] {
            assert_eq!(
                var(&e, k).to_string(),
                field(after, &format!("{k}=")),
                "{after}"
            );
        }
    }
    assert!(n > 30, "{n}");
}

/// K: the hand of marbles at the centre, and its sprites drawn.
#[test]
fn the_marbles_at_the_centre_as_the_original() {
    let Some(files) = game() else { return };
    let data = files.read(Folder::Spr, "sprboul.spr").unwrap();
    let bank = SprBank::parse(&data).unwrap();
    assert!(row("K spr\\sprboul.SPR md5 ").contains(&format!("{:x}", Md5::digest(&data))));
    let bg: Vec<u16> = (0..W * H)
        .map(|i| ((i as u64 * 2_654_435_761) as u32 >> 16) as u16)
        .collect();
    for f in 0..5 {
        let mut px = bg.clone();
        bank.draw(&mut Target::screen(&mut px, W, H), Some(320), 240, f);
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        assert_eq!(
            row(&format!("K AFF_SPR sprboul#{f}@320,240 ")),
            format!("K AFF_SPR sprboul#{f}@320,240 {:x}", Md5::digest(&bytes))
        );
    }
    let mut e = in_pyramid(files);
    e.enter_at(55, 0.0, 0.0).unwrap();
    let mut screen = vec![0u16; W * H];
    for h in 0..6 {
        e.grids.hand = h;
        set_var(&mut e, "ObjetEnMain", 0);
        e.grids_cursor();
        e.pass(Some(&mut screen));
        let draws: Vec<String> = e
            .centre_draws
            .iter()
            .filter_map(|d| match d {
                CentreDraw::Sprite {
                    bank: CursorBank::Puzzle,
                    frame,
                    x,
                    y,
                } => Some(format!("spr boul#{frame}@{x},{y}")),
                _ => None,
            })
            .collect();
        let draws = if draws.is_empty() {
            "-".into()
        } else {
            draws.join(" ")
        };
        assert_eq!(
            row(&format!("K hand {h}: ")),
            format!("K hand {h}: pair {} draws {draws}", pair(&e))
        );
    }
}

fn background(n: usize) -> Vec<u16> {
    (0..n)
        .map(|i| ((i as u64 * 2_654_435_761) as u32 >> 16) as u16)
        .collect()
}

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    format!("{:x}", Md5::digest(&bytes))
}

/// J: the grids' slots drawn on the six faces.
#[test]
fn the_grids_are_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let mut n = 0;
    for (name, view) in [
        ("1pA1055055.SPW", 55),
        ("2pA1064064.SPW", 64),
        ("3pA1075075.SPW", 75),
    ] {
        let data = files.read(Folder::Spw, name).expect("the grid's SPW");
        let key = name[..10].to_ascii_lowercase();
        assert!(
            row(&format!("J file {key}.SPW md5 ")).contains(&format!("{:x}", Md5::digest(&data)))
        );
        let spw = Spw::parse(data).unwrap();
        let state = EntityState {
            letter: u32::from(b'A'),
            anim: 1,
            view,
            made_for: view,
            shown: 1,
            single: 0,
            ..Default::default()
        };
        for r in rows(&format!("J {key} ")) {
            let l = field(r, "list=").trim_end_matches(':');
            let list: Vec<u8> = if l == "-" {
                Vec::new()
            } else {
                (0..l.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&l[i..i + 2], 16).unwrap())
                    .collect()
            };
            let mut anim = Anim::new(0);
            anim.load(name, spw.clone(), 2, 1, 0, state);
            let mut fl = [atlantis2_engine::anim::LIST_END; atlantis2_engine::anim::FRAME_TAB];
            fl[..list.len()].copy_from_slice(&list);
            anim.slots[0].frames = fl;
            let bg = background(FACE * FACE);
            let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
            anim.draw(&mut faces, None, false);
            let md5s: Vec<String> = faces.iter().map(|f| md5_hex(f)).collect();
            let tail = r.split_once(": ").unwrap().1;
            let want: Vec<&str> = tail.split(' ').take(6).collect();
            assert_eq!(md5s, want, "{r}");
            for (k, f) in faces.iter().enumerate() {
                if md5s[k] == md5_hex(&bg) {
                    continue;
                }
                let diff: Vec<usize> = (0..FACE * FACE).filter(|&i| f[i] != bg[i]).collect();
                assert!(
                    r.contains(&format!("face {k}: {} pixels changed", diff.len())),
                    "{r}"
                );
                for j in 0..8 {
                    let i = diff[(diff.len() - 1) * j / 7];
                    let s = format!("({},{})={:04x}", i % FACE, i / FACE, f[i]);
                    assert!(r.contains(&s), "{r}: {s}");
                }
            }
            n += 1;
        }
    }
    assert_eq!(n, 9 * 3 + 80);
}

// ---------------------------------------------------------------------------
// Played with the mouse.

/// The engine at 59.94 passes a second ([`PASS`]), every pass drawn.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
    gap: f64,
    shots: Option<PathBuf>,
}

impl Run {
    fn new(files: A2Files) -> Run {
        let mut e = in_pyramid(files);
        e.events.clear();
        Run {
            e,
            pass: 0,
            screen: vec![0; W * H],
            gap: 0.0,
            shots: std::env::var_os("ATLANTIS2_PNG_DIR").map(PathBuf::from),
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
        self.e.set_clock(self.now());
        match self.e.loop_type {
            LoopType::Video => self.e.end_video(),
            LoopType::Dialog => self.e.end_dialog(),
            _ => {}
        }
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
        let fatal = self.e.script_state().and_then(|s| s.fatal.clone());
        assert!(fatal.is_none(), "{fatal:?}");
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = &self.shots {
            std::fs::create_dir_all(dir).unwrap();
            let path = dir.join(format!("pz-grids-{name}.png"));
            std::fs::write(&path, encode_rgb565(&self.screen, W, H)).unwrap();
        }
    }

    /// The view turned with the mouse until the centre is on zone `zone`
    /// (as `tests/ogham.rs` does).
    fn steer(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        let wrap = |a: f32| {
            let t = std::f32::consts::TAU;
            (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
        };
        let (mut sx, mut sy) = (1.0f32, -1.0f32);
        for _ in 0..40 {
            if self.e.centre_zone().map(|(i, _)| i) == Some(zone) {
                self.wait(30);
                if self.e.centre_zone().map(|(i, _)| i) == Some(zone) {
                    return;
                }
            }
            let (y0, p0) = self.e.camera.angles();
            let (dy, dp) = (wrap(yaw - y0), pitch - p0);
            let mx = (sx * dy * 500.0).round() as i32;
            let my = (sy * dp * 500.0).round() as i32;
            self.e.mouse_motion(mx, my);
            self.wait(40);
            let (y1, p1) = self.e.camera.angles();
            if mx != 0 && wrap(yaw - y1).abs() > dy.abs() {
                sx = -sx;
            }
            if my != 0 && (pitch - p1).abs() > dp.abs() {
                sy = -sy;
            }
        }
        panic!("the centre never reached zone {zone} of view {view}");
    }

    /// A click on the first zone of the view of that kind and value; the
    /// events of the pass.
    fn press(&mut self, kind: i32, value: i32) -> Vec<Event> {
        let view = self.e.place().1;
        let zone = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == kind && z.value == value)
            .unwrap_or_else(|| panic!("no zone {kind} {value} in view {view}"));
        self.steer(zone);
        self.e.events.clear();
        self.tap();
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// A move to view `to`, until the arrival.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        let mut all = self.press(1, to as i32);
        for _ in 0..900 {
            self.step();
            all.extend(std::mem::take(&mut self.e.events));
            if self.e.place().1 == to && !self.e.turning() && self.e.loop_type == LoopType::Warp {
                self.wait(5);
                all.extend(std::mem::take(&mut self.e.events));
                return all;
            }
        }
        panic!("never arrived in view {to}");
    }
}

/// The marbles a sequence of zone values lays, as the player clicks them.
fn play_grid(r: &mut Run, g: usize, seq: &[i32]) -> Vec<Event> {
    let mut all = Vec::new();
    for &v in seq {
        all.extend(r.press(0x32, v));
        let _ = g;
    }
    all
}

fn lines_of(evs: &[Event], chapter: i32) -> Vec<usize> {
    evs.iter()
        .filter_map(|x| match x {
            Event::Set { chapter: c, line } if *c == chapter => Some(*line),
            _ => None,
        })
        .collect()
}

/// Every `/set` line reported is one the original's handler runs
/// (`scripts_worlds_china.txt` holds chapters 25-60).
fn lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_china.txt");
    for x in evs {
        if let Event::Set { chapter, line } = x {
            let head = format!("chapi{chapter:03}.con:{line} ");
            assert!(golden.lines().any(|l| l.starts_with(&head)), "{head}");
        }
    }
}

/// The three grids played with the mouse, as a player does: the first
/// with a refusal, a swap and a take back on the way; each win sets its
/// `finmayaN` (the guards' and the crocodile's sections then run), the
/// marbles in hand drawn at the centre; the third's win opens the
/// crocodile's jaw in the same frame (chapter 34's section, sound 112);
/// the grid's view left by its move zone keeps the win.
#[test]
fn the_grids_played_with_the_mouse() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files);
    // Grid 1, view 55, chapter 31.
    let evs = r.walk(55);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_2f60
    }));
    assert_eq!(r.e.puzzle_mode(), 7);
    r.shot("1-grid1-empty");
    let mut journal = Vec::new();
    // Two marbles, refused on cell 2, laid on cell 1.
    journal.extend(play_grid(&mut r, 0, &[17, 17]));
    assert_eq!(r.e.grids.hand, 2);
    r.wait(2);
    assert!(r.e.centre_draws.iter().any(|d| matches!(
        d,
        CentreDraw::Sprite {
            bank: CursorBank::Puzzle,
            frame: 1,
            ..
        }
    )));
    r.shot("2-grid1-two-marbles");
    let evs = play_grid(&mut r, 0, &[2]);
    assert!(evs.contains(&Event::Sound("sound102.apc".into())));
    assert_eq!(&r.e.grid_digits(0)[..16], b"0000000000000000");
    play_grid(&mut r, 0, &[1]);
    assert_eq!(&r.e.grid_digits(0)[..16], b"0000000000000002");
    // Five swapped for the two, the two put back, five laid.
    play_grid(&mut r, 0, &[18, 1]);
    assert_eq!(
        (&r.e.grid_digits(0)[..16], r.e.grids.hand),
        (&b"0000000000000005"[..], 2)
    );
    play_grid(&mut r, 0, &[19, 19]);
    assert_eq!(r.e.grids.hand, 0);
    // Four on the second cell: the win.
    let evs = play_grid(&mut r, 0, &[17, 17, 17, 17, 2]);
    assert!(evs.contains(&Event::Sound("sound101.apc".into())));
    assert_eq!(&r.e.grid_digits(0)[..16], b"0000000000000045");
    assert_eq!(
        (
            r.e.grid_state(0),
            r.e.script_state().unwrap().var("finmaya1")
        ),
        (2, Some(1))
    );
    r.wait(10);
    r.shot("3-grid1-won");
    let evs = r.walk(54);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_2d60
    }));
    assert_eq!((r.e.puzzle_mode(), r.e.grids.bank_loaded), (0, false));
    assert_eq!(r.e.script_state().unwrap().var("finmaya1"), Some(1));
    journal.extend(evs);
    // The guards' sections are `tests/flow.rs`'s (Yucatán played whole):
    // here the game moves on as chapter 31's way out leaves it.
    for c in [&b"(vue=63)"[..], b"(chapter=32)"] {
        r.e.run_command(c);
    }
    r.wait(10);
    // Grid 2, view 64, chapter 32: one marble on cell 1, four on cell 5.
    let evs = r.walk(64);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_3070
    }));
    let evs = play_grid(&mut r, 1, &[17, 1, 17, 17, 17, 17, 5]);
    assert!(evs.contains(&Event::Sound("sound101.apc".into())));
    assert_eq!(&r.e.grid_digits(1)[..16], b"0000000000040001");
    assert_eq!(r.e.script_state().unwrap().var("finmaya2"), Some(1));
    r.wait(10);
    r.shot("4-grid2-won");
    journal.extend(r.walk(63));
    assert_eq!(r.e.script_state().unwrap().var("finmaya2"), Some(1));
    // Grid 3, view 75, chapter 34 (reached as chapter 33's door leaves it).
    // The view before view 75 (its move zone to 75).
    let before =
        r.e.wam()
            .places
            .iter()
            .flat_map(|p| p.views.iter())
            .find(|v| v.id != 75 && v.zones.iter().any(|z| z.kind == 1 && z.value == 75))
            .map(|v| v.id)
            .unwrap();
    for c in [format!("(vue={before})"), "(chapter=34)".into()] {
        r.e.run_command(c.as_bytes());
    }
    r.wait(10);
    let evs = r.walk(75);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_3100
    }));
    let mut evs = play_grid(
        &mut r,
        2,
        &[17, 1, 18, 5, 17, 17, 17, 6, 18, 9, 18, 10, 18, 11],
    );
    assert_eq!(&r.e.grid_digits(2)[..16], b"0000055500350001");
    r.wait(2);
    r.shot("5-grid3-nearly");
    evs.extend(play_grid(&mut r, 2, &[17, 12]));
    // The win and, in the same frame's click scan, chapter 34's crocodile
    // (lines 25-33): sound 112 after the win's 101.
    assert_eq!(&r.e.grid_digits(2)[..16], b"0000155500350001");
    assert_eq!(r.e.script_state().unwrap().var("finmaya3"), Some(1));
    assert_eq!(r.e.script_state().unwrap().var("croco_ouvert"), Some(1));
    let l = lines_of(&evs, 34);
    assert!((25..=33).all(|k| l.contains(&k)), "{l:?}");
    let win = evs
        .iter()
        .position(|x| *x == Event::Sound("sound101.apc".into()))
        .unwrap();
    let croc = evs
        .iter()
        .position(|x| *x == Event::Sound("sound112.apc".into()))
        .unwrap();
    assert!(win < croc);
    journal.extend(evs);
    r.wait(20);
    r.shot("6-grid3-won-crocodile");
    lines_as_the_original(&journal);
}

/// A save made in a grid's view with marbles laid keeps the digits (save
/// entries 20-25), and the saved view's entry draws them, whichever view
/// the load is made in. The reset's leaves drop the hand only when the
/// load is made in the grid's view (FUN_00492dd0); from another view the
/// marbles in hand, not saved, are kept (0x006d2690 is written only by the
/// drop and the step).
#[test]
fn a_save_keeps_the_grids() {
    let Some(files) = game() else { return };
    let mut e = in_pyramid(files);
    let dir = std::env::temp_dir().join(format!("a2-grids-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    e.set_data_dir(Some(dir.clone()));
    e.enter_at(55, 0.0, 0.0).unwrap();
    e.grid_click(0, 18);
    e.grid_click(0, 1);
    assert_eq!(&e.grid_digits(0)[..16], b"0000000000000005");
    e.save_slot(1);
    let laid_more = |e: &mut Engine| {
        e.grid_click(0, 17);
        e.grid_click(0, 2);
        assert_eq!(&e.grid_digits(0)[..16], b"0000000000000015");
        // One marble in hand at the load.
        e.grid_click(0, 17);
        assert_eq!(e.grids.hand, 1);
    };
    let drawn = |e: &Engine| {
        assert_eq!(e.place().1, 55);
        assert_eq!(&e.grid_digits(0)[..16], b"0000000000000005");
        assert_eq!(e.grid_state(0), 1);
        assert_eq!(e.puzzle_mode(), 7);
        let n = e
            .anim
            .slots
            .iter()
            .position(|s| s.id == SPRITE_IDS[0])
            .unwrap();
        assert_eq!(e.anim.frame_list(n), [4]);
    };
    // The load in the grid's view: the hand dropped.
    laid_more(&mut e);
    e.load_slot(1).unwrap();
    drawn(&e);
    assert_eq!(e.grids.hand, 0);
    // The load in another view (left by a script's `vue=`, no leave): the
    // reset's leaves skip the drop, the hand stays.
    laid_more(&mut e);
    e.enter_at(54, 0.0, 0.0).unwrap();
    assert_eq!(e.place().1, 54);
    assert_eq!(e.puzzle_mode(), 7);
    e.load_slot(1).unwrap();
    drawn(&e);
    assert_eq!(e.grids.hand, 1);
    assert!(e.grids.bank_loaded);
    let _ = std::fs::remove_dir_all(&dir);
}
