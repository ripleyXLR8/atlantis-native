//! The inventory bar: OBJETS0.SPR with the objects' icons, sliding up from
//! the bottom of the screen; the object picked up shown in it; the bow.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`toolbar.cpp`,
//! `navigation.cpp` bow mode; GPLv3, the ScummVM team), which follows
//! `atlantis.exe`: `FUN_0042c098` (a bar session and its click),
//! `FUN_0042c2c4` / `FUN_0042c59c` (the slides), `FUN_0042c8ac` (the bar
//! loop), `FUN_0042cc98` (the object picked up), `FUN_0042d098` /
//! `FUN_0042d198` / `FUN_0042d368` (bow mode), `ArcMagique=`'s animation.

use atlantis_data::sprite::{limits, read_spr_file, SprSprite};

use super::{draw_sprite, draw_sprite_below, object_icon};
use crate::engine::Engine;
use crate::enhancements::BugFix;
use crate::hooks::ScriptHooks;
use crate::warp::SCREEN_H;

/// The visible cells of the bar.
pub const BAR_CELLS: usize = 9;
/// The bar's top row on the screen (82 rows).
pub const BAR_TOP: usize = SCREEN_H - 82;
/// The bar sprite's hotspot y when up (`FUN_0042c8ac` @ 0x0042c96e).
pub const BAR_HOTSPOT_Y: i32 = 443;
/// The port's pace of the edge scroll (the original's was the machine's).
const SCROLL_MS: u64 = 110;
/// `FUN_0042c59c` @ 0x0042c643: the slot y (0x1fa) from which a record of
/// the slot table is no longer drawn.
const SLIDE_OUT_GATE_Y: i32 = 0x1fa;

/// `FUN_0042c098` @ 0x0042c0ff: the record a left click takes, first +
/// x / 70, when x / 70 + 1 <= first + 9 and y >= 409.
pub fn clicked_record(x: i32, y: i32, first: usize) -> Option<usize> {
    let cell = x / 70;
    (cell >= 0 && cell < first as i32 + 9 && y >= 409).then(|| cell as usize + first)
}

/// `FUN_0042c8ac` @ 0x0042cb83: +1 for x > 630 with y > 400, more than 9
/// records and first < count - 7; -1 for x < 5 with y > 400 and first > 0.
pub fn scroll_step(x: i32, y: i32, count: usize, first: usize) -> i32 {
    if y <= 400 {
        0
    } else if x > 630 && count > 9 && first + 7 < count {
        1
    } else if x < 5 && first > 0 {
        -1
    } else {
        0
    }
}

/// `FUN_0042c59c` @ 0x0042c643: on a slide-out frame drawn at slot y
/// `slot_y`, record `record` is drawn when the int32 at 0x00611788 + 4 *
/// record is below 0x1fa. That is the slot y itself for a record below 9;
/// for a record from 9 on it lies past the 9-entry slot table, where the
/// only store is `FUN_004278a0`'s 0x1bb (@ 0x00427af0) and .bss holds 0,
/// so such a record is drawn on every frame (1..4 rows of its icon at
/// 506). `fix` (`BugFix::BarSlideOut`) gates every record on the slot y.
pub fn slide_out_record_drawn(record: usize, slot_y: i32, fix: bool) -> bool {
    if record < BAR_CELLS || fix {
        slot_y < SLIDE_OUT_GATE_Y
    } else {
        true
    }
}

/// `FUN_004278a0` @ 0x00427acf..0x00427ae2: after its slide-in has set the
/// slot x table to 70 s + 42 (`FUN_0042c2c4` @ 0x0042c30f), a pick-up
/// stores slotX[c] = c * 0x46 + 0x23 for the new record c, 7 px left of
/// the other slots; the hold loop and the slide-out read it
/// (@ 0x0042ce3c, 0x0042c6d9) until the next slide-in.
pub fn pickup_slot_x(slot: usize) -> i32 {
    slot as i32 * 0x46 + 0x23
}

/// The slot a pick-up of record `record` (the new record's index) moves:
/// slotX[record], one of the 9 slots the loops read when below 9.
pub fn pickup_slot(record: usize) -> Option<usize> {
    (record < BAR_CELLS).then_some(record)
}

/// The x of visible slot `slot`: 70 s + 42 (`FUN_0042c2c4` @ 0x0042c30f),
/// or [`pickup_slot_x`] for the pick-up's slot (`fix`,
/// `BugFix::PickupIcon`, keeps it at 70 s + 42).
pub fn slot_x(slot: usize, pickup: Option<usize>, fix: bool) -> i32 {
    if pickup == Some(slot) && !fix {
        pickup_slot_x(slot)
    } else {
        42 + 70 * slot as i32
    }
}

/// The inventory as a bar frame draws it.
#[derive(Clone, Copy, Debug)]
pub struct BarState<'a> {
    /// The records' object ids (0x006ce138).
    pub records: &'a [i32],
    /// The first record shown (0x00496924).
    pub first: usize,
    /// OBJETENMAIN: its records get no icon.
    pub held: i32,
    /// The slot whose x the last pick-up set ([`pickup_slot_x`]).
    pub pickup_slot: Option<usize>,
    /// `BugFix::BarSlideOut`: every slide-out record gated on the slot y.
    pub slide_out_fix: bool,
    /// `BugFix::PickupIcon`: the pick-up's slot at 70 s + 42.
    pub pickup_fix: bool,
}

