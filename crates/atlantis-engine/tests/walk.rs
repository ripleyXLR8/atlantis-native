//! Walking around Atlantis headlessly: the engine task driven frame by
//! frame with synthetic input, from the new-game start (ATLAN1.WAM, vue 1)
//! through moves with their transition videos.
//!
//! Needs the game (`ATLANTIS_DATA`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::hooks::{ready, HookFuture};
use atlantis_engine::sys::{Input, MouseButton, Runner, Sys};
use atlantis_engine::{audio::Mixer, Engine, Event, Options, ScriptHooks};

/// What the hooks saw, shared with the test.
#[derive(Default)]
struct Seen {
    arrivals: Vec<(u16, i32)>,
    /// The place to move to next, chosen at the arrival.
    target: Option<u16>,
    /// The view angles at each warp frame.
    angles: Vec<(f32, f32)>,
    /// Zones clicked (the /con click scan's hook).
    clicks: Vec<u32>,
    departures: Vec<u16>,
}

/// Points the view at a zone with a move at each arrival (to `prefer` when
/// it is in view), as an /INIT AngleWarpX / AngleWarpY would; with
/// `start`, the game starts there, as a ChangeWam= / Vue= in /INIT would.
struct Aim {
    seen: Rc<RefCell<Seen>>,
    prefer: Option<u16>,
    start: Option<(&'static str, u16)>,
}

impl Aim {
    fn new(seen: &Rc<RefCell<Seen>>) -> Aim {
        Aim {
            seen: seen.clone(),
            prefer: None,
            start: None,
        }
    }
}

impl ScriptHooks for Aim {
    fn chapter_loaded<'a>(&'a mut self, e: &'a mut Engine, _chapter: u32) -> HookFuture<'a, ()> {
        Box::pin(async move {
            if let Some((wam, vue)) = self.start {
                assert!(e.load_wam(wam).await);
                e.next_place_id = Some(vue);
            }
        })
    }

    fn arrival_scan<'a>(
        &'a mut self,
        e: &'a mut Engine,
        vue: u16,
        arrivee: i32,
    ) -> HookFuture<'a, ()> {
        let mut seen = self.seen.borrow_mut();
        seen.arrivals.push((vue, arrivee));
        seen.target = None;
        let place = e.place().unwrap().clone();
        let mut w = e.warp.clone();
        // The first (alpha, beta) whose screen centre is over a zone with a
        // transition to another place (the preferred one first).
        for pass in 0..2 {
            'search: for b in -12..=12 {
                for k in 0..512 {
                    let (alpha, beta) = (k as f32 * w.two_pi() / 512.0, b as f32 * 0.05);
                    w.set_alpha(alpha);
                    w.set_beta(beta);
                    let (px, py) = w.screen_to_panorama(320, 240);
                    let zone = place.hit_test(px as i16, py as i16);
                    let wanted = pass == 1 || self.prefer.is_none() || self.prefer == Some(zone);
                    if wanted && zone != 0 && zone != vue && place.find_transition(zone).is_some() {
                        e.set_view(alpha, beta);
                        seen.target = Some(zone);
                        break 'search;
                    }
                }
            }
            if seen.target.is_some() {
                break;
            }
        }
        ready(())
    }

    fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
        self.seen
            .borrow_mut()
            .angles
            .push((e.warp.alpha(), e.warp.beta()));
        ready(false)
    }

    fn zone_click<'a>(&'a mut self, _e: &'a mut Engine, zone: u32) -> HookFuture<'a, ()> {
        self.seen.borrow_mut().clicks.push(zone);
        ready(())
    }

    fn departure_scan<'a>(
        &'a mut self,
        _e: &'a mut Engine,
        dest: u16,
    ) -> HookFuture<'a, atlantis_engine::Departure> {
        self.seen.borrow_mut().departures.push(dest);
        ready(atlantis_engine::Departure::Go)
    }
}

struct Harness {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    frames: u64,
}

impl Harness {
    /// Runs `n` frames of 16 ms; false once the engine has ended.
    fn run(&mut self, n: u64) -> bool {
        for _ in 0..n {
            if !self.runner.step(self.now) {
                return false;
            }
            self.now += 16;
            self.frames += 1;
        }
        true
    }

    fn click(&mut self) {
        let (x, y) = self.sys.mouse();
        self.sys.push_input(Input::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        self.run(2);
        self.sys.push_input(Input::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
    }

    /// Runs until `done` or `max` frames.
    fn run_until(&mut self, max: u64, mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..max {
            if done() {
                return true;
            }
            if !self.run(1) {
                return false;
            }
        }
        done()
    }
}

fn places_entered(trace: &RefCell<Vec<Event>>) -> Vec<(String, u16)> {
    trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::PlaceEntered { wam, place } => Some((wam.clone(), *place)),
            _ => None,
        })
        .collect()
}

