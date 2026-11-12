//! Atlantis Evolution in the browser: the install's files read ahead.
//!
//! The engine reads its files synchronously, as the desktop's does; in a
//! page a picked file can only be read at once by a synchronous request
//! ([`crate::web`]'s `PickedDir`), which holds the page's thread: about
//! 12 ms per MB, and 120 to 330 ms for a file's first read in Edge on
//! Windows, however small. Meanwhile neither the picture nor the sound's
//! output is fed (a 30 MB movie: 0.9 s of silence).
//!
//! [`Prefetch`] keeps the engine synchronous and puts a cache before the
//! picked folder: a file the browser has already read in the background
//! (`Blob.arrayBuffer`) is given at once, any other one is read at once as
//! before. What is read ahead comes from the scene being entered, the
//! moment the engine reads its description or script
//! (`3D/<zone>/<node>.xml` / `.lua`):
//!
//! 1. the files its script and description name: sounds, music, movies
//!    with their sound, voices, the dialog lines' talking heads and
//!    stills ([`candidates`]);
//! 2. the scenes it names (its exits, its `ChangeWarp` targets): their
//!    description, script and warp packages;
//! 3. the files those scenes' scripts name (a movie played on entering
//!    one).
//!
//! At the start, the first scene (`Texts/Full/FirstWarpPath.lua`) and
//! the scenes of the saves kept in the browser are read ahead the same
//! way, so that a new game's intro or a continued game's scene is there.
//!
//! The cache holds at most [`CAP`] bytes, the least recently used file
//! leaving first; a scene change drops what was still waiting to be read
//! for the scene before. Nothing of what the engine reads changes: the
//! same bytes, sooner.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use atlantis4_data::files::{WarpPath, PLATFORM_DIR};
use atlantis_data::FileSource;

use crate::web::{key, read_picked_async, PickedDir};

/// The cache's size limit.
pub const CAP: u64 = 256 << 20;
/// At most this much is queued after one scene change.
const SCENE_BUDGET: u64 = 160 << 20;
/// A larger file is never read ahead (the largest movie is 41 MB).
const MAX_FILE: u64 = 64 << 20;
/// A file read at once larger than this is not kept (the engine keeps
/// what it reads; a read again of a small one is common).
const KEEP_READ: u64 = 8 << 20;
/// A file given whole and larger than this leaves the cache (the engine
/// keeps it: no copy).
const TAKE: u64 = 8 << 20;
/// Background reads at a time.
const IN_FLIGHT: usize = 3;
/// A background read is made in parts of this size (each part's copy into
/// the program's memory then holds the page a few ms at most).
const PART: u64 = 4 << 20;
/// The candidates' language variants that are tried besides the game's.
const DEFAULT_LANGUAGE: &str = "en";
/// The interface's folders (menus, inventory, documents, dialog bar,
/// fonts, sound effects), read ahead once at the start after everything
/// else: their files are read as the player opens a menu or a document,
/// each a first read of a small file.
const INTERFACE: [&str; 10] = [
    "gui",
    "common",
    "inventory",
    "inventorymenu",
    "diary",
    "objectives",
    "puzzles",
    "2d",
    "sounds/sfx",
    "documentsbrowser",
];
/// The interface's files larger than this are left to the engine's read.
const INTERFACE_FILE: u64 = 2 << 20;
/// The language folders of the install (of the other languages, the
/// interface's files are not read ahead).
const LANGUAGES: [&str; 6] = ["de", "en", "es", "fr", "it", "ru"];
/// How far the sound is mixed ahead when a scene starts loading (its
/// description read): the scene's pictures decoded, its script run.
const SCENE_COVER: f64 = 0.45;
/// The same for a movie (the whole file demultiplexed when it opens):
/// this, plus [`MOVIE_COVER_PER_MB`] per MB.
const MOVIE_COVER: f64 = 0.15;
const MOVIE_COVER_PER_MB: f64 = 0.012;
/// A read at once of a larger file mixes the sound ahead for its time
/// (about 12 ms per MB in Edge, [`READ_COVER_PER_MB`], and
/// [`READ_COVER`]).
const READ_COVER_FROM: usize = 256 << 10;
const READ_COVER: f64 = 0.1;
const READ_COVER_PER_MB: f64 = 0.025;

