//! The canoe in the game: loop type 9 (`crate::canoe` is the puzzle).
//!
//! - **Start** FUN_0049c010(1), from the click hook of the `YU` view 402
//!   when `enable_pz_pirog` is 1 (`yucatan.rs`): nothing when the loop is
//!   already the canoe's; else `pz_pirogue_init` (FUN_00496a60:
//!   `(flag103=1)` first, @ 0x00496a7f, then the loading, the pointer at
//!   the 640 x 480 surface's centre, @ 0x00496b84, the cursor pair
//!   `interfac.spr` frame 0x28); when it succeeds the loop type left is kept
//!   (@ 0x006d84c4) and the loop type 9 set (FUN_0049da90(9): drawn on
//!   the 640 x 480 surface, as the init's FUN_00402960(0) asks); a failed
//!   load quits at once (FUN_0049a570) and the loop stays. The click goes
//!   on (`CLICZONE`, the click scan).
//! - **Frame** (the handler @ 0x0049c060: FUN_00498ca0 on each pass while
//!   the game is not paused, which the port's canoe never is): the pointer,
//!   the left click (@ 0x006569c9) and the right one (@ 0x006569a0), both
//!   cleared by their handlers, and Escape (the key code @ 0x00672014,
//!   which the key callback FUN_004762f0 writes: 1 quits).
//! - **Quit** FUN_0049a570 when the frame returns 0 (Escape, the win):
//!   the cursor pair cleared (FUN_0047bef0(0, 0)), everything freed, the
//!   loop type kept set again (FUN_0049da90).
//! - **Fatal errors** of the river's rebuild and the stones' search
//!   (FUN_00480cd0, [`Canoe::fatal`]): the original saves and quits; here
//!   the scripts stop with the message, as for their own fatal errors, and
//!   the puzzle quits.

use super::{CursorBank, Engine, Event, LoopType};
use crate::canoe::{Canoe, CanoeError, CanoeFiles, CanoeHost, POINTER_FRAME, START_COMMAND};
use atlantis2_data::spr::SprBank;
use atlantis2_data::Folder;

/// `pz_pirogue_init` (FUN_00496a60) and `pz_pirogue_quit` (FUN_0049a570),
/// for the events.
pub const CANOE_INIT: u32 = 0x0049_6a60;
pub const CANOE_QUIT: u32 = 0x0049_a570;

/// The game's files as the puzzle reads them: `datas_a2\3d\` of the
/// current disc (disc 4 in Xibalba), `spr\` of the install.
struct Files<'a>(&'a atlantis2_data::A2Files);

impl CanoeFiles for Files<'_> {
    fn three_d(&self, name: &str) -> Option<Vec<u8>> {
        let d = self.0.read(Folder::ThreeD, name);
        if d.is_none() {
            log::warn!("Fichier 3d inexistant : datas_a2\\3d\\{name}");
        }
        d
    }
    fn bank(&self, name: &str) -> Option<SprBank> {
        self.0
            .read(Folder::Spr, name)
            .and_then(|d| SprBank::parse(&d).ok())
    }
}

/// The host while loading: the music's steps counted.
struct LoadHost(usize);

impl CanoeHost for LoadHost {
    fn run_command(&mut self, _: &str) {}
    fn read_var(&mut self, _: &str) -> i32 {
        0
    }
    fn write_var(&mut self, _: &str, _: i32) {}
    fn scan(&mut self) {}
    fn music_step(&mut self) {
        self.0 += 1;
    }
}

/// The engine as the puzzle's host: the scripts and the music.
struct Host<'a>(&'a mut Engine);

impl CanoeHost for Host<'_> {
    fn run_command(&mut self, cmd: &str) {
        let c = cmd.as_bytes().to_vec();
        self.0.with_script(|s, h| s.run_command(h, &c));
    }
    fn read_var(&mut self, name: &str) -> i32 {
        self.0.con.as_ref().and_then(|s| s.var(name)).unwrap_or(0)
    }
    fn write_var(&mut self, name: &str, value: i32) {
        self.0.with_script(|s, _| s.set_var(name, value));
    }
    fn scan(&mut self) {
        self.0.with_script(|s, h| s.ctrl_scenar_in(h));
    }
    fn music_step(&mut self) {
        self.0.music_frame();
    }
}

