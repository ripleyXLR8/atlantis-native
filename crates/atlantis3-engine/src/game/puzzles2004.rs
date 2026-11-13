//! The engine's side of the 2004 program's mini-games
//! ([`crate::puzzles2004`]): the script keys, the loop mode, the stretch
//! surface, and [`Cx`], the games' [`Host`].

use std::sync::Arc;

use atlantis2_engine::script::{PuzzleOp, Script};
use atlantis3_data::spr::Bank;

use super::{stretch_video, Game, Mode};
use crate::puzzles2004::{self as pz, Host, Puzzle, PuzzleId};
use crate::px::Surface;

/// The games' state in the engine.
pub(crate) struct State {
    /// The game whose loop mode runs (set by its start).
    pub mode: Option<PuzzleId>,
    /// The game loaded (its init ran, its free did not); taken out while
    /// its frame runs.
    pub game: Option<Box<dyn Puzzle>>,
    /// The loaded game's id, also while its frame runs.
    pub loaded: Option<PuzzleId>,
    /// The stretch surface (FUN_00412c40(800, 600)), `None` when closed.
    pub surface: Option<Surface>,
    /// A `FINPUZZLE` of the loaded game ran during its own frame: its
    /// free runs once the frame returns.
    pub fin_in_frame: bool,
    /// The virtual keys held (`GetKeyboardState`).
    pub keys: Box<[bool; 256]>,
    /// The last key's code (0x006e9e6c) and Ctrl at that event
    /// (0x006e9e71).
    pub key_code: i32,
    pub key_ctrl: bool,
    /// The pointer's last vertical motion (0x006e9fe4).
    pub mouse_dy: f32,
    /// A game's travel film (Bulle) and its transition sprites.
    pub film: Option<(crate::film2004::Film, Vec<Bank>)>,
    /// The games' program-wide globals ([`Host::kept`]).
    pub kept: std::collections::HashMap<String, i32>,
}

impl Default for State {
    fn default() -> State {
        State {
            mode: None,
            game: None,
            loaded: None,
            surface: None,
            fin_in_frame: false,
            keys: Box::new([false; 256]),
            key_code: 0,
            key_ctrl: false,
            mouse_dy: 0.0,
            film: None,
            kept: std::collections::HashMap::new(),
        }
    }
}

impl Game {
    /// `INITPUZZLE` / `STARTPUZZLE` / `FINPUZZLE` (the dispatchers
    /// FUN_004759c0, FUN_00475f80, FUN_00475ca0).
    /// `script`: the interpreter when it runs the key (taken out of the
    /// game meanwhile).
    pub(crate) fn puzzle_op_2004(&mut self, op: PuzzleOp, name: &str, script: Option<&mut Script>) {
        let Some(id) = PuzzleId::from_name(name) else {
            log::warn!("{op:?}PUZZLE={name}: no such game");
            return;
        };
        log::info!("{op:?}PUZZLE={name}");
        match op {
            PuzzleOp::Init => self.init_puzzle_2004(id, script),
            PuzzleOp::Start => self.start_puzzle_2004(id),
            PuzzleOp::Fin => self.fin_puzzle_2004(id),
        }
    }

    /// A game's init, which ends with its start.
    fn init_puzzle_2004(&mut self, id: PuzzleId, mut script: Option<&mut Script>) {
        // A game already loaded is dropped (the original overwrites its
        // globals; no CD script inits a second one).
        if let Some(mut g) = self.puzzles2004.game.take() {
            g.free(&mut Cx {
                g: self,
                s: script.as_deref_mut(),
            });
        }
        let made = pz::init(id, &mut Cx { g: self, s: script });
        match made {
            Some(g) => {
                self.puzzles2004.game = Some(g);
                self.puzzles2004.loaded = Some(id);
            }
            None => {
                let what = format!("puzzle {}", id.name());
                log::warn!("{what}: not ported");
                if !self.not_ported.contains(&what) {
                    self.not_ported.push(what);
                }
                self.puzzles2004.loaded = None;
            }
        }
        self.start_puzzle_2004(id);
    }

    /// A game's start (e.g. Caisses FUN_004770d0): the stretch surface
    /// opened (a second open only logs, FUN_00412c40 @ 0x00412c5a), the
    /// loop mode, the `Puzzles` flag.
    fn start_puzzle_2004(&mut self, id: PuzzleId) {
        if self.puzzles2004.surface.is_none() {
            self.puzzles2004.surface = Some(Surface::new(self.depth(), pz::WIDTH * pz::HEIGHT));
        } else {
            log::warn!("AtlDisplay_OpenStretchSurface : la surface est deja ouverte");
        }
        self.puzzles2004.mode = Some(id);
        self.mode = Mode::Puzzle2004;
        self.puzzles_2004[id.cfg_index()] = 1;
    }

