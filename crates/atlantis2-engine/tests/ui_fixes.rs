//! The bag bar, the menus' input and the media pages against the original
//! (`tests/golden/ui_fixes.txt`, atlantis2-cd-fr-2199552 run under
//! Unicorn): a move's turn held under the open bag bar, `cinema=` closing
//! the bar, the zone under the centre picked by a mouse move under the bar,
//! the button-held flag cleared by every release, the turn speeds
//! kept across the Escape menu, the menu's sliders, the music copy's
//! please-wait page, the CD page, no sprite hit recorded under the bar.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::{Path, PathBuf};

use atlantis2_data::A2Files;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::{slider_values, PageKind, Settings, WidgetKind};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/ui_fixes.txt");

fn dirs() -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn game() -> Option<A2Files> {
    let (dir, discs) = dirs()?;
    let (files, _) = A2Files::open_dir(&dir, &[discs]).expect("game folder");
    Some(files)
}

/// The rows of the golden's section whose header starts with `head`.
fn section(head: &str) -> Vec<&'static str> {
    let mut lines = GOLDEN.lines().skip_while(|l| {
        !l.strip_prefix("== ")
            .is_some_and(|h| h.split(' ').next() == Some(head))
    });
    lines.next().expect(head);
    lines.take_while(|l| !l.starts_with("== ")).collect()
}

/// An empty scratch folder.
fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("a2-ui-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn degrees(r: f32) -> f64 {
    f64::from(r) / (std::f64::consts::PI / 180.0)
}

fn radians(d: f64) -> f32 {
    (d * (std::f64::consts::PI / 180.0)) as f32
}

fn fmt3(x: f64) -> String {
    format!("{x:.3}")
}

/// What the harness does before a pass's dispatcher call.
#[derive(Clone, Copy)]
enum Act {
    Click,
    Right,
    Motion(i32, i32),
    /// Escape: the main menu opened, or closed.
    Menu,
    MenuClosed,
}

/// The engine driven as the golden's cases were run (`moves.rs`'s
/// driver): a 15 Hz tick every fourth pass, the actions before their pass,
/// one row per pass that changed something.
struct Driver {
    e: Engine,
    pass_no: u64,
    clock: f64,
}

#[derive(Clone, PartialEq)]
struct State {
    loop_type: i32,
    view: u32,
    turn: bool,
    zone: i32,
    vars: [u32; 3],
    bar: bool,
    yaw: f32,
    pitch: f32,
}

impl Driver {
    fn state(&self) -> State {
        let (yaw, pitch) = self.e.camera.angles();
        State {
            loop_type: match self.e.loop_type {
                LoopType::Warp => 0,
                LoopType::Video => 1,
                LoopType::Dialog => 2,
                LoopType::Menu => 5,
                _ => -1,
            },
            view: self.e.place().1,
            turn: self.e.turning(),
            zone: self.e.centre_index(),
            vars: [
                self.e.vars.arrivee_vue,
                self.e.vars.last_vue,
                self.e.vars.vue,
            ],
            bar: self.e.bar.open,
            yaw,
            pitch,
        }
    }

    fn enter_at(&mut self, view: u32, yaw: f64, pitch: f64) {
        self.e.enter_at(view, radians(yaw), radians(pitch)).unwrap();
        self.e.events.clear();
    }

