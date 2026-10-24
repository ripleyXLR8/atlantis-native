//! The `Atl3D` models of the 3D puzzles (`datas_a2\3d\pz_araig\*.db`):
//! a `.db` scene loaded into a model slot by `Atl3D_LoadDatabase`
//! (@ 0x0042a920) and drawn over the panorama by `Atl3D_draw_DataBase`
//! (@ 0x0042a320) with the warp's camera (@ 0x00660f48), the Pentium II
//! routines (CPU type 8, the reference: decision 0004) and the Atl3D
//! Z buffer (@ 0x006596a0, cleared to 0 by the puzzle's draw).
//!
//! A model is a node of its own (the slot, a child of the scene root:
//! placed and turned by the puzzle, scaled at load) and the scene's
//! objects under it, depth first, children in name order (the list
//! @ +0x58), each with the track of its name (the list @ +0x68, the same
//! order) and maybe a mesh. Per frame:
//!
//! 1. every object's track at the model's time (+0x84, @ 0x004c7810 with
//!    flags 0x1f: [`head::pose_nodes`]);
//! 2. per object with a mesh, the pipeline @ 0x004d2770 with the flags
//!    0x6300000 (camera mode 1, no light; the meshes' flags 0x122 lack
//!    0x10): the object's world values by the x87 assembly
//!    ([`head::update_asm_node`]), the object → camera matrix and the
//!    oriented box test (@ 0x0057f280); a box beyond a plane rejects the
//!    object; a box inside every plane takes the "inside" path: the
//!    back faces by the face planes (@ 0x00571f30 → @ 0x0057fa40), then
//!    the projection of the used vertices with the focal lengths folded
//!    into the matrix (@ 0x0057eac0); otherwise the camera-space
//!    vertices and their clip codes (@ 0x0057eec0, the heads' transform),
//!    the back faces by the face planes (@ 0x00571de0 → @ 0x0057f840 when
//!    a vertex is outside a plane, @ 0x0057fa40 when none is) and the
//!    projection (@ 0x0057fbc0);
//! 3. the faces, material group by material group (@ 0x004380b0 /
//!    @ 0x00432150), through `Zi32Texture16FastPerspective1516II` or, for
//!    the objects the puzzle draws "opaque" (@ 0x0065969c), its `Opacity`
//!    twin with the key colour 0 ([`crate::raster3d::ii`]).
//!
//! Every value follows the original's x87 code operation by operation
//! under the control word in force ([`Fpu`]), as the heads do
//! ([`crate::head`]). No face of the game's frames crosses the near or
//! the far plane (the closest object, a skull on the player's own node,
//! lies about 64 units away; the near plane is at 1.5): the clipping of
//! such faces (@ 0x004380b0) is not ported, such a face is skipped and
//! logged (docs/TECHNICAL.md).

use atlantis2_data::db::Scene;
use atlantis2_data::tga::Tga;

use crate::camera::Camera as WarpCamera;
use crate::fpu::Fpu;
use crate::head::{self, matq, q64, qmat, v64, Node, Track, Vec3, USED};
use crate::raster3d::{ii, Target, Texture, Vertex};

/// The warp camera's near and far planes (@ 0x00459a10: 1.5, 100000).
pub const NEAR: f32 = 1.5;
pub const FAR: f32 = 100_000.0;

/// What the pipeline reads of the warp camera (@ 0x00660f48): the
/// projection, the frustum and the depth map (@ 0x004a49c0 →
/// @ 0x004a4a30, mode 1), and its node's orientation (a child of the
/// root at the origin, scale 1).
#[derive(Clone, Debug)]
pub struct View {
    pub q: [f32; 4],
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
    pub near: f64,
    pub far: f64,
    /// Left, top, right, bottom (x/z, y/z) (+0x20..+0x2c).
    pub slopes: [f64; 4],
    /// The side planes (+0x30..+0x5c): left (x, 0, z), right, top
    /// (0, y, z), bottom, unit normals pointing out.
    pub planes: [[f64; 3]; 4],
    /// Depth = a − b·w (+0x60, +0x64).
    pub a: f64,
    pub b: f64,
}

