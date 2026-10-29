//! `paleo.wam` (world 4): the wolves' fight (views 1, 2), the hunt with
//! the spear (view 4), the altar of the four hearts (views 5..11), the
//! bear and salmon ambience (views 22..51) and the tiger stone throw
//! (view 26).
//!
//! The spear and stone sprites drawn over the warp, the screen-size
//! reads (@ 0x0040ef70 / 0x0040ef90) and the frame lists of the hearts
//! (@ 0x00438a40 / 0x00438ae0) are drawing (M4). The camera lock
//! (@ 0x00440740 / 0x00440750, the byte @ 0x0077b8ac) taken during the
//! throws is not modelled (the port has no lock yet).

use super::{Hx, ZoneRec};

/// A wolf (0x178-byte records from 0x00786e00, built @ 0x004ca4f0).
#[derive(Debug, Default, Clone)]
pub struct Wolf {
    /// +4: picked for a fight (cleared only by a reset).
    pub picked: bool,
    /// +5: submitted.
    pub submitted: bool,
}

/// The paleo C globals.
#[derive(Debug, Default, Clone)]
pub struct State {
    /// The wolves' one-shot build byte (@ 0x0071e4d0).
    pub wolves_fresh: bool,
    /// The five wolves (@ 0x00786e00 + 0x178 k).
    pub wolves: [Wolf; 5],
    /// The fighters A and B (@ 0x0078755c, 0x00787560): wolf indexes.
    pub fighter_a: Option<usize>,
    pub fighter_b: Option<usize>,
    /// The wolves stopped once after the success (@ 0x00787558).
    pub wolves_stopped: bool,
    /// The success byte (@ 0x00787559, read by @ 0x004cac90).
    pub wolves_won: bool,
    /// The fight clock's start (@ 0x00786de0, seconds; 0 in the data).
    pub fight_clock: f64,

    /// The hunt's one-shot build byte (@ 0x0071e320).
    pub hunt_fresh: bool,
    /// The animals' hit bytes (record +0x18: @ 0x00786d88 stag,
    /// 0x00786d48 aurochs, 0x00786dc8 horse, 0x00786d68 ibex).
    pub hunt_hit: [bool; 4],
    /// Aiming (@ 0x0071e321, 1 in the data).
    pub hunt_aim: bool,
    /// The throw's 25 Hz counter (@ 0x00786dd0).
    pub hunt_count: i32,
    /// The spear drawn (@ 0x00786dcc).
    pub hunt_draw: bool,
    /// The 25 Hz clock's start (@ 0x00786d18).
    pub hunt_clock: f64,

    /// The altar's one-shot build byte (@ 0x0071d420).
    pub altar_fresh: bool,
    /// The hearts' positions on the altar (record +4 @ 0x00785fe4 +
    /// 0x48 k; −1 = not on the altar).
    pub altar_pos: [i32; 4],

    /// The ambience's byte cleared at view entry (@ 0x0071e6fc).
    pub ambience_fresh: bool,

    /// The tiger's one-shot byte (@ 0x0071f270), its hit byte
    /// (@ 0x0071f271) and a byte cleared at the entry (@ 0x0071f272).
    pub tiger_fresh: bool,
    pub tiger_hit: bool,
    pub tiger_f272: bool,
    /// Aiming with the stone (@ 0x0078977c), the stone flying
    /// (@ 0x0078977d), its 20 Hz counter (@ 0x00789780) and clock start
    /// (@ 0x00789750).
    pub tiger_aim: bool,
    pub tiger_throw: bool,
    pub tiger_count: i32,
    pub tiger_clock: f64,
}

impl State {
    /// The data's initial values.
    pub fn new() -> State {
        State {
            wolves_fresh: true,
            hunt_fresh: true,
            hunt_aim: true,
            altar_fresh: true,
            altar_pos: [-1; 4],
            ambience_fresh: true,
            tiger_fresh: true,
            tiger_hit: true,
            tiger_f272: true,
            ..Default::default()
        }
    }

    /// The load's one-shot bytes (@ 0x00459e70).
    pub fn after_load(&mut self) {
        self.wolves_fresh = true;
        self.hunt_fresh = true;
        self.altar_fresh = true;
        self.ambience_fresh = true;
        self.tiger_fresh = true;
    }
}

