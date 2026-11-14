//! The CON script interpreter: the chapter scripts run from navigation.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`con_exec.cpp`, the
//! section walk of `dialog.cpp` `playConSection`, the scans of
//! `engine.cpp` `gameStep` and `navigation.cpp` `handleWarp`; GPLv3, the
//! ScummVM team), which follow `atlantis.exe`: the /set handler
//! `FUN_0042a100`, the section executor `FUN_0042844c`, the scans
//! `FUN_004277c8` (/sel), `FUN_00425cf4` (/con click), `FUN_00425828` (a
//! character clicked), `FUN_0042bef4` / `FUN_0042bfa4` (timers) and the
//! chapter change `FUN_00427634`.
//!
//! [`Scripts`] implements [`ScriptHooks`]. The dialog lines of a section
//! are handed to a [`DialogPresenter`], which shows them (the talking
//! heads, the voices, the subtitles); the walk itself (the /set commands,
//! the track letters, /go, the Escape skip, the game over) is here.

use crate::checkpoint::ScriptState;
use crate::con_script::script::latin1_bytes;
use crate::con_script::{
    apply_con_subject_set, con_alt_game_over_narration, con_angle_radians, con_entry_allowed,
    con_escape_skip, con_game_over_narrator, con_line_buffer_text, con_narrator_buffer_text,
    con_track_letter, evaluate, parse_con_set, vars, ConItem, ConKeyBuffer, ConLine, ConScript,
    ConSection, ConSubjects, ConVariableNames, CON_PLAY_COUNTERS,
};
use crate::engine::{Abort, Engine, Event};
use crate::hooks::{ready, Departure, HookFuture, ScriptHooks};
use atlantis_data::sprite_anim::{
    set_perso_active, start_place_sprite_anim, stop_place_sprite_anim,
};
use atlantis_data::sprite_list::object_pickup_rows;
use std::collections::HashMap;

/// A dialog line to present.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRequest {
    /// The section number: the `<NNNN>` of `wav\ata<NNNN><L>.apc`.
    pub section: i32,
    /// The track letter, the `<L>`.
    pub letter: char,
    pub line: ConLine,
    /// ClicPerso when the line starts: the clicked character for the first
    /// line of a section entered with one, else the speaker's id
    /// (`FUN_00428f58`).
    pub perso: i32,
    /// The section was entered from a /con entry (an interactive dialog).
    pub interactive: bool,
    /// The IncPerso count: the INITDIAL row of a Seth line (0x00428a91).
    pub inc_perso: i32,
}

/// What the presentation of a line tells the walk.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineOutcome {
    /// Escape during a talking head (0x00496134): the following lines up to
    /// the next /set (or the /go, or the end) are skipped, their letters
    /// counted (`FUN_0042844c` @ 0x00428b6d..0x00428bcd).
    pub escape_skip: bool,
    /// The angles the walk sets back at its end, when the line moved them
    /// (a talking head, or a perso of 0x200 and up: 0x004289ef,
    /// 0x00428aa6).
    pub entry_angles: Option<(f32, f32)>,
}

/// The game over to present (`FUN_00426590`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameOverRequest {
    pub section: i32,
    /// GameOver=N.
    pub n: i32,
    /// The narrator's text, with its final blanks (`ConLine::trail`: the
    /// game over copies the line up to the CR too, atlantis-cd-zh-tw-696832
    /// FUN_00425e80 @ 0x004262d4..0x004262ef), and the track letter of its
    /// voice, when the line after the command is a narrator line.
    pub narrator: Option<(String, char)>,
}

/// The presentation of the dialogs: what a section walk hands over. The
/// default methods show nothing and keep the walk's control flow.
#[allow(unused_variables)]
pub trait DialogPresenter {
    /// One dialog line (`FUN_0042ee88` and its callers in `FUN_0042844c`).
    fn play_line<'a>(
        &'a mut self,
        e: &'a mut Engine,
        req: &'a LineRequest,
    ) -> HookFuture<'a, LineOutcome> {
        ready(LineOutcome::default())
    }

    /// The subject menu of a character with enabled subjects
    /// (`FUN_00425828` @ 0x00425930..0x00425a8a): the chosen subject, 0
    /// for an empty cell, `None` for a click outside the grid.
    fn subject_menu<'a>(
        &'a mut self,
        e: &'a mut Engine,
        perso: i32,
        subjects: &'a [u8],
    ) -> HookFuture<'a, Option<i32>> {
        ready(subjects.first().map(|&s| i32::from(s)))
    }

    /// The game-over screen (GereGameOver, `FUN_00426590`).
    fn game_over<'a>(
        &'a mut self,
        e: &'a mut Engine,
        req: &'a GameOverRequest,
    ) -> HookFuture<'a, ()> {
        ready(())
    }
}

/// Dialogs that show nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDialog;

impl DialogPresenter for NoDialog {}

/// `FUN_00428f58`: memcmp of the text after '[' against each literal of
/// the chain, in code order (case-sensitive); every test runs and a hit
/// overwrites the previous result, so the LAST match wins (e.g. "Seth" ->
/// 513 then 514).
pub fn resolve_speaker_perso(speaker: &str) -> i32 {
    const CHAIN: [(&str, i32); 68] = [
        ("Agat", 520),
        ("agat", 520),
        ("Lasc", 521),
        ("lasc", 521),
        ("Melj", 522),
        ("melj", 522),
        ("Sorc", 567),
        ("sorc", 567),
        ("Gard", 604),
        ("gard", 604),
        ("Sold", 604),
        ("Pech", 524),
        ("pech", 524),
        ("Acty", 524),
        ("acty", 524),
        ("Creo", 525),
        ("creo", 525),
        ("Chas", 540),
        ("chas", 540),
        ("Passant9", 602),
        ("passant9", 602),
        ("Carv", 602),
        ("carv", 602),
        ("Passant6", 601),
        ("passant6", 601),
        ("Passante7", 606),
        ("passante7", 606),
        ("Hect", 543),
        ("hect", 543),
        ("Cuis", 551),
        ("cuis", 551),
        ("Seth", 513),
        ("seth", 513),
        ("Seth", 514),
        ("seth", 514),
        ("Rein", 560),
        ("rein", 560),
        ("Reinem", 561),
        ("reinem", 561),
        ("Reinep", 562),
        ("reinep", 562),
        ("Garc", 526),
        ("garc", 526),
        ("Servan", 609),
        ("servan", 609),
        ("Servag", 582),
        ("servag", 582),
        ("Chie", 650),
        ("chie", 650),
        ("Anna", 663),
        ("anna", 663),
        ("Escl", 597),
        ("escl", 597),
        ("Enne", 667),
        ("Enne", 667),
        ("Hona", 682),
        ("hona", 682),
        ("Sama", 683),
        ("sama", 683),
        ("Naka", 684),
        ("naka", 684),
        ("Lona", 576),
        ("lona", 576),
        ("JPil", 571),
        ("jpil", 571),
        ("Gimb", 563),
        ("gimb", 563),
        ("", 0),
    ];
    let mut perso = 0;
    for (lit, id) in CHAIN {
        if !lit.is_empty() && speaker.starts_with(lit) {
            perso = id;
        }
    }
    perso
}

/// The state a walk carries through its /go chain: the view angles and
/// ClicPerso of its start, set back at its end.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Walk {
    entry: (f32, f32),
    clic_perso: i32,
}

/// The inventory's capacity: 30 records (0x006ce138, 0x10e bytes).
const INVENTORY_RECORDS: usize = 30;

/// The game's scripts.
pub struct Scripts {
    pub names: ConVariableNames,
    pub script: ConScript,
    /// The chapter of the loaded CON (0 before the first).
    pub chapter: u32,
    pub subjects: ConSubjects,
    /// The /con play counters (0x006cc6cc).
    pub counters: Vec<u8>,
    pub dialog: Box<dyn DialogPresenter>,
    /// The Seth row count of IncPerso (0x0077290c's offset).
    pub inc_perso: i32,
    /// The persistent variables of a checkpoint being loaded, set once
    /// VARIAS.CON is read, before the /INIT block.
    pending_vars: Option<[i32; 7]>,
    scan_key: ConKeyBuffer,
    /// The /INIT block runs (the loading flag 0x00496168).
    init_running: bool,
    /// The place loop's own arrival scan runs.
    in_place_arrival_scan: bool,
    /// A timer section plays (0x00496174).
    timer_play: bool,
    /// The walks of a chapter's /INIT commands so far (for tests and
    /// tools).
    pub init_walks: u32,
    /// 0x6cccac[section]: the 1999 build's game-over counters.
    game_over_count: HashMap<i32, u8>,
}

impl Default for Scripts {
    fn default() -> Self {
        Scripts::new()
    }
}

impl Scripts {
    pub fn new() -> Scripts {
        Scripts::with_dialog(Box::new(NoDialog))
    }

    pub fn with_dialog(dialog: Box<dyn DialogPresenter>) -> Scripts {
        Scripts {
            names: ConVariableNames::default(),
            script: ConScript::default(),
            chapter: 0,
            subjects: ConSubjects::default(),
            counters: vec![0; CON_PLAY_COUNTERS],
            dialog,
            inc_perso: 0,
            pending_vars: None,
            scan_key: [0; 16],
            init_running: false,
            in_place_arrival_scan: false,
            timer_play: false,
            init_walks: 0,
            game_over_count: HashMap::new(),
        }
    }

    /// A scan starts with its key buffer zeroed. With the restored content
    /// each evaluation starts from a clear key buffer (no buffer passed),
    /// so INVENTnnn is never extended by an earlier key and CHAPI063's Anna
    /// timer lines can play; the original keeps the buffer across a scan.
    fn begin_scan(&mut self) {
        self.scan_key = [0; 16];
    }

    /// The value of a variable as the conditions read it: VUE from the
    /// place shown, the others stored.
    fn value(e: &Engine, name: &str) -> i32 {
        match name {
            "vue" => e
                .vue_override
                .unwrap_or_else(|| e.current_place_id.map_or(-1, i32::from)),
            _ => e.vars.get(name),
        }
    }

    /// A play of section `id` counted: a byte, wrapping after 255 plays.
    fn bump_counter(&mut self, id: i32) {
        match usize::try_from(id)
            .ok()
            .and_then(|i| self.counters.get_mut(i))
        {
            Some(c) => *c = c.wrapping_add(1),
            None => log::warn!("play counter of section {id} out of range"),
        }
    }

