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

use crate::cursor::{warp_cursor_step, WarpCursorInput};
use crate::engine::{Engine, CURSOR_UNSET, WARP_ABORT};
use crate::hooks::ScriptHooks;
use crate::look::warp_look_frame;
use crate::sys::{Input, Key, MouseButton};
use crate::warp::{SCREEN_H, SCREEN_W};

/// The view keys' step, 0.034906 rad (2 degrees) on every poll they are
/// held (`FUN_0041a298` @ 0x0041a33c..0x0041a3ba).
const VIEW_KEY_STEP: f64 = 0.034906;

/// The cursor animation's step (the port's pace of `FUN_00422fcc`).
const CURSOR_STEP_MS: u64 = 80;

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

    /// Draws the cursor at the pointer: the object in hand (0x0041c45a),
    /// then the cursor sprite `FUN_00422fcc` chose over it.
    fn draw_cursor(&self) {
        let held = self.vars.get("objetenmain");
        if held != 0 {
            if let Some(spr) = crate::ui::object_icon(&self.game.exe.inventory_object_ids, held)
                .and_then(|i| self.ui.object(i))
            {
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

    /// The warp loop `FUN_0041bfa0`: the view until a click. Returns the
    /// zone clicked (0 for none) or [`WARP_ABORT`].
    pub async fn handle_warp(&mut self, hooks: &mut dyn ScriptHooks) -> u32 {
        let mut exit = false;
        let mut action = 0u32;

        // The loop takes the pointer where the caller left it (entry block):
        // a mouse motion since the engine last wrote it is the WM_MOUSEMOVE
        // that set it in the original.
        self.sync_pointer_mouse();
        // The entry block clears 0x00496120 and 0x004960c4 (0x0041bff4).
        self.bar.flags_reset();
        self.sys.set_mouse_pos(320, 240);
        // Drain stale clicks from the place change.
        for _ in 0..5 {
            if self.sys.should_quit() {
                break;
            }
            while self.sys.poll_event().is_some() {}
            self.sys.delay(20).await;
        }
        self.wait_buttons_up().await;
        self.sys.set_mouse_pos(320, 240);
        self.sys.set_mouse_locked(true);
        self.sys.take_mouse_relative();

        self.cursor_shown = CURSOR_UNSET;
        // The arrow frame starts at 0 on every entry (0x0041c017).
        self.arrow_frame = 0;
        let mut bow_button_held = self.sys.buttons().0;
        // FUN_00431a1c at 0x0041c012: the place's ambient sounds.
        self.ambient_enter_place().await;

        let id_rule = self.game.exe.build.cursor_id_rule;
        let mut left_pressed = false;
        while !left_pressed && !exit {
            let fi = self.poll_frame_input();
            self.update_con_clocks(); // FUN_0041bfa0 @ 0x0041c086
                                      // This frame's relative motion (x right, y down).
            let (motion_x, motion_y) = self.sys.take_mouse_relative();
            let mut clicked = fi.left_press || self.sys.buttons().0;
            // Bow mode consumes the click (0x0041c631): no zone or
            // character action; the press edge feeds the shot test after
            // the frame is drawn.
            let bow_click = self.bow_mode && clicked && !bow_button_held;
            bow_button_held = clicked;
            if self.bow_mode {
                clicked = false;
            }

            // The zone and the character under the pointer: a character
            // takes priority over the zone underneath it.
            action = 0;
            let mut centre_perso = 0u32;
            if let (Some(vue), true) = (self.current_place_id, self.warp_loaded) {
                if let Some(place) = self.current_place.and_then(|i| self.wam.places.get(i)) {
                    let (px, py) = self
                        .warp
                        .screen_to_panorama(self.pointer.pos.0, self.pointer.pos.1);
                    let (px, py) = (i32::from(px as i16), i32::from(py as i16));
                    centre_perso = npc_hit_test(
                        &self.wam.npcs,
                        &self.npc_rects,
                        &self.sprite_list,
                        vue,
                        px,
                        py,
                    );
                    action = u32::from(place.hit_test(px as i16, py as i16));
                }
            }

            // The cursor (FUN_00422fcc): talk (NPC) > navigate (zone) >
            // nothing; with an object in hand, its animation over the icon.
            let now = self.sys.millis();
            if self.cursor_shown == CURSOR_UNSET || now >= self.cursor_next_ms {
                self.cursor_next_ms = now + CURSOR_STEP_MS;
                let input = WarpCursorInput {
                    zone: action,
                    id: centre_perso,
                    held: self.vars.get("objetenmain").max(0) as u32,
                    hidden: false,
                    edge_scroll: false,
                    id_rule,
                };
                self.cursor_shown = warp_cursor_step(&mut self.cursor_state, &input).sprite;
            }

            // A click on a character or an object (FUN_004278a0): ids
            // 0x100..0x1ff are objects, from 0x200 the conversation; no move.
            if clicked && centre_perso >= 0x100 {
                self.sys.set_mouse_locked(false);
                // The original leaves the loop with the mouse at the
                // pointer; the dialog's mouse motions set the pointer.
                self.sys
                    .set_mouse_pos(self.pointer.pos.0, self.pointer.pos.1);
                self.mark_pointer_mouse();
                // FUN_004278a0 starts the click animation (0x004278b5).
                self.cursor_state.click_anim = true;
                // ClicZone = the zone under the pointer: the warp loop
                // returns it with the NPC (0x0041c71e) and the place loop
                // stores it before FUN_004278a0 (0x00423c1c); it returns to
                // 0 on the next pass.
                self.vars.set("cliczone", action as i32);
                hooks.npc_click(self, centre_perso, action).await;
                self.vars.set("cliczone", 0);
                self.sync_pointer_mouse();
                if self.should_abort() || self.arrival_scan_pending {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    // No recentre: the loop goes on with the pointer the
                    // dialog left.
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.sys.set_mouse_pos(320, 240);
                    self.cursor_shown = CURSOR_UNSET;
                    self.wait_buttons_up().await;
                    clicked = false;
                }
            }
            left_pressed = clicked;
            if self.should_abort() {
                action = WARP_ABORT;
                exit = true;
            }

            // A right click opens the inventory bar (FUN_0041a298 toggles
            // 0x004960c4, the loop opens the bar @ 0x0041c6d2), also when a
            // right press during the previous frame's timer section left
            // the request; each frame starts by clearing the flags
            // (0x0041c04a..0x0041c07c). Escape opens the in-game menu with
            // no bow, no request and an empty hand (0x0041c5f7).
            let pending = self.bar.warp_frame_start();
            let mut bar = !exit && (fi.right_press || self.sys.buttons().1 || pending);
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
                let guest = self.display_bar(hooks).await;
                // Closing the bar: bow mode with the bow in hand and
                // ArcMagique set (FUN_0042d098), the bow rising.
                if !self.should_abort() {
                    self.enter_bow_mode().await;
                }
                // FUN_0042c59c @ 0x0042c844: the timers resume.
                let (t1, t2) = (self.vars.get("time1"), self.vars.get("time2"));
                let now = self.now();
                self.clocks.rebase(now, t1, t2);
                if guest != 0 && !self.should_abort() {
                    hooks.talk_to_guest(self, guest).await;
                }
                if self.should_abort() || self.arrival_scan_pending {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    // FUN_0042c098 closes the bar with the pointer at
                    // (320, 240) (@ 0x0042c275).
                    self.sys.set_mouse_pos(320, 240);
                    self.pointer.recentre();
                    self.cursor_shown = CURSOR_UNSET;
                    self.wait_buttons_up().await;
                    bow_button_held = false;
                }
            } else if !exit && fi.escape && !self.bow_mode && self.vars.get("objetenmain") == 0 {
                self.sys.set_mouse_locked(false);
                self.sys
                    .set_mouse_pos(self.pointer.pos.0, self.pointer.pos.1);
                self.mark_pointer_mouse();
                self.run_ingame_menu().await;
                self.sync_pointer_mouse();
                if self.should_abort() {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.cursor_shown = CURSOR_UNSET;
                    self.wait_buttons_up().await;
                }
            }

            // The view: FUN_0041bfa0 steps 2..5. The stored angles first
            // take F5 / F6 (turn) and F8 / F7 (tilt) with the left button
            // up (FUN_0041a298 at the end of the previous frame).
            let (mut alpha, mut beta) = (self.warp.alpha(), self.warp.beta());
            if !self.sys.buttons().0 {
                if self.sys.key_held(Key::F(5)) {
                    alpha = (f64::from(alpha) + VIEW_KEY_STEP) as f32;
                }
                if self.sys.key_held(Key::F(6)) {
                    alpha = (f64::from(alpha) - VIEW_KEY_STEP) as f32;
                }
                if self.sys.key_held(Key::F(8)) {
                    beta = (f64::from(beta) + VIEW_KEY_STEP) as f32;
                }
                if self.sys.key_held(Key::F(7)) {
                    beta = (f64::from(beta) - VIEW_KEY_STEP) as f32;
                }
            }
            // OMNI 3D MODE 1, the original's default (0x004960ac == 0),
            // gives dy = prevY - y; MODE 2 y - prevY.
            let mode2 = self.settings.omni3d_mode2;
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
            self.vars.set("anglewarpx", look.angle_warp_x);
            self.vars.set("anglewarpy", look.angle_warp_y);
            self.set_view(alpha, beta);
            // The pointer follows the mouse, and goes back to the centre when
            // the new (unclamped) angles differ from the previous frame's
            // (step 5, @ 0x0041c2bb..0x0041c363).
            self.pointer.frame(
                motion_x,
                motion_y,
                look.new_alpha,
                look.new_beta,
                alpha,
                beta,
            );

            // Timer sections (@ 0x0041c369). A Vue= in the timer section has
            // entered another place; the warp loop does not return for it:
            // the section executor ends with the click flag cleared
            // (FUN_0042844c @ 0x00428d77) and the Vue= branch sets no exit
            // flag (FUN_0042a100 @ 0x0042a630..0x0042a749), so the loop runs
            // on in the new place and the arrival scan comes after the next
            // click.
            if !exit && hooks.warp_frame(self).await {
                self.sync_pointer_mouse();
                if self.should_abort() {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.cursor_shown = CURSOR_UNSET;
                    self.wait_buttons_up().await;
                }
            }

            // Place sprites: FUN_00425304 at 0x0041c3a9, after the timers,
            // unless GameOver or Exit is set (then it leaves, 0x0041c387).
            if !exit && self.vars.get("exit") != 0 {
                action = WARP_ABORT;
                exit = true;
            }
            if !exit {
                self.animate_sprites();
            }
            self.render_view();
            // The bow, ARCWARP.SPR frame arrow_frame at (0x2e, 0x79)
            // (0x0041c40b), then the guests' portraits (0x0041c489 ->
            // FUN_0042bc58).
            if self.warp_loaded {
                let mut s = self.sys.screen.borrow().clone();
                if self.bow_mode {
                    if let Some(spr) = self.bow_sprites.get(usize::from(self.arrow_frame)) {
                        crate::ui::draw_sprite(&mut s, spr, 0x2e, 0x79);
                    }
                }
                self.draw_guests(&mut s, None);
                *self.sys.screen.borrow_mut() = s;
            }
            self.draw_cursor();
            if self.debug_overlay.get() {
                self.draw_debug_overlay();
            }
            // The arrow shot (0x0041c61d..0x0041c6a0), after the frame is
            // drawn.
            let mut tir = self.vars.get("tirfleche");
            let angle_y = self.vars.get("anglewarpy");
            atlantis_data::sprite_anim::bow_frame_step(
                self.bow_mode,
                bow_click,
                angle_y,
                &mut tir,
                &mut self.arrow_frame,
            );
            self.vars.set("tirfleche", tir);
            // FUN_00432178, the last call of the frame: the ambient sounds,
            // then the NewSound poll FUN_00432a64.
            self.poll_new_sound().await;
            self.ambient_frame();
            if !exit {
                self.sys.frame().await;
            }
        }

        self.sys.set_mouse_locked(false);
        // The mouse at the pointer; motions from here on set it.
        self.sys
            .set_mouse_pos(self.pointer.pos.0, self.pointer.pos.1);
        self.mark_pointer_mouse();
        self.wait_buttons_up().await;
        log::debug!("handle_warp: action {action}");
        action
    }
}