/// The picked install with the read-ahead cache before it.
pub struct Prefetch {
    dir: Arc<PickedDir>,
}

impl Prefetch {
    pub fn new(dir: PickedDir) -> Prefetch {
        reserve_memory();
        let dir = Arc::new(dir);
        STATE.with(|s| s.borrow_mut().dir = Some(dir.clone()));
        Prefetch { dir }
    }
}

/// The program's memory at the start: the cache and the game.
const RESERVE: usize = (CAP as usize) + (192 << 20);

/// The program's memory grown at once to [`RESERVE`]. The allocator takes
/// memory from the browser in small steps, each a `memory.grow` of the
/// page that costs about a millisecond in Edge: opening a 30 MB movie
/// (its packets copied) took 0.5 s of them. What it frees it keeps, so a
/// block allocated and freed here is the room later allocations take.
fn reserve_memory() {
    let block: Vec<u8> = Vec::with_capacity(RESERVE);
    std::hint::black_box(&block);
}

/// The program's memory (MB).
fn memory_mb() -> u64 {
    use wasm_bindgen::JsCast;
    wasm_bindgen::memory()
        .dyn_into::<js_sys::WebAssembly::Memory>()
        .map_or(0, |m| {
            m.buffer()
                .dyn_into::<js_sys::ArrayBuffer>()
                .map_or(0, |b| u64::from(b.byte_length()) >> 20)
        })
}

/// A file read ahead.
struct Entry {
    data: Vec<u8>,
    /// The last use (a tick of [`State::tick`]).
    used: u64,
    /// Read ahead (not kept after a read at once).
    ahead: bool,
    /// Read by the engine since.
    hit: bool,
}

/// A file to read ahead.
struct Job {
    key: String,
    index: usize,
    size: u64,
    /// 0: named by the scene entered; 1: a scene it names; 2: named by
    /// such a scene's script.
    level: u8,
    /// A scene script: what it names is read ahead too (level 2).
    scan: bool,
    /// An interface file: kept queued across scene changes.
    keep: bool,
}

#[derive(Default)]
struct Stats {
    /// The engine's reads given from the cache, and their bytes.
    hits: u64,
    hit_bytes: u64,
    /// The engine's reads made at once, their bytes and time.
    misses: u64,
    miss_bytes: u64,
    miss_ms: f64,
    /// Files read ahead, and their bytes.
    ahead: u64,
    ahead_bytes: u64,
    /// Files read ahead that left the cache unused.
    wasted: u64,
    /// The last misses (name, bytes, ms), for the test runs.
    last: Vec<(String, u64, f64)>,
}

#[derive(Default)]
struct State {
    dir: Option<Arc<PickedDir>>,
    cache: HashMap<String, Entry>,
    bytes: u64,
    peak: u64,
    tick: u64,
    queue: Vec<Job>,
    queued: HashSet<String>,
    in_flight: HashSet<String>,
    /// The bytes queued since the last scene change.
    budget_used: u64,
    /// The scene entered last (`3d/<zone>/<node>`, lower case), and the
    /// scenes queued since.
    scene: String,
    scenes: HashSet<String>,
    language: String,
    stats: Stats,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

impl FileSource for Prefetch {
    fn size(&self, name: &str) -> Option<u64> {
        self.dir.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let k = key(name);
        let end = offset.checked_add(len as u64)?;
        let cached = STATE.with(|s| s.borrow_mut().take(&k, offset, end));
        let data = match cached {
            Some(d) => d,
            None => {
                if len >= READ_COVER_FROM {
                    let mb = len as f64 / f64::from(1 << 20);
                    crate::audio_out::web_out::cover(READ_COVER + mb * READ_COVER_PER_MB);
                }
                let t = web_time::Instant::now();
                let data = self.dir.read_at(name, offset, len)?;
                let ms = t.elapsed().as_secs_f64() * 1e3;
                STATE.with(|s| s.borrow_mut().missed(&k, offset, &data, ms));
                data
            }
        };
        if let Some((scene, lua)) = scene_file(&k) {
            if !lua {
                crate::audio_out::web_out::cover(SCENE_COVER);
            }
            entered(&scene, lua, &data);
        } else if k.ends_with(".ogv") && data.len() >= 1 << 20 {
            let mb = data.len() as f64 / f64::from(1 << 20);
            crate::audio_out::web_out::cover(MOVIE_COVER + mb * MOVIE_COVER_PER_MB);
        }
        Some(data)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.dir.has_dir(name)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.dir.list(dir)
    }
}

impl State {
    /// Bytes `offset..end` of `k` from the cache.
    fn take(&mut self, k: &str, offset: u64, end: u64) -> Option<Vec<u8>> {
        self.tick += 1;
        let tick = self.tick;
        let e = self.cache.get_mut(k)?;
        let size = e.data.len() as u64;
        if end > size {
            return None;
        }
        e.used = tick;
        e.hit = true;
        self.stats.hits += 1;
        self.stats.hit_bytes += end - offset;
        if offset == 0 && end == size && size > TAKE {
            let e = self.cache.remove(k)?;
            self.bytes -= size;
            return Some(e.data);
        }
        Some(e.data[offset as usize..end as usize].to_vec())
    }

