//! The saves (`AtlSaveGame_*`) and `atlant2.cfg`, in the original's
//! formats (`atlantis2_data::save`): what the 35 saved globals are in the
//! engine and the script, the save (FUN_00478df0 with the header refresh
//! FUN_00478eb0), the load (FUN_00478f30, the world restore FUN_00479130
//! and "init after load" FUN_004785a0), the slots of a player, and the
//! options file read at start-up and written at exit.

use std::path::{Path, PathBuf};

use atlantis2_data::save::{
    entry, read_save, save_file_name, scan_save, write_save, Cfg, InvalidSave, SaveData,
    SaveFormat, SaveHeader, BAG_ENTRIES, BAG_UNUSED, SHOWN_ROWS, SLOTS, SUBJECTS_PER_CHARACTER,
    SUBJECT_CHARACTERS, SUBJECT_RECORD,
};

use super::Engine;
use crate::script::{BagEntry, Shown, Subject};

/// The entries Native keeps only as bytes: the dowsing rod (13..16), the
/// puzzles (20..32) and the dialog flag (33).
const KEPT: [usize; 18] = [
    13, 14, 15, 16, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33,
];

/// Why a save could not be loaded.
#[derive(Debug)]
pub enum SaveError {
    /// No folder for the saves.
    NoFolder,
    /// No file in that slot (the original returns 0 silently).
    Missing,
    Invalid(InvalidSave),
    /// The scripts do not run this game.
    NoScripts,
    /// The saved chapter cannot be loaded (fatal in the original).
    Chapter(i32),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::NoFolder => write!(f, "no folder for the saves"),
            SaveError::Missing => write!(f, "no save in that slot"),
            SaveError::Invalid(e) => write!(f, "{e}"),
            SaveError::NoScripts => write!(f, "the scripts do not run"),
            SaveError::Chapter(c) => write!(f, "scenar\\chapi{c:03}.con cannot be loaded"),
        }
    }
}

impl std::error::Error for SaveError {}

