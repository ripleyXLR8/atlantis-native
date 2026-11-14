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

use super::{draw_sprite, draw_sprite_below};
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
/// a record's OBJETS1 sprite, from its icon byte), with `FUN_00410430`'s
/// clipping.
pub fn draw_bar_frame<'a>(
    screen: &mut [u16],
    frame: &BarFrame,
    slot_y: i32,
    bar: Option<&SprSprite>,
    icon: impl Fn(usize) -> Option<&'a SprSprite>,
) {
    if frame.background {
        if let Some(bar) = bar {
            draw_sprite_below(screen, bar, 0, slot_y, BAR_TOP);
        }
    }
    for i in &frame.icons {
        if let Some(spr) = icon(i.record) {
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
        let mut s = self.sys.screen.borrow().clone();
        let st = self.bar_state();
        let frame = bar_frame_layout(&st, slot_y, slide_out);
        // Each record with its own icon byte (@ 0x0042c445, 0x0042c6b9,
        // 0x0042c9db, 0x0042ce1c).
        draw_bar_frame(&mut s, &frame, slot_y, self.ui.bar.as_ref(), |r| {
            self.record_icon(r).and_then(|k| self.ui.object(k))
        });
        self.draw_guests(&mut s, None);
        self.draw_pointer(&mut s);
        *self.sys.screen.borrow_mut() = s;
    }

    /// The CON timers of a bar frame (`FUN_0042be3c`, after the frame's
    /// present); true when the bar has to close (GameOver, Exit, quit).
    /// The place sprites are stepped once a frame, before its render, by
    /// the loops themselves (`FUN_00425304`: no timer path steps them).
    async fn bar_tick(&mut self, hooks: &mut dyn ScriptHooks) -> bool {
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

    /// The edge scroll of the bar loops at the pointer (`mx`, `my`), at the
    /// port's pace.
    fn bar_scroll(&mut self, (mx, my): (i32, i32), next_ms: &mut u64) {
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
        // The pointer 0x004960ec / 0x004960f0 as the warp loop's last pump
        // left it: the slide-in pumps no message.
        let mut pointer = self.sys.mouse();
        self.bar_slide_in(hooks).await;
        if self.should_abort() {
            return 0;
        }
        // FUN_0042c2c4 polls no input, but the warp loop polls once after
        // it, between two clears of the click flag (0x0041c6e0..0x0041c6ef),
        // before FUN_0042c098: the press edges made during the slide-in are
        // taken there (FUN_0042e624 gives an edge-mode key its edge on the
        // first poll that sees it down). A left press is lost; a right
        // press sets 0x00496120 and toggles the request, which the bar loop
        // turns on again at its entry (0x0042c8c4); Escape sets nothing
        // (the request is on).
        // That poll pumps a message unless it saw a left press (0x0041a2b5):
        // the pointer the bar loop starts from.
        let mi = self.poll_menu_input();
        if !mi.left_press {
            pointer = self.sys.mouse();
            if mi.right_press {
                self.bar
                    .right_pressed(self.section_play, self.escape_request);
            }
        }
        let mut guest = 0;
        // FUN_0042c098 runs the bar loop unless GameOver is set (@
        // 0x0042c0c9); an Exit set by the slide-in ends its first pass.
        let closed = self.vars.get("gameover") != 0;
        if !closed {
            guest = self.bar_loop(hooks, pointer).await;
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
        // Then FUN_0042d098 (@ 0x0042c201) enters bow mode with the bow
        // in hand, before the slide-out: the hand is empty while the bar
        // slides down, the bow back among the bar's icons. A GameOver set
        // during the bar skips it (0x0042c0ed -> 0x0042c22d).
        let game_over = self.vars.get("gameover") != 0;
        if !game_over {
            self.take_bow().await;
        }
        if !game_over || guest != 0 {
            self.bar_slide_out(hooks).await;
        }
        // Without GameOver, FUN_0042c098 then polls once (0x0042c228,
        // after the click flag is cleared @ 0x0042c21e) and clears the
        // flags at once (0x0042c22d..0x0042c255): the press edges of a
        // left press, a right press or Escape made during the slide-out
        // are taken there and never reach the warp loop (Escape sets no
        // menu request: the bar request is 1 then, @ 0x0042c206). On the
        // guest path that poll comes after the conversation (the caller's).
        if !game_over && guest == 0 {
            self.drop_press_edges();
        }
        // FUN_0042d198 (@ 0x0042c25f) on every exit: the bow rises when bow
        // mode is on.
        // The cursor's click animation is cancelled (0x004968b8 = 0 @
        // 0x0042c241).
        self.cursor_state.click_anim = false;
        if self.bow_mode {
            self.raise_bow().await;
        }
        guest
    }

    /// `FUN_0041a298` called for its press edges only (`FUN_0042c098` @
    /// 0x0042c228): the presses queued since the last poll are taken and
    /// dropped.
    pub(crate) fn drop_press_edges(&self) {
        while self.sys.poll_event().is_some() {}
    }

    /// The bar loop `FUN_0042c8ac` from the pointer `pointer` of the last
    /// pump. Each pass: the exit test of the click flag (0x0042c8ce), the
    /// place sprites (`FUN_00425304` @ 0x0042c8e5), the view, the bar, the
    /// portraits and the pointer, the present (`FUN_0041afd0` @
    /// 0x0042cb22), the input (`FUN_0041a298` @ 0x0042cb27), the timers
    /// (`FUN_0042be3c` @ 0x0042cb2c) with the GameOver / Exit exit, the
    /// exit when the request is off (a right press toggled it or a timer
    /// section cleared it, 0x0042cb76), the edge scroll (0x0042cb83..
    /// 0x0042cbee) and `FUN_00432178` (@ 0x0042cbf4). A left press ends the
    /// loop at the next pass's test: its own pass runs the timers, the
    /// scroll and the sounds. Then `FUN_0042c098`'s item click.
    async fn bar_loop(&mut self, hooks: &mut dyn ScriptHooks, mut pointer: (i32, i32)) -> i32 {
        let mut next_scroll = 0u64;
        // The click flag 0x0049611c and the exits through GameOver / Exit.
        let (mut left, mut timer_exit) = (false, false);
        self.bar.opened(); // 0x0042c8c4
        loop {
            if self.should_abort() {
                return 0;
            }
            if left {
                break;
            }
            self.animate_sprites();
            self.bar_frame(BAR_HOTSPOT_Y, false);
            self.sys.frame().await;
            let mi = self.poll_menu_input();
            if mi.left_press {
                // The left-press branch of FUN_0041a298 sets the flag and
                // returns before its pump (0x0041a2b5): the pass keeps the
                // previous pass's pointer.
                left = true;
            } else {
                pointer = self.sys.mouse();
                if mi.right_press {
                    // The flag 0x00496120 and the request's toggle
                    // (@ 0x0041a2f2..0x0041a337).
                    self.bar
                        .right_pressed(self.section_play, self.escape_request);
                }
            }
            if self.bar_tick(hooks).await {
                // 0x0042cb3c..0x0042cb6b: the request off, then the exit.
                timer_exit = true;
                break;
            }
            if !self.bar.request {
                break;
            }
            self.bar_scroll(pointer, &mut next_scroll);
            // FUN_00432178 ends each bar pass.
            self.bar_sounds().await;
        }
        if self.should_abort() {
            return 0;
        }
        // FUN_0042c098 step 6 clears both flags whatever the exit.
        let session = self.bar;
        self.bar.flags_reset();
        // The exit test of FUN_0042c8ac (0x0042cbfe): the portrait under
        // the pointer, other than 592.
        let (mx, my) = pointer;
        let guest = match self.guest_at(mx, my) {
            592 => 0,
            g => g,
        };
        // A guest's portrait: the caller runs the conversation
        // (FUN_00425828, after the slide-out). A GameOver (0x0042c0ed): no
        // item click.
        if guest != 0 || self.vars.get("gameover") != 0 {
            return guest;
        }
        // FUN_0042c098 reads the cell at once (0x0042c0f8, 0x0042c117),
        // from the same pointer, with no wait for the release, when the
        // request is still on (an Exit, a right press or a timer section
        // turned it off); else either press flag empties the hand
        // (0x0042c1c9). A cell past the last record reads 0, which empties
        // the hand; the record in hand empties it too.
        let request = session.request && !timer_exit;
        let held = self.vars.get("objetenmain");
        match clicked_record(mx, my, self.bar_first).filter(|_| request) {
            Some(r) => {
                let id = self.inventory.get(r).copied().unwrap_or(0);
                if id == held {
                    self.vars.set("objetenmain", 0);
                } else {
                    // The hand's icon is the record's byte (@ 0x0042c18f).
                    self.vars.set("objetenmain", id);
                    self.held_icon = self.record_icon(r).map(|k| (id, k as u8));
                }
            }
            None if left || session.right_press => self.vars.set("objetenmain", 0),
            None => {}
        }
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
        // The loop ends on a press edge of either button; a press made
        // during the slide-in (which polls no input) is its first poll's.
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
        // FUN_004278a0 goes from FUN_0042cc98 to the slide-out
        // FUN_0042c59c (dc:19326-19333) with no wait for the release.
        if !right {
            self.vars.set("objetenmain", 0);
        }
        self.bar_slide_out(hooks).await;
        // FUN_004278a0 cancels the cursor's click animation after the
        // pick-up's slide-out (0x004968b8 = 0 @ 0x00427b5d).
        self.cursor_state.click_anim = false;
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
        self.bar_scroll(self.sys.mouse(), next_scroll);
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
    /// mode, the hand emptied.
    async fn take_bow(&mut self) {
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
    }

    /// `FUN_0042d198`: the bow rises, frame 0 from y 0x1e1 to 0x83, 10 px
    /// a frame.
    async fn raise_bow(&mut self) {
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

    /// One frame of the bow rising or lowering. `FUN_0042d198` /
    /// `FUN_0042d368` poll no input: a press made meanwhile is the next
    /// poll's press edge.
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
    use crate::hooks::{ready, HookFuture, NoScripts};
    use crate::sys::{Input, MouseButton};
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

    /// The hand and bow mode each bar frame's timers see; `game_over`: the
    /// first timer pass sets GameOver.
    struct BowProbe {
        seen: Rc<RefCell<Vec<(i32, bool)>>>,
        game_over: bool,
    }

    impl ScriptHooks for BowProbe {
        fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            if self.game_over {
                e.vars.set("gameover", 1);
            }
            self.seen
                .borrow_mut()
                .push((e.vars.get("objetenmain"), e.bow_mode));
            ready(self.game_over)
        }
    }

    /// ObjetEnMain and bow mode.
    type HandBow = (i32, bool);

    /// The inventory bar (`FUN_0042c098`) with the bow (object 334) in it
    /// and ArcMagique set, a click on the bow; `held`: the object in hand
    /// at the start. The hand and bow mode the timers saw on each frame,
    /// then after the bar.
    fn bow_from_the_bar(held: i32, game_over: bool) -> Option<(Vec<HandBow>, HandBow)> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let got = Rc::new(RefCell::new(None));
        let (s2, g2) = (seen.clone(), got.clone());
        let ran = with_engine(
            "bow_mode_starts_before_the_slide_out",
            move |mut e| async move {
                e.inventory.push(334);
                e.vars.set("arcmagique", 1);
                e.vars.set("objetenmain", held);
                let mut hooks = BowProbe {
                    seen: s2,
                    game_over,
                };
                e.display_bar(&mut hooks).await;
                *g2.borrow_mut() = Some((e.vars.get("objetenmain"), e.bow_mode));
            },
            |sys, f| match f {
                // The bow's cell, record 0.
                39 => sys.push_input(crate::sys::Input::MouseMove { x: 35, y: 420 }),
                40 => button(sys, MouseButton::Left, true),
                42 => button(sys, MouseButton::Left, false),
                _ => {}
            },
        );
        if !ran {
            return None;
        }
        let after = got.borrow().unwrap();
        let seen = seen.borrow().clone();
        Some((seen, after))
    }

    /// `FUN_0042c098` enters bow mode (`FUN_0042d098` @ 0x0042c201) before
    /// the slide-out (`FUN_0042c59c` @ 0x0042c219): the 11 slide-out
    /// frames run with the hand empty and bow mode on.
    #[test]
    fn bow_mode_starts_before_the_slide_out() {
        let Some((seen, after)) = bow_from_the_bar(0, false) else {
            return;
        };
        assert_eq!(after, (0, true));
        assert!(seen.len() > 11, "{seen:?}");
        assert!(
            seen[seen.len() - 11..].iter().all(|&s| s == (0, true)),
            "{seen:?}"
        );
    }

    /// A GameOver set during the bar skips `FUN_0042d098` (0x0042c0ed ->
    /// 0x0042c22d): the bow stays in hand, no bow mode.
    #[test]
    fn no_bow_mode_after_a_game_over_in_the_bar() {
        let Some((_, after)) = bow_from_the_bar(334, true) else {
            return;
        };
        assert_eq!(after, (334, false));
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

    /// Closing the bar (`FUN_0042c098` @ 0x0042c241) and the pick-up
    /// (`FUN_004278a0` @ 0x00427b5d) cancel the cursor's click animation.
    #[test]
    fn the_bars_cancel_the_click_animation() {
        for pickup in [false, true] {
            let got = Rc::new(RefCell::new(None));
            let g2 = got.clone();
            let ran = with_engine(
                "the_bars_cancel_the_click_animation",
                move |mut e| async move {
                    e.cursor_state.click_anim = true;
                    if pickup {
                        e.display_pickup(&mut NoScripts).await;
                    } else {
                        e.display_bar(&mut NoScripts).await;
                    }
                    *g2.borrow_mut() = Some(e.cursor_state.click_anim);
                },
                click_at_40,
            );
            if !ran {
                return;
            }
            assert_eq!(*got.borrow(), Some(false), "pickup {pickup}");
        }
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

    /// `FUN_0042c098` takes the cell under the pointer of the press
    /// (0x0042c0f8), with no wait for the release: a click that moves to
    /// another cell before its release takes the first one.
    #[test]
    fn bar_item_is_read_at_the_press() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "bar_item_is_read_at_the_press",
            move |mut e| async move {
                e.inventory = vec![1001, 1002, 1003];
                e.vars.set("objetenmain", 0);
                e.display_bar(&mut NoScripts).await;
                *g2.borrow_mut() = Some(e.vars.get("objetenmain"));
            },
            |sys, f| match f {
                30 => sys.push_input(Input::MouseMove { x: 35, y: 450 }),
                40 => button(sys, MouseButton::Left, true),
                41 => sys.push_input(Input::MouseMove { x: 105, y: 450 }),
                45 => button(sys, MouseButton::Left, false),
                _ => {}
            },
        );
        if ran {
            assert_eq!(*got.borrow(), Some(1001));
        }
    }

    /// The slide-ins (`FUN_0042c2c4`) poll no input. After the inventory
    /// bar's, the warp loop polls once (0x0041c6ea) between two clears of
    /// the click flag: a left press made during the slide-in is lost, a
    /// right press toggles the request, which the bar loop turns on again
    /// (0x0042c8c4); the bar stays up. The pick-up (`FUN_004278a0`) has no
    /// such poll: the press is the hold loop's (`FUN_0042cc98`) first
    /// poll's edge, which ends it on its first pass.
    #[test]
    fn press_during_the_slide_in() {
        for (pickup, b) in [
            (false, MouseButton::Left),
            (false, MouseButton::Right),
            (true, MouseButton::Right),
        ] {
            let got = Rc::new(RefCell::new(None));
            let g2 = got.clone();
            let ran = with_engine(
                "press_during_the_slide_in",
                move |mut e| async move {
                    e.inventory = vec![1001];
                    e.vars.set("objetenmain", 1001);
                    if pickup {
                        e.display_pickup(&mut NoScripts).await;
                    } else {
                        e.display_bar(&mut NoScripts).await;
                    }
                    *g2.borrow_mut() = Some(e.sys.millis());
                },
                move |sys, f| match f {
                    3 => button(sys, b, true),
                    4 => button(sys, b, false),
                    // Without the press edge the bar stays up until this.
                    300 => button(sys, MouseButton::Left, true),
                    301 => button(sys, MouseButton::Left, false),
                    _ => {}
                },
            );
            if !ran {
                return;
            }
            // 8 slide-in frames, one loop pass, 11 slide-out frames.
            let ms = got.borrow().expect("the bar did not end");
            if pickup {
                assert!(ms < 40 * 16, "pickup: ended at {ms} ms");
            } else {
                assert!(ms > 300 * 16, "{b:?}: ended at {ms} ms");
            }
        }
    }

    /// `FUN_0042c098` polls once after the slide-out (0x0042c228) and
    /// clears the flags: a press made during the slide-out never reaches
    /// the warp loop.
    #[test]
    fn presses_during_the_slide_out_are_taken() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "presses_during_the_slide_out_are_taken",
            move |mut e| async move {
                e.inventory = vec![1001];
                e.display_bar(&mut NoScripts).await;
                let mut presses = 0;
                while let Some(ev) = e.sys.poll_event() {
                    presses += usize::from(matches!(ev, Input::MouseDown { .. }));
                }
                *g2.borrow_mut() = Some((presses, e.sys.millis()));
            },
            |sys, f| match f {
                // A click off the bar's cells closes it, then a right and a
                // left press during the 11 slide-out frames.
                30 => sys.push_input(Input::MouseMove { x: 300, y: 200 }),
                40 => button(sys, MouseButton::Left, true),
                41 => button(sys, MouseButton::Left, false),
                44 => button(sys, MouseButton::Right, true),
                45 => button(sys, MouseButton::Right, false),
                47 => button(sys, MouseButton::Left, true),
                48 => button(sys, MouseButton::Left, false),
                _ => {}
            },
        );
        if !ran {
            return;
        }
        let (presses, ms) = got.borrow().expect("the bar did not end");
        assert!(ms > 48 * 16, "the bar ended at {ms} ms, before the presses");
        assert_eq!(presses, 0);
    }

    /// GameOver set by a timer section while the left button is down.
    struct GameOverWhileHeld;

    impl ScriptHooks for GameOverWhileHeld {
        fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            let over = e.sys.buttons().0;
            if over {
                e.vars.set("gameover", 1);
            }
            ready(over)
        }
    }

    /// The pass of `FUN_0042c8ac` whose poll (0x0042cb27) sees the left
    /// press still runs the timers (0x0042cb2c); the loop ends at the next
    /// pass's test (0x0042c8ce). A GameOver they set takes no item
    /// (0x0042c0ed).
    #[test]
    fn the_click_pass_runs_the_timers() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "the_click_pass_runs_the_timers",
            move |mut e| async move {
                e.inventory = vec![1001];
                e.display_bar(&mut GameOverWhileHeld).await;
                *g2.borrow_mut() = Some((e.vars.get("gameover"), e.vars.get("objetenmain")));
            },
            |sys, f| match f {
                30 => sys.push_input(Input::MouseMove { x: 35, y: 450 }),
                40 => button(sys, MouseButton::Left, true),
                45 => button(sys, MouseButton::Left, false),
                _ => {}
            },
        );
        if ran {
            assert_eq!(*got.borrow(), Some((1, 0)));
        }
    }

    /// `FUN_004278a0` goes from the hold loop to the slide-out with no wait
    /// for the release (dc:19326-19333): the pick-up ends while the button
    /// is still held.
    #[test]
    fn the_pickup_does_not_wait_for_the_release() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "the_pickup_does_not_wait_for_the_release",
            move |mut e| async move {
                e.inventory = vec![1001];
                e.vars.set("objetenmain", 1001);
                e.display_pickup(&mut NoScripts).await;
                *g2.borrow_mut() = Some(e.sys.millis());
            },
            |sys, f| match f {
                40 => button(sys, MouseButton::Left, true),
                300 => button(sys, MouseButton::Left, false),
                _ => {}
            },
        );
        if ran {
            let ms = got.borrow().expect("the pick-up did not end");
            assert!(ms < 80 * 16, "ended at {ms} ms");
        }
    }

    /// Counts the timer passes.
    struct TimerCount(Rc<std::cell::Cell<u32>>);

    impl ScriptHooks for TimerCount {
        fn bar_timers<'a>(&'a mut self, _e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.0.set(self.0.get() + 1);
            ready(false)
        }
    }

    /// Each pass of the bar loop `FUN_0042c8ac` presents its frame
    /// (`FUN_0041afd0` @ 0x0042cb22) before its timers (`FUN_0042be3c` @
    /// 0x0042cb2c), as each slide frame does: at every present the timers
    /// have run once for each earlier frame, so a timer's change is drawn
    /// on the next frame.
    #[test]
    fn the_bar_loop_runs_the_timers_after_the_present() {
        let timers = Rc::new(std::cell::Cell::new(0));
        let seen: Rc<RefCell<Vec<(u32, u32)>>> = Rc::default();
        let done = Rc::new(std::cell::Cell::new(false));
        let (t2, s2, d2) = (timers.clone(), seen.clone(), done.clone());
        let ran = with_engine(
            "the_bar_loop_runs_the_timers_after_the_present",
            move |mut e| async move {
                e.inventory = vec![1001];
                e.display_bar(&mut TimerCount(t2)).await;
                d2.set(true);
            },
            move |sys, f| {
                if !done.get() {
                    s2.borrow_mut().push((f as u32 + 1, timers.get()));
                }
                match f {
                    40 => button(sys, MouseButton::Left, true),
                    41 => button(sys, MouseButton::Left, false),
                    _ => {}
                }
            },
        );
        if !ran {
            return;
        }
        let seen = seen.borrow();
        assert!(seen.len() > 40);
        assert!(
            seen.iter()
                .all(|&(presents, timers)| timers + 1 == presents),
            "{seen:?}"
        );
    }

    /// The sprite-animation tick each slide frame's timers see, and the
    /// clock's tick then.
    struct SpriteTicks(Rc<RefCell<Vec<(u32, u32)>>>);

    impl ScriptHooks for SpriteTicks {
        fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            let now =
                (e.sys.millis() / u64::from(atlantis_data::sprite_anim::SPRITE_TICK_MS)) as u32;
            self.0.borrow_mut().push((e.sprite_anim_tick, now));
            ready(false)
        }
    }

    /// The slides step the place sprites once a frame, before the render
    /// (`FUN_0042c2c4` @ 0x0042c327, `FUN_0042c59c` @ 0x0042c5b4); their
    /// timers (`FUN_0042be3c`, after the present) step none, so they see
    /// the step of the frame's start, not one made after the present.
    #[test]
    fn the_slides_step_the_sprites_once_a_frame() {
        let got: Rc<RefCell<Vec<(u32, u32)>>> = Rc::default();
        let g2 = got.clone();
        let ran = with_engine(
            "the_slides_step_the_sprites_once_a_frame",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.current_place = Some(0);
                e.current_place_id = Some(e.wam.places[0].id);
                e.reload_current_view().await;
                assert!(e.warp_loaded);
                let mut hooks = SpriteTicks(g2);
                assert!(!e.bar_slide_in(&mut hooks).await);
                assert!(!e.bar_slide_out(&mut hooks).await);
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = got.borrow();
        assert_eq!(got.len(), 8 + 11);
        assert!(got.iter().all(|&(step, now)| step < now), "{got:?}");
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
                draw_bar_frame(&mut screen, &f, slot_y, bar.first(), |r| {
                    icons.get((records[r] - 1000) as usize)
                });
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
