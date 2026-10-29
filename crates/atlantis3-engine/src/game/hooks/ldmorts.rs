//! The Land of the Dead (`ldmorts.wam`, world 2): the feather chest, the
//! scales and the pedestal (views 2..4, 20), the Isis tiles (views 7..10),
//! the fisherman's scale (view 11) and the Ankh coffer's lid (views
//! 27..30).
//!
//! The lid, the feathers, the scale's beam, the tiles are mode-0 place
//! sprites: the C code chooses what they show through their frame lists
//! (@ 0x00438a40 add a frame, @ 0x00438ae0 remove one, @ 0x00438ba0 test
//! one; [`super::super::frame_tab`]). Every board here is checked against
//! the original under emulation (`tests/board_worlds12.rs`).

use super::{Hx, ZoneRec};
use atlantis2_engine::script::Host;

/// A feather record (@ 0x007875e8, 10 × 0x24 bytes: +0 the name
/// `LDM_PLUMEn`, +0x14 the weight, +0x18 the sprite n + 27, +0x1c the
/// object, +0x20 its frame on a pan).
#[derive(Debug, Default, Clone, Copy)]
pub struct Feather {
    pub id: i32,
    pub weight: i32,
    pub sprite: i32,
    pub pan_frame: i32,
}

/// An Isis tile record (0x18 bytes: +0 the object, +4 the frame = its
/// position, +0xc its own index, +0x10 the kind 4 / 5, +0x14 index + 0 /
/// 12).
#[derive(Debug, Default, Clone, Copy)]
pub struct Tile {
    pub id: i32,
    pub frame: i32,
    pub own: i32,
    pub kind: i32,
    pub index: i32,
}

/// An object animation the C code plays itself (@ 0x004c1c70 /
/// 0x004c1cd0 on a record: +0 the object, +4 its frame count, +8 the
/// frame reached, +0xc the play running).
#[derive(Debug, Default, Clone)]
pub struct AnimRec {
    pub id: i32,
    pub count: i32,
    pub reached: i32,
    pub running: bool,
}

/// The Land of the Dead's globals.
#[derive(Debug, Default, Clone)]
pub struct State {
    /// The fisherman's scale set up (@ 0x0071d3a8, 1 until the first
    /// entry into view 11; set again by the load), `LDM_BALANCEPE`'s id
    /// (@ 0x00785f70) and frame count (@ 0x00785f74).
    pub fisherman_fresh: bool,
    pub fisherman_id: i32,
    pub fisherman_count: i32,
    /// The chest's set-up (@ 0x0071e778, 1 until the first entry into
    /// views 2..4 / 20; set again by the load).
    pub chest_fresh: bool,
    /// A byte the Isis entry clears (@ 0x0071e264; set by the load, read
    /// by no code).
    pub b71e264: bool,
    /// The ten feathers (@ 0x007875e8).
    pub feathers: [Feather; 10],
    /// The chest lid's object and frame (@ 0x00787750, 0x00787754: 0
    /// shut, 0xe open), the feathers' (@ 0x00787758, 0x0078775c), the
    /// scale's beam (@ 0x00787768, 0x0078776c: 0 level or right pan down,
    /// 1 left pan down).
    pub lid_id: i32,
    pub plumes_id: i32,
    pub scale_id: i32,
    pub lid_frame: i32,
    pub plumes_frame: i32,
    pub scale_frame: i32,
    /// The feathers on the left and right pans (@ 0x00787790, 0x00787794)
    /// and their weights (@ 0x0071e77c, 0x0071e780; −1 empty).
    pub pans: [Option<usize>; 2],
    pub pan_weight: [i32; 2],
    /// `DIAL_TOUTES_PLUMES` offered (@ 0x0078778c), `DIAL_PLUME_CHOISIE`
    /// offered (@ 0x0078778d).
    pub all_feathers_offered: bool,
    pub one_feather_offered: bool,
    /// The beam's sound played for level (@ 0x0071e779, 1 at start-up)
    /// and for tilted (@ 0x0078778e).
    pub level_sounded: bool,
    pub tilt_sounded: bool,
    /// The Isis tiles: `LDM_ISISKI0..11` (@ 0x00786bd8, kind 4) and
    /// `LDM_ISISKS0..11` (@ 0x00786a98, kind 5).
    pub ki: [Tile; 12],
    pub ks: [Tile; 12],
    /// The tile in hand (@ 0x00786d10): (kind 4 / 5, index).
    pub held: Option<(i32, usize)>,
    /// The zone clicked (@ 0x00786cf8) and the one the held tile was
    /// taken from (@ 0x00786a68): `Aucune_Zone_Cliquée` from the static
    /// constructors (@ 0x004c7e50, 0x004c7e90) until a click.
    pub clicked: String,
    pub picked: String,
    /// The held KS tile's sample (@ 0x00786a80, `Rien` when none).
    pub sample: String,
    /// The held KS tile's text shown (@ 0x00786d0c), the tiles shown
    /// (@ 0x00786d0d).
    pub tile_text: bool,
    pub tiles_shown: bool,
    /// The Ankh coffer's lid (@ 0x00785f18, `ANK_COUVERCLE`, 5 frames).
    pub lid: AnimRec,
}

/// The zone names' initial value (@ 0x0071e268).
const NO_ZONE: &str = "Aucune_Zone_Cliquée";
/// The sample name's "none" (@ 0x00717094).
const NO_SAMPLE: &str = "Rien";

