//! The 3D talking head of the dialogs: the lip-synced, textured lower face
//! drawn over the dialog video (the animated mouth).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/head3d.cpp, `DialogHead`; GPLv3, the ScummVM
//! team), which follows `atlantis.exe`: the line player `FUN_0042ee88`
//! loads the head (step 4: the speaker's INITDIAL row, the camera, the
//! line's `.SYC`), then every frame `FUN_0044dd10` (the head animation at
//! the UBB frame index), `FUN_0044e860` (the lip-sync at the elapsed time)
//! or `FUN_0044e2e0` (the neutral mouth), and `FUN_0044c9c0` draws the head
//! into the back buffer.
//!
//! The files are parsed by [`atlantis_data::head`]; this module animates,
//! transforms and projects the mesh, and [`raster`] draws it.
//!
//! # Arithmetic
//!
//! The arithmetic follows the original exactly: 32-bit wrapping products,
//! arithmetic right shifts, truncating divisions and conversions, and the
//! x87 at 53-bit precision (control word 0x127f), which is IEEE double
//! arithmetic; a float store of the original is an `as f32` here. Rust
//! never contracts `a * b + c` into a fused multiply-add, so every product
//! is rounded as the original rounds it.

mod raster;

use std::collections::HashMap;
use std::sync::OnceLock;

use atlantis_data::head::{
    parse_head_tracks, parse_mouth_shapes, parse_syc, F3dc, HeadNode, HeadTexture, HeadTrack, S3d,
    SycRecord,
};

/// No index (the original's null pointer).
const NONE: usize = usize::MAX;

#[inline]
fn wrap32(v: i64) -> i32 {
    v as i32
}

/// C integer division (toward zero), wrapped to 32 bits.
#[inline]
fn tdiv(a: i64, b: i64) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b) as i32
    }
}

/// `(v + (v < 0 ? 0x7fff : 0)) >> 15` of a wrapped product: division by
/// 32768 toward zero.
#[inline]
fn tsar15(v: i64) -> i32 {
    wrap32(v) / 32768
}

/// A double to int32, truncated; out of range (or NaN) gives 0x80000000,
/// as the x86's conversion does.
#[inline]
fn truncd(x: f64) -> i32 {
    if x > -2_147_483_649.0 && x < 2_147_483_648.0 {
        x as i32
    } else {
        i32::MIN
    }
}

/// A float store and reload.
#[inline]
fn f32r(x: f64) -> f64 {
    f64::from(x as f32)
}

/// `FUN_0045c8a0`: the trig tables (cos 0x00bea780, sin 0x00bee780, acos
/// 0x00be6780), 4096 entries each, 1.15.
/// The cos table is not read by the head.
struct TrigTables {
    sin: Vec<i32>,
    acos: Vec<i32>,
}

fn trig() -> &'static TrigTables {
    static T: OnceLock<TrigTables> = OnceLock::new();
    T.get_or_init(|| {
        let step = f64::from(0.001_533_980_8_f32); // float 0x004bf064
        let mut a = 0.0f64;
        let mut sin = Vec::with_capacity(4096);
        for _ in 0..4096 {
            sin.push(truncd(libm::sin(a) * 32768.0));
            a += step;
        }
        let acos = (-2048..2048)
            .map(|i| {
                truncd(
                    libm::acos(f64::from(i) * 0.000_488_281_25) * 2048.0 * 0.318_309_886_183_800_13,
                )
            })
            .collect();
        TrigTables { sin, acos }
    })
}

/// `FUN_0045d460`: the slerp of two 1.15 quaternions, weight 0..256.
fn slerp(q0: &[i32; 4], q1: &[i32; 4], w: i32) -> [i32; 4] {
    let t = trig();
    // 32-bit IMUL / ADD then SAR 15 (0x0045d46a..0x0045d493)
    let dot = i64::from(
        wrap32(
            i64::from(q0[0]) * i64::from(q1[0])
                + i64::from(q0[1]) * i64::from(q1[1])
                + i64::from(q0[2]) * i64::from(q1[2])
                + i64::from(q0[3]) * i64::from(q1[3]),
        ) >> 15,
    );
    // The opposite-quaternion branch (0x0045d5ab) is not ported: no rotation
    // key pair of the shipped *A.3DA files reaches it.
    if dot + 0x8000 <= 20 {
        log::warn!("dialog head: FUN_0045d460 opposite-quaternion branch (0x0045d5ab) not ported");
    }
    let e = 256 - w;
    let (a, b);
    if 0x8000 - dot <= 20 {
        a = tdiv(i64::from(e) << 15, 256);
        b = tdiv(i64::from(w) << 15, 256);
    } else {
        let idx = truncd(dot as f64 * 3.051_757_812_5e-05 * 2048.0 + 2048.0);
        let at = |tab: &[i32], i: i32| usize::try_from(i).ok().and_then(|i| tab.get(i).copied());
        let th = at(&t.acos, idx).unwrap_or(0);
        let s = at(&t.sin, th).unwrap_or(0);
        let se = at(&t.sin, tdiv(i64::from(e) * i64::from(th), 256)).unwrap_or(0);
        let sw = at(&t.sin, tdiv(i64::from(w) * i64::from(th), 256)).unwrap_or(0);
        a = tdiv(i64::from(se) << 15, i64::from(s));
        b = tdiv(i64::from(sw) << 15, i64::from(s));
    }
    let mut out = [0; 4];
    for k in 0..4 {
        out[k] = wrap32(i64::from(q0[k]) * i64::from(a) + i64::from(b) * i64::from(q1[k])) >> 15;
    }
    out
}

