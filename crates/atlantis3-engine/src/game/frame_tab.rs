//! The place sprites' frame lists as the C code and the scripts edit them
//! (`AddSpwToFrameTab` @ 0x00438a40, `SubSpwToFrameTab` @ 0x00438ae0, the
//! test @ 0x00438ba0; the lists themselves and their semantics are in
//! [`crate::place_anim`]), and the warp cursor the puzzles set
//! (`SetCurseurWarp` @ 0x00428720, its reset @ 0x004287f0).
//!
//! A slot whose mode is not 1 draws its list (@ 0x00439130): a C puzzle
//! keeps such a slot (a lid, a star, a tile, a feather) in mode 0 and puts
//! the frames to show in its list. Each call is logged in
//! [`Game::trace`] as the board harnesses log it (`addf NAME f`,
//! `subf NAME f`, the object's `#OBJETS#` name), whatever its result;
//! the test is not logged.

use atlantis2_data::defnames;

use super::hooks::Hx;
use super::Game;

impl Game {
    /// The `#OBJETS#` name of an object id, for the log.
    fn object_name(&self, id: i32) -> String {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.names.name(defnames::OBJETS, i))
            .map_or_else(
                || format!("2#{id}"),
                |n| String::from_utf8_lossy(n).into_owned(),
            )
    }

    /// `AddSpwToFrameTab(id, frame)` (@ 0x00438a40; `ADDFRAMEPERSO` /
    /// `ADDFRAMEOBJET`).
    pub(crate) fn add_frame(&mut self, id: i32, frame: i32) -> bool {
        if self.trace.is_some() {
            let n = self.object_name(id);
            self.tr(|| format!("addf {n} {frame}"));
        }
        self.slots.add_frame(id, frame)
    }

    /// `SubSpwToFrameTab(id, frame)` (@ 0x00438ae0; `SUPFRAMEPERSO` /
    /// `SUPFRAMEOBJET`).
    pub(crate) fn sub_frame(&mut self, id: i32, frame: i32) -> bool {
        if self.trace.is_some() {
            let n = self.object_name(id);
            self.tr(|| format!("subf {n} {frame}"));
        }
        self.slots.sub_frame(id, frame)
    }

    /// `SetCurseurWarp(bank, index)` (@ 0x00428720): the cursor drawn at
    /// the screen's centre is sprite `index` of the bank, here always the
    /// world's in-hand bank the puzzles pass (@ 0x007565f0 →
    /// @ 0x00756518, the index @ 0x00716e08); nothing when the bank has no
    /// such sprite (@ 0x00436e40). Logged `cursor set <index>`.
    pub(crate) fn set_warp_cursor(&mut self, index: i32) {
        self.tr(|| format!("cursor set {index}"));
        self.world_banks();
        let exists = usize::try_from(index).ok().is_some_and(|i| {
            self.objects_hand
                .as_ref()
                .is_some_and(|b| b.info(i).is_some())
        });
        if exists {
            self.puzzles.warp_cursor = Some(index);
        } else {
            log::warn!("SetCurseurWarp: no sprite {index}");
        }
    }

    /// The warp cursor reset (@ 0x004287f0): the bank name emptied, the
    /// index −1. Logged `cursor reset`.
    pub(crate) fn reset_warp_cursor(&mut self) {
        self.tr(|| "cursor reset".into());
        self.puzzles.warp_cursor = None;
    }
}

impl Hx<'_> {
    /// @ 0x00438a40 on the object `id`.
    pub fn addf(&mut self, id: i32, frame: i32) -> bool {
        self.g.add_frame(id, frame)
    }

    /// @ 0x00438ae0 on the object `id`.
    pub fn subf(&mut self, id: i32, frame: i32) -> bool {
        self.g.sub_frame(id, frame)
    }

    /// @ 0x00438ba0 on the object `id`.
    pub fn has_frame(&self, id: i32, frame: i32) -> bool {
        self.g.slots.has_frame(id, frame)
    }

    /// `subf` when the frame is listed (the C code's usual pair
    /// @ 0x00438ba0 + @ 0x00438ae0).
    pub fn subf_if_listed(&mut self, id: i32, frame: i32) {
        if self.has_frame(id, frame) {
            self.subf(id, frame);
        }
    }

    /// `addf` unless the frame is listed.
    pub fn addf_if_absent(&mut self, id: i32, frame: i32) {
        if !self.has_frame(id, frame) {
            self.addf(id, frame);
        }
    }
}
