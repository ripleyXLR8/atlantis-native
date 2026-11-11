//! The warp's camera (`TeCamera`, perspective type 1) and the view's
//! rotation.
//!
//! - **Projection** (`TeCamera` type 1, atlantis4-gog-en-2363910
//!   @ 0x004f9b90): `f = 1 / tan(fov / 2)` with the tangent stored as a
//!   float, `m[0] = f / (W / H)`, `m[5] = f`, `m[10] = (F + N) / (N − F)`,
//!   `m[11] = F·N / (N − F)`, `m[14] = −1`, the others 0, in memory order;
//!   `glLoadMatrixf` takes it as is (@ 0x004faeb6), column-major, so
//!   `clip = (m0·x, m5·y, m10·z − 1, m11·z)`: the w row is `F·N/(N − F)`
//!   = −4096/4095 instead of −1, and the picture is 4095/4096 of a
//!   standard perspective. The warp's camera has near 1, far 4096
//!   (@ 0x004f476d..0x004f478e) and a vertical FOV of 60° when the game
//!   runs its `HD` definition (a window at least 480 pixels high), else
//!   45° (`ChangeWarp` @ 0x005270da..0x0052723f).
//! - **Rotation**: `GameWarp`'s per-frame update (@ 0x0052ab40) builds
//!   `q = qY(ψ) ⊗ qX(θ)` from the yaw ψ and pitch θ in degrees (half-angle
//!   sines and cosines, the product @ 0x00405bf0); the camera keeps
//!   `conj(q)` (@ 0x004efa20) and its matrix code turns camera space to
//!   the world by the rotation of `q`: yaw 0 looks at −Z, yaw 90 at −X,
//!   a positive pitch looks up.
//! - **`getRay`** (@ 0x004fa030): an integer screen point, y from the
//!   bottom, to `ndc = (2x/W − 1, 2y/H − 1, 1)`, through the inverse
//!   projection, normalised, turned to the world.
//! - **`project`** (@ 0x004f9e40): a world point to `(ndc_x + 1)·⌊W/2⌋`,
//!   `(ndc_y + 1)·⌊H/2⌋` (y from the bottom) and `ndc_z`.

/// The warp camera's near plane (@ 0x004f476d).
pub const NEAR: f32 = 1.0;
/// The warp camera's far plane (float @ 0x005bed98).
pub const FAR: f32 = 4096.0;
/// 60° in radians as the executable stores it (float @ 0x005bee7c,
/// 1.04719758, the float nearest π/3), the FOV of the `HD` definition.
pub const FOV_HD: f32 = std::f32::consts::FRAC_PI_3;
/// 45° (float @ 0x005bee78, 0.785398185, the float nearest π/4), the FOV
/// without the `HD` definition.
pub const FOV_SD: f32 = std::f32::consts::FRAC_PI_4;
/// The window height from which the `HD` definition is chosen
/// (@ 0x00575450; the GOG.com install has only `HD`).
pub const HD_MIN_HEIGHT: u32 = 480;

/// The vertical FOV of a window `height` pixels high: 60° from 480
/// pixels on (the `HD` definition), else 45°.
pub fn fov_for_height(height: u32) -> f32 {
    if height >= HD_MIN_HEIGHT {
        FOV_HD
    } else {
        FOV_SD
    }
}

/// A rotation quaternion `(x, y, z, w)`.
pub type Quat = [f32; 4];

/// The view's rotation for yaw `yaw` and pitch `pitch` in degrees:
/// `qY ⊗ qX` (@ 0x0052aeeb..0x0052b002). The angles go to radians as
/// floats; the half-angle sines and cosines are stored as floats.
pub fn view_quat(yaw: f32, pitch: f32) -> Quat {
    let half = |deg: f32| {
        let rad = (f64::from(deg) * std::f64::consts::PI / 180.0) as f32;
        let h = f64::from(rad) * 0.5;
        (libm::sin(h) as f32, libm::cos(h) as f32)
    };
    let (sy, cy) = half(yaw);
    let (sx, cx) = half(pitch);
    quat_mul([0.0, sy, 0.0, cy], [sx, 0.0, 0.0, cx])
}

/// The Hamilton product `a ⊗ b` (`0x00405bf0` with `ecx = b`, `edx = a`:
/// `w = a.w b.w − a·b`, `xyz = a.w b + b.w a + a × b`).
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a.map(f64::from);
    let [bx, by, bz, bw] = b.map(f64::from);
    [
        (aw * bx + bw * ax + (ay * bz - az * by)) as f32,
        (aw * by + bw * ay + (az * bx - ax * bz)) as f32,
        (aw * bz + bw * az + (ax * by - ay * bx)) as f32,
        (aw * bw - (ax * bx + ay * by + az * bz)) as f32,
    ]
}

/// `0x00417b50`: `q` divided by its length (the sum of squares and its
/// square root stored as floats), unless the length is at most 1e-9.
pub fn normalize(q: Quat) -> Quat {
    let f = q.map(f64::from);
    let sq = (f[1] * f[1] + f[0] * f[0] + f[2] * f[2] + f[3] * f[3]) as f32;
    let len = f64::from(sq).sqrt() as f32;
    if len <= 1e-9_f32 {
        return q;
    }
    q.map(|c| (f64::from(c) / f64::from(len)) as f32)
}

