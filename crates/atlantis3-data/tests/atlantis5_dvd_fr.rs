//! *Atlantis V*'s French DVD in each form a player has it (`ATLANTIS5_DVD_FR`:
//! the folder holding `soa-fr.iso`, `files/` the disc's files and `datas/`
//! the two cabinets extracted): the same bytes from the cabinets read in
//! place, from the image, and from the extracted tree.

use std::path::{Path, PathBuf};

use atlantis3_data::atlantis5::{Atlantis5Source, Form};
use atlantis_data::FileSource;

fn dvd() -> Option<PathBuf> {
    atlantis3_data::testing::a5_dvd_fr()
}

/// A few files of each cabinet's folders, the last ones of the big
/// folders among them (decoded from the folder's start).
const SAMPLES: [&str; 8] = [
    "datas/scenes/zeppelin.wam",
    "./datas/warps/ze0003.jp6",
    "datas/Sprites3D/(zeppelin)_(003)_(cendar)_(z1).spw",
    "datas/sounds/music/zeppelin_background.mls",
    "datas/fonts/french/font.txt",
    "datas/texts/dialog/french/dialogs.xml",
    "datas/texts/video/french/kinematic_subtitles.xml",
    "datas/scripts/on_new_game.lua",
];

fn check_against_tree(s: &Atlantis5Source, tree: &Atlantis5Source) {
    for name in SAMPLES {
        let a = s.read(name).unwrap_or_else(|| panic!("{name}: not read"));
        let b = tree
            .read(name)
            .unwrap_or_else(|| panic!("{name}: not in the tree"));
        assert!(a == b, "{name}: differs");
    }
}

fn tree(d: &Path) -> Atlantis5Source {
    let t = Atlantis5Source::open(&d.join("datas")).expect("the extracted tree");
    assert_eq!(t.form(), Form::Tree);
    t
}

#[test]
fn the_disc_folder_reads_the_cabinets_in_place() {
    let Some(d) = dvd() else { return };
    let s = Atlantis5Source::open(&d.join("files")).expect("the disc's files");
    assert_eq!(s.form(), Form::DiscFolder);
    assert_eq!(s.languages(), ["french"]);
    let build = s.build().expect("the program").expect("a known build");
    assert_eq!(build.id, "atlantis5-dvd-fr-3883008");
    check_against_tree(&s, &tree(&d));
    // Every file of the French cabinet, in its order.
    let t = tree(&d);
    let mut n = 0;
    for dir in [
        "datas/sounds/dialog/french",
        "datas/sounds/video/french",
        "datas/texts/menu/french",
    ] {
        for f in s.list(dir) {
            let name = format!("{dir}/{f}");
            assert!(s.read(&name) == t.read(&name), "{name}");
            n += 1;
        }
    }
    assert_eq!(n, 827 + 64 + 1);
}

#[test]
fn the_image_and_the_folder_holding_it() {
    let Some(d) = dvd() else { return };
    let img = Atlantis5Source::open(&d.join("soa-fr.iso")).expect("the image");
    assert_eq!(img.form(), Form::DiscImage);
    assert_eq!(img.languages(), ["french"]);
    assert_eq!(img.build().unwrap().unwrap().id, "atlantis5-dvd-fr-3883008");
    check_against_tree(&img, &tree(&d));
    let around = Atlantis5Source::open(&d).expect("the folder of the image");
    assert_eq!(around.form(), Form::DiscImage);
}

#[test]
fn the_tree_and_datas_itself() {
    let Some(d) = dvd() else { return };
    let t = tree(&d);
    assert_eq!(t.languages(), ["french"]);
    // The extracted tree has no program beside it.
    assert!(t.build().is_none());
    let inner = Atlantis5Source::open(&d.join("datas").join("Datas")).expect("Datas itself");
    assert_eq!(
        inner.read("datas/scenes/zeppelin.wam"),
        t.read("datas/scenes/zeppelin.wam")
    );
    assert_eq!(
        t.size("datas/warps/ze0003.jp6"),
        std::fs::metadata(d.join("datas/Datas/Warps/ze0003.jp6"))
            .ok()
            .map(|m| m.len())
    );
}

