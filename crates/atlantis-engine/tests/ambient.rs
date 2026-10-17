//! The places' ambient sounds against the original `atlantis.exe` (2001
//! English build).
//!
//! `FUN_00432178` / `FUN_00450ed8` (per frame) were run unmodified under
//! emulation for the gains golden; `FUN_00431a1c` / `FUN_004320bc` /
//! `FUN_00431e98` / `FUN_0043259c` / `FUN_00432758` were read in the
//! disassembly, and the state tests restate those rules. Ported from
//! ScummVM's `test/engines/cryomni3d/atlantis/audio_ambient.h`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::wam::{AmbientObject, AmbientRef, CentrePos, Place, Wam};
use atlantis_data::{Game, StdDir};
use atlantis_engine::ambient::{
    ambient_gains, miles_pan_balance, AmbientSampleOutput, AmbientSounds, MilesSampleStatus,
    EAR_ANGLE, SLOTS, SMP_DONE, SMP_PLAYING, SMP_STOPPED,
};

/// A recording sample output with a settable status per slot.
struct FakeOut {
    st: [i32; SLOTS],
    log: Vec<String>,
}

impl FakeOut {
    fn new() -> Self {
        FakeOut {
            st: [SMP_STOPPED; SLOTS],
            log: Vec::new(),
        }
    }
    fn has(&self, s: &str) -> bool {
        self.log.iter().any(|l| l == s)
    }
}

impl AmbientSampleOutput for FakeOut {
    fn status(&mut self, slot: usize) -> i32 {
        self.st[slot]
    }
    fn start(&mut self, slot: usize, object: usize, variant: usize, loop_forever: bool) {
        self.log.push(format!(
            "start {slot} {object} {variant} {}",
            u8::from(loop_forever)
        ));
        self.st[slot] = SMP_PLAYING;
    }
    fn stop(&mut self, slot: usize) {
        self.log.push(format!("stop {slot}"));
    }
    fn set_pan(&mut self, slot: usize, pan: u8) {
        self.log.push(format!("pan {slot} {pan}"));
    }
    fn set_volume(&mut self, slot: usize, v: u8) {
        self.log.push(format!("vol {slot} {v}"));
    }
    fn release(&mut self, object: usize) {
        self.log.push(format!("release {object}"));
    }
}

/// A sample output with the Miles statuses; `active`: the slot's sound is
/// still heard.
#[derive(Default)]
struct MilesOut {
    miles: MilesSampleStatus,
    active: [bool; SLOTS],
}

impl MilesOut {
    fn statuses(&mut self) -> [i32; SLOTS] {
        std::array::from_fn(|i| self.status(i))
    }
}

impl AmbientSampleOutput for MilesOut {
    fn status(&mut self, slot: usize) -> i32 {
        self.miles.status(slot, self.active[slot])
    }
    fn start(&mut self, slot: usize, _: usize, _: usize, _: bool) {
        self.miles.started(slot);
        self.active[slot] = true;
    }
    fn stop(&mut self, slot: usize) {
        self.miles.stopped(slot, self.active[slot]);
        self.active[slot] = false;
    }
    fn set_pan(&mut self, _: usize, _: u8) {}
    fn set_volume(&mut self, _: usize, _: u8) {}
    fn release(&mut self, _: usize) {}
}

/// A clock returning the given values, then 0.
fn clock(a: &mut AmbientSounds, values: &[u32]) {
    let v = Rc::new(RefCell::new(values.to_vec()));
    a.set_clock(move || {
        let mut v = v.borrow_mut();
        if v.is_empty() {
            0
        } else {
            v.remove(0)
        }
    });
}

fn object(id: u8, w: [u8; 4], variants: usize) -> AmbientObject {
    AmbientObject {
        id,
        pos: [100.0, 0.0, 0.0],
        loudness: 500.0,
        weights: w,
        variants: std::array::from_fn(|k| {
            if k < variants {
                format!("v{k}.apc")
            } else {
                String::new()
            }
        }),
        ..AmbientObject::default()
    }
}

fn place(id: u16, ids: &[u8]) -> Place {
    Place {
        id,
        ambient_refs: ids
            .iter()
            .map(|&object_id| AmbientRef {
                object_id,
                t: 0.0,
                m: 200.0,
                c: 0.0,
            })
            .collect(),
        ..Place::default()
    }
}

