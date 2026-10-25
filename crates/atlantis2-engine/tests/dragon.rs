//! The pearl and the dragon in the clouds (`miniature.wam` view 31,
//! chapter 58) against the original (`tests/golden/dragon.txt`, its header
//! there): the view entry hook (rows B), the click hook alone (C, through
//! [`Engine::dragon_click`]), the per-frame step alone (D), frames run as
//! the warp frame runs them at five rates of the 15 Hz tick with a player
//! answering (E: the dragon's flight, the transparent clouds, the low
//! clouds, the win), the dispatchers (F), the zone clicks with chapter
//! 58's scan (G: the clouds, the dragon, then the pearl laid in the
//! mechanism of view 32 to film 26 and chapter 59), view 31's files drawn
//! with the transparent cloud's blink (H: the faces' md5s and sampled
//! pixels) and the dragon's flight step by step (I).
//!
//! Then the puzzle played as a player plays it, with the mouse: from
//! chapter 56's view 30 into view 31 and chapter 58, the dragon woken,
//! the clouds clicked (one wrong answer), the pearl won, taken, and laid
//! in the dragon's mechanism of view 32 (chapter 58's lines it unlocks)
//! to the dragon's flight and chapter 59; and the state a save keeps. With
//! `ATLANTIS2_SHOTS` set to a folder, the played test writes
//! `pz-dragon-*.png` there.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::{EntityState, Zone};
use atlantis2_data::{A2Files, Folder};

use atlantis2_engine::anim::Anim;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::{chinese_entry_hooks, Dragon};
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/dragon.txt");

/// View 31's slots in load order, as the golden names them.
const SHORT: [&str; 12] = [
    "dragon", "left", "right", "low1", "low2", "low3", "low4", "low5", "pearlL", "pearlR", "hole",
    "other",
];

/// The puzzle's variables, as the golden names them.
const VARS: [(&str, &str); 7] = [
    ("perle", "Perle_cachee"),
    ("seq", "Num_sequence_dragon"),
    ("low", "State_nuages_bas"),
    ("action", "Action_nuage_bas"),
    ("time", "Time_nuage_transparent"),
    ("side", "NuageTransparent"),
    ("fin", "FINPERLE"),
];

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

/// The rows against the golden's, the first different pair shown.
fn same<A: AsRef<str>, B: AsRef<str>>(got: &[A], want: &[B]) {
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.as_ref(), w.as_ref());
    }
    assert_eq!(got.len(), want.len());
}

/// Films and dialogs ended until the panorama is back; the passes run.
/// The number of films and dialogs ended.
fn settle(e: &mut Engine) -> usize {
    let mut resumes = 0;
    for _ in 0..400 {
        match e.loop_type {
            LoopType::Video => {
                e.end_video();
                resumes += 1;
            }
            LoopType::Dialog => {
                e.end_dialog();
                resumes += 1;
            }
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    resumes
}

/// A game in `miniature.wam`, chapter `chapter`, view `view`, as the
/// scripts put it there (the world, the place, the view, the chapter).
fn in_chapter(files: A2Files, view: u32, chapter: i32) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=3)"[..],
        b"(changewam=miniature.wam)",
        format!("(vue={view})").as_bytes(),
        format!("(chapter={chapter})").as_bytes(),
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("miniature.wam", view));
    assert_eq!(e.script_state().unwrap().chapter, chapter);
    assert_eq!(e.files().current_disc(), 3);
    e.trace = true;
    e
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn set(e: &mut Engine, name: &str, v: i32) {
    assert!(e.script_state_mut().unwrap().set_var(name, v), "{name}");
}

fn id(e: &Engine, section: usize, name: &str) -> i32 {
    e.script_state()
        .unwrap()
        .names
        .id(section, name.as_bytes())
        .unwrap() as i32
}

/// The golden's slot list of the view: name:f<frame>/p<playing>/a<active>/b<+0xc0>.
fn slots(e: &Engine, names: &[&str]) -> String {
    e.anim
        .slots
        .iter()
        .zip(names)
        .map(|(s, n)| {
            format!(
                "{n}:f{}/p{}/a{}/b{}",
                s.frame,
                i32::from(s.playing),
                i32::from(s.active),
                s.blink
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The golden's state.
fn st(e: &Engine, names: &[&str]) -> String {
    let hole = id(e, defnames::OBJETS, "PERLE_TROU");
    let vars: Vec<String> = VARS
        .iter()
        .map(|(k, n)| format!("{k} {}", var(e, n)))
        .collect();
    format!(
        "mode {} st {} cloud {} {} hole {} | {}",
        e.puzzle_mode(),
        e.dragon.state,
        e.dragon.cloud,
        vars.join(" "),
        e.anim.table.get(hole, 2),
        slots(e, names)
    )
}

/// The `NEWSOUND`s voice 2 starts (`sound%03d.apc`), in order.
fn sounds(evs: &[Event]) -> String {
    lst(evs.iter().filter_map(|e| match e {
        Event::Sound(n) if n.starts_with("sound") => Some(n.clone()),
        _ => None,
    }))
}

/// The films started.
fn films(evs: &[Event]) -> String {
    lst(evs.iter().filter_map(|e| match e {
        Event::VideoStart {
            name,
            frames: Some(_),
        } => Some(name.clone()),
        _ => None,
    }))
}

/// Chapter 58's lines run.
fn lines(evs: &[Event]) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: 58, line } => Some(*line),
            _ => None,
        })
        .collect()
}

/// The warnings and the fatal error since the last call, as the golden
/// writes them; both cleared.
fn warns(e: &mut Engine) -> String {
    let s = e.script_state_mut().unwrap();
    let mut w: Vec<String> = s
        .warnings
        .drain(..)
        .map(|f| format!("warning:{f}"))
        .collect();
    if let Some(f) = s.fatal.take() {
        w.push(format!("fatal:{f}"));
    }
    lst(w)
}

/// The golden's names of the hooks the dispatchers reach.
fn hook_name(f: u32) -> String {
    match f {
        0x0049_3960 => "chest-click".into(),
        0x0049_38f0 => "chest-entry".into(),
        0x0048_da20 => "figurines-click".into(),
        0x0048_d680 => "figurines-entry".into(),
        0x0048_d170 => "wheels-click".into(),
        0x0048_d080 => "wheels-entry".into(),
        0x0049_c290 => "ring-click".into(),
        0x0049_c340 => "ring-entry".into(),
        0x0048_df10 => "souls-entry".into(),
        0x0049_1610 => "spider-tick".into(),
        0x0049_c400 => "ring-tick".into(),
        0x0048_eb30 => "souls-tick".into(),
        0x0049_aaa0 => "octopus-tick".into(),
        0x0049_3190 => "maya-tick".into(),
        f => format!("{f:08x}"),
    }
}

