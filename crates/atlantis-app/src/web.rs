//! Web entry point. The page passes the game folder the player picked
//! (`<input type="file" webkitdirectory>`); the browser only reads files
//! asynchronously and a BIG archive is 600 MB, so the files are read by
//! ranges: the executable whole, the first MiB of each archive (its
//! directory is at most 205 KB), then the members the game needs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use atlantis_data::{FileSource, Game};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileList};
use winit::event_loop::EventLoop;
use winit::platform::web::EventLoopExtWebSys;

use crate::{open_viewer, App};

const ARCHIVE_HEAD: u64 = 1 << 20;

/// Fetched ranges: path -> (offset, bytes).
type Ranges = HashMap<String, Vec<(u64, Vec<u8>)>>;

/// Files of the picked folder; reads are served from ranges fetched before.
struct WebFiles {
    // lower-case path below the picked folder -> size
    sizes: HashMap<String, u64>,
    ranges: Mutex<Ranges>,
}

fn key(name: &str) -> String {
    name.replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

impl FileSource for WebFiles {
    fn size(&self, name: &str) -> Option<u64> {
        self.sizes.get(&key(name)).copied()
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let ranges = self.ranges.lock().unwrap();
        ranges.get(&key(name))?.iter().find_map(|(start, bytes)| {
            let from = usize::try_from(offset.checked_sub(*start)?).ok()?;
            bytes.get(from..from.checked_add(len)?).map(<[u8]>::to_vec)
        })
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

/// Called by the page with the picked folder and a panorama name.
#[wasm_bindgen]
pub async fn start(files: FileList, panorama: String) -> Result<(), JsValue> {
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
        ranges: Mutex::new(HashMap::new()),
    });

    let add = |name: &str, offset: u64, bytes: Vec<u8>| {
        web.ranges
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

    let game = Game::open(web.clone()).map_err(|e| e.to_string())?;
    let name = crate::panorama_name(&panorama);
    let (file, offset, size) = game.big.locate(&name).ok_or(format!("{name} not found"))?;
    let f = by_key
        .get(&key(&file))
        .ok_or(format!("{file} is not in the chosen folder"))?;
    add(&file, offset, fetch(f, offset, size as u64).await?);

    let viewer = open_viewer(&game, &panorama)?;
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.spawn_app(App::new(Some(viewer)));
    Ok(())
}
