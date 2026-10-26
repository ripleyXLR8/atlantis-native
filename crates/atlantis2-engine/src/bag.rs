//! The bag bar (`AtlInventaire`, @ 0x00474c30 each panorama frame while it
//! is open), the overlays some objects open when they are picked from it
//! (@ 0x0047ec50: the forms, the fan, the ring, the space-travel balls)
//! and the dowsing rod (@ 0x0049c6e0): their state and the rules that do
//! not touch the rest of the game. The engine runs them in its warp frame
//! (`engine/ui.rs`).
//!
//! The bag itself (the 90 entries @ 0x00671a10, their count
//! @ 0x00671a08) is the script interpreter's ([`crate::script::BagEntry`]):
//! `/set(sacados=X)`, the take of an object sprite and the puzzles fill
//! it. The bar shows only the entries of the current world (`MONDE`, 6
//! counted as 2), eight at a time on a 640-pixel screen (ten wider), one
//! every 80 pixels.

use atlantis2_data::spr::SprBank;

use crate::fpu::Fpu;
use crate::script::BagEntry;

/// The sprite byte of an entry that holds nothing (the table's init
/// pattern @ 0x00474810, and what a removal writes past the last entry):
/// such an entry is never shown.
pub const NO_SPRITE: u8 = 99;

/// The bar's scroll delay in seconds: a float (@ 0x005c6320, read by
/// `fcomp dword` @ 0x004752fd).
pub const SCROLL_DELAY: f32 = 0.1;

/// Whether entry `e` is shown in world `monde` (FUN_004754d0 →
/// FUN_00475490, and the sprite test of FUN_00475870 / FUN_004757e0).
pub fn shown_in(e: &BagEntry, monde: i32) -> bool {
    e.sprite != NO_SPRITE && e.world == if monde == 6 { 2 } else { monde }
}

/// The number of entries shown in world `monde` (FUN_00475870).
pub fn visible_count(bag: &[BagEntry], monde: i32) -> i32 {
    bag.iter().filter(|e| shown_in(e, monde)).count() as i32
}

/// The next entry after `from` shown in world `monde`, −1 when none
/// (FUN_004757e0; −1 starts from the first entry).
pub fn next_shown(bag: &[BagEntry], from: i32, monde: i32) -> i32 {
    let start = usize::try_from(from + 1).unwrap_or(0);
    bag.iter()
        .enumerate()
        .skip(start)
        .find(|(_, e)| shown_in(e, monde))
        .map_or(-1, |(i, _)| i as i32)
}

/// What the bar draws, in its order, and what the overlays draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiDraw {
    /// `AtlSpr_Aff` (@ 0x0047bc40) mode 0: the sprite with its hot spot at
    /// (x, y).
    Sprite {
        bank: UiBank,
        frame: i32,
        x: i32,
        y: i32,
    },
    /// `AtlSpr_Aff` mode 1 (`AFF_SPRTRS50`): the sprite at 50 % over the
    /// screen, its hot spot at (x, y).
    Translucent {
        bank: UiBank,
        frame: i32,
        x: i32,
        y: i32,
    },
    /// One line of text in the popup font (`AtlFT_POPUP`), its cell's
    /// top-left corner at (x, y), colour 0xRRGGBB.
    Text {
        x: i32,
        y: i32,
        colour: u32,
        bytes: Vec<u8>,
    },
}

/// The banks the bar, the overlays and the subject menu draw from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiBank {
    /// `spr\interfac.spr` (@ 0x006719fc): frame 0 the bar, 40 the pointer.
    Interface,
    /// The world's objects bank (@ 0x00671ef4).
    Objects,
    /// `spr\baton.spr` (@ 0x006d84e8).
    Rod,
    /// `spr\form.spr` (@ 0x006c2e58): the forms and the fan.
    Form,
    /// `spr\anneau.spr` (@ 0x006c2e5c).
    Ring,
    /// The space-travel ball's bank (@ 0x006c2e60), named by the object's
    /// second `defnames.con` parameter.
    Ball,
    /// The world's subjects bank (`spr\suj*.spr`, @ 0x006cfb5c): frame 0
    /// the ring of a cell, the others the subjects.
    Subjects,
    /// The menus' pointer (`spr\Menu\curseur.spr` @ 0x006d1744), when the
    /// cursor pair still holds it after a menu.
    Menu,
}

/// The bar's layout and scroll (@ 0x00671ee0..0x00671ef0).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    /// Open (@ 0x00671ef0): a right click in the panorama toggles it.
    pub open: bool,
    /// The first shown entry, counted among the entries of the world
    /// (@ 0x00671ee4; 0 at start, never reset).
    pub first: i32,
    /// The entries of the world (@ 0x00671ee8), counted at each take and
    /// each frame of the open bar.
    pub visible: i32,
    /// The cells (@ 0x00671eec): 0 until the bar is first drawn, then 8
    /// on a 640-pixel screen, 10 on a wider one.
    pub slots: i32,
    /// The entries drawn by the last frame of the bar, cell by cell
    /// (@ 0x00671e48; their count @ 0x00671ee0).
    pub shown: Vec<usize>,
    /// The clock of the last scroll (@ 0x00671ec0, a float).
    scrolled_at: f32,
}