impl View {
    /// The warp camera as the view entry's `FUN_004a4950` leaves it for a
    /// `width` x `height` viewport (0, height − 1, width − 1, 0), its
    /// derived values computed under the camera's control word.
    pub fn of(cam: &WarpCamera, width: usize, height: usize) -> View {
        View::with(
            cam.fpu,
            cam.q,
            [cam.fx, cam.fy, cam.cx, cam.cy].map(f64::from),
            width,
            height,
        )
    }

    /// The view drawn again at the HD size (not the original's): the
    /// focal lengths and the centre times s = tile / 16, the centre moved
    /// right by the HD picture's `side` columns, the frustum of that
    /// `width` x `height` frame; at tile 16 without sides it is the view
    /// itself.
    pub fn scaled(&self, k: Fpu, tile: usize, side: usize, width: usize, height: usize) -> View {
        let s = tile as f64 / 16.0;
        let proj = [
            k.f(k.mul(self.fx, s)),
            k.f(k.mul(self.fy, s)),
            k.add(k.mul(self.cx, s), side as f64),
            k.mul(self.cy, s),
        ];
        View::with(k, self.q, proj, width, height)
    }

    fn with(k: Fpu, q: [f32; 4], proj: [f64; 4], width: usize, height: usize) -> View {
        let [fx, fy, cx, cy] = proj;
        let (x1, y1) = ((width - 1) as f64, (height - 1) as f64);
        // @ 0x004a4a7c: ((float)(edge) − centre ∓ 1) / f.
        let s_l = k.f(k.div(k.sub(k.sub(0.0, cx), 1.0), fx));
        let s_r = k.f(k.div(k.add(k.sub(x1, cx), 1.0), fx));
        let s_t = k.f(k.div(k.sub(k.sub(0.0, cy), 1.0), fy));
        let s_b = k.f(k.div(k.add(k.sub(y1, cy), 1.0), fy));
        // Each plane normalised: FUN_00492bf0 (x² + y²) + z², the float
        // 1 / √ of it, FUN_0049a720 the vector scaled.
        let unit = |v: [f64; 3]| {
            let n2 = k.add(
                k.add(k.mul(v[0], v[0]), k.mul(v[1], v[1])),
                k.mul(v[2], v[2]),
            );
            let inv = k.f(k.div(1.0, k.sqrt(n2)));
            v.map(|c| k.f(k.mul(inv, c)))
        };
        let near = f64::from(NEAR);
        let far = f64::from(FAR);
        let a = k.div(far, k.sub(far, near));
        View {
            q,
            fx,
            fy,
            cx,
            cy,
            near,
            far,
            slopes: [s_l, s_t, s_r, s_b],
            planes: [
                unit([-1.0, 0.0, s_l]),
                unit([1.0, 0.0, -s_r]),
                unit([0.0, -1.0, s_t]),
                unit([0.0, 1.0, -s_b]),
            ],
            a: k.f(a),
            b: k.f(k.mul(a, near)),
        }
    }
}

/// A mesh as the loader builds it (`FUN_004c8fc0`).
#[derive(Clone, Debug)]
struct Mesh {
    vertices: Vec<Vec3>,
    corners: Vec<[u16; 3]>,
    /// Texture coordinates of set 1, three per face.
    uvs: Vec<[[f64; 2]; 3]>,
    /// Faces per material group (+0x124: one count per material of the
    /// mesh, at least up to the highest a face uses) and each group's
    /// material (the scene's material of that name).
    groups: Vec<(usize, Option<usize>)>,
    /// One plane per face (+0x144: unit normal, d), the scene's.
    planes: Vec<[f64; 4]>,
    /// The box (+0x94): its three half axes, its centre.
    axes: [Vec3; 3],
    centre: Vec3,
    /// The mesh's materials (+0x234), as the scene's material indices.
    mats: Vec<Option<usize>>,
}

/// The mesh flags the loader gives every mesh of the puzzles' scenes
/// (+0x50: 0x122, without 0x10, the light).
pub const MESH_FLAGS: u32 = 0x122;

/// The plane the loader appends after the faces' ones (+0x144's last
/// entry: x = NaN 0xffffffff, y 0, z 1, d 0).
pub const SENTINEL_PLANE: [u32; 4] = [0xffff_ffff, 0, 0x3f80_0000, 0];

/// A loaded object as the checks print it.
#[derive(Clone, Debug)]
pub struct ObjectInfo {
    pub name: String,
    /// The parent object's index in the list; `None`: the model's node.
    pub parent: Option<usize>,
    /// The local rotation (x, y, z, w), position and scale, as floats.
    pub local: [f32; 8],
    pub mesh: Option<MeshInfo>,
}

