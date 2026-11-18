//! Android (milestone M0, docs/ANDROID.md): the first game in a
//! NativeActivity, its frames through softbuffer (the window's
//! `ANativeWindow`), its sound through cpal's AAudio output, the touch
//! screen as the mouse.
//!
//! The game is read from the app's folder on the shared storage
//! (`<getExternalFilesDir>/game`, e.g.
//! `/sdcard/Android/data/io.github.ripleyxlr8.atlantisnative/files/game`),
//! which the app reads without a permission: the game's files, or the
//! images of its CDs, there or in a folder directly inside it. Without
//! a game there, the screen says where to copy it. The players, the
//! checkpoints and `settings.ini` are kept in the app's internal files
//! folder ([`atlantis_engine::persist::user_data_dir`]).
//!
//! Touch: a tap is a left click where the finger is (the panoramic view:
//! a click at the centre), a drag moves the pointer with the button held
//! (the panoramic view: turns it), a tap with two fingers is a right
//! click, three fingers open the port's options menu. The Back button
//! is Escape.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use atlantis_engine::audio::Mixer;
use atlantis_engine::subtitles::SubtitleFont;
use atlantis_engine::sys::{Input, MouseButton, Sys};
use atlantis_engine::warp::{SCREEN_H, SCREEN_W};
use atlantis_engine::Start;
use winit::event::{Touch, TouchPhase};
use winit::event_loop::EventLoop;
use winit::platform::android::activity::{AndroidApp, WindowManagerFlags};
use winit::platform::android::EventLoopBuilderExtAndroid;

use crate::desktop::{data_dir, open_any, Opened};
use crate::session::Session;
use crate::App;

/// The activity, for the on-screen keyboard.
static ACTIVITY: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();

/// A finger moving less than this (window pixels) from where it went
/// down makes a tap, not a drag (the panoramic view).
const TAP_SLOP: f64 = 24.0;

/// The activity is being destroyed: its `onDestroy` waits for
/// `android_main` to return, which winit 0.30 does not ask for (it logs
/// the event and goes on): [`App`] ends its event loop when this is set.
static DESTROYED: AtomicBool = AtomicBool::new(false);

/// android-activity's `onDestroy`, which [`on_destroy`] calls.
static ANDROID_ACTIVITY_ON_DESTROY: std::sync::OnceLock<
    unsafe extern "C" fn(*mut ndk_sys::ANativeActivity),
> = std::sync::OnceLock::new();

extern "C" {
    /// android-activity's entry point (glue.rs).
    fn ANativeActivity_onCreate(
        activity: *mut ndk_sys::ANativeActivity,
        saved_state: *const std::ffi::c_void,
        saved_state_size: usize,
    );
}

/// The activity's entry point (the manifest's `android.app.func_name`):
/// android-activity's, then its `onDestroy` replaced by [`on_destroy`].
///
/// # Safety
/// Called by NativeActivity with its `ANativeActivity`.
#[no_mangle]
pub unsafe extern "C" fn atlantis_native_on_create(
    activity: *mut ndk_sys::ANativeActivity,
    saved_state: *const std::ffi::c_void,
    saved_state_size: usize,
) {
    ANativeActivity_onCreate(activity, saved_state, saved_state_size);
    let callbacks = (*activity).callbacks;
    if let Some(f) = (*callbacks).onDestroy {
        let _ = ANDROID_ACTIVITY_ON_DESTROY.set(f);
        (*callbacks).onDestroy = Some(on_destroy);
    }
}

/// The activity's `onDestroy`: [`DESTROYED`] set before android-activity's
/// own wakes the event loop (and waits for `android_main` to return).
unsafe extern "C" fn on_destroy(activity: *mut ndk_sys::ANativeActivity) {
    log::info!("activity destroyed");
    DESTROYED.store(true, Ordering::SeqCst);
    if let Some(f) = ANDROID_ACTIVITY_ON_DESTROY.get() {
        f(activity);
    }
}

/// Whether the activity is being destroyed (the event loop must end).
pub(crate) fn destroyed() -> bool {
    DESTROYED.load(Ordering::SeqCst)
}

