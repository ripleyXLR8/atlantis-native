//! The octopus fight of Atlantis (`atlantis.wam`, the `AT` views 1-19,
//! chapter 82; the original's module `pz_poulpe`, puzzle mode 6).
//!
//! - **The entry** FUN_0049a8c0 (from the view entry's dispatcher
//!   FUN_0049bd50 in every `AT` view numbered 1..19, after the view's own
//!   slots are loaded): the puzzle mode 6; the crosshair bank
//!   `viseur.spr`, loaded once; the camera's reset FUN_0045b4b0 (below);
//!   the records (five of 0x5c bytes @ 0x006d82d8) cleared; view 16 has
//!   the big octopus (prefix `GDP_`, body `GDP_CORPS_V`), the others the
//!   small ones (`PTP_`, `PTP_CORPS_V`); then `load_poulpes`
//!   FUN_0049b010.
//! - **`load_poulpes`**: the sectors' table (@ 0x006d829c, one word per
//!   last digit of an entity's name) holds 1 for digit 1 with the big
//!   octopus, nothing else: the small octopuses are never loaded, and the
//!   big one is `GDP_CORPS_V1` with `GDP_TENTA_V1`. Every state of a
//!   matching entity (the prefix by `_strnicmp` over four characters, in
//!   the place's order) gets a slot (FUN_0047c4e0, the template view 802's
//!   files), shown and playing; a tentacles' entity (not the body's 11
//!   characters) makes a record: per state a tentacle (6 hits, its slot's
//!   frame `random(39)`; with the big octopus a tentacle whose
//!   `BigTenta<k>Died` is 1 is hidden and stopped, not counted), the body's
//!   5 hits, its countdown `random(50) + 10`, its sector (the name's last
//!   digit). Then each record's body slot (`<body><sector>`, animation 1)
//!   and, for the small ones, its death (animation 2, hidden).
//! - **The tick** FUN_0049aaa0 (the puzzle tick FUN_0049bfb0's mode 6,
//!   every warp frame): outside views 1..19 the quit and nothing else. The
//!   view's yaw in degrees (`x · 180.0f / π` as a float) gives a sector
//!   (FUN_0049b3a0: 4 for (−40, 40), 2 for (50, 130), 3 for (140, 220), 1
//!   for (−130, −50), 0 in the gaps); unless a record attacks, a known
//!   sector marks the records looked at (theirs) or not. Per record, in
//!   order: the blink of a shot tentacle (state 3 → 4), a small body's
//!   death at its frame 35 (`PoulpeWarp<view>` = 1, the view's zones of
//!   type 0x7af made moves when none is left), the countdown while looked
//!   at (state 0 → 1, a new countdown), a shot (a click that hit a sprite,
//!   @ 0x00661640, while state 2 or 4: `(NewSound=301)`; the big octopus
//!   only with the crystal `CRISTAL_ENGLOUTI` in the bag: a hit off the
//!   tentacle, 6 frames of blink, or at 0 `BigTenta<k>Died` = 1, the
//!   tentacle hidden, `(NewSound=304)`, and with none left the quit,
//!   `(chapter=200)` and a scan; a small one loses a body hit, at 0 its
//!   death), the attack's blow (state 2 at the tentacle's frame 62 with its
//!   click kind set: the shake FUN_0045b470(2), `(NewSound=303)`,
//!   `EnergieJoueur` − 1, or below 1 the quit, `(chapter=201)` and a scan),
//!   the retreat (a frame below 44 in state 2 or 4: state 0), then each
//!   shown tentacle at frame 44: back to 0, or in state 1 (looked at, no
//!   attack running) the attack (state 2, click kind 5). After the
//!   records: the shake's step FUN_0045b510, and the crystal: in the bag,
//!   its entry's world set to 123 (FUN_00475420, hidden from every world's
//!   bar) and the crosshair frame 1, else 0.
//! - **The cursor** FUN_0049a9a0 (the warp draw's, @ 0x0045aa15, before
//!   the centre marker FUN_0045af10, which it replaces with the next
//!   frame's cursor choice in the mode 6): nothing while the bag bar is
//!   open; over an object the normal cursor and the hand; over a character
//!   (an attacking tentacle) with the crystal the crosshair's frame 2 (1
//!   with the small octopuses); over a move zone the hand pair emptied and
//!   the move cursor; otherwise the crosshair's frame 0 or 1.
//! - **The quit** FUN_0049aef0 (the tick's ends, outside views 1..19, the
//!   reset FUN_0049bf70): in the mode 6, the mode 0, the bank freed, the
//!   camera's reset.
//! - **The camera** (the warp module's): the reset FUN_0045b4b0 stops the
//!   shake, its field of view back to 76°, the camera's rotation **set**
//!   about the root's Y by the yaw read back (FUN_0045ad40 with radians,
//!   so the view turns to 90° − yaw and loses its pitch), the field of
//!   view 76°; the hit FUN_0045b470(n) starts a shake of n rounds when
//!   none runs; its step rolls the camera by 10°, −10°, −10°, 10° a frame
//!   per round (then 0°) about its own Z, the field of view widened by 8°
//!   a frame up to 90°, then back by 4° a frame to 76°.
//! - **The randomness**: `random(n)` = `rand() · (n + 1) / 32767`
//!   (@ 0x0049afe0) of the C runtime's one generator ([`Engine::rand`]),
//!   drawn at each entry of view 16 (one per tentacle, one per record) and
//!   at each countdown's end.
//! - **The timing**: the tick, the countdowns, the blinks and the shake
//!   count warp frames, which the original does not pace; the tentacles'
//!   frames move with the place animations' 15 Hz step.

