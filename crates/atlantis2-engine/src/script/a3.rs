//! *Atlantis III*'s rules for the interpreter (its `Game.exe`, the GOG.com
//! build of 3,473,408 bytes; addresses below are that executable's).
//!
//! The walks (`RunInit`, `CtrlScenarIN`, `RunSection`, `RunLine`, the
//! timers), `EvalCondition` and the 15 Hz clocks are *Atlantis II*'s
//! code, instruction for instruction or with the changes noted where
//! they apply. What differs:
//!
//! - `SetVariables` (@ 0x00447af0) looks every key up by name at each use,
//!   in its own order, with new keys ([`Script::set_variables_a3`]);
//! - `ANGLEWARPX` / `ANGLEWARPY` convert degrees as `deg × π × (1/180)`
//!   with two float constants ([`deg_to_rad_a3`]);
//! - `CHAPTER`, `CHANGEWAM` and the view entry remember the chapter, the
//!   place and the view of each world in the story flags 1020 + world,
//!   1010 + world and 1000 + world;
//! - `CHANGEWAM` runs a list of show / hide commands per place, chosen by
//!   flags (@ 0x0044f0c0, the host's [`Host::wam_commands`]);
//! - `CHANGECD` is a plain store (its test is patched out @ 0x00449f62);
//! - a waiting section is not scanned (`CtrlScenarIN` @ 0x0044a2f0), and a
//!   `RunOneCommand` meanwhile is queued in its place (@ 0x00447a38);
//! - the per-view reset (@ 0x0044a350) leaves the turn limits and clears
//!   `PSYCHOMOVE`; `Interp_GereFrame` (@ 0x0044b6c0) checks `GAMEOVER`;
//! - the warp loop resumes sections waiting for a camera re-alignment, a
//!   flag or an animation's end (@ 0x0043ff20, [`Script::check_waits`]).

use super::{DialStart, Host, Script, Src, CR};
use atlantis2_data::defnames;

/// The places whose index the world's flag 1010 + world keeps (the table
/// @ 0x0077bad8, filled by @ 0x0044ba10).
pub const WAM_NAMES: [&str; 14] = [
    "ankh", "cfuner", "check", "couloir", "hoggar", "interdim", "jardin", "ldmorts", "mmarchd",
    "paleo", "pmagi", "pyram", "rhea", "tourmagi",
];

/// The first story flag of the per-world views (flags 1000 + world).
pub const FLAG_WORLD_VIEW: usize = 1000;
/// ... of the per-world places (index in [`WAM_NAMES`]).
pub const FLAG_WORLD_WAM: usize = 1010;
/// ... of the per-world chapters.
pub const FLAG_WORLD_CHAPTER: usize = 1020;
/// The interdimension flag (@ 0x0077d2f0), set by `CHANGEWAM`.
pub const FLAG_INTERDIM: usize = 897;

/// *Atlantis III*'s story flags: 0x800 ints (@ 0x0077c4ec).
pub const FLAGS_A3: usize = 0x800;

/// Degrees to radians as `SetVariables` converts `ANGLEWARPX` /
/// `ANGLEWARPY` (@ 0x00448449: `fild`, `fmul` by 3.1415927f @ 0x00700720,
/// `fmul` by 0.0055555557f @ 0x0070047c, `fstp dword`).
pub fn deg_to_rad_a3(fpu: crate::fpu::Fpu, deg: i32) -> f32 {
    let pi = f64::from(f32::from_bits(0x4049_0fdb));
    let inv180 = f64::from(f32::from_bits(0x3bb6_0b61));
    fpu.f32(fpu.mul(fpu.mul(f64::from(deg), pi), inv180))
}

/// What *Atlantis III*'s interpreter keeps besides *Atlantis II*'s state.
#[derive(Clone, Debug, Default)]
pub struct A3State {
    /// The world (@ 0x0077bd04): 1 Hoggar, 2 LDM, 3 Bagdad, 4 Paleo, 5
    /// inter; 0 while the intro plays.
    pub world: i32,
    /// Commands run while a section waited (@ 0x00447a38): they replace
    /// the rest of that section when it is resumed.
    pub queue: Vec<Vec<u8>>,
    /// `WAITFORRECALAGE` (@ 0x0077b898): resumed when the re-alignment ends.
    pub wait_recalage: bool,
    /// `WAITFORFLAG` (@ 0x007182d8): resumed when the flag is not 0.
    pub wait_flag: Option<i32>,
    /// `WAITFORFINANIMP` / `WAITFORFINANIMO` (@ 0x007182d0, 0x007182d4):
    /// (kind, id, n), resumed when that animation has ended.
    pub wait_anim: Option<(i32, i32, i32)>,
}