/// `FUN_0045d128`: a quaternion to a 3x3 matrix (1.15).
fn quat_matrix(q: &[i32; 4]) -> [i32; 9] {
    let (a, b, c, d) = (
        i64::from(q[0]),
        i64::from(q[1]),
        i64::from(q[2]),
        i64::from(q[3]),
    );
    let (xx, yy, zz) = (
        i64::from(wrap32(a * a)),
        i64::from(wrap32(b * b)),
        i64::from(wrap32(c * c)),
    );
    let mut m = [0; 9];
    m[0] = 0x8000 - (wrap32(yy + zz) >> 14);
    m[4] = 0x8000 - (wrap32(xx + zz) >> 14);
    m[8] = 0x8000 - (wrap32(xx + yy) >> 14);
    let (ab, cd) = (i64::from(wrap32(a * b)), i64::from(wrap32(c * d)));
    m[1] = wrap32(ab - cd) >> 14;
    m[3] = wrap32(ab + cd) >> 14;
    let (ac, bd) = (i64::from(wrap32(a * c)), i64::from(wrap32(b * d)));
    m[2] = wrap32(ac + bd) >> 14;
    m[6] = wrap32(ac - bd) >> 14;
    let (bc, da) = (i64::from(wrap32(b * c)), i64::from(wrap32(d * a)));
    m[5] = wrap32(bc - da) >> 14;
    m[7] = wrap32(bc + da) >> 14;
    m
}

/// `FUN_0045d004`: `(lo * (256 - w) + hi * w) >> 8` per coordinate.
fn lerp3(lo: &[i32], hi: &[i32], w: i32) -> [i32; 3] {
    let mut r = [0; 3];
    for k in 0..3 {
        r[k] = wrap32(i64::from(lo[k]) * i64::from(256 - w) + i64::from(hi[k]) * i64::from(w)) >> 8;
    }
    r
}

/// `FUN_0045cda8`: `(A x B) >> 15`.
fn matmul(a: &[i32; 9], b: &[i32; 9]) -> [i32; 9] {
    let mut r = [0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[3 * i + j] = wrap32(
                i64::from(a[3 * i]) * i64::from(b[j])
                    + i64::from(a[3 * i + 1]) * i64::from(b[3 + j])
                    + i64::from(a[3 * i + 2]) * i64::from(b[6 + j]),
            ) >> 15;
        }
    }
    r
}

/// `FUN_0045cf7c`: `v * 32768 / |v|`, truncated; `|v|^2` summed y, x, z.
fn normalise(v: &[i32; 3]) -> [i32; 3] {
    let sq = |a: i32| (i64::from(a) * i64::from(a)) as f64;
    let s = (sq(v[1]) + sq(v[0]) + sq(v[2])).sqrt();
    let inv = 1.0 / s;
    let mut out = [0; 3];
    for k in 0..3 {
        out[k] = truncd(f64::from(wrap32(i64::from(v[k]) << 15)) * inv);
    }
    out
}

/// `FUN_0045caec(n, roll 0)`: the camera rotation from the normalised
/// direction `n`.
fn basis(n: &[i32; 3]) -> [i32; 9] {
    let c = f64::from(1.0f32 / 32768.0f32);
    let k = 32768.0;
    let (x, y, z) = (
        f64::from(n[0]) * c,
        f64::from(n[1]) * c,
        f64::from(n[2]) * c,
    );
    let s = (1.0 - y * y).sqrt();
    let inv = 1.0 / s;
    [
        truncd(z * inv * k),
        truncd((-x) * y * inv * k),
        n[0],
        0,
        truncd(s * k),
        n[1],
        truncd((-x) * inv * k),
        truncd(k * (z * (-y) * inv)),
        n[2],
    ]
}

/// A projected vertex of the drawn node: visible (0x40 set) and its screen
/// position.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProjectedVertex {
    pub flags: u32,
    pub x: i32,
    pub y: i32,
    /// 2^30 / z, as the rasteriser reads it.
    pub iz: f32,
}

/// A node of the mesh with its per-frame state.
#[derive(Clone)]
struct Node {
    d: HeadNode,
    /// The world rotation and position of the last walk.
    w: [i32; 9],
    wp: [i32; 3],
    proj: Vec<ProjectedVertex>,
    /// Per face, the three edge records (indices into `DialogHead::edges`).
    face_edges: Vec<[usize; 3]>,
    drawn: bool,
}

/// An edge record of the original (0x58 / 0x64 bytes, kept in the loaded
/// block between frames): one per face corner, shared by the two faces of
/// an edge.
#[derive(Clone, Copy, Debug)]
struct Edge {
    va: usize,
    vb: usize,
    node: usize,
    flags: u32,
    x: i32,
    dx: i32,
    iz: f32,
    diz: f32,
    dizdx: f32,
    next: usize,
    pred: usize,
    shade: i32,
    tex: usize,
    // group type 2 (16.16)
    u: i32,
    v: i32,
    du: i32,
    dv: i32,
    dudx: i32,
    dvdx: i32,
    // group type 9 (u / z, v / z)
    pu: f32,
    pv: f32,
    dpu: f32,
    dpv: f32,
    dpudx: f32,
    dpvdx: f32,
    a58: f32,
    a5c: f32,
    a60: f32,
    gtype: u32,
    /// Group type 1.
    colour: u16,
    entry: usize,
}

