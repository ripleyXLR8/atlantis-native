//! Shamballa's fireflies (`voiles.wam` view 1, chapter 78) against the
//! original (`tests/golden/fireflies.txt`, its header there): the click
//! hook alone on every zone type, value and count (rows A, B), whole
//! games of six fireflies for six placements of Tibet's mandalas (C: all
//! 720 orders each, the answer decided by the sixth firefly alone), the
//! move end without a view under both x87 control words (D: the field of
//! view, the automatic way on and its lines), the place-name test (E), the
//! dispatcher's choice (F), and the click as the game runs it, the
//! fireflies aimed and clicked one after the other, their moves played to
//! their ends, with the lines of chapter 78 each runs (G).
//!
//! The puzzle draws nothing of its own: a firefly lit is chapter 78's
//! `showObjet` of its sprite, drawn as every place sprite.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::wam::Zone;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::{
    fireflies_place, firefly_order_byte, shamballa_click_hooks, FIREFLIES_CLICK,
    FIREFLIES_MOVE_END, FIREFLY_MANDALA,
};
use atlantis2_engine::{Camera, Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/fireflies.txt");

/// The variables of the rows' state, in their order.
const SHOWN: [&str; 3] = [
    "NbLucioleAllumed",
    "Puzzle_Lucioles_Fini",
    "Puzzle_Lucioles_AutoWin",
];

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

fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
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

/// A new game's film ended, then Shamballa as chapter 75's crystal leaves
/// it: the world, the place, view 1, chapter 78 (its `/INIT` run).
fn in_shamballa(files: A2Files, fpu: Fpu) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), fpu, 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        "(monde=4)",
        "(changewam=voiles.wam)",
        "(vue=1)",
        "(chapter=78)",
    ] {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    assert_eq!(e.place(), ("voiles.wam", 1));
    assert_eq!(e.script_state().unwrap().chapter, 78);
    e.trace = true;
    e
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn set(e: &mut Engine, name: &str, v: i32) {
    assert!(e.script_state_mut().unwrap().set_var(name, v), "{name}");
}

fn state(e: &Engine) -> String {
    let v: Vec<String> = SHOWN.iter().map(|k| format!("{k}={}", var(e, k))).collect();
    v.join(" ")
}

fn speeds(e: &Engine) -> String {
    format!("{}/{}", e.look.yaw_speed, e.look.pitch_speed)
}

fn set_order(e: &mut Engine, om: &[i32]) {
    for (k, &o) in om.iter().enumerate() {
        set(e, &format!("OrdreMandala{}", k + 1), o);
    }
}

/// `OrdreMandala1..6=a,b,c,d,e,f` of a row.
fn order_of(row: &str) -> Vec<i32> {
    row.split("OrdreMandala1..6=")
        .nth(1)
        .unwrap()
        .split([':', ' '])
        .next()
        .unwrap()
        .split(',')
        .map(|x| x.parse().unwrap())
        .collect()
}

/// The hook alone on a zone {type, value}: the calls and errors fields of
/// a row. The fatal error is `DefName_GetIdName`'s, which saves the game
/// and quits in the original (the harness's stub returns, so its rows go
/// on to the sound; the game never reaches it): compared up to the fatal.
fn click(e: &mut Engine, kind: i32, value: i32) -> (String, String) {
    let zone = Zone {
        kind,
        value,
        name: None,
        points: Vec::new(),
    };
    e.events.clear();
    {
        let s = e.script_state_mut().unwrap();
        s.warnings.clear();
        s.fatal = None;
    }
    e.fireflies_click(&zone);
    // `RunOneCommand("(NewSound=300)")`: `sound300.apc` is on no disc,
    // the engine notes the command.
    assert!(!e.events.iter().any(|x| matches!(x, Event::Sound(_))));
    let sounds = e
        .events
        .iter()
        .filter(|x| **x == Event::FireflySound)
        .count();
    let calls = match sounds {
        0 => "-".to_string(),
        1 => "RunOneCommand('(NewSound=300)')".to_string(),
        n => format!("{n} sounds"),
    };
    let s = e.script_state().unwrap();
    let mut errors: Vec<String> = s.warnings.iter().map(|w| format!("warning:{w}")).collect();
    if let Some(f) = &s.fatal {
        assert!(f.starts_with("DefName_GetIdName: Nom de #VARIABLES# inconnu:OrdreMandala"));
        errors.push("fatal:DefName_GetIdName: Nom de %s inconnu:%s".into());
    }
    let errors = if errors.is_empty() {
        "-".to_string()
    } else {
        errors.join(",")
    };
    (calls, errors)
}

#[test]
fn the_click_alone_against_the_original() {
    // The table as the golden reads it from the executable.
    let t = rows("# table")[0].rsplit(": ").next().unwrap();
    let t: Vec<u8> = t.split(' ').map(|x| x.parse().unwrap()).collect();
    assert_eq!(t, FIREFLY_MANDALA);

    let Some(files) = game() else { return };
    let mut e = in_shamballa(files, Fpu::default());
    set_order(&mut e, &[1, 2, 3, 4, 5, 6]);
    let a = rows("A ");
    assert_eq!(a.len(), 6 * 2 * 6);
    for want in a {
        let head = want.split(": ").next().unwrap();
        let f: Vec<i32> = head
            .split(' ')
            .skip(1)
            .map(|x| x.split('=').nth(1).unwrap().parse().unwrap())
            .collect();
        let (kind, n, v) = (f[0], f[1], f[2]);
        set(&mut e, "NbLucioleAllumed", n);
        set(&mut e, "Puzzle_Lucioles_Fini", 0);
        set(&mut e, "Puzzle_Lucioles_AutoWin", 0);
        (e.look.yaw_speed, e.look.pitch_speed) = (1.5, -2.0);
        let (calls, errors) = click(&mut e, kind, v);
        let got = format!(
            "{head}: {} speeds={} calls={calls} errors={errors}",
            state(&e),
            speeds(&e)
        );
        assert_eq!(got, want);
    }

    // Values outside 1..6: the warning, the count kept, the speeds kept,
    // the name looked up not a variable: fatal.
    let b = rows("B ");
    assert_eq!(b.len(), 6);
    for want in b {
        let head = want.split(": ").next().unwrap();
        let v: i32 = head
            .split(' ')
            .nth(1)
            .unwrap()
            .trim_start_matches("v=")
            .parse()
            .unwrap();
        set(&mut e, "NbLucioleAllumed", 5);
        set(&mut e, "Puzzle_Lucioles_Fini", 0);
        (e.look.yaw_speed, e.look.pitch_speed) = (1.5, 1.5);
        // The byte the original reads @ 0x005e1d38 + v, and the name it
        // builds from it (`OrdreMandala%d`).
        let byte: i8 = head.rsplit("byte=").next().unwrap().parse().unwrap();
        assert_eq!(firefly_order_byte(v), Some(byte), "{head}");
        let (calls, errors) = click(&mut e, 1, v);
        assert_eq!(calls, "-", "the fatal ends the click");
        assert_eq!(
            e.script_state().unwrap().fatal.as_deref(),
            Some(
                format!("DefName_GetIdName: Nom de #VARIABLES# inconnu:OrdreMandala{byte}")
                    .as_str()
            ),
            "{head}"
        );
        let got = format!(
            "{head}: {} speeds={} calls=RunOneCommand('(NewSound=300)') errors={errors}",
            state(&e),
            speeds(&e)
        );
        assert_eq!(got, want);
    }
    e.script_state_mut().unwrap().fatal = None;
}

/// The k-th firefly the intended way: the one whose mandala was laid k-th.
fn intended(om: &[i32]) -> Vec<i32> {
    (1..=6)
        .map(|k| {
            (1..=6)
                .find(|&v| om[FIREFLY_MANDALA[v as usize] as usize - 1] == k)
                .unwrap()
        })
        .collect()
}

fn lit(e: &mut Engine, clicks: &[i32]) -> String {
    set(e, "NbLucioleAllumed", 0);
    set(e, "Puzzle_Lucioles_Fini", 0);
    let row: Vec<String> = clicks
        .iter()
        .map(|&v| {
            click(e, 1, v);
            format!(
                "{v}->n{}/f{}",
                var(e, "NbLucioleAllumed"),
                var(e, "Puzzle_Lucioles_Fini")
            )
        })
        .collect();
    row.join(" ")
}

/// Every order of the six fireflies, for six placements in Tibet.
#[test]
fn whole_games_against_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files, Fpu::default());
    let c = rows("C ");
    let intended_rows = rows("  intended: ");
    let same_rows = rows("  same six, then 1, then it again: ");
    assert_eq!((c.len(), intended_rows.len(), same_rows.len()), (6, 6, 6));
    for ((want, want_i), want_s) in c.iter().zip(intended_rows).zip(same_rows) {
        let om = order_of(want);
        set_order(&mut e, &om);
        let mut wins = 0;
        let mut lasts = std::collections::BTreeSet::new();
        let mut by_last: std::collections::BTreeMap<i32, std::collections::BTreeSet<bool>> =
            Default::default();
        let mut perm = [1, 2, 3, 4, 5, 6];
        permutations(&mut perm, 0, &mut |p| {
            set(&mut e, "NbLucioleAllumed", 0);
            set(&mut e, "Puzzle_Lucioles_Fini", 0);
            for &v in p {
                click(&mut e, 1, v);
            }
            let won = var(&e, "Puzzle_Lucioles_Fini") == 1;
            by_last.entry(p[5]).or_default().insert(won);
            if won {
                wins += 1;
                lasts.insert(p[5]);
            }
        });
        let exact = by_last.values().all(|s| s.len() == 1);
        let lasts: Vec<String> = lasts.iter().map(|v| v.to_string()).collect();
        let tail = format!(
            "wins {wins}/720 last values [{}] decided by the last value alone: {}",
            lasts.join(", "),
            u8::from(exact)
        );
        assert!(want.ends_with(&tail), "{want}\n{tail}");
        let i = intended(&om);
        assert_eq!(format!("  intended: {}", lit(&mut e, &i)), want_i);
        let mut same = vec![i[5]; 6];
        same.extend([1, i[5]]);
        assert_eq!(
            format!("  same six, then 1, then it again: {}", lit(&mut e, &same)),
            want_s
        );
    }
}