/// A map as the checks print it: width, height, RGB565 texels.
pub type MapInfo = (usize, usize, Vec<u16>);

/// A loaded mesh as the checks print it.
#[derive(Clone, Debug)]
pub struct MeshInfo {
    /// Per material: its name and its map.
    pub mats: Vec<(Option<String>, Option<MapInfo>)>,
    pub vertices: usize,
    pub faces: usize,
    pub groups: Vec<usize>,
    pub corners: Vec<[u16; 3]>,
    /// The face planes (unit normal, d) as floats.
    pub planes: Vec<[f32; 4]>,
    /// The box: three axes (x, y, z, 0) and the centre, as floats.
    pub bbox: [f32; 15],
}

/// An object of the model: its node and its mesh.
#[derive(Clone, Debug)]
struct Object {
    node: usize,
    mesh: Option<usize>,
}

/// What a frame of the pipeline did to one object, for the checks
/// against the original.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectStage {
    /// The box test's planes (+0x144 after the transform: the OR of the
    /// codes, 0 inside), the AND of the codes (+0x148), `None` when the
    /// box test rejected it.
    pub or: u8,
    pub and: u8,
    pub rejected: bool,
    /// The eye in object space (pipeline + 0x64).
    pub eye: [f32; 3],
    /// Projected vertices (pipeline + 0x14: sx, sy, depth, w) of the used
    /// vertices, 0 elsewhere; vertex codes (+0x24); face bytes (+0x34).
    pub projected: Vec<[f32; 4]>,
    pub codes: Vec<u8>,
    pub faces: Vec<u8>,
    /// Faces skipped because they cross the near or far plane.
    pub clipped: usize,
}

/// A model slot (0xa0 bytes in the original).
#[derive(Clone, Debug)]
pub struct Model {
    /// The model's own node (FUN_004a2860 / FUN_004a2000 / FUN_004a2f40,
    /// relative to the root: copies).
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    pub scale: f32,
    /// The animation time (+0x84) and length (+0x90: the first animated
    /// node's last frame, in name order; 0 without one).
    pub time: f32,
    pub length: f32,
    /// [0] the model's node, then the objects' nodes.
    nodes: Vec<Node>,
    tracks: Vec<Option<Track>>,
    objects: Vec<Object>,
    meshes: Vec<Mesh>,
    textures: Vec<Option<Tga>>,
    material_names: Vec<String>,
}

/// Objects the loader creates: every object (unlike the heads, the
/// puzzles' scenes have no mouth helpers).
fn objects_depth_first(scene: &Scene) -> Vec<(usize, Option<usize>)> {
    let mut objs: Vec<usize> = (0..scene.objects.len()).collect();
    objs.sort_by(|&a, &b| {
        scene.objects[a]
            .name
            .as_bytes()
            .cmp(scene.objects[b].name.as_bytes())
    });
    let mut out = Vec::new();
    let mut stack: Vec<(Option<usize>, &str)> = vec![(None, "")];
    while let Some((parent, pname)) = stack.pop() {
        if out.len() > objs.len() {
            break;
        }
        let children: Vec<usize> = objs
            .iter()
            .copied()
            .filter(|&o| scene.objects[o].parent == pname)
            .collect();
        for &c in children.iter().rev() {
            stack.push((Some(c), scene.objects[c].name.as_str()));
        }
        if let Some(p) = parent {
            let parent_obj = scene
                .objects
                .iter()
                .position(|o| o.name == scene.objects[p].parent);
            out.push((p, parent_obj));
        }
    }
    out
}

