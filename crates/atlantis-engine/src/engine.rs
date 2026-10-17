//! The game: its state, the new-game start and the place loop.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`engine.cpp`
//! `run` / `gameStep`, `place.cpp` `changeChapter` / `setupChapterWAM`;
//! GPLv3, the ScummVM team), which follows `atlantis.exe`'s main loop and
//! place loop `FUN_00423ad8`.
//!
//! The engine is one async task ([`run`]): where the original presents a
//! frame and polls its events, the code awaits [`Sys::frame`].

use std::cell::RefCell;
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
use crate::enhancements::Enhancements;
use crate::hooks::{Departure, DirectLoader, Loader, ScriptHooks};
use crate::look::{WarpMomentum, WarpPointer};
use crate::music::Music;
use crate::persist::{MemStore, SaveStore, Settings};
use crate::scripts::Scripts;
use crate::sys::Sys;
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
    /// The first arrival scan after a chapter load reads ArriveeVue =
    /// LastVue.
    pub(crate) arrival_after_load: bool,
    /// LastVue.
    pub last_vue: u16,
    pub chapter: u32,
    /// The CON clocks (ChapterTime, Time1, Time2).
    pub clocks: ConClocks,
    /// The CON variables.
    pub vars: Vars,
    /// The music and the NewSound sample.
    pub music: Music,
    /// Set when the place loop has to end.
    pub abort: Option<Abort>,
    /// A CON section plays from a click (0x00496170).
    pub section_play: bool,
    /// A script Vue= named a place the WAM does not have: the variable VUE
    /// reads it until the next place change.
    pub vue_override: Option<i32>,
    /// The inventory records (0x006ce138): the object id of each, in
    /// order. The object in hand is OBJETENMAIN.
    pub inventory: Vec<i32>,
    /// The first record the bar shows (0x00496924).
    pub bar_first: usize,
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
    /// The SUBTITLES toggle's click counter (0x00496098).
    pub subtitle_clicks: u8,
    /// The sound menu's knobs, kept for the session (0 = the file's).
    pub son_knob_x: [i32; 4],
    pub(crate) channel_test_voice: Option<crate::audio::VoiceId>,
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
    /// The optional differences from the original game.
    pub enhancements: Enhancements,
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
    /// WSPRFRAME.
    pub wspr_frame: i32,
    /// F9 during navigation: the diagnostic overlay (port-only).
    pub debug_overlay: std::cell::Cell<bool>,
    pub(crate) debug_font: std::cell::OnceCell<crate::subtitles::SubtitleFont>,
    /// 0x00496188 == 1 (a talking-head line): the persos are not drawn.
    pub sprites_hide_npcs: bool,
    /// The SPW files of the place, by name.
    pub(crate) spw_cache: HashMap<String, Rc<Vec<u8>>>,

    // The view.
    pub warp: Warp,
    /// The panorama as loaded, and with the place sprites drawn on it.
    pub(crate) cyclo: Vec<u16>,
    pub(crate) cyclo_h: usize,
    pub(crate) warp_surface: Vec<u16>,
    pub warp_loaded: bool,
    pub(crate) momentum: WarpMomentum,
    pub(crate) pointer: WarpPointer,
    pub(crate) pointer_mouse_mark: (i32, i32),
    pub(crate) cursor_sprites: Vec<SprSprite>,
    pub(crate) cursor_state: WarpCursorState,
    pub(crate) cursor_shown: i32,
    pub(crate) cursor_next_ms: u64,

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

/// The clock `FUN_004320bc` draws from, which redraws while the value &
/// 0x7f is above 100: the original's clock moves while it spins. The
/// engine's clock only moves between frames, so a draw of 101..127 spun
/// forever (the window hung after a conversation, a video, a place
/// change); each further read within one frame reads 1 ms later.
pub(crate) fn spinning_clock(sys: Rc<Sys>) -> impl FnMut() -> u32 {
    let mut last = (u64::MAX, 0u64);
    move || {
        let now = sys.millis();
        last = if last.0 == now {
            (now, last.1 + 1)
        } else {
            (now, 0)
        };
        (now + last.1) as u32
    }
}