/// The wolves' table (@ 0x004ca4f0): zone, idle, fight left, fight right,
/// submitted left, submitted right, and the two indexes this wolf loses
/// to (+8, +0xc). Wolf 3's "submitted right" is `PAL_MARRON_COMBAT_D`
/// in the original (string @ 0x0071e560).
const WOLVES: [(&str, [&str; 5], [usize; 2]); 5] = [
    (
        "PAL_ZLBLANC",
        [
            "PAL_LOUP_BLANC",
            "PAL_BLANC_COMBAT_G",
            "PAL_BLANC_COMBAT_D",
            "PAL_BLANC_SOUMIS_G",
            "PAL_BLANC_SOUMIS_D",
        ],
        [3, 2],
    ),
    (
        "PAL_ZLGRIS",
        [
            "PAL_LOUP_GRIS",
            "PAL_GRIS_COMBAT_G",
            "PAL_GRIS_COMBAT_D",
            "PAL_GRIS_SOUMIS_G",
            "PAL_GRIS_SOUMIS_D",
        ],
        [0, 4],
    ),
    (
        "PAL_ZLNOIR",
        [
            "PAL_LOUP_NOIR",
            "PAL_NOIR_COMBAT_G",
            "PAL_NOIR_COMBAT_D",
            "PAL_NOIR_SOUMIS_G",
            "PAL_NOIR_SOUMIS_D",
        ],
        [1, 3],
    ),
    (
        "PAL_ZLMARRON",
        [
            "PAL_LOUP_MARRON",
            "PAL_MARRON_COMBAT_G",
            "PAL_MARRON_COMBAT_D",
            "PAL_MARRON_SOUMIS_G",
            "PAL_MARRON_COMBAT_D",
        ],
        [1, 4],
    ),
    (
        "PAL_ZLJAUNE",
        [
            "PAL_LOUP_JAUNE",
            "PAL_JAUNE_COMBAT_G",
            "PAL_JAUNE_COMBAT_D",
            "PAL_JAUNE_SOUMIS_G",
            "PAL_JAUNE_SOUMIS_D",
        ],
        [0, 2],
    ),
];

const IDLE: usize = 0;
const FIGHT_L: usize = 1;
const FIGHT_R: usize = 2;
const SUBMIT_L: usize = 3;
const SUBMIT_R: usize = 4;

/// "Show" / "hide" of the wolves (@ 0x004cacd0 / @ 0x004cad20): the first
/// slot of the object `name` (any kind) gets +0x2c := 1 / 0.
fn set_mode(h: &mut Hx, name: &str, v: i32) {
    let id = h.obj(name);
    if let Some(s) = h.g.slots.slots.iter_mut().find(|s| s.id == id) {
        s.mode = v;
    }
}

fn wolf_mode(h: &mut Hx, k: usize, which: usize, v: i32) {
    set_mode(h, WOLVES[k].1[which], v);
}

/// The fight reset (@ 0x004cad70): the fighters' fight (or submitted)
/// slots hidden, every pick and submission cleared, no fighter, every
/// idle wolf shown.
fn wolves_reset(h: &mut Hx) {
    if let Some(a) = h.g.puzzles.paleo.fighter_a {
        let w = if h.g.puzzles.paleo.wolves[a].submitted {
            SUBMIT_L
        } else {
            FIGHT_L
        };
        wolf_mode(h, a, w, 0);
    }
    if let Some(b) = h.g.puzzles.paleo.fighter_b {
        let w = if h.g.puzzles.paleo.wolves[b].submitted {
            SUBMIT_R
        } else {
            FIGHT_R
        };
        wolf_mode(h, b, w, 0);
    }
    let p = &mut h.g.puzzles.paleo;
    for w in &mut p.wolves {
        w.picked = false;
        w.submitted = false;
    }
    p.fighter_a = None;
    p.fighter_b = None;
    for k in 0..5 {
        wolf_mode(h, k, IDLE, 1);
    }
}

/// Views 1, 2 entry (@ 0x004ca4f0): the table built once (then the
/// reset); every entry the submitted wolves' idle slots hidden and the
/// fighters' fight slots shown.
pub(super) fn view_wolves(h: &mut Hx) {
    if h.g.puzzles.paleo.wolves_fresh {
        h.g.puzzles.paleo.wolves = Default::default();
        wolves_reset(h);
    }
    for k in 0..5 {
        if h.g.puzzles.paleo.wolves[k].submitted {
            wolf_mode(h, k, IDLE, 0);
        }
        if let Some(a) = h.g.puzzles.paleo.fighter_a {
            wolf_mode(h, a, IDLE, 0);
            wolf_mode(h, a, FIGHT_L, 1);
        }
        if let Some(b) = h.g.puzzles.paleo.fighter_b {
            wolf_mode(h, b, IDLE, 0);
            wolf_mode(h, b, FIGHT_R, 1);
        }
    }
    h.g.puzzles.paleo.wolves_fresh = false;
}

