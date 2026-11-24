//! *Atlantis V*'s menus (`CMenuManager`, ctor @ 0x0041b170) other than the
//! subject menu and the inventory: the main menu, the credits, the in-game
//! menu, the load and save pages, the three option pages, the death panel
//! and the examine view.
//!
//! Addresses are atlantis5-dvd-int-3891200's (the international DVD's
//! program; the French DVD's is encrypted and not read).
//!
//! - Every menu (`CMenu` ctor @ 0x0040d680) has a picture element (its
//!   `background.tga`, a `CIE_Picture` @ 0x0044fc70, at (0, 0)) as the
//!   parent of its elements, and a cursor element (`sprites2d/menu/
//!   cursor.spr`, path 20) drawn at the mouse (+0x0c, +0x10, kept within
//!   0..=W, 0..=H by @ 0x0040d890, the screen's centre when the menu
//!   opens, @ 0x0041b780). Its process (@ 0x0040d920) gives each event to
//!   the elements, then places the cursor; its drawing (@ 0x0040d9d0)
//!   draws the background with its elements, then the cursor.
//! - A text (`CIE_Text` @ 0x00451170): a font of `font.txt`, a colour
//!   (a `COLORREF`, +0x7c), the text, its place (+0x1c, +0x20, relative
//!   to its parent, @ 0x004520a0) and width (+0x24); its height (+0x28)
//!   is the text's at that width (@ 0x00461800). Drawn (@ 0x00451600) in
//!   the box (x, y, x + w, y + h) with the alignment +0x80 (@ 0x00451790:
//!   0 left, 1 centred, 2 right; the layout codes 0, 2, 1), top.
//! - A text button (`CIE_TextButton` @ 0x0044e9d0): a text with a second
//!   colour; +0x8c is 0 (drawn in +0x84) or 1 under the mouse (+0x88)
//!   (@ 0x0044ec10). A mouse move over its box (both ends included,
//!   @ 0x00452020) makes it 1; a press of the left button while it is 1
//!   is its click (event 2, @ 0x0044eb60), which calls the menu's handler
//!   with the element's name.
//! - A sprite button (`CIE_Button` @ 0x0044ee40): two `.spr` files,
//!   `button_click` and `button_focus` (arguments 3 and 4, kept at +0x48
//!   and +0x60 @ 0x0044ee9f..0x0044eeb8; the credits push them in that
//!   order @ 0x0040de60..0x0040de75);
//!   state +0x98 0: the first's frame 0; 1 (the mouse over it): the
//!   second played in a loop; 2 (pressed): the first played once; when
//!   it ends, the click (@ 0x0044f0e0, @ 0x0044f210, @ 0x0044f300). Both
//!   at 15 frames a second (0x402e0000) from frame 0 to the last.
//! - The menus' handlers compare the clicked element's name with their
//!   own (each menu's @ address is on its builder below).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use atlantis3_data::atlantis5_texts::StringResources;
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis_data::FileSource;

use crate::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::atlantis5_dialog::Fonts;
use crate::text::Zone;

/// The menu modes (the manager's slots, @ 0x0041b170).
pub const MAIN: u32 = 0;
pub const CREDITS: u32 = 1;
pub const IN_GAME: u32 = 2;
pub const LOAD: u32 = 3;
pub const SAVE: u32 = 4;
pub const GRAPHIC: u32 = 5;
pub const SOUND: u32 = 6;
pub const CONTROL: u32 = 7;
pub const INVENTORY: u32 = 8;
pub const SUBJECTS: u32 = 9;
pub const DEATH: u32 = 10;
pub const EXAMINE: u32 = 11;
pub const POKER: u32 = 12;

/// The screen (the 3D view's rectangle, 0x006b0b28).
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
const W: i32 = VIEW_WIDTH as i32;
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
const H: i32 = VIEW_HEIGHT as i32;

/// Whether a mode hides the 3D view (@ 0x0041b780: below 8, or above 9
/// but 12: @ 0x0044d1d0(1)).
pub fn hides_view(mode: u32) -> bool {
    mode < 8 || (mode > 9 && mode != POKER)
}

/// Whether a mode pauses the music channels 0..3 (@ 0x0045b430) and
/// stops the sounds (@ 0x00459740): all but 8, 9, 11, 12.
pub fn pauses_sound(mode: u32) -> bool {
    !matches!(mode, INVENTORY | SUBJECTS | EXAMINE | POKER)
}

/// `CConfiguration` (ctor @ 0x00429560, defaults @ 0x00429620): what the
/// option pages change and the save's `CONFIGURATION` holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// +0x14, `screen_size`: 0 for 4/3, 1 for 16/9 (the graphic page's
    /// label, @ 0x0041e8b0); default 0.
    pub screen_size: i32,
    /// +0x18, `graphic_quality`: default 1.
    pub graphic_quality: i32,
    /// +0x1c..+0x28, `master_vol`, `music_vol`, `voice_vol`, `fx_vol`:
    /// default 1.
    pub master: f32,
    pub music: f32,
    pub voice: f32,
    pub fx: f32,
    /// +0x2c `display_crosshair` (default on), +0x2d `invert_mouse`
    /// (off), +0x2e `display_subtitle`: on when the registry value
    /// `Half_Loc` exists under `HKLM\Software\Nobilis\The secrets of
    /// Atlantis...` (@ 0x0043dfb0); Native has no registry and takes it
    /// as absent [INFERRED: what the French DVD's installer writes is
    /// not known].
    pub crosshair: bool,
    pub invert_mouse: bool,
    pub subtitles: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            screen_size: 0,
            graphic_quality: 1,
            master: 1.0,
            music: 1.0,
            voice: 1.0,
            fx: 1.0,
            crosshair: true,
            invert_mouse: false,
            subtitles: false,
        }
    }
}

impl Config {
    /// The end of a game (@ 0x0042ffd0): the volumes 1, the crosshair on,
    /// the invert and the subtitles off.
    pub fn reset_for_new_session(&mut self) {
        self.master = 1.0;
        self.music = 1.0;
        self.voice = 1.0;
        self.fx = 1.0;
        self.crosshair = true;
        self.invert_mouse = false;
        self.subtitles = false;
    }
}

/// What a menu asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// A new game (@ 0x0042fc50).
    NewGame,
    /// `saves/<file>` read (@ 0x0043b1f0 + @ 0x0043b210).
    Load(String),
    /// The game written to `saves/<file>` (@ 0x0043ad90).
    Save(String),
    /// A request on the game manager's queue.
    Request(u32),
    /// `saves/<file>` deleted when it exists.
    Delete(String),
    /// `saves/<from>` copied over `saves/<to>` when it exists
    /// (`CopyFileA`, not failing if the target exists).
    Copy { from: String, to: String },
    /// The game ended (@ 0x0042ffd0).
    EndGame,
    /// The configuration changed (volumes, flags).
    Config,
    /// `Atl_Music_StopAll` with a fade (@ 0x0045b750).
    MusicStopAll(f32),
    /// `Atl_Music_Play` (@ 0x0045a730).
    MusicPlay(u32, Vec<u8>),
}

/// A `COLORREF` (0x00bbggrr) as 0x00rrggbb.
pub(crate) fn rgb(colorref: u32) -> u32 {
    ((colorref & 0xff) << 16) | (colorref & 0xff00) | ((colorref >> 16) & 0xff)
}

/// A text element's alignment (+0x80) as the layout's horizontal code
/// (@ 0x00451600: 0 → 0, 1 → 2, 2 → 1).
pub(crate) fn halign(align: u32) -> i32 {
    match align {
        1 => 2,
        2 => 1,
        _ => 0,
    }
}

#[derive(Clone, Debug)]
struct Text {
    font: String,
    text: String,
    width: i32,
    /// +0x7c (a plain text) or +0x84 (a button).
    color: u32,
    /// +0x88: a button's colour under the mouse.
    hover: Option<u32>,
    /// +0x80.
    align: u32,
    /// A button's +0x8c.
    over: bool,
    /// A scroll box (`CIE_ScrollText` @ 0x00450ab0): its height (+0x28),
    /// the scroll (+0x94).
    scroll_box: Option<(i32, i32)>,
}

impl Text {
    /// The colour drawn: a button's under the mouse, else its own.
    fn shown_color(&self) -> u32 {
        if self.over {
            self.hover.unwrap_or(self.color)
        } else {
            self.color
        }
    }
}

struct SprButton {
    click: Option<Bank>,
    focus: Option<Bank>,
    /// +0x98.
    state: u8,
    time: f64,
}

impl SprButton {
    /// The bank drawn and its picture: at 1 the focus looped, at 2 the
    /// click played once, else the click's first picture (15 a second).
    fn shown(&self) -> Option<(&Bank, usize)> {
        let frame = |bank: &Bank, looped: bool| {
            let n = bank.len().max(1);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let u = (self.time * 15.0) as usize;
            if looped {
                u % n
            } else {
                u.min(n - 1)
            }
        };
        match self.state {
            1 => self.focus.as_ref().map(|f| (f, frame(f, true))),
            2 => self.click.as_ref().map(|c| (c, frame(c, false))),
            _ => self.click.as_ref().map(|c| (c, 0)),
        }
    }
}

