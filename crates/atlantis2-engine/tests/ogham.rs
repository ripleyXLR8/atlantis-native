//! The Ogham stones of Ireland's view 6 against the original's
//! (`tests/golden/ogham.txt`, its header there): the view entry, each
//! zone value, every reachable row state and its six moves (the whole
//! puzzle: 66 × 71 × 71 states), the win, the dispatcher's choice, the
//! stones drawn by the frame list; then the puzzle played through the
//! engine with the mouse, from chapter 15's state to the win and the
//! lines of chapter 15 it unlocks, and kept by a save.
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped. With
//! `ATLANTIS2_PNG_DIR` set to a folder, the played test writes
//! `pz-ogham-*.png` there.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::{Spw, FACE};
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::anim::{Anim, FRAME_TAB as LIST, LIST_END};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{
    irish_click_hook, ogham_click, OghamClick, OGHAM_START, OGHAM_TARGET,
};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{Engine, Enhancements, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/ogham.txt");

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

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn rows(prefix: &str) -> impl Iterator<Item = &'static str> + '_ {
    GOLDEN.lines().filter(move |l| l.starts_with(prefix))
}

/// The value after `key=` in a golden row.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let i = line.find(key).unwrap_or_else(|| panic!("{key} in {line}")) + key.len();
    line[i..].split([' ', '|']).next().unwrap()
}

/// A list of 0x80 bytes: `b` then 0xfe.
fn list(b: &[u8]) -> [u8; LIST] {
    let mut l = [LIST_END; LIST];
    l[..b.len()].copy_from_slice(b);
    l
}

/// The bytes of a list up to its end.
fn used(l: &[u8; LIST]) -> &[u8] {
    let n = l.iter().position(|&b| b == LIST_END).unwrap_or(LIST);
    &l[..n]
}

/// A click from a fresh entry of `state` (one slot of the id, whose list
/// the entry makes the state: golden B): the state, the list and what the
/// click did; `slot` false: no slot of the id (the original then edits
/// the zeroed record past the last slot).
fn click_from(
    state: &[u8; 15],
    value: i32,
    finogham: i32,
    slot: bool,
) -> ([u8; 15], [u8; LIST], OghamClick) {
    let mut s = *state;
    let mut l = if slot { list(state) } else { [0; LIST] };
    let fresh = l;
    let found = |f: i32| {
        if slot && fresh.iter().any(|&b| i32::from(b as i8) == f) {
            f
        } else {
            -1
        }
    };
    let r = ogham_click(&mut s, &mut l, found, finogham, value);
    (s, l, r)
}

/// Without the scripts: `bigile1.wam` loaded and view 6 entered.
fn view6(files: A2Files) -> Engine {
    let mut e = Engine::new_game(
        files,
        Variant::default(),
        Fpu::default(),
        W as u32,
        H as u32,
    )
    .unwrap();
    e.load_place("bigile1.wam");
    e.enter_at(6, 0.0, 0.0).unwrap();
    e
}

#[test]
fn the_start_and_the_target_are_the_originals() {
    assert_eq!(
        rows("A initial")
            .next()
            .unwrap()
            .split(' ')
            .next_back()
            .unwrap(),
        hex(&OGHAM_START)
    );
    assert_eq!(
        rows("A target")
            .next()
            .unwrap()
            .split(' ')
            .next_back()
            .unwrap(),
        hex(&OGHAM_TARGET)
    );
    assert!(GOLDEN.contains("ids sprite {1:22} knife {1:50} variable {3:98}"));
}

