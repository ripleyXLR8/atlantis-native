//! Atlantis Native's SDL2 front end for handhelds and PortMaster (the
//! program `atlantis-native-sdl`, built with `--features sdl`): the parts
//! that need no SDL, tested on every system.
//!
//! The gamepad-as-mouse controls and the on-screen keyboard are from
//! Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement.

pub mod buttons;
pub mod config;
pub mod motion;
pub mod osk;

/// What to tell the player when the game folder holds a game this front
/// end does not play: Atlantis Evolution (`Atlantis Evolution.exe`) needs
/// OpenGL 3.3 (or OpenGL ES 3.0) and a desktop processor for its
/// panoramas and Theora videos, so the PortMaster build stays the first
/// game's, as for Atlantis II and III.
pub fn other_game(dir: &std::path::Path) -> Option<&'static str> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
        .collect();
    names
        .iter()
        .any(|n| n == "atlantis evolution.exe")
        .then_some(
            "This folder holds Atlantis Evolution, which the PortMaster build does not \
             play (it plays Atlantis: The Lost Tales only). Play Atlantis Evolution with \
             Atlantis Native for Windows, macOS or Linux, or in the browser.",
        )
}

/// The options of a run with its saves in `saves` (as the desktop's
/// `options_with_settings`): the enhancements and the subtitles read from
/// `settings.ini` (written with the defaults when there is none);
/// `original` turns them off and starts at the original's OMNI 3D mode and
/// volumes.
pub fn run_options(
    saves: std::rc::Rc<dyn atlantis_engine::SaveStore>,
    original: bool,
    start: atlantis_engine::Start,
) -> atlantis_engine::Options {
    let stored = atlantis_engine::Settings::load(saves.as_ref());
    let settings = stored.unwrap_or_default();
    if stored.is_none() {
        settings.save(saves.as_ref());
    }
    atlantis_engine::Options {
        enhancements: if original {
            atlantis_engine::Enhancements::original()
        } else {
            settings.enhancements
        },
        // `--original` starts with SUBTITLES off, as every start of the
        // original (0x004960a8 = 0), whatever settings.ini keeps; OMNI 3D
        // MODE 1 and the volumes 7f 6e 6e 6e too (0x004960ac, 0x00495e70..).
        subtitles: settings.subtitles && !original,
        original_start_values: original,
        saves,
        start,
        ..atlantis_engine::Options::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn atlantis_evolution_is_refused_with_a_message() {
        let d = std::env::temp_dir().join(format!("atlantis-sdl-other-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(super::other_game(&d), None);
        std::fs::write(d.join("atlantis.exe"), b"").unwrap();
        assert_eq!(super::other_game(&d), None);
        std::fs::write(d.join("ATLANTIS EVOLUTION.EXE"), b"").unwrap();
        assert!(super::other_game(&d).is_some_and(|m| m.contains("Atlantis Evolution")));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `--original` asks the engine for the original's start values of
    /// the options (OMNI 3D MODE 1, volumes 7f 6e 6e 6e) and SUBTITLES
    /// off, as the desktop front end does; the default run keeps
    /// `settings.ini`'s.
    #[test]
    fn original_run_starts_at_the_original_options() {
        use atlantis_engine::{MemStore, SaveStore, Settings, Start};
        use std::rc::Rc;
        let store = Rc::new(MemStore::new());
        Settings {
            subtitles: true,
            omni3d_mode2: true,
            volumes: [10, 20, 30, 40],
            ..Settings::default()
        }
        .save(store.as_ref());
        let saves: Rc<dyn SaveStore> = store.clone();
        let o = super::run_options(saves.clone(), true, Start::Boot);
        assert!(o.original_start_values && !o.subtitles);
        let o = super::run_options(saves, false, Start::Boot);
        assert!(!o.original_start_values && o.subtitles);
    }
}