/// Every playlist reads (@ 0x0045b7a0's rules) to tracks whose files are in
/// the music folder (any case): `LOOP` lists among them. Two lists name a
/// file the disc lacks (`death_theme.mls`: `electric_bzz.ogg`,
/// `floor_18_typewriter.mls`: `typewriter_1.ogg`); no script plays them
/// (the second's one call is commented out).
#[test]
fn the_playlists() {
    use atlantis3_data::evolution2004_mls::Playlist;
    let Some(d) = dvd() else { return };
    let t = tree(&d);
    let names: Vec<String> = t
        .list("datas/sounds/music")
        .iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();
    let (mut lists, mut looped, mut random) = (0, 0, 0);
    for n in names.iter().filter(|n| n.ends_with(".mls")) {
        let p = Playlist::parse_atlantis5(&t.read(&format!("datas/sounds/music/{n}")).unwrap());
        assert!(!p.tracks.is_empty(), "{n}");
        for tr in &p.tracks {
            let f = String::from_utf8_lossy(&tr.file).to_ascii_lowercase();
            let lacking = matches!(
                (n.as_str(), f.as_str()),
                ("death_theme.mls", "electric_bzz.ogg")
                    | ("floor_18_typewriter.mls", "typewriter_1.ogg")
            );
            assert_eq!(names.contains(&f), !lacking, "{n}: {f}");
        }
        lists += 1;
        looped += usize::from(p.looped);
        random += usize::from(p.random);
    }
    assert_eq!((lists, looped, random), (42, 17, 4));
}

/// The three French tables read whole: 238 strings, 31 talkers and 291
/// dialogs of 827 tirades (one voice file each in `sounds/dialog/french`),
/// 14 films' 37 subtitles, each film a `.bik` of `videos/`.
#[test]
fn the_text_tables() {
    use atlantis3_data::atlantis5::text_paths;
    use atlantis3_data::atlantis5_texts::{Dialogs, KinematicSubtitles, StringResources};
    let Some(d) = dvd() else { return };
    let t = tree(&d);
    let [dialogs, strings, subtitles] = text_paths("french");
    let s = StringResources::parse(&t.read(&strings).unwrap()).unwrap();
    assert_eq!((s.language.as_str(), s.strings.len()), ("french", 238));
    // A derived value, not the text: the menu string's length in chars.
    assert_eq!(s.get("new_game").map(|v| v.chars().count()), Some(15));
    assert!(s
        .strings
        .iter()
        .any(|(_, v)| v.contains('\n') && !v.contains('\r')));
    let g = Dialogs::parse(&t.read(&dialogs).unwrap()).unwrap();
    assert_eq!((g.talkers.len(), g.dialogs.len()), (31, 291));
    let tirades: usize = g.dialogs.iter().map(|x| x.tirades.len()).sum();
    assert_eq!(tirades, 827);
    let voices = t.list("datas/sounds/dialog/french");
    assert_eq!(voices.len(), 827);
    let k = KinematicSubtitles::parse(&t.read(&subtitles).unwrap()).unwrap();
    let n: usize = k.films.iter().map(|f| f.subtitles.len()).sum();
    assert_eq!((k.films.len(), n), (14, 37));
    for f in &k.films {
        assert!(
            t.size(&format!("datas/videos/{}.bik", f.name)).is_some(),
            "{}",
            f.name
        );
        assert!(f.subtitles.iter().all(|s| s.start <= s.end));
    }
    // Every tirade's talker is a talker of the set.
    for x in &g.dialogs {
        for tr in &x.tirades {
            assert!(
                g.talkers.iter().any(|k| k.name == tr.talker),
                "{} {}",
                x.id,
                tr.talker
            );
        }
    }
}

/// The French `font.txt`: 12 fonts, the quoted faces whole.
#[test]
fn the_font_list() {
    use atlantis3_data::font_txt::FontTxt;
    let Some(d) = dvd() else { return };
    let t = tree(&d);
    let f = FontTxt::parse_atlantis5(&t.read("datas/fonts/french/font.txt").unwrap());
    assert_eq!(f.fonts.len(), 12);
    let def = f.font("AtlFont_Default").unwrap();
    assert_eq!((def.face.as_str(), def.height), ("Courier New", 16));
    assert_eq!(f.font("AtlFont_Dialog").unwrap().weight(), 700);
    // On unless the block says `Antialias false` (one font does).
    assert_eq!(f.fonts.iter().filter(|x| !x.antialias).count(), 1);
}
