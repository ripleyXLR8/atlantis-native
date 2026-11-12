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
}
