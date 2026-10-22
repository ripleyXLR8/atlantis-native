//! The 3D dialog heads: a `.db` scene posed by the body video's frame
//! number, its mouth mixed from morph targets, drawn by the game's own
//! software pipeline over the body video with a Z buffer of its own
//! (`dlg_Anim`, @ 0x00462100 and its methods).
//!
//! What is drawn, per frame (`dlg_Anim::DisplayFace` @ 0x00463460):
//!
//! 1. the eye blink step, which may copy the eyes-closed, half-closed or
//!    open map over the top of the live texture;
//! 2. the object → camera matrix of the head object (@ 0x00571760), the
//!    transform of the mesh's vertices and their clip codes (@ 0x004d00a0);
//! 3. the back-face test (@ 0x004d12c0) and the projection (@ 0x004d20c0);
//! 4. the front faces, in the mesh's order, through the rasteriser
//!    ([`crate::raster3d`]).
//!
//! The pose ([`Head::pose`]) evaluates every node's key-frame tracks at
//! the body video's frame number (@ 0x00463140 → @ 0x004c7810); the mouth
//! is either the lip-sync mix ([`Head::mouth`], @ 0x004631a0 with weights)
//! or, without a voice, the idle babble ([`Head::idle_mouth`]).
//!
//! The arithmetic follows the original's x87 code operation by operation
//! (`f()` a float store, `r()` an x87 result at the [`Fpu`] precision),
//! so that the pixels and the Z buffer are the original's.

use std::collections::BTreeMap;

use atlantis2_data::db::Scene;
use atlantis2_data::tga::Tga;

use crate::crt::Rand;
use crate::fpu::{f, Fpu};
use crate::raster3d::{draw_polygon, Target, Texture, Vertex};

/// The visemes, in the weights' order (@ 0x005dba0c).
pub const VISEMES: &[u8; 7] = b"diaobsv";

/// A quaternion (x, y, z, w) and a vector, as the f32 values the original
/// stores, held in f64.
type Quat = [f64; 4];
type Vec3 = [f64; 3];

fn q64(q: [f32; 4]) -> Quat {
    q.map(f64::from)
}

fn v64(v: [f32; 3]) -> Vec3 {
    v.map(f64::from)
}

/// a ⊗ b (@ 0x004a29c0).
fn qmul(fpu: Fpu, a: &Quat, b: &Quat) -> Quat {
    let r = |x: f64| fpu.r(x);
    let [ax, ay, az, aw] = *a;
    let [bx, by, bz, bw] = *b;
    [
        f(r(r(r(r(ay * bz) + r(bw * ax)) + r(aw * bx)) - r(by * az))),
        f(r(r(r(r(ay * bw) + r(aw * by)) + r(bx * az)) - r(ax * bz))),
        f(r(r(r(r(aw * bz) + r(bw * az)) + r(by * ax)) - r(ay * bx))),
        f(r(r(r(r(aw * bw) - r(bx * ax)) - r(ay * by)) - r(bz * az))),
    ]
}

/// conj(a) ⊗ b (@ 0x004a2c50).
fn qcmul(fpu: Fpu, a: &Quat, b: &Quat) -> Quat {
    let r = |x: f64| fpu.r(x);
    let [ax, ay, az, aw] = *a;
    let [bx, by, bz, bw] = *b;
    [
        f(r(r(r(r(bx * aw) - r(bw * ax)) - r(bz * ay)) + r(by * az))),
        f(r(r(r(r(by * aw) - r(bw * ay)) - r(bx * az)) + r(bz * ax))),
        f(r(r(r(r(bz * aw) - r(bw * az)) - r(by * ax)) + r(bx * ay))),
        f(r(r(r(r(bw * aw) + r(bx * ax)) + r(bz * az)) + r(by * ay))),
    ]
}

/// The rotation matrix of a quaternion, rows (@ 0x004c6e40).
fn qmat(fpu: Fpu, q: &Quat) -> [f64; 9] {
    let r = |x: f64| fpu.r(x);
    let [x, y, z, w] = *q;
    let x2 = r(x + x);
    let y2 = f(r(y + y));
    let z2 = f(r(z + z));
    let wx = f(r(w * x2));
    let wy = f(r(w * y2));
    let wz = f(r(w * z2));
    let xx = r(x2 * x);
    let xy = r(x * y2);
    let xz = f(r(x * z2));
    let yy = f(r(y * y2));
    let yz = f(r(y * z2));
    let zz = r(z * z2);
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
}

