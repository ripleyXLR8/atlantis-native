//! *Atlantis V*'s North-American CDs (`ATLANTIS5_CD_US`: a folder holding
//! a copy of each disc's files in `CD1`, `CD2` and `CD3`; skipped when
//! unset): the three discs read in place (their InstallShield set), the
//! program, the one language, its text tables, one disc alone, and an
//! installed tree of the edition.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis3_data::atlantis5::{
    holds_atlantis5, language_places, text_paths, Atlantis5Source, Edition, Form, OpenError,
    CD_EXE, CD_LOGO,
};
use atlantis3_data::atlantis5_texts::{Dialogs, KinematicSubtitles, StringResources};
use atlantis_data::{FileSource, StdDir};

fn cds() -> Option<PathBuf> {
    atlantis3_data::testing::a5_cd_us()
}

fn scratch(name: &str) -> PathBuf {
    let base =
        std::env::var_os("ATLANTIS_NATIVE_DATA").map_or_else(std::env::temp_dir, PathBuf::from);
    let d = base.join(format!("a5-cd-us-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn check_the_game(s: &Atlantis5Source) {
    assert_eq!(s.edition(), Edition::NorthAmericanCds);
    assert_eq!(s.languages(), ["english"]);
    assert_eq!(s.language(Some("fr-FR")), "english");
    assert!(s.size("datas/scenes/zeppelin.wam").is_some());
    assert!(s.size("./datas/warps/ze0003.jp6").is_some());
    assert!(s.size(CD_LOGO).is_some());
    assert_eq!(s.list("datas/sounds/dialog/english").len(), 827);
    assert_eq!(s.list("datas/sounds/video/english").len(), 64);
}

/// The folder holding the three discs' folders, and each disc's folder
/// (the others found beside it): the set read in place, the program on
/// the first disc identified.
#[test]
fn the_three_discs_open_in_english_with_their_program() {
    let Some(d) = cds() else { return };
    for p in [d.clone(), d.join("CD1"), d.join("CD2"), d.join("CD3")] {
        let s = Atlantis5Source::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert_eq!(s.form(), Form::DiscFolder, "{}", p.display());
        check_the_game(&s);
        let build = s.build().expect("the program").expect("a known build");
        assert_eq!(build.id, "atlantis5-cd-us-7368704");
        assert_eq!(
            build.label,
            "The Secrets of Atlantis (2007), North-American CD"
        );
        assert!(holds_atlantis5(&p));
        // One language: no place per language.
        assert!(language_places(&p).is_empty());
    }
}

/// The English tables: those of the international DVD's English cabinet
/// (238 strings, 31 talkers, 291 dialogs of 827 tirades, 14 films'
/// subtitles).
#[test]
fn the_english_text_tables() {
    let Some(d) = cds() else { return };
    let s = Atlantis5Source::open(&d).unwrap();
    let [dialogs, strings, subtitles] = text_paths("english");
    let r = StringResources::parse(&s.read(&strings).unwrap()).unwrap();
    assert_eq!(r.strings.len(), 238);
    let g = Dialogs::parse(&s.read(&dialogs).unwrap()).unwrap();
    assert_eq!((g.talkers.len(), g.dialogs.len()), (31, 291));
    let tirades: usize = g.dialogs.iter().map(|x| x.tirades.len()).sum();
    assert_eq!(tirades, 827);
    let k = KinematicSubtitles::parse(&s.read(&subtitles).unwrap()).unwrap();
    assert_eq!(k.films.len(), 14);
}

/// With the international DVD too (`ATLANTIS5_DVD_INT`): every file of
/// its shared and English cabinets is in the CDs' set with its size, and
/// the MD5 the set's descriptor gives is the DVD file's (a sample of
/// them decoded; the descriptors' MD5 checked, not the CDs' bytes, which
/// a damaged copy of a disc would fail); the CDs add only The Adventure
/// Company's logo.
#[test]
fn the_data_are_the_international_dvd_s_english() {
    let Some(d) = cds() else { return };
    let Some(int) = atlantis3_data::testing::a5_dvd_int() else {
        return;
    };
    use md5::{Digest, Md5};
    let us = Atlantis5Source::open(&d).unwrap();
    let set = us.installshield().expect("the CDs' set").clone();
    let dvd = Atlantis5Source::open(&int.join("datas_english.cab")).unwrap();
    // A sample over every folder (the whole would decode 2 GB twice).
    let mut dirs = vec!["datas".to_string()];
    let (mut files, mut compared) = (0, 0);
    while let Some(dir) = dirs.pop() {
        let mut names = dvd.list(&dir);
        names.sort();
        for (i, n) in names.iter().enumerate() {
            let p = format!("{dir}/{n}");
            if dvd.has_dir(&p) {
                if !["german", "italian", "spanish"].contains(&n.as_str()) {
                    dirs.push(p);
                }
                continue;
            }
            files += 1;
            assert_eq!(us.size(&p), dvd.size(&p), "{p}");
            let small = dvd.size(&p).unwrap() < 200_000;
            if i % 25 == 0 && small {
                let k = set.file_index(&p).expect("in the set");
                let md5: [u8; 16] = Md5::digest(dvd.read(&p).unwrap()).into();
                assert!(set.cabinet().files()[k].md5 == md5, "{p}");
                compared += 1;
            }
        }
    }
    assert_eq!(files, 5129);
    assert!(compared > 100);
    assert!(dvd.size(CD_LOGO).is_none());
}

/// One disc alone (its files in a folder of their own, no other disc
/// beside it): the game's, the other discs' volumes named.
#[cfg(unix)]
#[test]
fn one_disc_alone_names_the_missing_ones() {
    let Some(d) = cds() else { return };
    let root = scratch("one-disc");
    let disc = root.join("drive");
    std::fs::create_dir_all(disc.join("bin")).unwrap();
    for n in ["data1.hdr", "data1.cab", "data2.cab"] {
        std::os::unix::fs::symlink(d.join("CD1").join(n), disc.join(n)).unwrap();
    }
    std::os::unix::fs::symlink(d.join("CD1").join(CD_EXE), disc.join(CD_EXE)).unwrap();
    match Atlantis5Source::open(&disc) {
        Err(OpenError::MissingDiscs(v)) => assert_eq!(v, ["data3.cab", "data4.cab"]),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("opened without discs 2 and 3"),
    }
    assert!(holds_atlantis5(&disc));
    // The second disc beside it: still the third missing.
    std::fs::create_dir_all(root.join("disc2")).unwrap();
    std::os::unix::fs::symlink(d.join("CD2/data3.cab"), root.join("disc2/data3.cab")).unwrap();
    assert!(matches!(
        Atlantis5Source::open(&disc),
        Err(OpenError::MissingDiscs(v)) if v == ["data4.cab"]
    ));
    let _ = std::fs::remove_dir_all(&root);
}

/// An installed tree of the edition (the set's `datas` files written to
/// a folder; a part of them): the tree form, the edition from its logo.
#[test]
fn an_installed_tree_is_the_edition() {
    let Some(d) = cds() else { return };
    let s = Atlantis5Source::open(&d).unwrap();
    let root = scratch("installed");
    let copy = |p: &str| {
        let out = root.join(p);
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(&out, s.read(p).expect(p)).unwrap();
    };
    for n in s.list("datas/scenes") {
        copy(&format!("datas/Scenes/{n}"));
    }
    for n in s.list("datas/texts/menu/english") {
        copy(&format!("datas/Texts/Menu/english/{n}"));
    }
    copy("datas/Warps/ze0003.jp6");
    copy("datas/Videos/Logo_TAC.bik");
    let t = Atlantis5Source::open(&root).expect("the tree");
    assert_eq!(t.form(), Form::Tree);
    assert_eq!(t.edition(), Edition::NorthAmericanCds);
    assert_eq!(t.languages(), ["english"]);
    assert!(t.build().is_none());
    // Without the logo: the DVD's tree.
    std::fs::remove_file(root.join("datas/Videos/Logo_TAC.bik")).unwrap();
    assert_eq!(
        Atlantis5Source::open(&root).unwrap().edition(),
        Edition::Dvd
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The discs read through any file source (the web build's picked
/// files): the folder holding the three discs' folders.
#[test]
fn the_discs_from_a_file_source() {
    let Some(d) = cds() else { return };
    let dir: Arc<dyn FileSource> = Arc::new(StdDir::new(&d));
    let s = Atlantis5Source::from_folder(dir).expect("the discs");
    check_the_game(&s);
    let _ = Path::new("");
}
