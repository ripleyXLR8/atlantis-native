//! The places' C hooks (`Game.exe` @ 0x004c2010 view entry, @ 0x004c27d0
//! every warp frame, @ 0x004c32c0 type-3 zone click), dispatched by the
//! place's WAM name and the view (`VUE`), and the full-screen loop modes
//! they start (the maps, the scarab case, the remote, the skulls...).
//!
//! Only what the scripts can see is reproduced here: the commands the hooks
//! run through `RunOneCommand`, the story flags and variables they write,
//! the bag and the hand, the loop modes they start and stop, the
//! `CtrlScenarIN` scans. The puzzles' own play (their drawing, their
//! animations' frames, their sounds) is ported board by board in M4 and
//! checked against the board goldens (`tests/common/board.rs`): the
//! merchant's house guard, the garden's banana bunch, monkey and soldiers,
//! the jewel case so far; a test walk skips a puzzle by writing its
//! result, after which the code here continues as after a real solution.
//!
//! Every function names the original's address; `FLAG[n]` is the story flag
//! array @ 0x0077c4ec (flag n at 0x0077c4ec + 4n).

use atlantis2_data::defnames;
use atlantis2_engine::script::Script;

use super::{Game, Mode};

mod couloir;
mod interdim;
mod jardin;
mod ldmorts;
mod mmarchd;
mod paleo;
mod pmagi;
mod rhea;
mod screens;
mod screens_draw;
mod tourmagi;

/// A full-screen loop mode started by a hook (the mode numbers of the main
/// loop's table @ 0x004d5e30).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// 9: the Land of the Dead's map (`AtlCarte`, @ 0x00405450).
    Carte,
    /// 10: the tower's map (`AtlCarteTour`, @ 0x00406620).
    CarteTour,
    /// 11: the unicorn board (`Licorne`, @ 0x0042e7a0).
    Licorne,
    /// 12: the game boy of the tower (`AtlGBoy`, @ 0x0041f660).
    GBoy,
    /// 13: the scarab case (`Scarabee`, @ 0x00431da0).
    Scarabee,
    /// 14: the jewel case (`AtlEcrin`, @ 0x00412e50).
    Ecrin,
    /// 15: a crystal skull (`AtlCraneBagdad`, @ 0x00409a40).
    CraneBagdad,
    /// 18: the remote control (`PuzzleTelecommande`, @ 0x004cfee0).
    Telecommande,
}

impl Screen {
    /// The main loop's mode number.
    pub fn mode_number(self) -> i32 {
        match self {
            Screen::Carte => 9,
            Screen::CarteTour => 10,
            Screen::Licorne => 11,
            Screen::GBoy => 12,
            Screen::Scarabee => 13,
            Screen::Ecrin => 14,
            Screen::CraneBagdad => 15,
            Screen::Telecommande => 18,
        }
    }
}

/// The hooks' C globals (each named after its address).
#[derive(Debug, Default, Clone)]
pub struct Puzzles {
    /// The clicked type-3 zone record copied by the click hook
    /// (@ 0x00785f38: +0 type, +4 value, +8 name); its value (@ 0x00785f3c)
    /// is cleared by the frame hook after the place's function saw it.
    pub zone: Option<(i32, i32, String)>,
    /// The spinning top's handler armed by the view entry (@ 0x004d3960).
    pub toupie_armed: bool,
    /// The spinning top's state (@ 0x007899d4): 0 idle, n = the top
    /// thrown on zone n spins.
    pub toupie_state: i32,
    /// The funeral chamber's star ring: the set-up byte (@ 0x0071d52c, 1
    /// until the first entry), the five last clicks (@ 0x0071d544,
    /// 1 = G, 2 = D, −1 none) and the star shown (@ 0x00786530).
    pub cfuner_fresh: bool,
    pub cfuner_ring: [i32; 5],
    pub cfuner_star: i32,
    /// `CHF_ETOILE01`'s `#OBJETS#` index (@ 0x0078652c), set at the first
    /// entry.
    pub cfuner_id: i32,
    /// The screen running and the mode to go back to (`SetMode`'s result).
    pub screen: Option<Screen>,
    pub screen_back: Option<Mode>,
    /// The input freeze (@ 0x00789d56): while set the mouse handler
    /// (@ 0x00431580) ignores the buttons.
    pub freeze: bool,
    /// The warp cursor a puzzle set (@ 0x00716e08: a sprite of the
    /// world's in-hand bank, `None` = −1; [`Game::set_warp_cursor`]).
    pub warp_cursor: Option<i32>,
    /// Tests: the sprite pass's cursor kind (@ 0x0076e928) as a harness
    /// wrote it; `None` in play (the hit test under the centre).
    pub cursor_kind: Option<i32>,
    /// The places' own globals.
    pub ldmorts: ldmorts::State,
    pub paleo: paleo::State,
    pub mmarchd: mmarchd::State,
    pub jardin: jardin::State,
    pub tourmagi: tourmagi::State,
    pub pmagi: pmagi::State,
    pub interdim: interdim::State,
    pub couloir: couloir::State,
    pub rhea: rhea::State,
    /// The screens' globals.
    pub screens: screens::State,
}

