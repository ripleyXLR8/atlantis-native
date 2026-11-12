//! The travel videos' transition sprites: the banks a move opens
//! (`Game.exe` @ 0x00435a90) and their sprites drawn over each frame of
//! the travel video (@ 0x00435d60, from `AtlHnm_continue` @ 0x00421fe8).
//!
//! `AtlHnm_start` with its fourth argument 1 (Hnm_Framer) opens a stretch
//! surface of the video's size (@ 0x00410090: 640 × 480 for every shipped
//! travel), keeps it in 0x00755908 and clears it (@ 0x00410050). Each
//! frame is drawn into it (@ 0x00443640), then the sprites (@ 0x00435d60
//! sets the sprite target to that surface), then the flip stretches it to
//! the screen (@ 0x0040fdf0). So the sprites' default positions and their
//! clipping are in the video's space, whatever the screen's size; the
//! surface's clip maxima are its size, exclusive (@ 0x004362b0).

use atlantis2_data::wam::{Destination, Wam};
use atlantis3_data::spr::{Bank, Pixel, Target};

/// The destination of the move from view `old` to view `next` (the first
/// view `old` of the WAM with a destination to `next`, @ 0x00462a60).
pub fn destination(wam: &Wam, old: u32, next: u32) -> Option<&Destination> {
    wam.places
        .iter()
        .flat_map(|p| p.views.iter())
        .filter(|v| v.id == old)
        .find_map(|v| v.destinations.iter().find(|d| d.target == next))
}

/// The transition banks of the move `old` → `next` (@ 0x00435a90), in
/// the destination's `id_spf` order: for each entity name not `hidden`
/// (its show-table state 0, @ 0x0044b8b0), the WAM entity of that CON
/// name, its two-letter prefix and `%s%03d%03d.spr` (@ 0x00717de8) under
/// [`spf_folder`]`\<world>\`. A name with no WAM entity gets no bank.
pub fn transition_paths(
    wam: &Wam,
    warp_res: i32,
    folder: &str,
    old: u32,
    next: u32,
    hidden: impl Fn(&str) -> bool,
) -> Vec<String> {
    let Some(d) = destination(wam, old, next) else {
        return Vec::new();
    };
    d.sprites
        .iter()
        .filter(|name| !hidden(name.as_str()))
        .filter_map(|name| {
            let e = wam
                .entities
                .iter()
                .find(|e| e.con_name.eq_ignore_ascii_case(name))?;
            Some(transition_path(warp_res, folder, &e.prefix, old, next))
        })
        .collect()
}

/// The transition banks' folder (the builder @ 0x00417ac0, the seventh
/// that tests WarpRes; the static constructors' strings):
/// `spf\` (0x00755b68) + `SPF800\` (0x00755c70) when WarpRes
/// (0x00789d34, the panorama faces' size) is 1024, else `SPF640\`
/// (0x00755cd0). The screen size plays no part. The GOG install ships
/// only `spf640`, so with WarpRes 1024 no transition bank opens.
pub fn spf_folder(warp_res: i32) -> &'static str {
    if warp_res == 1024 {
        "spf/SPF800"
    } else {
        "spf/SPF640"
    }
}

/// The other folders WarpRes chooses, each path builder testing
/// 0x00789d34 = 1024 exactly (the static constructors' strings
/// @ 0x004155f0..0x00415730): the cube `warp\` + `Warp1024\` or
/// `Warp512\` (@ 0x00415a30, the view entry and its re-entry), the
/// place animations `spw\` + `SPW1024\` or `SPW512\` (@ 0x004164c0), and
/// `HNM800\` or `HNM640\` for the videos: the cinematics (`hnm\`
/// @ 0x00416880, `loc\hnm\` @ 0x00416c60), the dialogs' body videos
/// (`hnmdial\` @ 0x00417280) and the travel videos (`hnmtraj\`
/// @ 0x00417670). GOG.com ships only the 512 and 640 folders, so with
/// WarpRes 1024 the original finds none of these files: the cube's debug
/// colours, no place animation, no film, no body video, no travel video.
pub fn warp_folder(warp_res: i32) -> &'static str {
    if warp_res == 1024 {
        "Warp1024"
    } else {
        "Warp512"
    }
}

/// The place animations' folder under `spw\` ([`warp_folder`]).
pub fn spw_folder(warp_res: i32) -> &'static str {
    if warp_res == 1024 {
        "SPW1024"
    } else {
        "spw512"
    }
}

/// The videos' folder under `hnm\`, `loc\hnm\`, `hnmdial\` and
/// `hnmtraj\` ([`warp_folder`]).
pub fn hnm_folder(warp_res: i32) -> &'static str {
    if warp_res == 1024 {
        "HNM800"
    } else {
        "HNM640"
    }
}

/// The transition bank of entity prefix `prefix` for the move `old` →
/// `next` of world folder `folder` (`%s%03d%03d.spr`, @ 0x00717de8).
pub fn transition_path(warp_res: i32, folder: &str, prefix: &str, old: u32, next: u32) -> String {
    format!(
        "{}/{folder}/{prefix}{old:03}{next:03}.spr",
        spf_folder(warp_res)
    )
}

/// The transition sprites over video frame `n` (1 = the first): every
/// bank's sprite `n − 1` (@ 0x00443620: the framer's frame counter − 1)
/// at its default position (`AtlSpr_Aff(bank, n − 1, 0x80000000, 0,
/// 1.0)`), in the banks' order, into the `w` × `h` video frame. A bank
/// shorter than the video draws nothing past its last sprite.
pub fn draw_transition_sprites<P: Pixel>(
    frame: &mut [P],
    w: usize,
    h: usize,
    banks: &[Bank],
    n: u32,
) {
    let Some(index) = (n as usize).checked_sub(1) else {
        return;
    };
    for bank in banks {
        let mut t = Target::screen(&mut *frame, w, h);
        t.xmax = w as i32;
        t.ymax = h as i32;
        bank.draw(&mut t, None, 0, index);
    }
}