    /// `FUN_00427634` from its CON load on: ChapterTime restarts, the
    /// variables but the persistent ones are cleared (OBJETENMAIN with
    /// them), the subject table too; the file is parsed and its /INIT block
    /// run; Time2 restarts. The hidden-perso list is kept: a chapter's
    /// /INIT holds only the persos it newly shows or hides.
    pub fn load_con_script<'a>(
        &'a mut self,
        e: &'a mut Engine,
        chapter: u32,
    ) -> HookFuture<'a, ()> {
        Box::pin(async move {
            e.clocks.chapter_start = e.now();
            self.script = ConScript::default();
            e.vars.chapter_reset();
            self.subjects.clear();
            if self.names.is_empty() {
                match e.read("SCENAR\\VARIAS.CON").await {
                    Some(d) => self.names = ConVariableNames::load(&d),
                    None => log::warn!("no VARIAS.CON"),
                }
            }
            // A checkpoint's variables 0..6 replace the persistent ones
            // (FUN_0042de5c reads them before the chapter change).
            if let Some(v) = self.pending_vars.take() {
                for (i, &value) in v.iter().enumerate().take(self.names.len()) {
                    let name = self.names.name(i).to_string();
                    e.vars.set(&name, value);
                }
                // FUN_0042de5c step 4, after the reads: ZOOM = NEWSOUND = 0
                // (0x0042e2b5 / 0x0042e2d1) and the last NewSound 0x006cd87c
                // = 0, before the chapter change polls NewSound. A NewSound
                // the checkpoint holds (CHAPI044 /010: NewSound=67 then
                // Chapter=45) is never played on a reload.
                e.vars.set("zoom", 0);
                e.vars.set("newsound", 0);
                e.zoom = false;
                e.music.new_sound_last = 0;
            }
            let name = con_file_name(e, chapter);
            let Some(data) = e.read(&name).await else {
                // (atlantis-dvd-ja-696832: no CHAPJ010 / CHAPJ086, chapters
                // no script reaches; FUN_00427070 would show its error box,
                // code 0xe, and keep the buffer.)
                log::warn!("no CON file {name}");
                return;
            };
            self.script = ConScript::parse(&data);
            log::info!("{name}: {} sections", self.script.sections().len());
            self.exec_init(e).await;
            // FUN_00427634 restarts Time2 after the /INIT block (0x0042779e).
            e.clocks.time2_start = e.now();
            e.clocks.time2_offset = 0;
            e.emit(Event::ChapterLoaded(chapter));
        })
    }

    /// The Japanese DVD build's SUBTITLES click (FUN_0041d768 @ 0x0041d833):
    /// the chapter's CON read again, `CHAPJ` or `CHAPI` as the language
    /// counter now stands (FUN_00426df4: the file into the script buffer,
    /// nothing else; the sections are looked up in the text as they are
    /// run). A failed read (no chapter's script has one) leaves the text:
    /// the original shows its error box (code 0xe) and leaves the options.
    pub async fn reload_con_text(&mut self, e: &mut Engine) {
        if self.chapter == 0 {
            return;
        }
        let name = con_file_name(e, self.chapter);
        match e.read(&name).await {
            Some(data) => {
                self.script = ConScript::parse(&data);
                e.emit(Event::ConTextReloaded(name));
            }
            None => log::warn!("no CON file {name}: the text kept"),
        }
    }

    /// `FUN_00425eb0`: the /INIT commands, with the loading flag set. Every
    /// command runs: the loop has no exit test (no shipped /INIT holds a
    /// Chapter= or a GameOver=).
    async fn exec_init(&mut self, e: &mut Engine) {
        let was = std::mem::replace(&mut self.init_running, true);
        self.init_walks += 1;
        let cmds = self.script.init_cmds().to_vec();
        for (cmd, offset) in cmds {
            self.exec_set(e, Some(offset), &cmd).await;
        }
        self.init_running = was;
        // FUN_00425eb0 ends with FUN_00428da8 (@ 0x00425f4c), once.
        after_item(e).await;
    }

    /// `FUN_0042a100`: a /set command, read from the CON text at `offset`
    /// (the "/set") so that a value search running past the line sees the
    /// same bytes; `None`: `cmd` alone (a tool's command).
    pub fn exec_set<'a>(
        &'a mut self,
        e: &'a mut Engine,
        offset: Option<u32>,
        cmd: &'a str,
    ) -> HookFuture<'a, ()> {
        Box::pin(async move {
            let own = match offset {
                Some(_) => Vec::new(),
                None => {
                    let mut t = b"/set(".to_vec();
                    t.extend(crate::con_script::script::latin1_bytes(cmd));
                    t.extend_from_slice(b")\r\n");
                    t
                }
            };
            // The text the command is read from (the list commands read on
            // from the value).
            macro_rules! text {
                () => {
                    if offset.is_some() {
                        self.script.text()
                    } else {
                        &own[..]
                    }
                };
            }
            let Some(set) = parse_con_set(&self.names, text!(), offset.unwrap_or(0)) else {
                return;
            };
            e.emit(Event::Set(cmd.to_string()));

            // ChangeWam=name: load "wam\name" and make its first place the
            // current one (@ 0x0042a1f8..0x0042a2b6). Every ChangeWam of the
            // game is followed by a Vue= in the same section, which moves
            // the player; until then no place is entered.
            if let Some(wam) = set.wam_name {
                e.set_wam_path(&latin1_bytes(&wam));
                e.load_wam(&wam).await;
                e.current_place = None;
                e.current_place_id = e.wam.places.first().map(|p| p.id);
                return;
            }
            // INVENTn=v: v != 0 appends object n (no duplicate check), v == 0
            // removes its first record (FUN_0042b328 / FUN_0042b3ec); the
            // hand keeps the object. An object given is shown under the
            // talking heads (0x0049619c = its OBJETS1.SPR sprite, written by
            // FUN_0042b328 @ 0x0042b3a2).
            if let Some((id, value)) = set.invent {
                if value != 0 {
                    if let Some(k) = e
                        .game
                        .exe
                        .inventory_object_ids
                        .iter()
                        .position(|&o| o == id)
                    {
                        e.given_object = Some(k);
                    }
                    if e.inventory.len() < INVENTORY_RECORDS {
                        e.inventory.push(id);
                    }
                } else if let Some(k) = e.inventory.iter().position(|&o| o == id) {
                    e.inventory.remove(k);
                }
                return;
            }

            let key = set.name.as_str();
            let value = set.value;
            match key {
                // EnableSuj,perso=suj,... / DisableSuj,perso=suj,...
                // (@ 0x0042a4ef..0x0042a560) on the text from the first digit.
                "enablesuj" | "disablesuj" => {
                    apply_con_subject_set(&mut self.subjects, &set, text!());
                }
                // Time1=N / Time2=N restart the timer at N seconds; the
                // variable is written by the clock update (@ 0x0042a321,
                // 0x0042a392).
                "time1" | "time2" => {
                    let now = e.now();
                    e.clocks
                        .restart(if key == "time1" { 1 } else { 2 }, value, now);
                }
                // Disable=N counts a play of section N, which disables /con
                // N (@ 0x0042a41a); the variable is not stored.
                "disable" => self.bump_counter(value),
                "vue" => self.enter_place_from_script(e, value).await,
                "chapter" => self.change_chapter(e, value).await,
                // The trigger commands act only for a value other than 0
                // (the store, then `cmp [var],0 / je`: FUN_0042a100 @
                // 0x0042ac44..0x0042b1d0): StartMusik, StopMusik, LoadMusik,
                // ShowPerso and StopSound with 0 do nothing.
                "startmusik" | "stopmusik" | "loadmusik" | "showperso" | "stopsound"
                    if value == 0 =>
                {
                    e.vars.set(key, 0);
                }
                "startmusik" => e.music_start_command(value).await,
                "stopmusik" => e.music_stop_command().await,
                "loadmusik" | "delmusik" => {
                    e.music_list_command(key == "loadmusik", text!(), set.value_pos)
                        .await;
                }
                // Cinema=N first puts 0x00496124 = 1 and 0x00496138 = 0
                // (0x0042a775, 0x0042a77f): an Escape latch left by a puzzle
                // or the subject menu does not skip the film.
                "cinema" => {
                    e.escape_request = false;
                    e.play_cinema(value).await;
                }
                // ChangeCd=n: the disc prompt for disc n (@ 0x0042a4c4:
                // FUN_0041eda8((n - 1) & 0xff), which clamps the disc index
                // to 3). The port reads every disc at once: no prompt, it
                // only notes the disc, which the checkpoints keep. The
                // prompt's function also empties the hand (OBJETENMAIN = 0 @
                // 0x0041ede9) and ends by clearing the input flags, the bar
                // request among them (0x0041f032..0x0041f064).
                "changecd" => {
                    e.disc = change_cd_disc(value);
                    e.vars.set("objetenmain", 0);
                    e.bar.flags_reset();
                }
                // HideGuest / ShowGuest store no variable.
                "hideguest" | "showguest" => e.guests_shown = key == "showguest",
                // AddGuest / SubGuest (FUN_0042b480 / FUN_0042b6c0): the
                // record and its portrait sliding in or out, then the
                // variable 0.
                "addguest" => {
                    e.add_guest(value).await;
                    e.vars.set(key, 0);
                }
                "subguest" => {
                    e.sub_guest(value).await;
                    e.vars.set(key, 0);
                }
                "hideperso" => {
                    if value > 0 {
                        hide_perso(e, value);
                    }
                }
                "showperso" => show_perso(e, value),
                // IncPerso=N (N != 0): the Seth row count steps, and returns
                // to 0 when N is 99; the variable returns to 0 (@ 0x0042b07a).
                "incperso" => {
                    if value != 0 {
                        self.inc_perso += 1;
                        if value == 99 {
                            self.inc_perso = 0;
                        }
                    }
                    e.vars.set(key, 0);
                }
                // NEWWSPRANIM=N starts the current view's entry N,
                // STOPWSPRANIM=N stops it (and clears WSPRFRAME); the variable
                // returns to 0 (FUN_00428da8).
                "newwspranim" | "stopwspranim" => {
                    if let Some(vue) = e
                        .current_place_id
                        .filter(|&v| value != 0 && usize::from(v) < e.sprite_anim_current.len())
                    {
                        let ok = if key == "newwspranim" {
                            start_place_sprite_anim(
                                &mut e.sprite_list,
                                vue,
                                value,
                                &mut e.sprite_anim_current[usize::from(vue)],
                            )
                        } else {
                            stop_place_sprite_anim(&mut e.sprite_list, vue, value)
                        };
                        if !ok {
                            log::warn!("{key}={value}: no such sprite in view {vue}");
                        }
                        if key == "stopwspranim" {
                            e.vars.set("wsprframe", 0);
                            e.wspr_frame = 0;
                        }
                        e.recomposite_sprite_layer();
                    }
                    e.vars.set(key, 0);
                }
                // NewSound=N: the value is stored and nothing more
                // (FUN_0042a100); the poller FUN_00432a64, run after every
                // executed item (FUN_00428da8, see `after_item`) and at the
                // end of every frame (FUN_00432178), starts the sound.
                "newsound" => e.vars.set(key, value),
                // StopSound: the NewSound sample stops, whatever N but 0.
                // FUN_0042b290 also clears the poller's playing flag
                // 0x00772912 (@ 0x0042b2d8) but not the last value
                // 0x006cd87c, so the next non-zero NewSound starts, the same
                // value included.
                "stopsound" => e.stop_sound_command(),
                // ArcMagique=N: the magic-bow animation (@ 0x0042aa09), then
                // the store.
                "arcmagique" => {
                    e.play_arc_magique().await;
                    e.vars.set(key, value);
                }
                // TirFleche=N restarts the arrow frame (0x0042a98e), then the
                // store.
                "tirfleche" => {
                    e.arrow_frame = 0;
                    e.vars.set(key, value);
                }
                // StartPuzzle=N only arms puzzle N: it runs when zone 1000 + N
                // is clicked (FUN_004240fc).
                _ => {
                    // AngleWarpX=N / AngleWarpY=N turn the view to N degrees,
                    // then store the variable (@ 0x0042a995, 0x0042a9cf).
                    if key == "anglewarpx" {
                        e.warp.set_alpha(con_angle_radians(value));
                    } else if key == "anglewarpy" {
                        e.warp.set_beta(con_angle_radians(value));
                    }
                    e.vars.set(key, value);
                }
            }
        })
    }

    /// Chapter=N: the CON of chapter N replaces the current one, then the
    /// /sel scan runs with ArriveeVue = LastVue (`FUN_00427634` sets it at
    /// 0x0042777d, `FUN_0042a100` scans at 0x0042a60e).
    async fn change_chapter(&mut self, e: &mut Engine, value: i32) {
        let left = e.vars.get("chapter");
        e.vars.set("chapter", value);
        if value <= 0 || value as u32 == self.chapter {
            return;
        }
        log::info!("chapter {} -> {value}", self.chapter);
        // The previous chapter (0x00772907 @ 0x0042a582), the chapter byte
        // (0x00772909 @ 0x0042a58a).
        e.prev_chapter = self.chapter as u8;
        self.chapter = value as u32;
        e.chapter = value as u32;
        // The chapter checkpoint, written first (0x0042765f) unless the
        // /INIT block or a load runs (0x00496168): it holds the state where
        // chapter= fired, with the new chapter number and, in variable 0,
        // the chapter left (the variable is stored after it).
        if !self.init_running && !e.load_running {
            let state = self.script_state(e, left);
            e.write_checkpoint(&state);
        }
        self.load_con_script(e, value as u32).await;
        if let Some(cur) = e.current_place_id {
            let last = i32::from(e.last_vue);
            // FUN_004277c8 runs the executor for the /sel entry found; a
            // clicked or a timer section's flag (0x00496170 / 0x00496174)
            // is still 1, so that entry counts a play (0x00428493).
            let counted = e.section_play || self.timer_play;
            let played = self
                .run_arrival_sel_counted(e, i32::from(cur), last, counted)
                .await;
            // With no /sel entry found FUN_004277c8 returns the start of the
            // new CON text (0x00427800), which the executor takes as its next
            // position (0x00428bf9..0x00428c02): it walks the file's head up
            // to the first section header, so the /INIT commands run again,
            // the loading flag now clear (chapters 27, 33 and 36).
            if !played && !self.init_running && !e.should_abort() {
                self.init_walks += 1;
                let cmds = self.script.init_cmds().to_vec();
                for (cmd, offset) in cmds {
                    self.exec_set(e, Some(offset), &cmd).await;
                    if e.should_abort() || e.vars.get("gameover") != 0 {
                        break;
                    }
                    // The executor's walk: FUN_00428da8 after the item.
                    after_item(e).await;
                }
            }
        }
        // ARRIVEEVUE stays = LastVue: only the place loop clears it, before
        // each warp loop (@ 0x00423be7, after its own arrival scan). A
        // chapter= run anywhere else in the pass therefore gets a second
        // arrival scan on the next pass (@ 0x00423b5a..0x00423b78) when
        // LastVue != 0, unless a move replaces it (0x00423def); until then
        // the scans read ArriveeVue = LastVue (the departure scan of an exit
        // click after a timer's chapter=).
        if !self.in_place_arrival_scan
            && !self.init_running
            && e.last_vue != 0
            && e.current_place_id.is_some()
        {
            e.arrival_scan_pending = true;
            e.arrival_from_vue = e.last_vue;
        }
    }

    /// Vue=N (@ 0x0042a630..0x0042a749): the sub-WAM swap `FUN_00422ad0`
    /// (current place, N), the place load `FUN_00421b4c` without a
    /// transition video, ArriveeVue = the previous place, ClicPerso = 0.
    /// The rest of the section then plays in the new place; the place loop
    /// runs the arrival scan when it comes round (ArriveeVue != 0 @
    /// 0x00423b48), unless the Vue= ran inside that very scan: ArriveeVue
    /// is cleared right after it (@ 0x00423be7).
    async fn enter_place_from_script(&mut self, e: &mut Engine, value: i32) {
        let Ok(place) = u16::try_from(value) else {
            log::warn!("Vue={value}: not a place");
            return;
        };
        // The place loop's zone 0x006cd7f4 = N (@ 0x0042a659).
        e.script_vue_place = Some(place);
        let from = e.current_place_id;
        if let Some(sub) = e
            .wam
            .sub_wams
            .iter()
            .find(|s| Some(s.src_place) == from && s.dst_place == place)
            .cloned()
        {
            // FUN_00422ad0 @ 0x00422b4c: the path from the record's name.
            e.set_wam_path(&latin1_bytes(&sub.wam));
            e.load_wam(&sub.wam).await;
            e.current_place = None;
        }
        // LastVue = the place being left, unless loading (0x0042a6a7..
        // 0x0042a6b5).
        if !self.init_running {
            e.last_vue = from.unwrap_or(0);
        }
        e.next_place_id = Some(place);
        e.do_place_change().await;
        if e.current_place_id == Some(place) {
            // FUN_00421cfc (0x0042a66d).
            e.vars.place_looked_up(value);
        } else {
            // The place not found: `Vue` = N anyway.
            e.vue_override = Some(value);
        }
        let from = from.unwrap_or(0);
        e.vars.set("arriveevue", i32::from(from));
        e.vars.set("clicperso", 0);
        e.arrival_scan_pending = vars::vue_queues_arrival_scan(self.in_place_arrival_scan);
        e.arrival_from_vue = from;
    }

    /// `FUN_004277c8`: the first /sel entry whose condition holds runs.
    async fn run_sel_scan(&mut self, e: &mut Engine) {
        self.run_sel_scan_counted(e, false).await;
    }

    /// The /sel scan; `counted`: the executor counts the play of the section
    /// it runs (0x00428493: entered while 0x00496170 or 0x00496174 is 1).
    /// True when a section played.
    async fn run_sel_scan_counted(&mut self, e: &mut Engine, counted: bool) -> bool {
        self.begin_scan();
        let restored = e.enhancements().restored_content;
        let found = self.script.find_sel_entry(|sec| {
            let key = (!restored).then_some(&mut self.scan_key);
            eval_entry(&self.names, key, self.script.text(), e, sec)
        });
        let sec = found.map(|i| self.script.sections()[i].clone());
        let Some(sec) = sec else {
            return false;
        };
        if counted {
            self.bump_counter(sec.id);
        }
        self.play_section(e, sec, "sel", None).await;
        true
    }

    /// The arrival scan: ArriveeVue = the place arrived FROM (a negative
    /// origin is no place yet: the arrived place stands for it), then the
    /// /sel scan (`FUN_004277c8`, which leaves ArriveeVue as it is: the place
    /// loop clears it before the warp loop, @ 0x00423be7). True when a
    /// section played.
    async fn run_arrival_sel(&mut self, e: &mut Engine, arrived: i32, from: i32) -> bool {
        self.run_arrival_sel_counted(e, arrived, from, false).await
    }

    async fn run_arrival_sel_counted(
        &mut self,
        e: &mut Engine,
        arrived: i32,
        from: i32,
        counted: bool,
    ) -> bool {
        e.vars
            .set("arriveevue", if from >= 0 { from } else { arrived });
        self.run_sel_scan_counted(e, counted).await
    }

    /// The timer scans (`FUN_0042bef4` / `FUN_0042bfa4`): with TESTTIME1 /
    /// TESTTIME2 set, the first /tim entry whose condition holds plays; a
    /// timer run counts as a play (flag 0x00496174). True when a section
    /// played.
    pub async fn run_con_timers(&mut self, e: &mut Engine, from_warp: bool) -> bool {
        let mut played = false;
        for timer in 1..=2 {
            let flag = if timer == 1 { "testtime1" } else { "testtime2" };
            if e.vars.get(flag) == 0 {
                continue;
            }
            self.begin_scan();
            let restored = e.enhancements().restored_content;
            let found = self.script.sections().iter().position(|sec| {
                let key = (!restored).then_some(&mut self.scan_key);
                sec.is_entry
                    && sec.is_timer
                    && sec.timer_num == timer
                    && eval_entry(&self.names, key, self.script.text(), e, sec)
            });
            let sec = found.map(|i| self.script.sections()[i].clone());
            if let Some(sec) = sec {
                self.bump_counter(sec.id);
                // The warp's mouse-look ends for the dialog; the mouse at the
                // warp pointer, the section's motions set it.
                e.sys.set_mouse_locked(false);
                if from_warp {
                    e.sys.set_mouse_pos(e.pointer.pos.0, e.pointer.pos.1);
                    e.mark_pointer_mouse();
                }
                let kind = if timer == 1 { "tim1" } else { "tim2" };
                let was = std::mem::replace(&mut self.timer_play, true);
                self.play_section(e, sec, kind, None).await;
                self.timer_play = was;
                played = true;
            }
            if e.should_abort() {
                break;
            }
        }
        played
    }

    /// `FUN_00425cf4`, then `FUN_00425dd0`: clicking zone 2000+N with
    /// object N in hand puts the object back into the place when the
    /// place's sprite list holds it. 0x00496170 is set around the scan, the
    /// play and the put-back (0x00425d2c..0x00425d82) for every caller (a
    /// character with an object in hand, an object click, a zone click): the
    /// executor counts the play (0x00428493) and `FUN_0041a298` ignores the
    /// right press's bar toggle (0x0041a305) during the section.
    async fn run_con_click(&mut self, e: &mut Engine) {
        e.section_play = true;
        self.begin_scan();
        let restored = e.enhancements().restored_content;
        let found = self.script.find_con_click_entry(&self.counters, |sec| {
            let key = (!restored).then_some(&mut self.scan_key);
            eval_entry(&self.names, key, self.script.text(), e, sec)
        });
        let sec = found.map(|i| self.script.sections()[i].clone());
        if let Some(sec) = sec {
            self.bump_counter(sec.id);
            self.play_section(e, sec, "con", None).await;
        }

        let held = e.vars.get("objetenmain");
        if held != 0 && e.vars.get("cliczone") - 2000 == held {
            let here = e.current_place_id;
            if e.sprite_list
                .iter()
                .any(|s| Some(s.place_id) == here && s.perso_id == held)
            {
                show_perso(e, held);
                if let Some(k) = e.inventory.iter().position(|&o| o == held) {
                    e.inventory.remove(k);
                }
                e.vars.set("objetenmain", 0);
            }
        }
        e.section_play = false; // 0x00425d82
        e.vars.set("clicperso", 0);
        e.vars.set("cliczone", 0);
    }

    /// `FUN_004278a0` for an object (0x100..0x1ff): ClicPerso = id, the
    /// /con click scan, nothing more once GameOver is set, then the pick-up
    /// of every row of the view with that id.
    async fn click_object(&mut self, e: &mut Engine, id: i32) {
        e.vars.set("clicperso", id);
        self.run_con_click(e).await;
        if e.should_abort() || e.vars.get("gameover") != 0 {
            return;
        }
        let Some(vue) = e.current_place_id else {
            return;
        };
        for r in object_pickup_rows(&e.sprite_list, vue, id) {
            let obj = e.sprite_list[r].perso_id;
            hide_perso(e, obj);
            e.vars.set("objetenmain", obj);
            if e.inventory.len() < INVENTORY_RECORDS {
                e.inventory.push(obj);
            } else {
                log::warn!("inventory full, object {obj} not added");
            }
            // The bar shows the object in hand (FUN_0042c2c4, FUN_0042cc98,
            // FUN_0042c59c): a right click keeps it in hand, a left click
            // puts it away.
            let keep = e.display_pickup(self).await;
            if e.should_abort() {
                return;
            }
            if !keep {
                e.vars.set("objetenmain", 0);
            }
        }
    }

    /// `FUN_00425828`: a character clicked.
    async fn interact_npc(&mut self, e: &mut Engine, perso: i32) {
        e.vars.set("clicperso", perso);
        // @ 0x00425867..0x004258b7: with an object in hand there is no
        // conversation; the /con click scan runs (FUN_00425cf4 sets
        // 0x00496170 around its scan and play) and the object leaves the
        // hand. The return at 0x004258b7 skips the followers' save, hide and
        // restore and leaves 0x00496144 (Guests::talk) as it is.
        if e.vars.get("objetenmain") != 0 {
            // 0x004960c0 = 1 around the scan (@ 0x00425882, 0x004258ad):
            // the still frames of its voice-only lines have no cursor.
            e.give_scan = true;
            self.run_con_click(e).await;
            e.give_scan = false;
            e.vars.set("objetenmain", 0);
            return;
        }
        // @ 0x004258c4: 0x00496170 = 1 up to its end (0x00425b7a).
        e.section_play = true;
        // 0x004258ce..0x004258fe: the followers' sprites are saved, and
        // restored at the end when 0x00496144 is set (0x00425ba0..0x00425bdb),
        // which clears it.
        let saved_guests = e.guests.sprite;
        let end_follower_talk = |e: &mut Engine| {
            if e.guests.talk {
                e.guests.sprite = saved_guests;
                e.guests.talk = false;
            }
        };

        // With enabled subjects the menu lists them in table order and is
        // shown first (0x00425930..0x00425a8a). Then the eligible /con
        // entries are walked in file order (FUN_004281f8) and the first
        // that fits plays (0x00425abe..0x00425b75): an entry whose body does
        // not start with /Suj or /suj always fits (a /suj line further down
        // is not a gate, ConSection::suj_ids); one that does fits when the
        // perso has no subjects, or when its list holds the chosen subject
        // (FUN_004264a8); a click outside the grid stops the walk at the
        // first /suj entry; an empty grid cell gives subject 0, which no
        // list holds.
        let sujs: Vec<u8> = self.subjects.subjects(perso).to_vec();
        let menu = !sujs.is_empty();
        let mut chosen = None;
        if menu {
            // 0x00425944..0x004259b4: with 0x00496144 set, every follower but
            // the clicked perso is hidden, for the menu and the entry's play.
            if e.guests.talk {
                for i in 0..e.guests.count.min(crate::ui::Guests::SLOTS) {
                    if e.guests.id[i] != perso {
                        e.guests.sprite[i] = crate::ui::Guests::HIDDEN;
                    }
                }
            }
            chosen = self.dialog.subject_menu(e, perso, &sujs).await;
            if e.should_abort() {
                e.section_play = false;
                e.escape_request = false;
                end_follower_talk(e);
                return;
            }
        }
        self.begin_scan();
        let restored = e.enhancements().restored_content;
        let mut play = None;
        for sec in self.script.sections() {
            let key = (!restored).then_some(&mut self.scan_key);
            if !sec.is_con
                || !con_entry_allowed(sec, &self.counters)
                || !eval_entry(&self.names, key, self.script.text(), e, sec)
            {
                continue;
            }
            if sec.suj_ids.is_empty() || !menu {
                play = Some(sec.clone());
                break;
            }
            let Some(chosen) = chosen else { break };
            let fits =
                sec.suj_ids.contains(&chosen) && sujs.iter().any(|&s| i32::from(s) == chosen);
            if fits {
                play = Some(sec.clone());
                break;
            }
        }
        match play {
            None => log::debug!("perso {perso}: nothing to play (subject {chosen:?})"),
            Some(sec) if !e.should_abort() => {
                // The executor counts the play when it is entered from a /con
                // click (FUN_0042844c @ 0x00428493).
                self.bump_counter(sec.id);
                self.play_section(e, sec, "con", None).await;
            }
            _ => {}
        }
        e.section_play = false;
        // FUN_00425828's end puts 0x00496124 and the menu request back
        // (0x00425be5, 0x00425bef).
        e.escape_request = false;
        e.vars.set("clicperso", 0);
        end_follower_talk(e);
    }

    /// `FUN_004240fc`: FinPuzzle = the puzzle's result, the view is
    /// reloaded (the place's panorama, the pointer at (0x140, 0x140), its
    /// sprites back to frame 0), StartPuzzle = 0, one /sel scan, then
    /// FinPuzzle = 0. It
    /// first stops the ambient sounds (FUN_00431e98 @ 0x0042417e); around
    /// it the music goes down by half (0x00495ea4 = 50, 0x00495eac = 2),
    /// then back up with 80 % again.
    async fn run_puzzle_zone(&mut self, e: &mut Engine, n: u32) {
        e.stop_ambient();
        e.music.duck(false, Some(50));
        // FinPuzzle = the result (crate::puzzle, FUN_0043a1e0).
        crate::puzzle::play_puzzle(e, self, n as i32).await;
        e.music.unduck(Some(80));
        if e.should_abort() {
            return;
        }
        // The bar request a right press in the puzzle toggled is cleared
        // (0x004241d3); 0x00496120 stays, and so does an Escape latch
        // (0x00496124 = 0, 0x00496138 = 1, crate::puzzle::assets::poll_flags):
        // no puzzle function resets it, so the first voice-only line or
        // game-over narration of the scan stops at its first poll (with
        // BugFix::PuzzleEscape a puzzle latches no Escape).
        e.bar.request = false;
        e.reload_current_view().await;
        e.vars.set("startpuzzle", 0);
        self.run_sel_scan(e).await;
        e.vars.set("finpuzzle", 0);
        // Only now 0x00496124 = 1 (0x00424224), 0x0049611c = 0 and
        // 0x00496138 = 0 (0x00424238).
        e.escape_request = false;
    }

    /// The section executor `FUN_0042844c`: the section's /set commands and
    /// dialog lines in file order, then its /go, which continues the same
    /// walk (`FUN_00425c04` returns the target header to the executor).
    pub(crate) fn play_section<'a>(
        &'a mut self,
        e: &'a mut Engine,
        sec: ConSection,
        kind: &'static str,
        chain: Option<Walk>,
    ) -> HookFuture<'a, ()> {
        Box::pin(async move {
            // The gesture that triggered the section is drained, so that the
            // first line is not skipped by a click still held.
            e.wait_buttons_up().await;
            let entry_chapter = self.chapter;
            e.emit(Event::Section {
                chapter: self.chapter,
                id: sec.id,
                kind,
            });
            // The executor saves ClicPerso (0x004284c2) and the view angles
            // (0x00428483 / 0x0042848b) at entry and sets them back at the
            // end (0x00428cb2); in between ClicPerso is cleared after every
            // line (0x00428b39). A /go keeps both.
            let mut walk = chain.unwrap_or(Walk {
                entry: (e.warp.alpha(), e.warp.beta()),
                clic_perso: e.vars.get("clicperso"),
            });
            // GameOver=N, N != 0, ends the walk (the executor breaks its line
            // loop once the variable goes non-zero); the line after it feeds
            // the game-over screen.
            let mut game_over = None;
            let mut line_idx = 0usize;
            let mut i = 0;
            while i < sec.items.len() {
                if e.should_abort() {
                    break;
                }
                match &sec.items[i] {
                    ConItem::Set { cmd, offset } => {
                        let n = gameover_value(cmd);
                        self.exec_set(e, Some(*offset), cmd).await;
                        if n != 0 {
                            game_over = Some(self.game_over_at(e, &sec, i, line_idx, n));
                            break;
                        }
                        // A chapter change replaced the script: the walk ends.
                        // The executor still ends that item with FUN_00428da8
                        // (only GameOver breaks before it, 0x00428bf9..).
                        if self.chapter != entry_chapter {
                            // The handler's result is the executor's new
                            // text position (0x00428bf9..0x00428c02), and the
                            // GAMEOVER test (0x00428c05) still comes before
                            // FUN_00428da8: a GAMEOVER left set
                            // (BugFix::GameOverPuzzle off) shows the screen
                            // again, with no narrator, the position being a
                            // header or the new file's head [INFERRED: no
                            // `[` there].
                            let n = e.vars.get("gameover");
                            if n != 0 {
                                game_over = Some(GameOverRequest {
                                    section: sec.id,
                                    n,
                                    narrator: None,
                                });
                                break;
                            }
                            after_item(e).await;
                            break;
                        }
                        // The executor's GAMEOVER test after every line
                        // (0x00428c05) also meets a GAMEOVER already set
                        // when the walk began: with BugFix::GameOverPuzzle
                        // off, the place loop's puzzle and /sel scans after
                        // a game over that left the warp loop without a
                        // click (Engine::no_click_exit) run with it.
                        if let Some(req) = self.pending_game_over(e, &sec, i, line_idx) {
                            game_over = Some(req);
                            break;
                        }
                    }
                    ConItem::Line(line) => {
                        // Every dialog line lowers the music when no fade
                        // runs (0x004284e8..0x004284f1, at the head of the
                        // per-line loop, so also a line after /set
                        // commands); the walk ends by raising it
                        // (0x00428d8b, on the only exit).
                        e.music.duck(true, None);
                        // The executor copies every `[` line it runs into the
                        // subtitle text buffer, shown or not, before the line
                        // plays (atlantis-cd-en-688128 FUN_00427420 @
                        // 0x00427707..0x00427722): the 1997 breakers read it.
                        e.subtitle_text
                            .store(con_line_buffer_text(&latin1_bytes(&line.raw)));
                        // FUN_00428f58 runs only when ClicPerso is 0
                        // (0x00428689) and writes its result there.
                        let mut perso = e.vars.get("clicperso");
                        if perso == 0 {
                            perso = resolve_speaker_perso(&line.speaker);
                            e.vars.set("clicperso", perso);
                        }
                        let req = LineRequest {
                            section: sec.id,
                            letter: con_track_letter(line_idx),
                            line: line.clone(),
                            perso,
                            interactive: sec.is_con,
                            inc_perso: self.inc_perso,
                        };
                        e.emit(Event::Line {
                            section: sec.id,
                            letter: req.letter,
                            speaker: line.speaker.clone(),
                        });
                        let out = self.dialog.play_line(e, &req).await;
                        if let Some(a) = out.entry_angles {
                            walk.entry = a;
                        }
                        // The letter counts the line (0x00428b21), ClicPerso
                        // is cleared (0x00428b39).
                        line_idx += 1;
                        e.vars.set("clicperso", 0);
                        if out.escape_skip {
                            // The skip block also sets 0x00496124 = 1 and
                            // clears the menu request (0x00428baf, 0x00428bc3).
                            e.escape_request = false;
                            i = con_escape_skip(&sec.items, i + 1, &mut line_idx);
                            after_item(e).await;
                            continue;
                        }
                        // A line's pass of the byte walk also runs the /set
                        // line after it (FUN_0042831c on the line's CR scans
                        // the next line) before the GAMEOVER test.
                        let set_next = matches!(sec.items.get(i + 1), Some(ConItem::Set { .. }));
                        if !set_next {
                            if let Some(req) = self.pending_game_over(e, &sec, i, line_idx) {
                                game_over = Some(req);
                                break;
                            }
                        }
                    }
                }
                after_item(e).await;
                i += 1;
            }

            // The game-over screen: the executor calls it when GAMEOVER is
            // set after a line (0x00428c05), before the end of the walk, so
            // the music stays lowered under the narrator. Then GameOver = 0
            // (FUN_0042de5c -> FUN_00426c68) and the main loop reloads the
            // player's checkpoint (0x0041975d); without one, a new game.
            // With BugFix::GameOverPuzzle off GAMEOVER stays set until the
            // reload clears it, as the original's (the main loop,
            // `run_game`): the place loop's puzzle and /sel scans after a
            // warp loop left without a click still see it.
            let had_game_over = game_over.is_some();
            if let Some(req) = game_over {
                e.emit(Event::GameOver(req.n));
                self.dialog.game_over(e, &req).await;
                if e.game_over_cleared_at_once() {
                    e.vars.set("gameover", 0);
                }
                e.abort = Some(Abort::NewGame);
            }

            let chain_go = self.chapter == entry_chapter
                && sec.goto_id > 0
                && !had_game_over
                && !e.should_abort();
            // The walk's end (0x00428c92..0x00428d8b) raises the music; a
            // `/go` (FUN_0042831c -> FUN_00425c04) only moves the walk on to
            // its target, so the music stays as the lines left it until the
            // walk ends.
            if !chain_go {
                // The given object is no longer shown (0x00428c92).
                e.given_object = None;
                e.vars.set("clicperso", walk.clic_perso);
                restore_entry(e, &mut walk);
                e.music.unduck(None);
            }

            if chain_go {
                match self.script.goto_section(sec.goto_id) {
                    Some(mut target) => {
                        target.is_con = self
                            .script
                            .sections()
                            .iter()
                            .find(|s| s.id == sec.goto_id)
                            .is_some_and(|s| s.is_con);
                        self.play_section(e, target, "go", Some(walk)).await;
                    }
                    None => {
                        e.given_object = None;
                        e.vars.set("clicperso", walk.clic_perso);
                        restore_entry(e, &mut walk);
                        e.music.unduck(None);
                    }
                }
            }
            // The click that ended the last line is not a navigation press.
            e.wait_buttons_up().await;
        })
    }
}

