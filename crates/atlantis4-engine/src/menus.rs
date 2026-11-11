//! The front end of atlantis4-gog-en-2363910: the splash screens, the
//! main menu, the confirmations, the options, the credits, the load
//! screen and the planks' transition (`RotationAnim`), as `Application`
//! runs them. Every layout comes from the install's files through
//! [`crate::gui`]; the screens' handlers below are the executable's.
//!
//! **Start-up** (`Application::create` 0x0057ad20, `STATIC`): the texts,
//! `options.xml` (0x00577530), the cursor, then `RotationAnim::load`
//! (0x00520660: `GUI/RotationAnim.lua`, its `rotationAnim` hidden, the
//! planks' `onFinished` connected) and `SplashScreens::enter` inlined
//! (`splashNo = 0`, `onAlarm` 0x00510e60). `onAlarm` unloads the shown
//! splash, then loads `GUI/Splash<n>.lua` (found in `GUI/PC-MacOSX/`:
//! `Splash0` Microïds, `Splash1` Koalabs), adds its `splash` layout to the
//! window and sets the alarm 1,500,000 µs later; when the file is missing
//! (`Splash2`) it enters the main menu (`App+0x1090`, 0x00566000). No
//! click or key ends a splash (no handler is connected: the only callback
//! of `SplashScreens` is the alarm, RTTI `TeCallback0Param<SplashScreens>`
//! connected @ 0x005113a0) and nothing fades.
//!
//! **Main menu** (`MainMenu::enter` 0x00566000): `GUI/MainMenu.lua`'s
//! `menu` inserted under the planks; its `musicPath`
//! (`Sounds/Musiques/xfiles.ogg`) loaded, played and set to volume 1 when
//! the music is not playing or plays another path; `newGameButton` →
//! 0x00565af0 (with `Backup/save.xml` present the `ConfirmNewGame.lua`
//! confirmation, else a new game), `continueGameButton` → 0x00565230
//! (enabled only when `Backup/save.xml` exists, else disabled: its down
//! picture), `loadGameButton` → the load screen (0x00565f90),
//! `optionsButton` → the options (0x00565db0), `quitButton` → the
//! window's quit flag (0x005651b0, `TeMainWindow+0x548`), `facebookButton`
//! hidden; `versionNumber` shows `versionString` (`Texts/Version.lua`)
//! after a left-aligned white `Arial_r_16` format (@ 0x005b6e08). A new
//! game (0x005655c0) deletes `save.xml` and `0.xml` .. `10.xml`, then runs
//! "continue": every button disabled, the planks close (landscape); at
//! their turn (`App+0xda8`, 0x00565e20) the menu leaves and the game
//! starts from `save.xml` (`Game::enter`, `Game::load("save.xml")`: a new
//! game when it is missing).
//!
//! **Planks** (`RotationAnim` @ `App+0x6b0`): close (0x0051fc10
//! landscape, 0x0051f3c0 portrait): `rotationAnim` shown, the
//! `<kind>ClosePlanche{Haut,Bas}` animations bound to `plancheHaut` /
//! `plancheBas` and played, `<kind>CloseSound` played at 1.0; when
//! `ClosePlancheBas` ends (0x0051e900) the rotate animations (the planks
//! still, `center` turned 0 → 90° or back) and `<kind>RotateSound`; when
//! `RotatePlancheBas` ends (0x0051e000) the kind's signal is emitted (the
//! screens' switch, behind the closed planks: `+0x6f8` landscape,
//! `+0x700` portrait) then the open animations and `<kind>OpenSound`;
//! when `OpenPlancheBas` ends (0x0051b920) `rotationAnim` is hidden. Each
//! step lasts 1000 ms with the ease-out curve. The rotate step also sets
//! a device orientation (`0x00605530+0x4cc`, a quaternion of −90° about
//! z) with no effect on this platform's picture `[INFERRED]`.
//!
//! **Options** (`OptionsMenu::enter` 0x00517230): `GUI/OptionsMenu.lua`
//! (`GUI/HD/`), `menu2`; music as the main menu (`mus_0022.ogg`, absent
//! from the install: the music stops); `quitButton` → the main menu,
//! `creditsButton` → the credits; the check boxes `permanentHelp` (no
//! picture in this file), `inverseLook` and `compassLook` set active when
//! their flag is set, then `compassLook` set "unactive disabled" (state 3,
//! no layout: nothing shows) whatever its flag; a check box's change sets
//! its flag to `state == 0 || state == 2`; the eight volume buttons
//! ([`crate::options`]) and the four rows' sprites refreshed. The
//! `dialog` and `video` buttons also replay a test file on their channel
//! (`PG813A01.ogg`, absent: silent).
//!
//! **Credits** (`Credits::enter` 0x005824d0): `GUI/Credits.lua`, music
//! `M_Credits.ogg`, `scrollTextPositionAnim` and `scrollTextAnchorAnim`
//! bound to `text` and played (30 s, linear); `quitButton2` or the
//! position animation's end → the options (0x00582420, 0x00582450).
//!
//! **Load screen** (`LoadingMenu::enter` 0x0051a2b0): `GUI/LoadingMenu.lua`;
//! `quitButton` → the main menu (0x0051a280); the file's six buttons
//! `"0"`..`"5"` become save buttons (0x005185b0: `Backup/<i>.xml`,
//! enabled when it exists, else disabled: `Frame.png`), sorted by
//! played time ([`crate::saves::chapter_order`]) into `slot0`..`slot5`;
//! `text` shows the current button's `Save<i>` text, which no language
//! file has (`[CONFIRMED-DATA]`: always empty); `leftButton` (0x00519480)
//! / `rightButton` (0x00518ee0) slide `slots` by `slideTranslation`
//! (0.4) with `slideAnimation` (250 ms) when it is not playing, the index
//! kept in `[0, 6]` (right while it is below the count, so one step past
//! the last); `slideButton` (full screen, on top) lets the clicks through
//! once pressed (`+0x580 = 1` @ 0x00518e72) and turns a drag of more than
//! 0.1 of its width into a slide (0x00519a20); a save button's click
//! (0x00518460) keeps `<i>.xml` and shows `ConfirmLoadGame.lua`, whose
//! "yes" (0x00519e20) closes the planks; at their turn (0x00519f40) the
//! menu leaves and `Game::load("<i>.xml")` runs.
//!
//! **In game** (`InventoryMenu`'s `mainMenuButton` 0x0057cc50):
//! `Game::save("save.xml")`, the portrait planks; at their turn
//! (0x00526910) `Game::leave` and the main menu.
//!
//! `[UNKNOWN]`, implemented narrowly: whether the very first press on
//! the load screen is taken by `slideButton` before it lets clicks
//! through (taken: through from the start); the divisor of the swipe
//! (taken: the window's width); the picture of a turned layout (see
//! [`crate::gui::draw_rotated`]).

