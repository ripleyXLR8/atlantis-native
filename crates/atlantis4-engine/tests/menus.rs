//! The front end against the installed game: the splashes, the main
//! menu's layout and handlers, the planks' transition, the options'
//! volume rows and check boxes, the credits and the load screen
//! (atlantis4-gog-en-2363910's handlers, see `atlantis4_engine::menus`).
//! The rectangles follow from the layout formulas (`updateSize`,
//! `updatePosition`, emulated) and the pictures' sizes.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder (see
//! `atlantis4_data::testing`); skipped cleanly with
//! `ATLANTIS4_SKIP_DATA_TESTS=1`.

use atlantis4_data::A4Files;
use atlantis4_engine::gui::State;
use atlantis4_engine::menus::{Front, FrontAction, Screen};
use atlantis4_engine::options::Settings;
use atlantis4_engine::saves::{SaveStore, StubSaves};

const W: u32 = 1365;
const H: u32 = 768;

fn front() -> Option<Front> {
    let dir = atlantis4_data::testing::install_dir()?;
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    Some(Front::new(&files, Settings::default(), "1.0.1".into()))
}

/// A click at the centre of `name`'s rectangle at `now` ms.
fn click(f: &mut Front, name: &str, now: f64, saves: &mut dyn SaveStore) {
    let r = f.rect_of(name, W, H).unwrap_or_else(|| panic!("no {name}"));
    let (x, y) = (r.left + r.width / 2.0, r.top + r.height / 2.0);
    f.press(W, H, x, y);
    f.release(W, H, x, y, now, saves);
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn two_splashes_of_1_5_s_then_the_main_menu_and_its_music() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    f.start(0.0, &saves);
    assert_eq!(f.screen, Screen::Splash);
    // Microids.png (1024 × 768) letterboxed in the 1.01-window button.
    let r = f.rect_of("splash", W, H).unwrap();
    assert!(close(r.width, f64::from(1.01f32 * 1365.0)));
    f.tick(1499.0, &mut saves);
    assert_eq!(f.screen, Screen::Splash);
    // A click does not end a splash (no handler).
    f.press(W, H, 600.0, 400.0);
    f.release(W, H, 600.0, 400.0, 1499.5, &mut saves);
    assert_eq!(f.screen, Screen::Splash);
    f.tick(1500.0, &mut saves);
    assert_eq!(f.screen, Screen::Splash);
    f.tick(2999.0, &mut saves);
    assert_eq!(f.screen, Screen::Splash);
    f.tick(3000.0, &mut saves);
    assert_eq!(f.screen, Screen::Main);
    assert_eq!(
        f.take_actions(),
        [FrontAction::Music("Sounds/Musiques/xfiles.ogg".into())]
    );
}

#[test]
fn the_main_menus_buttons_sit_where_the_layout_formulas_put_them() {
    let Some(mut f) = front() else { return };
    let saves = StubSaves::default();
    f.enter_main(&saves);
    // `menu` 1.01 × the window; the background PanScan of ratio
    // 1365/768 fills its height: 775.68 high, centred. Each button is its
    // picture's size (366 × 75 for NewGame.png) at (0.5, y) of it.
    let bg_h = f64::from(1.01f32 * 768.0);
    for (name, y) in [
        ("newGameButton", 0.42),
        ("continueGameButton", 0.54),
        ("loadGameButton", 0.66),
        ("optionsButton", 0.78),
        ("quitButton", 0.90),
    ] {
        let r = f.rect_of(name, W, H).unwrap();
        let cy = r.top + r.height / 2.0;
        let want = 384.0 + (y - 0.5) * bg_h;
        assert!((cy - want).abs() < 0.01, "{name}: {cy} vs {want}");
        assert!(close(r.left + r.width / 2.0, 682.5), "{name}");
    }
    let r = f.rect_of("newGameButton", W, H).unwrap();
    assert_eq!((r.width, r.height), (366.0, 75.0));
    // No `save.xml`: "continue" disabled (its down picture shows).
    assert_eq!(f.button_state("continueGameButton"), Some(State::Disabled));
    assert_eq!(f.shown("facebookButton"), Some(false));
    let mut with = StubSaves::default();
    with.record("save.xml");
    f.enter_main(&with);
    assert_eq!(f.button_state("continueGameButton"), Some(State::Up));
}

