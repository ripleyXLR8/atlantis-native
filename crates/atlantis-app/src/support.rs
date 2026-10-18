//! The support screen (desktop): a thank-you shown at start-up, before the
//! game library or the game, now and then, with links to the project's
//! Buy Me a Coffee page and its GitHub page. It shows rarely: only after
//! [`MIN_LAUNCHES`] launches and [`MIN_PLAYED`] of play, at most once every
//! [`SPACING`], never on the first launch, never again once the player
//! ticks "Don't show this again", and never on a run started with a game
//! folder or a testing option on the command line (`--support-screen`
//! forces it). The counts are kept in `support.ini` in the per-user data
//! folder; a file that cannot be read or written is only logged.
//!
//! This module is the rule, the file, the play-time clock and the screen
//! (its state, input and drawing, in the style of the game library's
//! screen, [`crate::launcher`]); the front end carries out its actions.

use std::path::Path;

use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};
use web_time::{Duration, Instant};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};

use crate::App;

/// The file of the counts, in the per-user data folder.
pub const SUPPORT_FILE: &str = "support.ini";
/// The project's page on GitHub.
pub const GITHUB_URL: &str = "https://github.com/ripleyXLR8/atlantis-native";
/// The launches before the screen first shows (this one counted).
pub const MIN_LAUNCHES: u32 = 3;
/// The play time before the screen first shows (seconds).
pub const MIN_PLAYED: u64 = 2 * 3600;
/// The time between two showings (seconds).
pub const SPACING: u64 = 7 * 24 * 3600;
/// The play time is written to the file this often while a game runs.
pub const SAVE_EVERY: Duration = Duration::from_secs(5 * 60);
/// A gap this long between two engine steps is not play (the game paused
/// in the options menu, the computer asleep, a stall).
const MAX_GAP: Duration = Duration::from_secs(2);

/// What `support.ini` holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SupportState {
    /// The program's launches (with a window).
    pub launches: u32,
    /// The seconds games have run.
    pub played_seconds: u64,
    /// When the screen was last shown (Unix seconds; 0: never).
    pub last_shown: u64,
    /// The player ticked "Don't show this again".
    pub never: bool,
}

impl SupportState {
    /// The counts of a `support.ini` text: `key = value` lines; unknown
    /// keys, bad values and other lines are skipped (their count is 0).
    pub fn parse(text: &str) -> SupportState {
        let mut s = SupportState::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "launches" => s.launches = v.parse().unwrap_or(0),
                "played_seconds" => s.played_seconds = v.parse().unwrap_or(0),
                "last_shown" => s.last_shown = v.parse().unwrap_or(0),
                "never" => s.never = matches!(v, "on" | "true" | "1" | "yes"),
                _ => {}
            }
        }
        s
    }

    pub fn format(&self) -> String {
        format!(
            "# Atlantis Native: the support screen shown now and then at start-up.\n\
             # never = on: it is not shown again.\n\
             launches = {}\nplayed_seconds = {}\nlast_shown = {}\nnever = {}\n",
            self.launches,
            self.played_seconds,
            self.last_shown,
            if self.never { "on" } else { "off" }
        )
    }

    /// The counts kept in `dir` (none: all 0).
    pub fn load(dir: &Path) -> SupportState {
        match std::fs::read_to_string(dir.join(SUPPORT_FILE)) {
            Ok(t) => SupportState::parse(&t),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("{}: {e}", dir.join(SUPPORT_FILE).display());
                }
                SupportState::default()
            }
        }
    }

    /// Writes the counts in `dir` (created if needed); a failure is logged.
    pub fn save(&self, dir: &Path) {
        let r = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(dir.join(SUPPORT_FILE), self.format()));
        if let Err(e) = r {
            log::warn!("{}: {e}", dir.join(SUPPORT_FILE).display());
        }
    }

    /// Changes the counts kept in `dir` with `f` (read again first: another
    /// window of the program may have written them).
    pub fn update(dir: &Path, f: impl FnOnce(&mut SupportState)) {
        let mut s = SupportState::load(dir);
        f(&mut s);
        s.save(dir);
    }
}

/// Whether the screen shows at a start-up at `now` (Unix seconds), the
/// launch already counted in `s`: not after "Don't show this again", not
/// on the first launch, after [`MIN_LAUNCHES`] launches and [`MIN_PLAYED`]
/// of play, [`SPACING`] after it was last shown.
pub fn should_show(s: &SupportState, now: u64) -> bool {
    if s.never || s.launches <= 1 {
        return false;
    }
    let spaced = s.last_shown == 0 || now >= s.last_shown.saturating_add(SPACING);
    s.launches >= MIN_LAUNCHES && s.played_seconds >= MIN_PLAYED && spaced
}

