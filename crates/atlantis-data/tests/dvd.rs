//! Cryo's DVD-ROM of 1998 and the Perfect Edition's program: each European
//! language of the DVD opened through its side's folders (a copy of each
//! side, then the images of the sides), its build, edition and tables; the
//! Perfect Edition's loose folder.
//!
//! Needs the DVD: `ATLANTIS_DVD_A_FILES` / `ATLANTIS_DVD_B_FILES` (copies
//! of side A and side B: the folders holding `FACE_A` / `FACE_B`),
//! `ATLANTIS_DVD_A` / `ATLANTIS_DVD_B` (the images of the sides, `.iso`
//! files), `ATLANTIS_PERFECT_EDITION` (its installed folder); `ATLANTIS_DATA`
//! (the reference) for the tables compared with the reference's. Each test
//! says what it skips.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::dvd::{self, DvdFiles, DvdLanguage};
use atlantis_data::{FileSource, Game, GameError, StdDir};

fn var(name: &str) -> Option<PathBuf> {
    let v = std::env::var_os(name).map(PathBuf::from);
    if v.is_none() {
        eprintln!("{name} not set: skipped");
    }
    v
}

/// (folder, build, edition, player row marker, label)
const EUROPEAN: [(&str, &str, &str, &str, &str); 7] = [
    (
        "ALL",
        "atlantis-dvd-de-695296",
        "DVD, German (1998 build)",
        "#NEUER SPIELER",
        "NEUER SPIELER",
    ),
    (
        "ENG",
        "atlantis-dvd-en-695296",
        "DVD, English (1998 build)",
        "#New Player",
        "New Player",
    ),
    (
        "FRA",
        "atlantis-dvd-fr-695296",
        "DVD, French (1998 build)",
        "#NOUVEAU JOUEUR",
        "NOUVEAU JOUEUR",
    ),
    (
        "HOL",
        "atlantis-dvd-nl-695808",
        "DVD, Dutch (1998 build)",
        "#Nieuwe speler",
        "Nieuwe speler",
    ),
    (
        "ESP",
        "atlantis-dvd-es-695296",
        "DVD, Spanish (1998 build)",
        "#Jugador nuevo",
        "Jugador nuevo",
    ),
    (
        "ITA",
        "atlantis-dvd-it-695296",
        "DVD, Italian (1998 build)",
        "#Nuovo Giocatore",
        "Nuovo Giocatore",
    ),
    (
        "SUE",
        "atlantis-dvd-sv-695808",
        "DVD, Swedish (1998 build)",
        "#Ny spelare",
        "Ny spelare",
    ),
];

fn language(folder: &str) -> &'static DvdLanguage {
    dvd::LANGUAGES.iter().find(|l| l.folder == folder).unwrap()
}

/// Each language of `src` (the disc's root) as the game of its build.
fn check_languages(src: Arc<dyn FileSource>, sides: &[char]) {
    for (folder, build, edition, marker, label) in EUROPEAN {
        let l = language(folder);
        if !sides.contains(&l.side) {
            continue;
        }
        let files = DvdFiles::new(src.clone(), l).expect(folder);
        let g = Game::open(Arc::new(files)).unwrap_or_else(|e| panic!("{folder}: {e}"));
        assert_eq!(g.exe.build.name, build);
        assert_eq!(g.edition().to_string(), edition);
        assert_eq!(g.exe.build.dvd_side(), Some(l.side), "{folder}");
        assert!(g.unpacked, "{folder}: the loose files");
        assert_eq!(g.exe.new_player_marker, marker.as_bytes(), "{folder}");
        assert_eq!(g.exe.new_player_label, label.as_bytes(), "{folder}");
        // The shared folders from the side, the scripts from the language.
        assert!(g.big.contains("UBB_VUE\\LOGO15N.HNM"), "{folder}");
        assert!(g.big.contains("CYCLO\\ATL16001.HNM"), "{folder}");
        assert!(g.big.contains("SCENAR\\CHAPI001.CON"), "{folder}");
        assert!(g.big.contains("DIALOG\\PERSO\\INITDIAL.TXT"), "{folder}");
        assert!(
            g.big.contains("UBB_VUE\\LOGORP.HNM"),
            "{folder}: on every side"
        );
    }
}

#[test]
fn every_european_language_from_copies_of_the_sides() {
    let (Some(a), Some(b)) = (var("ATLANTIS_DVD_A_FILES"), var("ATLANTIS_DVD_B_FILES")) else {
        return;
    };
    for (dir, side) in [(a, 'A'), (b, 'B')] {
        let src: Arc<dyn FileSource> = Arc::new(StdDir::new(&dir));
        let found: Vec<&str> = dvd::languages_in(src.as_ref())
            .iter()
            .map(|l| l.folder)
            .collect();
        let want: Vec<&str> = dvd::LANGUAGES
            .iter()
            .filter(|l| l.side == side)
            .map(|l| l.folder)
            .collect();
        assert_eq!(found, want, "side {side}");
        check_languages(src, &[side]);
    }
}

