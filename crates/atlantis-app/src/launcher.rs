//! The game library's screen (desktop): the game folders of the library,
//! each shown as its game and edition and its path, to choose the one to
//! play; in the look of the web page ([`crate::launcher_art`]). It is laid
//! out on the 640x480 screen: its background drawn in the 640x480 frame,
//! its text and ornaments either drawn there too or handed to the front
//! end to draw at the window's resolution, so that they stay smooth in a
//! large window. This module is the screen's state, input and layout; the
//! front end carries out its actions.

use std::path::PathBuf;

use atlantis_engine::warp::SCREEN_W;

use crate::launcher_art::{self as art, Frame565, Item, Style};
use crate::setup::GameKind;

/// An entry of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherEntry {
    pub path: PathBuf,
    /// The game and its edition ("Atlantis: The Lost Tales, GOG.com,
    /// French (1999 build)", "Atlantis II, GOG.com, French (patch
    /// 1.01)"), or why the folder does not open.
    pub label: String,
    /// The folder opens as the game.
    pub found: bool,
    /// The game the folder holds (or held), when known: its badge.
    pub kind: Option<GameKind>,
}

/// The keys the screen reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    Enter,
    Delete,
    Escape,
    Char(char),
}

/// The buttons under the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Play,
    Add,
    Find,
    Remove,
    Quit,
}

const BUTTONS: [Button; 5] = [
    Button::Play,
    Button::Add,
    Button::Find,
    Button::Remove,
    Button::Quit,
];

/// What the front end does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherAction {
    None,
    /// Play entry `i`.
    Play(usize),
    /// Entry `i` is not found: ask where the game is now.
    Locate(usize),
    /// Add a game folder (the folder picker).
    Add,
    /// Look for installations on the computer.
    Find,
    /// Take entry `i` off the list.
    Remove(usize),
    /// Open the project's Buy Me a Coffee page.
    Support,
    Quit,
}

const LIST_X: i32 = 24;
const LIST_Y: i32 = 116;
const LIST_W: i32 = SCREEN_W as i32 - 2 * LIST_X;
const ROW_H: i32 = 40;
/// The rows shown at once.
pub const VISIBLE: usize = 6;
const LIST_H: i32 = VISIBLE as i32 * ROW_H;
const BUTTON_Y: i32 = LIST_Y + LIST_H + 16;
const BUTTON_H: i32 = 26;
const BUTTON_GAP: i32 = 4;
/// A button's room for the sun on its left.
const BUTTON_PAD: i32 = 22;
const MESSAGE_Y: f32 = (BUTTON_Y + BUTTON_H + 22) as f32;
/// The link to the project's Buy Me a Coffee page, above the help line.
const SUPPORT_TEXT: &str = "Enjoying Atlantis Native? Buy me a coffee";
const SUPPORT_BASE: f32 = 447.0;
const HELP_BASE: f32 = 469.0;
/// Two clicks on a row this close (ms) play it.
const DOUBLE_CLICK_MS: u64 = 500;

const LOGO: Style = Style::logo(58.0);
const TAGLINE: Style = Style::caps(13.0);
const NAME: Style = Style::caps(11.5);
const EDITION: Style = Style::body(13.5, art::TEXT);
const PATH: Style = Style::body(12.0, art::MUTED);
const TAG: Style = Style::italic(12.0, art::MUTED);
const BUTTON: Style = Style {
    tracking: 0.07,
    ..Style::caps(11.5)
};
const BUTTON_OFF: Style = Style {
    tracking: 0.07,
    ..Style::caps_in(11.5, (70, 82, 110))
};
const MESSAGE: Style = Style::italic(13.5, art::MUTED);
const SUPPORT: Style = Style::body(13.5, art::LINK);
const HELP: Style = Style::body(11.5, art::MUTED);

/// An entry's label as the game's name and the rest (the edition, or why
/// the folder does not open): "Atlantis II, GOG.com, French" gives
/// ("Atlantis II", "GOG.com, French").
fn split_label(e: &LauncherEntry) -> (Option<&'static str>, &str) {
    let Some(kind) = e.kind else {
        return (None, &e.label);
    };
    let name = kind.name();
    match e.label.strip_prefix(name) {
        Some("") => (Some(name), ""),
        Some(rest) if rest.starts_with(", ") => (Some(name), &rest[2..]),
        _ if !e.found => (Some(name), &e.label),
        _ => (None, &e.label),
    }
}