/// Heap's algorithm is not needed: the 720 orders in any order (the rows
/// count them).
fn permutations(p: &mut [i32; 6], k: usize, f: &mut dyn FnMut(&[i32; 6])) {
    if k == p.len() {
        f(p);
        return;
    }
    for i in k..p.len() {
        p.swap(k, i);
        permutations(p, k + 1, f);
        p.swap(k, i);
    }
}

/// The `/set` lines of chapter 78 in `events`.
fn lines(events: &[Event]) -> String {
    let l: Vec<String> = events
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 78, line } => Some(line.to_string()),
            _ => None,
        })
        .collect();
    if l.is_empty() {
        "-".into()
    } else {
        l.join(",")
    }
}

fn films(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|x| match x {
            Event::VideoStart { name, .. } => Some(name.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

/// The calls of the win's scan the engine's events show, as the golden's
/// stubs name them.
fn win_calls(events: &[Event], e: &Engine) -> String {
    let mut c = Vec::new();
    if films(events) == ["cine062.hnm"] {
        c.push("AtlHnm_start('cine062.hnm')".to_string());
    }
    if events.contains(&Event::Wam("rhea.wam".into())) {
        c.push("AtlWam_Load('rhea.wam')".to_string());
    }
    if e.script_state().unwrap().chapter == 80 {
        c.push("LoadCurrentChap(80)".to_string());
    }
    c.join(",")
}

/// The golden's calls cut to those [`win_calls`] shows.
fn cut_calls(calls: &str) -> String {
    calls
        .split("),")
        .map(|c| c.trim_end_matches(')').to_string() + ")")
        .filter(|c| {
            c.starts_with("AtlHnm_start")
                || c.starts_with("AtlWam_Load")
                || c.starts_with("LoadCurrentChap")
        })
        .collect::<Vec<_>>()
        .join(",")
}

const SET_FOV: &str = "SetFov(ecx=0x660f48, 0x3fa9c91f, 640, 480, 640, 480)";

/// The camera as FUN_004952d0 leaves it: 76° (0x3fa9c91f) over 640x480.
fn fov_76(e: &Engine) -> bool {
    e.camera.projection() == Camera::new(e.camera.fpu, 640, 480).projection()
}

#[test]
fn the_move_end_against_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_shamballa(files, Fpu::default());
    for want in rows("D ") {
        let (head, rest) = want.split_once(": ").unwrap();
        let cw = head.split(' ').nth(1).unwrap();
        let fini: i32 = head.rsplit('=').next().unwrap().parse().unwrap();
        let fpu = match cw {
            "cw=0x027f" => Fpu::Double,
            "cw=0x0e7f" => Fpu::Chop,
            _ => unreachable!(),
        };
        if e.place().0 != "voiles.wam" {
            e = in_shamballa(game().unwrap(), Fpu::default());
        }
        // The control word the camera's arithmetic runs under (the warp
        // frames leave 0x0e7f).
        e.camera.fpu = fpu;
        set(&mut e, "Puzzle_Lucioles_Fini", fini);
        set(&mut e, "Puzzle_Lucioles_AutoWin", 0);
        // Any other field of view before (a firefly's zoom leaves 35°).
        e.camera.set_fov(35f32.to_radians(), 640, 480);
        e.events.clear();
        e.move_end_without_view();
        assert!(fov_76(&e));
        let mut resumes = 0;
        let mut evs = std::mem::take(&mut e.events);
        if e.loop_type == LoopType::Video {
            resumes = 1;
            settle(&mut e);
            evs.append(&mut e.events);
        }
        assert!(evs.contains(&Event::PuzzleRan {
            function: FIREFLIES_MOVE_END
        }));
        let want_calls = rest.rsplit("calls=").next().unwrap();
        let golden_calls = cut_calls(want_calls);
        let chapter = e.script_state().unwrap().chapter;
        let got = format!(
            "{head}: {} lines {} resumes={resumes} chapter_cur={chapter}",
            state(&e),
            lines(&evs)
        );
        assert_eq!(got, want.split(" calls=").next().unwrap());
        assert!(want_calls.starts_with(SET_FOV), "{want}");
        assert_eq!(win_calls(&evs, &e), golden_calls, "{want}");
        if fini == 1 {
            assert_eq!(e.place(), ("rhea.wam", 1));
        }
    }
}

#[test]
fn the_place_test_and_the_dispatcher_against_the_original() {
    for want in rows("E ") {
        let name = want
            .split("name='")
            .nth(1)
            .unwrap()
            .split("': ")
            .next()
            .unwrap();
        let ran = want.contains("ran=1");
        assert_eq!(fireflies_place(name), ran, "{want}");
    }
    let f = rows("F ");
    assert_eq!(f.len(), 19);
    for want in f {
        let head = want.split(": ").next().unwrap();
        let mut it = head
            .split(' ')
            .skip(1)
            .map(|x| x.split('=').nth(1).unwrap());
        let (name, view, kind) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        let hooks = shamballa_click_hooks(
            name.as_bytes(),
            view.parse().unwrap(),
            kind.parse().unwrap(),
        );
        let got = format!(
            "{head}: hook={} return AL=0",
            u8::from(hooks == [FIREFLIES_CLICK])
        );
        assert!(hooks.is_empty() || hooks == [FIREFLIES_CLICK]);
        assert_eq!(got, want);
    }
}

/// Turns the camera onto zone `zone` of the current view, checked by a
/// pass.
fn aim(e: &mut Engine, zone: usize) {
    let view = e.place().1;
    let (yaw, pitch) = e.zone_aim(view, zone).unwrap();
    e.camera.enter(yaw + 0.01, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zone), "view {view}");
}

/// One G row: firefly `v` clicked (its zone aimed at), the move played to
/// its end.
fn g_row(e: &mut Engine, v: i32, t: f64) -> String {
    let zone = e
        .wam()
        .view(1)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.kind == 1 && z.value == v)
        .unwrap();
    aim(e, zone);
    // A turn still going on, too small to move the centre off the zone
    // before the click: the hook stops it.
    (e.look.yaw_speed, e.look.pitch_speed) = (1e-7, 1e-7);
    e.events.clear();
    e.click(t);
    e.pass(None);
    let click_evs = std::mem::take(&mut e.events);
    let speeds_after = speeds(e);
    let hook = click_evs
        .iter()
        .position(|x| {
            *x == Event::PuzzleRan {
                function: FIREFLIES_CLICK,
            }
        })
        .unwrap_or_else(|| panic!("the hook: {click_evs:?}"));
    let scan = click_evs
        .iter()
        .position(|x| matches!(x, Event::ClickScan(_)))
        .expect("a click scan");
    let depart = click_evs
        .iter()
        .position(|x| matches!(x, Event::DepartScan(_)))
        .expect("the departure scan");
    assert!(depart < hook && hook < scan);
    let sound = click_evs[hook..scan].contains(&Event::FireflySound);
    let zoom = e.zoom.pending() && e.zoom.target == 35.0;
    let moving = e.turning();
    let mut calls = Vec::new();
    if zoom {
        calls.push("AtlWarp_ZoomStart(100, 35)".to_string());
    }
    if sound {
        calls.push("RunOneCommand('(NewSound=300)')".into());
    }
    if moving {
        calls.push(format!("MoveSetup({v})"));
    }
    // The move played to its end (and, at a win, the film and Rhea's).
    let mut evs = Vec::new();
    for _ in 0..3000 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        evs.append(&mut e.events);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    let end = if v == 1 {
        assert!(evs.contains(&Event::Enter { view: 1 }));
        assert!(!evs.contains(&Event::PuzzleRan {
            function: FIREFLIES_MOVE_END
        }));
        "-".to_string()
    } else {
        let at = evs
            .iter()
            .position(|x| {
                *x == Event::PuzzleRan {
                    function: FIREFLIES_MOVE_END,
                }
            })
            .expect("the move end");
        let after = &evs[at..];
        assert!(fov_76(e) || e.place().0 == "rhea.wam");
        let w = win_calls(after, e);
        format!(
            "{} calls={SET_FOV}{}{w}",
            lines(after),
            if w.is_empty() { "" } else { "," }
        )
    };
    format!(
        "v={v}: OUT+hook {} calls={} | IN {} | END {end} | {} chapter_cur={} speeds={speeds_after}",
        lines(&click_evs[depart..hook]),
        calls.join(","),
        lines(&click_evs[scan..]),
        state(e),
        e.script_state().unwrap().chapter
    )
}

