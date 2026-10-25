//! The labyrinth's souls (China, `labysol.wam` and `labyplaf.wam`, `LA`
//! views 1-60) against the original (`tests/golden/labyrinth.txt`, its
//! header there): the maps' cells and the candidates per view (rows A),
//! the visibility tests on synthetic neighbourhoods (A2), `random(n)`
//! (B), the entry per view and seed and its edge cases (C), the delay
//! alone (D), the tick alone and the warp frame pass by pass (E), a click
//! on a clickable soul (F), the dispatchers (H) and a soul's file drawn
//! (G: the faces' md5s, boxes, sampled pixels).
//!
//! Then chapter 60 played as a player plays it, with the mouse: the
//! labyrinth walked, the souls passing, the fan, the demons' stamps and
//! the buffalo's answer. With `ATLANTIS2_SHOTS` set to a folder, the
//! played test writes `pz-labyrinth-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped (rows A2 and B
//! run always).

use std::cell::RefCell;
use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{soul_cell, soul_drops, soul_open, soul_random, soul_visible};
use atlantis2_engine::{Engine, Fpu, Variant};

const GOLDEN: &str = include_str!("golden/labyrinth.txt");

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

/// The golden's rows starting with `prefix`, in order.
fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
}

fn lst<T: ToString>(xs: impl IntoIterator<Item = T>) -> String {
    let v: Vec<String> = xs.into_iter().map(|x| x.to_string()).collect();
    format!("[{}]", v.join(","))
}

/// The souls' ids (AME1..4), from the golden's `A souls` row.
fn soul_ids() -> Vec<i32> {
    rows("A souls ")[0]
        .split_whitespace()
        .skip(2)
        .map(|kv| kv.split('=').nth(1).unwrap().parse().unwrap())
        .collect()
}

/// A soul's id as the golden writes it.
fn named(id: i32) -> String {
    format!("{{0:{id}}}")
}

thread_local! {
    /// The warnings this test's thread logged.
    static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            LOGGED.with(|l| l.borrow_mut().push(r.args().to_string()));
        }
    }

    fn flush(&self) {}
}

fn capture() {
    static CAPTURE: Capture = Capture;
    if log::set_logger(&CAPTURE).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    LOGGED.with(|l| l.borrow_mut().clear());
}

/// The module's warnings logged since the last call, as the golden's
/// `warning:<text>` entries.
fn warnings() -> String {
    let w: Vec<String> = LOGGED.with(|l| {
        l.borrow_mut()
            .drain(..)
            .filter(|m| m.starts_with("pz_ames"))
            .map(|m| format!("warning:{m}"))
            .collect()
    });
    if w.is_empty() {
        "-".into()
    } else {
        w.join(";")
    }
}

/// The state whose next `rand()` is `r`.
fn forced(r: i32) -> Rand {
    // 214013⁻¹ mod 2³².
    let inv: u32 = 0xb9b3_3155;
    assert_eq!(inv.wrapping_mul(214_013), 1);
    Rand::new(((r as u32) << 16).wrapping_sub(2_531_011).wrapping_mul(inv))
}

/// The `rand()` calls from `from` to `to` (at most 10000).
fn calls(from: Rand, to: Rand) -> usize {
    let mut r = from;
    for n in 0..10_000 {
        if r == to {
            return n;
        }
        r.rand();
    }
    panic!("rand state not reached");
}

fn place_of(view: u32) -> &'static str {
    if view <= 30 {
        "labysol.wam"
    } else {
        "labyplaf.wam"
    }
}

/// A slot as the golden writes it.
fn slot_row(e: &Engine, n: usize) -> String {
    let s = &e.anim.slots[n];
    format!(
        "s{n}:{}/{}/{}{}@{} f{} e{} d{} k{} a{} p{}",
        s.id,
        s.kind,
        char::from(s.state.letter as u8),
        s.state.anim,
        s.state.view,
        s.frame,
        s.end,
        s.start,
        s.click,
        i32::from(s.active),
        i32::from(s.playing)
    )
}

fn soul_slots(e: &Engine) -> Vec<usize> {
    let ids = soul_ids();
    (0..e.anim.slots.len())
        .filter(|&n| ids.contains(&e.anim.slots[n].id))
        .collect()
}

fn souls_row(e: &Engine) -> String {
    let v: Vec<String> = soul_slots(e).iter().map(|&n| slot_row(e, n)).collect();
    if v.is_empty() {
        "none".into()
    } else {
        v.join(" ")
    }
}

/// An engine without the scripts in `place`.
struct Bare {
    e: Engine,
    place: &'static str,
}

