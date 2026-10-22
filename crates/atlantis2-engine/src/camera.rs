//! The warp's camera: the view camera object (@ 0x00660f48) with its
//! projection and the orientation of its scene node (@ 0x00660fb0, a child
//! of the scene root @ 0x006d8694).
//!
//! The orientation is a unit quaternion of floats. Turning composes it
//! with the quaternion of an axis-angle rotation, built through a 3 x 3
//! matrix; the renderer turns it back into a matrix. Every step is
//! transcribed operation by operation, so that the matrix the cube is drawn
//! with is the original's to the last bit: its float residues (1e-7 where
//! the exact value is 0) move a few pixels.
//!
//! Conventions: camera space is x right, y down, z forward. The angles read
//! back from the camera ([`Camera::angles`]) are the script's
//! `ANGLEWARPX` / `ANGLEWARPY`: yaw measured from +x toward +z (0 looks at
//! the `rt` face, 90° at `ft`, increasing turns left), pitch positive
//! looking down. At start-up the node is the identity: yaw 90°, pitch 0.

use crate::fpu::Fpu;

/// The 76° field of view the warp sets (@ 0x00459a10, the float
/// 0x3fa9c91f, in radians).
pub const WARP_FOV: f32 = f32::from_bits(0x3fa9_c91f);

/// Degrees to radians as the `/set` handler converts `ANGLEWARPX` /
/// `ANGLEWARPY` (@ 0x00481e20: `fild`, `fmul` by the float 0.017453292
/// @ 0x005c66cc, `fstp dword`).
pub fn deg_to_rad(fpu: Fpu, deg: i32) -> f32 {
    fpu.f32(fpu.mul(f64::from(deg), f64::from(f32::from_bits(0x3c8e_fa35))))
}

/// The camera: projection and orientation.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub fpu: Fpu,
    /// Focal lengths and centre (floats 0..3 of the camera object).
    pub fx: f32,
    pub fy: f32,
    pub cx: f32,
    pub cy: f32,
    /// 1 / fx, 1 / fy (floats +0x10, +0x14).
    pub inv_fx: f32,
    pub inv_fy: f32,
    /// The node's orientation (x, y, z, w).
    pub q: [f32; 4],
}

/// A float of the original as an `f64` operand.
#[inline(always)]
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// FUN_004c6d10: the rotation matrix (row-major) of an axis and an angle.
fn axis_angle(fpu: Fpu, axis: [f32; 3], angle: f32) -> [f64; 9] {
    let (add, sub, mul) = (
        |a, b| fpu.add(a, b),
        |a, b| fpu.sub(a, b),
        |a, b| fpu.mul(a, b),
    );
    let f = |x| fpu.f(x);
    let [ax, ay, az] = axis.map(d);
    // FUN_00492bf0: (x² + y²) + z², then 1 / sqrt, stored.
    let len2 = add(add(mul(ax, ax), mul(ay, ay)), mul(az, az));
    let inv = f(fpu.div(1.0, fpu.sqrt(len2)));
    // FUN_0049a720: the axis scaled.
    let (nx, ny, nz) = (f(mul(inv, ax)), f(mul(inv, ay)), f(mul(inv, az)));
    let s = libm::sin(d(angle));
    let c = f(libm::cos(d(angle)));
    let nyx = f(mul(ny, nx));
    let nyz = f(mul(ny, nz));
    let nzx = f(mul(nz, nx));
    let nxs = f(mul(nx, s));
    let nys = f(mul(ny, s));
    let zs = mul(s, nz);
    let t = f(sub(1.0, c));
    let a = mul(t, nyx);
    let b = mul(t, nzx);
    let cc = mul(t, nyz);
    [
        f(add(mul(mul(nx, nx), t), c)),
        f(sub(a, zs)),
        f(add(b, nys)),
        f(add(a, zs)),
        f(add(mul(mul(ny, ny), t), c)),
        f(sub(cc, nxs)),
        f(sub(f(b), nys)),
        f(add(cc, nxs)),
        f(add(mul(mul(nz, nz), t), c)),
    ]
}

