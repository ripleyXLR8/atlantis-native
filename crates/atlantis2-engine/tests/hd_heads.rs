//! The HD mode's dialogs and texts (`atlantis2_engine::hd`, a port
//! addition): the heads drawn at more pixels by the original's pipeline,
//! the texts drawn again at the output's size with the original's layout.
//!
//! - a head drawn by [`Head::draw_scaled`] at tile 16 (640x480) is the
//!   original's frame and Z buffer, bit for bit, with every variant;
//! - the HD draw leaves nothing behind: the original's draws that follow
//!   it are those of a head never drawn in HD;
//! - at tile 48 (three times larger) its pixel at the centre of each
//!   original pixel is the original's texel nearly everywhere, and it
//!   covers the original's footprint;
//! - the text recorder draws the original's 640x480 pixels, and the
//!   compositor's uncovering puts back what was under them;
//! - with the game (`ATLANTIS2_DATA`, `ATLANTIS2_DISCS`; skipped without):
//!   a dialog played with the HD mode on gives the 640x480 frames of the
//!   HD mode off, with either variant; its layer stands for the frame before the subtitles; the
//!   subtitles are recorded with the original's layout.

use std::path::PathBuf;

use atlantis2_data::db::Scene;
use atlantis2_data::tga::Tga;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::hd::{HdTexts, TextFont, SCREEN_H, SCREEN_W};
use atlantis2_engine::head::Head;
use atlantis2_engine::raster3d::{HEIGHT, WIDTH};
use atlantis2_engine::{Engine, Enhancements, Fpu, LoopType, Variant};

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn load_head(files: &A2Files, db: &str, variant: Variant, fpu: Fpu) -> Head {
    let data = files.read(Folder::ThreeDDial, db).expect(db);
    let scene = Scene::parse(&data).expect(db);
    let map = Head::texture_name(&scene).expect("texture").to_string();
    let tga = |name: &str| {
        files
            .read(Folder::ThreeDDial, name)
            .map(|d| Tga::parse(&d).unwrap())
    };
    let (yf, ymf) = Head::eye_map_names(&map);
    Head::new(
        fpu,
        variant,
        &scene,
        tga(&map).expect("map"),
        tga(&yf),
        tga(&ymf),
    )
    .unwrap()
}

/// The heads of the goldens, each variant under the word it is checked
/// under (the Pentium II's under 0x0e7f, the C one under 0x027f).
const HEADS: [&str; 3] = ["shs1.db", "H5s1.db", "Ais1.db"];
const VARIANTS: [(Variant, Fpu); 2] = [(Variant::PII, Fpu::Chop), (Variant::C, Fpu::Double)];

/// A frame of a head: the original's draw, then the HD draw at `tile`
/// with the control word the original started under.
struct Pair {
    px: Vec<u16>,
    z: Vec<u32>,
    hd: Vec<u16>,
    hz: Vec<u32>,
    w: usize,
}

fn draw_pair(head: &mut Head, frame: i32, cam: usize, tile: usize, rand: &mut Rand) -> Pair {
    head.pose(frame);
    head.mouth(1, &[0.0, 0.0, 0.6, 0.0, 0.4, 0.0, 0.0]);
    let mut px = vec![0xf81fu16; WIDTH * HEIGHT];
    let mut z = vec![0u32; WIDTH * HEIGHT];
    let before = head.fpu();
    head.display(cam, rand, &mut px, &mut z);
    let after = head.fpu();
    let (w, h) = (40 * tile, 30 * tile);
    let mut hd = vec![0xf81fu16; w * h];
    let mut hz = vec![0u32; w * h];
    head.draw_scaled(cam, before, tile, &mut hd, &mut hz, w, h);
    assert_eq!(
        head.fpu(),
        after,
        "the head's control word as the draw left it"
    );
    Pair { px, z, hd, hz, w }
}

