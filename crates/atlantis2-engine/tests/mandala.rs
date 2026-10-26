//! The Tibet mandala puzzle of view 10 against the original
//! (`tests/golden/mandala.txt`, its header there): the hook alone on each
//! zone, on wrong objects and names, twice, in whole placement orders
//! (rows A-D, through [`Engine::mandala_hook`]), then the zone click and
//! the click scan of the same frame on chapter 75 (rows E, through
//! [`Engine::click`] and a pass, the film a section starts ended by
//! [`Engine::end_video`]), and the dispatcher's choice of the hook by warp
//! name, view and zone type (rows F).
//!
//! The original then enters the boat's world (or Shamballa for the
//! crystal), which the port does not have: there the rows are compared up
//! to the place change (the chapter's lines up to its `changewam`, the
//! calls up to `AtlWam_Load`), and the refusal is checked (the world and
//! the view kept, the per-view reset run). Then: the save of the
//! puzzle's state, a played placement with the boat taken in its view,
//! and a refused save of another world.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped.

use std::path::PathBuf;

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::A2Files;
use atlantis2_engine::engine::{mandala_hook_applies, SaveError, MANDALAS};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/mandala.txt");

/// The variables the rows show, in their order.
const SHOWN: [&str; 7] = [
    "NbMandalaPosed",
    "OrdreMandala1",
    "OrdreMandala2",
    "OrdreMandala3",
    "OrdreMandala4",
    "OrdreMandala5",
    "OrdreMandala6",
];

