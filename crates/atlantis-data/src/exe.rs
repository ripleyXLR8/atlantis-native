//! What the port needs from the player's `atlantis.exe`: which build it is,
//! and the tables baked into its data segment.

use crate::text::TextCodePage;

/// One build of atlantis.exe: where its tables sit, and its code differences
/// from the 2001 reference that change behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExeBuild {
    /// The build's name. It also keys the player's saves (the desktop data
    /// folder, the web storage prefix): a released name must not change.
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
    /// The marker has no '#' and is drawn as it is (`New Player`,
    /// atlantis-cd-en-688128 FUN_0041fff4); otherwise the player list
    /// draws the marker (`#New Player`) with its '#' skipped (FUN_00420d8c
    /// @ 0x00420eff). The string after the marker's NUL in the later
    /// builds is not a label: it is the initial content of names[0] of the
    /// players.atl image (0x0049657c), which the file (ReadFile @
    /// 0x00419221) or the marker (strcpy @ 0x004191f6) overwrites before
    /// any draw; no code reads it as a label.
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
    /// (en-688128 FUN_0041e030, fr-687104 FUN_0041dda0), the rows of the
    /// file counted up to the marker only, an empty name being a row
    /// (en-688128 WinMain @ 0x00419194..0x004191b1, fr-687104 @
    /// 0x00418da7..0x00418dc4; the reference also stops at an empty name,
    /// @ 0x0041928f..0x004192cf). Set for every 1997 build.
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
    /// The menu walk's zone for a positioned (`<x,y>`) line: this many
    /// pixels per byte up to the CR (`imul ebx,[ebp-0x28],0x16`, 22, in
    /// FUN_0041fb6c and every build but one; atlantis-cd-it-687104
    /// FUN_0041ecb4 @ 0x0041f068..0x0041f06d: `mov ebx,[ebp-0x28]; shl
    /// ebx,4`, 16).
    pub menu_positioned_byte_width: u8,
    /// The menu walk draws a FONTMAX byte above 0xa8 as a space (pen + 22,
    /// nothing drawn): atlantis-cd-it-687104 FUN_0041ecb4 @ 0x0041f235
    /// (`cmp dword [ebp-0x28],0xa8; jle`), the test of atlantis-cd-en-688128
    /// FUN_0041ef34 @ 0x0041f4b3 (not modelled for that build). The other
    /// builds turn only the bytes below 0x20 into spaces. No shipped menu
    /// byte is above 0xa8.
    pub menu_high_bytes_as_space: bool,
    /// The records per menu of the menus' record table (one static table
    /// of 15 blocks, a count byte and this many records of 8 bytes each,
    /// with no bound on the count: extra records spill into the next
    /// menu's block): 20 (stride 0xa1, `imul eax,eax,0xa1` in FUN_0041fb6c
    /// @ 0x0041fbbc), 16 (stride 0x81) in atlantis-cd-en-688128
    /// (FUN_0041ef34 @ 0x0041ef84) and atlantis-cd-fr-688640 (FUN_0041efa4
    /// @ 0x0041eff4), whose SONMENU.TXT's 18 records then overwrite
    /// SUREMENU's count byte and first two records.
    pub menu_records_per_menu: u8,
    /// The GDI subtitle breaker's cut marks: the reference's 100-byte
    /// window cut at a space (FUN_0042ffe8), or the Chinese builds'
    /// 75-byte window cut after double-byte punctuation (FUN_0042f424 of
    /// atlantis-cd-zh-tw-696832: Big5 marks @ 0x0042f510..0x0042f566; of
    /// atlantis-cd-zh-cn-692736: GB2312 marks and the ideographic space
    /// @ 0x0042f510..0x0042f568), or the Japanese build's rows cut at each
    /// space while its texts are Japanese (FUN_0042f618).
    pub cjk_breaker: CjkBreaker,
    /// The GDI subtitle drawer's row step in pixels: row i of n at y =
    /// 0x1ce - (n - i + 1) * step (FUN_00430178: 14; atlantis-cd-zh-tw-696832
    /// FUN_0042f798 @ 0x0042f7f8, 0x0042f828: `imul eax,eax,0x12`, 18). The
    /// bitmap subtitles have their own (10).
    pub subtitle_row_step: u8,
    /// The menus' `/sprc=` records drawn from these two banks of
    /// `SPRITE\2D` (normal, hovered) instead of FONTMAX / FONTMAX2, which
    /// are then not loaded (atlantis-cd-zh-tw-696832 FUN_0042676c @
    /// 0x004267a2..0x00426874: `sprchi1.spr` into 0x006bd874, `sprchi2.spr`
    /// into 0x006bd878); `None`: the FONTMAX text menus.
    pub menu_sprite_banks: Option<[&'static str; 2]>,
    /// The options menu's toggles drawn as sprites of the menu banks: the
    /// record parser turns id 9 of menu 2 into 9 + SUBTITLES (0x004860a8;
    /// 9 off, 10 on) and id 12 into 13 when the OMNI 3D flag (0x004860ac) is
    /// 1 (atlantis-cd-zh-tw-696832 FUN_004201e8 @ 0x00420212..0x00420225,
    /// 0x0042022e..0x00420242; the id 12 test is made in every menu).
    pub menu_toggle_sprites: bool,
    /// What the LOAD list shows for its rows.
    pub load_list: LoadList,
    /// Captions burnt over one CINEMA video as sprites.
    pub cinema_captions: Option<CinemaCaptions>,
    /// SUBTITLES (0x004860a8) is set at every start-up
    /// (atlantis-cd-zh-tw-696832 WinMain @ 0x0041971f: `mov [0x4860a8],1`);
    /// the other builds leave the file's 0.
    pub subtitles_on_at_start: bool,
    /// The size of `players.atl`'s names block: 0x55 = 85 bytes (5 rows of
    /// 17; the reference reads 97 bytes, the 1997 builds 95), or 0x66 = 102
    /// in the Chinese builds, a 114-byte file with its 6 chapters and 6
    /// letters (atlantis-cd-zh-tw-696832 WinMain @ 0x0041921a: `push 0x66`;
    /// writers FUN_0041dd14 @ 0x0041e574, FUN_0041e65c @ 0x0041e9b0,
    /// FUN_0042cb1c @ 0x0042d147).
    pub players_names_block: u8,
    /// An `off1` dialog line is played by the off1 player: off0's, with the
    /// screen buffer copied to the screen once the voice has loaded. The
    /// 2001 build's FUN_00430884, the GOG.com and French patch builds'
    /// FUN_004307d0 (the same code; the GOG.com scripts have 30 such
    /// lines, the reference's none), the Chinese builds' FUN_0042fe7c
    /// (atlantis-cd-zh-tw-696832, called @ 0x0042e3a6; 29 lines). The 1997
    /// executables hold no "off1" string nor its player, and their scripts
    /// no such header: the port plays one as an `off` line there.
    pub off1_player: bool,
    /// The bytes the build's films' folder string holds at this address,
    /// for the builds whose size and PE timestamp are another build's: the
    /// English DVD's executable and the Perfect Edition's are one file but
    /// for seven path strings (atlantis-dvd-en-695296 @ 0x004abc78
    /// `\face_a\ubb_vue\`, atlantis-pe-2018-695296 `ubb_vue\`).
    /// `None`: not checked.
    pub path_check: Option<(u32, &'static str)>,
    /// The release's CON texts are the 1997 CDs' files, written in the DOS
    /// code page for those CDs' bitmap font, but this build draws them
    /// through Windows (GDI TextOutA, atlantis-dvd-nl-695808 FUN_0042f2c0,
    /// atlantis-dvd-sv-695808 FUN_0042f384) in the Windows code page
    /// (`text_code_page`): the Dutch and Swedish DVDs. The port reads them
    /// as the original shows them, or in the DOS code page with the bug fix
    /// `DosLetters`.
    pub dos_subtitle_texts: bool,
    /// The options' SUBTITLES button has three states, Japanese, English
    /// and none, kept in its click counter (0x00486098 of
    /// atlantis-dvd-ja-696832; [`ExeBuild::subtitle_click_cycle`]'s rules,
    /// FUN_0041d768 @ 0x0041d7e2..0x0041d82c): the chapter's CON is
    /// `SCENAR\CHAPJnnn.CON` (Japanese) at 0, `CHAPInnn.CON` (English) at
    /// 1 and 2 (FUN_00426df4 @ 0x00426e2d), reloaded at each click
    /// (@ 0x0041d833); the breaker cuts the Japanese rows at the spaces
    /// ([`CjkBreaker::Spaces`]) and the English ones as the reference
    /// (FUN_0042f618 @ 0x0042f685); the drawer puts the Japanese rows 36
    /// pixels lower (FUN_0042f86c @ 0x0042f887..0x0042f897: base 0x1f2,
    /// 0x1ce otherwise); a subtitle is shown when SUBTITLES is set or the
    /// counter is below 2 (the callers @ 0x0042ebf2, 0x0042fbfe, 0x0042fea0,
    /// 0x004301e0, 0x004264ce; the drawer has no test of its own); the
    /// options menu's `//` line shows the counter's word of three
    /// (FUN_0041f738 @ 0x0041fba5..0x0041fcd0).
    pub subtitle_languages: bool,
    /// The films WinMain plays from `UBB_VUE` before LOGO15N, through
    /// LOGO15N's player FUN_00423dcc (atlantis-dvd-ja-696832 @
    /// 0x00419713..0x00419772: `logogaga.hnm`, `logospik.hnm`, the logos of
    /// the Japanese publishers Gaga Communications and Spike). The disc has
    /// neither: the player's open fails and it shows its error box (dialog
    /// 0x65, code 0xc: "Fichier introuvable : " and the path, @
    /// 0x00423df2..0x00423e22), which WinMain does not test.
    pub startup_logos: &'static [&'static str],
}

