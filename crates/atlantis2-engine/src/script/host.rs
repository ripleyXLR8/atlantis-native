//! What the interpreter asks of the rest of the game.
//!
//! The `/set` handler (`SetVariables` @ 0x004818f0) keeps the script tables
//! itself (variables, flags, subjects, the bag, the show/hide table, the
//! hand) and calls out for everything else. Each method of [`Host`] is one
//! callee of the original, named after it, with its address; its default
//! does what the game shows when that subsystem is not there (nothing, or
//! "not started"), so a host implements only what it has: the engine
//! (`crate::engine`, `engine/host.rs`) implements them for the game, the
//! tests' hosts what they check.
//!
//! The subsystems and their methods:
//!
//! - **navigation** (places, views, moves, the place's animations):
//!   [`Host::load_wam`], [`Host::has_view`], [`Host::enter_view`],
//!   [`Host::turn_now`], [`Host::zoom_start`], [`Host::zoom_psycho_start`],
//!   [`Host::enable_zone`], [`Host::abort_departure`], [`Host::anim`],
//!   [`Host::anim_frame`], and for the special events
//!   ([`super::Script::special_event`]) [`Host::view_id`],
//!   [`Host::slot_frame`], [`Host::slot_rewind`], [`Host::voice_busy`];
//! - **videos**: [`Host::play_video`], [`Host::cinema_sound`];
//! - **dialogs**: [`Host::dialog_running`], [`Host::dial_start`],
//!   [`Host::dial_stop`];
//! - **sound and music**: [`Host::music_step`], [`Host::music_volume`],
//!   [`Host::new_sound`], [`Host::traj_sound`], [`Host::load_music`],
//!   [`Host::delete_music`], [`Host::start_music`], [`Host::stop_music`];
//! - **the bag bar**: [`Host::close_bar`];
//! - **worlds and discs**: [`Host::set_monde`], [`Host::change_cd`];
//! - **menus and screens**: [`Host::bag_layout`], [`Host::message_clic`],
//!   [`Host::view_big_sprite`], [`Host::credits`].
//!
//! The ends of the loops a command starts (a video, a dialog, a disc
//! request, a music copy's page) are the host's to report:
//! [`super::Script::video_ended`], [`super::Script::dialog_ended`],
//! [`super::Script::disc_inserted`], [`super::Script::copy_ended`].

use super::Script;

/// What the animation keys ask of the place's animation records
/// (@ 0x0067bfb0, 200 bytes each: the SPW slots of the view). `kind` is 1
/// for a character (`#PERSONNAGES#`), 2 for an object (`#OBJETS#`), 0 when
/// the key looked in both sections (`CONTINUEANIM`, `ANIM_THE_FRAME`: first
/// `#OBJETS#`, then `#PERSONNAGES#`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimOp {
    /// `hideperso` / `hideobjet` without `,x` (FUN_0048c6b0): the first
    /// record of `id` and `kind` gets visible = 0 and frame = 0.
    Hide { kind: i32, id: i32 },
    /// `showperso` / `showobjet` (FUN_0048c610): the first record of `id`
    /// and `kind` gets visible = 1; with `number` ≠ 0, the first one
    /// playing animation `number`.
    Show { kind: i32, id: i32, number: i32 },
    /// `PLAYANIMP` / `PLAYANIMO` (branches of `SetVariables`
    /// @ 0x004818f0): switch `id` to its record playing animation `number`.
    Play { kind: i32, id: i32, number: i32 },
    /// `CONTINUEANIM`.
    Continue { kind: i32, id: i32 },
    /// `STOPANIM`: stop the record of `id` playing `number`.
    Stop { id: i32, number: i32 },
    /// `GOTOFRAMEPERSO` / `GOTOFRAMEOBJET`.
    GotoFrame { kind: i32, id: i32, frame: i32 },
    /// `ANIM_THE_FRAME` (0 or 1): the playing byte (+0xbc) of the first
    /// record of `id`, any kind.
    TheFrame { kind: i32, id: i32, on: bool },
    /// `ADDFRAMEPERSO` / `ADDFRAMEOBJET`.
    AddFrame { kind: i32, id: i32, frames: i32 },
    /// `ChangeAnimGotoPerso` (FUN_0047df00, called @ 0x00482cbf): the character and the two
    /// numbers after its comma (read by @ 0x00485180).
    ChangeAnimGotoPerso { id: i32, a: i32, b: i32 },
}

