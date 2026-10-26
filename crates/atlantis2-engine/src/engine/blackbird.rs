//! The blackbird songs of Ireland in the engine: the poet `POETE_ASSIS`
//! of `ilepoete.wam` view 73 asks which of five blackbirds sings the song
//! he names; three right answers in a row set `FINMERLE` = 1, which
//! chapter 20 turns into the poet standing up.
//!
//! - **View entry** (@ 0x004958f0, from the entry dispatcher
//!   @ 0x0049bd50): the two answer sounds `merlem.apc` (wrong) and
//!   `merleb.apc` (right) loaded into the sound table (a failed load is
//!   fatal), the counter and the five song handles reset, the five songs
//!   drawn with the C runtime's `rand` ([`crate::blackbird::draw_songs`]),
//!   the cursor pair `interfac.spr` frame 40 and the pointer at the screen
//!   centre. The mode (@ 0x006d2754) is not touched.
//! - **The click** (@ 0x00495ae0), first thing of every click of a view
//!   numbered 73 of an `IR` place (@ 0x0049ca8e, before the menus' guard)
//!   and from the click hooks on zones of types 3, 4 and 50 (view 73 has
//!   none). The flag @ 0x006d2751 cleared, then by mode: **3** the
//!   answer, the bird under the pointer (the one-column grid of five from
//!   the menu registers less 31) against the right one: the right sound
//!   and the counter + 1, or the wrong sound and the counter 0 (any other
//!   click, outside the menu too, is wrong), mode 0; at 3 in a row
//!   `FINMERLE` = 1. **1** a round: with the pointer on the three-row
//!   grid, the right bird drawn (`random(5)`), the five songs (re)loaded
//!   (each old handle freed first, which stops voice 2), the poet's
//!   question `(dial=89 + song)` run, mode 2, the flag set, the pointer
//!   at the centre. Then, in every mode: when `CLICPERSO` is the poet,
//!   the mode 0 and `FINMERLE` 0, mode 1.
//! - **The menu** in mode 2 (@ 0x0047fc70, in the warp draw instead of the
//!   subjects): the five birds (frames 26..30 of the subjects bank), the
//!   menu open; before the subject menu's answer, the song of the bird
//!   under the pointer on voice 2 when it is free (@ 0x00495a70). The
//!   subject menu's answer (@ 0x0045ab69..0x0045ac72) then turns mode 2
//!   into 3 and keeps (or sets again) the click, which reaches the click
//!   above in the same frame.
//! - **Leaving** by a move zone of view 73 (@ 0x004959f0): the cursor
//!   pair cleared, the two answer sounds and the loaded songs freed (each
//!   free of a loaded sound stops voice 2), the music back to 100 %, the
//!   flag cleared.
//!
//! The sounds share the sound table with the place sounds (50 entries,
//! @ 0x006735c8): a handle is an entry's index, which the poet's question
//! reads when `random(5)` gives 5 (`rand()` = 32767): the "song" is then
//! the right sound's handle.
//!
//! None of this is saved (no entry of the save table), and nothing resets
//! the mode: a load keeps it, and a new game, which the original starts
//! only from the start-up's player menu, finds the start-up's 0.

use atlantis2_data::defnames;

use super::{CursorBank, Engine, Event};
use crate::blackbird::{
    bird_cell, draw_songs, question, random, song_file, BAD_SOUND, BIRDS, CELL, FIRST_BIRD_FRAME,
    GOOD_SOUND, MENU_X, MENU_Y, POET, VIEW, WIN_COUNT, WIN_VAR,
};
use crate::place_sound::{Entry, TABLE};
use crate::sound::VOICE_EFFECTS;

/// The cursor pair's frame of `interfac.spr` the entry sets.
const POINTER_FRAME: i32 = 40;

/// The game's state (@ 0x006d2718..0x006d275c, 0x006d2751, all in `.bss`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blackbird {
    /// @ 0x006d2754: 0 none, 1 the poet clicked (a round can start), 2 the
    /// birds shown, 3 an answer pending.
    pub mode: i32,
    /// @ 0x006d2751: set at a round's start, cleared by every click of the
    /// game and by leaving; while set the warp draw chooses no cursor.
    pub flag: bool,
    /// @ 0x006d2758: right answers in a row.
    pub count: i32,
    /// @ 0x006d275c: the right bird, 0..4 (5 when `rand()` = 32767).
    pub right: i32,
    /// @ 0x006d2718: the five song numbers.
    pub songs: [i32; 5],
    /// @ 0x006d2730, 0x006d272c: the wrong and right answer sounds'
    /// handles (sound table entries).
    pub bad: i32,
    pub good: i32,
    /// @ 0x006d2738: the five songs' handles, −1 not loaded.
    pub handles: [i32; 5],
    /// @ 0x006d056c, 0x006d0570: the handle and bird the pointer last
    /// played; written only, nothing reads them.
    pub hovered: (i32, i32),
}

