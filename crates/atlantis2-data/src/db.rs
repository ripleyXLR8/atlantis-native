//! `.db` scenes: the dialog heads (`datas_a2\3d\dial\<Mesh>s<stand>.db`)
//! and the 3D puzzles' objects, written by Cryo's 3ds Max exporter as a
//! C++ stream of versioned records and read by @ 0x004a7650.
//!
//! A file holds meshes, scene objects (a node placed in a hierarchy, with
//! the name of its mesh), materials, animated nodes (key-frame tracks and,
//! for the node called `morphing`, the mouth shapes as morph targets) and
//! cameras. Each list is stored as (key, record) pairs; the original keeps
//! them in `std::map`s keyed by name.
//!
//! Every value is little-endian. A string is a u32 length counting its
//! terminating NUL, then the bytes (@ 0x004db810); a vector is a u32 count
//! then the elements (@ 0x004dbe80 and its siblings).

use std::fmt;

/// A parsed `.db` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// The file version (2, 7, 10 or 11 on the discs); above 11 the
    /// original throws `Load, invalid .db file`.
    pub version: u32,
    /// Exporter tag → export date.
    pub info: Vec<(String, String)>,
    pub meshes: Vec<Mesh>,
    pub objects: Vec<Object>,
    pub materials: Vec<Material>,
    pub nodes: Vec<Node>,
    pub cameras: Vec<Camera>,
    /// The last section's records (@ 0x004a7590), none in Atlantis II's
    /// files; Atlantis III's scenes hold their lights there.
    pub lights: Vec<Light>,
}

/// Whose reader reads a `.db` file: the records of the last section are 68
/// bytes after their strings in Atlantis II (@ 0x004a7590), 64 in Atlantis
/// III (its `Game.exe` @ 0x0050cdc0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Atlantis2,
    Atlantis3,
}

/// A record of the last section: a key, two strings, a vec4, a vec3 and
/// 10 dwords (Atlantis II) or 9 (Atlantis III).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Light {
    pub key: String,
    pub name: String,
    pub target: String,
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    pub words: Vec<u32>,
}

/// A triangle mesh (@ 0x004a6d60).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub name: String,
    pub parent: String,
    /// Bounding box: centre, then three (axis, half-extent).
    pub bbox: [f32; 15],
    pub vertex_count: u32,
    pub face_count: u32,
    pub corner_count: u32,
    /// The materials the mesh uses, by name.
    pub materials: Vec<String>,
    pub vertices: Vec<[f32; 3]>,
    /// One byte per face (0 in the heads).
    pub face_materials: Vec<u8>,
    /// Three vertex indices per face.
    pub corners: Vec<u16>,
    /// Per-corner normals (the version-2 files).
    pub corner_normals: Vec<[f32; 3]>,
    /// Three texture-coordinate sets, one (u, v) per corner when present;
    /// the heads fill only the first.
    pub uvs: [Vec<[f32; 2]>; 3],
    /// Faces per group (one group of every face in the heads).
    pub groups: Vec<u16>,
    /// One value per corner (record +0x16c), meaning unknown. It equals
    /// `corners` in 22 of the 28 heads and differs in the other 6, where
    /// some values exceed the vertex count; the heads' draw uses `corners`
    /// (the original's run-time corner list equals `corners` in `h2s1.DB`).
    pub corner_vertices: Vec<u16>,
    pub vertex_normals: Vec<[f32; 3]>,
    /// Per-face planes (unit normal, d), version 6 and later.
    pub face_planes: Vec<[f32; 4]>,
    /// Per-face unit normals of the files before version 6, from which
    /// the loader makes the face planes (@ 0x004a7395..0x004a740b: d = the
    /// normal · the face's first vertex).
    pub face_normals: Vec<[f32; 3]>,
    /// Per-corner planes, version 6 and later.
    pub corner_planes: Vec<[f32; 4]>,
}

/// A scene object (@ 0x004a7460): a node of the hierarchy and its mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Object {
    pub name: String,
    /// The mesh's name; the mouth-shape helpers name meshes that have no
    /// record (their shapes are morph keys).
    pub mesh: String,
    /// The parent object's name (`Bip01 Head` → `Bip01 Neck`), empty at
    /// the top.
    pub parent: String,
    /// Local rotation, a quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    pub scale: f32,
}

