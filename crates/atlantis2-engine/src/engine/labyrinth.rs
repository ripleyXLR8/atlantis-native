//! The labyrinth's souls, China's labyrinth (`labysol.wam` views 1-30,
//! the floor, and `labyplaf.wam` views 31-60, the ceiling; chapter 60;
//! the original's module `pz_ames`).
//!
//! Four wandering souls, the characters `AME1..4`, pass now and then at
//! the end of the corridors seen from the player's cell. Each soul is
//! the soul of one direction (its entity's SPW prefix `1a`..`1d`); its
//! states are not placed in a view but in two template views, 509 and
//! 510, at a distance (animations 1-3). The hooks load the states that
//! the corridors around the cell show, hold them hidden, and play each
//! after a random delay, with sound 220. They write no variable: the
//! labyrinth's way out is the scripts' (the demons' form and the fan).
//!
//! - The maps (@ 0x005e1184 for views 31-60, @ 0x005e1214 for 1-30): 13
//!   rows of 11 bytes, a cell every other byte, a space for an open wall.
//!   FUN_0048e160 maps a view to its cell (rows 1, 3 .. 11, a row's five
//!   cells in a snake's order). The tests read 5 x 5 bytes around the
//!   cell; every byte they can reach outside a map is not a space.
//! - The visibility tests FUN_0048e200 (20 of them, in order, the first
//!   that holds answers): a test is a few cells open or walled around the
//!   cell, and a state: the entity's SPW prefix (`_stricmp`), letter `A`,
//!   an animation and a template view (FUN_0048e9d0). In views 1-30 the
//!   prefixes are swapped a ↔ c and b ↔ d (the floor's map is the
//!   ceiling's turned round).
//! - The entry FUN_0048df10 (from FUN_0049bd50, `LA` views 1-60, after
//!   the view's own slots are loaded): the puzzle mode 3; the candidate
//!   states, every state of every entity of the place in the file's
//!   order that a test accepts; with 3-4 candidates one is dropped at
//!   random, 5-7 two, 8 or more three (`random(count)` draws, a draw on a
//!   dropped one costs one of 100); then each kept candidate is loaded (a
//!   slot after the view's own), held (active 0, playing 0) and given a
//!   delay.
//! - The delay FUN_0048ec40(slot): 10 plus `random(b + 45)`, b =
//!   max(0, (150 − end) · 5), warp frames, in the slot's +0x1c (the start
//!   frame, which the souls' slots never use); while it is within 44 of
//!   another soul's non-zero delay it is drawn again as 10 plus
//!   `random(b + 105)`, ten tries at most.
//! - The tick FUN_0048eb30 (FUN_0049bfb0's mode 3, every warp frame, not
//!   on the 15 Hz flag): outside views 1-60 the quit (the mode 3 back to
//!   0); each soul's slot (by its id only): a delay counted down, at 0
//!   the passage: active, playing, frame 0, `(NewSound=220)`; a passage
//!   at its last frame: hidden, a new delay, `(NewSound=221)` (a sound on
//!   no disc: nothing heard); a passage of animation 3 otherwise: its
//!   click kind cleared. `labyplaf.wam`'s animation 3 is a character,
//!   but its click kind goes in the frame after it appears, when its
//!   file's frame 0 or 1 is still empty: no soul is ever clicked.
//! - The generator: `random(n)` = `rand() · n / 32767` (@ 0x0048e130), the
//!   C runtime's one `rand`, which the music's start seeds with the 15 Hz
//!   count and the dialogs, the music and the place sounds draw from too
//!   ([`Engine::rand`]); the place animations draw from it as well in the
//!   original, from their own generator here.

use atlantis2_data::defnames;

use super::{Engine, Event};
use crate::blackbird;
use crate::crt::Rand;

/// The puzzle mode of the souls (@ 0x0048df1b).
pub const SOULS_MODE: i32 = 3;

/// The souls' characters (@ 0x005e1338, 0x005e1330, 0x005e1328,
/// 0x005e1320, the tick's and the delay's order).
const SOULS: [&[u8]; 4] = [b"AME1", b"AME2", b"AME3", b"AME4"];

/// The commands of a passage's start and end (@ 0x005e1310, 0x005e1300).
const PASSAGE: &[u8] = b"(NewSound=220)";
const GONE: &[u8] = b"(NewSound=221)";

/// The warnings and the fatal error (@ 0x005e12a4, 0x005e12cc).
const ALREADY: &str = "pz_ames_entree_warp() ! spw_deja_loaded";
const OVERFLOW: &str = "pz_ames_entree_warp spw overflow";