impl Scripts {
    /// The game over GameOver=`n` asks for at the item `i` of `sec`, its
    /// text the line after the item: the narrator only when that line is
    /// one, its voice ATA<section><letter>, the letter counting the lines
    /// before it (FUN_00426590 @ 0x004267ea, 0x004268b2..). The 1997 build
    /// takes the executor's current letter (atlantis-cd-en-688128
    /// FUN_004256a4 @ 0x004256ee), the same file for every game over of
    /// the shipped scripts. The game-over function itself makes the 1999
    /// build's pick of an alternative narration and counts the section's
    /// deaths (GOG FUN_00426570 @ 0x004267ce..0x00426882, 0x004269e3), so
    /// both its callers, the GameOver= command and the executor's test after
    /// an item (0x00428c05), go through here.
    fn game_over_at(
        &mut self,
        e: &mut Engine,
        sec: &ConSection,
        i: usize,
        line_idx: usize,
        n: i32,
    ) -> GameOverRequest {
        let mut narrator = con_game_over_narrator(&sec.items, i)
            .map(|l| (l.text.clone() + &l.trail, con_track_letter(line_idx)));
        // The 1999 build: the second death in a section plays the next
        // narrator line. The 2001 and 1997 builds with the restored content
        // too.
        let alt = e.game.exe.build.alt_game_over_narration || e.enhancements().restored_content;
        let mut narrator_item = i + 1;
        if let (Some((text, letter)), true) = (narrator.as_mut(), alt) {
            let count = self.game_over_count.entry(sec.id).or_default();
            let pick = con_alt_game_over_narration(&sec.items, i, count);
            if pick > 0 {
                if let ConItem::Line(l) = &sec.items[i + 1 + pick] {
                    *text = l.text.clone() + &l.trail;
                }
                *letter = char::from(*letter as u8 + pick as u8);
                narrator_item += pick;
            }
        }
        // The game-over screen copies the narrator line into the subtitle
        // text buffer, shown or not (atlantis-cd-en-688128 FUN_004256a4 @
        // 0x00425a31..0x00425a50).
        if let (Some(_), Some(ConItem::Line(l))) = (&narrator, sec.items.get(narrator_item)) {
            e.subtitle_text
                .store(con_narrator_buffer_text(&latin1_bytes(&l.raw)));
        }
        GameOverRequest {
            section: sec.id,
            n,
            narrator,
        }
    }