/// What [`Host::dial_start`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialStart {
    /// The dialog loop runs; the host calls
    /// [`super::Script::dialog_ended`] when it ends.
    Started,
    /// The dialog could not start (FUN_0047ff60 returns 0 after restoring
    /// the loop type).
    Failed,
}

/// The rest of the game, as the interpreter sees it.
#[allow(unused_variables)]
pub trait Host {
    /// `LoadCurrentChap` (@ 0x00483af0): the bytes of
    /// `scenar\chapi%03d.con`, `None` when the file is missing.
    fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>>;

    /// `AtlWam_Load` (@ 0x0040fee0): load a place file (a failure is
    /// fatal in the original). No view is shown until a `vue=`. False
    /// when the host refuses the place (a world the port does not have
    /// yet): the section that asked for it ends there
    /// ([`super::Script::travel_refused`]).
    fn load_wam(&mut self, name: &str) -> bool {
        true
    }

    /// `AtlWam_FindView` (@ 0x004121a0): whether the current place has
    /// that view.
    fn has_view(&mut self, view: i32) -> bool {
        true
    }

    /// `AtlWarp_EnterVue` (@ 0x00459fe0): show a view of the current
    /// place, the camera turned to the start angles (radians, the floats
    /// @ 0x005e2f30 / 0x005e2f34 that `ANGLEWARPX` / `ANGLEWARPY` set).
    fn enter_view(&mut self, view: i32, yaw: f32, pitch: f32) {}

    /// The same, from the `VUE` branch, with the script variables at hand:
    /// the puzzles' set-up of the view (FUN_0049bd50, inside the entry)
    /// reads and writes them (`pos_statue1..6` for the figurines). By
    /// default [`Host::enter_view`].
    fn enter_view_with_vars(&mut self, view: i32, yaw: f32, pitch: f32, vars: &mut [i32]) {
        let _ = vars;
        self.enter_view(view, yaw, pitch);
    }

    /// The same with the interpreter itself at hand, for the hooks that
    /// run commands (`RunOneCommand`, which the original's interpreter
    /// runs re-entrantly in the middle of the `VUE` line, as the spider
    /// web's exit does). By default [`Host::enter_view_with_vars`] with
    /// its variables.
    fn enter_view_with_script(&mut self, view: i32, yaw: f32, pitch: f32, script: &mut Script) {
        self.enter_view_with_vars(view, yaw, pitch, &mut script.vars);
    }

    /// `anglewarpxnow` / `anglewarpynow` (@ 0x0045ad40 / 0x0045ada0):
    /// turn the current view at once, the angle in radians: `yaw` true,
    /// the camera's rotation set about the root's Y axis (the pitch
    /// dropped); false, a pitch from the current orientation.
    fn turn_now(&mut self, yaw: bool, angle: f32) {}

    /// `ZoomStart` (@ 0x0045b350): the zoom and the field of view in
    /// degrees (35).
    fn zoom_start(&mut self, zoom: f32, fov: f32) {}

    /// `ZoomPsychoStart` (@ 0x0045b3b0).
    fn zoom_psycho_start(&mut self, a: f32, b: f32) {}

    /// `AtlWam_EnableZone` (@ 0x00410060): enable or disable click zone
    /// `zone` of view `view`.
    fn enable_zone(&mut self, view: i32, zone: i32, on: bool) {}

    /// `ABORTDEPART` seen by `Interp_GereFrame` (@ 0x00484cf5): forget the
    /// hit animation record (@ 0x00661640 = −1) and the pending move
    /// (@ 0x006d85e8 = 0).
    fn abort_departure(&mut self) {}

    /// The place's animation records.
    fn anim(&mut self, op: AnimOp) {}

    /// The show / hide table's row of (`id`, `kind`) was set
    /// (FUN_0048c730, the script's [`super::Script::shown`]): the view
    /// entries read it to choose the records shown (FUN_0048c7b0).
    fn show_table(&mut self, id: i32, shown: bool, kind: i32) {}

    /// The current frame of the first animation record of `id` (any
    /// kind), for the `NUMFRAMEOBJET` / `NUMFRAMEPERSO` conditions
    /// (`CondValue` @ 0x00481300); `None` when it has none.
    fn anim_frame(&mut self, id: i32) -> Option<i32> {
        None
    }