/// The cut marks of the GDI subtitle breaker ([`ExeBuild::cjk_breaker`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CjkBreaker {
    /// The reference's breaker (FUN_0042ffe8), or the 1997 builds' own.
    None,
    /// atlantis-cd-zh-tw-696832 FUN_0042f424: a 75-byte window; backwards
    /// from its end, after `A1 41` `，`, `A1 43` `。`, `A1 46` `；`, `A1 48`
    /// `？`, `A1 49` `！`, `A1 4B` `…` or at a CR (@ 0x0042f510..0x0042f566).
    Big5,
    /// atlantis-cd-zh-cn-692736 FUN_0042f424: the same window; after `A3 AC`
    /// `，`, `A1 A3` `。`, `A3 BF` `？`, `A3 A1` `！`, or before `A1 A1` (the
    /// ideographic space, the next row starting one byte past its lead
    /// byte), or at a CR (@ 0x0042f510..0x0042f568, 0x0042f599..0x0042f5ef).
    Gb2312,
    /// atlantis-dvd-ja-696832 FUN_0042f618 with the subtitle language
    /// counter at 0 (Japanese, [`ExeBuild::subtitle_languages`]): no
    /// window; each ASCII space ends a row, the text copied byte by byte
    /// from after the buffer's leading space until a CR or the NUL, the
    /// last row always counted (@ 0x0042f798..0x0042f84e). With the
    /// counter at 1 or 2 the reference's rows (@ 0x0042f692..0x0042f793).
    Spaces,
}

/// The rows of the LOAD list ([`ExeBuild::load_list`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadList {
    /// Each row its checkpoint's `SPRLIST\EPI.TXT` line in FONTMIN, the
    /// chapter from its file name (FUN_00420b48).
    EpiText,
    /// Row i drawn as frame i of the first bank (the second when hovered),
    /// whatever the checkpoint; a pick sets the chapter to `chapters_va[i]`
    /// and shows frame i of the second bank centred by `widths_va[i]`
    /// (atlantis-cd-zh-tw-696832: banks loaded by FUN_0042676c into
    /// 0x006bd86c / 0x006bd870; rows FUN_004205a4 @ 0x0042060b..0x00420691;
    /// the pick FUN_0041ce00 @ 0x0041d413..0x0041d44d; the title FUN_004208bc
    /// @ 0x00420a22..0x00420a8d).
    Sprites {
        /// The banks of `SPRITE\2D` (normal, hovered).
        banks: [&'static str; 2],
        /// The chapter of each row (`rows` bytes).
        chapters_va: u32,
        /// The width of each row's title frame (`rows` bytes).
        widths_va: u32,
        /// The entries of both tables (the frames of the banks).
        rows: u8,
    },
}

