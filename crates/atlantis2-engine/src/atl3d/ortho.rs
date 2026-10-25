//! `Atl3D`: the 3D databases of the puzzles (`Atl3D_LoadDatabase`
//! @ 0x0042a920, `Atl3D_draw_DataBase` @ 0x0042a320), as the canoe
//! (`pz_pirogue`) uses them: the scene nodes and their operations, a
//! `.db` scene turned into a database (objects in a hierarchy, their
//! key-frame tracks, meshes with the planes and bounding sphere the loader
//! computes, materials and textures), the default camera made
//! orthographic, and the draw of every object of a database through the
//! polygon pipeline (@ 0x004d2770) and the generic face drawer
//! (@ 0x0042d020).
//!
//! The arithmetic follows the original's x87 code operation by operation
//! under the control word in force ([`Fpu`]: `c.f()` a float store, `c.add()`
//! and the like an x87 operation), as [`crate::head`] does for the dialog
//! heads, whose helpers (quaternion products, tracks) it shares.
//!
//! What the drawn faces look like is the original's own quirk: the
//! camera's mode bits in the pipeline flags (orthographic: 0x200) send
//! every face to the generic drawer (@ 0x0042c8d0, flags & 0x60eff not one
//! of the specialised drawers'), which writes only the screen position and
//! w of each vertex record (@ 0x0042d147 .. 0x0042d22a), never its texture
//! coordinates: the rasteriser reads u and v left in the shared record
//! buffer (@ 0x006e1520) by earlier polygons, which its own set-up keeps
//! rescaling in place (0 stays 0). So, as on the original's screen, each
//! face is filled with the texel those coordinates name: the texture's
//! first one once the buffer holds 0 (or values large enough to give the
//! integer indefinite, which also names it).

use atlantis2_data::db::Scene;
use atlantis2_data::tga::Tga;

use crate::fpu::Fpu;
use crate::head::{eval_track, matq, q64, qcmul, qmat, qmul, track, v64, Quat, Track, Vec3};
use crate::raster3d::{draw_polygon, ii, Target, Texture, Vertex};
use crate::warp::Variant;

/// The scene root (@ 0x006d8694): node 0 of a [`Nodes`] arena.
pub const ROOT: usize = 0;

/// A node (0x58 bytes, @ 0x004a1d40): local and world rotation, position
/// and uniform scale; the world values computed on demand (@ 0x004a33c0).
#[derive(Clone, Debug)]
struct Node {
    parent: usize,
    children: Vec<usize>,
    lq: Quat,
    lp: Vec3,
    ls: f64,
    wq: Quat,
    wp: Vec3,
    ws: f64,
    valid: bool,
}

/// The scene's nodes; node 0 is the root, whose world values are the
/// identity.
#[derive(Clone, Debug)]
pub struct Nodes {
    nodes: Vec<Node>,
    /// The x87 control word the code runs under.
    pub fpu: Fpu,
}

const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

impl Nodes {
    pub fn new(fpu: Fpu) -> Nodes {
        Nodes {
            nodes: vec![Node {
                parent: ROOT,
                children: Vec::new(),
                lq: IDENTITY,
                lp: [0.0; 3],
                ls: 1.0,
                wq: IDENTITY,
                wp: [0.0; 3],
                ws: 1.0,
                valid: true,
            }],
            fpu,
        }
    }

    /// A node attached to `parent` with these local values (@ 0x004a1da0;
    /// @ 0x004a1d40 is the identity).
    pub fn add(&mut self, parent: usize, q: Quat, p: Vec3, s: f64) -> usize {
        let i = self.nodes.len();
        self.nodes.push(Node {
            parent,
            children: Vec::new(),
            lq: q,
            lp: p,
            ls: s,
            wq: IDENTITY,
            wp: [0.0; 3],
            ws: 1.0,
            valid: false,
        });
        self.nodes[parent].children.push(i);
        i
    }

    /// The local values.
    pub fn local(&self, i: usize) -> (Quat, Vec3, f64) {
        let n = &self.nodes[i];
        (n.lq, n.lp, n.ls)
    }

    /// The parent.
    pub fn parent(&self, i: usize) -> usize {
        self.nodes[i].parent
    }

    /// The first child, if any (+0x54 of a node, the head of its children).
    pub fn first_child(&self, i: usize) -> Option<usize> {
        self.nodes[i].children.first().copied()
    }

    /// The world values (@ 0x004a33c0 run first).
    pub fn world(&mut self, i: usize) -> (Quat, Vec3, f64) {
        self.update(i);
        let n = &self.nodes[i];
        (n.wq, n.wp, n.ws)
    }

    /// A change of the local values: the node and its descendants stale
    /// (+0x44 = 0, @ 0x004a35d0 for the children of a valid node).
    fn invalidate(&mut self, i: usize) {
        if !self.nodes[i].valid {
            return;
        }
        self.nodes[i].valid = false;
        let children = self.nodes[i].children.clone();
        for c in children {
            self.invalidate(c);
        }
    }