/// The slots of the view as `AtlSpw_load` leaves them (no entry hook):
/// the frame at the state's first, playing, no blink, shown as the show /
/// hide table or the state says.
fn as_loaded(e: &mut Engine) {
    let table = e.anim.table.clone();
    for s in &mut e.anim.slots {
        s.frame = s.start;
        s.playing = true;
        s.blink = 0;
        let t = table.get(s.id, s.kind);
        s.active = if t == 2 { s.state.shown != 0 } else { t != 0 };
    }
}

/// The golden's fresh state: the puzzle's variables at 0 but those
/// given, no hand, an empty bag, no show / hide row of `PERLE_TROU`, the
/// mode 0, the puzzle's engine state cleared; view 31's slots as loaded
/// (the entry hook not run).
fn fresh(e: &mut Engine, vals: &[(&str, i32)]) {
    let hole = id(e, defnames::OBJETS, "PERLE_TROU");
    e.run_command(b"(objetenmain=0)");
    let s = e.script_state_mut().unwrap();
    s.bag.clear();
    s.shown.retain(|r| r.id != hole);
    for (_, n) in VARS {
        assert!(s.set_var(n, 0));
    }
    for (n, v) in ["MONDE", "ClicZone", "VoyageDragon", "PerleInMecanisme"]
        .iter()
        .zip([3, 0, 0, 0])
    {
        assert!(s.set_var(n, v));
    }
    for &(n, v) in vals {
        assert!(s.set_var(n, v), "{n}");
    }
    e.enter_at(31, 0.0, 0.0).unwrap();
    let s = e.script_state_mut().unwrap();
    s.shown.retain(|r| r.id != hole);
    s.warnings.clear();
    s.fatal = None;
    e.set_puzzle_mode(0);
    e.dragon = Dragon::default();
    e.anim.table.0.retain(|r| r.0 != hole);
    as_loaded(e);
    e.events.clear();
}

/// `fresh`, then the entry hook (the golden's `enter`).
fn entered(e: &mut Engine, vals: &[(&str, i32)]) {
    fresh(e, vals);
    e.dragon_entry();
}

/// A zone of type `kind`, value `value`, name `name` (empty: none).
fn zone(kind: i32, value: i32, name: &str) -> Zone {
    Zone {
        kind,
        value,
        name: (!name.is_empty()).then(|| name.to_string()),
        points: Vec::new(),
    }
}

/// A row's text up to its last field, for the rows whose state the
/// original's fatal error leaves (the game quits there; the port stops
/// the scripts and the hook).
fn fatal_row(r: &str) -> bool {
    r.contains("fatal:")
}

/// The row's label and its warnings up to the first fatal error (the
/// original's harness goes on after it, the game would not).
fn warn_part(r: &str) -> String {
    let label = r.split(" -> ").next().unwrap();
    let warn = r.rsplit("| warn ").next().unwrap();
    let warn = match warn.find("fatal:") {
        Some(i) => match warn[i..].find(",fatal:") {
            Some(j) => format!("{}]", &warn[..i + j]),
            None => warn.to_string(),
        },
        None => warn.to_string(),
    };
    format!("{label} | warn {warn}")
}

/// Rows made against the golden's rows of a section, in order.
struct Cmp {
    rows: Vec<&'static str>,
    at: usize,
    got: Vec<String>,
    want: Vec<String>,
}

impl Cmp {
    fn new(prefix: &str) -> Cmp {
        Cmp {
            rows: rows(prefix),
            at: 0,
            got: Vec::new(),
            want: Vec::new(),
        }
    }

    fn next(&mut self) -> &'static str {
        let r = self.rows.get(self.at).copied().expect("a golden row");
        self.at += 1;
        r
    }

    /// The row made against the next golden row: in full, or its label
    /// and warnings when the original's fatal error is in it.
    fn push(&mut self, line: String) {
        let w = self.next();
        if fatal_row(w) {
            self.got.push(warn_part(&line));
            self.want.push(warn_part(w));
        } else {
            self.got.push(line);
            self.want.push(w.to_string());
        }
    }

    fn check(&self) {
        assert_eq!(self.at, self.rows.len());
        same(&self.got, &self.want);
    }
}

