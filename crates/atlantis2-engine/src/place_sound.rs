//! The place sounds (`bruitage`): the sound sources of a place, heard on
//! voices 3 to 10 from a listener that the executable places in each view.
//!
//! A place file's ambient objects (tag 0x0b) are the sources: a position,
//! a level, four `.apc` files with a probability each. A view's bruitage
//! chunks (tag 0x06) name the sources heard there with three floats q:
//! q0 the share of the sound that does not depend on direction, q1 a
//! gain, q2 stored but not read. A table built into the executable gives
//! the listener's position for each (place, view) of ten places; a place
//! not in it has no place sound.
//!
//! Transcribed from the original:
//!
//! - the view entry (FUN_0047ad30): the listener moves to the new view's
//!   position (the old one kept as the current listener), each source
//!   heard there takes one of 8 slots unless one already holds it (it
//!   then goes on with the q of the view where it started), its files
//!   loaded once by name (a second source naming a file shares the first
//!   one's sound);
//! - every panorama frame (FUN_0047b390): for each slot whose voice
//!   plays, the two ears' gains (FUN_004594c0, its float arithmetic under
//!   the x87 control word) and the voice's volume and pan; then the
//!   scheduler (FUN_0047b4d0): a silent slot draws one of its files by
//!   their probabilities (one `rand()`), or waits a random number of 15 Hz
//!   ticks; a sound starts silent and gets its level at the next frame;
//!   a source whose first probability is 100 loops;
//! - during a video (FUN_0047b030, FUN_0047b090): the listener moves from
//!   the old view's position to the new one's in as many steps as the
//!   video has frames, and the sources not heard in the new view fade
//!   out over the last two thirds; at its end (FUN_0047b280) their slots
//!   are freed. A move without a video keeps every slot;
//! - a place file loaded (FUN_0047afc0): every slot freed.

use atlantis2_data::wam::{View, Wam};

use crate::crt::Rand;
use crate::fpu::{ftol, Fpu};
use crate::sound::volume_to_attenuation;

/// The slots (@ 0x006732d0, 0x1c bytes each).
pub const SLOTS: usize = 8;
/// The voice of slot 0; slot k plays on voice 3 + k.
pub const FIRST_VOICE: usize = 3;
/// The sound table's size (@ 0x006735c8, 0x30 bytes per entry).
pub const TABLE: usize = 50;

/// float 1/127 (@ 0x005c66a8).
const K127: f64 = (1.0f32 / 127.0f32) as f64;
/// float 20000 (@ 0x005c66a4).
const PAN_SCALE: f64 = 20000.0;
/// float π (@ 0x005c678c).
const PI_F: f64 = std::f32::consts::PI as f64;
/// The ear angle, float π / 4 (@ 0x005db848).
const EAR: f64 = std::f32::consts::FRAC_PI_4 as f64;
/// float 0.1 (@ 0x005c6320): the distance of a source at the listener.
const TENTH: f64 = 0.1f32 as f64;
/// float 0.66 (@ 0x005c66c8): the fade's share of a video.
const FADE_OUT: f64 = 0.66f32 as f64;

/// The listener table's place names (@ 0x005dd2c0, 16 bytes each) and
/// rows (@ 0x005dd370..0x005de954: place index, view, x, y, z).
const NAMES_VA: u32 = 0x005d_d2c0;
const ROWS_VA: u32 = 0x005d_d370;
const ROWS_END: u32 = 0x005d_e954;

/// An ambient object of the place file (tag 0x0b, 0x88 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    /// +0x00.
    pub id: u32,
    /// +0x14: x, y, z.
    pub pos: [f32; 3],
    /// +0x20.
    pub level: f32,
    /// +0x38: four probabilities in percent (signed bytes).
    pub probs: [i8; 4],
    /// +0x3c: four `.apc` names of 16 bytes; empty ends the list.
    pub files: Vec<String>,
}