impl State {
    pub fn new() -> State {
        State {
            fisherman_fresh: true,
            chest_fresh: true,
            b71e264: true,
            pan_weight: [-1, -1],
            level_sounded: true,
            clicked: NO_ZONE.to_string(),
            picked: NO_ZONE.to_string(),
            sample: NO_SAMPLE.to_string(),
            ..Default::default()
        }
    }

    /// @ 0x00459e70.
    pub fn after_load(&mut self) {
        self.fisherman_fresh = true;
        self.chest_fresh = true;
        self.b71e264 = true;
    }
}

// ---- the C-played object animations -----------------------------------

/// @ 0x004c1c70: the record set to the object `name` and `count` frames,
/// the frame reached 0 (the running byte kept).
pub(super) fn anim_start(h: &mut Hx, rec: &mut AnimRec, name: &str, count: i32) {
    rec.id = h.obj(name);
    rec.count = count;
    for f in 0..count {
        h.subf_if_listed(rec.id, f);
    }
    rec.reached = 0;
}

/// @ 0x004c1cd0, every slot of the object (any kind): one not yet playing
/// by its clock (+0x2c ≠ 1) is started at frame 0 now (running := 1);
/// one playing whose play ended (+0x46d) is stopped (+0x2c := 0) and the
/// record's last frame reached (running := 0). True when the last frame
/// is reached.
pub(super) fn anim_step(h: &mut Hx, rec: &mut AnimRec) -> bool {
    for f in 0..rec.count {
        h.subf_if_listed(rec.id, f);
    }
    let now = h.now();
    for k in 0..h.g.slots.slots.len() {
        let s = &mut h.g.slots.slots[k];
        if s.id != rec.id {
            continue;
        }
        if s.mode == 1 {
            if s.ended {
                s.mode = 0;
                rec.reached = rec.count;
                rec.running = false;
                // @ 0x004c1df0: the last frame listed (once).
                h.addf_if_absent(rec.id, rec.reached);
            }
        } else {
            s.frame = 0;
            s.mode = 1;
            s.restart_clock(now);
            rec.running = true;
        }
    }
    rec.reached == rec.count
}

fn in_ldmorts(h: &Hx) -> bool {
    h.g.place.name.eq_ignore_ascii_case("LDMorts.wam")
}

// ---- the Ankh coffer (views 27..30) -----------------------------------

/// The coffer's view entry (@ 0x004c1650, after every view hook and after
/// a load's rebuild @ 0x004c1e20): in `LDMorts.wam` views 27..30 the lid
/// record rebuilt, open (frame 5 reached) when flag 501 = 1.
///
/// The byte @ 0x00785f2f that would keep the record is written only by
/// @ 0x004c1e20 (to 0), so the record is rebuilt at every entry
/// (@ 0x0071d170 is set and cleared on the way).
pub(super) fn coffer_entry(h: &mut Hx) {
    if !in_ldmorts(h) || !(27..=30).contains(&h.vue()) {
        return;
    }
    let mut lid = std::mem::take(&mut h.g.puzzles.ldmorts.lid);
    anim_start(h, &mut lid, "ANK_COUVERCLE", 5);
    if h.flag(501) == 1 {
        lid.reached = lid.count;
    }
    if !lid.running {
        // @ 0x004c1df0.
        h.addf_if_absent(lid.id, lid.reached);
    }
    h.g.puzzles.ldmorts.lid = lid;
}

/// The coffer's frame (@ 0x004c1950, called by the main loop every frame,
/// in any mode, @ 0x004d5df9): in `LDMorts.wam` view 30 a click on the
/// zone `LDM_COFFRE_ANKH` (the record's name, whatever its value) with
/// the lid shut → `(PlaySound=111)`, the lid's play started and the
/// record's name := `NONE`; the play stepped while it runs; the lid open
/// → slot 2 takeable (+0x18 := 1, @ 0x00759e04) and flag 501 := 1, once.
pub(in crate::game) fn coffer_frame(h: &mut Hx) {
    if !in_ldmorts(h) || h.vue() != 30 {
        return;
    }
    let mut lid = std::mem::take(&mut h.g.puzzles.ldmorts.lid);
    let clicked =
        h.g.puzzles
            .zone
            .as_ref()
            .is_some_and(|z| z.2 == "LDM_COFFRE_ANKH");
    if clicked && lid.reached == 0 {
        h.run("(PlaySound=111)");
        anim_step(h, &mut lid);
        if let Some(z) = h.g.puzzles.zone.as_mut() {
            z.2 = "NONE".to_string();
        }
    }
    if lid.running {
        anim_step(h, &mut lid);
    }
    if lid.reached == lid.count && h.slot(2).map(|s| s.click) != Some(1) {
        if let Some(s) = h.slot(2) {
            s.click = 1;
        }
        h.set_flag(501, 1);
    }
    h.g.puzzles.ldmorts.lid = lid;
}

// ---- the fisherman's scale (view 11) ----------------------------------

/// View 11's entry (@ 0x004c39b0): once per session (and after a load),
/// the 210 frames of `LDM_BALANCEPE` taken out of its frame list.
pub(super) fn view_fisherman(h: &mut Hx) {
    if h.g.puzzles.ldmorts.fisherman_fresh {
        let id = h.obj("LDM_BALANCEPE");
        h.g.puzzles.ldmorts.fisherman_id = id;
        for f in 0..0xd2 {
            h.subf_if_listed(id, f);
        }
        h.g.puzzles.ldmorts.fisherman_count = 0xd2;
    }
    h.g.puzzles.ldmorts.fisherman_fresh = false;
}

