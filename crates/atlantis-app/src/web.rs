//! Web entry point. The page passes the game folder the player picked
//! (`<input type="file" webkitdirectory>`); the browser only reads files
//! asynchronously and a BIG archive is 600 MB, so the files are read by
//! ranges: the executable whole and the first MiB of each archive (its
//! directory is at most 205 KB) at the start, then each member the engine
//! reads, fetched just before (the engine's [`Loader`]).
//!
//! The players, checkpoints and settings are kept in the browser's
//! `localStorage` ([`WebStore`]), one key per file. The original's
//! `players.atl` and `p<letter><chapter>.gam` of the picked folder, if
//! any, are read at the start, so that the engine takes them over as the
//! desktop build does.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use atlantis_data::{BigArchive, FileSource, Game};
use atlantis_engine::hooks::{HookFuture, Loader};
use atlantis_engine::{SaveStore, Start};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileList, Storage};
use winit::event_loop::EventLoop;
use winit::platform::web::EventLoopExtWebSys;

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
}

async fn fetch(file: &File, offset: u64, len: u64) -> Result<Vec<u8>, JsValue> {
    let blob = file.slice_with_f64_and_f64(offset as f64, (offset + len) as f64)?;
    let buf = JsFuture::from(blob.array_buffer()).await?;
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

/// Called by the page with the picked folder.
#[wasm_bindgen]
pub async fn start(files: FileList) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    // Paths are "<picked folder>/<path>"; drop the first component.
    let mut by_key: HashMap<String, File> = HashMap::new();
    for i in 0..files.length() {
        let Some(f) = files.get(i) else { continue };
        // webkitRelativePath is not in web-sys's stable API.
        let rel = js_sys::Reflect::get(&f, &"webkitRelativePath".into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| f.name());
        let rel = rel.split_once('/').map_or(rel.as_str(), |(_, r)| r);
        by_key.insert(key(rel), f);
    }
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
        .ok_or("atlantis.exe is not in the chosen folder")?;
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

    let game = Game::open(web.clone()).map_err(|e| e.to_string())?;
    let saves: Rc<dyn SaveStore> = match WebStore::new(game.exe.build.name) {
        Some(store) => Rc::new(store),
        None => {
            log::warn!("no localStorage: the saves last as long as the page");
            Rc::new(atlantis_engine::MemStore::new())
        }
    };
    let options = atlantis_engine::Options {
        loader: Rc::new(WebLoader { files: by_key, web }),
        ..options_with_settings(saves, false, Start::Boot)
    };
    let (sys, runner) = start_game(game, options);
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(App::new(sys, Some(runner)));
    Ok(())
}