/// The two maps as the tests read them, `.` where the byte is a space
/// (an open wall or a cell), `#` elsewhere: 13 rows of 11 bytes, row 0
/// and 12 the borders, byte 0 of a row its letter. The ceiling's
/// (@ 0x005e1184, views 31-60) and the floor's (@ 0x005e1214, 1-30).
const CEILING: [&[u8; 11]; 13] = [
    b"###########",
    b"##.#.###.##",
    b"#...###...#",
    b"######.####",
    b"###...###.#",
    b"##.#.###.##",
    b"#..########",
    b"##.#####.##",
    b"#.....###.#",
    b"####.#.####",
    b"#.###..##.#",
    b"##.###.#.##",
    b"###########",
];
const FLOOR: [&[u8; 11]; 13] = [
    b"###########",
    b"##.###.#.##",
    b"#...###...#",
    b"####.######",
    b"#.###...###",
    b"##.###.#.##",
    b"########..#",
    b"##.#####.##",
    b"#.###.....#",
    b"####.#.####",
    b"#.###..##.#",
    b"##.#.###.##",
    b"###########",
];

/// A view's cell (FUN_0048e160): the map (`true` the ceiling's) and the
/// byte offset in it (row · 11 + 2 · column + 1). Views 31..: row
/// 2 · ((v − 31) / 5) + 1; up to 30: row 11 − 2 · ((v − 1) / 5); the
/// column (v − 1) mod 5, turned round (4 − it) unless row & 3 = 3. C
/// division and remainder (towards 0).
pub fn soul_cell(view: i32) -> (bool, i32) {
    let (ceiling, row) = if view > 30 {
        (true, (view - 31) / 5 * 2 + 1)
    } else {
        (false, 11 - (view - 1) / 5 * 2)
    };
    let mut col = (view - 1) % 5;
    if row & 3 != 3 {
        col = 4 - col;
    }
    (ceiling, row * 11 + col * 2 + 1)
}

/// FUN_0048e990 (`open`, a space) at (dx, dy) of the cell: the byte at
/// offset + dy · 11 + dx. A byte off the map is not a space (the
/// executable's bytes around the maps: the strings before, the NUL and
/// the other map between, the module's strings after).
pub fn soul_open(cell: (bool, i32), dx: i32, dy: i32) -> bool {
    let map = if cell.0 { &CEILING } else { &FLOOR };
    let off = cell.1 + dy * 11 + dx;
    (0..143).contains(&off) && map[(off / 11) as usize][(off % 11) as usize] == b'.'
}

/// One visibility test: every `open` cell open, and (when `walled` is not
/// empty) one of `walled` not open; then the state.
struct Test {
    open: &'static [(i32, i32)],
    walled: &'static [(i32, i32)],
    /// The direction letter of the SPW prefix `1?` (@ 0x005e12f0 `1d`,
    /// 0x005e12f4 `1a`, 0x005e12f8 `1b`, 0x005e12fc `1c`).
    dir: u8,
    anim: u32,
    view: u32,
}