/// FUN_004c8a10: the quaternion of a rotation matrix.
fn matrix_to_quat(fpu: Fpu, m: &[f64; 9]) -> [f32; 4] {
    let (add, sub, mul) = (
        |a, b| fpu.add(a, b),
        |a, b| fpu.sub(a, b),
        |a, b| fpu.mul(a, b),
    );
    let f = |x| fpu.f32(x);
    let mut q = [0f32; 4];
    let tr = add(add(m[4], m[0]), m[8]);
    if tr > 0.0 {
        let s = fpu.sqrt(add(tr, 1.0));
        q[3] = f(mul(s, 0.5));
        let k = fpu.div(0.5, s);
        q[0] = f(mul(sub(m[7], m[5]), k));
        q[1] = f(mul(sub(m[2], m[6]), k));
        q[2] = f(mul(sub(m[3], m[1]), k));
        return q;
    }
    // The largest diagonal term (table @ 0x005e4c88: next = 1, 2, 0).
    const NEXT: [usize; 3] = [1, 2, 0];
    let mut i = usize::from(m[4] > m[0]);
    if m[8] > m[i * 4] {
        i = 2;
    }
    let j = NEXT[i];
    let k = NEXT[j];
    let s = fpu.sqrt(add(sub(m[i * 4], add(m[k * 4], m[j * 4])), 1.0));
    q[i] = f(mul(s, 0.5));
    let h = fpu.div(0.5, s);
    q[j] = f(mul(add(m[j * 3 + i], m[i * 3 + j]), h));
    q[k] = f(mul(add(m[i + k * 3], m[i * 3 + k]), h));
    q[3] = f(mul(sub(m[k * 3 + j], m[j * 3 + k]), h));
    q
}

/// FUN_004a29c0 (and the in-place product of FUN_004a2d00): a · b.
fn quat_mul(fpu: Fpu, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let (add, sub, mul) = (
        |a, b| fpu.add(a, b),
        |a, b| fpu.sub(a, b),
        |a, b| fpu.mul(a, b),
    );
    let f = |x| fpu.f32(x);
    let [ax, ay, az, aw] = a.map(d);
    let [bx, by, bz, bw] = b.map(d);
    [
        f(sub(
            add(add(mul(ay, bz), mul(bw, ax)), mul(aw, bx)),
            mul(by, az),
        )),
        f(sub(
            add(add(mul(ay, bw), mul(aw, by)), mul(bx, az)),
            mul(ax, bz),
        )),
        f(sub(
            add(add(mul(aw, bz), mul(bw, az)), mul(by, ax)),
            mul(ay, bx),
        )),
        f(sub(
            sub(sub(mul(aw, bw), mul(bx, ax)), mul(ay, by)),
            mul(bz, az),
        )),
    ]
}

/// FUN_004c6e40: the 3 x 3 matrix of a quaternion.
fn quat_to_matrix(fpu: Fpu, q: [f32; 4]) -> [f32; 9] {
    let (add, sub, mul) = (
        |a, b| fpu.add(a, b),
        |a, b| fpu.sub(a, b),
        |a, b| fpu.mul(a, b),
    );
    let f = |x| fpu.f(x);
    let [qx, qy, qz, qw] = q.map(d);
    let x2 = add(qx, qx);
    let y2 = f(add(qy, qy));
    let z2 = f(add(qz, qz));
    let wx = f(mul(qw, x2));
    let wy = f(mul(qw, y2));
    let wz = f(mul(qw, z2));
    let xx = mul(x2, qx);
    let xy = mul(qx, y2);
    let xz = f(mul(qx, z2));
    let yy = f(mul(qy, y2));
    let yz = f(mul(qy, z2));
    let zz = mul(qz, z2);
    let zzf = f(zz);
    [
        sub(1.0, add(zz, yy)),
        sub(xy, wz),
        add(xz, wy),
        add(xy, wz),
        sub(1.0, add(zzf, xx)),
        sub(yz, wx),
        sub(xz, wy),
        add(yz, wx),
        sub(1.0, add(yy, xx)),
    ]
    .map(|v| fpu.f32(v))
}

