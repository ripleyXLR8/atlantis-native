//! The menu files `SPRLIST\*MENU.TXT` and the menu fonts' pens.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`menu_layout.cpp`;
//! GPLv3, the ScummVM team), which follows `atlantis.exe`'s menu walk
//! `FUN_0041fb6c`, its measuring `FUN_004206c8`, the FONTMIN renderers
//! `FUN_00420d8c` / `FUN_00420f60` / `FUN_00420b48`, the grid hit-test
//! `FUN_004211f0` and the record hit-test `FUN_004210c0`.
//!
//! The original does not parse a menu file ahead of time: every draw walks
//! its text byte by byte, blits the glyphs and sprites in file order and
//! rebuilds the menu's hit table (0x006126b0 + 0xa1 * menu) in the same
//! pass. [`walk_menu_text`] is that walk.

/// One record of a menu's hit table: {s16 x, y, w, h}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuRecord {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}

/// One blit of the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuOp {
    /// A sprite of SPRMENU.SPR (or of the alternate bank), else a glyph.
    pub sprite: bool,
    /// `/sprj=` `/sprr=` `/sprc=`: the bank 0x006cd874, which the
    /// executable never loads (the Chinese builds load their menu words
    /// there, [`atlantis_data::exe::ExeBuild::menu_sprite_banks`]).
    pub alt_bank: bool,
    /// The glyph is drawn with FONTMAX2 (the hovered line); a sprite of the
    /// alternate bank is drawn from the second bank (the hovered line's
    /// flag 0x004860a0, atlantis-cd-zh-tw-696832 FUN_004201e8 @
    /// 0x004204fa..0x0042053d).
    pub hover_font: bool,
    /// The pen (glyph) or the record position (sprite).
    pub x: i16,
    pub y: i16,
    /// Glyph `c - 0x20`, or the sprite index.
    pub index: i32,
    /// The walk's line counter at the op.
    pub line: i32,
    /// The record the op belongs to (-1 before the first).
    pub record: i32,
}

/// The walk's state between draws: in, the records of the previous draw (a
/// sprite record keeps a non-zero x / y, `FUN_004207c0`); out, this draw's
/// hit table and blits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuWalk {
    pub records: Vec<MenuRecord>,
    pub ops: Vec<MenuOp>,
}

/// The original's menu record table: one static table for the whole run
/// (every build), 15 blocks of a count byte and `records_per_menu`
/// records {s16 x, y, w, h}, block m at m * stride, stride = 1 + 8 *
/// records_per_menu: 0x006126b0 / 0xa1 / 20 in the reference
/// (FUN_0041fb6c, FUN_004207c0, FUN_004210c0) and every other build but
/// two, 0x006026b0 / 0x81 / 16 in atlantis-cd-en-688128 (FUN_0041ef34
/// `imul eax,eax,0x81` @ 0x0041ef84) and atlantis-cd-fr-688640
/// (FUN_0041efa4 @ 0x0041eff4; `ExeBuild::menu_records_per_menu`). A walk
/// zeroes the menu's count byte only (@ 0x0041fbc2; atlantis-cd-en-688128
/// @ 0x0041ef8a) and writes record k =
/// count at m * stride + 1 + 8k with no bound, so a menu of more records
/// writes into the next menu's block: SONMENU.TXT's 18 records in a
/// 16-record build put its records 16 and 17 over SUREMENU's count byte
/// and records 0 and 1, and SUREMENU's left ornament (`/spr=5`, which
/// keeps a stored x / y) is then drawn off screen for the session. Nothing
/// else writes the table but the LOAD picker (record 4 of menu 1, the
/// knob) and the sound menu's sliders (records 5..8 of menu 5). Past the
/// 15 blocks lies other memory of the original (what, unknown): no
/// shipped menu reaches there; the port keeps zeros.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRecordTable {
    stride: usize,
    bytes: Vec<u8>,
}

impl MenuRecordTable {
    /// The menus of the table ([`menu_number`]).
    pub const MENUS: usize = 15;

    /// A zeroed table (the original's at start-up).
    pub fn new(records_per_menu: usize) -> MenuRecordTable {
        let stride = 1 + 8 * records_per_menu;
        MenuRecordTable {
            stride,
            bytes: vec![0; MenuRecordTable::MENUS * stride],
        }
    }

    pub fn records_per_menu(&self) -> usize {
        (self.stride - 1) / 8
    }

    fn byte(&self, off: usize) -> u8 {
        self.bytes.get(off).copied().unwrap_or(0)
    }

    fn put_byte(&mut self, off: usize, v: u8) {
        if self.bytes.len() <= off {
            self.bytes.resize(off + 1, 0);
        }
        self.bytes[off] = v;
    }

    fn word(&self, off: usize) -> i16 {
        i16::from_le_bytes([self.byte(off), self.byte(off + 1)])
    }

    /// The count byte of `menu`.
    pub fn count(&self, menu: usize) -> u8 {
        self.byte(menu * self.stride)
    }

    fn set_count(&mut self, menu: usize, n: u8) {
        self.put_byte(menu * self.stride, n);
    }

    /// Record `k` of `menu`, wherever it lies (another menu's block for k
    /// past the records per menu).
    pub fn record(&self, menu: usize, k: usize) -> MenuRecord {
        let o = menu * self.stride + 1 + 8 * k;
        MenuRecord {
            x: self.word(o),
            y: self.word(o + 2),
            w: self.word(o + 4),
            h: self.word(o + 6),
        }
    }

    pub fn set_record(&mut self, menu: usize, k: usize, r: MenuRecord) {
        let o = menu * self.stride + 1 + 8 * k;
        for (i, v) in [r.x, r.y, r.w, r.h].into_iter().enumerate() {
            let [lo, hi] = v.to_le_bytes();
            self.put_byte(o + 2 * i, lo);
            self.put_byte(o + 2 * i + 1, hi);
        }
    }

    /// The records 0 .. count of `menu` as stored (another menu's overflow
    /// included): what the hit test `FUN_004210c0` reads.
    pub fn records(&self, menu: usize) -> Vec<MenuRecord> {
        (0..usize::from(self.count(menu)))
            .map(|k| self.record(menu, k))
            .collect()
    }
}

/// The LOAD picker's writes of its knob, record 4 of menu 1, before a walk
/// (`FUN_0041cf78`): the y always (0x0041d1fa at entry, 0x0041d2b0 while
/// dragging; atlantis-cd-en-688128 0x0041cc23 / 0x0041ccd9), the x 0xffce
/// only with 11 files or fewer (0x0041d21a; atlantis-cd-en-688128
/// 0x0041cc43) and never back: in the session's table a later visit with
/// more files keeps the knob off screen.
pub fn load_picker_knob_record(stored: MenuRecord, s: &LoadPickerScroll) -> MenuRecord {
    let mut r = stored;
    if s.knob_x != 0 {
        r.x = s.knob_x as i16;
    }
    r.y = s.knob_y as i16;
    r
}

