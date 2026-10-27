//! The port's options menu for Atlantis II (F10 or O), on the first
//! game's options box (`atlantis_engine::options_menu::MenuBox`: its
//! layout, font, colours, keys and mouse), drawn by the front end over the
//! frame shown while the game is paused. Settings of the port that the original's menus do not have,
//! and the original's own 800 x 600 screen, which its options page also
//! switches.
//!
//! Entries: the graphics (original or HD, [`crate::hd`]), the HD mode's
//! options (greyed in the original graphics), the screen (640 x 480 or
//! 800 x 600, [`crate::display`]; greyed in HD, whose picture is drawn
//! from the 640 x 480 screen), the bug fixes ([`Enhancements::fixes`]:
//! all on or all off, "Custom (n/N)" when mixed), "Choose the bug
//! fixes..." (a box of its own in the same style: all on, all off, one
//! switch per [`BugFix`] with what the original does, Back), the arrow
//! keys' turn ([`Enhancements::ui_ux`]), Close.

use atlantis_engine::hd::{HdText, PanoFilter, VideoSides};
use atlantis_engine::options_menu::{
    BoxInput, BoxLayout, FootLine, MenuBox, Row, DIM, TEXT as TEXT_COLOR,
};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::Key;

use crate::enhancements::{BugFix, BugFixes, Enhancements};

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
    Screen,
    BugFixes,
    ChooseFixes,
    ArrowKeys,
    Close,
}

const ENTRIES: [Entry; 12] = [
    Entry::Graphics,
    Entry::PanoramaFilter,
    Entry::Sharpen,
    Entry::Interface,
    Entry::SharpSubtitles,
    Entry::Widescreen,
    Entry::VideoSides,
    Entry::Screen,
    Entry::BugFixes,
    Entry::ChooseFixes,
    Entry::ArrowKeys,
    Entry::Close,
];

impl Entry {
    /// An option of the HD mode (greyed and fixed in the original
    /// graphics).
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

/// A row of the bug fixes' box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixRow {
    AllOn,
    AllOff,
    Fix(BugFix),
    Back,
}

const FIX_ROWS: [FixRow; 3 + BugFix::ALL.len()] = {
    let mut rows = [FixRow::Back; 3 + BugFix::ALL.len()];
    rows[0] = FixRow::AllOn;
    rows[1] = FixRow::AllOff;
    let mut i = 0;
    while i < BugFix::ALL.len() {
        rows[2 + i] = FixRow::Fix(BugFix::ALL[i]);
        i += 1;
    }
    rows
};

/// The lines of a fix's explanation at the foot of its box (at most
/// [`FIX_TEXT_LINES`]).
const FIX_TEXT_LINES: usize = 4;

/// The bug fixes' box: wider than the options' (the fixes' names), a gap
/// above the fixes and above Back, and a foot of four lines of
/// explanation and the keys.
const FIX_LAYOUT: BoxLayout<'static> = BoxLayout {
    title: "Original bug fixes",
    width: 520,
    gaps: &[2, 2 + BugFix::ALL.len()],
    footer_h: 12 + FIX_TEXT_LINES as i32 * 18 + 32,
};

/// The keys' line of the bug fixes' box.
const FIX_KEYS: &str = "Arrows, Enter or click; Escape goes back";

