//! Finding the player's copy of the game: the installations found on the
//! computer, and what a chosen folder holds when it is not a game folder
//! (so that the message says what to do), as other engines that need the
//! original files do (OpenMW's wizard, OpenRA's content detection,
//! devilutionX's GOG detection).

use std::path::{Path, PathBuf};

use std::sync::Arc;

use atlantis2_data::files::OpenError;
use atlantis_data::GameError;

pub use crate::guide::{HELP_URL, HELP_URL_A2};

/// The GOG.com product ids of the games (their registry keys on Windows):
/// the first game's, then Atlantis II's.
const GOG_IDS: [&str; 2] = ["1207658944", "1207658951"];

/// Which of the two games a folder holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKind {
    /// Atlantis: The Lost Tales (`atlantis.exe`, or the images of its CDs).
    LostTales,
    /// Atlantis II (`atlantis2.exe`, or GOG.com's `Game.exe`:
    /// [`atlantis2_data::files::holds_atlantis2`]).
    Atlantis2,
}

impl GameKind {
    /// The game's name, for people.
    pub fn name(self) -> &'static str {
        match self {
            GameKind::LostTales => "Atlantis: The Lost Tales",
            GameKind::Atlantis2 => "Atlantis II",
        }
    }
}

/// The first-start explanation.
pub const WELCOME: &str = "Atlantis Native is a new engine for Atlantis: The Lost Tales \
and Atlantis II. It does not include the games themselves: the pictures, videos, voices \
and music belong to the games' publisher, so Atlantis Native plays them from your own copy.\n\n\
Choose the folder of your game:\n\
\u{2022} GOG.com: the folder where the game is installed \
(for example C:\\GOG Games\\Atlantis The Lost Tales, or the Atlantis 2 folder of GOG Galaxy).\n\
\u{2022} Atlantis: The Lost Tales on CD: a folder holding atlantis.exe and BIGCD1.BIG to \
BIGCD4.BIG, copied from the four discs (each disc holds one BIGCD file), or the folder of \
the CD images (.iso, .bin). With a disc in the drive, Atlantis Native offers to copy the \
four discs itself.\n\
\u{2022} Atlantis II on CD: the folder where the game is installed (the one with \
atlantis2.exe), with the images of its four CDs (or copies of the CDs) in it or in a \
folder inside it.\n\n\
Atlantis Native only reads these files and never changes them.";

/// Case-insensitive lookup of a file in a folder.
fn has_file(dir: &Path, name: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_file()
        })
    })
}

/// The names of the files at the top of `dir` (lower case).
pub fn names_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// Which game `dir` itself holds, if any: Atlantis II when
/// [`atlantis2_data::files::holds_atlantis2`] says so (looked at first:
/// its CD installation's folder may hold the images of its discs), else
/// the first game when it holds atlantis.exe, or a CD image that is not
/// one of Atlantis II's discs (a folder of both games' images is the
/// first game's).
pub fn kind_of(dir: &Path) -> Option<GameKind> {
    if atlantis2_data::files::holds_atlantis2(dir) {
        Some(GameKind::Atlantis2)
    } else if has_file(dir, "atlantis.exe") || has_other_disc_images(dir) {
        Some(GameKind::LostTales)
    } else {
        None
    }
}