impl Puzzles {
    pub fn new() -> Puzzles {
        Puzzles {
            cfuner_fresh: true,
            cfuner_ring: [-1; 5],
            ldmorts: ldmorts::State::new(),
            paleo: paleo::State::new(),
            mmarchd: mmarchd::State::new(),
            jardin: jardin::State::new(),
            tourmagi: tourmagi::State::new(),
            pmagi: pmagi::State::new(),
            interdim: interdim::State::new(),
            couloir: couloir::State::new(),
            rhea: rhea::State::new(),
            screens: screens::State::new(),
            ..Default::default()
        }
    }

    /// The load's one-shot "rebuild from flags" bytes (@ 0x00459e70).
    pub fn after_load(&mut self) {
        self.cfuner_fresh = true;
        self.ldmorts.after_load();
        self.paleo.after_load();
        self.mmarchd.after_load();
        self.jardin.after_load();
        self.tourmagi.after_load();
        self.pmagi.after_load();
        self.interdim.after_load();
        self.couloir.after_load();
        self.rhea.after_load();
        self.screens.after_load();
    }
}

pub(in crate::game) use screens::ecrin_gem_hit;

/// Tests: the Land of the Dead boards' golden `st` lines.
pub(super) fn ldmorts_st(g: &Game, board: &str) -> String {
    ldmorts::st_line(g, board)
}

/// Tests: the guard's golden `st` line.
pub(super) fn mmarchd_st(m: &mmarchd::State, freeze: bool) -> String {
    mmarchd::st_line(m, freeze, m.flight_frame)
}

/// The solution of the funeral chamber's ring (@ 0x0071d530): G, D, D, G, D.
const CFUNER_SOLUTION: [i32; 5] = [1, 2, 2, 1, 2];

/// A hook's view of the game and the script (the script is out of the game
/// while it runs).
pub(super) struct Hx<'a> {
    pub g: &'a mut Game,
    pub s: &'a mut Script,
}

