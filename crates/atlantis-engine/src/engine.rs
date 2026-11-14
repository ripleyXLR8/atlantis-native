//! The game: its state, the new-game start and the place loop.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`engine.cpp`
//! `run` / `gameStep`, `place.cpp` `changeChapter` / `setupChapterWAM`;
//! GPLv3, the ScummVM team), which follows `atlantis.exe`'s main loop and
//! place loop `FUN_00423ad8`.
//!
//! The engine is one async task ([`run`]): where the original presents a
//! frame and polls its events, the code awaits [`Sys::frame`].

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use atlantis_data::saves::Players;
use atlantis_data::sprite::{read_spr_file, SprSprite};
use atlantis_data::sprite_list::{NpcClickRect, SpriteTransEntry};
use atlantis_data::wam::{CentrePos, Place, Transition, Wam};
use atlantis_data::{Game, SpritePlaceEntry};

use crate::ambient::{AmbientSounds, MixerAmbientOutput};
use crate::con_script::{ConClocks, Vars};
use crate::cursor::WarpCursorState;
use crate::dialog::Dialogs;
use crate::enhancements::{BugFix, Enhancements};
use crate::hooks::{Departure, DirectLoader, Loader, ScriptHooks};
use crate::look::{WarpMomentum, WarpPointer};
use crate::music::Music;
use crate::persist::{MemStore, SaveStore, Settings};
use crate::scripts::Scripts;
use crate::sys::{Input, MouseButton, Sys};
use crate::ui::menus::MenuOutcome;
use crate::ui::{BarSession, Guests, UiAssets};
use crate::warp::Warp;

/// The action [`Engine::handle_warp`] returns when it leaves without a
/// click (quit, a script that entered another place).
pub const WARP_ABORT: u32 = 66666;

/// The WAM of a new game and its start place: `FUN_0042de5c` without a
/// checkpoint (0x0042e26e..0x0042e29e) loads `wam\atlan1.wam` and sets the
/// requested vue 0x0049692c = 1; the view angles 0x00bcb59c / 0x00bcb598
/// keep their .bss value, 0.
pub const NEW_GAME_WAM: &str = "ATLAN1.WAM";

/// The size of the WAM path buffer 0x006cdce0, the checkpoint's first
/// field.
pub const WAM_PATH_LEN: usize = 0x104;
pub const NEW_GAME_VUE: u16 = 1;

/// What happened, for tests and tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A WAM was loaded.
    WamLoaded(String),
    /// A place was entered (its panorama shown).
    PlaceEntered {
        wam: String,
        place: u16,
    },
    /// A video was played: its name, the frames shown and the number of
    /// SPF sprites drawn over it.
    Video {
        name: String,
        frames: u32,
        sprites: usize,
    },
    /// A chapter's CON script was loaded and its /INIT block run.
    ChapterLoaded(u32),
    /// A CON section started (its chapter and number), and how: "sel",
    /// "con", "tim1", "tim2" or "go".
    Section {
        chapter: u32,
        id: i32,
        kind: &'static str,
    },
    /// A /set command ran (the command as written).
    Set(String),
    /// A dialog line was handed to the dialog presenter: section, track
    /// letter, speaker.
    Line {
        section: i32,
        letter: char,
        speaker: String,
    },
    /// A music track started (StartMusik).
    Music(i32),
    /// A NewSound sample started: its table entry and file.
    NewSound(i32, String),
    /// A game over (GameOver=N).
    GameOver(i32),
    /// A voice started (a dialog line's or a game-over narrator's): the
    /// archive member.
    Voice(String),
    /// The dialog presenter played a line: its section and letter, the
    /// talking-head video, the UBB frames shown, whether its voice played
    /// and the subtitle lines drawn.
    LinePlayed {
        section: i32,
        letter: char,
        video: Option<String>,
        frames: u32,
        voice: bool,
        subtitles: usize,
    },
    /// The subject menu of a character: the subjects shown and the one
    /// chosen (0 for an empty cell, `None` for a click outside the grid).
    SubjectMenu {
        perso: i32,
        subjects: Vec<u8>,
        chosen: Option<i32>,
    },
    /// A menu file was opened (`SPRLIST\<name>`).
    Menu(String),
    /// A checkpoint was written or loaded (its file).
    CheckpointWritten(String),
    CheckpointLoaded(String),
    /// A puzzle ended (StartPuzzle `id`), with its FinPuzzle.
    Puzzle {
        id: i32,
        result: i32,
    },
    /// The original's error box was shown (its text).
    ErrorBox(String),
    /// The chapter's CON text was read again (its file), for the Japanese
    /// DVD build's subtitle language.
    ConTextReloaded(String),
}

/// Why the place loop ends (`_abortCommand`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// A game over: the player's checkpoint is reloaded, a new game
    /// without one (0x0041975d -> `FUN_0042de5c`).
    NewGame,
    /// The load picker's OK, RESTART EPISODE or double click: the
    /// checkpoint of the player's chapter byte is reloaded.
    Load,
    /// QUIT GAME confirmed in the in-game menu (the credits have played).
    Quit,
    /// Exit was set: the end of the game for 86 (CHAPI085's, the only
    /// value shipped), the player screen again for any other value.
    Finished,
}

/// How [`run`] starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Start {
    /// As the original: the logos, PREINTRO, the player screen, the
    /// player's checkpoint (or a new game).
    #[default]
    Boot,
    /// A new game at once, with no player screen (tests, tools); the
    /// checkpoints are those of player 0.
    NewGame,
    /// A testing aid: StartPuzzle `id` at once, with object `object` in
    /// hand (0 for none); its FinPuzzle is reported (an
    /// [`Event::Puzzle`]), then a new game starts as with
    /// [`Start::NewGame`].
    Puzzle { id: i32, object: i32 },
}

/// What [`run`] needs besides the platform and the game.
pub struct Options {
    pub loader: Rc<dyn Loader>,
    pub hooks: Box<dyn ScriptHooks>,
    /// Receives the [`Event`]s when set.
    pub trace: Option<Rc<RefCell<Vec<Event>>>>,
    /// The optional differences from the original game.
    pub enhancements: Enhancements,
    /// The dialog subtitles are shown (the original's SUBTITLES setting).
    pub subtitles: bool,
    /// Where the players, the checkpoints and the settings are kept.
    pub saves: Rc<dyn SaveStore>,
    pub start: Start,
    /// `--original`: OMNI 3D MODE 1 and the volumes `7f 6e 6e 6e` at the
    /// start, as every run of the original (0x004960ac = 0, 0x00495e70..
    /// data bytes), whatever `settings.ini` keeps; a checkpoint load still
    /// sets the ones it holds.
    pub original_start_values: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            loader: Rc::new(DirectLoader),
            hooks: Box::new(Scripts::with_dialog(Box::new(Dialogs::new()))),
            trace: None,
            enhancements: Enhancements::default(),
            subtitles: true,
            saves: Rc::new(MemStore::new()),
            start: Start::default(),
            original_start_values: false,
        }
    }
}

/// The game state of navigation. Fields are public for the script hooks.
pub struct Engine {
    pub sys: Rc<Sys>,
    pub game: Game,
    pub(crate) loader: Rc<dyn Loader>,
    /// Receives the [`Event`]s when set.
    pub trace: Option<Rc<RefCell<Vec<Event>>>>,

