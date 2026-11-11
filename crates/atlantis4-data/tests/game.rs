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

/// The data folders of the install (the root also holds the executable,
/// DLLs and files of the GOG.com installer and client, whose set changes
/// with the client).
const DATA_FOLDERS: [&str; 16] = [
    "2D",
    "3D",
    "Common",
    "Diary",
    "DocumentsBrowser",
    "GUI",
    "Inventory",
    "InventoryMenu",
    "Objectives",
    "Puzzles",
    "Sounds",
    "Texts",
    "Versions",
    "Videos",
    "__redist",
    "__support",
];

#[test]
fn the_data_folders_hold_every_file_of_the_inventory() {
    let Some(dir) = install() else { return };
    let mut all = Vec::new();
    for folder in DATA_FOLDERS {
        walk(&dir, &dir.join(folder), &mut all);
    }
    let bytes: u64 = all.iter().map(|f| f.1).sum();
    assert_eq!((all.len(), bytes), (9_848, 1_928_223_369));
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
        ("font", 10),
        ("jpg", 102),
        ("lua", 504),
        ("ogg", 5_303),
        ("ogv", 446),
        ("png", 433),
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

/// Every script: source text, its kind, the callbacks the scenes define,
/// and the boot variables.
#[test]
fn every_script_is_source_of_a_known_kind() {
    use atlantis4_data::lua::{self, ScriptKind};
    use atlantis4_data::WarpPath;
    use std::collections::BTreeMap;

    let Some(dir) = install() else { return };
    let src = atlantis_data::StdDir::new(&dir);
    let read = |n: &str| atlantis_data::FileSource::read(&src, n).unwrap();
    let names = files_with(&dir, "lua");
    let mut kinds = BTreeMap::<ScriptKind, usize>::new();
    let mut defined = BTreeMap::<String, usize>::new();
    let mut bytes = 0;
    let mut boot = BTreeMap::<String, String>::new();
    for name in &names {
        let s = read(name);
        bytes += s.len();
        assert!(!lua::is_bytecode(&s), "{name}");
        let k = lua::kind(name, &s);
        *kinds.entry(k).or_default() += 1;
        if k == ScriptKind::Scene {
            for f in lua::functions_defined(&s) {
                if f.starts_with("On") {
                    *defined.entry(f).or_default() += 1;
                }
            }
        }
        if k == ScriptKind::Boot {
            boot.extend(lua::string_assignments(&s));
        }
    }
    assert_eq!((names.len(), bytes), (504, 715_750));
    let kinds: Vec<_> = kinds.into_iter().collect();
    assert_eq!(
        kinds,
        [
            (ScriptKind::Scene, 398),
            (ScriptKind::GameEnter, 1),
            (ScriptKind::Boot, 4),
            (ScriptKind::Version, 3),
            (ScriptKind::Layout, 96),
            (ScriptKind::Other, 2),
        ]
    );
    let defined: Vec<(&str, usize)> = defined.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    assert_eq!(
        defined,
        [
            ("OnAnimationFinished", 46),
            ("OnAnswered", 38),
            ("OnDialogClosed", 37),
            ("OnDialogFinished", 58),
            ("OnDocumentClosed", 1),
            ("OnMovieFinished", 78),
            ("OnSoundStopped", 1),
            ("OnWarpEnter", 398),
            ("OnWarpLeave", 398),
            ("OnWarpObjectHit", 398),
        ]
    );
    let keys: Vec<&str> = boot.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "applicationTitle",
            "backupPathTemplate",
            "firstWarpPath",
            "versionString"
        ]
    );
    let first = WarpPath::parse(&boot["firstWarpPath"]).unwrap();
    assert!(atlantis_data::FileSource::size(&src, &first.xml()).is_some());
}

