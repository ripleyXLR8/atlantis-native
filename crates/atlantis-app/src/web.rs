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

use atlantis_data::disc_image::{DiscImage, DiscSet, ImageBytes};
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

pub(crate) fn key(name: &str) -> String {
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
                // Mostly the storage full (`QuotaExceededError`): the game
                // only logs a save it could not write, as the original
                // does, so the page says it.
                log::warn!("{name}: not saved in the browser: {e:?}");
                show_notice(&format!(
                    "The browser refused to keep {name}: it was not saved. Its storage for \
                     this site is probably full; erase saves you no longer need."
                ));
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

/// The first game's state as the end-to-end runs read it: engine steps
/// run, the game's clock (ms) at the last, the sound started, the game
/// held (the page hidden).
#[derive(Clone, Copy, Default)]
pub(crate) struct A1State {
    pub steps: u64,
    pub ms: u64,
    pub audio: bool,
    pub held: bool,
}

thread_local! {
    static A1_STATE: std::cell::Cell<A1State> = std::cell::Cell::new(A1State::default());
}

/// Updates the first game's state ([`atlantis1_state`]).
pub(crate) fn note_a1(f: impl FnOnce(&mut A1State)) {
    A1_STATE.with(|c| {
        let mut s = c.get();
        f(&mut s);
        c.set(s);
    });
}

/// The first game's state for the end-to-end runs (a testing aid):
/// `{steps, ms, audio, held}`.
#[wasm_bindgen]
pub fn atlantis1_state() -> JsValue {
    let s = A1_STATE.with(|c| c.get());
    let o = js_sys::Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v);
    };
    set("steps", JsValue::from_f64(s.steps as f64));
    set("ms", JsValue::from_f64(s.ms as f64));
    set("audio", JsValue::from_bool(s.audio));
    set("held", JsValue::from_bool(s.held));
    o.into()
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
/// page to ask. A folder that holds more than one game is refused the
/// same way with `GAMES:` then `id=name` pairs, until the page gives the
/// game chosen (`game`: `atlantis`, `atlantis2` or `atlantis3`). Returns
/// the game started (`atlantis`, `atlantis2`, `atlantis3`). `paths` gives the files' paths below the
/// picked folder's parent when the browser does not
/// (`webkitRelativePath`): the end-to-end runs of a browser whose driver
/// sets files one by one (Firefox's WebDriver BiDi).
#[wasm_bindgen]
pub async fn start(
    files: FileList,
    language: Option<String>,
    game: Option<String>,
    paths: Option<Vec<String>>,
    a4_language: Option<String>,
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
    // Which game: Atlantis III, Atlantis II, the first game, or the
    // player's choice.
    let paths: Vec<&str> = all.keys().map(String::as_str).collect();
    let picked = crate::web_layout::picked(&paths);
    // The Secrets of Atlantis: a copy of its DVD, an install, or the
    // extracted cabinets, alone or chosen among the games of the folder.
    // A `data1.hdr` is the game's only when its header says so (any
    // InstallShield set has one): read from the picked files.
    let picked_index = std::cell::OnceCell::new();
    let cds = |dir: &str| {
        let index = picked_index.get_or_init(|| keep_picked(all.clone()));
        atlantis3_data::atlantis5::holds_atlantis5_cds_header(Arc::new(PickedDir::new(index, dir)))
    };
    if let Some(folder) = crate::web_layout::a5_folder(&paths, cds) {
        use crate::web_layout::Picked;
        let mut others: Vec<&str> = match &picked {
            Picked::Both(_) => vec!["atlantis=Atlantis: The Lost Tales", "atlantis2=Atlantis II"],
            Picked::Atlantis2(_) => vec!["atlantis2=Atlantis II"],
            Picked::LostTales => vec!["atlantis=Atlantis: The Lost Tales"],
            _ => Vec::new(),
        };
        if crate::web_layout::a3_install(&paths).is_some() {
            others.push("atlantis3=Atlantis III");
        }
        if crate::web_layout::a4_install(&paths).is_some() {
            others.push("atlantis4=Atlantis Evolution");
        }
        if crate::web_layout::a4cd_install(&paths).is_some() {
            others.push("atlantis4cd=Atlantis Evolution (2004)");
        }
        if others.is_empty() || game.as_deref() == Some("atlantis5") {
            drop(paths);
            start_atlantis5(all, &folder, language.as_deref(), a4_language.as_deref())?;
            return Ok("atlantis5".into());
        }
        if game.is_none() {
            return Err(format!(
                "GAMES:{};atlantis5=The Secrets of Atlantis",
                others.join(";")
            )
            .into());
        }
    }
    // Atlantis Evolution's 2004 CD alone, or chosen among the games of
    // the folder.
    if let Some(install) = crate::web_layout::a4cd_install(&paths) {
        use crate::web_layout::Picked;
        let mut others: Vec<&str> = match &picked {
            Picked::Both(_) => vec!["atlantis=Atlantis: The Lost Tales", "atlantis2=Atlantis II"],
            Picked::Atlantis2(_) => vec!["atlantis2=Atlantis II"],
            Picked::LostTales => vec!["atlantis=Atlantis: The Lost Tales"],
            _ => Vec::new(),
        };
        if crate::web_layout::a3_install(&paths).is_some() {
            others.push("atlantis3=Atlantis III");
        }
        if crate::web_layout::a4_install(&paths).is_some() {
            others.push("atlantis4=Atlantis Evolution");
        }
        if others.is_empty() || game.as_deref() == Some("atlantis4cd") {
            drop(paths);
            start_evolution_2004(all, &install)?;
            return Ok("atlantis4cd".into());
        }
        if game.is_none() {
            return Err(format!(
                "GAMES:{};atlantis4cd=Atlantis Evolution (2004)",
                others.join(";")
            )
            .into());
        }
    }
    // Atlantis Evolution alone, or chosen among the games of the folder.
    if let Some(install) = crate::web_layout::a4_install(&paths) {
        use crate::web_layout::Picked;
        let mut others: Vec<&str> = match &picked {
            Picked::Both(_) => vec!["atlantis=Atlantis: The Lost Tales", "atlantis2=Atlantis II"],
            Picked::Atlantis2(_) => vec!["atlantis2=Atlantis II"],
            Picked::LostTales => vec!["atlantis=Atlantis: The Lost Tales"],
            _ => Vec::new(),
        };
        if crate::web_layout::a3_install(&paths).is_some() {
            others.push("atlantis3=Atlantis III");
        }
        if others.is_empty() || game.as_deref() == Some("atlantis4") {
            drop(paths);
            start_atlantis4(all, &install, a4_language.as_deref())?;
            return Ok("atlantis4".into());
        }
        if game.is_none() {
            return Err(format!("GAMES:{};atlantis4=Atlantis Evolution", others.join(";")).into());
        }
    }
    if let Some(install) = crate::web_layout::a3_install(&paths) {
        use crate::web_layout::Picked;
        let others: &[&str] = match &picked {
            Picked::Both(_) => &["atlantis=Atlantis: The Lost Tales", "atlantis2=Atlantis II"],
            Picked::Atlantis2(_) => &["atlantis2=Atlantis II"],
            Picked::LostTales => &["atlantis=Atlantis: The Lost Tales"],
            _ => &[],
        };
        // Atlantis III alone, or chosen among the games of the folder.
        if others.is_empty() || game.as_deref() == Some("atlantis3") {
            start_atlantis3(all, &install).await?;
            return Ok("atlantis3".into());
        }
        if game.is_none() {
            return Err(format!("GAMES:{};atlantis3=Atlantis III", others.join(";")).into());
        }
    }
    let a2 = match picked {
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
        start_atlantis2(all, layout).await?;
        return Ok("atlantis2".into());
    }
    // The first game's CD images or Cryo's DVD's, when the folder holds no
    // copy of its files: the game read from them, a part at a time.
    let paths: Vec<&str> = all.keys().map(String::as_str).collect();
    if !crate::web_layout::holds_lost_tales(&paths) {
        // The Secrets of Atlantis's DVD image among them, found by
        // reading them.
        if let Some(image) = a5_image(&all) {
            drop(paths);
            start_atlantis5_image(all, &image, language.as_deref(), a4_language.as_deref())?;
            return Ok("atlantis5".into());
        }
        let folders = crate::web_layout::image_folders(&paths);
        if !folders.is_empty() {
            return start_from_images(all, folders, language).await;
        }
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
    launch_atlantis1(game, player_font, Rc::new(WebLoader { files: by_key, web })).await?;
    Ok("atlantis".into())
}

/// Starts the first game `game`, its files read through `loader`:
/// fetches the Chinese releases' subtitle font unless the player has
/// theirs (`player_font`), lists the bug fixes, opens the saves.
async fn launch_atlantis1(
    game: Game,
    player_font: bool,
    loader: Rc<dyn Loader>,
) -> Result<(), JsValue> {
    let code_page = game.exe.build.text_code_page;
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
        loader,
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
    Ok(())
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

#[wasm_bindgen(inline_js = r##"
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

export function show_notice(text) {
  // A message at the top of the page over the game, for 15 seconds.
  let box = document.getElementById("atlantis-notice");
  if (!box) {
    box = document.createElement("div");
    box.id = "atlantis-notice";
    box.setAttribute("role", "alert");
    Object.assign(box.style, {
      position: "fixed", top: "8px", left: "50%", transform: "translateX(-50%)",
      zIndex: "20", maxWidth: "min(640px, calc(100vw - 32px))", padding: "8px 14px",
      background: "#3a1410", color: "#ffd9d0", border: "1px solid #ff9a8a",
      borderRadius: "6px", font: "14px/1.4 sans-serif", pointerEvents: "none",
    });
    document.body.append(box);
  }
  box.textContent = text;
  box.style.display = "block";
  clearTimeout(box.hideTimer);
  box.hideTimer = setTimeout(() => { box.style.display = "none"; }, 15000);
}
"##)]
extern "C" {
    /// Bytes `start..end` of `file` into `out` (as long), at once; false
    /// when the browser could not read them.
    fn read_picked(file: &File, start: f64, end: f64, out: &mut [u8]) -> bool;
    /// A message shown at the top of the page for a while.
    fn show_notice(text: &str);
}

/// A message shown at the top of the page for a while.
pub(crate) fn notice(text: &str) {
    show_notice(text);
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

thread_local! {
    /// The time spent in synchronous reads of the picked files.
    static READ_TIME: std::cell::Cell<web_time::Duration> =
        const { std::cell::Cell::new(web_time::Duration::ZERO) };
}

/// The time the page has spent reading the picked files at once
/// ([`read_picked`]): Atlantis Evolution's clock does not count it.
pub(crate) fn read_time() -> web_time::Duration {
    READ_TIME.with(std::cell::Cell::get)
}

/// Reads `len` bytes at `offset` of picked file `index` without blocking
/// the page: the browser reads it in the background (Atlantis Evolution's
/// read-ahead, [`crate::a4_prefetch`]).
pub(crate) async fn read_picked_async(index: usize, offset: u64, len: u64) -> Option<Vec<u8>> {
    let file = PICKED.with(|p| p.borrow().get(index).cloned())?;
    fetch(&file, offset, len).await.ok()
}

/// Reads `len` bytes at `offset` of picked file `index`.
fn read_picked_part(index: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
    let t = web_time::Instant::now();
    let data = read_picked_part_now(index, offset, len);
    READ_TIME.with(|r| r.set(r.get() + t.elapsed()));
    data
}

fn read_picked_part_now(index: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; len];
    read_picked_into(index, offset, &mut out).then_some(out)
}

/// Fills `out` with the bytes at `offset` of picked file `index`, at once
/// (by parts of [`READ_PART`]).
fn read_picked_into(index: usize, offset: u64, out: &mut [u8]) -> bool {
    PICKED.with(|p| {
        let files = p.borrow();
        let Some(f) = files.get(index) else {
            return false;
        };
        let len = out.len();
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
                return false;
            }
            at += n;
        }
        true
    })
}

