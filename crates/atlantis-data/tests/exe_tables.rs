//! The executable's sound tables against the original atlantis.exe: the
//! music track names (0x00495eec) and the NewSound names (0x00496224).
//!
//! Needs the game (`ATLANTIS_DATA`, `ATLANTIS_DATA_1997` for the English
//! retail CD's 1997 build, `ATLANTIS_DATA_FR1997` for the French retail
//! CD's August 1997 build, `ATLANTIS_DATA_FR1997_MAY` for its May 1997
//! build, `ATLANTIS_DATA_DE1997` for the German retail CD's build and
//! `ATLANTIS_DATA_PATCH101` for a CD install with Cryo's patch 1.01);
//! skipped without it.

mod common;

use common::{
    game, game_1997, game_de1997, game_fr1997, game_fr1997_may, game_patch101, golden_map, md5_hex,
};

fn joined(names: &[String]) -> String {
    names
        .iter()
        .map(|n| n.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn music_and_newsound_tables_match_original() {
    let Some(game) = game("music_and_newsound_tables_match_original") else {
        return;
    };
    // The reference, not the patch build of the same size.
    assert_eq!(game.exe.build.name, "atlantis-cd-en-714240");
    check_sound_tables(&game);
}

fn check_sound_tables(game: &atlantis_data::Game) {
    let golden = golden_map(include_str!("golden/scripts_music.txt"));
    let exe = &game.exe;
    assert_eq!(exe.music_names.len().to_string(), golden["music_count"]);
    assert_eq!(
        md5_hex(joined(&exe.music_names).as_bytes()),
        golden["music_names"]
    );
    assert_eq!(
        exe.newsound_names.len().to_string(),
        golden["newsound_count"]
    );
    assert_eq!(
        md5_hex(joined(&exe.newsound_names).as_bytes()),
        golden["newsound_names"]
    );
}

/// The 1997 build's tables sit at other addresses (DGROUP at 0x00480000,
/// file offset 0x6c400; the NewSound pointers into 0x004a....) and hold the
/// reference's bytes, but for the player row: "New Player", no '#', which
/// is also the label.
#[test]
fn tables_of_the_1997_build() {
    let Some(g97) = game_1997("tables_of_the_1997_build") else {
        return;
    };
    let exe = &g97.exe;
    assert_eq!(exe.build.name, "atlantis-cd-en-688128");
    check_sound_tables(&g97);
    assert_eq!(exe.new_player_marker, b"New Player");
    assert_eq!(exe.new_player_label, b"New Player");
    // 51 inventory objects (placeholders at 39, 46, 47), 9 guests.
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    // As the reference's tables, when both games are there.
    if let Some(reference) = game("tables_of_the_1997_build (reference)") {
        let r = &reference.exe;
        assert_eq!(exe.inventory_object_ids, r.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, r.guest_sprite_ids);
        assert_eq!(exe.credit_widths[0x20..0xb3], r.credit_widths[0x20..0xb3]);
    }
}

/// The French 1997 build (atlantis-cd-fr-687104): DGROUP at 0x00480000,
/// file offset 0x6ba00; the reference's bytes but for the player row
/// ("NOUVEAU JOUEUR", no '#', also the label) and the credits table past
/// 0xb2 and at 0x1c..0x1f, which are the English 1997 build's.
#[test]
fn tables_of_the_french_1997_build() {
    let Some(fr) = game_fr1997("tables_of_the_french_1997_build") else {
        return;
    };
    let exe = &fr.exe;
    assert_eq!(exe.build.name, "atlantis-cd-fr-687104");
    check_sound_tables(&fr);
    assert_eq!(exe.new_player_marker, b"NOUVEAU JOUEUR");
    assert_eq!(exe.new_player_label, b"NOUVEAU JOUEUR");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    // The credits bytes the FONTMIN drawers reach for 0xa1, 0xa3, 0xa4.
    assert_eq!(
        [
            exe.credit_widths[0x1c],
            exe.credit_widths[0x1e],
            exe.credit_widths[0x1f]
        ],
        [17, 2, 10]
    );
    if let Some(reference) = game("tables_of_the_french_1997_build (reference)") {
        let r = &reference.exe;
        assert_eq!(exe.inventory_object_ids, r.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, r.guest_sprite_ids);
        assert_eq!(exe.credit_widths[0x20..0xb3], r.credit_widths[0x20..0xb3]);
    }
    if let Some(en) = game_1997("tables_of_the_french_1997_build (English 1997)") {
        assert_eq!(exe.credit_widths, en.exe.credit_widths);
    }
}

/// The German 1997 build (atlantis-cd-de-687104): the French August
/// build's code and DGROUP layout, its own PE stamp; the player row
/// "NEUER SPIELER" (no '#', also the label), the credits widths one byte
/// lower (0x004866d4), with the French build's bytes up to the padding
/// before the inventory table; the other tables at
/// the French build's addresses, with its bytes.
#[test]
fn tables_of_the_german_1997_build() {
    let Some(de) = game_de1997("tables_of_the_german_1997_build") else {
        return;
    };
    let exe = &de.exe;
    assert_eq!(exe.build.name, "atlantis-cd-de-687104");
    check_sound_tables(&de);
    assert_eq!(exe.new_player_marker, b"NEUER SPIELER");
    assert_eq!(exe.new_player_label, b"NEUER SPIELER");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    // The FONTMIN drawers' bytes for 0xa1, 0xa3, 0xa4 and for `ß` (0xe1,
    // glyph 193: credits byte 92).
    assert_eq!(
        [
            exe.credit_widths[0x1c],
            exe.credit_widths[0x1e],
            exe.credit_widths[0x1f],
            exe.credit_widths[92]
        ],
        [17, 2, 10, 7]
    );
    // Every flag as the French August build's.
    let fr_build = atlantis_data::exe::BUILDS
        .iter()
        .find(|b| b.name == "atlantis-cd-fr-687104")
        .unwrap();
    let mut b = *exe.build;
    b.name = fr_build.name;
    b.pe_timestamp = fr_build.pe_timestamp;
    b.credit_widths_va = fr_build.credit_widths_va;
    assert_eq!(&b, fr_build);
    if let Some(fr) = game_fr1997("tables_of_the_german_1997_build (French August)") {
        let f = &fr.exe;
        assert_eq!(exe.inventory_object_ids, f.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, f.guest_sprite_ids);
        // The French bytes, one address lower, up to the padding before
        // the inventory table (0x00486788 in both builds).
        assert_eq!(exe.credit_widths[..0xb2], f.credit_widths[..0xb2]);
        assert_eq!(exe.music_names, f.music_names);
        assert_eq!(exe.newsound_names, f.newsound_names);
    }
}

/// The French May 1997 build (atlantis-cd-fr-688640): DGROUP at
/// 0x00480000, file offset 0x6c600; the English 1997 build's bytes but for
/// the player row ("NOUVEAU JOUEUR", no '#', also the label, 0x0048654a)
/// and the credits widths (0x004866d9), whose E, b, d, g, n and t are one
/// pixel narrower.
#[test]
fn tables_of_the_french_may_1997_build() {
    let Some(fr) = game_fr1997_may("tables_of_the_french_may_1997_build") else {
        return;
    };
    let exe = &fr.exe;
    assert_eq!(exe.build.name, "atlantis-cd-fr-688640");
    check_sound_tables(&fr);
    assert_eq!(exe.new_player_marker, b"NOUVEAU JOUEUR");
    assert_eq!(exe.new_player_label, b"NOUVEAU JOUEUR");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    let narrow = b"Ebdgnt";
    assert_eq!(
        narrow.map(|c| exe.credit_widths[usize::from(c)]),
        [6, 8, 8, 8, 7, 5]
    );
    assert_eq!(
        [
            exe.credit_widths[0x1c],
            exe.credit_widths[0x1e],
            exe.credit_widths[0x1f]
        ],
        [17, 2, 10]
    );
    if let Some(en) = game_1997("tables_of_the_french_may_1997_build (English 1997)") {
        let e = &en.exe;
        assert_eq!(exe.inventory_object_ids, e.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, e.guest_sprite_ids);
        for c in 0..256 {
            let less = u8::from(narrow.contains(&(c as u8)));
            assert_eq!(
                exe.credit_widths[c] + less,
                e.credit_widths[c],
                "byte {c:#x}"
            );
        }
    }
}

/// The patch 1.01 build (atlantis-patch-fr-714240): the reference's size,
/// told apart by its PE stamp; DGROUP laid out as the reference's, the
/// tables at the GOG build's addresses (the credits widths, inventory and
/// guest tables 4 bytes further) with the reference's bytes, but for the
/// player row: "#NOUVEAU JOUEUR", then the label "NOUVEAU JOUEUR".
#[test]
fn tables_of_the_patch_101_build() {
    let Some(p) = game_patch101("tables_of_the_patch_101_build") else {
        return;
    };
    let exe = &p.exe;
    assert_eq!(exe.build.name, "atlantis-patch-fr-714240");
    check_sound_tables(&p);
    assert_eq!(exe.new_player_marker, b"#NOUVEAU JOUEUR");
    assert_eq!(exe.new_player_label, b"NOUVEAU JOUEUR");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    // The 1999 code: as the GOG build's.
    let b = exe.build;
    assert!(!b.cursor_id_rule && !b.rhea_camera2 && b.alt_game_over_narration);
    if let Some(reference) = game("tables_of_the_patch_101_build (reference)") {
        let r = &reference.exe;
        assert_eq!(exe.inventory_object_ids, r.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, r.guest_sprite_ids);
        assert_eq!(exe.credit_widths, r.credit_widths);
        assert_eq!(exe.music_names, r.music_names);
        assert_eq!(exe.newsound_names, r.newsound_names);
    }
    if let Ok(gog) = std::env::var("ATLANTIS_GOG_FR") {
        let g = atlantis_data::Game::open(std::sync::Arc::new(atlantis_data::StdDir::new(gog)))
            .unwrap();
        let g = &g.exe;
        assert_eq!(g.build.name, "atlantis-gog-714752");
        assert_eq!(exe.new_player_marker, g.new_player_marker);
        assert_eq!(exe.new_player_label, g.new_player_label);
        assert_eq!(exe.inventory_object_ids, g.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, g.guest_sprite_ids);
        assert_eq!(exe.credit_widths, g.credit_widths);
    }
}