use std::collections::HashMap;

use atlantis4_data::A4Files;

use crate::canvas::{Canvas, CpuCache, CpuCanvas};
use crate::gui::text::FontCache;
use crate::gui::{self, Class, Gui, Id, Painter, State};
use crate::layout::Rect;
use crate::options::{self, Settings};
use crate::package::Picture;
use crate::render::Weights;
use crate::saves::{self, SaveStore};

pub const MAIN_MENU_LUA: &str = "GUI/MainMenu.lua";
pub const CONFIRM_NEW_LUA: &str = "GUI/ConfirmNewGame.lua";
pub const CONFIRM_LOAD_LUA: &str = "GUI/ConfirmLoadGame.lua";
pub const OPTIONS_LUA: &str = "GUI/OptionsMenu.lua";
pub const CREDITS_LUA: &str = "GUI/Credits.lua";
pub const LOADING_LUA: &str = "GUI/LoadingMenu.lua";
pub const ROTATION_LUA: &str = "GUI/RotationAnim.lua";
/// The splash files' pattern (`"GUI/Splash"` + `%d` + `".lua"`, 0x00510eea).
pub fn splash_lua(n: u32) -> String {
    format!("GUI/Splash{n}.lua")
}
/// A splash's time, µs (`0x16e360` @ 0x005112ac).
pub const SPLASH_US: u64 = 1_500_000;
/// The version text's format (@ 0x005b6e08).
pub const VERSION_FORMAT: &str = "<section style=\"left\" /><color r=\"255\" g=\"255\" b=\"255\"/><font file=\"Common/Fonts/Arial_r_16.tef\" />";
/// The main menu's buttons, in `MainMenu::enter`'s order.
pub const MAIN_BUTTONS: [&str; 5] = [
    "newGameButton",
    "continueGameButton",
    "loadGameButton",
    "optionsButton",
    "quitButton",
];
/// The options' volume rows: the buttons' and sprites' prefix, the channel.
pub const VOLUME_ROWS: [(&str, &str); 4] = [
    ("sfx", "sfx"),
    ("music", "music"),
    ("dialog", "dialog"),
    ("video", "video"),
];
/// The swipe's threshold (0x0051`9bbe`: ±0.1 of the width).
pub const SWIPE: f64 = 0.1;

/// What the front end asks the application to do.
#[derive(Clone, Debug, PartialEq)]
pub enum FrontAction {
    /// Play a file once at volume 1.0 (`playFreeSound`).
    Sound(String),
    /// A menu's `musicPath`: start it unless it is the music playing.
    Music(String),
    /// `setChannelVolume`.
    Volume(String, f32),
    /// Stop and replay a test file on a channel (the options' rows).
    TestSound(String, String),
    /// The planks' turn of a "continue", new game or load: `Game::enter`
    /// then `Game::load` of this backup file.
    StartGame(String),
    /// The portrait planks' turn from the game: `Game::leave` (the main
    /// menu follows by itself).
    LeaveGame,
    /// The settings changed (a check box, a volume): to keep.
    Settings,
    /// The window's quit flag.
    Quit,
}