/// A part of a picked file read in the background ([`PickedDir`]'s
/// `read_ahead`): its bytes once the browser gave them.
struct Ahead {
    index: usize,
    offset: u64,
    len: usize,
    data: Rc<RefCell<Option<Vec<u8>>>>,
}

/// The parts read ahead kept (a cinematic needs the next one; Atlantis
/// III may play a cinematic over another's sound).
const AHEAD_KEPT: usize = 4;

thread_local! {
    /// The parts read ahead, the newest last.
    static AHEAD: RefCell<Vec<Ahead>> = const { RefCell::new(Vec::new()) };
    /// The parts of the images that hold the members the first game's
    /// scene needs ([`ImageLoader`]), until the next scene's.
    static MEMBERS: RefCell<Vec<Ahead>> = const { RefCell::new(Vec::new()) };
    /// The parts of the images that hold the first game's program and its
    /// archives' directories, kept.
    static HEADS: RefCell<Vec<Ahead>> = const { RefCell::new(Vec::new()) };
    /// The parts [`ImageLoader`] is collecting, while it asks for them.
    static COLLECTING: RefCell<Option<Vec<(usize, u64, usize)>>> = const { RefCell::new(None) };
}

/// Bytes `offset..offset + len` of picked file `index` from a part read
/// ahead, if one holds them and has arrived.
fn take_ahead(index: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
    let from = |a: &Vec<Ahead>| -> Option<Vec<u8>> {
        let p = a.iter().find(|p| {
            p.index == index && p.offset <= offset && offset + len as u64 <= p.offset + p.len as u64
        })?;
        let d = p.data.borrow();
        let d = d.as_ref()?;
        let at = usize::try_from(offset - p.offset).ok()?;
        d.get(at..at + len).map(<[u8]>::to_vec)
    };
    AHEAD
        .with(|a| from(&a.borrow()))
        .or_else(|| MEMBERS.with(|m| from(&m.borrow())))
        .or_else(|| HEADS.with(|h| from(&h.borrow())))
}

