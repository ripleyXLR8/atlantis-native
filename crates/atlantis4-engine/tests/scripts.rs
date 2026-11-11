//! The warp driven by the node scripts, against the installed game.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder (see
//! `atlantis4_data::testing`); skipped cleanly with
//! `ATLANTIS4_SKIP_DATA_TESTS=1`.
//!
//! The call orders compared here are the original's, from the live Lua
//! trace of a new game in the original, without their arguments.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use atlantis4_data::anim::Anim;
use atlantis4_data::scene::Scene;
use atlantis4_data::wpa::Package as Wpa;
use atlantis4_data::{A4Files, WarpPath};
use atlantis4_engine::markers::MarkerSprite;
use atlantis4_engine::package::{Codec, Package};
use atlantis4_engine::render::{bilinear_rgba, Weights};
use atlantis4_engine::Game;
use atlantis4_script::{new_game, read_boot_vars, BootValue, EngineRef, Options, Script, Value};

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// A new game as the viewer boots it, with the call trace on.
fn boot(dir: &Path) -> (Rc<RefCell<Game>>, Script) {
    let (files, _) = A4Files::open_dir(dir).unwrap();
    let game = Rc::new(RefCell::new(Game::new(files, 768)));
    game.borrow_mut().trace = Some(Vec::new());
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    // The live trace is the game state's: the boot `Texts` files each run
    // in a state of their own before it.
    take_trace(&game);
    let Some(BootValue::Str(first)) = boot.first_warp_path else {
        panic!("no firstWarpPath")
    };
    assert_eq!(first, b"3D/Bateau_CA/0001.wp");
    let script = new_game(&engine, Options::default(), &first).unwrap();
    (game, script)
}

fn take_trace(game: &Rc<RefCell<Game>>) -> Vec<String> {
    std::mem::take(game.borrow_mut().trace.as_mut().unwrap())
}

/// Where the warp's loaded animations stand: name, clickable, frame.
fn loaded(game: &Game) -> Vec<(String, bool, i32)> {
    game.warp
        .as_ref()
        .unwrap()
        .anims
        .iter()
        .filter(|a| a.package.is_some())
        .map(|a| (a.name.clone(), a.clickable, a.anim.frame))
        .collect()
}

/// The shown markers' images, in array order.
fn shown(game: &Game) -> Vec<u32> {
    game.warp
        .as_ref()
        .unwrap()
        .markers
        .iter()
        .filter_map(|m| m.sprite.map(|s| s.image))
        .collect()
}

#[test]
fn the_first_node_is_set_up_by_its_scripts_as_the_original() {
    let Some(dir) = install() else { return };
    let (game, mut script) = boot(&dir);
    // The live trace of the original up to the movie's end, arguments
    // left out.
    let want = [
        "FILE 3D/OnGameEnter.lua",
        "FILE Diary/Diary.lua",
        "FILE Inventory/InventoryScripts.lua",
        "FILE 3D/Bateau_CA/0001.lua",
        "ENTER OnWarpEnter",
        "CALL PlayMusic",
        "CALL PlaySound",
        "CALL SetMarker",
        "CALL SetMarker",
        "CALL PlayMovie",
        "CALL SetMarker",
        "CALL StartAnimationPart",
        "CALL StartAnimationPart",
    ];
    let got = take_trace(&game);
    let got: Vec<&str> = got.iter().map(String::as_str).collect();
    assert_eq!(got, want);
    {
        let g = game.borrow();
        let w = g.warp.as_ref().unwrap();
        assert_eq!(w.node.path, WarpPath::parse("3D/Bateau_CA/0001.wp").unwrap());
        // `StartAnimationPart("hua2", 14, 15, -1, true)` (the porthole
        // open), then `("mla2", 0, 1, -1, false)` (the trunk shut): last
        // in the list, one frame each.
        assert_eq!(
            loaded(&g),
            [("hua2".into(), true, 14), ("mla2".into(), false, 0)]
        );
        // Both exits with `Marker_0`, the porthole with `Marker_4`.
        let mut images = shown(&g);
        images.sort_unstable();
        assert_eq!(images, [0, 0, 4]);
        // 8 frames a second from the `SetMarker` call.
        let m = w.markers.iter().find_map(|m| m.sprite).unwrap();
        assert_eq!(m.frame(m.start_ms + 999.0), 7);
        assert_eq!(MarkerSprite::band(7), 7);
    }

    // A click on the porthole (on its marker) runs `OnWarpObjectHit("hua2")`,
    // which shuts it: frame 0, marker hidden.
    let (x, y) = {
        let g = game.borrow();
        let w = g.warp.as_ref().unwrap();
        let hua2 = w.anims.iter().find(|a| a.name == "hua2").unwrap();
        let p = w.markers[hua2.markers[0]].point;
        let yaw = -(f64::from(p[0])).atan2(-f64::from(p[2])).to_degrees() as f32;
        drop(g);
        let mut g = game.borrow_mut();
        g.view.set_yaw(yaw);
        let cam = g.camera(1365, 768);
        let s = cam.project(p);
        (s[0] as i32, 768 - s[1] as i32)
    };
    let hit = game.borrow().click(1365, 768, x, y);
    assert_eq!(hit.as_deref(), Some("hua2"));
    script
        .callback("OnWarpObjectHit", &[Value::Str(b"hua2".to_vec())])
        .unwrap();
    {
        let g = game.borrow();
        assert_eq!(
            loaded(&g),
            [("mla2".into(), false, 0), ("hua2".into(), true, 0)]
        );
        assert_eq!(shown(&g), [0, 0]);
    }
    take_trace(&game);

    // Ahead and low at yaw 0: the exit to the next node, through the
    // scripts.
    {
        let mut g = game.borrow_mut();
        g.view = atlantis4_engine::View::default();
        g.view.set_pitch(-30.0);
    }
    let hit = game.borrow().click(1365, 768, 682, 384).unwrap();
    assert_eq!(hit, "3D\\Bateau_CA\\0002.wp");
    script
        .callback("OnWarpObjectHit", &[Value::Str(hit.into_bytes())])
        .unwrap();
    let got = take_trace(&game);
    let got: Vec<&str> = got.iter().map(String::as_str).collect();
    // The original's trace from the click on, up to `ShowDocument`.
    assert_eq!(
        got,
        [
            "ENTER OnWarpObjectHit",
            "CALL ChangeWarp",
            "ENTER OnWarpLeave",
            "FILE 3D/Bateau_CA/0002.lua",
            "ENTER OnWarpEnter",
            "CALL SetMarker",
            "CALL SetMarker",
            "CALL StartAnimationPart",
            "CALL SetMarker",
            "CALL PutObject",
            "CALL ShowDocument",
        ]
    );
    let g = game.borrow();
    let w = g.warp.as_ref().unwrap();
    assert_eq!(w.node.path, WarpPath::parse("3D/Bateau_CA/0002.wp").unwrap());
    // `ChangeWarp` keeps the angles and resets the limits.
    assert_eq!((g.view.yaw, g.view.pitch), (0.0, -30.0));
    assert_eq!((g.view.pitch_plus, g.view.pitch_minus), (75.0, 25.0));
    assert_eq!(
        loaded(&g),
        [("hua2".into(), true, 0), ("mla1".into(), true, 0)]
    );
}