/// A folder below `dir` (`dir` itself first, then up to `depth` levels
/// down, a macOS app bundle included) that holds one of the games, and
/// which ([`kind_of`]) when `want` is none, else that game: the player
/// often picks the folder above the game's, or the GOG app on macOS. An
/// Atlantis II folder is not looked into for the first game.
fn find_root(dir: &Path, depth: usize, want: Option<GameKind>) -> Option<(PathBuf, GameKind)> {
    let kind = kind_of(dir);
    match (kind, want) {
        (Some(k), None) => return Some((dir.to_path_buf(), k)),
        (Some(k), Some(w)) if k == w => return Some((dir.to_path_buf(), k)),
        // The other game's folder: nothing of this one inside.
        (Some(_), Some(_)) => return None,
        (None, _) => {}
    }
    if depth == 0 {
        return None;
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .take(200)
        .collect();
    subdirs.sort();
    subdirs.iter().find_map(|d| find_root(d, depth - 1, want))
}

/// The first game's folder below `dir` (see [`find_root`]): one that
/// holds atlantis.exe or CD images, and not Atlantis II.
pub fn find_game_root(dir: &Path, depth: usize) -> Option<PathBuf> {
    find_root(dir, depth, Some(GameKind::LostTales)).map(|(d, _)| d)
}

/// Atlantis II's folder below `dir` (see [`find_root`]).
pub fn find_atlantis2_root(dir: &Path, depth: usize) -> Option<PathBuf> {
    find_root(dir, depth, Some(GameKind::Atlantis2)).map(|(d, _)| d)
}

/// The first folder below `dir` that holds either game, and which (see
/// [`find_root`]).
pub fn find_any_root(dir: &Path, depth: usize) -> Option<(PathBuf, GameKind)> {
    find_root(dir, depth, None)
}

/// The game folders below `dir`, up to `depth` levels down: the first
/// one met of the first game and of Atlantis II (a folder holding both
/// games side by side gives both), the first game's first; the same as
/// [`find_game_root`] and [`find_atlantis2_root`], in one walk.
pub fn find_roots(dir: &Path, depth: usize) -> Vec<(PathBuf, GameKind)> {
    fn walk(dir: &Path, depth: usize, found: &mut [Option<PathBuf>; 2]) {
        if let Some(k) = kind_of(dir) {
            let slot = &mut found[usize::from(k == GameKind::Atlantis2)];
            slot.get_or_insert_with(|| dir.to_path_buf());
            return;
        }
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut subdirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .take(200)
            .collect();
        subdirs.sort();
        for d in subdirs {
            if found.iter().all(Option::is_some) {
                return;
            }
            walk(&d, depth - 1, found);
        }
    }
    let mut found = [None, None];
    walk(dir, depth, &mut found);
    let [first, second] = found;
    first
        .map(|d| (d, GameKind::LostTales))
        .into_iter()
        .chain(second.map(|d| (d, GameKind::Atlantis2)))
        .collect()
}

/// What to tell the player about a folder that is not a game folder.
pub fn explain(dir: &Path, err: &GameError) -> String {
    if atlantis2_discs_only(dir) {
        return format!(
            "{} holds the discs of Atlantis II, not the installed game.\n\n\
             Choose the folder where Atlantis II is installed (the one with atlantis2.exe), \
             after putting the images of its four CDs (or copies of the CDs) in that folder \
             or in a folder inside it.",
            dir.display()
        );
    }
    crate::guide::explain(&names_in(dir), &dir.display().to_string(), err)
}

/// Whether the CD images at the top of `dir` (by their extension) are
/// taken for the first game's: when none is one of Atlantis II's discs
/// (as before Atlantis II was known: any image, the first game's message
/// then says what is wrong), or when one of them opens and is not (a
/// folder of both games' images is the first game's).
fn has_other_disc_images(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let (mut images, mut atlantis2, mut other) = (false, false, false);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if !atlantis_data::disc_image::IMAGE_EXTENSIONS
            .iter()
            .any(|x| name.ends_with(&format!(".{x}")))
        {
            continue;
        }
        images = true;
        match atlantis_data::disc_image::DiscImage::open(&e.path()) {
            Some(img) if atlantis2_data::files::disc_number(&img) != 0 => atlantis2 = true,
            Some(_) => other = true,
            None => {}
        }
        if other {
            return true;
        }
    }
    images && !atlantis2
}

/// Whether `dir` holds Atlantis II discs (CD images, or copies of the
/// CDs, in it or in a folder directly inside it) but not the installed
/// game.
fn atlantis2_discs_only(dir: &Path) -> bool {
    !atlantis2_data::files::holds_atlantis2(dir)
        && !atlantis2_data::A2Files::new(
            Arc::new(atlantis_data::MemFiles::new()),
            atlantis2_data::files::disc_candidates(dir),
        )
        .disc_numbers()
        .is_empty()
}

/// What to tell the player about an Atlantis II folder that does not open.
pub fn explain_atlantis2(dir: &Path, err: &OpenError) -> String {
    let shown = dir.display();
    match err {
        OpenError::NoDisc => format!(
            "{shown} holds Atlantis II, but none of its discs.\n\n\
             The CD release reads its videos and pictures from the four discs: put the \
             images of the four CDs (.iso, .bin, .img, .mdf or .nrg), or copies of the \
             CDs, in this folder or in a folder inside it (for example {shown}{sep}CD), \
             then choose the folder again. GOG.com's installation holds them already.",
            sep = std::path::MAIN_SEPARATOR
        ),
        OpenError::Exe { name, error } => format!(
            "{shown} holds an Atlantis II program, {name}, that Atlantis Native does not \
             know ({error}): an edition of the game that is not supported yet.\n\n\
             Supported: the French CD (1999), the CD with patch 1.01, and GOG.com."
        ),
        OpenError::NoExecutable => format!(
            "{shown} does not hold Atlantis II's program (atlantis2.exe, or GOG.com's \
             Game.exe): choose the folder where the game is installed."
        ),
    }
}