    /// A read made at once: counted, and kept when it is a whole small
    /// file.
    fn missed(&mut self, k: &str, offset: u64, data: &[u8], ms: f64) {
        self.stats.misses += 1;
        self.stats.miss_bytes += data.len() as u64;
        self.stats.miss_ms += ms;
        if ms > 20.0 {
            log::info!(
                "read at once: {k}, {} bytes in {ms:.0} ms{}",
                data.len(),
                if self.in_flight.contains(k) {
                    " (being read ahead)"
                } else {
                    ""
                }
            );
        }
        self.stats.last.push((k.to_string(), data.len() as u64, ms));
        let n = self.stats.last.len();
        if n > 8 {
            self.stats.last.drain(..n - 8);
        }
        let whole = self.dir.as_ref().and_then(|d| d.entry(k)).map(|(_, s)| s);
        if offset == 0 && whole == Some(data.len() as u64) && whole <= Some(KEEP_READ) {
            self.insert(k, data.to_vec(), false);
        }
    }

    /// `data` kept as `k`, the least recently used files leaving to make
    /// room.
    fn insert(&mut self, k: &str, data: Vec<u8>, ahead: bool) {
        let size = data.len() as u64;
        if let Some(old) = self.cache.remove(k) {
            self.bytes -= old.data.len() as u64;
        }
        while self.bytes + size > CAP {
            let Some(oldest) = self
                .cache
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(e) = self.cache.remove(&oldest) {
                self.bytes -= e.data.len() as u64;
                if e.ahead && !e.hit {
                    self.stats.wasted += 1;
                }
            }
        }
        self.tick += 1;
        self.cache.insert(
            k.to_string(),
            Entry {
                data,
                used: self.tick,
                ahead,
                hit: false,
            },
        );
        self.bytes += size;
        self.peak = self.peak.max(self.bytes);
    }

    /// `k` (an existing file of the install) queued, unless it is there,
    /// on its way, too large, or past the scene's budget.
    fn queue_file(&mut self, k: &str, level: u8, scan: bool) {
        if self.cache.contains_key(k) || self.in_flight.contains(k) || self.queued.contains(k) {
            return;
        }
        let Some((index, size)) = self.dir.as_ref().and_then(|d| d.entry(k)) else {
            return;
        };
        if size > MAX_FILE || self.budget_used + size > SCENE_BUDGET {
            return;
        }
        self.budget_used += size;
        self.queued.insert(k.to_string());
        self.queue.push(Job {
            key: k.to_string(),
            index,
            size,
            level,
            scan,
            keep: false,
        });
    }

    /// Scene `stem` (`3d/<zone>/<node>`) queued: its description, its
    /// script (read for what it names when `scan`) and its packages.
    fn queue_scene(&mut self, stem: &str, level: u8, scan: bool) {
        if !self.scenes.insert(stem.to_string()) {
            return;
        }
        self.queue_file(&format!("{stem}.xml"), level, false);
        self.queue_file(&format!("{stem}.lua"), level, scan);
        let packages = format!("{stem}/{}", PLATFORM_DIR.to_ascii_lowercase());
        let names = self
            .dir
            .as_ref()
            .map(|d| d.list(&packages))
            .unwrap_or_default();
        for n in names {
            self.queue_file(&format!("{packages}/{n}"), level, false);
        }
    }