    /// The current view's number (the view record @ 0x006d85d4, +0x10).
    fn view_id(&mut self) -> i32 {
        0
    }

    /// The current frame (+0x10) of the first animation record of `id`
    /// (FUN_0047de60, any kind), or with `anim` the first of `id` whose
    /// WAM state plays that animation (FUN_0047dea0); `None` when there
    /// is none (fatal in the original).
    fn slot_frame(&mut self, id: i32, anim: Option<i32>) -> Option<i32> {
        None
    }

    /// The end of a special event on the first animation record of `id`
    /// (FUN_00484880 @ 0x00484a83..0x00484a96): its frame 0, its end its
    /// WAM state's +0x24.
    fn slot_rewind(&mut self, id: i32) {}

    /// Whether `voice` plays (FUN_0047a9c0).
    fn voice_busy(&mut self, voice: i32) -> bool {
        false
    }

    /// The music state machine (@ 0x00476fc0), which the `cinema` key runs
    /// twice around the opening of the film's sound.
    fn music_step(&mut self) {}

    /// The cinematic's sound track `cineNNN.apc` (@ 0x0047a320).
    fn cinema_sound(&mut self, apc: &str) {}

    /// `AtlHnm_start` (@ 0x0040bf60): play a cinematic at 15 frames a
    /// second in the video loop; with `cinema=N,M` (@ 0x0040c030), dialog M
    /// is played over the film (FUN_004801a0, and the video loop runs the
    /// dialog step after each frame). True when it started: the section
    /// then waits for [`super::Script::video_ended`]. False skips the film
    /// and the section goes on.
    fn play_video(&mut self, hnm: &str, dialog: Option<i32>) -> bool {
        false
    }

    /// The bag bar closed (its flag @ 0x00671ef0 = 0, nothing else: the
    /// hand and the cursor are left as they are), as the `cinema` key
    /// does after the film's start (@ 0x00481d4e).
    fn close_bar(&mut self) {}

    /// Whether a dialog is running (@ 0x006cfb54).
    fn dialog_running(&self) -> bool {
        false
    }

    /// `AtlDial_stop` (@ 0x004808a0) of a running dialog before another:
    /// the host's part (the script's, [`super::Script::dialog_ended`],
    /// follows).
    fn dial_stop(&mut self) {}

    /// `AtlDial_start` (@ 0x0047ff60): dialog `n` of `loc\Dial*.txt`.
    ///
    /// The section waits after a `dial` line in both cases. Until dialogs
    /// are ported, a host returns `Started` and calls
    /// [`super::Script::dialog_ended`] on the next frame, so that the
    /// section goes on as after the dialog. The default does that through
    /// `Failed`, which leaves the section waiting as the original does.
    fn dial_start(&mut self, n: i32) -> DialStart {
        DialStart::Failed
    }

    /// `AtlMuzik_Volume` (@ 0x004770c0): music to `percent` % (20 under a
    /// cinematic, 16 under a dialog).
    fn music_volume(&mut self, percent: i32) {}

    /// `NEWSOUND` (@ 0x0047a290): play `soundNNN.apc`.
    fn new_sound(&mut self, apc: &str) {}

    /// `TrajSound` (@ 0x0047a390): the travel sound `NAME.apc`.
    fn traj_sound(&mut self, apc: &str) {}

    /// `LOADMUSIK` (@ 0x00479480): copy music files from the CD; the text
    /// is the value up to the end of the line (`1,2,3)`). True when the
    /// copy shows the please-wait page: the original copies before the
    /// next line runs, so the section waits for the page's end
    /// ([`super::Script::copy_ended`]).
    fn load_music(&mut self, list: &str) -> bool {
        false
    }

    /// `delmusik` (@ 0x00479d70): delete those copies.
    fn delete_music(&mut self, list: &str) {}

    /// `startmusik` (@ 0x00485140 → `Interp_SelectMusic` @ 0x004851f0,
    /// @ 0x0047a200): choose and start a music from the list (the value
    /// up to the end of the line, also kept in [`super::Script::music`]).
    /// True when the music's copy shows the please-wait page, as
    /// [`Host::load_music`].
    fn start_music(&mut self, list: &str) -> bool {
        false
    }

