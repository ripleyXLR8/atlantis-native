//! *Atlantis V*'s camera turns: the camera controller's mode 2, which
//! `Atl_Camera_LookAt(pitch, yaw, true)` and the `centring` fx of
//! `Atl_Address_Go` use, and the free mode's inertia step.
//!
//! Addresses are atlantis5-dvd-int-3891200's.
//!
//! - The camera (the object @ 0x00455590 returns) holds its node at +0x68
//!   whose parent is +0xa8. Its local rotation is q(−yaw about Y) ⊗
//!   q(pitch about X): the pitch setter @ 0x004447a0 and the yaw setter
//!   @ 0x004448c0 set the node's rotation relative to its parent to the yaw
//!   quaternion (@ 0x0048a5a0) and then turn it in its own frame by the
//!   pitch quaternion (@ 0x0048aa90, local ⊗ q). The quaternions come from
//!   the axis-angle matrix @ 0x0048ba50 through @ 0x0048f6c0.
//! - A turn (@ 0x0042cac0) keeps the camera's direction now (A, +0x08) and
//!   the target direction (B, +0x14), both in the parent's frame, sets the
//!   mode to 2 and its time to 0. Each step (@ 0x0042cf80 mode 2) builds the
//!   look rotations of A and B with the up vector (0, −1, 0) (@ 0x0048f7e0
//!   = @ 0x0048bd20 then @ 0x0048f6c0), adds dt to the time, takes
//!   f = t · 4/3 (0x00655730), x = cos(π · f · 0.5 − π · 0.5), the factor
//!   ln(1 + 5 · pow(x, 2)) / ln 6 (0x00655720, 0x00655728, 0x00655718;
//!   `fyl2x` with `fldln2`), the slerp of the two (@ 0x0048f810) at that
//!   factor, the node's rotation set to it, its matrix (@ 0x0048bc40) times
//!   (0, 0, 1) (@ 0x00407820), and from that vector v the pitch
//!   asin(v.y) · −180 / π (@ 0x0062b3a0, 0x00655710) and the yaw
//!   atan2(v.x, v.z) · −180 / π (`fpatan`), each through its setter. The
//!   step reports the end when f ≥ 1 (the last step's factor is the one of
//!   that f, not 1).
//!
//! The arithmetic is single floats where the original stores floats; the
//! x87's extended intermediates are kept in f64.

/// A quaternion (x, y, z, w).
pub type Quat = [f32; 4];
/// A vector.
pub type Vec3 = [f32; 3];

/// π as the program's float (0x00656248).
pub const PI: f32 = std::f32::consts::PI;

/// The rotation matrix (rows) of `angle` radians about `axis`
/// (@ 0x0048ba50: the axis normalised first).
fn axis_angle_matrix(axis: Vec3, angle: f32) -> [f32; 9] {
    let len2 = f64::from(axis[2]) * f64::from(axis[2])
        + f64::from(axis[1]) * f64::from(axis[1])
        + f64::from(axis[0]) * f64::from(axis[0]);
    #[allow(clippy::cast_possible_truncation)]
    let inv = (1.0 / len2.sqrt()) as f32;
    let [x, y, z] = axis.map(|a| inv * a);
    let s = f64::from(angle).sin();
    #[allow(clippy::cast_possible_truncation)]
    let c = f64::from(angle).cos() as f32;
    let t = 1.0 - c;
    #[allow(clippy::cast_possible_truncation)]
    let f = |v: f64| v as f32;
    let xy = f64::from(t) * f64::from(y * x);
    let xz = t * z * x;
    let yz = t * y * z;
    [
        x * x * t + c,
        f(xy - s * f64::from(z)),
        xz + f(f64::from(y) * s),
        f(xy + s * f64::from(z)),
        y * y * t + c,
        yz - f(f64::from(x) * s),
        xz - f(f64::from(y) * s),
        yz + f(f64::from(x) * s),
        z * z * t + c,
    ]
}

/// The quaternion of a rotation matrix (@ 0x0048f6c0; the table
/// @ 0x00677d68 is the next axis: 1, 2, 0).
pub fn matrix_quat(m: &[f32; 9]) -> Quat {
    let tr = m[4] + m[0] + m[8];
    let mut q = [0.0f32; 4];
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt();
        q[3] = s * 0.5;
        let s = 0.5 / s;
        q[0] = (m[7] - m[5]) * s;
        q[1] = (m[2] - m[6]) * s;
        q[2] = (m[3] - m[1]) * s;
        return q;
    }
    let next = [1usize, 2, 0];
    let mut i = usize::from(m[0] < m[4]);
    if m[i * 4] < m[8] {
        i = 2;
    }
    let j = next[i];
    let k = next[j];
    let s = ((m[i * 4] - (m[k * 4] + m[j * 4])) + 1.0).sqrt();
    q[i] = s * 0.5;
    let s = 0.5 / s;
    q[j] = (m[j * 3 + i] + m[i * 3 + j]) * s;
    q[k] = (m[i + k * 3] + m[i * 3 + k]) * s;
    q[3] = (m[k * 3 + j] - m[j * 3 + k]) * s;
    q
}

