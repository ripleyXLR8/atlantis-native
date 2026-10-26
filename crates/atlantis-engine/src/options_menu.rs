//! The port's options menu (F10), drawn by the front end over whatever
//! frame the game shows: settings of the port that the original's menus do
//! not have. The game is paused while it is open.
//!
//! Entries: the graphics mode (see [`crate::hd`]), the three groups of
//! [`Enhancements`] (the names of the ScummVM port's options), Close, and
//! on the desktop "Game library..." (back to the list of the player's
//! game folders, to play another edition).

use crate::enhancements::Enhancements;
use crate::hd::{HdOptions, HdText, PanoFilter, TextItem, VideoSides};
use crate::persist::Graphics;
use crate::subtitles::SubtitleFont;
use crate::sys::Key;
use crate::warp::{SCREEN_H, SCREEN_W};

/// An entry of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Graphics,
    PanoramaFilter,
    Sharpen,
    Interface,
    SharpSubtitles,
    Widescreen,
    VideoSides,
    BugFixes,
    RestoredContent,
    UiUx,
    /// Open the project's Buy Me a Coffee page in the browser.
    Support,
    Close,
    /// Stop this game and choose another in the game library.
    Library,
}

const ENTRIES: [Entry; 13] = [
    Entry::Graphics,
    Entry::PanoramaFilter,
    Entry::Sharpen,
    Entry::Interface,
    Entry::SharpSubtitles,
    Entry::Widescreen,
    Entry::VideoSides,
    Entry::BugFixes,
    Entry::RestoredContent,
    Entry::UiUx,
    Entry::Support,
    Entry::Close,
    Entry::Library,
];

impl Entry {
    /// An option of the HD mode (greyed and fixed in the original mode).
    pub fn hd_only(self) -> bool {
        matches!(
            self,
            Entry::PanoramaFilter
                | Entry::Sharpen
                | Entry::Interface
                | Entry::SharpSubtitles
                | Entry::Widescreen
                | Entry::VideoSides
        )
    }
}

/// What the front end does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    /// The graphics mode was changed: apply and save it.
    SetGraphics(Graphics),
    /// An HD option was changed: apply and save them.
    SetHd(HdOptions),
    /// An enhancement group was switched: apply and save them.
    SetEnhancements(Enhancements),
    /// The menu closed.
    Close,
    /// Stop the game and show the game library (the menu closed).
    OpenLibrary,
    /// Open the project's Buy Me a Coffee page (the menu stays open).
    OpenSupport,
}

/// The values the menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuValues {
    pub graphics: Graphics,
    pub hd: HdOptions,
    pub enhancements: Enhancements,
}

impl MenuValues {
    fn enabled(&self, e: Entry) -> bool {
        !e.hd_only() || self.graphics == Graphics::Hd
    }
}

/// The box's width; a row's height; the title's; the footer's: two lines
/// on the game and the port, the selected entry's description, the keys.
pub const BOX_W: i32 = 400;
pub const ROW_H: i32 = 24;
pub const TITLE_H: i32 = 34;
pub const FOOTER_H: i32 = 86;
#[cfg(test)]
const BOX_X: i32 = (SCREEN_W as i32 - BOX_W) / 2;

/// The box's colours (RGB565).
pub const BG: u16 = 0x18e3;
pub const BORDER: u16 = 0xbdf7;
pub const HIGHLIGHT: u16 = 0x3a0c;
pub const TEXT: u16 = 0xffff;
pub const DIM: u16 = 0x9cf3;
pub const GREYED: u16 = 0x632c;
pub const SELECTED: u16 = 0xffe0;

/// A row of an options box ([`MenuBox`]): a setting, its label at the
/// left and `< value >` at the right, or a button (no value), its label
/// centred; greyed (skipped by the keys, fixed under a click) when not
/// enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row<'a> {
    pub label: &'a str,
    pub value: Option<&'a str>,
    pub enabled: bool,
}

/// What an input did to an options box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxInput {
    None,
    /// Row `i` is to change (a click on it, Enter or Space, Left or
    /// Right on a setting): the game's menu checks that it is enabled.
    Activate(usize),
    /// The box closed (Escape, F10, O, a click outside).
    Close,
}