/// The time now, in Unix seconds (0 if the clock is before 1970).
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The play time not yet written: counted at each engine step, written
/// every [`SAVE_EVERY`], when the game stops and at exit.
#[derive(Debug, Default)]
pub struct PlayClock {
    last: Option<Instant>,
    unsaved: Duration,
}

impl PlayClock {
    /// An engine step at `now`: the time since the last one is play (a
    /// gap over [`MAX_GAP`] is not). True when the time is to be written.
    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(last) = self.last {
            let d = now.saturating_duration_since(last);
            if d <= MAX_GAP {
                self.unsaved += d;
            }
        }
        self.last = Some(now);
        self.unsaved >= SAVE_EVERY
    }

    /// The game paused or stopped: the time until the next step is not
    /// play.
    pub fn pause(&mut self) {
        self.last = None;
    }

    /// The whole seconds to write (the fraction kept for the next time).
    pub fn take(&mut self) -> u64 {
        let s = self.unsaved.as_secs();
        self.unsaved -= Duration::from_secs(s);
        s
    }
}

/// The keys the screen reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportKey {
    Tab,
    Left,
    Right,
    Space,
    Enter,
    Escape,
}

/// What can be chosen with the keys: the three buttons and the checkbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Coffee,
    GitHub,
    Play,
    Never,
}

const ITEMS: [Item; 4] = [Item::Coffee, Item::GitHub, Item::Play, Item::Never];
const BUTTONS: [Item; 3] = [Item::Coffee, Item::GitHub, Item::Play];

/// What the front end does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportAction {
    None,
    /// Open the Buy Me a Coffee page (the screen stays).
    Coffee,
    /// Open the project's GitHub page (the screen stays).
    GitHub,
    /// Close the screen and go on to the game; `never`: "Don't show this
    /// again" is ticked.
    Play {
        never: bool,
    },
}

const BOX_X: i32 = 90;
const BOX_Y: i32 = 40;
const BOX_W: i32 = 460;
const BOX_H: i32 = 400;
const CENTRE_X: i32 = BOX_X + BOX_W / 2;
/// The icon's side, in pixels.
pub const ICON_SIDE: usize = 84;
const ICON_X: i32 = CENTRE_X - ICON_SIDE as i32 / 2;
const ICON_Y: i32 = BOX_Y + 16;
const TITLE_Y: i32 = 150;
const TITLE: &[u8] = b"Thanks for playing!";
/// The text, line by line (an empty line between the two paragraphs).
const TEXT_LINES: [&[u8]; 5] = [
    b"Atlantis Native is a labour of love, made in my spare time",
    b"so that Seth's adventure can live on, on today's computers.",
    b"",
    b"If it brings back good memories, you can buy me a coffee,",
    b"or leave a star on GitHub. Either one makes my day!",
];
const TEXT_Y: i32 = 184;
const LINE_H: i32 = 19;
const BUTTON_Y: i32 = 298;
const BUTTON_H: i32 = 32;
const BUTTON_GAP: i32 = 12;
const CHECK_Y: i32 = 351;
const CHECK_SIDE: i32 = 12;
const CHECK_TEXT: &[u8] = b"Don't show this again";
const LINK_TEXT: &[u8] = b"github.com/ripleyXLR8/atlantis-native";
const LINK_Y: i32 = 379;
/// The footer (0xb7: the middle dot in Windows-1252, the font's code page).
const FOOTER: &[u8] = b"See you in Atlantis  \xb7  Esc or Enter to play";
const FOOTER_Y: i32 = 413;

const SCREEN_BG: u16 = 0x0000;
const BG: u16 = 0x18e3;
const BORDER: u16 = 0xbdf7;
const HIGHLIGHT: u16 = 0x3a0c;
const BUTTON_BG: u16 = 0x2945;
const BUTTON_BORDER: u16 = 0x9cf3;
const TEXT: u16 = 0xffff;
const DIM: u16 = 0x9cf3;
/// The gold of the game library's Buy Me a Coffee link.
const YELLOW: u16 = 0xfea0;
/// Light blue (144, 180, 255).
const LINK: u16 = 0x95bf;

/// The project's icon, 84x84 RGB565 (little-endian), made from
/// `assets/icon/atlantis-native-1024.png` by
/// `assets/icon/make_support_icon.py`.
static ICON: &[u8] = include_bytes!("../../../assets/icon/atlantis-native-84.rgb565");

/// The screen's fonts: the text (the game library's), the title and the
/// buttons' labels larger.
pub struct SupportFonts {
    pub text: SubtitleFont,
    pub title: SubtitleFont,
    pub button: SubtitleFont,
}