impl Engine {
    /// The folder the saves (in `save\`) and `atlant2.cfg` are kept in,
    /// and `atlant2.cfg` read from it (FUN_0047f590 at start-up: a missing
    /// file leaves the defaults). The save folder is made as the start-up
    /// does (FUN_004791c0, without its empty probe file).
    pub fn set_data_dir(&mut self, dir: Option<PathBuf>) {
        self.data_dir = dir;
        if let Some(d) = &self.data_dir {
            if let Err(e) = std::fs::create_dir_all(d.join("save")) {
                log::warn!("{}: {e}", d.join("save").display());
            }
            if let Ok(b) = std::fs::read(d.join("atlant2.cfg")) {
                self.apply_cfg(&Cfg::parse(&b));
            }
        }
    }

    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }

    /// The options as `atlant2.cfg` holds them.
    pub fn cfg(&self) -> Cfg {
        Cfg {
            invert_y: i32::from(self.look.invert_y),
            sensitivity: self.look.sensitivity,
            fullscreen: self.cfg_extra.fullscreen,
            subtitles: u8::from(self.subtitles),
            centre_marker: u8::from(self.centre_marker),
            general: self.music.master,
            effects: self.fx_volume,
            music: self.music.max,
            dialogs: self.voice_volume,
            width: self.cfg_extra.width,
            height: self.cfg_extra.height,
        }
    }

    /// The options of `cfg` in force.
    pub fn apply_cfg(&mut self, cfg: &Cfg) {
        self.look.invert_y = cfg.invert_y != 0;
        self.look.sensitivity = cfg.sensitivity;
        self.subtitles = cfg.subtitles != 0;
        self.centre_marker = cfg.centre_marker != 0;
        self.music.master = cfg.general;
        self.fx_volume = cfg.effects;
        self.music.max = cfg.music;
        self.voice_volume = cfg.dialogs;
        self.cfg_extra = *cfg;
    }

    /// FUN_0047f530 at exit: `atlant2.cfg` written.
    pub fn write_cfg(&self) {
        if let Some(d) = &self.data_dir {
            let path = d.join("atlant2.cfg");
            if let Err(e) = std::fs::write(&path, self.cfg().to_bytes()) {
                log::warn!("{}: {e}", path.display());
            }
        }
    }

    /// The path of a slot of the current player.
    fn slot_path(&self, slot: u8) -> Option<PathBuf> {
        let d = self.data_dir.as_ref()?;
        Some(
            d.join("save")
                .join(save_file_name(self.player_number, slot)),
        )
    }

    /// The valid saves of the current player (FUN_00479250 on slots
    /// 0..98).
    pub fn saves(&self) -> Vec<(u8, SaveHeader)> {
        (0..SLOTS)
            .filter_map(|s| {
                let b = std::fs::read(self.slot_path(s)?).ok()?;
                Some((s, scan_save(&b)?))
            })
            .collect()
    }

    /// The globals of a save, as FUN_00478eb0 refreshes them and
    /// FUN_00478df0 copies them.
    pub fn save_data(&self) -> SaveData {
        let mut d = self.save_extra.clone();
        let Some(s) = self.con.as_deref() else {
            return d;
        };
        // 0..4: the header. The place's name, the chapter, the view (0
        // without one), the time and the thumbnail (`VignetteSave`).
        d.set_wam(self.wam_name.as_bytes());
        d.set_i32(entry::CHAPTER, s.chapter);
        let view = if self.faces[0].is_empty() {
            0
        } else {
            self.view
        };
        d.set_i32(entry::VIEW, view as i32);
        d.set_time((self.wall_clock)());
        d.set_i32(entry::VIGNETTE, s.var("VignetteSave").unwrap_or(0));
        // 5, 6: the variables and flags.
        for (i, v) in s.vars.iter().enumerate().take(300) {
            d.set_i32_at(entry::VARS, i, *v);
        }
        for (i, v) in s.flags.iter().enumerate().take(1024) {
            d.set_i32_at(entry::FLAGS, i, *v);
        }
        // 7, 8: the bag, the entries from the count on in their initial
        // pattern.
        for i in 0..BAG_ENTRIES {
            let e = d.bag_entry_mut(i);
            match s.bag.get(i) {
                Some(b) => {
                    e[0] = b.sprite;
                    e[1..4].fill(0x63);
                    e[4..8].copy_from_slice(&b.id.to_le_bytes());
                    e[8..12].copy_from_slice(&b.world.to_le_bytes());
                }
                None => e.copy_from_slice(&BAG_UNUSED),
            }
        }
        d.set_i32(entry::BAG_COUNT, s.bag.len() as i32);
        // 9, 10: the camera's angles (FUN_0045adf0), 11, 12: the music.
        let (yaw, pitch) = self.camera.angles();
        d.set_f32(entry::YAW, yaw);
        d.set_f32(entry::PITCH, pitch);
        d.set_i32(entry::MUSIC, self.player_music());
        let list = d.entry_mut(entry::MUSIC_LIST);
        list.fill(0);
        let n = s.music.len().min(255);
        list[..n].copy_from_slice(&s.music[..n]);
        // 17, 18: the show/hide table.
        let table = d.entry_mut(entry::SHOWN);
        table.fill(0);
        for (i, r) in s.shown.iter().take(SHOWN_ROWS).enumerate() {
            let row = &mut table[12 * i..12 * i + 12];
            row[..4].copy_from_slice(&r.id.to_le_bytes());
            row[4..8].copy_from_slice(&i32::from(r.shown).to_le_bytes());
            row[8..].copy_from_slice(&r.kind.to_le_bytes());
        }
        d.set_i32(entry::SHOWN_COUNT, s.shown.len() as i32);
        // 19: the cursor's frame (FUN_0047bf20: 0 when it has no bank).
        d.set_i32(entry::CURSOR, self.cursor.shown.unwrap_or(0));
        // 33: the dialog flag.
        d.entry_mut(entry::DIALOG)[0] = u8::from(self.dialog.is_some());
        // 34: the subjects, per character a count and 16 entries.
        for c in 0..SUBJECT_CHARACTERS {
            let rec = d.subject_record_mut(c);
            rec.fill(0);
            let Some(list) = s.subjects.get(c) else {
                continue;
            };
            rec[..4].copy_from_slice(&(list.len() as i32).to_le_bytes());
            for (i, sub) in list.iter().take(SUBJECTS_PER_CHARACTER).enumerate() {
                let e = &mut rec[4 + 12 * i..16 + 12 * i];
                e[..4].copy_from_slice(&sub.value.to_le_bytes());
                e[4..8].copy_from_slice(&sub.index.to_le_bytes());
                e[8] = u8::from(sub.enabled);
            }
        }
        d
    }

    /// The music playing for the save (@ 0x00673f30's copy).
    fn player_music(&self) -> i32 {
        self.player.current
    }

    /// Saves slot `slot` of the current player (FUN_00478df0). False when
    /// the file cannot be written (the original only logs it).
    pub fn save_slot(&mut self, slot: u8) -> bool {
        let Some(path) = self.slot_path(slot) else {
            log::warn!("no folder for the saves: slot {slot} not saved");
            return false;
        };
        let file = write_save(&self.save_data());
        match std::fs::write(&path, file) {
            Ok(()) => {
                log::info!("saved {}", path.display());
                true
            }
            Err(e) => {
                log::warn!("ZapFile::Save({}): {e}", path.display());
                false
            }
        }
    }

    /// Deletes slot `slot` of the current player (FUN_00479260).
    pub fn delete_slot(&mut self, slot: u8) {
        if let Some(p) = self.slot_path(slot) {
            if p.exists() {
                if let Err(e) = std::fs::remove_file(&p) {
                    log::warn!("{}: {e}", p.display());
                }
            }
        }
    }

    /// Loads slot `slot` of the current player (`AtlSaveGame_Load`
    /// @ 0x00478f30); a raw file is saved again at once as a `ZapFile`.
    pub fn load_slot(&mut self, slot: u8) -> Result<(), SaveError> {
        let path = self.slot_path(slot).ok_or(SaveError::NoFolder)?;
        let file = std::fs::read(&path).map_err(|_| SaveError::Missing)?;
        let (data, format) = read_save(&file).map_err(|e| {
            log::warn!("AtlSaveGame_Load({slot}) {e}");
            SaveError::Invalid(e)
        })?;
        self.load_data(&data)?;
        if format == SaveFormat::Raw {
            self.save_slot(slot);
        }
        Ok(())
    }

    /// The load of a save's globals (FUN_00478f30 from step 2): the
    /// puzzles reset, the 35 entries copied back, then the world restore
    /// (FUN_00479130) and "init after load" (FUN_004785a0). The view comes
    /// back at the saved angles with no arrival scan and no per-view reset.
    pub fn load_data(&mut self, d: &SaveData) -> Result<(), SaveError> {
        if self.con.is_none() {
            return Err(SaveError::NoScripts);
        }
        // FUN_0049bf70 resets the puzzles (not ported: their saved bytes
        // are kept for the next save).
        for k in KEPT {
            self.save_extra.entry_mut(k).copy_from_slice(d.entry(k));
        }
        let header = d.header();
        let s = self.con.as_deref_mut().expect("checked above");
        // 1, 5, 6: the chapter, the variables, the flags.
        s.chapter = d.i32(entry::CHAPTER);
        for (i, v) in s.vars.iter_mut().enumerate().take(300) {
            *v = d.i32_at(entry::VARS, i);
        }
        for (i, v) in s.flags.iter_mut().enumerate().take(1024) {
            *v = d.i32_at(entry::FLAGS, i);
        }
        // 7, 8: the bag.
        let count = d.i32(entry::BAG_COUNT).clamp(0, BAG_ENTRIES as i32) as usize;
        s.bag = (0..count)
            .map(|i| {
                let e = d.bag_entry(i);
                BagEntry {
                    sprite: e[0],
                    id: i32::from_le_bytes(e[4..8].try_into().unwrap()),
                    world: i32::from_le_bytes(e[8..12].try_into().unwrap()),
                }
            })
            .collect();
        // 12: the music list.
        let list = d.entry(entry::MUSIC_LIST);
        let n = list.iter().position(|&c| c == 0).unwrap_or(list.len());
        s.music = list[..n].to_vec();
        // 17, 18: the show/hide table.
        let rows = d.i32(entry::SHOWN_COUNT).clamp(0, SHOWN_ROWS as i32) as usize;
        let table = d.entry(entry::SHOWN);
        let i32_at = |o: usize| i32::from_le_bytes(table[o..o + 4].try_into().unwrap());
        s.shown = (0..rows)
            .map(|i| Shown {
                id: i32_at(12 * i),
                shown: i32_at(12 * i + 4) != 0,
                kind: i32_at(12 * i + 8),
            })
            .collect();
        // 34: the subjects.
        for (c, list) in s.subjects.iter_mut().enumerate() {
            let rec = &d.entry(entry::SUBJECTS)[SUBJECT_RECORD * c..SUBJECT_RECORD * (c + 1)];
            let n = i32::from_le_bytes(rec[..4].try_into().unwrap())
                .clamp(0, SUBJECTS_PER_CHARACTER as i32) as usize;
            *list = (0..n)
                .map(|i| {
                    let e = &rec[4 + 12 * i..16 + 12 * i];
                    Subject {
                        value: i32::from_le_bytes(e[..4].try_into().unwrap()),
                        index: i32::from_le_bytes(e[4..8].try_into().unwrap()),
                        enabled: e[8] != 0,
                    }
                })
                .collect();
        }
        // What the save does not keep: a section waiting to go on and a
        // queued command are dropped [port choice: the original's flag
        // @ 0x006d1699 is not in the table].
        s.resume_pending = false;
        s.queued = None;
        let shown = s.shown.clone();
        let bag: Vec<i32> = s.bag.iter().map(|b| b.id).collect();
        self.anim.table.0 = shown
            .iter()
            .map(|r| (r.id, i32::from(r.shown), r.kind))
            .collect();
        self.bag = bag;

        // FUN_00479130: `CHAPTER`, the chapter's script, no current view,
        // the world (FUN_00484df0(MONDE, 0): the hand emptied, the banks,
        // the texts and the disc, no section resumed).
        let chapter = header.chapter;
        let loaded = self
            .with_script(|s, h| {
                s.chapter = chapter;
                s.vars[s.keys.chapter] = chapter;
                if !s.load_chapter(h, chapter) {
                    return false;
                }
                s.current_view = None;
                let monde = s.vars[s.keys.monde];
                s.set_monde(h, monde, false);
                true
            })
            .unwrap_or(false);
        if !loaded {
            log::error!("load: scenar\\chapi{chapter:03}.con cannot be loaded");
            return Err(SaveError::Chapter(chapter));
        }
        // FUN_004785a0: the saved angles are the next view entry's.
        self.arrival = (d.f32(entry::YAW), d.f32(entry::PITCH));
        let mut view = header.view;
        let monde = self.con.as_ref().map_or(0, |s| s.vars[s.keys.monde]);
        if monde == 6 && 0xc5 < view && view < 0x10a {
            // In the spider web (@ 0x0048f450): back to view 198; the bag
            // reconciliation of the puzzle is not ported.
            view = 198;
        }
        // FUN_0040fc60(WAM, view): the place loaded, the view made current
        // (`LASTVUE`, `VUE`) and entered at those angles.
        let wam: String = header.wam.iter().map(|&c| char::from(c)).collect();
        self.turn = None;
        self.move_pending = false;
        self.next_view = None;
        self.video = None;
        self.click = false;
        self.with_script(|s, h| {
            s.wam = header.wam.clone();
            h.load_wam(&wam);
            s.set_current_view(view);
            s.view_number = view;
        });
        let (yaw, pitch) = self.arrival;
        self.vars.last_vue = 0;
        if let Err(e) = self.enter_view(view as u32, yaw, pitch) {
            log::error!("load: {e}");
        }
        if let Some(s) = self.con.as_deref() {
            let k = &s.keys;
            self.vars.vue = s.vars[k.vue] as u32;
            self.vars.objet_en_main = s.vars[k.objetenmain];
            self.vars.cliczone = s.vars[k.cliczone];
            self.vars.clicperso = s.vars[k.clicperso];
            self.vars.clicobjet = s.vars[k.clicobjet];
        }
        // The saved music again (FUN_0047a200(music, −1, 0)) when not 0.
        let music = d.i32(entry::MUSIC);
        if music != 0 {
            self.player.list = self.con.as_ref().map_or(Vec::new(), |s| s.music.clone());
            self.play_music(music, -1, 0);
        }
        // The cursor's frame (FUN_0047bfa0, only with a bank).
        if self.cursor.shown.is_some() {
            self.cursor.shown = Some(d.i32(entry::CURSOR));
        }
        // Chapters 39, 46 and 83 run their `/INIT` again.
        if matches!(chapter, 39 | 46 | 83) {
            self.with_script(|s, h| s.run_init(h));
        }
        log::info!("Loaded game: wam={wam}, chapitre={chapter}, vue={view}");
        match self.script_error() {
            Some(e) => {
                log::error!("load: {e}");
                Err(SaveError::Chapter(chapter))
            }
            None => Ok(()),
        }
    }
}