/// Captions drawn over a CINEMA video ([`ExeBuild::cinema_captions`]):
/// atlantis-cd-zh-tw-696832's CON `CINEMA=20` loads the bank and resets the
/// frame 0x004865a0 (FUN_00429958 @ 0x00429fbe..0x0042a053, freed @
/// 0x0042a224..0x0042a235); while that video plays, the video frame
/// presenter FUN_0041afa0 draws the frame at (x, y) when `after` < t <
/// `before` (t = ticks of the 200 Hz timer since 0x004865a4), then steps
/// the frame from k to k + 1 once t > `steps[k]`, in order
/// (@ 0x0041b105..0x0041b2bc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CinemaCaptions {
    /// The CINEMA value (0x004865a1 compared @ 0x00429fcd, 0x0041b105).
    pub cinema: u8,
    /// The bank of `SPRITE\2D` (0x006bd868).
    pub bank: &'static str,
    pub x: i16,
    pub y: i16,
    pub after: u16,
    pub before: u16,
    pub steps: &'static [u16],
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
/// tables, with its own subtitle breaker, name entry and LOAD clamp), the
/// German retail CD's first build, of May 1997 (the English May build's
/// code with the developers' keys left in, the Dutch build's addresses),
/// the
/// Spanish retail CD's build of August 1997 (the French August build's code
/// with the reference's rat search restart, the German build's addresses)
/// the Italian retail CD's build of July 1997 (the French August
/// build's code with its own menu zones, the credits widths 1 byte further)
/// and the Traditional and Simplified Chinese CDs' builds of July 1998 and
/// March 1999 ([`ZH_TW`] and [`ZH_CN`]), the seven European builds of
/// Cryo's 1998 DVD-ROM ([`DVD_EN`]..) and the Perfect Edition's program
/// ([`PE_2018`]).
pub const BUILDS: [ExeBuild; 23] = [
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: true,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
    },
    // The GOG.com build: its French executable; the five language
    // executables are that one with the player row changed. The name is
    // also the key of the player's saves (the desktop data folder, the web
    // storage prefix) since the first release, so it keeps the "-fr"-less
    // spelling those saves were written under.
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: true,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: true,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 16,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
    },
    // The Spanish retail CD's build (August 1997): atlantis-cd-fr-687104's
    // code but for one line, the rat puzzle's T0023 search restarting at
    // point 0 as in the reference (FUN_004348fc @ 0x00434933: `mov word
    // [0x00762b54],0`, the reference's FUN_00437b38 @ 0x00437b6f), told
    // apart by the PE stamp. The player row "Jugador nuevo" (one byte
    // shorter than "NOUVEAU JOUEUR") puts the tables at the German build's
    // addresses: FONTMAX 0x004865e6, FONTMIN 0x0048666f, credits widths
    // 0x004866d4 (with the reference's bytes: 0x1c, 0x1e, 0x1f = 10, 14,
    // 14); inventory, guest, music and sound tables at the French build's.
    ExeBuild {
        name: "atlantis-cd-es-687104",
        size: 687_104,
        pe_timestamp: 0x3405_51c4,
        year: 1997,
        month: Some("August"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_ba00,
        music_va: 0x0048_5eec,
        // The names in 0x004ab37c..0x004abb90, as fr-687104's.
        sound_names_va: 0x0048_61cc,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        // "Jugador nuevo", no '#' (read as fr-687104's: WinMain @
        // 0x00418da7, FUN_0041d45c).
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
        // FUN_004348fc @ 0x00434933.
        rat_search_restart: true,
        warp_cursor_steps: 1,
        load_knob_starts_at_bar: false,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        // The CON texts (Windows-1252); the SPRLIST texts are glyph
        // indices (code page 850 letters, each with a glyph).
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
    },
    // The Italian retail CD's build (July 1997, eleven days before
    // atlantis-cd-fr-687104): the French August build's code but for the
    // menu walk FUN_0041ecb4 (positioned zones of 16 px per byte, FONTMAX
    // bytes above 0xa8 drawn as spaces), told apart by the PE stamp. The
    // player row "Nuovo Giocatore" (one byte longer than "NOUVEAU
    // JOUEUR") moves FONTMAX to 0x004865e8, FONTMIN to 0x00486671 and the
    // credits widths to 0x004866d6 (the French bytes); inventory, guest,
    // music and sound tables at the French build's addresses.
    ExeBuild {
        name: "atlantis-cd-it-687104",
        size: 687_104,
        pe_timestamp: 0x33e0_8f0f,
        year: 1997,
        month: Some("July"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_ba00,
        music_va: 0x0048_5eec,
        // The names in 0x004ab37c..0x004abb90, as fr-687104's.
        sound_names_va: 0x0048_61cc,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        // "Nuovo Giocatore", no '#' (read as fr-687104's).
        new_player_va: 0x0048_6546,
        inventory_va: 0x0048_6788,
        guest_va: 0x0048_685c,
        credit_widths_va: 0x0048_66d6,
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
        // The CON texts (Windows-1252); the SPRLIST texts are glyph
        // indices (code page 850 letters, each with a glyph).
        text_code_page: TextCodePage::Windows1252,
        startup_logo_rp: false,
        credits_read_per_run: false,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
        // FUN_0041ecb4 @ 0x0041f068 and @ 0x0041f235.
        menu_positioned_byte_width: 16,
        menu_high_bytes_as_space: true,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 16,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
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
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        menu_records_per_menu: 20,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
    },
    // The German retail CD's first pressing (May 1997): atlantis-cd-en-688128's
    // code (825 of its 829 functions the same, among them the subtitle
    // breaker FUN_0042f6fc and drawer FUN_0042f8c8, the game-over screen
    // FUN_00426430, the player list FUN_00420d80, the name entry FUN_0041e45c,
    // the LOAD picker FUN_0041d734, the /set executor FUN_00429dd0, LoadMusik
    // FUN_0041f54c, the menu walk FUN_0041fcc0, the credits FUN_0041a6a4),
    // with the reference's cryo.dll and the developers' keys left in: the
    // input poll FUN_0041a148 toggles a debug panel with T, the ogre game's
    // queen flag (0x00485e52) with R, the edge-scroll look mode
    // (0x004860b8) with F1 and an unread flag with F3; the panel
    // (FUN_0041ad30, 46 rows of the engine's variables in the FONTS.BIN
    // font) is drawn by the frame presenters FUN_0041b9f4 and FUN_0041ba94.
    // The port does not model these keys. The player row "NEUER SPIELER"
    // (marker = label) moves the names array to 0x00486558 and the tables
    // after it as in the Dutch build (FONTMAX 0x004865ea, FONTMIN
    // 0x00486673, both with the other builds' bytes); its credits widths
    // are en-688128's but `b` (0x62), one pixel narrower.
    ExeBuild {
        name: "atlantis-cd-de-693248",
        size: 693_248,
        pe_timestamp: 0x3385_89b0,
        year: 1997,
        month: Some("May"),
        dgroup_vaddr: 0x8_0000,
        dgroup_raw: 0x6_d200,
        music_va: 0x0048_5eec,
        // The names in 0x004ab37c..0x004abb90, as en-688128's.
        sound_names_va: 0x0048_61d0,
        sound_names_range: (0x004a_0000, 0x004b_0000),
        // "NEUER SPIELER", no '#'; the names array follows at 0x00486558.
        new_player_va: 0x0048_654a,
        inventory_va: 0x0048_678c,
        guest_va: 0x0048_6860,
        credit_widths_va: 0x0048_66d8,
        marker_is_label: true,
        cursor_id_rule: false,
        rhea_camera2: false,
        // FUN_00426430: no per-section counter, the voice at entry.
        alt_game_over_narration: false,
        bitmap_subtitles: true,
        // FUN_0041dd30: a plain toggle.
        subtitle_click_cycle: false,
        players_1997: true,
        // FUN_0041e45c @ 0x0041e83d: 0x20..0x7a, a..z stored upper case.
        upper_case_names: true,
        // FUN_0043d300.
        hexa_forces_402: false,
        // FUN_0043fc4c.
        queen_reveal: (5, 75),
        // FUN_0043679c; FUN_004359fc.
        rat_deadline_pacing: true,
        rat_search_restart: false,
        // FUN_0041c964.
        warp_cursor_steps: 1,
        // FUN_0041d734.
        load_knob_starts_at_bar: false,
        load_picker_clamp: false,
        // FUN_00429dd0 @ 0x0042a89e (LOADMUSIK), 0x0042a911 (DELMUSIK).
        music_first_value_rule: true,
        game_over_voice_at_entry: true,
        music_load_all_or_nothing: false,
        music_load_min_volume: 0,
        music_restart_rearms_fade: true,
        // The CON texts are in the DOS code page (0x81 ü, 0x84 ä, 0x8e Ä,
        // 0x94 ö, 0x9a Ü, 0xe1 ß), drawn glyph = byte with the English
        // FONTS.BIN.
        text_code_page: TextCodePage::Dos850,
        startup_logo_rp: false,
        credits_read_per_run: true,
        subtitle_cut_back_only: false,
        name_entry_dos_letters: false,
        menu_positioned_byte_width: 22,
        menu_high_bytes_as_space: false,
        // FUN_0041fcc0 @ 0x0041fd10: `imul eax,eax,0x81`.
        menu_records_per_menu: 16,
        cjk_breaker: CjkBreaker::None,
        subtitle_row_step: 14,
        menu_sprite_banks: None,
        menu_toggle_sprites: false,
        load_list: LoadList::EpiText,
        cinema_captions: None,
        subtitles_on_at_start: false,
        players_names_block: 85,
        off1_player: false,
        path_check: None,
        dos_subtitle_texts: false,
        subtitle_languages: false,
        startup_logos: &[],
    },
    ZH_TW,
    ZH_CN,
    DVD_EN,
    DVD_FR,
    DVD_DE,
    DVD_NL,
    DVD_ES,
    DVD_IT,
    DVD_SV,
    DVD_JA,
    PE_2018,
];

/// The Traditional Chinese 4-CD release's build (atlantis-cd-zh-tw-696832,
/// 1998-07-17). Its code line is atlantis-cd-fr-687104's moved toward the
/// reference by the 1998 program ([`DVD_EN`], also the "Perfect Edition"
/// executable, atlantis-pe-2018-695296), with fourteen changes of its own:
/// GDI subtitles in the system code page with a Big5 breaker and rows 18
/// px apart, the menus, the LOAD list and the opening captions drawn as
/// sprites, SUBTITLES on at start-up and a 114-byte players.atl. Every
/// table the port reads holds the reference's bytes but the player row
/// (`New Player`, no '#', 0x004865a8) and the two LOAD tables.
pub const ZH_TW: ExeBuild = ExeBuild {
    name: "atlantis-cd-zh-tw-696832",
    size: 696_832,
    pe_timestamp: 0x35af_1f09,
    year: 1998,
    month: None,
    dgroup_vaddr: 0x8_0000,
    dgroup_raw: 0x6_dc00,
    music_va: 0x0048_5eec,
    // The names in 0x004ab49c..0x004abcb0.
    sound_names_va: 0x0048_61dc,
    sound_names_range: (0x004a_0000, 0x004b_0000),
    // "New Player", then the names array (0x004865b3, initially "New
    // Player" too): the marker is the label, drawn as stored.
    new_player_va: 0x0048_65a8,
    inventory_va: 0x0048_688c,
    guest_va: 0x0048_6960,
    // The reference's 256 widths.
    credit_widths_va: 0x0048_67d8,
    marker_is_label: true,
    // FUN_00422990: no rule for ids 0x219 / 0x21a.
    cursor_id_rule: false,
    // FUN_00427cc0: no forced camera.
    rhea_camera2: false,
    // FUN_00425e80: the per-section counter, the voice after the image.
    alt_game_over_narration: true,
    // GDI: font FUN_0041ad5c, breaker FUN_0042f424, drawer FUN_0042f798.
    bitmap_subtitles: false,
    // FUN_0041d5c0 @ 0x0041d658..0x0041d67a.
    subtitle_click_cycle: true,
    // The '#' rule (FUN_004206e8) and the deletion's loop (FUN_0041e65c)
    // of the reference; the names block of its own (players_names_block).
    players_1997: false,
    // FUN_0041dd14 @ 0x0041e0fd: 0x20..0xa7, case kept.
    upper_case_names: false,
    // FUN_0043d4b0: OBJETENMAIN not forced.
    hexa_forces_402: false,
    // FUN_0043fdfc.
    queen_reveal: (3, 200),
    // FUN_00436938: 5 ticks after each pass.
    rat_deadline_pacing: false,
    // FUN_00435b8c @ 0x00435bc3: `mov word [0x762b74],0`.
    rat_search_restart: true,
    // FUN_0041c030.
    warp_cursor_steps: 1,
    // FUN_0041ce00 @ 0x0041d17a: `mov word [0x602774],0x7b`, with the clamp.
    load_knob_starts_at_bar: true,
    load_picker_clamp: true,
    // FUN_00429958 acts whatever the playing track.
    music_first_value_rule: false,
    game_over_voice_at_entry: false,
    music_load_all_or_nothing: false,
    music_load_min_volume: 0,
    music_restart_rearms_fade: true,
    // The CON texts; the SPRLIST texts are Windows-1252 glyph indices.
    text_code_page: TextCodePage::Windows950,
    startup_logo_rp: false,
    // FUN_0042676c @ 0x0042697f: once at start-up, with the menus.
    credits_read_per_run: false,
    subtitle_cut_back_only: false,
    name_entry_dos_letters: false,
    // FUN_0041f5a0 @ 0x0041f968 (`imul ebx,[ebp-0x28],0x16`), bytes below
    // 0x20 as spaces @ 0x0041fc19.
    menu_positioned_byte_width: 22,
    menu_high_bytes_as_space: false,
    menu_records_per_menu: 20,
    cjk_breaker: CjkBreaker::Big5,
    subtitle_row_step: 18,
    menu_sprite_banks: Some(["SPRCHI1.SPR", "SPRCHI2.SPR"]),
    menu_toggle_sprites: true,
    load_list: LoadList::Sprites {
        banks: ["EPICHI1.SPR", "EPICHI2.SPR"],
        // FUN_0041ce00 @ 0x0041d432, FUN_004208bc.
        chapters_va: 0x0048_6619,
        widths_va: 0x0048_6663,
        rows: 74,
    },
    cinema_captions: Some(CinemaCaptions {
        cinema: 20,
        bank: "SUBCHINE.SPR",
        // @ 0x0041b13b..0x0041b15c: (0xa, 0x1ae).
        x: 10,
        y: 430,
        // @ 0x0041b11d, 0x0041b12f: 0x4b0, 0x4970.
        after: 1200,
        before: 18800,
        // @ 0x0041b16f, 0x0041b192 .. 0x0041b2aa.
        steps: &[
            2200, 4400, 5000, 7200, 7800, 9400, 10600, 12600, 14800, 16400,
        ],
    }),
    subtitles_on_at_start: true,
    players_names_block: 102,
    // FUN_0042fe7c; the scripts' 29 off1 headers.
    off1_player: true,
    path_check: None,
    dos_subtitle_texts: false,
    subtitle_languages: false,
    startup_logos: &[],
};

/// The Simplified Chinese 4-CD release's build (atlantis-cd-zh-cn-692736,
/// 1999-03-18): [`ZH_TW`]'s program, data layout and Traditional Chinese
/// sprites with two functions changed, the subtitle breaker (GB2312 marks,
/// FUN_0042f424) and the hens' weighing (FUN_004412ec, a developer's frame
/// capture removed); DGROUP 0xe00 bytes lower in the file. The functions
/// after the breaker sit 0x24 bytes lower (the drawer FUN_0042f774, rows
/// 18 px apart @ 0x0042f7d4, 0x0042f804).
pub const ZH_CN: ExeBuild = ExeBuild {
    name: "atlantis-cd-zh-cn-692736",
    size: 692_736,
    pe_timestamp: 0x36f1_1f20,
    year: 1999,
    dgroup_raw: 0x6_ce00,
    // The CON texts, GB2312 with a few GBK pairs.
    text_code_page: TextCodePage::Windows936,
    cjk_breaker: CjkBreaker::Gb2312,
    ..ZH_TW
};

/// The 1998 program (atlantis-pe-2018-695296's, April 1998) as Cryo's
/// DVD-ROM ships it for English (atlantis-dvd-en-695296, `FACE_A\ENG`):
/// atlantis-cd-fr-687104's code line moved toward the reference, the
/// parent of [`ZH_TW`], which keeps its rules but those the Chinese builds
/// changed (the GDI text, the sprite menus, the LOAD list and the captions,
/// SUBTITLES at start-up, players.atl's size). The five shared
/// folders are read from `\face_a\` at the disc's root (seven DGROUP
/// strings, 0x004abbfc..0x004ae404), the others from the language's folder
/// (the port's `dvd::DvdFiles`). Its tables (located by the 32-bit operands
/// of the instructions that read them) hold the reference's bytes.
pub const DVD_EN: ExeBuild = ExeBuild {
    name: "atlantis-dvd-en-695296",
    size: 695_296,
    pe_timestamp: 0x3544_84bc,
    year: 1998,
    month: None,
    dgroup_vaddr: 0x8_0000,
    dgroup_raw: 0x6_d600,
    // Readers 0x0041ed7a.., 0x00431fbd.
    music_va: 0x0048_5eec,
    // Readers 0x004196ef, 0x0042d38c; the names in 0x004ab3bc...
    sound_names_va: 0x0048_61d8,
    sound_names_range: (0x004a_0000, 0x004b_0000),
    // "#New Player", then the label "New Player" (readers 0x004191c0..).
    new_player_va: 0x0048_6568,
    // Readers 0x0042a701.., 0x0042a9bc...
    inventory_va: 0x0048_67a8,
    guest_va: 0x0048_687c,
    // Readers 0x0041aab4, 0x0041ac0a, 0x0041ff15: the reference's widths.
    credit_widths_va: 0x0048_66f4,
    marker_is_label: false,
    // FUN_0042270c (fr-687104's FUN_00421f40): no rule for 0x219 / 0x21a.
    cursor_id_rule: false,
    // FUN_00427a14: no forced camera.
    rhea_camera2: false,
    // FUN_00425bfc: the GOG.com rule (the per-section counter), with the
    // reference's `0x00486098 < 2` term (@ 0x0042607f).
    alt_game_over_narration: true,
    // GDI Arial: font FUN_0041ae5c, breaker FUN_0042f108 (fr-687104's
    // FUN_0042e84c, the reference's rows), drawer FUN_0042f290 (the
    // reference's FUN_00430178).
    bitmap_subtitles: false,
    // FUN_0041d394: the three-click cycle.
    subtitle_click_cycle: true,
    // The '#' rule (FUN_00420520), players.atl 97 bytes.
    players_1997: false,
    // FUN_0041dae8: 0x20..0xa7, case kept.
    upper_case_names: false,
    // FUN_0043cfb0 (fr-687104's FUN_0043c210): OBJETENMAIN not forced.
    hexa_forces_402: false,
    // FUN_0043f8fc (fr-687104's FUN_0043eb5c).
    queen_reveal: (3, 200),
    // FUN_00436438 (fr-687104's FUN_004356a0): 5 ticks after each pass;
    // T0023's search restarted (@ 0x004356c3).
    rat_deadline_pacing: false,
    rat_search_restart: true,
    // FUN_0041bf70 (fr-687104's FUN_0041b938).
    warp_cursor_steps: 1,
    // FUN_0041cd40: the knob at 0x7b, with the clamp.
    load_knob_starts_at_bar: true,
    load_picker_clamp: true,
    music_first_value_rule: false,
    game_over_voice_at_entry: false,
    music_load_all_or_nothing: false,
    music_load_min_volume: 0,
    music_restart_rearms_fade: true,
    text_code_page: TextCodePage::Windows1252,
    startup_logo_rp: false,
    // Once at start-up, with the menus.
    credits_read_per_run: false,
    subtitle_cut_back_only: false,
    name_entry_dos_letters: false,
    // FUN_0041f30c: `imul ebx,..,0x16`; bytes below 0x20 as spaces.
    menu_positioned_byte_width: 22,
    menu_high_bytes_as_space: false,
    menu_records_per_menu: 20,
    cjk_breaker: CjkBreaker::None,
    subtitle_row_step: 14,
    // The `/sprj=` banks are never loaded: FONTMAX text menus.
    menu_sprite_banks: None,
    menu_toggle_sprites: false,
    load_list: LoadList::EpiText,
    cinema_captions: None,
    // SUBTITLES keeps the file's 0 (the Chinese builds' start-up set is
    // not here).
    subtitles_on_at_start: false,
    // `push 0x55`: 85 bytes, a 97-byte players.atl.
    players_names_block: 85,
    // FUN_0042f974 (the reference's FUN_00430884, the 1997 copy).
    off1_player: true,
    path_check: Some((0x004a_bc78, "\\face_a\\ubb_vue\\")),
    dos_subtitle_texts: false,
    subtitle_languages: false,
    startup_logos: &[],
};

/// The Japanese DVD build (atlantis-dvd-ja-696832, `FACE_B\JAP`, April
/// 1998): [`DVD_EN`]'s program (856 of its 867 functions the same) with
/// Japanese subtitles, the parent of [`ZH_TW`] (853 the same): the
/// `SCENAR\CHAPJnnn.CON` texts in Shift-JIS drawn by GDI in MS PGothic
/// (FUN_0041aec0 @ 0x0041aedb: `CreateFontA(16, ..., DEFAULT_CHARSET,
/// ..., "ＭＳ Ｐゴシック")`), rows 18 pixels apart, the three-state
/// SUBTITLES button ([`ExeBuild::subtitle_languages`]), the LOAD list and
/// the opening captions as sprites (the Chinese builds' code at other
/// addresses), two missing logo films at start-up
/// ([`ExeBuild::startup_logos`]); English menus, voices and CHAPI texts.
/// Its tables (by their readers' operands): music 0x00485eec (readers
/// 0x0041f1a6..), sound names 0x004861d8 (0x004196ef, 0x0042d89c), player
/// row 0x00486590 (0x004191c0..), inventory 0x00486864 (0x0042ac12..),
/// guests 0x00486938 (0x0042aecc, 0x0042aefa), credits widths 0x004867b0
/// (0x0041ab18, 0x0041ac6e, 0x004203b1: the reference's bytes).
pub const DVD_JA: ExeBuild = ExeBuild {
    name: "atlantis-dvd-ja-696832",
    size: 696_832,
    pe_timestamp: 0x3544_8976,
    dgroup_raw: 0x6_dc00,
    // The names in 0x004ab46c...
    sound_names_va: 0x0048_61d8,
    // "#New Player", then the label.
    new_player_va: 0x0048_6590,
    inventory_va: 0x0048_6864,
    guest_va: 0x0048_6938,
    credit_widths_va: 0x0048_67b0,
    // The CHAPJ texts; the CHAPI (English) ones are ASCII but for a few
    // bytes, which the program also hands to Windows in its ANSI code page.
    text_code_page: TextCodePage::Windows932,
    cjk_breaker: CjkBreaker::Spaces,
    // FUN_0042f86c @ 0x0042f8cf, 0x0042f8ff: `imul eax,eax,0x12`.
    subtitle_row_step: 18,
    load_list: LoadList::Sprites {
        // FUN_0041cfa8 @ 0x0041d0c1, 0x0041d13d: `sprite\2d\epi1.spr`,
        // `epi2.spr` into 0x006bd86c / 0x006bd870.
        banks: ["EPI1.SPR", "EPI2.SPR"],
        // The pick @ 0x0041d5da, 0x0041d5ea; the widths @ 0x0041d57e.
        chapters_va: 0x0048_65f1,
        widths_va: 0x0048_663b,
        rows: 74,
    },
    cinema_captions: Some(CinemaCaptions {
        // FUN_00429b20 @ 0x0042a195 (0x0048658a), the bank @ 0x0042a1c0
        // into 0x006bd868; FUN_0041b104 @ 0x0041b269.
        cinema: 20,
        bank: "SUBINTRO.SPR",
        // @ 0x0041b2a8..0x0041b2b4: (0xa, 0x1ae).
        x: 10,
        y: 430,
        // @ 0x0041b281, 0x0041b293: 0x4b0, 0x4650.
        after: 1200,
        before: 18000,
        // @ 0x0041b2d3, 0x0041b2f6 .. 0x0041b454: 0x898 .. 0x4010.
        steps: &[
            2200, 4400, 5800, 7200, 8200, 10400, 11400, 11800, 12800, 13800, 14800, 16400,
        ],
    }),
    subtitle_languages: true,
    // WinMain @ 0x00419722, 0x00419731, 0x00419763 (0x00486a08,
    // 0x00486a15): "\face_b\ubb_vue\" with these names.
    startup_logos: &["LOGOGAGA.HNM", "LOGOSPIK.HNM"],
    // Its size is the Traditional Chinese build's, its stamp its own; the
    // films' folder at 0x004abd28 (WinMain @ 0x00419722).
    path_check: Some((0x004a_bd28, "\\face_b\\ubb_vue\\")),
    ..DVD_EN
};

/// The Perfect Edition's program (atlantis-pe-2018-695296, Tgames 2018):
/// [`DVD_EN`]'s file with its seven `\face_a\` paths made relative, so
/// the same size and PE timestamp, told apart by the films' folder string.
pub const PE_2018: ExeBuild = ExeBuild {
    name: "atlantis-pe-2018-695296",
    path_check: Some((0x004a_bc78, "ubb_vue\\")),
    ..DVD_EN
};

/// The French DVD build (atlantis-dvd-fr-695296, `FACE_A\FRA`): [`DVD_EN`]'s
/// code (867 functions the same, at the same addresses); its French strings
/// move the tables after the player row by 4 bytes.
pub const DVD_FR: ExeBuild = ExeBuild {
    name: "atlantis-dvd-fr-695296",
    pe_timestamp: 0x3544_8427,
    // "#NOUVEAU JOUEUR" (0x00486568).
    inventory_va: 0x0048_67ac,
    guest_va: 0x0048_6880,
    credit_widths_va: 0x0048_66f8,
    ..DVD_EN
};

/// The German DVD build (atlantis-dvd-de-695296, `FACE_A\ALL`): [`DVD_EN`]'s
/// code; the credits widths at the operand of 0x0041aab4 hold its own bytes.
pub const DVD_DE: ExeBuild = ExeBuild {
    name: "atlantis-dvd-de-695296",
    pe_timestamp: 0x3544_8546,
    credit_widths_va: 0x0048_66f7,
    ..DVD_EN
};

/// The Dutch DVD build (atlantis-dvd-nl-695808, `FACE_A\HOL`): [`DVD_EN`]'s
/// code with 0x30 bytes inserted in WinMain (0x00419787..0x004197b4: the
/// R&P logo `\face_a\ubb_vue\logorp.hnm` played by 0x004239a4, the
/// player of PREINTRO, before it), the code from FUN_00419ec0 on 0x30
/// higher; the Dutch CD's CON texts (DOS code page) drawn by GDI.
pub const DVD_NL: ExeBuild = ExeBuild {
    name: "atlantis-dvd-nl-695808",
    size: 695_808,
    pe_timestamp: 0x3544_86f7,
    dgroup_raw: 0x6_d800,
    // The operand of 0x0041aae4: its own bytes.
    credit_widths_va: 0x0048_66f7,
    startup_logo_rp: true,
    dos_subtitle_texts: true,
    ..DVD_EN
};

/// The Spanish DVD build (atlantis-dvd-es-695296, `FACE_B\ESP`):
/// [`DVD_EN`]'s code, the shared folders at `\face_b\`.
pub const DVD_ES: ExeBuild = ExeBuild {
    name: "atlantis-dvd-es-695296",
    pe_timestamp: 0x3544_88d7,
    credit_widths_va: 0x0048_66f7,
    path_check: Some((0x004a_bc78, "\\face_b\\ubb_vue\\")),
    ..DVD_EN
};

/// The Italian DVD build (atlantis-dvd-it-695296, `FACE_B\ITA`):
/// [`DVD_EN`]'s code but the menu record parser FUN_0041f30c, whose
/// positioned items answer over 16 px per byte (`shl ebx,4`, the Italian
/// CD's rule), the code after it 4 bytes higher.
pub const DVD_IT: ExeBuild = ExeBuild {
    name: "atlantis-dvd-it-695296",
    pe_timestamp: 0x3544_882d,
    inventory_va: 0x0048_67ac,
    guest_va: 0x0048_6880,
    credit_widths_va: 0x0048_66f9,
    menu_positioned_byte_width: 16,
    path_check: Some((0x004a_bc78, "\\face_b\\ubb_vue\\")),
    ..DVD_EN
};

/// The Swedish DVD build (atlantis-dvd-sv-695808, `FACE_B\SUE`):
/// [`DVD_EN`]'s code but the name entry FUN_0041dae8, which stores the
/// typed `Ä Å Ö ä å ö` as DOS code page bytes (the Swedish CD's rule), the
/// code after it 0xf4 bytes higher; the Swedish CD's CON texts (DOS code
/// page) drawn by GDI.
pub const DVD_SV: ExeBuild = ExeBuild {
    name: "atlantis-dvd-sv-695808",
    size: 695_808,
    pe_timestamp: 0x3544_878c,
    dgroup_raw: 0x6_d800,
    name_entry_dos_letters: true,
    dos_subtitle_texts: true,
    path_check: Some((0x004a_bc78, "\\face_b\\ubb_vue\\")),
    ..DVD_EN
};

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

/// The build of an executable, by its size and PE timestamp (and the
/// string of [`ExeBuild::path_check`]); `None` when unknown.
pub fn build_of(exe: &[u8]) -> Option<&'static ExeBuild> {
    let stamp = pe_timestamp(exe)?;
    BUILDS.iter().find(|b| {
        b.size as usize == exe.len()
            && b.pe_timestamp == stamp
            && b.path_check.is_none_or(|(va, text)| {
                let at = b.file_offset(va);
                exe.get(at..at + text.len() + 1)
                    .is_some_and(|s| &s[..text.len()] == text.as_bytes() && s[text.len()] == 0)
            })
    })
}

