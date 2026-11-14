//! Entering a place, its sprites, the moves between places with their
//! transition videos, and the ambient sounds' calls.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`place.cpp`,
//! `sprite_renderer.cpp`, `engine.cpp` `loadCycloHNM`; GPLv3, the ScummVM
//! team).

use std::collections::HashMap;
use std::rc::Rc;

use atlantis_codecs::HnmVideo;
use atlantis_data::sprite::{composite_spf_frame, decode_spw_pixels, spw_frame_count};
use atlantis_data::sprite_anim::{animate_place_sprites, enter_place, SPRITE_TICK_MS};
use atlantis_data::sprite_list::{npc_click_rects, npc_row_matches};
use atlantis_data::wam::{CentrePos, Transition};

use crate::engine::{Engine, Event};
use crate::music::apc_pcm;
use crate::sys::{Input, MouseButton};
use crate::turn::{TransitionTurn, TURN_END_FOV_DEG};
use crate::video_skip::{VideoSkipRun, VideoSkipState};
use crate::warp::{Warp, PANO_W, SCREEN_H, SCREEN_W, WARP_FOV_DEG};

/// The frame delay of a move's transition video: 6 ticks of the 200 Hz
/// timer.
pub const TRANSITION_FRAME_MS: u64 = 6 * 5;
/// The frame delay of a cinematic (Cinema=, the start-up videos).
pub const CINEMA_FRAME_MS: u64 = 13 * 5;

/// The captions burnt over a CINEMA video as sprites
/// ([`atlantis_data::CinemaCaptions`]): its bank and the frame counter
/// 0x004865a0, which the CINEMA= command resets (atlantis-cd-zh-tw-696832
/// FUN_00429958 @ 0x00429fbe).
pub(crate) struct CinemaCaptionState {
    pub spec: atlantis_data::CinemaCaptions,
    pub bank: Vec<atlantis_data::sprite::SprSprite>,
    pub frame: u8,
}

impl CinemaCaptionState {
    /// One call of the video frame presenter at `t` ticks of the 200 Hz
    /// timer since the video started (0x004865a4, set by FUN_0041bd04 @
    /// 0x0041bdc0): when `after` < t < `before`, the frame to draw, then the
    /// counter stepped from k to k + 1 once t > `steps[k]`, the ten tests in
    /// order, so several steps can happen in one call
    /// (atlantis-cd-zh-tw-696832 FUN_0041afa0 @ 0x0041b105..0x0041b2bc).
    pub fn present(&mut self, t: u32) -> Option<u8> {
        let c = &self.spec;
        if t <= u32::from(c.after) || t >= u32::from(c.before) {
            return None;
        }
        let drawn = self.frame;
        for (k, &step) in c.steps.iter().enumerate() {
            if t > u32::from(step) && usize::from(self.frame) == k {
                self.frame += 1;
            }
        }
        Some(drawn)
    }
}

/// `name` with its extension replaced (or added).
fn with_ext(name: &str, ext: &str) -> String {
    match name.rfind('.') {
        Some(dot) => format!("{}.{ext}", &name[..dot]),
        None => format!("{name}.{ext}"),
    }
}

impl Engine {
    /// The mouse moved since the mark: the pointer is the last mouse
    /// position (WM_MOUSEMOVE, 0x00419c14).
    pub(crate) fn sync_pointer_mouse(&mut self) {
        let m = self.sys.mouse();
        if m != self.pointer_mouse_mark {
            self.pointer.mouse_moved(m);
        }
        self.pointer_mouse_mark = m;
    }

    pub(crate) fn mark_pointer_mouse(&mut self) {
        self.pointer_mouse_mark = self.sys.mouse();
    }

    /// Enters `next_place_id` (`doPlaceChange`, `FUN_00421b4c`).
    pub(crate) async fn do_place_change(&mut self) {
        let Some(next) = self.next_place_id else {
            return;
        };
        // The move's panorama name is for this entry only (every entry
        // rebuilds 0x006cd288: FUN_00421b4c), even one that fails.
        let move_cyclo = self.move_cyclo.take();
        let Some(index) = self.wam.places.iter().position(|p| p.id == next) else {
            log::warn!("place {next} not in {}", self.wam_name);
            self.next_place_id = None;
            return;
        };
        self.current_place = Some(index);
        self.current_place_id = Some(next);
        self.next_place_id = None;
        self.vue_override = None; // `Vue` follows the place again

        // A move loads the panorama its click named in the WAM it left
        // (FUN_0044a424 @ 0x00423ed4); FUN_00421b4c (0x00424068) only
        // selects the place and rebuilds the name. Every other entry
        // loads the record's.
        let cyclo = match move_cyclo {
            Some((id, name)) if id == next => name,
            _ => self.wam.places[index].cyclo.clone(),
        };
        if self.load_cyclo(&cyclo).await {
            self.composite_npc_sprites().await;
        }
        self.sys.set_mouse_pos(320, 240);
        self.mark_pointer_mouse(); // the engine's own move is no mouse motion
        self.emit(Event::PlaceEntered {
            wam: self.wam_name.clone(),
            place: next,
        });
    }

    /// The current place's panorama loaded again, as `FUN_004240fc` does
    /// after a puzzle (`FUN_0044a424` @ 0x004241e2: the panorama, the FOV,
    /// the pointer left at (0x140, 0x140)). It calls neither `FUN_0042431c`
    /// nor `FUN_0042488c`: the sprite rows keep their frames, frame counts
    /// and click areas, and the layer is drawn again at those frames. The
    /// place is not entered again: no place event, the mouse is not moved.
    pub(crate) async fn reload_view_after_puzzle(&mut self) {
        let Some(index) = self.current_place else {
            return;
        };
        let Some(cyclo) = self.wam.places.get(index).map(|p| p.cyclo.clone()) else {
            return;
        };
        if self.load_cyclo(&cyclo).await {
            self.recomposite_sprite_layer();
        }
    }

    /// The current place's view loaded as an entry does: its panorama and
    /// its sprites (frame 0, the NPC click areas rebuilt), with no place
    /// event (tests).
    #[cfg(test)]
    pub(crate) async fn reload_current_view(&mut self) {
        let Some(index) = self.current_place else {
            return;
        };
        let Some(cyclo) = self.wam.places.get(index).map(|p| p.cyclo.clone()) else {
            return;
        };
        if self.load_cyclo(&cyclo).await {
            self.composite_npc_sprites().await;
        }
    }

    /// The panorama `CYCLO\<name>` (`loadCycloHNM`, `FUN_0044a424`).
    async fn load_cyclo(&mut self, name: &str) -> bool {
        let mut path = format!("CYCLO\\{name}");
        // Some WAM entries name a .tga; the archive only has the .hnm.
        if !name.to_ascii_lowercase().ends_with(".hnm") && !self.game.big.contains(&path) {
            path = format!("CYCLO\\{}", with_ext(name, "hnm"));
        }
        let Some(data) = self.read(&path).await else {
            return false;
        };
        let pano = match atlantis_codecs::decode_panorama(&data) {
            Ok(p) if p.width == PANO_W => p,
            Ok(p) => {
                log::warn!("{path}: {}x{} is not a panorama", p.width, p.height);
                return false;
            }
            Err(e) => {
                log::warn!("{path}: {e}");
                return false;
            }
        };
        self.cyclo_h = pano.height;
        self.warp_surface.clone_from(&pano.pixels);
        self.hd_surface_changed();
        self.cyclo = pano.pixels;
        self.warp_loaded = true;
        // FUN_0044a424 sets the warp FOV (0x0044a701) and leaves the pointer
        // at (0x140, 0x140) without SetCursorPos (0x0044a70b / 0x0044a715).
        self.warp.set_hfov(WARP_FOV_DEG);
        self.pointer.panorama_loaded();
        self.mark_pointer_mouse();
        true
    }