/// The FONTMAX measuring widths (0x0049660e, indexed by `c - 0x20`; equal
/// in every known build, atlantis-cd-fr-688640 0x004865eb):
/// `FUN_004206c8` centres labels with them, and the pen advances by them
/// while drawing (0x004202cc).
pub const FONTMAX_WIDTHS: [u8; 137] = [
    22, 10, 15, 15, 16, 24, 20, 8, 16, 24, 13, 8, 8, 24, 7, 14, 20, 9, 19, 19, 17, 16, 18, 17, 20,
    18, 7, 8, 8, 8, 8, 18, 8, 21, 23, 21, 23, 19, 20, 22, 22, 14, 15, 20, 19, 28, 24, 28, 20, 27,
    21, 19, 24, 23, 21, 28, 26, 22, 21, 8, 19, 8, 11, 8, 8, 18, 19, 17, 19, 18, 14, 19, 19, 10, 11,
    18, 10, 23, 20, 20, 19, 19, 15, 15, 12, 20, 18, 23, 20, 19, 17, 18, 7, 20, 13, 18, 21, 20, 18,
    18, 18, 18, 18, 19, 18, 18, 18, 13, 11, 13, 21, 21, 19, 26, 32, 20, 20, 20, 20, 20, 19, 28, 23,
    17, 24, 0, 25, 20, 18, 13, 20, 20, 20, 24, 2, 12, 16,
];

/// The FONTMIN pen advances (0x00496697, 101 bytes indexed by `c - 0x20`;
/// control bytes use index 0x20). Equal in every known build
/// (atlantis-cd-fr-687104 0x00486670, atlantis-cd-fr-688640 0x00486674).
pub const FONTMIN_ADVANCE: [u8; 101] = [
    15, 8, 13, 6, 13, 20, 17, 6, 13, 20, 11, 6, 4, 20, 15, 12, 17, 8, 16, 16, 14, 13, 15, 13, 16,
    15, 7, 6, 6, 6, 6, 20, 6, 17, 19, 18, 19, 16, 16, 19, 18, 12, 12, 17, 16, 24, 20, 23, 17, 22,
    17, 15, 19, 20, 17, 23, 22, 18, 17, 7, 16, 6, 10, 6, 7, 16, 16, 15, 15, 16, 12, 16, 17, 8, 9,
    16, 7, 19, 16, 17, 16, 15, 13, 12, 10, 17, 15, 19, 17, 15, 14, 16, 6, 17, 11, 16, 17, 17, 15,
    16, 15,
];

/// The advances the FONTMIN drawers read: they index 0x00496697 with
/// `c - 0x20` and no bound, so glyph `i` >= 101 reads on into the credits
/// advance table that follows it (0x004966fc = FONTMIN + 101;
/// atlantis-cd-fr-687104 0x00486670 / 0x004866d5), the build's byte
/// `i - 101`: in atlantis-cd-fr-687104 the typed 0xa1, 0xa3 and 0xa4
/// advance 17, 2 and 10, in the reference 10, 14 and 14.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontMinAdvances([u8; 0x100 - 0x20]);

impl FontMinAdvances {
    /// The advances of a build from its credits advance table.
    pub fn new(credit_widths: &[u8; 256]) -> FontMinAdvances {
        let mut a = [0u8; 0x100 - 0x20];
        let n = FONTMIN_ADVANCE.len();
        a[..n].copy_from_slice(&FONTMIN_ADVANCE);
        let rest = a.len() - n;
        a[n..].copy_from_slice(&credit_widths[..rest]);
        FontMinAdvances(a)
    }

    /// The advance of glyph `idx`.
    pub fn get(&self, idx: u8) -> i32 {
        self.0.get(usize::from(idx)).map_or(0, |&a| i32::from(a))
    }
}

/// Without an executable: the glyphs past the table advance 0.
impl Default for FontMinAdvances {
    fn default() -> FontMinAdvances {
        FontMinAdvances::new(&[0; 256])
    }
}

/// `ß` in the DOS code page of the SPRLIST texts (the German EPI.TXT's
/// "verläßt", "Außerhalb", atlantis-cd-de-687104). No sprite font has a
/// glyph for it: FONTMIN and FONTMAX glyph `0xe1 - 0x20` = 193 is past
/// their 137 sprites, so the original draws nothing (`FUN_00410430`) and
/// moves the pen by the advance table's byte 193 (FONTMIN: the credits
/// widths' byte 92, 7 in atlantis-cd-de-687104).
pub const SHARP_S: u8 = 0xe1;

