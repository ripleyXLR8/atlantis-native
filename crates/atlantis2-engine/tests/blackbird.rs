//! The blackbird songs of Ireland (the poet in view 73 of `ilepoete.wam`)
//! against the original's emulated run (`tests/golden/blackbird.txt`):
//! the random draws, the view entry on sound-table states, the click in
//! each mode, the song under the pointer, the fixed menu, the leave, and
//! the warp frame pass by pass with chapter 20; then the game played with
//! the mouse as a player would, by ear, from chapter 20's arrival on the
//! poet's island to the poet standing up.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests that need it are skipped. With
//! `ATLANTIS2_PNG_DIR` set, the played game writes `pz-blackbird-*.png`
//! there.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::blackbird::{draw_songs, grid, random};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::Blackbird;
use atlantis2_engine::place_sound::Entry;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::sound::VOICE_EFFECTS;
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/blackbird.txt");

/// The C runtime's multiplier's inverse modulo 2³².
const INV: u32 = 0xb9b3_3155;

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

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// The golden's rows of section `s` (their text after `"s "`).
fn rows(s: &str) -> Vec<&'static str> {
    let head = format!("{s} ");
    GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix(head.as_str()))
        .collect()
}

/// The golden row of section `s` that starts with `label`, its text after
/// the label.
fn row(s: &str, label: &str) -> &'static str {
    rows(s)
        .into_iter()
        .find_map(|r| r.strip_prefix(label))
        .unwrap_or_else(|| panic!("no golden row {s} {label}"))
}

/// The state whose next `rand()` is `r`.
fn forced(r: u32) -> Rand {
    Rand::new((r << 16).wrapping_sub(2_531_011).wrapping_mul(INV))
}

/// How many `rand()` calls lead from `from` to `to` (at most 1000).
fn rand_calls(from: Rand, to: Rand) -> usize {
    let mut r = from;
    for n in 0..1000 {
        if r == to {
            return n;
        }
        r.rand();
    }
    panic!("rand state not reached");
}

fn join(v: &[i32]) -> String {
    v.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// The harness's `blackbird()` text.
fn bb(b: &Blackbird) -> String {
    format!(
        "mode {} count {} right {} flag51 {} songs {} handles {} bad {} good {}",
        b.mode,
        b.count,
        b.right,
        i32::from(b.flag),
        join(&b.songs),
        join(&b.handles),
        b.bad,
        b.good
    )
}

/// The golden's `flag51 5` (a value the harness writes before a call that
/// leaves the byte alone) read as the port's set flag.
fn norm(s: &str) -> String {
    s.replace("flag51 5", "flag51 1")
}

// ---- B, C: the random draws ------------------------------------------------

#[test]
fn random_and_songs_as_the_original() {
    for n in [3, 5, 10] {
        let mut raw = Vec::new();
        let mut per: std::collections::BTreeMap<i32, (u32, u32, u32)> = Default::default();
        for r in 0..32768_u32 {
            let v = random(&mut forced(r), n);
            raw.extend_from_slice(&v.to_le_bytes());
            let e = per.entry(v).or_insert((0, r, r));
            e.0 += 1;
            e.2 = r;
        }
        let cells: Vec<String> = per
            .iter()
            .map(|(v, (c, a, b))| format!("{v}:{c}@{a}..{b}"))
            .collect();
        let want = format!("md5 {} | {}", md5(&raw), cells.join(" "));
        assert_eq!(want, row("B", &format!("n={n} ")), "random({n})");
    }
    let seeds: Vec<u32> = (0..16)
        .chain([1000, 4321, 12345, 65535, 0x7fff_ffff, 0xffff_ffff])
        .collect();
    for s in seeds {
        let mut r = Rand::new(s);
        let songs = draw_songs(&mut r);
        let calls = rand_calls(Rand::new(s), r);
        let want = format!("songs {} calls {calls} state {:08x}", join(&songs), r.state);
        assert_eq!(want, row("C", &format!("seed {s}: ")));
    }
    let mut r = forced(32767);
    let songs = draw_songs(&mut r);
    assert_eq!(
        format!(
            "songs {} calls {}",
            join(&songs),
            rand_calls(forced(32767), r)
        ),
        row("C", "first rand 32767: ")
    );
    let mut r = forced(0);
    let songs = draw_songs(&mut r);
    assert_eq!(
        format!("songs {} calls {}", join(&songs), rand_calls(forced(0), r)),
        row("C", "first two rand 0, 0: ")
    );
}

// ---- E: the grid ---------------------------------------------------------------

#[test]
fn the_grids_as_the_original() {
    for (label, rows_) in [
        ("mode 1 (x-31, y-31, 62, 62, 1, 3, 1): ", 3),
        ("mode 3 and hover (x-31, y-31, 62, 62, 1, 5, 1): ", 5),
    ] {
        let mut raw = Vec::new();
        let mut cells: std::collections::BTreeMap<i32, [i32; 5]> = Default::default();
        for y in 130..480 {
            for x in 400..520 {
                let v = grid(419, 149, 62, 62, 1, rows_, true, (x, y));
                raw.extend_from_slice(&v.to_le_bytes());
                let b = cells.entry(v).or_insert([x, x, y, y, 0]);
                b[0] = b[0].min(x);
                b[1] = b[1].max(x);
                b[2] = b[2].min(y);
                b[3] = b[3].max(y);
                b[4] += 1;
            }
        }
        let text: Vec<String> = cells
            .iter()
            .map(|(v, b)| format!("{v}:{}..{},{}..{},n{}", b[0], b[1], b[2], b[3], b[4]))
            .collect();
        assert_eq!(
            format!("md5 {} | {}", md5(&raw), text.join(" ")),
            row("E", &format!("grid {label}"))
        );
    }
}

// ---- The engine in view 73 -----------------------------------------------------

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
        if e.loop_type == LoopType::Warp && e.dialog().is_none() {
            break;
        }
    }
    assert_eq!(e.loop_type, LoopType::Warp);
}

