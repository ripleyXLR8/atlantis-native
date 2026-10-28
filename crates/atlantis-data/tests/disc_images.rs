//! The game read straight from its CD images: the French, German (both
//! pressings) and Swedish CDs' ISO images (2048-byte sectors), the English and Dutch CDs'
//! raw images and the Spanish CD's Alcohol 120% `.mdf` images (2352-byte
//! sectors), the Italian CD's Nero `.nrg` images (2048-byte sectors after
//! a 307200-byte lead-in), the Traditional Chinese CD's ISO images and the
//! Simplified Chinese CD's raw images.
mod common;

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

/// The German CD's first pressing: its four ISO images (only the `.iso`
/// files of the folder `ATLANTIS_DATA_DE_MAY97` names), its May 1997 build
/// and its discs' archives.
#[test]
fn german_may_cd_iso_images() {
    let Some(g) = common::game_de_may97("german_may_cd_iso_images") else {
        return;
    };
    assert_eq!(g.exe.build.name, "atlantis-cd-de-693248");
    assert_eq!(g.edition().to_string(), "CD, German (May 1997 build)");
    assert!(g.big.contains("WAV\\ATA0897D.APC"));
    let data = g.big.read("WAV\\ATA0001A.APC").expect("a voice");
    assert!(data.len() > 1000);
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

/// The Chinese builds' tables against the reference's, which they hold
/// but for the player row and the two LOAD tables (when `ATLANTIS_DATA`,
/// the reference's folder, is set too).
fn check_chinese_tables(g: &Game) {
    let exe = &g.exe;
    // "New Player" without '#' (0x004865a8), the label itself.
    assert_eq!(exe.new_player_marker, b"New Player");
    assert_eq!(exe.new_player_label, b"New Player");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    // 0x00486619: 74 chapters, 2..85 ascending; 0x00486663: the widths of
    // EPICHI1's frames.
    assert_eq!(exe.load_chapters.len(), 74);
    assert_eq!(exe.load_chapters[..6], [2, 3, 4, 5, 6, 9]);
    assert_eq!(*exe.load_chapters.last().unwrap(), 85);
    assert!(exe.load_chapters.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(exe.load_widths.len(), 74);
    assert_eq!(exe.load_widths[..6], [80, 98, 139, 140, 160, 139]);
    if let Ok(dir) = std::env::var("ATLANTIS_DATA") {
        let r = Game::open(Arc::new(atlantis_data::StdDir::new(dir))).unwrap();
        let r = &r.exe;
        assert_eq!(exe.music_names, r.music_names);
        assert_eq!(exe.newsound_names, r.newsound_names);
        assert_eq!(exe.inventory_object_ids, r.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, r.guest_sprite_ids);
        assert_eq!(exe.credit_widths, r.credit_widths);
    }
    // The sprites the build loads besides the reference's.
    for s in ["SPRCHI1", "SPRCHI2", "EPICHI1", "EPICHI2", "SUBCHINE"] {
        assert!(g.big.contains(&format!("SPRITE\\2D\\{s}.SPR")), "{s}");
    }
}

/// The non-ASCII characters of the CON scripts in the release's code page
/// and the bytes or pairs it has no character for (the files' NUL bytes
/// not counted).
fn con_characters(g: &Game) -> (std::collections::BTreeSet<char>, usize) {
    let cp = g.exe.build.text_code_page;
    let mut chars = std::collections::BTreeSet::new();
    let mut undecoded = 0;
    let names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.to_ascii_uppercase().ends_with(".CON"))
        .map(str::to_string)
        .collect();
    assert!(names.len() > 70);
    for n in names {
        let bytes = g.big.read(&n).unwrap();
        let text = cp.decode(&bytes);
        undecoded += text.chars().filter(|&c| c == '\0').count();
        undecoded -= bytes.iter().filter(|&&b| b == 0).count();
        chars.extend(text.chars().filter(|c| !c.is_ascii()));
    }
    (chars, undecoded)
}

#[test]
fn traditional_chinese_cd_iso_images() {
    if let Some(g) = open("ATLANTIS_ISO_ZH_TW") {
        assert_eq!(g.exe.build.name, "atlantis-cd-zh-tw-696832");
        assert_eq!(
            g.edition().to_string(),
            "CD, Traditional Chinese (1998 build)"
        );
        check_chinese_tables(&g);
        let (chars, undecoded) = con_characters(&g);
        // The Big5 dialog texts: '，' and '。' among them.
        assert!(chars.contains(&'，') && chars.contains(&'。'));
        assert!(chars.len() > 1400, "{}", chars.len());
        // Only the stray binary bytes of two lines: one 0xff in CHAPI004,
        // three in CHAPI031.
        assert_eq!(undecoded, 4);
    }
}

/// The Simplified release's file names are lower case on its discs
/// (`bigcd1.big`...), its images raw (2352-byte sectors).
#[test]
fn simplified_chinese_cd_raw_images() {
    if let Some(g) = open("ATLANTIS_BIN_ZH_CN") {
        assert_eq!(g.exe.build.name, "atlantis-cd-zh-cn-692736");
        assert_eq!(
            g.edition().to_string(),
            "CD, Simplified Chinese (1999 build)"
        );
        check_chinese_tables(&g);
        let (chars, undecoded) = con_characters(&g);
        assert!(chars.contains(&'，') && chars.contains(&'\u{3000}'));
        // The GBK pairs outside GB2312 (`85 85`, `97 97`, `A8 44`).
        assert!(chars.contains(&'厖') && chars.contains(&'棗') && chars.contains(&'―'));
        assert!(chars.len() > 1400, "{}", chars.len());
        assert_eq!(undecoded, 0);
    }
}

/// ScummVM's detection keys of a release's files (the md5 of the first
/// 5000 bytes of `atlantis.exe` and of `BIGCD1.BIG`, the archive's size):
/// the files the port tells apart by the executable's size and PE
/// timestamp are those of these keys.
fn detection_keys(var: &str) -> Option<(String, String, u64)> {
    use atlantis_data::FileSource;
    use md5::{Digest, Md5};
    let dir = std::env::var(var).ok()?;
    let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
    let head = |name: &str| {
        let bytes = set.read_at(name, 0, 5000).expect(name);
        format!("{:x}", Md5::digest(bytes))
    };
    Some((
        head("atlantis.exe"),
        head("BIGCD1.BIG"),
        set.size("BIGCD1.BIG").unwrap(),
    ))
}

#[test]
fn chinese_cd_detection_keys() {
    if let Some(k) = detection_keys("ATLANTIS_ISO_ZH_TW") {
        assert_eq!(
            k,
            (
                "f40f1e6a240730d325168f4224da8457".into(),
                "7f65366bdac230d7735250a22c98d01e".into(),
                636_420_567
            )
        );
    }
    if let Some(k) = detection_keys("ATLANTIS_BIN_ZH_CN") {
        assert_eq!(
            k,
            (
                "591701abaaaa4efa3751da60a42fb7ad".into(),
                "920fff5d0adda30f14a5699554b4f529".into(),
                636_421_642
            )
        );
    }
}