#[test]
fn every_european_language_from_the_images() {
    let (Some(a), Some(b)) = (var("ATLANTIS_DVD_A"), var("ATLANTIS_DVD_B")) else {
        return;
    };
    // The images of both sides in one folder (links to them).
    let dir = std::env::temp_dir().join(format!("atlantis-dvd-images-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&a, dir.join("Atlantis DVD side A.iso")).unwrap();
    }
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&a, dir.join("Atlantis DVD side A.iso")).unwrap();
    // One side: its four languages, the other side's absent.
    let one: Arc<dyn FileSource> = Arc::new(DiscSet::open_folder(&dir).expect("side A's image"));
    assert_eq!(dvd::languages_in(one.as_ref()).len(), 4);
    assert!(dvd::has_side(one.as_ref(), 'A') && !dvd::has_side(one.as_ref(), 'B'));
    assert!(DvdFiles::new(one, language("ESP")).is_none());
    #[cfg(unix)]
    std::os::unix::fs::symlink(&b, dir.join("Atlantis DVD side B.iso")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&b, dir.join("Atlantis DVD side B.iso")).unwrap();
    let set = DiscSet::open_folder(&dir).expect("the images");
    assert_eq!(set.images.len(), 2);
    let src: Arc<dyn FileSource> = Arc::new(set);
    assert_eq!(dvd::languages_in(src.as_ref()).len(), 8);
    // The ISO 9660 tree lists the language folders' files.
    assert!(!src.list("FACE_A/ENG/SPRLIST").is_empty());
    check_languages(src, &['A', 'B']);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The tables of each language: the music and sound names, the inventory
/// and guest ids and the player row as the reference's but the row; the
/// credits widths the reference's up to the inventory table that follows
/// them (the reference's 180 bytes; 177 in the German, Dutch and Spanish
/// builds, 179 in the Italian, whose strings leave the widths 3 or 1 bytes
/// later: the original reads the inventory's bytes for the codes past
/// them, as the port does).
#[test]
fn tables_of_the_dvd_builds() {
    let (Some(a), Some(b)) = (var("ATLANTIS_DVD_A_FILES"), var("ATLANTIS_DVD_B_FILES")) else {
        return;
    };
    let reference = common::game("tables_of_the_dvd_builds (against the reference)");
    for (folder, build, ..) in EUROPEAN {
        let l = language(folder);
        let root = if l.side == 'A' { &a } else { &b };
        let files = DvdFiles::new(Arc::new(StdDir::new(root)), l).unwrap();
        let g = Game::open(Arc::new(files)).unwrap();
        let exe = &g.exe;
        assert_eq!(exe.build.name, build);
        assert_eq!(exe.music_names.len(), 31, "{build}");
        assert_eq!(exe.music_names[0], "01atlan1.apc", "{build}");
        assert!(exe.newsound_names.len() > 100, "{build}");
        let Some(r) = &reference else { continue };
        let r = &r.exe;
        assert_eq!(exe.music_names, r.music_names, "{build}");
        assert_eq!(exe.newsound_names, r.newsound_names, "{build}");
        assert_eq!(exe.inventory_object_ids, r.inventory_object_ids, "{build}");
        assert_eq!(exe.guest_sprite_ids, r.guest_sprite_ids, "{build}");
        let own = match folder {
            "ALL" | "HOL" | "ESP" => 177,
            "ITA" => 179,
            _ => 256,
        };
        assert_eq!(exe.credit_widths[..own], r.credit_widths[..own], "{build}");
    }
}

/// A language folder copied alone (without its side's shared folders) is
/// refused with its own error.
#[test]
fn a_language_folder_alone() {
    let Some(a) = var("ATLANTIS_DVD_A_FILES") else {
        return;
    };
    let alone = StdDir::new(a.join("FACE_A").join("ENG"));
    assert!(matches!(
        Game::open(Arc::new(alone)),
        Err(GameError::DvdSharedFolders)
    ));
}

/// The Perfect Edition: the English DVD's program with relative paths,
/// told apart from it by the films' folder string.
#[test]
fn perfect_edition() {
    let Some(dir) = var("ATLANTIS_PERFECT_EDITION") else {
        return;
    };
    let g = Game::open(Arc::new(StdDir::new(&dir))).expect("the Perfect Edition");
    assert_eq!(g.exe.build.name, "atlantis-pe-2018-695296");
    assert_eq!(
        g.edition().to_string(),
        "Perfect Edition, English (1998 build)"
    );
    assert_eq!(g.exe.build.dvd_side(), None);
    assert!(g.unpacked);
    assert!(g.big.contains("UBB_VUE\\LOGO15N.HNM"));
    if let Some(a) = std::env::var_os("ATLANTIS_DVD_A_FILES") {
        let en = DvdFiles::new(Arc::new(StdDir::new(a)), language("ENG")).unwrap();
        let dvd = Game::open(Arc::new(en)).unwrap();
        assert_eq!(dvd.exe.build.name, "atlantis-dvd-en-695296");
        // One program, one size and stamp: the same tables.
        assert_eq!(dvd.exe.music_names, g.exe.music_names);
        assert_eq!(dvd.exe.newsound_names, g.exe.newsound_names);
        assert_eq!(dvd.exe.credit_widths, g.exe.credit_widths);
        assert_eq!(dvd.exe.inventory_object_ids, g.exe.inventory_object_ids);
    }
}

/// The Japanese build is the next part's: not known yet.
#[test]
fn japanese_not_yet() {
    let Some(b) = var("ATLANTIS_DVD_B_FILES") else {
        return;
    };
    let files = DvdFiles::new(Arc::new(StdDir::new(b)), language("JAP")).unwrap();
    assert!(matches!(
        Game::open(Arc::new(files)),
        Err(GameError::UnknownExecutable(696_832))
    ));
}