    // The area.
    pub wam: Wam,
    pub wam_name: String,
    /// The WAM path buffer 0x006cdce0 (0x104 bytes) as the checkpoint
    /// writer copies it whole: see [`Engine::set_wam_path`].
    pub wam_path: [u8; WAM_PATH_LEN],
    /// The place shown (0x006cd7c0), `None` before the first.
    pub current_place_id: Option<u16>,
    /// Its record in `wam.places`; `None` after a sub-WAM swap until the
    /// next place change.
    pub current_place: Option<usize>,
    /// The place to enter next.
    pub next_place_id: Option<u16>,
    /// A script Vue= entered a place: the place loop runs its arrival scan.
    pub arrival_scan_pending: bool,
    pub arrival_from_vue: u16,
    /// The place a script Vue= entered, which the Vue= handler also stores
    /// as the place loop's zone 0x006cd7f4 (`FUN_0042a100` @ 0x0042a659):
    /// after a click section or a departure scan that ran it, the place
    /// loop goes on with that value.
    pub(crate) script_vue_place: Option<u16>,
    /// 0x004960c0: set around the /con click scan of a character clicked
    /// with an object in hand (`FUN_00425828` @ 0x00425882..0x004258ad);
    /// `FUN_0041afd0` then draws no cursor (@ 0x0041afe8).
    pub(crate) give_scan: bool,
    /// 0x00496154: set by `FUN_004240fc` around a puzzle and its /sel scan
    /// (@ 0x00424166, cleared @ 0x00424242); `FUN_0041afd0` then steps no
    /// cursor (@ 0x0041b03a).
    pub(crate) puzzle_scan: bool,
    /// A move ended: the place loop polls the input once and clears every
    /// input flag before the next arrival scan ([`Engine::end_of_move_input`]).
    pub move_input_poll: bool,
    /// The panorama a move loads (`FUN_00421cfc` at the click builds
    /// 0x006cd288 from the destination's record in the WAM being left; see
    /// [`Engine::execute_transition`]).
    pub move_cyclo: Option<(u16, String)>,
    /// The first arrival scan after a chapter load reads ArriveeVue =
    /// LastVue.
    pub(crate) arrival_after_load: bool,
    /// LastVue.
    pub last_vue: u16,
    pub chapter: u32,
    /// The CON clocks (ChapterTime, Time1, Time2).
    pub clocks: ConClocks,
    /// The epoch 0x00bcb5b8 of the game clock `FUN_00451d0c`
    /// ([`Engine::game_clock_ms`]), taken by `FUN_00451cec` at boot
    /// (WinMain @ 0x004193e8 -> `FUN_0041b7f0` @ 0x0041b80d: the engine's
    /// creation here), at the inventory bar's exit (0x0042c844) and after
    /// the in-game menu (0x0041cb84) ([`Engine::rebase_con_clocks`]).
    /// Shared with the ambient sounds' clock.
    pub(crate) clock_epoch: Rc<Cell<u64>>,
    /// The CON variables.
    pub vars: Vars,
    /// The music and the NewSound sample.
    pub music: Music,
    /// Set when the place loop has to end.
    pub abort: Option<Abort>,
    /// A CON section plays from a click (0x00496170).
    pub section_play: bool,
    /// An Escape that `FUN_0041a298` saw in a loop that does not end on it
    /// (the turn before a move's video, the subject menu) left 0x00496124
    /// = 0 and the menu request 0x00496138 = 1 (@ 0x0041a419..0x0041a423):
    /// the next loop that runs only while 0x00496124 is 1 (a video, a
    /// voice-only line, the game-over wait) stops at once and puts both
    /// back; the end of a conversation puts them back too (0x00425be5).
    pub escape_request: bool,
    /// The warp loop ended without a click, as the original's place loop
    /// then still acts on the zone under the pointer
    /// ([`Engine::no_click_exit`], [`BugFix::GameOverPuzzle`] off): `true`
    /// after a game over or an Exit (0x00496124 is 1), `false` after the
    /// in-game menu's LOAD or QUIT (0x00496124 is 0).
    pub(crate) pending_no_click_exit: Option<bool>,
    /// A script Vue= named a place the WAM does not have: the variable VUE
    /// reads it until the next place change.
    pub vue_override: Option<i32>,
    /// The inventory records (0x006ce138): the object id of each, in
    /// order. The object in hand is OBJETENMAIN.
    pub inventory: Vec<i32>,
    /// The records' icon bytes (0x006ce138 + 9 r), by index: `Some` for an
    /// icon taken from a sprite-list row or a checkpoint, `None` (or no
    /// entry) for the table's index of the id (`FUN_0042b328`). See
    /// [`Engine::record_icon`].
    pub inventory_icons: Vec<Option<u8>>,
    /// The icon of the object in hand (0x006cd7ec) with the id it was set
    /// for: a pick-up's row icon or a bar record's byte. For another id the
    /// hand shows the table's index of the id. See [`Engine::held_icon`].
    pub held_icon: Option<(i32, u8)>,
    /// The first record the bar shows (0x00496924).
    pub bar_first: usize,
    /// The visible slot whose x the last pick-up set (`FUN_004278a0` @
    /// 0x00427ae2, [`crate::ui::bar::pickup_slot_x`]), until the next
    /// slide-in resets the slot table.
    pub bar_pickup_slot: Option<usize>,
    /// The bar request and the right-press flag.
    pub bar: BarSession,
    /// The guests following the player, and whether they are shown
    /// (HideGuest / ShowGuest, 0x0049613c).
    pub guests: Guests,
    pub guests_shown: bool,
    /// Bow mode (0x00496164), the arrow frame (0x0049619d), ARCWARP.SPR.
    pub bow_mode: bool,
    pub arrow_frame: u8,
    pub(crate) bow_sprites: Vec<SprSprite>,
    /// The interface's sprites.
    pub ui: Rc<UiAssets>,
    /// SPRLIST\EPI.TXT, the checkpoints' names.
    pub epi_text: Vec<u8>,
    /// The options of the menus and the enhancements as the settings file
    /// keeps them.
    pub settings: Settings,
    /// The SUBTITLES toggle's click counter (0x00496098); in the Japanese
    /// DVD build the subtitles' language (`ExeBuild::subtitle_languages`).
    pub subtitle_clicks: u8,
    /// The Japanese DVD build's SUBTITLES button was clicked: the chapter's
    /// CON text is to be read again in the language of the counter
    /// (FUN_0041d768 @ 0x0041d833), which the scripts do when the menu
    /// closes ([`crate::hooks::ScriptHooks::reload_con_text`]).
    pub con_text_reload: bool,
    /// The sound menu's knobs, kept for the session (0 = the file's).
    pub son_knob_x: [i32; 4],
    /// The LOAD rows' widths 0x00611e80, by chapter - 1 (atoi of the
    /// checkpoint's name): written only by the rows drawer `FUN_00420b48`
    /// for the rows it draws, zero at launch, kept for the process; the
    /// picker's title is centred by it (0x0041d431..0x0041d44d).
    pub(crate) load_row_widths: [i32; 256],
    /// The menus' record table, one for the session as the original's
    /// (zeroed at start-up; see [`crate::ui::layout::MenuRecordTable`]);
    /// not used with the minor bug fixes ([`Engine::menu_table`]).
    pub(crate) menu_records: crate::ui::layout::MenuRecordTable,
    /// The captions of the CINEMA video playing (`ExeBuild::cinema_captions`).
    pub(crate) cinema_captions: Option<crate::place::CinemaCaptionState>,
    /// The voices' volume, 0..255.
    pub voice_volume: u8,
    /// The players, and the current one's row (0x00496607).
    pub players: Players,
    pub player: usize,
    pub saves: Rc<dyn SaveStore>,
    /// The previous chapter (0x00772907) and the disc in use (0x00772913).
    pub prev_chapter: u8,
    pub disc: u8,
    /// A checkpoint load runs (0x00496168): no checkpoint is written.
    pub load_running: bool,
    /// The place table slots as the WAM loads wrote them (0x006cd894 + the
    /// records' transition arrays), stale entries included: a departure
    /// Vue= reads entry i of the new place's record.
    pub(crate) place_table_slots: Vec<Vec<Transition>>,
    /// The hidden persos (HidePerso): their sprites start inactive.
    pub hidden_persos: Vec<i32>,
    /// The dialog subtitles are shown (SUBTITLES, 0x004960a8).
    pub subtitles: bool,
    /// The object just given, shown under the talking heads until the end
    /// of the section walk (0x0049619c): its OBJETS1.SPR sprite.
    pub given_object: Option<usize>,
    /// The CON variable ZOOM: the next transition turn zooms in.
    pub zoom: bool,
    /// ANGLEWARPX / ANGLEWARPY of the last warp frame.
    pub angle_warp: (i32, i32),
    /// What the puzzles keep from one run to the next.
    pub puzzles: crate::puzzle::PuzzleState,

    // The place sprites.
    pub sprite_list: Vec<SpritePlaceEntry>,
    pub spf_list: Vec<SpriteTransEntry>,
    pub(crate) npc_rects: Vec<NpcClickRect>,
    /// Per view, the row of the current animation (NEWWSPRANIM).
    pub sprite_anim_current: Vec<usize>,
    pub(crate) sprite_anim_last: [u32; atlantis_data::sprite_anim::SPRITE_ANIM_SLOTS],
    pub(crate) sprite_anim_tick: u32,
    /// A step of [`Engine::animate_sprites`] changed a frame or an active
    /// flag that the panorama's sprite layer does not show yet: the next
    /// call draws it before it steps (`FUN_00425304` draws, then steps).
    pub(crate) sprite_layer_stale: bool,
    /// WSPRFRAME.
    pub wspr_frame: i32,
    /// F9 during navigation: the diagnostic overlay (port-only).
    pub debug_overlay: std::cell::Cell<bool>,
    pub(crate) debug_font: std::cell::OnceCell<crate::subtitles::SubtitleFont>,
    /// 0x00496188 == 1 (a talking-head line): the persos are not drawn.
    pub sprites_hide_npcs: bool,
    /// The 1997 builds' subtitle text buffer: zeroed with the engine, as
    /// the original's start-up clear (`FUN_00422bd0`, once per run: loading
    /// a game or starting a new one keeps it); written for every `[` line
    /// the section executor runs and for a game-over narrator line
    /// (`Scripts`), whether the subtitles are shown or not. Their breaker
    /// reads its bytes past the NUL.
    pub subtitle_text: crate::subtitles::SubtitleTextBuffer,
    /// The SPW files of the place, by name.
    pub(crate) spw_cache: HashMap<String, Rc<Vec<u8>>>,

