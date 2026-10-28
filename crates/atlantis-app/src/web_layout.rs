//! Which game the folder the player picked in the browser holds, from the
//! paths of its files alone (`webkitRelativePath`, lower case, the picked
//! folder's name first): the web build cannot look into folders as the
//! desktop does, so it decides from the list the browser gives. Built on
//! every platform so that its tests run with the others (and in
//! WebAssembly, see `tests` below).
//!
//! Atlantis II needs its installed folder (the one holding
//! `atlantis2.exe`, or GOG.com's `Game.exe` beside `wam` and `scenar`) and
//! its four discs, as copies of the CDs (folders holding `DATAS_A2`): the
//! browser reads files, not CD images. The discs are looked for as the
//! desktop looks for them ([`atlantis2_data::files::disc_candidates`]):
//! in the installed folder and the folders directly inside it; and, when
//! the player picked a folder above the installed one, in that folder and
//! the folders directly inside it too.

/// Which game was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// The first game (its own detection follows: CD, GOG.com, DVD).
    LostTales,
    /// Atlantis II, its installed folder and its disc candidates.
    Atlantis2(A2Layout),
    /// Both games: the player chooses.
    Both(A2Layout),
    /// Atlantis II's discs without its installed folder.
    Atlantis2DiscsOnly,
    /// Neither.
    None,
}

/// Where Atlantis II's files are among the picked ones: folder prefixes
/// (`""` for the picked folder's parent, else ending with `/`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct A2Layout {
    /// The installed game's folder.
    pub install: String,
    /// The folders that may be discs (each holds a `datas_a2` folder), in
    /// the order the desktop looks at them: the installed folder, the
    /// folders inside it, then the picked folder and those inside it.
    pub discs: Vec<String>,
}

/// The depth of a folder prefix (`""` 0, `a/` 1, `a/b/` 2).
fn depth(prefix: &str) -> usize {
    prefix.matches('/').count()
}

/// Whether some path is under `prefix` + `sub` + `/`.
fn has_folder(paths: &[&str], prefix: &str, sub: &str) -> bool {
    let want = format!("{prefix}{sub}/");
    paths.iter().any(|p| p.starts_with(&want))
}

/// The installed Atlantis II folders among `paths`: those holding
/// `atlantis2.exe`, or `game.exe` with `wam` and `scenar` beside it
/// ([`atlantis2_data::files::holds_atlantis2`]), the shallowest first.
fn a2_installs(paths: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        let (dir, name) = p.rsplit_once('/').map_or(("", *p), |(d, n)| (d, n));
        let dir = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let found = match name {
            "atlantis2.exe" => true,
            "game.exe" => has_folder(paths, &dir, "wam") && has_folder(paths, &dir, "scenar"),
            _ => false,
        };
        if found && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out.sort_by(|a, b| depth(a).cmp(&depth(b)).then_with(|| a.cmp(b)));
    out
}

