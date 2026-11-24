//! The Secrets of Atlantis copied from its DVD (a drive, a copy of its
//! files, or its image) to the computer, once, so that the game then plays
//! from the hard disk: the disc's cabinets and its program
//! ([`atlantis3_data::atlantis5::disc_files`], about 1.4 GB for the French
//! DVD) go to `atlantis5/disc` in the per-user data folder ([`copy_dir`]),
//! where they open as the DVD's files. The port's own: the original's
//! installer unpacks the cabinets instead.
//!
//! Each file is written under a temporary name (`.part`) and renamed once
//! whole, so a copy cut short (cancelled, an error, the program closed)
//! starts again from the files not yet copied. The free room is checked
//! first.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use atlantis3_data::atlantis5::{disc_files, Atlantis5Source};
use atlantis_data::FileSource;

/// The bytes read and written at once.
const CHUNK: usize = 8 << 20;
/// The room kept free beyond the copy's (the saves, the cabinets'
/// indexes, the system's own).
const MARGIN: u64 = 256 << 20;

/// Where the copy goes: `atlantis5/disc` in the per-user data folder.
pub fn copy_dir() -> PathBuf {
    crate::desktop::data_dir().join("atlantis5").join("disc")
}

/// Whether `path` is the copy's folder ([`copy_dir`]).
pub fn is_copy(path: &Path) -> bool {
    crate::library::same_folder(path, &copy_dir())
}

/// The copy's progress, shared with the thread doing it: the bytes copied
/// and to copy, and the player's cancel.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    /// The bytes copied and the bytes to copy.
    pub fn get(&self) -> (u64, u64) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// Why a copy did not end.
#[derive(Debug, PartialEq, Eq)]
pub enum CopyError {
    /// The data at the path are not the DVD's cabinets (an install, the
    /// extracted cabinets, or not the game).
    NotADisc,
    /// Not enough room: the bytes needed and free.
    NoRoom { need: u64, free: u64 },
    /// The player cancelled.
    Cancelled,
    /// A read or a write failed.
    Io(String),
}

impl std::fmt::Display for CopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyError::NotADisc => write!(f, "this is not the DVD or its image"),
            CopyError::NoRoom { need, free } => write!(
                f,
                "not enough room: {} MB needed, {} MB free",
                need.div_ceil(1 << 20),
                free >> 20
            ),
            CopyError::Cancelled => write!(f, "cancelled"),
            CopyError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CopyError {}

/// What a copy from a disc reads: its root and the files with their sizes.
pub struct Plan {
    root: Arc<dyn FileSource>,
    pub files: Vec<(String, u64)>,
}

impl Plan {
    /// The copy of the disc at `path` (a drive or a copy of the disc's
    /// files, its image, or a folder holding the image).
    pub fn of(path: &Path) -> Result<Plan, CopyError> {
        let root = Atlantis5Source::disc_root(path).ok_or(CopyError::NotADisc)?;
        Self::from_root(root)
    }

    /// The copy of the disc whose root is `root`.
    pub fn from_root(root: Arc<dyn FileSource>) -> Result<Plan, CopyError> {
        let files: Vec<(String, u64)> = disc_files(&*root)
            .into_iter()
            .filter_map(|n| {
                let size = root.size(&n)?;
                // The image lists its names in lower case: the disc's own.
                let name = if n.len() > 6 && n[..6].eq_ignore_ascii_case("datas_") {
                    format!("DATAS_{}", &n[6..])
                } else {
                    n
                };
                Some((name, size))
            })
            .collect();
        // No language's cabinet (`DATAS_<language>.cab`; the program is
        // not one): not the disc, refused before anything is copied.
        if !files.iter().any(|(n, _)| n.starts_with("DATAS_")) {
            return Err(CopyError::NotADisc);
        }
        Ok(Plan { root, files })
    }

    /// The files still to copy into `dest` (those missing there or of
    /// another size) and their bytes.
    pub fn still_to_copy(&self, dest: &Path) -> (Vec<(String, u64)>, u64) {
        let left: Vec<(String, u64)> = self
            .files
            .iter()
            .filter(|(n, size)| {
                std::fs::metadata(dest.join(n)).map_or(true, |m| !m.is_file() || m.len() != *size)
            })
            .cloned()
            .collect();
        let bytes = left.iter().map(|f| f.1).sum();
        (left, bytes)
    }