/// At tile 16 the HD draw is the original's draw: the same camera, the
/// same viewport, the same rasteriser.
#[test]
fn hd_head_at_640x480_is_the_original() {
    let Some(files) = game() else { return };
    let mut n = 0;
    for db in HEADS {
        for (variant, fpu) in VARIANTS {
            let mut head = load_head(&files, db, variant, fpu);
            let mut rand = Rand::new(7);
            for cam in 0..head.camera_count() {
                for frame in (1..=40).step_by(3) {
                    let p = draw_pair(&mut head, frame, cam, 16, &mut rand);
                    assert!(p.px == p.hd, "{db} {variant:?} camera {cam} frame {frame}");
                    assert!(
                        p.z == p.hz,
                        "{db} {variant:?} camera {cam} frame {frame}: Z"
                    );
                    n += 1;
                }
            }
        }
    }
    assert!(n >= 6 * 14, "{n} frames");
}

/// The HD draw leaves nothing behind: a head drawn by the original's
/// pipeline after an HD draw at tile 48 gives the frames and Z buffers of
/// the same head never drawn in HD (every variant, every camera, the
/// poses and mouths of [`draw_pair`] in turn).
#[test]
fn hd_head_leaves_the_original_draws_unchanged() {
    let Some(files) = game() else { return };
    for db in HEADS {
        for (variant, fpu) in VARIANTS {
            let mut plain = load_head(&files, db, variant, fpu);
            let mut hd = load_head(&files, db, variant, fpu);
            let (mut r1, mut r2) = (Rand::new(7), Rand::new(7));
            for cam in 0..plain.camera_count() {
                for frame in (1..=40).step_by(3) {
                    plain.pose(frame);
                    plain.mouth(1, &[0.0, 0.0, 0.6, 0.0, 0.4, 0.0, 0.0]);
                    let mut px = vec![0xf81fu16; WIDTH * HEIGHT];
                    let mut z = vec![0u32; WIDTH * HEIGHT];
                    plain.display(cam, &mut r1, &mut px, &mut z);
                    let p = draw_pair(&mut hd, frame, cam, 48, &mut r2);
                    assert!(p.px == px, "{db} {variant:?} camera {cam} frame {frame}");
                    assert!(p.z == z, "{db} {variant:?} camera {cam} frame {frame}: Z");
                    assert_eq!(plain.fpu(), hd.fpu(), "{db} {variant:?}");
                }
            }
        }
    }
}

/// At tile 48 the HD pixel (3x + 1, 3y + 1) is sampled at the centre of
/// the original's pixel (x, y): the same texel nearly everywhere (the
/// rasteriser's fixed point and its 32-pixel perspective steps differ
/// over more pixels); every HD pixel lies on the original's footprint or
/// next to it; the head covers about nine times the pixels.
#[test]
fn hd_head_at_three_times_follows_the_original() {
    let Some(files) = game() else { return };
    for db in HEADS {
        for (variant, fpu) in VARIANTS {
            let mut head = load_head(&files, db, variant, fpu);
            let mut rand = Rand::new(7);
            let (mut same, mut both, mut drawn, mut hd_drawn, mut outside) = (0, 0, 0, 0, 0);
            for cam in 0..head.camera_count() {
                for frame in (1..=40).step_by(5) {
                    let p = draw_pair(&mut head, frame, cam, 48, &mut rand);
                    let near = |x: usize, y: usize| {
                        (y.saturating_sub(1)..(y + 2).min(HEIGHT)).any(|yy| {
                            (x.saturating_sub(1)..(x + 2).min(WIDTH))
                                .any(|xx| p.z[yy * WIDTH + xx] != 0)
                        })
                    };
                    for y in 0..HEIGHT {
                        for x in 0..WIDTH {
                            let i = y * WIDTH + x;
                            let j = (3 * y + 1) * p.w + 3 * x + 1;
                            drawn += usize::from(p.z[i] != 0);
                            if p.z[i] != 0 && p.hz[j] != 0 {
                                both += 1;
                                same += usize::from(p.px[i] == p.hd[j]);
                            }
                        }
                    }
                    for (j, &hz) in p.hz.iter().enumerate() {
                        if hz != 0 {
                            hd_drawn += 1;
                            let (x, y) = ((j % p.w) / 3, (j / p.w) / 3);
                            outside += usize::from(!near(x, y));
                        }
                    }
                }
            }
            eprintln!(
                "{db} {variant:?}: {same} of {both} centre pixels the same texel, \
                 {drawn} original pixels, {hd_drawn} HD pixels, {outside} off the footprint"
            );
            assert!(
                both * 100 >= drawn * 99,
                "{db} {variant:?}: {both} of {drawn}"
            );
            assert!(
                same * 100 >= both * 95,
                "{db} {variant:?}: {same} of {both}"
            );
            assert_eq!(outside, 0, "{db} {variant:?}");
            let ratio = hd_drawn as f64 / drawn as f64;
            assert!((8.5..9.5).contains(&ratio), "{db} {variant:?}: {ratio}");
        }
    }
}