/// The screen entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Splash,
    Main,
    Options,
    Credits,
    Loading,
    /// The game (nothing of the front end but the planks).
    Game,
}

/// The planks' kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Landscape,
    Portrait,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Landscape => "landscape",
            Kind::Portrait => "portrait",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Close,
    Rotate,
    Open,
}

/// What the planks' turn does.
#[derive(Clone, Debug, PartialEq)]
enum Turn {
    Start(String),
    ToMainMenu,
}

/// A loaded screen file and its root.
struct Screenful {
    gui: Gui,
    root: Id,
}

impl Screenful {
    fn load(files: &A4Files, path: &str, root: &str) -> Option<Screenful> {
        let gui = Gui::load(files, path)?;
        let Some(root) = gui.find(root) else {
            log::warn!("{path}: no {root}");
            return None;
        };
        Some(Screenful { gui, root })
    }

    fn find(&self, name: &str) -> Option<Id> {
        self.gui.find(name)
    }

    fn set_visible(&mut self, name: &str, v: bool) {
        if let Some(i) = self.find(name) {
            self.gui.node_mut(i).visible = v;
        }
    }

    /// `enable` / the state set as the handlers do (`+0x57c` then
    /// `setState`).
    fn set_button(&mut self, name: &str, enabled: bool) {
        if let Some(i) = self.find(name) {
            let n = self.gui.node_mut(i);
            n.enabled = enabled;
            n.state = if enabled { State::Up } else { State::Disabled };
        }
    }

    fn sound_of(&self, id: Id) -> Option<FrontAction> {
        self.gui
            .node(id)
            .validation_sound
            .clone()
            .map(FrontAction::Sound)
    }
}

/// The load screen's state.
struct Loading {
    s: Screenful,
    /// The save buttons by index in the slots (the file's button ids).
    order: Vec<usize>,
    buttons: Vec<Id>,
    /// `+0x378`.
    index: usize,
    /// Where `slideButton` was pressed (`+0x37c`).
    press_x: Option<f64>,
    /// The save chosen (`+0x388`).
    chosen: Option<String>,
}

/// The front end.
pub struct Front {
    files: A4Files,
    lang: String,
    pictures: HashMap<String, Option<Picture>>,
    fonts: FontCache,
    pub settings: Settings,
    pub screen: Screen,
    /// `versionString` (`App+0x8774`).
    pub version: String,
    splash_no: u32,
    splash: Option<Screenful>,
    splash_deadline: Option<f64>,
    main: Option<Screenful>,
    confirm: Option<Screenful>,
    confirm_shown: bool,
    options: Option<Screenful>,
    credits: Option<Screenful>,
    loading: Option<Loading>,
    rotation: Option<Screenful>,
    step: Option<(Kind, Step)>,
    turn: Option<Turn>,
    /// The layer the press went to: 0 splash, 1 planks, 2 confirm, 3
    /// screen.
    press: Option<u8>,
    actions: Vec<FrontAction>,
}

/// The front end's pictures for [`Gui::draw`].
struct Pics<'a>(&'a HashMap<String, Option<Picture>>);

impl Painter for Pics<'_> {
    fn picture(&self, path: &str) -> Option<&Picture> {
        self.0.get(path).and_then(Option::as_ref)
    }

    fn video(&self, _: &Gui, _: Id, _: &mut dyn Canvas, _: Rect) -> bool {
        false
    }
}

impl Front {
    /// `Application::create`'s part: the planks loaded and hidden, the
    /// settings read; [`Front::start`] begins the splashes.
    pub fn new(files: &A4Files, settings: Settings, version: String) -> Front {
        let lang = files.language();
        let mut rotation = Screenful::load(files, ROTATION_LUA, "rotationAnim");
        if let Some(r) = rotation.as_mut() {
            let root = r.root;
            r.gui.node_mut(root).visible = false;
        }
        Front {
            files: files.clone(),
            lang,
            pictures: HashMap::new(),
            fonts: FontCache::default(),
            settings,
            screen: Screen::Game,
            version,
            splash_no: 0,
            splash: None,
            splash_deadline: None,
            main: None,
            confirm: None,
            confirm_shown: false,
            options: None,
            credits: None,
            loading: None,
            rotation,
            step: None,
            turn: None,
            press: None,
            actions: Vec::new(),
        }
    }

    /// `SplashScreens::enter` (inlined @ 0x0057bc45): splash 0 at `now`.
    pub fn start(&mut self, now: f64, saves: &dyn SaveStore) {
        self.splash_no = 0;
        self.screen = Screen::Splash;
        self.on_alarm(now, saves);
    }