#[test]
fn the_entry_click_and_step_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter(files, 31, 58);
    let names = e.script_state().unwrap().names.clone();
    let obj = |n: &str| names.id(defnames::OBJETS, n.as_bytes()).unwrap();
    let zid = |n: &str| names.id(defnames::ZONES_CLIC, n.as_bytes()).unwrap() as i32;
    let vid = |n: &str| names.id(defnames::VARIABLES, n.as_bytes()).unwrap();
    // The ids: view 31's slots are the golden's objects in its order.
    let ids: Vec<String> = e.anim.slots.iter().map(|s| s.id.to_string()).collect();
    assert_eq!(
        rows("ids objects")[0],
        format!(
            "ids objects {} PERLE_MECANISME {}",
            ids.join(" "),
            obj("PERLE_MECANISME")
        )
    );
    for (n, s) in ["DRAGON_NUAGE", "NUAGE_GAUCHE", "PERLE_TROU"]
        .iter()
        .zip([0, 1, 10])
    {
        assert_eq!(e.anim.slots[s].id, obj(n) as i32);
    }
    assert_eq!(
        rows("ids zones")[0],
        format!(
            "ids zones {} {} {} {}",
            zid("CLIC_NUAGE_GAUCHE"),
            zid("CLIC_NUAGE_DROITE"),
            zid("CLIC_DRAGON"),
            zid("MECANISME_DRAGON")
        )
    );
    let vs: Vec<String> = VARS
        .iter()
        .map(|(_, n)| *n)
        .chain(["VoyageDragon", "PerleInMecanisme"])
        .map(|n| vid(n).to_string())
        .collect();
    assert_eq!(
        rows("ids variables")[0],
        format!("ids variables {}", vs.join(" "))
    );

    // B: the entry hook.
    let mut got = Vec::new();
    let mut want = Vec::new();
    for row in rows("B ") {
        let (head, _) = row.split_once(" -> ").unwrap();
        let w: Vec<&str> = head.split(' ').collect();
        if w[1] == "without" {
            let drop = SHORT.iter().position(|&s| s == w[2]).unwrap();
            fresh(&mut e, &[]);
            e.anim.slots.remove(drop);
            e.dragon_entry();
            let mut names: Vec<&str> = SHORT.to_vec();
            names.remove(drop);
            let line = format!(
                "B without {} -> {} | warn {}",
                w[2],
                st(&e, &names),
                warns(&mut e)
            );
            if fatal_row(row) {
                got.push(warn_part(&line));
                want.push(warn_part(row));
            } else {
                got.push(line);
                want.push(row.to_string());
            }
            continue;
        }
        // "B k=v ... mode m st s"
        let n = w.len();
        let (mode, state): (i32, i32) = (w[n - 3].parse().unwrap(), w[n - 1].parse().unwrap());
        let vals: Vec<(String, i32)> = w[1..n - 4]
            .iter()
            .filter(|x| **x != "-")
            .map(|x| {
                let (k, v) = x.split_once('=').unwrap();
                (k.to_string(), v.parse().unwrap())
            })
            .collect();
        let vals: Vec<(&str, i32)> = vals.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        fresh(&mut e, &vals);
        e.set_puzzle_mode(mode);
        e.dragon.state = state;
        e.anim.slots[0].frame = 7;
        e.events.clear();
        e.dragon_entry();
        let evs = std::mem::take(&mut e.events);
        got.push(format!(
            "{head} -> {} | sounds {} | warn {}",
            st(&e, &SHORT),
            sounds(&evs),
            warns(&mut e)
        ));
        want.push(row.to_string());
    }
    same(&got, &want);

    // C: the click hook alone, along the golden's sequence.
    let mut cmp = Cmp::new("C ");
    let click =
        |c: &mut Cmp, e: &mut Engine, label: &str, name: &str, kind: i32, value: Option<i32>| {
            let value = value.unwrap_or_else(|| if name.is_empty() { 0 } else { zid(name) });
            e.events.clear();
            e.dragon_click(&zone(kind, value, name));
            let evs = std::mem::take(&mut e.events);
            let names: Vec<&str> = if e.anim.slots.len() == 12 {
                SHORT.to_vec()
            } else {
                SHORT.iter().copied().filter(|&n| n != "left").collect()
            };
            let line = format!(
                "C {label} {} {kind} -> {} | sounds {} | warn {}",
                if name.is_empty() { "-" } else { name },
                st(e, &names),
                sounds(&evs),
                warns(e)
            );
            c.push(line);
        };
    entered(&mut e, &[]);
    cmp.push(format!("C entered -> {}", st(&e, &SHORT)));
    for z in [
        "CLIC_NUAGE_GAUCHE",
        "CLIC_NUAGE_DROITE",
        "MECANISME_DRAGON",
        "",
    ] {
        click(&mut cmp, &mut e, "before the dragon", z, 3, None);
    }
    click(&mut cmp, &mut e, "dragon", "CLIC_DRAGON", 3, None);
    click(&mut cmp, &mut e, "dragon again", "CLIC_DRAGON", 3, None);
    click(
        &mut cmp,
        &mut e,
        "cloud while st 1",
        "CLIC_NUAGE_GAUCHE",
        3,
        None,
    );
    for state in [2, 3] {
        e.dragon.state = state;
        click(
            &mut cmp,
            &mut e,
            &format!("st {state}"),
            "CLIC_NUAGE_GAUCHE",
            3,
            None,
        );
    }
    e.dragon.state = 0;
    click(
        &mut cmp,
        &mut e,
        "left, A hidden",
        "CLIC_NUAGE_GAUCHE",
        3,
        None,
    );
    click(
        &mut cmp,
        &mut e,
        "right while transparent",
        "CLIC_NUAGE_DROITE",
        3,
        None,
    );
    for (perle, z) in [
        (0x41, "CLIC_NUAGE_DROITE"),
        (0x42, "CLIC_NUAGE_DROITE"),
        (0x42, "CLIC_NUAGE_GAUCHE"),
        (0x43, "CLIC_NUAGE_GAUCHE"),
        (0x43, "CLIC_NUAGE_DROITE"),
        (0x41, "CLIC_DRAGON"),
        (-1, "CLIC_NUAGE_GAUCHE"),
    ] {
        entered(&mut e, &[("Perle_cachee", perle)]);
        click(&mut cmp, &mut e, &format!("perle {perle}"), z, 3, None);
    }
    for name in [
        "clic_nuage_gauche",
        "CLIC_NUAGE_GAUCHE ",
        "CLIC_NUAGE_GAUCH",
        "CLIC_DRAGONX",
    ] {
        entered(&mut e, &[("Perle_cachee", 0x41)]);
        click(
            &mut cmp,
            &mut e,
            "name",
            name,
            3,
            Some(zid("CLIC_NUAGE_GAUCHE")),
        );
        entered(&mut e, &[]);
        click(&mut cmp, &mut e, "name", name, 3, Some(zid("CLIC_DRAGON")));
    }
    for t in [1, 4, 0x32, 0] {
        entered(&mut e, &[("Perle_cachee", 0x42)]);
        click(&mut cmp, &mut e, "type", "CLIC_NUAGE_DROITE", t, None);
    }
    entered(&mut e, &[]);
    click(
        &mut cmp,
        &mut e,
        "value only",
        "",
        3,
        Some(zid("CLIC_DRAGON")),
    );
    for vals in [
        vec![("FINPERLE", 1)],
        vec![("Time_nuage_transparent", 1)],
        vec![("Time_nuage_transparent", -1)],
        vec![("FINPERLE", 1), ("Perle_cachee", 0x41)],
    ] {
        let label: Vec<String> = vals.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let label = label.join(" ");
        entered(&mut e, &vals);
        e.set_puzzle_mode(4);
        click(&mut cmp, &mut e, &label, "CLIC_DRAGON", 3, None);
        click(&mut cmp, &mut e, &label, "CLIC_NUAGE_GAUCHE", 3, None);
    }
    for mode in [0, 3, 5] {
        entered(&mut e, &[]);
        e.set_puzzle_mode(mode);
        click(
            &mut cmp,
            &mut e,
            &format!("mode {mode}"),
            "CLIC_DRAGON",
            3,
            None,
        );
    }
    entered(&mut e, &[("Num_sequence_dragon", 4)]);
    click(&mut cmp, &mut e, "seq 4", "CLIC_DRAGON", 3, None);
    entered(&mut e, &[("Num_sequence_dragon", 2)]);
    click(&mut cmp, &mut e, "seq 2", "CLIC_DRAGON", 3, None);
    // No left cloud's slot: the original's fatal search returns slot 0
    // and goes on; the port stops the hook there.
    fresh(&mut e, &[]);
    e.anim.slots.remove(1);
    e.dragon_entry();
    e.dragon.state = 0;
    set(&mut e, "Perle_cachee", 0x41);
    e.dragon_click(&zone(3, zid("CLIC_NUAGE_GAUCHE"), "CLIC_NUAGE_GAUCHE"));
    cmp.push(format!(
        "C no left cloud slot CLIC_NUAGE_GAUCHE 3 -> | warn {}",
        warns(&mut e)
    ));
    cmp.check();

    // D: the step alone.
    let mut cmp = Cmp::new("D ");
    let d = |c: &mut Cmp, e: &mut Engine, label: &str, n: usize, names: &[&str]| {
        e.events.clear();
        for _ in 0..n {
            e.dragon_tick();
        }
        let evs = std::mem::take(&mut e.events);
        let line = format!(
            "D {label} -> {} | sounds {} | warn {}",
            st(e, names),
            sounds(&evs),
            warns(e)
        );
        c.push(line);
    };
    let dragon = zone(3, zid("CLIC_DRAGON"), "CLIC_DRAGON");
    entered(&mut e, &[]);
    d(&mut cmp, &mut e, "st 0", 1, &SHORT);
    e.dragon_click(&dragon);
    d(&mut cmp, &mut e, "after the dragon's click", 1, &SHORT);
    d(&mut cmp, &mut e, "again", 1, &SHORT);
    d(&mut cmp, &mut e, "st 3, dragon frame 50", 1, &SHORT);
    e.dragon_click(&dragon);
    e.dragon_tick();
    e.dragon_tick();
    e.anim.slots[0].frame = 51;
    d(&mut cmp, &mut e, "st 3, dragon frame 51", 1, &SHORT);
    e.anim.slots[0].frame = 50;
    d(&mut cmp, &mut e, "st 3, dragon frame 50 again", 1, &SHORT);
    for (perle, z, low) in [
        (0x41, "CLIC_NUAGE_GAUCHE", 0),
        (0x41, "CLIC_NUAGE_DROITE", 0),
        (0x42, "CLIC_NUAGE_DROITE", 4),
        (0x41, "CLIC_NUAGE_DROITE", 3),
        (0x41, "CLIC_NUAGE_GAUCHE", 5),
        (0x42, "CLIC_NUAGE_GAUCHE", 5),
    ] {
        entered(
            &mut e,
            &[("Perle_cachee", perle), ("State_nuages_bas", low)],
        );
        e.dragon_click(&zone(3, zid(z), z));
        cmp.push(format!(
            "D clicked {z} perle {perle} low {low} -> {}",
            st(&e, &SHORT)
        ));
        d(&mut cmp, &mut e, "one", 1, &SHORT);
        d(&mut cmp, &mut e, "28 more", 28, &SHORT);
        d(&mut cmp, &mut e, "the 30th", 1, &SHORT);
        d(&mut cmp, &mut e, "the next", 1, &SHORT);
    }
    for (low, action) in [(2, 1), (2, 2), (0, 2), (4, 1), (2, 0), (2, 3), (6, 1)] {
        entered(
            &mut e,
            &[
                ("Time_nuage_transparent", 1),
                ("State_nuages_bas", low),
                ("Action_nuage_bas", action),
                ("Perle_cachee", 0x42),
            ],
        );
        d(
            &mut cmp,
            &mut e,
            &format!("time 1 low {low} action {action}"),
            1,
            &SHORT,
        );
    }
    entered(&mut e, &[("Time_nuage_transparent", -2)]);
    d(&mut cmp, &mut e, "time -2", 1, &SHORT);
    entered(&mut e, &[]);
    e.dragon.state = 7;
    d(&mut cmp, &mut e, "st 7", 1, &SHORT);
    // In view 32 with the mode left at 4: the pearls' slots are not
    // there (the original's fatal error).
    entered(
        &mut e,
        &[("Time_nuage_transparent", 1), ("Action_nuage_bas", 1)],
    );
    e.enter_at(32, 0.0, 0.0).unwrap();
    d(
        &mut cmp,
        &mut e,
        "time 1 in view 32",
        1,
        &["other", "mechanism"],
    );
    cmp.check();

    // I: the dragon's flight, step by step, after the click.
    entered(&mut e, &[]);
    e.dragon_click(&dragon);
    let mut seq = Vec::new();
    for _ in 0..84 {
        e.anim.step();
        seq.push(e.anim.slots[0].frame.to_string());
    }
    assert_eq!(
        rows("I ")[0],
        format!(
            "I dragon {} | playing {}",
            seq.join(" "),
            i32::from(e.anim.slots[0].playing)
        )
    );
}