/// Bytes `offset..offset + len` of picked file `index` read in the
/// background, unless asked for already; while the first game's
/// [`ImageLoader`] collects the parts a scene needs, noted for it.
fn read_ahead_part(index: usize, offset: u64, len: usize) {
    if len == 0 {
        return;
    }
    let collected = COLLECTING.with(|c| {
        c.borrow_mut()
            .as_mut()
            .map(|parts| parts.push((index, offset, len)))
            .is_some()
    });
    if collected {
        return;
    }
    let Some(file) = PICKED.with(|p| p.borrow().get(index).cloned()) else {
        return;
    };
    let data = Rc::new(RefCell::new(None));
    let fresh = AHEAD.with(|a| {
        let mut a = a.borrow_mut();
        if a.iter().any(|p| p.index == index && p.offset == offset) {
            return false;
        }
        if a.len() >= AHEAD_KEPT {
            a.remove(0);
        }
        a.push(Ahead {
            index,
            offset,
            len,
            data: data.clone(),
        });
        true
    });
    if fresh {
        wasm_bindgen_futures::spawn_local(async move {
            match fetch(&file, offset, len as u64).await {
                Ok(d) if d.len() == len => *data.borrow_mut() = Some(d),
                // Not read: forgotten ([`reads_pending`]), read at once
                // when it is needed.
                _ => AHEAD.with(|a| {
                    a.borrow_mut()
                        .retain(|p| !(p.index == index && p.offset == offset))
                }),
            }
        });
    }
}

