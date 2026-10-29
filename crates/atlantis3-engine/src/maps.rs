//! The two map screens' rules and drawing: `AtlCarte`, the Land of the
//! Dead's place map (loop mode 9, frame @ 0x00405700), and `AtlCarteTour`,
//! the tower's board (loop mode 10, frame @ 0x00406780). The game's side
//! (the banks, the mode, the scripts) is `game::hooks::screens`; what is
//! here takes the screen, the banks and the pointer, so that a test can
//! replay the original's calls.
//!
//! Both screens are drawn over the running warp frame (@ 0x00440690).

use atlantis2_engine::Fpu;
use atlantis3_data::spr::Bank;

use crate::inventory::{aff, aff_centre, Aff};

/// `Carte.spr`'s places 1..5: their offsets from the screen's centre where
/// they are drawn and clicked (@ 0x00405700, `AtlSpr_Aff` then
/// `SPR_COLLISION` @ 0x00437080).
pub const CARTE_PLACES: [(i32, i32); 5] = [(-9, 1), (-68, 102), (-130, -114), (202, 82), (83, 37)];

/// The hover test's positions of the same sprites, and where their labels
/// k + 5 are drawn (@ 0x00405520): place 1 16 pixels left of where it is
/// drawn, the others 5.
pub const CARTE_LABELS: [(i32, i32); 5] = [(-25, 1), (-73, 102), (-135, -114), (197, 82), (78, 37)];

/// The chapter of place k = 1..5 (@ 0x0040593e..: ESI).
pub const CARTE_CHAPTERS: [i32; 5] = [12, 14, 13, 15, 16];

/// The screen's centre: the integer halves of the width and the height
/// (@ 0x00405707, `cdq` / `sar`).
pub fn centre(w: usize, h: usize) -> (i32, i32) {
    (w as i32 / 2, h as i32 / 2)
}

/// Where place k (1..5) is drawn and clicked.
pub fn carte_place(w: usize, h: usize, k: usize) -> (i32, i32) {
    let (cx, cy) = centre(w, h);
    let (dx, dy) = CARTE_PLACES[k - 1];
    (cx + dx, cy + dy)
}

/// Where place k (1..5) is hover-tested and its label k + 5 drawn.
pub fn carte_label(w: usize, h: usize, k: usize) -> (i32, i32) {
    let (cx, cy) = centre(w, h);
    let (dx, dy) = CARTE_LABELS[k - 1];
    (cx + dx, cy + dy)
}

/// What the map refuses before anything is drawn (@ 0x00405707..0x004057fb):
/// the variable set to 1, then the screen closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarteRefusal {
    /// More than one feather carried: `TROP_DE_PLUMES`.
    Feathers,
    /// The high priest not met (flag 80 = 0): `GP_PAS_VU`.
    PriestNotSeen,
}

impl CarteRefusal {
    /// The variable the refusal sets to 1.
    pub fn var(self) -> &'static str {
        match self {
            CarteRefusal::Feathers => "TROP_DE_PLUMES",
            CarteRefusal::PriestNotSeen => "GP_PAS_VU",
        }
    }
}

/// The refusals, with `flag(n)` the story flag n: the feathers carried
/// (flags 535..544 summed, one less for each of the scale's pans holding
/// one, flags 562 / 563 ≠ 0) above 1, then flag 80 = 0.
pub fn carte_refusal(flag: impl Fn(i32) -> i32) -> Option<CarteRefusal> {
    let mut feathers: i32 = (535..=544).map(&flag).sum();
    if flag(562) != 0 {
        feathers -= 1;
    }
    if flag(563) != 0 {
        feathers -= 1;
    }
    if feathers > 1 {
        Some(CarteRefusal::Feathers)
    } else if flag(80) == 0 {
        Some(CarteRefusal::PriestNotSeen)
    } else {
        None
    }
}

/// The place (1..5) whose sprite covers the pointer at the hover
/// positions (@ 0x00405520), the first in order; none without the bank.
pub fn carte_hover(carte: &Bank, w: usize, h: usize, ptr: (i32, i32)) -> Option<usize> {
    (1..=5).find(|&k| {
        let (x, y) = carte_label(w, h, k);
        carte.opaque_at(k, Some(x), y, ptr.0, ptr.1)
    })
}

/// The place (1..5) a left click at the pointer selects (@ 0x0040593e:
/// `SPR_COLLISION` of sprite k at its drawn position, the first in order).
pub fn carte_click(carte: &Bank, w: usize, h: usize, ptr: (i32, i32)) -> Option<usize> {
    (1..=5).find(|&k| {
        let (x, y) = carte_place(w, h, k);
        carte.opaque_at(k, Some(x), y, ptr.0, ptr.1)
    })
}

