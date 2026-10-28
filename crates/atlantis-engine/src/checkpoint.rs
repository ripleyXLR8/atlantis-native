//! The players and their chapter checkpoints, as the original keeps them:
//! `players.atl` and `p<letter><chapter>.gam`, written at every chapter
//! change and read back when a player starts, after a game over and from
//! the load picker.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`saveload.cpp`;
//! GPLv3, the ScummVM team), which follows `atlantis.exe`'s writer
//! `FUN_0042d6b8` and reader `FUN_0042de5c`. Unlike the ScummVM port, the
//! files are the original's own: an original installation's files can be
//! taken over, and the files written here can be read by the original.

use atlantis_data::saves::{Checkpoint, InventoryRecord, Players, CHECKPOINT_GUESTS, MAX_PLAYERS};

use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::persist::Settings;
use crate::ui::{object_icon, Guests};

/// The player list file.
pub const PLAYERS_FILE: &str = "players.atl";

/// The script side of a checkpoint (the CON play counters, the IncPerso
/// count and the 7 persistent variables), which [`ScriptHooks`] owns.
#[derive(Clone, Debug, Default)]
pub struct ScriptState {
    pub counters: Vec<u8>,
    pub inc_perso: u8,
    pub vars: [i32; 7],
}

impl Engine {
    /// The player list: the store's `players.atl`, else an original one
    /// in the game folder, taken over with its checkpoints; else a new one.
    pub(crate) fn load_players(&mut self) {
        let marker = self.game.exe.new_player_marker.clone();
        let rule = self.players_row_rule();
        if let Some(p) = self
            .saves
            .read(PLAYERS_FILE)
            .and_then(|d| Players::read_rows(&d, &marker, rule))
        {
            self.players = self.with_build_layout(p);
            return;
        }
        self.players = self.with_build_layout(Players::new(&marker));
        self.import_original_saves();
    }

    /// The build's players.atl row rule ([`Players::read_rows`]): the rows
    /// also end at an empty name but in the 1997 builds, whose reader
    /// stops at the marker only.
    fn players_row_rule(&self) -> bool {
        !self.game.exe.build.players_1997
    }

    /// The list as the build writes it (the 1997 build: 95 bytes; the
    /// Chinese builds: 114 bytes, ExeBuild::players_names_block).
    fn with_build_layout(&self, mut p: Players) -> Players {
        let build = self.game.exe.build;
        p.layout_1997 = build.players_1997;
        p.sixth_row = match build.players_names_block {
            0x66 => Some(p.sixth_row.unwrap_or_default()),
            _ => None,
        };
        p
    }

    /// The original's files in the game folder (an installation the
    /// original played): `players.atl` and the checkpoints of its letters,
    /// copied once into the store. The game folder is only read.
    fn import_original_saves(&mut self) {
        let src = self.game.big.source().clone();
        let marker = self.game.exe.new_player_marker.clone();
        let Some(data) = src.read(PLAYERS_FILE) else {
            return;
        };
        let Some(p) = Players::read_rows(&data, &marker, self.players_row_rule()) else {
            log::warn!("{PLAYERS_FILE} of the game folder not understood");
            return;
        };
        let mut n = 0;
        for &letter in &p.letters[..MAX_PLAYERS] {
            if letter == 0 {
                continue;
            }
            for chapter in 0..=255u8 {
                let name = atlantis_data::saves::checkpoint_name(letter, chapter);
                if let Some(d) = src.read(&name) {
                    if Checkpoint::parse(&d).is_some() {
                        self.saves.write(&name, &d);
                        n += 1;
                    } else {
                        log::warn!("{name}: not a checkpoint");
                    }
                }
            }
        }
        log::info!(
            "{PLAYERS_FILE} of the game folder taken over: {} players, {n} checkpoints",
            p.player_count()
        );
        self.players = self.with_build_layout(p);
        self.save_players();
    }

    /// Writes `players.atl` (the player screen's OK, a deletion, a
    /// checkpoint).
    pub(crate) fn save_players(&self) {
        self.saves.write(PLAYERS_FILE, &self.players.to_bytes());
    }

    /// The settings file, from the current options.
    pub(crate) fn save_settings(&mut self) {
        self.settings.subtitles = self.subtitles;
        // The graphics mode, the HD options, the enhancements and full
        // screen are the front end's (its options menu and its full screen
        // keys write them): keep the stored values, or this save would undo
        // a choice made since start-up (and a --original run would store
        // its overrides).
        if let Some(stored) = Settings::load(self.saves.as_ref()) {
            self.settings.graphics = stored.graphics;
            self.settings.hd = stored.hd;
            self.settings.enhancements = stored.enhancements;
            self.settings.fullscreen = stored.fullscreen;
        }
        self.settings.save(self.saves.as_ref());
    }