/// The quaternion of a rotation matrix (@ 0x004c8a10).
fn matq(fpu: Fpu, m: &[f64; 9]) -> Quat {
    let r = |x: f64| fpu.r(x);
    let trace = r(r(m[4] + m[0]) + m[8]);
    let mut q = [0.0; 4];
    if trace > 0.0 {
        let s = r(r(trace + 1.0).sqrt());
        q[3] = f(r(s * 0.5));
        let s2 = r(0.5 / s);
        q[0] = f(r(r(m[7] - m[5]) * s2));
        q[1] = f(r(r(m[2] - m[6]) * s2));
        q[2] = f(r(r(m[3] - m[1]) * s2));
    } else {
        const NEXT: [usize; 3] = [1, 2, 0]; // @ 0x005e4c88
        let mut i = 0;
        if m[4] > m[0] {
            i = 1;
        }
        if m[8] > m[i * 4] {
            i = 2;
        }
        let j = NEXT[i];
        let k = NEXT[j];
        let s = r(r(r(m[i * 4] - r(m[k * 4] + m[j * 4])) + 1.0).sqrt());
        q[i] = f(r(s * 0.5));
        let s2 = r(0.5 / s);
        q[j] = f(r(r(m[j * 3 + i] + m[i * 3 + j]) * s2));
        q[k] = f(r(r(m[k * 3 + i] + m[i * 3 + k]) * s2));
        q[3] = f(r(r(m[k * 3 + j] - m[j * 3 + k]) * s2));
    }
    q
}

/// acos as the CRT computes it (@ 0x005ae2d0): atan2(sqrt((1 + x)(1 − x)),
/// x) with `fpatan`.
fn acos(fpu: Fpu, x: f64) -> f64 {
    let r = |v: f64| fpu.r(v);
    let s = r(r(r(1.0 + x) * r(1.0 - x)).sqrt());
    libm::atan2(s, x)
}

/// Spherical interpolation of two rotation keys (@ 0x004c8b60).
fn slerp(fpu: Fpu, a: &Quat, b: &Quat, t: f64) -> Quat {
    let r = |x: f64| fpu.r(x);
    let mut dot = f(r(
        r(r(r(a[0] * b[0]) + r(a[2] * b[2])) + r(a[1] * b[1])) + r(a[3] * b[3])
    ));
    let neg = dot < 0.0;
    if neg {
        dot = f(-dot);
    }
    // dot + 1 > FLT_EPSILON always holds here (dot ≥ 0); the original's
    // branch for opposite keys (@ 0x004c8c90) is never taken.
    let eps = f64::from(f32::EPSILON);
    let (k0, mut k1);
    if r(1.0 - dot) > eps {
        let ang = acos(fpu, dot);
        let angf = f(ang);
        let s = f(libm::sin(ang));
        k0 = r(libm::sin(r(r(1.0 - t) * angf)) / s);
        k1 = r(libm::sin(r(angf * t)) / s);
    } else {
        k0 = r(1.0 - t);
        k1 = t;
    }
    if neg {
        k1 = -k1;
    }
    std::array::from_fn(|i| f(r(r(a[i] * k0) + r(b[i] * k1))))
}

/// A node of the scene (0x58 bytes, @ 0x004a1d40): local and world
/// rotation, position and uniform scale; the world values are computed
/// on demand (@ 0x004a33c0).
#[derive(Clone, Debug)]
struct Node {
    name: String,
    parent: Option<usize>,
    lq: Quat,
    lp: Vec3,
    ls: f64,
    wq: Quat,
    wp: Vec3,
    ws: f64,
    valid: bool,
}

/// A node's key-frame track (@ 0x004c7250): (time, value) per channel.
#[derive(Clone, Debug, Default)]
struct Track {
    rotations: Vec<(f64, Quat)>,
    positions: Vec<(f64, Vec3)>,
    scales: Vec<(f64, f64)>,
}

/// The first key whose time is not below `t` (@ 0x004c87d0).
fn lower_bound<T>(keys: &[(f64, T)], t: f64) -> usize {
    keys.partition_point(|k| k.0 < t)
}

/// A camera of the head (0x10c bytes, @ 0x004a4730), set from the `.db`
/// camera by @ 0x004a6470: projection, frustum slopes, depth map and its
/// world pose (a node whose parent is the scene root).
#[derive(Clone, Debug)]
struct Camera {
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
    near: f64,
    far: f64,
    /// Frustum slopes left, top, right, bottom (x/z, y/z).
    slopes: [f64; 4],
    /// Depth map: depth = a − b·w.
    a: f64,
    b: f64,
    q: Quat,
    p: Vec3,
}

/// The dialog camera's reference frame (the camera constructor's 640 x
/// 480, aspect 4:3, centre (320, 240), viewport 0..639 x 0..479).
const REF_W: f64 = 640.0;
const REF_H: f64 = 480.0;