// The image base of the known builds.
const IMAGE_BASE: u32 = 0x40_0000;

impl ExeBuild {
    /// The side of Cryo's DVD whose root the build reads its shared
    /// folders from ('A' for `\face_a\`, 'B' for `\face_b\`: its
    /// [`ExeBuild::path_check`] string); `None` for the other builds.
    pub fn dvd_side(&self) -> Option<char> {
        let (_, path) = self.path_check?;
        let rest = path.strip_prefix("\\face_")?;
        rest.chars().next().map(|c| c.to_ascii_uppercase())
    }

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
    /// (atlantis-cd-de-687104 0x00486546, atlantis-cd-de-693248
    /// 0x0048654a), "Nieuwe speler"
    /// (atlantis-cd-nl-688128 0x0048654a), "Ny spelare"
    /// (atlantis-cd-sv-688128 0x0048654a), "Jugador nuevo"
    /// (atlantis-cd-es-687104 0x00486546), "Nuovo Giocatore"
    /// (atlantis-cd-it-687104 0x00486546).
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
    /// [`LoadList::Sprites`]: the chapter each LOAD row loads, and the
    /// width of each row's title frame (atlantis-cd-zh-tw-696832
    /// 0x00486619, 0x00486663); empty for the other builds.
    pub load_chapters: Vec<u8>,
    pub load_widths: Vec<u8>,
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
        let (load_chapters, load_widths) = match build.load_list {
            LoadList::EpiText => (Vec::new(), Vec::new()),
            LoadList::Sprites {
                chapters_va,
                widths_va,
                rows,
                ..
            } => {
                let table = |va: u32| {
                    let at = build.file_offset(va);
                    exe[at..at + usize::from(rows)].to_vec()
                };
                (table(chapters_va), table(widths_va))
            }
        };
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
            load_chapters,
            load_widths,
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

