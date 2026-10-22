//! The game, as far as it goes: a new game opens on its first view; the
//! player looks around, the place animations play, and a click on a move
//! zone walks to the next view as the original does (turn, transition
//! video, arrival).
//!
//! Time follows the original: the frame dispatcher (@ 0x0049ce20) runs as
//! often as frames are drawn ([`Engine::pass`]), and a 15 Hz task
//! ([`Engine::tick`], @ 0x0040caa0) paces the videos, the place animations
//! and the cursor. The turn of a move is counted in frames.

use atlantis2_data::con::NewGame;
use atlantis2_data::defnames::{DefNames, Section};
use atlantis2_data::spr::{SprBank, Target};
use atlantis2_data::spw::{FaceBox, Spw};
use atlantis2_data::wam::{self, View, WamError, Zone};
use atlantis2_data::{A2Files, Folder, Wam};
use atlantis_codecs::{DecodeError, HnmVideo};

use crate::anim::{restore, Anim, Texel};
use crate::camera::{deg_to_rad, zone_aim, Camera, WARP_FOV};
use crate::fpu::Fpu;
use crate::look::{Look, DEFAULT_LIMIT_Y, NO_LIMIT};
use crate::turn::Turn;
use crate::warp::{faces_from_hnm, Renderer, Variant};

/// Why the engine cannot show a view.
#[derive(Debug)]
pub enum EngineError {
    /// `scenar\chapi000.con` is missing or does not say where to start.
    NoStart,
    NoWam(String),
    BadWam(String, WamError),
    NoView(String, u32),
    /// The view's warp file is on no disc found.
    NoWarp(String),
    BadWarp(String, DecodeError),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NoStart => write!(f, "scenar\\chapi000.con missing or without a start"),
            EngineError::NoWam(w) => write!(f, "wam\\{w} not found"),
            EngineError::BadWam(w, e) => write!(f, "wam\\{w}: {e}"),
            EngineError::NoView(w, v) => write!(f, "{w} has no view {v}"),
            EngineError::NoWarp(n) => write!(f, "datas_a2\\warp\\{n} not found on the discs"),
            EngineError::BadWarp(n, e) => write!(f, "datas_a2\\warp\\{n}: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// What the engine did, for tests and logs ([`Engine::trace`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A left click reached the game (and whether it was a double click).
    Click { double: bool },
    /// `CtrlScenarOUT`, the departure scan, with `DEPARTVUE`.
    DepartScan { target: i32 },
    /// The click scan (@ 0x0045a870) with `CLICZONE`.
    ClickScan { zone: i32 },
    /// The arrival scan of `Interp_GereFrame` with `ARRIVEEVUE`.
    ArrivalScan { view: u32 },
    /// The turn set-up towards (yaw, pitch), radians.
    TurnSetup { yaw: f32, pitch: f32, turning: bool },
    /// A view entered (@ 0x00459fe0).
    Enter { view: u32 },
    /// An SPW slot loaded, or a file not found.
    SpwLoad { name: String, found: bool },
    /// `AtlHnm_start` on a transition: the file, and its frame count when
    /// it opened.
    VideoStart { name: String, frames: Option<u32> },
    /// A video frame shown.
    VideoFrame(u32),
    /// The video loop ended.
    VideoEnd,
    /// An object taken from the panorama (`CLICOBJET`).
    Take { id: i32 },
    /// A click on a zone the puzzle hooks take (@ 0x0049b910): not ported.
    PuzzleHook { zone: usize },
}

/// The loop types of the dispatcher (@ 0x006d8520) the engine has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopType {
    /// 0: the panorama.
    Warp,
    /// 1: a video (`AtlMain_loop_HNM` @ 0x0040c060).
    Video,
}

/// The script variables the navigation sets (`defnames.con` names).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vars {
    pub vue: u32,
    pub last_vue: u32,
    pub arrivee_vue: u32,
    pub cliczone: i32,
    pub clicobjet: i32,
    pub clicperso: i32,
    pub objet_en_main: i32,
}

/// The cursor (@ 0x00474500): a range of `interfac.spr` frames played on
/// the 15 Hz flag @ 0x00656a54, and the (bank, frame) pair drawn at the
/// screen centre (@ 0x00673fa0 / 0x005def58).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// @ 0x00671a00: the range's first frame, −1 none.
    pub start: i32,
    /// @ 0x006719e8: one past its last frame.
    pub end: i32,
    /// @ 0x006719f0: the frame shown next.
    pub frame: i32,
    /// The frame drawn, `None` when hidden.
    pub shown: Option<i32>,
}

impl Default for Cursor {
    fn default() -> Cursor {
        Cursor {
            start: -1,
            end: 0,
            frame: 0,
            shown: None,
        }
    }
}

