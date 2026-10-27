//! Desktop conveniences: finding the games (the first-start flow, the
//! system's folder picker, the installations on the computer, the copy
//! from the discs), error message boxes, and the log file of a run
//! without a terminal (a double click). The game folders used are kept in
//! the game library ([`crate::library`]): the first game's and Atlantis
//! II's, told apart by [`setup::kind_of`].

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use atlantis2_data::files::OpenError;
use atlantis2_data::{A2Files, Build};
use atlantis_data::{Game, GameError, StdDir};

use crate::launcher::LauncherEntry;
use crate::library::same_folder;
use crate::setup::{self, GameKind};
use atlantis_engine::persist::{user_data_dir, DirStore};
use atlantis_engine::SaveStore;

/// The log file of a run without a terminal, in the per-user data folder,
/// rewritten at every start.
pub const LOG_FILE: &str = "atlantis-native.log";
/// The log of the session before.
pub const PREV_LOG_FILE: &str = "atlantis-native.previous.log";

/// The per-user data folder, or a folder next to the current one when the
/// system gives none.
pub fn data_dir() -> PathBuf {
    user_data_dir().unwrap_or_else(|| PathBuf::from("atlantis-native-data"))
}

/// Starts the log: to the standard error when it is a terminal or
/// `RUST_LOG` is set (`env_logger`'s rules), else (a double click, the
/// Windows build without a console) to [`LOG_FILE`] at the `info` level.
/// Returns the log file's path when there is one.
pub fn init_log(force_stderr: bool) -> Option<PathBuf> {
    let to_stderr =
        force_stderr || std::io::stderr().is_terminal() || std::env::var_os("RUST_LOG").is_some();
    if !to_stderr {
        let dir = data_dir();
        let path = dir.join(LOG_FILE);
        // Keep the previous session's log: after a hang or a crash the
        // player restarts the game, which would overwrite the evidence.
        let _ = std::fs::rename(&path, dir.join(PREV_LOG_FILE));
        if let Ok(file) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::File::create(&path))
        {
            env_logger::Builder::new()
                .filter_level(log::LevelFilter::Info)
                .target(env_logger::Target::Pipe(Box::new(file)))
                .init();
            return Some(path);
        }
    }
    env_logger::init();
    None
}

/// Panics are logged, and shown in a message box when there is no
/// terminal to show them.
pub fn install_panic_hook(boxed: bool) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("{info}");
        if boxed {
            error_box(&format!("The game stopped on an internal error:\n\n{info}"));
        }
        default(info);
    }));
}

