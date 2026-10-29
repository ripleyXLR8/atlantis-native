//! The game library's screen (desktop): the game folders of the library,
//! each shown as its game and edition (the first game's or Atlantis II's)
//! and its path, to choose the one to play;
//! drawn in the window on the 640x480 screen, in the style of the options
//! menu ([`atlantis_engine::options_menu`]). This module is the screen's
//! state, input and drawing; the front end carries out its actions.

use std::path::PathBuf;

use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};

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

const BOX_X: i32 = 16;
const BOX_Y: i32 = 12;
const BOX_W: i32 = SCREEN_W as i32 - 2 * BOX_X;
const BOX_H: i32 = SCREEN_H as i32 - 2 * BOX_Y;
const LIST_X: i32 = BOX_X + 12;
const LIST_Y: i32 = BOX_Y + 62;
const LIST_W: i32 = BOX_W - 24;
const ROW_H: i32 = 44;
/// The rows shown at once.
pub const VISIBLE: usize = 6;
const LIST_H: i32 = VISIBLE as i32 * ROW_H;
const BUTTON_Y: i32 = LIST_Y + LIST_H + 14;
const BUTTON_H: i32 = 26;
const BUTTON_GAP: i32 = 9;
const MESSAGE_Y: i32 = BUTTON_Y + BUTTON_H + 14;
const HELP_Y: i32 = BOX_Y + BOX_H - 26;
/// The link to the project's Buy Me a Coffee page, above the help line.
const SUPPORT_TEXT: &[u8] = b"Enjoying Atlantis Native? Buy me a coffee";
const SUPPORT_Y: i32 = HELP_Y - 22;
const SUPPORT_W: i32 = 320;
const SUPPORT_X: i32 = BOX_X + (BOX_W - SUPPORT_W) / 2;
const LINK: u16 = 0xfea0;
/// Two clicks on a row this close (ms) play it.
const DOUBLE_CLICK_MS: u64 = 500;

const BG: u16 = 0x18e3;
const SCREEN_BG: u16 = 0x0000;
const LIST_BG: u16 = 0x1082;
const BORDER: u16 = 0xbdf7;
const HIGHLIGHT: u16 = 0x3a0c;
const BUTTON_BG: u16 = 0x2945;
const TEXT: u16 = 0xffff;
const DIM: u16 = 0x9cf3;
const GREYED: u16 = 0x632c;
const SELECTED: u16 = 0xffe0;
const MISSING: u16 = 0xfbef;

impl Button {
    fn width(self) -> i32 {
        match self {
            Button::Play => 90,
            Button::Add => 150,
            Button::Find => 140,
            Button::Remove => 90,
            Button::Quit => 70,
        }
    }

    fn label(self, missing: bool) -> &'static str {
        match self {
            Button::Play if missing => "Locate...",
            Button::Play => "Play",
            Button::Add => "Add a game folder...",
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
    let mut x = BOX_X + (BOX_W - total) / 2;
    BUTTONS.map(|b| {
        let r = (b, x, BUTTON_Y, b.width(), BUTTON_H);
        x += b.width() + BUTTON_GAP;
        r
    })
}

/// `text` as Windows-1252 bytes (the font's code page), '?' for what it
/// cannot show.
fn bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c as u32 {
            0x20..=0x7e | 0xa0..=0xff => c as u32 as u8,
            0x2026 => 0x85,
            _ => b'?',
        })
        .collect()
}

/// `text` cut on the right with "..." to fit `max_w` pixels (a label
/// keeps its start, the game's name).
fn fit_right(font: &SubtitleFont, text: &str, max_w: i32) -> Vec<u8> {
    let mut b = bytes(text);
    if font.width(&b) <= max_w {
        return b;
    }
    let dots = font.width(b"...");
    while !b.is_empty() && font.width(&b) + dots > max_w {
        b.pop();
    }
    b.extend_from_slice(b"...");
    b
}

