//! *Atlantis Evolution*'s 2004 program (atlantis4-cd-en-3022848): the
//! ship's node, the view entry's camera and `TANGAGEWARP`'s pitching.
//!
//! The 2004 warp has two scene nodes where *Atlantis III* has one: the
//! camera object's node 0x0070cee0 (camera 0x0070ce78 + 0x68, *Atlantis
//! III*'s 0x0077b788: the one the draw, the angle read-back and the turns
//! use) and a new node 0x0070c250, the "ship" (its static constructor
//! @ 0x00442840 → FUN_004b1950: identity, a child of the scene root
//! 0x0071bb20). While the pitching is on, the view entry hangs the camera
//! node under the ship (keeping its world orientation) and the warp frame
//! rocks the ship about the pitching axis; the turns then turn the camera
//! inside the ship. Every operation is the original's x87 code transcribed
//! (operands, association, float stores), checked bit for bit against the
//! original's functions under emulation (`tests/golden/tangage_2004.txt`).
//!
//! Nodes (FUN_004b1950): +0x00 rotation q (x, y, z, w), +0x10 position,
//! +0x1c scale, +0x20.. world q, position, scale (recomputed on demand by
//! FUN_004b23e0: a child of the root copies its own, another composes its
//! parent's world q with its q, FUN_004b1bf0), +0x40 parent. The root's
//! world transform is the identity (FUN_004b1900; FUN_004b1ec0 and
//! FUN_004b19d0 return at once on it), positions stay 0 and scales 1
//! (nothing writes them in the warp). [`Camera::q`] holds the camera
//! node's world q (what the draw reads, FUN_004b1c80 relative to the
//! root); [`Ship::cam`] its own q while the ship is its parent.

use atlantis2_engine::camera::{self, Camera, CameraOps};
use atlantis2_engine::Fpu;

pub type Quat = [f32; 4];

const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

/// π, 1/180 (floats @ 0x006996f0, @ 0x00699420), 0.6 (@ 0x00699474), 0.5
/// (the double @ 0x00699390), 1.0 (@ 0x00699348), the axis-angle's
/// threshold 1.1920929e-07 (@ 0x0069970c).
const PI: f32 = f32::from_bits(0x4049_0fdb);
const DEG: f32 = f32::from_bits(0x3bb6_0b61);
const SIX_TENTHS: f32 = f32::from_bits(0x3f19_999a);
const EPSILON: f32 = f32::from_bits(0x3400_0000);

#[inline(always)]
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// FUN_004b1bf0 (`this` = a): a · b.
pub fn mul(fpu: Fpu, a: Quat, b: Quat) -> Quat {
    let (add, sub, m) = (
        |x, y| fpu.add(x, y),
        |x, y| fpu.sub(x, y),
        |x, y| fpu.mul(x, y),
    );
    let [ax, ay, az, aw] = a.map(d);
    let [bx, by, bz, bw] = b.map(d);
    [
        fpu.f32(sub(add(add(m(aw, bx), m(ax, bw)), m(ay, bz)), m(az, by))),
        fpu.f32(sub(add(add(m(by, aw), m(ay, bw)), m(az, bx)), m(bz, ax))),
        fpu.f32(sub(add(add(m(bz, aw), m(az, bw)), m(by, ax)), m(ay, bx))),
        fpu.f32(sub(sub(sub(m(aw, bw), m(bx, ax)), m(ay, by)), m(az, bz))),
    ]
}

/// FUN_004b1e30 (`this` = a): a* · b.
pub fn conj_mul(fpu: Fpu, a: Quat, b: Quat) -> Quat {
    let (add, sub, m) = (
        |x, y| fpu.add(x, y),
        |x, y| fpu.sub(x, y),
        |x, y| fpu.mul(x, y),
    );
    let [ax, ay, az, aw] = a.map(d);
    let [bx, by, bz, bw] = b.map(d);
    [
        fpu.f32(add(sub(sub(m(aw, bx), m(ax, bw)), m(bz, ay)), m(by, az))),
        fpu.f32(add(sub(sub(m(by, aw), m(bw, ay)), m(bx, az)), m(bz, ax))),
        fpu.f32(add(sub(sub(m(bz, aw), m(bw, az)), m(by, ax)), m(bx, ay))),
        fpu.f32(add(add(add(m(bz, az), m(by, ay)), m(bx, ax)), m(aw, bw))),
    ]
}

