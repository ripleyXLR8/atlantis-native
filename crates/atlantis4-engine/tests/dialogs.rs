//! The GUI layer and the dialogs against numbers the original executable's
//! code produced (`tests/golden/a4_gui_*.txt`, see their headers) and, on
//! the installed game, the dialog screens' geometry, a text's glyph quads
//! and the flow of a narration and of a conversation.
//!
//! The game-data tests need `ATLANTIS4_DATA` (see `atlantis4_data::testing`)
//! and skip cleanly with `ATLANTIS4_SKIP_DATA_TESTS=1`. No game text is
//! written here: lines are named by their ids.

use std::cell::RefCell;
use std::rc::Rc;

use atlantis4_data::A4Files;
use atlantis4_engine::dialog::Action;
use atlantis4_engine::gui::anim::{curve, LinearAnim};
use atlantis4_engine::gui::text::{self, Font, Frame};
use atlantis4_engine::gui::{projection, Gui};
use atlantis4_engine::Game;
use atlantis4_script::{new_game, EngineRef, Options, Script, Value};

fn golden(name: &str) -> Vec<String> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn the_gui_projection_is_the_originals_to_the_bit() {
    let lines = golden("a4_gui_projection.txt");
    assert_eq!(lines.len(), 9);
    for l in &lines {
        let f: Vec<&str> = l.split('|').map(str::trim).collect();
        let wh: Vec<u32> = f[0]
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let m = projection(wh[0], wh[1]);
        let want: Vec<f32> = f[4]
            .split_whitespace()
            .map(|x| x.parse::<f64>().unwrap() as f32)
            .collect();
        assert_eq!(m.to_vec(), want, "{l}");
        for (k, x) in want.iter().enumerate() {
            assert_eq!(m[k].is_sign_negative(), x.is_sign_negative(), "{l} m{k}");
        }
        let bits: Vec<u32> = f[5]
            .split_whitespace()
            .map(|x| u32::from_str_radix(x, 16).unwrap())
            .collect();
        assert_eq!([m[0], m[5], m[10]].map(f32::to_bits).to_vec(), bits, "{l}");
    }
}

#[test]
fn the_bar_animation_follows_the_originals_curve_to_the_bit() {
    let lines = golden("a4_gui_curve.txt");
    assert_eq!(lines.len(), 23);
    let ease = vec![0.0, 0.35, 0.68, 0.85, 0.93, 0.97, 1.0];
    let up = |t: f64| {
        let mut a = LinearAnim::new(
            500.0,
            [0.5, -0.1, 0.0, 0.0],
            [0.5, 1.0, 0.0, 0.0],
            ease.clone(),
        );
        a.start_ms = 1000.0;
        a.value_at(1000.0 + t)
    };
    let down = |t: f64| {
        let mut a = LinearAnim::new(
            500.0,
            [0.5, 1.0, 0.0, 0.0],
            [0.5, -0.1, 0.0, 0.0],
            ease.clone(),
        );
        a.start_ms = 1000.0;
        a.value_at(1000.0 + t)
    };
    for l in &lines {
        let v: Vec<f64> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
        let t = v[0];
        assert_eq!(curve(&ease, t, 500.0), v[1], "progress {l}");
        let (u, fin) = up(t);
        assert_eq!(u[1], v[2] as f32, "up {l}");
        assert_eq!(down(t).0[1], v[3] as f32, "down {l}");
        assert_eq!(fin, v[4] == 1.0, "finished {l}");
    }
    // The extra samples of the golden's last line.
    assert_eq!(
        curve(&ease, 12.5, 500.0),
        "0.052499999999999998".parse().unwrap()
    );
    assert_eq!(
        curve(&ease, 499.0, 500.0),
        "0.99963999999999997".parse().unwrap()
    );
}

fn files() -> Option<A4Files> {
    let dir = atlantis4_data::testing::install_dir()?;
    Some(A4Files::open_dir(&dir).unwrap().0)
}

fn close(a: f32, b: f64, what: &str) {
    // The golden prints 6 significant digits (`%g`).
    let tol = 5e-6 * b.abs().max(1.0) * 10.0;
    assert!((f64::from(a) - b).abs() <= tol, "{what}: {a} vs {b}");
}

