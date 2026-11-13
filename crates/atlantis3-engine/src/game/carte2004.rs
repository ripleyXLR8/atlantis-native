//! *Atlantis Evolution*'s 2004 program's Gameboy (atlantis4-cd-en-3022848):
//! *Atlantis III*'s `AtlCarte` module given a new content, loop mode 10.
//!
//! The per-frame dispatcher FUN_00476320 (the panorama @ 0x00443db6, the
//! inventory @ 0x00428ed1, the mini-games @ 0x004a15b4) opens it when the
//! object in the hand is `GAMEBOY` (FUN_00409970, `AtlCarte_Start`) and
//! empties the hand (FUN_00429a20). Its frame (FUN_00409ed0, @ 0x004a159e)
//! draws `spr\gameboy.spr` #0 over the running panorama in the 800 × 600
//! stretch surface, the six buttons with their hover sprites
//! (FUN_00409c20), and a god's Bink clip blended into the Gameboy's screen;
//! a god button starts that clip and a light voice-over dialog
//! (FUN_004094e0); the exit button or the right button closes it
//! (FUN_00409a80, `AtlCarte_Close`: the mode back, `CtrlScenarIN`).
//!
//! The RE note is atlantis4-re `subsystems/cd2004-carte.md`. Not as the
//! original: `BinkWait`'s pacing (1 / fps from the open, as the dialog
//! heads); the panorama drawn at the screen's size and sampled into the
//! 800 × 600 surfaces (the original renders it at 800 × 600: exact when the
//! screen is 800 × 600).

use std::sync::Arc;

use atlantis2_data::defnames;
use atlantis3_data::bink::BinkVideo;
use atlantis3_data::spr::{Bank, Target};

use super::dialog2004::{blend_565, blend_8888};
use super::{stretch_video, Game, Mode};
use crate::px::Surface;

/// The stretch surface's size (`AtlDisplay_OpenStretchSurface(800, 600)`
/// @ 0x004099d0), and so the composition's and the clip's (FUN_00411a30 /
/// FUN_00411a50 once it is open).
pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 600;

/// The buttons in the order FUN_00409c20 draws them: sprite k, or k + 1
/// when the pointer is on k (`SPR_COLLISION` at the sprite's own place).
pub const HOVER_ORDER: [usize; 6] = [1, 5, 9, 11, 3, 7];

/// The click tests in their order (@ 0x0040a27c..0x0040a3b0): sprite and
/// clip number (FUN_004094e0), `None` for the exit (#7: return 0, Close).
pub const CLICK_ORDER: [(usize, Option<i32>); 6] = [
    (1, Some(1)),
    (3, Some(2)),
    (5, Some(3)),
    (9, Some(4)),
    (11, Some(5)),
    (7, None),
];

/// FUN_004094e0's cases 1..5: the clip (`bink\`, the current disc) and the
/// dialog started with it (`AtlDial_Start(d, 1, 1, 0x28)`).
pub const CLIPS: [(&str, i32); 5] = [
    ("clipcosmo.bik", 0x79),
    ("clipenna.bik", 0x78),
    ("clipkama.bik", 0x7a),
    ("clipsama.bik", 0x7b),
    ("clipzanat.bik", 0x77),
];

/// The button a click at `p` hits (the first of [`CLICK_ORDER`]): `Some(Some(n))`
/// a clip, `Some(None)` the exit, `None` nothing.
pub fn click_hit(bank: &Bank, p: (i32, i32)) -> Option<Option<i32>> {
    CLICK_ORDER
        .iter()
        .find(|(k, _)| bank.opaque_at(*k, None, 0, p.0, p.1))
        .map(|&(_, n)| n)
}

/// The sprites FUN_00409c20 draws for the pointer at `p`, in order.
pub fn buttons_drawn(bank: &Bank, p: (i32, i32)) -> [usize; 6] {
    HOVER_ORDER.map(|k| {
        if bank.opaque_at(k, None, 0, p.0, p.1) {
            k + 1
        } else {
            k
        }
    })
}

