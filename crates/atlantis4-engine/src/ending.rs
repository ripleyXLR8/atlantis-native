//! `FinishGame` and the end of a game (atlantis4-gog-en-2363910).
//!
//! The binding (@ 0x005807e0 → 0x0052bc30) takes no argument and does, in
//! order (`STATIC`):
//! 1. adds `Game::onFinished` (@ 0x00526910) to the application's
//!    "closed" signal (`App+0xdb0`, `0x00409110`, priority 0.0);
//! 2. closes the planks (`App+0x6b0`, @ 0x0051f3c0): the layout of
//!    `GUI/RotationAnim.lua` (`rotationAnim`, a full-screen button over
//!    everything) with `portraitClosePlancheHaut` on `plancheHaut` and
//!    `portraitClosePlancheBas` on `plancheBas` (1,000 ms each, the
//!    two planks meeting in the middle) and the sound `portraitCloseSound`
//!    (`Sounds/SFX/E_Piano_Clavier_ferme_NA.wav`);
//! 3. clears `game+0x4038` (the view's update; the warp takes no input).
//!
//! When the signal fires (the planks closed, `[INFERRED]`: the signal's
//! other users are the menus' transitions), `Game::onFinished` removes
//! itself from it (`0x00409280`), runs `Game::leave` (@ 0x00525f50: the
//! autosave `save.xml`, since a node is shown, the music stopped, the
//! scene and the Lua state dropped) and enters the main menu (`App+0x1090`,
//! `0x00566000`, `GUI/MainMenu.lua`). No credits: `GUI/Credits.lua` is
//! loaded only by the credits screen (@ 0x00582480, reached from the
//! options menu's `creditsButton` `[INFERRED]`), which nothing on this
//! path calls.
//!
//! The scripts reach it once: `3D/LZ/0004.lua`'s `OnMovieFinished` after
//! `Videos/epilogue.ogv` (`DATA`).

use std::collections::HashMap;

use atlantis4_data::A4Files;
use atlantis4_script::{BootValue, EngineRef, GuiState, Options};

use crate::canvas::Canvas;
use crate::gui::{self, Gui, Id, Painter};
use crate::layout::Rect;
use crate::package::Picture;

/// The planks' layout file (`DATA`: the only file with `plancheHaut`,
/// `plancheBas` and the `portraitClose*` animations).
pub const ROTATION_LUA: &str = "GUI/RotationAnim.lua";

/// The animations and sound 0x0051f3c0 names (strings @ 0x005b7a94,
/// 0x005b7af4, 0x005b7b48).
const CLOSE: [(&str, &str); 2] = [
    ("portraitClosePlancheHaut", "plancheHaut"),
    ("portraitClosePlancheBas", "plancheBas"),
];
const CLOSE_SOUND: &str = "portraitCloseSound";

/// The planks closing over the game.
pub struct Planks {
    gui: Gui,
    root: Option<Id>,
    anims: Vec<Id>,
    pictures: HashMap<String, Option<Picture>>,
    /// The closing finished: the "closed" signal fired.
    pub closed: bool,
}

struct Pics<'a>(&'a HashMap<String, Option<Picture>>);

impl Painter for Pics<'_> {
    fn picture(&self, path: &str) -> Option<&Picture> {
        self.0.get(path).and_then(Option::as_ref)
    }

    fn video(&self, _: &Gui, _: Id, _: &mut dyn Canvas, _: Rect) -> bool {
        false
    }
}

/// The sound the layout file names (`portraitCloseSound`), read in a
/// layout state as `TeLuaGUI` reads a value (0x00549580).
fn close_sound(files: &A4Files) -> Option<String> {
    let path = gui::resolve(files, ROTATION_LUA)?;
    let engine: EngineRef = std::rc::Rc::new(std::cell::RefCell::new(Reader(files.clone())));
    let mut state = GuiState::new(&engine, Options::default()).ok()?;
    state.run_file(path.as_bytes());
    match state.read(CLOSE_SOUND) {
        Ok(Some(BootValue::Str(s))) => Some(s.iter().map(|&c| char::from(c)).collect()),
        _ => None,
    }
}

/// A layout state that only reads files (its constructors' tables are
/// not kept).
struct Reader(A4Files);

impl atlantis4_script::Engine for Reader {
    fn call(&mut self, _: &atlantis4_script::Call) -> atlantis4_script::Reply {
        atlantis4_script::Reply::None
    }

    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        self.0.read(&String::from_utf8_lossy(path))
    }
}

impl Planks {
    /// Starts the closing at `now` ms; the sound to play with it.
    pub fn close(files: &A4Files, now: f64) -> (Planks, Option<String>) {
        let mut gui = Gui::load(files, ROTATION_LUA).unwrap_or_default();
        let root = gui.find("rotationAnim");
        let mut anims = Vec::new();
        for (a, target) in CLOSE {
            if let (Some(a), Some(t)) = (gui.find(a), gui.find(target)) {
                gui.bind(a, t);
                gui.play(a, now);
                anims.push(a);
            } else {
                log::warn!("{ROTATION_LUA}: no {a} or {target}");
            }
        }
        let planks = Planks {
            gui,
            root,
            anims,
            pictures: HashMap::new(),
            closed: false,
        };
        (planks, close_sound(files))
    }

    /// The frame's update at `now` ms; true when the closing just ended.
    pub fn update(&mut self, now: f64) -> bool {
        if self.closed {
            return false;
        }
        self.gui.tick(now);
        if self.anims.iter().any(|&a| self.gui.playing(a)) {
            return false;
        }
        self.closed = true;
        true
    }

    /// Draws the planks over the frame.
    pub fn draw_on(&mut self, files: &A4Files, c: &mut dyn Canvas) {
        let (width, height) = c.size();
        let Some(r) = self.root else { return };
        self.gui
            .update(r, (width as f32, height as f32), (0.0, 0.0), None);
        let mut paths = Vec::new();
        self.gui.pictures(r, &mut paths);
        for p in paths {
            self.pictures
                .entry(p.clone())
                .or_insert_with(|| gui::load_picture(files, &p));
        }
        self.gui.draw_on(r, c, &Pics(&self.pictures));
    }

    /// What the frame shows, for the frame key.
    pub fn key(&self, h: &mut impl std::hash::Hasher) {
        if let Some(r) = self.root {
            self.gui.hash_into(r, h);
        }
    }
}