    /// [`fake_exe`] of build `b`, with its [`ExeBuild::path_check`] string.
    fn fake_exe_of(b: &ExeBuild) -> Vec<u8> {
        let mut exe = fake_exe(b.size as usize, b.pe_timestamp);
        if let Some((va, text)) = b.path_check {
            let at = b.file_offset(va);
            exe[at..at + text.len()].copy_from_slice(text.as_bytes());
        }
        exe
    }

    #[test]
    fn size_and_stamp_tell_every_build() {
        for b in &BUILDS {
            let exe = fake_exe_of(b);
            assert_eq!(build_of(&exe).map(|f| f.name), Some(b.name));
        }
        // The reference and the patch: one size, two stamps.
        let reference = fake_exe(714_240, 0x3b82_75d6);
        assert_eq!(build_of(&reference).unwrap().name, "atlantis-cd-en-714240");
        let patch = fake_exe(714_240, 0x385a_4dc6);
        assert_eq!(build_of(&patch).unwrap().name, "atlantis-patch-fr-714240");
        // The French, German, Spanish and Italian 1997 builds: one size.
        let fr = fake_exe(687_104, 0x33ef_1516);
        assert_eq!(build_of(&fr).unwrap().name, "atlantis-cd-fr-687104");
        let de = fake_exe(687_104, 0x33f8_3e81);
        assert_eq!(build_of(&de).unwrap().name, "atlantis-cd-de-687104");
        // The Spanish and Italian builds: the same size, two more stamps.
        let es = fake_exe(687_104, 0x3405_51c4);
        assert_eq!(build_of(&es).unwrap().name, "atlantis-cd-es-687104");
        let it = fake_exe(687_104, 0x33e0_8f0f);
        assert_eq!(build_of(&it).unwrap().name, "atlantis-cd-it-687104");
        // The English May, Dutch and Swedish June 1997 builds: one size,
        // three stamps.
        let en = fake_exe(688_128, 0x3388_6fd1);
        assert_eq!(build_of(&en).unwrap().name, "atlantis-cd-en-688128");
        let nl = fake_exe(688_128, 0x3396_af27);
        assert_eq!(build_of(&nl).unwrap().name, "atlantis-cd-nl-688128");
        let sv = fake_exe(688_128, 0x339e_656f);
        assert_eq!(build_of(&sv).unwrap().name, "atlantis-cd-sv-688128");
        // The German May 1997 build: a size of its own.
        let de_may = fake_exe(693_248, 0x3385_89b0);
        assert_eq!(build_of(&de_may).unwrap().name, "atlantis-cd-de-693248");
        assert!(build_of(&fake_exe(693_248, 0x3388_6fd1)).is_none());
        // The Chinese builds: sizes of their own.
        let tw = fake_exe(696_832, 0x35af_1f09);
        assert_eq!(build_of(&tw).unwrap().name, "atlantis-cd-zh-tw-696832");
        let cn = fake_exe(692_736, 0x36f1_1f20);
        assert_eq!(build_of(&cn).unwrap().name, "atlantis-cd-zh-cn-692736");
        assert!(build_of(&fake_exe(696_832, 0x36f1_1f20)).is_none());
    }

