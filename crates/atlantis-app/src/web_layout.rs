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

/// Atlantis III's installed folder among `paths`: the shallowest holding
/// `game.exe` (GOG.com's) or `atlantis3.exe` (the French retail CD's),
/// and `datas/scenar/defnames.con`
/// ([`atlantis3_data::files::holds_atlantis3`]).
pub fn a3_install(paths: &[&str]) -> Option<String> {
    paths
        .iter()
        .filter_map(|p| p.strip_suffix("datas/scenar/defnames.con"))
        .filter(|dir| dir.is_empty() || dir.ends_with('/'))
        .filter(|dir| {
            atlantis3_data::exe::EXE_NAMES
                .iter()
                .any(|n| paths.contains(&format!("{dir}{}", n.to_ascii_lowercase()).as_str()))
        })
        .min_by_key(|dir| (depth(dir), dir.to_string()))
        .map(str::to_string)
}

/// The folders that may be the French retail CD's discs (copies of the
/// CDs: a disc marker `atlantis3.cd<n>` beside a `datas` folder,
/// [`atlantis3_data::discs::disc_number`]) for the installed folder
/// `install`, in the order the desktop looks at them
/// ([`atlantis3_data::discs::disc_candidates`]): the folders inside the
/// installed one and those inside them, in name order; then, when the
/// player picked a folder above the installed one, that folder and the
/// folders directly inside it (the copies beside the installed folder).
/// [`atlantis3_data::discs::CdSource`] keeps the first copy of each disc.
pub fn a3_discs(paths: &[&str], install: &str) -> Vec<String> {
    let is_disc = |f: &str| {
        let markers = atlantis3_data::discs::DISC_MARKERS
            .iter()
            .filter(|m| paths.contains(&format!("{f}{}", m.to_ascii_lowercase()).as_str()))
            .count();
        markers == 1 && has_folder(paths, f, "datas")
    };
    let root = paths
        .first()
        .and_then(|p| p.split_once('/'))
        .map_or(String::new(), |(r, _)| format!("{r}/"));
    let mut looked: Vec<String> = Vec::new();
    for c in children(paths, install) {
        // `datas` is the install's, not a disc.
        if c == format!("{install}datas/") {
            continue;
        }
        if is_disc(&c) {
            looked.push(c);
        } else {
            looked.extend(children(paths, &c));
        }
    }
    if root != install {
        looked.push(root.clone());
        looked.extend(children(paths, &root));
    }
    let mut discs = Vec::new();
    for f in looked {
        if f != install && !discs.contains(&f) && is_disc(&f) {
            discs.push(f);
        }
    }
    discs
}

/// Atlantis Evolution's installed folder among `paths`: the shallowest
/// holding `atlantis evolution.exe` and `3d/ongameenter.lua`
/// ([`atlantis4_data::files::holds_atlantis4`]).
pub fn a4_install(paths: &[&str]) -> Option<String> {
    paths
        .iter()
        .filter_map(|p| p.strip_suffix("3d/ongameenter.lua"))
        .filter(|dir| dir.is_empty() || dir.ends_with('/'))
        .filter(|dir| paths.contains(&format!("{dir}atlantis evolution.exe").as_str()))
        .min_by_key(|dir| (depth(dir), dir.to_string()))
        .map(str::to_string)
}

/// The installed folder of Atlantis Evolution's 2004 CD among `paths`:
/// the shallowest holding `atlantis evolution.exe` and files under
/// `cd0/datas/scenar/`
/// ([`atlantis3_data::evolution2004::holds_evolution_2004`]).
pub fn a4cd_install(paths: &[&str]) -> Option<String> {
    const SCENAR: &str = "cd0/datas/scenar/";
    paths
        .iter()
        .filter_map(|p| p.find(SCENAR).map(|i| &p[..i]))
        .filter(|dir| dir.is_empty() || dir.ends_with('/'))
        .filter(|dir| paths.contains(&format!("{dir}atlantis evolution.exe").as_str()))
        .min_by_key(|dir| (depth(dir), dir.to_string()))
        .map(str::to_string)
}

/// The entries of folder `dir` (lower case, `""` the top) among the
/// files `paths` (lower-case paths below a folder): its files and its
/// folders, each once, in name order, as the desktop's `StdDir` lists a
/// folder (the web's `PickedDir`).
pub fn entries<'a>(paths: impl Iterator<Item = &'a str>, dir: &str) -> Vec<String> {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{}/", dir.trim_end_matches('/'))
    };
    let mut out: Vec<String> = paths
        .filter_map(|k| k.strip_prefix(prefix.as_str()))
        .map(|n| n.split('/').next().unwrap_or(n).to_string())
        .collect();
    out.sort();
    out.dedup();
    out
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