/// FUN_004b1ec0 with the node itself as the frame (@ 0x004b1fc8..): q · r.
pub fn mul_self(fpu: Fpu, q: Quat, r: Quat) -> Quat {
    let (add, sub, m) = (
        |x, y| fpu.add(x, y),
        |x, y| fpu.sub(x, y),
        |x, y| fpu.mul(x, y),
    );
    let [qx, qy, qz, qw] = q.map(d);
    let [rx, ry, rz, rw] = r.map(d);
    [
        fpu.f32(sub(add(add(m(rz, qy), m(rw, qx)), m(qw, rx)), m(qz, ry))),
        fpu.f32(sub(add(add(m(qw, ry), m(qz, rx)), m(qy, rw)), m(rz, qx))),
        fpu.f32(sub(add(add(m(rz, qw), m(qz, rw)), m(qx, ry)), m(qy, rx))),
        fpu.f32(sub(sub(sub(m(qw, rw), m(rx, qx)), m(qy, ry)), m(rz, qz))),
    ]
}

/// FUN_004b21c0's rotation (@ 0x004b2318..0x004b23c7): the world q `w`
/// expressed in the new parent's frame (world q `n`): n* · w.
fn rebase(fpu: Fpu, n: Quat, w: Quat) -> Quat {
    let (add, sub, m) = (
        |x, y| fpu.add(x, y),
        |x, y| fpu.sub(x, y),
        |x, y| fpu.mul(x, y),
    );
    let [nx, ny, nz, nw] = n.map(d);
    let [wx, wy, wz, ww] = w.map(d);
    [
        fpu.f32(add(sub(sub(m(wx, nw), m(ww, nx)), m(wz, ny)), m(wy, nz))),
        fpu.f32(add(sub(sub(m(wy, nw), m(ww, ny)), m(wx, nz)), m(wz, nx))),
        fpu.f32(add(sub(sub(m(wz, nw), m(ww, nz)), m(wy, nx)), m(wx, ny))),
        fpu.f32(add(add(add(m(wz, nz), m(wy, ny)), m(ww, nw)), m(wx, nx))),
    ]
}

/// FUN_004b19d0 with the root as the frame, the parent another node
/// (@ 0x004b1a25..0x004b1ad0): p* · r, `p` the parent's world q.
fn set_from_root(fpu: Fpu, p: Quat, r: Quat) -> Quat {
    let (add, sub, m) = (
        |x, y| fpu.add(x, y),
        |x, y| fpu.sub(x, y),
        |x, y| fpu.mul(x, y),
    );
    let [px, py, pz, pw] = p.map(d);
    let [rx, ry, rz, rw] = r.map(d);
    [
        fpu.f32(add(sub(sub(m(pw, rx), m(rw, px)), m(rz, py)), m(ry, pz))),
        fpu.f32(add(sub(sub(m(pw, ry), m(rw, py)), m(pz, rx)), m(rz, px))),
        fpu.f32(add(sub(sub(m(pw, rz), m(rw, pz)), m(ry, px)), m(py, rx))),
        fpu.f32(add(add(add(m(rw, pw), m(px, rx)), m(rz, pz)), m(ry, py))),
    ]
}