/// A gap between two groups of rows of a box ([`BoxLayout::gaps`]).
pub const GAP_H: i32 = 8;

/// The layout of an options box: its title, its width, the gaps between
/// groups of rows and the height of its foot. [`BoxLayout::OPTIONS`] is the
/// options menu's (both games').
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxLayout<'a> {
    pub title: &'a str,
    pub width: i32,
    /// The rows that start a group: a [`GAP_H`]-pixel gap above each.
    pub gaps: &'a [usize],
    pub footer_h: i32,
}

impl BoxLayout<'_> {
    /// The options menu's box.
    pub const OPTIONS: BoxLayout<'static> = BoxLayout {
        title: "Atlantis Native options",
        width: BOX_W,
        gaps: &[],
        footer_h: FOOTER_H,
    };

    /// The height of a box of `n` rows.
    pub fn box_h(&self, n: usize) -> i32 {
        let gaps = self.gaps.iter().filter(|&&g| g < n).count() as i32;
        TITLE_H + n as i32 * ROW_H + gaps * GAP_H + self.footer_h
    }

    /// The top left corner of a box of `n` rows on a `w` x `h` screen
    /// (centred).
    pub fn origin(&self, n: usize, w: usize, h: usize) -> (i32, i32) {
        ((w as i32 - self.width) / 2, (h as i32 - self.box_h(n)) / 2)
    }

    /// The top of row `i`, from the box's top.
    pub fn row_y(&self, i: usize) -> i32 {
        let gaps = self.gaps.iter().filter(|&&g| g <= i).count() as i32;
        TITLE_H + i as i32 * ROW_H + gaps * GAP_H
    }

    /// The row under a point of a `w` x `h` screen, for `n` rows.
    fn row_at(&self, n: usize, x: i32, y: i32, w: usize, h: usize) -> Option<usize> {
        let (bx, by) = self.origin(n, w, h);
        if !(bx..bx + self.width).contains(&x) {
            return None;
        }
        (0..n).find(|&i| (0..ROW_H).contains(&(y - by - self.row_y(i))))
    }
}

/// A line of a box's foot: its text, its top (from the box's bottom) and
/// its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FootLine<'a> {
    pub text: &'a str,
    pub from_bottom: i32,
    pub color: u16,
}

/// The options box of the port's menus, both games': its layout (centred
/// on the screen, whatever its size), colours, font, keys and mouse. The
/// game's menu gives the rows and acts on [`BoxInput::Activate`]. The
/// methods without a layout are the options menu's
/// ([`BoxLayout::OPTIONS`]); the `_in` ones take another box's (Atlantis
/// II's bug fixes).
#[derive(Clone, Debug, Default)]
pub struct MenuBox {
    open: bool,
    selected: usize,
}

impl MenuBox {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the box, its first row selected.
    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// The selected row.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Selects row `i`.
    pub fn select(&mut self, i: usize) {
        self.selected = i;
    }

    /// Keeps the selection among `n` rows.
    pub fn clamp(&mut self, n: usize) {
        self.selected = self.selected.min(n.saturating_sub(1));
    }

    /// The height of a box of `n` rows.
    pub fn box_h(n: usize) -> i32 {
        BoxLayout::OPTIONS.box_h(n)
    }

    /// The top left corner of a box of `n` rows on a `w` x `h` screen
    /// (centred).
    pub fn origin(n: usize, w: usize, h: usize) -> (i32, i32) {
        BoxLayout::OPTIONS.origin(n, w, h)
    }

    /// The next row from the selected one that can be changed.
    fn step(&mut self, forward: bool, rows: &[Row]) {
        let n = rows.len();
        let dir = if forward { 1 } else { n - 1 };
        for _ in 0..n {
            self.selected = (self.selected + dir) % n;
            if rows[self.selected].enabled {
                break;
            }
        }
    }