impl Bare {
    fn new(files: A2Files) -> Bare {
        let mut e = Engine::new_game(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.trace = true;
        e.load_place("labysol.wam");
        Bare {
            e,
            place: "labysol.wam",
        }
    }

    fn place(&mut self, place: &'static str) {
        if self.place != place {
            self.e.load_place(place);
            self.place = place;
        }
    }

    /// `view` entered (in its place), `srand(seed)` before.
    fn enter(&mut self, view: u32, seed: Rand) {
        self.place(place_of(view));
        self.e.rand = seed;
        self.e.enter_at(view, 0.0, 0.0).unwrap();
    }
}

/// The soul states the visibility accepts in `view` of the engine's
/// place, as the golden names them.
fn candidates(e: &Engine, view: u32) -> Vec<String> {
    let ids = soul_ids();
    let cell = soul_cell(view as i32);
    let mut out = Vec::new();
    for ent in e
        .wam()
        .entities
        .iter()
        .filter(|x| ids.contains(&(x.index as i32)))
    {
        for s in ent.state_records() {
            if soul_visible(&cell, view as i32, &ent.prefix, s.letter, s.anim, s.view) {
                out.push(format!(
                    "{}{}{}@{}",
                    ent.prefix,
                    char::from(s.letter as u8),
                    s.anim,
                    s.view
                ));
            }
        }
    }
    out
}

#[test]
fn the_cells_and_candidates_as_the_original() {
    // The cells and the neighbourhoods the tests read.
    let cells = rows("A cell ");
    assert_eq!(cells.len(), 60);
    for (k, row) in cells.iter().enumerate() {
        let v = k as i32 + 1;
        let (ceiling, off) = soul_cell(v);
        let o = off - 1;
        let hood: String = (-2..=2)
            .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
            .map(|(dx, dy)| {
                if soul_open((ceiling, off), dx, dy) {
                    '.'
                } else {
                    '#'
                }
            })
            .collect();
        let mine = format!(
            "A cell {v} -> map {} row {} col {} hood {hood}",
            if ceiling { 0 } else { 1 },
            o / 11,
            o % 11
        );
        assert_eq!(&mine, row);
    }
    let Some(files) = game() else { return };
    let mut b = Bare::new(files);
    let rows_c = rows("A candidates ");
    assert_eq!(rows_c.len(), 60);
    for (k, row) in rows_c.iter().enumerate() {
        let v = k as u32 + 1;
        b.place(place_of(v));
        let mine = format!(
            "A candidates {v} {} -> {}",
            place_of(v),
            lst(candidates(&b.e, v))
        );
        assert_eq!(&mine, row);
    }
    // The souls' entities of the two places.
    let ids = soul_ids();
    for (place, row) in ["labysol.wam", "labyplaf.wam"]
        .into_iter()
        .zip(rows("A entities "))
    {
        b.place(place);
        let ents: Vec<String> =
            b.e.wam()
                .entities
                .iter()
                .filter(|x| ids.contains(&(x.index as i32)))
                .map(|x| {
                    let states: Vec<String> = x
                        .state_records()
                        .map(|s| {
                            format!(
                                "{}{}@{}/k{}/w{}",
                                char::from(s.letter as u8),
                                s.anim,
                                s.view,
                                s.click,
                                s.shown
                            )
                        })
                        .collect();
                    format!(
                        "{}:{}:{}:{}",
                        named(x.index as i32),
                        x.index,
                        x.prefix,
                        states.join(",")
                    )
                })
                .collect();
        assert_eq!(format!("A entities {place} {}", ents.join(" ")), *row);
    }
}

#[test]
fn the_visibility_tests_as_the_original() {
    // labyplaf.wam's soul states, as the golden's entities row lists them.
    let row = rows("A entities labyplaf.wam ")[0];
    let mut states = Vec::new();
    for ent in row.split_whitespace().skip(3) {
        let parts: Vec<&str> = ent.split(':').collect();
        let prefix = parts[3];
        for s in parts[4].split(',') {
            let (st, _) = s.split_once('/').unwrap();
            let (la, at) = st.split_once('@').unwrap();
            let letter = u32::from(la.as_bytes()[0]);
            let anim: u32 = la[1..].parse().unwrap();
            states.push((prefix.to_string(), letter, anim, at.parse::<u32>().unwrap()));
        }
    }
    assert_eq!(states.len(), 20);
    let rows_a2 = rows("A2 ");
    assert_eq!(rows_a2.len(), 403);
    for row in rows_a2 {
        let pat: [u8; 25] = row.as_bytes()[3..28].try_into().unwrap();
        let mut out = Vec::new();
        for view in [40, 10] {
            let acc: Vec<String> = states
                .iter()
                .filter(|(p, l, a, at)| soul_visible(&pat, view, p, *l, *a, *at))
                .map(|(p, l, a, at)| format!("{p}{}{a}@{at}", char::from(*l as u8)))
                .collect();
            out.push(lst(acc));
        }
        assert_eq!(format!("A2 {} -> {} {}", &row[3..28], out[0], out[1]), row);
    }
}

#[test]
fn random_as_the_original() {
    let rows_b = rows("B r ");
    assert_eq!(rows_b.len(), 7);
    for row in rows_b {
        let r: i32 = row.split_whitespace().nth(2).unwrap().parse().unwrap();
        let mut out = Vec::new();
        for n in [0, 1, 2, 3, 5, 8, 20, 34, 45, 105, 615, 855] {
            let mut g = forced(r);
            out.push(format!("{n}:{}", soul_random(&mut g, n)));
        }
        assert_eq!(format!("B r {r} -> {}", out.join(" ")), row);
    }
}

#[test]
fn the_drops_alone() {
    // Up to two candidates nothing is drawn; 3-4 drop one, 5-7 two, 8
    // and more three; a draw of `count` (rand() = 32767) is a miss.
    for (count, drops) in [
        (0, 0),
        (2, 0),
        (3, 1),
        (4, 1),
        (5, 2),
        (7, 2),
        (8, 3),
        (20, 3),
    ] {
        let mut g = Rand::new(1);
        let kept = soul_drops(&mut g, count);
        assert_eq!(kept.iter().filter(|k| !**k).count(), drops, "{count}");
    }
    let mut g = forced(32767);
    let before = g;
    let kept = soul_drops(&mut g, 4);
    assert_eq!(kept.iter().filter(|k| !**k).count(), 1);
    assert!(calls(before, g) >= 2);
}

/// The entry's row as the golden writes it.
fn entry_row(e: &Engine, seed: Rand, warn: &str) -> String {
    format!(
        "mode {} cand {} souls {} rand {} state {} slots {} | {} | warn {warn}",
        e.puzzle_mode(),
        e.souls_candidates,
        e.souls_loaded,
        calls(seed, e.rand),
        e.rand.state,
        e.anim.slots.len(),
        souls_row(e)
    )
}

#[test]
fn the_entry_as_the_original() {
    let Some(files) = game() else { return };
    capture();
    let mut b = Bare::new(files);
    let rows_c = rows("C seed ");
    assert_eq!(rows_c.len(), 180);
    for row in rows_c {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (seed, view): (u32, u32) = (w[2].parse().unwrap(), w[4].parse().unwrap());
        b.e.set_puzzle_mode(0);
        b.enter(view, Rand::new(seed));
        let mine = format!(
            "C seed {seed} view {view} -> {}",
            entry_row(&b.e, Rand::new(seed), &warnings())
        );
        assert_eq!(mine, row);
    }
    // The hook again on the view's slots.
    for (view, row) in [8, 21].into_iter().zip(rows("C again ")) {
        b.enter(view, Rand::new(1));
        warnings();
        let seed = b.e.rand;
        b.e.souls_entry(view);
        let r = entry_row(&b.e, seed, &warnings());
        // The golden has no rand state on these rows.
        let r = r.replacen(&format!(" state {}", b.e.rand.state), "", 1);
        assert_eq!(format!("C again view {view} -> {r}"), row);
    }
    // The mode before.
    for (mode, row) in [2, 5, 3].into_iter().zip(rows("C mode ")) {
        b.e.set_puzzle_mode(mode);
        b.enter(8, Rand::new(1));
        assert_eq!(
            format!(
                "C mode {mode} before, view 8 -> mode {} | {}",
                b.e.puzzle_mode(),
                souls_row(&b.e)
            ),
            row
        );
    }
    // The slot table nearly full: the view's own slots, unused ones to 33,
    // the hook alone: one soul loaded, the others refused (the original's
    // fatal error, AtlSpw_load's, the data never reach).
    for (view, row) in [8, 21].into_iter().zip(rows("C full ")) {
        b.enter(view, Rand::new(1));
        let own: Vec<usize> = (0..b.e.anim.slots.len())
            .filter(|n| !soul_slots(&b.e).contains(n))
            .collect();
        b.e.anim.slots.truncate(own.len());
        while b.e.anim.slots.len() < 33 {
            let mut s = b.e.anim.slots[0].clone();
            s.id = 0;
            s.kind = 0;
            s.record = None;
            s.start = 0;
            s.active = false;
            b.e.anim.slots.push(s);
        }
        b.e.rand = Rand::new(1);
        warnings();
        b.e.souls_entry(view);
        let tail: Vec<String> = (33..b.e.anim.slots.len())
            .map(|n| slot_row(&b.e, n))
            .collect();
        let mine = format!(
            "C full view {view} (own {}, zeroed to 33) -> cand {} souls {} rand {} slots {} | {} | warn ",
            own.len(),
            b.e.souls_candidates,
            b.e.souls_loaded,
            calls(Rand::new(1), b.e.rand),
            b.e.anim.slots.len(),
            tail.join(" ")
        );
        assert!(row.starts_with(&mine), "{mine}\n{row}");
        warnings();
    }
    // rand() = 32767 at the first drop draw: the port's miss is the
    // original's when the stack word there is not 1.
    for row in rows("C rand 32767 first, stack word ") {
        let w: Vec<&str> = row.split_whitespace().collect();
        let word: i32 = w[6].trim_end_matches(',').parse().unwrap();
        let view: u32 = w[8].parse().unwrap();
        b.enter(view, forced(32767));
        let mine = format!(
            "cand {} souls {} rand {} state {}",
            b.e.souls_candidates,
            b.e.souls_loaded,
            calls(forced(32767), b.e.rand),
            b.e.rand.state
        );
        let (_, theirs) = row.split_once(" -> ").unwrap();
        let (counts, rest) = theirs.split_once(" drop reads ").unwrap();
        let (_, slots) = rest.split_once(" | ").unwrap();
        let first_word_read = rest.contains(&format!("[{}:{word}", b.e.souls_candidates));
        if word == 1 && first_word_read {
            // The original drops nothing more: not the port's.
            assert_ne!((mine.as_str(), souls_row(&b.e).as_str()), (counts, slots));
        } else {
            assert_eq!((mine.as_str(), souls_row(&b.e).as_str()), (counts, slots));
        }
    }
}

#[test]
fn the_delay_as_the_original() {
    let Some(files) = game() else { return };
    let mut b = Bare::new(files);
    for row in rows("D ") {
        let (label, rest) = row[2..].split_once(" view ").unwrap();
        let w: Vec<&str> = rest.split_whitespace().collect();
        let view: u32 = w[0].parse().unwrap();
        let target: usize = w[2].parse().unwrap();
        b.enter(view, Rand::new(1));
        let souls = soul_slots(&b.e);
        let own: Vec<usize> = (0..b.e.anim.slots.len())
            .filter(|n| !souls.contains(n))
            .collect();
        let (a, bb, c) = (souls[0], souls.get(1).copied(), souls.get(2).copied());
        let mut seed = 7u32;
        let set = |e: &mut Engine, n: usize, d: i32| e.anim.slots[n].start = d;
        let parse_seed = |l: &str| -> Option<u32> {
            l.split("seed ")
                .nth(1)
                .map(|x| x.split([' ', ',']).next().unwrap().parse().unwrap())
        };
        if let Some(s) = parse_seed(label) {
            seed = s;
        }
        if label.starts_with("others 0") {
            set(&mut b.e, bb.unwrap(), 0);
            set(&mut b.e, c.unwrap(), 0);
        } else if let Some(d) = label.strip_prefix("one other at ") {
            set(&mut b.e, bb.unwrap(), d.parse().unwrap());
            set(&mut b.e, c.unwrap(), 0);
        } else if label.starts_with("two others (30, 75)") {
            set(&mut b.e, bb.unwrap(), 30);
            set(&mut b.e, c.unwrap(), 75);
        } else if label.starts_with("two others (20, 64)") {
            set(&mut b.e, bb.unwrap(), 20);
            set(&mut b.e, c.unwrap(), 64);
        } else if label.starts_with("six others") {
            for (j, &k) in souls[1..].iter().enumerate() {
                set(&mut b.e, k, 40 + 80 * j as i32);
            }
            b.e.anim.slots[a].end = if label.contains("end 200") { 200 } else { 0 };
        } else if let Some(end) = label.strip_prefix("end ") {
            set(&mut b.e, bb.unwrap(), 0);
            set(&mut b.e, c.unwrap(), 0);
            b.e.anim.slots[a].end = end.parse().unwrap();
            seed = 0x89ab_cdef;
        } else if label.starts_with("non-soul slot") {
            set(&mut b.e, bb.unwrap(), 0);
            set(&mut b.e, c.unwrap(), 0);
            set(&mut b.e, own[0], 30);
            if label.contains("soul's id") {
                b.e.anim.slots[own[0]].id = soul_ids()[0];
            }
        } else {
            assert!(label.starts_with("as entered"), "{label}");
        }
        assert_eq!(target, a);
        b.e.rand = Rand::new(seed);
        b.e.soul_delay(target);
        let mine = format!(
            "D {label} view {view} slot {target} souls {} -> d{} rand {} | {}",
            lst(&souls),
            b.e.anim.slots[target].start,
            calls(Rand::new(seed), b.e.rand),
            souls_row(&b.e)
        );
        assert_eq!(mine, row);
    }
}

// ---------------------------------------------------------------------------
// Chapter 60 played with the mouse.

use atlantis2_data::defnames;
use atlantis2_data::wam::Wam;
use atlantis2_data::Folder;
use atlantis2_engine::bag::{UiBank, UiDraw, FORM_FRAMES};
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{Event, LoopType};

/// The inverse of the warp's sampling of a face (as `tests/flow.rs`).
fn texel_angles(face: usize, col: f64, row: f64) -> (f32, f32) {
    let s = |t: f64| (t - 0.5) * 2.0 / 510.0 - 1.0;
    let (u, v) = (s(col), s(row));
    let [x, y, z] = match face {
        0 => [u, v, 1.0],
        1 => [-u, v, -1.0],
        2 => [u, -1.0, v],
        3 => [u, 1.0, -v],
        4 => [-1.0, v, u],
        _ => [1.0, v, -u],
    };
    (z.atan2(x) as f32, y.atan2((z * z + x * x).sqrt()) as f32)
}

/// The sprite a click would hit now (as `tests/flow.rs`).
fn centre_slot(e: &Engine) -> Option<usize> {
    let [x, y, z] = e.camera.ray(e.camera.cx, e.camera.cy).map(f64::from);
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, u, v) = if az >= ax && az >= ay {
        if z > 0.0 {
            (0, x / az, y / az)
        } else {
            (1, -x / az, y / az)
        }
    } else if ay >= ax {
        if y < 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if x < 0.0 {
        (4, z / ax, y / ax)
    } else {
        (5, -z / ax, y / ax)
    };
    let texel = |s: f64| (1.0 + 510.0 * (s + 1.0) / 2.0).floor() as i32;
    let (col, row) = (texel(u), texel(v));
    e.anim.slots.iter().position(|s| {
        s.active
            && s.click > 0
            && s.frame_box(face).is_some_and(|b| {
                let (bx, by, bw, bh) = (
                    i32::from(b.x),
                    i32::from(b.y),
                    i32::from(b.w),
                    i32::from(b.h),
                );
                row < by + bh + 1 && by < row && bx < col && col < bx + bw + 1
            })
    })
}

/// The engine at 59.94 passes a second ([`PASS`]), the 15 Hz ticks from its clock,
/// every pass drawn and a pass of sound mixed (the voices end as the
/// sound output plays them, so voice 2 is free again after a sound); the
/// mouse as a player moves it.
struct Run {
    e: Engine,
    pass: u64,
    gap: f64,
    screen: Vec<u16>,
    shots: Option<PathBuf>,
    mouse_signs: (f32, f32),
    journal: Vec<Event>,
    /// A demon (its id) and the frame F − 9 of its special event: each
    /// pass that leaves its first slot there is noted in `at_stamp`
    /// (voice 2 busy before the pass, sound 203 started by it).
    stamp: Option<(i32, i32)>,
    at_stamp: Vec<(bool, bool)>,
    sol: Wam,
    plaf: Wam,
}

impl Run {
    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
        let busy = self.e.voices().is_playing(2);
        let n = self.e.events.len();
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        if let Some((id, frame)) = self.stamp {
            if self
                .e
                .anim
                .slots
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.frame)
                == Some(frame)
            {
                let s203 = self.e.events[n..].contains(&Event::Sound("sound203.apc".into()));
                self.at_stamp.push((busy, s203));
            }
        }
        let mut sound = [0.0f32; 2 * 22050 / 60];
        self.e.voices().fill(&mut sound, 22050, 2);
        self.pass += 1;
        let fatal = self.e.script_state().and_then(|s| s.fatal.clone());
        assert!(fatal.is_none(), "{fatal:?}");
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    fn take_events(&mut self) -> Vec<Event> {
        let evs = std::mem::take(&mut self.e.events);
        self.journal.extend(evs.iter().cloned());
        evs
    }

