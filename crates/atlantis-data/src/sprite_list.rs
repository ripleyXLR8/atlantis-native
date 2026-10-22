//! The sprite lists of an area (SPRLIST\\SPW*.TXT, SPRLIST\\SPF*.TXT) and
//! the NPC click areas built from them.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/npc_click.cpp and sprite_renderer.cpp,
//! GPLv3, the ScummVM team), which follow atlantis.exe's list load
//! FUN_00424a20 and NPC hit-test FUN_00422d5c.

use crate::reader::{atoi, latin1, Reader};
use crate::wam::NpcEntry;

/// One row of an SPW list (e.g. SPWATL1.TXT): a sprite shown in a view.
/// NPC rows have type 0 and an id; animated props (torches) have a non-zero
/// type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpritePlaceEntry {
    /// The view (place id).
    pub place_id: u16,
    /// 0 = NPC, non-zero = animated prop / WSPRANIM pose (2: plays once, 3
    /// and 4: not animated).
    pub kind: i32,
    /// The frame field of the row (the animation length comes from the SPW
    /// header, see `frame_count`).
    pub frames: i32,
    /// The character or object id (0 for props).
    pub perso_id: i32,
    /// Byte 8 of the original's place entry: the absolute value of the
    /// fifth token of a 6-token row, a 2-digit icon (OBJETS1.SPR sprite)
    /// for objects (FUN_00424a20). 0 for 4-token rows.
    pub icon: u8,
    pub spw_file: String,
    /// FUN_00424a20 keeps the id only when written "-NNN" (else -1)...
    pub id_marked: bool,
    /// ...and compares the WAM NPC entries' file names with the rest of the
    /// line after the 2-digit field.
    pub raw_file: String,
    /// The NPC's home view, clickable there (look-only elsewhere): a
    /// leading '-' on the fifth token.
    pub interactive: bool,
    /// A trailing '-' on the view token makes the sprite start inactive
    /// (place entry +0x04 = 0): it shows only after a ShowPerso or a
    /// NEWWSPRANIM.
    pub start_active: bool,
    /// The place entry's +0x04 flag (see [`crate::sprite_anim`]).
    pub active: bool,
    /// The current animation frame.
    pub frame: i32,
    /// SPW header dword +4, set when the place is entered (FUN_0042431c);
    /// 0 when the SPW could not be read (the original skips such rows).
    pub frame_count: i32,
}

fn is_sep(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn tokens(line: &[u8]) -> Vec<&[u8]> {
    line.split(|&b| is_sep(b))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Parses one line of an SPW list. `None` for comments ('#', ';') and lines
/// of fewer than 4 tokens. Rows have 4 tokens (view type frames file) or 6
/// (view type frames -id icon file), never 5 or 7.
pub fn parse_sprite_place_line(line: &[u8]) -> Option<SpritePlaceEntry> {
    if line.is_empty() || line[0] == b'#' || line[0] == b';' {
        return None;
    }
    let tok = tokens(line);
    if tok.len() < 4 {
        return None;
    }
    let start_active = tok[0].last() != Some(&b'-');
    let mut e = SpritePlaceEntry {
        place_id: atoi(tok[0]) as u16,
        kind: atoi(tok[1]),
        frames: atoi(tok[2]),
        start_active,
        active: start_active,
        ..SpritePlaceEntry::default()
    };
    if tok.len() >= 6 {
        e.perso_id = atoi(tok[3]).wrapping_abs();
        e.icon = atoi(tok[4]).wrapping_abs() as u8;
        // atof drops the sign of a zero: check the raw token.
        e.interactive = tok[4][0] == b'-';
        e.spw_file = latin1(tok[5]);
        e.id_marked = tok[3][0] == b'-';
        // The rest of the line after the fifth token and one separator.
        let mut p = 0;
        for _ in 0..5 {
            while p < line.len() && is_sep(line[p]) {
                p += 1;
            }
            while p < line.len() && !is_sep(line[p]) {
                p += 1;
            }
        }
        if p < line.len() && is_sep(line[p]) {
            p += 1;
        }
        e.raw_file = latin1(&line[p..]);
    } else {
        e.spw_file = latin1(tok[3]);
    }
    Some(e)
}

/// Splits a list file into its lines as the engine's loaders do: the text
/// ends at its first NUL, leading white space and empty lines are skipped,
/// a line ends at CR or LF.
fn lines(data: &[u8]) -> Vec<&[u8]> {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    let text = &data[..end];
    let mut out = Vec::new();
    let mut p = 0;
    while p < text.len() {
        while p < text.len() && matches!(text[p], b'\r' | b'\n' | b' ' | b'\t') {
            p += 1;
        }
        if p >= text.len() {
            break;
        }
        let mut eol = p;
        while eol < text.len() && text[eol] != b'\r' && text[eol] != b'\n' {
            eol += 1;
        }
        out.push(&text[p..eol]);
        p = eol;
    }
    out
}

/// Parses an SPW list file (SPRLIST\\<Wam::spw_list>). The hidden persos
/// are applied separately ([`crate::sprite_anim::apply_hidden`]), as
/// FUN_00427d30 does after each row. The list ends at the first line that
/// starts with "FIN", as FUN_00424a20 stops there (3-byte strncmp
/// FUN_0044a83f @ 0x00424a69 and at the loop tail); every shipped list
/// ends with it.
pub fn parse_sprite_list(data: &[u8]) -> Vec<SpritePlaceEntry> {
    lines(data)
        .into_iter()
        .take_while(|l| !l.starts_with(b"FIN"))
        .filter_map(parse_sprite_place_line)
        .collect()
}

/// One row of an SPF list (e.g. SPFATL1.TXT): an NPC shown animated during
/// the transition from `from_place` to `to_place`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpriteTransEntry {
    pub from_place: u16,
    pub to_place: u16,
    pub perso_id: i32,
    pub spf_file: String,
}

/// Parses an SPF list file: rows "from to perso file"; '#' starts a
/// disabled row, a "FIN" line (any case) ends the list.
pub fn parse_spf_list(data: &[u8]) -> Vec<SpriteTransEntry> {
    let mut out = Vec::new();
    for line in lines(data) {
        if line[0] == b'#' {
            continue;
        }
        if line.eq_ignore_ascii_case(b"FIN") {
            break;
        }
        let tok = tokens(line);
        if tok.len() < 4 {
            continue;
        }
        out.push(SpriteTransEntry {
            from_place: atoi(tok[0]) as u16,
            to_place: atoi(tok[1]) as u16,
            perso_id: atoi(tok[2]),
            spf_file: latin1(tok[3]),
        });
    }
    out
}

/// A WAM NPC entry's click rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcClickRect {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}

/// Frame 0 of an SPW as FUN_00424a20 reads it: f = file + [file + 0x14];
/// x = u16 f[0], y = u16 f[0xc], f4 = i32 f[4], f8 = i32 f[8].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpwFrame0 {
    pub x: i16,
    pub y: i16,
    pub f4: i32,
    pub f8: i32,
}

