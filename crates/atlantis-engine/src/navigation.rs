//! The warp loop: the panoramic view driven by the mouse, the cursor, the
//! zone under the pointer and the click.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`navigation.cpp`
//! `handleWarp`; GPLv3, the ScummVM team), which follows `atlantis.exe`'s
//! warp loop `FUN_0041bfa0`.
//!
//! # Pointer
//!
//! The original reads the motion of a pointer it puts back at the screen
//! centre every frame. Here the front end locks the system pointer
//! ([`crate::sys::Sys::set_mouse_locked`]) and reports relative motion;
//! the pointer the loop hit-tests ([`crate::look::WarpPointer`]) follows
//! that motion and the recentre rule of the original. The cursor sprite is
//! drawn into the frame at that pointer, as `FUN_00422fcc` draws it with
//! the sprite blitter.
//!
//! # Pace
//!
//! The original's loop is not paced (it runs as fast as the machine
//! allows); one loop pass here is one displayed frame. The momentum decays
//! per frame, as in the original. The cursor animation steps every 80 ms
//! (the ScummVM port's choice).

use atlantis_data::sprite_list::npc_hit_test;

use crate::cursor::{warp_cursor_frame_drawn, warp_cursor_step, WarpCursorInput};
use crate::engine::{Abort, Engine, CURSOR_UNSET, WARP_ABORT};
use crate::hooks::ScriptHooks;
use crate::look::warp_look_frame;
use crate::sys::{Input, Key, MouseButton};
use crate::warp::{SCREEN_H, SCREEN_W};

/// The view keys' step, 0.034906 rad (2 degrees) on every poll they are
/// held (`FUN_0041a298` @ 0x0041a33c..0x0041a3ba).
const VIEW_KEY_STEP: f64 = 0.034906;

/// The keys of one poll ([`Engine::poll_view_keys`]): F5, F6, F8, F7.
const VIEW_F5: u8 = 1;
const VIEW_F6: u8 = 2;
const VIEW_F8: u8 = 4;
const VIEW_F7: u8 = 8;

/// The stored angles after the polls `polls`, in the order of
/// `FUN_0041a298` (0x0041a33c..0x0041a3ba): alpha + step for F5, - step
/// for F6, beta + step for F8, - step for F7, each added in double and
/// stored to the float (no wrap or clamp: `FUN_00451915` does that when
/// the view is set).
fn view_keys_over(polls: &[u8], (mut alpha, mut beta): (f32, f32)) -> (f32, f32) {
    for &k in polls {
        if k & VIEW_F5 != 0 {
            alpha = (f64::from(alpha) + VIEW_KEY_STEP) as f32;
        }
        if k & VIEW_F6 != 0 {
            alpha = (f64::from(alpha) - VIEW_KEY_STEP) as f32;
        }
        if k & VIEW_F8 != 0 {
            beta = (f64::from(beta) + VIEW_KEY_STEP) as f32;
        }
        if k & VIEW_F7 != 0 {
            beta = (f64::from(beta) - VIEW_KEY_STEP) as f32;
        }
    }
    (alpha, beta)
}

/// The port's pace of the cursor (the warp loop is not paced in the
/// original; atlantis-re decision 0013): one warp frame's cursor calls
/// every 160 ms.
/// The reference build calls `FUN_00422fcc` twice a frame (0x0041c535,
/// then through `FUN_0041afd0` @ 0x0041c5ed) and shows the first
/// ([`warp_cursor_frame`]), so its animation steps every 80 ms; the 1997
/// builds call it once (atlantis-cd-en-688128 FUN_0041bbd8), every 160 ms.
const CURSOR_FRAME_MS: u64 = 160;

/// What the loop read from the events of a frame.
#[derive(Default)]
struct FrameInput {
    left_press: bool,
    right_press: bool,
    escape: bool,
}

impl Engine {
    fn poll_frame_input(&self) -> FrameInput {
        let mut fi = FrameInput::default();
        while let Some(ev) = self.sys.poll_event() {
            match ev {
                Input::MouseDown {
                    button: MouseButton::Left,
                    ..
                } => fi.left_press = true,
                Input::MouseDown {
                    button: MouseButton::Right,
                    ..
                } => fi.right_press = true,
                Input::KeyDown(Key::Escape) => fi.escape = true,
                Input::KeyDown(Key::F(9)) => self.debug_overlay.set(!self.debug_overlay.get()),
                _ => {}
            }
        }
        fi
    }

    /// The view keys at a poll of `FUN_0041a298` (0x0041a33c..0x0041a3ba):
    /// F5 / F6 turn and F8 / F7 tilt the stored view angles 0x00bcb59c /
    /// 0x00bcb598 by [`VIEW_KEY_STEP`] on every poll they are held (not
    /// edge keys), unless the poll saw a new left press, which returns
    /// before (0x0041a2b5). Every polling loop does it: the warp loop, the
    /// dialog lines, the subject menu, the game-over screen, the puzzles,
    /// the bar, the menus and the videos. Only `FUN_0041bfa0`,
    /// `FUN_004212cc`, `FUN_0042844c`, `FUN_0042a100`, `FUN_00430258` and
    /// `FUN_00451915` read or write the angles otherwise, so outside the
    /// warp loop nothing turns until the view is set from them again.
    pub(crate) fn poll_view_keys(&self, left_press: bool) {
        self.poll_view_keys_with(left_press, false);
    }

    /// [`Engine::poll_view_keys`]; `arrows` adds the arrow keys
    /// ([`crate::enhancements::Enhancements::arrow_keys`], the warp loop
    /// only): left F5, right F6, up F7, down F8.
    fn poll_view_keys_with(&self, left_press: bool, arrows: bool) {
        if left_press {
            return;
        }
        let held = |f: u8, arrow: Key| {
            self.sys.key_held(Key::F(f)) || (arrows && self.sys.key_held(arrow))
        };
        let mut k = 0;
        for (bit, f, arrow) in [
            (VIEW_F5, 5, Key::Left),
            (VIEW_F6, 6, Key::Right),
            (VIEW_F8, 8, Key::Down),
            (VIEW_F7, 7, Key::Up),
        ] {
            if held(f, arrow) {
                k |= bit;
            }
        }
        if k != 0 {
            self.view_key_polls.borrow_mut().push(k);
        }
    }

    /// The stored view angles 0x00bcb59c / 0x00bcb598: the warp's, moved
    /// by the view keys polled since it was last set.
    pub(crate) fn stored_view(&self) -> (f32, f32) {
        view_keys_over(
            &self.view_key_polls.borrow(),
            (self.warp.alpha(), self.warp.beta()),
        )
    }

    /// `angles` moved by the view keys polled since, which are used up: the
    /// stored angles after a line whose view the port set to its camera.
    pub(crate) fn take_view_keys_over(&self, angles: (f32, f32)) -> (f32, f32) {
        view_keys_over(&self.view_key_polls.take(), angles)
    }

    /// The warp set to the stored view angles (where the original reads
    /// 0x00bcb59c / 0x00bcb598 to set the view or keep it).
    pub(crate) fn apply_view_keys(&mut self) {
        if self.view_key_polls.borrow().is_empty() {
            return;
        }
        let (a, b) = self.take_view_keys_over((self.warp.alpha(), self.warp.beta()));
        self.warp.set_alpha(a);
        self.warp.set_beta(b);
    }

    /// Waits for the buttons to be released (the loops drain the click
    /// that ended them).
    pub(crate) async fn wait_buttons_up(&self) {
        while self.sys.buttons() != (false, false) && !self.sys.should_quit() {
            while self.sys.poll_event().is_some() {}
            self.sys.frame().await;
        }
    }

    /// Port-only diagnostic overlay (F9): the variables the scripts test
    /// during navigation, and the conditions of CHAPI013's flower pot at
    /// view 126 (/178, /14: AngleWarpX 1..12 or 343..359, AngleWarpY 27..45,
    /// Wsprframe 8..33 or 70..96, ObjetEnMain 261).
    fn draw_debug_overlay(&self) {
        let font = self
            .debug_font
            .get_or_init(crate::subtitles::SubtitleFont::new);
        let v = |k: &str| self.vars.get(k);
        let (ax, ay, w, obj) = (
            v("anglewarpx"),
            v("anglewarpy"),
            v("wsprframe"),
            v("objetenmain"),
        );
        let vue = self.current_place_id.map_or(-1, i32::from);
        let mut lines = vec![format!(
            "{} vue {vue}  chap {}  AngleWarpX {ax}  AngleWarpY {ay}  Wsprframe {w}  ObjetEnMain {obj}",
            self.wam_name,
            v("chapter")
        )];
        // CHAPI022's patrol (60 s cycle on time2): 182, 183 and 185 are
        // safe only while FlagGarde is 184 (time2 28..48), 182 and 183 also
        // at 187.
        if v("chapter") == 22 {
            let g = v("flaggarde");
            let safe = g == 184;
            lines.push(format!(
                "GARDES: time2 {}  FlagGarde {g}  => {}",
                v("time2"),
                if safe {
                    "VOIE LIBRE: 182 > 183 > 185 > 186 vite"
                } else {
                    "attends a 181 (libre quand FlagGarde = 184, time2 28..48)"
                }
            ));
        }
        if vue == 126 || vue == 125 {
            let ok = |b: bool| if b { "OK" } else { "--" };
            let x_ok = (ax > 0 && ax < 13) || (ax > 342 && ax < 360);
            let y_ok = ay > 26 && ay < 46;
            let w_ok = (w > 7 && w < 34) || (w > 69 && w < 97);
            lines.push(format!(
                "POT: vue126 {}  X {}  Y {}  image {}  pot en main {}  => {}",
                ok(vue == 126),
                ok(x_ok),
                ok(y_ok),
                ok(w_ok),
                ok(obj == 261),
                if vue == 126 && x_ok && y_ok && w_ok && obj == 261 {
                    "CLIQUE MAINTENANT"
                } else {
                    "pas encore"
                }
            ));
        }
        let mut s = self.sys.screen.borrow_mut();
        for (i, line) in lines.iter().enumerate() {
            let y = 4 + i as i32 * 16;
            for yy in y.max(0)..(y + 15).min(SCREEN_H as i32) {
                for xx in 0..SCREEN_W {
                    let p = &mut s[yy as usize * SCREEN_W + xx];
                    *p = (*p >> 2) & 0x39e7; // darkened band
                }
            }
            font.draw(
                &mut s,
                SCREEN_W,
                SCREEN_H,
                line.as_bytes(),
                5,
                y,
                630,
                0xffe0,
            );
        }
    }

    /// The object in hand at the pointer (0x0041c43f..0x0041c481), drawn
    /// before the guests' portraits.
    fn draw_held_icon(&self, screen: &mut [u16]) {
        if let Some(spr) = self.held_icon().and_then(|i| self.ui.object(i)) {
            spr.draw(
                screen,
                SCREEN_W,
                SCREEN_H,
                self.pointer.pos.0,
                self.pointer.pos.1,
            );
        }
    }

    /// The cursor sprite `FUN_00422fcc` chose, at the pointer.
    fn draw_cursor_sprite(&self) {
        let Ok(i) = usize::try_from(self.cursor_shown) else {
            return;
        };
        if let Some(spr) = self.cursor_sprites.get(i) {
            let mut screen = self.sys.screen.borrow_mut();
            spr.draw(
                &mut screen,
                SCREEN_W,
                SCREEN_H,
                self.pointer.pos.0,
                self.pointer.pos.1,
            );
        }
    }

