//! What every puzzle's engine side uses: the PUZZLES\ files, the work
//! screen, the input, the puzzle samples.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (`puzzle_assets.cpp`, `con_exec.cpp` `playPuzzleSound`, the puzzles'
//! `readInput` / `drawBackground` / `present`; GPLv3, the ScummVM team).

use std::collections::HashMap;

use atlantis_data::sprite::{limits, read_spr_file, read_tga15, SprSprite};

use crate::ambient::{AmbientSampleOutput, MilesSampleStatus, SLOTS};
use crate::audio::Pcm;
use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::music::apc_pcm;
use crate::sys::{Input, MouseButton, Sys};
use crate::warp::{SCREEN_H, SCREEN_W};

/// The 640x480 class maps of the metal pieces and the hexagon.
pub const MAP_SIZE: usize = SCREEN_W * SCREEN_H;

/// A member of the PUZZLES\ folder.
pub async fn read_puzzle_file(e: &Engine, name: &str) -> Option<Vec<u8>> {
    e.read(&format!("PUZZLES\\{name}")).await
}

/// A PUZZLES\ sprite bank (`FUN_00410782`, the loader of
/// `loadPuzzleSpr`).
pub async fn load_spr(e: &Engine, name: &str) -> Option<Vec<SprSprite>> {
    let (w, h) = limits::DEFAULT;
    read_spr_file(&read_puzzle_file(e, name).await?, w, h)
}

/// A PUZZLES\ picture: a 640x480 15-bit TGA (`FUN_00447478`), RGB565.
pub async fn load_tga(e: &Engine, name: &str) -> Option<Vec<u16>> {
    let img = read_tga15(&read_puzzle_file(e, name).await?)?;
    (img.width == SCREEN_W && img.height == SCREEN_H).then_some(img.pixels)
}

/// A class map: the 640x480 8-bit pixels of a PUZZLES\ BMP from +0x436
/// (`FUN_004472cc` reads the file whole; the map is its pixels, as
/// stored).
pub async fn load_class_map(e: &Engine, name: &str) -> Option<Vec<u8>> {
    let d = read_puzzle_file(e, name).await?;
    d.get(0x436..0x436 + MAP_SIZE).map(<[u8]>::to_vec)
}

/// The pixels a sprite frame covers, as offsets from its drop point: the
/// pixels the placement tests (`FUN_00448404`, `FUN_00440efc`) count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub dx: Vec<i16>,
    pub dy: Vec<i16>,
}

fn le16(d: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([d[p], d[p + 1]])
}

fn le32(d: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}

/// The coverage of every frame of an SPR file, as the original blitter
/// `FUN_00410430` draws it: the raw pixels whose colour-table entry is not
/// 0, and the blended pixels with a non-zero colour and factor.
pub fn read_coverage(d: &[u8]) -> Option<Vec<Coverage>> {
    let size = d.len();
    if size < 4 {
        return None;
    }
    let flags = le16(d, 0);
    let mut p = 2usize;
    let mut table = [0u16; 256];
    if flags & 4 != 0 {
        if p + 512 > size {
            return None;
        }
        for (i, t) in table.iter_mut().enumerate() {
            *t = le16(d, p + 2 * i);
        }
        p += 512;
    }
    if flags & 8 != 0 {
        p += 512;
    }
    if p + 2 > size {
        return None;
    }
    let n = usize::from(le16(d, p));
    p += 2;
    let base = p;
    if base + 4 * n > size {
        return None;
    }
    let mut out = vec![Coverage::default(); n];
    for (f, c) in out.iter_mut().enumerate() {
        let mut q = base + le32(d, base + 4 * f) as usize;
        if q + 8 > size {
            return None;
        }
        let w = usize::from(le16(d, q));
        let h = usize::from(le16(d, q + 2));
        let xo = le16(d, q + 4) as i16 as i32;
        let yo = le16(d, q + 6) as i16 as i32;
        q += 8;
        if q + 4 * h > size {
            return None;
        }
        let pix = q + 4 * h;
        for ry in 0..h {
            let mut r = pix + le32(d, q + 4 * ry) as usize;
            let mut rx = 0usize;
            while rx < w && r < size {
                let b = d[r];
                r += 1;
                let cnt = usize::from(b & 0x3f);
                match b >> 6 {
                    0 => rx += cnt,
                    1 => {
                        let mut k = 0;
                        while k < cnt && r < size {
                            if table[usize::from(d[r])] != 0 {
                                c.dx.push((rx as i32 - xo) as i16);
                                c.dy.push((ry as i32 - yo) as i16);
                            }
                            r += 1;
                            k += 1;
                            rx += 1;
                        }
                    }
                    _ => {
                        let mut k = 0;
                        while k < cnt && r + 1 < size {
                            let factor = d[r] & 0x1f;
                            let col = table[usize::from(d[r + 1])];
                            r += 2;
                            if factor != 0 && col != 0 {
                                c.dx.push((rx as i32 - xo) as i16);
                                c.dy.push((ry as i32 - yo) as i16);
                            }
                            k += 1;
                            rx += 1;
                        }
                    }
                }
            }
        }
    }
    Some(out)
}