/// The six flags chapter 75 sets, in the rows' order.
const POSE: [&str; 6] = [
    "poseirb", "posechb", "poseyub", "poseira", "posecha", "poseyua",
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

/// A new game, the opening film ended, then the first click (chapter 75)
/// in view 10.
fn chapter_75(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    enter(&mut e, 10);
    // On a named zone (no move): its scan loads chapter 75.
    aim(&mut e, "ZONEPoseCristal");
    e.click(1.0);
    e.pass(None);
    assert_eq!(e.script_state().unwrap().chapter, 75);
    assert_eq!(
        (e.loop_type, e.place()),
        (LoopType::Warp, ("tibet.wam", 10))
    );
    e.trace = true;
    e
}

/// Enters `view` as a move ends there, for the scripts too (`VUE`).
fn enter(e: &mut Engine, view: u32) {
    e.enter_at(view, 0.0, 0.0).unwrap();
    e.script_state_mut().unwrap().set_current_view(view as i32);
    e.reset_view();
}

fn names(e: &Engine) -> DefNames {
    e.script_state().unwrap().names.clone()
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().unwrap().var(name).unwrap()
}

fn set(e: &mut Engine, name: &str, v: i32) {
    assert!(e.script_state_mut().unwrap().set_var(name, v), "{name}");
}

/// `{S:I}` of a name.
fn tok(n: &DefNames, section: usize, name: &str) -> String {
    format!("{{{section}:{}}}", n.id(section, name.as_bytes()).unwrap())
}

/// The name behind `{S:I}`.
fn untok(n: &DefNames, t: &str) -> String {
    let (s, i) = t
        .trim_start_matches('{')
        .trim_end_matches('}')
        .split_once(':')
        .unwrap();
    let b = n.name(s.parse().unwrap(), i.parse().unwrap()).unwrap();
    b.iter().map(|&c| char::from(c)).collect()
}

/// A zone of a row: `{4:I}`, or `lower:` / `space:` / `cut7:` before it.
fn zone_of(n: &DefNames, spec: &str) -> String {
    match spec.split_once(':') {
        Some(("lower", t)) => untok(n, t).to_lowercase(),
        Some(("space", t)) => untok(n, t) + " ",
        Some(("cut7", t)) => untok(n, t).chars().take(7).collect(),
        _ => untok(n, spec),
    }
}

/// The hand of a row: `0` or `{1:I}`.
fn hand_of(spec: &str) -> i32 {
    match spec {
        "0" => 0,
        t => t
            .trim_start_matches("{1:")
            .trim_end_matches('}')
            .parse()
            .unwrap(),
    }
}

fn state(e: &Engine, n: &DefNames) -> String {
    let v: Vec<String> = SHOWN
        .iter()
        .map(|k| format!("{}={}", tok(n, defnames::VARIABLES, k), var(e, k)))
        .collect();
    v.join(" ")
}

fn reset(e: &mut Engine) {
    for k in SHOWN {
        set(e, k, 0);
    }
}

/// The hook alone on a zone named `zone`, `hand` in hand: the calls row.
fn hook(e: &mut Engine, zone: &str, hand: i32) -> String {
    set(e, "ObjetEnMain", hand);
    e.events.clear();
    e.mandala_hook(Some(zone));
    let sounds: Vec<&str> = e
        .events
        .iter()
        .filter_map(|ev| match ev {
            // `RunOneCommand("(NewSound=400)")`: `sound400.apc` on voice 2.
            Event::Sound(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    match sounds.as_slice() {
        [] => "-".into(),
        ["sound400.apc"] => "RunOneCommand('(NewSound=400)')".into(),
        other => format!("{other:?}"),
    }
}

/// The zone (index in view 10) whose name is `name`, case ignored (the
/// place spells the crystal's zone in capitals, `defnames.con` not).
fn zone_index(e: &Engine, name: &str) -> usize {
    e.wam()
        .view(10)
        .unwrap()
        .zones
        .iter()
        .position(|z| {
            z.name
                .as_ref()
                .is_some_and(|z| z.eq_ignore_ascii_case(name))
        })
        .unwrap_or_else(|| panic!("no zone {name} in view 10"))
}

/// Turns the camera to zone `name` of view 10, checked by a pass.
fn aim(e: &mut Engine, name: &str) {
    let zi = zone_index(e, name);
    let (yaw, pitch) = e.zone_aim(10, zi).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(zi), "{name}");
}

/// The state chapter 75's sections see in the golden's E rows: variables
/// and flags at 0 (but the chapter, view, world and limits the engine
/// keeps), then those given.
fn e_state(e: &mut Engine, pre: &[(&str, i32)], hand: i32) {
    let s = e.script_state_mut().unwrap();
    let keep: Vec<usize> = [
        "CHAPTER",
        "VUE",
        "MONDE",
        "LIMITEANGLEWARPX",
        "LIMITEANGLEWARPY",
        "NEARW77",
    ]
    .iter()
    .map(|k| s.names.id(defnames::VARIABLES, k.as_bytes()).unwrap())
    .collect();
    for (i, v) in s.vars.iter_mut().enumerate() {
        if !keep.contains(&i) {
            *v = 0;
        }
    }
    s.flags.iter_mut().for_each(|f| *f = 0);
    s.bag.clear();
    for (k, v) in pre {
        set(e, k, *v);
    }
    set(e, "ObjetEnMain", hand);
}

/// One E row: the click, the film ended when one starts; the produced
/// row, compared with the golden's up to the place change.
fn e_row(e: &mut Engine, n: &DefNames, want: &str, t: &mut f64) {
    let (head, rest) = want.split_once(": ").unwrap();
    let mut f = head.split(' ');
    assert_eq!(f.next(), Some("E"));
    let zone = untok(n, f.next().unwrap());
    let hand_spec = f.next().unwrap().trim_start_matches("hand=");
    let hand = hand_of(hand_spec.trim_end_matches(','));
    let label: Vec<&str> = f.collect();
    let every: Vec<(&str, i32)> = POSE.iter().map(|p| (*p, 1)).collect();
    let pre: Vec<(&str, i32)> = match label.join(" ").as_str() {
        "" => Vec::new(),
        // `{3:188}=3 before`
        l if l.ends_with(" before") => vec![("NbMandalaPosed", 3)],
        "all posed" => {
            let mut v = every.clone();
            v.push(("NbMandalaPosed", 6));
            v
        }
        // one flag at 0: the five others laid
        l => {
            let off = untok(n, l.split('=').next().unwrap());
            let mut v: Vec<(&str, i32)> = every
                .iter()
                .map(|&(p, _)| (p, i32::from(!p.eq_ignore_ascii_case(&off))))
                .collect();
            v.push(("NbMandalaPosed", 5));
            v
        }
    };
    aim(e, &zone);
    e_state(e, &pre, hand);
    e.events.clear();
    *t += 1.0;
    e.click(*t);
    e.pass(None);
    let mut resumed = 0;
    if e.loop_type == LoopType::Video {
        e.end_video();
        e.pass(None);
        resumed = 1;
    }
    let ev = e.events.clone();
    let at = |p: &dyn Fn(&Event) -> bool| ev.iter().position(p);
    let depart = at(&|x| matches!(x, Event::DepartScan(_)));
    let hooked = at(&|x| matches!(x, Event::PuzzleHook { .. }));
    let scan = at(&|x| matches!(x, Event::ClickScan(_))).expect("a click scan");
    let out_lines: Vec<String> = match depart {
        Some(d) => ev[d..hooked.unwrap_or(scan)]
            .iter()
            .filter_map(|x| match x {
                Event::Set { line, .. } => Some(line.to_string()),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    };
    let cz = tok(n, defnames::VARIABLES, "ClicZone");
    let hook_cz = match hooked.map(|h| &ev[h]) {
        Some(Event::PuzzleHook { cliczone, .. }) => cliczone.to_string(),
        _ => "?".into(),
    };
    let hook_calls = match hooked {
        Some(h)
            if ev[h..scan]
                .iter()
                .any(|x| *x == Event::Sound("sound400.apc".into())) =>
        {
            "RunOneCommand('(NewSound=400)')".to_string()
        }
        _ => "-".to_string(),
    };
    let after_cz = match &ev[scan] {
        Event::ClickScan(v) => v.cliczone,
        _ => unreachable!(),
    };
    let refused = ev
        .iter()
        .position(|x| matches!(x, Event::WorldNotPorted(_)));
    let in_lines: Vec<String> = ev[scan..refused.unwrap_or(ev.len())]
        .iter()
        .filter_map(|x| match x {
            Event::Set { chapter: 75, line } => Some(line.to_string()),
            _ => None,
        })
        .collect();
    let lines = |l: &[String]| {
        if l.is_empty() {
            "-".to_string()
        } else {
            l.join(",")
        }
    };
    // The calls the engine's events show (the world, the film, the
    // place), up to the place's load, refused or not.
    let mut calls: Vec<&str> = ev[scan..refused.map_or(ev.len(), |r| r + 1)]
        .iter()
        .filter_map(|x| match x {
            Event::Monde(_) => Some("Interp_SetMonde"),
            Event::VideoStart {
                frames: Some(_), ..
            } => Some("AtlHnm_start"),
            Event::WorldNotPorted(_) | Event::Wam(_) => Some("AtlWam_Load"),
            _ => None,
        })
        .collect();
    if let Some(l) = calls.iter().position(|&c| c == "AtlWam_Load") {
        calls.truncate(l + 1);
    }
    let pose: Vec<String> = POSE
        .iter()
        .map(|p| format!("{}={}", tok(n, defnames::VARIABLES, p), var(e, p)))
        .collect();
    let produced = format!(
        "{head}: OUT ran={} lines {} | hook {cz}={hook_cz} {hook_calls} | after {cz}={after_cz} \
         | IN lines {} resumes={resumed} | {} {}={} pose: {} | calls={}",
        u8::from(depart.is_some()),
        lines(&out_lines),
        lines(&in_lines),
        state(e, n),
        tok(n, defnames::VARIABLES, "ObjetEnMain"),
        var(e, "ObjetEnMain"),
        pose.join(","),
        calls.join(",")
    );
    // The golden's row up to the place change: its lines up to the
    // `changewam` the engine refused, its calls up to `AtlWam_Load` and
    // only those the engine's events show.
    let mut want_cut = want.to_string();
    if refused.is_some() {
        let golden_lines = rest
            .split("IN lines ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        let kept: Vec<&str> = golden_lines.split(',').take(in_lines.len()).collect();
        assert!(kept.len() < golden_lines.split(',').count(), "{want}");
        want_cut = want_cut.replace(golden_lines, &kept.join(","));
    }
    let golden_calls = want.rsplit("calls=").next().unwrap();
    let mut cut: Vec<&str> = Vec::new();
    for c in golden_calls.split(',').filter(|c| !c.is_empty()) {
        if matches!(c, "Interp_SetMonde" | "AtlHnm_start" | "AtlWam_Load") {
            cut.push(c);
        }
        if c == "AtlWam_Load" {
            break;
        }
    }
    want_cut = format!(
        "{}{}",
        want_cut.strip_suffix(golden_calls).unwrap(),
        cut.join(",")
    );
    assert_eq!(produced, want_cut, "\nthe original's row: {want}");

    // What the port does instead of the place change: the world back to
    // Tibet's, the per-view reset, the game in view 10 of chapter 75.
    if let Some(r) = refused {
        assert!(matches!(&ev[r], Event::WorldNotPorted(w) if !w.eq_ignore_ascii_case("tibet.wam")));
        assert!(ev[r..].contains(&Event::Monde(5)));
        assert!(!ev[r..].iter().any(|x| matches!(x, Event::Set { .. })));
        assert_eq!(e.place(), ("tibet.wam", 10));
        assert_eq!(e.loop_type, LoopType::Warp);
        let s = e.script_state().unwrap();
        assert_eq!(
            (s.chapter, s.var("MONDE"), s.var("VUE")),
            (75, Some(5), Some(10))
        );
        assert!(!s.travel_refused && s.fatal.is_none() && !s.resume_pending);
        assert_eq!((e.hand, e.vars.objet_en_main), (None, 0));
    }
    // Ireland's boat: the trip made, the section's last lines (the place,
    // its view, chapter 2).
    if ev.contains(&Event::Wam("bigile1.wam".into())) {
        assert!(refused.is_none());
        assert_eq!(e.place(), ("bigile1.wam", 5));
        let s = e.script_state().unwrap();
        assert_eq!((s.chapter, s.var("MONDE")), (2, Some(1)));
        assert!(s.fatal.is_none());
    }
    // Yucatán's boat (first and second trips): its view 10, chapter 25.
    if ev.contains(&Event::Wam("yucatan.wam".into())) {
        assert!(refused.is_none());
        assert_eq!(e.place(), ("yucatan.wam", 10));
        let s = e.script_state().unwrap();
        assert_eq!((s.chapter, s.var("MONDE")), (25, Some(2)));
        assert!(s.fatal.is_none());
    }
}

#[test]
fn mandalas_against_the_original() {
    let Some(files) = game() else { return };
    let mut e = chapter_75(files);
    let n = names(&e);
    let rows: Vec<&str> = GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();

    // The ids: the hook's objects and zones in its order, then the
    // crystal; the variables.
    let ids = |section: usize, list: &[&str]| -> String {
        let v: Vec<String> = list
            .iter()
            .map(|k| n.id(section, k.as_bytes()).unwrap().to_string())
            .collect();
        v.join(" ")
    };
    let mut objects: Vec<&str> = MANDALAS.iter().map(|m| m.0).collect();
    objects.push("CRISTAL_SHAMAN");
    let mut zones: Vec<&str> = MANDALAS.iter().map(|m| m.1).collect();
    zones.push("ZONEPoseCristal");
    let mut vars: Vec<&str> = SHOWN.to_vec();
    vars.extend(POSE);
    assert_eq!(
        rows[0],
        format!("ids objects {}", ids(defnames::OBJETS, &objects))
    );
    assert_eq!(
        rows[1],
        format!("ids zones {}", ids(defnames::ZONES_CLIC, &zones))
    );
    assert_eq!(
        rows[2],
        format!("ids variables {}", ids(defnames::VARIABLES, &vars))
    );
    // The hook's order variables are its rows'.
    for (m, v) in MANDALAS.iter().zip(&SHOWN[1..]) {
        assert_eq!(m.2, *v);
    }

    let object_of = |zone: &str| {
        let m = MANDALAS.iter().find(|m| m.1 == zone).unwrap();
        n.id(defnames::OBJETS, m.0.as_bytes()).unwrap() as i32
    };
    let mut t = 10.0;
    let mut checked = 0;
    for want in &rows[3..] {
        let (head, _) = want.split_once(": ").unwrap_or((want, ""));
        let f: Vec<&str> = head.split(' ').collect();
        match f[0] {
            "A" | "B" | "C" => {
                if f[0] != "C" || want.contains(&format!("{}=1 ", tok(&n, 3, SHOWN[0]))) {
                    reset(&mut e);
                }
                let zone = zone_of(&n, f[1]);
                let hand = hand_of(f[2].trim_start_matches("hand="));
                let calls = hook(&mut e, &zone, hand);
                let mut got = format!("{head}: {}", state(&e, &n));
                if f[0] != "C" {
                    got = format!("{got} calls={calls}");
                }
                assert_eq!(&got, want);
            }
            "D" if f[1] == "order" => {
                reset(&mut e);
                for z in f[2].split(',') {
                    let zone = untok(&n, z);
                    hook(&mut e, &zone, object_of(&zone));
                }
                assert_eq!(&format!("{head}: {}", state(&e, &n)), want);
            }
            "D" if f[1] == "log" => {
                // `D log n1 .. n6`: the solution log the exit hook writes
                // (FUN_00495650) after the order above.
                let mut want_log = vec!["--- Solution Mandalas ---".to_string()];
                for (k, v) in f[2..].iter().enumerate() {
                    want_log.push(format!("Mandala {} : numero {v}", k + 1));
                }
                want_log.push(String::new());
                assert_eq!(e.mandala_solution_log(), want_log, "{want}");
            }
            "D" => {
                // `D last {4:Z} ({3:V}=6)`: the others in the hook's
                // order, then Z.
                let last = untok(&n, f[2]);
                reset(&mut e);
                for m in MANDALAS.iter().filter(|m| m.1 != last) {
                    hook(&mut e, m.1, object_of(m.1));
                }
                hook(&mut e, &last, object_of(&last));
                let v = f[3].trim_start_matches('(').split('=').next().unwrap();
                assert_eq!(var(&e, &untok(&n, v)), 6, "{want}");
                assert_eq!(var(&e, "NbMandalaPosed"), 6);
            }
            "E" => {
                // Each click on a fresh game: a section's showobjet
                // puts the laid mandala's sprite under the centre.
                e = chapter_75(game().unwrap());
                e_row(&mut e, &n, want, &mut t);
            }
            // The dispatcher's rows: `the_hook_only_in_view_10`.
            "F" => {}
            other => panic!("unknown row {other}"),
        }
        checked += 1;
    }
    assert_eq!(checked, rows.len() - 3);
}

/// The dispatcher (@ 0x0049b910) sends a zone click to the hook only in a
/// view whose warp name starts with `TI` (any case), numbered 10, and only
/// for zone types 3, 4 and 50: the golden's F rows, every name, number and
/// type there, against the port's test of the three. Then in the game: a
/// laid boat reaches the hook, a click on view 10's move zone does not
/// (the move starts), and no other Tibet view has a zone of those types
/// (so the view number is tested by the rows only).
#[test]
fn the_hook_only_in_view_10() {
    let rows: Vec<&str> = GOLDEN.lines().filter(|l| l.starts_with("F ")).collect();
    assert_eq!(rows.len(), 47);
    let (mut hooked, mut not) = (0, 0);
    for row in &rows {
        let (head, tail) = row.split_once(": ").unwrap();
        let (mut name, mut view, mut kind) = ("", 0u32, 0i32);
        for kv in head[2..].split(' ') {
            match kv.split_once('=').unwrap() {
                ("name", v) => name = v,
                ("view", v) => view = v.parse().unwrap(),
                ("type", v) => kind = v.parse().unwrap(),
                other => panic!("{other:?}"),
            }
        }
        let applies = mandala_hook_applies(name.as_bytes(), view, kind);
        // The port's dispatcher never consumes a Tibet click (AL = 0) and,
        // the hand empty, the hook calls nothing.
        assert_eq!(
            tail,
            format!("hook={} return AL=0 calls=-", u8::from(applies)),
            "{row}"
        );
        if applies {
            hooked += 1;
        } else {
            not += 1;
        }
    }
    assert_eq!((hooked, not), (6, 41));

    let Some(files) = game() else { return };
    let mut e = chapter_75(files);
    // Yucatán's boat (laid: the trip waits for the film).
    let boat = names(&e)
        .id(defnames::OBJETS, b"MANDALA_BATEAU_YU")
        .unwrap() as i32;
    // View 10, ZONE_YUB with its boat: laid.
    aim(&mut e, "ZONE_YUB");
    set(&mut e, "ObjetEnMain", boat);
    e.events.clear();
    e.click(5.0);
    e.pass(None);
    assert!(e
        .events
        .iter()
        .any(|x| matches!(x, Event::PuzzleHook { cliczone: 0, .. })));
    assert_eq!(var(&e, "OrdreMandala3"), 1);
    e.end_video();
    for _ in 0..50 {
        if e.loop_type == LoopType::Video {
            e.end_video();
        }
        e.tick();
        e.pass(None);
    }
    // The trip made; back to Tibet's view 10 by the scripts' commands.
    assert_eq!(e.place(), ("yucatan.wam", 10));
    for c in [
        &b"(monde=5)"[..],
        b"(changewam=tibet.wam)",
        b"(vue=10)",
        b"(chapter=75)",
    ] {
        e.run_command(c);
    }
    e.pass(None);
    assert_eq!(e.place(), ("tibet.wam", 10));
    // View 10's move zone (type 1, to view 9): the departure, no hook.
    set(&mut e, "ObjetEnMain", 0);
    let zones = e.wam().view(10).unwrap().zones.clone();
    let mv = zones.iter().position(|z| z.kind == 1).unwrap();
    assert_eq!(zones[mv].value, 9);
    let (yaw, pitch) = e.zone_aim(10, mv).unwrap();
    e.camera.enter(yaw, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(mv));
    e.events.clear();
    e.click(20.0);
    e.pass(None);
    assert!(e.events.iter().any(|x| matches!(x, Event::DepartScan(_))));
    assert!(!e
        .events
        .iter()
        .any(|x| matches!(x, Event::PuzzleHook { .. })));
    // The other views: move zones and view 4's type 1000 only.
    for view in 2..10 {
        let zones = &e.wam().view(view).unwrap().zones;
        assert!(!zones.is_empty());
        assert!(
            zones.iter().all(|z| matches!(z.kind, 1 | 1000)),
            "view {view}"
        );
    }
}

/// A boat taken in its view, put in hand from the bag and laid on its
/// zone: chapter 75's section, the film, the trip to Yucatán; the
/// puzzle's state survives a save there and a load; back in Tibet, the
/// laid boat is shown and a second click on it makes the trip again.
#[test]
fn a_boat_laid_saved_and_loaded() {
    let Some(files) = game() else { return };
    let mut e = chapter_75(files);
    let n = names(&e);
    let boat = n.id(defnames::OBJETS, b"MANDALA_BATEAU_YU").unwrap() as i32;
    let laid = n.id(defnames::OBJETS, b"MANDALA_YU1").unwrap() as i32;
    // The take in view 5 (the boats of views 5, 8, 9: Yucatán, Ireland,
    // China).
    enter(&mut e, 5);
    let slot = e
        .anim
        .slots
        .iter()
        .position(|s| s.id == boat && s.kind == 2 && s.click == 1)
        .expect("the boat's slot in view 5");
    e.take_slot(slot);
    e.pass(None);
    assert!(e.bag_entries().iter().any(|b| b.id == boat));
    // To view 10, the boat in hand (`objetenmain=`, as the bag bar's
    // pick sets it).
    enter(&mut e, 10);
    aim(&mut e, "ZONE_YUB");
    e.run_command(format!("(objetenmain={})", untok(&n, &format!("{{1:{boat}}}"))).as_bytes());
    assert_eq!(e.vars.objet_en_main, boat);
    e.events.clear();
    e.click(20.0);
    e.pass(None);
    assert!(e.events.contains(&Event::Sound("sound400.apc".into())));
    assert_eq!(e.loop_type, LoopType::Video);
    assert!(e.events.contains(&Event::Monde(2)));
    // The trip: Yucatán's view 10 and chapter 25 on disc 2; the boat has
    // left the bag, the laid mandala's row of the show / hide table is
    // set.
    for _ in 0..50 {
        if e.loop_type == LoopType::Video {
            e.end_video();
        }
        e.tick();
        e.pass(None);
    }
    assert!(!e
        .events
        .iter()
        .any(|x| matches!(x, Event::WorldNotPorted(_))));
    assert_eq!(e.place(), ("yucatan.wam", 10));
    assert_eq!(e.files().current_disc(), 2);
    assert_eq!(
        (var(&e, "MONDE"), e.script_state().unwrap().chapter),
        (2, 25)
    );
    assert_eq!(
        (
            var(&e, "poseyub"),
            var(&e, "NbMandalaPosed"),
            var(&e, "OrdreMandala3")
        ),
        (1, 1, 1)
    );
    assert!(!e.bag_entries().iter().any(|b| b.id == boat));
    let shown = |e: &Engine| {
        e.script_state()
            .unwrap()
            .shown
            .iter()
            .any(|r| r.id == laid && r.shown)
    };
    assert!(shown(&e));

    // Saved and loaded in a new game.
    let saved = e.save_data();
    let mut f = chapter_75(game().unwrap());
    f.load_data(&saved).unwrap();
    for k in SHOWN.iter().chain(POSE.iter()) {
        assert_eq!(var(&f, k), var(&e, k), "{k}");
    }
    assert!(shown(&f));
    assert_eq!(f.place(), ("yucatan.wam", 10));

    // Back in Tibet's view 10 (as the brazier's way back makes it, with
    // view 10 for view 7): the laid boat shown; clicked with an empty hand,
    // the trip again (chapter 75's second-trip section); the hook counts
    // nothing.
    for c in [
        &b"(monde=5)"[..],
        b"(changewam=tibet.wam)",
        b"(vue=10)",
        b"(chapter=75)",
    ] {
        f.run_command(c);
    }
    f.pass(None);
    assert_eq!(f.place(), ("tibet.wam", 10));
    assert!(f.anim.slots.iter().any(|s| s.id == laid && s.active));
    f.trace = true;
    aim(&mut f, "ZONE_YUB");
    f.events.clear();
    f.click(30.0);
    for _ in 0..50 {
        if f.loop_type == LoopType::Video {
            f.end_video();
        }
        f.tick();
        f.pass(None);
    }
    assert!(!f
        .events
        .iter()
        .any(|x| matches!(x, Event::WorldNotPorted(_))));
    assert_eq!(var(&f, "NbMandalaPosed"), 1);
    assert_eq!(f.place(), ("yucatan.wam", 10));
    assert_eq!(var(&f, "MONDE"), 2);

    // A save in a place the port does not have (chapter 1's `demo.wam`,
    // whose panoramas are on no disc) is refused before anything changes.
    let mut other = saved.clone();
    other.set_wam(b"demo.wam");
    let before = f.save_data();
    assert!(matches!(f.load_data(&other), Err(SaveError::NotPorted(w)) if w == "demo.wam"));
    assert_eq!(f.save_data().entry(6), before.entry(6));
    assert_eq!(f.place(), ("yucatan.wam", 10));
}
