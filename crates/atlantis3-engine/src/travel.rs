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
use atlantis3_data::spr::{Bank, Target};

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
/// `spf\SPF640\<world>\`. A name with no WAM entity gets no bank.
pub fn transition_paths(
    wam: &Wam,
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
            Some(format!(
                "spf/SPF640/{folder}/{}{old:03}{next:03}.spr",
                e.prefix
            ))
        })
        .collect()
}

/// The transition sprites over video frame `n` (1 = the first): every
/// bank's sprite `n − 1` (@ 0x00443620: the framer's frame counter − 1)
/// at its default position (`AtlSpr_Aff(bank, n − 1, 0x80000000, 0,
/// 1.0)`), in the banks' order, into the `w` × `h` video frame. A bank
/// shorter than the video draws nothing past its last sprite.
pub fn draw_transition_sprites(frame: &mut [u16], w: usize, h: usize, banks: &[Bank], n: u32) {
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