#[test]
fn the_dialog_screen_lands_where_the_originals_code_puts_it() {
    let Some(files) = files() else { return };
    let mut g = Gui::load(&files, "GUI/dialog.lua").unwrap();
    let id = |g: &Gui, n: &str| g.find(n).unwrap();
    let (lock, bar, video, text, quit) = (
        id(&g, "dialogLockButton"),
        id(&g, "dialog"),
        id(&g, "video"),
        id(&g, "text"),
        id(&g, "quit"),
    );
    // The talking heads are 800 × 448 (the sprite's ratio after a load).
    g.node_mut(video).ratio = (800.0f64 / 448.0) as f32;
    for l in golden("a4_gui_dialog_layout.txt") {
        let w: Vec<&str> = l.split_whitespace().collect();
        let (ww, wh): (u32, u32) = (w[0].parse().unwrap(), w[1].parse().unwrap());
        let what: Vec<&str> = w[2..]
            .iter()
            .take_while(|x| x.parse::<f64>().is_err())
            .copied()
            .collect();
        let what = what.join(" ");
        let nums: Vec<f64> = w.iter().filter_map(|x| x.parse().ok()).skip(2).collect();
        let win = (ww as f32, wh as f32);
        let anchor_y = if what.contains("up") { 1.0 } else { -0.1 };
        g.node_mut(bar).anchor[1] = anchor_y;
        g.update(lock, win, (0.0, 0.0), None);
        let node = match what.as_str() {
            "dialogLockButton size" => lock,
            "video size" => video,
            "dialog bar down size" | "dialog bar up size" => bar,
            "text box size" => text,
            "quit size" => quit,
            other => panic!("{other}"),
        };
        let n = g.node(node);
        close(n.computed_size.0, nums[0], &format!("{l} w"));
        close(n.computed_size.1, nums[1], &format!("{l} h"));
        let (px, py) = if node == text {
            let b = g.node(bar).world;
            (n.world.0 - b.0, n.world.1 - b.1)
        } else {
            n.world
        };
        close(px, nums[2], &format!("{l} x"));
        close(py, nums[3], &format!("{l} y"));
    }
}

#[test]
fn glyph_quads_are_the_transcribed_ones() {
    // The quads of the line drawer (@ 0x004a2020) for Arial 16, box 200:
    // L = 103, block 103 × 20; left: `H` (1, 4)–(12, 16), `!` (100, 4)–
    // (103, 16); centred: `H` (49, 4)–(60, 16).
    let Some(files) = files() else { return };
    let font = Font::parse(&files.read("Common/Fonts/Arial_r_16.tef").unwrap()).unwrap();
    let frame = Frame {
        width: 200,
        single_line: false,
        inter_line: 0.0,
    };
    let m = text::parse_markup(b"Hello, Atlantis!");
    let b = text::build(&font, m.clone(), frame);
    assert_eq!(b.lines.len(), 1);
    assert_eq!(b.lines[0].width, 103.0);
    assert_eq!((b.width, b.height), (103, 20));
    let q = text::line_quads(&font, &b, &b.lines[0]);
    assert_eq!((q[0].x0, q[0].y0, q[0].x1, q[0].y1), (1, 4, 12, 16));
    assert_eq!((q[1].x0, q[1].y0, q[1].x1, q[1].y1), (12, 7, 21, 16));
    let last = q.last().unwrap();
    assert_eq!((last.x0, last.y0, last.x1, last.y1), (100, 4, 103, 16));
    let mut c = m;
    c.align = text::Align::Center;
    let b = text::build(&font, c, frame);
    let q = text::line_quads(&font, &b, &b.lines[0]);
    assert_eq!((q[0].x0, q[0].x1), (49, 60));
}

const W: u32 = 1365;
const H: u32 = 768;
const T0: u64 = 5_000_000;

/// The game, its script and the test clock (µs).
type Booted = (Rc<RefCell<Game>>, Script, Rc<RefCell<u64>>);

fn boot(node: &str) -> Option<Booted> {
    let files = files()?;
    let clock = Rc::new(RefCell::new(T0));
    let mut g = Game::new(files, H);
    let c = clock.clone();
    g.clock_us = Some(Box::new(move || *c.borrow()));
    // The sounds' clock starts here (no output: mixed on this clock).
    g.update(T0 as f64 / 1000.0);
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let script = new_game(&engine, Options::default(), node.as_bytes()).unwrap();
    Some((game, script, clock))
}

/// Steps the clock by 20 ms until `until` µs, running the dialog
/// callbacks; returns them by name and first argument.
fn run(
    game: &Rc<RefCell<Game>>,
    script: &mut Script,
    clock: &Rc<RefCell<u64>>,
    until: u64,
) -> Vec<String> {
    let mut seen = Vec::new();
    while *clock.borrow() < until {
        *clock.borrow_mut() += 20_000;
        let now = *clock.borrow() as f64 / 1000.0;
        game.borrow_mut().update(now);
        let mut cbs = game.borrow_mut().sound_callbacks();
        cbs.extend(game.borrow_mut().dialog_callbacks());
        for (name, args) in cbs {
            let arg = match args.first() {
                Some(Value::Str(s)) => String::from_utf8_lossy(s).into_owned(),
                _ => String::new(),
            };
            if name != "OnSoundStopped" {
                seen.push(format!("{name}({arg})"));
            }
            script.callback(&name, &args).unwrap();
        }
    }
    seen
}

