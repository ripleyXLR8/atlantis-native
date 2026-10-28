//! The game's 2D interface: the menus, the player screen, the inventory
//! bar, the guests' portraits and the credits.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`menus.cpp`,
//! `menu_layout.cpp`, `toolbar.cpp`, `inventory.cpp`, `bar_session.h`,
//! `credits_layout.h`; GPLv3, the ScummVM team).

pub mod bar;
pub mod credits;
pub mod layout;
pub mod menus;
pub mod player_screen;

use atlantis_data::sprite::{limits, read_spr_file, SprSprite};

use crate::engine::Engine;
use crate::warp::{SCREEN_H, SCREEN_W};
use layout::{font_min_blits, FontMin, FontMinAdvances, MenuWalk};

/// The sprite banks of the interface (`SPRITE\2D\`).
#[derive(Clone, Debug, Default)]
pub struct UiAssets {
    /// FONTMAX.SPR, the menu font; FONTMAX2.SPR, its hovered glyphs.
    pub fontmax: Vec<SprSprite>,
    pub fontmax2: Vec<SprSprite>,
    /// FONTMIN.SPR, the player-name font; FONTMIN2.SPR, its hovered glyphs.
    pub fontmin: Vec<SprSprite>,
    pub fontmin2: Vec<SprSprite>,
    /// Where the FONTMIN.SPR loaded was read (archive file, offset).
    pub fontmin_from: Option<(String, u64)>,
    /// FONTMIN's advances, past its table the build's credits table's.
    pub fontmin_advance: FontMinAdvances,
    /// SPRMENU.SPR, the menus' ornaments.
    pub sprmenu: Vec<SprSprite>,
    /// OBJETS1.SPR, the objects' icons (index = the exe's table index).
    pub objects: Vec<SprSprite>,
    /// SUIVEUR.SPR, the guests' portraits.
    pub guests: Vec<SprSprite>,
    /// OBJETS0.SPR, the inventory bar.
    pub bar: Option<SprSprite>,
    /// The menus' `/sprj=` `/sprr=` `/sprc=` banks (normal, hovered): the
    /// Chinese builds' SPRCHI1.SPR / SPRCHI2.SPR
    /// ([`atlantis_data::exe::ExeBuild::menu_sprite_banks`]); empty for the
    /// other builds, which never load them.
    pub menu_banks: [Vec<SprSprite>; 2],
    /// The LOAD list's rows (normal, hovered) by position: the Chinese
    /// builds' EPICHI1.SPR / EPICHI2.SPR ([`atlantis_data::exe::LoadList`]).
    pub load_banks: [Vec<SprSprite>; 2],
}

/// A whole screen.
pub type Screen = Vec<u16>;

/// Draws a sprite with its hotspot at (x, y) on a 640x480 screen.
pub fn draw_sprite(screen: &mut [u16], spr: &SprSprite, x: i32, y: i32) {
    spr.draw(screen, SCREEN_W, SCREEN_H, x, y);
}

/// Draws a sprite clipped to the rows from `top` down.
pub fn draw_sprite_below(screen: &mut [u16], spr: &SprSprite, x: i32, y: i32, top: usize) {
    if top >= SCREEN_H {
        return;
    }
    spr.draw(
        &mut screen[top * SCREEN_W..],
        SCREEN_W,
        SCREEN_H - top,
        x,
        y - top as i32,
    );
}

/// `FUN_004108e0` (through `FUN_0041cc28`): every pixel dimmed, red and
/// blue to c - c / 2, green to g - ((g & 0x3c) >> 1) (mask 0xf79e).
pub fn dim_screen(screen: &mut [u16]) {
    for p in screen.iter_mut() {
        *p -= (*p & 0xf79e) >> 1;
    }
}

impl UiAssets {
    /// The blits of a menu walk: SPRMENU sprites and FONTMAX glyphs
    /// (FONTMAX2 on the hovered line), in file order. The alternate bank of
    /// `/sprj=` `/sprr=` `/sprc=` is never loaded by the executable but in
    /// the Chinese builds, which draw it from SPRCHI1, SPRCHI2 on the
    /// hovered line (atlantis-cd-zh-tw-696832 FUN_004201e8 @
    /// 0x004204fa..0x00420543).
    pub fn draw_menu(&self, screen: &mut [u16], walk: &MenuWalk) {
        for op in &walk.ops {
            let bank = if op.sprite {
                if op.alt_bank {
                    match &self.menu_banks {
                        [normal, _] if normal.is_empty() => continue,
                        [_, hovered] if op.hover_font => hovered,
                        [normal, _] => normal,
                    }
                } else {
                    &self.sprmenu
                }
            } else if op.hover_font && !self.fontmax2.is_empty() {
                &self.fontmax2
            } else {
                &self.fontmax
            };
            if let Some(spr) = usize::try_from(op.index).ok().and_then(|i| bank.get(i)) {
                draw_sprite(screen, spr, i32::from(op.x), i32::from(op.y));
            }
        }
    }