/// The sources of a place file, in its order.
pub fn sources(wam: &Wam) -> Vec<Source> {
    wam.ambient_objects
        .iter()
        .filter_map(|o| {
            let p = &o.payload;
            let f = |at: usize| Some(f32::from_le_bytes(p.get(at..at + 4)?.try_into().ok()?));
            let mut files = Vec::new();
            for k in 0..4 {
                let n = p.get(0x3c + 16 * k..0x3c + 16 * (k + 1))?;
                let n: String = n
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| char::from(c))
                    .collect();
                if n.is_empty() {
                    break;
                }
                files.push(n);
            }
            let pr = p.get(0x38..0x3c)?;
            Some(Source {
                id: o.id,
                pos: [f(0x14)?, f(0x18)?, f(0x1c)?],
                level: f(0x20)?,
                probs: [pr[0] as i8, pr[1] as i8, pr[2] as i8, pr[3] as i8],
                files,
            })
        })
        .collect()
}

/// FUN_004594c0: the right and left ears' gains (object +0x24, +0x28,
/// 0..255) and `trunc(q2)` (+0x2c) of a source at `pos` with `level`,
/// heard from `listener` through the view's `q`, the camera at `yaw_cam`
/// (the angle FUN_0045adf0 reads back; the callers add π) and `pitch`.
#[allow(clippy::too_many_arguments)]
pub fn gains(
    fpu: Fpu,
    pos: [f32; 3],
    level: f32,
    listener: [f32; 3],
    q: [f32; 3],
    yaw_cam: f32,
    pitch: f32,
) -> (i32, i32, i32) {
    let f = fpu;
    let yaw = f.f(f.add(PI_F, f64::from(yaw_cam)));
    let pitch = f64::from(pitch);
    let [px, py, pz] = pos.map(f64::from);
    let [lx, ly, lz] = listener.map(f64::from);
    let dx = f.f(f.sub(px, lx));
    let dy = f.f(f.sub(py, ly));
    let dze = f.sub(pz, lz);
    let dz = f.f(dze);
    let s = f.add(f.mul(dy, dy), f.mul(dze, dz));
    let s = f.add(f.mul(dx, dx), s);
    let mut dist = f.sqrt(s);
    if dist == 0.0 {
        dist = TENTH;
    }
    let cp = pitch.cos();
    let cy = yaw.cos();
    let a = f.f(f.mul(cy, cp));
    let b = f.mul(cp, yaw.sin());
    let sp = (-pitch).sin();
    let spf = f.f(sp);
    let k = f.div(1.0, f.sqrt(f.sub(1.0, f.mul(sp, spf))));
    let c = f.f(-f.mul(k, b));
    let d = f.f(f.mul(k, a));
    let sn = f.f(EAR.sin());
    let cs = f.f(EAR.cos());
    let e = f.mul(b, cs);
    let ff = f.mul(sn, d);
    let g = f.mul(a, cs);
    let h = f.mul(sn, c);
    let z = f.f(f.mul(f.mul(spf, cs), dz));
    let d2 = f.add(dist, dist);
    let x1 = f.mul(f.sub(g, h), dx);
    let y1 = f.mul(f.sub(e, ff), dy);
    let r1 = f.f(f.add(f.div(f.add(f.add(x1, y1), z), d2), 0.5));
    let x2 = f.mul(f.add(h, g), dx);
    let y2 = f.mul(f.add(ff, e), dy);
    let r2 = f.f(f.add(f.div(f.add(f.add(x2, y2), z), d2), 0.5));
    let [q0, q1, q2] = q.map(f64::from);
    let v = f.mul(f64::from(level), q1);
    let om = f.sub(1.0, q0);
    let ear = |r: f64| {
        let g = ftol(f.div(f.mul(f.add(f.mul(r, om), q0), v), dist));
        if g >= 256 {
            255
        } else {
            g
        }
    };
    (ear(r1), ear(r2), ftol(q2))
}

