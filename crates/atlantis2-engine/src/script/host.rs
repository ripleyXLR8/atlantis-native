//! What the interpreter asks of the rest of the game.
//!
//! The `/set` handler (`SetVariables` @ 0x004818f0) keeps the script tables
//! itself (variables, flags, subjects, the bag, the show/hide table, the
//! hand) and calls out for everything else. Each method of [`Host`] is one
//! callee of the original, named after it, with its address; its default
//! does what the game shows when that subsystem is not there yet (nothing,
//! or "not started"), so a host implements only what it has.
//!
//! Subsystems not ported yet, and the methods they will fill in:
//!
//! - **navigation** (places, views, moves, the place's animations):
//!   [`Host::load_wam`], [`Host::has_view`], [`Host::enter_view`],
//!   [`Host::turn_now`], [`Host::zoom_start`], [`Host::zoom_psycho_start`],
//!   [`Host::enable_zone`], [`Host::abort_departure`], [`Host::anim`],
//!   [`Host::anim_frame`], [`Host::special_event`];
//! - **videos**: [`Host::play_video`], [`Host::cinema_sound`];
//! - **dialogs**: [`Host::dialog_running`], [`Host::dial_start`],
//!   [`Host::dial_stop`];
//! - **sound and music**: [`Host::music_step`], [`Host::music_volume`],
//!   [`Host::new_sound`], [`Host::traj_sound`], [`Host::load_music`],
//!   [`Host::delete_music`], [`Host::start_music`], [`Host::stop_music`];
//! - **worlds and discs**: [`Host::set_monde`], [`Host::change_cd`];
//! - **menus and screens**: [`Host::bag_layout`], [`Host::message_clic`],
//!   [`Host::view_big_sprite`], [`Host::credits`].
//!
//! The ends of the loops a command starts (a video, a dialog, a disc
//! request) are the host's to report: [`super::Script::video_ended`],
//! [`super::Script::dialog_ended`], [`super::Script::disc_inserted`].

/// What the animation keys ask of the place's animation records
/// (@ 0x0067bfb0, 200 bytes each: the SPW slots of the view). `kind` is 1
/// for a character (`#PERSONNAGES#`), 2 for an object (`#OBJETS#`), 0 when
/// the key looked in both sections (`CONTINUEANIM`, `ANIM_THE_FRAME`: first
/// `#OBJETS#`, then `#PERSONNAGES#`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimOp {
    /// `hideperso` / `hideobjet` without `,x` (FUN_0048c6b0): the records
    /// of `id` and `kind` get visible = 0 and frame = 0.
    Hide { kind: i32, id: i32 },
    /// `showperso` / `showobjet` (FUN_0048c610): the records of `id` and
    /// `kind` get visible = 1; with `number` ≠ 0, only the record playing
    /// animation `number`.
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
    /// `ANIM_THE_FRAME` (0 or 1).
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
    /// fatal in the original). No view is shown until a `vue=`.
    fn load_wam(&mut self, name: &str) {}

    /// `AtlWam_FindView` (@ 0x004121a0): whether the current place has
    /// that view.
    fn has_view(&mut self, view: i32) -> bool {
        true
    }

    /// `AtlWarp_EnterVue` (@ 0x00459fe0): show a view of the current
    /// place, the camera turned to the start angles (radians, the floats
    /// @ 0x005e2f30 / 0x005e2f34 that `ANGLEWARPX` / `ANGLEWARPY` set).
    fn enter_view(&mut self, view: i32, yaw: f32, pitch: f32) {}

    /// `anglewarpxnow` / `anglewarpynow` (@ 0x0045ad40 / 0x0045ada0):
    /// turn the current view at once; `yaw` true for the yaw, the angle in
    /// radians.
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

    /// The current frame of the first animation record of `id` (any
    /// kind), for the `NUMFRAMEOBJET` / `NUMFRAMEPERSO` conditions
    /// (`CondValue` @ 0x00481300); `None` when it has none.
    fn anim_frame(&mut self, id: i32) -> Option<i32> {
        None
    }

    /// `TESTSPECIALEVENT` ≠ 0 in `Interp_GereFrame`: the chapter events of
    /// the other worlds (@ 0x00484880). Not reached in Tibet.
    fn special_event(&mut self, event: i32) {}

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

    /// Whether a dialog is running (@ 0x006cfb54).
    fn dialog_running(&self) -> bool {
        false
    }

    /// `AtlDial_stop` (@ 0x004808a0) of a running dialog before another.
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
    /// is the value up to the end of the line (`1,2,3)`).
    fn load_music(&mut self, list: &str) {}

    /// `delmusik` (@ 0x00479d70): delete those copies.
    fn delete_music(&mut self, list: &str) {}

    /// `startmusik` (@ 0x00485140 → `Interp_SelectMusic` @ 0x004851f0,
    /// @ 0x0047a200): choose and start a music from the list (the value
    /// up to the end of the line, also kept in [`super::Script::music`]).
    fn start_music(&mut self, list: &str) {}

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

    /// The bag bar after an object was put in (@ 0x00475870, then
    /// @ 0x004757b0 until it returns 0: the bar scrolls to its end).
    fn bag_layout(&mut self) {}

    /// `MESSAGE_CLIC` (@ 0x00476790): the message viewer (loop type 10).
    fn message_clic(&mut self, text: &str) {}

    /// `ViewBigSpr` (@ 0x0048c9d0): the picture viewer (loop type 11).
    fn view_big_sprite(&mut self, bank: &str) {}

    /// `CREDITS` (@ 0x00486e00); the section then waits.
    fn credits(&mut self) {}
}