    /// The actions raised since the last call, in order.
    pub fn take_actions(&mut self) -> Vec<FrontAction> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the front end covers the game (a menu entered).
    pub fn active(&self) -> bool {
        self.screen != Screen::Game
    }

    /// Whether the planks are moving or shown.
    pub fn in_transition(&self) -> bool {
        self.step.is_some()
    }

    /// `SplashScreens::onAlarm` (0x00510e60).
    fn on_alarm(&mut self, now: f64, saves: &dyn SaveStore) {
        self.splash = None;
        self.splash_deadline = None;
        let path = splash_lua(self.splash_no);
        self.splash_no += 1;
        if gui::resolve(&self.files, &path).is_none() {
            self.enter_main(saves);
            return;
        }
        self.splash = Screenful::load(&self.files, &path, "splash");
        self.splash_deadline = Some(now + SPLASH_US as f64 / 1000.0);
    }

    fn music(&mut self, s: Option<&Screenful>) {
        if let Some(p) = s.and_then(|s| s.gui.string("musicPath")) {
            self.actions.push(FrontAction::Music(p));
        }
    }

    /// `MainMenu::enter` (0x00566000).
    pub fn enter_main(&mut self, saves: &dyn SaveStore) {
        self.screen = Screen::Main;
        let mut m = Screenful::load(&self.files, MAIN_MENU_LUA, "menu");
        self.music(m.as_ref());
        if let Some(m) = m.as_mut() {
            m.set_visible("facebookButton", false);
            let exists = saves.exists(saves::AUTOSAVE);
            m.set_button("continueGameButton", exists);
            if let Some(t) = m.find("versionNumber") {
                let text = format!("{VERSION_FORMAT}{}", self.version);
                m.gui.set_text(t, text.into_bytes(), 0.0);
            }
        }
        self.main = m;
        self.confirm = Screenful::load(&self.files, CONFIRM_NEW_LUA, "confirm");
        self.confirm_shown = false;
    }

    /// `MainMenu::leave` (inlined in its handlers).
    fn leave_main(&mut self) {
        self.main = None;
        self.confirm = None;
        self.confirm_shown = false;
    }

    /// "Continue" (0x00565230): the buttons disabled, the landscape planks.
    fn continue_game(&mut self, now: f64) {
        if let Some(m) = self.main.as_mut() {
            for b in MAIN_BUTTONS {
                m.set_button(b, false);
            }
        }
        self.close(
            Kind::Landscape,
            Turn::Start(saves::AUTOSAVE.to_string()),
            now,
        );
    }

    /// "New game" (0x005655c0): the saves deleted, then "continue".
    fn new_game(&mut self, now: f64, saves: &mut dyn SaveStore) {
        for n in saves::NEW_GAME_REMOVES {
            saves.delete(n);
        }
        self.continue_game(now);
    }

    /// `OptionsMenu::enter` (0x00517230).
    pub fn enter_options(&mut self) {
        self.screen = Screen::Options;
        let mut o = Screenful::load(&self.files, OPTIONS_LUA, "menu2");
        self.music(o.as_ref());
        if let Some(o) = o.as_mut() {
            let set = |o: &mut Screenful, name: &str, check: u8| {
                if let Some(i) = o.find(name) {
                    o.gui.node_mut(i).check = check;
                }
            };
            if self.settings.permanent_help {
                set(o, "permanentHelp", 0);
            }
            if self.settings.inverse_look {
                set(o, "inverseLook", 0);
            }
            if self.settings.compass_look {
                set(o, "compassLook", 0);
            }
            set(o, "compassLook", 3);
        }
        self.options = o;
        for (row, ch) in VOLUME_ROWS {
            self.refresh_row(row, ch);
        }
    }

    /// The row's sprites (0x00511490 and twins).
    fn refresh_row(&mut self, row: &str, channel: &str) {
        let v = self.settings.volume(channel);
        let Some(o) = self.options.as_mut() else {
            return;
        };
        let count = row_count(&o.gui, row);
        if count == 0 {
            return;
        }
        let step = options::step(count);
        for i in 0..count {
            let on = options::level_shown(i, step, v);
            o.set_visible(&format!("{row}VolumeSprite{i}"), on);
            o.set_visible(&format!("{row}VolumeSprite{i}Off"), !on);
        }
    }

    /// A volume button (0x00514050 and twins).
    fn volume_button(&mut self, row: &str, channel: &str, up: bool) {
        let Some(o) = self.options.as_ref() else {
            return;
        };
        let count = row_count(&o.gui, row);
        if count == 0 {
            return;
        }
        let step = options::step(count);
        let v = self.settings.volume(channel);
        let v = if up {
            options::step_up(v, step)
        } else {
            options::step_down(v, step)
        };
        self.settings.set_volume(channel, v);
        self.actions
            .push(FrontAction::Volume(channel.to_string(), v));
        let test = match channel {
            "dialog" => o.gui.string("dialogTestPath"),
            "video" => o.gui.string("videoTestPath"),
            _ => None,
        };
        if let Some(p) = test {
            self.actions
                .push(FrontAction::TestSound(channel.to_string(), p));
        }
        self.actions.push(FrontAction::Settings);
        self.refresh_row(row, channel);
    }