/// The entry point android-activity calls on its own thread, once the
/// activity is created.
#[no_mangle]
fn android_main(activity: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_filter(
                android_logger::FilterBuilder::new()
                    .parse("info,android_activity=debug,winit=debug")
                    .build(),
            )
            .with_tag("AtlantisNative"),
    );
    // A handler that does not return is logged (its phase).
    crate::watchdog::start();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("{info}");
        default_hook(info);
    }));
    log::info!("{}", crate::version_string());
    let _ = ACTIVITY.set(activity.clone());
    if let Some(dir) = activity.internal_data_path() {
        atlantis_engine::persist::set_android_data_dir(dir);
    }
    log::info!("data folder {}", data_dir().display());
    // The screen stays on while the game runs, without the status bar.
    activity.set_window_flags(
        WindowManagerFlags::KEEP_SCREEN_ON | WindowManagerFlags::FULLSCREEN,
        WindowManagerFlags::empty(),
    );
    let game_dir = activity.external_data_path().map(|p| p.join("game"));
    if let Some(dir) = &game_dir {
        // Made at once, so that it shows in a file manager or over USB.
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("{}: {e}", dir.display());
        }
        log::info!("game folder {}", dir.display());
    }
    crate::android_saf::init(activity.vm_as_ptr(), activity.activity_as_ptr());
    let event_loop = match EventLoop::builder().with_android_app(activity).build() {
        Ok(el) => el,
        Err(e) => {
            log::error!("no event loop: {e}");
            return;
        }
    };
    let session = Session::new(data_dir(), false, false, false, Start::Boot);
    let mut app = App::new(Sys::new(Mixer::new()), None).with_session(session);
    // (no game library's screen on Android yet)
    app.menu.set_library(false);
    app.android_start(game_dir);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("event loop: {e}");
    }
    drop(app);
    // The process ends with the activity: winit makes one event loop per
    // process, and the activity Android may create again in it would have
    // none.
    log::info!("event loop ended");
    std::process::exit(0);
}

/// The file keeping the folder chosen in the picker (its tree URI).
fn chosen_folder_file() -> PathBuf {
    data_dir().join("android-game-folder.txt")
}

/// What a folder chosen in the picker holds.
enum FoundInTree {
    Game(Box<atlantis_data::Game>),
    /// Another game (its name): not played from a chosen folder yet.
    Other(&'static str),
    Nothing(String),
}

/// The game in the chosen folder `tree` or in a folder directly inside it
/// (the first that opens, in name order): its files, or the images of its
/// CDs.
fn find_game_in_tree(tree: &str) -> FoundInTree {
    use atlantis_data::FileSource;
    let Some(top) = crate::android_saf::SafDir::top(tree) else {
        return FoundInTree::Nothing("the folder cannot be read".to_string());
    };
    let mut candidates = vec![top];
    let subs = candidates[0].subfolders();
    candidates.extend(subs);
    let mut first_error = None;
    let mut other = None;
    for c in candidates {
        // The other games, by their program files.
        let names: Vec<String> = c.list("").iter().map(|n| n.to_lowercase()).collect();
        let has = |n: &str| names.iter().any(|x| x == n);
        if has("atlantis2.exe") {
            other.get_or_insert("Atlantis II");
        } else if has("atlantis evolution.exe") {
            other.get_or_insert("Atlantis Evolution");
        } else if has("game.exe") && has("datas") {
            other.get_or_insert("Atlantis III");
        }
        let c = Arc::new(c);
        let opened = match atlantis_data::Game::open(c.clone()) {
            Ok(g) => Ok(g),
            Err(e) => match c.disc_set() {
                Some(set) => {
                    log::info!("the game from {} disc images", set.images.len());
                    atlantis_data::Game::open(Arc::new(set))
                }
                None => Err(e),
            },
        };
        match opened {
            Ok(g) => return FoundInTree::Game(Box::new(g)),
            Err(e) => {
                log::info!("chosen folder: {e}");
                first_error.get_or_insert(e.to_string());
            }
        }
    }
    match other {
        Some(name) => FoundInTree::Other(name),
        None => FoundInTree::Nothing(first_error.unwrap_or_default()),
    }
}

/// The tap's line at the foot of the "no game" screen.
const TAP_TO_CHOOSE: &str = "Tap the screen to choose the game's folder.";

impl App {
    /// The game at start-up: in the folder chosen before (while the app
    /// may still read it), else in the app's own folder `game_dir`, else
    /// the screen saying where to put it (a tap opens the picker).
    fn android_start(&mut self, game_dir: Option<PathBuf>) {
        if let Ok(tree) = std::fs::read_to_string(chosen_folder_file()) {
            let tree = tree.trim().to_string();
            if !tree.is_empty() {
                if crate::android_saf::can_read(&tree) {
                    log::info!("the folder chosen before: {tree}");
                    if self.open_tree(&tree, false) {
                        return;
                    }
                } else {
                    log::info!("the folder chosen before can no longer be read: {tree}");
                }
            }
        }
        let Some(dir) = game_dir else {
            self.show_notice(&["No storage for the game's files.".to_string()]);
            return;
        };
        match find_game(&dir) {
            Found::Game(opened, dir) => self.play_opened(opened, &dir),
            Found::Other(label, dir) => {
                self.show_notice(&[
                    format!("{label} found in"),
                    dir.display().to_string(),
                    String::new(),
                    "This Android version plays Atlantis: The Lost Tales only.".to_string(),
                ]);
            }
            Found::Nothing(dir) => {
                log::info!("No game found: copy it to {}", dir.display());
                self.show_notice(&[
                    "No game found.".to_string(),
                    String::new(),
                    "Choose the folder of Atlantis: The Lost Tales (the CD's files, or the images of the CDs),".to_string(),
                    "or copy the game to".to_string(),
                    dir.display().to_string(),
                    "and start the app again.".to_string(),
                ]);
            }
        }
    }