/// The work screen of a puzzle: the pictures it shows behind its sprites,
/// the PUZZLES\ sprite bank, and the frame being drawn.
pub struct PuzzleScreen {
    pub working: Vec<u16>,
    /// The pictures; [`PuzzleScreen::picture`] is the one shown.
    pub pictures: Vec<Vec<u16>>,
    pub picture: usize,
    /// The puzzle's sprite bank (0x007beae0).
    pub frames: Vec<SprSprite>,
    bg_done: bool,
}

impl Default for PuzzleScreen {
    fn default() -> Self {
        PuzzleScreen {
            working: vec![0; MAP_SIZE],
            pictures: Vec::new(),
            picture: 0,
            frames: Vec::new(),
            bg_done: false,
        }
    }
}

impl PuzzleScreen {
    /// `FUN_00410f36`, called twice a pass by the original's loops: the
    /// first copy restores the picture, the second is the copy of the
    /// frame to the screen, which [`PuzzleScreen::present`] makes.
    pub fn draw_background(&mut self) {
        if self.bg_done {
            return;
        }
        self.bg_done = true;
        if let Some(p) = self.pictures.get(self.picture) {
            if p.len() == self.working.len() {
                self.working.copy_from_slice(p);
            }
        }
    }

    /// A frame of the sprite bank with its hotspot at (x, y).
    pub fn draw_frame(&mut self, frame: i32, x: i32, y: i32) {
        if let Some(spr) = usize::try_from(frame).ok().and_then(|f| self.frames.get(f)) {
            spr.draw(&mut self.working, SCREEN_W, SCREEN_H, x, y);
        }
    }

    /// `FUN_0041afd0`: the frame to the screen, and the next frame. The
    /// original's loops have no wait; one pass a displayed frame.
    pub async fn present(&mut self, sys: &Sys) {
        self.bg_done = false;
        sys.screen.borrow_mut().copy_from_slice(&self.working);
        sys.frame().await;
    }
}

/// The mouse as a puzzle reads it at a frame boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PuzzleMouse {
    pub x: i32,
    pub y: i32,
    /// The left button went down since the last read.
    pub click: bool,
    /// The right button is down (`FUN_0041a298` reports it while it is
    /// held; the puzzles keep their own repeat counters).
    pub right: bool,
}

/// The input of a puzzle: the left button's state at the last read.
#[derive(Clone, Copy, Debug, Default)]
pub struct PuzzleInput {
    prev_left: bool,
}

impl PuzzleInput {
    /// Starts with the button state as it is (a button still down from
    /// the zone's click is no click) and the events dropped.
    pub fn new(sys: &Sys) -> PuzzleInput {
        while sys.poll_event().is_some() {}
        PuzzleInput {
            prev_left: sys.buttons().0,
        }
    }

    /// The events since the last read, then the mouse. A press and its
    /// release between two reads still count as a click.
    pub fn read(&mut self, sys: &Sys) -> PuzzleMouse {
        let mut pressed = false;
        while let Some(ev) = sys.poll_event() {
            if let Input::MouseDown {
                button: MouseButton::Left,
                ..
            } = ev
            {
                pressed = true;
            }
        }
        let (left, right) = sys.buttons();
        let (x, y) = sys.mouse();
        let click = (left && !self.prev_left) || pressed;
        self.prev_left = left;
        PuzzleMouse { x, y, click, right }
    }
}

/// The players of the puzzle samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplePlayer {
    /// `FUN_0043ca28`: under 20000 bytes the first of samples 0 and 1 that
    /// is done, else sample 2 when done.
    Ca28,
    /// `FUN_0043cd60`: up to 120000 bytes, the first of samples 0..2 done.
    Cd60,
    /// `FUN_0043ce80`: sample 0 when done or stopped.
    Ce80,
    /// `FUN_0043cf70`: AIL_stop_sample, then the start, on sample 1.
    Cf70,
}

/// A puzzle's sounds, read at its set-up (`FUN_0043c8d8`): the decoded
/// size the players test (the APC header's sample count at +0xc times 2,
/// or 4 when +0x1c & 1, as `FUN_00417cb4` allocates) and the sound.
#[derive(Default)]
pub struct PuzzleSounds {
    sounds: HashMap<String, (i32, Pcm)>,
}

