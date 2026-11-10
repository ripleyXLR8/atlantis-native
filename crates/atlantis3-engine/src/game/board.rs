//! Tests: a board (a place puzzle's C code) driven as the emulation
//! harnesses drive the original's, for the board goldens
//! (`tests/common/board.rs` replays them):
//!
//! - [`Game::board_begin`]: the place loaded as `AtlWam_Load`
//!   (@ 0x0043eb00) does, in a world, with an empty chapter (the
//!   harnesses load none: every `CtrlScenarIN` finds nothing), the C
//!   library's `rand` seeded with 1;
//! - [`Game::board_enter`]: `VUE` := v (and `LASTVUE`), the view entry
//!   (@ 0x0043f840 with the place hooks) at the camera's rest;
//! - [`Game::board_frame`]: one warp frame as the harnesses run it: the
//!   place animations' 15 Hz step (@ 0x00439c90), a sprite click
//!   (@ 0x004403e0, the hit record = the slot) or a zone click
//!   (@ 0x00420330 on the zone record), the frame hook (@ 0x004c27d0), the
//!   click scan (@ 0x004405b0) on a click frame, the click bytes and
//!   `CLICZONE` cleared, the voice slots' release (the flip @ 0x0040de60);
//! - [`Game::board_screen_frame`]: one frame of a full-screen loop mode
//!   with the pointer and the click bytes, then its stop when it returns
//!   0.
//!
//! What the C code did is in [`Game::trace`] when it is on.

use atlantis2_engine::crt::Rand;

use super::{hooks, Game, Mode};

/// The input of a board frame.
#[derive(Clone, Copy, Debug)]
pub enum BoardInput<'a> {
    None,
    /// A click on the zone of the current view with that name.
    Zone(&'a str),
    /// A click on the sprite of slot k of the view.
    Sprite(usize),
}

impl Game {
    /// The place `wam` loaded in world `world` (its banks, the world's
    /// dialogs) without a chapter; `rand` seeded with 1.
    #[doc(hidden)]
    pub fn board_begin(&mut self, wam: &str, world: i32) -> bool {
        self.set_world(world);
        self.load_dialogs(world);
        if let Some(s) = self.script.as_mut() {
            // An empty chapter buffer of the harnesses' size (64000: the
            // size bounds `SetVariables`' search for the command's `(`).
            s.set_chapter_text(&vec![0u8; 64000]);
            s.resume_pending = false;
        }
        let ok = self.load_place(wam);
        self.mode = Mode::Warp;
        self.rand = Rand::new(1);
        ok
    }

    /// `VUE` := `view` (the old one in `LASTVUE`) and the view entered at
    /// `now`, yaw and pitch 0.
    #[doc(hidden)]
    pub fn board_enter(&mut self, view: u32, now: f64) {
        self.board_enter_at(view, 0.0, 0.0, now);
    }

    /// [`Game::board_enter`] at the arrival angles `yaw`, `pitch`
    /// (radians, as the `/set` handler leaves them @ 0x00756074 /
    /// 0x00756078).
    #[doc(hidden)]
    pub fn board_enter_at(&mut self, view: u32, yaw: f32, pitch: f32, now: f64) {
        self.now = now;
        if let Some(s) = self.script.as_mut() {
            let old = s.vars[s.keys.vue];
            let _ = s.set_var("LASTVUE", old);
            s.vars[s.keys.vue] = view as i32;
        }
        let before = self.rand.state;
        self.enter_view(view, yaw, pitch);
        self.tr_rand_since(before);
        if let Some(mut s) = self.script.take() {
            self.view_hook(&mut s);
            self.script = Some(s);
        }
        self.mode = Mode::Warp;
    }