/// Folders where the game is likely installed on this computer, the most
/// likely first (not yet checked to hold the game).
pub fn candidate_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(target_os = "windows")]
    {
        // GOG.com records its installations in the registry (both
        // games, both views of the registry).
        for id in GOG_IDS {
            for key in [
                format!(r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\{id}"),
                format!(r"HKLM\SOFTWARE\GOG.com\Games\{id}"),
            ] {
                if let Some(path) = reg_value(&key, "path") {
                    log::info!("GOG.com's registry entry {id}: {path}");
                    out.push(PathBuf::from(path));
                }
            }
        }
        for base in [
            r"C:\GOG Games",
            r"C:\Program Files (x86)\GOG Galaxy\Games",
            r"C:\Program Files\GOG Galaxy\Games",
            r"C:\Program Files (x86)\GOG.com",
            r"C:\Cryo",
        ] {
            push_matching(&mut out, Path::new(base), "atlantis");
        }
    }
    #[cfg(target_os = "macos")]
    {
        push_matching(&mut out, Path::new("/Applications"), "atlantis");
        if let Some(home) = std::env::var_os("HOME") {
            push_matching(&mut out, &Path::new(&home).join("Applications"), "atlantis");
            push_matching(&mut out, &Path::new(&home).join("GOG Games"), "atlantis");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let home = Path::new(&home);
            for base in ["GOG Games", "Games", ".wine/drive_c/GOG Games"] {
                push_matching(&mut out, &home.join(base), "atlantis");
            }
        }
    }
    let _ = GOG_IDS;
    out
}

/// The subfolders of `base` whose name contains `word` (ignoring case).
fn push_matching(out: &mut Vec<PathBuf>, base: &Path, word: &str) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains(word)
        })
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    out.extend(found);
}

/// A registry value read with `reg query` (no registry crate needed).
#[cfg(target_os = "windows")]
fn reg_value(key: &str, name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args(["query", key, "/v", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with(name))?;
    let value = line.split("REG_SZ").nth(1)?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// The roots of the mounted discs (CD drives, mounted images).
pub fn disc_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(target_os = "windows")]
    for letter in b'D'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if std::fs::metadata(&root).is_ok() {
            out.push(root);
        }
    }
    #[cfg(target_os = "macos")]
    push_all(&mut out, Path::new("/Volumes"));
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(user) = std::env::var_os("USER") {
            push_all(&mut out, &Path::new("/media").join(&user));
            push_all(&mut out, &Path::new("/run/media").join(&user));
        }
        for p in ["/media/cdrom", "/mnt/cdrom"] {
            out.push(PathBuf::from(p));
        }
    }
    out
}

#[cfg(unix)]
fn push_all(out: &mut Vec<PathBuf>, base: &Path) {
    if let Ok(entries) = std::fs::read_dir(base) {
        out.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
}

/// The file `name` (any case) at the top of `dir`.
fn file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        (e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && p.is_file()).then_some(p)
    })
}

/// The number (1..4) of the Atlantis disc at `root`: the BIGCD archive it
/// holds.
pub fn disc_number(root: &Path) -> Option<u8> {
    (1..=4u8).find(|n| file_in(root, &format!("BIGCD{n}.BIG")).is_some())
}

/// A mounted Atlantis disc: its root and number.
pub fn find_disc() -> Option<(PathBuf, u8)> {
    disc_roots()
        .into_iter()
        .find_map(|r| disc_number(&r).map(|n| (r, n)))
}

/// What the copy from the discs still needs in `dest`: the disc numbers
/// whose archive is missing, and whether atlantis.exe is.
pub fn still_needed(dest: &Path) -> (Vec<u8>, bool) {
    let discs = (1..=4u8)
        .filter(|n| file_in(dest, &format!("BIGCD{n}.BIG")).is_none())
        .collect();
    (discs, file_in(dest, "atlantis.exe").is_none())
}

