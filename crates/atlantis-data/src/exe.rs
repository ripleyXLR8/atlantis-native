//! What the port needs from the player's `atlantis.exe`: which build it is,
//! and the tables baked into its data segment.

/// One build of atlantis.exe: where its tables sit, and its code differences
/// from the 2001 reference that change behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExeBuild {
    pub name: &'static str,
    pub size: u32,
    /// The year the executable was built (shown with the release).
    pub year: u16,
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
    /// The 1997 player list (atlantis-cd-en-688128): names typed 0x20..0x7a
    /// with a..z stored upper case (FUN_0041d6d0 @ 0x0041dab1), no '#' rule
    /// in the list (FUN_0041fff4), a 95-byte players.atl (5 chapters, 5
    /// letters: FUN_0041d6d0, FUN_0041e030, FUN_0042c164), the rows after
    /// the last not cleared by a deletion (FUN_0041e030).
    pub players_1997: bool,
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
    /// FUN_0041cf78: the LOAD scrollbar knob starts at y 0x7b and is
    /// clamped there (0x0041d1fa, 0x0041d2d0), and a row click counts the
    /// scroll offset; atlantis-cd-en-688128 FUN_0041c9a8 starts it at 0,
    /// does not clamp, and tests the row alone.
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
}

/// The reference (a 2001 English build), the GOG build (a 1999 build, the
/// same code for its five languages; its tables are 4 bytes further) and
/// the English retail CD's build of May 1997 (its data segment 0x10000
/// lower).
pub const BUILDS: [ExeBuild; 3] = [
    ExeBuild {
        name: "atlantis-cd-en-714240",
        size: 714_240,
        year: 2001,
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
        hexa_forces_402: true,
        queen_reveal: (3, 200),
        rat_deadline_pacing: false,
        rat_search_restart: true,
        warp_cursor_steps: 2,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
    },
    ExeBuild {
        name: "atlantis-gog-714752",
        size: 714_752,
        year: 1999,
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
        hexa_forces_402: true,
        queen_reveal: (3, 200),
        rat_deadline_pacing: false,
        rat_search_restart: true,
        warp_cursor_steps: 2,
        load_picker_clamp: true,
        music_first_value_rule: false,
        game_over_voice_at_entry: false,
    },
    ExeBuild {
        name: "atlantis-cd-en-688128",
        size: 688_128,
        year: 1997,
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
        hexa_forces_402: false,
        queen_reveal: (5, 75),
        rat_deadline_pacing: true,
        rat_search_restart: false,
        warp_cursor_steps: 1,
        load_picker_clamp: false,
        music_first_value_rule: true,
        game_over_voice_at_entry: true,
    },
];

/// The build of an executable of `size` bytes, `None` when unknown.
pub fn build_for_size(size: u64) -> Option<&'static ExeBuild> {
    BUILDS.iter().find(|b| u64::from(b.size) == size)
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
    /// build's marker is its label, "New Player" (0x0048654a).
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
        let build = build_for_size(exe.len() as u64)?;
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
