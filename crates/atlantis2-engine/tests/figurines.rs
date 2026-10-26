//! The figurines on the floor of China's miniature garden
//! (`miniature.wam`, `SS` views 1-7) against the original
//! (`tests/golden/figurines.txt`, its header there): the slots and zones
//! (rows A), the view entry per view and state (B), the click alone along
//! a played sequence and the check (C), the dispatcher's choice (D), the
//! warp frame with chapter 47 (E), the animation step in view 7 (F) and a
//! figurine's slot drawn (G: the faces' md5s, boxes, sampled pixels).
//!
//! Then the puzzle played as a player plays it, with the mouse: the six
//! figurines taken in view 13 (chapter 55), laid in view 6 (chapter 47),
//! and in view 7 (chapter 48) the figurines alive, the sixth's coin and
//! the first figurine's disc. With `ATLANTIS2_SHOTS` set to a folder, the
//! played test writes `pz-figurines-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped (rows D run
//! always).

use std::cell::RefCell;
use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};

use atlantis2_engine::anim::Anim;
use atlantis2_engine::camera::direction_angles;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{chinese_click_hooks, FIGURINE_TARGETS};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{CursorBank, Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/figurines.txt");

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

/// The golden's rows starting with `prefix`, in order.
fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
}

fn lst<T: ToString>(xs: impl IntoIterator<Item = T>) -> String {
    let v: Vec<String> = xs.into_iter().map(|x| x.to_string()).collect();
    format!("[{}]", v.join(","))
}

/// The ids the golden names `{S:I}`.
const FIGURINE_IDS: [i32; 6] = [61, 62, 63, 64, 65, 121];

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

/// The scripts' warnings logged since the last call, as the golden's
/// `warning:<format>` entries.
fn warnings() -> String {
    let w: Vec<String> = LOGGED.with(|l| {
        l.borrow_mut()
            .drain(..)
            .filter_map(|m| {
                let rest = m.strip_prefix("script: ")?;
                let format = rest.split(" [").next()?;
                Some(format!("warning:{format}"))
            })
            .collect()
    });
    if w.is_empty() {
        "-".into()
    } else {
        w.join(";")
    }
}

/// A game in the miniature garden: a new game's film ended, then the
/// world, the place, view 6 and chapter 47, as the chapters leave them.
fn in_garden(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=miniature.wam)",
        b"(vue=6)",
        b"(chapter=47)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("miniature.wam", 6));
    e.trace = true;
    e
}

/// Films and dialogs ended until the panorama is back.
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

fn set_pos(e: &mut Engine, pos: &[i32]) {
    let s = e.script_state_mut().unwrap();
    for (k, p) in pos.iter().enumerate() {
        assert!(s.set_var(&format!("pos_statue{}", k + 1), *p));
    }
}

/// As the golden's `fresh`: the bag and the hand emptied, `MONDE` 3,
/// `pos_statue1..6` and `etat5statues` set, the view entered.
fn fresh(e: &mut Engine, pos: &[i32], etat: i32, view: u32) {
    e.run_command(b"(objetenmain=0)");
    let s = e.script_state_mut().unwrap();
    s.bag.clear();
    s.set_var("MONDE", 3);
    s.set_var("etat5statues", etat);
    set_pos(e, pos);
    e.figurine_slots = [0; 6];
    e.events.clear();
    e.enter_at(view, 0.0, 0.0).unwrap();
}

fn slot_row(e: &Engine, n: usize) -> String {
    let s = &e.anim.slots[n];
    format!(
        "s{n}:{}/{}/a{} f{} e{} c{} act{} p{} one{} k{}",
        s.id,
        s.kind,
        s.state.anim,
        s.frame,
        s.end,
        s.count,
        i32::from(s.active),
        i32::from(s.playing),
        s.single,
        s.click
    )
}

fn fig_slots(e: &Engine) -> String {
    let v: Vec<String> = (0..e.anim.slots.len())
        .filter(|&n| FIGURINE_IDS.contains(&e.anim.slots[n].id))
        .map(|n| slot_row(e, n))
        .collect();
    v.join(" ")
}

