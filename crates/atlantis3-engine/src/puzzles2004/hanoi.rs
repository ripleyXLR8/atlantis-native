//! `PUZ_HANOI`: the towers of Hanoi (init FUN_00499a00, start
//! FUN_00499ea0, frame FUN_0049a0a0, free FUN_00499ef0).
//!
//! Three pegs (`std::list`s 0x007197e4 / 0x007197f0 / 0x007197fc, their
//! sizes at +4), each disk a node `{value, peg}`. The init puts one disk
//! per `DISQUE_1`, `DISQUE_2`, `DISQUE_COSMO`, `DISQUE_KAMA`,
//! `DISQUE_SAMA` in the bag (0x00719880) on peg 0, values 0..n pushed at
//! the front: the front (the top) is the highest value, the smallest disk
//! (sprite value + 2). A click on a peg picks its top disk up (or nothing
//! on an empty peg), the next click on a peg drops it there if the peg is
//! empty, is the disk's own, or its top is a bigger disk (a lower value,
//! FUN_0049a590); a click elsewhere drops the disk back, or quits on the
//! exit sprite. All the disks on peg 2 (pegs 0 and 1 empty, FUN_0049a5f0)
//! with at least two disks: `HANOI_GAGNE` = 1.

use std::any::Any;
use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{aff, collision, Host, Puzzle, PuzzleId};

/// The objects counted as disks (@ 0x00499a1c..0x00499ab7, in order).
pub const DISKS: [&str; 5] = [
    "DISQUE_1",
    "DISQUE_2",
    "DISQUE_COSMO",
    "DISQUE_KAMA",
    "DISQUE_SAMA",
];

/// The pegs' click rectangles (`RECT`s 0x00719810.., @ 0x00499b50):
/// left, top, right, bottom.
pub const RECTS: [[i32; 4]; 3] = [
    [0x46, 0x96, 0xeb, 500],
    [0x140, 0x96, 0x1e5, 500],
    [0x23a, 0x96, 0x2df, 500],
];

/// The game's state.
pub struct Hanoi {
    /// Each peg's disks, the front (top) first.
    pub pegs: [Vec<i32>; 3],
    /// The disk held (0x00719884: the node's `{value, peg}`), `None` for
    /// the null pointer.
    pub held: Option<(i32, usize)>,
    /// The peg under the pointer (0x006b2718, -1; 0x00719888).
    pub hover: Option<usize>,
    /// The disks' count (0x00719880).
    pub disks: i32,
    hanoi: Option<Arc<Bank>>,
    bout: Option<Arc<Bank>>,
}