/// FUN_0048e200's tests, in its order (@ 0x0048e200..0x0048e98a): four
/// turns seen at distance 3 and 2 (template view 510), four turns at
/// distances 1 and 2 (509), four corridors crossing at distance 1 (510).
const TESTS: [Test; 20] = {
    const T1O: &[(i32, i32)] = &[(0, -1), (1, -2)];
    const T1W: &[(i32, i32)] = &[(1, 0), (2, -1)];
    const T2O: &[(i32, i32)] = &[(-1, 0), (-2, -1)];
    const T2W: &[(i32, i32)] = &[(0, -1), (-1, -2)];
    const T3O: &[(i32, i32)] = &[(0, 1), (-1, 2)];
    const T3W: &[(i32, i32)] = &[(-1, 0), (-2, 1)];
    const T4O: &[(i32, i32)] = &[(1, 0), (2, 1)];
    const T4W: &[(i32, i32)] = &[(0, 1), (1, 2)];
    const T9W: &[(i32, i32)] = &[(-2, -1), (-1, 0)];
    const T9O: &[(i32, i32)] = &[(0, -1), (-1, -2)];
    const T10W: &[(i32, i32)] = &[(-1, 2), (0, 1)];
    const T10O: &[(i32, i32)] = &[(-1, 0), (-2, 1)];
    const T11W: &[(i32, i32)] = &[(2, 1), (1, 0)];
    const T11O: &[(i32, i32)] = &[(0, 1), (1, 2)];
    const T12W: &[(i32, i32)] = &[(1, -2), (0, -1)];
    const T12O: &[(i32, i32)] = &[(1, 0), (2, -1)];
    [
        Test {
            open: T1O,
            walled: T1W,
            dir: b'c',
            anim: 3,
            view: 510,
        },
        Test {
            open: T2O,
            walled: T2W,
            dir: b'b',
            anim: 3,
            view: 510,
        },
        Test {
            open: T3O,
            walled: T3W,
            dir: b'a',
            anim: 3,
            view: 510,
        },
        Test {
            open: T4O,
            walled: T4W,
            dir: b'd',
            anim: 3,
            view: 510,
        },
        Test {
            open: T1O,
            walled: T1W,
            dir: b'c',
            anim: 2,
            view: 510,
        },
        Test {
            open: T2O,
            walled: T2W,
            dir: b'b',
            anim: 2,
            view: 510,
        },
        Test {
            open: T3O,
            walled: T3W,
            dir: b'a',
            anim: 2,
            view: 510,
        },
        Test {
            open: T4O,
            walled: T4W,
            dir: b'd',
            anim: 2,
            view: 510,
        },
        Test {
            open: T9O,
            walled: T9W,
            dir: b'c',
            anim: 1,
            view: 509,
        },
        Test {
            open: T10O,
            walled: T10W,
            dir: b'b',
            anim: 1,
            view: 509,
        },
        Test {
            open: T11O,
            walled: T11W,
            dir: b'a',
            anim: 1,
            view: 509,
        },
        Test {
            open: T12O,
            walled: T12W,
            dir: b'd',
            anim: 1,
            view: 509,
        },
        Test {
            open: T9O,
            walled: T9W,
            dir: b'c',
            anim: 2,
            view: 509,
        },
        Test {
            open: T10O,
            walled: T10W,
            dir: b'b',
            anim: 2,
            view: 509,
        },
        Test {
            open: T11O,
            walled: T11W,
            dir: b'a',
            anim: 2,
            view: 509,
        },
        Test {
            open: T12O,
            walled: T12W,
            dir: b'd',
            anim: 2,
            view: 509,
        },
        Test {
            open: &[(-1, -2), (-1, 2), (0, -1), (0, 1)],
            walled: &[(-1, 0)],
            dir: b'a',
            anim: 1,
            view: 510,
        },
        Test {
            open: &[(-2, -1), (2, -1), (-1, 0), (1, 0)],
            walled: &[(0, -1)],
            dir: b'b',
            anim: 1,
            view: 510,
        },
        Test {
            open: &[(1, -2), (1, 2), (0, -1), (0, 1)],
            walled: &[(1, 0)],
            dir: b'c',
            anim: 1,
            view: 510,
        },
        Test {
            open: &[(-2, 1), (2, 1), (-1, 0), (1, 0)],
            walled: &[(0, 1)],
            dir: b'd',
            anim: 1,
            view: 510,
        },
    ]
};

/// The 5 x 5 bytes the tests read around `cell`, as a getter.
pub trait Hood {
    fn open(&self, dx: i32, dy: i32) -> bool;
}

impl Hood for (bool, i32) {
    fn open(&self, dx: i32, dy: i32) -> bool {
        soul_open(*self, dx, dy)
    }
}

/// A 5 x 5 neighbourhood given as 25 bytes, row by row from (−2, −2),
/// `.` open: for tests.
impl Hood for [u8; 25] {
    fn open(&self, dx: i32, dy: i32) -> bool {
        (-2..=2).contains(&dx)
            && (-2..=2).contains(&dy)
            && self[((dy + 2) * 5 + dx + 2) as usize] == b'.'
    }
}

/// FUN_0048e200 for a state of an entity whose SPW prefix is `prefix`,
/// in the current view numbered `view` (FUN_0048e9d0 reads it: up to 30
/// the direction letters are swapped a ↔ c, b ↔ d, @ 0x0048ea2e..
/// 0x0048ea8c): whether a test holds around the cell and names the state
/// (`_stricmp` of the prefix, the letter `A`, the animation, the template
/// view).
pub fn soul_visible(
    hood: &impl Hood,
    view: i32,
    prefix: &str,
    letter: u32,
    anim: u32,
    at: u32,
) -> bool {
    TESTS.iter().any(|t| {
        if !t.open.iter().all(|&(dx, dy)| hood.open(dx, dy)) {
            return false;
        }
        if !t.walled.is_empty() && t.walled.iter().all(|&(dx, dy)| hood.open(dx, dy)) {
            return false;
        }
        let dir = if view > 30 {
            t.dir
        } else {
            match t.dir {
                b'a' => b'c',
                b'b' => b'd',
                b'c' => b'a',
                _ => b'b',
            }
        };
        let name = [b'1', dir];
        prefix.as_bytes().eq_ignore_ascii_case(&name)
            && letter as u8 == b'A'
            && anim == t.anim
            && at == t.view
    })
}

