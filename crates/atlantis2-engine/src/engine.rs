//! The game, as far as it goes: a new game opens on its first view, which
//! the player looks around in; a dialog can be played over the view.

use atlantis2_data::con::NewGame;
use atlantis2_data::dialog::{FontTxt, World};
use atlantis2_data::wam::{self, WamError, Zone};
use atlantis2_data::{A2Files, Folder, Wam};
use atlantis_codecs::DecodeError;

use crate::camera::{deg_to_rad, Camera, WARP_FOV};
use crate::crt::Rand;
use crate::dialog::{Dialog, DialogError, DialogScript, FrameContext, TickContext};
use crate::fpu::Fpu;
use crate::look::{Look, DEFAULT_LIMIT_Y, NO_LIMIT};
use crate::sound::{MusicVolume, Voices};
use crate::subtitle::SubtitleFont;
use crate::warp::{faces_from_hnm, Renderer, Variant};

/// The scheduler's tick: 15 a second (the dialogs' sequencer, the music
/// fades).
pub const TICK: f64 = 1.0 / 15.0;

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

/// The engine.
pub struct Engine {
    files: A2Files,
    wam: Wam,
    wam_name: String,
    view: u32,
    faces: [Vec<u16>; 6],
    renderer: Renderer,
    pub camera: Camera,
    pub look: Look,
    /// The world's dialog scripts (loaded with the world, @ 0x0047fe50).
    script: Option<DialogScript>,
    dialog: Option<Dialog>,
    /// The music's volume and fades.
    pub music: MusicVolume,
    voices: Voices,
    /// The C runtime's `rand` (seeded by the dialog object's `srand`).
    pub rand: Rand,
    font: Option<SubtitleFont>,
    /// The subtitles option (@ 0x005df970, on by default).
    pub subtitles: bool,
    /// The dialog voice volume, 0..127 (@ 0x005df974).
    pub voice_volume: i32,
    /// The scheduler's clock (seconds) and its next tick.
    clock: f64,
    next_tick: f64,
    zbuf: Vec<u32>,
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
        let wam = load_wam(&files, &start.wam)?;
        let camera = Camera::new(fpu, width, height);
        let mut engine = Engine {
            files,
            wam,
            wam_name: start.wam.clone(),
            view: 0,
            faces: Default::default(),
            renderer: Renderer::new(variant, fpu, width as usize, height as usize),
            camera,
            look: Look::default(),
            script: None,
            dialog: None,
            // A music playing at its default volume (no player yet: the
            // fades are kept for when there is one).
            music: MusicVolume::new(fpu, 100, 100, 4),
            voices: Voices::new(),
            rand: Rand::new(0),
            font: None,
            subtitles: true,
            voice_volume: 127,
            clock: 0.0,
            next_tick: 0.0,
            zbuf: vec![0; (width * height) as usize],
        };
        engine.load_world_texts(start.monde);
        engine.enter_view(
            start.view,
            deg_to_rad(engine.camera.fpu, start.yaw_deg),
            deg_to_rad(engine.camera.fpu, start.pitch_deg),
        )?;
        Ok(engine)
    }

    /// Enters a view of the current place (@ 0x00459fe0): loads its cube,
    /// orients the camera to the arrival angles (radians), stops the turn,
    /// sets the 76° field of view; the script resets the limits to ±70° in
    /// pitch and none in yaw at every view change.
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
        log::info!("view {id}: warp {name} (disc {disc})");
        self.view = id;
        self.camera.enter(yaw, pitch);
        let (w, h) = (self.renderer.width() as u32, self.renderer.height() as u32);
        self.camera.set_fov(WARP_FOV, w, h);
        self.look.stop();
        self.look.limit_y = DEFAULT_LIMIT_Y;
        self.look.limit_x = NO_LIMIT;
        Ok(())
    }

    /// The world's dialog texts (`MONDE`'s world change, @ 0x0047fe50) and
    /// the subtitle font (`AtlFont_MainInit` @ 0x00404140).
    fn load_world_texts(&mut self, monde: i32) {
        self.script = World::of_monde(monde).and_then(|w| {
            DialogScript::load(&self.files, w)
                .map_err(|e| log::warn!("dialog scripts: {e}"))
                .ok()
        });
        if self.font.is_none() {
            let font_txt = self
                .files
                .read(Folder::Loc, "Font.txt")
                .map(|d| FontTxt::parse(&d));
            self.font = font_txt.and_then(|f| {
                let file = f.font("AtlFT_Dialog")?.file.clone();
                SubtitleFont::new(self.files.read(Folder::Loc, &file)?, &f)
            });
            if self.font.is_none() {
                log::warn!("loc\\Font.txt or its font missing: no subtitles");
            }
        }
    }

    /// `/set(dial=N)`: `AtlDial_start` (@ 0x0047ff60), a running dialog
    /// stopped first; the views' angles are saved; then the music fades to
    /// 16 % (@ 0x004818f0, after the start, whether it worked or not).
    pub fn start_dialog(&mut self, number: u32) -> Result<(), DialogError> {
        if self.dialog.is_some() {
            log::warn!("SetVariables : AtlDial_start : a dialog is running");
            self.end_dialog();
        }
        self.music.in_dialog = true;
        let script = self
            .script
            .as_ref()
            .ok_or_else(|| DialogError::NoFile("the world's dialog scripts".into()))?;
        let result = Dialog::initialise(number, script, &self.files, &mut self.rand);
        let out = match result {
            Ok(mut d) => {
                let (y, p) = self.camera.angles();
                d.saved_yaw = y;
                d.saved_pitch = p;
                self.dialog = Some(d);
                Ok(())
            }
            Err(e) => {
                self.music.in_dialog = false;
                self.music.set_percent(100);
                Err(e)
            }
        };
        self.music.set_percent(16);
        out
    }

    /// The end of the dialog (@ 0x004808a0): the music back to 100 %.
    pub fn end_dialog(&mut self) {
        let screen = (self.renderer.width() as u32, self.renderer.height() as u32);
        if let Some(mut d) = self.dialog.take() {
            d.end(&mut self.camera, &self.voices, screen);
        }
        self.music.in_dialog = false;
        self.music.set_percent(100);
    }

    /// The running dialog.
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    pub fn dialog_mut(&mut self) -> Option<&mut Dialog> {
        self.dialog.as_mut()
    }

    /// The world's dialog scripts.
    pub fn script(&self) -> Option<&DialogScript> {
        self.script.as_ref()
    }

    /// The subtitle font.
    pub fn subtitle_font(&self) -> Option<&SubtitleFont> {
        self.font.as_ref()
    }

    /// The sound voices, for the sound output to mix.
    pub fn voices(&self) -> &Voices {
        &self.voices
    }

    /// The scheduler's clock, seconds.
    pub fn clock(&self) -> f64 {
        self.clock
    }

    /// Moves the clock to `now` (seconds), running the 15 Hz ticks due
    /// (a stall of more than a second skips the ticks missed).
    pub fn set_clock(&mut self, now: f64) {
        if now - self.next_tick > 1.0 {
            self.next_tick = now;
        }
        while self.next_tick <= now {
            let t = self.next_tick;
            self.tick(t);
            self.next_tick += TICK;
        }
        self.clock = now;
    }

    /// One tick at time `t`: the music fade, the dialog.
    fn tick(&mut self, t: f64) {
        if let Some(att) = self.music.step(t) {
            self.voices.set_attenuation(crate::sound::VOICE_MUSIC, att);
        }
        let screen = (self.renderer.width() as u32, self.renderer.height() as u32);
        let Some(d) = self.dialog.as_mut() else {
            return;
        };
        let mut ctx = TickContext {
            files: &self.files,
            camera: &mut self.camera,
            voices: &self.voices,
            voice_volume: self.voice_volume,
            master: self.music.master,
            clock: t,
            screen,
        };
        if !d.tick(&mut ctx) {
            self.end_dialog();
        }
    }

    /// A click (or a key): during a dialog, the next line.
    pub fn click(&mut self) {
        if let Some(d) = self.dialog.as_mut() {
            d.click();
        }
    }

    /// A mouse move, in mickeys (relative pointer motion).
    pub fn mouse_motion(&mut self, dx: i32, dy: i32) {
        self.look.mouse(self.camera.fpu, dx, dy);
    }

    /// One frame of the panorama loop: the turn. In a dialog only a
    /// `voixoff` line turns (@ 0x00480840).
    pub fn frame(&mut self) {
        if self.dialog.as_ref().is_none_or(|d| d.kind() == 1) {
            self.look.frame(&mut self.camera);
        }
    }

    /// Draws the view into `out` (width x height RGB565). The control word
    /// the renderer leaves is the one the game's float code runs under next.
    /// In a dialog (@ 0x00480840): nothing before its first line; the
    /// panorama for a head or `voixoff` line, then the dialog frame over
    /// it; for `voixoff0` and `cinema` lines the subtitles over what `out`
    /// holds.
    pub fn render(&mut self, out: &mut [u16]) {
        let kind = self.dialog.as_ref().map(Dialog::kind);
        if kind == Some(-1) {
            return;
        }
        if matches!(kind, None | Some(0) | Some(1)) {
            let (m, p) = (self.camera.matrix(), self.camera.projection());
            self.renderer.fpu = self.camera.fpu;
            self.renderer.render(&self.faces, &m, p, out);
            self.camera.fpu = self.renderer.fpu;
        }
        let screen = (self.renderer.width(), self.renderer.height());
        if let Some(d) = self.dialog.as_mut() {
            let mut frame = FrameContext {
                clock: self.clock,
                rand: &mut self.rand,
                font: self.font.as_ref(),
                subtitles: self.subtitles,
                screen,
            };
            d.draw(out, &mut self.zbuf, &mut frame);
        }
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

    /// The view's faces in cube order.
    pub fn faces(&self) -> &[Vec<u16>; 6] {
        &self.faces
    }

    /// The angles read back from the camera, in degrees.
    pub fn angles_deg(&self) -> (f32, f32) {
        let (y, p) = self.camera.angles();
        (y.to_degrees(), p.to_degrees())
    }

    /// The click zone under the screen centre (@ 0x0040c6a0 picks it every
    /// frame, @ 0x00458fe0): its index in the view's list, and the zone.
    pub fn centre_zone(&self) -> Option<(usize, &Zone)> {
        let view = self.wam.view(self.view)?;
        let d = self
            .camera
            .ray(self.camera.cx, self.camera.cy)
            .map(f64::from);
        let i = wam::pick(&view.zones, d)?;
        Some((i, &view.zones[i]))
    }
}

fn load_wam(files: &A2Files, name: &str) -> Result<Wam, EngineError> {
    let data = files
        .read(Folder::Wam, name)
        .ok_or_else(|| EngineError::NoWam(name.to_string()))?;
    Wam::parse(&data).map_err(|e| EngineError::BadWam(name.to_string(), e))
}