    fn events(&mut self) -> Vec<Event> {
        self.step();
        self.take_events()
    }

    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = &self.shots {
            std::fs::create_dir_all(dir).unwrap();
            let png = atlantis2_engine::png::encode_rgb565(&self.screen, 640, 480);
            std::fs::write(dir.join(format!("pz-labyrinth-{name}.png")), png).unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        self.e.script_state().unwrap().var(name).unwrap()
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.id(section, name.as_bytes()).unwrap() as i32
    }

    fn subject(&self, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.param(defnames::SUJETS, name.as_bytes())
    }

    /// Turns the view with the mouse (as `tests/flow.rs`'s `steer`).
    fn steer(&mut self, yaw: f32, pitch: f32, on: &dyn Fn(&Engine) -> bool) -> bool {
        let wrap = |a: f32| {
            let t = std::f32::consts::TAU;
            (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
        };
        let (mut sx, mut sy) = self.mouse_signs;
        for k in 0..24 {
            if k > 0 && on(&self.e) {
                self.wait(24);
                if on(&self.e) {
                    self.mouse_signs = (sx, sy);
                    return true;
                }
            }
            let (y0, p0) = self.e.camera.angles();
            let (dy, dp) = (wrap(yaw - y0), pitch - p0);
            let mx = (sx * dy * 500.0).round() as i32;
            let my = (sy * dp * 500.0).round() as i32;
            if mx == 0 && my == 0 {
                break;
            }
            self.e.mouse_motion(mx, my);
            self.wait(30);
            let (y1, p1) = self.e.camera.angles();
            if mx != 0 && wrap(yaw - y1).abs() > dy.abs() {
                sx = -sx;
            }
            if my != 0 && (pitch - p1).abs() > dp.abs() {
                sy = -sy;
            }
        }
        self.mouse_signs = (sx, sy);
        self.wait(24);
        on(&self.e)
    }

    /// The centre on zone `zone` with no clickable sprite under it.
    fn try_aim(&mut self, zone: usize) -> bool {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        let on = move |e: &Engine| {
            e.centre_zone().map(|(i, _)| i) == Some(zone) && e.sprite_under_centre().1 == 0
        };
        let off = 0.6f32.to_radians();
        let mut targets = vec![
            (yaw + off, pitch),
            (yaw - off, pitch),
            (yaw, pitch + off),
            (yaw, pitch - off),
        ];
        let fpu = self.e.fpu();
        for p in &self.e.wam().view(view).unwrap().zones[zone].points {
            let (y, q) = atlantis2_engine::camera::direction_angles(fpu, *p);
            let dy = (y - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            targets.push((yaw + dy / 2.0, (pitch + q) / 2.0));
        }
        targets.into_iter().any(|(y, p)| self.steer(y, p, &on))
    }

    /// A click on a move zone of kind `kind` (1 a view of the place, 2 a
    /// view of another) to view `to`, the transition video clicked away,
    /// until the arrival in `(place, to)`. The events, in order.
    fn walk(&mut self, kind: i32, place: &str, to: u32) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let zones: Vec<usize> = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.kind == kind && z.value == to as i32)
            .map(|(i, _)| i)
            .collect();
        assert!(!zones.is_empty(), "no move from view {view} to {to}");
        assert!(
            zones.iter().any(|&z| self.try_aim(z)),
            "no move zone to view {to} within reach in view {view}"
        );
        self.tap();
        let mut all = Vec::new();
        for _ in 0..1200 {
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
            all.extend(evs);
            if video {
                self.wait(15);
                self.tap();
            }
            if self.e.place() == (place, to)
                && self.e.loop_type != LoopType::Video
                && !self.e.turning()
            {
                self.wait(4);
                all.extend(self.take_events());
                if self.e.loop_type == LoopType::Dialog {
                    all.extend(self.listen());
                }
                assert_eq!(self.e.loop_type, LoopType::Warp);
                return all;
            }
        }
        panic!("no arrival in {place} view {to}");
    }

