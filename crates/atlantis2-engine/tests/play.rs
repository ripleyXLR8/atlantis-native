//! A new game run by the chapter scripts, headless, from the start-up to
//! the shaman's first dialog and on to view 10: the opening film, the
//! music, the first click that sends the game to chapter 75, the walk from
//! view 2 to view 10 with the arrival scans, the dialog the arrival in
//! view 7 plays and the section that goes on after it.
//!
//! The scan results are compared with the goldens of the original's
//! interpreter under emulation (`tests/golden/scripts_con_tibet_scan.txt`:
//! the lines a scan of chapter 75 runs, among them the arrivals in views 7
//! and 10, which `tests/golden/moves_tibet.txt` runs too) and with the
//! dialog golden (`tests/golden/dialog_script_ti.txt`: the lines of dialog
//! 360).
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped.

use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::anim::MsvcRand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::engine::TICK;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const TIBET_SCAN: &str = include_str!("golden/scripts_con_tibet_scan.txt");
const DIALOGS: &str = include_str!("golden/dialog_script_ti.txt");

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

/// The lines of a case of `scripts_con_tibet_scan.txt`.
fn scan_case(name: &str) -> Vec<usize> {
    let row = TIBET_SCAN
        .lines()
        .find(|l| l.starts_with(&format!("{name} ")))
        .unwrap_or_else(|| panic!("no {name}"));
    let lines = row.split(": lines ").nth(1).unwrap();
    let lines = lines.split_whitespace().next().unwrap();
    if lines == "-" {
        return Vec::new();
    }
    lines.split(',').map(|n| n.parse().unwrap()).collect()
}

/// The ids of dialog `n`'s lines in `dialog_script_ti.txt` (from the
/// voice of each `rep` row).
fn dialog_lines(n: u32) -> Vec<String> {
    DIALOGS
        .lines()
        .filter(|l| l.starts_with(&format!("rep {n} ")))
        .filter_map(|l| {
            let i = l.find(".apc")?;
            Some(l[i - 6..i].to_string())
        })
        .collect()
}

/// The engine at 59.94 passes a second ([`PASS`]), the clock running with them.
struct Run {
    e: Engine,
    pass: u64,
    log: Vec<String>,
}

impl Run {
    fn now(&self) -> f64 {
        self.pass as f64 * PASS
    }

    /// One pass; the events it produced.
    fn step(&mut self) -> Vec<Event> {
        let t = self.now();
        self.e.set_clock(t);
        self.e.pass(None);
        self.pass += 1;
        let evs: Vec<Event> = std::mem::take(&mut self.e.events);
        for ev in &evs {
            if !matches!(ev, Event::SpwLoad { .. } | Event::VideoFrame(_)) {
                self.log.push(format!("{:8.3} {ev:?}", t));
            }
        }
        evs
    }

    /// Passes until `until` holds on an event (at most `max` passes); all
    /// the events seen.
    fn until(&mut self, max: u64, until: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..max {
            let evs = self.step();
            let hit = evs.iter().any(&until);
            all.extend(evs);
            if hit {
                return all;
            }
        }
        panic!(
            "not reached in {max} passes; last events:\n{}",
            self.log[self.log.len().saturating_sub(30)..].join("\n")
        );
    }

    fn wait(&mut self, passes: u64) -> Vec<Event> {
        (0..passes).flat_map(|_| self.step()).collect()
    }

    fn click(&mut self) {
        let t = self.now();
        self.e.click(t);
    }

    /// Looks at zone `zone` of the current view and clicks; then runs to
    /// the arrival in `to` (a transition video clicked away).
    fn walk(&mut self, zone: usize, to: u32) -> Vec<Event> {
        // Clicks 0.25 s apart: no double click (which would skip the
        // videos).
        self.wait(15);
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        self.click();
        let mut all = Vec::new();
        for _ in 0..600 {
            let evs = self.step();
            if evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            }) {
                all.extend(self.wait(15));
                self.click();
            }
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if arrived {
                assert_eq!(self.e.place().1, to);
                return all;
            }
        }
        panic!("no arrival in view {to}:\n{}", self.log.join("\n"));
    }
}

fn set_lines(evs: &[Event], chapter: i32) -> Vec<usize> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Set { chapter: c, line } if *c == chapter => Some(*line),
            _ => None,
        })
        .collect()
}