impl Model {
    /// `Atl3D_LoadDatabase` (@ 0x0042a920) of `scene`, its maps read by
    /// `texture` (the material's map name), under the control word `k`;
    /// the scale given to the model's node (FUN_004a2f40, @ 0x0048f5f4).
    pub fn load(
        k: Fpu,
        scene: &Scene,
        scale: f32,
        mut texture: impl FnMut(&str) -> Option<Tga>,
    ) -> Model {
        let order = objects_depth_first(scene);
        let mut nodes = vec![Node {
            name: String::new(),
            parent: None,
            lq: [0.0, 0.0, 0.0, 1.0],
            lp: [0.0; 3],
            ls: f64::from(scale),
            wq: [0.0; 4],
            wp: [0.0; 3],
            ws: 1.0,
            valid: false,
        }];
        let index_of: Vec<usize> = (0..scene.objects.len())
            .map(|o| order.iter().position(|&(p, _)| p == o).map_or(0, |i| i + 1))
            .collect();
        for &(o, parent) in &order {
            let ob = &scene.objects[o];
            // The local rotation through a matrix and back, as the
            // heads' loader (@ 0x004c6e40, @ 0x004c89f0).
            let lq = matq(k, &qmat(k, &q64(ob.rotation)));
            nodes.push(Node {
                name: ob.name.clone(),
                parent: Some(parent.map_or(0, |p| index_of[p])),
                lq,
                lp: v64(ob.position),
                ls: f64::from(ob.scale),
                wq: [0.0; 4],
                wp: [0.0; 3],
                ws: 1.0,
                valid: false,
            });
        }
        let tracks = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if i == 0 {
                    None
                } else {
                    scene.node(&n.name).and_then(head::track)
                }
            })
            .collect();
        let mut meshes = Vec::new();
        let mut objects = Vec::new();
        for (i, &(o, _)) in order.iter().enumerate() {
            let mesh = scene.mesh(&scene.objects[o].mesh).map(|m| {
                meshes.push(Mesh::build(k, scene, m));
                meshes.len() - 1
            });
            objects.push(Object { node: i + 1, mesh });
        }
        let textures = scene
            .materials
            .iter()
            .map(|m| m.texture().and_then(&mut texture))
            .collect();
        // +0x90: the first node of the map (name order) with its last
        // frame (@ 0x0042ac9b).
        let mut names: Vec<&atlantis2_data::db::Node> = scene.nodes.iter().collect();
        names.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        let length = names.first().map_or(0.0, |n| n.last_frame as f32);
        Model {
            rotation: [0.0, 0.0, 0.0, 1.0],
            position: [0.0; 3],
            scale,
            time: 0.0,
            length,
            nodes,
            tracks,
            objects,
            meshes,
            textures,
            material_names: scene.materials.iter().map(|m| m.name.clone()).collect(),
        }
    }

    /// The objects as loaded, in the list's order.
    pub fn objects_info(&self) -> Vec<ObjectInfo> {
        self.objects
            .iter()
            .map(|o| {
                let n = &self.nodes[o.node];
                let parent = n.parent.filter(|&p| p != 0).map(|p| p - 1);
                let local = [
                    n.lq[0], n.lq[1], n.lq[2], n.lq[3], n.lp[0], n.lp[1], n.lp[2], n.ls,
                ]
                .map(|v| v as f32);
                let mesh = o.mesh.map(|mi| {
                    let m = &self.meshes[mi];
                    let mut bbox = [0.0f32; 15];
                    for (i, a) in m.axes.iter().enumerate() {
                        for c in 0..3 {
                            bbox[4 * i + c] = a[c] as f32;
                        }
                    }
                    for c in 0..3 {
                        bbox[12 + c] = m.centre[c] as f32;
                    }
                    MeshInfo {
                        mats: m
                            .mats
                            .iter()
                            .map(|&mat| {
                                let tex = mat.and_then(|i| self.textures[i].as_ref());
                                (
                                    mat.map(|i| self.material_names[i].clone()),
                                    tex.map(|t| (t.width, t.height, t.pixels.clone())),
                                )
                            })
                            .collect(),
                        vertices: m.vertices.len(),
                        faces: m.corners.len(),
                        groups: m.groups.iter().map(|g| g.0).collect(),
                        corners: m.corners.clone(),
                        planes: m.planes.iter().map(|p| p.map(|v| v as f32)).collect(),
                        bbox,
                    }
                });
                ObjectInfo {
                    name: n.name.clone(),
                    parent,
                    local,
                    mesh,
                }
            })
            .collect()
    }

    /// Draws the model (`Atl3D_draw_DataBase` @ 0x0042a320) with the
    /// camera `view` into `out`, through the `Opacity` routine when
    /// `opacity` (@ 0x0065969c). `k` is the control word the puzzle's code
    /// runs under; the II set-up loads 0x0e7f and leaves it.
    pub fn draw(
        &mut self,
        k: Fpu,
        view: &View,
        opacity: bool,
        out: &mut Target,
    ) -> Vec<ObjectStage> {
        // The model's node, then every object's track at the time.
        {
            let n = &mut self.nodes[0];
            n.lq = q64(self.rotation);
            n.lp = v64(self.position);
            n.ls = f64::from(self.scale);
        }
        head::pose_nodes(k, &mut self.nodes, &self.tracks, f64::from(self.time));
        let mut stages = Vec::new();
        for oi in 0..self.objects.len() {
            let Some(mi) = self.objects[oi].mesh else {
                continue;
            };
            let node = self.objects[oi].node;
            head::update_asm_node(k, &mut self.nodes, node);
            let st = self.draw_object(k, view, node, mi, opacity, out);
            stages.push(st);
        }
        stages
    }

    fn draw_object(
        &self,
        k: Fpu,
        view: &View,
        node: usize,
        mi: usize,
        opacity: bool,
        out: &mut Target,
    ) -> ObjectStage {
        let m = |x, y| k.mul(x, y);
        let mesh = &self.meshes[mi];
        let o = &self.nodes[node];
        let nv = mesh.vertices.len();
        let nf = mesh.corners.len();
        let mut st = ObjectStage {
            projected: vec![[0.0; 4]; nv],
            codes: vec![0; nv],
            faces: vec![0x40; nf],
            ..ObjectStage::default()
        };
        // The object → camera matrix (@ 0x0057f2b3, the heads'
        // @ 0x0057eef0): the camera node is the root's child at the origin.
        let [cx, cy, cz, cw] = q64(view.q);
        let (cpx, cpy, cpz, cs) = (0.0, 0.0, 0.0, 1.0);
        let is = k.div(1.0, cs);
        let dx = k.sub(o.wp[0], cpx);
        let dy = k.sub(o.wp[1], cpy);
        let dz = k.sub(o.wp[2], cpz);
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
        let scale = k.f(m(is, o.ws));
        let [ox, oy, oz, ow] = o.wq;
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
        // The box test (@ 0x0057f280): the box in camera space.
        let row = |r: usize, v: &Vec3| {
            if r < 2 {
                k.add(
                    m(mat[3 * r + 2], v[2]),
                    k.add(m(mat[3 * r + 1], v[1]), m(mat[3 * r], v[0])),
                )
            } else {
                k.add(k.add(m(v[1], mat[7]), m(v[0], mat[6])), m(v[2], mat[8]))
            }
        };
        let tc = [
            k.f(k.add(row(0, &mesh.centre), t[0])),
            k.f(k.add(row(1, &mesh.centre), t[1])),
            k.f(k.add(row(2, &mesh.centre), t[2])),
        ];
        let ax: [[f64; 3]; 3] =
            std::array::from_fn(|i| std::array::from_fn(|r| k.f(row(r, &mesh.axes[i]))));
        let Some(mask) = box_test(k, view, &tc, &ax) else {
            st.rejected = true;
            st.and = 0x3f;
            return st;
        };
        let eye = eye_in_object(k, o);
        st.eye = eye.map(|v| v as f32);
        let inside = mask == 0;
        if inside {
            // @ 0x00571f30 (codes cleared, @ 0x0057fa40), then @ 0x0057eac0.
            back_faces(k, mesh, &eye, false, &mut st.codes, &mut st.faces);
            let fxm: [f64; 3] = std::array::from_fn(|i| k.f(m(mat[i], view.fx)));
            let fym: [f64; 3] = std::array::from_fn(|i| k.f(m(mat[3 + i], view.fy)));
            let t0 = k.f(m(t[0], view.fx));
            let t1 = k.f(m(t[1], view.fy));
            for (i, v) in mesh.vertices.iter().enumerate() {
                if st.codes[i] & USED == 0 {
                    continue;
                }
                let [vx, vy, vz] = *v;
                let zz = k.add(
                    m(vz, mat[8]),
                    k.add(k.add(m(mat[6], vx), m(mat[7], vy)), t[2]),
                );
                let w = k.div(1.0, zz);
                let sx = k.f(k.add(
                    m(
                        k.add(
                            m(vz, fxm[2]),
                            k.add(k.add(m(fxm[0], vx), m(fxm[1], vy)), t0),
                        ),
                        w,
                    ),
                    view.cx,
                ));
                let sy = k.f(k.add(
                    m(
                        k.add(
                            m(vy, fym[1]),
                            k.add(k.add(m(vz, fym[2]), m(vx, fym[0])), t1),
                        ),
                        w,
                    ),
                    view.cy,
                ));
                let depth = k.f(k.sub(view.a, m(w, view.b)));
                st.projected[i] = [sx as f32, sy as f32, depth as f32, k.f32(w)];
            }
        } else {
            // @ 0x0057eec0: the camera-space vertices and their codes.
            let mut cv = vec![[0.0f64; 3]; nv];
            let (mut and, mut or) = (0xffu8, 0u8);
            let above = |a: f64, b: f64| a > b;
            let below = |a: f64, b: f64| a < b || a.is_nan() || b.is_nan();
            for (i, v) in mesh.vertices.iter().enumerate() {
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
                if above(view.near, pz) {
                    code |= 0x10;
                }
                if below(view.far, pz) {
                    code |= 0x20;
                }
                if above(m(pz, view.slopes[0]), px) {
                    code |= 1;
                }
                if below(m(pz, view.slopes[2]), px) {
                    code |= 4;
                }
                if above(m(pz, view.slopes[1]), py) {
                    code |= 2;
                }
                if below(m(pz, view.slopes[3]), py) {
                    code |= 8;
                }
                and &= code;
                or |= code;
                st.codes[i] = code;
                cv[i] = [k.f(px), k.f(py), k.f(pz)];
            }
            st.or = or;
            st.and = and;
            if and & 0x3f != 0 {
                st.rejected = true;
                return st;
            }
            back_faces(k, mesh, &eye, or != 0, &mut st.codes, &mut st.faces);
            // @ 0x0057fbc0: the used vertices not before the near plane.
            for (i, p) in cv.iter().enumerate() {
                if st.codes[i] & 0x90 != USED {
                    continue;
                }
                let w = k.div(1.0, p[2]);
                let sx = k.f(k.add(m(m(view.fx, p[0]), w), view.cx));
                let sy = k.f(k.add(m(m(view.fy, p[1]), w), view.cy));
                let depth = k.f(k.sub(view.a, m(w, view.b)));
                st.projected[i] = [sx as f32, sy as f32, depth as f32, k.f32(w)];
            }
        }
        // The faces, group by group (@ 0x004380b0 / @ 0x00432150).
        let key = opacity.then_some(0u16);
        let mut face = 0usize;
        for &(count, mat) in &mesh.groups {
            let tex = mat
                .and_then(|i| self.textures.get(i))
                .and_then(|t| t.as_ref());
            for fi in face..(face + count).min(nf) {
                let byte = st.faces[fi];
                if byte & 0x40 != 0 {
                    continue;
                }
                if byte & 0x30 != 0 {
                    st.clipped += 1;
                    log::warn!("Atl3D face crossing the near or far plane: not drawn");
                    continue;
                }
                let Some(tex) = tex else {
                    log::warn!("Atl3D material without its map: face not drawn");
                    continue;
                };
                let tri = mesh.corners[fi];
                let mut poly: [Vertex; 3] = std::array::from_fn(|c| {
                    let p = st.projected[tri[c] as usize];
                    let uv = mesh.uvs[fi][c];
                    Vertex {
                        x: f64::from(p[0]),
                        y: f64::from(p[1]),
                        w: f64::from(p[3]),
                        u: uv[0],
                        v: uv[1],
                    }
                });
                let texture = Texture {
                    pixels: &tex.pixels,
                    width: tex.width,
                    height: tex.height,
                };
                ii::draw_polygon_keyed(&mut poly, &texture, key, out);
            }
            face += count;
        }
        st
    }
}