    // The view.
    pub warp: Warp,
    /// The panorama as loaded, and with the place sprites drawn on it.
    pub(crate) cyclo: Vec<u16>,
    pub(crate) cyclo_h: usize,
    pub(crate) warp_surface: Vec<u16>,
    /// The port's HD mode (see [`crate::hd`]).
    pub(crate) hd: crate::hd::HdState,
    pub warp_loaded: bool,
    pub(crate) momentum: WarpMomentum,
    pub(crate) pointer: WarpPointer,
    pub(crate) pointer_mouse_mark: (i32, i32),
    pub(crate) cursor_sprites: Vec<SprSprite>,
    pub(crate) cursor_state: WarpCursorState,
    /// The double-click flag 0x00496118 and the press Windows pairs with
    /// the next for WM_LBUTTONDBLCLK (set during the turn, tested before a
    /// move's video @ 0x00423f8a).
    pub(crate) turn_double_click: crate::turn::TurnDoubleClick,
    pub(crate) cursor_shown: i32,
    pub(crate) cursor_next_ms: u64,
    /// The view keys F5..F8 held at the polls (`FUN_0041a298`) of the loops
    /// other than the warp loop's look, one mask per poll
    /// ([`Engine::poll_view_keys`]): they move the stored view angles
    /// 0x00bcb59c / 0x00bcb598, which the port's warp holds only once the
    /// view is set again ([`Engine::stored_view`]).
    pub(crate) view_key_polls: RefCell<Vec<u8>>,

    /// The two IX frame buffers, kept from video to video (malloc(0x12c000)
    /// @ 0x00418f83).
    pub(crate) ix_buffers: Vec<u16>,

    // The ambient sounds.
    pub(crate) ambient: AmbientSounds,
    pub(crate) ambient_out: MixerAmbientOutput,
    /// The centre table 0x00554510 (`FUN_004315b4`), never cleared.
    pub(crate) centres: BTreeMap<i32, CentrePos>,
}

/// The cursor has to be chosen again (`kWarpCursorUnset`).
pub(crate) const CURSOR_UNSET: i32 = -2;

/// The clock `FUN_004320bc` draws from, the game clock `FUN_00451d0c`
/// (timeGetTime less the epoch 0x00bcb5b8, `epoch` here), which redraws
/// while the value & 0x7f is above 100: the original's clock moves while it
/// spins. The engine's clock only moves between frames, so a draw of
/// 101..127 spun forever (the window hung after a conversation, a video, a
/// place change); each further read within one frame reads 1 ms later.
pub(crate) fn spinning_clock(sys: Rc<Sys>, epoch: Rc<Cell<u64>>) -> impl FnMut() -> u32 {
    let mut last = (u64::MAX, 0u64);
    move || {
        let now = sys.millis();
        last = if last.0 == now {
            (now, last.1 + 1)
        } else {
            (now, 0)
        };
        (now.wrapping_sub(epoch.get()) + last.1) as u32
    }
}

impl Engine {
    pub fn new(sys: Rc<Sys>, game: Game, loader: Rc<dyn Loader>) -> Engine {
        let mut ambient = AmbientSounds::new();
        let clock_sys = sys.clone();
        // FUN_00451cec @ 0x0041b80d: the game clock starts at boot.
        let clock_epoch = Rc::new(Cell::new(sys.millis()));
        // FUN_004320bc draws from timeGetTime - 0x00bcb5b8 (FUN_00451d0c).
        ambient.set_clock(spinning_clock(clock_sys, clock_epoch.clone()));
        // The effects volume 0x00495e73 as at start-up.
        ambient.effects_volume = crate::persist::DEFAULT_VOLUMES[2];
        let ambient_out = MixerAmbientOutput::new(sys.mixer.clone());
        let music = Music::new(&sys);
        let menu_records = crate::ui::layout::MenuRecordTable::new(usize::from(
            game.exe.build.menu_records_per_menu,
        ));
        Engine {
            sys,
            game,
            loader,
            trace: None,
            wam: Wam::default(),
            wam_name: String::new(),
            wam_path: [0; WAM_PATH_LEN],
            current_place_id: None,
            current_place: None,
            next_place_id: None,
            arrival_scan_pending: false,
            arrival_from_vue: 0,
            script_vue_place: None,
            give_scan: false,
            puzzle_scan: false,
            move_input_poll: false,
            move_cyclo: None,
            arrival_after_load: false,
            last_vue: 0,
            chapter: 0,
            clocks: ConClocks::default(),
            clock_epoch,
            vars: Vars::default(),
            music,
            abort: None,
            section_play: false,
            escape_request: false,
            pending_no_click_exit: None,
            vue_override: None,
            inventory: Vec::new(),
            inventory_icons: Vec::new(),
            held_icon: None,
            bar_first: 0,
            bar_pickup_slot: None,
            bar: BarSession::default(),
            guests: Guests::default(),
            guests_shown: true,
            bow_mode: false,
            arrow_frame: 0,
            bow_sprites: Vec::new(),
            ui: Rc::new(UiAssets::default()),
            epi_text: Vec::new(),
            settings: Settings::default(),
            subtitle_clicks: 0,
            con_text_reload: false,
            son_knob_x: [0; 4],
            load_row_widths: [0; 256],
            menu_records,
            cinema_captions: None,
            voice_volume: crate::persist::mixer_volume(crate::persist::DEFAULT_VOLUMES[3]),
            players: Players::new(b"#New Player"),
            player: 0,
            saves: Rc::new(MemStore::new()),
            prev_chapter: 0,
            disc: 1,
            load_running: false,
            place_table_slots: Vec::new(),
            hidden_persos: Vec::new(),
            subtitles: true,
            given_object: None,
            zoom: false,
            angle_warp: (0, 0),
            puzzles: crate::puzzle::PuzzleState::default(),
            sprite_list: Vec::new(),
            spf_list: Vec::new(),
            npc_rects: Vec::new(),
            sprite_anim_current: vec![0; 512],
            sprite_anim_last: [0; atlantis_data::sprite_anim::SPRITE_ANIM_SLOTS],
            sprite_anim_tick: 0,
            sprite_layer_stale: false,
            wspr_frame: 0,
            debug_overlay: std::cell::Cell::new(false),
            debug_font: std::cell::OnceCell::new(),
            sprites_hide_npcs: false,
            subtitle_text: Default::default(),
            spw_cache: HashMap::new(),
            warp: Warp::new(),
            cyclo: Vec::new(),
            cyclo_h: 0,
            warp_surface: Vec::new(),
            hd: crate::hd::HdState::default(),
            warp_loaded: false,
            momentum: WarpMomentum::default(),
            pointer: WarpPointer::default(),
            pointer_mouse_mark: (0, 0),
            cursor_sprites: Vec::new(),
            cursor_state: WarpCursorState::default(),
            turn_double_click: Default::default(),
            cursor_shown: CURSOR_UNSET,
            cursor_next_ms: 0,
            view_key_polls: RefCell::new(Vec::new()),
            ix_buffers: Vec::new(),
            ambient,
            ambient_out,
            centres: BTreeMap::new(),
        }
    }

    pub(crate) fn emit(&self, e: Event) {
        log::info!("{e:?}");
        if let Some(t) = &self.trace {
            t.borrow_mut().push(e);
        }
    }

    /// Reads an archive member (`openBigFileStream`: the name upper-cased).
    pub async fn read(&self, name: &str) -> Option<Vec<u8>> {
        let names = [name.to_string()];
        self.loader.prefetch(&self.game.big, &names).await;
        let data = self.game.big.read(name);
        if data.is_none() {
            log::warn!("{name} not found");
        }
        data
    }