#[test]
fn a_new_game_plays_to_the_first_dialog() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.anim.rand = MsvcRand(1000);
    e.trace = true;
    // Start-up, then the menu's new game: chapter 0's /INIT, up to the
    // film it starts (its sound track on voice 2).
    e.start_new_game().unwrap();
    let boot: Vec<Event> = e
        .events
        .drain(..)
        .filter(|e| !matches!(e, Event::SpwLoad { .. }))
        .collect();
    // The lines of the /INIT up to the film (chapter 0's lines 2..24; the
    // engine logs a call's lines at its end).
    assert_eq!(set_lines(&boot, 0), (2..=24).collect::<Vec<_>>());
    let frames = boot.iter().find_map(|e| match e {
        Event::VideoStart { frames, .. } => *frames,
        _ => None,
    });
    assert!(frames.is_some_and(|f| f > 15));
    let calls: Vec<Event> = boot
        .into_iter()
        .filter(|e| !matches!(e, Event::Set { .. }))
        .collect();
    assert_eq!(
        calls,
        [
            Event::Monde(5),
            Event::Wam("tibet.wam".into()),
            Event::Enter { view: 2 },
            Event::VideoStart {
                name: "cine069.hnm".into(),
                frames,
            },
            Event::Sound("cine069.apc".into()),
            Event::VideoFrame(0),
        ]
    );
    let mut r = Run {
        e,
        pass: 0,
        log: Vec::new(),
    };
    assert!(r.e.voices().is_playing(2));
    let s = r.e.script_state().unwrap();
    assert_eq!(
        (s.chapter, s.var("MONDE"), s.var("VUE"), s.var("TESTTIME1")),
        (0, Some(5), Some(2), Some(1))
    );
    assert!(s.resume_pending, "the film interrupts the /INIT");
    assert_eq!(r.e.place(), ("tibet.wam", 2));
    assert_eq!(r.e.loop_type, LoopType::Video);
    let (yaw, pitch) = r.e.angles_deg();
    assert!(
        (yaw + 90.0).abs() < 0.01 && pitch.abs() < 0.01,
        "{yaw} {pitch}"
    );

    // The film moves a frame a 15 Hz tick (every 4th pass: ticks after
    // passes 1, 5, 9, each frame shown by the next pass); a click ends it,
    // and the rest of the /INIT starts music 30 from its beginning.
    let evs = r.until(12, |e| matches!(e, Event::VideoFrame(3)));
    assert!(evs.iter().any(|e| matches!(e, Event::VideoFrame(1))));
    r.click();
    let evs = r.until(10, |e| *e == Event::VideoEnd);
    assert_eq!(set_lines(&evs, 0).len(), 1, "the /INIT's last line");
    let evs = r.until(5, |e| matches!(e, Event::Music { .. }));
    assert!(evs.contains(&Event::Music { music: 30, mark: 0 }));
    assert_eq!(r.e.loop_type, LoopType::Warp);
    // The first warp draw after the film asks for 100 %: a fade of no
    // length, then "playing".
    r.step();
    r.step();
    assert_eq!((r.e.music.state, r.e.music.volume), (4, 100));

    // A click on the first view (the no-move case of the original): the
    // departure scan runs chapter 0's first section, which loads chapter
    // 75 and runs its /INIT (its first line twice, as the original).
    r.wait(15);
    r.click();
    let evs = r.until(2, |e| matches!(e, Event::ClickScan(_)));
    assert_eq!(set_lines(&evs, 75), [2, 2, 3]);
    assert_eq!(r.e.script_state().unwrap().chapter, 75);
    assert_eq!(r.e.place(), ("tibet.wam", 2));

    // The walk to view 7. Moves run no line until there (the sweep of the
    // moves golden: no arrival section before view 7).
    for (zone, to) in [(0, 3), (1, 4), (0, 5), (1, 6)] {
        let evs = r.walk(zone, to);
        assert!(set_lines(&evs, 75).is_empty(), "move to {to}");
    }
    // View 7: the arrival section plays dialog 360 and waits (the
    // golden's case 1 runs line 14, then line 15 once the dialog ends).
    let want = scan_case("case1");
    assert_eq!(want, [14, 15]);
    let evs = r.walk(0, 7);
    let mut lines = set_lines(&evs, 75);
    lines.extend(set_lines(&r.step(), 75));
    assert_eq!(lines, want[..1]);
    assert!(evs.contains(&Event::DialogStart {
        number: 360,
        started: true
    }));
    assert_eq!(r.e.loop_type, LoopType::Dialog);
    assert!(r.e.script_state().unwrap().resume_pending);

    // The dialog: its lines as the original's script gives them, the music
    // ducked to 16 %, then back up; the section goes on with line 15.
    // The shaman's place animation is hidden while he speaks (his
    // `[IdText]` names), shown while the hero does.
    let shaman =
        r.e.script_state()
            .unwrap()
            .names
            .id(atlantis2_data::defnames::PERSONNAGES, b"SHAMAN")
            .unwrap() as i32;
    let shaman_shown = |e: &Engine| e.anim.slots.iter().any(|s| s.id == shaman && s.active);
    assert!(shaman_shown(&r.e));
    let mut min_music = 100;
    let mut seen = Vec::new();
    let mut evs = Vec::new();
    for _ in 0..60 * 60 {
        let step = r.step();
        min_music = min_music.min(r.e.music.volume);
        for ev in &step {
            if let Event::DialogLine(id) = ev {
                seen.push(id.clone());
                assert_eq!(shaman_shown(&r.e), id == "360_02", "{id}");
            }
        }
        let done = step.contains(&Event::DialogEnd);
        evs.extend(step);
        if done {
            break;
        }
    }
    assert!(evs.contains(&Event::DialogEnd), "{}", r.log.join("\n"));
    assert_eq!(seen, dialog_lines(360));
    assert_eq!(min_music, 16);
    assert_eq!(set_lines(&evs, 75), want[1..]);
    let s = r.e.script_state().unwrap();
    assert_eq!(
        (s.var("flagtibet0"), s.var("SUJET"), s.resume_pending),
        (Some(1), Some(-2), false)
    );
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert!(shaman_shown(&r.e));
    for _ in 0..120 {
        r.step();
    }
    assert_eq!(r.e.music.volume, 100);

    // Views 8, 9 and 10: the arrival in view 10 runs line 294 (the
    // golden's case 3).
    assert_eq!(scan_case("case3"), [294]);
    r.walk(1, 8);
    r.walk(1, 9);
    let evs = r.walk(1, 10);
    let mut lines = set_lines(&evs, 75);
    lines.extend(set_lines(&r.step(), 75));
    assert_eq!(lines, [294]);
    assert_eq!(r.e.script_state().unwrap().flags[202], 1);
    assert!(r.e.script_state().unwrap().fatal.is_none());
    eprintln!("{}", r.log.join("\n"));
}

