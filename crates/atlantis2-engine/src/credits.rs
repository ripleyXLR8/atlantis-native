//! The credits (`AtlMenu_start_Credits` @ 0x00486e00): the pages of
//! `loc\Credits.txt` written over 16 pictures that fade one into the next,
//! as the original builds them (FUN_0048b730, FUN_00470550), times them and
//! draws them (FUN_00470d00, FUN_00472bb0). Every rule below was read in the
//! disassembly and checked frame by frame against the original run under
//! emulation (`tests/golden/credits.txt`).
//!
//! **The pages.** `Credits.txt` is read line by line (a CR is dropped, a LF
//! ends the line). Outside a page, empty lines and `;` lines are skipped and
//! `[BeginPage]` opens a page (anything else is a warning). In a page, every
//! line up to `[EndPage]` takes one step down: `Font.txt`'s height in points
//! of `AtlFT_POPUP` (10), from y = 0 at the page's top; an empty line draws
//! nothing. A line starting with `#` has that byte replaced by a space and
//! is centred on x = (W − 405) / 2 + 405 (522 at 640 wide: x = (W − 405) / 2
//! − cx / 2 + 405, cx its `GetTextExtentPoint32A` width with the space, C
//! divisions); any other line starts at x = 405, the builder's argument
//! (@ 0x0048bd52). Each line is drawn as the tooltips are: `AtlFT_POPUP`,
//! black at (x + 1, y + 1), then white at (x, y), on the page's picture
//! (not on the screen), so it fades with it.
//!
//! **The pictures.** Page i is drawn over `f_cred%02d.tga` number i mod 16 + 1
//! (the 16 names of FUN_0048b730), copied into a 24-bit surface (a 1-5-5-5
//! channel c becomes c << 3). There are as many pages as `[BeginPage]`
//! blocks (21 in the shipped file): pictures 1..5 come back for the last
//! five.
//!
//! **The timing** (FUN_00470d00, at each menu frame, before the action of
//! the frame). Two surfaces: the old page and the new one. When the "next"
//! flag is set (at the start, and on the frame after a fade ends), the new
//! page is copied over the old one, the page number goes up, and the new
//! surface gets that page's picture and texts; one past the last page it is
//! cleared to black (the last page fades to black); two past, the credits
//! are done (the action of that same frame is 0x1a). The clock restarts at
//! that frame. Then, t seconds after it: alpha 0 for t < 5, ftol((t − 5) ·
//! 127.5) for t < 7, and 255 at t ≥ 7, which also sets the flag. The time
//! is the performance counter over its frequency, the arithmetic of the x87
//! under the control word of the moment ([`Fpu`]): [`counter_seconds`],
//! on the port's own counter, the host clock in whole milliseconds at the
//! frequency 1000 ([`counter`]; the original's counter and frequency are
//! the machine's).
//!
//! **The fade** (FUN_00472bb0). Each channel of the screen is
//! `T[255 − a][old] + T[a][new]` as a byte, with `T[i][j] = ftol(i · j ·
//! (1/256))` = ⌊i · j / 256⌋ (the 64 KiB table of FUN_00470550, exact under
//! every control word). So a page never shows its own colours: at alpha 0
//! a channel v ≥ 1 becomes v − 1, which drops the 5-bit channels of the
//! pictures by one step. The bytes become RGB565 as `(r & 0xf8) << 8 |
//! (g & 0xf8) << 3 | (b >> 3)`, with two quirks: green keeps only 5 bits
//! (the low green bit is 0), and blue is shifted as a **signed** byte
//! (`sar dl, 3`, then `movsx`): a blue byte of 128 or more adds −32 + its
//! top 5 bits, i.e. takes one from the green field, borrowing from red when
//! green is 0 (black-green pixels with a strong blue turn bright). That is
//! the 565 path (@ 0x00472c5b, flag @ 0x006d85c8 set); the port draws in
//! RGB565 only.

use crate::fpu::{ftol, Fpu};
use crate::hd::{HdTexts, TextFont};
use crate::subtitle::{SubtitleFont, TextOut};

/// The screen the credits are laid out for.
pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
/// The number of pictures (FUN_0048b730).
pub const PICTURES: usize = 16;
/// The left edge the builder passes (`push 0x195` @ 0x0048bd52).
pub const TEXT_X: i32 = 405;
/// The action of the frame once the last page is done (@ 0x00472393).
pub const END: i32 = 0x1a;

/// The picture of page slot `i` (0..16): `f_cred01.tga` .. `f_cred16.tga`.
pub fn picture_name(i: usize) -> String {
    format!("f_cred{:02}.tga", i + 1)
}