impl BugFix {
    /// Its row's name in the bug fixes' box.
    pub fn label(self) -> &'static str {
        match self {
            BugFix::MusicEnd => "Music: every track heard to its end",
            BugFix::MusicRestart => "Music: an ended track starts again",
            BugFix::MusicMark => "Music: the rare missing mark (1 in 32768)",
            BugFix::IrelandClicks => "Ireland: cross and ogham clicks",
        }
    }

    /// What the original does without it, and where a player notices it:
    /// the lines at the foot of the bug fixes' box.
    pub fn explanation(self) -> &'static [&'static str] {
        match self {
            BugFix::MusicEnd => &[
                "The original starts the next track about",
                "6 seconds before the old one ends: the last",
                "seconds of each piece of music are never heard.",
            ],
            BugFix::MusicRestart => &[
                "When a track ends with nothing to follow it,",
                "the original repeats its last 6 seconds over",
                "and over until another music starts.",
            ],
            BugFix::MusicMark => &[
                "Once in 32768 random starts of a music, the",
                "original picks a place past the track's last mark",
                "and stops with a fatal error (it saves and quits).",
            ],
            BugFix::IrelandClicks => &[
                "On Brigid's cross and the ogham stones, the",
                "original may take a click for another object:",
                "back to Tibet from the 4th stone (ch. 12), the",
                "water's film from the 9th with the horse (ch. 14).",
            ],
        }
    }
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

/// What the front end does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    /// The graphics, an HD option or the bug fixes changed: apply and save
    /// them (`settings.ini`).
    SetEnhancements(Enhancements),
    /// The screen size chosen (true: 800 x 600): the original's option,
    /// kept in `atlant2.cfg`.
    SetScreen800(bool),
    /// The menu closed.
    Close,
}

/// The values the menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuValues {
    pub enhancements: Enhancements,
    /// The original's screen option: 800 x 600.
    pub screen_800: bool,
}

impl MenuValues {
    fn enabled(&self, e: Entry) -> bool {
        match e {
            Entry::Screen => !self.enhancements.hd,
            e if e.hd_only() => self.enhancements.hd,
            _ => true,
        }
    }
}

/// The "Fix original bugs" row's value: On, Off, or how many are on.
fn fixes_value(x: &BugFixes) -> String {
    match x.count() {
        0 => "Off".into(),
        n if n == BugFix::ALL.len() => "On".into(),
        n => format!("Custom ({n}/{})", BugFix::ALL.len()),
    }
}

#[cfg(test)]
use atlantis_engine::options_menu::{BG, BOX_W, GAP_H, GREYED, ROW_H, SELECTED, TEXT, TITLE_H};

/// The menu's state.
#[derive(Debug, Default)]
pub struct OptionsMenu {
    menu: MenuBox,
    /// The bug fixes' box, over the menu while it is open.
    fixes: MenuBox,
    /// Shown at the foot of the menu: the game's release, the port's
    /// version (set by the front end).
    info: Vec<String>,
}

impl OptionsMenu {
    pub fn new() -> OptionsMenu {
        OptionsMenu::default()
    }

    /// The lines shown at the foot of the menu (at most two).
    pub fn set_info(&mut self, lines: Vec<String>) {
        self.info = lines;
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
        ENTRIES[self.menu.selected()]
    }

    /// The bug fixes' box is open.
    pub fn in_fixes(&self) -> bool {
        self.menu.is_open() && self.fixes.is_open()
    }

    /// The bug fixes' box's selected row.
    pub fn fix_selected(&self) -> FixRow {
        FIX_ROWS[self.fixes.selected()]
    }

    #[cfg(test)]
    fn box_h() -> i32 {
        MenuBox::box_h(ENTRIES.len())
    }

    /// The box's top left corner on a `w` x `h` screen (centred).
    #[cfg(test)]
    fn origin(w: usize, h: usize) -> (i32, i32) {
        MenuBox::origin(ENTRIES.len(), w, h)
    }