impl PuzzleSounds {
    /// Reads `WAV\<name>.APC` for each name.
    pub async fn load(e: &Engine, names: &[&str]) -> PuzzleSounds {
        let files: Vec<String> = names.iter().map(|n| format!("WAV\\{n}.APC")).collect();
        e.prefetch(&files).await;
        let mut sounds = HashMap::new();
        for (name, file) in names.iter().zip(&files) {
            let Some(d) = e.read(file).await else {
                continue;
            };
            if d.len() < 0x20 {
                continue;
            }
            let samples = le32(&d, 0xc);
            let bytes = samples.wrapping_mul(if le32(&d, 0x1c) & 1 != 0 { 4 } else { 2 }) as i32;
            if let Some(pcm) = apc_pcm(&d) {
                sounds.insert(name.to_ascii_uppercase(), (bytes, pcm));
            }
        }
        PuzzleSounds { sounds }
    }
}

/// `FUN_0043d0e0` (0x0043d0f8..0x0043d145): samples 0..3 stopped.
pub fn stop_samples(e: &mut Engine) {
    for i in 0..4 {
        e.ambient_out.stop(i);
    }
}

/// What a puzzle's engine side works with: the engine, the scripts (a
/// puzzle may run the /sel scan), the work screen, the input and the
/// sounds. The puzzles implement their game's I/O trait on it.
pub struct PuzzleHost<'a> {
    pub e: &'a mut Engine,
    pub hooks: &'a mut dyn ScriptHooks,
    pub screen: PuzzleScreen,
    pub input: PuzzleInput,
    pub sounds: PuzzleSounds,
}

impl<'a> PuzzleHost<'a> {
    /// The puzzles draw their own cursor into the frame and read the
    /// pointer where it is: the mouse is not locked.
    pub fn new(e: &'a mut Engine, hooks: &'a mut dyn ScriptHooks) -> PuzzleHost<'a> {
        e.sys.set_mouse_locked(false);
        let input = PuzzleInput::new(&e.sys);
        PuzzleHost {
            e,
            hooks,
            screen: PuzzleScreen::default(),
            input,
            sounds: PuzzleSounds::default(),
        }
    }

    /// The input at a frame boundary (`FUN_0043318c`).
    pub fn read_input(&mut self) -> PuzzleMouse {
        self.input.read(&self.e.sys)
    }

    pub fn draw_background(&mut self) {
        self.screen.draw_background();
    }

    /// A sprite: bank 'C' is CURSEURS (`FUN_00410430` with the cursor
    /// bank), every other bank the puzzle's own.
    pub fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        if bank == 'C' {
            self.draw_cursor(frame, x, y);
        } else {
            self.screen.draw_frame(frame, x, y);
        }
    }

    /// A CURSEURS frame. What `FUN_00410430` does with the animation's
    /// frame -1 is unknown; nothing is drawn.
    pub fn draw_cursor(&mut self, frame: i32, x: i32, y: i32) {
        let Ok(f) = usize::try_from(frame) else {
            return;
        };
        if let Some(spr) = self.e.cursor_sprites.get(f) {
            spr.draw(&mut self.screen.working, SCREEN_W, SCREEN_H, x, y);
        }
    }

    /// `FUN_0042d558` with no object in hand: CURSEURS frame 6.
    pub fn draw_standard_cursor(&mut self, x: i32, y: i32) {
        self.draw_cursor(6, x, y);
    }

    pub async fn present(&mut self) {
        self.screen.present(&self.e.sys).await;
    }

    /// A puzzle sound on the slot its player picks, pan centre and volume
    /// 0x6e (`FUN_0043cb8c` / `FUN_0043d150` / `FUN_0043d204`). The slot,
    /// or `None` when the player drops it.
    pub fn play_sound(&mut self, name: &str, player: SamplePlayer) -> Option<usize> {
        let (bytes, pcm) = self.sounds.sounds.get(name)?;
        let out = &mut self.e.ambient_out;
        let st: [i32; SLOTS] = std::array::from_fn(|i| out.status(i));
        let slot = match player {
            SamplePlayer::Ca28 => MilesSampleStatus::slot_ca28(&st, *bytes),
            SamplePlayer::Cd60 => MilesSampleStatus::slot_cd60(&st, *bytes),
            SamplePlayer::Ce80 => MilesSampleStatus::slot_ce80(&st, *bytes),
            SamplePlayer::Cf70 => {
                out.stop(1);
                Some(1)
            }
        };
        let Some(slot) = slot else {
            log::debug!("puzzle sound {name} ({bytes} bytes) dropped, statuses {st:?}");
            return None;
        };
        out.start_pcm(slot, pcm.clone(), 0x40, 0x6e);
        Some(slot)
    }

    /// AIL_sample_status of a sample handle.
    pub fn sample_status(&mut self, slot: usize) -> i32 {
        self.e.ambient_out.status(slot)
    }

    pub fn stop_samples(&mut self) {
        stop_samples(self.e);
    }

    /// `n` ticks of the 200 Hz timer.
    pub async fn wait_ticks(&mut self, n: u32) {
        self.e.sys.delay(u64::from(n) * 5).await;
    }

    /// The game was quit (or the place loop has to end).
    pub fn aborted(&self) -> bool {
        self.e.should_abort()
    }
}