/// The player of section E: the zone of `name` aimed at, clicked.
fn click_zone(e: &mut Engine, name: &str, t: &mut f64) {
    let view = e.place().1;
    let z = e
        .wam()
        .view(view)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.name.as_deref() == Some(name))
        .unwrap();
    let (yaw, pitch) = e.zone_aim(view, z).unwrap();
    e.camera.enter(yaw, pitch);
    *t += 100.0;
    e.click(*t);
}

#[test]
fn frames_as_the_warp_frame_runs_them() {
    let Some(files) = game() else { return };
    let mut e = in_chapter(files, 31, 58);
    let mut t = 0.0;
    let mut got = Vec::new();
    for head in rows("E run ") {
        got.push(head.to_string());
        let w: Vec<&str> = head.split(' ').collect();
        let (k, c0, answers, frames): (u64, u64, Vec<u8>, u64) = (
            w[3].parse().unwrap(),
            w[5].parse().unwrap(),
            w[7].bytes().collect(),
            w[9].parse().unwrap(),
        );
        entered(&mut e, &[]);
        let mut last = String::new();
        let mut clicked = false;
        let mut round = None;
        let mut ai = 0;
        for f in 0..frames {
            if f % k == 0 {
                e.tick();
            }
            let mut made = "-".to_string();
            if f == c0 {
                click_zone(&mut e, "CLIC_DRAGON", &mut t);
                made = "dragon".into();
            } else {
                let seq = var(&e, "Num_sequence_dragon");
                if round != Some(seq) {
                    round = Some(seq);
                    clicked = false;
                }
                if !clicked
                    && ai < answers.len()
                    && e.dragon.state == 0
                    && var(&e, "Time_nuage_transparent") == 0
                    && var(&e, "FINPERLE") == 0
                    && var(&e, "Perle_cachee") != 0
                {
                    let mut a = answers[ai];
                    if a == b'=' {
                        a = if var(&e, "Perle_cachee") == 0x41 {
                            b'L'
                        } else {
                            b'R'
                        };
                    }
                    let z = if a == b'L' {
                        "CLIC_NUAGE_GAUCHE"
                    } else {
                        "CLIC_NUAGE_DROITE"
                    };
                    click_zone(&mut e, z, &mut t);
                    made = (a as char).to_string();
                    ai += 1;
                    clicked = true;
                }
            }
            e.events.clear();
            e.pass(None);
            let evs = std::mem::take(&mut e.events);
            if made != "-" {
                assert!(evs.contains(&Event::PuzzleRan {
                    function: 0x0049_4600
                }));
            }
            let state = st(&e, &SHORT);
            let snd = sounds(&evs);
            let warn = warns(&mut e);
            let mut key = String::new();
            for part in state.split(' ') {
                let p = if part.starts_with("dragon:") || part.starts_with("other:") {
                    part.split('/').skip(1).collect::<Vec<_>>().join("/")
                } else {
                    part.to_string()
                };
                key.push_str(&p);
                key.push(' ');
            }
            let time = var(&e, "Time_nuage_transparent");
            let key = key.replace(
                &format!("time {time} "),
                if time == 0 { "time 0 " } else { "time n " },
            );
            if key != last || made != "-" || snd != "[]" || warn != "[]" {
                got.push(format!(
                    "E k {k} f {f} click {made} -> {state} | sounds []{snd} | warn {warn}"
                ));
                last = key;
            }
        }
    }
    let want: Vec<&str> = GOLDEN
        .lines()
        .filter(|l| l.starts_with("E run ") || l.starts_with("E k "))
        .collect();
    same(&got, &want);
}