    /// A file of the player's beside the game's files, not an archive
    /// member (their own subtitle font).
    pub async fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.loader.read_beside(&self.game.big, name).await
    }

    /// Makes several members readable at once (one fetch on the web).
    pub async fn prefetch(&self, names: &[String]) {
        self.loader.prefetch(&self.game.big, names).await;
    }

    /// The current place record.
    pub fn place(&self) -> Option<&Place> {
        self.current_place.and_then(|i| self.wam.places.get(i))
    }

    /// The time the CON clocks are counted in: ChapterTime, Time1 and
    /// Time2 are differences of two readings of the game clock
    /// `FUN_00451d0c`, so any base serves; this one is the engine's.
    pub fn now(&self) -> u32 {
        self.sys.millis() as u32
    }

    /// The game clock `FUN_00451d0c`: timeGetTime() - [0x00bcb5b8], the
    /// milliseconds since its epoch was last taken (`FUN_00451cec`: boot,
    /// the inventory bar's exit, the in-game menu).
    pub fn game_clock_ms(&self) -> u32 {
        self.sys.millis().wrapping_sub(self.clock_epoch.get()) as u32
    }

    /// CHAPTERTIME: `FUN_0041c75c`, (now - start) / 1000.
    pub fn chapter_time(&self) -> i32 {
        self.clocks.chapter_time(self.now())
    }

    /// The window closed, or the place loop has to end.
    /// The optional differences from the original game in force (shared
    /// with the options menu through [`Sys`], so a change applies at once).
    pub fn enhancements(&self) -> Enhancements {
        self.sys.enhancements()
    }

    /// The menus' record table of the session, as the original keeps it
    /// (the build's records per menu); `None` with the menus' own records
    /// ([`crate::enhancements::BugFix::MenuRecords`]):
    /// each menu then keeps its own records, from zero at every visit, so
    /// that the sound menu's records 16 and 17 no longer move the ARE YOU
    /// SURE ? ornament off screen in the two 16-record builds, and the
    /// LOAD knob hidden by a visit with 11 checkpoints or fewer comes back
    /// on a later visit with more.
    pub(crate) fn menu_table(&mut self) -> Option<&mut crate::ui::layout::MenuRecordTable> {
        if self.menus_own_records() {
            return None;
        }
        let n = usize::from(self.game.exe.build.menu_records_per_menu);
        if self.menu_records.records_per_menu() != n {
            self.menu_records = crate::ui::layout::MenuRecordTable::new(n);
        }
        Some(&mut self.menu_records)
    }

    pub fn set_enhancements(&mut self, x: Enhancements) {
        self.sys.set_enhancements(x);
    }

    // Each place a bug fix changes reads it through one of these, with the
    // fixes of the moment (tests/fix_menu.rs sets each fix alone, then
    // every other, and checks every reading).

    /// The music through the original's streaming ring
    /// ([`BugFix::MusicLoop`] off), read at each track start.
    pub fn music_through_ring(&self) -> bool {
        !self.enhancements().fix(BugFix::MusicLoop)
    }

    /// The talking heads' SPF overlay drawn ([`BugFix::HeadOverlay`]).
    pub fn head_overlay_drawn(&self) -> bool {
        self.enhancements().fix(BugFix::HeadOverlay)
    }

    /// A cinematic's own sound starts over the sound of the cinematic
    /// before it ([`BugFix::CinemaSound`]).
    pub fn cinema_sound_fix(&self) -> bool {
        self.enhancements().fix(BugFix::CinemaSound)
    }

    /// Each menu with its own click records ([`BugFix::MenuRecords`]).
    pub fn menus_own_records(&self) -> bool {
        self.enhancements().fix(BugFix::MenuRecords)
    }

    /// OK on the fifth player keeps its name ([`BugFix::FifthPlayer`]).
    pub fn keep_fifth_name(&self) -> bool {
        self.enhancements().fix(BugFix::FifthPlayer)
    }

    /// The snakes puzzle won only with every snake released
    /// ([`BugFix::SnakesFirstWin`]).
    pub fn snakes_first_run_fix(&self) -> bool {
        self.enhancements().fix(BugFix::SnakesFirstWin)
    }

    /// An Escape inside a puzzle latches 0x00496124 = 0 / 0x00496138 = 1,
    /// as the original ([`BugFix::PuzzleEscape`] off).
    pub fn puzzle_escape_latches(&self) -> bool {
        !self.enhancements().fix(BugFix::PuzzleEscape)
    }

    /// A game over ends the place loop's pass at once, GAMEOVER cleared
    /// as the screen ends ([`BugFix::GameOverPuzzle`]); off, GAMEOVER
    /// stays set until the reload and a warp loop left without a click
    /// still runs the zone under the pointer ([`Engine::no_click_exit`]).
    pub fn game_over_cleared_at_once(&self) -> bool {
        self.enhancements().fix(BugFix::GameOverPuzzle)
    }

    /// The hexagon puzzle forces OBJETENMAIN to 402 before its test: the
    /// build's rule, or [`BugFix::HexagonPieces`].
    pub fn hexa_forces_402(&self) -> bool {
        self.game.exe.build.hexa_forces_402 || self.enhancements().fix(BugFix::HexagonPieces)
    }

    /// LOADMUSIK copies its tracks one by one where the build's copies all
    /// or nothing ([`BugFix::LoadMusik`]).
    pub fn loadmusik_track_by_track(&self) -> bool {
        self.enhancements().fix(BugFix::LoadMusik)
    }

    /// `ß` drawn and measured as two 's' ([`BugFix::SharpS`]); never in the
    /// Czech and Brazilian CDs, whose 0xe1 is `á` with a glyph
    /// ([`crate::enhancements::sharp_s_without_glyph`]).
    pub fn sharp_s_as_ss(&self) -> bool {
        self.enhancements().fix(BugFix::SharpS)
            && crate::enhancements::sharp_s_without_glyph(self.game.exe.build)
    }

    /// The disc whose copy the reads from the disc in the drive take: the
    /// current disc, or the first copy (`None`) with
    /// [`BugFix::DiscRevision`].
    pub fn current_disc_copy(&self) -> Option<u8> {
        (!self.enhancements().fix(BugFix::DiscRevision)).then_some(self.disc)
    }

    /// The menu walk's rules: the build's, with [`BugFix::SharpS`],
    /// [`BugFix::MenuLineWidth`] and [`BugFix::SubtitlesToggle`].
    pub fn menu_rules(&self) -> crate::ui::layout::MenuRules {
        crate::ui::layout::MenuRules::of_build(self.game.exe.build, &self.enhancements().fixes)
    }

    /// The Chinese subtitle rows' fix ([`BugFix::ChineseRows`]).
    pub fn chinese_row_fix(&self) -> bool {
        self.enhancements().fix(BugFix::ChineseRows)
    }

    /// The subtitles' texts are the Japanese ones: the Japanese DVD build
    /// with its language counter at 0 (`ExeBuild::subtitle_languages`).
    pub fn japanese_texts(&self) -> bool {
        self.game.exe.build.subtitle_languages && self.subtitle_clicks == 0
    }

    /// A dialog line's or a game-over narrator's subtitle is drawn:
    /// SUBTITLES is set, or in the Japanese DVD build the language counter
    /// is below 2 (atlantis-dvd-ja-696832's callers @ 0x0042ebf2,
    /// 0x0042fbfe, 0x0042fea0, 0x004301e0, 0x004264ce test `SUBTITLES == 1
    /// || counter < 2`, and its drawer FUN_0042f86c has no test of its own;
    /// the other builds' drawers return when SUBTITLES is 0, FUN_00430178
    /// @ 0x0043019a).
    pub fn subtitles_shown(&self) -> bool {
        self.subtitles || (self.game.exe.build.subtitle_languages && self.subtitle_clicks < 2)
    }

    /// The subtitle breaker of the moment: the Chinese builds' (with their
    /// rows' fix as it stands), the Japanese DVD build's rows at the spaces
    /// while its texts are Japanese; `None`: the reference's.
    pub fn subtitle_split(&self) -> Option<crate::subtitles::CjkSplit> {
        let build = self.game.exe.build;
        if build.subtitle_languages && !self.japanese_texts() {
            return None;
        }
        crate::subtitles::CjkSplit::of_build(build, self.chinese_row_fix())
    }

    /// The code page the subtitles are read in: the build's, or with the
    /// Dutch and Swedish DVDs' DOS texts and the fix
    /// [`BugFix::DosLetters`], the DOS code page they are written in.
    pub fn subtitle_code_page(&self) -> atlantis_data::TextCodePage {
        let build = self.game.exe.build;
        if build.dos_subtitle_texts && self.enhancements().fix(BugFix::DosLetters) {
            atlantis_data::TextCodePage::Dos850
        } else {
            build.text_code_page
        }
    }

    pub fn should_abort(&self) -> bool {
        self.sys.should_quit() || self.abort.is_some()
    }

    /// After the inventory bar (`FUN_0042c59c` @ 0x0042c844), the pick-up
    /// bar and a resumed in-game menu (`FUN_0041c838` @ 0x0041cb84): Time1
    /// and Time2 resume from the values they stood at, ChapterTime starts
    /// again from 0.
    /// `FUN_00451cec` first restarts the game clock at about 0.
    pub fn rebase_con_clocks(&mut self) {
        self.clock_epoch.set(self.sys.millis());
        let (t1, t2) = (self.vars.get("time1"), self.vars.get("time2"));
        let now = self.now();
        self.clocks.rebase(now, t1, t2);
    }

    /// `FUN_0041c75c`, at the start of every warp frame (0x0041c086).
    /// Random is the low byte of the game clock (0x0041c774).
    pub fn update_con_clocks(&mut self) {
        let now = self.now();
        self.vars
            .set("random", (self.game_clock_ms() & 0xff) as i32);
        self.vars.set("chaptertime", self.clocks.chapter_time(now));
        self.vars.set("time1", self.clocks.timer_value(1, now));
        self.vars.set("time2", self.clocks.timer_value(2, now));
    }

    /// Sets the view angles (an /INIT AngleWarpX / AngleWarpY, a test). It
    /// writes 0x00bcb59c / 0x00bcb598: the view keys polled before it are
    /// overwritten.
    pub fn set_view(&mut self, alpha: f32, beta: f32) {
        self.view_key_polls.borrow_mut().clear();
        self.warp.set_alpha(alpha);
        self.warp.set_beta(beta);
    }

    /// The CURSEURS.SPR cursors (`loadCursorSprites`, 64x64 at most).
    pub(crate) async fn load_cursor_sprites(&mut self) {
        let (w, h) = atlantis_data::sprite::limits::CURSORS;
        self.cursor_sprites = match self.read("SPRITE\\2D\\CURSEURS.SPR").await {
            Some(d) => read_spr_file(&d, w, h).unwrap_or_default(),
            None => Vec::new(),
        };
        log::info!("{} cursor sprites", self.cursor_sprites.len());
    }

    /// The WAM path buffer 0x006cdce0 rebuilt as the original does before
    /// each WAM load (a new game @ 0x0042e250, ChangeWam= @ 0x0042a253, a
    /// sub-WAM swap @ 0x00422b4c): strcpy(root, "" @ 0x0049619f), then
    /// strcat("wam\\"), strcat(name). The name keeps the case of the
    /// script line or the WAM record, and the bytes after the new NUL keep
    /// what a longer path left there; the checkpoint writer copies the
    /// whole buffer (0x104 bytes), so real checkpoints hold such tails
    /// (`WAM\TRONE.WAM\0AM`).
    pub fn set_wam_path(&mut self, name: &[u8]) {
        let mut path = b"wam\\".to_vec();
        path.extend_from_slice(name);
        let n = path.len().min(WAM_PATH_LEN - 1);
        self.wam_path[..n].copy_from_slice(&path[..n]);
        self.wam_path[n] = 0;
    }

    /// A WAM file, its centre table and sprite lists (`setupWAMByName`,
    /// `FUN_00421ebc`). The loader writes the fixed place records on every
    /// load, the chapter's WAM included (a new game @ 0x004237b5, a load
    /// @ 0x0042e354): the place table slots.
    pub async fn load_wam(&mut self, name: &str) -> bool {
        let Some(data) = self.read(&format!("WAM\\{name}")).await else {
            return false;
        };
        self.wam = Wam::parse(&data);
        self.wam_name = name.to_ascii_uppercase();
        self.record_place_table();
        self.load_ambient_centres().await;
        self.load_sprite_list().await;
        self.load_spf_list().await;
        self.emit(Event::WamLoaded(self.wam_name.clone()));
        true
    }

    /// `FUN_004315b4`: file reference 2 (by position) names the centre
    /// table, `sprlist\` + name; entries are added, never cleared.
    async fn load_ambient_centres(&mut self) {
        let Some(name) = self.wam.file_refs.get(2).filter(|n| !n.is_empty()).cloned() else {
            return;
        };
        if let Some(d) = self.read(&format!("SPRLIST\\{name}")).await {
            atlantis_data::wam::parse_centre_vue(&d, &mut self.centres);
        }
    }

    /// The SPW list (`loadSpritePlaceList`, `FUN_00424a20`), then
    /// `FUN_00427d30` for the hidden persos.
    pub(crate) async fn load_sprite_list(&mut self) {
        self.sprite_list.clear();
        self.sprite_anim_current.iter_mut().for_each(|c| *c = 0);
        if self.wam.spw_list.is_empty() {
            return;
        }
        let name = format!("SPRLIST\\{}", self.wam.spw_list);
        if let Some(d) = self.read(&name).await {
            self.sprite_list = atlantis_data::sprite_list::parse_sprite_list(&d);
            atlantis_data::sprite_anim::apply_hidden(&mut self.sprite_list, &self.hidden_persos);
        }
    }

    /// The SPF list of the transitions' sprites (`loadSpfList`).
    async fn load_spf_list(&mut self) {
        self.spf_list.clear();
        if self.wam.spf_list.is_empty() {
            return;
        }
        let name = format!("SPRLIST\\{}", self.wam.spf_list);
        if let Some(d) = self.read(&name).await {
            self.spf_list = atlantis_data::sprite_list::parse_spf_list(&d);
        }
    }

    /// The new game: `changeChapter(1)` / `FUN_0042de5c` without a
    /// checkpoint, then `FUN_00423760` on the boot pass. `in_play`: a new
    /// game from play (a game over without a checkpoint), which keeps the
    /// rest of the state (`keepState`).
    pub async fn new_game(&mut self, hooks: &mut dyn ScriptHooks, in_play: bool) {
        // A direct chapter start begins with no hidden persos; the CON
        // /INIT block establishes the initial set.
        if !in_play {
            self.hidden_persos.clear();
            // The guest list is zeroed at boot (FUN_004239f4), shown.
            self.guests.clear();
            self.guests_shown = true;
        }
        self.bar_first = 0; // FUN_0042de5c @ 0x0042e2ec
        self.disc = 1; // the no-checkpoint branch (0x00772914 = 1)
        self.prev_chapter = 0;
        // FUN_0042de5c step 4 (both branches): ZOOM = NEWSOUND = 0; NEWSOUND
        // is one of the variables the chapter change keeps.
        self.zoom = false;
        self.vars.set("zoom", 0);
        self.vars.set("newsound", 0);
        self.next_place_id = None;
        self.arrival_scan_pending = false;
        // The view angles: no function of the load path writes them, only
        // an /INIT AngleWarpX / AngleWarpY; at boot they are 0 (.bss), kept
        // otherwise.
        if !in_play {
            self.set_view(0.0, 0.0);
        }
        // FUN_0042de5c's no-file branch (@ 0x0042e250..0x0042e279):
        // `wam\` + *(0x004961e0) = "atlan1.wam".
        self.set_wam_path(b"atlan1.wam");
        if !self.load_wam(NEW_GAME_WAM).await {
            log::error!("cannot start a new game: {NEW_GAME_WAM} missing");
            return;
        }
        self.chapter = 1;
        if !in_play {
            // The record FUN_0042c054 sets up before the player menu: object
            // 257 alone. FUN_00423760 zeroes the timer offsets and restarts
            // Time1 when a game starts.
            self.inventory = vec![257];
            self.inventory_icons.clear();
            let now = self.now();
            self.clocks.time1_offset = 0;
            self.clocks.time2_offset = 0;
            self.clocks.time1_start = now;
        }
        // LastVue = 1 (0x004195fb, 0x0042e285).
        self.last_vue = 1;
        hooks.chapter_loaded(self, 1, in_play).await;
        // An /INIT Vue= has entered its place already.
        if self.next_place_id.is_none() && !self.arrival_scan_pending {
            self.next_place_id = Some(NEW_GAME_VUE);
        }
        // ARRIVEEVUE = LastVue (FUN_00427634 @ 0x0042777d), whatever the
        // /INIT Vue= of chapter 1 did.
        self.arrival_after_load = true;
    }

    /// The first arrival scan after a chapter load: ARRIVEEVUE = LastVue,
    /// no scan when it is 0. Every other scan keeps the origin given.
    fn take_arrival_after_load(&mut self, arrivee: &mut i32) -> bool {
        if !self.arrival_after_load {
            return true;
        }
        self.arrival_after_load = false;
        *arrivee = i32::from(self.last_vue);
        *arrivee != 0
    }

    /// The place loop (`gameStep`, `FUN_00423ad8`): enter the pending
    /// place and run its arrival scan, run the warp view, act on the click.
    /// Returns when the game is quit or [`Engine::abort`] is set.
    pub async fn game_step(&mut self, hooks: &mut dyn ScriptHooks) {
        loop {
            if self.should_abort() {
                break;
            }
            // The place loop returns as soon as Exit is set (0x00423b26,
            // 0x00423bb3, 0x00423cf0); the main loop then ends the game
            // when it is 86 (0x00419787).
            if self.vars.get("exit") != 0 {
                self.abort = Some(Abort::Finished);
                break;
            }
            // Enter a pending place (a new game, a move, a script Vue=), then
            // run its arrival /sel scan, which may chain on.
            if self.next_place_id.is_some() {
                let mut arrivee = self.current_place_id.map_or(-1, i32::from);
                let scan = self.take_arrival_after_load(&mut arrivee);
                self.do_place_change().await;
                self.end_of_move_input();
                self.arrival_scan_pending = false;
                if let (Some(vue), true) =
                    (self.current_place_id, scan && self.current_place.is_some())
                {
                    hooks.arrival_scan(self, vue, arrivee).await;
                }
            } else if self.arrival_scan_pending {
                // A script Vue= already entered the place (@ 0x00423b48).
                self.end_of_move_input();
                self.arrival_scan_pending = false;
                let mut arrivee = i32::from(self.arrival_from_vue);
                let scan = self.take_arrival_after_load(&mut arrivee);
                if let (Some(vue), true) =
                    (self.current_place_id, scan && self.current_place.is_some())
                {
                    hooks.arrival_scan(self, vue, arrivee).await;
                }
            }
            if self.next_place_id.is_some() || self.arrival_scan_pending {
                continue;
            }
            if self.should_abort() {
                break;
            }
            // ArriveeVue = 0 before the warp loop (@ 0x00423be7), whatever
            // set it: a Vue= or a chapter= of the arrival scan.
            self.vars.set("arriveevue", 0);
            if !self.warp_loaded {
                // Nowhere to look (a missing panorama): wait for the quit.
                self.sys.frame().await;
                continue;
            }

            let action = self.handle_warp(hooks).await;
            if action == WARP_ABORT {
                if let Some(keep_running) = self.pending_no_click_exit.take() {
                    self.no_click_exit(hooks, keep_running).await;
                }
                if self.should_abort() {
                    break;
                }
                continue;
            }

            // A click with no NPC under the pointer: the zone goes to
            // ClicZone (0x00423c1c), and with a zone or an object in hand
            // FUN_004278a0 runs the /con click scan (0x00427b8c..0x00427bc2).
            self.vars.set("cliczone", action as i32);
            let mut zone = action;
            if (1000..1017).contains(&zone) && hooks.puzzle_zone(self, zone - 1000).await {
                zone = 0;
                if self.should_abort() {
                    break;
                }
            }
            if zone == 999 {
                hooks.zone_999(self).await;
                zone = 0;
                if self.should_abort() {
                    break;
                }
            }
            // FUN_004278a0 starts the cursor's click animation on every
            // click it gets (0x004968b8 = 1 @ 0x004278df): a zone, nothing,
            // before a move; the animation plays on in the next warp loop.
            self.cursor_state.click_anim = true;
            self.script_vue_place = None;
            hooks.zone_click(self, action).await;
            if self.should_abort() {
                break;
            }
            self.vars.set("cliczone", 0);
            // A Vue= of the click section left its place as the zone
            // (0x006cd7f4 @ 0x0042a659), which the move test below compares
            // with the destinations of that place (no shipped click section
            // enters a place with an exit to itself).
            if let Some(n) = self.script_vue_place.take() {
                zone = u32::from(n);
            }
            // No ARRIVEEVUE test here: the place loop tests it only at the
            // top of the pass (@ 0x00423b4d). An arrival scan queued since
            // (a Vue= or a chapter= of this click's section, or of a timer
            // section during the warp loop or the bar, which goes on in the
            // new place) runs on the next pass only when the click does not
            // move; a click on an exit of the place now shown moves at once
            // (0x00423d17..0x00423db5 on the current record 0x006cd7a8), and
            // the move's ARRIVEEVUE (0x00423def) replaces the pending one.
            // A move needs an empty hand (0x00423d2b)...
            if self.vars.get("objetenmain") != 0 {
                continue;
            }
            // ...and an entry for the zone in the place (@ 0x00423d4b).
            if zone == 0 || zone >= 10000 {
                continue;
            }
            let Some(place) = self.place() else { continue };
            // FUN_00423ad8's i: the first transition of the place whose
            // destination is the zone.
            let Some(index) = place
                .transitions
                .iter()
                .position(|t| u32::from(t.dst_place) == zone)
            else {
                continue;
            };
            let dest = zone as u16;
            self.next_place_id = Some(dest);
            // LastVue = the place being left (0x00423dcf), before the
            // departure scan.
            self.last_vue = self.current_place_id.unwrap_or(0);
            // Game over ends the pass as it is (0x00423e13); then an
            // AbortDepart, set by this scan or any scan since the last
            // arrival preparation, ends the move with ArriveeVue, AbortDepart
            // and DepartVue cleared, so a Vue= of the scan gets no arrival
            // scan (0x00423e2b..0x00423e7e).
            // The departure scan runs with ArriveeVue as the timer's Vue=
            // (the place it left) or chapter= (LastVue) left it.
            self.arrival_scan_pending = false;
            self.script_vue_place = None;
            let outcome = hooks.departure_scan(self, dest).await;
            let game_over = outcome == Departure::GameOver;
            let vue_ran = self.script_vue_place.take().is_some();
            if outcome == Departure::Abort {
                self.arrival_scan_pending = false;
            }
            if vue_ran && self.arrival_scan_pending {
                // A Vue= inside a departure section already entered its
                // place (CHAPI005 /113, CHAPI062 /864, CHAPI077 /010); the
                // place loop goes on with the turn and video of entry i of
                // the new place's record. ARRIVEEVUE = 0x006cd7c0 after the
                // scan (0x00423def), which the Vue= set to the place entered
                // (FUN_0042a100 @ 0x0042a6fd): the next pass's arrival scan
                // runs with ArriveeVue = Vue = that place, LastVue = the
                // place left.
                let here = self.current_place_id.unwrap_or(0);
                self.vars.set("arriveevue", i32::from(here));
                self.arrival_from_vue = here;
                self.next_place_id = None;
                if !self.should_abort() && !game_over {
                    self.transition_after_departure_vue(index).await;
                    self.vars.move_ended(); // 0x00424089
                    self.move_input_poll = true; // 0x00424093
                }
                continue;
            }
            // A chapter= of the departure scan (an arrival section such as
            // CHAPI059 /010 `/sel(arriveeVue>0)&(Vue=26)`, matched after a
            // timer's Vue=) queued an arrival scan; the move goes on and its
            // ARRIVEEVUE = the place left (0x00423def) replaces it.
            self.arrival_scan_pending = false;
            match (outcome, self.next_place_id.take()) {
                (Departure::Go, Some(dest)) if !self.should_abort() => {
                    // A departure that is not aborted stops timer 1 and
                    // restarts Time1 (@ 0x00423e8d).
                    self.vars.set("testtime1", 0);
                    self.clocks.time1_start = self.now();
                    self.clocks.time1_offset = 0;
                    self.execute_transition(dest).await;
                    self.vars.move_ended(); // 0x00424089
                    self.move_input_poll = true; // 0x00424093
                }
                _ => {}
            }
        }
    }

    /// The end of every move of `FUN_00423ad8`, after the place load and
    /// its sprites (0x00424068, 0x00424072) and DEPARTVUE = 0: one input
    /// poll `FUN_0041a298` (@ 0x00424093), then 0x00496118, 0x0049611c,
    /// 0x00496120, 0x00496138 = 0, 0x00496124 = 1 and 0x004960c4 = 0
    /// (0x00424098..0x004240ca). A left or right press or an Escape made
    /// during the video's last frame or the place load (the video's last
    /// frame has no poll after it) is taken by that poll and cleared at
    /// once: the warp loop's first poll sees no new press.
    pub(crate) fn end_of_move_input(&mut self) {
        if !std::mem::take(&mut self.move_input_poll) {
            return;
        }
        let mut left = false;
        while let Some(ev) = self.sys.poll_event() {
            if let Input::MouseDown {
                button: MouseButton::Left,
                ..
            } = ev
            {
                left = true;
            }
        }
        // F5..F8 move the stored angles (0x0041a33c..0x0041a3ba) unless the
        // poll saw a new left press (0x0041a2b5); nothing writes them
        // before the warp loop's first frame, which shows them.
        self.poll_view_keys(left);
        self.input_flags_reset();
    }

    /// The input flags reset the original writes in one block at the end
    /// of a move (0x00424098..0x004240ca), in StartMusik and StopMusik when
    /// they close the open stream (`FUN_0042a100` @ 0x0042add3..0x0042ae05,
    /// 0x0042af26..0x0042af58) and at the end of the disc prompt
    /// (`FUN_0041eda8` @ 0x0041f032..0x0041f064): 0x00496118 (the double
    /// click), 0x0049611c, 0x00496120, 0x00496138 = 0, 0x00496124 = 1 (the
    /// Escape latch) and 0x004960c4 = 0.
    pub(crate) fn input_flags_reset(&mut self) {
        self.turn_double_click.clear();
        self.escape_request = false;
        self.bar.flags_reset();
    }

    /// `FUN_00431e98` on the way out (0x004197a0).
    pub fn stop_ambient(&mut self) {
        if self.ambient.active() {
            self.ambient.stop_all(&mut self.ambient_out);
        }
    }
}

