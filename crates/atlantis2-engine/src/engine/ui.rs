//! The bag bar, the object in hand and the overlays in the warp frame
//! (the subject menu is in `subjects.rs`):
//! the right button (@ 0x0040c510), the bar's frame (@ 0x00474c30) and
//! the pointer drawn at the mouse (@ 0x0047c090), the overlays some
//! objects open (@ 0x0047ec50, their clicks @ 0x0047efd0, their close
//! @ 0x0047ef20, their draw @ 0x0047f0b0) and the dowsing rod
//! (@ 0x0049c6e0).
//!
//! The hand is a (bank, frame) pair (@ 0x00673f68 / 0x00673f94, set by
//! @ 0x0047c0c0) and the cursor another (@ 0x00673fa0 / 0x005def58, set
//! by @ 0x0047bef0); `OBJETENMAIN` is the script variable of the object's
//! id. When the scripts run, the hand pair and `OBJETENMAIN` are theirs
//! ([`Engine::with_script`] copies them back after each call).

use atlantis2_data::defnames;
use atlantis2_data::spr::{Mode, SprBank, Target};
use atlantis2_data::Folder;

use super::{CursorBank, Engine, Event, LoopType};
use crate::bag::{next_shown, param_word, visible_count, UiBank, UiDraw, FORM_FRAMES};
use crate::script::BagEntry;
use crate::subtitle::TextOut;

/// The frame of `interfac.spr` shown as the pointer while the bar or an
/// overlay is open.
const POINTER_FRAME: i32 = 40;

/// GDI's black and white (`COLORREF`), the colours of the shadowed text.
const BLACK: u32 = 0;
const WHITE: u32 = 0xff_ffff;

impl Engine {
    /// `MONDE`: the script's, or the world the game was started in.
    pub(crate) fn monde(&self) -> i32 {
        self.con
            .as_ref()
            .map_or(self.monde, |s| s.vars[s.keys.monde])
    }

    /// The bag's entries (the script's table; without the scripts the
    /// objects taken by a click).
    pub fn bag_entries(&self) -> &[BagEntry] {
        match &self.con {
            Some(s) => &s.bag,
            None => &self.own_bag,
        }
    }

    pub(super) fn objetenmain(&self) -> i32 {
        self.con
            .as_ref()
            .map_or(self.vars.objet_en_main, |s| s.vars[s.keys.objetenmain])
    }

    /// `OBJETENMAIN` = `id`.
    pub(super) fn set_objetenmain(&mut self, id: i32) {
        self.vars.objet_en_main = id;
        self.set_script_var(|k| k.objetenmain, id);
        if let Some(h) = self.hand.as_mut() {
            h.0 = id;
        }
    }

    /// The hand's frame and whether its bank is the objects bank.
    fn hand_pair(&self) -> (i32, bool) {
        match &self.con {
            Some(s) => (s.cursor_sprite, s.cursor_objects),
            None => self.hand.map_or((0, false), |(_, f)| (f, true)),
        }
    }

    /// FUN_0047c0c0(frame, bank): the hand pair, the objects bank or none.
    pub(crate) fn set_hand_pair(&mut self, frame: i32, objects: bool) {
        if let Some(s) = self.con.as_mut() {
            s.set_cursor(frame, objects);
        }
        let id = self.objetenmain();
        self.hand = objects.then_some((id, frame));
    }

    /// FUN_0047bef0(bank, frame): the cursor pair.
    pub(crate) fn set_cursor_pair(&mut self, bank: CursorBank, frame: i32) {
        self.cursor.shown = Some(frame);
        self.cursor.bank = bank;
        if let Some(s) = self.con.as_mut() {
            s.hand_drawn = true;
            s.hand_x = frame;
        }
    }

    /// FUN_0047bef0(0, 0): the cursor pair with no bank and frame 0,
    /// nothing drawn until the next cursor choice (the hand is kept).
    pub(crate) fn clear_cursor_pair(&mut self) {
        self.cursor.shown = None;
        if let Some(s) = self.con.as_mut() {
            s.hand_drawn = false;
            s.hand_x = 0;
        }
    }