#[test]
fn the_dispatchers_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter(files, 31, 58);
    let names = e.script_state().unwrap().names.clone();
    let dragon = names.id(defnames::ZONES_CLIC, b"CLIC_DRAGON").unwrap() as i32;
    let mut got = Vec::new();
    let mut want = Vec::new();
    for row in rows("F click ") {
        let (head, _) = row.split_once(" -> ").unwrap();
        let w: Vec<&str> = head.split(' ').collect();
        let (name, number, kind, mode): (&str, u32, i32, i32) = (
            w[2],
            w[3].parse().unwrap(),
            w[4].parse().unwrap(),
            w[6].parse().unwrap(),
        );
        // The engine's dispatcher (its Chinese part) on the row's warp
        // name, number, zone type and mode, the zone `CLIC_DRAGON`: the
        // hooks it reaches (their events; the mode 5 cleared outside
        // `SS` 1-7 with none), AL, the mode and the dragon's state after.
        entered(&mut e, &[]);
        e.set_puzzle_mode(mode);
        e.events.clear();
        let al = e.chinese_click(
            0,
            name.as_bytes(),
            number,
            &zone(kind, dragon, "CLIC_DRAGON"),
        );
        let entered: Vec<String> = e
            .events
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(hook_name(*function))
                }
                _ => None,
            })
            .collect();
        got.push(format!(
            "{head} -> {} AL {} mode {} st {}",
            if entered.is_empty() {
                "-".to_string()
            } else {
                entered.join(",")
            },
            i32::from(al),
            e.puzzle_mode(),
            e.dragon.state
        ));
        want.push(row.to_string());
    }
    for row in rows("F entry ") {
        let (head, _) = row.split_once(" -> ").unwrap();
        let w: Vec<&str> = head.split(' ').collect();
        let (name, number, mode): (&str, u32, i32) =
            (w[2], w[3].parse().unwrap(), w[5].parse().unwrap());
        fresh(&mut e, &[]);
        e.set_puzzle_mode(mode);
        let hooks = chinese_entry_hooks(name.as_bytes(), number);
        for &f in &hooks {
            if f == 0x0049_44a0 {
                e.dragon_entry();
            }
        }
        let entered: Vec<String> = hooks.iter().map(|&f| hook_name(f)).collect();
        got.push(format!(
            "{head} -> {} mode {} | {} | warn {}",
            if entered.is_empty() {
                "-".to_string()
            } else {
                entered.join(",")
            },
            e.puzzle_mode(),
            st(&e, &SHORT),
            warns(&mut e)
        ));
        want.push(row.to_string());
    }
    for row in rows("F tick ") {
        let w: Vec<&str> = row.split(' ').collect();
        let (mode, pause): (i32, i32) = (w[3].parse().unwrap(), w[5].parse().unwrap());
        if pause == 1 {
            // The port has no pause: the golden's paused frames run none.
            assert!(row.ends_with("-> -"));
            continue;
        }
        entered(&mut e, &[]);
        e.set_puzzle_mode(mode);
        e.events.clear();
        let f = e.puzzle_tick();
        // The turtle ring's step runs (`a2/pz-turtle`) and the Maya
        // marbles' (`a2/pz-grids`), noted as run; the souls'
        // (`a2/pz-labyrinth`), the spider web's (`a2/pz-web`) and the
        // octopus fight's (`a2/pz-octopus`) with no event, as this one;
        // the steps not ported are noted.
        match f {
            None | Some(0x0049_47b0 | 0x0048_eb30 | 0x0049_1610 | 0x0049_aaa0) => {}
            Some(function @ (0x0049_c400 | 0x0049_3190)) => {
                assert_eq!(e.events.first(), Some(&Event::PuzzleRan { function }));
            }
            Some(function) => assert_eq!(e.events, [Event::PuzzleNotPorted { function }]),
        }
        got.push(format!(
            "F tick mode {mode} pause 0 -> {}",
            f.map_or("-".to_string(), hook_name)
        ));
        want.push(row.to_string());
    }
    same(&got, &want);
}

