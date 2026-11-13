//! The subject menu (loop mode 8): a character with subjects clicked with
//! an empty hand opens it (@ 0x00440530); each frame draws the warp, the
//! character's subjects in columns of four cells (@ 0x0043b850) and the
//! pointer; a click on a cell picks its subject (@ 0x0043ba40 →
//! @ 0x0043b5b0), a click elsewhere does nothing, the right button closes
//! it.
//!
//! The geometry depends on the screen width (@ 0x0043aa40):
//!
//! | Width | x0 | y0 | cell | ring | bank |
//! |---|---|---|---|---|---|
//! | 640 | 450 | 160 | 64 | `Interface.spr` #41 | `<World>Sujets52.spr` |
//! | 800 | 560 | 200 | 80 | #42 | `<World>Sujets64.spr` |
//! | 1024 | 720 | 256 | 100 | #43 | `<World>Sujets80.spr` |
//!
//! Another width keeps the 640 values.
//!
//! *Atlantis Evolution*'s 2004 program (atlantis4-cd-en-3022848) sets them
//! when `CHANGECD=n` loads the disc's subject bank (FUN_0043dae0, from
//! FUN_00452310): the bank `spr\suj0<n>.spr` stripped of its last four
//! characters, then the width's suffix and `.spr` (0x006aef5c); a fourth
//! global 0x0070c0a0, the ring's frame, is 30 (`interface.SPR` has 31
//! frames, no #41..#43):
//!
//! | Width | x0 | y0 | cell | ring | bank |
//! |---|---|---|---|---|---|
//! | 640 | 450 | 160 | 64 | #30 | `suj0<n>52.spr` (not on the CD) |
//! | 800 | 560 | 200 | 80 | #30 | `suj0<n>64.spr` (not on the CD) |
//! | 1024 | 720 | 256 | **64** | #30 | `suj0<n>.spr` |
//!
//! Another width sets nothing: the data's 450, 160, 64, 64 (0x006aef4c..
//! 0x006aef58), ring 0 (0x0070c0a0 is `.bss`), no bank. The draw
//! (FUN_0043e790, new) skips a subject whose value is not above 0
//! (@ 0x0043e8d0); the rest is *Atlantis III*'s, and so is the hit test
//! (FUN_0043e9a0 → FUN_0043e480, identical to @ 0x0043ba40 → @ 0x0043b5b0).

use atlantis2_engine::script::{Subject, SubjectTable};

/// The menu's geometry for a screen width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub x0: i32,
    pub y0: i32,
    pub cw: i32,
    pub ch: i32,
    /// The ring's frame in `Interface.spr`.
    pub ring: usize,
    /// The bank's size suffix (`52`, `64`, `80`).
    pub suffix: &'static str,
}

impl Layout {
    pub fn for_width(width: usize) -> Layout {
        let (x0, y0, c, ring, suffix) = match width {
            800 => (560, 200, 80, 42, "64"),
            1024 => (720, 256, 100, 43, "80"),
            _ => (450, 160, 64, 41, "52"),
        };
        Layout {
            x0,
            y0,
            cw: c,
            ch: c,
            ring,
            suffix,
        }
    }

    /// The 2004 program's geometry (FUN_0043dae0 @ 0x0043db64..0x0043df0e)
    /// and the bank's suffix: `None` for a width it sets nothing for.
    pub fn for_width_2004(width: usize) -> (Layout, Option<&'static str>) {
        let (x0, y0, c, suffix) = match width {
            640 => (450, 160, 64, Some("52")),
            800 => (560, 200, 80, Some("64")),
            1024 => (720, 256, 64, Some("")),
            _ => (450, 160, 64, None),
        };
        let lay = Layout {
            x0,
            y0,
            cw: c,
            ch: c,
            ring: if suffix.is_some() { 30 } else { 0 },
            suffix: suffix.unwrap_or(""),
        };
        (lay, suffix)
    }

    /// The 2004 subject bank of disc `n` (1..4, FUN_00452310): `None`
    /// when the width loads none.
    pub fn bank_path_2004(width: usize, n: i32) -> Option<String> {
        let (_, suffix) = Self::for_width_2004(width);
        suffix.map(|s| format!("spr/suj0{n}{s}.spr"))
    }

