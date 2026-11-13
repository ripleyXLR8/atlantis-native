//! The 2004 program's inventory, hand and followers in the game
//! (atlantis4-cd-en-3022848; the rules are in [`crate::inventory2004`]).
//!
//! - The right button (mouse callback @ 0x0042e209..0x0042e269): in the
//!   warp it opens the inventory (FUN_00428690) unless it is open, in the
//!   subject menu (loop mode 9) it closes it (FUN_004288e0, `CLICPERSO` 0,
//!   `SUJET` −2); in the inventory its byte 0x006e9fe9 stays for the
//!   inventory's frame.
//! - The inventory's frame (FUN_00428940, open): the warp without its
//!   cursor (FUN_00444670), the followers, the items and the bar, or the
//!   object shown big; the hand's object at the pointer or the pointer;
//!   the pick (FUN_0042b170); the flip; `Interp_GereFrame`
//!   (FUN_004520c0); the buttons; the light dialog (FUN_00476320); the
//!   fade's step.
//! - Closed (FUN_00428940's other branch, from the warp frame
//!   FUN_00443cc0 after the render): the fade-out of the items and the
//!   bar, then, neither the inventory nor the subject menu open, the
//!   followers (FUN_0043d000) and the puzzles' warp cursor (FUN_00429ad0).
//! - The hand in the warp (FUN_00444cb0 → FUN_00429780): the object's
//!   sprite of the hand's bank centred on the screen, when the subject
//!   menu is closed.

use atlantis2_data::defnames;
use atlantis2_engine::script::{BagEntry, FollowerOp};
use atlantis3_data::evolution2004_save::{HideEntry, ZoneEntry};
use atlantis3_data::spr::Bank;

use super::{Game, Mode};
use crate::inventory2004::{self as inv, Action, Composition, Follower, Layout};
use crate::with_px;

/// The 2004 inventory's own state.
#[derive(Default)]
pub(crate) struct State {
    /// The followers (0x006ffc4c), in the order they came.
    pub followers: Vec<Follower>,
    /// `spr\suiveurs.spr` (0x006ffc30, FUN_0043d140 at `CHANGECD`).
    pub followers_bank: Option<Bank>,
    /// The object shown big (0x006ae550, −1 none): its id, its
    /// `big_*.spr` (0x006e7cc0) and `map\inventbig.jpg` (0x00716c50) as
    /// x8r8g8b8 rows of the screen's size.
    pub examine: Option<Examine>,
    /// `#COMPOSITIONS#` of `defnames.con`, read at the first use.
    pub compositions: Option<Vec<Composition>>,
    /// The zone table (0x006e7418, 16-byte rows, count 0x006e7c44): the
    /// place's zones enabled or disabled by the script, for the saves.
    pub zone_table: Vec<ZoneEntry>,
}

/// The disabled zones' type offset (0x15b38).
const ZONE_OFF: i32 = 88888;

pub(crate) struct Examine {
    pub id: i32,
    pub big: Option<Bank>,
    pub picture: Vec<u32>,
}

impl Game {
    /// The bag (the script's list; the 2004 program's has no world).
    pub(crate) fn bag_2004(&self) -> Vec<BagEntry> {
        self.script
            .as_ref()
            .map(|s| s.bag.clone())
            .unwrap_or_default()
    }

    /// The right button of the 2004 program (@ 0x0042e209..0x0042e269).
    pub(crate) fn right_click_2004(&mut self) {
        self.right_clicked = true;
        match self.mode {
            Mode::Warp => {
                if !self.inventory.open {
                    self.open_inventory();
                    self.right_clicked = false;
                }
            }
            Mode::Subjects => {
                // FUN_004288e0, then `CLICPERSO` = 0, `SUJET` = −2.
                self.close_subjects();
                self.set_script_var(|k| k.clicperso, 0);
                self.set_script_var(|k| k.sujet, -2);
                self.right_clicked = false;
            }
            // Mode 8 (the inventory): the byte stays for its frame.
            _ => {}
        }
    }

