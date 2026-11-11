//! The engine side of the scripts' bindings for the warp view
//! (`atlantis4_script::Engine`): the node change builds the node, and the
//! view bindings drive the warp (atlantis4-gog-en-2363910).
//!
//! The node change itself (`ChangeWarp`'s worker @ 0x00526c70) runs in
//! `atlantis4_script` in the original's order: `OnWarpLeave`, the
//! autosave, the new scene ([`Engine::enter_scene`]: here the node's
//! background and XML, the limits reset, the camera's FOV), the eight
//! callbacks set to `nil`, `warpName`, the node's `.lua`, `OnWarpEnter`.
//!
//! Done here: `PutObject`, `RemoveObject`, `StartAnimation`,
//! `StartAnimationPart`, `SetMarker`, `LookAt`, `SetViewAngleXLimits`,
//! `SetViewAngleYLimits`, `SetFOV`, `ChangeWarp`, `PlayMovie` (picture
//! and sound: its sound file plays on the `video` channel from the
//! movie's start to its end or skip, the music cut meanwhile), and the
//! sounds ([`crate::sound`]): `PlayMusic`, `PlaySound`, `StopSound` with
//! `OnSoundStopped`, the node change's stop of the `PlaySound` sounds, and
//! `AddToBag`'s notification sound; the inventory ([`crate::inventory`]:
//! `AddToBag`, `AddToBagWithoutNotification`, both `RemoveFromBag`,
//! `Selected`, `ObjectCount`, `TestObjectLimit`, the notification), the
//! documents ([`crate::documents`]: `AddDocument`, `ShowDocument`,
//! `HideDocument` with `OnDocumentClosed`), `LoadSprite` /
//! `UnloadSprite`, and the screens that show them ([`crate::screens`]:
//! the HUD's inventory button, the inventory menu, the shown document).
//! The save's text is built ([`crate::savexml`]) but not written (M4).
//! The dialogs ([`crate::dialog`]): `DrawText`, `InitDialog`,
//! `HideDialog`, `PushAnswer`, `HideAnswers`. Every other binding is
//! logged and does nothing.

use std::collections::HashMap;

use atlantis4_data::image::decode_png;
use atlantis4_data::video::RgbTable;
use atlantis4_data::{A4Files, WarpPath};
use atlantis4_script::tolua::Binding;
use atlantis4_script::{normalise_path, Call, Engine, ErrorSite, Event, Reply, SavedGlobal, Value};

use crate::camera::fov_for_height;
use crate::dialog::{Action, Ctx, Dialogs};
use crate::documents::{Diary, Documents, PuzzleTimer};
use crate::inventory::Inventory;
use crate::markers::sprite_path;
use crate::movie::{self, Click, Movie, MovieCue};
use crate::package::Picture;
use crate::render::{render, Coverage, Weights};
use crate::screens::{NoText, Screens, TextDraw};
use crate::sound::{SoundEvent, Sounds};
use crate::sprite::{self, cursor_corner, CURSOR_PATH};
use crate::warp::{Warp, WarpNote};
use crate::View;

/// The background of the last frame, kept while the view does not move.
struct BackgroundCache {
    key: (u64, u32, u32, u32, u32, u32),
    pixels: Vec<u32>,
}