#[allow(dead_code)]
impl Hx<'_> {
    pub fn flag(&self, n: usize) -> i32 {
        self.s.flags.get(n).copied().unwrap_or(0)
    }

    pub fn set_flag(&mut self, n: usize, v: i32) {
        if let Some(f) = self.s.flags.get_mut(n) {
            *f = v;
        }
    }

    pub fn var(&self, name: &str) -> i32 {
        self.s.var(name).unwrap_or(0)
    }

    pub fn vue(&self) -> i32 {
        self.s.vars[self.s.keys.vue]
    }

    /// `RunOneCommand(command, 0)` (@ 0x00447840).
    pub fn run(&mut self, command: &str) {
        self.g.tr(|| format!("cmd {command}"));
        self.s.run_command(self.g, command.as_bytes());
    }

    /// `CtrlScenarIN` (@ 0x0044a2f0).
    pub fn ctrl(&mut self) {
        self.s.ctrl_scenar_in(self.g);
    }

    /// An object's `#OBJETS#` index (@ 0x00460860(1, name)), −1 if absent.
    pub fn obj(&self, name: &str) -> i32 {
        self.g
            .names
            .id(defnames::OBJETS, name.as_bytes())
            .map_or(-1, |i| i as i32)
    }

    /// A character's `#PERSONNAGES#` index (@ 0x00460860(0, name)).
    pub fn perso(&self, name: &str) -> i32 {
        self.g
            .names
            .id(defnames::PERSONNAGES, name.as_bytes())
            .map_or(-1, |i| i as i32)
    }

    pub fn hand(&self) -> i32 {
        self.s.vars[self.s.keys.objetenmain]
    }

    /// `Hand_Clear` (@ 0x00428670).
    pub fn hand_clear(&mut self) {
        self.s.empty_hand();
    }

    pub fn in_bag(&self, id: i32) -> bool {
        self.s.bag.iter().any(|e| e.id == id)
    }

    /// The world (@ 0x0077bd04).
    pub fn world(&self) -> i32 {
        self.s.a3.world
    }

    /// `rand()` (the CRT's, @ 0x006d348b).
    pub fn rand(&mut self) -> i32 {
        self.g.script_rand()
    }

    /// A loop mode's start: the mode set (the old one kept to go back to),
    /// the hand cleared by the starts that call @ 0x004282e0 (the tower's
    /// map, the jewel case, the skull, the remote; the frame hook clears
    /// it after the others but the game boy's and the unicorn's).
    pub fn start_screen(&mut self, screen: Screen) {
        if self.g.puzzles.screen.is_some() {
            log::warn!("{screen:?}_Start(): deja en AtlMain_loop_{screen:?}!!");
            return;
        }
        log::info!("{screen:?} start (mode {})", screen.mode_number());
        self.g.puzzles.screen = Some(screen);
        self.g.puzzles.screen_back = Some(self.g.mode);
        self.g.mode = Mode::Screen;
        if matches!(
            screen,
            Screen::CarteTour | Screen::Ecrin | Screen::CraneBagdad | Screen::Telecommande
        ) {
            self.hand_clear();
        }
        if screen == Screen::Licorne {
            // @ 0x0042e7a0: the unicorn's film first.
            self.run("(Cinema=77,fps=25)");
        }
        screens::start(self, screen);
    }

    /// A variable by name (`FUN_00446f90(name)` then a store).
    pub fn set_var(&mut self, name: &str, v: i32) {
        let _ = self.s.set_var(name, v);
    }

    /// The chapter running (@ 0x0077e508).
    pub fn chapter(&self) -> i32 {
        self.s.chapter
    }

    /// The game's clock (seconds).
    pub fn now(&self) -> f64 {
        self.g.now
    }

    /// A voice started by the C code (@ 0x0040da70(dialog, line, r, g,
    /// b), [`super::sfx`]).
    pub fn play_voice(&mut self, d: i32, l: i32, r: i32, g: i32, b: i32) {
        self.g.play_voice_c(d, l, r, g, b);
    }

    /// The slot of the n-th state shown in this view of the entity whose
    /// `defnames.con` index is `id` (@ 0x0043f7f0, the table @ 0x0077b2d0
    /// built by the view's load: (index, n) per slot, any kind; the first
    /// match).
    pub fn slot_of(&self, id: i32, n: i32) -> Option<usize> {
        let slots = &self.g.slots.slots;
        (0..slots.len()).find(|&k| {
            let s = &slots[k];
            let ord = slots[..k]
                .iter()
                .filter(|o| o.id == s.id && o.kind == s.kind)
                .count() as i32;
            s.id == id && ord == n
        })
    }

    /// The first slot of the object `name` whose slot kind (−8) is `kind`
    /// (the lookup of @ 0x0045f820 / @ 0x0045f920).
    pub fn obj_slot(&self, name: &str, kind: i32) -> Option<usize> {
        let id = self.obj(name);
        self.g
            .slots
            .slots
            .iter()
            .position(|s| s.id == id && s.kind == kind)
    }

    /// The frame (+0x08) of that slot, −1 when absent (@ 0x0045f820).
    pub fn obj_frame(&self, name: &str, kind: i32) -> i32 {
        self.obj_slot(name, kind)
            .map_or(-1, |k| self.g.slots.slots[k].frame)
    }

    /// The end byte (+0x46d) of that slot, false when absent
    /// (@ 0x0045f920).
    pub fn obj_ended(&self, name: &str, kind: i32) -> bool {
        self.obj_slot(name, kind)
            .is_some_and(|k| self.g.slots.slots[k].ended)
    }

    /// Slot k, when the view has it (the original indexes its fixed array
    /// @ 0x007594cc + 0x490 k).
    pub fn slot(&mut self, k: usize) -> Option<&mut crate::place_anim::Slot> {
        self.g.slots.slots.get_mut(k)
    }

    /// The interdimension (@ 0x00409cc0): the world left noted, then the
    /// interdimension's place at a view drawn by `rand`.
    pub fn enter_interdimension(&mut self) {
        let w = self.world();
        self.set_flag(890, w);
        if let Ok(w) = usize::try_from(w) {
            self.set_flag(890 + w, 1);
        }
        let n = self.flag(894) + self.flag(893) + self.flag(891);
        self.set_flag(892, n);
        self.run("(ChangeCD=7)");
        self.run("(monde=5)");
        self.run("(changewam=Interdim.wam)");
        self.run("(AngleWarpX=0)");
        self.run("(AngleWarpY=0)");
        let v = self.rand() % 6 + 2;
        self.run(&format!("(vue={v})"));
        self.run("(chapter=52)");
    }
}

impl Game {
    fn wam_is(&self, name: &str) -> bool {
        self.place.name.eq_ignore_ascii_case(name)
    }

    /// The view entry's hook (@ 0x004c2010): `(LIMITEANGLEWARPY=70)` at
    /// every entry, then the place's.
    pub(super) fn view_hook(&mut self, s: &mut Script) {
        let mut h = Hx { g: self, s };
        h.run("(LIMITEANGLEWARPY=70)");
        let vue = h.vue();
        let wam = h.g.place.name.to_ascii_lowercase();
        h.g.puzzles.toupie_armed = false;
        match wam.as_str() {
            // @ 0x004d3960: the spinning top's handler armed.
            "hoggar.wam" if (1..=3).contains(&vue) => h.g.puzzles.toupie_armed = true,
            "cfuner.wam" if (1..=4).contains(&vue) => cfuner_view(&mut h),
            _ => other_view(&mut h, &wam, vue),
        }
        // @ 0x004c2010's interdimension byte (@ 0x007592ac): 1 in
        // `interdim.wam`, 0 elsewhere.
        h.g.puzzles.interdim.here = wam == "interdim.wam";
        // Right after the view hook, at both view entries (@ 0x0043fc7b,
        // 0x0043fedb): the Ankh coffer's lid (@ 0x004c1650).
        ldmorts::coffer_entry(&mut h);
    }

