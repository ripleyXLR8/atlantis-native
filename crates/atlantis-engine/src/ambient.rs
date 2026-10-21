//! The places' ambient sounds (the sea at the fisherman's, the palace
//! fountain, the forest waterfall...): the WAM's 3D ambient objects, loaded
//! on place entry by `FUN_00431a1c` and updated every frame by
//! `FUN_00432178`, on the six Miles samples of the original.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (`ambient_sounds.cpp`, `ambient_output.cpp`, `miles_samples.h`; GPLv3,
//! the ScummVM team).
//!
//! The x87 runs at 53 bits (control word 0x127f): `f64` arithmetic with a
//! rounding to `f32` wherever the original stores a float.

use std::collections::HashMap;
use std::sync::Arc;

use atlantis_data::wam::{AmbientObject, AmbientRef, CentrePos, Place};

use crate::audio::{Mixer, Pcm, VoiceId};

fn f(x: f64) -> f64 {
    f64::from(x as f32)
}

/// `FUN_00451d6e` then FISTP: truncation; out of the i32 range the x87
/// stores the indefinite value 0x80000000.
fn trunc_int(v: f64) -> i32 {
    if !(v > -2_147_483_649.0 && v < 2_147_483_648.0) {
        return i32::MIN;
    }
    v as i32
}

/// pan = a * 127 / (a + b) in 32 bits as the original (ADD / IMUL / CDQ /
/// IDIV, `FUN_00432178` @ 0x004324aa..0x004324bb, `FUN_0043259c` @
/// 0x004326b9..0x004326ca): the sum and the product wrap. Only a gain
/// outside 0..255 (the indefinite 0x80000000 of [`trunc_int`], an object at
/// the camera) is affected. A sum of -1 with a product of `i32::MIN` is a
/// divide error in the original and gives the product here.
fn ambient_pan32(a: i32, b: i32) -> i32 {
    a.wrapping_mul(127).wrapping_div(a.wrapping_add(b))
}

/// One glide step of `FUN_0043259c`: (target - last) / frames, as fild
/// int32, fsub float, fild int32 0x00896c50, fdivp, fstp float at the
/// game's 53-bit x87 precision (@ 0x004326cf..0x004326ec pan,
/// 0x00432724..0x00432741 volume).
pub fn ambient_glide_step(target: i32, last: f32, frames: i32) -> f32 {
    ((f64::from(target) - f64::from(last)) / f64::from(frames)) as f32
}

/// `FUN_00450ed8` and `FUN_00432178`'s pan / volume for one object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmbientGains {
    /// Object +0x1e.
    pub right: i32,
    /// Object +0x22, after `FUN_00432178`'s "a + b == 0" fix.
    pub left: i32,
    /// Object +0x26 = trunc(ref c).
    pub out26: i32,
    /// AIL_set_sample_pan.
    pub pan: u8,
    /// AIL_set_sample_volume.
    pub volume: u8,
}

/// The ear angle, float 0x00497c90 (0.65 once a WAM is loaded).
pub const EAR_ANGLE: f32 = 0.65;