    /// The warp frame over the rendered view (0x0041c40b..0x0041c4b4): the
    /// bow (ARCWARP.SPR frame arrow_frame at (0x2e, 0x79)) in bow mode,
    /// otherwise the object in hand; the guests' portraits
    /// (`FUN_0042bc58` @ 0x0041c489); then, out of bow mode, the cursor
    /// `FUN_00422fcc`.
    fn draw_warp_overlays(&mut self) {
        if self.warp_loaded {
            let mut s = self.sys.screen.borrow().clone();
            if self.bow_mode {
                if let Some(spr) = self.bow_sprites.get(usize::from(self.arrow_frame)) {
                    crate::ui::draw_sprite(&mut s, spr, 0x2e, 0x79);
                }
            } else {
                self.draw_held_icon(&mut s);
            }
            self.draw_guests(&mut s, None);
            *self.sys.screen.borrow_mut() = s;
        }
        // No cursor in bow mode (0x0041c4b4) or with the bar request a timer
        // section of this frame left (0x0041c4ca: 0x004960c4 == 0; the bar
        // opens at the frame's end, here at the next pass's start); its
        // animation state does not advance.
        self.warp_buffer_cursor = None;
        if !self.bow_mode && !self.bar.request {
            self.step_warp_cursor();
            self.draw_cursor_sprite();
            if self.cursor_after_copy >= 0 {
                self.warp_buffer_cursor = Some((self.cursor_after_copy, self.pointer.pos));
            }
        }
    }

    /// `FUN_00422fcc` in the warp frame, after the look, the timers and the
    /// render: its own hit tests (`FUN_00422bb0`, `FUN_00422d5c` at its
    /// entry) at this frame's view and pointer, which the next click acts
    /// on. One frame of calls ([`warp_cursor_frame`]: the build's
    /// `warp_cursor_steps`, the first shown) every [`CURSOR_FRAME_MS`]; a
    /// change of mode (nothing, navigate, talk, object, click), which the
    /// first call of a frame sees, is shown at once and starts a frame.
    fn step_warp_cursor(&mut self) {
        let (zone, id) = self.pointer_targets();
        let input = WarpCursorInput {
            zone,
            id,
            held: self.vars.get("objetenmain").max(0) as u32,
            hidden: false,
            edge_scroll: false,
            id_rule: self.game.exe.build.cursor_id_rule,
        };
        let now = self.sys.millis();
        let mut first = self.cursor_state;
        warp_cursor_step(&mut first, &input);
        if self.cursor_shown == CURSOR_UNSET
            || now >= self.cursor_next_ms
            || first.mode != self.cursor_state.mode
        {
            let steps = self.game.exe.build.warp_cursor_steps;
            let (shown, after_copy) =
                warp_cursor_frame_drawn(&mut self.cursor_state, &input, steps);
            self.cursor_shown = shown.sprite;
            self.cursor_after_copy = after_copy;
            self.cursor_next_ms = now + CURSOR_FRAME_MS;
        }
    }

    /// The zone (`FUN_00422bb0`) and the character (`FUN_00422d5c`) under
    /// the pointer at the view as it stands.
    pub(crate) fn pointer_targets(&mut self) -> (u32, u32) {
        let (Some(vue), true) = (self.current_place_id, self.warp_loaded) else {
            return (0, 0);
        };
        let Some(place) = self.current_place.and_then(|i| self.wam.places.get(i)) else {
            return (0, 0);
        };
        let (px, py) = self
            .warp
            .screen_to_panorama(self.pointer.pos.0, self.pointer.pos.1);
        let (px, py) = (i32::from(px as i16), i32::from(py as i16));
        let perso = npc_hit_test(
            &self.wam.npcs,
            &self.npc_rects,
            &self.sprite_list,
            vue,
            px,
            py,
        );
        (u32::from(place.hit_test(px as i16, py as i16)), perso)
    }

    /// `FUN_0041afd0`'s cursor step, for the still frame of a voice-only
    /// line (`FUN_00430258` @ 0x00430550): with 0x004960c0, GAMEOVER, bow
    /// mode, 0x00496188, the bar request, the menu request, 0x00496154 and
    /// 0x0049616c all 0 (0x0041afe8..0x0041b050), one step of
    /// `FUN_00422fcc` over what is under the pointer. True when it stepped.
    ///
    /// Its sprite is not shown: `FUN_00422fcc` draws into the back buffer
    /// `[0x004960d8]` (`push dword ptr [0x4960d8]` @ 0x00423394, 0x004234eb,
    /// 0x004236a6; a heap block of its own, `malloc(0x96000)` stored @
    /// 0x00418f9c), and `FUN_00430258` has already copied that buffer to
    /// the surface (`FUN_00410f36` @ 0x004304eb) before it draws the
    /// subtitle and calls `FUN_0041afd0`, whose present (`FUN_004337d4` @
    /// 0x0041b055) shows the surface. Its wait loop (0x00430555..0x0043058d)
    /// only polls. So the still frame shows no cursor, with Flip
    /// (fullscreen) or Blt (windowed) alike; the system pointer is hidden in
    /// fullscreen (WM_SETCURSOR, `SetCursor(NULL)` @ 0x00419d1a).
    pub(crate) fn still_frame_cursor_step(&mut self) -> bool {
        if self.give_scan
            || self.puzzle_scan
            || self.bow_mode
            || self.vars.get("gameover") != 0
            || self.bar.request
            || self.menu_request()
        {
            return false;
        }
        let (zone, perso) = self.pointer_targets();
        let input = WarpCursorInput {
            zone,
            id: perso,
            held: self.vars.get("objetenmain").max(0) as u32,
            hidden: false,
            edge_scroll: false,
            id_rule: self.game.exe.build.cursor_id_rule,
        };
        warp_cursor_step(&mut self.cursor_state, &input);
        true
    }

    /// The warp loop `FUN_0041bfa0`: the view until a click. Returns the
    /// zone clicked (0 for none) or [`WARP_ABORT`].
    /// The warp loop left (unlocked) with `unpumped`, the motion of the
    /// frame whose poll saw a left press edge and returned before its
    /// pump (`FUN_0041a298` @ 0x0041a2b5): it moved the Windows cursor
    /// (clamped as [`WarpPointer::follow`]) and its WM_MOUSEMOVE is still
    /// to be retrieved. The system mouse goes there and the mark stays at
    /// the cursor the pointer knows, so the next poll that pumps (a
    /// puzzle's first one after its first present, a dialog's, the turn's)
    /// moves the pointer to it (0x00419c10..0x00419c29).
    fn leave_warp_mouse(&mut self, unpumped: (i32, i32)) {
        let mut moved = self.pointer;
        moved.follow(unpumped.0, unpumped.1);
        self.sys.set_mouse_pos(moved.cursor.0, moved.cursor.1);
        self.pointer_mouse_mark = self.pointer.cursor;
    }