#[test]
fn the_clicks_and_chapter_58_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_chapter(files, 31, 58);
    entered(&mut e, &[]);
    let mut got = Vec::new();
    let mut t = 0.0;
    let mut game_click = |e: &mut Engine,
                          label: &str,
                          view: u32,
                          name: &str,
                          got: &mut Vec<String>| {
        let z = e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some(name))
            .unwrap();
        let (yaw, pitch) = e.zone_aim(view, z).unwrap();
        e.camera.enter(yaw, pitch);
        e.events.clear();
        t += 100.0;
        e.click(t);
        e.pass(None);
        let evs = std::mem::take(&mut e.events);
        let hooks: Vec<String> = evs
            .iter()
            .filter_map(|x| match x {
                Event::PuzzleRan { function } | Event::PuzzleNotPorted { function } => {
                    Some(hook_name(*function))
                }
                _ => None,
            })
            .collect();
        let zc = evs
            .iter()
            .position(|x| matches!(x, Event::ZoneClick { .. }))
            .expect("the zone click");
        let Event::ZoneClick { cliczone, .. } = evs[zc] else {
            unreachable!()
        };
        let names: Vec<&str> = if view == 31 {
            SHORT.to_vec()
        } else {
            vec!["other", "mechanism"]
        };
        let mut state = st(e, &names);
        let warn = warns(e);
        let resumes = if e.loop_type == LoopType::Warp {
            0
        } else {
            settle(e)
        };
        let after = std::mem::take(&mut e.events);
        if view != 31 {
            // The original's harness stays on view 32's slots (its view
            // entry is a stub); the port has entered view 19 by then.
            state = state.split(" | ").next().unwrap().to_string();
        }
        got.push(format!(
            "G {label} view {view} {name} -> out {} | hooks {} sounds {} | cliczone {cliczone} | in {} films {} | resumes {resumes} {} chapter {} | VoyageDragon {} PerleInMecanisme {} hand {} | {state} | warn {warn}",
            lst(lines(&evs[..zc])),
            if hooks.is_empty() { "-".to_string() } else { hooks.join(",") },
            sounds(&evs[..zc]),
            lst(lines(&evs[zc..])),
            films(&evs),
            lst(lines(&after)),
            e.script_state().unwrap().chapter,
            var(e, "VoyageDragon"),
            var(e, "PerleInMecanisme"),
            var(e, "OBJETENMAIN"),
        ));
    };
    for z in ["CLIC_NUAGE_GAUCHE", "CLIC_DRAGON", "CLIC_NUAGE_DROITE"] {
        game_click(&mut e, z, 31, z, &mut got);
    }
    set(&mut e, "Perle_cachee", 0x41);
    e.dragon.state = 0;
    game_click(&mut e, "left, A hidden", 31, "CLIC_NUAGE_GAUCHE", &mut got);
    // The win's state, the pearl in the bag and in hand, view 32.
    set(&mut e, "FINPERLE", 1);
    e.run_command(b"(sacados=PERLE_TROU)");
    e.run_command(b"(objetenmain=PERLE_TROU)");
    e.enter_at(32, 0.0, 0.0).unwrap();
    e.set_puzzle_mode(0);
    game_click(&mut e, "the pearl laid", 32, "MECANISME_DRAGON", &mut got);
    let strip = |r: &str| -> String {
        if r.contains(" view 31 ") {
            return r.to_string();
        }
        // The state's slots left out (above).
        let (a, b) = r.rsplit_once(" | warn ").unwrap();
        let (a, _) = a.rsplit_once(" | ").unwrap();
        format!("{a} | warn {b}")
    };
    let want: Vec<String> = rows("G ").iter().map(|r| strip(r)).collect();
    same(&got, &want);
    assert_eq!(e.place(), ("miniature.wam", 19));
    assert_eq!(e.script_state().unwrap().chapter, 59);
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
fn view_31_is_drawn_as_the_original_with_the_blink() {
    let Some(files) = game() else { return };
    let bg = background();
    let bg_md5 = md5_of(&bg);
    assert!(GOLDEN.contains(&format!("md5 {bg_md5}")));
    let mut got = Vec::new();
    let golden: Vec<&str> = rows("H ")
        .into_iter()
        .filter(|r| !r.starts_with("H errors"))
        .collect();
    for row in golden.iter().filter(|r| r.starts_with("H file ")) {
        let w: Vec<&str> = row.split(' ').collect();
        let (short, name) = (w[2], w[3]);
        let data = files.read(Folder::Spw, name).expect("the SPW");
        let spw = Spw::parse(data.clone()).unwrap();
        let n = spw.frame_count();
        got.push(format!(
            "H file {short} {name} md5 {:x} frames {n}",
            Md5::digest(&data)
        ));
        let mut cases: Vec<(i32, i32, u32)> = (0..n).map(|f| (f, 0, 0)).collect();
        if ["left", "right", "dragon"].contains(&short) {
            cases.extend([
                (0, 0x32, 0),
                (0, 0x32, 1),
                (0, 0x32, 2),
                (0, 1, 7),
                (0, -1, 4),
            ]);
        }
        for (f, blink, counter) in cases {
            let mut a = Anim::new(0);
            let state = EntityState {
                letter: 65,
                anim: 1,
                shown: 1,
                single: 1,
                ..Default::default()
            };
            a.load("s", Spw::parse(data.clone()).unwrap(), 2, 105, 0, state);
            a.slots[0].frame = f;
            a.slots[0].blink = blink;
            a.counter = counter;
            let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
            let (_, hit) = a.draw(&mut faces, None, true);
            let al = i32::from(hit.slot.is_some());
            let mut parts = Vec::new();
            for (k, px) in faces.iter().enumerate() {
                let h = md5_of(px);
                if h == bg_md5 {
                    parts.push(format!("f{k} bg"));
                    continue;
                }
                let changed = px.iter().zip(&bg).filter(|(a, b)| a != b).count();
                let mut samples = Vec::new();
                if let Some(b) = spw.face_box(f, k) {
                    let (x, y, w, hh) = (
                        i32::from(b.x),
                        i32::from(b.y),
                        i32::from(b.w),
                        i32::from(b.h),
                    );
                    if w > 0 && hh > 0 {
                        for (sx, sy) in [(x + w / 2, y + hh / 2), (x, y), (x + w - 1, y + hh - 1)] {
                            samples.push(format!("{sx},{sy}={:04x}", px[(sy * 512 + sx) as usize]));
                        }
                    }
                }
                parts.push(format!("f{k} {h} n{changed} {}", samples.join(" ")));
            }
            let extra = if (blink, counter) != (0, 0) {
                format!(" b {blink} t {counter}")
            } else {
                String::new()
            };
            got.push(format!(
                "H {short} frame {f}{extra} AL {al} | {}",
                parts.join(" | ")
            ));
        }
    }
    same(&got, &golden);
}