/// `FUN_00450ed8` + `FUN_00432178` @ 0x0043241e..0x00432581. `pos`: the
/// object's position after the load rotation (x' = -y, y' = x, z' = -z);
/// `cam`: the place's centre (`FUN_004315b4`); `alpha`, `beta`: the view
/// angles (0x00bcb59c, 0x00bcb598).
pub fn ambient_gains(
    pos: [f32; 3],
    loudness: f32,
    cam: &CentrePos,
    r: &AmbientRef,
    alpha: f32,
    beta: f32,
    ear: f32,
) -> AmbientGains {
    let dx = f(f64::from(pos[0]) - f64::from(cam.x));
    let dy = f(f64::from(pos[1]) - f64::from(cam.y));
    let dz = f(f64::from(pos[2]) - f64::from(cam.z));
    let dist = f((dx * dx + dy * dy + dz * dz).sqrt());
    let (a, b) = (f64::from(alpha), f64::from(beta));
    let cb = f(libm::cos(b));
    let f0 = f(libm::cos(a) * cb);
    let f1 = f(libm::sin(a) * cb);
    let sbx = libm::sin(-b);
    let sb = f(sbx);
    let k = f(1.0 / (1.0 - sbx * sb).sqrt());
    let g = f(-f1 * k);
    let h = f(f0 * k);
    let q = f64::from(ear);
    let s = f(libm::sin(q));
    let c = f(libm::cos(q));
    let a18 = f(f0 * c);
    let a1c = f(g * s);
    let a28 = f(f1 * c);
    let a2c = f(h * s);
    let lx = f(a18 - a1c);
    let ly = f(a28 - a2c);
    let z = f(sb * c);
    let rx = f(a18 + a1c);
    let ry = f(a28 + a2c);
    let zz = f(z * dz);
    let gl = f((lx * dx + ly * dy + zz) / (dist * 2.0) + 0.5); // 0x00bcab64
    let gr = f((rx * dx + ry * dy + zz) / (dist * 2.0) + 0.5); // 0x00bcab60
    let m = f(f64::from(r.m) * f64::from(loudness));
    let t = f64::from(r.t);
    let gain = |x: f64| {
        let v = trunc_int(((1.0 - t) * x + t) * m / dist);
        // A signed compare: a negative passes.
        if v >= 256 {
            255
        } else {
            v
        }
    };
    let mut out = AmbientGains {
        right: gain(gl),
        left: gain(gr),
        out26: trunc_int(f64::from(r.c)),
        pan: 0,
        volume: 0,
    };
    if out.right.wrapping_add(out.left) == 0 {
        out.left = 1;
    }
    let pan = ambient_pan32(out.right, out.left);
    let vol = out.right.max(out.left) / 2;
    out.pan = (pan & 0xff) as u8;
    out.volume = (vol & 0xff) as u8;
    out
}

/// AIL_sample_status values.
pub const SMP_DONE: i32 = 2;
pub const SMP_PLAYING: i32 = 4;
pub const SMP_STOPPED: i32 = 8;

/// The six Miles sample handles of the original (0x006cc560..0x006cc574).
pub trait AmbientSampleOutput {
    fn status(&mut self, slot: usize) -> i32;
    /// AIL_set_sample_address + loop count + AIL_start_sample: variant k of
    /// object `object` (an index in the WAM's object table).
    fn start(&mut self, slot: usize, object: usize, variant: usize, loop_forever: bool);
    fn stop(&mut self, slot: usize);
    fn set_pan(&mut self, slot: usize, pan: u8);
    fn set_volume(&mut self, slot: usize, volume: u8);
    /// The record's buffers are freed (`FUN_00431a1c` remove pass,
    /// `FUN_00431e98`).
    fn release(&mut self, object: usize);
}

/// Number of sample slots.
pub const SLOTS: usize = 6;

/// A 0x2e-byte record of 0x006120f0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// +0.
    pub count: u8,
    /// +1.
    pub current: u8,
    /// +2: CHAPTERTIME seconds.
    pub delay: u32,
    /// +6: pause start, 0 = none.
    pub start: u32,
    /// +0xa: the object (an index in the WAM table), `None` = none.
    pub object: Option<usize>,
}

/// The ambient sound state of `FUN_00431a1c` / `FUN_00432178` /
/// `FUN_0043259c` / `FUN_00432758` / `FUN_00431e98`.
pub struct AmbientSounds {
    pub records: [Record; SLOTS + 1],
    /// 0x00495e8c.
    pub loop_flag: bool,
    /// 0x00495e73 (0..127): the volume `FUN_00431e98` gives every sample
    /// before stopping it.
    pub effects_volume: u8,
    /// The last pan / volume set, as floats (0x006cc5d0 / 0x006cc548), and
    /// the video steps (0x006cc530 / 0x006cc5b8); nothing clears them.
    pub cur_pan: [f32; SLOTS],
    pub cur_vol: [f32; SLOTS],
    pub step_pan: [f32; SLOTS],
    pub step_vol: [f32; SLOTS],
    /// 0x00772911.
    count: usize,
    /// 0x00496980.
    active: bool,
    /// The milliseconds clock `FUN_004320bc` draws from (timeGetTime -
    /// base).
    clock: Box<dyn FnMut() -> u32>,
}