    /// Copies the disc's files into `dest` (created if needed), `progress`
    /// counting the bytes; files already whole there are kept, another
    /// language's cabinet and a copy cut short are removed first. Checks
    /// the room (`free`: the free bytes at `dest`, `None` when unknown)
    /// before writing. Ends with the copy opened as the game.
    pub fn copy_to(
        &self,
        dest: &Path,
        progress: &Progress,
        free: impl Fn(&Path) -> Option<u64>,
    ) -> Result<(), CopyError> {
        let io = |what: &Path, e: std::io::Error| CopyError::Io(format!("{}: {e}", what.display()));
        std::fs::create_dir_all(dest).map_err(|e| io(dest, e))?;
        // What a copy cut short left (part files) goes at once; another
        // disc's language cabinet only once this copy is whole, so an
        // earlier copy stays playable when this one stops (no room, Esc, a
        // read error), unless the room needs it gone first.
        let mut stale = Vec::new();
        for entry in std::fs::read_dir(dest).map_err(|e| io(dest, e))?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let l = name.to_ascii_lowercase();
            if l.ends_with(".part") {
                let _ = std::fs::remove_file(entry.path());
            } else if l.starts_with("datas")
                && l.ends_with(".cab")
                && !self
                    .files
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(&name))
            {
                let size = entry.metadata().map_or(0, |m| m.len());
                stale.push((entry.path(), size));
            }
        }
        let (left, need) = self.still_to_copy(dest);
        progress.done.store(0, Ordering::Relaxed);
        progress.total.store(need, Ordering::Relaxed);
        // The room only for bytes to write: a whole copy opens at once,
        // whatever is left free.
        if let Some(free) = free(dest).filter(|_| !left.is_empty()) {
            let freed: u64 = stale.iter().map(|s| s.1).sum();
            if free.saturating_add(freed) < need + MARGIN {
                return Err(CopyError::NoRoom {
                    need: need + MARGIN,
                    free,
                });
            }
            if free < need + MARGIN {
                log::info!("the other language's cabinets removed first for the room");
                for (path, _) in stale.drain(..) {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        for (name, size) in &left {
            let part = dest.join(format!("{name}.part"));
            let r = self.copy_file(name, *size, &part, progress);
            if let Err(e) = r {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
            std::fs::rename(&part, dest.join(name)).map_err(|e| io(&part, e))?;
            log::info!("{name}: copied to {}", dest.display());
        }
        for (path, _) in stale {
            let _ = std::fs::remove_file(path);
        }
        Atlantis5Source::open(dest)
            .map(|_| ())
            .map_err(|e| CopyError::Io(format!("{}: {e}", dest.display())))
    }

    fn copy_file(
        &self,
        name: &str,
        size: u64,
        part: &Path,
        progress: &Progress,
    ) -> Result<(), CopyError> {
        let io = |e: std::io::Error| CopyError::Io(format!("{}: {e}", part.display()));
        let mut out = std::fs::File::create(part).map_err(io)?;
        let mut at = 0u64;
        while at < size {
            if progress.cancel.load(Ordering::Relaxed) {
                return Err(CopyError::Cancelled);
            }
            let len = usize::try_from((size - at).min(CHUNK as u64)).unwrap_or(CHUNK);
            let data = self
                .root
                .read_at(name, at, len)
                .filter(|d| d.len() == len)
                .ok_or_else(|| {
                    CopyError::Io(format!("{name}: the disc cannot be read at byte {at}"))
                })?;
            out.write_all(&data).map_err(io)?;
            at += len as u64;
            progress.done.fetch_add(len as u64, Ordering::Relaxed);
        }
        out.sync_all().map_err(io)
    }
}

/// The free bytes on the volume of `dir` (an existing folder), when the
/// system says.
pub fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: a NUL-terminated path and a statvfs to fill.
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        #[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
        Some(u64::from(st.f_bavail as u64) * u64::from(st.f_frsize as u64))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
        let mut free = 0u64;
        // SAFETY: a NUL-terminated path; the other outputs may be null.
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(free)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = dir;
        None
    }
}