impl Camera {
    fn new(fpu: Fpu, c: &atlantis2_data::db::Camera) -> Camera {
        let r = |x: f64| fpu.r(x);
        // @ 0x004a49c0, mode 1 (perspective).
        let fov = f64::from(c.fov);
        let fx = r(REF_W / r(libm::tan(r(fov * 0.5)) * 2.0));
        let fy = f(r(fx * r(r(REF_W / REF_H) / r(4.0 / 3.0))));
        let fx = f(fx);
        let (cx, cy) = (320.0, 240.0);
        let (near, far) = (f64::from(c.near), f64::from(c.far));
        // @ 0x004a4e8d: the viewport edges ± 1 pixel.
        let slopes = [
            f(r(r(r(0.0 - cx) - 1.0) / fx)),
            f(r(r(r(0.0 - cy) - 1.0) / fy)),
            f(r(r(r(639.0 - cx) + 1.0) / fx)),
            f(r(r(r(479.0 - cy) + 1.0) / fy)),
        ];
        // @ 0x004a5026.
        let a = r(far / r(far - near));
        Camera {
            fx,
            fy,
            cx,
            cy,
            near,
            far,
            slopes,
            a: f(a),
            b: f(r(a * near)),
            q: q64(c.rotation),
            p: v64(c.position),
        }
    }
}

/// Clip codes of a vertex (pipeline + 0x24).
const CLIP_NEAR: u8 = 0x10;
const CLIP_FAR: u8 = 0x20;
const USED: u8 = 0x80;

/// The head mesh as the pipeline reads it.
#[derive(Clone, Debug)]
struct Mesh {
    /// Three vertex indices per face.
    corners: Vec<[u16; 3]>,
    /// Texture coordinates of set 1, three per face.
    uvs: Vec<[[f64; 2]; 3]>,
}

/// What a frame of the pipeline produced, for checks against the
/// original's intermediate buffers.
#[derive(Clone, Debug, Default)]
pub struct Stages {
    /// Camera-space vertices (pipeline + 0x04, x, y, z).
    pub camera: Vec<[f32; 3]>,
    /// Projected vertices (pipeline + 0x14: sx, sy, depth, w), for the
    /// vertices used by a front face.
    pub projected: Vec<[f32; 4]>,
    /// Clip codes (pipeline + 0x24).
    pub codes: Vec<u8>,
    /// Face bytes (pipeline + 0x34): 0 drawn, 0x40 back face.
    pub faces: Vec<u8>,
    /// The object was entirely beyond one plane: nothing drawn.
    pub rejected: bool,
}

/// Why a head could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeadError {
    /// No object whose mesh name holds `-d` and has a mesh record.
    NoHeadMesh,
    /// No camera in the file.
    NoCamera,
}

impl std::fmt::Display for HeadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeadError::NoHeadMesh => write!(f, "no head mesh in the scene"),
            HeadError::NoCamera => write!(f, "no camera in the scene"),
        }
    }
}

impl std::error::Error for HeadError {}

/// A speaker's head for one stand: `dlg_Anim`.
pub struct Head {
    fpu: Fpu,
    nodes: Vec<Node>,
    tracks: Vec<Option<Track>>,
    head: usize,
    mesh: Mesh,
    /// The mesh name (`shh1-d`), characters 3 (mood) and 5 (viseme)
    /// rewritten by the mouth (`dlg_Anim` + 0x12c).
    name: Vec<u8>,
    /// The head mesh's vertices as the mouth leaves them.
    vertices: Vec<[f64; 3]>,
    morphs: BTreeMap<Vec<u8>, Vec<[f64; 3]>>,
    cameras: Vec<Camera>,
    /// The live texture (the open map, with the blinks' copies over it).
    texture: Vec<u16>,
    tex_w: usize,
    tex_h: usize,
    open: Tga,
    closed: Option<Tga>,
    half: Option<Tga>,
    /// Blink state (`dlg_Anim` + 0x150): 1 at the start.
    blink: i32,
    /// Idle mouth accumulator and speed (+0x158, +0x15c).
    idle_acc: f64,
    idle_speed: f64,
}

/// Objects the loader creates: a name with `-d`, or with no `-` at all
/// (@ 0x00462550); the other mouth-shape helpers are left out.
fn kept(name: &str) -> bool {
    name.contains("-d") || !name.contains('-')
}

impl Head {
    /// The texture file of the head mesh's first material (slot 1 of the
    /// material, `dlg_Anim::LoadDataBase` @ 0x004626b0).
    pub fn texture_name(scene: &Scene) -> Option<&str> {
        let (_, mesh) = head_object(scene)?;
        let mat = mesh.materials.first()?;
        scene
            .materials
            .iter()
            .find(|m| &m.name == mat)
            .and_then(|m| m.texture())
    }

    /// The eye maps' names: `<map>yf.<ext>` (eyes closed) and
    /// `<map>ymf.<ext>` (half closed), @ 0x00463a90.
    pub fn eye_map_names(texture: &str) -> (String, String) {
        let (stem, ext) = texture.rsplit_once('.').unwrap_or((texture, ""));
        let dot = if texture.contains('.') { "." } else { "" };
        (
            format!("{stem}yf{dot}{ext}"),
            format!("{stem}ymf{dot}{ext}"),
        )
    }