    /// One harness warp frame at `now` (see the module's doc); false when
    /// the input names no zone or slot of the view.
    #[doc(hidden)]
    pub fn board_frame(&mut self, now: f64, input: BoardInput<'_>) -> bool {
        self.now = now;
        self.anim_tick = false;
        let before = self.rand.state;
        self.slots.step(now, &mut self.rand);
        self.tr_rand_since(before);
        let mut found = true;
        let clicked = !matches!(input, BoardInput::None);
        if clicked {
            self.click = true;
            if let Some(s) = self.script.as_mut() {
                s.click = true;
            }
        }
        if clicked {
            self.tr(|| "input".into());
        }
        match input {
            BoardInput::None => {}
            BoardInput::Sprite(k) => match self.slots.slots.get(k) {
                Some(s) => {
                    self.sprite_hit = Some((s.id, s.click));
                    self.sprite_clicks();
                }
                None => found = false,
            },
            BoardInput::Zone(name) => {
                self.sprite_hit = None;
                let zone = self
                    .place
                    .view
                    .and_then(|v| self.place.wam.view(v))
                    .and_then(|v| v.zones.iter().position(|z| z.name.as_deref() == Some(name)));
                match zone {
                    Some(i) => {
                        self.centre = Some(i);
                        self.zone_click();
                    }
                    None => found = false,
                }
            }
        }
        self.frame_hook();
        if clicked {
            self.click_scan();
        }
        self.click = false;
        self.right_clicked = false;
        self.sprite_hit = None;
        if let Some(s) = self.script.as_mut() {
            s.click = false;
            let k = s.keys.cliczone;
            s.vars[k] = 0;
        }
        // The flip's texts (@ 0x0040de60), not drawn: the voice slots whose
        // voice stopped freed (`SubTitleOn`, no subtitle line).
        self.flip_plan();
        found
    }

    /// The place's `CHANGEWAM` set-up (@ 0x0044f0c0: its show / hide
    /// commands under the story flags), unlogged, as the place harnesses
    /// run it after their set-up writes and before the view entry.
    #[doc(hidden)]
    pub fn board_wam_setup(&mut self) {
        let wam = self.place.name.clone();
        let flags = self
            .script
            .as_ref()
            .map(|s| s.flags.clone())
            .unwrap_or_default();
        let t = self.trace.take();
        for c in crate::wam_setup::commands(&wam, &flags) {
            self.run_command(&c);
        }
        self.trace = t;
    }

    /// [`Game::board_wam_setup`] with its commands logged (the palace's
    /// harness logs them: `cmd`, then the show / hide).
    #[doc(hidden)]
    pub fn board_wam_setup_traced(&mut self) {
        let wam = self.place.name.clone();
        let flags = self
            .script
            .as_ref()
            .map(|s| s.flags.clone())
            .unwrap_or_default();
        for c in crate::wam_setup::commands(&wam, &flags) {
            self.tr(|| format!("cmd {c}"));
            self.run_command(&c);
        }
    }

    /// One place-harness frame at `now` (the M4 place harnesses of worlds
    /// 1 and 2): the place animations' 15 Hz step (@ 0x00439c90), the
    /// frame hook (@ 0x004c27d0), the main loop's per-place tail
    /// (@ 0x004c1950). Their clicks go to [`Game::board_zone_hook`]
    /// between frames.
    #[doc(hidden)]
    pub fn board_place_frame(&mut self, now: f64) {
        self.now = now;
        self.anim_tick = false;
        self.slots.step(now, &mut self.rand);
        self.frame_hook();
        self.main_loop_hook();
    }