    /// FUN_0047bf40: no cursor, `OBJETENMAIN` = 0, the hand emptied.
    pub(crate) fn drop_hand(&mut self) {
        self.cursor.shown = None;
        self.set_objetenmain(0);
        if let Some(s) = self.con.as_mut() {
            s.empty_hand();
        }
        self.hand = None;
    }

    /// FUN_0047bf20: the cursor's frame (0 with a warning when there is no
    /// cursor).
    fn cursor_frame(&self) -> i32 {
        match self.cursor.shown {
            Some(f) => f,
            None => {
                log::warn!("AtlSpr_get_curseur_num(), mais mouse_SprBank=NULL!");
                0
            }
        }
    }

    /// A sprite bank of `spr\` (FUN_0047ba80), logged when missing.
    pub(super) fn load_bank(&self, name: &[u8]) -> Option<SprBank> {
        let name = String::from_utf8_lossy(name).into_owned();
        let bank = self
            .files
            .read(Folder::Spr, &name)
            .and_then(|d| SprBank::parse(&d).ok());
        if bank.is_none() {
            log::warn!("spr\\{name} missing or unreadable");
        }
        bank
    }

    fn object_id(&self, name: &[u8]) -> Option<i32> {
        self.names.id(defnames::OBJETS, name).ok().map(|i| i as i32)
    }

    fn script_var(&self, name: &str) -> i32 {
        self.con.as_ref().and_then(|s| s.var(name)).unwrap_or(0)
    }

    fn set_named_var(&mut self, name: &str, value: i32) {
        if let Some(s) = self.con.as_mut() {
            s.set_var(name, value);
        }
    }

    /// A mouse move: the pointer moves by (dx, dy) within the screen
    /// (@ 0x0040c420, `ftol` of the motion: one pixel per count), and the
    /// turn speeds gain it (only read by the mouse look, which the open
    /// bar and the overlays stop).
    pub(crate) fn move_pointer(&mut self, dx: i32, dy: i32) {
        let (w, h) = (self.width() as i32, self.height() as i32);
        self.pointer.0 = (self.pointer.0 + dx).clamp(0, w - 1);
        self.pointer.1 = (self.pointer.1 + dy).clamp(0, h - 1);
    }

    /// The right button (FUN_0040c510, button code 2): outside the
    /// panorama it only sets the click's companion flag (@ 0x006569a0); in
    /// the panorama, with no dialog and no overlay, it toggles the bar:
    /// opening it shows the pointer (`interfac.spr` frame 40) at the screen
    /// centre, closing it empties the hand (`OBJETENMAIN` = 0) and keeps
    /// the cursor pair; both stop the turn. With an overlay open it empties
    /// the hand and hides the cursor (@ 0x0047bf40) and stops the turn.
    /// With the subject menu open it only stops the turn (and the original
    /// moves the system's cursor to the window's centre, @ 0x0040c3d0,
    /// which is not the game's pointer). The parchment, which also takes
    /// the button, never opens (below).
    pub fn right_click(&mut self) {
        if self.loop_type != LoopType::Warp {
            self.click_aux = 1;
            return;
        }
        if self.subject_menu.open {
            self.look.stop();
            return;
        }
        if self.dialog.is_none() && !self.overlays.any() {
            self.click_aux = 1;
            self.bar.open = !self.bar.open;
            if self.bar.open {
                self.set_cursor_pair(CursorBank::Interface, POINTER_FRAME);
                self.pointer = (self.width() as i32 / 2, self.height() as i32 / 2);
            } else {
                self.set_objetenmain(0);
                self.set_hand_pair(0, false);
            }
            self.look.stop();
            let open = self.bar.open;
            self.event(Event::Bag { open });
            return;
        }
        self.drop_hand();
        self.look.stop();
    }