#[test]
fn the_entry_sets_the_lists_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view6(files);
    let sprite = 22;
    assert!(GOLDEN.contains(&format!("sprite {{1:{sprite}}}")));
    let n = e.anim.slots.iter().position(|s| s.id == sprite).unwrap();
    // The view's slot: its list is the state from the entry on.
    assert_eq!(used(&e.anim.slots[n].frames), &OGHAM_START[..]);
    let base = e.anim.slots[n].clone();
    // (label, the slots before: id and list, state)
    type Case = (&'static str, Vec<(i32, Vec<u8>)>, [u8; 15]);
    let cases: [Case; 6] = [
        (
            "fresh list, initial state",
            vec![(sprite, vec![])],
            OGHAM_START,
        ),
        (
            "fresh list, target state",
            vec![(sprite, vec![])],
            OGHAM_TARGET,
        ),
        (
            "stale list 03 14 00 0e 0f 07 3a",
            vec![(sprite, vec![3, 0x14, 0, 0x0e, 0x0f, 7, 0x3a])],
            OGHAM_START,
        ),
        (
            "stale list ff 05 ff",
            vec![(sprite, vec![0xff, 5, 0xff])],
            OGHAM_START,
        ),
        (
            "two slots of the id, another id first",
            vec![(sprite + 1, vec![]), (sprite, vec![]), (sprite, vec![1])],
            OGHAM_START,
        ),
        ("no slot of the id", vec![(sprite + 1, vec![])], OGHAM_START),
    ];
    for (label, before, state) in cases {
        let row = rows(&format!("B {label}:")).next().unwrap();
        e.anim.slots.clear();
        for (id, l) in &before {
            let mut s = base.clone();
            s.id = *id;
            s.frames = list(l);
            e.anim.slots.push(s);
        }
        e.ogham = state;
        e.ogham_entry();
        for (k, s) in e.anim.slots.iter().enumerate() {
            assert_eq!(
                hex(used(&s.frames)),
                field(row, &format!("s{k}=")),
                "{label}"
            );
        }
    }
}

#[test]
fn each_zone_value_from_the_start_as_the_original() {
    for row in rows("C ") {
        let head = row.split(':').next().unwrap();
        let without = head.contains("without");
        let value: i32 = head[2..].split(' ').next().unwrap().parse().unwrap();
        let (s, l, r) = click_from(&OGHAM_START, value, 0, !without);
        assert_eq!(hex(&s), field(row, "state="), "{row}");
        if !without {
            assert_eq!(hex(used(&l)), field(row, "list="), "{row}");
            assert_eq!(r.won, field(row, "{3:98}=") == "1", "{row}");
        }
        assert_eq!(r.sound, row.contains("(newsound=4)"), "{row}");
    }
}

