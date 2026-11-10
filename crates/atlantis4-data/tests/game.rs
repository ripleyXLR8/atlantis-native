//! Every file of the installed game read by this crate, with the counts the
//! original's readers give.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder.

use std::path::{Path, PathBuf};

use atlantis4_data::A4Files;

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// Every file under `dir`, as paths relative to `root` with `/`, sorted.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy();
            out.push((rel.replace('\\', "/"), e.metadata().unwrap().len()));
        }
    }
}

#[test]
fn the_executable_is_known() {
    let Some(dir) = install() else { return };
    let (files, build) = A4Files::open_dir(&dir).unwrap();
    assert_eq!(build.id, "atlantis4-gog-en-2363910");
    assert!(atlantis4_data::files::holds_atlantis4(&dir));
    assert!(files.lang_ini().is_some());
}

#[test]
fn the_install_holds_every_file_of_the_inventory() {
    let Some(dir) = install() else { return };
    let mut all = Vec::new();
    walk(&dir, &dir, &mut all);
    let bytes: u64 = all.iter().map(|f| f.1).sum();
    assert_eq!((all.len(), bytes), (9_858, 1_932_367_911));
    let mut by_ext = std::collections::BTreeMap::<String, usize>::new();
    for (name, _) in &all {
        let base = name.rsplit('/').next().unwrap();
        let ext = base
            .rsplit_once('.')
            .map_or("", |x| x.1)
            .to_ascii_lowercase();
        *by_ext.entry(ext).or_default() += 1;
    }
    let want = [
        ("dll", 5),
        ("exe", 1),
        ("font", 10),
        ("hashdb", 1),
        ("info", 1),
        ("ini", 1),
        ("jpg", 102),
        ("lua", 504),
        ("ogg", 5_303),
        ("ogv", 446),
        ("png", 433),
        ("script", 1),
        ("tef", 6),
        ("tga", 2),
        ("txt", 2),
        ("wav", 11),
        ("wpa", 2_613),
        ("wpp", 1),
        ("xml", 415),
    ];
    let want: std::collections::BTreeMap<String, usize> =
        want.iter().map(|(k, n)| (k.to_string(), *n)).collect();
    assert_eq!(by_ext, want);
}

/// Every file of the install with the extension `ext` (lower case), as
/// `/`-separated names relative to the install, sorted.
fn files_with(dir: &Path, ext: &str) -> Vec<String> {
    let mut all = Vec::new();
    walk(dir, dir, &mut all);
    all.into_iter()
        .map(|f| f.0)
        .filter(|n| n.to_ascii_lowercase().ends_with(&format!(".{ext}")))
        .collect()
}