impl Default for SupportFonts {
    fn default() -> Self {
        Self::new()
    }
}

impl SupportFonts {
    pub fn new() -> SupportFonts {
        SupportFonts {
            text: SubtitleFont::new(),
            title: SubtitleFont::with_size(21.0),
            button: SubtitleFont::with_size(15.0),
        }
    }
}

impl Item {
    fn width(self) -> i32 {
        match self {
            Item::Coffee => 150,
            Item::GitHub => 140,
            Item::Play => 102,
            Item::Never => 0,
        }
    }

    fn label(self) -> &'static [u8] {
        match self {
            Item::Coffee => b"Buy me a coffee",
            Item::GitHub => b"Star on GitHub",
            Item::Play => b"Let's play!",
            Item::Never => CHECK_TEXT,
        }
    }

    fn colour(self) -> u16 {
        match self {
            Item::Coffee => YELLOW,
            Item::GitHub => LINK,
            Item::Play => TEXT,
            Item::Never => DIM,
        }
    }
}

/// The buttons' rectangles (x, y, w, h).
fn button_rects() -> [(Item, i32, i32, i32, i32); 3] {
    let total: i32 =
        BUTTONS.iter().map(|b| b.width()).sum::<i32>() + BUTTON_GAP * (BUTTONS.len() as i32 - 1);
    let mut x = CENTRE_X - total / 2;
    BUTTONS.map(|b| {
        let r = (b, x, BUTTON_Y, b.width(), BUTTON_H);
        x += b.width() + BUTTON_GAP;
        r
    })
}

/// The checkbox with its label (x, y, w, h), the box at (x, y).
fn check_rect(font: &SubtitleFont) -> (i32, i32, i32, i32) {
    let w = CHECK_SIDE + 8 + font.width(CHECK_TEXT);
    (CENTRE_X - w / 2, CHECK_Y, w, CHECK_SIDE)
}

/// The link's rectangle (x, y, w, h).
fn link_rect(font: &SubtitleFont) -> (i32, i32, i32, i32) {
    let w = font.width(LINK_TEXT);
    (CENTRE_X - w / 2, LINK_Y, w, 16)
}

fn inside((x, y, w, h): (i32, i32, i32, i32), px: i32, py: i32) -> bool {
    (x..x + w).contains(&px) && (y..y + h).contains(&py)
}

/// The screen's state.
#[derive(Debug, Default)]
pub struct SupportScreen {
    /// "Don't show this again" is ticked.
    never: bool,
    /// The item chosen with Tab / Left / Right (none at first: Enter
    /// plays).
    focus: Option<Item>,
    /// The item under the pointer (the link: [`Item::GitHub`] with
    /// `hover_link`).
    hover: Option<Item>,
    hover_link: bool,
}

impl SupportScreen {
    pub fn new() -> SupportScreen {
        SupportScreen::default()
    }

    pub fn never(&self) -> bool {
        self.never
    }

    pub fn focus(&self) -> Option<Item> {
        self.focus
    }

