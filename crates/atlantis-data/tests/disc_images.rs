//! The game read straight from its CD images: the French CD's ISO images
//! (2048-byte sectors) and the English CD's raw images (2352-byte sectors).
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::Game;

fn open(var: &str) -> Option<Game> {
    let Ok(dir) = std::env::var(var) else {
        eprintln!("{var} not set: skipped");
        return None;
    };
    let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
    assert_eq!(set.images.len(), 4, "four discs");
    Some(Game::open(Arc::new(set)).expect("the game from its images"))
}

#[test]
fn french_cd_iso_images() {
    if let Some(g) = open("ATLANTIS_ISO_FR") {
        assert_eq!(g.exe.build.name, "atlantis-cd-fr-688640");
        assert!(g.big.contains("WAV\\ATA0001A.APC"));
    }
}

#[test]
fn english_cd_raw_images() {
    if let Some(g) = open("ATLANTIS_BIN_EN") {
        assert_eq!(g.exe.build.name, "atlantis-cd-en-688128");
        let data = g
            .big
            .read("WAV\\ATA0001A.APC")
            .expect("a voice from disc 1..4");
        assert!(data.len() > 1000);
    }
}