#[test]
fn a_narration_shows_each_line_then_closes() {
    // `3D/Centre17_C1/0001.wp`'s first entry queues five lines (22-1..5).
    let Some((game, mut script, clock)) = boot("3D/Centre17_C1/0001.wp") else {
        return;
    };
    {
        let g = game.borrow();
        assert!(g.dialogs.shown());
        assert_eq!(g.dialogs.current.as_ref().unwrap().id, "22-1");
        assert_eq!(g.dialogs.queue.len(), 4);
        assert!(!g.dialogs.in_conversation);
    }
    let mut frame = vec![0u32; (W * H) as usize];
    game.borrow_mut().draw(&mut frame, W, H, None);
    let seen = run(&game, &mut script, &clock, T0 + 120_000_000);
    assert_eq!(
        seen,
        [
            "OnDialogFinished(22-1)",
            "OnDialogFinished(22-2)",
            "OnDialogFinished(22-3)",
            "OnDialogFinished(22-4)",
            // The empty queue closes the narration inside `showNext`,
            // before the story hears of the last line.
            "OnDialogClosed()",
            "OnDialogFinished(22-5)",
        ]
    );
    assert!(!game.borrow().dialogs.shown());
}

#[test]
fn a_click_on_the_bar_skips_the_line() {
    let Some((game, mut script, clock)) = boot("3D/Centre17_C1/0001.wp") else {
        return;
    };
    // Up after 500 ms; the bar is 830 wide at the bottom centre.
    run(&game, &mut script, &clock, T0 + 600_000);
    if let Some(dir) = std::env::var_os("A4_DUMP_DIALOG") {
        let mut frame = vec![0u32; (W * H) as usize];
        game.borrow_mut().draw(&mut frame, W, H, None);
        dump(&frame, &std::path::Path::new(&dir).join("narration.ppm"));
    }
    let (x, y) = (f64::from(W) / 2.0, f64::from(H) - 40.0);
    assert!(game.borrow_mut().gui_press(W, H, x, y));
    let actions = game.borrow_mut().gui_release(W, H, x, y).unwrap();
    assert!(actions.is_empty());
    // Down 500 ms later: the next line.
    let t = *clock.borrow() + 600_000;
    let seen = run(&game, &mut script, &clock, t);
    assert_eq!(seen, ["OnDialogFinished(22-1)"]);
    assert_eq!(game.borrow().dialogs.current.as_ref().unwrap().id, "22-2");
}

#[test]
fn a_conversation_offers_its_topics_and_a_click_answers() {
    let Some((game, mut script, clock)) = boot("3D/Centre17_C1/0001.wp") else {
        return;
    };
    run(&game, &mut script, &clock, T0 + 120_000_000);
    script.run_chunk(b"ShowDialog()", b"test");
    {
        let g = game.borrow();
        assert!(g.dialogs.shown() && g.dialogs.in_conversation);
        assert!(g.dialogs.answers_shown());
        assert_eq!(g.dialogs.answers.len(), 6);
        assert!(g.dialogs.video.is_some());
    }
    let mut frame = vec![0u32; (W * H) as usize];
    game.borrow_mut().draw(&mut frame, W, H, None);
    if let Some(dir) = std::env::var_os("A4_DUMP_DIALOG") {
        dump(&frame, &std::path::Path::new(&dir).join("conversation.ppm"));
    }
    // The six icons, 110 apart plus a 6.24 gap, centred on the window's
    // middle, x 0.1 of the window.
    let centres: Vec<(f32, f32)> = {
        let g = game.borrow();
        g.dialogs
            .answers
            .iter()
            .map(|a| a.gui.node(a.button).world)
            .collect()
    };
    assert_eq!(centres[0].0, -546.0);
    assert!((centres[1].1 - centres[0].1 - 116.24).abs() < 1e-3);
    assert!((centres[0].1 + centres[5].1).abs() < 1e-3);
    // A click on the second icon answers "24-1".
    let (x, y) = (
        f64::from(W) / 2.0 + f64::from(centres[1].0),
        f64::from(H) / 2.0 + f64::from(centres[1].1),
    );
    assert!(game.borrow_mut().gui_press(W, H, x, y));
    let actions = game.borrow_mut().gui_release(W, H, x, y).unwrap();
    assert_eq!(
        actions,
        [
            Action::Lua("OnAnswered".into(), vec![Value::Str(b"24-1".to_vec())]),
            Action::HideAnswers
        ]
    );
    game.borrow_mut().dialogs.hide_answers();
    // The text panel beside the icon is not clickable.
    script.run_chunk(b"ShowDialog()", b"test");
    let (x2, y2) = (x + 200.0, y);
    game.borrow_mut().gui_press(W, H, x2, y2);
    let actions = game.borrow_mut().gui_release(W, H, x2, y2).unwrap();
    assert!(actions.is_empty());
    // `quit` closes the conversation: `OnDialogClosed()` (with the
    // topics hidden: their full-window lock button lies above it).
    game.borrow_mut().dialogs.hide_answers();
    let (qx, qy) = (f64::from(W) * 0.92 + 30.0, f64::from(H) * 0.01 + 30.0);
    assert!(game.borrow_mut().gui_press(W, H, qx, qy));
    let actions = game.borrow_mut().gui_release(W, H, qx, qy).unwrap();
    assert_eq!(actions, [Action::Lua("OnDialogClosed".into(), vec![])]);
    let _ = clock;
}

fn dump(px: &[u32], path: &std::path::Path) {
    let mut out = format!("P6\n{W} {H}\n255\n").into_bytes();
    for p in px {
        out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    std::fs::write(path, out).unwrap();
}