    /// FUN_00474c30, each panorama frame while the bar is open, then the
    /// cursor at the pointer (FUN_0047c090).
    pub(super) fn bag_frame(&mut self) {
        let (w, h) = (self.width() as i32, self.height() as i32);
        self.bar.shown.clear();
        // Opening the bar puts the dowsing rod away.
        if self.rod.active {
            self.rod.bank = None;
            self.rod.active = false;
            self.set_named_var("SOURCIER", 0);
        }
        let monde = self.monde();
        let bag = self.bag_entries().to_vec();
        self.bar.layout(w as usize, visible_count(&bag, monde));
        let (px, py) = self.pointer;
        let fpu = self.camera.fpu;
        if px > w - 10 && py > h - 80 && self.bar.scroll_due(fpu, self.clock) {
            self.bar.scroll_right();
        }
        if px < 5 && py > h - 80 && self.bar.scroll_due(fpu, self.clock) {
            self.bar.scroll_left();
        }
        let bar = |x| UiDraw::Sprite {
            bank: UiBank::Interface,
            frame: 0,
            x,
            y: h - 39,
        };
        if w == 640 {
            self.ui_draws.push(bar(320));
        } else {
            self.ui_draws.push(bar(w / 2 - 80));
            self.ui_draws.push(bar(w / 2 + 80));
        }
        let mut hover = None;
        let mut i = self.bar.first_entry(&bag, monde);
        let mut left = 10;
        while i != -1 && (self.bar.shown.len() as i32) < self.bar.slots {
            let e = bag[i as usize];
            self.ui_draws.push(UiDraw::Sprite {
                bank: UiBank::Objects,
                frame: i32::from(e.sprite),
                x: left + 30,
                y: h - 40,
            });
            if left < px && px < left + 60 && h - 70 < py && py < h - 10 {
                hover = Some((e.id, left));
            }
            self.bar.shown.push(i as usize);
            left += 80;
            i = next_shown(&bag, i, monde);
        }
        if let Some((id, left)) = hover {
            self.hover_name(id, left, w, h);
        }
        if let Some(cell) = self.bar.pick(self.click, self.pointer, h) {
            let e = bag[self.bar.shown[cell]];
            self.pick(e);
        }
        self.pointer_draw();
    }

    /// FUN_0047c090: the cursor pair drawn at the pointer, when it has a
    /// bank.
    pub(super) fn pointer_draw(&mut self) {
        if let Some(frame) = self.cursor.shown {
            let bank = match self.cursor.bank {
                CursorBank::Interface => UiBank::Interface,
                CursorBank::Objects => UiBank::Objects,
            };
            self.ui_draws.push(UiDraw::Sprite {
                bank,
                frame,
                x: self.pointer.0,
                y: self.pointer.1,
            });
        }
    }

    /// The name of the object under the pointer: its `defnames.con` name
    /// looked up in `Messages.txt`'s list (an empty text when it is not
    /// there), in `AtlFT_POPUP`, centred on its cell and kept on the
    /// screen, its top at H − 70: in black one pixel down and right, then
    /// in white.
    fn hover_name(&mut self, id: i32, left: i32, w: i32, h: i32) {
        let key = self
            .names
            .name(defnames::OBJETS, id as usize)
            .unwrap_or_default();
        let text = self.object_names.get(key).to_vec();
        let width = self.popup_font.as_ref().map_or(0, |f| f.extent(&text).0);
        let mut x = left - width / 2 + 30;
        if x < 0 {
            x = 0;
        }
        if w < x + width {
            x = w - width;
        }
        let y = h - 70;
        for (dx, colour) in [(1, BLACK), (0, WHITE)] {
            self.ui_draws.push(UiDraw::Text {
                x: x + dx,
                y: y + dx,
                colour,
                bytes: text.clone(),
            });
        }
    }

    /// A click on a cell of the bar: the object becomes the cursor and the
    /// hand, `OBJETENMAIN` its id, then the special objects; the turn
    /// stops, the click is used and the bar closes.
    fn pick(&mut self, e: BagEntry) {
        let sprite = i32::from(e.sprite);
        self.set_cursor_pair(CursorBank::Objects, sprite);
        self.set_hand_pair(sprite, true);
        self.set_objetenmain(e.id);
        if Some(e.id) == self.object_id(b"BATON_SOURCE") && !self.rod.active {
            self.rod.bank = self.load_bank(b"baton.spr");
            self.rod.active = true;
            self.set_objetenmain(0);
            self.set_named_var("SOURCIER", 1);
            self.set_hand_pair(0, false);
            self.drop_hand();
        }
        // `PARCHEMIN` in view 10 of an Irish place opens the parchment
        // only when the byte @ 0x006d05f0 is 1, which no code writes: it
        // never opens.
        self.open_overlay(e.id);
        self.look.stop();
        self.click_aux = 0;
        self.click = false;
        self.bar.open = false;
        self.event(Event::BagPick { id: e.id });
    }