impl Mesh {
    /// The runtime mesh of a `.db` mesh (`FUN_004c8fc0`), under the control
    /// word `k`: the faces in file order (every file's faces already
    /// sorted by material), one group per material index up to the
    /// highest used, the face planes and the box.
    fn build(k: Fpu, scene: &Scene, m: &atlantis2_data::db::Mesh) -> Mesh {
        let vertices: Vec<Vec3> = m.vertices.iter().map(|v| v64(*v)).collect();
        let nf = m.face_count as usize;
        let corners: Vec<[u16; 3]> = (0..nf)
            .map(|i| std::array::from_fn(|c| m.corners.get(3 * i + c).copied().unwrap_or(0)))
            .collect();
        let uvs = (0..nf)
            .map(|i| {
                std::array::from_fn(|c| {
                    m.uvs[0]
                        .get(3 * i + c)
                        .map_or([0.0; 2], |uv| [f64::from(uv[0]), f64::from(uv[1])])
                })
            })
            .collect();
        // One group per material of the mesh (+0x124; a material no face
        // uses has an empty group), and up to the highest a face uses.
        let top = m
            .face_materials
            .iter()
            .copied()
            .max()
            .map_or(0, usize::from);
        let groups = (0..(top + 1).max(m.materials.len()))
            .map(|g| {
                let count = m
                    .face_materials
                    .iter()
                    .filter(|&&f| usize::from(f) == g)
                    .count();
                let mat = m
                    .materials
                    .get(g)
                    .and_then(|name| scene.materials.iter().position(|x| &x.name == name));
                (count, mat)
            })
            .collect();
        // The face planes (+0x144, @ 0x004c91f0 copies the scene's): the
        // file's own from version 6; before, the file's face normals with
        // d = n · v0, (n.z v0.z + n.y v0.y) + n.x v0.x as a float
        // (@ 0x004a7395..0x004a740b, FUN_004a65f0).
        let fv = |i: u16| vertices.get(usize::from(i)).copied().unwrap_or([0.0; 3]);
        let planes = (0..nf)
            .map(|i| {
                if let Some(p) = m.face_planes.get(i) {
                    return p.map(f64::from);
                }
                let n = m
                    .face_normals
                    .get(i)
                    .copied()
                    .unwrap_or([0.0, 0.0, 1.0])
                    .map(f64::from);
                let v0 = fv(corners[i][0]);
                let dd = k.f(k.add(
                    k.add(k.mul(n[2], v0[2]), k.mul(n[1], v0[1])),
                    k.mul(n[0], v0[0]),
                ));
                [n[0], n[1], n[2], dd]
            })
            .collect();
        // The box: the file's centre, then three (axis, half extent); the
        // runtime keeps each axis times its half extent.
        let bb = m.bbox.map(f64::from);
        let axes = std::array::from_fn(|i| {
            let o = 3 + 4 * i;
            std::array::from_fn(|c| k.f(k.mul(bb[o + c], bb[o + 3])))
        });
        let mats = m
            .materials
            .iter()
            .map(|name| scene.materials.iter().position(|x| &x.name == name))
            .collect();
        Mesh {
            vertices,
            corners,
            uvs,
            groups,
            planes,
            axes,
            centre: [bb[0], bb[1], bb[2]],
            mats,
        }
    }
}

