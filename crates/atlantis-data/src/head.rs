//! The files of the 3D talking heads of the dialogs (the animated mouth
//! drawn over the dialog video): the F3DC mesh (`DIALOG\PERSO\*.3DC`), the
//! head animation and mouth shapes (`*.3DA`), the speaker tables and
//! cameras (`*.S3D`), the lip-sync tracks (`WAV\*.SYC`) and the texture
//! maps (`DIALOG\MAPS\*.3DM`).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/head3d.cpp, GPLv3, the ScummVM team), which
//! follows the loaders of `atlantis.exe`: `FUN_004187e0` / `FUN_0044b77c`
//! (the F3DC blocks), `FUN_0044b2e8` (their relocation), `FUN_0044b4a4`
//! (the material table), `FUN_0044d5f0` (the S3D) and `FUN_0044d390` (the
//! SYC). The engine side (animation, projection and rasteriser) is in
//! `atlantis-engine`'s `head` module.
//!
//! Every parser returns `None` for a file too short for what it names; the
//! original reads such files blindly.

/// Little-endian reads at absolute offsets, `None` past the end.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u32(&self, off: u32) -> Option<u32> {
        let o = off as usize;
        let b = self.0.get(o..o.checked_add(4)?)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&self, off: u32) -> Option<i32> {
        self.u32(off).map(|v| v as i32)
    }

    fn f64(&self, off: u32) -> Option<f64> {
        let o = off as usize;
        let b = self.0.get(o..o.checked_add(8)?)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Some(f64::from_le_bytes(a))
    }

    /// A NUL-terminated string of at most `max` bytes, read as Latin-1.
    fn cstr(&self, off: u32, max: u32) -> String {
        let mut s = String::new();
        for i in 0..max {
            match self.0.get(off.wrapping_add(i) as usize) {
                Some(&c) if c != 0 => s.push(char::from(c)),
                _ => break,
            }
        }
        s
    }
}

/// An F3DC block's section offsets, file-relative (`FUN_004187e0` /
/// `FUN_0044b77c`: the count at 0x1c, the offsets after it, relative to
/// 0x1c).
fn sections(f: &Bytes) -> Option<Vec<u32>> {
    if f.0.len() < 0x20 {
        return None;
    }
    let n = f.u32(0x1c)?;
    if 0x20 + 4 * u64::from(n) > f.0.len() as u64 {
        return None;
    }
    (0..n)
        .map(|i| f.u32(0x20 + 4 * i).map(|o| 0x1c_u32.wrapping_add(o)))
        .collect()
}

/// A face of a mesh: three corners, their edge records and texture
/// coordinates, the plane and the group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadFace {
    pub flags: u32,
    /// The corners, vertex indices of the node.
    pub vi: [usize; 3],
    /// The edge records' file offsets: the record of the edge from corner
    /// k to corner k + 1, shared by the two faces of an edge.
    pub recs: [u32; 3],
    /// The texture coordinates of the corners (none for group type 1).
    pub uv: [[i32; 2]; 3],
    /// The face's plane, an index into the node's planes.
    pub plane: usize,
    pub d: i32,
    /// The face's group, an index into the node's groups.
    pub group: usize,
}

/// A face group: its fill type (1 flat colour, 2 affine texture, 9
/// perspective texture) and its material.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadGroup {
    pub gtype: u32,
    pub material: String,
}

/// A node of the F3DC tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadNode {
    pub flags: u32,
    pub parent: Option<usize>,
    pub child: Option<usize>,
    pub sibling: Option<usize>,
    pub pos: [i32; 3],
    /// The 1.15 rotation.
    pub m: [i32; 9],
    pub verts: Vec<[i32; 3]>,
    /// A vertex record has flag 0x80 (its +0, the only flag `FUN_004618b0`
    /// keeps).
    pub vertex_flag80: bool,
    /// The face planes' normals.
    pub planes: Vec<[i32; 3]>,
    pub radius: i32,
    pub center: [i32; 3],
    /// The faces of the groups of types 1, 2 and 9, in list order.
    pub faces: Vec<HeadFace>,
    pub groups: Vec<HeadGroup>,
}