/// Every text table of every language, the combinations and the diary.
#[test]
fn every_text_table_reads() {
    use atlantis4_data::texts::{self, DialogTable, LANGUAGES};

    let Some(dir) = install() else { return };
    let src = atlantis_data::StdDir::new(&dir);
    let read = |n: &str| atlantis_data::FileSource::read(&src, n).unwrap_or_else(|| panic!("{n}"));
    let mut lines = Vec::new();
    for lang in LANGUAGES {
        let t = DialogTable::parse(&read(&texts::texts_path(lang))).unwrap();
        let mut names: Vec<&str> = t.lines.iter().map(|d| d.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        lines.push((lang, t.lines.len(), names.len()));
        let objects = texts::parse_labelled(&read(&texts::objects_path(lang)), "Object").unwrap();
        assert_eq!(objects.len(), 42, "{lang}");
        let docs = texts::parse_labelled(&read(&texts::documents_path(lang)), "Document").unwrap();
        assert_eq!(docs.len(), 5, "{lang}");
    }
    assert_eq!(
        lines,
        [
            ("de", 2_219, 2_218),
            ("en", 2_218, 2_217),
            ("es", 2_218, 2_217),
            ("fr", 2_217, 2_217),
            ("it", 2_218, 2_217),
        ]
    );
    let c = texts::parse_combinations(&read(texts::COMBINATIONS_PATH)).unwrap();
    let ins: usize = c.iter().map(|c| c.inputs.len()).sum();
    let outs: usize = c.iter().map(|c| c.outputs.len()).sum();
    let ids = c.iter().filter(|c| c.id.is_some()).count();
    assert_eq!((c.len(), ins, outs, ids), (125, 302, 128, 6));
    let diary = atlantis4_data::xml::parse(&read(texts::DIARY_PATH)).unwrap();
    let count = |n: &str| diary.iter().filter(|e| e.name == n).count();
    assert_eq!((count("sprite_button2"), count("value")), (6, 3));
}

/// Every `TESF` font: 256 glyphs and a PNG atlas that decodes.
#[test]
fn every_font_reads() {
    use atlantis4_data::font::Font;

    let Some(dir) = install() else { return };
    let names = files_with(&dir, "tef");
    let mut kerning = Vec::new();
    for name in &names {
        let data = std::fs::read(dir.join(name)).unwrap();
        let f = Font::parse(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(f.glyphs.len(), 256, "{name}");
        kerning.push(f.kerning.len());
    }
    assert_eq!(names.len(), 6);
    kerning.sort_unstable();
    assert_eq!(kerning, [8, 44, 128, 218, 374, 916]);
}

fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The golden `key` of `tests/golden/decoded.txt`: its count and hash.
fn golden(key: &str) -> (usize, String) {
    let text = include_str!("golden/decoded.txt");
    let line = text
        .lines()
        .find(|l| l.split(' ').next() == Some(key))
        .unwrap_or_else(|| panic!("no golden {key}"));
    let f: Vec<&str> = line.split(' ').collect();
    (f[1].parse().unwrap(), f[2].to_string())
}

/// The hash of sorted "name\tWxH\tmd5\n" lines.
fn picture_set_hash(mut lines: Vec<String>) -> (usize, String) {
    lines.sort();
    (lines.len(), md5_hex(lines.concat().as_bytes()))
}

fn picture_line(name: &str, img: &atlantis4_data::image::Image) -> String {
    format!(
        "{name}\t{}x{}\t{}\n",
        img.width,
        img.height,
        md5_hex(&img.pixels)
    )
}

/// Every PNG and TGA decodes to the reference's pixels.
#[test]
fn every_png_and_tga_decodes_exactly() {
    use atlantis4_data::image;

    let Some(dir) = install() else { return };
    let mut png = Vec::new();
    for name in files_with(&dir, "png") {
        let data = std::fs::read(dir.join(&name)).unwrap();
        let img = image::decode_png(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        png.push(picture_line(&name, &img));
    }
    assert_eq!(picture_set_hash(png), golden("png"));
    let mut tga = Vec::new();
    for name in files_with(&dir, "tga") {
        let data = std::fs::read(dir.join(&name)).unwrap();
        let img = image::decode_tga(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        tga.push(picture_line(&name, &img));
    }
    assert_eq!(picture_set_hash(tga), golden("tga"));
    // The fonts' atlases are PNG too.
    for name in files_with(&dir, "tef") {
        let data = std::fs::read(dir.join(&name)).unwrap();
        let f = atlantis4_data::font::Font::parse(&data).unwrap();
        image::decode_png(f.atlas(&data)).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// Every JPEG file and every JPEG entry of the warp packages, decoded:
/// (pictures, by channel count, set hash).
fn decode_every_jpeg(dir: &Path) -> [(usize, usize, usize, Vec<String>); 2] {
    use atlantis4_data::image;
    use atlantis4_data::wpa::Package;

    let mut out: [(usize, usize, usize, Vec<String>); 2] = Default::default();
    let mut add = |set: usize, name: &str, data: &[u8]| {
        let img = image::decode_jpeg(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let o = &mut out[set];
        o.0 += 1;
        match img.channels {
            1 => o.1 += 1,
            3 => o.2 += 1,
            c => panic!("{name}: {c} channels"),
        }
        o.3.push(picture_line(name, &img));
    };
    for name in files_with(dir, "jpg") {
        add(0, &name, &std::fs::read(dir.join(&name)).unwrap());
    }
    for name in files_with(dir, "wpa") {
        let data = std::fs::read(dir.join(&name)).unwrap();
        let p = Package::parse(&data).unwrap();
        for e in p.entries.iter().filter(|e| e.name.ends_with(".jpg")) {
            add(1, &format!("{name}:{}", e.name), p.slice(&data, e).unwrap());
        }
    }
    out
}

/// Every JPEG decodes, with the channel counts of the data.
#[test]
fn every_jpeg_decodes() {
    let Some(dir) = install() else { return };
    let [files, packages] = decode_every_jpeg(&dir);
    assert_eq!((files.0, files.1, files.2), (102, 0, 102));
    assert_eq!((packages.0, packages.1, packages.2), (11_834, 345, 11_489));
}

/// The JPEGs decode to libjpeg-turbo's pixels (`JDCT_ISLOW`, fancy
/// upsampling, the original's settings).
#[test]
fn every_jpeg_decodes_to_libjpeg_turbo_pixels() {
    let Some(dir) = install() else { return };
    let [files, packages] = decode_every_jpeg(&dir);
    assert_eq!(picture_set_hash(files.3), golden("jpeg-files"));
    assert_eq!(picture_set_hash(packages.3), golden("jpeg-packages"));
}

/// The samples per channel of all the `.ogg` files together (the sum of
/// their last pages' granule positions; about 7.2 hours at 44.1 kHz).
const FRAMES: u64 = 1_142_293_497;

/// Every Ogg Vorbis sound: its pages (CRC), one Vorbis stream, decoded
/// whole to as many samples as its last page's granule position says;
/// every WAVE file's format.
#[test]
fn every_sound_decodes() {
    use atlantis4_data::{audio, ogg};
    use std::collections::BTreeMap;

    let Some(dir) = install() else { return };
    let mut formats = BTreeMap::<(u8, u32), usize>::new();
    let mut frames = 0u64;
    let names = files_with(&dir, "ogg");
    for name in &names {
        let data = std::fs::read(dir.join(name)).unwrap();
        let (pages, packets) = ogg::read(&data, 1).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(packets.len(), 1, "{name}");
        let first = packets.values().next().unwrap();
        let ogg::StreamKind::Vorbis(info) = ogg::identify(&first[0]) else {
            panic!("{name}: not Vorbis")
        };
        *formats
            .entry((info.channels, info.sample_rate))
            .or_default() += 1;
        let d = audio::decode_vorbis(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            (d.channels, d.sample_rate),
            (info.channels, info.sample_rate)
        );
        let got = d.samples.len() as u64 / u64::from(d.channels);
        let want = pages.last().unwrap().granule;
        assert_eq!(got, want, "{name}");
        frames += got;
    }
    let formats: Vec<_> = formats.into_iter().collect();
    assert_eq!(names.len(), 5_303);
    assert_eq!(formats, [((1, 44_100), 4_255), ((2, 44_100), 1_048)]);
    assert_eq!(frames, FRAMES);
    let mut wav = BTreeMap::<(u16, u16, u32, u16), usize>::new();
    for name in files_with(&dir, "wav") {
        let data = std::fs::read(dir.join(&name)).unwrap();
        let i = audio::wav_info(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        *wav.entry((i.format, i.channels, i.sample_rate, i.bits))
            .or_default() += 1;
    }
    let wav: Vec<_> = wav.into_iter().collect();
    assert_eq!(wav, [((1, 2, 44_100, 16), 11)]);
}

/// Every Theora video: the `.ogv` files (Skeleton + Theora) and the `.ogv`
/// entries of the warp packages (Theora alone).
#[test]
fn every_video_header_reads() {
    use atlantis4_data::ogg::{self, StreamKind};
    use atlantis4_data::wpa::Package;
    use std::collections::BTreeMap;

    let Some(dir) = install() else { return };
    let mut census = BTreeMap::<(bool, u32, u32, u32, u32, u8), usize>::new();
    let mut theora = |inside: bool, name: &str, data: &[u8]| {
        let (_, packets) = ogg::read(data, 1).unwrap_or_else(|e| panic!("{name}: {e}"));
        let kinds: Vec<StreamKind> = packets.values().map(|p| ogg::identify(&p[0])).collect();
        let t = kinds
            .iter()
            .find_map(|k| match k {
                StreamKind::Theora(t) => Some(*t),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: no Theora"));
        let skeleton = kinds.contains(&StreamKind::Skeleton);
        assert_eq!(kinds.len(), 1 + usize::from(skeleton), "{name}");
        assert_eq!(skeleton, !inside, "{name}");
        assert_eq!(t.version, (3, 2, 1), "{name}");
        *census
            .entry((
                inside,
                t.picture_width,
                t.picture_height,
                t.fps.0,
                t.fps.1,
                t.pixel_format,
            ))
            .or_default() += 1;
    };
    let files = files_with(&dir, "ogv");
    for name in &files {
        theora(false, name, &std::fs::read(dir.join(name)).unwrap());
    }
    let mut entries = 0;
    for name in files_with(&dir, "wpa") {
        let data = std::fs::read(dir.join(&name)).unwrap();
        let p = Package::parse(&data).unwrap();
        for e in p.entries.iter().filter(|e| e.name.ends_with(".ogv")) {
            entries += 1;
            theora(
                true,
                &format!("{name}:{}", e.name),
                p.slice(&data, e).unwrap(),
            );
        }
    }
    assert_eq!((files.len(), entries), (446, 1_484));
    let alpha = files.iter().filter(|n| n.ends_with(".alpha.ogv")).count();
    assert_eq!(alpha, 182);
    // The files: (width, height, fps numerator, denominator) -> count.
    let outside: Vec<_> = census
        .iter()
        .filter(|(k, _)| !k.0)
        .map(|(k, n)| ((k.1, k.2, k.3, k.4), *n))
        .collect();
    assert_eq!(
        outside,
        [
            ((704, 394, 24, 1), 1),
            ((800, 448, 15, 1), 365),
            ((800, 448, 25, 1), 72),
            ((800, 600, 15, 1), 8),
        ]
    );
    // Inside the packages: 32 sizes, widths 64 to 1024 and heights 64 to
    // 256 in steps of 64, all
    // 15 frames a second; every stream is 4:2:0.
    let inside: Vec<_> = census.iter().filter(|(k, _)| k.0).collect();
    assert_eq!(inside.len(), 32);
    assert!(inside.iter().all(|(k, _)| (k.3, k.4) == (15, 1)));
    assert!(census.keys().all(|k| k.5 == 0));
}
