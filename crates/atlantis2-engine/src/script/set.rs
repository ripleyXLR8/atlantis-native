//! `/set(KEY=value)` lines (`SetVariables` @ 0x004818f0) and the script
//! tables the handler keeps.
//!
//! The line is scanned to its first `(`. `FLAGnnn` (any case, a digit
//! after `FLAG`) sets `flags[nnn]` to the `atoi` of the text after the
//! operator byte, so `flag7++` stores 0. Otherwise the key runs to a stop
//! byte and is looked up in `#VARIABLES#`; `KEY++` / `KEY--` add or
//! subtract 1 (`KEY=++` stores `atoi("++")` = 0); else the value is the
//! `atoi` of the text after the operator byte, and the key is compared,
//! by index, with the special keys in the original's order. A special key
//! returns at once or falls through the rest of the chain to the default
//! store `vars[key] = value` (`TrajSound`, `DEPOSEOBJ`,
//! `ChangeAnimGotoPerso`, the four `...ZoneClic...` keys, `ViewBigSpr`,
//! `CINEMA` with a `?` value, and every other key).
//!
//! The handler returns the position after the line's CR, a position the
//! key chooses (`CHAPTER`: before the new text's first section header),
//! or `None` (a fatal error).

use super::{AnimOp, BagEntry, DialStart, Host, Script, Shown, Src, CR, LF};
use crate::camera::deg_to_rad;
use atlantis2_data::con::atof;
use atlantis2_data::defnames;