    /// What `text` (a script or a description) names, queued: files at
    /// `level`, scenes at `level + 1` (when `scenes`).
    fn queue_named(&mut self, text: &[u8], lua: bool, level: u8, scenes: bool) {
        let language = self.language.clone();
        for s in strings(text, lua) {
            match warp_stem(&s) {
                Some(stem) => {
                    if scenes {
                        self.queue_scene(&stem, level + 1, true);
                    }
                }
                None => {
                    let dir = self.dir.clone();
                    let exists = |p: &str| dir.as_ref().is_some_and(|d| d.entry(p).is_some());
                    for c in candidates(&s, &language, exists) {
                        self.queue_file(&c, level, false);
                    }
                }
            }
        }
    }

    /// The next job: the lowest level first, then the smallest file.
    fn next_job(&mut self) -> Option<Job> {
        let i = (0..self.queue.len()).min_by_key(|&i| (self.queue[i].level, self.queue[i].size))?;
        let job = self.queue.swap_remove(i);
        self.queued.remove(&job.key);
        Some(job)
    }
}

/// The scene a file describes (`3d/<zone>/<node>.xml` or `.lua`, lower
/// case): its stem, and whether it is the script.
fn scene_file(k: &str) -> Option<(String, bool)> {
    let (stem, lua) = if let Some(s) = k.strip_suffix(".lua") {
        (s, true)
    } else {
        (k.strip_suffix(".xml")?, false)
    };
    let mut parts = stem.split('/');
    let ok = parts.next() == Some("3d")
        && parts.next().is_some_and(|z| !z.is_empty())
        && parts.next().is_some_and(|n| !n.is_empty())
        && parts.next().is_none();
    ok.then(|| (stem.to_string(), lua))
}

/// The scene a string names (`3D\Zone\0001.wp`, `Zone\0001.wp`): its
/// stem, lower case.
fn warp_stem(s: &str) -> Option<String> {
    if !s.to_ascii_lowercase().ends_with(".wp") {
        return None;
    }
    WarpPath::parse(s).map(|w| key(&w.stem()))
}

/// Scene `stem` entered: its script or description `data` read by the
/// engine. A new scene drops the queue of the one before.
fn entered(stem: &str, lua: bool, data: &[u8]) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.scene != stem {
            s.scene = stem.to_string();
            s.queue.retain(|j| j.keep);
            let queued = s.queue.iter().map(|j| j.key.clone()).collect();
            s.queued = queued;
            s.scenes.clear();
            s.scenes.insert(stem.to_string());
            s.budget_used = 0;
        }
        s.queue_named(data, lua, 0, true);
    });
    pump();
}

/// The language whose voices and localised files are read ahead.
pub fn set_language(language: &str) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.language != language {
            s.language = language.to_string();
        }
    });
}

/// What `texts` name (the first scene's script; the saves: their scene
/// and music) read ahead, the scenes as when a scene naming them is
/// entered.
pub fn seed(texts: &[(Vec<u8>, bool)]) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        for (text, lua) in texts {
            s.queue_named(text, *lua, 1, true);
        }
    });
    pump();
}

/// The interface's files ([`INTERFACE`]) read ahead after everything
/// else, those of the game's language only.
pub fn seed_interface() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(dir) = s.dir.clone() else { return };
        let language = s.language.clone();
        let other_language = |k: &str| {
            k.split('/')
                .any(|p| p != language && LANGUAGES.contains(&p))
        };
        let mut jobs: Vec<Job> = dir
            .entries()
            .filter(|(k, &(_, size))| {
                size <= INTERFACE_FILE
                    && INTERFACE
                        .iter()
                        .any(|f| k.strip_prefix(f).is_some_and(|r| r.starts_with('/')))
                    && !other_language(k)
            })
            .map(|(k, &(index, size))| Job {
                key: k.clone(),
                index,
                size,
                level: 3,
                scan: false,
                keep: true,
            })
            .collect();
        jobs.retain(|j| !s.cache.contains_key(&j.key) && !s.queued.contains(&j.key));
        for j in &jobs {
            s.queued.insert(j.key.clone());
        }
        s.queue.extend(jobs);
    });
    pump();
}

/// Background reads started while there are jobs and room.
fn pump() {
    loop {
        let job = STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.in_flight.len() >= IN_FLIGHT {
                return None;
            }
            let job = s.next_job()?;
            s.in_flight.insert(job.key.clone());
            Some(job)
        });
        let Some(job) = job else { return };
        wasm_bindgen_futures::spawn_local(read_ahead(job));
    }
}