impl Button {
    /// The button's width: its longer label's and the sun's room.
    fn width(self) -> i32 {
        let w = art::width(&BUTTON, self.label(false)).max(art::width(&BUTTON, self.label(true)));
        w.ceil() as i32 + BUTTON_PAD + 4
    }

    fn label(self, missing: bool) -> &'static str {
        match self {
            Button::Play if missing => "Locate\u{2026}",
            Button::Play => "Play",
            Button::Add => "Add a game folder\u{2026}",
            Button::Find => "Find installations",
            Button::Remove => "Remove",
            Button::Quit => "Quit",
        }
    }
}

/// The buttons' rectangles (x, y, w, h).
fn button_rects() -> [(Button, i32, i32, i32, i32); 5] {
    let total: i32 =
        BUTTONS.iter().map(|b| b.width()).sum::<i32>() + BUTTON_GAP * (BUTTONS.len() as i32 - 1);
    let mut x = (SCREEN_W as i32 - total) / 2;
    BUTTONS.map(|b| {
        let r = (b, x, BUTTON_Y, b.width(), BUTTON_H);
        x += b.width() + BUTTON_GAP;
        r
    })
}

/// The Buy Me a Coffee link's rectangle (x, y, w, h).
fn support_rect() -> (i32, i32, i32, i32) {
    let w = art::width(&SUPPORT, SUPPORT_TEXT).ceil() as i32;
    ((SCREEN_W as i32 - w) / 2, SUPPORT_BASE as i32 - 14, w, 19)
}

/// The screen's state.
#[derive(Debug, Default)]
pub struct Launcher {
    entries: Vec<LauncherEntry>,
    selected: usize,
    /// The first row shown.
    scroll: usize,
    /// The button under the pointer.
    hover: Option<Button>,
    /// The button chosen with Tab / Left / Right (Enter presses it).
    focus: Option<Button>,
    /// A line on what the last action did.
    message: String,
    /// The row and time of the last click, for the double click.
    last_click: Option<(usize, u64)>,
}

impl Launcher {
    pub fn new(entries: Vec<LauncherEntry>) -> Launcher {
        let mut l = Launcher::default();
        l.set_entries(entries, 0);
        l
    }

    /// The list changed: `selected` (clamped) is selected.
    pub fn set_entries(&mut self, entries: Vec<LauncherEntry>, selected: usize) {
        self.entries = entries;
        self.select(selected);
        self.last_click = None;
    }

    pub fn entries(&self) -> &[LauncherEntry] {
        &self.entries
    }