    /// `FUN_0042431c` and the NPC click areas of `FUN_00424a20`: frame 0
    /// and the frame count (SPW header dword +4) of each row of the place;
    /// the active flags are left as they are.
    async fn composite_npc_sprites(&mut self) {
        let Some(vue) = self.current_place_id else {
            return;
        };
        enter_place(&mut self.sprite_list, vue);
        let mut names: Vec<String> = self
            .sprite_list
            .iter()
            .filter(|e| e.place_id == vue || npc_row_matches(&self.wam.npcs, e))
            .map(|e| format!("SPRITE\\CYCLO\\{}", e.spw_file))
            .collect();
        names.sort();
        names.dedup();
        self.prefetch(&names).await;
        let mut cache = HashMap::new();
        for e in self.sprite_list.iter_mut().filter(|e| e.place_id == vue) {
            e.frame_count = 0;
            let data = cache.entry(e.spw_file.clone()).or_insert_with(|| {
                self.game
                    .big
                    .read(&format!("SPRITE\\CYCLO\\{}", e.spw_file))
                    .map(Rc::new)
            });
            match data {
                Some(d) => e.frame_count = spw_frame_count(d) as i32,
                None => log::warn!("SPRITE\\CYCLO\\{} not found", e.spw_file),
            }
        }
        self.spw_cache = cache
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (k, v)))
            .collect();
        let big = &self.game.big;
        self.npc_rects = npc_click_rects(&self.wam.npcs, &self.sprite_list, |name| {
            big.read(&format!("SPRITE\\CYCLO\\{name}"))
        });
        self.recomposite_sprite_layer();
    }

    /// The panorama with every active sprite of the place at its frame
    /// (`recompositeSpriteLayer`, `FUN_00452bde` per sprite).
    pub(crate) fn recomposite_sprite_layer(&mut self) {
        if !self.warp_loaded {
            return;
        }
        let Some(vue) = self.current_place_id else {
            return;
        };
        self.sprite_layer_stale = false;
        self.warp_surface.copy_from_slice(&self.cyclo);
        self.hd_surface_changed();
        for e in &self.sprite_list {
            if e.place_id != vue || !e.active {
                continue;
            }
            if self.sprites_hide_npcs && e.perso_id >= 0x200 {
                continue;
            }
            let Some(spw) = self.spw_cache.get(&e.spw_file) else {
                continue;
            };
            decode_spw_pixels(
                spw,
                &mut self.warp_surface,
                PANO_W,
                self.cyclo_h,
                e.frame.max(0) as usize,
            );
        }
    }

    /// The rows of view `vue` the layer draws (`recomposite_sprite_layer`),
    /// in list order, with their frames.
    fn drawn_sprite_rows(&self, vue: u16) -> Vec<(usize, i32)> {
        self.sprite_list
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.place_id == vue
                    && e.active
                    && !(self.sprites_hide_npcs && e.perso_id >= 0x200)
                    && self.spw_cache.contains_key(&e.spw_file)
            })
            .map(|(i, e)| (i, e.frame.max(0)))
            .collect()
    }

    /// A type-2 row's cycle ended on this pass. The original draws each
    /// row with `FUN_00452d39`, which saves the panorama pixels it covers
    /// in the row's buffer, and after the present `FUN_004255f0` puts them
    /// back with `FUN_00453037`, in reverse list order, for the rows whose
    /// active flag is still set only (dc:17688); the cycle end clears the
    /// flag (@ 0x004255d7) between the two, so the ended row's last frame
    /// is never restored and stays in the panorama buffer 0x008b9d00 until
    /// `FUN_0044a424` loads it again. Run on the base `cyclo` (the next
    /// [`Engine::load_cyclo`] replaces it): `drawn`, the rows drawn on the
    /// pass at their frames, then the restores of the rows still active.
    fn keep_ended_sprite_frames(&mut self, drawn: &[(usize, i32)]) {
        let n = (PANO_W * self.cyclo_h).min(self.cyclo.len());
        let mut buf = self.cyclo.clone();
        let mut a = vec![0u16; n];
        let mut b = vec![0u16; n];
        let mut saves = Vec::with_capacity(drawn.len());
        for &(i, frame) in drawn {
            let Some(spw) = self.spw_cache.get(&self.sprite_list[i].spw_file) else {
                continue;
            };
            // The pixels a frame covers: the same in two decodes over
            // different backgrounds.
            a.fill(0);
            b.fill(0xffff);
            decode_spw_pixels(spw, &mut a, PANO_W, self.cyclo_h, frame as usize);
            decode_spw_pixels(spw, &mut b, PANO_W, self.cyclo_h, frame as usize);
            let mut saved = Vec::new();
            for p in 0..n {
                if a[p] == b[p] {
                    saved.push((p, buf[p]));
                    buf[p] = a[p];
                }
            }
            saves.push((i, saved));
        }
        for (i, saved) in saves.iter().rev() {
            if self.sprite_list[*i].active {
                for &(p, v) in saved {
                    buf[p] = v;
                }
            }
        }
        self.cyclo = buf;
        self.sprite_layer_stale = true;
    }

    /// `FUN_00425304` once per frame: the test "more than 13 ticks" of the
    /// 200 Hz counter is run for each tick elapsed since the last call (at
    /// most 2 s, e.g. after a video), as the original, which does not wait
    /// for the display, steps every 14 or 15 ticks.
    ///
    /// The original draws each row at its current frame (`call 0x00452d39`
    /// @ 0x00425476) before it steps it (last tick written @ 0x0042552c,
    /// frame + 1, then the cycle end), and the view is rendered right
    /// after: a stepped frame is first seen on the next pass, and a type-2
    /// row is still drawn at its last frame on the pass that ends its
    /// cycle. So the layer is redrawn here with the previous call's steps,
    /// and this call's steps are drawn by the next one.
    pub(crate) fn animate_sprites(&mut self) {
        let Some(vue) = self.current_place_id else {
            return;
        };
        if !self.warp_loaded || usize::from(vue) >= self.sprite_anim_current.len() {
            return;
        }
        let now = (self.sys.millis() / u64::from(SPRITE_TICK_MS)) as u32;
        let mut t = if self.sprite_anim_tick != 0 && now.wrapping_sub(self.sprite_anim_tick) <= 400
        {
            self.sprite_anim_tick + 1
        } else {
            now
        };
        if self.sprite_layer_stale {
            self.recomposite_sprite_layer();
        }
        let mut changed = false;
        let current = self.sprite_anim_current[usize::from(vue)];
        // FUN_00425304 writes the CON variable WSPRFRAME itself (the frame
        // of the view's current animation row), which the scripts test:
        // CHAPI013's flower pot falls on the pacing slave only while
        // Wsprframe is in 8..33 or 70..96. It starts from the variable, so
        // a script's write stays until the next step changes it.
        let mut wspr = self.vars.get("wsprframe");
        // The rows drawn on this pass, and the type-2 rows that may end.
        let drawn = self.drawn_sprite_rows(vue);
        let one_shots: Vec<usize> = drawn
            .iter()
            .map(|&(i, _)| i)
            .filter(|&i| self.sprite_list[i].kind == 2)
            .collect();
        while now.wrapping_sub(t) as i32 >= 0 {
            changed |= animate_place_sprites(
                &mut self.sprite_list,
                vue,
                current,
                t,
                &mut self.sprite_anim_last,
                &mut wspr,
                self.sprites_hide_npcs,
            );
            t = t.wrapping_add(1);
        }
        self.sprite_anim_tick = now;
        if one_shots.iter().any(|&i| !self.sprite_list[i].active) {
            self.keep_ended_sprite_frames(&drawn);
        }
        self.wspr_frame = wspr;
        self.vars.set("wsprframe", wspr);
        if changed {
            self.sprite_layer_stale = true;
        }
    }

    /// Renders the view into the screen (the place sprites are on the
    /// panorama).
    pub(crate) fn render_view(&mut self) {
        if !self.warp_loaded {
            return;
        }
        let mut screen = self.sys.screen.borrow_mut();
        self.warp.render(&self.warp_surface, &mut screen);
        drop(screen);
        self.hd_show_view();
    }

    /// A move to `next` (`executeTransition`): the turn, the video, the
    /// sub-WAM swap.
    pub(crate) async fn execute_transition(&mut self, next: u16) {
        // After a ChangeWam= the script's next Vue= is the destination.
        let (Some(place), Some(cur)) = (self.place(), self.current_place_id) else {
            self.next_place_id = Some(next);
            return;
        };
        // The click's FUN_00421cfc (@ 0x00423dc5) names the panorama from
        // the destination's record in the current WAM, before any sub-WAM
        // swap; FUN_0044a424 loads that name (@ 0x00423ed4) and nothing
        // reloads it after the swap. Only BIBLION -> PALAIS3 at place 192
        // names another picture than the new WAM's (ATL16192, not
        // ATN16192).
        let transition = place.find_transition(next).cloned();
        self.move_cyclo = self
            .wam
            .places
            .iter()
            .find(|p| p.id == next)
            .map(|p| (next, p.cyclo.clone()));
        if let Some(t) = transition {
            // FUN_00423ad8 @ 0x00423ec0: FUN_004212cc turns the view to the
            // transition's source angles before the video.
            self.turn_to_transition(t.src_alpha, t.src_beta).await;
            // FUN_004212cc ends by setting the destination angles
            // (0x00421b01..0x00421b31), before the video, so FUN_0043259c's
            // ambient glide uses them.
            // Widescreen: the panoramas beside the video (the sides of the
            // view the turn ended on, the destination's).
            let crosses = self
                .wam
                .sub_wams
                .iter()
                .any(|s| s.src_place == cur && s.dst_place == next);
            let dest = (!crosses)
                .then(|| self.wam.places.iter().find(|p| p.id == next))
                .flatten()
                .map(|p| (p.cyclo.clone(), t.dst_alpha, t.dst_beta));
            // 0x00423f8a: a double click during the turn skips the video
            // (and its SPF sprites and ambient glide); the flag is reset
            // either way (0x00424013).
            let play = !self.turn_double_click.flag;
            // Without the video the turn's last frame stays on screen, and
            // its own sides with it: no video sides.
            if play {
                self.hd_video_panorama_start(Some((t.src_alpha, t.src_beta)), dest)
                    .await;
            } else {
                self.sys.hd_video.borrow_mut().active = false;
            }
            self.set_view(t.dst_alpha, t.dst_beta);
            if play {
                self.play_transition_video(&t.video, Some((cur, next)), TRANSITION_FRAME_MS)
                    .await;
                // The video's last frame stays on screen while the
                // destination loads: its sides stay those of the video's
                // end (switched off when the destination's view is shown,
                // `hd_show_view_over`).
                self.sys.hd_video.borrow_mut().t = 1.0;
            }
            self.turn_double_click.clear();
            // FUN_00423ad8 resets the input flags after the video block
            // whether the video played or not (a double click):
            // 0x00496118, 0x0049611c, 0x00496120, 0x00496138 = 0,
            // 0x00496124 = 1 and 0x004960c4 = 0 (decompilation of
            // atlantis-cd-en-714240, FUN_00423ad8 after FUN_0041bc74), so
            // the Escape latch ends here in both cases.
            self.escape_request = false;
        }
        // A sub-WAM swaps the area when (src, dst) crosses into another WAM
        // (FUN_00422ad0). The place id stays the one being left: the
        // original stores ARRIVEEVUE (0x00423def..0x00423df4) before the
        // swap.
        let sub = self
            .wam
            .sub_wams
            .iter()
            .find(|s| s.src_place == cur && s.dst_place == next)
            .cloned();
        if let Some(sub) = sub {
            // FUN_00422ad0 @ 0x00422b4c: the path from the record's name.
            self.set_wam_path(&crate::con_script::script::latin1_bytes(&sub.wam));
            self.load_wam(&sub.wam).await;
            self.current_place = None;
            self.next_place_id = Some(sub.dst_place);
            return;
        }
        self.next_place_id = Some(next);
    }

    /// `FUN_004212cc`: the turn's frames, each drawn as the warp loop draws
    /// one (place sprites, the view) without cursor, each with the ambient
    /// update `FUN_00432178`. With ZOOM the
    /// field of view narrows by 3 degrees a frame from 75.187; the turn
    /// ends with `FUN_0045179a(75.187)`, which lasts until the next
    /// panorama load sets the warp's 75.137, and ZOOM = 0.
    pub(crate) async fn turn_to_transition(&mut self, src_alpha: f32, src_beta: f32) {
        if !self.warp_loaded {
            return;
        }
        // The turn starts from the stored angles (the view keys of the
        // departure's lines included).
        self.apply_view_keys();
        let mut turn = TransitionTurn::new(
            Warp::exe_pi(),
            self.warp.alpha(),
            self.warp.beta(),
            src_alpha,
            src_beta,
            self.zoom || self.vars.get("zoom") != 0,
        );
        let mut first = true;
        loop {
            if self.sys.should_quit() {
                break;
            }
            // The messages FUN_0041a298 pumps after each frame (0x00421670 /
            // 0x00421a5a): a left press here can be the second of a double
            // click whose first ended the warp loop (TurnDoubleClick,
            // 0x00496118); a right press toggles the bar request; Escape
            // (no talking head, bow mode, bar request or object in hand)
            // clears 0x00496124 and sets the menu request, which ends
            // phase 1 and then the move's video at its first frame.
            if !first {
                self.turn_poll();
                // Each step (and phase 1's stop test after the poll) reads
                // the stored angles 0x00bcb59c / 0x00bcb598: the previous
                // frame's, wrapped by FUN_00451915, moved by the poll's
                // view keys (FUN_004212cc l. 15661, 15749-15772).
                let (a, b) = self.stored_view();
                turn.set_angles(a, b);
            }
            first = false;
            let Some(f) = turn.next_frame(!self.escape_request) else {
                break;
            };
            if f.fov_set {
                self.warp.set_hfov(f.fov);
            }
            self.set_view(f.alpha, f.beta);
            // FUN_00432178 on every turn frame, with the frame's angles
            // stored and before FUN_00451915 (0x004212cc, both phases): the
            // ambient gains follow the turning view, so the glide
            // FUN_0043259c starts from the last turn frame's; NewSound is
            // polled.
            self.poll_new_sound().await;
            self.ambient_frame();
            self.animate_sprites();
            self.render_view();
            if self.guests_shown && self.guests.count != 0 {
                let mut s = self.sys.screen.borrow().clone();
                self.draw_guests(&mut s, None);
                *self.sys.screen.borrow_mut() = s;
            }
            // The original's turn has no timer of its own: one frame each.
            self.sys.frame().await;
        }
        self.warp.set_hfov(TURN_END_FOV_DEG);
        self.zoom = false;
        self.vars.set("zoom", 0);
    }

    /// The turn's input poll (`FUN_0041a298` after each turn frame).
    pub(crate) fn turn_poll(&mut self) {
        let (mut left, mut right, mut escape) = (None, false, false);
        while let Some(ev) = self.sys.poll_event() {
            match ev {
                Input::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                } => {
                    // Windows queues WM_LBUTTONDBLCLK for a second press;
                    // this poll does not pump it (0x0041a2b5, below).
                    let now = self.sys.millis();
                    self.turn_double_click.queue_press(now, x, y);
                    left = Some((x, y));
                }
                Input::MouseDown {
                    button: MouseButton::Right,
                    ..
                } => right = true,
                Input::KeyDown(crate::sys::Key::Escape) => escape = true,
                _ => {}
            }
        }
        // A left press ends the poll before the rest (0x0041a2b5), its
        // message pump included: a double click's second press read here is
        // dispatched by a later poll without a new press.
        if left.is_some() {
            return;
        }
        // F5..F8 (0x0041a33c..0x0041a3ba): the turn's next step starts from
        // the angles they moved.
        self.poll_view_keys(false);
        // The message pump: a WM_LBUTTONDBLCLK queued during the departure
        // scan's lines or by an earlier poll of the turn.
        let (mode, menu) = (self.cursor_state.mode, self.escape_request);
        self.turn_double_click.dispatch(mode, menu);
        if right {
            self.bar
                .right_pressed(self.section_play, self.escape_request);
        }
        if escape && self.escape_gate_open() {
            self.escape_request = true;
        }
    }

    /// Escape clears 0x00496124 (`FUN_0041a298` @ 0x0041a3e8..0x0041a419)
    /// when no talking head plays (0x00496188, never in these loops) and
    /// bow mode, the bar request and OBJETENMAIN are all 0.
    pub(crate) fn escape_gate_open(&self) -> bool {
        !self.bow_mode && !self.bar.request && self.vars.get("objetenmain") == 0
    }

    /// A video of `UBB_VUE\` (`playTransitionVideo`, `FUN_0041bc74`):
    /// `places` is the move (from, to) whose SPF sprites are drawn over it
    /// (`FUN_0041b81c`, `FUN_004256e8`), `None` for a cinematic.
    pub async fn play_transition_video(
        &mut self,
        video: &str,
        places: Option<(u16, u16)>,
        frame_ms: u64,
    ) {
        self.play_film(video, places, frame_ms, places.is_none())
            .await;
    }

    /// A start-up film of WinMain (`FUN_00424254`, which plays it through
    /// `FUN_0041bc74`): its sound is the NewSound value WinMain stores
    /// before it (`newsound`, e.g. 0x12 for LOGO15N, 0x13 for PREINTRO;
    /// `None` leaves NEWSOUND as it is), started by the film loop's first
    /// poll of `FUN_00432a64` (0x0041bdcb) from the table 0x00496224, not
    /// by a Cinema='s name.
    pub(crate) async fn play_startup_film(&mut self, video: &str, newsound: Option<i32>) {
        if let Some(v) = newsound {
            self.vars.set("newsound", v);
        }
        self.play_film(video, None, CINEMA_FRAME_MS, false).await;
    }

    /// [`Engine::play_transition_video`]; `cinema_sound`: the Cinema=
    /// handler's sound (NEWSOUND = 1 and the film's own APC name).
    async fn play_film(
        &mut self,
        video: &str,
        places: Option<(u16, u16)>,
        frame_ms: u64,
        cinema_sound: bool,
    ) {
        // A cinematic is not a move: no panorama beside it (a move's is
        // set up just before its video).
        if places.is_none() {
            self.sys.hd_video.borrow_mut().active = false;
        }
        // WAM records name .UBB files; the archive stores .HNM.
        let name = if video.to_ascii_uppercase().ends_with(".UBB") {
            with_ext(video, "HNM")
        } else {
            video.to_string()
        };
        let Some(data) = self.read(&format!("UBB_VUE\\{name}")).await else {
            return;
        };
        // The SPF sprites of the move, skipping the hidden persos.
        let mut spf_names = Vec::new();
        if let Some((from, to)) = places {
            spf_names = self
                .spf_list
                .iter()
                .filter(|e| e.from_place == from && e.to_place == to)
                .filter(|e| !self.hidden_persos.contains(&e.perso_id))
                .map(|e| format!("SPRITE\\UBB\\{}", e.spf_file))
                .collect();
        }
        self.prefetch(&spf_names).await;
        let spfs: Vec<Vec<u8>> = spf_names
            .iter()
            .filter_map(|n| self.game.big.read(n))
            .collect();

        let mut hnm = match HnmVideo::with_buffers(data, std::mem::take(&mut self.ix_buffers)) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("{name}: {e}");
                return;
            }
        };
        hnm.set_regular_frame_delay(frame_ms);
        // FUN_0043259c between the HNM open and the play loop, for a move.
        if let Some((_, to)) = places {
            self.ambient_prepare_glide(to, hnm.frame_count() as i32);
        }
        // The same-name sound of a cinematic or a start-up video (e.g.
        // WAV\CINEM020.APC for CINEM020.HNM): the Cinema= handler names
        // 0x007728c7 "cinem" + N + ".apc", sets 0x00496190 = 1 and NEWSOUND
        // = 1 and runs the poller FUN_00432a64 (FUN_0042a100 @
        // 0x0042a7c0..0x0042a8e6), which takes that name. So, as any
        // NewSound: another value playing is stopped; a missing file stops
        // nothing more and clears NEWSOUND; and a cinematic's sound still
        // playing (value 1 again) is left to end, the new one not started
        // (its end, seen by a later poll, clears NEWSOUND), unless
        // BugFix::CinemaSound starts the new one. A move's video
        // (FUN_00423ad8) opens no sound: the transition-named APCs are
        // NewSound entries that the departure sections play, and a start-up
        // film's sound is WinMain's table value (Engine::play_startup_film).
        if cinema_sound {
            self.apply_new_sound_fade_end();
            let apc = format!("WAV\\{}", with_ext(&name, "APC"));
            if self.cinema_sound_fix() && self.music.new_sound_last == 1 {
                self.music.new_sound_playing = false;
            }
            self.vars.set("newsound", 1);
            self.poll_new_sound_named(Some(apc)).await;
        }

        let start = self.sys.millis();
        let mut first = true;
        let mut spf_frame = 0usize;
        // FUN_0041bc74's frame control (VideoSkipRun): a poll after each
        // presented frame but the last, with the conditions of
        // FUN_0041a298 (video_skip); after the poll that ends the video the
        // next frame is still presented.
        // The bar request as it stands: set for a Cinema= of a timer the
        // bar loop runs, so that Escape does not end it then.
        let mut run = VideoSkipRun::new(VideoSkipState {
            object_in_hand: self.vars.get("objetenmain") != 0,
            section_play: self.section_play,
            bow_mode: self.bow_mode,
            bar_request: self.bar.request,
            ..VideoSkipState::default()
        });
        // An Escape of the turn or the subject menu left 0x00496124 = 0:
        // the loop (`while 0x00496124 != 0`, 0x0041bd75) does not run, and
        // its exit presents the frame decoded before it, the first
        // (0x0041be4a), with the menu request still set.
        if self.escape_request {
            run.s.keep_running = false;
            run.s.menu_request = true;
            run.stop_after_frame = true;
        }
        let (w, h) = (hnm.width(), hnm.height());
        // The composite of the frame on screen, while it has overlays.
        let mut overlay: Option<Vec<u16>> = None;
        loop {
            if self.sys.should_quit()
                || (hnm.frame_count() != 0 && hnm.frames_decoded() >= hnm.frame_count())
            {
                break;
            }
            if self.sys.millis() - start >= hnm.next_frame_start_ms() {
                let decoded = match hnm.next_frame() {
                    Some(Ok(_)) => Some(Ok(())),
                    Some(Err(e)) => Some(Err(e)),
                    None => None,
                };
                // The previous frame's overlays into its buffer, the out
                // buffer of the next decode.
                if let (Some(Ok(())), Some(c)) = (&decoded, overlay.take()) {
                    if let Some(prev) = hnm.previous_frame_mut() {
                        prev.copy_from_slice(&c);
                    }
                }
                match decoded {
                    Some(Ok(())) => {
                        let frame = hnm.frame();
                        {
                            let mut screen = self.sys.screen.borrow_mut();
                            for y in 0..h.min(SCREEN_H) {
                                let n = w.min(SCREEN_W);
                                screen[y * SCREEN_W..y * SCREEN_W + n]
                                    .copy_from_slice(&frame[y * w..y * w + n]);
                            }
                            // The frame presenter FUN_0041b064 draws the SPF
                            // sprites, then the guests' portraits.
                            for spf in &spfs {
                                composite_spf_frame(
                                    spf,
                                    spf_frame,
                                    &mut screen,
                                    SCREEN_W,
                                    SCREEN_H,
                                );
                            }
                            // Not on the frame presented after an Escape
                            // poll: that poll sets the menu request
                            // 0x00496138 (0x0041a423), cleared only after
                            // that present (0x0041be4a, 0x0041bf79).
                            let guests =
                                self.guests_shown && self.guests.count != 0 && !run.s.menu_request;
                            if guests {
                                self.draw_guests(&mut screen, None);
                            }
                            // The presenter draws these overlays INTO the
                            // frame's decode buffer (0x0041b187..0x0041b192,
                            // then the copy to the surface @ 0x0041b1ff),
                            // and FUN_0041bc74 presents frame k only after
                            // decoding frame k + 1 (0x0041bde7), so the
                            // decode of frame k + 2, which writes into
                            // frame k's buffer, reads them in its in-place
                            // motion copies. The port presents each frame
                            // at once and writes the composite back into
                            // its buffer once the next frame is decoded,
                            // the last one after the loop (0x0041be45),
                            // which the next video keeps.
                            if !spfs.is_empty() || guests {
                                let mut c = frame.to_vec();
                                for y in 0..h.min(SCREEN_H) {
                                    let n = w.min(SCREEN_W);
                                    c[y * w..y * w + n]
                                        .copy_from_slice(&screen[y * SCREEN_W..y * SCREEN_W + n]);
                                }
                                overlay = Some(c);
                            }
                            // Then, on the screen, a CINEMA caption
                            // (atlantis-cd-zh-tw-696832 FUN_0041afa0 @
                            // 0x0041b13b..0x0041b15c).
                            if let Some(c) = self.cinema_captions.as_mut() {
                                let t = (self.sys.millis() - start) / 5;
                                let t = u32::try_from(t).unwrap_or(u32::MAX);
                                if let Some(spr) =
                                    c.present(t).and_then(|f| c.bank.get(usize::from(f)))
                                {
                                    let (x, y) = (i32::from(c.spec.x), i32::from(c.spec.y));
                                    spr.draw(&mut screen, SCREEN_W, SCREEN_H, x, y);
                                }
                            }
                        }
                        // FUN_0041bc74 decodes the first frame before its
                        // loop; each pass decodes the next, then runs
                        // FUN_00432758.
                        if !first {
                            self.ambient_video_frame();
                            // The NewSound poller after the decode
                            // (0x0041bdcb): a sound that ends during the
                            // film clears NEWSOUND then.
                            self.poll_new_sound().await;
                        }
                        first = false;
                        spf_frame += 1;
                        if hnm.frame_count() > 1 {
                            self.sys.hd_video.borrow_mut().t =
                                (run.presented) as f32 / (hnm.frame_count() - 1) as f32;
                        }
                        // HD: a video's frame is smoothed as a photograph.
                        self.sys.mark_photo();
                        // The frame after the ending poll (0x0041be2e) or the
                        // last one: no poll follows it.
                        let last =
                            hnm.frame_count() != 0 && hnm.frames_decoded() >= hnm.frame_count();
                        if run.frame_presented(last) {
                            self.sys.frame().await;
                            break;
                        }
                        // FUN_0041a298, once per presented frame (0x0041bdf7).
                        let (mut left, mut right, mut escape) = (false, false, false);
                        while let Some(ev) = self.sys.poll_event() {
                            match ev {
                                Input::MouseDown {
                                    button: MouseButton::Left,
                                    ..
                                } => left = true,
                                Input::MouseDown {
                                    button: MouseButton::Right,
                                    ..
                                } => right = true,
                                Input::KeyDown(crate::sys::Key::Escape) => escape = true,
                                _ => {}
                            }
                        }
                        self.poll_view_keys(left);
                        run.poll(left, right, escape);
                    }
                    Some(Err(e)) => {
                        log::warn!("{name}: {e}");
                        overlay = None;
                        break;
                    }
                    None => break,
                }
            } else if !first {
                // HD: a video's frame is smoothed as a photograph.
                self.sys.mark_photo();
            }
            self.sys.frame().await;
        }
        // A video that ends by itself leaves its sound playing to its end
        // (FUN_0042a100 @ 0x0042a907..0x0042a954 stops nothing); one cut
        // short fades it out, 2 volume steps every 25 ms from the effects
        // volume (FUN_0041bc74 @ 0x0041be69..0x0041be9e arms 0x00495eb0 =
        // 2, with no test of the sample: its end clears NEWSOUND even when
        // no sound played). Then 0x00496124 = 1, 0x00496134 = 0 and the
        // input flags are cleared (0x0041be55..0x0041bf8d).
        // The last frame's overlays into its own buffer (0x0041be45).
        if let Some(c) = overlay.take() {
            hnm.frame_mut().copy_from_slice(&c);
        }
        let fade = run.finish(frame_ms == CINEMA_FRAME_MS, &mut self.bar);
        self.escape_request = false;
        if fade {
            self.music.fade_out_new_sound(self.effects_volume());
        }
        let shown = run.presented;
        self.ix_buffers = hnm.into_buffers();
        self.emit(Event::Video {
            name,
            frames: shown,
            sprites: spfs.len(),
        });
    }

    /// The centre of a place (0x00554510 + 12 * place id), zero without an
    /// entry.
    fn centre(&self, place: u16) -> CentrePos {
        self.centres
            .get(&i32::from(place))
            .copied()
            .unwrap_or_default()
    }

    /// `FUN_00431a1c`, at the start of the warp loop (0x0041c012).
    pub(crate) async fn ambient_enter_place(&mut self) {
        let Some(place) = self.place().cloned() else {
            return;
        };
        let objects = &self.wam.ambient_objects;
        // The variants of the place's objects, read before FUN_00431a1c asks.
        let names: Vec<String> = place
            .ambient_refs
            .iter()
            .filter_map(|r| objects.iter().rfind(|o| o.id == r.object_id))
            .flat_map(|o| o.variants.iter().filter(|v| !v.is_empty()))
            .map(|v| format!("WAV\\{v}"))
            .collect();
        self.prefetch(&names).await;
        let objects = &self.wam.ambient_objects;
        let big = &self.game.big;
        let mut loaded = Vec::new();
        self.ambient
            .enter_place(&mut self.ambient_out, &place, objects, |obj, k| {
                let name = &objects[obj].variants[k];
                match big.read(&format!("WAV\\{name}")) {
                    Some(d) => {
                        loaded.push((obj, k, d));
                        true
                    }
                    None => {
                        log::warn!("cannot load the ambient sound WAV\\{name}");
                        false
                    }
                }
            });
        for (obj, k, d) in loaded {
            self.ambient_out.load(obj, k, &d);
        }
    }

    /// `FUN_00432178`'s ambient part, at the end of a warp frame (its
    /// tail, the NewSound poll, is [`Engine::poll_new_sound`]).
    pub(crate) fn ambient_frame(&mut self) {
        if !self.ambient.active() {
            return;
        }
        let (Some(place), Some(id)) = (self.place(), self.current_place_id) else {
            return;
        };
        let cam = self.centre(id);
        // The pause test and the pause start read the CON variable
        // CHAPTERTIME (@ 0x0043220f, 0x004323e9), which only FUN_0041c75c
        // writes (@ 0x0041c7c0), at the start of a warp frame: it stands
        // still in the bar, the turn and the videos.
        let t = self.vars.get("chaptertime");
        let (a, b) = (self.warp.alpha(), self.warp.beta());
        let place = place.clone();
        self.ambient.frame(
            &mut self.ambient_out,
            &place,
            &self.wam.ambient_objects,
            &cam,
            a,
            b,
            t,
        );
    }

    /// `FUN_0043259c`, between the HNM open and `FUN_0041bc74`: the steps
    /// toward the gains at the destination's camera for the sounds of the
    /// current place. Not gated by 0x00496980.
    fn ambient_prepare_glide(&mut self, to: u16, frames: i32) {
        if frames <= 0 {
            return;
        }
        let Some(place) = self.place().cloned() else {
            return;
        };
        let cam = self.centre(to);
        let (a, b) = (self.warp.alpha(), self.warp.beta());
        self.ambient
            .prepare_glide(&place, &self.wam.ambient_objects, &cam, a, b, frames);
    }

    /// `FUN_00432758`, on each frame of the video loop after the first.
    fn ambient_video_frame(&mut self) {
        if !self.ambient.active() {
            return;
        }
        let Some(place) = self.place().cloned() else {
            return;
        };
        // CHAPTERTIME as the warp frame left it (@ 0x004327ce, 0x00432978).
        let t = self.vars.get("chaptertime");
        self.ambient
            .video_frame(&mut self.ambient_out, &place, &self.wam.ambient_objects, t);
    }

    /// `recordPlaceTable`: the WAM's transitions written into the place
    /// table slots, over what earlier loads left there.
    pub(crate) fn record_place_table(&mut self) {
        let places = &self.wam.places;
        if self.place_table_slots.len() < places.len() {
            self.place_table_slots.resize(places.len(), Vec::new());
        }
        for (r, p) in places.iter().enumerate() {
            let slots = &mut self.place_table_slots[r];
            if slots.len() < p.transitions.len() {
                slots.resize(p.transitions.len(), Transition::default());
            }
            slots[..p.transitions.len()].clone_from_slice(&p.transitions);
        }
    }

    /// Entry `i` of record `record` of the place table: never written in
    /// this run, the zeroed table of a fresh start (empty name, destination
    /// 0, angles 0).
    fn place_table_entry(&self, record: usize, i: usize) -> Transition {
        self.place_table_slots
            .get(record)
            .and_then(|s| s.get(i))
            .cloned()
            .unwrap_or_default()
    }

    /// `FUN_00423ad8` after a departure section that ran Vue=N without
    /// AbortDepart (CHAPI062 /864): the Vue= handler has already pointed
    /// the place record 0x006cd7a8 at N and loaded its panorama, and the
    /// loop goes on with the index i of the clicked transition of the place
    /// left: TESTTIME1 and Time1 restart (@ 0x00423e8d), the turn
    /// `FUN_004212cc(i)`, then the video "ubb_vue\" + the name of entry i of
    /// N's record, a stale slot past N's transition count. An empty name has
    /// no '.', so `FUN_00422f2c` returns -1 and no video plays (the
    /// fresh-start case).
    pub(crate) async fn transition_after_departure_vue(&mut self, index: usize) {
        self.vars.set("testtime1", 0);
        self.clocks.time1_start = self.now();
        self.clocks.time1_offset = 0;
        let (Some(record), Some(cur)) = (self.current_place, self.current_place_id) else {
            return;
        };
        let t = self.place_table_entry(record, index);
        self.turn_to_transition(t.src_alpha, t.src_beta).await;
        // 0x00421b01..0x00421b31, before the video.
        self.set_view(t.dst_alpha, t.dst_beta);
        // The move's tail, as for any move: FUN_0044a424(0x006cd288) @
        // 0x00423ed4 loads the place the Vue= selected (its name built @
        // 0x0042a66d / 0x0042a69d): the warp FOV 75.137 again after the
        // turn's 75.187 (0x0044a6fc..0x0044a706), the pointer at (0x140,
        // 0x140).
        if let Some(cyclo) = self.wam.places.get(record).map(|p| p.cyclo.clone()) {
            self.load_cyclo(&cyclo).await;
        }
        // The same double-click test (0x00423f8a) and reset (0x00424013).
        if t.video.contains('.') && !self.turn_double_click.flag {
            self.play_transition_video(&t.video, Some((cur, cur)), TRANSITION_FRAME_MS)
                .await;
        }
        self.turn_double_click.clear();
        // Then FUN_00421b4c (no load) and FUN_0042431c (@ 0x00424072):
        // the place's sprites again, from frame 0.
        self.composite_npc_sprites().await;
    }

    /// Cinema=N: `CINEM<NNN>.HNM` full screen with its sound, at the cinema
    /// pace (`FUN_0042a100` @ 0x0042a7c0..0x0042a954).
    pub async fn play_cinema(&mut self, n: i32) {
        let name = format!("CINEM{n:03}.HNM");
        // The Chinese builds' captions: CINEMA=20 loads SUBCHINE.SPR and
        // resets the frame counter (atlantis-cd-zh-tw-696832 FUN_00429958 @
        // 0x00429fbe..0x0042a053), frees the bank after the video
        // (@ 0x0042a224..0x0042a235). The CINEMA value stays 20 after it
        // (0x004865a1 is written only there), so a later video of
        // FUN_0041bd04 past 1200 ticks would blit from the freed bank; the
        // port draws the captions over this video only.
        self.cinema_captions = None;
        if let Some(spec) = self
            .game
            .exe
            .build
            .cinema_captions
            .filter(|c| i32::from(c.cinema) == n)
        {
            let bank = self
                .read(&format!("SPRITE\\2D\\{}", spec.bank))
                .await
                .and_then(|d| {
                    let (w, h) = atlantis_data::sprite::limits::DEFAULT;
                    atlantis_data::sprite::read_spr_file(&d, w, h)
                })
                .unwrap_or_default();
            self.cinema_captions = Some(CinemaCaptionState {
                spec,
                bank,
                frame: 0,
            });
        }
        self.play_transition_video(&name, None, CINEMA_FRAME_MS)
            .await;
        self.cinema_captions = None;
    }

    /// The end of a NewSound fade-out (`FUN_0042f7a8` @ 0x0042f92f):
    /// NEWSOUND = 0.
    fn apply_new_sound_fade_end(&mut self) {
        if self.music.take_new_sound_fade_end() {
            self.vars.set("newsound", 0);
        }
    }

    /// `FUN_00432a64`, the NewSound poller. The loop flag 0x00496194 is 0
    /// in the image and never written, so a sound plays once.
    pub async fn poll_new_sound(&mut self) {
        self.poll_new_sound_named(None).await;
    }

    /// The poller; `cinema` is the cinematic's sound the Cinema= handler
    /// names while 0x00496190 is 1 (in place of the table's entry).
    async fn poll_new_sound_named(&mut self, cinema: Option<String>) {
        self.apply_new_sound_fade_end();
        let value = self.vars.get("newsound");
        if value == 0 {
            return; // 1: nothing is stopped
        }
        let active = self.music.new_sound_active();
        if value != self.music.new_sound_last {
            // 3: another value.
            self.music.stop_new_sound();
            self.music.new_sound_playing = false;
        }
        if self.music.new_sound_playing {
            if !active {
                // 4: done (status 2).
                self.music.stop_new_sound();
                self.music.new_sound_playing = false;
                self.vars.set("newsound", 0);
            }
            return;
        }
        // 5: wav\ + entry `value` of the table at 0x00496224, or the
        // cinematic's name.
        // (The event names a cinematic's sound with its folder, as before.)
        let event_name = cinema.clone();
        let Some(name) = cinema
            .map(|c| c.trim_start_matches("WAV\\").to_string())
            .or_else(|| {
                usize::try_from(value)
                    .ok()
                    .and_then(|v| self.game.exe.newsound_names.get(v))
                    .cloned()
            })
        else {
            log::warn!("newsound={value} out of the table");
            self.vars.set("newsound", 0);
            return;
        };
        // A missing file or a bad header: NEWSOUND = 0.
        let path = format!("WAV\\{name}");
        let data = if self.game.big.contains(&path) {
            self.read(&path).await
        } else {
            None
        };
        let Some(pcm) = data.and_then(|d| apc_pcm(&d)) else {
            log::warn!("newsound={value}: {path} not played");
            self.vars.set("newsound", 0);
            return;
        };
        self.music.start_new_sound(pcm, self.effects_volume());
        self.music.new_sound_playing = true;
        self.music.new_sound_last = value;
        self.emit(Event::NewSound(value, event_name.unwrap_or(name)));
    }
}