async fn read_ahead(job: Job) {
    let mut data = Vec::with_capacity(job.size as usize);
    let mut ok = true;
    while (data.len() as u64) < job.size {
        let at = data.len() as u64;
        let n = (job.size - at).min(PART);
        match read_picked_async(job.index, at, n).await {
            Some(part) if part.len() as u64 == n => data.extend_from_slice(&part),
            _ => {
                ok = false;
                break;
            }
        }
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.in_flight.remove(&job.key);
        if !ok {
            log::warn!("{}: not read ahead", job.key);
            return;
        }
        s.stats.ahead += 1;
        s.stats.ahead_bytes += job.size;
        if job.scan && s.scenes.contains(&job.key[..job.key.len() - 4]) {
            s.queue_named(&data, true, 2, false);
        }
        s.insert(&job.key, data, true);
    });
    pump();
}

/// The read-ahead's counters for the test runs (JSON).
pub fn report() -> String {
    STATE.with(|s| {
        let s = s.borrow();
        let st = &s.stats;
        let last: Vec<String> = st
            .last
            .iter()
            .map(|(k, n, ms)| format!("[\"{}\",{n},{ms:.0}]", k.replace(['"', '\\'], "")))
            .collect();
        format!(
            "{{\"hits\":{},\"hitBytes\":{},\"misses\":{},\"missBytes\":{},\"missMs\":{:.0},\
             \"ahead\":{},\"aheadBytes\":{},\"wasted\":{},\"bytes\":{},\"peak\":{},\
             \"inFlight\":{},\"queued\":{},\"memoryMB\":{},\"lastMisses\":[{}]}}",
            st.hits,
            st.hit_bytes,
            st.misses,
            st.miss_bytes,
            st.miss_ms,
            st.ahead,
            st.ahead_bytes,
            st.wasted,
            s.bytes,
            s.peak,
            s.in_flight.len(),
            s.queue.len(),
            memory_mb(),
            last.join(",")
        )
    })
}