/// The eye in object space (@ 0x00571f30 / @ 0x00571de0, pipeline +
/// 0x64): the camera's position minus the object's as floats
/// (FUN_0047df90), turned by the conjugate of the object's rotation
/// (FUN_0047e210), times 1 / the object's scale.
fn eye_in_object(k: Fpu, o: &Node) -> Vec3 {
    let m = |x, y| k.mul(x, y);
    let v: Vec3 = std::array::from_fn(|i| k.f(k.sub(0.0, o.wp[i])));
    let [qx, qy, qz, qw] = o.wq;
    let [vx, vy, vz] = v;
    let a = k.sub(k.add(m(vx, qw), m(vy, qz)), m(vz, qy));
    let b = k.sub(k.add(m(vz, qx), m(vy, qw)), m(vx, qz));
    let c = k.sub(k.add(m(vx, qy), m(vz, qw)), m(vy, qx));
    let d = k.sub(k.sub(-m(vx, qx), m(vy, qy)), m(vz, qz));
    let r = [
        k.f(k.add(k.sub(k.sub(m(a, qw), m(d, qx)), m(c, qy)), m(b, qz))),
        k.f(k.add(k.sub(k.sub(m(b, qw), m(d, qy)), m(a, qz)), m(c, qx))),
        k.f(k.add(k.sub(k.sub(m(c, qw), m(d, qz)), m(b, qx)), m(a, qy))),
    ];
    let inv = k.div(1.0, o.ws);
    r.map(|x| k.f(m(inv, x)))
}

