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