/// FUN_00450ed8 + FUN_00432178's pan / volume against the emulated
/// original (3006 cases, ear angle 0.65).
#[test]
fn gains_match_original() {
    let mut cases = 0;
    let mut bad = 0;
    for line in include_str!("golden/audio_ambient_gain.txt").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // "((x, y, z), (cx, cy, cz), (t, m, c), gain, alpha, beta) a b c26 pan vol"
        let v: Vec<f64> = line
            .split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | '+')))
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(v.len(), 17, "{line}");
        cases += 1;
        let pos = [v[0] as f32, v[1] as f32, v[2] as f32];
        let cam = CentrePos {
            x: v[3] as f32,
            y: v[4] as f32,
            z: v[5] as f32,
        };
        let r = AmbientRef {
            object_id: 1,
            t: v[6] as f32,
            m: v[7] as f32,
            c: v[8] as f32,
        };
        let g = ambient_gains(
            pos,
            v[9] as f32,
            &cam,
            &r,
            v[10] as f32,
            v[11] as f32,
            EAR_ANGLE,
        );
        let want = (
            v[12] as i32,
            v[13] as i32,
            v[14] as i32,
            v[15] as u8,
            v[16] as u8,
        );
        if (g.right, g.left, g.out26, g.pan, g.volume) != want {
            if bad < 5 {
                eprintln!("ambient gains differ: {line} / port {g:?}");
            }
            bad += 1;
        }
    }
    assert_eq!(cases, 3006);
    assert_eq!(bad, 0);
}

/// FUN_004320bc: draw = clock & 0x7f, redrawn above 100; the first variant
/// whose running sum exceeds it; none past the last; a weight above 100
/// counts as weight - 100 and sets the loop flag when scanned.
#[test]
fn pick_variant() {
    let mut a = AmbientSounds::new();
    clock(&mut a, &[0, 24, 25, 99, 100, 128 + 50, 101, 127, 30]);
    let o = object(1, [25, 25, 25, 25], 4);
    assert_eq!(a.pick_variant(&o, 4), Some(0)); // 0
    assert_eq!(a.pick_variant(&o, 4), Some(0)); // 24 < 25
    assert_eq!(a.pick_variant(&o, 4), Some(1)); // 25
    assert_eq!(a.pick_variant(&o, 4), Some(3)); // 99
    assert_eq!(a.pick_variant(&o, 4), None); // 100: no sum exceeds it
    assert_eq!(a.pick_variant(&o, 4), Some(2)); // & 0x7f -> 50
    assert_eq!(a.pick_variant(&o, 4), Some(1)); // 101, 127 redrawn, then 30
    assert!(!a.loop_flag);
    let lp = object(1, [199, 0, 0, 0], 1);
    clock(&mut a, &[100]);
    assert_eq!(a.pick_variant(&lp, 1), None); // 99 is not above 100
    assert!(a.loop_flag); // set even for a pause
}

/// FUN_00431a1c / FUN_00431f9c: records by ref index, the delay, the
/// pointer-identity "already loaded" test, the remove pass shifting the
/// records but not the handles; FUN_00431e98.
#[test]
fn enter_place_and_stop() {
    let mut a = AmbientSounds::new();
    let mut out = FakeOut::new();
    let objs = vec![
        object(1, [25, 25, 25, 25], 4), // delay (100 - 100) / 4 = 0
        object(2, [20, 20, 20, 20], 4), // delay 20 / 4 = 5
    ];
    assert!(a.enter_place(&mut out, &place(132, &[1, 2]), &objs, |_, _| true));
    assert_eq!(a.count(), 2);
    assert!(a.active());
    assert_eq!(a.records[0].object, Some(0));
    assert_eq!(a.records[1].object, Some(1));
    assert_eq!(a.records[0].delay, 0);
    assert_eq!(a.records[1].delay, 5);
    assert_eq!(a.records[1].count, 4);
    // The same objects in the other order: nothing reloaded, records kept.
    out.log.clear();
    assert!(a.enter_place(&mut out, &place(133, &[2, 1]), &objs, |_, _| true));
    assert!(out.log.is_empty());
    assert_eq!(a.records[0].object, Some(0));
    // Only object 2 left: record 0 removed, record 1 shifted to 0.
    out.log.clear();
    assert!(a.enter_place(&mut out, &place(134, &[2]), &objs, |_, _| true));
    assert_eq!(a.count(), 1);
    assert_eq!(a.records[0].object, Some(1));
    assert!(out.has("release 0"));
    assert!(out.has("pan 0 64"));
    // No references: FUN_00431e98.
    out.log.clear();
    assert!(a.enter_place(&mut out, &place(1, &[]), &objs, |_, _| true));
    assert_eq!(a.count(), 0);
    assert!(!a.active());
    assert!(out.has("release 1"));
}

