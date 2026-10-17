//! What the port needs from the player's `atlantis.exe`: which build it is,
//! and the tables baked into its data segment.

/// One build of atlantis.exe: where its tables sit, and its code differences
/// from the 2001 reference that change behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExeBuild {
    pub name: &'static str,
    pub size: u32,
    pub inventory_va: u32,     // FUN_0042b328's object table
    pub guest_va: u32,         // FUN_0042b480's guest sprite table
    pub credit_widths_va: u32, // credits font advance table
    /// FUN_00422fcc @ 0x0042313d: ids 0x219 / 0x21a get the navigate cursor.
    pub cursor_id_rule: bool,
    /// FUN_0042844c @ 0x0042892e: perso 0x230 (Rhea) always on camera 2.
    pub rhea_camera2: bool,
    /// FUN_00426590: a second death in the same section plays the next
    /// narrator line.
    pub alt_game_over_narration: bool,
}

/// The reference (a 2001 English build) and the GOG build (a 1999 build,
/// the same code for its five languages; its tables are 4 bytes further).
pub const BUILDS: [ExeBuild; 2] = [
    ExeBuild {
        name: "atlantis-cd-en-714240",
        size: 714_240,
        inventory_va: 0x0049_67b0,
        guest_va: 0x0049_6884,
        credit_widths_va: 0x0049_66fc,
        cursor_id_rule: true,
        rhea_camera2: true,
        alt_game_over_narration: false,
    },
    ExeBuild {
        name: "atlantis-gog-714752",
        size: 714_752,
        inventory_va: 0x0049_67b4,
        guest_va: 0x0049_6888,
        credit_widths_va: 0x0049_6700,
        cursor_id_rule: false,
        rhea_camera2: false,
        alt_game_over_narration: true,
    },
];

/// The build of an executable of `size` bytes, `None` when unknown.
pub fn build_for_size(size: u64) -> Option<&'static ExeBuild> {
    BUILDS.iter().find(|b| u64::from(b.size) == size)
}

// PE layout shared by the known builds: image base 0x400000, DGROUP at
// virtual address 0x90000, raw pointer 0x71200.
const IMAGE_BASE: u32 = 0x40_0000;
const DGROUP_VADDR: u32 = 0x9_0000;
const DGROUP_RAW: u32 = 0x7_1200;

fn file_offset(va: u32) -> usize {
    (va - IMAGE_BASE - DGROUP_VADDR + DGROUP_RAW) as usize
}

/// The executable's tables used by the port.
#[derive(Clone, Debug)]
pub struct ExeTables {
    pub build: &'static ExeBuild,
    /// Player-screen row marker and label (0x00496570; FUN_0041e8a8 /
    /// FUN_00420d8c read them by address), e.g. "#New Player".
    pub new_player_marker: Vec<u8>,
    pub new_player_label: Vec<u8>,
    /// Object id per OBJETS1.SPR sprite, up to the -1 terminator.
    pub inventory_object_ids: Vec<i32>,
    /// Perso id per SUIVEUR.SPR sprite, up to -1.
    pub guest_sprite_ids: Vec<i32>,
    /// Credits font advance per byte.
    pub credit_widths: [u8; 256],
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
        let marker_off = file_offset(0x0049_6570);
        let new_player_marker = cstr(exe, marker_off);
        let new_player_label = cstr(exe, marker_off + new_player_marker.len() + 1);
        let mut credit_widths = [0u8; 256];
        let cw = file_offset(build.credit_widths_va);
        credit_widths.copy_from_slice(&exe[cw..cw + 256]);
        Some(ExeTables {
            build,
            new_player_marker,
            new_player_label,
            // 51 entries with 0 placeholders at 39, 46 and 47 (FUN_0042b328)
            inventory_object_ids: i32_list(exe, file_offset(build.inventory_va), 64),
            guest_sprite_ids: i32_list(exe, file_offset(build.guest_va), 64),
            credit_widths,
        })
    }
}
