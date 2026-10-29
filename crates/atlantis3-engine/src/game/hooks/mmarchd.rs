//! The merchant's house (`mmarchd.wam`, world 3, chapters 28 and 29): the
//! guard (`Puzzle_Garde`, view entry @ 0x004c5590, frame @ 0x004c5dc0).
//!
//! The C code never writes a success: it runs the guard's state G
//! (@ 0x0071d5c0, kept in flag 885), his grumbling, the stone throw, and
//! catches the player by setting flag 880, 881, 882 or 888 and running
//! `CtrlScenarIN` (the chapters' capture sections). The way out is
//! scripted (chapi028 `DepartVue=13 & Flag124=1`, chapi029 `DepartVue=14`).
//!
//! Sounds: the original asks its sound objects whether they still play
//! (@ 0x00433780 on 0x007869a8 the guard's voice, 0x007869f8 the cat's,
//! 0x00786968 the capture voices, 0x00786a18 the ambience). The port has
//! no per-object sound state yet: [`playing`] answers false (M4), so each
//! voice or ambience counts as ended at the next test.

use super::{Hx, ZoneRec};

/// The guard's character name (the CString @ 0x00786550).
const GARDE: &str = "MMA_GARDE";

#[derive(Debug, Default, Clone)]
pub struct State {
    /// The reload byte (@ 0x0071d5bc): 1 at start and after a load; the
    /// next view entry rebuilds the run-time state from the flags.
    pub reload: bool,
    /// G, the guard's state (@ 0x0071d5c0, saved in flag 885): 1 on his
    /// patrol, 2 sent away by the stone, 3 and 4 coming back, 5 alerted by
    /// the grumbling.
    pub guard: i32,
    /// The stone thrown (@ 0x00786a44; flag 884 at a reload).
    pub stone: bool,
    /// The grumbling level (@ 0x00786980, saved in flag 869).
    pub grumble: i32,
    /// View 8 entered (@ 0x00786a4f, saved in flag 889): any other view
    /// entry then is caught 888.
    pub been_8: bool,
    /// The ambience playing (@ 0x007869bc): 0 none, 1 `Sound800.APC`, 2
    /// `Sound801.APC`; its pause's clock and length in seconds
    /// (@ 0x00786538, 0x00786a0c), and the byte "it ended" (@ 0x00786984).
    pub amb: i32,
    pub amb_start: f64,
    pub amb_delay: i32,
    pub amb_ended: bool,
    /// A guard voice running (@ 0x007869c5); the pause after it: clock and
    /// length in seconds (@ 0x007869c8, 0x00786a10).
    pub voice_on: bool,
    pub pause_start: f64,
    pub pause: i32,
    /// The cat's voice started (@ 0x00786a4c) and the cursor seen on a
    /// sprite in view 11 (@ 0x00786a4d).
    pub cat_voice: bool,
    pub cat_seen: bool,
    /// The capture voices' step (@ 0x00786a48, 0..3).
    pub voices_883: i32,
    /// The delayed capture: pending (@ 0x00786a4e), its clock to restart
    /// at the next frame (@ 0x00786a58), the clock (@ 0x007869e0), the
    /// camera command to run (@ 0x00786564) and the flag to set
    /// (@ 0x0078697c).
    pub pending: bool,
    pub pending_restart: bool,
    pub pending_start: f64,
    pub pending_cmd: String,
    pub pending_flag: usize,
    /// The stone's flight (@ 0x00786a50), its clock (@ 0x00786988) and
    /// frame (@ 0x00786a54, 40 frames at 15 Hz).
    pub flight: bool,
    pub flight_start: f64,
    pub flight_frame: i32,
    /// Caught (@ 0x00786a59): the next frame resets and scans.
    pub caught: bool,
    /// The camera lock (@ 0x0077b8ac, a warp global: 0x00440740 sets it,
    /// 0x00440750 clears it; while set the turn 0x00440760 does nothing).
    pub cam_lock: bool,
}

impl State {
    pub fn new() -> State {
        State {
            reload: true,
            ..Default::default()
        }
    }

    /// @ 0x00459e70: 0x0071d5bc := 1.
    pub fn after_load(&mut self) {
        self.reload = true;
    }
}

/// Whether a sound object still plays (@ 0x00433780): see the module's
/// doc (M4: no per-object sound state in the port).
fn playing(_h: &Hx, _object: u32) -> bool {
    false
}