    /// The world values of node `i` (@ 0x004a33c0): a child of the root
    /// takes its local values; else from the parent's world values,
    /// updated first.
    fn update(&mut self, i: usize) {
        if i == ROOT || self.nodes[i].valid {
            return;
        }
        let c = self.fpu;
        let m = |x, y| c.mul(x, y);
        let pi = self.nodes[i].parent;
        if pi == ROOT {
            let n = &mut self.nodes[i];
            (n.wq, n.wp, n.ws, n.valid) = (n.lq, n.lp, n.ls, true);
            return;
        }
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

    /// `SetPosition` (@ 0x004a2000) relative to the node's parent (the only
    /// frame the canoe passes): a copy.
    pub fn set_position(&mut self, i: usize, p: Vec3) {
        if i == ROOT {
            return;
        }
        self.nodes[i].lp = p;
        self.invalidate(i);
    }

    /// `GetPosition` (@ 0x004a2150) relative to the node's parent: its
    /// local position.
    pub fn position(&self, i: usize) -> Vec3 {
        self.nodes[i].lp
    }

    /// `SetRotation` (@ 0x004a2860): relative to the parent, a copy; else,
    /// in `frame`, conj(q_parent) ⊗ (q_frame ⊗ q) with the world values of
    /// both (@ 0x004a28bb .. 0x004a2996).
    pub fn set_rotation(&mut self, i: usize, frame: usize, q: Quat) {
        if i == ROOT {
            return;
        }
        let pi = self.nodes[i].parent;
        if frame == pi {
            self.nodes[i].lq = q;
        } else {
            self.update(pi);
            self.update(frame);
            let c = self.fpu;
            let m = |x, y| c.mul(x, y);
            let [ax, ay, az, aw] = qmul(c, &self.nodes[frame].wq, &q);
            let [px, py, pz, pw] = self.nodes[pi].wq;
            self.nodes[i].lq = [
                c.f(c.add(c.sub(c.sub(m(ax, pw), m(aw, px)), m(az, py)), m(ay, pz))),
                c.f(c.add(c.sub(c.sub(m(ay, pw), m(aw, py)), m(ax, pz)), m(az, px))),
                c.f(c.add(c.sub(c.sub(m(az, pw), m(aw, pz)), m(ay, px)), m(ax, py))),
                c.f(c.add(c.add(c.add(m(ax, px), m(az, pz)), m(ay, py)), m(aw, pw))),
            ];
        }
        self.invalidate(i);
    }

    /// `Rotate` (@ 0x004a2d00) in the node's own frame: q ⊗ r
    /// (@ 0x004a2e72, the product of @ 0x004a29c0 inlined).
    pub fn rotate_self(&mut self, i: usize, r: Quat) {
        if i == ROOT {
            return;
        }
        let c = self.fpu;
        self.nodes[i].lq = qmul(c, &self.nodes[i].lq, &r);
        self.invalidate(i);
    }

    /// `Translate` (@ 0x004a24a0) in the node's own frame (@ 0x004a26ef):
    /// the world positions of the node's origin and of v, their difference
    /// added to the local position (the parent being the root, the call
    /// @ 0x004a282c adds it to the local position, @ 0x004a24c0).
    pub fn translate_self(&mut self, i: usize, v: Vec3) {
        if i == ROOT {
            return;
        }
        let c = self.fpu;
        let (wq, wp, ws) = self.world(i);
        let o = vadd(c, &wp, &rotate(c, &wq, &vscale(c, &[0.0; 3], ws)));
        let t = vadd(c, &wp, &rotate(c, &wq, &vscale(c, &v, ws)));
        let d = [
            c.f(c.sub(t[0], o[0])),
            c.f(c.sub(t[1], o[1])),
            c.f(c.sub(t[2], o[2])),
        ];
        if self.nodes[i].parent == ROOT {
            self.nodes[i].lp = vadd(c, &d, &self.nodes[i].lp);
        } else {
            log::warn!("Atl3D translate in its own frame of a node not under the root");
        }
        self.invalidate(i);
    }
}

/// f(b + a) per component (@ 0x0047df40, `this` = a).
pub(crate) fn vadd(c: Fpu, b: &Vec3, a: &Vec3) -> Vec3 {
    std::array::from_fn(|k| c.f(c.add(b[k], a[k])))
}

/// f(a − b) per component (@ 0x0047df90, `this` = a).
pub(crate) fn vsub(c: Fpu, a: &Vec3, b: &Vec3) -> Vec3 {
    std::array::from_fn(|k| c.f(c.sub(a[k], b[k])))
}

/// f(s · v) per component (@ 0x0049a720).
pub(crate) fn vscale(c: Fpu, v: &Vec3, s: f64) -> Vec3 {
    std::array::from_fn(|k| c.f(c.mul(s, v[k])))
}

/// (x² + y²) + z², unrounded (@ 0x00492bf0).
pub(crate) fn len2(c: Fpu, v: &Vec3) -> f64 {
    c.add(
        c.add(c.mul(v[0], v[0]), c.mul(v[1], v[1])),
        c.mul(v[2], v[2]),
    )
}

/// v / |v|: k = f(1 / √|v|²), then f(k · v) (the pattern of @ 0x004c6f2d,
/// @ 0x00499cee).
pub(crate) fn normalize(c: Fpu, v: &Vec3) -> Vec3 {
    let k = c.f(c.div(1.0, c.sqrt(len2(c, v))));
    vscale(c, v, k)
}

/// The rotation of v by q (@ 0x0047e140).
pub(crate) fn rotate(c: Fpu, q: &Quat, v: &Vec3) -> Vec3 {
    let m = |x, y| c.mul(x, y);
    let [qx, qy, qz, qw] = *q;
    let [vx, vy, vz] = *v;
    let t1 = c.add(c.sub(m(vx, qw), m(vy, qz)), m(vz, qy));
    let t2 = c.add(c.sub(m(vy, qw), m(vz, qx)), m(vx, qz));
    let t3 = c.add(c.sub(m(vz, qw), m(vx, qy)), m(vy, qx));
    let t4 = c.add(c.add(m(vy, qy), m(vx, qx)), m(vz, qz));
    [
        c.f(c.sub(c.add(c.add(m(t4, qx), m(t1, qw)), m(t3, qy)), m(t2, qz))),
        c.f(c.sub(c.add(c.add(m(t1, qz), m(t2, qw)), m(t4, qy)), m(t3, qx))),
        c.f(c.sub(c.add(c.add(m(t4, qz), m(t2, qx)), m(t3, qw)), m(t1, qy))),
    ]
}

/// The inverse rotation of v by q (@ 0x0047e210).
pub(crate) fn inv_rotate(c: Fpu, q: &Quat, v: &Vec3) -> Vec3 {
    let m = |x, y| c.mul(x, y);
    let [qx, qy, qz, qw] = *q;
    let [vx, vy, vz] = *v;
    let a = c.sub(c.add(m(vx, qw), m(vy, qz)), m(vz, qy));
    let b = c.sub(c.add(m(vz, qx), m(vy, qw)), m(vx, qz));
    let cc = c.sub(c.add(m(vx, qy), m(vz, qw)), m(vy, qx));
    let d = c.sub(c.sub(-m(vx, qx), m(vy, qy)), m(vz, qz));
    [
        c.f(c.add(c.sub(c.sub(m(a, qw), m(d, qx)), m(cc, qy)), m(b, qz))),
        c.f(c.add(c.sub(c.sub(m(b, qw), m(d, qy)), m(a, qz)), m(cc, qx))),
        c.f(c.add(c.sub(c.sub(m(cc, qw), m(d, qz)), m(b, qx)), m(a, qy))),
    ]
}

/// The quaternion of a rotation by `angle` (a float) about `axis`
/// (@ 0x004c8950): s = sin(angle / 2) / f(|axis|), each of x, y, z
/// f(axis · s), w = f(cos(angle / 2)); the identity when |axis| is below
/// 1.1920929e-07 (@ 0x005c67d0).
pub(crate) fn axis_angle(c: Fpu, axis: &Vec3, angle: f64) -> Quat {
    let len = c.sqrt(len2(c, axis));
    let lenf = c.f(len);
    if len < f64::from(f32::EPSILON) || len.is_nan() {
        return IDENTITY;
    }
    let h = c.mul(angle, 0.5);
    let s = c.div(libm::sin(h), lenf);
    [
        c.f(c.mul(axis[0], s)),
        c.f(c.mul(axis[1], s)),
        c.f(c.mul(axis[2], s)),
        c.f(libm::cos(h)),
    ]
}

/// The rotation matrix about `axis` by `angle` (@ 0x004c6d10), rows.
pub(crate) fn axis_angle_matrix(c: Fpu, axis: &Vec3, angle: f64) -> [f64; 9] {
    let m = |x, y| c.mul(x, y);
    let [x, y, z] = normalize(c, axis);
    let s = libm::sin(angle);
    let cs = c.f(libm::cos(angle));
    let yx = c.f(m(y, x));
    let yz = c.f(m(y, z));
    let zx = c.f(m(z, x));
    let xs = c.f(m(x, s));
    let ys = c.f(m(y, s));
    let sz = m(s, z);
    let d = c.f(c.sub(1.0, cs));
    let t = m(d, yx);
    let u = m(d, zx);
    let w = m(d, yz);
    [
        c.f(c.add(m(m(x, x), d), cs)),
        c.f(c.sub(t, sz)),
        c.f(c.add(u, ys)),
        c.f(c.add(t, sz)),
        c.f(c.add(m(m(y, y), d), cs)),
        c.f(c.sub(w, xs)),
        c.f(c.sub(c.f(u), ys)),
        c.f(c.add(w, xs)),
        c.f(c.add(m(m(z, z), d), cs)),
    ]
}

/// M · v with M's rows (@ 0x00458e50).
pub(crate) fn mat_vec(c: Fpu, mt: &[f64; 9], v: &Vec3) -> Vec3 {
    let m = |x, y| c.mul(x, y);
    let [vx, vy, vz] = *v;
    [
        c.f(c.add(c.add(m(vz, mt[2]), m(vy, mt[1])), m(vx, mt[0]))),
        c.f(c.add(c.add(m(vx, mt[3]), m(vz, mt[5])), m(vy, mt[4]))),
        c.f(c.add(c.add(m(vx, mt[6]), m(vz, mt[8])), m(vy, mt[7]))),
    ]
}

/// The rotation that turns the frame onto `a` with `b` as the up
/// direction (@ 0x004c8b30: the basis of @ 0x004c6f20, n = a / |a|,
/// u = n × b normalised (another product when it is below 1e-05),
/// v = n × u, as the matrix's columns u, v, n; then its quaternion,
/// @ 0x004c8a10).
pub(crate) fn look_rotation(c: Fpu, a: &Vec3, b: &Vec3) -> Quat {
    let m = |x, y| c.mul(x, y);
    let n = normalize(c, a);
    let [nx, ny, nz] = n;
    let [bx, by, bz] = *b;
    let mut u = [
        c.f(c.sub(m(ny, bz), m(nz, by))),
        c.f(c.sub(m(nz, bx), m(nx, bz))),
        c.f(c.sub(m(nx, by), m(ny, bx))),
    ];
    let l2 = c.add(c.add(m(u[2], u[2]), m(u[1], u[1])), m(u[0], u[0]));
    if l2 < 1e-5 || l2.is_nan() {
        // @ 0x004c7009 .. 0x004c705f.
        u = [
            c.f(c.sub(m(bx, ny), m(bz, nz))),
            c.f(c.sub(m(nz, by), m(bx, nx))),
            c.f(c.sub(m(bz, nx), m(ny, by))),
        ];
    }
    let u = normalize(c, &u);
    let [ux, uy, uz] = u;
    let v = [
        c.f(c.sub(m(ny, uz), m(nz, uy))),
        c.f(c.sub(m(nz, ux), m(nx, uz))),
        c.f(c.sub(m(nx, uy), m(ny, ux))),
    ];
    let mt = [ux, v[0], nx, uy, v[1], ny, uz, v[2], nz];
    matq(c, &mt)
}

/// A texture of a material (`Map 3d`, @ 0x00415a10): the TGA copied into
/// a surface of the screen's format.
#[derive(Clone, Debug)]
pub struct Texture3 {
    pub pixels: Vec<u16>,
    pub width: usize,
    pub height: usize,
}

/// A material (the run-time object of @ 0x00426130), as the textured draw
/// reads it.
#[derive(Clone, Debug)]
struct Material {
    /// +0x1c: 1 − the `.db` transparency.
    opacity: f64,
    texture: Option<usize>,
}

/// A run-time mesh (@ 0x004c8fc0) as the pipeline reads it.
#[derive(Clone, Debug)]
struct Mesh3 {
    vertices: Vec<Vec3>,
    /// Three vertex indices per face, the file's order (+0x164).
    corners: Vec<[u16; 3]>,
    /// Per-face planes (unit normal, d) computed from the vertices
    /// (+0x144, @ 0x004c93a1 .. 0x004c95e3).
    planes: Vec<[f64; 4]>,
    /// Bounding sphere (+0x10): the file box's centre and f(√((e0² + e1²)
    /// + e2²)) of its half extents (@ 0x004dba04).
    sphere: [f64; 4],
    /// Face groups (+0x124): for material k of the mesh's list, the number
    /// of faces whose material byte is k, the faces themselves taken in
    /// the file's order (@ 0x004c9a40 .. 0x004c9b10).
    groups: Vec<(usize, Option<usize>)>,
}

/// An object of a database: its node, mesh and track.
#[derive(Clone, Debug)]
struct Object3 {
    node: usize,
    mesh: Option<usize>,
    track: Option<Track>,
}

/// The 3D library: the textures (`std::map` @ 0x006597d0), materials
/// (@ 0x00659738) and meshes (@ 0x00659728) by name, which every
/// database loaded adds to (@ 0x0042adba, 0x0042ae70, 0x0042af2d: a name
/// already there is not loaded again, so a database whose mesh has the
/// name of one loaded before draws that one), and whose objects find their
/// meshes in (@ 0x0042c1c0, FUN_00426660), the meshes their materials
/// (FUN_00426420 with @ 0x00659738) and the materials their textures
/// (FUN_00426130 with @ 0x006597d0). Emptied by FUN_0042a8d0.
#[derive(Clone, Debug, Default)]
pub struct Library {
    textures: Vec<(String, Texture3)>,
    materials: Vec<(String, Material)>,
    meshes: Vec<(String, Mesh3)>,
}

/// A database slot (0xa0 bytes, e.g. @ 0x006d7d70).
#[derive(Clone, Debug)]
pub struct Database {
    /// The slot's own node (+0x00), a child of the scene root.
    pub node: usize,
    objects: Vec<Object3>,
    /// +0x84: the animation's current frame.
    pub time: f64,
    /// +0x90: the first node's last frame (the animation's length).
    pub length: f64,
    /// +0x98: frames to wait before moving (set to 5 by a change of
    /// animation, @ 0x0049a552).
    pub delay: i32,
}

impl Database {
    /// `Atl3D_LoadDatabase` (@ 0x0042a920) of `scene` into a new slot
    /// node under the root: the objects depth first from the top, each
    /// node's children in name order (@ 0x0042c1c0), with the file's
    /// rotation through a matrix and back (@ 0x004c6e40, @ 0x004c89f0),
    /// position and scale; their tracks (@ 0x0042c5a0, @ 0x004c7250); the
    /// meshes (@ 0x004c8fc0); the materials (@ 0x00426130) and their
    /// textures (`maps[1]`, read by `load`), those three into `lib`
    /// unless it has their names.
    pub fn load(
        nodes: &mut Nodes,
        lib: &mut Library,
        scene: &Scene,
        load: &mut dyn FnMut(&str) -> Option<Tga>,
    ) -> Database {
        let c = nodes.fpu;
        let slot = nodes.add(ROOT, IDENTITY, [0.0; 3], 1.0);
        let mut db = Database {
            node: slot,
            objects: Vec::new(),
            time: 0.0,
            length: 0.0,
            delay: 0,
        };
        // Materials and their textures, by name.
        let mut mats: Vec<&atlantis2_data::db::Material> = scene.materials.iter().collect();
        mats.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        for m in &mats {
            if lib.materials.iter().any(|(n, _)| *n == m.name) {
                continue;
            }
            let texture = m.texture().and_then(|name| {
                if let Some(i) = lib.textures.iter().position(|(n, _)| n == name) {
                    return Some(i);
                }
                let t = load(name)?;
                lib.textures.push((
                    name.to_string(),
                    Texture3 {
                        pixels: t.pixels,
                        width: t.width,
                        height: t.height,
                    },
                ));
                Some(lib.textures.len() - 1)
            });
            lib.materials.push((
                m.name.clone(),
                Material {
                    opacity: f64::from(1.0f32 - m.transparency),
                    texture,
                },
            ));
        }
        let mut objs: Vec<&atlantis2_data::db::Object> = scene.objects.iter().collect();
        objs.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        fn walk<'a>(
            parent_name: &str,
            parent_node: usize,
            objs: &[&'a atlantis2_data::db::Object],
            out: &mut Vec<(&'a atlantis2_data::db::Object, usize)>,
            nodes: &mut Nodes,
            depth: usize,
        ) {
            if depth > objs.len() {
                return;
            }
            for o in objs.iter().filter(|o| o.parent == parent_name) {
                let c = nodes.fpu;
                let q = matq(c, &qmat(c, &q64(o.rotation)));
                let n = nodes.add(parent_node, q, v64(o.position), f64::from(o.scale));
                out.push((o, n));
                walk(&o.name, n, objs, out, nodes, depth + 1);
            }
        }
        let mut created = Vec::new();
        walk("", slot, &objs, &mut created, nodes, 0);
        for (o, n) in created {
            let mesh = scene.mesh(&o.mesh).map(|m| {
                if let Some(i) = lib.meshes.iter().position(|(name, _)| *name == m.name) {
                    return i;
                }
                let built = build_mesh(c, m, &|name: &str| {
                    lib.materials.iter().position(|(n, _)| n == name)
                });
                lib.meshes.push((m.name.clone(), built));
                lib.meshes.len() - 1
            });
            db.objects.push(Object3 {
                node: n,
                mesh,
                track: scene.node(&o.name).and_then(track),
            });
        }
        let mut names: Vec<&atlantis2_data::db::Node> = scene.nodes.iter().collect();
        names.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        db.length = names
            .first()
            .map_or(0.0, |n| f64::from(n.last_frame as f32));
        db
    }

    /// `Atl3D_next_frame` (@ 0x0042a690): the frame + 1, back to 0 at the
    /// animation's length.
    pub fn next_frame(&mut self, c: Fpu) {
        let t = c.f(c.add(self.time, 1.0));
        self.time = if self.length <= t { 0.0 } else { t };
    }
}

/// The run-time mesh of a `.db` mesh record (@ 0x004c8fc0 for a version-2
/// file: planes computed, groups counted).
fn build_mesh(
    c: Fpu,
    m: &atlantis2_data::db::Mesh,
    mat_index: &dyn Fn(&str) -> Option<usize>,
) -> Mesh3 {
    let mm = |x, y| c.mul(x, y);
    let vertices: Vec<Vec3> = m.vertices.iter().map(|v| v64(*v)).collect();
    let nf = m.face_count as usize;
    let corners: Vec<[u16; 3]> = (0..nf)
        .map(|i| std::array::from_fn(|k| m.corners.get(3 * i + k).copied().unwrap_or(0)))
        .collect();
    let vert = |i: u16| vertices.get(usize::from(i)).copied().unwrap_or([0.0; 3]);
    let planes = if m.face_planes.len() >= nf && nf > 0 {
        m.face_planes.iter().map(|p| p.map(f64::from)).collect()
    } else {
        corners
            .iter()
            .map(|t| {
                let (v0, v1, v2) = (vert(t[0]), vert(t[1]), vert(t[2]));
                let e1 = vsub(c, &v1, &v0);
                let e2 = vsub(c, &v2, &v0);
                let mut n = [
                    c.f(c.sub(mm(e2[2], e1[1]), mm(e1[2], e2[1]))),
                    c.f(c.sub(mm(e1[2], e2[0]), mm(e2[2], e1[0]))),
                    c.f(c.sub(mm(e2[1], e1[0]), mm(e2[0], e1[1]))),
                ];
                let l = len2(c, &n);
                if l < f64::from(1.421_085_5e-14_f32) || l.is_nan() {
                    n = [0.0, 0.0, 1.0];
                }
                // @ 0x004cda70: 1 / √|n|² kept unrounded.
                let k = c.div(1.0, c.sqrt(len2(c, &n)));
                let n: Vec3 = std::array::from_fn(|i| c.f(mm(k, n[i])));
                let d = c.add(c.add(mm(n[2], v0[2]), mm(n[1], v0[1])), mm(n[0], v0[0]));
                [n[0], n[1], n[2], c.f(d)]
            })
            .collect()
    };
    let b = &m.bbox;
    let (e0, e1, e2) = (f64::from(b[6]), f64::from(b[10]), f64::from(b[14]));
    let r = c.f(c.sqrt(c.add(c.add(mm(e0, e0), mm(e1, e1)), mm(e2, e2))));
    let sphere = [f64::from(b[0]), f64::from(b[1]), f64::from(b[2]), r];
    let groups = if !m.groups.is_empty() {
        m.groups
            .iter()
            .enumerate()
            .map(|(k, &n)| {
                (
                    usize::from(n),
                    m.materials.get(k).and_then(|s| mat_index(s)),
                )
            })
            .collect()
    } else {
        (0..m.materials.len())
            .map(|k| {
                let n = m
                    .face_materials
                    .iter()
                    .filter(|&&b| usize::from(b) == k)
                    .count();
                (n, mat_index(&m.materials[k]))
            })
            .collect()
    };
    Mesh3 {
        vertices,
        corners,
        planes,
        sphere,
        groups,
    }
}

/// The camera (0x10c bytes, @ 0x004a4730) in its orthographic mode 2, as
/// the canoe sets it (@ 0x00496db8 .. 0x00496ddc).
#[derive(Clone, Debug)]
pub struct Camera3 {
    /// +0x00, +0x04: screen units per world unit (orthographic).
    pub fx: f64,
    pub fy: f64,
    /// +0x08, +0x0c: the screen centre.
    pub cx: f64,
    pub cy: f64,
    pub near: f64,
    pub far: f64,
    /// +0x60, +0x64: the depth map depth = a − b · w.
    pub a: f64,
    pub b: f64,
    /// +0xd4 .. +0xe0: the viewport's top, bottom, right and left.
    pub viewport: [i32; 4],
    /// Its node (+0x68), under the root.
    pub node: usize,
}

impl Camera3 {
    /// The `Default` camera of a database loaded with its camera flag
    /// (@ 0x0042a9e5 .. 0x0042aa64): centre (w / 2, h / 2), viewport (0,
    /// h − 1, w − 1, 0), near 1.5 and far 20000; then the mode 2 and the
    /// scale `scale` (@ 0x004a4910, @ 0x004a4930), which give fx = fy =
    /// the scale and the depth map of @ 0x004a5026.
    pub fn orthographic(nodes: &mut Nodes, scale: [f32; 2], w: i32, h: i32) -> Camera3 {
        let node = nodes.add(ROOT, IDENTITY, [0.0; 3], 1.0);
        let mut cam = Camera3 {
            fx: f64::from(scale[0]),
            fy: f64::from(scale[1]),
            cx: f64::from((w as f32) * 0.5),
            cy: f64::from((h as f32) * 0.5),
            near: 1.5,
            far: 20000.0,
            a: 0.0,
            b: 0.0,
            viewport: [0, h - 1, w - 1, 0],
            node,
        };
        cam.derive(nodes.fpu);
        cam
    }