/// The texts' recorder: the 640x480 pixels of the font's own draw, the
/// pixels under the glyphs kept, and the compositor's uncovering puts
/// them back (the background's pixels all differ from the glyphs'
/// black and white); off, nothing is kept.
#[test]
fn recorded_texts_are_the_original_pixels() {
    let Some(files) = game() else { return };
    let f = files
        .read(Folder::Loc, "Font.txt")
        .map(|d| atlantis2_data::dialog::FontTxt::parse(&d))
        .expect("Font.txt");
    let ttf = files
        .read(Folder::Loc, &f.font("AtlFT_Dialog").unwrap().file)
        .unwrap();
    let font = atlantis2_engine::subtitle::SubtitleFont::new(ttf, &f).unwrap();
    let bg: Vec<u16> = (0..SCREEN_W * SCREEN_H)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761) as u16)
        .collect();
    let calls = font.layout(&b"Un essai, deux lignes peut-\xeatre ".repeat(5), 640, 480);
    let mut plain = bg.clone();
    font.draw(&mut plain, 640, 480, &calls);
    for on in [false, true] {
        let mut t = HdTexts::default();
        t.begin(on);
        let mut out = bg.clone();
        t.draw(TextFont::Dialog, &font, &mut out, 640, 480, &calls);
        assert!(out == plain, "on: {on}");
        assert_eq!(t.runs.len(), if on { calls.len() } else { 0 });
        if on {
            assert!(t
                .runs
                .iter()
                .zip(&calls)
                .all(|(r, c)| r.call == *c && r.alpha == 255 && r.font == TextFont::Dialog));
            let mut back = out.clone();
            t.text.uncover(&mut back);
            assert!(back == bg, "the glyph pixels put back");
        } else {
            assert!(t.text.under.is_empty());
        }
    }
}

/// The text at the HD size: its glyphs where the original's are, scaled
/// (each lit HD pixel within a pixel of a scaled cell of the original's
/// line), and nearly every original glyph pixel has HD pixels lit.
#[test]
fn hd_texts_keep_the_original_layout() {
    let Some(files) = game() else { return };
    let f = files
        .read(Folder::Loc, "Font.txt")
        .map(|d| atlantis2_data::dialog::FontTxt::parse(&d))
        .expect("Font.txt");
    for id in ["AtlFT_Dialog", "AtlFT_POPUP"] {
        let ttf = files.read(Folder::Loc, &f.font(id).unwrap().file).unwrap();
        let font = atlantis2_engine::subtitle::SubtitleFont::named(ttf, &f, id).unwrap();
        let calls: Vec<_> = font
            .layout(
                &b"Un texte d\x27essai, \xe0 la ligne apr\xe8s. ".repeat(4),
                640,
                480,
            )
            .into_iter()
            .filter(|c| c.colour != 0)
            .collect();
        let mut low = vec![0u16; 640 * 480];
        font.draw(&mut low, 640, 480, &calls);
        for (dw, dh) in [(1280, 960), (1440, 1080), (2560, 1920)] {
            let mut out = vec![0u32; dw * dh];
            for c in &calls {
                font.draw_hd(&mut out, dw, (0, 0, dw, dh), c, 255);
            }
            let s = dw as f64 / 640.0;
            let lit = |x: usize, y: usize| low[y * 640 + x] != 0;
            let mut off = 0;
            let mut n = 0;
            for (i, &p) in out.iter().enumerate() {
                if p & 0xff < 0x80 {
                    continue;
                }
                n += 1;
                let (x, y) = ((i % dw) as f64 / s, (i / dw) as f64 / s);
                let (x, y) = (x as usize, y as usize);
                let near = (y.saturating_sub(1)..(y + 2).min(480))
                    .any(|yy| (x.saturating_sub(1)..(x + 2).min(640)).any(|xx| lit(xx, yy)));
                off += usize::from(!near);
            }
            let (mut covered, mut total) = (0, 0);
            for y in 0..480 {
                for x in 0..640 {
                    if !lit(x, y) {
                        continue;
                    }
                    total += 1;
                    let (x0, y0) = ((x as f64 * s) as usize, (y as f64 * s) as usize);
                    let (x1, y1) = (((x + 1) as f64 * s) as usize, ((y + 1) as f64 * s) as usize);
                    let any = (y0.saturating_sub(1)..(y1 + 1).min(dh)).any(|yy| {
                        (x0.saturating_sub(1)..(x1 + 1).min(dw)).any(|xx| out[yy * dw + xx] != 0)
                    });
                    covered += usize::from(any);
                }
            }
            eprintln!(
                "{id} {dw}x{dh}: {n} HD pixels, {off} away from the original's glyphs; \
                 {covered} of {total} original pixels with HD ink"
            );
            assert!(n > 0 && off * 1000 <= n, "{id} {dw}x{dh}: {off} of {n}");
            assert!(
                covered * 100 >= total * 99,
                "{id} {dw}x{dh}: {covered} of {total}"
            );
        }
    }
}

