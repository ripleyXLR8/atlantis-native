//! What the port needs from the player's `atlantis.exe`: which build it is,
//! and the tables baked into its data segment.

use crate::text::TextCodePage;

/// One build of atlantis.exe: where its tables sit, and its code differences
/// from the 2001 reference that change behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExeBuild {
    pub name: &'static str,
    pub size: u32,
    /// The COFF header's TimeDateStamp: with the size, what tells the
    /// builds apart (the reference and the 1999 patch build have the same
    /// size; the 1999 patch and GOG builds the same stamp).
    pub pe_timestamp: u32,
    /// The year the executable was built (shown with the release).
    pub year: u16,
    /// The month, for the 1997 builds (two French builds that year).
    pub month: Option<&'static str>,
    /// DGROUP's virtual address (less the image base) and raw pointer.
    pub dgroup_vaddr: u32,
    pub dgroup_raw: u32,
    pub music_va: u32,       // the 31 music names (13-byte records)
    pub sound_names_va: u32, // the WAM + NewSound name pointers
    /// The range the NewSound name pointers fall in (the strings of the
    /// build's data); a pointer outside it ends the table.
    pub sound_names_range: (u32, u32),
    pub new_player_va: u32,    // the player-screen row marker
    pub inventory_va: u32,     // FUN_0042b328's object table
    pub guest_va: u32,         // FUN_0042b480's guest sprite table
    pub credit_widths_va: u32, // credits font advance table
    /// The marker is followed by the label the row shows (`#New Player`,
    /// then the names array); the 1997 build stores and shows the marker
    /// itself (`New Player`, atlantis-cd-en-688128 FUN_0041fff4).
    pub marker_is_label: bool,
    /// FUN_00422fcc @ 0x0042313d: ids 0x219 / 0x21a get the navigate cursor.
    pub cursor_id_rule: bool,
    /// FUN_0042844c @ 0x0042892e: perso 0x230 (Rhea) always on camera 2.
    pub rhea_camera2: bool,
    /// FUN_00426590: a second death in the same section plays the next
    /// narrator line.
    pub alt_game_over_narration: bool,
    /// The subtitles: the 8x8 bitmap font `DIALOG\FONTS.BIN`, rows of a
    /// 75-byte window 10 pixels apart (atlantis-cd-en-688128 FUN_0042e970,
    /// FUN_0042eb3c, FUN_00410800), instead of the GDI Arial of
    /// FUN_0041ae20 / FUN_0042ffe8 / FUN_00430178.
    pub bitmap_subtitles: bool,
    /// FUN_0041d5f4 @ 0x0041d73b: the SUBTITLES item's three-click cycle
    /// (counter 0x00496098); a plain toggle without it
    /// (atlantis-cd-en-688128 FUN_0041cfa4 @ 0x0041d01b).
    pub subtitle_click_cycle: bool,
    /// The 1997 player list (atlantis-cd-en-688128, atlantis-cd-fr-687104):
    /// no '#' rule in the list (en-688128 FUN_0041fff4, fr-687104
    /// FUN_0041fd6c), a 95-byte players.atl (5 chapters, 5 letters:
    /// en-688128 FUN_0041d6d0, FUN_0041e030, FUN_0042c164; fr-687104 reader
    /// WinMain @ 0x00418d29, writers FUN_0041d45c, FUN_0041dda0,
    /// FUN_0042bf74), the rows after the last not cleared by a deletion
    /// (en-688128 FUN_0041e030, fr-687104 FUN_0041dda0).
    pub players_1997: bool,
    /// Names typed 0x20..0x7a with a..z stored upper case
    /// (atlantis-cd-en-688128 FUN_0041d6d0 @ 0x0041dab1); else 0x20..0xa7,
    /// case kept (FUN_0041dea4; atlantis-cd-fr-687104 FUN_0041d45c @
    /// 0x0041d847).
    pub upper_case_names: bool,
    /// FUN_004406fc @ 0x0044076e: OBJETENMAIN set to 402 before the test,
    /// so the hexagon pieces are always placed (atlantis-cd-en-688128
    /// FUN_0043c570 tests the object the player holds).
    pub hexa_forces_402: bool,
    /// The ogre puzzle's queen reveal: frames and ticks per frame
    /// (FUN_004436f4: 3 x 200; atlantis-cd-en-688128 FUN_0043eebc: 5 x 75).
    pub queen_reveal: (u8, u16),
    /// The rat puzzle's pass: a deadline (`wait until tick >= next; next =
    /// tick + 5`, atlantis-cd-en-688128 FUN_00435a0c @ 0x00435df9) instead
    /// of 5 ticks after each pass (FUN_00438fd4 @ 0x004393f1).
    pub rat_deadline_pacing: bool,
    /// FUN_00437b38 @ 0x00437b6f: T0023's trajectory search restarts at
    /// point 0 (atlantis-cd-en-688128 FUN_00434c6c does not).
    pub rat_search_restart: bool,
    /// The warp loop's cursor animation steps per frame: two in the
    /// reference (FUN_0041bfa0 @ 0x0041c48e..0x0041c535), one in
    /// atlantis-cd-en-688128 (FUN_0041bbd8).
    pub warp_cursor_steps: u8,
    /// FUN_0041cf78 @ 0x0041d1fa: the LOAD scrollbar knob starts at y 0x7b;
    /// at 0 in atlantis-cd-en-688128 (FUN_0041c9a8 @ 0x0041cc23) and
    /// atlantis-cd-fr-687104 (FUN_0041c708 @ 0x0041c98a).
    pub load_knob_starts_at_bar: bool,
    /// FUN_0041cf78: a drag above the LOAD bar clamps the knob to y 0x7b
    /// (0x0041d2d0), and a row click tests row + offset < count
    /// (0x0041d37b); so does atlantis-cd-fr-687104 (FUN_0041c708 @
    /// 0x0041ca60..0x0041ca76, 0x0041cb0b..0x0041cb1b), while
    /// atlantis-cd-en-688128 FUN_0041c9a8 does not clamp and tests the row
    /// alone.
    pub load_picker_clamp: bool,
    /// LOADMUSIK / DELMUSIK do nothing when their first value is the
    /// playing track (atlantis-cd-en-688128 FUN_00429044 @ 0x00429b12,
    /// 0x00429b85).
    pub music_first_value_rule: bool,
    /// The game-over narrator's voice is loaded and started at the
    /// screen's entry, before the image (atlantis-cd-en-688128 FUN_004256a4
    /// @ 0x004256ee..0x0042587b); the reference starts it after the image
    /// is shown (FUN_00426590).
    pub game_over_voice_at_entry: bool,
    /// LoadMusik copies all or nothing on its first listed track
    /// (atlantis-cd-fr-688640 FUN_0041e830): nothing when that track's copy
    /// is there or it is the open track (0x0041e862..0x0041e891), else
    /// every listed track, without a test (0x0041e926..0x0041e96e). The
    /// other builds test each track (FUN_0041f1d0; atlantis-cd-en-688128
    /// FUN_0041e7c0).
    pub music_load_all_or_nothing: bool,
    /// LOADMUSIK runs only when the current music volume (the faded one)
    /// is at least this (atlantis-cd-fr-688640 FUN_00429184 @
    /// 0x00429c5e: `cmp byte ptr [0x485e71], 5; jae`); 0 for no test.
    pub music_load_min_volume: u8,
    /// StartMusik of the open track re-arms the fade-in (FUN_0042a100 @
    /// 0x0042ae3f..0x0042ae4d); atlantis-cd-fr-688640 does nothing
    /// (FUN_00429184 @ 0x00429d47..0x00429d51).
    pub music_restart_rearms_fade: bool,
    /// The code page of the release's texts, for what the port shows
    /// through Unicode (the original takes the bytes as they are).
    pub text_code_page: TextCodePage,
    /// The start-up plays `UBB_VUE\LOGORP.HNM` (the R&P Electronic Media
    /// logo) after LOGO15N and its 1-s wait, before PREINTRO, through the
    /// cinema player at 13 ticks a frame, with no NEWSOUND store
    /// (atlantis-cd-nl-688128 WinMain @ 0x0041958d..0x004195bf, player
    /// FUN_00423444).
    pub startup_logo_rp: bool,
    /// `SPRLIST\CREDITS.TXT` is read from the open archive each time the
    /// credits run (atlantis-cd-en-688128 FUN_0041a5f8 @ 0x0041a717,
    /// atlantis-cd-fr-688640 FUN_0041a638 @ 0x0041a757,
    /// atlantis-cd-nl-688128 FUN_0041a630 @ 0x0041a74e,
    /// atlantis-cd-sv-688128 FUN_0041a600 @ 0x0041a732), so from the disc
    /// in the drive; the other builds load it once at start-up with the
    /// menus (FUN_00426f20 @ 0x0042710c; atlantis-patch-fr-714240
    /// FUN_00426e88 @ 0x00427074; atlantis-cd-fr-687104 and
    /// atlantis-cd-de-687104 FUN_00425d14 @ 0x00425f00), from the boot disc.
    pub credits_read_per_run: bool,
    /// The bitmap subtitles' breaker always cuts a row at the space found
    /// backwards from the end of the 75-byte window, so no row is over 75
    /// bytes (atlantis-cd-sv-688128 FUN_0042ea7c @ 0x0042eb50: the forward
    /// choice of atlantis-cd-en-688128 FUN_0042e970 removed, the forward
    /// scan still run).
    pub subtitle_cut_back_only: bool,
    /// The name entry also takes the Windows-1252 letters `Ä Å Ö ä å ö`
    /// (0xc4 0xc5 0xd6 0xe4 0xe5 0xf6) and stores them as the DOS code page
    /// bytes FONTMIN draws, 0x8e 0x8f 0x99 0x84 0x86 0x94
    /// (atlantis-cd-sv-688128 FUN_0041d704 @ 0x0041dafa..0x0041dbe9).
    pub name_entry_dos_letters: bool,
}