    fn activate(&mut self, entry: Entry, v: MenuValues) -> MenuAction {
        if !v.enabled(entry) {
            return MenuAction::None;
        }
        let mut x = v.enhancements;
        let o = &mut x.hd_options;
        match entry {
            Entry::Graphics => x.hd = !x.hd,
            Entry::PanoramaFilter => {
                o.panorama = match o.panorama {
                    PanoFilter::Bilinear => PanoFilter::Bicubic,
                    PanoFilter::Bicubic => PanoFilter::Bilinear,
                };
            }
            Entry::Sharpen => o.sharpen = !o.sharpen,
            Entry::Interface => o.smooth_interface = !o.smooth_interface,
            Entry::SharpSubtitles => o.sharp_subtitles = !o.sharp_subtitles,
            Entry::Widescreen => o.widescreen = !o.widescreen,
            Entry::VideoSides => {
                o.video_sides = match o.video_sides {
                    VideoSides::Black => VideoSides::Blur,
                    _ => VideoSides::Black,
                };
            }
            // All on, then all off (from a mix: all on).
            Entry::BugFixes => x.fixes = BugFixes::all(x.fixes != BugFixes::ALL),
            Entry::ChooseFixes => {
                self.fixes.open();
                return MenuAction::None;
            }
            Entry::ArrowKeys => x.ui_ux = !x.ui_ux,
            Entry::Screen => return MenuAction::SetScreen800(!v.screen_800),
            Entry::Close => {
                self.close();
                return MenuAction::Close;
            }
        }
        MenuAction::SetEnhancements(x)
    }

    /// A row of the bug fixes' box activated.
    fn activate_fix(&mut self, row: FixRow, v: MenuValues) -> MenuAction {
        let mut x = v.enhancements;
        match row {
            FixRow::AllOn => x.fixes = BugFixes::ALL,
            FixRow::AllOff => x.fixes = BugFixes::NONE,
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
            return if entry == Entry::Screen {
                "An option of the original graphics"
            } else {
                "An option of the HD graphics"
            };
        }
        match entry {
            Entry::Graphics => "HD: the panorama at the window's resolution",
            Entry::PanoramaFilter => "The panorama's sampling: Bicubic is sharper",
            Entry::Sharpen => "A mild sharpening of the panorama, without halos",
            Entry::Interface => "Menus, bars and videos: Smooth (xBR) or Pixels",
            Entry::SharpSubtitles => "Subtitles drawn at the window's resolution",
            Entry::Widescreen => "The panorama fills the sides of a wide window",
            Entry::VideoSides => "Widescreen: the bars beside the videos",
            Entry::Screen => "The original's screen sizes (its options page)",
            Entry::BugFixes => "Every fix of the original's bugs on or off",
            Entry::ChooseFixes => "Enter: one fix at a time",
            Entry::ArrowKeys => "The arrow keys turn the view (the original: the mouse only)",
            Entry::Close => "Back to the game",
        }
    }

