//! Finding the player's copy of the game: the installations found on the
//! computer, and what a chosen folder holds when it is not a game folder
//! (so that the message says what to do), as other engines that need the
//! original files do (OpenMW's wizard, OpenRA's content detection,
//! devilutionX's GOG detection).

use std::path::{Path, PathBuf};

use atlantis_data::GameError;

pub use crate::guide::HELP_URL;

/// The GOG.com product id of the game (its registry key on Windows).
const GOG_ID: &str = "1207658944";

/// The first-start explanation.
pub const WELCOME: &str = "Atlantis Native is a new engine for Atlantis: The Lost Tales. \
It does not include the game itself: the pictures, videos, voices and music belong to \
the game's publisher, so Atlantis Native plays them from your own copy of the game.\n\n\
Choose the folder of your game:\n\
\u{2022} GOG.com: the folder where the game is installed \
(for example C:\\GOG Games\\Atlantis The Lost Tales).\n\
\u{2022} CD: a folder holding atlantis.exe and BIGCD1.BIG to BIGCD4.BIG, copied from \
the four discs (each disc holds one BIGCD file).\n\n\
Atlantis Native only reads these files and never changes them.";

/// Case-insensitive lookup of a file in a folder.
fn has_file(dir: &Path, name: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_file()
        })
    })
}

fn names_in(dir: &Path) -> Vec<String> {
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
    if has_file(dir, "atlantis.exe") {
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
