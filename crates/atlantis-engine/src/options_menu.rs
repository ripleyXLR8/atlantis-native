//! The port's options menu (F10), drawn by the front end over whatever
//! frame the game shows: settings of the port that the original's menus do
//! not have. The game is paused while it is open.
//!
//! Entries: the graphics mode (see [`crate::hd`]), the three groups of
//! [`Enhancements`] (the names of the ScummVM port's options): the bug
//! fixes all on or all off ("Custom (n/N)" when mixed) and "Choose the bug
//! fixes..." (a box of its own in the same style: all on, all off, one
//! switch per [`BugFix`] of the game's build with what the original does,
//! Back), Close, and on the desktop "Game library..." (back to the list of
//! the player's game folders, to play another edition).

use crate::enhancements::{BugFix, BugFixes, Enhancements};
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
    /// Open the bug fixes' box.
    ChooseFixes,
    RestoredContent,
    UiUx,
    /// Open the project's Buy Me a Coffee page in the browser.
    Support,
    Close,
    /// Stop this game and choose another in the game library.
    Library,
}

const ENTRIES: [Entry; 14] = [
    Entry::Graphics,
    Entry::PanoramaFilter,
    Entry::Sharpen,
    Entry::Interface,
    Entry::SharpSubtitles,
    Entry::Widescreen,
    Entry::VideoSides,
    Entry::BugFixes,
    Entry::ChooseFixes,
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

/// The layout of an options box: its title, its width, its rows' height,
/// the gaps between groups of rows and the height of its foot.
/// [`BoxLayout::OPTIONS`] is the options menu's (both games').
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxLayout<'a> {
    pub title: &'a str,
    pub width: i32,
    /// A row's height ([`ROW_H`] but in a box of many rows).
    pub row_h: i32,
    /// The rows that start a group: a [`GAP_H`]-pixel gap above each.
    pub gaps: &'a [usize],
    pub footer_h: i32,
}

