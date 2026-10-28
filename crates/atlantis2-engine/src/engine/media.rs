//! The media pages of the menus (`AtlMenu_start_PleaseWait`,
//! `AtlMenu_start_CD`, `AtlMenu_start_DiskFull`, `AtlMenu_start_DiskError`)
//! and the music copy that shows the first one.
//!
//! The original plays its music from hard-disk copies: the installer leaves
//! every `music\Muzik*.apc` / `.spp` empty, and `LOADMUSIK` (or a music
//! started whose copy is empty, FUN_0047a200) copies them from the CD in the
//! drive (`CopyMusikToDrive` @ 0x00479480) under the please-wait page. The
//! port reads the music from the discs, so it copies nothing, but it keeps
//! which copies the original would hold and shows the page where the
//! original copies: for the frames the original draws, two for each
//! 0x40000-byte block of the `.apc` (FUN_00486eb0 and a flip after the
//! block's read and after its write, @ 0x004797c4..0x00479807), as with a
//! disc read at once. The time a real drive takes over each block is not
//! modelled. The copies the original would hold are kept beside the saves
//! (`music_copies.txt`, one `muzikNN` a line), as the original's last
//! from one session to the next; the first session starts from the install
//! folder's (empty). Without a data folder (tests, tools) every copy is
//! taken as made until a `delmusik`.
//!
//! Nothing else runs during the copy: no 15 Hz tick, no frame of the
//! game, and the mouse's and the keys' events wait in DirectInput's buffer
//! until it ends ([`Input`]).
//!
//! The disk-full and disk-error pages (FUN_00486c60, FUN_00486d10) open
//! only from that copy: when the free space (FUN_0040de60) is below a
//! file's size, or when a block's read or write, or the `.spp`'s, is
//! short. The port writes no copy, so neither can happen.
//!
//! The CD page (FUN_004869f0) opens when a world's disc is not in any
//! drive (FUN_0040dff0). The port reads every disc it found at start-up as
//! if each were in a drive of its own, so the page opens only when the
//! world's disc was not found; it polls every 100 ticks (state 9) and goes
//! on when the disc has been added to a folder the discs are looked for in.

use std::collections::BTreeSet;

use atlantis2_data::Folder;

use super::Engine;
use crate::music;

/// The bytes `CopyMusikToDrive` copies between two page frames.
const COPY_BLOCK: u64 = 0x40000;

/// The file beside the saves that keeps the copies.
const COPIES_FILE: &str = "music_copies.txt";

/// The media pages' state.
#[derive(Debug, Default)]
pub struct Media {
    /// The musics whose hard-disk copy (`music\muzikNN.apc`) is not empty
    /// in the original: copied by `LOADMUSIK` or a music start, deleted by
    /// `delmusik`.
    pub copies: BTreeSet<String>,
    /// The please-wait page's frames still to show; the game is held
    /// meanwhile.
    pub copy_frames: u32,
    /// The please-wait page is built.
    copy_page: bool,
    /// A disc asked for and not found: the CD page opens at the next pass.
    cd_request: Option<u8>,
    /// The input that came during the copy, given to the game after it.
    held_input: Vec<Input>,
}

/// An input event held during a music copy: the copy's loop never returns
/// to the application's, where the input is read (FUN_0058e9f0, given to
/// the application's loop @ 0x0058f78a), and DirectInput keeps the mouse's
/// and the keys' events in its buffer (`GetDeviceData`, vtable +0x28, @
/// 0x0058ea35) until then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Click(f64),
    Release,
    Right,
    Motion(i32, i32),
    PointerMoved(i32, i32),
    Pointer(i32, i32),
    Escape,
}

impl Media {
    /// Every copy made (`muzik00` .. `muzik99`): the state of an
    /// installation whose copies were all made, the engine's without a
    /// data folder (tests and tools), where nothing could keep them.
    pub fn all_made() -> Media {
        Media {
            copies: (0..100)
                .map(|n| music::music_name(n).trim_end_matches(".apc").to_string())
                .collect(),
            ..Media::default()
        }
    }

    /// The copies the install folder holds: its non-empty
    /// `music\muzikNN.apc` (the installer's are all empty).
    pub fn from_install(files: &atlantis2_data::A2Files) -> Media {
        let copies = (0..100)
            .map(music::music_name)
            .filter(|name| {
                files
                    .install()
                    .size(&Folder::Music.path(name))
                    .is_some_and(|s| s > 0)
            })
            .map(|name| name.trim_end_matches(".apc").to_string())
            .collect();
        Media {
            copies,
            ..Media::default()
        }
    }
}

/// The names `CopyMusikToDrive` builds from its list (@ 0x004794fa..
/// 0x00479578): each item up to `,`, `)`, LF or the end, `muzik`, a `0`
/// when the item has one character, and the item (then `.apc` and
/// `.spp`). A non-digit is logged and kept.
pub(super) fn copy_names(list: &str) -> Vec<String> {
    let b = list.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < b.len() && !matches!(b[p], 0 | b'\n' | b')') {
        if b[p] == b',' {
            p += 1;
        }
        let start = p;
        while p < b.len() && !matches!(b[p], 0 | b'\n' | b',' | b')') {
            if !b[p].is_ascii_digit() {
                log::warn!("Erreur CopyMusikToDrive(char*): {list}");
            }
            p += 1;
        }
        let item = &list[start..p];
        let zero = if item.len() == 1 { "0" } else { "" };
        out.push(format!("muzik{zero}{item}").to_ascii_lowercase());
    }
    out
}