    pub async fn handle_warp(&mut self, hooks: &mut dyn ScriptHooks) -> u32 {
        let mut exit = false;
        let mut action = 0u32;
        self.pending_no_click_exit = None;

        // The loop takes the pointer where the caller left it (entry block):
        // a mouse motion since the engine last wrote it is the WM_MOUSEMOVE
        // that set it in the original.
        self.sync_pointer_mouse();
        // The entry block clears 0x00496120 and 0x004960c4 (0x0041bff4),
        // and the double-click flag 0x00496118 (0x0041bfc2).
        self.bar.flags_reset();
        self.sys.double_click().clear();
        // No wait at the entry (0x0041bfb8..0x0041c01e; the first frame
        // polls FUN_0041a298 @ 0x0041c5f2). A click is a press edge
        // (FUN_0042e624's latch): a button still down from a press an
        // earlier poll saw is none, and a press no poll has seen yet is
        // one. After a move none is left: FUN_00423ad8 polls once after
        // the place load and clears the flags (0x00424093..0x004240ca,
        // Engine::end_of_move_input), so a press during the video's last
        // frame or the load is lost. The engine's input events are those
        // edges.
        // The entry block calls no SetCursorPos: the Windows cursor stays
        // where the mouse is, apart from the pointer after a panorama load
        // (`WarpPointer::cursor`).
        self.mark_pointer_mouse();
        self.sys.set_mouse_locked(true);
        self.sys.take_mouse_relative();
        // The previous point of the look delta (0x0041c003 / 0x0041c00d).
        self.pointer.loop_entry();

        self.cursor_shown = CURSOR_UNSET;
        // The arrow frame starts at 0 on every entry (0x0041c017).
        self.arrow_frame = 0;
        // FUN_00431a1c at 0x0041c012: the place's ambient sounds.
        self.ambient_enter_place().await;

        let mut left_pressed = false;
        // The motion of a poll that saw a left press edge: FUN_0041a298
        // returns before its message pump (0x0041a2b5), so the
        // WM_MOUSEMOVE waits for the next poll.
        let mut unpumped = (0, 0);
        // No warp frame presented since the loop was entered: the port's
        // pass polls first, the original's polls last (0x0041c5f2).
        let mut entered = true;
        // AngleWarpY as the previous pass started: FUN_0041bfa0 keeps the
        // frame-start angle in a local (fistp [ebp-8] @ 0x0041c0e3, before
        // the look step 0x0041c1c3) that the bow test after the frame's
        // poll reads (0x0041c637).
        let mut frame_start_angle_y = self.vars.get("anglewarpy");
        while !left_pressed && !exit {
            // Windows sees a press at its cursor (`WarpPointer::cursor`,
            // the loop's pointer once a motion or a SetCursorPos set it):
            // every left press of the loop is paired there
            // (Sys::double_click), the click that ends it, a character
            // click, a bow shot and a press the latch did not report alike.
            self.sys.set_locked_cursor(self.pointer.cursor);
            // The loop's first frame reads no input: the entry block polls
            // nothing (0x0041bfb8..0x0041c01e) and the frame's one poll
            // FUN_0041a298 is its end (0x0041c5f2), after the clocks
            // (FUN_0041c75c @ 0x0041c086), the timer scan (FUN_0042be3c @
            // 0x0041c369), the render, the guests' portraits (0x0041c489),
            // the cursor (0x0041c535), the copy (0x0041c5a4) and the
            // present (0x0041c5ed). A press, a right press or an Escape
            // made before the loop (during a follower's slide, ArcMagique,
            // the pick-up's slide-out, which poll nothing) is still queued
            // when that frame's timer section runs, whose lines take it;
            // else the second pass reads it, as the original's loop head
            // (0x0041c028..0x0041c041) and FUN_0041c838 (0x0041c618, which
            // dims the frame the first pass presented, FUN_0041cc28 @
            // 0x0041ccd2) act on it after the first frame. Also after a
            // character's conversation: the place loop enters the loop
            // again (0x00423c1c).
            let fi = if entered {
                FrameInput::default()
            } else {
                self.poll_frame_input()
            };
            // Each frame starts by clearing the double-click flag
            // (0x0041c04a), after the previous frame's poll (0x0041c5f2)
            // pumped the messages, unless it saw a left press edge: a
            // WM_LBUTTONDBLCLK of the click that ends the loop stays
            // queued.
            self.sys.double_click().clear();
            // This frame's relative motion (x right, y down).
            let (motion_x, motion_y) = self.sys.take_mouse_relative();
            unpumped = (unpumped.0 + motion_x, unpumped.1 + motion_y);
            // A press edge (FUN_0042e624): a held button is no click.
            let mut clicked = fi.left_press;
            // Bow mode consumes the click (0x0041c631): no zone or
            // character action; the press edge feeds the shot test after
            // the frame is drawn.
            let bow_click = self.bow_mode && clicked;
            if self.bow_mode {
                clicked = false;
            }
            // Bow mode as the poll saw it: FUN_0041a298 sets the menu
            // request on Escape only with no bow (0x00496164 == 0, dc:11436),
            // so lowering the bow below does not let an Escape of the same
            // poll open the menu.
            let bow_at_poll = self.bow_mode;
            // The bow step right after the poll that ends the original's
            // frame (0x0041c61d..0x0041c678), before the next frame's look
            // step, timer scan (FUN_0042be3c @ 0x0041c369) and drawing: a
            // left edge in bow mode shoots when the AngleWarpY that frame
            // started with is below 20, then the arrow frame steps while
            // TirFleche is set. The timer scan of this pass sees TirFleche
            // = 1 and the bow is drawn with arrow frame 1 (0x0041c40b).
            // The first pass has had no poll.
            if !entered {
                let mut tir = self.vars.get("tirfleche");
                atlantis_data::sprite_anim::bow_frame_step(
                    self.bow_mode,
                    bow_click,
                    frame_start_angle_y,
                    &mut tir,
                    &mut self.arrow_frame,
                );
                self.vars.set("tirfleche", tir);
            }
            // A bar request a timer section of the previous pass left (a
            // right press during its lines): the original's pass ends its
            // inner loop on `0x004960c4 == 1` (FUN_0041bfa0, the
            // `while (0x004960c4 != 1)` after the poll at 0x0041c5f2)
            // before its head tests the click, opens the bar
            // (FUN_0042c2c4) and clears the click flag 0x0049611c: the bar
            // opens below and the click is lost.
            if clicked && self.bar.request {
                clicked = false;
            }

            // The zone and the character under the pointer: a character
            // takes priority over the zone underneath it.
            let (zone, centre_perso) = self.pointer_targets();
            action = zone;

            // A click on a character or an object (FUN_004278a0): ids
            // 0x100..0x1ff are objects, from 0x200 the conversation; no move.
            if clicked && centre_perso >= 0x100 {
                self.sys.set_mouse_locked(false);
                // The original leaves the loop with the mouse at the
                // Windows cursor; the dialog's mouse motions set the pointer,
                // that of this frame's press included.
                self.leave_warp_mouse(std::mem::take(&mut unpumped));
                // ClicZone = the zone under the pointer: the warp loop
                // returns it with the NPC (0x0041c71e) and the place loop
                // stores it before FUN_004278a0 (0x00423c1c); it returns to
                // 0 on the next pass.
                self.vars.set("cliczone", action as i32);
                // The place loop runs the puzzle-zone runner FUN_004240fc
                // (0x00423c42) before FUN_004278a0 (0x00423d12) on every
                // click: it does not test the id 0x006cd7e4, so an armed
                // puzzle zone under a character runs the puzzle first
                // (PALAIS2/3 vue 290 NPC 540 over zone 1002, FORET3/4 vue 52
                // NPC 567 over zone 1008). Quit, GAMEOVER or EXIT then end
                // the pass before FUN_004278a0 (0x00423ca5..0x00423d0d).
                let puzzle_ran =
                    (1000..1017).contains(&action) && hooks.puzzle_zone(self, action - 1000).await;
                let pass_ended = puzzle_ran
                    && (self.should_abort()
                        || self.vars.get("gameover") != 0
                        || self.vars.get("exit") != 0);
                if !pass_ended {
                    // FUN_004278a0 starts the click animation (0x004278df).
                    self.cursor_state.click_anim = true;
                    hooks.npc_click(self, centre_perso, action).await;
                }
                self.vars.set("cliczone", 0);
                self.sync_pointer_mouse();
                // A puzzle ran: the place loop takes over (the warp loop
                // entered again reloads the ambient sounds FUN_004240fc
                // stopped, 0x0041c012).
                if self.should_abort() || self.arrival_scan_pending || puzzle_ran {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    // No recentre: the loop goes on with the pointer the
                    // dialog left. The click the dialog ended on, if still
                    // held, is no new press (the dialog's polls took it).
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.sys
                        .set_mouse_pos(self.pointer.cursor.0, self.pointer.cursor.1);
                    self.mark_pointer_mouse();
                    self.cursor_shown = CURSOR_UNSET;
                    // The loop entered again (0x0041c003 / 0x0041c00d),
                    // the arrow frame at 0 (0x0041c017).
                    unpumped = (0, 0);
                    self.pointer.loop_entry();
                    self.arrow_frame = 0;
                    entered = true;
                    // The original's warp loop returns the clicked character
                    // (0x0041c71e) after this frame's timers (0x0041c369);
                    // the place loop runs the conversation (0x00423c1c) and
                    // enters the loop again, whose first frame updates the
                    // clocks (0x0041c086). Going on with this frame ran the
                    // timers with the clocks read before the conversation:
                    // a Time1=0 in it (CHAPI024 /280, Carven's betrayal) was
                    // seen as the old count and the guard struck at once.
                    continue;
                }
            }
            left_pressed = clicked;
            if self.should_abort() {
                action = WARP_ABORT;
                exit = true;
            }
            // The poll that read the click is the last call but two of the
            // original's pass (0x0041c5f2); the bow step and FUN_00432178
            // follow it, and the click ends the loop at the next loop head
            // (0x0041c028..0x0041c041): no clock update, cursor step, timer
            // scan, sprite step or drawing comes between the click and its
            // handling. (The port's pass starts with that poll.)
            if left_pressed && !exit {
                self.poll_new_sound().await;
                self.ambient_frame();
                break;
            }
            self.update_con_clocks(); // FUN_0041bfa0 @ 0x0041c086

            // The poll's WM_MOUSEMOVE (0x00419c14) moves the pointer; the
            // first frame has had no poll.
            if !fi.left_press && !entered {
                let (dx, dy) = std::mem::take(&mut unpumped);
                self.pointer.follow(dx, dy);
            }

            // A right click opens the inventory bar (FUN_0041a298 toggles
            // 0x004960c4, the loop opens the bar @ 0x0041c6d2), also when a
            // right press during the previous frame's timer section left
            // the request; each frame starts by clearing the flags
            // (0x0041c04a..0x0041c07c). Escape opens the in-game menu with
            // no bow, no request and an empty hand (0x0041c5f7).
            // A right press edge: a held right button does not open the bar
            // again.
            // The frame's poll (0x0041c5f2) toggles the request a timer
            // section left on a right press (FUN_0041a298 @ 0x0041a2f2..
            // 0x0041a337, read only without a left press edge, 0x0041a2b5),
            // and the inner loop ends only if it is still 1: a right press
            // right after such a request cancels it.
            let pending = self.bar.warp_frame_start();
            let toggled = fi.right_press && !fi.left_press;
            let mut bar = !exit && (pending != toggled);
            if bar && self.bow_mode {
                // In bow mode the request only lowers the bow
                // (0x0041c6af..0x0041c6c8: FUN_0042d368).
                self.sys.set_mouse_locked(false);
                self.lower_bow().await;
                self.sys.set_mouse_locked(true);
                self.sys.take_mouse_relative();
                bar = false;
            }
            if bar {
                // Closing the bar enters bow mode with the bow in hand and
                // ArcMagique set, before the slide-out (FUN_0042d098 in
                // `display_bar`). The slide-out ends with the timers
                // resuming (FUN_0042c59c @ 0x0042c844); a guest's portrait
                // under the pointer gets its conversation inside the bar
                // loop (FUN_0042c8ac @ 0x0042cc57).
                let guest = self.display_bar(hooks).await;
                // A guest's conversation that moved the player (a Vue= or a
                // Chapter=) ends the loop for the arrival scan. A Vue= from
                // a timer section that ran while the bar was open does not:
                // the loop runs on in the new place, and the arrival scan
                // comes after the next click unless that click moves (as for
                // a warp-frame timer, `Engine::game_step`).
                let moved = guest != 0 && !self.should_abort() && self.arrival_scan_pending;
                if self.should_abort() || moved {
                    // GAMEOVER after the bar (0x0041c6fe..0x0041c717): the
                    // bar closed with the pointer at (320, 240) (FUN_0042c098
                    // @ 0x0042c275).
                    if !moved && self.game_over_left_set() {
                        self.pointer.bar_closed();
                        self.pending_no_click_exit = Some(true);
                    }
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    // FUN_0042c098 closes the bar with the pointer and the
                    // previous point at (320, 240) (@ 0x0042c275,
                    // 0x0042c28e): the look delta of the next frame is 0.
                    unpumped = (0, 0);
                    self.pointer.bar_closed();
                    self.sys
                        .set_mouse_pos(self.pointer.cursor.0, self.pointer.cursor.1);
                    self.mark_pointer_mouse();
                    self.cursor_shown = CURSOR_UNSET;
                    // The original opens the bar at the end of its frame
                    // (0x0041c6d2), a guest's conversation inside it
                    // (FUN_0042c8ac -> FUN_00425828); the next frame reads
                    // the clocks (FUN_0041c75c @ 0x0041c086) before its
                    // timers: a Time1= of the conversation (CHAPI032 /419)
                    // is seen at once.
                    self.update_con_clocks();
                }
            } else if !exit && fi.escape && !bow_at_poll && self.vars.get("objetenmain") == 0 {
                self.sys.set_mouse_locked(false);
                self.sys
                    .set_mouse_pos(self.pointer.cursor.0, self.pointer.cursor.1);
                self.mark_pointer_mouse();
                self.run_ingame_menu().await;
                // The Japanese DVD build's language clicks: the CON text in
                // the new language (FUN_0041d768 reloads it at each click;
                // nothing reads it while the menus are open).
                if std::mem::take(&mut self.con_text_reload) {
                    hooks.reload_con_text(self).await;
                }
                self.sync_pointer_mouse();
                if self.should_abort() {
                    // LOAD or QUIT: 0x00496124 = 0 ends the loop.
                    if matches!(self.abort, Some(Abort::Load | Abort::Quit))
                        && !self.sys.should_quit()
                        && !self.game_over_cleared_at_once()
                    {
                        self.pending_no_click_exit = Some(false);
                    }
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.sys
                        .set_mouse_pos(self.pointer.cursor.0, self.pointer.cursor.1);
                    self.mark_pointer_mouse();
                    self.cursor_shown = CURSOR_UNSET;
                }
            }

            // The view: FUN_0041bfa0 steps 2..5. The stored angles first
            // take F5 / F6 (turn) and F8 / F7 (tilt) of the poll
            // (FUN_0041a298 at the end of the previous frame) when it saw no
            // new left press (@ 0x0041a2b5: VK_LBUTTON is an edge key, so a
            // held button does not stop them; only bow mode keeps a press in
            // the loop), and of the polls of the loops that ran since; the
            // port adds the arrow keys (Enhancements::arrow_keys), which the
            // original's view ignores, as the same keys: left F5, right F6,
            // up F7, down F8 (a larger beta looks down).
            // (The first frame's angles are those the polls before the
            // loop left.)
            if !entered {
                self.poll_view_keys_with(fi.left_press, self.enhancements().arrow_keys);
            }
            let (mut alpha, mut beta) = self.stored_view();
            self.view_key_polls.borrow_mut().clear();
            // The delta is previous point minus pointer (step 3,
            // 0x0041c1c3..0x0041c20d): the motion since the last step 5, the
            // loop entry or the bar close, the WM_MOUSEMOVEs of a timer
            // section's lines or of the in-game menu included. OMNI 3D MODE
            // 1, the original's default (0x004960ac == 0), gives dy = prevY -
            // y; MODE 2 y - prevY.
            let mode2 = self.settings.omni3d_mode2;
            let (motion_x, motion_y) = self.pointer.motion();
            let look = warp_look_frame(
                &mut self.momentum,
                &mut alpha,
                &mut beta,
                motion_x,
                motion_y,
                mode2,
                self.pointer.prev_alpha,
                self.pointer.prev_beta,
                self.warp.pi(),
                self.warp.two_pi(),
                self.warp.beta_limit(),
            );
            self.angle_warp = (look.angle_warp_x, look.angle_warp_y);
            frame_start_angle_y = look.angle_warp_y;
            self.vars.set("anglewarpx", look.angle_warp_x);
            self.vars.set("anglewarpy", look.angle_warp_y);
            self.set_view(alpha, beta);
            // The pointer goes back to the centre when the new (unclamped)
            // angles differ from the previous frame's, and the previous
            // point takes it (step 5, @ 0x0041c2bb..0x0041c363).
            self.pointer
                .frame(look.new_alpha, look.new_beta, alpha, beta);

            // Timer sections (@ 0x0041c369). A Vue= in the timer section has
            // entered another place; the warp loop does not return for it:
            // the section executor ends with the click flag cleared
            // (FUN_0042844c @ 0x00428d77) and the Vue= branch sets no exit
            // flag (FUN_0042a100 @ 0x0042a630..0x0042a749), so the loop runs
            // on in the new place and the arrival scan comes after the next
            // click, unless that click moves (`Engine::game_step`).
            let exit_before_timers = exit;
            if !exit && hooks.warp_frame(self).await {
                self.sync_pointer_mouse();
                if self.should_abort() {
                    if self.game_over_left_set() {
                        self.pending_no_click_exit = Some(true);
                    }
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    // A press the section's polls saw, if still held, is no
                    // new one.
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.sys
                        .set_mouse_pos(self.pointer.cursor.0, self.pointer.cursor.1);
                    self.mark_pointer_mouse();
                    self.cursor_shown = CURSOR_UNSET;
                }
            }

            // Place sprites: FUN_00425304 at 0x0041c3a9, after the timers,
            // unless GameOver or Exit is set (then it leaves, 0x0041c387).
            if !exit && self.vars.get("exit") != 0 {
                if !self.game_over_cleared_at_once() && !self.sys.should_quit() {
                    self.pending_no_click_exit = Some(true);
                }
                action = WARP_ABORT;
                exit = true;
            }
            // Leaving here (0x0041c387 / 0x0041c3a2 -> 0x0041c71e), the
            // frame ends at once: no sprites, render, bow step or
            // FUN_00432178.
            if exit && !exit_before_timers {
                break;
            }
            if !exit {
                self.animate_sprites();
            }
            self.render_view();
            self.draw_warp_overlays();
            if self.debug_overlay.get() {
                self.draw_debug_overlay();
            }
            // FUN_00432178, the last call of the frame: the ambient sounds,
            // then the NewSound poll FUN_00432a64.
            self.poll_new_sound().await;
            self.ambient_frame();
            if !exit {
                self.sys.frame().await;
                self.warp_frame_presented = Some(self.sys.frames_presented());
                entered = false;
            }
        }

        self.sys.set_mouse_locked(false);
        // The mouse at the Windows cursor; motions from here on set the
        // pointer to it, that of the frame whose poll saw the click
        // included.
        self.leave_warp_mouse(unpumped);
        // The click that ended the loop: Windows paired it with the last
        // press whatever loop read that one (the press that cut the
        // previous move's film, or ended a line), and pairs the next press
        // with it. As the second of a pair it came as WM_LBUTTONDBLCLK,
        // which this loop's last poll did not pump (0x0041a2b5): the next
        // poll that pumps (a departure line's, the turn's with the cursor
        // state 5) sets 0x00496118 and the move's film is skipped
        // (0x00423f8a). Sys::double_click.
        // No wait for the release: FUN_0041bfa0 returns from its exit block
        // (0x0041c71e) and FUN_00423ad8 goes on with the departure scan and
        // the turn at once. The buttons are read only through the latch
        // FUN_0042e624, whose press bit is set on the poll the button goes
        // down, so the later loops (their presses are edges) see a held
        // button as no press.
        log::debug!("handle_warp: action {action}");
        action
    }