    /// Builds the head of a `.db` scene with its texture and, when the
    /// files exist, its eye maps (`dlg_Anim::LoadDataBase` @ 0x004626b0).
    pub fn new(
        fpu: Fpu,
        scene: &Scene,
        open: Tga,
        closed: Option<Tga>,
        half: Option<Tga>,
    ) -> Result<Head, HeadError> {
        let (head_name, mesh) = head_object(scene).ok_or(HeadError::NoHeadMesh)?;
        // The objects, depth first from the root, children in name order
        // (the std::map walk), as the loader creates them.
        let mut objects: Vec<&atlantis2_data::db::Object> =
            scene.objects.iter().filter(|o| kept(&o.name)).collect();
        objects.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        let mut nodes = Vec::new();
        let mut stack: Vec<(Option<usize>, &str)> = vec![(None, "")];
        while let Some((parent, pname)) = stack.pop() {
            if nodes.len() > objects.len() {
                break; // a cycle of parent names
            }
            let children: Vec<_> = objects.iter().filter(|o| o.parent == pname).collect();
            let first = nodes.len();
            for o in &children {
                // The local rotation goes through a matrix and back
                // (@ 0x004c6e40 then @ 0x004c89f0 in @ 0x004166d0).
                let lq = matq(fpu, &qmat(fpu, &q64(o.rotation)));
                nodes.push(Node {
                    name: o.name.clone(),
                    parent,
                    lq,
                    lp: v64(o.position),
                    ls: f64::from(o.scale),
                    wq: [0.0; 4],
                    wp: [0.0; 3],
                    ws: 1.0,
                    valid: false,
                });
            }
            for (i, o) in children.iter().enumerate().rev() {
                stack.push((Some(first + i), o.name.as_str()));
            }
        }
        // (The original's list is depth first; the order of the nodes
        // here does not change any value.)
        let tracks = nodes
            .iter()
            .map(|n| scene.node(&n.name).and_then(track))
            .collect();
        let head = nodes
            .iter()
            .position(|n| n.name == head_name)
            .ok_or(HeadError::NoHeadMesh)?;
        let mut morphs = BTreeMap::new();
        if let Some(m) = scene.node("morphing") {
            for (name, verts) in &m.morph_keys {
                morphs.insert(
                    name.as_bytes().to_vec(),
                    verts.iter().map(|v| v64(*v)).collect(),
                );
            }
        }
        let n = mesh.face_count as usize;
        let corners: Vec<[u16; 3]> = (0..n)
            .map(|i| std::array::from_fn(|k| mesh.corners.get(3 * i + k).copied().unwrap_or(0)))
            .collect();
        let uvs = (0..n)
            .map(|i| {
                std::array::from_fn(|k| {
                    mesh.uvs[0]
                        .get(3 * i + k)
                        .map_or([0.0; 2], |uv| [f64::from(uv[0]), f64::from(uv[1])])
                })
            })
            .collect();
        // The cameras in name order (the map walk of @ 0x004626b0).
        let mut cams: Vec<&atlantis2_data::db::Camera> = scene.cameras.iter().collect();
        cams.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        if cams.is_empty() {
            return Err(HeadError::NoCamera);
        }
        Ok(Head {
            fpu,
            nodes,
            tracks,
            head,
            mesh: Mesh { corners, uvs },
            name: mesh.name.as_bytes().to_vec(),
            vertices: mesh.vertices.iter().map(|v| v64(*v)).collect(),
            morphs,
            cameras: cams.into_iter().map(|c| Camera::new(fpu, c)).collect(),
            texture: open.pixels.clone(),
            tex_w: open.width,
            tex_h: open.height,
            open,
            closed,
            half,
            blink: 1,
            idle_acc: 0.0,
            idle_speed: 0.5,
        })
    }

    /// The number of cameras (a line's camera index selects one).
    pub fn camera_count(&self) -> usize {
        self.cameras.len()
    }

    /// The current mesh vertices (as floats).
    pub fn vertices(&self) -> Vec<[f32; 3]> {
        self.vertices.iter().map(|v| v.map(|x| x as f32)).collect()
    }

    /// The current morph-target name (`dlg_Anim` + 0x12c).
    pub fn target_name(&self) -> String {
        self.name.iter().map(|&c| char::from(c)).collect()
    }

    /// The blink state (`dlg_Anim` + 0x150).
    pub fn blink_state(&self) -> i32 {
        self.blink
    }

    /// Sets the blink state, as the 2 % draw does (state 3 starts a
    /// blink).
    pub fn set_blink_state(&mut self, s: i32) {
        self.blink = s;
    }

    /// The live texture.
    pub fn texture(&self) -> (&[u16], usize, usize) {
        (&self.texture, self.tex_w, self.tex_h)
    }

    /// The head object's world rotation, position and scale.
    pub fn head_world(&mut self) -> ([f32; 4], [f32; 3], f32) {
        self.update(self.head);
        let n = &self.nodes[self.head];
        (n.wq.map(|x| x as f32), n.wp.map(|x| x as f32), n.ws as f32)
    }

    /// The camera's projection: fx, fy, slopes, depth map a and b.
    pub fn camera_values(&self, cam: usize) -> ([f32; 2], [f32; 4], [f32; 2]) {
        let c = &self.cameras[cam];
        (
            [c.fx as f32, c.fy as f32],
            c.slopes.map(|x| x as f32),
            [c.a as f32, c.b as f32],
        )
    }

