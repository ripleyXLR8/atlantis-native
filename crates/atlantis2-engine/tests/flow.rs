//! A game played through the features together, as a player starts it:
//! the player menu, a new game and its opening film, the first click that
//! sends the game to chapter 75, an object taken and the bag bar opened,
//! Escape and a save through the menus over the open bar, the save loaded
//! again through the load page, then continued from the player menu of a
//! new run (the music started again from the save, at a drawn mark).
//! Then a game from the start-up intro, played with the mouse: the walk
//! to the shaman, his subjects asked through the subject menu, Ireland's
//! boat taken, the walk to the hut, the boat picked from the bag bar and
//! laid on its zone (the mandala hook, chapter 75's section, the film),
//! the trip to Ireland and its first dialog, Liam asked (his 3D head, as
//! every dialog's, drawn by the Pentium II's routines), the walk to the
//! place of Brigid's cross (its hooks noted, the puzzle solved by the
//! testing aid: the goddess), a save and a load in Ireland, and the way
//! back to Tibet as chapter 16 makes it.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped. The saves go to a
//! scratch folder of the build's.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::save::{entry, read_save, SystemTime};
use atlantis2_data::A2Files;
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::engine::{IrishPuzzle, MANDALAS};
use atlantis2_engine::menu::{state, PageKind};
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn time() -> SystemTime {
    SystemTime {
        year: 2026,
        month: 10,
        day_of_week: 4,
        day: 22,
        hour: 12,
        minute: 0,
        second: 0,
        milliseconds: 0,
    }
}

/// The engine at 60 passes a second, every pass drawn.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
    /// Added to the click times of [`Run::tap`]: clicks far apart, never
    /// a double click.
    gap: f64,
    /// Passes that drew a dialog line's 3D head (its lip-sync running).
    head_passes: u64,
}