    /// FONTMIN text (FONTMIN2 when `hover`) with one of the renderers.
    pub fn draw_font_min(
        &self,
        screen: &mut [u16],
        r: FontMin,
        text: &[u8],
        x: i32,
        y: i32,
        hover: bool,
    ) {
        let bank = if hover && !self.fontmin2.is_empty() {
            &self.fontmin2
        } else {
            &self.fontmin
        };
        for b in font_min_blits(r, text, x, y, &self.fontmin_advance) {
            if let Some(spr) = bank.get(usize::from(b.idx)) {
                draw_sprite(screen, spr, b.x, b.y);
            }
        }
    }

    /// The icon of OBJETS1.SPR sprite `icon`.
    pub fn object(&self, icon: usize) -> Option<&SprSprite> {
        self.objects.get(icon).filter(|s| !s.pixels.is_empty())
    }
}

/// The characters following the player ("guests"): 5-byte records {int32
/// id, byte sprite} at 0x006cd7fc, count 0x0077290e. Fixed slots, so that a
/// slot reused by AddGuest keeps the sprite byte it held when the id is not
/// in the sprite table, as in the original.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guests {
    pub id: [i32; Guests::SLOTS],
    pub sprite: [u8; Guests::SLOTS],
    pub count: usize,
    /// 0x00496880: the portraits' y, 350 in the image; the bar's slide-in
    /// raises them 7 a frame and the slide-out lowers them back. Not reset
    /// with the list.
    pub portrait_y: i32,
    /// 0x00496144 (0 in the exe image): set by a follower portrait click
    /// (`FUN_0042c8ac` @ 0x0042cc1d), cleared only at the end of a
    /// conversation (`FUN_00425828` @ 0x00425bdb), so the object-in-hand
    /// path, which returns at 0x004258b7, leaves it set for the next
    /// conversation. Not reset with the list (no memset of `FUN_004239f4`
    /// covers it).
    pub talk: bool,
}

impl Default for Guests {
    fn default() -> Self {
        Guests {
            id: [0; Guests::SLOTS],
            sprite: [0; Guests::SLOTS],
            count: 0,
            portrait_y: 350,
            talk: false,
        }
    }
}

impl Guests {
    pub const SLOTS: usize = 12;
    /// The sprite of a record not drawn.
    pub const HIDDEN: u8 = 0xff;

    pub fn clear(&mut self) {
        self.id = [0; Guests::SLOTS];
        self.sprite = [0; Guests::SLOTS];
        self.count = 0;
    }

    /// `FUN_0042b480` (list part): append {id, index of id in `table`}.
    pub fn add(&mut self, guest: i32, table: &[i32]) {
        if self.count >= Guests::SLOTS {
            return;
        }
        self.id[self.count] = guest;
        if let Some(i) = table.iter().position(|&t| t == guest) {
            self.sprite[self.count] = i as u8;
        }
        self.count += 1;
    }

    /// Removes record `i`, the following ones move down.
    pub fn remove_at(&mut self, i: usize) {
        for k in i..self.count {
            if k + 1 < Guests::SLOTS {
                self.id[k] = self.id[k + 1];
                self.sprite[k] = self.sprite[k + 1];
            }
        }
        self.count -= 1;
    }

    /// `FUN_0042bdb8`: the guest whose portrait cell holds (x, y), 0 for
    /// none. Grid `FUN_004211f0` at x0 = 540 - 100 * (count - 1), y0 =
    /// portrait_y, cells 85 x 100, count columns, one row, edges inclusive;
    /// the record is count - 1 - cell. On the right and bottom edge pixels
    /// the original reads the memory before the list (see
    /// [`guest_edge_original`]); the port returns 0.
    pub fn at(&self, x: i32, y: i32) -> i32 {
        if self.count == 0 {
            return 0;
        }
        let n = self.count as i32;
        let x0 = 540 - 100 * (n - 1);
        let y0 = self.portrait_y;
        if x < x0 || x > x0 + n * 85 || y < y0 || y > y0 + 100 {
            return 0;
        }
        let cell = ((y - y0) / 100) * n + (x - x0) / 85;
        let k = n - 1 - cell;
        if k >= 0 {
            self.id[k as usize]
        } else {
            0
        }
    }
}