/// Views 1, 2 frame (@ 0x004caa40):
///
/// - view 1: the fight reset every frame; once `Flag105` = 1, every
///   idle wolf stopped once (@ 0x00787558);
/// - view 2, `Flag105` = 0, a zone click: a wolf zone whose wolf is not
///   picked → fighter A if free, else fighter B if free (the fight clock
///   restarted); `PAL_ZLCOMBAT` → the reset;
/// - every 1 / 0.25 s (double @ 0x0071e4d8) the fight settled
///   (@ 0x004cae00), then the success test (@ 0x004caed0).
#[allow(clippy::needless_range_loop)]
pub(super) fn frame_wolves(h: &mut Hx, z: &ZoneRec) {
    if h.vue() == 1 {
        wolves_reset(h);
        if h.flag(105) == 1 && !h.g.puzzles.paleo.wolves_stopped {
            for k in 0..5 {
                wolf_mode(h, k, IDLE, 0);
            }
            h.g.puzzles.paleo.wolves_stopped = true;
        }
    }
    if h.vue() == 2 && h.flag(105) == 0 && z.value != 0 {
        for k in 0..5 {
            if z.name != WOLVES[k].0 || h.g.puzzles.paleo.wolves[k].picked {
                continue;
            }
            if h.g.puzzles.paleo.fighter_a.is_none() {
                wolf_mode(h, k, IDLE, 0);
                wolf_mode(h, k, FIGHT_L, 1);
                h.g.puzzles.paleo.fighter_a = Some(k);
                h.g.puzzles.paleo.wolves[k].picked = true;
            } else if h.g.puzzles.paleo.fighter_b.is_none() {
                wolf_mode(h, k, IDLE, 0);
                wolf_mode(h, k, FIGHT_R, 1);
                h.g.puzzles.paleo.fighter_b = Some(k);
                h.g.puzzles.paleo.wolves[k].picked = true;
                h.g.puzzles.paleo.fight_clock = h.now();
            }
        }
        if z.name == "PAL_ZLCOMBAT" {
            wolves_reset(h);
        }
    }
    if h.now() - h.g.puzzles.paleo.fight_clock > 1.0 / 0.25 {
        wolves_settle(h);
        h.g.puzzles.paleo.fight_clock = h.now();
    }
    wolves_test(h);
}

/// The fight settled (@ 0x004cae00), `Flag105` = 0 and both fighters:
/// A submits unless A is one of the two wolves B loses to, else B
/// submits; the loser's submitted byte set on the first tick (its fight
/// slot hidden, its submitted slot shown), its slot freed on the next.
fn wolves_settle(h: &mut Hx) {
    let (Some(a), Some(b)) = (h.g.puzzles.paleo.fighter_a, h.g.puzzles.paleo.fighter_b) else {
        return;
    };
    if h.flag(105) != 0 {
        return;
    }
    if !WOLVES[b].2.contains(&a) {
        if !h.g.puzzles.paleo.wolves[a].submitted {
            h.g.puzzles.paleo.wolves[a].submitted = true;
            wolf_mode(h, a, FIGHT_L, 0);
            wolf_mode(h, a, SUBMIT_L, 1);
            return;
        }
        wolf_mode(h, a, SUBMIT_L, 0);
        h.g.puzzles.paleo.fighter_a = None;
        return;
    }
    if !h.g.puzzles.paleo.wolves[b].submitted {
        h.g.puzzles.paleo.wolves[b].submitted = true;
        wolf_mode(h, b, FIGHT_R, 0);
        wolf_mode(h, b, SUBMIT_R, 1);
        return;
    }
    wolf_mode(h, b, SUBMIT_R, 0);
    h.g.puzzles.paleo.fighter_b = None;
}