    /// Plays the game in the chosen folder `tree` (kept for the next
    /// starts); else, when `explain`, the screen says what was found.
    /// Whether a game started.
    fn open_tree(&mut self, tree: &str, explain: bool) -> bool {
        match find_game_in_tree(tree) {
            FoundInTree::Game(game) => {
                if let Err(e) = std::fs::create_dir_all(data_dir())
                    .and_then(|()| std::fs::write(chosen_folder_file(), tree))
                {
                    log::warn!("the chosen folder not kept: {e}");
                }
                self.touch.picker = false;
                self.play_game(*game, None);
                true
            }
            FoundInTree::Other(name) => {
                if explain {
                    self.show_notice(&[
                        format!("{name} found in the chosen folder."),
                        String::new(),
                        "This Android version plays Atlantis: The Lost Tales only.".to_string(),
                    ]);
                }
                false
            }
            FoundInTree::Nothing(why) => {
                if explain {
                    self.show_notice(&[
                        "No game found in the chosen folder.".to_string(),
                        why,
                        String::new(),
                        "Choose the folder holding atlantis.exe and the BIGCD files,".to_string(),
                        "or the images of the CDs.".to_string(),
                    ]);
                }
                false
            }
        }
    }

    /// The "no game" screen with `lines`, and the tap that opens the
    /// picker.
    fn show_notice(&mut self, lines: &[String]) {
        let mut all = lines.to_vec();
        all.push(String::new());
        all.push(TAP_TO_CHOOSE.to_string());
        self.pattern = notice(&all);
        self.touch.picker = true;
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The picker's answer, once it came (the activity resumed).
    pub(crate) fn poll_picked(&mut self) {
        let Some(tree) = crate::android_saf::take_picked() else {
            return;
        };
        if tree.is_empty() || self.runner.is_some() {
            return;
        }
        self.open_tree(&tree, true);
    }
}

/// What the game folder holds.
enum Found {
    Game(Opened, PathBuf),
    /// Another game (its label): not playable in this version.
    Other(String, PathBuf),
    Nothing(PathBuf),
}

/// The game in `dir` or in a folder directly inside it (the first that
/// opens, in name order).
fn find_game(dir: &Path) -> Found {
    let mut candidates = vec![dir.to_path_buf()];
    let mut subs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    subs.sort();
    candidates.extend(subs);
    let mut other = None;
    for c in candidates {
        match open_any(&c) {
            Ok(opened @ Opened::LostTales(..)) => {
                log::info!("{}: {}", c.display(), opened.label());
                return Found::Game(opened, c);
            }
            Ok(opened) => {
                log::info!(
                    "{}: {} (not played on Android yet)",
                    c.display(),
                    opened.label()
                );
                other.get_or_insert((opened.label(), c));
            }
            Err(e) => log::info!("{}: {e}", c.display()),
        }
    }
    match other {
        Some((label, c)) => Found::Other(label, c),
        None => Found::Nothing(dir.to_path_buf()),
    }
}

/// A 640x480 screen (RGB565) with `lines` of text, centred, wrapped at
/// the slashes and spaces to fit.
fn notice(lines: &[String]) -> Vec<u16> {
    const BG: u16 = 0x0008; // dark blue
    const FG: u16 = 0xffff;
    const MARGIN: i32 = 24;
    let (w, h) = (SCREEN_W, SCREEN_H);
    let mut out = vec![BG; w * h];
    // Large enough to be read on a small screen, where the 640x480
    // picture is drawn smaller.
    let font = SubtitleFont::with_size(22.0);
    let max_w = w as i32 - 2 * MARGIN;
    let mut rows: Vec<String> = Vec::new();
    for line in lines {
        rows.extend(wrap(line, |s| font.width(s.as_bytes()), max_w));
    }
    let step = font.row_step().max(26) + 4;
    let total = step * rows.len() as i32;
    let mut y = (h as i32 - total) / 2;
    for row in &rows {
        let tw = font.width(row.as_bytes());
        let x = ((w as i32 - tw) / 2).max(MARGIN);
        font.draw(&mut out, w, h, row.as_bytes(), x, y, max_w, FG);
        y += step;
    }
    out
}

/// `line` cut into rows no wider than `max_w`, after a space or a slash
/// where it can, else anywhere.
fn wrap(line: &str, width: impl Fn(&str) -> i32, max_w: i32) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    let mut rows = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        if width(rest) <= max_w {
            rows.push(rest.to_string());
            break;
        }
        // The longest prefix that fits.
        let mut fit = 0;
        for (i, c) in rest.char_indices() {
            let end = i + c.len_utf8();
            if width(&rest[..end]) > max_w {
                break;
            }
            fit = end;
        }
        let fit = fit.max(rest.chars().next().map_or(1, char::len_utf8));
        let cut = rest[..fit]
            .rfind([' ', '/'])
            .map(|i| i + 1)
            .filter(|&i| i > 0 && i < fit)
            .unwrap_or(fit);
        rows.push(rest[..cut].trim_end().to_string());
        rest = &rest[cut..];
    }
    rows
}