    /// `Credits::enter` (0x005824d0).
    fn enter_credits(&mut self, now: f64) {
        self.screen = Screen::Credits;
        let mut c = Screenful::load(&self.files, CREDITS_LUA, "menu");
        self.music(c.as_ref());
        if let Some(c) = c.as_mut() {
            if let Some(t) = c.find("text") {
                for a in ["scrollTextPositionAnim", "scrollTextAnchorAnim"] {
                    if let Some(a) = c.find(a) {
                        c.gui.bind(a, t);
                        c.gui.play(a, now);
                    }
                }
            }
        }
        self.credits = c;
    }

    /// `LoadingMenu::enter` (0x0051a2b0).
    fn enter_loading(&mut self, saves: &dyn SaveStore, now: f64) {
        self.screen = Screen::Loading;
        let Some(mut s) = Screenful::load(&self.files, LOADING_LUA, "menu") else {
            return;
        };
        let mut ids = Vec::new();
        for i in 0.. {
            let Some(b) = s
                .gui
                .roots()
                .into_iter()
                .find(|&r| s.gui.node(r).name == i.to_string())
            else {
                break;
            };
            let exists = saves.exists(&format!("{i}.xml"));
            let n = s.gui.node_mut(b);
            n.enabled = exists;
            n.state = if exists { State::Up } else { State::Disabled };
            ids.push(b);
        }
        let order = saves::chapter_order(saves, ids.len());
        for (k, &j) in order.iter().enumerate() {
            if let Some(slot) = s.find(&format!("slot{k}")) {
                s.gui.reparent(ids[j], slot);
            }
        }
        // `text`: the button's `Save<i>` text, in no language file.
        if let Some(t) = s.find("text") {
            s.gui.set_text(t, Vec::new(), now);
        }
        self.loading = Some(Loading {
            s,
            order,
            buttons: ids,
            index: 0,
            press_x: None,
            chosen: None,
        });
        self.confirm = Screenful::load(&self.files, CONFIRM_LOAD_LUA, "confirm");
        self.confirm_shown = false;
    }

    /// A slide of the load screen (0x00519480 left, 0x00518ee0 right).
    fn slide(&mut self, right: bool, now: f64) {
        let Some(l) = self.loading.as_mut() else {
            return;
        };
        let (Some(anim), Some(slots)) = (l.s.find("slideAnimation"), l.s.find("slots")) else {
            return;
        };
        if l.s.gui.playing(anim) {
            return;
        }
        if right && l.index >= l.buttons.len() || !right && l.index == 0 {
            return;
        }
        let t =
            l.s.gui
                .numbers
                .get("slideTranslation")
                .copied()
                .unwrap_or(0.0) as f32;
        let p = l.s.gui.node(slots).position;
        let end = if right {
            [p[0] - t, p[1] - 0.0, p[2] - 0.0, 0.0]
        } else {
            [p[0] + t, p[1] + 0.0, p[2] + 0.0, 0.0]
        };
        if let Some(a) = l.s.gui.node_mut(anim).anim.as_mut() {
            a.start_value = [p[0], p[1], p[2], 0.0];
            a.end_value = end;
        }
        l.s.gui.bind(anim, slots);
        l.s.gui.play(anim, now);
        if right {
            l.index += 1;
        } else {
            l.index -= 1;
        }
    }

    /// The planks close (0x0051fc10, 0x0051f3c0).
    fn close(&mut self, kind: Kind, turn: Turn, now: f64) {
        self.turn = Some(turn);
        self.run_step(kind, Step::Close, now);
    }

    /// From the game to the main menu (`mainMenuButton` 0x0057cc50, after
    /// the application saved `save.xml`): the portrait planks.
    pub fn to_main_menu(&mut self, now: f64) {
        self.close(Kind::Portrait, Turn::ToMainMenu, now);
    }