/// The guard's state written with its flag.
fn set_guard(h: &mut Hx, g: i32) {
    h.g.puzzles.mmarchd.guard = g;
    h.set_flag(885, g);
}

/// The camera turned half a turn (@ 0x00440760(π)), unless locked.
fn turn(h: &mut Hx) {
    if !h.g.puzzles.mmarchd.cam_lock {
        h.g.camera.yaw(std::f32::consts::PI);
    }
}

/// Caught at once (the view entry's form): the camera command, the
/// capture byte, flag n.
fn caught(h: &mut Hx, cmd: &str, n: usize) {
    h.run(cmd);
    h.g.puzzles.mmarchd.caught = true;
    h.set_flag(n, 1);
}

/// The guard back on his patrol: anims 1 / 1, G := 1 (@ 0x004c5c7b).
fn patrol(h: &mut Hx) {
    h.run("(playcutanimp=MMA_GARDE,1)");
    h.run("(playanimp=MMA_GARDE,1)");
    set_guard(h, 1);
}

/// The delayed capture armed (@ 0x004c5bde): pending, input frozen, its
/// clock restarted (and once more at the next frame).
fn delayed(h: &mut Hx, cmd: &str, n: usize) {
    h.run(cmd);
    let now = h.now();
    let m = &mut h.g.puzzles.mmarchd;
    m.pending_cmd = cmd.to_string();
    m.pending_flag = n;
    m.pending = true;
    m.pending_start = now;
    m.pending_restart = true;
    h.g.puzzles.freeze = true;
}

/// The view entry (@ 0x004c5590).
pub(super) fn view(h: &mut Hx) {
    let vue = h.vue();
    // visual: the first time (@ 0x0071d5bd) the `PierreVoleur%d.spr`
    // bank is loaded (@ 0x004c7210) (M4).
    if h.flag(910) == 0 {
        h.set_flag(910, 1);
        let m = &mut h.g.puzzles.mmarchd;
        m.stone = false;
        m.guard = 1;
        m.grumble = 0;
        m.been_8 = false;
        m.amb = 0;
        m.voice_on = false;
        h.set_flag(884, 0);
        h.set_flag(885, 1);
        h.set_flag(869, 0);
        h.set_flag(889, 0);
        // sound: the four sound objects' names cleared (M4).
    }
    if h.g.puzzles.mmarchd.reload {
        let (f884, f869, f889, f885) = (h.flag(884), h.flag(869), h.flag(889), h.flag(885));
        let m = &mut h.g.puzzles.mmarchd;
        m.stone = f884 != 0;
        m.grumble = f869;
        m.been_8 = f889 != 0;
        m.guard = f885;
        m.amb = 0;
        m.voice_on = false;
    }
    let now = h.now();
    {
        let m = &mut h.g.puzzles.mmarchd;
        m.pending_restart = false;
        m.cat_voice = false;
        m.cat_seen = false;
        m.flight = false;
        m.flight_frame = 0;
        m.pending = false;
        m.pause_start = now;
        m.amb_start = now;
        m.pause = 0;
        m.amb_delay = 2;
        m.caught = false;
    }
    h.g.puzzles.freeze = false;
    if h.g.puzzles.mmarchd.been_8 && vue != 8 {
        // Chapter 29's capture: any view entered after view 8.
        turn(h);
        h.set_flag(888, 1);
        h.g.puzzles.mmarchd.caught = true;
        h.g.puzzles.mmarchd.reload = false;
        return;
    }
    let g = h.g.puzzles.mmarchd.guard;
    match vue {
        1 => {
            if h.flag(119) != 0 && g != 5 {
                if g == 3 {
                    h.run("(playcutanimp=MMA_GARDE,5)");
                    set_guard(h, 4);
                } else if g == 2 {
                    caught(h, "(AngleWarpXNow=250)", 880);
                } else {
                    patrol(h);
                }
            } else {
                caught(h, "(AngleWarpXNow=250)", 880);
            }
        }
        2 => {
            if g == 2 {
                h.run("(playcutanimp=MMA_GARDE,4)");
                set_guard(h, 3);
                // sound: the guard's voice object stopped (@ 0x00443c90,
                // @ 0x0040dcc0) before the new voice (M4).
                h.play_voice(190, 1, 255, 100, 100);
                h.g.puzzles.mmarchd.voice_on = true;
            } else if g == 1 {
                h.run("(playcutanimp=MMA_GARDE,1)");
                set_guard(h, 1);
                // 881 when no stone was thrown, else 880.
                let n = if h.g.puzzles.mmarchd.stone { 880 } else { 881 };
                delayed(h, "(AngleWarpXNow=235)", n);
            } else {
                caught(h, "(AngleWarpXNow=235)", 880);
            }
        }
        3 => {
            h.run("(playcutanimp=MMA_GARDE,1)");
            set_guard(h, 1);
            caught(h, "(AngleWarpXNow=270)", 882);
        }
        4 => {
            h.run("(playcutanimp=MMA_GARDE,1)");
            if g == 1 {
                delayed(h, "(AngleWarpXNow=325)", 882);
            } else {
                caught(h, "(AngleWarpXNow=235)", 880);
            }
        }
        5 => {
            h.set_flag(886, 1);
            if g == 5 || g == 2 {
                caught(h, "(AngleWarpXNow=290)", 880);
            } else {
                patrol(h);
            }
        }
        8 => {
            if h.flag(889) == 0 {
                h.g.puzzles.mmarchd.been_8 = true;
                h.set_flag(889, 1);
                h.g.puzzles.mmarchd.voice_on = true;
                h.play_voice(192, 1, 255, 100, 100);
            }
        }
        11 => {
            if g == 5 {
                caught(h, "(AngleWarpXNow=290)", 880);
            } else {
                patrol(h);
            }
        }
        13 => set_guard(h, 1),
        _ => {}
    }
    h.g.puzzles.mmarchd.reload = false;
}