enum Kind {
    Text(Text),
    Button(SprButton),
    /// A sprite (`CIE_Sprite` @ 0x0044f800) at its hot spot.
    Sprite(Option<Bank>),
}

/// An element of a menu.
struct Element {
    name: &'static str,
    /// The place relative to the parent (the background at (0, 0), or
    /// `parent`).
    x: i32,
    y: i32,
    parent: Option<usize>,
    visible: bool,
    kind: Kind,
}

impl Element {
    fn text(name: &'static str, font: &str, text: String, x: i32, y: i32, w: i32) -> Element {
        Element {
            name,
            x,
            y,
            parent: None,
            visible: true,
            kind: Kind::Text(Text {
                font: font.to_string(),
                text,
                width: w,
                color: 0,
                hover: None,
                align: 0,
                over: false,
                scroll_box: None,
            }),
        }
    }

    /// A text button in black, red under the mouse (0, 0xff).
    fn button(name: &'static str, font: &str, text: String, x: i32, y: i32, w: i32) -> Element {
        let mut e = Element::text(name, font, text, x, y, w);
        if let Kind::Text(t) = &mut e.kind {
            t.hover = Some(0xff);
        }
        e
    }

    fn align(mut self, a: u32) -> Element {
        if let Kind::Text(t) = &mut self.kind {
            t.align = a;
        }
        self
    }
}

/// What a press of the left button did.
enum Press {
    Clicked(&'static str),
    Taken,
    None,
}

/// The credits' timer (@ 0x0040e510: 4 s (0x40100000) a page, 30 pages).
struct Credits {
    page: usize,
    left: f64,
}

/// The credits' pages (`_credits_<key>` and `_credits_<key>_name`,
/// the table at 0x0066b414).
pub const CREDIT_KEYS: [&str; 30] = [
    "start",
    "start_game_title",
    "production",
    "executive_production",
    "project_manager",
    "conception",
    "lead_programmer",
    "additional_programming",
    "artistic_direction",
    "designs",
    "nobilis_publishing_team",
    "nobilis_managing_director",
    "nobilis_production_manager",
    "nobilis_publishing_manager_sr",
    "nobilis_test_coordinator",
    "nobilis_operation_manager",
    "nobilis_publishing_manager_jr",
    "nobilis_assistant_producer",
    "nobilis_marketing_team",
    "nobilis_marketing_manager",
    "nobilis_pr_manager",
    "nobilis_pr_assistant",
    "nobilis_product_manager",
    "mzone_studio",
    "mzone_direction",
    "mzone_modeling_animations",
    "additional_designs",
    "music",
    "bink",
    "copyright",
];

/// A menu up.
pub struct Menu {
    pub mode: u32,
    background: Option<Vec<u32>>,
    elements: Vec<Element>,
    /// The menu's own state (+0x14, +0x3c, +0x44...): 0 to lay out, 1
    /// running; [`Menu::closed`] is its 2.
    laid_out: bool,
    closed: bool,
    credits: Option<Credits>,
    /// The save page's labels to make again.
    relabel: bool,
    /// Each text's height (+0x28) as last drawn, the hit test's
    /// (@ 0x00452020): 0 until the element's first draw.
    heights: Vec<i32>,
}

/// The menus' side of the game.
pub struct Menus {
    src: Arc<dyn FileSource>,
    pub strings: StringResources,
    cursor: Option<Bank>,
    menu: Option<Menu>,
    /// The manager's queue of modes (+0x44..).
    queue: VecDeque<u32>,
    /// The death panel's text (@ 0x00410d00: the string of
    /// `Atl_GameOver`'s key).
    death_text: String,
    /// The examine view's item (@ 0x004137b0).
    examine_item: Vec<u8>,
    pub mouse: (i32, i32),
    /// Where the menu's last hover test was made: the buttons' hit test
    /// runs on a mouse move only (@ 0x004523e0, event kind 2): the
    /// synthetic move at the centre when a mode opens (@ 0x0041b780), then
    /// the player's moves.
    last_mouse: (i32, i32),
    clicks: usize,
    /// The saves (a folder, the browser's storage).
    pub saves: Option<std::rc::Rc<dyn crate::atlantis5_saves::SaveFiles>>,
    /// The local time zone's offset from UTC (seconds) for the save
    /// pages' dates (`FileTimeToLocalFileTime`).
    pub utc_offset: fn() -> i64,
    effects: Vec<Effect>,
    /// How many menus were made: tells a menu from the one before it in
    /// [`MenuPicture`] (their backgrounds and sprites are made with them).
    made: u64,
    /// Each mode's texts' heights as last drawn ([`Menu::heights`]),
    /// kept for the program's run: the original makes its menus and
    /// their elements once (the manager's ctor @ 0x0041b170), and a
    /// text's +0x28 is 0 from its ctor (@ 0x00451170) until its first
    /// draw (@ 0x00451600, the only write).
    drawn: HashMap<u32, Vec<i32>>,
}

/// Everything [`Menus::render`] draws from while the menu's background
/// covers the screen (a host-side check, not the original's: it draws
/// every frame): two equal keys are the same picture. The menu (its
/// background, banks and fonts never change once it is made), the
/// cursor's place and, in drawing order, each drawn element's place and
/// what [`Menu::draw`] reads of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuPicture {
    menu: u64,
    mouse: (i32, i32),
    elements: Vec<ElementPicture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ElementPicture {
    Text {
        at: (i32, i32),
        font: String,
        text: String,
        width: i32,
        color: u32,
        align: u32,
        scroll_box: Option<(i32, i32)>,
    },
    /// The bank drawn (0 none, 1 the focus, 2 the click) and its picture.
    Button {
        at: (i32, i32),
        bank: u8,
        frame: usize,
    },
    Sprite {
        at: (i32, i32),
    },
}

fn no_offset() -> i64 {
    0
}

impl Menus {
    /// The menus of the data in `src`, texts of `language`
    /// (`texts/menu/<language>/string_resources.xml`, @ 0x0043d910).
    pub fn new(src: Arc<dyn FileSource>, language: &str) -> Menus {
        let strings = src
            .read(&format!("datas/texts/menu/{language}/string_resources.xml"))
            .and_then(|d| StringResources::parse(&d).ok())
            .unwrap_or_default();
        let cursor = src
            .read("datas/sprites2d/menu/cursor.spr")
            .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok());
        Menus {
            src,
            strings,
            cursor,
            menu: None,
            queue: VecDeque::new(),
            death_text: String::new(),
            examine_item: Vec::new(),
            mouse: (W / 2, H / 2),
            last_mouse: (W / 2, H / 2),
            clicks: 0,
            saves: None,
            utc_offset: no_offset,
            effects: Vec::new(),
            made: 0,
            drawn: HashMap::new(),
        }
    }

    /// The string `id` of `string_resources.xml` (@ 0x0040ed50); a
    /// missing one is empty (logged).
    pub fn string(&self, id: &str) -> String {
        match self.strings.get(id) {
            Some(s) => s.to_string(),
            None => {
                log::info!("string_resources: no {id}");
                String::new()
            }
        }
    }

    /// The text `id` of `string_resources.xml`, `None` when it has none.
    pub fn string_opt(&self, id: &str) -> Option<String> {
        self.strings.get(id).map(|s| s.to_string())
    }

    /// A mode asked (@ 0x0041b5d0, queued).
    pub fn request(&mut self, mode: u32) {
        self.queue.push_back(mode);
    }

    /// `Atl_GameOver`'s text (@ 0x00410d00).
    pub fn set_death_text(&mut self, key: &str) {
        self.death_text = self.string(key);
    }

    /// `Atl_ExamineItem`'s item (@ 0x004137b0).
    pub fn set_examine_item(&mut self, item: &[u8]) {
        self.examine_item = item.to_vec();
    }

    /// The menu up, if any.
    pub fn current(&self) -> Option<u32> {
        self.menu.as_ref().map(|m| m.mode)
    }

    /// Whether a menu is up or queued.
    pub fn active(&self) -> bool {
        self.menu.is_some() || !self.queue.is_empty()
    }

    /// The modes queued (tests).
    pub fn queued(&self) -> Vec<u32> {
        self.queue.iter().copied().collect()
    }

    /// Takes the next queued mode when it is `mode` and no menu is up
    /// (the inventory's mode 8, which the engine's side runs).
    pub fn take_next(&mut self, mode: u32) -> bool {
        if self.menu.is_none() && self.queue.front() == Some(&mode) {
            self.queue.pop_front();
            return true;
        }
        false
    }

    /// A press of the left button, taken at the next step.
    pub fn click(&mut self) {
        self.clicks += 1;
    }