    /// A key pressed while the box is open: Up / Down choose a row
    /// (skipping the greyed ones), Enter / Space (and Left / Right on a
    /// setting) change it, Escape, F10 and O close the box.
    pub fn key(&mut self, key: Key, rows: &[Row]) -> BoxInput {
        match key {
            Key::Escape | Key::F(10) | Key::Char('o' | 'O') => {
                self.close();
                BoxInput::Close
            }
            Key::Up => {
                self.step(false, rows);
                BoxInput::None
            }
            Key::Down => {
                self.step(true, rows);
                BoxInput::None
            }
            Key::Enter | Key::Space => BoxInput::Activate(self.selected),
            Key::Left | Key::Right if rows[self.selected].value.is_some() => {
                BoxInput::Activate(self.selected)
            }
            _ => BoxInput::None,
        }
    }

    /// The pointer moved to a point of a `w` x `h` screen: the row under
    /// it (of `n`) is selected.
    pub fn pointer(&mut self, n: usize, x: i32, y: i32, w: usize, h: usize) {
        self.pointer_in(&BoxLayout::OPTIONS, n, x, y, w, h);
    }

    /// [`MenuBox::pointer`] on a box laid out as `layout`.
    pub fn pointer_in(&mut self, layout: &BoxLayout, n: usize, x: i32, y: i32, w: usize, h: usize) {
        if let Some(i) = layout.row_at(n, x, y, w, h) {
            self.selected = i;
        }
    }

    /// A click at a point of a `w` x `h` screen: the row under it (of
    /// `n`) is selected and changes; outside the box the box closes.
    pub fn click(&mut self, n: usize, x: i32, y: i32, w: usize, h: usize) -> BoxInput {
        self.click_in(&BoxLayout::OPTIONS, n, x, y, w, h)
    }

    /// [`MenuBox::click`] on a box laid out as `layout`.
    pub fn click_in(
        &mut self,
        layout: &BoxLayout,
        n: usize,
        x: i32,
        y: i32,
        w: usize,
        h: usize,
    ) -> BoxInput {
        if let Some(i) = layout.row_at(n, x, y, w, h) {
            self.selected = i;
            return BoxInput::Activate(i);
        }
        let (bx, by) = layout.origin(n, w, h);
        let inside =
            (bx..bx + layout.width).contains(&x) && (by..by + layout.box_h(n)).contains(&y);
        if inside {
            BoxInput::None
        } else {
            self.close();
            BoxInput::Close
        }
    }