    /// @ 0x004a4a30 in mode 2: the depth map.
    fn derive(&mut self, c: Fpu) {
        let a = c.div(self.far, c.sub(self.far, self.near));
        self.a = c.f(a);
        self.b = c.f(c.mul(a, self.near));
    }

    /// `SetCenter` (@ 0x004a48f0).
    pub fn set_centre(&mut self, c: Fpu, cx: f64, cy: f64) {
        self.cx = cx;
        self.cy = cy;
        self.derive(c);
    }

    /// @ 0x004a4930: fx, fy.
    pub fn set_scale(&mut self, c: Fpu, fx: f64, fy: f64) {
        self.fx = fx;
        self.fy = fy;
        self.derive(c);
    }

    /// @ 0x004a6210 in mode 2: (f(x fx + cx), f(y fy + cy), f(1 / z)).
    fn project(&self, c: Fpu, p: &Vec3) -> [f64; 3] {
        let w = c.div(1.0, p[2]);
        [
            c.f(c.add(c.mul(p[0], self.fx), self.cx)),
            c.f(c.add(c.mul(p[1], self.fy), self.cy)),
            c.f(w),
        ]
    }

    /// @ 0x004a5040 in mode 2: the sphere (centre p in camera space,
    /// radius r) against the near and far planes and the viewport: 0x40
    /// all outside one of them, else the clip bits it crosses.
    fn sphere_codes(&self, c: Fpu, p: &Vec3, r: f64) -> u32 {
        let [sx, sy, _] = self.project(c, p);
        let rx = c.f(c.mul(r, self.fx));
        let ry = c.f(c.mul(r, self.fy));
        let z = p[2];
        // `fcomp` then `test ah, 0x41`: "≤ or unordered" / `test ah, 1`:
        // "< or unordered".
        let le = |a: f64, b: f64| a.partial_cmp(&b) != Some(std::cmp::Ordering::Greater);
        let lt =
            |a: f64, b: f64| matches!(a.partial_cmp(&b), Some(std::cmp::Ordering::Less) | None);
        let mut code = 0u32;
        if !le(z, self.near) {
            // @ 0x004a50b2.
            if le(c.sub(z, r), self.near) {
                code = 0x10;
            }
            if lt(z, self.far) {
                if !le(c.add(z, r), self.far) {
                    code |= 0x20;
                }
            } else {
                if !le(c.sub(z, r), self.far) {
                    return 0x40;
                }
                code |= 0x20;
            }
        } else {
            // @ 0x004a50fd.
            if lt(z, self.far) {
                if !le(c.add(z, r), self.far) {
                    code = 0x20;
                }
            } else {
                if !le(c.sub(z, r), self.far) {
                    return 0x40;
                }
                code = 0x20;
            }
            if le(c.add(z, r), self.near) {
                return 0x40;
            }
            code |= 0x10;
        }
        // @ 0x004a5149.
        let left = f64::from(self.viewport[3]);
        let right = f64::from(self.viewport[2]);
        let top = f64::from(self.viewport[0]);
        let bottom = f64::from(self.viewport[1]);
        if lt(sx, left) {
            if lt(c.add(sx, rx), left) {
                return 0x40;
            }
            code |= 1;
        } else if !le(sx, right) {
            if !le(c.sub(sx, rx), right) {
                return 0x40;
            }
            code |= 4;
        }
        if lt(sy, top) {
            if lt(c.add(sy, ry), top) {
                return 0x40;
            }
            return code | 2;
        }
        if le(sy, bottom) {
            return code;
        }
        if !le(c.sub(sy, ry), bottom) {
            return 0x40;
        }
        code | 8
    }
}

/// Clip codes of the pipeline's vertices (+0x24).
const CLIP_NEAR: u8 = 0x10;
const CLIP_FAR: u8 = 0x20;
const USED: u8 = 0x80;

/// What the draw of a database leaves for the next one: the rasteriser's
/// vertex records' texture coordinates (@ 0x006e1520: u at +0x0c, v at
/// +0x10 of each 9-float record), which the generic drawer never writes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Records {
    pub uv: [[f64; 2]; 3],
}

/// The rasteriser's destination and its Z buffer (the canoe's: the board
/// depth copied in each frame, never cleared by the draw).
pub struct DrawTarget<'a> {
    pub pixels: &'a mut [u16],
    pub zbuf: &'a mut [u32],
    pub width: usize,
    pub height: usize,
}

