//! Atlantis II's own files: detection, the discs and the `MONDE` rule, the
//! places and the start of a new game.
//!
//! Needs the game: `ATLANTIS2_DATA` = the installed folder (atlantis2.exe,
//! wam\, scenar\...), `ATLANTIS2_DISCS` = a folder holding the four CD
//! images (or copies of the CDs). Without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::con::NewGame;
use atlantis2_data::files::{holds_atlantis2, OpenError};
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