/// The voice's attenuation and pan from the ears' gains `g0` (right) and
/// `g1` (left) at the effects volume `fx` and the `master` volume:
/// FUN_0047b390 (`fade` none), FUN_0047b090 (`fade` the video's factor,
/// or 2 for a source of the new view: the right ear's value is then
/// stored as a float, nothing faded), FUN_0047a650 (`half_after` false:
/// the level halved before its truncation, the same value).
#[allow(clippy::too_many_arguments)]
pub fn pan_volume(
    fpu: Fpu,
    att: &[i32; 128],
    master: i32,
    fx: i32,
    g0: i32,
    g1: i32,
    fade: Option<f64>,
    half_after: bool,
) -> (i32, i32) {
    let f = fpu;
    let mut a = f.f(f.mul(f.mul(f64::from(g1), f64::from(fx)), K127));
    let mut b = f.mul(f.mul(f64::from(g0), f64::from(fx)), K127);
    if let Some(fade) = fade {
        b = f.f(b);
        if fade < 1.0 {
            b = f.f(f.mul(fade, b));
            a = f.f(f.mul(fade, a));
        }
    }
    let p = ftol(f.mul(f.div(a, f.add(a, b)), PAN_SCALE));
    let pan = 10000 - p;
    let m = if b > a { b } else { a };
    let v = if half_after {
        ftol(m) / 2
    } else {
        ftol(f.mul(m, 0.5))
    };
    (volume_to_attenuation(att, master, v), pan)
}

/// FUN_0047b090's fade of a source not in the new view on video frame
/// `count` of `frames`: (frames − count) / (0.66 · frames), applied when
/// below 1.
pub fn transition_factor(fpu: Fpu, frames: f64, count: f64) -> f64 {
    fpu.div(fpu.sub(frames, count), fpu.mul(frames, FADE_OUT))
}

/// FUN_0047b4d0's choice for a silent slot whose timer is due (0): one
/// `rand()`, the first file whose summed probability exceeds
/// `rand · 100 / 0x7fff`, when it was loaded; else a second `rand()` and a
/// wait of `rand · (100 − Σ) / 0x7fff + 15` ticks from `tick`. Returns the
/// file and the new timer.
pub fn draw(rand: &mut Rand, probs: [i8; 4], loaded: [bool; 4], tick: i32) -> (Option<usize>, i32) {
    let r = rand.rand();
    let mut acc = 0;
    let mut pick = None;
    for (k, &p) in probs.iter().enumerate() {
        acc += i32::from(p);
        if pick.is_none() && r * 100 / 0x7fff < acc {
            pick = Some(k);
        }
    }
    if let Some(k) = pick.filter(|&k| loaded[k]) {
        return (Some(k), 0);
    }
    let r2 = rand.rand();
    (None, tick + r2 * (100 - acc) / 0x7fff + 15)
}

/// The executable's listener table.
#[derive(Clone, Debug, Default)]
pub struct Listeners {
    names: Vec<String>,
    rows: Vec<(i32, i32, [f32; 3])>,
}

impl Listeners {
    /// Read from `atlantis2.exe` (the French CD build's addresses).
    pub fn from_exe(exe: &[u8]) -> Listeners {
        use atlantis2_data::exe::bytes_at;
        let mut names = Vec::new();
        let mut va = NAMES_VA;
        while let Some(n) = bytes_at(exe, va, 16) {
            if n[0] == 0 {
                break;
            }
            names.push(
                n.iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| char::from(c))
                    .collect(),
            );
            va += 16;
        }
        let mut rows = Vec::new();
        let mut va = ROWS_VA;
        while va + 4 < ROWS_END {
            let Some(r) = bytes_at(exe, va, 20) else {
                break;
            };
            let i = |o: usize| i32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            let f = |o: usize| f32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            rows.push((i(0), i(4), [f(8), f(12), f(16)]));
            va += 20;
        }
        Listeners { names, rows }
    }

    /// The place's index in the table (the last name equal to `wam`, any
    /// case, as FUN_0047ad30's loop leaves it); `None`: no place sound.
    pub fn place(&self, wam: &str) -> Option<usize> {
        self.names.iter().rposition(|n| n.eq_ignore_ascii_case(wam))
    }

    /// The listener of view `view` of place `place` (the first row);
    /// `None` when the table has none.
    pub fn position(&self, place: usize, view: u32) -> Option<[f32; 3]> {
        self.rows
            .iter()
            .find(|r| r.0 == place as i32 && r.1 == view as i32)
            .map(|r| r.2)
    }
}