/// The golden's G row with the calls of its END cut to those the engine's
/// events show.
fn g_want(row: &str) -> String {
    let (before, end) = row.split_once(" | END ").unwrap();
    let (end, after) = end.split_once(" | ").unwrap();
    let end = match end.split_once(" calls=") {
        Some((l, calls)) => {
            let cut = cut_calls(calls);
            format!(
                "{l} calls={SET_FOV}{}{cut}",
                if cut.is_empty() { "" } else { "," }
            )
        }
        None => end.to_string(),
    };
    format!("{} | END {end} | {after}", before.trim_start())
}

#[test]
fn the_fireflies_clicked_in_the_game_against_the_original() {
    let Some(files) = game() else { return };
    let mut files = Some(files);
    let heads: Vec<(usize, &str)> = GOLDEN
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("G "))
        .collect();
    assert_eq!(heads.len(), 4);
    let all: Vec<&str> = GOLDEN.lines().collect();
    let mut t = 10.0;
    for (i, head) in heads {
        let clicks: Vec<i32> = head
            .rsplit("clicks ")
            .next()
            .unwrap()
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect();
        let want: Vec<&str> = all[i + 1..]
            .iter()
            .take_while(|l| l.starts_with("  "))
            .copied()
            .collect();
        let mut e = in_shamballa(
            files.take().unwrap_or_else(|| game().unwrap()),
            Fpu::default(),
        );
        set_order(&mut e, &order_of(head));
        if head.starts_with("G aborted") {
            // A section of the departure scan setting `ABORTDEPART` (the
            // golden's synthetic chapter: chapter 78 has none): the
            // dispatcher is not reached, the hook not run.
            assert_eq!(want.len(), 1);
            let v = clicks[0];
            let zone = e
                .wam()
                .view(1)
                .unwrap()
                .zones
                .iter()
                .position(|z| z.kind == 1 && z.value == v)
                .unwrap();
            aim(&mut e, zone);
            let text = b"/INIT\r\n/FININIT\r\n/000\r\n/con(DepartVue!0)\r\n/set(ABORTDEPART=1)\r\n/000\r\n";
            assert!(e.script_state_mut().unwrap().set_chapter_text(text));
            e.events.clear();
            t += 10.0;
            e.click(t);
            e.pass(None);
            let hook = e.events.contains(&Event::PuzzleRan {
                function: FIREFLIES_CLICK,
            });
            let sound = e.events.contains(&Event::FireflySound);
            // `ABORTDEPART` as the click handler leaves it: set by the
            // section's line 5 (the frame's step later in the pass clears
            // it and drops the click, `interp_gereframe`).
            let depart = e
                .events
                .iter()
                .position(|x| matches!(x, Event::DepartScan(_)))
                .unwrap();
            let aborted = e.events[depart..].contains(&Event::Set {
                chapter: 78,
                line: 5,
            });
            assert_eq!(var(&e, "ABORTDEPART"), 0);
            let got = format!(
                "  v={v}: ABORTDEPART={} hook={} {} calls={}",
                u8::from(aborted),
                u8::from(hook),
                state(&e),
                if sound || e.turning() { "?" } else { "-" }
            );
            assert_eq!(got, want[0]);
            continue;
        }
        assert_eq!(want.len(), clicks.len());
        for (v, want) in clicks.iter().zip(want) {
            t += 10.0;
            let got = g_row(&mut e, *v, t);
            assert_eq!(got, g_want(want), "{head}");
        }
        let won = var(&e, "Puzzle_Lucioles_Fini") == 1;
        assert_eq!(won, !head.starts_with("G lost"));
    }
}
