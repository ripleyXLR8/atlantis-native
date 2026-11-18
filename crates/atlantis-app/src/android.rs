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

/// The entry point android-activity calls on its own thread, once the
/// activity is created.
#[no_mangle]
fn android_main(activity: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("AtlantisNative"),
    );
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
    match game_dir.as_deref().map(find_game) {
        Some(Found::Game(opened, dir)) => app.play_opened(opened, &dir),
        Some(Found::Other(label, dir)) => {
            app.pattern = notice(&[
                format!("{label} found in"),
                dir.display().to_string(),
                String::new(),
                "This first Android version plays".to_string(),
                "Atlantis: The Lost Tales only.".to_string(),
            ]);
        }
        Some(Found::Nothing(dir)) => {
            log::info!("No game found: copy it to {}", dir.display());
            app.pattern = notice(&[
                "No game found.".to_string(),
                String::new(),
                "Copy the files of Atlantis: The Lost Tales".to_string(),
                "(the CD's files, or the images of the CDs) to".to_string(),
                dir.display().to_string(),
                String::new(),
                "then start the app again.".to_string(),
            ]);
        }
        None => {
            app.pattern = notice(&["No storage for the game's files.".to_string()]);
        }
    }
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("event loop: {e}");
    }
    log::info!("event loop ended");
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
    let font = SubtitleFont::new();
    let max_w = w as i32 - 2 * MARGIN;
    let mut rows: Vec<String> = Vec::new();
    for line in lines {
        rows.extend(wrap(line, |s| font.width(s.as_bytes()), max_w));
    }
    let step = font.row_step().max(16) + 4;
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
            self.touch = TouchState::default();
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
                    self.touch = TouchState::default();
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
        self.touch = TouchState::default();
        self.suspended_at.get_or_insert_with(web_time::Instant::now);
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