    /// The game over the executor's test after the item `i` of `sec` meets
    /// when GAMEOVER is already set (0x00428c05, FUN_00426590 with the text
    /// after the item).
    fn pending_game_over(
        &mut self,
        e: &mut Engine,
        sec: &ConSection,
        i: usize,
        line_idx: usize,
    ) -> Option<GameOverRequest> {
        let n = e.vars.get("gameover");
        (n != 0).then(|| self.game_over_at(e, sec, i, line_idx, n))
    }

    /// The script side of a checkpoint; `left` is CHAPTER before the
    /// chapter= that writes it.
    fn script_state(&self, e: &Engine, left: i32) -> ScriptState {
        let mut vars = [0; 7];
        for (i, v) in vars.iter_mut().enumerate().take(self.names.len()) {
            let name = self.names.name(i);
            *v = if name == "chapter" {
                left
            } else {
                e.vars.get(name)
            };
        }
        ScriptState {
            counters: self.counters.clone(),
            inc_perso: self.inc_perso as u8,
            vars,
        }
    }
}

/// The condition of an entry (`FUN_004298ac`), VUE read from the place
/// shown, the other variables stored.
fn eval_entry(
    names: &ConVariableNames,
    key: Option<&mut ConKeyBuffer>,
    text: &[u8],
    e: &Engine,
    sec: &ConSection,
) -> bool {
    let Some(offset) = sec.cond_offset else {
        return true;
    };
    evaluate(
        names,
        &|n| Scripts::value(e, n),
        &e.inventory,
        text,
        offset,
        key,
    )
}