/// The golden's state: pos, etat5statues, the hand, the cursor pair, the
/// hand pair, the bag, the slot indexes, the figurines' slots.
fn state(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    let pos: Vec<i32> = (1..=6).map(|k| var(e, &format!("pos_statue{k}"))).collect();
    let cursor = if !s.hand_drawn {
        format!("0#{}", s.hand_x)
    } else {
        match e.cursor.bank {
            CursorBank::Objects => format!("OBJ#{}", s.hand_x),
            CursorBank::Interface => format!("0x11110000#{}", s.hand_x),
            CursorBank::Puzzle => format!("puzzle#{}", s.hand_x),
            CursorBank::Menu => format!("menu#{}", s.hand_x),
        }
    };
    let pair = format!(
        "{}/{}",
        s.cursor_sprite,
        if s.cursor_objects { "OBJ" } else { "0" }
    );
    let bag = lst(s
        .bag
        .iter()
        .map(|b| format!("{}/{}/{}", b.id, b.sprite, b.world)));
    format!(
        "pos {} etat {} hand {} cursor {cursor} pair {pair} bag {bag} idx {} | {}",
        lst(pos),
        var(e, "etat5statues"),
        var(e, "OBJETENMAIN"),
        lst(e.figurine_slots),
        fig_slots(e)
    )
}

/// The golden's calls kept: the sounds and the videos started.
fn calls(evs: &[Event]) -> String {
    let v: Vec<String> = evs
        .iter()
        .filter_map(|x| match x {
            Event::Sound(n) => Some(format!("AtlSound_NewSound('{n}')")),
            Event::VideoStart { name, .. } => Some(format!("AtlHnm_start({name})")),
            _ => None,
        })
        .collect();
    if v.is_empty() {
        "-".into()
    } else {
        v.join("; ")
    }
}

fn golden_calls(field: &str) -> String {
    let v: Vec<&str> = field
        .split("; ")
        .filter(|c| c.starts_with("AtlSound_NewSound") || c.starts_with("AtlHnm_start"))
        .collect();
    if v.is_empty() {
        "-".into()
    } else {
        v.join("; ")
    }
}

/// The zone of view 6 the golden names `{4:v}` (its value).
fn zone_of(e: &Engine, name: &str) -> usize {
    let v: i32 = name
        .trim_start_matches("{4:")
        .trim_end_matches('}')
        .parse()
        .unwrap();
    e.wam()
        .view(6)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.kind == 3 && z.value == v)
        .unwrap()
}