    fn run(&mut self, acts: &[(u32, Act)], passes: u32, out: &mut Vec<String>) {
        let s0 = self.state();
        out.push(format!(
            "start view {} yaw {} pitch {} centre zone {}",
            s0.view,
            fmt3(degrees(s0.yaw)),
            fmt3(degrees(s0.pitch)),
            s0.zone
        ));
        self.clock += 100.0;
        let mut prev = s0;
        for n in 0..passes {
            if self.pass_no.is_multiple_of(4) {
                self.e.tick();
            }
            self.pass_no += 1;
            let mut evs = Vec::new();
            for &(_, a) in acts.iter().filter(|(p, _)| *p == n) {
                match a {
                    Act::Click => {
                        self.clock += 10.0;
                        self.e.click(self.clock);
                    }
                    Act::Right => {
                        evs.push("right".to_string());
                        self.e.right_click();
                    }
                    Act::Motion(dx, dy) => {
                        evs.push(format!("motion {dx},{dy}"));
                        if self.e.loop_type == LoopType::Menu {
                            self.e.pointer_moved(dx, dy);
                        } else {
                            self.e.mouse_motion(dx, dy);
                        }
                    }
                    Act::Menu => {
                        evs.push("menu".into());
                        self.e.escape();
                    }
                    Act::MenuClosed => {
                        evs.push("menu closed".into());
                        self.e.escape();
                    }
                }
            }
            self.e.pass(None);
            for ev in self.e.events.drain(..) {
                match ev {
                    Event::Click { .. } => evs.insert(0, "click".into()),
                    Event::TurnSetup { yaw, pitch, .. } => evs.push(format!(
                        "turn {},{}",
                        fmt3(degrees(yaw)),
                        fmt3(degrees(pitch))
                    )),
                    Event::VideoStart { frames, .. } => {
                        evs.push(format!("video {}", frames.unwrap_or(0)))
                    }
                    Event::VideoFrame(k) => evs.push(format!("frame {k}")),
                    _ => {}
                }
            }
            let s = self.state();
            let mut changes = Vec::new();
            if s.loop_type != prev.loop_type {
                changes.push(format!("loop={}", s.loop_type));
            }
            if s.view != prev.view {
                changes.push(format!("view={}", s.view));
            }
            if s.turn != prev.turn {
                changes.push(format!("turn={}", i32::from(s.turn)));
            }
            if s.zone != prev.zone {
                changes.push(format!("zone={}", s.zone));
            }
            for (k, name) in ["ARRIVEEVUE", "LASTVUE", "VUE"].iter().enumerate() {
                if s.vars[k] != prev.vars[k] {
                    changes.push(format!("{name}={}", s.vars[k]));
                }
            }
            if s.bar != prev.bar {
                changes.push(format!("bar={}", i32::from(s.bar)));
            }
            let moved = (degrees(s.yaw) - degrees(prev.yaw)).abs() > 1e-4
                || (degrees(s.pitch) - degrees(prev.pitch)).abs() > 1e-4;
            if !changes.is_empty() || !evs.is_empty() || moved {
                let ang = if moved || changes.iter().any(|c| c.starts_with("view")) {
                    format!(
                        " yaw {} pitch {}",
                        fmt3(degrees(s.yaw)),
                        fmt3(degrees(s.pitch))
                    )
                } else {
                    String::new()
                };
                let mut row = format!("p{n} t{}{ang} {}", self.e.ticks, changes.join(" "))
                    .trim_end()
                    .to_string();
                if !evs.is_empty() {
                    row += &format!(" | {}", evs.join("; "));
                }
                out.push(row);
            }
            prev = s;
        }
        let se = self.state();
        out.push(format!(
            "end view {} yaw {} pitch {} loop {} centre zone {} bar {}",
            se.view,
            fmt3(degrees(se.yaw)),
            fmt3(degrees(se.pitch)),
            se.loop_type,
            se.zone,
            i32::from(se.bar)
        ));
    }
}

fn compare(name: &str, got: &[String], want: &[&str]) {
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g, w, "case {name}, row {i}");
    }
    assert_eq!(got.len(), want.len(), "case {name}: row count");
}

/// The engine of a new game with the scripts, its opening film ended
/// (the music copy's page shown first when `data` has no copies).
fn scripted(files: A2Files, data: Option<PathBuf>) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.rand = Rand::new(1000);
    e.set_data_dir(data);
    e.start_new_game().unwrap();
    let mut clock = 0.0;
    for _ in 0..200 {
        if !e.copying() {
            break;
        }
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    }
    e.end_video();
    for _ in 0..30 {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    e
}

/// M1: a move's turn waits while the bag bar is open (`AtlWarp_continue`
/// runs FUN_0040c6a0, the turn step and the centre pick, only with the
/// bar closed), and goes on once it is closed.
#[test]
fn the_turn_waits_under_the_open_bar() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.trace = true;
    e.sprite_picks = false;
    e.rand = Rand::new(1000);
    let mut d = Driver {
        e,
        pass_no: 0,
        clock: 0.0,
    };
    d.enter_at(2, -75.0, 15.0);
    let mut rows = Vec::new();
    d.run(
        &[(2, Act::Click), (10, Act::Right), (150, Act::Right)],
        166,
        &mut rows,
    );
    compare("M1", &rows, &section("M1"));
}