    fn run_step(&mut self, kind: Kind, step: Step, now: f64) {
        let Some(r) = self.rotation.as_mut() else {
            // No planks: the turn at once.
            self.step = None;
            if let Some(t) = self.turn.take() {
                self.pending_turn(t);
            }
            return;
        };
        let k = kind.prefix();
        let (what, binds): (&str, &[(&str, &str)]) = match step {
            Step::Close => (
                "Close",
                &[
                    ("ClosePlancheHaut", "plancheHaut"),
                    ("ClosePlancheBas", "plancheBas"),
                ],
            ),
            Step::Rotate => (
                "Rotate",
                &[
                    ("RotatePlancheHaut", "plancheHaut"),
                    ("RotateCenter", "center"),
                    ("RotatePlancheBas", "plancheBas"),
                ],
            ),
            Step::Open => (
                "Open",
                &[
                    ("OpenPlancheHaut", "plancheHaut"),
                    ("OpenPlancheBas", "plancheBas"),
                ],
            ),
        };
        if step == Step::Close {
            let root = r.root;
            r.gui.node_mut(root).visible = true;
        }
        for (anim, target) in binds {
            if let (Some(a), Some(t)) = (r.find(&format!("{k}{anim}")), r.find(target)) {
                r.gui.bind(a, t);
                r.gui.play(a, now);
            }
        }
        let sound = r.gui.string(&format!("{k}{what}Sound"));
        self.step = Some((kind, step));
        if let Some(s) = sound {
            self.actions.push(FrontAction::Sound(s));
        }
    }

    fn pending_turn(&mut self, t: Turn) {
        match t {
            Turn::Start(name) => {
                self.leave_main();
                self.loading = None;
                self.confirm = None;
                self.confirm_shown = false;
                self.screen = Screen::Game;
                self.actions.push(FrontAction::StartGame(name));
            }
            Turn::ToMainMenu => self.actions.push(FrontAction::LeaveGame),
        }
    }

    /// The frame's update at `now` ms: the splash alarm, the animations
    /// and what their ends lead to.
    pub fn tick(&mut self, now: f64, saves: &mut dyn SaveStore) {
        if self.splash_deadline.is_some_and(|d| now >= d) {
            self.on_alarm(now, saves);
        }
        if let Some(c) = self.credits.as_mut() {
            let done = c.gui.tick(now);
            let pos = c.find("scrollTextPositionAnim");
            if done.iter().any(|&a| Some(a) == pos) {
                self.credits = None;
                self.enter_options();
            }
        }
        if let Some(l) = self.loading.as_mut() {
            l.s.gui.tick(now);
        }
        let Some(r) = self.rotation.as_mut() else {
            return;
        };
        let done = r.gui.tick(now);
        let Some((kind, step)) = self.step else {
            return;
        };
        let k = kind.prefix();
        let last = |what: &str| r.find(&format!("{k}{what}PlancheBas"));
        let ended = |what: &str| last(what).is_some_and(|a| done.contains(&a));
        match step {
            Step::Close if ended("Close") => self.run_step(kind, Step::Rotate, now),
            Step::Rotate if ended("Rotate") => {
                if let Some(t) = self.turn.take() {
                    self.pending_turn(t);
                }
                self.run_step(kind, Step::Open, now);
            }
            Step::Open if ended("Open") => {
                let root = r.root;
                r.gui.node_mut(root).visible = false;
                self.step = None;
            }
            _ => {}
        }
    }

    /// The game left (the portrait turn): the main menu enters.
    pub fn game_left(&mut self, saves: &dyn SaveStore) {
        self.enter_main(saves);
    }

    fn layers(&mut self) -> Vec<(u8, &mut Screenful)> {
        let mut out: Vec<(u8, &mut Screenful)> = Vec::new();
        if let Some(s) = self.splash.as_mut() {
            out.push((0, s));
        }
        if let Some(r) = self.rotation.as_mut() {
            if r.gui.node(r.root).visible {
                out.push((1, r));
            }
        }
        if self.confirm_shown {
            if let Some(c) = self.confirm.as_mut() {
                out.push((2, c));
            }
        }
        let screen = match self.screen {
            Screen::Main => self.main.as_mut(),
            Screen::Options => self.options.as_mut(),
            Screen::Credits => self.credits.as_mut(),
            Screen::Loading => self.loading.as_mut().map(|l| &mut l.s),
            _ => None,
        };
        if let Some(s) = screen {
            out.push((3, s));
        }
        out
    }

    fn layout(&mut self, width: u32, height: u32) {
        let win = (width as f32, height as f32);
        let files = self.files.clone();
        let mut fonts = std::mem::take(&mut self.fonts);
        for (_, s) in self.layers() {
            let root = s.root;
            s.gui.update(root, win, (0.0, 0.0), None);
            s.gui.layout_texts(&files, &mut fonts);
            s.gui.update(root, win, (0.0, 0.0), None);
        }
        self.fonts = fonts;
    }

    /// A press: whether the front end took it (top layer first).
    pub fn press(&mut self, width: u32, height: u32, x: f64, y: f64) -> bool {
        self.layout(width, height);
        self.press = None;
        if let Some(l) = self.loading.as_mut() {
            if self.screen == Screen::Loading && !self.confirm_shown {
                if let Some(sb) = l.s.find("slideButton") {
                    // Clicks through, once pressed (0x00518e72).
                    l.s.gui.node_mut(sb).click_pass_through = true;
                    l.press_x = Some(x);
                }
            }
        }
        let mut took = None;
        for (layer, s) in self.layers() {
            let root = s.root;
            if s.gui.press(root, x, y, width, height) {
                took = Some(layer);
                break;
            }
            // A shown splash, planks or confirmation covers the window.
            if layer < 3 {
                took = Some(layer);
                break;
            }
        }
        self.press = took;
        took.is_some() || self.active()
    }