/// The game as the scripts see it.
pub struct Game {
    pub files: A4Files,
    /// The original's video colour table (from its executable).
    pub table: Option<RgbTable>,
    pub warp: Option<Warp>,
    pub view: View,
    /// The camera's field of view (`camera+0xb8`), radians.
    pub fov: f32,
    /// The window's height (the `HD` definition, hence the FOV).
    pub height: u32,
    /// The engine clock, ms.
    pub now_ms: f64,
    /// The global marker flag (`App+0x86b0`).
    pub markers_shown: bool,
    /// The bilinear weights' precision of the drawing.
    pub weights: Weights,
    /// When set, the files run, callbacks entered and bindings called, as
    /// `FILE path`, `ENTER name`, `CALL name` (the live trace's lines
    /// without their arguments).
    pub trace: Option<Vec<String>>,
    /// Counts the node changes (the background cache's key).
    serial: u64,
    background: Option<BackgroundCache>,
    cover: Coverage,
    /// Decoded 2-D pictures by path (`None`: missing or broken).
    sprites: HashMap<String, Option<Picture>>,
    /// The movie's fixed pictures as [`movie::resolve`] found them.
    resolved: HashMap<&'static str, String>,
    /// The `PlayMovie` in progress (`game+0x4038` clear while it is).
    pub movie: Option<Movie>,
    /// The movies' sound events, for the sound side to take.
    pub movie_cues: Vec<MovieCue>,
    /// The engine clock read at a binding call, in µs (`play()` reads the
    /// clock itself, @ 0x004f592b); `None`: the frame's time, `now_ms`.
    pub clock_us: Option<Box<dyn Fn() -> u64>>,
    /// The sound manager and the game's sound objects.
    pub sounds: Sounds,
    /// The dialog bar, the conversation view and the answers.
    pub dialogs: Dialogs,
    /// `OnSoundStopped` callbacks raised and not yet run.
    sound_pending: Vec<(String, Vec<Value>)>,
    /// The install's language (`lang.ini`).
    pub lang: String,
    pub inventory: Inventory,
    pub documents: Documents,
    pub diary: Diary,
    pub puzzle_timer: PuzzleTimer,
    /// The screens' layouts, read at their first use.
    pub(crate) screens: Option<Screens>,
    /// The text of the GUI (the dialog work's glyphs; nothing until then).
    pub text: Box<dyn TextDraw>,
    /// Callbacks the GUI raised (`OnDocumentClosed`, …), to run in order.
    pub(crate) callbacks: Vec<(String, Vec<Value>)>,
    /// The current node's path as `ChangeWarp` gave it (`WarpName`).
    pub(crate) warp_name: Vec<u8>,
    /// `SetInventoryAvatar`'s picture name (the avatar is a 1 × 1 button
    /// without picture in this game's `Inventory.lua`).
    pub avatar: Option<String>,
    /// The last save's file name and text (not written to disk yet).
    pub last_save: Option<(String, Vec<u8>)>,
    /// The GUI layers are drawn (the HUD, the inventory menu, the shown
    /// document, the notification); off, a frame is the warp, the
    /// markers, a movie and the cursor only.
    pub draw_gui: bool,
}

/// A PNG as RGBA.
fn png_rgba(data: &[u8]) -> Option<Picture> {
    let img = decode_png(data).ok()?;
    let rgba = match img.channels {
        4 => img.pixels,
        3 => img
            .pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        2 => img
            .pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        _ => img.pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    };
    Some(Picture {
        width: img.width,
        height: img.height,
        rgba,
    })
}

fn text(b: Option<&[u8]>) -> String {
    b.map(|b| b.iter().map(|&c| char::from(c)).collect())
        .unwrap_or_default()
}

/// `_ftol`: towards zero.
fn int(x: f64) -> i32 {
    x as i32
}

impl Game {
    pub fn new(files: A4Files, height: u32) -> Game {
        let table = files.read(atlantis4_data::exe::EXE_NAME).and_then(|exe| {
            match RgbTable::from_exe(&exe) {
                Ok(t) => Some(t),
                Err(e) => {
                    log::warn!("no video colour table: {e}");
                    None
                }
            }
        });
        // The language of the voices and of the localised sounds.
        let lang = files.lang_ini().unwrap_or_else(|| "en".to_string());
        Game {
            sounds: Sounds::new(&lang),
            lang: lang.clone(),
            inventory: Inventory::default(),
            documents: Documents::default(),
            diary: Diary::default(),
            puzzle_timer: PuzzleTimer::default(),
            screens: None,
            text: Box::new(NoText),
            callbacks: Vec::new(),
            warp_name: Vec::new(),
            avatar: None,
            last_save: None,
            draw_gui: true,
            files,
            table,
            warp: None,
            view: View::default(),
            fov: fov_for_height(height),
            height,
            now_ms: 0.0,
            markers_shown: true,
            weights: Weights::Exact,
            trace: None,
            serial: 0,
            background: None,
            cover: Coverage::new(),
            sprites: HashMap::new(),
            resolved: HashMap::new(),
            movie: None,
            movie_cues: Vec::new(),
            clock_us: None,
            dialogs: Dialogs::default(),
            sound_pending: Vec::new(),
        }
    }