    /// Poses the scene at the body video's frame `frame` (@ 0x00463140):
    /// every node's tracks at time `(float)frame`.
    pub fn pose(&mut self, frame: i32) {
        let t = f64::from(frame as f32);
        for i in 0..self.nodes.len() {
            let Some(tr) = &self.tracks[i] else { continue };
            let fpu = self.fpu;
            let r = |x: f64| fpu.r(x);
            // Rotation (@ 0x004c7810, flag 1).
            let q = if tr.rotations.is_empty() {
                None
            } else {
                let k = lower_bound(&tr.rotations, t);
                Some(if k == tr.rotations.len() {
                    tr.rotations[k - 1].1
                } else if tr.rotations[k].0 == t || k == 0 {
                    tr.rotations[k].1
                } else {
                    let (t0, a) = tr.rotations[k - 1];
                    let (t1, b) = tr.rotations[k];
                    // @ 0x004c7bc0.
                    let u = f(r(r(t - t0) / r(t1 - t0)));
                    slerp(fpu, &a, &b, u)
                })
            };
            // Position (flag 2).
            let p = if tr.positions.is_empty() {
                None
            } else {
                let k = lower_bound(&tr.positions, t);
                Some(if k == tr.positions.len() {
                    tr.positions[k - 1].1
                } else if tr.positions[k].0 == t || k == 0 {
                    tr.positions[k].1
                } else {
                    let (t0, a) = tr.positions[k - 1];
                    let (t1, b) = tr.positions[k];
                    lerp_position(fpu, t0, &a, t1, &b, t)
                })
            };
            // Scale (flag 4).
            let s = if tr.scales.is_empty() {
                None
            } else {
                let k = lower_bound(&tr.scales, t);
                Some(if k == tr.scales.len() {
                    tr.scales[k - 1].1
                } else if tr.scales[k].0 == t || k == 0 {
                    tr.scales[k].1
                } else {
                    let (t0, a) = tr.scales[k - 1];
                    let (t1, b) = tr.scales[k];
                    // @ 0x004c7c20.
                    let v = r(r(r(r(b - a) * r(t - t0)) / r(t1 - t0)) + a);
                    if v == 0.0 {
                        1.0
                    } else {
                        f(v)
                    }
                })
            };
            // Each value is set relative to the node's own parent: a copy
            // (@ 0x004a2860, @ 0x004a2000, @ 0x004a2f40), which marks the
            // node and its descendants stale.
            let n = &mut self.nodes[i];
            if let Some(q) = q {
                n.lq = q;
            }
            if let Some(p) = p {
                n.lp = p;
            }
            if let Some(s) = s {
                n.ls = s;
            }
        }
        for n in &mut self.nodes {
            n.valid = false;
        }
    }

    /// The world values of node `i` (@ 0x004a33c0).
    fn update(&mut self, i: usize) {
        if self.nodes[i].valid {
            return;
        }
        let fpu = self.fpu;
        let r = |x: f64| fpu.r(x);
        let Some(pi) = self.nodes[i].parent else {
            let n = &mut self.nodes[i];
            (n.wq, n.wp, n.ws, n.valid) = (n.lq, n.lp, n.ls, true);
            return;
        };
        self.update(pi);
        let (pq, pp, ps) = {
            let p = &self.nodes[pi];
            (p.wq, p.wp, p.ws)
        };
        let n = &mut self.nodes[i];
        n.wq = qmul(fpu, &pq, &n.lq);
        let [px, py, pz, pw] = pq;
        let a = f(r(ps * n.lp[0]));
        let b = f(r(ps * n.lp[1]));
        let c = r(ps * n.lp[2]);
        let x0 = f(r(r(r(a * pw) - r(b * pz)) + r(c * py)));
        let y0 = f(r(r(r(b * pw) - r(c * px)) + r(a * pz)));
        let z0 = f(r(r(r(c * pw) - r(a * py)) + r(b * px)));
        let w0 = r(r(r(c * pz) + r(b * py)) + r(a * px));
        let x1 = f(r(r(r(r(x0 * pw) + r(w0 * px)) + r(z0 * py)) - r(y0 * pz)));
        let y1 = f(r(r(r(r(x0 * pz) + r(y0 * pw)) + r(w0 * py)) - r(z0 * px)));
        let z1 = r(r(r(r(w0 * pz) + r(z0 * pw)) + r(y0 * px)) - r(x0 * py));
        n.wp = [f(r(pp[0] + x1)), f(r(y1 + pp[1])), f(r(z1 + pp[2]))];
        let s = r(ps * n.ls);
        n.ws = if s == 0.0 { 1.0 } else { f(s) };
        n.valid = true;
    }