#[test]
fn a_new_game_closes_turns_and_opens_the_planks() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    saves.record("3.xml");
    f.enter_main(&saves);
    f.take_actions();
    click(&mut f, "newGameButton", 10_000.0, &mut saves);
    // No `save.xml`: no confirmation; the chapters deleted.
    assert!(!f.confirm_shown());
    assert!(!saves.exists("3.xml"));
    assert!(f.in_transition());
    let a = f.take_actions();
    assert_eq!(
        a,
        [
            FrontAction::Sound("Sounds/SFX/ChoixMenu.wav".into()),
            FrontAction::Sound("Sounds/SFX/E_Piano_Clavier_ferme_NA.wav".into()),
        ]
    );
    for b in ["newGameButton", "continueGameButton", "quitButton"] {
        assert_eq!(f.button_state(b), Some(State::Disabled), "{b}");
    }
    f.tick(10_999.0, &mut saves);
    assert!(f.take_actions().is_empty());
    f.tick(11_000.0, &mut saves);
    assert_eq!(
        f.take_actions(),
        [FrontAction::Sound("Sounds/SFX/DemontageObjet.wav".into())]
    );
    f.tick(12_000.0, &mut saves);
    assert_eq!(
        f.take_actions(),
        [
            FrontAction::StartGame("save.xml".into()),
            FrontAction::Sound("Sounds/SFX/E_Piano_Clavier_ouvre_NA.wav".into()),
        ]
    );
    assert_eq!(f.screen, Screen::Game);
    assert!(f.in_transition());
    f.tick(13_000.0, &mut saves);
    assert!(!f.in_transition());
    assert!(!f.active());
}

#[test]
fn with_a_save_new_game_asks_first() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    saves.record("save.xml");
    f.enter_main(&saves);
    click(&mut f, "newGameButton", 0.0, &mut saves);
    assert!(f.confirm_shown());
    assert!(saves.exists("save.xml"));
    click(&mut f, "no", 100.0, &mut saves);
    assert!(!f.confirm_shown() && !f.in_transition());
    click(&mut f, "newGameButton", 200.0, &mut saves);
    click(&mut f, "yes", 300.0, &mut saves);
    assert!(!saves.exists("save.xml"));
    assert!(f.in_transition());
}

#[test]
fn the_volume_rows_step_in_fifths_and_light_their_sprites() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    f.enter_main(&saves);
    click(&mut f, "optionsButton", 0.0, &mut saves);
    assert_eq!(f.screen, Screen::Options);
    for _ in 0..5 {
        click(&mut f, "musicVolumeMinusButton", 0.0, &mut saves);
    }
    // The fifth step leaves 2.98e-08, which lights nothing.
    assert_eq!(f.settings.volume("music").to_bits(), 0x3300_0000);
    for i in 0..5 {
        assert_eq!(f.shown(&format!("musicVolumeSprite{i}")), Some(false));
        assert_eq!(f.shown(&format!("musicVolumeSprite{i}Off")), Some(true));
    }
    click(&mut f, "musicVolumeMinusButton", 0.0, &mut saves);
    assert_eq!(f.settings.volume("music"), 0.0);
    click(&mut f, "musicVolumePlusButton", 0.0, &mut saves);
    assert_eq!(f.shown("musicVolumeSprite0"), Some(true));
    assert_eq!(f.shown("musicVolumeSprite1"), Some(false));
    let a = f.take_actions();
    assert!(a.contains(&FrontAction::Volume("music".into(), 0.2)));
    // The dialog row replays its test file.
    click(&mut f, "dialogVolumeMinusButton", 0.0, &mut saves);
    assert!(f.take_actions().contains(&FrontAction::TestSound(
        "dialog".into(),
        "Sounds/Dialogs/PG813A01.ogg".into()
    )));
    // `inverseLook` toggles its flag; `compassLook` starts disabled with
    // nothing shown.
    assert!(!f.settings.inverse_look);
    click(&mut f, "inverseLook", 0.0, &mut saves);
    assert!(f.settings.inverse_look);
    click(&mut f, "inverseLook", 0.0, &mut saves);
    assert!(!f.settings.inverse_look);
    click(&mut f, "quitButton", 0.0, &mut saves);
    assert_eq!(f.screen, Screen::Main);
}

#[test]
fn the_credits_scroll_30_s_then_go_back_to_the_options() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    f.enter_options();
    f.take_actions();
    click(&mut f, "creditsButton", 1000.0, &mut saves);
    assert_eq!(f.screen, Screen::Credits);
    assert!(f
        .take_actions()
        .contains(&FrontAction::Music("Sounds/Musiques/M_Credits.ogg".into())));
    f.tick(30_999.0, &mut saves);
    assert_eq!(f.screen, Screen::Credits);
    f.tick(31_000.0, &mut saves);
    assert_eq!(f.screen, Screen::Options);
}