impl Bar {
    /// FUN_00475740, at the start of each frame of the bar: the cells for
    /// the screen width, the entries of the world, and the first one
    /// brought back so that the last cell is used.
    pub fn layout(&mut self, width: usize, visible: i32) {
        self.slots = if width == 640 { 8 } else { 10 };
        self.visible = visible;
        if self.visible <= self.slots {
            self.first = 0;
        } else if self.visible < self.first + self.slots {
            self.first = self.visible - self.slots;
        }
    }

    /// FUN_004757b0: one entry to the right, while the last cell is not
    /// the last entry.
    pub fn scroll_right(&mut self) -> bool {
        if self.slots < self.visible && self.first < self.visible - self.slots {
            self.first += 1;
            return true;
        }
        false
    }

    /// FUN_00475790: one entry to the left.
    pub fn scroll_left(&mut self) -> bool {
        if self.first > 0 {
            self.first -= 1;
            return true;
        }
        false
    }

    /// The end of a take (FUN_004754f0): the entries of the world counted,
    /// then the bar scrolled to its end. Before the bar was ever drawn its
    /// cell count is 0, so the first shown entry goes past the last one
    /// until the first frame of the bar brings it back.
    pub fn taken(&mut self, visible: i32) {
        self.visible = visible;
        while self.scroll_right() {}
    }

    /// FUN_004752f0: true, and the time kept (`fstp dword`), when the
    /// time since the last scroll (the scheduler's clock, FUN_0040ca90,
    /// minus the kept float) is more than the float 0.1 @ 0x005c6320.
    pub fn scroll_due(&mut self, fpu: Fpu, now: f64) -> bool {
        if fpu.sub(now, f64::from(self.scrolled_at)) > f64::from(SCROLL_DELAY) {
            self.scrolled_at = fpu.f32(now);
            return true;
        }
        false
    }

    /// FUN_00475830: the entry in the first cell, −1 when none.
    pub fn first_entry(&self, bag: &[BagEntry], monde: i32) -> i32 {
        let mut i = next_shown(bag, -1, monde);
        let mut k = 0;
        while k < self.first {
            if i == -1 {
                return -1;
            }
            i = next_shown(bag, i, monde);
            k += 1;
        }
        i
    }

    /// FUN_004752a0: the cell a click picks: below H − 80, cell x / 80 of
    /// those drawn (cells of 80 pixels from the left edge, not the hover
    /// boxes).
    pub fn pick(&self, click: bool, pointer: (i32, i32), height: i32) -> Option<usize> {
        let cell = pointer.0 / 80;
        (click && pointer.1 > height - 80 && cell < self.shown.len() as i32)
            .then_some(cell as usize)
    }
}

/// The frames of `form.spr` for the stamps and symbols (@ 0x005df744):
/// a value v is drawn as frame `FORM_FRAMES[v]`; the fan's solution rows
/// (@ 0x005df754, three values per row) are the same array from index 4,
/// row r at 4 + 3 r.
pub const FORM_FRAMES: [i32; 37] = [
    0, 4, 3, 2, 1, 6, 5, 1, 4, 6, 2, 5, 3, 3, 1, 4, 4, 2, 1, 5, 1, 6, 6, 3, 4, 1, 3, 5, 2, 6, 2, 3,
    6, 1, 4, 1, 5,
];

/// The overlays of FUN_0047ec50 (flags @ 0x006c2e54..0x006c2e57), opened
/// by picking their object from the bag and closed by a click.
#[derive(Default)]
pub struct Overlays {
    /// A form (`FORMULAIRE_VIDE`, `_1`..`_3`), the fan (`EVENTAIL`), the
    /// ring (`ANNEAU_GRAVE`), a space-travel ball (`BOULE_SHAMAN*`).
    pub form: bool,
    pub fan: bool,
    pub ring: bool,
    pub ball: bool,
    /// The full-screen flag (@ 0x00671efc): set by the form, fan and ring
    /// (the mouse no longer turns the view), not by the ball.
    pub cover: bool,
    /// Which form (@ 0x005df7d8: 0 empty, 1..3 the stamped ones; −1 at
    /// start).
    pub form_number: i32,
    /// The screen centre when it opened (@ 0x006c2e50 / 0x006c2e0c).
    pub centre: (i32, i32),
    pub form_bank: Option<SprBank>,
    pub ring_bank: Option<SprBank>,
    pub ball_bank: Option<SprBank>,
    /// The ball's zone (@ 0x006c2e14, the object's third parameter), its
    /// bank's frame count (@ 0x006c2e38) and the frame shown
    /// (@ 0x006c2e48).
    pub ball_zone: Vec<u8>,
    pub ball_frames: i32,
    pub ball_frame: i32,
    /// What the opening put aside and a click gives back: `OBJETENMAIN`
    /// (@ 0x006c2e44), the cursor's frame (@ 0x006c2e10), the hand's frame
    /// (@ 0x006c2e4c) and whether its bank was the objects bank
    /// (@ 0x006c2e64).
    pub saved_objetenmain: i32,
    pub saved_cursor_frame: i32,
    pub saved_hand_frame: i32,
    pub saved_hand_objects: bool,
    /// Whether that bank was the counting grids' marbles.
    pub saved_hand_puzzle: bool,
}

