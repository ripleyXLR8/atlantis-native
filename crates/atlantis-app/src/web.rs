//! Web entry point. The page passes the game folder the player picked
//! (`<input type="file" webkitdirectory>`); the browser only reads files
//! asynchronously and a BIG archive is 600 MB, so the files are read by
//! ranges: the executable whole and the first MiB of each archive (its
//! directory is at most 205 KB) at the start, then each member the engine
//! reads, fetched just before (the engine's [`Loader`]).
//!
//! The Chinese releases' subtitle fonts are not in the program: the page
//! fetches the one the release needs from `fonts/` beside it
//! (`web/build.sh` copies them there) when the folder does not hold the
//! player's own font; the others never load.
//!
//! The players, checkpoints and settings are kept in the browser's
//! `localStorage` ([`WebStore`]), one key per file. The original's
//! `players.atl` and `p<letter><chapter>.gam` of the picked folder, if
//! any, are read at the start, so that the engine takes them over as the
//! desktop build does.
//!
//! Atlantis II ([`crate::web_layout`] finds it among the picked files):
//! its engine reads its files when it needs them, as the original does,
//! and waits for them; the browser reads a picked file only
//! asynchronously on the page's thread, except through a synchronous
//! request of a part of it (`XMLHttpRequest` of a `blob:` URL of the
//! slice, the bytes as `x-user-defined` text), which [`PickedDir`] makes:
//! each file is read when the engine asks for it, the part it asks for,
//! never the whole of the 2.4 GB. Its saves, `atlant2.cfg` and
//! `settings.ini` are kept in `localStorage` under `atlantis-native/atlantis2/`,
//! one key per file as the first game's.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use atlantis_data::dvd::DvdLanguage;
use atlantis_data::{BigArchive, FileSource, Game};
use atlantis_engine::hooks::{HookFuture, Loader};
use atlantis_engine::{SaveStore, Start};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileList, Response, Storage};
use winit::event_loop::EventLoop;
use winit::platform::web::EventLoopExtWebSys;
use winit::window::Window;

use crate::{options_with_settings, start_game, App};

const ARCHIVE_HEAD: u64 = 1 << 20;

/// Fetched ranges: path -> (offset, bytes).
type Ranges = HashMap<String, Vec<(u64, Vec<u8>)>>;

/// Files of the picked folder; reads are served from ranges fetched before.
struct WebFiles {
    // lower-case path below the picked folder -> size
    sizes: HashMap<String, u64>,
    /// The executable and the archive directories, kept.
    heads: Mutex<Ranges>,
    /// The members fetched for the engine's current reads.
    members: Mutex<Ranges>,
}

fn key(name: &str) -> String {
    name.replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

fn find(ranges: &Ranges, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
    ranges.get(&key(name))?.iter().find_map(|(start, bytes)| {
        let from = usize::try_from(offset.checked_sub(*start)?).ok()?;
        bytes.get(from..from.checked_add(len)?).map(<[u8]>::to_vec)
    })
}

impl FileSource for WebFiles {
    fn size(&self, name: &str) -> Option<u64> {
        self.sizes.get(&key(name)).copied()
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        find(&self.heads.lock().unwrap(), name, offset, len)
            .or_else(|| find(&self.members.lock().unwrap(), name, offset, len))
    }

    fn has_dir(&self, name: &str) -> bool {
        let prefix = key(name) + "/";
        self.sizes.keys().any(|k| k.starts_with(&prefix))
    }

    /// The files of folder `dir` (lower case).
    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = match key(dir) {
            d if d.is_empty() => d,
            d => d + "/",
        };
        self.sizes
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|n| !n.contains('/'))
            .map(str::to_string)
            .collect()
    }
}

async fn fetch(file: &File, offset: u64, len: u64) -> Result<Vec<u8>, JsValue> {
    let blob = file.slice_with_f64_and_f64(offset as f64, (offset + len) as f64)?;
    let buf = JsFuture::from(blob.array_buffer()).await?;
    Ok(Uint8Array::new(&buf).to_vec())
}