/// The rotation matrix (rows) of a quaternion (@ 0x0048bc40).
pub fn quat_matrix(q: &Quat) -> [f32; 9] {
    let (x2, y2, z2) = (q[0] + q[0], q[1] + q[1], q[2] + q[2]);
    let wx = q[3] * x2;
    let xx = x2 * q[0];
    [
        1.0 - (q[2] * z2 + q[1] * y2),
        q[0] * y2 - q[3] * z2,
        q[0] * z2 + q[3] * y2,
        q[0] * y2 + q[3] * z2,
        1.0 - (q[2] * z2 + xx),
        q[1] * z2 - wx,
        q[0] * z2 - q[3] * y2,
        q[1] * z2 + wx,
        1.0 - (q[1] * y2 + xx),
    ]
}

/// The product a ⊗ b (@ 0x0048a7c0).
pub fn quat_mul(a: &Quat, b: &Quat) -> Quat {
    [
        (a[1] * b[2] + a[0] * b[3] + a[3] * b[0]) - a[2] * b[1],
        (a[2] * b[0] + a[1] * b[3] + b[1] * a[3]) - b[2] * a[0],
        (b[1] * a[0] + a[2] * b[3] + b[2] * a[3]) - a[1] * b[0],
        ((a[3] * b[3] - b[0] * a[0]) - a[1] * b[1]) - a[2] * b[2],
    ]
}

/// The matrix (rows) times a vector (@ 0x00407820).
pub fn mat_vec(m: &[f32; 9], v: &Vec3) -> Vec3 {
    [
        v[0] * m[0] + v[1] * m[1] + v[2] * m[2],
        v[1] * m[4] + v[2] * m[5] + v[0] * m[3],
        v[1] * m[7] + v[2] * m[8] + v[0] * m[6],
    ]
}

fn cross(a: &Vec3, b: &Vec3) -> Vec3 {
    [
        b[2] * a[1] - b[1] * a[2],
        b[0] * a[2] - b[2] * a[0],
        b[1] * a[0] - b[0] * a[1],
    ]
}

fn normalize(v: &Vec3) -> Vec3 {
    let l2 = f64::from(v[2]) * f64::from(v[2])
        + f64::from(v[1]) * f64::from(v[1])
        + f64::from(v[0]) * f64::from(v[0]);
    #[allow(clippy::cast_possible_truncation)]
    let inv = (1.0 / l2.sqrt()) as f32;
    v.map(|a| inv * a)
}

/// The rotation that turns (0, 0, 1) onto `a` with `up` as the frame's
/// up direction (@ 0x0048f7e0: the basis @ 0x0048bd20 — n = a / |a|,
/// u = n × up normalised (below 1e-05 squared, 0x00656240, another
/// product), v = n × u, the matrix's columns u, v, n — then its
/// quaternion @ 0x0048f6c0).
pub fn look_rotation(a: &Vec3, up: &Vec3) -> Quat {
    let n = normalize(a);
    let mut u = cross(&n, up);
    if u[0] * u[0] + u[1] * u[1] + u[2] * u[2] < 1e-5 {
        // @ 0x0048bd20's other product, as atlantis2's look rotation.
        u = [
            up[0] * n[1] - up[2] * n[2],
            up[1] * n[2] - up[0] * n[0],
            up[2] * n[0] - up[1] * n[1],
        ];
    }
    let u = normalize(&u);
    let v = cross(&n, &u);
    matrix_quat(&[u[0], v[0], n[0], u[1], v[1], n[1], u[2], v[2], n[2]])
}