/// The success test (@ 0x004caed0), `Flag105` = 0: four wolves submitted
/// and a fighter slot free → the white wolf not submitted: the success
/// byte, @ 0x004cac90 (`Flag105` := 1, the reset, every idle wolf
/// stopped, `CtrlScenarIN`); else the reset.
fn wolves_test(h: &mut Hx) {
    if h.flag(105) != 0 {
        return;
    }
    let p = &h.g.puzzles.paleo;
    let n = p.wolves.iter().filter(|w| w.submitted).count();
    if (p.fighter_a.is_none() || p.fighter_b.is_none()) && n == 4 {
        if !p.wolves[0].submitted {
            h.g.puzzles.paleo.wolves_won = true;
            // @ 0x004cac90.
            if h.g.puzzles.paleo.wolves_won {
                h.set_flag(105, 1);
            }
            wolves_reset(h);
            for k in 0..5 {
                wolf_mode(h, k, IDLE, 0);
            }
            h.ctrl();
            return;
        }
        wolves_reset(h);
    }
}

/// An animal of the hunt (records built @ 0x004c98b0): name, heart, the
/// flag of its heart, yaw bounds +0 / +4 (degrees), pitch band +8, frame
/// count +0xc, slot number +0x14. In the hit test's order (stag
/// @ 0x00786d70, aurochs @ 0x00786d30, horse @ 0x00786db0, ibex
/// @ 0x00786d50).
/// An animal: name, heart, flag, then the hit window and its numbers.
type Animal = (&'static str, &'static str, usize, f32, f32, i32, i32, usize);

const ANIMALS: [Animal; 4] = [
    ("PAL_CERF", "PAL_COEUR_CERF", 608, 160.0, 132.0, 1, 0x8d, 0),
    (
        "PAL_AVROCH",
        "PAL_COEUR_AVROCH",
        609,
        -76.0,
        -104.0,
        2,
        0x83,
        1,
    ),
    (
        "PAL_CHEVAL",
        "PAL_COEUR_CHEVAL",
        610,
        -130.0,
        -158.0,
        1,
        0x79,
        2,
    ),
    (
        "PAL_BOUQUETIN",
        "PAL_COEUR_BOUQUETIN",
        611,
        111.0,
        91.0,
        2,
        0xaa,
        3,
    ),
];

/// The centre ray's angles (@ 0x004408e0): yaw, pitch (radians).
fn aim(h: &Hx) -> (f32, f32) {
    h.g.camera.angles()
}

/// View 4 entry (@ 0x004c92e0): at the first entry the hit bytes from
/// flags 608..611 (@ 0x004c98b0) and, in view 4, the spear's sprite
/// opened (`Lance*.spr`, or `propulseur*.spr` with `Flag614` = 1: M4)
/// and the animals shown or hidden by their flags (@ 0x004c9ba0); every
/// entry the 25 Hz clock restarted.
pub(super) fn view_hunt(h: &mut Hx) {
    if h.g.puzzles.paleo.hunt_fresh {
        for (k, a) in ANIMALS.iter().enumerate() {
            let hit = h.flag(a.2) == 1;
            h.g.puzzles.paleo.hunt_hit[k] = hit;
        }
        // @ 0x004c9ba0.
        if h.vue() == 4 {
            // visual: the spear's sprite bank opened (M4).
            for (k, cmd) in [
                ("(ShowObjet=PAL_CERF)", "(HideObjet=PAL_CERF)"),
                ("(ShowObjet=PAL_AVROCH)", "(HideObjet=PAL_AVROCH)"),
                ("(ShowObjet=PAL_CHEVAL)", "(HideObjet=PAL_CHEVAL)"),
                ("(ShowObjet=PAL_BOUQUETIN)", "(HideObjet=PAL_BOUQUETIN)"),
            ]
            .into_iter()
            .enumerate()
            {
                let hit = h.flag(ANIMALS[k].2) == 1;
                h.g.puzzles.paleo.hunt_hit[k] = hit;
                h.run(if hit { cmd.1 } else { cmd.0 });
            }
        }
    }
    h.g.puzzles.paleo.hunt_clock = h.now();
    h.g.puzzles.paleo.hunt_fresh = false;
}