impl Cursor {
    /// FUN_00474500(zone type, sprite state): moves (types 1, 2) frames
    /// 1..12, sprite state 1 frames 13..24, state 2 and zones 3.. frames
    /// 25..39; a range restarts only when it changes; types ≥ 1000 show
    /// nothing; one frame on per 15 Hz flag.
    fn choose(&mut self, kind: i32, state: i32, flag: &mut bool) {
        if (kind == 1 || kind == 2) && self.start != 1 {
            (self.start, self.end, self.frame) = (1, 13, 1);
        } else {
            let wide = if state == 1 {
                if self.start != 13 {
                    (self.start, self.end, self.frame) = (13, 25, 13);
                    false
                } else {
                    !(kind < 3 || self.start == 25)
                }
            } else if state != 2 || self.start == 25 {
                !(kind < 3 || self.start == 25)
            } else {
                true
            };
            if wide {
                (self.start, self.end, self.frame) = (25, 40, 25);
            }
        }
        self.shown = (kind <= 999).then_some(self.frame);
        if *flag {
            *flag = false;
            self.frame += 1;
        }
        if self.frame == self.end {
            self.frame = self.start;
        }
    }

    /// FUN_0047bf80: no cursor (and the range forgotten).
    fn hide(&mut self) {
        self.shown = None;
        self.start = -1;
    }
}

/// A transition video playing.
struct Video {
    hnm: HnmVideo,
    /// The tick counter when the last frame was shown (@ 0x006569dc).
    tick: u32,
    end: bool,
}

/// The engine.
pub struct Engine {
    files: A2Files,
    wam: Wam,
    wam_name: String,
    names: DefNames,
    view: u32,
    /// The view's faces as loaded, and the faces drawn on (with the
    /// sprites).
    faces: [Vec<u16>; 6],
    work: [Vec<u16>; 6],
    renderer: Renderer,
    pub camera: Camera,
    pub look: Look,
    /// The place animations.
    pub anim: Anim,
    pub loop_type: LoopType,
    turn: Option<Turn>,
    move_pending: bool,
    next_view: Option<u32>,
    /// The arrival angles (@ 0x005e2f30 / 0x005e2f34), radians.
    arrival: (f32, f32),
    /// The zone under the centre (@ 0x006d85e0) as (view, index): the
    /// pointer stays on the old view's zone after a move until the next
    /// pick.
    centre_zone: Option<(u32, usize)>,
    sprite_hit: Option<usize>,
    /// Whether the draw finds the sprites under the centre. False
    /// reproduces the harness of the moves golden, which stubs the draw.
    pub sprite_picks: bool,
    /// The sprite cursor state of the last draw (@ 0x006c2df4).
    sprite_state: i32,
    /// Input flags: a click (@ 0x006569c9) and a double click
    /// (@ 0x006569cb), and the time of the last click (@ 0x006569a8).
    click: bool,
    double: bool,
    last_click: Option<f64>,
    /// The 15 Hz counter (@ 0x006569d8) and the flags it sets for the
    /// SPW step (@ 0x00656a4c) and the cursor (@ 0x00656a54).
    pub ticks: u32,
    spw_flag: bool,
    cursor_flag: bool,
    /// Whether any SPW was ever loaded (@ 0x006c2df8).
    spw_loaded: bool,
    video: Option<Video>,
    pub vars: Vars,
    pub cursor: Cursor,
    cursor_bank: Option<SprBank>,
    /// The centre marker option (@ 0x005db84c, on by default).
    pub centre_marker: bool,
    /// The objects taken, in order (the bag, @ 0x004754f0: not ported
    /// beyond the list).
    pub bag: Vec<i32>,
    /// The object in hand: its id and sprite in the world's objects bank.
    pub hand: Option<(i32, i32)>,
    /// Record [`Event`]s into [`Engine::events`].
    pub trace: bool,
    pub events: Vec<Event>,
}

/// The texel of the cube under a direction: the face (cube order) and the
/// texel (column, row) the warp samples there, a + (b − a)·t of the face's
/// texture coordinates with a = 1/512, b = 511/512 (the cube's corners,
/// @ 0x0045c790).
fn texel_of(dir: [f32; 3]) -> Option<Texel> {
    let [x, y, z] = dir.map(f64::from);
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, u, v) = if az >= ax && az >= ay {
        if z > 0.0 {
            (0, x / az, y / az)
        } else {
            (1, -x / az, y / az)
        }
    } else if ay >= ax {
        if y < 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if x < 0.0 {
        (4, z / ax, y / ax)
    } else {
        (5, -z / ax, y / ax)
    };
    if !(u.is_finite() && v.is_finite()) {
        return None;
    }
    let texel = |s: f64| (1.0 + 510.0 * (s + 1.0) / 2.0).floor() as i32;
    Some((face, texel(u), texel(v)))
}