/// FUN_004581d0: the yaw and pitch (radians, stored as floats) of a
/// direction: `fpatan` of (z, x), then of (y, √(z² + x²)).
pub fn direction_angles(fpu: Fpu, v: [f32; 3]) -> (f32, f32) {
    let [x, y, z] = v.map(d);
    let yaw = fpu.f32(libm::atan2(z, x));
    let h = fpu.sqrt(fpu.add(fpu.mul(z, z), fpu.mul(x, x)));
    let pitch = fpu.f32(libm::atan2(y, h));
    (yaw, pitch)
}

/// FUN_00459300: a zone's aim, the angles of the sum of its points (each
/// sum stored as a float, in file order).
pub fn zone_aim(fpu: Fpu, points: &[[f32; 3]]) -> (f32, f32) {
    let mut s = [0f32; 3];
    for p in points {
        for i in 0..3 {
            s[i] = fpu.f32(fpu.add(d(p[i]), d(s[i])));
        }
    }
    direction_angles(fpu, s)
}

impl Camera {
    /// The warp's camera for a `width` x `height` screen, after the warp's
    /// initialisation (@ 0x00459a10): 76° field of view, centre (W/2, H/2),
    /// orientation the identity.
    pub fn new(fpu: Fpu, width: u32, height: u32) -> Camera {
        let mut c = Camera {
            fpu,
            fx: 0.0,
            fy: 0.0,
            cx: (width / 2) as f32,
            cy: (height / 2) as f32,
            inv_fx: 0.0,
            inv_fy: 0.0,
            q: [0.0, 0.0, 0.0, 1.0],
        };
        c.set_fov(WARP_FOV, width, height);
        c
    }

    /// FUN_004a4950(fov, W, H, W, H) → FUN_004a49c0 (perspective mode):
    /// fx = W / (2 tan(fov / 2)) (`fptan`), fy = fx · (W/H) / (W/H); then
    /// FUN_004a4a30: 1 / fx, 1 / fy.
    pub fn set_fov(&mut self, fov: f32, width: u32, height: u32) {
        let fpu = self.fpu;
        let (w, h) = (f64::from(width), f64::from(height));
        let t = libm::tan(fpu.mul(d(fov), 0.5));
        let fx = fpu.div(w, fpu.add(t, t));
        self.fx = fpu.f32(fx);
        let ratio = fpu.div(fpu.div(w, h), fpu.div(w, h));
        self.fy = fpu.f32(fpu.mul(ratio, fx));
        self.inv_fx = fpu.f32(fpu.div(1.0, d(self.fx)));
        self.inv_fy = fpu.f32(fpu.div(1.0, d(self.fy)));
    }

    /// The projection as the renderer reads it: fx, fy, cx, cy.
    pub fn projection(&self) -> [f32; 4] {
        [self.fx, self.fy, self.cx, self.cy]
    }

    /// FUN_0045ac90: turn by `angle` (radians) about the world's Y axis;
    /// positive turns right (the read-back yaw decreases).
    pub fn yaw(&mut self, angle: f32) {
        let m = axis_angle(self.fpu, [0.0, 1.0, 0.0], angle);
        let rot = matrix_to_quat(self.fpu, &m);
        // FUN_004a2d00 with the parent as the frame: rot · q.
        self.q = quat_mul(self.fpu, rot, self.q);
    }