/// View 4 frame (@ 0x004c9350): the spear taken from the hand (`Flag620`
/// := 1), a throw on `PAL_ZONE_LANCE1..5`, the 25 Hz flight (the hit
/// test at tick 12 with the propulsor, `Flag614` = 1; the spear back in
/// the bag at 25), and the end (@ 0x004c9860) when the four animals are
/// hit, or on any right click in view 4.
pub(super) fn frame_hunt(h: &mut Hx, z: &ZoneRec) {
    if !h.g.puzzles.paleo.hunt_hit.iter().all(|&b| b) && hunt_step(h, z) {
        return;
    }
    // The tail (@ 0x004c96d4).
    if h.g.puzzles.paleo.hunt_hit.iter().all(|&b| b) && h.flag(91) == 0 {
        hunt_end(h);
    }
    // The right button's byte (@ 0x007592b9): see the report.
    if h.g.right_clicked {
        hunt_end(h);
    }
}

/// @ 0x004c9384..0x004c96d4. True at a hit: the function returns there,
/// before the tail.
#[allow(clippy::needless_range_loop)]
fn hunt_step(h: &mut Hx, z: &ZoneRec) -> bool {
    if h.hand() == h.obj("PAL_LANCE") {
        // A direct store, not `Hand_Clear`.
        let k = h.s.keys.objetenmain;
        h.s.vars[k] = 0;
        h.set_flag(620, 1);
        h.run("(sacados=NOT_PAL_LANCE)");
        let p = &mut h.g.puzzles.paleo;
        p.hunt_aim = true;
        p.hunt_count = 0;
        p.hunt_draw = true;
    }
    // visual: with the draw byte the spear drawn (@ 0x004c9720, M4).
    if z.value != 0 && h.g.puzzles.paleo.hunt_aim {
        if (1..=5).any(|n| z.name == format!("PAL_ZONE_LANCE{n}")) {
            h.run("(PlaySound=201)");
            let k = h.s.keys.objetenmain;
            h.s.vars[k] = 0;
            h.g.puzzles.paleo.hunt_aim = false;
            // The camera locked (@ 0x00440740).
        }
        return false;
    }
    if z.value == 0 && h.g.puzzles.paleo.hunt_aim {
        return false;
    }
    // The 25 Hz clock (double @ 0x0071e328).
    if h.now() - h.g.puzzles.paleo.hunt_clock <= 1.0 / 25.0 {
        return false;
    }
    h.g.puzzles.paleo.hunt_clock = h.now();
    if h.g.puzzles.paleo.hunt_count < 25 {
        h.g.puzzles.paleo.hunt_count += 1;
    } else {
        // The camera unlocked (@ 0x00440750); the spear not drawn.
        h.g.puzzles.paleo.hunt_draw = false;
    }
    if h.g.puzzles.paleo.hunt_count == 12 {
        if h.flag(614) != 1 {
            return false;
        }
        for k in 0..4 {
            if hunt_hit_test(h, k) {
                let cmd = format!("(SacADos={})", ANIMALS[k].1);
                h.run(&cmd);
                h.set_flag(ANIMALS[k].2, 1);
                return true;
            }
        }
    }
    // Every 25 Hz tick while the counter stays at 25.
    if h.g.puzzles.paleo.hunt_count == 25 {
        h.run("(sacados=PAL_LANCE)");
        h.set_flag(620, 0);
    }
    false
}

/// The hit test of animal k (@ 0x004c99d0): not yet hit; the pitch's
/// band (1 in (1°, 10°), 2 in (−23°, −12°)) the animal's; with o = 360 /
/// frames · the frame (+0x08) of slot `+0x14`, hit iff hi − o + 180 <
/// yaw + 180 < lo − o + 180 + 5 (each bound + 360 below −180) →
/// `(HideObjet=PAL_<animal>)`, the hit byte.
fn hunt_hit_test(h: &mut Hx, k: usize) -> bool {
    if h.g.puzzles.paleo.hunt_hit[k] {
        return false;
    }
    let (name, _, _, lo, hi, band, frames, slot) = ANIMALS[k];
    let (y, p) = aim(h);
    let yaw = y * 57.29747 + 180.0;
    let pitch = p * 57.29747;
    let b = if pitch > 1.0 && pitch < 10.0 {
        1
    } else if pitch > -23.0 && pitch < -12.0 {
        2
    } else {
        return false;
    };
    if b != band {
        return false;
    }
    // Slot number +0x14 indexes the fixed array (slots 0..3 of view 4).
    let f = h.g.slots.slots.get(slot).map_or(0, |s| s.frame);
    let o = (360.0 / frames as f32) * f as f32;
    let mut a = lo - o;
    if a < -180.0 {
        a += 360.0;
    }
    let mut c = hi - o;
    if c < -180.0 {
        c += 360.0;
    }
    if c + 180.0 < yaw && yaw < a + 180.0 + 5.0 {
        let cmd = format!("(HideObjet={name})");
        h.run(&cmd);
        h.g.puzzles.paleo.hunt_hit[k] = true;
        return true;
    }
    false
}