impl Blackbird {
    /// The song of bird `i` as the question reads it (@ 0x00495ce2,
    /// `[0x006d2718 + 4 i]`): past the five songs, `i` = 5 is the right
    /// sound's handle (@ 0x006d272c).
    pub fn song_at(&self, i: i32) -> i32 {
        match i {
            0..=4 => self.songs[i as usize],
            5 => self.good,
            _ => 0,
        }
    }
}

impl Engine {
    /// The view's warp name starts with `IR` (any case) and its number is
    /// 73: the click handler's test (@ 0x0049ca8e..0x0049cab1).
    pub(super) fn in_blackbird_view(&self) -> bool {
        self.current_view().is_some_and(|v| {
            v.name.len() >= 2 && v.name.as_bytes()[..2].eq_ignore_ascii_case(b"IR")
        }) && self.view == VIEW
    }

    /// The menu registers @ 0x006cf298 / 0x006cf29c.
    fn menu_registers(&self) -> (i32, i32) {
        (self.subject_menu.column_x, self.subject_menu.first_row)
    }

    /// FUN_004958f0: the view entry.
    pub fn blackbird_entry(&mut self) {
        let bad = self.sound_load(BAD_SOUND);
        self.blackbird.bad = bad;
        if bad == -1 {
            self.fatal("### ERREUR ### ID son non accepté.. : ID_son_merlem");
        }
        let good = self.sound_load(GOOD_SOUND);
        self.blackbird.good = good;
        if good == -1 {
            self.fatal("### ERREUR ### ID son non accepté.. : ID_son_merleb");
        }
        self.blackbird.count = 0;
        self.blackbird.handles = [-1; 5];
        self.blackbird.songs = draw_songs(&mut self.rand);
        self.set_cursor_pair(CursorBank::Interface, POINTER_FRAME);
        self.pointer = (self.width() as i32 / 2, self.height() as i32 / 2);
    }

    /// FUN_00495ae0: a click of the game.
    pub fn blackbird_click(&mut self) {
        self.event(Event::PuzzleRan {
            function: 0x0049_5ae0,
        });
        self.blackbird.flag = false;
        let regs = self.menu_registers();
        match self.blackbird.mode {
            3 => {
                let bird = bird_cell(regs, BIRDS, self.pointer);
                if bird != self.blackbird.right {
                    self.sound_play(self.blackbird.bad, VOICE_EFFECTS);
                    self.blackbird.count = 0;
                } else {
                    self.sound_play(self.blackbird.good, VOICE_EFFECTS);
                    self.blackbird.count += 1;
                }
                self.blackbird.mode = 0;
                if self.blackbird.count == WIN_COUNT {
                    self.with_script(|s, _| s.set_var(WIN_VAR, 1));
                    self.blackbird.mode = 0;
                }
            }
            1 => {
                let row = bird_cell(regs, 3, self.pointer);
                if (0..3).contains(&row) {
                    self.blackbird_round();
                }
            }
            _ => {}
        }
        // The poet clicked (`CLICPERSO`, @ 0x00495dfa..0x00495e3f).
        let clicperso = self
            .con
            .as_ref()
            .map_or(self.vars.clicperso, |s| s.vars[s.keys.clicperso]);
        let poet = self.names.id(defnames::PERSONNAGES, POET).map(|i| i as i32);
        if poet.is_ok_and(|p| p == clicperso)
            && self.blackbird.mode == 0
            && self.con.as_ref().and_then(|s| s.var(WIN_VAR)) == Some(0)
        {
            self.blackbird.mode = 1;
        }
    }

    /// A round's start (@ 0x00495bec..0x00495df5).
    fn blackbird_round(&mut self) {
        self.blackbird.right = random(&mut self.rand, 5);
        for i in 0..5 {
            let name = song_file(self.blackbird.songs[i]);
            if self.blackbird.handles[i] != -1 {
                self.sound_free(self.blackbird.handles[i], VOICE_EFFECTS);
            }
            let h = self.sound_load(&name);
            self.blackbird.handles[i] = h;
            if h == -1 {
                self.fatal("### ERREUR ### ID son non accepté.. : ID_son_merle[n]");
            }
        }
        let song = self.blackbird.song_at(self.blackbird.right);
        match question(song) {
            Some(c) => {
                self.with_script(|s, h| s.run_command(h, c.as_bytes()));
            }
            None => self.fatal(&format!(
                "### ERREUR ### Appeler Eric en urgence    soluce[bonne] incorrect... : {song}"
            )),
        }
        self.blackbird.mode = 2;
        self.blackbird.flag = true;
        self.pointer = (self.width() as i32 / 2, self.height() as i32 / 2);
    }

    /// FUN_00495a70: in mode 2, the song of the bird under the pointer on
    /// voice 2 when nothing plays there.
    pub fn blackbird_hover(&mut self) {
        if self.blackbird.mode != 2 {
            return;
        }
        let bird = bird_cell(self.menu_registers(), BIRDS, self.pointer);
        if !(0..BIRDS).contains(&bird) || self.voices.is_playing(VOICE_EFFECTS) {
            return;
        }
        let h = self.blackbird.handles[bird as usize];
        self.blackbird.hovered = (h, bird);
        self.sound_play(h, VOICE_EFFECTS);
    }