impl Script {
    /// `SetVariables` (@ 0x004818f0) on the line at `p` of a text.
    pub fn set_variables(&mut self, host: &mut dyn Host, src: Src, mut p: usize) -> Option<usize> {
        if self.fatal.is_some() {
            return None;
        }
        log::debug!("interp: SET{}", self.line_text(src, p));
        if let (Some(t), Src::Chapter) = (self.trace.as_mut(), src) {
            t.push(p);
        }
        if let (true, Src::Chapter) = (self.lines.is_some(), src) {
            let end = p.min(self.chapter_size);
            let line = 1 + self.chapter_text[..end]
                .iter()
                .filter(|&&c| c == LF)
                .count();
            let chapter = self.chapter;
            if let Some(l) = self.lines.as_mut() {
                l.push((chapter, line));
            }
        }
        // The key's index stays 0 (CHAPTER) when no `(` is found within the
        // chapter's size (@ 0x00481941).
        let mut key = 0usize;
        let mut found = false;
        let mut n = 0usize;
        if self.chapter_size > 0 {
            loop {
                let c = self.b(src, p);
                p += 1;
                if c == b'(' {
                    found = true;
                    break;
                }
                n += 1;
                if n >= self.chapter_size {
                    break;
                }
            }
        }
        if found {
            if self.at_ci(src, p, b"FLAG") && self.b(src, p + 4).is_ascii_digit() {
                let flag = self.atoi_at(src, p + 4);
                while super::eval::is_key_byte(self.b(src, p)) && p < self.len(src) {
                    p += 1;
                }
                let v = self.atoi_at(src, p + 1);
                match usize::try_from(flag)
                    .ok()
                    .and_then(|f| self.flags.get_mut(f))
                {
                    Some(slot) => *slot = v,
                    // The original writes past the table; the data never do.
                    None => self.warn("SetVariables: flag hors table", &flag.to_string()),
                }
                return Some(self.after_cr(src, p + 1));
            }
            let mut name = Vec::with_capacity(16);
            loop {
                name.push(self.b(src, p));
                p += 1;
                if !super::eval::is_key_byte(self.b(src, p)) || p >= self.len(src) {
                    break;
                }
            }
            for c in &mut name {
                if c.is_ascii_lowercase() {
                    *c -= 0x20;
                }
            }
            key = match self.names.id(defnames::VARIABLES, &name) {
                Ok(k) => k,
                Err(e) => {
                    self.fail(format!("SetVariables: {e}"));
                    return None;
                }
            };
            if key == self.keys.changewam {
                // The text between the operator and `)`.
                let mut wam = Vec::new();
                let mut i = p + 1;
                while self.b(src, i) != b')' && i < self.len(src) {
                    wam.push(self.b(src, i));
                    i += 1;
                }
                let name: String = wam.iter().map(|&c| char::from(c)).collect();
                if !host.load_wam(&name) {
                    // A place the port does not have (not the original's:
                    // its failure is fatal): the section ends here.
                    self.travel_refused = true;
                    return None;
                }
                self.wam = wam;
                // `AtlWam_Load` forgets the current view (@ 0x0040ff98): the
                // next `vue=` sets `LASTVUE` = 0.
                self.current_view = None;
                return Some(self.after_cr(src, p));
            }
        }
        // `p` is at the operator byte.
        let k = self.keys.clone();
        if self.at(src, p, b"++") {
            self.vars[key] = self.vars[key].wrapping_add(1);
            return Some(self.after_cr(src, p));
        }
        if self.at(src, p, b"--") {
            self.vars[key] = self.vars[key].wrapping_sub(1);
            return Some(self.after_cr(src, p));
        }
        let value = self.atoi_at(src, p + 1);
        let done = |s: &Script| Some(s.after_cr(src, p));

        if key == k.cinema && !self.at_ci(src, p + 1, b"?") {
            return Some(self.cinema(host, src, p, value));
        }
        if key == k.trajsound {
            // The name up to a blank, `,`, `/`, `)` or the line's end, plus
            // ".apc" (FUN_004108c0, 0xff bytes at most).
            let mut t = String::new();
            let mut i = p + 1;
            while !matches!(
                self.b(src, i),
                CR | LF | 0 | b' ' | b',' | b'\t' | b'/' | b')'
            ) && t.len() < 0xfe
            {
                t.push(char::from(self.b(src, i)));
                i += 1;
            }
            t.push_str(".apc");
            host.traj_sound(&t);
        }
        let deg = |s: &Script| deg_to_rad(s.fpu, value);
        if key == k.anglewarpx {
            // The variable is not stored; `Interp_GereFrame` writes the
            // current yaw there every frame.
            self.angle_x = deg(self);
            (self.start_x, self.start_y) = (self.angle_x, self.angle_y);
            return done(self);
        }
        if key == k.anglewarpy {
            self.angle_y = deg(self);
            (self.start_x, self.start_y) = (self.angle_x, self.angle_y);
            return done(self);
        }
        if key == k.anglewarpxnow {
            self.angle_x = deg(self);
            host.turn_now(true, self.angle_x);
            return done(self);
        }
        if key == k.anglewarpynow {
            self.angle_y = deg(self);
            host.turn_now(false, self.angle_y);
            return done(self);
        }
        if key == k.zoomstart {
            let z = atof(self.tail(src, p + 1)) as f32;
            host.zoom_start(z, 35.0);
            return done(self);
        }
        if key == k.zoompsychostart {
            let mut i = p;
            while self.b(src, i) != b',' && i < self.len(src) {
                i += 1;
            }
            let b = atof(self.tail(src, i + 1)) as f32;
            host.zoom_psycho_start(value as f32, b);
            return Some(self.after_cr(src, i + 1));
        }
        if key == k.limiteanglewarpx || key == k.limiteanglewarpy {
            self.vars[key] = if value == 999 { -1 } else { value };
            return done(self);
        }
        if key == k.sacados {
            if self.at_ci(src, p + 1, b"NOT_") {
                let id = self.id(defnames::OBJETS, src, p + 5);
                self.bag_remove(id);
            } else {
                self.bag_take(host, src, p + 1);
            }
            return done(self);
        }
        if key == k.chapter {
            return self.change_chapter(host, value);
        }
        if key == k.vue {
            self.set_vue(host, value);
            return done(self);
        }
        if key == k.nextvue {
            self.next_view = host.has_view(value).then_some(value);
            self.vars[key] = value;
            return done(self);
        }
        if key == k.disable || key == k.sujet {
            return done(self);
        }
        if key == k.hideguest || key == k.showguest {
            self.show_guest = key == k.showguest;
            return done(self);
        }
        if key == k.enablesuj || key == k.disablesuj || key == k.killsuj {
            self.subjects_command(src, p + 1, key);
            return done(self);
        }
        if key == k.clearsuj {
            let id = self.id(defnames::PERSONNAGES, src, p + 1);
            if id == 0 {
                let t = self.line_text(src, p + 1);
                self.warn("SetVariables() : CLEARSUJ(%s) : perso DUMMY", &t);
            }
            self.subjects.clear(id);
            return done(self);
        }
        if key == k.dial {
            return Some(self.dial(host, src, p));
        }
        if key == k.changecd {
            let present = host.change_cd(value - 1);
            self.disc_answer(host, present, true);
            return done(self);
        }
        if key == k.newsound {
            host.new_sound(&format!("sound{value:03}.apc"));
            return done(self);
        }
        if key == k.stopsound {
            self.warn("SetVariables() : STOPSOUND a faire", "");
            return done(self);
        }
        if key == k.loadmusik {
            host.load_music(&self.line_text(src, p + 1));
            return done(self);
        }
        if key == k.delmusik {
            host.delete_music(&self.line_text(src, p + 1));
            return done(self);
        }
        if key == k.startmusik {
            // Copied up to the CR, 0xff bytes at most (FUN_00481680).
            let t = self.line_text(src, p + 1);
            self.music = t.bytes().take(0xff).collect();
            let click = self.click;
            host.start_music(&t);
            self.click = click;
            return done(self);
        }
        if key == k.stopmusik {
            host.stop_music();
            return done(self);
        }
        if key == k.deposeobj {
            self.warn("SetVariables() : DEPOSEOBJ a faire", "");
        }
        if key == k.hideperso || key == k.hideobjet {
            let (section, kind) = if key == k.hideperso {
                (defnames::PERSONNAGES, 1)
            } else {
                (defnames::OBJETS, 2)
            };
            let id = self.id(section, src, p + 1);
            let mut i = p;
            loop {
                i += 1;
                if self.b(src, i) == CR || self.b(src, i) == b',' || i >= self.len(src) {
                    break;
                }
            }
            let comma = self.b(src, i) == b',';
            self.hide(host, id, kind, comma);
            return Some(self.after_cr(src, i));
        }
        if key == k.showperso {
            let id = self.id(defnames::PERSONNAGES, src, p + 1);
            let mut i = p;
            loop {
                i += 1;
                if self.b(src, i) == CR || self.b(src, i) == b',' || i >= self.len(src) {
                    break;
                }
            }
            let number = if self.b(src, i) == b',' {
                self.atoi_at(src, i + 1)
            } else {
                0
            };
            self.show(host, id, 1, number);
            return Some(self.after_cr(src, i));
        }
        if key == k.showobjet {
            let id = self.id(defnames::OBJETS, src, p + 1);
            self.show(host, id, 2, 0);
            return done(self);
        }
        if key == k.loadanim {
            // Parsed, nothing done.
            let i = self.after_comma(src, p);
            return Some(self.after_cr(src, i));
        }
        if key == k.changeanimgotoperso {
            let id = self.id(defnames::PERSONNAGES, src, p + 1);
            let mut i = self.after_comma(src, p + 1);
            let a = self.parse_int(src, &mut i);
            let b = self.parse_int(src, &mut i);
            host.anim(AnimOp::ChangeAnimGotoPerso { id, a, b });
        }
        let anim_key = [
            k.playanimp,
            k.playanimo,
            k.continueanim,
            k.stopanim,
            k.gotoframeperso,
            k.gotoframeobjet,
            k.anim_the_frame,
            k.addframeperso,
            k.addframeobjet,
        ];
        if anim_key.contains(&key) {
            return Some(self.anim_command(host, src, p, key));
        }
        if key == k.incperso {
            self.inc_perso = if value == 99 {
                0
            } else {
                self.inc_perso.wrapping_add(1)
            };
            return done(self);
        }
        if key == k.addguest || key == k.subguest {
            // Both call an empty function (@ 0x0048c600).
            return done(self);
        }
        if key == k.monde {
            self.vars[key] = value;
            let next = self.after_cr(src, p);
            self.resume_at = next;
            self.set_monde(host, value, true);
            return Some(next);
        }
        if key == k.exit {
            self.vars[key] = value;
            log::info!("SetVariables: EXIT, quitat2=1");
            self.quit = true;
            return done(self);
        }
        if key == k.objetenmain {
            if self.b(src, p + 1) == b'0' {
                self.vars[key] = 0;
                self.set_cursor(0, false);
                self.empty_hand();
            } else {
                let id = self.id(defnames::OBJETS, src, p + 1);
                self.hold(id);
                let sprite = self.param(defnames::OBJETS, src, p + 1);
                self.set_cursor(sprite, true);
            }
            return done(self);
        }
        if key == k.changeobjectworld {
            let id = self.id(defnames::OBJETS, src, p + 1);
            let i = self.after_comma(src, p);
            let world = self.atoi_at(src, i);
            if let Some(e) = self.bag.iter_mut().find(|e| e.id == id) {
                e.world = if world == 6 { 2 } else { world };
            }
            return Some(self.after_cr(src, i));
        }
        let zone_keys = [
            (k.disablezoneclicnwnz, false, false),
            (k.enablezoneclicnwnz, false, true),
            (k.disablezoneclicnwnamez, true, false),
            (k.enablezoneclicnwnamez, true, true),
        ];
        let mut q = p;
        for (zk, by_name, on) in zone_keys {
            if key == zk {
                q = self.after_comma(src, q);
                let zone = if by_name {
                    self.id(defnames::ZONES_CLIC, src, q)
                } else {
                    self.atoi_at(src, q)
                };
                host.enable_zone(value, zone, on);
            }
        }
        if key == k.message_clic {
            host.message_clic(&self.line_text(src, q + 1));
            return Some(self.after_cr(src, q));
        }
        if key == k.viewbigspr {
            host.view_big_sprite(&self.line_text(src, q + 1));
        }
        if key == k.credits {
            host.credits();
            let next = self.after_cr(src, q);
            if !self.in_command {
                self.resume_pending = true;
            }
            self.resume_at = next;
            return Some(next);
        }
        self.vars[key] = value;
        Some(self.after_cr(src, q))
    }

