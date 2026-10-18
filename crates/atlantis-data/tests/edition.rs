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

#[test]
fn english_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_1997") else {
        eprintln!("ATLANTIS_DATA_1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, English (May 1997 build)");
}

#[test]
fn french_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_FR1997") else {
        eprintln!("ATLANTIS_DATA_FR1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, French (August 1997 build)");
}

#[test]
fn french_cd_may_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_FR1997_MAY") else {
        eprintln!("ATLANTIS_DATA_FR1997_MAY not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, French (May 1997 build)");
}

#[test]
fn german_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_DE1997") else {
        eprintln!("ATLANTIS_DATA_DE1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, German (August 1997 build)");
}

#[test]
fn dutch_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_NL1997") else {
        eprintln!("ATLANTIS_DATA_NL1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, Dutch (June 1997 build)");
}

/// The patch 1.01 executable installed over the Dutch CD
/// (`ATLANTIS_DATA_PATCH101_NL`: the patch's atlantis.exe beside the Dutch
/// BIGCD1..4.BIG): the patch's build, its interface French.
#[test]
fn dutch_cd_with_patch_101() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_PATCH101_NL") else {
        eprintln!("ATLANTIS_DATA_PATCH101_NL not set: skipped");
        return;
    };
    let g = Game::open(Arc::new(StdDir::new(&dir))).unwrap();
    assert_eq!(g.exe.build.name, "atlantis-patch-fr-714240");
    assert_eq!(
        g.edition().to_string(),
        "CD, patch 1.01, French (1999 build)"
    );
    // The Dutch archives (the R&P logo only they carry), which the patch's
    // start-up does not play.
    assert!(g.big.contains("UBB_VUE\\LOGORP.HNM"));
    assert!(!g.exe.build.startup_logo_rp);
}

#[test]
fn swedish_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_SV1997") else {
        eprintln!("ATLANTIS_DATA_SV1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, Swedish (June 1997 build)");
}

#[test]
fn spanish_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_ES1997") else {
        eprintln!("ATLANTIS_DATA_ES1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, Spanish (August 1997 build)");
}

#[test]
fn italian_cd_1997() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_IT1997") else {
        eprintln!("ATLANTIS_DATA_IT1997 not set: skipped");
        return;
    };
    assert_eq!(edition(&dir).unwrap(), "CD, Italian (July 1997 build)");
}

/// Whether one of the archive's menu files holds `word`.
fn menus_hold(g: &Game, word: &[u8]) -> bool {
    ["MAINMENU", "LOADMENU", "OPTMENU", "SUREMENU", "SONMENU"]
        .iter()
        .filter_map(|m| g.big.read(&format!("SPRLIST\\{m}.TXT")))
        .any(|t| t.windows(word.len()).any(|w| w == word))
}

/// The patch 1.01 executable installed over the Spanish and Italian CDs
/// (`ATLANTIS_DATA_PATCH101_ES` / `_IT`: the patch's atlantis.exe beside
/// that CD's BIGCD1..4.BIG): the patch's build, its interface French, the
/// CD's own menus.
#[test]
fn spanish_italian_cds_with_patch_101() {
    for (var, word) in [
        ("ATLANTIS_DATA_PATCH101_ES", &b"Cancelar"[..]),
        ("ATLANTIS_DATA_PATCH101_IT", &b"Annulla"[..]),
    ] {
        let Ok(dir) = std::env::var(var) else {
            eprintln!("{var} not set: skipped");
            continue;
        };
        let g = Game::open(Arc::new(StdDir::new(&dir))).unwrap();
        assert_eq!(g.exe.build.name, "atlantis-patch-fr-714240");
        assert_eq!(
            g.edition().to_string(),
            "CD, patch 1.01, French (1999 build)"
        );
        assert_eq!(g.exe.build.menu_positioned_byte_width, 22);
        assert!(menus_hold(&g, word), "{var}");
    }
}

/// The patch 1.01 executable installed over the Swedish CD
/// (`ATLANTIS_DATA_PATCH101_SV`: the patch's atlantis.exe beside the
/// Swedish BIGCD1..4.BIG): the patch's build, its interface French, its
/// name entry and subtitles its own.
#[test]
fn swedish_cd_with_patch_101() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_PATCH101_SV") else {
        eprintln!("ATLANTIS_DATA_PATCH101_SV not set: skipped");
        return;
    };
    let g = Game::open(Arc::new(StdDir::new(&dir))).unwrap();
    assert_eq!(g.exe.build.name, "atlantis-patch-fr-714240");
    assert_eq!(
        g.edition().to_string(),
        "CD, patch 1.01, French (1999 build)"
    );
    assert!(!g.exe.build.name_entry_dos_letters && !g.exe.build.subtitle_cut_back_only);
    // The Swedish archives: SELEMENU.TXT's "VÄLJ SPELARNAMN" (0x8e Ä).
    let sel = g.big.read("SPRLIST\\SELEMENU.TXT").unwrap();
    assert!(sel.windows(15).any(|w| w == b"V\x8eLJ SPELARNAMN"));
}

/// The patch 1.01 executable installed over the German CD
/// (`ATLANTIS_DATA_PATCH101_DE`: the patch's atlantis.exe beside the German
/// BIGCD1..4.BIG): the patch's build, its interface French.
#[test]
fn german_cd_with_patch_101() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_PATCH101_DE") else {
        eprintln!("ATLANTIS_DATA_PATCH101_DE not set: skipped");
        return;
    };
    let g = Game::open(Arc::new(StdDir::new(&dir))).unwrap();
    assert_eq!(g.exe.build.name, "atlantis-patch-fr-714240");
    assert_eq!(
        g.edition().to_string(),
        "CD, patch 1.01, French (1999 build)"
    );
    // The German archives: EPI.TXT spells "verläßt" in the DOS code page.
    let epi = g.big.read("SPRLIST\\EPI.TXT").unwrap();
    assert!(epi.windows(7).any(|w| w == b"verl\x84\xe1t"));
}

#[test]
fn cd_with_patch_101() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_PATCH101") else {
        eprintln!("ATLANTIS_DATA_PATCH101 not set: skipped");
        return;
    };
    assert_eq!(
        edition(&dir).unwrap(),
        "CD, patch 1.01, French (1999 build)"
    );
}
