//! Finding the player's copy of the game: the installations found on the
//! computer, and what a chosen folder holds when it is not a game folder
//! (so that the message says what to do), as other engines that need the
//! original files do (OpenMW's wizard, OpenRA's content detection,
//! devilutionX's GOG detection).

use std::path::{Path, PathBuf};

use atlantis_data::GameError;

pub use crate::guide::HELP_URL;

/// The GOG.com product id of the game (its registry key on Windows): the
/// first game's. Atlantis II's is 1207658951; it is not looked for here,
/// as the launcher opens the first game only (Atlantis II opens from the
/// command line, GOG's folder included).
const GOG_ID: &str = "1207658944";

/// The first-start explanation.
pub const WELCOME: &str = "Atlantis Native is a new engine for Atlantis: The Lost Tales. \
It does not include the game itself: the pictures, videos, voices and music belong to \
the game's publisher, so Atlantis Native plays them from your own copy of the game.\n\n\
Choose the folder of your game:\n\
\u{2022} GOG.com: the folder where the game is installed \
(for example C:\\GOG Games\\Atlantis The Lost Tales).\n\
\u{2022} CD: a folder holding atlantis.exe and BIGCD1.BIG to BIGCD4.BIG, copied from \
the four discs (each disc holds one BIGCD file), or the folder of the CD images (.iso, .bin). \
With a disc in the drive, Atlantis Native offers to copy the four discs itself.\n\n\
Atlantis Native only reads these files and never changes them.";

/// Case-insensitive lookup of a file in a folder.
fn has_file(dir: &Path, name: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_file()
        })
    })
}

/// Whether `dir` holds CD images (by their extension).
fn has_disc_images(dir: &Path) -> bool {
    names_in(dir).iter().any(|n| {
        atlantis_data::disc_image::IMAGE_EXTENSIONS
            .iter()
            .any(|e| n.ends_with(&format!(".{e}")))
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

/// A folder below `dir` (`dir` itself first, then up to `depth` levels
/// down, a macOS app bundle included) that holds atlantis.exe: the player
/// often picks the folder above the game's, or the GOG app on macOS.
pub fn find_game_root(dir: &Path, depth: usize) -> Option<PathBuf> {
    if has_file(dir, "atlantis.exe") || has_disc_images(dir) {
        return Some(dir.to_path_buf());
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
    subdirs.iter().find_map(|d| find_game_root(d, depth - 1))
}

/// What to tell the player about a folder that is not a game folder.
pub fn explain(dir: &Path, err: &GameError) -> String {
    // Atlantis II opens from the command line only for now (the launcher,
    // the library and the web build know the first game alone).
    if atlantis2_data::files::holds_atlantis2(dir) {
        return format!(
            "{} holds Atlantis II.\n\nThis version opens Atlantis II from the command line only, for now:\n\n    atlantis-native \"{}\"",
            dir.display(),
            dir.display()
        );
    }
    crate::guide::explain(&names_in(dir), &dir.display().to_string(), err)
}

/// Folders where the game is likely installed on this computer, the most
/// likely first (not yet checked to hold the game).
pub fn candidate_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(target_os = "windows")]
    {
        // GOG.com records its installations in the registry.
        for key in [
            format!(r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\{GOG_ID}"),
            format!(r"HKLM\SOFTWARE\GOG.com\Games\{GOG_ID}"),
        ] {
            if let Some(path) = reg_value(&key, "path") {
                out.push(PathBuf::from(path));
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
    let _ = GOG_ID;
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