/// What `FUN_0042bdb8` reads for record -1 (the right or bottom edge pixel
/// of a one-guest grid): the 4 bytes before the list, the high byte of
/// ClicPerso and the low 3 bytes of the zone left under the pointer.
pub fn guest_edge_original(clic_perso: u32, zone: u32) -> u32 {
    (zone << 8) | (clic_perso >> 24)
}

/// The bar request 0x004960c4 and the right-press flag 0x00496120.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BarSession {
    pub request: bool,
    pub right_press: bool,
}

impl BarSession {
    /// The bar loop's entry (`FUN_0042c8ac` @ 0x0042c8c4).
    pub fn opened(&mut self) {
        self.request = true;
    }

    /// The input flag resets of a video end (0x0041bf5b..0x0041bf8d), a
    /// Cinema=, a StartMusik= that closes the open stream, a StopMusik=
    /// with a stream open, the end of a bar session and the warp loop's
    /// entry.
    pub fn flags_reset(&mut self) {
        *self = BarSession::default();
    }

    /// The end of a voice-only line that loaded its voice (0x004305cb).
    pub fn voice_line_ended(&mut self) {
        self.request = false;
    }

    /// A right press seen by `FUN_0041a298` (@ 0x0041a2f2..0x0041a337):
    /// the flag, and the request toggles unless a clicked section plays or
    /// the menu is requested.
    pub fn right_pressed(&mut self, section_play: bool, menu_request: bool) {
        self.right_press = true;
        if !section_play && !menu_request {
            self.request = !self.request;
        }
    }

    /// The warp loop's frame start: the request a timer section left (a
    /// right press during its lines) opens the bar, then the flags clear
    /// (0x0041c04a..0x0041c07c).
    pub fn warp_frame_start(&mut self) -> bool {
        let pending = self.request;
        self.flags_reset();
        pending
    }
}

/// The icon (OBJETS1.SPR index) of an object: the last entry of the
/// table 0x004967b0 with its id (`FUN_0042b328`).
pub fn object_icon(table: &[i32], id: i32) -> Option<usize> {
    table.iter().rposition(|&t| t == id && id != 0)
}

impl Engine {
    /// What the interface needs before the game: the cursors, the sprite
    /// banks, EPI.TXT, the settings' volumes and the player list.
    pub async fn load_interface(&mut self) {
        self.load_cursor_sprites().await;
        self.load_ui_assets().await;
        self.load_players();
    }

    /// Loads the interface's sprite banks.
    pub(crate) async fn load_ui_assets(&mut self) {
        async fn bank(e: &Engine, name: &str, lim: (u16, u16)) -> Vec<SprSprite> {
            match e.read(&format!("SPRITE\\2D\\{name}")).await {
                Some(d) => read_spr_file(&d, lim.0, lim.1).unwrap_or_default(),
                None => Vec::new(),
            }
        }
        let d = limits::DEFAULT;
        let build = self.game.exe.build;
        // The Chinese builds load their menu words instead of FONTMAX /
        // FONTMAX2 (atlantis-cd-zh-tw-696832 FUN_0042676c @
        // 0x004267a2..0x00426874), and the LOAD rows' banks
        // (FUN_0041ce00 @ 0x0041cf19..0x0041cfae, at each entry of the
        // picker: the four discs hold the same copies).
        let (fontmax, fontmax2, menu_banks) = match build.menu_sprite_banks {
            Some([a, b]) => (
                Vec::new(),
                Vec::new(),
                [bank(self, a, d).await, bank(self, b, d).await],
            ),
            None => (
                bank(self, "FONTMAX.SPR", d).await,
                bank(self, "FONTMAX2.SPR", d).await,
                [Vec::new(), Vec::new()],
            ),
        };
        let load_banks = match build.load_list {
            atlantis_data::LoadList::Sprites { banks: [a, b], .. } => {
                [bank(self, a, d).await, bank(self, b, d).await]
            }
            atlantis_data::LoadList::EpiText => [Vec::new(), Vec::new()],
        };
        let mut a = UiAssets {
            fontmax,
            fontmax2,
            fontmin: bank(self, "FONTMIN.SPR", d).await,
            fontmin2: bank(self, "FONTMIN2.SPR", d).await,
            fontmin_from: self
                .game
                .big
                .locate("SPRITE\\2D\\FONTMIN.SPR")
                .map(|(file, at, _)| (file, at)),
            fontmin_advance: FontMinAdvances::new(&self.game.exe.credit_widths),
            sprmenu: bank(self, "SPRMENU.SPR", d).await,
            objects: bank(self, "OBJETS1.SPR", d).await,
            guests: bank(self, "SUIVEUR.SPR", d).await,
            bar: None,
            menu_banks,
            load_banks,
        };
        a.bar = bank(self, "OBJETS0.SPR", limits::INVENTORY_BAR)
            .await
            .into_iter()
            .next()
            .filter(|s| !s.pixels.is_empty());
        if let Some(t) = self.read("SPRLIST\\EPI.TXT").await {
            self.epi_text = t;
        }
        self.ui = std::rc::Rc::new(a);
    }

