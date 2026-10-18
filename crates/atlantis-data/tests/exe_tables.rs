//! The executable's sound tables against the original atlantis.exe: the
//! music track names (0x00495eec) and the NewSound names (0x00496224).
//!
//! Needs the game (`ATLANTIS_DATA`, `ATLANTIS_DATA_1997` for the English
//! retail CD's 1997 build and `ATLANTIS_DATA_FR1997` for the French retail
//! CD's August 1997 build); skipped without it.

mod common;

use common::{game, game_1997, game_fr1997, golden_map, md5_hex};

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