/// A material (@ 0x004a6c70).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    pub name: String,
    /// Six map slots; slot 1 (record +0x20) holds the diffuse texture.
    pub maps: [String; 6],
    /// Four colours 0xAARRGGBB (roles unknown; the heads are unlit).
    pub colours: [u32; 4],
    pub unknown: f32,
    /// Transparency: the run-time opacity is 1 − this.
    pub transparency: f32,
    pub flags: u32,
    /// Version ≥ 4, else 1.0 (role unknown).
    pub unknown2: f32,
}

impl Material {
    /// The diffuse texture's file name, if any.
    pub fn texture(&self) -> Option<&str> {
        Some(self.maps[1].as_str()).filter(|s| !s.is_empty())
    }
}

/// An animated node (@ 0x004a6bb0) and its tracks (one key per frame in
/// the heads; times are frame numbers).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub name: String,
    /// Record +0x10 (59, 40: the last frame, it seems; unused).
    pub last_frame: u32,
    /// TCB rotation keys: (time, quaternion); the TCB parameters, the
    /// tangents and the version < 9 extra value are read and dropped,
    /// as the track builder @ 0x004c7250 ignores them.
    pub rotation_keys: Vec<(u32, [f32; 4])>,
    /// TCB scale keys: (time, the scale vector's x).
    pub scale_keys: Vec<(u32, f32)>,
    /// TCB position keys: (time, position).
    pub position_keys: Vec<(u32, [f32; 3])>,
    /// Morph targets: (target name, its full vertex array).
    pub morph_keys: Vec<(String, Vec<[f32; 3]>)>,
    /// Note keys (version ≥ 4), empty in every file.
    pub note_keys: Vec<(u32, String)>,
    /// Linear keys (version ≥ 11), used when the node has no TCB key.
    pub linear_rotation_keys: Vec<(u32, [f32; 4])>,
    pub linear_scale_keys: Vec<(u32, [f32; 3])>,
    pub linear_position_keys: Vec<(u32, [f32; 3])>,
}

/// A camera (@ 0x004a7530): world pose, field of view, near and far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Camera {
    pub name: String,
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    /// Horizontal field of view, radians.
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

/// Why a `.db` file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbError {
    /// The file ends inside a record (the original's stream throws).
    Truncated,
    /// A version above 11 (`Load, invalid .db file`).
    Version(u32),
    /// A count no file of this size could hold.
    Count(u32),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Truncated => write!(f, "truncated .db file"),
            DbError::Version(v) => write!(f, "invalid .db file (version {v})"),
            DbError::Count(n) => write!(f, "invalid .db file (count {n})"),
        }
    }
}

impl std::error::Error for DbError {}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], DbError> {
        let end = self.at.checked_add(n).ok_or(DbError::Truncated)?;
        let s = self.b.get(self.at..end).ok_or(DbError::Truncated)?;
        self.at = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, DbError> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, DbError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], DbError> {
        let mut v = [0.0; N];
        for x in &mut v {
            *x = self.f32()?;
        }
        Ok(v)
    }
    /// A count of elements of `size` bytes each, checked against the rest
    /// of the file.
    fn count(&mut self, size: usize) -> Result<usize, DbError> {
        let n = self.u32()?;
        if (n as usize).saturating_mul(size.max(1)) > self.b.len() - self.at {
            return Err(DbError::Count(n));
        }
        Ok(n as usize)
    }
    /// A string (@ 0x004db810): the length counts the NUL.
    fn string(&mut self) -> Result<String, DbError> {
        let n = self.count(1)?;
        let s = self.bytes(n)?;
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        Ok(s[..end].iter().map(|&c| char::from(c)).collect())
    }
    fn vec<T>(
        &mut self,
        size: usize,
        mut f: impl FnMut(&mut Self) -> Result<T, DbError>,
    ) -> Result<Vec<T>, DbError> {
        let n = self.count(size)?;
        (0..n).map(|_| f(self)).collect()
    }
    fn vec_u8(&mut self) -> Result<Vec<u8>, DbError> {
        let n = self.count(1)?;
        Ok(self.bytes(n)?.to_vec())
    }
    fn vec_u16(&mut self) -> Result<Vec<u16>, DbError> {
        self.vec(2, |r| {
            Ok(u16::from_le_bytes(r.bytes(2)?.try_into().unwrap()))
        })
    }
    fn vec_u32(&mut self) -> Result<Vec<u32>, DbError> {
        self.vec(4, |r| r.u32())
    }
    fn vec_f<const N: usize>(&mut self) -> Result<Vec<[f32; N]>, DbError> {
        self.vec(4 * N, |r| r.floats::<N>())
    }
    fn strings(&mut self) -> Result<Vec<String>, DbError> {
        self.vec(4, |r| r.string())
    }
    /// The TCB parameters of a key (@ 0x004a6920): five dwords, unused.
    fn tcb(&mut self) -> Result<(), DbError> {
        self.bytes(20).map(|_| ())
    }
}