#[test]
fn the_slots_and_zones_of_the_garden_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_garden(files);
    let s = e.script_state().unwrap();
    let ids: Vec<String> = (1..=5)
        .map(|k| {
            s.names
                .id(defnames::PERSONNAGES, format!("FIGURINE{k}").as_bytes())
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        format!("A figurines {}", ids.join(" ")),
        rows("A figurines")[0]
    );
    let objects: Vec<String> = (1..=6)
        .map(|k| {
            let n = format!("OB_FIG{k}HERBE");
            let id = s.names.id(defnames::OBJETS, n.as_bytes()).unwrap();
            format!("{id}:{}", s.names.param(defnames::OBJETS, n.as_bytes()))
        })
        .collect();
    assert_eq!(
        format!("A objects {}", objects.join(" ")),
        rows("A objects")[0]
    );
    let mut vars: Vec<String> = (1..=6)
        .map(|k| {
            s.names
                .id(defnames::VARIABLES, format!("pos_statue{k}").as_bytes())
                .unwrap()
                .to_string()
        })
        .collect();
    for n in ["etat5statues", "ObjetEnMain"] {
        vars.push(
            s.names
                .id(defnames::VARIABLES, n.as_bytes())
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(
        format!("A variables {}", vars.join(" ")),
        rows("A variables")[0]
    );
    assert_eq!(
        format!("A targets {}", lst(FIGURINE_TARGETS)),
        rows("A targets")[0]
    );
    // View 6's zones: type, value and name (as its `#ZONES_CLIC#` index).
    let zones: Vec<String> = e
        .wam()
        .view(6)
        .unwrap()
        .zones
        .iter()
        .enumerate()
        .map(|(i, z)| {
            let name = z
                .name
                .as_deref()
                .filter(|n| !n.is_empty())
                .map_or("-".into(), |n| {
                    format!(
                        "{{4:{}}}",
                        s.names.id(defnames::ZONES_CLIC, n.as_bytes()).unwrap()
                    )
                });
            format!("{i}:{} {} {name}", z.kind, z.value)
        })
        .collect();
    assert_eq!(
        format!("A view 6 zones {}", zones.join(" ")),
        rows("A view 6 zones")[0]
    );
    // The slots each view loads (the figurines' in full; the others by id,
    // kind and animation: the chest's entry holds its slot at another
    // frame than the golden's harness, which ran the figurines' alone).
    for row in rows("A view ").iter().filter(|r| r.contains(" slots ")) {
        let view: u32 = row.split(' ').nth(2).unwrap().parse().unwrap();
        fresh(&mut e, &[0; 6], 0, view);
        let want: Vec<&str> = row.split(" slots ").nth(1).unwrap().split(" s").collect();
        assert_eq!(want.len(), e.anim.slots.len(), "view {view}");
        for (n, w) in want.iter().enumerate() {
            let w = if n == 0 {
                w.to_string()
            } else {
                format!("s{w}")
            };
            let got = slot_row(&e, n);
            if FIGURINE_IDS.contains(&e.anim.slots[n].id) {
                assert_eq!(got, w, "view {view}");
            } else {
                let head = |x: &str| x.split(' ').next().unwrap().to_string();
                assert_eq!(head(&got), head(&w), "view {view}");
            }
        }
    }
}

#[test]
fn the_entry_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_garden(files);
    let golden = rows("B ");
    let mut got = Vec::new();
    let tail = |s: &str| s.split(" idx ").nth(1).unwrap().to_string();
    for row in &golden {
        let (head, rest) = row.split_once(" -> ").unwrap();
        let w: Vec<&str> = head.split(' ').collect();
        let view: u32 = w[2].parse().unwrap();
        if head.ends_with(" again") {
            e.figurines_entry();
            let st = state(&e);
            let prev = got
                .last()
                .map(|g: &String| g.split(" -> ").nth(1).unwrap().to_string());
            let same = prev.is_some_and(|p| tail(&p) == tail(&st));
            got.push(format!(
                "{head} -> {}",
                if same { "same".to_string() } else { st }
            ));
            continue;
        }
        let pos: Vec<i32> = head
            .split("pos [")
            .nth(1)
            .unwrap()
            .trim_end_matches(']')
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        if head.contains("table (") {
            fresh(&mut e, &[0; 6], 0, 1);
            e.run_command(b"(hideobjet=OB_FIG6HERBE,x)");
            set_pos(&mut e, &pos);
            e.events.clear();
            e.enter_at(view, 0.0, 0.0).unwrap();
        } else {
            fresh(&mut e, &pos, 0, view);
        }
        let ran = e.events.iter().any(|x| {
            matches!(
                x,
                Event::PuzzleRan {
                    function: 0x0048_d680
                }
            )
        });
        assert!(ran, "{head}");
        let st = state(&e);
        let golden_state = rest.split(" | errors").next().unwrap();
        assert_eq!(tail(&st), tail(golden_state), "{head}");
        got.push(format!("{head} -> {st}"));
    }
    // The "again" rows: the hook run a second time changes nothing.
    for (g, w) in got.iter().zip(&golden) {
        if w.ends_with("again -> same") {
            assert!(g.ends_with("-> same"), "{g}");
        }
    }
}

/// What the golden's section C does before each of its rows, in order
/// (the harness's sequence).
enum Step {
    /// `fresh` with these `pos_statue1..6`, view 6.
    Fresh([i32; 6]),
    /// Objects given to the bag (`(sacados=…)`, FUN_004754f0 with the
    /// object's parameter).
    Give(&'static [&'static str]),
    /// The object held (`(objetenmain=…)`: FUN_00475340; "0": FUN_0047bf40).
    Hold(&'static str),
    /// `etat5statues` set.
    Etat(i32),
    /// The view entered again.
    Enter,
    /// A row: the state after the steps before it.
    State,
    /// A row: the click on the view-6 zone of this `#ZONES_CLIC#` index.
    Click(i32),
    /// A row: the check alone on these `pos_statue1..6` and `etat5statues`.
    Check([i32; 6], i32),
    /// The check's header row.
    Header,
}

const C_STEPS: &[Step] = {
    use Step::*;
    &[
        Fresh([0; 6]),
        State,
        Click(27),
        Give(&[
            "OB_FIG1HERBE",
            "OB_FIG2HERBE",
            "OB_FIG3HERBE",
            "OB_FIG4HERBE",
            "OB_FIG5HERBE",
            "OB_FIG6HERBE",
        ]),
        Hold("OB_FIG1HERBE"),
        State,
        Click(28),
        Click(28),
        Click(30),
        Click(30),
        Hold("OB_FIG2HERBE"),
        Click(30),
        Click(27),
        Hold("OB_FIG3HERBE"),
        Click(27),
        Click(29),
        Hold("OB_FIG4HERBE"),
        Click(27),
        Click(28),
        Give(&["BAGUETTE"]),
        Hold("BAGUETTE"),
        Click(32),
        Click(28),
        Click(23),
        Click(31),
        Hold("OB_FIG5HERBE"),
        Click(28),
        Hold("OB_FIG6HERBE"),
        Click(32),
        Enter,
        State,
        Hold("0"),
        Click(32),
        Etat(0),
        Click(32),
        Etat(2),
        Hold("BAGUETTE"),
        Click(32),
        Header,
        Check([4, 3, 5, 1, 2, 6], 0),
        Check([4, 3, 5, 1, 2, 6], 1),
        Check([4, 3, 5, 1, 2, 6], 2),
        Check([4, 3, 5, 1, 2, 5], 0),
        Check([0, 3, 5, 1, 2, 6], 0),
        Fresh([2, 2, 0, 0, 0, 0]),
        Give(&[
            "OB_FIG1HERBE",
            "OB_FIG2HERBE",
            "OB_FIG3HERBE",
            "OB_FIG4HERBE",
            "OB_FIG5HERBE",
            "OB_FIG6HERBE",
        ]),
        Hold("0"),
        Click(28),
        Fresh([4, 4, 0, 0, 0, 0]),
        Click(30),
    ]
};

#[test]
fn the_click_as_the_original() {
    let Some(files) = game() else { return };
    capture();
    let mut e = in_garden(files);
    let golden = rows("C ");
    let mut k = 0;
    for step in C_STEPS {
        match step {
            Step::Fresh(pos) => fresh(&mut e, pos, 0, 6),
            Step::Give(names) => {
                for n in *names {
                    e.run_command(format!("(sacados={n})").as_bytes());
                }
            }
            Step::Hold(n) => e.run_command(format!("(objetenmain={n})").as_bytes()),
            Step::Etat(v) => {
                e.script_state_mut().unwrap().set_var("etat5statues", *v);
            }
            Step::Enter => e.enter_at(6, 0.0, 0.0).unwrap(),
            Step::Header => {
                assert!(golden[k].starts_with("C the check"));
                k += 1;
            }
            Step::State => {
                let rest = golden[k].split_once(" -> ").unwrap().1;
                assert_eq!(state(&e), rest, "{}", golden[k]);
                k += 1;
            }
            Step::Click(value) => {
                let row = golden[k];
                k += 1;
                let (head, rest) = row.split_once(" -> ").unwrap();
                assert!(head.ends_with(&format!(" on {{4:{value}}}")), "{head}");
                warnings();
                e.events.clear();
                let z = zone_of(&e, &format!("{{4:{value}}}"));
                let zone = e.wam().view(6).unwrap().zones[z].clone();
                e.figurines_click(&zone);
                let fields: Vec<&str> = rest.split(" | ").collect();
                let n = fields.len();
                assert_eq!(state(&e), fields[..n - 2].join(" | "), "{head}");
                assert_eq!(calls(&e.events), golden_calls(fields[n - 2]), "{head}");
                let errors = fields[n - 1].trim_start_matches("errors ");
                assert_eq!(warnings(), errors, "{head}");
                // `AtlHnm_start("???")`: nothing opens, the panorama stays.
                assert_eq!(e.loop_type, LoopType::Warp);
            }
            Step::Check(pos, etat) => {
                let row = golden[k];
                k += 1;
                assert!(row.starts_with(&format!("C check pos {} etat {etat} ->", lst(pos))));
                set_pos(&mut e, pos);
                e.script_state_mut().unwrap().set_var("etat5statues", *etat);
                e.events.clear();
                e.figurines_check();
                let fields: Vec<&str> = row.split_once(" -> ").unwrap().1.split(" | ").collect();
                assert_eq!(
                    format!("etat {}", var(&e, "etat5statues")),
                    fields[0],
                    "{row}"
                );
                assert_eq!(calls(&e.events), golden_calls(fields[1]), "{row}");
                assert_eq!(e.loop_type, LoopType::Warp);
            }
        }
    }
    assert_eq!(k, golden.len());
}

#[test]
fn the_dispatcher_as_the_original() {
    for row in rows("D ") {
        let w: Vec<&str> = row.split(' ').collect();
        let (name, view, kind): (&str, u32, i32) =
            (w[1], w[2].parse().unwrap(), w[4].parse().unwrap());
        let hooks: Vec<String> = chinese_click_hooks(name.as_bytes(), view, kind)
            .into_iter()
            .filter(|&f| f == 0x0049_3960 || f == 0x0048_da20)
            .map(|f| format!("{f:08x}"))
            .collect();
        let hooks = if hooks.is_empty() {
            "-".to_string()
        } else {
            hooks.join(",")
        };
        // The dispatcher never takes a Chinese click (AL 0).
        assert_eq!(format!("D {name} {view} type {kind} -> {hooks} AL 0"), row);
    }
}

#[test]
fn the_click_in_the_warp_frame_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_garden(files);
    for k in 1..=6 {
        e.run_command(format!("(sacados=OB_FIG{k}HERBE)").as_bytes());
    }
    let mut t = 10.0;
    for row in rows("E click") {
        let w: Vec<&str> = row.split(' ').collect();
        let zone: usize = w[4].parse().unwrap();
        let held = w[7];
        if held != "-" {
            let id: i32 = held
                .trim_start_matches("{1:")
                .trim_end_matches('}')
                .parse()
                .unwrap();
            let name = e
                .script_state()
                .unwrap()
                .names
                .name(defnames::OBJETS, id as usize)
                .unwrap()
                .to_vec();
            let mut c = b"(objetenmain=".to_vec();
            c.extend(name);
            c.push(b')');
            e.run_command(&c);
        }
        let (yaw, pitch) = e.zone_aim(6, zone).unwrap();
        e.camera.enter(yaw, pitch);
        e.pass(None);
        assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone));
        e.events.clear();
        t += 10.0;
        e.click(t);
        e.pass(None);
        e.pass(None);
        let evs = std::mem::take(&mut e.events);
        // The hooks: the chest's noted, the figurines' run, in that order.
        let hooks: Vec<String> = evs
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleNotPorted { function } | Event::PuzzleRan { function } => {
                    Some(format!("{function:08x}"))
                }
                _ => None,
            })
            .collect();
        let golden_hooks = row
            .split("hooks ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        let golden_hooks: Vec<&str> = golden_hooks
            .split(',')
            .filter(|h| *h != "0048dcb0")
            .collect();
        assert_eq!(hooks, golden_hooks, "{row}");
        // The click scan's `CLICZONE`: the zone's value, as the golden's
        // last scan.
        let cz = evs
            .iter()
            .filter_map(|x| match x {
                Event::ClickScan(v) => Some(v.cliczone),
                _ => None,
            })
            .next_back();
        let golden_cz: i32 = row
            .rsplit("CLICZONE=")
            .next()
            .unwrap()
            .split(',')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(cz, Some(golden_cz), "{row}");
        // No line of chapter 47 runs.
        assert!(!evs
            .iter()
            .any(|x| matches!(x, Event::Set { chapter: 47, .. })));
        let fields: Vec<&str> = row.split(" | ").collect();
        let n = fields.len();
        assert_eq!(calls(&evs), golden_calls(fields[n - 4]), "{row}");
        // The state but the cursor pair (the next frame's cursor choice).
        let strip = |s: &str| {
            let (a, b) = s.split_once(" cursor ").unwrap();
            format!("{a} {}", b.split_once(" pair ").unwrap().1)
        };
        let golden_state = fields[n - 3..n - 1].join(" | ");
        assert_eq!(strip(&state(&e)), strip(&golden_state), "{row}");
    }
}