/// The scene descriptions `3D/<zone>/<node>.xml`, sorted, as node paths.
fn nodes(dir: &Path) -> Vec<WarpPath> {
    let mut out = Vec::new();
    let mut zones: Vec<_> = std::fs::read_dir(dir.join("3D"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    zones.sort();
    for z in zones {
        let mut xs: Vec<_> = std::fs::read_dir(&z)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")))
            .collect();
        xs.sort();
        for x in xs {
            let rel = x.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
            out.push(WarpPath::parse(rel.strip_suffix(".xml").unwrap()).unwrap());
        }
    }
    out
}

/// Most range lists start at frame 0; 77 tiles (and no pick list) start
/// later and draw nothing before it (the null entries of the frame table,
/// [`atlantis4_engine::package::range_index`]).
#[test]
fn some_tiles_start_after_frame_0() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let src = files.source();
    let (mut packages, mut tiles, mut late_tiles, mut late_picks) = (0, 0, 0, 0);
    for node in nodes(&dir) {
        let scene = Scene::parse(&files.read(&node.xml()).unwrap()).unwrap();
        let names = scene
            .objects
            .iter()
            .map(|o| &o.name)
            .chain(scene.animations.iter().map(|a| &a.name));
        for name in names {
            let pkg_name = node.package(name);
            let Some(Ok(pkg)) = Wpa::open(src, &pkg_name) else {
                continue;
            };
            let raw = pkg.read(src, &pkg_name, pkg.entry("anim.anim").unwrap()).unwrap();
            let anim = Anim::parse(&raw).unwrap();
            packages += 1;
            for t in &anim.tiles {
                tiles += 1;
                if t.ranges.first().is_some_and(|r| r.first_frame > 0) {
                    late_tiles += 1;
                }
            }
            if anim.picks.first().is_some_and(|p| p.first_frame > 0) {
                late_picks += 1;
            }
        }
    }
    assert_eq!((packages, tiles, late_tiles, late_picks), (1951, 4195, 77, 0));
}

/// The objects' pictures (`X.rgb.jpg` + `X.alpha.jpg`) are addressed as
/// the background atlases: row `(1 − v)·h` from the top. An object covers
/// several cells whose slots are packed in its picture in no visual order;
/// read this way, neighbouring cells continue each other across their
/// shared edge far better than read upside down (which jumbles them).
#[test]
fn object_pictures_are_read_from_the_bottom_as_the_atlases() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let (mut edges, mut closer) = (0u32, 0u32);
    let (mut decisive, mut decisive_closer) = (0u32, 0u32);
    let (mut sum_up, mut sum_down) = (0.0f64, 0.0f64);
    for node_path in nodes(&dir) {
        let scene = Scene::parse(&files.read(&node_path.xml()).unwrap()).unwrap();
        for o in &scene.objects {
            let Ok(pkg) = Package::load(&files, &node_path, &o.name) else {
                continue;
            };
            for tile in &pkg.tiles {
                let Codec::Still(pic) = &tile.codec else {
                    continue;
                };
                // Each cell: its face and grid place, and its UV as an
                // affine function of (s, t), from its first triangle.
                let r = &tile.ranges[0];
                let mut cells = std::collections::HashMap::new();
                for q in r.indices.as_chunks::<6>().0 {
                    let ids: Vec<usize> = q.iter().map(|&i| i as usize).collect();
                    let c = ids.iter().fold([0.0f64; 3], |a, &i| {
                        std::array::from_fn(|k| a[k] + f64::from(r.vertices[i][k]) / 6.0)
                    });
                    let Some((f, s, t)) = atlantis4_engine::cube::face_point(c) else {
                        continue;
                    };
                    let st: Vec<(f64, f64)> = ids[..3]
                        .iter()
                        .map(|&i| atlantis4_engine::cube::st_of(f, r.vertices[i]))
                        .collect();
                    let uv: Vec<[f64; 2]> = ids[..3].iter().map(|&i| r.uvs[i].map(f64::from)).collect();
                    let key = (f, (s * 13.0).floor() as i32, (t * 13.0).floor() as i32);
                    cells.insert(key, (st, uv));
                }
                let uv_at = |cell: &(Vec<(f64, f64)>, Vec<[f64; 2]>), ps: f64, pt: f64| {
                    let (st, uv) = cell;
                    let (ds1, dt1) = (st[1].0 - st[0].0, st[1].1 - st[0].1);
                    let (ds2, dt2) = (st[2].0 - st[0].0, st[2].1 - st[0].1);
                    let det = ds1 * dt2 - ds2 * dt1;
                    let (es, et) = (ps - st[0].0, pt - st[0].1);
                    let k1 = (es * dt2 - ds2 * et) / det;
                    let k2 = (ds1 * et - es * dt1) / det;
                    let g = |c: usize| uv[0][c] + k1 * (uv[1][c] - uv[0][c]) + k2 * (uv[2][c] - uv[0][c]);
                    (g(0), g(1))
                };
                let sample = |cell, ps, pt, flip: bool| {
                    let (u, v) = uv_at(cell, ps, pt);
                    let h = f64::from(pic.height);
                    let y = if flip { v * h } else { (1.0 - v) * h };
                    bilinear_rgba(pic, u * f64::from(pic.width) - 0.5, y - 0.5, Weights::Exact)
                };
                let diff = |a: [u8; 4], b: [u8; 4]| -> f64 {
                    (0..3).map(|k| (f64::from(a[k]) - f64::from(b[k])).abs()).sum()
                };
                let inside = 1.0 / (13.0 * 60.0);
                for (&(f, ci, cj), a) in &cells {
                    for (di, dj) in [(1, 0), (0, 1)] {
                        let Some(b) = cells.get(&(f, ci + di, cj + dj)) else {
                            continue;
                        };
                        let (mut up, mut down, mut n) = (0.0, 0.0, 0);
                        for k in 0..24 {
                            let along = (f64::from(k) + 0.5) / 24.0 / 13.0;
                            let (es, et) = (f64::from(ci + di) / 13.0, f64::from(cj + dj) / 13.0);
                            let (pa, pb) = if di == 1 {
                                ((es - inside, f64::from(cj) / 13.0 + along), (es + inside, f64::from(cj) / 13.0 + along))
                            } else {
                                ((f64::from(ci) / 13.0 + along, et - inside), (f64::from(ci) / 13.0 + along, et + inside))
                            };
                            let (ra, rb) = (sample(a, pa.0, pa.1, false), sample(b, pb.0, pb.1, false));
                            let (fa, fb) = (sample(a, pa.0, pa.1, true), sample(b, pb.0, pb.1, true));
                            if [ra, rb, fa, fb].iter().any(|p| p[3] < 250) {
                                continue;
                            }
                            up += diff(ra, rb);
                            down += diff(fa, fb);
                            n += 1;
                        }
                        if n >= 12 {
                            edges += 1;
                            sum_up += up;
                            sum_down += down;
                            if up < down {
                                closer += 1;
                            }
                            // An edge whose two readings differ by half.
                            if (up - down).abs() > 0.5 * up.max(down) {
                                decisive += 1;
                                if up < down {
                                    decisive_closer += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "{closer} of {edges} shared edges continue better read from the bottom \
         ({decisive_closer} of {decisive} decisive); mean difference {:.1} vs {:.1}",
        sum_up / f64::from(edges),
        sum_down / f64::from(edges)
    );
    assert!(edges >= 500, "{edges} edges compared");
    assert!(sum_up < 0.35 * sum_down, "{sum_up} vs {sum_down}");
    assert!(
        f64::from(decisive_closer) >= 0.85 * f64::from(decisive),
        "{decisive_closer} of {decisive} decisive edges"
    );
}