/// The 15 Hz task as the scheduler FUN_00584a00 runs it, against the
/// original's scheduler under emulation (`tests/golden/scheduler.txt`: the
/// game's queue, frame task and 15 Hz timer, one FUN_00584a00 call a pass
/// at the times of a 60 Hz, 72, 75 and 85 Hz screen, of an unpaced loop at
/// 500 passes a second and of a stall, under 0x0e7f and 0x027f): the task
/// runs in the same passes, after the pass's frame (the golden's `F`
/// before `T`), and the port's own pass period, a 60 Hz VGA frame, is the
/// golden's `vga60`: every 4th pass.
#[test]
fn the_15_hz_task_as_the_scheduler_runs_it() {
    const GOLDEN: &str = include_str!("golden/scheduler.txt");
    if game().is_none() {
        return;
    }
    assert!(TICK > 1.0 / 15.0 && TICK as f32 == 1.0f32 / 15.0f32);
    let mut scenarios = 0;
    let mut lines = GOLDEN.lines().filter(|l| !l.starts_with('#')).peekable();
    while let Some(head) = lines.next() {
        let rest = head.strip_prefix("scenario ").expect(head);
        let (name, cw) = rest.split_once(" cw ").unwrap();
        let fpu = match cw {
            "0x0e7f" => Fpu::Chop,
            "0x027f" => Fpu::Double,
            _ => panic!("{cw}"),
        };
        // A new game: the tasks started at time 0.
        let mut e = Engine::boot(game().unwrap(), Variant::default(), fpu, 640, 480).unwrap();
        let mut runs = Vec::new();
        while let Some(row) = lines.next_if(|l| !l.starts_with("summary ")) {
            let (pass, rest) = row.split_once(' ').unwrap();
            let (_, rest) = rest.split_once(' ').unwrap();
            let (now, tasks) = rest.split_once(": ").unwrap();
            let now = f64::from_bits(u64::from_str_radix(now, 16).unwrap());
            e.camera.fpu = fpu;
            let before = e.ticks;
            e.set_clock(now);
            // The frame runs first: no tick yet.
            assert_eq!(e.ticks, before, "{name} {cw} pass {pass}");
            e.pass(None);
            e.camera.fpu = fpu;
            let ran = e.ticks.wrapping_sub(before);
            assert_eq!(
                ran,
                u32::from(tasks == "FT"),
                "{name} {cw} pass {pass}: {tasks}"
            );
            assert!(tasks == "F" || tasks == "FT", "{tasks}");
            if ran == 1 {
                runs.push(pass.parse::<u32>().unwrap());
            }
        }
        let summary = lines.next().unwrap();
        let want: Vec<u32> = summary
            .split_once(": ")
            .unwrap()
            .1
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(runs, want, "{summary}");
        if name == "vga60" {
            // The port's passes: a tick every 4th pass.
            assert!(want.windows(2).all(|w| w[1] - w[0] == 4));
            let row = GOLDEN
                .lines()
                .find(|l| l.starts_with("scenario vga60"))
                .unwrap();
            let first = GOLDEN.lines().skip_while(|l| *l != row).nth(1).unwrap();
            let count: f64 = first.split(' ').nth(1).unwrap().parse().unwrap();
            assert!((count / 1_193_182.0 - PASS).abs() < 1e-6);
        }
        scenarios += 1;
    }
    assert_eq!(scenarios, 14);
}