/// The zone under the centre while the bag bar is open: the warp frame
/// does not pick it, but the motion callback (@ 0x0040c420) does on every
/// mouse move, whatever the gate; after a view entry turned the camera
/// under the bar, the zone follows at the next move.
#[test]
fn a_move_of_the_mouse_picks_the_zone_under_the_bar() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.trace = true;
    e.sprite_picks = false;
    e.rand = Rand::new(1000);
    let mut d = Driver {
        e,
        pass_no: 0,
        clock: 0.0,
    };
    d.enter_at(2, -75.0, 15.0);
    let mut rows = Vec::new();
    d.run(&[(2, Act::Right)], 3, &mut rows);
    compare("M1b", &rows, &section("M1b"));
    d.enter_at(2, 0.0, 0.0);
    let mut rows = Vec::new();
    d.run(&[(3, Act::Motion(1, 0)), (6, Act::Right)], 12, &mut rows);
    compare("M1c", &rows, &section("M1c"));
}

/// M10: the turn speeds are kept across the Escape menu and the menu's
/// pointer moves add to them; without the menu the same motion decays at
/// once.
#[test]
fn the_turn_speeds_go_across_the_menu() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::PII, Fpu::Chop, 640, 480).unwrap();
    e.trace = true;
    e.sprite_picks = false;
    e.rand = Rand::new(1000);
    let mut d = Driver {
        e,
        pass_no: 0,
        clock: 0.0,
    };
    d.enter_at(2, 0.0, 0.0);
    let mut rows = Vec::new();
    d.run(
        &[
            (2, Act::Motion(40, 0)),
            (3, Act::Menu),
            (5, Act::Motion(-10, 6)),
            (8, Act::Motion(25, -3)),
            (12, Act::MenuClosed),
        ],
        60,
        &mut rows,
    );
    compare("M10", &rows, &section("M10"));
    d.enter_at(2, 0.0, 0.0);
    let mut rows = Vec::new();
    d.run(&[(2, Act::Motion(40, 0))], 30, &mut rows);
    compare("M10b", &rows, &section("M10b"));
}

/// M3: `cinema=` closes the bag bar (@ 0x00481d4e), the hand and the
/// cursor left as they are.
#[test]
fn a_film_closes_the_bar() {
    let Some(files) = game() else { return };
    let want = section("M3");
    let mut e = scripted(files, None);
    e.right_click();
    assert!(e.bar.open);
    e.run_command(b"(cinema=12)");
    assert_eq!(e.loop_type, LoopType::Video);
    assert_eq!(
        format!("bar after the line {}", i32::from(e.bar.open)),
        want[0]
    );
    e.end_video();
    e.pass(None);
    assert!(!e.bar.open);
}

/// M9: every release of the left button clears the held flag, whatever
/// the loop type (FUN_0040c660), and every press sets it: a press that
/// closes a menu leaves the next menu's buttons free to animate, and a
/// button held into the menu keeps them still until it is released.
#[test]
fn every_release_clears_the_held_flag() {
    let want = section("M9");
    assert_eq!(want.len(), 12);
    assert!(want
        .iter()
        .all(|r| r.ends_with("left up 1 -> 0; right up 1 -> 1")));
    let Some(files) = game() else { return };
    let mut e = scripted(files, None);
    let mut clock = 100.0;
    let frame = |e: &Engine| {
        let p = e.menu().unwrap().page.as_ref().unwrap();
        match p.widget("a_toolbx.spr").unwrap().kind {
            WidgetKind::Animated { frame, .. } => frame,
            _ => panic!("not an animated button"),
        }
    };
    let mut wait = |e: &mut Engine, n: u32| {
        for _ in 0..n {
            clock += PASS;
            e.set_clock(clock);
            e.pass(None);
        }
    };
    // Pressed in the panorama and held, then Escape: the buttons are
    // still under the mouse.
    e.click(1.0);
    wait(&mut e, 1);
    e.escape();
    assert_eq!(e.loop_type, LoopType::Menu);
    let w = e
        .menu()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .widget("a_toolbx.spr")
        .unwrap();
    let (x, y) = (
        (w.hit.left + w.hit.right) / 2,
        (w.hit.top + w.hit.bottom) / 2,
    );
    e.pointer(x, y);
    wait(&mut e, 30);
    assert_eq!(frame(&e), 0);
    // Released: they animate.
    e.release();
    wait(&mut e, 30);
    assert!(frame(&e) > 0);
    // A press that closes a menu (a player symbol: a new game, under the
    // music copy's page), released outside it: the next menu's buttons
    // animate.
    let mut e = Engine::boot(
        game().unwrap(),
        Variant::default(),
        Fpu::default(),
        640,
        480,
    )
    .unwrap();
    e.open_player_menu();
    wait(&mut e, 2);
    let w = e.menu().unwrap().page.as_ref().unwrap();
    let sym = w.widget("b_gala.spr").unwrap();
    let (sx, sy) = (
        (sym.hit.left + sym.hit.right) / 2,
        (sym.hit.top + sym.hit.bottom) / 2,
    );
    e.pointer(sx, sy);
    e.click(3.0);
    wait(&mut e, 2);
    assert_ne!(e.loop_type, LoopType::Menu, "the player chosen");
    e.release();
    wait(&mut e, 200);
    e.end_video();
    wait(&mut e, 30);
    e.escape();
    assert_eq!(e.loop_type, LoopType::Menu);
    e.pointer(x, y);
    wait(&mut e, 30);
    assert!(frame(&e) > 0);
}