/// The file at `url`, relative to the page.
async fn fetch_url(url: &str) -> Result<Vec<u8>, JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let resp: Response = JsFuture::from(window.fetch_with_str(url))
        .await?
        .dyn_into()?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()).into());
    }
    let buf = JsFuture::from(resp.array_buffer()?).await?;
    Ok(Uint8Array::new(&buf).to_vec())
}

/// Fetches the members the engine is about to read. The engine copies
/// what it reads, so the members fetched for earlier reads are dropped.
struct WebLoader {
    files: HashMap<String, File>,
    web: Arc<WebFiles>,
}

impl Loader for WebLoader {
    fn prefetch<'a>(&'a self, big: &'a BigArchive, names: &'a [String]) -> HookFuture<'a, ()> {
        Box::pin(async move {
            let mut fetched: Ranges = HashMap::new();
            for name in names {
                let Some((file, offset, size)) = big.locate(name) else {
                    continue;
                };
                let Some(f) = self.files.get(&key(&file)) else {
                    continue;
                };
                match fetch(f, offset, size as u64).await {
                    Ok(bytes) => fetched.entry(key(&file)).or_default().push((offset, bytes)),
                    Err(e) => log::warn!("{name}: {e:?}"),
                }
            }
            *self.web.members.lock().unwrap() = fetched;
        })
    }

    /// The whole file among those the player gave.
    fn read_beside<'a>(
        &'a self,
        _big: &'a BigArchive,
        name: &'a str,
    ) -> HookFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            let f = self.files.get(&key(name))?;
            match fetch(f, 0, f.size() as u64).await {
                Ok(bytes) => Some(bytes),
                Err(e) => {
                    log::warn!("{name}: {e:?}");
                    None
                }
            }
        })
    }
}

/// The save files in the browser's `localStorage`: one key
/// `atlantis-native/<build>/<file name>` per file, its bytes in base64
/// (the storage holds strings only).
struct WebStore {
    storage: Storage,
    prefix: String,
}

impl WebStore {
    fn new(build: &str) -> Option<WebStore> {
        let storage = web_sys::window()?.local_storage().ok()??;
        Some(WebStore {
            storage,
            prefix: format!("atlantis-native/{build}/"),
        })
    }

    fn key(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name.to_ascii_lowercase())
    }
}

/// Bytes as a string of code units 0..255, for `btoa`.
fn to_base64(data: &[u8]) -> Option<String> {
    let binary: String = data.iter().map(|&b| char::from(b)).collect();
    web_sys::window()?.btoa(&binary).ok()
}

fn from_base64(text: &str) -> Option<Vec<u8>> {
    let binary = web_sys::window()?.atob(text).ok()?;
    binary.chars().map(|c| u8::try_from(c).ok()).collect()
}

impl SaveStore for WebStore {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        from_base64(&self.storage.get_item(&self.key(name)).ok()??)
    }

    fn write(&self, name: &str, data: &[u8]) -> bool {
        let Some(text) = to_base64(data) else {
            return false;
        };
        match self.storage.set_item(&self.key(name), &text) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("{name}: not saved in the browser: {e:?}");
                false
            }
        }
    }

    fn remove(&self, name: &str) {
        let _ = self.storage.remove_item(&self.key(name));
    }

    fn list(&self) -> Vec<String> {
        let n = self.storage.length().unwrap_or(0);
        (0..n)
            .filter_map(|i| self.storage.key(i).ok()?)
            .filter_map(|k| k.strip_prefix(&self.prefix).map(str::to_string))
            .collect()
    }
}

/// A file of the original's saves at the top of the game folder.
fn is_original_save(key: &str) -> bool {
    !key.contains('/')
        && (key == atlantis_engine::checkpoint::PLAYERS_FILE
            || (key.starts_with('p') && key.ends_with(".gam")))
}

/// The port's version and commit, for the page.
#[wasm_bindgen]
pub fn version() -> String {
    crate::version_string()
}