    /// The labyrinth's views reachable from the current one, by the
    /// fewest moves (the two places' move zones and their link), to
    /// `(place, to)`: walked.
    fn go(&mut self, place: &'static str, to: u32) -> Vec<Event> {
        let start = (self.e.place().0.to_string(), self.e.place().1);
        let goal = (place.to_string(), to);
        let mut prev: std::collections::HashMap<(String, u32), (String, u32, i32)> =
            std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::from([start.clone()]);
        let mut seen = std::collections::HashSet::from([start.clone()]);
        while let Some(node) = queue.pop_front() {
            if node == goal {
                break;
            }
            let wam = if node.0 == "labysol.wam" {
                &self.sol
            } else {
                &self.plaf
            };
            let Some(v) = wam.view(node.1) else { continue };
            for z in &v.zones {
                let next = match z.kind {
                    1 if wam.view(z.value as u32).is_some() => (node.0.clone(), z.value as u32),
                    2 if node == ("labysol.wam".into(), 28) && z.value == 33 => {
                        ("labyplaf.wam".to_string(), 33)
                    }
                    2 if node == ("labyplaf.wam".into(), 33) && z.value == 28 => {
                        ("labysol.wam".to_string(), 28)
                    }
                    _ => continue,
                };
                if seen.insert(next.clone()) {
                    prev.insert(next.clone(), (node.0.clone(), node.1, z.kind));
                    queue.push_back(next);
                }
            }
        }
        let mut path = Vec::new();
        let mut at = goal.clone();
        while at != start {
            let (p, v, kind) = prev
                .get(&at)
                .unwrap_or_else(|| panic!("no way to {goal:?}"))
                .clone();
            path.push((kind, at.clone()));
            at = (p, v);
        }
        path.reverse();
        let mut all = Vec::new();
        for (kind, (p, v)) in path {
            let p: &'static str = if p == "labysol.wam" {
                "labysol.wam"
            } else {
                "labyplaf.wam"
            };
            all.extend(self.walk(kind, p, v));
        }
        all
    }

    /// The running dialog played to its end, a click every 20 passes.
    fn listen(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        for n in 0..20_000 {
            if n % 20 == 19 {
                self.tap();
            }
            let evs = self.events();
            let end = evs.contains(&Event::DialogEnd);
            all.extend(evs);
            if end {
                self.wait(4);
                return all;
            }
        }
        panic!("the dialog does not end");
    }

    /// Where a player looks to have slot `n`'s sprite under the centre.
    fn slot_aims(&self, n: usize) -> Vec<(f32, f32)> {
        let slots = &self.e.anim.slots;
        let mut out = Vec::new();
        for face in 0..6 {
            let Some(b) = slots[n].frame_box(face) else {
                continue;
            };
            if b.w == 0 || b.h == 0 {
                continue;
            }
            let mut texels = Vec::new();
            for i in 1..8 {
                for j in 1..8 {
                    let col = i32::from(b.x) + 1 + (i32::from(b.w) - 1) * i / 8;
                    let row = i32::from(b.y) + 1 + (i32::from(b.h) - 1) * j / 8;
                    let d = (i - 4) * (i - 4) + (j - 4) * (j - 4);
                    texels.push((d, col, row));
                }
            }
            texels.sort();
            for (_, col, row) in texels {
                out.push(texel_angles(face, f64::from(col), f64::from(row)));
            }
        }
        out.truncate(12);
        out
    }

    /// A click on the shown sprite of character `id`: the events. Its
    /// animation moves it (and may hand over to another of its slots):
    /// the slots shown are looked at again after each try.
    fn click_character(&mut self, id: i32) -> Vec<Event> {
        for _ in 0..8 {
            let shown: Vec<usize> = (0..self.e.anim.slots.len())
                .filter(|&k| {
                    let s = &self.e.anim.slots[k];
                    s.id == id && s.active && s.click == 2
                })
                .collect();
            for n in shown {
                for (yaw, pitch) in self.slot_aims(n) {
                    let on = |e: &Engine| {
                        e.sprite_under_centre().1 == 2
                            && centre_slot(e).is_some_and(|k| e.anim.slots[k].id == id)
                    };
                    if self.steer(yaw, pitch, &on) {
                        self.tap();
                        let mut evs = self.events();
                        evs.extend(self.events());
                        return evs;
                    }
                }
            }
            self.wait(15);
        }
        let dump: Vec<String> = (0..self.e.anim.slots.len())
            .map(|k| {
                let s = &self.e.anim.slots[k];
                let boxes: Vec<String> = (0..6)
                    .filter_map(|f| {
                        s.frame_box(f)
                            .map(|b| format!("{f}:{},{},{},{}", b.x, b.y, b.w, b.h))
                    })
                    .collect();
                format!("{} {}", slot_row(&self.e, k), boxes.join(" "))
            })
            .collect();
        panic!(
            "character {id} never under the centre in view {}: {dump:#?} angles {:?}",
            self.e.place().1,
            self.e.camera.angles()
        );
    }

    /// The bag bar opened with the right button, scrolled until object
    /// `id` shows, its cell clicked; the bar closed.
    fn choose(&mut self, id: i32) {
        if self.e.bar.open {
            self.e.right_click();
            self.step();
        }
        self.e.right_click();
        assert!(self.e.bar.open);
        self.step();
        let mut right = true;
        for _ in 0..400 {
            let bag = self.e.bag_entries();
            if let Some(k) = self.e.bar.shown.iter().position(|&n| bag[n].id == id) {
                self.point(40 + 80 * k as i32, 440);
                self.step();
                self.tap();
                self.step();
                assert!(!self.e.bar.open);
                return;
            }
            let first = self.e.bar.first;
            if right {
                self.point(636, 440);
            } else {
                self.point(1, 440);
            }
            self.wait(8);
            if self.e.bar.first == first {
                assert!(right, "object {id} not in the bag");
                right = false;
            }
        }
        panic!("object {id} never shown in the bar");
    }

    /// The `form.spr` frames the overlay draws now, in order.
    fn form_frames(&mut self) -> Vec<(i32, i32, i32)> {
        self.step();
        self.e
            .ui_draws
            .iter()
            .filter_map(|d| match d {
                UiDraw::Sprite {
                    bank: UiBank::Form,
                    frame,
                    x,
                    y,
                } => Some((*frame, *x, *y)),
                _ => None,
            })
            .collect()
    }

    /// The overlay closed with a click.
    fn close_overlay(&mut self) {
        self.tap();
        self.step();
        self.step();
        assert!(!self.e.overlays.any());
    }
}