/// A copy running in its own thread.
pub struct Job {
    /// The data copied (the library's entry).
    pub from: PathBuf,
    pub dest: PathBuf,
    pub progress: Arc<Progress>,
    thread: Option<std::thread::JoinHandle<Result<(), CopyError>>>,
}

impl Job {
    /// Starts copying the disc at `from` into `dest`.
    pub fn start(from: &Path, dest: &Path) -> Result<Job, CopyError> {
        let plan = Plan::of(from)?;
        let progress = Arc::new(Progress::default());
        let (p, d) = (progress.clone(), dest.to_path_buf());
        let thread = std::thread::Builder::new()
            .name("atlantis5-copy".into())
            .spawn(move || plan.copy_to(&d, &p, free_space))
            .map_err(|e| CopyError::Io(e.to_string()))?;
        Ok(Job {
            from: from.to_path_buf(),
            dest: dest.to_path_buf(),
            progress,
            thread: Some(thread),
        })
    }

    pub fn cancel(&self) {
        self.progress.cancel.store(true, Ordering::Relaxed);
    }

    /// The copy's end, once it has ended.
    pub fn result(&mut self) -> Option<Result<(), CopyError>> {
        if !self.thread.as_ref()?.is_finished() {
            return None;
        }
        let t = self.thread.take()?;
        Some(
            t.join()
                .unwrap_or_else(|_| Err(CopyError::Io("the copy stopped".into()))),
        )
    }

    /// A line on the copy's progress for the game library's screen.
    pub fn status(&self) -> String {
        let (done, total) = self.progress.get();
        if self.progress.cancel.load(Ordering::Relaxed) {
            return "Cancelling the copy\u{2026}".to_string();
        }
        let pct = (done * 100).checked_div(total).unwrap_or(0);
        format!(
            "Copying to this computer: {pct}% ({} of {} MB) \u{b7} Esc cancels",
            done >> 20,
            total.div_ceil(1 << 20)
        )
    }
}