/// The files of language `l` of the DVD whose root is `root` in `all`, as
/// its program reads them ([`atlantis_data::dvd::DvdFiles`]): the shared
/// folders from the side's folder, the rest from the language's.
fn dvd_files(all: HashMap<String, File>, l: &DvdLanguage, root: &str) -> HashMap<String, File> {
    all.into_iter()
        .filter_map(|(k, f)| atlantis_data::dvd::path_for_language(&k, l, root).map(|r| (r, f)))
        .collect()
}

/// Called by the page with the picked folder, and for Cryo's DVD the
/// language's folder (`ENG`, ...) once the player chose it: without it,
/// a DVD of more than one language is refused with the languages found,
/// `LANGUAGES:` then `folder=language` pairs separated by `;`, for the
/// page to ask. A folder that holds both games is refused the same way
/// with `GAMES:` then `id=name` pairs, until the page gives the game
/// chosen (`game`: `atlantis` or `atlantis2`). Returns the game started
/// (`atlantis`, `atlantis2`). `paths` gives the files' paths below the
/// picked folder's parent when the browser does not
/// (`webkitRelativePath`): the end-to-end runs of a browser whose driver
/// sets files one by one (Firefox's WebDriver BiDi).
#[wasm_bindgen]
pub async fn start(
    files: FileList,
    language: Option<String>,
    game: Option<String>,
    paths: Option<Vec<String>>,
) -> Result<JsValue, JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    // Paths are "<picked folder>/<path>".
    let mut all: HashMap<String, File> = HashMap::new();
    for i in 0..files.length() {
        let Some(f) = files.get(i) else { continue };
        // webkitRelativePath is not in web-sys's stable API.
        let rel = js_sys::Reflect::get(&f, &"webkitRelativePath".into())
            .ok()
            .and_then(|v| v.as_string())
            .filter(|r| !r.is_empty())
            .or_else(|| paths.as_ref().and_then(|p| p.get(i as usize).cloned()))
            .unwrap_or_else(|| f.name());
        all.insert(key(&rel), f);
    }
    // Which game: Atlantis II, the first game, or the player's choice.
    let paths: Vec<&str> = all.keys().map(String::as_str).collect();
    let a2 = match crate::web_layout::picked(&paths) {
        crate::web_layout::Picked::Atlantis2(l) => Some(l),
        crate::web_layout::Picked::Both(l) => match game.as_deref() {
            Some("atlantis2") => Some(l),
            Some(_) => None,
            None => {
                return Err("GAMES:atlantis=Atlantis: The Lost Tales;atlantis2=Atlantis II".into())
            }
        },
        crate::web_layout::Picked::Atlantis2DiscsOnly => {
            return Err(format!(
                "{}\n\nHelp: {}",
                crate::web_layout::A2_DISCS_ONLY,
                crate::guide::HELP_URL_A2
            )
            .into())
        }
        _ => None,
    };
    if let Some(layout) = a2 {
        start_atlantis2(all, layout)?;
        return Ok("atlantis2".into());
    }
    // Cryo's DVD: one of its languages.
    let mut dvd = atlantis_data::dvd::languages_in_paths(all.keys().map(String::as_str));
    // Only the languages whose program the port knows (the desktop library
    // lists no other): a known build is not reported as an unknown one.
    let mut known = Vec::with_capacity(dvd.len());
    let mut unsupported = Vec::new();
    for (l, root) in dvd {
        let exe = format!("{root}{}/atlantis.exe", l.path().to_ascii_lowercase());
        let Some(f) = all.get(&exe) else { continue };
        let bytes = fetch(f, 0, f.size() as u64).await?;
        if atlantis_data::exe::build_of(&bytes).is_some() {
            known.push((l, root));
        } else {
            log::info!("the DVD's {}: its program is not supported", l.language);
            unsupported.push(l.language);
        }
    }
    if known.is_empty() && !unsupported.is_empty() {
        return Err(format!(
            "Cryo's DVD: its {} version is not supported yet. Choose the folder \
             of another of its languages.",
            unsupported.join(", ")
        )
        .into());
    }
    dvd = known;
    let mut by_key: HashMap<String, File> = if dvd.is_empty() {
        // Drop the picked folder's name.
        all.into_iter()
            .map(|(k, f)| {
                (
                    k.split_once('/').map_or(k.clone(), |(_, r)| r.to_string()),
                    f,
                )
            })
            .collect()
    } else {
        let chosen = match &language {
            Some(want) => dvd
                .iter()
                .find(|(l, _)| l.folder.eq_ignore_ascii_case(want)),
            None if dvd.len() == 1 => dvd.first(),
            None => None,
        };
        let Some((l, root)) = chosen else {
            let list: Vec<String> = dvd
                .iter()
                .map(|(l, _)| format!("{}={}", l.folder, l.language))
                .collect();
            return Err(format!("LANGUAGES:{}", list.join(";")).into());
        };
        log::info!("the DVD's {} ({})", l.language, l.path());
        dvd_files(all, l, root)
    };
    // The player may pick the folder above the game's: take the shallowest
    // folder that holds atlantis.exe as the game's.
    if !by_key.contains_key("atlantis.exe") {
        let prefix = by_key
            .keys()
            .filter_map(|k| k.strip_suffix("/atlantis.exe"))
            .min_by_key(|p| p.matches('/').count())
            .map(|p| format!("{p}/"));
        if let Some(prefix) = prefix {
            by_key = by_key
                .into_iter()
                .filter_map(|(k, f)| k.strip_prefix(&prefix).map(|r| (r.to_string(), f)))
                .collect();
        }
    }
    let top: Vec<String> = by_key
        .keys()
        .filter(|k| !k.contains('/'))
        .cloned()
        .collect();
    let explain = |e: &atlantis_data::GameError| -> JsValue {
        let text = crate::guide::explain(&top, "you chose", e);
        format!("{text}\n\nHelp: {}", crate::guide::HELP_URL).into()
    };
    // An edition not supported yet: the page shows a link to a report
    // filled in with its identifiers.
    let report = |files: &WebFiles| -> JsValue {
        let e =
            atlantis_data::GameError::UnknownExecutable(files.size("atlantis.exe").unwrap_or(0));
        let text = crate::guide::explain(&top, "you chose", &e);
        let url = crate::guide::EditionReport::collect(files, &top, false)
            .map(|r| r.url("web", &crate::version_string()))
            .unwrap_or_else(|| crate::guide::NEW_ISSUE_URL.to_string());
        format!("{text}\n\nREPORT:{url}").into()
    };
    let sizes = by_key
        .iter()
        .map(|(k, f)| (k.clone(), f.size() as u64))
        .collect();
    let web = Arc::new(WebFiles {
        sizes,
        heads: Mutex::new(HashMap::new()),
        members: Mutex::new(HashMap::new()),
    });

    let add = |name: &str, offset: u64, bytes: Vec<u8>| {
        web.heads
            .lock()
            .unwrap()
            .entry(key(name))
            .or_default()
            .push((offset, bytes));
    };
    let exe = by_key
        .get("atlantis.exe")
        .ok_or_else(|| explain(&atlantis_data::GameError::NoExecutable))?;
    add("atlantis.exe", 0, fetch(exe, 0, exe.size() as u64).await?);
    for n in 1..=4 {
        let name = format!("bigcd{n}.big");
        if let Some(f) = by_key.get(&name) {
            add(
                &name,
                0,
                fetch(f, 0, ARCHIVE_HEAD.min(f.size() as u64)).await?,
            );
        }
    }

    // The original's saves (a few KB each), for the engine to take over.
    for (k, f) in &by_key {
        if is_original_save(k) && f.size() <= 65536.0 {
            add(k, 0, fetch(f, 0, f.size() as u64).await?);
        }
    }

    let game = Game::open(web.clone()).map_err(|e| match e {
        atlantis_data::GameError::UnknownExecutable(_) => report(&web),
        e => explain(&e),
    })?;
    // A Chinese release's subtitle font, before anything draws a subtitle:
    // none when the folder holds the player's own font, which the engine
    // takes first (`FontFace::for_release`).
    let code_page = game.exe.build.text_code_page;
    let player_font = match atlantis_engine::subtitles::player_font(code_page)
        .and_then(|(name, _)| by_key.get(&key(name)).map(|f| (name, f)))
    {
        Some((name, f)) => match fetch(f, 0, 4.min(f.size() as u64)).await {
            Ok(head) if atlantis_engine::subtitles::is_font_header(&head) => {
                log::info!("subtitles: the player's {name}, no font fetched");
                true
            }
            Ok(_) => {
                log::warn!("{name}: not a font");
                false
            }
            Err(e) => {
                log::warn!("{name}: {e:?}");
                false
            }
        },
        None => false,
    };
    if let Some(name) =
        atlantis_engine::subtitles::cjk_font_file(code_page).filter(|_| !player_font)
    {
        let url = format!("fonts/{name}");
        match fetch_url(&url).await {
            Ok(data) => {
                if atlantis_engine::subtitles::provide_cjk_font(code_page, data) {
                    log::info!("subtitles: {url}");
                } else {
                    log::warn!("{url}: not a font");
                }
            }
            Err(e) => log::warn!("{url}: {e:?}"),
        }
    }
    // The members that decide which bug fixes the options menu lists
    // (`FixData`: the texts' `ß`, the discs' revisions), kept.
    for (name, disc) in atlantis_engine::FixData::members(&game) {
        game.big.set_disc_copy(disc);
        let at = game.big.locate(&name);
        game.big.set_disc_copy(None);
        let Some((file, offset, size)) = at else {
            continue;
        };
        let Some(f) = by_key.get(&key(&file)) else {
            continue;
        };
        match fetch(f, offset, size as u64).await {
            Ok(bytes) => add(&file, offset, bytes),
            Err(e) => log::warn!("{name}: {e:?}"),
        }
    }
    let fixes = atlantis_engine::BugFix::of_game(&game);
    let saves: Rc<dyn SaveStore> = match WebStore::new(game.exe.build.name) {
        Some(store) => Rc::new(store),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    };
    let graphics = atlantis_engine::Settings::load(saves.as_ref())
        .unwrap_or_default()
        .graphics;
    let options = atlantis_engine::Options {
        loader: Rc::new(WebLoader { files: by_key, web }),
        ..options_with_settings(saves.clone(), false, Start::Boot)
    };
    let edition = game.edition();
    log::info!("{edition}, {}", crate::version_string());
    let (sys, runner) = start_game(game, options);
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(
        App::new(sys, Some(runner))
            .with_graphics(graphics, Some(saves))
            .with_edition(&edition, fixes),
    );
    Ok("atlantis".into())
}