    /// The warp frame's hook (@ 0x004c27d0), after `Interp_GereFrame`.
    pub(super) fn frame_hook(&mut self) {
        let Some(mut s) = self.script.take() else {
            return;
        };
        {
            let mut h = Hx { g: self, s: &mut s };
            frame(&mut h);
        }
        self.script = Some(s);
    }

    /// The zone click's hook (@ 0x004c32c0): the zone record kept for the
    /// next frame hook; the Hoggar's throws, the jewel case.
    pub(super) fn zone_hook(&mut self, kind: i32, value: i32, name: &str) {
        self.puzzles.zone = Some((kind, value, name.to_string()));
        let Some(mut s) = self.script.take() else {
            return;
        };
        {
            let mut h = Hx { g: self, s: &mut s };
            let vue = h.vue();
            if h.g.wam_is("hoggar.wam") && vue == 1 {
                toupie_throw(&mut h, name);
            }
            if h.g.wam_is("jardin.wam")
                && name == "BAG_ZONE_ECRIN"
                && h.flag(144) == 1
                && h.flag(131) == 0
            {
                h.start_screen(Screen::Ecrin);
                h.set_flag(131, 1);
            }
        }
        self.script = Some(s);
    }

    /// The running screen's frame ([`screens`]). The two maps and the
    /// jewel case draw themselves in their handlers and hit-test the
    /// pointer; the other screens' drawing and their sprite hit test are
    /// M4: a real click finds no sprite under the cursor (tests give it
    /// through [`Game::screen_click`]).
    pub(super) fn screen_frame(&mut self) {
        if !matches!(
            self.puzzles.screen,
            Some(Screen::Carte | Screen::CarteTour | Screen::Ecrin)
        ) {
            self.draw_warp_only();
            self.draw_pointer();
        }
        let left = std::mem::take(&mut self.click);
        let right = std::mem::take(&mut self.right_clicked);
        self.screen_step(left, right, None);
    }

    /// A screen's stop: the old mode back, the hand cleared, the click
    /// bytes cleared, `CtrlScenarIN` (all but the skull's, which goes back
    /// to the warp without a scan, @ 0x00409b00; the jewel case puts its
    /// three gems in the bag first, @ 0x00412f40; the game boy's music
    /// after the scan, @ 0x0041f790).
    pub fn stop_screen(&mut self) {
        let Some(screen) = self.puzzles.screen.take() else {
            return;
        };
        self.tr(|| "stop".into());
        let back = self.puzzles.screen_back.take().unwrap_or(Mode::Warp);
        self.click = false;
        self.right_clicked = false;
        let Some(mut s) = self.script.take() else {
            self.mode = back;
            return;
        };
        {
            let mut h = Hx { g: self, s: &mut s };
            match screen {
                Screen::CraneBagdad => {
                    // Mode 3, the hand cleared (@ 0x004282e0), no scan.
                    h.g.mode = Mode::Warp;
                    h.hand_clear();
                }
                _ => {
                    h.g.mode = back;
                    if screen != Screen::Licorne {
                        h.hand_clear();
                    }
                    if screen == Screen::Ecrin {
                        h.run("(SacADos=BAG_SAPHIR)");
                        h.run("(SacADos=BAG_RUBIS)");
                        h.run("(SacADos=BAG_EMERAUDE)");
                    }
                    h.ctrl();
                    if screen == Screen::GBoy {
                        h.run("(StopMusik=1)");
                        h.run("(StartMusik=6)");
                    }
                }
            }
        }
        self.script = Some(s);
    }

    /// The main loop's per-frame hook (@ 0x004c1950, called from the main
    /// loop @ 0x004d5df9 in every mode): the Ankh coffer's lid.
    pub(super) fn main_loop_hook(&mut self) {
        let Some(mut s) = self.script.take() else {
            return;
        };
        {
            let mut h = Hx { g: self, s: &mut s };
            ldmorts::coffer_frame(&mut h);
        }
        self.script = Some(s);
    }

    /// After a load's one-shot bytes (@ 0x00459e70), the coffer's lid
    /// rebuilt (@ 0x004c1e20: 0x00785f2f := 0, then @ 0x004c1650).
    pub(super) fn after_load_hook(&mut self) {
        let Some(mut s) = self.script.take() else {
            return;
        };
        {
            let mut h = Hx { g: self, s: &mut s };
            ldmorts::coffer_entry(&mut h);
        }
        self.script = Some(s);
    }

    /// [`Game::after_load_hook`] while the script is out of the game.
    pub(super) fn after_load_hook_with(&mut self, s: &mut Script) {
        let mut h = Hx { g: self, s };
        ldmorts::coffer_entry(&mut h);
    }