#[cfg(test)]
mod caption_tests {
    use super::CinemaCaptionState;

    /// atlantis-cd-zh-tw-696832's CINEMA 20 captions: nothing up to tick
    /// 1200 and from 18800; the frame drawn, then stepped, several steps
    /// in one call when the presenter is late.
    #[test]
    fn caption_frames_by_tick() {
        let mut c = CinemaCaptionState {
            spec: atlantis_data::exe::ZH_TW.cinema_captions.unwrap(),
            bank: Vec::new(),
            frame: 0,
        };
        assert_eq!(c.present(1200), None);
        assert_eq!(c.present(1201), Some(0));
        assert_eq!(c.present(2200), Some(0));
        assert_eq!(c.present(2201), Some(0));
        assert_eq!(c.frame, 1);
        // Late: 4400 and 5000 both passed, two steps after drawing frame 1.
        assert_eq!(c.present(5001), Some(1));
        assert_eq!(c.frame, 3);
        assert_eq!(c.present(16401), Some(3));
        assert_eq!(c.frame, 10);
        assert_eq!(c.present(18799), Some(10));
        assert_eq!(c.present(18800), None);
        assert_eq!(c.frame, 10);
    }
    /// atlantis-dvd-ja-696832's CINEMA 20 captions (FUN_0041b104 @
    /// 0x0041b269..0x0041b46c): 13 SUBINTRO frames, 12 steps, nothing up to
    /// tick 1200 and from 18000.
    #[test]
    fn japanese_caption_frames_by_tick() {
        let spec = atlantis_data::exe::DVD_JA.cinema_captions.unwrap();
        let mut c = CinemaCaptionState {
            spec,
            bank: Vec::new(),
            frame: 0,
        };
        assert_eq!(c.present(1200), None);
        assert_eq!(c.present(2201), Some(0));
        assert_eq!(c.frame, 1);
        // 5800 is a step of its own (the Chinese builds' 5000).
        assert_eq!(c.present(5000), Some(1));
        assert_eq!(c.frame, 2);
        assert_eq!(c.present(5801), Some(2));
        assert_eq!(c.frame, 3);
        assert_eq!(c.present(16401), Some(3));
        assert_eq!(c.frame, 12);
        assert_eq!(c.present(17999), Some(12));
        assert_eq!(c.present(18000), None);
    }
}