    /// FUN_0045acf0: turn by `angle` (radians) about the camera's own X
    /// axis; positive looks up (the read-back pitch decreases).
    pub fn pitch(&mut self, angle: f32) {
        let m = axis_angle(self.fpu, [1.0, 0.0, 0.0], angle);
        let rot = matrix_to_quat(self.fpu, &m);
        // FUN_004a2d00 with the node itself as the frame: q · rot.
        self.q = quat_mul(self.fpu, self.q, rot);
    }

    /// The matrix the cube's corners are multiplied by (FUN_0045c030:
    /// FUN_004a2a70 relative to the root, then FUN_004c6e40).
    pub fn matrix(&self) -> [f32; 9] {
        quat_to_matrix(self.fpu, self.q)
    }

    /// The direction of the screen point (sx, sy) in the world: FUN_004a6380
    /// at depth 0.0001 (perspective mode), turned by the matrix as
    /// FUN_0045adf0 does. Floats, as stored.
    pub fn ray(&self, sx: f32, sy: f32) -> [f32; 3] {
        let fpu = self.fpu;
        let (add, sub, mul) = (
            |a, b| fpu.add(a, b),
            |a, b| fpu.sub(a, b),
            |a, b| fpu.mul(a, b),
        );
        let z = fpu.div(1.0, d(0.0001f32));
        let vx = fpu.f(mul(mul(sub(d(sx), d(self.cx)), d(self.inv_fx)), z));
        let vy = fpu.f(mul(mul(sub(d(sy), d(self.cy)), d(self.inv_fy)), z));
        let vz = fpu.f(z);
        let m = self.matrix().map(d);
        let row = |a: usize| {
            fpu.f32(add(
                add(mul(vz, m[a + 2]), mul(vy, m[a + 1])),
                mul(vx, m[a]),
            ))
        };
        [row(0), row(3), row(6)]
    }

    /// FUN_0045adf0 → FUN_004581d0: the yaw and pitch (radians, floats) of
    /// the screen centre's direction: yaw = atan2(z, x), pitch = atan2(y,
    /// sqrt(z² + x²)).
    pub fn angles(&self) -> (f32, f32) {
        direction_angles(self.fpu, self.ray(self.cx, self.cy))
    }

    /// The view entry's orientation (FUN_00459fe0 @ 0x0045a0b0..0x0045a1c2):
    /// turn back by the angles read now, then by minus the arrival angles
    /// (radians), so that reading the angles back gives them.
    pub fn enter(&mut self, yaw: f32, pitch: f32) {
        let (y0, p0) = self.angles();
        self.yaw(y0);
        self.pitch(p0);
        self.yaw(-yaw);
        self.pitch(-pitch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_up_and_first_view() {
        let mut c = Camera::new(Fpu::Double, 640, 480);
        assert_eq!(c.projection(), [409.581_33, 409.581_33, 320.0, 240.0]);
        let (y, p) = c.angles();
        assert_eq!((y, p), (std::f32::consts::FRAC_PI_2, 0.0));
        c.enter(deg_to_rad(Fpu::Double, -90), deg_to_rad(Fpu::Double, 0));
        let (y, p) = c.angles();
        assert!((y.to_degrees() + 90.0).abs() < 1e-4 && p.abs() < 1e-6);
        // The game's matrix, residues included (yaw -90, pitch 0).
        let m = c.matrix().map(f32::to_bits);
        assert_eq!(
            m,
            [0xbf800000, 0, 0xb43504f4, 0, 0x3f800000, 0, 0x343504f4, 0, 0xbf800000]
        );
    }

    #[test]
    fn focal_at_both_precisions() {
        // 409.58133 and 409.58130 at 640 wide, 511.97665 at 800 (the
        // golden frames' camera floats).
        let fx = |fpu, w, h| Camera::new(fpu, w, h).fx.to_bits();
        assert_eq!(fx(Fpu::Double, 640, 480), 0x43cc_ca69);
        assert_eq!(fx(Fpu::Single, 640, 480), 0x43cc_ca68);
        assert_eq!(fx(Fpu::Double, 800, 600), 0x43ff_fd03);
    }
}