/// FUN_004b19d0's general case (@ 0x004b1af1..0x004b1bbe): `t` (the frame's
/// world q times the rotation, FUN_004b1bf0) in the parent's frame (world
/// q `p`): p* · t.
fn set_from_other(fpu: Fpu, p: Quat, t: Quat) -> Quat {
    let (add, sub, m) = (
        |x, y| fpu.add(x, y),
        |x, y| fpu.sub(x, y),
        |x, y| fpu.mul(x, y),
    );
    let [px, py, pz, pw] = p.map(d);
    let [tx, ty, tz, tw] = t.map(d);
    [
        fpu.f32(add(sub(sub(m(tx, pw), m(tw, px)), m(tz, py)), m(ty, pz))),
        fpu.f32(add(sub(sub(m(ty, pw), m(tw, py)), m(tx, pz)), m(tz, px))),
        fpu.f32(add(sub(sub(m(tz, pw), m(tw, pz)), m(ty, px)), m(tx, py))),
        fpu.f32(add(add(add(m(ty, py), m(tz, pz)), m(tx, px)), m(tw, pw))),
    ]
}

/// 1 / √(sum) as the x87 leaves it (`fsqrt`, `fdivr` 1.0).
fn inv_sqrt(fpu: Fpu, sum: f64) -> f64 {
    fpu.div(1.0, fpu.sqrt(sum))
}

/// The ship's inline renormalisation (@ 0x004435a9, @ 0x00443897,
/// @ 0x00445b55): ((y² + z²) + w²) + x², each component times the
/// unrounded 1 / √.
fn normalize_ship(fpu: Fpu, q: Quat) -> Quat {
    let [x, y, z, w] = q.map(d);
    let m = |a, b| fpu.mul(a, b);
    let sum = fpu.add(fpu.add(fpu.add(m(y, y), m(z, z)), m(w, w)), m(x, x));
    let s = inv_sqrt(fpu, sum);
    q.map(|c| fpu.f32(m(s, d(c))))
}

/// The camera node's inline renormalisation (@ 0x00443651, @ 0x00445a43):
/// ((x² + y²) + z²) + w², each component times the unrounded 1 / √.
fn normalize_cam(fpu: Fpu, q: Quat) -> Quat {
    let [x, y, z, w] = q.map(d);
    let m = |a, b| fpu.mul(a, b);
    let sum = fpu.add(fpu.add(fpu.add(m(x, x), m(y, y)), m(z, z)), m(w, w));
    let s = inv_sqrt(fpu, sum);
    q.map(|c| fpu.f32(m(s, d(c))))
}

/// FUN_00446350: the sum as [`normalize_cam`]'s; 1 / √ stored as a float
/// (`fst dword`) for x, y, z, w multiplied by the unrounded value.
fn normalize_446350(fpu: Fpu, q: Quat) -> Quat {
    let [x, y, z, w] = q.map(d);
    let m = |a, b| fpu.mul(a, b);
    let sum = fpu.add(fpu.add(fpu.add(m(x, x), m(y, y)), m(z, z)), m(w, w));
    let s = inv_sqrt(fpu, sum);
    let sf = d(fpu.f32(s));
    [
        fpu.f32(m(sf, x)),
        fpu.f32(m(sf, y)),
        fpu.f32(m(sf, z)),
        fpu.f32(m(s, w)),
    ]
}

/// FUN_004539b0: a vector normalised, (x² + y²) + z²; 1 / √ stored as a
/// float for x and y, z multiplied by the unrounded value.
pub fn normalize_axis(fpu: Fpu, v: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = v.map(d);
    let m = |a, b| fpu.mul(a, b);
    let s = inv_sqrt(fpu, fpu.add(fpu.add(m(x, x), m(y, y)), m(z, z)));
    let sf = d(fpu.f32(s));
    [fpu.f32(m(sf, x)), fpu.f32(m(sf, y)), fpu.f32(m(s, z))]
}