impl Default for AmbientSounds {
    fn default() -> Self {
        Self::new()
    }
}

impl AmbientSounds {
    pub fn new() -> Self {
        AmbientSounds {
            records: [Record::default(); SLOTS + 1],
            loop_flag: false,
            effects_volume: 0x6e,
            cur_pan: [0.0; SLOTS],
            cur_vol: [0.0; SLOTS],
            step_pan: [0.0; SLOTS],
            step_vol: [0.0; SLOTS],
            count: 0,
            active: false,
            clock: Box::new(|| 0),
        }
    }

    pub fn set_clock(&mut self, clock: impl FnMut() -> u32 + 'static) {
        self.clock = Box::new(clock);
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// `FUN_0043187c`: the object with the ref's id; the last match wins.
    fn object_of(r: &AmbientRef, objects: &[AmbientObject]) -> Option<usize> {
        if r.object_id == 0xff {
            return None;
        }
        objects.iter().rposition(|o| o.id == r.object_id)
    }

    /// `FUN_004320bc`: a draw of the clock & 0x7f, redrawn while above 100,
    /// then the first variant whose running weight sum exceeds it; `None`
    /// for a pause. A weight above 100 counts as weight - 100 and sets
    /// 0x00495e8c (loop forever) as soon as it is scanned.
    pub fn pick_variant(&mut self, obj: &AmbientObject, count: usize) -> Option<usize> {
        let r = loop {
            let r = (self.clock)() & 0x7f;
            if r <= 100 {
                break r;
            }
        };
        let mut sum = 0u32;
        for k in 0..count.min(4) {
            let mut p = u32::from(obj.weights[k]);
            if p > 100 {
                p -= 100;
                self.loop_flag = true;
            }
            sum += p;
            if sum > r {
                return Some(k);
            }
        }
        None
    }

    /// `FUN_00431a1c` on entering `place` of `objects` (the WAM's table).
    /// False when a variant cannot be loaded (`can_load` says so).
    pub fn enter_place(
        &mut self,
        out: &mut dyn AmbientSampleOutput,
        place: &Place,
        objects: &[AmbientObject],
        mut can_load: impl FnMut(usize, usize) -> bool,
    ) -> bool {
        let n = place.ambient_refs.len().min(SLOTS);
        if n == 0 {
            if self.active {
                self.stop_all(out);
            }
            return true;
        }
        // The add pass: "already loaded" is pointer identity, i.e. the same
        // slot of the static object table.
        for i in 0..n {
            let Some(obj) = Self::object_of(&place.ambient_refs[i], objects) else {
                log::warn!(
                    "ambient: reference {i} of place {} names no object",
                    place.id
                );
                continue;
            };
            if self.records[..self.count]
                .iter()
                .any(|r| r.object == Some(obj))
            {
                continue;
            }
            // FUN_00431f9c(i), into record i (the ref index, not the count).
            let o = &objects[obj];
            let count = o.variants.iter().filter(|v| !v.is_empty()).count();
            let sum: i32 = o.weights[..count]
                .iter()
                .map(|&w| i32::from(if w > 100 { w - 100 } else { w }))
                .sum();
            self.records[i].count = count as u8;
            self.records[i].delay = ((100 - sum) / 4) as u32;
            let _ = self.pick_variant(o, count); // discarded; may set 0x00495e8c
            self.records[i].object = Some(obj);
            for k in 0..count {
                if !can_load(obj, k) {
                    return false;
                }
                self.records[i].start = 0;
                out.stop(i); // loop count 1, AIL_stop_sample
            }
            self.active = true;
            self.count += 1;
        }
        // The remove pass: records whose object is no longer referenced; the
        // records shift down, the handles do not.
        let mut r = 0;
        while r < self.count {
            let referenced = place.ambient_refs[..n]
                .iter()
                .any(|a| Self::object_of(a, objects) == self.records[r].object);
            if referenced {
                r += 1;
                continue;
            }
            out.set_pan(r, 0x40);
            out.set_volume(r, 0);
            out.stop(r);
            if let Some(o) = self.records[r].object {
                out.release(o);
            }
            let end = self.count.min(SLOTS);
            for j in r..end {
                self.records[j] = self.records[j + 1];
            }
            self.count -= 1;
        }
        true
    }

    fn start_variant(&mut self, out: &mut dyn AmbientSampleOutput, i: usize, v: usize) {
        self.records[i].current = v as u8;
        out.stop(i);
        let loop_forever = std::mem::take(&mut self.loop_flag);
        if let Some(o) = self.records[i].object {
            out.start(i, o, v, loop_forever);
        }
    }

    /// Redraws until a variant (the original loops until one comes).
    fn pick_some(&mut self, obj: &AmbientObject, count: usize) -> Option<usize> {
        (0..100_000).find_map(|_| self.pick_variant(obj, count))
    }

    /// `FUN_00432178`'s per-sample loop for the current place.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        out: &mut dyn AmbientSampleOutput,
        place: &Place,
        objects: &[AmbientObject],
        cam: &CentrePos,
        alpha: f32,
        beta: f32,
        chapter_time: i32,
    ) {
        // @ 0x0043219e: only with references and 0x00496980 == 1
        if place.ambient_refs.is_empty() || !self.active {
            return;
        }
        for i in 0..self.count {
            let Some(r) = place.ambient_refs.get(i) else {
                break;
            };
            let Some(ro) = Self::object_of(r, objects) else {
                continue;
            };
            let o = &objects[ro];
            let status = out.status(i);
            if self.records[i].start != 0 {
                // A pause: over when CHAPTERTIME - start > delay (unsigned).
                let rec = self.records[i];
                if (chapter_time.wrapping_sub(rec.start as i32) as u32) > rec.delay {
                    if let Some(v) = self.pick_some(o, usize::from(rec.count)) {
                        self.records[i].current = v as u8;
                        out.stop(i);
                        if let Some(obj) = rec.object {
                            out.start(i, obj, v, false); // loop count 1
                        }
                        self.records[i].start = 0;
                    }
                }
                continue;
            }
            if (status == SMP_DONE || status == SMP_STOPPED) && self.records[i].count > 0 {
                match self.pick_variant(o, usize::from(self.records[i].count)) {
                    None => {
                        if self.records[i].start == 0 {
                            self.records[i].start = chapter_time as u32;
                        }
                        out.stop(i);
                        continue;
                    }
                    Some(v) => self.start_variant(out, i, v),
                }
            }
            // The positional update (FUN_00450ed8 with the rotated position).
            let pos = [-o.pos[1], o.pos[0], -o.pos[2]];
            let g = ambient_gains(pos, o.loudness, cam, r, alpha, beta, EAR_ANGLE);
            self.cur_pan[i] = f32::from(g.pan);
            self.cur_vol[i] = f32::from(g.volume);
            out.set_pan(i, g.pan);
            out.set_volume(i, g.volume);
        }
    }