#[cfg(test)]
mod wsprframe_tests {
    use super::*;
    use crate::audio::Mixer;
    use crate::sys::{Runner, Sys};

    /// The slave pacing in the ATLAN5 alley (SPWATL5.TXT, perso 529, type
    /// 1, 104 frames at view 126): the animation feeds WSPRFRAME, which
    /// CHAPI013's flower pot tests (it stayed 0).
    #[test]
    fn place_animation_feeds_wsprframe() {
        let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
            eprintln!("ATLANTIS_DATA not set: skipped");
            return;
        };
        let g = atlantis_data::Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(dir)))
            .unwrap();
        let sys = Sys::new(Mixer::new());
        let s2 = sys.clone();
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen2 = seen.clone();
        let mut r = Runner::new(sys.clone(), async move {
            let mut e = Engine::new(s2, g, Rc::new(crate::hooks::DirectLoader));
            assert!(e.load_wam("ATLAN5.WAM").await);
            e.next_place_id = Some(126);
            e.do_place_change().await;
            for _ in 0..400 {
                e.animate_sprites();
                seen2.borrow_mut().push(e.vars.get("wsprframe"));
                e.sys.frame().await;
            }
        });
        let mut t = 0;
        while r.step(t) {
            t += 16;
        }
        let seen = seen.borrow();
        // 400 frames of 16 ms: a step every 70 ms, past frame 33.
        assert!(
            seen.iter().any(|&f| (8..=33).contains(&f)),
            "{:?}",
            &seen[..40]
        );
        assert!(*seen.iter().max().unwrap() > 33);
    }
}