/// View 11's frame (@ 0x004c3a20; the zone record is not read): with
/// flag 87 (the talisman given) the scale's animation (slots 3, 4 and 6
/// played from frame 0, slot 2 parked, the input frozen); at its end
/// (slot 3's +0x46d) `LDM_BALANCEPE` hidden, `LDM_BALANCE_PR` and
/// `LDM_POISSON2` shown, flag 561 := 1. Then, while flag 69 = 1 (the
/// scale taken), `CtrlScenarIN` every frame.
pub(super) fn frame_fisherman(h: &mut Hx, _z: &ZoneRec) {
    if h.vue() == 11 {
        let now = h.now();
        let mode3 = h.slot(3).map_or(0, |s| s.mode);
        let mut test_end = mode3 == 1;
        if h.flag(87) == 1 {
            if h.flag(69) == 0 {
                if let Some(s) = h.slot(2) {
                    s.click = 0;
                }
            }
            if h.flag(561) == 0 && mode3 != 1 {
                h.g.puzzles.freeze = true;
                if let Some(s) = h.slot(2) {
                    s.mode = 0;
                }
                for k in [3, 4, 6] {
                    if let Some(s) = h.slot(k) {
                        s.frame = 0;
                        s.mode = 1;
                        s.click = 0;
                        s.start = now;
                    }
                }
                test_end = true;
            }
        }
        if test_end && h.slot(3).is_some_and(|s| s.ended) {
            h.g.puzzles.freeze = false;
            if let Some(s) = h.slot(3) {
                s.mode = 0;
                s.click = 1;
            }
            h.run("(Hideobjet=LDM_BALANCEPE)");
            h.run("(showobjet=LDM_BALANCE_PR)");
            if let Some(s) = h.slot(2) {
                s.mode = 1;
                s.click = 0;
                s.start = now;
            }
            for k in [6, 4] {
                if let Some(s) = h.slot(k) {
                    s.mode = 0;
                }
            }
            h.run("(showobjet=LDM_POISSON2)");
            h.set_flag(561, 1);
        }
    }
    if h.flag(69) == 1 {
        h.ctrl();
    }
}

// ---- the feathers: chest, scales, pedestal (views 2..4, 20) ------------

/// The feathers' set-up (@ 0x004cc5e0 → @ 0x004cc6c0): the lid's, the
/// feathers' and the beam's objects, their frames 0..13 (0..1 for the
/// beam) taken out of their lists when listed, their frames 0; the
/// feather records rebuilt; with flag 85 (the feathers taken) the weights
/// from flags 545..554, else a random permutation of 0..9 (`rand() % 10`
/// until free) written to the records and to flags 545..554.
fn feathers_setup(h: &mut Hx) {
    h.g.puzzles.ldmorts.all_feathers_offered = false;
    let (lid, plumes, scale) = (
        h.obj("LDM_COUVERCLE"),
        h.obj("LDM_PLUMES"),
        h.obj("LDM_BALANCE"),
    );
    let p = &mut h.g.puzzles.ldmorts;
    p.lid_id = lid;
    p.plumes_id = plumes;
    p.scale_id = scale;
    for f in 0..0xe {
        h.subf_if_listed(lid, f);
        h.subf_if_listed(plumes, f);
    }
    for f in 0..2 {
        h.subf_if_listed(scale, f);
    }
    let p = &mut h.g.puzzles.ldmorts;
    p.lid_frame = 0;
    p.plumes_frame = 0;
    p.scale_frame = 0;
    for n in 0..10 {
        let id = h.obj(&format!("LDM_PLUME{n}"));
        let f = &mut h.g.puzzles.ldmorts.feathers[n];
        f.id = id;
        f.pan_frame = 0;
        f.sprite = n as i32 + 27;
    }
    if h.flag(85) != 0 {
        for n in 0..10 {
            let w = h.flag(545 + n);
            h.g.puzzles.ldmorts.feathers[n].weight = w;
        }
        return;
    }
    let mut taken = [false; 10];
    for n in 0..10 {
        let mut w = h.rand() % 10;
        while taken[w as usize] {
            w = h.rand() % 10;
        }
        taken[w as usize] = true;
        h.g.puzzles.ldmorts.feathers[n].weight = w;
        h.set_flag(545 + n, w);
        log::warn!("flagptr[{}] poid plume {}", n + 0x221, w);
    }
}

/// The feather whose weight is `w` (the last one, as the C loop).
fn feather_of_weight(h: &Hx, w: i32) -> Option<usize> {
    (0..10).rfind(|&n| h.g.puzzles.ldmorts.feathers[n].weight == w)
}