/// The 32-bit blend of the clip into the composition (FUN_00666b07 called
/// @ 0x0040a05d with source and destination at (100, 145), W − 100 pixels a
/// row, H − 245 rows, the destination's stride argument pitch − 100): the
/// routine moves the source on by the row's pixels only and the destination
/// by the stride argument, so with pitch = W both run through 700 × 355
/// consecutive pixels — a linear run from (100, 145), not a rectangle.
pub fn blend_clip_8888(dst: &mut [u32], src: &[u32], w: usize, h: usize) {
    // The surfaces' pitch is their width [INFERRED].
    let pitch = w;
    let (count, rows, stride) = (w - 100, h - 245, pitch - 100);
    // Pairs of pixels (`shr ebx, 1`); the destination moves on by
    // (stride − count) more after each row (`sub edx, [ebp+0x10]`).
    let n = count & !1;
    let skip = stride - count;
    let (mut s, mut d) = (145 * pitch + 100, 145 * pitch + 100);
    for _ in 0..rows {
        for _ in 0..n {
            if let (Some(o), Some(&c)) = (dst.get_mut(d), src.get(s)) {
                blend_8888(o, c);
            }
            s += 1;
            d += 1;
        }
        d += skip;
    }
}

/// The 16-bit blend (@ 0x0040a06a..0x0040a202): the rectangle x 100 ..
/// 100 + (W − 410), y 145 .. 145 + (H − 285), the dialog heads' RGB565
/// blend (alpha 0 left alone).
pub fn blend_clip_565(dst: &mut [u16], src: &[u32], w: usize, h: usize) {
    for y in 145..145 + (h - 285) {
        for x in 100..100 + (w - 410) {
            blend_565(&mut dst[y * w + x], src[y * w + x]);
        }
    }
}

/// A god's clip (0x006e03ec, `BinkOpen(path, 0x100000)`).
pub(crate) struct Clip {
    /// FUN_004094e0's number (0x006e03dc, no reader).
    pub n: i32,
    video: BinkVideo<Arc<[u8]>>,
    /// When the next frame is due (`BinkWait`, [INFERRED]: 1 / fps from
    /// the open, as the dialog heads).
    due: f64,
    /// The clip surface 0x006e02a8 (a8r8g8b8, `BinkCopyToBuffer(.., 5)`):
    /// the frame centred in W × H.
    argb: Vec<u32>,
    /// `FrameNum` of the frame decoded last.
    pub frame_num: u32,
    /// 0x006e03f0: frozen on `Frames` − 5.
    pub ended: bool,
}

impl Clip {
    fn open(n: i32, data: Vec<u8>, now: f64) -> Option<Clip> {
        let video = BinkVideo::open(Arc::<[u8]>::from(data)).ok()?;
        Some(Clip {
            n,
            video,
            due: now,
            argb: vec![0; WIDTH * HEIGHT],
            frame_num: 0,
            ended: false,
        })
    }

    /// `BinkDoFrame` and `BinkCopyToBuffer` centred into the W × H clip
    /// surface.
    fn do_frame(&mut self) {
        let (bw, bh) = (self.video.width() as usize, self.video.height() as usize);
        let mut frame = vec![0u32; bw * bh];
        match self.video.next_frame() {
            Some(Ok(f)) => f.to_argb8888(&mut frame),
            Some(Err(e)) => {
                log::warn!("gameboy clip {}: {e:?}", self.n);
                return;
            }
            None => return,
        }
        self.frame_num = self.video.frames_decoded();
        let (x0, y0) = (WIDTH.saturating_sub(bw) / 2, HEIGHT.saturating_sub(bh) / 2);
        let cw = bw.min(WIDTH);
        for y in 0..bh.min(HEIGHT) {
            self.argb[(y0 + y) * WIDTH + x0..][..cw].copy_from_slice(&frame[y * bw..][..cw]);
        }
    }

    /// `BinkNextFrame`.
    fn next(&mut self, now: f64) {
        let fps = self.video.fps();
        self.due += if fps > 0.0 { 1.0 / fps } else { 1.0 / 15.0 };
        if self.due < now {
            self.due = now;
        }
    }
}

/// The Gameboy's globals.
#[derive(Default)]
pub(crate) struct State {
    /// 0x006e03d4.
    pub open: bool,
    /// 0x006e03d0: the mode before.
    back: Option<Mode>,
    /// `gameboy.spr` (0x006e0330).
    bank: Option<Arc<Bank>>,
    /// 0x006e0350: the composition while a clip is open (display format).
    comp: Option<Surface>,
    /// The clip (0x006e03ec, 0x006e03f0).
    pub clip: Option<Clip>,
}

impl Game {
    /// FUN_00476320's part after the light dialog (@ 0x00476380..
    /// 0x004763b2): `OBJETENMAIN` = `GAMEBOY` → `AtlCarte_Start`
    /// (FUN_00409970), then the hand emptied (FUN_00429a20).
    pub(crate) fn carte_hand_2004(&mut self) {
        let Ok(gameboy) = self.names.id(defnames::OBJETS, b"GAMEBOY") else {
            return;
        };
        if self.object_in_hand() != gameboy as i32 {
            return;
        }
        self.carte_start_2004();
        if let Some(s) = self.script.as_mut() {
            s.empty_hand();
        }
    }

