//! *Atlantis Evolution*'s 2004 cursor (`ATLANTIS4_CD` for the drawn one):
//! FUN_00444cb0 → FUN_00429260 → FUN_00429290 draws its own
//! `spr\Interface.spr` (31 sprites: 1..8 move, 10..21 action, 23..27 talk,
//! 29 pointer), not *Atlantis III*'s frame numbers.

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::A3Files;
use atlantis3_engine::game::{cursor_range, cursor_range_2004, pointer_frame, Game};

#[test]
fn the_ranges() {
    let cur = (5, 6);
    // Mode 0: a zone.
    assert_eq!(cursor_range_2004(1, 0, cur), (1, 9));
    assert_eq!(cursor_range_2004(2, 0, cur), (1, 9));
    assert_eq!(cursor_range_2004(3, 0, cur), (10, 22));
    assert_eq!(cursor_range_2004(1000, 0, cur), (10, 22));
    assert_eq!(cursor_range_2004(0, 0, cur), cur);
    // Mode 1: an object sprite, whatever its click kind.
    for k in [1, 2, 3, 4] {
        assert_eq!(cursor_range_2004(k, 1, cur), (10, 22));
    }
    // Mode 2: a character sprite.
    assert_eq!(cursor_range_2004(2, 2, cur), (23, 28));
    assert_eq!(cursor_range_2004(4, 2, cur), (10, 22));
    assert_eq!(cursor_range_2004(6, 2, cur), (10, 22));
    assert_eq!(cursor_range_2004(3, 2, cur), cur);
    // Atlantis III's are unchanged.
    assert_eq!(cursor_range(1, 0, cur), (1, 13));
    assert_eq!(cursor_range(3, 0, cur), (25, 40));
    assert_eq!(pointer_frame(false), 40);
    assert_eq!(pointer_frame(true), 29);
}

/// The sprites of `bank` drawn centred on the screen's centre whose every
/// opaque pixel (the same drawn over two backgrounds) is the screen's.
fn drawn(bank: &Bank, screen: &[u16], w: usize, h: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for i in 1..bank.len() {
        let Some(info) = bank.info(i) else { continue };
        let x = (w / 2) as i32 - info.w as i32 / 2;
        let y = (h / 2) as i32 - info.h as i32 / 2;
        let [a, b] = [0x0821u16, 0xf7de].map(|key| {
            let mut buf = vec![key; w * h];
            let mut t = Target::screen(&mut buf, w, h);
            bank.draw(&mut t, Some(x), y, i);
            buf
        });
        let mut opaque = 0;
        let same = a.iter().zip(&b).zip(screen).all(|((&p, &q), &s)| {
            if p != q || p == 0x0821 {
                return true;
            }
            opaque += 1;
            p == s
        });
        if same && opaque > 0 {
            out.push(i);
        }
    }
    out
}

#[test]
fn the_cabins_door_shows_the_action_cursor() {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    let files = A3Files::evolution_2004(src);
    let bank = files
        .read("spr/Interface.spr")
        .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok())
        .expect("the 2004 interface bank");
    // The CD's new `interface.SPR` (60,605 bytes), not `interface_.SPR`.
    assert_eq!(bank.len(), 31);
    let mut g = Game::new(files).expect("the 2004 program's game");
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    let mut t = 0.01;
    // View 1's zones: a move (type 1), then the corridor (type 3).
    for (ty, frames) in [(1, 1..9), (3, 10..22)] {
        assert!(g.aim_at_zone(|z| z.kind == ty));
        t += 0.001;
        g.step(t);
        let z = g.centre_zone().expect("a zone under the centre");
        assert_eq!(g.wam().view(1).unwrap().zones[z].kind, ty);
        let (w, h) = (g.width(), g.height());
        let found = drawn(&bank, g.screen(), w, h);
        assert!(
            found.iter().any(|f| frames.contains(f)),
            "type {ty}: {found:?}"
        );
        // Nothing of the other cursors (frame 22, past the range, is
        // frame 10's picture).
        let same_picture = |f: usize| frames.contains(&f) || (frames.start == 10 && f == 22);
        assert!(
            found.iter().all(|&f| same_picture(f)),
            "type {ty}: {found:?}"
        );
    }
}
