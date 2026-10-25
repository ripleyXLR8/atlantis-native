//! The menus: the `CMnMenu` pages (@ 0x0046ff70), their widgets, and the
//! menu loop's state machine (loop type 5, @ 0x00486eb0).
//!
//! A page is a 640 x 480 background (a TGA of `spr\menu\`, or the game
//! screen at half brightness) and widgets in creation order, each a
//! sprite bank of `spr\menu\` at a fixed position with a tooltip from
//! `loc\Menu.txt`. Every frame the page is drawn (background, every
//! visible widget, the cursor `curseur.spr` at the mouse), the widgets are
//! asked in order for an action (the first non-zero one wins and takes
//! the click; a click no widget takes is 0x13), and the state machine acts
//! on it: the slides of the main page's panels, the questions of the
//! yes / no box, the pages, the saves, the options and the volumes.
//!
//! What a menu does outside itself (a save, a load, a new game, the
//! volumes, quitting) goes through [`MenuHost`], which the engine
//! implements.

use std::collections::HashMap;
use std::sync::Arc;

use atlantis2_data::dialog::FontTxt;
use atlantis2_data::menu_txt::{MenuTxt, Section};
use atlantis2_data::save::{SaveHeader, SystemTime};
use atlantis2_data::spr::{SprBank, Target};
use atlantis2_data::tga::{Bgr24, Tga};
use atlantis2_data::{A2Files, Folder};

use crate::credits::{self, Credits};
use crate::fpu::{ftol, Fpu};
use crate::hd::{HdTexts, TextFont};
use crate::subtitle::{SubtitleFont, TextOut};

/// The size every page is laid out for.
pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;

/// The actions widgets return (@ 0x00486eb0's cases).
pub mod action {
    pub const BACK: i32 = 1;
    pub const OPTIONS: i32 = 2;
    pub const SOUND: i32 = 3;
    pub const LOAD_SAVE: i32 = 4;
    /// 5..9: players 0..4.
    pub const PLAYER: i32 = 5;
    pub const MARKER: i32 = 10;
    pub const INVERT_Y: i32 = 11;
    pub const SUBTITLES: i32 = 12;
    pub const ERASE: i32 = 13;
    pub const RESOLUTION: i32 = 14;
    pub const GENERAL: i32 = 15;
    pub const MUSIC: i32 = 16;
    pub const EFFECTS: i32 = 17;
    pub const DIALOGS: i32 = 18;
    pub const LIST: i32 = 20;
    pub const SAVE: i32 = 21;
    pub const CLOSE: i32 = 22;
    pub const QUIT: i32 = 23;
    pub const NO: i32 = 24;
    pub const YES: i32 = 25;
    /// A click no widget took (@ 0x00472320).
    pub const ELSEWHERE: i32 = 0x13;
    /// In the credits: a key or a click, and the end of the pages.
    pub const CREDITS_KEY: i32 = 0x17;
    pub const CREDITS_END: i32 = 0x1a;
}

/// The menu states (@ 0x006d174c).
pub mod state {
    pub const IDLE: i32 = 0;
    pub const OPTIONS_SLIDE: i32 = 1;
    pub const SOUND_SLIDE: i32 = 2;
    pub const DISC_SLIDE: i32 = 3;
    pub const DISC_BACK: i32 = 4;
    pub const ASK_SAVE: i32 = 5;
    pub const ASK_ERASE: i32 = 6;
    pub const ASK_QUIT: i32 = 7;
    pub const LOAD_OR_SAVE: i32 = 8;
    pub const CD: i32 = 9;
    pub const PLAYER: i32 = 10;
    pub const CREDITS: i32 = 11;
}

/// The pages (`AtlMenu_start*` and their builders).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    /// The player menu (@ 0x00489080): five symbols and quit.
    Player,
    /// The main menu (@ 0x00489770).
    Main,
    /// The options (@ 0x0048a050).
    Options,
    /// The four volumes (@ 0x0048a480).
    Sound,
    /// The saves of the player (@ 0x0048abf0).
    Load,
    /// "Insert disc N" (@ 0x0048b130).
    Cd(u8),
    /// "Please wait" (@ 0x0048b290).
    PleaseWait,
    /// "Disk full" and "write error" (@ 0x0048b4b0, 0x0048b5f0).
    DiskFull,
    DiskError,
    /// The credits (@ 0x0048b730).
    Credits,
}

/// The options and volumes the menus change (the globals of
/// `atlant2.cfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub subtitles: bool,
    pub centre_marker: bool,
    pub invert_y: bool,
    /// The screen size, 640 x 480 or 800 x 600.
    pub width: i32,
    pub height: i32,
    /// 0..256 (@ 0x005dd2b8).
    pub general: i32,
    /// 0..127 (@ 0x005dc9dc).
    pub music: i32,
    /// 0..127 (@ 0x005dd2bc).
    pub effects: i32,
    /// 0..127 (@ 0x005df974).
    pub dialogs: i32,
}

/// What the menus do outside themselves.
pub trait MenuHost {
    fn files(&self) -> &A2Files;
    /// The x87 control word of the float code.
    fn fpu(&self) -> Fpu;
    /// The 15 Hz counter (@ 0x006569d8) and the clock in seconds (the
    /// performance counter of the credits' timing).
    fn ticks(&self) -> u32;
    fn clock(&self) -> f64;
    /// The loop type becomes the menu's, the current one kept
    /// (@ 0x006d16e0); `AtlMenu_stop` (@ 0x00489040) puts it back.
    fn enter_menu_loop(&mut self);
    fn leave_menu_loop(&mut self);
    /// The current game screen (for the pages drawn over it).
    fn game_screen(&mut self) -> Vec<u16>;
    fn settings(&self) -> Settings;
    fn set_settings(&mut self, s: Settings);
    /// FUN_00478100 then FUN_00476f80: the music volume, and the current
    /// music's volume set to it at once.
    fn set_music_volume(&mut self, v: i32);
    /// The player number (@ 0x005dcf70).
    fn set_player(&mut self, player: u8);
    fn player(&self) -> u8;
    /// The headers of the player's valid saves, by slot (0..98).
    fn saves(&mut self) -> Vec<(u8, SaveHeader)>;
    /// Saves slot `slot` of the player (FUN_00478df0).
    fn save(&mut self, slot: u8);
    /// Loads slot `slot` of the player (FUN_00478f30).
    fn load(&mut self, slot: u8);
    /// Deletes slot `slot` of the player (FUN_00479260).
    fn delete(&mut self, slot: u8);
    /// The start-up chapter's `/INIT` (FUN_00484040).
    fn new_game(&mut self);
    /// The program ends (FUN_005853e0).
    fn quit(&mut self);
    /// Whether disc `n` is in (FUN_0040e090).
    fn disc_present(&mut self, n: u8) -> bool {
        let _ = n;
        true
    }
    /// The disc asked for is in: a load waiting for it finishes
    /// (FUN_00478580), else an interrupted section resumes.
    fn disc_inserted(&mut self) {}
    /// An interrupted script section resumes (@ 0x006d1699,
    /// FUN_00483f90); false when there is none.
    fn resume_section(&mut self) -> bool {
        false
    }
}

/// The mouse and keys as the menu reads them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// The pointer (@ 0x006569c0, 0x006569bc), clamped to the screen.
    pub x: i32,
    pub y: i32,
    /// A left click to handle (@ 0x006569c9), and the button held
    /// (@ 0x006569ca).
    pub click: bool,
    pub held: bool,
    /// Escape held (the credits' test, @ 0x00472378: the key state
    /// @ 0x006e2b6c of the key table's code 1 @ 0x006e3174).
    pub escape: bool,
}

