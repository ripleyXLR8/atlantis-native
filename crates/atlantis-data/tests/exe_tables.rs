//! The executable's sound tables against the original atlantis.exe: the
//! music track names (0x00495eec) and the NewSound names (0x00496224).
//!
//! Needs the game (`ATLANTIS_DATA`, `ATLANTIS_DATA_1997` for the English
//! retail CD's 1997 build, `ATLANTIS_DATA_FR1997` for the French retail
//! CD's August 1997 build, `ATLANTIS_DATA_FR1997_MAY` for its May 1997
//! build, `ATLANTIS_DATA_DE1997` for the German retail CD's build,
//! `ATLANTIS_DATA_NL1997` for the Dutch retail CD's build,
//! `ATLANTIS_DATA_SV1997` for the Swedish retail CD's build,
//! `ATLANTIS_DATA_ES1997` / `ATLANTIS_DATA_IT1997` for the Spanish and
//! Italian retail CDs' builds and `ATLANTIS_DATA_PATCH101` for a CD install with Cryo's patch 1.01);
//! skipped without it.

mod common;

use common::{
    game, game_1997, game_de1997, game_es1997, game_fr1997, game_fr1997_may, game_it1997,
    game_nl1997, game_patch101, game_sv1997, golden_map, md5_hex,
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

/// The Dutch 1997 build (atlantis-cd-nl-688128): the English May build's
/// code and DGROUP layout (0x00480000, file offset 0x6c400), its own PE
/// stamp; the player row "Nieuwe speler" (no '#', also the label, at
/// 0x0048654a), then the credits widths 3 bytes further (0x004866d8) and
/// the inventory and guest tables 4 bytes further (0x0048678c, 0x00486860),
/// with the English build's bytes. Every flag is en-688128's but the name
/// entry (0x20..0xa7, case kept) and the R&P logo at start-up.
#[test]
fn tables_of_the_dutch_1997_build() {
    let Some(nl) = game_nl1997("tables_of_the_dutch_1997_build") else {
        return;
    };
    let exe = &nl.exe;
    assert_eq!(exe.build.name, "atlantis-cd-nl-688128");
    check_sound_tables(&nl);
    assert_eq!(exe.new_player_marker, b"Nieuwe speler");
    assert_eq!(exe.new_player_label, b"Nieuwe speler");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    assert_eq!(
        [
            exe.credit_widths[0x1c],
            exe.credit_widths[0x1e],
            exe.credit_widths[0x1f]
        ],
        [17, 2, 10]
    );
    let en_build = atlantis_data::exe::BUILDS
        .iter()
        .find(|b| b.name == "atlantis-cd-en-688128")
        .unwrap();
    let mut b = *exe.build;
    assert!(b.startup_logo_rp && !b.upper_case_names && b.players_1997);
    assert_eq!(b.month, Some("June"));
    assert_eq!(b.text_code_page, atlantis_data::TextCodePage::Dos850);
    b.name = en_build.name;
    b.pe_timestamp = en_build.pe_timestamp;
    b.month = en_build.month;
    b.inventory_va = en_build.inventory_va;
    b.guest_va = en_build.guest_va;
    b.credit_widths_va = en_build.credit_widths_va;
    b.upper_case_names = en_build.upper_case_names;
    b.startup_logo_rp = en_build.startup_logo_rp;
    b.text_code_page = en_build.text_code_page;
    // 20 records per menu (the menu walk's `imul eax,eax,0xa1` @
    // 0x0041efa4), not the English build's 16.
    b.menu_records_per_menu = en_build.menu_records_per_menu;
    assert_eq!(&b, en_build);
    if let Some(en) = game_1997("tables_of_the_dutch_1997_build (English 1997)") {
        let e = &en.exe;
        assert_eq!(exe.inventory_object_ids, e.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, e.guest_sprite_ids);
        // Up to the padding before the inventory table.
        assert_eq!(exe.credit_widths[..0xb3], e.credit_widths[..0xb3]);
        assert_eq!(exe.music_names, e.music_names);
        assert_eq!(exe.newsound_names, e.newsound_names);
    }
}

/// The Dutch CD's archives: the members several discs carry are the same
/// bytes on each, but for an older revision on disc 2 of the menus, CD and
/// error texts, EPI.TXT, CREDITS.TXT, PREINTRO.TGA and the FONTMIN fonts,
/// and a third EPI.TXT on disc 4.
#[test]
fn dutch_archives_disc_copies() {
    let Some(nl) = game_nl1997("dutch_archives_disc_copies") else {
        return;
    };
    let big = &nl.big;
    let mut names: Vec<String> = big.names().map(str::to_string).collect();
    names.sort();
    let mut differ = Vec::new();
    let mut several = 0;
    for n in &names {
        let discs = big.discs_of(n);
        if discs.len() < 2 {
            continue;
        }
        several += 1;
        let copies: Vec<Vec<u8>> = discs
            .iter()
            .map(|&d| {
                big.set_disc_copy(Some(d));
                big.read(n).unwrap()
            })
            .collect();
        big.set_disc_copy(None);
        let odd: Vec<u8> = discs
            .iter()
            .zip(&copies)
            .filter(|(_, c)| *c != &copies[0])
            .map(|(&d, _)| d)
            .collect();
        if !odd.is_empty() {
            assert_eq!(discs, [1, 2, 3, 4], "{n}");
            differ.push(format!("{n} {odd:?}"));
        }
    }
    assert_eq!(several, 1292);
    let menus = [
        "CD1MENU", "CD2MENU", "CD3MENU", "CD4MENU", "LOADMENU", "OPTMENU", "SONMENU", "SUREMENU",
    ];
    let mut want: Vec<String> = menus
        .iter()
        .map(|m| format!("SPRLIST\\{m}.TXT [2]"))
        .chain(
            [
                "IMAGES\\PREINTRO.TGA [2]",
                "SPRITE\\2D\\FONTMIN.SPR [2]",
                "SPRITE\\2D\\FONTMIN2.SPR [2]",
                "SPRLIST\\CREDITS.TXT [2]",
                "SPRLIST\\EPI.TXT [2, 4]",
                "SPRLIST\\ERROR1.TXT [2]",
                "SPRLIST\\ERROR2.TXT [2]",
                "SPRLIST\\ERROR3.TXT [2]",
                "SPRLIST\\WAIT.TXT [2]",
            ]
            .map(String::from),
        )
        .collect();
    want.sort();
    assert_eq!(differ, want);
    // SONMENU: "Musiek" on disc 2, "Muziek" on the others.
    let son = |d| {
        big.set_disc_copy(Some(d));
        let t = big.read("SPRLIST\\SONMENU.TXT").unwrap();
        big.set_disc_copy(None);
        t
    };
    assert!(son(2).windows(6).any(|w| w == b"Musiek"));
    assert!(son(1).windows(6).any(|w| w == b"Muziek"));
}

/// The Swedish 1997 build (atlantis-cd-sv-688128): the English May build's
/// code, DGROUP layout and every table at its address with its bytes; its
/// own PE stamp; the player row "Ny spelare" (no '#', also the label, at
/// 0x0048654a). Every flag is en-688128's but the name entry (0x20..0xa7,
/// case kept, the Swedish letters as DOS bytes), the LOAD clamp (the knob
/// still at y 0), the subtitle breaker (back space only) and the DOS code
/// page of its texts.
#[test]
fn tables_of_the_swedish_1997_build() {
    let Some(sv) = game_sv1997("tables_of_the_swedish_1997_build") else {
        return;
    };
    let exe = &sv.exe;
    assert_eq!(exe.build.name, "atlantis-cd-sv-688128");
    check_sound_tables(&sv);
    assert_eq!(exe.new_player_marker, b"Ny spelare");
    assert_eq!(exe.new_player_label, b"Ny spelare");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    let en_build = atlantis_data::exe::BUILDS
        .iter()
        .find(|b| b.name == "atlantis-cd-en-688128")
        .unwrap();
    let mut b = *exe.build;
    assert!(b.subtitle_cut_back_only && b.name_entry_dos_letters);
    assert!(!b.upper_case_names && b.players_1997 && b.credits_read_per_run);
    assert!(b.load_picker_clamp && !b.load_knob_starts_at_bar);
    assert!(!b.startup_logo_rp && b.bitmap_subtitles);
    assert_eq!(b.month, Some("June"));
    assert_eq!(b.text_code_page, atlantis_data::TextCodePage::Dos850);
    b.name = en_build.name;
    b.pe_timestamp = en_build.pe_timestamp;
    b.month = en_build.month;
    b.upper_case_names = en_build.upper_case_names;
    b.load_picker_clamp = en_build.load_picker_clamp;
    b.text_code_page = en_build.text_code_page;
    b.subtitle_cut_back_only = en_build.subtitle_cut_back_only;
    b.name_entry_dos_letters = en_build.name_entry_dos_letters;
    // 20 records per menu (the menu walk's `imul eax,eax,0xa1` @
    // 0x0041f090), not the English build's 16.
    b.menu_records_per_menu = en_build.menu_records_per_menu;
    assert_eq!(&b, en_build);
    if let Some(en) = game_1997("tables_of_the_swedish_1997_build (English 1997)") {
        let e = &en.exe;
        assert_eq!(exe.inventory_object_ids, e.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, e.guest_sprite_ids);
        assert_eq!(exe.credit_widths, e.credit_widths);
        assert_eq!(exe.music_names, e.music_names);
        assert_eq!(exe.newsound_names, e.newsound_names);
    }
}

/// The Swedish CD's archives: the members several discs carry are the same
/// bytes on each, but for the CD texts (disc 4's differs from discs 2 and
/// 3's, both older than disc 1's), EPI.TXT and SELEMENU.TXT (an older
/// revision on discs 2..4). CREDITS.TXT and the FONTMIN fonts, which the
/// original reads from the disc in the drive, are the same on every disc.
#[test]
fn swedish_archives_disc_copies() {
    let Some(sv) = game_sv1997("swedish_archives_disc_copies") else {
        return;
    };
    let big = &sv.big;
    let mut names: Vec<String> = big.names().map(str::to_string).collect();
    names.sort();
    let mut differ = Vec::new();
    let mut several = 0;
    for n in &names {
        let discs = big.discs_of(n);
        if discs.len() < 2 {
            continue;
        }
        several += 1;
        let copies: Vec<Vec<u8>> = discs
            .iter()
            .map(|&d| {
                big.set_disc_copy(Some(d));
                big.read(n).unwrap()
            })
            .collect();
        big.set_disc_copy(None);
        let odd: Vec<u8> = discs
            .iter()
            .zip(&copies)
            .filter(|(_, c)| *c != &copies[0])
            .map(|(&d, _)| d)
            .collect();
        if !odd.is_empty() {
            assert_eq!(discs, [1, 2, 3, 4], "{n}");
            differ.push(format!("{n} {odd:?}"));
        }
    }
    assert_eq!(several, 1292);
    for n in [
        "SPRITE\\2D\\FONTMIN.SPR",
        "SPRITE\\2D\\FONTMIN2.SPR",
        "SPRLIST\\CREDITS.TXT",
    ] {
        assert_eq!(big.discs_of(n), [1, 2, 3, 4], "{n}");
    }
    let want: Vec<String> = [
        "SPRLIST\\CD1MENU.TXT [2, 3, 4]",
        "SPRLIST\\CD2MENU.TXT [2, 3, 4]",
        "SPRLIST\\CD3MENU.TXT [2, 3, 4]",
        "SPRLIST\\CD4MENU.TXT [2, 3, 4]",
        "SPRLIST\\EPI.TXT [2, 3, 4]",
        "SPRLIST\\SELEMENU.TXT [2, 3, 4]",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(differ, want);
    // CD1MENU: discs 2 and 3 share one revision, disc 4 another.
    let cd1 = |d| {
        big.set_disc_copy(Some(d));
        let t = big.read("SPRLIST\\CD1MENU.TXT").unwrap();
        big.set_disc_copy(None);
        t
    };
    assert_eq!(cd1(2), cd1(3));
    assert_ne!(cd1(2), cd1(4));
}

/// The French August build's entry (atlantis-cd-fr-687104).
fn fr_687104() -> &'static atlantis_data::exe::ExeBuild {
    atlantis_data::exe::BUILDS
        .iter()
        .find(|b| b.name == "atlantis-cd-fr-687104")
        .unwrap()
}

/// The Spanish 1997 build (atlantis-cd-es-687104): the French August
/// build's code with the reference's T0023 restart (FUN_004348fc @
/// 0x00434933), its own PE stamp; the player row "Jugador nuevo" (no '#',
/// also the label) at 0x00486546; the credits widths at the German build's
/// 0x004866d4, holding the reference's 256 bytes; the other tables at the
/// French build's addresses, with its bytes.
#[test]
fn tables_of_the_spanish_1997_build() {
    let Some(es) = game_es1997("tables_of_the_spanish_1997_build") else {
        return;
    };
    let exe = &es.exe;
    assert_eq!(exe.build.name, "atlantis-cd-es-687104");
    check_sound_tables(&es);
    assert_eq!(exe.new_player_marker, b"Jugador nuevo");
    assert_eq!(exe.new_player_label, b"Jugador nuevo");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    // The FONTMIN drawers' bytes for 0xa1, 0xa3, 0xa4 (`í ú ñ` in code
    // page 850): the reference's 10, 14, 14 (fr-687104: 17, 2, 10).
    assert_eq!(
        [
            exe.credit_widths[0x1c],
            exe.credit_widths[0x1e],
            exe.credit_widths[0x1f]
        ],
        [10, 14, 14]
    );
    // Every flag as the French August build's but the search restart.
    let fr_build = fr_687104();
    let mut b = *exe.build;
    assert!(b.rat_search_restart);
    assert_eq!(b.month, Some("August"));
    assert_eq!(b.text_code_page, atlantis_data::TextCodePage::Windows1252);
    assert_eq!(
        (b.menu_positioned_byte_width, b.menu_high_bytes_as_space),
        (22, false)
    );
    b.name = fr_build.name;
    b.pe_timestamp = fr_build.pe_timestamp;
    b.credit_widths_va = fr_build.credit_widths_va;
    b.rat_search_restart = fr_build.rat_search_restart;
    assert_eq!(&b, fr_build);
    // The German build's table addresses.
    let de_build = atlantis_data::exe::BUILDS
        .iter()
        .find(|b| b.name == "atlantis-cd-de-687104")
        .unwrap();
    assert_eq!(exe.build.credit_widths_va, de_build.credit_widths_va);
    assert_eq!(exe.build.new_player_va, de_build.new_player_va);
    if let Some(reference) = game("tables_of_the_spanish_1997_build (reference)") {
        assert_eq!(exe.credit_widths, reference.exe.credit_widths);
    }
    if let Some(fr) = game_fr1997("tables_of_the_spanish_1997_build (French August)") {
        let f = &fr.exe;
        assert_eq!(exe.inventory_object_ids, f.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, f.guest_sprite_ids);
        assert_eq!(exe.music_names, f.music_names);
        assert_eq!(exe.newsound_names, f.newsound_names);
    }
}

/// The Italian 1997 build (atlantis-cd-it-687104): the French August
/// build's code with its own menu walk (FUN_0041ecb4: positioned zones 16
/// px per byte @ 0x0041f068, bytes above 0xa8 as spaces @ 0x0041f235), its
/// own PE stamp; the player row "Nuovo Giocatore" (no '#', also the label)
/// at 0x00486546; the credits widths at 0x004866d6 with the French bytes
/// up to the inventory table; the other tables at the French build's
/// addresses, with its bytes.
#[test]
fn tables_of_the_italian_1997_build() {
    let Some(it) = game_it1997("tables_of_the_italian_1997_build") else {
        return;
    };
    let exe = &it.exe;
    assert_eq!(exe.build.name, "atlantis-cd-it-687104");
    check_sound_tables(&it);
    assert_eq!(exe.new_player_marker, b"Nuovo Giocatore");
    assert_eq!(exe.new_player_label, b"Nuovo Giocatore");
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.inventory_object_ids[39], 0);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    assert_eq!(
        [
            exe.credit_widths[0x1c],
            exe.credit_widths[0x1e],
            exe.credit_widths[0x1f]
        ],
        [17, 2, 10]
    );
    let fr_build = fr_687104();
    let mut b = *exe.build;
    assert!(!b.rat_search_restart);
    assert_eq!(b.month, Some("July"));
    assert_eq!(b.text_code_page, atlantis_data::TextCodePage::Windows1252);
    assert_eq!(
        (b.menu_positioned_byte_width, b.menu_high_bytes_as_space),
        (16, true)
    );
    b.name = fr_build.name;
    b.pe_timestamp = fr_build.pe_timestamp;
    b.month = fr_build.month;
    b.credit_widths_va = fr_build.credit_widths_va;
    b.menu_positioned_byte_width = fr_build.menu_positioned_byte_width;
    b.menu_high_bytes_as_space = fr_build.menu_high_bytes_as_space;
    assert_eq!(&b, fr_build);
    if let Some(fr) = game_fr1997("tables_of_the_italian_1997_build (French August)") {
        let f = &fr.exe;
        assert_eq!(exe.inventory_object_ids, f.inventory_object_ids);
        assert_eq!(exe.guest_sprite_ids, f.guest_sprite_ids);
        // The French bytes, one address further, up to the inventory table
        // (0x00486788 in both builds).
        assert_eq!(exe.credit_widths[..0xb2], f.credit_widths[..0xb2]);
        assert_eq!(exe.music_names, f.music_names);
        assert_eq!(exe.newsound_names, f.newsound_names);
    }
}

