//! *Atlantis Evolution*'s 2004 program's start-up logos, saves and options
//! file, the game's side (atlantis4-cd-en-3022848): the logo sequence of
//! loop mode 1 (FUN_0045e970), the saves' writer (FUN_0045f080) and loader
//! (FUN_00460b50) with the menu's thumbnail, and `Atlantis4.cfg`'s writer
//! (FUN_0040c940). The pages themselves are [`crate::menu::evolution`].
//!
//! The RE note is atlantis4-re `subsystems/cd2004-menus.md`.

use atlantis2_engine::script::a3::LoadData;
use atlantis2_engine::script::Script;
use atlantis3_data::evolution2004_save::{SaveGame2004, THUMB_MENU_WIDTH, THUMB_SIZE};
use atlantis3_data::save::{PersoDialogs, Subject};

use super::pages::{date_and_time_now, thumbnail_key};
use super::{angles_quaternion, Game, Mode};
use crate::menu::PAGE_START;

/// `logos.txt`'s names: one per line, the first word of every line that is
/// not empty and does not start with `//` (FUN_0045e970 @ 0x0045ea2a..
/// 0x0045eaa4: FUN_00449d70 reads a line, FUN_00449180 its first word).
pub fn logo_names(data: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(data);
    text.split(['\r', '\n'])
        .map(str::trim_start)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .filter_map(|l| l.split([' ', '\t', ',', ';', '(', ')']).next())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

impl Game {
    /// The start-up's logo sequence (loop mode 1, FUN_0045e970): the menu
    /// page 0x00712800 and its sub-state 0x007111e4 := 0, the player 0
    /// (FUN_00465d50(0) @ 0x004a243d), the names of
    /// `loc\text\<SousTitre>\logos.txt` (FUN_00419880), then each film
    /// played in turn by FUN_00402a70(name, "", 25, 0, 1) — the fifth
    /// argument takes it from `loc\bink\<Langue>\` — and its loop
    /// (FUN_00402fb0); at the end loop mode 7, the start page. A click,
    /// Space or Escape ends the logo playing ([`Game::skip`]); the next one
    /// plays. No `/INIT` runs: chapter 0's runs with the new game.
    pub fn start_logos_2004(&mut self) {
        self.player = 0;
        self.menus.page = PAGE_START;
        self.menus.sub = 0;
        let path = format!("loc/text/{}/logos.txt", self.startup.text_language());
        let mut names = self
            .files
            .read(&path)
            .map(|d| logo_names(&d))
            .unwrap_or_default();
        names.reverse();
        self.logos_2004 = Some(names);
        self.mode = Mode::Idle;
    }

    /// The start page's player (0x006b083c; 0 after the start-up).
    pub fn player(&self) -> i32 {
        self.player
    }

    /// Whether the logo sequence runs.
    pub fn logos_running_2004(&self) -> bool {
        self.logos_2004.is_some()
    }

    /// The logo sequence's next film, or its end: the start page.
    pub(super) fn next_logo_2004(&mut self) {
        while let Some(name) = self.logos_2004.as_mut().and_then(Vec::pop) {
            if self.film_start_2004(&name, 25, true) {
                return;
            }
        }
        self.logos_2004 = None;
        // FUN_004a2620(7) @ 0x0045eaf8.
        if !self.show_start_page() {
            self.mode = Mode::Idle;
        }
    }

    /// The 2004 new game of the start page's player (FUN_00457850): back
    /// to the game (FUN_00456ad0), then the chapter loaded and its `/INIT`
    /// run (FUN_004a2640).
    pub(super) fn new_game_at_2004(&mut self, chapter: i32) -> bool {
        self.video = None;
        self.film = None;
        self.film_cine = None;
        self.mode = Mode::Warp;
        let ok = self.with_script(|s, h| {
            s.close_interrupt(h);
            s.new_game_2004_at(h, chapter)
        });
        self.look_last = None;
        ok
    }

    /// The 2004 loader (FUN_00460b50) on the save `text`: the script's part
    /// ([`Script::load_2004`]), then the hide table (applied to the view's
    /// place animations) and the zone table (applied to the place's
    /// zones), then `SprPleinEcran` (a name not empty
    /// opened by FUN_00436790 @ 0x0046488c; the port opens `spr\` + its file part).
    pub(super) fn load_text_2004(&mut self, text: &[u8]) -> bool {
        let g = SaveGame2004::parse(text);
        let data = LoadData {
            current_cd: g.current_cd,
            wam: String::from_utf8_lossy(&g.wam).into_owned(),
            monde: g.monde,
            vue: g.vue,
            chapter: g.chapitre,
            flags: g.flags.clone(),
            objects: g.objects.clone(),
            subjects: g
                .dialogs
                .iter()
                .map(|p| {
                    (
                        p.perso,
                        p.subjects
                            .iter()
                            .map(|s| (s.etat, s.id, s.nb_sprite))
                            .collect(),
                    )
                })
                .collect(),
            camera: g.camera(),
        };
        self.puzzles.after_load();
        let vars = g.vars.clone();
        let ok = self.with_script(|s, h| s.load_2004(h, &data, &vars));
        if ok {
            self.load_hide_table_2004(&g.hide_table);
            self.load_zone_table_2004(&g.zone_table);
            let spr = String::from_utf8_lossy(&g.spr_plein_ecran).into_owned();
            let file = spr.rsplit(['\\', '/']).next().unwrap_or("").trim();
            if !file.is_empty() {
                atlantis2_engine::script::Host::spr_plein_ecran_2004(self, Some(file));
            }
        }
        self.after_load_hook();
        ok
    }

    /// The bag's object names as the writer lists them (FUN_0042b860): the
    /// one place the save code reads the bag.
    fn bag_names_2004(&self, s: &Script) -> Vec<Vec<u8>> {
        s.bag
            .iter()
            .filter_map(|e| usize::try_from(e.id).ok())
            .map(|i| {
                self.names
                    .name(atlantis2_data::defnames::OBJETS, i)
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The writer's text (FUN_0045f080) of the script's state.
    pub(super) fn save_text_2004(&self, s: &Script) -> Vec<u8> {
        let dialogs = s
            .subjects
            .characters()
            .map(|p| PersoDialogs {
                perso: p,
                subjects: (0..s.subjects.count(p).max(0) as usize)
                    .map(|i| {
                        let e = s.subjects.entry(p, i);
                        Subject {
                            etat: e.flag != 0,
                            id: e.index,
                            nb_sprite: e.value,
                        }
                    })
                    .collect(),
            })
            .collect();
        let (yaw, pitch) = self.camera.angles();
        let q = angles_quaternion(yaw, pitch);
        let (hide_table, zone_table) = self.save_tables_2004_of(s);
        let mut g = SaveGame2004 {
            current_cd: self.disc_2004.max(1),
            date_and_time: date_and_time_now(self.local_time()),
            wam: s.wam.clone(),
            monde: s.a3.world,
            vue: s.current_view.unwrap_or(0),
            chapitre: s.chapter,
            vars: s.vars.iter().copied().take(300).collect(),
            objects: self.bag_names_2004(s),
            dialogs,
            hide_table,
            zone_table,
            // The bank's name (0x0070c970): the path FUN_00418d50 built,
            // `spr\<name>` here (the original's exact prefix [UNKNOWN]).
            spr_plein_ecran: self
                .spr_plein_ecran
                .as_ref()
                .map(|(n, _)| format!("spr\\{n}").into_bytes())
                .unwrap_or_default(),
            quaternion: q.map(Some),
            ..Default::default()
        };
        g.set_flags_from_table(&s.flags);
        g.write()
    }

    /// The save page's `BT_YES` (FUN_0045c4b0 → FUN_0045f080): the save
    /// `name` and its one thumbnail (the Escape capture stretched to 85 x 65
    /// in x8r8g8b8 by FUN_00412f20, the dark pixels lifted by FUN_00412e70,
    /// encoded by the sprite library's FUN_00647790 after `SPRCRYO1`) for
    /// the 1024 menus.
    pub(super) fn write_menu_save_2004(&mut self, name: &str) {
        let Some(s) = self.script.take() else {
            return;
        };
        let text = self.save_text_2004(&s);
        self.script = Some(s);
        self.store_save(name, text);
        let crate::menu::Menus {
            capture, encoder, ..
        } = &mut self.menus;
        let Some((pixels, w, h)) = capture.as_ref() else {
            return;
        };
        let (tw, th) = THUMB_SIZE;
        let mut px = match pixels {
            crate::px::Surface::Rgb565(c) => {
                crate::menu::to_x888(&crate::menu::stretch(c, *w, *h, tw, th))
            }
            crate::px::Surface::Xrgb8888(c) => crate::menu::stretch(c, *w, *h, tw, th),
        };
        atlantis3_data::spr_encode::darken_black(&mut px);
        let mut bank = atlantis3_data::save::THUMB_MAGIC.to_vec();
        bank.extend(encoder.encode(&px, tw, th, tw, &[]));
        let key = thumbnail_key(THUMB_MENU_WIDTH, name);
        if let Some(dir) = &self.save_dir {
            let path = dir.join(&key);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, &bank) {
                log::warn!("{key}: {e}");
            }
        }
        self.saves_written.push(key.clone());
        self.saves.insert(key, bank);
    }

    /// `Atlantis4.cfg`'s text as FUN_0040c940 writes it, the play time
    /// counted first.
    pub(super) fn cfg_text_2004(&mut self) -> Vec<u8> {
        self.play_time_count();
        let lang = &self.startup.install_language;
        atlantis3_data::evolution2004_cfg::write(
            &self.startup.config,
            &self.puzzles_2004,
            lang,
            lang,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_list_of_logos_txt() {
        assert_eq!(
            logo_names(b"logoTAC.bik\r\nlogoAIE.bik\r\n// x\r\n\r\nlogo.bik"),
            ["logoTAC.bik", "logoAIE.bik", "logo.bik"]
        );
    }
}