    /// The rows the box shows for the values `v` (`fixes`: the bug fixes'
    /// value, [`fixes_value`]).
    fn rows(v: MenuValues, fixes: &str) -> Vec<Row<'_>> {
        let on = |b: bool| if b { "On" } else { "Off" };
        let x = v.enhancements;
        ENTRIES
            .iter()
            .map(|&e| {
                let (label, value) = match e {
                    Entry::Graphics => ("Graphics", Some(if x.hd { "HD" } else { "Original" })),
                    Entry::PanoramaFilter => (
                        "Panorama filter",
                        Some(match x.hd_options.panorama {
                            PanoFilter::Bilinear => "Bilinear",
                            PanoFilter::Bicubic => "Bicubic",
                        }),
                    ),
                    Entry::Sharpen => ("Panorama sharpening", Some(on(x.hd_options.sharpen))),
                    Entry::Interface => (
                        "Interface",
                        Some(if x.hd_options.smooth_interface {
                            "Smooth"
                        } else {
                            "Pixels"
                        }),
                    ),
                    Entry::SharpSubtitles => {
                        ("Sharp subtitles", Some(on(x.hd_options.sharp_subtitles)))
                    }
                    Entry::Widescreen => ("Widescreen", Some(on(x.hd_options.widescreen))),
                    Entry::VideoSides => (
                        "Video sides",
                        Some(match x.hd_options.video_sides {
                            VideoSides::Blur => "Blurred",
                            _ => "Black",
                        }),
                    ),
                    Entry::Screen => (
                        "Screen",
                        Some(if v.screen_800 {
                            "800 x 600"
                        } else {
                            "640 x 480"
                        }),
                    ),
                    Entry::BugFixes => ("Fix original bugs", Some(fixes)),
                    Entry::ChooseFixes => ("Choose the bug fixes...", None),
                    Entry::ArrowKeys => ("Arrow keys turn", Some(on(x.ui_ux))),
                    Entry::Close => ("Close", None),
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
    fn fix_rows(v: MenuValues) -> Vec<Row<'static>> {
        let x = v.enhancements.fixes;
        FIX_ROWS
            .iter()
            .map(|&r| {
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
            BoxInput::Activate(i) => self.activate(ENTRIES[i], v),
        }
    }

    /// What the bug fixes' box's input `input` does: Escape and a click
    /// outside go back to the menu.
    fn act_fix(&mut self, input: BoxInput, v: MenuValues) -> MenuAction {
        match input {
            BoxInput::None | BoxInput::Close => MenuAction::None,
            BoxInput::Activate(i) => self.activate_fix(FIX_ROWS[i], v),
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
            let input = self.fixes.key(key, &Self::fix_rows(v));
            return self.act_fix(input, v);
        }
        let value = fixes_value(&v.enhancements.fixes);
        let input = self.menu.key(key, &Self::rows(v, &value));
        self.act(input, v)
    }

    /// The pointer moved to a point of a `w` x `h` screen: the entry under
    /// it is selected.
    pub fn pointer(&mut self, x: i32, y: i32, w: usize, h: usize) {
        if self.fixes.is_open() {
            self.fixes
                .pointer_in(&FIX_LAYOUT, FIX_ROWS.len(), x, y, w, h);
        } else {
            self.menu.pointer(ENTRIES.len(), x, y, w, h);
        }
    }

    /// A click at a point of a `w` x `h` screen: the entry under it
    /// changes; outside the box the menu closes (the bug fixes' box goes
    /// back to the menu).
    pub fn click(&mut self, x: i32, y: i32, w: usize, h: usize, v: MenuValues) -> MenuAction {
        if self.fixes.is_open() {
            let input = self.fixes.click_in(&FIX_LAYOUT, FIX_ROWS.len(), x, y, w, h);
            return self.act_fix(input, v);
        }
        let input = self.menu.click(ENTRIES.len(), x, y, w, h);
        self.act(input, v)
    }

    /// Draws the menu over a `w` x `h` RGB565 frame, centred.
    pub fn draw(&self, screen: &mut [u16], w: usize, h: usize, font: &SubtitleFont, v: MenuValues) {
        self.draw_recording(screen, w, h, font, v, None);
    }

    /// [`OptionsMenu::draw`], recording the text in `text` for the HD
    /// mode to draw sharp (the first game's `HdText`).
    pub fn draw_recording(
        &self,
        screen: &mut [u16],
        w: usize,
        h: usize,
        font: &SubtitleFont,
        v: MenuValues,
        record: Option<&mut HdText>,
    ) {
        if self.fixes.is_open() {
            let lines = self.fix_selected().explanation();
            let mut foot: Vec<FootLine> = lines
                .iter()
                .take(FIX_TEXT_LINES)
                .enumerate()
                .map(|(i, t)| FootLine {
                    text: t,
                    from_bottom: FIX_LAYOUT.footer_h - 12 - i as i32 * 18,
                    color: TEXT_COLOR,
                })
                .collect();
            foot.push(FootLine {
                text: FIX_KEYS,
                from_bottom: 22,
                color: DIM,
            });
            self.fixes.draw_in(
                &FIX_LAYOUT,
                screen,
                w,
                h,
                font,
                &Self::fix_rows(v),
                &foot,
                record,
            );
            return;
        }
        let d = Self::description(self.selected(), v);
        let value = fixes_value(&v.enhancements.fixes);
        self.menu.draw_recording(
            screen,
            w,
            h,
            font,
            &Self::rows(v, &value),
            &self.info,
            d,
            record,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIG: MenuValues = MenuValues {
        enhancements: Enhancements {
            fixes: BugFixes::ALL,
            ui_ux: true,
            hd: false,
            hd_options: atlantis_engine::hd::HdOptions {
                panorama: PanoFilter::Bicubic,
                sharpen: true,
                smooth_interface: true,
                sharp_subtitles: true,
                widescreen: false,
                video_sides: VideoSides::Black,
            },
        },
        screen_800: false,
    };

    fn hd() -> MenuValues {
        let mut v = ORIG;
        v.enhancements.hd = true;
        v
    }

    #[test]
    fn keys_switch_and_close() {
        let mut m = OptionsMenu::new();
        m.open();
        assert_eq!(m.selected(), Entry::Graphics);
        let MenuAction::SetEnhancements(x) = m.key(Key::Enter, ORIG) else {
            panic!("no change");
        };
        assert!(x.hd);
        // Original graphics: Down skips the HD rows to the screen.
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::Screen);
        assert_eq!(m.key(Key::Right, ORIG), MenuAction::SetScreen800(true));
        let v = MenuValues {
            screen_800: true,
            ..ORIG
        };
        assert_eq!(m.key(Key::Left, v), MenuAction::SetScreen800(false));
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::BugFixes);
        let MenuAction::SetEnhancements(x) = m.key(Key::Space, ORIG) else {
            panic!("no change");
        };
        assert!(x.fixes == BugFixes::NONE && !x.hd);
        // "Choose the bug fixes...": its box, nothing changed yet.
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::ChooseFixes);
        assert_eq!(m.key(Key::Right, ORIG), MenuAction::None, "a button");
        assert!(!m.in_fixes());
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::None);
        assert!(m.in_fixes() && m.is_open());
        assert_eq!(m.key(Key::Escape, ORIG), MenuAction::None);
        assert!(!m.in_fixes() && m.is_open());
        assert_eq!(m.selected(), Entry::ChooseFixes);
        // The arrow keys' turn, a port convenience (`--original` off).
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::ArrowKeys);
        let MenuAction::SetEnhancements(x) = m.key(Key::Enter, ORIG) else {
            panic!("no change");
        };
        assert!(!x.ui_ux && x.fixes == BugFixes::ALL);
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::Close);
        assert_eq!(m.key(Key::Left, ORIG), MenuAction::None);
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::Close);
        assert!(!m.is_open());
        for k in [Key::Escape, Key::F(10), Key::Char('O')] {
            m.open();
            assert_eq!(m.key(k, ORIG), MenuAction::Close);
            assert!(!m.is_open());
        }
    }

    #[test]
    fn hd_rows_in_hd_only_and_the_screen_in_original_only() {
        let mut m = OptionsMenu::new();
        m.open();
        let v = hd();
        let mut seen = Vec::new();
        for _ in 0..6 {
            m.key(Key::Down, v);
            seen.push(m.selected());
            let MenuAction::SetEnhancements(x) = m.key(Key::Enter, v) else {
                panic!("no change");
            };
            assert_ne!(x, v.enhancements);
        }
        assert!(seen.iter().all(|e| e.hd_only()));
        m.key(Key::Down, v);
        assert_eq!(m.selected(), Entry::BugFixes, "the screen is skipped in HD");
        // A click on a greyed row selects it but changes nothing.
        let (bx, by) = OptionsMenu::origin(640, 480);
        let y = by + TITLE_H + 7 * ROW_H + 5;
        assert_eq!(m.click(bx + 50, y, 640, 480, v), MenuAction::None);
        assert_eq!(m.selected(), Entry::Screen);
        assert_eq!(
            OptionsMenu::description(Entry::Screen, v),
            "An option of the original graphics"
        );
        assert_eq!(
            OptionsMenu::description(Entry::Sharpen, ORIG),
            "An option of the HD graphics"
        );
        // The video sides: black and blurred.
        let mut m = OptionsMenu::new();
        m.open();
        for _ in 0..6 {
            m.key(Key::Down, v);
        }
        assert_eq!(m.selected(), Entry::VideoSides);
        let MenuAction::SetEnhancements(x) = m.key(Key::Enter, v) else {
            panic!();
        };
        assert_eq!(x.hd_options.video_sides, VideoSides::Blur);
    }

    #[test]
    fn draw_records_its_text_for_hd() {
        let font = SubtitleFont::new();
        let mut m = OptionsMenu::new();
        m.open();
        let mut s = vec![0x1234u16; 640 * 480];
        let mut t = HdText::default();
        m.draw_recording(&mut s, 640, 480, &font, hd(), Some(&mut t));
        let mut plain = vec![0x1234u16; 640 * 480];
        m.draw(&mut plain, 640, 480, &font, hd());
        assert!(s == plain, "same pixels with or without recording");
        assert!(t.after == s);
        // The title, 10 rows of a label and a value, two buttons, the
        // description and the keys.
        assert_eq!(t.items.len(), 1 + 10 * 2 + 2 + 2);
        assert!(t.items.iter().any(|i| i.text == b"< HD >"));
        let mut u = s.clone();
        t.uncover(&mut u);
        assert!(u == t.under && !u.contains(&TEXT) && u.contains(&BG));
    }

    #[test]
    fn draws_centred_on_both_screens() {
        let font = SubtitleFont::new();
        for (w, h) in [(640, 480), (800, 600)] {
            let mut m = OptionsMenu::new();
            m.set_info(vec!["Atlantis II".into(), "Atlantis Native".into()]);
            m.open();
            let mut s = vec![0x1234u16; w * h];
            m.draw(&mut s, w, h, &font, ORIG);
            let (bx, by) = OptionsMenu::origin(w, h);
            assert!(bx >= 1 && by >= 1 && by + OptionsMenu::box_h() < h as i32);
            assert_eq!(s[(by as usize + 2) * w + bx as usize + 2], BG);
            assert!(s.contains(&SELECTED) && s.contains(&GREYED));
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let inside = (bx - 1..bx + BOX_W + 1).contains(&x)
                        && (by - 1..by + OptionsMenu::box_h() + 1).contains(&y);
                    if !inside {
                        assert_eq!(s[y as usize * w + x as usize], 0x1234, "({x}, {y})");
                    }
                }
            }
            // The pointer and the clicks find the rows on that screen.
            m.pointer(bx + 30, by + TITLE_H + 8 * ROW_H + 3, w, h);
            assert_eq!(m.selected(), Entry::BugFixes);
            assert_eq!(m.click(2, 2, w, h, ORIG), MenuAction::Close);
        }
    }

    fn with_fixes(f: BugFixes) -> MenuValues {
        let mut v = ORIG;
        v.enhancements.fixes = f;
        v
    }

    #[test]
    fn the_bug_fixes_row_shows_all_none_or_how_many() {
        let rows = |f: BugFixes| {
            let value = fixes_value(&f);
            let row = OptionsMenu::rows(with_fixes(f), &value)[8];
            (row.label.to_string(), row.value.map(str::to_string))
        };
        assert_eq!(rows(BugFixes::ALL).1.as_deref(), Some("On"));
        assert_eq!(rows(BugFixes::NONE).1.as_deref(), Some("Off"));
        let mut three = BugFixes::ALL;
        three.ireland_clicks = false;
        assert_eq!(
            rows(three),
            ("Fix original bugs".into(), Some("Custom (3/4)".into()))
        );
        assert_eq!(
            rows(BugFixes::only(BugFix::MusicEnd)).1.as_deref(),
            Some("Custom (1/4)")
        );
        // Pressed: all on, then all off; from a mix, all on.
        let mut m = OptionsMenu::new();
        m.open();
        let press = |m: &mut OptionsMenu, f: BugFixes| {
            m.pointer(
                320,
                OptionsMenu::origin(640, 480).1 + TITLE_H + 8 * ROW_H + 3,
                640,
                480,
            );
            assert_eq!(m.selected(), Entry::BugFixes);
            match m.key(Key::Enter, with_fixes(f)) {
                MenuAction::SetEnhancements(x) => x.fixes,
                a => panic!("{a:?}"),
            }
        };
        assert_eq!(press(&mut m, BugFixes::ALL), BugFixes::NONE);
        assert_eq!(press(&mut m, BugFixes::NONE), BugFixes::ALL);
        assert_eq!(press(&mut m, three), BugFixes::ALL);
    }

    /// The bug fixes' box by the keys: each fix alone toggled, all on, all
    /// off, Back; F10 closes everything.
    #[test]
    fn the_fixes_box_by_the_keys() {
        let mut m = OptionsMenu::new();
        m.open();
        for _ in 0..9 {
            m.key(Key::Down, ORIG);
        }
        assert_eq!(m.selected(), Entry::ChooseFixes);
        m.key(Key::Enter, ORIG);
        assert!(m.in_fixes());
        assert_eq!(m.fix_selected(), FixRow::AllOn);
        let set = |a: MenuAction| match a {
            MenuAction::SetEnhancements(x) => x.fixes,
            a => panic!("{a:?}"),
        };
        assert_eq!(
            set(m.key(Key::Enter, with_fixes(BugFixes::NONE))),
            BugFixes::ALL
        );
        m.key(Key::Down, ORIG);
        assert_eq!(m.fix_selected(), FixRow::AllOff);
        assert_eq!(m.key(Key::Left, ORIG), MenuAction::None, "a button");
        assert_eq!(set(m.key(Key::Space, ORIG)), BugFixes::NONE);
        for f in BugFix::ALL {
            m.key(Key::Down, ORIG);
            assert_eq!(m.fix_selected(), FixRow::Fix(f));
            // Each key changes that fix alone, both ways.
            let mut want = BugFixes::ALL;
            want.set(f, false);
            assert_eq!(set(m.key(Key::Right, ORIG)), want);
            assert_eq!(set(m.key(Key::Left, ORIG)), want);
            assert_eq!(
                set(m.key(Key::Enter, with_fixes(BugFixes::NONE))),
                BugFixes::only(f)
            );
            assert!(!f.explanation().is_empty() && f.explanation().len() <= FIX_TEXT_LINES);
        }
        m.key(Key::Down, ORIG);
        assert_eq!(m.fix_selected(), FixRow::Back);
        assert_eq!(m.key(Key::Enter, ORIG), MenuAction::None);
        assert!(!m.in_fixes() && m.is_open());
        // Up from the first row wraps to Back.
        m.key(Key::Enter, ORIG);
        m.key(Key::Up, ORIG);
        assert_eq!(m.fix_selected(), FixRow::Back);
        for k in [Key::F(10), Key::Char('o')] {
            assert_eq!(m.key(k, ORIG), MenuAction::Close);
            assert!(!m.is_open() && !m.in_fixes());
            m.open();
            assert!(!m.in_fixes(), "the menu opens on its first box");
            for _ in 0..9 {
                m.key(Key::Down, ORIG);
            }
            m.key(Key::Enter, ORIG);
        }
    }

    /// The bug fixes' box by the mouse: its rows found across the gaps,
    /// a click outside back to the menu; on both screens.
    #[test]
    fn the_fixes_box_by_the_mouse() {
        for (w, h) in [(640, 480), (800, 600)] {
            let mut m = OptionsMenu::new();
            m.open();
            let (bx, by) = OptionsMenu::origin(w, h);
            let y = by + TITLE_H + 9 * ROW_H + 5;
            assert_eq!(m.click(bx + 50, y, w, h, ORIG), MenuAction::None);
            assert!(m.in_fixes());
            let (fx, fy) = FIX_LAYOUT.origin(FIX_ROWS.len(), w, h);
            assert_eq!(FIX_LAYOUT.row_y(2), TITLE_H + 2 * ROW_H + GAP_H);
            for (i, &row) in FIX_ROWS.iter().enumerate() {
                m.pointer(fx + 30, fy + FIX_LAYOUT.row_y(i) + 3, w, h);
                assert_eq!(m.fix_selected(), row);
            }
            // The gap above the fixes is no row.
            m.pointer(fx + 30, fy + TITLE_H + 2 * ROW_H + 2, w, h);
            assert_eq!(m.fix_selected(), FixRow::Back);
            let ireland = 2 + 3;
            let y = fy + FIX_LAYOUT.row_y(ireland) + 5;
            assert_eq!(
                m.click(fx + 30, y, w, h, ORIG),
                MenuAction::SetEnhancements(Enhancements {
                    fixes: {
                        let mut f = BugFixes::ALL;
                        f.ireland_clicks = false;
                        f
                    },
                    ..ORIG.enhancements
                })
            );
            assert_eq!(m.fix_selected(), FixRow::Fix(BugFix::IrelandClicks));
            // Inside the box, off the rows: nothing.
            assert_eq!(m.click(fx + 30, fy + 3, w, h, ORIG), MenuAction::None);
            assert!(m.in_fixes());
            // Outside: back to the menu, still open.
            assert_eq!(m.click(2, 2, w, h, ORIG), MenuAction::None);
            assert!(!m.in_fixes() && m.is_open());
        }
    }

    #[test]
    fn the_fixes_box_draws_its_rows_and_the_explanation() {
        let font = SubtitleFont::new();
        let mut m = OptionsMenu::new();
        m.open();
        for _ in 0..9 {
            m.key(Key::Down, ORIG);
        }
        m.key(Key::Enter, ORIG);
        for _ in 0..5 {
            m.key(Key::Down, ORIG);
        }
        assert_eq!(m.fix_selected(), FixRow::Fix(BugFix::IrelandClicks));
        for (w, h) in [(640, 480), (800, 600)] {
            let mut s = vec![0x1234u16; w * h];
            let mut t = HdText::default();
            m.draw_recording(&mut s, w, h, &font, ORIG, Some(&mut t));
            let texts: Vec<String> = t
                .items
                .iter()
                .map(|i| String::from_utf8_lossy(&i.text).into_owned())
                .collect();
            assert_eq!(texts[0], "Original bug fixes");
            for f in BugFix::ALL {
                assert!(texts.iter().any(|t| t == f.label()));
            }
            for line in BugFix::IrelandClicks.explanation() {
                assert!(texts.iter().any(|t| t == line), "{line}");
            }
            assert_eq!(texts.last().unwrap(), FIX_KEYS);
            // The title, 3 buttons, 4 rows of a label and a value, the
            // explanation and the keys.
            assert_eq!(t.items.len(), 1 + 3 + 4 * 2 + 4 + 1);
            assert!(s.contains(&SELECTED) && s.contains(&BG));
            // Inside the screen, a margin around.
            let (bx, by) = FIX_LAYOUT.origin(FIX_ROWS.len(), w, h);
            assert!(bx >= 16 && by >= 16);
            assert!(by + FIX_LAYOUT.box_h(FIX_ROWS.len()) + 16 <= h as i32);
        }
        // Every text fits its box: the labels with their values, the
        // explanations.
        for f in BugFix::ALL {
            let label = font.width(f.label().as_bytes()) + font.width(b"< Off >");
            assert!(16 + label + 24 + 16 <= FIX_LAYOUT.width, "{f:?}");
            for line in f.explanation() {
                assert!(
                    font.width(line.as_bytes()) <= FIX_LAYOUT.width - 32,
                    "{line}"
                );
            }
        }
        for line in [FIX_KEYS, "Every fix off: the original, bug for bug"] {
            assert!(font.width(line.as_bytes()) <= FIX_LAYOUT.width - 32);
        }
        assert!(
            font.width(OptionsMenu::description(Entry::BugFixes, ORIG).as_bytes()) <= BOX_W - 20
        );
    }
}