    /// The clock now, in ms (µs / 1000.0 as the original divides it).
    pub fn clock_ms(&self) -> f64 {
        match &self.clock_us {
            Some(c) => c() as f64 / 1000.0,
            None => self.now_ms,
        }
    }

    /// `PlayMovie(video, sound)` (worker @ 0x005232b0): the background
    /// shown, the skip button hidden, the music stopped and `sound`
    /// started on the `video` channel, the `video` sprite loaded with
    /// `video` and played from the clock's time.
    pub fn play_movie(&mut self, video: &str, sound: &str) {
        log::info!("PlayMovie({video}, {sound})");
        self.movie_cues.push(MovieCue::Start {
            sound: sound.to_string(),
        });
        // The music stopped, then the sound on `video` (@ 0x00523435,
        // 0x0052344e..0x00523573), before the sprite's load.
        self.movie_started(Some(sound));
        let now = self.clock_ms();
        self.movie = Some(Movie::start(&self.files, video, now, self.table.as_ref()));
    }

    /// The movie's frame at `now_ms`: the name for `OnMovieFinished` when
    /// it ended there.
    pub fn update_movie(&mut self, now_ms: f64) -> Option<String> {
        self.now_ms = now_ms;
        let ended = self.movie.as_mut()?.update(now_ms, self.table.as_ref());
        ended.then(|| self.end_movie())
    }

    /// The end (@ 0x00524740): the movie's layouts hidden, the `video`
    /// channel stopped and the music played again; the name for
    /// `OnMovieFinished`.
    fn end_movie(&mut self) -> String {
        let m = self.movie.take().expect("a movie");
        self.movie_cues.push(MovieCue::End);
        // `video` stopped, the music played again, then `OnMovieFinished`.
        self.movie_finished();
        let name = m.finished_name();
        log::info!("movie {name} finished");
        name
    }

    /// The skip button's picture size (75 × 75 in the data).
    fn skip_size(&mut self) -> (u32, u32) {
        let p = self.resolved(movie::SKIP_PICTURE);
        self.load_sprite(&p);
        match self.sprites.get(&p) {
            Some(Some(pic)) => (pic.width, pic.height),
            _ => (75, 75),
        }
    }

    /// A press at window pixel (`x`, `y`) of a `width × height` view
    /// while a movie plays (the movie's buttons take it).
    pub fn movie_press(&mut self, width: u32, height: u32, x: f64, y: f64) {
        let skip = self.skip_size();
        if let Some(m) = self.movie.as_mut() {
            let l = m.layout(width, height, skip);
            m.press(&l, x, y);
        }
    }

    /// The pointer moved to (`x`, `y`) during a movie.
    pub fn movie_drag(&mut self, width: u32, height: u32, x: f64, y: f64) {
        let skip = self.skip_size();
        if let Some(m) = self.movie.as_mut() {
            let l = m.layout(width, height, skip);
            m.drag(&l, x, y);
        }
    }

    /// A release at (`x`, `y`) during a movie: the name for
    /// `OnMovieFinished` when it skipped the movie.
    pub fn movie_release(&mut self, width: u32, height: u32, x: f64, y: f64) -> Option<String> {
        let skip = self.skip_size();
        let m = self.movie.as_mut()?;
        let l = m.layout(width, height, skip);
        match m.release(&l, x, y) {
            Click::Skipped => Some(self.end_movie()),
            Click::Toggled => {
                log::info!(
                    "skip button {}",
                    if m.skip_shown { "shown" } else { "hidden" }
                );
                None
            }
            Click::None => None,
        }
    }

    /// The movie's pictures loaded; their resolved paths.
    fn movie_pictures(&mut self) -> [String; 3] {
        [
            movie::BACKGROUND_PICTURE,
            movie::SKIP_PICTURE,
            movie::SKIP_DOWN_PICTURE,
        ]
        .map(|p| {
            let r = self.resolved(p);
            self.load_sprite(&r);
            r
        })
    }

    /// [`movie::resolve`] of one of the movie's fixed pictures, looked up
    /// once (the path itself when no variant exists).
    fn resolved(&mut self, path: &'static str) -> String {
        if let Some(r) = self.resolved.get(path) {
            return r.clone();
        }
        let r = movie::resolve(&self.files, path).unwrap_or_else(|| path.to_string());
        self.resolved.insert(path, r.clone());
        r
    }