    /// The four volumes of the settings applied: the master volume on the
    /// mixer, the music volume the fades move to, the effects volume of the
    /// ambient sounds and NewSound, the voices' volume.
    pub(crate) fn apply_volumes(&mut self) {
        let v = self.settings.volumes;
        let scale = |x: u8| (u32::from(x.min(127)) * 255 / 127) as u8;
        self.sys.mixer.set_master(scale(v[0]));
        self.music.with_fade(|f| f.vol = v[1]);
        self.ambient.effects_volume = v[2];
        self.voice_volume = scale(v[3]);
    }

    /// `FUN_0042d6b8`, from the chapter change `FUN_00427634` (0x0042765f):
    /// the checkpoint `p<letter><chapter>.gam` of the current player, then
    /// `players.atl` with the player's resume chapter.
    pub fn write_checkpoint(&mut self, script: &ScriptState) {
        let player = self.player.min(MAX_PLAYERS - 1);
        let chapter = self.chapter as u8;
        let table = &self.game.exe.inventory_object_ids;
        let index = self.current_place.unwrap_or(0) as i32;
        let place = i32::from(self.current_place_id.unwrap_or(0));
        let mut guests = [(0, 0); CHECKPOINT_GUESTS];
        for (i, g) in guests.iter_mut().enumerate() {
            *g = (self.guests.id[i], self.guests.sprite[i]);
        }
        let cp = Checkpoint {
            wam_path: format!("wam\\{}", self.wam_name.to_ascii_lowercase()).into_bytes(),
            chapter,
            prev_chapter: self.prev_chapter,
            bank: 0,
            place_index: index,
            prev_place_index: index,
            place_id: place,
            last_vue: i32::from(self.last_vue),
            requested_vue: place,
            inc_perso: script.inc_perso,
            inventory: self
                .inventory
                .iter()
                .map(|&id| InventoryRecord {
                    sprite: object_icon(table, id).unwrap_or(0) as u8,
                    id,
                    present: 1,
                })
                .collect(),
            counters: script.counters.clone(),
            vars: script.vars,
            guest_count: self.guests.count as u8,
            guests,
            hidden: self.hidden_persos.iter().map(|&h| h as i16).collect(),
            subtitles: i32::from(self.subtitles),
            omni3d: i32::from(self.settings.omni3d_mode2),
            volumes: self.settings.volumes,
            disc: self.disc,
        };
        let name = self.players.checkpoint_name(player, chapter);
        self.players.chapters[player] = chapter; // 0x0042d836
        if self.saves.write(&name, &cp.to_bytes()) {
            self.emit(crate::engine::Event::CheckpointWritten(name));
        }
        self.save_players();
    }

    /// The checkpoint the player's resume chapter names, if it exists.
    pub fn player_checkpoint(&self) -> Option<Checkpoint> {
        let player = self.player.min(MAX_PLAYERS - 1);
        let name = self
            .players
            .checkpoint_name(player, self.players.chapters[player]);
        let data = self.saves.read(&name)?;
        let cp = Checkpoint::parse(&data);
        if cp.is_none() {
            log::warn!("{name}: not a checkpoint");
        }
        cp
    }