    /// The position after the first `,` at or after `p` (the handler's
    /// `while (p++, c != ',')` walks).
    fn after_comma(&self, src: Src, mut p: usize) -> usize {
        while self.b(src, p) != b',' && p < self.len(src) {
            p += 1;
        }
        p + 1
    }

    /// `FUN_00485180`: the next number from `*p` (up to a CR), then past
    /// its digits and the blanks after them.
    fn parse_int(&self, src: Src, p: &mut usize) -> i32 {
        while !matches!(self.b(src, *p), 0 | CR) && !self.b(src, *p).is_ascii_digit() {
            *p += 1;
        }
        let v = self.atoi_at(src, *p);
        while self.b(src, *p).is_ascii_digit() {
            *p += 1;
        }
        while matches!(self.b(src, *p), b' ' | b'\t') {
            *p += 1;
        }
        v
    }

    /// The `CINEMA` branch (`AtlHnm_start` called @ 0x00481d2a): `N`, or
    /// `N,M` with dialog M over the film; the film plays and the section
    /// waits for its end.
    fn cinema(&mut self, host: &mut dyn Host, src: Src, p: usize, value: i32) -> usize {
        let mut i = p + 1;
        while !matches!(self.b(src, i), 0 | LF | b',' | b')') {
            i += 1;
        }
        let dialog = (self.b(src, i) == b',').then(|| {
            let digits: Vec<u8> = self
                .tail(src, i + 1)
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .copied()
                .collect();
            atlantis2_data::con::atoi(&digits)
        });
        host.music_volume(20);
        self.cinema_starting = true;
        host.music_step();
        host.cinema_sound(&format!("cine{value:03}.apc"));
        host.music_step();
        let started = host.play_video(&format!("cine{value:03}.hnm"), dialog);
        self.double_click = false;
        self.click = false;
        self.click_aux = 0;
        self.overlay = false;
        self.cinema_starting = false;
        let next = self.after_cr(src, p);
        if !self.in_command && started {
            self.resume_pending = true;
        }
        self.resume_at = next;
        next
    }