    /// A C global of a puzzle written as a test walk's `MEM8` / `MEM32`
    /// item does (a state the puzzle's code, or an animation, would have
    /// reached); false for an address not modelled.
    #[doc(hidden)]
    pub fn test_poke(&mut self, addr: u32, v: i32) -> bool {
        let b = v != 0;
        let p = &mut self.puzzles;
        match addr {
            // The fisherman's animation ended (SPW slot 3, +0x46d).
            0x0075a6e9 => match self.slots.slots.get_mut(3) {
                Some(s) => s.ended = b,
                None => return false,
            },
            0x00785f20 => p.ldmorts.lid.reached = v,
            0x00786510 => p.couloir.anim.reached = v,
            0x00785fd1 => p.jardin.banana_done = b,
            0x00785fd2 => p.jardin.banana_playing = b,
            0x007897a8 => p.pmagi.placard.current = v,
            0x00789810 => p.pmagi.lampe.current = v,
            0x00787559 => p.paleo.wolves_won = b,
            0x00786d88 => p.paleo.hunt_hit[0] = b,
            0x00786d48 => p.paleo.hunt_hit[1] = b,
            0x00786dc8 => p.paleo.hunt_hit[2] = b,
            0x00786d68 => p.paleo.hunt_hit[3] = b,
            0x0078977d => p.paleo.tiger_throw = b,
            0x00789780 => p.paleo.tiger_count = v,
            0x0071f271 => p.paleo.tiger_hit = b,
            0x00789d56 => p.freeze = b,
            0x0076e928 => p.cursor_kind = Some(v),
            0x00789ad4 => p.tourmagi.player = v,
            0x00789ae2 => p.tourmagi.won_seen = b,
            _ => return false,
        }
        true
    }

    /// A puzzle's function called as a test walk's `CALL` item does (its
    /// success tail or its screen's end); false when not modelled.
    #[doc(hidden)]
    pub fn test_call(&mut self, addr: u32) -> bool {
        match addr {
            // The Licorne's and the jewel case's ends.
            0x0042e990 | 0x00412f40 => {
                self.stop_screen();
                return true;
            }
            // The camera unlocked.
            0x00440750 => {
                self.puzzles.mmarchd.cam_lock = false;
                return true;
            }
            _ => {}
        }
        let Some(mut s) = self.script.take() else {
            return false;
        };
        let known = {
            let mut h = Hx { g: self, s: &mut s };
            match addr {
                0x004c8720 => ldmorts::isis_success(&mut h),
                0x004d56b0 => tourmagi::player_won(&mut h),
                0x004cf850 => pmagi::stars_solved(&mut h),
                0x004cac90 => paleo::wolves_success(&mut h),
                0x004d15a0 => paleo::tiger_success(&mut h),
                0x004c9860 => paleo::hunt_end(&mut h),
                _ => {
                    self.script = Some(s);
                    return false;
                }
            }
            true
        };
        self.script = Some(s);
        known
    }

    /// The screen running, if any.
    pub fn puzzle_screen(&self) -> Option<Screen> {
        self.puzzles.screen
    }
}

/// The frame hook (@ 0x004c27d0) in its order: the places' parts, the
/// generic hand tests, the later places' parts.
fn frame(h: &mut Hx) {
    let vue = h.vue();
    let wam = h.g.place.name.to_ascii_lowercase();
    match wam.as_str() {
        "hoggar.wam" if (1..=3).contains(&vue) => hoggar_toupie_frame(h),
        "cfuner.wam" if vue == 1 => {
            cfuner_frame(h);
            clear_zone(h);
        }
        "paleo.wam" | "ldmorts.wam" => other_frame_first(h, &wam, vue),
        _ => {}
    }
    // The generic hand tests (@ 0x004c2b2a..0x004c2d30).
    if h.flag(147) == 1 {
        h.start_screen(Screen::Licorne);
    }
    let hand = h.hand();
    if hand == h.obj("LDM_CARTELIEUX") {
        h.start_screen(Screen::Carte);
        h.hand_clear();
    }
    if h.hand() == h.obj("BAG_MAP_TOUR") {
        h.start_screen(Screen::CarteTour);
        h.hand_clear();
    }
    if h.hand() == h.obj("CHF_ECRIN_SCARABEE") {
        h.start_screen(Screen::Scarabee);
        h.hand_clear();
    }
    if h.flag(900) == 1 {
        h.hand_clear();
        h.enter_interdimension();
        h.set_flag(900, 0);
        h.set_flag(901, 1);
    } else if h.flag(901) == 0 {
        let hand = h.hand();
        let skull = [
            ("HOG_OBJ_CRANE", 891),
            ("PMA_CRANE", 893),
            ("PAL_CRANE", 117),
        ]
        .into_iter()
        .find(|(n, _)| hand == h.obj(n));
        if let Some((_, f)) = skull {
            if h.flag(f) != 0 {
                h.start_screen(Screen::CraneBagdad);
                h.hand_clear();
            }
        }
    }
    let hand = h.hand();
    if ["CMP_TELECOMMANDE", "BAG_TELECOMMANDE", "PAL_TELECOMMANDE"]
        .iter()
        .any(|n| hand == h.obj(n))
    {
        h.start_screen(Screen::Telecommande);
        h.hand_clear();
    }
    if h.hand() == h.obj("CMP_OMEGA") {
        h.enter_interdimension();
        h.hand_clear();
    }
    other_frame_last(h, &wam);
}