#[test]
fn every_reachable_row_move_as_the_original() {
    // The golden's rows: row, state, then each zone's result.
    type Row = (usize, String, Vec<(i32, String)>);
    let mut golden: Vec<Row> = Vec::new();
    for line in rows("D r").filter(|l| l.as_bytes()[3].is_ascii_digit()) {
        let (head, tail) = line.split_once(": ").unwrap();
        let mut w = head.split(' ');
        let r: usize = w.nth(1).unwrap()[1..].parse().unwrap();
        let state = w.next().unwrap().to_string();
        let moves = tail
            .split(' ')
            .map(|m| {
                let (z, b) = m.split_once('>').unwrap();
                (z.parse().unwrap(), b.to_string())
            })
            .collect();
        golden.push((r, state, moves));
    }
    assert_eq!(golden.len(), 66 + 71 + 71);
    // The port's own search, row by row, in the same order (breadth
    // first, zones in the row's order), each move from a fresh entry.
    let mut port = Vec::new();
    let mut total = 1usize;
    let mut solution = Vec::new();
    // The golden's checks, counted over every transition (the port's
    // click has no error path: its sound is the one call).
    let (mut same_list, mut one_sound, mut others_kept, mut transitions) = (0, 0, 0, 0);
    for r in 0..3 {
        let zones: Vec<i32> = (1..=5)
            .map(|i| 5 * r as i32 + i)
            .chain([16 + r as i32])
            .collect();
        let mut seen: HashMap<[u8; 15], Option<([u8; 15], i32)>> = HashMap::new();
        seen.insert(OGHAM_START, None);
        let mut queue = VecDeque::from([OGHAM_START]);
        let mut count = 0;
        while let Some(s) = queue.pop_front() {
            count += 1;
            let mut moves = Vec::new();
            for &z in &zones {
                let (t, l, c) = click_from(&s, z, 0, true);
                transitions += 1;
                same_list += usize::from(used(&l) == &t[..]);
                one_sound += usize::from(c.sound && !c.won);
                others_kept += usize::from((0..15).filter(|i| i / 5 != r).all(|i| t[i] == s[i]));
                moves.push((z, hex(&t[5 * r..5 * r + 5])));
                if let std::collections::hash_map::Entry::Vacant(v) = seen.entry(t) {
                    v.insert(Some((s, z)));
                    queue.push_back(t);
                }
            }
            port.push((r, hex(&s[5 * r..5 * r + 5]), moves));
        }
        total *= count;
        // The shortest way to the target's row.
        let mut s = *seen
            .keys()
            .find(|k| k[5 * r..5 * r + 5] == OGHAM_TARGET[5 * r..5 * r + 5])
            .expect("the target's row is reachable");
        let mut path = Vec::new();
        while let Some(Some((p, z))) = seen.get(&s) {
            path.push(*z);
            s = *p;
        }
        path.reverse();
        solution.extend(path);
    }
    assert_eq!(port, golden);
    assert_eq!(
        format!(
            "D checks over every transition: list = state {same_list}, one (newsound=4), \
             no error {one_sound}, other rows unchanged {others_kept}, transitions {transitions}"
        ),
        rows("D checks").next().unwrap()
    );
    // The golden's independence sample: each reachable row state's six
    // moves with the other rows in the target, an odd and the initial
    // state, against the same move with the other rows initial.
    let odd: [u8; 15] = unhex("0304ffffff10ffffffffff03ffffff").try_into().unwrap();
    let (mut same, mut sampled) = (0, 0);
    for r in 0..3 {
        let zones: Vec<i32> = (1..=5)
            .map(|i| 5 * r as i32 + i)
            .chain([16 + r as i32])
            .collect();
        let seen: std::collections::BTreeSet<&str> = port
            .iter()
            .filter(|p| p.0 == r)
            .map(|p| p.1.as_str())
            .collect();
        for rb in seen {
            let rb = unhex(rb);
            for o in [OGHAM_TARGET, odd, OGHAM_START] {
                for &z in &zones {
                    let mut s = o;
                    s[5 * r..5 * r + 5].copy_from_slice(&rb);
                    let a = click_from(&s, z, 0, true).0;
                    let mut i = OGHAM_START;
                    i[5 * r..5 * r + 5].copy_from_slice(&rb);
                    let b = click_from(&i, z, 0, true).0;
                    sampled += 1;
                    same += usize::from(a[5 * r..5 * r + 5] == b[5 * r..5 * r + 5]);
                }
            }
        }
    }
    assert_eq!(
        format!(
            "D independence sample: {same} of {sampled} row transitions equal with the \
             other rows in the target, an odd and the initial state"
        ),
        rows("D independence").next().unwrap()
    );
    let line = rows("D the whole puzzle").next().unwrap();
    assert!(line.contains(&format!("{total} reachable states")));
    let want: Vec<i32> = line
        .split(": ")
        .last()
        .unwrap()
        .split(' ')
        .map(|z| z.parse().unwrap())
        .collect();
    // Breadth first in the same order: the same path, the shortest per
    // row.
    assert_eq!(solution, want);
    let mut s = OGHAM_START;
    for &z in &want {
        s = click_from(&s, z, 0, true).0;
    }
    assert_eq!(s, OGHAM_TARGET);
}

#[test]
fn the_win_as_the_original() {
    let e = |label: &str| rows(&format!("E zone 99 {label}:")).next().unwrap();
    // The target, `FINOGHAM` 0, 1 (a second win), 2.
    for (label, fin) in [("{3:98}=0", 0), ("again", 1), ("{3:98}=2", 2)] {
        let row = e(label);
        let (_, _, r) = click_from(&OGHAM_TARGET, 99, fin, true);
        let after = if r.won { 1 } else { fin };
        assert_eq!(
            after.to_string(),
            field(row.split(": ").nth(1).unwrap(), "{3:98}=")
        );
        assert!(r.sound);
    }
    let (_, _, r) = click_from(&OGHAM_START, 99, 0, true);
    assert!(!r.won && r.sound);
    assert!(e("on the initial state").contains("{3:98}=0"));
    let mut off = OGHAM_TARGET;
    off[14] = 0x3a;
    let (_, _, r) = click_from(&off, 99, 0, true);
    assert!(!r.won);
    assert!(e(&format!("on {}", hex(&off))).contains("{3:98}=0"));
}

