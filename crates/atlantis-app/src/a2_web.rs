//! Atlantis II's window in the browser ([`crate::a2`]'s web side): the
//! frame presented through WebGL 2 (the 2D canvas without it), drawn at
//! most [`crate::WEB_MAX_PIXELS`] pixels and enlarged by the graphics
//! card, as the first game's; the canvas fitted to the page; the pointer
//! lock taken on a click (a browser grants it on a user gesture only) and
//! its loss seen; the sound started on the first click or key (the same
//! rule); full screen through the page; and the testing aids the
//! end-to-end runs drive (`atlantis2_test`, `atlantis2_state`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use atlantis2_engine::hd::picture_rect;
use atlantis2_engine::{Engine, LoopType};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_time::{Duration, Instant};
use winit::window::Window;

use crate::a2::{CardParts, Picture};
use crate::gl_cube::{GlCube, SharedCube};
use crate::gl_present::{GlCompose, GlPresenter};

/// In the browser the HD view is drawn on one core: its tiles at most
/// 24 pixels (a view of 960 x 720), as the first game's
/// (`crate::HD_MAX_TILE`).
pub const HD_MAX_TILE: usize = 24;

/// A pause longer than this between two redraws (the tab hidden, the
/// browser busy) is not game time: the game was held, as the desktop
/// holds it without the focus.
const GAP: Duration = Duration::from_millis(250);

/// How long a pointer lock request may take before the page's state is
/// believed.
pub const LOCK_GRACE: Duration = Duration::from_millis(600);

/// The browser's state of the window.
pub struct WebFront {
    pub gl: Option<GlPresenter>,
    /// The HD view drawn by the graphics card (shared with the engine),
    /// the HD picture composed there, its sharp text overlay and the rows
    /// the overlay used, whether the composition is checked.
    cube: Option<Rc<RefCell<GlCube>>>,
    compose: Option<GlCompose>,
    text: Vec<u32>,
    text_rows: Option<(usize, usize)>,
    check: bool,
    /// The frame drawn for the graphics card.
    frame: Vec<u32>,
    /// The sound is started on the first click or key.
    pub audio_pending: bool,
    last_redraw: Option<Instant>,
    /// When the pointer lock was last asked for; whether the page has held
    /// it since; whether a request was refused (no lock in this browser).
    pub lock_asked: Option<Instant>,
    pub lock_seen: bool,
    pub lock_refused: bool,
    /// The canvas's size at the last presentation.
    presented: (u32, u32),
    /// The redraws until the state is recorded again.
    state_in: u32,
    /// `atlant2.cfg` as last written (the browser never closes the game:
    /// its changes are written as they are made).
    cfg_written: Vec<u8>,
    cfg_checked: Option<Instant>,
    pub times: FrameTimes,
}

impl Default for WebFront {
    fn default() -> Self {
        WebFront::new()
    }
}

impl WebFront {
    pub fn new() -> WebFront {
        WebFront {
            gl: None,
            cube: None,
            compose: None,
            text: Vec::new(),
            text_rows: None,
            check: false,
            frame: Vec::new(),
            audio_pending: true,
            last_redraw: None,
            lock_asked: None,
            lock_seen: false,
            lock_refused: false,
            presented: (0, 0),
            state_in: 0,
            cfg_written: Vec::new(),
            cfg_checked: None,
            times: FrameTimes::default(),
        }
    }