/// The back faces by the face planes: @ 0x0057fa40 (`clipped` false: a
/// front face's byte 0, its vertices' codes set to 0x80) or @ 0x0057f840
/// (a front face whose three vertices share a plane they are beyond is
/// dropped; else its vertices' codes get 0x80 and its byte is their OR).
/// A face is in front when (n.x e.x + n.y e.y) + n.z e.z is not below d
/// (unordered: behind).
fn back_faces(k: Fpu, mesh: &Mesh, eye: &Vec3, clipped: bool, codes: &mut [u8], faces: &mut [u8]) {
    for (fi, (tri, p)) in mesh.corners.iter().zip(&mesh.planes).enumerate() {
        faces[fi] = 0x40;
        let dot = k.add(
            k.add(k.mul(p[0], eye[0]), k.mul(p[1], eye[1])),
            k.mul(p[2], eye[2]),
        );
        if dot < p[3] || dot.is_nan() || p[3].is_nan() {
            continue;
        }
        let [i0, i1, i2] = tri.map(usize::from);
        if i0 >= codes.len() || i1 >= codes.len() || i2 >= codes.len() {
            continue;
        }
        if !clipped {
            faces[fi] = 0;
            codes[i1] = USED;
            codes[i0] = USED;
            codes[i2] = USED;
        } else {
            if codes[i0] & codes[i1] & codes[i2] & 0x3f != 0 {
                continue;
            }
            codes[i2] |= USED;
            codes[i0] |= USED;
            codes[i1] |= USED;
            faces[fi] = codes[i0] | codes[i1] | codes[i2];
        }
    }
}