#[test]
fn the_dispatcher_as_the_original() {
    for row in rows("F ") {
        let name = field(row, "name=");
        let view: u32 = field(row, "view=").parse().unwrap();
        let kind: i32 = field(row, "type=").parse().unwrap();
        let knife = field(row, "hand=").trim_end_matches(':') == "50";
        let want = match field(row, "hook=") {
            "click" => Some(0x0049_60a0),
            "move" => Some(0x0049_6080),
            _ => None,
        };
        let got = irish_click_hook(name.as_bytes(), view, kind, knife)
            .filter(|f| [0x0049_60a0, 0x0049_6080].contains(f));
        assert_eq!(got, want, "{row}");
        assert_eq!(field(row, "AL="), "0");
    }
}

/// The harnesses' background: pixel i = (i × 2654435761 mod 2³²) >> 16.
fn background(n: usize) -> Vec<u16> {
    (0..n)
        .map(|i| ((i as u64 * 2_654_435_761) as u32 >> 16) as u16)
        .collect()
}

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    format!("{:x}", Md5::digest(&bytes))
}

#[test]
fn the_stones_are_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let data = files
        .read(Folder::Spw, "ogA1006006.SPW")
        .expect("the stones' SPW");
    let spw = Spw::parse(data).unwrap();
    let state = EntityState {
        letter: u32::from(b'A'),
        anim: 1,
        view: 6,
        made_for: 6,
        shown: 1,
        single: 0,
        ..Default::default()
    };
    let mut n = 0;
    for row in rows("H ") {
        let st = unhex(field(row, "list=").trim_end_matches(':'));
        let mut anim = Anim::new();
        anim.load("ogA1006006.SPW", spw.clone(), 2, 1, 0, state);
        anim.slots[0].frames = list(&st);
        let bg = background(FACE * FACE);
        let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
        anim.draw(&mut faces, None, false);
        let md5s: Vec<String> = faces.iter().map(|f| md5_hex(f)).collect();
        let tail = &row[row.find("list=").unwrap()..];
        let tail = tail.split_once(": ").unwrap().1;
        let want: Vec<&str> = tail.split(' ').take(6).collect();
        assert_eq!(md5s, want, "{row}");
        let diff: Vec<usize> = (0..FACE * FACE).filter(|&i| faces[5][i] != bg[i]).collect();
        assert!(
            row.contains(&format!("face 5: {} pixels changed", diff.len())),
            "{row}"
        );
        if !diff.is_empty() {
            for j in 0..8 {
                let i = diff[(diff.len() - 1) * j / 7];
                let s = format!("({},{})={:04x}", i % FACE, i / FACE, faces[5][i]);
                assert!(row.contains(&s), "{row}: {s}");
            }
        }
        n += 1;
    }
    assert_eq!(n, 9);
}

// ---------------------------------------------------------------------
// Through the engine, with the mouse.