/// Copies what `dest` still needs from the disc at `root`: its BIGCD
/// archive, and atlantis.exe from disc 1 (each file written under a
/// temporary name, then renamed, so that a copy cut short is done again).
pub fn copy_disc(root: &Path, dest: &Path) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(dest)?;
    let (discs, exe) = still_needed(dest);
    let mut names = Vec::new();
    if let Some(n) = disc_number(root).filter(|n| discs.contains(n)) {
        names.push(format!("BIGCD{n}.BIG"));
    }
    if exe && file_in(root, "atlantis.exe").is_some() {
        names.push("atlantis.exe".to_string());
    }
    for name in &names {
        let src = file_in(root, name).ok_or(std::io::ErrorKind::NotFound)?;
        let tmp = dest.join(format!("{name}.part"));
        let n = std::fs::copy(&src, &tmp)?;
        if n != std::fs::metadata(&src)?.len() {
            return Err(std::io::Error::other(format!("{name}: copy incomplete")));
        }
        std::fs::rename(&tmp, dest.join(name))?;
    }
    Ok(names)
}

/// Opens `url` in the system's browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = r {
        log::warn!("{url}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("atlantis-setup-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn finds_the_game_below_the_chosen_folder() {
        let d = tmp("root");
        touch(&d.join("Atlantis/game/ATLANTIS.EXE"));
        assert_eq!(find_game_root(&d, 3), Some(d.join("Atlantis/game")));
        assert_eq!(find_game_root(&d, 1), None);
        assert_eq!(
            find_game_root(&d.join("Atlantis/game"), 0),
            Some(d.join("Atlantis/game"))
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn copies_the_discs_one_by_one() {
        let d = tmp("discs");
        let (d1, d2, dest) = (d.join("cd1"), d.join("cd2"), d.join("game"));
        touch(&d1.join("ATLANTIS.EXE"));
        std::fs::write(d1.join("BIGCD1.BIG"), vec![7u8; 5000]).unwrap();
        touch(&d2.join("bigcd2.big"));
        assert_eq!(disc_number(&d1), Some(1));
        assert_eq!(disc_number(&d2), Some(2));
        assert_eq!(still_needed(&dest), (vec![1, 2, 3, 4], true));
        assert_eq!(
            copy_disc(&d1, &dest).unwrap(),
            vec!["BIGCD1.BIG", "atlantis.exe"]
        );
        assert_eq!(
            std::fs::read(dest.join("BIGCD1.BIG")).unwrap(),
            vec![7u8; 5000]
        );
        assert_eq!(still_needed(&dest), (vec![2, 3, 4], false));
        // A disc copied already: nothing again.
        assert!(copy_disc(&d1, &dest).unwrap().is_empty());
        assert_eq!(copy_disc(&d2, &dest).unwrap(), vec!["BIGCD2.BIG"]);
        assert_eq!(still_needed(&dest), (vec![3, 4], false));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A fake GOG.com installation of Atlantis II: `Game.exe` beside the
    /// game's folders, its launcher `atlantis2.exe` left out, the four
    /// discs in `DATAS_A2`.
    fn fake_atlantis2_gog(dir: &Path) {
        touch(&dir.join("Game.exe"));
        touch(&dir.join("wam/tibet.wam"));
        touch(&dir.join("scenar/start.con"));
    }

    /// An Atlantis II disc copied from the CD (its marker file).
    fn fake_atlantis2_disc(dir: &Path, n: usize) {
        touch(&dir.join(atlantis2_data::files::DISC_MARKERS[n - 1]));
    }

    #[test]
    fn tells_the_two_games_apart() {
        let d = tmp("kinds");
        // GOG.com's layout of Atlantis II.
        let gog = d.join("GOG Galaxy/Games/Atlantis 2");
        fake_atlantis2_gog(&gog);
        assert_eq!(kind_of(&gog), Some(GameKind::Atlantis2));
        // Game.exe alone is not Atlantis II (nor the first game).
        touch(&d.join("other/Game.exe"));
        assert_eq!(kind_of(&d.join("other")), None);
        // The CD layout: atlantis2.exe (the CD's or patch 1.01's), the
        // discs' images or copies in a folder inside it.
        let cd = d.join("Cryo/Atlantis II");
        touch(&cd.join("ATLANTIS2.EXE"));
        fake_atlantis2_disc(&cd.join("CD/disc1"), 1);
        touch(&cd.join("CD/Atlantis2-CD2.iso"));
        assert_eq!(kind_of(&cd), Some(GameKind::Atlantis2));
        // The first game.
        let first = d.join("GOG Galaxy/Games/Atlantis The Lost Tales");
        touch(&first.join("atlantis.exe"));
        assert_eq!(kind_of(&first), Some(GameKind::LostTales));
        let images = d.join("isos");
        touch(&images.join("ATLANTIS_DISC1.ISO"));
        assert_eq!(kind_of(&images), Some(GameKind::LostTales));
        // Atlantis II's discs alone (a copy of one of its CDs): the
        // message says what to choose (the images themselves: in
        // `desktop`'s test with the game's discs).
        let discs = d.join("a2discs");
        fake_atlantis2_disc(&discs.join("CD1"), 1);
        assert_eq!(kind_of(&discs), None);
        assert!(explain(&discs, &GameError::NoExecutable).contains("discs of Atlantis II"));
        // A folder that holds no game.
        let none = d.join("Documents");
        touch(&none.join("letter.txt"));
        assert_eq!(kind_of(&none), None);
        assert_eq!(find_any_root(&none, 4), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn finds_both_games_below_a_folder() {
        let d = tmp("both");
        let games = d.join("Games");
        let gog = games.join("Atlantis 2");
        fake_atlantis2_gog(&gog);
        let first = games.join("Atlantis The Lost Tales/game");
        touch(&first.join("ATLANTIS.EXE"));
        // Atlantis II's folder is not taken for the first game's, and the
        // first game's search does not look into it.
        touch(&gog.join("extras/atlantis.exe"));
        assert_eq!(find_game_root(&games, 3), Some(first.clone()));
        assert_eq!(find_game_root(&gog, 3), None);
        assert_eq!(find_atlantis2_root(&games, 3), Some(gog.clone()));
        assert_eq!(find_atlantis2_root(&first, 3), None);
        // The first one met, in name order ("Atlantis 2" first).
        assert_eq!(
            find_any_root(&games, 3),
            Some((gog.clone(), GameKind::Atlantis2))
        );
        assert_eq!(
            find_any_root(&first, 0),
            Some((first.clone(), GameKind::LostTales))
        );
        assert_eq!(
            find_roots(&games, 3),
            vec![
                (first.clone(), GameKind::LostTales),
                (gog.clone(), GameKind::Atlantis2)
            ]
        );
        // As deep as the first game's search: the folder above "Games".
        assert_eq!(find_atlantis2_root(&d, 2), Some(gog.clone()));
        assert_eq!(find_atlantis2_root(&d, 1), None);
        // The candidate folders' names: "Atlantis 2", "Atlantis II",
        // "Beyond Atlantis" all contain the word.
        let mut out = Vec::new();
        for name in ["Atlantis II", "Beyond Atlantis", "Myst"] {
            std::fs::create_dir_all(games.join(name)).unwrap();
        }
        push_matching(&mut out, &games, "atlantis");
        let names: Vec<String> = out
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "Atlantis 2",
                "Atlantis II",
                "Atlantis The Lost Tales",
                "Beyond Atlantis"
            ]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn explains_atlantis2_folders() {
        let d = Path::new("/games/Atlantis 2");
        let e = explain_atlantis2(d, &OpenError::NoDisc);
        assert!(e.contains("none of its discs") && e.contains("images of the four CDs"));
        assert!(explain_atlantis2(d, &OpenError::NoExecutable).contains("Game.exe"));
        assert_eq!(GameKind::Atlantis2.name(), "Atlantis II");
    }

    #[test]
    fn explains_each_case() {
        let d = tmp("explain");
        let e = explain(&d, &GameError::MissingArchives(vec![2, 4]));
        assert!(e.contains("BIGCD2.BIG, from disc 2") && e.contains("BIGCD4.BIG, from disc 4"));
        assert!(explain(&d, &GameError::UnknownExecutable(123)).contains("123 bytes"));
        assert!(explain(&d, &GameError::NoExecutable).contains("does not hold"));
        touch(&d.join("DISQUE1.iso"));
        assert!(explain(&d, &GameError::NoExecutable).contains("disc images"));
        std::fs::remove_file(d.join("DISQUE1.iso")).unwrap();
        touch(&d.join("setup_atlantis_the_lost_tales_2.0.0.1.exe"));
        assert!(explain(&d, &GameError::NoExecutable).contains("GOG.com installer"));
        std::fs::remove_file(d.join("setup_atlantis_the_lost_tales_2.0.0.1.exe")).unwrap();
        touch(&d.join("BIGCD1.BIG"));
        assert!(explain(&d, &GameError::NoExecutable).contains("not atlantis.exe"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