/// `--atlantis5-install <disc>`: copies the disc at `from` to [`copy_dir`]
/// with the progress on the standard output, then adds the copy to the
/// game library in place of the disc's entry (or at its end).
pub fn install_cli(from: &Path) -> Result<PathBuf, String> {
    let dest = copy_dir();
    let mut job = Job::start(from, &dest).map_err(|e| format!("{}: {e}", from.display()))?;
    println!("copying {} to {}", from.display(), dest.display());
    let t0 = std::time::Instant::now();
    let mut last = u64::MAX;
    let r = loop {
        if let Some(r) = job.result() {
            break r;
        }
        let (done, total) = job.progress.get();
        let pct = (done * 100).checked_div(total).unwrap_or(0);
        if total > 0 && (pct / 10 != last / 10 || last == u64::MAX) {
            println!("{pct}% ({} of {} MB)", done >> 20, total.div_ceil(1 << 20));
            last = pct;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    r.map_err(|e| format!("{}: {e}", from.display()))?;
    println!("copied in {:.1} s", t0.elapsed().as_secs_f64());
    let data = crate::desktop::data_dir();
    let mut lib = crate::library::Library::load(&data);
    move_entries_to_copy(&mut lib, from, &dest);
    lib.save(&data)
        .map_err(|e| format!("{}: {e}", data.display()))?;
    Ok(dest)
}

/// The game library `lib` once the disc at `from` (its root, or a
/// language's place of it, [`atlantis3_data::atlantis5::language_place`])
/// is copied to `copy`: every entry of that disc now names the copy (a
/// language's place the same language's place of the copy). With no
/// entry of it, the copy is added: a place per language when it has
/// several. Returns where `from`'s entry is now.
pub fn move_entries_to_copy(lib: &mut crate::library::Library, from: &Path, copy: &Path) -> usize {
    use crate::library::same_folder;
    use atlantis3_data::atlantis5::{language_place, language_places, place_language, place_root};
    let to = |f: &Path| match place_language(f) {
        Some(l) => language_place(copy, &l),
        None => copy.to_path_buf(),
    };
    let disc = place_root(from);
    let mine: Vec<PathBuf> = lib
        .folders()
        .iter()
        .filter(|f| same_folder(&place_root(f), &disc))
        .cloned()
        .collect();
    if mine.is_empty() {
        let places = language_places(copy);
        let added = if places.is_empty() {
            vec![to(from)]
        } else {
            places
        };
        for p in &added {
            lib.add(p);
        }
        let want = to(from);
        return lib
            .position(&want)
            .or_else(|| lib.position(&added[0]))
            .unwrap_or(0);
    }
    for f in mine {
        if let Some(i) = lib.position(&f) {
            lib.relocate(i, &to(&f));
        }
    }
    lib.position(&to(from)).unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("atlantis-a5copy-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A cabinet of `files` (name, data), stored, in blocks of 30000
    /// bytes (the layout of `atlantis_data::cab`'s own tests).
    pub(crate) fn stored_cab(files: &[(&str, &[u8])]) -> Vec<u8> {
        let all: Vec<u8> = files.iter().flat_map(|f| f.1.iter().copied()).collect();
        let mut entries = Vec::new();
        let mut off = 0u32;
        for (n, d) in files {
            entries.extend((d.len() as u32).to_le_bytes());
            entries.extend(off.to_le_bytes());
            entries.extend(0u16.to_le_bytes());
            entries.extend([0u8; 4]);
            entries.extend(0x20u16.to_le_bytes());
            entries.extend(n.as_bytes());
            entries.push(0);
            off += d.len() as u32;
        }
        let mut data = Vec::new();
        let mut blocks = 0u16;
        for chunk in all.chunks(30_000) {
            data.extend(0u32.to_le_bytes());
            data.extend((chunk.len() as u16).to_le_bytes());
            data.extend((chunk.len() as u16).to_le_bytes());
            data.extend(chunk);
            blocks += 1;
        }
        let files_at = 36 + 8;
        let data_at = files_at + entries.len();
        let mut c = b"MSCF".to_vec();
        c.extend(0u32.to_le_bytes());
        c.extend(((data_at + data.len()) as u32).to_le_bytes());
        c.extend(0u32.to_le_bytes());
        c.extend((files_at as u32).to_le_bytes());
        c.extend(0u32.to_le_bytes());
        c.extend([3, 1]);
        c.extend(1u16.to_le_bytes());
        c.extend((files.len() as u16).to_le_bytes());
        c.extend([0u8; 6]);
        c.extend((data_at as u32).to_le_bytes());
        c.extend(blocks.to_le_bytes());
        c.extend(0u16.to_le_bytes());
        c.extend(entries);
        c.extend(data);
        c
    }

    /// The bytes of the fake disc's large file.
    fn big() -> Vec<u8> {
        (0..3_000_000u32).map(|i| (i * 7 % 251) as u8).collect()
    }

    /// A disc's files in `dir`: the two cabinets (a warp and a scene; the
    /// French menu texts), the program and the installer. Returns the
    /// bytes the copy takes.
    pub(crate) fn fake_disc(dir: &Path) -> u64 {
        fake_disc_of(dir, &["french"])
    }

    /// [`fake_disc`] with a cabinet per language of `languages`
    /// (`datas_<language>.cab`, as the international DVD's).
    pub(crate) fn fake_disc_of(dir: &Path, languages: &[&str]) -> u64 {
        std::fs::create_dir_all(dir).unwrap();
        let main = stored_cab(&[
            (".\\Datas\\Scenes\\zeppelin.wam", b"wam"),
            (".\\Datas\\Warps\\ze0003.jp6", &big()),
        ]);
        let mut n = main.len() + 10;
        std::fs::write(dir.join("DATAS.cab"), main).unwrap();
        for l in languages {
            let name = format!(".\\Datas\\Texts\\Menu\\{l}\\string_resources.xml");
            let cab = stored_cab(&[(name.as_str(), b"<x/>")]);
            n += cab.len();
            std::fs::write(dir.join(format!("datas_{l}.cab")), cab).unwrap();
        }
        std::fs::write(dir.join("The secrets of Atlantis.exe"), [1; 10]).unwrap();
        std::fs::write(dir.join("Install.exe"), [2; 10]).unwrap();
        n as u64
    }

    #[test]
    fn copies_the_disc_files_and_goes_on_after_a_cut() {
        let d = tmp("copy");
        let (disc, dest) = (d.join("disc"), d.join("dest"));
        let total = fake_disc(&disc);
        let plan = Plan::of(&disc).unwrap();
        let sizes: Vec<(&str, u64)> = plan.files.iter().map(|f| (f.0.as_str(), f.1)).collect();
        assert_eq!(
            sizes.iter().map(|f| f.0).collect::<Vec<_>>(),
            [
                "DATAS.cab",
                "DATAS_french.cab",
                "The secrets of Atlantis.exe"
            ]
        );
        assert_eq!(sizes.iter().map(|f| f.1).sum::<u64>(), total);
        let main = sizes[0].1;
        // Not enough room: nothing written.
        let p = Progress::default();
        assert_eq!(
            plan.copy_to(&dest, &p, |_| Some(1 << 20)),
            Err(CopyError::NoRoom {
                need: total + MARGIN,
                free: 1 << 20
            })
        );
        assert!(!dest.join("DATAS.cab").exists());
        // Cancelled: no file left half written.
        let p = Progress::default();
        p.cancel.store(true, Ordering::Relaxed);
        assert_eq!(plan.copy_to(&dest, &p, |_| None), Err(CopyError::Cancelled));
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
        assert_eq!(plan.still_to_copy(&dest).1, total);
        // Another disc's copy there (its language cabinet), then a copy of
        // this one that stops: the earlier copy keeps its cabinet.
        std::fs::write(dest.join("DATAS_german.cab"), [0; 5]).unwrap();
        let p = Progress::default();
        assert!(matches!(
            plan.copy_to(&dest, &p, |_| Some(1 << 20)),
            Err(CopyError::NoRoom { .. })
        ));
        assert!(dest.join("DATAS_german.cab").is_file());
        let p = Progress::default();
        p.cancel.store(true, Ordering::Relaxed);
        assert_eq!(plan.copy_to(&dest, &p, |_| None), Err(CopyError::Cancelled));
        assert!(dest.join("DATAS_german.cab").is_file());
        // Room only once it is gone: removed first, then the copy.
        let tight = tmp("copy-tight");
        std::fs::write(tight.join("DATAS_german.cab"), vec![0; 1000]).unwrap();
        let p = Progress::default();
        assert_eq!(
            plan.copy_to(&tight, &p, |_| Some(total + MARGIN - 500)),
            Ok(())
        );
        assert!(!tight.join("DATAS_german.cab").exists());
        let _ = std::fs::remove_dir_all(&tight);
        // A copy cut short (a part file, another language's cabinet, the
        // program whole): the rest copied, the leftovers removed, the
        // player's other files kept.
        std::fs::write(dest.join("DATAS.cab.part"), [0; 5]).unwrap();
        std::fs::write(dest.join("DATAS_german.cab"), [0; 5]).unwrap();
        std::fs::write(dest.join("The secrets of Atlantis.exe"), [1; 10]).unwrap();
        std::fs::write(dest.join("notes.txt"), [0; 5]).unwrap();
        assert_eq!(plan.still_to_copy(&dest).1, total - 10);
        let p = Progress::default();
        assert_eq!(plan.copy_to(&dest, &p, |_| Some(u64::MAX)), Ok(()));
        assert_eq!(p.get(), (total - 10, total - 10));
        let mut names: Vec<String> = std::fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "DATAS.cab",
                "DATAS_french.cab",
                "The secrets of Atlantis.exe",
                "notes.txt"
            ]
        );
        assert_eq!(
            std::fs::metadata(dest.join("DATAS.cab")).unwrap().len(),
            main
        );
        assert_eq!(plan.still_to_copy(&dest), (Vec::new(), 0));
        // Copied again with no room left: nothing to write, no room asked.
        let p = Progress::default();
        assert_eq!(plan.copy_to(&dest, &p, |_| Some(0)), Ok(()));
        // The copy opens as the disc's files, with the same data.
        let src = Atlantis5Source::open(&dest).unwrap();
        assert_eq!(src.form(), atlantis3_data::atlantis5::Form::DiscFolder);
        assert_eq!(src.languages(), ["french"]);
        assert_eq!(src.read("datas/warps/ze0003.jp6"), Some(big()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn only_a_disc_is_copied() {
        let d = tmp("install");
        assert!(matches!(Plan::of(&d), Err(CopyError::NotADisc)));
        // The extracted cabinets (an install's Datas): already on the disk.
        std::fs::create_dir_all(d.join("Datas/Scenes")).unwrap();
        std::fs::create_dir_all(d.join("Datas/Warps")).unwrap();
        assert!(matches!(Plan::of(&d), Err(CopyError::NotADisc)));
        let mut m = atlantis_data::files::MemFiles::new();
        m.insert("DATAS.cab", vec![0; 4]);
        assert!(matches!(
            Plan::from_root(Arc::new(m)),
            Err(CopyError::NotADisc)
        ));
        // DATAS.cab and the program, no language's cabinet: still not the
        // disc (the program is not a language).
        let mut m = atlantis_data::files::MemFiles::new();
        m.insert("DATAS.cab", vec![0; 4]);
        m.insert("The secrets of Atlantis.exe", vec![1; 10]);
        assert!(matches!(
            Plan::from_root(Arc::new(m)),
            Err(CopyError::NotADisc)
        ));
        assert!(free_space(&d).is_some_and(|f| f > 0) || cfg!(not(any(unix, windows))));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The French DVD's image (`ATLANTIS5_DVD_FR`): its plan.
    #[test]
    fn the_french_dvd_plan() {
        let Some(root) = std::env::var_os("ATLANTIS5_DVD_FR").map(PathBuf::from) else {
            eprintln!("SKIPPED: ATLANTIS5_DVD_FR is not set");
            return;
        };
        for p in [root.clone(), root.join("files")] {
            let plan = Plan::of(&p).unwrap();
            let names: Vec<&str> = plan.files.iter().map(|f| f.0.as_str()).collect();
            assert_eq!(
                names,
                [
                    "DATAS.cab",
                    "DATAS_french.cab",
                    "The secrets of Atlantis.exe"
                ],
                "{}",
                p.display()
            );
        }
        assert!(matches!(
            Plan::of(&root.join("datas")),
            Err(CopyError::NotADisc)
        ));
    }

    /// The international DVD (`ATLANTIS5_DVD_INT`): its plan, from the
    /// disc's root and from a language's place (the same disc).
    #[test]
    fn the_international_dvd_plan() {
        let Some(root) = atlantis3_data::testing::a5_dvd_int() else {
            return;
        };
        for p in [root.clone(), root.join("datas_german.cab")] {
            let plan = Plan::of(&p).unwrap();
            let names: Vec<&str> = plan.files.iter().map(|f| f.0.as_str()).collect();
            assert_eq!(
                names,
                [
                    "DATAS.cab",
                    "DATAS_english.cab",
                    "DATAS_german.cab",
                    "DATAS_italian.cab",
                    "DATAS_spanish.cab",
                    "The secrets of Atlantis.exe"
                ],
                "{}",
                p.display()
            );
            let bytes: u64 = plan.files.iter().map(|f| f.1).sum();
            assert_eq!(bytes, 1_709_151_146 + 3_883_008);
        }
    }

    /// A disc of two languages copied: the copy opens in both, each from
    /// its place.
    #[test]
    fn a_disc_of_two_languages() {
        let d = tmp("two");
        let disc = d.join("dvd");
        fake_disc_of(&disc, &["english", "german"]);
        let dest = d.join("copy");
        let plan = Plan::of(&disc.join("datas_german.cab")).unwrap();
        plan.copy_to(&dest, &Progress::default(), |_| Some(u64::MAX))
            .unwrap();
        let places = atlantis3_data::atlantis5::language_places(&dest);
        assert_eq!(
            places,
            [
                dest.join("DATAS_english.cab"),
                dest.join("DATAS_german.cab")
            ]
        );
        let src = Atlantis5Source::open(&places[1]).unwrap();
        assert_eq!(src.chosen_language(), Some("german"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
