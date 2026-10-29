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