    /// The `DIAL` branch (`AtlDial_start` called @ 0x004826c9): dialog N, or
    /// the value of the variable named
    /// when the value is not a number (`dial=NAME`); the section
    /// waits, also when the dialog could not start.
    fn dial(&mut self, host: &mut dyn Host, src: Src, p: usize) -> usize {
        if host.dialog_running() {
            self.warn("SetVariables() : AtlDial_start() : dialogue en cours", "");
            host.dial_stop();
        }
        let mut n = self.atoi_at(src, p + 1);
        if n == 0 {
            let v = self.id(defnames::VARIABLES, src, p + 1);
            n = self.vars[v as usize];
        }
        if host.dial_start(n) == DialStart::Failed {
            // FUN_0047ff60's failure path: the loop type back (the host's),
            // then what touches the script.
            self.dialog_reset();
            self.resume_if_pending(host);
        }
        host.music_volume(16);
        let next = self.after_cr(src, p);
        if !self.in_command {
            self.resume_pending = true;
        }
        self.resume_at = next;
        next
    }

    /// The `CHAPTER` branch (`LoadCurrentChap` called @ 0x0048220a): the
    /// new chapter's text replaces the
    /// buffer, its `/INIT` runs (or the running one restarts on it), and
    /// the position before the new text's first section header ends the
    /// section that changed it.
    fn change_chapter(&mut self, host: &mut dyn Host, value: i32) -> Option<usize> {
        self.chapter_reload = true;
        self.previous_chapter = self.chapter;
        self.chapter = value;
        self.vars[self.keys.chapter] = self.chapter;
        if !self.load_chapter(host, self.chapter) {
            self.fail(format!(
                "SetVariables() : chapitre introuvable : scenar\\chapi{:03}.con",
                self.chapter
            ));
            return None;
        }
        if !self.stub_run_init {
            self.run_init(host);
        }
        let mut i = 0;
        while !self.is_header(Src::Chapter, i) {
            i += 1;
            if i >= CHAPTER_LIMIT {
                return None;
            }
        }
        // The byte before the header (0 for a header at the start: the
        // original's pointer would be one before the buffer).
        Some(i.saturating_sub(1))
    }