impl BoxLayout<'_> {
    /// The options menu's box.
    pub const OPTIONS: BoxLayout<'static> = BoxLayout {
        title: "Atlantis Native options",
        width: BOX_W,
        row_h: ROW_H,
        gaps: &[],
        footer_h: FOOTER_H,
    };

    /// The height of a box of `n` rows.
    pub fn box_h(&self, n: usize) -> i32 {
        let gaps = self.gaps.iter().filter(|&&g| g < n).count() as i32;
        TITLE_H + n as i32 * self.row_h + gaps * GAP_H + self.footer_h
    }

    /// The top left corner of a box of `n` rows on a `w` x `h` screen
    /// (centred).
    pub fn origin(&self, n: usize, w: usize, h: usize) -> (i32, i32) {
        ((w as i32 - self.width) / 2, (h as i32 - self.box_h(n)) / 2)
    }

    /// The top of row `i`, from the box's top.
    pub fn row_y(&self, i: usize) -> i32 {
        let gaps = self.gaps.iter().filter(|&&g| g <= i).count() as i32;
        TITLE_H + i as i32 * self.row_h + gaps * GAP_H
    }

    /// The row under a point of a `w` x `h` screen, for `n` rows.
    fn row_at(&self, n: usize, x: i32, y: i32, w: usize, h: usize) -> Option<usize> {
        let (bx, by) = self.origin(n, w, h);
        if !(bx..bx + self.width).contains(&x) {
            return None;
        }
        (0..n).find(|&i| (0..self.row_h).contains(&(y - by - self.row_y(i))))
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
        fill(bx + 6, sel_y, bw - 12, layout.row_h - 2, HIGHLIGHT);
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
        // The text's top in its row: 4 pixels down in a 24-pixel row.
        let pad = (layout.row_h - 16) / 2;
        for (i, row) in rows.iter().enumerate() {
            let y = by + layout.row_y(i) + pad;
            let c = match (row.enabled, i == self.selected) {
                (false, _) => GREYED,
                (true, true) => SELECTED,
                (true, false) => TEXT,
            };
            match row.value {
                Some(value) => {
                    text(row.label, bx + 16, y, c);
                    let t = format!("< {value} >");
                    text(&t, right(&t), y, c);
                }
                None => text(row.label, centre(row.label), y, c),
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

/// A row of the bug fixes' box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixRow {
    AllOn,
    AllOff,
    Fix(BugFix),
    Back,
}

impl FixRow {
    fn explanation(self) -> &'static [&'static str] {
        match self {
            FixRow::AllOn => &["Every fix on (the default)"],
            FixRow::AllOff => &["Every fix off: the original, bug for bug"],
            FixRow::Fix(f) => f.explanation(),
            FixRow::Back => &["Back to the options"],
        }
    }
}

/// The lines of a fix's explanation at the foot of its box (at most).
pub const FIX_TEXT_LINES: usize = 3;

/// The bug fixes' box's width: wider than the options' (the fixes' names
/// and the game's in its title).
pub const FIX_BOX_W: i32 = 580;

/// Its foot: the explanation, then the keys.
const FIX_FOOTER_H: i32 = 12 + FIX_TEXT_LINES as i32 * 18 + 32;

/// The space kept around the bug fixes' box on the 640x480 screen; its
/// rows are 20 pixels high when 24 would not leave it.
const FIX_MARGIN: i32 = 8;

/// The keys' line of the bug fixes' box.
pub const FIX_KEYS: &str = "Arrows, Enter or click; Escape goes back";

impl BugFix {
    /// Its row's name in the bug fixes' box.
    pub fn label(self) -> &'static str {
        match self {
            BugFix::MusicLoop => "Music: seamless loops",
            BugFix::HeadOverlay => "Faces: the guards' coif, the pilot's scarf",
            BugFix::HeadFirstFrame => "Faces: the lower face's first frame",
            BugFix::BarSlideOut => "Inventory: icons left as the bar hides",
            BugFix::PickupIcon => "Inventory: a new object's icon",
            BugFix::MenuRecords => "Menus: each its own click areas",
            BugFix::FifthPlayer => "Players: OK on the fifth player",
            BugFix::SnakesFirstWin => "Snakes puzzle: the false first win",
            BugFix::HexagonPieces => "Hexagon puzzle: its pieces always there",
            BugFix::LoadMusik => "Music: tracks copied one by one",
            BugFix::SharpS => "German: the sharp s in the LOAD list",
            BugFix::DiscRevision => "Dutch, German May: the fonts of disc 1",
            BugFix::MenuLineWidth => "Italian: the whole of OK and Annulla",
            BugFix::ChineseRows => "Chinese: the subtitle rows",
            BugFix::SubtitlesToggle => "Chinese: the SUBTITLES button",
            BugFix::DosLetters => "DVD: the subtitles' accented letters",
            BugFix::StartupLogos => "Japanese: the two missing logo films",
        }
    }

    /// What the original does without it, and where a player notices it:
    /// the lines at the foot of the bug fixes' box.
    pub fn explanation(self) -> &'static [&'static str] {
        match self {
            BugFix::MusicLoop => &[
                "The original streams the music through a ring that",
                "its refills do not always keep ahead of: stale",
                "fragments of sound are heard in the music.",
            ],
            BugFix::HeadOverlay => &[
                "The original draws the overlay of some talking heads",
                "where the next copy of the picture erases it: the",
                "guards' coif and the pilot's scarf are never seen.",
            ],
            BugFix::HeadFirstFrame => &[
                "On the first frame of each loop of some talking heads",
                "(Meljanz, Actyon, Servage, the Queen, Gimbas), the",
                "original draws the 3D lower face away from the face.",
            ],
            BugFix::BarSlideOut => &[
                "When the inventory bar is scrolled, the original",
                "leaves rows of icons at the bottom of the screen",
                "as the bar slides away.",
            ],
            BugFix::PickupIcon => &[
                "After a pick-up, the original draws the new object's",
                "icon 7 pixels left of its slot until the bar next",
                "slides in.",
            ],
            BugFix::MenuRecords => &[
                "The original keeps one table of the menus' click",
                "areas for the session: the LOAD list's knob, once",
                "hidden, stays hidden (and in May 1997, an ornament).",
            ],
            BugFix::FifthPlayer => &[
                "With five players, OK on the fifth empties its name",
                "in the original: that player stays listed with no",
                "name and cannot be chosen.",
            ],
            BugFix::SnakesFirstWin => &[
                "On a first visit to the four snakes puzzle, the",
                "original takes the first piece carried to the",
                "screen's top left corner for the solution.",
            ],
            BugFix::HexagonPieces => &[
                "This build places the hexagon puzzle's pieces only",
                "when the player comes holding the right object:",
                "the later builds always place them.",
            ],
            BugFix::LoadMusik => &[
                "This build copies a music list to the disk all or",
                "nothing, on its first track, where the other builds",
                "copy each missing track.",
            ],
            BugFix::SharpS => &[
                "No sprite font of the game has the sharp s: the",
                "original leaves a gap in the LOAD list's chapter",
                "names. Fixed, it is drawn as ss.",
            ],
            BugFix::DiscRevision => &[
                "The original reads the credits and the name fonts",
                "from the disc in the drive: the Dutch disc 2 and",
                "the German May discs 2-4 hold other revisions.",
            ],
            BugFix::MenuLineWidth => &[
                "This build gives a menu line at a fixed place 16",
                "pixels per letter, not 22: the right ends of OK and",
                "Annulla do not highlight or take a click.",
            ],
            BugFix::ChineseRows => &[
                "The original may start a subtitle row in the middle",
                "of a character (after an ideographic space or a",
                "mark): that row shows the wrong characters.",
            ],
            BugFix::SubtitlesToggle => &[
                "With SUBTITLES off, the original reads its button's",
                "record one byte out of step: its click area takes",
                "the wrong width and height.",
            ],
            BugFix::DosLetters => &[
                "The DVD's Dutch and Swedish subtitles are the CDs'",
                "texts, but its program reads them as Windows text:",
                "punctuation signs show in place of ä, å, ö and é.",
            ],
            BugFix::StartupLogos => &[
                "The Japanese program starts with two logo films the",
                "DVD does not hold: the original shows two \"file not",
                "found\" boxes before the game. Fixed, none.",
            ],
        }
    }
}