use atlantis2_data::defnames;
use atlantis2_data::spr::SprBank;
use atlantis2_data::Folder;

use super::{CentreDraw, CursorBank, Engine, Event};
use crate::fpu::Fpu;
use crate::script::HandBank;

/// The puzzle mode of the fight (@ 0x0049a8cf).
pub const OCTOPUS_MODE: i32 = 6;

/// The module's entry, tick, cursor and quit.
pub const OCTOPUS_ENTRY: u32 = 0x0049_a8c0;
pub const OCTOPUS_TICK: u32 = 0x0049_aaa0;
pub const OCTOPUS_CURSOR: u32 = 0x0049_a9a0;
pub const OCTOPUS_QUIT: u32 = 0x0049_aef0;

/// The records (0x73 dwords from @ 0x006d82d8, 0x5c bytes each); the
/// fifth is the end mark `load_poulpes` checks (@ 0x0049b1ef).
pub const RECORDS: usize = 5;

/// The big octopus's view (@ 0x0049a90f).
const BIG_VIEW: u32 = 0x10;
/// The prefixes and bodies (@ 0x005e2d38, 0x005e2d2c, 0x005e2d24,
/// 0x005e2d18).
const BIG_PREFIX: &str = "GDP_";
const BIG_BODY: &str = "GDP_CORPS_V";
const SMALL_PREFIX: &str = "PTP_";
const SMALL_BODY: &str = "PTP_CORPS_V";
/// The crosshair bank (@ 0x005e2d40).
const CROSSHAIR: &str = "viseur.spr";
/// The crystal (@ 0x005e2dbc) and the world its bag entry is given
/// (@ 0x0049ae4b).
const CRYSTAL: &[u8] = b"CRISTAL_ENGLOUTI";
const CRYSTAL_WORLD: i32 = 0x7b;
/// The hero's energy (@ 0x005e2d6c).
const ENERGY: &str = "EnergieJoueur";

/// The commands (@ 0x005e2dac, 0x005e2d8c, 0x005e2d7c, 0x005e2d9c,
/// 0x005e2d5c, 0x005e2d4c).
pub const SHOT: &str = "(NewSound=301)";
pub const BODY_DEATH: &str = "(NewSound=302)";
pub const BLOW: &str = "(NewSound=303)";
pub const TENTACLE_DEATH: &str = "(NewSound=304)";
pub const OCTOPUS_DEAD: &str = "(chapter=200)";
pub const HERO_DEAD: &str = "(chapter=201)";

/// The tentacles' frames: the attack's first (0x2c) and its blow (0x3e);
/// the small body's death frame (0x23).
const ATTACK_FRAME: i32 = 0x2c;
const BLOW_FRAME: i32 = 0x3e;
const DEATH_FRAME: i32 = 0x23;
/// The click kind of an attacking tentacle (@ 0x0049ae20).
const ATTACK_CLICK: i32 = 5;
/// The zone type FUN_0049b370 makes a move (1).
const CLOSED_ZONE: i32 = 0x7af;

/// π as the float @ 0x005c678c, 180.0f (@ 0x005c6304), 360.0f
/// (@ 0x005c6714).
const PI: f32 = f32::from_bits(0x4049_0fdb);
const DEG_180: f64 = 180.0;
const DEG_360: f64 = 360.0;
/// The shake's roll table (@ 0x005db85c, degrees), its angle factor (the
/// float 1/180 @ 0x005c62b8), the field of view's (the double 1/180
/// @ 0x005c6328) and the reset's 76° (π · the double 76/180
/// @ 0x005c6298).
const ROLLS: [i32; 5] = [0, 10, -10, -10, 10];
const ROLL_PER_180: f32 = f32::from_bits(0x3bb6_0b61);
const FOV_PER_180: f64 = f64::from_bits(0x3f76_c16c_16c1_6c17);
const FOV_76_PER_180: f64 = f64::from_bits(0x3fdb_05b0_5b05_b05b);