    fn load_sprite(&mut self, path: &str) {
        if !self.sprites.contains_key(path) {
            let pic = self.files.read(path).and_then(|d| png_rgba(&d));
            if pic.is_none() {
                log::warn!("no picture {path}");
            }
            self.sprites.insert(path.to_string(), pic);
        }
    }

    /// A digest of everything [`Game::draw`] reads for a `width × height`
    /// frame with the cursor at `pointer`: two equal keys give the same
    /// picture, so a frame whose key did not change need not be drawn
    /// again (the view, the node, each animation's frame, each marker's
    /// band, the movie's frame and buttons, the cursor).
    pub fn frame_key(&self, width: u32, height: u32, pointer: Option<(f64, f64)>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (width, height, self.serial, self.weights == Weights::Exact).hash(&mut h);
        (
            self.fov.to_bits(),
            self.view.yaw.to_bits(),
            self.view.pitch.to_bits(),
        )
            .hash(&mut h);
        pointer
            .map(|(x, y)| (x.to_bits(), y.to_bits()))
            .hash(&mut h);
        self.sprites.len().hash(&mut h);
        if let Some(w) = &self.warp {
            w.markers_shown.hash(&mut h);
            for a in &w.anims {
                (&a.name, a.package.is_some(), a.anim.frame).hash(&mut h);
            }
            for m in &w.markers {
                m.point.map(f32::to_bits).hash(&mut h);
                m.sprite
                    .map(|s| (s.image, s.frame(self.now_ms)))
                    .hash(&mut h);
            }
        }
        self.dialogs.hash_into(&mut h);
        self.draw_gui.hash(&mut h);
        if self.draw_gui {
            self.gui_key(&mut h);
        }
        if let Some(m) = &self.movie {
            (&m.path, m.skip_shown, m.skip_pressed).hash(&mut h);
            m.sprite
                .as_ref()
                .map(|s| (s.anim.frame, s.picture().is_some()))
                .hash(&mut h);
        }
        h.finish()
    }

    /// The frame of a `width × height` window into `out` (XRGB, rows
    /// top-down), in the original's order: the warp (background, then the
    /// draw list), the markers, a movie's background, video and skip
    /// button, the cursor at `pointer` (window pixels) when given.
    pub fn draw(&mut self, out: &mut [u32], width: u32, height: u32, pointer: Option<(f64, f64)>) {
        let n = width as usize * height as usize;
        let out = &mut out[..n];
        let cam = self.camera(width, height);
        // Under a movie's background nothing of the warp shows: it is
        // not drawn.
        let covered = self.movie.as_ref().is_some_and(|m| m.covers(width, height));
        if covered {
            out.fill(0);
        } else if let Some(w) = self.warp.as_mut() {
            let key = (
                self.serial,
                cam.fov.to_bits(),
                self.view.yaw.to_bits(),
                self.view.pitch.to_bits(),
                width,
                height,
            );
            let stale = self.background.as_ref().is_none_or(|b| b.key != key);
            if stale {
                let mut pixels = vec![0u32; n];
                render(&cam, &w.node.faces, &mut pixels, self.weights);
                self.background = Some(BackgroundCache { key, pixels });
            }
            if let Some(b) = &self.background {
                out.copy_from_slice(&b.pixels);
            }
            w.draw_layers(
                &cam,
                self.table.as_ref(),
                out,
                &mut self.cover,
                self.weights,
            );
        } else {
            out.fill(0);
        }
        let images: Vec<u32> = self
            .warp
            .iter()
            .flat_map(|w| w.markers.iter().filter_map(|m| m.sprite.map(|s| s.image)))
            .collect();
        for i in images {
            self.load_sprite(&sprite_path(i, false));
        }
        if let Some(w) = self.warp.as_ref().filter(|_| !covered) {
            let sprites = &self.sprites;
            w.draw_markers(&cam, self.now_ms, out, |i| {
                sprites.get(&sprite_path(i, false)).and_then(Option::as_ref)
            });
        }
        // The HUD (`inGame`: the sprites, the inventory button) under the
        // movie's layouts, which `InGame.lua` declares after it.
        if self.draw_gui {
            self.draw_hud(out, width, height);
        }
        if self.movie.is_some() {
            let [bg, up, down] = self.movie_pictures();
            let get = |p: &str| self.sprites.get(p).and_then(Option::as_ref);
            if let Some(m) = &self.movie {
                m.draw(out, width, height, get(&bg), get(&up), get(&down));
            }
        }
        // The inventory menu (and the shown document), the notification.
        if self.draw_gui {
            self.draw_menu(out, width, height, pointer);
        }
        // Above the HUD (and the inventory menu): the dialog, then the
        // answers (`Game::load` @ 0x005294bf..0x00529712).
        self.dialogs.draw(&self.files, out, width, height);
        if let Some((x, y)) = pointer {
            self.load_sprite(CURSOR_PATH);
            if let Some(Some(pic)) = self.sprites.get(CURSOR_PATH) {
                let (l, t) = cursor_corner(pic, x, y);
                sprite::draw(out, width, height, pic, (0, 0, pic.width, pic.height), l, t);
            }
        }
    }

