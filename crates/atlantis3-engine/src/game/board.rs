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
        self.now = now;
        if let Some(s) = self.script.as_mut() {
            let old = s.vars[s.keys.vue];
            let _ = s.set_var("LASTVUE", old);
            s.vars[s.keys.vue] = view as i32;
        }
        let before = self.rand.state;
        self.enter_view(view, 0.0, 0.0);
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
        self.voices_release();
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
        self.screen_step(left, right, None);
        self.puzzles.screen == Some(screen)
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
                Some(hooks::mmarchd_st(m, p.freeze))
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
            _ => None,
        }
    }
}
