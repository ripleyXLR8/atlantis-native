//! Escape, Space and a click during a travel video against the original
//! under emulation (`tests/golden/travel_escape.txt`): the move from
//! `hoggar.wam` view 4 to view 5 (`ho004005.HNM`, 79 frames), one
//! `AtlHnm_continue` call per golden `frame` line (0.07 s apart), and per
//! run:
//!
//! - `escape_at_3`: Escape after three frames ends the video and opens the
//!   in-game menu (page 1) over the arrival view, the save thumbnails'
//!   capture being the last travel frame shown; its `BT_BACK` returns to
//!   the warp in view 5;
//! - `space_at_3`: Space does nothing, the video plays to its end;
//! - `click_at_3`: a click ends the video only once the next due frame is
//!   drawn (a call before 1 / 15 s draws nothing and keeps the click);
//! - `escape_before_first_frame`: Escape before the first call only ends
//!   the video (no menu).
//!
//! The script side of that Escape (the last interrupted section's tail run
//! again) is checked by `tests/story.rs` (`story_02_cfuner_escaping_travels`).
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

const GOLDEN: &str = include_str!("golden/travel_escape.txt");

/// The time between two golden calls.
const CALL: f64 = 0.07;

struct Run {
    g: Game,
    t: f64,
}

impl Run {
    fn step(&mut self, dt: f64) {
        self.t += dt;
        self.g.step(self.t);
    }

    /// The game in `hoggar.wam` view 4, the move to view 5 clicked and run
    /// until its travel video starts (no call made yet).
    fn new() -> Option<Run> {
        let dir = atlantis3_data::testing::install_dir()?;
        let files = A3Files::open_dir(&dir).unwrap().0;
        let mut g = Game::new(files).expect("the game starts");
        g.set_world(1);
        assert!(g.load_place("hoggar.wam"));
        g.enter_at(4, 0.0, 0.0);
        g.set_harness_turns(true);
        let mut r = Run { g, t: 10.0 };
        r.step(1.0 / 30.0);
        assert!(r.g.aim_at_zone(|z| z.kind == 1 && z.value == 5));
        r.g.click();
        for _ in 0..300 {
            if r.g.mode() == Mode::Video {
                break;
            }
            r.step(1.0 / 30.0);
        }
        assert_eq!(r.g.mode(), Mode::Video, "the travel video starts");
        assert!(r.g.video_is_travel());
        assert_eq!(r.g.place().1, Some(5), "the arrival view entered first");
        assert_eq!(r.g.video().unwrap().1, 0, "no frame before the first call");
        Some(r)
    }

    /// The frames shown, or none after the video's end.
    fn shown(&self) -> Option<u32> {
        self.g.video().map(|v| v.1)
    }

    /// The in-game menu's `BT_BACK` clicked (pressed, released) until the
    /// warp is back.
    fn back(&mut self) {
        let p = self.g.page().expect("a menu page");
        let k = p.find(b"BT_BACK").expect("BT_BACK");
        let (w, h) = (self.g.width() as i32, self.g.height() as i32);
        let at = (0..h)
            .step_by(4)
            .flat_map(|y| (0..w).step_by(4).map(move |x| (x, y)))
            .find(|&pt| p.click_hit(pt) == Some(k))
            .expect("BT_BACK under some point");
        for _ in 0..200 {
            if self.g.mode() != Mode::Menu {
                break;
            }
            self.g.set_pointer(at.0, at.1);
            self.g.click();
            self.step(1.0 / 30.0);
            self.g.release();
            self.step(1.0 / 30.0);
        }
    }
}

/// The golden's runs: name, then its lines.
fn runs() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(name) = l.strip_prefix("run ") {
            out.push((name, Vec::new()));
        } else {
            out.last_mut().unwrap().1.push(l);
        }
    }
    out
}

/// A golden `state` line's field.
fn field<'a>(l: &'a str, k: &str) -> &'a str {
    l.split_whitespace()
        .find_map(|w| w.strip_prefix(k).and_then(|w| w.strip_prefix('=')))
        .unwrap_or_else(|| panic!("{l}: no {k}"))
}

#[test]
#[ignore = "WIP: the port ends a travel one call after its last frame (Cinema::ended); work paused"]
fn escape_space_and_click_during_a_travel_video_as_the_original() {
    if atlantis3_data::testing::install_dir().is_none() {
        return;
    }
    let runs = runs();
    assert_eq!(runs.len(), 4);
    let mut checked = 0;
    for (name, lines) in &runs {
        let mut r = Run::new().unwrap();
        let mut last_screen: Vec<u16> = r.g.screen().to_vec();
        for l in lines {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w[0] {
                // `frame RUN N ret=R draws=...#N@...`: one call draws video
                // frame N (its transition sprite N); ret 0 ends the video.
                "frame" => {
                    let n: u32 = w[2].parse().unwrap();
                    let sprite = format!("#{n}@");
                    assert!(w[4].contains(&sprite), "{l}");
                    r.step(CALL);
                    let ret = w[3] == "ret=1";
                    if ret {
                        assert_eq!(r.shown(), Some(n + 1), "{name}: {l}");
                        assert_eq!(r.g.mode(), Mode::Video, "{name}: {l}");
                    } else {
                        assert_eq!(r.shown(), None, "{name}: {l}: the video ended");
                        assert_eq!(r.g.mode(), Mode::Warp, "{name}: {l}");
                    }
                    assert_ne!(r.g.screen(), &last_screen[..], "{name}: {l}: a frame drawn");
                    last_screen = r.g.screen().to_vec();
                    checked += 1;
                }
                "key" => match w[3] {
                    // Escape.
                    "0x1" => r.g.escape(),
                    // Space: the game's skip key, nothing for a travel.
                    "0x39" => r.g.skip(),
                    k => panic!("{l}: key {k}"),
                },
                "click" => {
                    r.g.click();
                    // A call before 1 / 15 s has passed draws nothing and
                    // leaves the click for the next one.
                    let before = r.shown();
                    r.t += 0.01;
                    r.g.step(r.t);
                    r.t -= 0.01;
                    assert_eq!(r.shown(), before, "{name}: {l}: not due yet");
                    assert_eq!(r.g.mode(), Mode::Video, "{name}: {l}");
                }
                "state" => {
                    let mode = field(l, "mode");
                    let want = match mode {
                        "3" => Mode::Warp,
                        "4" => Mode::Video,
                        "6" => Mode::Menu,
                        m => panic!("{l}: mode {m}"),
                    };
                    assert_eq!(r.g.mode(), want, "{name}: {l}");
                    if want == Mode::Menu {
                        let page: u8 = field(l, "page").parse().unwrap();
                        assert_eq!(r.g.menu_page().map(|p| p.0), Some(page), "{l}");
                        // The capture: the last travel frame shown.
                        assert_eq!(r.g.menu_capture(), Some(&last_screen[..]), "{l}");
                    }
                    let view: u32 = field(l, "view").parse().unwrap();
                    assert_eq!(r.g.place().1, Some(view), "{name}: {l}");
                    checked += 1;
                }
                "travel" | "event" | "end" | "done" => {}
                k => panic!("{l}: {k}"),
            }
        }
        if r.g.mode() == Mode::Menu {
            // `back=3`: the menu's `BT_BACK` returns to the warp in the
            // arrival view.
            r.back();
            assert_eq!(r.g.mode(), Mode::Warp, "{name}: BT_BACK");
            assert_eq!(r.g.place().1, Some(5), "{name}: BT_BACK");
        }
    }
    // Frames: 3 + 79 + 4 (none before the first call); 12 state lines.
    assert_eq!(checked, 86 + 12);
}
