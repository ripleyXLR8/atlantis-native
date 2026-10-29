//! The Licorne board's 3D part (`CPuzzle_Licorne`, atlantis3-gog-fr-3473408
//! @ 0x004b9c00..0x004bed40): three scenes (the unicorn, the beast, the
//! four beacons) on a node graph driven by node name, the unicorn's and
//! the beast's rigid skinning, the board's key-frame tracks, and the draw
//! through camera `Camera01` of `PL_Dum&Cam.DB` over the pre-rendered
//! background and its depth buffer.
//!
//! The arithmetic stages shared with the dialog heads come from
//! `atlantis2_engine::head` (the type-8 transform, back faces and
//! projection of the Pentium II path, the asm world update, the slerp)
//! and `raster3d::linear_ii` (`Zi32Texture16Linear1516II`, the only
//! routine the board draws with). New here, each transcribed from its
//! x87 code under the control word 0x0e7f the warp leaves (every float
//! operation is [`Fpu::Chop`]):
//!
//! - the scene class: a scene's objects split into two node trees under
//!   two roots of the scene (+0x28 the objects, kind ≠ 1; +0x90 the
//!   skeleton, kind 1 or named `skeleton`, @ 0x004bdb20 / @ 0x004bdb80),
//!   each walked depth first with the children in name order (the lists
//!   +0x84 and +0xec);
//! - the C world update of a node (@ 0x004e6c70) the loader uses, beside
//!   the assembly one the transform uses (@ 0x0069a440, the heads'): both
//!   fill the same cached world values;
//! - the skin binding (@ 0x0051b510): each vertex taken into the bone
//!   space of its bone at the bind pose, once at load;
//! - the board's tracks (@ 0x0050a150): the A3 track builder's keys
//!   (@ 0x00509710) at the integer frame, with this game's position lerp
//!   (@ 0x0050a470);
//! - the per-bone transform of a skinned mesh (@ 0x00690650).

use atlantis2_data::db::{Mesh as DbMesh, Scene};
use atlantis2_data::tga::Tga;
use atlantis2_engine::fpu::Fpu;
use atlantis2_engine::head::{
    self, back_faces_pii, matq, project, q64, qmat, slerp, transform_pii, update_asm_node, v64,
    Camera, Node, Quat, Track, Vec3,
};
use atlantis2_engine::raster3d::{linear_ii, Target, Texture, Vertex};

/// Every float operation of the board runs under 0x0e7f.
pub(crate) const K: Fpu = Fpu::Chop;

/// A mesh of a scene object, as the draw reads it.
#[derive(Clone, Debug)]
struct ObjMesh {
    /// The object's node.
    node: usize,
    /// Three vertex indices per face.
    corners: Vec<[u16; 3]>,
    /// Texture coordinates of set 1, per face corner.
    uvs: Vec<[[f32; 2]; 3]>,
    /// The vertices the transform reads (mesh +0xf4): the file's for a
    /// rigid mesh, the bone-space ones for a skinned mesh.
    vertices: Vec<Vec3>,
    /// Skinned: each vertex's bone node.
    bones: Option<Vec<usize>>,
    /// The texture (the material's map slot 1), RGB565.
    texture: Option<(Vec<u16>, usize, usize)>,
    name: String,
}

/// A scene of the board (`CPuzzle3D` object, 0x1e4 bytes, @ 0x004bcf80).
#[derive(Clone, Debug)]
pub struct Scene3 {
    pub(crate) nodes: Vec<Node>,
    /// The objects list (+0x84) and the skeleton list (+0xec): node
    /// indices, depth first.
    pub(crate) objects: Vec<usize>,
    pub(crate) skeleton: Vec<usize>,
    meshes: Vec<ObjMesh>,
    /// Per animation: its frame count (the first node's last frame,
    /// @ 0x004bdbe0) and one track per skeleton list entry (zipped by
    /// position, @ 0x004be070).
    anims: Vec<(i32, Vec<Option<Track>>)>,
}

/// The two roots of every scene: +0x28 (objects) and +0x90 (skeleton).
pub(crate) const ROOT_OBJ: usize = 0;
pub(crate) const ROOT_SKEL: usize = 1;

