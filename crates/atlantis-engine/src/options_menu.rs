//! The port's options menu (F10), drawn by the front end over whatever
//! frame the game shows: settings of the port that the original's menus do
//! not have. The game is paused while it is open.
//!
//! Entries: the graphics mode (see [`crate::hd`]), the three groups of
//! [`Enhancements`] (the names of the ScummVM port's options), and Close;
//! the entries are a list so that more can follow.

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
    Entry::BugFixes,
    Entry::RestoredContent,
    Entry::UiUx,
    Entry::Close,
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

const BOX_W: i32 = 400;
const ROW_H: i32 = 24;
const TITLE_H: i32 = 34;
/// The footer: two lines on the game and the port, the selected entry's
/// description, the keys.
const FOOTER_H: i32 = 86;
const BOX_H: i32 = TITLE_H + ENTRIES.len() as i32 * ROW_H + FOOTER_H;
const BOX_X: i32 = (SCREEN_W as i32 - BOX_W) / 2;
const BOX_Y: i32 = (SCREEN_H as i32 - BOX_H) / 2;

const BG: u16 = 0x18e3;
const BORDER: u16 = 0xbdf7;
const HIGHLIGHT: u16 = 0x3a0c;
const TEXT: u16 = 0xffff;
const DIM: u16 = 0x9cf3;
const GREYED: u16 = 0x632c;
const SELECTED: u16 = 0xffe0;