    /// The Chinese builds' new fields; every other build keeps the
    /// reference's values of them.
    #[test]
    fn chinese_fields() {
        // The 2001 code line's off1 player (FUN_00430884, FUN_004307d0).
        for b in &BUILDS[..3] {
            assert!(b.off1_player, "{}", b.name);
        }
        for b in &BUILDS[3..12] {
            assert!(!b.off1_player, "{}", b.name);
        }
        for b in &BUILDS[..12] {
            assert_eq!(b.cjk_breaker, CjkBreaker::None, "{}", b.name);
            assert_eq!(b.subtitle_row_step, 14, "{}", b.name);
            assert_eq!(b.menu_sprite_banks, None, "{}", b.name);
            assert!(
                !b.menu_toggle_sprites && !b.subtitles_on_at_start,
                "{}",
                b.name
            );
            assert_eq!(b.load_list, LoadList::EpiText, "{}", b.name);
            assert_eq!(b.cinema_captions, None, "{}", b.name);
            assert_eq!(b.players_names_block, 85, "{}", b.name);
            assert!(!b.text_code_page.is_double_byte(), "{}", b.name);
        }
        assert_eq!(
            (ZH_TW.cjk_breaker, ZH_CN.cjk_breaker),
            (CjkBreaker::Big5, CjkBreaker::Gb2312)
        );
        assert_eq!(ZH_TW.text_code_page, TextCodePage::Windows950);
        assert_eq!(ZH_CN.text_code_page, TextCodePage::Windows936);
        // zh-cn is zh-tw but for its identity, file layout, code page and
        // breaker.
        let mut cn = ZH_CN;
        cn.name = ZH_TW.name;
        cn.size = ZH_TW.size;
        cn.pe_timestamp = ZH_TW.pe_timestamp;
        cn.year = ZH_TW.year;
        cn.dgroup_raw = ZH_TW.dgroup_raw;
        cn.text_code_page = ZH_TW.text_code_page;
        cn.cjk_breaker = ZH_TW.cjk_breaker;
        assert_eq!(cn, ZH_TW);
        for b in [&ZH_TW, &ZH_CN] {
            assert_eq!(b.subtitle_row_step, 18);
            assert!(b.subtitles_on_at_start && b.menu_toggle_sprites && b.off1_player);
            assert_eq!(b.players_names_block, 0x66);
            assert_eq!(b.cinema_captions.map(|c| c.cinema), Some(20));
            // The DGROUP file offsets: zh-tw 0x6dc00 + (VA - 0x480000).
            assert_eq!(
                b.file_offset(0x0048_65a8),
                0x741a8 - (0x6dc00 - b.dgroup_raw as usize)
            );
        }
        let steps = ZH_TW.cinema_captions.unwrap().steps;
        assert!(steps.windows(2).all(|w| w[0] < w[1]));
    }