    /// The subject bank of a world (`MONDE` @ 0x0044bb30 → @ 0x0043aa40):
    /// `spr\<World>Sujets<suffix>.spr`; only Hoggar, LDM and Inter ship.
    pub fn bank_path(&self, world: i32) -> Option<String> {
        let name = match world {
            1 => "Hoggar",
            2 => "LDM",
            3 => "Bagdad",
            4 => "Paleo",
            5 => "Inter",
            _ => return None,
        };
        Some(format!("spr/{name}Sujets{}.spr", self.suffix))
    }

    /// The first row's top for `n` subjects (@ 0x0077ad6c): `y0 − (n −
    /// 3) × ch` below five, else `y0`.
    pub fn ystart(&self, n: i32) -> i32 {
        if n < 5 {
            self.y0 - (n - 3) * self.ch
        } else {
            self.y0
        }
    }

    /// The cells drawn for the list (@ 0x0043b850): per subject, its
    /// frame, the centre it is drawn at (`AtlSpr_AffCentre`), its alpha
    /// (1.0, or 0.5 when not enabled), and the ring's centre.
    pub fn cells(&self, list: &[Subject]) -> Vec<Cell> {
        let n = list.len() as i32;
        let mut x = self.x0;
        let mut ys = self.ystart(n);
        let mut out = Vec::with_capacity(list.len());
        for (i, s) in list.iter().enumerate() {
            let i = i as i32;
            if i == 4 || i == 8 || i == 12 {
                x += self.cw;
                ys = self.y0 - i * self.ch;
            }
            out.push(Cell {
                value: s.value,
                x: x + self.cw / 2,
                y: ys + self.ch * i + self.ch / 2,
                alpha: if s.flag == 1 { 1.0 } else { 0.5 },
                ring: (x + self.cw / 2, ys + self.ch * i + self.cw / 2),
            });
        }
        out
    }

    /// The hit test of a click at the pointer (@ 0x0043b5b0): the index
    /// of the cell under it (column-major, edges included, not checked
    /// against the count), or `None` outside the grid (−2 in the
    /// original).
    pub fn hit(&self, n: i32, pointer: (i32, i32)) -> Option<usize> {
        let cols = match n {
            ..=4 => 1,
            5..=8 => 2,
            9..=12 => 3,
            _ => 0,
        };
        let rows = n.min(4);
        let ys = self.ystart(n);
        let (px, py) = pointer;
        if px >= self.x0 && px <= self.x0 + self.cw * cols && py >= ys && py <= ys + self.ch * rows
        {
            Some((((px - self.x0) / self.cw) * rows + (py - ys) / self.ch) as usize)
        } else {
            None
        }
    }

    /// What a click answers (@ 0x0043ba40): the value of the entry under
    /// the pointer as the table's memory holds it, or −2 outside.
    pub fn pick(&self, table: &SubjectTable, character: i32, pointer: (i32, i32)) -> i32 {
        match self.hit(table.count(character), pointer) {
            Some(i) => table.entry(character, i).value,
            None => -2,
        }
    }
}

/// One drawn cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub value: i32,
    pub x: i32,
    pub y: i32,
    pub alpha: f32,
    pub ring: (i32, i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_2004_layout_by_width() {
        let (l, s) = Layout::for_width_2004(1024);
        assert_eq!(
            (l.x0, l.y0, l.cw, l.ch, l.ring, s),
            (720, 256, 64, 64, 30, Some(""))
        );
        let (l, s) = Layout::for_width_2004(800);
        assert_eq!(
            (l.x0, l.y0, l.cw, l.ring, s),
            (560, 200, 80, 30, Some("64"))
        );
        let (l, s) = Layout::for_width_2004(1280);
        assert_eq!((l.x0, l.y0, l.cw, l.ring, s), (450, 160, 64, 0, None));
        assert_eq!(
            Layout::bank_path_2004(1024, 3).as_deref(),
            Some("spr/suj03.spr")
        );
        assert_eq!(
            Layout::bank_path_2004(640, 1).as_deref(),
            Some("spr/suj0152.spr")
        );
        assert_eq!(Layout::bank_path_2004(1280, 1), None);
    }
}
