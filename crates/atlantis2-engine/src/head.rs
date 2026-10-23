//! The 3D dialog heads: a `.db` scene posed by the body video's frame
//! number, its mouth mixed from morph targets, drawn by the game's own
//! software pipeline over the body video with a Z buffer of its own
//! (`dlg_Anim`, @ 0x00462100 and its methods).
//!
//! What is drawn, per frame (`dlg_Anim::DisplayFace` @ 0x00463460), with
//! the routines of the CPU type ([`Variant`], the warp's: the pipeline
//! @ 0x004d2770 picks them from the same CPU type):
//!
//! 1. the eye blink step, which may copy the eyes-closed, half-closed or
//!    open map over the top of the live texture;
//! 2. the object → camera matrix of the head object, the transform of the
//!    mesh's vertices and their clip codes: C (@ 0x00571760,
//!    @ 0x004d00a0), or the x87 assembly of CPU type 8 (@ 0x00571bd0 →
//!    @ 0x0057eec0, with its own world update @ 0x00584340);
//! 3. the back-face test and the projection: C (@ 0x004d12c0,
//!    @ 0x004d20c0) or assembly (@ 0x00572030, @ 0x00572080);
//! 4. the front faces, in the mesh's order, through the rasteriser: the C
//!    one ([`crate::raster3d`]) or the Pentium II's
//!    ([`crate::raster3d::ii`]), which leaves the x87 control word at
//!    0x0e7f.
//!
//! The pose ([`Head::pose`]) evaluates every node's key-frame tracks at
//! the body video's frame number (@ 0x00463140 → @ 0x004c7810); the mouth
//! is either the lip-sync mix ([`Head::mouth`], @ 0x004631a0 with weights)
//! or, without a voice, the idle babble ([`Head::idle_mouth`]).
//!
//! The arithmetic follows the original's x87 code operation by operation
//! under the control word in force ([`Fpu`]: `c.f()` a float store,
//! `c.add()` and the like an x87 operation), so that the pixels and the Z
//! buffer are the original's.

use std::collections::BTreeMap;

use atlantis2_data::db::Scene;
use atlantis2_data::tga::Tga;

use crate::crt::Rand;
use crate::fpu::Fpu;
use crate::raster3d::{draw_polygon, ii, Target, Texture, Vertex};
use crate::warp::Variant;

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
fn qmul(c: Fpu, a: &Quat, b: &Quat) -> Quat {
    let m = |x, y| c.mul(x, y);
    let [ax, ay, az, aw] = *a;
    let [bx, by, bz, bw] = *b;
    [
        c.f(c.sub(c.add(c.add(m(ay, bz), m(bw, ax)), m(aw, bx)), m(by, az))),
        c.f(c.sub(c.add(c.add(m(ay, bw), m(aw, by)), m(bx, az)), m(ax, bz))),
        c.f(c.sub(c.add(c.add(m(aw, bz), m(bw, az)), m(by, ax)), m(ay, bx))),
        c.f(c.sub(c.sub(c.sub(m(aw, bw), m(bx, ax)), m(ay, by)), m(bz, az))),
    ]
}

/// conj(a) ⊗ b (@ 0x004a2c50).
fn qcmul(c: Fpu, a: &Quat, b: &Quat) -> Quat {
    let m = |x, y| c.mul(x, y);
    let [ax, ay, az, aw] = *a;
    let [bx, by, bz, bw] = *b;
    [
        c.f(c.add(c.sub(c.sub(m(bx, aw), m(bw, ax)), m(bz, ay)), m(by, az))),
        c.f(c.add(c.sub(c.sub(m(by, aw), m(bw, ay)), m(bx, az)), m(bz, ax))),
        c.f(c.add(c.sub(c.sub(m(bz, aw), m(bw, az)), m(by, ax)), m(bx, ay))),
        c.f(c.add(c.add(c.add(m(bw, aw), m(bx, ax)), m(bz, az)), m(by, ay))),
    ]
}