/// Whether a part asked ahead ([`FileSource::read_ahead`]) is still
/// being read: The Secrets of Atlantis decodes its cabinets between
/// frames only when it would not wait for the browser.
pub(crate) fn reads_pending() -> bool {
    AHEAD.with(|a| a.borrow().iter().any(|p| p.data.borrow().is_none()))
}

/// A folder of the picked files (Atlantis II's installed folder, a disc's
/// copy): its files by their path below it (lower case), read at once
/// when the engine reads them ([`read_picked`]), or from a part read
/// ahead in the background when the engine announced it
/// ([`FileSource::read_ahead`]: a cinematic read by parts).
pub(crate) struct PickedDir {
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

impl PickedDir {
    /// A file's index in [`PICKED`] and its size.
    pub(crate) fn entry(&self, name: &str) -> Option<(usize, u64)> {
        self.files.get(&key(name)).copied()
    }

    /// Every file: lower-case path, index in [`PICKED`] and size.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&String, &(usize, u64))> {
        self.files.iter()
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
        if let Some(d) = take_ahead(index, offset, len) {
            return Some(d);
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

    fn read_ahead(&self, name: &str, offset: u64, len: usize) {
        let Some(&(index, size)) = self.files.get(&key(name)) else {
            return;
        };
        let len = usize::try_from(size.saturating_sub(offset)).map_or(len, |rest| rest.min(len));
        read_ahead_part(index, offset, len);
    }

    fn has_dir(&self, name: &str) -> bool {
        let prefix = key(name) + "/";
        self.files.keys().any(|k| k.starts_with(&prefix))
    }

    /// The entries of folder `dir` (lower case): its files and its
    /// folders, as the desktop's `StdDir` lists them (Atlantis III's
    /// install language is the one folder of `Datas\loc\text`).
    fn list(&self, dir: &str) -> Vec<String> {
        crate::web_layout::entries(self.files.keys().map(String::as_str), &key(dir))
    }
}

/// The picked files kept in [`PICKED`]: lower-case path -> (index, size).
fn keep_picked(all: HashMap<String, File>) -> HashMap<String, (usize, u64)> {
    let mut index: HashMap<String, (usize, u64)> = HashMap::new();
    PICKED.with(|p| {
        let mut p = p.borrow_mut();
        p.clear();
        for (k, f) in all {
            index.insert(k, (p.len(), f.size() as u64));
            p.push(f);
        }
    });
    index
}

/// A picked CD image's bytes ([`DiscImage`]'s source in the browser):
/// read at once when the image reader asks for them ([`read_picked`]),
/// or from a part read ahead or fetched for the first game's loader.
struct PickedImage {
    /// Its index in [`PICKED`].
    index: usize,
    size: u64,
}

impl ImageBytes for PickedImage {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool {
        let len = buf.len();
        if offset.saturating_add(len as u64) > self.size {
            return false;
        }
        if let Some(d) = take_ahead(self.index, offset, len) {
            buf.copy_from_slice(&d);
            return true;
        }
        let t = web_time::Instant::now();
        let ok = read_picked_into(self.index, offset, buf);
        READ_TIME.with(|r| r.set(r.get() + t.elapsed()));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        if ms > SLOW_READ_MS {
            log::info!("image {}: {len} bytes read in {ms:.0} ms", self.index);
        }
        if !ok {
            log::warn!("image {}: the browser could not read it", self.index);
        }
        ok
    }