    /// `stopmusik` (@ 0x00477090): fade the music out.
    fn stop_music(&mut self) {}

    /// The world's part of `Interp_SetMonde` (@ 0x00484df0): its object
    /// and subject sprite banks, its dialog texts, and its disc (1 → CD 1,
    /// 2 → CD 2, 3 → CD 3, 4 → CD 4, 6 → CD 4; 5, Tibet, keeps the disc in
    /// the drive). Returns whether the disc is there; a host that reads
    /// every disc returns true. Not called for worlds outside 1..6.
    fn set_monde(&mut self, monde: i32) -> bool {
        true
    }

    /// `CHANGECD` (@ 0x0040dff0 with `disc` = the value − 1): whether that
    /// disc is there.
    fn change_cd(&mut self, disc: i32) -> bool {
        true
    }

    /// The bag bar after an object was put in: `visible` is the count of
    /// the current world's entries (@ 0x00475870, kept @ 0x00671ee8), then
    /// @ 0x004757b0 until it returns 0: the bar scrolls to its end.
    fn bag_layout(&mut self, visible: i32) {}

    /// A fatal error the host met while it ran the line's command
    /// (FUN_00480cd0 called from inside it), for the script to stop on.
    fn take_fatal(&mut self) -> Option<String> {
        None
    }

    /// `MESSAGE_CLIC` (@ 0x00476790): the message viewer (loop type 10).
    fn message_clic(&mut self, text: &str) {}

    /// `ViewBigSpr` (@ 0x0048c9d0): the picture viewer (loop type 11).
    fn view_big_sprite(&mut self, bank: &str) {}

    /// `CREDITS` (@ 0x00486e00); the section then waits.
    fn credits(&mut self) {}

    // Atlantis III's calls (its `Game.exe`'s addresses).

    /// `CINEMA=n,fps=f` (`AtlHnm_start("cine%03d.hnm", "cine%03d.txt",
    /// fps, 0)` @ 0x004481ed): whether the film started (then the section
    /// waits for its end, `super::Script::close_interrupt`).
    fn play_cinema(&mut self, n: i32, fps: i32) -> bool {
        let _ = fps;
        self.play_video(&format!("cine{n:03}.hnm"), None)
    }

    /// The show / hide commands `CHANGEWAM` runs for place `wam` given the
    /// story flags (@ 0x0044f0c0).
    fn wam_commands(&mut self, wam: &str, flags: &[i32]) -> Vec<String> {
        Vec::new()
    }

    /// `INITRECALAGE` (@ 0x004410b0): the camera starts turning toward
    /// (yaw, pitch), radians; false when it is already there.
    fn init_recalage(&mut self, yaw: f32, pitch: f32) -> bool {
        false
    }

    /// Whether a camera re-alignment runs (@ 0x00756080).
    fn recalage_running(&self) -> bool {
        false
    }

    /// `DISPLAY` (@ 0x00449262): one whole frame drawn and shown now.
    fn display(&mut self) {}

    /// `PLAYVOICE=a,b,c,d` (@ 0x0040da70).
    fn play_voice(&mut self, values: [i32; 4]) {}

    /// `SAVE=n` (@ 0x0044c6d0): the quick save `n` of the script's state.
    fn quick_save(&mut self, n: i32, script: &Script) {}

    /// `LOAD=n` (@ 0x0044c9b0): quick save `n` read back
    /// ([`Script::load_a3`]).
    fn quick_load(&mut self, n: i32, script: &mut Script) {}

    /// A loaded save's camera (`Quaternion0..3`, @ 0x00441040).
    fn set_camera_quaternion(&mut self, q: [f32; 4]) {}

    /// `DIAL=n[, voixoff]` (`AtlDial_start` @ 0x0040cfe0).
    fn dial_start_a3(&mut self, n: i32, voixoff: bool) -> DialStart {
        let _ = voixoff;
        self.dial_start(n)
    }

    /// `GAMEOVER` set (@ 0x0042ac50).
    fn game_over(&mut self) {}

    /// Whether the animation (kind, id, n) has ended (record +0x471), for
    /// `WAITFORFINANIMP` / `WAITFORFINANIMO`.
    fn anim_ended(&mut self, kind: i32, id: i32, n: i32) -> bool {
        true
    }
}