fn passages(evs: &[Event]) -> Vec<(usize, bool)> {
    evs.iter()
        .filter_map(|e| match e {
            Event::SoulPassage { slot, start } => Some((*slot, *start)),
            _ => None,
        })
        .collect()
}

fn lines(evs: &[Event], chapter: i32) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: c, line } if *c == chapter => Some(*line),
            _ => None,
        })
        .collect()
}

fn dialogs(evs: &[Event]) -> Vec<u32> {
    evs.iter()
        .filter_map(|e| match e {
            Event::DialogStart {
                number,
                started: true,
            } => Some(*number),
            _ => None,
        })
        .collect()
}

/// The demons by their number on the form (`essai_labyN`, chapter 60):
/// the character, its place and the view where it takes the form.
/// The frame F that ends each demon's special event, in `DEMONS`' order
/// (the special events 1..6 of FUN_00484880, its tables built
/// @ 0x004848a7..0x00484a1c).
const STAMP_FRAMES: [i32; 6] = [70, 75, 60, 76, 70, 70];

const DEMONS: [(&str, &str, u32); 6] = [
    ("DEMON_DOGUE", "labysol.wam", 12),
    ("DEMON_COCHON", "labysol.wam", 19),
    ("DEMON_RAT", "labysol.wam", 25),
    ("DEMON_LAPIN", "labyplaf.wam", 57),
    ("DEMON_CHEVRE", "labyplaf.wam", 51),
    ("DEMON_CHEVAL", "labyplaf.wam", 33),
];