/// The capture's reset (@ 0x004c6065 and @ 0x004c6848): the guard back on
/// his patrol, the house's memory cleared, `CtrlScenarIN` (the chapter's
/// capture section).
fn reset(h: &mut Hx, first: bool) {
    h.g.puzzles.mmarchd.caught = false;
    h.run("(playanimp=MMA_GARDE,1)");
    {
        let m = &mut h.g.puzzles.mmarchd;
        m.pending = false;
        m.stone = false;
        m.guard = 1;
        m.amb = 0;
        m.grumble = 0;
        m.voice_on = false;
        m.been_8 = false;
        if first {
            m.pending_restart = false;
        }
    }
    h.g.puzzles.freeze = false;
    h.set_flag(885, 1);
    h.set_flag(869, 0);
    h.set_flag(886, 0);
    h.set_flag(889, 0);
    h.ctrl();
}

/// The cursor's kind from the sprite pass's hit test (@ 0x0076e928: 2 on
/// an entity of kind 1 or 3, 1 on 2 or 4, 0 on none), at the centre.
fn cursor_kind(h: &Hx) -> i32 {
    let c = &h.g.camera;
    c.texel_a3(c.cx, c.cy)
        .and_then(|(f, u, v)| h.g.slots.hit(f, u, v))
        .map_or(0, |(_, k)| k)
}

/// Whether the guard's slot (`MMA_GARDE`, n) of this view has ended
/// (@ 0x0043f7f0 then byte +0x46d; a missing slot is −1 in the original,
/// which reads the byte before slot 0: false here).
fn guard_ended(h: &Hx, n: i32) -> bool {
    let id = h.perso(GARDE);
    h.slot_of(id, n).is_some_and(|k| h.g.slots.slots[k].ended)
}