/// Views 2, 3, 4, 20's entry (@ 0x004cbfe0): the feathers set up once,
/// the lid's old frame taken out of its list and its frame from flag 76,
/// the pans from flags 562 / 563, the beam (and its sound) when the scale
/// hangs (flag 79), then the frames listed (@ 0x004cc8d0).
pub(super) fn view_chest(h: &mut Hx) {
    if h.g.puzzles.ldmorts.chest_fresh {
        feathers_setup(h);
    }
    let (lid, old) = (h.g.puzzles.ldmorts.lid_id, h.g.puzzles.ldmorts.lid_frame);
    h.subf(lid, old);
    h.g.puzzles.ldmorts.lid_frame = if h.flag(76) == 1 { 0xe } else { 0 };
    for (p, flag) in [(0, 562), (1, 563)] {
        let f = h.flag(flag);
        if f == 0 {
            h.g.puzzles.ldmorts.pans[p] = None;
            h.g.puzzles.ldmorts.pan_weight[p] = -1;
        } else {
            h.g.puzzles.ldmorts.pan_weight[p] = f - 1;
            if let Some(n) = feather_of_weight(h, f - 1) {
                h.g.puzzles.ldmorts.pans[p] = Some(n);
            }
        }
    }
    if h.flag(79) == 1 {
        scales(h, &ZoneRec::default());
    }
    chest_frames(h);
    h.g.puzzles.ldmorts.chest_fresh = false;
}

/// The frames listed (@ 0x004cc8d0): in view 20 each feather's frame 0
/// taken out (when listed) and the feather on the pedestal (weight + 1 =
/// flag 84) listed; elsewhere the lid (frame 1 when open and the view is
/// not 3, listed whether or not it already is; else its frame when not
/// listed), the beam when the scale hangs, each pan's feather at its pan
/// frame (when not listed).
fn chest_frames(h: &mut Hx) {
    if h.vue() == 20 {
        for n in 0..10 {
            let f = h.g.puzzles.ldmorts.feathers[n];
            h.subf_if_listed(f.id, 0);
            if h.flag(84) == f.weight + 1 {
                h.addf(f.id, 0);
            }
        }
        return;
    }
    let p = &h.g.puzzles.ldmorts;
    let (lid, lid_frame, scale, scale_frame) = (p.lid_id, p.lid_frame, p.scale_id, p.scale_frame);
    if h.flag(76) == 1 && h.vue() != 3 {
        h.addf(lid, 1);
    } else {
        h.addf_if_absent(lid, lid_frame);
    }
    if h.flag(79) == 1 {
        h.addf_if_absent(scale, scale_frame);
    }
    for p in 0..2 {
        if let Some(n) = h.g.puzzles.ldmorts.pans[p] {
            let f = h.g.puzzles.ldmorts.feathers[n];
            h.addf_if_absent(f.id, f.pan_frame);
        }
    }
}

/// Views 3, 4, 20's frame (@ 0x004cc0f0).
pub(super) fn frame_chest(h: &mut Hx, z: &ZoneRec) {
    let vue = h.vue();
    if vue != 20 {
        // The lid's play (slot 3) over: the lid open, the subject offered.
        if h.slot(3).is_some_and(|s| s.mode == 1 && s.ended) {
            if let Some(s) = h.slot(3) {
                s.mode = 0;
            }
            let (lid, old) = (h.g.puzzles.ldmorts.lid_id, h.g.puzzles.ldmorts.lid_frame);
            h.subf(lid, old);
            h.g.puzzles.ldmorts.lid_frame = 0xe;
            h.addf(lid, 0xe);
            h.run("(EnableSuj,CHF_PSYCHOP,DIAL_TOUTES_PLUMES)");
        }
        if h.flag(76) == 1 && h.flag(85) == 1 && !h.g.puzzles.ldmorts.all_feathers_offered {
            h.run("(EnableSuj,CHF_PSYCHOP,DIAL_TOUTES_PLUMES)");
            h.g.puzzles.ldmorts.all_feathers_offered = true;
        }
        let mut n: i32 = (535..=544).map(|f| h.flag(f)).sum();
        if h.flag(562) != 0 {
            n -= 1;
        }
        if h.flag(563) != 0 {
            n -= 1;
        }
        if n == 1 {
            if !h.g.puzzles.ldmorts.one_feather_offered {
                h.run("(EnableSuj,CHF_PSYCHOP,DIAL_PLUME_CHOISIE)");
                h.g.puzzles.ldmorts.one_feather_offered = true;
            }
        } else if h.g.puzzles.ldmorts.one_feather_offered {
            h.run("(KillSuj,CHF_PSYCHOP,DIAL_PLUME_CHOISIE)");
            h.g.puzzles.ldmorts.one_feather_offered = false;
        }
    }
    if h.flag(84) != 0 || z.value == 0 {
        return;
    }
    // The chest opened with its key: the lid's play (slot 3) started.
    if h.flag(76) != 1
        && h.slot(3).map_or(0, |s| s.mode) != 1
        && z.name == "LDM_COUVERCLEPLUMES"
        && h.hand() == h.obj("LDM_CLEFCOFFRE")
    {
        let now = h.now();
        if let Some(s) = h.slot(3) {
            s.frame = 0;
            s.mode = 1;
            s.restart_clock(now);
        }
        h.run("(showobjet=LDM_PLUMES)");
        h.run("(ObjetEnMain=0)");
        h.run("(SacADos=NOT_LDM_CLEFCOFFRE)");
        h.run("(PlaySound = 10)");
        h.run("(KillSuj,CHF_PSYCHOP,DIAL_CLEF_PLUMES)");
        h.run("(KillSuj,CHF_PSYCHOP,DIAL_COFFRE_PLUMES)");
        h.set_flag(76, 1);
    }
    // The scale hung on its hook; once hung, the pans (not in view 20).
    let mut pans = h.flag(79) == 1;
    if h.flag(79) != 1 && h.hand() == h.obj("LDM_BALANCE_PR") {
        if z.name == "LDM_ACCROCHEBALANCE" {
            h.run("(ObjetEnMain=0)");
            h.run("(SacADos=NOT_LDM_BALANCE_PR)");
            let (scale, f) = (
                h.g.puzzles.ldmorts.scale_id,
                h.g.puzzles.ldmorts.scale_frame,
            );
            h.addf(scale, f);
            h.run("(KillSuj,CHF_PSYCHOP,DIAL_BALANCE)");
            h.run("(KillSuj,CHF_PSYCHOP,DIAL_PLACE_BALANCE)");
            h.set_flag(79, 1);
        }
        pans = false;
    }
    if pans && h.vue() != 20 {
        scales(h, z);
    }
    // A feather on the pedestal.
    if z.name == "LDM_ZONE_PLUME" {
        for n in 0..10 {
            let f = h.g.puzzles.ldmorts.feathers[n];
            if h.hand() == f.id {
                pedestal(h, f.weight);
                chest_frames(h);
            }
        }
    }
}