    fn read_ahead(&self, offset: u64, len: u64) {
        let len = len.min(self.size.saturating_sub(offset));
        if let Ok(len) = usize::try_from(len) {
            read_ahead_part(self.index, offset, len);
        }
    }
}

/// The disc candidate at `place` of the picked files (`index`): a CD
/// image ([`crate::web_layout::is_image`], read by parts through the
/// desktop's reader) or a folder; `None` for an image that holds no ISO
/// 9660 file system.
fn picked_source(
    index: &HashMap<String, (usize, u64)>,
    place: &str,
) -> Option<Arc<dyn FileSource>> {
    if crate::web_layout::is_image(place) {
        let &(i, size) = index.get(place)?;
        let img = DiscImage::from_bytes(place, Box::new(PickedImage { index: i, size }));
        if img.is_none() {
            log::info!("{place}: not a CD image the game reads");
        }
        Some(Arc::new(img?))
    } else {
        Some(Arc::new(PickedDir::new(index, place)))
    }
}

/// The disc candidates at `places` among the picked files.
fn picked_sources(
    index: &HashMap<String, (usize, u64)>,
    places: &[String],
) -> Vec<(String, Arc<dyn FileSource>)> {
    places
        .iter()
        .filter_map(|p| {
            let name = if p.is_empty() { "/" } else { p.as_str() };
            Some((name.to_string(), picked_source(index, p)?))
        })
        .collect()
}

/// The first game from CD images: the images of the first of `folders`
/// that holds it (its CDs' images, or Cryo's DVD's sides, then the
/// language as from the DVD's copies), read a part at a time; the members
/// a scene needs fetched in the background before it reads them
/// ([`ImageLoader`]).
async fn start_from_images(
    all: HashMap<String, File>,
    folders: Vec<(String, Vec<String>)>,
    language: Option<String>,
) -> Result<JsValue, JsValue> {
    use atlantis_data::dvd::{self, DvdFiles};
    let index = keep_picked(all);
    let mut other = None;
    for (folder, images) in folders {
        let set = DiscSet {
            images: images
                .iter()
                .filter_map(|p| {
                    let &(i, size) = index.get(p)?;
                    DiscImage::from_bytes(p.as_str(), Box::new(PickedImage { index: i, size }))
                })
                .collect(),
        };
        if set.images.is_empty() {
            continue;
        }
        let set: Arc<dyn FileSource> = Arc::new(set);
        let languages: Vec<&'static DvdLanguage> = dvd::languages_in(set.as_ref())
            .into_iter()
            .filter(|l| DvdFiles::new(set.clone(), l).is_some())
            .collect();
        let source: Arc<dyn FileSource> = if !languages.is_empty() {
            let known: Vec<&'static DvdLanguage> = languages
                .into_iter()
                .filter(|l| {
                    set.read(&format!("{}/ATLANTIS.EXE", l.path()))
                        .is_some_and(|b| atlantis_data::exe::build_of(&b).is_some())
                })
                .collect();
            let chosen = match &language {
                Some(want) => known.iter().find(|l| l.folder.eq_ignore_ascii_case(want)),
                None if known.len() == 1 => known.first(),
                None => None,
            };
            let Some(l) = chosen else {
                let list: Vec<String> = known
                    .iter()
                    .map(|l| format!("{}={}", l.folder, l.language))
                    .collect();
                return Err(format!("LANGUAGES:{}", list.join(";")).into());
            };
            log::info!("the DVD's {} ({}) from its images", l.language, l.path());
            Arc::new(DvdFiles::new(set, l).ok_or("the DVD's language")?)
        } else if set.size("atlantis.exe").is_some() {
            set
        } else {
            if !atlantis2_data::files::disc_numbers_of(set.as_ref()).is_empty() {
                other.get_or_insert(crate::web_layout::A2_DISCS_ONLY);
            } else if atlantis3_data::discs::disc_number(set.as_ref()) != 0 {
                other.get_or_insert(crate::web_layout::A3_DISCS_ONLY);
            }
            continue;
        };
        log::info!("{folder}: the game from {} disc images", images.len());
        // The program and the archives' directories (read entry by entry
        // as the game opens: a request each would take minutes), fetched
        // first and kept, as from the copies.
        let heads = fetch_collected(|| {
            if let Some(size) = source.size("atlantis.exe") {
                source.read_ahead("atlantis.exe", 0, size as usize);
            }
            for n in 1..=4 {
                let name = format!("bigcd{n}.big");
                if let Some(size) = source.size(&name) {
                    source.read_ahead(&name, 0, ARCHIVE_HEAD.min(size) as usize);
                }
            }
        })
        .await;
        HEADS.with(|h| *h.borrow_mut() = heads);
        let top: Vec<String> = images
            .iter()
            .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
            .collect();
        let game = Game::open(source.clone()).map_err(|e| -> JsValue {
            if let atlantis_data::GameError::UnknownExecutable(_) = e {
                let text = crate::guide::explain(&top, "you chose", &e);
                let url = crate::guide::EditionReport::collect(source.as_ref(), &top, true)
                    .map(|r| r.url("web", &crate::version_string()))
                    .unwrap_or_else(|| crate::guide::NEW_ISSUE_URL.to_string());
                return format!("{text}\n\nREPORT:{url}").into();
            }
            let text = crate::guide::explain(&top, "you chose", &e);
            format!("{text}\n\nHelp: {}", crate::guide::HELP_URL).into()
        })?;
        // The player's own subtitle font beside the images.
        let code_page = game.exe.build.text_code_page;
        let player_font = atlantis_engine::subtitles::player_font(code_page)
            .and_then(|(name, _)| index.get(&format!("{folder}{}", key(name))))
            .and_then(|&(i, size)| read_picked_part(i, 0, 4.min(size as usize)))
            .is_some_and(|head| atlantis_engine::subtitles::is_font_header(&head));
        launch_atlantis1(game, player_font, Rc::new(ImageLoader)).await?;
        return Ok("atlantis".into());
    }
    let text =
        other.unwrap_or("The CD images among the chosen files hold no game Atlantis Native knows.");
    Err(format!("{text}\n\nHelp: {}", crate::guide::HELP_URL).into())
}

/// The parts of the picked files that `ask` announces
/// ([`FileSource::read_ahead`] through the image reader), collected rather
/// than read in the background, then fetched (`Blob.arrayBuffer`).
async fn fetch_collected(ask: impl FnOnce()) -> Vec<Ahead> {
    COLLECTING.with(|c| *c.borrow_mut() = Some(Vec::new()));
    ask();
    let wanted = COLLECTING
        .with(|c| c.borrow_mut().take())
        .unwrap_or_default();
    let mut got = Vec::with_capacity(wanted.len());
    for (index, offset, len) in wanted {
        let Some(file) = PICKED.with(|p| p.borrow().get(index).cloned()) else {
            continue;
        };
        match fetch(&file, offset, len as u64).await {
            Ok(d) if d.len() == len => got.push(Ahead {
                index,
                offset,
                len,
                data: Rc::new(RefCell::new(Some(d))),
            }),
            Ok(_) => {}
            Err(e) => log::warn!("image {index}: {e:?}"),
        }
    }
    got
}

/// The first game's loader when it is read from CD images: the members a
/// scene needs ([`Loader::prefetch`]) are fetched in the background, as
/// the parts of the images that hold them ([`FileSource::read_ahead`]
/// through the image reader), and the engine's reads then find them
/// there; what was not fetched is read at once.
struct ImageLoader;

impl Loader for ImageLoader {
    fn prefetch<'a>(&'a self, big: &'a BigArchive, names: &'a [String]) -> HookFuture<'a, ()> {
        Box::pin(async move {
            let got = fetch_collected(|| {
                for name in names {
                    if let Some((file, offset, size)) = big.locate(name) {
                        big.source().read_ahead(&file, offset, size);
                    }
                }
            })
            .await;
            MEMBERS.with(|m| *m.borrow_mut() = got);
        })
    }
}

