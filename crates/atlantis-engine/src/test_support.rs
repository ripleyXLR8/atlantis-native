//! Helpers of the in-crate tests that need the game: set `ATLANTIS_DATA`
//! to the game directory (or the variable a test names for another
//! release); without it the tests are skipped.

use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};

use crate::audio::Mixer;
use crate::engine::Engine;
use crate::hooks::DirectLoader;
use crate::sys::{Input, MouseButton, Runner, Sys};

/// Runs `task` on an engine over the game, frame by frame (16 ms each);
/// `each_frame` is called after every frame with the frame number (to
/// push input). False when the game is not available.
pub(crate) fn with_engine<Fut: Future<Output = ()> + 'static>(
    test: &str,
    task: impl FnOnce(Engine) -> Fut,
    each_frame: impl FnMut(&Rc<Sys>, u64),
) -> bool {
    with_engine_in("ATLANTIS_DATA", test, task, each_frame)
}

/// [`with_engine`] over the game folder that the variable `var` names
/// (`ATLANTIS_DATA_FR1997_MAY`...).
pub(crate) fn with_engine_in<Fut: Future<Output = ()> + 'static>(
    var: &str,
    test: &str,
    task: impl FnOnce(Engine) -> Fut,
    mut each_frame: impl FnMut(&Rc<Sys>, u64),
) -> bool {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: {test} skipped");
        return false;
    };
    let game = Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory");
    let sys = Sys::new(Mixer::new());
    let e = Engine::new(sys.clone(), game, Rc::new(DirectLoader));
    let mut runner = Runner::new(sys.clone(), task(e));
    let mut frame = 0u64;
    while runner.step(frame * 16) {
        each_frame(&sys, frame);
        frame += 1;
        assert!(frame < 200_000, "{test}: the task did not end");
    }
    true
}

/// A press (`down`) or a release of a mouse button at the mouse.
pub(crate) fn button(sys: &Sys, button: MouseButton, down: bool) {
    let (x, y) = sys.mouse();
    sys.push_input(if down {
        Input::MouseDown { button, x, y }
    } else {
        Input::MouseUp { button, x, y }
    });
}