    /// A member the original reads from the open archive at the time it
    /// uses it, so from the disc in the drive: the current disc's copy
    /// (0x00772913). With the disc revision's fix
    /// ([`crate::enhancements::BugFix::DiscRevision`]), the first copy, disc 1's:
    /// the Dutch CD's disc 2 holds an older revision of CREDITS.TXT and
    /// FONTMIN / FONTMIN2, the German May CD's discs 2..4 another
    /// CREDBLAN.SPR (the others carry the same bytes on every disc).
    pub async fn read_from_current_disc(&self, name: &str) -> Option<Vec<u8>> {
        let disc = self.current_disc_copy();
        self.game.big.set_disc_copy(disc);
        let data = self.read(name).await;
        self.game.big.set_disc_copy(None);
        data
    }

    /// FONTMIN.SPR and FONTMIN2.SPR from the current disc, as the LOAD
    /// picker and the player screen load them on entry
    /// (atlantis-cd-en-714240 FUN_0041cf78 @ 0x0041cfc2, FUN_0041dea4 @
    /// 0x0041def3; atlantis-cd-nl-688128 FUN_0041c9e0 @
    /// 0x0041ca29..0x0041caaa, FUN_0041d70c @ 0x0041d75b..0x0041d7c3;
    /// atlantis-cd-sv-688128 FUN_0041c9b0 @ 0x0041ca0d / 0x0041ca75,
    /// FUN_0041d704 @ 0x0041d766 / 0x0041d7ce).
    /// Read again only when that copy is not the one loaded.
    pub async fn load_fontmin_from_current_disc(&mut self) {
        const FONTMIN: &str = "SPRITE\\2D\\FONTMIN.SPR";
        let disc = self.current_disc_copy();
        self.game.big.set_disc_copy(disc);
        let from = self
            .game
            .big
            .locate(FONTMIN)
            .map(|(file, at, _)| (file, at));
        self.game.big.set_disc_copy(None);
        if from.is_none() || from == self.ui.fontmin_from {
            return;
        }
        let d = limits::DEFAULT;
        let bank = |data: Option<Vec<u8>>| {
            data.and_then(|d2| read_spr_file(&d2, d.0, d.1))
                .unwrap_or_default()
        };
        let fontmin = bank(self.read_from_current_disc(FONTMIN).await);
        let fontmin2 = bank(
            self.read_from_current_disc("SPRITE\\2D\\FONTMIN2.SPR")
                .await,
        );
        let ui = std::rc::Rc::make_mut(&mut self.ui);
        ui.fontmin = fontmin;
        ui.fontmin2 = fontmin2;
        ui.fontmin_from = from;
    }

    /// CREDBLAN.SPR and CREDBLEU.SPR, the credits' white and blue fonts,
    /// from the current disc: every build's credits function loads them at
    /// the start of each run from the archive open then (0x52 bytes into
    /// it: atlantis-cd-en-714240 FUN_0041a72c @ 0x0041a77e / 0x0041a7e4,
    /// atlantis-cd-en-688128 FUN_0041a5f8 @ 0x0041a64a / 0x0041a6b0,
    /// atlantis-cd-fr-687104 FUN_0041a1f0 @ 0x0041a242 / 0x0041a2a8,
    /// atlantis-cd-de-693248 FUN_0041a6a4 @ 0x0041a6f6 / 0x0041a75c...).
    /// The German May 1997 CD's disc 1 holds another CREDBLAN.SPR than its
    /// discs 2..4; every other release, the same on each disc.
    pub async fn load_credits_fonts(&self) -> [Vec<SprSprite>; 2] {
        let d = limits::DEFAULT;
        let bank = |data: Option<Vec<u8>>| {
            data.and_then(|b| read_spr_file(&b, d.0, d.1))
                .unwrap_or_default()
        };
        [
            bank(
                self.read_from_current_disc("SPRITE\\2D\\CREDBLAN.SPR")
                    .await,
            ),
            bank(
                self.read_from_current_disc("SPRITE\\2D\\CREDBLEU.SPR")
                    .await,
            ),
        ]
    }

