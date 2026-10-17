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
                _ => {}
            }
        }
        fi
    }

    /// Waits for the buttons to be released (the loops drain the click
    /// that ended them).
    async fn wait_buttons_up(&self) {
        while self.sys.buttons() != (false, false) && !self.sys.should_quit() {
            while self.sys.poll_event().is_some() {}
            self.sys.frame().await;
        }
    }

    /// Draws the cursor sprite at the pointer.
    fn draw_cursor(&self) {
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
        // TODO(phase 2): the arrow frame starts at 0 (0x0041c017), bow mode.
        // FUN_00431a1c at 0x0041c012: the place's ambient sounds.
        self.ambient_enter_place().await;

        let id_rule = self.game.exe.build.cursor_id_rule;
        let mut left_pressed = false;
        while !left_pressed && !exit {
            let fi = self.poll_frame_input();
            // This frame's relative motion (x right, y down).
            let (motion_x, motion_y) = self.sys.take_mouse_relative();
            let mut clicked = fi.left_press || self.sys.buttons().0;
            // TODO(phase 2): bow mode consumes the click (0x0041c631).

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
            // nothing. TODO(phase 3): the carried object's cursor.
            let now = self.sys.millis();
            if self.cursor_shown == CURSOR_UNSET || now >= self.cursor_next_ms {
                self.cursor_next_ms = now + CURSOR_STEP_MS;
                let input = WarpCursorInput {
                    zone: action,
                    id: centre_perso,
                    held: 0,
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
                hooks.npc_click(self, centre_perso, action).await;
                self.sync_pointer_mouse();
                if self.sys.should_quit() || self.arrival_scan_pending {
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
            if self.sys.should_quit() {
                action = WARP_ABORT;
                exit = true;
            }

            // A right click opens the inventory bar, Escape the in-game
            // menu (0x0041c6d2, 0x0041c5f7).
            if !exit && (fi.right_press || self.sys.buttons().1) {
                self.sys.set_mouse_locked(false);
                hooks.bar_requested(self).await;
                self.sys.set_mouse_locked(true);
                self.sys.take_mouse_relative();
                // FUN_0042c098 closes the bar with the pointer at (320, 240).
                self.pointer.recentre();
                self.cursor_shown = CURSOR_UNSET;
                self.wait_buttons_up().await;
            } else if !exit && fi.escape {
                self.sys.set_mouse_locked(false);
                self.sys
                    .set_mouse_pos(self.pointer.pos.0, self.pointer.pos.1);
                self.mark_pointer_mouse();
                hooks.menu_requested(self).await;
                self.sync_pointer_mouse();
                if self.sys.should_quit() {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.cursor_shown = CURSOR_UNSET;
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
            // OMNI 3D MODE 1, the original's default (0x004960ac == 0).
            // TODO(phase 3): MODE 2 from the options menu.
            let look = warp_look_frame(
                &mut self.momentum,
                &mut alpha,
                &mut beta,
                motion_x,
                motion_y,
                false,
                self.pointer.prev_alpha,
                self.pointer.prev_beta,
                self.warp.pi(),
                self.warp.two_pi(),
                self.warp.beta_limit(),
            );
            self.angle_warp = (look.angle_warp_x, look.angle_warp_y);
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

            // Timer sections (@ 0x0041c369).
            if !exit && hooks.warp_frame(self).await {
                self.sync_pointer_mouse();
                if self.sys.should_quit() {
                    action = WARP_ABORT;
                    exit = true;
                } else {
                    self.sys.set_mouse_locked(true);
                    self.sys.take_mouse_relative();
                    self.cursor_shown = CURSOR_UNSET;
                    self.wait_buttons_up().await;
                }
            }

            // Place sprites: FUN_00425304 at 0x0041c3a9, after the timers.
            if !exit {
                self.animate_sprites();
            }
            self.render_view();
            self.draw_cursor();
            // TODO(phase 2/3): the bow (0x0041c40b) and the guests'
            // portraits (0x0041c489) over the view; the arrow shot.
            // FUN_00432178, the last call of the frame.
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