#[cfg(test)]
mod sprite_draw_order_tests {
    use super::*;
    use crate::audio::Mixer;
    use crate::sys::{Runner, Sys};

    /// `FUN_00425304` draws a row at its frame, then steps it: the frame a
    /// step makes is on the panorama only from the next pass. The slave
    /// pacing in the ATLAN5 alley (view 126, type 1, 104 frames).
    #[test]
    fn place_sprite_step_is_drawn_on_the_next_pass() {
        let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
            eprintln!("ATLANTIS_DATA not set: skipped");
            return;
        };
        let g = atlantis_data::Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(dir)))
            .unwrap();
        let sys = Sys::new(Mixer::new());
        let s2 = sys.clone();
        let got = Rc::new(std::cell::RefCell::new(None));
        let g2 = got.clone();
        let mut r = Runner::new(sys.clone(), async move {
            let mut e = Engine::new(s2, g, Rc::new(crate::hooks::DirectLoader));
            assert!(e.load_wam("ATLAN5.WAM").await);
            e.next_place_id = Some(126);
            e.do_place_change().await;
            let frames = |e: &Engine| -> Vec<i32> {
                e.sprite_list
                    .iter()
                    .filter(|r| r.place_id == 126)
                    .map(|r| r.frame)
                    .collect()
            };
            for _ in 0..400 {
                let before = frames(&e);
                let shown = e.warp_surface.clone();
                e.animate_sprites();
                if frames(&e) != before {
                    // The step's pass shows the frames from before it.
                    let same_pass = e.warp_surface == shown;
                    e.sys.frame().await;
                    e.animate_sprites();
                    let next_pass = e.warp_surface != shown;
                    *g2.borrow_mut() = Some((same_pass, next_pass));
                    return;
                }
                e.sys.frame().await;
            }
        });
        let mut t = 0;
        while r.step(t) {
            t += 16;
        }
        assert_eq!(*got.borrow(), Some((true, true)));
    }
}