    /// The selected entry's index (0 when the list is empty).
    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_message(&mut self, text: impl Into<String>) {
        self.message = text.into();
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn focus(&self) -> Option<Button> {
        self.focus
    }

    fn select(&mut self, i: usize) {
        self.selected = i.min(self.entries.len().saturating_sub(1));
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + VISIBLE {
            self.scroll = self.selected + 1 - VISIBLE;
        }
        self.scroll = self.scroll.min(self.entries.len().saturating_sub(VISIBLE));
    }

    fn selected_missing(&self) -> bool {
        self.entries.get(self.selected).is_some_and(|e| !e.found)
    }

    /// Whether `b` can be pressed now.
    fn enabled(&self, b: Button) -> bool {
        match b {
            Button::Play | Button::Remove => !self.entries.is_empty(),
            _ => true,
        }
    }

    fn press(&mut self, b: Button) -> LauncherAction {
        if !self.enabled(b) {
            return LauncherAction::None;
        }
        match b {
            Button::Play => self.play(self.selected),
            Button::Add => LauncherAction::Add,
            Button::Find => LauncherAction::Find,
            Button::Remove => LauncherAction::Remove(self.selected),
            Button::Quit => LauncherAction::Quit,
        }
    }

    fn play(&self, i: usize) -> LauncherAction {
        match self.entries.get(i) {
            Some(e) if e.found => LauncherAction::Play(i),
            Some(_) => LauncherAction::Locate(i),
            None => LauncherAction::None,
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let n = BUTTONS.len();
        let at = self
            .focus
            .and_then(|f| BUTTONS.iter().position(|&b| b == f));
        let mut i = match (at, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        for _ in 0..n {
            if self.enabled(BUTTONS[i]) {
                break;
            }
            i = if forward {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            };
        }
        self.focus = Some(BUTTONS[i]);
    }

    /// A key: Up / Down (Home, End, Page Up / Down) choose an entry, Enter
    /// plays it (or presses the button chosen with Tab / Left / Right),
    /// Delete removes it, A adds a folder, F finds installations, Escape
    /// quits.
    pub fn key(&mut self, key: LauncherKey) -> LauncherAction {
        let page = VISIBLE - 1;
        match key {
            LauncherKey::Up => self.select(self.selected.saturating_sub(1)),
            LauncherKey::Down => self.select(self.selected + 1),
            LauncherKey::Home => self.select(0),
            LauncherKey::End => self.select(usize::MAX),
            LauncherKey::PageUp => self.select(self.selected.saturating_sub(page)),
            LauncherKey::PageDown => self.select(self.selected + page),
            LauncherKey::Tab | LauncherKey::Right => self.move_focus(true),
            LauncherKey::Left => self.move_focus(false),
            LauncherKey::Enter => {
                return match self.focus {
                    Some(b) => self.press(b),
                    None => self.press(Button::Play),
                }
            }
            LauncherKey::Delete => return self.press(Button::Remove),
            LauncherKey::Escape => return LauncherAction::Quit,
            LauncherKey::Char(c) => match c.to_ascii_lowercase() {
                'a' => return LauncherAction::Add,
                'f' => return LauncherAction::Find,
                'q' => return LauncherAction::Quit,
                _ => {}
            },
        }
        LauncherAction::None
    }

    /// The row of the list under a 640x480 screen point.
    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        if !(LIST_X..LIST_X + LIST_W).contains(&x) || !(LIST_Y..LIST_Y + LIST_H).contains(&y) {
            return None;
        }
        let i = self.scroll + ((y - LIST_Y) / ROW_H) as usize;
        (i < self.entries.len()).then_some(i)
    }

    fn button_at(x: i32, y: i32) -> Option<Button> {
        button_rects()
            .into_iter()
            .find(|&(_, bx, by, bw, bh)| (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y))
            .map(|r| r.0)
    }

    /// The pointer moved to a 640x480 screen point: the button under it
    /// lights up.
    pub fn pointer(&mut self, x: i32, y: i32) {
        self.hover = Self::button_at(x, y);
    }

    /// A left click at a 640x480 screen point at `now_ms`: a row is
    /// selected (a second click on it soon after plays it), a button
    /// pressed.
    pub fn click(&mut self, x: i32, y: i32, now_ms: u64) -> LauncherAction {
        if let Some(i) = self.row_at(x, y) {
            let double = self
                .last_click
                .is_some_and(|(j, t)| j == i && now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS);
            self.select(i);
            self.focus = None;
            if double {
                self.last_click = None;
                return self.play(i);
            }
            self.last_click = Some((i, now_ms));
            return LauncherAction::None;
        }
        self.last_click = None;
        if Self::on_support_link(x, y) {
            return LauncherAction::Support;
        }
        match Self::button_at(x, y) {
            Some(b) => self.press(b),
            None => LauncherAction::None,
        }
    }

    /// Whether a screen point is on the Buy Me a Coffee link.
    fn on_support_link(x: i32, y: i32) -> bool {
        let (sx, sy, sw, sh) = support_rect();
        (sx..sx + sw).contains(&x) && (sy..sy + sh).contains(&y)
    }

    /// The mouse wheel: `rows` down (negative: up).
    pub fn wheel(&mut self, rows: i32) {
        let max = self.entries.len().saturating_sub(VISIBLE) as i64;
        self.scroll = (self.scroll as i64 + i64::from(rows)).clamp(0, max) as usize;
    }

    /// Draws the screen on a 640x480 RGB565 frame, its text and ornaments
    /// included.
    pub fn draw(&self, screen: &mut [u16]) {
        for item in self.draw_scene(screen) {
            art::draw(&mut Frame565(screen), &item);
        }
    }

    /// Draws the screen's background on a 640x480 RGB565 frame and gives
    /// its text and ornaments, for the front end to draw over the frame
    /// once it is scaled to the window ([`art::draw`] on an
    /// [`art::Window`]).
    pub fn draw_scene(&self, screen: &mut [u16]) -> Vec<Item> {
        let mut items = Vec::new();
        let cx = SCREEN_W as f32 / 2.0;
        let centred = |style: &Style, t: &str, baseline: f32| {
            Item::text(*style, t, cx - art::width(style, t) / 2.0, baseline)
        };
        screen.copy_from_slice(art::background());
        // The logo, under it the tagline between two spears.
        items.push(centred(&LOGO, "Atlantis", 62.0));
        let t = "Choose a game";
        let tw = art::width(&TAGLINE, t);
        items.push(centred(&TAGLINE, t, 92.0));
        let (sw, sy) = (60.0, 92.0 - art::cap_height(&TAGLINE) / 2.0 - 10.0);
        items.push(Item::Spear {
            x: cx - tw / 2.0 - 16.0 - sw,
            y: sy,
            w: sw,
            flip: false,
        });
        items.push(Item::Spear {
            x: cx + tw / 2.0 + 16.0,
            y: sy,
            w: sw,
            flip: true,
        });
        // The tablet: the list on a dark veil between double gold rules.
        let (tx, tw) = (LIST_X, LIST_W);
        art::shade(screen, (tx, LIST_Y - 8, tw, LIST_H + 16), art::TABLET, 0.72);
        for y in [
            LIST_Y - 11,
            LIST_Y - 9,
            LIST_Y + LIST_H + 8,
            LIST_Y + LIST_H + 10,
        ] {
            art::shade(screen, (tx, y, tw, 1), art::GOLD, 0.75);
        }
        if self.entries.is_empty() {
            let mid = (LIST_Y + LIST_H / 2) as f32;
            items.push(centred(&EDITION, "No game in the list yet.", mid - 6.0));
            items.push(centred(
                &MESSAGE,
                "Add the folder of your game, or let Atlantis Native find it.",
                mid + 16.0,
            ));
        }
        let scrolls = self.entries.len() > VISIBLE;
        let right = LIST_X + LIST_W - if scrolls { 18 } else { 12 };
        let text_x = LIST_X + 40;
        let rows = self.entries.iter().enumerate().skip(self.scroll);
        for (i, e) in rows.take(VISIBLE) {
            let y = LIST_Y + (i - self.scroll) as i32 * ROW_H;
            let sel = i == self.selected;
            if sel {
                art::shade(
                    screen,
                    (LIST_X + 4, y + 1, LIST_W - 8, ROW_H - 2),
                    (60, 100, 170),
                    0.22,
                );
                items.push(Item::Sun {
                    cx: (LIST_X + 20) as f32,
                    cy: (y + ROW_H / 2) as f32,
                    r: 9.0,
                });
            } else if i + 1 < self.scroll + VISIBLE
                && i + 1 < self.entries.len()
                && i + 1 != self.selected
            {
                art::shade(
                    screen,
                    (text_x, y + ROW_H - 1, right - text_x, 1),
                    art::GOLD,
                    0.16,
                );
            }
            // The game's name in the gold capitals, its edition beside it,
            // the folder under them.
            let line = (y + 17) as f32;
            let mut w = (right - text_x) as f32;
            if i == 0 && e.found {
                let tag = "last played";
                let tgw = art::width(&TAG, tag);
                items.push(Item::text(TAG, tag, right as f32 - tgw, line));
                w -= tgw + 14.0;
            }
            let (name, rest) = split_label(e);
            let rest_style = if e.found {
                EDITION
            } else {
                Style::body(13.5, art::ERROR)
            };
            let mut x = text_x as f32;
            if let Some(name) = name {
                let style = if e.found {
                    NAME
                } else {
                    Style::caps_in(11.5, art::MUTED)
                };
                let n = art::fit_right(&style, name, w);
                let nw = art::width(&style, &n);
                items.push(Item::text(style, &n, x, line));
                x += nw + 12.0;
                w -= nw + 12.0;
            }
            if !rest.is_empty() && w > 20.0 {
                let r = art::fit_right(&rest_style, rest, w);
                items.push(Item::text(rest_style, &r, x, line));
            }
            let path = art::fit_left(
                &PATH,
                &e.path.display().to_string(),
                (right - text_x) as f32,
            );
            items.push(Item::text(PATH, &path, text_x as f32, (y + 34) as f32));
        }
        // A scroll bar when the list is longer than the tablet.
        let n = self.entries.len();
        if scrolls {
            let track = LIST_H - 8;
            let bar_h = (track * VISIBLE as i32 / n as i32).max(16);
            let bar_y = LIST_Y + 4 + (track - bar_h) * self.scroll as i32 / (n - VISIBLE) as i32;
            let x = LIST_X + LIST_W - 9;
            art::shade(screen, (x, LIST_Y + 4, 2, track), art::GOLD, 0.18);
            art::shade(screen, (x, bar_y, 2, bar_h), art::GOLD, 0.7);
        }
        // The buttons: the gold capitals of the game's menus, the sun
        // beside the one under the pointer.
        let missing = self.selected_missing();
        for (b, x, y, w, h) in button_rects() {
            let on = self.hover == Some(b) || self.focus == Some(b);
            let enabled = self.enabled(b);
            let style = if enabled { BUTTON } else { BUTTON_OFF };
            let base = (y + h / 2) as f32 + art::cap_height(&style) / 2.0;
            // (the label centred in the button's room, the sun before it)
            let label = b.label(missing);
            let room = (w - BUTTON_PAD - 4) as f32;
            let lx = (x + BUTTON_PAD) as f32 + (room - art::width(&style, label)) / 2.0;
            items.push(Item::text(style, label, lx, base));
            if on && enabled {
                items.push(Item::Sun {
                    cx: lx - 12.0,
                    cy: (y + h / 2) as f32,
                    r: 9.0,
                });
            }
        }
        if !self.message.is_empty() {
            let m = art::fit_right(&MESSAGE, &self.message, (SCREEN_W - 32) as f32);
            items.push(centred(&MESSAGE, &m, MESSAGE_Y));
        }
        items.push(centred(&SUPPORT, SUPPORT_TEXT, SUPPORT_BASE));
        let t =
            "Arrows choose \u{b7} Enter or double-click plays \u{b7} Del removes \u{b7} Esc quits";
        items.push(centred(&HELP, t, HELP_BASE));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_engine::warp::SCREEN_H;

    fn entry(path: &str, found: bool) -> LauncherEntry {
        LauncherEntry {
            path: PathBuf::from(path),
            label: if found {
                "GOG.com, French (1999 build)".to_string()
            } else {
                "Not found".to_string()
            },
            found,
            kind: Some(GameKind::LostTales),
        }
    }

    fn three() -> Launcher {
        Launcher::new(vec![
            entry("/a", true),
            entry("/b", false),
            entry("/c", true),
        ])
    }

    fn row_y(i: i32) -> i32 {
        LIST_Y + i * ROW_H + 10
    }

    fn button_centre(b: Button) -> (i32, i32) {
        let (_, x, y, w, h) = button_rects().into_iter().find(|r| r.0 == b).unwrap();
        (x + w / 2, y + h / 2)
    }

    #[test]
    fn keys_choose_and_play() {
        let mut l = three();
        assert_eq!(l.selected(), 0);
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::Play(0));
        assert_eq!(l.key(LauncherKey::Down), LauncherAction::None);
        // A folder not found: Enter asks where it is.
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::Locate(1));
        assert_eq!(l.key(LauncherKey::Delete), LauncherAction::Remove(1));
        l.key(LauncherKey::Down);
        l.key(LauncherKey::Down);
        assert_eq!(l.selected(), 2);
        l.key(LauncherKey::Home);
        assert_eq!(l.selected(), 0);
        l.key(LauncherKey::End);
        assert_eq!(l.selected(), 2);
        l.key(LauncherKey::Up);
        assert_eq!(l.selected(), 1);
        assert_eq!(l.key(LauncherKey::Escape), LauncherAction::Quit);
        assert_eq!(l.key(LauncherKey::Char('A')), LauncherAction::Add);
        assert_eq!(l.key(LauncherKey::Char('f')), LauncherAction::Find);
        assert_eq!(l.key(LauncherKey::Char('x')), LauncherAction::None);
    }

    #[test]
    fn tab_chooses_a_button() {
        let mut l = three();
        l.key(LauncherKey::Tab);
        assert_eq!(l.focus(), Some(Button::Play));
        l.key(LauncherKey::Tab);
        assert_eq!(l.focus(), Some(Button::Add));
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::Add);
        l.key(LauncherKey::Right);
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::Find);
        l.key(LauncherKey::Left);
        l.key(LauncherKey::Left);
        assert_eq!(l.focus(), Some(Button::Play));
        l.key(LauncherKey::Left);
        assert_eq!(l.focus(), Some(Button::Quit));
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::Quit);
        // A click on a row gives the keys back to the list.
        l.click(100, row_y(2), 0);
        assert_eq!(l.focus(), None);
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::Play(2));
    }

    #[test]
    fn clicks_select_and_double_clicks_play() {
        let mut l = three();
        assert_eq!(l.click(100, row_y(2), 1000), LauncherAction::None);
        assert_eq!(l.selected(), 2);
        assert_eq!(l.click(100, row_y(2), 1300), LauncherAction::Play(2));
        // Too slow: two single clicks.
        assert_eq!(l.click(100, row_y(0), 2000), LauncherAction::None);
        assert_eq!(l.click(100, row_y(0), 3000), LauncherAction::None);
        // Two rows: no double click.
        assert_eq!(l.click(100, row_y(1), 3100), LauncherAction::None);
        assert_eq!(l.selected(), 1);
        // Below the last entry: nothing.
        assert_eq!(l.click(100, row_y(4), 3200), LauncherAction::None);
        assert_eq!(l.selected(), 1);
        let (x, y) = button_centre(Button::Play);
        assert_eq!(l.click(x, y, 4000), LauncherAction::Locate(1));
        let (x, y) = button_centre(Button::Remove);
        assert_eq!(l.click(x, y, 4100), LauncherAction::Remove(1));
        let (x, y) = button_centre(Button::Find);
        assert_eq!(l.click(x, y, 4200), LauncherAction::Find);
        let (x, y) = button_centre(Button::Add);
        assert_eq!(l.click(x, y, 4300), LauncherAction::Add);
        let (x, y) = button_centre(Button::Quit);
        assert_eq!(l.click(x, y, 4400), LauncherAction::Quit);
        assert_eq!(l.click(2, 2, 4500), LauncherAction::None);
        l.pointer(x, y);
        assert_eq!(l.hover, Some(Button::Quit));
        l.pointer(2, 2);
        assert_eq!(l.hover, None);
    }

    #[test]
    fn an_empty_list_can_only_add_find_or_quit() {
        let mut l = Launcher::new(Vec::new());
        assert_eq!(l.key(LauncherKey::Enter), LauncherAction::None);
        assert_eq!(l.key(LauncherKey::Delete), LauncherAction::None);
        assert_eq!(l.key(LauncherKey::Down), LauncherAction::None);
        l.key(LauncherKey::Tab);
        assert_eq!(l.focus(), Some(Button::Add));
        let (x, y) = button_centre(Button::Play);
        assert_eq!(l.click(x, y, 0), LauncherAction::None);
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        l.draw(&mut s);
        assert_ne!(s, art::background());
        let items = l.draw_scene(&mut s);
        assert!(items
            .iter()
            .any(|i| matches!(i, Item::Text { text, .. } if text == "No game in the list yet.")));
    }

    #[test]
    fn a_long_list_scrolls() {
        let mut l = Launcher::new((0..10).map(|i| entry(&format!("/g{i}"), true)).collect());
        l.key(LauncherKey::End);
        assert_eq!((l.selected(), l.scroll), (9, 4));
        // The row under the pointer counts from the first shown.
        assert_eq!(l.row_at(100, row_y(0)), Some(4));
        l.key(LauncherKey::PageUp);
        assert_eq!((l.selected(), l.scroll), (4, 4));
        l.key(LauncherKey::Up);
        assert_eq!((l.selected(), l.scroll), (3, 3));
        l.wheel(-10);
        assert_eq!(l.scroll, 0);
        l.wheel(10);
        assert_eq!(l.scroll, 4);
        l.set_entries(vec![entry("/a", true)], 7);
        assert_eq!((l.selected(), l.scroll), (0, 0));
    }

    fn texts(items: &[Item]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn draws_the_list_and_the_buttons() {
        // The buttons fit across the screen, the sun's room beside each.
        let rects = button_rects();
        assert!(rects[0].1 >= 4 && rects[4].1 + rects[4].3 <= SCREEN_W as i32 - 4);
        for b in BUTTONS {
            for missing in [false, true] {
                let w = art::width(&BUTTON, b.label(missing)).ceil() as i32;
                assert!(w + BUTTON_PAD <= b.width(), "{b:?}");
            }
        }
        const { assert!(MESSAGE_Y + 14.0 < SUPPORT_BASE && SUPPORT_BASE + 18.0 < HELP_BASE) };
        const { assert!(HELP_BASE + 6.0 < SCREEN_H as f32) };
        let mut l = three();
        l.set_message("2 installations found");
        let mut bare = vec![0u16; SCREEN_W * SCREEN_H];
        let items = l.draw_scene(&mut bare);
        let t = texts(&items);
        for want in [
            "Atlantis",
            "Choose a game",
            "Not found",
            "last played",
            "2 installations found",
        ] {
            assert!(t.contains(&want), "{want}: {t:?}");
        }
        // The selected row (the first) and its sun; the spears.
        let suns = items
            .iter()
            .filter(|i| matches!(i, Item::Sun { .. }))
            .count();
        assert_eq!(suns, 1);
        assert_eq!(
            items
                .iter()
                .filter(|i| matches!(i, Item::Spear { .. }))
                .count(),
            2
        );
        // Hovering a button shows a second sun.
        let (x, y) = button_centre(Button::Find);
        l.pointer(x, y);
        let items = l.draw_scene(&mut bare);
        assert_eq!(
            items
                .iter()
                .filter(|i| matches!(i, Item::Sun { .. }))
                .count(),
            2
        );
        // Drawn whole on the frame: the text changes it.
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        l.draw(&mut s);
        assert_ne!(s, bare);
    }

    #[test]
    fn labels_split_into_the_game_and_its_edition() {
        let e = |label: &str, found: bool, kind| LauncherEntry {
            path: PathBuf::from("/g"),
            label: label.to_string(),
            found,
            kind,
        };
        let a2 = Some(GameKind::Atlantis2);
        let a3 = Some(GameKind::Atlantis3);
        assert_eq!(
            split_label(&e("Atlantis II, GOG.com, French (patch 1.01)", true, a2)),
            (Some("Atlantis II"), "GOG.com, French (patch 1.01)")
        );
        // "Atlantis II" is not the start of Atlantis III's name.
        assert_eq!(
            split_label(&e("Atlantis III, GOG.com, French", true, a3)),
            (Some("Atlantis III"), "GOG.com, French")
        );
        assert_eq!(
            split_label(&e("Not found: the folder was moved", false, a2)),
            (Some("Atlantis II"), "Not found: the folder was moved")
        );
        assert_eq!(
            split_label(&e("Not found: gone", false, None)),
            (None, "Not found: gone")
        );
    }

    #[test]
    fn the_support_link() {
        let mut l = Launcher::new(Vec::new());
        let (x, y, w, h) = support_rect();
        assert_eq!(l.click(x + w / 2, y + h / 2, 0), LauncherAction::Support);
        assert_eq!(l.click(x - 5, y + h / 2, 0), LauncherAction::None);
    }
}