/// The spherical interpolation of two rotations (@ 0x0048f810): the dot
/// product's sign flips the second; 1 + dot ≤ FLT_EPSILON (0x00656250)
/// takes a perpendicular rotation; 1 − dot ≤ FLT_EPSILON a straight
/// interpolation; else the sines over acos(dot) (@ 0x0062d230).
pub fn slerp(a: &Quat, b: &Quat, t: f32) -> Quat {
    let mut dot = b[3] * a[3] + b[1] * a[1] + b[2] * a[2] + a[0] * b[0];
    let neg = dot < 0.0;
    if neg {
        dot = -dot;
    }
    let eps = f32::EPSILON;
    if dot + 1.0 > eps {
        let (k0, mut k1);
        if 1.0 - dot <= eps {
            k0 = 1.0 - f64::from(t);
            k1 = f64::from(t);
        } else {
            let ang = f64::from(dot).acos();
            #[allow(clippy::cast_possible_truncation)]
            let angf = ang as f32;
            #[allow(clippy::cast_possible_truncation)]
            let s = ang.sin() as f32;
            k0 = ((1.0 - f64::from(t)) * f64::from(angf)).sin() / f64::from(s);
            k1 = (f64::from(angf) * f64::from(t)).sin() / f64::from(s);
        }
        if neg {
            k1 = -k1;
        }
        #[allow(clippy::cast_possible_truncation)]
        let f = |i: usize| (f64::from(b[i]) * k1 + f64::from(a[i]) * k0) as f32;
        return [f(0), f(1), f(2), f(3)];
    }
    // Opposite rotations: (−y, x, −w, z) blended with a by the sines of
    // (0.5 − t) · π and π · t, then normalised.
    let mut c = [-a[1], a[0], -a[3], a[2]];
    let h = (0.5 - f64::from(t)) * f64::from(PI);
    let s0 = if h >= 0.0 { h.sin() } else { (-h).sin() };
    let s1 = (f64::from(PI) * f64::from(t)).sin();
    for i in 0..3 {
        #[allow(clippy::cast_possible_truncation)]
        {
            c[i] = (s1 * f64::from(c[i]) + f64::from(a[i]) * s0) as f32;
        }
    }
    let inv = 1.0 / (c[0] * c[0] + c[3] * c[3] + c[2] * c[2] + c[1] * c[1]).sqrt();
    c.map(|x| inv * x)
}

/// The camera node's local rotation for the angles (degrees), as the
/// setters build it: q(−yaw · π/180 about Y) ⊗ q(pitch · π/180 about X)
/// (0x00655410 = 1/180).
pub fn camera_rotation(pitch: f32, yaw: f32) -> Quat {
    let inv180 = f32::from_bits(0x3bb6_0b61);
    let qy = matrix_quat(&axis_angle_matrix([0.0, 1.0, 0.0], -yaw * PI * inv180));
    let qp = matrix_quat(&axis_angle_matrix([1.0, 0.0, 0.0], pitch * PI * inv180));
    quat_mul(&qy, &qp)
}

/// The quaternion of the angles (x, y, z) radians (@ 0x0048f5f0: halves
/// by 0.5 (0x00655408); cos z/2 and sin y/2 stored as floats, as is
/// sin x/2 in the last two products): q = (s1 c2 c3 − c1 s2 s3,
/// s1 c2 s3 + c1 s2 c3, c1 c2 s3 − s1 s2 c3, c1 c2 c3 + s1 s2 s3).
#[allow(clippy::cast_possible_truncation, clippy::many_single_char_names)]
pub fn euler_quat(x: f32, y: f32, z: f32) -> Quat {
    let half = f64::from(0.5f32);
    let c3 = f64::from((f64::from(z) * half).cos() as f32);
    let s3 = (f64::from(z) * half).sin();
    let c2 = (f64::from(y) * half).cos();
    let s2 = f64::from((f64::from(y) * half).sin() as f32);
    let c1 = (f64::from(x) * half).cos();
    let s1 = (f64::from(x) * half).sin();
    let c1s2 = c1 * s2;
    let s1s2 = f64::from(s1 as f32) * s2;
    [
        (s1 * c2 * c3 - c1s2 * s3) as f32,
        (s1 * c2 * s3 + c1s2 * c3) as f32,
        (c1 * c2 * s3 - s1s2 * c3) as f32,
        (c1 * c2 * c3 + s1s2 * s3) as f32,
    ]
}

/// The camera's direction for the angles, in its parent's frame: the
/// node's rotation times (0, 0, 1) (@ 0x0042cac0 reads it from the node's
/// world transform; under `BoatFx` that frame is the boat's node, [`crate::atlantis5_play`]).
pub fn forward(pitch: f32, yaw: f32) -> Vec3 {
    mat_vec(&quat_matrix(&camera_rotation(pitch, yaw)), &[0.0, 0.0, 1.0])
}