/// The lines of the file read as FUN_0046c790 reads them: up to a LF, a CR
/// dropped wherever it is followed by a byte (that byte is kept).
fn lines(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let mut line = Vec::new();
        while at < data.len() {
            let mut c = data[at];
            at += 1;
            if c == b'\r' {
                // The byte after the CR replaces it (0 past the end).
                c = data.get(at).copied().unwrap_or(0);
                at += 1;
            }
            if c == b'\n' {
                break;
            }
            line.push(c);
        }
        out.push(line);
    }
    out
}

/// `loc\Credits.txt` as FUN_00470550 reads it: the pages, each the lines
/// between `[BeginPage]` and `[EndPage]` (empty ones kept: they take their
/// step). A page left open at the end of the file is a fatal error of the
/// original (FUN_00480cd0 @ 0x004709a1); here the page ends there.
pub fn parse(data: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let all = lines(data);
    let mut pages = Vec::new();
    let mut it = all.into_iter();
    while let Some(line) = it.next() {
        if line.is_empty() || line[0] == b';' {
            continue;
        }
        if line != b"[BeginPage]" {
            // FUN_00480c20 @ 0x00470bc6: a warning, the line skipped.
            log::warn!(
                "Credits.txt: {:?} outside a page",
                String::from_utf8_lossy(&line)
            );
            continue;
        }
        let mut page = Vec::new();
        loop {
            match it.next() {
                Some(l) if l == b"[EndPage]" => break,
                Some(l) => page.push(l),
                None => {
                    log::warn!("Credits.txt: a page is not closed");
                    break;
                }
            }
        }
        pages.push(page);
    }
    pages
}

/// The `TextOutA` calls of each page (FUN_00470550 @ 0x0047098d..0x00470b98,
/// the text widget's draw @ 0x0046f0a0): `measure` is the width of a run
/// of bytes in `AtlFT_POPUP`, `step` that font's height in points, `w` the
/// screen width.
pub fn layout(
    pages: &[Vec<Vec<u8>>],
    measure: impl Fn(&[u8]) -> i32,
    step: i32,
    w: i32,
) -> Vec<Vec<TextOut>> {
    pages
        .iter()
        .map(|page| {
            let mut calls = Vec::new();
            for (i, line) in page.iter().enumerate() {
                if line.is_empty() {
                    continue;
                }
                let y = step * i as i32;
                let mut bytes = line.clone();
                // The text widget keeps its string up to its first 0 byte.
                if let Some(z) = bytes.iter().position(|&b| b == 0) {
                    bytes.truncate(z);
                }
                let x = if bytes.first() == Some(&b'#') {
                    bytes[0] = b' ';
                    (w - TEXT_X) / 2 - measure(&bytes) / 2 + TEXT_X
                } else {
                    TEXT_X
                };
                calls.push(TextOut {
                    x: x + 1,
                    y: y + 1,
                    colour: 0,
                    bytes: bytes.clone(),
                });
                calls.push(TextOut {
                    x,
                    y,
                    colour: 0xff_ffff,
                    bytes,
                });
            }
            calls
        })
        .collect()
}

/// The frequency of the port's counter: milliseconds ([`counter`]).
pub const COUNTER_FREQUENCY: f64 = 1000.0;

/// The port's counter at the host clock `seconds`: whole milliseconds.
pub fn counter(seconds: f64) -> u64 {
    (seconds * COUNTER_FREQUENCY) as u64
}

/// FUN_004c3440: the 64-bit `counter` over `frequency`, as the x87 computes
/// it under `fpu` (@ 0x004c3467..0x004c3482): the high dword loaded as a
/// signed integer times 4294967295.0 (the double @ 0x005c67c0, not 2³²),
/// plus the low dword as an unsigned one, over the frequency; the loads
/// are exact, the product, the sum and the quotient round.
pub fn counter_seconds(fpu: Fpu, counter: u64, frequency: f64) -> f64 {
    let hi = f64::from((counter >> 32) as u32 as i32);
    let lo = (counter & 0xffff_ffff) as f64;
    fpu.div(fpu.add(fpu.mul(hi, 4294967295.0), lo), frequency)
}

/// The fade's weight at `now` for a page started at `start` (seconds, as
/// FUN_004c3440 gives them), and whether the fade is over (the "next"
/// flag): @ 0x00470ef2..0x00470f3e.
pub fn alpha(fpu: Fpu, now: f64, start: f64) -> (u8, bool) {
    let t = fpu.sub(now, start);
    if t < 5.0 {
        (0, false)
    } else if t < 7.0 {
        (ftol(fpu.mul(fpu.sub(t, 5.0), 127.5)) as u8, false)
    } else {
        (255, true)
    }
}

