//! Every file of the installed game's `Datas\` read by this crate (and the
//! shared readers of `atlantis-codecs` and `atlantis2-data`), with the
//! counts the original's readers give.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::{
    atl, cfg, cine_txt, con, coord, db, dialog, font_txt, hnm_header, objects, set, vue, zbf,
};
use atlantis_data::{FileSource, StdDir};

fn install() -> Option<PathBuf> {
    atlantis3_data::testing::install_dir()
}

/// Every file under `dir`, as paths relative to `root` with `/`.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy();
            out.push(rel.replace('\\', "/"));
        }
    }
}

/// The files of `Datas\` (relative to the install), by lower-case
/// extension.
fn datas_files(install: &Path) -> BTreeMap<String, Vec<String>> {
    let mut all = Vec::new();
    walk(install, &install.join("Datas"), &mut all);
    let mut by = BTreeMap::<String, Vec<String>>::new();
    for f in all {
        let ext = f.rsplit_once('.').map_or("", |x| x.1).to_ascii_lowercase();
        by.entry(ext).or_default().push(f);
    }
    by
}

fn read(src: &StdDir, name: &str) -> Vec<u8> {
    src.read(name).unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn the_executable_and_the_sprite_libraries_are_known() {
    let Some(dir) = install() else { return };
    let (_, build) = atlantis3_data::A3Files::open_dir(&dir).unwrap();
    assert_eq!(build.id, "atlantis3-gog-fr-3473408");
    for (name, want) in [("spr_p5.dll", "spr_p5.dll"), ("spr_p6.dll", "spr_p6.dll")] {
        let dll = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(
            atlantis3_data::exe::identify_spr_dll(&dll).map(|b| b.name),
            Some(want)
        );
    }
    assert!(atlantis3_data::files::holds_atlantis3(&dir));
}

#[test]
fn every_file_of_datas_is_of_a_known_kind() {
    let Some(dir) = install() else { return };
    let counts: Vec<(String, usize)> = datas_files(&dir)
        .into_iter()
        .map(|(k, v)| (k, v.len()))
        .collect();
    let want = [
        ("apc", 711),
        ("atl", 13),
        ("cfg", 1),
        ("con", 57),
        ("db", 31),
        ("hnm", 402),
        // scenar\vssver.scc, a Visual SourceSafe leftover the game never
        // reads.
        ("scc", 1),
        ("set", 1),
        ("spr", 432),
        ("spw", 1530),
        ("tga", 66),
        ("ttf", 1),
        ("txt", 35),
        ("vue", 22),
        ("wam", 11),
        ("zbf", 3),
    ];
    let want: Vec<(String, usize)> = want.iter().map(|(k, n)| (k.to_string(), *n)).collect();
    assert_eq!(counts, want);
}

#[test]
fn every_video_decodes_and_the_cinematics_carry_their_sound() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let (mut frames, mut sound, mut tags) = (0u64, 0, BTreeMap::new());
    for name in &files["hnm"] {
        let data = read(&src, name);
        let h = hnm_header::HnmHeader::parse(&data).unwrap();
        assert_eq!(h.file_size as usize, data.len(), "{name}");
        assert_eq!(h.bpp, 16, "{name}");
        assert_eq!(
            h.frame_size,
            u32::from(h.width) * u32::from(h.height) * 2,
            "{name}"
        );
        *tags.entry(h.tag_text().map(str::to_string)).or_insert(0) += 1;
        if h.has_sound() {
            sound += 1;
            let chunk = hnm_header::first_sound_chunk(&data).expect(name);
            let f = hnm_header::SoundFormat::from_apc_header(chunk).expect(name);
            assert_eq!((f.sample_rate, f.stereo), (44_100, true), "{name}");
        } else {
            assert!(hnm_header::first_sound_chunk(&data).is_none(), "{name}");
        }
        let mut v = atlantis_codecs::HnmVideo::new(&data[..]).unwrap();
        assert_eq!(v.frame_count(), h.frame_count(), "{name}");
        while let Some(f) = v.next_frame() {
            f.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            frames += 1;
        }
        assert_eq!(v.frames_decoded(), h.frame_count(), "{name}");
    }
    assert_eq!(frames, 45_335);
    assert_eq!(sound, 76);
    let tags: Vec<(Option<String>, i32)> = tags.into_iter().collect();
    assert_eq!(
        tags,
        [
            (None, 283),
            (Some("V109".to_string()), 72),
            (Some("V110".to_string()), 47)
        ]
    );
}