/// `random(n)` (@ 0x0048e130): `rand() · n / 32767`, the compiler's
/// multiply, the code of @ 0x004958c0.
pub fn soul_random(rand: &mut Rand, n: i32) -> i32 {
    blackbird::random(rand, n)
}

/// How many candidates the entry drops (@ 0x0048dfa1..0x0048dfd9): none
/// up to 2, one for 3-4, two for 5-7, three from 8.
fn drops(count: usize) -> usize {
    match count {
        0..=2 => 0,
        3 | 4 => 1,
        5..=7 => 2,
        _ => 3,
    }
}

/// The entry's drops (@ 0x0048dfa1..0x0048e00d) on `count` candidates
/// (each 1, kept): `random(count)` drawn while drops are left and the
/// budget of 100 misses is not spent; a draw on a kept one drops it, on a
/// dropped one costs one of the budget. A draw of `count` (`rand()` =
/// 32767) reads the stack word past the candidates' (never written by the
/// entry: what earlier calls left there); the port takes it as not 1, a
/// miss (the original drops nothing more when it is 1).
pub fn soul_drops(rand: &mut Rand, count: usize) -> Vec<bool> {
    let mut kept = vec![true; count];
    let mut left = drops(count);
    let mut budget = 100;
    if left == 0 {
        return kept;
    }
    loop {
        if budget == 0 {
            break;
        }
        let r = soul_random(rand, count as i32);
        if r >= 0 && (r as usize) < count && kept[r as usize] {
            kept[r as usize] = false;
            left -= 1;
            if left == 0 {
                break;
            }
        } else {
            budget -= 1;
        }
    }
    kept
}

impl Engine {
    /// The souls' ids: `DefName_GetIdName(0, "AME1".."AME4")` (the tick's
    /// and the delay's lookups, @ 0x0048eb69..0x0048eba5). An unknown name
    /// is the original's fatal error; the game's data have all four.
    fn soul_ids(&self) -> [i32; 4] {
        SOULS.map(|n| {
            self.names
                .id(defnames::PERSONNAGES, n)
                .map_or(i32::MIN, |i| i as i32)
        })
    }

    /// FUN_0048eb10: the mode 3 back to 0.
    fn souls_quit(&mut self) {
        if self.puzzle_mode() == SOULS_MODE {
            self.set_puzzle_mode(0);
        }
    }

    /// FUN_0048e200 for state `si` of entity `ei` of the place, the
    /// current view numbered `view`, around `cell`.
    fn soul_candidate(&self, cell: (bool, i32), view: i32, ei: usize, si: usize) -> bool {
        let e = &self.wam.entities[ei];
        let Some(s) = e.state_records().nth(si) else {
            return false;
        };
        soul_visible(&cell, view, &e.prefix, s.letter, s.anim, s.view)
    }

    /// FUN_0048df10 in a view numbered `view` (the current view's +0x10):
    /// run by the entry of `LA` views 1-60 after the view's slots are
    /// loaded; public for tests and tools (the hook alone, on the slots
    /// loaded).
    pub fn souls_entry(&mut self, view: u32) {
        let view = view as i32;
        self.set_puzzle_mode(SOULS_MODE);
        let cell = soul_cell(view);
        // Every state of every entity, in the place's order.
        let all: Vec<(usize, usize)> = self
            .wam
            .entities
            .iter()
            .enumerate()
            .flat_map(|(ei, e)| (0..e.states.len()).map(move |si| (ei, si)))
            .collect();
        let candidates: Vec<(usize, usize)> = all
            .iter()
            .copied()
            .filter(|&(ei, si)| self.soul_candidate(cell, view, ei, si))
            .collect();
        if candidates.len() >= 0x22 {
            // The fatal error (@ 0x0048df86); the data never reach it.
            log::error!("{OVERFLOW}");
            self.with_script(|s, _| s.fail(OVERFLOW.to_string()));
        }
        self.souls_candidates = candidates.len() as i32;
        self.souls_loaded = 0;
        let kept = soul_drops(&mut self.rand, candidates.len());
        for (k, &(ei, si)) in candidates.iter().enumerate() {
            if !kept[k] {
                continue;
            }
            // FUN_0048eae0: a slot of that record already loaded.
            let found = self
                .anim
                .slots
                .iter()
                .position(|s| s.record == Some((ei, si)));
            let n = match found {
                Some(n) => {
                    log::warn!("{ALREADY}");
                    n
                }
                None => {
                    let Some(n) = self.load_soul(ei, si) else {
                        continue;
                    };
                    n
                }
            };
            let slot = &mut self.anim.slots[n];
            slot.active = false;
            slot.playing = false;
            self.soul_delay(n);
            self.souls_loaded += 1;
        }
    }

