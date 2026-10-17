//! The inventory bar: OBJETS0.SPR with the objects' icons, sliding up from
//! the bottom of the screen; the object picked up shown in it; the bow.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`toolbar.cpp`,
//! `navigation.cpp` bow mode; GPLv3, the ScummVM team), which follows
//! `atlantis.exe`: `FUN_0042c098` (a bar session and its click),
//! `FUN_0042c2c4` / `FUN_0042c59c` (the slides), `FUN_0042c8ac` (the bar
//! loop), `FUN_0042cc98` (the object picked up), `FUN_0042d098` /
//! `FUN_0042d198` / `FUN_0042d368` (bow mode), `ArcMagique=`'s animation.

use atlantis_data::sprite::{limits, read_spr_file};

use super::{draw_sprite, draw_sprite_below, object_icon};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::warp::SCREEN_H;

/// The visible cells of the bar.
pub const BAR_CELLS: usize = 9;
/// The bar's top row on the screen (82 rows).
pub const BAR_TOP: usize = SCREEN_H - 82;
/// The bar sprite's hotspot y when up (`FUN_0042c8ac` @ 0x0042c96e).
pub const BAR_HOTSPOT_Y: i32 = 443;
/// A slide position that hides the bar.
pub const BAR_HIDDEN: i32 = 82;
/// The port's pace of the edge scroll (the original's was the machine's).
const SCROLL_MS: u64 = 110;

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

impl Engine {
    /// The view with the bar at slide position `pos` (0 up, 82 hidden),
    /// the icons (the object in hand left out) and the guests' portraits,
    /// the pointer on top.
    fn bar_frame(&mut self, pos: i32, guests_first: bool) {
        if self.warp_loaded {
            self.render_view();
        }
        let held = self.vars.get("objetenmain");
        let table = self.game.exe.inventory_object_ids.clone();
        let mut s = self.sys.screen.borrow().clone();
        if guests_first {
            self.draw_guests(&mut s, None);
        }
        if pos < BAR_HIDDEN {
            if let Some(bar) = &self.ui.bar {
                draw_sprite_below(&mut s, bar, 0, BAR_HOTSPOT_Y + pos, BAR_TOP);
            }
            for i in 0..BAR_CELLS {
                let Some(&id) = self.inventory.get(i + self.bar_first) else {
                    continue;
                };
                if id == held && held != 0 {
                    continue;
                }
                if let Some(spr) = object_icon(&table, id).and_then(|k| self.ui.object(k)) {
                    draw_sprite_below(
                        &mut s,
                        spr,
                        42 + 70 * i as i32,
                        BAR_HOTSPOT_Y + pos,
                        BAR_TOP,
                    );
                }
            }
        }
        if !guests_first {
            self.draw_guests(&mut s, None);
        }
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

    /// `FUN_0042c2c4`: the slots from y 499 up 7 a frame to 443 (8 frames),
    /// the portraits up with them. True when a timer closed the bar.
    async fn bar_slide_in(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
        let mut slot_y = 499 - 7;
        while slot_y >= BAR_HOTSPOT_Y && !self.should_abort() {
            self.slide_guest_portraits(-7);
            self.bar_frame(slot_y - BAR_HOTSPOT_Y, true);
            self.sys.frame().await;
            if self.bar_tick(hooks).await {
                return true;
            }
            slot_y -= 7;
        }
        false
    }

    /// `FUN_0042c59c`: drawn while the slots are above 0x1fa, then down 7,
    /// until 0x208 (11 frames, the last two without the bar); the portraits
    /// back down to 350.
    async fn bar_slide_out(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
        let mut slot_y = BAR_HOTSPOT_Y;
        while slot_y < 0x208 && !self.should_abort() {
            let pos = if slot_y < 0x1fa {
                slot_y - BAR_HOTSPOT_Y
            } else {
                BAR_HIDDEN
            };
            self.bar_frame(pos, true);
            self.sys.frame().await;
            self.slide_guest_portraits(7);
            if self.bar_tick(hooks).await {
                return true;
            }
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
        // FUN_0042c098 slides the bar out on every exit (0x0042c219).
        self.bar_slide_out(hooks).await;
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
            self.bar_frame(0, false);
            self.ambient_frame();
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
        let mut right = false;
        let mut next_scroll = 0u64;
        // The loop ends on a press edge of either button.
        while self.sys.poll_event().is_some() {}
        while !self.should_abort() {
            let mi = self.poll_menu_input();
            if mi.left_press {
                break;
            }
            if mi.right_press {
                right = true;
                break;
            }
            // FUN_0042cc98 animates the place sprites but runs no timers.
            self.animate_sprites();
            self.bar_scroll(&mut next_scroll);
            self.bar_frame(0, false);
            self.sys.frame().await;
        }
        self.wait_buttons_up().await;
        if !right {
            self.vars.set("objetenmain", 0);
        }
        self.bar_slide_out(hooks).await;
        right
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