    /// The window's canvas: WebGL 2 on it when the browser has it, and
    /// the keyboard's focus; the HD view drawn by the graphics card
    /// (`atlantis-native/gpu-view` in the page's storage: `off` keeps it
    /// on the processor, `check` compares the two), the first game's
    /// setting.
    pub fn attach(&mut self, window: &Window, engine: &mut Engine) {
        use winit::platform::web::WindowExtWebSys;
        let Some(canvas) = window.canvas() else {
            return;
        };
        let _ = canvas.focus();
        self.gl = GlPresenter::new(canvas);
        log::info!(
            "presentation: {}",
            if self.gl.is_some() {
                "WebGL 2"
            } else {
                "2D canvas"
            }
        );
        let setting = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("atlantis-native/gpu-view").ok().flatten())
            .unwrap_or_default();
        if setting == "off" {
            return;
        }
        if let Some(c) = self.gl.as_ref().and_then(|p| GlCube::new(p.context())) {
            let c = Rc::new(RefCell::new(c));
            engine.cube_renderer = Some(Box::new(SharedCube(c.clone())));
            engine.view_check = setting == "check";
            self.check = setting == "check";
            self.cube = Some(c);
            self.compose = self.gl.as_ref().and_then(|p| GlCompose::new(p.context()));
            log::info!(
                "HD view: graphics card{}",
                if setting == "check" { " (checked)" } else { "" }
            );
        }
    }

    /// The size the frame is drawn at for a `ww` x `wh` canvas, with
    /// WebGL: at most [`crate::WEB_MAX_PIXELS`], and in the HD mode a
    /// picture no taller than the HD view of [`HD_MAX_TILE`] (the graphics
    /// card enlarges it). The canvas's own size without WebGL.
    pub fn frame_size(&self, window: (u32, u32), hd: bool) -> (usize, usize) {
        if self.gl.is_none() {
            return (window.0 as usize, window.1 as usize);
        }
        card_frame_size(window, hd)
    }

    /// Presents through WebGL: in the HD mode `compose` draws the picture
    /// at [`WebFront::frame_size`] and the card enlarges it smoothly;
    /// otherwise the screen (`screen`, its `w` x `h` pixels RGB565) goes
    /// as it is, each pixel a block of the 4:3 picture. False without
    /// WebGL.
    /// With `card`, the view is the graphics card's drawing (its number,
    /// the layer's width and widescreen columns): `compose` leaves it
    /// transparent and it is drawn under the picture.
    pub fn present(
        &mut self,
        window: (u32, u32),
        hd: bool,
        screen: (&[u16], usize, usize),
        card: Option<(u64, usize, usize)>,
        compose: impl FnOnce(&mut [u32], usize, usize),
    ) -> bool {
        let (sw, sh) = self.frame_size(window, true);
        let Some(gl) = &mut self.gl else {
            return false;
        };
        let (ww, wh) = window;
        if hd {
            self.frame.resize(sw * sh, 0);
            if card.is_none() {
                self.frame.fill(0);
            }
            compose(&mut self.frame, sw, sh);
            // The view's rectangle on the canvas: the picture's, with the
            // widescreen sides.
            let cube = self.cube.as_ref().map(|c| c.borrow());
            let under = card.and_then(|(gen, lw, side)| {
                let (ox, oy, dw, dh) = picture_rect(sw, sh);
                let cw = lw.saturating_sub(2 * side).max(1);
                let sd = side * dw / cw;
                let k = f64::from(ww) / sw as f64;
                let r = |v: f64| (v * k).round() as i32;
                let x = ox as f64 - sd as f64;
                let rect = (
                    r(x),
                    r(oy as f64),
                    r(x + (dw + 2 * sd) as f64) - r(x),
                    r((oy + dh) as f64) - r(oy as f64),
                );
                let t = cube.as_ref().and_then(|c| c.picture(gen));
                if t.is_none() {
                    log::warn!("HD view {gen} not on the graphics card");
                }
                t.map(|t| (t, rect))
            });
            gl.present(
                window,
                &self.frame,
                (sw as u32, sh as u32),
                (0, 0, ww as i32, wh as i32),
                true,
                under,
            );
        } else {
            let (pixels, w, h) = screen;
            let lut = atlantis2_engine::png::rgb565_to_xrgb();
            self.frame.clear();
            self.frame
                .extend(pixels[..w * h].iter().map(|&p| lut[usize::from(p)]));
            let (ox, oy, dw, dh) = picture_rect(ww as usize, wh as usize);
            gl.present(
                window,
                &self.frame,
                (w as u32, h as u32),
                (ox as i32, oy as i32, dw as i32, dh as i32),
                false,
                None,
            );
        }
        true
    }

    /// Whether the canvas's size changed since the last call.
    pub fn resized(&mut self, size: (u32, u32)) -> bool {
        std::mem::replace(&mut self.presented, size) != size
    }

    /// Whether the state for [`atlantis2_state`] is to be recorded (every
    /// 15th pass, four times a second).
    pub fn state_due(&mut self) -> bool {
        if self.state_in == 0 {
            self.state_in = 15;
            true
        } else {
            self.state_in -= 1;
            false
        }
    }

    /// The HD picture composed by the graphics card and shown (see
    /// [`Picture::compose_card`]); false when it cannot.
    pub fn present_card(
        &mut self,
        window: (u32, u32),
        picture: &mut Picture,
        engine: &Engine,
        screen: &[u16],
    ) -> bool {
        let size = self.frame_size(window, true);
        let (Some(gl), Some(gc)) = (self.gl.as_mut(), self.compose.as_mut()) else {
            return false;
        };
        let cube = self.cube.as_ref().map(|c| c.borrow());
        let card_view = engine
            .hd_layer()
            .filter(|l| l.on_card)
            .map(|l| cube.as_ref().and_then(|c| c.picture(l.card_gen)));
        let parts = CardParts {
            gl,
            gc,
            card_view,
            text: &mut self.text,
            text_rows: &mut self.text_rows,
            check: self.check,
        };
        picture.compose_card(engine, screen, parts, window, size)
    }

    /// A redraw at `now`: the time since the last one when it is a gap
    /// the game did not run through (see [`GAP`]).
    pub fn gap(&mut self, now: Instant) -> Option<(Instant, Instant)> {
        let last = self.last_redraw.replace(now)?;
        (now.saturating_duration_since(last) > GAP).then_some((last, now))
    }

    /// `atlant2.cfg` written when it changed (checked every two seconds).
    pub fn keep_cfg(&mut self, engine: &Engine, now: Instant) {
        if self
            .cfg_checked
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(2))
        {
            return;
        }
        self.cfg_checked = Some(now);
        let cfg = engine.cfg().to_bytes();
        if cfg[..] != self.cfg_written[..] {
            if !self.cfg_written.is_empty() {
                engine.write_cfg();
            }
            self.cfg_written = cfg.to_vec();
        }
    }
}

