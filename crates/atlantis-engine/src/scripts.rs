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

use crate::con::{
    con_alt_game_over_narration, con_angle_radians, con_entry_allowed, con_escape_skip,
    con_game_over_narrator, con_track_letter, evaluate, parse_con_set, vars, ConItem, ConKeyBuffer,
    ConLine, ConScript, ConSection, ConSubjects, ConVariableNames, CON_PLAY_COUNTERS,
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
    /// The narrator's text and the track letter of its voice, when the line
    /// after the command is a narrator line.
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

    /// A new conversation starts (the previous camera is forgotten).
    fn new_conversation(&mut self) {}

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
    /// The guests following the player: their ids, and whether they are
    /// shown. TODO(phase 3): the portraits and their slides.
    pub guests: Vec<i32>,
    pub guests_shown: bool,
    scan_key: ConKeyBuffer,
    /// The /INIT block runs (the loading flag 0x00496168).
    init_running: bool,
    /// The place loop's own arrival scan runs.
    in_place_arrival_scan: bool,
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
            guests: Vec::new(),
            guests_shown: true,
            scan_key: [0; 16],
            init_running: false,
            in_place_arrival_scan: false,
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
            let name = format!("SCENAR\\CHAPI{chapter:03}.CON");
            let Some(data) = e.read(&name).await else {
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

    /// `FUN_00425eb0`: the /INIT commands, with the loading flag set.
    async fn exec_init(&mut self, e: &mut Engine) {
        let was = std::mem::replace(&mut self.init_running, true);
        let cmds = self.script.init_cmds().to_vec();
        let chapter = self.chapter;
        for (cmd, offset) in cmds {
            self.exec_set(e, Some(offset), &cmd).await;
            if self.chapter != chapter || e.should_abort() {
                break;
            }
        }
        self.init_running = was;
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
                    t.extend(crate::con::script::latin1_bytes(cmd));
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
                "enablesuj" => self.subjects.enable(text!(), set.value_pos),
                "disablesuj" => self.subjects.disable(text!(), set.value_pos),
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
                "startmusik" => e.music_start_command(value).await,
                "stopmusik" => e.music_stop_command().await,
                "loadmusik" | "delmusik" => {
                    e.music_list_command(key == "loadmusik", text!(), set.value_pos)
                        .await;
                }
                "cinema" => e.play_cinema(value).await,
                // ChangeCd=n: the disc prompt for disc n (@ 0x0042a4c4); the
                // port reads every disc at once.
                "changecd" => {}
                // HideGuest / ShowGuest store no variable.
                "hideguest" | "showguest" => self.guests_shown = key == "showguest",
                // AddGuest / SubGuest (FUN_0042b480 / FUN_0042b6c0): the
                // record, then the variable 0. TODO(phase 3): the portraits
                // sliding in and out.
                "addguest" => {
                    if self.guests.len() < 5 {
                        self.guests.push(value);
                    }
                    e.vars.set(key, 0);
                }
                "subguest" => {
                    self.guests.retain(|&g| g != value);
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
                // NewSound=N: the value is stored; the poller FUN_00432a64,
                // run after every executed line (FUN_00428da8) and at the end
                // of every frame (FUN_00432178), starts the sound.
                "newsound" => {
                    e.vars.set(key, value);
                    e.poll_new_sound().await;
                }
                // StopSound: the NewSound sample stops, whatever N.
                "stopsound" => {
                    e.vars.set("newsound", 0);
                    e.music.stop_new_sound();
                }
                // ArcMagique=N: the magic-bow animation (@ 0x0042aa09), then
                // the store. TODO(phase 3): the animation.
                // TirFleche=N restarts the arrow frame (0x0042a98e), then the
                // store. TODO(phase 3): bow mode.
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
        e.vars.set("chapter", value);
        if value <= 0 || value as u32 == self.chapter {
            return;
        }
        log::info!("chapter {} -> {value}", self.chapter);
        self.chapter = value as u32;
        e.chapter = value as u32;
        // TODO(phase 3): the chapter checkpoint, written first
        // (0x0042765f) unless the /INIT block runs (0x00496168).
        self.load_con_script(e, value as u32).await;
        if let Some(cur) = e.current_place_id {
            let last = i32::from(e.last_vue);
            self.run_arrival_sel(e, i32::from(cur), last).await;
        }
        // ARRIVEEVUE stays = LastVue: only the place loop clears it, right
        // after its own arrival scan (@ 0x00423be7). A chapter= run anywhere
        // else in the pass therefore gets a second arrival scan on the next
        // pass (@ 0x00423b5a..0x00423b78) when LastVue != 0.
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
        let from = e.current_place_id;
        if let Some(sub) = e
            .wam
            .sub_wams
            .iter()
            .find(|s| Some(s.src_place) == from && s.dst_place == place)
            .cloned()
        {
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
        self.begin_scan();
        let restored = e.enhancements.restored_content;
        let found = self.script.find_sel_entry(|sec| {
            let key = (!restored).then_some(&mut self.scan_key);
            eval_entry(&self.names, key, self.script.text(), e, sec)
        });
        let sec = found.map(|i| self.script.sections()[i].clone());
        if let Some(sec) = sec {
            self.play_section(e, sec, "sel", None).await;
        }
    }

    /// The arrival scan: ArriveeVue = the place arrived FROM (a negative
    /// origin is no place yet: the arrived place stands for it), the /sel
    /// scan, then ArriveeVue = 0.
    async fn run_arrival_sel(&mut self, e: &mut Engine, arrived: i32, from: i32) {
        e.vars
            .set("arriveevue", if from >= 0 { from } else { arrived });
        self.run_sel_scan(e).await;
        e.vars.set("arriveevue", 0);
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
            let restored = e.enhancements.restored_content;
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
                self.play_section(e, sec, kind, None).await;
                played = true;
            }
            if e.should_abort() {
                break;
            }
        }
        played
    }

    /// `FUN_00425cf4` (flag 0x00496170 set: a play is counted), then
    /// `FUN_00425dd0`: clicking zone 2000+N with object N in hand puts the
    /// object back into the place when the place's sprite list holds it.
    async fn run_con_click(&mut self, e: &mut Engine) {
        self.begin_scan();
        let restored = e.enhancements.restored_content;
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
            // TODO(phase 3): the bar shows the object; a right click keeps
            // it in hand, a left click puts it away.
            e.vars.set("objetenmain", 0);
        }
    }

    /// `FUN_00425828`: a character clicked.
    async fn interact_npc(&mut self, e: &mut Engine, perso: i32) {
        e.vars.set("clicperso", perso);
        // @ 0x00425867..0x004258b7: with an object in hand there is no
        // conversation; the /con click scan runs and the object leaves the
        // hand.
        if e.vars.get("objetenmain") != 0 {
            e.section_play = true;
            self.run_con_click(e).await;
            e.section_play = false;
            e.vars.set("objetenmain", 0);
            return;
        }
        // @ 0x004258c4: 0x00496170 = 1 up to its end (0x00425b7a).
        e.section_play = true;
        self.dialog.new_conversation();

        // With enabled subjects the menu lists them in table order and is
        // shown first (0x00425930..0x00425a8a). Then the eligible /con
        // entries are walked in file order (FUN_004281f8) and the first
        // that fits plays (0x00425abe..0x00425b75): an entry with no /suj
        // line always fits; one with a /suj line fits when the perso has no
        // subjects, or when its list holds the chosen subject
        // (FUN_004264a8); a click outside the grid stops the walk at the
        // first /suj entry; an empty grid cell gives subject 0, which no
        // list holds.
        let sujs: Vec<u8> = self.subjects.subjects(perso).to_vec();
        let menu = !sujs.is_empty();
        let mut chosen = None;
        if menu {
            chosen = self.dialog.subject_menu(e, perso, &sujs).await;
            if e.should_abort() {
                e.section_play = false;
                return;
            }
        }
        self.begin_scan();
        let restored = e.enhancements.restored_content;
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
        e.vars.set("clicperso", 0);
    }

    /// `FUN_004240fc`: FinPuzzle = the puzzle's result, the view is
    /// reloaded, StartPuzzle = 0, one /sel scan, then FinPuzzle = 0. It
    /// first stops the ambient sounds (FUN_00431e98 @ 0x0042417e); around
    /// it the music goes down by half (0x00495ea4 = 50, 0x00495eac = 2),
    /// then back up with 80 % again.
    async fn run_puzzle_zone(&mut self, e: &mut Engine, n: u32) {
        e.stop_ambient();
        e.music.duck(false, Some(50));
        // TODO(phase 5): the puzzles. Each one is won at once, as the
        // puzzles not translated yet are in the ScummVM port, so that the
        // chapters it gates go on (FinPuzzle = 255).
        log::warn!("puzzle {n}: not translated yet, won");
        e.vars.set("finpuzzle", 255);
        e.music.unduck(Some(80));
        if e.should_abort() {
            return;
        }
        e.vars.set("startpuzzle", 0);
        self.run_sel_scan(e).await;
        e.vars.set("finpuzzle", 0);
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
            // A body that starts with a dialog line lowers the music when no
            // fade runs (@ 0x004284e8); every section ends by raising it
            // (@ 0x00428d8b, on the only exit).
            if matches!(sec.items.first(), Some(ConItem::Line(_))) {
                e.music.duck(true, None);
            }
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
                            // The narrator's voice is ATA<section><letter>,
                            // the letter counting the lines before it
                            // (FUN_00426590 @ 0x004267ea, 0x004268b2..).
                            let mut narrator = con_game_over_narrator(&sec.items, i)
                                .map(|l| (l.text.clone(), con_track_letter(line_idx)));
                            // The 1999 build: the second death in a section
                            // plays the next narrator line.
                            // The 2001 build with the restored content too.
                            let alt = e.game.exe.build.alt_game_over_narration
                                || e.enhancements.restored_content;
                            if let (Some((text, letter)), true) = (narrator.as_mut(), alt) {
                                let count = self.game_over_count.entry(sec.id).or_default();
                                let pick = con_alt_game_over_narration(&sec.items, i, count);
                                if pick > 0 {
                                    if let ConItem::Line(l) = &sec.items[i + 1 + pick] {
                                        text.clone_from(&l.text);
                                    }
                                    *letter = char::from(*letter as u8 + pick as u8);
                                }
                            }
                            game_over = Some(GameOverRequest {
                                section: sec.id,
                                n,
                                narrator,
                            });
                            break;
                        }
                        // A chapter change replaced the script: the walk ends.
                        if self.chapter != entry_chapter {
                            break;
                        }
                    }
                    ConItem::Line(line) => {
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
                            i = con_escape_skip(&sec.items, i + 1, &mut line_idx);
                            continue;
                        }
                    }
                }
                i += 1;
            }

            // The game-over screen: the executor calls it when GAMEOVER is
            // set after a line (0x00428c05), before the end of the walk, so
            // the music stays lowered under the narrator. Then GameOver = 0
            // (FUN_0042de5c -> FUN_00426c68) and the main loop reloads the
            // player's checkpoint (0x0041975d), TODO(phase 3); without one,
            // a new game.
            let had_game_over = game_over.is_some();
            if let Some(req) = game_over {
                e.emit(Event::GameOver(req.n));
                self.dialog.game_over(e, &req).await;
                e.vars.set("gameover", 0);
                e.abort = Some(Abort::NewGame);
            }

            let chain_go = self.chapter == entry_chapter
                && sec.goto_id > 0
                && !had_game_over
                && !e.should_abort();
            if !chain_go {
                // The given object is no longer shown (0x00428c92).
                e.given_object = None;
                e.vars.set("clicperso", walk.clic_perso);
                restore_entry(e, &mut walk);
            }
            e.music.unduck(None);

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
                    }
                }
            }
            // The click that ended the last line is not a navigation press.
            e.wait_buttons_up().await;
        })
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

/// N of a "gameover=N" command, 0 for any other.
fn gameover_value(cmd: &str) -> i32 {
    match cmd.get(..9) {
        Some(k) if k.eq_ignore_ascii_case("gameover=") => {
            crate::con::script::ConText(&cmd.as_bytes()[9..]).atoi(0)
        }
        _ => 0,
    }
}

/// The end of a walk: the view goes back to the entry angles, except an
/// axis whose ANGLEWARPX / ANGLEWARPY is non-zero: that one keeps the
/// current angle and the variable is cleared (0x00428ccc..0x00428d68).
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

impl ScriptHooks for Scripts {
    fn chapter_loaded<'a>(
        &'a mut self,
        e: &'a mut Engine,
        chapter: u32,
        keep_state: bool,
    ) -> HookFuture<'a, ()> {
        Box::pin(async move {
            if !keep_state {
                // FUN_00423760 zeroes the /con play counters when a game
                // starts, and the IncPerso count (@ 0x004237a0); the guest
                // list is zeroed at boot (FUN_004239f4), shown.
                self.counters.iter_mut().for_each(|c| *c = 0);
                self.inc_perso = 0;
                self.guests.clear();
                self.guests_shown = true;
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
