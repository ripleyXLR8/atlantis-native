//! *Atlantis V*'s first view drawn headlessly from the French DVD
//! (`ATLANTIS5_DVD_FR`; skipped without it): the new game's view 3 of the
//! zeppelin with the five sprites its entry places, under the camera of
//! `Atl_Camera_LookAt(0, 270)` and `Atl_Camera_SetFOV(76)`.
//!
//! The scripts do not run in Native yet: the view is entered from the calls
//! the game's scripts make ([`FIRST_VIEW`], recorded by running them under
//! Lua 5.0.3), not by the scripts themselves.
//!
//! With `ATLANTIS5_SHOT_DIR` set, the picture is written there as
//! `first_view.png` (and the faces as `first_view_face<k>.png`).

use std::path::PathBuf;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{View, FACE, FIRST_VIEW, VIEW_HEIGHT, VIEW_WIDTH};

fn source(sub: &str) -> Option<Atlantis5Source> {
    let d = atlantis3_data::testing::a5_dvd_fr()?;
    Some(Atlantis5Source::open(&d.join(sub)).expect("the French DVD's data"))
}

fn shot(name: &str, px: &[u32], w: usize, h: usize) {
    if let Some(dir) = std::env::var_os("ATLANTIS5_SHOT_DIR").map(PathBuf::from) {
        std::fs::create_dir_all(&dir).unwrap();
        let png = atlantis2_engine::png::encode_xrgb8888(px, w, h);
        std::fs::write(dir.join(name), png).unwrap();
    }
}

#[test]
fn the_first_view() {
    let Some(src) = source("datas") else { return };
    let v = View::load(&src, &FIRST_VIEW).expect("the first view");
    assert_eq!(v.panorama, "ze0003.jp6");
    assert_eq!(v.sprites.len(), 5, "{:?}", v.sprites);
    // The sprites changed the faces they lie on, and only within their
    // rectangles: compare with the bare panorama.
    let bare = View::load(
        &src,
        &atlantis3_engine::atlantis5::ViewCalls {
            sprites: &[],
            ..FIRST_VIEW
        },
    )
    .unwrap();
    let changed: usize = (0..6)
        .map(|f| {
            v.faces[f]
                .iter()
                .zip(&bare.faces[f])
                .filter(|(a, b)| a != b)
                .count()
        })
        .sum();
    assert!(changed > 1000, "{changed} texels changed by the sprites");
    // Each sprite's single face where its header puts it (hot spot at
    // (0, 0), the header's offsets negative): the coin's 38 x 16 at
    // (67, 540) of face 0, the ashtray's 30 x 41 at (606, 424) of face 4.
    for (face, x0, y0, w, h) in [(0, 67, 540, 38, 16), (4, 606, 424, 30, 41)] {
        let inside = (y0..y0 + h)
            .flat_map(|y| (x0..x0 + w).map(move |x| y * FACE + x))
            .filter(|&i| v.faces[face][i] != bare.faces[face][i])
            .count();
        assert!(inside > w * h / 4, "face {face}: {inside}");
    }
    let pic = v.render(true);
    assert_eq!(pic.len(), VIEW_WIDTH * VIEW_HEIGHT);
    // A panorama everywhere: no pixel left at the buffer's 0 in rows of
    // the picture's middle, and more than a handful of colours.
    let mut colours: Vec<u32> = pic.iter().step_by(97).copied().collect();
    colours.sort_unstable();
    colours.dedup();
    assert!(colours.len() > 500, "{} colours", colours.len());
    shot("first_view.png", &pic, VIEW_WIDTH, VIEW_HEIGHT);
    for (k, f) in v.faces.iter().enumerate() {
        shot(&format!("first_view_face{k}.png"), f, FACE, FACE);
    }
    // The same view from the disc's cabinets read in place gives the same
    // picture.
    let Some(disc) = source("files") else { return };
    let w = View::load(&disc, &FIRST_VIEW).unwrap();
    assert!(w.faces == v.faces);
}