    /// A game over ended the warp loop and GAMEOVER is left set until the
    /// reload ([`crate::enhancements::BugFix::GameOverPuzzle`] off).
    pub(crate) fn game_over_left_set(&self) -> bool {
        self.abort == Some(Abort::NewGame)
            && !self.sys.should_quit()
            && !self.game_over_cleared_at_once()
    }

    /// The place loop after a warp loop left without a click
    /// ([`crate::enhancements::BugFix::GameOverPuzzle`] off): the warp loop's exit block returns
    /// the zone under the pointer whatever ended it (`FUN_0041bfa0` @
    /// 0x0041c71e: FUN_00422bb0, and the id under it), and `FUN_00423ad8`
    /// acts on it before it tests why (0x00423c12..0x00423ca5): ClicZone =
    /// the zone; `FUN_004240fc` runs the armed puzzle of a zone 1000..1016
    /// while 0x00496124 is 1 (0x00424114: after a game over or an Exit,
    /// not after the in-game menu's LOAD or QUIT, `keep_running`), then
    /// the view reload and a /sel scan; zone 999 with no id runs the /sel
    /// scan with DepartVue = 999 (0x00423c47..0x00423c9b). GAMEOVER is
    /// still set, so the scan's section shows the game-over picture again
    /// after its first line (`Scripts::play_section`). Then the pass ends
    /// for the reason the loop ended.
    pub(crate) async fn no_click_exit(&mut self, hooks: &mut dyn ScriptHooks, keep_running: bool) {
        let abort = self.abort.take();
        let (zone, id) = self.pointer_targets();
        self.vars.set("cliczone", zone as i32);
        // 0x00496124 = 0 after LOAD or QUIT: the lines of the zone-999
        // scan stop at their first poll, as after a puzzle's Escape. The
        // menu request 0x00496138 is 0 there (FUN_0041c838's tail @
        // 0x0041cbff..0x0041cc09), so a right press of the scan's lines
        // toggles the bar request 0x004960c4, which nothing clears before
        // the reloaded chapter's warp loop (`FUN_0042de5c` does not):
        // Escape then latches nothing, the intro film of the loaded
        // chapter's arrival scan included.
        self.escape_request = !keep_running;
        self.menu_request_cleared = !keep_running;
        if keep_running && (1000..1017).contains(&zone) {
            hooks.puzzle_zone(self, zone - 1000).await;
        }
        if zone == 999 && id == 0 && !self.should_abort() {
            hooks.zone_999(self).await;
        }
        // The main loop sets 0x00496124 = 1 before the reload (0x0041975d).
        self.escape_request = false;
        self.menu_request_cleared = false;
        // An abort raised here wins over the one that ended the warp loop:
        // the game-over screen sets 0x00496124 = 1 (FUN_00426590 @
        // 0x00426c41), so a game over raised by the zone-999 scan after a
        // LOAD or QUIT (CHAPI031 /372 under `--original`) passes the place
        // loop's run-flag test (0x00423ca5), and its GAMEOVER test reloads.
        if self.abort.is_none() {
            self.abort = abort;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{ready, Departure, HookFuture};
    use crate::test_support::{button, with_engine};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// The cursor sprites the warp loop shows over a navigation zone with
    /// the reference build: the first of each frame's two `FUN_00422fcc`
    /// calls (0x0041c535 shown, 0x0041c5ed after the copy), so 9, 11, 13
    /// and never 10, 12, 14.
    #[test]
    fn the_warp_loop_shows_every_second_cursor_sprite() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "the_warp_loop_shows_every_second_cursor_sprite",
            move |mut e| async move {
                assert_eq!(e.game.exe.build.warp_cursor_steps, 2);
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.pointer.pos = (320, 240);
                let view = (0..315)
                    .flat_map(|ia| (-4..=4).map(move |ib| (ia as f32 * 0.02, ib as f32 * 0.1)))
                    .find(|&(a, b)| {
                        e.set_view(a, b);
                        matches!(e.pointer_targets(), (z, 0) if (1..0x200).contains(&z))
                    })
                    .expect("a navigation view in place 1");
                e.set_view(view.0, view.1);
                e.cursor_shown = CURSOR_UNSET;
                for _ in 0..200 {
                    e.step_warp_cursor();
                    g2.borrow_mut().push(e.cursor_shown);
                    e.sys.frame().await;
                }
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = got.borrow();
        for s in [9, 11, 13] {
            assert!(got.contains(&s), "{s} never shown: {got:?}");
        }
        assert!(got.iter().all(|s| [9, 11, 13].contains(s)), "{got:?}");
    }

    /// FUN_0041cc28 (@ 0x0041ccd2) dims the warp frame's back buffer as
    /// composed: the screen of the last warp frame with the cursor sprite
    /// FUN_0041afd0 drew after the copy (@ 0x0041c5ed) over it. With
    /// another frame presented since, the view rendered anew.
    #[test]
    fn the_menu_backdrop_is_the_composed_warp_frame() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "the_menu_backdrop_is_the_composed_warp_frame",
            move |mut e| async move {
                e.load_interface().await;
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                let pattern: Vec<u16> = (0..SCREEN_W * SCREEN_H).map(|k| k as u16).collect();
                e.sys.screen.borrow_mut().copy_from_slice(&pattern);
                e.warp_frame_presented = Some(e.sys.frames_presented());
                e.warp_buffer_cursor = Some((10, (320, 240)));
                let composed = e.dimmed_view();
                let mut want = pattern.clone();
                e.cursor_sprites[10].draw(&mut want, SCREEN_W, SCREEN_H, 320, 240);
                crate::ui::dim_screen(&mut want);
                let cursor_drawn = want != {
                    let mut p = pattern.clone();
                    crate::ui::dim_screen(&mut p);
                    p
                };
                // A frame presented since: the view rendered anew.
                e.sys.screen.borrow_mut().copy_from_slice(&pattern);
                e.sys.frame().await;
                let fresh = e.dimmed_view();
                *g2.borrow_mut() = Some((composed == want, cursor_drawn, fresh != want));
            },
            |_, _| {},
        );
        if ran {
            assert_eq!(got.borrow_mut().take(), Some((true, true, true)));
        }
    }

    /// With a follower, the menu opened from the warp loop shows the dimmed
    /// portraits the warp frame drew (FUN_0042bc58 @ 0x0041c489).
    #[test]
    fn the_menu_backdrop_keeps_the_guests_portraits() {
        let shots = Rc::new(RefCell::new((None::<Vec<u16>>, None::<Vec<u16>>)));
        let s2 = shots.clone();
        let box_ = Rc::new(Cell::new(None));
        let b2 = box_.clone();
        let (mut locked, mut escape_at) = (0u64, None::<u64>);
        let ran = with_engine(
            "the_menu_backdrop_keeps_the_guests_portraits",
            move |mut e| async move {
                e.load_interface().await;
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.guests.id[0] = 600;
                e.guests.sprite[0] = 0;
                e.guests.count = 1;
                e.guests_shown = true;
                e.guests.portrait_y = 350;
                let spr = &e.ui.guests[0];
                let (x0, y0) = (540 - i32::from(spr.xoff), 350 - i32::from(spr.yoff));
                b2.set(Some((x0, y0, i32::from(spr.w), i32::from(spr.h))));
                e.handle_warp(&mut crate::hooks::NoScripts).await;
            },
            move |sys, f| {
                if escape_at.is_none() && sys.mouse_locked() {
                    locked += 1;
                    if locked == 5 {
                        s2.borrow_mut().0 = Some(sys.screen.borrow().clone());
                        sys.push_input(Input::KeyDown(Key::Escape));
                        sys.push_input(Input::KeyUp(Key::Escape));
                        escape_at = Some(f);
                    }
                }
                match escape_at.map(|e| f - e) {
                    Some(10) => s2.borrow_mut().1 = Some(sys.screen.borrow().clone()),
                    Some(12) => {
                        sys.push_input(Input::MouseMove { x: 2, y: 2 });
                        button(sys, MouseButton::Left, true);
                    }
                    Some(13) => button(sys, MouseButton::Left, false),
                    Some(30) => sys.push_input(Input::Quit),
                    _ => {}
                }
            },
        );
        if !ran {
            return;
        }
        let (warp, menu) = std::mem::take(&mut *shots.borrow_mut());
        let (mut warp, menu) = (warp.unwrap(), menu.unwrap());
        crate::ui::dim_screen(&mut warp);
        let (x0, y0, w, h) = box_.get().unwrap();
        let (mut same, mut all) = (0, 0);
        for y in y0.max(0)..(y0 + h).min(SCREEN_H as i32) {
            for x in x0.max(0)..(x0 + w).min(SCREEN_W as i32) {
                let k = y as usize * SCREEN_W + x as usize;
                all += 1;
                same += usize::from(warp[k] == menu[k]);
            }
        }
        assert!(all > 0 && same == all, "{same}/{all}");
    }