/// A material of the F3DC table (`FUN_0044b4a4`, records of 0x2c bytes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadMaterial {
    pub name: String,
    /// The texture map, `DIALOG\MAPS\<texture>.3DM`; empty for none.
    pub texture: String,
    /// The record's +0x20, a colour group's +8 (`FUN_0044ac84` @
    /// 0x0044ace6).
    pub colour: u32,
}

/// A `.3DC` mesh.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct F3dc {
    pub nodes: Vec<HeadNode>,
    pub materials: Vec<HeadMaterial>,
}

impl F3dc {
    pub fn parse(data: &[u8]) -> Option<F3dc> {
        let f = Bytes(data);
        let secs = sections(&f)?;
        let first = *secs.first()?;
        // FUN_0044b2e8: the relocation base is section 0 - 1
        let r = first.wrapping_sub(1);
        let rel = |p: u32| r.wrapping_add(p);
        let mut nodes = Vec::with_capacity(secs.len());
        let mut links = Vec::with_capacity(secs.len());
        for &s in &secs {
            let mut n = HeadNode {
                flags: f.u32(s + 0xc)?,
                ..HeadNode::default()
            };
            let mut l = [0u32; 3];
            for (k, lk) in l.iter_mut().enumerate() {
                let p = f.u32(s + 0x10 + 4 * k as u32)?;
                *lk = if p != 0 { rel(p) } else { 0 };
            }
            links.push(l);
            for k in 0..3 {
                n.pos[k] = f.i32(s + 0x1c + 4 * k as u32)?;
            }
            for k in 0..9 {
                n.m[k] = f.i32(s + 0x28 + 4 * k as u32)?;
            }
            let nv = f.u32(s + 0x7c)?;
            let va = rel(f.u32(s + 0x80)?);
            for v in 0..nv {
                let rec = va.wrapping_add(0x28 * v);
                if f.u32(rec)? & 0x80 != 0 {
                    n.vertex_flag80 = true;
                }
                let b = rec.wrapping_add(4);
                n.verts.push([f.i32(b)?, f.i32(b + 4)?, f.i32(b + 8)?]);
            }
            let npl = f.u32(s + 0x94)?;
            let pa = rel(f.u32(s + 0x98)?);
            for p in 0..npl {
                let b = pa.wrapping_add(16 * p);
                n.planes.push([f.i32(b)?, f.i32(b + 4)?, f.i32(b + 8)?]);
            }
            n.radius = f.i32(s + 0xb0)?;
            for k in 0..3 {
                n.center[k] = f.i32(s + 0xb4 + 4 * k as u32)?;
            }
            let mut g = f.u32(s + 0xa4)?;
            g = if g != 0 { rel(g) } else { 0 };
            while g != 0 {
                let gtype = f.u32(g + 4)?;
                n.groups.push(HeadGroup {
                    gtype,
                    material: f.cstr(g + 12, 16),
                });
                if matches!(gtype, 1 | 2 | 9) {
                    let mut fc = f.u32(g + 0x20)?;
                    fc = if fc != 0 { rel(fc) } else { 0 };
                    while fc != 0 {
                        let mut face = HeadFace {
                            flags: f.u32(fc)?,
                            group: n.groups.len() - 1,
                            ..HeadFace::default()
                        };
                        for k in 0..3u32 {
                            let vo = rel(f.u32(fc + 8 + 12 * k)?).wrapping_sub(va);
                            let vi = (vo / 0x28) as usize;
                            if vo % 0x28 != 0 || vi >= n.verts.len() {
                                return None;
                            }
                            face.vi[k as usize] = vi;
                            face.recs[k as usize] = rel(f.u32(fc + 16 + 12 * k)?);
                            if gtype != 1 {
                                // type 1 faces are 0x34 bytes: no
                                // texture-coordinate pointers
                                let uvp = rel(f.u32(fc + 0x34 + 4 * k)?);
                                face.uv[k as usize] = [f.i32(uvp)?, f.i32(uvp.wrapping_add(4))?];
                            }
                        }
                        let po = rel(f.u32(fc + 0x2c)?).wrapping_sub(pa);
                        face.plane = (po / 16) as usize;
                        if face.plane >= n.planes.len() {
                            return None;
                        }
                        face.d = f.i32(fc + 0x30)?;
                        n.faces.push(face);
                        let nx = f.u32(fc + 4)?;
                        fc = if nx != 0 { rel(nx) } else { 0 };
                    }
                }
                let nx = f.u32(g)?;
                g = if nx != 0 { rel(nx) } else { 0 };
            }
            nodes.push(n);
        }
        let find = |off: u32| secs.iter().position(|&s| s == off);
        for (n, l) in nodes.iter_mut().zip(&links) {
            n.parent = find(l[0]);
            n.child = find(l[1]);
            n.sibling = find(l[2]);
        }
        // The material table (FUN_0044b4a4): records of 0x2c bytes after
        // the sections.
        let mbase = 0x20 + 4 * secs.len() as u32;
        let count = f.u32(mbase)?;
        let mut materials = Vec::new();
        for i in 0..count {
            let rec = mbase + 4 + 0x2c * i;
            materials.push(HeadMaterial {
                name: f.cstr(rec, 16),
                texture: f.cstr(rec + 16, 16),
                colour: f.u32(rec + 0x20)?,
            });
        }
        Some(F3dc { nodes, materials })
    }
}