/// A feather on the pedestal (@ 0x004cc520): `flag84` := its weight + 1,
/// every feather out of the bag, the subjects killed, `CtrlScenarIN`.
fn pedestal(h: &mut Hx, weight: i32) {
    h.set_flag(84, weight + 1);
    h.run("(ObjetEnMain=0)");
    for n in 0..10 {
        h.run(&format!("(SacADos=NOT_LDM_PLUME{n})"));
    }
    h.run("(KillSuj,CHF_PSYCHOP,DIAL_GP)");
    h.run("(KillSuj,CHF_PSYCHOP,DIAL_PLUME_CHOISIE)");
    h.ctrl();
}

/// The scales (@ 0x004cca40): a click on a pan (`LDM_BALANCEG` → flag
/// 562, `LDM_BALANCED` → flag 563 := its feather's weight + 1, 0 empty),
/// then the beam from the two weights (its old frame out of the list
/// when listed, the new one listed) with `(PlaySound = 103)` when it
/// changes side, and the pans' feathers at their frames (the old frame
/// out when listed, the new one listed; not when the weights are equal).
fn scales(h: &mut Hx, z: &ZoneRec) {
    if z.name == "LDM_BALANCEG" {
        pan(h, 0);
        let w = h.g.puzzles.ldmorts.pan_weight[0];
        h.set_flag(562, w + 1);
    } else if z.name == "LDM_BALANCED" {
        pan(h, 1);
        let w = h.g.puzzles.ldmorts.pan_weight[1];
        h.set_flag(563, w + 1);
    }
    let [wg, wd] = h.g.puzzles.ldmorts.pan_weight;
    if wg == wd {
        beam_level(h);
    } else if wd < wg {
        set_beam(h, 1);
        if !h.g.puzzles.ldmorts.tilt_sounded {
            h.run("(PlaySound = 103)");
            h.g.puzzles.ldmorts.tilt_sounded = true;
            h.g.puzzles.ldmorts.level_sounded = false;
        }
        set_pan_frames(h, 1, 3);
    } else {
        beam_level(h);
        set_pan_frames(h, 0, 2);
    }
}

/// The beam's frame: the old one out of its list when listed, the new
/// one listed.
fn set_beam(h: &mut Hx, frame: i32) {
    let (scale, old) = (
        h.g.puzzles.ldmorts.scale_id,
        h.g.puzzles.ldmorts.scale_frame,
    );
    h.subf_if_listed(scale, old);
    h.g.puzzles.ldmorts.scale_frame = frame;
    h.addf(scale, frame);
}

/// The beam level (frame 0), its sound once.
fn beam_level(h: &mut Hx) {
    set_beam(h, 0);
    if !h.g.puzzles.ldmorts.level_sounded {
        h.run("(PlaySound = 103)");
        h.g.puzzles.ldmorts.level_sounded = true;
        h.g.puzzles.ldmorts.tilt_sounded = false;
    }
}

fn set_pan_frames(h: &mut Hx, left: i32, right: i32) {
    for (p, frame) in [(0, left), (1, right)] {
        if let Some(n) = h.g.puzzles.ldmorts.pans[p] {
            let f = h.g.puzzles.ldmorts.feathers[n];
            h.subf_if_listed(f.id, f.pan_frame);
            h.g.puzzles.ldmorts.feathers[n].pan_frame = frame;
            h.addf(f.id, frame);
        }
    }
}

/// A pan clicked (@ 0x004cce10): empty, with a feather in hand → the
/// feather on it (the hand variable 0, the feather out of the bag, the
/// hand cleared); holding a feather, with an empty hand → the feather
/// back in the bag (sprite n + 27) and in the hand, its pan frame out of
/// its list.
fn pan(h: &mut Hx, p: usize) {
    let hand = h.hand();
    match h.g.puzzles.ldmorts.pans[p] {
        None => {
            if hand == 0 || h.g.puzzles.ldmorts.pan_weight[p] != -1 {
                return;
            }
            for n in 0..10 {
                if h.g.puzzles.ldmorts.pan_weight[p] != -1 {
                    break;
                }
                let f = h.g.puzzles.ldmorts.feathers[n];
                if h.obj(&format!("LDM_PLUME{n}")) == hand {
                    h.g.puzzles.ldmorts.pans[p] = Some(n);
                    h.g.puzzles.ldmorts.pan_weight[p] = f.weight;
                    let k = h.s.keys.objetenmain;
                    h.s.vars[k] = 0;
                    h.s.bag_remove(f.id);
                    h.hand_clear();
                }
            }
        }
        Some(n) => {
            if hand != 0 {
                return;
            }
            let f = h.g.puzzles.ldmorts.feathers[n];
            h.s.bag_add(h.g, f.id, f.sprite as u8);
            h.s.hold(f.id);
            h.subf(f.id, f.pan_frame);
            h.g.puzzles.ldmorts.pans[p] = None;
            h.g.puzzles.ldmorts.pan_weight[p] = -1;
        }
    }
}

