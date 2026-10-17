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
use crate::turn::{transition_turn, TURN_END_FOV_DEG};
use crate::video_skip::VideoSkipState;
use crate::warp::{Warp, PANO_W, SCREEN_H, SCREEN_W, WARP_FOV_DEG};

/// The frame delay of a move's transition video: 6 ticks of the 200 Hz
/// timer.
pub const TRANSITION_FRAME_MS: u64 = 6 * 5;
/// The frame delay of a cinematic (Cinema=, the start-up videos).
pub const CINEMA_FRAME_MS: u64 = 13 * 5;

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
        let Some(index) = self.wam.places.iter().position(|p| p.id == next) else {
            log::warn!("place {next} not in {}", self.wam_name);
            self.next_place_id = None;
            return;
        };
        self.current_place = Some(index);
        self.current_place_id = Some(next);
        self.next_place_id = None;
        self.vue_override = None; // `Vue` follows the place again
        let cyclo = self.wam.places[index].cyclo.clone();
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

    /// The current place's view loaded again, as `FUN_004240fc` does after
    /// a puzzle: its panorama (the pointer left at (0x140, 0x140)) and its
    /// sprites (frame 0, the NPC click areas rebuilt). The place is not
    /// entered again: no place event, the mouse is not moved.
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
        self.warp_surface.copy_from_slice(&self.cyclo);
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

    /// `FUN_00425304` once per frame: the test "more than 13 ticks" of the
    /// 200 Hz counter is run for each tick elapsed since the last call (at
    /// most 2 s, e.g. after a video), as the original, which does not wait
    /// for the display, steps every 14 or 15 ticks.
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
        let mut changed = false;
        let current = self.sprite_anim_current[usize::from(vue)];
        // FUN_00425304 writes the CON variable WSPRFRAME itself (the frame
        // of the view's current animation row), which the scripts test:
        // CHAPI013's flower pot falls on the pacing slave only while
        // Wsprframe is in 8..33 or 70..96. It starts from the variable, so
        // a script's write stays until the next step changes it.
        let mut wspr = self.vars.get("wsprframe");
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
        self.wspr_frame = wspr;
        self.vars.set("wsprframe", wspr);
        if changed {
            self.recomposite_sprite_layer();
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
    }

    /// A move to `next` (`executeTransition`): the turn, the video, the
    /// sub-WAM swap.
    pub(crate) async fn execute_transition(&mut self, next: u16) {
        // After a ChangeWam= the script's next Vue= is the destination.
        let (Some(place), Some(cur)) = (self.place(), self.current_place_id) else {
            self.next_place_id = Some(next);
            return;
        };
        if let Some(t) = place.find_transition(next).cloned() {
            // FUN_00423ad8 @ 0x00423ec0: FUN_004212cc turns the view to the
            // transition's source angles before the video.
            self.turn_to_transition(t.src_alpha, t.src_beta).await;
            // FUN_004212cc ends by setting the destination angles
            // (0x00421b01..0x00421b31), before the video, so FUN_0043259c's
            // ambient glide uses them.
            self.set_view(t.dst_alpha, t.dst_beta);
            self.play_transition_video(&t.video, Some((cur, next)), TRANSITION_FRAME_MS)
                .await;
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
            self.load_wam(&sub.wam).await;
            self.current_place = None;
            self.next_place_id = Some(sub.dst_place);
            return;
        }
        self.next_place_id = Some(next);
    }

    /// `FUN_004212cc`: the turn's frames, each drawn as the warp loop draws
    /// one (place sprites, the view) without input or cursor. With ZOOM the
    /// field of view narrows by 3 degrees a frame from 75.187; the turn
    /// ends with `FUN_0045179a(75.187)`, which lasts until the next
    /// panorama load sets the warp's 75.137, and ZOOM = 0.
    pub(crate) async fn turn_to_transition(&mut self, src_alpha: f32, src_beta: f32) {
        if !self.warp_loaded {
            return;
        }
        let (frames, _, _) = transition_turn(
            Warp::exe_pi(),
            self.warp.alpha(),
            self.warp.beta(),
            src_alpha,
            src_beta,
            self.zoom || self.vars.get("zoom") != 0,
        );
        for f in frames {
            if self.sys.should_quit() {
                break;
            }
            while self.sys.poll_event().is_some() {}
            if f.fov_set {
                self.warp.set_hfov(f.fov);
            }
            self.set_view(f.alpha, f.beta);
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

    /// A video of `UBB_VUE\` (`playTransitionVideo`, `FUN_0041bc74`):
    /// `places` is the move (from, to) whose SPF sprites are drawn over it
    /// (`FUN_0041b81c`, `FUN_004256e8`), `None` for a cinematic.
    pub async fn play_transition_video(
        &mut self,
        video: &str,
        places: Option<(u16, u16)>,
        frame_ms: u64,
    ) {
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
        // 0x007728c7 "cinem" + N + ".apc" and sets NEWSOUND = 1
        // (FUN_0042a100 @ 0x0042a7c0..0x0042a881, poller FUN_00432a64). A
        // move's video (FUN_00423ad8) opens no sound: the transition-named
        // APCs are NewSound entries that the departure sections play.
        if places.is_none() {
            let apc = format!("WAV\\{}", with_ext(&name, "APC"));
            if self.game.big.contains(&apc) {
                if let Some(pcm) = self.read(&apc).await.and_then(|d| apc_pcm(&d)) {
                    self.music.start_new_sound(pcm);
                    self.music.new_sound_last = 1;
                    self.music.new_sound_playing = true;
                    self.vars.set("newsound", 1);
                    self.emit(Event::NewSound(1, apc));
                }
            }
        }

        let start = self.sys.millis();
        let mut first = true;
        let mut spf_frame = 0usize;
        let mut shown = 0u32;
        // FUN_0041bc74's input: a left click, or Escape with the conditions
        // of FUN_0041a298 (video_skip); after that poll the next frame is
        // still presented.
        // The bar request as it stands: set for a Cinema= of a timer the
        // bar loop runs, so that Escape does not end it then.
        let mut skip = VideoSkipState {
            object_in_hand: self.vars.get("objetenmain") != 0,
            section_play: self.section_play,
            bow_mode: self.bow_mode,
            bar_request: self.bar.request,
            ..VideoSkipState::default()
        };
        let mut stop_after_frame = false;
        let (w, h) = (hnm.width(), hnm.height());
        loop {
            if self.sys.should_quit()
                || (hnm.frame_count() != 0 && hnm.frames_decoded() >= hnm.frame_count())
            {
                break;
            }
            let mut presented = false;
            if self.sys.millis() - start >= hnm.next_frame_start_ms() {
                match hnm.next_frame() {
                    Some(Ok(frame)) => {
                        let mut screen = self.sys.screen.borrow_mut();
                        for y in 0..h.min(SCREEN_H) {
                            let n = w.min(SCREEN_W);
                            screen[y * SCREEN_W..y * SCREEN_W + n]
                                .copy_from_slice(&frame[y * w..y * w + n]);
                        }
                        // The frame presenter FUN_0041b064 draws the SPF
                        // sprites, then the guests' portraits.
                        for spf in &spfs {
                            composite_spf_frame(spf, spf_frame, &mut screen, SCREEN_W, SCREEN_H);
                        }
                        if self.guests_shown && self.guests.count != 0 {
                            self.draw_guests(&mut screen, None);
                        }
                        drop(screen);
                        // FUN_0041bc74 decodes the first frame before its
                        // loop; each pass decodes the next, then runs
                        // FUN_00432758.
                        if !first {
                            self.ambient_video_frame();
                        }
                        first = false;
                        spf_frame += 1;
                        shown += 1;
                        presented = true;
                    }
                    Some(Err(e)) => {
                        log::warn!("{name}: {e}");
                        break;
                    }
                    None => break,
                }
            }
            if presented && stop_after_frame {
                self.sys.frame().await; // the frame presented at 0x0041be2e
                break;
            }
            if !stop_after_frame {
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
                if skip.poll(left, right, escape) {
                    stop_after_frame = true;
                }
            }
            self.sys.frame().await;
        }
        // A video that ends by itself leaves its sound playing to its end
        // (FUN_0042a100 @ 0x0042a907..0x0042a954 stops nothing); one cut
        // short fades it out, 2 volume steps every 25 ms from the effects
        // volume (FUN_0041bc74 @ 0x0041be69..0x0041be9e arms 0x00495eb0 =
        // 2). Then the input flags are cleared (0x0041bf5b..0x0041bf8d).
        if stop_after_frame
            && skip.fades(frame_ms == CINEMA_FRAME_MS)
            && self.music.new_sound_active()
        {
            self.music.fade_out_new_sound(self.effects_volume());
        }
        self.bar.flags_reset();
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
        let t = self.chapter_time();
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
        let t = self.chapter_time();
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
        if t.video.contains('.') {
            self.play_transition_video(&t.video, Some((cur, cur)), TRANSITION_FRAME_MS)
                .await;
        }
    }

    /// Cinema=N: `CINEM<NNN>.HNM` full screen with its sound, at the cinema
    /// pace (`FUN_0042a100` @ 0x0042a7c0..0x0042a954).
    pub async fn play_cinema(&mut self, n: i32) {
        let name = format!("CINEM{n:03}.HNM");
        self.play_transition_video(&name, None, CINEMA_FRAME_MS)
            .await;
    }

    /// `FUN_00432a64`, the NewSound poller. The loop flag 0x00496194 is 0
    /// in the image and never written, so a sound plays once.
    pub async fn poll_new_sound(&mut self) {
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
        // 5: wav\ + entry `value` of the table at 0x00496224.
        let Some(name) = usize::try_from(value)
            .ok()
            .and_then(|v| self.game.exe.newsound_names.get(v))
            .cloned()
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
        self.music.start_new_sound(pcm);
        self.music.new_sound_playing = true;
        self.music.new_sound_last = value;
        self.emit(Event::NewSound(value, name));
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
            assert!(e.load_wam_with("ATLAN5.WAM", true).await);
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