#[test]
fn new_game_look_around_and_walk() {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: walk test skipped");
        return;
    };
    let game = Game::open(Arc::new(StdDir::new(dir))).unwrap();
    let mixer = Mixer::new();
    let sys = Sys::new(mixer.clone());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::new(RefCell::new(Seen::default()));
    let opts = Options {
        hooks: Box::new(Aim::new(&seen)),
        trace: Some(trace.clone()),
        ..Options::default()
    };
    let runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), game, opts));
    let mut h = Harness {
        sys: sys.clone(),
        runner,
        now: 1000,
        frames: 0,
    };

    // The new game: ATLAN1.WAM, vue 1, its arrival scan with ARRIVEEVUE =
    // LastVue = 1.
    assert!(h.run_until(200, || !seen.borrow().angles.is_empty()));
    assert_eq!(places_entered(&trace), vec![("ATLAN1.WAM".to_string(), 1)]);
    assert_eq!(seen.borrow().arrivals, vec![(1, 1)]);
    assert!(sys.mouse_locked(), "the warp view locks the mouse");
    // A rendered view with the cursor.
    let screen = sys.screen.borrow().clone();
    assert!(screen.iter().any(|&p| p != screen[0]), "a blank view");

    // Walk: three moves, each from the zone the hook aims at.
    for step in 0..3 {
        let entered = places_entered(&trace).len();
        let target = seen.borrow().target;
        let Some(target) = target else {
            panic!("no zone with a move in view at step {step}");
        };
        let videos_before = trace
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Video { .. }))
            .count();
        h.run(5);
        h.click();
        assert!(
            h.run_until(3000, || places_entered(&trace).len() > entered),
            "no place change after the click (step {step}, target {target})"
        );
        let entered_now = places_entered(&trace);
        let (wam, place) = entered_now.last().unwrap().clone();
        eprintln!("step {step}: {wam} place {place} (zone {target})");
        assert_eq!(seen.borrow().departures.last(), Some(&target));
        let videos: Vec<Event> = trace
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Video { .. }))
            .cloned()
            .collect();
        assert!(videos.len() > videos_before, "no transition video");
        if let Some(Event::Video { name, frames, .. }) = videos.last() {
            eprintln!("  video {name}: {frames} frames");
            assert!(*frames > 0);
        }
        // The next warp loop.
        let n = seen.borrow().angles.len();
        assert!(h.run_until(400, || seen.borrow().angles.len() > n));
    }
    // Look around: a motion to the right turns the view.
    let screen = sys.screen.borrow().clone();
    let (a0, _) = *seen.borrow().angles.last().unwrap();
    for _ in 0..10 {
        sys.push_input(Input::MouseDelta { dx: 12, dy: 0 });
        h.run(1);
    }
    h.run(60); // the momentum dies out
    let (a1, _) = *seen.borrow().angles.last().unwrap();
    assert!(a1 != a0, "the view did not turn");
    assert_ne!(*sys.screen.borrow(), screen, "the view was not redrawn");

    eprintln!(
        "{} frames, {} ambient voices, events: {:?}",
        h.frames,
        mixer.voice_count(),
        trace.borrow()
    );

    // Quit.
    sys.push_input(Input::Quit);
    assert!(!h.run(10), "the engine did not end on quit");
}

/// The ambient sounds of the fisherman's hut (PECHEUR1.WAM, places
/// 131..145) and the sub-WAM back to ATLAN1.WAM (place 131 to 147).
#[test]
fn ambient_sounds_and_sub_wam() {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: ambient walk test skipped");
        return;
    };
    let game = Game::open(Arc::new(StdDir::new(dir))).unwrap();
    let mixer = Mixer::new();
    let sys = Sys::new(mixer.clone());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::new(RefCell::new(Seen::default()));
    let hooks = Aim {
        start: Some(("PECHEUR1.WAM", 132)),
        prefer: Some(131),
        ..Aim::new(&seen)
    };
    let opts = Options {
        hooks: Box::new(hooks),
        trace: Some(trace.clone()),
        ..Options::default()
    };
    let runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), game, opts));
    let mut h = Harness {
        sys: sys.clone(),
        runner,
        now: 1000,
        frames: 0,
    };
    assert!(h.run_until(300, || seen.borrow().angles.len() > 10));
    assert_eq!(places_entered(&trace).last().unwrap().1, 132);
    // The two waves play (place 132 refers to objects 1 and 2), turned
    // with the view.
    assert!(
        h.run_until(100, || mixer.voice_count() > 0),
        "no ambient sound"
    );
    eprintln!("{} ambient voices at place 132", mixer.voice_count());

    // To 131, then across the sub-WAM to ATLAN1.WAM place 147.
    for want in [131u16, 147] {
        let target = seen.borrow().target;
        assert_eq!(target, Some(want), "zone {want} not found in view");
        let entered = places_entered(&trace).len();
        h.run(5);
        h.click();
        assert!(h.run_until(3000, || places_entered(&trace).len() > entered));
        let n = seen.borrow().angles.len();
        assert!(h.run_until(400, || seen.borrow().angles.len() > n));
    }
    let entered = places_entered(&trace);
    assert_eq!(entered.last().unwrap(), &("ATLAN1.WAM".to_string(), 147));
    assert!(trace
        .borrow()
        .contains(&Event::WamLoaded("ATLAN1.WAM".into())));
    // The arrival scan's origin after a sub-WAM is the place left.
    assert_eq!(seen.borrow().arrivals.last(), Some(&(147, 131)));
    // No ambient sound in ATLAN1.WAM place 147: FUN_00431e98 stopped them.
    assert!(h.run_until(10, || mixer.voice_count() == 0));
    sys.push_input(Input::Quit);
    assert!(!h.run(10));
}
