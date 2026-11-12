//! Each language of the installed game: the language tag chosen as the
//! original does (`lang.ini`, then the texts' check), a new game's boot,
//! first node and intro sound, a narration's text and voice, and a menu
//! picture, all from that language's folders.
//!
//! The game-data tests need `ATLANTIS4_DATA` (see `atlantis4_data::testing`)
//! and skip cleanly with `ATLANTIS4_SKIP_DATA_TESTS=1`. No game text is
//! written here: lines are named by their ids.

use std::cell::RefCell;
use std::rc::Rc;

use atlantis4_data::texts::{texts_path, DialogTable};
use atlantis4_data::A4Files;
use atlantis4_engine::Game;
use atlantis4_script::{new_game, read_boot_vars, BootValue, EngineRef, Options};

fn files() -> Option<A4Files> {
    let dir = atlantis4_data::testing::install_dir()?;
    Some(A4Files::open_dir(&dir).unwrap().0)
}

/// A game in `lang`, booted, at `node` (the first scene when `None`).
fn boot(files: &A4Files, lang: &str, node: Option<&str>) -> Rc<RefCell<Game>> {
    let mut g = Game::new(files.clone(), 768);
    assert_eq!(g.set_language(Some(lang)), lang);
    g.clock_us = Some(Box::new(|| 5_000_000));
    g.update(5_000.0);
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    let first = match (node, boot.first_warp_path) {
        (Some(n), _) => n.as_bytes().to_vec(),
        (None, Some(BootValue::Str(f))) => f,
        _ => panic!("no firstWarpPath"),
    };
    new_game(&engine, Options::default(), &first).unwrap();
    game
}

#[test]
fn the_install_has_the_five_gog_languages() {
    let Some(files) = files() else { return };
    assert_eq!(files.languages(), ["en", "fr", "de", "es", "it"]);
    // The install's own: its `lang.ini`, which has texts.
    let ini = files.lang_ini().expect("lang.ini");
    assert_eq!(files.language(), ini);
    // A language without texts (`ru`, in the executable's list) falls
    // back to English.
    assert_eq!(files.choose_language(Some("ru")), "en");
}

#[test]
fn each_language_boots_with_its_intro_sound() {
    let Some(files) = files() else { return };
    for lang in files.languages() {
        let game = boot(&files, lang, None);
        let g = game.borrow();
        assert_eq!(g.lang, lang);
        assert_eq!(
            g.warp.as_ref().unwrap().node.path.stem(),
            "3D/Bateau_CA/0001",
            "{lang}"
        );
        assert_eq!(g.movie.as_ref().unwrap().path, "Videos/intro.ogv");
        assert_eq!(g.sounds.video.path(), "Videos/intro.ogg");
        let sound = format!("Videos/{lang}/intro.ogg");
        assert_eq!(g.sounds.video.resolved(), sound);
        assert!(files.exists(&sound), "{sound}");
        assert!(g.sounds.video.is_playing(), "{lang}");
    }
}

#[test]
fn each_language_narrates_with_its_text_and_voice() {
    let Some(files) = files() else { return };
    let mut texts = Vec::new();
    for lang in files.languages() {
        // `3D/Centre17_C1/0001.wp`'s first entry narrates 22-1..5.
        let game = boot(&files, lang, Some("3D/Centre17_C1/0001.wp"));
        let g = game.borrow();
        let line = g.dialogs.current.as_ref().expect("a narration");
        assert_eq!(line.id, "22-1");
        let table = DialogTable::parse(&files.read(&texts_path(lang)).unwrap()).unwrap();
        assert!(
            table.get("22-1").is_some_and(|d| !d.text.is_empty()),
            "{lang}"
        );
        assert!(!line.text.is_empty(), "{lang}");
        texts.push(line.text.clone());
        let voice = format!("Sounds/Dialogs/{lang}/{}", line.voice);
        assert!(files.exists(&voice), "{voice}");
        assert_eq!(
            g.sounds.voice.resolved().to_ascii_lowercase(),
            voice.to_ascii_lowercase()
        );
        assert!(g.sounds.voice.is_playing(), "{lang}");
    }
    // Five languages, five texts.
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), files.languages().len());
}

#[test]
fn each_language_has_its_menu_pictures() {
    let Some(files) = files() else { return };
    for lang in files.languages() {
        files.choose_language(Some(lang));
        assert_eq!(
            atlantis4_engine::gui::resolve(&files, "2D/Menus/MainMenu/NewGame.png").as_deref(),
            Some(format!("2D/Menus/MainMenu/HD/{lang}/NewGame.png").as_str())
        );
        assert!(files.exists(&atlantis4_data::texts::objects_path(lang)));
        assert!(files.exists(&atlantis4_data::texts::documents_path(lang)));
    }
}
