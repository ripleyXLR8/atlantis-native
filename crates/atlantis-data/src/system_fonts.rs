//! Fonts of the player's system: a game asks Windows for a face by name
//! (GDI's `CreateFontIndirect`), which Native can only draw when the
//! face's file is on the machine. Not on the web, which has no such
//! folders.

use std::path::{Path, PathBuf};

/// The folders a system keeps its fonts in: `%WINDIR%\Fonts` on Windows,
/// the font folders of macOS and of the Linux desktops (one level of their
/// sub-folders is searched too).
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        for var in ["WINDIR", "SystemRoot"] {
            if let Some(w) = std::env::var_os(var) {
                dirs.push(PathBuf::from(w).join("Fonts"));
            }
        }
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if cfg!(target_os = "macos") {
            dirs.extend(
                [
                    "/Library/Fonts",
                    "/Library/Fonts/Microsoft",
                    "/System/Library/Fonts",
                ]
                .map(PathBuf::from),
            );
            dirs.extend(home.iter().map(|h| h.join("Library/Fonts")));
        } else {
            dirs.extend(
                home.iter()
                    .flat_map(|h| [h.join(".local/share/fonts"), h.join(".fonts")]),
            );
            dirs.extend(["/usr/local/share/fonts", "/usr/share/fonts"].map(PathBuf::from));
        }
    }
    dirs
}

/// The file `name` (any case) in `dirs` or one level below them, read.
pub fn font_in(dirs: &[PathBuf], name: &str) -> Option<Vec<u8>> {
    let find = |dir: &Path| -> Option<PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
    };
    for dir in dirs {
        let mut at = find(dir);
        if at.is_none() {
            let subs = std::fs::read_dir(dir).into_iter().flatten().flatten();
            at = subs
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .find_map(|p| find(&p));
        }
        if let Some(p) = at {
            return std::fs::read(p).ok();
        }
    }
    None
}

/// The system's font file `name` (`l_10646.ttf`), read; `None` when it is
/// not installed (and always on the web).
pub fn system_font_file(name: &str) -> Option<Vec<u8>> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    font_in(&system_font_dirs(), name)
}