/// The routines a draw uses ([`Variant`], the warp's).
#[derive(Clone, Copy, Debug)]
pub struct DrawContext {
    pub variant: Variant,
}

/// `Atl3D_draw_DataBase` (@ 0x0042a320): every object's tracks at the
/// database's frame (@ 0x004c7810, flags 0x1f), then each object with a
/// mesh through the pipeline with the flags ((camera mode & 6) | 0x67000)
/// << 8 (@ 0x0042a4ca; the mesh flags have no 0x10, so no 0x1080000; the
/// lighting bits 0x400000 / 0x800000 cleared when no light exists,
/// @ 0x0042a4ef, and the meshes' lighting skipped as the original skips
/// it without a light list, @ 0x0042a54f), the generic drawer for each.
/// The control word may change (the II rasteriser loads 0x0e7f): the
/// nodes' `fpu` is left as the draw leaves it.
pub fn draw_database(
    nodes: &mut Nodes,
    cam: &Camera3,
    lib: &Library,
    db: &Database,
    ctx: DrawContext,
    out: &mut DrawTarget,
    records: &mut Records,
) {
    let t = f64::from(db.time as f32);
    for o in &db.objects {
        let Some(tr) = &o.track else { continue };
        let (q, p, s) = eval_track(nodes.fpu, tr, t);
        let n = &mut nodes.nodes[o.node];
        if let Some(q) = q {
            n.lq = q;
        }
        if let Some(p) = p {
            n.lp = p;
        }
        if let Some(s) = s {
            n.ls = s;
        }
        nodes.invalidate(o.node);
    }
    for o in &db.objects {
        let Some(mi) = o.mesh else { continue };
        draw_object(
            nodes,
            cam,
            lib,
            &lib.meshes[mi].1,
            o.node,
            ctx,
            out,
            records,
        );
    }
}