#[test]
fn every_sound_decodes() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    for name in &files["apc"] {
        let a = atlantis_codecs::decode_apc(&read(&src, name))
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(!a.samples.is_empty(), "{name}");
    }
    assert_eq!(files["apc"].len(), 711);
}

#[test]
fn every_place_file_and_picture_reads() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let mut views = 0;
    for name in &files["wam"] {
        let w = atlantis2_data::Wam::parse(&read(&src, name)).unwrap();
        assert_eq!((w.version, w.skipped), (208, 0), "{name}");
        views += w.places.iter().map(|p| p.views.len()).sum::<usize>();
    }
    assert_eq!(views, 257);
    for name in &files["tga"] {
        atlantis2_data::tga::Tga::parse(&read(&src, name)).unwrap();
    }
}

#[test]
fn every_scene_reads_to_its_last_byte() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let (mut lights, mut ambient) = (0, 0);
    for name in &files["db"] {
        let data = read(&src, name);
        let (scene, len) = db::parse_len(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(len, data.len(), "{name}");
        assert_eq!(scene.version, 11, "{name}");
        // Atlantis II's reader stops in the lights.
        assert!(atlantis2_data::db::Scene::parse(&data).is_err(), "{name}");
        for l in &scene.lights {
            let f = db::light_fields(l).unwrap();
            assert_eq!(f.colours[0], f.colours[1]);
            assert_eq!(f.colours[0], f.colours[2]);
            assert_eq!(f.unknown, [0; 3]);
            ambient += usize::from(l.name == "AMBIENT");
        }
        lights += scene.lights.len();
    }
    assert_eq!((lights, ambient), (33, 31));
}

/// Every sprite of every bank and every face of every place sprite draws
/// (the pixels are checked against the original in `tests/sprites.rs`).
#[test]
fn every_sprite_and_face_draws() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let (mut sprites, mut empty) = (0, 0);
    let mut px16 = vec![0u16; 1024 * 768];
    let mut px32 = vec![0u32; 1024 * 768];
    for name in &files["spr"] {
        let data = read(&src, name);
        let b16 = Bank::parse(&data, Depth::Rgb565).unwrap();
        let b32 = Bank::parse(&data, Depth::Xrgb8888).unwrap();
        for i in 0..b16.len() {
            let Some(info) = b16.info(i) else {
                empty += 1;
                continue;
            };
            sprites += 1;
            let (x, y) = (Some(i32::from(info.xoff)), i32::from(info.yoff));
            assert!(b16.draw(&mut Target::screen(&mut px16, 1024, 768), x, y, i));
            assert!(b32.draw(&mut Target::screen(&mut px32, 1024, 768), x, y, i));
        }
    }
    assert_eq!((files["spr"].len(), sprites, empty), (432, 3848, 963));
    let (mut frames, mut faces) = (0, 0);
    let mut f16 = vec![0u16; 512 * 512];
    for name in &files["spw"] {
        let b = Bank::parse(&read(&src, name), Depth::Rgb565).unwrap();
        assert!(b.is_spw());
        for frame in 0..b.len() {
            frames += 1;
            for face in 0..6 {
                if b.face_info(frame, face).is_some() {
                    faces += 1;
                    assert!(b.draw_face(&mut Target::screen(&mut f16, 512, 512), frame, face));
                }
            }
        }
    }
    assert_eq!((files["spw"].len(), frames, faces), (1530, 67_723, 83_145));
}