/// One octopus (a record @ 0x006d82d8 + 0x5c k).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// +0x00: warp frames before it is ready (state 0 → 1).
    pub countdown: i32,
    /// +0x04: the record is used (the list ends at the first that is not).
    pub present: bool,
    /// +0x05: its sector is the one looked at.
    pub looked: bool,
    /// +0x08: its six tentacles' slots (pointers into the slots; `None`
    /// null).
    pub tentacles: [Option<usize>; 6],
    /// +0x20: the hits each tentacle has left (6).
    pub hits: [i32; 6],
    /// +0x38: the big octopus's blink of a shot tentacle.
    pub big_blink: i32,
    /// +0x3c: the body's slot (animation 1).
    pub body: Option<usize>,
    /// +0x40: the small body's death (animation 2).
    pub death: Option<usize>,
    /// +0x44: 0 idle, 1 ready, 2 attacking, 3 shot (blinking), 4 after the
    /// blink, 5 a small body dying, 6 dead.
    pub state: i32,
    /// +0x48: the attacking tentacle.
    pub attacking: i32,
    /// +0x4c: the tentacles left.
    pub left: i32,
    /// +0x50: the small body's hits left (5).
    pub body_hits: i32,
    /// +0x54: the small octopus's blink.
    pub small_blink: i32,
    /// +0x58: its sector (the last digit of its name).
    pub sector: i32,
}

/// The module's globals.
#[derive(Clone, Debug, Default)]
pub struct Octopus {
    pub records: [Record; RECORDS],
    /// @ 0x006d84a8: the big octopus (view 16).
    pub big: bool,
    /// @ 0x006d84a4: the octopuses of the view not dead.
    pub count: i32,
    /// @ 0x006d84b0: the crosshair's frame (1 with the crystal).
    pub cross: i32,
    /// @ 0x006d84ac: `viseur.spr`, loaded by the entry, freed by the quit.
    pub bank: Option<SprBank>,
    /// Whether the entry loaded the bank (a missing file leaves it null,
    /// and the next entry tries again).
    pub bank_loaded: bool,
    /// @ 0x006d829c: the sectors' table, words 0..4 (1 = loaded).
    pub sectors: [i32; 5],
    /// @ 0x006d82b4, @ 0x006d82c4: the prefix and the body's name.
    pub prefix: &'static str,
    pub body: &'static str,
}

impl Octopus {
    /// The records used, as the module's loops walk them (up to the first
    /// not present).
    pub fn live(&self) -> usize {
        self.records.iter().take_while(|r| r.present).count()
    }
}

/// The hit shake of the warp module (@ 0x006617bc, 0x006617c0,
/// 0x006611c8).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shake {
    /// @ 0x006617bc: 0 none, else the step 1..5 of the round.
    pub counter: u32,
    /// @ 0x006617c0: the rounds left.
    pub repeats: i32,
    /// @ 0x006611c8: the field of view in degrees.
    pub fov: f32,
}

#[inline(always)]
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// `random(n)` (@ 0x0049afe0): `rand() · (n + 1) / 32767`, C division.
pub fn octopus_random(rand: &mut crate::crt::Rand, n: i32) -> i32 {
    rand.rand().wrapping_mul(n.wrapping_add(1)) / 0x7fff
}

/// The tick's yaw in degrees (@ 0x0049aad3..0x0049aaf3): the read-back
/// radians times 180.0f over π (floats), stored as a float.
pub fn octopus_degrees(fpu: Fpu, yaw: f32) -> f32 {
    fpu.f32(fpu.div(fpu.mul(d(yaw), DEG_180), d(PI)))
}

/// FUN_0049b4f0: `x` brought into [−180, 180] by steps of 360, in the
/// x87 register (the loops' sums are not stored).
fn wrap180(fpu: Fpu, mut x: f64) -> f64 {
    if x > DEG_180 {
        loop {
            x = fpu.sub(x, DEG_360);
            if x <= DEG_180 {
                break;
            }
        }
    }
    if x < -DEG_180 {
        loop {
            x = fpu.add(x, DEG_360);
            if x >= -DEG_180 {
                break;
            }
        }
    }
    x
}

/// FUN_0049b420(yaw, hi, lo): the yaw strictly between `lo` and `hi`, the
/// three wrapped (the first two stored as floats, the third kept in the
/// register), across ±180 by moving `hi` up or `lo` down by 360
/// (@ 0x0049b420..0x0049b4e5).
pub fn octopus_between(fpu: Fpu, yaw: f32, hi: f32, lo: f32) -> bool {
    let y = d(fpu.f32(wrap180(fpu, d(yaw))));
    let mut hi = d(fpu.f32(wrap180(fpu, d(hi))));
    let mut lo = wrap180(fpu, d(lo));
    if y > 0.0 && lo > 0.0 && hi < 0.0 {
        hi = d(fpu.f32(fpu.add(hi, DEG_360)));
    } else if y < 0.0 && hi < 0.0 && lo > 0.0 {
        lo = fpu.sub(lo, DEG_360);
    }
    y < hi && y > lo
}

/// FUN_0049b3a0: the sector of a yaw in degrees: 4 in (−40, 40), 2 in
/// (50, 130), 3 in (140, 220), 1 in (−130, −50), else 0.
pub fn octopus_sector(fpu: Fpu, yaw: f32) -> i32 {
    const SECTORS: [(i32, f32, f32); 4] = [
        (4, 40.0, -40.0),
        (2, 130.0, 50.0),
        (3, 220.0, 140.0),
        (1, -50.0, -130.0),
    ];
    SECTORS
        .iter()
        .find(|&&(_, hi, lo)| octopus_between(fpu, yaw, hi, lo))
        .map_or(0, |&(s, _, _)| s)
}