/// FUN_004c8750 (= *Atlantis III*'s @ 0x004fd4f0): the quaternion of the
/// rotation by `angle` about `axis`: |axis| = √((x² + y²) + z²) (stored as
/// a float, compared unrounded with 1.1920929e-07: below, the identity);
/// h = angle · 0.5; (axis · (sin h / |axis|), cos h).
pub fn axis_angle_quat(fpu: Fpu, axis: [f32; 3], angle: f32) -> Quat {
    let [x, y, z] = axis.map(d);
    let m = |a, b| fpu.mul(a, b);
    let len = fpu.sqrt(fpu.add(fpu.add(m(x, x), m(y, y)), m(z, z)));
    let lenf = fpu.f32(len);
    if len < d(EPSILON) {
        return IDENTITY;
    }
    let h = m(d(angle), 0.5);
    let s = fpu.div(libm::sin(h), d(lenf));
    [
        fpu.f32(m(x, s)),
        fpu.f32(m(y, s)),
        fpu.f32(m(z, s)),
        fpu.f32(libm::cos(h)),
    ]
}

/// A frame of FUN_004b1ec0 / FUN_004b19d0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Frame {
    Root,
    Ship,
    Camera,
}

/// The pitching state and the ship's node.
#[derive(Clone, Debug, PartialEq)]
pub struct Ship {
    /// 0x0070d098: the pitching on.
    pub on: bool,
    /// 0x0070c8e0: the axis (normalised by the script; static (0, 0, 1),
    /// @ 0x004427e0).
    pub axis: [f32; 3],
    /// 0x006af064: degrees a second at the swing's fastest (static 15.0).
    pub speed: f32,
    /// 0x0070d0a0: the amplitude, radians.
    pub amp: f32,
    /// 0x0070d0a4: the swings left before the amplitude reaches 0 (0:
    /// undamped).
    pub damp: i32,
    /// 0x0070d0a8, 0x0070ccb0, 0x006af06c: the pitching a damped one was
    /// set over (amplitude, axis, speed; static 15.0), put back at its end.
    pub saved_amp: f32,
    pub saved_axis: [f32; 3],
    pub saved_speed: f32,
    /// 0x0070d09c: the current angle, radians.
    pub phase: f32,
    /// 0x006af068: the phase increasing (static 1).
    pub up: bool,
    /// 0x0070c250's rotation (a child of the root: its world q too).
    pub q: Quat,
    /// 0x0070cee0's own rotation while the ship is its parent.
    pub cam: Option<Quat>,
}

impl Default for Ship {
    fn default() -> Ship {
        Ship {
            on: false,
            axis: [0.0, 0.0, 1.0],
            speed: 15.0,
            amp: 0.0,
            damp: 0,
            saved_amp: 0.0,
            saved_axis: [0.0; 3],
            saved_speed: 15.0,
            phase: 0.0,
            up: true,
            q: IDENTITY,
            cam: None,
        }
    }
}

impl Ship {
    /// The camera node's own rotation.
    fn local(&self, c: &Camera) -> Quat {
        self.cam.unwrap_or(c.q)
    }

    fn parent(&self) -> Frame {
        if self.cam.is_some() {
            Frame::Ship
        } else {
            Frame::Root
        }
    }

    fn world(&self, c: &Camera, f: Frame) -> Quat {
        match f {
            Frame::Root => IDENTITY,
            Frame::Ship => self.q,
            Frame::Camera => c.q,
        }
    }

    /// The camera node's rotation set to `q`, its world q (FUN_004b23e0:
    /// the parent's world q times its own, FUN_004b1bf0) into the camera.
    fn store(&mut self, c: &mut Camera, q: Quat) {
        if self.cam.is_some() {
            self.cam = Some(q);
            c.q = mul(c.fpu, self.q, q);
        } else {
            c.q = q;
        }
    }

    /// The camera's world q again after the ship turned.
    fn refresh(&self, c: &mut Camera) {
        if let Some(q) = self.cam {
            c.q = mul(c.fpu, self.q, q);
        }
    }

