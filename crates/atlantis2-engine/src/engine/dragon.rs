//! The pearl and the dragon in the clouds (`pz_dragon`), `miniature.wam`
//! view 31 (chapter 58).
//!
//! The view has a dragon (`DRAGON_NUAGE`, one animated state of 80
//! frames, held on frame 50), two clouds (`NUAGE_GAUCHE`,
//! `NUAGE_DROITE`) with a pearl behind each (`PERLE_NUAGE_GAUCHE`,
//! `PERLE_NUAGE_DROITE`), five low clouds (`NUAGE_BAS1..5`, frame 1
//! raised, 0 lowered) and the pearl with a hole (`PERLE_TROU`, an object
//! to take). A click on the dragon (zone `CLIC_DRAGON`) starts the first
//! round: the dragon plays once round to frame 50 and hides the pearl,
//! left or right after the fixed sequence `ABBAB` (`Perle_cachee` 'A' or
//! 'B', `Num_sequence_dragon` 0..4). A click on a cloud (zones
//! `CLIC_NUAGE_GAUCHE` / `_DROITE`) makes it transparent for 30 frames,
//! the pearl showing through it when it hides there; then the low clouds
//! rise one step for the right cloud and fall one for the wrong one (a
//! sound 205 + the new count each), and the next round starts by itself.
//! Five raised (`State_nuages_bas` = 5): `FINPERLE` = 1 and `PERLE_TROU`
//! shown. Nothing is random.
//!
//! - The view entry FUN_004944a0 (from FUN_0049bd50 for `SS` view 31):
//!   the slots of the five low clouds and of the dragon kept
//!   (@ 0x006d26c4..0x006d26d4, 0x006d26e0; each the first slot of its
//!   id, FUN_004949f0, none being the fatal `pz_dragon find_idx(%d)
//!   introuvable`), both pearls' slots made inactive, the dragon put in
//!   state 0 (FUN_00494a40(0)); with `FINPERLE` = 1: `State_nuages_bas` =
//!   5 and the low clouds placed (FUN_00494990), nothing else; otherwise
//!   the puzzle mode 4 (@ 0x004945a5), the transparency restored when
//!   `Time_nuage_transparent` is not 0 (FUN_00494af0 of
//!   `NuageTransparent`), the low clouds placed and `PERLE_TROU` hidden
//!   (FUN_0048c6b0(id, 2, 0): its slot and the show / hide table).
//! - The click FUN_00494600(zone) (from FUN_0049b910 for zone types 3, 4,
//!   0x32 of `SS` view 31; it never consumes the click): nothing unless
//!   the mode is 4, `Time_nuage_transparent`, the dragon's state and
//!   `FINPERLE` are 0. Then, once a pearl is hidden (`Perle_cachee` ≠ 0),
//!   the zone's name (record +0x08, compared byte for byte) chooses: the
//!   left cloud sets `Action_nuage_bas` = 1 when 'A' is hidden, else 2,
//!   and starts the transparency of side 1 (FUN_00494c00); the right one
//!   1 for 'B', else 2, side 2. Before the first round (`Perle_cachee` =
//!   0), `CLIC_DRAGON` starts it (FUN_00494a40(1)).
//! - The step FUN_004947b0, once per warp frame while the mode is 4
//!   (FUN_0049bfb0 from `AtlWarp_continue` @ 0x0045a5e8, after the move's
//!   completion, whether turning or not, not while paused): the dragon's
//!   state 1 → 2 → 3 one frame each, 3 → 0 at a frame where its slot shows
//!   frame 50 (held there: playing byte cleared); then the transparency's
//!   count down, and at its end the pearls hidden, the cloud opaque again,
//!   the low clouds raised or lowered, the win at five, and the next round
//!   while fewer than five are raised.
//!
//! **Timing.** The dragon's frames move with the 15 Hz step of the place
//! animations; the puzzle's step and its 30-frame transparency count warp
//! frames, which the original does not pace. The dragon is stopped at the
//! first warp frame of state 3 that finds it on frame 50: when no 15 Hz
//! step ran in the two warp frames after the click, it has not left frame
//! 50 and the round starts without its flight (at 60 frames a second, a
//! click in two of four frames; the rounds that start by themselves have
//! three frames). The transparent cloud is drawn only when the 15 Hz
//! counter is odd (FUN_0047cf00 @ 0x0047d181..0x0047d1aa: slot +0xc0 ≠ 0).