/// M12: the sliders' values at every opening, against the original's
/// whole table (every general volume 0..256 with every other volume
/// 0..127) under both control words; with the general volume at 0 the
/// music's is 0, not general's quotient.
#[test]
fn the_sliders_values_are_the_originals() {
    let want = section("M12");
    for (cw, fpu) in [("0x0e7f", Fpu::Chop), ("0x027f", Fpu::Double)] {
        let mut rows = Vec::new();
        for g in 0..=256 {
            for v in 0..128 {
                let s = Settings {
                    subtitles: true,
                    centre_marker: true,
                    invert_y: false,
                    width: 640,
                    height: 480,
                    general: g,
                    music: v,
                    effects: v,
                    dialogs: v,
                };
                let x = slider_values(fpu, &s);
                // The golden's order: general, music, dialogs, effects.
                rows.push(format!(
                    "g{g} m{v} {:08x} {:08x} {:08x} {:08x}",
                    x[0].to_bits(),
                    x[1].to_bits(),
                    x[2].to_bits(),
                    x[3].to_bits()
                ));
            }
        }
        let md5 = format!("{:x}", Md5::digest(rows.join("\n").as_bytes()));
        let head = want
            .iter()
            .find(|r| r.starts_with(&format!("cw {cw} table md5")))
            .unwrap();
        assert!(head.contains(&md5), "cw {cw}: {md5} / {head}");
        for r in want
            .iter()
            .filter_map(|r| r.strip_prefix(&format!("cw {cw} g")))
        {
            let r = format!("g{r}");
            assert!(rows.contains(&r), "cw {cw}: {r}");
        }
    }
}