/// FUN_00472bb0 on a 565 screen: `old` and `new` (B, G, R bytes) mixed
/// with weight `a` into `out` (see the module's notes for the quirks).
pub fn blend(old: &[u8], new: &[u8], a: u8, out: &mut [u16]) {
    let (wo, wn) = (u32::from(255 - a), u32::from(a));
    let (old, _) = old.as_chunks::<3>();
    let (new, _) = new.as_chunks::<3>();
    for ((o, n), px) in old.iter().zip(new).zip(out.iter_mut()) {
        // T[i][j] = ⌊i · j / 256⌋, the bytes added as bytes (the sum stays
        // below 255: ⌊(255 − a) o / 256⌋ + ⌊a n / 256⌋ ≤ 254).
        let ch = |k: usize| {
            (((wo * u32::from(o[k])) >> 8) as u8).wrapping_add(((wn * u32::from(n[k])) >> 8) as u8)
        };
        let (b, g, r) = (ch(0), ch(1), ch(2));
        let blue = i32::from((b as i8) >> 3);
        let rg = (i32::from(r & 0xf8) << 8) + (i32::from(g & 0xf8) << 3);
        *px = (blue + rg) as u16;
    }
}

/// What a page's texts set on its picture: (pixel index, B, G, R), in the
/// order drawn.
type Overlay = Vec<(usize, [u8; 3])>;

/// The credits' two page surfaces and their timing (the `CMnMenu` fields
/// +0xac / +0x12c, page +0x1bc, clock +0x1c0, "next" +0x1d0, done +0xa9).
#[derive(Clone, Debug)]
pub struct Credits {
    /// The 16 pictures, 640 x 480 B, G, R (black when missing).
    pictures: Vec<Vec<u8>>,
    /// Each page's texts on its picture, and its `TextOutA` calls (the HD
    /// mode draws them again).
    overlays: Vec<Overlay>,
    calls: Vec<Vec<TextOut>>,
    old: Vec<u8>,
    new: Vec<u8>,
    /// The page on the new surface; past the pages: black, then done.
    pub page: usize,
    /// The fade is over: the next frame moves to the next page.
    pub next: bool,
    /// The clock when the page started.
    pub start: f64,
    /// The fade's weight of the last frame.
    pub alpha: u8,
    /// The pages are done (the action 0x1a).
    pub done: bool,
}

impl Credits {
    /// FUN_00470550: `pictures` (each 640 x 480 x 3 bytes; a wrong size is
    /// black), the `TextOutA` calls of each page drawn with `font` (none:
    /// no texts). Page 0 is on the new surface, the flag set.
    pub fn new(
        pictures: Vec<Vec<u8>>,
        pages: &[Vec<TextOut>],
        font: Option<&SubtitleFont>,
    ) -> Credits {
        let size = WIDTH * HEIGHT * 3;
        let mut pictures: Vec<Vec<u8>> = pictures
            .into_iter()
            .map(|p| if p.len() == size { p } else { vec![0; size] })
            .collect();
        pictures.resize(PICTURES, vec![0; size]);
        let overlays = pages
            .iter()
            .map(|calls| {
                let mut o = Vec::new();
                if let Some(f) = font {
                    f.draw_with(calls, |x, y, c| {
                        if x >= 0 && y >= 0 && (x as usize) < WIDTH && (y as usize) < HEIGHT {
                            o.push((
                                y as usize * WIDTH + x as usize,
                                [(c >> 16) as u8, (c >> 8) as u8, c as u8],
                            ));
                        }
                    });
                }
                o
            })
            .collect();
        let mut c = Credits {
            pictures,
            overlays,
            calls: if font.is_some() {
                pages.to_vec()
            } else {
                vec![Vec::new(); pages.len()]
            },
            old: vec![0; size],
            new: vec![0; size],
            page: 0,
            next: true,
            start: 0.0,
            alpha: 0,
            done: false,
        };
        c.load(0);
        c
    }

    /// The number of pages.
    pub fn pages(&self) -> usize {
        self.overlays.len()
    }

    /// Page `i`'s picture and texts on the new surface (FUN_00472830, then
    /// FUN_00472b70).
    fn load(&mut self, i: usize) {
        if i >= self.overlays.len() {
            return;
        }
        self.new.copy_from_slice(&self.pictures[i % PICTURES]);
        for &(at, bgr) in &self.overlays[i] {
            self.new[3 * at..3 * at + 3].copy_from_slice(&bgr);
        }
    }

    /// FUN_00470d00 at the port's counter `counter` ([`counter`]), before
    /// the draw: the clock is FUN_004c3440's quotient under `fpu`.
    pub fn update(&mut self, fpu: Fpu, counter: u64) {
        let now = counter_seconds(fpu, counter, COUNTER_FREQUENCY);
        if self.next {
            self.old.copy_from_slice(&self.new);
            self.page += 1;
            let n = self.overlays.len();
            if self.page < n {
                self.load(self.page);
            } else if self.page == n {
                self.new.fill(0);
            } else {
                self.done = true;
            }
            self.next = false;
            self.start = now;
        }
        let (a, over) = alpha(fpu, now, self.start);
        self.alpha = a;
        if over {
            self.next = true;
        }
    }