/// A sprite-font text as the port draws and measures it: with the `ß`
/// fix ([`crate::enhancements::BugFix::SharpS`]),
/// [`SHARP_S`] is spelled "ss" (the glyph and advance of 's' twice), so
/// "verläßt" reads "verlässt"; otherwise the bytes as they are.
pub fn spell_sharp_s(text: &[u8], sharp_s_as_ss: bool) -> std::borrow::Cow<'_, [u8]> {
    if !sharp_s_as_ss || !text.contains(&SHARP_S) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = Vec::with_capacity(text.len() + 4);
    for &c in text {
        if c == SHARP_S {
            out.extend_from_slice(b"ss");
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// The width of glyph `c` (byte - 0x20). `FUN_004206c8` reads
/// 0x004965ee[byte] with no bound, so bytes below 0x20 and from 0xa9 read
/// the strings before the table and the FONTMIN table after it; the port
/// counts them 0, as for the pen advance (0x004202cc). No shipped menu
/// holds such a byte.
fn fontmax_width(c: i32) -> i32 {
    usize::try_from(c)
        .ok()
        .and_then(|i| FONTMAX_WIDTHS.get(i))
        .map_or(0, |&w| i32::from(w))
}

/// The FONTMAX width of a label (`FUN_004206c8`).
pub fn font_max_measure(text: &[u8]) -> i32 {
    text.iter()
        .map(|&c| fontmax_width(i32::from(c) - 0x20))
        .sum()
}

/// The menu number the original gives a file (the order of the name table
/// 0x0049644c): MAINMENU 0 .. ERROR3 14.
pub fn menu_number(file: &str) -> Option<usize> {
    const MENUS: [&str; 15] = [
        "MAINMENU", "LOADMENU", "OPTMENU", "NEWMENU", "SELEMENU", "SONMENU", "SUREMENU", "CD1MENU",
        "CD2MENU", "CD3MENU", "CD4MENU", "WAIT", "ERROR1", "ERROR2", "ERROR3",
    ];
    let base = file.split('.').next().unwrap_or(file);
    MENUS.iter().position(|m| m.eq_ignore_ascii_case(base))
}

/// `FUN_00451b7a` (atoi): leading spaces, a sign, digits.
fn walk_atoi(t: &[u8], mut p: usize) -> i32 {
    while p < t.len() && (t[p] == b' ' || t[p] == b'\t') {
        p += 1;
    }
    let mut neg = false;
    if p < t.len() && (t[p] == b'-' || t[p] == b'+') {
        neg = t[p] == b'-';
        p += 1;
    }
    let mut v: i32 = 0;
    while p < t.len() && t[p].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add(i32::from(t[p] - b'0'));
        p += 1;
    }
    if neg {
        -v
    } else {
        v
    }
}

/// The digit steps of `FUN_0041fb6c` / `FUN_004207c0`: base, + 1 above 9,
/// + 1 above 99 (a 4-digit or negative number desynchronises the walk).
fn walk_step(p: usize, v: i32, base: usize, three_digits: bool) -> usize {
    let mut p = p + base;
    if v > 9 {
        p += 1;
    }
    if three_digits && v > 99 {
        p += 1;
    }
    p
}

fn walk_at(t: &[u8], p: usize, word: &[u8]) -> bool {
    t.get(p..p + word.len()) == Some(word)
}

fn slot(walk: &mut MenuWalk, n: usize) -> &mut MenuRecord {
    if walk.records.len() <= n {
        walk.records.resize(n + 1, MenuRecord::default());
    }
    &mut walk.records[n]
}

/// Where a walk keeps its records: the session's table, as the original
/// (its menu's block and the count byte, `inc byte ptr`), or the walk's
/// own records.
struct WalkRecords<'a> {
    table: Option<(&'a mut MenuRecordTable, usize)>,
    count: usize,
}

impl WalkRecords<'_> {
    /// Record `count` as the walk finds it.
    fn get(&self, walk: &MenuWalk) -> MenuRecord {
        match &self.table {
            Some((t, m)) => t.record(*m, self.count),
            None => walk.records.get(self.count).copied().unwrap_or_default(),
        }
    }

    /// Writes record `count` and counts it.
    fn put(&mut self, walk: &mut MenuWalk, r: MenuRecord) {
        match &mut self.table {
            Some((t, m)) => {
                t.set_record(*m, self.count, r);
                let n = t.count(*m).wrapping_add(1);
                t.set_count(*m, n);
                self.count = usize::from(n);
            }
            None => {
                *slot(walk, self.count) = r;
                self.count += 1;
            }
        }
    }
}

/// The build's and the port's rules of the menu walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuRules {
    /// The port's `ß` fix (see [`spell_sharp_s`]): a [`SHARP_S`] byte is
    /// drawn and measured as two 's' glyphs; the records and the walk are
    /// the original's.
    pub sharp_s_as_ss: bool,
    /// The zone width per byte of a positioned (`<x,y>`) line: 22, 16 in
    /// atlantis-cd-it-687104 (`ExeBuild::menu_positioned_byte_width`).
    pub positioned_byte_width: i32,
    /// A byte above 0xa8 is a space (atlantis-cd-it-687104 FUN_0041ecb4 @
    /// 0x0041f235, `ExeBuild::menu_high_bytes_as_space`).
    pub high_bytes_as_space: bool,
    /// The options toggles as sprite records (`ExeBuild::menu_toggle_sprites`):
    /// a record of id 9 in menu 2 is drawn as id 9 + SUBTITLES (`toggle_a`),
    /// one of id 12 as 13 when OMNI 3D (`toggle_b`) is 1
    /// (atlantis-cd-zh-tw-696832 FUN_004201e8 @ 0x00420212..0x00420242).
    pub toggle_sprites: bool,
    /// The port's fix of the id 9 record ([`MenuRules::toggle_sprites`]):
    /// the original steps its text pointer back one byte to make room for
    /// the two digits of id 10 (@ 0x0042022b), also when the id stays 9, so
    /// with SUBTITLES off the record reads its x at the comma (0) and each
    /// later field from inside the one before (a slot that holds an x / y
    /// keeps them, a fresh one stores the shifted values); with the fix
    /// ([`crate::enhancements::BugFix::SubtitlesToggle`]) the step back is
    /// taken only for id 10.
    pub toggle_in_step: bool,
}

/// The reference's walk, without the port's fix.
impl Default for MenuRules {
    fn default() -> MenuRules {
        MenuRules {
            sharp_s_as_ss: false,
            positioned_byte_width: 22,
            high_bytes_as_space: false,
            toggle_sprites: false,
            toggle_in_step: false,
        }
    }
}

impl MenuRules {
    /// The rules of a build: its own, with the bug fixes `fixes`
    /// ([`crate::enhancements::BugFix`]): the `ß` fix (`SharpS`), the 22-px
    /// positioned zones of every other build (`MenuLineWidth`: the
    /// Italian build's 16 px per byte fall short of the drawn word: `OK`
    /// 11 px, `Annulla` 18 px) and the SUBTITLES record read in step
    /// (`SubtitlesToggle`).
    pub fn of_build(
        build: &atlantis_data::exe::ExeBuild,
        fixes: &crate::enhancements::BugFixes,
    ) -> MenuRules {
        use crate::enhancements::BugFix;
        MenuRules {
            sharp_s_as_ss: fixes.get(BugFix::SharpS),
            positioned_byte_width: if fixes.get(BugFix::MenuLineWidth) {
                22
            } else {
                i32::from(build.menu_positioned_byte_width)
            },
            high_bytes_as_space: build.menu_high_bytes_as_space,
            toggle_sprites: build.menu_toggle_sprites,
            toggle_in_step: fixes.get(BugFix::SubtitlesToggle),
        }
    }
}

/// `FUN_0041fb6c`, the menu walk of menu `menu`, with the hovered line
/// `hover_line` (0x006cd850) and the options toggles `toggle_a` /
/// `toggle_b` (0x004960a8 SUBTITLES / 0x004960ac OMNI 3D). At each
/// position, in this order: `/fin` ends; `<>,Y>` centred text; `<X,Y>`
/// positioned text; `//` and `&&` the options toggles (menu 2, lines 3 and
/// 5); `/sprj=` `/sprr=` `/sprc=` `/spr=` sprite records; then the byte:
/// CR (the next byte is consumed), space and other control bytes (pen +
/// 22), or a FONTMAX glyph. The build's and the port's rules are `rules`
/// ([`MenuRules`]). The records are the walk's own
/// ([`walk_menu_text_in`] keeps them in the session's table).
pub fn walk_menu_text(
    t: &[u8],
    menu: i32,
    hover_line: i32,
    toggle_a: i32,
    toggle_b: i32,
    rules: MenuRules,
    walk: &mut MenuWalk,
) {
    walk_menu_text_in(t, menu, hover_line, toggle_a, toggle_b, rules, walk, None);
}