    /// One place-harness frame of the palace's harness at `now`: the place
    /// animations' step, then for an object click the hit dispatch
    /// (@ 0x004403e0 with the hit record = the first slot of the object
    /// `object` whose click kind is 3: `CLICOBJET` := the object), the
    /// frame hook (@ 0x004c27d0), on a click frame (`scan`: an object or a
    /// zone click) the click scan (@ 0x004405b0), the main loop's place
    /// tail (@ 0x004c1950). False when the view has no such slot.
    #[doc(hidden)]
    pub fn board_place_click(&mut self, now: f64, object: Option<&str>, scan: bool) -> bool {
        self.now = now;
        self.anim_tick = false;
        self.slots.step(now, &mut self.rand);
        let mut found = true;
        if scan {
            self.click = true;
            if let Some(s) = self.script.as_mut() {
                s.click = true;
            }
        }
        if let Some(name) = object {
            let id = self
                .names
                .id(atlantis2_data::defnames::OBJETS, name.as_bytes())
                .map_or(-1, |i| i as i32);
            match self.slots.slots.iter().find(|s| s.id == id && s.click == 3) {
                Some(s) => {
                    self.sprite_hit = Some((s.id, s.click));
                    self.sprite_clicks();
                }
                None => found = false,
            }
        }
        self.frame_hook();
        if scan {
            self.click_scan();
        }
        self.click = false;
        self.sprite_hit = None;
        if let Some(s) = self.script.as_mut() {
            s.click = false;
        }
        self.main_loop_hook();
        found
    }

    /// The place animations' 15 Hz step (@ 0x00439c90) at `now`, alone:
    /// the Paleo / corridor harness gives its inputs between it and the
    /// frame hook ([`Game::board_hook`]).
    #[doc(hidden)]
    pub fn board_anim_step(&mut self, now: f64) {
        self.now = now;
        self.anim_tick = false;
        self.slots.step(now, &mut self.rand);
    }

    /// The frame hook (@ 0x004c27d0) alone.
    #[doc(hidden)]
    pub fn board_hook(&mut self) {
        self.frame_hook();
    }

    /// The main loop's per-place tail (@ 0x004c1950) alone.
    #[doc(hidden)]
    pub fn board_main_tail(&mut self) {
        self.main_loop_hook();
    }

    /// The camera as the warp's initialisation leaves it, then turned to
    /// the arrival angles (radians) as the view entry turns it: the aim
    /// the player turned to.
    #[doc(hidden)]
    pub fn board_camera(&mut self, yaw: f32, pitch: f32) {
        self.reset_camera();
        self.camera.enter(yaw, pitch);
        self.centre = self.pick_centre();
    }

    /// The screen centre's angles (@ 0x004408e0), radians.
    #[doc(hidden)]
    pub fn board_aim(&self) -> (f32, f32) {
        self.camera.angles()
    }

    /// The right button's byte (@ 0x007592b9) set, as the mouse handler
    /// sets it.
    #[doc(hidden)]
    pub fn board_right_button(&mut self) {
        self.right_clicked = true;
    }

    /// `SetObjetEnMain` (@ 0x00428380) of the object `name`, or the hand
    /// cleared (@ 0x004282e0) for `None`, unlogged.
    #[doc(hidden)]
    pub fn board_hand(&mut self, name: Option<&str>) {
        let t = self.trace.take();
        self.run_command(&format!("(objetenmain={})", name.unwrap_or("0")));
        self.trace = t;
    }

    /// One frame of the running full-screen mode at `now` with the
    /// pointer at `ptr` and the click bytes; false when the mode's frame
    /// returned 0 (its stop then ran).
    #[doc(hidden)]
    pub fn board_screen_frame(
        &mut self,
        now: f64,
        ptr: (i32, i32),
        left: bool,
        right: bool,
    ) -> bool {
        self.now = now;
        self.pointer = ptr;
        let Some(screen) = self.puzzles.screen else {
            return false;
        };
        self.screen_step(left, right);
        self.puzzles.screen == Some(screen)
    }

    /// The game boy (`GBOY`, mode 12) started at `now` as the race step
    /// starts it, without its music commands: the music volume saved
    /// (0x007899d8) and set to 99 (@ 0x00434100), then the start
    /// (@ 0x0041f660).
    #[doc(hidden)]
    pub fn board_gboy_start(&mut self, now: f64) {
        self.now = now;
        self.puzzles.tourmagi.saved_volume = self.music_level();
        self.music_volume_set(99);
        let Some(mut s) = self.script.take() else {
            return;
        };
        {
            let mut h = hooks::Hx { g: self, s: &mut s };
            h.start_screen(hooks::Screen::GBoy);
        }
        self.script = Some(s);
    }