    /// FUN_0047ec50: the overlay of a special object picked from the bag.
    /// The forms and the fan show `form.spr`, the ring `anneau.spr`, the
    /// space-travel balls the bank named by their second parameter (the
    /// zone that sends the game on, their third). The object, the hand and
    /// the cursor's frame are put aside, the hand emptied, the cursor the
    /// pointer frame.
    fn open_overlay(&mut self, id: i32) {
        let is = |name: &[u8]| self.object_id(name) == Some(id);
        let form = [
            &b"FORMULAIRE_VIDE"[..],
            b"FORMULAIRE_1",
            b"FORMULAIRE_2",
            b"FORMULAIRE_3",
        ]
        .iter()
        .position(|n| is(n));
        let (fan, ring) = (is(b"EVENTAIL"), is(b"ANNEAU_GRAVE"));
        let ball = [
            &b"BOULE_SHAMAN_CH"[..],
            b"BOULE_SHAMAN_IR",
            b"BOULE_SHAMAN_YU",
            b"BOULE_SHAMAN",
        ]
        .iter()
        .any(|n| is(n));
        let o = &mut self.overlays;
        o.centre = (
            self.renderer.width() as i32 / 2,
            self.renderer.height() as i32 / 2,
        );
        let load_form = match form {
            Some(k) if !o.form => {
                o.form_number = k as i32;
                o.form = true;
                o.fan = false;
                o.cover = true;
                true
            }
            _ if fan && !o.fan => {
                o.fan = true;
                o.form = false;
                o.cover = true;
                true
            }
            _ => {
                if ring && !o.ring {
                    o.ring = true;
                    o.form = false;
                    o.fan = false;
                    o.cover = true;
                    self.overlays.ring_bank = self.load_bank(b"anneau.spr");
                } else {
                    if !ball || o.ball {
                        return;
                    }
                    let param = |n| {
                        self.names
                            .param_text_of(defnames::OBJETS, id as usize, n)
                            .map(|t| param_word(t).to_vec())
                            .unwrap_or_default()
                    };
                    let (bank, zone) = (param(2), param(3));
                    self.overlays.ball_zone = zone;
                    self.overlays.ball = true;
                    let bank = self.load_bank(&bank);
                    self.overlays.ball_frames = match &bank {
                        Some(b) => b.len() as i32,
                        None => {
                            log::warn!("AtlSpr_GetNbSprBank, bank=NULL");
                            0
                        }
                    };
                    self.overlays.ball_bank = bank;
                    self.overlays.ball_frame = 0;
                }
                self.overlays.form || self.overlays.fan
            }
        };
        if load_form {
            self.overlays.form_bank = self.load_bank(b"form.spr");
        }
        self.overlays.saved_objetenmain = self.objetenmain();
        self.overlays.saved_cursor_frame = self.cursor_frame();
        self.set_objetenmain(0);
        let (frame, objects) = self.hand_pair();
        self.overlays.saved_hand_frame = frame;
        self.overlays.saved_hand_objects = objects;
        self.set_hand_pair(0, false);
        self.set_cursor_pair(CursorBank::Interface, POINTER_FRAME);
    }

