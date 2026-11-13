//! *Atlantis Evolution*'s 2004 program's menu pages (atlantis4-cd-en-3022848).
//!
//! The menu frame FUN_0045e790 (loop mode 7, *Atlantis III*'s 0x004591a0
//! with mode 7 for 6) dispatches on the page 0x00712800 as *Atlantis III*
//! does: 0 `i_debut` FUN_00457850, 1 `i_princ` FUN_00459ea0, 2 `i_load`
//! FUN_00458b30 (+ FUN_00459af0 after the copy), 4 `i_outil` FUN_0045a230,
//! 5 `i_option` FUN_0045a7d0, 6 `i_langue_full` FUN_0045b8c0, 8 `i_save`
//! FUN_0045c4b0, 9 `i_credit` FUN_0045c7f0. The pages' interfaces are made
//! at start-up by FUN_00456e40 from `menu\desc\<page>.atl` (banks from
//! `menu\spr_<screen width>\32bits\`, the CD has `spr_1024` only) and
//! `loc\text\<SousTitre>\<page>.txt`.
//!
//! What differs from *Atlantis III*'s pages ([`super`]):
//!
//! - the elements: every button is a `BT_ANIME3` (hover animation, pressed
//!   frame) at an explicit `pos`, the click sound is a `#SONS#` name;
//! - no opening animations: `i_princ`'s covers and `yes_no` are gone, a
//!   page set by the in-game menu is shown at once (sub-state 0 sets it
//!   up);
//! - the start page's five players start chapters 0, 999, 998, 997, 996
//!   (copies of chapter 0's `/INIT`), or load their newest save;
//! - the load page has four slots and scrolls by four saves;
//! - the volume sliders have 15 steps (× 0.15 / × 6.6666665).
//!
//! The RE note is atlantis4-re `subsystems/cd2004-menus.md`.

use atlantis3_data::atl::menu_sprite_path;
use atlantis3_data::save::game_save_name;
use atlantis3_data::spr::Bank;

use super::{
    date_of, ftol, Env, Event, Menus, EMPTY, PAGE_LANGUAGES, PAGE_LOAD, PAGE_MAIN, PAGE_OPTIONS,
    PAGE_SAVE, PAGE_START, PAGE_VOLUMES,
};
use crate::text::{FontId, Zone};

/// The chapter each start-page player's new game loads (FUN_00457850:
/// `BT_LOAD1` `CHAPTER` = 0 @ 0x00457b4a, `BT_LOAD2` 0x3e7 @ 0x00457e09,
/// `BT_LOAD3` 0x3e6 @ 0x004580bc, `BT_LOAD4` 0x3e5 @ 0x0045836d,
/// `BT_LOAD5` 0x3e4 @ 0x00458782). `chapi996.con` .. `chapi999.con` are
/// byte-identical copies of the start of chapter 0 (1,132 bytes, the same
/// `/INIT`).
pub const NEW_GAME_CHAPTERS: [i32; 5] = [0, 999, 998, 997, 996];

/// The load page's slots (`SAVE1` .. `SAVE4`, FUN_00458b30's loop to 4).
pub const LOAD_SLOTS: i32 = 4;

/// The volumes' slider factors (0x006994ac, 0x006994b0, f32).
const SLIDER_OF_VOLUME: f32 = 0.15;
const VOLUME_OF_SLIDER: f32 = 6.666_666_5;