/// A finger down: its id, where it went down, where it is (window
/// pixels).
type Finger = (u64, (f64, f64), (f64, f64));

/// The touch screen as the mouse (see the module's documentation).
#[derive(Default)]
pub(crate) struct TouchState {
    /// The fingers down.
    fingers: Vec<Finger>,
    /// The finger that is the pointer.
    primary: Option<u64>,
    /// A second finger came down: the gesture is a right click.
    two: bool,
    /// The panoramic view: the finger moved past [`TAP_SLOP`].
    dragged: bool,
    /// The left button is down in the engine (or queued down).
    left_down: bool,
    /// The "no game" screen: a tap opens the folder picker.
    pub(crate) picker: bool,
    /// The on-screen keyboard is shown (the player screen's name entry).
    keyboard: bool,
    /// The engine's inputs, one per engine step (a press is seen after
    /// the pointer's move, a release after its press).
    queue: VecDeque<Input>,
}

impl App {
    /// A finger on the screen.
    pub(crate) fn touch(&mut self, t: Touch) {
        let (x, y) = (t.location.x, t.location.y);
        let (sx, sy) = self.viewport.to_screen(x, y);
        // The port's options menu: a tap is its click.
        if self.menu.is_open() {
            self.touch = TouchState {
                picker: self.touch.picker,
                ..TouchState::default()
            };
            self.menu.pointer(sx, sy);
            if t.phase == TouchPhase::Ended {
                let action = self.menu.click(sx, sy, self.menu_values());
                self.menu_action(action);
            }
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            return;
        }
        // The "no game" screen: a tap opens the folder picker.
        if self.runner.is_none() && self.touch.picker {
            if t.phase == TouchPhase::Ended && crate::android_saf::pick_folder() {
                log::info!("folder picker opened");
            }
            return;
        }
        let st = &mut self.touch;
        match t.phase {
            TouchPhase::Started => {
                st.fingers.retain(|f| f.0 != t.id);
                st.fingers.push((t.id, (x, y), (x, y)));
                if st.fingers.len() >= 3 {
                    // Three fingers: the port's options menu (F10).
                    cancel_left(st, self.sys.mouse());
                    let queue = std::mem::take(&mut st.queue);
                    for i in queue {
                        self.sys.push_input(i);
                    }
                    self.touch = TouchState {
                        picker: self.touch.picker,
                        ..TouchState::default()
                    };
                    self.open_menu();
                    return;
                }
                if st.primary.is_none() {
                    st.primary = Some(t.id);
                    st.two = false;
                    st.dragged = false;
                    if !self.locked {
                        self.sys.push_input(Input::MouseMove { x: sx, y: sy });
                        st.queue.push_back(Input::MouseDown {
                            button: MouseButton::Left,
                            x: sx,
                            y: sy,
                        });
                        st.left_down = true;
                    }
                } else if !st.two {
                    // A second finger: no left click, a right click when
                    // the fingers lift.
                    st.two = true;
                    cancel_left(st, self.sys.mouse());
                }
            }
            TouchPhase::Moved => {
                let Some(f) = st.fingers.iter_mut().find(|f| f.0 == t.id) else {
                    return;
                };
                let (lx, ly) = f.2;
                f.2 = (x, y);
                let from = f.1;
                if st.primary != Some(t.id) || st.two {
                    return;
                }
                if self.locked {
                    // The panoramic view turns with the finger, as with
                    // the mouse.
                    if (x - from.0).hypot(y - from.1) > TAP_SLOP {
                        st.dragged = true;
                    }
                    let s = self.viewport.scale.max(1e-6);
                    self.relative((x - lx) / s, (y - ly) / s);
                } else {
                    let m = Input::MouseMove { x: sx, y: sy };
                    if st.queue.is_empty() {
                        self.sys.push_input(m);
                    } else {
                        st.queue.push_back(m);
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let primary = st.primary == Some(t.id);
                st.fingers.retain(|f| f.0 != t.id);
                if primary && !st.two {
                    if t.phase == TouchPhase::Cancelled {
                        cancel_left(st, (sx, sy));
                    } else if st.left_down {
                        st.queue.push_back(Input::MouseUp {
                            button: MouseButton::Left,
                            x: sx,
                            y: sy,
                        });
                        st.left_down = false;
                    } else if self.locked && !st.dragged {
                        // The panoramic view: a tap clicks where the view
                        // points.
                        let (mx, my) = self.sys.mouse();
                        let button = MouseButton::Left;
                        st.queue.push_back(Input::MouseDown {
                            button,
                            x: mx,
                            y: my,
                        });
                        st.queue.push_back(Input::MouseUp {
                            button,
                            x: mx,
                            y: my,
                        });
                    }
                }
                if st.fingers.is_empty() {
                    if st.two && t.phase == TouchPhase::Ended {
                        let (mx, my) = self.sys.mouse();
                        let button = MouseButton::Right;
                        st.queue.push_back(Input::MouseDown {
                            button,
                            x: mx,
                            y: my,
                        });
                        st.queue.push_back(Input::MouseUp {
                            button,
                            x: mx,
                            y: my,
                        });
                    }
                    st.primary = None;
                    st.two = false;
                    st.dragged = false;
                }
            }
        }
        self.last_cursor = Some((x, y));
    }

    /// Gives the engine the next queued touch input (before each step),
    /// and shows the on-screen keyboard while the player screen takes
    /// the letters (hidden after).
    pub(crate) fn feed_touch(&mut self) {
        if let Some(i) = self.touch.queue.pop_front() {
            self.sys.push_input(i);
        }
        let entry = self
            .sys
            .takes_key_repeat(atlantis_engine::sys::Key::Char('a'));
        if entry != self.touch.keyboard {
            self.touch.keyboard = entry;
            if let Some(a) = ACTIVITY.get() {
                if entry {
                    a.show_soft_input(true);
                } else {
                    a.hide_soft_input(true);
                }
            }
        }
    }

    /// The activity went to the background (its window is gone): the
    /// window, its surface and the sound output are dropped, the game's
    /// clock stops.
    pub(crate) fn android_suspend(&mut self) {
        log::info!("suspended");
        self.surface = None;
        self.window = None;
        self.audio = None;
        self.audio_pending = true;
        self.sys.mixer.set_paused(true);
        self.touch = TouchState {
            picker: self.touch.picker,
            ..TouchState::default()
        };
        self.suspended_at.get_or_insert_with(web_time::Instant::now);
        log::info!("suspended: window dropped");
    }

    /// The activity is back (a new window): the game goes on where it
    /// was, its sound output opened again.
    pub(crate) fn android_resume(&mut self) {
        if let Some(t) = self.suspended_at.take() {
            log::info!("resumed");
            if self.paused_at.is_none() {
                let now = web_time::Instant::now();
                self.start += now.duration_since(t);
                self.next_frame = now;
                self.sys.mixer.set_paused(false);
            }
        }
        if self.runner.is_some() {
            self.start_audio();
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

/// The left button's press withdrawn: dropped while still queued, else
/// released at `at`.
fn cancel_left(st: &mut TouchState, at: (i32, i32)) {
    if !st.left_down {
        return;
    }
    st.left_down = false;
    let before = st.queue.len();
    st.queue.retain(|i| {
        !matches!(
            i,
            Input::MouseDown {
                button: MouseButton::Left,
                ..
            }
        )
    });
    if st.queue.len() == before {
        st.queue.push_back(Input::MouseUp {
            button: MouseButton::Left,
            x: at.0,
            y: at.1,
        });
    } else {
        // (its moves go with it)
        st.queue.retain(|i| !matches!(i, Input::MouseMove { .. }));
    }
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wraps_at_slashes() {
        let rows = wrap("/storage/emulated/0/Android/data", |s| s.len() as i32, 12);
        assert!(rows.iter().all(|r| r.len() <= 12), "{rows:?}");
        assert_eq!(rows.concat(), "/storage/emulated/0/Android/data");
    }
}