    /// The `VUE` branch (`AtlWam_SetCurrentVue` called @ 0x00482321): a
    /// view of the current place.
    fn set_vue(&mut self, host: &mut dyn Host, value: i32) {
        let k = self.keys.clone();
        self.vars[k.next_warp_invalide] = 0;
        self.vue_value = value;
        self.vars[k.arriveevue] = self.view_number;
        self.vars[k.clicperso] = 0;
        self.vars[k.cliczone] = 0;
        if !host.has_view(value) {
            self.fail("AtlWam_SetCurrentVue NULL".into());
            return;
        }
        self.set_current_view(value);
        self.view_number = value;
        self.vars[k.vue] = value;
        host.enter_view_with_vars(value, self.start_x, self.start_y, &mut self.vars);
        self.view_reset();
        self.vars[k.vue] = value;
        self.click = false;
        self.click_aux = 0;
    }

    /// `AtlWam_SetCurrentVue` (@ 0x0040fe50): `LASTVUE` = the view left
    /// (0 without one), `VUE` = the new one.
    pub fn set_current_view(&mut self, view: i32) {
        self.vars[self.keys.lastvue] = self.current_view.unwrap_or(0);
        self.current_view = Some(view);
        self.vars[self.keys.vue] = view;
    }

    /// `ENABLESUJ` (@ 0x0047f920), `DISABLESUJ` (@ 0x0047f9f0), `KILLSUJ`
    /// (@ 0x0047fa70): `PERSO,SUJ1[,SUJ2...]`, subjects matched by their
    /// `#SUJETS#` parameter.
    fn subjects_command(&mut self, src: Src, mut p: usize, key: usize) {
        let perso = self.id(defnames::PERSONNAGES, src, p);
        p = self.after_comma(src, p);
        loop {
            if self.fatal.is_some() {
                return;
            }
            let value = self.param(defnames::SUJETS, src, p);
            if key == self.keys.enablesuj {
                // The index is looked up only for a new entry.
                let index = if self.subjects.list(perso).iter().any(|s| s.value == value) {
                    0
                } else {
                    self.id(defnames::SUJETS, src, p)
                };
                self.subjects.enable(perso, value, index);
            } else if key == self.keys.disablesuj {
                self.subjects.disable(perso, value);
            } else {
                self.subjects.kill(perso, value);
            }
            while !matches!(self.b(src, p), b',' | b')') && p < self.len(src) {
                p += 1;
            }
            let c = self.b(src, p);
            p += 1;
            if c != b',' {
                return;
            }
        }
    }

