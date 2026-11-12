//! The player's whole loop through the menus, as the viewer runs it: the
//! splashes, the main menu's *new game* (the planks, then `Game::enter`
//! and `Game::load("save.xml")` of a missing file: a new game), the
//! first node, the inventory menu's *menu* button (`save.xml`, the
//! portrait planks, `Game::leave` at their turn, the main menu), then
//! *continue*, which loads `save.xml` back: the same node and the same
//! globals.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder (see
//! `atlantis4_data::testing`); skipped cleanly with
//! `ATLANTIS4_SKIP_DATA_TESTS=1`. The saves stay in memory.

use std::cell::RefCell;
use std::rc::Rc;

use atlantis4_data::A4Files;
use atlantis4_engine::dialog::Action;
use atlantis4_engine::game_saves::GameSaves;
use atlantis4_engine::gui::State;
use atlantis4_engine::menus::{Front, FrontAction, Screen};
use atlantis4_engine::options::Settings;
use atlantis4_engine::saves::SaveStore;
use atlantis4_engine::Game;
use atlantis4_script::{read_boot_vars, EngineRef, Options, SavedGlobal, Script, Value};

const W: u32 = 1365;
const H: u32 = 768;

/// The viewer's part: the game, its saves, the front end, the script.
struct App {
    game: Rc<RefCell<Game>>,
    saves: GameSaves,
    front: Front,
    script: Option<Script>,
    /// The front end's clock, ms.
    now: f64,
}

impl App {
    /// Runs the front end to `now` and does what it asks, as the viewer.
    fn tick_to(&mut self, now: f64) -> Vec<FrontAction> {
        self.now = now;
        self.front.tick(now, &mut self.saves);
        let actions = self.front.take_actions();
        for a in &actions {
            match a {
                FrontAction::StartGame(name) => {
                    Game::leave(&self.game, self.script.take().as_ref());
                    self.script = Some(self.saves.load(&self.game, name).unwrap());
                }
                FrontAction::LeaveGame => {
                    Game::leave(&self.game, self.script.as_ref());
                    self.script = None;
                    self.front.game_left(&self.saves);
                }
                _ => {}
            }
        }
        actions
    }

    /// A click on a front-end button.
    fn click_front(&mut self, name: &str) {
        let r = self
            .front
            .rect_of(name, W, H)
            .unwrap_or_else(|| panic!("no {name}"));
        let (x, y) = (r.left + r.width / 2.0, r.top + r.height / 2.0);
        assert!(self.front.press(W, H, x, y), "{name}");
        self.front.release(W, H, x, y, self.now, &mut self.saves);
    }

    /// A click on the game's GUI at (`x`, `y`), its Lua callbacks run.
    fn click_game(&mut self, x: f64, y: f64) {
        assert!(self.game.borrow_mut().gui_press(W, H, x, y));
        let actions = self.game.borrow_mut().gui_release(W, H, x, y);
        let script = self.script.as_mut().unwrap();
        for a in actions.unwrap_or_default() {
            match a {
                Action::Lua(name, args) => script.callback(&name, &args).unwrap(),
                Action::HideAnswers => self.game.borrow_mut().dialogs.hide_answers(),
            }
        }
        self.callbacks();
    }

    fn callbacks(&mut self) {
        let script = self.script.as_mut().unwrap();
        loop {
            let cbs = self.game.borrow_mut().gui_callbacks();
            if cbs.is_empty() {
                break;
            }
            for (n, a) in cbs {
                script.callback(&n, &a).unwrap();
            }
        }
    }

    /// The viewer's check after the game's GUI: the *menu* button's
    /// `save.xml`, then the portrait planks.
    fn main_menu_button(&mut self) {
        if self.game.borrow_mut().take_main_menu() {
            let script = self.script.as_mut().unwrap();
            self.saves.save(&self.game, script, "save.xml").unwrap();
            self.front.to_main_menu(self.now);
        }
    }

    fn node(&self) -> String {
        String::from_utf8(self.game.borrow().warp_name().to_vec()).unwrap()
    }

    fn globals(&self) -> Vec<SavedGlobal> {
        let mut g = self.script.as_ref().unwrap().globals().unwrap();
        g.sort_by(|a, b| a.name.cmp(&b.name));
        g
    }
}

/// Starts as the viewer: the splashes, then the main menu at 3 s.
fn app() -> Option<App> {
    let dir = atlantis4_data::testing::install_dir()?;
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let clock = Rc::new(RefCell::new(5_000_000u64));
    let mut g = Game::new(files.clone(), H);
    let c = clock.clone();
    g.clock_us = Some(Box::new(move || *c.borrow()));
    g.sounds.headless = true;
    g.app_planks = true;
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    game.borrow_mut().configure_saves(&boot, None);
    let saves = GameSaves {
        game: game.clone(),
        options: Options::default(),
    };
    let mut front = Front::new(&files, Settings::default(), "1.0.1".into());
    front.start(0.0, &saves);
    let mut a = App {
        game,
        saves,
        front,
        script: None,
        now: 0.0,
    };
    a.tick_to(1500.0);
    a.tick_to(3000.0);
    assert_eq!(a.front.screen, Screen::Main);
    Some(a)
}