/// [`WebFront::frame_size`] with WebGL.
fn card_frame_size((ww, wh): (u32, u32), hd: bool) -> (usize, usize) {
    if ww == 0 || wh == 0 {
        return (ww as usize, wh as usize);
    }
    let (fw, fh) = (f64::from(ww), f64::from(wh));
    let mut k = (crate::WEB_MAX_PIXELS as f64 / (fw * fh)).sqrt().min(1.0);
    if hd {
        let picture_h = fh.min(fw * 3.0 / 4.0);
        k = k.min((HD_MAX_TILE * 480 / 16) as f64 / picture_h);
    }
    (
        ((fw * k).round() as usize).max(1),
        ((fh * k).round() as usize).max(1),
    )
}

/// Whether the page holds the pointer lock.
pub fn pointer_locked() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.pointer_lock_element())
        .is_some()
}

/// Asks the browser for the pointer lock on the window's canvas (granted
/// on a user gesture only; its refusal is seen by [`pointer_locked`],
/// not reported as an error).
pub fn request_lock(window: &Window) {
    use winit::platform::web::WindowExtWebSys;
    let Some(canvas) = window.canvas() else {
        return;
    };
    let ask = js_sys::Reflect::get(&canvas, &"requestPointerLock".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let Some(ask) = ask else { return };
    match ask.call0(&canvas) {
        // (newer browsers answer with a promise)
        Ok(r) => {
            if let Ok(p) = r.dyn_into::<js_sys::Promise>() {
                thread_local! {
                    static IGNORE: Closure<dyn FnMut(JsValue)> =
                        Closure::new(|_: JsValue| {});
                }
                IGNORE.with(|c| {
                    let _ = p.catch(c);
                });
            }
        }
        Err(e) => log::debug!("pointer lock: {e:?}"),
    }
}

/// Lets the pointer lock go.
pub fn exit_lock() {
    if let Some(d) = web_sys::window().and_then(|w| w.document()) {
        d.exit_pointer_lock();
    }
}

/// Calls the page's function `name` (`window.<name>()`), if it has it;
/// true when it did.
fn call_page(name: &str) -> bool {
    let f = web_sys::window().and_then(|w| js_sys::Reflect::get(&w, &name.into()).ok());
    match f.and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
        Some(f) => f.call0(&JsValue::NULL).is_ok(),
        None => false,
    }
}

/// Full screen on or off: the page's (it fits the canvas to the screen).
pub fn toggle_fullscreen() {
    call_page("atlantisToggleFullscreen");
}

/// The game quit: the page goes back to its start (the folder to
/// choose), or is loaded again.
pub fn quit() {
    if !call_page("atlantisQuit") {
        if let Some(w) = web_sys::window() {
            let _ = w.location().reload();
        }
    }
}