#[test]
fn the_scripts_and_their_keys() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let names = atlantis2_data::DefNames::parse(&read(&src, "Datas/scenar/defnames.con")).unwrap();
    let lens: Vec<usize> = (0..5).map(|s| names.len(s)).collect();
    assert_eq!(lens, [65, 301, 57, 135, 132]);
    let (mut inits, mut fininits, mut cons, mut sets, mut sections) = (0, 0, 0, 0, 0);
    let mut ops = BTreeMap::new();
    let mut set_keys = BTreeSet::new();
    let mut chapters = 0;
    for name in &files["con"] {
        if name.ends_with("defnames.con") {
            continue;
        }
        chapters += 1;
        let s = con::Script::scan(&read(&src, name));
        assert!(s.other_directives.is_empty(), "{name}");
        inits += s.inits;
        fininits += s.fininits;
        cons += s.cons.len();
        sections += s.sections.len();
        sets += s.sets.len();
        for c in s.cons.iter().flatten() {
            *ops.entry(char::from(c.op)).or_insert(0) += 1;
        }
        // The command of a `/set`: its first key (the others are its
        // arguments, `fps` of `cinema`...).
        for (k, _) in s.sets.iter().filter_map(|s| s.pairs.first()) {
            set_keys.insert(con::fold_digits(k));
        }
    }
    assert_eq!(chapters, 56);
    assert_eq!((inits, fininits, cons, sets), (102, 102, 706, 3089));
    assert_eq!(sections, 803);
    let ops: Vec<(char, i32)> = ops.into_iter().collect();
    assert_eq!(ops, [('!', 8), ('=', 1145), ('>', 1)]);
    assert_eq!(set_keys.len(), 54, "{set_keys:?}");
}

#[test]
fn the_dialog_texts() {
    let Some(dir) = install() else { return };
    let src = StdDir::new(&dir);
    let persos = dialog::parse_info(&read(&src, dialog::INFO_PATH)).unwrap();
    assert_eq!(persos.len(), 31);
    assert_eq!(
        persos
            .iter()
            .map(|p| p.tab_weight_lines.len())
            .sum::<usize>(),
        252
    );
    assert_eq!(persos.iter().map(|p| p.id_texts.len()).sum::<usize>(), 35);
    let com = dialog::parse_com(&read(&src, dialog::COM_PATH));
    assert_eq!(com.len(), 619);
    let dial = dialog::parse_dial(&read(&src, &dialog::dial_path("fra")));
    assert_eq!((dial.lines.len(), dial.rejected), (619, 0));
    let script = dialog::DialogScript::build(persos, &com, &dial);
    assert_eq!(script.entries.len(), 619);
    assert!(script.entries.values().all(|e| e.text.is_some()));
    assert!(script.issues.is_empty(), "{:?}", script.issues);

    let o = objects::ObjectNames::parse(&read(&src, &objects::objects_path("fra"))).unwrap();
    assert_eq!(o.entries.len(), 64);
    let e = objects::ErrorMessages::parse(&read(&src, &objects::errors_path("fra"))).unwrap();
    let numbers: Vec<i32> = e.entries.iter().map(|x| x.0).collect();
    assert_eq!(numbers, (1..=12).collect::<Vec<_>>());

    let mut cues = 0;
    let mut cue_files = 0;
    for n in 0..1000 {
        let (_, txt) = cine_txt::cinema_names(n);
        let Some(data) = src.read(&cine_txt::cue_path(&txt)) else {
            continue;
        };
        cue_files += 1;
        let f = cine_txt::CueFile::parse(&data);
        assert_eq!(f.bad_markers, 0);
        for c in &f.cues {
            assert!(script.entry(c.dialog, c.line).is_some());
        }
        cues += f.cues.len();
    }
    assert_eq!((cue_files, cues), (12, 89));
    let captions: Vec<(i32, i32)> = ["Cine001.txt", "Cine093.txt"]
        .iter()
        .map(|n| {
            let c =
                cine_txt::Caption::parse(&read(&src, &cine_txt::caption_path("fra", n))).unwrap();
            (c.fade_in_frame, c.fade_out_frame)
        })
        .collect();
    assert_eq!(captions, [(50, 150), (105, 150)]);

    let fonts = font_txt::FontTxt::parse(&read(&src, font_txt::FONT_TXT_PATH));
    let heights: Vec<(String, i32)> = fonts
        .fonts
        .iter()
        .map(|f| (f.id.clone(), f.height))
        .collect();
    assert_eq!(heights.len(), 5);
    assert_eq!(
        fonts
            .fonts
            .iter()
            .map(|f| f.scaled_height(640))
            .collect::<Vec<_>>(),
        [Some(18), Some(17), Some(21), Some(53), Some(16)]
    );
}