    /// `FUN_0043259c` (0x0043259c..0x00432754), before a move video: for
    /// each ambient sound of the place being left, the per-frame pan /
    /// volume steps from the last values toward `FUN_00450ed8`'s gains at
    /// the destination's camera `cam` (with the current angles), divided by
    /// the video's frame count.
    pub fn prepare_glide(
        &mut self,
        place: &Place,
        objects: &[AmbientObject],
        cam: &CentrePos,
        alpha: f32,
        beta: f32,
        frames: i32,
    ) {
        let n = place.ambient_refs.len().min(SLOTS);
        for i in 0..n {
            let r = &place.ambient_refs[i];
            let Some(ro) = Self::object_of(r, objects) else {
                continue;
            };
            let o = &objects[ro];
            let pos = [-o.pos[1], o.pos[0], -o.pos[2]];
            let g = ambient_gains(pos, o.loudness, cam, r, alpha, beta, EAR_ANGLE);
            let pan = ambient_pan32(g.right, g.left);
            let vol = g.right.max(g.left) / 2;
            self.step_pan[i] = ambient_glide_step(pan, self.cur_pan[i], frames);
            self.step_vol[i] = ambient_glide_step(vol, self.cur_vol[i], frames);
        }
    }

    /// `FUN_00432758` (0x00432758..0x00432a60), once per frame of any video
    /// (`FUN_0041bc74`): restarts a finished multi-variant sample, then adds
    /// the steps and sets pan and volume.
    pub fn video_frame(
        &mut self,
        out: &mut dyn AmbientSampleOutput,
        place: &Place,
        objects: &[AmbientObject],
        chapter_time: i32,
    ) {
        let n = place.ambient_refs.len().min(SLOTS);
        for i in 0..n {
            let ro = Self::object_of(&place.ambient_refs[i], objects);
            let status = out.status(i);
            let rec = self.records[i];
            if rec.start != 0 {
                // The pause, as in FUN_00432178; the sample starts with the
                // loop count it already has.
                if let Some(ro) = ro {
                    if (chapter_time.wrapping_sub(rec.start as i32) as u32) > rec.delay {
                        if let Some(v) = self.pick_some(&objects[ro], usize::from(rec.count)) {
                            self.records[i].current = v as u8;
                            out.stop(i);
                            if let Some(obj) = rec.object {
                                out.start(i, obj, v, false);
                            }
                            self.records[i].start = 0;
                        }
                    }
                }
                continue;
            }
            // Status 2 only (FUN_00432178 also takes 8) and more than one
            // variant.
            if let (SMP_DONE, true, Some(ro)) = (status, rec.count > 1, ro) {
                match self.pick_variant(&objects[ro], usize::from(rec.count)) {
                    None => {
                        self.records[i].start = chapter_time as u32; // 0x004bc324
                        out.stop(i);
                        continue; // no step this frame
                    }
                    Some(v) => {
                        // A finished sample had a finite loop count (1),
                        // which no call here changes.
                        self.records[i].current = v as u8;
                        out.stop(i);
                        if let Some(obj) = rec.object {
                            out.start(i, obj, v, false);
                        }
                    }
                }
            }
            self.cur_pan[i] += self.step_pan[i];
            self.cur_vol[i] += self.step_vol[i];
            // FUN_00451d6e truncates (RC = 11), then & 0xff.
            out.set_pan(i, (self.cur_pan[i] as i32 & 0xff) as u8);
            out.set_volume(i, (self.cur_vol[i] as i32 & 0xff) as u8);
        }
    }