/// The zone record's value cleared (@ 0x00785f3c := 0).
fn clear_zone(h: &mut Hx) {
    if let Some(z) = h.g.puzzles.zone.as_mut() {
        z.1 = 0;
    }
}

/// The clicked zone record (@ 0x00785f38) as the frame functions get it,
/// by value: +0 type, +4 value (0 = no click to handle), +8 name (kept
/// from the last click once the value is cleared).
#[derive(Debug, Default, Clone)]
pub(super) struct ZoneRec {
    pub kind: i32,
    pub value: i32,
    pub name: String,
}

impl ZoneRec {
    /// A type-3 zone click not yet handled, on the zone `name`.
    pub fn is(&self, name: &str) -> bool {
        self.value != 0 && self.name == name
    }
}

/// The record copied (the frame hook's 0x30-byte copy).
fn zone_rec(h: &Hx) -> ZoneRec {
    h.g.puzzles
        .zone
        .as_ref()
        .map(|(kind, value, name)| ZoneRec {
            kind: *kind,
            value: *value,
            name: name.clone(),
        })
        .unwrap_or_default()
}

/// The clicked zone's name when the record's value is not 0 (a type-3
/// zone click not yet handled).
fn clicked_zone(h: &Hx) -> Option<String> {
    h.g.puzzles
        .zone
        .as_ref()
        .filter(|z| z.1 != 0)
        .map(|z| z.2.clone())
}

/// The spinning top's animations (`HOG_ANIM_TOUPIE01..05`, the CStrings
/// @ 0x00789958 + 0x18 (n − 1)) and the stars (`HOG_ETOILE01..05`, flags
/// 420..424).
const TOUPIE_ANIMS: [&str; 5] = [
    "HOG_ANIM_TOUPIE01",
    "HOG_ANIM_TOUPIE02",
    "HOG_ANIM_TOUPIE03",
    "HOG_ANIM_TOUPIE04",
    "HOG_ANIM_TOUPIE05",
];

/// Per zone, in the step's order (@ 0x004d3a9c..0x004d40f4): the
/// animation frame past which (`>`) a star (1..5) goes out (`false`,
/// every frame) or lights (`true`, once: while its flag is 0).
const TOUPIE_STARS: [&[(i32, usize, bool)]; 5] = [
    &[
        (0x2c, 2, false),
        (0x48, 5, false),
        (100, 3, false),
        (0x7c, 1, true),
    ],
    &[(0x2c, 2, true)],
    &[(0x18, 2, false), (0x36, 5, false), (0x5e, 3, true)],
    &[
        (0x18, 2, false),
        (0x36, 5, false),
        (0x56, 3, false),
        (0x6b, 1, false),
        (0x95, 4, true),
    ],
    &[(0x27, 2, false), (0x3b, 5, true)],
];

/// The throw (@ 0x004d4160, the zone click hook in `hoggar.wam` view 1):
/// armed, `Flag13` ≠ 1, the top in hand and the zone one of
/// `HOG_TOUPIE_ZONE01..05` → its animation shown at frame 0, the hand
/// emptied, the top out of the bag, the state := n, the input frozen.
fn toupie_throw(h: &mut Hx, zone: &str) {
    if !h.g.puzzles.toupie_armed || h.flag(13) == 1 {
        return;
    }
    if h.hand() != h.obj("HOG_OBJ_TOUPIE") {
        return;
    }
    let Some(n) = (1..=5).find(|n| zone == format!("HOG_TOUPIE_ZONE0{n}")) else {
        return;
    };
    h.run(&format!("(showobjet=HOG_ANIM_TOUPIE0{n})"));
    h.run(&format!("(gotoframeobjet=HOG_ANIM_TOUPIE0{n},0)"));
    h.run("(objetenmain=0)");
    h.run("(sacados=NOT_HOG_OBJ_TOUPIE)");
    h.g.puzzles.toupie_state = n;
    h.g.puzzles.freeze = true;
}