impl Run {
    fn new(dir: &Path) -> Run {
        let mut e = Engine::boot(
            game().unwrap(),
            Variant::default(),
            Fpu::default(),
            640,
            480,
        )
        .unwrap();
        e.set_data_dir(Some(dir.to_path_buf()));
        e.wall_clock = time;
        e.trace = true;
        Run {
            e,
            pass: 0,
            screen: vec![0; 640 * 480],
            gap: 0.0,
            head_passes: 0,
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 / 60.0
    }

    fn step(&mut self) {
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
        let fatal = self.e.script_state().and_then(|s| s.fatal.clone());
        assert!(fatal.is_none(), "{fatal:?}");
        // A dialog's heads follow the panorama's CPU: the Pentium II's
        // routines (the default `warpPII`), under the 0x0e7f the warp
        // frames and the II set-up leave, handed back to the engine.
        if let Some(d) = self.e.dialog() {
            assert_eq!(d.variant(), Variant::PII);
            assert_eq!((d.fpu(), self.e.fpu()), (Fpu::Chop, Fpu::Chop));
            if d.lipsync().is_some() {
                self.head_passes += 1;
            }
        }
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// A click at (x, y) of the menu, the button released after a pass.
    fn press(&mut self, x: i32, y: i32) {
        self.e.pointer(x, y);
        self.e.click(self.now());
        self.step();
        self.e.release();
        self.step();
    }

    fn press_widget(&mut self, name: &str) {
        let m = self.e.menu().unwrap();
        let w = m.page.as_ref().unwrap().widget(name).unwrap();
        let (x, y) = (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
        );
        self.press(x, y);
    }

    fn page(&self) -> Option<PageKind> {
        self.e.menu().and_then(|m| m.kind())
    }

    /// Escape, the disc button: the load / save panel slid in.
    fn disc_panel(&mut self) {
        self.e.escape();
        assert_eq!(self.page(), Some(PageKind::Main));
        self.press_widget("a_disc.spr");
        self.wait(30);
        assert_eq!(self.e.menu().unwrap().state, state::LOAD_OR_SAVE);
    }

    /// The sprites the last pass drew over the panorama.
    fn ui_sprites(&self) -> Vec<(UiBank, i32, i32, i32)> {
        self.e
            .ui_draws
            .iter()
            .filter_map(|d| match d {
                UiDraw::Sprite { bank, frame, x, y } => Some((*bank, *frame, *x, *y)),
                _ => None,
            })
            .collect()
    }

    /// What a load gives back, as the scripts and the navigation see it
    /// (the angles apart, `AngleWarpX`.. `AngleWarpYNow`: the view entry
    /// turns the camera back to the saved ones with float residues).
    fn state(&self) -> String {
        let s = self.e.script_state().unwrap();
        let mut vars = s.vars.clone();
        vars[41..45].fill(0);
        format!(
            "chapter {} place {:?} vars {:?} bag {:?} music {:?}",
            s.chapter,
            self.e.place(),
            vars,
            s.bag,
            String::from_utf8_lossy(&s.music),
        )
    }
}

/// Each answer against the original's (`tests/golden/subjects.txt`'s
/// `ask` rows): when the golden asked `character` about that subject, the
/// dialog is one it started for it (an answer can depend on what was asked
/// before; a subject the golden never asked is not checked).
fn answers_as_the_original(character: i32, asked: &[(i32, u32)]) {
    let golden = include_str!("golden/subjects.txt");
    for &(value, dialog) in asked {
        let head = format!("ask {{0:{character}}} ");
        let tail = format!(" value {value} |");
        let rows: Vec<&str> = golden
            .lines()
            .filter(|l| l.starts_with(&head) && l.contains(&tail))
            .collect();
        let start = format!("AtlDial_start({dialog})");
        assert!(
            rows.is_empty() || rows.iter().any(|l| l.contains(&start)),
            "{character} {value}: dialog {dialog}, the original's {rows:?}"
        );
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn player_menu_new_game_bag_save_and_load() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow");
    let mut r = Run::new(&dir);

    // The player menu; player 4 has no save: a new game, its opening film.
    r.e.open_player_menu();
    r.wait(5);
    assert_eq!(r.page(), Some(PageKind::Player));
    r.press_widget("b_ter.spr");
    assert_eq!((r.e.player_number, r.e.loop_type), (4, LoopType::Video));
    r.wait(120);
    assert_eq!(r.e.loop_type, LoopType::Video);
    // A click ends the film; the first view.
    r.e.click(r.now());
    for _ in 0..600 {
        r.step();
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    r.wait(30);
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert_eq!(r.e.place(), ("tibet.wam", 2));

    // The first click sends the game to chapter 75.
    r.e.click(r.now());
    r.wait(30);
    assert_eq!(r.e.script_state().unwrap().chapter, 75);

    // An object in the bag, the bar opened with the right button: the bar,
    // its one cell, the pointer at the screen centre.
    let names = DefNames::parse(
        &r.e.files()
            .read(atlantis2_data::Folder::Scenar, "defnames.con")
            .unwrap(),
    )
    .unwrap();
    let name = String::from_utf8_lossy(names.name(defnames::OBJETS, 61).unwrap()).into_owned();
    r.e.run_command(format!("(sacados={name})").as_bytes());
    r.e.right_click();
    assert!(r.e.bar.open);
    r.step();
    let sprites = r.ui_sprites();
    assert_eq!(sprites[0], (UiBank::Interface, 0, 320, 441));
    assert_eq!(sprites[1].0, UiBank::Objects);
    assert_eq!(*sprites.last().unwrap(), (UiBank::Interface, 40, 320, 240));
    assert_eq!(r.e.bar.shown.len(), 1);

    // Escape over the open bar: the main menu, the save.
    let before = r.state();
    let want = r.e.save_data();
    r.disc_panel();
    r.press_widget("a_save.spr");
    r.press_widget("b_oui.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert!(r.e.bar.open, "the menus leave the bar as it was");
    let file = std::fs::read(dir.join("save").join("ATL04_00.SAV")).unwrap();
    let (got, _) = read_save(&file).unwrap();
    assert_eq!(got.entry(entry::BAG_COUNT), want.entry(entry::BAG_COUNT));
    assert_eq!(got.i32(entry::CHAPTER), 75);
    let music = got.i32(entry::MUSIC);
    // Chapter 75 plays music 30.
    assert_ne!(music, 0);

    // The bar closed, the game goes on, then the load page loads the save.
    r.e.right_click();
    assert!(!r.e.bar.open);
    r.e.mouse_motion(300, 0);
    r.wait(30);
    r.disc_panel();
    r.press_widget("a_load.spr");
    assert_eq!(r.page(), Some(PageKind::Load));
    r.press(108 + 30, 129 + 30);
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(r.state(), before);
    r.wait(60);

    // A new run: the player menu continues player 4's save, the music
    // started again at a drawn mark.
    let mut r = Run::new(&dir);
    r.e.open_player_menu();
    r.wait(5);
    r.press_widget("b_ter.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(r.state(), before);
    r.wait(120);
    assert_eq!(r.e.music_playing(), Some(music));
    assert!(r
        .e
        .events
        .iter()
        .any(|e| matches!(e, Event::Music { music: m, .. } if *m == music)));
}

/// The mouse play of the Tibet game: walks, sprites, subjects, the bar.
impl Run {
    /// A left click far from the last one (never a double click).
    fn tap(&mut self) {
        self.gap += 10.0;
        self.e.click(self.now() + self.gap);
    }

    /// One pass, its events taken.
    fn events(&mut self) -> Vec<Event> {
        self.step();
        std::mem::take(&mut self.e.events)
    }

    /// The mouse moved to put the pointer at (x, y).
    fn point(&mut self, x: i32, y: i32) {
        let (px, py) = self.e.pointer;
        self.e.mouse_motion(x - px, y - py);
    }

    fn var(&self, name: &str) -> i32 {
        self.e.script_state().unwrap().var(name).unwrap()
    }

    /// The value of subject `name` (its `defnames.con` parameter).
    fn subject(&self, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.param(defnames::SUJETS, name.as_bytes())
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        let s = self.e.script_state().unwrap();
        s.names.id(section, name.as_bytes()).unwrap() as i32
    }

    /// The move zone of the current view that leads to view `to`.
    fn zone_to(&self, to: u32) -> usize {
        let view = self.e.place().1;
        let zones = &self.e.wam().view(view).unwrap().zones;
        zones
            .iter()
            .position(|z| z.kind == 1 && z.value == to as i32)
            .unwrap_or_else(|| panic!("no move from view {view} to {to}"))
    }

    /// Turns the view onto zone `zone` (the centre on it).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        assert_eq!(self.e.centre_zone().map(|(i, _)| i), Some(zone));
    }

    /// A click on the move zone to view `to`, the transition video
    /// clicked away; a dialog the arrival plays listened to. The events,
    /// in order.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        let zone = self.zone_to(to);
        self.aim(zone);
        self.tap();
        let mut all = Vec::new();
        for _ in 0..600 {
            let evs = self.events();
            let video = evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            });
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if video {
                self.wait(15);
                self.tap();
            }
            if arrived {
                assert_eq!(self.e.place().1, to);
                self.wait(4);
                all.extend(std::mem::take(&mut self.e.events));
                if self.e.loop_type == LoopType::Dialog {
                    all.extend(self.listen());
                }
                assert_eq!(self.e.loop_type, LoopType::Warp);
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// Turns the view as a player does until a sprite is under the centre
    /// and clicks it; the events of the click when `done` holds on them.
    fn click_sprite(&mut self, done: impl Fn(&Event) -> bool) -> Vec<Event> {
        for pitch in (-8..=8).map(|p| p * 5) {
            for yaw in (0..72).map(|y| y * 5 - 180) {
                let (y, p) = ((yaw as f32).to_radians(), (pitch as f32).to_radians());
                self.e.camera.enter(y, p);
                self.step();
                if self.e.sprite_under_centre().1 == 0 {
                    continue;
                }
                self.tap();
                let evs = self.events();
                if evs.iter().any(&done) {
                    return evs;
                }
                // Another character's menu: closed by a click outside.
                if self.e.subject_menu.open {
                    self.point(0, 0);
                    self.tap();
                    self.step();
                }
            }
        }
        panic!("no sprite answers in view {}", self.e.place().1);
    }

    /// The running dialog played to its end, a click every 20 passes.
    fn listen(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        for n in 0..20_000 {
            if n % 20 == 19 {
                self.tap();
            }
            let evs = self.events();
            let end = evs.contains(&Event::DialogEnd);
            all.extend(evs);
            if end {
                self.wait(4);
                return all;
            }
        }
        panic!("the dialog does not end");
    }

    /// Asks `character` once about each subject his menu offers (the list
    /// changes with the answers), each through the menu: a click on him,
    /// the pointer on the subject's cell, a click, the dialog listened to.
    /// An object an answer puts in the hand goes back to the bag (the bar
    /// opened and closed). The subjects and their dialogs, in order.
    fn ask_all(&mut self, character: i32) -> Vec<(i32, u32)> {
        let mut asked: Vec<(i32, u32)> = Vec::new();
        loop {
            let evs = self.click_sprite(|e| matches!(e, Event::SubjectMenu { .. }));
            let s = self.e.script_state().unwrap();
            let list = s.subjects.list(character);
            assert!(evs.contains(&Event::SubjectMenu {
                character,
                count: list.len() as i32
            }));
            let Some(i) = list
                .iter()
                .position(|e| asked.iter().all(|a| a.0 != e.value))
            else {
                // All asked: a click outside closes the menu.
                self.point(100, 100);
                self.step();
                self.tap();
                let evs = self.events();
                assert!(evs.contains(&Event::Subject {
                    character,
                    value: None
                }));
                assert!(!self.e.subject_menu.open);
                return asked;
            };
            let value = list[i].value;
            let (x, y) = cell(list.len() as i32, i as i32);
            self.point(x + 31, y + 31);
            self.step();
            self.tap();
            let evs = self.events();
            assert!(evs.contains(&Event::Subject {
                character,
                value: Some(value)
            }));
            assert!(!self.e.subject_menu.open);
            let number = evs
                .iter()
                .find_map(|e| match e {
                    Event::DialogStart {
                        number,
                        started: true,
                    } => Some(*number),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("subject {value}: no dialog"));
            asked.push((value, number));
            self.listen();
            assert_eq!(self.e.loop_type, LoopType::Warp);
            assert_eq!((self.var("CLICPERSO"), self.var("SUJET")), (0, -2));
            if self.var("OBJETENMAIN") != 0 {
                self.e.right_click();
                self.step();
                self.e.right_click();
                self.step();
                assert_eq!(self.var("OBJETENMAIN"), 0);
            }
            assert!(asked.len() < 16, "the subjects never run out");
        }
    }

    /// The bag bar opened with the right button and object `id`'s cell
    /// clicked: the object in hand, the bar closed.
    fn pick(&mut self, id: i32) {
        self.e.right_click();
        assert!(self.e.bar.open);
        self.step();
        let bag = self.e.bag_entries();
        let k = self
            .e
            .bar
            .shown
            .iter()
            .position(|&n| bag[n].id == id)
            .expect("the object in a cell of the bar");
        self.point(40 + 80 * k as i32, 440);
        self.step();
        self.tap();
        self.step();
        assert!(!self.e.bar.open);
        assert_eq!(self.var("OBJETENMAIN"), id);
    }
}

#[test]
fn a_game_from_tibet_to_ireland_and_back() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow-tibet");
    let mut r = Run::new(&dir);

    // The start-up: the player menu under the intro; a double click ends
    // every video, the player page is back.
    r.e.open_player_menu();
    r.e.start_intro();
    assert!(r.e.intro_playing());
    r.wait(30);
    r.e.click(r.now());
    r.e.click(r.now() + 0.1);
    for _ in 0..60 {
        if r.e.loop_type == LoopType::Menu {
            break;
        }
        r.step();
    }
    assert!(!r.e.intro_playing());
    assert_eq!(r.page(), Some(PageKind::Player));

    // A new game, its film clicked away, the first click (chapter 75).
    r.press_widget("b_ter.spr");
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.wait(30);
    r.tap();
    for _ in 0..600 {
        r.step();
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    r.wait(15);
    assert_eq!(r.e.place(), ("tibet.wam", 2));
    r.tap();
    r.wait(15);
    assert_eq!(r.e.script_state().unwrap().chapter, 75);

    // To the shaman of view 7 (his greeting on the first arrival), then
    // everything he can be asked.
    for to in 3..=7 {
        r.walk(to);
    }
    let shaman = r.id(defnames::PERSONNAGES, "SHAMAN");
    // His subjects, in the order the menu offers them, and the dialogs
    // their answers play.
    let asked = r.ask_all(shaman);
    let want: Vec<(i32, u32)> = [
        ("SHAMAN", 361),
        ("TATOUAGE", 369),
        ("CRISTAL", 364),
        ("HEROS_TIBET", 362),
    ]
    .iter()
    .map(|&(n, d)| (r.subject(n), d))
    .collect();
    assert_eq!(asked, want);
    answers_as_the_original(shaman, &asked);
    // A subject asked and still offered is drawn at 50 % (a flag not 1).
    let s = r.e.script_state().unwrap();
    assert!(s
        .subjects
        .list(shaman)
        .iter()
        .any(|e| e.flag != 1 && asked.iter().any(|a| a.0 == e.value)));

    // View 8: its boat taken with a click.
    r.walk(8);
    let evs = r.click_sprite(|e| matches!(e, Event::Take { .. }));
    let boat = evs
        .iter()
        .find_map(|e| match e {
            Event::Take { id } => Some(*id),
            _ => None,
        })
        .unwrap();
    let (_, zone, order) = *MANDALAS
        .iter()
        .find(|(object, _, _)| r.id(defnames::OBJETS, object) == boat)
        .expect("a boat of the mandala puzzle");
    assert!(r.e.bag_entries().iter().any(|b| b.id == boat));
    // Into the hut, the shaman of view 10 asked too.
    r.walk(9);
    r.walk(10);
    let shaman10 = r.id(defnames::PERSONNAGES, "SHAMANW10");
    let asked10 = r.ask_all(shaman10);
    let want: Vec<(i32, u32)> = [
        ("TATOUAGE", 369),
        ("CRISTAL", 364),
        ("HEROS_TIBET", 362),
        ("MORCEAU_MANDALA", 365),
        ("MANDALA", 363),
    ]
    .iter()
    .map(|&(n, d)| (r.subject(n), d))
    .collect();
    assert_eq!(asked10, want);
    answers_as_the_original(shaman10, &asked10);

    // The boat picked from the bar, the centre on its zone, a click: the
    // hook (the laid sound, the count and the order), chapter 75's
    // section (its film), the trip to the boat's world refused.
    assert_eq!(r.var("NbMandalaPosed"), 0);
    r.pick(boat);
    let zi =
        r.e.wam()
            .view(10)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some(zone))
            .unwrap();
    r.aim(zi);
    r.tap();
    let mut evs = r.events();
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::PuzzleHook { zone, .. } if *zone == zi)));
    assert!(evs.contains(&Event::Sound("sound400.apc".into())));
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.wait(30);
    r.tap();
    for _ in 0..600 {
        evs.extend(r.events());
        if r.e.loop_type != LoopType::Video {
            break;
        }
    }
    // The trip to Ireland, as chapter 75's section goes on after the film:
    // the world (disc 1, Ireland's objects and texts), `bigile1.wam` view
    // 5, chapter 2, whose `/INIT` plays the first arrival's dialog.
    if r.e.loop_type == LoopType::Dialog {
        evs.extend(r.listen());
    }
    evs.extend(std::mem::take(&mut r.e.events));
    assert!(!evs.iter().any(|e| matches!(e, Event::WorldNotPorted(_))));
    assert!(evs.contains(&Event::Monde(1)));
    assert!(evs.contains(&Event::Wam("bigile1.wam".into())));
    assert!(evs.contains(&Event::DialogStart {
        number: 1,
        started: true
    }));
    assert_eq!(r.e.place(), ("bigile1.wam", 5));
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert_eq!(r.e.files().current_disc(), 1);
    let pose = format!("pose{}", zone[5..].to_ascii_lowercase());
    assert_eq!(
        (
            r.e.script_state().unwrap().chapter,
            r.var("MONDE"),
            r.var("NbMandalaPosed"),
            r.var(order),
            r.var(&pose),
            r.var("VignetteSave"),
        ),
        (2, 1, 1, 1, 1, 1)
    );
    // Its first `/INIT` section ran once (the dialog, then its flag).
    assert_eq!(r.var("firsteire"), 1);
    // The boat has left the bag, the hand is empty.
    assert!(!r.e.bag_entries().iter().any(|b| b.id == boat));
    assert_eq!(r.var("OBJETENMAIN"), 0);
    ireland_lines_as_the_original(&evs);

    // Liam, the first character, in view 5: his one subject (dialog 2),
    // after which chapter 2 sends the game to chapter 5. His answer is
    // spoken by a 3D head, drawn by the Pentium II's routines (`step`).
    let liam = r.id(defnames::PERSONNAGES, "LIAM_SCRIPT");
    r.head_passes = 0;
    let asked = r.ask_all(liam);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].1, 2);
    assert!(r.head_passes > 0, "no head drawn for Liam's answer");
    assert_eq!(r.e.script_state().unwrap().chapter, 5);

