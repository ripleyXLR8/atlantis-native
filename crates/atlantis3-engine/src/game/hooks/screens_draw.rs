//! The map screens' banks and drawing in the game ([`crate::maps`] has the
//! rules): `AtlCarte` (mode 9, @ 0x00405700) and `AtlCarteTour` (mode 10,
//! @ 0x00406780), each drawn over the running warp frame (@ 0x00440690).

use std::sync::Arc;

use atlantis3_data::spr::{Bank, Depth};

use crate::maps;

use super::Game;

impl Game {
    /// A screen bank read from `spr\`.
    fn screen_bank(&self, name: &str) -> Option<Arc<Bank>> {
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
}
