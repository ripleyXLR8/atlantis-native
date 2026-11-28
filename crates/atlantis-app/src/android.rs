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
//! click, three fingers open the port's options menu. The bag button in
//! the black bar right of the picture is the right click too (the bag,
//! README's "Inventory"), the menu button in the bar on its left the
//! options menu. The Back button is Escape.

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

/// The front end of an activity Android destroyed (relaunched, or
/// destroyed in the background while the process lives on), for the next
/// activity of the process: the game goes on where it was.
static CARRIED: std::sync::Mutex<Option<Carried>> = std::sync::Mutex::new(None);

/// [`App`] between two activities' threads.
struct Carried(App);

// SAFETY: one `android_main` runs at a time (an activity's `onDestroy`
// waits for its end before the next activity is created), and the front
// end is handed over without its window, surface and sound stream
// ([`App::android_suspend`]): what is left (the engine, its `Rc`s and
// cells) is used by one thread after the other, never by two.
unsafe impl Send for Carried {}

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

/// The sound of Atlantis II, III or Evolution held (`pause`) while the
/// app is in the background, or played again.
pub(crate) fn pause_audio(stream: &Option<cpal::Stream>, pause: bool) {
    use cpal::traits::StreamTrait;
    if let Some(s) = stream {
        let r = if pause {
            s.pause().map_err(|e| e.to_string())
        } else {
            s.play().map_err(|e| e.to_string())
        };
        if let Err(e) = r {
            log::warn!("sound output: {e}");
        }
    }
}

/// The fingers on the screen in Atlantis II, III or Evolution's window.
#[derive(Default)]
struct ModeTouch {
    fingers: Vec<u64>,
    /// A second finger came: a right click when they lift.
    two: bool,
    /// The left button is down.
    left: bool,
    /// The first finger's place.
    at: (f64, f64),
}

static MODE_TOUCH: std::sync::Mutex<Option<ModeTouch>> = std::sync::Mutex::new(None);

