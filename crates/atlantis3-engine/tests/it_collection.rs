//! The Italian *Atlantis Collection* DVD (2014) installed
//! (`ATLANTIS3_IT_COLLECTION`): what its `Game.exe`, GOG.com's 2015 / 2018
//! file, does otherwise than GOG.com's 2022 file, and the Italian texts.
//!
//! - its start-up reads `Datas\Atlantis4.cfg`, which no install has: the
//!   defaults (16 bits, WarpRes 512), the installed `Atlantis3.cfg`
//!   (`WarpRes = 1024`, `PixelFormat = 32bits`) unread; `fix_cfg_name`
//!   reads `Atlantis3.cfg` as the 2022 file does (then `fix_warpres_cd`
//!   gives 512, `Install` being `CD`);
//! - its save thumbnails store every pixel of a row (the 2022 file's one
//!   fewer);
//! - the texts are `loc\text\ita\`'s, the voices `loc\voix\ita\`'s, drawn
//!   in `ANSI_CHARSET` (`font.txt` is GOG.com's French and Spanish one).
//!
//! The intro, the scarab case and the whole story are `languages.rs`'s and
//! `story.rs`'s, with GOG.com's languages.

use atlantis3_data::cfg::PixelFormat;
use atlantis3_data::spr_encode::RowScan;
use atlantis3_data::testing::it_collection;
use atlantis3_data::A3Files;
use atlantis3_engine::px::Surface;
use atlantis3_engine::startup::Startup;
use atlantis3_engine::text::{FontId, Texts};
use atlantis3_engine::{Fixes, Game};

fn files() -> Option<A3Files> {
    Some(A3Files::open_dir(&it_collection()?).unwrap().0)
}

#[test]
fn settings_file_by_build() {
    let Some(files) = files() else {
        return;
    };
    // The original: `Atlantis4.cfg` missing, the defaults, then the
    // registry's values.
    let s = Startup::read_with(&files, Fixes::original());
    let c = &s.config;
    assert_eq!(
        (c.warp_res, c.pixel_format, c.resolution),
        (512, PixelFormat::R5G6B5, (800, 600))
    );
    assert_eq!(
        (&c.langue[..], &c.sous_titre[..], &c.version[..]),
        (&b"ita"[..], &b"ita"[..], &b"CD"[..])
    );
    // `fix_cfg_name` alone: the installed `Atlantis3.cfg`'s 1024 and
    // 32 bits.
    let only = Fixes {
        cfg_name: true,
        ..Fixes::original()
    };
    let c = Startup::read_with(&files, only).config;
    assert_eq!((c.warp_res, c.pixel_format), (1024, PixelFormat::X8R8G8B8));
    assert_eq!(&c.version[..], b"CD");
    // Every fix: `fix_warpres_cd` brings `Version = CD` back to 512.
    let c = Startup::read_with(&files, Fixes::default()).config;
    assert_eq!(
        (c.warp_res, c.warp_view(), c.pixel_format),
        (512, (640, 480), PixelFormat::X8R8G8B8)
    );
}

/// The byte length of each row of a thumbnail bank (`SPRCRYO1`, flags,
/// palette, count, offset, then the sprite: w, h, hot spot, row offsets).
fn row_lengths(bank: &[u8]) -> Vec<usize> {
    let s = 8 + 4 + 1024 + 4 + 4;
    let h = u16::from_le_bytes([bank[s + 2], bank[s + 3]]) as usize;
    let data = s + 8 + 4 * h;
    let mut offs: Vec<usize> = (0..h)
        .map(|y| {
            let o = s + 8 + 4 * y;
            u32::from_le_bytes(bank[o..o + 4].try_into().unwrap()) as usize
        })
        .collect();
    offs.push(bank.len() - data);
    offs.windows(2).map(|w| w[1] - w[0]).collect()
}

/// The thumbnails of an opaque capture: every row whole, one index more
/// than the 2022 file's encoder stores, whose last rows also lose the
/// pixels its drifting probe reads past the picture (zero bytes there).
#[test]
fn thumbnails_store_every_pixel() {
    let Some(files) = files() else {
        return;
    };
    let g = Game::new(files).expect("the game starts");
    assert_eq!(g.menus().encoder.rows, RowScan::Original);
    let (w, h) = (800, 600);
    let capture = Surface::Rgb565(vec![0x8410; w * h]);
    let mut enc = g.menus().encoder.clone();
    let original = atlantis3_engine::menu::thumbnails_of(&capture, w, h, &mut enc);
    enc.rows = RowScan::W10;
    let w10 = atlantis3_engine::menu::thumbnails_of(&capture, w, h, &mut enc);
    for ((&(_, _, th), (_, a)), (_, b)) in atlantis3_engine::menu::THUMBNAILS
        .iter()
        .zip(&original)
        .zip(&w10)
    {
        let (ra, rb) = (row_lengths(a), row_lengths(b));
        assert_eq!(ra.len(), th);
        assert!(ra.iter().all(|&n| n == ra[0]), "{th}: {ra:?}");
        assert!(ra[0] > rb[0], "{th}: {} {}", ra[0], rb[0]);
        assert!(rb[th - 1] < rb[0], "{th}: {rb:?}");
    }
}

/// The dialog texts and fonts of `ita`.
#[test]
fn italian_texts() {
    let Some(files) = files() else {
        return;
    };
    let g = Game::new(files.clone()).expect("the game starts");
    assert_eq!(g.startup.text_language(), "ita");
    assert_eq!(g.startup.voice_language(), "ita");
    let texts = Texts::load(&files, "ita");
    let dial = atlantis3_data::dialog::parse_dial(&files.read("loc/text/ita/Dial.txt").unwrap());
    for l in dial.lines.iter().take(50) {
        assert_eq!(texts.dial(l.dialog, l.line), Some(&l.text[..]));
    }
    for id in [
        FontId::Default,
        FontId::Bulle,
        FontId::Special,
        FontId::Titre,
        FontId::Credit,
    ] {
        for w in [640, 800, 1024] {
            assert_eq!(texts.gdi(id, w).map(|f| f.charset), Some(0), "{id:?} {w}");
        }
    }
}