/// The hunt's end (@ 0x004c9860): the spear's sprite closed, the right
/// button's byte cleared, the camera unlocked, `(sacados=NOT_PAL_LANCE)`,
/// `Flag620` := 0, `Flag91` := 1, `CtrlScenarIN`.
pub(super) fn hunt_end(h: &mut Hx) {
    // visual: the sprite bank @ 0x00786d90 closed (M4).
    h.g.right_clicked = false;
    h.run("(sacados=NOT_PAL_LANCE)");
    h.set_flag(620, 0);
    h.set_flag(91, 1);
    h.ctrl();
}

/// The hearts (records @ 0x00785fe0 + 0x48 k): name, bag sprite (+8).
const HEARTS: [(&str, u8); 4] = [
    ("PAL_COEUR_CERF", 2),
    ("PAL_COEUR_AVROCH", 3),
    ("PAL_COEUR_CHEVAL", 4),
    ("PAL_COEUR_BOUQUETIN", 5),
];

/// Views 5..11 entry (@ 0x004c42c0): every entry the positions from
/// flags 616..619 − 1 (visual: the hearts' frame lists rebuilt, M4).
pub(super) fn view_altar(h: &mut Hx) {
    for k in 0..4 {
        let v = h.flag(616 + k) - 1;
        h.g.puzzles.paleo.altar_pos[k] = v;
    }
    // visual: frame 0 removed from each heart's list, its position's
    // frame added (@ 0x00438ba0 / 0x00438ae0 / 0x00438a40, M4).
    h.g.puzzles.paleo.altar_fresh = false;
}

/// Views 5..8 frame (@ 0x004c4470), `Flag70` = 0 and a zone click:
/// `PAL_ZFEU` 0, `PAL_ZAIR` 1, `PAL_ZEAU` 2, `PAL_ZTERRE` 3 → @ 0x004c4600;
/// then (any zone) flags 616..619 := positions + 1 and the success test
/// 2, 3, 1, 4 → `Flag70` := flag 618, `CtrlScenarIN`.
pub(super) fn frame_altar(h: &mut Hx, z: &ZoneRec) {
    if h.flag(70) != 0 || z.value == 0 {
        return;
    }
    let pos = ["PAL_ZFEU", "PAL_ZAIR", "PAL_ZEAU", "PAL_ZTERRE"]
        .iter()
        .position(|n| z.name == *n);
    if let Some(p) = pos {
        altar_place(h, p as i32);
    }
    for k in 0..4 {
        let v = h.g.puzzles.paleo.altar_pos[k] + 1;
        h.set_flag(616 + k, v);
    }
    if h.flag(616) == 2 && h.flag(617) == 3 && h.flag(618) == 1 && h.flag(619) == 4 {
        let v = h.flag(618);
        h.set_flag(70, v);
        h.ctrl();
    }
}

/// A position clicked (@ 0x004c4600): an empty hand takes back the
/// heart(s) there (bag add with its sprite, in hand, position −1); a
/// heart in hand: `(PlaySound=200)` (any object), then the heart of that
/// name placed, `(ObjetEnMain=0)`, out of the bag.
fn altar_place(h: &mut Hx, position: i32) {
    if h.hand() == 0 {
        for (k, (name, sprite)) in HEARTS.iter().enumerate() {
            if h.g.puzzles.paleo.altar_pos[k] != position {
                continue;
            }
            // visual: the frame removed from its list (@ 0x00438ae0).
            let id = h.obj(name);
            h.s.bag_add(h.g, id, *sprite);
            h.s.hold(id);
            h.g.puzzles.paleo.altar_pos[k] = -1;
        }
        return;
    }
    let hand = h.hand();
    h.run("(PlaySound=200)");
    for (k, (name, _)) in HEARTS.iter().enumerate() {
        if h.obj(name) != hand {
            continue;
        }
        h.g.puzzles.paleo.altar_pos[k] = position;
        // visual: the frame added to its list (@ 0x00438a40).
        h.run("(ObjetEnMain=0)");
        let id = h.obj(name);
        h.s.bag_remove(id);
    }
}