    /// The pointer moved (roll-overs; the load screen's swipe).
    pub fn hover(&mut self, width: u32, height: u32, x: f64, y: f64, held: bool, now: f64) {
        self.layout(width, height);
        let mut taken = false;
        for (_, s) in self.layers() {
            let root = s.root;
            s.gui.hover(root, x, y, width, height, &mut taken);
        }
        // 0x00519a20: a held press on `slideButton` past 0.1 of the width.
        let swipe = self.loading.as_ref().and_then(|l| {
            let x0 = l.press_x?;
            let anim = l.s.find("slideAnimation")?;
            if !held || l.s.gui.playing(anim) {
                return None;
            }
            let d = (x - x0) / f64::from(width.max(1));
            if d > SWIPE {
                Some(false)
            } else if d < -SWIPE {
                Some(true)
            } else {
                None
            }
        });
        if let Some(right) = swipe {
            self.slide(right, now);
            if let Some(l) = self.loading.as_mut() {
                l.press_x = Some(x);
            }
        }
    }

    /// A release: the validated click's handler.
    pub fn release(
        &mut self,
        width: u32,
        height: u32,
        x: f64,
        y: f64,
        now: f64,
        saves: &mut dyn SaveStore,
    ) -> bool {
        self.layout(width, height);
        if let Some(l) = self.loading.as_mut() {
            l.press_x = None;
        }
        let Some(layer) = self.press.take() else {
            return self.active();
        };
        let mut hit = None;
        for (l, s) in self.layers() {
            if l != layer {
                continue;
            }
            let root = s.root;
            let (_, b) = s.gui.release(root, x, y, width, height);
            hit = b.map(|b| {
                let n = s.gui.node(b);
                let sound = if n.class == Class::Checkbox && n.check != 0 {
                    // Made unactive: `unactivationSound` (0x004dd225).
                    n.unactivation_sound.clone().map(FrontAction::Sound)
                } else {
                    s.sound_of(b)
                };
                (n.name.clone(), sound, n.class)
            });
            break;
        }
        let Some((name, sound, class)) = hit else {
            return true;
        };
        if let Some(s) = sound {
            self.actions.push(s);
        }
        self.clicked(layer, &name, class, now, saves);
        true
    }

    fn clicked(
        &mut self,
        layer: u8,
        name: &str,
        class: Class,
        now: f64,
        saves: &mut dyn SaveStore,
    ) {
        if layer == 2 {
            // `Confirm`: "yes" and "no" hide it and emit their signal.
            self.confirm_shown = false;
            if name != "yes" {
                return;
            }
            match self.screen {
                Screen::Main => self.new_game(now, saves),
                Screen::Loading => {
                    let chosen = self.loading.as_ref().and_then(|l| l.chosen.clone());
                    if let Some(c) = chosen {
                        self.close(Kind::Landscape, Turn::Start(c), now);
                    }
                }
                _ => {}
            }
            return;
        }
        if layer != 3 {
            return;
        }
        match (self.screen, name) {
            (Screen::Main, "newGameButton") => {
                if saves.exists(saves::AUTOSAVE) {
                    self.confirm_shown = true;
                } else {
                    self.new_game(now, saves);
                }
            }
            (Screen::Main, "continueGameButton") => self.continue_game(now),
            (Screen::Main, "loadGameButton") => {
                self.leave_main();
                self.enter_loading(saves, now);
            }
            (Screen::Main, "optionsButton") => {
                self.leave_main();
                self.enter_options();
            }
            (Screen::Main, "quitButton") => self.actions.push(FrontAction::Quit),
            (Screen::Options, "quitButton") => {
                self.options = None;
                self.enter_main(saves);
            }
            (Screen::Options, "creditsButton") => {
                self.options = None;
                self.enter_credits(now);
            }
            (Screen::Options, n) if class == Class::Checkbox => {
                let check = self
                    .options
                    .as_ref()
                    .and_then(|o| o.find(n).map(|i| o.gui.node(i).check))
                    .unwrap_or(1);
                // 0x00511450, 0x00511410, 0x005113d0.
                let on = check == 0 || check == 2;
                match n {
                    "permanentHelp" => self.settings.permanent_help = on,
                    "inverseLook" => self.settings.inverse_look = on,
                    "compassLook" => self.settings.compass_look = on,
                    _ => return,
                }
                self.actions.push(FrontAction::Settings);
            }
            (Screen::Options, n) => {
                for (row, ch) in VOLUME_ROWS {
                    if n == format!("{row}VolumePlusButton") {
                        self.volume_button(row, ch, true);
                    } else if n == format!("{row}VolumeMinusButton") {
                        self.volume_button(row, ch, false);
                    }
                }
            }
            (Screen::Credits, "quitButton2") => {
                self.credits = None;
                self.enter_options();
            }
            (Screen::Loading, "quitButton") => {
                self.loading = None;
                self.confirm = None;
                self.enter_main(saves);
            }
            (Screen::Loading, "leftButton") => self.slide(false, now),
            (Screen::Loading, "rightButton") => self.slide(true, now),
            (Screen::Loading, n) => {
                // A save button (0x00518460): its file, the confirmation.
                if let Ok(i) = n.parse::<usize>() {
                    if let Some(l) = self.loading.as_mut() {
                        l.chosen = Some(format!("{i}.xml"));
                    }
                    self.confirm_shown = true;
                }
            }
            _ => {}
        }
    }