    /// `FUN_0042de5c`: the player's checkpoint rebuilds the game (the
    /// state read, the WAM and place set up, the chapter's CON loaded and
    /// its /INIT run, the volumes applied); without one, a new game at
    /// chapter 1 (`in_play`: from play, which keeps the rest of the state).
    pub async fn load_checkpoint(&mut self, hooks: &mut dyn ScriptHooks, in_play: bool) {
        // The walk that asked for the reload (a game over, the load picker)
        // set `abort`; the rebuild runs with it cleared, so that the
        // /INIT's commands (a Vue=, a video) are not cut short.
        self.abort = None;
        self.stop_ambient();
        self.bar_first = 0; // 0x0042e2ec
        self.vars.set("objetenmain", 0);
        self.bow_mode = false;
        self.bow_sprites.clear();
        let Some(cp) = self.player_checkpoint() else {
            self.new_game(hooks, in_play).await;
            return;
        };
        if !in_play {
            // FUN_00423760 on the boot pass restarts the timers.
            let now = self.now();
            self.clocks.time1_offset = 0;
            self.clocks.time2_offset = 0;
            self.clocks.time1_start = now;
        }
        self.disc = cp.disc.max(1);
        // The 25 fields: the hidden persos before the WAM's sprite list.
        self.hidden_persos = cp.hidden.iter().map(|&h| i32::from(h)).collect();
        let wam = cp.wam_name();
        if !self.load_wam(&wam).await {
            log::error!("checkpoint: {wam} missing, new game");
            self.new_game(hooks, in_play).await;
            return;
        }
        self.last_vue = cp.last_vue.clamp(0, i32::from(u16::MAX)) as u16;
        self.prev_chapter = cp.prev_chapter;
        self.chapter = u32::from(cp.chapter);
        self.current_place = None;
        self.current_place_id = None;
        self.inventory = cp.inventory.iter().map(|r| r.id).collect();
        // The follower-talk flag 0x00496144 is not in the checkpoint and the
        // reader leaves it as it was (no reference in `FUN_0042de5c`).
        let mut g = Guests {
            portrait_y: self.guests.portrait_y,
            talk: self.guests.talk,
            ..Guests::default()
        };
        g.count = usize::from(cp.guest_count).min(CHECKPOINT_GUESTS);
        for i in 0..CHECKPOINT_GUESTS {
            g.id[i] = cp.guests[i].0;
            g.sprite[i] = cp.guests[i].1;
        }
        self.guests = g;
        // The options and volumes the checkpoint holds (step 8).
        self.subtitles = cp.subtitles != 0;
        self.settings.subtitles = self.subtitles;
        self.settings.omni3d_mode2 = cp.omni3d != 0;
        self.settings.volumes = cp.volumes.map(|v| v.min(127));
        self.apply_volumes();
        // Step 8: the current music volume 0x00495e71 = the music volume.
        self.music.with_fade(|f| f.cur = f.vol);
        // Step 4: ZOOM = NEWSOUND = 0, the last NewSound 0.
        self.zoom = false;
        self.music.new_sound_last = 0;
        hooks.restore_script_state(
            self,
            &ScriptState {
                counters: cp.counters.clone(),
                inc_perso: cp.inc_perso,
                vars: cp.vars,
            },
        );
        self.vars.set("zoom", 0);
        self.vars.set("newsound", 0);
        // Step 7: the chapter change with the load flag (no checkpoint),
        // the CON and its /INIT on the loaded state.
        self.load_running = true;
        hooks
            .chapter_loaded(self, u32::from(cp.chapter), true)
            .await;
        self.load_running = false;
        self.next_place_id = u16::try_from(cp.place_id).ok();
        self.arrival_scan_pending = false;
        self.arrival_after_load = true;
        let player = self.player.min(MAX_PLAYERS - 1);
        let name = self.players.checkpoint_name(player, cp.chapter);
        self.emit(crate::engine::Event::CheckpointLoaded(name));
    }

    /// The settings of the store, or those of the options.
    pub(crate) fn load_settings(&mut self, fallback: Settings) {
        let s = Settings::load(self.saves.as_ref()).unwrap_or(fallback);
        self.settings = Settings {
            subtitles: self.subtitles,
            enhancements: s.enhancements,
            ..s
        };
        self.music.on_disk = self.settings.music_on_disk;
        self.apply_volumes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::MemStore;
    use crate::test_support::with_engine;
    use std::rc::Rc;

    /// The game's own save of its settings (its options page) keeps what
    /// the front end stored since start-up: the graphics mode, the HD
    /// options, the enhancements and full screen.
    #[test]
    fn the_settings_save_keeps_the_front_end_choices() {
        with_engine(
            "the_settings_save_keeps_the_front_end_choices",
            |mut e| async move {
                let store = Rc::new(MemStore::new());
                e.saves = store.clone();
                e.load_settings(Settings::default());
                let mut front = Settings {
                    graphics: crate::persist::Graphics::Hd,
                    fullscreen: true,
                    ..Settings::default()
                };
                front.hd.widescreen = true;
                front.hd.sharpen = false;
                front.enhancements.ui_ux = true;
                front.save(store.as_ref());
                e.subtitles = false;
                e.save_settings();
                let saved = Settings::load(store.as_ref()).unwrap();
                assert_eq!(
                    saved,
                    Settings {
                        subtitles: false,
                        ..front
                    }
                );
            },
            |_, _| {},
        );
    }
}