/// A game in chapter 20 on the poet's island, view 73, the arrival over.
fn view_73(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=5)",
        b"(chapter=2)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    for c in [
        &b"(changewam=ilepoete.wam)"[..],
        b"(chapter=20)",
        b"(vue=73)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("ilepoete.wam", 73));
    e.trace = true;
    e
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn set_var(e: &mut Engine, name: &str, v: i32) {
    assert!(e.script_state_mut().unwrap().set_var(name, v));
}

fn poet(e: &Engine) -> i32 {
    e.script_state()
        .unwrap()
        .names
        .id(defnames::PERSONNAGES, b"POETE_ASSIS")
        .unwrap() as i32
}

/// The sound calls of the events, as the harness writes them (its `stop`
/// rows aside): loads, the poet's questions, plays.
fn sounds(e: &Engine, evs: &[Event]) -> String {
    let (att, pan) = e.voices().levels(VOICE_EFFECTS).unwrap_or((0, 0));
    let v: Vec<String> = evs
        .iter()
        .filter_map(|x| match x {
            Event::SoundLoaded { name, entry: -1 } => Some(format!("load-fail {name}")),
            Event::SoundLoaded { name, entry } => Some(format!("load {name}>{entry}")),
            Event::DialogStart { number, .. } => Some(format!("RunOneCommand (dial={number})")),
            // A film's track (`cine*.apc`): the harness stubs its load.
            Event::Sound(n) if n.starts_with("cine") => None,
            Event::Sound(n) => Some(format!("play {n}>v2 att {att} pan {pan}")),
            _ => None,
        })
        .collect();
    if v.is_empty() {
        "-".into()
    } else {
        v.join("; ")
    }
}

/// The golden's sound calls without the `stop v2` rows (the port's
/// voices do not report their stops).
fn golden_sounds(s: &str) -> String {
    let v: Vec<&str> = s.split("; ").filter(|x| *x != "stop v2").collect();
    if v.is_empty() {
        "-".into()
    } else {
        v.join("; ")
    }
}

/// The script's fatal error as the golden's errors column starts it (the
/// format, its `%d` not filled).
fn errors(e: &mut Engine) -> String {
    let f = e.script_state_mut().unwrap().fatal.take();
    f.map_or("-".into(), |m| format!("fatal:{m}"))
}

fn first_error(g: &str) -> String {
    g.split(';')
        .next()
        .unwrap()
        .trim_end_matches("%d")
        .to_string()
}