impl Menus {
    /// Page 0 (FUN_00457850): hover, click, element pass, FUN_00431d10(0);
    /// nothing more while the sub-state is not 0. A click on `BT_LOAD<n>`:
    /// the play time counted to the previous player, player n (FUN_00465d50),
    /// the resets (flags and `GAMEOVER` FUN_004569e0, the object list
    /// FUN_0042b960, the dialog table FUN_0043eb20, FUN_0043d710), then the
    /// newest `game<n>_<k>.sav` of the run k = 1, 2, … (FUN_00465350,
    /// `Rien` when `_1` is missing): loaded (FUN_00460b50), page 1, back to
    /// the game (FUN_00456ad0); none: the new game at the player's chapter
    /// ([`NEW_GAME_CHAPTERS`]: the bag, the variables and the flags
    /// cleared, `LIMITEANGLEWARPY` 50, `LIMITEANGLEWARPX` −1, back to the
    /// game, then the chapter loaded and its `/INIT` run, FUN_004a2640).
    /// Player 5 with `SuperCine` runs `(chapter=666)` instead (the CD has
    /// no `chapi666.con`). `BT_QUIT`: the credits (FUN_0045cc20). Once a
    /// player is set, `i_princ`'s `BT_LOAD`, `BT_SAVE` and `BT_BACK` are on
    /// (@ 0x0045885f..0x00458b12).
    pub(super) fn page_start_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        let (_, c) = self.run(PAGE_START, env, false, ev);
        if self.sub != 0 {
            return;
        }
        let mut player = env.player;
        match self.name(PAGE_START, c) {
            None => {
                self.clear(PAGE_START);
                self.clear_flags();
            }
            Some(n) if n.starts_with(b"BT_LOAD") && n.len() == 8 => {
                self.clear(PAGE_START);
                let k = i32::from(n[7].wrapping_sub(b'0'));
                if (1..=5).contains(&k) {
                    player = k;
                    ev.push(Event::Player(k));
                    if k == 5 && env.config.super_cine {
                        self.demo = true;
                        ev.push(Event::Command(b"(chapter=666)".to_vec()));
                        ev.push(Event::Resume);
                    } else {
                        let mut newest = None;
                        for s in 1.. {
                            let name = game_save_name(k, s);
                            if !env.exists(&name, ev) {
                                break;
                            }
                            newest = Some(name);
                        }
                        match newest {
                            Some(name) => {
                                ev.push(Event::Load(name));
                                self.page = PAGE_MAIN;
                                self.sub = 0;
                                ev.push(Event::Resume);
                            }
                            None => ev.push(Event::NewGameAt(NEW_GAME_CHAPTERS[(k - 1) as usize])),
                        }
                    }
                }
            }
            Some(n) if n == b"BT_QUIT" => {
                self.clear(PAGE_START);
                ev.push(Event::Quit);
            }
            Some(_) => self.clear(PAGE_START),
        }
        if player != 0 {
            for name in [&b"BT_LOAD"[..], b"BT_SAVE", b"BT_BACK"] {
                if let Some(it) = self.item(PAGE_MAIN, name) {
                    it.el.selectable = true;
                    it.el.clickable = true;
                }
            }
        }
    }

    /// Page 1, the in-game menu (FUN_00459ea0): tooltips on (0x007127c0),
    /// hover, click, element pass; a click writes `Atlantis4.cfg`
    /// (FUN_0040c940) then: `BT_LOAD` the save list (FUN_00459540) and page
    /// 2, `BT_SAVE` page 8, `BT_OUTIL` page 4, `BT_OPTION` page 5 (sub-state
    /// 0, shown at once: no cover animation), `BT_BACK`
    /// `Interp_CloseInterrupt(1)` and back to the game (FUN_00456ad0, which
    /// enters the view again only after a colour depth change, 0x007129b5),
    /// `BT_QUIT` the start page (page 0, loop mode 7, 0x00719b54 := 1).
    pub(super) fn page_main_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        self.set_tooltips(PAGE_MAIN, true);
        let (_, c) = self.run(PAGE_MAIN, env, false, ev);
        let Some(name) = self.name(PAGE_MAIN, c) else {
            self.clear(PAGE_MAIN);
            self.clear_flags();
            return;
        };
        ev.push(Event::WriteCfg);
        self.clear(PAGE_MAIN);
        match name.as_slice() {
            b"BT_LOAD" => {
                self.build_list_2004(env, ev);
                self.page = PAGE_LOAD;
                self.sub = 0;
            }
            b"BT_SAVE" => {
                self.page = PAGE_SAVE;
                self.sub = 0;
            }
            b"BT_OUTIL" => {
                self.page = PAGE_VOLUMES;
                self.sub = 0;
            }
            b"BT_OPTION" => {
                self.page = PAGE_OPTIONS;
                self.sub = 0;
            }
            b"BT_BACK" => {
                self.clear_flags();
                ev.push(Event::Resume);
                if std::mem::take(&mut self.depth_changed) {
                    ev.push(Event::ReEnter);
                }
                return;
            }
            b"BT_QUIT" => {
                self.page = PAGE_START;
                self.sub = 0;
                self.clear_flags();
                ev.push(Event::StartPage);
                return;
            }
            _ => {}
        }
        self.clear_flags();
    }

    /// The save list (FUN_00459540): the 256 entries emptied (`...`), then
    /// for N = 256 down to 1 every existing `game<P>_<N>.sav` with its
    /// `DateAndTime` (FUN_00465740), newest first; the count 0x0071113c.
    pub(super) fn build_list_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        self.names = vec![EMPTY.to_string(); 256];
        self.dates = vec![EMPTY.as_bytes().to_vec(); 256];
        self.count = 0;
        for n in (1..=256).rev() {
            let name = game_save_name(env.player, n);
            if env.exists(&name, ev) {
                self.dates[self.count] = env
                    .host
                    .read(&name)
                    .map(|d| date_of(&d))
                    .unwrap_or_default();
                self.names[self.count] = name;
                self.count += 1;
            }
        }
    }

    /// Page 2, the load page (FUN_00458b30). Sub-state 0: sub-state 1, the
    /// tooltips of pages 1 and 2 on, the scroll 0x007129b0 := 0 and
    /// `SL_LOAD` at frame 0, pressed bytes and click flags cleared, the
    /// layout by screen width (unused by the 2004 slots); nothing drawn.
    /// Sub-state 1: hover, drag (FUN_00427460), click, element pass; a drag
    /// of `SL_LOAD` sets the scroll to its frame; otherwise a click clears
    /// the bytes, `BT_BACK` → page 1 (its tooltip element cleared), `SAVEk`
    /// (k = 1..4) loads `game<P>_<count − 4 × scroll − (k − 1)>.sav` when it
    /// exists (after the flags, the object list and the dialog table's
    /// resets), then page 1 and back to the game.
    pub(super) fn page_load_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        if self.sub == 0 {
            self.sub = 1;
            self.set_tooltips(PAGE_MAIN, true);
            self.set_tooltips(PAGE_LOAD, true);
            self.scroll = 0;
            if let Some(it) = self.item(PAGE_LOAD, b"SL_LOAD") {
                it.frame = 0;
            }
            self.clear(PAGE_LOAD);
            self.clear_flags();
            return;
        }
        let (d, c) = self.run(PAGE_LOAD, env, true, ev);
        if let Some(d) = d {
            if let Some(it) = self.get(PAGE_LOAD).map(|p| &p.items[d]) {
                if it.el.nomref == b"SL_LOAD" {
                    self.scroll = it.frame;
                }
            }
            return;
        }
        let Some(name) = self.name(PAGE_LOAD, c) else {
            self.clear(PAGE_LOAD);
            self.clear_flags();
            return;
        };
        if name == b"BT_BACK" {
            self.page = PAGE_MAIN;
            self.sub = 0;
            if let Some(p) = self.get_mut(PAGE_LOAD) {
                p.tip_el = None;
            }
            self.clear(PAGE_LOAD);
            self.clear_flags();
        }
        for k in 1..=LOAD_SLOTS {
            if name != format!("SAVE{k}").as_bytes() {
                continue;
            }
            let n = self.count as i32 - LOAD_SLOTS * self.scroll - (k - 1);
            let save = game_save_name(env.player, n);
            if env.exists(&save, ev) {
                ev.push(Event::Load(save));
                self.page = PAGE_MAIN;
                self.sub = 0;
                ev.push(Event::Resume);
            }
            self.clear(PAGE_LOAD);
            self.clear_flags();
        }
    }

    /// The load page's slots after the page is on the screen (FUN_00459af0):
    /// for k = 1..4, `SPR_SAVEk` hidden, its save's date (entry 4 × scroll
    /// \+ k − 1, `...` when empty) printed in `AtlFont_Default`, white, with
    /// the layout (x + 20, 824 − x, y, 791 − y, ·, bottom) from the
    /// element's position (+0x20, +0x24): the date's bottom 23 pixels above
    /// the thumbnail; then a used slot shows `BACKSPR_SAVEk` and
    /// `SPR_SAVEk` with the bank `save\<name>` (loaded again when the name
    /// changed, FUN_00425d80), an empty one hides `BACKSPR_SAVEk`. The
    /// layout's horizontal code 4 is `[UNKNOWN]` (FUN_004211d0 does not
    /// read it): left-aligned here, as the 2004 dialogs' port.
    pub(super) fn load_slots_2004(&mut self, env: &mut Env<'_>, screen: &mut crate::px::Surface) {
        let (w, files, depth) = (self.width, env.files, self.depth);
        for k in 1..=LOAD_SLOTS {
            let i = (LOAD_SLOTS * self.scroll + k - 1).max(0) as usize;
            let spr_name = format!("SPR_SAVE{k}");
            let Some(it) = self.item(PAGE_LOAD, spr_name.as_bytes()) else {
                continue;
            };
            it.el.displayed = false;
            let (x, y) = (it.el.pos[0], it.el.pos[1]);
            let date = self.dates.get(i).cloned().unwrap_or_else(|| EMPTY.into());
            let zone = Zone::new(x + 0x14, 0x338 - x, y, 0x317 - y, 0, 1);
            self.print(env, screen, FontId::Default, &date, zone, 0xff_ffff);
            let name = self.names.get(i).cloned().unwrap_or_else(|| EMPTY.into());
            let used = name != EMPTY;
            if let Some(b) = self.item(PAGE_LOAD, format!("BACKSPR_SAVE{k}").as_bytes()) {
                b.el.displayed = used;
            }
            if !used {
                continue;
            }
            let host = &mut *env.host;
            let Some(it) = self.item(PAGE_LOAD, spr_name.as_bytes()) else {
                continue;
            };
            let spr = format!("save\\{name}").into_bytes();
            if it.el.nomspr.as_deref() != Some(&spr[..]) {
                it.bank = host
                    .thumbnail(w, &name)
                    .or_else(|| {
                        let p = menu_sprite_path(w as i32, &spr);
                        files.read(p.trim_start_matches("Datas/"))
                    })
                    .and_then(|d| Bank::parse(&d, depth).ok());
                it.el.nomspr = Some(spr);
            }
            it.el.displayed = true;
        }
    }

    /// Page 4, the volumes (FUN_0045a230). Sub-state 0: the tooltips of
    /// pages 1 and 4 off, each slider at `ftol(volume × 0.15)` (voices,
    /// effects: integers; music, master: floats), sub-state 1 and the
    /// tooltips on, then on in the same frame. Sub-state 1: hover, drag,
    /// click, element pass; a dragged slider sets its volume to `frame ×
    /// 6.6666665` (`ftol` for voices and effects; the setters clamp to
    /// 0..99) and returns; `BT_BACK` → page 1 (its tooltip element
    /// cleared); then the pressed bytes and click flags cleared.
    pub(super) fn page_volumes_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        const SLIDERS: [&[u8]; 4] = [b"SL_VOICE", b"SL_SFX", b"SL_MUSIC", b"SL_MASTER"];
        if self.sub == 0 {
            let c = &*env.config;
            let vols = [
                c.voices_volume,
                c.sfx_volume,
                c.music_volume,
                c.global_volume,
            ];
            for (name, v) in SLIDERS.iter().zip(vols) {
                if let Some(it) = self.item(PAGE_VOLUMES, name) {
                    it.frame = ftol(f64::from(v as f32 * SLIDER_OF_VOLUME));
                }
            }
            self.sub = 1;
            self.set_tooltips(PAGE_MAIN, true);
            self.set_tooltips(PAGE_VOLUMES, true);
        }
        let (d, c) = self.run(PAGE_VOLUMES, env, true, ev);
        if let Some(d) = d {
            let (name, f) = {
                let it = &self.get(PAGE_VOLUMES).unwrap().items[d];
                (it.el.nomref.clone(), it.frame)
            };
            let v = ftol(f64::from(f as f32 * VOLUME_OF_SLIDER)).clamp(0, 99);
            let cfg = &mut *env.config;
            match name.as_slice() {
                b"SL_VOICE" => cfg.voices_volume = v,
                b"SL_SFX" => cfg.sfx_volume = v,
                b"SL_MUSIC" => cfg.music_volume = v,
                b"SL_MASTER" => cfg.global_volume = v,
                _ => return,
            }
            ev.push(Event::Volumes);
            self.clear(PAGE_VOLUMES);
            return;
        }
        if self.name(PAGE_VOLUMES, c).as_deref() == Some(b"BT_BACK") {
            self.page = PAGE_MAIN;
            self.sub = 0;
            if let Some(p) = self.get_mut(PAGE_VOLUMES) {
                p.tip_el = None;
            }
        }
        self.clear(PAGE_VOLUMES);
        self.clear_flags();
    }

    /// Page 5, the options (FUN_0045a7d0). Sub-state 0 (@ 0x0045b197..):
    /// sub-state 1, the tooltips of pages 1 and 5 on, page 1's bytes and
    /// the flags cleared, no size pending, the radios set from the state
    /// (`BT_SMOOTH` / `BT_NOSMOOTH`, `BT_16BITS` / `BT_32BITS`: the active
    /// one at its last frame, neither selectable nor clickable), no depth
    /// pending (0x007129b5), the toggles `BT_VISEUR`, `BT_SOURIS`,
    /// `BT_TITRE` (and `BT_TEXTURE`, absent from the 2004 data) at 2 when
    /// on, else 0; nothing drawn. Sub-state 1: *Atlantis III*'s reactions
    /// (the toggles set their option to `frame ≠ 0` at once, the depth on
    /// `BT_BACK`); `BT_BACK` → page 1.
    pub(super) fn page_options_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        if self.sub == 0 {
            self.sub = 1;
            self.set_tooltips(PAGE_MAIN, true);
            self.set_tooltips(PAGE_OPTIONS, true);
            self.clear(PAGE_MAIN);
            self.clear_flags();
            self.res_pending = 0xffff;
            let cfg = &*env.config;
            let (smooth, bits16) = (cfg.smooth, cfg.pixel_format.bits() == 16);
            let toggles = [
                (&b"BT_VISEUR"[..], cfg.cursor),
                (&b"BT_SOURIS"[..], cfg.invert_mouse_y),
                (&b"BT_TITRE"[..], cfg.subtitle_on),
                (&b"BT_TEXTURE"[..], cfg.warp_res != 512),
            ];
            let radio = |m: &mut Menus, on: &[u8], off: &[u8]| {
                if let Some(it) = m.item(PAGE_OPTIONS, on) {
                    it.frame = it.count() - 1;
                    it.el.selectable = false;
                    it.el.clickable = false;
                }
                if let Some(it) = m.item(PAGE_OPTIONS, off) {
                    it.frame = 0;
                    it.el.selectable = true;
                    it.el.clickable = true;
                }
            };
            if smooth {
                radio(self, b"BT_SMOOTH", b"BT_NOSMOOTH");
            } else {
                radio(self, b"BT_NOSMOOTH", b"BT_SMOOTH");
            }
            if bits16 {
                radio(self, b"BT_16BITS", b"BT_32BITS");
            } else {
                radio(self, b"BT_32BITS", b"BT_16BITS");
            }
            self.depth_pending = false;
            self.format = self.depth;
            for (name, on) in toggles {
                if let Some(it) = self.item(PAGE_OPTIONS, name) {
                    it.frame = if on { 2 } else { 0 };
                }
            }
            return;
        }
        let (_, c) = self.run(PAGE_OPTIONS, env, false, ev);
        let Some(name) = self.name(PAGE_OPTIONS, c) else {
            self.clear(PAGE_OPTIONS);
            self.clear_flags();
            return;
        };
        let frame = c
            .and_then(|k| self.get(PAGE_OPTIONS).map(|p| p.items[k].frame))
            .unwrap_or(0);
        let cfg = &mut *env.config;
        match name.as_slice() {
            b"BT_BACK" => {
                self.page = PAGE_MAIN;
                self.sub = 0;
                if let Some(p) = self.get_mut(PAGE_OPTIONS) {
                    p.tip_el = None;
                }
                self.clear(PAGE_OPTIONS);
                self.clear_flags();
                if self.depth_pending {
                    // @ 0x0045a8e4..0x0045a9ff: the display and the page
                    // surface re-created; 0x007129b5 stays set until the
                    // back to the game enters the view again.
                    ev.push(Event::DisplayChange(self.width as i32, self.height as i32));
                    self.depth_pending = false;
                    self.depth_changed = true;
                }
                return;
            }
            b"BT_LANGUE" => {
                self.page = PAGE_LANGUAGES;
                self.sub = 0;
            }
            b"BT_SMOOTH" | b"BT_NOSMOOTH" => {
                cfg.smooth = !cfg.smooth;
                ev.push(Event::Options);
            }
            b"BT_16BITS" => {
                self.format = atlantis3_data::spr::Depth::Rgb565;
                self.depth_pending = true;
            }
            b"BT_32BITS" => {
                self.format = atlantis3_data::spr::Depth::Xrgb8888;
                self.depth_pending = true;
            }
            b"BT_VISEUR" => {
                cfg.cursor = frame != 0;
                ev.push(Event::Options);
            }
            b"BT_SOURIS" => {
                cfg.invert_mouse_y = frame != 0;
                ev.push(Event::Options);
            }
            b"BT_TITRE" => {
                cfg.subtitle_on = frame != 0;
                ev.push(Event::Options);
            }
            _ => {}
        }
        self.clear(PAGE_OPTIONS);
        self.clear_flags();
    }

    /// Page 8, the save confirmation (FUN_0045c4b0). Sub-state 0: page 1's
    /// tooltips off and page 8's on, sub-state 1, bytes and flags cleared,
    /// `i_princ`'s `BT_SAVE` not hovered; nothing drawn. Sub-state 1: hover
    /// and click on `i_save`, the element passes of `i_princ` then
    /// `i_save`; `BT_NO` → page 1; `BT_YES` the next free
    /// `game<P>_<N>.sav` (FUN_00464f90) written (FUN_0045f080), page 1; both
    /// put page 1's and page 8's tooltips on.
    pub(super) fn page_save_2004(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        if self.sub == 0 {
            self.set_tooltips(PAGE_MAIN, false);
            self.set_tooltips(PAGE_SAVE, true);
            self.sub = 1;
            self.clear(PAGE_MAIN);
            self.clear_flags();
            if let Some(it) = self.item(PAGE_MAIN, b"BT_SAVE") {
                it.hovered = false;
            }
            return;
        }
        let input = self.input;
        let c = match self.get_mut(PAGE_SAVE) {
            Some(p) => {
                p.hover(env.pointer, env.now);
                p.click(input, env.pointer, env.now, ev)
            }
            None => None,
        };
        self.pass_only(PAGE_MAIN, env);
        self.pass_only(PAGE_SAVE, env);
        if let Some(name) = self.name(PAGE_SAVE, c) {
            if name == b"BT_YES" {
                let player = env.player;
                let save = (1..)
                    .map(|n| game_save_name(player, n))
                    .find(|s| !env.exists(s, ev))
                    .unwrap_or_default();
                ev.push(Event::Save(save));
            }
            if name == b"BT_YES" || name == b"BT_NO" {
                self.page = PAGE_MAIN;
                self.sub = 0;
                self.set_tooltips(PAGE_MAIN, true);
                self.set_tooltips(PAGE_SAVE, true);
            }
        }
        self.clear(PAGE_SAVE);
        self.clear_flags();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_sliders_have_fifteen_steps() {
        assert_eq!(ftol(f64::from(99.0f32 * SLIDER_OF_VOLUME)), 14);
        assert_eq!(ftol(f64::from(14.0f32 * VOLUME_OF_SLIDER)), 93);
        assert_eq!(ftol(f64::from(0.0f32 * VOLUME_OF_SLIDER)), 0);
    }
}