/// A key of an animation track: its time and value (a 1.15 quaternion, or
/// a position with a zero fourth value).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeadKey {
    pub t: i32,
    pub v: [i32; 4],
}

/// The animation of one node (one section of an `A.3DA` file).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadTrack {
    /// The last frame.
    pub last: i32,
    pub rot: Vec<HeadKey>,
    pub pos: Vec<HeadKey>,
}

/// The head animation (`*A.3DA`): one track per node.
pub fn parse_head_tracks(data: &[u8]) -> Option<Vec<HeadTrack>> {
    let f = Bytes(data);
    let secs = sections(&f)?;
    let mut out = Vec::with_capacity(secs.len());
    for &s in &secs {
        let nrot = f.i32(s + 4)?;
        let npos = f.i32(s + 8)?;
        let rk = 0x1c_u32.wrapping_add(f.u32(s + 12)?);
        let pk = 0x1c_u32.wrapping_add(f.u32(s + 16)?);
        let mut t = HeadTrack {
            last: f.i32(s)?,
            ..HeadTrack::default()
        };
        for k in 0..nrot.max(0) as u32 {
            let b = rk.wrapping_add(0x14 * k);
            t.rot.push(HeadKey {
                t: f.i32(b)?,
                v: [f.i32(b + 4)?, f.i32(b + 8)?, f.i32(b + 12)?, f.i32(b + 16)?],
            });
        }
        for k in 0..npos.max(0) as u32 {
            let b = pk.wrapping_add(0x10 * k);
            t.pos.push(HeadKey {
                t: f.i32(b)?,
                v: [f.i32(b + 4)?, f.i32(b + 8)?, f.i32(b + 12)?, 0],
            });
        }
        out.push(t);
    }
    Some(out)
}

/// The mouth shapes (`*M.3DA`): per shape, a pivot then `nv` vertices
/// (`nv` the largest vertex count of the meshes the S3D deforms).
pub fn parse_mouth_shapes(data: &[u8], nv: usize) -> Option<Vec<Vec<[i32; 3]>>> {
    let f = Bytes(data);
    let secs = sections(&f)?;
    secs.iter()
        .map(|&s| {
            (0..=nv as u32)
                .map(|v| {
                    let b = s.wrapping_add(12 * v);
                    Some([
                        f.i32(b)?,
                        f.i32(b.wrapping_add(4))?,
                        f.i32(b.wrapping_add(8))?,
                    ])
                })
                .collect()
        })
        .collect()
}

/// A speaker's `.S3D` (`FUN_0044d5f0`, version >= 0x96).
#[derive(Clone, Debug, PartialEq)]
pub struct S3d {
    /// The nodes the mouth deforms.
    pub meshes: Vec<i32>,
    /// Per mesh, the mouth shape of each table entry.
    pub rows: Vec<Vec<i32>>,
    /// Per phoneme (26 is the neutral mouth): shape, -, shape, weight,
    /// shape, weight (entries of `rows`).
    pub phonemes: [[i32; 6]; 27],
    /// Per camera, 8 doubles: the target, the eye, the scale and the field
    /// of view.
    pub cams: Vec<[f64; 8]>,
}

