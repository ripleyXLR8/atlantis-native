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

use crate::fpu::{f, Fpu};

/// The 76° field of view the warp sets (@ 0x00459a10, the float
/// 0x3fa9c91f, in radians).
pub const WARP_FOV: f32 = f32::from_bits(0x3fa9_c91f);

/// Degrees to radians as the `/set` handler converts `ANGLEWARPX` /
/// `ANGLEWARPY` (@ 0x00481e20: `fild`, `fmul` by the float 0.017453292
/// @ 0x005c66cc, `fstp dword`).
pub fn deg_to_rad(fpu: Fpu, deg: i32) -> f32 {
    f(fpu.r(f64::from(deg) * f64::from(f32::from_bits(0x3c8e_fa35)))) as f32
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
    let r = |x: f64| fpu.r(x);
    let [ax, ay, az] = axis.map(d);
    // FUN_00492bf0: (x² + y²) + z², then 1 / sqrt, stored.
    let len2 = r(r(r(ax * ax) + r(ay * ay)) + r(az * az));
    let inv = f(r(1.0 / r(len2.sqrt())));
    // FUN_0049a720: the axis scaled.
    let (nx, ny, nz) = (f(r(inv * ax)), f(r(inv * ay)), f(r(inv * az)));
    let s = libm::sin(d(angle));
    let c = f(libm::cos(d(angle)));
    let nyx = f(r(ny * nx));
    let nyz = f(r(ny * nz));
    let nzx = f(r(nz * nx));
    let nxs = f(r(nx * s));
    let nys = f(r(ny * s));
    let zs = r(s * nz);
    let t = f(r(1.0 - c));
    let a = r(t * nyx);
    let b = r(t * nzx);
    let cc = r(t * nyz);
    [
        f(r(r(r(nx * nx) * t) + c)),
        f(r(a - zs)),
        f(r(b + nys)),
        f(r(a + zs)),
        f(r(r(r(ny * ny) * t) + c)),
        f(r(cc - nxs)),
        f(r(f(b) - nys)),
        f(r(cc + nxs)),
        f(r(r(r(nz * nz) * t) + c)),
    ]
}

/// FUN_004c8a10: the quaternion of a rotation matrix.
fn matrix_to_quat(fpu: Fpu, m: &[f64; 9]) -> [f32; 4] {
    let r = |x: f64| fpu.r(x);
    let mut q = [0f32; 4];
    let tr = r(r(m[4] + m[0]) + m[8]);
    if tr > 0.0 {
        let s = r(r(tr + 1.0).sqrt());
        q[3] = f(r(s * 0.5)) as f32;
        let k = r(0.5 / s);
        q[0] = f(r(r(m[7] - m[5]) * k)) as f32;
        q[1] = f(r(r(m[2] - m[6]) * k)) as f32;
        q[2] = f(r(r(m[3] - m[1]) * k)) as f32;
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
    let s = r(r(r(m[i * 4] - r(m[k * 4] + m[j * 4])) + 1.0).sqrt());
    q[i] = f(r(s * 0.5)) as f32;
    let h = r(0.5 / s);
    q[j] = f(r(r(m[j * 3 + i] + m[i * 3 + j]) * h)) as f32;
    q[k] = f(r(r(m[i + k * 3] + m[i * 3 + k]) * h)) as f32;
    q[3] = f(r(r(m[k * 3 + j] - m[j * 3 + k]) * h)) as f32;
    q
}

/// FUN_004a29c0 (and the in-place product of FUN_004a2d00): a · b.
fn quat_mul(fpu: Fpu, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let r = |x: f64| fpu.r(x);
    let [ax, ay, az, aw] = a.map(d);
    let [bx, by, bz, bw] = b.map(d);
    [
        f(r(r(r(r(ay * bz) + r(bw * ax)) + r(aw * bx)) - r(by * az))) as f32,
        f(r(r(r(r(ay * bw) + r(aw * by)) + r(bx * az)) - r(ax * bz))) as f32,
        f(r(r(r(r(aw * bz) + r(bw * az)) + r(by * ax)) - r(ay * bx))) as f32,
        f(r(r(r(r(aw * bw) - r(bx * ax)) - r(ay * by)) - r(bz * az))) as f32,
    ]
}

/// FUN_004c6e40: the 3 x 3 matrix of a quaternion.
fn quat_to_matrix(fpu: Fpu, q: [f32; 4]) -> [f32; 9] {
    let r = |x: f64| fpu.r(x);
    let [qx, qy, qz, qw] = q.map(d);
    let x2 = r(qx + qx);
    let y2 = f(r(qy + qy));
    let z2 = f(r(qz + qz));
    let wx = f(r(qw * x2));
    let wy = f(r(qw * y2));
    let wz = f(r(qw * z2));
    let xx = r(x2 * qx);
    let xy = r(qx * y2);
    let xz = f(r(qx * z2));
    let yy = f(r(qy * y2));
    let yz = f(r(qy * z2));
    let zz = r(qz * z2);
    let zzf = f(zz);
    [
        f(r(1.0 - r(zz + yy))),
        f(r(xy - wz)),
        f(r(xz + wy)),
        f(r(xy + wz)),
        f(r(1.0 - r(zzf + xx))),
        f(r(yz - wx)),
        f(r(xz - wy)),
        f(r(yz + wx)),
        f(r(1.0 - r(yy + xx))),
    ]
    .map(|v| v as f32)
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
        let r = |x: f64| self.fpu.r(x);
        let t = libm::tan(r(d(fov) * 0.5));
        let fx = r(f64::from(width) / r(t + t));
        self.fx = fx as f32;
        let ratio =
            r(r(f64::from(width) / f64::from(height)) / r(f64::from(width) / f64::from(height)));
        self.fy = r(ratio * fx) as f32;
        self.inv_fx = r(1.0 / d(self.fx)) as f32;
        self.inv_fy = r(1.0 / d(self.fy)) as f32;
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
        let r = |x: f64| self.fpu.r(x);
        let z = r(1.0 / d(0.0001f32));
        let vx = f(r(r(r(d(sx) - d(self.cx)) * d(self.inv_fx)) * z));
        let vy = f(r(r(r(d(sy) - d(self.cy)) * d(self.inv_fy)) * z));
        let vz = f(z);
        let m = self.matrix().map(d);
        let row = |a: usize| f(r(r(r(vz * m[a + 2]) + r(vy * m[a + 1])) + r(vx * m[a]))) as f32;
        [row(0), row(3), row(6)]
    }

    /// FUN_0045adf0 → FUN_004581d0: the yaw and pitch (radians, floats) of
    /// the screen centre's direction: yaw = atan2(z, x), pitch = atan2(y,
    /// sqrt(z² + x²)).
    pub fn angles(&self) -> (f32, f32) {
        let r = |x: f64| self.fpu.r(x);
        let [x, y, z] = self.ray(self.cx, self.cy).map(d);
        let yaw = libm::atan2(z, x) as f32;
        let pitch = libm::atan2(y, r(r(r(z * z) + r(x * x)).sqrt())) as f32;
        (yaw, pitch)
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