/// `atoi` of a name's last character (FUN_0049b230): its digit, else 0.
fn last_digit(name: &str) -> i32 {
    match name.as_bytes().last() {
        Some(c @ b'0'..=b'9') => i32::from(c - b'0'),
        _ => 0,
    }
}

/// `_strnicmp(a, b, n) == 0` (FUN_005bbd60, ASCII letters folded), with a
/// shorter name compared up to its end.
fn same_start(a: &str, b: &str, n: usize) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    for i in 0..n {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if !x.eq_ignore_ascii_case(&y) {
            return false;
        }
        if x == 0 {
            return true;
        }
    }
    true
}

/// How a record's step ended: on, or the fight over (a `chapter=`).
enum Step {
    On,
    Over,
}

impl Engine {
    // --- the warp module's camera -----------------------------------------

    /// FUN_0045b4b0: the shake stopped, its field of view 76, the camera's
    /// rotation set about the root's Y by the yaw read back (FUN_0045ad40
    /// with the radians: the view turns to 90° − yaw, its pitch dropped),
    /// the field of view 76° (π · 76/180 as a float).
    pub fn camera_reset(&mut self) {
        self.shake.counter = 0;
        self.shake.fov = 76.0;
        let (yaw, _) = self.camera.angles();
        self.camera.set_yaw(yaw);
        let fpu = self.camera.fpu;
        let fov = fpu.f32(fpu.mul(d(PI), FOV_76_PER_180));
        let (w, h) = (self.renderer.width() as u32, self.renderer.height() as u32);
        self.camera.set_fov(fov, w, h);
    }

    /// FUN_0045b470(n): a shake of `n` rounds (1 for 0), its field of view
    /// 76, unless one runs.
    pub fn shake_start(&mut self, n: i32) {
        if self.shake.counter == 0 {
            self.shake.counter = 1;
            self.shake.repeats = if n == 0 { 1 } else { n };
            self.shake.fov = 76.0;
        }
    }

    /// FUN_0045b510: the shake's step, every tick of the mode 6.
    pub fn shake_step(&mut self) {
        let fpu = self.camera.fpu;
        let (w, h) = (self.renderer.width() as u32, self.renderer.height() as u32);
        if self.shake.counter == 0 {
            // The field of view back by 4° a frame down to 76.
            if d(self.shake.fov) > 76.0 {
                let f = fpu.sub(d(self.shake.fov), 4.0);
                self.shake.fov = fpu.f32(f);
                if f < 76.0 {
                    self.shake.fov = 76.0;
                }
                let fov = fpu.f32(fpu.mul(fpu.mul(d(self.shake.fov), d(PI)), FOV_PER_180));
                self.camera.set_fov(fov, w, h);
            }
            return;
        }
        if self.shake.counter > 4 {
            self.shake.repeats = self.shake.repeats.wrapping_sub(1);
            self.shake.counter = u32::from(self.shake.repeats != 0);
        }
        let deg = ROLLS[self.shake.counter as usize];
        let angle = fpu.f32(fpu.mul(fpu.mul(f64::from(deg), d(PI)), d(ROLL_PER_180)));
        self.camera.roll(angle);
        let fov = fpu.f32(fpu.mul(fpu.mul(d(self.shake.fov), d(PI)), FOV_PER_180));
        self.camera.set_fov(fov, w, h);
        let f = fpu.add(d(self.shake.fov), 8.0);
        self.shake.fov = fpu.f32(f);
        if f > 90.0 {
            self.shake.fov = 90.0;
        }
        if self.shake.counter != 0 {
            self.shake.counter += 1;
        }
    }

    // --- the module ---------------------------------------------------------

    /// A command of the module (`RunOneCommand`, FUN_00481800).
    fn octopus_command(&mut self, cmd: &'static str) {
        self.event(Event::OctopusCommand(cmd));
        self.with_script(|s, h| s.run_command(h, cmd.as_bytes()));
    }

    /// `CtrlScenarIN` (FUN_00483c90) after an end's command.
    fn octopus_scan(&mut self) {
        self.event(Event::OctopusScan);
        self.with_script(|s, h| {
            s.ctrl_scenar_in(h);
        });
    }

    /// FUN_004753d0: the crystal in the bag.
    fn crystal_in_bag(&self) -> bool {
        match self.names.id(defnames::OBJETS, CRYSTAL) {
            Ok(id) => self.bag_entries().iter().any(|b| b.id == id as i32),
            Err(_) => false,
        }
    }

    /// FUN_00475420(id, 123): the crystal's bag entry put in world 123.
    fn crystal_to_no_world(&mut self) {
        let Ok(id) = self.names.id(defnames::OBJETS, CRYSTAL) else {
            return;
        };
        let id = id as i32;
        if let Some(s) = self.con.as_mut() {
            if let Some(e) = s.bag.iter_mut().find(|e| e.id == id) {
                e.world = CRYSTAL_WORLD;
            }
        } else if let Some(e) = self.own_bag.iter_mut().find(|e| e.id == id) {
            e.world = CRYSTAL_WORLD;
        }
    }