/// Every warp package: its directory, the kinds of its entries, and its
/// `anim.anim` parsed to the last byte, every tile's files found.
#[test]
fn every_warp_package_and_its_animation_read() {
    use atlantis4_data::anim::Anim;
    use atlantis4_data::wpa::{Package, TileSource};
    use std::collections::BTreeMap;

    let Some(dir) = install() else { return };
    let src = atlantis_data::StdDir::new(&dir);
    let names = files_with(&dir, "wpa");
    let mut kinds = BTreeMap::<String, usize>::new();
    let mut sources = BTreeMap::<&str, usize>::new();
    let (mut tiles, mut ranges, mut picks, mut verts, mut pick_verts) = (0, 0, 0, 0, 0);
    let mut backgrounds = 0;
    for name in &names {
        let data = atlantis_data::FileSource::read(&src, name).unwrap();
        let p = Package::parse(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The first entry follows the directory, the last ends the file.
        let first = p.entries.first().unwrap();
        assert_eq!(first.offset as usize, p.directory_end, "{name}");
        let last = p.entries.last().unwrap();
        assert_eq!((last.offset + last.size) as usize, data.len(), "{name}");
        for e in &p.entries {
            let kind = match e.name.split_once('.') {
                Some((_, k)) => k.to_string(),
                None => e.name.clone(),
            };
            *kinds.entry(kind).or_default() += 1;
        }
        let e = p.entry("anim.anim").expect(name);
        let a = Anim::parse(p.slice(&data, e).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        for t in &a.tiles {
            let s =
                TileSource::resolve(&p, &t.name).unwrap_or_else(|| panic!("{name}: {}", t.name));
            *sources
                .entry(match s {
                    TileSource::Jpeg(_) => "jpeg",
                    TileSource::JpegWithAlpha { .. } => "jpeg+alpha",
                    TileSource::TheoraWithAlpha { .. } => "theora+alpha",
                })
                .or_default() += 1;
            for r in &t.ranges {
                assert_eq!(r.uvs.len(), r.vertices.len(), "{name}");
                assert!(r.indices.iter().all(|&i| (i as usize) < r.vertices.len()));
                verts += r.vertices.len();
            }
            assert!(t
                .ranges
                .windows(2)
                .all(|w| w[0].first_frame < w[1].first_frame));
            ranges += t.ranges.len();
        }
        tiles += a.tiles.len();
        picks += a.picks.len();
        pick_verts += a.picks.iter().map(|p| p.vertices.len()).sum::<usize>();
        if name.ends_with("/background.wpa") && !a.tiles.is_empty() {
            backgrounds += 1;
            assert_eq!((a.frame_count, a.tiles.len()), (1, 28), "{name}");
            let quads: usize = a.tiles.iter().map(|t| t.ranges[0].indices.len() / 6).sum();
            assert_eq!(quads, 6 * 13 * 13, "{name}");
            let on_cube = |v: &[f32; 3]| v.iter().any(|c| c.abs() == 500.0);
            assert!(a
                .tiles
                .iter()
                .all(|t| t.ranges[0].vertices.iter().all(on_cube)));
        }
    }
    assert_eq!(names.len(), 2_613);
    assert_eq!(backgrounds, 398);
    let kinds: Vec<(&str, usize)> = kinds.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    assert_eq!(
        kinds,
        [
            ("alpha.jpg", 345),
            ("alpha.ogv", 742),
            ("anim", 2_613),
            ("jpg", 11_144),
            ("ogv", 742),
            ("rgb.jpg", 345),
        ]
    );
    let sources: Vec<(&str, usize)> = sources.into_iter().collect();
    assert_eq!(
        sources,
        [("jpeg", 11_144), ("jpeg+alpha", 345), ("theora+alpha", 742)]
    );
    assert_eq!(
        (tiles, ranges, picks, verts, pick_verts),
        (12_231, 19_501, 11_039, 1_958_716, 882_038)
    );
}

/// Every scene description: its elements, exits that lead to scenes, and
/// the warp packages its objects and animations name.
#[test]
fn every_scene_description_reads() {
    use atlantis4_data::scene::Scene;
    use atlantis4_data::WarpPath;
    use std::collections::BTreeMap;

    let Some(dir) = install() else { return };
    let src = atlantis_data::StdDir::new(&dir);
    let read = |n: &str| atlantis_data::FileSource::read(&src, n);
    let has = |n: &str| atlantis_data::FileSource::size(&src, n).is_some();
    let names: Vec<String> = files_with(&dir, "xml")
        .into_iter()
        .filter(|n| n.starts_with("3D/"))
        .collect();
    assert_eq!(names.len(), 398);
    let mut versions = BTreeMap::<(u32, u32), usize>::new();
    let mut fps = BTreeMap::<u32, usize>::new();
    let (mut objects, mut animations, mut anim_markers, mut exits) = (0, 0, 0, 0);
    let (mut blocks, mut cameras, mut to_scenes, mut missing_packages) = (0, 0, 0, 0);
    let mut wp_named = 0;
    for name in &names {
        let s = Scene::parse(&read(name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(s.unknown.is_empty(), "{name}");
        *versions.entry(s.version.expect(name)).or_default() += 1;
        let here = WarpPath::parse(name.strip_suffix(".xml").unwrap()).unwrap();
        assert!(has(&here.lua()), "{name}");
        objects += s.objects.len();
        animations += s.animations.len();
        anim_markers += s.animations.iter().map(|a| a.markers.len()).sum::<usize>();
        assert!(s.objects.iter().all(|o| o.markers.len() == 1), "{name}");
        for a in &s.animations {
            *fps.entry(a.fps.to_bits()).or_default() += 1;
        }
        for p in s
            .objects
            .iter()
            .map(|o| &o.name)
            .chain(s.animations.iter().map(|a| &a.name))
        {
            if !has(&here.package(p)) {
                missing_packages += 1;
            }
        }
        for e in &s.exits {
            exits += 1;
            blocks += e.blocks.len();
            cameras += usize::from(e.camera.is_some());
            assert_eq!(e.markers.len(), 1, "{name}");
            assert_eq!(e.nb_warp_block, 16, "{name}");
            assert!(e.blocks.iter().all(|b| b.face < 6 && b.x < 16 && b.y < 16));
            if e.linked_warp.ends_with(".wp") {
                wp_named += 1;
                if WarpPath::parse(&e.linked_warp).is_some_and(|w| has(&w.xml())) {
                    to_scenes += 1;
                }
            }
        }
    }
    let versions: Vec<_> = versions.into_iter().collect();
    assert_eq!(versions, [((1, 0), 283), ((1, 1), 115)]);
    assert_eq!(
        (objects, animations, anim_markers, exits, blocks, cameras),
        (484, 1_480, 75, 843, 52_261, 843)
    );
    // 37 exits name an object, not a scene; 17 name a scene that is not in
    // the install; 13 animations name a package that is not there.
    assert_eq!((wp_named, to_scenes, missing_packages), (806, 789, 13));
    let fps: Vec<(f32, usize)> = fps
        .into_iter()
        .map(|(b, n)| (f32::from_bits(b), n))
        .collect();
    assert_eq!(fps, [(0.0, 70), (15.0, 1_410)]);
}