impl Scene {
    /// Reads a `.db` file (@ 0x004a7650).
    pub fn parse(data: &[u8]) -> Result<Scene, DbError> {
        Scene::parse_len(data).map(|(s, _)| s)
    }

    /// Reads a `.db` file and says how many bytes the reader consumed
    /// (the original ignores what follows).
    pub fn parse_len(data: &[u8]) -> Result<(Scene, usize), DbError> {
        Scene::parse_len_as(data, Dialect::Atlantis2)
    }

    /// [`Scene::parse_len`] with the reader of `dialect`.
    pub fn parse_len_as(data: &[u8], dialect: Dialect) -> Result<(Scene, usize), DbError> {
        let words = match dialect {
            Dialect::Atlantis2 => 10,
            Dialect::Atlantis3 => 9,
        };
        let mut r = Reader { b: data, at: 0 };
        let _marker = r.u32()?; // 0xff in every file; not tested
        let version = r.u32()?;
        if version > 11 {
            return Err(DbError::Version(version));
        }
        let mut s = Scene {
            version,
            ..Scene::default()
        };
        s.info = r.vec(8, |r| Ok((r.string()?, r.string()?)))?;
        s.meshes = r.vec(8, |r| {
            r.string()?;
            read_mesh(r)
        })?;
        s.objects = r.vec(8, |r| {
            r.string()?;
            read_object(r)
        })?;
        s.materials = r.vec(8, |r| {
            r.string()?;
            read_material(r)
        })?;
        s.nodes = r.vec(8, |r| {
            r.string()?;
            read_node(r)
        })?;
        if version >= 2 {
            s.cameras = r.vec(8, |r| {
                r.string()?;
                read_camera(r)
            })?;
            // @ 0x004a7590: str, str, vec4, vec3, 10 dwords; none in the
            // files.
            s.lights = r.vec(8, |r| {
                Ok(Light {
                    key: r.string()?,
                    name: r.string()?,
                    target: r.string()?,
                    rotation: r.floats::<4>()?,
                    position: r.floats::<3>()?,
                    words: (0..words).map(|_| r.u32()).collect::<Result<_, _>>()?,
                })
            })?;
        }
        if (3..=8).contains(&version) {
            // @ 0x004a7630: vectors of dwords; none in the files.
            r.vec(8, |r| {
                r.string()?;
                r.vec_u32().map(|_| ())
            })?;
        }
        Ok((s, r.at))
    }

    /// The mesh called `name`.
    pub fn mesh(&self, name: &str) -> Option<&Mesh> {
        self.meshes.iter().find(|m| m.name == name)
    }