/// In the browser, outside full screen: the canvas as large as the page
/// allows (less the frame's margin), 4:3, or wider with the HD mode's
/// widescreen (`wide`, up to the 2.4:1 its sides reach); the browser then
/// reports the new size as a resize. Both games' windows.
pub fn fit_canvas(w: &Window, wide: bool) {
    use winit::platform::web::WindowExtWebSys;
    let Some(page) = web_sys::window() else {
        return;
    };
    if page
        .document()
        .and_then(|d| d.fullscreen_element())
        .is_some()
    {
        return;
    }
    let Some(canvas) = w.canvas() else { return };
    let size = |v: Result<JsValue, _>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    // (16 pixels of margin and the frame's 5 on each side)
    let page_w = size(page.inner_width());
    let avail_h = (size(page.inner_height()) - 42.0).max(240.0);
    // The columns of keys beside the frame (`.keys` in web/index.html):
    // a 4:3 picture leaves them their room on a page wide enough; a
    // wide one comes first, the keys showing only in the margins it
    // leaves.
    let (avail_w, keys) = if wide {
        let avail_w = (page_w - 42.0).max(320.0);
        let w = avail_w.min(avail_h.min(avail_w * 3.0 / 4.0) * 2.4);
        (avail_w, (page_w - w) / 2.0 >= crate::SIDE_KEYS_WIDTH)
    } else if page_w >= crate::SIDE_KEYS_MIN_WIDTH {
        (
            (page_w - 42.0 - 2.0 * crate::SIDE_KEYS_WIDTH).max(320.0),
            true,
        )
    } else {
        ((page_w - 42.0).max(320.0), false)
    };
    if let Some(body) = page.document().and_then(|d| d.body()) {
        let _ = body.class_list().toggle_with_force("show-keys", keys);
    }
    let h = avail_h.min(avail_w * 3.0 / 4.0);
    let w = if wide {
        avail_w.min(h * 2.4)
    } else {
        h * 4.0 / 3.0
    };
    let (w, h) = (format!("{}px", w.floor()), format!("{}px", h.floor()));
    let style = canvas.style();
    if style.get_property_value("width").ok().as_deref() != Some(w.as_str())
        || style.get_property_value("height").ok().as_deref() != Some(h.as_str())
    {
        let _ = style.set_property("width", &w);
        let _ = style.set_property("height", &h);
    }
}