#[test]
fn the_step_in_view_7_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_garden(files);
    let golden = rows("F ");
    let mut i = 0;
    while i < golden.len() {
        let head = golden[i];
        let pos: Vec<i32> = head
            .split("pos [")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap()
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        let label = head.split(" pos [").next().unwrap();
        fresh(&mut e, &pos, 0, 7);
        e.rand = Rand::new(1);
        let mine: Vec<usize> = (0..e.anim.slots.len())
            .filter(|&n| FIGURINE_IDS.contains(&e.anim.slots[n].id))
            .collect();
        assert_eq!(
            head,
            format!("{label} pos {} slots {}", lst(&pos), lst(&mine))
        );
        i += 1;
        for t in 1..=70 {
            e.anim.step(&mut e.rand);
            let row: Vec<String> = mine
                .iter()
                .map(|&n| {
                    let s = &e.anim.slots[n];
                    format!("f{}a{}", s.frame, i32::from(s.active))
                })
                .collect();
            assert_eq!(format!("{label} t{t} {}", row.join(" ")), golden[i]);
            i += 1;
        }
    }
}

/// The background of the golden's faces.
fn background() -> Vec<u16> {
    (0..512u64 * 512)
        .map(|i| (((i * 2_654_435_761) & 0xffff_ffff) >> 16) as u16)
        .collect()
}