/// ARGB to RGB565.
fn rgb565(argb: u32) -> u16 {
    let (r, g, b) = ((argb >> 16) & 0xff, (argb >> 8) & 0xff, argb & 0xff);
    (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
}

impl Engine {
    /// A new game (menu action 10 for a player without a save, @ 0x00486eb0):
    /// chapter 0's `/INIT` chooses the world's disc, the place, the view
    /// and its angles (Tibet, `tibet.wam`, view 2, yaw −90°, pitch 0). The
    /// opening cinematic it then plays, and the rest of the script, are not
    /// run yet: the view is shown at once.
    ///
    /// `variant` is the CPU's cube renderer, `fpu` the x87 control word the
    /// process starts with. With `warpPII` the first frame changes it to
    /// 0x0e7f for good: the first view is entered under `fpu` and every
    /// later turn, entry and frame runs under 0x0e7f, as in the original if
    /// no assembly renderer drew before the first view (not known).
    pub fn new_game(
        mut files: A2Files,
        variant: Variant,
        fpu: Fpu,
        width: u32,
        height: u32,
    ) -> Result<Engine, EngineError> {
        let text = files
            .read(Folder::Scenar, "chapi000.con")
            .ok_or(EngineError::NoStart)?;
        let text: String = text.iter().map(|&b| char::from(b)).collect();
        let start = NewGame::from_chapter0(&text).ok_or(EngineError::NoStart)?;
        log::info!(
            "new game: MONDE {}, {} view {} at ({}, {})",
            start.monde,
            start.wam,
            start.view,
            start.yaw_deg,
            start.pitch_deg
        );
        files.set_monde(start.monde);
        let names = match files.read(Folder::Scenar, "defnames.con") {
            Some(d) => DefNames::parse(&d),
            None => {
                log::warn!("scenar\\defnames.con not found");
                DefNames::default()
            }
        };
        let mut wam = load_wam(&files, &start.wam)?;
        wam.apply_defnames(&names);
        let cursor_bank = files
            .read(Folder::Spr, "interfac.spr")
            .and_then(|d| SprBank::parse(&d).ok());
        if cursor_bank.is_none() {
            log::warn!("spr\\interfac.spr missing or unreadable: no cursor");
        }
        let camera = Camera::new(fpu, width, height);
        let mut engine = Engine {
            files,
            wam,
            wam_name: start.wam.clone(),
            names,
            view: 0,
            faces: Default::default(),
            work: Default::default(),
            renderer: Renderer::new(variant, fpu, width as usize, height as usize),
            camera,
            look: Look::default(),
            // GetTickCount() in the original: a port seeds with its clock
            // (`anim.rand`).
            anim: Anim::new(0),
            loop_type: LoopType::Warp,
            turn: None,
            move_pending: false,
            next_view: None,
            arrival: (0.0, 0.0),
            centre_zone: None,
            sprite_hit: None,
            sprite_picks: true,
            sprite_state: 0,
            click: false,
            double: false,
            last_click: None,
            ticks: 0,
            spw_flag: false,
            cursor_flag: false,
            spw_loaded: false,
            video: None,
            vars: Vars::default(),
            cursor: Cursor::default(),
            cursor_bank,
            centre_marker: true,
            bag: Vec::new(),
            hand: None,
            trace: false,
            events: Vec::new(),
        };
        engine.arrival = (
            deg_to_rad(engine.camera.fpu, start.yaw_deg),
            deg_to_rad(engine.camera.fpu, start.pitch_deg),
        );
        engine.enter_view(start.view, engine.arrival.0, engine.arrival.1)?;
        Ok(engine)
    }

    fn event(&mut self, e: Event) {
        if self.trace {
            self.events.push(e);
        }
    }

    /// Enters a view of the current place (@ 0x00459fe0): loads its cube,
    /// orients the camera to the arrival angles (radians), stops the turn,
    /// sets the 76° field of view and loads the view's place animations;
    /// the script resets the limits to ±70° in pitch and none in yaw at
    /// every view change.
    pub fn enter_view(&mut self, id: u32, yaw: f32, pitch: f32) -> Result<(), EngineError> {
        let view = self
            .wam
            .view(id)
            .ok_or_else(|| EngineError::NoView(self.wam_name.clone(), id))?;
        let name = view.name.clone();
        let (hnm, disc) = self
            .files
            .read_with_disc(Folder::Warp, &name)
            .ok_or_else(|| EngineError::NoWarp(name.clone()))?;
        self.faces = faces_from_hnm(&hnm).map_err(|e| EngineError::BadWarp(name.clone(), e))?;
        self.work = self.faces.clone();
        log::info!("view {id}: warp {name} (disc {disc})");
        self.view = id;
        self.vars.vue = id;
        self.event(Event::Enter { view: id });
        self.camera.enter(yaw, pitch);
        let (w, h) = (self.renderer.width() as u32, self.renderer.height() as u32);
        self.camera.set_fov(WARP_FOV, w, h);
        self.look.stop();
        self.look.limit_y = DEFAULT_LIMIT_Y;
        self.look.limit_x = NO_LIMIT;
        self.load_animations(id);
        // No sound in Tibet: the place sounds (@ 0x0047ad30) come with the
        // first world that has some.
        self.turn = None;
        Ok(())
    }

    /// The SPW slots of the view (@ 0x00459fe0's loop): every state of
    /// every entity, in file order, shown in this view.
    fn load_animations(&mut self, view: u32) {
        self.anim.clear();
        let mut loads = Vec::new();
        for e in &self.wam.entities {
            for s in e.state_records().filter(|s| s.view == view) {
                let sprite = if e.kind == 2 {
                    self.names.param(Section::Objects, &e.con_name)
                } else {
                    0
                };
                loads.push((e.spw_name(&s), e.kind as i32, e.index as i32, sprite, s));
            }
        }
        for (name, kind, id, sprite, s) in loads {
            let spw = self
                .files
                .read(Folder::Spw, &name)
                .and_then(|d| Spw::parse(d).ok());
            self.event(Event::SpwLoad {
                name: name.clone(),
                found: spw.is_some(),
            });
            match spw {
                Some(spw) => {
                    self.spw_loaded = true;
                    self.anim.load(&name, spw, kind, id, sprite, s);
                }
                None => log::warn!("Can't load SPW {name}"),
            }
        }
    }

    /// A mouse move, in mickeys (relative pointer motion).
    pub fn mouse_motion(&mut self, dx: i32, dy: i32) {
        self.look.mouse(self.camera.fpu, dx, dy);
    }

    /// A left button press at `now` seconds (the input callback
    /// @ 0x0040c510): a click, and a double click when it follows the
    /// previous one by less than 0.2 s.
    pub fn click(&mut self, now: f64) {
        self.click = true;
        self.double = self.last_click.is_some_and(|t| now - t < 0.2);
        self.last_click = Some(now);
        let double = self.double;
        self.event(Event::Click { double });
    }

    /// The 15 Hz task (@ 0x0040caa0): the counter and the flags of the
    /// animations and the cursor.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        self.spw_flag = true;
        self.cursor_flag = true;
    }

    /// One pass of the frame dispatcher (@ 0x0049ce20), drawing into `out`
    /// (width x height RGB565) when given. Without `out` everything but the
    /// pixels is done.
    pub fn pass(&mut self, out: Option<&mut [u16]>) {
        match self.loop_type {
            LoopType::Warp => self.warp_pass(out),
            LoopType::Video => self.video_pass(out),
        }
    }

    /// `AtlWarp_continue` (@ 0x0045a3b0).
    fn warp_pass(&mut self, mut out: Option<&mut [u16]>) {
        // 1. The turn step, or the mouse inertia and the limits
        // (@ 0x0040c6a0); then the zone under the centre.
        if let Some(turn) = &mut self.turn {
            if !turn.step(&mut self.camera) {
                self.turn = None;
                self.move_pending = true;
            }
        } else {
            self.look.frame(&mut self.camera);
        }
        self.centre_zone = self.pick_centre().map(|i| (self.view, i));
        // 2. The draw (@ 0x0045a9f0).
        self.draw(out.as_deref_mut());
        // 3. The clicks and the scans, unless turning.
        if self.turn.is_none() {
            self.sprite_click();
            self.zone_click();
            if self.vars.arrivee_vue != 0 {
                // Interp_GereFrame (@ 0x00484b70): the per-view reset
                // (@ 0x00483ce0) empties the hand, then the arrival scan.
                self.vars.objet_en_main = 0;
                self.hand = None;
                let view = self.vars.arrivee_vue;
                self.event(Event::ArrivalScan { view });
                self.vars.arrivee_vue = 0;
            }
            if self.click {
                let zone = self.vars.cliczone;
                self.event(Event::ClickScan { zone });
            }
        }
        // 4. The click flags.
        self.click = false;
        // 5. The move completes.
        if self.move_pending {
            self.complete_move();
            // AtlHnm_start shows the video's first frame at once.
            if let (Some(v), Some(out)) = (&self.video, out) {
                let (w, h) = (self.renderer.width(), self.renderer.height());
                blit_centred(v.hnm.frame(), v.hnm.width(), v.hnm.height(), out, w, h);
            }
        }
        // 6.
        self.vars.cliczone = 0;
    }

    /// The zone under the screen centre (@ 0x00458fe0).
    fn pick_centre(&self) -> Option<usize> {
        let view = self.wam.view(self.view)?;
        let d = self
            .camera
            .ray(self.camera.cx, self.camera.cy)
            .map(f64::from);
        wam::pick(&view.zones, d)
    }

    /// The warp draw (@ 0x0045a9f0): sprites into the faces, the cube, the
    /// faces restored, the animation step; the cursor and the centre
    /// marker; the cursor chosen for the next frame.
    fn draw(&mut self, out: Option<&mut [u16]>) {
        let centre = texel_of(self.camera.ray(self.camera.cx, self.camera.cy));
        let (dirty, mut hit): (Vec<(usize, FaceBox)>, _) =
            self.anim.draw(&mut self.work, centre, self.click);
        if !self.sprite_picks {
            hit = Default::default();
        }
        self.sprite_state = hit.cursor;
        self.sprite_hit = hit.slot;
        let (w, h) = (self.renderer.width(), self.renderer.height());
        if let Some(out) = out {
            let (m, p) = (self.camera.matrix(), self.camera.projection());
            self.renderer.fpu = self.camera.fpu;
            self.renderer.render(&self.work, &m, p, out);
            self.camera.fpu = self.renderer.fpu;
            self.draw_centre(out, w, h);
        } else if self.renderer.variant == Variant::PII {
            // warpPII leaves 0x0e7f whether or not the pixels are wanted.
            self.camera.fpu = Fpu::Chop;
            self.renderer.fpu = Fpu::Chop;
        }
        restore(&mut self.work, &self.faces, &dirty);
        if self.spw_flag && self.spw_loaded {
            self.spw_flag = false;
            self.anim.step();
        }
        // The cursor for the next frame (after FUN_0045af10 drew this one).
        let zone_kind = self.centre().map(|(_, z)| z.kind);
        if self.sprite_state != 0 {
            self.cursor
                .choose(0, self.sprite_state, &mut self.cursor_flag);
        } else if let Some(kind) = zone_kind {
            self.cursor.choose(kind, 0, &mut self.cursor_flag);
        } else {
            self.cursor.hide();
        }
    }

    /// FUN_0045af10: the cursor at the screen centre (FUN_0047bfd0, the
    /// object in hand under it: not drawn, its bank is not known), then
    /// the 5-pixel cross of the centre marker in the colour of what is
    /// under the centre.
    fn draw_centre(&self, out: &mut [u16], w: usize, h: usize) {
        let (cx, cy) = (w as i32 / 2, h as i32 / 2);
        if let (Some(bank), Some(frame)) = (&self.cursor_bank, self.cursor.shown) {
            let mut t = Target::screen(out, w, h);
            bank.draw(&mut t, Some(cx), cy, frame as usize);
        }
        if !self.centre_marker {
            return;
        }
        let kind = self.centre().map(|(_, z)| z.kind);
        let mut argb = match kind {
            Some(1) => 0xff00_00ff,
            Some(2) => 0xffff_0000,
            Some(k) if k > 3 && k < 1000 => 0xffff_00ff,
            _ => 0xffff_ffff,
        };
        if self.sprite_state != 0 {
            argb = 0xff00_ff00;
        }
        // The two lines of FUN_004c4620, end points included [INFERRED].
        let c = rgb565(argb);
        for k in -2..=2 {
            for (x, y) in [(cx, cy + k), (cx + k, cy)] {
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    out[y as usize * w + x as usize] = c;
                }
            }
        }
    }

    /// The zone the centre pointer designates in the current view.
    fn centre(&self) -> Option<(usize, &Zone)> {
        let (v, i) = self.centre_zone?;
        if v != self.view {
            return None;
        }
        Some((i, self.wam.view(v)?.zones.get(i)?))
    }

    /// The index of the zone the centre pointer designates in the current
    /// view's list, −1 when none (or a zone of another view).
    pub fn centre_index(&self) -> i32 {
        self.centre().map_or(-1, |(i, _)| i as i32)
    }

    fn current_view(&self) -> Option<&View> {
        self.wam.view(self.view)
    }

    /// FUN_0045a650 and FUN_0045a7c0: a click on a sprite.
    fn sprite_click(&mut self) {
        let Some(n) = self.sprite_hit else { return };
        let Some(slot) = self.anim.slots.get(n) else {
            return;
        };
        let (id, click, kind, sprite) = (slot.id, slot.click, slot.kind, slot.object_sprite);
        match click {
            1 => {
                // An object to take: hidden for good, in the bag and in
                // the hand (its sprite in the world's objects bank).
                self.vars.clicobjet = id;
                self.anim.take(n);
                self.bag.push(id);
                self.vars.objet_en_main = id;
                self.hand = Some((id, sprite));
                self.event(Event::Take { id });
                log::info!("Objet pris en spw : {}", self.name_of(1, id));
            }
            3 => self.vars.clicobjet = id,
            4 => self.vars.clicperso = id,
            _ => {}
        }
        if click == 2 {
            // The subject menu of a character (FUN_0045a7c0) needs the
            // dialogs: not ported.
            let section = if kind == 1 { 0 } else { 1 };
            log::info!(
                "{}: the subject menu is not ported",
                self.name_of(section, id)
            );
        }
    }

    fn name_of(&self, section: u8, id: i32) -> String {
        let s = if section == 0 {
            Section::Characters
        } else {
            Section::Objects
        };
        self.names
            .name(s, id as usize)
            .map_or_else(|| id.to_string(), str::to_string)
    }

    /// FUN_0049ca80: a click on the zone under the centre.
    fn zone_click(&mut self) {
        if !self.click {
            return;
        }
        let Some((zi, zone)) = self.centre().map(|(i, z)| (i, z.clone())) else {
            return;
        };
        if self.sprite_hit.is_some() {
            self.vars.cliczone = zone.value;
            self.puzzle_hooks(zi, &zone);
            return;
        }
        let target = if zone.kind == 1 || zone.kind == 2 {
            zone.value
        } else {
            0
        };
        self.event(Event::DepartScan { target });
        // ABORTDEPART is never set without the scripts.
        if self.puzzle_hooks(zi, &zone) {
            return;
        }
        if zone.kind > 2 && zone.kind < 88888 {
            self.vars.cliczone = zone.value;
        }
        match zone.kind {
            1 => self.move_setup(&zone),
            2 => log::warn!("zone {zi}: moves to other places are not ported"),
            _ => {}
        }
    }

    /// FUN_0049b910: the puzzles' own click handling, by warp prefix and
    /// view. Tibet's is the mandala placing of view 10 (zone types 3, 4,
    /// 50, @ 0x004954f0), which returns 0; not ported.
    fn puzzle_hooks(&mut self, zi: usize, zone: &Zone) -> bool {
        let tibet = self
            .current_view()
            .is_some_and(|v| v.name.len() >= 2 && v.name[..2].eq_ignore_ascii_case("ti"));
        if tibet && self.view == 10 && matches!(zone.kind, 3 | 4 | 50) {
            self.event(Event::PuzzleHook { zone: zi });
        }
        false
    }

    /// FUN_0049cc40: the move set-up of a type-1 zone.
    fn move_setup(&mut self, zone: &Zone) {
        let fpu = self.camera.fpu;
        let target = zone.value as u32;
        // NEXTVUE (a script key) is not ported: the zone's view.
        self.next_view = self.wam.view(target).map(|v| v.id);
        let aim = || zone_aim(fpu, &zone.points);
        if self.next_view.is_none() {
            let (y, p) = aim();
            self.start_turn(y, p);
            return;
        }
        let mut start = (999.0f32, f32::from_bits(0x4479_c000));
        let dests: Vec<_> = self
            .current_view()
            .map(|v| {
                v.destinations
                    .iter()
                    .filter(|d| d.target == target)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !dests.is_empty() {
            for d in &dests {
                if d.angles_as_given == 0 {
                    start = if d.start_yaw <= 998.0 {
                        (d.start_yaw, d.start_pitch)
                    } else {
                        aim()
                    };
                    self.arrival = if d.end_yaw <= 998.0 {
                        (d.end_yaw, d.end_pitch)
                    } else {
                        start
                    };
                    if !self.start_turn(start.0, start.1) {
                        self.move_pending = true;
                    }
                } else {
                    start = (d.start_yaw, d.start_pitch);
                    self.arrival = (d.end_yaw, d.end_pitch);
                    // No turn needed: nothing makes the move pending (a
                    // click that does nothing, kept from the original).
                    self.start_turn(start.0, start.1);
                }
            }
            if start.0 != 999.0 {
                return;
            }
        }
        let (y, p) = aim();
        self.start_turn(y, p);
        self.arrival = (y, p);
    }

    /// FUN_0045b690: sets up a turn; false when none is needed.
    fn start_turn(&mut self, yaw: f32, pitch: f32) -> bool {
        let (turn, turning) = Turn::new(&self.camera, yaw, pitch);
        self.event(Event::TurnSetup {
            yaw,
            pitch,
            turning,
        });
        self.turn = turning.then_some(turn);
        turning
    }

    /// The end of a move (@ 0x0045a3b0, move pending): the new view
    /// entered at the arrival angles, then its transition video.
    fn complete_move(&mut self) {
        self.move_pending = false;
        let Some(next) = self.next_view.take() else {
            // FUN_0045a630 (after a change of place): not ported.
            log::warn!("move without a next view");
            return;
        };
        let old = self.view;
        self.vars.last_vue = old;
        self.vars.arrivee_vue = next;
        if let Err(e) = self.enter_view(next, self.arrival.0, self.arrival.1) {
            log::error!("{e}");
            return;
        }
        let dest = self
            .wam
            .places
            .iter()
            .flat_map(|p| p.views.iter())
            .filter(|v| v.id == old)
            .find_map(|v| v.destinations.iter().find(|d| d.target == next))
            .cloned();
        if let Some(d) = &dest {
            if !d.sprites.is_empty() {
                // FUN_0047c100: none in Tibet.
                log::warn!("transition sprites are not ported");
            }
        }
        // UDADA (a script key) is 0 without the scripts.
        if let (Some(d), false) = (&dest, self.double) {
            // The name is used as a printf format by the original.
            self.start_video(&d.video.clone());
        }
        if self.sprite_hit.is_none() {
            // FUN_0047bf40: the hand emptied.
            self.cursor.shown = None;
            self.vars.objet_en_main = 0;
            self.hand = None;
        }
    }

    /// `AtlHnm_start` (@ 0x0040bf60): opens the video, shows its first
    /// frame at once and switches to the video loop; false (a cut) when the
    /// file is on no disc or is not a video.
    fn start_video(&mut self, name: &str) -> bool {
        let opened = self
            .files
            .read(Folder::Hnm, name)
            .and_then(|d| HnmVideo::new(d).ok());
        let Some(mut hnm) = opened else {
            self.event(Event::VideoStart {
                name: name.to_string(),
                frames: None,
            });
            log::info!("{name}: no transition video, a cut");
            return false;
        };
        let frames = hnm.frame_count();
        self.event(Event::VideoStart {
            name: name.to_string(),
            frames: Some(frames),
        });
        // A transition has no sound of its own in Tibet (no `TrajSound`).
        let end = !matches!(hnm.next_frame(), Some(Ok(_))) || hnm.frames_decoded() >= frames;
        self.event(Event::VideoFrame(0));
        self.video = Some(Video {
            hnm,
            tick: self.ticks,
            end,
        });
        self.loop_type = LoopType::Video;
        true
    }

    /// `AtlMain_loop_HNM` (@ 0x0040c060): a new frame at each 15 Hz tick;
    /// the end, a click or a double click ends it.
    fn video_pass(&mut self, out: Option<&mut [u16]>) {
        let Some(v) = &mut self.video else {
            self.loop_type = LoopType::Warp;
            return;
        };
        let mut shown = None;
        if v.tick != self.ticks {
            v.tick = self.ticks;
            if !v.end {
                match v.hnm.next_frame() {
                    Some(Ok(_)) => shown = Some(v.hnm.frames_decoded() - 1),
                    _ => v.end = true,
                }
                if v.hnm.frames_decoded() >= v.hnm.frame_count() {
                    v.end = true;
                }
            }
            if let Some(out) = out {
                let (w, h) = (self.renderer.width(), self.renderer.height());
                blit_centred(v.hnm.frame(), v.hnm.width(), v.hnm.height(), out, w, h);
            }
            let end = v.end;
            if let Some(k) = shown {
                self.event(Event::VideoFrame(k));
            }
            if !end && !self.click && !self.double {
                return;
            }
        } else {
            if let Some(out) = out {
                let (w, h) = (self.renderer.width(), self.renderer.height());
                blit_centred(v.hnm.frame(), v.hnm.width(), v.hnm.height(), out, w, h);
            }
            return;
        }
        self.click = false;
        self.video = None;
        self.loop_type = LoopType::Warp;
        self.event(Event::VideoEnd);
    }

    /// Draws the current screen into `out` without running a pass: the
    /// cube with the place animations as FUN_0045a930 draws it (no cursor,
    /// no centre marker), or the video's frame.
    pub fn render(&mut self, out: &mut [u16]) {
        let (w, h) = (self.renderer.width(), self.renderer.height());
        if let Some(v) = &self.video {
            blit_centred(v.hnm.frame(), v.hnm.width(), v.hnm.height(), out, w, h);
            return;
        }
        let centre = texel_of(self.camera.ray(self.camera.cx, self.camera.cy));
        let (dirty, _) = self.anim.draw(&mut self.work, centre, false);
        let (m, p) = (self.camera.matrix(), self.camera.projection());
        self.renderer.fpu = self.camera.fpu;
        self.renderer.render(&self.work, &m, p, out);
        self.camera.fpu = self.renderer.fpu;
        restore(&mut self.work, &self.faces, &dirty);
    }

    /// The x87 control word the game's float code runs under now.
    pub fn fpu(&self) -> Fpu {
        self.camera.fpu
    }

    /// The cube renderer.
    pub fn variant(&self) -> Variant {
        self.renderer.variant
    }

    pub fn width(&self) -> usize {
        self.renderer.width()
    }

    pub fn height(&self) -> usize {
        self.renderer.height()
    }

    /// The current place file and view id.
    pub fn place(&self) -> (&str, u32) {
        (&self.wam_name, self.view)
    }

    pub fn wam(&self) -> &Wam {
        &self.wam
    }

    pub fn files(&self) -> &A2Files {
        &self.files
    }

    /// The view's faces in cube order, as loaded (without the sprites).
    pub fn faces(&self) -> &[Vec<u16>; 6] {
        &self.faces
    }

    /// Whether a move's turn is running.
    pub fn turning(&self) -> bool {
        self.turn.is_some()
    }

    /// The angles read back from the camera, in degrees.
    pub fn angles_deg(&self) -> (f32, f32) {
        let (y, p) = self.camera.angles();
        (y.to_degrees(), p.to_degrees())
    }

    /// The click zone under the screen centre (@ 0x0040c6a0 picks it every
    /// frame, @ 0x00458fe0): its index in the view's list, and the zone.
    pub fn centre_zone(&self) -> Option<(usize, &Zone)> {
        let i = self.pick_centre()?;
        Some((i, &self.current_view()?.zones[i]))
    }

    /// A zone's aim (FUN_00459300), radians.
    pub fn zone_aim(&self, view: u32, zone: usize) -> Option<(f32, f32)> {
        let z = self.wam.view(view)?.zones.get(zone)?;
        Some(zone_aim(self.camera.fpu, &z.points))
    }

    /// Enters `view` as a move leaves it (the arrival angles, radians,
    /// then the view entry), outside any move: for tests and tools.
    pub fn enter_at(&mut self, view: u32, yaw: f32, pitch: f32) -> Result<(), EngineError> {
        self.arrival = (yaw, pitch);
        self.vars.last_vue = self.view;
        self.enter_view(view, yaw, pitch)
    }
}

/// A video frame onto the screen, centred (the original's screen and
/// videos are both 640 x 480).
fn blit_centred(src: &[u16], sw: usize, sh: usize, out: &mut [u16], w: usize, h: usize) {
    if sw > w || sh > h {
        for y in 0..h.min(sh) {
            let n = w.min(sw);
            out[y * w..y * w + n].copy_from_slice(&src[y * sw..y * sw + n]);
        }
        return;
    }
    let (ox, oy) = ((w - sw) / 2, (h - sh) / 2);
    for y in 0..sh {
        out[(oy + y) * w + ox..(oy + y) * w + ox + sw].copy_from_slice(&src[y * sw..(y + 1) * sw]);
    }
}

fn load_wam(files: &A2Files, name: &str) -> Result<Wam, EngineError> {
    let data = files
        .read(Folder::Wam, name)
        .ok_or_else(|| EngineError::NoWam(name.to_string()))?;
    Wam::parse(&data).map_err(|e| EngineError::BadWam(name.to_string(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_ranges() {
        let mut c = Cursor::default();
        let mut flag = false;
        // A move zone: frames 1..12; the frame shown, then one on per
        // flag, wrapping.
        c.choose(1, 0, &mut flag);
        assert_eq!(c.shown, Some(1));
        for k in 0..13 {
            flag = true;
            c.choose(1, 0, &mut flag);
            assert_eq!(c.shown, Some(1 + k % 12));
        }
        // A character sprite: 25..39; an object: 13..24.
        c.choose(0, 2, &mut flag);
        assert_eq!((c.start, c.shown), (25, Some(25)));
        c.choose(0, 1, &mut flag);
        assert_eq!((c.start, c.shown), (13, Some(13)));
        // A named zone, then a type-1000 zone: no cursor.
        c.choose(3, 0, &mut flag);
        assert_eq!(c.shown, Some(25));
        c.choose(1000, 0, &mut flag);
        assert_eq!(c.shown, None);
        c.hide();
        assert_eq!((c.shown, c.start), (None, -1));
    }

    #[test]
    fn texels_under_the_centre() {
        assert_eq!(texel_of([0.0, 0.0, 1.0]), Some((0, 256, 256)));
        assert_eq!(texel_of([1.0, 0.0, 0.0]), Some((5, 256, 256)));
        assert_eq!(texel_of([0.0, -1.0, 0.0]), Some((2, 256, 256)));
        // Top-left corner of ft: texel 1.
        assert_eq!(
            texel_of([-1.0, -1.0, 1.0]).map(|t| (t.1, t.2)),
            Some((1, 1))
        );
        assert_eq!(rgb565(0xff00_00ff), 0x001f);
    }
}