/// One object through the pipeline (@ 0x004d2770 with the C routines of
/// the flags 0x6700200, whatever the CPU type: (flags & 0x80e00) = 0x200
/// selects @ 0x004d00a0) and the generic face drawer (@ 0x0042d020).
#[allow(clippy::too_many_arguments)]
fn draw_object(
    nodes: &mut Nodes,
    cam: &Camera3,
    lib: &Library,
    mesh: &Mesh3,
    node: usize,
    ctx: DrawContext,
    out: &mut DrawTarget,
    records: &mut Records,
) {
    let c = nodes.fpu;
    let m = |x, y| c.mul(x, y);
    // The object → camera matrix (@ 0x00571760, mode 7).
    let (cq, cp, cs) = nodes.world(cam.node);
    let (nq, np, ns) = nodes.world(node);
    let is = c.f(c.div(1.0, cs));
    let dx = c.sub(np[0], cp[0]);
    let dy = c.f(c.sub(np[1], cp[1]));
    let dz = c.f(c.sub(np[2], cp[2]));
    let (x, y, z) = (m(dx, is), m(dy, is), m(dz, is));
    let [qx, qy, qz, qw] = cq;
    let tw = c.f(c.sub(c.sub(-m(x, qx), m(y, qy)), m(z, qz)));
    let tz = c.f(c.sub(c.add(m(z, qw), m(x, qy)), m(y, qx)));
    let ty = c.f(c.sub(c.add(m(z, qx), m(y, qw)), m(x, qz)));
    let tx = c.f(c.sub(c.add(m(y, qz), m(x, qw)), m(z, qy)));
    let tr = [
        c.f(c.add(c.sub(c.sub(m(tx, qw), m(tw, qx)), m(tz, qy)), m(ty, qz))),
        c.f(c.add(c.sub(c.sub(m(ty, qw), m(tw, qy)), m(tx, qz)), m(tz, qx))),
        c.f(c.add(c.sub(c.sub(m(tz, qw), m(tw, qz)), m(ty, qx)), m(tx, qy))),
    ];
    let factor = m(ns, is);
    let mat = qmat(c, &qcmul(c, &cq, &nq)).map(|e| c.f(m(factor, e)));
    // +0x30: the scale (@ 0x0057198e .. 0x005719d6, FUN_004a53f0).
    let scale = {
        let s = if ns == 0.0 { 1.0 } else { ns };
        let v = c.f(c.mul(c.div(1.0, cs), s));
        if v == 0.0 {
            1.0
        } else {
            v
        }
    };
    // The bounding sphere (@ 0x004d00e3: flags & 0x300000, no
    // 0x1000000), case 2 of @ 0x004a5440.
    let sphere_code = {
        let r0 = if mesh.sphere[3] == 0.0 {
            1.0
        } else {
            mesh.sphere[3]
        };
        let rs = m(r0, scale);
        let r = if rs == 0.0 { 1.0 } else { c.f(rs) };
        let centre = [mesh.sphere[0], mesh.sphere[1], mesh.sphere[2]];
        let rc = mat_vec(c, &mat, &centre);
        let pc = vadd(c, &tr, &rc);
        cam.sphere_codes(c, &pc, r)
    };
    if sphere_code & 0x40 != 0 {
        return;
    }
    // The vertices (@ 0x004d00a0): unclipped (@ 0x004d0691) or with clip
    // codes in mode 2 (@ 0x004d035c), the same sums.
    let nv = mesh.vertices.len();
    let mut cv: Vec<Vec3> = Vec::with_capacity(nv);
    let mut codes = vec![0u8; nv];
    let mut or = 0u8;
    let mut and = 0xffu8;
    for (i, v) in mesh.vertices.iter().enumerate() {
        let row = |r: usize| {
            c.f(c.add(
                c.add(
                    c.add(m(v[0], mat[3 * r]), m(v[2], mat[3 * r + 2])),
                    m(v[1], mat[3 * r + 1]),
                ),
                tr[r],
            ))
        };
        let p = [row(0), row(1), row(2)];
        if sphere_code != 0 {
            let [sx, sy, _] = cam.project(c, &p);
            let mut code = if p[2] < cam.near || p[2].is_nan() {
                CLIP_NEAR
            } else if p[2] > cam.far {
                CLIP_FAR
            } else {
                0
            };
            let (left, right) = (f64::from(cam.viewport[3]), f64::from(cam.viewport[2]));
            let (top, bottom) = (f64::from(cam.viewport[0]), f64::from(cam.viewport[1]));
            if left > sx {
                code |= 1;
            } else if right < sx || sx.is_nan() {
                code |= 4;
            }
            if top > sy {
                code |= 2;
            } else if bottom < sy || sy.is_nan() {
                code |= 8;
            }
            codes[i] = code;
            or |= code;
            and &= code;
        }
        cv.push(p);
    }
    if sphere_code != 0 {
        if and & 0x3f != 0 {
            return;
        }
    } else {
        or = 0;
    }
    // The back faces (@ 0x004d16b0 with no clipped vertex, flags 0x4200;
    // else @ 0x004d14c0): the view direction in the object's frame against
    // the face planes.
    let (oq, _, _) = nodes.world(node);
    let dir = inv_rotate(c, &oq, &rotate(c, &cq, &[0.0, 0.0, 1.0]));
    let mut faces = Vec::with_capacity(mesh.corners.len());
    for (fi, t) in mesh.corners.iter().enumerate() {
        let pl = mesh.planes.get(fi).copied().unwrap_or([0.0, 0.0, 1.0, 0.0]);
        let dot = c.add(c.add(m(pl[2], dir[2]), m(pl[1], dir[1])), m(dir[0], pl[0]));
        // `fcomp 0.0; test ah, 1`: front when below 0 or unordered.
        if dot >= 0.0 {
            faces.push(0x40u8);
            continue;
        }
        let idx = t.map(usize::from);
        let cc = idx.map(|i| codes.get(i).copied().unwrap_or(0));
        if or == 0 {
            for &i in &idx {
                if let Some(code) = codes.get_mut(i) {
                    *code = USED;
                }
            }
            faces.push(0);
        } else if (cc[0] & cc[1] & cc[2]) & 0x3f != 0 {
            faces.push(0x40);
        } else {
            faces.push(cc[0] | cc[1] | cc[2]);
            for &i in &idx {
                if let Some(code) = codes.get_mut(i) {
                    *code |= USED;
                }
            }
        }
    }
    // The projection of the used vertices (@ 0x004d2170).
    let mut proj = vec![[0.0f64; 4]; nv];
    for (i, p) in cv.iter().enumerate() {
        if codes[i] & 0x90 != USED {
            continue;
        }
        let w = c.div(1.0, p[2]);
        proj[i] = [
            c.f(c.add(c.mul(cam.fx, p[0]), cam.cx)),
            c.f(c.add(c.mul(p[1], cam.fy), cam.cy)),
            c.f(c.sub(cam.a, c.mul(w, cam.b))),
            c.f(w),
        ];
    }
    // The generic drawer (@ 0x0042d020), the groups in order: the
    // material's state (@ 0x00429700: a transparent one defers its faces,
    // which no canoe material is; a textured one binds its texture), each
    // drawable face's three records (sx, sy, w) over the records' u, v.
    let mut fi = 0usize;
    let mut fpu = c;
    for &(count, mat) in &mesh.groups {
        let material = mat.and_then(|k| lib.materials.get(k)).map(|m| &m.1);
        let texture = material
            .and_then(|m| m.texture)
            .and_then(|t| lib.textures.get(t))
            .map(|t| &t.1);
        let deferred = material.is_some_and(|m| 0.0 < c.sub(1.0, m.opacity));
        for _ in 0..count {
            let f = faces.get(fi).copied().unwrap_or(0x40);
            let tri = mesh.corners.get(fi).copied();
            fi += 1;
            if f & 0x40 != 0 || deferred {
                if deferred && f & 0x40 == 0 {
                    log::warn!("Atl3D: a transparent material's faces are not drawn");
                }
                continue;
            }
            let Some(tri) = tri else { continue };
            if f & 0x30 != 0 {
                // A face across the near or far plane (@ 0x0042d276): the
                // canoe's objects stay hundreds of units away from both.
                log::warn!("Atl3D: a face across the near or far plane is not drawn");
                continue;
            }
            let Some(tex) = texture else {
                log::warn!("Atl3D: a face without texture is not drawn");
                continue;
            };
            let mut poly: [Vertex; 3] = std::array::from_fn(|k| {
                let p = proj[usize::from(tri[k])];
                Vertex {
                    x: p[0],
                    y: p[1],
                    w: p[3],
                    u: records.uv[k][0],
                    v: records.uv[k][1],
                }
            });
            let t = Texture {
                pixels: &tex.pixels,
                width: tex.width,
                height: tex.height,
            };
            let mut target = Target {
                pixels: out.pixels,
                zbuf: out.zbuf,
                width: out.width,
                height: out.height,
            };
            match ctx.variant {
                Variant::C => draw_polygon(fpu, &mut poly, &t, &mut target),
                Variant::PII => {
                    ii::draw_polygon(&mut poly, &t, &mut target);
                    fpu = Fpu::Chop;
                }
            }
            for (k, p) in poly.iter().enumerate() {
                records.uv[k] = [p.u, p.v];
            }
        }
    }
    nodes.fpu = fpu;
}