    /// The full-screen mode of loop mode number `mode` (9..15, 18) started
    /// at `now` by its start (the common part and the screen's own, as the
    /// hooks start it); false for another number.
    #[doc(hidden)]
    pub fn board_screen_start(&mut self, mode: i32, now: f64) -> bool {
        use hooks::Screen::*;
        let Some(screen) = [
            Carte,
            CarteTour,
            Licorne,
            GBoy,
            Scarabee,
            Ecrin,
            CraneBagdad,
            Telecommande,
        ]
        .into_iter()
        .find(|s| s.mode_number() == mode) else {
            return false;
        };
        self.now = now;
        let Some(mut s) = self.script.take() else {
            return false;
        };
        {
            let mut h = hooks::Hx { g: self, s: &mut s };
            h.start_screen(screen);
        }
        self.script = Some(s);
        true
    }

    /// The cube of the view entered (`warp/Warp512/<world>/<name>`) and its
    /// six faces (512 x 512 RGB565 on a 16-bit display, face k = the
    /// loader's surface 0x0077af18 + 0x80 k).
    #[doc(hidden)]
    pub fn board_cube(&self) -> (&str, &[Vec<u16>; 6]) {
        static NONE: [Vec<u16>; 6] = [const { Vec::new() }; 6];
        (&self.place.cube, self.place.faces.as_565().unwrap_or(&NONE))
    }

    /// The zone click hook (@ 0x004c32c0) on the zone `name` of the view
    /// at `now`, alone (the jewel case's harness starts the case so).
    #[doc(hidden)]
    pub fn board_zone_hook(&mut self, name: &str, now: f64) -> bool {
        self.now = now;
        let Some(z) = self
            .place
            .view
            .and_then(|v| self.place.wam.view(v))
            .and_then(|v| v.zones.iter().find(|z| z.name.as_deref() == Some(name)))
            .cloned()
        else {
            return false;
        };
        self.zone_hook(z.kind, z.value, name);
        true
    }

    /// The jewel case's hit test of gem k (0..2) at `ptr`
    /// (`SPR_COLLISION` @ 0x00437080, the loop's call).
    #[doc(hidden)]
    pub fn board_ecrin_hit(&self, k: usize, ptr: (i32, i32)) -> bool {
        self.puzzles
            .screens
            .ecrin_bank
            .as_ref()
            .is_some_and(|b| hooks::ecrin_gem_hit(b, k, self.width as i32, self.height as i32, ptr))
    }

    /// The remote's hit test of button k (0..3: sprites 0, 0x0e, 0x1d,
    /// 0x2c) at `ptr` (`SPR_COLLISION` @ 0x00437080, the loop's call).
    #[doc(hidden)]
    pub fn board_tele_hit(&self, k: usize, ptr: (i32, i32)) -> bool {
        self.puzzles
            .screens
            .tele_bank
            .as_ref()
            .is_some_and(|b| hooks::tele_hit(b, k, self.width as i32, self.height as i32, ptr))
    }

    /// The loop mode's number (@ 0x00789af0): 3 the warp, the screens'.
    #[doc(hidden)]
    pub fn board_mode_number(&self) -> i32 {
        match (self.mode, self.puzzles.screen) {
            (Mode::Screen, Some(s)) => s.mode_number(),
            (Mode::Warp, _) => 3,
            _ => -1,
        }
    }

    /// The mode a running screen goes back to (@ 0x00755934 for the
    /// jewel case), as a loop mode number.
    #[doc(hidden)]
    pub fn board_screen_back(&self) -> i32 {
        match self.puzzles.screen_back {
            Some(Mode::Warp) => 3,
            Some(_) => -1,
            None => 0,
        }
    }