    /// A variable by name (`Interp_ReadVar` FUN_00484ff0), as the scripts
    /// or a `vue=` line's lent interpreter hold it.
    fn octopus_var(&self, name: &str) -> i32 {
        self.script_var_or_lent(name).unwrap_or(0)
    }

    /// `Interp_WriteVar` FUN_00485010 / `AddVar` FUN_00485040: a name not
    /// in `defnames.con` is the original's fatal error (FUN_00480cd0 from
    /// the name's lookup); the game's data only reach it with
    /// `PoulpeWarp<view>` of a small octopus, which is never loaded.
    fn octopus_write(&mut self, name: &str, value: i32, add: bool) {
        let ok = self
            .with_script(|s, _| {
                if add {
                    s.add_var(name, value).is_some()
                } else {
                    s.set_var(name, value)
                }
            })
            .unwrap_or(true);
        if !ok {
            let msg = format!("DefName_GetIdName: Nom de {name} inconnu");
            log::error!("{msg}");
            self.with_script(|s, _| s.fail(msg));
        }
    }

    /// The fatal error FUN_00480cd0 (the original saves and quits): here
    /// the scripts stop.
    fn octopus_fatal(&mut self, msg: String) {
        log::error!("{msg}");
        self.with_script(|s, _| s.fail(msg));
    }

    fn slot_active(&mut self, n: Option<usize>, on: bool) {
        if let Some(s) = n.and_then(|n| self.anim.slots.get_mut(n)) {
            s.active = on;
        }
    }

    /// FUN_0049a8c0: the entry of an `AT` view numbered 1..19.
    pub fn octopus_entry(&mut self) {
        self.event(Event::PuzzleRan {
            function: OCTOPUS_ENTRY,
        });
        self.set_puzzle_mode(OCTOPUS_MODE);
        if !self.octopus.bank_loaded {
            self.octopus.bank = self
                .files
                .read(Folder::Spr, CROSSHAIR)
                .and_then(|d| SprBank::parse(&d).ok());
            self.octopus.bank_loaded = self.octopus.bank.is_some();
            if !self.octopus.bank_loaded {
                log::warn!("AtlSpr_Load: {CROSSHAIR} not found");
            }
        }
        self.camera_reset();
        self.octopus.records = Default::default();
        if self.view == BIG_VIEW {
            self.octopus.big = true;
            self.octopus.prefix = BIG_PREFIX;
            self.octopus.body = BIG_BODY;
        } else {
            self.octopus.big = false;
            self.octopus.prefix = SMALL_PREFIX;
            self.octopus.body = SMALL_BODY;
        }
        self.octopus_load();
    }

    /// FUN_0049b010 (`load_poulpes`).
    fn octopus_load(&mut self) {
        // FUN_0049b290: only digit 1, with the big octopus.
        self.octopus.sectors = [0, i32::from(self.octopus.big), 0, 0, 0];
        self.octopus.count = 0;
        let (prefix, body) = (self.octopus.prefix, self.octopus.body);
        let mut rec = 0;
        for ei in 0..self.wam.entities.len() {
            let name = self.wam.entities[ei].con_name.clone();
            // FUN_0049b2c0: the prefix, and the sector's word.
            let digit = last_digit(&name);
            let on = same_start(&name, prefix, 4)
                && usize::try_from(digit)
                    .ok()
                    .and_then(|k| self.octopus.sectors.get(k))
                    .is_some_and(|&w| w == 1);
            if !on {
                continue;
            }
            // FUN_0049b300: the body's entity.
            let is_body = same_start(&name, body, 11);
            let mut k = 0;
            for si in 0..self.wam.entities[ei].states.len() {
                let found = self
                    .anim
                    .slots
                    .iter()
                    .position(|s| s.record == Some((ei, si)));
                let slot = match found {
                    Some(n) => {
                        log::warn!("load_poulpes() ! spw_deja_loaded");
                        Some(n)
                    }
                    None => self.load_soul(ei, si),
                };
                if let Some(n) = slot.filter(|&n| n < 0x22) {
                    let s = &mut self.anim.slots[n];
                    s.active = true;
                    s.playing = true;
                }
                if !is_body {
                    let dead = self.octopus.big
                        && self.octopus_var(&format!("BigTenta{}Died", k + 1)) == 1;
                    let r = &mut self.octopus.records[rec.min(RECORDS - 1)];
                    if dead {
                        if let Some(s) = slot.and_then(|n| self.anim.slots.get_mut(n)) {
                            s.active = false;
                            s.playing = false;
                        }
                    } else {
                        r.left += 1;
                    }
                    r.present = true;
                    let frame = octopus_random(&mut self.rand, 0x27);
                    // A state whose file is on no disc has no slot: the
                    // original writes the frame and the pointer one slot
                    // before the table (never with the game's data).
                    let r = &mut self.octopus.records[rec.min(RECORDS - 1)];
                    if let Some(s) = slot.and_then(|n| self.anim.slots.get_mut(n)) {
                        s.frame = frame;
                    }
                    if k < 6 {
                        r.hits[k] = 6;
                        r.tentacles[k] = slot;
                    }
                    k += 1;
                }
            }
            if !is_body {
                let countdown = octopus_random(&mut self.rand, 0x32) + 10;
                let r = &mut self.octopus.records[rec.min(RECORDS - 1)];
                r.body_hits = 5;
                r.countdown = countdown;
                r.sector = digit;
                rec += 1;
                self.octopus.count += 1;
            }
        }
        if self.octopus.records[RECORDS - 1].present {
            self.octopus_fatal("load_poulpes(): tbl_poulpe[MAX_PLP].IsLoaded".to_string());
        }
        for k in 0..self.octopus.live().min(RECORDS) {
            let sector = self.octopus.records[k].sector;
            let body = self.octopus_body_slot(sector, 1);
            self.octopus.records[k].body = body;
            if !self.octopus.big {
                let death = self.octopus_body_slot(sector, 2);
                self.octopus.records[k].death = death;
                self.slot_active(death, false);
            }
        }
    }