/// Whether choosing `chapter` sends the fishermen away (@ 0x00405980..
/// 0x004059c0): flag 69 = 1 and flag 560 ≠ 1, for every place but
/// chapter 15's. The caller then sets flag 560 and runs
/// `(HidePerso=LDM_PECHEUR01)`, `(HidePerso=LDM_PECHEUR02)`,
/// `(HideObjet=LDM_POISSON2)`.
pub fn carte_fishermen(chapter: i32, flag69: i32, flag560: i32) -> bool {
    chapter != 15 && flag69 == 1 && flag560 != 1
}

/// The map's drawing (@ 0x004057fd..0x0040593b), after the warp: the map
/// `Carte.spr` #0 centred, places 1..5, the label of the place under the
/// pointer (@ 0x00405520), then the pointer `Interface.spr` #40
/// (@ 0x004281e0; `AtlCarte_Start` set that cursor). Alpha 1.0
/// (@ 0x0074e964, set by the start, never faded). The calls made.
pub fn draw_carte(
    screen: &mut [u16],
    w: usize,
    h: usize,
    carte: &Bank,
    interface: Option<&Bank>,
    ptr: (i32, i32),
) -> Vec<Aff> {
    let alpha = 1.0f32;
    let (cx, cy) = centre(w, h);
    let mut calls = Vec::new();
    calls.extend(aff_centre(screen, w, h, "Carte", carte, 0, cx, cy, alpha));
    for k in 1..=5 {
        let (x, y) = carte_place(w, h, k);
        calls.push(aff(screen, w, h, "Carte", carte, k, x, y, alpha));
    }
    if let Some(k) = carte_hover(carte, w, h, ptr) {
        let (x, y) = carte_label(w, h, k);
        calls.push(aff(screen, w, h, "Carte", carte, k + 5, x, y, alpha));
    }
    if let Some(i) = interface {
        calls.push(aff(screen, w, h, "Interface", i, 40, ptr.0, ptr.1, 1.0));
    }
    calls
}

/// The board's first square (@ 0x0071f96c): the tables' cell 0.
pub const TOUR_FIRST: i32 = 39;

/// The board's squares 39..99 (written by the start-up's constructor
/// @ 0x00406130, called @ 0x004d67a1): x (@ 0x0074e97c) and y
/// (@ 0x0074ea70) at 800 × 600.
pub const TOUR_X: [i32; 61] = [
    312, 356, 392, 432, 469, 507, 542, 571, 589, 602, 612, 614, 608, 587, 559, 521, 487, 447, 412,
    380, 348, 316, 285, 255, 229, 206, 189, 184, 185, 196, 219, 248, 279, 312, 348, 386, 418, 455,
    487, 512, 526, 528, 509, 477, 442, 407, 373, 342, 309, 285, 272, 269, 288, 316, 349, 385, 425,
    445, 436, 393, 350,
];
pub const TOUR_Y: [i32; 61] = [
    442, 440, 443, 429, 416, 400, 374, 350, 314, 282, 250, 217, 181, 152, 124, 105, 91, 91, 91, 94,
    98, 109, 120, 135, 155, 178, 205, 232, 264, 293, 319, 335, 347, 353, 353, 356, 350, 340, 319,
    292, 258, 222, 186, 167, 156, 153, 153, 161, 175, 196, 223, 252, 280, 291, 295, 293, 277, 248,
    214, 205, 232,
];

/// Pawn A's frames when it leads (@ 0x0074ebb4 = 0 .. @ 0x00714680 = 29),
/// when it does not (@ 0x00714684 = 30 .. @ 0x00714688 = 74), pawn B's
/// (@ 0x0071468c = 75 .. @ 0x00714690 = 157).
pub const PAWN_A_LEADS: (i32, i32) = (0, 29);
pub const PAWN_A_BEHIND: (i32, i32) = (30, 74);
pub const PAWN_B: (i32, i32) = (75, 157);

/// The pawns' animation (@ 0x0074eb68..0x0074eb8c).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TourAnim {
    /// Pawn A's frame (@ 0x0074eb6c), pawn B's (@ 0x0074eb68).
    pub frame_a: i32,
    pub frame_b: i32,
    /// The clock's last time (@ 0x0074eb78) and the accumulator added to
    /// the elapsed time (@ 0x0074eb88; zeroed by the start and the step).
    pub t0: f64,
    pub acc: f64,
}