/// Atlantis III from its installed folder among the picked files
/// ([`crate::a3_web`]), the saves in the browser's storage. The
/// Traditional Chinese CD's Big5 text font is fetched first, unless the
/// folder holds the player's `mingliu.ttc`.
async fn start_atlantis3(all: HashMap<String, File>, install: &str) -> Result<(), JsValue> {
    let discs = {
        let paths: Vec<&str> = all.keys().map(String::as_str).collect();
        crate::web_layout::a3_discs(&paths, install)
    };
    let index = keep_picked(all);
    let discs = picked_sources(&index, &discs);
    // The Big5 fonts are drawn in the system's Big5 face
    // (`atlantis3_engine::text::Texts::big5_face`): the player's, else
    // the port's, fetched beside the page.
    let dir = PickedDir::new(&index, install);
    if atlantis3_data::install::has_big5_fonts(&dir) {
        let code_page = atlantis_data::TextCodePage::Windows950;
        let player_font = atlantis_engine::subtitles::player_font(code_page)
            .is_some_and(|(name, _)| dir.size(name).is_some());
        if let Some(name) =
            atlantis_engine::subtitles::cjk_font_file(code_page).filter(|_| !player_font)
        {
            let url = format!("fonts/{name}");
            match fetch_url(&url).await {
                Ok(data) => {
                    if atlantis_engine::subtitles::provide_cjk_font(code_page, data) {
                        log::info!("text font: {url}");
                    } else {
                        log::warn!("{url}: not a font");
                    }
                }
                Err(e) => log::warn!("{url}: {e:?}"),
            }
        }
    }
    let store: Rc<dyn SaveStore> = match WebStore::new("atlantis3") {
        Some(s) => Rc::new(s),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    };
    crate::a3_web::start(Arc::new(PickedDir::new(&index, install)), discs, store)
}