    /// `SACADOS=NAME` (FUN_00475610 → FUN_004754f0): the object goes in the
    /// bag unless it is there.
    fn bag_take(&mut self, host: &mut dyn Host, src: Src, p: usize) {
        let id = self.id(defnames::OBJETS, src, p);
        let sprite = self.param(defnames::OBJETS, src, p);
        if self.bag.iter().any(|e| e.id == id) {
            let name = self.line_text(src, p);
            self.warn("PrendObj(%s) : deja pris!!", &name);
            return;
        }
        self.bag_add(host, id, sprite as u8);
    }

    /// FUN_004754f0: an object in the bag, in the current world.
    pub fn bag_add(&mut self, host: &mut dyn Host, id: i32, sprite: u8) {
        if self.bag.iter().any(|e| e.id == id) {
            self.warn(
                "Inventaire_PrendObj_id_nspr(%d) : deja pris!",
                &id.to_string(),
            );
            return;
        }
        let monde = self.vars[self.keys.monde];
        self.bag.push(BagEntry {
            sprite,
            id,
            world: if monde == 6 { 2 } else { monde },
        });
        if self.bag.len() > 0x59 {
            self.fail(format!("Inventaire: {} objets", self.bag.len()));
        }
        host.bag_layout(self.visible_objects());
    }

    /// The entries of the bag shown in the current world (FUN_00475870).
    pub fn visible_objects(&self) -> i32 {
        crate::bag::visible_count(&self.bag, self.vars[self.keys.monde])
    }

    /// `SACADOS=NOT_NAME` (FUN_00475720 → FUN_00475680).
    pub fn bag_remove(&mut self, id: i32) {
        if let Some(i) = self.bag.iter().position(|e| e.id == id) {
            self.bag.remove(i);
        }
    }

    /// FUN_00475340: the object in hand (any object, in the bag or not),
    /// the cursor its sprite.
    pub fn hold(&mut self, id: i32) {
        self.vars[self.keys.objetenmain] = id;
        let sprite = usize::try_from(id).map_or(0, |i| self.names.param_of(defnames::OBJETS, i));
        self.set_cursor(sprite, true);
    }

    /// FUN_0047bf40: the hand emptied.
    pub fn empty_hand(&mut self) {
        self.hand_drawn = false;
        self.hand_x = -1;
        self.vars[self.keys.objetenmain] = 0;
        self.set_cursor(0, false);
    }

    /// FUN_0048c6b0: hide; without `,x` the place's records too.
    pub(crate) fn hide(&mut self, host: &mut dyn Host, id: i32, kind: i32, comma: bool) {
        if !comma {
            host.anim(AnimOp::Hide { kind, id });
        }
        self.show_row(id, false, kind);
        host.show_table(id, false, kind);
    }