/// The oriented box against the frustum (@ 0x0057f658..0x0057f809): `None`
/// beyond a plane, else the planes the box crosses (0x10 near, 0x20 far,
/// 1 left, 4 right, 2 top, 8 bottom).
fn box_test(k: Fpu, view: &View, c: &Vec3, a: &[[f64; 3]; 3]) -> Option<u8> {
    let m = |x, y| k.mul(x, y);
    let mut bits = 0u8;
    // `fcomi` then `ja` (beyond) / `setb` (crossing): unordered never
    // beyond, always crossing.
    let beyond = |d: f64, r: f64| d > r;
    let crossing = |x: f64, y: f64| x < y || x.is_nan() || y.is_nan();
    let r = k.add(k.add(a[0][2].abs(), a[1][2].abs()), a[2][2].abs());
    let dn = k.sub(view.near, c[2]);
    if beyond(dn, r) {
        return None;
    }
    if crossing(-r, dn) {
        bits |= 0x10;
    }
    let df = k.sub(c[2], view.far);
    if beyond(df, r) {
        return None;
    }
    if crossing(-r, df) {
        bits |= 0x20;
    }
    // The side planes, (x, z) for left and right, (y, z) for top and
    // bottom.
    for (pi, bit, axis) in [(0usize, 1u8, 0usize), (1, 4, 0), (2, 2, 1), (3, 8, 1)] {
        let p = &view.planes[pi];
        let (nu, nz) = (p[axis], p[2]);
        let d = k.add(m(nz, c[2]), m(c[axis], nu));
        let e0 = k.add(m(a[0][axis], nu), m(a[0][2], nz)).abs();
        let e1 = k.add(m(a[1][axis], nu), m(a[1][2], nz)).abs();
        let e2 = k.add(m(nz, a[2][2]), m(nu, a[2][axis])).abs();
        let r = k.add(k.add(e1, e0), e2);
        if beyond(d, r) {
            return None;
        }
        if crossing(d.abs(), r) {
            bits |= bit;
        }
    }
    Some(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warp_view_of_the_reference_camera() {
        // The camera after the warp's init (@ 0x00660f48), read under
        // emulation.
        let cam = WarpCamera::new(Fpu::Chop, 640, 480);
        let v = View::of(&cam, 640, 480);
        let bits = |x: f64| (x as f32).to_bits();
        assert_eq!(bits(v.fx), 409.581_3_f32.to_bits());
        assert_eq!(
            v.slopes.map(bits),
            [0xbf48_a258, 0xbf16_a1c2, 0x3f48_0256, 0x3f16_01c0]
        );
        assert_eq!(bits(v.planes[0][0]), (-0.787_077_55_f32).to_bits());
        assert_eq!(bits(v.a), 1.000_014_9_f32.to_bits());
        assert_eq!(bits(v.b), 1.500_022_4_f32.to_bits());
    }
}
