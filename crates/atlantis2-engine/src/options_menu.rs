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
//! from the 640 x 480 screen), the minor bug fixes
//! ([`Enhancements::minor_bug_fixes`]), the arrow keys' turn
//! ([`Enhancements::ui_ux`]), Close.

use atlantis_engine::hd::{HdText, PanoFilter, VideoSides};
use atlantis_engine::options_menu::{BoxInput, MenuBox, Row};
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::Key;

use crate::enhancements::Enhancements;

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
    ArrowKeys,
    Close,
}

const ENTRIES: [Entry; 11] = [
    Entry::Graphics,
    Entry::PanoramaFilter,
    Entry::Sharpen,
    Entry::Interface,
    Entry::SharpSubtitles,
    Entry::Widescreen,
    Entry::VideoSides,
    Entry::Screen,
    Entry::BugFixes,
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

#[cfg(test)]
use atlantis_engine::options_menu::{BG, BOX_W, GREYED, ROW_H, SELECTED, TEXT, TITLE_H};

/// The menu's state.
#[derive(Debug, Default)]
pub struct OptionsMenu {
    menu: MenuBox,
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
    }

    pub fn close(&mut self) {
        self.menu.close();
    }

    /// The selected entry.
    pub fn selected(&self) -> Entry {
        ENTRIES[self.menu.selected()]
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
            Entry::BugFixes => x.minor_bug_fixes = !x.minor_bug_fixes,
            Entry::ArrowKeys => x.ui_ux = !x.ui_ux,
            Entry::Screen => return MenuAction::SetScreen800(!v.screen_800),
            Entry::Close => {
                self.close();
                return MenuAction::Close;
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
            Entry::BugFixes => "Fixes bugs of the original release (on by default)",
            Entry::ArrowKeys => "The arrow keys turn the view (the original: the mouse only)",
            Entry::Close => "Back to the game",
        }
    }

    /// The rows the box shows for the values `v`.
    fn rows(v: MenuValues) -> Vec<Row<'static>> {
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
                    Entry::BugFixes => ("Fix original bugs", Some(on(x.minor_bug_fixes))),
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

    /// What the box's input `input` does.
    fn act(&mut self, input: BoxInput, v: MenuValues) -> MenuAction {
        match input {
            BoxInput::None => MenuAction::None,
            BoxInput::Close => MenuAction::Close,
            BoxInput::Activate(i) => self.activate(ENTRIES[i], v),
        }
    }

    /// A key pressed while the menu is open: Up / Down choose an entry
    /// (skipping the greyed ones), Enter / Space (and Left / Right on a
    /// setting) change it, Escape, F10 and O close the menu.
    pub fn key(&mut self, key: Key, v: MenuValues) -> MenuAction {
        let input = self.menu.key(key, &Self::rows(v));
        self.act(input, v)
    }

    /// The pointer moved to a point of a `w` x `h` screen: the entry under
    /// it is selected.
    pub fn pointer(&mut self, x: i32, y: i32, w: usize, h: usize) {
        self.menu.pointer(ENTRIES.len(), x, y, w, h);
    }

    /// A click at a point of a `w` x `h` screen: the entry under it
    /// changes; outside the box the menu closes.
    pub fn click(&mut self, x: i32, y: i32, w: usize, h: usize, v: MenuValues) -> MenuAction {
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
        let d = Self::description(self.selected(), v);
        self.menu
            .draw_recording(screen, w, h, font, &Self::rows(v), &self.info, d, record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIG: MenuValues = MenuValues {
        enhancements: Enhancements {
            minor_bug_fixes: true,
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
        assert!(!x.minor_bug_fixes && !x.hd);
        // The arrow keys' turn, a port convenience (`--original` off).
        m.key(Key::Down, ORIG);
        assert_eq!(m.selected(), Entry::ArrowKeys);
        let MenuAction::SetEnhancements(x) = m.key(Key::Enter, ORIG) else {
            panic!("no change");
        };
        assert!(!x.ui_ux && x.minor_bug_fixes);
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
        // The title, 10 rows of a label and a value, Close, the
        // description and the keys.
        assert_eq!(t.items.len(), 1 + 10 * 2 + 1 + 2);
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
}