/// An error in the system's message box, and on the standard error.
pub fn error_box(text: &str) {
    eprintln!("{text}");
    log::error!("{text}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Atlantis Native")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// The store of the players, checkpoints and settings of `game`'s build:
/// each build in its own folder of the per-user data folder (the player
/// lists of the languages differ in their marker row).
pub fn store_for(game: &Game) -> Rc<dyn SaveStore> {
    let dir = data_dir().join(game.exe.build.name);
    log::info!("saves in {}", dir.display());
    Rc::new(DirStore::new(dir))
}

/// Opens the game in `dir`.
pub fn open_game(dir: &Path) -> Result<Game, String> {
    try_open(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// The game in `dir`: its files, or the images of its CDs.
pub fn try_open(dir: &Path) -> Result<Game, GameError> {
    open_with_kind(dir).map(|(game, _)| game)
}

/// The game in `dir`, and whether it was read from disc images.
pub fn open_with_kind(dir: &Path) -> Result<(Game, bool), GameError> {
    match Game::open(Arc::new(StdDir::new(dir))) {
        Err(e @ (GameError::NoExecutable | GameError::NoArchives)) => {
            match atlantis_data::disc_image::DiscSet::open_folder(dir) {
                Some(set) => {
                    log::info!(
                        "{}: the game from {} disc images",
                        dir.display(),
                        set.images.len()
                    );
                    Game::open(Arc::new(set)).map(|g| (g, true))
                }
                None => Err(e),
            }
        }
        r => r.map(|g| (g, false)),
    }
}

/// Atlantis II in `dir`: its executable's build, once its discs are found
/// (in `dir` or a folder directly inside it; GOG.com's folder holds them).
/// The game itself is booted when it is played ([`crate::a2::open`]).
pub fn open_atlantis2(dir: &Path) -> Result<&'static Build, OpenError> {
    A2Files::open_dir(dir, &[]).map(|(_, build)| build)
}

/// A game folder that opens, of either game.
pub enum Opened {
    /// The first game, opened, and whether from disc images.
    LostTales(Box<Game>, bool),
    /// Atlantis II: its build.
    Atlantis2(&'static Build),
}

impl Opened {
    pub fn kind(&self) -> GameKind {
        match self {
            Opened::LostTales(..) => GameKind::LostTales,
            Opened::Atlantis2(_) => GameKind::Atlantis2,
        }
    }

    /// How the game library shows it: the game's name and its edition
    /// ("Atlantis: The Lost Tales, GOG.com, French (1999 build)",
    /// "Atlantis II, GOG.com (patch 1.01)").
    pub fn label(&self) -> String {
        match self {
            Opened::LostTales(game, images) => edition_label(game, *images),
            Opened::Atlantis2(build) => build.label.to_string(),
        }
    }
}

/// The game in `dir`, either one: Atlantis II when the folder holds it
/// ([`setup::kind_of`]), else the first game. Why it does not open
/// otherwise, for the log ([`setup::explain`] and
/// [`setup::explain_atlantis2`] say it to the player).
pub fn open_any(dir: &Path) -> Result<Opened, String> {
    if setup::kind_of(dir) == Some(GameKind::Atlantis2) {
        open_atlantis2(dir)
            .map(Opened::Atlantis2)
            .map_err(|e| e.to_string())
    } else {
        open_with_kind(dir)
            .map(|(game, images)| Opened::LostTales(Box::new(game), images))
            .map_err(|e| e.to_string())
    }
}

/// How the game library shows the first game: its name and edition.
pub fn edition_label(game: &Game, from_images: bool) -> String {
    let name = GameKind::LostTales.name();
    if from_images {
        format!("{name}, {}, from disc images", game.edition())
    } else {
        format!("{name}, {}", game.edition())
    }
}

/// The game library's entry for `dir`: the game and its edition, or why
/// it does not open.
pub fn describe(dir: &Path) -> LauncherEntry {
    let (label, found) = if setup::kind_of(dir) == Some(GameKind::Atlantis2) {
        match open_atlantis2(dir) {
            Ok(build) => (build.label.to_string(), true),
            Err(e) => {
                log::info!("{}: {e}", dir.display());
                let why = match e {
                    OpenError::NoDisc => "Not found: Atlantis II's discs are not in this folder",
                    OpenError::Exe { .. } => {
                        "Not found: an edition of Atlantis II not supported yet"
                    }
                    OpenError::NoExecutable => "Not found: this folder no longer holds the game",
                };
                (why.to_string(), false)
            }
        }
    } else {
        match open_with_kind(dir) {
            Ok((game, images)) => (edition_label(&game, images), true),
            Err(e) => {
                log::info!("{}: {e}", dir.display());
                let why = if dir.exists() {
                    "Not found: this folder no longer holds the game"
                } else {
                    "Not found: the folder was moved, renamed, or is on a drive not plugged in"
                };
                (why.to_string(), false)
            }
        }
    };
    LauncherEntry {
        path: dir.to_path_buf(),
        label,
        found,
    }
}

/// The answer to a three-button question.
enum Choice {
    First,
    Second,
    Third,
}

/// A question in the system's message box with three labelled buttons
/// (closing the box counts as the third).
fn ask(text: &str, first: &str, second: &str, third: &str) -> Choice {
    let r = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title("Atlantis Native")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            first.to_string(),
            second.to_string(),
            third.to_string(),
        ))
        .show();
    match r {
        rfd::MessageDialogResult::Custom(s) if s == first => Choice::First,
        rfd::MessageDialogResult::Yes => Choice::First,
        rfd::MessageDialogResult::Custom(s) if s == second => Choice::Second,
        rfd::MessageDialogResult::No => Choice::Second,
        _ => Choice::Third,
    }
}

/// A question with two labelled buttons: true for the first.
fn ask2(text: &str, first: &str, second: &str) -> bool {
    let r = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title("Atlantis Native")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::OkCancelCustom(
            first.to_string(),
            second.to_string(),
        ))
        .show();
    matches!(r, rfd::MessageDialogResult::Ok)
        || matches!(r, rfd::MessageDialogResult::Custom(s) if s == first)
}