fn md5_of(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    format!("{:x}", Md5::digest(&bytes))
}

#[test]
fn a_figurine_is_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let bg = background();
    let bg_md5 = md5_of(&bg);
    let mut got = Vec::new();
    let golden: Vec<&str> = rows("G ");
    let mut data = Vec::new();
    for row in &golden {
        if let Some(rest) = row.strip_prefix("G file ") {
            let name = rest.split(' ').next().unwrap();
            data = files.read(Folder::Spw, name).expect("the figurine's SPW");
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
        let mut a = Anim::new();
        a.load(
            name,
            Spw::parse(data.clone()).unwrap(),
            2,
            121,
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
    let want: Vec<String> = golden
        .iter()
        .map(|r| r.replace(".SPW md5", " md5"))
        .collect();
    let got: Vec<String> = got.iter().map(|r| r.replace(".SPW md5", " md5")).collect();
    assert_eq!(got, want);
}

/// The engine at 59.94 passes a second ([`PASS`]), a 15 Hz tick every fourth, every
/// pass drawn; the mouse as a player moves it.
struct Run {
    e: Engine,
    pass: u64,
    gap: f64,
    screen: Vec<u16>,
    shots: Option<PathBuf>,
}

impl Run {
    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    fn step(&mut self) {
        if self.pass.is_multiple_of(4) {
            self.e.tick();
        }
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

    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// A left click far from the last one (never a double click).
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
            std::fs::write(
                dir.join(format!("pz-figurines-{name}.png")),
                encode_rgb565(&self.screen, 640, 480),
            )
            .unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.id(section, name.as_bytes()).unwrap() as i32
    }

    /// Turns the view onto zone `zone` (the centre on it).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        assert_eq!(self.e.centre_zone().map(|(i, _)| i), Some(zone));
    }

    /// A click on the move zone to view `to`, the transition video
    /// clicked away, a dialog the arrival plays listened to.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
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
        self.aim(zone);
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
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if video {
                self.wait(15);
                self.tap();
            }
            if arrived {
                assert_eq!(self.e.place().1, to);
                self.wait(4);
                all.extend(std::mem::take(&mut self.e.events));
                if self.e.loop_type == LoopType::Dialog {
                    all.extend(self.listen());
                }
                assert_eq!(self.e.loop_type, LoopType::Warp);
                return all;
            }
        }
        panic!("no arrival in view {to}");
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

    /// Turns the view so that a point of slot `n`'s sprite is under the
    /// centre with the cursor of `kind` (1 an object, 2 a character): the
    /// point a 5 x 5 grid inside its frame's box on a face, the direction
    /// the inverse of the centre's texel. True when found.
    fn aim_slot(&mut self, n: usize, kind: i32) -> bool {
        let fpu = self.e.fpu();
        for face in 0..6 {
            let Some(b) = self.e.anim.slots[n].frame_box(face) else {
                continue;
            };
            let (x, y, w, h) = (
                i32::from(b.x),
                i32::from(b.y),
                i32::from(b.w),
                i32::from(b.h),
            );
            if w < 1 || h < 1 {
                continue;
            }
            for i in 0..5 {
                for j in 0..5 {
                    let col = x + 1 + (w - 1) * i / 4;
                    let row = y + 1 + (h - 1) * j / 4;
                    // Not where an earlier clickable sprite's box is: the
                    // first slot under the centre is the one clicked.
                    let inside = |m: usize| {
                        let o = &self.e.anim.slots[m];
                        o.active
                            && o.click > 0
                            && o.frame_box(face).is_some_and(|c| {
                                let (cx, cy) = (i32::from(c.x), i32::from(c.y));
                                let (cw, ch) = (i32::from(c.w), i32::from(c.h));
                                cx < col && col < cx + cw + 1 && cy < row && row < cy + ch + 1
                            })
                    };
                    if (0..n).any(inside) {
                        continue;
                    }
                    // The inverse of the centre's texel (FUN_0047db60 as
                    // the port maps a ray to a face).
                    let s = |t: i32| (f64::from(t) - 0.5) / 255.0 - 1.0;
                    let (u, v) = (s(col), s(row));
                    let d = match face {
                        0 => [u, v, 1.0],
                        1 => [-u, v, -1.0],
                        2 => [u, -1.0, v],
                        3 => [u, 1.0, -v],
                        4 => [-1.0, v, u],
                        _ => [1.0, v, -u],
                    };
                    let (yaw, pitch) =
                        direction_angles(fpu, [d[0] as f32, d[1] as f32, d[2] as f32]);
                    self.e.camera.enter(yaw, pitch);
                    self.e.pass(None);
                    if self.e.sprite_under_centre().1 == kind {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Slot `n`'s sprite clicked (its cursor `kind` under the centre):
    /// the click's events.
    fn click_slot(&mut self, n: usize, kind: i32) -> Vec<Event> {
        assert!(
            self.aim_slot(n, kind),
            "slot {n} never under the centre in view {}",
            self.e.place().1
        );
        self.tap();
        let mut evs = self.events();
        evs.extend(self.events());
        evs
    }

    /// The bag bar opened with the right button, scrolled with the pointer
    /// at its edge until object `id` shows (one cell a tenth of a second),
    /// and its cell clicked: the object in hand, the bar closed.
    fn pick(&mut self, id: i32) {
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
                assert_eq!(self.var("OBJETENMAIN"), id);
                return;
            }
            // Out of the cells: the pointer at the bar's right edge, then at
            // its left one.
            let first = self.e.bar.first;
            if right {
                self.point(636, 440);
            } else {
                self.point(1, 440);
            }
            self.wait(8);
            if self.e.bar.first == first {
                assert!(
                    right,
                    "object {id} not shown: bag {:?} shown {:?} first {}",
                    self.e.bag_entries(),
                    self.e.bar.shown,
                    self.e.bar.first
                );
                right = false;
            }
        }
        panic!("object {id} never shown in the bar");
    }

    /// A click on the place `SOL_FIGUR<d>` of view 6, the centre on it.
    fn click_place(&mut self, d: i32) -> Vec<Event> {
        let s = self.e.script_state().unwrap();
        let value = s
            .names
            .id(defnames::ZONES_CLIC, format!("SOL_FIGUR{d}").as_bytes())
            .unwrap() as i32;
        let zone = self
            .e
            .wam()
            .view(6)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 3 && z.value == value)
            .unwrap();
        self.aim(zone);
        self.tap();
        let evs = self.events();
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0048_da20
        }));
        evs
    }

    fn pos(&self) -> Vec<i32> {
        (1..=6)
            .map(|k| self.var(&format!("pos_statue{k}")))
            .collect()
    }
}