/// An icon of a bar frame: the record, its visible slot and the hotspot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarIcon {
    pub record: usize,
    pub slot: usize,
    pub x: i32,
    pub y: i32,
}

/// What a frame of the bar loops draws with the slots at y `slot_y`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BarFrame {
    /// OBJETS0 sprite 0 at (0, slot y).
    pub background: bool,
    /// The icons, in drawing order.
    pub icons: Vec<BarIcon>,
}

/// The bar of a frame of `FUN_0042c2c4`, `FUN_0042c8ac` or `FUN_0042cc98`
/// (`slide_out` false: the background and every shown record), or of
/// `FUN_0042c59c` (each record gated by [`slide_out_record_drawn`], the
/// background with slot 0 under record first's gate, @ 0x0042c643..
/// 0x0042c674). A shown record with no entry, or whose id is OBJETENMAIN,
/// gets no icon (@ 0x0042c682..0x0042c6b1).
pub fn bar_frame_layout(st: &BarState, slot_y: i32, slide_out: bool) -> BarFrame {
    let drawn = |r: usize| !slide_out || slide_out_record_drawn(r, slot_y, st.slide_out_fix);
    let mut f = BarFrame {
        background: drawn(st.first),
        icons: Vec::new(),
    };
    for slot in 0..BAR_CELLS {
        let record = st.first + slot;
        if !drawn(record) {
            continue;
        }
        let Some(&id) = st.records.get(record) else {
            continue;
        };
        if id == st.held && st.held != 0 {
            continue;
        }
        f.icons.push(BarIcon {
            record,
            slot,
            x: slot_x(slot, st.pickup_slot, st.pickup_fix),
            y: slot_y,
        });
    }
    f
}

/// Draws a bar frame: the background (`bar`), then each icon (`icon` gives
/// a record id's OBJETS1 sprite), with `FUN_00410430`'s clipping.
pub fn draw_bar_frame<'a>(
    screen: &mut [u16],
    frame: &BarFrame,
    st: &BarState,
    slot_y: i32,
    bar: Option<&SprSprite>,
    icon: impl Fn(i32) -> Option<&'a SprSprite>,
) {
    if frame.background {
        if let Some(bar) = bar {
            draw_sprite_below(screen, bar, 0, slot_y, BAR_TOP);
        }
    }
    for i in &frame.icons {
        if let Some(spr) = st.records.get(i.record).and_then(|&id| icon(id)) {
            draw_sprite_below(screen, spr, i.x, i.y, BAR_TOP);
        }
    }
}