/// The text of the error box of a missing start-up logo film `name`
/// (`ExeBuild::startup_logos`): the error code 0xc's "Fichier
/// introuvable : " (atlantis-dvd-ja-696832 0x004abfe8, its dialog procedure
/// 0x00419978 @ 0x00419b59) and the path FUN_00423dcc failed to open
/// (copied to 0x006bd9c8 @ 0x00423e01): the films' folder of the build's
/// data (0x004abd28, `\face_b\ubb_vue\`) and the name in lower case
/// (0x00486a08 `logogaga.hnm`, 0x00486a15 `logospik.hnm`).
pub fn startup_logo_error_text(build: &atlantis_data::exe::ExeBuild, name: &str) -> String {
    let folder = build.path_check.map_or("ubb_vue\\", |(_, p)| p);
    format!(
        "Fichier introuvable : {folder}{}",
        name.to_ascii_lowercase()
    )
}

/// The game: the start-up videos, the player screen, the player's
/// checkpoint and the place loop, until the game is quit or ends; the task
/// the front end runs with a [`crate::sys::Runner`].
pub async fn run(sys: Rc<Sys>, game: Game, opts: Options) {
    let Options {
        loader,
        mut hooks,
        trace,
        enhancements,
        subtitles,
        saves,
        start,
        original_start_values,
    } = opts;
    let mut e = Engine::new(sys, game, loader);
    e.trace = trace;
    e.set_enhancements(enhancements);
    // The Chinese builds set SUBTITLES at every start-up, whatever was
    // chosen before (atlantis-cd-zh-tw-696832 WinMain @ 0x0041971f); a
    // checkpoint load still sets the one it holds.
    let subtitles = subtitles || e.game.exe.build.subtitles_on_at_start;
    e.subtitles = subtitles;
    e.saves = saves;
    e.load_settings(Settings {
        subtitles,
        enhancements,
        ..Settings::default()
    });
    if original_start_values {
        e.original_start_values();
    }
    e.load_interface().await;
    if start == Start::Boot {
        e.play_startup().await;
        // The player screen (FUN_0041dea4), then FUN_0042de5c on the
        // player's resume chapter; the boot pass runs FUN_00423760.
        match e.player_screen().await {
            Some(p) => e.player = p,
            None => {
                e.stop_ambient();
                return;
            }
        }
        e.load_checkpoint(hooks.as_mut(), false).await;
    } else {
        if let Start::Puzzle { id, object } = start {
            e.vars.set("objetenmain", object);
            let r = crate::puzzle::play_puzzle(&mut e, hooks.as_mut(), id).await;
            log::warn!("puzzle {id} ended with FinPuzzle {r}");
            e.vars.set("objetenmain", 0);
            e.vars.set("finpuzzle", 0);
            if e.sys.should_quit() {
                e.stop_ambient();
                return;
            }
        }
        e.new_game(hooks.as_mut(), false).await;
    }
    let finished = loop {
        e.abort = None;
        e.game_step(hooks.as_mut()).await;
        match e.abort {
            // A game over or the load picker: the checkpoint of the
            // player's chapter byte (0x0041975d -> FUN_0042de5c), a new
            // game from play without one.
            Some(Abort::NewGame | Abort::Load) if !e.sys.should_quit() => {
                // The checkpoint loader clears GAMEOVER (FUN_0042de5c ->
                // FUN_00426c68), which BugFix::GameOverPuzzle off leaves
                // set until here.
                e.vars.set("gameover", 0);
                e.load_checkpoint(hooks.as_mut(), true).await;
            }
            Some(Abort::Finished) if e.vars.get("exit") == 86 => break true,
            // Any other Exit value (none is shipped): the main loop ends
            // the game only for 86 (0x00419775); otherwise Exit = 0 and the
            // player screen again, then the chosen player's checkpoint as
            // at boot.
            Some(Abort::Finished) if !e.sys.should_quit() => {
                log::warn!(
                    "Exit={}: only 86 ends the game; back to the player screen",
                    e.vars.get("exit")
                );
                e.vars.set("exit", 0);
                match e.player_screen().await {
                    Some(p) => e.player = p,
                    None => break false,
                }
                e.load_checkpoint(hooks.as_mut(), false).await;
            }
            _ => break false,
        }
    };
    if finished && !e.sys.should_quit() {
        // Exit == 86 (CHAPI085, the only value shipped, 0x00419775):
        // FUN_0041a66c, then the program ends.
        e.end_of_game().await;
    }
    e.stop_ambient();
}