/// Copies the game from the player's four CDs, one after the other, into
/// a folder of the per-user data folder (as OpenRA installs its content
/// from the discs). Returns that folder once complete; `None` when the
/// player cancels. A copy cut short goes on from where it was.
fn copy_from_discs() -> Option<PathBuf> {
    let dest = data_dir().join("game-files");
    let mut wrong: Option<u8> = None;
    loop {
        let (discs, exe) = setup::still_needed(&dest);
        if discs.is_empty() && !exe {
            return Some(dest);
        }
        // atlantis.exe is on disc 1.
        let next = if exe { 1 } else { discs[0] };
        if let Some((root, n)) = setup::find_disc() {
            if discs.contains(&n) || (exe && n == 1) {
                match setup::copy_disc(&root, &dest) {
                    Ok(names) => {
                        log::info!("copied {} from disc {n}", names.join(", "));
                        wrong = None;
                        continue;
                    }
                    Err(e) => {
                        error_box(&format!(
                            "The copy from disc {n} failed:\n\n{e}\n\n\
                             Check that the disc is clean and that there is room on the \
                             computer, then try again."
                        ));
                        return None;
                    }
                }
            }
            wrong = Some(n);
        }
        let left: Vec<String> = (1..=4u8)
            .filter(|n| discs.contains(n) || (exe && *n == 1))
            .map(|n| n.to_string())
            .collect();
        let mut text = String::new();
        if let Some(n) = wrong {
            text.push_str(&format!("Disc {n} is copied already.\n\n"));
        }
        text.push_str(&format!(
            "Insert disc {next} of Atlantis: The Lost Tales, then click Copy.\n\n\
             Still to copy: disc {}. Each disc takes a minute or two: the next message \
             appears when it is done.\n\nThe files go to {}.",
            left.join(", "),
            dest.display()
        ));
        if !ask2(&text, "Copy", "Cancel") {
            return None;
        }
    }
}

/// Offers to copy the game from the disc at `root` (number `n`); the game
/// copied and its folder, or `None`.
fn offer_disc_copy(root: &Path, n: u8) -> Option<(Opened, PathBuf)> {
    let text = format!(
        "Disc {n} of Atlantis: The Lost Tales is in {}.\n\n\
         Atlantis Native can copy the game from your four discs into a folder on this \
         computer (about 2.3 GB; a minute or two per disc), then play it without the \
         discs.\n\nCopy the game from the discs?",
        root.display()
    );
    if !ask2(&text, "Copy from the discs", "Not now") {
        return None;
    }
    let dest = copy_from_discs()?;
    match open_with_kind(&dest) {
        Ok((game, images)) => Some((Opened::LostTales(Box::new(game), images), dest)),
        Err(e) => {
            error_box(&setup::explain(&dest, &e));
            None
        }
    }
}

/// What the first start gives.
pub enum FirstStart {
    /// The game to play, and its folder.
    Play(Opened, PathBuf),
    /// The games found on the computer, two or more, for the game
    /// library's screen.
    Library(Vec<PathBuf>),
}