#[test]
fn new_game_menu_button_then_continue_comes_back_to_the_same_node() {
    let Some(mut a) = app() else { return };
    // Leftovers of an earlier game: *new game* removes them all.
    {
        let mut g = a.game.borrow_mut();
        g.backup
            .write("Backup/3.xml", b"<PlayedTime value=\"1.0\" />");
        g.backup
            .write("Backup/10.xml", b"<PlayedTime value=\"1.0\" />");
    }
    assert_eq!(
        a.front.button_state("continueGameButton"),
        Some(State::Disabled)
    );
    a.click_front("newGameButton");
    assert!(!a.saves.exists("3.xml") && !a.saves.exists("10.xml"));
    a.tick_to(4000.0);
    assert!(a.script.is_none());
    let acts = a.tick_to(5000.0);
    assert!(acts.contains(&FrontAction::StartGame("save.xml".into())));
    assert_eq!(a.front.screen, Screen::Game);
    assert_eq!(a.node(), "3D/Bateau_CA/0001.wp");
    a.tick_to(6000.0);
    assert!(!a.front.in_transition());

    // The intro skipped and the first node's two tutorials closed.
    a.game.borrow_mut().movie = None;
    a.script
        .as_mut()
        .unwrap()
        .callback(
            "OnMovieFinished",
            &[Value::Str(b"Videos/intro.ogv".to_vec())],
        )
        .unwrap();
    for _ in 0..2 {
        a.game.borrow_mut().hide_document();
        a.callbacks();
    }
    // A mark of this game in its globals, and a view turned.
    a.script
        .as_mut()
        .unwrap()
        .run_chunk(b"FLOW_TEST = 7 FLOW_NAME = 'menu'", b"t");
    a.game.borrow_mut().view.set_yaw(30.0);
    let yaw = a.game.borrow().view.yaw;
    let node = a.node();
    let globals = a.globals();
    assert!(globals.iter().any(|g| g.name == b"FLOW_TEST"));

    // The HUD's inventory button, then the menu's `mainMenuButton`.
    let b = a.game.borrow_mut().gui().hud_button(W, H).unwrap();
    a.click_game(b.left + b.width / 2.0, b.top + b.height / 2.0);
    let m = a
        .game
        .borrow_mut()
        .gui()
        .menu_button("mainMenuButton", W, H)
        .unwrap();
    a.click_game(m.left + m.width / 2.0, m.top + m.height / 2.0);
    a.main_menu_button();
    assert!(a.saves.exists("save.xml"));
    assert!(a.front.in_transition());
    // Close, rotate: at the turn the game is left and the menu enters.
    a.tick_to(7000.0);
    assert!(a.script.is_some());
    let acts = a.tick_to(8000.0);
    assert!(acts.contains(&FrontAction::LeaveGame));
    assert!(a.script.is_none());
    assert!(a.game.borrow().warp.is_none());
    assert_eq!(a.front.screen, Screen::Main);
    assert_eq!(a.front.button_state("continueGameButton"), Some(State::Up));
    a.tick_to(9000.0);
    assert!(!a.front.in_transition());

    // *Continue*: `save.xml` loaded at the landscape turn.
    a.click_front("continueGameButton");
    a.tick_to(10_000.0);
    let acts = a.tick_to(11_000.0);
    assert!(acts.contains(&FrontAction::StartGame("save.xml".into())));
    assert_eq!(a.node(), node);
    assert_eq!(a.globals(), globals);
    assert!((a.game.borrow().view.yaw - yaw).abs() < 1e-4);
    assert!(a.game.borrow().warp.is_some());
}

/// `FinishGame` with the application's planks: no planks of the game's
/// own, the portrait close; at the turn `Game::leave` autosaves at
/// `LZ/0004` and the main menu enters, without the credits.
#[test]
fn the_end_goes_back_to_the_main_menu_through_the_planks() {
    let Some(mut a) = app() else { return };
    a.click_front("newGameButton");
    a.tick_to(4000.0);
    a.tick_to(5000.0);
    a.tick_to(6000.0);
    a.game.borrow_mut().movie = None;
    let s = a.script.as_mut().unwrap();
    s.change_warp(b"3D/LZ/0004.wp").unwrap();
    s.callback(
        "OnMovieFinished",
        &[Value::Str(b"Videos/epilogue.ogv".to_vec())],
    )
    .unwrap();
    assert!(a.game.borrow().planks.is_none());
    assert!(a.game.borrow_mut().take_finish());
    a.front.to_main_menu(a.now);
    a.tick_to(7000.0);
    let acts = a.tick_to(8000.0);
    assert!(acts.contains(&FrontAction::LeaveGame));
    assert_eq!(a.front.screen, Screen::Main);
    let g = a.game.borrow();
    assert_eq!(g.last_save.as_ref().unwrap().0, "save.xml");
    let text = String::from_utf8_lossy(&g.last_save.as_ref().unwrap().1).into_owned();
    assert!(text.contains("<WarpName name=\"3D/LZ/0004.wp\" />"));
    assert!(g.has_autosave());
}