/// Views 22, 25..29, 31, 32, 35..44, 48..51 entry (@ 0x004cb240): the byte
/// @ 0x0071e6fc cleared.
pub(super) fn view_common(h: &mut Hx) {
    h.g.puzzles.paleo.ambience_fresh = false;
}

/// The salmon views of @ 0x004cb250 (25..29, 31, 35..39, 44, 48, 51).
fn salmon_view(v: i32) -> bool {
    matches!(v, 25..=29 | 31 | 35..=39 | 44 | 48 | 51)
}

/// The same views' frame (@ 0x004cb250): the sounds' draw (@ 0x004cbc70),
/// then the bear's slot (`PAL_OURS`, its second state) at frame 0 of a
/// running play restarts the bear's spray (`PAL_GERBEOURS`, or
/// `PAL_GERBEOURS2` in views 40..43) and, half the time, the salmon's
/// leap (`PAL_FLYINGSALMON`, or `..2` in 40..43); an ended spray or leap
/// is stopped.
pub(super) fn frame_common(h: &mut Hx) {
    // @ 0x004cbc70.
    let r = h.rand() & 0x3ff;
    if r < 0x3e9 {
        if r < 8 {
            h.run("(PlaySound=202)");
        } else if r > 8 && r < 0x10 {
            h.run("(PlaySound=203)");
        }
    }
    let bear = h.perso("PAL_OURS");
    let gerbe = h.obj("PAL_GERBEOURS");
    let salmon = h.obj("PAL_FLYINGSALMON");
    let gerbe2 = h.obj("PAL_GERBEOURS2");
    let salmon2 = h.obj("PAL_FLYINGSALMON2");
    let playing = h.slot_of(bear, 1).is_some_and(|k| {
        let s = &h.g.slots.slots[k];
        s.mode == 1 && s.shown && s.frame == 0
    });
    if playing {
        let v = h.vue();
        restart(
            h,
            if (40..=43).contains(&v) {
                gerbe2
            } else {
                gerbe
            },
        );
        let v = h.vue();
        if salmon_view(v) {
            maybe_restart(h, salmon);
        } else if (40..=43).contains(&v) {
            maybe_restart(h, salmon2);
        }
    }
    let v = h.vue();
    stop_ended(
        h,
        if (40..=43).contains(&v) {
            gerbe2
        } else {
            gerbe
        },
    );
    let v = h.vue();
    if salmon_view(v) {
        stop_ended(h, salmon);
    } else if (40..=43).contains(&v) {
        stop_ended(h, salmon2);
    }
}

/// A play restarted (@ 0x004cbb40) unless running (+0x2c = 1): frame 0,
/// +0x2c := 1, +0x18 := 0, the clock restarted.
fn restart(h: &mut Hx, id: i32) {
    let now = h.now();
    // The original indexes slot −1 when the entity is not in the view;
    // here nothing.
    let Some(k) = h.slot_of(id, 0) else { return };
    let s = &mut h.g.slots.slots[k];
    if s.mode != 1 {
        s.frame = 0;
        s.mode = 1;
        s.click = 0;
        s.start = now;
    }
}

/// @ 0x004cb9e0: not running → `(rand() & 0x3ff) / 10` < 51 → restarted.
fn maybe_restart(h: &mut Hx, id: i32) {
    let Some(k) = h.slot_of(id, 0) else { return };
    if h.g.slots.slots[k].mode != 1 {
        let u = (h.rand() & 0x3ff) / 10;
        if u < 0x33 {
            restart(h, id);
        }
    }
}

/// A running play that ended: +0x2c := 0, its end byte cleared.
fn stop_ended(h: &mut Hx, id: i32) {
    let Some(k) = h.slot_of(id, 0) else { return };
    let s = &mut h.g.slots.slots[k];
    if s.mode == 1 && s.ended {
        s.mode = 0;
        s.ended = false;
    }
}

/// View 26 entry (@ 0x004d1130): the byte @ 0x0071f272 cleared; at the
/// first entry with `Flag111` = 0 the hit byte cleared; no aim, no
/// throw; the stone's sprite opened (`CaillouTigre*.spr`, @ 0x004d15d0:
/// M4); the 20 Hz clock restarted.
pub(super) fn view_tiger(h: &mut Hx) {
    let now = h.now();
    let flag111 = h.flag(111);
    let p = &mut h.g.puzzles.paleo;
    p.tiger_f272 = false;
    if p.tiger_fresh && flag111 == 0 {
        p.tiger_hit = false;
    }
    p.tiger_aim = false;
    p.tiger_throw = false;
    p.tiger_clock = now;
    p.tiger_fresh = false;
}