    /// The items, the arrows (FUN_0042af70) and the bar (`Interface.spr`
    /// #0 centred on (W/2, H − 45), @ 0x004289c2..0x004289ec) at the
    /// inventory's alpha.
    pub(crate) fn draw_inventory_items_2004(&mut self) {
        let (w, h) = (self.width, self.height);
        let lay = Layout::for_width(w);
        let bag = self.bag_2004();
        let hand = self.object_in_hand();
        let (scroll, alpha) = (self.inventory.scroll as i32, self.inventory.alpha);
        if let Some(bank) = &self.objects {
            for (e, j) in inv::drawn_items(&bag, &lay, scroll, hand) {
                let (x, y) = lay.item_centre(j, h);
                Self::aff_centre(
                    &mut self.screen,
                    w,
                    h,
                    bank,
                    usize::from(e.sprite),
                    x,
                    y,
                    alpha,
                );
            }
        }
        if let Some(bank) = &self.arrows {
            for (i, x, y) in inv::arrows(&lay, scroll, bag.len(), w, h) {
                Self::aff(&mut self.screen, w, h, bank, i, x, y, alpha);
            }
        }
        if let Some(bank) = &self.interface {
            Self::aff_centre(
                &mut self.screen,
                w,
                h,
                bank,
                0,
                (w / 2) as i32,
                inv::bar_centre_y(h),
                alpha,
            );
        }
    }

    /// The followers (FUN_0043d000).
    pub(crate) fn draw_followers_2004(&mut self) {
        let (w, h) = (self.width, self.height);
        let Some(bank) = &self.inv2004.followers_bank else {
            return;
        };
        for (frame, x, y) in inv::follower_draws(&self.inv2004.followers) {
            if let Ok(f) = usize::try_from(frame) {
                Self::aff(&mut self.screen, w, h, bank, f, x, y, 1.0);
            }
        }
    }

    /// The hand's object (`OBJETENMAIN`) and its sprite, when it can be
    /// drawn (FUN_00429780 / FUN_004298d0: the hand set, its sprite
    /// 0x006ae2e0 not −1, the object in the bag (FUN_0042af10), else the
    /// hand is emptied (FUN_00429a20)).
    fn hand_sprite_2004(&mut self) -> Option<usize> {
        let hand = self.object_in_hand();
        if hand == 0 {
            return None;
        }
        let sprite = self
            .bag_2004()
            .iter()
            .find(|e| e.id == hand)
            .map(|e| e.sprite);
        if sprite.is_none() {
            if let Some(s) = self.script.as_mut() {
                s.empty_hand();
            }
        }
        sprite.map(usize::from)
    }

    /// The hand in the warp (FUN_00429780 from the cursor pass
    /// FUN_00444cb0 when the subject menu is closed):
    /// `AtlSpr_AffCentre(hand bank, sprite, W/2, H/2, 1.0)`.
    pub(crate) fn draw_hand_2004(&mut self) {
        let Some(sprite) = self.hand_sprite_2004() else {
            return;
        };
        let (w, h) = (self.width, self.height);
        if let Some(bank) = &self.objects_hand {
            Self::aff_centre(
                &mut self.screen,
                w,
                h,
                bank,
                sprite,
                (w / 2) as i32,
                (h / 2) as i32,
                1.0,
            );
        }
    }

    /// The pointer of the inventory: the hand's object top-left at the
    /// pointer (FUN_004298d0) when there is one, else the pointer
    /// (FUN_00429460).
    fn draw_inventory_pointer_2004(&mut self) {
        let named = self.objects_hand.is_some();
        match self.hand_sprite_2004().filter(|_| named) {
            Some(sprite) => {
                let (w, h) = (self.width, self.height);
                let (px, py) = self.pointer;
                if let Some(bank) = &self.objects_hand {
                    Self::aff(&mut self.screen, w, h, bank, sprite, px, py, 1.0);
                }
            }
            None => self.draw_pointer(),
        }
    }