/// `text` cut on the left with "..." to fit `max_w` pixels (a path keeps
/// its end, the game's folder).
fn fit_left(font: &SubtitleFont, text: &str, max_w: i32) -> Vec<u8> {
    let b = bytes(text);
    if font.width(&b) <= max_w {
        return b;
    }
    let dots = font.width(b"...");
    let mut start = 0;
    while start < b.len() && font.width(&b[start..]) + dots > max_w {
        start += 1;
    }
    let mut out = b"...".to_vec();
    out.extend_from_slice(&b[start..]);
    out
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
        (SUPPORT_X..SUPPORT_X + SUPPORT_W).contains(&x)
            && (SUPPORT_Y - 2..SUPPORT_Y + 18).contains(&y)
    }

    /// The mouse wheel: `rows` down (negative: up).
    pub fn wheel(&mut self, rows: i32) {
        let max = self.entries.len().saturating_sub(VISIBLE) as i64;
        self.scroll = (self.scroll as i64 + i64::from(rows)).clamp(0, max) as usize;
    }

    /// Draws the screen on a 640x480 RGB565 frame.
    pub fn draw(&self, screen: &mut [u16], font: &SubtitleFont) {
        let fill = |screen: &mut [u16], x: i32, y: i32, w: i32, h: i32, c: u16| {
            for yy in y.max(0)..(y + h).min(SCREEN_H as i32) {
                for xx in x.max(0)..(x + w).min(SCREEN_W as i32) {
                    screen[yy as usize * SCREEN_W + xx as usize] = c;
                }
            }
        };
        let text = |screen: &mut [u16], t: &[u8], x: i32, y: i32, max_w: i32, c: u16| {
            font.draw(screen, SCREEN_W, SCREEN_H, t, x, y, max_w, c);
        };
        let centre = |t: &[u8]| BOX_X + (BOX_W - font.width(t)) / 2;
        screen.fill(SCREEN_BG);
        fill(screen, BOX_X - 1, BOX_Y - 1, BOX_W + 2, BOX_H + 2, BORDER);
        fill(screen, BOX_X, BOX_Y, BOX_W, BOX_H, BG);
        let t = b"Atlantis Native: game library";
        text(screen, t, centre(t), BOX_Y + 12, BOX_W, TEXT);
        let t = b"Choose the game to play (Atlantis: The Lost Tales or Atlantis II)";
        text(screen, t, centre(t), BOX_Y + 34, BOX_W, DIM);
        // The list.
        fill(
            screen,
            LIST_X - 1,
            LIST_Y - 1,
            LIST_W + 2,
            LIST_H + 2,
            GREYED,
        );
        fill(screen, LIST_X, LIST_Y, LIST_W, LIST_H, LIST_BG);
        if self.entries.is_empty() {
            let t = b"No game in the list yet.";
            text(screen, t, centre(t), LIST_Y + 100, BOX_W, TEXT);
            let t = b"Add the folder of your game, or let Atlantis Native find it.";
            text(screen, t, centre(t), LIST_Y + 122, BOX_W, DIM);
        }
        let rows = self.entries.iter().enumerate().skip(self.scroll);
        for (i, e) in rows.take(VISIBLE) {
            let y = LIST_Y + (i - self.scroll) as i32 * ROW_H;
            let sel = i == self.selected;
            if sel {
                fill(screen, LIST_X + 3, y + 2, LIST_W - 6, ROW_H - 4, HIGHLIGHT);
            }
            let label_c = match (e.found, sel) {
                (false, _) => MISSING,
                (true, true) => SELECTED,
                (true, false) => TEXT,
            };
            let w = LIST_W - 24;
            let mut label = fit_right(font, &e.label, w);
            if i == 0 && e.found {
                let tag = b"last played";
                let tw = font.width(tag);
                text(screen, tag, LIST_X + LIST_W - 12 - tw, y + 5, tw + 2, DIM);
                label = fit_right(font, &e.label, w - tw - 12);
            }
            text(screen, &label, LIST_X + 12, y + 5, w, label_c);
            let path = fit_left(font, &e.path.display().to_string(), w);
            let path_c = if e.found { DIM } else { GREYED };
            text(screen, &path, LIST_X + 12, y + 23, w, path_c);
        }
        // A scroll bar when the list is longer than the box.
        let n = self.entries.len();
        if n > VISIBLE {
            let bar_h = (LIST_H * VISIBLE as i32 / n as i32).max(12);
            let bar_y = LIST_Y + (LIST_H - bar_h) * self.scroll as i32 / (n - VISIBLE) as i32;
            fill(screen, LIST_X + LIST_W - 5, bar_y, 3, bar_h, DIM);
        }
        // The buttons.
        let missing = self.selected_missing();
        for (b, x, y, w, h) in button_rects() {
            let on = self.hover == Some(b) || self.focus == Some(b);
            let enabled = self.enabled(b);
            fill(
                screen,
                x - 1,
                y - 1,
                w + 2,
                h + 2,
                if on && enabled { SELECTED } else { GREYED },
            );
            fill(
                screen,
                x,
                y,
                w,
                h,
                if on && enabled { HIGHLIGHT } else { BUTTON_BG },
            );
            let t = b.label(missing).as_bytes();
            let c = match (enabled, on) {
                (false, _) => GREYED,
                (true, true) => SELECTED,
                (true, false) => TEXT,
            };
            text(screen, t, x + (w - font.width(t)) / 2, y + 5, w, c);
        }
        if !self.message.is_empty() {
            let m = bytes(&self.message);
            text(
                screen,
                &m,
                centre(&m).max(BOX_X + 8),
                MESSAGE_Y,
                BOX_W - 16,
                TEXT,
            );
        }
        let x = centre(SUPPORT_TEXT);
        text(screen, SUPPORT_TEXT, x, SUPPORT_Y, BOX_W, LINK);
        fill(screen, x, SUPPORT_Y + 15, font.width(SUPPORT_TEXT), 1, LINK);
        let t = b"Arrows choose, Enter or double-click plays, Del removes, Esc quits";
        text(screen, t, centre(t), HELP_Y, BOX_W, DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, found: bool) -> LauncherEntry {
        LauncherEntry {
            path: PathBuf::from(path),
            label: if found {
                "GOG.com, French (1999 build)".to_string()
            } else {
                "Not found".to_string()
            },
            found,
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
        let font = SubtitleFont::new();
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        l.draw(&mut s, &font);
        assert!(s.contains(&TEXT));
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

    #[test]
    fn draws_the_list_and_the_buttons() {
        let font = SubtitleFont::new();
        for b in BUTTONS {
            for missing in [false, true] {
                assert!(
                    font.width(b.label(missing).as_bytes()) + 12 <= b.width(),
                    "{b:?}"
                );
            }
        }
        let (_, last_x, _, last_w, _) = button_rects()[4];
        assert!(button_rects()[0].1 >= BOX_X && last_x + last_w <= BOX_X + BOX_W);
        assert!(HELP_Y > MESSAGE_Y + 18 && BOX_Y + BOX_H <= SCREEN_H as i32);
        let mut l = three();
        l.set_message("2 installations found");
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        l.draw(&mut s, &font);
        for c in [BG, HIGHLIGHT, SELECTED, MISSING, DIM, TEXT] {
            assert!(s.contains(&c), "{c:04x}");
        }
    }

    #[test]
    fn long_paths_keep_their_end() {
        let font = SubtitleFont::new();
        let long = format!("/home/player/{}/Atlantis", "very-long-folder".repeat(20));
        let b = fit_left(&font, &long, 300);
        assert!(font.width(&b) <= 300);
        assert!(b.starts_with(b"...") && b.ends_with(b"/Atlantis"));
        assert_eq!(fit_left(&font, "/short", 300), b"/short");
        // A label keeps its start, the game's name.
        let label = "Atlantis: The Lost Tales, GOG.com, French (1999 build), from disc images";
        let b = fit_right(&font, label, 250);
        assert!(font.width(&b) <= 250);
        assert!(b.starts_with(b"Atlantis: The") && b.ends_with(b"..."));
        assert_eq!(fit_right(&font, "Atlantis II", 300), b"Atlantis II");
        assert_eq!(bytes("Jeux\u{e9}\u{4e2d}"), b"Jeux\xe9?");
    }

    #[test]
    fn the_support_link() {
        let mut l = Launcher::new(Vec::new());
        assert_eq!(
            l.click(SUPPORT_X + SUPPORT_W / 2, SUPPORT_Y + 6, 0),
            LauncherAction::Support
        );
        assert_eq!(
            l.click(SUPPORT_X - 5, SUPPORT_Y + 6, 0),
            LauncherAction::None
        );
    }
}
