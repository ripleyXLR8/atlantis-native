//! The game read straight from its CD images: the French, German and
//! Swedish CDs' ISO images (2048-byte sectors), the English and Dutch CDs'
//! raw images and the Spanish CD's Alcohol 120% `.mdf` images (2352-byte
//! sectors), the Italian CD's Nero `.nrg` images (2048-byte sectors after
//! a 307200-byte lead-in).
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

#[test]
fn german_cd_iso_images() {
    if let Some(g) = open("ATLANTIS_ISO_DE") {
        assert_eq!(g.exe.build.name, "atlantis-cd-de-687104");
        assert_eq!(g.edition().to_string(), "CD, German (August 1997 build)");
        assert!(g.big.contains("SPRLIST\\EPI.TXT"));
    }
}

#[test]
fn dutch_cd_raw_images() {
    if let Some(g) = open("ATLANTIS_BIN_NL") {
        assert_eq!(g.exe.build.name, "atlantis-cd-nl-688128");
        assert_eq!(g.edition().to_string(), "CD, Dutch (June 1997 build)");
        assert!(g.big.contains("UBB_VUE\\LOGORP.HNM"));
        // Disc 2's own revision of CREDITS.TXT, read from its image.
        g.big.set_disc_copy(Some(2));
        let disc2 = g.big.read("SPRLIST\\CREDITS.TXT").unwrap();
        g.big.set_disc_copy(None);
        let disc1 = g.big.read("SPRLIST\\CREDITS.TXT").unwrap();
        assert_eq!((disc1.len(), disc2.len()), (5034, 5031));
    }
}

#[test]
fn swedish_cd_iso_images() {
    if let Some(g) = open("ATLANTIS_ISO_SV") {
        assert_eq!(g.exe.build.name, "atlantis-cd-sv-688128");
        assert_eq!(g.edition().to_string(), "CD, Swedish (June 1997 build)");
        assert!(!g.big.contains("UBB_VUE\\LOGORP.HNM"));
        // Disc 2's own revision of SELEMENU.TXT, read from its image.
        g.big.set_disc_copy(Some(2));
        let disc2 = g.big.read("SPRLIST\\SELEMENU.TXT").unwrap();
        g.big.set_disc_copy(None);
        let disc1 = g.big.read("SPRLIST\\SELEMENU.TXT").unwrap();
        assert!(disc1.windows(11).any(|w| w == b"SPELARNAMN\r"));
        assert!(disc2.windows(11).any(|w| w == b"SPELARNAMM\r"));
    }
}

#[test]
fn spanish_cd_mdf_images() {
    if let Some(g) = open("ATLANTIS_MDF_ES") {
        assert_eq!(g.exe.build.name, "atlantis-cd-es-687104");
        assert_eq!(g.edition().to_string(), "CD, Spanish (August 1997 build)");
        assert_eq!(g.exe.new_player_marker, b"Jugador nuevo");
        let data = g.big.read("WAV\\ATA0001A.APC").expect("a voice");
        assert!(data.len() > 1000);
    }
}

#[test]
fn italian_cd_nrg_images() {
    if let Some(g) = open("ATLANTIS_NRG_IT") {
        assert_eq!(g.exe.build.name, "atlantis-cd-it-687104");
        assert_eq!(g.edition().to_string(), "CD, Italian (July 1997 build)");
        assert_eq!(g.exe.new_player_marker, b"Nuovo Giocatore");
        assert!(g.big.contains("IMAGES\\END.TGA"));
        let data = g.big.read("WAV\\ATA0001A.APC").expect("a voice");
        assert!(data.len() > 1000);
    }
}