/// The please-wait frames the original draws for a case of the golden.
fn golden_frames(case: &str) -> u32 {
    let row = section("M13")
        .into_iter()
        .find(|r| r.starts_with(case))
        .unwrap_or_else(|| panic!("{case}"));
    row.split("frames ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// Passes until the please-wait page is gone: its frame count; on each,
/// the page shown, the loop type unchanged and no tick.
fn page_frames(e: &mut Engine, clock: &mut f64) -> u32 {
    let (lt, ticks) = (e.loop_type, e.ticks);
    let mut n = 0;
    while e.copying() {
        *clock += PASS;
        e.set_clock(*clock);
        e.pass(None);
        n += 1;
        assert_eq!(e.ticks, ticks);
        if e.copying() {
            assert_eq!(e.loop_type, lt);
            assert_eq!(e.menu().and_then(|m| m.kind()), Some(PageKind::PleaseWait));
        }
        assert!(n < 1000);
    }
    n
}

/// M13: the music copy's please-wait page, for the frames the original
/// draws (two per 0x40000-byte block of the `.apc`); no page for a music
/// already copied or not on the current disc.
#[test]
fn the_music_copy_shows_the_please_wait_page() {
    let Some(files) = game() else { return };
    let data = scratch("copies");
    let mut clock = 0.0;
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.set_data_dir(Some(data.clone()));
    // The new game: chapter 0's `LOADMUSIK=30` holds the start-up section
    // under the page; the opening film starts after it.
    e.start_new_game().unwrap();
    assert!(e.copying());
    assert_eq!(e.loop_type, LoopType::Warp);
    assert_eq!(
        page_frames(&mut e, &mut clock),
        golden_frames("LOADMUSIK=30, empty copies")
    );
    assert_eq!(e.loop_type, LoopType::Video, "the film after the page");
    assert_eq!(e.menu().and_then(|m| m.kind()), None);
    e.end_video();
    for _ in 0..30 {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    }
    // Copied: no page.
    e.run_command(b"(LOADMUSIK=30)");
    assert_eq!(
        page_frames(&mut e, &mut clock),
        golden_frames("LOADMUSIK=30, muzik30.apc copied")
    );
    // Disc 1 (Tibet's at a new game): 1, 2 and 3; then with 2 copied.
    e.run_command(b"(LOADMUSIK=1,2,3)");
    assert_eq!(
        page_frames(&mut e, &mut clock),
        golden_frames("LOADMUSIK=1,2,3, empty copies")
    );
    e.run_command(b"(delmusik=1,3)");
    e.run_command(b"(LOADMUSIK=1,2,3)");
    assert_eq!(
        page_frames(&mut e, &mut clock),
        golden_frames("LOADMUSIK=1,2,3, muzik02.apc copied")
    );
    // Not on the current disc: no copy, no page.
    e.run_command(b"(LOADMUSIK=20)");
    assert_eq!(
        page_frames(&mut e, &mut clock),
        golden_frames("LOADMUSIK=20, not on the disc")
    );
    assert!(!e.media.copies.contains("muzik20"));
    // A music started whose copy is empty (FUN_0047a200 copies it).
    e.run_command(b"(startmusik=5)");
    assert_eq!(
        page_frames(&mut e, &mut clock),
        golden_frames("startmusik 5: FUN_0047a200 copies '5'")
    );
    // The copies last from one session to the next, beside the saves.
    let kept = std::fs::read_to_string(data.join("music_copies.txt")).unwrap();
    assert_eq!(kept, "muzik01\nmuzik02\nmuzik03\nmuzik05\nmuzik30\n");
    let mut e = Engine::boot(
        game().unwrap(),
        Variant::default(),
        Fpu::default(),
        640,
        480,
    )
    .unwrap();
    e.set_data_dir(Some(data));
    e.start_new_game().unwrap();
    assert!(!e.copying());
    assert_eq!(e.loop_type, LoopType::Video);
}

/// M13 with M16: the copy is a loop inside the task that started it
/// (`CopyMusikToDrive` @ 0x00479480 draws the page and flips, and calls
/// no scheduler; FUN_00584a00 is called only by the main loop, through
/// FUN_00584b40). A copy started in a pass whose 15 Hz task is due: no
/// tick during the page; the pass goes on after its last frame, so the
/// due task runs then; its next time is that pass's time + the period
/// (the time read @ 0x00584a0a, added @ 0x00584ad2..0x00584ad8 after the
/// task's callback), long past after the page: due in the next pass.
#[test]
fn a_music_copy_holds_the_scheduler_until_its_end() {
    let Some(files) = game() else { return };
    let mut clock = 0.0;
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.set_data_dir(Some(scratch("copy-scheduler")));
    e.start_new_game().unwrap();
    page_frames(&mut e, &mut clock);
    e.end_video();
    let pass = |e: &mut Engine, clock: &mut f64| {
        *clock += PASS;
        e.set_clock(*clock);
        e.pass(None);
    };
    for _ in 0..30 {
        pass(&mut e, &mut clock);
    }
    // A pass that ran the task: the next one is due 4 passes later.
    let t = e.ticks;
    while e.ticks == t {
        pass(&mut e, &mut clock);
    }
    for _ in 0..3 {
        pass(&mut e, &mut clock);
    }
    let t0 = e.ticks;
    clock += PASS;
    e.set_clock(clock);
    // The copy started in this pass's frame (a script's `LOADMUSIK`).
    e.run_command(b"(delmusik=1)");
    e.run_command(b"(LOADMUSIK=1)");
    assert!(e.copying());
    e.pass(None);
    let mut n = 1;
    while e.copying() {
        assert_eq!(e.ticks, t0, "page frame {n}");
        pass(&mut e, &mut clock);
        n += 1;
    }
    assert!(f64::from(n) * PASS > atlantis2_engine::engine::TICK, "{n}");
    assert_eq!(e.ticks, t0 + 1, "the due task after the page");
    pass(&mut e, &mut clock);
    assert_eq!(e.ticks, t0 + 2, "due again in the first pass after it");
    pass(&mut e, &mut clock);
    assert_eq!(e.ticks, t0 + 2);
}

/// The disk-full and disk-error pages only come from a copy the port does
/// not write: the golden's cases show them only after a short read or
/// write, or too little free space.
#[test]
fn the_disk_pages_need_a_copy_written() {
    for r in section("M13") {
        if r.contains("page disk-full") {
            assert!(r.contains("espace disk insufisant"), "{r}");
        }
        if r.contains("page disk-error") {
            assert!(r.contains("erreure de lecture"), "{r}");
        }
    }
}

/// Links to the disc images of `discs` numbered in `wanted` (`CDn.iso`).
fn link_discs(from: &Path, to: &Path, wanted: &[u8]) {
    for n in wanted {
        let name = format!("CD{n}.iso");
        let src = from.join(&name);
        assert!(src.exists(), "{}", src.display());
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, to.join(&name)).unwrap();
        #[cfg(not(unix))]
        std::fs::copy(&src, to.join(&name)).unwrap();
    }
}

/// M13: the CD page asks for a world's disc that was not found
/// (FUN_0040dff0), polls every 100 ticks (state 9: `100 < ticks −
/// start`) and goes on when the disc is added.
#[test]
fn a_missing_disc_opens_the_cd_page() {
    let Some((dir, discs)) = dirs() else { return };
    if !discs.join("CD4.iso").exists() {
        eprintln!("CD4.iso not in ATLANTIS2_DISCS: test skipped");
        return;
    }
    let only = scratch("discs");
    link_discs(&discs, &only, &[1, 2, 3]);
    let (files, _) = A2Files::open_dir(&dir, std::slice::from_ref(&only)).unwrap();
    assert_eq!(files.disc_numbers(), vec![1, 2, 3]);
    let mut e = scripted(files, None);
    let mut clock = 1000.0;
    let mut pass = |e: &mut Engine| {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    };
    // Disc 1's world: there.
    e.run_command(b"(monde=1)");
    pass(&mut e);
    assert_eq!(e.loop_type, LoopType::Warp);
    // Disc 4's: the CD page.
    e.run_command(b"(monde=4)");
    pass(&mut e);
    assert_eq!(e.loop_type, LoopType::Menu);
    assert_eq!(e.menu().and_then(|m| m.kind()), Some(PageKind::Cd(4)));
    let start = e.ticks;
    // Escape does not close it; nothing found for 101 ticks.
    e.escape();
    while e.ticks.wrapping_sub(start) <= 101 {
        pass(&mut e);
    }
    assert_eq!(e.loop_type, LoopType::Menu);
    // The disc added: found at the next poll, which is at most 101 ticks
    // later, never at 100.
    link_discs(&discs, &only, &[4]);
    let added = e.ticks;
    while e.loop_type == LoopType::Menu {
        pass(&mut e);
        assert!(e.ticks.wrapping_sub(added) <= 102);
    }
    assert_eq!(e.loop_type, LoopType::Warp);
    let polled = e.ticks.wrapping_sub(start);
    assert_eq!(polled % 101, 0, "polled every 101 ticks: {polled}");
}

/// M15: on a click with the bag bar open, no sprite under the centre is
/// recorded (FUN_0045b040 @ 0x0045b0cd): a click-kind 3 or 4 sprite sets
/// neither `CLICOBJET` nor `CLICPERSO`.
#[test]
fn no_sprite_hit_under_the_open_bar() {
    let want = section("M15");
    let rec = |click: u8, bar: u8, kind: i32| -> i32 {
        let row = want
            .iter()
            .find(|r| r.starts_with(&format!("click {click} bar {bar} kind {kind} cursor 1 ")))
            .unwrap();
        row.rsplit(' ').next().unwrap().parse().unwrap()
    };
    assert_eq!((rec(1, 0, 3), rec(1, 0, 4)), (0, 0));
    assert_eq!((rec(1, 1, 3), rec(1, 1, 4)), (-1, -1));
    let Some(files) = game() else { return };
    let mut e = scripted(files, None);
    let mut clock = 1000.0;
    let mut pass = |e: &mut Engine| {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    };
    // The first view of Ireland's place with a click-kind 3 or 4 sprite
    // shown: its frame's centre under the screen's.
    for c in [&b"(monde=1)"[..], b"(changewam=bigile1.wam)"] {
        e.run_command(c);
    }
    let mut found = None;
    for v in 1..120 {
        if e.wam().view(v).is_none() {
            continue;
        }
        e.run_command(format!("(vue={v})").as_bytes());
        pass(&mut e);
        found = e.anim.slots.iter().find_map(|s| {
            if !s.active || !matches!(s.click, 3 | 4) {
                return None;
            }
            (0..6).find_map(|f| {
                let b = s.frame_box(f).filter(|b| b.w > 2 && b.h > 2)?;
                let c = (f, i32::from(b.x + b.w / 2), i32::from(b.y + b.h / 2));
                Some((s.id, s.click, c))
            })
        });
        if found.is_some() {
            break;
        }
    }
    let (id, kind, centre) = found.expect("a click-kind 3 or 4 sprite in Ireland");
    let key = if kind == 3 { "CLICOBJET" } else { "CLICPERSO" };
    e.forced_centre = Some(centre);
    e.trace = true;
    // The bar open: nothing recorded, the variable left at 0.
    e.right_click();
    assert!(e.bar.open);
    e.events.clear();
    e.click(5000.0);
    pass(&mut e);
    assert_eq!(
        e.script_state().unwrap().var(key),
        Some(0),
        "{key} under the bar"
    );
    // The bar closed: the click scan runs with the sprite's id.
    e.right_click();
    assert!(!e.bar.open);
    e.events.clear();
    e.click(5010.0);
    pass(&mut e);
    let scan = e.events.iter().find_map(|ev| match ev {
        Event::ClickScan(v) => Some(if kind == 3 { v.clicobjet } else { v.clicperso }),
        _ => None,
    });
    assert_eq!(scan, Some(id), "{key} in the click scan");
}

/// M11: the load and save icons are shown, still disabled, from the
/// first frame of the disc panel's slide (state 0, action 4: FUN_00472710
/// on both), and enabled at its end (state 3, FUN_00472680).
#[test]
fn the_disc_icons_show_at_the_slides_start() {
    let Some(files) = game() else { return };
    let mut e = scripted(files, None);
    let mut clock = 1000.0;
    let mut pass = |e: &mut Engine| {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    };
    e.escape();
    pass(&mut e);
    let at = |e: &Engine, name: &str| {
        let w = e
            .menu()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .widget(name)
            .unwrap();
        (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
            w.visible,
            w.enabled,
        )
    };
    let icons = |e: &Engine| {
        let (_, _, lv, le) = at(e, "a_load.spr");
        let (_, _, sv, se) = at(e, "a_save.spr");
        ((lv, le), (sv, se))
    };
    // Hidden before.
    let ((lv, _), (sv, _)) = icons(&e);
    assert!(!lv && !sv);
    let (x, y, ..) = at(&e, "a_disc.spr");
    e.pointer(x, y);
    e.click(5000.0);
    pass(&mut e);
    e.release();
    assert_eq!(
        e.menu().unwrap().state,
        atlantis2_engine::menu::state::DISC_SLIDE
    );
    assert_eq!(icons(&e), ((true, false), (true, false)));
    for _ in 0..29 {
        pass(&mut e);
        assert_eq!(icons(&e), ((true, false), (true, false)));
    }
    pass(&mut e);
    assert_eq!(icons(&e), ((true, true), (true, true)));
    assert_eq!(
        e.menu().unwrap().state,
        atlantis2_engine::menu::state::LOAD_OR_SAVE
    );
}

/// `CHANGECD=n` (no shipped script uses it) asks for disc n − 1
/// (FUN_0040dff0(value − 1, 1), atlantis2-cd-fr-2199552 @ 0x0048275c): that
/// disc becomes the current one, and the CD page asks for it when it was
/// not found.
#[test]
fn changecd_asks_for_the_disc_below_its_value() {
    let Some((dir, discs)) = dirs() else { return };
    if !discs.join("CD4.iso").exists() {
        eprintln!("CD4.iso not in ATLANTIS2_DISCS: test skipped");
        return;
    }
    let only = scratch("changecd");
    link_discs(&discs, &only, &[1, 2, 3]);
    let (files, _) = A2Files::open_dir(&dir, std::slice::from_ref(&only)).unwrap();
    let mut e = scripted(files, None);
    let mut clock = 1000.0;
    let mut pass = |e: &mut Engine| {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    };
    e.run_command(b"(CHANGECD=3)");
    pass(&mut e);
    assert_eq!(e.loop_type, LoopType::Warp);
    assert_eq!(e.files().current_disc(), 2);
    e.run_command(b"(CHANGECD=5)");
    pass(&mut e);
    assert_eq!(e.files().current_disc(), 4);
    assert_eq!(e.loop_type, LoopType::Menu);
    assert_eq!(e.menu().and_then(|m| m.kind()), Some(PageKind::Cd(4)));
}

/// L3: `MESSAGE_CLIC=<id>` (no shipped script uses it) opens the message
/// viewer (loop type 10, FUN_00476790 / FUN_00476860): `Messages.txt`'s
/// line drawn over the panorama, which no longer turns, until a click or
/// the tick past `tick + 30`; an unknown id shows `Unknown message ID :`
/// and the id; a second `MESSAGE_CLIC` in it drops the click; Escape
/// opens the main menu (the key callback's default case, @ 0x004762f0).
#[test]
fn message_clic_shows_a_message_until_a_click_or_two_seconds() {
    let Some(files) = game() else { return };
    let mut e = scripted(files, None);
    let mut clock = 1000.0;
    let mut screen = vec![0u16; 640 * 480];
    let mut bare = vec![0u16; 640 * 480];
    let mut pass = |e: &mut Engine, out: &mut [u16]| {
        clock += PASS;
        e.set_clock(clock);
        e.pass(Some(out));
    };
    pass(&mut e, &mut bare);
    pass(&mut e, &mut bare);
    e.run_command(b"(MESSAGE_CLIC=PAUSED)");
    assert_eq!(e.loop_type, LoopType::Message);
    assert_eq!(e.message(), Some((true, b"PAUSED".to_vec())));
    let start = e.ticks;
    pass(&mut e, &mut screen);
    // The text at the centre, over the panorama.
    let differ: Vec<usize> = (0..screen.len())
        .filter(|&i| screen[i] != bare[i])
        .collect();
    assert!(!differ.is_empty(), "no text drawn");
    assert!(differ.iter().all(|&i| {
        let (x, y) = (i % 640, i / 640);
        (200..440).contains(&x) && (200..280).contains(&y)
    }));
    // `SetTextColor(0x0fffffff)`: white, as Windows 98 SE's GDI draws it
    // (Windows 11's black is not the release's), over the panorama.
    assert!(differ.iter().any(|&i| screen[i] == 0xffff), "no white text");
    assert!(differ.iter().all(|&i| screen[i] != 0), "black text");
    while e.loop_type == LoopType::Message {
        pass(&mut e, &mut screen);
        assert!(e.ticks.wrapping_sub(start) <= 32);
    }
    assert_eq!(e.ticks.wrapping_sub(start), 31);
    assert_eq!(e.loop_type, LoopType::Warp);
    // An unknown id (the value up to the line's end, `)` included); a
    // click ends it.
    e.run_command(b"(MESSAGE_CLIC=NOSUCH)");
    assert_eq!(
        e.message(),
        Some((false, b"Unknown message ID :NOSUCH)".to_vec()))
    );
    pass(&mut e, &mut screen);
    assert_eq!(e.loop_type, LoopType::Message);
    e.click(e.clock() + 1.0);
    pass(&mut e, &mut screen);
    assert_eq!(e.loop_type, LoopType::Warp);
    // Escape opens the main menu.
    e.run_command(b"(MESSAGE_CLIC=PAUSED)");
    e.escape();
    assert_eq!(e.loop_type, LoopType::Menu);
}