#[test]
fn the_labyrinth_played_with_the_mouse() {
    let Some(files) = game() else { return };
    let wam = |name: &str| Wam::parse(&files.read(Folder::Wam, name).unwrap()).unwrap();
    let (sol, plaf) = (wam("labysol.wam"), wam("labyplaf.wam"));
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // China as chapter 59 leaves it at the labyrinth's door: the
    // immortal's shadow asked about (`flag169`), the fox woman met in
    // chapter 56 (`flag164`); `miniature.wam` view 123.
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=miniature.wam)",
        b"(vue=123)",
        b"(chapter=59)",
        b"(flag169=1)",
        b"(flag164=1)",
    ] {
        e.run_command(c);
    }
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
    e.trace = true;
    e.events.clear();
    let mut r = Run {
        e,
        pass: 0,
        gap: 0.0,
        screen: vec![0; 640 * 480],
        shots: std::env::var_os("ATLANTIS2_SHOTS").map(PathBuf::from),
        mouse_signs: (1.0, -1.0),
        journal: Vec::new(),
        stamp: None,
        at_stamp: Vec::new(),
        sol,
        plaf,
    };
    r.wait(20);
    assert_eq!(r.e.place(), ("miniature.wam", 123));
    r.shot("1-door");

    // Into the labyrinth (the link to `labysol.wam` view 3): chapter 59's
    // arrival section sends the game to chapter 60, whose `/INIT` starts
    // music 20 and picks the solution row by `random`.
    let evs = r.walk(2, "labysol.wam", 3);
    assert_eq!(r.e.script_state().unwrap().chapter, 60);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0048_df10
    }));
    assert_eq!(r.e.puzzle_mode(), 3);
    let ids = soul_ids();
    let souls: Vec<usize> = soul_slots(&r.e);
    assert_eq!(souls.len() as i32, r.e.souls_loaded);
    assert!(!souls.is_empty(), "view 3 sees souls");
    for &n in &souls {
        let s = &r.e.anim.slots[n];
        assert!(!s.active && !s.playing && s.start >= 10);
    }
    let soluce = r.var("soluce_laby");
    assert!((1..=10).contains(&soluce));

    // A soul passes: shown after its delay, sound 220, hidden at its last
    // frame with a new delay.
    let mut evs = Vec::new();
    let mut seen = false;
    for _ in 0..3000 {
        evs.extend(r.events());
        if let Some(&(slot, _)) = passages(&evs).iter().find(|p| p.1).filter(|_| !seen) {
            seen = true;
            // Turned to it with the mouse.
            r.wait(4);
            if let Some(&(yaw, pitch)) = r.slot_aims(slot).first() {
                let near = move |e: &Engine| {
                    let (y, p) = e.camera.angles();
                    let d = (y - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    d.abs() < 0.05 && (p - pitch).abs() < 0.05
                };
                r.steer(yaw, pitch, &near);
            }
            r.shot("2-soul");
        }
        if passages(&evs).iter().any(|p| !p.1) {
            break;
        }
    }
    let p = passages(&evs);
    assert!(p.len() >= 2, "{p:?}");
    let (slot, _) = p[0];
    assert!(ids.contains(&r.e.anim.slots[slot].id));
    assert!(evs.contains(&Event::Sound("sound220.apc".into())));
    // `sound221.apc` is on no disc: nothing heard at the end.
    assert!(!evs
        .iter()
        .any(|x| *x == Event::Sound("sound221.apc".into())));

    // The buffalo asked about the mushroom: dialog 316, the empty form in
    // hand.
    let buffle = r.id(defnames::PERSONNAGES, "DEMON_BUFFLE");
    let evs = r.click_character(buffle);
    let count = r.e.script_state().unwrap().subjects.list(buffle).len();
    assert!(evs.contains(&Event::SubjectMenu {
        character: buffle,
        count: count as i32
    }));
    let mushroom = r.subject("CHAMPIGNON");
    let list = r.e.script_state().unwrap().subjects.list(buffle);
    let i = list.iter().position(|x| x.value == mushroom).unwrap();
    let (x, y) = cell(list.len() as i32, i as i32);
    r.point(x + 31, y + 31);
    r.step();
    r.tap();
    let mut evs = r.events();
    evs.extend(r.listen());
    assert_eq!(dialogs(&evs), [316]);
    let empty = r.id(defnames::OBJETS, "FORMULAIRE_VIDE");
    assert_eq!(r.var("OBJETENMAIN"), empty);
    r.shot("3-form");

    // The fox woman in view 21 gives the fan (dialog 345).
    r.go("labysol.wam", 21);
    let fox = r.id(defnames::PERSONNAGES, "FEMME_RENARDE");
    // Clicked with an empty hand: the form put back in the bag first (the
    // bar opened and closed).
    r.e.right_click();
    r.step();
    r.e.right_click();
    r.step();
    let mut evs = r.click_character(fox);
    evs.extend(r.listen());
    assert!(dialogs(&evs).contains(&345), "{:?}", dialogs(&evs));
    let fan = r.id(defnames::OBJETS, "EVENTAIL");
    assert!(r.e.bag_entries().iter().any(|b| b.id == fan));

    // The fan read: its three symbols are the demons' stamps in order.
    r.choose(fan);
    let frames = r.form_frames();
    assert_eq!(frames[0].0, 7);
    let order: Vec<usize> = frames[1..]
        .iter()
        .map(|&(f, _, _)| (1..=6).find(|&v| FORM_FRAMES[v] == f).unwrap())
        .collect();
    assert_eq!(order.len(), 3);
    let row = 4 + 3 * soluce as usize;
    assert_eq!(
        order.iter().map(|&v| v as i32).collect::<Vec<_>>(),
        FORM_FRAMES[row..row + 3].to_vec()
    );
    r.shot("4-fan");
    r.close_overlay();

    // Each demon in the fan's order stamps the form it is given.
    let forms = [
        "FORMULAIRE_VIDE",
        "FORMULAIRE_1",
        "FORMULAIRE_2",
        "FORMULAIRE_3",
    ];
    let mut stamps_heard = 0;
    for (k, &v) in order.iter().enumerate() {
        let (name, place, view) = DEMONS[v - 1];
        r.go(place, view);
        let form = r.id(defnames::OBJETS, forms[k]);
        if r.var("OBJETENMAIN") != form {
            // A form picked from the bar opens its overlay; the click that
            // closes it gives it back to the hand.
            r.choose(form);
            assert!(r.e.overlays.any());
            r.close_overlay();
        }
        assert_eq!(r.var("OBJETENMAIN"), form);
        let demon = r.id(defnames::PERSONNAGES, name);
        r.stamp = Some((demon, STAMP_FRAMES[v - 1] - 9));
        r.at_stamp.clear();
        let mut evs = r.click_character(demon);
        // The stamp's animation, then the stamped form and the dialog.
        let next = r.id(defnames::OBJETS, forms[k + 1]);
        for _ in 0..6000 {
            if r.e.loop_type == LoopType::Dialog {
                evs.extend(r.listen());
            }
            if r.var("OBJETENMAIN") == next && r.e.loop_type == LoopType::Warp {
                break;
            }
            evs.extend(r.events());
        }
        assert_eq!(r.var("OBJETENMAIN"), next, "{name}");
        assert_eq!(r.var(&format!("essai_laby{}", k + 1)), v as i32);
        // The stamp's sound 203 at frame F − 9 exactly when voice 2 is
        // idle then (the passes mix their sound, so a voice ends as it is
        // played); a soul's passage (sound 220) heard before keeps it busy.
        r.stamp = None;
        let heard = evs.contains(&Event::Sound("sound203.apc".into()));
        let at = std::mem::take(&mut r.at_stamp);
        assert!(!at.is_empty(), "{name}");
        assert_eq!(heard, at.iter().any(|&(busy, _)| !busy), "{name} {at:?}");
        assert_eq!(heard, at.iter().any(|&(_, s)| s), "{name} {at:?}");
        if !heard {
            assert!(evs.contains(&Event::Sound("sound220.apc".into())), "{name}");
        }
        stamps_heard += usize::from(heard);
        r.wait(10);
        r.shot(&format!("5-stamp-{}", k + 1));
    }
    // This run hears the stamp once (the passages are the engine's rand).
    assert_eq!(stamps_heard, 1);
    // The stamped form read.
    let third = r.id(defnames::OBJETS, "FORMULAIRE_3");
    r.choose(third);
    let frames = r.form_frames();
    assert_eq!(
        frames[1..].iter().map(|f| f.0).collect::<Vec<_>>(),
        order.iter().map(|&v| FORM_FRAMES[v]).collect::<Vec<_>>()
    );
    r.shot("6-form-stamped");
    r.close_overlay();

    // Back to the buffalo with the third form: the right solution, dialog
    // 322, the mushroom in hand, `flag173` = 1.
    r.go("labysol.wam", 3);
    if r.var("OBJETENMAIN") != third {
        // The arrival's hand is `Objetenmain_NextWarp`: the form picked
        // again, its overlay closed with a click.
        r.choose(third);
        r.close_overlay();
    }
    assert_eq!(r.var("OBJETENMAIN"), third);
    let mut evs = r.click_character(buffle);
    evs.extend(r.listen());
    for _ in 0..20 {
        evs.extend(r.events());
    }
    assert_eq!(dialogs(&evs), [322]);
    assert_eq!(r.e.script_state().unwrap().flags[173], 1);
    assert_eq!(r.var("OBJETENMAIN"), r.id(defnames::OBJETS, "CHAMPIGNON"));
    let set = lines(&evs, 60);
    let first = [126, 143, 158, 173, 188, 203, 218, 233, 248, 263][soluce as usize - 1];
    // The solution's section (its `/con` line `first`): its lines run.
    assert!(
        set.iter().filter(|&&l| l > first && l < first + 15).count() >= 5,
        "{set:?}"
    );
    assert!(!r.e.bag_entries().iter().any(|b| b.id == third));
    r.wait(10);
    r.shot("7-won");
    // The souls passed on the way, in every view.
    let all = passages(&r.journal);
    assert!(all.iter().filter(|p| p.1).count() >= 2, "{all:?}");
}

// ---------------------------------------------------------------------------
// The tick, the dispatchers, the warp frame, a click on a soul, the drawing.

use atlantis2_data::spw::Spw;
use atlantis2_data::wam::EntityState;
use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::chinese_entry_hooks;
use md5::{Digest, Md5};

/// The commands the souls ran, from the events.
fn commands(evs: &[Event]) -> String {
    lst(passages(evs).into_iter().map(|(_, start)| {
        if start {
            "(NewSound=220)"
        } else {
            "(NewSound=221)"
        }
    }))
}