/// [`walk_menu_text`] with the session's record table: the walk reads the
/// stored records from the menu's block (whatever an earlier walk, another
/// menu's overflow or the LOAD picker left there) and writes them back;
/// `walk.records` then receives the menu's count records. Without a table
/// (or a menu number), the walk's own records, unbounded.
#[allow(clippy::too_many_arguments)]
pub fn walk_menu_text_in(
    t: &[u8],
    menu: i32,
    hover_line: i32,
    toggle_a: i32,
    toggle_b: i32,
    rules: MenuRules,
    walk: &mut MenuWalk,
    table: Option<&mut MenuRecordTable>,
) {
    let table_menu = usize::try_from(menu)
        .ok()
        .filter(|&m| m < MenuRecordTable::MENUS);
    let mut rec = WalkRecords {
        table: table.zip(table_menu),
        count: 0,
    };
    if let Some((t, m)) = &mut rec.table {
        t.set_count(*m, 0); // the count byte only (@ 0x0041fbc2)
    }
    let sharp_s_as_ss = rules.sharp_s_as_ss;
    let ss = |c: u8| -> i32 {
        if sharp_s_as_ss && c == SHARP_S {
            2 * fontmax_width(i32::from(b's') - 0x20)
        } else {
            fontmax_width(i32::from(c) - 0x20)
        }
    };
    walk.ops.clear();
    let size = t.len();
    let mut p = 0usize;
    let mut line = 0i32;
    let (mut pen_x, mut pen_y) = (0i32, 0i32);
    let to_cr = |mut q: usize| {
        while q < size && t[q] != 0x0d {
            q += 1;
        }
        q
    };
    while p < size && !walk_at(t, p, b"/fin") {
        let mut draw = true;
        // The hovered line's flag 0x004860a0 (atlantis-cd-zh-tw-696832
        // FUN_0041f5a0 @ 0x0041f6c9..0x0041f707): menu 2's lines 2 and 4
        // take FONTMAX back (@ 0x0041f711..0x0041f72a) but keep the flag, so
        // their sprite records are drawn hovered.
        let hover_line_flag =
            (line == hover_line && line != 0) || (line == hover_line && menu == 0);
        let mut hover = hover_line_flag;
        if (line == 2 || line == 4) && menu == 2 {
            hover = false;
        }
        if walk_at(t, p, b"<>") {
            // 0x0041fd08
            p += 3;
            let y = walk_atoi(t, p);
            p = walk_step(p, y, 2, true);
            let w: i32 = (p..size)
                .take_while(|&q| t[q] != 0x0d)
                .map(|q| if t[q] >= 0x20 { ss(t[q]) } else { 0 })
                .sum();
            pen_x = (640 - w) / 2;
            pen_y = y;
            let r = MenuRecord {
                x: (pen_x - 11) as i16,
                y: (y - 28) as i16,
                w: w as i16,
                h: 28,
            };
            rec.put(walk, r);
        }
        if walk_at(t, p, b"<") {
            // 0x0041fe42
            p += 1;
            let x = walk_atoi(t, p);
            p = walk_step(p, x, 2, true);
            let y = walk_atoi(t, p);
            p = walk_step(p, y, 2, true);
            pen_x = x;
            pen_y = y;
            // `imul ebx,[ebp-0x28],0x16`; `shl ebx,4` in
            // atlantis-cd-it-687104 (FUN_0041ecb4 @ 0x0041f068).
            let w = rules.positioned_byte_width * (to_cr(p).saturating_sub(p)) as i32;
            let r = MenuRecord {
                x: (x - 11) as i16,
                y: (y - 28) as i16,
                w: w as i16,
                h: 28,
            };
            rec.put(walk, r);
        }
        if walk_at(t, p, b"//") {
            // 0x0041ffb7
            p += 2;
            for k in 0..2 {
                let on = menu == 2
                    && if k == 0 {
                        line == 3 && toggle_a == 0
                    } else {
                        line == 5 && toggle_b == 0
                    };
                if !on {
                    continue;
                }
                let mut q = p; // strchr(p, '&') + 2
                while q < size && t[q] != b'&' {
                    q += 1;
                }
                p = q + 2;
            }
        }
        if walk_at(t, p, b"&&")
            && menu == 2
            && ((line == 3 && toggle_a == 1) || (line == 5 && toggle_b == 1))
        {
            // 0x0042002d
            p = to_cr(p) + 1;
            line += 1;
        }
        const SPR: [&[u8]; 4] = [b"/sprj=", b"/sprr=", b"/sprc=", b"/spr="];
        for (v, word) in SPR.iter().enumerate() {
            // 0x0042009d..0x004201be
            if !walk_at(t, p, word) {
                continue;
            }
            p += word.len();
            // FUN_004207c0: I (1-2 digits), X, Y, W (1-3 digits), H.
            let mut idx = walk_atoi(t, p);
            if rules.toggle_sprites {
                // atlantis-cd-zh-tw-696832 FUN_004201e8 @ 0x00420212..0x0042022b:
                // in menu 2, id 9 + SUBTITLES, the pointer one byte back.
                if idx == 9 && menu == 2 {
                    idx += toggle_a;
                    if !rules.toggle_in_step || idx > 9 {
                        p -= 1;
                    }
                }
                // @ 0x0042022e..0x00420242: id 12 becomes 13 with OMNI 3D
                // 1, in any menu. The menus other than OPTMENU walk with 0
                // here; none of the shipped ones holds an id 12 record.
                if idx == 12 && toggle_b == 1 {
                    idx = 13;
                }
            }
            p = walk_step(p, idx, 2, false);
            let x = walk_atoi(t, p);
            p = walk_step(p, x, 2, true);
            let y = walk_atoi(t, p);
            p = walk_step(p, y, 2, true);
            let w = walk_atoi(t, p);
            p = walk_step(p, w, 2, true);
            let h = walk_atoi(t, p);
            // FUN_004207c0 stores the parsed x / y only into a zero field
            // and otherwise reloads the stored one (x @ 0x00420910, y @
            // 0x004209b0; atlantis-cd-zh-tw-696832 FUN_004201e8 @
            // 0x00420412); the blit takes both (0x00420a7f..0x00420a95), in
            // every build: a slider keeps its knob, and the LOAD knob is
            // drawn where the picker put it.
            let mut r = rec.get(walk);
            let mut bx = x;
            if r.x == 0 {
                r.x = x as i16;
            } else {
                bx = i32::from(r.x);
            }
            let mut by = y;
            if r.y == 0 {
                r.y = y as i16;
            } else {
                by = i32::from(r.y);
            }
            r.w = w as i16;
            r.h = h as i16;
            walk.ops.push(MenuOp {
                sprite: true,
                alt_bank: v < 3,
                hover_font: v < 3 && hover_line_flag,
                x: bx as i16,
                y: by as i16,
                index: idx,
                line,
                record: rec.count as i32,
            });
            rec.put(walk, r);
            p = to_cr(p) + 1; // past the CR
            draw = false;
            line += 1;
        }
        if p >= size {
            break;
        }
        let mut c = i32::from(t[p]); // 0x004201c1
        if c == 0x0d {
            p += 1;
            pen_x = 0;
            pen_y += 28;
            draw = false;
            line += 1;
        }
        if (c < 0x20 && c != 0x0d) || (rules.high_bytes_as_space && c > 0xa8) {
            c = 0x20;
        }
        if c == 0x20 {
            pen_x += 0x16;
            draw = false;
        }
        c -= 0x20;
        if draw {
            let sharp_s = sharp_s_as_ss && c == i32::from(SHARP_S) - 0x20;
            let (glyph, n) = if sharp_s {
                (i32::from(b's') - 0x20, 2)
            } else {
                (c, 1)
            };
            for _ in 0..n {
                walk.ops.push(MenuOp {
                    sprite: false,
                    alt_bank: false,
                    hover_font: hover,
                    x: pen_x as i16,
                    y: pen_y as i16,
                    index: glyph,
                    line,
                    record: rec.count as i32 - 1,
                });
                pen_x += fontmax_width(glyph);
            }
        }
        p += 1;
    }
    match &rec.table {
        Some((t, m)) => walk.records = t.records(*m),
        None => {
            walk.records.truncate(rec.count);
            walk.records.resize(rec.count, MenuRecord::default());
        }
    }
}