    /// The pointer of the bar loops, `FUN_0042d558`: CURSEURS.SPR frame 6
    /// at the mouse (0x0042d5b9..0x0042d5dd), then the object in hand over
    /// it at the same position when OBJETENMAIN != 0
    /// (0x0042d600..0x0042d627).
    pub(crate) fn draw_pointer(&self, screen: &mut [u16]) {
        let (x, y) = self.sys.mouse();
        if let Some(spr) = self.cursor_sprites.get(6) {
            draw_sprite(screen, spr, x, y);
        }
        let held = self.vars.get("objetenmain");
        if held != 0 {
            if let Some(spr) = object_icon(&self.game.exe.inventory_object_ids, held)
                .and_then(|i| self.ui.object(i))
            {
                draw_sprite(screen, spr, x, y);
            }
        }
    }

    /// `FUN_0042bc58`: every guest record not hidden at (540 - 100 i,
    /// portrait_y), when they are shown; then a moving portrait.
    pub(crate) fn draw_guests(&self, screen: &mut [u16], moving: Option<(usize, i32)>) {
        if !self.guests_shown {
            return;
        }
        let g = &self.guests;
        for i in 0..g.count.min(Guests::SLOTS) {
            let s = g.sprite[i];
            if s != Guests::HIDDEN {
                if let Some(spr) = self.ui.guests.get(usize::from(s)) {
                    draw_sprite(screen, spr, 540 - 100 * i as i32, g.portrait_y);
                }
            }
        }
        if let Some((s, x)) = moving {
            if let Some(spr) = self.ui.guests.get(s) {
                draw_sprite(screen, spr, x, g.portrait_y);
            }
        }
    }

    /// The guest under (x, y) when they are shown.
    pub(crate) fn guest_at(&self, x: i32, y: i32) -> i32 {
        if self.guests_shown {
            self.guests.at(x, y)
        } else {
            0
        }
    }

    /// The portraits' y during a bar slide: -7 a frame up (`FUN_0042c2c4`),
    /// +7 down while above 350 (`FUN_0042c59c`), with guests shown.
    pub(crate) fn slide_guest_portraits(&mut self, dy: i32) {
        if self.guests.count == 0 || !self.guests_shown {
            return;
        }
        if dy < 0 || self.guests.portrait_y < 350 {
            self.guests.portrait_y += dy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guests_list() {
        let table = [600, 587, 601];
        let mut g = Guests::default();
        g.add(587, &table);
        assert_eq!((g.count, g.sprite[0]), (1, 1));
        // One guest: the cell at (540, 350), 85 x 100.
        assert_eq!(g.at(540, 350), 587);
        assert_eq!(g.at(624, 449), 587);
        assert_eq!(g.at(625, 400), 0); // the edge column: record -1
        assert_eq!(g.at(540, 450), 0); // the edge row
        assert_eq!(g.at(539, 400), 0);
        g.add(601, &table);
        assert_eq!(g.at(440, 400), 601);
        assert_eq!(g.at(530, 400), 587);
        g.remove_at(0);
        assert_eq!((g.count, g.id[0], g.sprite[0]), (1, 601, 2));
    }

    #[test]
    fn bar_session() {
        let mut s = BarSession::default();
        s.right_pressed(false, false);
        assert!(s.request && s.right_press);
        s.right_pressed(true, false);
        assert!(s.request);
        assert!(s.warp_frame_start());
        assert_eq!(s, BarSession::default());
        s.opened();
        s.voice_line_ended();
        assert!(!s.request);
    }

    #[test]
    fn dim() {
        let mut s = vec![0xffff, 0x0000, 0x0842];
        dim_screen(&mut s);
        assert_eq!(
            s,
            vec![0xffff - (0xf79e >> 1), 0, 0x0842 - ((0x0842 & 0xf79e) >> 1)]
        );
    }
}