impl SpwFrame0 {
    /// Reads it from an SPW file (bytes past the end read as 0).
    pub fn read(spw: &[u8]) -> SpwFrame0 {
        let mut s = Reader::new(spw);
        s.seek(0x14);
        let off = s.u32() as usize;
        s.seek(off);
        let x = s.u16() as i16;
        s.u16();
        let f4 = s.i32();
        let f8 = s.i32();
        let y = s.u16() as i16;
        SpwFrame0 { x, y, f4, f8 }
    }
}

/// The rectangles as stored in the WAM file.
pub fn initial_npc_click_rects(npcs: &[NpcEntry]) -> Vec<NpcClickRect> {
    npcs.iter()
        .map(|n| NpcClickRect {
            x: n.rect_x,
            y: n.rect_y,
            w: n.rect_w,
            h: n.rect_h,
        })
        .collect()
}

/// Whether a row overwrites some NPC rectangle (its SPW is needed): a row
/// with a "-NNN" id and the '-' click flag, for an NPC entry of the same id
/// and file name (strcmp).
pub fn npc_row_matches(npcs: &[NpcEntry], row: &SpritePlaceEntry) -> bool {
    row.id_marked
        && row.interactive
        && npcs
            .iter()
            .any(|n| n.perso_id as i32 == row.perso_id && n.sprite_file == row.raw_file)
}

/// FUN_00424a20 (0x00424c0b..0x00424ebe): a matching row overwrites the
/// rectangle of every NPC entry with the same id and file name from frame 0
/// of its SPW: x, y, and w = |f8 - x|, h = |f4 - y| (signed compare, 16-bit
/// difference).
pub fn apply_npc_row_rect(
    npcs: &[NpcEntry],
    row: &SpritePlaceEntry,
    f: &SpwFrame0,
    rects: &mut [NpcClickRect],
) {
    if !row.id_marked || !row.interactive {
        return;
    }
    for (n, r) in npcs.iter().zip(rects.iter_mut()) {
        if n.perso_id as i32 != row.perso_id || n.sprite_file != row.raw_file {
            continue;
        }
        let (x, y) = (i32::from(f.x), i32::from(f.y));
        r.x = f.x;
        r.y = f.y;
        r.w = (if x <= f.f8 {
            f.f8.wrapping_sub(x)
        } else {
            x.wrapping_sub(f.f8)
        }) as i16;
        r.h = (if y <= f.f4 {
            f.f4.wrapping_sub(y)
        } else {
            y.wrapping_sub(f.f4)
        }) as i16;
    }
}