    // The first views: 4 (Liam again), 3 (chapter 5 sends the game to
    // chapter 16 on the way), 2, the place of Brigid's cross.
    let mut walked = Vec::new();
    for to in [4, 3, 2] {
        walked.extend(r.walk(to));
        assert_eq!(r.e.place(), ("bigile1.wam", to));
    }
    assert_eq!(r.e.script_state().unwrap().chapter, 16);
    ireland_lines_as_the_original(&walked);
    // The cross's entry hook (FUN_00494120), not ported, is noted.
    assert!(walked.contains(&Event::PuzzleNotPorted {
        function: 0x0049_4120
    }));
    // A click on one of the cross's zones: the hook (FUN_00494190) noted,
    // the click goes on (`CLICZONE`, the click scan), nothing else.
    let cross_zone =
        r.e.wam()
            .view(2)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 4)
            .unwrap();
    r.aim(cross_zone);
    r.tap();
    let evs = r.events();
    assert!(evs.contains(&Event::PuzzleNotPorted {
        function: 0x0049_4190
    }));
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::ClickScan(v) if v.cliczone != 0)));
    assert_eq!(
        (r.e.place(), r.e.loop_type),
        (("bigile1.wam", 2), LoopType::Warp)
    );

    // The cross solved by the testing aid (what its win writes:
    // `FINCROIX` and a scan): chapter 16's section plays the goddess's
    // film and dialog, and shows her.
    r.e.solve_irish_puzzle(IrishPuzzle::Cross);
    let mut evs = std::mem::take(&mut r.e.events);
    assert!(evs.iter().any(
        |e| matches!(e, Event::VideoStart { name, frames: Some(_) } if name == "cine015.hnm")
    ));
    r.wait(30);
    r.tap();
    for _ in 0..600 {
        evs.extend(r.events());
        if r.e.loop_type != LoopType::Video {
            break;
        }
    }
    if r.e.loop_type == LoopType::Dialog {
        evs.extend(r.listen());
    }
    assert!(evs.contains(&Event::DialogStart {
        number: 56,
        started: true
    }));
    assert_eq!(r.var("flagdeesse"), 1);
    ireland_lines_as_the_original(&evs);
    let goddess = r.id(defnames::PERSONNAGES, "DEESSE");
    assert!(!r
        .e
        .script_state()
        .unwrap()
        .subjects
        .list(goddess)
        .is_empty());

    // Saved in Ireland through the menus, continued by a new run's player
    // menu: the place, the world and its disc, the chapter and the story.
    let kept = |r: &Run| {
        let s = r.e.script_state().unwrap();
        format!(
            "chapter {} place {:?} disc {} monde {} laid {} {} {} goddess {} bag {:?} subjects {:?}",
            s.chapter,
            r.e.place(),
            r.e.files().current_disc(),
            r.var("MONDE"),
            r.var("NbMandalaPosed"),
            r.var(order),
            r.var(&pose),
            r.var("flagdeesse"),
            s.bag,
            s.subjects.list(goddess),
        )
    };
    let before = kept(&r);
    r.disc_panel();
    r.press_widget("a_save.spr");
    r.press_widget("b_oui.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    let mut r = Run::new(&dir);
    r.e.open_player_menu();
    r.wait(5);
    r.press_widget("b_ter.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(kept(&r), before);
    r.wait(30);

    // The way back as chapter 16 makes it, once the story has set its flag
    // (`flag71` = 1, set here by a command): leaving view 2 for view 1
    // plays film 19 with dialog 71 over it, and the game is back in Tibet,
    // view 7, chapter 75, Ireland's mandala in the bag and in hand.
    r.e.run_command(b"(flag71=1)");
    r.wait(15);
    let zone = r.zone_to(1);
    r.aim(zone);
    r.tap();
    let mut evs = Vec::new();
    for _ in 0..600 {
        evs.extend(r.events());
        if evs
            .iter()
            .any(|e| matches!(e, Event::VideoStart { name, .. } if name == "cine019.hnm"))
        {
            break;
        }
    }
    r.wait(30);
    r.tap();
    for _ in 0..600 {
        evs.extend(r.events());
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    // The section ran to its end when the film ended: the mandala in the
    // bag and in hand, `ARRIVEEVUE` = the view `vue=7` left (2).
    let mandala = r.id(defnames::OBJETS, "MANDALA_IRLANDE");
    assert_eq!((r.var("OBJETENMAIN"), r.var("ARRIVEEVUE")), (mandala, 2));
    // The next panorama frame's `Interp_GereFrame` sees that `ARRIVEEVUE`:
    // the per-view reset empties the hand, as the original's does.
    r.wait(4);
    evs.extend(std::mem::take(&mut r.e.events));
    assert_eq!((r.var("OBJETENMAIN"), r.var("ARRIVEEVUE")), (0, 0));
    ireland_lines_as_the_original(&evs);
    assert!(evs.contains(&Event::Monde(5)));
    assert!(evs.contains(&Event::Wam("tibet.wam".into())));
    assert!(!evs.iter().any(|e| matches!(e, Event::WorldNotPorted(_))));
    assert_eq!(r.e.place(), ("tibet.wam", 7));
    assert_eq!(
        (
            r.e.script_state().unwrap().chapter,
            r.var("MONDE"),
            r.e.script_state().unwrap().flags[71]
        ),
        (75, 5, 2)
    );
    assert!(r.e.bag_entries().iter().any(|b| b.id == mandala));
}

/// Every `/set` line of an Irish chapter that ran (the events'
/// chapter and line) is one of the lines the original's handler ran
/// (`scripts_worlds_ireland.txt`, chapters 1-24): the line numbers the
/// port reports are the scripts' own.
fn ireland_lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_ireland.txt");
    let mut n = 0;
    for e in evs {
        if let Event::Set { chapter, line } = e {
            if (1..=24).contains(chapter) {
                let head = format!("chapi{chapter:03}.con:{line} ");
                assert!(
                    golden.lines().any(|l| l.starts_with(&head)),
                    "chapter {chapter} line {line}: not a line the original runs"
                );
                n += 1;
            }
        }
    }
    assert!(n > 0 || !evs.iter().any(|e| matches!(e, Event::Set { .. })));
}