/// The menu's state.
#[derive(Debug)]
pub struct OptionsMenu {
    menu: MenuBox,
    /// The bug fixes' box, over the menu while it is open.
    fixes: MenuBox,
    /// Shown at the foot of the menu: the game's release, the port's
    /// version (set by the front end).
    info: Vec<String>,
    /// The front end has a game library: the "Game library..." entry.
    library: bool,
    /// The fixes the bug fixes' box lists: those of the game's build
    /// ([`BugFix::of_game`]), in the menu's order.
    game_fixes: Vec<BugFix>,
    /// The bug fixes' box's title, the game named.
    fix_title: String,
    /// The bug fixes' box's gaps: above the fixes and above Back.
    fix_gaps: [usize; 2],
}

impl Default for OptionsMenu {
    fn default() -> Self {
        let mut m = OptionsMenu {
            menu: MenuBox::default(),
            fixes: MenuBox::default(),
            info: Vec::new(),
            library: false,
            game_fixes: Vec::new(),
            fix_title: String::new(),
            fix_gaps: [0; 2],
        };
        m.set_game(None, None);
        m
    }
}

impl OptionsMenu {
    pub fn new() -> OptionsMenu {
        OptionsMenu::default()
    }

    /// The lines shown at the foot of the menu (at most two).
    pub fn set_info(&mut self, lines: Vec<String>) {
        self.info = lines;
    }

    /// The game: its release (`edition`, named in the bug fixes' box) and
    /// the fixes of its build ([`BugFix::of_game`]); without a game, the
    /// fixes of every build.
    pub fn set_game(&mut self, edition: Option<&str>, fixes: Option<Vec<BugFix>>) {
        self.game_fixes = fixes.unwrap_or_else(|| {
            BugFix::ALL
                .into_iter()
                .filter(|f| f.of_every_build())
                .collect()
        });
        self.fix_title = match edition {
            Some(e) => format!("Bug fixes: {e}"),
            None => "Original bug fixes".to_string(),
        };
        self.fix_gaps = [2, 2 + self.game_fixes.len()];
        self.fixes.close();
    }

    /// The fixes the bug fixes' box lists.
    pub fn game_fixes(&self) -> &[BugFix] {
        &self.game_fixes
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
        self.fixes.close();
    }

    pub fn close(&mut self) {
        self.menu.close();
        self.fixes.close();
    }

    /// The selected entry.
    pub fn selected(&self) -> Entry {
        self.entries()[self.menu.selected()]
    }

    /// The bug fixes' box is open.
    pub fn in_fixes(&self) -> bool {
        self.menu.is_open() && self.fixes.is_open()
    }

    /// The bug fixes' box's rows.
    pub fn fix_rows(&self) -> Vec<FixRow> {
        let mut rows = vec![FixRow::AllOn, FixRow::AllOff];
        rows.extend(self.game_fixes.iter().map(|&f| FixRow::Fix(f)));
        rows.push(FixRow::Back);
        rows
    }