/// The first start (the game library is empty): the installation found
/// on the computer, if the player takes it (two games or more found: the
/// game library's screen with them, to choose from); else the copy from a
/// disc in a drive; else the folder the player chooses in the system's
/// folder picker, after an explanation of why the game's files are
/// needed. `None` when the player quits.
pub fn first_start() -> Option<FirstStart> {
    // The installations on this computer (GOG.com's, copies in usual
    // places), of either game.
    let found: Vec<(PathBuf, Opened)> = installed_games()
        .into_iter()
        .filter_map(|d| open_any(&d).ok().map(|o| (d, o)))
        .collect();
    let offered = !found.is_empty();
    if found.len() >= 2 {
        let list: Vec<String> = found
            .iter()
            .map(|(d, o)| format!("\u{2022} {}\n   {}", o.label(), d.display()))
            .collect();
        let text = format!(
            "These games were found on this computer:\n\n{}\n\nAdd them to the game library, to choose the one to play?",
            list.join("\n")
        );
        match ask(&text, "Add them", "Choose another folder", "Quit") {
            Choice::First => {
                return Some(FirstStart::Library(
                    found.into_iter().map(|(d, _)| d).collect(),
                ))
            }
            Choice::Second => {}
            Choice::Third => return None,
        }
    } else if let Some((root, opened)) = found.into_iter().next() {
        let text = format!(
            "{} was found on this computer:\n\n{}\n\nPlay it from this folder?",
            opened.kind().name(),
            root.display()
        );
        match ask(&text, "Use this folder", "Choose another folder", "Quit") {
            Choice::First => return Some(FirstStart::Play(opened, root)),
            Choice::Second => {}
            Choice::Third => return None,
        }
    }
    // A disc of the game in a drive: copy the four discs.
    if !offered {
        if let Some((root, n)) = setup::find_disc() {
            if let Some((opened, dir)) = offer_disc_copy(&root, n) {
                return Some(FirstStart::Play(opened, dir));
            }
        }
    }
    if !offered {
        loop {
            match ask(
                setup::WELCOME,
                "Choose the game folder",
                "How to get the game",
                "Quit",
            ) {
                Choice::First => break,
                Choice::Second => setup::open_url(setup::HELP_URL),
                Choice::Third => return None,
            }
        }
    }
    pick_game_folder("Quit").map(|(opened, dir)| FirstStart::Play(opened, dir))
}

/// Opens the project's new-issue page in the browser, filled in with the
/// identifiers of the edition in `dir` (its files or its disc images).
fn report_edition(dir: &Path) {
    let names = setup::names_in(dir);
    let report = if names.iter().any(|n| n == "atlantis.exe") {
        crate::guide::EditionReport::collect(&StdDir::new(dir), &names, false)
    } else {
        atlantis_data::disc_image::DiscSet::open_folder(dir)
            .and_then(|set| crate::guide::EditionReport::collect(&set, &names, true))
    };
    match report {
        Some(r) => setup::open_url(&r.url(std::env::consts::OS, &crate::version_string())),
        None => setup::open_url(crate::guide::NEW_ISSUE_URL),
    }
}

/// The game folder the player chooses in the system's folder picker (of
/// either game: the folder itself or one below it, [`setup::find_any_root`]),
/// with a message saying what to do when the folder is not right (its
/// third button labelled `give_up`), and the offer to copy the discs when
/// the folder is a CD drive of the first game. The game and its folder;
/// `None` when the player cancels.
pub fn pick_game_folder(give_up: &str) -> Option<(Opened, PathBuf)> {
    loop {
        let dir = rfd::FileDialog::new()
            .set_title(
                "Choose the folder of your game (the one with atlantis.exe, or atlantis2.exe for Atlantis II)",
            )
            .pick_folder()?;
        // A CD drive: copy the discs.
        if let Some(n) = setup::disc_number(&dir) {
            if try_open(&dir).is_err() {
                if let Some(found) = offer_disc_copy(&dir, n) {
                    return Some(found);
                }
                continue;
            }
        }
        // The folder above the game's, or a macOS app bundle, holds it
        // further down.
        let (root, kind) =
            setup::find_any_root(&dir, 4).unwrap_or_else(|| (dir.clone(), GameKind::LostTales));
        if kind == GameKind::Atlantis2 {
            match open_atlantis2(&root) {
                Ok(build) => {
                    log::info!("game folder {} ({})", root.display(), build.label);
                    return Some((Opened::Atlantis2(build), root));
                }
                Err(e) => {
                    log::warn!("{}: {e}", root.display());
                    let text = setup::explain_atlantis2(&root, &e);
                    match ask(&text, "Choose another folder", "Help", give_up) {
                        Choice::First => {}
                        Choice::Second => setup::open_url(setup::HELP_URL_A2),
                        Choice::Third => return None,
                    }
                    continue;
                }
            }
        }
        match open_with_kind(&root) {
            Ok((game, images)) => {
                log::info!("game folder {}", root.display());
                return Some((Opened::LostTales(Box::new(game), images), root));
            }
            Err(e) => {
                log::warn!("{}: {e}", root.display());
                let text = setup::explain(&root, &e);
                if matches!(e, GameError::UnknownExecutable(_)) {
                    // An edition not supported yet: a report in one click.
                    match ask(
                        &text,
                        "Report it on GitHub",
                        "Choose another folder",
                        give_up,
                    ) {
                        Choice::First => report_edition(&root),
                        Choice::Second => {}
                        Choice::Third => return None,
                    }
                    continue;
                }
                match ask(&text, "Choose another folder", "Help", give_up) {
                    Choice::First => {}
                    Choice::Second => setup::open_url(setup::HELP_URL),
                    Choice::Third => return None,
                }
            }
        }
    }
}

