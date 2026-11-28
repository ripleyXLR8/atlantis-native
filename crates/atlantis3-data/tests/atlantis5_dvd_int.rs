//! *Atlantis V*'s international DVD (`ATLANTIS5_DVD_INT`: a copy of the
//! pressed disc's files, `datas.cab` and `datas_<language>.cab` in lower
//! case beside its program; skipped when unset): its four languages, its
//! program, each language's text tables, and a place per language.

use std::path::PathBuf;

use atlantis3_data::atlantis5::{language_places, text_paths, Atlantis5Source, Form, OpenError};
use atlantis3_data::atlantis5_texts::{Dialogs, KinematicSubtitles, StringResources};
use atlantis_data::FileSource;

const LANGUAGES: [&str; 4] = ["english", "german", "italian", "spanish"];

fn dvd() -> Option<PathBuf> {
    atlantis3_data::testing::a5_dvd_int()
}

#[test]
fn the_disc_opens_with_its_four_languages_and_its_program() {
    let Some(d) = dvd() else { return };
    let s = Atlantis5Source::open(&d).expect("the disc's files");
    assert_eq!(s.form(), Form::DiscFolder);
    assert_eq!(s.languages(), LANGUAGES);
    assert_eq!(s.chosen_language(), None);
    let build = s.build().expect("the program").expect("a known build");
    assert_eq!(build.id, "atlantis5-dvd-int-3883008");
    assert_eq!(
        build.label,
        "The Secrets of Atlantis (2006), international DVD"
    );
    // The shared cabinet's files and each language's.
    assert!(s.size("datas/scenes/zeppelin.wam").is_some());
    assert!(s.size("./datas/warps/ze0003.jp6").is_some());
    for l in LANGUAGES {
        assert!(
            s.size(&format!("datas/fonts/{l}/font.txt")).is_some(),
            "{l}"
        );
        assert_eq!(
            s.list(&format!("datas/sounds/dialog/{l}")).len(),
            827,
            "{l}"
        );
        assert_eq!(s.list(&format!("datas/sounds/video/{l}")).len(), 64, "{l}");
    }
}

/// A place per language, in the installer's order (English, Spanish,
/// Italian, German), each the language's cabinet on the disc and opened in
/// that language.
#[test]
fn a_place_per_language() {
    let Some(d) = dvd() else { return };
    let places = language_places(&d);
    let names: Vec<String> = places
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "datas_english.cab",
            "datas_spanish.cab",
            "datas_italian.cab",
            "datas_german.cab"
        ]
    );
    for (p, l) in places
        .iter()
        .zip(["english", "spanish", "italian", "german"])
    {
        assert!(p.is_file(), "{}", p.display());
        let s = Atlantis5Source::open(p).expect("a language's place");
        assert_eq!(s.chosen_language(), Some(l));
        assert_eq!(s.language(Some("fr")), l);
        assert!(language_places(p).is_empty());
    }
    assert!(matches!(
        Atlantis5Source::open(&d.join("DATAS_french.cab")),
        Err(OpenError::NotThisLanguage(l)) if l == "french"
    ));
    // The root itself: the system's language when the disc has it, else
    // English.
    let s = Atlantis5Source::open(&d).unwrap();
    assert_eq!(s.language(Some("de-DE")), "german");
    assert_eq!(s.language(Some("fr-FR")), "english");
}

/// Each language's three tables read whole: 238 strings, 31 talkers and
/// 291 dialogs of 827 tirades (a voice file each), the films' subtitles
/// (14 films) each naming a film of `videos/`; the font list
/// the French one's twelve fonts.
#[test]
fn the_text_tables_of_each_language() {
    use atlantis3_data::font_txt::FontTxt;
    let Some(d) = dvd() else { return };
    let s = Atlantis5Source::open(&d).unwrap();
    for l in LANGUAGES {
        let [dialogs, strings, subtitles] = text_paths(l);
        let r = StringResources::parse(&s.read(&strings).unwrap()).unwrap();
        assert_eq!(r.strings.len(), 238, "{l}");
        assert!(r.get("new_game").is_some_and(|v| !v.is_empty()), "{l}");
        let g = Dialogs::parse(&s.read(&dialogs).unwrap()).unwrap();
        assert_eq!((g.talkers.len(), g.dialogs.len()), (31, 291), "{l}");
        let tirades: usize = g.dialogs.iter().map(|x| x.tirades.len()).sum();
        assert_eq!(tirades, 827, "{l}");
        for x in &g.dialogs {
            for tr in &x.tirades {
                assert!(
                    g.talkers.iter().any(|k| k.name == tr.talker),
                    "{l} {}",
                    x.id
                );
            }
        }
        let k = KinematicSubtitles::parse(&s.read(&subtitles).unwrap()).unwrap();
        assert_eq!(k.films.len(), 14, "{l}");
        for f in &k.films {
            assert!(
                s.size(&format!("datas/videos/{}.bik", f.name)).is_some(),
                "{l} {}",
                f.name
            );
            assert!(f.subtitles.iter().all(|x| x.start <= x.end), "{l}");
        }
        let fonts =
            FontTxt::parse_atlantis5(&s.read(&format!("datas/fonts/{l}/font.txt")).unwrap());
        assert_eq!(fonts.fonts.len(), 12, "{l}");
    }
}

/// The disc's texts hold only characters of Windows-1252, the code page
/// the game's fonts are drawn in (German ß and umlauts, Spanish ¡ ¿ ñ,
/// Italian grave accents, the typographic apostrophe and ellipsis).
#[test]
fn every_character_is_in_windows_1252() {
    let Some(d) = dvd() else { return };
    let s = Atlantis5Source::open(&d).unwrap();
    let extra = ['\u{2019}', '\u{2026}'];
    for l in LANGUAGES {
        let mut seen = std::collections::BTreeSet::new();
        for p in text_paths(l) {
            let text = String::from_utf8(s.read(&p).unwrap()).expect("UTF-8");
            seen.extend(text.chars().filter(|c| !c.is_ascii()));
        }
        for c in &seen {
            let u = *c as u32;
            assert!(
                (0xa0..0x100).contains(&u) || extra.contains(c),
                "{l}: U+{u:04X}"
            );
        }
        if l == "german" {
            assert!(seen.contains(&'\u{df}') && seen.contains(&'\u{fc}'), "ß, ü");
        }
        if l == "spanish" {
            assert!(seen.contains(&'\u{bf}') && seen.contains(&'\u{f1}'), "¿, ñ");
        }
    }
}