/// The rotation matrix of `q`, rows: `world = R · camera`.
fn rotation(q: Quat) -> [[f64; 3]; 3] {
    let [x, y, z, w] = q.map(f64::from);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// The type-1 projection matrix of `0x004f9b90` (memory order).
pub fn perspective(fov: f32, near: f32, far: f32, width: u32, height: u32) -> [f32; 16] {
    let tan = libm::tan(f64::from(fov) * 0.5) as f32;
    let f = (1.0 / f64::from(tan)) as f32;
    let aspect = f64::from(width) / f64::from(height);
    let (n, fr) = (f64::from(near), f64::from(far));
    let mut m = [0.0f32; 16];
    m[0] = (f64::from(f) / aspect) as f32;
    m[5] = f;
    m[10] = ((fr + n) / (n - fr)) as f32;
    m[11] = (fr * n / (n - fr)) as f32;
    m[14] = -1.0;
    m
}

/// The warp's camera: viewport `(0, 0, width, height)`, its projection
/// and the view's rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub width: u32,
    pub height: u32,
    pub fov: f32,
    pub m: [f32; 16],
    pub q: Quat,
}

impl Camera {
    /// The warp's camera (`0x004f44f0`): near 1, far 4096, the window's
    /// size, looking along yaw `yaw` and pitch `pitch` (degrees).
    pub fn new(fov: f32, width: u32, height: u32, yaw: f32, pitch: f32) -> Camera {
        Camera {
            width,
            height,
            fov,
            m: perspective(fov, NEAR, FAR, width, height),
            // `getRay` normalises the camera's rotation in place
            // (`0x00417b50` @ 0x004fa13d).
            q: normalize(view_quat(yaw, pitch)),
        }
    }

    /// The camera-space direction through the normalised device point
    /// (`nx`, `ny`) on the far side (`ndc_z = 1`): the inverse projection
    /// of `(nx, ny, 1, 1)`, before the division by its w (positive).
    pub fn camera_dir(&self, nx: f64, ny: f64) -> [f64; 3] {
        let m = self.m.map(f64::from);
        let w = m[10] / m[11] - 1.0;
        [nx / m[0] / w, ny / m[5] / w, 1.0 / m[11] / w]
    }

    /// The world direction of a camera-space direction.
    pub fn to_world(&self, d: [f64; 3]) -> [f64; 3] {
        let r = rotation(self.q);
        std::array::from_fn(|i| r[i][0] * d[0] + r[i][1] * d[1] + r[i][2] * d[2])
    }

    /// `getRay` (`0x004fa030`): the world ray through the integer screen
    /// point (`x`, `y_up`), `y_up` counted from the bottom; origin 0,
    /// direction normalised (floats).
    pub fn ray(&self, x: i32, y_up: i32) -> [f32; 3] {
        let nx = 2.0 * f64::from(x) / f64::from(self.width) - 1.0;
        let ny = 2.0 * f64::from(y_up) / f64::from(self.height) - 1.0;
        let d = self.camera_dir(nx, ny);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let n = [d[0] / len, d[1] / len, d[2] / len].map(|c| c as f32);
        self.to_world(n.map(f64::from)).map(|c| c as f32)
    }

    /// `project` (`0x004f9e40`): a world point to the screen, x from the
    /// left and y from the bottom in pixels, and its `ndc_z` (more than 1
    /// behind the camera).
    pub fn project(&self, p: [f32; 3]) -> [f32; 3] {
        let r = rotation(self.q);
        let p = p.map(f64::from);
        // Camera space: the transpose of the world turn.
        let c: [f64; 3] = std::array::from_fn(|j| r[0][j] * p[0] + r[1][j] * p[1] + r[2][j] * p[2]);
        let m = self.m.map(f64::from);
        let clip = [m[0] * c[0], m[5] * c[1], m[10] * c[2] + m[14], m[11] * c[2]];
        let (hx, hy) = (f64::from(self.width / 2), f64::from(self.height / 2));
        [
            ((clip[0] / clip[3] + 1.0) * hx) as f32,
            ((clip[1] / clip[3] + 1.0) * hy) as f32,
            (clip[2] / clip[3]) as f32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_projection_shrinks_by_4095_of_4096() {
        let m = perspective(FOV_HD, NEAR, FAR, 1365, 768);
        assert_eq!(m[5], 1.732_050_9);
        assert_eq!(m[0], 0.974_516_6);
        assert_eq!((m[10], m[11], m[14]), (-1.000_488_4, -1.000_244_3, -1.0));
        assert_eq!(fov_for_height(479), FOV_SD);
        assert_eq!(fov_for_height(480), FOV_HD);
    }

    #[test]
    fn yaw_0_looks_at_minus_z_and_90_at_minus_x() {
        let c = Camera::new(FOV_HD, 800, 600, 0.0, 0.0);
        let d = c.to_world([0.0, 0.0, -1.0]);
        assert!((d[2] + 1.0).abs() < 1e-6);
        let c = Camera::new(FOV_HD, 800, 600, 90.0, 0.0);
        let d = c.to_world([0.0, 0.0, -1.0]);
        assert!((d[0] + 1.0).abs() < 1e-6);
        let c = Camera::new(FOV_HD, 800, 600, 0.0, 30.0);
        let d = c.to_world([0.0, 0.0, -1.0]);
        assert!((d[1] - 0.5).abs() < 1e-6);
    }
}
