//! Atlantis Native's SDL2 front end for handhelds and PortMaster (the
//! program `atlantis-native-sdl`, built with `--features sdl`): the parts
//! that need no SDL, tested on every system.
//!
//! The gamepad-as-mouse controls and the on-screen keyboard are from
//! Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement.

pub mod buttons;
pub mod config;
pub mod motion;
pub mod osk;