/// The engine at 59.94 passes a second ([`PASS`]), the 15 Hz ticks run by its clock,
/// every pass drawn; the mouse as a player moves it.
struct Run {
    e: Engine,
    pass: u64,
    gap: f64,
    screen: Vec<u16>,
    shots: Option<PathBuf>,
    flight_shot: bool,
}

impl Run {
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

    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// A left click far from the last one (never a double click).
    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = &self.shots {
            std::fs::write(
                dir.join(format!("pz-dragon-{name}.png")),
                encode_rgb565(&self.screen, 640, 480),
            )
            .unwrap();
        }
    }

    fn var(&self, name: &str) -> i32 {
        var(&self.e, name)
    }

    /// Turns the view onto zone `zone` (the centre on it).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        assert_eq!(self.e.centre_zone().map(|(i, _)| i), Some(zone));
    }

    /// The zone of the current view named `name`.
    fn zone(&self, name: &str) -> usize {
        let view = self.e.place().1;
        self.e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no zone {name} in view {view}"))
    }

    /// A click on the move zone to view `to`; the films clicked away,
    /// until the panorama of view `to` is back.
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
        let mut still = 0;
        for _ in 0..3000 {
            let evs = self.events();
            let film = evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            });
            all.extend(evs);
            if film {
                self.wait(30);
                self.tap();
            }
            if self.e.place().1 == to && self.e.loop_type == LoopType::Warp && !self.e.turning() {
                still += 1;
                if still == 8 {
                    all.extend(std::mem::take(&mut self.e.events));
                    return all;
                }
            } else {
                still = 0;
            }
        }
        panic!("no arrival in view {to}");
    }

    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
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

    /// A click on the zone named `name` of the current view, the centre
    /// on it: the events of that pass.
    fn click_zone(&mut self, name: &str) -> Vec<Event> {
        let zone = self.zone(name);
        self.aim(zone);
        self.tap();
        self.events()
    }

    /// Passes until the dragon is still and no cloud is transparent; the
    /// events, and whether the dragon left frame 50 (its flight; the
    /// first one shot 60 passes in).
    fn until_still(&mut self) -> (Vec<Event>, bool) {
        let mut all = Vec::new();
        let mut flight = 0;
        for _ in 0..2000 {
            if self.e.dragon.state == 0 && self.var("Time_nuage_transparent") == 0 {
                return (all, flight > 0);
            }
            all.extend(self.events());
            if self.e.anim.slots[0].frame != 50 {
                flight += 1;
                if flight == 60 && !self.flight_shot {
                    self.shot("2-dragon-flying");
                    self.flight_shot = true;
                }
            }
        }
        panic!("the dragon never stops");
    }
}