/// The menu's state.
#[derive(Debug, Default)]
pub struct OptionsMenu {
    open: bool,
    selected: usize,
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
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// The selected entry.
    pub fn selected(&self) -> Entry {
        ENTRIES[self.selected]
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
        }
    }

    /// The next entry from the selected one in `dir` (+1 / -1) that can be
    /// changed.
    fn step(&mut self, dir: usize, v: MenuValues) {
        let n = ENTRIES.len();
        for _ in 0..n {
            self.selected = (self.selected + dir) % n;
            if v.enabled(ENTRIES[self.selected]) {
                break;
            }
        }
    }

    /// A key pressed while the menu is open: Up / Down choose an entry
    /// (skipping the greyed ones), Enter / Space (and Left / Right on a
    /// setting) change it, Escape and F10 close the menu.
    pub fn key(&mut self, key: Key, v: MenuValues) -> MenuAction {
        match key {
            Key::Escape | Key::F(10) => {
                self.close();
                MenuAction::Close
            }
            Key::Up => {
                self.step(ENTRIES.len() - 1, v);
                MenuAction::None
            }
            Key::Down => {
                self.step(1, v);
                MenuAction::None
            }
            Key::Enter | Key::Space => self.activate(self.selected(), v),
            Key::Left | Key::Right if self.selected() != Entry::Close => {
                self.activate(self.selected(), v)
            }
            _ => MenuAction::None,
        }
    }

    /// The entry under a 640x480 screen point.
    fn entry_at(x: i32, y: i32) -> Option<usize> {
        if !(BOX_X..BOX_X + BOX_W).contains(&x) {
            return None;
        }
        let r = y - (BOX_Y + TITLE_H);
        (r >= 0 && r < ENTRIES.len() as i32 * ROW_H).then_some((r / ROW_H) as usize)
    }

    /// The pointer moved to a 640x480 screen point: the entry under it is
    /// selected.
    pub fn pointer(&mut self, x: i32, y: i32) {
        if let Some(i) = Self::entry_at(x, y) {
            self.selected = i;
        }
    }

    /// A click at a 640x480 screen point: the entry under it changes;
    /// outside the box the menu closes.
    pub fn click(&mut self, x: i32, y: i32, v: MenuValues) -> MenuAction {
        if let Some(i) = Self::entry_at(x, y) {
            self.selected = i;
            return self.activate(ENTRIES[i], v);
        }
        let inside = (BOX_X..BOX_X + BOX_W).contains(&x) && (BOX_Y..BOX_Y + BOX_H).contains(&y);
        if inside {
            MenuAction::None
        } else {
            self.close();
            MenuAction::Close
        }
    }

    /// The menu's boxes (x, y, w, h, colour) and text.
    fn layout(&self, font: &SubtitleFont, v: MenuValues) -> (Vec<[i32; 5]>, Vec<TextItem>) {
        let mut fills = vec![
            [
                BOX_X - 1,
                BOX_Y - 1,
                BOX_W + 2,
                BOX_H + 2,
                i32::from(BORDER),
            ],
            [BOX_X, BOX_Y, BOX_W, BOX_H, i32::from(BG)],
        ];
        let mut texts = Vec::new();
        let mut text = |t: &str, x: i32, y: i32, c: u16| {
            texts.push(TextItem {
                x,
                y,
                max_w: BOX_W - 20,
                color: c,
                text: t.as_bytes().to_vec(),
                bitmap: None,
            });
        };
        let centre = |t: &str| BOX_X + (BOX_W - font.width(t.as_bytes())) / 2;
        let right = |t: &str| BOX_X + BOX_W - 16 - font.width(t.as_bytes());
        let on = |b: bool| if b { "On" } else { "Off" };
        let t = "Atlantis Native options";
        text(t, centre(t), BOX_Y + 10, TEXT);
        for (i, &e) in ENTRIES.iter().enumerate() {
            let y = BOX_Y + TITLE_H + i as i32 * ROW_H;
            let sel = i == self.selected;
            if sel {
                fills.push([BOX_X + 6, y, BOX_W - 12, ROW_H - 2, i32::from(HIGHLIGHT)]);
            }
            let c = match (v.enabled(e), sel) {
                (false, _) => GREYED,
                (true, true) => SELECTED,
                (true, false) => TEXT,
            };
            let (label, value) = match e {
                Entry::Graphics => (
                    "Graphics",
                    match v.graphics {
                        Graphics::Original => "Original",
                        Graphics::Hd => "HD",
                    },
                ),
                Entry::PanoramaFilter => (
                    "Panorama filter",
                    match v.hd.panorama {
                        PanoFilter::Bilinear => "Bilinear",
                        PanoFilter::Bicubic => "Bicubic",
                    },
                ),
                Entry::Sharpen => ("Panorama sharpening", on(v.hd.sharpen)),
                Entry::Interface => (
                    "Interface",
                    if v.hd.smooth_interface {
                        "Smooth"
                    } else {
                        "Pixels"
                    },
                ),
                Entry::SharpSubtitles => ("Sharp subtitles", on(v.hd.sharp_subtitles)),
                Entry::Widescreen => ("Widescreen", on(v.hd.widescreen)),
                Entry::VideoSides => (
                    "Video sides",
                    match v.hd.video_sides {
                        VideoSides::Black => "Black",
                        VideoSides::Blur => "Blurred",
                        VideoSides::Zoom => "Zoomed",
                        VideoSides::Panorama => "Panorama",
                    },
                ),
                Entry::BugFixes => ("Fix original bugs", on(v.enhancements.minor_bug_fixes)),
                Entry::RestoredContent => ("Restored content", on(v.enhancements.restored_content)),
                Entry::UiUx => ("Modern UI/UX", on(v.enhancements.ui_ux)),
                Entry::Close => {
                    text("Close", centre("Close"), y + 4, c);
                    continue;
                }
            };
            text(label, BOX_X + 16, y + 4, c);
            let t = format!("< {value} >");
            text(&t, right(&t), y + 4, c);
        }
        for (i, line) in self.info.iter().take(2).enumerate() {
            text(line, centre(line), BOX_Y + BOX_H - 82 + i as i32 * 18, DIM);
        }
        let d = Self::description(self.selected(), v);
        text(d, centre(d), BOX_Y + BOX_H - 44, TEXT);
        let k = "Arrows, Enter or click; Escape or F10 closes";
        text(k, centre(k), BOX_Y + BOX_H - 22, DIM);
        (fills, texts)
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
        let (fills, items) = self.layout(font, v);
        for [x, y, w, h, c] in fills {
            for yy in y.max(0)..(y + h).min(SCREEN_H as i32) {
                for xx in x.max(0)..(x + w).min(SCREEN_W as i32) {
                    screen[yy as usize * SCREEN_W + xx as usize] = c as u16;
                }
            }
        }
        let under = text.as_ref().map(|_| screen.to_vec());
        for it in &items {
            font.draw(
                screen, SCREEN_W, SCREEN_H, &it.text, it.x, it.y, it.max_w, it.color,
            );
        }
        if let (Some(t), Some(under)) = (text, under) {
            t.set(items, &under, screen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for _ in 0..4 {
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
        assert_eq!(t.items.len(), 1 + 10 * 2 + 1 + 2);
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
}
