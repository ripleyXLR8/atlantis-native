//! The game from launch to the places: the loop modes of `Game.exe`'s frame
//! task (@ 0x004d5c90) that the first chapters use: the intro (mode 1), the
//! warp (mode 3, `AtlWarp_continue` @ 0x0043ff20) and the videos (mode 4,
//! `AtlHnm_continue` @ 0x00421c10), with the chapter scripts run by
//! *Atlantis II*'s interpreter under *Atlantis III*'s rules
//! (`atlantis2_engine::script::a3`).
//!
//! Time is the caller's: [`Game::step`] is given the seconds since the
//! start, once per displayed frame. The menus are not ported: after the
//! intro, [`Game::new_game`] does what the start page's empty player slot
//! does.

use std::sync::Arc;

use atlantis2_data::defnames::DefNames;
use atlantis2_data::wam::{self, Wam};
use atlantis2_engine::camera::direction_angles;
use atlantis2_engine::look::Look;
use atlantis2_engine::script::{AnimOp, DialStart, Host, Script};
use atlantis2_engine::turn::{Turn, Zoom};
use atlantis2_engine::warp::{A3Raster, FRAME_TO_FACE};
use atlantis2_engine::{Camera, Fpu, Renderer, Variant};
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::A3Files;

use crate::audio::{
    csm_gain, Audio, CHANNELS, CHANNEL_CINEMA, CHANNEL_MUSIC, CHANNEL_PLACE, CHANNEL_SOUND,
};
use crate::cinema::Cinema;
use crate::place_anim::Slots;
use crate::startup::Startup;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::place_sound::{sources, Source};
use atlantis3_data::coord::Coord;
use std::collections::{BTreeMap, HashMap};

/// A decoded sound: rate, channels, interleaved samples.
type Decoded = (u32, u16, Arc<Vec<i16>>);

/// The world folders (@ 0x004150b0..0x004151f0, index = `MONDE`).
pub const WORLDS: [&str; 6] = ["", "hoggar", "ldm", "bagdad", "paleo", "inter"];

/// The warp's field of view, 76°, as the 25 Hz ticks set it (@ 0x004409f0:
/// f32((76 · f32 π) · f32 1/180)).
fn warp_fov(fpu: Fpu) -> f32 {
    let pi = f64::from(f32::from_bits(0x4049_0fdb));
    let k = f64::from(f32::from_bits(0x3bb6_0b61));
    fpu.f32(fpu.mul(fpu.mul(76.0, pi), k))
}

/// The zone aim of *Atlantis III* (@ 0x004634a0): the points summed on the
/// x87 stack and rounded to float once, then their direction's angles.
pub fn zone_aim_a3(fpu: Fpu, points: &[[f32; 3]]) -> (f32, f32) {
    let mut s = [0f64; 3];
    for p in points {
        for (i, v) in s.iter_mut().enumerate() {
            *v = fpu.add(f64::from(p[i]), *v);
        }
    }
    direction_angles(fpu, s.map(|v| fpu.f32(v)))
}

/// Why the game cannot start.
#[derive(Debug)]
pub enum GameError {
    Missing(String),
    Script(String),
}

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameError::Missing(n) => write!(f, "{n}: not found"),
            GameError::Script(e) => write!(f, "script: {e}"),
        }
    }
}

impl std::error::Error for GameError {}

/// The loop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Mode 1: the intro.
    Intro,
    /// Mode 3: the panorama.
    Warp,
    /// Mode 4: a cinematic (CryoMovie) or a travel video (Hnm_Framer).
    Video,
    /// Mode 7: the inventory over the frozen warp.
    Inventory,
    /// Mode 8: the subject menu over the frozen warp.
    Subjects,
    /// Nothing to show (before the intro, after the game stopped).
    Idle,
}

/// A video being played.
struct Playing {
    cinema: Cinema,
    /// When it started (the game's seconds).
    start: f64,
    /// A travel video (Hnm_Framer, silent, 15 fps by the clock), drawn
    /// with the move's transition sprites.
    travel: bool,
    /// The mode to go back to (@ 0x0075609c).
    back: Mode,
    /// The frame drawn last.
    shown: u32,
}

/// A move in progress (the globals @ 0x00756074..0x00756094).
#[derive(Clone, Debug, Default)]
struct Moving {
    turn: Option<Turn>,
    pending: bool,
    next: Option<u32>,
    arrival: (f32, f32),
}

/// The place the player is in.
#[derive(Default)]
struct Place {
    wam: Wam,
    name: String,
    view: Option<u32>,
    faces: [Vec<u16>; 6],
    /// The cube file the faces come from.
    cube: String,
}

/// The game.
pub struct Game {
    files: A3Files,
    pub startup: Startup,
    width: usize,
    height: usize,
    screen: Vec<u16>,
    pub audio: Audio,
    names: DefNames,
    script: Option<Box<Script>>,
    mode: Mode,
    /// The game's clock (seconds) at the last step.
    now: f64,
    /// The 15 Hz tick's last time and the warp step's 25 Hz accumulator
    /// (@ 0x007592a0) and its last time (@ 0x00759298).
    tick_last: f64,
    look_acc: f32,
    look_last: Option<f64>,
    place: Place,
    camera: Camera,
    look: Look,
    renderer: Renderer,
    zoom: Zoom,
    moving: Moving,
    centre: Option<usize>,
    /// The view's place animations, the faces they are drawn into, the
    /// C library's `rand` and the 15 Hz tick count with its "a tick
    /// passed" flag for the animations (@ 0x0077ad78, 0x0077addc).
    slots: Slots,
    spw_cache: HashMap<String, Option<Arc<Bank>>>,
    work: [Vec<u16>; 6],
    rand: Rand,
    ticks: u32,
    anim_tick: bool,
    video: Option<Playing>,
    /// The click flags (@ 0x007592b8, double click @ 0x007592bb) and the
    /// last left press (@ 0x00759280).
    click: bool,
    double_click: bool,
    last_press: Option<f64>,
    /// The cursor: `spr\Interface.spr`, its frame range and frame, and the
    /// time of its last step (@ 0x00756570..0x00756578, 0x00756500).
    interface: Option<Bank>,
    cursor_range: (i32, i32),
    cursor_frame: i32,
    cursor_time: f64,
    /// The music (`STARTMUSIK`): the file playing, to start it again after
    /// a cinematic (@ 0x00433d00).
    music: Option<String>,
    /// The sounds decoded so far, by file.
    sounds: HashMap<String, Option<Decoded>>,
    /// The place's `Coord` file, the listener's position (@ 0x0074e898)
    /// and the active place sounds by object name (the map of
    /// `AtlBruitages_InitWarp` @ 0x00401f30).
    coord: Coord,
    listener: [f32; 3],
    place_sounds: BTreeMap<String, Source>,
    /// A dialog the port does not play: the waiting section goes on at the
    /// next warp frame, as after the dialog's end.
    resume_next: bool,
    /// The free pointer of the inventory and the subject menu
    /// (@ 0x007592b0, 0x007592b4) and the mode they return to
    /// (@ 0x007564e0).
    pointer: (i32, i32),
    prev_mode: Mode,
    inventory: crate::inventory::Inventory,
    /// The world's banks: `<World>Objets.SPR`, `<World>ObjetsEnMain.SPR`,
    /// `<World>SujetsNN.spr` (opened by `MONDE`), `fleche.SPR`.
    objects: Option<Bank>,
    objects_hand: Option<Bank>,
    subjects_bank: Option<Bank>,
    arrows: Option<Bank>,
    banks_world: i32,
    /// The place sprite under the centre when a click was made (the
    /// render's hit, @ 0x0077b670): its slot's character or object id and
    /// click kind.
    sprite_hit: Option<(i32, i32)>,
    /// The script asked to stop.
    pub quit: bool,
    /// What the scripts did that the port does not play yet (logged once
    /// each).
    pub not_ported: Vec<String>,
}