impl Engine {
    pub fn new(sys: Rc<Sys>, game: Game, loader: Rc<dyn Loader>) -> Engine {
        let mut ambient = AmbientSounds::new();
        let clock_sys = sys.clone();
        // FUN_004320bc draws from timeGetTime - 0x00bcb5b8 (FUN_00451d0c).
        ambient.set_clock(spinning_clock(clock_sys));
        // The effects volume 0x00495e73 at full scale.
        ambient.effects_volume = 127;
        let ambient_out = MixerAmbientOutput::new(sys.mixer.clone());
        let music = Music::new(&sys);
        Engine {
            sys,
            game,
            loader,
            trace: None,
            wam: Wam::default(),
            wam_name: String::new(),
            current_place_id: None,
            current_place: None,
            next_place_id: None,
            arrival_scan_pending: false,
            arrival_from_vue: 0,
            arrival_after_load: false,
            last_vue: 0,
            chapter: 0,
            clocks: ConClocks::default(),
            vars: Vars::default(),
            music,
            abort: None,
            section_play: false,
            vue_override: None,
            inventory: Vec::new(),
            bar_first: 0,
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
            son_knob_x: [0; 4],
            channel_test_voice: None,
            voice_volume: 255,
            players: Players::new(b"#New Player"),
            player: 0,
            saves: Rc::new(MemStore::new()),
            prev_chapter: 0,
            disc: 1,
            load_running: false,
            place_table_slots: Vec::new(),
            hidden_persos: Vec::new(),
            enhancements: Enhancements::default(),
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
            wspr_frame: 0,
            debug_overlay: std::cell::Cell::new(false),
            debug_font: std::cell::OnceCell::new(),
            sprites_hide_npcs: false,
            spw_cache: HashMap::new(),
            warp: Warp::new(),
            cyclo: Vec::new(),
            cyclo_h: 0,
            warp_surface: Vec::new(),
            warp_loaded: false,
            momentum: WarpMomentum::default(),
            pointer: WarpPointer::default(),
            pointer_mouse_mark: (0, 0),
            cursor_sprites: Vec::new(),
            cursor_state: WarpCursorState::default(),
            cursor_shown: CURSOR_UNSET,
            cursor_next_ms: 0,
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

    /// Makes several members readable at once (one fetch on the web).
    pub async fn prefetch(&self, names: &[String]) {
        self.loader.prefetch(&self.game.big, names).await;
    }

    /// The current place record.
    pub fn place(&self) -> Option<&Place> {
        self.current_place.and_then(|i| self.wam.places.get(i))
    }

    /// The game clock (`FUN_00451d0c`, timeGetTime less its epoch).
    pub fn now(&self) -> u32 {
        self.sys.millis() as u32
    }

    /// CHAPTERTIME: `FUN_0041c75c`, (now - start) / 1000.
    pub fn chapter_time(&self) -> i32 {
        self.clocks.chapter_time(self.now())
    }

    /// The window closed, or the place loop has to end.
    pub fn should_abort(&self) -> bool {
        self.sys.should_quit() || self.abort.is_some()
    }

    /// After the inventory bar (`FUN_0042c59c` @ 0x0042c844), the pick-up
    /// bar and a resumed in-game menu (`FUN_0041c838` @ 0x0041cb84): Time1
    /// and Time2 resume from the values they stood at, ChapterTime starts
    /// again from 0.
    pub fn rebase_con_clocks(&mut self) {
        let (t1, t2) = (self.vars.get("time1"), self.vars.get("time2"));
        let now = self.now();
        self.clocks.rebase(now, t1, t2);
    }

    /// `FUN_0041c75c`, at the start of every warp frame (0x0041c086).
    pub fn update_con_clocks(&mut self) {
        let now = self.now();
        self.vars.set("random", (now & 0xff) as i32);
        self.vars.set("chaptertime", self.clocks.chapter_time(now));
        self.vars.set("time1", self.clocks.timer_value(1, now));
        self.vars.set("time2", self.clocks.timer_value(2, now));
    }

    /// Sets the view angles (an /INIT AngleWarpX / AngleWarpY, a test).
    pub fn set_view(&mut self, alpha: f32, beta: f32) {
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

    /// A WAM file, its centre table and sprite lists (`setupWAMByName`,
    /// `FUN_00421ebc`).
    pub async fn load_wam(&mut self, name: &str) -> bool {
        self.load_wam_with(name, true).await
    }

    /// `record`: the place table slots are written (`setupWAMByName`; the
    /// chapter's own WAM load `setupChapterWAM` does not).
    pub(crate) async fn load_wam_with(&mut self, name: &str, record: bool) -> bool {
        let Some(data) = self.read(&format!("WAM\\{name}")).await else {
            return false;
        };
        self.wam = Wam::parse(&data);
        self.wam_name = name.to_ascii_uppercase();
        if record {
            self.record_place_table();
        }
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
        if !self.load_wam_with(NEW_GAME_WAM, false).await {
            log::error!("cannot start a new game: {NEW_GAME_WAM} missing");
            return;
        }
        self.chapter = 1;
        if !in_play {
            // The record FUN_0042c054 sets up before the player menu: object
            // 257 alone. FUN_00423760 zeroes the timer offsets and restarts
            // Time1 when a game starts.
            self.inventory = vec![257];
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
                self.arrival_scan_pending = false;
                if let (Some(vue), true) =
                    (self.current_place_id, scan && self.current_place.is_some())
                {
                    hooks.arrival_scan(self, vue, arrivee).await;
                }
            } else if self.arrival_scan_pending {
                // A script Vue= already entered the place (@ 0x00423b48).
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
            if !self.warp_loaded {
                // Nowhere to look (a missing panorama): wait for the quit.
                self.sys.frame().await;
                continue;
            }

            let action = self.handle_warp(hooks).await;
            if action == WARP_ABORT {
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
            hooks.zone_click(self, action).await;
            if self.should_abort() {
                break;
            }
            self.vars.set("cliczone", 0);
            if self.arrival_scan_pending {
                continue; // a Vue= in a section entered another place
            }
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
            let outcome = hooks.departure_scan(self, dest).await;
            let game_over = outcome == Departure::GameOver;
            if outcome == Departure::Abort {
                self.arrival_scan_pending = false;
            }
            if self.arrival_scan_pending {
                // A Vue= inside a departure section already entered its
                // place (CHAPI005 /113, CHAPI062 /864, CHAPI077 /010); the
                // place loop goes on with the turn and video of entry i of
                // the new place's record.
                self.next_place_id = None;
                if !self.should_abort() && !game_over {
                    self.transition_after_departure_vue(index).await;
                    self.vars.move_ended(); // 0x00424089
                }
                continue;
            }
            match (outcome, self.next_place_id.take()) {
                (Departure::Go, Some(dest)) if !self.should_abort() => {
                    // A departure that is not aborted stops timer 1 and
                    // restarts Time1 (@ 0x00423e8d).
                    self.vars.set("testtime1", 0);
                    self.clocks.time1_start = self.now();
                    self.clocks.time1_offset = 0;
                    self.execute_transition(dest).await;
                    self.vars.move_ended(); // 0x00424089
                }
                _ => {}
            }
        }
    }

    /// `FUN_00431e98` on the way out (0x004197a0).
    pub fn stop_ambient(&mut self) {
        if self.ambient.active() {
            self.ambient.stop_all(&mut self.ambient_out);
        }
    }
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
    } = opts;
    let mut e = Engine::new(sys, game, loader);
    e.trace = trace;
    e.enhancements = enhancements;
    e.subtitles = subtitles;
    e.saves = saves;
    // The music loops seamlessly with the minor bug fixes; the original's
    // streaming ring otherwise.
    e.music.original_ring = !enhancements.minor_bug_fixes;
    e.load_settings(Settings {
        subtitles,
        enhancements,
        ..Settings::default()
    });
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
    /// cinema pace, a 200-tick (1 s) wait that reads no input between them.
    pub async fn play_startup(&mut self) {
        use crate::place::CINEMA_FRAME_MS;
        self.play_transition_video("LOGO15N.HNM", None, CINEMA_FRAME_MS)
            .await;
        let start = self.sys.millis();
        while self.sys.millis() - start < 1000 && !self.sys.should_quit() {
            while self.sys.poll_event().is_some() {}
            self.sys.frame().await;
        }
        self.play_transition_video("PREINTRO.HNM", None, CINEMA_FRAME_MS)
            .await;
    }

    /// The in-game menu from the warp loop: a reload or a quit ends the
    /// place loop.
    pub(crate) async fn run_ingame_menu(&mut self) {
        // FUN_0041c838 lowers the music while it runs (@ 0x0041c88a) and
        // raises it at the end (@ 0x0041cb7d).
        self.music.duck(false, None);
        let outcome = self.ingame_menu().await;
        self.music.unduck(None);
        match outcome {
            // The clocks resume only when the play goes on: a reload, a
            // quit or an abort leaves them as they are.
            MenuOutcome::Resume if !self.should_abort() => self.rebase_con_clocks(),
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
        let mut clock = spinning_clock(sys.clone());
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
                a.set_clock(spinning_clock(s));
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

    /// The in-game menu ended by an abort (as a reload or a quit) leaves
    /// the clocks alone; a resume rebases them.
    #[test]
    fn ingame_menu_rebases_the_clocks_only_on_resume() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "ingame_menu_rebases_the_clocks_only_on_resume",
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
        assert_eq!(*got, [0, 7]);
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