    /// `FUN_00431e98`.
    pub fn stop_all(&mut self, out: &mut dyn AmbientSampleOutput) {
        for i in 0..self.count {
            out.set_pan(i, 0x40);
            out.set_volume(i, self.effects_volume); // [0x00495e73] @ 0x00431eec
            out.stop(i);
            if let Some(o) = self.records[i].object {
                out.release(o);
            }
            self.records[i].object = None;
            self.records[i].count = 0;
        }
        self.count = 0;
        self.active = false;
    }
}

/// The statuses of the six Miles sample handles 0x006cc560..0x006cc574.
/// AIL_sample_status gives 2 (SMP_DONE) for a sample that finished or was
/// never started, 4 (SMP_PLAYING) while it plays, and 8 (SMP_STOPPED) after
/// AIL_stop_sample stopped it while playing; AIL_stop_sample leaves a done
/// (2) or stopped (8) sample as it is. Only a start changes 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MilesSampleStatus {
    pub state: [u8; SLOTS],
}

impl Default for MilesSampleStatus {
    fn default() -> Self {
        MilesSampleStatus {
            state: [SMP_DONE as u8; SLOTS],
        }
    }
}

impl MilesSampleStatus {
    /// AIL_sample_status; `active`: the slot's sound is still playing.
    pub fn status(&self, i: usize, active: bool) -> i32 {
        if i32::from(self.state[i]) == SMP_PLAYING && !active {
            SMP_DONE
        } else {
            i32::from(self.state[i])
        }
    }

    /// AIL_start_sample.
    pub fn started(&mut self, i: usize) {
        self.state[i] = SMP_PLAYING as u8;
    }

    /// AIL_stop_sample.
    pub fn stopped(&mut self, i: usize, active: bool) {
        self.state[i] = if self.status(i, active) == SMP_PLAYING {
            SMP_STOPPED as u8
        } else {
            self.status(i, active) as u8
        };
    }