#[test]
fn the_tick_alone_as_the_original() {
    let Some(files) = game() else { return };
    let mut b = Bare::new(files);
    let rows_t = rows("T ");
    assert_eq!(rows_t.len(), 2 * 7 + 5);
    for row in rows_t {
        let (label, rest) = row[2..]
            .split_once(" -> ")
            .unwrap()
            .0
            .rsplit_once(" view ")
            .unwrap();
        let view: u32 = rest.parse().unwrap();
        b.enter(view, Rand::new(1));
        b.e.set_puzzle_mode(3);
        let k = *soul_slots(&b.e).last().unwrap();
        let end = b.e.anim.slots[k].end;
        let mut cur = view;
        {
            let s = &mut b.e.anim.slots[k];
            match label {
                "delay 5" => s.start = 5,
                "delay 1" => s.start = 1,
                "delay 0, frame 0" => (s.start, s.frame) = (0, 0),
                "delay 0, frame end - 1" => (s.start, s.frame) = (0, end - 1),
                "delay 0, frame end - 2" => (s.start, s.frame) = (0, end - 2),
                "delay -3, frame 2" => (s.start, s.frame) = (-3, 2),
                "delay 0, frame -1" => (s.start, s.frame) = (0, -1),
                l => {
                    cur = l
                        .strip_prefix("current view number ")
                        .unwrap()
                        .parse()
                        .unwrap();
                    s.start = 5;
                }
            }
        }
        b.e.rand = Rand::new(3);
        b.e.events.clear();
        b.e.souls_tick(cur);
        let mine = format!(
            "T {label} view {view} -> cmds {} rand {} | {} | mode {} | warn -",
            commands(&b.e.events),
            calls(Rand::new(3), b.e.rand),
            slot_row(&b.e, k),
            b.e.puzzle_mode()
        );
        assert_eq!(mine, row);
    }
}

#[test]
fn the_dispatchers_as_the_original() {
    let names = |hooks: Vec<u32>| -> String {
        let v: Vec<&str> = hooks
            .iter()
            .filter_map(|h| match h {
                0x0048_df10 => Some("souls-entry"),
                0x0049_c340 => Some("ring-entry"),
                _ => None,
            })
            .collect();
        if v.is_empty() {
            "-".into()
        } else {
            v.join(",")
        }
    };
    let entries = rows("H entry ");
    assert_eq!(entries.len(), 14);
    let mut b = game().map(Bare::new);
    for row in entries {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (name, number): (&str, u32) = (w[2], w[3].parse().unwrap());
        let hooks = chinese_entry_hooks(name.as_bytes(), number);
        let got = names(hooks.clone());
        assert!(row.contains(&format!("-> {got} |")), "{row}");
        // The souls' entry on an empty slot table, seed 1.
        if let (Some(b), true) = (b.as_mut(), hooks.contains(&0x0048_df10)) {
            b.place("labysol.wam");
            b.e.anim.slots.clear();
            b.e.set_puzzle_mode(0);
            b.e.rand = Rand::new(1);
            b.e.souls_entry(number);
            assert!(
                row.ends_with(&format!("| mode 3 souls {}", b.e.souls_loaded)),
                "{row}"
            );
        }
    }
    let Some(mut b) = b else { return };
    // The tick dispatcher with mode 3 (the pause byte is not in the port)
    // and 0; the ring's (mode 2) is another puzzle's.
    for row in rows("H tick mode ") {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (mode, pause, view): (i32, i32, u32) = (
            w[3].parse().unwrap(),
            w[5].parse().unwrap(),
            w[7].parse().unwrap(),
        );
        if pause != 0 || mode == 2 {
            continue;
        }
        b.enter(8, Rand::new(1));
        b.e.set_puzzle_mode(mode);
        b.e.events.clear();
        if view == 8 {
            b.e.puzzle_tick();
        } else if mode == 3 {
            b.e.souls_tick(view);
        }
        let after = format!("| mode {} | cmds [] | warn -", b.e.puzzle_mode());
        assert!(row.ends_with(&after), "{row}");
    }
    // The quit, through the tick outside views 1-60.
    for row in rows("H quit mode ") {
        let mode: i32 = row.split_whitespace().nth(3).unwrap().parse().unwrap();
        b.e.set_puzzle_mode(mode);
        b.e.souls_tick(0);
        assert_eq!(
            format!("H quit mode {mode} -> mode {}", b.e.puzzle_mode()),
            row
        );
    }
}

/// A game in chapter 60 in `place` view `view`: a new game's film ended,
/// then the world, the place, the view and the chapter.
fn in_labyrinth(files: A2Files, place: &str, view: u32) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        "(monde=3)".to_string(),
        format!("(changewam={place})"),
        format!("(vue={view})"),
        "(chapter=60)".to_string(),
    ] {
        e.run_command(c.as_bytes());
    }
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
    e.trace = true;
    e
}

/// The souls' slots as the frame rows compare them.
fn soul_state(e: &Engine) -> Vec<(i32, bool, bool, i32)> {
    soul_slots(e)
        .iter()
        .map(|&n| {
            let s = &e.anim.slots[n];
            (s.frame, s.active, s.playing, s.click)
        })
        .collect()
}

#[test]
fn the_warp_frame_as_the_original() {
    let Some(files) = game() else { return };
    let golden = rows("E ");
    let runs: Vec<usize> = (0..golden.len())
        .filter(|&i| golden[i].starts_with("E run "))
        .collect();
    assert_eq!(runs.len(), 3);
    let mut e: Option<Engine> = None;
    let mut files = Some(files);
    for (r, &at) in runs.iter().enumerate() {
        let head: Vec<&str> = golden[at].split_whitespace().collect();
        let (label, place, view, seed): (&str, &str, u32, u32) = (
            head[2],
            head[3],
            head[5].parse().unwrap(),
            head[7].parse().unwrap(),
        );
        let end = runs.get(r + 1).copied().unwrap_or(golden.len());
        let mut want: Vec<&str> = golden[at..end].to_vec();
        let last = want.pop().unwrap();
        assert!(last.starts_with(&format!("E end {label} ")));
        let passes: usize = if r == 2 { 400 } else { 1600 };
        let eng = match e.take() {
            Some(mut x) if x.place().0 == place => {
                x.run_command(format!("(vue={view})").as_bytes());
                x
            }
            Some(x) => {
                let mut x = x;
                x.run_command(format!("(changewam={place})").as_bytes());
                x.run_command(format!("(vue={view})").as_bytes());
                x
            }
            None => in_labyrinth(files.take().unwrap(), place, view),
        };
        let mut eng = eng;
        for _ in 0..8 {
            eng.pass(None);
        }
        eng.rand = Rand::new(seed);
        eng.enter_at(view, 0.0, 0.0).unwrap();
        eng.events.clear();
        let mut got = vec![format!(
            "E run {label} {place} view {view} seed {seed} -> {}",
            souls_row(&eng)
        )];
        let mut prev = None;
        let t0 = eng.ticks;
        for n in 0..passes {
            if n % 4 == 0 {
                eng.tick();
            }
            eng.pass(None);
            let evs = std::mem::take(&mut eng.events);
            let cur = soul_state(&eng);
            let cmds = commands(&evs);
            if Some(&cur) != prev.as_ref() || cmds != "[]" || n % 100 == 99 {
                got.push(format!(
                    "E p{n} t{} | {} | {cmds}",
                    eng.ticks.wrapping_sub(t0),
                    souls_row(&eng)
                ));
            }
            prev = Some(cur);
        }
        assert_eq!(got, want, "run {label}");
        assert!(
            last.starts_with(&format!("E end {label} rand state {} ", eng.rand.state)),
            "{last}"
        );
        e = Some(eng);
    }
}