fn identity_node(name: &str) -> Node {
    Node {
        name: name.to_string(),
        parent: None,
        lq: [0.0, 0.0, 0.0, 1.0],
        lp: [0.0; 3],
        ls: 1.0,
        wq: [0.0, 0.0, 0.0, 1.0],
        wp: [0.0; 3],
        ws: 1.0,
        valid: false,
    }
}

/// The objects of `scene` kept by `keep`, depth first from the top
/// (parent `""`), the children of a node in name order (byte order).
fn depth_first<'a>(
    scene: &'a Scene,
    keep: impl Fn(&atlantis2_data::db::Object) -> bool,
) -> Vec<(&'a atlantis2_data::db::Object, Option<usize>)> {
    let mut objects: Vec<&atlantis2_data::db::Object> =
        scene.objects.iter().filter(|o| keep(o)).collect();
    objects.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let mut out = Vec::new();
    fn walk<'a>(
        objects: &[&'a atlantis2_data::db::Object],
        parent: &str,
        parent_index: Option<usize>,
        out: &mut Vec<(&'a atlantis2_data::db::Object, Option<usize>)>,
    ) {
        for o in objects.iter().filter(|o| o.parent == parent) {
            let i = out.len();
            out.push((o, parent_index));
            if out.len() > objects.len() {
                return;
            }
            walk(objects, &o.name, Some(i), out);
        }
    }
    walk(&objects, "", None, &mut out);
    out
}

fn is_bone(o: &atlantis2_data::db::Object) -> bool {
    o.kind == 1 || o.name == "skeleton"
}

/// The C world update of node `i` (@ 0x004e6c70): a child of a root of
/// the world takes its local values; else the parent's (updated first)
/// composed with its own, through float stores.
pub(crate) fn update_c(nodes: &mut [Node], i: usize) {
    if nodes[i].valid {
        return;
    }
    let c = K;
    let m = |x, y| c.mul(x, y);
    let Some(pi) = nodes[i].parent else {
        let n = &mut nodes[i];
        (n.wq, n.wp, n.ws, n.valid) = (n.lq, n.lp, n.ls, true);
        return;
    };
    update_c(nodes, pi);
    let (pq, pp, ps) = {
        let p = &nodes[pi];
        (p.wq, p.wp, p.ws)
    };
    let n = &mut nodes[i];
    // @ 0x004e6370: the parent's rotation times the node's.
    let a = pq;
    let b = n.lq;
    let w = c.sub(c.sub(c.sub(m(a[3], b[3]), m(b[0], a[0])), m(a[1], b[1])), m(a[2], b[2]));
    let z = c.sub(c.add(c.add(m(b[2], a[3]), m(a[2], b[3])), m(b[1], a[0])), m(a[1], b[0]));
    let y = c.sub(c.add(c.add(m(b[1], a[3]), m(a[1], b[3])), m(a[2], b[0])), m(b[2], a[0]));
    let x = c.sub(c.add(c.add(m(a[3], b[0]), m(a[0], b[3])), m(a[1], b[2])), m(a[2], b[1]));
    n.wq = [c.f(x), c.f(y), c.f(z), c.f(w)];
    let [px, py, pz, pw] = pq;
    let sa = c.f(m(ps, n.lp[0]));
    let sb = c.f(m(ps, n.lp[1]));
    let sc = m(ps, n.lp[2]);
    let x0 = c.f(c.add(c.sub(m(sa, pw), m(sb, pz)), m(sc, py)));
    let y0 = c.f(c.add(c.sub(m(sb, pw), m(sc, px)), m(sa, pz)));
    let z0 = c.f(c.add(c.sub(m(sc, pw), m(sa, py)), m(sb, px)));
    let w0 = c.add(c.add(m(sa, px), m(sc, pz)), m(sb, py));
    let x1 = c.f(c.sub(c.add(c.add(m(w0, px), m(x0, pw)), m(z0, py)), m(y0, pz)));
    let y1 = c.f(c.sub(c.add(c.add(m(x0, pz), m(y0, pw)), m(w0, py)), m(z0, px)));
    let z1 = c.sub(c.add(c.add(m(y0, px), m(w0, pz)), m(z0, pw)), m(x0, py));
    n.wp = [
        c.f(c.add(pp[0], x1)),
        c.f(c.add(pp[1], y1)),
        c.f(c.add(pp[2], z1)),
    ];
    let s = m(n.ls, ps);
    n.ws = if s == 0.0 { 1.0 } else { c.f(s) };
    n.valid = true;
}

/// Node `i` and its descendants made stale (a setter, @ 0x004e6e70).
pub(crate) fn invalidate(nodes: &mut [Node], i: usize) {
    nodes[i].valid = false;
    let mut stack = vec![i];
    while let Some(p) = stack.pop() {
        for (j, n) in nodes.iter_mut().enumerate() {
            if n.parent == Some(p) && n.valid {
                n.valid = false;
                stack.push(j);
            }
        }
    }
}

/// v · s (@ 0x0042fbb0), each component stored.
fn vscale(v: &Vec3, s: f64) -> Vec3 {
    v.map(|x| K.f(K.mul(s, x)))
}

/// q ⊗ v ⊗ q* (@ 0x00434a40).
fn qrot(q: &Quat, v: &Vec3) -> Vec3 {
    let c = K;
    let m = |x, y| c.mul(x, y);
    let [vx, vy, vz] = *v;
    let [qx, qy, qz, qw] = *q;
    let t0 = c.f(c.add(c.sub(m(vx, qw), m(vy, qz)), m(vz, qy)));
    let t1 = c.f(c.add(c.sub(m(vy, qw), m(vz, qx)), m(vx, qz)));
    let t2 = c.f(c.add(c.sub(m(vz, qw), m(vx, qy)), m(vy, qx)));
    let t3u = c.add(c.add(m(vy, qy), m(vx, qx)), m(vz, qz));
    let t3 = c.f(t3u);
    let a = c.sub(c.add(c.add(m(t3u, qz), m(t1, qx)), m(t2, qw)), m(t0, qy));
    let b = c.sub(c.add(c.add(m(t0, qz), m(t1, qw)), m(t3, qy)), m(t2, qx));
    let d = c.sub(c.add(c.add(m(t3, qx), m(t0, qw)), m(t2, qy)), m(t1, qz));
    [c.f(d), c.f(b), c.f(a)]
}

/// q* ⊗ v ⊗ q (@ 0x0043a6a0).
fn qrot_inv(q: &Quat, v: &Vec3) -> Vec3 {
    let c = K;
    let m = |x, y| c.mul(x, y);
    let [vx, vy, vz] = *v;
    let [qx, qy, qz, qw] = *q;
    let s0 = c.f(c.sub(c.add(m(vx, qw), m(vy, qz)), m(vz, qy)));
    let s1 = c.f(c.sub(c.add(m(vz, qx), m(vy, qw)), m(vx, qz)));
    let s2 = c.f(c.sub(c.add(m(vx, qy), m(vz, qw)), m(vy, qx)));
    let s3 = c.f(c.sub(c.sub(-m(vx, qx), m(vy, qy)), m(vz, qz)));
    let a = c.add(c.sub(c.sub(m(s2, qw), m(s3, qz)), m(s1, qx)), m(s0, qy));
    let b = c.add(c.sub(c.sub(m(s1, qw), m(s3, qy)), m(s0, qz)), m(s2, qx));
    let d = c.add(c.sub(c.sub(m(s0, qw), m(s3, qx)), m(s2, qy)), m(s1, qz));
    [c.f(d), c.f(b), c.f(a)]
}

/// a + b, a − b (@ 0x004347b0, @ 0x0043a570), each component stored.
fn vadd(a: &Vec3, b: &Vec3) -> Vec3 {
    std::array::from_fn(|i| K.f(K.add(a[i], b[i])))
}
fn vsub(a: &Vec3, b: &Vec3) -> Vec3 {
    std::array::from_fn(|i| K.f(K.sub(a[i], b[i])))
}

/// The board's position lerp of two keys (@ 0x0050a470).
fn lerp_position(t0: f64, a: &Vec3, t1: f64, b: &Vec3, t: f64) -> Vec3 {
    let c = K;
    let dx = c.sub(b[0], a[0]);
    let dy = c.f(c.sub(b[1], a[1]));
    let dz = c.f(c.sub(b[2], a[2]));
    let dt = c.f(c.sub(t, t0));
    let x = c.mul(dx, dt);
    let py = c.f(c.mul(dy, dt));
    let pz = c.f(c.mul(dz, dt));
    let inv = c.div(1.0, c.sub(t1, t0));
    let invf = c.f(inv);
    let xs = c.f(c.mul(inv, x));
    let y = c.f(c.mul(py, invf));
    let z = c.add(c.mul(pz, invf), a[2]);
    [c.f(c.add(xs, a[0])), c.f(c.add(y, a[1])), c.f(z)]
}

/// A track at time `t` (@ 0x0050a150 with the flags 0x1f): the key at or
/// after `t` (the last past the end, the first before the start), or the
/// interpolation of its neighbours.
fn eval(tr: &Track, t: f64) -> (Option<Quat>, Option<Vec3>, Option<f64>) {
    let c = K;
    let lb = |n: usize, time: &dyn Fn(usize) -> f64| (0..n).find(|&k| time(k) >= t).unwrap_or(n);
    let q = (!tr.rotations.is_empty()).then(|| {
        let r = &tr.rotations;
        let k = lb(r.len(), &|k| r[k].0);
        if k == r.len() {
            r[k - 1].1
        } else if r[k].0 == t || k == 0 {
            r[k].1
        } else {
            let (t0, a) = r[k - 1];
            let (t1, b) = r[k];
            slerp(c, &a, &b, c.f(c.div(c.sub(t, t0), c.sub(t1, t0))))
        }
    });
    let p = (!tr.positions.is_empty()).then(|| {
        let r = &tr.positions;
        let k = lb(r.len(), &|k| r[k].0);
        if k == r.len() {
            r[k - 1].1
        } else if r[k].0 == t || k == 0 {
            r[k].1
        } else {
            let (t0, a) = r[k - 1];
            let (t1, b) = r[k];
            lerp_position(t0, &a, t1, &b, t)
        }
    });
    let s = (!tr.scales.is_empty()).then(|| {
        let r = &tr.scales;
        let k = lb(r.len(), &|k| r[k].0);
        if k == r.len() {
            r[k - 1].1
        } else if r[k].0 == t || k == 0 {
            r[k].1
        } else {
            let (t0, a) = r[k - 1];
            let (t1, b) = r[k];
            // @ 0x0050ab50.
            let v = c.add(c.div(c.mul(c.sub(b, a), c.sub(t, t0)), c.sub(t1, t0)), a);
            if v == 0.0 {
                1.0
            } else {
                c.f(v)
            }
        }
    });
    (q, p, s)
}

/// A texture file of `map\` as the display's RGB565.
pub(crate) fn texture(files: &atlantis3_data::A3Files, name: &str) -> Option<(Vec<u16>, usize, usize)> {
    let t = files
        .read(&format!("map/{name}"))
        .and_then(|d| Tga::parse(&d).ok())?;
    Some((t.pixels, t.width, t.height))
}

impl Scene3 {
    /// A scene file (`FUN_004bd3c0`): its two node trees, its meshes with
    /// their textures, the skin binding at the bind pose.
    pub fn load(files: &atlantis3_data::A3Files, scene: &Scene) -> Scene3 {
        let mut nodes = vec![identity_node("+0x28"), identity_node("+0x90")];
        let mut add = |list: &[(&atlantis2_data::db::Object, Option<usize>)], root: usize| {
            let first = nodes.len();
            let mut idx = Vec::new();
            for (o, p) in list {
                idx.push(nodes.len());
                nodes.push(Node {
                    name: o.name.clone(),
                    parent: Some(p.map_or(root, |p| first + p)),
                    lq: matq(K, &qmat(K, &q64(o.rotation))),
                    lp: v64(o.position),
                    ls: f64::from(o.scale),
                    wq: [0.0, 0.0, 0.0, 1.0],
                    wp: [0.0; 3],
                    ws: 1.0,
                    valid: false,
                });
            }
            idx
        };
        let objects = add(&depth_first(scene, |o| !is_bone(o)), ROOT_OBJ);
        let skeleton = add(&depth_first(scene, is_bone), ROOT_SKEL);
        let mut s = Scene3 {
            nodes,
            objects,
            skeleton,
            meshes: Vec::new(),
            anims: Vec::new(),
        };
        for &oi in &s.objects.clone() {
            let name = s.nodes[oi].name.clone();
            let Some(o) = scene.object(&name) else { continue };
            let Some(m) = scene.mesh(&o.mesh) else { continue };
            let tex = m
                .materials
                .first()
                .and_then(|mat| scene.materials.iter().find(|x| &x.name == mat))
                .and_then(|mat| mat.texture())
                .and_then(|t| texture(files, t));
            let mesh = s.build_mesh(oi, m, tex);
            s.meshes.push(mesh);
        }
        s
    }

    fn build_mesh(&mut self, node: usize, m: &DbMesh, texture: Option<(Vec<u16>, usize, usize)>) -> ObjMesh {
        let n = m.face_count as usize;
        let corners: Vec<[u16; 3]> = (0..n)
            .map(|i| std::array::from_fn(|k| m.corners.get(3 * i + k).copied().unwrap_or(0)))
            .collect();
        let uvs = (0..n)
            .map(|i| std::array::from_fn(|k| m.uvs[0].get(3 * i + k).copied().unwrap_or([0.0; 2])))
            .collect();
        let mut vertices: Vec<Vec3> = m.vertices.iter().map(|v| v64(*v)).collect();
        let mut bones = None;
        if !m.bones.is_empty() && m.vertex_bones.len() == vertices.len() {
            // The binding (@ 0x0051b510): each bone name resolved in the
            // skeleton list; each vertex to world by the object's node,
            // then into its bone's space, at the bind pose (C updates).
            let lookup: Vec<Option<usize>> = m
                .bones
                .iter()
                .map(|b| self.skeleton.iter().copied().find(|&i| self.nodes[i].name == *b))
                .collect();
            let mut vb = Vec::with_capacity(vertices.len());
            for (v, &bi) in vertices.iter_mut().zip(&m.vertex_bones) {
                let Some(bone) = lookup.get(bi as usize).copied().flatten() else {
                    log::warn!("Licorne: bone {bi} of {} not found", m.name);
                    vb.push(node);
                    continue;
                };
                update_c(&mut self.nodes, bone);
                update_c(&mut self.nodes, node);
                let o = &self.nodes[node];
                let world = vadd(&qrot(&o.wq, &vscale(v, o.ws)), &o.wp);
                let b = &self.nodes[bone];
                let local = qrot_inv(&b.wq, &vsub(&world, &b.wp));
                *v = vscale(&local, K.f(K.div(1.0, b.ws)));
                vb.push(bone);
            }
            bones = Some(vb);
        }
        ObjMesh {
            node,
            corners,
            uvs,
            vertices,
            bones,
            texture,
            name: m.name.clone(),
        }
    }

    /// An animation file (`FUN_004bdbe0`): its tracks, zipped with the
    /// skeleton list by position (both depth first by name); its frame
    /// count, the first node's last frame.
    pub fn add_anim(&mut self, scene: &Scene) {
        let list = depth_first(scene, is_bone);
        let tracks: Vec<Option<Track>> = (0..self.skeleton.len())
            .map(|k| {
                let (o, _) = list.get(k)?;
                if self.nodes[self.skeleton[k]].name != o.name {
                    log::warn!("Licorne: track {} on node {}", o.name, self.nodes[self.skeleton[k]].name);
                }
                scene.node(&o.name).and_then(head::track)
            })
            .collect();
        let frames = scene.nodes.first().map_or(0, |n| n.last_frame as i32);
        self.anims.push((frames, tracks));
    }

    /// Frames of animation `k` (@ 0x004bec10).
    pub fn anim_frames(&self, k: i32) -> i32 {
        usize::try_from(k)
            .ok()
            .and_then(|k| self.anims.get(k))
            .map_or(0, |a| a.0)
    }

    /// Animation `k` at integer frame `frame` (@ 0x004be070): every
    /// skeleton node with a track set to the track's values at
    /// `(float)frame`.
    pub fn pose(&mut self, k: i32, frame: i32) {
        let Some((_, tracks)) = usize::try_from(k).ok().and_then(|k| self.anims.get(k)) else {
            return;
        };
        let t = f64::from(frame as f32);
        let sets: Vec<(usize, (Option<Quat>, Option<Vec3>, Option<f64>))> = tracks
            .iter()
            .enumerate()
            .filter_map(|(i, tr)| tr.as_ref().map(|tr| (self.skeleton[i], eval(tr, t))))
            .collect();
        for (n, (q, p, s)) in sets {
            if let Some(q) = q {
                self.nodes[n].lq = q;
            }
            if let Some(p) = p {
                self.nodes[n].lp = p;
            }
            if let Some(s) = s {
                self.nodes[n].ls = s;
            }
            invalidate(&mut self.nodes, n);
        }
    }

    /// The object node called `name` (@ 0x004bec20: the objects list;
    /// the list's first node when none matches).
    pub fn object_node(&self, name: &str) -> Option<usize> {
        self.objects
            .iter()
            .copied()
            .find(|&i| self.nodes[i].name == name)
            .or_else(|| self.objects.first().copied())
    }

    /// Position, rotation and scale of node `n` relative to the world
    /// (@ 0x004e5d00, @ 0x004e6150, @ 0x004e6810): a node under one of
    /// the scene's roots, which never move but for the board's own
    /// calls on them, so the parent's world values are taken by the C
    /// update.
    pub fn set_position(&mut self, n: usize, p: Vec3) {
        let lp = match self.nodes[n].parent {
            None => p,
            Some(pi) => {
                update_c(&mut self.nodes, pi);
                let par = &self.nodes[pi];
                let d = vsub(&p, &par.wp);
                let r = qrot_inv(&par.wq, &d);
                // @ 0x004e5d73: 1 / the parent's scale, times each.
                let inv = K.div(1.0, par.ws);
                r.map(|x| K.f(K.mul(inv, x)))
            }
        };
        self.nodes[n].lp = lp;
        invalidate(&mut self.nodes, n);
    }

    pub fn set_rotation(&mut self, n: usize, q: Quat) {
        if self.nodes[n].parent.is_some() {
            log::warn!("Licorne: rotation of a non-root node");
        }
        self.nodes[n].lq = q;
        invalidate(&mut self.nodes, n);
    }

    pub fn set_scale(&mut self, n: usize, s: f64) {
        let ls = match self.nodes[n].parent {
            None => s,
            Some(pi) => {
                update_c(&mut self.nodes, pi);
                let v = K.mul(K.div(1.0, self.nodes[pi].ws), s);
                if v == 0.0 {
                    1.0
                } else {
                    K.f(v)
                }
            }
        };
        self.nodes[n].ls = ls;
        invalidate(&mut self.nodes, n);
    }

    /// The world position of node `n` (@ 0x004e5e90, relative to the
    /// world): for a root, its local position.
    pub fn world_position(&mut self, n: usize) -> Vec3 {
        update_c(&mut self.nodes, n);
        self.nodes[n].wp
    }

    /// Draws the scene's objects (@ 0x004be1c0), or only the one called
    /// `only` (@ 0x004be580), with camera `cam` into `target`; each
    /// polygon's vertex records go to `records` (the rasteriser's input,
    /// 0x24 bytes a vertex) when given.
    pub fn draw(
        &mut self,
        cam: &Camera,
        target: &mut Target,
        only: Option<&str>,
        mut records: Option<&mut PolyLog>,
    ) {
        for mi in 0..self.meshes.len() {
            if only.is_some_and(|n| self.nodes[self.meshes[mi].node].name != n) {
                continue;
            }
            let mesh = &self.meshes[mi];
            let nv = mesh.vertices.len();
            let mut cv = vec![[0.0f64; 3]; nv];
            let mut codes = vec![0u8; nv];
            let and;
            match &mesh.bones {
                None => {
                    update_asm_node(K, &mut self.nodes, mesh.node);
                    let (st, out) = transform_pii(K, cam, &self.nodes[mesh.node], &mesh.vertices);
                    cv = out;
                    codes = st.codes;
                    and = codes.iter().fold(0xff, |a, &b| a & b);
                }
                Some(bones) => {
                    // @ 0x00690650: per bone of the skeleton list, its
                    // vertices through the transform with its matrix.
                    let mut a = 0xffu8;
                    for &b in &self.skeleton {
                        let idx: Vec<usize> = (0..nv).filter(|&v| bones[v] == b).collect();
                        if idx.is_empty() {
                            continue;
                        }
                        update_asm_node(K, &mut self.nodes, b);
                        let vs: Vec<Vec3> = idx.iter().map(|&v| mesh.vertices[v]).collect();
                        let (st, out) = transform_pii(K, cam, &self.nodes[b], &vs);
                        for (j, &v) in idx.iter().enumerate() {
                            cv[v] = out[j];
                            codes[v] = st.codes[j];
                            a &= st.codes[j];
                        }
                    }
                    and = a;
                }
            }
            if and & 0x3f != 0 {
                continue;
            }
            if codes.iter().any(|&c| c & 0x3f != 0) {
                log::debug!("Licorne: {} crosses the view (not clipped)", mesh.name);
            }
            let faces = back_faces_pii(K, &mesh.corners, &cv, &mut codes);
            let proj = project(K, cam, &cv, &codes);
            let (pixels, tw, th) = match &mesh.texture {
                Some((p, w, h)) => (p.as_slice(), *w, *h),
                None => (&[][..], 1, 1),
            };
            let tex = Texture {
                pixels,
                width: tw,
                height: th,
            };
            for (fi, tri) in mesh.corners.iter().enumerate() {
                if faces[fi] != 0 {
                    continue;
                }
                let mut poly: [Vertex; 3] = std::array::from_fn(|k| {
                    let p = proj[tri[k] as usize];
                    let uv = mesh.uvs[fi][k];
                    Vertex {
                        x: p[0],
                        y: p[1],
                        w: p[3],
                        u: f64::from(uv[0]),
                        v: f64::from(uv[1]),
                    }
                });
                if let Some(r) = records.as_deref_mut() {
                    r.add(&poly);
                }
                if !pixels.is_empty() {
                    linear_ii::draw_polygon(&mut poly, &tex, target);
                }
            }
        }
    }
}

impl Scene3 {
    /// Development: the skinned vertices per bone, in the skeleton
    /// list's order (`<tag> <bone> <x> <y> <z>`, float bits).
    #[doc(hidden)]
    pub fn dev_skin(&self, tag: &str) -> String {
        let mut out = String::new();
        for m in &self.meshes {
            let Some(bones) = &m.bones else { continue };
            for &b in &self.skeleton {
                for (v, &vb) in bones.iter().enumerate() {
                    if vb == b {
                        let p = m.vertices[v];
                        out += &format!(
                            "{tag} {} {:08x} {:08x} {:08x}\n",
                            self.nodes[b].name.replace(' ', "_"),
                            (p[0] as f32).to_bits(),
                            (p[1] as f32).to_bits(),
                            (p[2] as f32).to_bits()
                        );
                    }
                }
            }
        }
        out
    }
}

/// The polygons given to the rasteriser: their count and the md5 of
/// their vertex records (x, y, w, u, v as floats, then four zero
/// floats), as the golden's `poly` lines.
#[derive(Clone, Debug, Default)]
pub struct PolyLog {
    pub count: usize,
    pub bytes: Vec<u8>,
}

impl PolyLog {
    fn add(&mut self, poly: &[Vertex; 3]) {
        self.count += 1;
        for v in poly {
            for x in [v.x, v.y, v.w, v.u, v.v] {
                self.bytes.extend_from_slice(&(x as f32).to_le_bytes());
            }
            self.bytes.extend_from_slice(&[0; 16]);
        }
    }
}
