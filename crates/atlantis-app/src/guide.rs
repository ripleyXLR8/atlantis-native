//! What to tell the player about a folder that is not a game folder, from
//! the names of the files in it (the desktop and the web build share it).

use atlantis_data::GameError;

/// The page of the player's guide on getting the game files.
pub const HELP_URL: &str =
    "https://github.com/ripleyXLR8/atlantis-native/blob/main/docs/GUIDE.md#getting-the-game-files";

/// What to tell the player about the folder `shown` whose files are
/// `names` (lower case), which `Game::open` refused with `err`.
pub fn explain(names: &[String], shown: &str, err: &GameError) -> String {
    let has = |name: &str| names.iter().any(|n| n == name);
    let any_ext = |exts: &[&str]| {
        names
            .iter()
            .any(|n| exts.iter().any(|e| n.ends_with(&format!(".{e}"))))
    };
    match err {
        GameError::MissingArchives(missing) => {
            let list: Vec<String> = missing
                .iter()
                .map(|n| format!("\u{2022} BIGCD{n}.BIG, from disc {n}"))
                .collect();
            format!(
                "The folder {shown} holds the game, but not all of it.\n\n\
                 The CD release spreads its files over four discs, one BIGCD file each. \
                 Copy these files into the folder:\n{}\n\n\
                 (Installing from the CD only copies a few files: the game then reads the \
                 rest from the disc, which Atlantis Native does not do.)",
                list.join("\n")
            )
        }
        GameError::NoArchives => format!(
            "The folder {shown} holds atlantis.exe but none of the game's archives \
             (BIGCD1.BIG to BIGCD4.BIG).\n\n\
             If you installed the game from its CD, the installation only holds a few \
             files: copy BIGCD1.BIG to BIGCD4.BIG from the four discs into this folder \
             (each disc holds one)."
        ),
        GameError::UnknownExecutable(size) => format!(
            "The folder {shown} holds an atlantis.exe ({size} bytes) that Atlantis Native \
             does not know: an edition of the game that is not supported yet.\n\n\
             Supported: the English CD (1997 and 2001), the French CD (1997), the CD with \
             Cryo's patch 1.01, and GOG.com (English, French, German, Spanish, Italian).\n\n\
             You can help support your edition: open an issue on the project's page with \
             the size above and where your copy comes from."
        ),
        GameError::NoExecutable => {
            if any_ext(&["iso", "bin", "cue", "mdf", "mds", "nrg", "img"]) {
                format!(
                    "The folder {shown} holds disc images.\n\n\
                     Open each image (double-click it on Windows 10 and later, or use \
                     7-Zip) and copy atlantis.exe from disc 1 and BIGCD1.BIG to \
                     BIGCD4.BIG from the four discs into one folder, then choose that \
                     folder."
                )
            } else if names
                .iter()
                .any(|n| n.starts_with("setup_atlantis") || n.ends_with(".pkg"))
            {
                format!(
                    "The folder {shown} holds the GOG.com installer, not the installed \
                     game.\n\n\
                     Run the installer first, then choose the folder where it installed \
                     the game."
                )
            } else if has("bigcd1.big") || names.iter().any(|n| n.starts_with("bigcd")) {
                format!(
                    "The folder {shown} holds game archives but not atlantis.exe.\n\n\
                     Copy atlantis.exe from the first disc of the CD into this folder: \
                     Atlantis Native reads the game's tables from it (it never runs it)."
                )
            } else if any_ext(&["zip", "7z", "rar"]) {
                format!(
                    "The folder {shown} holds archives (zip, 7z, rar).\n\n\
                     Extract them first, then choose the folder that holds atlantis.exe."
                )
            } else {
                format!(
                    "The folder {shown} does not hold Atlantis: The Lost Tales.\n\n\
                     Choose the folder that holds atlantis.exe: the game's installation \
                     folder from GOG.com, or a folder where you copied atlantis.exe and \
                     BIGCD1.BIG to BIGCD4.BIG from the CD."
                )
            }
        }
    }
}