    /// FUN_0047efd0, in the warp frame before the draw while an overlay is
    /// open. The ball animates while the zone under the centre is its zone
    /// (by name, ignoring case: one frame per panorama frame, back to 0
    /// elsewhere); a click there closes the overlays, sets `SpaceTravel` = 1
    /// and runs a scan. Any other click (or the click's companion flag)
    /// gives back the object, the hand and the cursor and closes them.
    pub(super) fn overlay_clicks(&mut self) {
        if self.overlays.ball {
            let zone = self.overlays.ball_zone.clone();
            let on = self
                .centre()
                .and_then(|(_, z)| z.name.as_ref())
                .is_some_and(|n| n.as_bytes().eq_ignore_ascii_case(&zone));
            if on {
                let o = &mut self.overlays;
                o.ball_frame += 1;
                if o.ball_frames <= o.ball_frame {
                    o.ball_frame = 0;
                }
                if self.click {
                    self.close_overlays();
                    self.with_script(|s, h| {
                        s.run_command(h, b"(SpaceTravel=1)");
                        s.ctrl_scenar_in(h);
                    });
                }
            } else {
                self.overlays.ball_frame = 0;
            }
        }
        if self.overlays.any() && (self.click || self.click_aux == 1) {
            let o = &self.overlays;
            let (id, hand, objects, cursor) = (
                o.saved_objetenmain,
                o.saved_hand_frame,
                o.saved_hand_objects,
                o.saved_cursor_frame,
            );
            self.set_objetenmain(id);
            self.set_hand_pair(hand, objects);
            self.set_cursor_pair(CursorBank::Objects, cursor);
            self.close_overlays();
        }
    }

    /// FUN_0047ef20: every overlay closed, its bank freed, the click used,
    /// the turn stopped; closing the ball sets `SpaceTravel` = 0.
    fn close_overlays(&mut self) {
        let o = &mut self.overlays;
        let mut closed = false;
        if o.form || o.fan {
            o.form_bank = None;
            o.form = false;
            o.fan = false;
            o.cover = false;
            closed = true;
        }
        if o.ring {
            o.ring_bank = None;
            o.ring = false;
            o.cover = false;
            closed = true;
        }
        let ball = o.ball;
        if ball {
            o.ball_bank = None;
            o.ball = false;
            closed = true;
        }
        if closed {
            self.click = false;
            self.click_aux = 0;
            self.look.stop();
        }
        if ball {
            self.with_script(|s, h| s.run_command(h, b"(SpaceTravel=0)"));
        }
    }

    /// FUN_0047f0b0 (from FUN_00475320 in the draw): the overlays at the
    /// screen centre they opened with. A form: frame 0, then for form k
    /// its k stamps 120 pixels apart (`essai_laby1..3` through
    /// [`FORM_FRAMES`]); the fan: frame 7 and the three symbols of solution
    /// row `soluce_laby` (1..10, else a warning and 1); the ring: frame 0;
    /// the ball: its frame centred by its size (FUN_0047bdb0).
    pub(super) fn overlay_draw(&mut self) {
        let (x, y) = self.overlays.centre;
        let frame_of = |v: i32| -> Option<i32> {
            let f = usize::try_from(v)
                .ok()
                .and_then(|i| FORM_FRAMES.get(i))
                .copied();
            if f.is_none() {
                log::warn!("BigObj_Gestion: value {v} outside the frame table");
            }
            f
        };
        let mut draws = Vec::new();
        let form = |frame, x, y| UiDraw::Sprite {
            bank: UiBank::Form,
            frame,
            x,
            y,
        };
        if self.overlays.form {
            draws.push(form(0, x, y));
            let stamps = ["essai_laby1", "essai_laby2", "essai_laby3"];
            let n = self.overlays.form_number.clamp(0, 3) as usize;
            for (k, name) in stamps.iter().enumerate().take(n) {
                if let Some(f) = frame_of(self.script_var(name)) {
                    draws.push(form(f, x, y - 120 + 120 * k as i32));
                }
            }
        }
        if self.overlays.fan {
            let mut row = self.script_var("soluce_laby");
            if !(1..=10).contains(&row) {
                log::warn!("BigObj_Gestion() : soluce={row}!!");
                row = 1;
                self.set_named_var("soluce_laby", 1);
            }
            draws.push(form(7, x, y));
            let at = [(x - 130, y - 50), (x, y - 100), (x + 130, y - 50)];
            for (j, (sx, sy)) in at.into_iter().enumerate() {
                let symbol = FORM_FRAMES[4 + 3 * row as usize + j];
                if let Some(f) = frame_of(symbol) {
                    draws.push(form(f, sx, sy));
                }
            }
        }
        if self.overlays.ring {
            draws.push(UiDraw::Sprite {
                bank: UiBank::Ring,
                frame: 0,
                x,
                y,
            });
        }
        if self.overlays.ball {
            let frame = self.overlays.ball_frame;
            match &self.overlays.ball_bank {
                None => log::warn!("AtlSpr_Aff_centre, bank=NULL"),
                Some(b) => match usize::try_from(frame).ok().and_then(|f| b.info(f)) {
                    Some(info) if (frame as usize) < b.len() => draws.push(UiDraw::Sprite {
                        bank: UiBank::Ball,
                        frame,
                        x: x - i32::from(info.w) / 2,
                        y: y - i32::from(info.h) / 2,
                    }),
                    _ => log::warn!("AtlSpr_Aff_centre:num spr incorrect : {frame}"),
                },
            }
        }
        self.ui_draws.extend(draws);
    }