/// The Hoggar's views 1..3 (@ 0x004d3980): the spinning top.
///
/// - another view than 1: flag 415 = 1, `Flag13` ≠ 1 and the top not in
///   the bag (a throw left unfinished) → the five animations hidden, the
///   top back in the bag, the state 0, the input free;
/// - view 1: the top in the bag → flag 415 = 1; idle with flags 420..424
///   (the five stars) all 1 → `Flag13` = 1, `(sacados=NOT_HOG_OBJ_TOUPIE)`,
///   the input free, `CtrlScenarIN` (no test of `Flag13`: the success
///   repeats every frame until the scripts leave the view); a throw on
///   zone n: the stars of its table from the animation's frame (the
///   first slot of the object, @ 0x0045f820), then at the animation's end
///   (its ended byte, @ 0x0045f920) the animation hidden, the top back in
///   the bag and in hand, the state 0, the input free.
fn hoggar_toupie_frame(h: &mut Hx) {
    if !h.g.puzzles.toupie_armed {
        return;
    }
    let vue = h.vue();
    let top = h.obj("HOG_OBJ_TOUPIE");
    if vue != 1 {
        if h.flag(415) == 1 && h.flag(13) != 1 && !h.in_bag(top) {
            for name in TOUPIE_ANIMS {
                let id = h.obj(name);
                h.g.slot_hide(id, 2);
            }
            h.run("(sacados=HOG_OBJ_TOUPIE)");
            h.g.puzzles.toupie_state = 0;
            h.g.puzzles.freeze = false;
        }
        return;
    }
    if h.in_bag(top) {
        h.set_flag(415, 1);
    }
    let state = h.g.puzzles.toupie_state;
    if state == 0 {
        if (420..=424).all(|f| h.flag(f) == 1) {
            h.set_flag(13, 1);
            h.run("(sacados=NOT_HOG_OBJ_TOUPIE)");
            h.g.puzzles.freeze = false;
            h.ctrl();
        }
        return;
    }
    let Some(table) = usize::try_from(state - 1)
        .ok()
        .and_then(|i| TOUPIE_STARS.get(i))
    else {
        return;
    };
    let anim = TOUPIE_ANIMS[(state - 1) as usize];
    for &(threshold, star, on) in table.iter() {
        if h.obj_frame(anim, 2) <= threshold {
            continue;
        }
        let flag = 419 + star;
        let name = format!("HOG_ETOILE0{star}");
        let id = h.obj(&name);
        if on {
            if h.flag(flag) == 0 {
                h.g.slot_show(id, 2, 0);
                h.run(&format!("(gotoframeobjet={name},0)"));
                h.run("(playsound=1)");
                h.set_flag(flag, 1);
            }
        } else {
            h.g.slot_hide(id, 2);
            h.set_flag(flag, 0);
        }
    }
    if h.obj_ended(anim, 2) {
        let id = h.obj(anim);
        h.g.slot_hide(id, 2);
        h.run("(sacados=HOG_OBJ_TOUPIE)");
        h.run("(ObjetEnMain=HOG_OBJ_TOUPIE)");
        h.g.puzzles.toupie_state = 0;
        h.g.puzzles.freeze = false;
    }
}

/// The funeral chamber's views 1..4 (@ 0x004c4de0): at the first entry
/// (and after a load) the star's frames 0..4 taken out of its list (when
/// listed), the ring emptied and the star from flag 500; then, in
/// `CFuner.wam` (a case-sensitive compare), the star's frame put in its
/// list (the star `CHF_ETOILE01` is a mode-0 slot showing its list).
fn cfuner_view(h: &mut Hx) {
    if h.g.puzzles.cfuner_fresh {
        let id = h.obj("CHF_ETOILE01");
        h.g.puzzles.cfuner_id = id;
        for f in 0..5 {
            h.subf_if_listed(id, f);
        }
        h.g.puzzles.cfuner_ring = [-1; 5];
        h.g.puzzles.cfuner_star = h.flag(500);
    }
    if h.g.place.name == "CFuner.wam" {
        let (id, star) = (h.g.puzzles.cfuner_id, h.g.puzzles.cfuner_star);
        h.addf(id, star);
    }
    h.g.puzzles.cfuner_fresh = false;
}

/// The funeral chamber's view 1 (@ 0x004c4f50): a click on the left or
/// right star zone turns the ring of stars (sounds 101 / 102, flag 500 =
/// the star shown); the last five clicks equal to G, D, D, G, D →
/// `(flag45=1)` and `CtrlScenarIN`. Nothing once `flag45` = 1.
fn cfuner_frame(h: &mut Hx) {
    if h.flag(45) == 1 {
        return;
    }
    if let Some(zone) = clicked_zone(h) {
        let (id, old) = (h.g.puzzles.cfuner_id, h.g.puzzles.cfuner_star);
        if zone == "CHF_ETOILEZONEG" || zone == "CHF_ETOILEZONED" {
            h.subf(id, old);
        }
        let p = &mut h.g.puzzles;
        let sound = if zone == "CHF_ETOILEZONEG" {
            p.cfuner_ring.rotate_left(1);
            p.cfuner_ring[4] = 1;
            p.cfuner_star -= 1;
            if p.cfuner_star == -1 {
                p.cfuner_star = 4;
            }
            Some("(PlaySound = 101)")
        } else if zone == "CHF_ETOILEZONED" {
            p.cfuner_ring.rotate_left(1);
            p.cfuner_ring[4] = 2;
            p.cfuner_star += 1;
            if p.cfuner_star == 5 {
                p.cfuner_star = 0;
            }
            Some("(PlaySound = 102)")
        } else {
            None
        };
        if let Some(sound) = sound {
            let star = h.g.puzzles.cfuner_star;
            h.set_flag(500, star);
            h.run(sound);
            h.addf(id, star);
        }
    }
    if h.g.puzzles.cfuner_ring == CFUNER_SOLUTION {
        h.run("(flag45=1)");
        h.ctrl();
    }
}