/// View 26 frame (@ 0x004d11c0), only while `Flag111` = 0: the stone in
/// hand arms the aim; a click on `PAL_ZONE_LANCER` while aiming clears
/// the hand and throws (input frozen); 21 ticks of 20 Hz later the hit
/// test (@ 0x004d1380): hit → `Flag111` := 1 and @ 0x004d15a0 (the
/// right button's byte cleared, `CtrlScenarIN`); miss → `Flag111` := 2,
/// the right button's byte cleared, `CtrlScenarIN`.
pub(super) fn frame_tiger(h: &mut Hx, z: &ZoneRec) {
    if h.flag(111) != 0 {
        return;
    }
    if h.hand() == h.obj("PAL_CAILLOU") {
        if !h.g.puzzles.paleo.tiger_aim {
            h.g.puzzles.paleo.tiger_aim = true;
            h.g.puzzles.paleo.tiger_count = 0;
        }
        // visual: the stone drawn (@ 0x004d1470, M4).
    } else {
        h.g.puzzles.paleo.tiger_aim = false;
        // visual: while thrown, the stone drawn (@ 0x004d1470, M4).
    }
    if z.value != 0 && h.g.puzzles.paleo.tiger_aim && z.name == "PAL_ZONE_LANCER" {
        h.hand_clear();
        h.g.puzzles.paleo.tiger_aim = false;
        h.g.puzzles.paleo.tiger_throw = true;
        // The camera locked (@ 0x00440740).
        h.g.puzzles.freeze = true;
    }
    // The 20 Hz clock (double @ 0x0071f278).
    if !h.g.puzzles.paleo.tiger_throw || h.now() - h.g.puzzles.paleo.tiger_clock <= 1.0 / 20.0 {
        return;
    }
    h.g.puzzles.paleo.tiger_clock = h.now();
    if h.g.puzzles.paleo.tiger_count < 0x14 {
        h.g.puzzles.paleo.tiger_count += 1;
        return;
    }
    // The camera unlocked (@ 0x00440750).
    h.g.puzzles.freeze = false;
    h.g.puzzles.paleo.tiger_throw = false;
    h.g.puzzles.paleo.tiger_count = 0;
    if tiger_hit_test(h) {
        h.g.puzzles.paleo.tiger_hit = true;
        h.set_flag(111, 1);
        // @ 0x004d15a0. visual: the sprite bank @ 0x00789768 closed (M4).
        h.g.right_clicked = false;
        h.ctrl();
        return;
    }
    h.set_flag(111, 2);
    h.g.right_clicked = false;
    h.ctrl();
}

/// The stone's hit test (@ 0x004d1380): pitch in (−6°, −1°), yaw + 180
/// in (230°, 240°), and the frame of `PAL_TIGRE`'s first slot in
/// 0x56..0x86.
fn tiger_hit_test(h: &mut Hx) -> bool {
    let tiger = h.perso("PAL_TIGRE");
    let (y, p) = aim(h);
    let yaw = y * 57.29747 + 180.0;
    let pitch = p * 57.29747;
    if !(pitch > -6.0 && pitch < -1.0 && yaw > 230.0 && yaw < 240.0) {
        return false;
    }
    let f = h.slot_of(tiger, 0).map_or(-1, |k| h.g.slots.slots[k].frame);
    f > 0x55 && f < 0x87
}

/// The wolves' success (@ 0x004cac90) as a test walk injects it: the
/// success byte read, `Flag105` := 1, the reset, every wolf idle,
/// `CtrlScenarIN`.
pub(super) fn wolves_success(h: &mut Hx) {
    if h.g.puzzles.paleo.wolves_won {
        h.set_flag(105, 1);
    }
    wolves_reset(h);
    for k in 0..5 {
        wolf_mode(h, k, IDLE, 0);
    }
    h.ctrl();
}

/// The tiger stone's hit tail (@ 0x004d15a0): the right button's byte
/// cleared, `CtrlScenarIN`.
pub(super) fn tiger_success(h: &mut Hx) {
    h.g.right_clicked = false;
    h.ctrl();
}