#[cfg(test)]
mod one_shot_residue_tests {
    use super::*;
    use crate::audio::Mixer;
    use crate::sys::{Runner, Sys};

    /// A type-2 row's last frame stays on the panorama after its cycle
    /// ends (FUN_004255f0 restores only the rows still active, dc:17688)
    /// until the panorama is loaded again; a type-1 row leaves nothing.
    /// The ATLAN5 alley's slave row (view 126, 104 frames), as type 2 and
    /// as type 1.
    fn residue(kind: i32) -> Option<(usize, usize)> {
        let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
            eprintln!("ATLANTIS_DATA not set: skipped");
            return None;
        };
        let g = atlantis_data::Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(dir)))
            .unwrap();
        let sys = Sys::new(Mixer::new());
        let s2 = sys.clone();
        let got = Rc::new(std::cell::Cell::new(None));
        let g2 = got.clone();
        let mut r = Runner::new(sys.clone(), async move {
            let mut e = Engine::new(s2, g, Rc::new(crate::hooks::DirectLoader));
            assert!(e.load_wam("ATLAN5.WAM").await);
            e.next_place_id = Some(126);
            e.do_place_change().await;
            let pristine = e.cyclo.clone();
            let row = e
                .sprite_list
                .iter()
                .position(|r| r.place_id == 126 && r.perso_id == 529)
                .expect("the slave row");
            e.sprite_list[row].kind = kind;
            e.sprite_list[row].frame = e.sprite_list[row].frame_count - 2;
            let diff = |e: &Engine| {
                e.cyclo
                    .iter()
                    .zip(&pristine)
                    .filter(|(a, b)| a != b)
                    .count()
            };
            for _ in 0..40 {
                e.animate_sprites();
                e.sys.frame().await;
            }
            let after_end = diff(&e);
            e.reload_current_view().await;
            g2.set(Some((after_end, diff(&e))));
        });
        let mut t = 0;
        while r.step(t) {
            t += 16;
        }
        got.get()
    }

    #[test]
    fn one_shot_last_frame_stays_until_the_panorama_reloads() {
        let Some((kept, reloaded)) = residue(2) else {
            return;
        };
        assert!(kept > 0, "no residue");
        assert_eq!(reloaded, 0);
        assert_eq!(residue(1), Some((0, 0)));
    }
}