fn table(e: &Engine) -> String {
    e.sound_table()
        .iter()
        .map(|(i, n)| format!("{i}:{n}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The harness's `fresh`: a view entry on an empty table, srand(1), then
/// the songs given.
fn fresh(e: &mut Engine, songs: [i32; 5]) {
    e.sound_table_clear();
    e.voices().stop(VOICE_EFFECTS);
    e.rand = Rand::new(1);
    e.blackbird_entry();
    e.blackbird.songs = songs;
    e.events.clear();
    if e.dialog().is_some() {
        e.end_dialog();
    }
}

/// The harness's `click`: the mode, registers (450, 180), the pointer,
/// `CLICPERSO`, `FINMERLE`, the count and right bird when given, the flag
/// set; then FUN_00495ae0; the row's result as the harness writes it.
#[allow(clippy::too_many_arguments)]
fn click(
    e: &mut Engine,
    mode: i32,
    ptr: (i32, i32),
    clicperso: i32,
    fin: i32,
    count: Option<i32>,
    right: Option<i32>,
) -> (String, String, String) {
    e.blackbird.mode = mode;
    e.subject_menu.column_x = 450;
    e.subject_menu.first_row = 180;
    e.pointer = ptr;
    set_var(e, "CLICPERSO", clicperso);
    set_var(e, "FINMERLE", fin);
    if let Some(c) = count {
        e.blackbird.count = c;
    }
    if let Some(r) = right {
        e.blackbird.right = r;
    }
    e.blackbird.flag = true;
    let before = e.rand;
    e.events.clear();
    e.blackbird_click();
    let evs = std::mem::take(&mut e.events);
    // The harness stubs AtlDial_start; the port's question starts the
    // dialog, which draws its camera from the same `rand` (dialogs.md):
    // the draws of the click itself are those before the dialog's.
    let mut calls = rand_calls(before, e.rand);
    if evs
        .iter()
        .any(|x| matches!(x, Event::DialogStart { started: true, .. }))
    {
        // The click's own draw is the first after `before`: random(5).
        let mut r = before;
        assert_eq!(random(&mut r, 5), e.blackbird.right);
        calls = rand_calls(before, r);
    }
    let head = format!(
        "{} {{3:93}} {} pointer {},{} rand calls {}",
        bb(&e.blackbird),
        var(e, "FINMERLE"),
        e.pointer.0,
        e.pointer.1,
        calls
    );
    let s = sounds(e, &evs);
    let err = errors(e);
    if e.dialog().is_some() {
        e.end_dialog();
    }
    (head, s, err)
}

/// Compares a click row with its golden text (`head | sounds | errors
/// ...`).
fn same(got: (String, String, String), want: &str, what: &str) {
    let parts: Vec<&str> = want.split(" | ").collect();
    assert_eq!(norm(&got.0), norm(parts[0]), "{what}: state");
    assert_eq!(got.1, golden_sounds(parts[1]), "{what}: sounds");
    let we = parts[2].strip_prefix("errors ").unwrap();
    if we == "-" {
        assert_eq!(got.2, "-", "{what}: errors");
    } else {
        assert!(
            got.2.starts_with(&first_error(we)),
            "{what}: {} / {we}",
            got.2
        );
    }
}

#[test]
fn the_view_entry_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view_73(files);
    type Setup = fn(&mut Engine);
    let cases: [(&str, Setup); 5] = [
        ("empty table", |_| {}),
        ("three entries taken", |e| {
            for n in ["merle1.apc", "merle2.apc", "merle3.apc"] {
                e.sound_load(n);
            }
        }),
        ("merleb.apc already loaded", |e| {
            e.sound_load("merleb.apc");
        }),
        ("entry 1 freed between two", |e| {
            for n in ["merle1.apc", "merle2.apc", "merle3.apc"] {
                e.sound_load(n);
            }
            e.sound_free(1, 2);
        }),
        ("table full", |e| {
            for (i, x) in e.place.table.iter_mut().enumerate() {
                *x = Some(Entry {
                    name: format!("x{i}"),
                    owner: None,
                    looping: false,
                });
            }
        }),
    ];
    // ("sound system off" is left out: the flag @ 0x00673f3c is set by the
    // sound's start-up, whether or not a card answers.)
    for (label, setup) in cases {
        e.sound_table_clear();
        e.blackbird.mode = 7;
        e.blackbird.flag = true;
        e.blackbird.handles = [0x1234; 5];
        e.blackbird.count = 9;
        e.pointer = (3, 4);
        e.events.clear();
        setup(&mut e);
        e.rand = Rand::new(1);
        e.blackbird_entry();
        let evs = std::mem::take(&mut e.events);
        let got = format!(
            "{} | rand calls {} | pointer {},{} | table {} | {}",
            bb(&e.blackbird),
            rand_calls(Rand::new(1), e.rand),
            e.pointer.0,
            e.pointer.1,
            table(&e),
            sounds(&e, &evs)
        );
        let want = row("D", &format!("{label}: "));
        let (w, werr) = want.rsplit_once(" | errors ").unwrap();
        assert_eq!(norm(&got), norm(w), "{label}");
        let err = errors(&mut e);
        if werr == "-" {
            assert_eq!(err, "-");
        } else {
            assert!(err.starts_with(&first_error(werr)), "{label}: {err}");
        }
        assert_eq!(e.cursor.shown, Some(40));
    }
}

#[test]
fn the_click_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view_73(files);
    let poet = poet(&e);
    let debout = e
        .script_state()
        .unwrap()
        .names
        .id(defnames::PERSONNAGES, b"POETE_DEBOUT")
        .unwrap() as i32;
    let no = (None, None);
    // F: the modes with no branch, the poet test.
    for mode in [0, 2, 4, -1] {
        for (cp, cpn) in [(0, "0"), (poet, "{0:31}"), (debout, "{0:32}")] {
            for fin in [0, 1, 2] {
                fresh(&mut e, [1, 2, 3, 4, 5]);
                let got = click(&mut e, mode, (450, 200), cp, fin, no.0, no.1);
                let label = format!("mode {mode} {{3:10}} {cpn} {{3:93}} {fin}: ");
                same(got, row("F", &label), &label);
            }
        }
    }
    // G: the round's start.
    let songs = [3, 7, 1, 10, 5];
    for ptr in [
        (450, 180),
        (419, 149),
        (481, 211),
        (481, 335),
        (482, 200),
        (450, 336),
        (418, 200),
        (450, 148),
        (481, 149),
    ] {
        fresh(&mut e, songs);
        e.rand = forced(0);
        let got = click(&mut e, 1, ptr, poet, 0, None, None);
        let label = format!("pointer {},{} {{3:10}} {{0:31}}: ", ptr.0, ptr.1);
        same(got, row("G", &label), &label);
    }
    for r in [
        0, 6553, 6554, 13107, 13108, 19660, 19661, 26213, 26214, 32766, 32767,
    ] {
        fresh(&mut e, songs);
        e.rand = forced(r);
        let got = click(&mut e, 1, (450, 200), poet, 0, None, None);
        let label = format!("rand {r}: ");
        same(got, row("G", &label), &label);
    }
    fresh(&mut e, songs);
    e.rand = forced(100);
    click(&mut e, 1, (450, 200), poet, 0, None, None);
    e.rand = forced(30000);
    let got = click(&mut e, 1, (450, 200), poet, 0, None, None);
    same(got, row("G", "round 2: "), "round 2");
    for v in -1..13 {
        fresh(&mut e, [v, 2, 3, 4, 5]);
        e.rand = forced(0);
        let got = click(&mut e, 1, (450, 200), poet, 0, None, None);
        let label = format!("songs[0] {v}: ");
        same(got, row("G", &label), &label);
    }
    fresh(&mut e, songs);
    e.rand = forced(0);
    let got = click(&mut e, 1, (450, 200), 0, 1, None, None);
    same(got, rows("G").last().unwrap(), "FINMERLE 1");
    // H: the answers.
    for right in 0..5 {
        for ptr in [
            (450, 180 + 62 * right),
            (450, 180 + 62 * ((right + 1) % 5)),
            (300, 300),
        ] {
            for cnt in [0, 1, 2] {
                fresh(&mut e, songs);
                e.rand = forced(0);
                click(&mut e, 1, (450, 200), poet, 0, None, None);
                e.voices().stop(VOICE_EFFECTS);
                let got = click(&mut e, 3, ptr, poet, 0, Some(cnt), Some(right));
                let label = format!("right {right} pointer {},{} count {cnt}: ", ptr.0, ptr.1);
                same(got, row("H", &label), &label);
            }
        }
    }
    for ptr in [
        (481, 149),
        (481, 210),
        (481, 211),
        (419, 459),
        (481, 459),
        (450, 460),
    ] {
        fresh(&mut e, songs);
        e.rand = forced(0);
        click(&mut e, 1, (450, 200), poet, 0, None, None);
        e.voices().stop(VOICE_EFFECTS);
        let got = click(&mut e, 3, ptr, 0, 0, Some(0), Some(5));
        let label = format!("right 5 pointer {},{} count 0: ", ptr.0, ptr.1);
        same(got, row("H", &label), &label);
    }
    let h = rows("H");
    let plain: Vec<&str> = h
        .iter()
        .copied()
        .filter(|r| r.starts_with("mode "))
        .collect();
    fresh(&mut e, songs);
    let got = click(&mut e, 3, (450, 180), poet, 0, Some(2), Some(0));
    same(got, plain[0], "the third right answer");
    for cnt in [3, 4] {
        fresh(&mut e, [1, 2, 3, 4, 5]);
        let got = click(&mut e, 3, (450, 180), 0, 0, Some(cnt), Some(0));
        same(got, row("H", &format!("count {cnt}: ")), "count");
    }
    for pre in [0, 3, 12] {
        e.sound_table_clear();
        for k in 0..pre {
            // Past ten, names with no file: their loads fail.
            let name = if k < 10 {
                format!("merle{}.apc", k % 10 + 1)
            } else {
                format!("x{k}.apc")
            };
            e.sound_load(&name);
        }
        e.voices().stop(VOICE_EFFECTS);
        e.rand = Rand::new(1);
        e.blackbird_entry();
        e.blackbird.songs = songs;
        e.events.clear();
        e.rand = forced(32767);
        let got = click(&mut e, 1, (450, 200), poet, 0, None, None);
        let label = format!("{pre} entries before: ");
        same(got, row("H", &label), &label);
    }
}

#[test]
fn the_song_under_the_pointer_and_the_menu_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view_73(files);
    let poet = poet(&e);
    fresh(&mut e, [3, 7, 1, 10, 5]);
    e.rand = forced(0);
    click(&mut e, 1, (450, 200), poet, 0, None, None);
    for mode in [2, 0, 1, 3] {
        for busy in [false, true] {
            for ptr in [
                (450, 150),
                (450, 211),
                (450, 212),
                (481, 459),
                (482, 300),
                (450, 460),
                (418, 300),
            ] {
                e.blackbird.mode = mode;
                e.subject_menu.column_x = 450;
                e.subject_menu.first_row = 180;
                e.pointer = ptr;
                e.voices().stop(VOICE_EFFECTS);
                if busy {
                    e.sound_play(e.blackbird.bad, VOICE_EFFECTS);
                }
                e.blackbird.hovered = (0x5555, 0x5555);
                e.events.clear();
                e.blackbird_hover();
                let evs = std::mem::take(&mut e.events);
                let evs: Vec<Event> = if busy { Vec::new() } else { evs };
                let got = format!(
                    "{} | 0x006d056c {} 0x006d0570 {}",
                    sounds(&e, &evs),
                    e.blackbird.hovered.0,
                    e.blackbird.hovered.1
                );
                let label = format!(
                    "mode {mode} voice2 {} pointer {},{}: ",
                    if busy { "busy" } else { "free" },
                    ptr.0,
                    ptr.1
                );
                assert_eq!(got, row("I", &label), "{label}");
            }
        }
    }
    // J: the fixed menu.
    e.subject_menu.column_x = 1;
    e.subject_menu.first_row = 2;
    e.subject_menu.open = false;
    e.ui_draws.clear();
    e.blackbird_menu_draw();
    let draws: Vec<String> = e
        .ui_draws
        .iter()
        .map(|d| match d {
            UiDraw::Sprite {
                bank: UiBank::Subjects,
                frame,
                x,
                y,
            } => format!("spr subjects#{frame}@{x},{y}"),
            other => format!("{other:?}"),
        })
        .collect();
    let got = format!(
        "draws {} | registers {},{} menu {} | errors -",
        draws.join("; "),
        e.subject_menu.column_x,
        e.subject_menu.first_row,
        i32::from(e.subject_menu.open)
    );
    assert_eq!(got, rows("J")[0]);
    // K: leaving.
    for (label, round) in [("after the entry", false), ("after a round", true)] {
        fresh(&mut e, [3, 7, 1, 10, 5]);
        if round {
            e.rand = forced(0);
            click(&mut e, 1, (450, 200), poet, 0, None, None);
            e.sound_play(e.blackbird.good, VOICE_EFFECTS);
        }
        let busy = e.voices().is_playing(VOICE_EFFECTS);
        e.blackbird.flag = true;
        e.music.state = 4;
        e.music.set_percent(20);
        e.cursor.shown = Some(40);
        e.blackbird_leave();
        let want = row("K", &format!("{label} (view 73): "));
        let parts: Vec<&str> = want.split(" | ").collect();
        assert_eq!(parts[0] == "stop v2", busy, "{label}");
        assert!(!e.voices().is_playing(VOICE_EFFECTS));
        assert_eq!(parts[1], "AtlMuzik_Volume(100)");
        assert_eq!(e.music.target, e.music.max);
        assert_eq!(e.cursor.shown, None);
        // The mode the harness left before (its rows run in order) is not
        // the port's here: compare the rest.
        let state = bb(&e.blackbird);
        let tail = |s: &str| s.split_once(" count ").unwrap().1.to_string();
        assert_eq!(tail(&state), tail(parts[2]), "{label}");
        assert_eq!(parts[3], "table -");
        assert_eq!(table(&e), "");
    }
}