impl Overlays {
    pub fn new() -> Overlays {
        Overlays {
            form_number: -1,
            ..Overlays::default()
        }
    }

    /// FUN_0047ec20: one of the four is open.
    pub fn any(&self) -> bool {
        self.form || self.fan || self.ring || self.ball
    }
}

/// The dowsing rod (`BATON_SOURCE`, @ 0x006d84e8..0x006d8504): picked from
/// the bag it shows `baton.spr` above the bottom of the screen, swinging
/// between frame 0 and `NEARW77` − 1, one frame per panorama frame.
pub struct Rod {
    /// Shown (@ 0x006d84fd).
    pub active: bool,
    pub bank: Option<SprBank>,
    /// The frame drawn next (@ 0x006d8500, 0 at start) and its step
    /// (@ 0x005e2efc, 1 at start); neither is reset when the rod is taken
    /// again.
    pub frame: i32,
    pub step: i32,
}

impl Default for Rod {
    fn default() -> Rod {
        Rod {
            active: false,
            bank: None,
            frame: 0,
            step: 1,
        }
    }
}

/// The text up to the first delimiter of a `defnames.con` parameter
/// (`DefName_CopyParam` @ 0x004108c0: CR, LF, NUL, space, comma, tab, `/`,
/// `)`).
pub fn param_word(text: &[u8]) -> &[u8] {
    let end = text
        .iter()
        .position(|b| matches!(b, b'\r' | b'\n' | 0 | b' ' | b',' | b'\t' | b'/' | b')'))
        .unwrap_or(text.len());
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i32, world: i32) -> BagEntry {
        BagEntry {
            sprite: id as u8,
            id,
            world,
        }
    }

    #[test]
    fn layout_scroll_and_pick() {
        // Ten entries of world 5, two of world 1, one removed.
        let mut bag: Vec<BagEntry> = (1..=10).map(|i| entry(i, 5)).collect();
        bag.insert(3, entry(20, 1));
        bag.push(entry(21, 1));
        bag.push(BagEntry {
            sprite: NO_SPRITE,
            id: 22,
            world: 5,
        });
        assert_eq!(visible_count(&bag, 5), 10);
        assert_eq!(visible_count(&bag, 1), 2);
        let mut bar = Bar::default();
        // Before the first frame: no cells, the first entry past the end.
        bar.taken(10);
        assert_eq!((bar.first, bar.slots), (10, 0));
        bar.layout(640, 10);
        assert_eq!((bar.first, bar.slots), (2, 8));
        assert_eq!(bar.first_entry(&bag, 5), 2);
        assert!(!bar.scroll_right());
        assert!(bar.scroll_left());
        assert_eq!(bar.first_entry(&bag, 5), 1);
        // Entry 3 is world 1's: the third cell of world 5 is entry 4.
        assert_eq!(next_shown(&bag, 2, 5), 4);
        bar.layout(800, 10);
        assert_eq!((bar.first, bar.slots), (0, 10));
        // World 6 shows world 2's entries.
        assert_eq!(visible_count(&[entry(1, 2)], 6), 1);
        bar.shown = vec![0, 1, 2];
        assert_eq!(bar.pick(true, (239, 401), 480), Some(2));
        assert_eq!(bar.pick(true, (240, 401), 480), None);
        assert_eq!(bar.pick(true, (100, 400), 480), None);
        assert_eq!(bar.pick(false, (100, 401), 480), None);
    }

    #[test]
    fn scroll_waits_a_tenth_of_a_second() {
        let mut bar = Bar::default();
        let fpu = Fpu::default();
        assert!(bar.scroll_due(fpu, 0.5));
        assert!(!bar.scroll_due(fpu, 0.55));
        // The delay is the float 0.1, a little more than the double.
        assert!(!bar.scroll_due(fpu, 0.6000000001));
        assert!(bar.scroll_due(fpu, 0.6000000015));
    }

    #[test]
    fn parameter_words() {
        assert_eq!(
            param_word(b"snlfa1004004.spr,\tZone //rem"),
            b"snlfa1004004.spr"
        );
        assert_eq!(param_word(b"ZoneBoule_TI //rem tibet\r\n"), b"ZoneBoule_TI");
    }
}