#[test]
fn the_load_screen_sorts_by_played_time_and_loads_after_the_planks() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    for n in ["0.xml", "2.xml", "4.xml"] {
        saves.record(n);
    }
    saves.played = vec![("0.xml".into(), 900.0), ("2.xml".into(), 100.0)];
    f.enter_main(&saves);
    click(&mut f, "loadGameButton", 0.0, &mut saves);
    assert_eq!(f.screen, Screen::Loading);
    // 4.xml has no `PlayedTime`: 1e43, as the missing ones, in index
    // order after the timed ones.
    assert_eq!(f.loading_state(), Some((vec![2, 0, 1, 3, 4, 5], 0)));
    // A missing chapter is disabled.
    assert_eq!(f.button_state("1"), Some(State::Disabled));
    // Slot 0 holds button "2"; a click asks, "yes" closes the planks.
    click(&mut f, "2", 0.0, &mut saves);
    assert!(f.confirm_shown());
    click(&mut f, "yes", 50.0, &mut saves);
    f.take_actions();
    f.tick(1050.0, &mut saves);
    f.tick(2050.0, &mut saves);
    assert!(f
        .take_actions()
        .contains(&FrontAction::StartGame("2.xml".into())));
}

#[test]
fn the_load_screen_slides_by_0_4_in_250_ms() {
    let Some(mut f) = front() else { return };
    let mut saves = StubSaves::default();
    f.enter_main(&saves);
    click(&mut f, "loadGameButton", 0.0, &mut saves);
    let before = f.rect_of("slot0", W, H).unwrap();
    // Left at index 0: nothing.
    click(&mut f, "leftButton", 0.0, &mut saves);
    assert_eq!(f.loading_state().unwrap().1, 0);
    click(&mut f, "rightButton", 100.0, &mut saves);
    assert_eq!(f.loading_state().unwrap().1, 1);
    f.tick(350.0, &mut saves);
    let after = f.rect_of("slot0", W, H).unwrap();
    // 0.4 of the background's width (1378.65 at this window).
    let moved = before.left - after.left;
    assert!(
        (moved - 0.4 * f64::from(1.01f32 * 1365.0)).abs() < 0.01,
        "{moved}"
    );
}

/// Writes the front end's screens as PNG files into `A4_SHOT_DIR` for a
/// look: the splashes, the main menu, the planks closed and turning, the
/// options, the credits, the load screen.
#[test]
#[ignore]
fn screenshots() {
    let Some(mut f) = front() else { return };
    let Some(out) = std::env::var_os("A4_SHOT_DIR") else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    let mut saves = StubSaves::default();
    saves.record("0.xml");
    let shot = |f: &mut Front, name: &str| {
        let mut px = vec![0u32; (W * H) as usize];
        f.draw(&mut px, W, H);
        let rgb: Vec<u8> = px
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
            .collect();
        let file = std::fs::File::create(out.join(name)).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), W, H);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&rgb).unwrap();
    };
    f.start(0.0, &saves);
    shot(&mut f, "1-splash0.png");
    f.tick(1500.0, &mut saves);
    shot(&mut f, "2-splash1.png");
    f.tick(3000.0, &mut saves);
    shot(&mut f, "3-main.png");
    click(&mut f, "optionsButton", 3100.0, &mut saves);
    for _ in 0..2 {
        click(&mut f, "musicVolumeMinusButton", 3200.0, &mut saves);
    }
    shot(&mut f, "4-options.png");
    click(&mut f, "creditsButton", 4000.0, &mut saves);
    f.tick(14_000.0, &mut saves);
    shot(&mut f, "5-credits.png");
    click(&mut f, "quitButton2", 14_100.0, &mut saves);
    click(&mut f, "quitButton", 14_200.0, &mut saves);
    click(&mut f, "loadGameButton", 14_300.0, &mut saves);
    shot(&mut f, "6-load.png");
    click(&mut f, "quitButton", 14_400.0, &mut saves);
    click(&mut f, "newGameButton", 20_000.0, &mut saves);
    f.tick(20_500.0, &mut saves);
    shot(&mut f, "7-planks-closing.png");
    f.tick(21_500.0, &mut saves);
    shot(&mut f, "8-planks-turning.png");
    f.tick(22_500.0, &mut saves);
    shot(&mut f, "9-planks-opening.png");
}