    /// The bug fixes' box's selected row.
    pub fn fix_selected(&self) -> FixRow {
        self.fix_rows()[self.fixes.selected()]
    }

    /// The bug fixes' box's layout: rows of 24 pixels, or of 20 when that
    /// would not leave [`FIX_MARGIN`] around it on the screen.
    pub fn fix_layout(&self) -> BoxLayout<'_> {
        let mut l = BoxLayout {
            title: &self.fix_title,
            width: FIX_BOX_W,
            row_h: ROW_H,
            gaps: &self.fix_gaps,
            footer_h: FIX_FOOTER_H,
        };
        if l.box_h(self.fix_rows().len()) + 2 * FIX_MARGIN > SCREEN_H as i32 {
            l.row_h = 20;
        }
        l
    }

    /// The "Fix original bugs" row's value: On, Off, or how many of the
    /// game's fixes are on.
    fn fixes_value(&self, x: &BugFixes) -> String {
        let n = self.game_fixes.len();
        match x.count_of(&self.game_fixes) {
            0 => "Off".into(),
            k if k == n => "On".into(),
            k => format!("Custom ({k}/{n})"),
        }
    }

    /// The game's fixes all `on` or all off, the others (which change
    /// nothing in this game) kept: an edition whose build another edition
    /// shares stores its choices with it (the French and German GOG
    /// editions), where they may be listed.
    fn set_game_fixes(&self, x: &mut BugFixes, on: bool) {
        for &f in &self.game_fixes {
            x.set(f, on);
        }
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
            // All on, then all off (from a mix of the game's: all on).
            Entry::BugFixes => {
                let all_on = x.fixes.count_of(&self.game_fixes) == self.game_fixes.len();
                self.set_game_fixes(&mut x.fixes, !all_on);
            }
            Entry::ChooseFixes => {
                self.fixes.open();
                return MenuAction::None;
            }
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

    /// A row of the bug fixes' box activated.
    fn activate_fix(&mut self, row: FixRow, v: MenuValues) -> MenuAction {
        let mut x = v.enhancements;
        match row {
            FixRow::AllOn => self.set_game_fixes(&mut x.fixes, true),
            FixRow::AllOff => self.set_game_fixes(&mut x.fixes, false),
            FixRow::Fix(f) => x.fixes.set(f, !x.fixes.get(f)),
            FixRow::Back => {
                self.fixes.close();
                return MenuAction::None;
            }
        }
        MenuAction::SetEnhancements(x)
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
            Entry::BugFixes => "Every fix of the original's bugs on or off",
            Entry::ChooseFixes => "Enter: one fix at a time",
            Entry::RestoredContent => "Restores lines the original never plays (off by default)",
            Entry::UiUx => "Modern comforts, e.g. no error box (off by default)",
            Entry::Close => "Back to the game",
            Entry::Library => "Stop this game and choose another edition",
            Entry::Support => "Enjoying it? Support the project (opens the browser)",
        }
    }

    /// The rows the box shows for the values `v` (`fixes`: the bug fixes'
    /// value, [`OptionsMenu::fixes_value`]).
    fn rows<'a>(&self, v: MenuValues, fixes: &'a str) -> Vec<Row<'a>> {
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
                    Entry::BugFixes => ("Fix original bugs", Some(fixes)),
                    Entry::ChooseFixes => ("Choose the bug fixes...", None),
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

    /// The bug fixes' box's rows for the values `v`.
    fn fix_box_rows(&self, v: MenuValues) -> Vec<Row<'static>> {
        let x = v.enhancements.fixes;
        self.fix_rows()
            .into_iter()
            .map(|r| {
                let (label, value) = match r {
                    FixRow::AllOn => ("All on", None),
                    FixRow::AllOff => ("All off (as the original)", None),
                    FixRow::Fix(f) => (f.label(), Some(if x.get(f) { "On" } else { "Off" })),
                    FixRow::Back => ("Back", None),
                };
                Row {
                    label,
                    value,
                    enabled: true,
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

    /// What the bug fixes' box's input `input` does: Escape and a click
    /// outside go back to the menu.
    fn act_fix(&mut self, input: BoxInput, v: MenuValues) -> MenuAction {
        match input {
            BoxInput::None | BoxInput::Close => MenuAction::None,
            BoxInput::Activate(i) => self.activate_fix(self.fix_rows()[i], v),
        }
    }

    /// A key pressed while the menu is open: Up / Down choose an entry
    /// (skipping the greyed ones), Enter / Space (and Left / Right on a
    /// setting) change it, Escape, F10 and O close the menu. In the bug
    /// fixes' box the same, but Escape goes back to the menu.
    pub fn key(&mut self, key: Key, v: MenuValues) -> MenuAction {
        if self.fixes.is_open() {
            if matches!(key, Key::F(10) | Key::Char('o' | 'O')) {
                self.close();
                return MenuAction::Close;
            }
            let rows = self.fix_box_rows(v);
            let input = self.fixes.key(key, &rows);
            return self.act_fix(input, v);
        }
        let value = self.fixes_value(&v.enhancements.fixes);
        let input = self.menu.key(key, &self.rows(v, &value));
        self.act(input, v)
    }

    /// The pointer moved to a 640x480 screen point: the entry under it is
    /// selected.
    pub fn pointer(&mut self, x: i32, y: i32) {
        if self.fixes.is_open() {
            let n = self.fix_rows().len();
            let layout = self.fix_layout();
            let mut b = self.fixes.clone();
            b.pointer_in(&layout, n, x, y, SCREEN_W, SCREEN_H);
            self.fixes = b;
            return;
        }
        let n = self.entries().len();
        self.menu.pointer(n, x, y, SCREEN_W, SCREEN_H);
    }

    /// A click at a 640x480 screen point: the entry under it changes;
    /// outside the box the menu closes (the bug fixes' box goes back to
    /// the menu).
    pub fn click(&mut self, x: i32, y: i32, v: MenuValues) -> MenuAction {
        if self.fixes.is_open() {
            let n = self.fix_rows().len();
            let layout = self.fix_layout();
            let mut b = self.fixes.clone();
            let input = b.click_in(&layout, n, x, y, SCREEN_W, SCREEN_H);
            self.fixes = b;
            return self.act_fix(input, v);
        }
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
        if self.fixes.is_open() {
            let layout = self.fix_layout();
            let lines = self.fix_selected().explanation();
            let mut foot: Vec<FootLine> = lines
                .iter()
                .take(FIX_TEXT_LINES)
                .enumerate()
                .map(|(i, t)| FootLine {
                    text: t,
                    from_bottom: layout.footer_h - 12 - i as i32 * 18,
                    color: TEXT,
                })
                .collect();
            foot.push(FootLine {
                text: FIX_KEYS,
                from_bottom: 22,
                color: DIM,
            });
            self.fixes.draw_in(
                &layout,
                screen,
                SCREEN_W,
                SCREEN_H,
                font,
                &self.fix_box_rows(v),
                &foot,
                text,
            );
            return;
        }
        let d = Self::description(self.selected(), v);
        let value = self.fixes_value(&v.enhancements.fixes);
        self.menu.draw_recording(
            screen,
            SCREEN_W,
            SCREEN_H,
            font,
            &self.rows(v, &value),
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
    const BOX_H: i32 = TITLE_H + 13 * ROW_H + FOOTER_H;
    const BOX_Y: i32 = (SCREEN_H as i32 - BOX_H) / 2;

    const DEFAULTS: Enhancements = Enhancements {
        fixes: BugFixes::ALL,
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
        for _ in 0..6 {
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
        m.pointer(320, gy + 11 * ROW_H);
        assert_eq!(m.selected(), Entry::Support);
        m.pointer(320, gy + 12 * ROW_H);
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
        // Without a game, the fixes of every build listed: those off, the
        // others (which no game here can show) kept.
        assert_eq!(
            x,
            Enhancements {
                fixes: game_fixes_set(&m, BugFixes::ALL, false),
                ..DEFAULTS
            }
        );
        assert_eq!(x.fixes.count(), BugFix::ALL.len() - 8);
        // "Choose the bug fixes...": a button, its box.
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::ChooseFixes);
        assert_eq!(m.key(Key::Right, ORIG), MenuAction::None);
        assert!(!m.in_fixes());
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
        // The title, 10 rows of a label and a value, three buttons, the
        // description and the keys.
        assert_eq!(t.items.len(), 1 + 10 * 2 + 3 + 2);
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
        let y = m.box_y() + TITLE_H + 13 * ROW_H + 5;
        assert_eq!(m.click(320, y, HD), MenuAction::OpenLibrary);
        let font = SubtitleFont::new();
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        let mut t = HdText::default();
        m.open();
        m.draw_recording(&mut s, &font, HD, Some(&mut t));
        assert!(t.items.iter().any(|i| i.text == b"Game library..."));
    }

    fn with_fixes(f: BugFixes) -> MenuValues {
        MenuValues {
            enhancements: Enhancements {
                fixes: f,
                ..DEFAULTS
            },
            ..ORIG
        }
    }

    /// The menu of the French May 1997 build: the fixes of every build,
    /// the hexagon pieces' and LOADMUSIK's.
    fn may_1997() -> OptionsMenu {
        let mut m = OptionsMenu::new();
        let mut fixes: Vec<BugFix> = BugFix::ALL
            .into_iter()
            .filter(|f| f.of_every_build())
            .collect();
        fixes.extend([BugFix::HexagonPieces, BugFix::LoadMusik]);
        m.set_game(Some("CD, French (May 1997 build)"), Some(fixes));
        m
    }

    /// `from` with the fixes `m` lists all `on` or all off.
    fn game_fixes_set(m: &OptionsMenu, from: BugFixes, on: bool) -> BugFixes {
        let mut x = from;
        for &f in m.game_fixes() {
            x.set(f, on);
        }
        x
    }

    fn set(a: MenuAction) -> BugFixes {
        match a {
            MenuAction::SetEnhancements(x) => x.fixes,
            a => panic!("{a:?}"),
        }
    }

    /// "Fix original bugs": On, Off or how many of the game's fixes are
    /// on; pressed, all on, then all off (from a mix: all on), the fixes
    /// of other builds left as they are.
    #[test]
    fn the_bug_fixes_row_shows_all_none_or_how_many() {
        let m = may_1997();
        let value = |f: BugFixes| m.fixes_value(&f);
        assert_eq!(value(BugFixes::ALL), "On");
        assert_eq!(value(BugFixes::NONE), "Off");
        assert_eq!(
            value(BugFixes::all_but(BugFix::SnakesFirstWin)),
            "Custom (9/10)"
        );
        assert_eq!(value(BugFixes::only(BugFix::LoadMusik)), "Custom (1/10)");
        // The fixes of other builds do not count.
        assert_eq!(value(BugFixes::all_but(BugFix::SharpS)), "On");
        assert_eq!(value(BugFixes::only(BugFix::SharpS)), "Off");
        let mut m = may_1997();
        m.open();
        let press = |m: &mut OptionsMenu, f: BugFixes| {
            m.pointer(320, BOX_Y + TITLE_H + 7 * ROW_H + 3);
            assert_eq!(m.selected(), Entry::BugFixes);
            set(m.key(Key::Enter, with_fixes(f)))
        };
        let off = game_fixes_set(&m, BugFixes::ALL, false);
        assert_eq!(press(&mut m, BugFixes::ALL), off);
        assert!(off.get(BugFix::SharpS) && !off.get(BugFix::LoadMusik));
        assert_eq!(off.count(), BugFix::ALL.len() - 10);
        let on = game_fixes_set(&m, BugFixes::NONE, true);
        assert_eq!(press(&mut m, BugFixes::NONE), on);
        assert_eq!(on.count(), 10);
        assert!(!on.get(BugFix::SharpS) && on.get(BugFix::LoadMusik));
        let mix = BugFixes::all_but(BugFix::HexagonPieces);
        assert_eq!(press(&mut m, mix), BugFixes::ALL);
        let mut want = off;
        want.set(BugFix::SharpS, false);
        assert_eq!(press(&mut m, BugFixes::all_but(BugFix::SharpS)), want);
        // Drawn in the row.
        let font = SubtitleFont::new();
        let mut s = vec![0u16; SCREEN_W * SCREEN_H];
        let mut t = HdText::default();
        m.draw_recording(&mut s, &font, with_fixes(mix), Some(&mut t));
        assert!(t.items.iter().any(|i| i.text == b"< Custom (9/10) >"));
    }

    /// The bug fixes' box by the keys: its rows (the game's fixes only),
    /// each fix alone toggled both ways, all on, all off, Back; Escape
    /// goes back, F10 and O close everything.
    #[test]
    fn the_fixes_box_by_the_keys() {
        let mut m = may_1997();
        m.open();
        m.key(Key::Down, ORIG);
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::ChooseFixes);
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::None);
        assert!(m.in_fixes() && m.is_open());
        assert_eq!(m.fix_selected(), FixRow::AllOn);
        // All on, all off: the game's fixes only.
        assert_eq!(
            set(m.key(Key::Enter, with_fixes(BugFixes::NONE))),
            game_fixes_set(&m, BugFixes::NONE, true)
        );
        m.key(Key::Down, ORIG);
        assert_eq!(m.fix_selected(), FixRow::AllOff);
        assert_eq!(m.key(Key::Left, ORIG), MenuAction::None, "a button");
        assert_eq!(
            set(m.key(Key::Space, ORIG)),
            game_fixes_set(&m, BugFixes::ALL, false)
        );
        let fixes = m.game_fixes().to_vec();
        assert_eq!(fixes.len(), 10);
        for &f in &fixes {
            m.key(Key::Down, ORIG);
            assert_eq!(m.fix_selected(), FixRow::Fix(f));
            assert_eq!(set(m.key(Key::Right, ORIG)), BugFixes::all_but(f));
            assert_eq!(set(m.key(Key::Left, ORIG)), BugFixes::all_but(f));
            assert_eq!(
                set(m.key(Key::Enter, with_fixes(BugFixes::NONE))),
                BugFixes::only(f)
            );
        }
        m.key(Key::Down, ORIG);
        assert_eq!(m.fix_selected(), FixRow::Back);
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::None);
        assert!(!m.in_fixes() && m.is_open());
        assert_eq!(m.selected(), Entry::ChooseFixes);
        // Up from the first row wraps to Back; Escape goes back.
        m.key(Key::Enter, ORIG);
        assert_eq!(m.fix_selected(), FixRow::AllOn, "opens on its first row");
        m.key(Key::Up, ORIG);
        assert_eq!(m.fix_selected(), FixRow::Back);
        assert_eq!(m.key(Key::Escape, ORIG), MenuAction::None);
        assert!(!m.in_fixes() && m.is_open());
        for k in [Key::F(10), Key::Char('o')] {
            m.key(Key::Enter, ORIG);
            assert!(m.in_fixes());
            assert_eq!(m.key(k, ORIG), MenuAction::Close);
            assert!(!m.is_open() && !m.in_fixes());
            m.open();
            assert!(!m.in_fixes(), "the menu opens on its first box");
            m.key(Key::Down, ORIG);
            m.key(Key::Down, ORIG);
        }
    }

    /// The bug fixes' box by the mouse: its rows found across the gaps, a
    /// click outside back to the menu.
    #[test]
    fn the_fixes_box_by_the_mouse() {
        let mut m = may_1997();
        m.open();
        let y = BOX_Y + TITLE_H + 8 * ROW_H + 5;
        assert_eq!(m.click(320, y, ORIG), MenuAction::None);
        assert!(m.in_fixes());
        let rows = m.fix_rows();
        let (fx, fy, row_y) = {
            let layout = m.fix_layout();
            assert_eq!(layout.row_h, ROW_H, "ten fixes: rows of 24 pixels");
            assert_eq!(layout.row_y(2), TITLE_H + 2 * ROW_H + GAP_H);
            let (fx, fy) = layout.origin(rows.len(), SCREEN_W, SCREEN_H);
            let row_y: Vec<i32> = (0..rows.len()).map(|i| layout.row_y(i)).collect();
            (fx, fy, row_y)
        };
        for (i, &row) in rows.iter().enumerate() {
            m.pointer(fx + 30, fy + row_y[i] + 3);
            assert_eq!(m.fix_selected(), row);
        }
        // The gap above the fixes is no row.
        m.pointer(fx + 30, fy + TITLE_H + 2 * ROW_H + 2);
        assert_eq!(m.fix_selected(), FixRow::Back);
        let i = rows
            .iter()
            .position(|&r| r == FixRow::Fix(BugFix::LoadMusik))
            .unwrap();
        let y = fy + row_y[i] + 5;
        assert_eq!(
            set(m.click(fx + 30, y, ORIG)),
            BugFixes::all_but(BugFix::LoadMusik)
        );
        // Inside the box, off the rows: nothing.
        assert_eq!(m.click(fx + 30, fy + 3, ORIG), MenuAction::None);
        assert!(m.in_fixes());
        // Outside: back to the menu, still open.
        assert_eq!(m.click(2, 2, ORIG), MenuAction::None);
        assert!(!m.in_fixes() && m.is_open());
    }

    /// The box of every build's fixes: its title names the game, its rows
    /// and the selected fix's explanation drawn, everything inside the
    /// screen with a margin, every text inside the box.
    #[test]
    fn the_fixes_box_fits_for_every_build() {
        use atlantis_data::exe::BUILDS;
        let font = SubtitleFont::new();
        // The data's fixes, one or the other (the patch 1.01's data is the
        // German or the Dutch CD's, not both).
        let plain = crate::enhancements::FixData::default();
        let data = [
            plain,
            crate::enhancements::FixData {
                sharp_s: true,
                ..plain
            },
            crate::enhancements::FixData {
                disc_revisions: true,
                ..plain
            },
        ];
        for b in BUILDS.iter() {
            for data in &data {
                let fixes: Vec<BugFix> = BugFix::ALL
                    .into_iter()
                    .filter(|f| f.applies(b, data))
                    .collect();
                let mut m = OptionsMenu::new();
                m.set_game(
                    Some("CD, Simplified Chinese (2001 build)"),
                    Some(fixes.clone()),
                );
                m.open();
                m.key(Key::Down, ORIG);
                m.key(Key::Down, ORIG);
                m.key(Key::Enter, ORIG);
                for _ in 0..fixes.len() {
                    m.key(Key::Down, ORIG);
                }
                m.key(Key::Down, ORIG);
                assert_eq!(m.fix_selected(), FixRow::Fix(*fixes.last().unwrap()));
                let layout = m.fix_layout();
                let n = m.fix_rows().len();
                let (bx, by) = layout.origin(n, SCREEN_W, SCREEN_H);
                assert!(bx >= 8 && by >= 8, "{}", b.name);
                assert!(by + layout.box_h(n) + 8 <= SCREEN_H as i32, "{}", b.name);
                let mut s = vec![0x1234u16; SCREEN_W * SCREEN_H];
                let mut t = HdText::default();
                m.draw_recording(&mut s, &font, ORIG, Some(&mut t));
                let texts: Vec<String> = t
                    .items
                    .iter()
                    .map(|i| String::from_utf8_lossy(&i.text).into_owned())
                    .collect();
                assert_eq!(texts[0], "Bug fixes: CD, Simplified Chinese (2001 build)");
                for f in &fixes {
                    assert!(texts.iter().any(|t| t == f.label()));
                }
                let last = fixes.last().unwrap().explanation();
                for line in last {
                    assert!(texts.iter().any(|t| t == line), "{line}");
                }
                assert_eq!(texts.last().unwrap(), FIX_KEYS);
                // The title, 3 buttons, a label and a value per fix, the
                // explanation and the keys.
                assert_eq!(t.items.len(), 1 + 3 + 2 * fixes.len() + last.len() + 1);
                for y in 0..SCREEN_H as i32 {
                    for x in 0..SCREEN_W as i32 {
                        let inside = (bx - 1..bx + layout.width + 1).contains(&x)
                            && (by - 1..by + layout.box_h(n) + 1).contains(&y);
                        if !inside {
                            assert_eq!(s[y as usize * SCREEN_W + x as usize], 0x1234);
                        }
                    }
                }
            }
        }
        // Every text fits its box: the labels with their values, the
        // explanations, the longest title.
        for f in BugFix::ALL {
            let label = font.width(f.label().as_bytes()) + font.width(b"< Off >");
            assert!(16 + label + 24 + 16 <= FIX_BOX_W, "{f:?}");
            let e = f.explanation();
            assert!(!e.is_empty() && e.len() <= FIX_TEXT_LINES, "{f:?}");
            for line in e {
                assert!(font.width(line.as_bytes()) <= FIX_BOX_W - 32, "{line}");
            }
        }
        for line in [
            FIX_KEYS,
            "Every fix off: the original, bug for bug",
            "Bug fixes: CD, patch 1.01, Traditional Chinese (May 1997 build)",
        ] {
            assert!(font.width(line.as_bytes()) <= FIX_BOX_W - 32, "{line}");
        }
        for e in [Entry::BugFixes, Entry::ChooseFixes] {
            assert!(font.width(OptionsMenu::description(e, ORIG).as_bytes()) <= BOX_W - 20);
        }
    }

    /// Without a game, the fixes of every build.
    #[test]
    fn without_a_game_the_fixes_of_every_build() {
        let m = OptionsMenu::new();
        assert_eq!(m.game_fixes().len(), 8);
        assert!(m.game_fixes().iter().all(|f| f.of_every_build()));
        assert_eq!(m.fix_title, "Original bug fixes");
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