    /// FUN_004b1ec0 on the camera node with the frame `f`.
    fn rotate(&mut self, c: &mut Camera, f: Frame, rot: Quat) {
        let fpu = c.fpu;
        let q = self.local(c);
        let parent = self.parent();
        let q = if f == parent {
            // @ 0x004b1ee0: rot · q.
            mul(fpu, rot, q)
        } else if f == Frame::Camera {
            mul_self(fpu, q, rot)
        } else {
            // @ 0x004b1f16..0x004b1fc0: into the world, into the frame,
            // turned, back to the world, back into the parent.
            let p = self.world(c, parent);
            let w = self.world(c, f);
            let t = conj_mul(fpu, w, mul(fpu, p, q));
            conj_mul(fpu, p, mul(fpu, w, mul(fpu, rot, t)))
        };
        self.store(c, q);
    }

    /// FUN_004b19d0 on the camera node: its rotation set to `r` relative
    /// to the frame `f`.
    fn set_rotation(&mut self, c: &mut Camera, f: Frame, r: Quat) {
        let fpu = c.fpu;
        let parent = self.parent();
        let p = self.world(c, parent);
        let q = if f == parent {
            r
        } else if f == Frame::Root {
            set_from_root(fpu, p, r)
        } else {
            let t = mul(fpu, self.world(c, f), r);
            set_from_other(fpu, p, t)
        };
        self.store(c, q);
    }

    /// FUN_004b2090(parent, keep) on the camera node: nothing when `to` is
    /// already its parent; with `keep`, its rotation re-expressed so that
    /// its world orientation stays (FUN_004b21c0: the old parent's world q
    /// times it, FUN_004b1bf0, then [`rebase`] on the new parent's).
    fn reparent(&mut self, c: &mut Camera, to: Frame, keep: bool) {
        let from = self.parent();
        if to == from {
            return;
        }
        let fpu = c.fpu;
        let mut q = self.local(c);
        if keep {
            let w = mul(fpu, self.world(c, from), q);
            q = rebase(fpu, self.world(c, to), w);
        }
        self.cam = (to == Frame::Ship).then_some(q);
        if self.cam.is_none() {
            c.q = q;
        } else {
            self.refresh(c);
        }
    }

    /// The pitching axis in the ship's frame, as the step computes it
    /// (@ 0x00445826..0x004458b0): times the root's world scale
    /// (FUN_00432f00), turned by the root's world q (FUN_00432ff0), plus
    /// its world position (FUN_00432ed0), minus the ship's (FUN_0043bbe0),
    /// turned back by the ship's world q (FUN_0043bdc0), times 1 / the
    /// ship's world scale (the 1 / not stored).
    fn axis_step(&self, fpu: Fpu) -> [f32; 3] {
        let v1 = self.axis.map(|a| fpu.f32(fpu.mul(1.0, d(a))));
        let v2 = camera::rotate_a3(fpu, IDENTITY, v1);
        let v4 = shift(fpu, v2);
        let v5 = camera::rotate_back_a3(fpu, self.q, v4);
        let s = fpu.div(1.0, 1.0);
        v5.map(|a| fpu.f32(fpu.mul(s, d(a))))
    }

    /// The same at the view entry (@ 0x0044370f..0x0044386a): the turn by
    /// the root's world q inline (each product of q v stored as a float,
    /// then its own order of the sums), the scale by FUN_0043bd80 (1 /
    /// stored as a float for x and y).
    fn axis_entry(&self, fpu: Fpu) -> [f32; 3] {
        let (add, sub, m) = (
            |x, y| fpu.add(x, y),
            |x, y| fpu.sub(x, y),
            |x, y| fpu.mul(x, y),
        );
        let [x, y, z] = self.axis.map(|a| d(fpu.f32(fpu.mul(1.0, d(a)))));
        let [rx, ry, rz, rw] = IDENTITY.map(d);
        let t3 = d(fpu.f32(add(add(m(y, ry), m(x, rx)), m(z, rz))));
        let t2 = d(fpu.f32(add(sub(m(z, rw), m(x, ry)), m(y, rx))));
        let t1 = d(fpu.f32(add(sub(m(y, rw), m(z, rx)), m(x, rz))));
        let t0 = d(fpu.f32(add(sub(m(x, rw), m(y, rz)), m(z, ry))));
        let v2 = [
            fpu.f32(sub(add(add(m(t0, rw), m(t3, rx)), m(t2, ry)), m(t1, rz))),
            fpu.f32(sub(add(add(m(t0, rz), m(t1, rw)), m(t3, ry)), m(t2, rx))),
            fpu.f32(sub(add(add(m(t3, rz), m(t2, rw)), m(t1, rx)), m(t0, ry))),
        ];
        let v4 = shift(fpu, v2);
        let v5 = camera::rotate_back_a3(fpu, self.q, v4).map(d);
        let s = fpu.div(1.0, 1.0);
        let sf = d(fpu.f32(s));
        [
            fpu.f32(m(sf, v5[0])),
            fpu.f32(m(sf, v5[1])),
            fpu.f32(m(s, v5[2])),
        ]
    }