    /// An Escape the warp loop's first poll reads (pressed while a loop
    /// that polls nothing ran before it, here before the loop's entry):
    /// the original reads it at the end of its first frame (0x0041c5f2),
    /// after that frame's portraits (0x0041c489), cursor (0x0041c535) and
    /// present (0x0041c5ed), and FUN_0041c838 (0x0041c618) dims that
    /// buffer: a warp frame is presented before the menu, and the menu's
    /// backdrop keeps the portraits it drew (round 7, ui F2).
    #[test]
    fn an_escape_of_the_first_poll_dims_a_composed_warp_frame() {
        // (locked, screen) of the loop's first frame and of its 10th.
        let shots = Rc::new(RefCell::new(Vec::<(bool, Vec<u16>)>::new()));
        let s2 = shots.clone();
        let box_ = Rc::new(Cell::new(None));
        let b2 = box_.clone();
        // The frames presented before the loop.
        let start = Rc::new(Cell::new(None::<u64>));
        let st2 = start.clone();
        let mut last = 0u64;
        let ran = with_engine(
            "an_escape_of_the_first_poll_dims_a_composed_warp_frame",
            move |mut e| async move {
                e.load_interface().await;
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.guests.id[0] = 600;
                e.guests.sprite[0] = 0;
                e.guests.count = 1;
                e.guests_shown = true;
                e.guests.portrait_y = 350;
                let spr = &e.ui.guests[0];
                let (x0, y0) = (540 - i32::from(spr.xoff), 350 - i32::from(spr.yoff));
                b2.set(Some((x0, y0, i32::from(spr.w), i32::from(spr.h))));
                e.sys.push_input(Input::KeyDown(Key::Escape));
                e.sys.push_input(Input::KeyUp(Key::Escape));
                st2.set(Some(e.sys.frames_presented()));
                e.handle_warp(&mut crate::hooks::NoScripts).await;
            },
            move |sys, _| {
                let Some(n0) = start.get() else {
                    return;
                };
                let n = sys.frames_presented() - n0;
                if n == std::mem::replace(&mut last, n) {
                    return;
                }
                match n {
                    1 | 10 => s2
                        .borrow_mut()
                        .push((sys.mouse_locked(), sys.screen.borrow().clone())),
                    12 => {
                        sys.push_input(Input::MouseMove { x: 2, y: 2 });
                        button(sys, MouseButton::Left, true);
                    }
                    13 => button(sys, MouseButton::Left, false),
                    30 => sys.push_input(Input::Quit),
                    _ => {}
                }
            },
        );
        if !ran {
            return;
        }
        let shots = std::mem::take(&mut *shots.borrow_mut());
        // The loop's first frame is a warp frame (the mouse still locked),
        // the 10th the menu.
        assert_eq!(shots.len(), 2);
        assert!(shots[0].0, "no warp frame before the menu");
        assert!(!shots[1].0, "no menu");
        let mut warp = shots[0].1.clone();
        let menu = &shots[1].1;
        crate::ui::dim_screen(&mut warp);
        let (x0, y0, w, h) = box_.get().unwrap();
        let (mut same, mut all) = (0, 0);
        for y in y0.max(0)..(y0 + h).min(SCREEN_H as i32) {
            for x in x0.max(0)..(x0 + w).min(SCREEN_W as i32) {
                let k = y as usize * SCREEN_W + x as usize;
                all += 1;
                same += usize::from(warp[k] == menu[k]);
            }
        }
        assert!(all > 0 && same == all, "{same}/{all}");
    }

    /// A timer section due on the warp loop's first frame: its line's polls
    /// take what is queued (the left presses counted).
    struct FirstFrameTimer {
        calls: Rc<Cell<u32>>,
        presses: Rc<Cell<u32>>,
    }