impl Default for Edge {
    fn default() -> Edge {
        Edge {
            va: NONE,
            vb: NONE,
            node: NONE,
            flags: 0,
            x: 0,
            dx: 0,
            iz: 0.0,
            diz: 0.0,
            dizdx: 0.0,
            next: NONE,
            pred: NONE,
            shade: 0,
            tex: NONE,
            u: 0,
            v: 0,
            du: 0,
            dv: 0,
            dudx: 0,
            dvdx: 0,
            pu: 0.0,
            pv: 0.0,
            dpu: 0.0,
            dpv: 0.0,
            dpudx: 0.0,
            dpvdx: 0.0,
            a58: 0.0,
            a5c: 0.0,
            a60: 0.0,
            gtype: 0,
            colour: 0,
            entry: NONE,
        }
    }
}

/// The number of sentinel edges after the records (`FUN_0045c104`).
const SENTINELS: usize = 6;

/// The paths of a head's five files: the `.3DC`, the `A.3DA`, the `M.3DA`,
/// the `.S3D` and the line's `.SYC`, archive names in upper case.
pub fn head_paths(s3d: &str, c3dc: &str, a3da: &str, m3da: &str, syc: &str) -> [String; 5] {
    let p = "DIALOG\\PERSO\\";
    [
        format!("{p}{c3dc}.3DC"),
        format!("{p}{a3da}.3DA"),
        format!("{p}{m3da}.3DA"),
        format!("{p}{s3d}.S3D"),
        format!("WAV\\{syc}"),
    ]
    .map(|s| s.to_ascii_uppercase())
}