/// `PtInRect`: left and top inside, right and bottom outside.
fn in_rect(r: &[i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

impl Hanoi {
    /// The init's set-up without the engine (tests): `disks` disks on
    /// peg 0.
    pub fn with_disks(disks: i32) -> Hanoi {
        let mut pegs: [Vec<i32>; 3] = Default::default();
        // Pushed at the front, 0 first (@ 0x00499b22..0x00499b4e).
        for i in 0..disks {
            pegs[0].insert(0, i);
        }
        Hanoi {
            pegs,
            held: None,
            hover: None,
            disks,
            hanoi: None,
            bout: None,
        }
    }

    /// FUN_00499a00 (its start FUN_00499ea0 is the engine's).
    pub fn init(host: &mut dyn Host) -> Hanoi {
        let n = DISKS.iter().filter(|d| host.in_bag(d)).count() as i32;
        // 0x00719d98 (never set) would make it 5.
        let mut g = Hanoi::with_disks(n);
        g.hanoi = host.bank("hanoi.spr");
        g.bout = host.bank("hanoi_bout.spr");
        // FUN_0042e390: the pointer centred on the display (the stretch
        // surface opens in the start, afterwards).
        let (w, h) = host.display_size();
        host.set_pointer(w / 2, h / 2);
        host.set_var("HANOI_GAGNE", 0);
        host.set_var("HANOI_QUIT", 0);
        host.sfx_load("placard.wav");
        host.sfx_load("sound500.wav");
        g
    }

    /// FUN_0049a590: whether the top disk of `from` may go on `to`.
    pub fn can_move(&self, from: usize, to: usize) -> bool {
        let (Some(&f), t) = (self.pegs[from].first(), self.pegs[to].first()) else {
            return false;
        };
        match t {
            None => true,
            _ if from == to => true,
            Some(&t) => t < f,
        }
    }

    /// FUN_0049a4d0: the top disk of `from` moved onto `to`.
    pub fn move_disk(&mut self, from: usize, to: usize) {
        if from != to && !self.pegs[from].is_empty() {
            let v = self.pegs[from].remove(0);
            self.pegs[to].insert(0, v);
        }
    }

    /// FUN_0049a5f0: pegs 0 and 1 empty.
    pub fn solved(&self) -> bool {
        self.pegs[0].is_empty() && self.pegs[1].is_empty()
    }

    /// The frame's play part (@ 0x0049a198..0x0049a3b2) for the pointer
    /// at (x, y) and a click; the sound played, the variables set to 1
    /// in order.
    pub fn play(
        &mut self,
        x: i32,
        y: i32,
        click: bool,
    ) -> (Option<&'static str>, Vec<&'static str>) {
        let mut sound = None;
        let mut vars = Vec::new();
        // The first rectangle holding the pointer.
        self.hover = RECTS.iter().position(|r| in_rect(r, x, y));
        if click {
            match self.hover {
                None => {
                    if collision(self.bout.as_ref(), 0, Some(0x32), 0x32, x, y) {
                        // The disk stays held (@ 0x0049a4bd → 0x0049a394).
                        vars.push("HANOI_QUIT");
                    } else {
                        self.held = None;
                    }
                }
                Some(p) => match self.held {
                    None => {
                        // FUN_0049a610: the top node, or nothing.
                        self.held = self.pegs[p].first().map(|&v| (v, p));
                        sound = Some("sound500.wav");
                    }
                    Some((_, from)) => {
                        if self.can_move(from, p) {
                            sound = Some("placard.wav");
                            self.move_disk(from, p);
                        } else {
                            sound = Some("sound500.wav");
                        }
                        self.held = None;
                    }
                },
            }
        }
        if self.solved() && self.disks >= 2 {
            vars.push("HANOI_GAGNE");
        }
        (sound, vars)
    }

    /// FUN_0049a640.
    fn draw(&self, host: &mut dyn Host) {
        let (px, py) = host.pointer();
        let hanoi = self.hanoi.clone();
        let hb = hanoi.as_ref();
        aff(host, "hanoi.spr", hb, 0, Some(0), 0, 1.0);
        aff(
            host,
            "hanoi_bout.spr",
            self.bout.as_ref(),
            0,
            Some(0x32),
            0x32,
            1.0,
        );
        // The rectangles' outlines (0x006e0518) are never drawn: the flag
        // is only written with 0 (@ 0x0040d72d).
        for (i, peg) in self.pegs.iter().enumerate() {
            let x = 0x96 + 0xfa * i as i32;
            aff(host, "hanoi.spr", hb, 1, Some(x), 500, 1.0);
            let mut y = (0x18 - (peg.len() as i32 - 1)) * 0x14;
            for &v in peg {
                if self.held.map(|h| h.0) != Some(v) {
                    aff(host, "hanoi.spr", hb, (v + 2) as usize, Some(x), y, 1.0);
                }
                y += 0x14;
            }
        }
        match self.held {
            Some((v, _)) => aff(host, "hanoi.spr", hb, (v + 2) as usize, Some(px), py, 1.0),
            None => host.draw_pointer(),
        }
        host.flip();
    }
}

impl Puzzle for Hanoi {
    fn id(&self) -> PuzzleId {
        PuzzleId::Hanoi
    }

    /// FUN_0049a0a0.
    fn frame(&mut self, host: &mut dyn Host) {
        if !host.script_prologue() {
            return;
        }
        if host.var("HANOI_GAGNE") != 0 || host.var("HANOI_QUIT") != 0 {
            return;
        }
        let (x, y) = host.pointer();
        let click = host.click();
        if click {
            host.set_click(false);
        }
        let (sound, vars) = self.play(x, y, click);
        if let Some(s) = sound {
            host.sfx_play(s);
        }
        for v in vars {
            host.set_var(v, 1);
        }
        self.draw(host);
    }

    /// FUN_00499ef0's game part.
    fn free(&mut self, host: &mut dyn Host) {
        self.hanoi = None;
        self.bout = None;
        host.sfx_remove("placard.wav");
        host.sfx_remove("sound500.wav");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let mut g = Hanoi::with_disks(3);
        assert_eq!(g.pegs[0], [2, 1, 0]);
        // The smallest disk (2) on an empty peg, then the middle one may
        // not go on it.
        assert!(g.can_move(0, 2));
        g.move_disk(0, 2);
        assert!(!g.can_move(0, 2));
        assert!(g.can_move(0, 1));
        assert!(g.can_move(2, 2));
        assert!(!g.can_move(1, 0));
        assert!(!g.solved());
    }

    #[test]
    fn clicks() {
        let mut g = Hanoi::with_disks(2);
        let peg = |i: usize| (RECTS[i][0] + 1, RECTS[i][1] + 1);
        // Pick from peg 0, drop on peg 1.
        let (x, y) = peg(0);
        assert_eq!(g.play(x, y, true), (Some("sound500.wav"), vec![]));
        assert_eq!(g.held, Some((1, 0)));
        let (x, y) = peg(1);
        assert_eq!(g.play(x, y, true), (Some("placard.wav"), vec![]));
        assert_eq!(g.pegs[1], [1]);
        // The big disk onto the small one: refused.
        let (x, y) = peg(0);
        g.play(x, y, true);
        let (x, y) = peg(1);
        assert_eq!(g.play(x, y, true), (Some("sound500.wav"), vec![]));
        assert_eq!(g.pegs[0], [0]);
        // An empty peg picks nothing up, but sounds.
        let (x, y) = peg(2);
        assert_eq!(g.play(x, y, true), (Some("sound500.wav"), vec![]));
        assert_eq!(g.held, None);
        // Solve: 0 → 2, 1 → 2.
        for (a, b) in [(0, 2), (1, 2)] {
            let (x, y) = peg(a);
            g.play(x, y, true);
            let (x, y) = peg(b);
            let r = g.play(x, y, true);
            if a == 1 {
                assert_eq!(r, (Some("placard.wav"), vec!["HANOI_GAGNE"]));
            }
        }
        assert_eq!(g.pegs[2], [1, 0]);
        // Right and bottom edges are outside the rectangle.
        assert_eq!(g.play(RECTS[0][2], RECTS[0][1], false).0, None);
        assert_eq!(g.hover, None);
    }

    #[test]
    fn one_disk_never_wins() {
        let mut g = Hanoi::with_disks(1);
        g.move_disk(0, 2);
        assert!(g.solved());
        assert_eq!(g.play(0, 0, false), (None, vec![]));
    }
}