// ---- L: the warp frame ---------------------------------------------------------

/// The camera of the frame rows: the poet under the centre, no zone there.
const FRAME_YAW: f32 = -77.5;
const FRAME_PITCH: f32 = -10.0;

/// The golden's state columns without the cursor and click ones (the
/// cursor's animation is the cursor choice's, tested in `tests/centre.rs`).
fn want_state(w: &str) -> String {
    let toks: Vec<&str> = w.split(' ').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i] == "click" || toks[i] == "cursor" {
            i += 2;
            continue;
        }
        out.push(toks[i]);
        i += 1;
    }
    out.join(" ")
}

fn state(e: &Engine) -> String {
    let s = e.script_state().unwrap();
    format!(
        "menu {} {{3:10}} {} {{3:92}} {} hit {} {{3:28}} {} {{3:93}} {} | {} | regs {},{}",
        i32::from(e.subject_menu.open),
        s.var("CLICPERSO").unwrap(),
        s.var("SUJET").unwrap(),
        e.sprite_under_centre().0.map_or(-1, |n| n as i32),
        s.var("OBJETENMAIN").unwrap(),
        s.var("FINMERLE").unwrap(),
        bb(&e.blackbird),
        e.subject_menu.column_x,
        e.subject_menu.first_row
    )
}