/// Atlantis II, III and Evolution's windows on Android: the events to
/// handle in place of `event`, `None` to handle it as it is. A finger is
/// the mouse with its left button held (a tap: a click there), a tap
/// with two fingers a right click, the Back button Escape.
pub(crate) fn mode_events(
    event: &winit::event::WindowEvent,
) -> Option<Vec<winit::event::WindowEvent>> {
    use winit::dpi::PhysicalPosition;
    use winit::event::{DeviceId, ElementState, MouseButton as Button, WindowEvent};
    use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
    let device_id = DeviceId::dummy();
    let moved = |(x, y): (f64, f64)| WindowEvent::CursorMoved {
        device_id,
        position: PhysicalPosition::new(x, y),
    };
    let button = |state, button| WindowEvent::MouseInput {
        device_id,
        state,
        button,
    };
    match event {
        WindowEvent::KeyboardInput {
            device_id,
            event: k,
            is_synthetic,
        } if k.logical_key == Key::Named(NamedKey::BrowserBack) => {
            let mut k = k.clone();
            k.logical_key = Key::Named(NamedKey::Escape);
            k.physical_key = PhysicalKey::Code(KeyCode::Escape);
            Some(vec![WindowEvent::KeyboardInput {
                device_id: *device_id,
                event: k,
                is_synthetic: *is_synthetic,
            }])
        }
        WindowEvent::Touch(t) => {
            let mut guard = MODE_TOUCH.lock().unwrap();
            let st = guard.get_or_insert_with(ModeTouch::default);
            let at = (t.location.x, t.location.y);
            let mut out = Vec::new();
            match t.phase {
                TouchPhase::Started => {
                    st.fingers.retain(|f| *f != t.id);
                    st.fingers.push(t.id);
                    if st.fingers.len() == 1 {
                        st.two = false;
                        st.at = at;
                        st.left = true;
                        out.push(moved(at));
                        out.push(button(ElementState::Pressed, Button::Left));
                    } else if !st.two {
                        st.two = true;
                        if st.left {
                            st.left = false;
                            out.push(button(ElementState::Released, Button::Left));
                        }
                    }
                }
                TouchPhase::Moved => {
                    if st.fingers.first() == Some(&t.id) && !st.two {
                        st.at = at;
                        out.push(moved(at));
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    st.fingers.retain(|f| *f != t.id);
                    if st.left {
                        st.left = false;
                        out.push(moved(at));
                        out.push(button(ElementState::Released, Button::Left));
                    }
                    if st.fingers.is_empty() {
                        if st.two && t.phase == TouchPhase::Ended {
                            out.push(moved(st.at));
                            out.push(button(ElementState::Pressed, Button::Right));
                            out.push(button(ElementState::Released, Button::Right));
                        }
                        st.two = false;
                    }
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Escape pressed and released, as the window's key events (Atlantis
/// III's menu button).
pub(crate) fn escape_press() -> Vec<winit::event::WindowEvent> {
    use winit::event::{DeviceId, ElementState, WindowEvent};
    use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
    use winit::platform::android::key_event;
    [ElementState::Pressed, ElementState::Released]
        .into_iter()
        .map(|state| WindowEvent::KeyboardInput {
            device_id: DeviceId::dummy(),
            event: key_event(
                PhysicalKey::Code(KeyCode::Escape),
                Key::Named(NamedKey::Escape),
                None,
                state,
            ),
            is_synthetic: false,
        })
        .collect()
}

/// Atlantis II, III and Evolution's windows on Android: the Back button
/// (Escape) and the on-screen keyboard's text and keys, which reach the
/// app outside winit (`AtlantisActivity`), as the window's key events.
pub(crate) fn mode_typed() -> Vec<winit::event::WindowEvent> {
    use crate::android_saf::Typed;
    use winit::event::{DeviceId, ElementState, WindowEvent};
    use winit::keyboard::{Key, KeyCode, NamedKey, NativeKeyCode, PhysicalKey, SmolStr};
    use winit::platform::android::key_event;
    let mut out = Vec::new();
    let mut press = |physical: PhysicalKey, logical: Key, text: Option<SmolStr>| {
        for state in [ElementState::Pressed, ElementState::Released] {
            let text = (state == ElementState::Pressed)
                .then(|| text.clone())
                .flatten();
            out.push(WindowEvent::KeyboardInput {
                device_id: DeviceId::dummy(),
                event: key_event(physical, logical.clone(), text, state),
                is_synthetic: false,
            });
        }
    };
    for t in crate::android_saf::take_typed() {
        match t {
            Typed::Back => press(
                PhysicalKey::Code(KeyCode::Escape),
                Key::Named(NamedKey::Escape),
                None,
            ),
            Typed::Key(66) => press(
                PhysicalKey::Code(KeyCode::Enter),
                Key::Named(NamedKey::Enter),
                Some(SmolStr::new("\r")),
            ),
            Typed::Key(67) => press(
                PhysicalKey::Code(KeyCode::Backspace),
                Key::Named(NamedKey::Backspace),
                None,
            ),
            Typed::Text(text) => {
                for c in text.chars() {
                    let s = SmolStr::new(c.to_string());
                    press(
                        PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
                        Key::Character(s.clone()),
                        Some(s),
                    );
                }
            }
            Typed::Key(_) | Typed::Redraw => {}
        }
    }
    out
}

/// The entry point android-activity calls on its own thread, once the
/// activity is created.
#[no_mangle]
fn android_main(activity: AndroidApp) {
    // Once per process (an activity may follow another in it).
    static ONCE: std::sync::Once = std::sync::Once::new();
    let first = !ONCE.is_completed();
    ONCE.call_once(|| {});
    DESTROYED.store(false, Ordering::SeqCst);
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
    if first {
        // A handler that does not return is logged (its phase).
        crate::watchdog::start();
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("{info}");
            default_hook(info);
        }));
    }
    log::info!("{}", crate::version_string());
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
    GAME_DIR.lock().unwrap().clone_from(&game_dir);
    if let Some(dir) = &game_dir {
        // Made at once, so that it shows in a file manager or over USB.
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("{}: {e}", dir.display());
        }
        log::info!("game folder {}", dir.display());
    }
    crate::android_saf::init(activity.vm_as_ptr(), activity.activity_as_ptr());
    let _ = crate::android_saf::take_typed();
    let event_loop = match EventLoop::builder().with_android_app(activity).build() {
        Ok(el) => el,
        Err(e) => {
            log::error!("no event loop: {e}");
            return;
        }
    };
    let mut app = match CARRIED.lock().unwrap().take() {
        Some(Carried(app)) => {
            log::info!("the game goes on in the new activity");
            app
        }
        None => {
            let mut app = front_end();
            app.android_start(game_dir);
            app
        }
    };
    let mut event_loop = event_loop;
    crate::android_saf::set_waker(event_loop.create_proxy());
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    loop {
        // The first game's front end (also the game library's screen and
        // the "no game" screen), unless Atlantis II, III or Evolution was
        // chosen: their own modes, on the same event loop, after it (as
        // the desktop's `main`).
        if !app.session().is_some_and(|s| s.wants_own_mode()) && app.android_other.is_none() {
            if let Err(e) = event_loop.run_app_on_demand(&mut app) {
                log::error!("event loop: {e}");
            }
        }
        if destroyed() {
            // Destroyed by Android (not left by the player): kept for the
            // activity it may create next in this process.
            app.android_suspend();
            *CARRIED.lock().unwrap() = Some(Carried(app));
            log::info!("event loop ended (activity destroyed)");
            return;
        }
        let tree_game = app.android_other.take();
        let dirs = app.session_mut().map(|s| {
            (
                s.take_atlantis2(),
                s.take_atlantis3(),
                s.take_atlantis4(),
                s.take_atlantis5(),
            )
        });
        let next = match (tree_game, dirs) {
            (Some(game), _) => Next::Tree(game),
            (None, Some((a2, a3, a4, a5)))
                if a2.is_some() || a3.is_some() || a4.is_some() || a5.is_some() =>
            {
                Next::Dirs(a2, a3, a4, a5)
            }
            // The player left the app.
            _ => break,
        };
        drop(app);
        let ran = match next {
            Next::Tree(game) => run_tree_game(&mut event_loop, game),
            Next::Dirs(a2, a3, a4, a5) => run_other_game(&mut event_loop, a2, a3, a4, a5),
        };
        if destroyed() {
            // (the next activity opens the library's first game again)
            log::info!("event loop ended (activity destroyed)");
            return;
        }
        // The game ended: the game library's screen, with its error if
        // it could not start.
        app = front_end();
        let message = match ran {
            Ok(()) => None,
            Err(why) => {
                log::error!("the game could not start: {why}");
                Some(format!("The game could not start: {why}"))
            }
        };
        app.show_launcher(message);
    }
    drop(app);
    log::info!("event loop ended");
    // android-activity then finishes the activity and ends this thread.
}

/// The game played after the first game's front end.
enum Next {
    /// From a chosen folder.
    Tree(OtherGame),
    /// From the app's own folder: Atlantis II's, III's, Evolution's or
    /// The Secrets of Atlantis's.
    Dirs(
        Option<PathBuf>,
        Option<PathBuf>,
        Option<PathBuf>,
        Option<PathBuf>,
    ),
}

/// The first game's front end (and the game library's screen), without a
/// game.
fn front_end() -> App {
    let session = Session::new(data_dir(), false, false, false, Start::Boot);
    App::new(Sys::new(Mixer::new()), None).with_session(session)
}

/// The library entry the folder picker's answer is for (`None`: a game
/// to add).
static PICK_FOR: std::sync::Mutex<Option<usize>> = std::sync::Mutex::new(None);

/// Quit chosen in the game library: the app ends.
static QUIT: AtomicBool = AtomicBool::new(false);

/// Whether a library entry is a folder chosen in the picker (its tree
/// URI) rather than a path.
pub(crate) fn is_tree(path: &Path) -> bool {
    path.to_string_lossy().starts_with("content://")
}

/// A chosen folder as the library shows it: its storage and the folders
/// in it ("Phone/AtlantisNative/Atlantis3"), from its tree URI.
pub(crate) fn shown_tree(path: &Path) -> String {
    let uri = path.to_string_lossy();
    let id = uri.rsplit("/tree/").next().unwrap_or(&uri);
    let id = percent_decode(id);
    match id.split_once(':') {
        Some(("primary", rest)) => format!("Phone/{rest}"),
        Some((volume, rest)) => format!("{volume}/{rest}"),
        None => id,
    }
}

/// `%XX` escapes decoded (as UTF-8).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The library's entry for a chosen folder: the game and its edition, or
/// why it does not open.
pub(crate) fn describe(path: &Path) -> crate::launcher::LauncherEntry {
    use crate::setup::GameKind;
    let tree = path.to_string_lossy().to_string();
    let entry =
        |label: String, found: bool, kind: Option<GameKind>| crate::launcher::LauncherEntry {
            path: path.to_path_buf(),
            label,
            found,
            kind,
            copyable: false,
        };
    if !crate::android_saf::can_read(&tree) {
        return entry(
            "Not found: Android no longer lets the app read this folder".to_string(),
            false,
            None,
        );
    }
    match find_game_in_tree(&tree) {
        FoundInTree::Game(game) => entry(
            crate::desktop::edition_label(&game, false),
            true,
            Some(GameKind::LostTales),
        ),
        FoundInTree::Other(game) => entry(other_label(&game), true, Some(game.kind)),
        FoundInTree::Unsupported(why) => entry(format!("Not found: {why}"), false, None),
        FoundInTree::Nothing(_) => entry(
            "Not found: this folder no longer holds the game".to_string(),
            false,
            None,
        ),
    }
}

/// The edition of Atlantis II, III or Evolution in a chosen folder, as
/// the desktop's library names it.
fn other_label(game: &OtherGame) -> String {
    use crate::setup::GameKind;
    let named = match game.kind {
        GameKind::Atlantis2 => {
            atlantis2_data::files::find_exe(&*game.install)
                .ok()
                .map(|(name, build, _)| {
                    let mut files =
                        atlantis2_data::A2Files::new(game.install.clone(), game.discs.clone());
                    files.set_exe(name, build);
                    files.label()
                })
        }
        GameKind::Atlantis3 => atlantis3_data::A3Files::new(game.install.clone())
            .edition_label()
            .ok(),
        GameKind::Atlantis4 if game.evolution_2004 => evolution_2004_build(&*game.install)
            .and_then(Result::ok)
            .map(atlantis3_data::evolution2004::edition_label),
        GameKind::Atlantis4 => {
            let files = atlantis4_data::A4Files::new(game.install.clone());
            files
                .build()
                .ok()
                .map(|b| crate::a4::files_label(&files, b))
        }
        GameKind::Atlantis5 => game
            .atlantis5
            .as_ref()
            .map(|src| crate::a5::edition_of(src, false).0),
        GameKind::LostTales => None,
    };
    named.unwrap_or_else(|| game_name(game.kind).to_string())
}

/// The app's own game folder (`<external files>/game`).
static GAME_DIR: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Atlantis II, III or Evolution in its own mode (the folder found),
/// until it ends or the activity is destroyed (the next activity finds
/// the game again and starts it anew).
fn run_other_game(
    el: &mut EventLoop<()>,
    a2: Option<PathBuf>,
    a3: Option<PathBuf>,
    a4: Option<PathBuf>,
    a5: Option<PathBuf>,
) -> Result<(), String> {
    let run = if let Some(dir) = a5 {
        crate::a5::run_window_on(el, &dir, true, false)
    } else if let Some(dir) = a2 {
        let enhancements = crate::a2::stored_settings().enhancements;
        crate::a2::open(&dir, &[], &crate::a2::A2Start::default(), enhancements).and_then(
            |(engine, label)| crate::a2::run_window_on(el, engine, label, true, None, false),
        )
    } else if let Some(dir) = a4 {
        crate::a4::run_window_on(el, &dir, true, false, false)
    } else if let Some(dir) = a3 {
        crate::a3::open(&dir, &[], false)
            .and_then(|(game, _)| crate::a3::run_window_on(el, game, true, false))
    } else {
        return Ok(());
    };
    run.map(|_| ())
}

/// The file keeping the folder chosen in the picker (its tree URI).
fn chosen_folder_file() -> PathBuf {
    data_dir().join("android-game-folder.txt")
}

/// What a folder chosen in the picker holds.
enum FoundInTree {
    Game(Box<atlantis_data::Game>),
    /// Atlantis II, III or Evolution, played in its own mode after the
    /// first game's front end ([`android_main`]).
    Other(OtherGame),
    /// A game's program without what it needs to be played (why).
    Unsupported(String),
    Nothing(String),
}

/// Atlantis II, III or Evolution in a chosen folder: its install folder,
/// and the folders and CD images its discs may be in (the other folders
/// of the chosen one, those inside the install, and the install itself).
pub(crate) struct OtherGame {
    kind: crate::setup::GameKind,
    install: Arc<dyn atlantis_data::FileSource>,
    discs: Vec<(String, Arc<dyn atlantis_data::FileSource>)>,
    /// Atlantis Evolution's 2004 program (the CD's installation, played
    /// by Atlantis III's engine), not GOG.com's.
    evolution_2004: bool,
    /// The Secrets of Atlantis: its data opened (its disc's files, its
    /// image, or the unpacked tree).
    atlantis5: Option<atlantis3_data::atlantis5::Atlantis5Source>,
}

/// The Secrets of Atlantis in the folder `dir` of a chosen one (the
/// first form found, as [`atlantis3_data::atlantis5::Atlantis5Source::open`]
/// tries them): the disc's files (`DATAS.cab` and its languages'
/// cabinets, as the desktop's "Copy to this computer" leaves them), the
/// unpacked tree (an install's `Datas`, or `Datas` itself), or an image
/// of the DVD in it, read by parts. Its error when the folder holds the
/// game's files but they do not open.
fn atlantis5_in(
    dir: &Arc<crate::android_saf::SafDir>,
    names: &[String],
) -> Option<Result<atlantis3_data::atlantis5::Atlantis5Source, String>> {
    use atlantis3_data::atlantis5::{Atlantis5Source, Form, OpenError, DATA_CAB};
    use atlantis_data::FileSource;
    // (the checkpoints of the cabinets' folders kept between runs)
    atlantis_data::cab::set_index_dir(Some(crate::a5::cab_index_dir()));
    let src: Arc<dyn FileSource> = dir.clone();
    match Atlantis5Source::from_folder(src) {
        Ok(s) => return Some(Ok(s)),
        // (another game's tree is not this one's, as on the desktop)
        Err(OpenError::NotTheGame) => {}
        Err(_) if dir.size(DATA_CAB).is_none() => {}
        Err(e) => return Some(Err(e.to_string())),
    }
    let image = names.iter().any(|n| {
        n.rsplit_once('.')
            .is_some_and(|(_, ext)| atlantis_data::disc_image::IMAGE_EXTENSIONS.contains(&ext))
    });
    if !image {
        return None;
    }
    let set = dir.disc_set()?;
    // (another game's image)
    set.size(DATA_CAB)?;
    Some(Atlantis5Source::from_disc(Arc::new(set), Form::DiscImage).map_err(|e| e.to_string()))
}

/// The 2004 program's installation in `src` (its program beside
/// `cd0\datas\scenar`, as [`crate::a3::holds_evolution_2004`]): which
/// known build its program is.
fn evolution_2004_build(
    src: &dyn atlantis_data::FileSource,
) -> Option<
    Result<&'static atlantis3_data::evolution2004::Build, atlantis3_data::evolution2004::ExeError>,
> {
    if !src.has_dir("cd0/datas/scenar") {
        return None;
    }
    atlantis3_data::evolution2004::identify_install(src)
}

/// The game in the chosen folder `tree` or in a folder directly inside it
/// (the first that opens, in name order): its files, or the images of its
/// CDs.
fn find_game_in_tree(tree: &str) -> FoundInTree {
    use crate::setup::GameKind;
    use atlantis_data::FileSource;
    let Some(top) = crate::android_saf::SafDir::top(tree) else {
        return FoundInTree::Nothing("the folder cannot be read".to_string());
    };
    let subs = top.subfolders();
    let candidates: Vec<Arc<crate::android_saf::SafDir>> =
        std::iter::once(top).chain(subs).map(Arc::new).collect();
    let mut first_error = None;
    let mut other: Option<(GameKind, usize, bool)> = None;
    let mut unsupported = None;
    let mut atlantis5 = None;
    for (i, c) in candidates.iter().enumerate() {
        // The other games, by their program files, then checked.
        if other.is_none() {
            let names: Vec<String> = c.list("").iter().map(|n| n.to_lowercase()).collect();
            let has = |n: &str| names.iter().any(|x| x == n);
            let src: Arc<dyn FileSource> = c.clone();
            if (has("atlantis2.exe") || (has("game.exe") && !has("datas")))
                && atlantis2_data::files::find_exe(&*src).is_ok()
            {
                other = Some((GameKind::Atlantis2, i, false));
            } else if has("datas")
                && (has("game.exe") || has("atlantis3.exe"))
                && atlantis3_data::A3Files::new(src.clone()).build().is_ok()
            {
                other = Some((GameKind::Atlantis3, i, false));
            } else if has("atlantis evolution.exe") {
                if atlantis4_data::A4Files::new(src.clone()).build().is_ok() {
                    other = Some((GameKind::Atlantis4, i, false));
                } else {
                    match evolution_2004_build(&*src) {
                        Some(Ok(_)) => other = Some((GameKind::Atlantis4, i, true)),
                        Some(Err(e)) => {
                            unsupported.get_or_insert(format!("Atlantis Evolution (2004): {e}"));
                        }
                        None => {
                            unsupported.get_or_insert(
                                "Atlantis Evolution's program is here, but not its data"
                                    .to_string(),
                            );
                        }
                    }
                }
            } else {
                // The Secrets of Atlantis, before the first game: its
                // DVD's image would be taken for one of the first game's.
                match atlantis5_in(c, &names) {
                    Some(Ok(src)) => {
                        atlantis5 = Some(src);
                        other = Some((GameKind::Atlantis5, i, false));
                        continue;
                    }
                    Some(Err(e)) => {
                        unsupported.get_or_insert(format!("The Secrets of Atlantis: {e}"));
                    }
                    None => {}
                }
            }
        }
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
    if let Some((kind, i, evolution_2004)) = other {
        log::info!("chosen folder: {}", game_name(kind));
        let install = candidates[i].clone();
        let mut dirs: Vec<Arc<crate::android_saf::SafDir>> = candidates
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, c)| c.clone())
            .collect();
        dirs.extend(install.subfolders().into_iter().map(Arc::new));
        dirs.push(install.clone());
        let mut discs: Vec<(String, Arc<dyn FileSource>)> = Vec::new();
        for (n, d) in dirs.into_iter().enumerate() {
            if let Some(set) = d.disc_set() {
                discs.push((format!("images in folder {n}"), Arc::new(set)));
            }
            discs.push((format!("folder {n}"), d));
        }
        return FoundInTree::Other(OtherGame {
            kind,
            install,
            discs,
            evolution_2004,
            atlantis5,
        });
    }
    match unsupported {
        Some(name) => FoundInTree::Unsupported(name),
        None => FoundInTree::Nothing(first_error.unwrap_or_default()),
    }
}

/// Atlantis II, III or Evolution from a chosen folder on `el`, until it
/// ends or the activity is destroyed (the next activity opens the folder
/// again and starts it anew).
fn run_tree_game(el: &mut EventLoop<()>, game: OtherGame) -> Result<(), String> {
    use crate::setup::GameKind;
    let shown = Path::new("The chosen folder");
    match game.kind {
        GameKind::Atlantis2 => {
            let (engine, label) = crate::a2::boot_source(game.install, game.discs)?;
            crate::a2::run_window_on(el, engine, label, true, None, false).map(|_| ())
        }
        GameKind::Atlantis3 => {
            let (g, _) =
                crate::a3::open_source_in(game.install, game.discs, shown, &crate::a3::data_dir())?;
            crate::a3::run_window_on(el, g, true, false).map(|_| ())
        }
        GameKind::Atlantis4 if game.evolution_2004 => {
            let g = crate::a3::open_evolution_2004_in(
                game.install,
                shown,
                &crate::a3::evolution_2004_data_dir(),
            )?;
            crate::a3::run_evolution_2004_game_on(el, g, true, false).map(|_| ())
        }
        GameKind::Atlantis4 => {
            let files = atlantis4_data::A4Files::new(game.install);
            let build = files
                .build()
                .map_err(|e| crate::setup::explain_atlantis4(shown, &e))?;
            crate::a4::run_files_on(el, files, build)
        }
        GameKind::Atlantis5 => {
            let src = game
                .atlantis5
                .ok_or_else(|| "The Secrets of Atlantis: no data".to_string())?;
            let label = crate::a5::edition_of(&src, false).0;
            crate::a5::run_source_on(el, src, &label, true, false).map(|_| ())
        }
        GameKind::LostTales => Ok(()),
    }
}

/// The tap's line at the foot of the "no game" screen.
const TAP_TO_CHOOSE: &str = "Tap the screen to choose the game's folder.";

impl App {
    /// The game library at start-up (as on the desktop,
    /// [`crate::session::startup`]): its one game at once, its screen
    /// when it holds two or more, else the screen saying where to put a
    /// game (a tap opens the picker). The folder an earlier version kept
    /// and the game in the app's own folder `game_dir` are added to it.
    fn android_start(&mut self, game_dir: Option<PathBuf>) {
        let mut own_found = None;
        if let Some(session) = self.session_mut() {
            if let Ok(tree) = std::fs::read_to_string(chosen_folder_file()) {
                let tree = tree.trim();
                if !tree.is_empty() && session.library_mut().add(Path::new(tree)) {
                    log::info!("the folder chosen before, into the library: {tree}");
                }
                session.save_library();
                let _ = std::fs::remove_file(chosen_folder_file());
            }
        }
        if let Some(dir) = &game_dir {
            match find_game(dir) {
                Found::Game(_, found) => {
                    if let Some(session) = self.session_mut() {
                        if session.library_mut().add(&found) {
                            log::info!("the app's folder's game, into the library");
                            session.save_library();
                        }
                    }
                }
                Found::Broken(name, found, why) => {
                    log::info!(
                        "{name} found in {} but it does not open: {why}",
                        found.display()
                    );
                    own_found = Some((name, found, why));
                }
                Found::Nothing => {}
            }
        }
        let folders = self
            .session()
            .map(|s| s.library().folders().to_vec())
            .unwrap_or_default();
        let first_opens = || {
            folders.first().is_some_and(|f| {
                if is_tree(f) {
                    describe(f).found
                } else {
                    open_any(f).is_ok()
                }
            })
        };
        match crate::session::startup(folders.len(), false, first_opens) {
            crate::session::Startup::PlayFirst => {
                self.run_launcher_action(crate::launcher::LauncherAction::Play(0));
            }
            crate::session::Startup::Launcher => self.show_launcher(None),
            crate::session::Startup::FirstStart => match own_found {
                Some((name, dir, why)) => self.show_notice(&[
                    format!("{name} found in"),
                    dir.display().to_string(),
                    "but it does not open:".to_string(),
                    why,
                ]),
                None => {
                    let dir = game_dir.map_or_else(String::new, |d| d.display().to_string());
                    log::info!("No game found: copy it to {dir}");
                    self.show_notice(&[
                        "No game found.".to_string(),
                        String::new(),
                        "Choose the folder of one of the Atlantis games (its files, or the images of its discs),".to_string(),
                        "or copy the game to".to_string(),
                        dir,
                        "and start the app again.".to_string(),
                    ]);
                }
            },
        }
    }

    /// Plays the game in the chosen folder `path` (a tree URI): the first
    /// game at once, Atlantis II, III or Evolution once this front end's
    /// event loop ends; it moves to the top of the library. Why not.
    fn play_tree(&mut self, path: &Path) -> Result<(), String> {
        let tree = path.to_string_lossy().to_string();
        if !crate::android_saf::can_read(&tree) {
            return Err(
                "Android no longer lets the app read this folder: choose it again".to_string(),
            );
        }
        match find_game_in_tree(&tree) {
            FoundInTree::Game(game) => {
                self.touch.picker = false;
                self.play_game(*game, Some(path));
                Ok(())
            }
            FoundInTree::Other(game) => {
                log::info!("chosen folder {tree}: {}", game_name(game.kind));
                self.stop_game();
                self.touch.picker = false;
                if let Some(session) = self.session_mut() {
                    session.hide_launcher();
                    session.mark_played(path);
                }
                // (played in its own mode once this front end's event
                // loop ends, android_main)
                self.android_other = Some(game);
                Ok(())
            }
            FoundInTree::Unsupported(why) => Err(why),
            FoundInTree::Nothing(why) => Err(if why.is_empty() {
                "No game found in this folder".to_string()
            } else {
                format!("No game found in this folder: {why}")
            }),
        }
    }

    /// The game library's actions that differ on Android (the picker, the
    /// chosen folders, the app's own folder); false for the desktop's.
    pub(crate) fn android_launcher_action(
        &mut self,
        action: crate::launcher::LauncherAction,
    ) -> bool {
        use crate::launcher::LauncherAction as A;
        match action {
            A::Play(i) => {
                let Some(path) = self
                    .session()
                    .and_then(|s| s.library().folders().get(i).cloned())
                else {
                    return true;
                };
                if !is_tree(&path) {
                    return false;
                }
                if let Err(why) = self.play_tree(&path) {
                    log::warn!("{}: {why}", path.display());
                    if let Some(s) = self.session_mut() {
                        s.show_entries(i, why);
                    }
                }
                true
            }
            A::Add | A::Locate(_) => {
                *PICK_FOR.lock().unwrap() = match action {
                    A::Locate(i) => Some(i),
                    _ => None,
                };
                if !crate::android_saf::pick_folder() {
                    if let Some(s) = self.session_mut() {
                        s.set_message("The folder picker could not open");
                    }
                }
                true
            }
            A::Find => {
                // The app's own folder.
                let dir = GAME_DIR.lock().unwrap().clone();
                let text = match dir.as_deref().map(find_game) {
                    Some(Found::Game(opened, found)) => {
                        let label = opened.label();
                        let Some(s) = self.session_mut() else {
                            return true;
                        };
                        if s.library_mut().add(&found) {
                            s.save_library();
                            let at = s.library().position(&found).unwrap_or(0);
                            s.show_entries(at, format!("Added: {label}"));
                            return true;
                        }
                        "The game in the app's folder is in the list already".to_string()
                    }
                    _ => "No game in the app's own folder: use Add a game folder".to_string(),
                };
                if let Some(s) = self.session_mut() {
                    s.set_message(text);
                }
                true
            }
            A::Support => {
                if let Some(s) = self.session_mut() {
                    s.set_message(crate::guide::SUPPORT_URL);
                }
                true
            }
            A::Quit => {
                QUIT.store(true, Ordering::SeqCst);
                true
            }
            A::Remove(_) | A::Copy(_) | A::CancelCopy | A::None => false,
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
    pub(crate) fn poll_picked(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if let Some(tree) = crate::android_saf::take_picked() {
            let locate = PICK_FOR.lock().unwrap().take();
            if !tree.is_empty() && self.runner.is_none() {
                self.picked(&tree, locate);
            }
        }
        // Atlantis II, III or Evolution chosen: its own mode, after this
        // event loop; Quit in the library: the app ends.
        if self.android_other.is_some() || QUIT.swap(false, Ordering::SeqCst) {
            el.exit();
        }
    }

    /// The folder `tree` chosen in the picker: on the library's screen,
    /// added to it (or the place of entry `locate`, not found any more);
    /// else (the "no game" screen) played, and so added.
    fn picked(&mut self, tree: &str, locate: Option<usize>) {
        let path = PathBuf::from(tree);
        if !self.session().is_some_and(|s| s.launcher().is_some()) {
            if let Err(why) = self.play_tree(&path) {
                self.show_notice(&[why]);
            }
            return;
        }
        let entry = describe(&path);
        let Some(s) = self.session_mut() else {
            return;
        };
        if !entry.found {
            s.set_message(entry.label);
            return;
        }
        match locate {
            Some(i) => {
                let at = s.library_mut().relocate(i, &path);
                s.save_library();
                s.show_entries(at, format!("Found again: {}", entry.label));
            }
            None => {
                let added = s.library_mut().add(&path);
                s.save_library();
                let at = s.library().position(&path).unwrap_or(0);
                s.show_entries(
                    at,
                    if added {
                        format!("Added: {}", entry.label)
                    } else {
                        "This game is in the list already".to_string()
                    },
                );
            }
        }
    }
}

/// What the game folder holds.
enum Found {
    /// A game that opens (any of the four).
    Game(Opened, PathBuf),
    /// A game's files that do not open: its name, folder, and why.
    Broken(&'static str, PathBuf, String),
    Nothing,
}

/// A game's name.
fn game_name(kind: crate::setup::GameKind) -> &'static str {
    use crate::setup::GameKind;
    match kind {
        GameKind::LostTales => "Atlantis: The Lost Tales",
        GameKind::Atlantis2 => "Atlantis II",
        GameKind::Atlantis3 => "Atlantis III",
        GameKind::Atlantis4 => "Atlantis Evolution",
        GameKind::Atlantis5 => "The Secrets of Atlantis",
    }
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
    let mut broken = None;
    for c in candidates {
        match open_any(&c) {
            Ok(opened) => {
                log::info!("{}: {}", c.display(), opened.label());
                return Found::Game(opened, c);
            }
            Err(e) => {
                log::info!("{}: {e}", c.display());
                if let Some(kind) = crate::setup::kind_of(&c) {
                    broken.get_or_insert((game_name(kind), c, e));
                }
            }
        }
    }
    match broken {
        Some((name, c, e)) => Found::Broken(name, c, e),
        None => Found::Nothing,
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
    queue: VecDeque<Option<Input>>,
    /// Where the finger went down (the game's screen): a tap clicks
    /// there, whatever small motion the finger made (see [`TAP_SLOP`]).
    down_at: (i32, i32),
    /// The finger that went down on a button, and which.
    button: Option<(u64, Button)>,
    /// The game library's screen: where the finger went down, and the
    /// height the list was last scrolled at (window pixels).
    list_from: f64,
    list_at: f64,
}

impl TouchState {
    /// The on-screen keyboard is up for the player screen's name entry.
    pub(crate) fn typing(&self) -> bool {
        self.keyboard
    }
}

/// A rectangle in window pixels: x, y, width, height.
pub(crate) type Rect = (usize, usize, usize, usize);

/// The on-screen buttons in the black bars beside the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    /// Right of the picture: the bag (a right click).
    Bag,
    /// Left of the picture: the port's options menu.
    Menu,
}

/// A bar beside the picture narrower than this (window pixels): no
/// button (a 4:3 screen; the fingers' gestures still do the same).
const MIN_BAR: usize = 48;

/// Draws `button` in its black bar (window columns `bar_x` to
/// `bar_x + bar`, `fit_h` rows high) when `shown`; returns where a tap
/// presses it (the bar's lower half).
pub(crate) fn draw_button(
    buf: &mut [u32],
    stride: usize,
    (right, bar): (usize, usize),
    fit_h: usize,
    shown: bool,
    button: Button,
) -> Option<Rect> {
    if !shown || bar < MIN_BAR || fit_h < 4 || right + bar > stride {
        return None;
    }
    let size = ((bar as f64 * 0.62).min(fit_h as f64 * 0.16)) as usize;
    if size < 16 {
        return None;
    }
    let x0 = right + (bar - size) / 2;
    let margin = (bar - size) / 2;
    let y0 = fit_h.saturating_sub(size + margin.max(size / 3));
    let s = size as f64;
    // The shapes in units of the icon's side. The bag: the body a
    // rounded rectangle, the handle half a ring above it, a clasp in the
    // middle. The menu: three rounded lines.
    const OUTLINE: (f64, f64, f64) = (200.0, 180.0, 138.0);
    const FILL: (f64, f64, f64) = (70.0, 52.0, 34.0);
    let line = 0.06;
    for py in 0..size {
        let row = (y0 + py) * stride;
        let v = (py as f64 + 0.5) / s;
        for px in 0..size {
            let u = (px as f64 + 0.5) / s;
            let aa = |d: f64| (0.5 - d * s).clamp(0.0, 1.0);
            let (fill_a, out_a) = match button {
                Button::Bag => {
                    // The body's signed distance (negative inside).
                    let (cx, cy, hw, hh, r) = (0.5, 0.64, 0.36, 0.27, 0.12);
                    let qx = (u - cx).abs() - (hw - r);
                    let qy = (v - cy).abs() - (hh - r);
                    let body = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                    // The handle: half a ring above the body.
                    let ring = if v <= 0.40 {
                        ((u - 0.5).hypot(v - 0.40) - 0.19).abs() - line / 2.0
                    } else {
                        f64::INFINITY
                    };
                    // The clasp: a small square on the body's top edge.
                    let clasp = ((u - 0.5).abs() - 0.07).max((v - 0.48).abs() - 0.07);
                    (
                        aa(body),
                        aa(body.abs() - line / 2.0).max(aa(ring)).max(aa(clasp)),
                    )
                }
                Button::Menu => {
                    let d = [0.3, 0.5, 0.7]
                        .iter()
                        .map(|&cy| {
                            let dx = ((u - 0.5).abs() - 0.28).max(0.0);
                            dx.hypot(v - cy) - 0.045
                        })
                        .fold(f64::INFINITY, f64::min);
                    (0.0, aa(d))
                }
            };
            if fill_a <= 0.0 && out_a <= 0.0 {
                continue;
            }
            let p = &mut buf[row + x0 + px];
            let bg = (
                (*p >> 16 & 255) as f64,
                (*p >> 8 & 255) as f64,
                (*p & 255) as f64,
            );
            let mix = |a: f64, b: f64, t: f64| a + (b - a) * t;
            let mut c = (
                mix(bg.0, FILL.0, fill_a),
                mix(bg.1, FILL.1, fill_a),
                mix(bg.2, FILL.2, fill_a),
            );
            c = (
                mix(c.0, OUTLINE.0, out_a),
                mix(c.1, OUTLINE.1, out_a),
                mix(c.2, OUTLINE.2, out_a),
            );
            *p = (c.0 as u32) << 16 | (c.1 as u32) << 8 | c.2 as u32;
        }
    }
    // A tap anywhere in the bar's lower half presses it.
    let top = fit_h / 2;
    Some((right, top, bar, fit_h - top))
}

/// Shows a `gw` x `gh` picture through `frame` in a window of `window`
/// pixels (`row` as in [`crate::android_frame::Frame::present`]), with
/// the bag and menu buttons in its black bars when `shown`. Returns the
/// picture's rectangle and where a tap presses the bag (0) and the menu
/// (1), in window pixels; None when the window took no frame.
pub(crate) fn present_with_buttons(
    frame: &mut crate::android_frame::Frame,
    window: (usize, usize),
    (gw, gh): (usize, usize),
    row: impl FnMut(usize, &mut [u32]),
    shown: bool,
) -> Option<(Rect, [Option<Rect>; 2])> {
    let mut pressed: [Option<Rect>; 2] = [None; 2];
    let bars = |b: crate::android_frame::Bars<'_>| {
        let (lw, lh) = b.size;
        let (ox, _, pw, _) = b.picture;
        let right = ox + pw;
        for (i, (bar, button)) in [((right, lw - right), Button::Bag), ((0, ox), Button::Menu)]
            .into_iter()
            .enumerate()
        {
            let r = draw_button(b.px, b.stride, bar, lh, shown, button);
            // (drawn as 0x00RRGGBB: put in the buffer's byte order)
            if let Some((x, y, w, h)) = r {
                for line in b.px.chunks_exact_mut(b.stride).skip(y).take(h) {
                    for p in &mut line[x..x + w] {
                        *p = crate::android_frame::from_xrgb(*p);
                    }
                }
            }
            pressed[i] = r;
        }
    };
    let view = frame.present(window, (gw, gh), row, bars)?;
    // The buttons' rectangles in window pixels: the buffer's pixel is the
    // picture's (gw across `view.2` window pixels), its picture at the
    // view's place.
    let (vx, vy, vw, vh) = view;
    let (bw, bh) = frame.buffer_size();
    let (box_, boy) = (bw.saturating_sub(gw) / 2, bh.saturating_sub(gh) / 2);
    let to_window = |r: Option<Rect>| {
        r.map(|(x, y, w, h)| {
            let sx =
                |v: usize| (vx as f64 + (v as f64 - box_ as f64) * vw as f64 / gw as f64).max(0.0);
            let sy =
                |v: usize| (vy as f64 + (v as f64 - boy as f64) * vh as f64 / gh as f64).max(0.0);
            let (x0, y0, x1, y1) = (sx(x), sy(y), sx(x + w), sy(y + h));
            (
                x0 as usize,
                y0 as usize,
                (x1 - x0) as usize,
                (y1 - y0) as usize,
            )
        })
    };
    Some((view, [to_window(pressed[0]), to_window(pressed[1])]))
}

/// Whether window point (x, y) is in `r`.
fn inside(r: Option<Rect>, x: f64, y: f64) -> bool {
    r.is_some_and(|(rx, ry, rw, rh)| {
        x >= rx as f64 && y >= ry as f64 && x < (rx + rw) as f64 && y < (ry + rh) as f64
    })
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
        // The player screen's name entry: a tap shows the on-screen
        // keyboard again (closed by the player).
        if t.phase == TouchPhase::Ended && self.touch.keyboard {
            crate::android_saf::show_keyboard(true);
        }
        // The game library's screen: a tap is a click (on the selected
        // row: Play), a drag scrolls the list.
        if self.session().is_some_and(|s| s.launcher().is_some()) {
            self.library_touch(&t, (x, y), (sx, sy));
            return;
        }
        // The "no game" screen: a tap opens the folder picker.
        if self.runner.is_none() && self.touch.picker {
            if t.phase == TouchPhase::Ended && crate::android_saf::pick_folder() {
                log::info!("folder picker opened");
            }
            return;
        }
        // The buttons beside the picture: the bag (a right click) or the
        // options menu when the finger lifts on the button it went down
        // on, the only finger down.
        if self.runner.is_some() {
            if let Some((id, button)) = self.touch.button {
                if id == t.id {
                    if t.phase == TouchPhase::Ended && inside(self.button_rect(button), x, y) {
                        self.press_button(button);
                    }
                    if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                        self.touch.button = None;
                    }
                }
                // (other fingers while a button is held: ignored)
                return;
            }
            if t.phase == TouchPhase::Started && self.touch.fingers.is_empty() {
                for button in [Button::Bag, Button::Menu] {
                    if inside(self.button_rect(button), x, y) {
                        self.touch.button = Some((t.id, button));
                        return;
                    }
                }
            }
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
                    for i in queue.into_iter().flatten() {
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
                        // The pointer there first, a step without input,
                        // then the press: the game's menus hit-test the
                        // pointer of the pass before the press (the
                        // subject menu, FUN_004211f0), and highlight on
                        // the move.
                        st.down_at = (sx, sy);
                        st.queue.push_back(Some(Input::MouseMove { x: sx, y: sy }));
                        st.queue.push_back(None);
                        st.queue.push_back(Some(Input::MouseDown {
                            button: MouseButton::Left,
                            x: sx,
                            y: sy,
                        }));
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
                    // A finger moving a little (less than TAP_SLOP from
                    // where it went down) is still a tap there.
                    if !st.dragged && (x - from.0).hypot(y - from.1) <= TAP_SLOP {
                        return;
                    }
                    st.dragged = true;
                    let m = Input::MouseMove { x: sx, y: sy };
                    if st.queue.is_empty() {
                        self.sys.push_input(m);
                    } else {
                        st.queue.push_back(Some(m));
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
                        let (ux, uy) = if st.dragged { (sx, sy) } else { st.down_at };
                        st.queue.push_back(Some(Input::MouseUp {
                            button: MouseButton::Left,
                            x: ux,
                            y: uy,
                        }));
                        st.left_down = false;
                    } else if self.locked && !st.dragged {
                        // The panoramic view: a tap clicks where the view
                        // points.
                        let (mx, my) = self.sys.mouse();
                        let button = MouseButton::Left;
                        st.queue.push_back(Some(Input::MouseDown {
                            button,
                            x: mx,
                            y: my,
                        }));
                        st.queue.push_back(Some(Input::MouseUp {
                            button,
                            x: mx,
                            y: my,
                        }));
                    }
                }
                if st.fingers.is_empty() {
                    if st.two && t.phase == TouchPhase::Ended {
                        let (mx, my) = self.sys.mouse();
                        let button = MouseButton::Right;
                        st.queue.push_back(Some(Input::MouseDown {
                            button,
                            x: mx,
                            y: my,
                        }));
                        st.queue.push_back(Some(Input::MouseUp {
                            button,
                            x: mx,
                            y: my,
                        }));
                    }
                    st.primary = None;
                    st.two = false;
                    st.dragged = false;
                }
            }
        }
        self.last_cursor = Some((x, y));
    }

    /// A finger on the game library's screen (window point `at`, screen
    /// point `screen`).
    fn library_touch(&mut self, t: &Touch, at: (f64, f64), screen: (i32, i32)) {
        let row_px = f64::from(crate::launcher::Launcher::row_height()) * self.viewport.scale;
        let ms = self.start.elapsed().as_millis() as u64;
        let st = &mut self.touch;
        let mut action = crate::launcher::LauncherAction::None;
        match t.phase {
            TouchPhase::Started => {
                if st.primary.is_none() {
                    st.primary = Some(t.id);
                    st.dragged = false;
                    st.list_from = at.1;
                    st.list_at = at.1;
                }
            }
            TouchPhase::Moved if st.primary == Some(t.id) => {
                if (at.1 - st.list_from).abs() > TAP_SLOP {
                    st.dragged = true;
                }
                if st.dragged && row_px > 0.0 {
                    // The list follows the finger: up shows the rows below.
                    let rows = ((st.list_at - at.1) / row_px).trunc();
                    if rows != 0.0 {
                        st.list_at -= rows * row_px;
                        if let Some(l) = self.session.as_mut().and_then(|s| s.launcher_mut()) {
                            l.wheel(rows as i32);
                        }
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled if st.primary == Some(t.id) => {
                st.primary = None;
                if !st.dragged && t.phase == TouchPhase::Ended {
                    if let Some(l) = self.session.as_mut().and_then(|s| s.launcher_mut()) {
                        l.pointer(screen.0, screen.1);
                        action = l.tap(screen.0, screen.1, ms);
                    }
                }
            }
            _ => {}
        }
        if action != crate::launcher::LauncherAction::None {
            self.run_launcher_action(action);
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Where a tap presses `button` (window pixels), as last drawn.
    fn button_rect(&self, button: Button) -> Option<Rect> {
        match button {
            Button::Bag => self.bag_button,
            Button::Menu => self.menu_button,
        }
    }

    /// `button` tapped: the bag (a right click where the pointer is) or
    /// the options menu.
    fn press_button(&mut self, button: Button) {
        log::info!("{button:?} button");
        match button {
            Button::Bag => {
                let (mx, my) = self.sys.mouse();
                let button = MouseButton::Right;
                let q = &mut self.touch.queue;
                q.push_back(Some(Input::MouseDown {
                    button,
                    x: mx,
                    y: my,
                }));
                q.push_back(Some(Input::MouseUp {
                    button,
                    x: mx,
                    y: my,
                }));
            }
            Button::Menu => {
                let queue = std::mem::take(&mut self.touch.queue);
                for i in queue.into_iter().flatten() {
                    self.sys.push_input(i);
                }
                self.touch = TouchState {
                    picker: self.touch.picker,
                    ..TouchState::default()
                };
                self.open_menu();
            }
        }
    }

    /// The on-screen keyboard's text and keys, and the Back button: the
    /// engine's keys (one input per step, after the touches queued), or
    /// the options menu's; Back without a game leaves the app.
    pub(crate) fn poll_typed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        use crate::android_saf::Typed;
        use atlantis_engine::sys::Key as EngineKey;
        for t in crate::android_saf::take_typed() {
            let keys: Vec<EngineKey> = match t {
                Typed::Text(text) => text.chars().map(EngineKey::Char).collect(),
                Typed::Key(66) => vec![EngineKey::Enter],
                Typed::Key(67) => vec![EngineKey::Backspace],
                Typed::Key(_) | Typed::Redraw => Vec::new(),
                Typed::Back => {
                    if self.runner.is_none() && !self.menu.is_open() {
                        log::info!("Back on the \"no game\" screen: the app ends");
                        el.exit();
                        return;
                    }
                    vec![EngineKey::Escape]
                }
            };
            for k in keys {
                if self.menu.is_open() {
                    let action = self.menu.key(k, self.menu_values());
                    self.menu_action(action);
                } else if self.runner.is_some() {
                    self.touch.queue.push_back(Some(Input::KeyDown(k)));
                    self.touch.queue.push_back(Some(Input::KeyUp(k)));
                }
            }
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Gives the engine the next queued touch input (before each step),
    /// and shows the on-screen keyboard while the player screen takes
    /// the letters (hidden after).
    pub(crate) fn feed_touch(&mut self) {
        if let Some(Some(i)) = self.touch.queue.pop_front() {
            self.sys.push_input(i);
        }
        let entry = self
            .sys
            .takes_key_repeat(atlantis_engine::sys::Key::Char('a'));
        if entry != self.touch.keyboard {
            self.touch.keyboard = entry;
            log::info!(
                "on-screen keyboard {}",
                if entry { "shown" } else { "hidden" }
            );
            crate::android_saf::show_keyboard(entry);
        }
    }

    /// The activity went to the background (its window is gone): the
    /// window, its surface and the sound output are dropped, the game's
    /// clock stops.
    pub(crate) fn android_suspend(&mut self) {
        log::info!("suspended");
        self.surface = None;
        self.native = None;
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
            Some(Input::MouseDown {
                button: MouseButton::Left,
                ..
            })
        )
    });
    if st.queue.len() == before {
        st.queue.push_back(Some(Input::MouseUp {
            button: MouseButton::Left,
            x: at.0,
            y: at.1,
        }));
    } else {
        // (its moves go with it)
        st.queue
            .retain(|i| !matches!(i, None | Some(Input::MouseMove { .. })));
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