/// The frame (@ 0x004c5dc0), with the zone record.
pub(super) fn frame(h: &mut Hx, z: &ZoneRec) {
    let vue = h.vue();
    let now = h.now();
    // sound: view 12: the ambience `AmbMMA.apc` started when it does not
    // play; other views: stopped (M4).
    // 2. Caught: reset, scan, nothing more this frame.
    if h.g.puzzles.mmarchd.caught {
        reset(h, true);
        return;
    }
    // 3. The delayed capture.
    if h.g.puzzles.mmarchd.pending_restart {
        h.g.puzzles.mmarchd.pending_start = now;
        h.g.puzzles.mmarchd.pending_restart = false;
    }
    if h.g.puzzles.mmarchd.pending && now - h.g.puzzles.mmarchd.pending_start > 1.0 {
        h.g.puzzles.mmarchd.pending = false;
        h.g.puzzles.freeze = false;
        let cmd = h.g.puzzles.mmarchd.pending_cmd.clone();
        h.run(&cmd);
        h.g.puzzles.mmarchd.caught = true;
        let n = h.g.puzzles.mmarchd.pending_flag;
        h.set_flag(n, 1);
    }
    // 4. Alerted while the cat is freed.
    if h.flag(119) == 1 && h.g.puzzles.mmarchd.guard == 5 {
        h.run("(playcutanimp=MMA_GARDE,1)");
        set_guard(h, 1);
        h.run("(AngleWarpXNow=290)");
        h.g.puzzles.mmarchd.caught = true;
        h.set_flag(880, 1);
    }
    // 5. The stone's flight: 40 frames at 15 Hz.
    if h.g.puzzles.mmarchd.flight {
        let f = ((now - h.g.puzzles.mmarchd.flight_start) * 15.0) as i32;
        h.g.puzzles.mmarchd.flight_frame = f;
        if f < 40 {
            // visual: the `PierreVoleur` sprite of frame f over the place
            // (@ 0x004362b0, 0x004367b0) (M4).
        } else {
            h.g.puzzles.mmarchd.flight = false;
            h.play_voice(189, 1, 255, 100, 100);
            h.g.puzzles.mmarchd.voice_on = true;
            h.g.puzzles.mmarchd.stone = true;
            h.run("(playcutanimp=MMA_GARDE,3)");
            set_guard(h, 2);
            h.g.puzzles.mmarchd.cam_lock = false;
            h.g.puzzles.freeze = false;
        }
    }
    // 6. View 11, the cat not freed: its voice when the cursor is on a
    // sprite of an entity of kind 2 or 4, once.
    if h.flag(119) == 0 && vue == 11 {
        if cursor_kind(h) == 1 {
            if !playing(h, 0x007869f8) && !h.g.puzzles.mmarchd.cat_seen {
                h.g.puzzles.mmarchd.cat_voice = true;
                h.play_voice(182, 1, 200, 200, 255);
            }
            h.g.puzzles.mmarchd.cat_seen = true;
        } else {
            h.g.puzzles.mmarchd.cat_seen = false;
        }
    }
    if h.g.puzzles.mmarchd.cat_voice && !playing(h, 0x007869f8) {
        h.g.puzzles.mmarchd.cat_voice = false;
    }
    // 7. The cat clicked (chapi028 `Flag879=1`).
    if h.flag(879) != 0 {
        h.set_flag(879, 0);
        h.set_flag(886, 1);
        if !playing(h, 0x007869f8) {
            h.set_flag(879, 0);
            let r = h.rand() % 60;
            let d = if r < 20 {
                181
            } else if r < 40 {
                191
            } else {
                184
            };
            h.play_voice(d, 1, 200, 200, 255);
        }
    }
    // 8. The throw: the stone in hand on `MMA_ZONE_PIERRE`.
    if z.is("MMA_ZONE_PIERRE") && h.hand() == h.obj("MMA_PIERRE") {
        h.run("(ObjetEnMain=0)");
        h.run("(SacADos=NOT_MMA_PIERRE)");
        h.g.puzzles.mmarchd.flight = true;
        h.run("(AngleWarpXNow=590)");
        h.run("(AngleWarpYNow=0)");
        h.g.puzzles.mmarchd.cam_lock = true;
        h.g.puzzles.mmarchd.flight_start = now;
        h.g.puzzles.freeze = true;
    }
    // 9. The guard's animations' ends (jump table @ 0x004c7200).
    let mut reset_now = false;
    match h.g.puzzles.mmarchd.guard {
        2 if guard_ended(h, 2) => {
            h.run("(playcutanimp=MMA_GARDE,4)");
            if vue == 1 {
                h.run("(AngleWarpXNow=250)");
                h.g.puzzles.mmarchd.caught = true;
                h.set_flag(880, 1);
            } else {
                h.play_voice(190, 1, 255, 100, 100);
                h.g.puzzles.mmarchd.voice_on = true;
            }
            set_guard(h, 3);
        }
        3 if guard_ended(h, 3) => {
            h.run("(playcutanimp=MMA_GARDE,5)");
            set_guard(h, 4);
            if vue == 2 {
                h.run("(AngleWarpXNow=215)");
                h.set_flag(882, 1);
                reset_now = true;
            }
        }
        4 if guard_ended(h, 4) => patrol(h),
        5 if guard_ended(h, 2) => {
            h.run("(AngleWarpXNow=290)");
            h.set_flag(880, 1);
            reset_now = true;
        }
        _ => {}
    }
    if reset_now || h.g.puzzles.mmarchd.caught {
        // @ 0x004c6848: the same reset, the frame goes on.
        reset(h, false);
    }
    // 10. Chapi028's capture 881 (`Flag883=1`): three voices in a row.
    if h.flag(883) != 0 {
        h.set_flag(883, 0);
        h.g.puzzles.mmarchd.voices_883 = 1;
        h.play_voice(188, 1, 200, 255, 255);
    }
    // Each step waits for the voice before (@ 0x004c6a4c: steps 1, 2, 3
    // fall into the next one's test).
    let step = h.g.puzzles.mmarchd.voices_883;
    if (1..=3).contains(&step) {
        if step == 1 && !playing(h, 0x00786968) {
            h.g.puzzles.mmarchd.voices_883 = 2;
            h.play_voice(188, 2, 255, 255, 200);
        }
        if step <= 2 && !playing(h, 0x00786968) {
            h.g.puzzles.mmarchd.voices_883 = 3;
            h.play_voice(188, 3, 200, 255, 255);
        }
        if !playing(h, 0x00786968) {
            h.g.puzzles.mmarchd.voices_883 = 0;
        }
    }
    // 11. The grumbling's ambience ended (or stopped: the cat freed).
    if h.g.puzzles.mmarchd.amb != 0 && h.flag(119) == 1 {
        // sound: the ambience stopped (@ 0x004335f0) (M4).
        h.g.puzzles.mmarchd.amb = 0;
    }
    if h.g.puzzles.mmarchd.amb != 0 && !playing(h, 0x00786a18) {
        h.g.puzzles.mmarchd.amb_start = now;
        let d = (h.rand() & 3) + 2;
        let m = &mut h.g.puzzles.mmarchd;
        m.amb = 0;
        m.amb_delay = d;
        if h.flag(119) == 0 {
            h.g.puzzles.mmarchd.amb_ended = true;
        }
    }
    if matches!(vue, 11 | 5 | 1 | 4) && h.flag(119) == 0 {
        let m = &h.g.puzzles.mmarchd;
        if m.amb == 0 && now - m.amb_start > f64::from(m.amb_delay) {
            let r = h.rand() & 1;
            h.g.puzzles.mmarchd.amb = r + 1;
            // sound: `Sound800.APC` (r = 0) or `Sound801.APC` (r = 1)
            // started (@ 0x004329e0(.., 0, 2), @ 0x00433140) (M4).
        }
        if h.g.puzzles.mmarchd.amb_ended && !h.g.puzzles.mmarchd.voice_on {
            let m = &h.g.puzzles.mmarchd;
            if now - m.pause_start > f64::from(m.pause) {
                grumble(h);
            }
        }
    }
    // The guard's voice ended: the pause before the next grumble.
    if h.g.puzzles.mmarchd.voice_on && !playing(h, 0x007869a8) {
        h.g.puzzles.mmarchd.pause_start = now;
        let p = (h.rand() & 3) + 6;
        h.g.puzzles.mmarchd.voice_on = false;
        h.g.puzzles.mmarchd.pause = p;
    }
    h.g.puzzles.mmarchd.amb_ended = false;
}

/// A grumble (@ 0x004c6f3e): the level up (1 → 2 → 3 when the cat has been
/// clicked, flag 886; back to 1 past 3), flag 869; level 3 alerts the
/// guard (G := 5, caught at the end of his anim 3).
fn grumble(h: &mut Hx) {
    h.g.puzzles.mmarchd.voice_on = true;
    let mut level = h.g.puzzles.mmarchd.grumble;
    if level != 0 {
        if h.flag(886) != 0 {
            level += 1;
        }
        if level >= 4 {
            level = 1;
        }
    } else {
        level = 1;
    }
    h.g.puzzles.mmarchd.grumble = level;
    h.set_flag(869, level);
    // sound: the guard's voice object stopped (@ 0x00443c90, 0x0040dcc0).
    match level {
        1 => h.play_voice(183, 1, 255, 100, 100),
        2 => h.play_voice(190, 1, 255, 100, 100),
        3 => {
            h.play_voice(189, 1, 255, 100, 100);
            set_guard(h, 5);
            h.run("(playcutanimp=MMA_GARDE,3)");
        }
        _ => log::error!("Puzzle_Garde : iGardeRaling={level}"),
    }
}