use atlantis2_data::wam::Zone;

use super::Engine;

/// The strings of the module (@ 0x005e1b10..0x005e1c94).
const DRAGON: &[u8] = b"DRAGON_NUAGE";
const LOW_CLOUDS: [&[u8]; 5] = [
    b"NUAGE_BAS1",
    b"NUAGE_BAS2",
    b"NUAGE_BAS3",
    b"NUAGE_BAS4",
    b"NUAGE_BAS5",
];
const PEARL_LEFT: &[u8] = b"PERLE_NUAGE_GAUCHE";
const PEARL_RIGHT: &[u8] = b"PERLE_NUAGE_DROITE";
const HOLE: &[u8] = b"PERLE_TROU";
const CLOUD_LEFT: &[u8] = b"NUAGE_GAUCHE";
const CLOUD_RIGHT: &[u8] = b"NUAGE_DROITE";
const ZONE_LEFT: &[u8] = b"CLIC_NUAGE_GAUCHE";
const ZONE_RIGHT: &[u8] = b"CLIC_NUAGE_DROITE";
const ZONE_DRAGON: &[u8] = b"CLIC_DRAGON";
const FIN: &str = "FINPERLE";
const LOW: &str = "State_nuages_bas";
const TIME: &str = "Time_nuage_transparent";
const SIDE: &str = "NuageTransparent";
const HIDDEN: &str = "Perle_cachee";
const ACTION: &str = "Action_nuage_bas";
const SEQUENCE_VAR: &str = "Num_sequence_dragon";

/// The hiding places (@ 0x005e1b10) and the three zero bytes after them;
/// the byte read is `Num_sequence_dragon`'s (`movsx` @ 0x00494a73).
const SEQUENCE: [u8; 8] = *b"ABBAB\0\0\0";
const LEFT: i32 = 0x41;
const RIGHT: i32 = 0x42;

/// The puzzle mode the entry sets (@ 0x004945a5).
pub(super) const DRAGON_MODE: i32 = 4;

/// The frame the dragon is held on and waits for (@ 0x00494acb,
/// @ 0x004947e1).
const REST_FRAME: i32 = 0x32;

/// The transparency's length in warp frames (@ 0x00494c01) and the value
/// the cloud's slot +0xc0 takes (@ 0x00494bed).
const TRANSPARENT_FRAMES: i32 = 0x1e;
const BLINK: i32 = 0x32;

/// The sound of a step is 205 + the new count (FUN_00494970).
const SOUND_BASE: i32 = 0xcd;

/// The puzzle's engine state, never saved nor reset (`.bss`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dragon {
    /// The dragon's state (@ 0x006d26e4): 0 still, 1 just started, 2, 3
    /// flying.
    pub state: i32,
    /// The slot of the cloud last made transparent (@ 0x006d26e8; 0 before
    /// any).
    pub cloud: i32,
    /// The slots of the five low clouds (@ 0x006d26c4..0x006d26d4).
    pub low: [i32; 5],
    /// The dragon's slot (@ 0x006d26e0).
    pub dragon: i32,
}

impl Engine {
    /// FUN_004949f0 (`find_idx`): the first slot whose id is that of the
    /// object `name`, whatever its kind. None: the original's fatal error
    /// (FUN_00480cd0: an emergency save, then the game quits); here the
    /// scripts stop and so does the hook. The game's data never reach it.
    fn dragon_find(&mut self, name: &[u8]) -> Option<i32> {
        let id = self.object_index(name)?;
        if let Some(n) = self.anim.slots.iter().position(|s| s.id == id) {
            return Some(n as i32);
        }
        let msg = format!("pz_dragon find_idx({id}) introuvable");
        log::error!("{msg}");
        self.with_script(|s, _| s.fail(msg));
        None
    }

    /// `Interp_ReadVar` (FUN_00484ff0) of one of the puzzle's variables.
    fn dragon_var(&self, name: &str) -> i32 {
        self.con.as_deref().and_then(|s| s.var(name)).unwrap_or(0)
    }

    /// `Interp_WriteVar` (FUN_00485010).
    fn dragon_set(&mut self, name: &str, value: i32) {
        self.with_script(|s, _| s.set_var(name, value));
    }

    /// `Interp_AddVar` (FUN_00485040).
    fn dragon_add(&mut self, name: &str, delta: i32) {
        self.with_script(|s, _| s.add_var(name, delta));
    }