    fn note(r: Result<(), WarpNote>) {
        if let Err(n) = r {
            match n {
                WarpNote::NoPackage(name, e) => log::warn!("no package {name}: {e}"),
                WarpNote::NoMarkerTarget(name) => {
                    log::warn!("SetMarker: no exit or animation {name}")
                }
                WarpNote::UnknownMarker(n, name) => {
                    log::warn!("[TeWarp3::configMarker: Unknown marker {n} on animation : {name}")
                }
            }
        }
    }

    /// The frame animations at `now_ms`; the names whose animation
    /// finished (for `OnAnimationFinished(name)`).
    pub fn update(&mut self, now_ms: f64) -> Vec<String> {
        self.now_ms = now_ms;
        // `TeSoundManager::update` runs first in the frame (@ 0x0057c160).
        self.sounds.update(&self.files, now_ms);
        // Its signals: the voice's end reaches the dialog bar at once.
        self.route_sound_events();
        let finished = self
            .warp
            .as_mut()
            .map(|w| w.update(now_ms))
            .unwrap_or_default();
        let mut c = Ctx {
            files: &self.files,
            sounds: &mut self.sounds,
            table: self.table.as_ref(),
            now: now_ms,
        };
        self.dialogs.update(&mut c);
        finished
    }

    /// The Lua callbacks the dialogs raised in the last update
    /// (`OnDialogClosed`, `OnDialogFinished`), to run in this order after
    /// the animations'.
    pub fn dialog_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        self.dialogs.take_callbacks()
    }

    /// The sounds' events: `VoiceStopped` to the dialog bar
    /// (`Dialog2::onVoiceStopped` @ 0x0052c220), `SoundStopped` kept for
    /// `OnSoundStopped`.
    fn route_sound_events(&mut self) {
        for e in self.sounds.take_events() {
            match e {
                SoundEvent::SoundStopped(name) => self.sound_pending.push((
                    "OnSoundStopped".to_string(),
                    vec![Value::Str(name.into_bytes())],
                )),
                SoundEvent::VoiceStopped => {
                    let now = self.clock_ms();
                    let mut c = Ctx {
                        files: &self.files,
                        sounds: &mut self.sounds,
                        table: self.table.as_ref(),
                        now,
                    };
                    self.dialogs.voice_stopped(&mut c);
                }
            }
        }
    }

    /// Whether a GUI layer above the view takes the pointer (the dialog's
    /// or the answers' lock button is shown).
    pub fn gui_shown(&self) -> bool {
        self.dialogs.shown() || self.dialogs.answers_shown()
    }

    /// A press at window pixel (`x`, `y`): `true` when a GUI layer took it
    /// (the view then neither turns nor clicks): the dialogs above the
    /// screens (the HUD, the inventory menu, a document).
    pub fn gui_press(&mut self, width: u32, height: u32, x: f64, y: f64) -> bool {
        if self.gui_shown() {
            self.dialogs.layout(&self.files, width, height);
            if self.dialogs.press(x, y, width, height) {
                return true;
            }
        }
        self.screens_press(width, height, x, y)
    }

    /// The pointer moved to (`x`, `y`): the buttons' pressed and roll-over
    /// states.
    pub fn gui_hover(&mut self, width: u32, height: u32, x: f64, y: f64) {
        if self.gui_shown() {
            self.dialogs.layout(&self.files, width, height);
            self.dialogs.hover(x, y, width, height);
        }
    }

    /// A release at (`x`, `y`): `None` when no GUI layer took it, else the
    /// validated click's consequences to run in order (Lua callbacks, the
    /// answers removed after `OnAnswered`).
    pub fn gui_release(&mut self, width: u32, height: u32, x: f64, y: f64) -> Option<Vec<Action>> {
        if self.gui_shown() {
            self.dialogs.layout(&self.files, width, height);
            let now = self.clock_ms();
            let mut c = Ctx {
                files: &self.files,
                sounds: &mut self.sounds,
                table: self.table.as_ref(),
                now,
            };
            if let Some(mut out) = self.dialogs.release(&mut c, x, y, width, height) {
                // A click on the bar stopped the voice: its signal at once.
                self.route_sound_events();
                out.extend(
                    self.dialogs
                        .take_callbacks()
                        .into_iter()
                        .map(|(n, a)| Action::Lua(n, a)),
                );
                return Some(out);
            }
        }
        self.screens_release(width, height, x, y).then(|| {
            self.gui_callbacks()
                .into_iter()
                .map(|(n, a)| Action::Lua(n, a))
                .collect()
        })
    }

    /// The camera of a `width × height` view.
    pub fn camera(&self, width: u32, height: u32) -> crate::Camera {
        crate::Camera::new(self.fov, width, height, self.view.yaw, self.view.pitch)
    }

    /// What a click at window pixel (`x`, `y`) of a `width × height` view
    /// hits: the name `OnWarpObjectHit` gets (an animation's, or `3D\` +
    /// the exit's `linkedWarp`).
    pub fn click(&self, width: u32, height: u32, x: i32, y: i32) -> Option<String> {
        let w = self.warp.as_ref()?;
        let cam = self.camera(width, height);
        match crate::pick::click(&cam, x, y, &w.picks(), &w.node.exits)? {
            crate::pick::Target::Anim(a) => Some(a),
            crate::pick::Target::Exit(e) => Some(crate::warp::exit_name(&e)),
        }
    }

    /// The Lua callbacks the sounds' signals raised (`OnSoundStopped`),
    /// to run in this order; the viewer calls it after [`Game::update`].
    pub fn sound_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        self.route_sound_events();
        std::mem::take(&mut self.sound_pending)
    }

    /// A cut-scene starts (`PlayMovie`'s worker @ 0x005232b0): the music
    /// stops and the movie's sound file, if any, plays on the `video`
    /// channel. For the movie player.
    pub fn movie_started(&mut self, sound: Option<&str>) {
        self.sounds.start_movie(&self.files, sound);
    }

    /// The cut-scene ends or is skipped (@ 0x00524740): its sound stops and
    /// the music restarts from its beginning; `OnMovieFinished` follows
    /// (the movie player's).
    pub fn movie_finished(&mut self) {
        self.sounds.end_movie(&self.files);
    }

    fn with_warp(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut Warp, &A4Files, f64) -> Result<(), WarpNote>,
    ) {
        let now = self.now_ms;
        match self.warp.as_mut() {
            Some(w) => Self::note(f(w, &self.files, now)),
            None => log::warn!("{what}: no node"),
        }
    }
}