// ---- the Isis tiles (views 7..10) -------------------------------------

/// The tiles of one kind built (@ 0x004c8790): `prefix` 0..11, each
/// tile's own index taken out of its frame list when listed (twelve
/// times), then at a random free position (`rand() % 12` until free).
fn tiles_build(h: &mut Hx, prefix: &str, kind: i32, base: i32) -> [Tile; 12] {
    let mut out = [Tile::default(); 12];
    let mut taken = [false; 12];
    for (i, t) in out.iter_mut().enumerate() {
        t.id = h.obj(&format!("{prefix}{i}"));
        for _ in 0..12 {
            h.subf_if_listed(t.id, i as i32);
        }
        let mut p = h.rand() % 12;
        while taken[p as usize] {
            p = h.rand() % 12;
        }
        taken[p as usize] = true;
        t.frame = p;
        t.own = i as i32;
        t.kind = kind;
        t.index = i as i32 + base;
    }
    out
}

/// The positions saved: flags 510..521 (KI), 522..533 (KS).
fn tiles_save(h: &mut Hx) {
    for i in 0..12 {
        let (a, b) = (
            h.g.puzzles.ldmorts.ki[i].frame,
            h.g.puzzles.ldmorts.ks[i].frame,
        );
        h.set_flag(510 + i, a);
        h.set_flag(522 + i, b);
    }
}

/// Views 7..10's entry (@ 0x004c7f40, `PuzzleIsis_Init`; the dispatch
/// clears the zone value first): `LDM_CARREAUENMAIN` := 0, the tiles
/// rebuilt (24 random positions drawn), saved the first time (flag 506),
/// then per index (KI, then KS) the tile's current frame out of its list
/// when listed and its frame from the solved picture (flag 63) or the
/// saved positions; in `LDMorts.wam` views 7..10 every tile's frame
/// listed (each tile is a mode-0 slot showing its position); the warp
/// cursor reset, no tile held.
pub(super) fn view_isis(h: &mut Hx) {
    h.set_var("LDM_CARREAUENMAIN", 0);
    let ki = tiles_build(h, "LDM_ISISKI", 4, 0);
    let ks = tiles_build(h, "LDM_ISISKS", 5, 12);
    h.g.puzzles.ldmorts.ki = ki;
    h.g.puzzles.ldmorts.ks = ks;
    if h.flag(506) == 0 {
        tiles_save(h);
        h.set_flag(506, 1);
    }
    let solved = h.flag(63) == 1;
    for i in 0..12 {
        let (a, b) = (h.g.puzzles.ldmorts.ki[i], h.g.puzzles.ldmorts.ks[i]);
        h.subf_if_listed(a.id, a.frame);
        h.subf_if_listed(b.id, b.frame);
        let (fa, fb) = if solved {
            (a.own, b.own)
        } else {
            (h.flag(510 + i), h.flag(522 + i))
        };
        h.g.puzzles.ldmorts.ki[i].frame = fa;
        h.g.puzzles.ldmorts.ks[i].frame = fb;
    }
    h.g.puzzles.ldmorts.tiles_shown = true;
    let here = h.g.place.name == "LDMorts.wam" && (7..=10).contains(&h.vue());
    if here {
        for i in 0..12 {
            let (a, b) = (h.g.puzzles.ldmorts.ki[i], h.g.puzzles.ldmorts.ks[i]);
            h.addf(a.id, a.frame);
            h.addf(b.id, b.frame);
        }
    }
    h.g.puzzles.ldmorts.b71e264 = false;
    if h.g.puzzles.ldmorts.held.is_some() {
        log::warn!("PuzzleIsis_Init(): Carreau en main pas NULL");
    }
    h.g.reset_warp_cursor();
    h.g.puzzles.ldmorts.held = None;
}

/// The number after `LDM_ICARO` / `LDM_MCARO` (the first 9 characters
/// deleted, then `atoi`).
fn zone_number(name: &str) -> i32 {
    let s = name.get(9..).unwrap_or("");
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(0)
}

fn tiles<'a>(h: &'a mut Hx, kind: i32) -> &'a mut [Tile; 12] {
    if kind == 4 {
        &mut h.g.puzzles.ldmorts.ki
    } else {
        &mut h.g.puzzles.ldmorts.ks
    }
}

/// The held tile (@ 0x00786d10).
fn held_tile(h: &Hx) -> Option<Tile> {
    let (kind, i) = h.g.puzzles.ldmorts.held?;
    let arr = if kind == 4 {
        &h.g.puzzles.ldmorts.ki
    } else {
        &h.g.puzzles.ldmorts.ks
    };
    Some(arr[i])
}