    /// FUN_0049b320(record, anim): the slot of `<body><sector>`'s
    /// animation `anim` (FUN_0047dea0: the first slot of that id and
    /// animation; none is the original's fatal error and slot 0).
    fn octopus_body_slot(&mut self, sector: i32, anim: u32) -> Option<usize> {
        let name = format!("{}{}", self.octopus.body, sector);
        let id = match self.names.id(defnames::PERSONNAGES, name.as_bytes()) {
            Ok(id) => id as i32,
            Err(_) => {
                self.octopus_fatal(format!("DefName_GetIdName: Nom de {name} inconnu"));
                -1
            }
        };
        match self
            .anim
            .slots
            .iter()
            .position(|s| s.id == id && s.state.anim == anim)
        {
            Some(n) => Some(n),
            None => {
                self.octopus_fatal(format!(
                    "AtlSpw_find_index(anum) : vue={} {id} {anim}",
                    self.view
                ));
                (!self.anim.slots.is_empty()).then_some(0)
            }
        }
    }

    /// FUN_0049aef0: the quit (in the mode 6 only).
    pub fn octopus_quit(&mut self) {
        if self.puzzle_mode() == OCTOPUS_MODE {
            self.set_puzzle_mode(0);
            self.octopus.bank = None;
            self.octopus.bank_loaded = false;
            self.camera_reset();
        }
    }

    /// FUN_0049b540(record, on): the body and the six tentacles shown or
    /// hidden.
    fn octopus_show_all(&mut self, k: usize, on: bool) {
        let r = self.octopus.records[k];
        self.slot_active(r.body, on);
        for t in r.tentacles {
            self.slot_active(t, on);
        }
    }

    /// FUN_0049af90(record): when its body is shown, the body and the six
    /// tentacles hidden.
    fn octopus_hide_all(&mut self, k: usize) {
        let r = self.octopus.records[k];
        let shown = r
            .body
            .and_then(|n| self.anim.slots.get(n))
            .is_some_and(|s| s.active);
        if shown {
            self.octopus_show_all(k, false);
        }
    }

    /// The attacking tentacle's slot.
    fn attacker(&self, k: usize) -> Option<usize> {
        let r = &self.octopus.records[k];
        usize::try_from(r.attacking)
            .ok()
            .and_then(|i| r.tentacles.get(i).copied().flatten())
    }

    /// FUN_0049aaa0: the tick of the mode 6.
    pub fn octopus_tick(&mut self) {
        if !(1..=0x13).contains(&self.view) {
            self.octopus_quit();
            return;
        }
        let fpu = self.camera.fpu;
        let (yaw, _) = self.camera.angles();
        let deg = octopus_degrees(fpu, yaw);
        let live = self.octopus.live();
        let attacking = self.octopus.records[..live].iter().any(|r| r.state == 2);
        let sector = octopus_sector(fpu, deg);
        if !attacking && sector != 0 {
            for r in &mut self.octopus.records[..live] {
                r.looked = r.sector == sector;
            }
        }
        for k in 0..live {
            if let Step::Over = self.octopus_record(k, attacking) {
                return;
            }
        }
        self.shake_step();
        if self.crystal_in_bag() {
            self.crystal_to_no_world();
            self.octopus.cross = 1;
        } else {
            self.octopus.cross = 0;
        }
    }