/// ChangeCd=n: the disc `FUN_0041eda8` is called for, `(n - 1) & 0xff`
/// clamped to index 3, plus 1 (n = 0 or n > 4 asks for disc 4).
pub fn change_cd_disc(value: i32) -> u8 {
    ((value.wrapping_sub(1) & 0xff) as u8).min(3) + 1
}

/// N of a "gameover=N" command, 0 for any other.
fn gameover_value(cmd: &str) -> i32 {
    match cmd.get(..9) {
        Some(k) if k.eq_ignore_ascii_case("gameover=") => {
            crate::con_script::script::ConText(&cmd.as_bytes()[9..]).atoi(0)
        }
        _ => 0,
    }
}

/// The end of a walk: the view goes back to the entry angles, except an
/// axis whose ANGLEWARPX / ANGLEWARPY is non-zero: that one keeps the
/// current angle and the variable is cleared (0x00428ccc..0x00428d68).
/// `FUN_00428da8`, which the executor `FUN_0042844c` calls after every
/// executed item, a /set or a line (@ 0x00428c45, after the GameOver test),
/// and `FUN_00425eb0` once after the /INIT commands: it ends with the
/// NewSound poller `FUN_00432a64` (decompilation of atlantis-cd-en-714240,
/// FUN_00428da8's last call). So a sound that ended during a line is seen
/// as done, NEWSOUND cleared, before the next item (a `Cinema=` among
/// them) runs.
async fn after_item(e: &mut Engine) {
    e.poll_new_sound().await;
}

fn restore_entry(e: &mut Engine, walk: &mut Walk) {
    if e.vars.get("anglewarpx") != 0 {
        walk.entry.0 = e.warp.alpha();
        e.vars.set("anglewarpx", 0);
    }
    if e.vars.get("anglewarpy") != 0 {
        walk.entry.1 = e.warp.beta();
        e.vars.set("anglewarpy", 0);
    }
    e.set_view(walk.entry.0, walk.entry.1);
}

/// `FUN_00427bd0` (HidePerso): every sprite of the perso inactive, and the
/// id in the persistent hidden list, which the transition sprites and the
/// next sprite lists read.
fn hide_perso(e: &mut Engine, perso: i32) {
    if !e.hidden_persos.contains(&perso) {
        e.hidden_persos.push(perso);
    }
    set_perso_active(&mut e.sprite_list, perso, false);
    e.recomposite_sprite_layer();
}

/// `FUN_00427eb0` (ShowPerso): every sprite of the perso active again.
fn show_perso(e: &mut Engine, perso: i32) {
    e.hidden_persos.retain(|&h| h != perso);
    set_perso_active(&mut e.sprite_list, perso, true);
    e.recomposite_sprite_layer();
}

