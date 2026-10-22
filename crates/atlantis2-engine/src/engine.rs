//! The game, as far as it goes: a new game opens on its first view, which
//! the player looks around in.

use atlantis2_data::con::NewGame;
use atlantis2_data::wam::{self, WamError, Zone};
use atlantis2_data::{A2Files, Folder, Wam};
use atlantis_codecs::DecodeError;

use crate::camera::{deg_to_rad, Camera, WARP_FOV};
use crate::fpu::Fpu;
use crate::look::{Look, DEFAULT_LIMIT_Y, NO_LIMIT};
use crate::warp::{faces_from_hnm, Renderer};

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
}

impl Engine {
    /// A new game (menu action 10 for a player without a save, @ 0x00486eb0):
    /// chapter 0's `/INIT` chooses the world's disc, the place, the view
    /// and its angles (Tibet, `tibet.wam`, view 2, yaw −90°, pitch 0). The
    /// opening cinematic it then plays, and the rest of the script, are not
    /// run yet: the view is shown at once.
    pub fn new_game(
        mut files: A2Files,
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
            renderer: Renderer::new(fpu, width as usize, height as usize),
            camera,
            look: Look::default(),
        };
        engine.enter_view(
            start.view,
            deg_to_rad(fpu, start.yaw_deg),
            deg_to_rad(fpu, start.pitch_deg),
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

    /// A mouse move, in mickeys (relative pointer motion).
    pub fn mouse_motion(&mut self, dx: i32, dy: i32) {
        self.look.mouse(self.camera.fpu, dx, dy);
    }

    /// One frame of the panorama loop: the turn.
    pub fn frame(&mut self) {
        self.look.frame(&mut self.camera);
    }

    /// Draws the view into `out` (width x height RGB565).
    pub fn render(&mut self, out: &mut [u16]) {
        let (m, p) = (self.camera.matrix(), self.camera.projection());
        self.renderer.render(&self.faces, &m, p, out);
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
