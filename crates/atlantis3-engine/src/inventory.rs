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
use atlantis3_data::spr::{Bank, Target};

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

    /// The fade's step (@ 0x00427c40): when more than 1/50 s passed since
    /// the last one, alpha + 0.2 (1.0 once it reached 1) while open, alpha
    /// − 0.2 while closed and above 0 (f32 sums).
    pub fn fade_step(&mut self, now: f64) {
        if self.open || self.alpha > 0.0 {
            if now - self.fade_time <= 1.0 / 50.0 {
                return;
            }
            self.fade_time = now;
            self.alpha = if !self.open {
                self.alpha - 0.2
            } else if self.alpha >= 1.0 {
                1.0
            } else {
                self.alpha + 0.2
            };
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

    /// The shown entry under the pointer, if any (the hover of
    /// @ 0x00429b80).
    pub fn hovered(
        &self,
        width: usize,
        height: usize,
        pointer: (i32, i32),
        shown: &[BagEntry],
    ) -> Option<BagEntry> {
        self.pick(width, height, pointer, shown)
    }

    /// The pick of a click (@ 0x00429b80): the shown entry under the
    /// pointer.
    pub fn pick(
        &self,
        width: usize,
        height: usize,
        pointer: (i32, i32),
        shown: &[BagEntry],
    ) -> Option<BagEntry> {
        self.slot_at(width, height, pointer)
            .and_then(|i| shown.get(i))
            .copied()
    }

    /// The pointer at an edge scrolls one slot every 0.25 s.
    pub fn scroll_step(&mut self, width: usize, shown: usize, px: i32, now: f64) {
        let (n, _) = slots(width);
        if now - self.scroll_time <= 0.25 {
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

/// The banks the inventory draws with.
pub struct Banks<'a> {
    pub interface: Option<&'a Bank>,
    /// `<World>Objets.SPR`.
    pub objects: Option<&'a Bank>,
    /// `fleche.SPR`.
    pub arrows: Option<&'a Bank>,
}

/// One `AtlSpr_Aff` call (@ 0x004367b0): bank, sprite, top-left x, y (a
/// centred draw already moved by half the sprite's size), alpha, and what
/// it drew: `AFF_SPR` (`None`), `AFF_SPRTRS` at a level, or nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Aff {
    pub bank: &'static str,
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub alpha: f32,
    pub drawn: Option<Option<u8>>,
}

/// `AtlSpr_Aff`: `AFF_SPR` at alpha ≥ 1, `AFF_SPRTRS` at
/// `ftol(alpha × 31) & 31` below, nothing at alpha ≤ 0, on the game's
/// target (clip maxima at the size).
#[allow(clippy::too_many_arguments)]
pub fn aff(
    screen: &mut [u16],
    w: usize,
    h: usize,
    name: &'static str,
    bank: &Bank,
    index: usize,
    x: i32,
    y: i32,
    alpha: f32,
) -> Aff {
    let mut t = Target::screen(screen, w, h);
    t.xmax = w as i32;
    t.ymax = h as i32;
    let drawn = trs_level(alpha);
    match drawn {
        None => {}
        Some(None) => {
            bank.draw(&mut t, Some(x), y, index);
        }
        Some(Some(l)) => {
            bank.draw_trs(&mut t, Some(x), y, index, l);
        }
    }
    Aff {
        bank: name,
        index,
        x,
        y,
        alpha,
        drawn,
    }
}

/// `AtlSpr_AffCentre` (@ 0x00436b80): centred on (x, y) by half the
/// sprite's size.
#[allow(clippy::too_many_arguments)]
pub fn aff_centre(
    screen: &mut [u16],
    w: usize,
    h: usize,
    name: &'static str,
    bank: &Bank,
    index: usize,
    x: i32,
    y: i32,
    alpha: f32,
) -> Option<Aff> {
    // `AtlSpr_AffCentre` draws nothing at alpha 0 or below.
    if alpha <= 0.0 {
        return None;
    }
    let info = bank.info(index)?;
    Some(aff(
        screen,
        w,
        h,
        name,
        bank,
        index,
        x - i32::from(info.w) / 2,
        y - i32::from(info.h) / 2,
        alpha,
    ))
}

/// The items (@ 0x004299b0), the arrows (@ 0x00428fd0) and the bar at
/// `alpha`; the calls made.
#[allow(clippy::too_many_arguments)]
pub fn draw_items(
    screen: &mut [u16],
    w: usize,
    h: usize,
    banks: &Banks,
    items: &[BagEntry],
    scroll: usize,
    alpha: f32,
    objects_name: &'static str,
) -> Vec<Aff> {
    let mut calls = Vec::new();
    let (n, margin) = slots(w);
    let hi = h as i32;
    if let Some(bank) = banks.objects {
        for (j, e) in items.iter().skip(scroll).take(n as usize).enumerate() {
            let x = margin + SLOT * j as i32 + 40;
            calls.extend(aff_centre(
                screen,
                w,
                h,
                objects_name,
                bank,
                usize::from(e.sprite),
                x,
                hi - 40,
                alpha,
            ));
        }
    }
    if let Some(bank) = banks.arrows {
        if scroll > 0 {
            calls.push(aff(screen, w, h, "fleche.SPR", bank, 0, 0, hi - 110, alpha));
        }
        if scroll + (n as usize) < items.len() {
            calls.push(aff(
                screen,
                w,
                h,
                "fleche.SPR",
                bank,
                1,
                w as i32 - 16,
                hi - 110,
                alpha,
            ));
        }
    }
    if let Some(bank) = banks.interface {
        calls.extend(aff_centre(
            screen,
            w,
            h,
            "Interface.spr",
            bank,
            0,
            (w / 2) as i32,
            hi - 39,
            alpha,
        ));
    }
    calls
}

/// The hover label (@ 0x0042a530): the object's `Objects.txt` name in
/// `AtlFont_Default`, white, at (px + 50, py − 20). A first layout
/// (x0, W − x1, y0, H − y1, left, bottom) with x1 = x0 + 20 × length and
/// y1 = y0 + 18 measures the text; its box (line 0's rectangle, extended
/// by the others) is clamped to the screen's right edge only; the text is
/// printed with the layout (x0, W − x0 − x1, y0, H − y1, left, bottom) of
/// that box (the same slip as the tooltips' text). Returns both layouts
/// and the lines printed.
pub fn label(
    font: &crate::text::GdiFont,
    text: &[u8],
    pointer: (i32, i32),
    w: i32,
    h: i32,
) -> (crate::text::Zone, crate::text::Zone, Vec<crate::text::Line>) {
    use crate::text::{layout, Zone};
    let (px, py) = pointer;
    let (x0, y0) = (px + 50, py - 20);
    let (x1, y1) = (x0 + 20 * text.len() as i32, y0 + 18);
    let first = Zone::new(x0, w - x1, y0, h - y1, 0, 1);
    let lines = layout(font, text, w, h, first);
    let th = font.tm_height;
    let mut bx = (x0, y0, x0, y0);
    for (j, l) in lines.iter().enumerate() {
        let r = (l.x, l.y, l.x + l.dx.iter().sum::<i32>(), l.y + th);
        bx = if j == 0 {
            r
        } else {
            (bx.0.min(r.0), bx.1.min(r.1), bx.2.max(r.2), bx.3.max(r.3))
        };
    }
    if bx.2 > w {
        bx.0 += w - bx.2;
        bx.2 = w;
    }
    let second = Zone::new(bx.0, w - bx.0 - bx.2, bx.1, h - bx.3, 0, 1);
    let printed = layout(font, text, w, h, second);
    (first, second, printed)
}