impl Engine {
    /// FUN_0049c010(1): the canoe started (see the module).
    pub fn start_canoe(&mut self) {
        if self.loop_type == LoopType::Canoe && self.canoe.is_some() {
            return;
        }
        self.event(Event::PuzzleRan {
            function: CANOE_INIT,
        });
        self.with_script(|s, h| s.run_command(h, START_COMMAND.as_bytes()));
        let fpu = self.camera.fpu;
        let variant = self.renderer.variant;
        // The loading's only effect on the game around it is the music's
        // steps (FUN_00476fc0, 47 of them), run after it.
        let mut steps = LoadHost(0);
        let r = Canoe::load(fpu, variant, &Files(&self.files), &mut steps);
        for _ in 0..steps.0 {
            self.music_frame();
        }
        match r {
            Ok(mut c) => {
                self.camera.fpu = c.nodes.fpu;
                c.spr_dll = self.spr_dll;
                self.canoe = Some(Box::new(c));
                self.canoe_prev_loop = self.loop_type;
                self.set_loop_type(LoopType::Canoe);
                // The pointer at the centre of the surface the frame is
                // drawn on (@ 0x00496b84 .. 0x00496bb1: half the size
                // @ 0x005d9b9c, 0x005d9ba0, which FUN_00402960(0) has set:
                // 640 x 480 at 800 x 600 too).
                let (w, h) = self.display.logical_size();
                self.pointer = (w as i32 / 2, h as i32 / 2);
                self.set_cursor_pair(CursorBank::Interface, POINTER_FRAME as i32);
                self.canoe_escape = false;
            }
            Err(CanoeError::Fatal(m)) => {
                self.with_script(|s, _| s.fail(m));
                self.canoe_quit();
            }
            Err(e) => {
                log::warn!("pz_pirogue_init: {e:?}");
                self.canoe_quit();
            }
        }
    }

    /// The canoe being played, if any.
    pub fn canoe(&self) -> Option<&Canoe> {
        self.canoe.as_deref()
    }

    pub fn canoe_mut(&mut self) -> Option<&mut Canoe> {
        self.canoe.as_deref_mut()
    }

    /// FUN_0049a570, and the handler's return to the loop type kept.
    fn canoe_quit(&mut self) {
        self.event(Event::PuzzleRan {
            function: CANOE_QUIT,
        });
        self.clear_cursor_pair();
        if self.canoe.take().is_some() {
            let prev = self.canoe_prev_loop;
            self.set_loop_type(prev);
        }
    }

    /// The loop type 9's pass (the handler @ 0x0049c060).
    pub(super) fn canoe_pass(&mut self, out: Option<&mut [u16]>) {
        let Some(mut c) = self.canoe.take() else {
            let prev = self.canoe_prev_loop;
            self.set_loop_type(prev);
            return;
        };
        let (left, right) = (self.click, self.click_aux != 0);
        self.click = false;
        self.click_aux = 0;
        let escape = std::mem::take(&mut self.canoe_escape);
        c.nodes.fpu = self.camera.fpu;
        let mut scratch;
        let screen: &mut [u16] = match out {
            Some(o) if o.len() >= crate::canoe::W * crate::canoe::H => o,
            _ => {
                scratch = vec![0u16; crate::canoe::W * crate::canoe::H];
                &mut scratch
            }
        };
        let pointer = self.pointer;
        let going = {
            let mut host = Host(self);
            c.frame(&mut host, pointer, left, right, escape, screen)
        };
        self.camera.fpu = c.nodes.fpu;
        let fatal = c.fatal.clone();
        self.canoe = Some(c);
        if let Some(m) = fatal {
            self.with_script(|s, _| s.fail(m));
        }
        if !going {
            self.canoe_quit();
        }
    }
}