    /// Draws the front end's layers, bottom first: the screen, the
    /// confirmation, the planks, the splash.
    pub fn draw(&mut self, out: &mut [u32], width: u32, height: u32) {
        let mut cache = CpuCache::default();
        let mut c = CpuCanvas::new(out, width, height, &mut cache, Weights::Exact);
        self.draw_on(&mut c);
    }

    /// [`Front::draw`] on `c`.
    pub fn draw_on(&mut self, c: &mut dyn Canvas) {
        let (width, height) = c.size();
        self.layout(width, height);
        let mut paths = Vec::new();
        for (_, s) in self.layers() {
            s.gui.pictures(s.root, &mut paths);
        }
        for p in paths {
            self.picture(&p);
        }
        let Front {
            pictures,
            splash,
            rotation,
            confirm,
            confirm_shown,
            main,
            options,
            credits,
            loading,
            screen,
            ..
        } = self;
        let pics = Pics(pictures);
        let screen = match screen {
            Screen::Main => main.as_ref(),
            Screen::Options => options.as_ref(),
            Screen::Credits => credits.as_ref(),
            Screen::Loading => loading.as_ref().map(|l| &l.s),
            _ => None,
        };
        let mut layers: Vec<&Screenful> = screen.into_iter().collect();
        if *confirm_shown {
            layers.extend(confirm.as_ref());
        }
        if let Some(r) = rotation.as_ref().filter(|r| r.gui.node(r.root).visible) {
            layers.push(r);
        }
        layers.extend(splash.as_ref());
        for s in layers {
            s.gui.draw_on(s.root, c, &pics);
        }
    }

    /// A digest of what [`Front::draw`] shows.
    pub fn key(&mut self, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        (self.screen as u8, self.confirm_shown).hash(h);
        for (l, s) in self.layers() {
            l.hash(h);
            s.gui.hash_into(s.root, h);
        }
    }

    fn picture(&mut self, path: &str) {
        if self.pictures.contains_key(path) {
            return;
        }
        let pic = gui::resolve_in(&self.files, path, &self.lang)
            .and_then(|r| self.files.read(&r))
            .and_then(|d| gui::decode_picture(&d));
        if pic.is_none() {
            log::warn!("no picture {path}");
        }
        self.pictures.insert(path.to_string(), pic);
    }

    /// The rectangle of a layout of the entered screen (tests, tools).
    pub fn rect_of(&mut self, name: &str, width: u32, height: u32) -> Option<Rect> {
        self.layout(width, height);
        self.layers()
            .into_iter()
            .find_map(|(_, s)| s.find(name).map(|i| s.gui.rect(i, width, height)))
    }

    /// Whether a layout of the shown layers is visible (tests, tools).
    pub fn shown(&mut self, name: &str) -> Option<bool> {
        self.layers()
            .into_iter()
            .find_map(|(_, s)| s.find(name).map(|i| s.gui.shown(i)))
    }

    /// The state of a button of the shown layers (tests, tools).
    pub fn button_state(&mut self, name: &str) -> Option<State> {
        self.layers()
            .into_iter()
            .find_map(|(_, s)| s.find(name).map(|i| s.gui.node(i).state))
    }

    /// The load screen's slot order and index (tests, tools).
    pub fn loading_state(&self) -> Option<(Vec<usize>, usize)> {
        self.loading.as_ref().map(|l| (l.order.clone(), l.index))
    }

    /// Whether the confirmation shows.
    pub fn confirm_shown(&self) -> bool {
        self.confirm_shown
    }
}

/// The count of `<row>VolumeSprite<i>` layouts (0x00514090..0x0051420c:
/// up to the first missing).
fn row_count(g: &Gui, row: &str) -> usize {
    (0..)
        .take_while(|i| g.find(&format!("{row}VolumeSprite{i}")).is_some())
        .count()
}