/// A widget's hit box: `left < x < right` and `top < y < bottom`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn holds(&self, x: i32, y: i32) -> bool {
        self.left < x && x < self.right && self.top < y && y < self.bottom
    }
}

/// One save of the load page's list (24 bytes in the original:
/// thumbnail, `SYSTEMTIME`, slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListEntry {
    pub vignette: i32,
    pub time: SystemTime,
    pub slot: u8,
}

/// The kinds of widgets (their classes and vtables).
#[derive(Clone, Debug)]
pub enum WidgetKind {
    /// Frame 0, never hit (vtable @ 0x005c666c).
    Sprite,
    /// Frame 0 (vtable @ 0x005c63e4).
    Button,
    /// Frame 1 while the mouse is in the box (vtable @ 0x005c64ac).
    Hover,
    /// The frames played while hovered with the button up, while pressed,
    /// or always (vtable @ 0x005c6424).
    Animated {
        count: i32,
        frame: i32,
        pressed: bool,
        always: bool,
    },
    /// Frame 0 off, 1 on, a click flips it (vtable @ 0x005c64ec).
    Toggle {
        off: Vec<u8>,
        on: Vec<u8>,
        state: bool,
    },
    /// Blinks while on (vtable @ 0x005c6468).
    Blink { state: bool, cycle: u32 },
    /// A volume: the gauge `g_son.spr` and the knob `m_son.spr` (vtable
    /// @ 0x005c65fc); `value` is an index into [`Menu::values`].
    Slider {
        value: usize,
        grabbed: bool,
        gauge: Option<Arc<SprBank>>,
    },
    /// The saves (vtable @ 0x005c6560) with their scroll bar `Glisse`
    /// (vtable @ 0x005c6638).
    List(Box<ListBox>),
}

/// The load page's list.
#[derive(Clone, Debug)]
pub struct ListBox {
    pub w: i32,
    pub h: i32,
    /// Newest first.
    pub entries: Vec<ListEntry>,
    /// The scroll bar's value, 0..1.
    pub scroll: f32,
    /// The scroll bar: its position, track and banks.
    bar: ScrollBar,
}

#[derive(Clone, Debug)]
struct ScrollBar {
    x: i32,
    y: i32,
    /// +0x30, +0x34: the track's x and top.
    track_x: i32,
    track_y: i32,
    /// +0x38, +0x3c: the box's width and the track's length.
    width: i32,
    length: i32,
    /// +0x44, +0x48: the knob's offset from the track.
    knob_dx: i32,
    knob_dy: i32,
    grabbed: bool,
    gauge: Option<Arc<SprBank>>,
    knob: Option<Arc<SprBank>>,
}

/// One widget.
#[derive(Clone, Debug)]
pub struct Widget {
    /// Its sprite file's name, by which the pages find it.
    pub name: String,
    pub kind: WidgetKind,
    pub x: i32,
    pub y: i32,
    /// The tooltip.
    pub text: Vec<u8>,
    pub enabled: bool,
    pub visible: bool,
    pub action: i32,
    pub hit: Rect,
    bank: Option<Arc<SprBank>>,
    /// The tick the mouse entered the box (the tooltip's count).
    hover_since: Option<u32>,
}

/// A page (`CMnMenu`, 0x1e8 bytes).
#[derive(Clone, Debug)]
pub struct Page {
    pub kind: PageKind,
    /// 640 x 480 RGB565.
    pub background: Vec<u16>,
    pub widgets: Vec<Widget>,
    credits: Option<Credits>,
}

/// The menu system: its texts and banks (`AtlMenu_MainInit`
/// @ 0x00485c00), the page shown and the state machine's globals.
pub struct Menu {
    pub texts: MenuTxt,
    cursor: Option<SprBank>,
    font: Option<SubtitleFont>,
    /// The HD mode's text recorder (a port addition), lent by the engine
    /// for a frame: the texts the page draws are recorded in it.
    pub hd_texts: Option<HdTexts>,
    banks: HashMap<String, Option<Arc<SprBank>>>,
    pub page: Option<Page>,
    /// @ 0x006d174c and its counter @ 0x006d1750.
    pub state: i32,
    pub counter: i32,
    /// The main menu opened from the game: Escape closes it
    /// (@ 0x006d1741).
    pub from_game: bool,
    /// The cursor is drawn (@ 0x006d1758).
    pub cursor_shown: bool,
    /// The load page's erase mode (@ 0x006d1759).
    pub erase_mode: bool,
    /// The sliders' values: general, music (relative to the general one),
    /// dialogs, effects.
    pub values: [f32; 4],
    /// The list entry clicked (@ 0x006d16a0, its slot @ 0x006d16b4).
    pub chosen: Option<ListEntry>,
    /// The player chosen on the player page.
    player: u8,
    /// The disc the CD page asks for, and the tick of the last check.
    cd_disc: u8,
    cd_check: u32,
    /// The 15 Hz flag the animated buttons step on (@ 0x00656a50).
    pub tick_flag: bool,
    pub input: Input,
}

impl std::fmt::Debug for Menu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Menu")
            .field("page", &self.page.as_ref().map(|p| p.kind))
            .field("state", &self.state)
            .finish()
    }
}

/// The slider values' slots.
const GENERAL: usize = 0;
const MUSIC: usize = 1;
const DIALOGS: usize = 2;
const EFFECTS: usize = 3;

/// A text of `Menu.txt` as bytes.
fn t(texts: &MenuTxt, s: Section, i: usize) -> Vec<u8> {
    texts.line(s, i).to_vec()
}

impl Menu {
    /// The options page's resolution button after the screen size was
    /// chosen elsewhere (the port's options menu): its text names the size
    /// it switches to, as action 14 leaves it (Options[8] at 800 x 600,
    /// Options[9] at 640 x 480).
    pub fn screen_changed(&mut self, width: i32) {
        let text = t(
            &self.texts,
            Section::Options,
            if width == 800 { 8 } else { 9 },
        );
        if let Some(p) = self.page.as_mut() {
            if p.widget("b_rezo.spr").is_some() {
                p.set_text("b_rezo.spr", text);
            }
        }
    }

    /// `AtlMenu_MainInit` (@ 0x00485c00): `loc\Menu.txt` (fatal when
    /// missing in the original; empty texts here), `curseur.spr`, and the
    /// popup font (`AtlFT_POPUP` of `loc\Font.txt`).
    pub fn new(files: &A2Files) -> Menu {
        let texts = match files.read(Folder::Loc, "Menu.txt") {
            Some(d) => MenuTxt::parse(&d),
            None => {
                log::warn!("loc\\Menu.txt not found: the menus have no texts");
                MenuTxt::parse(b"")
            }
        };
        let cursor = files
            .read(Folder::SprMenu, "curseur.spr")
            .and_then(|d| SprBank::parse(&d).ok());
        let font = files
            .read(Folder::Loc, "Font.txt")
            .map(|d| FontTxt::parse(&d))
            .and_then(|f| {
                let file = f.font("AtlFT_POPUP")?.file.clone();
                SubtitleFont::named(files.read(Folder::Loc, &file)?, &f, "AtlFT_POPUP")
            });
        if font.is_none() {
            log::warn!("AtlFT_POPUP not found: the menus draw no text");
        }
        Menu {
            texts,
            cursor,
            font,
            hd_texts: None,
            banks: HashMap::new(),
            page: None,
            state: 0,
            counter: 0,
            from_game: false,
            cursor_shown: false,
            erase_mode: false,
            values: [1.0; 4],
            chosen: None,
            player: 0,
            cd_disc: 1,
            cd_check: 0,
            tick_flag: false,
            input: Input::default(),
        }
    }