    /// FUN_0047fc70: the five birds and their rings, the menu registers at
    /// (450, 180), the menu open.
    pub fn blackbird_menu_draw(&mut self) {
        self.subject_menu.column_x = MENU_X;
        self.subject_menu.first_row = MENU_Y;
        for k in 0..BIRDS {
            let (x, y) = (MENU_X, MENU_Y + CELL * k);
            self.centred(FIRST_BIRD_FRAME + k, x + 25, y + 25, false);
            self.centred(0, x + 31, y + 31, false);
        }
        self.subject_menu.open = true;
    }

    /// FUN_004959f0: a move zone of view 73 clicked (the dispatcher tests
    /// the `IR` prefix; this function only the view's number).
    pub fn blackbird_leave(&mut self) {
        if self.view != VIEW {
            return;
        }
        self.clear_cursor_pair();
        self.sound_free(self.blackbird.bad, VOICE_EFFECTS);
        self.sound_free(self.blackbird.good, VOICE_EFFECTS);
        for i in 0..5 {
            let h = self.blackbird.handles[i];
            if h != -1 {
                self.sound_free(h, VOICE_EFFECTS);
            }
            self.blackbird.handles[i] = -1;
        }
        self.music.set_percent(100);
        self.blackbird.flag = false;
    }

    /// The sound table's loaded entries (index, name), for tests and
    /// tools.
    pub fn sound_table(&self) -> Vec<(usize, String)> {
        self.place
            .table
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.as_ref().map(|e| (i, e.name.clone())))
            .collect()
    }

    /// Every entry of the sound table freed (nothing stopped), for tests
    /// and tools.
    pub fn sound_table_clear(&mut self) {
        for e in self.place.table.iter_mut() {
            *e = None;
        }
        self.place_pcm.clear();
    }

    /// A fatal error of the original (FUN_00480cd0: an emergency save,
    /// then the game quits): here the scripts stop.
    fn fatal(&mut self, msg: &str) {
        log::error!("{msg}");
        self.with_script(|s, _| s.fail(msg.to_string()));
    }

    /// FUN_0047a530(name, 0): the sound table's entry of that name if one
    /// is loaded (exact bytes), else the first free entry (@ 0x0047a610;
    /// none: a fatal error and entry 0) with the file decoded into it
    /// (@ 0x0047aad0, which first marks the entry empty); −1 when the
    /// file cannot be read. The entry has no position (+0x14 = 0): it
    /// plays at the effects volume, centred.
    pub fn sound_load(&mut self, name: &str) -> i32 {
        if let Some(i) = self
            .place
            .table
            .iter()
            .position(|e| e.as_ref().is_some_and(|e| e.name == name))
        {
            return i as i32;
        }
        let free = match self.place.table.iter().position(Option::is_none) {
            Some(i) => i,
            None => {
                self.fatal("find_free_sound: Sound overflow");
                0
            }
        };
        self.place.table[free] = None;
        self.place_pcm.remove(&free);
        let Some((_, pcm)) = self.load_sound(name) else {
            log::warn!("### ERREUR lecture impossible fichier : {name}");
            self.event(Event::SoundLoaded {
                name: name.to_string(),
                entry: -1,
            });
            return -1;
        };
        self.event(Event::SoundLoaded {
            name: name.to_string(),
            entry: free as i32,
        });
        self.place.table[free] = Some(Entry {
            name: name.to_string(),
            owner: None,
            looping: false,
        });
        self.place_pcm.insert(free, pcm);
        free as i32
    }

    /// FUN_0047aa30(handle, voice): a loaded entry's voice stopped
    /// (whatever it plays) and the entry freed; a handle outside 0..49 is
    /// only reported; an empty entry: nothing.
    pub fn sound_free(&mut self, handle: i32, voice: usize) {
        let Some(i) = usize::try_from(handle).ok().filter(|&i| i < TABLE) else {
            log::warn!("AtlSound_free:ID_son invalide:{handle}");
            return;
        };
        if self.place.table[i].is_none() {
            return;
        }
        self.voices.stop(voice);
        self.place.table[i] = None;
        self.place_pcm.remove(&i);
    }

    /// FUN_0047a650(handle, voice) for an entry with no position: the
    /// voice stopped, then the sound played once on it at the effects
    /// volume, centred; a handle outside 0..49 is reported, an empty entry
    /// plays nothing.
    pub fn sound_play(&mut self, handle: i32, voice: usize) {
        let Some(i) = usize::try_from(handle).ok().filter(|&i| i < TABLE) else {
            log::warn!("AtlSound_play:ID_son invalide:{handle}");
            return;
        };
        let (Some(entry), Some(pcm)) =
            (self.place.table[i].clone(), self.place_pcm.get(&i).cloned())
        else {
            return;
        };
        self.voices.stop(voice);
        let att = self.fx_attenuation();
        self.voices.play(voice, pcm, entry.looping, att, 0);
        self.event(Event::Sound(entry.name));
    }
}