/// The quoted strings of `text`, with a script's comments left out (a
/// scene script keeps old lines commented out: `-- PlayMovie(...)`).
fn strings(text: &[u8], lua: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i];
        if lua && c == b'-' && text.get(i + 1) == Some(&b'-') {
            if text[i + 2..].starts_with(b"[[") {
                i = find(text, i + 4, b"]]").map_or(text.len(), |j| j + 2);
            } else {
                i = find(text, i, b"\n").map_or(text.len(), |j| j + 1);
            }
            continue;
        }
        if !lua && text[i..].starts_with(b"<!--") {
            i = find(text, i + 4, b"-->").map_or(text.len(), |j| j + 3);
            continue;
        }
        if c == b'"' || (lua && c == b'\'') {
            let mut s = Vec::new();
            let mut j = i + 1;
            while j < text.len() && text[j] != c && text[j] != b'\n' {
                if lua && text[j] == b'\\' && j + 1 < text.len() {
                    j += 1;
                }
                s.push(text[j]);
                j += 1;
            }
            if s.len() <= 160 && s.iter().all(|&b| b.is_ascii_graphic()) && !s.is_empty() {
                out.push(String::from_utf8_lossy(&s).into_owned());
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

fn find(text: &[u8], from: usize, what: &[u8]) -> Option<usize> {
    text.get(from..)?
        .windows(what.len())
        .position(|w| w == what)
        .map(|p| p + from)
}

/// The files of the install a string of a script may name (lower case),
/// those for which `exists`: the path as written and its language and
/// platform variants (as the engine resolves sounds and movies), a
/// voice's `Sounds/Dialogs/` file, a movie's alpha, a dialog line's
/// talking head (`Videos/Dialogs/<v>.ogv`) or still (`2D/Dialogs/<v>`,
/// `2D/<v>`).
fn candidates(s: &str, language: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let s = s.replace('\\', "/");
    let s = s.trim_matches('/');
    let (dir, file) = s.rsplit_once('/').unwrap_or(("", s));
    let mut all: Vec<String> = Vec::new();
    let languages = [language, DEFAULT_LANGUAGE];
    if file.contains('.') {
        let dirs: Vec<String> = if dir.is_empty() {
            let mut d = vec![String::new(), "sounds/dialogs".to_string()];
            d.extend(languages.iter().map(|l| format!("sounds/dialogs/{l}")));
            d
        } else {
            let mut d = vec![dir.to_string()];
            d.extend(languages.iter().map(|l| format!("{dir}/{l}")));
            d.extend(["pc-macosx", "hd"].iter().map(|v| format!("{dir}/{v}")));
            d
        };
        for d in dirs {
            all.push(if d.is_empty() {
                file.to_string()
            } else {
                format!("{d}/{file}")
            });
        }
    } else if !file.is_empty()
        && dir.is_empty()
        && file.len() <= 32
        && file.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        for d in ["videos/dialogs", "videos/dialogs/pc-macosx"] {
            all.push(format!("{d}/{file}.ogv"));
        }
        for d in ["2d/dialogs", "2d", "2d/hd"] {
            for e in ["png", "jpg"] {
                all.push(format!("{d}/{file}.{e}"));
            }
        }
    }
    let mut out = Vec::new();
    for p in all {
        let p = key(&p);
        if let Some(stem) = p.strip_suffix(".ogv") {
            let alpha = format!("{stem}.alpha.ogv");
            if exists(&alpha) && !out.contains(&alpha) {
                out.push(alpha);
            }
        }
        if exists(&p) && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn a_scripts_strings_without_its_comments() {
        let lua = b"-- PlayMovie(\"Videos/a.ogv\")\r\n\tPlaySound(\"Sounds/b.ogg\",-1); -- \"c\"\n\
            --[[ \"d\" ]] x = '3D\\\\Zone\\\\0001.wp'\n";
        assert_eq!(strings(lua, true), ["Sounds/b.ogg", "3D\\Zone\\0001.wp"]);
        let xml =
            b"<!-- <exit linkedWarp=\"Z\\0002.wp\"> --><exit linkedWarp=\"Z\\0003.wp\" n=\"16\">";
        assert_eq!(strings(xml, false), ["Z\\0003.wp", "16"]);
    }

    #[test]
    fn the_scenes_named() {
        assert_eq!(
            warp_stem("3D\\Centre17_C1\\0002.wp").as_deref(),
            Some("3d/centre17_c1/0002")
        );
        assert_eq!(
            warp_stem("Bateau_CO\\0003.wp").as_deref(),
            Some("3d/bateau_co/0003")
        );
        assert_eq!(warp_stem("Rame"), None);
        assert_eq!(
            scene_file("3d/bateau_ca/0001.lua"),
            Some(("3d/bateau_ca/0001".to_string(), true))
        );
        assert_eq!(scene_file("3d/ongameenter.lua"), None);
        assert_eq!(
            scene_file("3d/bateau_ca/0001/pc-macosx/background.wpa"),
            None
        );
    }

    #[test]
    fn the_files_a_string_names() {
        let files = [
            "sounds/dialogs/fr/at002201.ogg",
            "sounds/dialogs/en/at002201.ogg",
            "videos/pc-macosx/intro.ogv",
            "videos/intro.ogg",
            "videos/dialogs/bb0002s.ogv",
            "videos/dialogs/bb0002s.alpha.ogv",
            "2d/g1c1000001.png",
            "sounds/musiques/muzik20.ogg",
        ];
        let exists = |p: &str| files.contains(&p);
        assert_eq!(
            candidates("AT002201.ogg", "fr", exists),
            [
                "sounds/dialogs/fr/at002201.ogg",
                "sounds/dialogs/en/at002201.ogg"
            ]
        );
        assert_eq!(
            candidates("Videos/intro.ogv", "fr", exists),
            ["videos/pc-macosx/intro.ogv"]
        );
        assert_eq!(
            candidates("Videos/intro.ogg", "fr", exists),
            ["videos/intro.ogg"]
        );
        assert_eq!(
            candidates("bb0002S", "fr", exists),
            [
                "videos/dialogs/bb0002s.alpha.ogv",
                "videos/dialogs/bb0002s.ogv"
            ]
        );
        assert_eq!(
            candidates("g1c1000001", "fr", exists),
            ["2d/g1c1000001.png"]
        );
        assert_eq!(
            candidates("Sounds/Musiques/muzik20.ogg", "fr", exists),
            ["sounds/musiques/muzik20.ogg"]
        );
        assert!(candidates("C1_Garde_Dialogue", "fr", exists).is_empty());
    }
}