    /// FUN_0049c6e0 (from FUN_00475320 in the draw): the dowsing rod.
    /// `NEARW77` = 0 puts it away (`OBJETENMAIN` = `SOURCIER` = 0, the hand
    /// emptied, the cursor hidden); otherwise its frame is drawn at
    /// (W / 2, H − 240) and moves one step, the step reversed (and the
    /// frame stepped back) when it reaches `NEARW77` or −1.
    pub(super) fn rod_frame(&mut self) {
        if !self.rod.active {
            return;
        }
        let near = self.script_var("NEARW77");
        if near == 0 {
            self.rod.bank = None;
            self.rod.active = false;
            self.set_objetenmain(0);
            self.set_named_var("SOURCIER", 0);
            self.set_hand_pair(0, false);
            self.drop_hand();
            return;
        }
        let count = self.rod.bank.as_ref().map_or(0, |b| b.len() as i32);
        if self.rod.frame < 0 || count <= self.rod.frame {
            log::warn!("BatonUp():baton_SprNum={}", self.rod.frame);
            self.rod.frame = 0;
        }
        self.ui_draws.push(UiDraw::Sprite {
            bank: UiBank::Rod,
            frame: self.rod.frame,
            x: self.width() as i32 / 2,
            y: self.height() as i32 - 240,
        });
        self.rod.frame += self.rod.step;
        if self.rod.frame == near || self.rod.frame == -1 {
            self.rod.frame -= self.rod.step;
            self.rod.step = -self.rod.step;
        }
    }

    /// Draws [`Engine::ui_draws`] into `out` (`AtlSpr_Aff` refuses a frame
    /// outside its bank; the texts in the popup font).
    pub(super) fn paint_ui(&self, out: &mut [u16], w: usize, h: usize) {
        for d in &self.ui_draws {
            match d {
                UiDraw::Sprite { bank, frame, x, y }
                | UiDraw::Translucent { bank, frame, x, y } => {
                    let mode = if matches!(d, UiDraw::Translucent { .. }) {
                        Mode::Half
                    } else {
                        Mode::Opaque
                    };
                    let bank = match bank {
                        UiBank::Interface => &self.cursor_bank,
                        UiBank::Objects => &self.objects_bank,
                        UiBank::Rod => &self.rod.bank,
                        UiBank::Form => &self.overlays.form_bank,
                        UiBank::Ring => &self.overlays.ring_bank,
                        UiBank::Ball => &self.overlays.ball_bank,
                        UiBank::Subjects => &self.subjects_bank,
                    };
                    if let (Some(bank), Ok(frame)) = (bank, usize::try_from(*frame)) {
                        let t = &mut Target::screen(out, w, h);
                        bank.draw_mode(t, Some(*x), *y, frame, mode);
                    }
                }
                UiDraw::Text {
                    x,
                    y,
                    colour,
                    bytes,
                } => {
                    if let Some(font) = &self.popup_font {
                        let call = TextOut {
                            x: *x,
                            y: *y,
                            colour: *colour,
                            bytes: bytes.clone(),
                        };
                        font.draw(out, w, h, &[call]);
                    }
                }
            }
        }
    }
}