    /// The frame (FUN_00472bb0 into the 640 x 480 screen).
    pub fn draw(&self, out: &mut [u16]) {
        blend(&self.old, &self.new, self.alpha, out);
    }

    /// The HD mode (a port addition): [`Credits::draw`], and the texts
    /// recorded in `texts` (when it records) for the front end to draw
    /// again at its size: the old page's calls at the weight 255 − alpha,
    /// the new page's at alpha (the fade's), over the frame the same
    /// pictures give without their texts.
    pub fn draw_hd(&self, out: &mut [u16], texts: &mut HdTexts) {
        self.draw(out);
        if !texts.is_on() || out.len() != WIDTH * HEIGHT {
            return;
        }
        // The old surface holds page − 1, the new one page (black past
        // the last page).
        let n = self.overlays.len();
        let plain = |i: Option<usize>| i.filter(|&i| i < n).map(|i| &self.pictures[i % PICTURES]);
        let black = vec![0u8; WIDTH * HEIGHT * 3];
        let old = self.page.checked_sub(1);
        let new = Some(self.page);
        let mut under = vec![0u16; WIDTH * HEIGHT];
        blend(
            plain(old).unwrap_or(&black),
            plain(new).unwrap_or(&black),
            self.alpha,
            &mut under,
        );
        texts.set_pixels(&under, out);
        for (page, alpha) in [(old, 255 - self.alpha), (new, self.alpha)] {
            if let Some(calls) = page.and_then(|i| self.calls.get(i)) {
                texts.push(TextFont::Popup, calls, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_pages() {
        let f =
            b"; c\r\n\r\n[BeginPage]\r\n\r\n#A\r\nb\r\n[EndPage]\r\nx\r\n[BeginPage]\n[EndPage]";
        let p = parse(f);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], vec![b"".to_vec(), b"#A".to_vec(), b"b".to_vec()]);
        assert!(p[1].is_empty());
        let l = layout(&p, |b| 10 * b.len() as i32, 10, 640);
        // " A": 20 wide, centred on 522; "b" at 405; one step down each.
        assert_eq!((l[0][1].x, l[0][1].y, l[0][1].colour), (512, 10, 0xffffff));
        assert_eq!(l[0][1].bytes, b" A");
        assert_eq!((l[0][0].x, l[0][0].y, l[0][0].colour), (513, 11, 0));
        assert_eq!((l[0][3].x, l[0][3].y), (405, 20));
    }

    #[test]
    fn alpha_and_end() {
        let f = Fpu::Double;
        assert_eq!(alpha(f, 4.999, 0.0), (0, false));
        assert_eq!(alpha(f, 6.0, 0.0), (127, false));
        assert_eq!(alpha(f, 7.0, 0.0), (255, true));
        let pic = vec![vec![8u8; WIDTH * HEIGHT * 3]; 16];
        let mut c = Credits::new(pic, &[vec![], vec![]], None);
        let mut ms = 0;
        let mut frames = 0;
        while !c.done {
            c.update(f, ms);
            ms += 500;
            frames += 1;
        }
        // Page 1 at 0 s, black at 7.5 s (page 1 fades to it), done at 15 s.
        assert_eq!(c.page, 3);
        assert_eq!(frames, 31);
    }

    #[test]
    fn counter_quotient() {
        // 2.5 s at 1 kHz; above 2^32 counts the high dword weighs
        // 4294967295, not 2^32 (@ 0x005c67c0).
        assert_eq!(counter_seconds(Fpu::Double, 2500, 1000.0), 2.5);
        assert_eq!(counter_seconds(Fpu::Double, 1 << 32, 1.0), 4294967295.0);
        // Under 0x007f the quotient keeps 24 bits.
        assert_eq!(
            counter_seconds(Fpu::Single, 1001, 1000.0),
            f64::from(1.001f32)
        );
        assert_eq!(counter(2.5), 2500);
    }

    #[test]
    fn blend_quirks() {
        let mut out = [0u16; 1];
        // Alpha 0: each channel v becomes ⌊255 v / 256⌋.
        blend(&[0x08, 0x08, 0x08], &[0; 3], 0, &mut out);
        assert_eq!(out[0], 0);
        blend(&[0x10, 0x10, 0x10], &[0; 3], 0, &mut out);
        assert_eq!(out[0], (1 << 11) | (1 << 6) | 1);
        // A blue byte of 128 or more borrows from green, then red.
        blend(&[0xff, 0x00, 0x00], &[0xff, 0, 0], 255, &mut out);
        // T[0][255] + T[255][255] = 254: −32 + 31 = −1.
        assert_eq!(out[0], 0xffff);
    }
}