/// `FUN_004210c0`: the first record strictly containing the point, its
/// coordinates read as u16 (a negative x / y is never hit).
pub fn menu_walk_hit(records: &[MenuRecord], mx: i32, my: i32) -> Option<usize> {
    records.iter().position(|r| {
        let (x, y) = (i32::from(r.x as u16), i32::from(r.y as u16));
        let (w, h) = (i32::from(r.w as u16), i32::from(r.h as u16));
        x < mx && mx < x + w && y < my && my < y + h
    })
}

/// `FUN_004211f0`: the cell under (mx, my) of a cols x rows grid at
/// (x0, y0) with cells w x h, its bounds inclusive on both sides (so the
/// far edge gives an index past the grid); column-major or row-major.
#[allow(clippy::too_many_arguments)]
pub fn menu_grid_hit(
    x0: i32,
    y0: i32,
    cell_w: i32,
    cell_h: i32,
    cols: i32,
    rows: i32,
    col_major: bool,
    mx: i32,
    my: i32,
) -> Option<i32> {
    if mx < x0 || mx > x0 + cols * cell_w || my < y0 || my > y0 + rows * cell_h {
        return None;
    }
    let (col, row) = ((mx - x0) / cell_w, (my - y0) / cell_h);
    Some(if col_major {
        col * rows + row
    } else {
        row * cols + col
    })
}

/// The FONTMIN renderers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontMin {
    /// `FUN_00420d8c`, the player list: a space advances 20, not drawn;
    /// '#' is neither drawn nor advanced.
    PlayerList,
    /// The 1997 build's player list (atlantis-cd-en-688128 FUN_0041fff4):
    /// as [`FontMin::PlayerList`], '#' drawn as any glyph.
    PlayerList1997,
    /// `FUN_00420f60`, the name field: a space advances 20, not drawn;
    /// glyph 0x2e advances 20.
    NameField,
    /// `FUN_00420b48`, the load list: glyph 0x20 advances 6, not drawn;
    /// the space is drawn and advances by the table.
    LoadList,
}

/// One FONTMIN glyph blit: pen x, baseline y, glyph index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontMinBlit {
    pub x: i32,
    pub y: i32,
    pub idx: u8,
}

pub fn font_min_blits(
    r: FontMin,
    text: &[u8],
    x: i32,
    y: i32,
    adv: &FontMinAdvances,
) -> Vec<FontMinBlit> {
    let mut out = Vec::new();
    let mut cx = x;
    for &c in text {
        if r != FontMin::LoadList && c == b' ' {
            cx += 20;
            continue;
        }
        if r == FontMin::PlayerList && c == b'#' {
            continue;
        }
        let idx = if c < 0x20 { 0x20 } else { c - 0x20 };
        if r == FontMin::LoadList && idx == 0x20 {
            cx += 6;
            continue;
        }
        out.push(FontMinBlit { x: cx, y, idx });
        cx += if r == FontMin::NameField && idx == 0x2e {
            20
        } else {
            adv.get(idx)
        };
    }
    out
}

/// The width `FUN_00420b48` records for a load-list row (0x00611e80).
pub fn font_min_load_list_width(text: &[u8], adv: &FontMinAdvances) -> i32 {
    text.iter()
        .map(|&c| {
            let idx = if c < 0x20 { 0x20 } else { c - 0x20 };
            if idx == 0x20 {
                6
            } else {
                adv.get(idx)
            }
        })
        .sum()
}

/// `FUN_0041cf78`'s scrollbar (0x0041d1f3..0x0041d24b at entry,
/// 0x0041d273..0x0041d2e6 each frame). The 1997 builds start the knob at
/// y 0 (atlantis-cd-en-688128 FUN_0041c9a8 @ 0x0041cc23,
/// atlantis-cd-fr-687104 FUN_0041c708 @ 0x0041c98a); with `clamp` false
/// (atlantis-cd-en-688128, atlantis-cd-fr-688640 FUN_0041ca00,
/// atlantis-cd-nl-688128 FUN_0041c9e0) it is not clamped above the bar;
/// atlantis-cd-fr-687104 (and the de / es / it 687104 builds) clamps it
/// as the reference (FUN_0041c708 @ 0x0041ca60..0x0041ca76), and
/// atlantis-cd-sv-688128 too (0x0041cd08..0x0041cd1e).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadPickerScroll {
    pub drag_on: bool,
    pub step: i32,
    /// Record 4's x when moved off-screen (0xffce), else 0: the file's.
    pub knob_x: i32,
    /// Record 4's y (0x00612774).
    pub knob_y: i32,
    /// The list offset 0x00496605.
    pub offset: u8,
    /// The clamp above the bar.
    pub clamp: bool,
}

impl LoadPickerScroll {
    /// With 11 files or fewer the knob is off-screen and never moves; else
    /// step = 0xd7 / (count - 10). The knob starts at y 0x7b when
    /// `at_bar`, else at 0.
    pub fn new(count: i32, at_bar: bool, clamp: bool) -> LoadPickerScroll {
        let knob_y = if at_bar { 0x7b } else { 0 };
        if count <= 11 {
            LoadPickerScroll {
                drag_on: false,
                step: 1,
                knob_x: i32::from(0xffce_u16 as i16),
                knob_y,
                offset: 0,
                clamp,
            }
        } else {
            LoadPickerScroll {
                drag_on: true,
                step: 0xd7 / (count - 10),
                knob_x: 0,
                knob_y,
                offset: 0,
                clamp,
            }
        }
    }