    /// FUN_0047c4e0 for state `si` of entity `ei`: the slot loaded after
    /// the others, or `None` (the file on no disc, the slots full). The
    /// octopus fight loads its states with it too.
    pub(super) fn load_soul(&mut self, ei: usize, si: usize) -> Option<usize> {
        let e = &self.wam.entities[ei];
        let s = e.state_records().nth(si)?;
        let name = e.spw_name(&s);
        let (kind, id) = (e.kind as i32, e.index as i32);
        let sprite = if e.kind == 2 {
            self.names.param(defnames::OBJETS, e.con_name.as_bytes())
        } else {
            0
        };
        let spw = self
            .files
            .read(atlantis2_data::Folder::Spw, &name)
            .and_then(|d| atlantis2_data::spw::Spw::parse(d).ok());
        self.event(Event::SpwLoad {
            name: name.clone(),
            found: spw.is_some(),
        });
        let spw = spw?;
        self.spw_loaded = true;
        let n = self.anim.slots.len();
        self.anim.load(&name, spw, kind, id, sprite, s);
        let slot = self.anim.slots.get_mut(n)?;
        slot.record = Some((ei, si));
        // AtlSpw_load's next state (+0x28); no chain is linked here
        // (FUN_0047c820 runs only at a view's entry).
        let next = self.next_state_record((ei, si));
        self.anim.set_next(n, next);
        Some(n)
    }

    /// FUN_0048ec40(n): slot `n`'s delay (its +0x1c), drawn again while it
    /// is within 44 of another soul's non-zero delay, ten tries at most
    /// (@ 0x0048ec40..0x0048ed75). Public for tests and tools.
    pub fn soul_delay(&mut self, n: usize) {
        let ids = self.soul_ids();
        let base = 150i32
            .wrapping_sub(self.anim.slots[n].end)
            .wrapping_mul(5)
            .max(0);
        let d = soul_random(&mut self.rand, base.wrapping_add(45)) + 10;
        self.anim.slots[n].start = d;
        let mut tries = 0;
        loop {
            let me = self.anim.slots[n].start;
            let ok = !self.anim.slots.iter().enumerate().any(|(j, s)| {
                j != n
                    && ids.contains(&s.id)
                    && s.start != 0
                    && s.start.wrapping_sub(me).wrapping_abs() < 45
            });
            tries += 1;
            if !ok {
                let d = soul_random(&mut self.rand, base.wrapping_add(105)) + 10;
                self.anim.slots[n].start = d;
            }
            if tries >= 10 || ok {
                break;
            }
        }
    }

    /// FUN_0048eb30 in a view numbered `view`: the puzzle tick of mode 3,
    /// every warp frame; public for tests and tools (the tick alone).
    pub fn souls_tick(&mut self, view: u32) {
        let view = view as i32;
        if !(1..=60).contains(&view) {
            self.souls_quit();
            return;
        }
        let ids = self.soul_ids();
        for n in 0..self.anim.slots.len() {
            if !ids.contains(&self.anim.slots[n].id) {
                continue;
            }
            let slot = &mut self.anim.slots[n];
            if slot.start > 0 {
                slot.start -= 1;
                if slot.start == 0 {
                    slot.active = true;
                    slot.playing = true;
                    slot.frame = 0;
                    self.event(Event::SoulPassage {
                        slot: n,
                        start: true,
                    });
                    self.with_script(|s, h| s.run_command(h, PASSAGE));
                }
            } else if slot.frame == slot.end - 1 {
                slot.active = false;
                slot.playing = false;
                self.soul_delay(n);
                self.event(Event::SoulPassage {
                    slot: n,
                    start: false,
                });
                self.with_script(|s, h| s.run_command(h, GONE));
            } else if slot.state.letter as u8 == b'A' && slot.state.anim == 3 {
                slot.click = 0;
            }
        }
    }
}