fn chapter_lines(evs: &[Event]) -> Vec<String> {
    evs.iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 20, line } => Some(format!("chapi020:{line}")),
            _ => None,
        })
        .collect()
}

fn want_lines(col: &str) -> Vec<String> {
    col.split("; ")
        .filter(|x| x.starts_with("chapi020:"))
        .map(str::to_string)
        .collect()
}

/// The golden's sound column with its `dial N` as the port's events show
/// a dialog started by the question.
fn want_sounds(col: &str) -> String {
    golden_sounds(col)
        .split("; ")
        .map(|x| match x.strip_prefix("dial ") {
            Some(n) => format!("RunOneCommand (dial={n})"),
            None => x.to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Section L replayed: the engine in view 73 with the harness's state,
/// then each pass of the golden with its pointer and actions (a click,
/// voice 2's sound ended, the `rand` state forced), the dialogs and the
/// film ended after it; the subjects bank's draws, the chapter lines, the
/// sound calls and the state compared.
#[test]
fn the_warp_frame_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = view_73(files);
    e.camera
        .enter(FRAME_YAW.to_radians(), FRAME_PITCH.to_radians());
    // The harness's state: the game reset, the sound table empty (the
    // place sounds are the harness's stubs), srand(1) before the entry.
    e.blackbird = Blackbird::default();
    e.subject_menu = Default::default();
    for (v, x) in [("SUJET", -2), ("CLICPERSO", 0), ("FINMERLE", 0)] {
        set_var(&mut e, v, x);
    }
    e.place.slots = Default::default();
    e.sound_table_clear();
    for v in 0..12 {
        e.voices().stop(v);
    }
    e.rand = Rand::new(1);
    e.blackbird_entry();
    e.events.clear();
    let block: Vec<&str> = GOLDEN
        .lines()
        .skip_while(|l| !l.starts_with("L poet"))
        .take_while(|l| !l.starts_with("frame errors"))
        .collect();
    let entry = block.iter().find(|l| l.starts_with("L poet")).unwrap();
    assert!(entry.contains(&format!("after the entry {}", bb(&e.blackbird))));
    let mut k = 0u64;
    let mut passes = 0;
    for l in block {
        if let Some(rest) = l.strip_prefix(&format!("p{k} ")) {
            let parts: Vec<&str> = rest.split(" | ").collect();
            let mut head = parts[0].split(' ');
            let ptr = head.next().unwrap();
            if ptr != "-" {
                let (x, y) = ptr.split_once(',').unwrap();
                e.pointer = (x.parse().unwrap(), y.parse().unwrap());
            }
            let acts: Vec<&str> = head.collect();
            if k.is_multiple_of(4) {
                e.tick();
            }
            e.set_clock(k as f64 / 60.0);
            let mut i = 0;
            while i < acts.len() {
                match acts[i] {
                    "click" => e.click(k as f64 * 10.0),
                    "end2" => e.voices().stop(VOICE_EFFECTS),
                    "rand" => {
                        e.rand = forced(acts[i + 1].parse().unwrap());
                        i += 1;
                    }
                    "" => {}
                    a => panic!("action {a}"),
                }
                i += 1;
            }
            e.events.clear();
            e.pass(None);
            let evs = std::mem::take(&mut e.events);
            let label = format!("pass {k}");
            let draws: Vec<String> = e
                .ui_draws
                .iter()
                .filter_map(|d| match d {
                    UiDraw::Sprite {
                        bank: UiBank::Subjects,
                        frame,
                        x,
                        y,
                    } => Some(format!("spr subjects#{frame}@{x},{y}")),
                    _ => None,
                })
                .collect();
            let wd: Vec<&str> = parts[1]
                .split("; ")
                .filter(|x| x.starts_with("spr subjects#"))
                .collect();
            assert_eq!(draws, wd, "{label}: draws");
            assert_eq!(chapter_lines(&evs), want_lines(parts[1]), "{label}: lines");
            // The scans (CtrlScenarIN, @ 0x00483c90): the departure, arrival
            // and click scans, with the variables they see.
            let scans: Vec<String> = evs
                .iter()
                .filter_map(|x| match x {
                    Event::DepartScan(v) | Event::ArrivalScan(v) | Event::ClickScan(v) => {
                        Some(format!(
                            "scan(DEPARTVUE={},ARRIVEEVUE={},CLICZONE={},{{3:10}}={},CLICOBJET={})",
                            v.departvue, v.arriveevue, v.cliczone, v.clicperso, v.clicobjet
                        ))
                    }
                    _ => None,
                })
                .collect();
            let ws: Vec<&str> = parts[1]
                .split("; ")
                .filter(|x| x.starts_with("scan(") && x.contains("CLICOBJET"))
                .collect();
            assert_eq!(scans, ws, "{label}: scans");
            assert_eq!(sounds(&e, &evs), want_sounds(parts[2]), "{label}: sounds");
            let want = format!("{} | {} | {}", parts[3], parts[4], parts[5]);
            assert_eq!(norm(&state(&e)), norm(&want_state(&want)), "{label}: state");
            k += 1;
            passes += 1;
        } else if let Some(rest) = l
            .strip_prefix("   dialog end | ")
            .or_else(|| l.strip_prefix("   video end, section resumed | "))
        {
            if l.contains("video end") {
                assert_eq!(e.loop_type, LoopType::Video);
                e.end_video();
            } else {
                assert!(e.dialog().is_some(), "a dialog after pass {}", k - 1);
                e.end_dialog();
            }
            let evs = std::mem::take(&mut e.events);
            let parts: Vec<&str> = rest.split(" | ").collect();
            assert_eq!(
                chapter_lines(&evs),
                want_lines(parts[0]),
                "after pass {}",
                k - 1
            );
            let want = format!("{} | {} | {}", parts[2], parts[3], parts[4]);
            assert_eq!(
                norm(&state(&e)),
                norm(&want_state(&want)),
                "after pass {}",
                k - 1
            );
        }
    }
    assert_eq!(passes, 58);
    assert_eq!(var(&e, "FINMERLE"), 2);
}

// ---- The game played with the mouse -------------------------------------------

/// The engine at 60 passes a second (a 15 Hz tick every fourth), every
/// pass drawn and its sound mixed (as a sound device would take it), so
/// the songs end as they do for a player.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
    audio: Vec<f32>,
    gap: f64,
    png: Option<PathBuf>,
}

impl Run {
    fn now(&self) -> f64 {
        self.pass as f64 / 60.0
    }

    fn step(&mut self) {
        if self.pass.is_multiple_of(4) {
            self.e.tick();
        }
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.e.voices().fill(&mut self.audio, 22050, 2);
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

    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// The mouse moved so that the pointer is at (x, y).
    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
    }

    fn save(&self, name: &str) {
        if let Some(dir) = &self.png {
            std::fs::create_dir_all(dir).unwrap();
            let p = dir.join(format!("pz-blackbird-{name}.png"));
            std::fs::write(p, encode_rgb565(&self.screen, 640, 480)).unwrap();
        }
    }

    /// A click on the move zone to view `to`, the transition clicked
    /// away; the events, in order.
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
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        self.tap();
        let mut all = Vec::new();
        for _ in 0..600 {
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
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// Turns the view until character `id` is under the centre (the
    /// cursor shows a character there) and clicks it; the click's events.
    fn click_character(&mut self, id: i32) -> Vec<Event> {
        for pitch in (-8..=8).map(|p| p * 5) {
            for yaw in (0..72).map(|y| y * 5 - 180) {
                let (y, p) = ((yaw as f32).to_radians(), (pitch as f32).to_radians());
                self.e.camera.enter(y, p);
                self.step();
                if self.e.sprite_under_centre().1 == 0 {
                    continue;
                }
                self.tap();
                let evs = self.events();
                if evs
                    .iter()
                    .any(|x| matches!(x, Event::SubjectMenu { character, .. } if *character == id))
                {
                    return evs;
                }
            }
        }
        panic!("character {id} not found in view {}", self.e.place().1);
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
                all.extend(std::mem::take(&mut self.e.events));
                return all;
            }
        }
        panic!("the dialog does not end");
    }

    /// A film played to its end, clicked away after a second.
    fn watch(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        self.wait(60);
        self.tap();
        for _ in 0..2000 {
            all.extend(self.events());
            if self.e.loop_type != LoopType::Video {
                return all;
            }
        }
        panic!("the film does not end");
    }

    /// The pointer over each bird in turn, each song heard to its end:
    /// the song of each bird.
    fn hear_the_birds(&mut self) -> [i32; 5] {
        let mut songs = [0; 5];
        for (k, song) in songs.iter_mut().enumerate() {
            self.point(450, 180 + 62 * k as i32);
            for _ in 0..1200 {
                let evs = self.events();
                if let Some(n) = evs.iter().find_map(|x| match x {
                    Event::Sound(n) => n.strip_prefix("merle").and_then(|n| n.strip_suffix(".apc")),
                    _ => None,
                }) {
                    *song = n.parse().unwrap_or(-1);
                    break;
                }
            }
            assert_ne!(*song, 0, "bird {k}: no song heard");
            // Heard to its end.
            for _ in 0..1200 {
                if !self.e.voices().is_playing(VOICE_EFFECTS) {
                    break;
                }
                self.step();
            }
            assert!(!self.e.voices().is_playing(VOICE_EFFECTS));
        }
        songs
    }

    /// A round: the poet clicked, subject `k` of his menu clicked, his
    /// question heard; the song he asks for.
    fn ask(&mut self, poet: i32, k: i32, all: &mut Vec<Event>) -> i32 {
        let evs = self.click_character(poet);
        assert_eq!(self.e.blackbird.mode, 1, "the poet's click sets mode 1");
        all.extend(evs);
        let (x, y) = cell(self.e.subject_count(poet), k);
        self.wait(2);
        self.point(x + 25, y + 25);
        self.wait(2);
        self.save(&format!("subjects-{}", self.pass));
        self.tap();
        let evs = self.events();
        let question = evs
            .iter()
            .find_map(|x| match x {
                Event::DialogStart {
                    number,
                    started: true,
                } => Some(*number as i32),
                _ => None,
            })
            .expect("the poet's question");
        assert!((90..=99).contains(&question));
        assert_eq!(self.e.blackbird.mode, 2);
        all.extend(evs);
        self.wait(30);
        self.save(&format!("question-{question}"));
        all.extend(self.listen());
        // The bird menu shows; a pass, then the mouse is free to move.
        self.step();
        assert!(self.e.subject_menu.open);
        question - 89
    }
}