/// The game folders, of both games, below each of `candidates` (up to 4
/// levels down) and `discs` (1 level), then `extra`: each once, in that
/// order (not yet checked to open).
pub fn game_folders_in(
    candidates: &[PathBuf],
    discs: &[PathBuf],
    extra: &[PathBuf],
) -> Vec<PathBuf> {
    let timed = |d: &Path, roots: Vec<(PathBuf, GameKind)>, t: std::time::Instant| {
        log::debug!("{}: {roots:?} in {:?}", d.display(), t.elapsed());
        roots
    };
    let mut roots: Vec<PathBuf> = candidates
        .iter()
        .flat_map(|d| {
            let t = std::time::Instant::now();
            timed(d, setup::find_roots(d, 4), t)
        })
        .chain(discs.iter().flat_map(|d| {
            let t = std::time::Instant::now();
            timed(d, setup::find_any_root(d, 1).into_iter().collect(), t)
        }))
        .map(|(d, _)| d)
        .collect();
    roots.extend(extra.iter().cloned());
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !out.iter().any(|o| same_folder(o, &r)) {
            out.push(r);
        }
    }
    out
}

/// The game folders of both games in the usual installation places
/// (GOG.com's registry entries and folders, ...: [`setup::candidate_folders`]),
/// on the mounted discs and disc images, and the copy from the discs (not
/// yet checked to open).
fn installed_games() -> Vec<PathBuf> {
    game_folders_in(
        &setup::candidate_folders(),
        &setup::disc_roots(),
        &[data_dir().join("game-files")],
    )
}