    /// `TANGAGEWARP=v,...` with v ≠ 0: FUN_00445450(axis, amp°, speed,
    /// damp). Off: everything set (amplitude amp · π · 1/180, the phase 0,
    /// increasing, nothing saved) and on. On with `damp` ≠ 0: the current
    /// axis, speed and amplitude saved, the new ones set the same way (the
    /// ship keeps its tilt). On with `damp` = 0: nothing.
    pub fn set(&mut self, fpu: Fpu, axis: [f32; 3], amp: i32, speed: i32, damp: i32) {
        let amp_rad = || fpu.f32(fpu.mul(fpu.mul(f64::from(amp), d(PI)), d(DEG)));
        if !self.on {
            self.speed = speed as f32;
            self.axis = axis;
            self.amp = amp_rad();
            self.on = true;
            self.phase = 0.0;
            self.up = true;
            self.damp = damp;
            self.saved_amp = 0.0;
        } else if damp != 0 {
            self.saved_axis = self.axis;
            self.saved_speed = self.speed;
            self.speed = speed as f32;
            self.saved_amp = self.amp;
            self.axis = axis;
            self.amp = amp_rad();
            self.on = true;
            self.phase = 0.0;
            self.up = true;
            self.damp = damp;
        }
    }

    /// `TANGAGEWARP=0` (FUN_004455b0): off, the phase 0. The camera keeps
    /// its tilt and stays under the ship until the next view entry (the
    /// vector computations made there are not stored).
    pub fn stop(&mut self) {
        self.on = false;
        self.phase = 0.0;
    }

    /// The warp frame's step (FUN_00445760, @ 0x00444168 at the end of the
    /// warp routine FUN_00443cc0), `t` the previous frame routine's
    /// duration in seconds (FUN_004a2630: the float 0x00719d94).
    pub fn step(&mut self, c: &mut Camera, t: f32) {
        if !self.on || (self.amp == 0.0 && self.speed == 0.0) {
            return;
        }
        let fpu = c.fpu;
        let m = |a, b| fpu.mul(a, b);
        // (float)(cos(π / amp · phase) · 0.5 + 0.6), then
        // (float)(π · (t · speed · that) · 1/180).
        let a = m(fpu.div(d(PI), d(self.amp)), d(self.phase));
        let k = fpu.f32(fpu.add(m(libm::cos(a), 0.5), d(SIX_TENTHS)));
        let ts = m(m(d(t), d(self.speed)), d(k));
        let delta = fpu.f32(m(m(d(PI), ts), d(DEG)));
        let axis = self.axis_step(fpu);
        self.q = mul_self(fpu, self.q, axis_angle_quat(fpu, axis, -self.phase));
        let mut turned = false;
        if self.up {
            let s = fpu.add(d(self.phase), d(delta));
            self.phase = fpu.f32(s);
            // `fcomp amp; test ah, 1`: the unrounded sum, a turn unless
            // below (or unordered).
            if s >= d(self.amp) {
                self.up = false;
                self.phase = self.amp;
                turned = true;
            }
        } else {
            self.phase = fpu.f32(fpu.add(-d(delta), d(self.phase)));
            let low = -d(self.amp);
            // `test ah, 0x41`: below, equal or unordered.
            if d(self.phase).partial_cmp(&low) != Some(std::cmp::Ordering::Greater) {
                self.phase = -self.amp;
                self.up = true;
                turned = true;
            }
        }
        if turned && self.damp != 0 {
            let old = self.damp;
            self.damp -= 1;
            let a = d(self.amp);
            self.amp = fpu.f32(fpu.sub(a, fpu.div(a, f64::from(old))));
            if self.damp == 0 {
                self.amp = 0.0;
            }
        }
        if self.amp != 0.0 {
            self.q = mul_self(fpu, self.q, axis_angle_quat(fpu, axis, self.phase));
            self.q = normalize_ship(fpu, self.q);
            self.refresh(c);
            return;
        }
        if self.saved_amp == 0.0 {
            self.stop();
            self.refresh(c);
            return;
        }
        // @ 0x004459d2: the swing undone, the ship renormalised
        // (FUN_00446350), the camera node under the ship (not kept) and
        // renormalised, the saved pitching back from phase 0.
        self.q = mul_self(fpu, self.q, axis_angle_quat(fpu, axis, -self.phase));
        self.q = normalize_446350(fpu, self.q);
        self.reparent(c, Frame::Ship, false);
        let local = normalize_cam(fpu, self.local(c));
        self.store(c, local);
        self.axis = self.saved_axis;
        self.phase = 0.0;
        self.up = true;
        self.speed = self.saved_speed;
        self.amp = self.saved_amp;
    }