impl Engine {
    /// The start-up videos (0x0041960a..0x004196cc): LOGO15N.HNM and
    /// PREINTRO.HNM with their sounds, through the transition player at the
    /// cinema pace, a 200-tick (1 s) wait between them; the Dutch build
    /// plays LOGORP.HNM after the wait.
    pub async fn play_startup(&mut self) {
        // The Japanese DVD build: its publishers' logos first, through
        // LOGO15N's player (WinMain @ 0x00419713..0x00419772). The disc has
        // neither film: the player's open fails and it shows its error box
        // (dialog 0x65, code 0xc, the path as WinMain built it: "" +
        // "\face_b\ubb_vue\" + the name, FUN_00423dcc @ 0x00423df2..
        // 0x00423e22), then returns, untested. With the bug fix
        // (`BugFix::StartupLogos`) a missing film is skipped silently.
        for name in self.game.exe.build.startup_logos {
            let path = format!("UBB_VUE\\{name}");
            if self.game.big.contains(&path) {
                // NEWSOUND is not written before them (0x00419713..
                // 0x00419772): 0 at boot, no sound.
                self.play_startup_film(name, None).await;
            } else if !self.enhancements().fix(BugFix::StartupLogos) {
                log::warn!("{path}: missing, the original's error box");
                let text = startup_logo_error_text(self.game.exe.build, name);
                let font = crate::subtitles::SubtitleFont::new();
                crate::dialog::error_box(self, &font, &text).await;
            } else {
                log::info!("{path}: missing, skipped (fix_startup_logos)");
            }
            if self.sys.should_quit() {
                return;
            }
        }
        // NEWSOUND = 0x12 (0x00419653; every build stores it, e.g.
        // atlantis-cd-en-688128 @ 0x00419535 on its own array):
        // logo15n.apc from the table.
        self.play_startup_film("LOGO15N.HNM", Some(0x12)).await;
        // The busy wait (0x00419662..0x0041967c, no call) reads no input but
        // loses none: FUN_0042e624 fills the key table from
        // GetAsyncKeyState, so the next video's first FUN_0041a298 poll sees
        // a press made during the wait (a left button: 0x0049611c = 1;
        // Escape: 0x00496124 = 0), unless it was already down at LOGO15N's
        // last poll. The input events stay queued for that poll.
        let start = self.sys.millis();
        while self.sys.millis() - start < 1000 && !self.sys.should_quit() {
            self.sys.frame().await;
        }
        // The Dutch build: the R&P Electronic Media logo, through the same
        // player at the cinema pace, its NEWSOUND left as it is (WinMain @
        // 0x0041958d..0x004195bf; the archives hold no LOGORP.APC).
        if self.game.exe.build.startup_logo_rp {
            self.play_startup_film("LOGORP.HNM", None).await;
        }
        // NEWSOUND = 0x13 (0x004196bd): another value than LOGO15N's, so
        // the film loop's first poll stops LOGO15N's sound, playing or
        // fading, and starts preintro.apc (FUN_00432a64, dc:26175..26198).
        self.play_startup_film("PREINTRO.HNM", Some(0x13)).await;
    }