/// A saved subject: enabled, `#SUJETS#` index, value.
pub type SavedSubject = (bool, i32, i32);

/// What a save gives the script ([`Script::load_a3`]).
#[derive(Clone, Debug, Default)]
pub struct LoadData {
    pub current_cd: i32,
    pub wam: String,
    pub monde: i32,
    pub vue: i32,
    pub chapter: i32,
    /// (index, value) of the flag lines.
    pub flags: Vec<(i32, i32)>,
    /// The bag's object names.
    pub objects: Vec<Vec<u8>>,
    /// Per character its subjects (etat, `#SUJETS#` index, value).
    pub subjects: Vec<(i32, Vec<SavedSubject>)>,
    /// `Quaternion0..3`, when all four were read.
    pub camera: Option<[f32; 4]>,
}

impl Script {
    /// Whether this is *Atlantis III*'s interpreter.
    pub fn is_a3(&self) -> bool {
        self.dialect == super::Dialect::Atlantis3
    }

    /// The `#VARIABLES#` index of `name`, if the file has it.
    fn var_id(&self, name: &str) -> Option<usize> {
        self.names.id(defnames::VARIABLES, name.as_bytes()).ok()
    }

    /// A variable by name (0 when the file lacks it).
    pub fn var_a3(&self, name: &str) -> i32 {
        self.var_id(name).map_or(0, |i| self.vars[i])
    }

    fn set_var_a3(&mut self, name: &str, v: i32) {
        if let Some(i) = self.var_id(name) {
            self.vars[i] = v;
        }
    }

    fn set_flag(&mut self, n: usize, v: i32) {
        if let Some(f) = self.flags.get_mut(n) {
            *f = v;
        }
    }

    /// The world must be 1..5 where `CHANGEWAM`, `VUE` and `CHAPTER`
    /// store their slot ("Vous vous foutez de moi ?", fatal).
    fn world_slot(&mut self, what: &str) -> Option<usize> {
        let w = self.a3.world;
        if (1..=5).contains(&w) {
            Some(w as usize)
        } else {
            self.fail(format!("Vous vous foutez de moi ? - {what} (monde {w})"));
            None
        }
    }

    /// The start-up's script part (@ 0x004d6801..0x004d6b10): the tables,
    /// the 0x800 flags cleared, `LIMITEANGLEWARPY` = 70, `LIMITEANGLEWARPX`
    /// = −1, `NEARW77` = 6, world 1, `CHAPTER` = 0, then chapter 0 loaded
    /// and its `/INIT` run (the first block only): the boot state behind
    /// the intro and the menus.
    pub fn boot_a3(&mut self, host: &mut dyn Host) -> bool {
        self.flags = vec![0; FLAGS_A3];
        // The subject table's reset at start-up (@ 0x0043bbc0).
        self.subjects.reset_a3();
        let k = self.keys.clone();
        self.vars[k.limiteanglewarpy] = 70;
        self.vars[k.limiteanglewarpx] = -1;
        self.vars[k.nearw77] = 6;
        self.a3.world = 1;
        self.click = false;
        self.click_aux = 0;
        self.vars[k.chapter] = self.chapter;
        if !self.load_chapter(host, self.chapter) {
            self.fail(format!("chapitre {} introuvable", self.chapter));
            return false;
        }
        self.run_init(host)
    }

    /// The menu's new game for an empty player slot (@ 0x00452b98): the
    /// flags cleared (@ 0x00451190, `GAMEOVER` = 0 with them), the bag
    /// emptied (@ 0x0042a020), `RunOneCommand("(chapter=600)")`, then the
    /// way back to the game (@ 0x00451280: `Interp_CloseInterrupt(1)`).
    pub fn new_game_a3(&mut self, host: &mut dyn Host) -> bool {
        self.flags.iter_mut().for_each(|f| *f = 0);
        self.set_var_a3("GAMEOVER", 0);
        self.bag.clear();
        // The subject table reset (@ 0x00452000 → @ 0x0043bbc0).
        self.subjects.reset_a3();
        self.run_command(host, b"(chapter=600)");
        self.close_interrupt(host);
        self.fatal.is_none()
    }