/// The NPC rectangles after a list load (FUN_00424a20): the WAM's, then
/// each matching row's. `read_spw` gives the bytes of an SPW
/// (SPRITE\\CYCLO\\<name>), `None` when it cannot be read.
pub fn npc_click_rects(
    npcs: &[NpcEntry],
    entries: &[SpritePlaceEntry],
    mut read_spw: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<NpcClickRect> {
    let mut rects = initial_npc_click_rects(npcs);
    for e in entries {
        if !npc_row_matches(npcs, e) {
            continue;
        }
        if let Some(spw) = read_spw(&e.spw_file) {
            apply_npc_row_rect(npcs, e, &SpwFrame0::read(&spw), &mut rects);
        }
    }
    rects
}

/// FUN_00422d5c: the id of the first WAM NPC entry whose views (those of
/// the first entry with that id, FUN_0045240c) include `vue`, which has an
/// active place entry of the same id and file name in `vue`, and whose
/// rectangle holds (x, y), edges inclusive, unsigned compares; 0 if none.
pub fn npc_hit_test(
    npcs: &[NpcEntry],
    rects: &[NpcClickRect],
    entries: &[SpritePlaceEntry],
    vue: u16,
    x: i32,
    y: i32,
) -> u32 {
    let (px, py) = (x as u32, y as u32);
    for (i, (npc, r)) in npcs.iter().zip(rects).enumerate() {
        let first = npcs[..i]
            .iter()
            .find(|n| n.perso_id == npc.perso_id)
            .unwrap_or(npc);
        if !first.vues.contains(&vue) {
            continue;
        }
        for e in entries {
            if e.place_id != vue || !e.active {
                continue;
            }
            if e.perso_id != npc.perso_id as i32 || npc.sprite_file != e.raw_file {
                continue;
            }
            let (rx, ry) = (i32::from(r.x), i32::from(r.y));
            if px >= rx as u32
                && px <= (rx + i32::from(r.w)) as u32
                && py >= ry as u32
                && py <= (ry + i32::from(r.h)) as u32
            {
                return npc.perso_id;
            }
        }
    }
    0
}

/// FUN_004278a0 (0x00427983..0x00427b85): the rows of view `vue` the object
/// pick-up takes for a clicked object id, in list order: every row of that
/// view with that id whose type is not 3. The active flag is not tested and
/// the loop does not stop at the first row.
pub fn object_pickup_rows(entries: &[SpritePlaceEntry], vue: u16, id: i32) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.place_id == vue && e.perso_id == id && e.kind != 3)
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows after the FIN line are not read (case-sensitive compare).
    #[test]
    fn sprite_list_ends_at_fin() {
        let l = parse_sprite_list(b"1 0 4 A.SPW\r\nfin\r\n2 0 4 B.SPW\r\nFIN\r\n3 0 4 C.SPW\r\n");
        let files: Vec<&str> = l.iter().map(|e| e.spw_file.as_str()).collect();
        assert_eq!(files, ["A.SPW", "B.SPW"]);
    }

    #[test]
    fn row_formats() {
        let e = parse_sprite_place_line(b"12- 1 6 a012g001.spw").unwrap();
        assert_eq!((e.place_id, e.kind, e.frames), (12, 1, 6));
        assert!(!e.start_active && !e.active);
        assert_eq!(e.spw_file, "a012g001.spw");
        assert_eq!(e.perso_id, 0);

        let e = parse_sprite_place_line(b"5 0 1 -601 -00 a005q001.spw").unwrap();
        assert!(e.start_active && e.id_marked && e.interactive);
        assert_eq!((e.perso_id, e.icon), (601, 0));
        assert_eq!(e.raw_file, "a005q001.spw");

        let e = parse_sprite_place_line(b"7 0 1 300 12 obj.spw").unwrap();
        assert!(!e.id_marked && !e.interactive);
        assert_eq!((e.perso_id, e.icon), (300, 12));

        assert!(parse_sprite_place_line(b"# 1 2 3 4").is_none());
        assert!(parse_sprite_place_line(b"1 2 3").is_none());
    }

    #[test]
    fn list_lines() {
        let list = parse_sprite_list(b"  1 0 1 a.spw\r\n;x\r\n\r\n2 1 3 b.spw\0 3 0 1 c.spw");
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].place_id, 2);
        let spf = parse_spf_list(b"001 002 600 a1001002.spf\n#014 016 612 x.spf\nfin\n3 4 5 y.spf");
        assert_eq!(
            spf,
            vec![SpriteTransEntry {
                from_place: 1,
                to_place: 2,
                perso_id: 600,
                spf_file: "a1001002.spf".into()
            }]
        );
    }
}