    fn press(&mut self, item: Item) -> SupportAction {
        match item {
            Item::Coffee => SupportAction::Coffee,
            Item::GitHub => SupportAction::GitHub,
            Item::Play => SupportAction::Play { never: self.never },
            Item::Never => {
                self.never = !self.never;
                SupportAction::None
            }
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let n = ITEMS.len();
        let at = self.focus.and_then(|f| ITEMS.iter().position(|&i| i == f));
        let i = match (at, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.focus = Some(ITEMS[i]);
    }

    /// A key: Tab / Right and Left choose a button or the checkbox, Space
    /// presses the one chosen (ticks the checkbox), Enter presses the
    /// button chosen (or plays), Escape plays.
    pub fn key(&mut self, key: SupportKey) -> SupportAction {
        match key {
            SupportKey::Tab | SupportKey::Right => self.move_focus(true),
            SupportKey::Left => self.move_focus(false),
            SupportKey::Space => {
                if let Some(f) = self.focus {
                    return self.press(f);
                }
            }
            SupportKey::Enter => {
                return match self.focus {
                    Some(b @ (Item::Coffee | Item::GitHub)) => self.press(b),
                    _ => self.press(Item::Play),
                }
            }
            SupportKey::Escape => return self.press(Item::Play),
        }
        SupportAction::None
    }

    fn item_at(x: i32, y: i32, fonts: &SupportFonts) -> (Option<Item>, bool) {
        if let Some(r) = button_rects()
            .into_iter()
            .find(|&(_, bx, by, bw, bh)| inside((bx, by, bw, bh), x, y))
        {
            return (Some(r.0), false);
        }
        let (cx, cy, cw, ch) = check_rect(&fonts.text);
        if inside((cx - 4, cy - 4, cw + 8, ch + 8), x, y) {
            return (Some(Item::Never), false);
        }
        if inside(link_rect(&fonts.text), x, y) {
            return (Some(Item::GitHub), true);
        }
        (None, false)
    }

    /// The pointer moved to a 640x480 screen point: what is under it
    /// lights up.
    pub fn pointer(&mut self, x: i32, y: i32, fonts: &SupportFonts) {
        (self.hover, self.hover_link) = Self::item_at(x, y, fonts);
    }

    /// A left click at a 640x480 screen point.
    pub fn click(&mut self, x: i32, y: i32, fonts: &SupportFonts) -> SupportAction {
        match Self::item_at(x, y, fonts).0 {
            Some(i) => {
                if i == Item::Never {
                    self.focus = Some(Item::Never);
                }
                self.press(i)
            }
            None => SupportAction::None,
        }
    }

    /// Draws the screen on a 640x480 RGB565 frame.
    pub fn draw(&self, screen: &mut [u16], fonts: &SupportFonts) {
        let fill = |screen: &mut [u16], x: i32, y: i32, w: i32, h: i32, c: u16| {
            for yy in y.max(0)..(y + h).min(SCREEN_H as i32) {
                for xx in x.max(0)..(x + w).min(SCREEN_W as i32) {
                    screen[yy as usize * SCREEN_W + xx as usize] = c;
                }
            }
        };
        let text = |screen: &mut [u16], f: &SubtitleFont, t: &[u8], x: i32, y: i32, c: u16| {
            f.draw(screen, SCREEN_W, SCREEN_H, t, x, y, BOX_W, c);
        };
        let centre = |f: &SubtitleFont, t: &[u8]| CENTRE_X - f.width(t) / 2;
        screen.fill(SCREEN_BG);
        fill(screen, BOX_X - 1, BOX_Y - 1, BOX_W + 2, BOX_H + 2, BORDER);
        fill(screen, BOX_X, BOX_Y, BOX_W, BOX_H, BG);
        // The icon.
        for (i, px) in ICON.as_chunks::<2>().0.iter().enumerate() {
            let (x, y) = (
                ICON_X + (i % ICON_SIDE) as i32,
                ICON_Y + (i / ICON_SIDE) as i32,
            );
            screen[y as usize * SCREEN_W + x as usize] = u16::from_le_bytes([px[0], px[1]]);
        }
        text(
            screen,
            &fonts.title,
            TITLE,
            centre(&fonts.title, TITLE),
            TITLE_Y,
            TEXT,
        );
        for (i, t) in TEXT_LINES.iter().enumerate() {
            let y = TEXT_Y + i as i32 * LINE_H;
            text(screen, &fonts.text, t, centre(&fonts.text, t), y, DIM);
        }
        // The buttons: "Buy me a coffee" lit, the one chosen or under the
        // pointer framed in yellow.
        for (b, x, y, w, h) in button_rects() {
            let on = self.focus == Some(b) || (self.hover == Some(b) && !self.hover_link);
            let frame = if on { YELLOW } else { BUTTON_BORDER };
            fill(screen, x - 1, y - 1, w + 2, h + 2, frame);
            let bg = if on || b == Item::Coffee {
                HIGHLIGHT
            } else {
                BUTTON_BG
            };
            fill(screen, x, y, w, h, bg);
            let t = b.label();
            let f = &fonts.button;
            text(screen, f, t, x + (w - f.width(t)) / 2, y + 7, b.colour());
        }
        // The checkbox.
        let (cx, cy, _, _) = check_rect(&fonts.text);
        let on = self.focus == Some(Item::Never) || self.hover == Some(Item::Never);
        let frame = if on { YELLOW } else { BUTTON_BORDER };
        fill(screen, cx, cy, CHECK_SIDE, CHECK_SIDE, frame);
        fill(screen, cx + 1, cy + 1, CHECK_SIDE - 2, CHECK_SIDE - 2, BG);
        if self.never {
            // A tick.
            for (dx, dy) in [
                (2, 6),
                (3, 7),
                (4, 8),
                (5, 7),
                (6, 6),
                (7, 5),
                (8, 4),
                (9, 3),
            ] {
                fill(screen, cx + dx, cy + dy - 1, 1, 2, TEXT);
            }
        }
        let label_c = if on { TEXT } else { DIM };
        text(
            screen,
            &fonts.text,
            CHECK_TEXT,
            cx + CHECK_SIDE + 8,
            cy - 1,
            label_c,
        );
        // The link, underlined under the pointer.
        let (lx, ly, lw, _) = link_rect(&fonts.text);
        text(screen, &fonts.text, LINK_TEXT, lx, ly, LINK);
        if self.hover_link {
            fill(screen, lx, ly + 15, lw, 1, LINK);
        }
        text(
            screen,
            &fonts.text,
            FOOTER,
            centre(&fonts.text, FOOTER),
            FOOTER_Y,
            DIM,
        );
    }
}

/// What the program does when the support screen closes (false: the
/// program ends).
pub type Then = Box<dyn FnOnce(&mut App) -> bool>;

/// The screen while it is shown: its fonts, and what the program does
/// when it closes (start the game or the game library; false: nothing to
/// do, the program ends).
pub struct Shown {
    pub screen: SupportScreen,
    pub fonts: SupportFonts,
    data_dir: std::path::PathBuf,
    then: Option<Then>,
}

fn support_key(key: &Key) -> Option<SupportKey> {
    Some(match key {
        Key::Named(NamedKey::Tab) => SupportKey::Tab,
        Key::Named(NamedKey::ArrowLeft) => SupportKey::Left,
        Key::Named(NamedKey::ArrowRight) => SupportKey::Right,
        Key::Named(NamedKey::Space) => SupportKey::Space,
        Key::Named(NamedKey::Enter) => SupportKey::Enter,
        Key::Named(NamedKey::Escape) => SupportKey::Escape,
        Key::Character(s) if s == " " => SupportKey::Space,
        _ => return None,
    })
}

/// Draws the support screen in `frame` when it is shown; false when it is
/// not.
pub fn draw_support(shown: Option<&Shown>, frame: &mut Vec<u16>) -> bool {
    let Some(s) = shown else {
        return false;
    };
    frame.resize(SCREEN_W * SCREEN_H, 0);
    s.screen.draw(frame, &s.fonts);
    true
}

impl App {
    /// Shows the support screen; `then` runs when it closes (false: the
    /// program ends). "Don't show this again" is saved in `data_dir`.
    pub fn show_support(&mut self, data_dir: std::path::PathBuf, then: Then) {
        log::info!("support screen");
        self.support = Some(Shown {
            screen: SupportScreen::new(),
            fonts: SupportFonts::new(),
            data_dir,
            then: Some(then),
        });
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Whether the support screen is shown.
    pub fn support_shown(&self) -> bool {
        self.support.is_some()
    }

    /// The window events while the support screen is shown; true when the
    /// event was taken.
    pub(crate) fn support_event(&mut self, el: &ActiveEventLoop, event: &WindowEvent) -> bool {
        let Some(shown) = &mut self.support else {
            return false;
        };
        let action = match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor = Some((position.x, position.y));
                let (x, y) = self.viewport.to_screen(position.x, position.y);
                shown.screen.pointer(x, y, &shown.fonts);
                SupportAction::None
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => match self.last_cursor {
                Some((wx, wy)) => {
                    let (x, y) = self.viewport.to_screen(wx, wy);
                    shown.screen.click(x, y, &shown.fonts)
                }
                None => SupportAction::None,
            },
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match support_key(&event.logical_key) {
                    Some(k) => shown.screen.key(k),
                    None => SupportAction::None,
                }
            }
            // The other buttons and key releases: nothing (not the game's).
            WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::KeyboardInput { .. } => SupportAction::None,
            WindowEvent::CloseRequested => {
                // The checkbox counts when the window is closed too.
                if shown.screen.never() {
                    SupportState::update(&shown.data_dir, |s| s.never = true);
                }
                return false;
            }
            // The rest (redraw, resize, focus) as with a game.
            _ => return false,
        };
        if !self.support_action(action) {
            el.exit();
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        true
    }

    /// Carries out an action of the support screen; false when the program
    /// is to end.
    pub(crate) fn support_action(&mut self, action: SupportAction) -> bool {
        match action {
            SupportAction::None => true,
            SupportAction::Coffee => {
                crate::open_support_page();
                true
            }
            SupportAction::GitHub => {
                crate::setup::open_url(GITHUB_URL);
                true
            }
            SupportAction::Play { never } => {
                let Some(mut shown) = self.support.take() else {
                    return true;
                };
                if never {
                    log::info!("support screen: not shown again");
                    SupportState::update(&shown.data_dir, |s| s.never = true);
                }
                // The start may ask with dialogs: not a hang.
                crate::watchdog::leave();
                match shown.then.take() {
                    Some(then) => then(self),
                    None => true,
                }
            }
        }
    }

    /// An engine step of the game at `now`: its play time counted (written
    /// every [`SAVE_EVERY`]).
    pub(crate) fn tick_play_time(&mut self, now: Instant) {
        let Some(session) = &mut self.session else {
            return;
        };
        if session.play_clock.tick(now) {
            session.save_play_time();
        }
    }

    /// Writes the play time not yet written (the game stops, the program
    /// ends).
    pub fn save_play_time(&mut self) {
        if let Some(session) = &mut self.session {
            session.save_play_time();
        }
    }
}

impl crate::session::Session {
    /// Writes the play time counted since the last time to `support.ini`.
    pub(crate) fn save_play_time(&mut self) {
        let secs = self.play_clock.take();
        if secs > 0 && self.counts_play {
            SupportState::update(self.data_dir(), |s| {
                s.played_seconds = s.played_seconds.saturating_add(secs)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 3600;
    const NOW: u64 = 1_800_000_000;

    fn due() -> SupportState {
        SupportState {
            launches: 3,
            played_seconds: 2 * 3600,
            last_shown: 0,
            never: false,
        }
    }

    #[test]
    fn the_rule() {
        assert!(should_show(&due(), NOW));
        // Each condition alone keeps it hidden.
        assert!(!should_show(
            &SupportState {
                never: true,
                ..due()
            },
            NOW
        ));
        assert!(!should_show(
            &SupportState {
                launches: 2,
                ..due()
            },
            NOW
        ));
        assert!(!should_show(
            &SupportState {
                played_seconds: 2 * 3600 - 1,
                ..due()
            },
            NOW
        ));
        // Never on the first launch, whatever the rest says (a file
        // copied from elsewhere, a clock gone wrong).
        for launches in [0, 1] {
            let s = SupportState {
                launches,
                played_seconds: 1_000_000,
                ..due()
            };
            assert!(!should_show(&s, NOW));
        }
        // A week between two showings.
        let shown = |last| SupportState {
            last_shown: last,
            launches: 50,
            ..due()
        };
        assert!(!should_show(&shown(NOW - 6 * DAY), NOW));
        assert!(!should_show(&shown(NOW - 7 * DAY + 1), NOW));
        assert!(should_show(&shown(NOW - 7 * DAY), NOW));
        assert!(should_show(&shown(NOW - 30 * DAY), NOW));
        // Shown "in the future" (the clock set back): not until a week
        // after.
        assert!(!should_show(&shown(NOW + DAY), NOW));
        assert!(!should_show(&shown(u64::MAX), NOW));
    }

    #[test]
    fn the_file() {
        let s = SupportState {
            launches: 12,
            played_seconds: 98_765,
            last_shown: NOW,
            never: true,
        };
        assert_eq!(SupportState::parse(&s.format()), s);
        assert_eq!(SupportState::parse(&due().format()), due());
        assert!(due().format().contains("never = off"));
        // Garbage: what reads is kept, the rest is 0.
        let g = SupportState::parse(
            "launches=7\nplayed_seconds = lots\n\u{0}\u{1}garbage\n=\nnever = maybe\n\
             last_shown = -5\nother = 3\n[section]\n",
        );
        assert_eq!(
            g,
            SupportState {
                launches: 7,
                ..SupportState::default()
            }
        );
        assert_eq!(SupportState::parse(""), SupportState::default());
        assert!(SupportState::parse(" never =  on \r\n").never);
    }

    #[test]
    fn the_file_on_disk() {
        let d = std::env::temp_dir().join(format!("atlantis-support-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(SupportState::load(&d), SupportState::default());
        SupportState::update(&d, |s| s.launches += 1);
        SupportState::update(&d, |s| s.launches += 1);
        assert_eq!(SupportState::load(&d).launches, 2);
        std::fs::write(d.join(SUPPORT_FILE), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(SupportState::load(&d), SupportState::default());
        let _ = std::fs::remove_dir_all(&d);
        // A folder that cannot be made: logged, no panic.
        let f = std::env::temp_dir().join(format!("atlantis-support-f-{}", std::process::id()));
        std::fs::write(&f, b"a file").unwrap();
        due().save(&f.join("sub"));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn the_play_clock() {
        let t0 = Instant::now();
        let mut c = PlayClock::default();
        assert!(!c.tick(t0));
        let mut t = t0;
        for _ in 0..60 {
            t += Duration::from_millis(500);
            assert!(!c.tick(t));
        }
        assert_eq!(c.take(), 30);
        // A long gap (paused, asleep) is not play.
        t += Duration::from_secs(600);
        assert!(!c.tick(t));
        assert_eq!(c.take(), 0);
        // Paused: the next step starts again.
        c.pause();
        t += Duration::from_secs(1);
        c.tick(t);
        assert_eq!(c.take(), 0);
        // Due every five minutes; the fraction is kept.
        let mut due = false;
        for _ in 0..(5 * 60 * 4 + 2) {
            t += Duration::from_millis(250);
            due = c.tick(t);
        }
        assert!(due);
        assert_eq!(c.take(), 300);
        assert!(!c.tick(t + Duration::from_millis(250)));
        assert_eq!(c.unsaved, Duration::from_millis(750));
    }

    fn centre_of(item: Item) -> (i32, i32) {
        let (_, x, y, w, h) = button_rects().into_iter().find(|r| r.0 == item).unwrap();
        (x + w / 2, y + h / 2)
    }

    #[test]
    fn keys() {
        let mut s = SupportScreen::new();
        // Nothing chosen: Enter and Escape play, Space does nothing.
        assert_eq!(s.key(SupportKey::Space), SupportAction::None);
        assert_eq!(
            s.key(SupportKey::Enter),
            SupportAction::Play { never: false }
        );
        assert_eq!(
            s.key(SupportKey::Escape),
            SupportAction::Play { never: false }
        );
        s.key(SupportKey::Tab);
        assert_eq!(s.focus(), Some(Item::Coffee));
        assert_eq!(s.key(SupportKey::Enter), SupportAction::Coffee);
        assert_eq!(s.key(SupportKey::Space), SupportAction::Coffee);
        s.key(SupportKey::Right);
        assert_eq!(s.key(SupportKey::Enter), SupportAction::GitHub);
        s.key(SupportKey::Tab);
        assert_eq!(s.focus(), Some(Item::Play));
        assert_eq!(
            s.key(SupportKey::Enter),
            SupportAction::Play { never: false }
        );
        s.key(SupportKey::Tab);
        assert_eq!(s.focus(), Some(Item::Never));
        assert_eq!(s.key(SupportKey::Space), SupportAction::None);
        assert!(s.never());
        // Enter on the checkbox plays, with the box ticked.
        assert_eq!(
            s.key(SupportKey::Enter),
            SupportAction::Play { never: true }
        );
        assert_eq!(
            s.key(SupportKey::Escape),
            SupportAction::Play { never: true }
        );
        s.key(SupportKey::Space);
        assert!(!s.never());
        // Round the items both ways.
        s.key(SupportKey::Tab);
        assert_eq!(s.focus(), Some(Item::Coffee));
        s.key(SupportKey::Left);
        assert_eq!(s.focus(), Some(Item::Never));
        let mut s = SupportScreen::new();
        s.key(SupportKey::Left);
        assert_eq!(s.focus(), Some(Item::Never));
    }

    #[test]
    fn clicks() {
        let fonts = SupportFonts::new();
        let mut s = SupportScreen::new();
        let (x, y) = centre_of(Item::Coffee);
        assert_eq!(s.click(x, y, &fonts), SupportAction::Coffee);
        let (x, y) = centre_of(Item::GitHub);
        assert_eq!(s.click(x, y, &fonts), SupportAction::GitHub);
        let (lx, ly, lw, lh) = link_rect(&fonts.text);
        assert_eq!(
            s.click(lx + lw / 2, ly + lh / 2, &fonts),
            SupportAction::GitHub
        );
        s.pointer(lx + 3, ly + 3, &fonts);
        assert!(s.hover_link && s.hover == Some(Item::GitHub));
        // The checkbox and its label tick it.
        let (cx, cy, cw, _) = check_rect(&fonts.text);
        assert_eq!(s.click(cx + 5, cy + 5, &fonts), SupportAction::None);
        assert!(s.never());
        assert_eq!(s.click(cx + cw - 5, cy + 5, &fonts), SupportAction::None);
        assert!(!s.never());
        s.click(cx + 5, cy + 5, &fonts);
        let (x, y) = centre_of(Item::Play);
        assert_eq!(s.click(x, y, &fonts), SupportAction::Play { never: true });
        // Elsewhere: nothing.
        assert_eq!(s.click(5, 5, &fonts), SupportAction::None);
        assert_eq!(s.click(CENTRE_X, TEXT_Y + 5, &fonts), SupportAction::None);
        s.pointer(x, y, &fonts);
        assert_eq!((s.hover, s.hover_link), (Some(Item::Play), false));
        s.pointer(5, 5, &fonts);
        assert_eq!(s.hover, None);
    }

    #[test]
    fn the_drawing_fits() {
        let fonts = SupportFonts::new();
        assert_eq!(ICON.len(), ICON_SIDE * ICON_SIDE * 2);
        let in_box = |x: i32, y: i32, w: i32, h: i32| {
            x >= BOX_X + 8
                && x + w <= BOX_X + BOX_W - 8
                && y >= BOX_Y + 8
                && y + h <= BOX_Y + BOX_H - 8
        };
        const { assert!(BOX_X >= 0 && BOX_Y >= 0) };
        assert!(BOX_X + BOX_W <= SCREEN_W as i32 && BOX_Y + BOX_H <= SCREEN_H as i32);
        assert!(in_box(ICON_X, ICON_Y, ICON_SIDE as i32, ICON_SIDE as i32));
        assert!(ICON_Y + ICON_SIDE as i32 <= TITLE_Y);
        let fits = |f: &SubtitleFont, t: &[u8], y: i32, h: i32| {
            let w = f.width(t);
            in_box(CENTRE_X - w / 2, y, w, h)
        };
        assert!(fits(&fonts.title, TITLE, TITLE_Y, 26));
        const { assert!(TITLE_Y + 26 <= TEXT_Y) };
        for t in TEXT_LINES {
            assert!(fits(&fonts.text, t, TEXT_Y, 16));
        }
        assert!(TEXT_Y + TEXT_LINES.len() as i32 * LINE_H <= BUTTON_Y);
        for (b, x, y, w, h) in button_rects() {
            assert!(in_box(x - 1, y - 1, w + 2, h + 2), "{b:?}");
            assert!(fonts.button.width(b.label()) + 16 <= w, "{b:?}");
        }
        let (cx, cy, cw, ch) = check_rect(&fonts.text);
        assert!(in_box(cx, cy, cw, ch) && cy > BUTTON_Y + BUTTON_H + 8);
        let (lx, ly, lw, lh) = link_rect(&fonts.text);
        assert!(in_box(lx, ly, lw, lh) && ly > cy + ch + 8);
        assert!(fits(&fonts.text, FOOTER, FOOTER_Y, 16) && FOOTER_Y > ly + lh + 8);
        // The middle dot is in the font.
        assert!(fonts.text.width(b"\xb7") > 0);
        // Drawn: every colour of the screen is on it.
        let mut s = SupportScreen::new();
        let mut frame = vec![0u16; SCREEN_W * SCREEN_H];
        s.draw(&mut frame, &fonts);
        for c in [BG, BORDER, HIGHLIGHT, BUTTON_BG, TEXT, DIM, YELLOW, LINK] {
            assert!(frame.contains(&c), "{c:04x}");
        }
        // The icon is copied as it is.
        let at = |x: i32, y: i32| frame[y as usize * SCREEN_W + x as usize];
        assert_eq!(at(ICON_X, ICON_Y), u16::from_le_bytes([ICON[0], ICON[1]]));
        // Ticked and focused: drawn too, nothing outside the box but black.
        s.key(SupportKey::Left);
        s.key(SupportKey::Space);
        let mut frame = vec![0x1234u16; SCREEN_W * SCREEN_H];
        s.draw(&mut frame, &fonts);
        assert!(frame[..SCREEN_W * (BOX_Y as usize - 1)]
            .iter()
            .all(|&p| p == SCREEN_BG));
    }

    #[test]
    fn the_screen_in_the_front_end() {
        use atlantis_engine::audio::Mixer;
        use atlantis_engine::sys::Sys;
        use atlantis_engine::Start;
        use std::cell::Cell;
        use std::rc::Rc;

        let d = std::env::temp_dir().join(format!("atlantis-support-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let session = crate::session::Session::new(d.clone(), false, false, false, Start::Boot);
        let mut app = App::new(Sys::new(Mixer::new()), None).with_session(session);
        let started = Rc::new(Cell::new(false));
        let s2 = started.clone();
        app.show_support(
            d.clone(),
            Box::new(move |_| {
                s2.set(true);
                true
            }),
        );
        assert!(app.support_shown());
        let mut frame = Vec::new();
        assert!(draw_support(app.support.as_ref(), &mut frame));
        assert_eq!(frame.len(), SCREEN_W * SCREEN_H);
        assert!(app.support_action(SupportAction::None));
        assert!(app.support_shown() && !started.get());
        assert!(app.support_action(SupportAction::Play { never: true }));
        assert!(!app.support_shown() && started.get());
        assert!(!draw_support(app.support.as_ref(), &mut frame));
        assert!(SupportState::load(&d).never);
        // The play time is added to the file.
        let t = Instant::now();
        let session = app.session_mut().unwrap();
        session.play_clock.tick(t);
        session.play_clock.tick(t + Duration::from_millis(1500));
        app.save_play_time();
        assert_eq!(SupportState::load(&d).played_seconds, 1);
        // A run with a game folder or a testing option counts nothing.
        let session = app.session_mut().unwrap();
        session.counts_play = false;
        session.play_clock.tick(t + Duration::from_millis(2000));
        session.play_clock.tick(t + Duration::from_millis(3900));
        app.save_play_time();
        assert_eq!(SupportState::load(&d).played_seconds, 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