#[cfg(test)]
mod move_video_skip_tests {
    use super::*;
    use crate::audio::Mixer;
    use crate::sys::{Runner, Sys};

    /// Widescreen HD with panorama sides: a move whose video plays keeps
    /// the video's sides until the destination's view is shown; a move
    /// whose video a double click skipped has none, so the turn's last
    /// frame is not drawn beside the destination's sides.
    #[test]
    fn skipped_move_video_has_no_video_sides() {
        let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
            eprintln!("ATLANTIS_DATA not set: skipped");
            return;
        };
        for skip in [false, true] {
            let g =
                atlantis_data::Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(&dir)))
                    .unwrap();
            let sys = Sys::new(Mixer::new());
            sys.set_hd_tile(Some(24));
            sys.set_hd_side_tiles(2);
            sys.set_hd_options(crate::hd::HdOptions {
                widescreen: true,
                video_sides: crate::hd::VideoSides::Panorama,
                ..Default::default()
            });
            let s2 = sys.clone();
            let out = Rc::new(std::cell::RefCell::new(None));
            let o2 = out.clone();
            let mut r = Runner::new(sys.clone(), async move {
                let mut e = Engine::new(s2, g, Rc::new(crate::hooks::DirectLoader));
                assert!(e.load_wam("ATLAN5.WAM").await);
                e.next_place_id = Some(126);
                e.do_place_change().await;
                let next = e
                    .place()
                    .and_then(|p| p.transitions.iter().find(|t| t.video.contains('.')))
                    .map(|t| t.dst_place)
                    .expect("no move with a video at view 126");
                e.turn_double_click.flag = skip;
                e.execute_transition(next).await;
                let v = e.sys.hd_video.borrow();
                *o2.borrow_mut() = Some((v.active, v.t));
            });
            let mut t = 0;
            while r.step(t) {
                t += 16;
            }
            let got = out.borrow().expect("the move did not end");
            if skip {
                assert!(!got.0, "skipped video: the sides are on");
            } else {
                assert_eq!(got, (true, 1.0));
            }
        }
    }
}