    /// The camera part of the view entry FUN_00442c80, `turn` its second
    /// argument (1 from `VUE`, the moves and `CHANGEWAM`; 0 from
    /// `VUEANGLES`), (yaw, pitch) the arrival angles (0x006e7c20 / 24).
    pub fn enter(&mut self, c: &mut Camera, yaw: f32, pitch: f32, turn: bool) {
        let fpu = c.fpu;
        let rot_y = |a| camera::rotation(fpu, [0.0, 1.0, 0.0], a);
        let rot_x = |a| camera::rotation(fpu, [1.0, 0.0, 0.0], a);
        if !self.on {
            // @ 0x00442e32: the camera node back under the root, its world
            // orientation kept; the ship's (already the root's child);
            // FUN_00446350 on the camera node; with `turn`, *Atlantis
            // III*'s turn: back by the read-back angles (yaw about the
            // root's Y, pitch about its own X), then by minus the arrival
            // angles.
            self.reparent(c, Frame::Root, true);
            let q = normalize_446350(fpu, self.local(c));
            self.store(c, q);
            if turn {
                let (y0, p0) = c.angles();
                self.rotate(c, Frame::Root, rot_y(y0));
                self.rotate(c, Frame::Camera, rot_x(p0));
                self.rotate(c, Frame::Root, rot_y(-yaw));
                self.rotate(c, Frame::Camera, rot_x(-pitch));
            }
            return;
        }
        // @ 0x004434bd: with `turn`, back by the read-back angles.
        if turn {
            let (y0, p0) = c.angles();
            self.rotate(c, Frame::Root, rot_y(y0));
            self.rotate(c, Frame::Camera, rot_x(p0));
        }
        // @ 0x00443573: the camera node under the root (kept), the ship
        // set to its orientation (FUN_004b1c80 / FUN_004b19d0) and
        // renormalised, the camera node under the ship (kept) and
        // renormalised.
        self.reparent(c, Frame::Root, true);
        self.q = normalize_ship(fpu, self.local(c));
        self.reparent(c, Frame::Ship, true);
        let local = normalize_cam(fpu, self.local(c));
        self.store(c, local);
        // @ 0x004436f5..0x00443936: the ship turned by the current phase
        // about the axis in its frame, renormalised.
        let axis = self.axis_entry(fpu);
        self.q = mul_self(fpu, self.q, axis_angle_quat(fpu, axis, self.phase));
        self.q = normalize_ship(fpu, self.q);
        self.refresh(c);
        // @ 0x00443940: with `turn`, minus the arrival angles, the yaw
        // about the ship's Y.
        if turn {
            self.rotate(c, Frame::Ship, rot_y(-yaw));
            self.rotate(c, Frame::Camera, rot_x(-pitch));
        }
    }

