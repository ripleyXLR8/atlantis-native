//! The credits (`FUN_0041a72c`) as a list of events.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`credits_layout.h`;
//! GPLv3, the ScummVM team). The bytes of `SPRLIST\CREDITS.TXT` are walked
//! one at a time from offset 3 (past the leading `$01`); the text is drawn
//! into a work buffer over the picture, which reaches the screen only at
//! the next `$NN` marker, after the page time. Ticks are the 200 Hz
//! timer's; the character loop takes no time.

/// One event of the credits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditsEvent {
    /// `images\creditNN.tga` into the work buffer (`FUN_0041ad7c`).
    Image(i32),
    /// A glyph: CREDBLEU.SPR when blue, else CREDBLAN.SPR, frame `c - 0x21`
    /// at (x, y) (`FUN_00410430`).
    Glyph {
        blue: bool,
        x: i32,
        y: i32,
        frame: i32,
    },
    /// The work buffer presented at the tick.
    Present(u32),
    /// The end, at the tick.
    Clear(u32),
}

/// The page time (0x556 ticks).
pub const PAGE_TICKS: u32 = 0x556;

fn atoi(t: &[u8], mut p: usize) -> i32 {
    while p < t.len() && (t[p] == b' ' || t[p] == b'\t') {
        p += 1;
    }
    let mut neg = false;
    if p < t.len() && (t[p] == b'-' || t[p] == b'+') {
        neg = t[p] == b'-';
        p += 1;
    }
    let mut n = 0i32;
    while p < t.len() && t[p].is_ascii_digit() {
        n = n * 10 + i32::from(t[p] - b'0');
        p += 1;
    }
    if neg {
        -n
    } else {
        n
    }
}

/// `FUN_0042073c`: the line from `p` to the next CR centred on x = 480.
fn centre(t: &[u8], mut p: usize, widths: &[u8; 256]) -> i32 {
    let mut w = 0i32;
    while p < t.len() && t[p] != b'\r' {
        w += if t[p] == b' ' {
            6
        } else {
            i32::from(widths[usize::from(t[p])])
        };
        p += 1;
    }
    (0x3c0 - w) / 2
}

/// The events of the credits text `t`; `widths` is the byte table
/// 0x004966fc.
pub fn credits_events(t: &[u8], widths: &[u8; 256]) -> Vec<CreditsEvent> {
    let mut ev = vec![CreditsEvent::Image(1)];
    let mut tick: u32 = 0;
    let mut timer: u32 = tick.wrapping_sub(0x53c); // 0x006cd810
    let (mut x, mut y) = (0x140i32, 0i32);
    let mut blue = false;
    let mut p = 3usize;
    while p < t.len() {
        let mut c = t[p];
        let mut draw = true;
        if c == b'<' {
            p += 2;
            if p < t.len() && t[p] == b'\\' {
                p += 1;
                blue = true;
            }
            x = centre(t, p, widths);
            c = t.get(p).copied().unwrap_or(0);
        }
        if c == b'\r' {
            blue = false;
            y += 16;
            x = 0x140;
            p += 1; // the LF
            draw = false;
        } else if c == b'\\' {
            x -= i32::from(widths[0x5c]);
            blue = true;
            draw = false;
        } else if c == b'$' {
            x = 0x140;
            y = 0;
            if tick.wrapping_sub(timer) < PAGE_TICKS {
                tick = timer.wrapping_add(PAGE_TICKS);
            }
            timer = tick;
            ev.push(CreditsEvent::Present(tick));
            ev.push(CreditsEvent::Image(atoi(t, p + 1)));
            p += 2;
            draw = false;
        }
        if draw && c != b' ' {
            ev.push(CreditsEvent::Glyph {
                blue,
                x,
                y,
                frame: i32::from(c) - 0x21,
            });
            x += i32::from(widths[usize::from(c)]);
        } else {
            x += 6;
        }
        p += 1;
    }
    if tick.wrapping_sub(timer) < PAGE_TICKS {
        tick = timer.wrapping_add(PAGE_TICKS);
    }
    ev.push(CreditsEvent::Clear(tick));
    ev
}

impl std::fmt::Display for CreditsEvent {
    /// The line form of the original's emulated trace.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            CreditsEvent::Image(n) => write!(f, "image {n}"),
            CreditsEvent::Glyph { blue, x, y, frame } => {
                write!(f, "glyph {} {x} {y} {frame}", if blue { 'B' } else { 'W' })
            }
            CreditsEvent::Present(t) => write!(f, "present at tick {t}"),
            CreditsEvent::Clear(t) => write!(f, "clear at tick {t}"),
        }
    }
}