    /// `AtlCarte_Start` (FUN_00409970).
    fn carte_start_2004(&mut self) {
        if self.mode == Mode::Carte2004 {
            log::warn!("AtlCarte_Start(): deja en AtlMain_loop_CARTE!!");
            return;
        }
        if self.carte2004.open {
            log::warn!("AtlCarte_Open() : carte deja ouverte.");
            return;
        }
        let depth = self.depth();
        // `AtlDisplay_OpenStretchSurface(800, 600)` (FUN_00412c40): the
        // program's one stretch surface, the mini-games'.
        if self.puzzles2004.surface.is_none() {
            self.puzzles2004.surface = Some(Surface::new(depth, WIDTH * HEIGHT));
        } else {
            log::warn!("AtlDisplay_OpenStretchSurface : la surface est deja ouverte");
        }
        // FUN_004a2620(10): the old mode kept.
        self.carte2004.back = Some(self.mode);
        self.mode = Mode::Carte2004;
        // `AtlCarte_ReloadSprBank` (FUN_0040a3f0): `spr\gameboy.spr`.
        self.carte2004.bank = self
            .files
            .read("spr/gameboy.spr")
            .and_then(|d| Bank::parse(&d, depth).ok())
            .map(Arc::new);
        if self.carte2004.bank.is_none() {
            log::warn!(
                "AtlCarte_ReloadSprBank() : pb ouverture spr de la carte (spr\\gameboy.spr)."
            );
        }
        // FUN_0042e390: the turn speeds 0, the pointer at the stretch
        // surface's centre; the cursor #29 (FUN_00429160 / FUN_00429180).
        self.look.stop();
        self.look_last = None;
        self.pointer = ((WIDTH / 2) as i32, (HEIGHT / 2) as i32);
        self.carte2004.open = true;
        // The composition surface (FUN_004b3210 @ 0x00409a3e); the clip
        // surface is the clip's.
        self.carte2004.comp = Some(Surface::new(depth, WIDTH * HEIGHT));
        log::info!("AtlCarte_Start");
    }

    /// FUN_004094e0(n): the clip `n` (1..5) and its dialog.
    fn carte_clip_2004(&mut self, n: i32) {
        // An open clip closed, the ended byte cleared.
        self.carte2004.clip = None;
        let Some(&(file, dialog)) = usize::try_from(n - 1).ok().and_then(|i| CLIPS.get(i)) else {
            return;
        };
        self.carte2004.clip = self
            .files
            .read(&format!("bink/{file}"))
            .and_then(|d| Clip::open(n, d, self.now));
        // A light dialog closed, then `AtlDial_Start(d, 1, 1, 0x28)`.
        if self.dialog2004.as_ref().is_some_and(|d| d.light) {
            self.close_dialog_2004();
        }
        self.start_dialog_2004(dialog, true, true, 0x28);
        if self.carte2004.clip.is_none() {
            log::warn!("Gameboy : Bik introuvable : {n}");
        }
        self.tr(|| format!("gameboy clip {n} {file} dial {dialog}"));
    }

    /// One pass of loop mode 10 (@ 0x004a159e): the frame FUN_00409ed0,
    /// `AtlCarte_Close` when it returns 0.
    pub(crate) fn carte_frame_2004(&mut self) {
        if !self.carte_continue_2004() {
            self.carte_close_2004();
        }
    }

    /// FUN_00409ed0; false: the caller closes.
    fn carte_continue_2004(&mut self) -> bool {
        if self.carte2004.open {
            // The light dialog (@ 0x00409ee6..0x00409ef7).
            self.light_dialog_2004();
            self.carte_draw_2004();
        }
        let p = self.pointer;
        if self.click {
            let hit = self.carte2004.bank.as_deref().and_then(|b| click_hit(b, p));
            self.carte_clear_clicks();
            return match hit {
                Some(Some(n)) => {
                    self.carte_clip_2004(n);
                    true
                }
                Some(None) => false,
                None => true,
            };
        }
        if self.right_clicked {
            self.right_clicked = false;
            return false;
        }
        true
    }

    /// Both click bytes cleared (0x006e9fe8, 0x006e9fe9; the script reads
    /// the same left byte).
    fn carte_clear_clicks(&mut self) {
        self.click = false;
        self.right_clicked = false;
        if let Some(s) = self.script.as_mut() {
            s.click = false;
        }
    }

