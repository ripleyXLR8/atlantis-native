//! The jewel case's bank by display depth (`AtlEcrin_ReloadSprBank`
//! @ 0x00413370: `Ecrin%d.spr`, %d the display's bit count from 0x0040efc0).
//! `Ecrin16.spr` has blitters only at 16 bits and `Ecrin32.spr` only at 32,
//! so the case must come from the bank of the display's depth to be drawn
//! whole: once the fade reaches 1, the case covers the screen's centre that
//! the fade-0 frame (the warp alone) left bare, and the 32-bit frame shows
//! the same picture as the 16-bit one (to the 16-bit precision).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::spr::Depth;
use atlantis3_data::A3Files;
use atlantis3_engine::px::Surface;
use atlantis3_engine::Game;

/// The screen's centre quarter as (r, g, b) at 5 / 6 / 5 bits.
fn centre(s: &Surface, w: usize, h: usize) -> Vec<(i32, i32, i32)> {
    let px: Vec<(i32, i32, i32)> = match s {
        Surface::Rgb565(p) => p
            .iter()
            .map(|&c| {
                let c = i32::from(c);
                (c >> 11, (c >> 5) & 0x3f, c & 0x1f)
            })
            .collect(),
        Surface::Xrgb8888(p) => p
            .iter()
            .map(|&c| {
                let c = c as i32;
                ((c >> 19) & 0x1f, (c >> 10) & 0x3f, (c >> 3) & 0x1f)
            })
            .collect(),
    };
    (h * 3 / 8..h * 5 / 8)
        .flat_map(|y| (w * 3 / 8..w * 5 / 8).map(move |x| (x, y)))
        .map(|(x, y)| px[y * w + x])
        .collect()
}

#[test]
fn the_jewel_case_is_drawn_at_both_depths() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut at16 = Vec::new();
    for depth in [Depth::Rgb565, Depth::Xrgb8888] {
        let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
        g.start_at_depth(depth);
        g.script_mut().unwrap().flags[829] = 1;
        assert!(g.board_begin("jardin.wam", 3));
        g.board_enter(114, 100.0);
        g.script_mut().unwrap().flags[144] = 1;
        assert!(g.board_zone_hook("BAG_ZONE_ECRIN", 200.0));
        let w = g.width();
        let h = g.surface().len() / w;
        // The pointer in a corner, away from the centre.
        let ptr = (2, 2);
        assert!(g.board_screen_frame(200.0, ptr, false, false));
        let bare = centre(g.surface(), w, h);
        let mut t = 200.0;
        while !g.board_st("ecrin").unwrap().contains("fade=0000803f") {
            t += 1.0 / 32.0;
            assert!(t < 210.0, "{depth:?}: the fade never reached 1");
            assert!(g.board_screen_frame(t, ptr, false, false));
        }
        t += 1.0 / 32.0;
        assert!(g.board_screen_frame(t, ptr, false, false));
        let shown = centre(g.surface(), w, h);
        let changed = bare.iter().zip(&shown).filter(|(a, b)| a != b).count();
        assert!(
            changed * 2 > bare.len(),
            "{depth:?}: the case covers {changed} of {} centre pixels",
            bare.len()
        );
        if depth == Depth::Rgb565 {
            at16 = shown;
        } else {
            let near = |a: i32, b: i32| (a - b).abs() <= 1;
            let same = at16
                .iter()
                .zip(&shown)
                .filter(|(a, b)| near(a.0, b.0) && near(a.1, b.1) && near(a.2, b.2))
                .count();
            assert!(
                same * 10 > shown.len() * 9,
                "the 32-bit case matches the 16-bit one on {same} of {} pixels",
                shown.len()
            );
        }
    }
}