/// The frame times of the browser's runs: each pass and its
/// presentation, by what the frame showed.
#[derive(Default)]
pub struct FrameTimes {
    /// (what, pass ms, presentation ms), the latest last.
    samples: VecDeque<(&'static str, f64, f64)>,
}

/// The samples kept.
const SAMPLES: usize = 2000;

impl FrameTimes {
    pub fn add(&mut self, what: &'static str, pass: f64, present: f64) {
        if self.samples.len() == SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back((what, pass, present));
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Per kind of frame: the count, the mean, median, 95th percentile and
    /// the slowest of pass + presentation, and the means of each part, in
    /// milliseconds (JSON).
    pub fn report(&self) -> String {
        let mut kinds: Vec<&str> = self.samples.iter().map(|s| s.0).collect();
        kinds.sort_unstable();
        kinds.dedup();
        let parts: Vec<String> = kinds
            .iter()
            .map(|k| {
                let mut t: Vec<f64> = Vec::new();
                let (mut pass, mut present) = (0.0, 0.0);
                for s in self.samples.iter().filter(|s| s.0 == *k) {
                    t.push(s.1 + s.2);
                    pass += s.1;
                    present += s.2;
                }
                t.sort_by(f64::total_cmp);
                let n = t.len().max(1) as f64;
                let at = |q: f64| t[((t.len() - 1) as f64 * q).round() as usize];
                format!(
                    "\"{k}\":{{\"n\":{},\"mean\":{:.2},\"p50\":{:.2},\"p95\":{:.2},\"max\":{:.2},\"pass\":{:.2},\"present\":{:.2}}}",
                    t.len(),
                    t.iter().sum::<f64>() / n,
                    at(0.5),
                    at(0.95),
                    at(1.0),
                    pass / n,
                    present / n
                )
            })
            .collect();
        format!("{{{}}}", parts.join(","))
    }
}

/// What a frame shows, for [`FrameTimes`].
pub fn frame_kind(engine: &Engine, menu_open: bool) -> &'static str {
    if menu_open {
        return "options";
    }
    let hd = engine.hd_layer().is_some();
    match engine.loop_type {
        LoopType::Warp if hd => "view-hd",
        LoopType::Warp => "view",
        LoopType::Dialog if hd => "dialog-hd",
        LoopType::Dialog => "dialog",
        LoopType::Video => "video",
        LoopType::Menu => "menu",
        _ => "other",
    }
}

thread_local! {
    /// The testing aids' commands, run at the next pass.
    static COMMANDS: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
    /// The game's state after the last pass (JSON).
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A testing aid for the end-to-end runs (the desktop's `--view`,
/// `--command`...), run before the next pass: `aim <view>` turns the view
/// onto the move zone to view `<view>` (the click is the runner's, a
/// real one at the canvas's centre); `command <(script commands)>` runs
/// them; `times` clears the frame times.
#[wasm_bindgen]
pub fn atlantis2_test(command: String) {
    COMMANDS.with(|c| c.borrow_mut().push_back(command));
}

/// The game's state after the last pass, for the end-to-end runs (JSON:
/// the loop, the place, the dialog, the video, the menu page, the frame
/// times).
#[wasm_bindgen]
pub fn atlantis2_state() -> String {
    STATE.with(|s| s.borrow().clone())
}

/// The testing aids asked for, run on `engine`.
pub fn run_test_commands(engine: &mut Engine, times: &mut FrameTimes) {
    let commands: Vec<String> = COMMANDS.with(|c| c.borrow_mut().drain(..).collect());
    for c in commands {
        let (verb, arg) = c.split_once(' ').unwrap_or((c.as_str(), ""));
        match verb {
            "aim" => match arg
                .trim()
                .parse::<u32>()
                .ok()
                .and_then(|to| aim(engine, to))
            {
                Some((yaw, pitch)) => log::info!(
                    "test: aimed at the move to view {arg} ({:.1}, {:.1})",
                    yaw.to_degrees(),
                    pitch.to_degrees()
                ),
                None => log::warn!("test: no move zone to view {arg} in reach"),
            },
            "command" => {
                for part in arg
                    .split_inclusive(')')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                {
                    engine.run_command(part.as_bytes());
                }
            }
            "times" => times.clear(),
            _ => log::warn!("test: {c}?"),
        }
    }
}

/// Turns the view onto a move zone to view `to` (the zone's aim, then
/// nearby when another zone or a sprite is under the centre there).
fn aim(engine: &mut Engine, to: u32) -> Option<(f32, f32)> {
    let view = engine.place().1;
    let zones: Vec<usize> = engine
        .wam()
        .view(view)?
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| z.kind == 1 && z.value == to as i32)
        .map(|(i, _)| i)
        .collect();
    let off = 0.6f32.to_radians();
    for &z in &zones {
        let (yaw, pitch) = engine.zone_aim(view, z)?;
        for (y, p) in [
            (yaw, pitch),
            (yaw + off, pitch),
            (yaw - off, pitch),
            (yaw, pitch + off),
            (yaw, pitch - off),
        ] {
            engine.camera.enter(y, p);
            if engine.centre_zone().map(|(i, _)| i) == Some(z) {
                return Some((y, p));
            }
        }
    }
    None
}

/// Keeps the game's state for [`atlantis2_state`].
pub fn record_state(engine: &Engine, times: &FrameTimes, audio: bool) {
    let (wam, view) = engine.place();
    let (yaw, pitch) = engine.angles_deg();
    let chapter = engine.script_state().map_or(-1, |s| s.chapter);
    let dialog = engine.dialog().and_then(|d| {
        d.current_line()
            .map(|l| (d.number, l.id.clone(), l.speaker.clone()))
    });
    let dialog = match dialog {
        Some((n, id, speaker)) => format!(
            "{{\"number\":{n},\"line\":{},\"speaker\":{}}}",
            json(&id.to_string()),
            json(&speaker.to_string())
        ),
        None => "null".to_string(),
    };
    let video = engine
        .video_playing()
        .map_or("null".to_string(), |(n, f)| format!("[{},{f}]", json(n)));
    let page = engine
        .menu()
        .and_then(|m| m.kind())
        .map_or("null".to_string(), |p| json(&format!("{p:?}")));
    // The menu page's buttons (name, hit box on the 640 x 480 screen).
    let widgets: Vec<String> = engine
        .menu()
        .and_then(|m| m.page.as_ref())
        .map(|p| {
            p.widgets
                .iter()
                .filter(|w| w.visible && w.enabled)
                .map(|w| {
                    format!(
                        "[{},{},{},{},{}]",
                        json(&w.name),
                        w.hit.left,
                        w.hit.top,
                        w.hit.right,
                        w.hit.bottom
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let zone = engine.centre_zone().map_or("null".to_string(), |(i, z)| {
        format!("[{i},{},{}]", z.kind, z.value)
    });
    let s = format!(
        "{{\"loop\":{},\"wam\":{},\"view\":{view},\"yaw\":{yaw:.2},\"pitch\":{pitch:.2},\
         \"zone\":{zone},\"chapter\":{chapter},\"dialog\":{dialog},\"video\":{video},\
         \"intro\":{},\"page\":{page},\"widgets\":[{}],\"hd\":{},\"audio\":{audio},\"times\":{}}}",
        json(&format!("{:?}", engine.loop_type)),
        json(wam),
        engine.intro_playing(),
        widgets.join(","),
        engine.hd_layer().is_some(),
        times.report()
    );
    STATE.with(|st| *st.borrow_mut() = s);
}

/// A JSON string.
fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn frame_sizes() {
        // A small canvas: its own size; a large one: at most 1280 x 960
        // pixels, and in HD a picture no taller than the HD view (720).
        assert_eq!(card_frame_size((800, 600), false), (800, 600));
        assert_eq!(card_frame_size((2560, 1920), false), (1280, 960));
        assert_eq!(card_frame_size((1920, 1440), true), (960, 720));
        assert_eq!(card_frame_size((1280, 960), true), (960, 720));
        assert_eq!(card_frame_size((640, 480), true), (640, 480));
        // Widescreen: the picture's height, or the pixels' count.
        assert_eq!(card_frame_size((1920, 800), true), (1717, 716));
        assert_eq!(card_frame_size((2400, 1000), true), (1717, 716));
        assert_eq!(card_frame_size((0, 0), true), (0, 0));
    }

    #[test]
    fn frame_times_report() {
        let mut t = FrameTimes::default();
        assert_eq!(t.report(), "{}");
        for i in 0..10 {
            t.add("view-hd", f64::from(i), 1.0);
        }
        t.add("dialog", 4.0, 0.5);
        let r = t.report();
        assert!(r.starts_with("{\"dialog\":{\"n\":1,\"mean\":4.50,"), "{r}");
        assert!(
            r.contains("\"view-hd\":{\"n\":10,\"mean\":5.50,\"p50\":6.00,\"p95\":10.00,\"max\":10.00,\"pass\":4.50,\"present\":1.00}"),
            "{r}"
        );
        for _ in 0..SAMPLES {
            t.add("video", 1.0, 1.0);
        }
        assert!(!t.report().contains("dialog"));
        t.clear();
        assert_eq!(t.report(), "{}");
    }

    #[test]
    fn json_strings() {
        assert_eq!(json("tibet.wam"), "\"tibet.wam\"");
        assert_eq!(json("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
    }
}
