//! Xibalba (`MONDE` 6, disc 4, chapters 35-41): `bat.wam` (the bat's
//! island, chapter 35), `riviere.wam` (the river, 36), `xibalba.wam`
//! (Chac's pond, 37; the statues' clearing, 39) and `toile.wam` (the
//! spider's web, 38), reached from the priestess of chapter 27 and left
//! for the king's hall by chapter 35. Their views share Yucatán's `YU`
//! prefix, so their puzzle hooks are the `YU` branches of `yucatan.rs`.
//!
//! Its three puzzles run:
//!
//! - the star map (view 121 of `xibalba.wam`): `stars.rs`;
//! - the canoe (view 402 of `bat.wam`): FUN_0049c010(1), the 3D puzzle's
//!   own loop (type 9), started by a click there once `enable_pz_pirog`
//!   is 1 (before, the click is consumed); `canoe.rs` and
//!   [`crate::canoe`];
//! - the spider's web (`toile.wam`, views 198-265): `web.rs`: the entry
//!   of view 198 FUN_0048f530 (the puzzle mode 1), in that mode the entry
//!   FUN_0048f9a0, the move click FUN_0048fdd0, the zone click
//!   FUN_00491290, the tick FUN_00491610 and the draw FUN_00491b30, and a
//!   save made in the web loads in view 198 (FUN_0048f450 from
//!   FUN_004785a0, [`super::Engine::load_data`]).
//!
//! No testing aid is left: every win is played.