    /// Slot `n` of the view, if the view has it (a stale index, kept from
    /// view 31 while the mode 4 outlives it, names no slot of the port;
    /// the original reads and writes the record left there).
    fn dragon_slot(&mut self, n: i32) -> Option<&mut crate::anim::Slot> {
        usize::try_from(n)
            .ok()
            .and_then(|n| self.anim.slots.get_mut(n))
    }

    /// The pearl behind the cloud `name` made inactive (record +0x30 = 0).
    fn dragon_pearl(&mut self, name: &[u8], shown: bool) -> Option<()> {
        let n = self.dragon_find(name)?;
        if let Some(s) = self.dragon_slot(n) {
            s.active = shown;
        }
        Some(())
    }

    /// FUN_004944a0 (run by the entry of `SS` view 31; public for tests
    /// and tools: the hook alone).
    pub fn dragon_entry(&mut self) {
        for (k, name) in LOW_CLOUDS.iter().enumerate() {
            let Some(n) = self.dragon_find(name) else {
                return;
            };
            self.dragon.low[k] = n;
        }
        let Some(n) = self.dragon_find(DRAGON) else {
            return;
        };
        self.dragon.dragon = n;
        if self.dragon_pearl(PEARL_LEFT, false).is_none()
            || self.dragon_pearl(PEARL_RIGHT, false).is_none()
        {
            return;
        }
        self.dragon_state(0);
        if self.dragon_var(FIN) == 1 {
            self.dragon_set(LOW, 5);
            self.dragon_low_clouds();
            return;
        }
        self.set_puzzle_mode(DRAGON_MODE);
        if self.dragon_var(TIME) != 0 {
            let side = self.dragon_var(SIDE);
            if self.dragon_transparent(side).is_none() {
                return;
            }
        }
        self.dragon_low_clouds();
        let Some(hole) = self.object_index(HOLE) else {
            return;
        };
        self.with_script(|s, h| s.hide(h, hole, 2, false));
    }

    /// FUN_00494a40(state): 1 starts a round unless one has just started
    /// (the dragon plays, the next hiding place of the sequence, which
    /// wraps after five); 0 holds the dragon on frame 50; the state
    /// stored.
    fn dragon_state(&mut self, state: i32) {
        if state == 1 {
            if self.dragon.state == 1 {
                return;
            }
            let n = self.dragon.dragon;
            if let Some(s) = self.dragon_slot(n) {
                s.playing = true;
            }
            let k = self.dragon_var(SEQUENCE_VAR);
            let place = match usize::try_from(k).ok().and_then(|k| SEQUENCE.get(k)) {
                Some(&b) => i32::from(b as i8),
                None => {
                    // The bytes after the table (the module's other
                    // strings): only a save written by hand reaches them.
                    log::warn!("Num_sequence_dragon {k}: out of the sequence");
                    0
                }
            };
            self.dragon_set(HIDDEN, place);
            self.dragon_add(SEQUENCE_VAR, 1);
            if self.dragon_var(SEQUENCE_VAR) > 4 {
                self.dragon_set(SEQUENCE_VAR, 0);
            }
        } else if state == 0 {
            let n = self.dragon.dragon;
            if let Some(s) = self.dragon_slot(n) {
                s.frame = REST_FRAME;
                s.playing = false;
            }
        }
        self.dragon.state = state;
    }

    /// FUN_00494990: each low cloud held (playing byte cleared) on frame 1
    /// when `State_nuages_bas` is above its rank (0..4), else 0.
    fn dragon_low_clouds(&mut self) {
        for k in 0..5 {
            let n = self.dragon.low[k];
            let up = (k as i32) < self.dragon_var(LOW);
            if let Some(s) = self.dragon_slot(n) {
                s.playing = false;
                s.frame = i32::from(up);
            }
        }
    }

    /// FUN_00494970: `RunOneCommand("(NewSound=%03d)", 205 +
    /// State_nuages_bas)` (FUN_004818c0).
    fn dragon_sound(&mut self) {
        let c = format!(
            "(NewSound={:03})",
            self.dragon_var(LOW).wrapping_add(SOUND_BASE)
        );
        self.with_script(|s, h| s.run_command(h, c.as_bytes()));
    }