    /// Whether a page is shown.
    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }

    pub fn kind(&self) -> Option<PageKind> {
        self.page.as_ref().map(|p| p.kind)
    }

    /// The bank `spr\menu\<name>`, loaded once.
    fn bank(&mut self, files: &A2Files, name: &str) -> Option<Arc<SprBank>> {
        self.banks
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| {
                let b = files
                    .read(Folder::SprMenu, name)
                    .and_then(|d| SprBank::parse(&d).ok());
                if b.is_none() {
                    log::warn!("spr\\menu\\{name} missing or unreadable");
                }
                b.map(Arc::new)
            })
            .clone()
    }

    // --- Opening and closing (`AtlMenu_start*`, `AtlMenu_stop`) ---

    /// The slider values from the volumes (at every opening).
    fn values_from(&mut self, fpu: Fpu, s: &Settings) {
        let general = fpu.f32(fpu.div(f64::from(s.general), 256.0));
        let music = fpu.f32(fpu.div(fpu.div(f64::from(s.music), 127.0), f64::from(general)));
        self.values = [
            general,
            music,
            fpu.f32(fpu.div(f64::from(s.dialogs), 127.0)),
            fpu.f32(fpu.div(f64::from(s.effects), 127.0)),
        ];
    }

    fn start(&mut self, host: &mut dyn MenuHost, page: PageKind, enter_loop: bool) {
        if enter_loop {
            host.enter_menu_loop();
        }
        let s = host.settings();
        self.values_from(host.fpu(), &s);
        self.state = state::IDLE;
        self.from_game = false;
        self.cursor_shown = !matches!(page, PageKind::PleaseWait | PageKind::Credits);
        self.build(host, page);
    }

    /// `AtlMenu_start` (@ 0x00486930): the main menu, from the game
    /// (Escape, @ 0x004762f0).
    pub fn start_main(&mut self, host: &mut dyn MenuHost) {
        self.start(host, PageKind::Main, true);
        self.from_game = true;
    }

    /// `AtlMenu_start_Player` (@ 0x00486af0), at start-up.
    pub fn start_player(&mut self, host: &mut dyn MenuHost) {
        self.start(host, PageKind::Player, true);
    }

    /// `AtlMenu_start_CD` (@ 0x004869f0): disc `n` is asked for.
    pub fn start_cd(&mut self, host: &mut dyn MenuHost, n: u8) {
        self.cd_disc = n;
        self.cd_check = host.ticks();
        self.start(host, PageKind::Cd(n), true);
        self.state = state::CD;
    }

    /// `AtlMenu_start_PleaseWait` (@ 0x00486bb0): the loop type is not
    /// changed.
    pub fn start_please_wait(&mut self, host: &mut dyn MenuHost) {
        self.start(host, PageKind::PleaseWait, false);
    }

    /// `AtlMenu_start_DiskFull` / `_DiskError` (@ 0x00486c60,
    /// 0x00486d10).
    pub fn start_disk_full(&mut self, host: &mut dyn MenuHost) {
        self.start(host, PageKind::DiskFull, true);
    }

    pub fn start_disk_error(&mut self, host: &mut dyn MenuHost) {
        self.start(host, PageKind::DiskError, true);
    }

    /// `AtlMenu_start_Credits` (@ 0x00486e00).
    pub fn start_credits(&mut self, host: &mut dyn MenuHost) {
        self.start(host, PageKind::Credits, true);
        self.state = state::CREDITS;
    }

    /// `AtlMenu_stop` (@ 0x00489040): the page deleted, the loop type
    /// back.
    pub fn stop(&mut self, host: &mut dyn MenuHost) {
        if let Some(p) = self.page.take() {
            if p.kind != PageKind::PleaseWait {
                host.leave_menu_loop();
            }
        }
        self.state = state::IDLE;
    }

    /// Escape in the menu loop (@ 0x004762f0): closes the main menu
    /// opened from the game.
    pub fn escape(&mut self, host: &mut dyn MenuHost) {
        if self.from_game && self.page.is_some() {
            self.stop(host);
        }
    }

    // --- The pages ---

    /// Shows page `kind` in state 0 (the pages the slides lead to).
    pub fn show_page(&mut self, host: &mut dyn MenuHost, kind: PageKind) {
        self.state = state::IDLE;
        self.build(host, kind);
    }

    fn build(&mut self, host: &mut dyn MenuHost, kind: PageKind) {
        let files = host.files();
        let background = match kind {
            PageKind::Player => self.tga(files, "f_player.tga"),
            PageKind::Main => self.tga(files, "f_main.tga"),
            PageKind::Options => self.tga(files, "f_option.tga"),
            PageKind::Sound => self.tga(files, "f_son.tga"),
            PageKind::Load => self.tga(files, "f_load.tga"),
            PageKind::Credits => vec![0; WIDTH * HEIGHT],
            _ => Vec::new(),
        };
        let mut page = Page {
            kind,
            background,
            widgets: Vec::new(),
            credits: None,
        };
        if page.background.is_empty() {
            page.background = dim(&host.game_screen());
        }
        let tx = self.texts.clone();
        let m = Section::Main;
        match kind {
            PageKind::Player => {
                let p = Section::Player;
                self.sprite(host, &mut page, "c_player.spr", 320, 240);
                self.animated(
                    host,
                    &mut page,
                    "a_exit.spr",
                    t(&tx, p, 1),
                    20,
                    600,
                    440,
                    action::QUIT,
                );
                let symbols = [
                    ("b_gala.spr", 277, 146),
                    ("b_lune.spr", 335, 133),
                    ("b_sol.spr", 395, 129),
                    ("b_star.spr", 453, 138),
                    ("b_ter.spr", 504, 171),
                ];
                for (i, (name, x, y)) in symbols.into_iter().enumerate() {
                    let text = t(&tx, p, 2 + i);
                    self.button(
                        host,
                        &mut page,
                        name,
                        text,
                        x,
                        y,
                        action::PLAYER + i as i32,
                        true,
                    );
                }
                self.yes_no(host, &mut page);
            }
            PageKind::Main => {
                self.animated(
                    host,
                    &mut page,
                    "a_load.spr",
                    t(&tx, m, 5),
                    20,
                    120,
                    270,
                    action::LOAD_SAVE,
                );
                self.animated(
                    host,
                    &mut page,
                    "a_save.spr",
                    t(&tx, m, 6),
                    20,
                    120,
                    370,
                    action::SAVE,
                );
                for w in page.widgets.iter_mut() {
                    w.visible = false;
                }
                self.sprite(host, &mut page, "c_opt.spr", 320, 240);
                self.sprite(host, &mut page, "c_son.spr", 320, 240);
                self.sprite(host, &mut page, "c_load.spr", 320, 240);
                self.animated(
                    host,
                    &mut page,
                    "a_exit.spr",
                    t(&tx, m, 1),
                    20,
                    600,
                    440,
                    action::QUIT,
                );
                self.animated(
                    host,
                    &mut page,
                    "a_toolbx.spr",
                    t(&tx, m, 2),
                    20,
                    550,
                    80,
                    action::OPTIONS,
                );
                self.animated(
                    host,
                    &mut page,
                    "a_baffle.spr",
                    t(&tx, m, 3),
                    20,
                    390,
                    250,
                    action::SOUND,
                );
                self.animated(
                    host,
                    &mut page,
                    "a_disc.spr",
                    t(&tx, m, 4),
                    20,
                    130,
                    270,
                    action::LOAD_SAVE,
                );
                page.set_hit(
                    "a_disc.spr",
                    Rect {
                        left: 70,
                        top: 250,
                        right: 135,
                        bottom: 310,
                    },
                );
                self.yes_no(host, &mut page);
            }
            PageKind::Options => {
                let o = Section::Options;
                let s = host.settings();
                self.button(
                    host,
                    &mut page,
                    "b_back.spr",
                    t(&tx, o, 1),
                    600,
                    440,
                    action::BACK,
                    false,
                );
                self.toggle(
                    host,
                    &mut page,
                    "s_sub.spr",
                    (t(&tx, o, 2), t(&tx, o, 3)),
                    230,
                    200,
                    s.subtitles,
                    action::SUBTITLES,
                );
                self.toggle(
                    host,
                    &mut page,
                    "s_cross.spr",
                    (t(&tx, o, 4), t(&tx, o, 5)),
                    300,
                    300,
                    s.centre_marker,
                    action::MARKER,
                );
                self.toggle(
                    host,
                    &mut page,
                    "s_xyinv.spr",
                    (t(&tx, o, 6), t(&tx, o, 7)),
                    430,
                    300,
                    s.invert_y,
                    action::INVERT_Y,
                );
                // Only on a CPU with MMX (CPUID(1).EDX bit 23, FUN_004db490):
                // every CPU the port runs on.
                let text = t(&tx, o, if s.width == 800 { 8 } else { 9 });
                self.button(
                    host,
                    &mut page,
                    "b_rezo.spr",
                    text,
                    430,
                    205,
                    action::RESOLUTION,
                    false,
                );
            }
            PageKind::Sound => {
                let so = Section::Sound;
                self.button(
                    host,
                    &mut page,
                    "b_back.spr",
                    t(&tx, so, 1),
                    600,
                    440,
                    action::BACK,
                    false,
                );
                let sliders = [
                    (
                        "MasterVolume",
                        425,
                        140,
                        GENERAL,
                        action::GENERAL,
                        "a_baffle.spr",
                        300,
                        140,
                        Rect {
                            left: 250,
                            top: 90,
                            right: 500,
                            bottom: 150,
                        },
                    ),
                    (
                        "MusicVolume",
                        300,
                        215,
                        MUSIC,
                        action::MUSIC,
                        "a_harpe.spr",
                        425,
                        215,
                        Rect {
                            left: 210,
                            top: 175,
                            right: 450,
                            bottom: 235,
                        },
                    ),
                    (
                        "DialogVolume",
                        450,
                        290,
                        DIALOGS,
                        action::DIALOGS,
                        "a_hp.spr",
                        325,
                        290,
                        Rect {
                            left: 300,
                            top: 260,
                            right: 525,
                            bottom: 300,
                        },
                    ),
                    (
                        "FxVolume",
                        350,
                        365,
                        EFFECTS,
                        action::EFFECTS,
                        "a_cloche.spr",
                        475,
                        365,
                        Rect {
                            left: 260,
                            top: 335,
                            right: 490,
                            bottom: 385,
                        },
                    ),
                ];
                for (i, (name, x, y, value, act, icon, ix, iy, hit)) in
                    sliders.into_iter().enumerate()
                {
                    let text = t(&tx, so, 2 + i);
                    self.slider(host, &mut page, name, text.clone(), x, y, value, act);
                    self.animated(host, &mut page, icon, text, 20, ix, iy, 0);
                    page.widgets.last_mut().unwrap().hit = hit;
                }
            }
            PageKind::Load => {
                let l = Section::Load;
                self.button(
                    host,
                    &mut page,
                    "b_back.spr",
                    t(&tx, l, 1),
                    600,
                    440,
                    action::BACK,
                    false,
                );
                self.list(host, &mut page, 108, 129, 420, 300);
                self.erase_mode = false;
                self.blink(
                    host,
                    &mut page,
                    "b_erase.spr",
                    t(&tx, l, 2),
                    590,
                    65,
                    false,
                    action::ERASE,
                );
                self.yes_no(host, &mut page);
            }
            PageKind::Cd(n) => {
                let name = format!("b_cd{n}.spr");
                let text = t(&tx, Section::Cd, usize::from(n.max(1)) - 1);
                self.button(host, &mut page, &name, text, 320, 240, 0, false);
            }
            PageKind::PleaseWait => {
                self.animated(host, &mut page, "a_wait.spr", Vec::new(), 20, 600, 420, 0);
                if let Some(WidgetKind::Animated { always, .. }) =
                    page.widgets.last_mut().map(|w| &mut w.kind)
                {
                    *always = true;
                }
            }
            PageKind::DiskFull | PageKind::DiskError => {
                let (name, line) = if kind == PageKind::DiskFull {
                    ("b_dfull.spr", 2)
                } else {
                    ("b_derr.spr", 3)
                };
                self.blink(
                    host,
                    &mut page,
                    name,
                    t(&tx, Section::Messages, line),
                    320,
                    240,
                    false,
                    action::CLOSE,
                );
            }
            PageKind::Credits => {
                // FUN_0048b730 → FUN_00470550: the 16 pictures in 24-bit
                // surfaces, the pages of `loc\Credits.txt` (a missing file
                // is fatal in the original, @ 0x00470c63: no page here, so
                // the credits end at once).
                let files = host.files();
                let pictures = (0..credits::PICTURES)
                    .map(|i| {
                        let name = credits::picture_name(i);
                        match files.read(Folder::SprMenu, &name).map(|d| Bgr24::parse(&d)) {
                            Some(Ok(t)) if t.width == WIDTH && t.height == HEIGHT => t.bytes,
                            other => {
                                log::warn!(
                                    "spr\\menu\\{name}: {:?}",
                                    other.map(|r| r.map(|_| "size"))
                                );
                                Vec::new()
                            }
                        }
                    })
                    .collect();
                let pages = match files.read(Folder::Loc, "Credits.txt") {
                    Some(d) => credits::parse(&d),
                    None => {
                        log::warn!("loc\\Credits.txt not found: no credits page");
                        Vec::new()
                    }
                };
                let font = self.font.as_ref();
                let calls = credits::layout(
                    &pages,
                    |b| font.map_or(0, |f| f.extent(b).0),
                    font.map_or(10, |f| f.line_step()),
                    WIDTH as i32,
                );
                page.credits = Some(Credits::new(pictures, &calls, font));
            }
        }
        self.page = Some(page);
    }

    /// `CMnMenu::LoadTga` (@ 0x00472830): a 640 x 480 background (black
    /// when missing or of another size; the original stops there).
    fn tga(&self, files: &A2Files, name: &str) -> Vec<u16> {
        match files.read(Folder::SprMenu, name).map(|d| Tga::parse(&d)) {
            Some(Ok(t)) if t.width == WIDTH && t.height == HEIGHT => t.pixels,
            other => {
                log::warn!("spr\\menu\\{name}: {:?}", other.map(|r| r.map(|_| "size")));
                vec![0; WIDTH * HEIGHT]
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn widget(
        &mut self,
        host: &dyn MenuHost,
        name: &str,
        kind: WidgetKind,
        x: i32,
        y: i32,
        text: Vec<u8>,
        action: i32,
    ) -> Widget {
        let bank = match kind {
            WidgetKind::Slider { .. } => self.bank(host.files(), "m_son.spr"),
            WidgetKind::List(_) => self.bank(host.files(), "save.spr"),
            _ => self.bank(host.files(), name),
        };
        // The box: frame 0's size centred on the position (LAB_0046d5e0:
        // x ± w / 2, y ± h / 2; the hot spot is not used).
        let (w, h) = bank
            .as_ref()
            .and_then(|b| b.info(0))
            .map_or((0, 0), |i| (i32::from(i.w), i32::from(i.h)));
        Widget {
            name: name.to_string(),
            kind,
            x,
            y,
            text,
            enabled: true,
            visible: true,
            action,
            hit: Rect {
                left: x - w / 2,
                top: y - h / 2,
                right: x + w / 2,
                bottom: y + h / 2,
            },
            bank,
            hover_since: None,
        }
    }

    /// FUN_00470f70.
    fn sprite(&mut self, host: &dyn MenuHost, page: &mut Page, name: &str, x: i32, y: i32) {
        let w = self.widget(host, name, WidgetKind::Sprite, x, y, Vec::new(), 0);
        page.widgets.push(w);
    }

    /// FUN_004711b0 (a button) or FUN_00471410 (`hover`).
    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        host: &dyn MenuHost,
        page: &mut Page,
        name: &str,
        text: Vec<u8>,
        x: i32,
        y: i32,
        action: i32,
        hover: bool,
    ) {
        let kind = if hover {
            WidgetKind::Hover
        } else {
            WidgetKind::Button
        };
        let w = self.widget(host, name, kind, x, y, text, action);
        page.widgets.push(w);
    }

    /// FUN_00471670.
    #[allow(clippy::too_many_arguments)]
    fn animated(
        &mut self,
        host: &dyn MenuHost,
        page: &mut Page,
        name: &str,
        text: Vec<u8>,
        count: i32,
        x: i32,
        y: i32,
        action: i32,
    ) {
        let kind = WidgetKind::Animated {
            count,
            frame: 0,
            pressed: false,
            always: false,
        };
        let w = self.widget(host, name, kind, x, y, text, action);
        page.widgets.push(w);
    }

    /// FUN_004718d0.
    #[allow(clippy::too_many_arguments)]
    fn toggle(
        &mut self,
        host: &dyn MenuHost,
        page: &mut Page,
        name: &str,
        (off, on): (Vec<u8>, Vec<u8>),
        x: i32,
        y: i32,
        state: bool,
        action: i32,
    ) {
        let text = if state { on.clone() } else { off.clone() };
        let kind = WidgetKind::Toggle { off, on, state };
        let w = self.widget(host, name, kind, x, y, text, action);
        page.widgets.push(w);
    }

    /// FUN_00471b40.
    #[allow(clippy::too_many_arguments)]
    fn blink(
        &mut self,
        host: &dyn MenuHost,
        page: &mut Page,
        name: &str,
        text: Vec<u8>,
        x: i32,
        y: i32,
        state: bool,
        action: i32,
    ) {
        let kind = WidgetKind::Blink {
            state,
            cycle: host.ticks(),
        };
        let w = self.widget(host, name, kind, x, y, text, action);
        page.widgets.push(w);
    }

    /// FUN_00471da0: the gauge `g_son.spr`, the knob `m_son.spr`.
    #[allow(clippy::too_many_arguments)]
    fn slider(
        &mut self,
        host: &dyn MenuHost,
        page: &mut Page,
        name: &str,
        text: Vec<u8>,
        x: i32,
        y: i32,
        value: usize,
        action: i32,
    ) {
        let kind = WidgetKind::Slider {
            value,
            grabbed: false,
            gauge: self.bank(host.files(), "g_son.spr"),
        };
        let w = self.widget(host, name, kind, x, y, text, action);
        page.widgets.push(w);
    }

    /// FUN_00471f60: the player's saves, newest first, with the scroll
    /// bar at the list's right (x + w, y + h / 2; its track 10 to the left
    /// and 106 up from there, 20 wide and 200 long, the knob 10 right and
    /// 6 down from the track).
    fn list(&mut self, host: &mut dyn MenuHost, page: &mut Page, x: i32, y: i32, w: i32, h: i32) {
        let player = host.player();
        host.set_player(player);
        let mut entries: Vec<ListEntry> = Vec::new();
        for (slot, hdr) in host.saves() {
            let e = ListEntry {
                vignette: hdr.vignette,
                time: hdr.time,
                slot,
            };
            // LAB_0046f420 with FUN_0040cc40: before the first older one.
            let at = entries
                .iter()
                .position(|o| e.time.key() > o.time.key())
                .unwrap_or(entries.len());
            entries.insert(at, e);
        }
        let (bx, by) = (x + w, y + h / 2);
        let bar = ScrollBar {
            x: bx,
            y: by,
            track_x: bx - 10,
            track_y: by - 106,
            width: 20,
            length: 200,
            knob_dx: 10,
            knob_dy: 6,
            grabbed: false,
            gauge: self.bank(host.files(), "g_load.spr"),
            knob: self.bank(host.files(), "m_load.spr"),
        };
        let list = ListBox {
            w,
            h,
            entries,
            scroll: 0.0,
            bar,
        };
        let mut wd = self.widget(
            host,
            "Listbox",
            WidgetKind::List(Box::new(list)),
            x,
            y,
            Vec::new(),
            action::LIST,
        );
        wd.hit = Rect {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        page.widgets.push(wd);
    }

    /// The yes / no box, hidden until a question.
    fn yes_no(&mut self, host: &dyn MenuHost, page: &mut Page) {
        self.sprite(host, page, "c_yn.spr", 320, 240);
        self.button(
            host,
            page,
            "b_oui.spr",
            Vec::new(),
            290,
            230,
            action::YES,
            false,
        );
        self.button(
            host,
            page,
            "b_non.spr",
            Vec::new(),
            340,
            240,
            action::NO,
            false,
        );
        for w in page.widgets.iter_mut().rev().take(3) {
            w.visible = false;
        }
    }

    /// A question: every widget disabled, the box shown with its texts.
    fn ask(&mut self, yes: Vec<u8>, no: Vec<u8>) {
        let Some(p) = self.page.as_mut() else { return };
        p.disable_all();
        p.show("c_yn.spr");
        p.show("b_oui.spr");
        p.show("b_non.spr");
        p.enable("b_oui.spr");
        p.enable("b_non.spr");
        p.set_text("b_oui.spr", yes);
        p.set_text("b_non.spr", no);
    }

    fn hide_question(&mut self) {
        if let Some(p) = self.page.as_mut() {
            p.hide("c_yn.spr");
            p.hide("b_oui.spr");
            p.hide("b_non.spr");
        }
    }

    // --- The frame (@ 0x00486eb0) ---

    /// One frame of the menu loop: the page drawn into `out` (640 x 480)
    /// when given, then the action of the frame and the state machine.
    pub fn frame(&mut self, host: &mut dyn MenuHost, out: Option<&mut [u16]>) {
        if self.page.is_none() {
            return;
        }
        let ticks = host.ticks();
        let clock = host.clock();
        let input = self.input;
        if let Some(p) = self.page.as_mut() {
            p.update(
                &input,
                ticks,
                std::mem::take(&mut self.tick_flag),
                host.fpu(),
                clock,
            );
        }
        if let Some(out) = out {
            let mut t = self.hd_texts.take();
            self.draw_with(out, ticks, t.as_mut());
            self.hd_texts = t;
        }
        let act = self.action(host);
        self.input.click = false;
        self.run(host, act);
    }

    /// Draws the page and the cursor (no state change).
    pub fn draw(&self, out: &mut [u16], ticks: u32) {
        self.draw_with(out, ticks, None);
    }

    /// The popup font (`AtlFT_POPUP`), when loaded.
    pub fn font(&self) -> Option<&SubtitleFont> {
        self.font.as_ref()
    }

    /// [`Menu::draw`], the texts recorded in `texts` (the HD mode).
    pub fn draw_with(&self, out: &mut [u16], ticks: u32, texts: Option<&mut HdTexts>) {
        let Some(p) = self.page.as_ref() else { return };
        p.draw(
            out,
            ticks,
            &self.values,
            self.font.as_ref(),
            &self.input,
            texts,
        );
        if self.cursor_shown {
            if let Some(c) = &self.cursor {
                let mut t = Target::screen(out, WIDTH, HEIGHT);
                c.draw(&mut t, Some(self.input.x), self.input.y, 0);
            }
        }
    }

    /// FUN_00472320: the widgets asked in order.
    fn action(&mut self, host: &mut dyn MenuHost) -> i32 {
        let fpu = host.fpu();
        let input = &mut self.input;
        let Some(p) = self.page.as_mut() else {
            return 0;
        };
        if let Some(c) = &p.credits {
            if input.escape || input.click {
                return action::CREDITS_KEY;
            }
            return if c.done { action::CREDITS_END } else { 0 };
        }
        for w in p.widgets.iter_mut() {
            let a = w.input(input, fpu, &mut self.values, &mut self.chosen);
            if a != 0 {
                input.click = false;
                return a;
            }
        }
        if input.click {
            input.click = false;
            return action::ELSEWHERE;
        }
        0
    }

    fn run(&mut self, host: &mut dyn MenuHost, act: i32) {
        match self.state {
            state::IDLE => self.idle(host, act),
            state::OPTIONS_SLIDE | state::SOUND_SLIDE => {
                self.counter += 1;
                let c = self.counter;
                let (panel, icon, frames, next) = if self.state == state::OPTIONS_SLIDE {
                    ("c_opt.spr", "a_toolbx.spr", 30, PageKind::Options)
                } else {
                    ("c_son.spr", "a_baffle.spr", 40, PageKind::Sound)
                };
                if let Some(p) = self.page.as_mut() {
                    p.move_by(panel, c, 0);
                    p.move_by(icon, c, 0);
                }
                if c >= frames {
                    self.state = state::IDLE;
                    self.build(host, next);
                }
            }
            state::DISC_SLIDE => {
                self.counter += 1;
                let c = self.counter;
                if let Some(p) = self.page.as_mut() {
                    p.move_by("c_load.spr", -c, 0);
                    p.move_by("a_disc.spr", -c, 0);
                    if c >= 30 {
                        for n in ["a_load.spr", "a_save.spr"] {
                            p.show(n);
                            p.enable(n);
                        }
                    }
                }
                if c >= 30 {
                    self.state = state::LOAD_OR_SAVE;
                }
            }
            state::DISC_BACK => {
                let c = self.counter;
                if let Some(p) = self.page.as_mut() {
                    p.move_by("c_load.spr", c, 0);
                    p.move_by("a_disc.spr", c, 0);
                }
                self.counter -= 1;
                if self.counter <= 0 {
                    self.state = state::IDLE;
                    if let Some(p) = self.page.as_mut() {
                        p.enable_all();
                        p.hide("a_load.spr");
                        p.hide("a_save.spr");
                    }
                }
            }
            state::ASK_SAVE => match act {
                action::YES => {
                    let player = host.player();
                    host.set_player(player);
                    let used: Vec<u8> = host.saves().into_iter().map(|(s, _)| s).collect();
                    let free = (0..atlantis2_data::save::SLOTS).find(|s| !used.contains(s));
                    self.stop(host);
                    match free {
                        Some(slot) => host.save(slot),
                        None => {
                            log::warn!("all 99 slots of player {player} are used: nothing saved")
                        }
                    }
                }
                action::NO => {
                    self.hide_question();
                    self.state = state::DISC_BACK;
                }
                _ => {}
            },
            state::ASK_ERASE => {
                if act == action::YES || act == action::NO {
                    if act == action::YES {
                        if let Some(e) = self.chosen {
                            host.delete(e.slot);
                        }
                    }
                    self.state = state::IDLE;
                    self.build(host, PageKind::Load);
                }
            }
            state::ASK_QUIT => match act {
                action::YES => {
                    self.stop(host);
                    self.start_credits(host);
                }
                action::NO => {
                    self.hide_question();
                    if let Some(p) = self.page.as_mut() {
                        p.enable_all();
                    }
                    self.state = state::IDLE;
                }
                _ => {}
            },
            state::LOAD_OR_SAVE => match act {
                action::LOAD_SAVE => {
                    self.state = state::IDLE;
                    self.build(host, PageKind::Load);
                }
                action::SAVE => {
                    let tx = &self.texts;
                    let (yes, no) = (t(tx, Section::Main, 7), t(tx, Section::Main, 8));
                    self.ask(yes, no);
                    self.state = state::ASK_SAVE;
                }
                action::ELSEWHERE => self.state = state::DISC_BACK,
                _ => {}
            },
            state::CD => {
                // Every 100 ticks (6.7 s) the disc is looked for.
                let now = host.ticks();
                if now.wrapping_sub(self.cd_check) >= 100 {
                    self.cd_check = now;
                    if host.disc_present(self.cd_disc) {
                        log::info!("Changement de CD => insertion CD N°{}", self.cd_disc);
                        self.stop(host);
                        host.disc_inserted();
                    }
                }
            }
            state::PLAYER => {
                let player = self.player;
                host.set_player(player);
                let newest =
                    host.saves()
                        .into_iter()
                        .fold(None::<(u8, SaveHeader)>, |best, (s, h)| match best {
                            Some((_, ref b)) if b.time.key() >= h.time.key() => best,
                            _ => Some((s, h)),
                        });
                self.stop(host);
                match newest {
                    Some((slot, _)) => host.load(slot),
                    None => host.new_game(),
                }
            }
            state::CREDITS => match act {
                action::CREDITS_KEY => host.quit(),
                action::CREDITS_END => {
                    self.stop(host);
                    if !host.resume_section() {
                        host.quit();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// The actions of state 0.
    fn idle(&mut self, host: &mut dyn MenuHost, act: i32) {
        let fpu = host.fpu();
        match act {
            action::BACK => self.build(host, PageKind::Main),
            action::OPTIONS | action::SOUND | action::LOAD_SAVE => {
                if let Some(p) = self.page.as_mut() {
                    p.disable_all();
                }
                self.counter = 0;
                self.state = match act {
                    action::OPTIONS => state::OPTIONS_SLIDE,
                    action::SOUND => state::SOUND_SLIDE,
                    _ => state::DISC_SLIDE,
                };
            }
            5..=9 => {
                self.player = (act - action::PLAYER) as u8;
                self.state = state::PLAYER;
            }
            action::MARKER | action::INVERT_Y | action::SUBTITLES => {
                let mut s = host.settings();
                match act {
                    action::MARKER => s.centre_marker = !s.centre_marker,
                    action::INVERT_Y => s.invert_y = !s.invert_y,
                    _ => s.subtitles = !s.subtitles,
                }
                host.set_settings(s);
            }
            action::ERASE => self.erase_mode = !self.erase_mode,
            action::RESOLUTION => {
                let mut s = host.settings();
                (s.width, s.height) = if s.width == 800 {
                    (640, 480)
                } else {
                    (800, 600)
                };
                host.set_settings(s);
                let text = t(
                    &self.texts,
                    Section::Options,
                    if s.width == 800 { 8 } else { 9 },
                );
                if let Some(p) = self.page.as_mut() {
                    p.set_text("b_rezo.spr", text);
                }
            }
            action::GENERAL | action::MUSIC => {
                let mut s = host.settings();
                if act == action::GENERAL {
                    s.general = ftol(fpu.mul(f64::from(self.values[GENERAL]), 256.0));
                    host.set_settings(s);
                }
                // FUN_00478100 / FUN_00476f80.
                let v = fpu.add(
                    fpu.mul(
                        fpu.mul(
                            f64::from(self.values[MUSIC]),
                            f64::from(self.values[GENERAL]),
                        ),
                        127.0,
                    ),
                    0.5,
                );
                host.set_music_volume(ftol(v));
            }
            action::EFFECTS => {
                let mut s = host.settings();
                s.effects = ftol(fpu.mul(f64::from(self.values[EFFECTS]), 127.0));
                host.set_settings(s);
            }
            action::DIALOGS => {
                let mut s = host.settings();
                s.dialogs = ftol(fpu.mul(f64::from(self.values[DIALOGS]), 127.0));
                host.set_settings(s);
            }
            action::LIST => {
                if self.erase_mode {
                    let tx = &self.texts;
                    let (yes, no) = (t(tx, Section::Load, 3), t(tx, Section::Load, 4));
                    self.ask(yes, no);
                    self.state = state::ASK_ERASE;
                } else if let Some(e) = self.chosen {
                    self.stop(host);
                    host.load(e.slot);
                }
            }
            action::CLOSE => self.stop(host),
            action::QUIT => {
                let tx = &self.texts;
                let (yes, no) = (t(tx, Section::Main, 9), t(tx, Section::Main, 10));
                self.ask(yes, no);
                self.state = state::ASK_QUIT;
            }
            _ => {}
        }
    }
}

impl Page {
    fn find(&mut self, name: &str) -> Option<&mut Widget> {
        self.widgets
            .iter_mut()
            .find(|w| w.name.eq_ignore_ascii_case(name))
    }

    /// A widget by its name.
    pub fn widget(&self, name: &str) -> Option<&Widget> {
        self.widgets
            .iter()
            .find(|w| w.name.eq_ignore_ascii_case(name))
    }

    /// FUN_00472620 / FUN_00472650.
    pub fn disable_all(&mut self) {
        for w in &mut self.widgets {
            w.enabled = false;
        }
    }

    pub fn enable_all(&mut self) {
        for w in &mut self.widgets {
            w.enabled = true;
        }
    }

    /// FUN_00472680, FUN_00472710, FUN_004727a0, FUN_00472500.
    pub fn enable(&mut self, name: &str) {
        if let Some(w) = self.find(name) {
            w.enabled = true;
        }
    }

    pub fn show(&mut self, name: &str) {
        if let Some(w) = self.find(name) {
            w.visible = true;
        }
    }

    pub fn hide(&mut self, name: &str) {
        if let Some(w) = self.find(name) {
            w.visible = false;
        }
    }

    pub fn set_text(&mut self, name: &str, text: Vec<u8>) {
        if let Some(w) = self.find(name) {
            w.text = text;
        }
    }

    /// FUN_00472460.
    pub fn set_hit(&mut self, name: &str, r: Rect) {
        if let Some(w) = self.find(name) {
            w.hit = r;
        }
    }

    /// FUN_004723c0 → FUN_0046f080: the widget and its box moved.
    pub fn move_by(&mut self, name: &str, dx: i32, dy: i32) {
        if let Some(w) = self.find(name) {
            w.x += dx;
            w.y += dy;
            w.hit.left += dx;
            w.hit.right += dx;
            w.hit.top += dy;
            w.hit.bottom += dy;
        }
    }

    /// What changes with time before the draw: the hover counts, the
    /// animated buttons' frames, the credits' clock.
    fn update(&mut self, input: &Input, ticks: u32, flag: bool, fpu: Fpu, clock: f64) {
        for w in &mut self.widgets {
            let inside = w.visible && w.enabled && w.hit.holds(input.x, input.y);
            w.hover_since = match (inside, w.hover_since) {
                (true, None) => Some(ticks),
                (true, s) => s,
                (false, _) => None,
            };
            if let WidgetKind::Blink { state: true, cycle } = &mut w.kind {
                // FUN_0046de20: the cycle restarts after 10 ticks.
                if ticks.wrapping_sub(*cycle) > 10 {
                    *cycle = ticks;
                }
            }
            if let WidgetKind::Animated {
                count,
                frame,
                pressed,
                always,
            } = &mut w.kind
            {
                // FUN_0046db00 (the draw, before the action of the frame):
                // a click in the box of a visible, enabled button presses
                // it, the button released lets it go
                // (@ 0x0046db03..0x0046db30).
                let hit = w.hit.holds(input.x, input.y);
                if w.visible && w.enabled && input.click && hit {
                    *pressed = true;
                }
                if !input.held {
                    *pressed = false;
                }
                if !w.visible {
                    continue;
                }
                // One frame on per tick flag when enabled with the mouse
                // in the box and the button up, or enabled and pressed, or
                // always; otherwise frame 0 (@ 0x0046db34..0x0046db81).
                if (w.enabled && ((hit && !input.held) || *pressed)) || *always {
                    if flag {
                        *frame += 1;
                        if *frame >= *count {
                            *frame = 0;
                        }
                    }
                } else {
                    *frame = 0;
                }
            }
        }
        if let Some(c) = &mut self.credits {
            // FUN_00470d00 (in the draw, before the action of the frame).
            c.update(fpu, crate::credits::counter(clock));
        }
    }

    fn draw(
        &self,
        out: &mut [u16],
        ticks: u32,
        values: &[f32; 4],
        font: Option<&SubtitleFont>,
        input: &Input,
        mut rec: Option<&mut HdTexts>,
    ) {
        if let Some(c) = &self.credits {
            match rec {
                Some(t) => c.draw_hd(out, t),
                None => c.draw(out),
            }
            return;
        }
        let n = self.background.len().min(out.len());
        out[..n].copy_from_slice(&self.background[..n]);
        for w in self.widgets.iter().filter(|w| w.visible) {
            w.draw(out, ticks, values, input);
            let Some(f) = font else { continue };
            let mut texts: Vec<TextOut> = Vec::new();
            if let WidgetKind::List(l) = &w.kind {
                // The date and time of each row, in white.
                for (i, e) in l.visible_rows().enumerate() {
                    texts.push(TextOut {
                        x: w.x + 100,
                        y: w.y + 30 + 60 * i as i32,
                        colour: 0xff_ffff,
                        bytes: date_time(&e.time),
                    });
                }
            }
            // The tooltip (FUN_0046d750) of every button class: after 10
            // ticks in the box, centred on it (kept on the screen), its
            // top at the box's top, black under white one pixel up-left.
            let button = !matches!(
                w.kind,
                WidgetKind::Sprite | WidgetKind::Slider { .. } | WidgetKind::List(_)
            );
            if let Some(since) = w.hover_since.filter(|_| button && !w.text.is_empty()) {
                if ticks.wrapping_sub(since) > 10 {
                    let (tw, _) = f.extent(&w.text);
                    let cx = (w.hit.left + w.hit.right) / 2;
                    let x = (cx - tw / 2).clamp(0, (WIDTH as i32 - tw).max(0));
                    let y = w.hit.top;
                    texts.push(TextOut {
                        x: x + 1,
                        y: y + 1,
                        colour: 0,
                        bytes: w.text.clone(),
                    });
                    texts.push(TextOut {
                        x,
                        y,
                        colour: 0xff_ffff,
                        bytes: w.text.clone(),
                    });
                }
            }
            match rec.as_deref_mut() {
                Some(t) => t.draw(TextFont::Popup, f, out, WIDTH, HEIGHT, &texts),
                None => f.draw(out, WIDTH, HEIGHT, &texts),
            }
        }
    }
}

/// The list's text: `GetDateFormatA(DATE_SHORTDATE)`, a space,
/// `GetTimeFormatA(0)`, in the French system's formats.
pub fn date_time(t: &SystemTime) -> Vec<u8> {
    format!(
        "{:02}/{:02}/{:04} {:02}:{:02}:{:02}",
        t.day, t.month, t.year, t.hour, t.minute, t.second
    )
    .into_bytes()
}

/// The game screen at half brightness (FUN_00470430 at 640 wide:
/// `(p >> 1) & 0x7bef` in RGB565).
fn dim(screen: &[u16]) -> Vec<u16> {
    let mut out = vec![0u16; WIDTH * HEIGHT];
    for (o, &p) in out.iter_mut().zip(screen) {
        *o = (p >> 1) & 0x7bef;
    }
    out
}

impl ListBox {
    /// The first visible row: ftol((count − rows) × scroll) when the list
    /// is longer than its h / 60 rows.
    fn first(&self) -> usize {
        let rows = (self.h / 60) as usize;
        let n = self.entries.len();
        if n > rows {
            ftol(f64::from((n - rows) as i32) * f64::from(self.scroll)).max(0) as usize
        } else {
            0
        }
    }

    fn rows(&self) -> usize {
        self.entries.len().min((self.h / 60) as usize)
    }

    fn visible_rows(&self) -> impl Iterator<Item = &ListEntry> {
        self.entries.iter().skip(self.first()).take(self.rows())
    }
}

impl Widget {
    /// The frame of the widget's bank drawn now.
    fn frame(&self, ticks: u32, input: &Input) -> Option<usize> {
        match &self.kind {
            WidgetKind::Sprite | WidgetKind::Button => Some(0),
            WidgetKind::Hover => Some(usize::from(
                self.enabled && self.hit.holds(input.x, input.y),
            )),
            WidgetKind::Animated { frame, .. } => Some(*frame as usize),
            WidgetKind::Toggle { state, .. } => Some(usize::from(*state)),
            WidgetKind::Blink { state, cycle } => {
                // FUN_0046de20: on, frame 0 only after 5 ticks of the
                // cycle, which restarts after 10.
                if !*state || ticks.wrapping_sub(*cycle) > 5 {
                    Some(0)
                } else {
                    None
                }
            }
            WidgetKind::Slider { .. } | WidgetKind::List(_) => None,
        }
    }

    fn draw(&self, out: &mut [u16], ticks: u32, values: &[f32; 4], input: &Input) {
        let mut t = Target::screen(out, WIDTH, HEIGHT);
        match &self.kind {
            WidgetKind::Slider { value, gauge, .. } => {
                // LAB_00473bd0: the gauge at (x, y), the knob at
                // (x − 55 + value × 115, y).
                if let Some(g) = gauge {
                    g.draw(&mut t, Some(self.x), self.y, 0);
                }
                if let Some(k) = &self.bank {
                    let kx = ftol(f64::from(self.x - 55) + f64::from(values[*value]) * 115.0);
                    k.draw(&mut t, Some(kx), self.y, 0);
                }
            }
            WidgetKind::List(l) => {
                // FUN_0046f830: the thumbnails at (x + 30, y + 30 + 60 i).
                if let Some(b) = &self.bank {
                    for (i, e) in l.visible_rows().enumerate() {
                        let frame = usize::try_from(e.vignette).unwrap_or(usize::MAX);
                        b.draw(
                            &mut t,
                            Some(self.x + 30),
                            self.y + 30 + 60 * i as i32,
                            frame,
                        );
                    }
                }
                // The scroll bar (@ 0x00473f70): the gauge at its
                // position, the knob at (track x + 10, ftol(value × 200 +
                // 6 + track top)).
                let s = &l.bar;
                if let Some(g) = &s.gauge {
                    g.draw(&mut t, Some(s.x), s.y, 0);
                }
                if let Some(k) = &s.knob {
                    let ky = ftol(
                        f64::from(l.scroll) * f64::from(s.length)
                            + f64::from(s.knob_dy + s.track_y),
                    );
                    k.draw(&mut t, Some(s.track_x + s.knob_dx), ky, 0);
                }
            }
            _ => {
                if let (Some(b), Some(f)) = (&self.bank, self.frame(ticks, input)) {
                    b.draw(&mut t, Some(self.x), self.y, f);
                }
            }
        }
    }

    /// The widget's input (vtable +0x20): its action, 0 for none.
    fn input(
        &mut self,
        input: &Input,
        fpu: Fpu,
        values: &mut [f32; 4],
        chosen: &mut Option<ListEntry>,
    ) -> i32 {
        let live = self.visible && self.enabled;
        let (mx, my) = (input.x, input.y);
        let inside = self.hit.holds(mx, my);
        match &mut self.kind {
            WidgetKind::Sprite => 0,
            WidgetKind::Button | WidgetKind::Hover => {
                if live && input.click && inside {
                    self.action
                } else {
                    0
                }
            }
            WidgetKind::Animated { .. } => {
                // FUN_0046d670 (the button's; the pressed flag is the
                // draw's).
                if live && input.click && inside {
                    self.action
                } else {
                    0
                }
            }
            WidgetKind::Toggle { off, on, state } => {
                if live && input.click && inside {
                    *state = !*state;
                    self.text = if *state { on.clone() } else { off.clone() };
                    self.action
                } else {
                    0
                }
            }
            WidgetKind::Blink { state, .. } => {
                if live && input.click && inside {
                    *state = !*state;
                    self.action
                } else {
                    0
                }
            }
            WidgetKind::Slider { value, grabbed, .. } => {
                // LAB_00473b00: track origin x0 = x − 65, length 115.
                let x0 = self.x - 65;
                if !live {
                    return 0;
                }
                if !*grabbed {
                    let hit = x0 - 10 < mx && mx < x0 + 125 && self.y - 13 < my && my < self.y + 1;
                    if !(input.click && hit) {
                        return 0;
                    }
                    *grabbed = true;
                } else if !input.held {
                    *grabbed = false;
                    return 0;
                }
                let v = fpu.f32(fpu.div(f64::from(mx - x0), 115.0));
                values[*value] = v.clamp(0.0, 1.0);
                self.action
            }
            WidgetKind::List(l) => {
                let a = list_input(l, self.x, self.y, live, input, chosen, self.action);
                scroll_input(l, live, input, fpu);
                a
            }
        }
    }
}

/// LAB_0046f760: a click in the thumbnails' column selects a row.
fn list_input(
    l: &ListBox,
    x: i32,
    y: i32,
    live: bool,
    input: &Input,
    chosen: &mut Option<ListEntry>,
    action: i32,
) -> i32 {
    if !(live && input.click) {
        return 0;
    }
    // LAB_0046fb30: x < mx < x + 60, y < my < y + 60 × rows.
    let rows = l.rows() as i32;
    let (mx, my) = (input.x, input.y);
    if !(x < mx && mx < x + 60 && y < my && my < y + 60 * rows) {
        return 0;
    }
    let i = ((my - y) / 60) as usize + l.first();
    *chosen = l.entries.get(i).copied();
    action
}

/// The scroll bar's input (@ 0x00473ea0): a click in its box grabs it;
/// while the button is held its value follows the mouse, 0..1.
fn scroll_input(l: &mut ListBox, live: bool, input: &Input, fpu: Fpu) {
    let s = &mut l.bar;
    if !live {
        return;
    }
    if !s.grabbed {
        // @ 0x00473fd0: track x ± width / 2, track top − 10 .. + length
        // + 10.
        let hit = s.track_x - s.width / 2 < input.x
            && input.x < s.track_x + s.width / 2
            && s.track_y - 10 < input.y
            && input.y < s.track_y + s.length + 10;
        if !(input.click && hit) {
            return;
        }
        s.grabbed = true;
    } else if !input.held {
        s.grabbed = false;
        return;
    }
    let v = fpu.f32(fpu.div(f64::from(input.y - s.track_y), f64::from(s.length)));
    l.scroll = v.clamp(0.0, 1.0);
}