#[wasm_bindgen(inline_js = r#"
export function read_picked(file, start, end, out) {
  // A part of a picked file, read at once: a synchronous request of a
  // blob: URL of the slice (an array buffer is refused to a synchronous
  // request on the page's thread; the bytes come as x-user-defined text,
  // each char's low byte).
  const url = URL.createObjectURL(file.slice(start, end));
  try {
    const x = new XMLHttpRequest();
    x.open("GET", url, false);
    x.overrideMimeType("text/plain; charset=x-user-defined");
    x.send();
    const s = x.responseText;
    if (x.status !== 200 && x.status !== 0) return false;
    if (s.length !== out.length) return false;
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return true;
  } catch (e) {
    console.warn("read_picked", e);
    return false;
  } finally {
    URL.revokeObjectURL(url);
  }
}
"#)]
extern "C" {
    /// Bytes `start..end` of `file` into `out` (as long), at once; false
    /// when the browser could not read them.
    fn read_picked(file: &File, start: f64, end: f64, out: &mut [u8]) -> bool;
}

thread_local! {
    /// The files the player picked (Atlantis II's), by index: a browser's
    /// `File` stays on the page's thread, the engine's sources hold the
    /// indices.
    static PICKED: RefCell<Vec<File>> = const { RefCell::new(Vec::new()) };
}