    /// The 1998 DVD's builds and the Perfect Edition's: the English DVD
    /// and the Perfect Edition share size and stamp, told apart by the
    /// films' folder string; a 1998 file with neither string is unknown.
    /// Their fields are the 1998 program's (pe-2018's, the Chinese builds'
    /// parent) but each language's own rule.
    #[test]
    fn dvd_builds() {
        let en = fake_exe_of(&DVD_EN);
        assert_eq!(build_of(&en).unwrap().name, "atlantis-dvd-en-695296");
        let pe = fake_exe_of(&PE_2018);
        assert_eq!(build_of(&pe).unwrap().name, "atlantis-pe-2018-695296");
        assert!(build_of(&fake_exe(695_296, 0x3544_84bc)).is_none());
        // `\face_a\ubb_vue\` then more: not the string.
        let mut longer = en.clone();
        let at = DVD_EN.file_offset(0x004a_bc78) + "\\face_a\\ubb_vue\\".len();
        longer[at] = b'x';
        assert!(build_of(&longer).is_none());
        let dvd = [DVD_EN, DVD_FR, DVD_DE, DVD_NL, DVD_ES, DVD_IT, DVD_SV];
        for b in &dvd {
            let side = if matches!(
                b.name,
                "atlantis-dvd-es-695296" | "atlantis-dvd-it-695296" | "atlantis-dvd-sv-695808"
            ) {
                'B'
            } else {
                'A'
            };
            assert_eq!(b.dvd_side(), Some(side), "{}", b.name);
            assert_eq!(b.year, 1998);
            // The 1998 program: zh-tw's rules but those it changed.
            let mut x = *b;
            x.name = ZH_TW.name;
            x.size = ZH_TW.size;
            x.pe_timestamp = ZH_TW.pe_timestamp;
            x.dgroup_raw = ZH_TW.dgroup_raw;
            x.sound_names_va = ZH_TW.sound_names_va;
            x.new_player_va = ZH_TW.new_player_va;
            x.inventory_va = ZH_TW.inventory_va;
            x.guest_va = ZH_TW.guest_va;
            x.credit_widths_va = ZH_TW.credit_widths_va;
            x.marker_is_label = ZH_TW.marker_is_label;
            x.text_code_page = ZH_TW.text_code_page;
            x.cjk_breaker = ZH_TW.cjk_breaker;
            x.subtitle_row_step = ZH_TW.subtitle_row_step;
            x.menu_sprite_banks = ZH_TW.menu_sprite_banks;
            x.menu_toggle_sprites = ZH_TW.menu_toggle_sprites;
            x.load_list = ZH_TW.load_list;
            x.cinema_captions = ZH_TW.cinema_captions;
            x.subtitles_on_at_start = ZH_TW.subtitles_on_at_start;
            x.players_names_block = ZH_TW.players_names_block;
            x.path_check = ZH_TW.path_check;
            // Each language's own rule.
            x.startup_logo_rp = false;
            x.dos_subtitle_texts = false;
            x.name_entry_dos_letters = false;
            x.menu_positioned_byte_width = 22;
            assert_eq!(x, ZH_TW, "{}", b.name);
        }
        const { assert!(DVD_NL.startup_logo_rp && DVD_NL.dos_subtitle_texts) };
        const { assert!(DVD_SV.name_entry_dos_letters && DVD_SV.dos_subtitle_texts) };
        assert_eq!(DVD_IT.menu_positioned_byte_width, 16);
        assert_eq!(PE_2018.dvd_side(), None);
        for b in &BUILDS[..14] {
            assert!(
                b.path_check.is_none() && !b.dos_subtitle_texts,
                "{}",
                b.name
            );
        }
    }