/// The other places' view entries (@ 0x004c2010's cases).
fn other_view(h: &mut Hx, wam: &str, vue: i32) {
    match wam {
        "paleo.wam" => match vue {
            1 | 2 => paleo::view_wolves(h),
            4 => paleo::view_hunt(h),
            5..=11 => paleo::view_altar(h),
            22 | 25 | 27..=29 | 31 | 32 | 35..=44 | 48..=51 => paleo::view_common(h),
            26 => {
                paleo::view_common(h);
                paleo::view_tiger(h);
            }
            _ => {}
        },
        "ldmorts.wam" => match vue {
            2..=4 | 20 => ldmorts::view_chest(h),
            7..=10 => {
                clear_zone(h);
                ldmorts::view_isis(h);
            }
            11 => ldmorts::view_fisherman(h),
            _ => {}
        },
        "mmarchd.wam" => mmarchd::view(h),
        "jardin.wam" => jardin::view(h, vue),
        "tourmagi.wam" => tourmagi::view(h),
        "pmagi.wam" => match vue {
            10 => {
                h.run("(LIMITEANGLEWARPY=85)");
                pmagi::view_stars(h);
            }
            11 | 12 => pmagi::view_tt(h),
            111 | 112 => pmagi::view_tr(h),
            _ => {}
        },
        "interdim.wam" => interdim::view(h),
        "couloir.wam" => couloir::view(h),
        "rhea.wam" => rhea::view(h),
        _ => {}
    }
}

/// The places whose frame part comes before the hand tests (`paleo.wam`,
/// `ldmorts.wam`; @ 0x004c27d0's first cases). Each "(zone)" function
/// gets the record by value, then its value is cleared.
fn other_frame_first(h: &mut Hx, wam: &str, vue: i32) {
    let z = zone_rec(h);
    match wam {
        "paleo.wam" => match vue {
            1 | 2 => {
                paleo::frame_wolves(h, &z);
                clear_zone(h);
            }
            4 => {
                paleo::frame_hunt(h, &z);
                clear_zone(h);
            }
            5..=8 => {
                paleo::frame_altar(h, &z);
                clear_zone(h);
            }
            22 | 25 | 27..=29 | 31 | 32 | 35..=44 | 48..=51 => paleo::frame_common(h),
            26 => {
                paleo::frame_common(h);
                let z = zone_rec(h);
                paleo::frame_tiger(h, &z);
                clear_zone(h);
            }
            _ => {}
        },
        "ldmorts.wam" => {
            match vue {
                3 | 4 | 20 => ldmorts::frame_chest(h, &z),
                10 => ldmorts::frame_isis(h, &z),
                11 => ldmorts::frame_fisherman(h, &z),
                _ => return,
            }
            clear_zone(h);
        }
        _ => {}
    }
}

/// The places whose frame part comes after the hand tests (@ 0x004c27d0's
/// later cases, in its order). The WAM is the one read at the hook's
/// start; `VUE` is read again at each place.
fn other_frame_last(h: &mut Hx, wam: &str) {
    if wam == "mmarchd.wam" {
        let z = zone_rec(h);
        mmarchd::frame(h, &z);
        clear_zone(h);
    }
    if wam == "jardin.wam" {
        let vue = h.vue();
        jardin::frame(h, vue);
    }
    // The tower's byte (@ 0x00789ae3): 1 in `tourmagi.wam`, 0 elsewhere.
    if wam == "tourmagi.wam" {
        let z = zone_rec(h);
        tourmagi::frame(h, &z);
        clear_zone(h);
        h.g.puzzles.tourmagi.here = true;
    } else {
        h.g.puzzles.tourmagi.here = false;
    }
    if wam == "pmagi.wam" {
        let z = zone_rec(h);
        let handled = match h.vue() {
            10 => {
                pmagi::frame_stars(h, &z);
                true
            }
            // @ 0x007898d4 set: nothing, not even the clear.
            11 | 12 if !h.g.puzzles.pmagi.tt_done => {
                pmagi::frame_tt(h, &z);
                if h.g.puzzles.pmagi.tt_end {
                    pmagi::frame_tt_end(h);
                }
                true
            }
            111 | 112 => {
                pmagi::frame_tr(h, &z);
                true
            }
            _ => false,
        };
        if handled {
            clear_zone(h);
        }
    }
    if wam == "interdim.wam" {
        let z = zone_rec(h);
        interdim::frame(h, &z);
        clear_zone(h);
    }
    if wam == "couloir.wam" && !h.g.puzzles.couloir.solved {
        let z = zone_rec(h);
        couloir::frame(h, &z);
        if h.g.puzzles.couloir.solved {
            couloir::open(h);
        }
        clear_zone(h);
    }
    if wam == "rhea.wam" {
        let z = zone_rec(h);
        rhea::frame(h, &z);
        clear_zone(h);
    }
}