#[test]
fn the_pearl_played_with_the_mouse_to_the_mechanism_and_chapter_59() {
    let Some(files) = game() else { return };
    // `miniature.wam` view 18 in chapter 56, the dragon not ridden yet.
    let mut e = in_chapter(files, 18, 56);
    assert_eq!(var(&e, "VoyageDragon"), 0);
    let shots = std::env::var_os("ATLANTIS2_SHOTS").map(PathBuf::from);
    if let Some(d) = &shots {
        std::fs::create_dir_all(d).unwrap();
    }
    e.events.clear();
    let mut r = Run {
        e,
        pass: 0,
        gap: 0.0,
        screen: vec![0; 640 * 480],
        shots,
        flight_shot: false,
    };
    r.wait(20);
    let hole = id(&r.e, defnames::OBJETS, "PERLE_TROU");

    // To view 30: chapter 56's departure towards it sets chapter 58
    // (`DEPARTVUE` is the view a move goes to). Into view 31: the clouds'
    // entry: the mode 4, the dragon held on frame 50, the pearls and the
    // pearl with a hole hidden, the low clouds down.
    r.walk(30);
    assert_eq!(r.e.script_state().unwrap().chapter, 58);
    let evs = r.walk(31);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_44a0
    }));
    assert_eq!(r.e.script_state().unwrap().chapter, 58);
    assert_eq!(r.e.puzzle_mode(), 4);
    let s = &r.e.anim.slots;
    assert_eq!((s[0].frame, s[0].playing), (50, false));
    assert!(!s[8].active && !s[9].active && !s[10].active);
    assert!(s[3..8].iter().all(|s| s.frame == 0 && !s.playing));
    let dragon = r.zone("CLIC_DRAGON");
    r.aim(dragon);
    r.wait(8);
    r.shot("1-clouds");

    // The clouds before the dragon is woken: nothing.
    r.click_zone("CLIC_NUAGE_GAUCHE");
    assert_eq!(r.var("Time_nuage_transparent"), 0);
    // The dragon: the first round, 'A' hidden, clicked on the pass after
    // a 15 Hz step.
    let ticks = r.e.ticks;
    while r.e.ticks == ticks {
        r.wait(1);
    }
    let evs = r.click_zone("CLIC_DRAGON");
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_4600
    }));
    assert_eq!(r.var("Perle_cachee"), 0x41);
    // No 15 Hz step falls in the two passes after this click (the click's
    // frame takes the last tick's step, the next tick runs 4 passes after
    // it, after the frame): the dragon is stopped on frame 50 at
    // once, without its flight (the original's race, `engine/dragon.rs`).
    let (_, flew) = r.until_still();
    assert!(!flew);
    // The rounds: 'A' left, 'B' right, after the sequence ABBAB; the
    // second answer wrong.
    let mut heard = Vec::new();
    let mut first = true;
    let mut flights = 0;
    for (k, wrong) in [false, true, false, false, false, false, false]
        .into_iter()
        .enumerate()
    {
        let hidden = r.var("Perle_cachee");
        let left = (hidden == 0x41) != wrong;
        let low = r.var("State_nuages_bas");
        r.click_zone(if left {
            "CLIC_NUAGE_GAUCHE"
        } else {
            "CLIC_NUAGE_DROITE"
        });
        assert_eq!(r.var("Time_nuage_transparent"), 29);
        assert_eq!(r.var("Action_nuage_bas"), if wrong { 2 } else { 1 });
        // The transparent cloud blinks with the 15 Hz counter, the pearl
        // shows through it when it is there.
        let cloud = if left { 1 } else { 2 };
        assert_eq!(r.e.anim.slots[cloud].blink, 0x32);
        assert_eq!(r.e.anim.slots[if left { 8 } else { 9 }].active, !wrong);
        if first || wrong {
            // Both halves of the blink, one 15 Hz tick apart.
            let mut last = None;
            for _ in 0..2 {
                r.wait(1);
                while last == Some(r.e.ticks & 1) {
                    r.wait(1);
                }
                let odd = r.e.ticks & 1;
                last = Some(odd);
                r.shot(&format!(
                    "3-round-{}-{}-{}",
                    k + 1,
                    if wrong { "wrong" } else { "right" },
                    if odd == 1 { "cloud" } else { "through" }
                ));
            }
            first = false;
        }
        let (evs, flew) = r.until_still();
        flights += usize::from(flew);
        heard.push(sounds(&evs));
        let now = r.var("State_nuages_bas");
        if wrong {
            assert_eq!(now, (low - 1).max(0));
        } else {
            assert_eq!(now, low + 1);
        }
        if now == 5 {
            break;
        }
    }
    assert_eq!(
        heard,
        [
            "[sound206.apc]",
            "[sound205.apc]",
            "[sound206.apc]",
            "[sound207.apc]",
            "[sound208.apc]",
            "[sound209.apc]",
            "[sound210.apc]"
        ]
    );
    // The rounds that start by themselves have three passes for a step:
    // the dragon flew in some (the last round's win starts none).
    assert!(flights > 0 && r.flight_shot);
    // The win: FINPERLE, the pearl with a hole shown, the dragon still.
    assert_eq!(r.var("FINPERLE"), 1);
    assert!(r.e.anim.slots[10].active);
    assert_eq!(r.e.anim.table.get(hole, 2), 1);
    assert!(r.e.anim.slots[3..8].iter().all(|s| s.frame == 1));
    // The dragon is not woken again.
    r.click_zone("CLIC_DRAGON");
    assert_eq!(r.e.dragon.state, 0);

    // The pearl taken: the centre on its sprite (a search over the view),
    // a click.
    let mut found = None;
    'search: for yi in 0..360 {
        for pi in 0..60 {
            let (yaw, pitch) = (
                -std::f32::consts::PI + yi as f32 * std::f32::consts::PI / 180.0,
                -0.6 + pi as f32 * 0.02,
            );
            r.e.camera.enter(yaw, pitch);
            r.e.pass(None);
            if r.e.sprite_under_centre().1 == 1 {
                found = Some((yaw, pitch));
                break 'search;
            }
        }
    }
    let (yaw, pitch) = found.expect("the pearl's sprite under the centre");
    r.e.camera.enter(yaw, pitch);
    r.wait(4);
    r.shot("4-pearl-won");
    r.tap();
    let evs = r.events();
    assert!(evs.contains(&Event::Take { id: hole }));
    assert_eq!(r.var("OBJETENMAIN"), hole);
    assert!(r.e.bag_entries().iter().any(|b| b.id == hole));
    assert!(!r.e.anim.slots[10].active);

    // To view 32: the clouds' move hook puts the mode back to 0; the
    // view's reset empties the hand, the pearl is picked from the bag.
    let evs = r.walk(32);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_45f0
    }));
    assert_eq!(r.e.puzzle_mode(), 0);
    assert_eq!(r.var("OBJETENMAIN"), 0);
    r.pick(hole);
    let mech = r.zone("MECANISME_DRAGON");
    r.aim(mech);
    r.wait(4);
    r.shot("5-mechanism");
    // The pearl laid in the dragon's mechanism with it in hand: chapter
    // 58's section of line 18, its lines 19-27 (the dragon's flight, film
    // 26, then view 19 and chapter 59).
    let evs = r.click_zone("MECANISME_DRAGON");
    assert_eq!(lines(&evs), [19]);
    assert_eq!(films(&evs), "[cine026.hnm]");
    r.wait(60);
    r.shot("6-film-26");
    r.tap();
    let mut evs = Vec::new();
    for _ in 0..600 {
        evs.extend(r.events());
        if r.e.loop_type == LoopType::Warp && !r.e.turning() {
            break;
        }
    }
    assert_eq!(lines(&evs), [20, 21, 22, 23, 24, 25, 26, 27]);
    r.wait(10);
    assert_eq!(r.e.place(), ("miniature.wam", 19));
    assert_eq!(r.e.script_state().unwrap().chapter, 59);
    assert_eq!(r.var("VoyageDragon"), 1);
    assert_eq!(r.var("PerleInMecanisme"), 1);
    assert!(!r.e.bag_entries().iter().any(|b| b.id == hole));
    r.shot("7-view-19-chapter-59");
}

#[test]
fn the_pearl_game_is_saved_and_entered_again_after_a_load() {
    let Some(files) = game() else { return };
    let mut e = in_chapter(files, 31, 58);
    assert_eq!(e.puzzle_mode(), 4);
    let names = e.script_state().unwrap().names.clone();
    let zid = |n: &str| names.id(defnames::ZONES_CLIC, n.as_bytes()).unwrap() as i32;
    // A round played to a transparent left cloud with the pearl behind.
    e.dragon_click(&zone(3, zid("CLIC_DRAGON"), "CLIC_DRAGON"));
    e.dragon.state = 0;
    e.dragon_click(&zone(3, zid("CLIC_NUAGE_GAUCHE"), "CLIC_NUAGE_GAUCHE"));
    for _ in 0..10 {
        e.dragon_tick();
    }
    set(&mut e, "State_nuages_bas", 3);
    let d = e.save_data();
    assert_eq!(d.entry(26), 4i32.to_le_bytes());
    set(&mut e, "State_nuages_bas", 0);
    e.load_data(&d).unwrap();
    settle(&mut e);
    // The entry run again: the transparency restored from the variables,
    // the low clouds from the count, the dragon held still.
    assert_eq!(e.place(), ("miniature.wam", 31));
    assert_eq!(e.puzzle_mode(), 4);
    assert_eq!(var(&e, "Perle_cachee"), 0x41);
    let s = &e.anim.slots;
    assert_eq!(s[1].blink, 0x32);
    assert!(s[8].active);
    assert_eq!((s[0].frame, s[0].playing), (50, false));
    let low: Vec<i32> = s[3..8].iter().map(|s| s.frame).collect();
    assert_eq!(low, [1, 1, 1, 0, 0]);
}