    /// The effects the last steps asked for.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    /// The mouse at (x, y), kept within the screen (@ 0x0040d890).
    pub fn mouse_at(&mut self, x: i32, y: i32) {
        self.mouse = (x.clamp(0, W), y.clamp(0, H));
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.src.read(path)
    }

    fn spr(&self, path: &str) -> Option<Bank> {
        self.read(path)
            .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())
    }

    fn tga(&self, path: &str) -> Option<Vec<u32>> {
        let d = self.read(path)?;
        let img = atlantis4_data::image::decode_tga(&d).ok()?;
        if img.width as usize != VIEW_WIDTH || img.height as usize != VIEW_HEIGHT {
            log::warn!("{path}: {}x{}", img.width, img.height);
        }
        let (w, h) = (img.width as usize, img.height as usize);
        let mut out = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        for y in 0..h.min(VIEW_HEIGHT) {
            for x in 0..w.min(VIEW_WIDTH) {
                let p = &img.pixels[(y * w + x) * 4..][..4];
                out[y * VIEW_WIDTH + x] =
                    (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
            }
        }
        Some(out)
    }

    fn sprite_button(&self, dir: &str, name: &'static str, click: &str, focus: &str) -> Element {
        Element {
            name,
            x: 0,
            y: 0,
            parent: None,
            visible: true,
            kind: Kind::Button(SprButton {
                click: self.spr(&format!("{dir}/{click}")),
                focus: self.spr(&format!("{dir}/{focus}")),
                state: 0,
                time: 0.0,
            }),
        }
    }

    /// The saves' file `name` exists.
    fn save_exists(&self, name: &str) -> bool {
        self.saves.as_ref().is_some_and(|d| d.exists(name))
    }

    // -- the menus -----------------------------------------------------------

    /// The main menu (`CMenu_Main` @ 0x00428000): `sprites2d/menu/enter/
    /// background.tga` (path 22); the buttons `new_game`, `load_game`,
    /// `rescue_game`, `credits` and `quit_game` (the text
    /// `quit_atlantis`), `AtlFont_Default`, 350 wide, centred.
    fn build_main(&self) -> Menu {
        let f = "AtlFont_Default";
        let elements = vec![
            Element::button("new_game", f, self.string("new_game"), 0x118, 0x140, 0x15e).align(1),
            Element::button(
                "load_game",
                f,
                self.string("load_game"),
                0x118,
                0x168,
                0x15e,
            )
            .align(1),
            Element::button(
                "rescue_game",
                f,
                self.string("rescue_game"),
                0x118,
                400,
                0x15e,
            )
            .align(1),
            Element::button("credits", f, self.string("credits"), 0x118, 0x1b8, 0x15e).align(1),
            Element::button(
                "quit_game",
                f,
                self.string("quit_atlantis"),
                0x118,
                0x1e0,
                0x15e,
            )
            .align(1),
        ];
        Menu::new(
            MAIN,
            self.tga("datas/sprites2d/menu/enter/background.tga"),
            elements,
        )
    }

    /// The in-game menu (`CMenu_InGame` @ 0x004193f0):
    /// `sprites2d/menu/main/background.tga` (path 26); `AtlFont_Default`,
    /// 500 wide; the buttons `resume` (`back_to_game`, right-aligned at
    /// (280, 450)), `save` (`save_game`), `option_graphic`,
    /// `option_sound`, `option_control` (`*_options`), `exit`
    /// (`quit_game`), the text `confirm_exit_title`
    /// (`confirm_quit_game`) and the buttons `exit_yes` (`yes`) and
    /// `exit_no` (`no`), centred.
    fn build_in_game(&self) -> Menu {
        let f = "AtlFont_Default";
        let b =
            |name, key: &str| Element::button(name, f, self.string(key), 0x118, 0, 500).align(1);
        // In the ctor's order (`evidence/m2/menu-in-game-ctor.trace`):
        // `resume` after `exit`.
        let elements = vec![
            b("save", "save_game"),
            b("option_graphic", "graphic_options"),
            b("option_sound", "sound_options"),
            b("option_control", "control_options"),
            b("exit", "quit_game"),
            Element::button("resume", f, self.string("back_to_game"), 0x118, 0x1c2, 500).align(2),
            Element::text(
                "confirm_exit_title",
                f,
                self.string("confirm_quit_game"),
                0x118,
                200,
                500,
            )
            .align(1),
            b("exit_yes", "yes"),
            b("exit_no", "no"),
        ];
        Menu::new(
            IN_GAME,
            self.tga("datas/sprites2d/menu/main/background.tga"),
            elements,
        )
    }

    /// The load (`CMenu_Load` @ 0x00417be0, path 28) and save
    /// (`CMenu_Save` @ 0x00426ab0, path 29) pages: ten buttons `save_0`
    /// .. `save_9`, left-aligned, 500 wide, and `back` right-aligned.
    fn build_slots(&self, mode: u32) -> Menu {
        const NAMES: [&str; 10] = [
            "save_0", "save_1", "save_2", "save_3", "save_4", "save_5", "save_6", "save_7",
            "save_8", "save_9",
        ];
        let f = "AtlFont_Default";
        let mut elements: Vec<Element> = NAMES
            .iter()
            .map(|n| Element::button(n, f, String::new(), 0x118, 0, 500))
            .collect();
        elements.push(Element::button("back", f, self.string("back"), 0xb4, 0x1c2, 500).align(2));
        let dir = if mode == LOAD { "load" } else { "save" };
        let mut m = Menu::new(
            mode,
            self.tga(&format!("datas/sprites2d/menu/{dir}/background.tga")),
            elements,
        );
        self.slot_labels(&mut m);
        m
    }

    /// The slots' labels (@ 0x004183a0, the save page's @ 0x00427840):
    /// `saves/slot_<i>` missing: `save` + " " + i + " : " + `empty`;
    /// else `save` + " " + i + " : " + its last write in local time
    /// `%.2d/%.2d/%d (%.2d:%.2d:%.2d)` (0x0066bc30: day, month, year,
    /// hour, minute, second).
    fn slot_labels(&self, m: &mut Menu) {
        let save = self.string("save");
        let empty = self.string("empty");
        for i in 0..10usize {
            let file = format!("slot_{i}");
            let tail = match self.saves.as_ref().and_then(|d| d.modified(&file)) {
                None => empty.clone(),
                Some(secs) => {
                    let (y, mo, d, h, mi, s) = civil(secs + (self.utc_offset)());
                    format!("{d:02}/{mo:02}/{y} ({h:02}:{mi:02}:{s:02})")
                }
            };
            if let Kind::Text(t) = &mut m.elements[i].kind {
                t.text = format!("{save} {i} : {tail}");
            }
        }
    }

    /// The graphic page (`CMenu_OptionGraphic` @ 0x0041ded0, path 30):
    /// `depth` (`graphic_quality : high/low`) and `screen_size`
    /// (`screen_size : 4/3` or `16/9`), centred, and `back`.
    fn build_graphic(&self, config: &Config) -> Menu {
        let f = "AtlFont_Default";
        let elements = vec![
            Element::button("depth", f, self.graphic_label(config), 0x118, 0, 500).align(1),
            Element::button("screen_size", f, self.screen_label(config), 0x118, 0, 500).align(1),
            Element::button("back", f, self.string("back"), 0x118, 0x1c2, 500).align(2),
        ];
        Menu::new(
            GRAPHIC,
            self.tga("datas/sprites2d/menu/option_graphic/background.tga"),
            elements,
        )
    }

    /// @ 0x0041e8b0: `graphic_quality` + " : " + `high` (0x0066bdf4) when
    /// the display's pixel format (@ 0x0048d660) is `x8r8g8b8` (0x96) or
    /// `a8r8g8b8` (0x9a), else `low` (0x0066be0c). The display follows
    /// `graphic_quality` (@ 0x00429950: 1 → 0x96, 0 → `r5g6b5` 0x8a), so
    /// the label is `high` for 1, `low` for 0. Native keeps drawing in 32
    /// bits (the 16-bit look is [UNKNOWN]).
    fn graphic_label(&self, config: &Config) -> String {
        let q = if config.graphic_quality == 1 {
            "high"
        } else {
            "low"
        };
        format!("{} : {}", self.string("graphic_quality"), self.string(q))
    }

    /// @ 0x0041e8b0: `screen_size` + " 4/3" (0x0066bde4) or " 16/9"
    /// (0x0066bdd4) as +0x14 is 0 or not.
    fn screen_label(&self, config: &Config) -> String {
        let s = if config.screen_size == 0 {
            " 4/3"
        } else {
            " 16/9"
        };
        format!("{}{s}", self.string("screen_size"))
    }

    /// The sound page (`CMenu_OptionSound` @ 0x0041f370, path 31): for
    /// master, music, voice and fx a title (`*_volume`), a `-` button
    /// (`*_sub`), the level (`*_level`) and a `+` button (`*_add`);
    /// `back`.
    fn build_sound(&self, config: &Config) -> Menu {
        let f = "AtlFont_Default";
        let mut elements = Vec::new();
        let rows: [(
            &'static str,
            &'static str,
            &'static str,
            &'static str,
            &str,
            f32,
        ); 4] = [
            (
                "master_title",
                "master_sub",
                "master_level",
                "master_add",
                "master_volume",
                config.master,
            ),
            (
                "music_title",
                "music_sub",
                "music_level",
                "music_add",
                "music_volume",
                config.music,
            ),
            (
                "voice_title",
                "voice_sub",
                "voice_level",
                "voice_add",
                "voice_volume",
                config.voice,
            ),
            (
                "fx_title",
                "fx_sub",
                "fx_level",
                "fx_add",
                "fx_volume",
                config.fx,
            ),
        ];
        for (title, sub, level, add, key, v) in rows {
            elements.push(Element::text(title, f, self.string(key), 0x118, 0, 500).align(1));
            elements.push(Element::button(sub, f, "-".into(), 0x1b8, 0, 0x28).align(2));
            elements.push(Element::text(level, f, level_text(v), 0x1e0, 0, 100).align(1));
            elements.push(Element::button(add, f, "+".into(), 0x244, 0, 0x28).align(0));
        }
        elements.push(Element::button("back", f, self.string("back"), 0x118, 0x1c2, 500).align(2));
        Menu::new(
            SOUND,
            self.tga("datas/sprites2d/menu/option_sound/background.tga"),
            elements,
        )
    }

    /// The control page (`CMenu_OptionControl` @ 0x0041bec0, path 32):
    /// `crosshair` (`crosshair : yes/no`), `invert_pitch`
    /// (`invert_mouse : yes/no`), `subtitle` (`subtitle : yes/no`),
    /// centred; `back`.
    fn build_control(&self, config: &Config) -> Menu {
        let f = "AtlFont_Default";
        let elements = vec![
            Element::button(
                "crosshair",
                f,
                self.flag_label("crosshair", config.crosshair),
                0x118,
                0,
                500,
            )
            .align(1),
            Element::button(
                "invert_pitch",
                f,
                self.flag_label("invert_mouse", config.invert_mouse),
                0x118,
                0,
                500,
            )
            .align(1),
            Element::button(
                "subtitle",
                f,
                self.flag_label("subtitle", config.subtitles),
                0x118,
                0,
                500,
            )
            .align(1),
            Element::button("back", f, self.string("back"), 0x118, 0x1c2, 500).align(2),
        ];
        Menu::new(
            CONTROL,
            self.tga("datas/sprites2d/menu/option_control/background.tga"),
            elements,
        )
    }

    /// @ 0x0041c8d0: the key's string + " : " (0x0066bc1c) + `yes` or
    /// `no`.
    fn flag_label(&self, key: &str, on: bool) -> String {
        let v = self.string(if on { "yes" } else { "no" });
        format!("{} : {v}", self.string(key))
    }

    /// The credits (`CMenu_Credits` @ 0x0040dbf0, path 21): the sprite
    /// button `back` (`button_click.spr`, `button_focus.spr`), the texts
    /// `title` (`AtlFont_CreditTitle`) and `names` (`AtlFont_Credit`),
    /// both centred.
    fn build_credits(&self) -> Menu {
        let dir = "datas/sprites2d/menu/credits";
        let elements = vec![
            self.sprite_button(dir, "back", "button_click.spr", "button_focus.spr"),
            Element::text("title", "AtlFont_CreditTitle", String::new(), 0, 0, W).align(1),
            Element::text("names", "AtlFont_Credit", String::new(), 0, 0, W).align(1),
        ];
        let mut m = Menu::new(
            CREDITS,
            self.tga(&format!("{dir}/background.tga")),
            elements,
        );
        m.credits = Some(Credits { page: 0, left: 4.0 });
        m
    }

    /// The death panel (`CMenu_Death` @ 0x004104c0, path 23): the text
    /// `text` (`AtlFont_DeathDescription`, black, 600 wide, centred) and
    /// the sprite button `button`.
    fn build_death(&self) -> Menu {
        let dir = "datas/sprites2d/menu/death";
        let elements = vec![
            Element::text(
                "text",
                "AtlFont_DeathDescription",
                self.death_text.clone(),
                0,
                0,
                600,
            )
            .align(1),
            self.sprite_button(dir, "button", "button_click.spr", "button_focus.spr"),
        ];
        Menu::new(DEATH, self.tga(&format!("{dir}/background.tga")), elements)
    }

    /// The examine view (`CMenu_Examine` @ 0x00412f60, path 24): the
    /// sprite `object` (`sprites2d/objects_detailed/<item>.spr`, path 19;
    /// `proxy.spr` when missing, @ 0x00413a90), the scroll box
    /// `description` (`AtlFont_ExamineDescription`, colour 1, 320 × 360;
    /// the text `_desc_<item>`), the sprite buttons `up` and `down`
    /// (`up_button[_over].spr`, `down_button[_over].spr`) inside it.
    fn build_examine(&self) -> Menu {
        let dir = "datas/sprites2d/menu/examine";
        let item = String::from_utf8_lossy(&self.examine_item).into_owned();
        let object = self
            .spr(&format!("datas/sprites2d/objects_detailed/{item}.spr"))
            .or_else(|| self.spr("datas/sprites2d/objects_detailed/proxy.spr"));
        let key = format!("_desc_{item}");
        // A missing description shows the program's own text (0x0066ba70),
        // not shipped with the data: Native shows nothing (logged).
        let text = self.string(&key);
        let mut description = Element::text(
            "description",
            "AtlFont_ExamineDescription",
            text,
            0,
            0,
            0x140,
        );
        if let Kind::Text(t) = &mut description.kind {
            t.color = 1;
            t.scroll_box = Some((0x168, 0));
        }
        let mut up = self.sprite_button(dir, "up", "up_button.spr", "up_button_over.spr");
        let mut down = self.sprite_button(dir, "down", "down_button.spr", "down_button_over.spr");
        // Their parent is the description: the scroll text kept at +0x1c
        // (0x004132ac) is both calls' parent argument (0x004133f3,
        // 0x004135f6).
        up.parent = Some(1);
        down.parent = Some(1);
        let elements = vec![
            Element {
                name: "object",
                x: 0,
                y: 0,
                parent: None,
                visible: true,
                kind: Kind::Sprite(object),
            },
            description,
            up,
            down,
        ];
        Menu::new(
            EXAMINE,
            self.tga(&format!("{dir}/background.tga")),
            elements,
        )
    }

    /// The menu of `mode`, made and initialized (the manager's vtable
    /// +8); none for a mode not ported here.
    fn build(&self, mode: u32, config: &Config) -> Option<Menu> {
        Some(match mode {
            MAIN => self.build_main(),
            CREDITS => self.build_credits(),
            IN_GAME => {
                let mut m = self.build_in_game();
                // @ 0x0041a6c0: the confirmation hidden.
                for n in ["confirm_exit_title", "exit_yes", "exit_no"] {
                    m.set_visible(n, false);
                }
                m
            }
            LOAD | SAVE => self.build_slots(mode),
            GRAPHIC => self.build_graphic(config),
            SOUND => self.build_sound(config),
            CONTROL => self.build_control(config),
            DEATH => self.build_death(),
            EXAMINE => self.build_examine(),
            _ => return None,
        })
    }

    /// The manager's process (@ 0x0041b780) for one frame of `dt`
    /// seconds: with no menu up, the next mode of the queue is made (the
    /// mouse at the screen's centre); then its process. Returns the mode
    /// that ended this frame with nothing queued after it (the manager's
    /// 1: the game manager takes its next request), if any.
    pub fn step(&mut self, dt: f64, fonts: &Fonts, config: &mut Config) -> Option<u32> {
        let mut opened = false;
        if self.menu.is_none() {
            let mode = self.queue.pop_front()?;
            match self.build(mode, config) {
                Some(m) => {
                    self.menu = Some(m);
                    self.made += 1;
                    // @ 0x0041b780: the mouse put at the screen's centre
                    // (@ 0x0040d890(w >> 1, h >> 1)), the mode's
                    // Initialize, then the frame's event list emptied and
                    // one mouse move (kind 2) at the centre inserted
                    // (@ 0x004546a0(2, 0, w >> 1, h >> 1, 0, 0),
                    // @ 0x00412cc0) before the mode's process. Every
                    // ported mode's first state lays out and reaches
                    // @ 0x0040d920, so the button whose box (its height as
                    // last drawn: 0 before its first draw of the run)
                    // holds the centre is hot from the opening frame, and
                    // that frame's own presses are dropped.
                    self.mouse = (W / 2, H / 2);
                    self.clicks = 0;
                    opened = true;
                    if mode == DEATH {
                        // @ 0x00410bd0: the music stopped with a 0.5 s
                        // fade, `death_theme` on channel 4.
                        self.effects.push(Effect::MusicStopAll(0.5));
                        self.effects
                            .push(Effect::MusicPlay(4, b"death_theme".to_vec()));
                    }
                    if mode == CREDITS {
                        // @ 0x0040e510 state 0 (its first frame): the
                        // music stopped at once (0x0040e59c, fade 0) and
                        // `credits` (0x0066b854) on channel 4 (0x0040e5bd).
                        self.effects.push(Effect::MusicStopAll(0.0));
                        self.effects.push(Effect::MusicPlay(4, b"credits".to_vec()));
                    }
                }
                None => {
                    log::info!("menu mode {mode}: not ported, closed");
                    return self.queue.is_empty().then_some(mode);
                }
            }
        }
        let clicks = std::mem::take(&mut self.clicks);
        let mut menu = self.menu.take()?;
        let mouse = self.mouse;
        menu.layout(fonts, self);
        // The texts' heights as last drawn (the draw @ 0x00451600 comes
        // after the process): none yet at a mode's first opening in the
        // run, so the opening's move at the centre tests one-row boxes.
        menu.heights = self.drawn.get(&menu.mode).cloned().unwrap_or_default();
        // The hit test on a move only (@ 0x004523e0 tests on an event of
        // kind 2 and keeps the result in +0x40; the text button
        // @ 0x0044eb60 turns 1 on a move with it, the sprite button
        // @ 0x0044f0e0 likewise): the opening frame's synthetic move, then
        // the player's moves. A button shown later is not hot until a move
        // (`Menu::set_visible`).
        if opened || mouse != self.last_mouse {
            menu.hover(mouse);
            self.last_mouse = mouse;
        }
        // The original takes a frame's events in turn (@ 0x0040d920), but
        // at the screen's refresh two presses never share a frame, so the
        // one that closes the menu is always its frame's last. Only a long
        // Native frame can hold several; the presses after the closing one
        // are dropped (else a double click would click the same button
        // twice: two new games, two save pages). This changes nothing the
        // original does.
        for _ in 0..clicks {
            if menu.closed {
                break;
            }
            match menu.press() {
                Press::Clicked(name) => self.handle(&mut menu, name, config, fonts),
                Press::Taken => {}
                // @ 0x004138f0: a left press no element took ends the
                // examine view.
                Press::None if menu.mode == EXAMINE => menu.closed = true,
                Press::None => {}
            }
        }
        for name in menu.animate(dt, mouse) {
            if menu.closed {
                break;
            }
            self.handle(&mut menu, name, config, fonts);
        }
        if let Some(c) = &mut menu.credits {
            c.left -= dt;
            if c.left < 0.0 {
                c.left = 4.0;
                c.page = (c.page + 1) % CREDIT_KEYS.len();
                menu.laid_out = false;
            }
        }
        if menu.closed {
            let mode = menu.mode;
            if mode == CREDITS {
                // @ 0x0040e510 state 2: the music stopped with a 1 s fade.
                self.effects.push(Effect::MusicStopAll(1.0));
            }
            return self.queue.is_empty().then_some(mode);
        }
        if menu.credits.is_some() && !menu.laid_out {
            self.credit_page(&mut menu, fonts);
        }
        // The frame's draw (@ 0x0041baa0, after the process): each shown
        // text measures its height (+0x28).
        menu.measure(fonts);
        self.drawn.insert(menu.mode, menu.heights.clone());
        self.menu = Some(menu);
        None
    }

    /// A credits page (@ 0x0040e750): `title` = `_credits_<key>`,
    /// `names` = `_credits_<key>_name` (empty when missing); `names` at
    /// (0, (H − its height) / 2), `title` 20 above it; both the screen's
    /// width; the button at (W − 100, H − 100).
    fn credit_page(&self, m: &mut Menu, fonts: &Fonts) {
        let page = m.credits.as_ref().map_or(0, |c| c.page);
        let key = CREDIT_KEYS[page];
        let title = self
            .strings
            .get(&format!("_credits_{key}"))
            .unwrap_or("")
            .to_string();
        let names = self
            .strings
            .get(&format!("_credits_{key}_name"))
            .unwrap_or("")
            .to_string();
        // @ 0x00461800(W, font, text, 0) for both: not partitioned (the
        // texts are drawn partitioned all the same).
        let ht = fonts.text_height_raw("AtlFont_CreditTitle", &title);
        let hn = fonts.text_height_raw("AtlFont_Credit", &names);
        let y = (H - hn) / 2;
        m.set_text("title", title);
        m.set_text("names", names);
        m.place("back", W - 100, H - 100);
        m.place("title", 0, y - ht - 0x14);
        m.place("names", 0, y);
        m.laid_out = true;
    }

    /// A clicked element's handler.
    fn handle(&mut self, m: &mut Menu, name: &'static str, config: &mut Config, fonts: &Fonts) {
        match m.mode {
            MAIN => self.main_click(m, name),
            IN_GAME => self.in_game_click(m, name),
            LOAD => self.load_click(m, name),
            SAVE => self.save_click(m, name),
            GRAPHIC => {
                // @ 0x0041edd0: `back` → mode 2; `depth`: `graphic_quality`
                // 0 when it was 1, else 1 (0x0041ee4d..0x0041ee6d), the
                // display switched to that format (@ 0x0044d590, not
                // ported: Native draws in 32 bits), then the page's init
                // again (@ 0x0041e8b0): its label. `screen_size` changes
                // the display mode (not ported: logged).
                match name {
                    "back" => {
                        self.request(IN_GAME);
                        m.closed = true;
                    }
                    "depth" => {
                        config.graphic_quality = i32::from(config.graphic_quality != 1);
                        self.effects.push(Effect::Config);
                        m.set_text("depth", self.graphic_label(config));
                        log::info!(
                            "graphic_quality {}: the display stays in 32 bits",
                            config.graphic_quality
                        );
                    }
                    _ => log::info!("graphic option {name}: the display mode is not changed"),
                }
            }
            SOUND => self.sound_click(m, name, config),
            CONTROL => self.control_click(m, name, config),
            CREDITS => {
                // @ 0x0040e6a0: `back` → mode 0.
                if name == "back" {
                    self.request(MAIN);
                    m.closed = true;
                }
            }
            DEATH => {
                // @ 0x00410d30: the menu ends.
                m.closed = true;
            }
            EXAMINE => {
                // @ 0x00414000: `up` scrolls a step up (@ 0x00450f00),
                // `down` down (@ 0x00450f20).
                m.scroll(name == "down", fonts);
            }
            _ => {}
        }
    }

    /// @ 0x00428ea0.
    fn main_click(&mut self, m: &mut Menu, name: &str) {
        match name {
            "new_game" => {
                self.effects.push(Effect::NewGame);
                m.closed = true;
            }
            "load_game" => {
                self.request(LOAD);
                m.closed = true;
            }
            "rescue_game" => {
                if self.save_exists("autoslot") {
                    self.effects.push(Effect::Load("autoslot".into()));
                    m.closed = true;
                }
            }
            "credits" => {
                self.request(CREDITS);
                m.closed = true;
            }
            "quit_game" => {
                self.effects.push(Effect::Request(8));
                m.closed = true;
            }
            _ => {}
        }
    }

    /// @ 0x0041a730.
    fn in_game_click(&mut self, m: &mut Menu, name: &str) {
        const MAIN_BUTTONS: [&str; 6] = [
            "resume",
            "save",
            "option_graphic",
            "option_sound",
            "option_control",
            "exit",
        ];
        const CONFIRM: [&str; 3] = ["confirm_exit_title", "exit_yes", "exit_no"];
        match name {
            "resume" => {
                self.effects.push(Effect::Request(4));
                m.closed = true;
            }
            "save" | "option_graphic" | "option_sound" | "option_control" => {
                self.request(match name {
                    "save" => SAVE,
                    "option_graphic" => GRAPHIC,
                    "option_sound" => SOUND,
                    _ => CONTROL,
                });
                m.closed = true;
            }
            "exit" | "exit_no" => {
                let confirm = name == "exit";
                for n in MAIN_BUTTONS {
                    m.set_visible(n, !confirm);
                }
                for n in CONFIRM {
                    m.set_visible(n, confirm);
                }
            }
            "exit_yes" => {
                // The autosave deleted, the game ended, the main menu,
                // the request 3.
                self.effects.push(Effect::Delete("autoslot".into()));
                self.effects.push(Effect::EndGame);
                self.request(MAIN);
                self.effects.push(Effect::Request(3));
                m.closed = true;
            }
            _ => {}
        }
    }

    /// The slot of `save_<n>` (@ 0x0062b347 `atoi` on the name after
    /// `save_`).
    fn slot_of(name: &str) -> Option<u32> {
        name.strip_prefix("save_")?.parse().ok()
    }

    /// @ 0x00418960: `back` → mode 0; `save_<n>`: when `slot_<n>` exists
    /// it is read and the menu ends; then `autoslot` deleted and
    /// `autoslot_<n>` copied to it (0x00418e05..0x00418e4c).
    fn load_click(&mut self, m: &mut Menu, name: &str) {
        if name == "back" {
            self.request(MAIN);
            m.closed = true;
            return;
        }
        let Some(n) = Self::slot_of(name) else { return };
        let slot = format!("slot_{n}");
        if self.save_exists(&slot) {
            self.effects.push(Effect::Load(slot));
            m.closed = true;
        }
        self.effects.push(Effect::Delete("autoslot".into()));
        self.effects.push(Effect::Copy {
            from: format!("autoslot_{n}"),
            to: "autoslot".into(),
        });
    }

    /// @ 0x004272b0: `back` → mode 2; `save_<n>`: the game written to
    /// `slot_<n>`, `autoslot_<n>` deleted and `autoslot` copied to it
    /// (0x004276dc..0x00427729), the labels made again; the page stays.
    fn save_click(&mut self, m: &mut Menu, name: &str) {
        if name == "back" {
            self.request(IN_GAME);
            m.closed = true;
            return;
        }
        let Some(n) = Self::slot_of(name) else { return };
        self.effects.push(Effect::Save(format!("slot_{n}")));
        self.effects.push(Effect::Delete(format!("autoslot_{n}")));
        self.effects.push(Effect::Copy {
            from: "autoslot".into(),
            to: format!("autoslot_{n}"),
        });
        m.relabel = true;
    }

    /// @ 0x00421370: `back` → mode 2; `-` and `+` change the level by
    /// 0.09999988 (0x0065563c) and 0.10000012 (0x00655638), kept within
    /// 0..1, through the sound's setters (master @ 0x0045a6d0, music
    /// @ 0x0045a640, voice @ 0x0045a590, fx @ 0x0045a540) into the
    /// configuration; then the four levels `%d%%` of `ftol(level ×
    /// 100)` (0x0066bf18, 0x00655634).
    fn sound_click(&mut self, m: &mut Menu, name: &str, config: &mut Config) {
        if name == "back" {
            self.request(IN_GAME);
            m.closed = true;
            return;
        }
        let add = f32::from_bits(0x3dcc_ccdd);
        let sub = f32::from_bits(0x3dcc_ccbd);
        let step = |v: f32, up: bool| (if up { v + add } else { v - sub }).clamp(0.0, 1.0);
        match name {
            "master_sub" | "master_add" => {
                config.master = step(config.master, name == "master_add")
            }
            "music_sub" | "music_add" => config.music = step(config.music, name == "music_add"),
            "voice_sub" | "voice_add" => config.voice = step(config.voice, name == "voice_add"),
            "fx_sub" | "fx_add" => config.fx = step(config.fx, name == "fx_add"),
            _ => return,
        }
        self.effects.push(Effect::Config);
        m.set_text("master_level", level_text(config.master));
        m.set_text("music_level", level_text(config.music));
        m.set_text("voice_level", level_text(config.voice));
        m.set_text("fx_level", level_text(config.fx));
    }

    /// @ 0x0041d0b0: `back` → mode 2; each flag turned over and its label
    /// made again (`subtitle` the game manager's +0x55, through
    /// @ 0x00477850 into the configuration).
    fn control_click(&mut self, m: &mut Menu, name: &str, config: &mut Config) {
        match name {
            "back" => {
                self.request(IN_GAME);
                m.closed = true;
                return;
            }
            "crosshair" => config.crosshair = !config.crosshair,
            "invert_pitch" => config.invert_mouse = !config.invert_mouse,
            "subtitle" => config.subtitles = !config.subtitles,
            _ => return,
        }
        self.effects.push(Effect::Config);
        m.set_text("crosshair", self.flag_label("crosshair", config.crosshair));
        m.set_text(
            "invert_pitch",
            self.flag_label("invert_mouse", config.invert_mouse),
        );
        m.set_text("subtitle", self.flag_label("subtitle", config.subtitles));
    }

    /// The menu over `out` (1024 × 576 x8r8g8b8): its background, its
    /// elements, the cursor; nothing with no menu up.
    pub fn render(&self, out: &mut [u32], fonts: &Fonts) {
        let Some(m) = &self.menu else { return };
        if let Some(bg) = &m.background {
            out.copy_from_slice(bg);
        }
        m.draw(out, fonts);
        self.draw_cursor(out);
    }

    /// Whether the menu up draws its background over the whole screen
    /// (every menu has one; none when its picture could not be read):
    /// then nothing drawn before it shows.
    pub fn covers_screen(&self) -> bool {
        self.menu
            .as_ref()
            .and_then(|m| m.background.as_ref())
            .is_some_and(|bg| bg.len() == VIEW_WIDTH * VIEW_HEIGHT)
    }

    /// What the menu's picture is drawn from ([`MenuPicture`]), when its
    /// background covers the screen; none otherwise.
    pub fn picture(&self) -> Option<MenuPicture> {
        if !self.covers_screen() {
            return None;
        }
        let m = self.menu.as_ref()?;
        let elements = (0..m.elements.len())
            .filter(|&i| m.visible(i))
            .map(|i| {
                let at = m.abs(i);
                match &m.elements[i].kind {
                    Kind::Text(t) => ElementPicture::Text {
                        at,
                        font: t.font.clone(),
                        text: t.text.clone(),
                        width: t.width,
                        color: t.shown_color(),
                        align: t.align,
                        scroll_box: t.scroll_box,
                    },
                    Kind::Button(b) => {
                        let (bank, frame) = match b.shown() {
                            Some((_, frame)) if b.state == 1 => (1, frame),
                            Some((_, frame)) => (2, frame),
                            None => (0, 0),
                        };
                        ElementPicture::Button { at, bank, frame }
                    }
                    Kind::Sprite(_) => ElementPicture::Sprite { at },
                }
            })
            .collect();
        Some(MenuPicture {
            menu: self.made,
            mouse: self.mouse,
            elements,
        })
    }

    /// The cursor (`cursor.spr`, frame 0) at the mouse.
    pub fn draw_cursor(&self, out: &mut [u32]) {
        self.draw_cursor_at(out, self.mouse);
    }

    /// The menus' cursor drawn at (x, y) (a board whose cursor element
    /// stays where it was while the mouse moved).
    pub fn draw_cursor_at(&self, out: &mut [u32], (x, y): (i32, i32)) {
        if let Some(c) = &self.cursor {
            let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
            c.draw(&mut t, Some(x), y, 0);
        }
    }

    /// The elements' names and texts as shown (tests): (name, text,
    /// visible, x, y).
    pub fn shown(&self) -> Vec<(String, String, bool, i32, i32)> {
        let Some(m) = &self.menu else {
            return Vec::new();
        };
        m.elements
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (x, y) = m.abs(i);
                let text = match &e.kind {
                    Kind::Text(t) => t.text.clone(),
                    _ => String::new(),
                };
                (e.name.to_string(), text, e.visible, x, y)
            })
            .collect()
    }

    /// The buttons hot (tests): text buttons at 1 (+0x8c), sprite buttons
    /// at 1 or 2 (+0x98), in the elements' order.
    #[doc(hidden)]
    pub fn hot(&self) -> Vec<&'static str> {
        let Some(m) = &self.menu else {
            return Vec::new();
        };
        m.elements
            .iter()
            .filter(|e| match &e.kind {
                Kind::Text(t) => t.over,
                Kind::Button(b) => b.state != 0,
                Kind::Sprite(_) => false,
            })
            .map(|e| e.name)
            .collect()
    }

    /// The middle of element `name`'s box (tests and tools), if shown.
    pub fn point_of(&self, name: &str, fonts: &Fonts) -> Option<(i32, i32)> {
        let m = self.menu.as_ref()?;
        let i = m
            .elements
            .iter()
            .position(|e| e.name == name && e.visible)?;
        let (x, y) = m.abs(i);
        let (w, h) = m.size(i, fonts);
        Some((x + w / 2, y + h / 2))
    }
}