    /// A game's free: the stretch surface closed (FUN_00412db0), the game's
    /// banks and sounds, loop mode 3.
    fn fin_puzzle_2004(&mut self, id: PuzzleId) {
        self.puzzles2004.surface = None;
        self.puzzles2004.mode = None;
        self.mode = Mode::Warp;
        if self.puzzles2004.loaded != Some(id) {
            return;
        }
        match self.puzzles2004.game.take() {
            Some(mut g) => {
                g.free(&mut Cx { g: self, s: None });
                self.puzzles2004.loaded = None;
            }
            // Its own frame runs: freed when it returns.
            None => self.puzzles2004.fin_in_frame = true,
        }
    }

    /// One pass of a game's loop mode (FUN_00476320(mode)): the game's
    /// frame, then the light dialog's pass.
    pub(crate) fn puzzle_frame_2004(&mut self) {
        let running = self.puzzles2004.mode;
        if running.is_some() && running == self.puzzles2004.loaded {
            if let Some(mut g) = self.puzzles2004.game.take() {
                g.frame(&mut Cx { g: self, s: None });
                if std::mem::take(&mut self.puzzles2004.fin_in_frame) {
                    g.free(&mut Cx { g: self, s: None });
                    self.puzzles2004.loaded = None;
                } else {
                    self.puzzles2004.game = Some(g);
                }
            }
        }
        self.light_dialog_2004();
        // FUN_00476320's `GAMEBOY` in the hand: the Gameboy (FUN_00409970).
        self.carte_hand_2004();
    }

    /// The free pointer while a game runs: clamped to the stretch
    /// surface (@ 0x0042dfc2..0x0042e00f).
    pub(crate) fn puzzle_pointer_2004(&mut self, dx: i32, dy: i32) {
        // The event's vertical delta kept as a float (@ 0x0042df73..7d).
        self.puzzles2004.mouse_dy = dy as f32;
        let (w, h) = if self.puzzles2004.surface.is_some() {
            (pz::WIDTH as i32, pz::HEIGHT as i32)
        } else {
            (self.width as i32, self.height as i32)
        };
        self.pointer.0 = (self.pointer.0 + dx).clamp(0, w - 1);
        self.pointer.1 = (self.pointer.1 + dy).clamp(0, h - 1);
    }

    /// A virtual key pressed or released (the state `GetKeyboardState`
    /// gives the games).
    pub fn key_2004(&mut self, vk: u8, down: bool) {
        let p = &mut self.puzzles2004;
        p.keys[usize::from(vk)] = down;
        // The key handlers: a press stores its code (0x0042c470 @
        // 0x0042c4fe), a release 0 (0x0042c3f0 @ 0x0042c400); both Ctrl
        // (@ 0x0042c417 / 0x0042c4f5). The press handler's flags
        // 0x00719d8e / 0x00719d8c are not ported [UNKNOWN].
        p.key_code = if down { pz::key_code_of_vk(vk) } else { 0 };
        p.key_ctrl = p.keys[0x11];
    }

    /// Tests: the game whose loop mode runs.
    pub fn puzzle_2004(&self) -> Option<PuzzleId> {
        self.puzzles2004.mode
    }

    /// Tests: the loaded game's state.
    pub fn puzzle_2004_state<T: 'static>(&self) -> Option<&T> {
        self.puzzles2004.game.as_ref()?.as_any().downcast_ref()
    }

    /// Tests: `Atlantis4.cfg`'s `Puzzles` values.
    pub fn puzzles_2004_cfg(&self) -> [i32; 8] {
        self.puzzles_2004
    }

    /// Tests: the stretch surface.
    pub fn puzzle_2004_surface(&self) -> Option<&Surface> {
        self.puzzles2004.surface.as_ref()
    }
}

/// The games' [`Host`]: the game engine while a game runs.
pub(crate) struct Cx<'a> {
    pub g: &'a mut Game,
    /// The interpreter when it runs the key (`INITPUZZLE`): the game's
    /// own is taken out meanwhile.
    pub s: Option<&'a mut Script>,
}

impl Cx<'_> {
    fn script(&self) -> Option<&Script> {
        self.s.as_deref().or(self.g.script.as_deref())
    }

    fn script_mut(&mut self) -> Option<&mut Script> {
        match self.s.as_deref_mut() {
            Some(s) => Some(s),
            None => self.g.script.as_deref_mut(),
        }
    }
}