    /// Draws the box over a `w` x `h` RGB565 frame: the title, `rows`,
    /// the `info` lines (at most two), the selected row's `description`
    /// and the keys; the text recorded in `record` for the HD mode to
    /// draw sharp (see [`HdText`]).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_recording(
        &self,
        screen: &mut [u16],
        w: usize,
        h: usize,
        font: &SubtitleFont,
        rows: &[Row],
        info: &[String],
        description: &str,
        record: Option<&mut HdText>,
    ) {
        let mut foot: Vec<FootLine> = info
            .iter()
            .take(2)
            .enumerate()
            .map(|(i, line)| FootLine {
                text: line,
                from_bottom: 82 - i as i32 * 18,
                color: DIM,
            })
            .collect();
        foot.push(FootLine {
            text: description,
            from_bottom: 44,
            color: TEXT,
        });
        foot.push(FootLine {
            text: KEYS,
            from_bottom: 22,
            color: DIM,
        });
        self.draw_in(&BoxLayout::OPTIONS, screen, w, h, font, rows, &foot, record);
    }

    /// Draws a box laid out as `layout` over a `w` x `h` RGB565 frame: the
    /// title, `rows` and the `foot` lines, each centred; the text recorded
    /// in `record` for the HD mode to draw sharp (see [`HdText`]).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_in(
        &self,
        layout: &BoxLayout,
        screen: &mut [u16],
        w: usize,
        h: usize,
        font: &SubtitleFont,
        rows: &[Row],
        foot: &[FootLine],
        record: Option<&mut HdText>,
    ) {
        let n = rows.len();
        let bw = layout.width;
        let (bx, by) = layout.origin(n, w, h);
        let bh = layout.box_h(n);
        let mut fill = |x: i32, y: i32, fw: i32, fh: i32, c: u16| {
            for yy in y.max(0)..(y + fh).min(h as i32) {
                for xx in x.max(0)..(x + fw).min(w as i32) {
                    screen[yy as usize * w + xx as usize] = c;
                }
            }
        };
        fill(bx - 1, by - 1, bw + 2, bh + 2, BORDER);
        fill(bx, by, bw, bh, BG);
        let sel_y = by + layout.row_y(self.selected);
        fill(bx + 6, sel_y, bw - 12, ROW_H - 2, HIGHLIGHT);
        let mut items = Vec::new();
        let mut text = |t: &str, x: i32, y: i32, c: u16| {
            items.push(TextItem {
                x,
                y,
                max_w: bw - 20,
                color: c,
                text: t.as_bytes().to_vec(),
                bitmap: None,
                code_page: atlantis_data::TextCodePage::Windows1252,
                face: None,
            });
        };
        let centre = |t: &str| bx + (bw - font.width(t.as_bytes())) / 2;
        let right = |t: &str| bx + bw - 16 - font.width(t.as_bytes());
        text(layout.title, centre(layout.title), by + 10, TEXT);
        for (i, row) in rows.iter().enumerate() {
            let y = by + layout.row_y(i);
            let c = match (row.enabled, i == self.selected) {
                (false, _) => GREYED,
                (true, true) => SELECTED,
                (true, false) => TEXT,
            };
            match row.value {
                Some(value) => {
                    text(row.label, bx + 16, y + 4, c);
                    let t = format!("< {value} >");
                    text(&t, right(&t), y + 4, c);
                }
                None => text(row.label, centre(row.label), y + 4, c),
            }
        }
        for line in foot {
            text(
                line.text,
                centre(line.text),
                by + bh - line.from_bottom,
                line.color,
            );
        }
        let under = record.as_ref().map(|_| screen.to_vec());
        for it in &items {
            font.draw(screen, w, h, &it.text, it.x, it.y, it.max_w, it.color);
        }
        if let (Some(t), Some(under)) = (record, under) {
            t.set(items, &under, screen);
        }
    }
}

/// The keys' line at the foot of the options menu.
pub const KEYS: &str = "Arrows, Enter or click; Escape, F10 or O closes";

/// The menu's state.
#[derive(Debug, Default)]
pub struct OptionsMenu {
    menu: MenuBox,
    /// Shown at the foot of the menu: the game's release, the port's
    /// version (set by the front end).
    info: Vec<String>,
    /// The front end has a game library: the "Game library..." entry.
    library: bool,
}

impl OptionsMenu {
    pub fn new() -> OptionsMenu {
        OptionsMenu::default()
    }

    /// The lines shown at the foot of the menu (at most two).
    pub fn set_info(&mut self, lines: Vec<String>) {
        self.info = lines;
    }

    /// Whether the menu has the "Game library..." entry (the desktop).
    pub fn set_library(&mut self, on: bool) {
        self.library = on;
        self.menu.clamp(self.entries().len());
    }

