//! The screens' banks and drawing in the game ([`crate::maps`] has the
//! maps' rules): `AtlCarte` (mode 9, @ 0x00405700), `AtlCarteTour` (mode
//! 10, @ 0x00406780) and `AtlEcrin` (mode 14, @ 0x00412fc0), each drawn
//! over the running warp frame (@ 0x00440690); the sprite calls the C
//! code makes itself (`AtlSpr_Aff` @ 0x004367b0, `AtlSpr_AffCentre`
//! @ 0x00436b80).

use std::sync::Arc;

use atlantis3_data::spr::{Bank, Depth};

use crate::inventory::aff_at;
use crate::maps;

use super::Game;

impl Game {
    /// A screen bank read from `spr\`.
    pub(crate) fn screen_bank(&self, name: &str) -> Option<Arc<Bank>> {
        let path = format!("spr/{name}");
        let b = self
            .files
            .read(&path)
            .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok());
        if b.is_none() {
            log::warn!("{path}: not found");
        }
        b.map(Arc::new)
    }

    /// `AtlCarte_ReloadSprBank` (@ 0x00405bf0): `Carte.spr` opened again;
    /// the WAM loader calls it for `ldmorts.wam` (@ 0x0043eb00 →
    /// @ 0x004c34e0, @ 0x004c372b). The map keeps the last one loaded.
    pub(crate) fn carte_reload_banks(&mut self) {
        self.puzzles.screens.carte_bank = self.screen_bank("Carte.spr");
    }

    /// `AtlCarteTour_ReloadSprBank` (@ 0x00406a10), run by the start when
    /// either bank is empty (@ 0x00406625): `CarteTour<W>.spr` (the
    /// screen's width, @ 0x007147bc) and `PionsTour.spr`.
    pub(super) fn tour_banks(&mut self) {
        let st = &self.puzzles.screens;
        if st.tour_board.is_some() && st.tour_pawns.is_some() {
            return;
        }
        let board = self.screen_bank(&format!("CarteTour{}.spr", self.width));
        let pawns = self.screen_bank("PionsTour.spr");
        self.puzzles.screens.tour_board = board;
        self.puzzles.screens.tour_pawns = pawns;
    }

    /// The map's frame drawing (@ 0x004057fd..0x0040593b): the warp, the
    /// map, its places, the hovered place's label, the pointer
    /// `Interface.spr` #40 (the cursor `AtlCarte_Start` set, @ 0x00427f80).
    pub(super) fn carte_draw(&mut self) {
        self.draw_warp_only();
        let (w, h) = (self.width, self.height);
        let ptr = self.pointer;
        if let Some(carte) = self.puzzles.screens.carte_bank.clone() {
            maps::draw_carte(&mut self.screen, w, h, &carte, None, ptr);
        }
        self.draw_pointer();
    }

    /// The map's place under the pointer for a left click (1..5,
    /// @ 0x0040593e).
    pub(super) fn carte_hit(&self) -> Option<i32> {
        let carte = self.puzzles.screens.carte_bank.as_ref()?;
        maps::carte_click(carte, self.width, self.height, self.pointer).map(|k| k as i32)
    }

    /// The tower board's frame drawing (@ 0x004067ab..0x004069b5): the
    /// scale under the control word the last frame left, the warp, the
    /// board, and in `tourmagi.wam` the pawns' step and the pawns:
    /// A = the square @ 0x00789ad8, B = @ 0x00789ad4. No pointer (the
    /// start reset the cursor, @ 0x004282e0).
    pub(super) fn tour_draw(&mut self) {
        let fpu = self.camera.fpu;
        self.draw_warp_only();
        let Some(board) = self.puzzles.screens.tour_board.clone() else {
            return;
        };
        let pawns = self.puzzles.screens.tour_pawns.clone();
        let t = &self.puzzles.tourmagi;
        let pos = (t.demon, t.player, t.here);
        let (w, h, now) = (self.width, self.height, self.now);
        maps::draw_tour(
            &mut self.screen,
            w,
            h,
            &board,
            pawns.as_deref(),
            &mut self.puzzles.screens.tour_anim,
            fpu,
            now,
            pos,
        );
    }

    /// `AtlSpr_Aff` (@ 0x004367b0) of a bank the C code holds (`path` its
    /// file, `spr\Name.spr`): x = `None` for the sprite's own position.
    pub(crate) fn draw_spr(
        &mut self,
        path: &str,
        bank: &Bank,
        index: usize,
        x: Option<i32>,
        y: i32,
        alpha: f32,
    ) {
        let (w, h) = (self.width, self.height);
        let a = aff_at(&mut self.screen, w, h, "", bank, index, x, y, alpha);
        self.tr(|| {
            let (xs, ys) = match x {
                Some(x) => (x.to_string(), a.y.to_string()),
                None => ("def".to_string(), "def".to_string()),
            };
            let hex: String = alpha
                .to_bits()
                .to_le_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!("aff {path}#{index}@{xs},{ys}*{hex}")
        });
    }

    /// `AtlSpr_AffCentre` (@ 0x00436b80): `AtlSpr_Aff` at (x, y) minus
    /// half the sprite's size; nothing at alpha 0 or below.
    pub(crate) fn draw_spr_centre(
        &mut self,
        path: &str,
        bank: &Bank,
        index: usize,
        x: i32,
        y: i32,
        alpha: f32,
    ) {
        self.tr(|| format!("affc {path}#{index}@{x},{y}"));
        if alpha <= 0.0 {
            return;
        }
        if let Some(info) = bank.info(index) {
            let (dx, dy) = (i32::from(info.w) / 2, i32::from(info.h) / 2);
            self.draw_spr(path, bank, index, Some(x - dx), y - dy, alpha);
        }
    }

    /// The unicorn board's picture (@ 0x0042f983..0x0042fa2d): the warp
    /// frame (@ 0x00440690, covered by the board), the board
    /// (@ 0x004bb3e0), the cursor when nothing runs and nothing is
    /// carried (`Interface.spr` #40 at the pointer, else the animated
    /// frame centred on it), the flip.
    pub(crate) fn licorne_draw(&mut self, board: &mut crate::licorne::Board) {
        self.draw_warp_only();
        board.draw(&mut self.screen);
        if let Some(c) = board.cursor() {
            if let Some(i) = self.interface.take() {
                let path = "spr\\Interface.spr";
                match c {
                    crate::licorne::Cursor::Plain(x, y) => {
                        self.draw_spr(path, &i, 40, Some(x), y, 1.0);
                    }
                    crate::licorne::Cursor::Centred(k, x, y) => {
                        self.draw_spr_centre(path, &i, k.max(0) as usize, x, y, 1.0);
                    }
                }
                self.interface = Some(i);
            }
        }
        self.tr_flip();
    }

    /// `AtlEcrin_ReloadSprBank` (@ 0x00413370), run by the `jardin.wam`
    /// load (@ 0x004c367e): `Ecrin%d.spr`, %d the display's depth (the
    /// port draws in 16 bits). `Interface.spr` is the game's.
    pub(crate) fn ecrin_reload_banks(&mut self) {
        self.puzzles.screens.ecrin_bank = self.screen_bank("Ecrin16.spr");
    }

    /// `ECRIN`'s drawing (@ 0x00412fc0): the warp, the case (#0) and each
    /// untaken gem (#1..#3) at the screen's centre with the fade's alpha,
    /// then with the fade at 1 only the cursor: #40 at the pointer, the
    /// glow centred on it; the flip.
    pub(crate) fn ecrin_draw(&mut self) {
        self.draw_warp_only();
        let (cx, cy) = (self.width as i32 / 2, self.height as i32 / 2);
        let st = &self.puzzles.screens;
        let (fade, taken, cursor) = (st.ecrin_fade, st.ecrin_taken, st.ecrin_cursor);
        if let Some(bank) = st.ecrin_bank.clone() {
            let path = "spr\\Ecrin16.spr";
            self.draw_spr(path, &bank, 0, Some(cx), cy, fade);
            for (k, t) in taken.iter().enumerate() {
                if !t {
                    self.draw_spr(path, &bank, k + 1, Some(cx), cy, fade);
                }
            }
        }
        if fade == 1.0 {
            if let Some(i) = self.interface.take() {
                let path = "spr\\Interface.spr";
                let (px, py) = self.pointer;
                if cursor == 40 {
                    self.draw_spr(path, &i, 40, Some(px), py, 1.0);
                } else {
                    self.draw_spr_centre(path, &i, cursor.max(0) as usize, px, py, 1.0);
                }
                self.interface = Some(i);
            }
        }
        self.tr_flip();
    }
}