/// The picked image (shallowest first) that holds The Secrets of
/// Atlantis's DVD (`DATAS.cab` at its root), if any.
fn a5_image(all: &HashMap<String, File>) -> Option<String> {
    let mut images: Vec<&String> = all
        .keys()
        .filter(|k| crate::web_layout::is_image(k))
        .collect();
    images.sort_by_key(|k| (k.matches('/').count(), (*k).clone()));
    // The images alone in the picked files' table, for the reader.
    PICKED.with(|p| {
        let mut p = p.borrow_mut();
        p.clear();
        p.extend(images.iter().map(|k| all[*k].clone()));
    });
    let found = images.iter().enumerate().find_map(|(i, k)| {
        let size = all[*k].size() as u64;
        let img = DiscImage::from_bytes(k.as_str(), Box::new(PickedImage { index: i, size }))?;
        img.size(atlantis3_data::atlantis5::DATA_CAB)
            .map(|_| (*k).clone())
    });
    PICKED.with(|p| p.borrow_mut().clear());
    found
}

/// The browser's storage of The Secrets of Atlantis's saves.
fn a5_store() -> Rc<dyn SaveStore> {
    match WebStore::new("atlantis5") {
        Some(s) => Rc::new(s),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    }
}

/// Why The Secrets of Atlantis cannot be played from the picked files.
fn a5_error(e: &atlantis3_data::atlantis5::OpenError) -> JsValue {
    format!(
        "The Secrets of Atlantis: {e}.\n\nChoose the DVD's image (.iso), the DVD drive or a \
         copy of the disc (the folder holding DATAS.cab), a folder holding copies of the three \
         North-American CDs (one folder per disc, or their files together), or the folder \
         where the game is installed.\n\nHelp: {}",
        crate::guide::HELP_URL
    )
    .into()
}

/// The language of The Secrets of Atlantis's disc of several languages
/// (the international DVD's four): `language` once the page's player chose
/// it (a folder name: `german`, ...), else the page address's `?lang=`
/// (`code`: `de`, `en`, `es`, `it`, `fr`); without either the page is
/// asked, with `A5LANGUAGES:` then `folder=name` pairs separated by `;`
/// in the installer's order (as Cryo's DVD's `LANGUAGES:`). A disc of one
/// language plays it.
fn a5_choose_language(
    src: &mut atlantis3_data::atlantis5::Atlantis5Source,
    language: Option<&str>,
    code: Option<&str>,
) -> Result<(), JsValue> {
    use atlantis3_data::atlantis5::{in_installer_order, language_name, language_of_code};
    if src.languages().len() < 2 {
        return Ok(());
    }
    let wanted = [language, code.and_then(language_of_code)];
    if wanted.into_iter().flatten().any(|l| src.choose_language(l)) {
        return Ok(());
    }
    let list: Vec<String> = in_installer_order(src.languages())
        .iter()
        .map(|l| format!("{l}={}", language_name(l)))
        .collect();
    Err(format!("A5LANGUAGES:{}", list.join(";")).into())
}

/// The Secrets of Atlantis from its folder among the picked files (a
/// copy of the DVD, an install, the extracted cabinets), read when the
/// game reads it, the part it reads; in `language` or the page's `?lang=`
/// ([`a5_choose_language`]).
fn start_atlantis5(
    all: HashMap<String, File>,
    folder: &str,
    language: Option<&str>,
    code: Option<&str>,
) -> Result<(), JsValue> {
    use atlantis3_data::atlantis5::Atlantis5Source;
    let index = keep_picked(all);
    let dir = Arc::new(PickedDir::new(&index, folder));
    let mut src = Atlantis5Source::from_folder(dir).map_err(|e| a5_error(&e))?;
    a5_choose_language(&mut src, language, code)?;
    crate::a5_web::start(src, a5_store())
}