/// The folders directly inside `prefix`, in name order.
fn children(paths: &[&str], prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .filter_map(|p| p.strip_prefix(prefix))
        .filter_map(|rest| rest.split_once('/').map(|(d, _)| format!("{prefix}{d}/")))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Atlantis II's layout among `paths`, if they hold its installed folder.
pub fn a2_layout(paths: &[&str]) -> Option<A2Layout> {
    let install = a2_installs(paths).into_iter().next()?;
    // The picked folder: the first part of every path.
    let root = paths
        .first()
        .and_then(|p| p.split_once('/'))
        .map_or(String::new(), |(r, _)| format!("{r}/"));
    let mut looked: Vec<String> = vec![install.clone()];
    looked.extend(children(paths, &install));
    if root != install {
        looked.push(root.clone());
        looked.extend(children(paths, &root));
    }
    let mut discs = Vec::new();
    for f in looked {
        let name = f.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        // `datas_a2` itself is a disc's content, not a disc.
        if name == "datas_a2" || discs.contains(&f) {
            continue;
        }
        if has_folder(paths, &f, "datas_a2") {
            discs.push(f);
        }
    }
    Some(A2Layout { install, discs })
}

/// Whether `paths` hold the first game: an `atlantis.exe`, or the folders
/// of Cryo's DVD (`face_a`, `face_b`).
pub fn holds_lost_tales(paths: &[&str]) -> bool {
    paths.iter().any(|p| {
        *p == "atlantis.exe"
            || p.ends_with("/atlantis.exe")
            || p.starts_with("face_a/")
            || p.starts_with("face_b/")
            || p.contains("/face_a/")
            || p.contains("/face_b/")
    })
}

/// Whether `paths` hold one of Atlantis II's disc markers
/// ([`atlantis2_data::files::DISC_MARKERS`]).
fn holds_a2_disc(paths: &[&str]) -> bool {
    paths.iter().any(|p| {
        atlantis2_data::files::DISC_MARKERS
            .iter()
            .any(|m| *p == *m || p.ends_with(&format!("/{m}")))
    })
}

/// What the picked files hold.
pub fn picked(paths: &[&str]) -> Picked {
    let a1 = holds_lost_tales(paths);
    match (a2_layout(paths), a1) {
        (Some(l), true) => Picked::Both(l),
        (Some(l), false) => Picked::Atlantis2(l),
        (None, true) => Picked::LostTales,
        (None, false) if holds_a2_disc(paths) => Picked::Atlantis2DiscsOnly,
        (None, false) => Picked::None,
    }
}

/// The folder names of a path list (its files' folders, each once), for
/// messages.
pub fn top_names(paths: &[&str], prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .filter_map(|p| p.strip_prefix(prefix))
        .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The message for Atlantis II's discs picked without its installed
/// folder.
pub const A2_DISCS_ONLY: &str = "This folder holds the discs of Atlantis II, not the \
    installed game.\n\nChoose the folder where Atlantis II is installed (the one with \
    atlantis2.exe, wam and scenar), after copying the four CDs into it or into folders \
    inside it (for example CD1 to CD4, each holding the disc's DATAS_A2 folder).";

/// The message for an installed Atlantis II without its discs.
pub const A2_NO_DISC: &str = "This folder holds Atlantis II, but none of its discs.\n\n\
    The CD release reads its videos and pictures from the four discs: copy each CD into \
    this folder or into a folder inside it (for example CD1 to CD4, each holding the \
    disc's DATAS_A2 folder), then choose the folder again. The browser cannot read CD \
    images (.iso, .bin...): copy the discs' files. GOG.com's installation holds them \
    already.";

/// The message for discs missing (`missing`: their numbers).
pub fn a2_missing_discs(missing: &[u8]) -> String {
    let list: Vec<String> = missing.iter().map(|n| format!("CD {n}")).collect();
    format!(
        "Atlantis II needs its four discs, and {} not among the chosen files. Copy each CD \
         into the folder of the installed game or into a folder inside it (for example \
         CD1 to CD4, each holding the disc's DATAS_A2 folder), then choose the folder \
         again.",
        match list.as_slice() {
            [one] => format!("{one} is"),
            _ => format!("{} are", list.join(", ")),
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    const CD: &[&str] = &[
        "atlantis 2/atlantis2.exe",
        "atlantis 2/wam/tibet.wam",
        "atlantis 2/scenar/chapi000.con",
        "atlantis 2/cd1/setup.exe",
        "atlantis 2/cd1/datas_a2/spf/sphex.spr",
        "atlantis 2/cd2/datas_a2/spf/cerceris.spr",
        "atlantis 2/cd3/datas_a2/spf/bousier.spr",
        "atlantis 2/cd4/datas_a2/spf/mante.spr",
        "atlantis 2/loc/menu.txt",
    ];

    #[test]
    fn the_cd_installation_with_its_discs_inside() {
        let l = a2_layout(CD).unwrap();
        assert_eq!(l.install, "atlantis 2/");
        assert_eq!(
            l.discs,
            [
                "atlantis 2/cd1/",
                "atlantis 2/cd2/",
                "atlantis 2/cd3/",
                "atlantis 2/cd4/"
            ]
        );
        assert_eq!(picked(CD), Picked::Atlantis2(l));
    }

    #[test]
    fn the_folder_above_the_installation() {
        // The installed game and the four copies side by side.
        let paths = [
            "games/atlantis 2/atlantis2.exe",
            "games/atlantis 2/wam/tibet.wam",
            "games/cd1/datas_a2/spf/sphex.spr",
            "games/cd2/datas_a2/spf/cerceris.spr",
            "games/zz/datas_a2/hnm/a.hnm",
        ];
        let l = a2_layout(&paths).unwrap();
        assert_eq!(l.install, "games/atlantis 2/");
        assert_eq!(l.discs, ["games/cd1/", "games/cd2/", "games/zz/"]);
    }

    #[test]
    fn gog_is_its_own_disc() {
        // GOG.com: Game.exe beside wam and scenar, every disc in DATAS_A2;
        // its atlantis2.exe launcher counts as the folder too.
        let paths = [
            "atlantis 2/atlantis2.exe",
            "atlantis 2/game.exe",
            "atlantis 2/wam/tibet.wam",
            "atlantis 2/scenar/chapi000.con",
            "atlantis 2/datas_a2/spf/sphex.spr",
            "atlantis 2/datas_a2/spf/mante.spr",
            "atlantis 2/__support/app/x.txt",
        ];
        let l = a2_layout(&paths).unwrap();
        assert_eq!(l.install, "atlantis 2/");
        assert_eq!(l.discs, ["atlantis 2/"]);
        // Game.exe alone is not the game without its folders.
        assert_eq!(a2_layout(&["x/game.exe", "x/data/a.bin"]), None);
        assert_eq!(
            a2_layout(&["x/game.exe", "x/wam/a", "x/scenar/b"])
                .unwrap()
                .install,
            "x/"
        );
    }

    #[test]
    fn the_shallowest_installation() {
        let paths = [
            "a/b/c/atlantis2.exe",
            "a/b/atlantis2.exe",
            "a/b/datas_a2/spf/sphex.spr",
        ];
        let l = a2_layout(&paths).unwrap();
        assert_eq!(l.install, "a/b/");
        // `datas_a2` is the disc's content: the folder holding it is the
        // disc.
        assert_eq!(l.discs, ["a/b/"]);
    }

    #[test]
    fn which_game() {
        assert_eq!(
            picked(&["gog/atlantis.exe", "gog/bigcd1.big"]),
            Picked::LostTales
        );
        assert_eq!(picked(&["dvd/face_a/eng/atlantis.exe"]), Picked::LostTales);
        let both = [
            "games/atlantis/atlantis.exe",
            "games/atlantis 2/atlantis2.exe",
            "games/atlantis 2/wam/a.wam",
        ];
        assert!(matches!(picked(&both), Picked::Both(_)));
        assert_eq!(
            picked(&["cd1/datas_a2/spf/sphex.spr", "cd1/setup.exe"]),
            Picked::Atlantis2DiscsOnly
        );
        assert_eq!(picked(&["photos/a.jpg"]), Picked::None);
        assert_eq!(top_names(CD, "atlantis 2/")[0], "atlantis2.exe");
    }

    #[test]
    fn messages() {
        assert!(a2_missing_discs(&[3]).contains("CD 3 is not"));
        assert!(a2_missing_discs(&[2, 4]).contains("CD 2, CD 4 are not"));
    }
}