    /// `Interp_CloseInterrupt` (@ 0x0044c650): a waiting section goes on,
    /// or, when commands were run during the wait, they run instead of it.
    pub fn close_interrupt(&mut self, host: &mut dyn Host) {
        log::debug!("Interp_CloseInterrupt");
        if !self.resume_pending {
            return;
        }
        if self.a3.queue.is_empty() {
            self.resume_if_pending(host);
            return;
        }
        self.resume_pending = false;
        for cmd in std::mem::take(&mut self.a3.queue) {
            self.run_command(host, &cmd);
        }
    }

    /// The warp loop's waits (@ 0x0043ff20 step 3): a section waiting for
    /// the end of a re-alignment, of an animation or for a flag goes on.
    /// True when one did (the loop's frame ends there).
    pub fn check_waits(&mut self, host: &mut dyn Host) -> bool {
        if self.a3.wait_recalage && !host.recalage_running() {
            self.a3.wait_recalage = false;
            self.close_interrupt(host);
            return true;
        }
        if let Some((kind, id, n)) = self.a3.wait_anim {
            if host.anim_ended(kind, id, n) {
                self.a3.wait_anim = None;
                self.close_interrupt(host);
                return true;
            }
        }
        if let Some(f) = self.a3.wait_flag {
            if usize::try_from(f)
                .ok()
                .and_then(|f| self.flags.get(f))
                .is_some_and(|&v| v != 0)
            {
                self.a3.wait_flag = None;
                self.close_interrupt(host);
                return true;
            }
        }
        false
    }

    /// A line that makes the section wait (not inside a `RunOneCommand`).
    fn wait(&mut self, next: usize) -> usize {
        if !self.in_command {
            self.resume_pending = true;
        }
        self.resume_at = next;
        next
    }