    /// The node called `name`.
    pub fn node(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    /// The object called `name`.
    pub fn object(&self, name: &str) -> Option<&Object> {
        self.objects.iter().find(|o| o.name == name)
    }
}

fn read_mesh(r: &mut Reader) -> Result<Mesh, DbError> {
    let v = r.u32()?;
    let mut m = Mesh {
        name: r.string()?,
        parent: r.string()?,
        bbox: r.floats::<15>()?,
        vertex_count: r.u32()?,
        face_count: r.u32()?,
        corner_count: r.u32()?,
        materials: r.strings()?,
        vertices: r.vec_f::<3>()?,
        ..Mesh::default()
    };
    r.vec_u32()?; // empty in every file
    if v < 8 {
        r.vec_u16()?; // face start offsets into the corner list
    }
    m.face_materials = r.vec_u8()?;
    if v < 6 {
        m.face_normals = r.vec_f::<3>()?;
    }
    r.vec_u32()?;
    m.corners = r.vec_u16()?;
    r.vec_u32()?;
    m.corner_normals = r.vec_f::<3>()?;
    for set in &mut m.uvs {
        *set = r.vec_f::<2>()?;
    }
    r.vec_u32()?;
    if v < 6 {
        r.vec_f::<3>()?;
    }
    if v >= 4 {
        r.vec_u16()?;
        m.groups = r.vec_u16()?;
        if v < 6 {
            r.vec_u32()?;
        }
        r.strings()?;
        r.vec_u8()?;
        m.corner_vertices = r.vec_u16()?;
        r.vec_u16()?;
        r.vec_u16()?;
        r.vec_u16()?;
        m.vertex_normals = r.vec_f::<3>()?;
        if v == 5 {
            r.vec_u16()?;
        }
    }
    if v >= 6 {
        m.face_planes = r.vec_f::<4>()?;
        m.corner_planes = r.vec_f::<4>()?;
    }
    Ok(m)
}

fn read_object(r: &mut Reader) -> Result<Object, DbError> {
    let v = r.u32()?;
    let name = r.string()?;
    r.string()?;
    let mesh = r.string()?;
    let parent = r.string()?;
    let o = Object {
        name,
        mesh,
        parent,
        rotation: r.floats::<4>()?,
        position: r.floats::<3>()?,
        scale: r.f32()?,
    };
    if v >= 7 {
        r.u32()?;
    }
    if v >= 9 {
        r.vec_u32()?;
    }
    if v >= 10 {
        r.vec_f::<2>()?;
        r.strings()?;
        r.vec_u16()?;
    }
    Ok(o)
}

fn read_material(r: &mut Reader) -> Result<Material, DbError> {
    let v = r.u32()?;
    let name = r.string()?;
    let mut maps: [String; 6] = Default::default();
    for m in &mut maps {
        *m = r.string()?;
    }
    let mut colours = [0; 4];
    for c in &mut colours {
        *c = r.u32()?;
    }
    Ok(Material {
        name,
        maps,
        colours,
        unknown: r.f32()?,
        transparency: r.f32()?,
        flags: r.u32()?,
        unknown2: if v >= 4 { r.f32()? } else { 1.0 },
    })
}

fn read_node(r: &mut Reader) -> Result<Node, DbError> {
    let v = r.u32()?;
    let name = r.string()?;
    r.string()?;
    let mut n = Node {
        name,
        last_frame: r.u32()?,
        ..Node::default()
    };
    // The key readers test the node version stored @ 0x006e0ac4 (< 9: an
    // extra value per key, dropped).
    let old = v < 9;
    n.rotation_keys = r.vec(0x4c - 16, |r| {
        let t = r.u32()?;
        let q = r.floats::<4>()?;
        if old {
            r.floats::<4>()?;
        }
        r.floats::<4>()?;
        r.floats::<4>()?;
        r.u32()?;
        r.tcb()?;
        Ok((t, q))
    })?;
    n.scale_keys = r.vec(0x24 - 12, |r| {
        let t = r.u32()?;
        let s = r.floats::<3>()?;
        if old {
            r.floats::<3>()?;
        }
        r.tcb()?;
        Ok((t, s[0]))
    })?;
    n.position_keys = r.vec(0x3c - 12, |r| {
        let t = r.u32()?;
        let p = r.floats::<3>()?;
        if old {
            r.floats::<3>()?;
        }
        r.floats::<3>()?;
        r.floats::<3>()?;
        r.tcb()?;
        Ok((t, p))
    })?;
    n.morph_keys = r.vec(8, |r| {
        let target = r.string()?;
        r.u32()?;
        let verts = r.vec_f::<3>()?;
        r.tcb()?;
        Ok((target, verts))
    })?;
    if v >= 4 {
        n.note_keys = r.vec(8, |r| Ok((r.u32()?, r.string()?)))?;
    }
    if v >= 11 {
        n.linear_rotation_keys = r.vec(20, |r| Ok((r.u32()?, r.floats::<4>()?)))?;
        n.linear_scale_keys = r.vec(16, |r| Ok((r.u32()?, r.floats::<3>()?)))?;
        n.linear_position_keys = r.vec(16, |r| Ok((r.u32()?, r.floats::<3>()?)))?;
    }
    Ok(n)
}

/// The camera record has no version word.
fn read_camera(r: &mut Reader) -> Result<Camera, DbError> {
    let name = r.string()?;
    r.string()?;
    let c = Camera {
        name,
        rotation: r.floats::<4>()?,
        position: r.floats::<3>()?,
        fov: r.f32()?,
        near: r.f32()?,
        far: r.f32()?,
    };
    r.u32()?;
    Ok(c)
}