/// A new game's first view with dialog `number` started over it, with
/// the routines of `variant`.
fn dialog(files: A2Files, number: u32, hd: bool, (variant, fpu): (Variant, Fpu)) -> Engine {
    let mut e = Engine::boot(files, variant, fpu, 640, 480).unwrap();
    e.enhancements = Enhancements {
        hd,
        ..Enhancements::default()
    };
    e.start_new_game().unwrap();
    e.end_video();
    for _ in 0..200 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp {
            break;
        }
    }
    e.start_dialog(number).unwrap();
    e
}

/// Dialog 360 (Tibet's shaman, its three lines), with the Pentium II's
/// routines and the C ones: the HD mode on gives the original's frames
/// (the HD head's draw leaves nothing behind for the next original draw);
/// each frame's layer stands for the frame before the subtitles (the
/// frame differs from it only by the recorded glyphs); the layer shows the
/// head at the HD size; the subtitles recorded are the layout of the line
/// on screen.
#[test]
fn hd_dialog_keeps_the_original_frames() {
    if game().is_none() {
        return;
    }
    for variant in VARIANTS {
        dialog_both_ways(variant);
    }
}

fn dialog_both_ways(variant: (Variant, Fpu)) {
    let v = variant.0;
    let mut off = dialog(game().unwrap(), 360, false, variant);
    let mut on = dialog(game().unwrap(), 360, true, variant);
    for e in [&mut off, &mut on] {
        e.set_hd_output(Some(24), 3);
    }
    let (mut a, mut b) = (vec![0u16; 640 * 480], vec![0u16; 640 * 480]);
    let (mut layers, mut texts, mut heads) = (0, 0, 0);
    for i in 0..900 {
        for e in [&mut off, &mut on] {
            e.set_clock(i as f64 * PASS);
            e.tick();
        }
        if i % 120 == 119 {
            off.click(i as f64 * PASS);
            on.click(i as f64 * PASS);
        }
        off.pass(Some(&mut a));
        on.pass(Some(&mut b));
        assert!(a == b, "{v:?}: frame {i} differs with the HD mode");
        assert!(off.hd_layer().is_none() && off.hd_texts().is_none());
        if on.loop_type != LoopType::Dialog {
            break;
        }
        let Some(l) = on.hd_layer() else { continue };
        layers += 1;
        assert_eq!((l.width, l.height, l.side), (40 * 24 + 2 * 72, 30 * 24, 72));
        let t = on.hd_texts().expect("texts");
        // The frame is the layer's base but for the glyphs.
        for (k, &p) in b.iter().enumerate() {
            if p != l.base[k] {
                assert!(
                    t.text.after[k] == p && t.text.under[k] == l.base[k],
                    "frame {i}: pixel ({}, {}) is not a glyph's",
                    k % 640,
                    k / 640
                );
            }
        }
        if let Some(line) = on.dialog().and_then(|d| d.current_line()) {
            if on.subtitles {
                let font = on.subtitle_font().unwrap();
                let calls = font.layout(&line.text, 640, 480);
                let runs: Vec<_> = t.runs.iter().map(|r| r.call.clone()).collect();
                assert_eq!(runs, calls, "frame {i}");
                texts += 1;
            }
        }
        // The head drawn at the HD size is in the layer, where the
        // original's head was drawn (and next to it).
        let d = on.hd_dialog_capture();
        if i % 120 == 0 {
            let body = d.body.iter().filter(|&&b| b != 0).count();
            let differ = (0..640 * 480)
                .filter(|&k| d.composite[k] != d.pano[k])
                .count();
            eprintln!("frame {i}: body {body} pixels, {differ} differ from the view");
        }
        if d.drawn {
            let lut = atlantis2_engine::hd::rgb565_to_888();
            let (hw, hh) = d.head_size();
            assert_eq!((hw, hh), (40 * 24, 30 * 24));
            // Widescreen moves a character that touches one edge of the
            // picture to the wide picture's edge (the first game's
            // `dialog_shift`): the head with it.
            let mut best = (0, 0);
            for shift in [0, l.side as isize, -(l.side as isize)] {
                let (mut shown, mut masked) = (0, 0);
                for y in 0..hh {
                    for x in 0..hw {
                        if d.mask[y * hw + x] == 0 {
                            continue;
                        }
                        let (sx, sy) = (x * 16 / 24, y * 16 / 24);
                        let near = (sy.saturating_sub(1)..(sy + 2).min(480)).any(|yy| {
                            (sx.saturating_sub(1)..(sx + 2).min(640))
                                .any(|xx| d.pre_head[yy * 640 + xx] != d.composite[yy * 640 + xx])
                        });
                        let Some(c) = (l.side + x)
                            .checked_add_signed(shift)
                            .filter(|&c| c < l.width)
                        else {
                            continue;
                        };
                        masked += 1;
                        let px = l.pixels[y * l.width + c];
                        shown += usize::from(near && px == lut[usize::from(d.head[y * hw + x])]);
                    }
                }
                if shown > best.0 {
                    best = (shown, masked);
                }
            }
            let (shown, masked) = best;
            assert!(shown * 100 >= masked * 99, "frame {i}: {shown} of {masked}");
            heads += 1;
        }
    }
    eprintln!("{v:?}: {layers} frames with the layer, {texts} with subtitles, {heads} with a head");
    assert!(
        layers > 300 && texts > 300 && heads > 300,
        "{v:?}: {layers} {texts} {heads}"
    );
}