/// The texture maps a mesh needs (`DIALOG\MAPS\<name>.3DM`), in load order.
pub fn texture_paths(mesh: &F3dc) -> Vec<String> {
    let textures: HashMap<&str, &str> = mesh
        .materials
        .iter()
        .map(|m| (m.name.as_str(), m.texture.as_str()))
        .collect();
    let mut out: Vec<String> = Vec::new();
    for n in &mesh.nodes {
        for g in &n.groups {
            if let Some(t) = textures.get(g.material.as_str()).filter(|t| !t.is_empty()) {
                if matches!(g.gtype, 2 | 9) {
                    let p = format!("DIALOG\\MAPS\\{t}.3DM").to_ascii_uppercase();
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

/// The 3D head of a dialog line.
#[derive(Clone)]
#[cfg_attr(test, derive(Default))]
pub struct DialogHead {
    nodes: Vec<Node>,
    tracks: Vec<HeadTrack>,
    /// Per shape: the pivot then the vertices.
    shapes: Vec<Vec<[i32; 3]>>,
    /// S3D: the meshes the mouth deforms.
    meshes: Vec<usize>,
    /// S3D: per mesh, the shape of each entry.
    rows: Vec<Vec<i32>>,
    phon: [[i32; 6]; 27],
    cams: Vec<[f64; 8]>,
    syc: Vec<SycRecord>,
    /// Material -> the material record's +0x20.
    colours: HashMap<String, u32>,

    frames: i32,
    frame: i32,
    /// Enhancement (minor bug fixes): frame 0 of the head animation is
    /// drawn with frame 1's pose (see [`DialogHead::set_rest_key_fix`]).
    rest_key_fix: bool,
    idx: usize,
    next: i32,
    prev: i32,
    blend: i32,
    root_w: [i32; 9],
    root_p: [i32; 3],
    fx: i32,
    fy: i32,
    cx: i32,
    cy: i32,
    near: i32,
    far: i32,
    kx: i32,
    ky: i32,
    planes: [[i32; 3]; 4],
    drawn_node: usize,

    /// The edge records sorted by record id, then the sentinels.
    edges: Vec<Edge>,
    textures: Vec<HeadTexture>,
    /// Material -> index into `textures`.
    texture_index: HashMap<String, usize>,
    raster: raster::RasterState,
    /// The camera (index into `cams`).
    cam: usize,
    /// The output: 640x480 for the original; the port's HD mode draws a
    /// copy of the head at `tile` / 16 times that (see
    /// [`DialogHead::scaled`]).
    out_w: i32,
    out_h: i32,
    tile: i32,
}

impl DialogHead {
    /// `FUN_0042ee88` step 4: `s3d`, `c3dc`, `a3da`, `m3da` are the INITDIAL
    /// row's base names, `cam` the camera 2..5, `syc` the voice's `.SYC`
    /// (`ATA<NNNN><letter>.SYC`); `read` gives an archive member by name.
    /// `None` when a file is missing or malformed.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        mut read: impl FnMut(&str) -> Option<Vec<u8>>,
        s3d: &str,
        c3dc: &str,
        a3da: &str,
        m3da: &str,
        cam: i32,
        syc: &str,
    ) -> Option<DialogHead> {
        let [p3dc, pa, pm, ps, py] = head_paths(s3d, c3dc, a3da, m3da, syc);
        let (f3dc, fa, fm, fs, fy) = (read(&p3dc)?, read(&pa)?, read(&pm)?, read(&ps)?, read(&py)?);
        let mesh = F3dc::parse(&f3dc)?;
        let tracks = parse_head_tracks(&fa)?;
        let s3d = S3d::parse(&fs)?;
        let syc = parse_syc(&fy)?;
        let mut meshes = Vec::new();
        for &m in &s3d.meshes {
            meshes.push(usize::try_from(m).ok().filter(|&m| m < mesh.nodes.len())?);
        }
        let nv = meshes
            .iter()
            .map(|&m| mesh.nodes[m].verts.len())
            .max()
            .unwrap_or(0);
        let shapes = parse_mouth_shapes(&fm, nv)?;
        if cam < 2 || (cam - 2) as usize >= s3d.cams.len() || tracks.is_empty() {
            return None;
        }

        let mut colours = HashMap::new();
        let mut tex_names = HashMap::new();
        for m in &mesh.materials {
            tex_names.insert(m.name.clone(), m.texture.clone());
            // FUN_0044ac84 @ 0x0044ace6: a colour group's +8 is the record's
            // +0x20
            colours.insert(m.name.clone(), m.colour);
        }

        let mut nodes: Vec<Node> = mesh
            .nodes
            .into_iter()
            .map(|d| Node {
                d,
                w: [0; 9],
                wp: [0; 3],
                proj: Vec::new(),
                face_edges: Vec::new(),
                drawn: false,
            })
            .collect();
        // FUN_00418b68: every node hidden (FUN_0044c250); FUN_00418e0c: the
        // s3d meshes shown (FUN_0044c284)
        for n in &mut nodes {
            n.d.flags |= 4;
        }
        // Not ported, no shipped .3DC sets these flags: node flag 0x10
        // (FUN_00463398 0x004634a1..0x004634f3: a culled node is drawn when
        // its parent is not, and FUN_004619f0 transforms the parent's 0x80
        // vertices) and vertex flag 0x80 (FUN_004618b0: visible, 0x40, as
        // soon as z >= near).
        for n in &nodes {
            if n.d.flags & 0x10 != 0 {
                log::warn!("dialog head: node flag 0x10 (FUN_00463398 0x004634a1) not ported");
            }
            if n.d.vertex_flag80 {
                log::warn!("dialog head: vertex flag 0x80 (FUN_004618b0, FUN_004619f0) not ported");
            }
        }
        for &m in &meshes {
            nodes[m].d.flags &= !4;
        }
        // The edge records, in block order (the order FUN_004567b4 walks
        // them).
        let mut ids: Vec<u32> = nodes
            .iter()
            .flat_map(|n| n.d.faces.iter().flat_map(|f| f.recs))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let blank = Edge {
            gtype: 2,
            ..Edge::default()
        };
        let mut edges = vec![blank; ids.len()];
        let edge_index: HashMap<u32, usize> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let mut textures = Vec::new();
        let mut texture_index = HashMap::new();
        for (ni, n) in nodes.iter_mut().enumerate() {
            n.face_edges = Vec::with_capacity(n.d.faces.len());
            for f in &n.d.faces {
                let mut fe = [0; 3];
                for k in 0..3 {
                    let ei = edge_index[&f.recs[k]];
                    fe[k] = ei;
                    if edges[ei].va == NONE {
                        // the corners of the first face that names the record
                        edges[ei].va = f.vi[k];
                        edges[ei].vb = f.vi[(k + 1) % 3];
                        edges[ei].node = ni;
                    }
                }
                n.face_edges.push(fe);
            }
            for g in &n.d.groups {
                let t = tex_names.get(&g.material).map_or("", String::as_str);
                if matches!(g.gtype, 2 | 9)
                    && !t.is_empty()
                    && !texture_index.contains_key(&g.material)
                {
                    let path = format!("DIALOG\\MAPS\\{t}.3DM").to_ascii_uppercase();
                    let tex = read(&path).and_then(|d| HeadTexture::parse(&d))?;
                    texture_index.insert(g.material.clone(), textures.len());
                    textures.push(tex);
                }
            }
        }
        edges.extend(std::iter::repeat_n(Edge::default(), SENTINELS));

        let mut h = DialogHead {
            nodes,
            // FUN_0044da90: FUN_00468534(...) + 1
            frames: tracks[0].last.wrapping_add(1),
            rest_key_fix: false,
            tracks,
            shapes,
            meshes,
            rows: s3d.rows,
            phon: s3d.phonemes,
            cams: s3d.cams,
            syc,
            colours,
            frame: 0,
            idx: 1,
            next: 0,
            prev: 0,
            blend: 0,
            root_w: [0; 9],
            root_p: [0; 3],
            fx: 0,
            fy: 0,
            cx: 320,
            cy: 240,
            near: 0x80,
            far: 65000,
            kx: 0,
            ky: 0,
            planes: [[0; 3]; 4],
            drawn_node: NONE,
            edges,
            textures,
            texture_index,
            raster: raster::RasterState::default(),
            cam: 0,
            out_w: 640,
            out_h: 480,
            tile: 16,
        };
        h.camera((cam - 2) as usize);
        h.neutral();
        Some(h)
    }

    /// `FUN_0044d7b0(slot, c)`: the camera.
    fn camera(&mut self, c: usize) {
        self.cam = c;
        let r = self.cams[c];
        let mut d = [0; 3];
        let mut p = [0; 3];
        for k in 0..3 {
            d[k] = truncd(r[k] * r[6] - r[3 + k] * r[6]);
            p[k] = truncd(r[3 + k] * r[6]);
        }
        let m = basis(&normalise(&d));
        for i in 0..3 {
            for j in 0..3 {
                self.root_w[3 * i + j] = m[3 * j + i]; // FUN_0045ce1c
            }
        }
        for i in 0..3 {
            // FUN_0045ce64, FUN_0045cef4
            let w = &self.root_w;
            self.root_p[i] = -(wrap32(
                i64::from(w[3 * i]) * i64::from(p[0])
                    + i64::from(w[3 * i + 1]) * i64::from(p[1])
                    + i64::from(w[3 * i + 2]) * i64::from(p[2]),
            ) >> 15);
        }
        // the original's own pi, 3.1415926535897, not f64::consts::PI
        #[allow(clippy::approx_constant)]
        let pi = 3.141_592_653_589_7;
        let focal = truncd(640.0 * 0.5 / libm::tan(r[7] * pi * 0.002_777_777_777_777_777_5));
        self.fx = tdiv(i64::from(focal) * 640, 640);
        // (float)((480 * 4.0) / (640 * 3.0)), doubles 0x004bee63 = 4.0 and
        // 0x004bee6b = 3.0: 1.0 for the 640x480 screen
        let aspect = 1.0;
        self.fy = truncd(f64::from(self.fx) * aspect);
        self.cx = 320;
        self.cy = 240;
        self.near = 0x80;
        self.far = 65000;
        self.kx = tdiv(640i64 << 15, -2 * i64::from(self.fx));
        self.ky = tdiv(480i64 << 15, -2 * i64::from(self.fy));
        let (fx, fy) = (self.fx, self.fy);
        let pl = [[-fx, 0, -320], [fx, 0, -320], [0, -fy, -240], [0, fy, -240]];
        self.planes = pl.map(|p| normalise(&p));
        if self.tile != 16 {
            self.scale_camera();
        }
    }

    /// The port's HD mode: the camera of [`DialogHead::camera`] for an
    /// output of `tile` / 16 times 640x480, the same view (focal lengths
    /// and centre scaled; the clipping planes are the same).
    fn scale_camera(&mut self) {
        let t = i64::from(self.tile);
        self.fx = tdiv(i64::from(self.fx) * t, 16);
        self.fy = tdiv(i64::from(self.fy) * t, 16);
        self.cx = self.out_w / 2;
        self.cy = self.out_h / 2;
        self.kx = tdiv(i64::from(self.out_w) << 15, -2 * i64::from(self.fx));
        self.ky = tdiv(i64::from(self.out_h) << 15, -2 * i64::from(self.fy));
        let (fx, fy, hw, hh) = (self.fx, self.fy, self.cx, self.cy);
        let pl = [[-fx, 0, -hw], [fx, 0, -hw], [0, -fy, -hh], [0, fy, -hh]];
        self.planes = pl.map(|p| normalise(&p));
    }

    /// A copy of the head drawn at `tile` / 16 times the original's size
    /// (into a `40 * tile` x `30 * tile` buffer, see
    /// [`crate::warp::hd_size`]): the port's HD mode. The copy is animated
    /// by the same calls as the original-size head; the original-size
    /// head itself is not changed.
    pub fn scaled(&self, tile: usize) -> DialogHead {
        let mut h = self.clone();
        let (w, ht) = crate::warp::hd_size(tile);
        h.out_w = w as i32;
        h.out_h = ht as i32;
        h.tile = tile as i32;
        h.camera(h.cam);
        h
    }

    /// The output size the head is drawn for (640x480, or the HD size of
    /// [`DialogHead::scaled`]).
    pub fn output_size(&self) -> (usize, usize) {
        (self.out_w as usize, self.out_h as usize)
    }

    /// `FUN_00468458`: a mouth shape blended into a mesh at weight `w`.
    fn morph(&mut self, mesh: usize, shape: i32, w: i32) {
        if w == 0 {
            return;
        }
        let Some(sh) = usize::try_from(shape).ok().and_then(|s| self.shapes.get(s)) else {
            log::warn!("dialog head: no mouth shape {shape}");
            return;
        };
        let v = &mut self.nodes[mesh].d.verts;
        for (i, vert) in v.iter_mut().enumerate() {
            let s = &sh[i + 1];
            *vert = if w == 255 { *s } else { lerp3(vert, s, w) };
        }
    }

    /// The shape of entry `k` of mesh row `r`.
    fn row_shape(&self, r: usize, k: i32) -> i32 {
        usize::try_from(k)
            .ok()
            .and_then(|k| self.rows[r].get(k).copied())
            .unwrap_or(-1)
    }

    /// `FUN_00468990(EAX = 3dc, EDX = EBX = A.3da + frame, ECX = 255,
    /// flags 0)`, then `FUN_0046853c` per node: the head animation at the
    /// current frame.
    fn head_anim(&mut self) {
        if self.frame >= self.frames {
            return;
        }
        let fr = if self.rest_key_fix && self.frame == 0 && self.frames > 1 {
            1
        } else {
            self.frame
        };
        // (frame * (256 - 255) + frame * 255) / 256, the blend of the frame
        // with itself at weight 255
        let f = f32r(f64::from(fr.wrapping_mul(256)) * f64::from(1.0f32 / 256.0f32));
        for (n, track) in self.nodes.iter_mut().zip(&self.tracks) {
            for kind in 0..2 {
                let keys = if kind == 0 { &track.rot } else { &track.pos };
                if keys.len() <= 1 {
                    continue;
                }
                let (mut lo, mut hi) = (0, keys.len() - 1);
                while lo + 1 < hi {
                    let mid = (lo + hi) / 2;
                    if f64::from(keys[mid].t) < f {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let k1 = &keys[hi];
                if f64::from(k1.t) > f {
                    let k0 = &keys[hi - 1];
                    let w =
                        truncd((f - f64::from(k0.t)) * 256.0 / f64::from(k1.t.wrapping_sub(k0.t)));
                    if kind == 0 {
                        n.d.m = quat_matrix(&slerp(&k0.v, &k1.v, w));
                    } else {
                        n.d.pos = lerp3(&k0.v, &k1.v, w);
                    }
                } else if kind == 0 {
                    n.d.m = quat_matrix(&k1.v);
                } else {
                    n.d.pos = [k1.v[0], k1.v[1], k1.v[2]];
                }
            }
        }
    }

    /// `FUN_0044dba0`.
    fn reset(&mut self) {
        self.idx = 1;
        self.next = -1;
        self.prev = -1;
        self.blend = 0x7f;
    }

    /// `FUN_0044dd10`: the UBB frame index drives the head animation.
    pub fn set_frame(&mut self, frame: i32) {
        self.frame = frame;
    }

    /// Enhancement (minor bug fixes). The original evaluates the head
    /// animation at the UBB frame index, so frame 0 (the first frame of a
    /// line and of every loop of its video) uses the first key of each
    /// track. In some `*A.3DA` files that key is a rest pose that is not
    /// part of the loop (Meljanz, Actyon, Servage, the Queen, Gimbas...):
    /// the 3D lower face is drawn up to 35 pixels away from the video's face
    /// for that one frame. With the fix, frame 0 is drawn with frame 1's
    /// pose, which every loop's last frame also matches in those files.
    pub fn set_rest_key_fix(&mut self, on: bool) {
        self.rest_key_fix = on;
    }

    /// `FUN_0044e2e0`: the neutral mouth.
    pub fn neutral(&mut self) {
        let e = self.phon[26];
        for r in 0..self.meshes.len() {
            let mesh = self.meshes[r];
            // the >= 0 tests of 0x0044e55d / 0x0044e5a7 are on the mapped
            // row value
            let s1 = if e[2] >= 0 && e[3] >= 0 {
                self.row_shape(r, e[2])
            } else {
                -1
            };
            let s2 = if e[4] >= 0 && e[5] >= 0 {
                self.row_shape(r, e[4])
            } else {
                -1
            };
            self.morph(mesh, self.row_shape(r, e[0]), 255);
            if s1 >= 0 {
                self.morph(mesh, s1, e[3]);
            }
            if s2 >= 0 {
                self.morph(mesh, s2, e[5]);
            }
        }
        self.head_anim();
        self.reset();
    }

    /// `FUN_0044e860`: the mouth at `t` ms; 0, or -1 once the track has
    /// ended.
    pub fn lipsync(&mut self, t: i32) -> i32 {
        let last = self.syc.len() - 1;
        if t >= self.syc[last].t {
            self.reset();
            return -1;
        }
        if t != 0 {
            loop {
                if self.syc[self.idx - 1].t < t && t < self.syc[self.idx].t {
                    break;
                }
                if t > self.syc[self.idx].t {
                    self.idx += 1;
                } else {
                    break;
                }
            }
            if self.idx > last {
                self.idx = last;
            }
        }
        // every phoneme of the shipped .SYC files is 0..0x1a
        if (self.syc[self.idx].phoneme as u32) <= 0x1a {
            self.next = self.syc[self.idx].phoneme;
        }
        if (self.syc[self.idx - 1].phoneme as u32) <= 0x1a {
            self.prev = self.syc[self.idx - 1].phoneme;
        }
        let (t0, t1) = (self.syc[self.idx - 1].t, self.syc[self.idx].t);
        let frac = f64::from(t.wrapping_sub(t0)) / f64::from(t1.wrapping_sub(t0));
        self.blend = truncd(frac * 255.0).clamp(0, 255);
        // prev and next are always set above for the shipped tracks; the
        // fallback only keeps a malformed track inside the table
        let pp = self.phon[if self.prev >= 0 {
            self.prev as usize
        } else {
            26
        }];
        let nn = self.phon[if self.next >= 0 {
            self.next as usize
        } else {
            26
        }];
        let c = 0.003_921_568_627_450_98;
        let b = self.blend;
        let w = [
            f64::from(255 - b),
            f64::from(pp[3].wrapping_mul(255 - b)) * c,
            f64::from(pp[5].wrapping_mul(255 - b)) * c,
            f64::from(b),
            f64::from(nn[3].wrapping_mul(b)) * c,
            f64::from(nn[5].wrapping_mul(b)) * c,
        ];
        let idx = [pp[0], pp[2], pp[4], nn[0], nn[2], nn[4]];
        for r in 0..self.meshes.len() {
            // the row value, -1 for a negative table index; the >= 0 test of
            // 0x0044f054 is on this mapped value
            let s = idx.map(|i| if i < 0 { -1 } else { self.row_shape(r, i) });
            for k in 0..6 {
                if 0.0 > w[k] || s[k] < 0 {
                    continue;
                }
                self.morph(self.meshes[r], s[k], truncd(w[k]));
            }
        }
        self.head_anim();
        if self.idx >= last || t >= self.syc[last].t {
            self.reset();
            return -1;
        }
        0
    }

    /// `FUN_00461660`: the bounding sphere against the four side planes
    /// and near / far.
    fn node_culled(&self, n: &Node) -> bool {
        let mut c = [0; 3];
        for (i, ci) in c.iter_mut().enumerate() {
            *ci = n.wp[i].wrapping_add(tsar15(
                i64::from(n.w[3 * i]) * i64::from(n.d.center[0])
                    + i64::from(n.w[3 * i + 1]) * i64::from(n.d.center[1])
                    + i64::from(n.w[3 * i + 2]) * i64::from(n.d.center[2]),
            ));
        }
        let mut out = false;
        for p in &self.planes {
            let d = wrap32(
                i64::from(c[0]) * i64::from(p[0])
                    + i64::from(c[1]) * i64::from(p[1])
                    + i64::from(c[2]) * i64::from(p[2]),
            ) >> 15;
            if d >= 1 && n.d.radius < d {
                out = true;
            }
        }
        let z = c[2];
        if z <= self.near && self.near >= z.wrapping_add(n.d.radius) {
            out = true;
        }
        if z >= self.far && z.wrapping_sub(n.d.radius) >= self.far {
            out = true;
        }
        out
    }

    /// `FUN_004618b0`, `FUN_00461b98`, `FUN_00462ebc`, `FUN_00463060`: the
    /// node transformed, its faces culled and its vertices projected; then
    /// its groups set up for the rasteriser (`FUN_00459fd0`).
    fn draw(&mut self, ni: usize) {
        if self.node_culled(&self.nodes[ni]) {
            return;
        }
        let (near, far, kx, ky) = (self.near, self.far, self.kx, self.ky);
        let n = &mut self.nodes[ni];
        let (w, wp) = (n.w, n.wp);
        let nv = n.d.verts.len();
        // FUN_004618b0
        let mut cam = vec![[0i32; 3]; nv];
        let mut vf = vec![0u32; nv];
        for v in 0..nv {
            let p = n.d.verts[v];
            let c = &mut cam[v];
            for i in 0..3 {
                c[i] = tsar15(
                    i64::from(w[3 * i]) * i64::from(p[0])
                        + i64::from(w[3 * i + 1]) * i64::from(p[1])
                        + i64::from(w[3 * i + 2]) * i64::from(p[2]),
                )
                .wrapping_add(wp[i]);
            }
            let mut fl = 0;
            if c[2] < near {
                fl |= 0x10;
            } else if c[2] >= far {
                fl |= 0x20;
            }
            let tx = wrap32(i64::from(kx) * i64::from(c[2])) >> 15;
            let ty = wrap32(i64::from(ky) * i64::from(c[2])) >> 15;
            if c[0] < tx {
                fl |= 2;
            } else if tx.wrapping_neg() < c[0] {
                fl |= 1;
            }
            if ty < c[1] {
                if ty.wrapping_neg() < c[1] {
                    fl |= 8;
                }
            } else {
                fl |= 4;
            }
            vf[v] = fl;
        }
        // FUN_00461b98: the eye in object space against each face plane
        let mut e = [0i64; 3];
        for j in 0..3 {
            e[j] = (-i64::from(wrap32(
                i64::from(w[j]) * i64::from(wp[0])
                    + i64::from(w[3 + j]) * i64::from(wp[1])
                    + i64::from(w[6 + j]) * i64::from(wp[2]),
            ))) >> 15;
        }
        let pd: Vec<i64> =
            n.d.planes
                .iter()
                .map(|pl| {
                    i64::from(tsar15(e[1].wrapping_mul(i64::from(pl[1]))))
                        + i64::from(tsar15(e[0].wrapping_mul(i64::from(pl[0]))))
                        + i64::from(tsar15(e[2].wrapping_mul(i64::from(pl[2]))))
                })
                .collect();
        // FUN_00462ebc
        for fc in &mut n.d.faces {
            let mut fl = fc.flags;
            let hidden = if fl & 8 == 0 {
                pd[fc.plane] - i64::from(fc.d) < 0
            } else {
                let (a, b, c3) = (cam[fc.vi[0]], cam[fc.vi[1]], cam[fc.vi[2]]);
                let mut d1 = [0i64; 3];
                let mut d2 = [0i64; 3];
                for k in 0..3 {
                    d1[k] = i64::from(a[k].wrapping_sub(b[k]));
                    d2[k] = i64::from(b[k].wrapping_sub(c3[k]));
                }
                let x = |p: i64, q: i64, r: i64, s: i64| i64::from(wrap32(p * q - r * s));
                let cr = wrap32(
                    (x(d1[0], d2[1], d1[1], d2[0]) * i64::from(a[2]))
                        .wrapping_add(x(d1[2], d2[0], d1[0], d2[2]) * i64::from(a[1]))
                        .wrapping_add(x(d1[1], d2[2], d1[2], d2[1]) * i64::from(a[0])),
                );
                cr >= 0
            };
            if hidden {
                fl |= 1;
            } else {
                fl &= 8;
                let (f0, f1, f2) = (vf[fc.vi[0]], vf[fc.vi[1]], vf[fc.vi[2]]);
                if (f0 & f1 & f2 & 0x3f) != 0 || ((f0 | f1 | f2) & 0x20) != 0 {
                    fl |= 1;
                } else if (f0 | f1 | f2) & 0x10 != 0 {
                    // FUN_00461ecc (near-plane clipping) is not ported: no
                    // shipped head and camera reaches it.
                    log::warn!(
                        "dialog head: near-plane clipping FUN_00461ecc not ported; face skipped"
                    );
                    fl |= 1;
                } else {
                    for k in 0..3 {
                        vf[fc.vi[k]] |= 0x40;
                    }
                }
            }
            fc.flags = fl;
        }
        // FUN_00463060
        let (fx, fy, cx, cy) = (self.fx, self.fy, self.cx, self.cy);
        n.proj.clear();
        n.proj.extend(cam.iter().zip(&vf).map(|(c, &f)| {
            if f & 0x40 != 0 {
                ProjectedVertex {
                    flags: f,
                    // 32-bit IMUL then CDQ / IDIV (0x00463084, 0x004630a8);
                    // FILD of the exact z (0x00463091)
                    x: tdiv(
                        i64::from(wrap32(i64::from(c[0]) * i64::from(fx))),
                        i64::from(c[2]),
                    )
                    .wrapping_add(cx),
                    y: tdiv(
                        i64::from(wrap32(i64::from(c[1]) * i64::from(fy))),
                        i64::from(c[2]),
                    )
                    .wrapping_add(cy),
                    iz: (f64::from(1_073_741_824.0f32) / f64::from(c[2])) as f32,
                }
            } else {
                ProjectedVertex {
                    flags: f,
                    ..ProjectedVertex::default()
                }
            }
        }));
        n.drawn = true;
        // FUN_00459fd0: one set-up per group, in list order
        for g in 0..self.nodes[ni].d.groups.len() {
            if matches!(self.nodes[ni].d.groups[g].gtype, 1 | 2 | 9) {
                self.setup_group(ni, g);
            }
        }
    }

    /// The walk of the tree (`FUN_00463530`), drawing the shown nodes.
    fn visit(&mut self, ni: usize, w: &[i32; 9], wp: &[i32; 3]) {
        let (m, pos) = (self.nodes[ni].d.m, self.nodes[ni].d.pos);
        let w2 = matmul(w, &m);
        let mut wp2 = [0; 3];
        for i in 0..3 {
            wp2[i] = wp[i].wrapping_add(tdiv(
                i64::from(wrap32(
                    i64::from(w[3 * i]) * i64::from(pos[0])
                        + i64::from(w[3 * i + 1]) * i64::from(pos[1])
                        + i64::from(w[3 * i + 2]) * i64::from(pos[2]),
                )),
                32768,
            ));
        }
        self.nodes[ni].w = w2;
        self.nodes[ni].wp = wp2;
        if self.nodes[ni].d.flags & 4 == 0 {
            self.draw(ni);
        }
        let mut c = self.nodes[ni].d.child;
        while let Some(ci) = c {
            if self.nodes[ci].d.flags & 1 == 0 {
                self.visit(ci, &w2, &wp2);
            }
            c = self.nodes[ci].d.sibling;
        }
    }

    /// `FUN_00463530` up to the projection (`FUN_00463060`), for every shown
    /// node.
    pub fn transform(&mut self) {
        for n in &mut self.nodes {
            n.drawn = false;
        }
        if self.nodes.is_empty() {
            return;
        }
        let (w, p) = (self.root_w, self.root_p);
        self.visit(0, &w, &p);
        self.drawn_node = NONE;
        for (i, n) in self.nodes.iter().enumerate() {
            if n.drawn
                && (self.drawn_node == NONE
                    || n.d.verts.len() > self.nodes[self.drawn_node].d.verts.len())
            {
                self.drawn_node = i;
            }
        }
    }

    /// The node drawn (the largest shown mesh) and its projection, for the
    /// tests.
    pub fn projected(&self) -> &[ProjectedVertex] {
        self.nodes.get(self.drawn_node).map_or(&[], |n| &n.proj)
    }

    /// `FUN_0044c9c0` after [`DialogHead::transform`]: the head drawn over
    /// a 640x480 RGB565 buffer (`pitch` in pixels); pixels outside the
    /// faces are left alone.
    pub fn render(&mut self, dst: &mut [u16], pitch: usize) {
        self.build_lists();
        self.rasterise(dst, pitch, None);
    }

    /// [`DialogHead::render`] that also sets `mask` (one byte per pixel of
    /// `dst`) to 1 where it draws: the HD head is laid over another
    /// picture.
    pub fn render_masked(&mut self, dst: &mut [u16], pitch: usize, mask: &mut [u8]) {
        self.build_lists();
        self.rasterise(dst, pitch, Some(mask));
    }
}