const W: usize = 640;
const H: usize = 480;

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
        let mut e = Engine::boot(
            files,
            Variant::default(),
            Fpu::default(),
            W as u32,
            H as u32,
        )
        .unwrap();
        e.trace = true;
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

    /// One pass, its events taken.
    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// A left click far from the last one (never a double click).
    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    /// The mouse moved to put the pointer at (x, y).
    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
    }

    fn var(&self, name: &str) -> i32 {
        self.e.script_state().unwrap().var(name).unwrap()
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.id(section, name.as_bytes()).unwrap() as i32
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = &self.shots {
            std::fs::create_dir_all(dir).unwrap();
            let path = dir.join(format!("pz-ogham-{name}.png"));
            std::fs::write(&path, encode_rgb565(&self.screen, W, H)).unwrap();
        }
    }

    /// The view turned with the mouse as a player does until the centre
    /// is on zone `zone`: a move of d mickeys turns the view by d / 500
    /// radians in all (d / 2000 a frame, × 0.8 each frame), so each move
    /// aims at the zone's centre and the turn is let settle.
    fn steer(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        let wrap = |a: f32| {
            let t = std::f32::consts::TAU;
            (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
        };
        // The signs of the mouse axes, found by the first moves.
        let (mut sx, mut sy) = (1.0f32, -1.0f32);
        for _ in 0..40 {
            if self.e.centre_zone().map(|(i, _)| i) == Some(zone) {
                // Settled on it: the speeds spent.
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

    /// A click on the move zone to view `to`, the transition clicked
    /// away, until the arrival.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        let view = self.e.place().1;
        let zone = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 1 && z.value == to as i32)
            .unwrap_or_else(|| panic!("no move from view {view} to {to}"));
        self.steer(zone);
        self.tap();
        let mut all = Vec::new();
        for _ in 0..900 {
            let evs = self.events();
            let video = evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            });
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if video {
                self.wait(15);
                self.tap();
            }
            if arrived {
                self.wait(4);
                all.extend(std::mem::take(&mut self.e.events));
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// The bag bar opened with the right button and object `id`'s cell
    /// clicked: the object in hand, the bar closed.
    fn pick(&mut self, id: i32) {
        self.e.right_click();
        assert!(self.e.bar.open);
        self.step();
        let bag = self.e.bag_entries();
        let k = self
            .e
            .bar
            .shown
            .iter()
            .position(|&n| bag[n].id == id)
            .expect("the object in a cell of the bar");
        self.point(40 + 80 * k as i32, 440);
        self.step();
        self.tap();
        self.step();
        assert!(!self.e.bar.open);
        assert_eq!(self.var("OBJETENMAIN"), id);
    }

    /// A click on the first zone of value `value` of the view; the events.
    fn press_stone(&mut self, value: i32) -> Vec<Event> {
        let zone = self
            .e
            .wam()
            .view(6)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 4 && z.value == value)
            .unwrap();
        self.steer(zone);
        self.tap();
        let mut evs = self.events();
        evs.extend(self.events());
        evs
    }
}

/// Every `/set` line of an Irish chapter that ran is one the original's
/// handler runs (`scripts_worlds_ireland.txt`).
fn lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_ireland.txt");
    for e in evs {
        if let Event::Set { chapter, line } = e {
            if (1..=24).contains(chapter) {
                let head = format!("chapi{chapter:03}.con:{line} ");
                assert!(
                    golden.lines().any(|l| l.starts_with(&head)),
                    "chapter {chapter} line {line}: not a line the original runs"
                );
            }
        }
    }
}

/// A golden G row whose click scan runs nothing (no line, no film), by a
/// click on the first zone of value `value`: the hook's sound, `CLICZONE`
/// as the click scan sees it, the lines it ran, the bytes, `VUE` and
/// `FINOGHAM` after. (The hook's own `FINOGHAM` is not observed apart:
/// with no line run it is the one after.) The golden's click has no
/// sprite under the centre (@ 0x00661640 = −1), so the spiral of view 6
/// (`SPIRALE_W6`, over the stones: chapter 15's section 3 reacts to it)
/// is hidden for the click.
fn quiet_g_row(r: &mut Run, value: i32, label: &str) {
    let head = format!("G {label}: ");
    let want = rows(&head).next().unwrap().strip_prefix(&head).unwrap();
    r.e.run_command(b"(hideobjet=SPIRALE_W6)");
    r.wait(2);
    let evs = r.press_stone(value);
    r.e.run_command(b"(showobjet=SPIRALE_W6)");
    r.wait(2);
    assert!(
        !evs.iter()
            .any(|e| matches!(e, Event::ClickScan(v) if v.clicobjet != 0)),
        "{label}: an object under the centre"
    );
    let sound = evs
        .iter()
        .filter(|e| matches!(e, Event::Sound(_)))
        .collect::<Vec<_>>();
    assert_eq!(sound, [&Event::Sound("sound004.apc".into())], "{label}");
    let cz = evs
        .iter()
        .find_map(|e| match e {
            Event::ClickScan(v) => Some(v.cliczone),
            _ => None,
        })
        .expect("the click scan");
    let lines: Vec<String> = evs
        .iter()
        .filter_map(|e| match e {
            Event::Set { line, .. } => Some(line.to_string()),
            _ => None,
        })
        .collect();
    let lines = if lines.is_empty() {
        "-".to_string()
    } else {
        lines.join(",")
    };
    assert!(
        !evs.iter().any(|e| matches!(e, Event::VideoStart { .. })),
        "{label}"
    );
    let fin = r.var("FINOGHAM");
    let got = format!(
        "OUT lines - | hook RunOneCommand('(newsound=4)') {{3:98}}={fin} | ClicZone={cz} | \
         IN lines {lines} resumes=0 | state={} Vue={} {{3:98}}={fin} | calls=",
        hex(&r.e.ogham),
        r.var("VUE")
    );
    assert_eq!(got, want, "{label}");
}