/// The game folders found on this computer, of both games: those of
/// [`installed_games`] that open.
pub fn find_installations() -> Vec<PathBuf> {
    installed_games()
        .into_iter()
        .filter(|r| open_any(r).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("atlantis-desktop-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn game_folders_of_both_games() {
        let d = tmp("folders");
        let gog = d.join("GOG Games");
        touch(&gog.join("Atlantis The Lost Tales/atlantis.exe"));
        touch(&gog.join("Atlantis 2/Game.exe"));
        std::fs::create_dir_all(gog.join("Atlantis 2/wam")).unwrap();
        std::fs::create_dir_all(gog.join("Atlantis 2/scenar")).unwrap();
        touch(&d.join("Cryo/Beyond Atlantis/ATLANTIS2.EXE"));
        touch(&d.join("drive/atlantis.exe"));
        std::fs::create_dir_all(d.join("empty")).unwrap();
        let found = game_folders_in(
            &[
                gog.clone(),
                d.join("Cryo"),
                d.join("empty"),
                // The registry's path and the folder's search: once.
                gog.join("Atlantis 2"),
            ],
            &[d.join("drive")],
            &[d.join("game-files"), gog.join("Atlantis The Lost Tales/")],
        );
        assert_eq!(
            found,
            vec![
                gog.join("Atlantis The Lost Tales"),
                gog.join("Atlantis 2"),
                d.join("Cryo/Beyond Atlantis"),
                d.join("drive"),
                d.join("game-files"),
            ]
        );
        // Fake files: neither opens, each as its own game.
        let first = describe(&gog.join("Atlantis The Lost Tales"));
        assert!(!first.found && first.label.contains("no longer holds the game"));
        let second = describe(&gog.join("Atlantis 2"));
        assert!(
            !second.found && second.label.contains("not supported yet"),
            "{}",
            second.label
        );
        assert!(open_any(&gog.join("Atlantis 2"))
            .err()
            .unwrap()
            .contains("Game.exe"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A symbolic link `link` to `target` (the game's files, only read).
    fn link(target: &Path, link: &Path) {
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link).unwrap();
        } else {
            std::os::windows::fs::symlink_file(target, link).unwrap();
        }
    }

    /// Atlantis II's editions as the library shows them: GOG.com's folder,
    /// the CD installation with its discs' images in a folder inside it,
    /// the same patched to 1.01, the CD installation without its discs.
    #[test]
    fn describes_atlantis2_editions() {
        let (Some(gog), Some(cd), Some(discs), Some(patch)) = (
            std::env::var_os("ATLANTIS2_GOG").map(PathBuf::from),
            std::env::var_os("ATLANTIS2_DATA").map(PathBuf::from),
            std::env::var_os("ATLANTIS2_DISCS").map(PathBuf::from),
            std::env::var_os("ATLANTIS2_PATCH101").map(PathBuf::from),
        ) else {
            atlantis2_data::testing::missing(
                "ATLANTIS2_GOG / ATLANTIS2_DATA / ATLANTIS2_DISCS / ATLANTIS2_PATCH101 not set",
            );
            return;
        };
        let e = describe(&gog);
        assert!(e.found, "{}", e.label);
        assert_eq!(e.label, "Atlantis II, GOG.com (patch 1.01)");
        assert!(matches!(open_any(&gog), Ok(Opened::Atlantis2(_))));
        // The CD layout, made of links: the installed folder, the images
        // of the four CDs in its folder "CD".
        let d = tmp("a2-editions");
        let make = |name: &str, exe: &Path| -> PathBuf {
            let root = d.join(name);
            for entry in std::fs::read_dir(&cd).unwrap().flatten() {
                let n = entry.file_name();
                if !n.to_string_lossy().eq_ignore_ascii_case("atlantis2.exe") {
                    link(&entry.path(), &root.join(&n));
                }
            }
            link(exe, &root.join("atlantis2.exe"));
            root
        };
        // The folder of the CD images alone: neither game, and the
        // message says what to choose.
        assert_eq!(setup::kind_of(&discs), None);
        assert!(open_any(&discs).is_err());
        assert!(setup::explain(&discs, &GameError::NoExecutable).contains("discs of Atlantis II"));
        let installed = make("Atlantis II", &cd.join("atlantis2.exe"));
        let e = describe(&installed);
        assert!(!e.found && e.label.contains("discs"), "{}", e.label);
        let err = open_atlantis2(&installed).err().unwrap();
        assert!(matches!(err, OpenError::NoDisc), "{err}");
        assert!(setup::explain_atlantis2(&installed, &err).contains("none of its discs"));
        for n in 1..=4 {
            let iso = format!("CD{n}.iso");
            link(&discs.join(&iso), &installed.join("CD").join(&iso));
        }
        assert_eq!(describe(&installed).label, "Atlantis II, French CD (1999)");
        let patched = make("Atlantis II 1.01", &patch);
        link(&installed.join("CD"), &patched.join("CD"));
        assert_eq!(describe(&patched).label, "Atlantis II, patch 1.01 (1999)");
        // Found below a parent folder, with the first game beside them.
        let mut candidates = vec![d.clone()];
        if let Some(first) = std::env::var_os("ATLANTIS_DATA").map(PathBuf::from) {
            link(&first, &d.join("Atlantis The Lost Tales"));
            candidates.push(gog.clone());
            let found: Vec<PathBuf> = game_folders_in(&candidates, &[], &[])
                .into_iter()
                .filter(|r| open_any(r).is_ok())
                .collect();
            assert_eq!(found.len(), 3, "{found:?}");
            let e = describe(&d.join("Atlantis The Lost Tales"));
            assert!(
                e.found && e.label.starts_with("Atlantis: The Lost Tales, CD, English"),
                "{}",
                e.label
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