/// Every `/set` line of an Irish chapter that ran is one of the lines the
/// original's handler ran (`scripts_worlds_ireland.txt`).
fn ireland_lines_as_the_original(evs: &[Event]) {
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

fn lines_of(evs: &[Event], chapter: i32) -> Vec<usize> {
    evs.iter()
        .filter_map(|x| match x {
            Event::Set { chapter: c, line } if *c == chapter => Some(*line),
            _ => None,
        })
        .collect()
}

#[test]
fn the_blackbirds_played_with_the_mouse() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        &b"(monde=1)"[..],
        b"(changewam=bigile1.wam)",
        b"(vue=5)",
        b"(chapter=2)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    // What chapter 14's section on the big horse leaves (its film 7 left
    // out): the poet's island, view 71, chapter 20.
    for c in [
        &b"(FLAG46=1)"[..],
        b"(killsuj=AILILL,CHEVAL)",
        b"(changewam=ilepoete.wam)",
        b"(vue=71)",
        b"(chapter=20)",
    ] {
        e.run_command(c);
    }
    settle(&mut e);
    assert_eq!(e.place(), ("ilepoete.wam", 71));
    assert_eq!(e.script_state().unwrap().chapter, 20);
    e.trace = true;
    let mut r = Run {
        e,
        pass: 0,
        screen: vec![0; 640 * 480],
        audio: vec![0.0; 368 * 2],
        gap: 0.0,
        png: std::env::var_os("ATLANTIS2_PNG_DIR").map(PathBuf::from),
    };
    r.wait(10);
    let poet = poet(&r.e);
    let mut all = Vec::new();
    for to in [72, 73] {
        all.extend(r.walk(to));
    }
    assert!(all.contains(&Event::PuzzleRan {
        function: 0x0049_58f0
    }));
    assert!(!all.iter().any(|x| matches!(
        x,
        Event::PuzzleNotPorted {
            function: 0x0049_58f0
        }
    )));
    r.wait(4);
    r.save("1-view73");

    // Round 1: right. Round 2: a wrong bird, the count back to 0. Then
    // three right answers in a row.
    let mut wins = 0;
    for round in 0..5 {
        let song = r.ask(poet, round % 3, &mut all);
        r.save(&format!("{}-birds", round + 2));
        let songs = r.hear_the_birds();
        let right = songs
            .iter()
            .position(|&s| s == song)
            .expect("the asked song is one of the birds'") as i32;
        let pick = if round == 1 { (right + 1) % 5 } else { right };
        r.point(450, 180 + 62 * pick);
        r.step();
        r.e.voices().stop(VOICE_EFFECTS);
        r.tap();
        let evs = r.events();
        let answer = if pick == right {
            "merleb.apc"
        } else {
            "merlem.apc"
        };
        assert!(evs.contains(&Event::Sound(answer.into())), "round {round}");
        wins = if pick == right { wins + 1 } else { 0 };
        assert_eq!(r.e.blackbird.count, wins);
        assert_eq!(r.e.blackbird.mode, 0);
        if wins == 3 {
            // FINMERLE = 1, seen by the click scan of the same frame:
            // chapter 20's section hides the sitting poet and starts film
            // 14.
            assert_eq!(lines_of(&evs, 20), [66, 67]);
            assert!(evs
                .iter()
                .any(|x| matches!(x, Event::VideoStart { name, .. } if name == "cine014.hnm")));
            all.extend(evs);
            break;
        }
        all.extend(evs);
        assert_eq!(var(&r.e, "FINMERLE"), 0);
        r.wait(30);
    }
    assert_eq!(wins, 3);
    // After the film: the standing poet shown with his subjects, dialog
    // 73, FINMERLE = 2, flag 62.
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.wait(30);
    r.save("7-film");
    all.extend(r.watch());
    assert!(r.e.dialog().is_some());
    r.wait(30);
    r.save("8-dialog-73");
    all.extend(r.listen());
    assert_eq!(var(&r.e, "FINMERLE"), 2);
    assert_eq!(r.e.script_state().unwrap().flags[62], 1);
    let lines = lines_of(&all, 20);
    let win: Vec<usize> = lines
        .iter()
        .copied()
        .filter(|l| (66..=73).contains(l))
        .collect();
    assert_eq!(win, [66, 67, 68, 69, 70, 71, 72, 73]);
    ireland_lines_as_the_original(&all);
    let debout =
        r.e.script_state()
            .unwrap()
            .names
            .id(defnames::PERSONNAGES, b"POETE_DEBOUT")
            .unwrap() as i32;
    let shown = |e: &Engine, id: i32| e.anim.slots.iter().any(|s| s.id == id && s.active);
    assert!(!shown(&r.e, poet));
    assert!(shown(&r.e, debout));
    r.wait(8);
    r.save("9-standing");

    // What the win unlocks: the standing poet's subjects (chapter 20's
    // section /003): his first subject, `POETE`, asked: dialog 64.
    let mut evs = r.click_character(debout);
    let (x, y) = cell(r.e.subject_count(debout), 0);
    r.wait(2);
    r.point(x + 25, y + 25);
    r.wait(2);
    r.save("10-standing-subjects");
    r.tap();
    evs.extend(r.events());
    assert!(evs.contains(&Event::DialogStart {
        number: 64,
        started: true
    }));
    evs.extend(r.listen());
    assert_eq!(lines_of(&evs, 20), [77, 78, 79]);
    assert_eq!(r.e.script_state().unwrap().flags[50], 1);
    ireland_lines_as_the_original(&evs);
    assert_eq!(r.e.loop_type, LoopType::Warp);
}