/// The rotation matrix of a quaternion, rows (@ 0x004c6e40).
fn qmat(c: Fpu, q: &Quat) -> [f64; 9] {
    let [x, y, z, w] = *q;
    let x2 = c.add(x, x);
    let y2 = c.f(c.add(y, y));
    let z2 = c.f(c.add(z, z));
    let wx = c.f(c.mul(w, x2));
    let wy = c.f(c.mul(w, y2));
    let wz = c.f(c.mul(w, z2));
    let xx = c.mul(x2, x);
    let xy = c.mul(x, y2);
    let xz = c.f(c.mul(x, z2));
    let yy = c.f(c.mul(y, y2));
    let yz = c.f(c.mul(y, z2));
    let zz = c.mul(z, z2);
    let zzf = c.f(zz);
    [
        c.f(c.sub(1.0, c.add(zz, yy))),
        c.f(c.sub(xy, wz)),
        c.f(c.add(xz, wy)),
        c.f(c.add(xy, wz)),
        c.f(c.sub(1.0, c.add(zzf, xx))),
        c.f(c.sub(yz, wx)),
        c.f(c.sub(xz, wy)),
        c.f(c.add(yz, wx)),
        c.f(c.sub(1.0, c.add(yy, xx))),
    ]
}

/// The quaternion of a rotation matrix (@ 0x004c8a10).
fn matq(c: Fpu, m: &[f64; 9]) -> Quat {
    let trace = c.add(c.add(m[4], m[0]), m[8]);
    let mut q = [0.0; 4];
    if trace > 0.0 {
        let s = c.sqrt(c.add(trace, 1.0));
        q[3] = c.f(c.mul(s, 0.5));
        let s2 = c.div(0.5, s);
        q[0] = c.f(c.mul(c.sub(m[7], m[5]), s2));
        q[1] = c.f(c.mul(c.sub(m[2], m[6]), s2));
        q[2] = c.f(c.mul(c.sub(m[3], m[1]), s2));
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
        let s = c.sqrt(c.add(c.sub(m[i * 4], c.add(m[k * 4], m[j * 4])), 1.0));
        q[i] = c.f(c.mul(s, 0.5));
        let s2 = c.div(0.5, s);
        q[j] = c.f(c.mul(c.add(m[j * 3 + i], m[i * 3 + j]), s2));
        q[k] = c.f(c.mul(c.add(m[k * 3 + i], m[i * 3 + k]), s2));
        q[3] = c.f(c.mul(c.sub(m[k * 3 + j], m[j * 3 + k]), s2));
    }
    q
}

/// acos as the CRT computes it (@ 0x005ae2d0): atan2(sqrt((1 + x)(1 − x)),
/// x) with `fpatan`, under the control word with the rounding to nearest
/// (@ 0x005b5d95 sets (word & 0x300) | 0x7f and @ 0x005b5e1e restores
/// the caller's).
fn acos(fpu: Fpu, x: f64) -> f64 {
    let c = fpu.nearest();
    let s = c.sqrt(c.mul(c.add(1.0, x), c.sub(1.0, x)));
    libm::atan2(s, x)
}