#[test]
fn a_soul_is_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let bg: Vec<u16> = (0..512u64 * 512)
        .map(|i| (((i * 2_654_435_761) & 0xffff_ffff) >> 16) as u16)
        .collect();
    let md5_of = |px: &[u16]| -> String {
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        format!("{:x}", Md5::digest(&bytes))
    };
    let bg_md5 = md5_of(&bg);
    let golden = rows("G ");
    assert_eq!(golden.len(), 20 + 20 * 7);
    let mut got = Vec::new();
    let mut data = Vec::new();
    for row in &golden {
        if let Some(rest) = row.strip_prefix("G file ") {
            let name = rest.split(' ').next().unwrap();
            data = files.read(Folder::Spw, name).expect("the soul's SPW");
            let spw = Spw::parse(data.clone()).unwrap();
            got.push(format!(
                "G file {name} md5 {:x} frames {}",
                Md5::digest(&data),
                spw.frame_count()
            ));
            continue;
        }
        let w: Vec<&str> = row.split(' ').collect();
        let (name, frame): (&str, i32) = (w[1], w[3].parse().unwrap());
        let spw = Spw::parse(data.clone()).unwrap();
        let mut a = Anim::new(0);
        a.load(
            name,
            Spw::parse(data.clone()).unwrap(),
            1,
            100,
            0,
            EntityState {
                letter: 65,
                anim: 1,
                shown: 1,
                single: 1,
                first: frame,
                ..Default::default()
            },
        );
        a.slots[0].frame = frame;
        let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
        let (_, hit) = a.draw(&mut faces, None, true);
        let al = i32::from(hit.slot.is_some());
        let mut parts = Vec::new();
        for (k, f) in faces.iter().enumerate() {
            let b = spw.face_box(frame, k).unwrap_or_default();
            let (x, y, bw, bh) = (
                i32::from(b.x),
                i32::from(b.y),
                i32::from(b.w),
                i32::from(b.h),
            );
            let bx = format!("box {x},{y},{bw},{bh}");
            let h = md5_of(f);
            if h == bg_md5 {
                parts.push(format!("f{k} bg {bx}"));
                continue;
            }
            let n = f.iter().zip(&bg).filter(|(a, b)| a != b).count();
            let mut samples = Vec::new();
            if bw > 0 && bh > 0 {
                for (px, py) in [(x + bw / 2, y + bh / 2), (x, y), (x + bw - 1, y + bh - 1)] {
                    if (0..512).contains(&px) && (0..512).contains(&py) {
                        samples.push(format!("{px},{py}={:04x}", f[(py * 512 + px) as usize]));
                    }
                }
            }
            parts.push(format!("f{k} {h} n{n} {bx} {}", samples.join(" ")));
        }
        got.push(format!(
            "G {name} frame {frame} AL {al} | {}",
            parts.join(" | ")
        ));
    }
    assert_eq!(got, golden);
}

/// The dialogs a row of rows F lists.
fn dialogs_of(row: &str) -> Vec<u32> {
    let list = row
        .rsplit("dialogs [")
        .next()
        .unwrap()
        .trim_end_matches(']');
    list.split(',')
        .filter(|x| !x.is_empty())
        .map(|x| x.parse().unwrap())
        .collect()
}

/// The box of frame `frame` of slot `n`'s file on the first face that has
/// one: (face, x, y, w, h).
fn first_box(e: &mut Engine, n: usize, frame: i32) -> Option<(usize, i32, i32, i32, i32)> {
    let keep = e.anim.slots[n].frame;
    e.anim.slots[n].frame = frame;
    let b = (0..6).find_map(|face| {
        e.anim.slots[n]
            .frame_box(face)
            .filter(|b| b.w > 0 && b.h > 0)
            .map(|b| {
                (
                    face,
                    i32::from(b.x),
                    i32::from(b.y),
                    i32::from(b.w),
                    i32::from(b.h),
                )
            })
    });
    e.anim.slots[n].frame = keep;
    b
}

#[test]
fn a_click_on_a_soul_as_the_original() {
    let Some(files) = game() else { return };
    let golden = rows("F ");
    let heads: Vec<usize> = (0..golden.len())
        .filter(|&i| golden[i].starts_with("F labyplaf.wam view "))
        .collect();
    assert_eq!(heads.len(), 8);
    let mut e = in_labyrinth(files, "labyplaf.wam", 35);
    let mut gap = 0.0;
    for (h, &at) in heads.iter().enumerate() {
        let end = heads.get(h + 1).copied().unwrap_or(golden.len());
        let head = golden[at];
        let (case, _) = head.split_once(": ").unwrap();
        let w: Vec<&str> = case.split_whitespace().collect();
        let (view, late, keep): (u32, i32, bool) =
            (w[3].parse().unwrap(), w[5].parse().unwrap(), w.len() > 6);
        e.run_command(format!("(vue={view})").as_bytes());
        for _ in 0..8 {
            e.pass(None);
        }
        e.forced_centre = None;
        e.rand = Rand::new(1);
        e.enter_at(view, 0.0, 0.0).unwrap();
        e.events.clear();
        let k = soul_slots(&e)
            .into_iter()
            .find(|&n| e.anim.slots[n].state.anim == 3)
            .expect("an animation-3 soul");
        let mut got = Vec::new();
        let mut act = None;
        let mut dialog = false;
        for n in 0..4000i32 {
            if n % 4 == 0 {
                e.tick();
            }
            if keep && act.is_some() {
                e.anim.slots[k].click = 2;
            }
            let click = act.is_some_and(|a| n == a + late);
            if click {
                gap += 10.0;
                e.click(gap);
            }
            e.pass(None);
            let evs = std::mem::take(&mut e.events);
            if act.is_none() && e.anim.slots[k].active {
                act = Some(n);
                let boxes: Vec<String> = (0..5)
                    .map(|f| match first_box(&mut e, k, f) {
                        Some((face, x, y, bw, bh)) => format!("{f}:{face}:{x},{y},{bw},{bh}"),
                        None => format!("{f}:-"),
                    })
                    .collect();
                let (face, x, y, bw, bh) = (0..40).find_map(|f| first_box(&mut e, k, f)).unwrap();
                let texel = (face, x + bw / 2, y + bh / 2);
                e.forced_centre = Some(texel);
                got.push(format!(
                    "{case}: slot {k} active at pass {n} | {} | boxes {} | texel ({}, {}, {})",
                    slot_row(&e, k),
                    boxes.join(" "),
                    texel.0,
                    texel.1,
                    texel.2
                ));
                continue;
            }
            let Some(a) = act else { continue };
            let scans: Vec<i32> = evs
                .iter()
                .filter_map(|x| match x {
                    Event::DepartScan(v) | Event::ClickScan(v) | Event::ArrivalScan(v) => {
                        Some(v.clicperso)
                    }
                    _ => None,
                })
                .collect();
            let dials = dialogs(&evs);
            got.push(format!(
                "F p{} {}| {} | hit {} | {} | scans CLICPERSO {} | chapi060 {} | dialogs {}",
                n - a,
                if click { "click " } else { "" },
                slot_row(&e, k),
                e.sprite_under_centre().0.map_or(-1, |s| s as i32),
                commands(&evs),
                lst(scans),
                lst(lines(&evs, 60)),
                lst(&dials)
            ));
            dialog |= !dials.is_empty();
            if n >= a + late + 2 {
                break;
            }
        }
        let want = &golden[at..end];
        if dialog {
            // The dialog runs here (the harness only records its start):
            // the rows up to the click's. The talk is chosen by `random`
            // (the 15 Hz count at the last second, not the harness's
            // here): the click's row up to its scans, then one of the
            // soul's seven talks.
            let upto = got.iter().position(|r| r.contains(" click ")).unwrap();
            assert_eq!(&got[..upto], &want[..upto]);
            let cut = |r: &str| r.split(" | chapi060 ").next().unwrap().to_string();
            assert_eq!(cut(&got[upto]), cut(want[upto]));
            let talk = dialogs_of(&got[upto]);
            assert!(
                talk.len() == 1 && (346..=352).contains(&talk[0]),
                "{}",
                got[upto]
            );
            while e.loop_type == LoopType::Dialog {
                e.end_dialog();
                e.pass(None);
            }
        } else {
            assert_eq!(got, want);
        }
    }
    e.forced_centre = None;
}
