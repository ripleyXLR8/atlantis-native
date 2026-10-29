//! The places' C hooks (`Game.exe` @ 0x004c2010 view entry, @ 0x004c27d0
//! every warp frame, @ 0x004c32c0 type-3 zone click), dispatched by the
//! place's WAM name and the view (`VUE`), and the full-screen loop modes
//! they start (the maps, the scarab case, the remote, the skulls...).
//!
//! Only what the scripts can see is reproduced here: the commands the hooks
//! run through `RunOneCommand`, the story flags and variables they write,
//! the bag and the hand, the loop modes they start and stop, the
//! `CtrlScenarIN` scans. The puzzles' own play (their drawing, their
//! animations' frames) is M4; a test walk skips a puzzle by writing its
//! result, after which the code here continues as after a real solution.
//!
//! Every function names the original's address; `FLAG[n]` is the story flag
//! array @ 0x0077c4ec (flag n at 0x0077c4ec + 4n).

use atlantis2_data::defnames;
use atlantis2_engine::script::Script;

use super::{Game, Mode};

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
    /// The funeral chamber's star ring: the set-up byte (@ 0x0071d52c, 1
    /// until the first entry), the five last clicks (@ 0x0071d544,
    /// 1 = G, 2 = D, −1 none) and the star shown (@ 0x00786530).
    pub cfuner_fresh: bool,
    pub cfuner_ring: [i32; 5],
    pub cfuner_star: i32,
    /// The screen running and the mode to go back to (`SetMode`'s result).
    pub screen: Option<Screen>,
    pub screen_back: Option<Mode>,
}

impl Puzzles {
    pub fn new() -> Puzzles {
        Puzzles {
            cfuner_fresh: true,
            cfuner_ring: [-1; 5],
            ..Default::default()
        }
    }

    /// The load's one-shot "rebuild from flags" bytes (@ 0x00459e70).
    pub fn after_load(&mut self) {
        self.cfuner_fresh = true;
    }
}

/// The solution of the funeral chamber's ring (@ 0x0071d530): G, D, D, G, D.
const CFUNER_SOLUTION: [i32; 5] = [1, 2, 2, 1, 2];

/// A hook's view of the game and the script (the script is out of the game
/// while it runs).
pub(super) struct Hx<'a> {
    pub g: &'a mut Game,
    pub s: &'a mut Script,
}

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
        self.g.rand.rand()
    }

    /// A loop mode's start: the mode set (the old one kept to go back to),
    /// the hand cleared (all but the unicorn's).
    pub fn start_screen(&mut self, screen: Screen) {
        if self.g.puzzles.screen.is_some() {
            log::warn!("{screen:?}_Start(): deja en AtlMain_loop_{screen:?}!!");
            return;
        }
        log::info!("{screen:?} start (mode {})", screen.mode_number());
        self.g.puzzles.screen = Some(screen);
        self.g.puzzles.screen_back = Some(self.g.mode);
        self.g.mode = Mode::Screen;
        if screen != Screen::Licorne {
            self.hand_clear();
        }
        if screen == Screen::Licorne {
            // @ 0x0042e7a0: the unicorn's film first.
            self.run("(Cinema=77,fps=25)");
        }
    }

    /// The interdimension (@ 0x00409cc0): the world left noted, then the
    /// interdimension's place at a view drawn by `rand`.
    pub fn to_interdimension(&mut self) {
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
            "hoggar.wam" | "cfuner.wam" => {}
            _ => other_view(&mut h, &wam, vue),
        }
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
                // @ 0x004d4160: a throw of the top (M4).
                h.g.not_ported("the spinning top's throws");
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

    /// The running screen's frame. The screens' play is M4: they wait for
    /// a right click (leave) or for a test's input.
    pub(super) fn screen_frame(&mut self) {
        self.draw_warp_only();
        self.draw_pointer();
        if std::mem::take(&mut self.right_clicked) {
            self.stop_screen();
        }
        self.click = false;
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
                    h.g.mode = Mode::Warp;
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
        h.to_interdimension();
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
        h.to_interdimension();
        h.hand_clear();
    }
    if wam != "hoggar.wam" && wam != "cfuner.wam" && wam != "paleo.wam" && wam != "ldmorts.wam" {
        other_frame_last(h, &wam, vue);
    }
}