    /// The lip-sync mouth (`dlg_Anim::LipSync` @ 0x004631a0 with an
    /// analyser): key `a` of the mood, then a lerp toward each viseme of
    /// positive weight by weight / running sum.
    pub fn mouth(&mut self, mood: u32, weights: &[f32; 7]) {
        let fpu = self.fpu;
        let r = move |x: f64| fpu.r(x);
        self.set_mood(mood);
        self.name[5] = b'a';
        self.copy_key();
        let mut s = 0.0;
        for (c, &w) in weights.iter().enumerate() {
            let w = f64::from(w);
            if w > 0.0 {
                s = f(r(s + w));
                self.name[5] = VISEMES[c];
                let k = f(r(w / s));
                self.lerp_key(k);
            }
        }
    }

    /// A new line starts (@ 0x00463190): the idle mouth's accumulator back
    /// to 0.
    pub fn reset_idle(&mut self) {
        self.idle_acc = 0.0;
    }

    /// The idle mouth (@ 0x004631a0 without an analyser): every call the
    /// accumulator grows by 0.5; when it reaches 1 a random viseme is
    /// chosen; the mesh moves halfway toward the current one.
    pub fn idle_mouth(&mut self, mood: u32, rand: &mut Rand) {
        let fpu = self.fpu;
        let r = move |x: f64| fpu.r(x);
        let acc = r(self.idle_acc + self.idle_speed);
        self.idle_acc = f(acc);
        if acc >= 1.0 {
            self.idle_acc = f(r(acc - 1.0));
            let i = crate::fpu::ftol(r(
                r(f64::from(rand.rand()) * f64::from(3.051_851e-5_f32)) * 6.0
            ));
            self.name[5] = if (0..7).contains(&i) {
                VISEMES[i as usize]
            } else {
                b'a'
            };
        }
        self.set_mood(mood);
        self.lerp_key(self.idle_speed);
    }

    fn set_mood(&mut self, mood: u32) {
        // `_itoa(mood)`'s first character.
        self.name[3] = mood.to_string().as_bytes()[0];
    }

    /// The morph key named by the current name; a missing key leaves the
    /// mesh as it is (the map lookup @ 0x0046a810 then returns the map's
    /// header, whose vertex array is empty under emulation).
    fn key(&self) -> Option<&Vec<[f64; 3]>> {
        self.morphs.get(&self.name)
    }

    /// Copies the key's vertices into the mesh (@ 0x004daea0).
    fn copy_key(&mut self) {
        if let Some(k) = self.morphs.get(&self.name) {
            for (v, t) in self.vertices.iter_mut().zip(k) {
                *v = *t;
            }
        }
    }

    /// Moves the mesh toward the key by `w` (@ 0x004daf20): x stays on
    /// the x87 stack, y and z go through a float between the steps.
    fn lerp_key(&mut self, w: f64) {
        let fpu = self.fpu;
        let r = move |x: f64| fpu.r(x);
        let Some(k) = self.key().cloned() else { return };
        for (v, t) in self.vertices.iter_mut().zip(&k) {
            let nx = f(r(r(r(t[0] - v[0]) * w) + v[0]));
            let dy = f(r(t[1] - v[1]));
            let dz = f(r(t[2] - v[2]));
            let ny = f(r(f(r(dy * w)) + v[1]));
            let nz = f(r(f(r(dz * w)) + v[2]));
            *v = [nx, ny, nz];
        }
    }

    /// The blink step of `DisplayFace` (@ 0x004634cf), run when the three
    /// maps exist.
    fn blink_step(&mut self, rand: &mut Rand) {
        let (Some(closed), Some(half)) = (&self.closed, &self.half) else {
            return;
        };
        let blit = |tex: &mut Vec<u16>, w: usize, h: usize, src: &Tga| {
            // @ 0x004b6cc0: the map copied at (0, 0) of the live texture.
            for y in 0..src.height.min(h) {
                let n = src.width.min(w);
                tex[y * w..y * w + n]
                    .copy_from_slice(&src.pixels[y * src.width..y * src.width + n]);
            }
        };
        let (w, h) = (self.tex_w, self.tex_h);
        match self.blink {
            0 => {
                let p = self
                    .fpu
                    .r(f64::from(rand.rand()) * f64::from(3.051_851e-5_f32));
                if p <= f64::from(0.02_f32) {
                    self.blink = 3;
                }
            }
            1 => {
                blit(&mut self.texture, w, h, &self.open);
                self.blink = -25;
            }
            2 => {
                blit(&mut self.texture, w, h, half);
                self.blink = 1;
            }
            3 => {
                blit(&mut self.texture, w, h, closed);
                self.blink = 2;
            }
            4 => {
                blit(&mut self.texture, w, h, half);
                self.blink = 3;
            }
            _ => self.blink += 1,
        }
    }

    /// `dlg_Anim::DisplayFace` (@ 0x00463460): the blink step, then the
    /// head drawn with camera `cam` into a 640 x 480 RGB565 frame and its
    /// Z buffer (which the dialog clears to 0 every frame).
    pub fn display(
        &mut self,
        cam: usize,
        rand: &mut Rand,
        pixels: &mut [u16],
        zbuf: &mut [u32],
    ) -> Stages {
        self.blink_step(rand);
        if cam >= self.cameras.len() {
            // `numéro de caméra non valide`
            log::warn!("head camera {cam} out of range");
            return Stages::default();
        }
        self.draw(cam, pixels, zbuf)
    }