/// A read larger than this is made in parts (the text a synchronous
/// request returns is twice the bytes).
const READ_PART: u64 = 8 << 20;

/// A read slower than this is logged.
const SLOW_READ_MS: f64 = 50.0;

/// Reads `len` bytes at `offset` of picked file `index`.
fn read_picked_part(index: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
    PICKED.with(|p| {
        let files = p.borrow();
        let f = files.get(index)?;
        let mut out = vec![0u8; len];
        let mut at = 0usize;
        while at < len {
            let n = (len - at).min(READ_PART as usize);
            let start = offset + at as u64;
            if !read_picked(
                f,
                start as f64,
                (start + n as u64) as f64,
                &mut out[at..at + n],
            ) {
                return None;
            }
            at += n;
        }
        Some(out)
    })
}

/// A folder of the picked files (Atlantis II's installed folder, a disc's
/// copy): its files by their path below it (lower case), read at once
/// when the engine reads them ([`read_picked`]).
struct PickedDir {
    /// Lower-case path below the folder -> (index in [`PICKED`], size).
    files: HashMap<String, (usize, u64)>,
}

impl PickedDir {
    /// The files of `all` (lower-case path -> index) under `prefix`.
    fn new(all: &HashMap<String, (usize, u64)>, prefix: &str) -> PickedDir {
        PickedDir {
            files: all
                .iter()
                .filter_map(|(k, v)| k.strip_prefix(prefix).map(|r| (r.to_string(), *v)))
                .collect(),
        }
    }
}