    /// FUN_00494af0(side): both pearls hidden; side 1 the left cloud, the
    /// left pearl shown when 'A' is hidden; any other side the right
    /// cloud, the right pearl for 'B'; the cloud's slot kept and made
    /// transparent (+0xc0 = 0x32).
    fn dragon_transparent(&mut self, side: i32) -> Option<()> {
        self.dragon_pearl(PEARL_LEFT, false)?;
        self.dragon_pearl(PEARL_RIGHT, false)?;
        let (cloud, pearl, place) = if side == 1 {
            (CLOUD_LEFT, PEARL_LEFT, LEFT)
        } else {
            (CLOUD_RIGHT, PEARL_RIGHT, RIGHT)
        };
        self.dragon.cloud = self.dragon_find(cloud)?;
        if self.dragon_var(HIDDEN) == place {
            self.dragon_pearl(pearl, true)?;
        }
        let n = self.dragon.cloud;
        if let Some(s) = self.dragon_slot(n) {
            s.blink = BLINK;
        }
        Some(())
    }

    /// FUN_00494600 on a zone (run by the clicks on zones of type 3, 4 and
    /// 0x32 of `SS` view 31; public for tests and tools: the hook alone,
    /// without the dispatcher and the scans around it).
    pub fn dragon_click(&mut self, zone: &Zone) {
        if self.puzzle_mode() != DRAGON_MODE
            || self.dragon_var(TIME) != 0
            || self.dragon.state != 0
            || self.dragon_var(FIN) != 0
        {
            return;
        }
        // The zone's name (record +0x08) against the strings, byte for
        // byte (the inline `strcmp`s @ 0x0049466c, 0x004946dc,
        // 0x00494756); a zone of the 8-byte form has an empty name.
        let is = |name: &[u8]| {
            zone.name
                .as_deref()
                .unwrap_or("")
                .chars()
                .map(|c| c as u32)
                .eq(name.iter().map(|&b| u32::from(b)))
        };
        let hidden = self.dragon_var(HIDDEN);
        if hidden != 0 {
            let side = if is(ZONE_LEFT) {
                Some((1, LEFT))
            } else if is(ZONE_RIGHT) {
                Some((2, RIGHT))
            } else {
                None
            };
            if let Some((side, place)) = side {
                self.dragon_set(ACTION, if hidden == place { 1 } else { 2 });
                // FUN_00494c00.
                self.dragon_set(TIME, TRANSPARENT_FRAMES);
                self.dragon_set(SIDE, side);
                if self.dragon_transparent(side).is_none() {
                    return;
                }
            }
        }
        if self.dragon_var(HIDDEN) == 0
            && is(ZONE_DRAGON)
            && self.dragon.state == 0
            && self.dragon_var(TIME) == 0
        {
            self.dragon_state(1);
        }
    }

    /// FUN_004947b0: the per-frame step of the puzzle mode 4 (public for
    /// tests and tools: the step alone).
    pub fn dragon_tick(&mut self) {
        match self.dragon.state {
            1 => self.dragon_state(2),
            2 => self.dragon_state(3),
            3 => {
                let n = self.dragon.dragon;
                if self.dragon_slot(n).is_some_and(|s| s.frame == REST_FRAME) {
                    self.dragon_state(0);
                }
            }
            _ => {}
        }
        if self.dragon_var(TIME) == 0 {
            return;
        }
        self.dragon_add(TIME, -1);
        if self.dragon_var(TIME) != 0 {
            return;
        }
        if self.dragon_pearl(PEARL_LEFT, false).is_none()
            || self.dragon_pearl(PEARL_RIGHT, false).is_none()
        {
            return;
        }
        let n = self.dragon.cloud;
        if let Some(s) = self.dragon_slot(n) {
            s.blink = 0;
        }
        if self.dragon_var(ACTION) == 1 && self.dragon_var(LOW) < 5 {
            self.dragon_add(LOW, 1);
            self.dragon_sound();
            self.dragon_low_clouds();
            self.dragon_set(ACTION, 0);
            if self.dragon_var(LOW) == 5 {
                self.dragon_set(FIN, 1);
                let Some(hole) = self.object_index(HOLE) else {
                    return;
                };
                self.with_script(|s, h| s.show(h, hole, 2, 0));
            }
        }
        if self.dragon_var(ACTION) == 2 && self.dragon_var(LOW) > 0 {
            self.dragon_add(LOW, -1);
            self.dragon_sound();
            self.dragon_low_clouds();
            self.dragon_set(ACTION, 0);
        }
        if self.dragon_var(LOW) != 5 {
            self.dragon_state(1);
        }
    }
}