    /// `SetVariables` (@ 0x00447af0) for a key other than `FLAGnnn`,
    /// `key` its `#VARIABLES#` index and `p` at the byte after the key.
    pub(crate) fn set_variables_a3(
        &mut self,
        host: &mut dyn Host,
        src: Src,
        p: usize,
        key: usize,
    ) -> Option<usize> {
        let is = |s: &Script, name: &str| s.var_id(name) == Some(key);
        let next = self.after_cr(src, p);
        if is(self, "CHANGEWAM") {
            return self.change_wam_a3(host, src, p, next);
        }
        if self.at(src, p, b"++") {
            self.vars[key] = self.vars[key].wrapping_add(1);
            return Some(next);
        }
        if self.at(src, p, b"--") {
            self.vars[key] = self.vars[key].wrapping_sub(1);
            return Some(next);
        }
        let value = self.atoi_at(src, p + 1);
        if is(self, "CINEMA") && !self.at_ci(src, p + 1, b"?") {
            return Some(self.cinema_a3(host, src, p, value, next));
        }
        if is(self, "ANGLEMONS") || is(self, "ANGLEDAME") {
            // Angles of two characters (@ 0x0040d290 / 0x0040d280); no
            // shipped line sets them.
            return Some(next);
        }
        let k = self.keys.clone();
        if key == k.anglewarpx || key == k.anglewarpy {
            // Not stored: `Interp_GereFrame` writes the camera's angles.
            let a = deg_to_rad_a3(self.fpu, value);
            if key == k.anglewarpx {
                self.angle_x = a;
            } else {
                self.angle_y = a;
            }
            (self.start_x, self.start_y) = (self.angle_x, self.angle_y);
            return Some(next);
        }
        if key == k.anglewarpxnow || key == k.anglewarpynow {
            let a = deg_to_rad_a3(self.fpu, value);
            let yaw = key == k.anglewarpxnow;
            if yaw {
                self.angle_x = a;
            } else {
                self.angle_y = a;
            }
            host.turn_now(yaw, a);
            return Some(next);
        }
        if key == k.limiteanglewarpx || key == k.limiteanglewarpy {
            self.vars[key] = if value == 999 { -1 } else { value };
            return Some(next);
        }
        if key == k.sacados {
            if self.at_ci(src, p + 1, b"NOT_") {
                let id = self.id(defnames::OBJETS, src, p + 5);
                self.bag_remove(id);
            } else {
                self.bag_take(host, src, p + 1);
            }
            return Some(next);
        }
        if is(self, "PERSOJOUEUR") {
            log::info!("PersoJoueur: {}", self.line_text(src, p + 1));
            return Some(next);
        }
        if key == k.chapter {
            let r = self.change_chapter(host, value);
            // The chapter global after the new `/INIT` (@ 0x004487d8), which
            // a `chapter=` inside it may have changed again.
            if let Some(w) = self.world_slot("Chapter") {
                let c = self.chapter;
                self.set_flag(FLAG_WORLD_CHAPTER + w, c);
            }
            return r;
        }
        if key == k.vue {
            self.set_vue(host, value);
            return Some(next);
        }
        if is(self, "LOADVUEMONDE") {
            // The section goes on in the remembered chapter's text, as
            // after `CHAPTER=` (not at the old offset).
            return self.load_vue_monde(host);
        }
        if key == k.nextvue {
            self.next_view = host.has_view(value).then_some(value);
            self.vars[key] = value;
            return Some(next);
        }
        if key == k.disable {
            return Some(next);
        }
        if key == k.enablesuj || key == k.disablesuj || key == k.killsuj {
            self.subjects_command(src, p + 1, key);
            return Some(next);
        }
        if key == k.clearsuj {
            let id = self.id(defnames::PERSONNAGES, src, p + 1);
            self.subjects.clear(id);
            return Some(next);
        }
        if key == k.dial {
            return Some(self.dial_a3(host, src, p, next));
        }
        if key == k.sujet {
            return Some(next);
        }
        if is(self, "PLAYSOUND") {
            host.new_sound(&format!("sound{value:03}.apc"));
            return Some(next);
        }
        if is(self, "PLAYVOICE") {
            let mut i = p + 1;
            let n = [0; 4].map(|_| self.parse_int(src, &mut i));
            host.play_voice(n);
            return Some(next);
        }
        if key == k.stopsound {
            self.warn("SetVariables() : STOPSOUND a faire", "");
            return Some(next);
        }
        if key == k.loadmusik || key == k.delmusik {
            return Some(next);
        }
        if key == k.startmusik {
            let t = self.line_text(src, p + 1);
            self.music = t.bytes().take(0xff).collect();
            let click = self.click;
            host.start_music(&t);
            self.click = click;
            return Some(next);
        }
        if key == k.stopmusik {
            host.stop_music();
            return Some(next);
        }
        if is(self, "INITRECALAGE") {
            host.init_recalage(self.angle_x, self.angle_y);
            return Some(next);
        }
        if is(self, "DISPLAY") {
            host.display();
            return Some(next);
        }
        if is(self, "WAITFORRECALAGE") {
            self.a3.wait_recalage = true;
            return Some(self.wait(next));
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
            return Some(next);
        }
        if key == k.loadanim {
            return Some(next);
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
        for name in [
            "PLAYCUTANIMP",
            "PLAYCUTANIMO",
            "UNANIMATE",
            "ANIMATE",
            "SUPFRAMEPERSO",
            "SUPFRAMEOBJET",
        ] {
            if is(self, name) {
                // Not on the first chapters' path: not ported yet.
                self.warn("SetVariables() : %s non porté", name);
                return Some(next);
            }
        }
        if is(self, "WAITFORFLAG") {
            self.a3.wait_flag = Some(value);
            return Some(self.wait(next));
        }
        if is(self, "WAITFORFINANIMP") || is(self, "WAITFORFINANIMO") {
            let perso = is(self, "WAITFORFINANIMP");
            let section = if perso {
                defnames::PERSONNAGES
            } else {
                defnames::OBJETS
            };
            let id = self.id(section, src, p + 1);
            let i = self.after_comma(src, p);
            let n = self.atoi_at(src, i);
            self.a3.wait_anim = Some((if perso { 1 } else { 2 }, id, n));
            return Some(self.wait(next));
        }
        if key == k.exit {
            self.vars[key] = value;
            log::info!("SetVariables: EXIT");
            self.quit = true;
            // The section ends (the walk goes on in an empty text).
            return Some(self.len(src));
        }
        if key == k.objetenmain {
            if self.b(src, p + 1) == b'0' {
                self.vars[key] = 0;
                self.empty_hand();
            } else {
                let id = self.id(defnames::OBJETS, src, p + 1);
                self.hold(id);
            }
            return Some(next);
        }
        if is(self, "SAVE") {
            host.quick_save(value, self);
            return Some(next);
        }
        if is(self, "LOAD") {
            host.quick_load(value, self);
            return Some(self.len(src));
        }
        if key == k.monde {
            self.vars[key] = value;
            self.a3.world = value;
            self.bag.clear();
            // The world's bag again from the flags (@ 0x0044bb30): the
            // objects 0..399 whose flag 1100 + id is this world, their
            // sprite in flag 1500 + id.
            for id in 0..400usize {
                if self.flags.get(1100 + id) == Some(&value) && value != 0 {
                    let sprite = self.flags.get(1500 + id).copied().unwrap_or(0);
                    self.bag.push(super::BagEntry {
                        sprite: sprite as u8,
                        id: id as i32,
                        world: value,
                    });
                }
            }
            self.set_cursor(0, false);
            host.set_monde(value);
            return Some(next);
        }
        // `CHANGECD` and every other key: the default store.
        self.vars[key] = value;
        Some(next)
    }

    /// `CHANGEWAM=name` (@ 0x00447c45): the place loaded, its show / hide
    /// commands run, its index kept in the world's flag.
    fn change_wam_a3(
        &mut self,
        host: &mut dyn Host,
        src: Src,
        p: usize,
        next: usize,
    ) -> Option<usize> {
        let mut wam = Vec::new();
        let mut i = p + 1;
        while self.b(src, i) != b')' && i < self.len(src) {
            wam.push(self.b(src, i));
            i += 1;
        }
        let name: String = wam.iter().map(|&c| char::from(c)).collect();
        if !self.load_wam_a3(host, &name) {
            return None;
        }
        Some(next)
    }

    /// `AtlWam_Load` and what `CHANGEWAM` does after it.
    fn load_wam_a3(&mut self, host: &mut dyn Host, name: &str) -> bool {
        if !host.load_wam(name) {
            self.travel_refused = true;
            return false;
        }
        self.wam = name.as_bytes().to_vec();
        self.current_view = None;
        let lower = name.to_ascii_lowercase();
        // `Interp_SetInterdimFlag` (@ 0x0044b910): flag 897 (0x0077d2f0)
        // tells the interdimension's first view entry to set its puzzle up.
        self.set_flag(FLAG_INTERDIM, i32::from(lower == "interdim.wam"));
        let cmds = host.wam_commands(name, &self.flags);
        let (saved, in_command) = (std::mem::take(&mut self.command), self.in_command);
        for c in cmds {
            self.run_command(host, c.as_bytes());
        }
        (self.command, self.in_command) = (saved, in_command);
        let Some(w) = self.world_slot("Changewam") else {
            return false;
        };
        let index = WAM_NAMES
            .iter()
            .position(|n| lower.strip_suffix(".wam") == Some(n))
            .map_or(-1, |i| i as i32);
        self.set_flag(FLAG_WORLD_WAM + w, index);
        true
    }

    /// The loader of a save (atlantis3-gog-fr-3473408 @ 0x0045b750), the
    /// script side, in its order: `(changecd=..)`, the place loaded
    /// (`AtlWam_Load` @ 0x0043eb00, without its commands), `(monde=..)`,
    /// the flags cleared and set, the bag rebuilt from the object names,
    /// the subject table, the place's own commands (@ 0x0045dd43), `(Vue=..)`,
    /// the camera, then the chapter loaded and its `/INIT` run
    /// (@ 0x00449fc0) and the world's view / chapter slots.
    pub fn load_a3(&mut self, host: &mut dyn Host, d: &LoadData) -> bool {
        log::info!("-- Debut Load --");
        self.run_command(host, format!("(changecd={})", d.current_cd).as_bytes());
        if !host.load_wam(&d.wam) {
            self.fail(format!("Load: {} introuvable", d.wam));
            return false;
        }
        self.wam = d.wam.as_bytes().to_vec();
        self.current_view = None;
        self.run_command(host, format!("(monde={})", d.monde).as_bytes());
        let previous = self.chapter;
        self.flags.iter_mut().for_each(|f| *f = 0);
        for &(i, v) in &d.flags {
            if let Some(f) = usize::try_from(i).ok().and_then(|i| self.flags.get_mut(i)) {
                *f = v;
            }
        }
        self.bag.clear();
        for name in &d.objects {
            // The loader keeps only the name's letters (@ 0x00445d00); the
            // object whose name has the same letters is found (the
            // original's run finds `HOG_OBJ_BATON` from `HOGOBJBATON`).
            let letters = |n: &[u8]| -> Vec<u8> {
                n.iter().copied().filter(u8::is_ascii_alphabetic).collect()
            };
            let want = letters(name);
            let found = (0..self.names.len(defnames::OBJETS)).find(|&i| {
                self.names
                    .name(defnames::OBJETS, i)
                    .is_some_and(|n| letters(n).eq_ignore_ascii_case(&want))
            });
            let Some(id) = found else {
                log::warn!("Load: objet {} inconnu", String::from_utf8_lossy(name));
                continue;
            };
            let sprite = self.names.param_of(defnames::OBJETS, id);
            self.bag_add(host, id as i32, sprite as u8);
        }
        self.subjects.reset_a3();
        for (perso, list) in &d.subjects {
            for (i, &(etat, id, value)) in list.iter().enumerate() {
                self.subjects.set_entry(
                    *perso,
                    i,
                    super::Subject {
                        value,
                        index: id,
                        flag: u8::from(etat),
                    },
                );
            }
            self.subjects.set_count(*perso, list.len() as i32);
        }
        let cmds = host.wam_commands(&d.wam, &self.flags);
        for c in cmds {
            self.run_command(host, c.as_bytes());
        }
        self.run_command(host, format!("(Vue={})", d.vue).as_bytes());
        if let Some(q) = d.camera {
            host.set_camera_quaternion(q);
        }
        self.previous_chapter = previous;
        self.chapter = d.chapter;
        self.vars[self.keys.chapter] = d.chapter;
        if !self.load_chapter(host, d.chapter) {
            self.fail(format!(
                "SetVariables(): chapitre introuvable scenar\\chapi{:03}.con",
                d.chapter
            ));
            return false;
        }
        self.run_init(host);
        if let Some(w) = self.world_slot("Load") {
            let (v, c) = (self.view_number, self.chapter);
            self.set_flag(FLAG_WORLD_VIEW + w, v);
            self.set_flag(FLAG_WORLD_CHAPTER + w, c);
        }
        log::info!("-- Fin de Load --");
        self.fatal.is_none()
    }

    /// `LOADVUEMONDE` (@ 0x0044890f): back to the current world's place,
    /// view and chapter, as kept in its flags.
    fn load_vue_monde(&mut self, host: &mut dyn Host) -> Option<usize> {
        let w = self.world_slot("LoadVueMonde")?;
        let wam = self.flags[FLAG_WORLD_WAM + w];
        let Some(name) = usize::try_from(wam).ok().and_then(|i| WAM_NAMES.get(i)) else {
            self.fail(format!("LoadVueMonde: pas de wam pour le monde {w}"));
            return None;
        };
        if !self.load_wam_a3(host, &format!("{name}.wam")) {
            return None;
        }
        let view = self.flags[FLAG_WORLD_VIEW + w];
        self.set_vue(host, view);
        let chapter = self.flags[FLAG_WORLD_CHAPTER + w];
        self.change_chapter(host, chapter)
    }

    /// The view's slot of its world, written by the view entry
    /// (`AtlWarp_EnterVue` @ 0x0043fc5a).
    pub fn note_world_view(&mut self, view: i32) {
        if let Some(w) = self.world_slot("Vue") {
            self.set_flag(FLAG_WORLD_VIEW + w, view);
        }
    }

    /// `CINEMA=n[,fps=f]` (@ 0x00447dcd): the line split into words, `fps`
    /// read (15 by default), `dial` dropped; the section waits only when
    /// the film started.
    fn cinema_a3(&mut self, host: &mut dyn Host, src: Src, p: usize, n: i32, next: usize) -> usize {
        let line = self.line_text(src, p + 1);
        let mut fps = 15;
        for word in line.split([' ', '\t', ',', ';', '(', ')']) {
            if word.len() >= 3 && word[..3].eq_ignore_ascii_case("fps") {
                if let Some(eq) = word.find('=') {
                    fps = atlantis2_data::con::atoi(&word.as_bytes()[eq + 1..]);
                }
            }
        }
        let started = host.play_cinema(n, fps);
        self.double_click = false;
        self.click = false;
        self.click_aux = 0;
        if started {
            self.wait(next)
        } else {
            next
        }
    }

    /// `DIAL=n[, voixoff]` (@ 0x00448d04): the value's letters removed,
    /// `atoi`; 0 → the value of the variable it names; `voixoff` anywhere
    /// in it. The section always waits.
    fn dial_a3(&mut self, host: &mut dyn Host, src: Src, p: usize, next: usize) -> usize {
        let text = self.line_text(src, p + 1);
        let digits: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_alphabetic()).collect();
        let mut n = atlantis2_data::con::atoi(&digits);
        if n == 0 {
            let v = self.id(defnames::VARIABLES, src, p + 1);
            n = self.vars[v as usize];
        }
        let voixoff = text.contains("voixoff");
        let next = self.wait(next);
        if host.dial_start_a3(n, voixoff) == DialStart::Failed {
            self.dialog_reset();
        }
        next
    }
}
