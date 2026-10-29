//! The corridor of the interdimension (`couloir.wam`, world 5, chapter
//! 53): the doors opened in order. chapi053 turns a click on a door
//! (`DepartVue=30` left / `31` right) into flag 920; the frame plays the
//! door `COU_PORTE<VUE><G|D>`, compares it with the sequence and puts the
//! player back in view 15.

use super::ldmorts::{anim_start, anim_step, AnimRec};
use super::{Hx, ZoneRec};

/// The doors to open, in order (@ 0x0070054c, 0x20 bytes each; the entry
/// after the sixth is an empty string: no door matches it).
const SEQUENCE: [&str; 6] = [
    "COU_PORTE15G",
    "COU_PORTE17D",
    "COU_PORTE18G",
    "COU_PORTE17G",
    "COU_PORTE15D",
    "COU_PORTE20G",
];

/// The corridor's globals.
#[derive(Debug, Default, Clone)]
pub struct State {
    /// Solved (@ 0x00786519): the frame hook stops calling the frame, then
    /// runs [`open`] once.
    pub solved: bool,
    /// Solved, as restored from flag 870 (@ 0x00786518).
    pub solved_saved: bool,
    /// The opened door handled (@ 0x0078651a).
    pub handled: bool,
    /// No door opening (@ 0x0071d4c1, 1 at start-up).
    pub idle: bool,
    /// The step in the sequence (@ 0x0078651c, mirrored in flag 871) and
    /// the doors clicked (@ 0x00786520, mirrored in flag 906).
    pub step: i32,
    pub count: i32,
    /// Restore from the flags at the next entry (@ 0x0071d4c0, 1 at
    /// start-up and after a load).
    pub restore: bool,
    /// The door opening (@ 0x00786100) and its play (@ 0x00786508).
    pub door: String,
    pub anim: AnimRec,
}

impl State {
    pub fn new() -> State {
        State {
            idle: true,
            restore: true,
            ..Default::default()
        }
    }

    /// @ 0x00459e70.
    pub fn after_load(&mut self) {
        self.restore = true;
    }
}

/// The entry (@ 0x004c4930): everything cleared at the first visit (flag
/// 918); after start-up or a load the step and count from flags 871 /
/// 906 and "solved" from flag 870 (which no code writes).
pub(super) fn view(h: &mut Hx) {
    if h.flag(918) == 0 {
        h.set_flag(918, 1);
        let c = &mut h.g.puzzles.couloir;
        c.handled = false;
        c.idle = true;
        c.step = 0;
        c.count = 0;
        h.set_flag(871, 0);
        h.set_flag(906, 0);
        let c = &mut h.g.puzzles.couloir;
        c.solved = false;
        c.solved_saved = false;
    }
    if h.g.puzzles.couloir.restore {
        let (count, step, saved) = (h.flag(906), h.flag(871), h.flag(870) != 0);
        let c = &mut h.g.puzzles.couloir;
        c.handled = false;
        c.count = count;
        c.step = step;
        c.solved_saved = saved;
        c.idle = true;
        c.solved = saved;
    }
    h.g.puzzles.couloir.restore = false;
}

/// The frame (@ 0x004c49c0; the zone record is not read), while not
/// solved: a door asked (flag 920 = 30 / 31) starts its play (input
/// frozen, flag 872 = 1; the second door ever clicked sets flag 17); at
/// the play's end the door is compared with the sequence's step (flag
/// 871), the sixth right door with flags 159, 866, 867, 908 and 605 set
/// solves it, a wrong one resets the step (to 1 for `COU_PORTE15G`);
/// unless solved, back to view 15.
pub(super) fn frame(h: &mut Hx, _z: &ZoneRec) {
    if !h.g.puzzles.couloir.solved && h.flag(872) == 0 {
        let vue = h.vue();
        let asked = h.flag(920);
        if asked == 30 || asked == 31 {
            h.g.puzzles.freeze = true;
            let side = if asked == 30 { 'G' } else { 'D' };
            let name = format!("COU_PORTE{vue}{side}");
            log::info!("clic sur porte {name}");
            h.g.puzzles.couloir.door = name.clone();
            h.set_flag(872, 1);
            h.g.puzzles.couloir.idle = false;
            let mut anim = std::mem::take(&mut h.g.puzzles.couloir.anim);
            anim_start(h, &mut anim, &name, 20);
            h.g.puzzles.couloir.anim = anim;
            let count = h.g.puzzles.couloir.count + 1;
            h.set_flag(906, count);
            if count == 2 {
                h.set_flag(17, 1);
            }
            h.set_flag(920, 0);
            h.g.puzzles.couloir.count = count;
        }
    }
    if h.g.puzzles.couloir.idle || h.g.puzzles.couloir.handled {
        return;
    }
    let mut anim = std::mem::take(&mut h.g.puzzles.couloir.anim);
    let done = anim_step(h, &mut anim);
    h.g.puzzles.couloir.anim = anim;
    if !done {
        return;
    }
    h.g.puzzles.couloir.handled = true;
    let step = h.g.puzzles.couloir.step;
    let door = h.g.puzzles.couloir.door.clone();
    let expected = usize::try_from(step)
        .ok()
        .and_then(|s| SEQUENCE.get(s))
        .copied()
        .unwrap_or("");
    if door == expected {
        log::info!("Etape validée : {step}");
        h.set_flag(871, step + 1);
        h.g.puzzles.couloir.step = step + 1;
        if step + 1 == 6 && [159, 866, 867, 908, 605].iter().all(|&f| h.flag(f) != 0) {
            log::info!("Puzzle Résolu");
            h.g.puzzles.couloir.solved = true;
        }
    } else {
        let mut s = 0;
        if door == "COU_PORTE15G" {
            log::info!("Etape validée : {s}");
            s += 1;
        }
        h.g.puzzles.couloir.step = s;
        h.set_flag(871, s);
    }
    h.g.puzzles.freeze = false;
    let c = &mut h.g.puzzles.couloir;
    c.idle = true;
    c.handled = false;
    h.set_flag(872, 0);
    if !h.g.puzzles.couloir.solved {
        h.run("(AngleWarpX=180)");
        h.run("(Vue=15)");
    }
}

/// The corridor solved (@ 0x004c4bf0, from the frame hook once the frame
/// set [`State::solved`]): flag 873 := 1, `CtrlScenarIN` (chapi053 leaves
/// for `Rhea.wam`).
pub(super) fn open(h: &mut Hx) {
    h.g.puzzles.couloir.solved_saved = true;
    h.set_flag(873, 1);
    h.ctrl();
}