#[cfg(test)]
mod escape_tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::engine::Event;
    use crate::sys::{Input, Key, MouseButton};
    use crate::test_support::with_engine;
    use crate::turn::{transition_turn, TransitionTurn};
    use crate::warp::Warp;

    /// The yaw after the turn from (0, 0) to the source (0.3, 0) of place
    /// 1, F5 held (`f5`) or not, and the number of frames.
    fn turn_with_f5(test: &str, f5: bool) -> Option<(f32, u64)> {
        let got = Rc::new(Cell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            test,
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.set_view(0.0, 0.0);
                if f5 {
                    e.sys.push_input(Input::KeyDown(Key::F(5)));
                    while e.sys.poll_event().is_some() {}
                }
                let start = e.sys.millis();
                e.turn_to_transition(0.3, 0.0).await;
                g2.set(Some((e.warp.alpha(), (e.sys.millis() - start) / 16)));
            },
            |_, _| {},
        );
        if ran {
            got.get()
        } else {
            None
        }
    }

    /// FUN_004212cc's poll after each frame (0x00421670 / 0x00421a5a)
    /// moves the stored angles by the held view keys, and the next step
    /// and phase 1's stop test read them: F5 held turns the frames ahead
    /// and, for 0 -> 0.3, ends phase 1 on the 0.05 test (9 frames, not 11).
    #[test]
    fn f5_moves_the_turn_frames() {
        let Some((still, n_still)) = turn_with_f5("f5_moves_the_turn_frames", false) else {
            return;
        };
        let (turned, n_turned) = turn_with_f5("f5_moves_the_turn_frames", true).unwrap();
        assert_ne!(turned, still);
        assert_eq!((n_still, n_turned), (11, 9));
    }

    /// A second press of a double click read by a poll of the turn: the
    /// poll returns before its message pump (0x0041a2b5), so the flag
    /// 0x00496118 is set by the next poll, not this one.
    #[test]
    fn the_turn_poll_with_a_press_does_not_pump() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "the_turn_poll_with_a_press_does_not_pump",
            move |mut e| async move {
                e.cursor_state.mode = 5;
                let now = e.sys.millis();
                e.turn_double_click.first_press(now, 320, 240);
                e.sys.push_input(Input::MouseDown {
                    button: MouseButton::Left,
                    x: 320,
                    y: 240,
                });
                e.turn_poll();
                g2.borrow_mut().push(e.turn_double_click.flag);
                e.turn_poll();
                g2.borrow_mut().push(e.turn_double_click.flag);
            },
            |_, _| {},
        );
        if ran {
            assert_eq!(*got.borrow(), vec![false, true]);
        }
    }

    /// The end of a move polls once (`FUN_0041a298` @ 0x00424093): a held
    /// F5 moves the stored angles there.
    #[test]
    fn f5_at_the_end_of_move_poll() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "f5_at_the_end_of_move_poll",
            move |mut e| async move {
                e.set_view(1.0, 0.0);
                e.sys.push_input(Input::KeyDown(Key::F(5)));
                e.move_input_poll = true;
                e.end_of_move_input();
                g2.borrow_mut().push(e.stored_view().0);
                // A left press in that poll returns before the keys.
                e.set_view(1.0, 0.0);
                e.sys.push_input(Input::MouseDown {
                    button: MouseButton::Left,
                    x: 0,
                    y: 0,
                });
                e.move_input_poll = true;
                e.end_of_move_input();
                g2.borrow_mut().push(e.stored_view().0);
            },
            |_, _| {},
        );
        if ran {
            let got = got.borrow();
            assert_eq!(got[0], (1.0f64 + 0.034906) as f32);
            assert_eq!(got[1], 1.0);
        }
    }

    /// The turn frame by frame: with 0x00496124 set all along, the frames
    /// of `transition_turn`; cleared after the first frame, phase 1 ends
    /// there and the five frames of phase 2 follow.
    #[test]
    fn turn_phase_one_ends_with_the_run_flag() {
        let pi = Warp::exe_pi();
        let (all, _, _) = transition_turn(pi, 0.0, 0.0, 2.5, 0.3, false);
        assert!(all.len() > 7, "{} frames", all.len());
        let mut t = TransitionTurn::new(pi, 0.0, 0.0, 2.5, 0.3, false);
        let first = t.next_frame(true).unwrap();
        assert_eq!(first, all[0]);
        let mut n = 1;
        while t.next_frame(false).is_some() {
            n += 1;
        }
        assert_eq!(n, 6);
    }

    /// Escape during a move's turn (`FUN_0041a298` from `FUN_004212cc`)
    /// clears 0x00496124: the video loop does not run and its exit shows
    /// the first frame only (0x0041bd75, 0x0041be4a), then the flags are
    /// back.
    #[test]
    fn escape_in_the_turn_cuts_the_move_video() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let armed = Rc::new(Cell::new(0u32));
        let a2 = armed.clone();
        let ran = with_engine(
            "escape_in_the_turn_cuts_the_move_video",
            move |mut e| async move {
                let trace = Rc::new(RefCell::new(Vec::new()));
                e.trace = Some(trace.clone());
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                let dest = e
                    .place()
                    .and_then(|p| p.transitions.iter().find(|t| !t.video.is_empty()))
                    .map(|t| t.dst_place)
                    .expect("a move with a video from the first place");
                a2.set(1);
                e.execute_transition(dest).await;
                let frames = trace.borrow().iter().find_map(|ev| match ev {
                    Event::Video { frames, .. } => Some(*frames),
                    _ => None,
                });
                *g2.borrow_mut() = Some((frames, e.escape_request));
            },
            move |sys, _| {
                let n = armed.get();
                if n > 0 {
                    armed.set(n + 1);
                    if n == 2 {
                        sys.push_input(Input::KeyDown(Key::Escape));
                    }
                }
            },
        );
        if ran {
            assert_eq!(*got.borrow(), Some((Some(1), false)));
        }
    }

    /// The subject menu steps the place sprites on every frame
    /// (`FUN_004261a0` @ 0x0042622b), and an Escape there sets the menu
    /// request (the guests are no longer drawn) without ending it.
    #[test]
    fn subject_menu_steps_the_sprites_and_takes_escape() {
        use crate::scripts::DialogPresenter;
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let armed = Rc::new(Cell::new(0u32));
        let a2 = armed.clone();
        let ran = with_engine(
            "subject_menu_steps_the_sprites_and_takes_escape",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                let t0 = e.sprite_anim_tick;
                a2.set(1);
                let mut d = crate::Dialogs::new();
                d.subject_menu(&mut e, 600, &[1, 5]).await;
                *g2.borrow_mut() = Some((e.sprite_anim_tick > t0 + 10, e.escape_request));
            },
            move |sys, _| {
                let n = armed.get();
                if n > 0 {
                    armed.set(n + 1);
                    if n == 20 {
                        sys.push_input(Input::KeyDown(Key::Escape));
                    } else if n == 40 {
                        sys.push_input(Input::MouseDown {
                            button: MouseButton::Left,
                            x: 10,
                            y: 10,
                        });
                    } else if n == 41 {
                        sys.push_input(Input::MouseUp {
                            button: MouseButton::Left,
                            x: 10,
                            y: 10,
                        });
                    }
                }
            },
        );
        if ran {
            assert_eq!(*got.borrow(), Some((true, true)));
        }
    }
}

#[cfg(test)]
mod move_panorama_tests {
    use super::*;
    use crate::test_support::with_engine;
    use std::cell::RefCell;

    /// The panorama `CYCLO\<name>` decoded.
    async fn pano(e: &Engine, name: &str) -> Vec<u16> {
        let d = e.read(&format!("CYCLO\\{name}")).await.expect(name);
        atlantis_codecs::decode_panorama(&d).unwrap().pixels
    }

    /// The sub-WAM move BIBLION 230 -> PALAIS3 192 shows the picture the
    /// click named in BIBLION's record of 192 (FUN_00421cfc @ 0x00423dc5,
    /// loaded @ 0x00423ed4 before the swap), ATL16192, not PALAIS3's
    /// ATN16192: FUN_00421b4c loads nothing after the swap.
    #[test]
    fn a_sub_wam_move_shows_the_source_wams_picture() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "a_sub_wam_move_shows_the_source_wams_picture",
            move |mut e| async move {
                assert!(e.load_wam("BIBLION.WAM").await);
                let rec = |e: &Engine| {
                    e.wam
                        .places
                        .iter()
                        .find(|p| p.id == 192)
                        .map(|p| p.cyclo.to_ascii_uppercase())
                };
                assert_eq!(rec(&e).as_deref(), Some("ATL16192.HNM"));
                e.next_place_id = Some(230);
                e.do_place_change().await;
                e.execute_transition(192).await;
                e.do_place_change().await;
                assert_eq!(e.wam_name, "PALAIS3.WAM");
                assert_eq!(e.current_place_id, Some(192));
                assert_eq!(rec(&e).as_deref(), Some("ATN16192.HNM"));
                let atl = pano(&e, "ATL16192.HNM").await;
                let atn = pano(&e, "ATN16192.HNM").await;
                *g2.borrow_mut() = Some((e.cyclo == atl, e.cyclo == atn));
            },
            |_, _| {},
        );
        if ran {
            assert_eq!(*got.borrow(), Some((true, false)));
        }
    }

    /// A departure section that ran Vue= (CHAPI062 /864) takes the move's
    /// tail: FUN_0044a424 @ 0x00423ed4 loads the place the Vue= selected,
    /// so the warp FOV is 75.137 again after the turn's 75.187, and the
    /// pointer is at (0x140, 0x140).
    #[test]
    fn a_departure_vue_reloads_the_place() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "a_departure_vue_reloads_the_place",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.pointer.pos = (5, 5);
                e.transition_after_departure_vue(0).await;
                *g2.borrow_mut() = Some((e.warp.hfov(), e.pointer.pos));
            },
            |_, _| {},
        );
        if ran {
            let (fov, pos) = got.borrow().unwrap();
            assert!((fov - WARP_FOV_DEG).abs() < 1e-4, "FOV {fov}");
            assert_ne!(fov, TURN_END_FOV_DEG);
            assert_eq!(pos, (0x140, 0x140));
        }
    }
}