    /// The Japanese DVD build: the English DVD's fields but its identity,
    /// tables and Japanese rules; zh-tw's size, its own stamp.
    #[test]
    fn japanese_dvd_build() {
        let ja = fake_exe_of(&DVD_JA);
        assert_eq!(build_of(&ja).unwrap().name, "atlantis-dvd-ja-696832");
        assert_eq!(DVD_JA.size, ZH_TW.size);
        assert_eq!(DVD_JA.dvd_side(), Some('B'));
        let mut x = DVD_JA;
        x.name = DVD_EN.name;
        x.size = DVD_EN.size;
        x.pe_timestamp = DVD_EN.pe_timestamp;
        x.dgroup_raw = DVD_EN.dgroup_raw;
        x.new_player_va = DVD_EN.new_player_va;
        x.inventory_va = DVD_EN.inventory_va;
        x.guest_va = DVD_EN.guest_va;
        x.credit_widths_va = DVD_EN.credit_widths_va;
        x.path_check = DVD_EN.path_check;
        x.text_code_page = DVD_EN.text_code_page;
        x.cjk_breaker = DVD_EN.cjk_breaker;
        x.subtitle_row_step = DVD_EN.subtitle_row_step;
        x.load_list = DVD_EN.load_list;
        x.cinema_captions = DVD_EN.cinema_captions;
        x.subtitle_languages = DVD_EN.subtitle_languages;
        x.startup_logos = DVD_EN.startup_logos;
        assert_eq!(x, DVD_EN);
        // The Chinese builds' LOAD list and caption code, its own tables.
        assert!(matches!(
            DVD_JA.load_list,
            LoadList::Sprites {
                rows: 74,
                chapters_va: 0x0048_65f1,
                ..
            }
        ));
        let c = DVD_JA.cinema_captions.unwrap();
        assert_eq!(
            (c.bank, c.x, c.y, c.after, c.before),
            ("SUBINTRO.SPR", 10, 430, 1200, 18000)
        );
        assert_eq!(c.steps.len(), 12);
        assert!(c.steps.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(DVD_JA.text_code_page, TextCodePage::Windows932);
        for b in &BUILDS {
            assert_eq!(b.subtitle_languages, b.name == DVD_JA.name, "{}", b.name);
            assert_eq!(
                b.startup_logos.is_empty(),
                b.name != DVD_JA.name,
                "{}",
                b.name
            );
            assert_eq!(
                b.cjk_breaker == CjkBreaker::Spaces,
                b.name == DVD_JA.name,
                "{}",
                b.name
            );
        }
    }

    /// The menus' record table: 16 records per menu in the English May
    /// 1997 code's English, French and German May builds (`imul eax,eax,0x81`), 20 in
    /// the others (`0xa1`, the Dutch and Swedish builds of that code too);
    /// the 1997 players.atl row rule in every 1997 build.
    #[test]
    fn menu_records_and_player_rows() {
        for b in &BUILDS {
            let short = matches!(
                b.name,
                "atlantis-cd-en-688128" | "atlantis-cd-fr-688640" | "atlantis-cd-de-693248"
            );
            assert_eq!(
                b.menu_records_per_menu,
                if short { 16 } else { 20 },
                "{}",
                b.name
            );
            assert_eq!(b.players_1997, b.year == 1997, "{}", b.name);
        }
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
