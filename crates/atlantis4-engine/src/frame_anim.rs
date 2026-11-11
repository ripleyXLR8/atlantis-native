//! `TeFrameAnim::update` (atlantis4-gog-en-2363910 @ 0x0056e850): the
//! frame of an animation at a time.
//!
//! `first' = min(first, N)`, `end = min(first' + count, N)` (unsigned),
//! `len = end − first'`, `k = trunc((now − start) / 1000 · fps)` (x87
//! truncation, low 32 bits); `len ≠ 0`: `loop = k / len`, `frame = k mod
//! len + first'`; `len = 0`: `loop = 0xffffffff`, `frame = 0`. When `loops
//! ≠ −1` and `loop ≥ loops` (unsigned) the animation stops (if playing) and
//! signals "finished", the frame unchanged; else `reverse` gives `N − 1 −
//! frame` and a new frame signals "frame changed".
//!
//! `N` and `fps` are the tile codec's (Theora: the stream's; JPEG: 1 frame
//! at 0 fps), not the XML's `fps` (@ 0x004f2661..0x004f2696).

/// A frame animation's state (the fields at `+0x10..+0x58` of `TeFrameAnim`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameAnim {
    pub playing: bool,
    /// Start, in milliseconds of the engine clock.
    pub start_ms: f64,
    /// −1: forever.
    pub loops: i32,
    pub reverse: bool,
    /// Frame count.
    pub n: u32,
    pub fps: f32,
    pub first: u32,
    pub count: u32,
    /// The current frame, −1 before the first update.
    pub frame: i32,
}

/// What an update signalled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events {
    /// The frame changed.
    pub changed: bool,
    /// It stopped (the stop call and its "stopped" signal, when playing).
    pub stopped: bool,
    /// It played its loops.
    pub finished: bool,
}

impl FrameAnim {
    /// `0x0056e850` at `now_ms`.
    pub fn update(&mut self, now_ms: f64) -> Events {
        let mut ev = Events::default();
        let first = self.first.min(self.n);
        let end = first.wrapping_add(self.count).min(self.n);
        let len = end.wrapping_sub(first);
        let k = ((now_ms - self.start_ms) / 1000.0 * f64::from(self.fps)).trunc() as i64 as u32;
        let (lp, frame) = match k.checked_div(len) {
            Some(lp) => (lp, k % len + first),
            None => (u32::MAX, 0),
        };
        if self.loops != -1 && lp >= self.loops as u32 {
            if self.playing {
                self.playing = false;
                ev.stopped = true;
            }
            ev.finished = true;
            return ev;
        }
        let frame = if self.reverse {
            self.n.wrapping_sub(frame).wrapping_sub(1)
        } else {
            frame
        } as i32;
        if frame != self.frame {
            self.frame = frame;
            ev.changed = true;
        }
        ev
    }
}
