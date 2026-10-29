//! The inventory (loop mode 7, the right button in the warp) and the
//! object in the hand.
//!
//! Open (@ 0x00427a60): mode 7, the turn speeds zeroed, the hand emptied,
//! the pointer `Interface.spr` #40, alpha 0, the fade timer restarted.
//! Each frame while open (@ 0x00427c40): the warp without its cursor, the
//! items (@ 0x004299b0), the bar `Interface.spr` #0 centred on (W/2,
//! H − 39), the pointer (top-left at the free pointer), then the pick
//! (@ 0x00429b80); every 1/50 s, alpha += 0.2 (1.0 once it reached 1).
//! Closed with alpha > 0, the items and the bar fade out over the warp by
//! −0.2 every 1/50 s. A drawn alpha below 1 is `AFF_SPRTRS` at level
//! `ftol(alpha × 31)`; 0 draws nothing.
//!
//! Layout (@ 0x00429180 by screen width): slots of 80 pixels, the bar 78
//! pixels high; 8 slots at 640, 10 at 800, 12 at 1024 with a left margin
//! of 34 (0 otherwise). Item j (the j-th of the world's entries from the
//! scroll position) is centred on (margin + 80 j + 40, H − 40). The arrows
//! (`fleche.SPR`): #0 at (0, H − 110) when scrolled, #1 at (W − 16, H −
//! 110) when more items follow. The pointer at x = 0 or W − 1 scrolls one
//! slot every 0.25 s.

use atlantis2_engine::script::BagEntry;

/// The slot width (@ 0x00717080) and the bar's height (@ 0x00717084).
pub const SLOT: i32 = 80;
pub const BAR_H: i32 = 78;

/// The world's name in the sprite banks (`<World>Objets.SPR`,
/// `<World>ObjetsEnMain.SPR`).
pub fn world_bank_name(world: i32) -> Option<&'static str> {
    Some(match world {
        1 => "Hoggar",
        2 => "LDM",
        3 => "Bagdad",
        4 => "Paleo",
        5 => "Inter",
        _ => return None,
    })
}

/// The slots and the left margin for a screen width.
pub fn slots(width: usize) -> (i32, i32) {
    match width {
        640 => (8, 0),
        1024 => (12, 34),
        _ => (10, 0),
    }
}

/// The bag's entries shown in world `monde` (6 counts as 2).
pub fn shown(bag: &[BagEntry], monde: i32) -> Vec<BagEntry> {
    let w = if monde == 6 { 2 } else { monde };
    bag.iter().copied().filter(|e| e.world == w).collect()
}

/// The level `AFF_SPRTRS` is given for an alpha (@ 0x00436a26: float 31.0,
/// `ftol`, the low five bits): `None` for alpha ≤ 0 (nothing drawn) and
/// for alpha ≥ 1 (`AFF_SPR`).
pub fn trs_level(alpha: f32) -> Option<Option<u8>> {
    if alpha <= 0.0 {
        None
    } else if alpha >= 1.0 {
        Some(None)
    } else {
        Some(Some(((alpha * 31.0) as i32 & 0x1f) as u8))
    }
}

/// The inventory's state.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    /// Open (mode 7).
    pub open: bool,
    /// The fade's alpha (@ 0x00756568).
    pub alpha: f32,
    /// The fade timer (seconds of the game's clock).
    pub fade_time: f64,
    /// The first entry shown (@ 0x0075859c).
    pub scroll: usize,
    /// The last scroll by the pointer.
    pub scroll_time: f64,
}

impl Inventory {
    /// Whether anything is drawn (open, or fading out).
    pub fn drawn(&self) -> bool {
        self.open || self.alpha > 0.0
    }

    /// The fade's step, at most once per 1/50 s of the clock.
    pub fn fade_step(&mut self, now: f64) {
        if now - self.fade_time < 1.0 / 50.0 {
            return;
        }
        self.fade_time = now;
        if self.open {
            if self.alpha >= 1.0 {
                self.alpha = 1.0;
            } else {
                self.alpha += 0.2;
            }
        } else if self.alpha > 0.0 {
            self.alpha -= 0.2;
        }
    }

    /// The slot under the pointer (@ 0x00429b80): pointer y > H − 78 and
    /// 0 < px − margin < 80 × slots; the entry's index among the shown.
    pub fn slot_at(&self, width: usize, height: usize, pointer: (i32, i32)) -> Option<usize> {
        let (n, margin) = slots(width);
        let (px, py) = pointer;
        let dx = px - margin;
        (py > height as i32 - BAR_H && dx > 0 && dx < SLOT * n)
            .then(|| (dx / SLOT) as usize + self.scroll)
    }

    /// The pointer at an edge scrolls one slot every 0.25 s.
    pub fn scroll_step(&mut self, width: usize, shown: usize, px: i32, now: f64) {
        let (n, _) = slots(width);
        if now - self.scroll_time < 0.25 {
            return;
        }
        if px <= 0 && self.scroll > 0 {
            self.scroll -= 1;
            self.scroll_time = now;
        } else if px >= width as i32 - 1 && self.scroll + (n as usize) < shown {
            self.scroll += 1;
            self.scroll_time = now;
        }
    }
}