    fn draw(&mut self, cam: usize, pixels: &mut [u16], zbuf: &mut [u32]) -> Stages {
        let fpu = self.fpu;
        let r = |x: f64| fpu.r(x);
        self.update(self.head);
        let c = self.cameras[cam].clone();
        let n = &self.nodes[self.head];
        // The object → camera matrix (@ 0x00571760, mode 7); the camera
        // node is the camera's world pose, scale 1.
        let cs = 1.0;
        let is = f(r(1.0 / cs));
        let dx = r(n.wp[0] - c.p[0]);
        let dy = f(r(n.wp[1] - c.p[1]));
        let dz = f(r(n.wp[2] - c.p[2]));
        let (x, y, z) = (r(dx * is), r(dy * is), r(dz * is));
        let [qx, qy, qz, qw] = c.q;
        let tw = f(r(r(r(-r(x * qx)) - r(y * qy)) - r(z * qz)));
        let tz = f(r(r(r(z * qw) + r(x * qy)) - r(y * qx)));
        let ty = f(r(r(r(z * qx) + r(y * qw)) - r(x * qz)));
        let tx = f(r(r(r(y * qz) + r(x * qw)) - r(z * qy)));
        let t = [
            f(r(r(r(r(tx * qw) - r(tw * qx)) - r(tz * qy)) + r(ty * qz))),
            f(r(r(r(r(ty * qw) - r(tw * qy)) - r(tx * qz)) + r(tz * qx))),
            f(r(r(r(r(tz * qw) - r(tw * qz)) - r(ty * qx)) + r(tx * qy))),
        ];
        let factor = r(n.ws * is);
        let m = qmat(fpu, &qcmul(fpu, &c.q, &n.wq)).map(|e| f(r(factor * e)));
        // The transform and the clip codes (@ 0x004d00a0, mode 1).
        let nv = self.vertices.len();
        let mut st = Stages {
            camera: Vec::with_capacity(nv),
            codes: Vec::with_capacity(nv),
            ..Stages::default()
        };
        let mut cv = Vec::with_capacity(nv);
        let (mut or, mut and) = (0u8, 0xffu8);
        for v in &self.vertices {
            let row = |k: usize| {
                f(r(r(
                    r(r(v[1] * m[3 * k + 1]) + r(v[0] * m[3 * k])) + r(v[2] * m[3 * k + 2])
                ) + t[k]))
            };
            let p = [row(0), row(1), row(2)];
            let mut code = if p[2] < c.near {
                CLIP_NEAR
            } else if p[2] > c.far {
                CLIP_FAR
            } else {
                0
            };
            if p[0] < r(p[2] * c.slopes[0]) {
                code |= 1;
            } else if p[0] > r(p[2] * c.slopes[2]) {
                code |= 4;
            }
            if p[1] < r(p[2] * c.slopes[1]) {
                code |= 2;
            } else if p[1] > r(p[2] * c.slopes[3]) {
                code |= 8;
            }
            or |= code;
            and &= code;
            st.camera.push(p.map(|x| x as f32));
            st.codes.push(code);
            cv.push(p);
        }
        if and & 0x3f != 0 {
            st.rejected = true;
            return st;
        }
        if or != 0 {
            // The original then takes another back-face routine
            // (@ 0x004d1070) and clips faces crossing the near or far
            // plane; no head frame of the game needs either.
            log::debug!("head partly outside the view (clip codes {or:#x})");
        }
        // Back faces (@ 0x004d12c0): N = (c1 − c0) × (c2 − c0), drawn when
        // N · c0 < 0; the used vertices' codes become 0x80.
        st.faces = Vec::with_capacity(self.mesh.corners.len());
        for tri in &self.mesh.corners {
            let [c0, c1, c2] = tri.map(|i| cv.get(i as usize).copied().unwrap_or([0.0; 3]));
            let a = [
                f(r(c2[0] - c0[0])),
                f(r(c2[1] - c0[1])),
                f(r(c2[2] - c0[2])),
            ];
            let b = [
                f(r(c1[0] - c0[0])),
                f(r(c1[1] - c0[1])),
                f(r(c1[2] - c0[2])),
            ];
            let nz = f(r(r(a[1] * b[0]) - r(a[0] * b[1])));
            let ny = f(r(r(b[2] * a[0]) - r(a[2] * b[0])));
            let nx = f(r(r(a[2] * b[1]) - r(b[2] * a[1])));
            let dot = r(r(r(c0[2] * nz) + r(c0[1] * ny)) + r(nx * c0[0]));
            if dot < 0.0 {
                st.faces.push(0);
                for &i in tri {
                    if let Some(code) = st.codes.get_mut(i as usize) {
                        *code = USED;
                    }
                }
            } else {
                st.faces.push(0x40);
            }
        }
        // The projection (@ 0x004d20c0) of the used vertices.
        let mut proj = vec![[0.0f64; 4]; nv];
        st.projected = vec![[0.0f32; 4]; nv];
        for (i, p) in cv.iter().enumerate() {
            if st.codes[i] & 0x90 != USED {
                continue;
            }
            let w = r(1.0 / p[2]);
            let sx = f(r(r(r(c.fx * p[0]) * w) + c.cx));
            let sy = f(r(r(r(c.fy * p[1]) * w) + c.cy));
            let depth = f(r(c.a - r(w * c.b)));
            proj[i] = [sx, sy, depth, f(w)];
            st.projected[i] = proj[i].map(|x| x as f32);
        }
        // The faces (@ 0x004380b0): one group, the texture of its
        // material; three vertex records (sx, sy, w, u, v) per front face.
        let tex = Texture {
            pixels: &self.texture,
            width: self.tex_w,
            height: self.tex_h,
        };
        let mut target = Target { pixels, zbuf };
        for (fi, tri) in self.mesh.corners.iter().enumerate() {
            if st.faces[fi] != 0 {
                continue;
            }
            let mut poly: [Vertex; 3] = std::array::from_fn(|k| {
                let p = proj[tri[k] as usize];
                let uv = self.mesh.uvs[fi][k];
                Vertex {
                    x: p[0],
                    y: p[1],
                    w: p[3],
                    u: uv[0],
                    v: uv[1],
                }
            });
            draw_polygon(fpu, &mut poly, &tex, &mut target);
        }
        st
    }
}