    /// While the track (record 1) is latched, the button down and
    /// 0x7b < mouse y < 0x152: knob y = mouse y - 10, offset = (knob y -
    /// 0x7b) / step as a byte (not clamped to count - 11).
    pub fn frame(&mut self, track_latched: bool, button_down: bool, mouse_y: i32) {
        if !track_latched || !button_down || !self.drag_on {
            return;
        }
        if mouse_y <= 0x7b || mouse_y >= 0x152 {
            return;
        }
        self.knob_y = i32::from((mouse_y - 10) as u16);
        self.offset = ((self.knob_y - 0x7b) / self.step) as u8;
        if self.clamp && mouse_y - 10 < 0x7b {
            self.knob_y = 0x7b;
            self.offset = 0;
        }
    }
}

/// The sound menu's OK: a knob at x gives the volume (x - 305) / 213 *
/// 127, rounded toward zero (`FUN_00451d6e`), 0..127.
pub fn menu_volume_from_knob(x: i32) -> u8 {
    let a = (f64::from(x - 305) / 213.0 * 127.0) as i32;
    a.clamp(0, 127) as u8
}

/// The options menu's SUBTITLES item (`FUN_0041d5f4`): on = !on; the
/// counter 0x00496098 steps; at 2 it forces off, at 3 it returns to 0.
/// Without `cycle` (the 1997 build, atlantis-cd-en-688128 FUN_0041cfa4 @
/// 0x0041d01b): on = !on alone.
pub fn subtitle_toggle_click(on: &mut bool, counter: &mut u8, cycle: bool) {
    *on = !*on;
    if !cycle {
        return;
    }
    *counter = counter.wrapping_add(1);
    if *counter == 2 {
        *on = false;
    }
    if *counter == 3 {
        *counter = 0;
    }
}

/// The code the original's ANSI window stores from WM_CHAR (0x00419c9a):
/// the Windows-1252 code of a typed character, 0 when it has none.
pub fn name_key_code(c: char) -> u8 {
    let u = u32::from(c);
    if u < 0x80 || (0xa0..=0xff).contains(&u) {
        return u as u8;
    }
    const CP1252: [(u32, u8); 32] = [
        (0x20ac, 0x80),
        (0x0081, 0x81),
        (0x201a, 0x82),
        (0x0192, 0x83),
        (0x201e, 0x84),
        (0x2026, 0x85),
        (0x2020, 0x86),
        (0x2021, 0x87),
        (0x02c6, 0x88),
        (0x2030, 0x89),
        (0x0160, 0x8a),
        (0x2039, 0x8b),
        (0x0152, 0x8c),
        (0x008d, 0x8d),
        (0x017d, 0x8e),
        (0x008f, 0x8f),
        (0x0090, 0x90),
        (0x2018, 0x91),
        (0x2019, 0x92),
        (0x201c, 0x93),
        (0x201d, 0x94),
        (0x2022, 0x95),
        (0x2013, 0x96),
        (0x2014, 0x97),
        (0x02dc, 0x98),
        (0x2122, 0x99),
        (0x0161, 0x9a),
        (0x203a, 0x9b),
        (0x0153, 0x9c),
        (0x009d, 0x9d),
        (0x017e, 0x9e),
        (0x0178, 0x9f),
    ];
    CP1252
        .iter()
        .find(|&&(code, _)| code == u)
        .map_or(0, |&(_, b)| b)
}

/// The byte the name entry stores for a typed character, `None` when it is
/// refused (the leading-space and length rules aside). `upper_case`: the
/// English 1997 build's 0x20..0x7a with a..z stored as A..Z
/// (atlantis-cd-en-688128 FUN_0041d6d0 @ 0x0041dab1..0x0041daf8); else
/// 0x20..0xa7 as typed (FUN_0041dea4). `dos_letters`: the Swedish build
/// also takes the Windows-1252 `Ä Å Ö ä å ö` and stores the DOS code page
/// bytes FONTMIN draws (atlantis-cd-sv-688128 FUN_0041d704 @
/// 0x0041dafa..0x0041dbe9: 0xc4 -> 0x8e, 0xc5 -> 0x8f, 0xd6 -> 0x99,
/// 0xe4 -> 0x84, 0xe5 -> 0x86, 0xf6 -> 0x94).
pub fn name_entry_byte(c: char, upper_case: bool, dos_letters: bool) -> Option<u8> {
    name_entry_code(name_key_code(c), upper_case, dos_letters)
}