fn sounds(evs: &[Event]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Sound(n) => Some(n.clone()),
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

#[test]
fn the_figurines_played_with_the_mouse() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    // China as the chapters before leave it: the incense lit (chapter 47,
    // `flag142`), the garden's bridge side (view 13, chapter 55).
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=miniature.wam)",
        b"(vue=13)",
        b"(chapter=55)",
        b"(flag142=1)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    e.trace = true;
    let shots = std::env::var_os("ATLANTIS2_SHOTS").map(PathBuf::from);
    if let Some(d) = &shots {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut r = Run {
        e,
        pass: 0,
        gap: 0.0,
        screen: vec![0; 640 * 480],
        shots,
    };
    r.wait(20);
    assert_eq!(r.e.place(), ("miniature.wam", 13));
    let objects: Vec<i32> = (1..=6)
        .map(|k| r.id(defnames::OBJETS, &format!("OB_FIG{k}HERBE")))
        .collect();

    // View 13: the six figurines lying in the grass, each taken with a
    // click on its sprite (click kind 1): in the bag.
    r.shot("1-view-13");
    // In the slots' order (an earlier sprite over a later one takes the
    // click).
    let grass: Vec<(usize, i32)> = (0..r.e.anim.slots.len())
        .map(|n| (n, r.e.anim.slots[n].id))
        .filter(|&(n, id)| objects.contains(&id) && r.e.anim.slots[n].active)
        .collect();
    assert_eq!(grass.len(), 6);
    for (n, id) in grass {
        let evs = r.click_slot(n, 1);
        assert!(evs.contains(&Event::Take { id }), "{id} taken");
        assert!(
            r.e.bag_entries().iter().any(|b| b.id == id),
            "{id} in the bag"
        );
        r.wait(4);
    }
    r.shot("2-taken");

    // To view 6: 13 → 12 → 11 → 10 → 7 (chapter 48) → 6 (chapter 47).
    for to in [12, 11, 10, 7, 6] {
        r.walk(to);
    }
    assert_eq!(r.e.place(), ("miniature.wam", 6));
    assert_eq!(r.e.script_state().unwrap().chapter, 47);
    assert_eq!(r.pos(), [0; 6]);
    r.shot("3-floor");

    // An empty hand on a place: nothing.
    let evs = r.click_place(4);
    assert!(sounds(&evs).is_empty());
    assert_eq!(r.pos(), [0; 6]);
    // The first figurine laid on place 2 (not its own): drawn, sound 202,
    // out of the hand and the bag.
    r.pick(objects[0]);
    let evs = r.click_place(2);
    assert_eq!(sounds(&evs), ["sound202.apc"]);
    assert_eq!(r.pos(), [2, 0, 0, 0, 0, 0]);
    assert_eq!(r.var("OBJETENMAIN"), 0);
    assert!(!r.e.bag_entries().iter().any(|b| b.id == objects[0]));
    // The second laid on place 4 (the first's own place).
    r.pick(objects[1]);
    r.click_place(4);
    assert_eq!(r.pos(), [2, 4, 0, 0, 0, 0]);
    // The first taken back from place 2 with an empty hand: in hand and in
    // the bag, no sound.
    let evs = r.click_place(2);
    assert!(sounds(&evs).is_empty());
    assert_eq!(r.pos(), [0, 4, 0, 0, 0, 0]);
    assert_eq!(r.var("OBJETENMAIN"), objects[0]);
    assert!(r.e.bag_entries().iter().any(|b| b.id == objects[0]));
    // Held on place 4: a swap, the second in hand, the first on its own
    // place.
    let evs = r.click_place(4);
    assert_eq!(sounds(&evs), ["sound202.apc"]);
    assert_eq!(r.pos(), [4, 0, 0, 0, 0, 0]);
    assert_eq!(r.var("OBJETENMAIN"), objects[1]);
    r.wait(2);
    r.shot("4-swap");
    // The first on its own place stays: a click on it does nothing.
    let evs = r.click_place(4);
    assert!(sounds(&evs).is_empty());
    assert_eq!((r.pos()[0], r.var("OBJETENMAIN")), (4, objects[1]));
    // The second, in hand, laid on its own place 3.
    r.click_place(3);
    assert_eq!(r.pos(), [4, 3, 0, 0, 0, 0]);
    // The others on their places: 3 on 5, 4 on 1, 5 on 2, then 6 on 6:
    // the sixth wins.
    for (k, d) in [(2usize, 5), (3, 1), (4, 2)] {
        r.pick(objects[k]);
        r.click_place(d);
    }
    assert_eq!(r.var("etat5statues"), 0);
    r.pick(objects[5]);
    let evs = r.click_place(6);
    assert_eq!(r.pos(), FIGURINE_TARGETS);
    assert_eq!(r.var("etat5statues"), 1);
    // `AtlHnm_start("???")`: no film.
    assert!(evs.contains(&Event::VideoStart {
        name: "???".into(),
        frames: None
    }));
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert!(r.e.bag_entries().iter().all(|b| !objects.contains(&b.id)));
    r.wait(2);
    r.shot("5-all-six");
    // Laid for good: a click on a figurine does nothing now.
    let evs = r.click_place(1);
    assert!(sounds(&evs).is_empty());
    assert_eq!(r.var("OBJETENMAIN"), 0);

    // To view 7 (chapter 47's lines 27-33: film 23, chapter 48): the
    // figurines alive (their animation 2 shown and playing).
    let evs = r.walk(7);
    assert_eq!(r.e.script_state().unwrap().chapter, 48);
    assert!(lines(&evs, 47).contains(&33));
    let figs: Vec<i32> = (1..=5)
        .map(|k| r.id(defnames::PERSONNAGES, &format!("FIGURINE{k}")))
        .chain([objects[5]])
        .collect();
    for &id in &figs {
        let s =
            r.e.anim
                .slots
                .iter()
                .find(|s| s.id == id && s.active)
                .expect("the figurine shown");
        assert_eq!((s.state.anim, s.playing), (2, true));
    }
    r.wait(30);
    r.shot("6-alive");

    // The sixth figurine clicked (`CLICOBJET`, chapter 48's lines 34-39):
    // the fifth coin in hand, the figurine gone, `pos_statue6` 0.
    // (The only object of view 7 with a click kind.)
    let piece = r.id(defnames::OBJETS, "PIECE5");
    let n =
        r.e.anim
            .slots
            .iter()
            .position(|s| s.id == objects[5] && s.active)
            .unwrap();
    let evs = r.click_slot(n, 1);
    assert_eq!(lines(&evs, 48), [35, 36, 37, 38, 39]);
    assert_eq!(r.var("OBJETENMAIN"), piece);
    assert_eq!(r.var("pos_statue6"), 0);
    r.wait(2);
    r.shot("7-coin");

    // The first figurine clicked, the coin in hand (chapter 48's section
    // 004, lines 64-71): dialogs 281 and 282, the first disc in hand, the
    // coin traded for the fourth.
    let n =
        r.e.anim
            .slots
            .iter()
            .position(|s| s.id == figs[0] && s.active)
            .unwrap();
    let mut evs = r.click_slot(n, 2);
    assert!(evs
        .iter()
        .any(|x| matches!(x, Event::ClickScan(v) if v.clicperso == figs[0])));
    let k = 0;
    let first = 281;
    assert!(evs.contains(&Event::DialogStart {
        number: first,
        started: true
    }));
    r.wait(100);
    r.shot(&format!("8-dialog-{first}"));
    evs.extend(r.listen());
    for _ in 0..20 {
        if r.e.loop_type == LoopType::Dialog {
            evs.extend(r.listen());
        }
        evs.extend(r.events());
    }
    assert!(evs.contains(&Event::DialogStart {
        number: first + 1,
        started: true
    }));
    assert_eq!(r.e.script_state().unwrap().flags[150 + k as usize], 1);
    let disc = r.id(defnames::OBJETS, &format!("DISQUE{}", k + 1));
    assert_eq!(r.var("OBJETENMAIN"), disc);
    let bag: Vec<i32> = r.e.bag_entries().iter().map(|b| b.id).collect();
    assert!(bag.contains(&disc) && bag.contains(&r.id(defnames::OBJETS, "PIECE4")));
    assert!(!bag.contains(&piece));
    // Its section's lines after the dialogs (`sacados`, `objetenmain`).
    assert!(!lines(&evs, 48).is_empty());
    r.wait(2);
    r.shot("9-disc");
}