    /// The in-game menu from the warp loop: a reload or a quit ends the
    /// place loop.
    pub(crate) async fn run_ingame_menu(&mut self) {
        // FUN_0041c838 lowers the music while it runs (@ 0x0041c88a) and
        // raises it at the end (@ 0x0041cb7d).
        self.music.duck(false, None);
        let outcome = self.ingame_menu().await;
        self.music.unduck(None);
        // Every exit of FUN_0041c838, the load entry included (its
        // `jmp 0x41ca75` @ 0x0041ca10), runs the tail that rebases the
        // clocks (0x0041cb84..0x0041cbdc: FUN_00451cec, the Time1 / Time2 /
        // ChapterTime bases = now, their offsets = TIME1 / TIME2). The
        // reload then restarts ChapterTime and Time2 but not Time1
        // (FUN_00427634), so Time1 does not count the menu's time.
        self.rebase_con_clocks();
        match outcome {
            MenuOutcome::Resume => {}
            MenuOutcome::Reload => self.abort = Some(Abort::Load),
            MenuOutcome::Quit => self.abort = Some(Abort::Quit),
        }
    }
}

#[cfg(test)]
mod spinning_clock_tests {
    use super::*;
    use crate::ambient::AmbientSounds;
    use crate::audio::Mixer;
    use atlantis_data::wam::AmbientObject;