    /// The drawing part of FUN_00409ed0 (@ 0x00409efc..0x0040a26c).
    fn carte_draw_2004(&mut self) {
        let depth = self.depth();
        let mut stretch = self
            .puzzles2004
            .surface
            .take()
            .unwrap_or_else(|| Surface::new(depth, WIDTH * HEIGHT));
        let mut comp = self
            .carte2004
            .comp
            .take()
            .unwrap_or_else(|| Surface::new(depth, WIDTH * HEIGHT));
        let clip_open = self.carte2004.clip.is_some();
        // FUN_00444670(dest): the panorama into the stretch surface, or into
        // the composition while a clip is open; the sprites follow it there
        // (FUN_00436670(dest) @ 0x00444802).
        {
            let dest = if clip_open { &mut comp } else { &mut stretch };
            self.carte_warp_into(dest);
            if let Some(bank) = self.carte2004.bank.clone() {
                let buttons = buttons_drawn(&bank, self.pointer);
                crate::with_px!(dest, s => {
                    let mut t = Target::screen(s.as_mut_slice(), WIDTH, HEIGHT);
                    // `AtlSpr_Aff(gameboy, 0, 0x80000000, 0, 1.0)`.
                    bank.draw(&mut t, None, 0, 0);
                    for k in buttons {
                        bank.draw(&mut t, None, 0, k);
                    }
                });
                self.tr(|| format!("gameboy draw 0 {buttons:?}"));
            }
        }
        // The clip (@ 0x00409f64..0x0040a247).
        let now = self.now;
        let mut copied = false;
        if let Some(c) = self.carte2004.clip.as_mut() {
            let wait = now < c.due;
            if !(wait && !c.ended) {
                if !wait && !c.ended {
                    c.do_frame();
                }
                match &mut comp {
                    Surface::Xrgb8888(d) => blend_clip_8888(d, &c.argb, WIDTH, HEIGHT),
                    Surface::Rgb565(d) => blend_clip_565(d, &c.argb, WIDTH, HEIGHT),
                }
                // FUN_004b5120(stretch, composition): the whole stretch.
                match (&mut stretch, &comp) {
                    (Surface::Xrgb8888(o), Surface::Xrgb8888(i)) => o.copy_from_slice(i),
                    (Surface::Rgb565(o), Surface::Rgb565(i)) => o.copy_from_slice(i),
                    _ => {}
                }
                copied = true;
                if c.frame_num + 5 == c.video.frame_count() {
                    c.ended = true;
                } else if !c.ended {
                    c.next(now);
                }
            }
        }
        if copied {
            self.tr(|| "gameboy clip copied".to_string());
        }
        // The pointer (FUN_00429460) on the stretch surface, then the flip
        // FUN_004127a0(0): the stretch to the screen, then the texts.
        let (px, py) = self.pointer;
        if let Some(bank) = self.interface.as_ref() {
            let frame = super::pointer_frame(true);
            crate::with_px!(&mut stretch, s => {
                crate::inventory::aff(s.as_mut_slice(), WIDTH, HEIGHT, "interface", bank, frame, px, py, 1.0);
            });
        }
        let (sw, sh) = (self.width, self.height);
        match (&stretch, &mut self.screen) {
            (Surface::Rgb565(s), Surface::Rgb565(o)) => stretch_video(s, WIDTH, HEIGHT, o, sw, sh),
            (Surface::Xrgb8888(s), Surface::Xrgb8888(o)) => {
                stretch_video(s, WIDTH, HEIGHT, o, sw, sh)
            }
            _ => {}
        }
        self.puzzles2004.surface = Some(stretch);
        self.carte2004.comp = Some(comp);
        self.flip_texts();
    }

    /// FUN_00444670 into an 800 × 600 surface: the panorama's frame drawn
    /// on the screen, then sampled into `dest` (a copy at 800 × 600).
    fn carte_warp_into(&mut self, dest: &mut Surface) {
        if !self.draw_warp_only() {
            return;
        }
        let (sw, sh) = (self.width, self.height);
        fn sample<P: Copy>(src: &[P], sw: usize, sh: usize, out: &mut [P]) {
            if sw == WIDTH && sh == HEIGHT {
                out.copy_from_slice(&src[..WIDTH * HEIGHT]);
                return;
            }
            for y in 0..HEIGHT {
                let row = &src[(y * sh / HEIGHT) * sw..][..sw];
                for x in 0..WIDTH {
                    out[y * WIDTH + x] = row[x * sw / WIDTH];
                }
            }
        }
        match (&self.screen, dest) {
            (Surface::Rgb565(s), Surface::Rgb565(o)) => sample(s, sw, sh, o),
            (Surface::Xrgb8888(s), Surface::Xrgb8888(o)) => sample(s, sw, sh, o),
            _ => {}
        }
    }