impl FileSource for PickedDir {
    fn size(&self, name: &str) -> Option<u64> {
        self.files.get(&key(name)).map(|&(_, s)| s)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let &(index, size) = self.files.get(&key(name))?;
        if offset.checked_add(len as u64)? > size {
            return None;
        }
        let t = web_time::Instant::now();
        let data = read_picked_part(index, offset, len);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        if ms > SLOW_READ_MS {
            log::info!("{name}: {len} bytes read in {ms:.0} ms");
        }
        if data.is_none() {
            log::warn!("{name}: the browser could not read it");
        }
        data
    }

    fn has_dir(&self, name: &str) -> bool {
        let prefix = key(name) + "/";
        self.files.keys().any(|k| k.starts_with(&prefix))
    }

    /// The files of folder `dir` (lower case).
    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = match key(dir) {
            d if d.is_empty() => d,
            d => d + "/",
        };
        self.files
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|n| !n.contains('/'))
            .map(str::to_string)
            .collect()
    }
}

/// Atlantis II from the picked files: its installed folder and discs
/// ([`crate::web_layout::A2Layout`]), the saves in the browser's storage,
/// the player menu under the start-up intro, as the desktop starts it.
fn start_atlantis2(
    all: HashMap<String, File>,
    layout: crate::web_layout::A2Layout,
) -> Result<(), JsValue> {
    use atlantis2_data::files::{find_exe, OpenError};
    let help =
        |text: &str| -> JsValue { format!("{text}\n\nHelp: {}", crate::guide::HELP_URL_A2).into() };
    let mut index: HashMap<String, (usize, u64)> = HashMap::new();
    PICKED.with(|p| {
        let mut p = p.borrow_mut();
        p.clear();
        for (k, f) in all {
            index.insert(k, (p.len(), f.size() as u64));
            p.push(f);
        }
    });
    let install = PickedDir::new(&index, &layout.install);
    let (exe_name, build, _) = find_exe(&install).map_err(|e| match e {
        OpenError::Exe { name, error } => help(&format!(
            "This folder holds an Atlantis II program, {name}, that Atlantis Native does not \
             know ({error}): an edition of the game that is not supported yet.\n\n\
             Supported: the French CD (1999), the CD with patch 1.01, and GOG.com."
        )),
        e => help(&e.to_string()),
    })?;
    let discs: Vec<(String, Arc<dyn FileSource>)> = layout
        .discs
        .iter()
        .map(|p| {
            let name = if p.is_empty() { "/" } else { p.as_str() };
            (
                name.to_string(),
                Arc::new(PickedDir::new(&index, p)) as Arc<dyn FileSource>,
            )
        })
        .collect();
    let mut files = atlantis2_data::A2Files::new(Arc::new(install), discs);
    files.set_exe(exe_name, build);
    let found = files.disc_numbers();
    if found.is_empty() {
        return Err(help(crate::web_layout::A2_NO_DISC));
    }
    // Every disc at once: the browser cannot be given another one while
    // the game runs (the desktop's CD page looks in its folders again).
    let missing: Vec<u8> = (1..=4).filter(|n| !found.contains(n)).collect();
    if !missing.is_empty() {
        return Err(help(&crate::web_layout::a2_missing_discs(&missing)));
    }
    let store: Rc<dyn SaveStore> = match WebStore::new("atlantis2") {
        Some(s) => Rc::new(s),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    };
    let enhancements = crate::a2::load_settings(store.as_ref()).enhancements;
    let (mut engine, label) =
        crate::a2::boot_files(files, enhancements, |e| e.set_store(Some(store.clone())))
            .map_err(|e| help(&e))?;
    crate::a2::A2Start::default().begin(&mut engine, false)?;
    log::info!("{label}, {}", crate::version_string());
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(crate::a2::window_app(engine, &label, false, None));
    Ok(())
}