    /// The tick's step of record `k` (@ 0x0049ab5f..0x0049ae35).
    fn octopus_record(&mut self, k: usize, attacking: bool) -> Step {
        // The blink of a shot tentacle (state 3).
        if self.octopus.records[k].state == 3 {
            let left = if !self.octopus.big {
                let r = &mut self.octopus.records[k];
                r.small_blink = r.small_blink.wrapping_sub(1);
                let b = r.small_blink;
                self.octopus_show_all(k, b & 1 == 0);
                b
            } else {
                let r = &mut self.octopus.records[k];
                r.big_blink = r.big_blink.wrapping_sub(1);
                let b = r.big_blink;
                let t = self.attacker(k);
                self.slot_active(t, b & 1 == 0);
                b
            };
            if left == 0 {
                self.octopus.records[k].state = 4;
            }
        }
        // A small body's death at its frame 35.
        let r = self.octopus.records[k];
        if r.state == 5 {
            if let Some(n) = r.death {
                if self
                    .anim
                    .slots
                    .get(n)
                    .is_some_and(|s| s.frame == DEATH_FRAME)
                {
                    // FUN_0049af60.
                    let name = format!("PoulpeWarp{}", self.view);
                    self.octopus_write(&name, 1, false);
                    self.slot_active(Some(n), false);
                    self.octopus.count = self.octopus.count.wrapping_sub(1);
                    self.octopus.records[k].state = 6;
                    if self.octopus.count == 0 {
                        self.octopus_open_zones();
                    }
                }
            }
        }
        // The countdown while looked at.
        {
            let r = &mut self.octopus.records[k];
            if r.countdown != 0 && r.state == 0 && r.looked && r.left > 0 {
                r.countdown = r.countdown.wrapping_sub(1);
                if r.countdown == 0 {
                    r.state = 1;
                    let c = octopus_random(&mut self.rand, 0x32) + 10;
                    self.octopus.records[k].countdown = c;
                }
            }
        }
        // A shot.
        let state = self.octopus.records[k].state;
        if self.sprite_hit.is_some() && (state == 2 || state == 4) {
            let big = self.octopus.big;
            if !big || self.crystal_in_bag() {
                self.octopus.records[k].state = 0;
                self.octopus_command(SHOT);
                if big {
                    if let Step::Over = self.octopus_tentacle_shot(k) {
                        return Step::Over;
                    }
                } else {
                    self.octopus_body_shot(k);
                }
            }
            // The big octopus without the crystal: nothing (@ 0x0049ac93,
            // the bag tested again).
        }
        // The attack's blow.
        let r = self.octopus.records[k];
        if r.state == 2 {
            let t = self.attacker(k);
            let blow = t
                .and_then(|n| self.anim.slots.get(n))
                .is_some_and(|s| s.frame == BLOW_FRAME && s.click != 0);
            if blow {
                self.shake_start(2);
                self.octopus_command(BLOW);
                if let Some(s) = t.and_then(|n| self.anim.slots.get_mut(n)) {
                    s.click = 0;
                }
                if self.octopus_var(ENERGY) < 1 {
                    self.octopus_quit();
                    self.octopus_command(HERO_DEAD);
                    self.octopus_scan();
                    return Step::Over;
                }
                self.octopus_write(ENERGY, -1, true);
            }
        }
        // The retreat.
        let r = self.octopus.records[k];
        if r.state == 2 || r.state == 4 {
            let t = self.attacker(k);
            if let Some(s) = t.and_then(|n| self.anim.slots.get_mut(n)) {
                if s.frame < ATTACK_FRAME {
                    self.octopus.records[k].state = 0;
                    s.click = 0;
                }
            }
        }
        // The tentacles at the attack's first frame.
        for i in 0..6 {
            let Some(n) = self.octopus.records[k].tentacles[i] else {
                continue;
            };
            let Some(s) = self.anim.slots.get(n) else {
                continue;
            };
            if !s.active || s.frame != ATTACK_FRAME {
                continue;
            }
            let r = self.octopus.records[k];
            let back = match r.state {
                0 | 4 => true,
                2 => r.attacking != i as i32,
                1 => {
                    if attacking || !r.looked {
                        true
                    } else {
                        let r = &mut self.octopus.records[k];
                        r.state = 2;
                        r.attacking = i as i32;
                        self.anim.slots[n].click = ATTACK_CLICK;
                        false
                    }
                }
                _ => false,
            };
            if back {
                self.anim.slots[n].frame = 0;
            }
        }
        Step::On
    }

    /// The big octopus's shot with the crystal (@ 0x0049ac93..0x0049acfc).
    fn octopus_tentacle_shot(&mut self, k: usize) -> Step {
        if !self.crystal_in_bag() {
            return Step::On;
        }
        let at = self.octopus.records[k].attacking;
        let Some(i) = usize::try_from(at).ok().filter(|&i| i < 6) else {
            return Step::On;
        };
        let r = &mut self.octopus.records[k];
        r.hits[i] = r.hits[i].wrapping_sub(1);
        if r.hits[i] > 0 {
            r.big_blink = 6;
            r.state = 3;
            return Step::On;
        }
        // FUN_0049af20.
        self.octopus_write(&format!("BigTenta{}Died", at + 1), 1, false);
        let t = self.octopus.records[k].tentacles[i];
        if let Some(s) = t.and_then(|n| self.anim.slots.get_mut(n)) {
            s.active = false;
            s.click = 0;
        }
        let r = &mut self.octopus.records[k];
        r.left = r.left.wrapping_sub(1);
        if r.left < 1 {
            self.octopus_hide_all(k);
            self.octopus_quit();
            self.octopus_command(OCTOPUS_DEAD);
            self.octopus_scan();
            return Step::Over;
        }
        self.octopus_command(TENTACLE_DEATH);
        Step::On
    }