    /// The menu's view re-entry FUN_00443a10 (@ 0x00443c57): with the
    /// pitching off, the camera node back under the root (kept).
    pub fn reenter(&mut self, c: &mut Camera) {
        if !self.on {
            self.reparent(c, Frame::Root, true);
        }
    }

    /// The turn about Y (FUN_00444920): relative to the ship while the
    /// pitching is on, to the root when off.
    pub fn yaw(&mut self, c: &mut Camera, angle: f32) {
        let f = if self.on { Frame::Ship } else { Frame::Root };
        let rot = camera::rotation(c.fpu, [0.0, 1.0, 0.0], angle);
        self.rotate(c, f, rot);
    }

    /// The turn about the camera's own X (FUN_004449e0).
    pub fn pitch(&mut self, c: &mut Camera, angle: f32) {
        let rot = camera::rotation(c.fpu, [1.0, 0.0, 0.0], angle);
        self.rotate(c, Frame::Camera, rot);
    }

    /// A roll about the camera's own Z (the turns' `ZoomPsycho` roll; the
    /// 2004 program's FUN_00445220 / FUN_004452d0 turn the camera node
    /// about (0, 0, 1) with itself as the frame).
    pub fn roll(&mut self, c: &mut Camera, angle: f32) {
        let rot = camera::rotation(c.fpu, [0.0, 0.0, 1.0], angle);
        self.rotate(c, Frame::Camera, rot);
    }

    /// `ANGLEWARPXNOW` (FUN_00444a40): the rotation set about Y, relative
    /// to the ship while the pitching is on, to the root when off.
    pub fn set_yaw(&mut self, c: &mut Camera, angle: f32) {
        let f = if self.on { Frame::Ship } else { Frame::Root };
        let rot = camera::rotation(c.fpu, [0.0, 1.0, 0.0], angle);
        self.set_rotation(c, f, rot);
    }

    /// `ANGLEWARPYNOW` (FUN_00444b00 = *Atlantis III*'s @ 0x00440880):
    /// the rotation set about X relative to the camera node itself
    /// (FUN_004b19d0's general case).
    pub fn set_pitch(&mut self, c: &mut Camera, angle: f32) {
        let rot = camera::rotation(c.fpu, [1.0, 0.0, 0.0], angle);
        self.set_rotation(c, Frame::Camera, rot);
    }

    /// Whether the camera's turns need this model ([`Camera`]'s own methods
    /// are the original's while the camera node is the root's child and
    /// the pitching off).
    pub fn engaged(&self) -> bool {
        self.on || self.cam.is_some()
    }
}

/// FUN_00432ed0 with the root's world position (0) then FUN_0043bbe0 with
/// the ship's (0): each component stored after each.
fn shift(fpu: Fpu, v: [f32; 3]) -> [f32; 3] {
    let v3 = v.map(|a| fpu.f32(fpu.add(0.0, d(a))));
    v3.map(|a| fpu.f32(fpu.sub(d(a), 0.0)))
}

/// The camera with the ship, for the look and the turns.
pub struct ShipCamera<'a> {
    pub cam: &'a mut Camera,
    pub ship: &'a mut Ship,
}

impl CameraOps for ShipCamera<'_> {
    fn fpu(&self) -> Fpu {
        self.cam.fpu
    }
    fn yaw(&mut self, angle: f32) {
        self.ship.yaw(self.cam, angle);
    }
    fn pitch(&mut self, angle: f32) {
        self.ship.pitch(self.cam, angle);
    }
    fn roll(&mut self, angle: f32) {
        self.ship.roll(self.cam, angle);
    }
    fn angles(&self) -> (f32, f32) {
        self.cam.angles()
    }
    fn set_fov(&mut self, fov: f32, width: u32, height: u32) {
        self.cam.set_fov(fov, width, height);
    }
}
