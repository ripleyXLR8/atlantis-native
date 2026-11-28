//! *Atlantis V*'s Italian *Atlantis Collection* DVD (2014), installed
//! (`ATLANTIS5_IT_COLLECTION`: the folder its Inno Setup installer fills
//! in Italian, `The Secrets of Atlantis.exe` beside `datas`; skipped when
//! unset): the international DVD's tree with the Italian files only, and
//! the collection's own program.

use std::path::PathBuf;

use atlantis3_data::atlantis5::{
    holds_atlantis5, language_places, text_paths, Atlantis5Source, Edition, Form, COLLECTION_BUILD,
};
use atlantis3_data::atlantis5_texts::{Dialogs, KinematicSubtitles, StringResources};
use atlantis_data::FileSource;

fn install() -> Option<PathBuf> {
    atlantis3_data::testing::a5_it_collection()
}

/// The install folder opens as a tree in Italian only, its program the
/// collection's build, which names the edition.
#[test]
fn the_install_opens_in_italian_with_its_program() {
    let Some(d) = install() else { return };
    assert!(holds_atlantis5(&d));
    let s = Atlantis5Source::open(&d).expect("the install");
    assert_eq!(s.form(), Form::Tree);
    assert_eq!(s.languages(), ["italian"]);
    // One language: played whatever the system's.
    assert_eq!(s.language(None), "italian");
    assert_eq!(s.language(Some("en-GB")), "italian");
    let build = s.build().expect("the program").expect("a known build");
    assert_eq!(build.id, COLLECTION_BUILD);
    assert_eq!(build.size, 2_822_144);
    assert_eq!(build.pe_timestamp, 0x453f_5758);
    assert_eq!(
        build.label,
        "The Secrets of Atlantis (2006), Atlantis Collection"
    );
    assert_eq!(s.edition(), Edition::ItalianCollection);
    // One language: no place per language.
    assert!(language_places(&d).is_empty());
    // The shared files and the Italian ones; no other language's.
    assert!(s.size("datas/scenes/zeppelin.wam").is_some());
    assert!(s.size("./datas/warps/ze0003.jp6").is_some());
    assert!(s.size("datas/videos/Logo_AIE.bik").is_some());
    assert!(s.size("datas/fonts/italian/font.txt").is_some());
    assert_eq!(s.list("datas/sounds/dialog/italian").len(), 827);
    assert_eq!(s.list("datas/sounds/video/italian").len(), 64);
    for l in ["english", "german", "spanish"] {
        assert!(
            s.size(&format!("datas/fonts/{l}/font.txt")).is_none(),
            "{l}"
        );
    }
    // `datas` itself: the same data, no program beside them.
    let t = Atlantis5Source::open(&d.join("datas")).expect("datas itself");
    assert_eq!(t.languages(), ["italian"]);
    assert!(t.build().is_none());
    assert_eq!(t.edition(), Edition::Dvd);
}

/// The Italian tables read whole (the international DVD's counts: 238
/// strings, 31 talkers, 291 dialogs of 827 tirades, 14 films' subtitles,
/// 12 fonts) and, with the international DVD too
/// (`ATLANTIS5_DVD_INT`), byte for byte its Italian cabinet's.
#[test]
fn the_italian_tables() {
    use atlantis3_data::font_txt::FontTxt;
    let Some(d) = install() else { return };
    let s = Atlantis5Source::open(&d).unwrap();
    let [dialogs, strings, subtitles] = text_paths("italian");
    let r = StringResources::parse(&s.read(&strings).unwrap()).unwrap();
    assert_eq!(r.strings.len(), 238);
    assert!(r.get("new_game").is_some_and(|v| !v.is_empty()));
    let g = Dialogs::parse(&s.read(&dialogs).unwrap()).unwrap();
    assert_eq!((g.talkers.len(), g.dialogs.len()), (31, 291));
    let tirades: usize = g.dialogs.iter().map(|x| x.tirades.len()).sum();
    assert_eq!(tirades, 827);
    let k = KinematicSubtitles::parse(&s.read(&subtitles).unwrap()).unwrap();
    assert_eq!(k.films.len(), 14);
    let font = "datas/fonts/italian/font.txt";
    assert_eq!(
        FontTxt::parse_atlantis5(&s.read(font).unwrap()).fonts.len(),
        12
    );
    let Some(int) = atlantis3_data::testing::a5_dvd_int() else {
        return;
    };
    let dvd = Atlantis5Source::open(&int.join("datas_italian.cab")).expect("the Italian place");
    for p in [dialogs, strings, subtitles, font.to_string()] {
        assert_eq!(s.read(&p), dvd.read(&p), "{p}");
    }
}