/// A slot (@ 0x006732d0 + 0x1c k).
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// +0x00: the sound table entries of the source's files, −1 none.
    pub sounds: [i32; 4],
    /// +0x10: the source (the bruitage record's object), none: free.
    pub source: Option<usize>,
    /// The q of the bruitage record it was taken for.
    pub q: [f32; 3],
    /// +0x14: the file playing, −1 none.
    pub playing: i32,
    /// +0x18: the tick it waits for, 0 when due.
    pub timer: i32,
    /// Its voice started (@ 0x00673154 + 4 (3 + k)).
    pub voice_on: bool,
}

impl Default for Slot {
    fn default() -> Slot {
        Slot {
            sounds: [-1; 4],
            source: None,
            q: [0.0; 3],
            playing: -1,
            timer: 0,
            voice_on: false,
        }
    }
}

/// An entry of the sound table: the file's name and the source whose
/// gains position it (FUN_0047a530 keeps the first loader's).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub owner: usize,
    pub looping: bool,
}

/// The place sounds' state.
#[derive(Clone, Debug, Default)]
pub struct PlaceSounds {
    pub slots: [Slot; SLOTS],
    /// The listener heard from (@ 0x00673188), the view's (@ 0x00673148),
    /// the step of a video (@ 0x006735b8), its frame counter
    /// (@ 0x006733b0) and length (@ 0x00673f28).
    pub listener: [f32; 3],
    pub target: [f32; 3],
    pub step: [f32; 3],
    pub count: f32,
    pub frames: f32,
    /// The sources of the place file loaded, and their ears' gains.
    pub sources: Vec<Source>,
    pub gains: Vec<(i32, i32)>,
    /// The sound table's place-sound entries, by id.
    pub table: Vec<Option<Entry>>,
    pub listeners: Listeners,
}

impl PlaceSounds {
    pub fn new(listeners: Listeners) -> PlaceSounds {
        PlaceSounds {
            table: vec![None; TABLE],
            listeners,
            ..Default::default()
        }
    }

    /// The first source of id `id` (the bruitage's object, FUN_0047ad30).
    pub fn source_of(&self, id: u32) -> Option<usize> {
        self.sources.iter().position(|s| s.id == id)
    }

    /// FUN_0047b650: whether `view` hears `source`.
    pub fn heard_in(&self, view: &View, source: usize) -> bool {
        view.sounds
            .iter()
            .any(|b| self.source_of(b.ambient_id) == Some(source))
    }

    /// Whether another slot than `slot` uses sound `id`.
    pub fn shared(&self, slot: usize, id: i32) -> bool {
        self.slots
            .iter()
            .enumerate()
            .any(|(k, s)| k != slot && s.sounds.contains(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_at_the_listener_is_a_tenth_away() {
        // dist 0 → 0.1: the gain is capped at 255.
        let (g0, g1, _) = gains(
            Fpu::Double,
            [0.0; 3],
            500.0,
            [0.0; 3],
            [0.0, 100.0, 0.0],
            0.0,
            0.0,
        );
        assert_eq!((g0, g1), (255, 255));
    }

    #[test]
    fn silence_pans_right() {
        let att = crate::sound::attenuation_table();
        assert_eq!(
            pan_volume(Fpu::Double, &att, 256, 127, 0, 0, None, false),
            (-10000, 10000)
        );
    }
}