/// The zone record's value cleared (@ 0x00785f3c := 0).
fn clear_zone(h: &mut Hx) {
    if let Some(z) = h.g.puzzles.zone.as_mut() {
        z.1 = 0;
    }
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

/// The Hoggar's views 1..3 (@ 0x004d3980): the spinning top. The throws and
/// the stars' animation are M4; its success check and the top's return:
///
/// - `VUE` = 1: the top in the bag → flag 415 = 1; no throw running and
///   flags 420..424 (the five stars) all 1 → `Flag13` = 1,
///   `(sacados=NOT_HOG_OBJ_TOUPIE)`, then `CtrlScenarIN`
///   (@ 0x004d3adf..0x004d3b35);
/// - another view: flag 415 = 1, `Flag13` ≠ 1 and the top not in the
///   bag (a throw left unfinished) → the top back in the bag.
fn hoggar_toupie_frame(h: &mut Hx) {
    if !h.g.puzzles.toupie_armed {
        return;
    }
    let vue = h.vue();
    let top = h.obj("HOG_OBJ_TOUPIE");
    let in_bag = h.in_bag(top);
    if vue != 1 {
        if h.flag(415) == 1 && h.flag(13) != 1 && !in_bag {
            h.run("(sacados=HOG_OBJ_TOUPIE)");
        }
        return;
    }
    if in_bag {
        h.set_flag(415, 1);
    }
    // No test of `Flag13` (@ 0x004d3adf): in view 1 the success repeats
    // every frame until the scripts leave.
    if (420..=424).all(|f| h.flag(f) == 1) {
        h.set_flag(13, 1);
        h.run("(sacados=NOT_HOG_OBJ_TOUPIE)");
        h.ctrl();
    }
}

/// The funeral chamber's views 1..4 (@ 0x004c4de0): at the first entry
/// (and after a load) the ring emptied and the star shown from flag 500;
/// in `CFuner.wam` the star's slot shown at that frame (place sprites).
fn cfuner_view(h: &mut Hx) {
    if h.g.puzzles.cfuner_fresh {
        h.g.puzzles.cfuner_ring = [-1; 5];
        h.g.puzzles.cfuner_star = h.flag(500);
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
        }
    }
    if h.g.puzzles.cfuner_ring == CFUNER_SOLUTION {
        h.run("(flag45=1)");
        h.ctrl();
    }
}

/// The other places' view entries (filled world by world).
fn other_view(h: &mut Hx, wam: &str, vue: i32) {
    let _ = vue;
    match wam {
        "couloir.wam" | "rhea.wam" | "interdim.wam" | "paleo.wam" | "ldmorts.wam"
        | "mmarchd.wam" | "jardin.wam" | "tourmagi.wam" | "pmagi.wam" => {
            h.g.not_ported("the places' puzzle hooks at view entry")
        }
        _ => {}
    }
}

/// The places whose frame part comes before the hand tests (`paleo.wam`,
/// `ldmorts.wam`).
fn other_frame_first(h: &mut Hx, wam: &str, vue: i32) {
    let _ = (wam, vue);
    h.g.not_ported("the places' frame hooks");
}

/// The places whose frame part comes after the hand tests.
fn other_frame_last(h: &mut Hx, wam: &str, vue: i32) {
    let _ = vue;
    if matches!(
        wam,
        "mmarchd.wam"
            | "jardin.wam"
            | "tourmagi.wam"
            | "pmagi.wam"
            | "interdim.wam"
            | "couloir.wam"
            | "rhea.wam"
    ) {
        h.g.not_ported("the places' frame hooks");
    }
}
