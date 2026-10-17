//! Desktop conveniences: the game folder chosen in the system's folder
//! picker and remembered, error message boxes, and the log file of a run
//! without a terminal (a double click).

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::persist::user_data_dir;

/// The file of the per-user data folder that holds the game folder last
/// chosen in the picker (its path, UTF-8).
pub const GAME_FOLDER_FILE: &str = "game-folder.txt";

/// The log file of a run without a terminal, in the per-user data folder,
/// rewritten at every start.
pub const LOG_FILE: &str = "atlantis-native.log";

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
    Game::open(Arc::new(StdDir::new(dir))).map_err(|e| format!("{}: {e}", dir.display()))
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

/// The game when no folder is given on the command line: the folder
/// chosen last time if it still holds the game, else the one the player
/// chooses in the system's folder picker (asked again after an error, and
/// remembered). `None` when the player cancels or there is no picker.
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
    loop {
        let dir = rfd::FileDialog::new()
            .set_title(
                "Choose the folder of Atlantis: The Lost Tales (with atlantis.exe and BIGCD*.BIG, or the GOG.com installation)",
            )
            .pick_folder()?;
        match open_game(&dir) {
            Ok(game) => {
                remember_folder(&dir);
                return Some(game);
            }
            Err(e) => error_box(&format!(
                "This folder does not hold a game Atlantis Native knows:\n\n{e}\n\n\
                 Choose the folder with atlantis.exe and BIGCD1.BIG .. BIGCD4.BIG \
                 (the CD release), or the GOG.com installation folder."
            )),
        }
    }
}