/// The reference (a 2001 English build), the GOG build (a 1999 build, the
/// same code for its five languages; its tables are 4 bytes further), the
/// English retail CD's build of May 1997 (its data segment 0x10000 lower),
/// the French retail CD's build of August 1997 (laid out as the English
/// one, the sound table and player row 4 bytes lower) and the French
/// retail CD's first build, of May 1997 (the English one's code and
/// layout, its tables from the names array on 4 bytes further), the
/// German retail CD's build of August 1997 (the French August build's code,
/// its tables from the credits widths on 1 byte lower), the build of
/// Cryo's patch 1.01 (the GOG build before GOG's edits) and the Dutch
/// retail CD's build of June 1997 (the English May build's code, its
/// tables from the credits widths on 3 or 4 bytes further) and the Swedish
/// retail CD's build of June 1997 (the English May build's code and
/// tables, with its own subtitle breaker, name entry and LOAD clamp).
pub const BUILDS: [ExeBuild; 9] = [
    ExeBuild {
        name: "atlantis-cd-en-714240",
        size: 714_240,
        pe_timestamp: 0x3b82_75d6,
        year: 2001,
        month: None,
        dgroup_vaddr: 0x9_0000,
        dgroup_raw: 0x7_1200,
        music_va: 0x0049_5eec,
        sound_names_va: 0x0049_61e0,
        sound_names_range: (0x004b_0000, 0x004d_0000),
        new_player_va: 0x0049_6570,
        inventory_va: 0x0049_67b0,
        guest_va: 0x0049_6884,
        credit_widths_va: 0x0049_66fc,
        marker_is_label: false,
        cursor_id_rule: true,
        rhea_camera2: true,
        alt_game_over_narration: false,
        bitmap_subtitles: false,
        subtitle_click_cycle: true,
        players_1997: false,
        upper_case_names: false,
        hexa_forces_402: true,
        queen_reveal: (3, 200),
        rat_deadline_pacing: false,
        rat_search_restart: true,
        warp_cursor_steps: 2,
        load_knob_starts_at_bar: true,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    ExeBuild {
        name: "atlantis-gog-714752",
        size: 714_752,
        pe_timestamp: 0x385a_4dc6,
        year: 1999,
        month: None,
        dgroup_vaddr: 0x9_0000,
        dgroup_raw: 0x7_1200,
        music_va: 0x0049_5eec,
        sound_names_va: 0x0049_61e0,
        sound_names_range: (0x004b_0000, 0x004d_0000),
        new_player_va: 0x0049_6570,
        inventory_va: 0x0049_67b4,
        guest_va: 0x0049_6888,
        credit_widths_va: 0x0049_6700,
        marker_is_label: false,
        cursor_id_rule: false,
        rhea_camera2: false,
        alt_game_over_narration: true,
        bitmap_subtitles: false,
        subtitle_click_cycle: true,
        players_1997: false,
        upper_case_names: false,
        hexa_forces_402: true,
        queen_reveal: (3, 200),
        rat_deadline_pacing: false,
        rat_search_restart: true,
        warp_cursor_steps: 2,
        load_knob_starts_at_bar: true,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    // Cryo's Windows patch AT1CDINT.EXE ("1.01"), installed over the
    // English, French and German CD installs, whose BIGCD1..4.BIG it keeps.
    // The 1999 build GOG later edited: the same bytes as GOG's French
    // executable but for GOG's code patches (which concern the original's
    // platform), its drive strings (0x00495eb4, 0x004961a0..0x004961ab),
    // its manifest and its PE header. DGROUP is laid out as the
    // reference's; the tables are GOG's, at the same addresses (the
    // reference's bytes but for the player row).
    ExeBuild {
        name: "atlantis-patch-fr-714240",
        size: 714_240,
        pe_timestamp: 0x385a_4dc6,
        year: 1999,
        month: None,
        dgroup_vaddr: 0x9_0000,
        dgroup_raw: 0x7_1200,
        music_va: 0x0049_5eec,
        sound_names_va: 0x0049_61e0,
        sound_names_range: (0x004b_0000, 0x004d_0000),
        // "#NOUVEAU JOUEUR", then the label "NOUVEAU JOUEUR".
        new_player_va: 0x0049_6570,
        inventory_va: 0x0049_67b4,
        guest_va: 0x0049_6888,
        credit_widths_va: 0x0049_6700,
        marker_is_label: false,
        // FUN_00422fcc: no test of ids 0x219 / 0x21a (the reference's
        // 0x0042313d..0x00423156).
        cursor_id_rule: false,
        // FUN_004283b4: no forced camera (the reference's FUN_0042844c @
        // 0x0042892e).
        rhea_camera2: false,
        // FUN_00426570: the per-section counter added to the narrator
        // letter, that many lines skipped (0x004267ce..0x00426882).
        alt_game_over_narration: true,
        bitmap_subtitles: false,
        subtitle_click_cycle: true,
        players_1997: false,
        upper_case_names: false,
        hexa_forces_402: true,
        queen_reveal: (3, 200),
        rat_deadline_pacing: false,
        rat_search_restart: true,
        warp_cursor_steps: 2,
        load_knob_starts_at_bar: true,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    ExeBuild {
        name: "atlantis-cd-en-688128",
        size: 688_128,
        pe_timestamp: 0x3388_6fd1,
        year: 1997,
        month: Some("May"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_c400,
        music_va: 0x0048_5eec,
        sound_names_va: 0x0048_61d0,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        new_player_va: 0x0048_654a,
        inventory_va: 0x0048_6788,
        guest_va: 0x0048_685c,
        credit_widths_va: 0x0048_66d5,
        marker_is_label: true,
        cursor_id_rule: false,
        rhea_camera2: false,
        // FUN_004256a4: no per-section counter, so no alternative line.
        alt_game_over_narration: false,
        bitmap_subtitles: true,
        subtitle_click_cycle: false,
        players_1997: true,
        upper_case_names: true,
        hexa_forces_402: false,
        queen_reveal: (5, 75),
        rat_deadline_pacing: true,
        rat_search_restart: false,
        warp_cursor_steps: 1,
        load_knob_starts_at_bar: false,
        load_picker_clamp: false,
        music_first_value_rule: true,
        game_over_voice_at_entry: true,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: true,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    // A 1997 build sharing most of atlantis-cd-en-688128's code, with the
    // reference's GDI subtitles (FUN_0041a834, FUN_0042e84c, FUN_0042e9cc),
    // name entry, ogre timing, rat pacing, music copy rules and LOAD clamp,
    // and the GOG build's alternative game-over narrations.
    ExeBuild {
        name: "atlantis-cd-fr-687104",
        size: 687_104,
        pe_timestamp: 0x33ef_1516,
        year: 1997,
        month: Some("August"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_ba00,
        music_va: 0x0048_5eec,
        // WinMain @ 0x004190d9; the names in 0x004ab37c..0x004abb90.
        sound_names_va: 0x0048_61cc,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        // "NOUVEAU JOUEUR", no '#' (WinMain @ 0x00418da7, FUN_0041d45c).
        new_player_va: 0x0048_6546,
        inventory_va: 0x0048_6788,
        guest_va: 0x0048_685c,
        credit_widths_va: 0x0048_66d5,
        marker_is_label: true,
        // FUN_00421f40: no rule for ids 0x219 / 0x21a.
        cursor_id_rule: false,
        // FUN_00427240: no forced camera.
        rhea_camera2: false,
        // FUN_00425430: the per-section counter 0x006bccac
        // (0x0042565f..0x00425719, + 1 @ 0x00425875).
        alt_game_over_narration: true,
        bitmap_subtitles: false,
        // FUN_0041cd30: a plain toggle.
        subtitle_click_cycle: false,
        players_1997: true,
        upper_case_names: false,
        // FUN_0043c210: OBJETENMAIN not forced.
        hexa_forces_402: false,
        // FUN_0043eb5c @ 0x0043ecce..0x0043ece6.
        queen_reveal: (3, 200),
        // FUN_004356a0 @ 0x00435a92..0x00435aa5: 5 ticks after each pass.
        rat_deadline_pacing: false,
        // FUN_004348fc: the search not restarted.
        rat_search_restart: false,
        // FUN_0041b938.
        warp_cursor_steps: 1,
        load_knob_starts_at_bar: false,
        load_picker_clamp: true,
        // FUN_00428e64 acts whatever the playing track.
        music_first_value_rule: false,
        // FUN_00425430: the voice after the image (@ 0x0042559e..0x00425650).
        game_over_voice_at_entry: false,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    // The German retail CD's build: atlantis-cd-fr-687104's code byte for
    // byte (its BEGTEXT section is the French one's) and its DGROUP layout,
    // told apart by the PE stamp. The data strings are German: the player
    // row "NEUER SPIELER" (one byte shorter than "NOUVEAU JOUEUR") moves
    // the names array to 0x00486554 and the credits widths to 0x004866d4;
    // FONTMAX (0x004865e6) and FONTMIN (0x0048666f) hold the other builds'
    // bytes, and the inventory, guest, music and sound tables are at the
    // French build's addresses. Every flag is fr-687104's.
    ExeBuild {
        name: "atlantis-cd-de-687104",
        size: 687_104,
        pe_timestamp: 0x33f8_3e81,
        year: 1997,
        month: Some("August"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_ba00,
        music_va: 0x0048_5eec,
        // The names in 0x004ab37c..0x004abb90, as fr-687104's.
        sound_names_va: 0x0048_61cc,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        // "NEUER SPIELER", no '#' (read as fr-687104's: WinMain @ 0x00418da7,
        // FUN_0041d45c).
        new_player_va: 0x0048_6546,
        inventory_va: 0x0048_6788,
        guest_va: 0x0048_685c,
        credit_widths_va: 0x0048_66d4,
        marker_is_label: true,
        cursor_id_rule: false,
        rhea_camera2: false,
        alt_game_over_narration: true,
        bitmap_subtitles: false,
        subtitle_click_cycle: false,
        players_1997: true,
        upper_case_names: false,
        hexa_forces_402: false,
        queen_reveal: (3, 200),
        rat_deadline_pacing: false,
        rat_search_restart: false,
        warp_cursor_steps: 1,
        load_knob_starts_at_bar: false,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        // The CON texts; the SPRLIST texts are glyph indices (their umlauts
        // at the DOS code page's places, 0x81 0x84 0x8e 0x94 0x99 0x9a).
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    // The first French pressing: atlantis-cd-en-688128's code (every
    // function pairs; most differences are the calling convention of its
    // own, older cdecl CRYO.DLL), with French data strings, its own credits
    // widths and its own music copy rules.
    ExeBuild {
        name: "atlantis-cd-fr-688640",
        size: 688_640,
        pe_timestamp: 0x3381_8054,
        year: 1997,
        month: Some("May"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_c600,
        music_va: 0x0048_5eec,
        // WinMain @ 0x0041950d; the names in 0x004ab37c..0x004abb90.
        sound_names_va: 0x0048_61d0,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        // "NOUVEAU JOUEUR", no '#'; the names array follows at 0x00486559.
        new_player_va: 0x0048_654a,
        inventory_va: 0x0048_678c,
        guest_va: 0x0048_6860,
        // E, b, d, g, n, t one pixel narrower than in the other builds.
        credit_widths_va: 0x0048_66d9,
        marker_is_label: true,
        cursor_id_rule: false,
        rhea_camera2: false,
        // The game-over screen is en-688128's: no per-section counter, the
        // voice at entry.
        alt_game_over_narration: false,
        // FONTS.BIN: breaker FUN_0042eab4, drawer FUN_0042ec80.
        bitmap_subtitles: true,
        subtitle_click_cycle: false,
        players_1997: true,
        // Names typed 0x20..0x7a and upper-cased, as en-688128.
        upper_case_names: true,
        hexa_forces_402: false,
        queen_reveal: (5, 75),
        rat_deadline_pacing: true,
        rat_search_restart: false,
        warp_cursor_steps: 1,
        load_knob_starts_at_bar: false,
        load_picker_clamp: false,
        // FUN_00429184 @ 0x00429c52 (LOADMUSIK) and DELMUSIK as en-688128.
        music_first_value_rule: true,
        game_over_voice_at_entry: true,
        music_load_all_or_nothing: true,
        music_load_min_volume: 5,
        music_restart_rearms_fade: false,
        text_code_page: TextCodePage::Dos850,
        startup_logo_rp: false,
        credits_read_per_run: true,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    // The Dutch retail CD's build: atlantis-cd-en-688128's code (the same
    // size, told apart by the PE stamp) but for the R&P logo at start-up
    // (WinMain @ 0x0041958d..0x004195bf), the name entry (codes 0x20..0xa7
    // kept as typed, FUN_0041d70c @ 0x0041daf7) and changes no player
    // sees (the menu record stride, the CD scan, dead stores). The player
    // row "Nieuwe speler" (marker = label, drawn as stored, FUN_00420014)
    // moves the names array to 0x00486558, the credits widths to 0x004866d8
    // (FONTMAX 0x004865ea, FONTMIN 0x00486673, both with the other builds'
    // bytes) and the inventory and guest tables 4 bytes up.
    ExeBuild {
        name: "atlantis-cd-nl-688128",
        size: 688_128,
        pe_timestamp: 0x3396_af27,
        year: 1997,
        month: Some("June"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_c400,
        music_va: 0x0048_5eec,
        sound_names_va: 0x0048_61d0,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        new_player_va: 0x0048_654a,
        inventory_va: 0x0048_678c,
        guest_va: 0x0048_6860,
        credit_widths_va: 0x0048_66d8,
        marker_is_label: true,
        // FUN_004221e8: no rule for ids 0x219 / 0x21a.
        cursor_id_rule: false,
        // FUN_00427440: no forced camera.
        rhea_camera2: false,
        // FUN_004256c4: the current dialog letter, the voice at entry.
        alt_game_over_narration: false,
        // FONTS.BIN: breaker FUN_0042e990, drawer FUN_0042eb5c.
        bitmap_subtitles: true,
        // FUN_0041cfe0: a plain toggle.
        subtitle_click_cycle: false,
        // FUN_00420014, players.atl 95 bytes (FUN_0041e050, FUN_0042c184).
        players_1997: true,
        // FUN_0041d70c @ 0x0041daf7: `cmp 0xa7`, no a..z -> A..Z.
        upper_case_names: false,
        // FUN_0043c590.
        hexa_forces_402: false,
        // FUN_0043eedc.
        queen_reveal: (5, 75),
        // FUN_00435a2c; FUN_00434c8c.
        rat_deadline_pacing: true,
        rat_search_restart: false,
        // FUN_0041bc10.
        warp_cursor_steps: 1,
        // FUN_0041c9e0.
        load_knob_starts_at_bar: false,
        load_picker_clamp: false,
        // FUN_00429064.
        music_first_value_rule: true,
        game_over_voice_at_entry: true,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        // The CON and SPRLIST texts are in the DOS code page (0x82 é, 0x89
        // ë, 0x8a è, 0x8b ï, 0xa3 ú), drawn glyph = byte.
        text_code_page: TextCodePage::Dos850,
        startup_logo_rp: true,
        credits_read_per_run: true,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
    },
    // The Swedish retail CD's build: atlantis-cd-en-688128's code rebuilt
    // on 1997-06-11 (the same size, told apart by the PE stamp), every
    // table at en-688128's address with en-688128's bytes, the player row
    // "Ny spelare" (marker = label, 0x0048654a). It has the Dutch build's
    // name entry range but not its R&P logo, and three changes of its own:
    // the subtitle breaker cuts at the last space of the window
    // (FUN_0042ea7c @ 0x0042eb50), the name entry stores the Swedish
    // letters as DOS code page bytes (FUN_0041d704 @ 0x0041dafa..0x0041dbe9)
    // and the LOAD picker clamps as the reference's (FUN_0041c9b0 @
    // 0x0041cd08..0x0041cd1e, 0x0041cdb3..0x0041cdc3).
    ExeBuild {
        name: "atlantis-cd-sv-688128",
        size: 688_128,
        pe_timestamp: 0x339e_656f,
        year: 1997,
        month: Some("June"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_c400,
        music_va: 0x0048_5eec,
        sound_names_va: 0x0048_61d0,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        new_player_va: 0x0048_654a,
        inventory_va: 0x0048_6788,
        guest_va: 0x0048_685c,
        credit_widths_va: 0x0048_66d5,
        marker_is_label: true,
        cursor_id_rule: false,
        rhea_camera2: false,
        alt_game_over_narration: false,
        // FONTS.BIN: breaker FUN_0042ea7c, drawer FUN_0042ebfc.
        bitmap_subtitles: true,
        subtitle_click_cycle: false,
        // FUN_00420100, players.atl 95 bytes (FUN_0041e13c).
        players_1997: true,
        // FUN_0041d704 @ 0x0041dae5..0x0041daf8: 0x20..0xa7, no a..z -> A..Z.
        upper_case_names: false,
        hexa_forces_402: false,
        queen_reveal: (5, 75),
        rat_deadline_pacing: true,
        rat_search_restart: false,
        warp_cursor_steps: 1,
        // FUN_0041c9b0: the knob starts at y 0 (0x0041cc32), the drag
        // clamped above the bar (0x0041cd08..0x0041cd1e), a row click tests
        // row + offset < count (0x0041cdb3..0x0041cdc3).
        load_knob_starts_at_bar: false,
        load_picker_clamp: true,
        music_first_value_rule: true,
        game_over_voice_at_entry: true,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        // The CON and SPRLIST texts are in the DOS code page (0x84 ä, 0x86
        // å, 0x8e Ä, 0x8f Å, 0x94 ö, 0x99 Ö), drawn glyph = byte.
        text_code_page: TextCodePage::Dos850,
        startup_logo_rp: false,
        // FUN_0041a600 @ 0x0041a732.
        credits_read_per_run: true,
        subtitle_cut_back_only: true,
        name_entry_dos_letters: true,
    },
];

/// The TimeDateStamp of a PE file's COFF header (at e_lfanew + 8, after the
/// "PE\0\0" signature), `None` when the bytes are not a PE file.
pub fn pe_timestamp(exe: &[u8]) -> Option<u32> {
    let u32_at = |at: usize| {
        exe.get(at..at + 4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    };
    if exe.get(..2)? != b"MZ" {
        return None;
    }
    let pe = usize::try_from(u32_at(0x3c)?).ok()?;
    if exe.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    u32_at(pe + 8)
}

/// The build of an executable, by its size and PE timestamp; `None` when
/// unknown.
pub fn build_of(exe: &[u8]) -> Option<&'static ExeBuild> {
    let stamp = pe_timestamp(exe)?;
    BUILDS
        .iter()
        .find(|b| b.size as usize == exe.len() && b.pe_timestamp == stamp)
}

// The image base of the known builds.
const IMAGE_BASE: u32 = 0x40_0000;

impl ExeBuild {
    /// The file offset of a DGROUP address.
    fn file_offset(&self, va: u32) -> usize {
        (va - IMAGE_BASE - self.dgroup_vaddr + self.dgroup_raw) as usize
    }
}

/// The executable's tables used by the port.
#[derive(Clone, Debug)]
pub struct ExeTables {
    pub build: &'static ExeBuild,
    /// Player-screen row marker and label (0x00496570; FUN_0041e8a8 /
    /// FUN_00420d8c read them by address), e.g. "#New Player"; the 1997
    /// builds' marker is their label, "New Player" (atlantis-cd-en-688128
    /// 0x0048654a), "NOUVEAU JOUEUR" (atlantis-cd-fr-687104 0x00486546,
    /// atlantis-cd-fr-688640 0x0048654a), "NEUER SPIELER"
    /// (atlantis-cd-de-687104 0x00486546), "Nieuwe speler"
    /// (atlantis-cd-nl-688128 0x0048654a), "Ny spelare"
    /// (atlantis-cd-sv-688128 0x0048654a).
    pub new_player_marker: Vec<u8>,
    pub new_player_label: Vec<u8>,
    /// Object id per OBJETS1.SPR sprite, up to the -1 terminator.
    pub inventory_object_ids: Vec<i32>,
    /// Perso id per SUIVEUR.SPR sprite, up to -1.
    pub guest_sprite_ids: Vec<i32>,
    /// Credits font advance per byte.
    pub credit_widths: [u8; 256],
    /// The music tracks, StartMusik=N naming entry N - 1: 13-byte records
    /// at 0x00495eec (FUN_0042a100 @ 0x0042ae0f calls FUN_00432ea8(N - 1)),
    /// as written there (lower case).
    pub music_names: Vec<String>,
    /// The NewSound APC names, /set(NewSound=N) naming entry N: the pointer
    /// table at 0x00496224 (17 WAM names at 0x004961e0 come first), up to
    /// its "FIN" entry. The poller FUN_00432a64 plays "wav\" + entry N.
    pub newsound_names: Vec<String>,
}

/// The number of music tracks (StartMusik=1..31).
pub const MUSIC_TRACKS: usize = 31;

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// The 31 names of 0x00495eec.
fn music_names(build: &ExeBuild, exe: &[u8]) -> Vec<String> {
    let off = build.file_offset(build.music_va);
    (0..MUSIC_TRACKS)
        .map(|i| latin1(&cstr(&exe[off + 13 * i..off + 13 * (i + 1)], 0)))
        .collect()
}

/// The pointer table 0x004961e0: 17 WAM names, then the NewSound names up
/// to "FIN".
fn newsound_names(build: &ExeBuild, exe: &[u8]) -> Vec<String> {
    const SKIP_WAMS: usize = 17;
    let table = build.file_offset(build.sound_names_va) + 4 * SKIP_WAMS;
    let (lo, hi) = build.sound_names_range;
    let mut names = Vec::new();
    for i in 0..200 {
        let at = table + 4 * i;
        let Some(c) = exe.get(at..at + 4) else { break };
        let va = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        if !(lo..=hi).contains(&va) {
            break; // off the end of the data
        }
        let name = latin1(&cstr(exe, build.file_offset(va)));
        if name.eq_ignore_ascii_case("FIN") {
            break;
        }
        names.push(name);
    }
    names
}

fn cstr(exe: &[u8], off: usize) -> Vec<u8> {
    exe[off..]
        .iter()
        .take(32)
        .take_while(|&&b| b != 0)
        .copied()
        .collect()
}

fn i32_list(exe: &[u8], off: usize, max: usize) -> Vec<i32> {
    exe[off..]
        .as_chunks::<4>()
        .0
        .iter()
        .take(max)
        .map(|c| i32::from_le_bytes(*c))
        .take_while(|&v| v != -1)
        .collect()
}

impl ExeTables {
    /// Reads the tables of a known build; `None` for an unknown executable.
    pub fn read(exe: &[u8]) -> Option<ExeTables> {
        let build = build_of(exe)?;
        let marker_off = build.file_offset(build.new_player_va);
        let new_player_marker = cstr(exe, marker_off);
        let new_player_label = if build.marker_is_label {
            new_player_marker.clone()
        } else {
            cstr(exe, marker_off + new_player_marker.len() + 1)
        };
        let mut credit_widths = [0u8; 256];
        let cw = build.file_offset(build.credit_widths_va);
        credit_widths.copy_from_slice(&exe[cw..cw + 256]);
        Some(ExeTables {
            build,
            new_player_marker,
            new_player_label,
            // 51 entries with 0 placeholders at 39, 46 and 47 (FUN_0042b328)
            inventory_object_ids: i32_list(exe, build.file_offset(build.inventory_va), 64),
            guest_sprite_ids: i32_list(exe, build.file_offset(build.guest_va), 64),
            credit_widths,
            music_names: music_names(build, exe),
            newsound_names: newsound_names(build, exe),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of `size` bytes with only an MZ header pointing at a PE header
    /// stamped `stamp`.
    fn fake_exe(size: usize, stamp: u32) -> Vec<u8> {
        let mut exe = vec![0u8; size];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        exe[0x80..0x84].copy_from_slice(b"PE\0\0");
        exe[0x88..0x8c].copy_from_slice(&stamp.to_le_bytes());
        exe
    }

    #[test]
    fn size_and_stamp_tell_every_build() {
        for b in &BUILDS {
            let exe = fake_exe(b.size as usize, b.pe_timestamp);
            assert_eq!(build_of(&exe).map(|f| f.name), Some(b.name));
        }
        // The reference and the patch: one size, two stamps.
        let reference = fake_exe(714_240, 0x3b82_75d6);
        assert_eq!(build_of(&reference).unwrap().name, "atlantis-cd-en-714240");
        let patch = fake_exe(714_240, 0x385a_4dc6);
        assert_eq!(build_of(&patch).unwrap().name, "atlantis-patch-fr-714240");
        // The French and German August 1997 builds: one size, two stamps.
        let fr = fake_exe(687_104, 0x33ef_1516);
        assert_eq!(build_of(&fr).unwrap().name, "atlantis-cd-fr-687104");
        let de = fake_exe(687_104, 0x33f8_3e81);
        assert_eq!(build_of(&de).unwrap().name, "atlantis-cd-de-687104");
        // The English May, Dutch and Swedish June 1997 builds: one size,
        // three stamps.
        let en = fake_exe(688_128, 0x3388_6fd1);
        assert_eq!(build_of(&en).unwrap().name, "atlantis-cd-en-688128");
        let nl = fake_exe(688_128, 0x3396_af27);
        assert_eq!(build_of(&nl).unwrap().name, "atlantis-cd-nl-688128");
        let sv = fake_exe(688_128, 0x339e_656f);
        assert_eq!(build_of(&sv).unwrap().name, "atlantis-cd-sv-688128");
    }

    #[test]
    fn unknown_stamp_or_no_pe_header_is_unknown() {
        assert!(build_of(&fake_exe(714_240, 0x1234_5678)).is_none());
        let mut exe = fake_exe(714_240, 0x3b82_75d6);
        exe[0x80] = b'X';
        assert!(build_of(&exe).is_none());
        assert!(build_of(&vec![0u8; 714_240]).is_none());
        assert!(build_of(b"MZ").is_none());
    }
}