/// FUN_00432178: a stopped sample starts a drawn variant and gets its pan
/// and volume; a pause arms CHAPTERTIME and ends when the difference
/// exceeds the delay; the loop flag makes the next start loop forever.
#[test]
fn frame() {
    let mut a = AmbientSounds::new();
    let mut out = FakeOut::new();
    let objs = vec![object(1, [20, 20, 20, 20], 4)]; // delay 5
    let p = place(135, &[1]);
    clock(&mut a, &[0]); // the load's discarded draw
    assert!(a.enter_place(&mut out, &p, &objs, |_, _| true));
    let cam = CentrePos::default();
    // Draw 90: no variant (sum 80) -> a pause from CHAPTERTIME 10.
    clock(&mut a, &[90]);
    out.log.clear();
    a.frame(&mut out, &p, &objs, &cam, 0.0, 0.0, 10);
    assert_eq!(a.records[0].start, 10);
    assert!(!out.has("pan 0 63"));
    // CHAPTERTIME 15: 15 - 10 = 5 is not above the delay 5.
    clock(&mut a, &[30]);
    out.log.clear();
    a.frame(&mut out, &p, &objs, &cam, 0.0, 0.0, 15);
    assert_eq!(a.records[0].start, 10);
    assert!(out.log.is_empty());
    // CHAPTERTIME 16: redraws until a variant (90 none, then 30 -> 1).
    clock(&mut a, &[90, 30]);
    a.frame(&mut out, &p, &objs, &cam, 0.0, 0.0, 16);
    assert_eq!(a.records[0].start, 0);
    assert!(out.has("start 0 0 1 0"));
    // Playing: only the positional update.
    out.log.clear();
    a.frame(&mut out, &p, &objs, &cam, 0.0, 0.0, 17);
    assert_eq!(out.log.len(), 2);
    // A stale loop flag makes the next start loop forever.
    a.loop_flag = true;
    out.st[0] = SMP_DONE;
    clock(&mut a, &[0]);
    out.log.clear();
    a.frame(&mut out, &p, &objs, &cam, 0.0, 0.0, 18);
    assert!(out.has("start 0 0 0 1"));
    assert!(!a.loop_flag);
}

/// FUN_0043259c / FUN_00432758 (static reading): steps = (gain at the
/// destination - last value) / frames; each video frame adds them and sets
/// the truncated values; a finished sample (status 2 only) with more than
/// one variant restarts, keeping a finite loop count.
#[test]
fn video_glide() {
    let mut a = AmbientSounds::new();
    let mut out = FakeOut::new();
    let objs = vec![object(1, [50, 50, 0, 0], 2)];
    let p = place(135, &[1]);
    clock(&mut a, &[0]);
    assert!(a.enter_place(&mut out, &p, &objs, |_, _| true));
    out.st[0] = SMP_PLAYING;
    a.cur_pan[0] = 10.0;
    a.cur_vol[0] = 0.0;
    let dst = CentrePos {
        x: 0.0,
        y: 300.0,
        z: 0.0,
    };
    let o = &objs[0];
    let pos = [-o.pos[1], o.pos[0], -o.pos[2]];
    let g = ambient_gains(
        pos,
        o.loudness,
        &dst,
        &p.ambient_refs[0],
        0.5,
        0.0,
        EAR_ANGLE,
    );
    a.prepare_glide(&p, &objs, &dst, 0.5, 0.0, 4);
    assert_eq!(a.step_pan[0], ((f64::from(g.pan) - 10.0) / 4.0) as f32);
    assert_eq!(a.step_vol[0], (f64::from(g.volume) / 4.0) as f32);
    for _ in 1..=4 {
        out.log.clear();
        a.video_frame(&mut out, &p, &objs, 100);
        assert_eq!(out.log.len(), 2);
    }
    assert!(out.has(&format!("pan 0 {}", a.cur_pan[0] as i32)));
    assert!((a.cur_pan[0] - f32::from(g.pan)).abs() < 1e-4);
    assert!((a.cur_vol[0] - f32::from(g.volume)).abs() < 1e-4);
    // Status 8 (stopped) does not restart here.
    out.st[0] = SMP_STOPPED;
    out.log.clear();
    a.video_frame(&mut out, &p, &objs, 100);
    assert_eq!(out.log.len(), 2);
    // Status 2 restarts a drawn variant with a finite loop count.
    out.st[0] = SMP_DONE;
    a.loop_flag = true;
    clock(&mut a, &[60]);
    out.log.clear();
    a.video_frame(&mut out, &p, &objs, 100);
    assert!(out.has("start 0 0 1 0"));
    assert!(a.loop_flag); // FUN_00432758 neither reads nor clears it
}

