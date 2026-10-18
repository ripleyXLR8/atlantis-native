//! The icon read from the player's atlantis.exe.
mod common;

use atlantis_data::exe_icon::exe_icon;

#[test]
fn reference_and_gog_icons() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: skipped");
        return;
    };
    let exe = std::fs::read(format!("{dir}/atlantis.exe")).unwrap();
    let icon = exe_icon(&exe).expect("an icon");
    assert_eq!((icon.width, icon.height), (32, 32));
    assert_eq!(icon.rgba.len(), 32 * 32 * 4);
    let opaque = icon.rgba.chunks(4).filter(|p| p[3] == 255).count();
    let clear = icon.rgba.chunks(4).filter(|p| p[3] == 0).count();
    assert!(
        opaque > 100 && opaque + clear == 32 * 32,
        "{opaque} opaque, {clear} clear"
    );
    if let Ok(gog) = std::env::var("ATLANTIS_GOG") {
        let g = std::fs::read(format!("{gog}/fr-FR/Atlantis.exe")).unwrap();
        assert_eq!(exe_icon(&g), Some(icon));
    }
}

#[test]
fn icon_of_the_1997_build() {
    let Ok(dir) = std::env::var("ATLANTIS_DATA_1997") else {
        eprintln!("ATLANTIS_DATA_1997 not set: skipped");
        return;
    };
    let exe = std::fs::read(format!("{dir}/atlantis.exe")).unwrap();
    let icon = exe_icon(&exe).expect("an icon");
    assert_eq!((icon.width, icon.height), (32, 32));
}

#[test]
fn icon_of_the_french_1997_build() {
    let Some(g) = common::game_fr1997("icon_of_the_french_1997_build") else {
        return;
    };
    let icon = g.icon.expect("an icon");
    assert_eq!((icon.width, icon.height), (32, 32));
}

#[test]
fn icon_of_the_french_may_1997_build() {
    let Some(g) = common::game_fr1997_may("icon_of_the_french_may_1997_build") else {
        return;
    };
    let icon = g.icon.expect("an icon");
    assert_eq!((icon.width, icon.height), (32, 32));
}
