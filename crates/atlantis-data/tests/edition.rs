//! The release shown to the player.
use std::sync::Arc;

use atlantis_data::{Game, StdDir};

fn edition(dir: &str) -> Option<String> {
    let g = Game::open(Arc::new(StdDir::new(dir))).ok()?;
    Some(g.edition().to_string())
}

#[test]
fn reference_cd_is_english_2001() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, English (2001 build)");
}

#[test]
fn gog_languages() {
    let Ok(dir) = std::env::var("ATLANTIS_GOG") else {
        eprintln!("ATLANTIS_GOG (folder of the per-language GOG trees) not set: skipped");
        return;
    };
    for (sub, lang) in [
        ("en-US", "English"),
        ("fr-FR", "French"),
        ("de-DE", "German"),
        ("es-ES", "Spanish"),
        ("it-IT", "Italian"),
    ] {
        assert_eq!(
            edition(&format!("{dir}/{sub}")).unwrap(),
            format!("GOG.com, {lang} (1999 build)")
        );
    }
}