/// The head object and its mesh: the first object, depth first, whose
/// mesh exists and whose mesh name holds `-d` (@ 0x004626b0 → +0x128).
fn head_object(scene: &Scene) -> Option<(String, &atlantis2_data::db::Mesh)> {
    let mut objects: Vec<_> = scene.objects.iter().filter(|o| kept(&o.name)).collect();
    objects.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    // Depth-first order.
    let mut order = Vec::new();
    let mut stack = vec![""];
    while let Some(p) = stack.pop() {
        if order.len() > objects.len() {
            break;
        }
        let children: Vec<_> = objects.iter().filter(|o| o.parent == p).collect();
        for o in children.iter().rev() {
            stack.push(o.name.as_str());
        }
        if !p.is_empty() {
            order.push(p);
        }
    }
    order.iter().find_map(|name| {
        let o = scene.object(name)?;
        let m = scene.mesh(&o.mesh)?;
        o.mesh.contains("-d").then(|| (o.name.clone(), m))
    })
}

/// A node's track from its `.db` record (@ 0x004c7250): the TCB keys'
/// values, or the version-11 linear keys when there is no TCB key.
fn track(d: &atlantis2_data::db::Node) -> Option<Track> {
    let tcb =
        !(d.rotation_keys.is_empty() && d.position_keys.is_empty() && d.scale_keys.is_empty());
    let scale = |s: f32| if s == 0.0 { 1.0 } else { f64::from(s) };
    let t = if tcb {
        Track {
            rotations: d
                .rotation_keys
                .iter()
                .map(|(t, q)| (f64::from(*t as f32), q64(*q)))
                .collect(),
            positions: d
                .position_keys
                .iter()
                .map(|(t, p)| (f64::from(*t as f32), v64(*p)))
                .collect(),
            scales: d
                .scale_keys
                .iter()
                .map(|(t, s)| (f64::from(*t as f32), scale(*s)))
                .collect(),
        }
    } else {
        Track {
            rotations: d
                .linear_rotation_keys
                .iter()
                .map(|(t, q)| (f64::from(*t as f32), q64(*q)))
                .collect(),
            positions: d
                .linear_position_keys
                .iter()
                .map(|(t, p)| (f64::from(*t as f32), v64(*p)))
                .collect(),
            scales: d
                .linear_scale_keys
                .iter()
                .map(|(t, s)| (f64::from(*t as f32), scale(s[0])))
                .collect(),
        }
    };
    let empty = t.rotations.is_empty() && t.positions.is_empty() && t.scales.is_empty();
    (!empty).then_some(t)
}

/// Linear interpolation of two position keys (@ 0x004c7b10).
fn lerp_position(fpu: Fpu, t0: f64, a: &Vec3, t1: f64, b: &Vec3, t: f64) -> Vec3 {
    let r = |x: f64| fpu.r(x);
    let dx = r(b[0] - a[0]);
    let dy = f(r(b[1] - a[1]));
    let dz = f(r(b[2] - a[2]));
    let dt = f(r(t - t0));
    let x = r(dx * dt);
    let py = f(r(dy * dt));
    let pz = f(r(dz * dt));
    let inv = r(1.0 / r(t1 - t0));
    let invf = f(inv);
    let x = r(x * inv);
    let y = f(r(py * invf));
    let z = f(r(pz * invf));
    [f(r(x + a[0])), f(r(y + a[1])), f(r(z + a[2]))]
}