    /// The pick (FUN_0042b170): the scrolling, then the object id of the
    /// item clicked (either button, in the bar), −2 for the right button
    /// above the bar (its byte cleared on an arrow: no close) or a
    /// follower chosen, −1 otherwise.
    fn inventory_pick_2004(&mut self) -> i32 {
        let (w, h) = (self.width, self.height);
        let lay = Layout::for_width(w);
        let bag = self.bag_2004();
        let (px, py) = self.pointer;
        // Scrolling (@ 0x0042b1c5..0x0042b2d4): the pointer at x = 0 or
        // W − 1, one slot when more than 0.25 s passed.
        let now = self.now;
        if (px == 0 || px == w as i32 - 1) && now - self.inventory.scroll_time > 0.25 {
            self.inventory.scroll_time = now;
            let s = self.inventory.scroll as i32;
            if px == 0 && s > 0 {
                self.inventory.scroll -= 1;
            } else if px != 0 && s < bag.len() as i32 - lay.slots {
                self.inventory.scroll += 1;
            }
        }
        let scroll = self.inventory.scroll as i32;
        let hand = self.object_in_hand();
        if py > h as i32 - lay.bar {
            // The hover label (@ 0x0042b35b..0x0042b3f1): the bag's entry
            // at the pointer's slot, its `#OBJETS#` name (FUN_004674d0)
            // matched against the `Objects.txt` keys; not the hand's object
            // → FUN_0042bed0(its text) into the back buffer.
            let hover = lay
                .index_at(h, self.pointer, scroll)
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| bag.get(i))
                .map(|e| e.id)
                .filter(|&id| id != hand);
            if let Some(id) = hover {
                self.draw_object_label(id);
            }
            if self.click || self.right_clicked {
                let hit = lay
                    .index_at(h, self.pointer, scroll)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| bag.get(i))
                    .filter(|e| e.id != hand);
                if let Some(e) = hit {
                    return e.id;
                }
            }
        } else if self.right_clicked {
            // @ 0x0042b577..0x0042b613: on an arrow (`SPR_COLLISION`) the
            // byte is cleared.
            if let Some(bank) = &self.arrows {
                for (i, x, y) in inv::arrows(&lay, scroll, bag.len(), w, h) {
                    if bank.opaque_at(i, Some(x), y, px, py) {
                        self.right_clicked = false;
                    }
                }
            }
            return -2;
        }
        if self.click {
            // FUN_0043d080: the first enabled follower whose sprite is
            // under the pointer.
            let name = self.inv2004.followers_bank.as_ref().and_then(|bank| {
                self.inv2004
                    .followers
                    .iter()
                    .zip(inv::follower_draws(&self.inv2004.followers))
                    .find(|(f, (frame, x, y))| {
                        f.enabled
                            && usize::try_from(*frame)
                                .is_ok_and(|i| bank.opaque_at(i, Some(*x), *y, px, py))
                    })
                    .map(|(f, _)| f.name.clone())
            });
            if let Some(name) = name {
                // @ 0x0042b69f..0x0042b700: its character, with subjects
                // and an empty hand: `CLICPERSO`, the inventory closed and
                // the subject menu opened.
                let id = self.names.id_cool(defnames::PERSONNAGES, name.as_bytes()) as i32;
                let n = self.script.as_ref().map_or(0, |s| s.subjects.count(id));
                if n > 0 && hand == 0 {
                    self.close_inventory();
                    self.open_subjects(id);
                    return -2;
                }
            }
        }
        -1
    }

    /// The inventory's frame while open (FUN_00428940).
    pub(crate) fn inventory_frame_2004(&mut self) {
        if self.inv2004.examine.is_none() {
            self.draw_warp_only();
            self.draw_followers_2004();
            self.draw_inventory_items_2004();
        } else {
            self.draw_examine_2004();
        }
        self.draw_inventory_pointer_2004();
        let pick = if self.inv2004.examine.is_none() {
            self.inventory_pick_2004()
        } else {
            -1
        };
        self.flip_texts();
        let (yaw, pitch) = self.camera.angles();
        self.with_script(|s, h| s.gere_frame(h, yaw, pitch));
        let hand = self.object_in_hand();
        let right = std::mem::take(&mut self.right_clicked);
        let left = std::mem::take(&mut self.click);
        let examining = self.inv2004.examine.is_some();
        for a in inv::action(right, left, examining, pick, hand) {
            self.inventory_action_2004(a, hand);
        }
        if self.mode == Mode::Inventory {
            if self.dialog2004.is_some() {
                self.light_dialog_2004();
            }
            self.inventory.fade_step(self.now);
        } else if self.mode == Mode::Subjects {
            // A follower chosen: the subject menu's part of the same call
            // (@ 0x00429097).
            self.overlay_frame();
        }
    }

    fn inventory_action_2004(&mut self, a: Action, hand: i32) {
        match a {
            Action::Close => {
                self.close_inventory();
                if hand > 0 {
                    self.hold_2004(hand);
                }
                self.with_script(|s, h| {
                    s.ctrl_scenar_in(h);
                });
            }
            Action::Examine(id) => self.examine_2004(id),
            Action::EndExamine => self.inv2004.examine = None,
            Action::Hold(id) => self.hold_2004(id),
            Action::EmptyHand => {
                if let Some(s) = self.script.as_mut() {
                    s.empty_hand();
                }
            }
            Action::Combine(a, b) => self.combine_2004(a, b),
        }
    }

    /// `AtlInterfaceJeu_SetObjetEnMain` (FUN_00429600): an object of the
    /// bag whose sprite the hand's bank has goes in the hand.
    pub(crate) fn hold_2004(&mut self, id: i32) {
        let Some(e) = self.bag_2004().into_iter().find(|e| e.id == id) else {
            log::warn!("AtlInterfaceJeu_SetObjetEnMain() : id invalide ({id}).");
            return;
        };
        if self
            .objects_hand
            .as_ref()
            .is_some_and(|b| b.info(usize::from(e.sprite)).is_none())
        {
            log::warn!("AtlInterfaceJeu_SetObjetEnMain() : sprite objet en main invalide");
            return;
        }
        if let Some(s) = self.script.as_mut() {
            s.hold(id);
        }
    }

    /// The composition of the hand's object `a` and the item `b`
    /// (@ 0x00428e70..0x00428ec6): with a `#COMPOSITIONS#` line, both
    /// leave the bag (FUN_0042aaf0), the products enter it with their
    /// `#OBJETS#` parameter 1 as sprite (FUN_0042ad40, FUN_00467e90,
    /// FUN_0042a980) and the hand is emptied; without one nothing
    /// happens.
    fn combine_2004(&mut self, a: i32, b: i32) {
        if self.inv2004.compositions.is_none() {
            let text = self.files.read("scenar/defnames.con").unwrap_or_default();
            self.inv2004.compositions = Some(inv::compositions(&text));
        }
        let name = |id: i32| {
            usize::try_from(id)
                .ok()
                .and_then(|i| self.names.name(defnames::OBJETS, i))
                .map(<[u8]>::to_vec)
                .unwrap_or_default()
        };
        let (na, nb) = (name(a), name(b));
        let Some(products) = self
            .inv2004
            .compositions
            .as_deref()
            .and_then(|c| inv::combine(c, &na, &nb))
            .map(<[Vec<u8>]>::to_vec)
        else {
            return;
        };
        let products: Vec<(i32, u8)> = products
            .iter()
            .map(|p| {
                let id = self.names.id_cool(defnames::OBJETS, p);
                (id as i32, self.names.param_of(defnames::OBJETS, id) as u8)
            })
            .collect();
        self.with_script(|s, h| {
            s.bag_remove(a);
            s.bag_remove(b);
            for (id, sprite) in products {
                // FUN_0042ad40 gives 0 for an unknown name: skipped.
                if id != 0 {
                    s.bag_add(h, id, sprite);
                }
            }
            s.empty_hand();
        });
    }

    /// The right button on an item (@ 0x00428b9c..0x00428cfa): the
    /// object's `#OBJETS#` parameter 2 (`big_*.spr`) from `spr\`, the
    /// picture `map\inventbig.jpg` of the current disc into a surface of
    /// the screen's size (FUN_0046d390 → FUN_00453df0).
    fn examine_2004(&mut self, id: i32) {
        let big_name = usize::try_from(id)
            .ok()
            .and_then(|i| self.names.param_text_of(defnames::OBJETS, i, 2))
            .map(|t| {
                // FUN_00467620: up to a blank, a comma, a `/` or a `)`.
                t.iter()
                    .take_while(|&&c| {
                        !matches!(c, b'\r' | b'\n' | 0 | b' ' | b',' | b'\t' | b'/' | b')')
                    })
                    .map(|&c| char::from(c))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let depth = self.depth();
        let big = self
            .files
            .read(&format!("spr/{big_name}"))
            .and_then(|d| Bank::parse(&d, depth).ok());
        if big.is_none() {
            log::warn!("spr\\{big_name}: not found");
        }
        let (w, h) = (self.width, self.height);
        let mut picture = vec![0u32; w * h];
        match self
            .files
            .read("map/inventbig.jpg")
            .and_then(|d| atlantis4_data::jpeg::decode(&d).ok())
        {
            Some(img) => super::dialog2004::load_background(&mut picture, (w, h), &img),
            None => log::warn!("Impossible de trouver le fichier map\\inventbig.jpg"),
        }
        self.inv2004.examine = Some(Examine { id, big, picture });
    }

    /// The object shown big (@ 0x00428a05..0x00428a86): the picture
    /// (FUN_004b5120), then `AtlSpr_AffCentre(big, 0, W/2, H/2, 1.0)`.
    fn draw_examine_2004(&mut self) {
        let (w, h) = (self.width, self.height);
        let Some(ex) = &self.inv2004.examine else {
            return;
        };
        with_px!(&mut self.screen, s => {
            for (d, &p) in s.iter_mut().zip(&ex.picture) {
                *d = crate::menu::PagePixel::from_argb(p);
            }
        });
        if let Some(bank) = &ex.big {
            Self::aff_centre(
                &mut self.screen,
                w,
                h,
                bank,
                0,
                (w / 2) as i32,
                (h / 2) as i32,
                1.0,
            );
        }
    }

    /// The followers' keys (FUN_0043d240, FUN_0043d440, FUN_0043d550,
    /// FUN_0043d630, FUN_0043d710).
    pub(crate) fn followers_op_2004(&mut self, op: FollowerOp, name: &str) {
        let list = &mut self.inv2004.followers;
        match op {
            FollowerOp::Add => {
                // The `#PERSONNAGES#` parameters 1 and 2 (`atoi`, 0 for
                // none, @ 0x0043d2d0..0x0043d31b).
                let p = |n| {
                    self.names
                        .param_text(defnames::PERSONNAGES, name.as_bytes(), n)
                        .map_or(0, atlantis2_data::con::atoi)
                };
                let (frame_off, frame_on) = (p(1), p(2));
                list.push(Follower {
                    name: name.to_string(),
                    frame_off,
                    frame_on,
                    enabled: false,
                });
            }
            FollowerOp::Kill => list.retain(|f| f.name != name),
            FollowerOp::Enable | FollowerOp::Disable => {
                for f in list.iter_mut().filter(|f| f.name == name) {
                    f.enabled = op == FollowerOp::Enable;
                }
            }
            FollowerOp::Clear => list.clear(),
        }
    }

    /// `ENABLEZONE` / `DISABLEZONE` (FUN_004420f0): every zone named
    /// `name` (an exact compare, FUN_00449410) of every view of the place
    /// 0x00712ba4 (each WAM of the CD has one place):
    /// enabled, a type above 88887 loses 88888; disabled, a type of 88887
    /// or less gains 88888 (the zone click ignores those, @ 0x0043eef0).
    /// Each change is also kept in a table (FUN_00422950, 0x006e7418,
    /// emptied by `CHANGEWAM`) that a loaded game applies again: not
    /// ported (the saves').
    pub(crate) fn zone_toggle_2004(&mut self, name: &str, on: bool) {
        self.zones_toggle_2004(|_, z| z.name.as_deref() == Some(name), on);
    }

    /// `DISABLEWARP=n` (@ 0x0044f76e..0x0044f789 → FUN_00442030(n = 0)):
    /// every zone of the current view (0x006e7c30) enabled (n = 0) or
    /// disabled (n ≠ 0), each change recorded (FUN_00422950).
    pub(crate) fn disable_warp_2004(&mut self, disable: bool) {
        let Some(view) = self.place.view else {
            return;
        };
        self.zones_toggle_2004(|v, _| v == view, !disable);
    }

    /// The toggles of FUN_004420f0 / FUN_00442030 / FUN_00441ec0 on the
    /// zones `pick` (view id, zone) chooses: enabling takes 88888 from a
    /// type > 88887 and records (zone `+4`, the new type, view, 1);
    /// disabling records (zone `+4`, the type, view, 0) and adds 88888 to
    /// a type ≤ 88887 (FUN_00422950).
    fn zones_toggle_2004(
        &mut self,
        pick: impl Fn(u32, &atlantis2_data::wam::Zone) -> bool,
        on: bool,
    ) {
        let mut rows = Vec::new();
        for v in self
            .place
            .wam
            .places
            .iter_mut()
            .flat_map(|p| p.views.iter_mut())
        {
            let view = v.id;
            for z in v.zones.iter_mut().filter(|z| pick(view, z)) {
                if on && z.kind >= ZONE_OFF {
                    z.kind -= ZONE_OFF;
                    rows.push((z.value, z.kind, view as i32, 1));
                } else if !on && z.kind < ZONE_OFF {
                    rows.push((z.value, z.kind, view as i32, 0));
                    z.kind += ZONE_OFF;
                }
            }
        }
        for (id, kind, vue, etat) in rows {
            self.zone_record_2004(id, kind, vue, etat);
        }
    }

    /// FUN_00422950(id, type, view, state): the row of (id, type, view)
    /// gets the state, else a row is appended (no bound check in the
    /// original: 128 rows fit, 0x006e7418..0x006e7c18).
    fn zone_record_2004(&mut self, id: i32, kind: i32, vue: i32, etat: i32) {
        let t = &mut self.inv2004.zone_table;
        match t
            .iter_mut()
            .find(|r| r.id == id && r.vue == vue && r.kind == kind)
        {
            Some(r) => r.etat = etat,
            None => t.push(ZoneEntry {
                id,
                kind,
                vue,
                etat,
            }),
        }
    }

    /// The loader's zone part (FUN_00460b50 @ 0x00463873..0x004645a5):
    /// every zone of every view of the place enabled (FUN_00441ec0(view,
    /// zone, 1)), the table cleared, the file's rows read, then for every
    /// zone the state of its row (FUN_004229d0(zone `+4`, type, view): no
    /// row → on) applied by FUN_00441f70(view, zone `+4`, type, state),
    /// which records the zones it disables.
    pub(crate) fn load_zone_table_2004(&mut self, rows: &[ZoneEntry]) {
        self.zones_toggle_2004(|_, _| true, true);
        self.inv2004.zone_table = rows.to_vec();
        let mut off = Vec::new();
        for v in self.place.wam.places.iter().flat_map(|p| p.views.iter()) {
            for z in &v.zones {
                let on = self
                    .inv2004
                    .zone_table
                    .iter()
                    .find(|r| r.id == z.value && r.kind == z.kind && r.vue == v.id as i32)
                    .is_none_or(|r| r.etat != 0);
                if !on {
                    off.push((v.id, z.value, z.kind));
                }
            }
        }
        for (view, value, kind) in off {
            self.zones_toggle_2004(
                |v, z| v == view && z.value == value && z.kind == kind,
                false,
            );
        }
    }

    /// The loader's hide table (@ 0x00463037..0x00463860): the file's rows
    /// (`Id`, `Type`, `Etat`) become the table (the script's show rows);
    /// then for each place-animation slot k whose (id, type) has a row
    /// (FUN_004664f0 ≠ 2): its shown := the row's state, and 0 when
    /// another slot of the same (id, type) is shown.
    pub(crate) fn load_hide_table_2004(&mut self, rows: &[HideEntry]) {
        if let Some(s) = self.script.as_mut() {
            s.shown = rows
                .iter()
                .map(|r| atlantis2_engine::script::Shown {
                    id: r.id,
                    shown: r.etat != 0,
                    kind: r.kind,
                })
                .collect();
        }
        let state = |id: i32, kind: i32| {
            rows.iter()
                .find(|r| r.id == id && r.kind == kind)
                .map(|r| r.etat)
        };
        for k in 0..self.slots.slots.len() {
            let (id, kind) = (self.slots.slots[k].id, self.slots.slots[k].kind);
            let Some(st) = state(id, kind) else {
                continue;
            };
            let other = self
                .slots
                .slots
                .iter()
                .enumerate()
                .any(|(j, s)| j != k && s.id == id && s.kind == kind && s.shown);
            self.slots.slots[k].shown = st == 1 && !other;
        }
    }

    /// The tables as the save writes them (FUN_0045f080): the hide
    /// table's rows (`Id`, `Type`, `Etat`) and the zone table.
    pub(crate) fn save_tables_2004_of(
        &self,
        s: &atlantis2_engine::script::Script,
    ) -> (Vec<HideEntry>, Vec<ZoneEntry>) {
        let hide = s
            .shown
            .iter()
            .map(|r| HideEntry {
                id: r.id,
                kind: r.kind,
                etat: i32::from(r.shown),
            })
            .collect();
        (hide, self.inv2004.zone_table.clone())
    }

    /// Tests: the zone table.
    #[doc(hidden)]
    pub fn zone_table_2004(&self) -> &[ZoneEntry] {
        &self.inv2004.zone_table
    }

    /// The followers (tests).
    #[doc(hidden)]
    pub fn followers_2004(&self) -> &[Follower] {
        &self.inv2004.followers
    }

    /// The object shown big, if any (tests).
    #[doc(hidden)]
    pub fn examined_2004(&self) -> Option<i32> {
        self.inv2004.examine.as_ref().map(|e| e.id)
    }
}