/// `%d%%` of `ftol(v × 100)`.
fn level_text(v: f32) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let n = (v * 100.0) as i32;
    format!("{n}%")
}

/// The civil date and time of `secs` seconds from 1970 (days from the
/// proleptic Gregorian calendar): year, month, day, hour, minute,
/// second.
fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (
        y,
        m as u32,
        d as u32,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

impl Menu {
    fn new(mode: u32, background: Option<Vec<u32>>, elements: Vec<Element>) -> Menu {
        Menu {
            mode,
            background,
            elements,
            laid_out: false,
            closed: false,
            credits: None,
            relabel: false,
            heights: Vec::new(),
        }
    }

    fn index(&self, name: &str) -> Option<usize> {
        self.elements.iter().position(|e| e.name == name)
    }

    fn set_visible(&mut self, name: &str, v: bool) {
        if let Some(i) = self.index(name) {
            let e = &mut self.elements[i];
            // A show resets the hover (@ 0x00452530: +0x40; the text
            // button's @ 0x0044ec50: +0x8c): hot again after a move only.
            if v && !e.visible {
                if let Kind::Text(t) = &mut e.kind {
                    t.over = false;
                }
            }
            e.visible = v;
        }
    }

    fn set_text(&mut self, name: &str, text: String) {
        if let Some(i) = self.index(name) {
            if let Kind::Text(t) = &mut self.elements[i].kind {
                t.text = text;
            }
        }
    }

    fn place(&mut self, name: &str, x: i32, y: i32) {
        if let Some(i) = self.index(name) {
            self.elements[i].x = x;
            self.elements[i].y = y;
        }
    }

    /// The text height of element `name` at its width.
    fn height_of(&self, name: &str, fonts: &Fonts) -> i32 {
        self.index(name)
            .map_or(0, |i| match &self.elements[i].kind {
                Kind::Text(t) => fonts.text_height(&t.font, &t.text, t.width),
                _ => 0,
            })
    }

    /// The place on the screen (@ 0x004520a0: the parents' places
    /// added).
    fn abs(&self, i: usize) -> (i32, i32) {
        let e = &self.elements[i];
        let (px, py) = e.parent.map_or((0, 0), |p| self.abs(p));
        (px + e.x, py + e.y)
    }

    /// The box's size: a text's width and height (a scroll box's own
    /// height), a sprite button's first picture's.
    fn size(&self, i: usize, fonts: &Fonts) -> (i32, i32) {
        match &self.elements[i].kind {
            Kind::Text(t) => match t.scroll_box {
                Some((h, _)) => (t.width, h),
                None => (t.width, fonts.text_height(&t.font, &t.text, t.width)),
            },
            Kind::Button(b) => b
                .click
                .as_ref()
                .and_then(|b| b.info(0))
                .map_or((0, 0), |s| (i32::from(s.w), i32::from(s.h))),
            Kind::Sprite(_) => (0, 0),
        }
    }

    /// Each menu's first step (its state 0): the places that depend on
    /// the texts' heights.
    fn layout(&mut self, fonts: &Fonts, host: &Menus) {
        if self.relabel {
            self.relabel = false;
            host.slot_labels(self);
            self.laid_out = false;
        }
        if self.laid_out {
            return;
        }
        self.laid_out = true;
        let h = |m: &Menu, n: &str| m.height_of(n, fonts);
        match self.mode {
            MAIN => {
                // @ 0x004289e0 state 0: x 100, from y 250, each 20 below
                // the previous; `rescue_game` hidden without
                // `saves/autoslot`; `quit_game` then `credits` 40 below
                // it, by the height of `load_game`'s text (+0x20, as the
                // code reads it).
                let mut y = 0xfa;
                self.place("new_game", 100, y);
                y += h(self, "new_game") + 0x14;
                self.place("load_game", 100, y);
                y += h(self, "load_game") + 0x14;
                if host.save_exists("autoslot") {
                    self.place("rescue_game", 100, y);
                    y += h(self, "rescue_game") + 0x14;
                } else {
                    self.set_visible("rescue_game", false);
                }
                self.place("quit_game", 100, y);
                y += h(self, "load_game") + 0x28;
                self.place("credits", 100, y);
            }
            IN_GAME => {
                // @ 0x004193f0: `save` at y 180, each 20 below the
                // previous; the confirmation at 200, `exit_yes` 20 below
                // its text, `exit_no` 20 below that.
                let mut y = 0xb4;
                for n in [
                    "save",
                    "option_graphic",
                    "option_sound",
                    "option_control",
                    "exit",
                ] {
                    self.place(n, 0x118, y);
                    y += h(self, n) + 0x14;
                }
                let y = 0xdc + h(self, "confirm_exit_title");
                self.place("exit_yes", 0x118, y);
                self.place("exit_no", 0x118, y + 0x14 + h(self, "exit_yes"));
            }
            LOAD | SAVE => {
                // @ 0x00418210 / 0x004270c0 state 0: the slots at x 280
                // from y 180, each 10 below the previous; `back` at
                // (280, 450).
                let mut y = 0xb4;
                for i in 0..10 {
                    self.elements[i].x = 0x118;
                    self.elements[i].y = y;
                    let e = &self.elements[i];
                    if let Kind::Text(t) = &e.kind {
                        y += fonts.text_height(&t.font, &t.text, t.width) + 10;
                    }
                }
                self.place("back", 0x118, 0x1c2);
            }
            GRAPHIC => {
                // @ 0x0041e560: `depth` at (280, 200), `screen_size` 220
                // below the top by `depth`'s height, `back` (280, 450).
                self.place("depth", 0x118, 200);
                let y = h(self, "depth") + 0xdc;
                self.place("screen_size", 0x118, y);
                self.place("back", 0x118, 0x1c2);
            }
            CONTROL => {
                // @ 0x0041c680: (280, 200), then 220 + h1, then 240 + h1
                // + h2; `back` (280, 450).
                self.place("crosshair", 0x118, 200);
                let h1 = h(self, "crosshair");
                self.place("invert_pitch", 0x118, h1 + 0xdc);
                let h2 = h(self, "invert_pitch");
                self.place("subtitle", 0x118, h1 + 0xf0 + h2);
                self.place("back", 0x118, 0x1c2);
            }
            SOUND => {
                // @ 0x00420b80: each title at x 280, its `-` at 440, its
                // level at 480, its `+` at 580, 5 below the title; the
                // next title 20 below the level; from y 180.
                let mut y = 0xb4;
                for (title, sub, level, add) in [
                    ("master_title", "master_sub", "master_level", "master_add"),
                    ("music_title", "music_sub", "music_level", "music_add"),
                    ("voice_title", "voice_sub", "voice_level", "voice_add"),
                    ("fx_title", "fx_sub", "fx_level", "fx_add"),
                ] {
                    self.place(title, 0x118, y);
                    let row = y + h(self, title) + 5;
                    self.place(sub, 0x1b8, row);
                    self.place(level, 0x1e0, row);
                    self.place(add, 0x244, row);
                    y = row + h(self, level) + 0x14;
                }
                self.place("back", 0x118, 0x1c2);
            }
            DEATH => {
                // @ 0x00410a00 state 0: the button at (W − 100, H − 100),
                // the text at (150, H/2 − its height/2 − 50).
                self.place("button", W - 100, H - 100);
                let th = h(self, "text");
                self.place("text", 0x96, H / 2 - th / 2 - 0x32);
            }
            EXAMINE => {
                // @ 0x004138f0 state 0: the object at (730, H/2), the
                // description at (80, 120) scrolled to 0, `up` at
                // (140, −64) and `down` at (140, 370) in it.
                self.place("object", 0x2da, H / 2);
                self.place("description", 0x50, 0x78);
                self.place("up", 0x8c, -0x40);
                self.place("down", 0x8c, 0x172);
            }
            CREDITS => {
                // Laid out by the page (`credit_page`).
                self.laid_out = false;
            }
            _ => {}
        }
    }

    /// The draw's measure (+0x28, @ 0x00451600): each shown text's height
    /// at its width; a hidden one is not drawn and keeps its last.
    fn measure(&mut self, fonts: &Fonts) {
        self.heights.resize(self.elements.len(), 0);
        for i in 0..self.elements.len() {
            if !self.visible(i) {
                continue;
            }
            if let Kind::Text(t) = &self.elements[i].kind {
                self.heights[i] = fonts.text_height(&t.font, &t.text, t.width);
            }
        }
    }

    /// The order an event goes through the elements (@ 0x004523e0): the
    /// background's children from the list's head, each new child put at
    /// the head (@ 0x00452140 with index 0), so the last made first; an
    /// element's own children before it (its process calls @ 0x004523e0
    /// first); a hidden element and its children skipped (@ 0x004520f0).
    /// The elements are kept in the ctors' order.
    fn walk(&self) -> Vec<usize> {
        fn under(m: &Menu, parent: Option<usize>, out: &mut Vec<usize>) {
            for i in (0..m.elements.len()).rev() {
                let e = &m.elements[i];
                if e.parent != parent || !e.visible {
                    continue;
                }
                under(m, Some(i), out);
                out.push(i);
            }
        }
        let mut out = Vec::new();
        under(self, None, &mut out);
        out
    }

    /// The mouse's moves (event kind 0, +0xc 2), through the elements in
    /// the walk's order: a text button out of 0 under the mouse turns 1,
    /// at 1 away from it 0 (@ 0x0044eb60); a sprite button at 0 under the
    /// mouse turns 1 (its focus loop from frame 0), at 1 away from it 0
    /// (@ 0x0044f0e0). A button that turns 1 returns 1 and the walk stops
    /// there (@ 0x004523e0): the elements after it do not see the move,
    /// so one hot before stays hot until the next move.
    fn hover(&mut self, mouse: (i32, i32)) {
        for i in self.walk() {
            let inside = self.inside_cached(i, mouse);
            match &mut self.elements[i].kind {
                Kind::Text(t) if t.hover.is_some() => {
                    if !t.over && inside {
                        t.over = true;
                        return;
                    }
                    if t.over && !inside {
                        t.over = false;
                    }
                }
                Kind::Button(b) => match b.state {
                    0 if inside => {
                        b.state = 1;
                        b.time = 0.0;
                        return;
                    }
                    1 if !inside => b.state = 0,
                    _ => {}
                },
                _ => {}
            }
        }
    }

    /// Whether the mouse is in element `i`'s box, both ends included
    /// (@ 0x00452020), its height as last drawn.
    fn inside_cached(&self, i: usize, (mx, my): (i32, i32)) -> bool {
        let (x, y) = self.abs(i);
        let (w, h) = self.cached_size(i);
        (x..=x + w).contains(&mx) && (y..=y + h).contains(&my)
    }

    fn cached_size(&self, i: usize) -> (i32, i32) {
        match &self.elements[i].kind {
            Kind::Text(t) => (
                t.width,
                t.scroll_box
                    .map_or(self.heights.get(i).copied().unwrap_or(0), |b| b.0),
            ),
            Kind::Button(b) => b
                .click
                .as_ref()
                .and_then(|b| b.info(0))
                .map_or((0, 0), |s| (i32::from(s.w), i32::from(s.h))),
            Kind::Sprite(_) => (0, 0),
        }
    }

    /// A press of the left button, through the elements in the walk's
    /// order, the first that takes it: a text button at 1 is clicked; a
    /// sprite button at 1 turns 2 (its click played once from frame 0);
    /// a sprite button at 2 takes it too (@ 0x0044f0e0).
    fn press(&mut self) -> Press {
        for i in self.walk() {
            let e = &mut self.elements[i];
            match &mut e.kind {
                Kind::Text(t) if t.over => return Press::Clicked(e.name),
                Kind::Button(b) if b.state == 1 => {
                    b.state = 2;
                    b.time = 0.0;
                    return Press::Taken;
                }
                Kind::Button(b) if b.state == 2 => return Press::Taken,
                _ => {}
            }
        }
        Press::None
    }

    /// The sprite buttons' players; a click that ended its picture
    /// clicks the button (back to 1 when the mouse is still on it).
    fn animate(&mut self, dt: f64, mouse: (i32, i32)) -> Vec<&'static str> {
        let mut out = Vec::new();
        for i in 0..self.elements.len() {
            let inside = self.inside_cached(i, mouse);
            let e = &mut self.elements[i];
            let Kind::Button(b) = &mut e.kind else {
                continue;
            };
            b.time += dt;
            if b.state == 2 {
                let n = b.click.as_ref().map_or(1, Bank::len).max(1);
                #[allow(clippy::cast_precision_loss)]
                if b.time > n as f64 / 15.0 {
                    if inside {
                        b.state = 1;
                        b.time = 0.0;
                    }
                    out.push(e.name);
                }
            }
        }
        out
    }

    /// One step of the scroll box (@ 0x00450f00 / 0x00450f20): the step
    /// is a line (the height of `a`, 0x0066ea30, at 1024) when the text
    /// is taller than the box, else 0.
    fn scroll(&mut self, down: bool, fonts: &Fonts) {
        let Some(i) = self.index("description") else {
            return;
        };
        let Kind::Text(t) = &mut self.elements[i].kind else {
            return;
        };
        let Some((bh, s)) = t.scroll_box else { return };
        let total = fonts.text_height(&t.font, &t.text, t.width).max(bh);
        let step = if total > bh {
            fonts.text_height(&t.font, "a", 0x400)
        } else {
            0
        };
        let s = if down {
            s + step.min(total - bh - s)
        } else {
            s - step.min(s)
        };
        t.scroll_box = Some((bh, s));
    }

    fn draw(&self, out: &mut [u32], fonts: &Fonts) {
        for i in 0..self.elements.len() {
            if !self.visible(i) {
                continue;
            }
            let (x, y) = self.abs(i);
            match &self.elements[i].kind {
                Kind::Text(t) => {
                    let color = t.shown_color();
                    match t.scroll_box {
                        None => {
                            let h = fonts.text_height(&t.font, &t.text, t.width);
                            let zone = Zone::new(0, 0, 0, 0, halign(t.align), 0);
                            let lines = fonts.lines(&t.font, &t.text, t.width, h.max(1), zone);
                            fonts.draw(
                                &t.font,
                                &lines,
                                rgb(color),
                                out,
                                VIEW_WIDTH,
                                VIEW_HEIGHT,
                                x,
                                y,
                            );
                        }
                        Some((bh, s)) => {
                            // @ 0x00450c10: the text laid out in a box its
                            // width and max(its height, the box's), left,
                            // zone valign 2 (`DT_VCENTER`, no effect on a
                            // multi-line `DrawTextW`: at the top [INFERRED:
                            // Win32's rule, "used only with DT_SINGLELINE"]),
                            // on a surface filled with the key 0xffffffff
                            // (@ 0x0047c2d0); the rows s..s + bh blitted
                            // with that key (@ 0x00453270).
                            let th = fonts.text_height(&t.font, &t.text, t.width).max(bh);
                            let zone = Zone::new(0, 0, 0, 0, 0, 0);
                            let lines = fonts.lines(&t.font, &t.text, t.width, th, zone);
                            let mut pic =
                                vec![0x00ff_ffffu32; t.width.max(1) as usize * th.max(1) as usize];
                            #[allow(clippy::cast_sign_loss)]
                            fonts.draw(
                                &t.font,
                                &lines,
                                rgb(color),
                                &mut pic,
                                t.width as usize,
                                th as usize,
                                0,
                                0,
                            );
                            #[allow(clippy::cast_sign_loss)]
                            for row in 0..bh {
                                let (sy, dy) = (row + s, y + row);
                                if sy >= th || !(0..H).contains(&dy) {
                                    continue;
                                }
                                for col in 0..t.width {
                                    let dx = x + col;
                                    if !(0..W).contains(&dx) {
                                        continue;
                                    }
                                    let p = pic[(sy * t.width + col) as usize] & 0x00ff_ffff;
                                    if p != 0x00ff_ffff {
                                        out[(dy * W + dx) as usize] = p;
                                    }
                                }
                            }
                        }
                    }
                }
                Kind::Button(b) => {
                    if let Some((bank, frame)) = b.shown() {
                        let mut tg = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
                        bank.draw(&mut tg, Some(x), y, frame);
                    }
                }
                Kind::Sprite(Some(bank)) => {
                    let mut tg = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
                    bank.draw(&mut tg, Some(x), y, 0);
                }
                Kind::Sprite(None) => {}
            }
        }
    }

    /// An element is drawn when it and its parents are shown.
    fn visible(&self, i: usize) -> bool {
        let e = &self.elements[i];
        e.visible && e.parent.is_none_or(|p| self.visible(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_alignments_and_modes() {
        assert_eq!(rgb(0xff), 0x00ff_0000, "COLORREF 0xff is red");
        assert_eq!(rgb(1), 0x0001_0000);
        assert_eq!((halign(0), halign(1), halign(2)), (0, 2, 1));
        assert!(hides_view(MAIN) && hides_view(CONTROL) && hides_view(DEATH));
        assert!(hides_view(EXAMINE));
        assert!(!hides_view(INVENTORY) && !hides_view(SUBJECTS) && !hides_view(POKER));
        assert!(pauses_sound(IN_GAME) && pauses_sound(DEATH));
        assert!(!pauses_sound(EXAMINE) && !pauses_sound(INVENTORY));
    }

    #[test]
    fn the_configurations_defaults_and_reset() {
        let mut c = Config::default();
        assert_eq!((c.screen_size, c.graphic_quality), (0, 1));
        assert!(c.crosshair && !c.invert_mouse && !c.subtitles);
        c.master = 0.2;
        c.subtitles = true;
        c.invert_mouse = true;
        c.screen_size = 1;
        c.reset_for_new_session();
        assert_eq!(c.master, 1.0);
        assert!(!c.subtitles && !c.invert_mouse);
        assert_eq!(c.screen_size, 1, "kept");
    }

    #[test]
    fn levels_and_dates() {
        assert_eq!(level_text(1.0), "100%");
        assert_eq!(level_text(1.0 - f32::from_bits(0x3dcc_ccbd)), "90%");
        assert_eq!(level_text(0.0), "0%");
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        // 2026-11-22 08:14:54 UTC.
        assert_eq!(civil(1_795_335_294), (2026, 11, 22, 8, 14, 54));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn slot_names() {
        assert_eq!(Menus::slot_of("save_7"), Some(7));
        assert_eq!(Menus::slot_of("back"), None);
        assert_eq!(CREDIT_KEYS.len(), 30);
    }
}