/// The direction `Atl_Camera_LookAt(pitch, yaw, true)` turns to
/// (@ 0x00406f00): the matrix @ 0x0048bb70 of (pitch · π/180, yaw · π ·
/// −1/180 (0x00655414), 0) times (0, 0, 1).
pub fn look_at_direction(pitch: f32, yaw: f32) -> Vec3 {
    let a = f64::from(PI * pitch * f32::from_bits(0x3bb6_0b61));
    let b = f64::from(PI * yaw * f32::from_bits(0xbbb6_0b61));
    // c = 0: sin 0, cos 1.
    #[allow(clippy::cast_possible_truncation)]
    let (sa, ca, sb, cb) = (
        a.sin() as f32,
        a.cos() as f32,
        b.sin() as f32,
        b.cos() as f32,
    );
    [sb * ca, -sa, ca * cb]
}

/// The angles (pitch, yaw, degrees) of a direction as the turn's step
/// reads them: asin(v.y) and atan2(v.x, v.z), each · −180 / π.
pub fn angles_of(v: &Vec3) -> (f32, f32) {
    let m180 = -180.0f64;
    let pi = f64::from(PI);
    #[allow(clippy::cast_possible_truncation)]
    let yaw = (f64::from(v[0]).atan2(f64::from(v[2])) * m180 / pi) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let pitch = (f64::from(v[1]).asin() * m180 / pi) as f32;
    (pitch, yaw)
}

/// A turn of the camera (the controller in mode 2).
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    /// +0x04: the time (seconds).
    pub t: f32,
    /// +0x08: the direction at the start.
    pub from: Vec3,
    /// +0x14: the target direction.
    pub to: Vec3,
}

impl Turn {
    /// A turn from `from` to `to` (@ 0x0042cac0: time 0).
    pub fn new(from: Vec3, to: Vec3) -> Turn {
        Turn { t: 0.0, from, to }
    }

    /// One step of `dt` seconds (@ 0x0042cf80 mode 2): the angles
    /// (pitch, yaw) to set, and whether the turn is over (f ≥ 1).
    pub fn step(&mut self, dt: f32) -> ((f32, f32), bool) {
        let up = [0.0, -1.0, 0.0];
        let qa = look_rotation(&self.from, &up);
        let qb = look_rotation(&self.to, &up);
        self.t = (f64::from(dt) + f64::from(self.t)) as f32;
        let f = self.t * f32::from_bits(0x3faa_aaab);
        let pi = f64::from(PI);
        // `fst` keeps the cosine on the x87 stack; `_CIpow` takes it as a
        // double.
        let x = (pi * f64::from(f) * 0.5 - pi * 0.5).cos();
        let p = x.powf(2.0);
        #[allow(clippy::cast_possible_truncation)]
        let k = ((p * 5.0 + 1.0).ln() / 6.0f64.ln()) as f32;
        let q = slerp(&qa, &qb, k);
        let v = mat_vec(&quat_matrix(&q), &[0.0, 0.0, 1.0]);
        (angles_of(&v), f >= 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, e: f32) -> bool {
        (a - b).abs() <= e
    }

    #[test]
    fn forward_and_angles_agree() {
        for &(p, y) in &[(0.0, 0.0), (10.0, 270.0), (-30.0, 45.0), (60.0, -120.0)] {
            let v = forward(p, y);
            let (p2, y2) = angles_of(&v);
            assert!(close(p, p2, 1e-3), "{p} {p2}");
            let dy = (y - y2).rem_euclid(360.0);
            assert!(dy < 1e-3 || dy > 360.0 - 1e-3, "{y} {y2}");
            // LookAt's own matrix gives the same direction.
            let w = look_at_direction(p, y);
            for i in 0..3 {
                assert!(close(v[i], w[i], 1e-5), "{v:?} {w:?}");
            }
        }
    }

    #[test]
    fn look_rotation_turns_z_onto_the_direction() {
        let d = forward(-20.0, 135.0);
        let q = look_rotation(&d, &[0.0, -1.0, 0.0]);
        let v = mat_vec(&quat_matrix(&q), &[0.0, 0.0, 1.0]);
        for i in 0..3 {
            assert!(close(v[i], d[i], 1e-5));
        }
    }

    #[test]
    fn a_turn_takes_three_quarters_of_a_second() {
        let mut t = Turn::new(forward(0.0, 0.0), forward(0.0, 90.0));
        let ((_, y0), end0) = t.step(0.0);
        assert!(!end0 && close(y0, 0.0, 1e-3));
        let mut n = 0;
        loop {
            n += 1;
            let ((p, y), end) = t.step(1.0 / 60.0);
            assert!(close(p, 0.0, 1e-3));
            assert!((-1e-3..=90.001).contains(&y));
            if end {
                break;
            }
        }
        // f = t · 4/3 reaches 1 at 0.75 s: 45 frames of 1/60 s, or 46 when
        // the float sum of the times falls short.
        assert!((45..=46).contains(&n), "{n}");
    }
}
