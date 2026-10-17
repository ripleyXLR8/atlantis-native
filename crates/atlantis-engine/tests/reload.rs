//! A reload asked by a game over or the load picker (`Engine::abort` set)
//! runs the chapter's whole /INIT: it used to stop after the first
//! command, since the /INIT loop stops on an abort.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::scripts::Scripts;
use atlantis_engine::sys::{Runner, Sys};
use atlantis_engine::{Abort, Engine};

#[test]
fn reload_after_game_over_runs_the_whole_init() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: skipped");
        return;
    };
    let g = Game::open(Arc::new(StdDir::new(dir))).unwrap();
    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let result = Rc::new(RefCell::new(None));
    let r2 = result.clone();
    let mut runner = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        let mut scripts = Scripts::default();
        // As after a game over: the walk set the abort, then the main loop
        // reloads (no checkpoint here: a new game, CHAPI001's /INIT).
        e.abort = Some(Abort::NewGame);
        e.load_checkpoint(&mut scripts, true).await;
        // CHAPI001's /INIT: EnableSuj 600 (1st), EnableSuj 601 (2nd),
        // HidePerso 262 (3rd) ...
        *r2.borrow_mut() = Some((
            scripts.subjects.subjects(601).to_vec(),
            e.hidden_persos.contains(&262),
        ));
    });
    for t in 0..100_000u64 {
        if !runner.step(t * 16) {
            break;
        }
    }
    let (subjects_601, hidden_262) = result.borrow().clone().expect("the reload did not finish");
    assert_eq!(subjects_601, vec![12, 1, 2]);
    assert!(hidden_262);
}
