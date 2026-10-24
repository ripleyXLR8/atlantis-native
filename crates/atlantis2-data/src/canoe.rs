//! The canoe puzzle's files (`datas_a2\3d\pz_pirog\`, disc 4): the
//! waypoints of `traj_pirog.txt` and the depth tiles `zbf\*.zb`.
//!
//! The 3D objects (`*.db`) are [`crate::db`] scenes, the board's picture
//! `fdpuzzle.tga` a [`crate::tga`] map, the tiles and the stone counter the
//! sprite banks `spr\SolPirog.spr` and `spr\ponPirog.spr`
//! ([`crate::spr`]).

use crate::con::atof;

/// The waypoints file (@ 0x005e29a4, under `datas_a2\3d\`).
pub const TRAJECTORY_FILE: &str = "pz_pirog\\traj_pirog.txt";

/// The most waypoints the table holds (`MAX_COORD_TRAJET`, @ 0x00498474).
pub const MAX_WAYPOINTS: usize = 400;

/// A waypoint record (0x18 bytes from @ 0x006d2cf8): a position, the name
/// of its mesh in the exported scene (`W9A002`, `X9E1`: the tile's
/// trajectory group digit at [1], at [2] the hero's animation letter or
/// `E` / `S` for a segment's ends) and the flag the trajectory's rebuild
/// sets on a segment joined across a gap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Waypoint {
    pub pos: [f32; 3],
    /// Up to 8 characters, NUL padded (record +0x0c; the record's +0x14
    /// dword follows it).
    pub name: [u8; 8],
    /// Record +0x14.
    pub flag: i32,
}

impl Waypoint {
    /// The name as text.
    pub fn name_str(&self) -> String {
        let end = self.name.iter().position(|&c| c == 0).unwrap_or(8);
        self.name[..end].iter().map(|&c| char::from(c)).collect()
    }
}

/// `isalnum` of the C runtime in the "C" locale (@ 0x005ad9f0, the class
/// mask 0x107).
fn alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

/// `read_coord_traj` (FUN_004982a0): for each `$Mesh:` (@ 0x005e298c),
/// the name (up to 8 characters, to the first space), then after the next
/// `[` (@ 0x005e2964) three numbers read by FUN_00498490: `atof` (@
/// 0x005ae620) stored as a float (`fstp dword` @ 0x004984a6, toward zero
/// when `toward_zero`, the x87 control word 0x0e7f), the pointer then
/// moved past the number's characters (alphanumeric, `.`, `-`) and the
/// characters that follow until the next alphanumeric, `.` or `-`. The
/// search for the next `$Mesh:` starts there. At most [`MAX_WAYPOINTS`]
/// (the original reports `MAX_COORD_TRAJET` and stops).
pub fn parse_trajectory(data: &[u8], toward_zero: bool) -> Vec<Waypoint> {
    const MESH: &[u8] = b"$Mesh:";
    let find = |from: usize, pat: &[u8]| -> Option<usize> {
        data.get(from..)?
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|i| from + i)
    };
    let store = |x: f64| -> f32 {
        let y = x as f32;
        if toward_zero && y.is_finite() && y != 0.0 && f64::from(y).abs() > x.abs() {
            f32::from_bits(y.to_bits() - 1)
        } else {
            y
        }
    };
    let at = |i: usize| data.get(i).copied().unwrap_or(0);
    let mut out = Vec::new();
    let mut p = find(0, MESH);
    while let Some(mut i) = p {
        if out.len() >= MAX_WAYPOINTS {
            log::warn!("read_coord_traj() : MAX_COORD_TRAJET");
            break;
        }
        i += MESH.len();
        let mut w = Waypoint::default();
        let mut k = 0;
        while at(i) != b' ' && k < 8 && i < data.len() {
            w.name[k] = at(i);
            i += 1;
            k += 1;
        }
        let Some(b) = find(i, b"[") else {
            log::warn!("read_coord_traj() : err [ Point {}", out.len());
            break;
        };
        i = b + 1;
        for c in &mut w.pos {
            *c = store(atof(data.get(i..).unwrap_or(&[])));
            while alnum(at(i)) || at(i) == b'.' || at(i) == b'-' {
                i += 1;
            }
            while i < data.len() && !alnum(at(i)) && at(i) != b'.' && at(i) != b'-' {
                i += 1;
            }
        }
        out.push(w);
        p = find(i, MESH);
    }
    out
}

/// The 33 depth tiles' files, in the order of the table built on the
/// stack by FUN_004970f0 from @ 0x005e28c4 (one name each 0x1c bytes
/// down): tile id t = 4 k + r is `pz<A + r><k + 1>0001`, the tile of
/// square k turned r quarters; id 32 is square 8's (`pzA90001`). The
/// table names `.zbf` files (the full-screen depth images, which no disc
/// has); the shipped `.zb` caches (`.zbf` → `.zb`, @ 0x005e253c,
/// 0x005e2538) are read instead when the first exists.
pub fn depth_tile_name(id: usize) -> String {
    let (k, r) = if id == 32 { (8, 0) } else { (id / 4, id % 4) };
    format!(
        "pz_pirog\\zbf\\pz{}{}0001.zb",
        char::from(b'A' + r as u8),
        k + 1
    )
}

/// A depth tile (`.zb`, read whole by FUN_00403b70): `w` x `h` 32-bit
/// values, rows top-down, 0 where the tile is empty. The size is the
/// tile sprite's of `SolPirog.spr` (FUN_0059d8c4); the original reads the
/// file into a buffer of that size, so a shorter file leaves the rest as
/// the allocation left it (zero here) and a longer one is cut.
pub fn depth_tile(data: &[u8], w: usize, h: usize) -> Vec<u32> {
    let mut out = vec![0u32; w * h];
    for (v, b) in out.iter_mut().zip(data.as_chunks::<4>().0) {
        *v = u32::from_le_bytes(*b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoints_are_read_with_their_names() {
        let t = b"Index 1 a $Mesh:Q1E1 @ [1.5,-2.25,3.0] b  $Mesh:Q1Z0007 @ [0.1,-7,8e2] c 0";
        let w = parse_trajectory(t, false);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].name_str(), "Q1E1");
        assert_eq!(w[1].name_str(), "Q1Z0007");
        assert_eq!(w[0].pos, [1.5, -2.25, 3.0]);
        assert_eq!(w[1].pos, [0.1, -7.0, 800.0]);
        // Toward zero: 0.1 is just above its nearest float's double.
        assert_eq!(
            parse_trajectory(t, true)[1].pos[0],
            f32::from_bits(0.1f32.to_bits() - 1)
        );
    }

    #[test]
    fn tile_names_follow_the_table() {
        assert_eq!(depth_tile_name(0), "pz_pirog\\zbf\\pzA10001.zb");
        assert_eq!(depth_tile_name(7), "pz_pirog\\zbf\\pzD20001.zb");
        assert_eq!(depth_tile_name(32), "pz_pirog\\zbf\\pzA90001.zb");
    }
}