#[test]
fn the_menus_settings_and_save_thumbnails() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let mut elements = 0;
    let mut banks = BTreeSet::new();
    for name in &files["atl"] {
        let l = atl::Layout::parse(&read(&src, name), (800, 600));
        assert!(l.errors.is_empty(), "{name}: {:?}", l.errors);
        elements += l.elements.len();
        for e in &l.elements {
            if let Some(spr) = &e.nomspr {
                banks.insert(spr.clone());
                for w in [640, 800, 1024] {
                    let p = atl::menu_sprite_path(w, spr);
                    assert!(src.size(&p).is_some(), "{p}");
                }
            }
        }
    }
    assert_eq!(elements, 177);
    let mut tips = 0;
    for name in files["txt"].iter().filter(|f| f.contains("/i_")) {
        tips += atl::parse_tooltips(&read(&src, name)).len();
    }
    assert_eq!(tips, 70);

    let c = cfg::Config::parse(&read(&src, cfg::CFG_PATH));
    assert_eq!(c.sous_titre, b"fra");
    assert_eq!(c.resolution, (800, 600));
    assert_eq!(c.warp_res, 512);

    for w in [640, 800, 1024] {
        let p = format!("Datas/menu/spr_{w}/32bits/save/gamex_x.sav.spr");
        let b = Bank::parse(&read(&src, &p), Depth::Rgb565).unwrap();
        assert_eq!((b.flags, b.len()), (0xa0, 1));
        let i = b.info(0).unwrap();
        assert_eq!((i.w, i.h), (85, 65));
    }
}

#[test]
fn the_places_sound_positions_travel_cameras_and_depth_buffers() {
    let Some(dir) = install() else { return };
    let files = datas_files(&dir);
    let src = StdDir::new(&dir);
    let mut warps = BTreeMap::new();
    for name in files["txt"].iter().filter(|f| f.contains("scenar/Coord")) {
        let c = coord::Coord::parse(&read(&src, name)).unwrap();
        assert_eq!(c.bad_warp_lines, 0);
        warps.insert(name.rsplit('/').next().unwrap().to_string(), c.warps.len());
    }
    assert_eq!(warps.values().sum::<usize>(), 212);
    assert_eq!(warps.len(), 7);

    let travels: BTreeSet<String> = files["hnm"]
        .iter()
        .filter(|f| f.contains("/hnmtraj/"))
        .map(|f| {
            f.rsplit('/')
                .next()
                .unwrap()
                .to_ascii_lowercase()
                .replace(".hnm", "")
        })
        .collect();
    let mut stems = BTreeSet::new();
    for name in &files["vue"] {
        let v = vue::Vue::parse(&read(&src, name)).unwrap();
        assert_eq!(v.cameras.len(), 2, "{name}");
        let stem = name.rsplit('/').next().unwrap().to_ascii_lowercase();
        stems.insert(stem.replace(".vue", ""));
    }
    assert_eq!(stems, travels);

    let mut sizes = Vec::new();
    for w in [640, 800, 1024] {
        let name = format!("Datas/map/{}", zbf::Zbf::file_name(w).unwrap());
        let z = zbf::Zbf::parse(&read(&src, &name)).unwrap();
        sizes.push((z.header.width, z.header.height));
        assert_eq!(z.depth.len() as u32, z.header.width * z.header.height);
    }
    assert_eq!(sizes, [(640, 480), (800, 600), (1024, 768)]);

    for name in &files["set"] {
        set::SetFile::parse(&read(&src, name)).unwrap();
    }
}