    /// A small octopus's shot (@ 0x0049ad09..0x0049ad59).
    fn octopus_body_shot(&mut self, k: usize) {
        let r = &mut self.octopus.records[k];
        r.body_hits = r.body_hits.wrapping_sub(1);
        if r.body_hits == 0 {
            let death = r.death;
            self.octopus_hide_all(k);
            match death {
                None => self.octopus.records[k].state = 6,
                Some(n) => {
                    self.slot_active(Some(n), true);
                    self.octopus.records[k].state = 5;
                    self.octopus_command(BODY_DEATH);
                }
            }
        } else {
            self.octopus_show_all(k, false);
            let r = &mut self.octopus.records[k];
            r.small_blink = 8;
            r.state = 3;
        }
    }

    /// FUN_0049b370: the current view's zones of type 0x7af made moves.
    fn octopus_open_zones(&mut self) {
        let view = self.view;
        let v = self
            .wam
            .places
            .iter_mut()
            .flat_map(|p| p.views.iter_mut())
            .find(|v| v.id == view);
        if let Some(v) = v {
            for z in v.zones.iter_mut().filter(|z| z.kind == CLOSED_ZONE) {
                z.kind = 1;
            }
        }
    }

    /// FUN_0049a9a0, in the warp draw: `None` outside the mode 6 (the
    /// centre marker and the cursor choice follow), else what it draws at
    /// the centre (`w` x `h` screen).
    pub(super) fn octopus_cursor(&mut self, w: usize, h: usize) -> Option<Vec<CentreDraw>> {
        if self.puzzle_mode() != OCTOPUS_MODE {
            return None;
        }
        let mut plan = Vec::new();
        if self.bar.open {
            return Some(plan);
        }
        let (cx, cy) = (w as i32 / 2, h as i32 / 2);
        let cross = |frame| CentreDraw::Crosshair {
            frame,
            x: cx,
            y: cy,
        };
        if self.sprite_state == 1 {
            self.cursor.choose(0, 1, &mut self.cursor_flag);
            plan = self.hand_and_cursor(cx, cy);
        } else if self.sprite_state != 0 && self.octopus.cross == 1 {
            plan.push(cross(if self.octopus.big { 2 } else { 1 }));
        } else if self.centre().is_some_and(|(_, z)| z.kind == 1) {
            self.set_hand_pair_bank(0, HandBank::None);
            self.cursor.choose(1, 0, &mut self.cursor_flag);
            plan = self.hand_and_cursor(cx, cy);
        } else {
            plan.push(cross(self.octopus.cross));
        }
        Some(plan)
    }

    /// FUN_0049a9a0 alone at the screen's size: for tests and tools.
    pub fn octopus_cursor_alone(&mut self) -> Option<Vec<CentreDraw>> {
        let (w, h) = (self.renderer.width(), self.renderer.height());
        self.octopus_cursor(w, h)
    }

    /// The sprite a click hit (@ 0x00661640) and the cursor state
    /// (@ 0x006c2df4) as a warp draw leaves them: for tests and tools.
    pub fn set_sprite_under_centre(&mut self, hit: Option<usize>, state: i32) {
        self.sprite_hit = hit;
        self.sprite_state = state;
    }

    /// The zone of the current view under the centre (@ 0x006d85e0), its
    /// index in the view's list: for tests and tools.
    pub fn set_centre_zone(&mut self, zone: Option<usize>) {
        self.centre_zone = zone.map(|z| (self.view, z));
    }

    /// FUN_0047bfd0: the hand (its bank set, its frame ≥ 0), then the
    /// cursor (its bank set), at (cx, cy).
    fn hand_and_cursor(&self, cx: i32, cy: i32) -> Vec<CentreDraw> {
        let mut plan = Vec::new();
        if let Some((_, sprite)) = self.hand {
            if sprite >= 0 {
                plan.push(CentreDraw::Sprite {
                    bank: CursorBank::Objects,
                    frame: sprite,
                    x: cx,
                    y: cy,
                });
            }
        }
        if let Some(frame) = self.hand_puzzle {
            if frame >= 0 {
                plan.push(CentreDraw::Sprite {
                    bank: CursorBank::Puzzle,
                    frame,
                    x: cx,
                    y: cy,
                });
            }
        }
        if let Some(frame) = self.cursor.shown {
            plan.push(CentreDraw::Sprite {
                bank: self.cursor.bank,
                frame,
                x: cx,
                y: cy,
            });
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_digits() {
        assert!(same_start("GDP_TENTA_V1", "GDP_", 4));
        assert!(same_start("gdp_corps_v1", "GDP_CORPS_V", 11));
        assert!(!same_start("GDP_TENTA_V1", "GDP_CORPS_V", 11));
        assert_eq!(last_digit("GDP_TENTA_V1"), 1);
        assert_eq!(last_digit("PLANCTON"), 0);
    }

    #[test]
    fn sectors_at_their_bounds() {
        let f = Fpu::Chop;
        for (y, s) in [
            (0.0, 4),
            (39.9, 4),
            (40.0, 0),
            (50.1, 2),
            (135.0, 0),
            (180.0, 3),
            (-180.0, 3),
            (-90.0, 1),
            (-50.0, 0),
            (230.1, 1),
        ] {
            assert_eq!(octopus_sector(f, y), s, "{y}");
        }
    }
}
