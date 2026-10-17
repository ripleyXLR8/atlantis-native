//! Where the game's scripts plug into navigation, and how the engine reads
//! files.
//!
//! Wherever the original runs a script from the place loop
//! (`FUN_00423ad8`) or the warp loop (`FUN_0041bfa0`), the engine calls a
//! method of [`ScriptHooks`] with full access to the [`Engine`]. The game's
//! implementation is [`crate::scripts::Scripts`] (the CON scripts); the
//! default methods do nothing, which is [`NoScripts`], navigation alone.

use std::future::Future;
use std::pin::Pin;

use atlantis_data::BigArchive;

use crate::engine::Engine;

/// A boxed future borrowing the hook and the engine.
pub type HookFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A future that is ready at once.
pub fn ready<'a, T: 'a>(v: T) -> HookFuture<'a, T> {
    Box::pin(std::future::ready(v))
}

/// The outcome of a departure scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Departure {
    /// The move goes on (to [`Engine::next_place_id`], which a section's
    /// Vue= may have changed).
    Go,
    /// AbortDepart (`FUN_00423ad8` @ 0x00423e2b..0x00423e7e): no move.
    Abort,
    /// GameOver was set (0x00423e13): the pass ends as it is.
    GameOver,
}

/// The script side of navigation. Every method has a no-op default.
#[allow(unused_variables)]
pub trait ScriptHooks {
    /// A new game's WAM is loaded (`FUN_00427634` from `FUN_0042de5c`):
    /// load `SCENAR\CHAPI<n>.CON` and run its /INIT block, which may set the
    /// start place ([`Engine::next_place_id`]) and the view angles.
    /// `keep_state`: a new game from play, which keeps the rest of the
    /// state; otherwise the game's start (`FUN_00423760`).
    fn chapter_loaded<'a>(
        &'a mut self,
        e: &'a mut Engine,
        chapter: u32,
        keep_state: bool,
    ) -> HookFuture<'a, ()> {
        ready(())
    }

    /// The arrival /sel scan after a place change (`FUN_00423ad8`'s own
    /// scan, `FUN_00427548` first); `arrivee` is ArriveeVue.
    fn arrival_scan<'a>(
        &'a mut self,
        e: &'a mut Engine,
        vue: u16,
        arrivee: i32,
    ) -> HookFuture<'a, ()> {
        ready(())
    }

    /// Once per warp frame, after the view moved: CON clocks
    /// (@ 0x0041c086) and the timer sections (`FUN_0041bfa0` @ 0x0041c369).
    /// True when a section ran (the engine then re-arms the look, as the
    /// original does after a section).
    fn warp_frame<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, bool> {
        ready(false)
    }

    /// A click on a character or an object of the panorama
    /// (`FUN_004278a0`): ids 0x100..0x1ff are objects (click scan and
    /// pick-up), from 0x200 the conversation (`FUN_00425828`). `zone` is
    /// ClicZone, the zone under the pointer.
    fn npc_click<'a>(&'a mut self, e: &'a mut Engine, id: u32, zone: u32) -> HookFuture<'a, ()> {
        ready(())
    }

    /// A click with no character under the pointer, `zone` the zone (0 for
    /// none, ClicZone is set): with a zone or an object in hand, the /con
    /// click scan (`FUN_004278a0` @ 0x00427b8c..0x00427bc2), before any
    /// move.
    fn zone_click<'a>(&'a mut self, e: &'a mut Engine, zone: u32) -> HookFuture<'a, ()> {
        ready(())
    }

    /// Zone 999: a /sel scan with DepartVue = 999 and no move
    /// (`FUN_00423ad8` @ 0x00423c47..0x00423c9b).
    fn zone_999<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, ()> {
        ready(())
    }

    /// Zones 1000..1016 (`FUN_004240fc`): puzzle `n` when it is the armed
    /// one (StartPuzzle). True when the click was taken by the puzzle (the
    /// zone then counts as 0).
    fn puzzle_zone<'a>(&'a mut self, e: &'a mut Engine, n: u32) -> HookFuture<'a, bool> {
        ready(false)
    }

    /// The departure /sel scan before a move to `dest` (`FUN_00423ad8` @
    /// 0x00423dc5..0x00423e7e), DEPARTVUE = `dest`.
    fn departure_scan<'a>(&'a mut self, e: &'a mut Engine, dest: u16) -> HookFuture<'a, Departure> {
        ready(Departure::Go)
    }

    /// A right click in the warp view: the inventory bar (`FUN_0041bfa0` @
    /// 0x0041c6d2). TODO(phase 3).
    fn bar_requested<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, ()> {
        ready(())
    }

    /// Escape in the warp view: the in-game menu (`FUN_0041c838`, opened
    /// @ 0x0041c5f7..0x0041c618). TODO(phase 3).
    fn menu_requested<'a>(&'a mut self, e: &'a mut Engine) -> HookFuture<'a, ()> {
        ready(())
    }
}

/// Navigation alone: every hook does nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoScripts;

impl ScriptHooks for NoScripts {}

/// How archive members become readable. On desktop the archive reads the
/// files directly; the browser reads the player's files only
/// asynchronously, so the web front end fetches the members' byte ranges
/// before the engine reads them.
pub trait Loader {
    /// Makes `names` (archive member names) readable through `big`.
    fn prefetch<'a>(&'a self, big: &'a BigArchive, names: &'a [String]) -> HookFuture<'a, ()>;
}

/// Files that are always readable (desktop, tests).
#[derive(Clone, Copy, Debug, Default)]
pub struct DirectLoader;

impl Loader for DirectLoader {
    fn prefetch<'a>(&'a self, _big: &'a BigArchive, _names: &'a [String]) -> HookFuture<'a, ()> {
        ready(())
    }
}