/// Runs `frames` passes of both engines from `at`, the frames compared;
/// `check` sees each frame of the HD engine with its texts.
fn both_ways(
    off: &mut Engine,
    on: &mut Engine,
    at: usize,
    frames: usize,
    mut check: impl FnMut(usize, &[u16], &HdTexts),
) {
    let (mut a, mut b) = (vec![0u16; 640 * 480], vec![0u16; 640 * 480]);
    for i in at..at + frames {
        for e in [&mut *off, &mut *on] {
            e.set_clock(i as f64 * PASS);
            e.tick();
        }
        off.pass(Some(&mut a));
        on.pass(Some(&mut b));
        assert!(a == b, "frame {i} differs with the HD mode");
        assert!(off.hd_texts().is_none());
        let t = on.hd_texts().expect("texts");
        // The frame shows the recorded glyphs (but where something was
        // drawn over them afterwards: the pointer).
        let glyphs: Vec<usize> = (0..640 * 480)
            .filter(|&k| t.text.under[k] != t.text.after[k])
            .collect();
        let shown = glyphs.iter().filter(|&&k| b[k] == t.text.after[k]).count();
        assert!(
            shown * 10 >= glyphs.len() * 9,
            "frame {i}: {shown} of {}",
            glyphs.len()
        );
        check(i, &b, t);
    }
}