/// The Secrets of Atlantis from its DVD's image `image` among the picked
/// files, read a part at a time (never whole: 1.5 GB for the French DVD);
/// in `language` or the page's `?lang=` ([`a5_choose_language`]).
fn start_atlantis5_image(
    all: HashMap<String, File>,
    image: &str,
    language: Option<&str>,
    code: Option<&str>,
) -> Result<(), JsValue> {
    use atlantis3_data::atlantis5::{Atlantis5Source, Form};
    let index = keep_picked(all);
    let &(i, size) = index.get(image).ok_or("the DVD's image")?;
    let img = DiscImage::from_bytes(image, Box::new(PickedImage { index: i, size }))
        .ok_or("the DVD's image cannot be read")?;
    let mut src =
        Atlantis5Source::from_disc(Arc::new(img), Form::DiscImage).map_err(|e| a5_error(&e))?;
    a5_choose_language(&mut src, language, code)?;
    crate::a5_web::start(src, a5_store())
}

/// Atlantis Evolution's 2004 CD program from its installed folder among
/// the picked files (Atlantis III's engine, [`crate::a3_web`]), the saves
/// in the browser's storage. The install (about 4 GB, the four discs
/// copied by the CD's installer) stays in the player's files: each is read
/// when the engine reads it, the part it reads.
fn start_evolution_2004(all: HashMap<String, File>, install: &str) -> Result<(), JsValue> {
    let index = keep_picked(all);
    let store: Rc<dyn SaveStore> = match WebStore::new("atlantis-evolution-2004") {
        Some(s) => Rc::new(s),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    };
    crate::a3_web::start_evolution_2004(Arc::new(PickedDir::new(&index, install)), store)
}

/// Atlantis Evolution from its installed folder among the picked files
/// ([`crate::a4::web`]), the saves in the browser's storage. The install
/// (about 2 GB) stays in the player's files: each is read when the engine
/// reads it, the part it reads.
fn start_atlantis4(
    all: HashMap<String, File>,
    install: &str,
    language: Option<&str>,
) -> Result<(), JsValue> {
    let index = keep_picked(all);
    let dir = PickedDir::new(&index, install);
    let bytes: u64 = dir.files.values().map(|&(_, s)| s).sum();
    log::info!(
        "Atlantis Evolution: {} files, {} MB, read when needed",
        dir.files.len(),
        bytes >> 20
    );
    let store: Rc<dyn SaveStore> = match WebStore::new("atlantis4") {
        Some(s) => Rc::new(s),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    };
    // Read ahead in the background, so that the engine's reads seldom
    // wait for the browser.
    let dir = crate::a4_prefetch::Prefetch::new(dir);
    crate::a4::web::start(Arc::new(dir), store, language)
}

/// Atlantis II from the picked files: its installed folder and discs
/// ([`crate::web_layout::A2Layout`]), the saves in the browser's storage,
/// the player menu under the start-up intro, as the desktop starts it.
/// The Traditional Chinese CD's text font is fetched first, unless the
/// folder holds the player's `mingliu.ttc`.
async fn start_atlantis2(
    all: HashMap<String, File>,
    layout: crate::web_layout::A2Layout,
) -> Result<(), JsValue> {
    use atlantis2_data::files::{find_exe, OpenError};
    let help =
        |text: &str| -> JsValue { format!("{text}\n\nHelp: {}", crate::guide::HELP_URL_A2).into() };
    let index = keep_picked(all);
    let install = PickedDir::new(&index, &layout.install);
    let (exe_name, build, _) = find_exe(&install).map_err(|e| match e {
        OpenError::Exe { name, error } => help(&format!(
            "This folder holds an Atlantis II program, {name}, that Atlantis Native does not \
             know ({error}): an edition of the game that is not supported yet.\n\n\
             Supported: the French CD (1999), the CD with patch 1.01, the German CD (1999), \
             the Spanish CD (1999), the Swedish CD (1999), the Polish CD (1999), the \
             Czech CD (Bohemia Interactive, 2000), the Traditional Chinese and English CDs \
             (Interwise, 2000), the DreamCatcher CD (2000), GOG.com and the Italian Atlantis \
             Collection."
        )),
        e => help(&e.to_string()),
    })?;
    let discs = picked_sources(&index, &layout.discs);
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
    // The Traditional Chinese CD's texts are drawn in the system's Big5
    // face (`atlantis2_engine::subtitle::system_cjk_face`): the player's,
    // else the port's, fetched beside the page.
    let code_page = files.text_code_page();
    let player_font = atlantis_engine::subtitles::player_font(code_page)
        .is_some_and(|(name, _)| files.install().size(name).is_some());
    if let Some(name) =
        atlantis_engine::subtitles::cjk_font_file(code_page).filter(|_| !player_font)
    {
        let url = format!("fonts/{name}");
        match fetch_url(&url).await {
            Ok(data) => {
                if atlantis_engine::subtitles::provide_cjk_font(code_page, data) {
                    log::info!("text font: {url}");
                } else {
                    log::warn!("{url}: not a font");
                }
            }
            Err(e) => log::warn!("{url}: {e:?}"),
        }
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