/// The FONTMAX and FONTMIN advances of the Spanish and Italian builds are
/// the ones every build has: es FONTMAX 0x004865e6, FONTMIN 0x0048666f (the
/// German build's), it 0x004865e8, 0x00486671 (DGROUP 0x00480000, raw
/// 0x6ba00), checked against the French August build's at 0x004865e7 /
/// 0x00486670.
#[test]
fn spanish_italian_menu_font_advances() {
    use atlantis_data::{FileSource, StdDir};
    let off = |va: usize| va - 0x0048_0000 + 0x6_ba00;
    let read =
        |var: &str| std::env::var_os(var).map(|d| StdDir::new(d).read("atlantis.exe").unwrap());
    let (Some(fr), Some(es), Some(it)) = (
        read("ATLANTIS_DATA_FR1997"),
        read("ATLANTIS_DATA_ES1997"),
        read("ATLANTIS_DATA_IT1997"),
    ) else {
        eprintln!("a French, Spanish or Italian 1997 folder not set: skipped");
        return;
    };
    let fontmax = &fr[off(0x0048_65e7)..off(0x0048_65e7) + 137];
    let fontmin = &fr[off(0x0048_6670)..off(0x0048_6670) + 101];
    assert_eq!(fontmax[0], 22);
    assert_eq!(fontmin[0], 15);
    assert_eq!(&es[off(0x0048_65e6)..off(0x0048_65e6) + 137], fontmax);
    assert_eq!(&es[off(0x0048_666f)..off(0x0048_666f) + 101], fontmin);
    assert_eq!(&it[off(0x0048_65e8)..off(0x0048_65e8) + 137], fontmax);
    assert_eq!(&it[off(0x0048_6671)..off(0x0048_6671) + 101], fontmin);
}

/// The Spanish and Italian CDs' archives: every member several discs carry
/// is the same bytes on each disc.
#[test]
fn spanish_italian_archives_disc_copies() {
    for (game, lang) in [
        (game_es1997("spanish_italian_archives_disc_copies"), "es"),
        (game_it1997("spanish_italian_archives_disc_copies"), "it"),
    ] {
        let Some(g) = game else { continue };
        let big = &g.big;
        let mut names: Vec<String> = big.names().map(str::to_string).collect();
        names.sort();
        let (mut several, mut differ) = (0, Vec::new());
        for n in &names {
            let discs = big.discs_of(n);
            if discs.len() < 2 {
                continue;
            }
            several += 1;
            let copies: Vec<Vec<u8>> = discs
                .iter()
                .map(|&d| {
                    big.set_disc_copy(Some(d));
                    big.read(n).unwrap()
                })
                .collect();
            big.set_disc_copy(None);
            if copies.iter().any(|c| c != &copies[0]) {
                differ.push(n.clone());
            }
        }
        eprintln!("{lang}: {several} members on several discs");
        assert!(several > 0, "{lang}");
        assert!(differ.is_empty(), "{lang}: {differ:?}");
    }
}
