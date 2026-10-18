//! Desktop conveniences: the game folder chosen in the system's folder
//! picker and remembered, error message boxes, and the log file of a run
//! without a terminal (a double click).

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis_data::{Game, GameError, StdDir};

use crate::setup;
use atlantis_engine::persist::user_data_dir;

/// The file of the per-user data folder that holds the game folder last
/// chosen in the picker (its path, UTF-8).
pub const GAME_FOLDER_FILE: &str = "game-folder.txt";

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

/// Opens the game in `dir`.
pub fn open_game(dir: &Path) -> Result<Game, String> {
    try_open(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// The game in `dir`: its files, or the images of its CDs.
fn try_open(dir: &Path) -> Result<Game, GameError> {
    match Game::open(Arc::new(StdDir::new(dir))) {
        Err(e @ (GameError::NoExecutable | GameError::NoArchives)) => {
            match atlantis_data::disc_image::DiscSet::open_folder(dir) {
                Some(set) => {
                    log::info!(
                        "{}: the game from {} disc images",
                        dir.display(),
                        set.images.len()
                    );
                    Game::open(Arc::new(set))
                }
                None => Err(e),
            }
        }
        r => r,
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

fn remembered_folder() -> Option<PathBuf> {
    let text = std::fs::read_to_string(data_dir().join(GAME_FOLDER_FILE)).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

fn remember_folder(dir: &Path) {
    let Some(text) = dir.to_str() else {
        log::warn!("{}: not UTF-8, not remembered", dir.display());
        return;
    };
    let base = data_dir();
    let written = std::fs::create_dir_all(&base)
        .and_then(|()| std::fs::write(base.join(GAME_FOLDER_FILE), format!("{text}\n")));
    if let Err(e) = written {
        log::warn!("{}: {e}", base.join(GAME_FOLDER_FILE).display());
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
/// copied, or `None`.
fn offer_disc_copy(root: &Path, n: u8) -> Option<Game> {
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
    match try_open(&dest) {
        Ok(game) => {
            remember_folder(&dest);
            Some(game)
        }
        Err(e) => {
            error_box(&setup::explain(&dest, &e));
            None
        }
    }
}

/// The game when no folder is given on the command line: the folder
/// chosen last time if it still holds the game; else an installation found
/// on the computer, if the player takes it; else the folder the player
/// chooses in the system's folder picker, after an explanation of why the
/// game's files are needed, with a message saying what to do when the
/// folder is not right (remembered once right). `None` when the player
/// quits.
pub fn choose_game() -> Option<Game> {
    if let Some(dir) = remembered_folder() {
        match open_game(&dir) {
            Ok(game) => {
                log::info!("game folder {}", dir.display());
                return Some(game);
            }
            Err(e) => log::warn!("the remembered game folder: {e}"),
        }
    }
    // An installation on this computer (GOG.com's, a copy in a usual place).
    let mut offered = false;
    for dir in setup::candidate_folders() {
        let Some(root) = setup::find_game_root(&dir, 4) else {
            continue;
        };
        let Ok(game) = try_open(&root) else {
            continue;
        };
        offered = true;
        let text = format!(
            "Atlantis: The Lost Tales was found on this computer:\n\n{}\n\nPlay it from this folder?",
            root.display()
        );
        match ask(&text, "Use this folder", "Choose another folder", "Quit") {
            Choice::First => {
                remember_folder(&root);
                return Some(game);
            }
            Choice::Second => break,
            Choice::Third => return None,
        }
    }
    // A disc of the game in a drive: copy the four discs.
    if !offered {
        if let Some((root, n)) = setup::find_disc() {
            if let Some(game) = offer_disc_copy(&root, n) {
                return Some(game);
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
    loop {
        let dir = rfd::FileDialog::new()
            .set_title("Choose the folder of Atlantis: The Lost Tales (the one with atlantis.exe)")
            .pick_folder()?;
        // A CD drive: copy the discs.
        if let Some(n) = setup::disc_number(&dir) {
            if try_open(&dir).is_err() {
                if let Some(game) = offer_disc_copy(&dir, n) {
                    return Some(game);
                }
                continue;
            }
        }
        // The folder above the game's, or a macOS app bundle, holds it
        // further down.
        let root = setup::find_game_root(&dir, 4).unwrap_or_else(|| dir.clone());
        let result = try_open(&root).map(|g| (g, root.clone()));
        match result {
            Ok((game, root)) => {
                remember_folder(&root);
                log::info!("game folder {}", root.display());
                return Some(game);
            }
            Err(e) => {
                log::warn!("{}: {e}", root.display());
                match ask(
                    &setup::explain(&root, &e),
                    "Choose another folder",
                    "Help",
                    "Quit",
                ) {
                    Choice::First => {}
                    Choice::Second => setup::open_url(setup::HELP_URL),
                    Choice::Third => return None,
                }
            }
        }
    }
}