/// The message for the French retail CD of Atlantis III picked without
/// its discs.
pub const A3_NO_DISC: &str = "This folder holds Atlantis III's CD release, but none of its \
    discs.\n\nThe CD release reads its videos and panoramas from its three discs: copy each \
    CD into the folder of the installed game or into a folder inside it (for example CD1 to \
    CD3, each holding the disc's Datas folder and its Atlantis3.cd file), or pick the folder \
    that holds both the installed game and the copies, then choose the folder again. The \
    browser cannot read CD images (.iso...): copy the discs' files.";

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
    fn atlantis3s_installed_folder() {
        let paths = [
            "atlantis 3/game.exe",
            "atlantis 3/datas/scenar/defnames.con",
            "atlantis 3/datas/wam/hoggar.wam",
        ];
        assert_eq!(a3_install(&paths).as_deref(), Some("atlantis 3/"));
        assert_eq!(picked(&paths), Picked::None);
        // No program beside the data: not the game.
        assert_eq!(a3_install(&paths[1..]), None);
        assert_eq!(a3_install(CD), None);
    }

    /// The French retail CD of Atlantis III: its installed folder (the
    /// CD's `Atlantis3.exe`), its three copies inside it, or beside it in
    /// the picked folder.
    #[test]
    fn atlantis3s_cd_with_its_discs() {
        let inside = [
            "atlantis 3/atlantis3.exe",
            "atlantis 3/datas/scenar/defnames.con",
            "atlantis 3/datas/atlantis3.cfg",
            "atlantis 3/cd1/atlantis3.cd1",
            "atlantis 3/cd1/datas/warp/x.pak",
            "atlantis 3/cd1/datas/atlantis3.cfg",
            "atlantis 3/discs/cd2/atlantis3.cd2",
            "atlantis 3/discs/cd2/datas/hnm/a.hnm",
            "atlantis 3/discs/cd3/atlantis3.cd3",
            "atlantis 3/discs/cd3/datas/hnm/b.hnm",
            "atlantis 3/goodies/readme.txt",
        ];
        assert_eq!(a3_install(&inside).as_deref(), Some("atlantis 3/"));
        assert_eq!(
            a3_discs(&inside, "atlantis 3/"),
            [
                "atlantis 3/cd1/",
                "atlantis 3/discs/cd2/",
                "atlantis 3/discs/cd3/"
            ]
        );
        assert_eq!(picked(&inside), Picked::None);
        // The copies beside the installed folder (the folder above both
        // picked), and a marker without its Datas: no disc.
        let beside = [
            "a3/installed/atlantis3.exe",
            "a3/installed/datas/scenar/defnames.con",
            "a3/cd1/atlantis3.cd1",
            "a3/cd1/datas/warp/x.pak",
            "a3/cd2/atlantis3.cd2",
            "a3/cd2/datas/hnm/a.hnm",
            "a3/cd3/atlantis3.cd3",
            "a3/cd3/datas/hnm/b.hnm",
            "a3/iso/atlantis3_cd1.iso",
            "a3/loose/atlantis3.cd2",
        ];
        assert_eq!(a3_install(&beside).as_deref(), Some("a3/installed/"));
        assert_eq!(
            a3_discs(&beside, "a3/installed/"),
            ["a3/cd1/", "a3/cd2/", "a3/cd3/"]
        );
        // The installed folder alone: no disc (the page then says so).
        assert!(a3_discs(&beside[..2], "a3/installed/").is_empty());
        // GOG.com's Spanish install holds the three markers at its root:
        // neither it nor anything in it is a disc.
        let gog = [
            "atlantis 3/game.exe",
            "atlantis 3/atlantis3.exe",
            "atlantis 3/atlantis3.cd1",
            "atlantis 3/atlantis3.cd2",
            "atlantis 3/atlantis3.cd3",
            "atlantis 3/datas/scenar/defnames.con",
        ];
        assert_eq!(a3_install(&gog).as_deref(), Some("atlantis 3/"));
        assert!(a3_discs(&gog, "atlantis 3/").is_empty());
        assert!(A3_NO_DISC.contains("none of its discs"));
    }

    /// A folder's entries are its files and its folders (Atlantis III's
    /// install language is the one folder of `datas/loc/text`).
    #[test]
    fn a_folders_entries() {
        let paths = [
            "datas/loc/text/fra/menu.txt",
            "datas/loc/text/fra/obj.txt",
            "datas/loc/readme.txt",
            "game.exe",
        ];
        let e = |d| entries(paths.iter().copied(), d);
        assert_eq!(e("datas/loc/text"), ["fra"]);
        assert_eq!(e("datas/loc/"), ["readme.txt", "text"]);
        assert_eq!(e(""), ["datas", "game.exe"]);
        assert!(e("spw").is_empty());
    }

    /// The French CD's installed folder and its three copies on disk
    /// (`ATLANTIS3_CD_FR`, the copies `cd1` to `cd3` beside it as in the
    /// project's data): the paths the browser would give for their parent
    /// folder, the layout found, the discs read from the copies and the
    /// edition named as on the desktop.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn atlantis3s_cd_on_disk() {
        use atlantis_data::{FileSource, StdDir};
        use std::sync::Arc;
        let Some((install, _)) = atlantis3_data::testing::cd_fr() else {
            return;
        };
        let parent = install.parent().unwrap();
        if !parent.join("cd2").is_dir() {
            eprintln!("no copies of the CDs beside {}", install.display());
            return;
        }
        fn walk(dir: &std::path::Path, rel: &str, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                let path = format!("{rel}{name}");
                if e.path().is_dir() {
                    walk(&e.path(), &format!("{path}/"), out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut all = Vec::new();
        for d in ["installed", "cd1", "cd2", "cd3"] {
            walk(&parent.join(d), &format!("a3cd/{d}/"), &mut all);
        }
        let paths: Vec<&str> = all.iter().map(String::as_str).collect();
        assert_eq!(a3_install(&paths).as_deref(), Some("a3cd/installed/"));
        let discs = a3_discs(&paths, "a3cd/installed/");
        assert_eq!(discs, ["a3cd/cd1/", "a3cd/cd2/", "a3cd/cd3/"]);
        assert_eq!(
            entries(paths.iter().copied(), "a3cd/installed/datas/loc/text"),
            ["fra"]
        );
        let sources: Vec<(String, Arc<dyn FileSource>)> = discs
            .iter()
            .map(|p| {
                let dir = parent.join(p.trim_start_matches("a3cd/"));
                (p.clone(), Arc::new(StdDir::new(dir)) as Arc<dyn FileSource>)
            })
            .collect();
        let files =
            atlantis3_data::A3Files::with_disc_sources(Arc::new(StdDir::new(&install)), || sources);
        assert_eq!(files.edition_label().unwrap(), "Atlantis III, CD, French");
        let cd = files.discs().unwrap();
        assert_eq!(cd.disc_numbers(), [1, 2, 3]);
        // A new game's disc test (ChangeCD=2): the Hoggar's panorama from
        // disc 2.
        cd.change_cd(2);
        assert_eq!(cd.current(), 2);
        assert!(files
            .source()
            .size("Datas/warp/Warp512/hoggar/ho0004.hnm")
            .is_some());
    }

    #[test]
    fn atlantis_evolutions_installed_folder() {
        let paths = [
            "atlantis evolution/atlantis evolution.exe",
            "atlantis evolution/3d/ongameenter.lua",
            "atlantis evolution/3d/bateau_ca/0001.lua",
            "atlantis evolution/videos/pc-macosx/intro.ogv",
        ];
        assert_eq!(a4_install(&paths).as_deref(), Some("atlantis evolution/"));
        assert_eq!(picked(&paths), Picked::None);
        assert_eq!(a3_install(&paths), None);
        // The program alone, or the scripts alone: not the game.
        assert_eq!(a4_install(&paths[..1]), None);
        assert_eq!(a4_install(&paths[1..]), None);
        assert_eq!(a4_install(CD), None);
    }

    #[test]
    fn atlantis_evolutions_2004_installed_folder() {
        let paths = [
            "atlantis evolution/atlantis evolution.exe",
            "atlantis evolution/cd0/datas/scenar/chapi000.con",
            "atlantis evolution/cd1/datas/warp/c10001.jp6",
            "atlantis evolution/datas/atlantis4.cfg",
        ];
        assert_eq!(a4cd_install(&paths).as_deref(), Some("atlantis evolution/"));
        assert_eq!(a4_install(&paths), None);
        assert_eq!(a3_install(&paths), None);
        assert_eq!(picked(&paths), Picked::None);
        // The install picked itself.
        let top: Vec<&str> = paths
            .iter()
            .map(|p| p.strip_prefix("atlantis evolution/").unwrap())
            .collect();
        assert_eq!(a4cd_install(&top).as_deref(), Some(""));
        // The program alone, or the data alone: not the game.
        assert_eq!(a4cd_install(&paths[..1]), None);
        assert_eq!(a4cd_install(&paths[1..]), None);
        assert_eq!(a4cd_install(CD), None);
    }

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
    fn the_italian_collection_is_its_own_disc() {
        // Its launcher, its Launch and Splash folders, one empty music file.
        let paths = [
            "atlantis 2/atlantis2.exe",
            "atlantis 2/game.exe",
            "atlantis 2/launch/settings.ini",
            "atlantis 2/splash/splash_1024x768.bmp",
            "atlantis 2/music/muzik39.apc",
            "atlantis 2/wam/tibet.wam",
            "atlantis 2/scenar/chapi000.con",
            "atlantis 2/datas_a2/spf/sphex.spr",
            "atlantis 2/datas_a2/spf/mante.spr",
        ];
        let l = a2_layout(&paths).unwrap();
        assert_eq!(l.install, "atlantis 2/");
        assert_eq!(l.discs, ["atlantis 2/"]);
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