/// [`name_entry_byte`] for the code the window stores
/// ([`name_key_code`]).
pub fn name_entry_code(code: u8, upper_case: bool, dos_letters: bool) -> Option<u8> {
    if dos_letters {
        const LETTERS: [(u8, u8); 6] = [
            (0xc4, 0x8e),
            (0xc5, 0x8f),
            (0xd6, 0x99),
            (0xe4, 0x84),
            (0xe5, 0x86),
            (0xf6, 0x94),
        ];
        if let Some(&(_, b)) = LETTERS.iter().find(|&&(w, _)| w == code) {
            return Some(b);
        }
    }
    let last = if upper_case { 0x7a } else { 0xa7 };
    if !(0x20..=last).contains(&code) {
        return None;
    }
    Some(if upper_case {
        code.to_ascii_uppercase()
    } else {
        code
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::exe::BUILDS;

    /// The three name entry rules: the English 1997 build's, the others',
    /// the Swedish build's letters.
    #[test]
    fn name_entry_bytes() {
        let typed = |s: &str, upper, dos| -> Vec<Option<u8>> {
            s.chars().map(|c| name_entry_byte(c, upper, dos)).collect()
        };
        assert_eq!(
            typed("az{\u{e9}", true, false),
            [Some(b'A'), Some(b'Z'), None, None]
        );
        assert_eq!(
            typed("az\u{a7}\u{a8}\u{e4}\u{2026}", false, false),
            [Some(b'a'), Some(b'z'), Some(0xa7), None, None, Some(0x85)]
        );
        let sv = typed("\u{c4}\u{c5}\u{d6}\u{e4}\u{e5}\u{f6}", false, true);
        assert_eq!(sv, [0x8e, 0x8f, 0x99, 0x84, 0x86, 0x94].map(Some));
        // Other letters past 0xa7 stay refused; the range is the others'.
        assert_eq!(
            typed("\u{e9}\u{fc}\u{c9}az ", false, true),
            [None, None, None, Some(b'a'), Some(b'z'), Some(b' ')]
        );
        // Code page 850 reads the stored bytes as the typed letters.
        for (c, b) in "\u{c4}\u{c5}\u{d6}\u{e4}\u{e5}\u{f6}".chars().zip(sv) {
            assert_eq!(atlantis_data::TextCodePage::Dos850.char_of(b.unwrap()), c);
        }
    }

    #[test]
    fn name_key_codes() {
        assert_eq!(name_key_code('A'), 0x41);
        assert_eq!(name_key_code('\u{a3}'), 0xa3);
        assert_eq!(name_key_code('\u{2026}'), 0x85);
        assert_eq!(name_key_code('\u{20ac}'), 0x80);
        assert_eq!(name_key_code('\u{153}'), 0x9c);
        assert_eq!(name_key_code('\u{e9}'), 0xe9);
        assert_eq!(name_key_code('\u{101}'), 0);
        assert_eq!(name_key_code('\u{81}'), 0x81);
    }

    /// The 1997 build: '#' is a glyph of the player list, the SUBTITLES
    /// item a plain toggle, the LOAD knob starting at 0 and not clamped.
    #[test]
    fn rules_of_the_1997_build() {
        let idx = |r| -> Vec<u8> {
            font_min_blits(r, b"A#B", 0, 0, &FontMinAdvances::default())
                .iter()
                .map(|b| b.idx + 0x20)
                .collect()
        };
        assert_eq!(idx(FontMin::PlayerList), b"AB");
        assert_eq!(idx(FontMin::PlayerList1997), b"A#B");
        let (mut on, mut n) = (false, 0);
        let seq: Vec<bool> = (0..4)
            .map(|_| {
                subtitle_toggle_click(&mut on, &mut n, false);
                on
            })
            .collect();
        assert_eq!(seq, [true, false, true, false]);
        assert_eq!(n, 0);
        let mut sb = LoadPickerScroll::new(20, false, false);
        assert_eq!((sb.knob_y, sb.step), (0, 21));
        sb.frame(true, true, 0x80);
        assert_eq!((sb.knob_y, sb.offset), (0x76, 0));
        let mut sb = LoadPickerScroll::new(20, true, true);
        assert_eq!(sb.knob_y, 0x7b);
        sb.frame(true, true, 0x80);
        assert_eq!((sb.knob_y, sb.offset), (0x7b, 0));
        // atlantis-cd-fr-687104 FUN_0041c708: the knob from 0, clamped.
        let mut sb = LoadPickerScroll::new(20, false, true);
        assert_eq!(sb.knob_y, 0);
        sb.frame(true, true, 0x80);
        assert_eq!((sb.knob_y, sb.offset), (0x7b, 0));
    }

    #[test]
    fn fontmin_reads_on_into_the_credits_table() {
        let mut credits = [0u8; 256];
        credits[0x1c] = 17;
        credits[0x1e] = 2;
        credits[0x1f] = 10;
        credits[122] = 9;
        let adv = FontMinAdvances::new(&credits);
        assert_eq!(adv.get(0x21), 17); // 'A'
        assert_eq!(adv.get(100), 15);
        assert_eq!(
            [
                adv.get(0xa1 - 0x20),
                adv.get(0xa3 - 0x20),
                adv.get(0xa4 - 0x20)
            ],
            [17, 2, 10]
        );
        assert_eq!(adv.get(0xff - 0x20), 9);
        let xs: Vec<i32> = font_min_blits(FontMin::NameField, b"\xa3\xa4A", 0, 0, &adv)
            .iter()
            .map(|b| b.x)
            .collect();
        assert_eq!(xs, [0, 2, 12]);
    }

    /// `ß` (0xe1): no glyph, the advance of the table's byte 193 (the
    /// credits byte 92); with the fix, two 's' glyphs with their advances,
    /// in the rows and in the width the title is centred with.
    #[test]
    fn sharp_s_both_ways() {
        let mut credits = [0u8; 256];
        credits[92] = 7;
        let adv = FontMinAdvances::new(&credits);
        let text = b"verl\x84\xe1t";
        // The original: glyph 193 (past the 137 sprites, so not drawn).
        let raw = font_min_blits(FontMin::LoadList, &spell_sharp_s(text, false), 0, 0, &adv);
        let idx: Vec<u8> = raw.iter().map(|b| b.idx).collect();
        assert_eq!(idx, [0x56, 0x45, 0x52, 0x4c, 0x64, 193, 0x54]);
        assert_eq!(raw[6].x - raw[5].x, 7);
        let w_raw = font_min_load_list_width(&spell_sharp_s(text, false), &adv);
        assert_eq!(w_raw, raw[6].x + adv.get(0x54));
        // The fix: "verlässt".
        let fixed_text = spell_sharp_s(text, true);
        assert_eq!(&*fixed_text, b"verl\x84sst");
        let fixed = font_min_blits(FontMin::LoadList, &fixed_text, 0, 0, &adv);
        let idx: Vec<u8> = fixed.iter().map(|b| b.idx).collect();
        assert_eq!(idx, [0x56, 0x45, 0x52, 0x4c, 0x64, 0x53, 0x53, 0x54]);
        let s_adv = adv.get(0x53);
        assert_eq!(fixed[6].x - fixed[5].x, s_adv);
        assert_eq!(fixed[7].x - fixed[6].x, s_adv);
        let w_fixed = font_min_load_list_width(&fixed_text, &adv);
        assert_eq!(w_fixed, w_raw - 7 + 2 * s_adv);
        // Texts without 0xe1 are borrowed as they are.
        assert!(matches!(
            spell_sharp_s(b"abc", true),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// The menu walk: 0xe1 one glyph 193 with the FONTMAX advance (0, past
    /// the table) as in the original; with the fix two 's' glyphs, and the
    /// centred line measured with them.
    #[test]
    fn sharp_s_in_menu_walk() {
        let text = b"<>,100,\xe1a\r\n/fin";
        let mut walk = MenuWalk::default();
        let glyphs = |w: &MenuWalk| -> Vec<(i32, i16)> {
            w.ops
                .iter()
                .filter(|o| !o.sprite)
                .map(|o| (o.index, o.x))
                .collect()
        };
        let (s, a) = (fontmax_width(0x53), fontmax_width(0x41));
        walk_menu_text(text, 1, -1, 0, 0, MenuRules::default(), &mut walk);
        let x0 = ((640 - a) / 2) as i16;
        assert_eq!(glyphs(&walk), [(193, x0), (0x41, x0)]);
        assert_eq!(i32::from(walk.records[0].w), a);
        walk_menu_text(
            text,
            1,
            -1,
            0,
            0,
            MenuRules {
                sharp_s_as_ss: true,
                ..MenuRules::default()
            },
            &mut walk,
        );
        let x0 = ((640 - a - 2 * s) / 2) as i16;
        let s16 = s as i16;
        assert_eq!(
            glyphs(&walk),
            [(0x53, x0), (0x53, x0 + s16), (0x41, x0 + 2 * s16)]
        );
        assert_eq!(i32::from(walk.records[0].w), a + 2 * s);
    }

    /// The Chinese builds' options toggles as sprite records: id 9 +
    /// SUBTITLES, id 12 -> 13 with OMNI 3D; with SUBTITLES off the record
    /// is read one byte out of step (x 0, the rest shifted), and keeps the
    /// x / y a previous walk stored; the sprite is drawn there.
    #[test]
    fn toggle_sprite_records() {
        let text = b"/sprc=2,260,50,150,32\r\n/sprc=8,160,250,100,32\r\n/sprc=9,345,267,51,33\r\n/sprc=12,344,300,250,32\r\n/fin\r\n";
        let rules = MenuRules {
            toggle_sprites: true,
            ..MenuRules::default()
        };
        let sprites = |w: &MenuWalk| -> Vec<(i32, i16, i16, bool)> {
            w.ops
                .iter()
                .map(|o| (o.index, o.x, o.y, o.hover_font))
                .collect()
        };
        let mut walk = MenuWalk::default();
        walk_menu_text(text, 2, 1, 1, 1, rules, &mut walk);
        assert!(walk.ops.iter().all(|o| o.sprite && o.alt_bank));
        assert_eq!(
            sprites(&walk)[2..],
            [(10, 345, 267, false), (13, 344, 300, false)]
        );
        assert_eq!(
            walk.records[2],
            MenuRecord {
                x: 345,
                y: 267,
                w: 51,
                h: 33
            }
        );
        // SUBTITLES off: x 0 (atoi of ","), y 45, w 267, h 51; the stored
        // x / y kept.
        walk_menu_text(text, 2, -1, 0, 0, rules, &mut walk);
        assert_eq!(
            sprites(&walk)[2..],
            [(9, 345, 267, false), (12, 344, 300, false)]
        );
        assert_eq!(
            walk.records[2],
            MenuRecord {
                x: 345,
                y: 267,
                w: 267,
                h: 51
            }
        );
        // In a first walk the shifted values are stored.
        let mut fresh = MenuWalk::default();
        walk_menu_text(text, 2, -1, 0, 0, rules, &mut fresh);
        assert_eq!(sprites(&fresh)[2], (9, 0, 45, false));
        // The SUBTITLES record fix reads it in step.
        let fixed = MenuRules {
            toggle_in_step: true,
            ..rules
        };
        let mut fresh = MenuWalk::default();
        walk_menu_text(text, 2, -1, 0, 0, fixed, &mut fresh);
        assert_eq!(sprites(&fresh)[2], (9, 345, 267, false));
        assert_eq!((fresh.records[2].w, fresh.records[2].h), (51, 33));
        // Hovered lines: the label of line 1 too (menu 2's lines 2 and 4
        // keep the flag); line 0 never, but in menu 0.
        walk_menu_text(text, 2, 1, 1, 0, rules, &mut walk);
        assert!(sprites(&walk)[1].3);
        walk_menu_text(text, 2, 0, 1, 0, rules, &mut walk);
        assert!(!sprites(&walk)[0].3);
        walk_menu_text(text, 0, 0, 1, 0, rules, &mut walk);
        assert!(sprites(&walk)[0].3);
        // Id 9 changes in menu 2 only, id 12 in any menu; without the
        // rule, neither.
        walk_menu_text(text, 1, -1, 1, 1, rules, &mut fresh);
        let ids: Vec<i32> = fresh.ops.iter().map(|o| o.index).collect();
        assert_eq!(ids, [2, 8, 9, 13]);
        walk_menu_text(text, 2, -1, 1, 1, MenuRules::default(), &mut fresh);
        let ids: Vec<i32> = fresh.ops.iter().map(|o| o.index).collect();
        assert_eq!(ids, [2, 8, 9, 12]);
    }

    /// A sprite record keeps a stored non-zero x and y and is drawn there
    /// (the LOAD knob), whatever the build; its w / h are written again.
    #[test]
    fn sprite_record_drawn_at_stored_position() {
        let text = b"/spr=13,600,130,32,32\r\n/fin\r\n";
        for b in BUILDS.iter() {
            let mut walk = MenuWalk {
                records: vec![MenuRecord {
                    x: 0,
                    y: 0x7b,
                    w: 0,
                    h: 0,
                }],
                ops: Vec::new(),
            };
            walk_menu_text(
                text,
                1,
                -1,
                0,
                0,
                MenuRules::of_build(b, &crate::enhancements::BugFixes::NONE),
                &mut walk,
            );
            assert_eq!((walk.ops[0].x, walk.ops[0].y), (600, 0x7b), "{}", b.name);
            assert!(!walk.ops[0].alt_bank);
            assert_eq!(walk.records[0].y, 0x7b);
        }
    }

    /// A redraw over a stored sprite record: its x / y kept and blitted
    /// there, w / h written again; a fresh record takes the text's. Values
    /// of the original walk run under emulation with record 0 set to (50,
    /// 77, 0, 0) beforehand: record (50, 77, 30, 40), sprite 3 at (50, 77);
    /// the positioned line's record (-6, -19, 22, 28).
    #[test]
    fn walk_redraw_keeps_stored_position() {
        let text = b"/spr=3,100,200,30,40\r\n<5,9>X\r\n/fin\r\n";
        let mut walk = MenuWalk {
            records: vec![MenuRecord {
                x: 50,
                y: 77,
                w: 0,
                h: 0,
            }],
            ops: Vec::new(),
        };
        walk_menu_text(text, 1, -1, 0, 0, MenuRules::default(), &mut walk);
        let rec = |r: &MenuRecord| (r.x, r.y, r.w, r.h);
        let recs: Vec<_> = walk.records.iter().map(rec).collect();
        assert_eq!(recs, [(50, 77, 30, 40), (-6, -19, 22, 28)]);
        let sprites: Vec<_> = walk
            .ops
            .iter()
            .filter(|o| o.sprite)
            .map(|o| (o.index, o.x, o.y))
            .collect();
        assert_eq!(sprites, [(3, 50, 77)]);
        let mut fresh = MenuWalk::default();
        walk_menu_text(text, 1, -1, 0, 0, MenuRules::default(), &mut fresh);
        assert_eq!((fresh.ops[0].x, fresh.ops[0].y), (100, 200));
    }

    #[test]
    fn menu_numbers() {
        assert_eq!(menu_number("MAINMENU.TXT"), Some(0));
        assert_eq!(menu_number("optmenu.txt"), Some(2));
        assert_eq!(menu_number("ERROR3.TXT"), Some(14));
        assert_eq!(menu_number("CREDITS.TXT"), None);
    }
}