impl Engine for Game {
    fn call(&mut self, call: &Call) -> Reply {
        let name = text(call.str(0));
        if let Some(t) = &mut self.trace {
            t.push(format!("CALL {}", call.binding.name()));
        }
        match call.binding {
            Binding::ChangeWarp => return Reply::Warp,
            Binding::PutObject => {
                let clickable = call.flag(1);
                self.with_warp("PutObject", |w, f, now| {
                    w.put_object(f, &name, clickable, now)
                });
            }
            Binding::RemoveObject => {
                if let Some(w) = self.warp.as_mut() {
                    w.remove_object(&name);
                }
            }
            Binding::StartAnimation => {
                let (loops, clickable) = (int(call.num(1)), call.flag(2));
                self.with_warp("StartAnimation", |w, f, now| {
                    w.start_animation(f, &name, loops, clickable, now)
                });
            }
            Binding::StartAnimationPart => {
                let (first, last) = (int(call.num(1)), int(call.num(2)));
                let (loops, clickable) = (int(call.num(3)), call.flag(4));
                self.with_warp("StartAnimationPart", |w, f, now| {
                    w.start_animation_part(f, &name, first, last, loops, clickable, now)
                });
            }
            Binding::SetMarker => {
                let (image, n) = (int(call.num(1)), int(call.num(2)));
                self.with_warp("SetMarker", |w, _, now| {
                    w.set_marker(&name, image, n as u32, now)
                });
            }
            Binding::LookAt => {
                // `0x0052b0d0`: both truncated to int first.
                let (x, y) = (int(call.num(0)), int(call.num(1)));
                self.view.look_at(x as f32, y as f32);
            }
            Binding::SetViewAngleXLimits => {
                self.view
                    .set_yaw_limits(call.num(0) as f32, call.num(1) as f32);
            }
            Binding::SetViewAngleYLimits => {
                self.view
                    .set_pitch_limits(call.num(0) as f32, call.num(1) as f32);
            }
            Binding::SetFov => {
                self.fov = (call.num(0) * f64::from(std::f32::consts::PI) / 180.0) as f32;
            }
            Binding::Selected => {
                self.gui();
                return Reply::Bool(self.inventory.is_selected(&name));
            }
            Binding::PlayMovie => {
                let sound = text(call.str(1));
                self.play_movie(&name, &sound);
            }
            Binding::TestObjectLimit => {
                self.gui();
                // `count(name) >= n`, unsigned (@ 0x0057fa50).
                let n = call.num(1) as u32;
                return Reply::Bool(self.inventory.count(&name) as u32 >= n);
            }
            Binding::ObjectCount => {
                self.gui();
                return Reply::Num(self.inventory.count(&name) as f64);
            }
            Binding::PrintDebugMessage => log::info!("PrintDebugMessage: {name}"),
            Binding::PlayMusic => {
                let path = text(Some(&normalise_path(call.str(0).unwrap_or_default())));
                self.sounds.play_music(&self.files, &path);
            }
            Binding::PlaySound => {
                let path = text(Some(&normalise_path(call.str(0).unwrap_or_default())));
                self.sounds
                    .play_sound(&self.files, &name, &path, int(call.num(1)));
            }
            Binding::StopSound => self.sounds.stop_sound(&name),
            Binding::DrawText => {
                let voice = text(call.str(1));
                let n = int(call.num(2));
                let (video, bg) = (text(call.str(3)), text(call.str(4)));
                let now = self.clock_ms();
                let mut c = Ctx {
                    files: &self.files,
                    sounds: &mut self.sounds,
                    table: self.table.as_ref(),
                    now,
                };
                self.dialogs
                    .draw_text(&mut c, &name, &voice, n, &video, &bg);
            }
            Binding::InitDialog => {
                let video = text(call.str(1));
                let now = self.clock_ms();
                let mut c = Ctx {
                    files: &self.files,
                    sounds: &mut self.sounds,
                    table: self.table.as_ref(),
                    now,
                };
                self.dialogs.init_dialog(&mut c, &name, &video);
            }
            Binding::HideDialog => {
                let now = self.clock_ms();
                let mut c = Ctx {
                    files: &self.files,
                    sounds: &mut self.sounds,
                    table: self.table.as_ref(),
                    now,
                };
                self.dialogs.hide_dialog(&mut c);
            }
            Binding::PushAnswer => {
                let layout = call
                    .str(1)
                    .map_or_else(|| crate::dialog::ANSWER_LUA.to_string(), |l| text(Some(l)));
                let now = self.clock_ms();
                let mut c = Ctx {
                    files: &self.files,
                    sounds: &mut self.sounds,
                    table: self.table.as_ref(),
                    now,
                };
                self.dialogs.push_answer(&mut c, &name, &layout);
            }
            Binding::HideAnswers => self.dialogs.hide_answers(),
            Binding::AddToBag => self.add_to_bag(&name, true),
            Binding::AddToBagWithoutNotification => self.add_to_bag(&name, false),
            Binding::RemoveFromBag => {
                self.gui();
                if self.inventory.remove_named(&name).is_none() {
                    log::info!("RemoveFromBag({name}): not in the bag");
                }
            }
            Binding::RemoveSelectedFromBag => {
                self.gui();
                let gone = self.inventory.remove_selected();
                log::info!("RemoveFromBag(): {gone:?}");
            }
            Binding::AddDocument => {
                self.gui();
                if !self.documents.add(&name) {
                    log::warn!("AddDocument({name}): no free slot, dropped");
                }
            }
            Binding::ShowDocument => self.show_document(&name, 0),
            Binding::HideDocument => self.hide_document(),
            Binding::LoadSprite => self.gui().load_sprite(&name),
            Binding::UnloadSprite => self.gui().unload_sprite(&name),
            Binding::SetInventoryAvatar => {
                log::info!("SetInventoryAvatar({name})");
                self.avatar = Some(name);
            }
            // Empty in the original (@ 0x0057e030, 0x0057df60).
            Binding::ClearImportantDocuments | Binding::SetImportantDocument => {}
            other => log::info!("{other:?}({name}): not done in the warp view yet"),
        }
        Reply::None
    }

    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        self.files.read(&text(Some(path)))
    }

    /// `Texts/<file>` is looked for in `Texts/Full/`, `Texts/PC/`, then
    /// `Texts/` (the `part` and `platform` values the application sets).
    fn resolve(&mut self, path: &[u8]) -> Vec<u8> {
        let Some(slash) = path.iter().position(|&c| c == b'/') else {
            return path.to_vec();
        };
        let (dir, file) = (&path[..slash], &path[slash + 1..]);
        if !dir.eq_ignore_ascii_case(b"texts") {
            return path.to_vec();
        }
        for sub in [&b"Texts/Full/"[..], b"Texts/PC/", b"Texts/"] {
            let p = [sub, file].concat();
            if self.files.read(&text(Some(&p))).is_some() {
                return p;
            }
        }
        path.to_vec()
    }

    fn has_scene(&self) -> bool {
        self.warp.is_some()
    }

    fn save_game(&mut self, file: &[u8], globals: &[SavedGlobal]) {
        self.build_save(&text(Some(file)), globals);
    }

    /// The new scene (`0x00526c70` @ 0x0052703c..0x00527265, then the XML
    /// @ 0x00527c1b): the limits reset (yaw and pitch kept), the FOV of
    /// the definition, the node's background, objects, animations, exits
    /// and markers, the global marker flag.
    fn enter_scene(&mut self, path: &[u8]) {
        // Step 4 of the node change: the `PlaySound` sounds stop.
        self.sounds.leave_node();
        let name = text(Some(path));
        self.warp_name = path.to_vec();
        self.view.reset_limits();
        self.fov = fov_for_height(self.height);
        self.warp = None;
        let Some(p) = WarpPath::parse(&name) else {
            log::warn!("ChangeWarp({name}): not a node");
            return;
        };
        match Warp::load(&self.files, &p) {
            Ok(mut w) => {
                w.markers_shown = self.markers_shown;
                self.warp = Some(w);
                self.serial += 1;
            }
            Err(e) => log::warn!("ChangeWarp({name}): {e}"),
        }
    }

    /// The callbacks a binding raised (`HideDialog`'s `OnDialogClosed`),
    /// after the sounds'.
    fn take_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        let mut out = self.sound_callbacks();
        out.extend(self.dialogs.take_callbacks());
        out.extend(self.gui_callbacks());
        out
    }

    fn event(&mut self, event: &Event<'_>) {
        if let Some(t) = &mut self.trace {
            match event {
                Event::Load(p) => t.push(format!("FILE {}", text(Some(p)))),
                Event::Enter(name, _) => t.push(format!("ENTER {name}")),
                _ => {}
            }
        }
        match event {
            Event::Warp(p) => log::info!("entering {}", text(Some(p))),
            Event::MissingFile(p) => log::warn!("missing script {}", text(Some(p))),
            Event::Error(site, msg) => {
                let site = match site {
                    ErrorSite::Load(f) | ErrorSite::Chunk(f) => text(Some(f)),
                    ErrorSite::Callback(c) => c.clone(),
                };
                log::warn!("Lua error in {site}: {}", text(Some(msg)));
            }
            Event::Missing(name) => {
                log::debug!("[TeLuaScript::Execute] La fonction : \"{name}\" n'existe pas.")
            }
            Event::Enter(name, _) => log::debug!("callback {name}"),
            _ => {}
        }
    }
}
