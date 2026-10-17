//! The port's options menu (F10), drawn by the front end over whatever
//! frame the game shows: settings of the port that the original's menus do
//! not have. The game is paused while it is open.
//!
//! Entries: the graphics mode (see [`crate::hd`]), the three groups of
//! [`Enhancements`] (the names of the ScummVM port's options), and Close;
//! the entries are a list so that more can follow.

use crate::enhancements::Enhancements;
use crate::persist::Graphics;
use crate::subtitles::SubtitleFont;
use crate::sys::Key;
use crate::warp::{SCREEN_H, SCREEN_W};

/// An entry of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Graphics,
    BugFixes,
    RestoredContent,
    UiUx,
    Close,
}

const ENTRIES: [Entry; 5] = [
    Entry::Graphics,
    Entry::BugFixes,
    Entry::RestoredContent,
    Entry::UiUx,
    Entry::Close,
];

/// What the front end does after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    /// The graphics mode was changed: apply and save it.
    SetGraphics(Graphics),
    /// An enhancement group was switched: apply and save them.
    SetEnhancements(Enhancements),
    /// The menu closed.
    Close,
}

/// The values the menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuValues {
    pub graphics: Graphics,
    pub enhancements: Enhancements,
}

const BOX_W: i32 = 360;
const ROW_H: i32 = 24;
const TITLE_H: i32 = 34;
const FOOTER_H: i32 = 50;
const BOX_H: i32 = TITLE_H + ENTRIES.len() as i32 * ROW_H + FOOTER_H;
const BOX_X: i32 = (SCREEN_W as i32 - BOX_W) / 2;
const BOX_Y: i32 = (SCREEN_H as i32 - BOX_H) / 2;

const BG: u16 = 0x18e3;
const BORDER: u16 = 0xbdf7;
const HIGHLIGHT: u16 = 0x3a0c;
const TEXT: u16 = 0xffff;
const DIM: u16 = 0x9cf3;
const SELECTED: u16 = 0xffe0;

/// The menu's state.
#[derive(Debug, Default)]
pub struct OptionsMenu {
    open: bool,
    selected: usize,
}

impl OptionsMenu {
    pub fn new() -> OptionsMenu {
        OptionsMenu::default()
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
        match entry {
            Entry::Graphics => MenuAction::SetGraphics(match v.graphics {
                Graphics::Original => Graphics::Hd,
                Graphics::Hd => Graphics::Original,
            }),
            Entry::BugFixes => {
                let mut x = v.enhancements;
                x.minor_bug_fixes = !x.minor_bug_fixes;
                MenuAction::SetEnhancements(x)
            }
            Entry::RestoredContent => {
                let mut x = v.enhancements;
                x.restored_content = !x.restored_content;
                MenuAction::SetEnhancements(x)
            }
            Entry::UiUx => {
                let mut x = v.enhancements;
                x.ui_ux = !x.ui_ux;
                MenuAction::SetEnhancements(x)
            }
            Entry::Close => {
                self.close();
                MenuAction::Close
            }
        }
    }

    /// One line on what the selected entry does.
    fn description(entry: Entry) -> &'static str {
        match entry {
            Entry::Graphics => "HD: the panorama and the faces at the window's resolution",
            Entry::BugFixes => "Fixes bugs of the original release (on by default)",
            Entry::RestoredContent => "Restores lines the original never plays (off by default)",
            Entry::UiUx => "Modern comforts, e.g. no error box (off by default)",
            Entry::Close => "Back to the game",
        }
    }

    /// A key pressed while the menu is open: Up / Down choose an entry,
    /// Enter / Space (and Left / Right on a setting) change it, Escape and
    /// F10 close the menu.
    pub fn key(&mut self, key: Key, v: MenuValues) -> MenuAction {
        match key {
            Key::Escape | Key::F(10) => {
                self.close();
                MenuAction::Close
            }
            Key::Up => {
                self.selected = (self.selected + ENTRIES.len() - 1) % ENTRIES.len();
                MenuAction::None
            }
            Key::Down => {
                self.selected = (self.selected + 1) % ENTRIES.len();
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

    /// Draws the menu over a 640x480 RGB565 frame.
    pub fn draw(&self, screen: &mut [u16], font: &SubtitleFont, v: MenuValues) {
        let fill = |s: &mut [u16], x: i32, y: i32, w: i32, h: i32, c: u16| {
            for yy in y.max(0)..(y + h).min(SCREEN_H as i32) {
                for xx in x.max(0)..(x + w).min(SCREEN_W as i32) {
                    s[yy as usize * SCREEN_W + xx as usize] = c;
                }
            }
        };
        let text = |s: &mut [u16], t: &str, x: i32, y: i32, c: u16| {
            font.draw(s, SCREEN_W, SCREEN_H, t.as_bytes(), x, y, BOX_W - 20, c);
        };
        let centred = |s: &mut [u16], t: &str, y: i32, c: u16| {
            let w = font.width(t.as_bytes());
            text(s, t, BOX_X + (BOX_W - w) / 2, y, c);
        };
        fill(screen, BOX_X - 1, BOX_Y - 1, BOX_W + 2, BOX_H + 2, BORDER);
        fill(screen, BOX_X, BOX_Y, BOX_W, BOX_H, BG);
        centred(screen, "Atlantis Native options", BOX_Y + 10, TEXT);
        for (i, e) in ENTRIES.iter().enumerate() {
            let y = BOX_Y + TITLE_H + i as i32 * ROW_H;
            let sel = i == self.selected;
            if sel {
                fill(screen, BOX_X + 6, y, BOX_W - 12, ROW_H - 2, HIGHLIGHT);
            }
            let c = if sel { SELECTED } else { TEXT };
            match e {
                Entry::Graphics => {
                    let value = match v.graphics {
                        Graphics::Original => "Original",
                        Graphics::Hd => "HD",
                    };
                    text(screen, "Graphics", BOX_X + 16, y + 4, c);
                    let t = format!("< {value} >");
                    let w = font.width(t.as_bytes());
                    text(screen, &t, BOX_X + BOX_W - 16 - w, y + 4, c);
                }
                Entry::BugFixes | Entry::RestoredContent | Entry::UiUx => {
                    let (label, on) = match e {
                        Entry::BugFixes => ("Fix original bugs", v.enhancements.minor_bug_fixes),
                        Entry::RestoredContent => {
                            ("Restored content", v.enhancements.restored_content)
                        }
                        _ => ("Modern UI/UX", v.enhancements.ui_ux),
                    };
                    text(screen, label, BOX_X + 16, y + 4, c);
                    let t = format!("< {} >", if on { "On" } else { "Off" });
                    let w = font.width(t.as_bytes());
                    text(screen, &t, BOX_X + BOX_W - 16 - w, y + 4, c);
                }
                Entry::Close => centred(screen, "Close", y + 4, c),
            }
        }
        centred(
            screen,
            Self::description(self.selected()),
            BOX_Y + BOX_H - 44,
            TEXT,
        );
        centred(
            screen,
            "Arrows, Enter or click; Escape or F10 closes",
            BOX_Y + BOX_H - 22,
            DIM,
        );
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
    const ORIG: MenuValues = MenuValues {
        graphics: Graphics::Original,
        enhancements: DEFAULTS,
    };
    const HD: MenuValues = MenuValues {
        graphics: Graphics::Hd,
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
        m.pointer(320, gy + 4 * ROW_H);
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