#[test]
fn the_ogham_stones_played_with_the_mouse() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files);
    // The golden's rows are the original's: `CLICZONE` = the stone's
    // value (the minor bug fixes leave it as it was,
    // `tests/plain_values.rs`).
    r.e.enhancements = Enhancements::original();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ogham");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    r.e.set_data_dir(Some(dir));
    r.e.start_new_game().unwrap();
    // The music copy's please-wait page of a first new game.
    while r.e.copying() {
        r.step();
    }
    r.e.end_video();
    r.wait(5);
    // Chapter 15's state: Ireland's hub chapter (3) with Ailill's knife
    // in the bag (chapter 7's exchange), next to the stones (view 7).
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=7)",
        b"(chapter=3)",
        b"(sacados=COUTEAU_AILILL)",
    ] {
        r.e.run_command(c);
    }
    for _ in 0..200 {
        match r.e.loop_type {
            LoopType::Video => r.e.end_video(),
            LoopType::Dialog => r.e.end_dialog(),
            _ => {}
        }
        r.step();
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    r.wait(30);
    r.e.events.clear();
    assert_eq!(r.e.place(), ("bigile1.wam", 7));
    assert_eq!(r.e.ogham, OGHAM_START);
    let knife = r.id(defnames::OBJETS, "COUTEAU_AILILL");
    let sprite = r.id(defnames::OBJETS, "OGHAM_SPRITE");

    // To the stones: chapter 3 sends the game to chapter 15.
    let evs = r.walk(6);
    lines_as_the_original(&evs);
    assert_eq!(r.e.place(), ("bigile1.wam", 6));
    assert_eq!(r.e.script_state().unwrap().chapter, 15);
    let slot = r.e.anim.slots.iter().position(|s| s.id == sprite).unwrap();
    assert_eq!(used(&r.e.anim.slots[slot].frames), &OGHAM_START[..]);

    // A click on a stone without the knife: no hook, the stones stay.
    let evs = r.press_stone(5);
    r.shot("1-stones");
    assert!(!evs.iter().any(|e| matches!(e, Event::PuzzleRan { .. })));
    assert_eq!(r.e.ogham, OGHAM_START);

    // The knife from the bag, then the original's shortest solution,
    // each click checked against the golden's move from that state.
    r.pick(knife);
    // Golden G, the click handler and the click scan of chapter 15 on the
    // initial bytes: zone 99 (no win), then zone 17 (row 1 turned); three
    // more turns bring row 1 back.
    quiet_g_row(&mut r, 99, "zone 99, initial, {3:98}=0");
    quiet_g_row(&mut r, 17, "zone 17, initial");
    for _ in 0..3 {
        r.press_stone(17);
    }
    assert_eq!(r.e.ogham, OGHAM_START);
    let solution: Vec<i32> = rows("D the whole puzzle")
        .next()
        .unwrap()
        .split(": ")
        .last()
        .unwrap()
        .split(' ')
        .map(|z| z.parse().unwrap())
        .collect();
    for (n, &z) in solution.iter().enumerate() {
        let before = r.e.ogham;
        let evs = r.press_stone(z);
        assert!(
            evs.contains(&Event::PuzzleRan {
                function: 0x0049_60a0
            }),
            "zone {z}"
        );
        let row = (z as usize - 1) / 5 % 3;
        let row = if z >= 16 { z as usize - 16 } else { row };
        let head = format!("D r{row} {}:", hex(&before[5 * row..5 * row + 5]));
        let line = rows(&head).next().unwrap();
        let want = line
            .split(' ')
            .find_map(|m| m.strip_prefix(&format!("{z}>")))
            .unwrap();
        assert_eq!(hex(&r.e.ogham[5 * row..5 * row + 5]), want, "zone {z}");
        assert_eq!(used(&r.e.anim.slots[slot].frames), &r.e.ogham[..]);
        assert_eq!(r.var("OBJETENMAIN"), knife, "the knife stays in hand");
        assert_eq!(r.var("FINOGHAM"), 0);
        if n == 2 {
            r.wait(2);
            r.shot("2-row-0-cleared");
        }
    }
    assert_eq!(r.e.ogham, OGHAM_TARGET);
    r.wait(2);
    r.shot("3-solved");

    // Saved here, the stones changed, the save loaded: the stones back.
    let saved = r.e.save_data();
    r.e.ogham = OGHAM_START;
    r.e.load_data(&saved).unwrap();
    r.wait(2);
    assert_eq!(r.e.ogham, OGHAM_TARGET);
    let slot = r.e.anim.slots.iter().position(|s| s.id == sprite).unwrap();
    assert_eq!(used(&r.e.anim.slots[slot].frames), &OGHAM_TARGET[..]);
    assert_eq!(r.e.place(), ("bigile1.wam", 6));
    if r.var("OBJETENMAIN") != knife {
        r.pick(knife);
    }

    // Golden G, zone 99 on the solved bytes with the stick already in the
    // bag: `FINOGHAM` = 1 by the hook, chapter 15 runs nothing. Then the
    // save loaded again (`FINOGHAM` 0, the stick not in the bag).
    r.e.run_command(b"(sacados=BATON_OGHAM)");
    quiet_g_row(&mut r, 99, "zone 99, solved, {3:98}=0, {1:34} in the bag");
    r.e.load_data(&saved).unwrap();
    r.wait(2);
    assert_eq!((r.e.ogham, r.var("FINOGHAM")), (OGHAM_TARGET, 0));
    if r.var("OBJETENMAIN") != knife {
        r.pick(knife);
    }

    // Zone 99: `FINOGHAM` = 1 by the hook, then the click scan of the
    // same frame runs chapter 15's lines 16-19 (the original's, golden
    // section G): the film, the stick shown, Ailill's subject, view 106.
    let g = rows("G zone 99, solved, {3:98}=0:").next().unwrap();
    let want_lines: Vec<usize> = g
        .split("IN lines ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .split(',')
        .map(|l| l.parse().unwrap())
        .collect();
    assert_eq!(want_lines, [16, 17, 18, 19]);
    let zone =
        r.e.wam()
            .view(6)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.value == 99)
            .unwrap();
    r.steer(zone);
    r.tap();
    let mut evs = Vec::new();
    for _ in 0..3 {
        evs.extend(r.events());
    }
    assert_eq!(r.var("FINOGHAM"), 1);
    assert!(evs.iter().any(
        |e| matches!(e, Event::VideoStart { name, frames: Some(_) } if name == "cine003.hnm")
    ));
    r.wait(30);
    r.shot("4-film");
    r.tap();
    for _ in 0..900 {
        evs.extend(r.events());
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    let ran: Vec<usize> = evs
        .iter()
        .filter_map(|e| match e {
            Event::Set { chapter: 15, line } => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(ran, want_lines);
    lines_as_the_original(&evs);
    assert_eq!(r.e.place(), ("bigile1.wam", 106));
    let baton = r.id(defnames::OBJETS, "BATON_OGHAM");
    assert_eq!(r.e.anim.table.get(baton, 2), 1, "the stick shown");
    r.wait(10);
    r.shot("5-the-stick");

    // The stones stay solved.
    assert_eq!(r.e.ogham, OGHAM_TARGET);
}

#[test]
fn a_new_game_keeps_the_stones() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files);
    r.e.ogham = OGHAM_TARGET;
    r.e.start_new_game().unwrap();
    // FUN_0049bf70 has no Ogham step: the bytes stay.
    assert_eq!(r.e.ogham, OGHAM_TARGET);
}