/// A KS tile's sample (@ 0x004c8b19..0x004c8d7f): `pret0<390 + i>.apc`
/// in the world's voice folder (@ 0x00416030: `loc\voix\`, the voice
/// language, the world's folder).
fn tile_sample(h: &Hx, i: usize) -> String {
    let lang = h.g.startup.voice_language();
    let folder = super::super::WORLDS
        .get(h.world() as usize)
        .copied()
        .unwrap_or("");
    format!("loc\\voix\\{lang}\\{folder}\\pret0{}.apc", 390 + i)
}

/// A tile taken (@ 0x004c89c0), with an empty hand: the first tile of
/// the kind at the clicked position held, the cursor its in-hand sprite
/// (its index + 3: KI 3..14, KS 15..26), its frame out of its list when
/// listed; a KS tile's sample loaded as a voice and, when loaded, kept
/// and played. `LDM_CARREAUENMAIN` := 1 in any case.
fn isis_pick(h: &mut Hx, kind: i32) {
    if h.hand() == 0 {
        let n = zone_number(&h.g.puzzles.ldmorts.clicked.clone());
        if h.g.puzzles.ldmorts.held.is_none() {
            if let Some(i) = tiles(h, kind).iter().position(|t| t.frame == n) {
                h.g.puzzles.ldmorts.held = Some((kind, i));
                let t = tiles(h, kind)[i];
                h.g.set_warp_cursor(t.index + 3);
                h.subf_if_listed(t.id, t.frame);
                if kind == 5 {
                    let name = tile_sample(h, i);
                    h.g.sfx_load(&name, 1, 2);
                    if h.g.sfx_loaded(&name) {
                        h.g.puzzles.ldmorts.sample = name.clone();
                        h.g.sfx_play(&name);
                    }
                }
            }
        }
    }
    h.set_var("LDM_CARREAUENMAIN", 1);
}

/// The held tile dropped on another position of its kind (@ 0x004c8e50):
/// the tile there loses its frame from its list (when listed), the two
/// swap their positions and both are listed, the cursor reset, the hand
/// freed.
fn isis_swap(h: &mut Hx, kind: i32) {
    let n = zone_number(&h.g.puzzles.ldmorts.clicked.clone());
    let Some((_, k)) = h.g.puzzles.ldmorts.held else {
        return;
    };
    // The original's search has no bound: the positions are a permutation.
    if let Some(i) = tiles(h, kind).iter().position(|t| t.frame == n) {
        let other = tiles(h, kind)[i];
        h.subf_if_listed(other.id, other.frame);
        let arr = tiles(h, kind);
        arr[i].frame = arr[k].frame;
        arr[k].frame = n;
        let (a, b) = (arr[k], arr[i]);
        h.addf(a.id, a.frame);
        h.addf(b.id, b.frame);
    }
    h.g.reset_warp_cursor();
    h.g.puzzles.ldmorts.held = None;
    h.set_var("LDM_CARREAUENMAIN", 0);
}

/// The held KS tile's sample stopped (@ 0x004335f0(0x00786a80)) and the
/// name `Rien` again.
fn stop_sample(h: &mut Hx) {
    let name = std::mem::replace(&mut h.g.puzzles.ldmorts.sample, NO_SAMPLE.to_string());
    h.g.sfx_stop(&name);
}

/// The test after a move (@ 0x004c8730): every tile at its own position
/// in both sets → @ 0x004c8720: `flag63` := 1, `CtrlScenarIN`.
///
/// The original's loop reads one entry past each set as its stop marker
/// (@ 0x00786d1c / 0x00786d24 for KI, inside the paleo hunt's data;
/// @ 0x00786bbc / 0x00786bc4 for KS, inside a sound string object): it
/// is taken here as differing, the evident intent.
fn isis_test(h: &mut Hx) {
    let p = &h.g.puzzles.ldmorts;
    if p.ki.iter().chain(p.ks.iter()).all(|t| t.frame == t.own) {
        h.set_flag(63, 1);
        h.ctrl();
    }
}

/// The held KS tile's caption (@ 0x004c8f90, every frame): the layout
/// (10, 10, 20, 20, left, bottom) on the screen, then the dialog text
/// `GetTextFromDial(217 + i, 1)` (@ 0x004655a0) in `AtlFont_Default`,
/// white (@ 0x0041e200).
fn isis_caption(h: &mut Hx, own: i32) {
    let id = own + 0xd9;
    h.g.tr(|| format!("caption {id} 1"));
    h.g.tr(|| "draw box 10 10 20 20 0 1".into());
    h.g.tr(|| format!("textid {id} 1"));
    h.g.tr(|| "draw text AtlFont_Default".into());
    h.g.texts();
    let text =
        h.g.texts
            .as_ref()
            .and_then(|t| t.dial(id, 1))
            .map(<[u8]>::to_vec);
    let Some(text) = text else {
        return;
    };
    let (w, ht) = (h.g.width, h.g.height);
    let mut screen = std::mem::take(&mut h.g.screen);
    if let Some(t) = h.g.texts.as_ref() {
        t.print(
            &mut screen,
            w,
            ht,
            crate::text::FontId::Default,
            &text,
            crate::text::Zone::new(10, 10, 20, 20, 0, 1),
            0xff_ffff,
        );
    }
    h.g.screen = screen;
}