    /// `FUN_0043ca28` (0x0043ca7e..0x0043cb79): the slot a puzzle effect of
    /// `bytes` decoded bytes starts on, or `None` when it is dropped: under
    /// 20000 (0x4e20) bytes, the first of samples 0 and 1 in status 2;
    /// otherwise sample 2 in status 2.
    pub fn slot_ca28(st: &[i32; SLOTS], bytes: i32) -> Option<usize> {
        if bytes <= 0 {
            return None;
        }
        if bytes < 0x4e20 {
            return (0..2).find(|&i| st[i] == SMP_DONE);
        }
        (st[2] == SMP_DONE).then_some(2)
    }

    /// `FUN_0043cd60` (0x0043cdb6..0x0043ce6a): over 120000 (0x1d4c0) bytes
    /// nothing; else the first of samples 0..2 in status 2.
    pub fn slot_cd60(st: &[i32; SLOTS], bytes: i32) -> Option<usize> {
        if bytes <= 0 || bytes > 0x1d4c0 {
            return None;
        }
        (0..3).find(|&i| st[i] == SMP_DONE)
    }

    /// `FUN_0043ce80` (0x0043ced3..0x0043ceed): sample 0 in status 2 or 8.
    pub fn slot_ce80(st: &[i32; SLOTS], bytes: i32) -> Option<usize> {
        if bytes <= 0 {
            return None;
        }
        (st[0] == SMP_DONE || st[0] == SMP_STOPPED).then_some(0)
    }
}

/// Miles pan 0 (left) .. 127 (right) onto the mixer's balance, as the
/// DirectSound back-end of the shipped MSS32.DLL plays it: 0x2000dd50
/// calls IDirectSoundBuffer::SetPan with trunc(2000 log10(p / 63)) for
/// p < 63, -trunc(2000 log10((127 - p) / 63)) for p > 64, 0 for 63 and 64,
/// -10000 for p = 0 (helper 0x2000dcd0). The near channel stays full and
/// the far one gets 10^(cB / 2000); the mixer's far channel is
/// (127 - |balance|) / 127, so the balance is the nearest step to that.
pub fn miles_pan_balance(pan: u8) -> i8 {
    let p = i32::from(pan & 0x7f);
    if p == 63 || p == 64 {
        return 0;
    }
    let q = if p < 63 { p } else { 127 - p }; // the far channel's numerator
    let cb = if q == 0 {
        -10000.0
    } else {
        (2000.0 * libm::log10(f64::from(q) / 63.0)).trunc()
    };
    let far = libm::floor(127.0 * libm::pow(10.0, cb / 2000.0) + 0.5) as i32;
    (if p < 63 { far - 127 } else { 127 - far }) as i8
}

/// Miles volume 0..127 onto the mixer's 0..255, linear in amplitude as in
/// MSS32.DLL (0x2000dc60).
fn mixer_volume(v: u8) -> u8 {
    (u32::from(v & 0x7f) * 255 / 127) as u8
}

/// The rate the six samples are set up with: `FUN_00430d20` sets them up
/// as 16-bit mono at 22050 Hz and nothing changes that, so every variant
/// plays at 22050 Hz whatever its header says (two 16000 Hz waves are
/// heard 1.38 times fast in the original too).
pub const SAMPLE_RATE: u32 = 22050;

/// The six Miles samples on the engine's [`Mixer`]. Their voices are scaled
/// by the master volume alone, as Miles scales a sample only by the digital
/// master volume (MSS32.DLL 0x2000dc60: master x volume / 127):
/// `FUN_00432178` / `FUN_00432758` rewrite the six volumes on every frame
/// with max(a, b) / 2, never with the effects byte 0x00495e73, and the
/// puzzles' effects set their own volume. Where the original writes the
/// byte to the handles ([`AmbientSounds::stop_all`] before the stop; also
/// the sound menu's OK @ 0x0041d8c4..0x0041d8f5 and the checkpoint load @
/// 0x0042e436..0x0042e46c, which the port leaves out), it holds only until
/// the next ambient frame.
pub struct MixerAmbientOutput {
    mixer: Mixer,
    voices: [Option<VoiceId>; SLOTS],
    miles: MilesSampleStatus,
    pan: [u8; SLOTS],
    volume: [u8; SLOTS],
    /// object * 4 + variant -> 16-bit samples.
    pcm: HashMap<usize, Arc<Vec<i16>>>,
}