impl Engine {
    /// `CopyMusikToDrive` (@ 0x00479480): for each music of the list whose
    /// copy is empty, the copy from the current disc, two please-wait
    /// frames per block of its `.apc` (its `.spp` is copied whole, with
    /// no frame). A music not on the current disc is not copied (the
    /// original logs it and deletes the empty copy). True when the page
    /// has frames to show.
    pub(super) fn copy_music(&mut self, list: &str) -> bool {
        let disc = self.files.current_disc();
        let mut frames = 0u64;
        for name in copy_names(list) {
            if self.media.copies.contains(&name) {
                continue;
            }
            let apc = format!("{name}.apc");
            let Some(size) = self.files.size_on_disc(disc, Folder::ApcMusic, &apc) else {
                log::warn!(
                    "Erreur CopyMusikToDrive(char*): impossible d'ouvrir le fichier {apc} (CD {disc})"
                );
                continue;
            };
            frames += 2 * size.div_ceil(COPY_BLOCK);
            self.media.copies.insert(name);
        }
        self.save_copies();
        if frames == 0 {
            return false;
        }
        log::info!("LOADMUSIK {list}: {frames} please-wait frames");
        self.media.copy_frames += frames as u32;
        true
    }

    /// `DeleteMusikOnDrive` (`delmusik` @ 0x00479d70): the copies of the
    /// list's musics deleted.
    pub(super) fn delete_copies(&mut self, list: &str) {
        for name in copy_names(list) {
            self.media.copies.remove(&name);
        }
        self.save_copies();
    }

    /// Whether the please-wait page holds the game.
    pub fn copying(&self) -> bool {
        self.media.copy_frames > 0
    }

    /// One frame of the copy: the menu frame of the please-wait page with
    /// its tick flag raised (@ 0x004797c4), which runs the music state
    /// machine first (@ 0x00486ecf); nothing else runs. After the last
    /// one the page is deleted (`AtlMenu_stop` @ 0x00479b79, the loop type
    /// unchanged) and the section that waited goes on.
    pub(super) fn copy_pass(&mut self, out: Option<&mut [u16]>) {
        if !self.media.copy_page {
            self.media.copy_page = true;
            self.with_menu(|m, e| m.start_please_wait(e));
        }
        // The menu frame runs the music state machine first
        // ([`Engine::menu_pass`]).
        if let Some(m) = self.menu.as_mut() {
            m.tick_flag = true;
        }
        self.menu_pass(out);
        self.media.copy_frames -= 1;
        if self.media.copy_frames == 0 {
            self.media.copy_page = false;
            self.with_menu(|m, e| m.stop(e));
            self.with_script(|s, h| s.copy_ended(h));
            for i in std::mem::take(&mut self.media.held_input) {
                match i {
                    Input::Click(t) => self.click(t),
                    Input::Release => self.release(),
                    Input::Right => self.right_click(),
                    Input::Motion(dx, dy) => self.mouse_motion(dx, dy),
                    Input::PointerMoved(dx, dy) => self.pointer_moved(dx, dy),
                    Input::Pointer(x, y) => self.pointer(x, y),
                    // A copy started by one of them holds the rest again.
                    Input::Escape => self.escape(),
                }
            }
        }
    }

    /// During a music copy the input waits for its end: true when
    /// `input` was held.
    pub(super) fn hold_input(&mut self, input: Input) -> bool {
        if !self.copying() {
            return false;
        }
        self.media.held_input.push(input);
        true
    }

    /// FUN_0040dff0 for disc `disc`: whether a drive holds it; when none
    /// does, the CD page opens (at the next pass) and polls for it.
    pub(super) fn ask_disc(&mut self, disc: u8) -> bool {
        if self.files.disc_numbers().contains(&disc) {
            return true;
        }
        log::info!("disc {disc} not found: the CD page asks for it");
        self.media.cd_request = Some(disc);
        false
    }

    /// The CD page asked for by [`Engine::ask_disc`], opened at the start
    /// of a pass (`AtlMenu_start_CD` @ 0x004869f0: loop type 5).
    pub(super) fn open_cd_page(&mut self) {
        if let Some(n) = self.media.cd_request.take() {
            self.with_menu(|m, e| m.start_cd(e, n));
        }
    }

    /// The copies kept beside the saves; without that file the install
    /// folder's (the installer's are all empty). Without a data folder
    /// they stay as they are.
    pub(super) fn load_copies(&mut self) {
        let Some(store) = &self.store else { return };
        let Some(text) = store.read(COPIES_FILE) else {
            self.media.copies = Media::from_install(&self.files).copies;
            return;
        };
        let text = String::from_utf8_lossy(&text);
        self.media.copies = text
            .lines()
            .map(|l| l.trim().to_ascii_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
    }

    fn save_copies(&self) {
        let Some(store) = &self.store else { return };
        let text: String = self.media.copies.iter().map(|n| format!("{n}\n")).collect();
        // (a failure is logged by the store)
        store.write(COPIES_FILE, text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::copy_names;

    #[test]
    fn the_names_of_a_list() {
        let n = |l: &str| copy_names(l);
        assert_eq!(n("1,2,3)"), ["muzik01", "muzik02", "muzik03"]);
        assert_eq!(n("30)"), ["muzik30"]);
        assert_eq!(n("5"), ["muzik05"]);
        assert_eq!(n("05)"), ["muzik05"]);
        assert_eq!(n("100)"), ["muzik100"]);
        assert!(n(")").is_empty());
        assert_eq!(n("4a)"), ["muzik4a"]);
    }
}