/// Spherical interpolation of two rotation keys (@ 0x004c8b60).
fn slerp(c: Fpu, a: &Quat, b: &Quat, t: f64) -> Quat {
    let m = |x, y| c.mul(x, y);
    let mut dot = c.f(c.add(
        c.add(c.add(m(a[0], b[0]), m(a[2], b[2])), m(a[1], b[1])),
        m(a[3], b[3]),
    ));
    let neg = dot < 0.0;
    if neg {
        dot = c.f(-dot);
    }
    // dot + 1 > FLT_EPSILON always holds here (dot ≥ 0); the original's
    // branch for opposite keys (@ 0x004c8c90) is never taken.
    let eps = f64::from(f32::EPSILON);
    let (k0, mut k1);
    if c.sub(1.0, dot) > eps {
        let ang = acos(c, dot);
        let angf = c.f(ang);
        let s = c.f(libm::sin(ang));
        k0 = c.div(libm::sin(c.mul(c.sub(1.0, t), angf)), s);
        k1 = c.div(libm::sin(c.mul(angf, t)), s);
    } else {
        k0 = c.sub(1.0, t);
        k1 = t;
    }
    if neg {
        k1 = -k1;
    }
    std::array::from_fn(|i| c.f(c.add(m(a[i], k0), m(b[i], k1))))
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
        let k = fpu;
        // @ 0x004a49c0, mode 1 (perspective).
        let fov = f64::from(c.fov);
        let fx = k.div(REF_W, k.mul(libm::tan(k.mul(fov, 0.5)), 2.0));
        let fy = k.f(k.mul(fx, k.div(k.div(REF_W, REF_H), k.div(4.0, 3.0))));
        let fx = k.f(fx);
        let (cx, cy) = (320.0, 240.0);
        let (near, far) = (f64::from(c.near), f64::from(c.far));
        // @ 0x004a4e8d: the viewport edges ± 1 pixel.
        let slopes = [
            k.f(k.div(k.sub(k.sub(0.0, cx), 1.0), fx)),
            k.f(k.div(k.sub(k.sub(0.0, cy), 1.0), fy)),
            k.f(k.div(k.add(k.sub(639.0, cx), 1.0), fx)),
            k.f(k.div(k.add(k.sub(479.0, cy), 1.0), fy)),
        ];
        // @ 0x004a5026.
        let a = k.div(far, k.sub(far, near));
        Camera {
            fx,
            fy,
            cx,
            cy,
            near,
            far,
            slopes,
            a: k.f(a),
            b: k.f(k.mul(a, near)),
            q: q64(c.rotation),
            p: v64(c.position),
        }
    }

    /// The HD mode (a port addition): the same camera for a frame `tile`
    /// / 16 times 640 x 480 (`w` x `h` pixels), the screen point (sx, sy)
    /// of the original becoming (s·sx, s·sy): fx, fy, cx and cy times s
    /// (s = tile / 16), the frustum slopes of @ 0x004a4e8d from that
    /// frame's viewport (0..w − 1, 0..h − 1), the pose and the depth map
    /// unchanged. At tile 16 it is the camera itself.
    fn scaled(&self, fpu: Fpu, tile: usize, w: usize, h: usize) -> Camera {
        let k = fpu;
        let s = tile as f64 / 16.0;
        let (fx, fy) = (k.f(k.mul(self.fx, s)), k.f(k.mul(self.fy, s)));
        let (cx, cy) = (k.mul(self.cx, s), k.mul(self.cy, s));
        let (x1, y1) = ((w - 1) as f64, (h - 1) as f64);
        let slopes = [
            k.f(k.div(k.sub(k.sub(0.0, cx), 1.0), fx)),
            k.f(k.div(k.sub(k.sub(0.0, cy), 1.0), fy)),
            k.f(k.div(k.add(k.sub(x1, cx), 1.0), fx)),
            k.f(k.div(k.add(k.sub(y1, cy), 1.0), fy)),
        ];
        Camera {
            fx,
            fy,
            cx,
            cy,
            slopes,
            ..self.clone()
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
    /// The x87 control word the head's float code runs under now: the
    /// caller's, then 0x0e7f once the II rasteriser has drawn.
    fpu: Fpu,
    /// The CPU's routines: the II rasteriser with the x87 assembly
    /// transform (CPU type 8), or the C ones (CPU type 0).
    variant: Variant,
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
        variant: Variant,
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
            variant,
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

    /// The x87 control word the head's code runs under now.
    pub fn fpu(&self) -> Fpu {
        self.fpu
    }

    /// The control word the caller's code has now (the head's next
    /// operations run under it).
    pub fn set_fpu(&mut self, fpu: Fpu) {
        self.fpu = fpu;
    }

    /// The routines the head is drawn with.
    pub fn variant(&self) -> Variant {
        self.variant
    }

    /// The head object's world rotation, position and scale, as the
    /// variant's transform computes them.
    pub fn head_world(&mut self) -> ([f32; 4], [f32; 3], f32) {
        match self.variant {
            Variant::C => self.update(self.head),
            Variant::PII => self.update_asm(self.head),
        }
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
            let c = self.fpu;
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
                    let u = c.f(c.div(c.sub(t, t0), c.sub(t1, t0)));
                    slerp(c, &a, &b, u)
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
                    lerp_position(c, t0, &a, t1, &b, t)
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
                    let v = c.add(c.div(c.mul(c.sub(b, a), c.sub(t, t0)), c.sub(t1, t0)), a);
                    if v == 0.0 {
                        1.0
                    } else {
                        c.f(v)
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
        let c = self.fpu;
        let m = |x, y| c.mul(x, y);
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
        n.wq = qmul(c, &pq, &n.lq);
        let [px, py, pz, pw] = pq;
        let a = c.f(m(ps, n.lp[0]));
        let b = c.f(m(ps, n.lp[1]));
        let cc = m(ps, n.lp[2]);
        let x0 = c.f(c.add(c.sub(m(a, pw), m(b, pz)), m(cc, py)));
        let y0 = c.f(c.add(c.sub(m(b, pw), m(cc, px)), m(a, pz)));
        let z0 = c.f(c.add(c.sub(m(cc, pw), m(a, py)), m(b, px)));
        let w0 = c.add(c.add(m(cc, pz), m(b, py)), m(a, px));
        let x1 = c.f(c.sub(c.add(c.add(m(x0, pw), m(w0, px)), m(z0, py)), m(y0, pz)));
        let y1 = c.f(c.sub(c.add(c.add(m(x0, pz), m(y0, pw)), m(w0, py)), m(z0, px)));
        let z1 = c.sub(c.add(c.add(m(w0, pz), m(z0, pw)), m(y0, px)), m(x0, py));
        n.wp = [
            c.f(c.add(pp[0], x1)),
            c.f(c.add(y1, pp[1])),
            c.f(c.add(z1, pp[2])),
        ];
        let s = m(ps, n.ls);
        n.ws = if s == 0.0 { 1.0 } else { c.f(s) };
        n.valid = true;
    }

    /// The world values of node `i` as the x87 assembly computes them
    /// (@ 0x00584340, called by the type-8 transform for the camera and the
    /// object nodes): a child of the root takes its local values; else, from
    /// the parent's world values (updated first), no float store in
    /// between and no rule for a zero scale.
    fn update_asm(&mut self, i: usize) {
        if self.nodes[i].valid {
            return;
        }
        let c = self.fpu;
        let m = |x, y| c.mul(x, y);
        let Some(pi) = self.nodes[i].parent else {
            let n = &mut self.nodes[i];
            (n.wq, n.wp, n.ws, n.valid) = (n.lq, n.lp, n.ls, true);
            return;
        };
        self.update_asm(pi);
        let (pq, pp, ps) = {
            let p = &self.nodes[pi];
            (p.wq, p.wp, p.ws)
        };
        let n = &mut self.nodes[i];
        let [px, py, pz, pw] = pq;
        n.ws = c.f(m(ps, n.ls));
        let (a, b, cc) = (m(n.lp[0], ps), m(n.lp[1], ps), m(n.lp[2], ps));
        let x0 = c.add(c.sub(m(a, pw), m(pz, b)), m(py, cc));
        let y0 = c.add(c.sub(m(b, pw), m(px, cc)), m(pz, a));
        let z0 = c.add(c.sub(m(cc, pw), m(py, a)), m(px, b));
        let w0 = c.add(m(a, px), c.add(m(b, py), m(cc, pz)));
        n.wp = [
            c.f(c.add(
                c.add(m(x0, pw), c.sub(c.add(m(px, w0), m(py, z0)), m(pz, y0))),
                pp[0],
            )),
            c.f(c.add(
                c.add(m(y0, pw), c.sub(c.add(m(py, w0), m(pz, x0)), m(px, z0))),
                pp[1],
            )),
            c.f(c.add(
                c.add(m(z0, pw), c.sub(c.add(m(w0, pz), m(px, y0)), m(py, x0))),
                pp[2],
            )),
        ];
        let [lx, ly, lz, lw] = n.lq;
        n.wq = [
            c.f(c.sub(c.add(c.add(m(lx, pw), m(px, lw)), m(py, lz)), m(ly, pz))),
            c.f(c.sub(c.add(c.add(m(ly, pw), m(py, lw)), m(lx, pz)), m(px, lz))),
            c.f(c.sub(c.add(c.add(m(lz, pw), m(lw, pz)), m(ly, px)), m(py, lx))),
            c.f(c.sub(c.sub(c.sub(m(pw, lw), m(px, lx)), m(py, ly)), m(pz, lz))),
        ];
        n.valid = true;
    }

    /// The lip-sync mouth (`dlg_Anim::LipSync` @ 0x004631a0 with an
    /// analyser): key `a` of the mood, then a lerp toward each viseme of
    /// positive weight by weight / running sum.
    pub fn mouth(&mut self, mood: u32, weights: &[f32; 7]) {
        let c = self.fpu;
        self.set_mood(mood);
        self.name[5] = b'a';
        self.copy_key();
        let mut s = 0.0;
        for (i, &w) in weights.iter().enumerate() {
            let w = f64::from(w);
            if w > 0.0 {
                s = c.f(c.add(s, w));
                self.name[5] = VISEMES[i];
                let k = c.f(c.div(w, s));
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
        let c = self.fpu;
        let acc = c.add(self.idle_acc, self.idle_speed);
        self.idle_acc = c.f(acc);
        if acc >= 1.0 {
            self.idle_acc = c.f(c.sub(acc, 1.0));
            let i = crate::fpu::ftol(c.mul(
                c.mul(f64::from(rand.rand()), f64::from(3.051_851e-5_f32)),
                6.0,
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
        let c = self.fpu;
        let Some(k) = self.key().cloned() else { return };
        for (v, t) in self.vertices.iter_mut().zip(&k) {
            let nx = c.f(c.add(c.mul(c.sub(t[0], v[0]), w), v[0]));
            let dy = c.f(c.sub(t[1], v[1]));
            let dz = c.f(c.sub(t[2], v[2]));
            let ny = c.f(c.add(c.f(c.mul(dy, w)), v[1]));
            let nz = c.f(c.add(c.f(c.mul(dz, w)), v[2]));
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
                    .mul(f64::from(rand.rand()), f64::from(3.051_851e-5_f32));
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
        let c = self.cameras[cam].clone();
        self.draw_with(&c, &mut Target::screen(pixels, zbuf))
    }

    /// The HD mode (a port addition, the first game's HD heads): the head
    /// as [`Head::display`] last drew it (the same pose, mouth and
    /// texture; no blink step) drawn again with camera `cam` into a frame
    /// `tile` / 16 times 640 x 480, `w` x `h` RGB565 pixels with their Z
    /// buffer (cleared to 0 by the caller), by the same pipeline and
    /// rasteriser: the camera's projection scaled ([`Camera::scaled`]),
    /// the vertices and faces those of the original, the pixels the
    /// rasteriser's at the larger frame's pixel centres. `fpu` is the
    /// control word the head's code had before the original draw (the
    /// head's own is left as that draw left it). Returns whether a face
    /// was drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_scaled(
        &mut self,
        cam: usize,
        fpu: Fpu,
        tile: usize,
        pixels: &mut [u16],
        zbuf: &mut [u32],
        w: usize,
        h: usize,
    ) -> bool {
        let Some(c) = self.cameras.get(cam) else {
            return false;
        };
        assert!(pixels.len() >= w * h && zbuf.len() >= w * h);
        let c = c.scaled(fpu, tile, w, h);
        let after = self.fpu;
        self.fpu = fpu;
        let st = self.draw_with(
            &c,
            &mut Target {
                pixels,
                zbuf,
                width: w,
                height: h,
            },
        );
        self.fpu = after;
        !st.rejected && st.faces.contains(&0)
    }

    /// The pipeline with camera `c` into `target`.
    fn draw_with(&mut self, c: &Camera, target: &mut Target) -> Stages {
        let c = c.clone();
        let (mut st, cv) = match self.variant {
            Variant::C => self.transform_c(&c),
            Variant::PII => self.transform_pii(&c),
        };
        if st.rejected {
            return st;
        }
        let or = st.codes.iter().fold(0, |a, &b| a | b);
        if or != 0 {
            // The original then takes another back-face routine
            // (@ 0x004d1070, type 8: @ 0x00571ee0) and clips faces crossing
            // the near or far plane; no head frame of the game needs either.
            log::debug!("head partly outside the view (clip codes {or:#x})");
        }
        st.faces = match self.variant {
            Variant::C => self.back_faces_c(&cv, &mut st.codes),
            Variant::PII => self.back_faces_pii(&cv, &mut st.codes),
        };
        // The projection of the used vertices (@ 0x004d20c0; type 8
        // @ 0x00572080 → @ 0x0057fbc0, the same operations in x87
        // assembly).
        let k = self.fpu;
        let nv = cv.len();
        let mut proj = vec![[0.0f64; 4]; nv];
        st.projected = vec![[0.0f32; 4]; nv];
        for (i, p) in cv.iter().enumerate() {
            if st.codes[i] & 0x90 != USED {
                continue;
            }
            let w = k.div(1.0, p[2]);
            let sx = k.f(k.add(k.mul(k.mul(c.fx, p[0]), w), c.cx));
            let sy = k.f(k.add(k.mul(k.mul(c.fy, p[1]), w), c.cy));
            let depth = k.f(k.sub(c.a, k.mul(w, c.b)));
            proj[i] = [sx, sy, depth, k.f(w)];
            st.projected[i] = proj[i].map(|x| x as f32);
        }
        // The faces (@ 0x004380b0): one group, the texture of its
        // material; three vertex records (sx, sy, w, u, v) per front face.
        let tex = Texture {
            pixels: &self.texture,
            width: self.tex_w,
            height: self.tex_h,
        };
        let mut drawn = false;
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
            match self.variant {
                Variant::C => draw_polygon(k, &mut poly, &tex, target),
                Variant::PII => ii::draw_polygon(&mut poly, &tex, target),
            }
            drawn = true;
        }
        if drawn && self.variant == Variant::PII {
            // The II set-up loaded 0x0e7f and left it.
            self.fpu = Fpu::Chop;
        }
        st
    }

    /// The C transform: the object → camera matrix (@ 0x00571760, mode 7;
    /// the camera node is the camera's world pose, scale 1), the camera-
    /// space vertices and their clip codes (@ 0x004d00a0, mode 1).
    fn transform_c(&mut self, c: &Camera) -> (Stages, Vec<[f64; 3]>) {
        let k = self.fpu;
        let m = |x, y| k.mul(x, y);
        self.update(self.head);
        let n = &self.nodes[self.head];
        let cs = 1.0;
        let is = k.f(k.div(1.0, cs));
        let dx = k.sub(n.wp[0], c.p[0]);
        let dy = k.f(k.sub(n.wp[1], c.p[1]));
        let dz = k.f(k.sub(n.wp[2], c.p[2]));
        let (x, y, z) = (m(dx, is), m(dy, is), m(dz, is));
        let [qx, qy, qz, qw] = c.q;
        let tw = k.f(k.sub(k.sub(-m(x, qx), m(y, qy)), m(z, qz)));
        let tz = k.f(k.sub(k.add(m(z, qw), m(x, qy)), m(y, qx)));
        let ty = k.f(k.sub(k.add(m(z, qx), m(y, qw)), m(x, qz)));
        let tx = k.f(k.sub(k.add(m(y, qz), m(x, qw)), m(z, qy)));
        let t = [
            k.f(k.add(k.sub(k.sub(m(tx, qw), m(tw, qx)), m(tz, qy)), m(ty, qz))),
            k.f(k.add(k.sub(k.sub(m(ty, qw), m(tw, qy)), m(tx, qz)), m(tz, qx))),
            k.f(k.add(k.sub(k.sub(m(tz, qw), m(tw, qz)), m(ty, qx)), m(tx, qy))),
        ];
        let factor = m(n.ws, is);
        let mat = qmat(k, &qcmul(k, &c.q, &n.wq)).map(|e| k.f(m(factor, e)));
        let nv = self.vertices.len();
        let mut st = Stages {
            camera: Vec::with_capacity(nv),
            codes: Vec::with_capacity(nv),
            ..Stages::default()
        };
        let mut cv = Vec::with_capacity(nv);
        let mut and = 0xffu8;
        for v in &self.vertices {
            let row = |r: usize| {
                k.f(k.add(
                    k.add(
                        k.add(m(v[1], mat[3 * r + 1]), m(v[0], mat[3 * r])),
                        m(v[2], mat[3 * r + 2]),
                    ),
                    t[r],
                ))
            };
            let p = [row(0), row(1), row(2)];
            let mut code = if p[2] < c.near {
                CLIP_NEAR
            } else if p[2] > c.far {
                CLIP_FAR
            } else {
                0
            };
            if p[0] < m(p[2], c.slopes[0]) {
                code |= 1;
            } else if p[0] > m(p[2], c.slopes[2]) {
                code |= 4;
            }
            if p[1] < m(p[2], c.slopes[1]) {
                code |= 2;
            } else if p[1] > m(p[2], c.slopes[3]) {
                code |= 8;
            }
            and &= code;
            st.camera.push(p.map(|x| x as f32));
            st.codes.push(code);
            cv.push(p);
        }
        st.rejected = and & 0x3f != 0;
        (st, cv)
    }

    /// The C back faces (@ 0x004d12c0): N = (c1 − c0) × (c2 − c0) from
    /// float differences, drawn when N · c0 < 0; the used vertices' codes
    /// become 0x80.
    fn back_faces_c(&self, cv: &[[f64; 3]], codes: &mut [u8]) -> Vec<u8> {
        let k = self.fpu;
        let m = |x, y| k.mul(x, y);
        let mut faces = Vec::with_capacity(self.mesh.corners.len());
        for tri in &self.mesh.corners {
            let [c0, c1, c2] = tri.map(|i| cv.get(i as usize).copied().unwrap_or([0.0; 3]));
            let a: [f64; 3] = std::array::from_fn(|i| k.f(k.sub(c2[i], c0[i])));
            let b: [f64; 3] = std::array::from_fn(|i| k.f(k.sub(c1[i], c0[i])));
            let nz = k.f(k.sub(m(a[1], b[0]), m(a[0], b[1])));
            let ny = k.f(k.sub(m(b[2], a[0]), m(a[2], b[0])));
            let nx = k.f(k.sub(m(a[2], b[1]), m(b[2], a[1])));
            let dot = k.add(k.add(m(c0[2], nz), m(c0[1], ny)), m(nx, c0[0]));
            faces.push(mark_front(dot < 0.0, tri, codes));
        }
        faces
    }

    /// The type-8 transform (@ 0x00571bd0 → @ 0x0057eec0, x87 assembly):
    /// the head node's world pose by the assembly update (@ 0x00584340),
    /// the matrix computed inline (@ 0x0057eef0), then each vertex and its
    /// clip codes, compared before the float stores.
    fn transform_pii(&mut self, c: &Camera) -> (Stages, Vec<[f64; 3]>) {
        let k = self.fpu;
        let m = |x, y| k.mul(x, y);
        self.update_asm(self.head);
        let n = &self.nodes[self.head];
        // The camera node: its world pose is its local one (a child of the
        // scene root), scale 1.
        let [cx, cy, cz, cw] = c.q;
        let cs = 1.0;
        let is = k.div(1.0, cs);
        let dx = k.sub(n.wp[0], c.p[0]);
        let dy = k.sub(n.wp[1], c.p[1]);
        let dz = k.sub(n.wp[2], c.p[2]);
        let a = k.sub(k.add(m(dx, cw), m(cz, dy)), m(cy, dz));
        let b = k.sub(k.add(m(dy, cw), m(cx, dz)), m(cz, dx));
        let e = k.sub(k.add(m(dz, cw), m(cy, dx)), m(cx, dy));
        let d = k.add(m(dx, cx), k.add(m(dy, cy), m(dz, cz)));
        let t = [
            k.f(m(
                k.add(m(a, cw), k.add(k.sub(m(cx, d), m(cy, e)), m(cz, b))),
                is,
            )),
            k.f(m(
                k.add(m(b, cw), k.add(k.sub(m(cy, d), m(cz, a)), m(cx, e))),
                is,
            )),
            k.f(m(
                k.add(m(e, cw), k.add(k.sub(m(d, cz), m(cx, b)), m(cy, a))),
                is,
            )),
        ];
        let scale = k.f(m(is, n.ws));
        // conj(q_C) ⊗ q_N.
        let [ox, oy, oz, ow] = n.wq;
        let x = k.f(k.add(k.sub(k.sub(m(ox, cw), m(cx, ow)), m(cy, oz)), m(oy, cz)));
        let y = k.f(k.add(k.sub(k.sub(m(oy, cw), m(cy, ow)), m(ox, cz)), m(cx, oz)));
        let z = k.f(k.add(k.sub(k.sub(m(oz, cw), m(ow, cz)), m(oy, cx)), m(cy, ox)));
        let w = k.f(k.add(k.add(k.add(m(cw, ow), m(cx, ox)), m(cy, oy)), m(cz, oz)));
        let (x2, y2, z2) = (k.add(x, x), k.add(y, y), k.add(z, z));
        let mat = [
            k.sub(1.0, k.add(m(y, y2), m(z, z2))),
            k.sub(m(y2, x), m(w, z2)),
            k.add(m(z2, x), m(w, y2)),
            k.add(m(w, z2), m(y2, x)),
            k.sub(1.0, k.add(m(x, x2), m(z, z2))),
            k.sub(m(y, z2), m(x2, w)),
            k.sub(m(z2, x), m(w, y2)),
            k.add(m(x2, w), m(y, z2)),
            k.sub(1.0, k.add(m(x, x2), m(y, y2))),
        ]
        .map(|v| k.f(m(k.f(v), scale)));
        let nv = self.vertices.len();
        let mut st = Stages {
            camera: Vec::with_capacity(nv),
            codes: Vec::with_capacity(nv),
            ..Stages::default()
        };
        let mut cv = Vec::with_capacity(nv);
        let mut and = 0xffu8;
        // `seta` / `setb` after `fcomi`: false / true when unordered.
        let above = |a: f64, b: f64| a > b;
        let below = |a: f64, b: f64| a < b || a.is_nan() || b.is_nan();
        for v in &self.vertices {
            let [vx, vy, vz] = *v;
            let px = k.add(
                k.add(m(mat[2], vz), k.add(m(mat[1], vy), m(mat[0], vx))),
                t[0],
            );
            let py = k.add(
                k.add(m(mat[5], vz), k.add(m(mat[4], vy), m(mat[3], vx))),
                t[1],
            );
            let pz = k.add(
                k.add(k.add(m(vy, mat[7]), m(vx, mat[6])), m(vz, mat[8])),
                t[2],
            );
            let mut code = 0;
            if above(c.near, pz) {
                code |= CLIP_NEAR;
            }
            if below(c.far, pz) {
                code |= CLIP_FAR;
            }
            if above(m(pz, c.slopes[0]), px) {
                code |= 1;
            }
            if below(m(pz, c.slopes[2]), px) {
                code |= 4;
            }
            if above(m(pz, c.slopes[1]), py) {
                code |= 2;
            }
            if below(m(pz, c.slopes[3]), py) {
                code |= 8;
            }
            and &= code;
            let p = [k.f(px), k.f(py), k.f(pz)];
            st.camera.push(p.map(|x| x as f32));
            st.codes.push(code);
            cv.push(p);
        }
        st.rejected = and & 0x3f != 0;
        (st, cv)
    }

    /// The type-8 back faces (@ 0x00572030 → @ 0x0057fac0); see
    /// [`back_faces_pii`].
    fn back_faces_pii(&self, cv: &[[f64; 3]], codes: &mut [u8]) -> Vec<u8> {
        back_faces_pii(self.fpu, &self.mesh.corners, cv, codes)
    }
}

/// The type-8 back faces (@ 0x00572030 → @ 0x0057fac0): the normal
/// from unrounded differences, stored as floats, the test on the
/// unrounded normal; drawn when N · c0 ≤ 0, culled when unordered.
fn back_faces_pii(k: Fpu, corners: &[[u16; 3]], cv: &[[f64; 3]], codes: &mut [u8]) -> Vec<u8> {
    let m = |x, y| k.mul(x, y);
    let mut faces = Vec::with_capacity(corners.len());
    for tri in corners {
        let [c0, c1, c2] = tri.map(|i| cv.get(i as usize).copied().unwrap_or([0.0; 3]));
        let a: [f64; 3] = std::array::from_fn(|i| k.sub(c1[i], c0[i]));
        let b: [f64; 3] = std::array::from_fn(|i| k.sub(c2[i], c0[i]));
        let nx = k.sub(m(b[2], a[1]), m(a[2], b[1]));
        let ny = k.sub(m(a[2], b[0]), m(a[0], b[2]));
        let nz = k.sub(m(a[0], b[1]), m(b[0], a[1]));
        let dot = k.add(m(c0[0], nx), k.add(m(c0[1], ny), m(c0[2], nz)));
        // `fldz; fcomi; jb`: culled when 0 < dot or unordered.
        faces.push(mark_front(dot <= 0.0, tri, codes));
    }
    faces
}

/// A face's byte (0 drawn, 0x40 back face); a drawn face's three vertices
/// get the code 0x80.
fn mark_front(front: bool, tri: &[u16; 3], codes: &mut [u8]) -> u8 {
    if !front {
        return 0x40;
    }
    for &i in tri {
        if let Some(code) = codes.get_mut(i as usize) {
            *code = USED;
        }
    }
    0
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
fn lerp_position(c: Fpu, t0: f64, a: &Vec3, t1: f64, b: &Vec3, t: f64) -> Vec3 {
    let dx = c.sub(b[0], a[0]);
    let dy = c.f(c.sub(b[1], a[1]));
    let dz = c.f(c.sub(b[2], a[2]));
    let dt = c.f(c.sub(t, t0));
    let x = c.mul(dx, dt);
    let py = c.f(c.mul(dy, dt));
    let pz = c.f(c.mul(dz, dt));
    let inv = c.div(1.0, c.sub(t1, t0));
    let invf = c.f(inv);
    let x = c.mul(x, inv);
    let y = c.f(c.mul(py, invf));
    let z = c.f(c.mul(pz, invf));
    [
        c.f(c.add(x, a[0])),
        c.f(c.add(y, a[1])),
        c.f(c.add(z, a[2])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The type-8 back-face rule at its edges, against the original's loop
    /// (@ 0x0057fac0) run under emulation on the same crafted faces under
    /// 0x0e7f: a face whose N · c0 is exactly 0 (in a plane through the
    /// eye, either winding, or degenerate) is drawn, a NaN one is culled.
    #[test]
    fn pii_back_faces_at_zero_and_unordered() {
        let cv = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 5.0],
            [1.0, 0.0, 5.0],
            [0.0, 1.0, 5.0],
            [f64::NAN, 0.0, 5.0],
        ];
        let corners = [
            [0, 1, 2],
            [0, 2, 1],
            [3, 4, 5],
            [3, 5, 4],
            [3, 3, 3],
            [6, 4, 5],
        ];
        let mut codes = [0x01; 7];
        let faces = back_faces_pii(Fpu::Chop, &corners, &cv, &mut codes);
        assert_eq!(faces, [0x00, 0x00, 0x40, 0x00, 0x00, 0x40]);
        assert_eq!(codes, [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);
    }
}