/// The six Miles samples shared by the ambient sounds and the puzzles'
/// players, against runs of the original under Wine and the players'
/// disassembly (FUN_0043ca28, FUN_0043cd60, FUN_0043ce80).
#[test]
fn miles_sample_pools() {
    type M = MilesSampleStatus;
    // Statuses: 2 at start-up; 4 while playing, 2 once done; a stop makes
    // a playing sample 8 and leaves 2 or 8 as they are.
    let mut out = MilesOut::default();
    assert_eq!(out.statuses(), [SMP_DONE; SLOTS]);
    out.start(0, 0, 0, false);
    assert_eq!(out.status(0), SMP_PLAYING);
    out.active[0] = false; // the sound ended
    assert_eq!(out.status(0), SMP_DONE);
    out.stop(0);
    assert_eq!(out.status(0), SMP_DONE);
    out.start(0, 0, 0, false);
    out.stop(0);
    assert_eq!(out.status(0), SMP_STOPPED);
    out.stop(0);
    assert_eq!(out.status(0), SMP_STOPPED);
    out.start(0, 0, 0, false); // only a start changes 8
    assert_eq!(out.status(0), SMP_PLAYING);

    // Muria (two ambient sounds on samples 0, 1): both playing at the
    // puzzle entry, FUN_00431e98 leaves them at 8 and every short effect
    // is dropped.
    let mut a = AmbientSounds::new();
    let mut amb = MilesOut::default();
    let objs = vec![
        object(1, [25, 25, 25, 25], 4),
        object(2, [25, 25, 25, 25], 4),
    ];
    assert!(a.enter_place(&mut amb, &place(1, &[1, 2]), &objs, |_, _| true));
    amb.start(0, 0, 0, false);
    amb.start(1, 1, 0, false);
    a.stop_all(&mut amb);
    let st = amb.statuses();
    assert_eq!((st[0], st[1]), (SMP_STOPPED, SMP_STOPPED));
    assert_eq!(M::slot_ca28(&st, 6464), None);
    assert_eq!(M::slot_ca28(&st, 12036), None);
    // Second puzzle: only sample 0 was playing; sample 1 is used.
    assert!(a.enter_place(&mut amb, &place(1, &[1, 2]), &objs, |_, _| true));
    amb.start(0, 0, 0, false);
    amb.start(1, 1, 0, false);
    amb.active[1] = false; // sample 1 had finished
    a.stop_all(&mut amb);
    let st = amb.statuses();
    assert_eq!((st[0], st[1]), (SMP_STOPPED, SMP_DONE));
    assert_eq!(M::slot_ca28(&st, 6464), Some(1));
    // The ambient player restarts 8 at the place (FUN_00432178: 2 or 8).
    assert!(a.enter_place(&mut amb, &place(1, &[1, 2]), &objs, |_, _| true));
    a.frame(
        &mut amb,
        &place(1, &[1, 2]),
        &objs,
        &CentrePos::default(),
        0.0,
        0.0,
        0,
    );
    assert_eq!(amb.status(0), SMP_PLAYING);
    assert_eq!(amb.status(1), SMP_PLAYING);

    // The hens (no ambient sound): POULE2 still playing on sample 2 at the
    // exit, FUN_0043d0e0 makes it 8, and the next run's long sounds are
    // dropped.
    let mut hens = MilesOut::default();
    assert_eq!(M::slot_ca28(&hens.statuses(), 22230), Some(2));
    hens.start(2, 0, 0, false);
    for i in 0..4 {
        hens.stop(i); // FUN_0043d0e0
    }
    let st = hens.statuses();
    assert_eq!((st[2], st[0]), (SMP_STOPPED, SMP_DONE));
    assert_eq!(M::slot_ca28(&st, 22230), None);
    assert_eq!(M::slot_ca28(&st, 41150), None);
    assert_eq!(M::slot_ca28(&st, 19999), Some(0)); // under 0x4e20: samples 0 / 1

    // FUN_0043cd60: refuses over 120000 bytes, else samples 0..2 in status 2.
    let s822 = [8, 2, 2, 2, 2, 2];
    assert_eq!(M::slot_cd60(&s822, 120001), None);
    assert_eq!(M::slot_cd60(&s822, 120000), Some(1));
    let s884 = [8, 8, 4, 2, 2, 2];
    assert_eq!(M::slot_cd60(&s884, 100), None);
    // FUN_0043ce80: sample 0 when 2 or 8, never another.
    assert_eq!(M::slot_ce80(&s884, 100), Some(0));
    let s42 = [4, 2, 2, 2, 2, 2];
    assert_eq!(M::slot_ce80(&s42, 100), None);
    assert_eq!(M::slot_ca28(&s42, 100), Some(1));
    assert_eq!(M::slot_ca28(&s42, 0), None); // an empty record
}

