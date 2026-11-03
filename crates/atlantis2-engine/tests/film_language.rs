//! The bug fix `fix_film_language` on the real data: which file each edition
//! plays for films 19, 55 and 66, with the fix and without, and that each
//! language's track fits its film (its music at the same samples as the
//! English track's).

use std::path::PathBuf;

use atlantis2_data::files::{A2Files, Folder};
use atlantis2_engine::film_tracks::{self, TRACKS};
use atlantis2_engine::{BugFix, BugFixes, Engine, Fpu, Variant};
use atlantis_codecs::decode_apc;

fn folder(var: &str) -> Option<PathBuf> {
    let dir = std::env::var_os(var);
    if dir.is_none() {
        atlantis2_data::testing::missing(&format!("{var} not set"));
    }
    dir.map(PathBuf::from)
}

fn one_folder(var: &str) -> Option<A2Files> {
    Some(A2Files::open_dir(&folder(var)?, &[]).expect(var).0)
}

fn cd(install: &str, discs: &str) -> Option<A2Files> {
    let (install, discs) = (folder(install)?, folder(discs)?);
    Some(A2Files::open_dir(&install, &[discs]).expect("CD").0)
}

/// The samples the engine loads for film `film`.
fn played(files: A2Files, film: u32, fixes: BugFixes) -> Vec<i16> {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.enhancements.fixes = fixes;
    e.run_command(b"(monde=1)");
    e.run_command(format!("(cinema={film})").as_bytes());
    let name = format!("cine{film:03}.apc");
    let pcm = e.sound_table_pcm(&name);
    pcm.unwrap_or_else(|| panic!("{name}: {:?}", e.sound_table()))
        .samples
        .to_vec()
}

fn samples(data: &[u8]) -> Vec<i16> {
    decode_apc(data).unwrap().samples
}

const FILMS: [u32; 3] = [19, 55, 66];

/// The GOG.com languages other than English and the Italian collection: the
/// `dial\` track with the fix, the `apc\` one without; the `apc\` track is
/// another file. One fix alone is enough, none is the original.
#[test]
fn a_language_with_its_track_unused_plays_it() {
    for var in [
        "ATLANTIS2_GOG",
        "ATLANTIS2_GOG_DE",
        "ATLANTIS2_GOG_ES",
        "ATLANTIS2_IT_COLLECTION",
    ] {
        let Some(files) = one_folder(var) else {
            continue;
        };
        for film in FILMS {
            let name = format!("cine{film:03}.apc");
            let original = files.read(Folder::Apc, &name).unwrap();
            let own = files.read(Folder::ApcDial, &name).unwrap();
            assert_ne!(original, own, "{var} {name}");
            assert_eq!(film_tracks::own_track(&files, &name).as_ref(), Some(&own));
            let open = || one_folder(var).unwrap();
            assert_eq!(
                played(open(), film, BugFixes::only(BugFix::FilmLanguage)),
                samples(&own),
                "{var} {name}"
            );
            assert_eq!(
                played(open(), film, BugFixes::ALL),
                samples(&own),
                "{var} {name}"
            );
            assert_eq!(
                played(open(), film, BugFixes::NONE),
                samples(&original),
                "{var} {name}"
            );
        }
    }
}

/// GOG.com's English: the two folders hold one file, so the fix plays what
/// the original does.
#[test]
fn gog_english_changes_nothing() {
    let Some(files) = one_folder("ATLANTIS2_GOG_EN") else {
        return;
    };
    for film in FILMS {
        let name = format!("cine{film:03}.apc");
        assert_eq!(
            files.read(Folder::Apc, &name),
            files.read(Folder::ApcDial, &name),
            "{name}"
        );
        let open = || one_folder("ATLANTIS2_GOG_EN").unwrap();
        assert_eq!(
            played(open(), film, BugFixes::ALL),
            played(open(), film, BugFixes::NONE),
            "{name}"
        );
    }
}

/// The CDs (French, with or without 1.01, whose patch changes no disc, and
/// North American): no film track in `dial\` on any disc, so the fix finds
/// none and the film plays the disc's track.
#[test]
fn the_cds_change_nothing() {
    for (install, discs) in [
        ("ATLANTIS2_DATA", "ATLANTIS2_DISCS"),
        ("ATLANTIS2_DATA_US", "ATLANTIS2_DISCS_US"),
    ] {
        let Some(files) = cd(install, discs) else {
            continue;
        };
        for name in TRACKS {
            for disc in 1..=4 {
                assert_eq!(files.size_on_disc(disc, Folder::ApcDial, name), None);
            }
            assert_eq!(film_tracks::own_track(&files, name), None, "{name}");
        }
        let original = files.read_current(Folder::Apc, "cine019.apc").unwrap();
        assert_eq!(
            played(files, 19, BugFixes::ALL),
            samples(&original),
            "{install}"
        );
    }
}

/// `x` from `s` seconds to `e` (stereo, 22050 Hz).
fn part(x: &[i16], s: usize, e: usize) -> &[i16] {
    &x[s * 44100..e * 44100]
}

/// The level of `y` against `x`, and how far `y` is from `x` at that level
/// (0 for one recording, sample for sample).
fn level_and_apart(x: &[i16], y: &[i16]) -> (f64, f64) {
    let (mut xx, mut xy) = (0.0, 0.0);
    for (&a, &b) in x.iter().zip(y) {
        xx += f64::from(a) * f64::from(a);
        xy += f64::from(a) * f64::from(b);
    }
    let gain = xy / xx;
    let (mut diff, mut level) = (0.0, 0.0);
    for (&a, &b) in x.iter().zip(y) {
        diff += (gain * f64::from(a) - f64::from(b)).abs();
        level += (gain * f64::from(a)).abs();
    }
    (gain, diff / level)
}

/// Each language's track against GOG's English one: the same rate, channels
/// and length, and in the part of the film where no one speaks the same
/// music at the same samples, at the level given (the Spanish film 55's
/// whole sound is twice as loud). Decoded windows of the voices aside, this
/// is what fits a track to its film.
#[test]
fn every_track_fits_its_film() {
    // The film, a part without voices (seconds), and the music's level.
    const QUIET: [(u32, usize, usize); 3] = [(19, 65, 105), (55, 80, 100), (66, 30, 120)];
    let Some(en) = one_folder("ATLANTIS2_GOG_EN") else {
        return;
    };
    for var in [
        "ATLANTIS2_GOG",
        "ATLANTIS2_GOG_DE",
        "ATLANTIS2_GOG_ES",
        "ATLANTIS2_IT_COLLECTION",
    ] {
        let Some(files) = one_folder(var) else {
            continue;
        };
        for (film, from, to) in QUIET {
            let name = format!("cine{film:03}.apc");
            let reference = decode_apc(&en.read(Folder::Apc, &name).unwrap()).unwrap();
            let own = decode_apc(&files.read(Folder::ApcDial, &name).unwrap()).unwrap();
            assert_eq!(own.sample_rate, reference.sample_rate, "{var} {name}");
            assert_eq!(own.channels, reference.channels, "{var} {name}");
            assert_eq!(own.samples.len(), reference.samples.len(), "{var} {name}");
            let (gain, apart) = level_and_apart(
                part(&reference.samples, from, to),
                part(&own.samples, from, to),
            );
            eprintln!("{var} {name}: level {gain:.3}, apart {apart:.3}");
            assert!(apart < 0.1, "{var} {name}: {apart}");
            let expected = if var.ends_with("_ES") && film == 55 {
                2.0
            } else {
                1.0
            };
            assert!((gain - expected).abs() < 0.08, "{var} {name}: {gain}");
        }
    }
}