impl TourAnim {
    /// `AtlCarteTour_Start` (@ 0x00406620): the clock at `now`, pawn A's
    /// first frame 0 when A (`a`, @ 0x00789ad8) is past B (`b`,
    /// @ 0x00789ad4), else 30; pawn B's 75.
    pub fn start(a: i32, b: i32, now: f64) -> TourAnim {
        TourAnim {
            frame_a: if a > b {
                PAWN_A_LEADS.0
            } else {
                PAWN_A_BEHIND.0
            },
            frame_b: PAWN_B.0,
            t0: now,
            acc: 0.0,
        }
    }

    /// The step (@ 0x00406811..0x004068f4): when `now − t0 + acc` >
    /// 1.0 / 15.0 (strict, `test ah, 0x41`), t0 := now, acc := 0, both
    /// frames + 1, each wrapped to its first past its last. True when the
    /// frames moved.
    pub fn step(&mut self, fpu: Fpu, now: f64, a: i32, b: i32) -> bool {
        let d = fpu.add(fpu.sub(now, self.t0), self.acc);
        if d <= fpu.div(1.0, 15.0) {
            return false;
        }
        self.t0 = now;
        self.acc = 0.0;
        self.frame_a += 1;
        self.frame_b += 1;
        let (first, last) = if a > b { PAWN_A_LEADS } else { PAWN_A_BEHIND };
        if self.frame_a > last {
            self.frame_a = first;
        }
        if self.frame_b > PAWN_B.1 {
            self.frame_b = PAWN_B.0;
        }
        true
    }
}

/// The board's scale (@ 0x004067ab): `fild W`, `fmul dword 0.00125`
/// (@ 0x00700350), `fstp dword`: under the renderers' control word
/// (0x0e7f, truncating) 0.99999994 at W = 800, so the pawns' positions
/// truncate one pixel down.
pub fn tour_scale(fpu: Fpu, w: usize) -> f32 {
    fpu.f32(fpu.mul(w as f64, f64::from(0.00125f32)))
}

/// Where a pawn on `square` is drawn (@ 0x00406908..0x00406935): the
/// table's x and y − `lift` (15.0 for pawn A @ 0x00700344, 25.0 for pawn B
/// @ 0x00700340), each × the scale and truncated (`_ftol` @ 0x006d34bc).
/// None for a square off the board.
pub fn pawn_pos(fpu: Fpu, s: f32, square: i32, lift: f64) -> Option<(i32, i32)> {
    let i = usize::try_from(square - TOUR_FIRST).ok()?;
    let (x, y) = (*TOUR_X.get(i)?, *TOUR_Y.get(i)?);
    let s = f64::from(s);
    let py = fpu.mul(fpu.sub(f64::from(y), lift), s);
    let px = fpu.mul(f64::from(x), s);
    Some((px.trunc() as i32, py.trunc() as i32))
}

/// The tower board's drawing (@ 0x004067c1..0x004069b2), after the warp:
/// `CarteTour<W>.spr` #0 at the centre (its hot spot), then, in
/// `tourmagi.wam` only (`on`, @ 0x00789ae3), the animation's step and the
/// pawns: A (square `a`, @ 0x00789ad8) then B (`b`, @ 0x00789ad4). No
/// pointer: the start reset the cursor (@ 0x004282e0), so @ 0x004281e0
/// draws nothing. Alpha 1.0 (@ 0x0074ebb0). The calls made.
#[allow(clippy::too_many_arguments)]
pub fn draw_tour(
    screen: &mut [u16],
    w: usize,
    h: usize,
    board: &Bank,
    pawns: Option<&Bank>,
    anim: &mut TourAnim,
    fpu: Fpu,
    now: f64,
    pos: (i32, i32, bool),
) -> Vec<Aff> {
    let (a, b, on) = pos;
    let alpha = 1.0f32;
    let (cx, cy) = centre(w, h);
    let s = tour_scale(fpu, w);
    let mut calls = vec![aff(screen, w, h, "CarteTour", board, 0, cx, cy, alpha)];
    if !on {
        return calls;
    }
    anim.step(fpu, now, a, b);
    let Some(p) = pawns else {
        return calls;
    };
    for (square, lift, frame) in [(a, 15.0, anim.frame_a), (b, 25.0, anim.frame_b)] {
        let (Some((x, y)), Ok(f)) = (pawn_pos(fpu, s, square, lift), usize::try_from(frame)) else {
            continue;
        };
        calls.push(aff(screen, w, h, "PionsTour", p, f, x, y, alpha));
    }
    calls
}