/// MSS32.DLL's DirectSound pan (0x2000dd50, helper 0x2000dcd0): SetPan of
/// trunc(2000 log10(p / 63)) (p < 63), its mirror with 127 - p (p > 64),
/// 0 for 63 / 64, -10000 for 0. Expected balances: 127 x the far channel's
/// amplitude 10^(cB / 2000), to the nearest step, computed outside the
/// engine.
#[test]
fn miles_pan_balance_law() {
    #[rustfmt::skip]
    let want: [i8; 128] = [
        -127, -125, -123, -121, -119, -117, -115, -113, -111, -109, -107, -105, -103, -101, -99, -97,
        -95, -93, -91, -89, -87, -85, -83, -81, -79, -77, -75, -73, -71, -68, -66, -64,
        -62, -60, -58, -56, -54, -52, -50, -48, -46, -44, -42, -40, -38, -36, -34, -32,
        -30, -28, -26, -24, -22, -20, -18, -16, -14, -12, -10, -8, -6, -4, -2, 0,
        0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30,
        32, 34, 36, 38, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 62,
        64, 66, 68, 71, 73, 75, 77, 79, 81, 83, 85, 87, 89, 91, 93, 95,
        97, 99, 101, 103, 105, 107, 109, 111, 113, 115, 117, 119, 121, 123, 125, 127,
    ];
    for (p, &w) in want.iter().enumerate() {
        assert_eq!(miles_pan_balance(p as u8), w, "pan {p}");
    }
}

/// CHAPI038 /551 (departure from place 145 of the night hut PECHEUR2.WAM)
/// runs ChangeWam=pecheur1.wam, Vue=145 and a cinema; nothing in between
/// calls FUN_00431e98, so the next FUN_00431a1c sees the night records. Its
/// reuse test is pointer identity on the object table, which the new WAM
/// refills slot for slot: the day place's objects 1 and 2 count as loaded,
/// and the night waves go on playing.
#[test]
fn changewam_keeps_samples() {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: ChangeWam ambient test skipped");
        return;
    };
    let game = Game::open(Arc::new(StdDir::new(dir))).unwrap();
    let night = Wam::parse(&game.big.read("WAM\\PECHEUR2.WAM").unwrap());
    let day = Wam::parse(&game.big.read("WAM\\PECHEUR1.WAM").unwrap());
    let (p2, p1) = (night.find_place(145).unwrap(), day.find_place(145).unwrap());
    let mut a = AmbientSounds::new();
    let mut out = FakeOut::new();
    let mut names = Vec::new();
    assert!(a.enter_place(&mut out, p2, &night.ambient_objects, |o, k| {
        names.push(night.ambient_objects[o].variants[k].clone());
        true
    }));
    assert_eq!(a.count(), 2);
    assert_eq!(names.len(), 8);
    assert_eq!(names[0], "vague_07.apc"); // mernuit
    assert_eq!(names[4], "vague_01.apc"); // mernuit2
                                          // The day WAM names other waves for the same slots.
    assert_eq!(day.ambient_objects[0].variants[0], "vague_01.apc"); // mer1
    assert_eq!(day.ambient_objects[1].variants[0], "vague_07.apc"); // mer2
    names.clear();
    out.log.clear();
    assert!(a.enter_place(&mut out, p1, &day.ambient_objects, |o, k| {
        names.push(day.ambient_objects[o].variants[k].clone());
        true
    }));
    assert!(names.is_empty()); // nothing reloaded
    assert!(out.log.is_empty()); // nothing stopped or released
    assert_eq!(a.count(), 2);
    assert_eq!(a.records[0].object, Some(0));
    assert_eq!(a.records[1].object, Some(1));
    assert_eq!(a.records[1].delay, 5); // mernuit2's 20 x 4, not mer2's 25 x 4
}