impl S3d {
    pub fn parse(data: &[u8]) -> Option<S3d> {
        let f = Bytes(data);
        let n1 = f.i32(0x10)?.max(0) as u32;
        let n2 = f.i32(0x14)?.max(0) as u32;
        let mt = f.u32(0x24)?;
        let rt = f.u32(0x28)?;
        let mut meshes = Vec::new();
        let mut rows = Vec::new();
        for r in 0..n1 {
            meshes.push(f.i32(mt.wrapping_add(4 * r))?);
            let mut row = Vec::new();
            for k in 0..n2 {
                row.push(f.i32(rt.wrapping_add(4 * n1 + 4 * (r * n2 + k)))?);
            }
            rows.push(row);
        }
        let mut phonemes = [[0i32; 6]; 27];
        for (i, p) in phonemes.iter_mut().enumerate() {
            for (k, v) in p.iter_mut().enumerate() {
                *v = f.i32(0x164 + 24 * i as u32 + 4 * k as u32)?;
            }
        }
        let mut cams = Vec::new();
        let ct = f.u32(0x30)?;
        if ct != 0 {
            let nc = f.i32(0x1c)?.max(0) as u32;
            for c in 0..nc {
                let mut cam = [0f64; 8];
                for (k, v) in cam.iter_mut().enumerate() {
                    *v = f.f64(ct.wrapping_add(0x54 * c + 8 * k as u32))?;
                }
                cams.push(cam);
            }
        }
        Some(S3d {
            meshes,
            rows,
            phonemes,
            cams,
        })
    }
}

/// A lip-sync record: the time in ms and the phoneme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SycRecord {
    pub t: i32,
    pub phoneme: i32,
}

/// A line's lip-sync track (`WAV\ATA<NNNN><letter>.SYC`, `FUN_0044d390`:
/// records of 12 bytes {time ms, phoneme, energy}); at least two records.
pub fn parse_syc(data: &[u8]) -> Option<Vec<SycRecord>> {
    let f = Bytes(data);
    if data.len() < 0x90 {
        return None;
    }
    let count = f.u32(0x20)?;
    let base = 0x8c_u32.checked_add(f.u32(0x8c)?)?.checked_add(4)?;
    if u64::from(base) + 12 * u64::from(count) > data.len() as u64 || count < 2 {
        return None;
    }
    (0..count)
        .map(|i| {
            Some(SycRecord {
                t: f.i32(base + 12 * i)?,
                phoneme: f.i32(base + 12 * i + 4)?,
            })
        })
        .collect()
}

/// A texture map (`.3DM`): the shade table (32 shades x 256 colours, the
/// RGB565 colour in the high half) at 0x1c, the 256 x 256 image after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadTexture {
    pub table: Vec<u32>,
    pub image: Vec<u8>,
}

impl HeadTexture {
    pub fn parse(data: &[u8]) -> Option<HeadTexture> {
        if data.len() < 0x1801c {
            return None;
        }
        let table = data[0x1c..0x801c]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        Some(HeadTexture {
            table,
            image: data[0x801c..0x1801c].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syc_records() {
        let mut d = vec![0u8; 0x90 + 24];
        d[0x20..0x24].copy_from_slice(&2u32.to_le_bytes());
        d[0x8c..0x90].copy_from_slice(&0u32.to_le_bytes());
        d[0x90..0x94].copy_from_slice(&10i32.to_le_bytes());
        d[0x94..0x98].copy_from_slice(&3i32.to_le_bytes());
        d[0x9c..0xa0].copy_from_slice(&90i32.to_le_bytes());
        d[0xa0..0xa4].copy_from_slice(&26i32.to_le_bytes());
        let s = parse_syc(&d).unwrap();
        assert_eq!(
            s,
            vec![
                SycRecord { t: 10, phoneme: 3 },
                SycRecord { t: 90, phoneme: 26 }
            ]
        );
        assert!(parse_syc(&d[..0x90 + 20]).is_none());
    }
}