    /// The entries shown.
    fn entries(&self) -> &'static [Entry] {
        if self.library {
            &ENTRIES
        } else {
            &ENTRIES[..ENTRIES.len() - 1]
        }
    }

    #[cfg(test)]
    fn box_h(&self) -> i32 {
        MenuBox::box_h(self.entries().len())
    }

    #[cfg(test)]
    fn box_y(&self) -> i32 {
        MenuBox::origin(self.entries().len(), SCREEN_W, SCREEN_H).1
    }

    pub fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    pub fn open(&mut self) {
        self.menu.open();
    }

    pub fn close(&mut self) {
        self.menu.close();
    }

    /// The selected entry.
    pub fn selected(&self) -> Entry {
        self.entries()[self.menu.selected()]
    }

    fn activate(&mut self, entry: Entry, v: MenuValues) -> MenuAction {
        if !v.enabled(entry) {
            return MenuAction::None;
        }
        let mut hd = v.hd;
        let mut x = v.enhancements;
        match entry {
            Entry::Graphics => {
                return MenuAction::SetGraphics(match v.graphics {
                    Graphics::Original => Graphics::Hd,
                    Graphics::Hd => Graphics::Original,
                })
            }
            Entry::PanoramaFilter => {
                hd.panorama = match hd.panorama {
                    PanoFilter::Bilinear => PanoFilter::Bicubic,
                    PanoFilter::Bicubic => PanoFilter::Bilinear,
                };
            }
            Entry::Sharpen => hd.sharpen = !hd.sharpen,
            Entry::Interface => hd.smooth_interface = !hd.smooth_interface,
            Entry::SharpSubtitles => hd.sharp_subtitles = !hd.sharp_subtitles,
            Entry::Widescreen => hd.widescreen = !hd.widescreen,
            Entry::VideoSides => hd.video_sides = hd.video_sides.next(),
            Entry::BugFixes => x.minor_bug_fixes = !x.minor_bug_fixes,
            Entry::RestoredContent => x.restored_content = !x.restored_content,
            Entry::UiUx => x.ui_ux = !x.ui_ux,
            Entry::Close => {
                self.close();
                return MenuAction::Close;
            }
            Entry::Library => {
                self.close();
                return MenuAction::OpenLibrary;
            }
            Entry::Support => return MenuAction::OpenSupport,
        }
        if entry.hd_only() {
            MenuAction::SetHd(hd)
        } else {
            MenuAction::SetEnhancements(x)
        }
    }

    /// One line on what the selected entry does.
    fn description(entry: Entry, v: MenuValues) -> &'static str {
        if !v.enabled(entry) {
            return "An option of the HD graphics";
        }
        match entry {
            Entry::Graphics => "HD: the panorama and the faces at the window's resolution",
            Entry::PanoramaFilter => "The panorama's sampling: Bicubic is sharper",
            Entry::Sharpen => "A mild sharpening of the panorama, without halos",
            Entry::Interface => "Menus, bars and videos: Smooth (xBR) or Pixels",
            Entry::SharpSubtitles => "Subtitles drawn at the window's resolution",
            Entry::Widescreen => "The panorama fills the sides of a wide window",
            Entry::VideoSides => "Widescreen: the bars beside the videos",
            Entry::BugFixes => "Fixes bugs of the original release (on by default)",
            Entry::RestoredContent => "Restores lines the original never plays (off by default)",
            Entry::UiUx => "Modern comforts, e.g. no error box (off by default)",
            Entry::Close => "Back to the game",
            Entry::Library => "Stop this game and choose another edition",
            Entry::Support => "Enjoying it? Support the project (opens the browser)",
        }
    }

    /// The rows the box shows for the values `v`.
    fn rows(&self, v: MenuValues) -> Vec<Row<'static>> {
        let on = |b: bool| if b { "On" } else { "Off" };
        self.entries()
            .iter()
            .map(|&e| {
                let (label, value) = match e {
                    Entry::Graphics => (
                        "Graphics",
                        Some(match v.graphics {
                            Graphics::Original => "Original",
                            Graphics::Hd => "HD",
                        }),
                    ),
                    Entry::PanoramaFilter => (
                        "Panorama filter",
                        Some(match v.hd.panorama {
                            PanoFilter::Bilinear => "Bilinear",
                            PanoFilter::Bicubic => "Bicubic",
                        }),
                    ),
                    Entry::Sharpen => ("Panorama sharpening", Some(on(v.hd.sharpen))),
                    Entry::Interface => (
                        "Interface",
                        Some(if v.hd.smooth_interface {
                            "Smooth"
                        } else {
                            "Pixels"
                        }),
                    ),
                    Entry::SharpSubtitles => ("Sharp subtitles", Some(on(v.hd.sharp_subtitles))),
                    Entry::Widescreen => ("Widescreen", Some(on(v.hd.widescreen))),
                    Entry::VideoSides => (
                        "Video sides",
                        Some(match v.hd.video_sides {
                            VideoSides::Black => "Black",
                            VideoSides::Blur => "Blurred",
                            VideoSides::Zoom => "Zoomed",
                            VideoSides::Panorama => "Panorama",
                        }),
                    ),
                    Entry::BugFixes => (
                        "Fix original bugs",
                        Some(on(v.enhancements.minor_bug_fixes)),
                    ),
                    Entry::RestoredContent => (
                        "Restored content",
                        Some(on(v.enhancements.restored_content)),
                    ),
                    Entry::UiUx => ("Modern UI/UX", Some(on(v.enhancements.ui_ux))),
                    Entry::Close => ("Close", None),
                    Entry::Library => ("Game library...", None),
                    Entry::Support => ("Buy me a coffee...", None),
                };
                Row {
                    label,
                    value,
                    enabled: v.enabled(e),
                }
            })
            .collect()
    }

    /// What the box's input `input` does.
    fn act(&mut self, input: BoxInput, v: MenuValues) -> MenuAction {
        match input {
            BoxInput::None => MenuAction::None,
            BoxInput::Close => MenuAction::Close,
            BoxInput::Activate(i) => self.activate(self.entries()[i], v),
        }
    }

    /// A key pressed while the menu is open: Up / Down choose an entry
    /// (skipping the greyed ones), Enter / Space (and Left / Right on a
    /// setting) change it, Escape, F10 and O close the menu.
    pub fn key(&mut self, key: Key, v: MenuValues) -> MenuAction {
        let input = self.menu.key(key, &self.rows(v));
        self.act(input, v)
    }

    /// The pointer moved to a 640x480 screen point: the entry under it is
    /// selected.
    pub fn pointer(&mut self, x: i32, y: i32) {
        let n = self.entries().len();
        self.menu.pointer(n, x, y, SCREEN_W, SCREEN_H);
    }

    /// A click at a 640x480 screen point: the entry under it changes;
    /// outside the box the menu closes.
    pub fn click(&mut self, x: i32, y: i32, v: MenuValues) -> MenuAction {
        let n = self.entries().len();
        let input = self.menu.click(n, x, y, SCREEN_W, SCREEN_H);
        self.act(input, v)
    }

    /// Draws the menu over a 640x480 RGB565 frame.
    pub fn draw(&self, screen: &mut [u16], font: &SubtitleFont, v: MenuValues) {
        self.draw_recording(screen, font, v, None);
    }

    /// [`OptionsMenu::draw`], recording the text in `text` for the HD
    /// mode to draw sharp (see [`HdText`]).
    pub fn draw_recording(
        &self,
        screen: &mut [u16],
        font: &SubtitleFont,
        v: MenuValues,
        text: Option<&mut HdText>,
    ) {
        let d = Self::description(self.selected(), v);
        self.menu.draw_recording(
            screen,
            SCREEN_W,
            SCREEN_H,
            font,
            &self.rows(v),
            &self.info,
            d,
            text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu without the game library's entry (the web build).
    const BOX_H: i32 = TITLE_H + 12 * ROW_H + FOOTER_H;
    const BOX_Y: i32 = (SCREEN_H as i32 - BOX_H) / 2;

    const DEFAULTS: Enhancements = Enhancements {
        minor_bug_fixes: true,
        restored_content: false,
        ui_ux: false,
    };
    const HD_OPTS: HdOptions = HdOptions {
        panorama: PanoFilter::Bicubic,
        sharpen: true,
        smooth_interface: true,
        sharp_subtitles: true,
        widescreen: false,
        video_sides: VideoSides::Black,
    };
    const ORIG: MenuValues = MenuValues {
        graphics: Graphics::Original,
        hd: HD_OPTS,
        enhancements: DEFAULTS,
    };
    const HD: MenuValues = MenuValues {
        graphics: Graphics::Hd,
        hd: HD_OPTS,
        enhancements: DEFAULTS,
    };

    #[test]
    fn keys_toggle_and_close() {
        let mut m = OptionsMenu::new();
        m.open();
        assert_eq!(m.selected(), Entry::Graphics);
        assert_eq!(
            m.key(Key::Enter, ORIG),
            MenuAction::SetGraphics(Graphics::Hd)
        );
        assert_eq!(
            m.key(Key::Right, HD),
            MenuAction::SetGraphics(Graphics::Original)
        );
        for _ in 0..5 {
            assert_eq!(m.key(Key::Down, ORIG), MenuAction::None);
        }
        assert_eq!(m.selected(), Entry::Close);
        assert_eq!(m.key(Key::Left, ORIG), MenuAction::None);
        assert!(m.is_open());
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::Close);
        assert!(!m.is_open());
        m.open();
        assert_eq!(m.key(Key::Up, ORIG), MenuAction::None);
        assert_eq!(m.selected(), Entry::Close);
        assert_eq!(m.key(Key::F(10), ORIG), MenuAction::Close);
        m.open();
        assert_eq!(m.key(Key::Char('o'), ORIG), MenuAction::Close);
        assert!(!m.is_open());
        assert!(!m.is_open());
    }

    #[test]
    fn clicks_hit_the_entries() {
        let mut m = OptionsMenu::new();
        m.open();
        let gy = BOX_Y + TITLE_H + 5;
        assert_eq!(
            m.click(320, gy, ORIG),
            MenuAction::SetGraphics(Graphics::Hd)
        );
        m.pointer(320, gy + 10 * ROW_H);
        assert_eq!(m.selected(), Entry::Support);
        m.pointer(320, gy + 11 * ROW_H);
        assert_eq!(m.selected(), Entry::Close);
        assert_eq!(m.click(320, BOX_Y + 2, ORIG), MenuAction::None);
        assert!(m.is_open());
        assert_eq!(m.click(5, 5, ORIG), MenuAction::Close);
        assert!(!m.is_open());
    }

    #[test]
    fn enhancement_rows_switch_their_group() {
        let mut m = OptionsMenu::new();
        m.open();
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::BugFixes);
        let MenuAction::SetEnhancements(x) = m.key(Key::Enter, ORIG) else {
            panic!("no change");
        };
        assert_eq!(
            x,
            Enhancements {
                minor_bug_fixes: false,
                ..DEFAULTS
            }
        );
        m.key(Key::Down, ORIG);
        assert_eq!(
            m.key(Key::Right, ORIG),
            MenuAction::SetEnhancements(Enhancements {
                restored_content: true,
                ..DEFAULTS
            })
        );
        m.key(Key::Down, ORIG);
        assert_eq!(
            m.key(Key::Space, ORIG),
            MenuAction::SetEnhancements(Enhancements {
                ui_ux: true,
                ..DEFAULTS
            })
        );
    }

    #[test]
    fn hd_rows_change_the_hd_options_in_hd_only() {
        let mut m = OptionsMenu::new();
        m.open();
        // HD: Down visits the four HD rows in turn.
        let want = [
            HdOptions {
                panorama: PanoFilter::Bilinear,
                ..HD_OPTS
            },
            HdOptions {
                sharpen: false,
                ..HD_OPTS
            },
            HdOptions {
                smooth_interface: false,
                ..HD_OPTS
            },
            HdOptions {
                sharp_subtitles: false,
                widescreen: false,
                video_sides: VideoSides::Black,
                ..HD_OPTS
            },
        ];
        for w in want {
            m.key(Key::Down, HD);
            assert!(m.selected().hd_only());
            assert_eq!(m.key(Key::Enter, HD), MenuAction::SetHd(w));
        }
        let back = HdOptions {
            panorama: PanoFilter::Bilinear,
            ..HD_OPTS
        };
        let v = MenuValues { hd: back, ..HD };
        m.open();
        m.key(Key::Down, v);
        assert_eq!(m.key(Key::Left, v), MenuAction::SetHd(HD_OPTS));
        // Original: greyed, skipped by the keys, fixed under a click.
        m.open();
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::BugFixes);
        m.key(Key::Up, ORIG);
        assert_eq!(m.selected(), Entry::Graphics);
        for row in 1..5 {
            let y = BOX_Y + TITLE_H + row * ROW_H + 5;
            assert_eq!(m.click(320, y, ORIG), MenuAction::None);
            assert!(m.selected().hd_only());
            assert_eq!(m.key(Key::Enter, ORIG), MenuAction::None);
        }
        assert_eq!(
            OptionsMenu::description(Entry::Sharpen, ORIG),
            "An option of the HD graphics"
        );
    }

    #[test]
    fn draw_records_its_text_for_hd() {
        let font = SubtitleFont::new();
        let mut m = OptionsMenu::new();
        m.open();
        let mut s = vec![0x1234u16; SCREEN_W * SCREEN_H];
        let mut t = HdText::default();
        m.draw_recording(&mut s, &font, ORIG, Some(&mut t));
        let mut plain = vec![0x1234u16; SCREEN_W * SCREEN_H];
        m.draw(&mut plain, &font, ORIG);
        assert!(s == plain, "same pixels with or without recording");
        assert!(t.after == s);
        assert_eq!(t.items.len(), 1 + 10 * 2 + 2 + 2);
        assert!(t.items.iter().any(|i| i.color == GREYED));
        assert!(t.items.iter().any(|i| i.text == b"< Bicubic >"));
        // Without the text, the menu's boxes.
        let mut u = s.clone();
        t.uncover(&mut u);
        assert!(u == t.under);
        assert!(!u.contains(&TEXT) && u.contains(&BG));
    }

    #[test]
    fn info_lines_fit_on_the_screen() {
        assert!(BOX_Y >= 0 && BOX_Y + BOX_H <= SCREEN_H as i32);
        let font = SubtitleFont::new();
        let mut s = vec![0x1234u16; SCREEN_W * SCREEN_H];
        let mut m = OptionsMenu::new();
        m.set_info(vec![
            "Game: GOG.com, French (1999 build)".to_string(),
            "Atlantis Native 0.1.0 (abcdef0)".to_string(),
        ]);
        m.open();
        m.draw(&mut s, &font, HD);
        // The text stays inside the box.
        for y in 0..SCREEN_H as i32 {
            for x in 0..SCREEN_W as i32 {
                let inside = (BOX_X - 1..BOX_X + BOX_W + 1).contains(&x)
                    && (BOX_Y - 1..BOX_Y + BOX_H + 1).contains(&y);
                if !inside {
                    assert_eq!(s[y as usize * SCREEN_W + x as usize], 0x1234, "({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn draws_inside_its_box() {
        let font = SubtitleFont::new();
        let mut s = vec![0x1234u16; SCREEN_W * SCREEN_H];
        let mut m = OptionsMenu::new();
        m.open();
        m.draw(&mut s, &font, HD);
        assert_eq!(s[0], 0x1234);
        assert_eq!(s[(BOX_Y as usize + 2) * SCREEN_W + BOX_X as usize + 2], BG);
        assert!(s.contains(&SELECTED));
    }

    #[test]
    fn the_library_entry_stops_the_game() {
        let mut m = OptionsMenu::new();
        m.open();
        // Without a game library (the web build): no entry.
        m.key(Key::Up, ORIG);
        assert_eq!(m.selected(), Entry::Close);
        m.set_library(true);
        m.open();
        m.key(Key::Up, ORIG);
        assert_eq!(m.selected(), Entry::Library);
        assert_eq!(m.key(Key::Left, ORIG), MenuAction::None);
        assert!(m.is_open());
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::OpenLibrary);
        assert!(!m.is_open());
        // The last row, under Close, by the pointer.
        m.open();
        assert!(m.box_y() >= 0 && m.box_y() + m.box_h() <= SCREEN_H as i32);
        let y = m.box_y() + TITLE_H + 12 * ROW_H + 5;
        assert_eq!(m.click(320, y, HD), MenuAction::OpenLibrary);
        let font = SubtitleFont::new();
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        let mut t = HdText::default();
        m.open();
        m.draw_recording(&mut s, &font, HD, Some(&mut t));
        assert!(t.items.iter().any(|i| i.text == b"Game library..."));
    }

    #[test]
    fn the_support_entry_opens_the_page() {
        let mut m = OptionsMenu::new();
        m.open();
        m.key(Key::Up, ORIG);
        m.key(Key::Up, ORIG);
        assert_eq!(m.selected(), Entry::Support);
        assert_eq!(m.key(Key::Left, ORIG), MenuAction::None);
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::OpenSupport);
        assert!(m.is_open(), "the menu stays open");
    }
}