impl Engine {
    /// The inventory as the bar draws it.
    pub fn bar_state(&self) -> BarState<'_> {
        BarState {
            records: &self.inventory,
            first: self.bar_first,
            held: self.vars.get("objetenmain"),
            pickup_slot: self.bar_pickup_slot,
            slide_out_fix: self.enhancements().fix(BugFix::BarSlideOut),
            pickup_fix: self.enhancements().fix(BugFix::PickupIcon),
        }
    }

    /// A frame of the bar loops with the slots at y `slot_y`: the view
    /// rendered anew (`FUN_00451d8b`), the bar ([`bar_frame_layout`];
    /// `slide_out` for `FUN_0042c59c`), then the guests' portraits
    /// (`FUN_0042bc58`, after the bar: `FUN_0042c2c4` @ 0x0042c4a9,
    /// `FUN_0042c59c` @ 0x0042c702), the pointer on top.
    fn bar_frame(&mut self, slot_y: i32, slide_out: bool) {
        if self.warp_loaded {
            self.render_view();
        }
        let table = &self.game.exe.inventory_object_ids;
        let mut s = self.sys.screen.borrow().clone();
        let st = self.bar_state();
        let frame = bar_frame_layout(&st, slot_y, slide_out);
        draw_bar_frame(&mut s, &frame, &st, slot_y, self.ui.bar.as_ref(), |id| {
            object_icon(table, id).and_then(|k| self.ui.object(k))
        });
        self.draw_guests(&mut s, None);
        self.draw_pointer(&mut s);
        *self.sys.screen.borrow_mut() = s;
    }

    /// One frame of the bar loops' engine work: the place sprites, the CON
    /// timers; true when the bar has to close (GameOver, Exit, quit).
    async fn bar_tick(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
        self.animate_sprites();
        hooks.bar_timers(self).await;
        self.should_abort() || self.vars.get("gameover") != 0 || self.vars.get("exit") != 0
    }

    /// `FUN_00432178` in the bar loops: the NewSound poll `FUN_00432a64`,
    /// then the ambient sounds.
    async fn bar_sounds(&mut self) {
        self.poll_new_sound().await;
        self.ambient_frame();
    }

    /// `FUN_0042c2c4`: the slots from y 499 up 7 a frame to 443 (8 frames),
    /// the portraits up with them (-7 before the frame, @ 0x0042c4a2). Each
    /// frame: the place sprites (`FUN_00425304` @ 0x0042c327), the view,
    /// the bar, the guests; the timers (`FUN_0042be3c`); then, unless
    /// GameOver or Exit ended the slide (@ 0x0042c55d), `FUN_00432178`
    /// (@ 0x0042c569). True when a timer closed the bar.
    async fn bar_slide_in(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
        // slotX[s] = 70 s + 42 (@ 0x0042c30f): a pick-up's slot x is undone.
        self.bar_pickup_slot = None;
        let mut slot_y = 499 - 7;
        while slot_y >= BAR_HOTSPOT_Y && !self.should_abort() {
            self.slide_guest_portraits(-7);
            self.animate_sprites();
            self.bar_frame(slot_y, false);
            self.sys.frame().await;
            if self.bar_tick(hooks).await {
                return true;
            }
            self.bar_sounds().await;
            slot_y -= 7;
        }
        false
    }

    /// `FUN_0042c59c`: the slots from 443 down 7 a frame until 0x208 (11
    /// frames), each record gated at 0x0042c643 ([`slide_out_record_drawn`]);
    /// the portraits back down to 350 (+7 after the frame, @ 0x0042c7b4).
    /// Each frame: the place sprites (@ 0x0042c5b4), the view, the bar, the
    /// guests; the timers; then, unless GameOver or Exit ended the slide
    /// (@ 0x0042c805), `FUN_00432178` (@ 0x0042c811).
    async fn bar_slide_out(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
        let mut slot_y = BAR_HOTSPOT_Y;
        while slot_y < 0x208 && !self.should_abort() {
            self.animate_sprites();
            self.bar_frame(slot_y, true);
            self.sys.frame().await;
            self.slide_guest_portraits(7);
            if self.bar_tick(hooks).await {
                return true;
            }
            self.bar_sounds().await;
            slot_y += 7;
        }
        false
    }

    /// The edge scroll of the bar loops, at the port's pace.
    fn bar_scroll(&mut self, next_ms: &mut u64) {
        let (mx, my) = self.sys.mouse();
        let now = self.sys.millis();
        if now < *next_ms {
            return;
        }
        let step = scroll_step(mx, my, self.inventory.len(), self.bar_first);
        if step != 0 {
            self.bar_first = (self.bar_first as i32 + step).max(0) as usize;
            *next_ms = now + SCROLL_MS;
        }
    }

    /// A bar session (`FUN_0042c098`): the slide-in, the bar loop
    /// `FUN_0042c8ac` (the timers run every frame; it ends on a left or a
    /// right press, GameOver / Exit, or when a timer section cleared the
    /// request), the item click, the slide-out. Returns the guest whose
    /// portrait was under the pointer at the end (other than 592), whom the
    /// caller talks to; 0 for none.
    pub async fn display_bar(&mut self, hooks: &mut dyn ScriptHooks) -> i32 {
        self.sys.set_mouse_locked(false);
        let closed = self.bar_slide_in(hooks).await;
        if self.should_abort() {
            return 0;
        }
        let mut guest = 0;
        if !closed {
            self.wait_buttons_up().await;
            while self.sys.poll_event().is_some() {}
            guest = self.bar_loop(hooks).await;
        }
        if self.should_abort() {
            return 0;
        }
        // FUN_0042c098 slides the bar out on every exit without GameOver
        // (0x0042c219). GameOver set by the slide-in (tested @ 0x0042c0c9)
        // or by the bar loop (@ 0x0042c0ed) skips to its tail with no
        // slide-out, except that the bar loop's exit with a guest's
        // portrait under the pointer runs FUN_0042c59c itself (0x0042cbfe,
        // before the conversation).
        if self.vars.get("gameover") == 0 || guest != 0 {
            self.bar_slide_out(hooks).await;
        }
        guest
    }

    async fn bar_loop(&mut self, hooks: &mut dyn ScriptHooks) -> i32 {
        let mut next_scroll = 0u64;
        let (mut left, mut game_over, mut section_closed) = (false, false, false);
        self.bar.opened(); // 0x0042c8c4
        loop {
            if self.should_abort() {
                return 0;
            }
            let mi = self.poll_menu_input();
            if mi.left_press {
                left = true;
                break;
            }
            if mi.right_press {
                break; // the right button empties the hand below
            }
            if self.bar_tick(hooks).await {
                game_over = true;
                break;
            }
            // A timer section that played a video, a voice-only line or a
            // music change (or saw a right press) cleared the request
            // (0x0042cb76).
            if !self.bar.request {
                section_closed = true;
                break;
            }
            self.bar_scroll(&mut next_scroll);
            self.bar_frame(BAR_HOTSPOT_Y, false);
            // FUN_00432178 ends each bar frame (FUN_0042c8ac step 5).
            self.bar_sounds().await;
            self.sys.frame().await;
        }
        // FUN_0042c098 step 6 clears both flags whatever the exit.
        let session = self.bar;
        self.bar.flags_reset();
        // The exit test of FUN_0042c8ac (0x0042cbfe): the portrait under
        // the pointer, other than 592.
        let (mx, my) = self.sys.mouse();
        let guest = match self.guest_at(mx, my) {
            592 => 0,
            g => g,
        };
        self.wait_buttons_up().await;
        if game_over || guest != 0 {
            return guest;
        }
        // Left by a timer section: no item click; the hand is emptied only
        // by a right press seen during it (0x0042c1c9).
        if section_closed && !session.right_press {
            return 0;
        }
        let (mx, my) = self.sys.mouse();
        let held = self.vars.get("objetenmain");
        // FUN_0042c098 compares ids; a cell past the last record reads 0,
        // which empties the hand.
        let taken = left
            .then(|| clicked_record(mx, my, self.bar_first))
            .flatten()
            .and_then(|r| self.inventory.get(r).copied())
            .filter(|&id| id != 0 && id != held);
        self.vars.set("objetenmain", taken.unwrap_or(0));
        0
    }

    /// `FUN_004278a0` after a pick-up: the bar slides in, `FUN_0042cc98`
    /// shows the new object in hand until a left or a right press, the bar
    /// slides out. True for a right press (the object stays in hand).
    pub async fn display_pickup(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
        self.sys.set_mouse_locked(false);
        self.bar_slide_in(hooks).await;
        // The new record's slot x = index * 0x46 + 0x23, stored after the
        // slide-in (@ 0x00427ae2); a record from 9 on lands past the slots.
        let held = self.vars.get("objetenmain");
        self.bar_pickup_slot = match self.inventory.len() {
            n if n > 0 && self.inventory[n - 1] == held => pickup_slot(n - 1),
            _ => None,
        };
        let mut right = false;
        let mut next_scroll = 0u64;
        // The loop ends on a press edge of either button.
        while self.sys.poll_event().is_some() {}
        while !self.should_abort() {
            // FUN_0042cc98 animates the place sprites (@ 0x0042cd1c) and
            // draws like the bar loop; it runs no timers.
            self.animate_sprites();
            self.bar_frame(BAR_HOTSPOT_Y, false);
            self.sys.frame().await;
            let mi = self.poll_menu_input();
            match self
                .hold_pass(mi.left_press, mi.right_press, &mut next_scroll)
                .await
            {
                0 => {}
                edge => {
                    right = edge == 2;
                    break;
                }
            }
        }
        self.wait_buttons_up().await;
        if !right {
            self.vars.set("objetenmain", 0);
        }
        self.bar_slide_out(hooks).await;
        // The timers resume from their values, ChapterTime from 0, as after
        // the inventory bar (FUN_0042c59c @ 0x0042c844 ends the slide-out).
        self.rebase_con_clocks();
        right
    }

    /// One pass of the hold loop `FUN_0042cc98` after its frame, with the
    /// press edges the input `FUN_0041a298` read (@ 0x0042cf14):
    /// `FUN_00432178` (@ 0x0042cf19), the edge scroll (@ 0x0042cf1e..
    /// 0x0042cf8f, at the port's pace), then the loop test on the edges
    /// (@ 0x0042cd05): 1 for a left press, 2 for a right one, else 0. The
    /// pass that reads the click runs the sounds and the scroll too.
    async fn hold_pass(&mut self, left: bool, right: bool, next_scroll: &mut u64) -> i32 {
        self.bar_sounds().await;
        self.bar_scroll(next_scroll);
        if left {
            1
        } else if right {
            2
        } else {
            0
        }
    }

    /// `FUN_0042d098`, closing the bar: the bow (object 334) in hand with
    /// ArcMagique set, not yet in bow mode: `SPRITE\2D\ARCWARP.SPR`, bow
    /// mode, the hand emptied; then the bow rises (`FUN_0042d198`: frame 0
    /// from y 0x1e1 to 0x83, 10 px a frame).
    pub(crate) async fn enter_bow_mode(&mut self) {
        if self.vars.get("objetenmain") != 0x14e
            || self.vars.get("arcmagique") == 0
            || self.bow_mode
        {
            return;
        }
        let (w, h) = limits::DEFAULT;
        self.bow_sprites = match self.read("SPRITE\\2D\\ARCWARP.SPR").await {
            Some(d) => read_spr_file(&d, w, h).unwrap_or_default(),
            None => Vec::new(),
        };
        if self.bow_sprites.is_empty() {
            return;
        }
        self.bow_mode = true;
        self.vars.set("objetenmain", 0);
        let mut y = 0x1e1;
        while y != 0x79 && !self.should_abort() {
            self.show_bow_at(y).await;
            y -= 10;
        }
    }

    /// `FUN_0042d368`: frame 0 from y 0x79 down to 0x1d7, then bow mode
    /// ends and the sprite is freed.
    pub(crate) async fn lower_bow(&mut self) {
        let mut y = 0x79;
        while y != 0x1e1 && !self.should_abort() {
            self.show_bow_at(y).await;
            y += 10;
        }
        self.bow_mode = false;
        self.bow_sprites.clear();
    }

    async fn show_bow_at(&mut self, y: i32) {
        self.animate_sprites();
        if !self.warp_loaded {
            return;
        }
        self.render_view();
        if let Some(spr) = self.bow_sprites.first() {
            let mut s = self.sys.screen.borrow_mut();
            draw_sprite(&mut s, spr, 0x2e, y);
        }
        self.sys.frame().await;
        while self.sys.poll_event().is_some() {}
    }

    /// ArcMagique=N (`FUN_0042a100` @ 0x0042aa21..0x0042ac2b):
    /// `SPRITE\2D\ARCANIM.SPR` frames 0..19 at the mouse over the view, the
    /// place sprites animating; nothing without the file.
    pub(crate) async fn play_arc_magique(&mut self) {
        let (w, h) = limits::DEFAULT;
        let anim = match self.read("SPRITE\\2D\\ARCANIM.SPR").await {
            Some(d) => read_spr_file(&d, w, h).unwrap_or_default(),
            None => return,
        };
        if anim.is_empty() {
            return;
        }
        let (mx, my) = self.sys.mouse();
        for i in 0..20 {
            if self.should_abort() || !self.warp_loaded {
                break;
            }
            self.animate_sprites();
            self.render_view();
            if let Some(spr) = anim.get(i) {
                let mut s = self.sys.screen.borrow_mut();
                draw_sprite(&mut s, spr, mx, my);
            }
            self.sys.frame().await;
            while self.sys.poll_event().is_some() {}
        }
    }

    /// The guests' portraits sliding in from the right edge (`FUN_0042b480`)
    /// or out to it (`FUN_0042b6c0`), one redrawn view per 10 px.
    pub(crate) async fn slide_guest(&mut self, sprite: u8, from: i32, to: i32, step: i32) {
        if !self.warp_loaded {
            return;
        }
        let mut x = from;
        while if step < 0 { x > to } else { x < to } {
            if self.should_abort() {
                return;
            }
            self.render_view();
            let mut s = self.sys.screen.borrow().clone();
            self.draw_guests(&mut s, Some((usize::from(sprite), x)));
            *self.sys.screen.borrow_mut() = s;
            self.sys.frame().await;
            x += step;
        }
    }

    /// AddGuest=N (`FUN_0042b480`): the record, the portrait sliding in,
    /// then the count grows.
    pub async fn add_guest(&mut self, guest: i32) {
        if self.guests.count >= super::Guests::SLOTS {
            return;
        }
        let mut next = self.guests.clone();
        next.add(guest, &self.game.exe.guest_sprite_ids);
        let n = self.guests.count;
        if self.guests_shown {
            self.slide_guest(next.sprite[n], 640, 540 - 100 * n as i32, -10)
                .await;
        }
        let y = self.guests.portrait_y;
        self.guests = next;
        self.guests.portrait_y = y;
    }

    /// SubGuest=N (`FUN_0042b6c0`): each matching record hidden, sliding
    /// out to the right, then the following records move down; after a
    /// removal the scan goes on at the next index.
    pub async fn sub_guest(&mut self, guest: i32) {
        let mut i = 0;
        while i < self.guests.count {
            if self.guests.id[i] == guest {
                let s = self.guests.sprite[i];
                self.guests.sprite[i] = super::Guests::HIDDEN;
                if self.guests_shown {
                    self.slide_guest(s, 540 - 100 * i as i32, 640, 10).await;
                }
                self.guests.remove_at(i);
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::NoScripts;
    use crate::sys::MouseButton;
    use crate::test_support::{button, with_engine};
    use crate::warp::SCREEN_W;
    use md5::{Digest, Md5};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A left click at frame 40 (the bar is up by then).
    fn click_at_40(sys: &Rc<crate::sys::Sys>, f: u64) {
        match f {
            40 => button(sys, MouseButton::Left, true),
            42 => button(sys, MouseButton::Left, false),
            _ => {}
        }
    }

    /// The pick-up bar ends like the inventory bar: Time1 / Time2 resume
    /// from their frozen values, ChapterTime from 0.
    #[test]
    fn pickup_bar_rebases_the_clocks() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "pickup_bar_rebases_the_clocks",
            move |mut e| async move {
                e.vars.set("time1", 7);
                e.vars.set("time2", 9);
                e.display_pickup(&mut NoScripts).await;
                let now = e.now();
                let c = e.clocks;
                *g2.borrow_mut() = Some((
                    c.time1_offset,
                    c.time2_offset,
                    c.time1_start == now,
                    c.chapter_start == now,
                ));
            },
            click_at_40,
        );
        if !ran {
            return;
        }
        let got = *got.borrow();
        assert_eq!(got, Some((7, 9, true, true)));
    }

    /// Each bar frame polls NewSound (FUN_00432178): a sample that ended
    /// while the bar is open sets NEWSOUND back to 0.
    #[test]
    fn bar_loop_polls_newsound() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "bar_loop_polls_newsound",
            move |mut e| async move {
                e.vars.set("newsound", 5);
                e.music.new_sound_last = 5;
                e.music.new_sound_playing = true; // and no voice: it ended
                e.display_bar(&mut NoScripts).await;
                *g2.borrow_mut() = Some(e.vars.get("newsound"));
            },
            click_at_40,
        );
        if !ran {
            return;
        }
        let got = *got.borrow();
        assert_eq!(got, Some(0));
    }

    /// The view the goldens' harness renders: u16 pixel i = (i *
    /// 2654435761) >> 16.
    fn hash_view() -> Vec<u16> {
        (0..(SCREEN_W * SCREEN_H) as u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 16) as u16)
            .collect()
    }

    fn md5_rows(screen: &[u16], rows: std::ops::Range<usize>) -> String {
        let b: Vec<u8> = screen[rows.start * SCREEN_W..rows.end * SCREEN_W]
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect();
        Md5::digest(&b).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The value of `key=` among a golden line's words.
    fn field<'a>(words: &[&'a str], key: &str) -> &'a str {
        words
            .iter()
            .find_map(|w| w.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
            .unwrap_or_else(|| panic!("no {key}= in {words:?}"))
    }

    fn golden_lines(text: &str) -> impl Iterator<Item = Vec<&str>> {
        text.lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| l.split_whitespace().collect())
    }

    /// `FUN_0042c59c`, frame by frame, against the original (the slide-out
    /// and the blit run under emulation on OBJETS0.SPR / OBJETS1.SPR over
    /// the view pattern, the slot tables left by the slide-in
    /// `FUN_0042c2c4` and, for the pick-ups, by `FUN_004278a0`): the
    /// background flag, the icons (record, slot, sprite, hotspot) and the
    /// md5 of the bar's rows on each of the 165 frames, the slivers of the
    /// records from 9 on at slot y 506 included, and slotX[0..8] against
    /// [`slot_x`]. With the slide-out's fix the frames from 506 show the
    /// view alone; with the pick-up's fix a pick-up's slot stays at
    /// 70 s + 42; each fix alone and both.
    #[test]
    fn slide_out_frames_vs_original() {
        let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
            eprintln!("ATLANTIS_DATA not set: slide_out_frames_vs_original skipped");
            return;
        };
        let game = atlantis_data::Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(dir)))
            .expect("cannot open the game directory");
        let spr = |name: &str, lim: (u16, u16)| {
            let d = game.big.read(&format!("SPRITE\\2D\\{name}")).expect(name);
            read_spr_file(&d, lim.0, lim.1).expect(name)
        };
        let bar = spr("OBJETS0.SPR", limits::INVENTORY_BAR);
        let icons = spr("OBJETS1.SPR", limits::DEFAULT);
        let view = hash_view();
        let view_only = md5_rows(&view, BAR_TOP..SCREEN_H);
        let icon = |id: i32| icons.get((id - 1000) as usize);
        let (mut frames, mut slivers, mut pickups) = (0, 0, 0);
        let mut bad = Vec::new();
        for (slide_out_fix, pickup_fix) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let fixes = slide_out_fix || pickup_fix;
            let mut records = Vec::new();
            let (mut first, mut held, mut pickup) = (0, 0, None);
            for w in golden_lines(include_str!("../../tests/golden/ui_bar_slide_out.txt")) {
                if w[0] == "scenario" {
                    records = field(&w, "sprites")
                        .split(',')
                        .map(|k| 1000 + k.parse::<i32>().unwrap())
                        .collect();
                    first = field(&w, "first").parse().unwrap();
                    held = match field(&w, "held") {
                        "-" => 0,
                        k => 1000 + k.parse::<i32>().unwrap(),
                    };
                    pickup = match field(&w, "pickup") {
                        "-" => None,
                        r => pickup_slot(r.parse().unwrap()),
                    };
                    if !fixes {
                        pickups += usize::from(field(&w, "pickup") != "-");
                        let want: Vec<i32> = field(&w, "slotx")
                            .split(',')
                            .map(|x| x.parse().unwrap())
                            .collect();
                        let got: Vec<i32> =
                            (0..BAR_CELLS).map(|k| slot_x(k, pickup, false)).collect();
                        assert_eq!(got, want, "slotx of {}", w[1]);
                    }
                    continue;
                }
                let slot_y: i32 = field(&w, "slotY").parse().unwrap();
                let st = BarState {
                    records: &records,
                    first,
                    held,
                    pickup_slot: pickup,
                    slide_out_fix,
                    pickup_fix,
                };
                let f = bar_frame_layout(&st, slot_y, true);
                let mut screen = view.clone();
                draw_bar_frame(&mut screen, &f, &st, slot_y, bar.first(), icon);
                let got_icons = f
                    .icons
                    .iter()
                    .map(|i| {
                        let k = records[i.record] - 1000;
                        format!("{}:{}:{k}:{}:{}", i.record, i.slot, i.x, i.y)
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let got_icons = if got_icons.is_empty() {
                    "-".to_string()
                } else {
                    got_icons
                };
                let got = (
                    if f.background { "1" } else { "0" },
                    got_icons.as_str(),
                    md5_rows(&screen, BAR_TOP..SCREEN_H),
                );
                if !fixes {
                    frames += 1;
                    if slot_y == 0x1fa && field(&w, "rows") != "-" {
                        slivers += 1;
                    }
                    let want = (
                        field(&w, "bg"),
                        field(&w, "icons"),
                        field(&w, "md5").to_string(),
                    );
                    if got != want {
                        bad.push(format!("{}: {got:?}", w.join(" ")));
                    }
                } else if slide_out_fix && slot_y >= 0x1fa {
                    // The view alone from 506.
                    if got != ("0", "-", view_only.clone()) {
                        bad.push(format!("fixes {}: {got:?}", w.join(" ")));
                    }
                } else {
                    // The same frames (before 506 with the slide-out's
                    // fix), but with the pick-up's fix for a pick-up's
                    // slot at 70 s + 42 (its icon moved: another md5).
                    let mut moved = false;
                    let want_icons = field(&w, "icons")
                        .split(',')
                        .map(|i| {
                            let p: Vec<&str> = i.split(':').collect();
                            if pickup_fix && p.len() == 5 && pickup == p[1].parse().ok() {
                                moved = true;
                                let s: i32 = p[1].parse().unwrap();
                                format!("{}:{}:{}:{}:{}", p[0], p[1], p[2], 70 * s + 42, p[4])
                            } else {
                                i.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    if got.0 != field(&w, "bg")
                        || got.1 != want_icons
                        || (!moved && got.2 != field(&w, "md5"))
                    {
                        bad.push(format!("fixes {}: {got:?}", w.join(" ")));
                    }
                }
            }
        }
        assert_eq!((frames, slivers, pickups), (165, 9, 6));
        assert!(
            bad.is_empty(),
            "{} frames differ:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    /// The bar slides with the guests' portraits against the original
    /// (`FUN_0042c2c4` / `FUN_0042c59c` with `FUN_0042bc58` and the blit run
    /// under emulation on OBJETS0.SPR / SUIVEUR.SPR, the view rendered anew
    /// each frame, no inventory record, the portraits at y 350 at the
    /// start): 11 guest lists, the slide-in then the slide-out run whole,
    /// the number of frames of each slide and the md5 of every whole frame.
    /// The portraits move before the slide-in's frames and after the
    /// slide-out's, and are drawn after the bar. Here no view is loaded, so
    /// each frame starts from the screen: the frame hook puts the view
    /// back, as the renderer would.
    #[test]
    fn slide_guest_frames_vs_original() {
        let view = hash_view();
        let shot: Rc<RefCell<Vec<String>>> = Rc::default();
        // Per guest list: its name, the md5s of the slide-in and slide-out
        // frames.
        type Frames = Vec<(String, Vec<String>, Vec<String>)>;
        let got: Rc<RefCell<Frames>> = Rc::default();
        let (shot2, got2, view2) = (shot.clone(), got.clone(), view.clone());
        let text = include_str!("../../tests/golden/ui_bar_slide_guests.txt");
        let scenarios: Vec<Vec<&str>> = golden_lines(text).filter(|w| w[0] == "scenario").collect();
        let lists: Vec<(String, bool, Vec<u8>)> = scenarios
            .iter()
            .map(|w| {
                (
                    w[1].to_string(),
                    field(w, "shown") == "1",
                    field(w, "sprites")
                        .split(',')
                        .map(|k| k.parse().unwrap())
                        .collect(),
                )
            })
            .collect();
        let ran = with_engine(
            "slide_guest_frames_vs_original",
            move |mut e| async move {
                e.load_ui_assets().await;
                *e.sys.screen.borrow_mut() = view2.clone();
                for (name, shown, sprites) in lists {
                    e.guests = super::super::Guests::default();
                    for (i, &k) in sprites.iter().enumerate() {
                        e.guests.id[i] = 600 + i as i32;
                        e.guests.sprite[i] = k;
                    }
                    e.guests.count = sprites.len();
                    e.guests_shown = shown;
                    shot2.borrow_mut().clear();
                    assert!(!e.bar_slide_in(&mut NoScripts).await);
                    let slide_in = std::mem::take(&mut *shot2.borrow_mut());
                    assert!(!e.bar_slide_out(&mut NoScripts).await);
                    let slide_out = std::mem::take(&mut *shot2.borrow_mut());
                    assert_eq!(e.guests.portrait_y, 350);
                    got2.borrow_mut().push((name, slide_in, slide_out));
                }
            },
            move |sys, _| {
                let mut s = sys.screen.borrow_mut();
                shot.borrow_mut().push(md5_rows(&s, 0..SCREEN_H));
                s.copy_from_slice(&view);
            },
        );
        if !ran {
            return;
        }
        let mut want: Frames = Vec::new();
        for w in golden_lines(text) {
            if w[0] == "scenario" {
                want.push((w[1].to_string(), Vec::new(), Vec::new()));
                continue;
            }
            let last = want.last_mut().unwrap();
            let md5 = field(&w, "md5").to_string();
            match field(&w, "phase") {
                "in" => last.1.push(md5),
                _ => last.2.push(md5),
            }
        }
        assert_eq!(want.len(), 11);
        let got = got.borrow();
        assert_eq!(got.len(), want.len());
        let mut bad = Vec::new();
        for (g, w) in got.iter().zip(&want) {
            assert_eq!(g.0, w.0);
            assert_eq!((g.1.len(), g.2.len()), (8, 11), "{}", g.0);
            for (phase, gp, wp) in [("in", &g.1, &w.1), ("out", &g.2, &w.2)] {
                for (k, (a, b)) in gp.iter().zip(wp).enumerate() {
                    if a != b {
                        bad.push(format!("{} {phase} frame {k}: {a}", g.0));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "{} frames differ:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    /// The frame hook of [`bar_ambient_restart_vs_original`]: on each slide
    /// frame's timers it notes whether `FUN_00432178` ran after the
    /// previous frame (it polls NewSound, which an ended sample sets back
    /// to 0), arms NewSound again, and sets the golden's timer variable on
    /// its frame.
    struct AmbientProbe {
        phase: &'static str,
        timer: Option<(String, usize, String)>,
        rows: Vec<(&'static str, usize, bool)>,
        unsettled: Option<usize>,
    }

    impl AmbientProbe {
        fn arm(e: &mut Engine) {
            e.vars.set("newsound", 5);
            e.music.new_sound_last = 5;
            e.music.new_sound_playing = true; // and no voice: it ended
        }

        /// The previous frame's `FUN_00432178`, once.
        fn settle(&mut self, e: &Engine) {
            if let Some(i) = self.unsettled.take() {
                self.rows[i].2 = e.vars.get("newsound") == 0;
            }
        }
    }

    impl ScriptHooks for AmbientProbe {
        fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> crate::hooks::HookFuture<'a, bool> {
            self.settle(e);
            let frame = self.rows.iter().filter(|r| r.0 == self.phase).count();
            self.unsettled = Some(self.rows.len());
            self.rows.push((self.phase, frame, false));
            Self::arm(e);
            if let Some((phase, f, var)) = &self.timer {
                if phase == self.phase && *f == frame {
                    e.vars.set(&var.to_ascii_lowercase(), 1);
                }
            }
            crate::hooks::ready(false)
        }
    }

    /// `FUN_00432178` in the bar slides and the pick-up's hold loop against
    /// the original (`FUN_0042c2c4`, `FUN_0042c59c` and `FUN_0042cc98` run
    /// under emulation with `FUN_00432178`): after each slide frame whose
    /// timers set neither GameOver nor Exit, none on the frame that ends
    /// the slide; on every hold pass, the one that reads the click
    /// included, which scrolls the bar too. The hold passes take the
    /// golden's pointer and press edge (an input of the run), and the exit
    /// row's end is one past the last cell [`clicked_record`] takes.
    #[test]
    fn bar_ambient_restart_vs_original() {
        type Rows = Vec<(String, Vec<String>)>;
        let text = include_str!("../../tests/golden/ui_bar_ambient_restart.txt");
        let mut golden: Rows = Vec::new();
        for w in golden_lines(text) {
            if w[0] == "scenario" {
                golden.push((w[1..].join(" "), Vec::new()));
            } else {
                golden.last_mut().unwrap().1.push(w[1..].join(" "));
            }
        }
        assert_eq!(golden.len(), 8);
        let got: Rc<RefCell<Rows>> = Rc::default();
        let (got2, golden2) = (got.clone(), golden.clone());
        let ran = with_engine(
            "bar_ambient_restart_vs_original",
            move |mut e| async move {
                for (head, rows) in golden2 {
                    let w: Vec<&str> = head.split_whitespace().collect();
                    let mut out = Vec::new();
                    e.vars.set("gameover", 0);
                    e.vars.set("exit", 0);
                    if let Some(t) = w.iter().find_map(|x| x.strip_prefix("timer=")) {
                        let timer = match t.split(':').collect::<Vec<_>>()[..] {
                            [p, f, v] => Some((p.to_string(), f.parse().unwrap(), v.to_string())),
                            _ => None,
                        };
                        let mut probe = AmbientProbe {
                            phase: "in",
                            timer,
                            rows: Vec::new(),
                            unsettled: None,
                        };
                        AmbientProbe::arm(&mut e);
                        let closed = e.bar_slide_in(&mut probe).await;
                        probe.settle(&e);
                        if !closed {
                            probe.phase = "out";
                            AmbientProbe::arm(&mut e);
                            e.bar_slide_out(&mut probe).await;
                            probe.settle(&e);
                        }
                        for (phase, frame, ambient) in probe.rows {
                            let slot_y = if phase == "in" {
                                492 - 7 * frame as i32
                            } else {
                                443 + 7 * frame as i32
                            };
                            let timer = match &probe.timer {
                                Some((p, f, v)) if p == phase && *f == frame => v.as_str(),
                                _ => "-",
                            };
                            out.push(format!(
                                "phase={phase} frame={frame} slotY={slot_y} timer={timer} ambient={}",
                                u8::from(ambient)
                            ));
                        }
                    } else {
                        let n: i32 = field(&w, "records").parse().unwrap();
                        e.inventory = (0..n).map(|i| 0x100 + i).collect();
                        e.bar_first = field(&w, "first").parse().unwrap();
                        let mut next_scroll = 0u64;
                        let mut passes = 0;
                        for r in &rows {
                            let r: Vec<&str> = r.split_whitespace().collect();
                            if r[0] == "exit" {
                                break;
                            }
                            let (x, y) = field(&r, "mouse").split_once(',').unwrap();
                            e.sys.set_mouse_pos(x.parse().unwrap(), y.parse().unwrap());
                            let edge = field(&r, "edge");
                            AmbientProbe::arm(&mut e);
                            let end = e
                                .hold_pass(edge == "left", edge == "right", &mut next_scroll)
                                .await;
                            passes += 1;
                            let edge = ["-", "left", "right"][end as usize];
                            out.push(format!(
                                "pass={} mouse={} edge={edge} ambient={} first={}",
                                passes - 1,
                                field(&r, "mouse"),
                                u8::from(e.vars.get("newsound") == 0),
                                e.bar_first
                            ));
                            if end != 0 {
                                break;
                            }
                        }
                        let end = (0..32)
                            .filter(|&c| clicked_record(70 * c, 409, e.bar_first).is_some())
                            .max()
                            .map_or(0, |c| c + 1);
                        out.push(format!(
                            "exit passes={passes} first={} end={end}",
                            e.bar_first
                        ));
                    }
                    got2.borrow_mut().push((head, out));
                }
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        // The golden's Miles calls are FUN_00432178's own (the ambient
        // tests); here the rows without them.
        let strip = |r: &String| -> String {
            r.split_whitespace()
                .filter(|w| !w.starts_with("calls="))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let got = got.borrow();
        assert_eq!(got.len(), golden.len());
        let mut bad = Vec::new();
        let mut rows = 0;
        for ((head, g), (_, w)) in got.iter().zip(&golden) {
            let w: Vec<String> = w.iter().map(strip).collect();
            rows += w.len();
            if *g != w {
                bad.push(format!("{head}:\n  port {g:?}\n  orig {w:?}"));
            }
        }
        assert_eq!(rows, 50);
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn bar_rules() {
        assert_eq!(clicked_record(0, 409, 0), Some(0));
        assert_eq!(clicked_record(0, 408, 0), None);
        assert_eq!(clicked_record(629, 420, 0), Some(8));
        assert_eq!(clicked_record(630, 420, 0), None);
        assert_eq!(clicked_record(630, 420, 1), Some(10));
        assert_eq!(scroll_step(631, 401, 10, 0), 1);
        assert_eq!(scroll_step(631, 401, 9, 0), 0);
        assert_eq!(scroll_step(631, 401, 10, 3), 0);
        assert_eq!(scroll_step(4, 401, 10, 1), -1);
        assert_eq!(scroll_step(4, 400, 10, 1), 0);
    }
}