    impl ScriptHooks for FirstFrameTimer {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() > 1 {
                return ready(false);
            }
            while let Some(ev) = e.sys.poll_event() {
                if let Input::MouseDown {
                    button: MouseButton::Left,
                    ..
                } = ev
                {
                    self.presses.set(self.presses.get() + 1);
                }
            }
            ready(true)
        }
    }

    /// A left press made before the warp loop (during a follower's slide,
    /// ArcMagique, the pick-up's slide-out, which poll nothing) is still
    /// queued when the loop's first frame runs its timer scan
    /// (`FUN_0042be3c` @ 0x0041c369): the loop's first poll is that frame's
    /// end (0x0041c5f2), so a due /tim section plays first and its line
    /// takes the press; the loop does not end on it.
    #[test]
    fn a_press_queued_before_the_warp_loop_goes_to_the_first_frames_timer() {
        let got = Rc::new(Cell::new(None));
        let g2 = got.clone();
        let calls = Rc::new(Cell::new(0));
        let presses = Rc::new(Cell::new(0));
        let (c2, p2) = (calls.clone(), presses.clone());
        let ran = with_engine(
            "a_press_queued_before_the_warp_loop_goes_to_the_first_frames_timer",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                button(&e.sys, MouseButton::Left, true);
                button(&e.sys, MouseButton::Left, false);
                let mut hooks = FirstFrameTimer {
                    calls: c2,
                    presses: p2,
                };
                g2.set(Some(e.handle_warp(&mut hooks).await));
            },
            |sys, f| {
                if f == 10 {
                    sys.push_input(Input::Quit);
                }
            },
        );
        if !ran {
            return;
        }
        assert_eq!(got.get(), Some(WARP_ABORT), "the loop ended on the press");
        assert!(calls.get() > 1);
        assert_eq!(presses.get(), 1, "the timer's line did not see the press");
    }

    /// A timer section that moved the player while the bar was open (as a
    /// Vue= does: the arrival scan queued) on the bar's first frame; the
    /// arrival scans counted.
    struct BarVue {
        fired: bool,
        arrivals: Rc<Cell<u32>>,
    }

    impl ScriptHooks for BarVue {
        fn arrival_scan<'a>(
            &'a mut self,
            _e: &'a mut Engine,
            _v: u16,
            _a: i32,
        ) -> HookFuture<'a, ()> {
            self.arrivals.set(self.arrivals.get() + 1);
            ready(())
        }

        fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            if self.fired {
                return ready(false);
            }
            self.fired = true;
            e.arrival_scan_pending = true;
            e.arrival_from_vue = 1;
            ready(true)
        }
    }

    /// The warp loop goes on after the bar when a timer section queued the
    /// arrival scan during it (CHAPI028 /tim2 Vue=351): the scan comes
    /// after the next click, not when the bar closes.
    #[test]
    fn bar_timer_vue_keeps_the_warp_loop() {
        let arrivals = Rc::new(Cell::new(0));
        let a2 = arrivals.clone();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s2 = seen.clone();
        let (mut locked_for, mut t0) = (0u64, None::<u64>);
        let ran = with_engine(
            "bar_timer_vue_keeps_the_warp_loop",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                let mut hooks = BarVue {
                    fired: false,
                    arrivals: a2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, f| {
                let Some(t) = t0 else {
                    locked_for = if sys.mouse_locked() {
                        locked_for + 1
                    } else {
                        0
                    };
                    if locked_for == 10 {
                        // In the warp view: open the bar.
                        button(sys, MouseButton::Right, true);
                        t0 = Some(f);
                    }
                    return;
                };
                match f - t {
                    2 => button(sys, MouseButton::Right, false),
                    // Close the bar with a click above it.
                    60 => {
                        sys.push_input(Input::MouseMove { x: 320, y: 100 });
                        button(sys, MouseButton::Left, true);
                    }
                    62 => button(sys, MouseButton::Left, false),
                    200 => {
                        s2.borrow_mut().push((arrivals.get(), sys.mouse_locked()));
                        sys.push_input(Input::Quit);
                    }
                    _ => {}
                }
            },
        );
        if !ran {
            return;
        }
        // The arrival scan of place 1 only; the warp view still runs.
        assert_eq!(*seen.borrow(), [(1, true)]);
    }

    /// A conversation that restarts Time1 (CHAPI024 /280's Time1=0) and the
    /// time1 the timers see on the next frame.
    struct TalkTimer {
        talked: Rc<Cell<bool>>,
        seen: Rc<RefCell<Vec<i32>>>,
    }

    impl ScriptHooks for TalkTimer {
        fn npc_click<'a>(&'a mut self, e: &'a mut Engine, _id: u32, _z: u32) -> HookFuture<'a, ()> {
            let now = e.now();
            e.clocks.restart(1, 0, now);
            self.talked.set(true);
            ready(())
        }

        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            if self.talked.get() && self.seen.borrow().is_empty() {
                self.seen.borrow_mut().push(e.vars.get("time1"));
            }
            ready(false)
        }
    }

    /// After a conversation the timers run on a new frame, whose clocks are
    /// read again (the original's warp loop returns the character, 0x0041c71e,
    /// and is entered again): a Time1=0 in the conversation is seen as 0, not
    /// as the count before it (the guard of CHAPI024 struck at once).
    #[test]
    fn timers_after_a_conversation_read_fresh_clocks() {
        let talked = Rc::new(Cell::new(false));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (t2, s2) = (talked.clone(), seen.clone());
        let found = Rc::new(Cell::new(false));
        let f2 = found.clone();
        let mut locked_for = 0u64;
        let mut clicked_at = None::<u64>;
        let ran = with_engine(
            "timers_after_a_conversation_read_fresh_clocks",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                // A place and a view with a character under the pointer.
                let ids: Vec<u16> = e.wam.places.iter().map(|p| p.id).collect();
                let mut view = None;
                'search: for id in ids {
                    e.next_place_id = Some(id);
                    e.do_place_change().await;
                    if e.npc_rects.is_empty() {
                        continue;
                    }
                    for ia in 0..315 {
                        for ib in -4..=4 {
                            let (a, b) = (ia as f32 * 0.02, ib as f32 * 0.1);
                            e.set_view(a, b);
                            // Both pointer positions the loop may use
                            // (after a panorama load, after a recentre).
                            let hit = [(320, 320), (320, 240)].iter().all(|&(x, y)| {
                                let (px, py) = e.warp.screen_to_panorama(x, y);
                                let (px, py) = (i32::from(px as i16), i32::from(py as i16));
                                npc_hit_test(&e.wam.npcs, &e.npc_rects, &e.sprite_list, id, px, py)
                                    >= 0x100
                            });
                            if hit {
                                view = Some((id, a, b));
                                break 'search;
                            }
                        }
                    }
                }
                let Some((id, a, b)) = view else { return };
                f2.set(true);
                e.next_place_id = Some(id);
                e.do_place_change().await;
                e.set_view(a, b);
                // The count well past any timer's limit before the talk.
                let now = e.now();
                e.clocks.restart(1, 100, now);
                e.update_con_clocks();
                let mut hooks = TalkTimer {
                    talked: t2,
                    seen: s2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, f| {
                if clicked_at.is_none() {
                    locked_for = if sys.mouse_locked() {
                        locked_for + 1
                    } else {
                        0
                    };
                    if locked_for == 10 {
                        button(sys, MouseButton::Left, true);
                        clicked_at = Some(f);
                    }
                    return;
                }
                let t = clicked_at.unwrap();
                if f - t == 2 {
                    button(sys, MouseButton::Left, false);
                }
                if f - t == 60 {
                    sys.push_input(Input::Quit);
                }
            },
        );
        if !ran || !found.get() {
            return;
        }
        assert!(talked.get(), "the character was not clicked");
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0] <= 1,
            "the timers saw time1 = {} after Time1=0",
            seen[0]
        );
    }

    struct ClickPass {
        timers: Rc<Cell<u32>>,
        at_click: Rc<Cell<Option<u32>>>,
    }

    impl ScriptHooks for ClickPass {
        fn warp_frame<'a>(&'a mut self, _e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.timers.set(self.timers.get() + 1);
            ready(false)
        }

        fn zone_click<'a>(&'a mut self, _e: &'a mut Engine, _z: u32) -> HookFuture<'a, ()> {
            if self.at_click.get().is_none() {
                self.at_click.set(Some(self.timers.get()));
            }
            ready(())
        }
    }

    /// The poll that reads a left click ends the original's pass (the bow
    /// step and FUN_00432178 follow it) and the loop at its head
    /// (0x0041c028): no timer scan (FUN_0042be3c @ 0x0041c369) runs
    /// between the click and its handling.
    #[test]
    fn no_timer_pass_between_a_click_and_its_handling() {
        let timers = Rc::new(Cell::new(0u32));
        let at_click = Rc::new(Cell::new(None));
        let pushed = Rc::new(Cell::new(None::<u32>));
        let (t2, a2, t3, a3, p2) = (
            timers.clone(),
            at_click.clone(),
            timers.clone(),
            at_click.clone(),
            pushed.clone(),
        );
        let mut locked_for = 0u64;
        let ran = with_engine(
            "no_timer_pass_between_a_click_and_its_handling",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                let mut hooks = ClickPass {
                    timers: t2,
                    at_click: a2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, _f| {
                if a3.get().is_some() {
                    sys.push_input(Input::Quit);
                    return;
                }
                if p2.get().is_some() {
                    locked_for += 1;
                    if locked_for == 2 {
                        button(sys, MouseButton::Left, false);
                    }
                    return;
                }
                locked_for = if sys.mouse_locked() {
                    locked_for + 1
                } else {
                    0
                };
                if locked_for == 10 {
                    p2.set(Some(t3.get()));
                    button(sys, MouseButton::Left, true);
                    locked_for = 0;
                }
            },
        );
        if ran {
            assert_eq!(at_click.get().unwrap() - pushed.get().unwrap(), 0);
        }
    }

    struct RequestThenClick {
        arm: Rc<Cell<u8>>,
        clicked: Rc<Cell<bool>>,
        bar: Rc<Cell<bool>>,
    }

    impl ScriptHooks for RequestThenClick {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            // A timer section whose line saw a right press: the request
            // stays for the next pass.
            if self.arm.get() == 1 {
                e.bar.request = true;
                self.arm.set(2);
            }
            ready(false)
        }

        fn zone_click<'a>(&'a mut self, _e: &'a mut Engine, _z: u32) -> HookFuture<'a, ()> {
            self.clicked.set(true);
            ready(())
        }

        fn bar_timers<'a>(&'a mut self, _e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.bar.set(true);
            ready(false)
        }
    }

    /// A bar request left by a timer section and a click in the next pass:
    /// the original's inner loop ends on the request before its head reads
    /// the click (FUN_0041bfa0, `while (0x004960c4 != 1)`), opens the bar
    /// and clears the click flag 0x0049611c. The bar opens, the click is
    /// lost.
    #[test]
    fn a_pending_bar_request_takes_the_click() {
        let arm = Rc::new(Cell::new(0u8));
        let clicked = Rc::new(Cell::new(false));
        let bar = Rc::new(Cell::new(false));
        let (a2, c2, b2) = (arm.clone(), clicked.clone(), bar.clone());
        let (a3, c3, b3) = (arm.clone(), clicked.clone(), bar.clone());
        let mut locked = 0u64;
        let mut pressed = false;
        let ran = with_engine(
            "a_pending_bar_request_takes_the_click",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                let mut hooks = RequestThenClick {
                    arm: a2,
                    clicked: c2,
                    bar: b2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, _f| {
                if b3.get() || c3.get() {
                    sys.push_input(Input::Quit);
                    return;
                }
                if a3.get() == 2 && !pressed {
                    pressed = true;
                    button(sys, MouseButton::Left, true);
                    return;
                }
                locked = if sys.mouse_locked() { locked + 1 } else { 0 };
                if locked == 10 && a3.get() == 0 {
                    a3.set(1);
                }
            },
        );
        if ran {
            assert!(bar.get(), "the bar did not open");
            assert!(!clicked.get(), "the click was handled");
        }
    }

    /// A timer section on the first warp frame during whose lines the
    /// mouse moves 280 px right; the view's yaw then and 40 frames later.
    struct MouseDuringTimer {
        calls: u32,
        alpha: Rc<RefCell<Vec<f32>>>,
    }

    impl ScriptHooks for MouseDuringTimer {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.calls += 1;
            if self.calls == 1 {
                self.alpha.borrow_mut().push(e.warp.alpha());
                // As Scripts::timer_scan: the mouse at the Windows cursor.
                e.sys.set_mouse_locked(false);
                e.sys.set_mouse_pos(e.pointer.cursor.0, e.pointer.cursor.1);
                e.mark_pointer_mouse();
                let (x, y) = e.pointer.cursor;
                e.sys.push_input(Input::MouseMove { x: x + 280, y });
                return ready(true);
            }
            if self.calls == 41 {
                self.alpha.borrow_mut().push(e.warp.alpha());
            }
            ready(false)
        }
    }

    /// The look delta is previous point minus pointer (0x0041c1c3..
    /// 0x0041c20d), and a timer section (step 6, after step 5) writes
    /// neither: the mouse's motion during its lines turns the view on the
    /// frames after it (280 px to the right: 280 / 600 rad to the left).
    #[test]
    fn the_mouse_moved_during_a_timer_section_turns_the_view() {
        let alpha = Rc::new(RefCell::new(Vec::new()));
        let (a2, a3) = (alpha.clone(), alpha.clone());
        let ran = with_engine(
            "the_mouse_moved_during_a_timer_section_turns_the_view",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                let mut hooks = MouseDuringTimer {
                    calls: 0,
                    alpha: a2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, _| {
                if a3.borrow().len() == 2 {
                    sys.push_input(Input::Quit);
                }
            },
        );
        if !ran {
            return;
        }
        let a = alpha.borrow();
        let turn = f64::from(a[0]) - f64::from(a[1]);
        let turn = turn.rem_euclid(std::f64::consts::TAU);
        assert!((0.4..0.47).contains(&turn), "turned by {turn}");
    }

    /// The yaw at every warp frame's timer step.
    struct RecordAlpha {
        alpha: Rc<RefCell<Vec<f32>>>,
    }

    impl ScriptHooks for RecordAlpha {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.alpha.borrow_mut().push(e.warp.alpha());
            ready(false)
        }
    }

    /// After a puzzle the panorama load `FUN_0044a424` leaves the pointer
    /// at (0x140, 0x140) and the Windows cursor where the mouse was
    /// (0x0044a70b / 0x0044a715, no SetCursorPos); the warp loop's entry
    /// takes the previous point from the pointer (0x0041c003 / 0x0041c00d)
    /// and moves no cursor. With the view at rest the first frames do not
    /// recentre, and the first motion's WM_MOUSEMOVE (0x00419c10..
    /// 0x00419c29) puts the pointer at the cursor plus that motion: a
    /// motion of 1 px with the mouse at (600, 450) gives the delta
    /// 320 - 601, the view turns by about 281 / 600 rad (round 7).
    #[test]
    fn the_first_motion_after_a_puzzle_swings_the_view() {
        let alpha = Rc::new(RefCell::new(Vec::new()));
        let (a2, a3) = (alpha.clone(), alpha.clone());
        let moved = Rc::new(Cell::new(false));
        let ran = with_engine(
            "the_first_motion_after_a_puzzle_swings_the_view",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.set_view(1.0, 0.0);
                // The view at rest: the stored angles are the view's.
                e.pointer.prev_alpha = e.warp.alpha();
                e.pointer.prev_beta = e.warp.beta();
                // The puzzle left the mouse at (600, 450).
                e.sys.set_mouse_pos(600, 450);
                e.reload_view_after_puzzle().await;
                assert_eq!(e.pointer.pos, (320, 320));
                let mut hooks = RecordAlpha { alpha: a2 };
                e.handle_warp(&mut hooks).await;
            },
            move |sys, _| {
                let n = a3.borrow().len();
                if n == 3 && !moved.get() {
                    moved.set(true);
                    sys.push_input(Input::MouseDelta { dx: 1, dy: 0 });
                }
                if n == 80 {
                    sys.push_input(Input::Quit);
                }
            },
        );
        if !ran {
            return;
        }
        let a = alpha.borrow();
        // At rest until the motion.
        assert_eq!(a[0], a[2]);
        let turn = (f64::from(a[0]) - f64::from(a[a.len() - 1])).abs();
        let turn = turn.min(std::f64::consts::TAU - turn);
        assert!((0.4..0.5).contains(&turn), "turned by {turn}");
    }

    /// A timer section that leaves the bar request (a right press during
    /// its line) and clears the shown cursor; at the bar's first frame,
    /// the cursor shown then.
    struct RequestLeft {
        arm: Rc<Cell<u8>>,
        bar: Rc<Cell<Option<i32>>>,
    }

    impl ScriptHooks for RequestLeft {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            if self.arm.get() == 1 {
                e.bar.request = true;
                e.cursor_shown = CURSOR_UNSET;
                self.arm.set(2);
            }
            ready(false)
        }

        fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            if self.bar.get().is_none() {
                self.bar.set(Some(e.cursor_shown));
            }
            ready(false)
        }
    }

    /// A bar request left by a timer section, then `right` a right press in
    /// the next poll; whether the bar opened and the cursor shown then.
    fn request_left(right: bool) -> Option<Option<i32>> {
        let arm = Rc::new(Cell::new(0u8));
        let bar = Rc::new(Cell::new(None));
        let (a2, b2, a3, b3) = (arm.clone(), bar.clone(), arm.clone(), bar.clone());
        let mut locked = 0u64;
        let mut after = 0u64;
        let ran = with_engine(
            "a_bar_request_left_by_a_timer",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                let mut hooks = RequestLeft { arm: a2, bar: b2 };
                e.game_step(&mut hooks).await;
            },
            move |sys, _f| {
                if b3.get().is_some() || after > 30 {
                    sys.push_input(Input::Quit);
                    return;
                }
                if a3.get() == 2 {
                    if after == 0 && right {
                        button(sys, MouseButton::Right, true);
                    }
                    after += 1;
                    return;
                }
                locked = if sys.mouse_locked() { locked + 1 } else { 0 };
                if locked == 10 && a3.get() == 0 {
                    a3.set(1);
                }
            },
        );
        ran.then(|| bar.get())
    }

    /// Both cursor calls of the warp frame are gated on 0x004960c4 == 0
    /// (0x0041c4ca, 0x0041b024): the frame in which a timer section leaves
    /// a bar request steps and shows no cursor; the bar opens at its end.
    #[test]
    fn no_cursor_in_the_frame_that_leaves_a_bar_request() {
        let Some(bar) = request_left(false) else {
            return;
        };
        assert_eq!(bar, Some(CURSOR_UNSET));
    }

    /// The same frame's poll (0x0041c5f2) toggles that request on a right
    /// press (FUN_0041a298 l. 11417) and the inner loop ends only on 1
    /// (l. 12748): the bar does not open.
    #[test]
    fn a_right_press_cancels_a_bar_request_left_by_a_timer() {
        let Some(bar) = request_left(true) else {
            return;
        };
        assert_eq!(bar, None, "the bar opened");
    }

    /// A timer section that runs Vue=`to` on the first warp frame (the place
    /// entered, ArriveeVue = the place left, the arrival scan queued, the
    /// view aimed at an exit), and a departure scan that runs a chapter=
    /// (an arrival section matched by ArriveeVue, CHAPI059 /010); the scans
    /// as they come: ("dep", destination, ArriveeVue) and ("arr", place,
    /// ArriveeVue).
    struct TimerVue {
        to: u16,
        view: (f32, f32),
        fired: Rc<Cell<bool>>,
        scans: Scans,
    }

    /// (scan, place, ArriveeVue) in order.
    type Scans = Rc<RefCell<Vec<(&'static str, i32, i32)>>>;

    impl ScriptHooks for TimerVue {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            Box::pin(async move {
                if self.fired.get() {
                    return false;
                }
                self.fired.set(true);
                let from = e.current_place_id.unwrap_or(0);
                e.last_vue = from;
                e.next_place_id = Some(self.to);
                e.do_place_change().await;
                e.set_view(self.view.0, self.view.1);
                e.vars.set("arriveevue", i32::from(from));
                e.arrival_scan_pending = true;
                e.arrival_from_vue = from;
                true
            })
        }

        fn arrival_scan<'a>(
            &'a mut self,
            _e: &'a mut Engine,
            vue: u16,
            arrivee: i32,
        ) -> HookFuture<'a, ()> {
            self.scans
                .borrow_mut()
                .push(("arr", i32::from(vue), arrivee));
            ready(())
        }

        fn departure_scan<'a>(
            &'a mut self,
            e: &'a mut Engine,
            dest: u16,
        ) -> HookFuture<'a, Departure> {
            self.scans
                .borrow_mut()
                .push(("dep", i32::from(dest), e.vars.get("arriveevue")));
            // A chapter= in the section: the arrival scan queued again.
            e.arrival_scan_pending = true;
            e.arrival_from_vue = e.last_vue;
            ready(Departure::Go)
        }
    }

    /// After a timer section's Vue= (CHAPI028 /300 `Vue=351`), the first
    /// click on an exit of the place entered moves at once (FUN_00423ad8
    /// tests ARRIVEEVUE only at the top of the pass, @ 0x00423b4d): the
    /// departure scan sees ArriveeVue = the place the Vue= left, a
    /// chapter= in it does not stop the move, and the timer's pending
    /// arrival scan never runs: the next one is the destination's, with
    /// ArriveeVue = the place left by the move (0x00423def).
    #[test]
    fn exit_click_after_a_timer_vue_moves_at_once() {
        let fired = Rc::new(Cell::new(false));
        let scans = Rc::new(RefCell::new(Vec::new()));
        let (f2, s2) = (fired.clone(), scans.clone());
        let (f3, s3) = (fired.clone(), scans.clone());
        let target = Rc::new(Cell::new(None::<(u16, u16)>));
        let t2 = target.clone();
        let mut locked = 0u64;
        let mut pressed = None::<u64>;
        let ran = with_engine(
            "exit_click_after_a_timer_vue_moves_at_once",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                // A place other than 1 and a view with one of its exits under
                // both pointer positions the loop may use (after a panorama
                // load, after a recentre).
                let ids: Vec<u16> = e.wam.places.iter().map(|p| p.id).collect();
                let mut found = None;
                'search: for id in ids.into_iter().filter(|&id| id != 1) {
                    e.next_place_id = Some(id);
                    e.do_place_change().await;
                    let Some(place) = e.place().cloned() else {
                        continue;
                    };
                    for ia in 0..315 {
                        for ib in -4..=4 {
                            let (a, b) = (ia as f32 * 0.02, ib as f32 * 0.1);
                            e.set_view(a, b);
                            let zones: Vec<u16> = [(320, 320), (320, 240)]
                                .iter()
                                .map(|&(x, y)| {
                                    let (px, py) = e.warp.screen_to_panorama(x, y);
                                    place.hit_test(px as i16, py as i16)
                                })
                                .collect();
                            if zones[0] == zones[1]
                                && place.transitions.iter().any(|t| t.dst_place == zones[0])
                            {
                                found = Some((id, zones[0], (a, b)));
                                break 'search;
                            }
                        }
                    }
                }
                let (to, exit, view) = found.expect("a place with an exit in view");
                t2.set(Some((to, exit)));
                e.next_place_id = Some(1);
                let mut hooks = TimerVue {
                    to,
                    view,
                    fired: f2,
                    scans: s2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, f| {
                if let Some(t) = pressed {
                    if f == t + 2 {
                        button(sys, MouseButton::Left, false);
                    }
                    if s3.borrow().len() >= 3 || f > t + 2000 {
                        sys.push_input(Input::Quit);
                    }
                    return;
                }
                locked = if sys.mouse_locked() { locked + 1 } else { 0 };
                if f3.get() && locked >= 10 {
                    button(sys, MouseButton::Left, true);
                    pressed = Some(f);
                }
            },
        );
        if !ran {
            return;
        }
        let (to, exit) = target.get().unwrap();
        let scans = scans.borrow();
        assert!(scans.len() >= 3, "{scans:?}");
        assert_eq!((scans[0].0, scans[0].1), ("arr", 1), "{scans:?}");
        assert_eq!(
            scans[1..3],
            [
                ("dep", i32::from(exit), 1),
                ("arr", i32::from(exit), i32::from(to)),
            ],
            "{scans:?}"
        );
    }

    struct ClickAnim(Rc<Cell<Option<bool>>>);

    impl ScriptHooks for ClickAnim {
        fn zone_click<'a>(&'a mut self, e: &'a mut Engine, _z: u32) -> HookFuture<'a, ()> {
            self.0.set(Some(e.cursor_state.click_anim));
            ready(())
        }
    }

    /// `FUN_004278a0` starts the cursor's click animation at its entry
    /// (0x004968b8 = 1 @ 0x004278df), for every click the place loop
    /// hands it (0x00423d12), not only on a character or an object.
    #[test]
    fn every_click_starts_the_click_animation() {
        let got = Rc::new(Cell::new(None));
        let (g2, g3) = (got.clone(), got.clone());
        let mut locked = 0u64;
        let mut pressed = None::<u64>;
        let ran = with_engine(
            "every_click_starts_the_click_animation",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.game_step(&mut ClickAnim(g2)).await;
            },
            move |sys, f| {
                if g3.get().is_some() {
                    sys.push_input(Input::Quit);
                    return;
                }
                if let Some(t) = pressed {
                    if f == t + 2 {
                        button(sys, MouseButton::Left, false);
                    }
                    return;
                }
                locked = if sys.mouse_locked() { locked + 1 } else { 0 };
                if locked == 10 {
                    button(sys, MouseButton::Left, true);
                    pressed = Some(f);
                }
            },
        );
        if ran {
            assert_eq!(got.get(), Some(true));
        }
    }

    /// On its first frame, a timer section turns the view from `start`
    /// to `to`; on the next one, the cursor shown.
    struct TurnView {
        to: (f32, f32),
        calls: u32,
        shown: Rc<Cell<Option<i32>>>,
    }

    impl ScriptHooks for TurnView {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            self.calls += 1;
            if self.calls == 1 {
                e.set_view(self.to.0, self.to.1);
            } else if self.calls == 2 {
                self.shown.set(Some(e.cursor_shown));
            }
            ready(false)
        }
    }

    /// `FUN_00422fcc` runs after the look, the timers and the render
    /// (0x0041c4b4) and does its own hit tests at its entry: the cursor of
    /// a frame shows what is under the pointer at that frame's view. A
    /// timer that turns the view off a navigation zone leaves no
    /// navigate cursor.
    #[test]
    fn the_cursor_shows_the_frames_own_target() {
        let shown = Rc::new(Cell::new(None));
        let (s2, s3) = (shown.clone(), shown.clone());
        let ran = with_engine(
            "the_cursor_shows_the_frames_own_target",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                // Under both pointer positions the loop may use: a
                // navigation zone and no character, then nothing at all.
                let targets = |e: &mut Engine, a: f32, b: f32| {
                    e.set_view(a, b);
                    let t: Vec<(u32, u32)> = [(320, 320), (320, 240)]
                        .iter()
                        .map(|&p| {
                            e.pointer.pos = p;
                            e.pointer_targets()
                        })
                        .collect();
                    (t[0] == t[1]).then_some(t[0])
                };
                let views: Vec<(f32, f32)> = (0..315)
                    .flat_map(|ia| (-4..=4).map(move |ib| (ia as f32 * 0.02, ib as f32 * 0.1)))
                    .collect();
                let zone = views.iter().copied().find(|&(a, b)| {
                    matches!(targets(&mut e, a, b), Some((z, 0)) if (1..0x200).contains(&z))
                });
                let none = views
                    .iter()
                    .copied()
                    .find(|&(a, b)| targets(&mut e, a, b) == Some((0, 0)));
                let (Some(start), Some(to)) = (zone, none) else {
                    panic!("no navigation view or no empty view in place 1");
                };
                e.set_view(start.0, start.1);
                e.pointer.pos = (320, 240);
                let mut hooks = TurnView {
                    to,
                    calls: 0,
                    shown: s2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, _| {
                if s3.get().is_some() {
                    sys.push_input(Input::Quit);
                }
            },
        );
        if ran {
            assert_eq!(shown.get(), Some(-1));
        }
    }

    /// The object in hand is drawn before the guests' portraits
    /// (0x0041c45a, then `FUN_0042bc58` @ 0x0041c489): over a portrait,
    /// the portrait is drawn over it.
    #[test]
    fn the_held_object_is_drawn_under_the_portraits() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "the_held_object_is_drawn_under_the_portraits",
            move |mut e| async move {
                e.load_ui_assets().await;
                let guest = e.game.exe.guest_sprite_ids[0];
                let table = e.game.exe.guest_sprite_ids.clone();
                e.guests.add(guest, &table);
                e.guests_shown = true;
                e.warp_loaded = true;
                // No cursor sprite: the icon and the portraits only.
                e.cursor_sprites.clear();
                let object = e
                    .game
                    .exe
                    .inventory_object_ids
                    .iter()
                    .copied()
                    .find(|&id| {
                        id != 0
                            && crate::ui::object_icon(&e.game.exe.inventory_object_ids, id)
                                .and_then(|i| e.ui.object(i))
                                .is_some()
                    })
                    .expect("an object with an icon");
                e.vars.set("objetenmain", object);
                // The pointer in the middle of the portrait at (540, 350).
                e.pointer.pos = (580, 400);
                e.sys.screen.borrow_mut().fill(0);
                let blank = e.sys.screen.borrow().clone();
                // The icon, then the portraits over it.
                let mut under = blank.clone();
                e.draw_held_icon(&mut under);
                e.draw_guests(&mut under, None);
                // The portraits, then the icon over them.
                let mut over = blank.clone();
                e.draw_guests(&mut over, None);
                e.draw_held_icon(&mut over);
                e.draw_warp_overlays();
                let frame = e.sys.screen.borrow().clone();
                *g2.borrow_mut() = Some((frame == under, under != over));
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        // The icon overlaps the portrait, and the frame has it under.
        assert_eq!(got.borrow().unwrap(), (true, true));
    }

    /// A guest's conversation from the bar that sets Time1 (CHAPI032
    /// /419's time1=20000); the time1 the timers see next.
    struct GuestTimer {
        talked: Rc<Cell<bool>>,
        seen: Rc<RefCell<Vec<i32>>>,
    }

    impl ScriptHooks for GuestTimer {
        fn talk_to_guest<'a>(&'a mut self, e: &'a mut Engine, _id: i32) -> HookFuture<'a, ()> {
            let now = e.now();
            e.clocks.restart(1, 20000, now);
            self.talked.set(true);
            ready(())
        }

        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            if self.talked.get() && self.seen.borrow().is_empty() {
                self.seen.borrow_mut().push(e.vars.get("time1"));
            }
            ready(false)
        }
    }

    /// The timers after a guest's conversation from the bar read the
    /// clocks again (the original's next frame, FUN_0041c75c @ 0x0041c086):
    /// a Time1=20000 in it is seen at once.
    #[test]
    fn timers_after_a_bar_conversation_read_fresh_clocks() {
        let talked = Rc::new(Cell::new(false));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (t2, s2) = (talked.clone(), seen.clone());
        let (t3, s3) = (talked.clone(), seen.clone());
        let mut locked = 0u64;
        let mut opened = None::<u64>;
        let ran = with_engine(
            "timers_after_a_bar_conversation_read_fresh_clocks",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                let table = e.game.exe.guest_sprite_ids.clone();
                e.guests.add(table[0], &table);
                e.guests_shown = true;
                e.next_place_id = Some(1);
                let mut hooks = GuestTimer {
                    talked: t2,
                    seen: s2,
                };
                e.game_step(&mut hooks).await;
            },
            move |sys, f| {
                if !s3.borrow().is_empty() || (t3.get() && opened.is_some_and(|t| f > t + 300)) {
                    sys.push_input(Input::Quit);
                    return;
                }
                let Some(t) = opened else {
                    locked = if sys.mouse_locked() { locked + 1 } else { 0 };
                    if locked == 10 {
                        button(sys, MouseButton::Right, true);
                        opened = Some(f);
                    }
                    return;
                };
                match f - t {
                    2 => button(sys, MouseButton::Right, false),
                    // The guest's portrait, raised with the bar: the
                    // pointer a pass before the press (the bar reads the
                    // pointer of the pass before, @ 0x0041a2b5).
                    58 => sys.push_input(Input::MouseMove { x: 580, y: 340 }),
                    60 => button(sys, MouseButton::Left, true),
                    62 => button(sys, MouseButton::Left, false),
                    _ => {}
                }
            },
        );
        if !ran {
            return;
        }
        assert!(talked.get(), "no conversation");
        assert_eq!(seen.borrow().len(), 1);
        assert!(seen.borrow()[0] >= 20000, "time1 = {}", seen.borrow()[0]);
    }

    /// The view's alpha after the warp loop of place 1, with the left
    /// arrow held for 20 frames (`press`), under `x`.
    fn alpha_after_left_arrow(x: crate::enhancements::Enhancements, press: bool) -> Option<f32> {
        let got = Rc::new(Cell::new(None));
        let g2 = got.clone();
        let mut locked = 0u64;
        let ran = with_engine(
            "arrow_keys_turn_the_view_unless_original",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.set_enhancements(x);
                e.next_place_id = Some(1);
                let mut hooks = crate::hooks::NoScripts;
                e.game_step(&mut hooks).await;
                g2.set(Some(e.warp.alpha()));
            },
            move |sys, _| {
                locked = if sys.mouse_locked() {
                    locked + 1
                } else {
                    locked
                };
                if locked == 5 && press {
                    sys.push_input(Input::KeyDown(Key::Left));
                } else if locked == 25 {
                    sys.push_input(Input::KeyUp(Key::Left));
                    sys.push_input(Input::Quit);
                }
            },
        );
        if ran {
            got.get()
        } else {
            None
        }
    }

    /// The view's alpha after the warp loop of place 1 in bow mode, the
    /// left button pressed and held, with F5 held too (`f5`).
    fn alpha_with_the_left_button_held(f5: bool) -> Option<f32> {
        let got = Rc::new(Cell::new(None));
        let g2 = got.clone();
        let mut locked = 0u64;
        let ran = with_engine(
            "f5_turns_the_view_with_the_left_button_held",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.bow_mode = true;
                e.next_place_id = Some(1);
                e.game_step(&mut crate::hooks::NoScripts).await;
                g2.set(Some(e.warp.alpha()));
            },
            move |sys, _| {
                locked = if sys.mouse_locked() {
                    locked + 1
                } else {
                    locked
                };
                if locked == 5 {
                    button(sys, MouseButton::Left, true);
                    if f5 {
                        sys.push_input(Input::KeyDown(Key::F(5)));
                    }
                } else if locked == 25 {
                    sys.push_input(Input::KeyUp(Key::F(5)));
                    button(sys, MouseButton::Left, false);
                    sys.push_input(Input::Quit);
                }
            },
        );
        if ran {
            got.get()
        } else {
            None
        }
    }

    /// F5..F8 are read whenever the poll saw no new left press
    /// (`FUN_0041a298` @ 0x0041a2b5; VK_LBUTTON is an edge
    /// key): holding the left button (in bow mode, where the press does not
    /// end the loop) does not stop them.
    #[test]
    fn f5_turns_the_view_with_the_left_button_held() {
        let Some(still) = alpha_with_the_left_button_held(false) else {
            return;
        };
        let turned = alpha_with_the_left_button_held(true).unwrap();
        assert_ne!(turned, still);
    }

    /// FUN_0041bfa0 returns from its exit block (0x0041c71e) without
    /// waiting for the release: the warp loop ended by a left press held
    /// 500 ms returns with the button still down.
    #[test]
    fn the_warp_loop_returns_with_the_button_still_held() {
        let got = Rc::new(Cell::new(None));
        let g2 = got.clone();
        let (mut locked, mut pressed_at) = (0u64, None::<u64>);
        let ran = with_engine(
            "the_warp_loop_returns_with_the_button_still_held",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.handle_warp(&mut crate::hooks::NoScripts).await;
                g2.set(Some(e.sys.buttons().0));
            },
            move |sys, f| {
                if pressed_at.is_none() && sys.mouse_locked() {
                    locked += 1;
                    if locked == 5 {
                        button(sys, MouseButton::Left, true);
                        pressed_at = Some(f);
                    }
                }
                if pressed_at.is_some_and(|p| f == p + 30) {
                    button(sys, MouseButton::Left, false);
                }
            },
        );
        if ran {
            assert_eq!(got.get(), Some(true));
        }
    }

    /// The arrow keys turn the view as F5..F8 by default
    /// (`Enhancements::arrow_keys`, a port convenience); with `--original`
    /// they do not, as the original's poll reads F5..F8 only
    /// (`FUN_0041a298` @ 0x0041a33c..0x0041a3ba).
    #[test]
    fn arrow_keys_turn_the_view_unless_original() {
        use crate::enhancements::Enhancements;
        let Some(still) = alpha_after_left_arrow(Enhancements::default(), false) else {
            return;
        };
        let turned = alpha_after_left_arrow(Enhancements::default(), true).unwrap();
        let original = alpha_after_left_arrow(Enhancements::original(), true).unwrap();
        let original_still = alpha_after_left_arrow(Enhancements::original(), false).unwrap();
        assert_ne!(turned, still);
        assert_eq!(original, original_still);
    }

    /// At each warp frame's timer scan: TirFleche, the arrow frame and bow
    /// mode; at call `press_at` a left press (and the view's tilt set to
    /// `tilt_then`), at `escape_at` a right press and an Escape.
    struct BowRecord {
        seen: Rc<RefCell<Vec<(i32, u8, bool)>>>,
        press_at: usize,
        tilt_then: Option<f32>,
        right_escape_at: usize,
    }

    impl ScriptHooks for BowRecord {
        fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
            let n = self.seen.borrow().len();
            self.seen
                .borrow_mut()
                .push((e.vars.get("tirfleche"), e.arrow_frame, e.bow_mode));
            if n == self.press_at {
                if let Some(beta) = self.tilt_then {
                    e.set_view(e.warp.alpha(), beta);
                }
                button(&e.sys, MouseButton::Left, true);
            }
            if n == self.right_escape_at {
                button(&e.sys, MouseButton::Right, true);
                e.sys.push_input(Input::KeyDown(Key::Escape));
                e.sys.push_input(Input::KeyUp(Key::Escape));
            }
            ready(false)
        }
    }

    /// The warp loop in bow mode in place 1 with the view tilted to
    /// `tilt` degrees; the timer-scan records of its first passes.
    fn bow_run(
        test: &str,
        tilt: f32,
        press_at: usize,
        tilt_then: Option<f32>,
        right_escape_at: usize,
    ) -> Option<Vec<(i32, u8, bool)>> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (s2, s3) = (seen.clone(), seen.clone());
        let rad = |deg: f32| deg / 180.0 * std::f32::consts::PI;
        let mut frames = 0u32;
        let ran = with_engine(
            test,
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.set_view(1.0, rad(tilt));
                e.pointer.prev_alpha = e.warp.alpha();
                e.pointer.prev_beta = e.warp.beta();
                e.bow_mode = true;
                let mut hooks = BowRecord {
                    seen: s2,
                    press_at,
                    tilt_then: tilt_then.map(rad),
                    right_escape_at,
                };
                e.handle_warp(&mut hooks).await;
            },
            move |sys, _| {
                frames += 1;
                let n = s3.borrow().len();
                if n == press_at + 3 {
                    button(sys, MouseButton::Left, false);
                }
                if n == right_escape_at + 3 {
                    button(sys, MouseButton::Right, false);
                }
                if n >= 12 || frames > 400 {
                    sys.push_input(Input::Quit);
                }
            },
        );
        ran.then(|| seen.borrow().clone())
    }

    /// FUN_0041bfa0 tests a bow click right after the frame's poll
    /// (0x0041c61d..0x0041c653) and steps the arrow frame (0x0041c678)
    /// before the next frame's timer scan (0x0041c369): the timer section
    /// of the pass whose poll read the click sees TirFleche = 1 and arrow
    /// frame 1 (round 9, navigation).
    #[test]
    fn a_bow_shot_is_seen_by_the_timer_scan_of_its_pass() {
        let Some(seen) = bow_run(
            "a_bow_shot_is_seen_by_the_timer_scan_of_its_pass",
            10.5,
            2,
            None,
            usize::MAX,
        ) else {
            return;
        };
        assert_eq!(seen[2].0, 0, "{seen:?}");
        assert_eq!(seen[3], (1, 1, true), "{seen:?}");
        assert_eq!(seen[4], (1, 2, true), "{seen:?}");
    }

    /// The shot's AngleWarpY < 20 test reads the angle the original's frame
    /// started with (the local [ebp-8], 0x0041c0e3 / 0x0041c637), not the
    /// one of the frame after: a view tilted from 21 to 19 degrees in the
    /// frame whose poll reads the click shoots nothing.
    #[test]
    fn a_bow_shot_tests_the_angle_its_frame_started_with() {
        let Some(seen) = bow_run(
            "a_bow_shot_tests_the_angle_its_frame_started_with",
            21.5,
            2,
            Some(19.5),
            usize::MAX,
        ) else {
            return;
        };
        assert!(seen.len() >= 8, "{seen:?}");
        assert!(
            seen.iter().all(|&(tir, frame, _)| tir == 0 && frame == 0),
            "{seen:?}"
        );
    }

    /// A right press and an Escape in one poll in bow mode: FUN_0041a298
    /// sets no menu request with the bow up (0x00496164 == 0, dc:11436),
    /// and the loop only lowers the bow (FUN_0042d368 @ 0x0041c6c8): the
    /// warp loop goes on, no in-game menu (round 9, navigation).
    #[test]
    fn right_press_and_escape_in_bow_mode_only_lower_the_bow() {
        let Some(seen) = bow_run(
            "right_press_and_escape_in_bow_mode_only_lower_the_bow",
            10.5,
            usize::MAX,
            None,
            2,
        ) else {
            return;
        };
        assert!(seen.len() >= 12, "the warp loop stopped: {seen:?}");
        assert!(seen[2].2);
        assert!(!seen[seen.len() - 1].2, "{seen:?}");
    }

    /// The motion of the frame whose poll saw the click that ends the warp
    /// loop: `FUN_0041a298` returned before its pump (0x0041a2b5), the
    /// motion moved the Windows cursor and its WM_MOUSEMOVE waits. With
    /// the pointer apart from the cursor (after a panorama load), a puzzle
    /// entered on that click keeps the pointer for its entry read (no poll
    /// of the original before its first present) and its first pumping
    /// read puts it at the cursor plus that motion (round 9, puzzles-1).
    #[test]
    fn the_click_frames_motion_reaches_the_puzzles_pointer() {
        let got = Rc::new(Cell::new(None));
        let g2 = got.clone();
        let frames = Rc::new(Cell::new(0u32));
        let f2 = frames.clone();
        let ran = with_engine(
            "the_click_frames_motion_reaches_the_puzzles_pointer",
            move |mut e| async move {
                e.sys.set_mouse_pos(600, 450);
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.set_view(1.0, 0.0);
                e.pointer.prev_alpha = e.warp.alpha();
                e.pointer.prev_beta = e.warp.beta();
                assert_eq!(e.pointer.pos, (320, 320));
                let mut hooks = RecordAlpha {
                    alpha: Rc::new(RefCell::new(Vec::new())),
                };
                e.handle_warp(&mut hooks).await;
                let at_exit = (e.pointer.pos, e.pointer.cursor, e.sys.mouse());
                let mut host = crate::puzzle::PuzzleHost::new(&mut e, &mut hooks);
                let entry = host.read_entry();
                let first = host.read_input();
                g2.set(Some((at_exit, (entry.x, entry.y), (first.x, first.y))));
            },
            move |sys, _| {
                if !sys.mouse_locked() {
                    return;
                }
                let n = f2.get() + 1;
                f2.set(n);
                if n == 5 {
                    sys.push_input(Input::MouseDelta { dx: 7, dy: -3 });
                    button(sys, MouseButton::Left, true);
                }
                if n == 6 {
                    button(sys, MouseButton::Left, false);
                }
            },
        );
        if !ran {
            return;
        }
        let (at_exit, entry, first) = got.get().expect("the warp loop did not end");
        assert_eq!(at_exit, ((320, 320), (600, 450), (607, 447)));
        assert_eq!(entry, (320, 320));
        assert_eq!(first, (607, 447));
    }
}