/// The CON file of `chapter`: `SCENAR\CHAPInnn.CON`, or in the Japanese DVD
/// build while its texts are Japanese `SCENAR\CHAPJnnn.CON`
/// (atlantis-dvd-ja-696832 FUN_00426df4 @ 0x00426e2d..0x00426e56).
pub fn con_file_name(e: &Engine, chapter: u32) -> String {
    let set = if e.japanese_texts() { 'J' } else { 'I' };
    format!("SCENAR\\CHAP{set}{chapter:03}.CON")
}

impl ScriptHooks for Scripts {
    fn reload_con_text<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, ()> {
        Box::pin(Scripts::reload_con_text(self, e))
    }

    fn chapter_loaded<'a>(
        &'a mut self,
        e: &'a mut Engine,
        chapter: u32,
        keep_state: bool,
    ) -> HookFuture<'a, ()> {
        Box::pin(async move {
            if !keep_state {
                // FUN_00423760 zeroes the /con play counters when a game
                // starts, and the IncPerso count (@ 0x004237a0).
                self.counters.iter_mut().for_each(|c| *c = 0);
                self.inc_perso = 0;
                self.game_over_count.clear();
            }
            // FUN_00427634 reloads the CON and runs its /INIT even when the
            // chapter does not change (the in-play no-checkpoint branch).
            self.chapter = chapter;
            e.chapter = chapter;
            self.load_con_script(e, chapter).await;
            // FUN_0042de5c step 7: CHAPTER = the chapter byte after the
            // /INIT.
            e.vars.set("chapter", self.chapter as i32);
        })
    }

    fn arrival_scan<'a>(
        &'a mut self,
        e: &'a mut Engine,
        vue: u16,
        arrivee: i32,
    ) -> HookFuture<'a, ()> {
        Box::pin(async move {
            e.vars.arrival_prepared(); // FUN_00427548
            self.in_place_arrival_scan = true;
            self.run_arrival_sel(e, i32::from(vue), arrivee).await;
            self.in_place_arrival_scan = false;
        })
    }

    fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
        Box::pin(self.run_con_timers(e, true))
    }

    fn bar_timers<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
        Box::pin(self.run_con_timers(e, false))
    }

    /// `FUN_0042c8ac` @ 0x0042cc1d: a follower portrait click sets
    /// 0x00496144, then runs `FUN_00425828` (0x0042cc57). The conversation
    /// hides the other followers on its subject path, restores them and
    /// clears the flag; with an object in hand the flag stays set.
    fn talk_to_guest<'a>(&'a mut self, e: &'a mut Engine, id: i32) -> HookFuture<'a, ()> {
        Box::pin(async move {
            e.guests.talk = true;
            self.interact_npc(e, id).await;
        })
    }

    /// The game-over counters are not part of the checkpoint and outlive
    /// its load (0x6cccac is cleared at start-up only), so a second death
    /// after the reload gets the second narration.
    fn restore_script_state(&mut self, _e: &mut Engine, state: &ScriptState) {
        self.counters.iter_mut().for_each(|c| *c = 0);
        for (c, &v) in self.counters.iter_mut().zip(&state.counters) {
            *c = v;
        }
        self.inc_perso = i32::from(state.inc_perso);
        self.pending_vars = Some(state.vars);
    }

    fn npc_click<'a>(&'a mut self, e: &'a mut Engine, id: u32, _zone: u32) -> HookFuture<'a, ()> {
        Box::pin(async move {
            let id = id as i32;
            if id < 0x200 {
                self.click_object(e, id).await;
            } else {
                self.interact_npc(e, id).await;
            }
        })
    }

    fn zone_click<'a>(&'a mut self, e: &'a mut Engine, _zone: u32) -> HookFuture<'a, ()> {
        Box::pin(async move {
            if e.vars.get("cliczone") != 0 || e.vars.get("objetenmain") != 0 {
                self.run_con_click(e).await;
            }
        })
    }

    fn zone_999<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, ()> {
        Box::pin(async move {
            e.vars.zone999_begin();
            self.run_sel_scan(e).await;
            e.vars.zone999_end();
        })
    }

    fn sel_scan<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, ()> {
        Box::pin(self.run_sel_scan(e))
    }

    fn puzzle_zone<'a>(&'a mut self, e: &'a mut Engine, n: u32) -> HookFuture<'a, bool> {
        Box::pin(async move {
            if e.vars.get("startpuzzle") != n as i32 {
                return false;
            }
            self.run_puzzle_zone(e, n).await;
            true
        })
    }

    fn departure_scan<'a>(&'a mut self, e: &'a mut Engine, dest: u16) -> HookFuture<'a, Departure> {
        Box::pin(async move {
            // FUN_00421cfc (0x00423dc5), then FUN_00427814: the /sel scan
            // and FLAGVUE1 = 0.
            e.vars.place_looked_up(i32::from(dest));
            self.run_sel_scan(e).await;
            e.vars.departure_scanned();
            if e.vars.get("gameover") != 0 {
                Departure::GameOver
            } else if e.vars.take_abort() {
                Departure::Abort
            } else {
                Departure::Go
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::ScriptHooks;

    /// The 29 literals of `FUN_00428f58`'s chain that no speaker token of
    /// the CON files reaches, each as a token, with the ClicPerso the
    /// original writes (run under emulation), so that a wrong literal,
    /// length or id in any entry fails.
    #[test]
    fn speaker_chain_literals() {
        let cases = [
            ("agat", 520),
            ("lasc", 521),
            ("melj", 522),
            ("sorc", 567),
            ("Pech", 524),
            ("pech", 524),
            ("acty", 524),
            ("creo", 525),
            ("chas", 540),
            ("passant9", 602),
            ("carv", 602),
            ("passant6", 601),
            ("passante7", 606),
            ("hect", 543),
            ("cuis", 551),
            ("rein", 560),
            ("reinem", 561),
            ("reinep", 562),
            ("garc", 526),
            ("servan", 609),
            ("servag", 582),
            ("chie", 650),
            ("anna", 663),
            ("escl", 597),
            ("hona", 682),
            ("sama", 683),
            ("naka", 684),
            ("lona", 576),
            ("gimb", 563),
        ];
        for (token, id) in cases {
            assert_eq!(resolve_speaker_perso(token), id, "{token}");
        }
    }
    use crate::test_support::with_engine;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A NewSound that ends during a dialog line is seen as done before the
    /// next item: the line players call FUN_00432178 every frame and the
    /// executor FUN_00428da8 after every item, both ending with the poller
    /// FUN_00432a64. So under the original's rules (`--original`, no
    /// BugFix::CinemaSound) the following `Cinema=` starts its own sound
    /// (CHAPI015 /190: Cinema=42, one line, Cinema=44).
    #[test]
    fn a_sound_that_ends_during_a_line_lets_the_next_cinematic_sound_start() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "a_sound_that_ends_during_a_line_lets_the_next_cinematic_sound_start",
            move |mut e| async move {
                let trace = Rc::new(RefCell::new(Vec::new()));
                e.trace = Some(trace.clone());
                let mut s = Scripts::with_dialog(Box::new(crate::dialog::Dialogs::new()));
                e.new_game(&mut s, false).await;
                assert!(e.game.big.contains("WAV\\ATA0190A.APC"));
                assert!(e.game.big.contains("WAV\\CINEM044.APC"));
                e.set_enhancements(crate::enhancements::Enhancements::original());
                let script = crate::con_script::ConScript::parse(
                    b"/190\r\n/sel(vue=1)\r\n[Meljanz,off] Grab him!\r\n/set(Cinema=44)\r\n/0\r\n",
                );
                let sec = script.sections()[0].clone();
                // The /set commands are read from the script's text.
                s.script = script;
                // The first cinematic's sound, a tenth of a second long and
                // playing as value 1.
                let short = crate::audio::Pcm {
                    rate: 22050,
                    channels: 1,
                    samples: std::sync::Arc::new(vec![0; 2205]),
                };
                e.music.start_new_sound(short, 0x6e);
                e.music.new_sound_playing = true;
                e.music.new_sound_last = 1;
                e.vars.set("newsound", 1);
                trace.borrow_mut().clear();
                s.play_section(&mut e, sec, "sel", None).await;
                let t = trace.borrow();
                let voice = t.iter().any(
                    |ev| matches!(ev, crate::engine::Event::Voice(n) if n.contains("ATA0190A")),
                );
                let started = t
                    .iter()
                    .any(|ev| matches!(ev, crate::engine::Event::NewSound(1, n) if n.contains("CINEM044")));
                *g2.borrow_mut() = Some((voice, started));
            },
            |sys, _| {
                // The mixer runs, so that the short sound and the voice end.
                let mut out = vec![0.0f32; 2 * (22050 * 16 / 1000)];
                sys.mixer.fill(&mut out, 22050, 2);
            },
        );
        if ran {
            assert_eq!(*got.borrow(), Some((true, true)));
        }
    }

    /// A presenter that notes, for each line, whether its still frame
    /// would carry the warp cursor.
    struct CursorProbe(Rc<RefCell<Vec<bool>>>);

    impl DialogPresenter for CursorProbe {
        fn play_line<'a>(
            &'a mut self,
            e: &'a mut Engine,
            _req: &'a LineRequest,
        ) -> HookFuture<'a, LineOutcome> {
            self.0.borrow_mut().push(e.still_frame_cursor().is_some());
            ready(LineOutcome::default())
        }
    }

    /// The voice-only line of a /con section that tests only ObjetEnMain,
    /// with object 257 in hand and a character under the pointer, reached
    /// by a click on that character (`give`) or on a zone: whether its
    /// still frame carries the cursor.
    fn give_line_cursor(give: bool) -> Option<Vec<bool>> {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "no_cursor_on_the_lines_of_a_given_object",
            move |mut e| async move {
                let mut s = Scripts::with_dialog(Box::new(CursorProbe(g2.clone())));
                e.new_game(&mut s, false).await;
                assert!(e.load_wam("ATLAN1.WAM").await);
                // A place and a view with a character under the pointer.
                let ids: Vec<u16> = e.wam.places.iter().map(|p| p.id).collect();
                let mut hit = None;
                'search: for id in ids {
                    e.next_place_id = Some(id);
                    e.do_place_change().await;
                    if e.npc_rects.is_empty() {
                        continue;
                    }
                    e.pointer.pos = (320, 240);
                    for ia in 0..315 {
                        for ib in -4..=4 {
                            e.set_view(ia as f32 * 0.02, ib as f32 * 0.1);
                            let (zone, perso) = e.pointer_targets();
                            if perso >= 0x200 && zone > 0 && zone < 0x200 {
                                hit = Some((perso, zone));
                                break 'search;
                            }
                        }
                    }
                }
                let (perso, zone) = hit.expect("a character over a zone");
                s.script = crate::con_script::ConScript::parse(
                    b"/1\r\n/con(ObjetEnMain=257)\r\n[Seth,off] Here.\r\n/0\r\n",
                );
                e.vars.set("objetenmain", 257);
                g2.borrow_mut().clear();
                if give {
                    s.npc_click(&mut e, perso, zone).await;
                } else {
                    e.vars.set("cliczone", zone as i32);
                    s.zone_click(&mut e, zone).await;
                }
            },
            |_, _| {},
        );
        ran.then(|| got.borrow().clone())
    }

    /// `FUN_00425828` sets 0x004960c0 around the /con click scan of a
    /// character clicked with an object in hand (@ 0x00425882..
    /// 0x004258ad), and `FUN_0041afd0` draws no cursor while it is set
    /// (@ 0x0041afe8): the still frames of that section's voice-only lines
    /// show none (CHAPI021 /270, CHAPI055 /685). A zone click's scan does
    /// not set it.
    #[test]
    fn no_cursor_on_the_lines_of_a_given_object() {
        let Some(given) = give_line_cursor(true) else {
            return;
        };
        assert_eq!(given, [false]);
        assert_eq!(give_line_cursor(false).unwrap(), [true]);
    }

    #[test]
    fn change_cd_clamps_the_disc() {
        let got: Vec<u8> = [1, 2, 3, 4, 5, 0, -1, 255, 257]
            .iter()
            .map(|&v| change_cd_disc(v))
            .collect();
        assert_eq!(got, [1, 2, 3, 4, 4, 4, 4, 4, 1]);
    }

    /// ChangeCd outside 1..=4 asks for disc 4 (it was ignored).
    #[test]
    fn change_cd_command_clamps_the_disc() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "change_cd_command_clamps_the_disc",
            move |mut e| async move {
                let mut s = Scripts::new();
                e.new_game(&mut s, false).await;
                for v in [2, 0, 1, 9, 3] {
                    s.exec_set(&mut e, None, &format!("changecd={v}")).await;
                    g2.borrow_mut().push(e.disc);
                }
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = got.borrow();
        assert_eq!(*got, [2, 4, 1, 4, 3]);
    }

    struct GameOvers(Rc<RefCell<Vec<GameOverRequest>>>);

    impl DialogPresenter for GameOvers {
        fn game_over<'a>(
            &'a mut self,
            _e: &'a mut Engine,
            req: &'a GameOverRequest,
        ) -> HookFuture<'a, ()> {
            self.0.borrow_mut().push(req.clone());
            ready(())
        }
    }

    /// CHAPI003 /45 (GameOver=2, then two narrator lines): with the
    /// alternative narration (the 1999 build, or the restored content), a
    /// second death after the checkpoint reload plays the second line; the
    /// reload used to clear the counters.
    #[test]
    fn second_game_over_after_a_reload_plays_the_second_narration() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "second_game_over_after_a_reload_plays_the_second_narration",
            move |mut e| async move {
                {
                    let mut x = e.enhancements();
                    x.restored_content = true;
                    e.set_enhancements(x);
                }
                let mut s = Scripts::with_dialog(Box::new(GameOvers(g2)));
                s.chapter_loaded(&mut e, 3, false).await;
                for _ in 0..2 {
                    let sec = s.script.sections().iter().find(|x| x.id == 45).cloned();
                    s.play_section(&mut e, sec.unwrap(), "con", None).await;
                    assert_eq!(e.abort, Some(Abort::NewGame));
                    // The reload's script side (load_checkpoint).
                    e.abort = None;
                    let state = s.script_state(&e, 3);
                    s.restore_script_state(&mut e, &state);
                    s.chapter_loaded(&mut e, 3, true).await;
                }
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = got.borrow();
        let letters: Vec<char> = got.iter().map(|r| r.narrator.as_ref().unwrap().1).collect();
        assert_eq!(letters, ['C', 'D']);
        assert_ne!(got[0].narrator, got[1].narrator);
    }

    /// The game overs (GAMEOVER, with a narrator), the puzzles' (id,
    /// result), GAMEOVER and the abort as the place loop returns.
    type ThroneRun = (Vec<(i32, bool)>, Vec<(i32, i32)>, i32, Option<Abort>);

    /// What CHAPI044's guards' timer does in the throne room: /602
    /// (Tim2(Time2>315)&(FlagTemp1=0): Cinema=84, Gameover=14) fires in
    /// view 281, where the arrival scan armed the throne lock
    /// (StartPuzzle 5), the pointer at the screen's centre on zone 1005.
    /// `fixed`: [`crate::enhancements::BugFix::GameOverPuzzle`]. Returns
    /// the game overs (GAMEOVER, with a narrator), the puzzles' results,
    /// and GAMEOVER and the abort as the place loop returns.
    fn throne_timer_game_over(test: &str, fixed: bool) -> Option<ThroneRun> {
        use crate::sys::{Input, MouseButton};
        let got = Rc::new(RefCell::new(None));
        let overs = Rc::new(RefCell::new(Vec::new()));
        let (g2, o2, o3) = (got.clone(), overs.clone(), overs.clone());
        let trace = Rc::new(RefCell::new(Vec::new()));
        let t2 = trace.clone();
        let mut since = None::<u64>;
        let ran = with_engine(
            test,
            move |mut e| async move {
                if !fixed {
                    e.set_enhancements(crate::enhancements::Enhancements::all_but_fix(
                        crate::enhancements::BugFix::GameOverPuzzle,
                    ));
                }
                let mut s = Scripts::with_dialog(Box::new(GameOvers(o2)));
                s.chapter_loaded(&mut e, 44, false).await;
                // The palace WAM that holds view 281, and the view with
                // the throne's zone 1005 at the screen's centre.
                let wams: Vec<String> = e
                    .game
                    .big
                    .names()
                    .filter(|n| n.ends_with(".WAM"))
                    .map(|n| n.rsplit('\\').next().unwrap_or(n).to_string())
                    .collect();
                let mut found = None;
                'search: for w in wams {
                    if !e.load_wam(&w).await || !e.wam.places.iter().any(|p| p.id == 281) {
                        continue;
                    }
                    e.next_place_id = Some(281);
                    e.do_place_change().await;
                    e.pointer.recentre();
                    for ia in 0..315 {
                        for ib in -4..=4 {
                            let (a, b) = (ia as f32 * 0.02, ib as f32 * 0.1);
                            e.set_view(a, b);
                            if e.pointer_targets() == (1005, 0) {
                                found = Some((a, b));
                                break 'search;
                            }
                        }
                    }
                }
                let (a, b) = found.expect("no view of zone 1005 in place 281");
                e.set_view(a, b);
                // Past the chapter's first scan and the guards' first
                // timer (/600); the lock armed; the clock past 315 s.
                e.vars.set("initflag", 1);
                e.vars.set("nbvisite", 2);
                e.vars.set("startpuzzle", 5);
                e.vars.set("testtime2", 1);
                let now = e.now();
                e.clocks.restart(2, 400, now);
                e.update_con_clocks();
                e.trace = Some(t2);
                e.game_step(&mut s).await;
                *g2.borrow_mut() = Some((e.vars.get("gameover"), e.abort));
            },
            move |sys, frame| {
                // The throne lock left: a click left of its rings, a while
                // after the first game over.
                if since.is_none() && !o3.borrow().is_empty() {
                    since = Some(frame);
                }
                if let Some(f0) = since {
                    match (frame - f0) % 8 {
                        0 if frame - f0 >= 40 => {
                            sys.push_input(Input::MouseMove { x: 10, y: 240 });
                            crate::test_support::button(sys, MouseButton::Left, true);
                        }
                        2 if frame - f0 >= 40 => {
                            crate::test_support::button(sys, MouseButton::Left, false)
                        }
                        _ => {}
                    }
                }
            },
        );
        if !ran {
            return None;
        }
        let overs = overs
            .borrow()
            .iter()
            .map(|r: &GameOverRequest| (r.n, r.narrator.is_some()))
            .collect();
        let puzzles = trace
            .borrow()
            .iter()
            .filter_map(|ev| match ev {
                crate::engine::Event::Puzzle { id, result } => Some((*id, *result)),
                _ => None,
            })
            .collect();
        let (gameover, abort) = got.borrow().expect("the place loop did not return");
        Some((overs, puzzles, gameover, abort))
    }

    /// FUN_00423ad8 after a warp loop left on a timer's GAMEOVER
    /// (0x0041c387): the zone under the pointer still runs its armed
    /// puzzle (FUN_004240fc @ 0x00424114), then the /sel scan, GAMEOVER
    /// still set: /010 /sel(FinPuzzle>0) meets the executor's test after
    /// its first line (0x00428c05) and shows the picture again, without a
    /// narrator (the next line is a /set). With
    /// [`crate::enhancements::BugFix::GameOverPuzzle`], the reload at once.
    #[test]
    fn a_timer_game_over_still_runs_the_throne_lock() {
        let t = "a_timer_game_over_still_runs_the_throne_lock";
        let Some(original) = throne_timer_game_over(t, false) else {
            return;
        };
        assert_eq!(
            original,
            (
                vec![(14, true), (14, false)],
                vec![(5, 2)],
                14,
                Some(Abort::NewGame)
            )
        );
        let fixed = throne_timer_game_over(t, true).unwrap();
        assert_eq!(fixed, (vec![(14, true)], vec![], 0, Some(Abort::NewGame)));
    }

    /// StopSound clears the NewSound poller's playing flag (FUN_0042b290 @
    /// 0x0042b2d8) and keeps the last value, so the same NewSound starts
    /// again.
    #[test]
    fn stopsound_lets_the_same_newsound_start_again() {
        let got = Rc::new(RefCell::new((0, Vec::new())));
        let g2 = got.clone();
        let ran = with_engine(
            "stopsound_lets_the_same_newsound_start_again",
            move |mut e| async move {
                let mut s = Scripts::new();
                e.new_game(&mut s, false).await;
                let n = (1..e.game.exe.newsound_names.len())
                    .find(|&v| {
                        e.game
                            .big
                            .contains(&format!("WAV\\{}", e.game.exe.newsound_names[v]))
                    })
                    .expect("a NewSound sample");
                g2.borrow_mut().0 = n as i32;
                let cmds = [
                    format!("newsound={n}"),
                    "stopsound=1".into(),
                    format!("newsound={n}"),
                ];
                for cmd in cmds {
                    s.exec_set(&mut e, None, &cmd).await;
                    // The executor's FUN_00428da8 after the item.
                    after_item(&mut e).await;
                    g2.borrow_mut()
                        .1
                        .push((e.music.new_sound_playing, e.music.new_sound_last));
                }
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let (n, got) = &*got.borrow();
        assert_eq!(*got, [(true, *n), (false, *n), (true, *n)]);
    }

    /// The follower-talk flag 0x00496144: a portrait click with an object
    /// in hand leaves it set (FUN_00425828 returns at 0x004258b7) and the
    /// followers as they are.
    #[test]
    fn follower_talk_flag_outlives_an_object_click() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "follower_talk_flag_outlives_an_object_click",
            move |mut e| async move {
                let mut s = Scripts::new();
                e.new_game(&mut s, false).await;
                e.guests.add(587, &[587, 600]);
                e.guests.add(600, &[587, 600]);
                let sprites = e.guests.sprite;
                e.vars.set("objetenmain", 257);
                s.talk_to_guest(&mut e, 587).await;
                *g2.borrow_mut() = Some((
                    e.guests.talk,
                    e.guests.sprite == sprites,
                    e.section_play,
                    e.vars.get("objetenmain"),
                ));
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = *got.borrow();
        assert_eq!(got, Some((true, true, false, 0)));
    }

    /// A NewSound the checkpoint holds (CHAPI044 /010: NewSound=67, then
    /// Chapter=45) is not played on the reload: NEWSOUND = 0 after the
    /// checkpoint's variables are applied.
    #[test]
    fn checkpoint_newsound_is_cleared_on_load() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "checkpoint_newsound_is_cleared_on_load",
            move |mut e| async move {
                let mut s = Scripts::new();
                e.new_game(&mut s, false).await;
                let mut state = s.script_state(&e, 1);
                let k = (0..s.names.len().min(7))
                    .find(|&i| s.names.name(i) == "newsound")
                    .expect("NEWSOUND is not a persistent variable");
                state.vars[k] = 67;
                e.write_checkpoint(&state);
                e.music.new_sound_last = 12;
                e.load_checkpoint(&mut s, true).await;
                *g2.borrow_mut() = Some((e.vars.get("newsound"), e.music.new_sound_last));
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = *got.borrow();
        assert_eq!(got, Some((0, 0)));
    }

    /// After a puzzle the view is loaded again (FUN_004240fc): the pointer
    /// back at (0x140, 0x140), StartPuzzle and FinPuzzle cleared.
    #[test]
    fn puzzle_zone_reloads_the_view() {
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let ran = with_engine(
            "puzzle_zone_reloads_the_view",
            move |mut e| async move {
                let mut s = Scripts::new();
                assert!(e.load_wam("ATLAN1.WAM").await);
                e.next_place_id = Some(1);
                e.do_place_change().await;
                e.pointer.pos = (5, 5);
                // StartPuzzle 17: no such puzzle, refused at once with 0.
                e.vars.set("startpuzzle", 17);
                let taken = s.puzzle_zone(&mut e, 17).await;
                *g2.borrow_mut() = Some((
                    taken,
                    e.pointer.pos,
                    e.vars.get("startpuzzle"),
                    e.vars.get("finpuzzle"),
                ));
            },
            |_, _| {},
        );
        if !ran {
            return;
        }
        let got = *got.borrow();
        assert_eq!(got, Some((true, (0x140, 0x140), 0, 0)));
    }

    struct MusicAtLines(Rc<RefCell<Vec<u8>>>);

    impl DialogPresenter for MusicAtLines {
        fn play_line<'a>(
            &'a mut self,
            e: &'a mut Engine,
            _req: &'a LineRequest,
        ) -> HookFuture<'a, LineOutcome> {
            self.0.borrow_mut().push(e.music.fade().mode);
            ready(LineOutcome::default())
        }
    }

    /// A `/go` moves the walk on without its end (FUN_0042831c ->
    /// FUN_00425c04): the music is not raised there, so the target's first
    /// line lowers it (CHAPI001 /14 -> /3: no line before the /go).
    #[test]
    fn go_does_not_raise_the_music() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "go_does_not_raise_the_music",
            move |mut e| async move {
                let mut s = Scripts::with_dialog(Box::new(MusicAtLines(g2)));
                s.chapter_loaded(&mut e, 1, false).await;
                let sec = s.script.sections().iter().find(|x| x.id == 14).cloned();
                let sec = sec.expect("CHAPI001 /14");
                assert_eq!(sec.goto_id, 3);
                assert!(!sec.items.iter().any(|i| matches!(i, ConItem::Line(_))));
                e.music.with_fade(|f| f.mode = 0);
                s.play_section(&mut e, sec, "con", None).await;
            },
            |_, _| {},
        );
        if ran {
            // Mode 2 (down) at the target's first line, not 1 (up).
            assert_eq!(got.borrow().first(), Some(&2));
        }
    }

    /// Trigger commands with 0 do nothing (FUN_0042a100 @
    /// 0x0042ac44..0x0042b1d0); ChangeCd empties the hand and clears the
    /// bar request (FUN_0041eda8).
    #[test]
    fn zero_triggers_and_change_cd() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "zero_triggers_and_change_cd",
            move |mut e| async move {
                let mut s = Scripts::new();
                e.new_game(&mut s, false).await;
                let pcm = crate::audio::Pcm {
                    rate: 22050,
                    channels: 1,
                    samples: std::sync::Arc::new(vec![0; 22050]),
                };
                e.music.start_new_sound(pcm, 0x6e);
                e.music.new_sound_playing = true;
                s.exec_set(&mut e, None, "stopsound=0").await;
                g2.borrow_mut().push(e.music.new_sound_active());
                s.exec_set(&mut e, None, "stopsound=1").await;
                g2.borrow_mut().push(e.music.new_sound_active());
                e.vars.set("objetenmain", 257);
                e.bar.request = true;
                s.exec_set(&mut e, None, "changecd=2").await;
                g2.borrow_mut()
                    .push(e.vars.get("objetenmain") == 0 && !e.bar.request);
            },
            |_, _| {},
        );
        if ran {
            assert_eq!(*got.borrow(), [true, false, true]);
        }
    }

    /// `chapter=` from a clicked or a timer section: the /sel entry the
    /// nested scan runs counts a play (0x00428493); from an arrival scan,
    /// none. With no /sel entry (chapter 36), the new /INIT commands run a
    /// second time (0x00427800, 0x00428bf9).
    #[test]
    fn chapter_change_nested_scan() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let ran = with_engine(
            "chapter_change_nested_scan",
            move |mut e| async move {
                let mut s = Scripts::new();
                e.new_game(&mut s, false).await;
                for (chapter, click) in [(2, true), (3, false)] {
                    let before: u32 = s.counters.iter().map(|&c| u32::from(c)).sum();
                    e.section_play = click;
                    s.change_chapter(&mut e, chapter).await;
                    e.section_play = false;
                    let after: u32 = s.counters.iter().map(|&c| u32::from(c)).sum();
                    g2.borrow_mut().push(after - before);
                }
                // Chapter 36 has no /sel entry: its /INIT runs twice; the
                // first chapter's /sel entry ends the walk.
                for chapter in [36, 1] {
                    let before = s.init_walks;
                    s.change_chapter(&mut e, chapter).await;
                    g2.borrow_mut().push(s.init_walks - before);
                }
            },
            |_, _| {},
        );
        if ran {
            assert_eq!(*got.borrow(), [1, 0, 2, 1]);
        }
    }

    /// CHAPI066's crab puzzle left from its choice screen (FinPuzzle 1),
    /// with Escape pressed in the puzzle first (`escape`): the post-puzzle
    /// scan plays /941, Hona's talking head, then GameOver=20. The time
    /// from the narrator's voice to the end of the puzzle zone, in ms.
    fn crab_narration_ms(test: &str, escape: bool, latch: bool) -> Option<u64> {
        use crate::sys::{Input, Key, MouseButton};
        let end = Rc::new(std::cell::Cell::new(None::<u64>));
        let voice = Rc::new(std::cell::Cell::new(None::<u64>));
        let trace = Rc::new(RefCell::new(Vec::new()));
        let (e2, t2, v3, t3) = (end.clone(), trace.clone(), voice.clone(), trace.clone());
        let ran = with_engine(
            test,
            move |mut e| async move {
                if latch {
                    e.set_enhancements(crate::enhancements::Enhancements::all_but_fix(
                        crate::enhancements::BugFix::PuzzleEscape,
                    ));
                }
                let mut s = Scripts::with_dialog(Box::new(crate::dialog::Dialogs::new()));
                e.new_game(&mut s, false).await;
                s.load_con_script(&mut e, 66).await;
                // Past the chapter's first scan (/10 is /sel(Initflag=0)).
                e.vars.set("initflag", 1);
                e.trace = Some(t2);
                s.run_puzzle_zone(&mut e, 15).await;
                e2.set(Some(e.sys.millis()));
            },
            move |sys, frame| {
                let mut sound = vec![0.0f32; 2 * (22050 * 16 / 1000)];
                sys.mixer.fill(&mut sound, 22050, 2);
                if v3.get().is_none()
                    && t3.borrow().iter().any(
                        |ev| matches!(ev, crate::engine::Event::Voice(n) if n.contains("ATA0941B")),
                    )
                {
                    v3.set(Some(frame * 16));
                }
                let at = |x, y, down| {
                    sys.push_input(Input::MouseMove { x, y });
                    crate::test_support::button(sys, MouseButton::Left, down);
                };
                match frame {
                    30 if escape => sys.push_input(Input::KeyDown(Key::Escape)),
                    31 if escape => sys.push_input(Input::KeyUp(Key::Escape)),
                    // The crab handles a click at the previous pass's
                    // pointer: the mouse gets to the top band first.
                    38 => sys.push_input(Input::MouseMove { x: 320, y: 10 }),
                    40 => at(320, 10, true),
                    42 => at(320, 10, false),
                    _ => {}
                }
            },
        );
        if !ran {
            return None;
        }
        let ev = trace.borrow();
        assert!(
            ev.contains(&crate::engine::Event::GameOver(20)),
            "no game over"
        );
        Some(end.get().unwrap() - voice.get().expect("no narrator voice"))
    }

    /// An Escape inside a puzzle latches 0x00496124 = 0 / 0x00496138 = 1
    /// (FUN_0041a298 @ 0x0041a419 / 0x0041a423); FUN_004240fc resets them
    /// only after its /sel scan (0x00424224, 0x00424238), so the game-over
    /// narration that scan reaches stops at its first poll (FUN_00426590's
    /// wait leaves on 0x00496124 != 1). Without the Escape it plays, and
    /// with [`crate::enhancements::BugFix::PuzzleEscape`] too.
    #[test]
    fn a_puzzle_escape_cuts_the_crab_game_over_narration() {
        let t = "a_puzzle_escape_cuts_the_crab_game_over_narration";
        let Some(cut) = crab_narration_ms(t, true, true) else {
            return;
        };
        let full = crab_narration_ms(t, false, true).unwrap();
        assert!(cut < 200, "cut narration lasted {cut} ms");
        assert!(full > 2000, "full narration lasted {full} ms");
        let fixed = crab_narration_ms(t, true, false).unwrap();
        assert!(fixed > 2000, "fixed: the narration lasted {fixed} ms");
    }
}