/// View 10's frame (@ 0x004c82b0): the tile play on the type-4 (KI) and
/// type-5 (KS) zones `LDM_ICARO<n>` / `LDM_MCARO<n>` (n = a position),
/// and once solved, with no re-alignment running, `flag899` := 1 and
/// `CtrlScenarIN` every frame.
pub(super) fn frame_isis(h: &mut Hx, z: &ZoneRec) {
    if h.hand() == 0 {
        if let Some(t) = held_tile(h).filter(|t| t.kind == 5) {
            h.g.puzzles.ldmorts.tile_text = true;
            isis_caption(h, t.own);
        }
    } else if let Some(t) = held_tile(h) {
        // An object put in hand: the tile dropped back (the variable
        // `LDM_CARREAUENMAIN` is not cleared).
        h.g.reset_warp_cursor();
        h.addf(t.id, t.frame);
        h.g.puzzles.ldmorts.held = None;
    }
    if h.flag(63) == 1 {
        if h.g.recalage_running() {
            return;
        }
        h.set_flag(899, 1);
        h.ctrl();
        if h.flag(63) == 1 {
            return;
        }
    }
    // (@ 0x004c8378: a sound string's bookkeeping, nothing the scripts
    // see.)
    if z.value == 0 {
        return;
    }
    let Some(held) = held_tile(h) else {
        h.g.puzzles.ldmorts.clicked = z.name.clone();
        h.g.puzzles.ldmorts.picked = z.name.clone();
        match z.kind {
            4 | 5 => isis_pick(h, z.kind),
            _ => {}
        }
        return;
    };
    h.g.puzzles.ldmorts.clicked = z.name.clone();
    if h.g.puzzles.ldmorts.clicked == h.g.puzzles.ldmorts.picked {
        // The same position again: the tile put back.
        h.g.reset_warp_cursor();
        if held.kind == 4 {
            h.run(" (PlaySound = 104) ");
        }
        if held.kind == 5 {
            stop_sample(h);
            h.run(" (PlaySound = 105) ");
        }
        h.addf(held.id, held.frame);
        h.set_var("LDM_CARREAUENMAIN", 0);
        h.g.puzzles.ldmorts.held = None;
    } else if z.kind == 4 {
        if held.kind == 4 {
            isis_swap(h, 4);
            h.run(" (PlaySound = 104) ");
        }
    } else if z.kind == 5 && held.kind == 5 {
        isis_swap(h, 5);
        stop_sample(h);
        h.run(" (PlaySound = 105) ");
    }
    tiles_save(h);
    isis_test(h);
}

/// The Isis tiles' success (@ 0x004c8720): `flag63` := 1, `CtrlScenarIN`.
pub(super) fn isis_success(h: &mut Hx) {
    h.set_flag(63, 1);
    h.ctrl();
}

/// Tests: a board's `st` line (the harness's C globals): `isis`,
/// `fisherman`, `feathers`, `ankh`.
pub(super) fn st_line(g: &crate::game::Game, board: &str) -> String {
    let p = &g.puzzles.ldmorts;
    let b = |v: bool| u8::from(v);
    let ints =
        |v: &mut dyn Iterator<Item = i32>| v.map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    match board {
        "isis" => {
            let held = p.held.map_or(
                -1,
                |(k, i)| if k == 4 { p.ki[i].index } else { p.ks[i].index },
            );
            let carreau = g
                .script
                .as_ref()
                .and_then(|s| s.var("LDM_CARREAUENMAIN"))
                .unwrap_or(0);
            format!(
                "held={held} text={} shown={} b71e264={} carreau={carreau} clicked={} picked={} \
                 sample={} ki={} ks={}",
                b(p.tile_text),
                b(p.tiles_shown),
                b(p.b71e264),
                p.clicked,
                p.picked,
                p.sample,
                ints(&mut p.ki.iter().map(|t| t.frame)),
                ints(&mut p.ks.iter().map(|t| t.frame)),
            )
        }
        "fisherman" => format!(
            "first={} id={} n={} frz={}",
            b(p.fisherman_fresh),
            p.fisherman_id,
            p.fisherman_count,
            b(g.puzzles.freeze)
        ),
        "feathers" => format!(
            "first={} lid={} plumes={} beam={} panG={} panD={} wG={} wD={} toutes={} choisie={} \
             sndlevel={} sndtilt={} weights={} panfr={}",
            b(p.chest_fresh),
            p.lid_frame,
            p.plumes_frame,
            p.scale_frame,
            p.pans[0].map_or(-1, |n| n as i32),
            p.pans[1].map_or(-1, |n| n as i32),
            p.pan_weight[0],
            p.pan_weight[1],
            b(p.all_feathers_offered),
            b(p.one_feather_offered),
            b(p.level_sounded),
            b(p.tilt_sounded),
            ints(&mut p.feathers.iter().map(|f| f.weight)),
            ints(&mut p.feathers.iter().map(|f| f.pan_frame)),
        ),
        "ankh" => {
            let (name, value) = g
                .puzzles
                .zone
                .as_ref()
                .map_or(("''".to_string(), 0), |z| (z.2.clone(), z.1));
            format!(
                "id={} count={} reached={} running={} b785f2f=0 b71d170=0 rec={name}/{value}",
                p.lid.id,
                p.lid.count,
                p.lid.reached,
                b(p.lid.running)
            )
        }
        _ => String::new(),
    }
}