    /// FUN_0048c610: show.
    fn show(&mut self, host: &mut dyn Host, id: i32, kind: i32, number: i32) {
        host.anim(AnimOp::Show { kind, id, number });
        self.show_row(id, true, kind);
        host.show_table(id, true, kind);
    }

    /// FUN_0048c730: the row of (id, kind) updated, or appended.
    fn show_row(&mut self, id: i32, shown: bool, kind: i32) {
        match self.shown.iter_mut().find(|r| r.id == id && r.kind == kind) {
            Some(r) => r.shown = shown,
            None => self.shown.push(Shown { id, shown, kind }),
        }
    }

    /// The animation keys (`PLAYANIMP` .. `ADDFRAMEOBJET` in `SetVariables`'
    /// order): the name, then the
    /// number after the comma.
    fn anim_command(&mut self, host: &mut dyn Host, src: Src, p: usize, key: usize) -> usize {
        let k = self.keys.clone();
        let either = |s: &mut Script| {
            // CONTINUEANIM, ANIM_THE_FRAME: #OBJETS#, then #PERSONNAGES#.
            let t = s.tail(src, p + 1);
            let mut id = s.names.id_cool(defnames::OBJETS, t);
            if id == 0 {
                id = s.names.id_cool(defnames::PERSONNAGES, t);
            }
            if id == 0 {
                s.fail("SetVariables() : animation: nom inconnu".into());
            }
            id as i32
        };
        let perso = [k.playanimp, k.stopanim, k.gotoframeperso, k.addframeperso].contains(&key);
        let kind = if perso { 1 } else { 2 };
        if key == k.continueanim {
            let id = either(self);
            host.anim(AnimOp::Continue { kind: 0, id });
            return self.after_cr(src, p);
        }
        let id = if key == k.anim_the_frame {
            either(self)
        } else {
            let section = if perso {
                defnames::PERSONNAGES
            } else {
                defnames::OBJETS
            };
            self.id(section, src, p + 1)
        };
        let i = self.after_comma(src, p);
        let n = self.atoi_at(src, i);
        let op = if key == k.playanimp || key == k.playanimo {
            AnimOp::Play {
                kind,
                id,
                number: n,
            }
        } else if key == k.stopanim {
            AnimOp::Stop { id, number: n }
        } else if key == k.gotoframeperso || key == k.gotoframeobjet {
            AnimOp::GotoFrame { kind, id, frame: n }
        } else if key == k.anim_the_frame {
            if n != 0 && n != 1 {
                self.fail("SetVariables() : ANIM_THE_FRAME : 0 ou 1".into());
            }
            AnimOp::TheFrame {
                kind: 0,
                id,
                on: n != 0,
            }
        } else {
            AnimOp::AddFrame {
                kind,
                id,
                frames: n,
            }
        };
        host.anim(op);
        self.after_cr(src, i)
    }

    /// `Interp_SetMonde` (@ 0x00484df0): the cursor cleared, the world's
    /// banks, texts and disc (through the host), and the disc rule of
    /// FUN_0040dff0 for every world but Tibet's (5, whose disc is the one
    /// in the drive).
    pub fn set_monde(&mut self, host: &mut dyn Host, monde: i32, ask: bool) {
        self.set_cursor(0, false);
        if !(1..=6).contains(&monde) {
            return;
        }
        let present = host.set_monde(monde);
        if monde != 5 {
            self.disc_answer(host, present, ask);
        }
    }

    /// FUN_0040dff0's answer: with the disc there, a waiting section goes
    /// on; without it, the section waits for the disc menu (unless in a
    /// `RunOneCommand`).
    fn disc_answer(&mut self, host: &mut dyn Host, present: bool, ask: bool) {
        if present {
            self.resume_if_pending(host);
        } else if !self.in_command && ask {
            self.resume_pending = true;
        }
    }
}

/// The bound of the header search after a chapter change.
const CHAPTER_LIMIT: usize = super::CHAPTER_BUFFER;