/// The menus' tooltips, the bag's names and the credits: the frames of
/// the HD mode off, the texts recorded with their font and weight.
#[test]
fn hd_texts_of_the_menus_bag_and_credits() {
    if game().is_none() {
        return;
    }
    let boot = |hd| {
        let mut e = dialog(game().unwrap(), 360, hd, VARIANTS[0]);
        e.end_dialog();
        e.set_hd_output(Some(36), 0);
        e
    };
    // The bag: two objects, the pointer over the first cell.
    let (mut off, mut on) = (boot(false), boot(true));
    for e in [&mut off, &mut on] {
        e.run_command(b"(sacados=LANTERNE)");
        e.run_command(b"(sacados=BAGUETTE)");
        e.right_click();
        e.pointer = (40, 440);
    }
    let mut named = 0;
    both_ways(&mut off, &mut on, 1000, 20, |_, _, t| {
        if let [black, white] = &t.runs[..] {
            assert_eq!((black.font, white.font), (TextFont::Popup, TextFont::Popup));
            assert_eq!((black.call.colour, white.call.colour), (0, 0xffffff));
            assert_eq!(
                (black.call.x - white.call.x, black.call.y - white.call.y),
                (1, 1)
            );
            assert_eq!(white.call.y, 480 - 70);
            named += 1;
        }
    });
    assert!(named >= 18, "{named} frames with the name");
    // A menu button's tooltip, after 10 ticks in its box.
    let (mut off, mut on) = (boot(false), boot(true));
    for e in [&mut off, &mut on] {
        e.open_menu_page(atlantis2_engine::menu::PageKind::Main);
        e.pointer(375, 238);
    }
    let mut tips = 0;
    both_ways(&mut off, &mut on, 2000, 90, |_, _, t| {
        if !t.runs.is_empty() {
            assert!(t
                .runs
                .iter()
                .all(|r| r.font == TextFont::Popup && r.alpha == 255));
            tips += 1;
        }
    });
    assert!(tips > 30, "{tips} frames with the tooltip");
    // The credits: each page's texts at the fade's weights.
    let (mut off, mut on) = (boot(false), boot(true));
    for e in [&mut off, &mut on] {
        e.open_credits();
    }
    let (mut faded, mut pages) = (0, 0);
    both_ways(&mut off, &mut on, 3000, 900, |_, _, t| {
        let weights: std::collections::BTreeSet<u8> = t.runs.iter().map(|r| r.alpha).collect();
        if weights.len() == 2 {
            let w: Vec<u8> = weights.into_iter().collect();
            assert_eq!(u32::from(w[0]) + u32::from(w[1]), 255);
            faded += 1;
        }
        pages += usize::from(!t.runs.is_empty());
    });
    assert!(faded > 100 && pages > 800, "{faded} {pages}");
}

/// The HD head's timings (`cargo test --release -p atlantis2-engine --test
/// hd_heads -- --ignored --nocapture hd_head_timings`): the original's
/// draw and the HD draw at 1440x1080 (tile 36) and 1920x1440 (tile 48),
/// the largest head of the goldens, the Pentium II's routines.
#[test]
#[ignore]
fn hd_head_timings() {
    use std::time::Instant;
    let Some(files) = game() else { return };
    let mut head = load_head(&files, "Ais1.db", Variant::PII, Fpu::Chop);
    let mut rand = Rand::new(7);
    let n = 40;
    let mut px = vec![0u16; WIDTH * HEIGHT];
    let mut z = vec![0u32; WIDTH * HEIGHT];
    let t = Instant::now();
    for f in 0..n {
        head.pose(1 + f % 40);
        z.fill(0);
        head.display(0, &mut rand, &mut px, &mut z);
    }
    let low = t.elapsed().as_secs_f64() * 1e3 / f64::from(n);
    eprintln!("original 640x480: {low:.2} ms a frame");
    for tile in [36, 48] {
        let (w, h) = (40 * tile, 30 * tile);
        let mut hp = vec![0u16; w * h];
        let mut hz = vec![0u32; w * h];
        let t = Instant::now();
        for f in 0..n {
            head.pose(1 + f % 40);
            hz.fill(0);
            head.draw_scaled(0, Fpu::Chop, tile, &mut hp, &mut hz, w, h);
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / f64::from(n);
        eprintln!("HD {w}x{h}: {ms:.2} ms a frame (Z buffer cleared included)");
    }
}