    /// The golden `st` line of a board's C globals: `guard` (the
    /// merchant's house), `jardin` (the garden's banana bunch and
    /// monkey), `ecrin` (the jewel case).
    #[doc(hidden)]
    pub fn board_st(&self, board: &str) -> Option<String> {
        let p = &self.puzzles;
        match board {
            "guard" => {
                let m = &p.mmarchd;
                Some(hooks::mmarchd_st(m, p.freeze, p.cam_lock))
            }
            "jardin" => {
                let j = &p.jardin;
                Some(format!(
                    "banana_done={} banana_playing={} banana_reload={} monkey_reload={} \
                     monkey_at={}",
                    i32::from(j.banana_done),
                    i32::from(j.banana_playing),
                    i32::from(j.banana_reload),
                    i32::from(j.monkey_reload),
                    j.monkey_at
                ))
            }
            "ecrin" => {
                let s = &p.screens;
                let hex: String = s
                    .ecrin_fade
                    .to_bits()
                    .to_le_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let taken: String = s
                    .ecrin_taken
                    .iter()
                    .map(|&t| if t { '1' } else { '0' })
                    .collect();
                Some(format!(
                    "fade={hex} taken={taken} count={} cursor={} mode={}",
                    s.ecrin_count,
                    s.ecrin_cursor,
                    self.board_mode_number()
                ))
            }
            "licorne" => p
                .screens
                .licorne
                .as_ref()
                .map(|b| format!("held={} n={} run={}", b.held, b.placed, u8::from(b.running))),
            "toupie" => Some(format!(
                "state={} frz={} active={}",
                p.toupie_state,
                u8::from(p.freeze),
                u8::from(p.toupie_armed)
            )),
            "cfuner" => {
                let ring: Vec<String> = p.cfuner_ring.iter().map(|v| v.to_string()).collect();
                Some(format!(
                    "pos={} ring={} first={} id={}",
                    p.cfuner_star,
                    ring.join(","),
                    u8::from(p.cfuner_fresh),
                    p.cfuner_id
                ))
            }
            "isis" | "fisherman" | "feathers" | "ankh" => Some(hooks::ldmorts_st(self, board)),
            "wolves" | "tiger" | "hunt" | "altar" => Some(hooks::paleo_st(self, board)),
            "couloir" => Some(hooks::couloir_st(self)),
            "star" | "tt" | "tr" => Some(hooks::pmagi_st(self, board)),
            "tower" => Some(hooks::tourmagi_st(self)),
            "gboy" => {
                let g = &p.screens.gboy;
                Some(format!(
                    "cur={} pick={} anim={} res={} blink={}/{}",
                    g.cursor,
                    g.pick,
                    g.anim,
                    p.tourmagi.dice,
                    u8::from(g.phase),
                    g.blinks
                ))
            }
            "tele" => {
                // The C globals' order: 0x0078972c.. (Paleo, Bagdad,
                // power, home), 0x00789734.. the same.
                let s = &p.screens;
                let order = [2, 1, 0, 3];
                let hover: String = order
                    .iter()
                    .map(|&k| if s.tele_hover[k] { '1' } else { '0' })
                    .collect();
                let fr: Vec<String> = order
                    .iter()
                    .map(|&k| s.tele_frames[k].to_string())
                    .collect();
                let flags = &self.script.as_ref()?.flags;
                let f: Vec<String> = (400..405).map(|n| flags[n].to_string()).collect();
                Some(format!(
                    "hover={hover} fr={} f400..404={} f425={} f427={} mode={}",
                    fr.join(","),
                    f.join(","),
                    flags[425],
                    flags[427],
                    self.board_mode_number()
                ))
            }
            "gboy_full" => {
                let g = &p.screens.gboy;
                let faces: Vec<String> = g.faces.iter().map(|f| f.to_string()).collect();
                Some(format!(
                    "cur={} pick={} k={} anim={} res={} hit={} blink={}/{} faces={} go={}",
                    g.cursor,
                    g.pick,
                    g.k,
                    g.anim,
                    p.tourmagi.dice,
                    u8::from(g.hit),
                    u8::from(g.phase),
                    g.blinks,
                    faces.join(","),
                    u8::from(g.go)
                ))
            }
            _ => None,
        }
    }
}