    #[test]
    fn reads_advance_within_a_frame_and_the_variant_pick_ends() {
        let sys = Sys::new(Mixer::new());
        let mut clock = spinning_clock(sys.clone(), Rc::new(Cell::new(0)));
        // Sys starts at 0 ms: 0, then 1 ms per read in the same frame.
        assert_eq!((clock(), clock(), clock()), (0, 1, 2));
        // Within a frame whose clock reads 115 (& 0x7f above 100) the
        // variant pick ends (it spun forever with the frame's clock).
        for t in [101u64, 115, 127, 128 + 110] {
            let sys = Sys::new(Mixer::new());
            let s = sys.clone();
            let picked = Rc::new(std::cell::Cell::new(false));
            let p = picked.clone();
            let mut r = crate::sys::Runner::new(sys, async move {
                let mut a = AmbientSounds::new();
                a.set_clock(spinning_clock(s, Rc::new(Cell::new(0))));
                let obj = AmbientObject {
                    weights: [50, 50, 0, 0],
                    ..Default::default()
                };
                assert!(a.pick_variant(&obj, 2).is_some());
                p.set(true);
            });
            r.step(t);
            assert!(picked.get(), "clock {t}");
        }
    }
}

#[cfg(test)]
mod exit_and_menu_tests {
    use super::*;
    use crate::audio::Mixer;
    use crate::hooks::{ready, HookFuture};
    use crate::sys::{Input, MouseButton, Runner};
    use crate::test_support::{button, with_engine};

    /// Every exit of the in-game menu rebases the clocks, a load or an
    /// abort as a resume (FUN_0041c838 @ 0x0041cb84..0x0041cbdc, reached
    /// from the load entry @ 0x0041ca10).
    #[test]
    fn ingame_menu_rebases_the_clocks_on_every_exit() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "ingame_menu_rebases_the_clocks_on_every_exit",
            move |mut e| async move {
                e.vars.set("time1", 7);
                e.abort = Some(Abort::Load);
                e.run_ingame_menu().await;
                g2.borrow_mut().push(e.clocks.time1_offset);
                e.abort = None;
                // A click outside the records: the menu resumes.
                e.sys.set_mouse_pos(5, 5);
                e.run_ingame_menu().await;
                g2.borrow_mut().push(e.clocks.time1_offset);
            },
            |sys, f| match f {
                40 => button(sys, MouseButton::Left, true),
                42 => button(sys, MouseButton::Left, false),
                _ => {}
            },
        );
        if !ran {
            return;
        }
        let got = got.borrow();
        assert_eq!(*got, [7, 7]);
    }

    /// The game clock FUN_00451d0c counts from its epoch: the engine's
    /// creation (boot), then each rebase (FUN_00451cec); Random is its low
    /// byte (0x0041c774).
    #[test]
    fn game_clock_restarts_with_the_con_clocks() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "game_clock_restarts_with_the_con_clocks",
            move |mut e| async move {
                e.sys.delay(500).await;
                let before = e.game_clock_ms();
                e.update_con_clocks();
                let random = e.vars.get("random");
                e.rebase_con_clocks();
                let after = e.game_clock_ms();
                e.update_con_clocks();
                *g2.borrow_mut() = Some((
                    before,
                    random == (before & 0xff) as i32,
                    after,
                    e.vars.get("random"),
                ));
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let (before, random_ok, after, random) = got.borrow().unwrap();
        assert!(before >= 500, "{before}");
        assert!(random_ok);
        assert_eq!((after, random), (0, 0));
    }

    /// Exit set on the first chapter load.
    struct ExitHook(i32, bool);

    impl ScriptHooks for ExitHook {
        fn chapter_loaded<'a>(
            &'a mut self,
            e: &'a mut Engine,
            _chapter: u32,
            _keep_state: bool,
        ) -> HookFuture<'a, ()> {
            if !self.1 {
                self.1 = true;
                e.vars.set("exit", self.0);
            }
            ready(())
        }
    }

    fn run_with_exit(exit: i32) -> Option<Vec<Event>> {
        let dir = std::env::var_os("ATLANTIS_DATA")?;
        let g = Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(dir))).unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let sys = Sys::new(Mixer::new());
        let opts = Options {
            trace: Some(trace.clone()),
            hooks: Box::new(ExitHook(exit, false)),
            start: Start::NewGame,
            ..Options::default()
        };
        let mut runner = Runner::new(sys.clone(), run(sys.clone(), g, opts));
        let mut frame = 0u64;
        while runner.step(frame * 16) {
            frame += 1;
            let seen = trace
                .borrow()
                .iter()
                .any(|e| *e == Event::Menu("SELEMENU.TXT".into()));
            if seen || frame == 20_000 {
                sys.push_input(Input::Quit);
            }
            assert!(frame < 40_000);
        }
        let t = trace.borrow().clone();
        Some(t)
    }

    /// Exit=N for N other than 86 does not end the game: the player
    /// screen again (it played the ending).
    #[test]
    fn exit_other_than_86_returns_to_the_player_screen() {
        let Some(t) = run_with_exit(5) else {
            eprintln!("ATLANTIS_DATA not set: skipped");
            return;
        };
        assert!(t.contains(&Event::Menu("SELEMENU.TXT".into())), "{t:?}");
    }
}

#[cfg(test)]
mod end_of_move_tests {
    use super::*;
    use crate::hooks::{ready, HookFuture};
    use crate::sys::{Input, Key, MouseButton};
    use crate::test_support::with_engine;
    use std::cell::Cell;

    struct Clicks(Rc<Cell<u32>>);

    impl ScriptHooks for Clicks {
        fn zone_click<'a>(&'a mut self, _e: &'a mut Engine, _z: u32) -> HookFuture<'a, ()> {
            self.0.set(self.0.get() + 1);
            ready(())
        }
    }

    /// The zone clicks and the bar request after a place load, with a left
    /// press, a right press and Escape queued during it; `moved`: the load
    /// ends a move.
    fn after_load(moved: bool) -> Option<(u32, bool)> {
        let clicks = Rc::new(Cell::new(0));
        let got = Rc::new(Cell::new(None));
        let (c2, g2) = (clicks.clone(), got.clone());
        let ran = with_engine(
            "presses_during_a_move_end_are_lost",
            move |mut e| async move {
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.move_input_poll = moved;
                // During the video's last frame and the place load.
                let (x, y) = (320, 320);
                for b in [MouseButton::Left, MouseButton::Right] {
                    e.sys.push_input(Input::MouseDown { button: b, x, y });
                    e.sys.push_input(Input::MouseUp { button: b, x, y });
                }
                e.sys.push_input(Input::KeyDown(Key::Escape));
                e.sys.push_input(Input::KeyUp(Key::Escape));
                let mut hooks = Clicks(c2);
                e.game_step(&mut hooks).await;
                g2.set(Some(e.bar.request));
            },
            |sys, f| {
                if f == 30 {
                    sys.push_input(Input::Quit);
                }
            },
        );
        ran.then(|| (clicks.get(), got.get().unwrap()))
    }

    /// FUN_00423ad8 polls once after a move's place load and clears every
    /// input flag (0x00424093..0x004240ca): the warp loop sees none of the
    /// presses made meanwhile. Without a move, the first warp pass takes
    /// the queued click.
    #[test]
    fn presses_during_a_move_end_are_lost() {
        let Some(moved) = after_load(true) else {
            return;
        };
        assert_eq!(moved.0, 0, "a click after the move was handled");
        let not_moved = after_load(false).unwrap();
        assert_eq!(not_moved.0, 1);
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::test_support::with_engine;
    use std::cell::RefCell;

    /// WinMain stores NEWSOUND = 0x12 before LOGO15N (0x00419653) and 0x13
    /// before PREINTRO (0x004196bd); each film's first poll of
    /// `FUN_00432a64` starts that table entry, so PREINTRO has its sound
    /// also without `BugFix::CinemaSound` (LOGO15N's sound outlasts its
    /// film).
    #[test]
    fn startup_films_play_their_table_sounds() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "startup_films_play_their_table_sounds",
            move |mut e| async move {
                let trace = Rc::new(RefCell::new(Vec::new()));
                e.trace = Some(trace.clone());
                e.set_enhancements(crate::enhancements::Enhancements::original());
                e.play_startup().await;
                let sounds: Vec<(i32, String)> = trace
                    .borrow()
                    .iter()
                    .filter_map(|ev| match ev {
                        Event::NewSound(v, n) => Some((*v, n.to_ascii_lowercase())),
                        _ => None,
                    })
                    .collect();
                *g2.borrow_mut() = sounds;
            },
            |_, _| {},
        );
        if ran {
            assert_eq!(
                *got.borrow(),
                [
                    (0x12, "logo15n.apc".to_string()),
                    (0x13, "preintro.apc".to_string())
                ]
            );
        }
    }
}