impl Host for Cx<'_> {
    fn clock(&self) -> f64 {
        self.g.now
    }

    fn frame_time(&self) -> f32 {
        self.g.frame_t
    }

    fn pointer(&self) -> (i32, i32) {
        self.g.pointer
    }

    fn set_pointer(&mut self, x: i32, y: i32) {
        self.g.pointer = (x, y);
    }

    fn click(&self) -> bool {
        self.g.click
    }

    fn set_click(&mut self, on: bool) {
        self.g.click = on;
    }

    fn key(&self, vk: u8) -> bool {
        self.g.puzzles2004.keys[usize::from(vk)]
    }

    fn srand_time(&mut self) {
        let t = (self.g.unix_time)();
        self.g.rand = atlantis2_engine::crt::Rand::new(t as u32);
        self.g.tr(|| format!("srand {t}"));
    }

    fn rand(&mut self) -> i32 {
        self.g.script_rand()
    }

    fn var(&self, name: &str) -> i32 {
        self.script().and_then(|s| s.var(name)).unwrap_or(0)
    }

    fn set_var(&mut self, name: &str, v: i32) {
        self.g.tr(|| format!("var {name} {v}"));
        if let Some(s) = self.script_mut() {
            if let Ok(k) = s
                .names
                .id(atlantis2_data::defnames::VARIABLES, name.as_bytes())
            {
                s.vars[k] = v;
            }
        }
    }

    fn flag(&self, n: usize) -> i32 {
        self.script()
            .and_then(|s| s.flags.get(n).copied())
            .unwrap_or(0)
    }

    fn set_flag(&mut self, n: usize, v: i32) {
        self.g.tr(|| format!("flag {n} {v}"));
        if let Some(f) = self.script_mut().and_then(|s| s.flags.get_mut(n)) {
            *f = v;
        }
    }

    fn bank(&mut self, file: &str) -> Option<Arc<Bank>> {
        let depth = self.g.depth();
        let b = self
            .g
            .files
            .read(&format!("spr/{file}"))
            .and_then(|d| Bank::parse(&d, depth).ok())
            .map(Arc::new);
        if b.is_none() {
            log::warn!("spr\\{file}: not found");
        }
        b
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        self.g.files.read(path)
    }

    fn sfx_load(&mut self, file: &str) {
        self.g.load_sfx_2004(&format!("sfx\\{file}"), 0, false);
    }

    fn sfx_play(&mut self, file: &str) {
        self.g.play_sfx_2004(&format!("sfx\\{file}"));
    }

    fn sfx_remove(&mut self, file: &str) {
        self.g.remove_sfx_2004(&format!("sfx\\{file}"));
    }

    fn surface(&mut self) -> &mut Surface {
        let depth = self.g.depth();
        self.g
            .puzzles2004
            .surface
            .get_or_insert_with(|| Surface::new(depth, pz::WIDTH * pz::HEIGHT))
    }

    fn draw_pointer(&mut self) {
        let frame = super::pointer_frame(true);
        let (x, y) = self.g.pointer;
        let depth = self.g.depth();
        if let Some(bank) = self.g.interface.as_ref() {
            let s = self
                .g
                .puzzles2004
                .surface
                .get_or_insert_with(|| Surface::new(depth, pz::WIDTH * pz::HEIGHT));
            crate::with_px!(s, p => {
                crate::inventory::aff(p.as_mut_slice(), pz::WIDTH, pz::HEIGHT, "interface", bank, frame, x, y, 1.0);
            });
        }
    }

    fn flip(&mut self) {
        let g = &mut *self.g;
        if let Some(s) = g.puzzles2004.surface.as_ref() {
            let (sw, sh) = (g.width, g.height);
            match (s, &mut g.screen) {
                (Surface::Rgb565(s), Surface::Rgb565(o)) => {
                    stretch_video(s, pz::WIDTH, pz::HEIGHT, o, sw, sh)
                }
                (Surface::Xrgb8888(s), Surface::Xrgb8888(o)) => {
                    stretch_video(s, pz::WIDTH, pz::HEIGHT, o, sw, sh)
                }
                _ => {}
            }
        }
        g.flip_texts();
        g.tr(|| "flip".to_string());
    }

    fn script_prologue(&mut self) -> bool {
        let light = self.g.dialog2004.as_ref().is_some_and(|d| d.light);
        self.g.with_script(|s, h| {
            if !light {
                s.close_interrupt(h);
            }
            s.puz_scan_2004(h);
        });
        !self.g.puzzles2004.fin_in_frame
    }

    fn display_size(&self) -> (i32, i32) {
        if self.g.puzzles2004.surface.is_some() {
            (pz::WIDTH as i32, pz::HEIGHT as i32)
        } else {
            (self.g.width as i32, self.g.height as i32)
        }
    }

    fn tracing(&self) -> bool {
        self.g.trace.is_some()
    }

    fn trace(&mut self, line: String) {
        if let Some(t) = self.g.trace.as_mut() {
            t.push(line);
        }
    }

    fn key_code(&self) -> i32 {
        self.g.puzzles2004.key_code
    }

    fn set_key_code(&mut self, code: i32) {
        self.g.puzzles2004.key_code = code;
    }

    fn key_ctrl(&self) -> bool {
        self.g.puzzles2004.key_ctrl
    }

    fn in_bag(&self, name: &str) -> bool {
        self.script().is_some_and(|s| {
            s.names
                .id(atlantis2_data::defnames::OBJETS, name.as_bytes())
                .is_ok_and(|id| s.bag.iter().any(|e| e.id == id as i32))
        })
    }

    fn right_click(&self) -> bool {
        self.g.right_clicked
    }

    fn mouse_dy(&self) -> f32 {
        self.g.puzzles2004.mouse_dy
    }
    fn film_start(&mut self, file: &str, fps: i32, spf: &[&str]) -> bool {
        let depth = self.g.depth();
        // FUN_00436150, then FUN_00435f50 per name (`spf\`, FUN_00418f10).
        let banks: Vec<Bank> = spf
            .iter()
            .filter_map(|n| {
                let b = self
                    .g
                    .files
                    .read(&format!("spf/{n}"))
                    .and_then(|d| Bank::parse(&d, depth).ok());
                if b.is_none() {
                    log::warn!("spf\\{n}: not found");
                }
                b
            })
            .collect();
        self.g.puzzles2004.film = None;
        if fps != 15 && fps != 25 {
            log::warn!("AtlBink_start({file}): frame rate incorrect (p_Fps={fps}).");
            return false;
        }
        let path = format!("binktraj/{file}");
        let Some(data) = self.g.files.read(&path) else {
            log::warn!("AtlBink_start({path}): cinematique introuvable");
            return false;
        };
        match crate::film2004::Film::open(file, Arc::from(data), depth, self.g.now) {
            Ok(f) => {
                self.g.tr(|| format!("film {file} {}", spf.join(",")));
                self.g.puzzles2004.film = Some((f, banks));
                true
            }
            Err(e) => {
                log::warn!("AtlBink_start - Trajectoire non ouverte : {path} ({e:?})");
                false
            }
        }
    }
    fn film_frame_num(&self) -> i32 {
        self.g
            .puzzles2004
            .film
            .as_ref()
            .map_or(-1, |f| f.0.frame_num as i32)
    }
    fn film_frames(&self) -> i32 {
        self.g
            .puzzles2004
            .film
            .as_ref()
            .map_or(-1, |f| f.0.frames() as i32)
    }
    fn film_draw(&mut self) -> bool {
        let now = self.g.now;
        let depth = self.g.depth();
        let p = &mut self.g.puzzles2004;
        let Some((film, spf)) = p.film.as_mut() else {
            return false;
        };
        if !film.time_come(now) {
            return false;
        }
        if !film.drawn() && !film.do_frame() {
            return false;
        }
        let (w, h) = film.size();
        let n = film.frame_num;
        let surf = p
            .surface
            .get_or_insert_with(|| Surface::new(depth, pz::WIDTH * pz::HEIGHT));
        let top = pz::HEIGHT.saturating_sub(h) >> 1;
        let cw = w.min(pz::WIDTH);
        match surf {
            Surface::Rgb565(out) => {
                for y in 0..h.min(pz::HEIGHT - top) {
                    out[(top + y) * pz::WIDTH..][..cw].copy_from_slice(&film.rgb565[y * w..][..cw]);
                }
                crate::travel::draw_transition_sprites(out, pz::WIDTH, pz::HEIGHT, spf, n);
            }
            Surface::Xrgb8888(out) => {
                for y in 0..h.min(pz::HEIGHT - top) {
                    out[(top + y) * pz::WIDTH..][..cw].copy_from_slice(&film.xrgb[y * w..][..cw]);
                }
                crate::travel::draw_transition_sprites(out, pz::WIDTH, pz::HEIGHT, spf, n);
            }
        }
        true
    }
    fn film_next(&mut self) -> bool {
        let Some(f) = self.g.puzzles2004.film.as_mut() else {
            return false;
        };
        if f.0.next_frame() {
            return true;
        }
        // @ 0x00402f73: the last frame shown, `NUMFRAMEVIDEO` := FrameNum.
        let n = f.0.frame_num as i32;
        self.set_var("NUMFRAMEVIDEO", n);
        false
    }
    fn film_close(&mut self) {
        self.g.puzzles2004.film = None;
    }
    fn kept(&self, key: &str) -> Option<i32> {
        self.g.puzzles2004.kept.get(key).copied()
    }
    fn keep(&mut self, key: &str, v: i32) {
        self.g.puzzles2004.kept.insert(key.to_string(), v);
    }
    fn set_mouse_dy(&mut self, dy: f32) {
        self.g.puzzles2004.mouse_dy = dy;
    }
    fn set_right_click(&mut self, on: bool) {
        self.g.right_clicked = on;
    }
}