impl MixerAmbientOutput {
    pub fn new(mixer: Mixer) -> Self {
        MixerAmbientOutput {
            mixer,
            voices: [None; SLOTS],
            miles: MilesSampleStatus::default(),
            // FUN_00430d20: pan 0x40, volume 0.
            pan: [0x40; SLOTS],
            volume: [0; SLOTS],
            pcm: HashMap::new(),
        }
    }

    /// `FUN_00431a1c` / `FUN_00417cb4`: decodes variant `variant` of object
    /// `object` (an APC file), kept until [`release`](Self::release). The
    /// buffer holds the header's sample count times 2 (mono) or 4 (stereo)
    /// bytes, played as mono.
    pub fn load(&mut self, object: usize, variant: usize, apc: &[u8]) -> bool {
        match atlantis_codecs::decode_apc(apc) {
            Ok(a) => {
                self.pcm.insert(object * 4 + variant, Arc::new(a.samples));
                true
            }
            Err(e) => {
                log::warn!("ambient: object {object} variant {variant}: {e}");
                false
            }
        }
    }

    /// A puzzle's sound on sample `slot` (the players `FUN_0043ca28` and
    /// its siblings): AIL_set_sample_pan / volume, then the start. The
    /// sound plays at its own rate, as the ScummVM port plays it.
    pub fn start_pcm(&mut self, slot: usize, pcm: Pcm, pan: u8, volume: u8) {
        if let Some(v) = self.voices[slot].take() {
            self.mixer.stop(v);
        }
        self.pan[slot] = pan & 0x7f;
        self.volume[slot] = volume & 0x7f;
        self.miles.started(slot);
        if pcm.samples.is_empty() {
            return;
        }
        self.voices[slot] = Some(self.mixer.play(
            pcm,
            false,
            mixer_volume(self.volume[slot]),
            miles_pan_balance(self.pan[slot]),
        ));
    }

    fn active(&self, slot: usize) -> bool {
        self.voices[slot].is_some_and(|v| self.mixer.is_playing(v))
    }

    fn stop_all(&mut self) {
        for i in 0..SLOTS {
            self.stop(i);
        }
    }
}

impl Drop for MixerAmbientOutput {
    fn drop(&mut self) {
        self.stop_all();
    }
}

impl AmbientSampleOutput for MixerAmbientOutput {
    fn status(&mut self, slot: usize) -> i32 {
        self.miles.status(slot, self.active(slot))
    }

    fn start(&mut self, slot: usize, object: usize, variant: usize, loop_forever: bool) {
        if let Some(v) = self.voices[slot].take() {
            self.mixer.stop(v);
        }
        self.miles.started(slot); // without a buffer the slot reads done at once
        let Some(samples) = self.pcm.get(&(object * 4 + variant)) else {
            return;
        };
        if samples.is_empty() {
            return;
        }
        let pcm = Pcm {
            rate: SAMPLE_RATE,
            channels: 1,
            samples: samples.clone(),
        };
        self.voices[slot] = Some(self.mixer.play(
            pcm,
            loop_forever,
            mixer_volume(self.volume[slot]),
            miles_pan_balance(self.pan[slot]),
        ));
    }

    fn stop(&mut self, slot: usize) {
        let active = self.active(slot);
        self.miles.stopped(slot, active);
        if let Some(v) = self.voices[slot].take() {
            self.mixer.stop(v);
        }
    }

    fn set_pan(&mut self, slot: usize, pan: u8) {
        self.pan[slot] = pan & 0x7f;
        if let Some(v) = self.voices[slot] {
            self.mixer.set_volume(
                v,
                mixer_volume(self.volume[slot]),
                miles_pan_balance(self.pan[slot]),
            );
        }
    }

    fn set_volume(&mut self, slot: usize, volume: u8) {
        self.volume[slot] = volume & 0x7f;
        if let Some(v) = self.voices[slot] {
            self.mixer.set_volume(
                v,
                mixer_volume(self.volume[slot]),
                miles_pan_balance(self.pan[slot]),
            );
        }
    }

    fn release(&mut self, object: usize) {
        for k in 0..4 {
            self.pcm.remove(&(object * 4 + k));
        }
    }
}