impl Game {
    /// The game of the install `files`: its settings and the start-up's
    /// script part (chapter 0's boot state, the funeral chamber). Nothing
    /// is shown before [`Game::start_intro`] or [`Game::new_game`].
    pub fn new(files: A3Files) -> Result<Game, GameError> {
        let startup = Startup::read(&files);
        let (width, height) = startup.screen();
        let defnames = files
            .read("scenar/defnames.con")
            .ok_or_else(|| GameError::Missing("scenar\\defnames.con".into()))?;
        let names = DefNames::parse(&defnames).map_err(|e| GameError::Script(e.to_string()))?;
        let script = Script::new_a3(&defnames).map_err(GameError::Script)?;
        let fpu = Fpu::Double;
        let mut renderer = Renderer::new(Variant::PII, fpu, width, height);
        renderer.a3 = Some(A3Raster {
            smooth: startup.config.smooth,
        });
        let interface = files
            .read("spr/Interface.spr")
            .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok());
        if interface.is_none() {
            log::warn!("spr\\Interface.spr missing or unreadable: no cursor");
        }
        let look = Look {
            invert_y: startup.config.invert_mouse_y,
            ..Look::default()
        };
        let mut game = Game {
            files,
            startup,
            width,
            height,
            screen: vec![0; width * height],
            audio: Audio::new(),
            names,
            script: Some(Box::new(script)),
            mode: Mode::Idle,
            now: 0.0,
            tick_last: 0.0,
            look_acc: 0.0,
            look_last: None,
            place: Place::default(),
            camera: Camera::new(fpu, width as u32, height as u32),
            look,
            renderer,
            zoom: Zoom::default(),
            moving: Moving::default(),
            centre: None,
            slots: Slots::default(),
            spw_cache: HashMap::new(),
            work: Default::default(),
            rand: Rand::new(0),
            ticks: 0,
            anim_tick: false,
            video: None,
            click: false,
            double_click: false,
            last_press: None,
            interface,
            cursor_range: (0, 0),
            cursor_frame: 0,
            cursor_time: 0.0,
            resume_next: false,
            music: None,
            sounds: HashMap::new(),
            coord: Coord::default(),
            listener: [0.0; 3],
            place_sounds: BTreeMap::new(),
            pointer: (0, 0),
            prev_mode: Mode::Warp,
            inventory: Default::default(),
            objects: None,
            objects_hand: None,
            subjects_bank: None,
            arrows: None,
            banks_world: 0,
            sprite_hit: None,
            quit: false,
            not_ported: Vec::new(),
        };
        if !game.with_script(|s, h| s.boot_a3(h)) {
            return Err(GameError::Script(game.script_error()));
        }
        Ok(game)
    }

    fn script_error(&self) -> String {
        self.script
            .as_ref()
            .and_then(|s| s.fatal.clone())
            .unwrap_or_default()
    }

    /// Runs `f` with the script and the game as its host.
    fn with_script<R: Default>(&mut self, f: impl FnOnce(&mut Script, &mut dyn Host) -> R) -> R {
        let Some(mut s) = self.script.take() else {
            return R::default();
        };
        let r = f(&mut s, self);
        if let Some(e) = &s.fatal {
            log::error!("script: {e}");
        }
        self.quit |= s.quit;
        self.script = Some(s);
        r
    }

    /// The script, for tests.
    pub fn script(&self) -> Option<&Script> {
        self.script.as_deref()
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The screen as last drawn (RGB565, `width` x `height`).
    pub fn screen(&self) -> &[u16] {
        &self.screen
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The place's file name and the current view.
    pub fn place(&self) -> (&str, Option<u32>) {
        (&self.place.name, self.place.view)
    }

    pub fn wam(&self) -> &Wam {
        &self.place.wam
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// The camera's angles in degrees (yaw, pitch).
    pub fn angles_deg(&self) -> (f32, f32) {
        let (y, p) = self.camera.angles();
        (y.to_degrees(), p.to_degrees())
    }

    /// The zone under the screen centre (its index in the view).
    pub fn centre_zone(&self) -> Option<usize> {
        self.centre
    }

    /// The video playing: its name and the frames shown.
    pub fn video(&self) -> Option<(&str, u32)> {
        self.video
            .as_ref()
            .map(|v| (v.cinema.name(), v.cinema.frames_shown()))
    }

    /// Whether a move turns the view.
    pub fn turning(&self) -> bool {
        self.moving.turn.is_some()
    }

    /// The intro (mode 1, @ 0x00459360): `intro.hnm` from `hnm\` at 15
    /// frames a second, no caption (`Rien.txt` does not exist).
    pub fn start_intro(&mut self) {
        let lang = self.startup.config.langue.clone();
        if lang != b"fra" && !lang.is_empty() {
            log::warn!(
                "intro: the localised intros ({}) are not ported: intro.hnm",
                String::from_utf8_lossy(&lang)
            );
        }
        if !self.start_video("hnm/intro.hnm", 15, false, Mode::Idle) {
            self.mode = Mode::Idle;
        }
        self.mode = if self.video.is_some() {
            Mode::Intro
        } else {
            Mode::Idle
        };
    }

    /// The start page's new game for an empty player slot (@ 0x00452b98):
    /// the flags and the bag cleared, `(chapter=600)`, then the warp. The
    /// first warp frame's arrival scan plays chapter 1's three cinematics.
    pub fn new_game(&mut self) -> bool {
        self.video = None;
        self.audio.stop(CHANNEL_CINEMA);
        self.mode = Mode::Warp;
        let ok = self.with_script(|s, h| s.new_game_a3(h));
        self.look_last = None;
        ok
    }

    /// Space or Escape (`WM_KEYDOWN` in the cinematic's wait loop
    /// @ 0x00421c5b): the cinematic stops. Travel videos ignore keys.
    pub fn skip(&mut self) {
        if self.video.as_ref().is_some_and(|v| !v.travel) {
            self.end_video();
        }
    }

    /// A mouse move of (dx, dy) mickeys (@ 0x004313a0): in the warp, not
    /// turning, the turn speeds gain dx / 2000 and lose dy / 2000; the
    /// centre zone is picked again.
    pub fn mouse_motion(&mut self, dx: i32, dy: i32) {
        if matches!(self.mode, Mode::Inventory | Mode::Subjects) {
            // The free pointer, clamped to the screen (@ 0x004313f6).
            self.pointer.0 = (self.pointer.0 + dx).clamp(0, self.width as i32 - 1);
            self.pointer.1 = (self.pointer.1 + dy).clamp(0, self.height as i32 - 1);
            return;
        }
        if self.mode != Mode::Warp || self.moving.turn.is_some() {
            return;
        }
        self.look.mouse(self.camera.fpu, dx, dy);
        self.centre = self.pick_centre();
    }

    /// The left button pressed (@ 0x00431580): the click flag, and the
    /// double-click flag within 0.2 s of the previous press. In a travel
    /// video a click ends it.
    pub fn click(&mut self) {
        self.click = true;
        if self.last_press.is_some_and(|t| self.now - t < 0.2) {
            self.double_click = true;
        }
        // The script reads the same flags (`Script::click`).
        if let Some(s) = self.script.as_mut() {
            s.click = true;
            s.double_click |= self.double_click;
        }
        self.last_press = Some(self.now);
    }

    /// The free pointer (inventory, subject menu).
    pub fn pointer(&self) -> (i32, i32) {
        self.pointer
    }

    /// Puts the free pointer at (x, y) (tests; the window's absolute
    /// mouse).
    pub fn set_pointer(&mut self, x: i32, y: i32) {
        self.pointer = (
            x.clamp(0, self.width as i32 - 1),
            y.clamp(0, self.height as i32 - 1),
        );
    }

    /// The right button (@ 0x00431639..0x00431676): in the warp the
    /// inventory opens (unless it is open), in the inventory it closes, in
    /// the subject menu the menu closes.
    pub fn right_click(&mut self) {
        match self.mode {
            Mode::Warp if self.moving.turn.is_none() && !self.moving.pending => {
                self.open_inventory()
            }
            Mode::Inventory => self.close_inventory(),
            Mode::Subjects => self.close_subjects(),
            _ => {}
        }
    }

    /// Whether the inventory is open.
    pub fn inventory_open(&self) -> bool {
        self.inventory.open
    }

    /// The bag entries the inventory shows (the current world's).
    pub fn inventory_items(&self) -> Vec<atlantis2_engine::script::BagEntry> {
        self.script
            .as_ref()
            .map(|s| crate::inventory::shown(&s.bag, s.a3.world))
            .unwrap_or_default()
    }

    /// The object in the hand (`OBJETENMAIN`), 0 for none.
    pub fn object_in_hand(&self) -> i32 {
        self.script
            .as_ref()
            .map_or(0, |s| s.vars[s.keys.objetenmain])
    }

    /// The world's sprite banks, opened again when the world changed.
    fn world_banks(&mut self) {
        let world = self.script.as_ref().map_or(1, |s| s.a3.world);
        if world == self.banks_world {
            return;
        }
        self.banks_world = world;
        let open = |files: &A3Files, path: String| {
            let b = files
                .read(&path)
                .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok());
            if b.is_none() {
                log::warn!("{path}: not found");
            }
            b
        };
        let name = crate::inventory::world_bank_name(world);
        self.objects = name.and_then(|n| open(&self.files, format!("spr/{n}Objets.SPR")));
        self.objects_hand =
            name.and_then(|n| open(&self.files, format!("spr/{n}ObjetsEnMain.SPR")));
        let lay = crate::subjects::Layout::for_width(self.width);
        self.subjects_bank = lay.bank_path(world).and_then(|p| open(&self.files, p));
        if self.arrows.is_none() {
            self.arrows = open(&self.files, "spr/fleche.SPR".into());
        }
    }

    /// Opens the inventory (@ 0x00427a60): mode 7, the turn speeds
    /// zeroed, the hand emptied, alpha 0, the fade timer restarted.
    fn open_inventory(&mut self) {
        self.world_banks();
        self.prev_mode = self.mode;
        self.mode = Mode::Inventory;
        self.look.stop();
        if let Some(s) = self.script.as_mut() {
            s.empty_hand();
        }
        self.inventory.open = true;
        self.inventory.alpha = 0.0;
        self.inventory.fade_time = self.now;
        self.click = false;
    }

    /// Closes the inventory (@ 0x00427b20): the mode back; the items fade
    /// out over the warp.
    fn close_inventory(&mut self) {
        self.inventory.open = false;
        self.inventory.fade_time = self.now;
        self.mode = self.prev_mode;
        self.look.stop();
        self.click = false;
        self.look_last = None;
    }

    /// Opens the subject menu for `character` (@ 0x00440530 →
    /// @ 0x00427b80): `CLICPERSO`, mode 8, the pointer at the centre.
    fn open_subjects(&mut self, character: i32) {
        self.world_banks();
        if let Some(s) = self.script.as_mut() {
            let k = s.keys.clicperso;
            s.vars[k] = character;
        }
        self.prev_mode = self.mode;
        self.mode = Mode::Subjects;
        self.pointer = ((self.width / 2) as i32, (self.height / 2) as i32);
    }

    /// Closes the subject menu (@ 0x00427be0): the mode back, the turn
    /// speeds zeroed, the hand emptied, the click flags cleared.
    fn close_subjects(&mut self) {
        self.mode = self.prev_mode;
        self.look.stop();
        if let Some(s) = self.script.as_mut() {
            s.empty_hand();
            s.click = false;
        }
        self.click = false;
        self.double_click = false;
        self.look_last = None;
    }

    /// Whether the subject menu is open.
    pub fn subjects_open(&self) -> bool {
        self.mode == Mode::Subjects
    }

    /// A sprite drawn centred on (x, y) at an alpha (`AtlSpr_AffCentre`
    /// @ 0x00436b80 → `AtlSpr_Aff` @ 0x004367b0).
    fn aff_centre(
        screen: &mut [u16],
        w: usize,
        h: usize,
        bank: &Bank,
        idx: usize,
        x: i32,
        y: i32,
        alpha: f32,
    ) {
        let Some(info) = bank.info(idx) else { return };
        let x = x - i32::from(info.w) / 2;
        let y = y - i32::from(info.h) / 2;
        Self::aff(screen, w, h, bank, idx, x, y, alpha);
    }

    /// `AtlSpr_Aff`: `AFF_SPR` at alpha ≥ 1, `AFF_SPRTRS` below, nothing
    /// at 0.
    #[allow(clippy::too_many_arguments)]
    fn aff(
        screen: &mut [u16],
        w: usize,
        h: usize,
        bank: &Bank,
        idx: usize,
        x: i32,
        y: i32,
        alpha: f32,
    ) {
        let mut t = Target::screen(screen, w, h);
        // The game's target: maxima are the size (formats/spr.md).
        t.xmax = w as i32;
        t.ymax = h as i32;
        match crate::inventory::trs_level(alpha) {
            None => {}
            Some(None) => {
                bank.draw(&mut t, Some(x), y, idx);
            }
            Some(Some(l)) => {
                bank.draw_trs(&mut t, Some(x), y, idx, l);
            }
        }
    }

    /// The inventory's items, bar and arrows at its alpha (@ 0x004299b0,
    /// @ 0x00428fd0).
    fn draw_inventory_items(&mut self) {
        let alpha = self.inventory.alpha;
        let (w, h) = (self.width, self.height);
        let items = self.inventory_items();
        let (n, margin) = crate::inventory::slots(w);
        if let Some(bank) = &self.objects {
            for (j, e) in items
                .iter()
                .skip(self.inventory.scroll)
                .take(n as usize)
                .enumerate()
            {
                let x = margin + crate::inventory::SLOT * j as i32 + 40;
                Self::aff_centre(
                    &mut self.screen,
                    w,
                    h,
                    bank,
                    usize::from(e.sprite),
                    x,
                    h as i32 - 40,
                    alpha,
                );
            }
        }
        if let Some(bank) = &self.interface {
            Self::aff_centre(
                &mut self.screen,
                w,
                h,
                bank,
                0,
                (w / 2) as i32,
                h as i32 - 39,
                alpha,
            );
        }
        if let Some(bank) = &self.arrows {
            if self.inventory.scroll > 0 {
                Self::aff(&mut self.screen, w, h, bank, 0, 0, h as i32 - 110, alpha);
            }
            if self.inventory.scroll + (n as usize) < items.len() {
                Self::aff(
                    &mut self.screen,
                    w,
                    h,
                    bank,
                    1,
                    w as i32 - 16,
                    h as i32 - 110,
                    alpha,
                );
            }
        }
    }

    /// The pointer `Interface.spr` #40 at the free pointer (@ 0x004281e0).
    fn draw_pointer(&mut self) {
        if let Some(bank) = &self.interface {
            let (w, h) = (self.width, self.height);
            Self::aff(
                &mut self.screen,
                w,
                h,
                bank,
                40,
                self.pointer.0,
                self.pointer.1,
                1.0,
            );
        }
    }

    /// A frame of the inventory or the subject menu (@ 0x00427c40): the
    /// warp drawn, not stepped, without its cursor; the overlay; the
    /// pointer; then the pick.
    fn overlay_frame(&mut self) {
        self.draw_warp_only();
        if self.mode == Mode::Inventory {
            self.draw_inventory_items();
            self.draw_pointer();
            let items = self.inventory_items();
            self.inventory
                .scroll_step(self.width, items.len(), self.pointer.0, self.now);
            if std::mem::take(&mut self.click) {
                if let Some(e) = self
                    .inventory
                    .slot_at(self.width, self.height, self.pointer)
                    .and_then(|i| items.get(i))
                {
                    // @ 0x00428380: the inventory closed, the item in the
                    // hand.
                    let id = e.id;
                    self.close_inventory();
                    if let Some(s) = self.script.as_mut() {
                        s.hold(id);
                    }
                }
            }
            self.inventory.fade_step(self.now);
        } else {
            self.draw_subjects();
            self.draw_pointer();
            if std::mem::take(&mut self.click) {
                let lay = crate::subjects::Layout::for_width(self.width);
                let r = self.script.as_ref().map_or(-2, |s| {
                    lay.pick(&s.subjects, s.vars[s.keys.clicperso], self.pointer)
                });
                if r > -1 {
                    self.close_subjects();
                    self.sprite_hit = None;
                    self.with_script(|s, h| {
                        let k = s.keys.clone();
                        s.vars[k.sujet] = r;
                        s.subject_scan(h);
                        s.vars[k.clicperso] = 0;
                        s.vars[k.sujet] = -2;
                    });
                }
            }
        }
    }

    /// The subjects of `CLICPERSO` (@ 0x0043b850).
    fn draw_subjects(&mut self) {
        let lay = crate::subjects::Layout::for_width(self.width);
        let Some(s) = self.script.as_ref() else {
            return;
        };
        let cells = lay.cells(&s.subjects.list(s.vars[s.keys.clicperso]));
        let (w, h) = (self.width, self.height);
        for c in cells {
            if let (Some(bank), Ok(v)) = (&self.subjects_bank, usize::try_from(c.value)) {
                Self::aff_centre(&mut self.screen, w, h, bank, v, c.x, c.y, c.alpha);
            }
            if let Some(bank) = &self.interface {
                Self::aff_centre(
                    &mut self.screen,
                    w,
                    h,
                    bank,
                    lay.ring,
                    c.ring.0,
                    c.ring.1,
                    1.0,
                );
            }
        }
    }

    /// One displayed frame at the game's time `now` (seconds).
    pub fn step(&mut self, now: f64) {
        self.now = now;
        // The 15 Hz tick (@ 0x0043e550): the scripts' clocks.
        while now - self.tick_last >= 1.0 / 15.0 {
            let last = self.tick_last;
            self.tick_last += 1.0 / 15.0;
            let t = self.tick_last;
            if let Some(s) = self.script.as_mut() {
                s.tick(t, last);
            }
            self.ticks = self.ticks.wrapping_add(1);
            self.anim_tick = true;
        }
        match self.mode {
            Mode::Intro | Mode::Video => self.video_step(),
            Mode::Warp => self.warp_frame(),
            Mode::Inventory | Mode::Subjects => self.overlay_frame(),
            Mode::Idle => {}
        }
    }

    // ---- videos -------------------------------------------------------

    /// `AtlHnm_start`: opens `path` (under `Datas\`) and switches to the
    /// video mode; false when the file is missing or unreadable.
    fn start_video(&mut self, path: &str, fps: u32, travel: bool, back: Mode) -> bool {
        let Some(data) = self.files.read(path) else {
            log::warn!("{path}: not found");
            return false;
        };
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let cinema = match Cinema::open(&name, Arc::from(data), fps) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("{e}");
                return false;
            }
        };
        if let Some(f) = cinema.audio_format() {
            self.audio.start_stream(
                CHANNEL_CINEMA,
                f.sample_rate,
                f.channels,
                self.cinema_gain(),
            );
        }
        self.video = Some(Playing {
            cinema,
            start: self.now,
            travel,
            back,
            shown: 0,
        });
        self.mode = Mode::Video;
        // The first frame at once.
        self.video_step();
        true
    }

    /// CryoMovie's volume (@ 0x006b9100 with `GlobalVolume`, 0..99).
    fn cinema_gain(&self) -> f32 {
        self.startup.config.global_volume as f32 / 99.0
    }

    fn video_step(&mut self) {
        let Some(v) = self.video.as_mut() else {
            self.mode = Mode::Idle;
            return;
        };
        if v.travel && std::mem::take(&mut self.click) {
            // A click or a double click ends a travel video.
            self.double_click = false;
            self.end_video();
            return;
        }
        let elapsed_ms = ((self.now - v.start) * 1000.0).max(0.0) as u64;
        let mut advanced = false;
        while v.cinema.frames_shown() == 0 || v.cinema.next_due_ms() <= elapsed_ms {
            if !v.cinema.advance() {
                break;
            }
            advanced = true;
            let pcm = v.cinema.take_audio();
            if !pcm.is_empty() {
                self.audio.push(CHANNEL_CINEMA, &pcm);
            }
        }
        if advanced {
            v.shown = v.cinema.frames_shown();
            let (w, h) = v.cinema.size();
            stretch_video(
                v.cinema.frame(),
                w,
                h,
                &mut self.screen,
                self.width,
                self.height,
            );
        }
        if v.cinema.ended() {
            self.end_video();
        }
    }

    /// The end of a video (@ 0x00422090): the mode back, then
    /// `Interp_CloseInterrupt(1)`: a section waiting for it goes on.
    fn end_video(&mut self) {
        let Some(v) = self.video.take() else {
            return;
        };
        self.audio.stop(CHANNEL_CINEMA);
        self.mode = v.back;
        if !v.travel {
            // The music starts again from its beginning (@ 0x00433d00).
            if let Some(m) = self.music.clone() {
                self.play_music(&m);
            }
        }
        if v.back == Mode::Idle {
            return;
        }
        self.click = false;
        self.with_script(|s, h| s.close_interrupt(h));
        self.look_last = None;
    }

    // ---- the warp ------------------------------------------------------

    /// `AtlWarp_continue` (@ 0x0043ff20).
    fn warp_frame(&mut self) {
        if std::mem::take(&mut self.resume_next) {
            self.with_script(|s, h| {
                s.dialog_ended(h);
            });
        }
        // 1. The step: 25 Hz ticks of the look or the turn, the centre pick.
        self.warp_step();
        // 2. The frame drawn.
        self.draw_warp();
        // 3. The script's waits.
        if self.with_script(|s, h| s.check_waits(h)) {
            self.click = false;
            return;
        }
        // 4. Not turning: the click, `Interp_GereFrame`.
        if self.moving.turn.is_none() && !self.moving.pending {
            if self.click {
                self.sprite_clicks();
            }
            if self.click && self.mode == Mode::Warp {
                self.zone_click();
            }
            let (yaw, pitch) = self.camera.angles();
            self.with_script(|s, h| s.gere_frame(h, yaw, pitch));
            if self.mode == Mode::Warp {
                self.click_scan();
            }
            if !matches!(self.mode, Mode::Warp | Mode::Subjects) {
                self.click = false;
                self.double_click = false;
                return;
            }
            self.place_sounds_frame();
        }
        // 6. The click flags cleared.
        self.click = false;
        if let Some(s) = self.script.as_mut() {
            s.click = false;
        }
        // 7. A pending move completes.
        if self.moving.pending {
            self.complete_move();
        }
        self.double_click = false;
        // 8. `CLICZONE` = 0.
        if let Some(s) = self.script.as_mut() {
            let k = s.keys.cliczone;
            s.vars[k] = 0;
        }
    }

    /// @ 0x004317a0: `dt` since the last step (at most 0.5 s) × 25 into the
    /// accumulator; per whole tick, a turn step or the look's inertia and
    /// limits, and the 76° field of view; then the centre pick.
    fn warp_step(&mut self) {
        let dt = match self.look_last {
            Some(t) => (self.now - t).min(0.5),
            None => 0.0,
        };
        self.look_last = Some(self.now);
        let fpu = self.camera.fpu;
        self.look_acc = fpu.f32(fpu.add(f64::from(self.look_acc), fpu.mul(dt, 25.0)));
        if let Some(s) = self.script.as_ref() {
            self.look.limit_x = s.vars[s.keys.limiteanglewarpx];
            self.look.limit_y = s.vars[s.keys.limiteanglewarpy];
        }
        let (w, h) = (self.width as u32, self.height as u32);
        while self.look_acc >= 1.0 {
            self.look_acc -= 1.0;
            if let Some(turn) = self.moving.turn.as_mut() {
                self.camera.set_fov(warp_fov(self.camera.fpu), w, h);
                let step = turn.step(&mut self.camera, &mut self.zoom, (w, h));
                if !step.going {
                    self.moving.turn = None;
                    self.moving.pending = self.moving.next.is_some();
                    return;
                }
            } else {
                self.look.frame(&mut self.camera);
                self.camera.set_fov(warp_fov(self.camera.fpu), w, h);
            }
        }
        self.centre = self.pick_centre();
    }

    /// The zone under the screen centre (@ 0x00463310 = *Atlantis II*'s
    /// pick).
    fn pick_centre(&self) -> Option<usize> {
        let view = self.place.wam.view(self.place.view?)?;
        let d = self
            .camera
            .ray(self.camera.cx, self.camera.cy)
            .map(f64::from);
        wam::pick(&view.zones, d)
    }

    /// The warp drawn into the screen (@ 0x00440690), then the cursor
    /// (@ 0x00440a30).
    fn draw_warp(&mut self) {
        if !self.draw_warp_only() {
            return;
        }
        // The inventory fading out after it closed (@ 0x00427c40).
        if !self.inventory.open && self.inventory.alpha > 0.0 {
            self.draw_inventory_items();
            self.inventory.fade_step(self.now);
        }
    }

    /// The warp frame without the overlays (@ 0x00440690) and the cursor
    /// (@ 0x00440a30); false when there is no cube.
    fn draw_warp_only(&mut self) -> bool {
        if self.place.faces.iter().any(|f| f.is_empty()) {
            return false;
        }
        let m = self.camera.matrix();
        let proj = self.camera.projection();
        if self.slots.slots.iter().any(|s| s.shown) {
            for (w, f) in self.work.iter_mut().zip(&self.place.faces) {
                w.clone_from(f);
            }
            self.slots.draw(&mut self.work, self.ticks);
            self.renderer.render(&self.work, &m, proj, &mut self.screen);
        } else {
            self.renderer
                .render(&self.place.faces, &m, proj, &mut self.screen);
        }
        // The animations' step, once per 15 Hz tick (@ 0x00439c90).
        if std::mem::take(&mut self.anim_tick) {
            self.slots.step(self.now, &mut self.rand);
        }
        // The renderer leaves the control word 0x0e7f.
        self.camera.fpu = self.renderer.fpu;
        if let Some(s) = self.script.as_mut() {
            s.set_fpu(self.renderer.fpu);
        }
        if self.mode == Mode::Warp {
            self.draw_cursor();
        }
        true
    }

    /// @ 0x00440a30 → @ 0x00428060: the cursor of the zone under the
    /// centre, centred on the screen's centre, animated at 15 frames a
    /// second of real time; none without a zone.
    fn draw_cursor(&mut self) {
        let zone_kind = self
            .centre
            .and_then(|i| self.place.wam.view(self.place.view?)?.zones.get(i))
            .map(|z| z.kind);
        if self.startup.config.cursor {
            // The cross (`Cursor`, @ 0x00440a6a..0x00440b6c): five pixels
            // up and across through the centre, white, blue on a type-1
            // zone, red on type 2, magenta on 3..999.
            let argb: u32 = match zone_kind {
                Some(1) => 0xff00_00ff,
                Some(2) => 0xffff_0000,
                Some(3..=999) => 0xffff_00ff,
                _ => 0xffff_ffff,
            };
            let px = (((argb >> 19) & 0x1f) << 11 | ((argb >> 10) & 0x3f) << 5 | (argb >> 3) & 0x1f)
                as u16;
            let (cx, cy) = (self.width / 2, self.height / 2);
            for k in 0..5 {
                self.screen[(cy + k - 2) * self.width + cx] = px;
                self.screen[cy * self.width + cx + k - 2] = px;
            }
        }
        // The object in the hand, centred on the screen (@ 0x00428500,
        // `<World>ObjetsEnMain.SPR`), in place of the zone cursor.
        if let Some(sprite) = self
            .script
            .as_ref()
            .filter(|s| s.vars[s.keys.objetenmain] != 0 && s.cursor_objects)
            .map(|s| s.cursor_sprite)
        {
            self.world_banks();
            if let (Some(bank), Ok(i)) = (&self.objects_hand, usize::try_from(sprite)) {
                let (w, h) = (self.width, self.height);
                Self::aff_centre(
                    &mut self.screen,
                    w,
                    h,
                    bank,
                    i,
                    (w / 2) as i32,
                    (h / 2) as i32,
                    1.0,
                );
            }
            return;
        }
        let Some(kind) = self
            .centre
            .and_then(|i| self.place.wam.view(self.place.view?)?.zones.get(i))
            .map(|z| z.kind)
        else {
            return;
        };
        let range = match kind {
            1 | 2 => (1, 13),
            3..=999 => (25, 40),
            k if k >= 1000 => (25, 40),
            _ => self.cursor_range,
        };
        if range != self.cursor_range {
            self.cursor_range = range;
            self.cursor_frame = range.0;
        }
        if kind < 1000 {
            if let Some(bank) = &self.interface {
                let idx = self.cursor_frame.max(0) as usize;
                if let Some(info) = bank.info(idx) {
                    let x = (self.width / 2) as i32 - info.w as i32 / 2;
                    let y = (self.height / 2) as i32 - info.h as i32 / 2;
                    let mut t = Target::screen(&mut self.screen, self.width, self.height);
                    bank.draw(&mut t, Some(x), y, idx);
                }
            }
        }
        if self.now - self.cursor_time >= 1.0 / 15.0 {
            self.cursor_frame += 1;
            self.cursor_time = self.now;
            if self.cursor_frame >= self.cursor_range.1 {
                self.cursor_frame = self.cursor_range.0;
            }
        }
    }

    /// The place-sprite clicks of a warp frame: @ 0x004403e0 (click kind 1
    /// with an empty hand: the object taken — `CLICOBJET`, hidden, in the
    /// bag and in the hand; 3: `CLICOBJET`; 4: `CLICPERSO`), then
    /// @ 0x00440530 (kind 2, a character with subjects, an empty hand: the
    /// subject menu).
    fn sprite_clicks(&mut self) {
        let Some((id, kind)) = self.sprite_hit else {
            return;
        };
        let hand = self.object_in_hand();
        match kind {
            1 if hand == 0 => {
                let sprite = usize::try_from(id).map_or(0, |i| {
                    self.names.param_of(atlantis2_data::defnames::OBJETS, i)
                });
                self.with_script(|s, h| s.take_object(h, id, sprite as u8));
            }
            3 => self.set_script_var(|k| k.clicobjet, id),
            4 => self.set_script_var(|k| k.clicperso, id),
            _ => {}
        }
        if kind == 2 && hand == 0 {
            let n = self.script.as_ref().map_or(0, |s| s.subjects.count(id));
            if n > 0 {
                self.open_subjects(id);
            }
        }
    }

    fn set_script_var(&mut self, key: impl Fn(&atlantis2_engine::script::Keys) -> usize, v: i32) {
        if let Some(s) = self.script.as_mut() {
            let k = key(&s.keys);
            s.vars[k] = v;
        }
    }

    /// The click scan (@ 0x004405b0), on a click with neither the subject
    /// menu nor the inventory open: `CLICPERSO` = the character of a kind
    /// 2 sprite hit, `CtrlScenarIN` and the timer scans, then `CLICPERSO`
    /// = `CLICOBJET` = 0, `SUJET` = −2, the click flags cleared.
    fn click_scan(&mut self) {
        if !self.click {
            return;
        }
        let character = self.sprite_hit.filter(|h| h.1 == 2).map(|h| h.0);
        self.with_script(|s, h| s.click_scan(h, character));
        self.click = false;
        self.sprite_hit = None;
    }

    /// The zone click (@ 0x00420330): a move zone's departure scan, then
    /// the move; `CLICZONE` for the others.
    fn zone_click(&mut self) {
        let Some(zone) = self
            .centre
            .and_then(|i| self.place.wam.view(self.place.view?)?.zones.get(i))
            .cloned()
        else {
            return;
        };
        if self.sprite_hit.is_some() {
            // A place sprite clicked this frame: `CLICZONE` = the zone's
            // value, whatever its type.
            self.set_script_var(|k| k.cliczone, zone.value);
            return;
        }
        let target = matches!(zone.kind, 1 | 2).then_some(zone.value);
        let go = self.with_script(|s, h| s.departure_scan(h, target));
        if !go {
            return;
        }
        if zone.kind > 2 && zone.kind < 88888 {
            if let Some(s) = self.script.as_mut() {
                let k = s.keys.cliczone;
                s.vars[k] = zone.value;
            }
        }
        match zone.kind {
            1 => self.move_setup(&zone),
            2 => self.not_ported("a link to another place (zone type 2)"),
            _ => {}
        }
    }

    /// The move's set-up (@ 0x00420490, *Atlantis II*'s FUN_0049cc40).
    fn move_setup(&mut self, zone: &wam::Zone) {
        let fpu = self.camera.fpu;
        let target = zone.value as u32;
        let next = self.script.as_mut().and_then(|s| {
            let k = s.keys.nextvue;
            (s.vars[k] != 0).then(|| {
                s.vars[k] = 0;
                s.next_view
            })
        });
        self.moving.next = match next {
            Some(v) => v.and_then(|v| self.place.wam.view(v as u32)).map(|v| v.id),
            None => self.place.wam.view(target).map(|v| v.id),
        };
        let aim = || zone_aim_a3(fpu, &zone.points);
        if self.moving.next.is_none() {
            let (y, p) = aim();
            self.start_turn(y, p);
            return;
        }
        let dests: Vec<_> = self
            .place
            .view
            .and_then(|v| self.place.wam.view(v))
            .map(|v| {
                v.destinations
                    .iter()
                    .filter(|d| d.target == target)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut start = (999.0f32, 0.0f32);
        for d in &dests {
            if d.angles_as_given == 0 {
                start = if d.start_yaw <= 998.0 {
                    (d.start_yaw, d.start_pitch)
                } else {
                    aim()
                };
                self.moving.arrival = if d.end_yaw <= 998.0 {
                    (d.end_yaw, d.end_pitch)
                } else {
                    start
                };
                if !self.start_turn(start.0, start.1) {
                    self.moving.pending = true;
                }
            } else {
                start = (d.start_yaw, d.start_pitch);
                self.moving.arrival = (d.end_yaw, d.end_pitch);
                self.start_turn(start.0, start.1);
            }
        }
        if start.0 != 999.0 {
            return;
        }
        let (y, p) = aim();
        self.start_turn(y, p);
        self.moving.arrival = (y, p);
    }

    /// The turn's set-up (@ 0x004410b0); false when none is needed.
    fn start_turn(&mut self, yaw: f32, pitch: f32) -> bool {
        let (turn, turning) = Turn::new_a3(&self.camera, yaw, pitch, &self.zoom);
        self.moving.turn = turning.then_some(turn);
        turning
    }

    /// The move's end (@ 0x0043ff20 @ 0x004401d4..): the new view entered
    /// at the arrival angles, then its travel video.
    fn complete_move(&mut self) {
        self.moving.pending = false;
        let Some(next) = self.moving.next.take() else {
            return;
        };
        let old = self.place.view;
        let (yaw, pitch) = self.moving.arrival;
        self.with_script(|s, _| s.arrive(next as i32));
        self.enter_view(next, yaw, pitch);
        if let Some(mut s) = self.script.take() {
            self.place_hooks(&mut s);
            self.script = Some(s);
        }
        let dest = old.and_then(|old| {
            self.place
                .wam
                .places
                .iter()
                .flat_map(|p| p.views.iter())
                .filter(|v| v.id == old)
                .find_map(|v| v.destinations.iter().find(|d| d.target == next))
                .cloned()
        });
        if let (Some(d), false) = (dest, self.double_click) {
            let world = self.script.as_ref().map_or(1, |s| s.a3.world);
            let folder = WORLDS.get(world as usize).copied().unwrap_or("");
            let path = format!("hnmtraj/HNM640/{folder}/{}", d.video);
            if self.files.size(&path).is_some() {
                self.start_video(&path, 15, true, Mode::Warp);
            }
        }
    }

    /// `AtlWarp_EnterVue` (@ 0x0043f840): the cube of the view loaded
    /// (`warp\Warp512\<world>\<name>`, frames to faces 1, 3, 0, 4, 5, 2),
    /// the camera turned to the arrival angles, the turn speeds stopped.
    fn enter_view(&mut self, id: u32, yaw: f32, pitch: f32) {
        let Some(view) = self.place.wam.view(id) else {
            log::error!("{}: no view {id}", self.place.name);
            return;
        };
        let name = view.name.clone();
        let world = self.script.as_ref().map_or(1, |s| s.a3.world);
        let folder = WORLDS.get(world as usize).copied().unwrap_or("");
        let path = format!("warp/Warp512/{folder}/{name}");
        if self.place.cube != path {
            match self.files.read(&path).map(|d| faces_from_hnm(&d)) {
                Some(Ok(faces)) => self.place.faces = faces,
                Some(Err(e)) => log::error!("{path}: {e:?}"),
                None => {
                    // The loader's fallback fills the faces with debug
                    // colours; nothing is drawn here.
                    log::error!("{path}: not found");
                    self.place.faces = Default::default();
                }
            }
            self.place.cube = path.clone();
        }
        log::info!("view {id}: {path}");
        self.place.view = Some(id);
        self.place_sounds_enter(id);
        let shown = self
            .script
            .as_ref()
            .map(|s| s.shown.clone())
            .unwrap_or_default();
        self.slots = Slots::load(
            &self.files,
            &mut self.spw_cache,
            &self.place.wam,
            id,
            folder,
            &mut self.rand,
            self.now,
            |id, kind| {
                shown
                    .iter()
                    .find(|r| r.id == id && r.kind == kind)
                    .map(|r| r.shown)
            },
        );
        self.camera.enter(yaw, pitch);
        self.look.stop();
        self.moving.turn = None;
        self.look_last = None;
        self.look_acc = 0.0;
        self.centre = self.pick_centre();
    }

    /// A sound file decoded (cached); `None` when missing or unreadable.
    fn sound(&mut self, path: &str) -> Option<Decoded> {
        if let Some(s) = self.sounds.get(path) {
            return s.clone();
        }
        let s = self
            .files
            .read(path)
            .and_then(|d| atlantis_codecs::decode_apc(&d).ok())
            .map(|a| (a.sample_rate, a.channels, Arc::new(a.samples)));
        if s.is_none() {
            log::warn!("{path}: not found or not APC");
        }
        self.sounds.insert(path.to_string(), s.clone());
        s
    }

    /// A category's volume: `ftol(GlobalVolume · 0.01f · level)`
    /// (@ 0x00433bc8).
    fn level(&self, level: i32) -> i32 {
        let g = f64::from(self.startup.config.global_volume as f32) * f64::from(0.01f32);
        (g * f64::from(level)) as i32
    }

    fn play_music(&mut self, path: &str) {
        if let Some((rate, ch, pcm)) = self.sound(path) {
            let g = csm_gain(self.level(self.startup.config.music_volume));
            self.audio.play(CHANNEL_MUSIC, rate, ch, pcm, true, [g, g]);
        }
    }

    /// `AtlBruitages_InitWarp` (@ 0x00401f30): the listener at the view's
    /// `Warp` position of the place's `Coord` file (else 0), the active
    /// sounds forgotten (not stopped), then each of the view's `bruitage`
    /// records' ambient object added by name, its files loaded.
    fn place_sounds_enter(&mut self, view: u32) {
        self.listener = self.coord.warps.get(&view).copied().unwrap_or([0.0; 3]);
        self.place_sounds.clear();
        let Some(v) = self.place.wam.view(view) else {
            return;
        };
        let ids: Vec<u32> = v.sounds.iter().map(|b| b.ambient_id).collect();
        let all = sources(&self.place.wam);
        for id in ids {
            let Some(k) = self
                .place
                .wam
                .ambient_objects
                .iter()
                .position(|o| o.id == id)
            else {
                continue;
            };
            let name = self.place.wam.ambient_objects[k].name.clone();
            if self.place_sounds.contains_key(&name) {
                continue;
            }
            let Some(src) = all.iter().find(|s| s.id == id).cloned() else {
                continue;
            };
            for f in &src.files {
                self.sound(&format!("sfx/{f}"));
            }
            self.place_sounds.insert(name, src);
        }
    }

    /// The place sounds of a warp frame (@ 0x004024b0): each active source
    /// whose sound is silent starts its first file again (one `rand()`
    /// drawn and dropped); the gains follow the listener. The original
    /// hands the positions to DirectSound3D (minimum distance 100,
    /// rolloff 1, the listener facing the camera's −Z, heights dropped);
    /// here the gain is 100 / distance beyond 100 and the side's share of
    /// the direction in the listener's frame pans it, DirectSound3D's own
    /// panning law being Windows'.
    fn place_sounds_frame(&mut self) {
        let (yaw, pitch) = self.camera.angles();
        let (ps, pc) = (f64::from(pitch).sin(), f64::from(pitch).cos());
        let (ys, yc) = (f64::from(yaw).sin(), f64::from(yaw).cos());
        let front = [-pc * yc, ps, -pc * ys];
        let top = [ps * yc, pc, ps * ys];
        // Left-handed: right = top × front.
        let right = [
            top[1] * front[2] - top[2] * front[1],
            top[2] * front[0] - top[0] * front[2],
            top[0] * front[1] - top[1] * front[0],
        ];
        let base = csm_gain(self.level(self.startup.config.sfx_volume));
        let l = [
            f64::from(self.listener[0]),
            0.0,
            f64::from(self.listener[1]),
        ];
        let entries: Vec<Source> = self.place_sounds.values().cloned().collect();
        for (k, src) in entries.iter().enumerate() {
            let ch = CHANNEL_PLACE + k;
            if ch >= CHANNELS {
                break;
            }
            let d = [
                f64::from(src.pos[0]) - l[0],
                0.0,
                f64::from(src.pos[1]) - l[2],
            ];
            let dist = (d[0] * d[0] + d[2] * d[2]).sqrt();
            let att = if dist > 100.0 { 100.0 / dist } else { 1.0 };
            let pan = if dist > 0.0 {
                (d[0] * right[0] + d[2] * right[2]) / dist
            } else {
                0.0
            };
            let g = base * att as f32;
            let gains = [
                g * (1.0 - pan).min(1.0) as f32,
                g * (1.0 + pan).min(1.0) as f32,
            ];
            if !self.audio.is_playing(ch) {
                self.rand.rand();
                if let Some(first) = src.files.first() {
                    if let Some((rate, chans, pcm)) = self.sound(&format!("sfx/{first}")) {
                        self.audio.play(ch, rate, chans, pcm, false, gains);
                    }
                }
            } else {
                self.audio.set_gain(ch, gains);
            }
        }
    }

    /// The places' hooks of a view entry (@ 0x004c2010, after the
    /// animations and the sounds): `(LIMITEANGLEWARPY=70)` at every entry,
    /// then per place and view the puzzles' set-ups (the Hoggar's views 1
    /// to 3: the stars @ 0x004d3960; the funeral chamber's 1 to 4
    /// @ 0x004c4de0; ...), not ported yet.
    fn place_hooks(&mut self, script: &mut Script) {
        script.run_command(self, b"(LIMITEANGLEWARPY=70)");
        let view = self.place.view.unwrap_or(0);
        let hooked = match self.place.name.to_ascii_lowercase().as_str() {
            "hoggar.wam" => (1..=3).contains(&view),
            "cfuner.wam" => (1..=4).contains(&view),
            "paleo.wam" | "ldmorts.wam" | "mmarchd.wam" | "jardin.wam" | "tourmagi.wam"
            | "pmagi.wam" | "interdim.wam" => true,
            _ => false,
        };
        if hooked {
            self.not_ported("the places' puzzle hooks at view entry");
        }
    }

    fn not_ported(&mut self, what: &str) {
        if !self.not_ported.iter().any(|w| w == what) {
            log::warn!("not ported yet: {what}");
            self.not_ported.push(what.to_string());
        }
    }

    /// Enters a view directly at (yaw, pitch) radians (tests and the
    /// developers' example): as a move's arrival, without its script.
    pub fn enter_at(&mut self, view: u32, yaw: f32, pitch: f32) {
        self.mode = Mode::Warp;
        self.moving = Moving::default();
        self.enter_view(view, yaw, pitch);
    }

    /// Loads a place as `CHANGEWAM` does, without its script commands
    /// (tests).
    pub fn load_place(&mut self, wam: &str) -> bool {
        Host::load_wam(self, wam)
    }

    /// Runs a script command as `RunOneCommand` does (`(KEY=value)`).
    pub fn run_command(&mut self, command: &str) {
        self.with_script(|s, h| s.run_command(h, command.as_bytes()));
    }

    /// Draws the warp (the place animations, then the cube) with a given
    /// camera matrix under the control word `fpu`, the projection being
    /// the camera's (tests: the goldens give the original's matrices).
    #[doc(hidden)]
    pub fn draw_with_matrix(&mut self, matrix: &[f32; 9], fpu: Fpu) -> &[u16] {
        self.renderer.fpu = fpu;
        let proj = self.camera.projection();
        for (w, f) in self.work.iter_mut().zip(&self.place.faces) {
            w.clone_from(f);
        }
        self.slots.draw(&mut self.work, self.ticks);
        self.screen.fill(0xf81f);
        self.renderer
            .render(&self.work, matrix, proj, &mut self.screen);
        &self.screen
    }

    /// The faces after the sprite pass of the last draw (tests).
    #[doc(hidden)]
    pub fn sprite_faces(&self) -> &[Vec<u16>; 6] {
        &self.work
    }

    /// The place animations of the view.
    pub fn slots(&self) -> &Slots {
        &self.slots
    }

    /// Seeds `rand` (the original seeds it with the clock).
    pub fn seed(&mut self, seed: u32) {
        self.rand = Rand::new(seed);
    }

    /// The x87 control word the game's float code runs under (the
    /// process's, 0x027f, until the first warp frame leaves 0x0e7f).
    pub fn set_fpu(&mut self, fpu: Fpu) {
        self.camera.fpu = fpu;
        self.renderer.fpu = fpu;
        if let Some(s) = self.script.as_mut() {
            s.set_fpu(fpu);
        }
    }

    /// The world (`MONDE`) whose folders the views' cubes come from.
    pub fn set_world(&mut self, world: i32) {
        if let Some(s) = self.script.as_mut() {
            s.a3.world = world;
        }
    }

    /// A click on the centre zone as the warp frame handles it (the zone
    /// click and the move's set-up), without drawing (tests): (target view,
    /// turning, move pending).
    #[doc(hidden)]
    pub fn click_centre_now(&mut self) -> (Option<u32>, bool, bool) {
        self.click();
        self.centre = self.pick_centre();
        self.zone_click();
        self.click = false;
        if let Some(s) = self.script.as_mut() {
            s.click = false;
        }
        (
            self.moving.next,
            self.moving.turn.is_some(),
            self.moving.pending,
        )
    }

    /// One 25 Hz tick of a move's turn (tests): whether it goes on.
    #[doc(hidden)]
    pub fn turn_tick(&mut self) -> bool {
        let (w, h) = (self.width as u32, self.height as u32);
        let Some(turn) = self.moving.turn.as_mut() else {
            return false;
        };
        self.camera.set_fov(warp_fov(self.camera.fpu), w, h);
        let going = turn.step(&mut self.camera, &mut self.zoom, (w, h)).going;
        if !going {
            self.moving.turn = None;
            self.moving.pending = self.moving.next.is_some();
        }
        going
    }

    /// The camera put back as the warp's initialisation leaves it (the
    /// emulation goldens restore it before each view entry), keeping the
    /// control word.
    #[doc(hidden)]
    pub fn reset_camera(&mut self) {
        let fpu = self.camera.fpu;
        self.camera = Camera::new(Fpu::Double, self.width as u32, self.height as u32);
        self.camera.fpu = fpu;
    }

    /// The move's turn, if one runs (tests).
    #[doc(hidden)]
    pub fn turn_state(&self) -> Option<&Turn> {
        self.moving.turn.as_ref()
    }

    /// The arrival angles of the move set up (radians).
    #[doc(hidden)]
    pub fn move_arrival(&self) -> (f32, f32) {
        self.moving.arrival
    }

    /// The zone of the current view under screen point (sx, sy).
    pub fn pick_at(&self, sx: f32, sy: f32) -> Option<usize> {
        let view = self.place.wam.view(self.place.view?)?;
        let d = self.camera.ray(sx, sy).map(f64::from);
        wam::pick(&view.zones, d)
    }

    /// The `defnames.con` names.
    pub fn names(&self) -> &DefNames {
        &self.names
    }
}

/// The six faces of a cube file (Hnm_Framer: `IX` frames in video mode;
/// frames → faces 1, 3, 0, 4, 5, 2 @ 0x00430745).
pub fn faces_from_hnm(hnm: &[u8]) -> Result<[Vec<u16>; 6], atlantis_codecs::DecodeError> {
    use atlantis_codecs::{DecodeError, HnmVideo};
    let mut video = HnmVideo::new(hnm)?;
    if video.width() != 512 || video.height() != 512 {
        return Err(DecodeError::InvalidFileHeader);
    }
    let mut faces: [Vec<u16>; 6] = Default::default();
    for &face in &FRAME_TO_FACE {
        let frame = video.next_frame().ok_or(DecodeError::NoVideo)??;
        faces[face] = frame.to_vec();
    }
    Ok(faces)
}

/// A video frame in a black W x ftol(W · 0.75) surface at y = (that height
/// − h) / 2 (@ 0x004211aa..0x00421205), the surface stretched to the
/// screen (DirectDraw's `Blt`, the driver's scaler; here the nearest
/// pixel).
pub fn stretch_video(frame: &[u16], w: usize, h: usize, out: &mut [u16], sw: usize, sh: usize) {
    let surf_h = (w as f32 * 0.75) as usize;
    let top = (surf_h as i64 - h as i64) / 2;
    for y in 0..sh {
        let sy = (y * surf_h / sh) as i64 - top;
        let row = &mut out[y * sw..(y + 1) * sw];
        if sy < 0 || sy >= h as i64 {
            row.fill(0);
            continue;
        }
        let src = &frame[sy as usize * w..(sy as usize + 1) * w];
        for (x, px) in row.iter_mut().enumerate() {
            *px = src[x * w / sw];
        }
    }
}

impl Host for Game {
    fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>> {
        self.files.read(&format!("scenar/chapi{chapter:03}.con"))
    }

    fn load_wam(&mut self, name: &str) -> bool {
        let Some(data) = self.files.read(&format!("wam/{name}")) else {
            log::error!("Can't load WAM {name}");
            return false;
        };
        match Wam::parse(&data) {
            Ok(mut w) => {
                w.apply_defnames(&self.names);
                let coord_name = Coord::file_name(name);
                self.coord = self
                    .files
                    .read(&format!("scenar/{coord_name}"))
                    .and_then(|d| Coord::parse(&d).ok())
                    .unwrap_or_default();
                self.place = Place {
                    wam: w,
                    name: name.to_string(),
                    view: None,
                    faces: Default::default(),
                    cube: String::new(),
                };
                true
            }
            Err(e) => {
                log::error!("{name}: {e:?}");
                false
            }
        }
    }

    fn has_view(&mut self, view: i32) -> bool {
        u32::try_from(view)
            .ok()
            .and_then(|v| self.place.wam.view(v))
            .is_some()
    }

    fn enter_view(&mut self, view: i32, yaw: f32, pitch: f32) {
        Game::enter_view(self, view as u32, yaw, pitch);
    }

    fn enter_view_with_script(&mut self, view: i32, yaw: f32, pitch: f32, script: &mut Script) {
        Game::enter_view(self, view as u32, yaw, pitch);
        self.place_hooks(script);
    }

    fn turn_now(&mut self, yaw: bool, angle: f32) {
        let _ = (yaw, angle);
        self.not_ported("AngleWarpXNow / AngleWarpYNow");
    }

    fn anim(&mut self, op: AnimOp) {
        match op {
            AnimOp::Show { kind, id, number } => self.slots.show(id, kind, number, self.now),
            AnimOp::Hide { kind, id } => self.slots.hide(id, kind, self.now),
            _ => self.not_ported("the animation commands (PlayAnim...)"),
        }
    }

    fn wam_commands(&mut self, wam: &str, flags: &[i32]) -> Vec<String> {
        crate::wam_setup::commands(wam, flags)
    }

    fn play_cinema(&mut self, n: i32, fps: i32) -> bool {
        let world = self.script.as_ref().map_or(1, |s| s.a3.world);
        let folder = WORLDS.get(world as usize).copied().unwrap_or("");
        let (video, subtitles) = atlantis3_data::cine_txt::cinema_names(n as u32);
        // The cue file decides the folder (@ 0x00420f3c..0x00421089).
        let localised = self.files.size(&format!("hnm/{subtitles}")).is_some();
        let lang = self.startup.voice_language();
        let path = if localised {
            format!("loc/hnm/HNM640/{lang}/{folder}/{video}")
        } else {
            format!("hnm/HNM640/{folder}/{video}")
        };
        let back = self.mode;
        self.start_video(&path, fps.max(0) as u32, false, back)
    }

    /// `STARTMUSIK=n` (@ 0x0044c3b0): `music\<world>\Muzik0<n>.apc`,
    /// looped, unless it plays already. The original cross-fades two
    /// slots by ±5 a 15 Hz tick; here the new track starts at once.
    fn start_music(&mut self, list: &str) -> bool {
        let n = list
            .split([' ', '\t', ',', ';', '(', ')'])
            .find(|w| !w.is_empty())
            .unwrap_or("");
        let world = self.script.as_ref().map_or(1, |s| s.a3.world);
        let folder = WORLDS.get(world as usize).copied().unwrap_or("");
        let path = format!("music/{folder}/Muzik0{n}.apc");
        if self.music.as_deref() == Some(path.as_str()) && self.audio.is_playing(CHANNEL_MUSIC) {
            return false;
        }
        self.music = Some(path.clone());
        self.play_music(&path);
        false
    }

    /// `STOPMUSIK` (@ 0x004491eb): the music faded out (here stopped).
    fn stop_music(&mut self) {
        self.music = None;
        self.audio.stop(CHANNEL_MUSIC);
    }

    /// `PLAYSOUND=n` (@ 0x00448eb9): `sfx\soundNNN.apc` once.
    fn new_sound(&mut self, apc: &str) {
        let path = format!("sfx/{apc}");
        if let Some((rate, ch, pcm)) = self.sound(&path) {
            let g = csm_gain(self.level(self.startup.config.sfx_volume));
            self.audio.play(CHANNEL_SOUND, rate, ch, pcm, false, [g, g]);
        }
    }

    fn dial_start_a3(&mut self, n: i32, voixoff: bool) -> DialStart {
        let _ = (n, voixoff);
        self.not_ported("the dialogs (DIAL)");
        self.resume_next = true;
        DialStart::Started
    }

    fn set_monde(&mut self, monde: i32) -> bool {
        let _ = monde;
        true
    }
}
