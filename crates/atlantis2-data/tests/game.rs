//! Atlantis II's own files: detection, the discs and the `MONDE` rule, the
//! places and the start of a new game.
//!
//! Needs the game: `ATLANTIS2_DATA` = the installed folder (atlantis2.exe,
//! wam\, scenar\...), `ATLANTIS2_DISCS` = a folder holding the four CD
//! images (or copies of the CDs). Without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::con::NewGame;
use atlantis2_data::db::Scene;
use atlantis2_data::dialog::{parse_com, parse_dial, parse_info, FontTxt, LineKind, World};
use atlantis2_data::files::{holds_atlantis2, OpenError};
use atlantis2_data::tga::Tga;
use atlantis2_data::wam::{pick, Wam};
use atlantis2_data::{identify, A2Files, Folder};
use md5::{Digest, Md5};

fn install() -> Option<PathBuf> {
    let dir = std::env::var_os("ATLANTIS2_DATA");
    if dir.is_none() {
        eprintln!("ATLANTIS2_DATA not set: test skipped");
    }
    dir.map(PathBuf::from)
}

fn game() -> Option<A2Files> {
    let dir = install()?;
    let Some(discs) = std::env::var_os("ATLANTIS2_DISCS") else {
        eprintln!("ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, build) = A2Files::open_dir(&dir, &[PathBuf::from(discs)]).expect("game folder");
    assert_eq!(build.id, "atlantis2-cd-fr-2199552");
    Some(files)
}

fn hex(d: &[u8]) -> String {
    Md5::digest(d).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn detects_the_french_cd_build() {
    let Some(dir) = install() else { return };
    assert!(holds_atlantis2(&dir));
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    let build = identify(&exe).unwrap();
    assert_eq!(build.id, "atlantis2-cd-fr-2199552");
    assert_eq!(exe.len(), 2_199_552);
    // The version string the game logs is in the executable.
    assert!(exe
        .windows(build.version.len())
        .any(|w| w == build.version.as_bytes()));
    // The install folder alone has no disc.
    assert!(matches!(
        A2Files::open_dir(&dir, &[]),
        Err(OpenError::NoDisc)
    ));
}

#[test]
fn discs_and_the_monde_rule() {
    let Some(mut files) = game() else { return };
    assert_eq!(files.disc_numbers(), vec![1, 2, 3, 4]);
    // Start: the first disc.
    assert_eq!(files.current_disc(), 1);
    // hnm/intro00.hnm differs between disc 1 and the others.
    let size = |f: &A2Files| f.size(Folder::Hnm, "intro00.hnm");
    assert_eq!(size(&files), Some(4_398_680));
    files.set_monde(2);
    assert_eq!(size(&files), Some(3_959_816));
    // spw/sna1013013.spw: discs 2 and 4 only, different copies.
    assert_eq!(files.size(Folder::Spw, "SNA1013013.SPW"), Some(11_178));
    files.set_monde(6);
    assert_eq!(files.current_disc(), 4);
    assert_eq!(files.size(Folder::Spw, "sna1013013.spw"), Some(1_323));
    // Tibet: whichever disc is in, here the first.
    files.set_monde(5);
    assert_eq!(files.current_disc(), 1);
    // The first view's panorama is the same on every disc.
    let md5s: Vec<String> = (1..=4)
        .map(|n| hex(&files.read_from_disc(n, Folder::Warp, "ti0002.hnm").unwrap()))
        .collect();
    assert!(md5s.iter().all(|m| *m == md5s[0]));
    // A file of disc 3 only, read while disc 1 is current.
    let (_, disc) = files.read_with_disc(Folder::Spf, "bousier.spr").unwrap();
    assert_eq!(disc, 3);
}

#[test]
fn every_place_file() {
    let Some(dir) = install() else { return };
    let mut names: Vec<_> = std::fs::read_dir(dir.join("wam"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    names.sort();
    assert_eq!(names.len(), 25);
    let (mut views, mut dests, mut spf, mut zones, mut points) = (0, 0, 0, 0, 0);
    let (mut sounds, mut links, mut ambient, mut entities, mut states) = (0, 0, 0, 0, 0);
    let mut kinds = std::collections::BTreeMap::new();
    for p in &names {
        let w = Wam::parse(&std::fs::read(p).unwrap()).unwrap();
        assert_eq!(w.skipped, 0, "{}", p.display());
        assert_eq!(w.places.len(), 1, "{}", p.display());
        for place in &w.places {
            links += place.links.len();
            for v in &place.views {
                views += 1;
                dests += v.destinations.len();
                spf += v
                    .destinations
                    .iter()
                    .map(|d| d.sprites.len())
                    .sum::<usize>();
                zones += v.zones.len();
                points += v.zones.iter().map(|z| z.points.len()).sum::<usize>();
                sounds += v.sounds.len();
                for z in &v.zones {
                    *kinds.entry(z.kind).or_insert(0) += 1;
                }
            }
        }
        ambient += w.ambient_objects.len();
        entities += w.entities.len();
        states += w.entities.iter().map(|e| e.states.len()).sum::<usize>();
    }
    assert_eq!(
        (views, dests, spf, zones, points),
        (515, 1274, 139, 1531, 6248)
    );
    assert_eq!(
        (sounds, links, ambient, entities, states),
        (194, 30, 44, 283, 1419)
    );
    let kinds: Vec<(i32, i32)> = kinds.into_iter().collect();
    assert_eq!(
        kinds,
        vec![
            (1, 1276),
            (2, 28),
            (3, 84),
            (4, 48),
            (10, 1),
            (50, 89),
            (1000, 5)
        ]
    );
}

#[test]
fn tibet_and_the_new_game() {
    let Some(dir) = install() else { return };
    let data = std::fs::read(dir.join("wam/tibet.wam")).unwrap();
    assert_eq!(hex(&data), "ccd1609d6ef1e337c3d3304293824a64");
    let w = Wam::parse(&data).unwrap();
    assert_eq!(w.version, 208);
    assert_eq!(w.prefixes[0], "ti");
    let views = &w.places[0].views;
    let ids: Vec<u32> = views.iter().map(|v| v.id).collect();
    assert_eq!(ids, (2..=10).collect::<Vec<_>>());
    assert_eq!(views[0].name, "ti0002.hnm");
    let count =
        |f: &dyn Fn(&atlantis2_data::wam::View) -> usize| views.iter().map(f).sum::<usize>();
    assert_eq!(count(&|v| v.destinations.len()), 16);
    assert_eq!(count(&|v| v.zones.len()), 24);
    assert_eq!(count(&|v| v.zones.iter().map(|z| z.points.len()).sum()), 99);
    assert_eq!(w.entities.len(), 12);
    assert_eq!(w.entities.iter().map(|e| e.states.len()).sum::<usize>(), 49);
    // 2 -> 3 turns to yaw 270° and arrives so.
    let d = &w.view(2).unwrap().destinations[0];
    assert_eq!((d.target, d.angles_as_given), (3, 1));
    assert!((d.start_yaw - 3.0 * std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    let con = std::fs::read(dir.join("scenar/chapi000.con")).unwrap();
    let text: String = con.iter().map(|&b| char::from(b)).collect();
    assert_eq!(
        NewGame::from_chapter0(&text),
        Some(NewGame {
            monde: 5,
            wam: "tibet.wam".to_string(),
            view: 2,
            yaw_deg: -90,
            pitch_deg: 0,
        })
    );
}

/// The picks of the original (golden) with the closed form of the camera:
/// d = Ry(90° − yaw) · Rx(−pitch) · ((x − 320) / f, (y − 240) / f, 1).
#[test]
fn zone_picks_match_the_original() {
    let Some(dir) = install() else { return };
    let w = Wam::parse(&std::fs::read(dir.join("wam/tibet.wam")).unwrap()).unwrap();
    let golden = include_str!("golden/wam_pick_tibet.txt");
    let (mut picks, mut bad, mut aims) = (0, Vec::new(), 0);
    for line in golden.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f[0] == "aim" {
            let v = w.view(f[1].parse().unwrap()).unwrap();
            let (yaw, pitch) = v.zones[f[2].parse::<usize>().unwrap()].aim();
            let (gy, gp): (f64, f64) = (f[3].parse().unwrap(), f[4].parse().unwrap());
            assert!((yaw.to_degrees() - gy).abs() < 6e-4, "{line}");
            assert!((pitch.to_degrees() - gp).abs() < 6e-4, "{line}");
            aims += 1;
            continue;
        }
        let n: Vec<f64> = f.iter().map(|s| s.parse().unwrap()).collect();
        let (yaw, pitch) = (n[1].to_radians(), n[2].to_radians());
        let foc = 409.5813;
        let (vx, vy, vz) = ((n[3] - 320.0) / foc, (n[4] - 240.0) / foc, 1.0);
        let (cp, sp) = ((-pitch).cos(), (-pitch).sin());
        let (vy, vz) = (cp * vy - sp * vz, sp * vy + cp * vz);
        let a = std::f64::consts::FRAC_PI_2 - yaw;
        let d = [
            a.cos() * vx + a.sin() * vz,
            vy,
            -a.sin() * vx + a.cos() * vz,
        ];
        let got = pick(&w.view(n[0] as u32).unwrap().zones, d).map_or(-1, |i| i as i64);
        if got != n[5] as i64 {
            bad.push(line.to_string());
        }
        picks += 1;
    }
    assert_eq!((picks, aims), (10908, 24));
    assert!(
        bad.is_empty(),
        "{} picks differ: {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

/// The `.db` scenes of the four discs (disc, path under `datas_a2\3d\`, size).
const DB_FILES: &[(u8, &str, u64)] = &[
    (1, "Dial/Ais1.db", 174542),
    (1, "Dial/des1.DB", 153093),
    (1, "Dial/dis1.db", 213276),
    (1, "Dial/Fis1.db", 232311),
    (1, "Dial/h1s1.db", 379722),
    (1, "Dial/H5s1.db", 351447),
    (1, "Dial/Lis1.db", 202553),
    (1, "Dial/Nus1.db", 205503),
    (1, "Dial/pos1.db", 288771),
    (1, "Dial/Q1s1.db", 135306),
    (1, "Dial/shs1.db", 222700),
    (2, "dial/cos1.DB", 198822),
    (2, "dial/frs1.DB", 165819),
    (2, "dial/GPS1.DB", 229498),
    (2, "dial/h2s1.DB", 237020),
    (2, "dial/H5s1.db", 351447),
    (2, "dial/prs1.DB", 335574),
    (2, "dial/q2s1.DB", 243654),
    (2, "dial/ROS1.DB", 180854),
    (2, "dial/shs1.DB", 222700),
    (3, "Dial/ABS1.DB", 180983),
    (3, "Dial/Als1.db", 306293),
    (3, "Dial/H4S1.DB", 277025),
    (3, "Dial/H4S2.DB", 273400),
    (3, "Dial/H5s1.db", 351447),
    (3, "Dial/M1s1.db", 358907),
    (3, "Dial/M2s1.db", 332056),
    (3, "Dial/mds1.DB", 319251),
    (3, "Dial/mds2.DB", 319172),
    (3, "Dial/shs1.DB", 222700),
    (4, "dial/Ais1.db", 174542),
    (4, "dial/Als1.db", 306293),
    (4, "dial/dis1.db", 213276),
    (4, "dial/frs1.DB", 165819),
    (4, "dial/h2s1.DB", 237020),
    (4, "dial/H5s1.db", 351447),
    (4, "dial/H6S1.DB", 349965),
    (4, "dial/pos1.db", 288771),
    (4, "dial/Rhs1.db", 227601),
    (4, "dial/shs1.DB", 222700),
    (4, "pz_araig/1ich.DB", 5451),
    (4, "pz_araig/1kcr.DB", 4206),
    (4, "pz_araig/2isa.DB", 5386),
    (4, "pz_araig/ARDmar.DB", 97266),
    (4, "pz_araig/ARmar.DB", 95442),
    (4, "pz_araig/ARmen.DB", 124550),
    (4, "pz_araig/ARstby.DB", 125066),
    (4, "pz_araig/ARtoil.DB", 135786),
    (4, "pz_araig/bo01.DB", 1546),
    (4, "pz_araig/bouclier.db", 1546),
    (4, "pz_araig/chenille.db", 5451),
    (4, "pz_araig/crane.db", 4205),
    (4, "pz_araig/epee.db", 2957),
    (4, "pz_araig/err_epee.db", 2957),
    (4, "pz_araig/fema.DB", 2957),
    (4, "pz_araig/macahuitl.db", 3949),
    (4, "pz_araig/PARmar.DB", 67328),
    (4, "pz_araig/PARstby.DB", 49932),
    (4, "pz_araig/plume.db", 980),
    (4, "pz_araig/pmpl.DB", 980),
    (4, "pz_araig/sauterelle.db", 7391),
    (4, "pz_pirog/a1.DB", 74091),
    (4, "pz_pirog/b1.DB", 62911),
    (4, "pz_pirog/c1.DB", 71027),
    (4, "pz_pirog/d1.DB", 53923),
    (4, "pz_pirog/e1.DB", 72691),
    (4, "pz_pirog/f1.DB", 66515),
    (4, "pz_pirog/g1.DB", 68075),
    (4, "pz_pirog/h1.DB", 64691),
    (4, "pz_pirog/prbc.db", 35914),
    (4, "pz_pirog/prbf.db", 35914),
    (4, "pz_pirog/prjn.db", 35917),
    (4, "pz_pirog/prog.db", 35920),
    (4, "pz_pirog/prrg.db", 35917),
    (4, "pz_pirog/prve.db", 35917),
    (4, "pz_pirog/prvi.db", 35926),
];
/// The texture maps of the heads (disc, path under `datas_a2\3d\`).
const HEAD_MAPS: &[(u8, &str)] = &[
    (1, "Dial/aimap.tga"),
    (1, "Dial/aimapyf.tga"),
    (1, "Dial/aimapymf.tga"),
    (1, "Dial/deess512.tga"),
    (1, "Dial/deess512yf.tga"),
    (1, "Dial/deess512ymf.tga"),
    (1, "Dial/dimap.tga"),
    (1, "Dial/dimapyf.tga"),
    (1, "Dial/dimapymf.tga"),
    (1, "Dial/finmap.tga"),
    (1, "Dial/finmapyf.tga"),
    (1, "Dial/finmapymf.tga"),
    (1, "Dial/h5map.tga"),
    (1, "Dial/h5mapyf.tga"),
    (1, "Dial/h5mapymf.tga"),
    (1, "Dial/herot512.TGA"),
    (1, "Dial/herot512ymf.tga"),
    (1, "Dial/herot512yf.tga"),
    (1, "Dial/limap.tga"),
    (1, "Dial/limapyf.tga"),
    (1, "Dial/limapymf.tga"),
    (1, "Dial/numap.tga"),
    (1, "Dial/numapyf.tga"),
    (1, "Dial/numapymf.tga"),
    (1, "Dial/pomap.tga"),
    (1, "Dial/pomapyf.tga"),
    (1, "Dial/pomapymf.tga"),
    (1, "Dial/q1map.tga"),
    (1, "Dial/q1mapyf.tga"),
    (1, "Dial/q1mapymf.tga"),
    (1, "Dial/shamap.tga"),
    (1, "Dial/shamapyf.tga"),
    (1, "Dial/shamapymf.tga"),
    (2, "dial/camerampetite.tga"),
    (2, "dial/cameramapyf.tga"),
    (2, "dial/cameramap.tga"),
    (2, "dial/cameramapymf.tga"),
    (2, "dial/camerampetiteyf.tga"),
    (2, "dial/camerampetiteymf.tga"),
    (2, "dial/comap.tga"),
    (2, "dial/CoMAPYF.TGA"),
    (2, "dial/CoMAPYMF.tga"),
    (2, "dial/gpmap.tga"),
    (2, "dial/gpmapyf.tga"),
    (2, "dial/gpmapymf.tga"),
    (2, "dial/h2map.tga"),
    (2, "dial/H2MAPYF.TGA"),
    (2, "dial/h2mapymf.tga"),
    (2, "dial/h5map.tga"),
    (2, "dial/h5mapyf.tga"),
    (2, "dial/h5mapymf.tga"),
    (2, "dial/prmap.tga"),
    (2, "dial/PrMAPYF.TGA"),
    (2, "dial/prmapymf.tga"),
    (2, "dial/romap.TGA"),
    (2, "dial/romapyf.tga"),
    (2, "dial/romapymf.tga"),
    (2, "dial/shamap.tga"),
    (2, "dial/shamapyf.tga"),
    (2, "dial/shamapymf.tga"),
    (3, "Dial/abbe512.tga"),
    (3, "Dial/abbe512yf.tga"),
    (3, "Dial/abbe512ymf.tga"),
    (3, "Dial/almap.tga"),
    (3, "Dial/almapyf.tga"),
    (3, "Dial/almapymf.tga"),
    (3, "Dial/h5map.tga"),
    (3, "Dial/h5mapyf.tga"),
    (3, "Dial/h5mapymf.tga"),
    (3, "Dial/herch512.tga"),
    (3, "Dial/herch512yf.tga"),
    (3, "Dial/herch512ymf.tga"),
    (3, "Dial/m1map.tga"),
    (3, "Dial/m1mapyf.tga"),
    (3, "Dial/m1mapymf.tga"),
    (3, "Dial/m2map.tga"),
    (3, "Dial/m2mapyf.tga"),
    (3, "Dial/m2mapymf.tga"),
    (3, "Dial/mdmap1.tga"),
    (3, "Dial/mdmap1yf.tga"),
    (3, "Dial/mdmap1ymf.tga"),
    (3, "Dial/mdmap2.tga"),
    (3, "Dial/mdmap2yf.tga"),
    (3, "Dial/mdmap2ymf.tga"),
    (3, "Dial/shamap.tga"),
    (3, "Dial/shamapyf.tga"),
    (3, "Dial/shamapymf.tga"),
    (4, "dial/aimap.tga"),
    (4, "dial/aimapyf.tga"),
    (4, "dial/aimapymf.tga"),
    (4, "dial/almap.tga"),
    (4, "dial/almapyf.tga"),
    (4, "dial/almapymf.tga"),
    (4, "dial/camerampetite.tga"),
    (4, "dial/camerampetiteyf.tga"),
    (4, "dial/camerampetiteymf.tga"),
    (4, "dial/dimap.tga"),
    (4, "dial/dimapyf.tga"),
    (4, "dial/dimapymf.tga"),
    (4, "dial/h2map.tga"),
    (4, "dial/H2MAPYF.TGA"),
    (4, "dial/h2mapymf.tga"),
    (4, "dial/h5map.tga"),
    (4, "dial/h5mapyf.tga"),
    (4, "dial/h5mapymf.tga"),
    (4, "dial/h6map.tga"),
    (4, "dial/h6mapyf.tga"),
    (4, "dial/h6mapymf.tga"),
    (4, "dial/pomap.tga"),
    (4, "dial/pomapyf.tga"),
    (4, "dial/pomapymf.tga"),
    (4, "dial/rhmap.tga"),
    (4, "dial/rhmapyf.tga"),
    (4, "dial/rhmapymf.tga"),
    (4, "dial/shamap.tga"),
    (4, "dial/shamapyf.tga"),
    (4, "dial/shamapymf.tga"),
];

/// Every `.db` scene of the four discs is read to its last byte: 28
/// dialog heads and the two 3D puzzles' objects.
#[test]
fn db_files_parse_to_their_end() {
    let Some(files) = game() else { return };
    let mut heads = std::collections::BTreeSet::new();
    for &(disc, path, size) in DB_FILES {
        let data = files
            .read_from_disc(disc, Folder::ThreeD, path)
            .unwrap_or_else(|| panic!("disc {disc} {path}"));
        assert_eq!(data.len() as u64, size, "{path}");
        let (scene, used) = Scene::parse_len(&data).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(used, data.len(), "disc {disc} {path}");
        assert!([2, 7, 10, 11].contains(&scene.version), "{path}");
        if path.to_ascii_lowercase().starts_with("dial/") {
            heads.insert(path.to_ascii_lowercase());
            // One mesh with a texture, the mouth shapes as morph keys of
            // the node `morphing`, each with the mesh's vertex count, and
            // 1 to 4 cameras.
            assert_eq!(scene.meshes.len(), 1, "{path}");
            let m = &scene.meshes[0];
            assert_eq!(m.vertices.len(), m.vertex_count as usize, "{path}");
            assert_eq!(m.corners.len(), 3 * m.face_count as usize, "{path}");
            assert_eq!(m.uvs[0].len(), m.corners.len(), "{path}");
            let morph = scene.node("morphing").expect(path);
            assert!(!morph.morph_keys.is_empty(), "{path}");
            for (name, verts) in &morph.morph_keys {
                assert_eq!(verts.len(), m.vertices.len(), "{path} {name}");
            }
            assert!((1..=4).contains(&scene.cameras.len()), "{path}");
        }
    }
    // 28 heads (some on several discs) and 36 puzzle objects.
    assert_eq!(heads.len(), 28);
    assert_eq!(DB_FILES.len(), 76);
}

/// Every head map decodes: 512 x 512 for the maps, 512 x H for the
/// eyes-closed (`yf`) and half-closed (`ymf`) ones.
#[test]
fn head_maps_decode() {
    let Some(files) = game() else { return };
    for &(disc, path) in HEAD_MAPS {
        let data = files.read_from_disc(disc, Folder::ThreeD, path).unwrap();
        let t = Tga::parse(&data).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(t.width, 512, "{path}");
        let low = path.to_ascii_lowercase();
        if low.ends_with("yf.tga") || low.ends_with("ymf.tga") {
            assert!((140..=292).contains(&t.height), "{path} {}", t.height);
        } else {
            assert_eq!(t.height, 512, "{path}");
        }
        assert_eq!(t.pixels.len(), t.width * t.height);
        // RGB565 from 1-5-5-5: green's low bit is always 0.
        assert!(t.pixels.iter().all(|p| p & 0x20 == 0), "{path}");
    }
}

/// The dialog scripts of the five worlds: every line's speaker has a
/// block, and a speaker with a `[Matrice]` has its 70 numbers.
#[test]
fn dialog_scripts() {
    let Some(files) = game() else { return };
    let (mut lines, mut persos, mut heads) = (0, 0, 0);
    for world in ["CH", "IR", "SH", "TI", "YU"] {
        let info = parse_info(
            &files
                .read(Folder::Scenar, &format!("info{world}.txt"))
                .unwrap(),
        );
        let com = parse_com(
            &files
                .read(Folder::Scenar, &format!("com{world}.txt"))
                .unwrap(),
        );
        for l in &com {
            assert!((1..=5).contains(&l.mood), "{world} {}", l.id);
            assert!((1..=8).contains(&l.stand), "{world} {}", l.id);
            if l.kind == LineKind::Head {
                assert!(
                    info.iter().any(|p| p.name == l.speaker),
                    "{world} {} {}",
                    l.id,
                    l.speaker
                );
            }
        }
        lines += com.len();
        persos += info.len();
        heads += info.iter().filter(|p| p.matrix.is_some()).count();
    }
    assert_eq!((lines, persos), (1214, 56));
    assert!(heads > 0);
}

/// The texts of `loc\Dial*.txt`: 1214 lines and 358
/// dialogs, the same ids in the same order as `com*.txt`, texts of at most
/// 327 bytes (342 with the id and the speaker); the worlds' file sets;
/// `Font.txt`.
#[test]
fn dialog_texts_and_fonts() {
    let Some(files) = game() else { return };
    let (mut lines, mut dialogs, mut longest) = (0, 0, 0);
    for monde in 1..=5 {
        let world = World::of_monde(monde).unwrap();
        let [dial, com, _] = world.files();
        let d = parse_dial(&files.read(Folder::Loc, &dial).unwrap());
        let c = parse_com(&files.read(Folder::Scenar, &com).unwrap());
        assert_eq!(d.len(), c.len(), "{dial}");
        for (a, b) in d.iter().zip(&c) {
            assert_eq!(a.id, b.id, "{dial}");
            assert!(a.numbers().is_some(), "{dial} {}", a.id);
            assert!(!a.text.contains(&b'\r'), "{dial} {}", a.id);
            longest = longest.max(a.text.len());
        }
        let ids: std::collections::BTreeSet<u32> =
            d.iter().filter_map(|l| l.numbers()).map(|n| n.0).collect();
        lines += d.len();
        dialogs += ids.len();
    }
    assert_eq!((lines, dialogs, longest), (1214, 358, 327));
    let f = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(f.breaks, [0x20]);
    assert_eq!(f.fonts.len(), 4);
    let d = f.font("AtlFT_Dialog").unwrap();
    assert_eq!((d.file.as_str(), d.height), ("Atl_Font.ttf", 12));
}