    /// `AtlCarte_Close` (FUN_00409a80).
    fn carte_close_2004(&mut self) {
        if !self.carte2004.open {
            log::warn!("AtlCarte_Close() : Carte deja fermee.");
            return;
        }
        // The bank freed, the stretch surface closed (FUN_00412db0).
        self.carte2004.bank = None;
        self.puzzles2004.surface = None;
        // A light dialog: its line's voice removed and `AtlDial_Close`.
        if self.dialog2004.as_ref().is_some_and(|d| d.light) {
            self.close_dialog_2004();
        }
        // The mode back, the turn speeds 0 (FUN_0042e370), closed, both
        // click bytes 0, `CtrlScenarIN` (FUN_00450d30).
        self.mode = self.carte2004.back.take().unwrap_or(Mode::Warp);
        self.look.stop();
        self.look_last = None;
        self.carte2004.open = false;
        self.carte_clear_clicks();
        self.with_script(|s, h| {
            s.ctrl_scenar_in(h);
        });
        // The two surfaces freed, the clip `BinkClose`d.
        self.carte2004.comp = None;
        self.carte2004.clip = None;
        log::info!("AtlCarte_Close");
    }

    /// Tests: whether the Gameboy is open.
    #[doc(hidden)]
    pub fn carte_2004_open(&self) -> bool {
        self.carte2004.open
    }

    /// Tests: the Gameboy's clip (its number, `FrameNum`, ended).
    #[doc(hidden)]
    pub fn carte_2004_clip(&self) -> Option<(i32, u32, bool)> {
        self.carte2004
            .clip
            .as_ref()
            .map(|c| (c.n, c.frame_num, c.ended))
    }

    /// Tests: `gameboy.spr` as the Gameboy loaded it.
    #[doc(hidden)]
    pub fn carte_2004_bank(&self) -> Option<Arc<Bank>> {
        self.carte2004.bank.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_32_bit_run_is_linear() {
        // An opaque white clip over black: the run from (100, 145), 700 ×
        // 355 pixels, ends at row 455, column 599.
        let src = vec![0xffff_ffffu32; WIDTH * HEIGHT];
        let mut dst = vec![0u32; WIDTH * HEIGHT];
        blend_clip_8888(&mut dst, &src, WIDTH, HEIGHT);
        let at = |x: usize, y: usize| dst[y * WIDTH + x] != 0;
        assert!(!at(99, 145) && at(100, 145) && at(799, 145));
        assert!(at(0, 146) && at(0, 454));
        assert!(at(599, 455) && !at(600, 455));
        assert!(!at(0, 456));
        assert_eq!(dst.iter().filter(|&&c| c != 0).count(), 700 * 355);
    }

    #[test]
    fn the_16_bit_rectangle() {
        let src = vec![0xffff_ffffu32; WIDTH * HEIGHT];
        let mut dst = vec![0u16; WIDTH * HEIGHT];
        blend_clip_565(&mut dst, &src, WIDTH, HEIGHT);
        let at = |x: usize, y: usize| dst[y * WIDTH + x] != 0;
        assert!(at(100, 145) && at(489, 459));
        assert!(!at(99, 145) && !at(490, 145) && !at(100, 144) && !at(100, 460));
        assert_eq!(dst.iter().filter(|&&c| c != 0).count(), 390 * 315);
        // A transparent source leaves the destination.
        let mut d = vec![0x1234u16; WIDTH * HEIGHT];
        blend_clip_565(&mut d, &vec![0x00ff_ffff; WIDTH * HEIGHT], WIDTH, HEIGHT);
        assert!(d.iter().all(|&c| c == 0x1234));
    }

    #[test]
    fn tables() {
        // Every god button once, the exit last; the hover order covers the
        // same six sprites.
        let mut a: Vec<usize> = CLICK_ORDER.iter().map(|c| c.0).collect();
        let mut b = HOVER_ORDER.to_vec();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
        assert_eq!(CLICK_ORDER[5], (7, None));
        assert_eq!(CLIPS.map(|c| c.1), [121, 120, 122, 123, 119]);
    }
}
