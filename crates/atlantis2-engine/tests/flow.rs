//! A game played through the features together, as a player starts it:
//! the player menu, a new game and its opening film, the first click that
//! sends the game to chapter 75, an object taken and the bag bar opened,
//! Escape and a save through the menus over the open bar, the save loaded
//! again through the load page, then continued from the player menu of a
//! new run (the music started again from the save, at a drawn mark).
//! Then a game from the start-up intro, played with the mouse (the view
//! turned by mouse moves, every action a click, every object picked from
//! the bag bar): the walk to the shaman, his subjects asked through the
//! subject menu, Ireland's boat taken, the walk to the hut, the boat laid
//! on its zone (the mandala hook, chapter 75's section, the film), the
//! trip to Ireland and its first dialog, Liam asked (his 3D head, as every
//! dialog's, drawn by the Pentium II's routines); then the whole of
//! Ireland's story as its chapters 2-24 make it ([`IRELAND`], stage by
//! stage: the book, the five skulls and the skull puzzle, Ailill, the
//! ogham stones, the horse, the poet's blackbird songs, the dowsing rod,
//! the serpent's well, Brigid's cross and the goddess, the salmon, the
//! glass tower and Nuada's hand), every chapter left by its own script
//! line, a save and a load in Ireland through the menus, and the way back
//! to Tibet as chapter 16 makes it.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped. The saves go to a
//! scratch folder of the build's.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::save::{entry, read_save, SystemTime};
use atlantis2_data::A2Files;
use atlantis2_engine::bag::{UiBank, UiDraw};
use atlantis2_engine::engine::MANDALAS;
use atlantis2_engine::menu::{state, PageKind};
use atlantis2_engine::sound::VOICE_EFFECTS;
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
    /// The pass's sound, mixed as a sound device takes it (so voices end
    /// as a player hears them end).
    audio: Vec<f32>,
    /// The signs of the mouse axes for the view's turn, found by the
    /// first moves.
    mouse_signs: (f32, f32),
    /// Every event taken so far, in order.
    journal: Vec<Event>,
    /// Where [`Run::shot`] writes the screen (`ATLANTIS2_PNG_DIR`).
    shots: Option<PathBuf>,
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
            audio: vec![0.0; 368 * 2],
            mouse_signs: (1.0, -1.0),
            journal: Vec::new(),
            shots: std::env::var_os("ATLANTIS2_PNG_DIR").map(PathBuf::from),
        }
    }

    fn now(&self) -> f64 {
        self.pass as f64 / 60.0
    }

    fn step(&mut self) {
        self.e.set_clock(self.now());
        self.e.pass(Some(&mut self.screen));
        self.e.voices().fill(&mut self.audio, 22050, 2);
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

/// The angles of the middle of texel (`col`, `row`) of cube face `face`:
/// the inverse of the warp's sampling of a face (the coordinate s of a
/// direction on the face is texel 1 + 510 (s + 1) / 2, rounded down), with
/// the angles a zone's aim takes (yaw = atan2(z, x), pitch = atan2(y,
/// √(z² + x²))).
fn texel_angles(face: usize, col: f64, row: f64) -> (f32, f32) {
    let s = |t: f64| (t - 0.5) * 2.0 / 510.0 - 1.0;
    let (u, v) = (s(col), s(row));
    let [x, y, z] = match face {
        0 => [u, v, 1.0],
        1 => [-u, v, -1.0],
        2 => [u, -1.0, v],
        3 => [u, 1.0, -v],
        4 => [-1.0, v, u],
        _ => [1.0, v, -u],
    };
    (z.atan2(x) as f32, y.atan2((z * z + x * x).sqrt()) as f32)
}

/// The sprite a click would hit now (what the warp draw records on a
/// click, @ 0x00661640): the first shown clickable slot whose box on the
/// face under the centre holds the centre's texel (the warp's sampling of
/// the centre ray, as the engine's).
fn centre_slot(e: &Engine) -> Option<usize> {
    let [x, y, z] = e.camera.ray(e.camera.cx, e.camera.cy).map(f64::from);
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, u, v) = if az >= ax && az >= ay {
        if z > 0.0 {
            (0, x / az, y / az)
        } else {
            (1, -x / az, y / az)
        }
    } else if ay >= ax {
        if y < 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if x < 0.0 {
        (4, z / ax, y / ax)
    } else {
        (5, -z / ax, y / ax)
    };
    let texel = |s: f64| (1.0 + 510.0 * (s + 1.0) / 2.0).floor() as i32;
    let (col, row) = (texel(u), texel(v));
    e.anim.slots.iter().position(|s| {
        s.active
            && s.click > 0
            && s.frame_box(face).is_some_and(|b| {
                let (bx, by, bw, bh) = (
                    i32::from(b.x),
                    i32::from(b.y),
                    i32::from(b.w),
                    i32::from(b.h),
                );
                row < by + bh + 1 && by < row && bx < col && col < bx + bw + 1
            })
    })
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
        self.take_events()
    }

    /// The screen written as `flow-ireland-<name>.png` when
    /// `ATLANTIS2_PNG_DIR` is set.
    fn shot(&self, name: &str) {
        self.shot_as(&format!("flow-ireland-{name}.png"));
    }

    /// The screen written as `file` in `ATLANTIS2_PNG_DIR`, when set.
    fn shot_as(&self, file: &str) {
        if let Some(dir) = &self.shots {
            std::fs::create_dir_all(dir).unwrap();
            let png = atlantis2_engine::png::encode_rgb565(&self.screen, 640, 480);
            std::fs::write(dir.join(file), png).unwrap();
        }
    }

    /// The events since the last take, kept in the journal too.
    fn take_events(&mut self) -> Vec<Event> {
        let evs = std::mem::take(&mut self.e.events);
        self.journal.extend(evs.iter().cloned());
        evs
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

    /// Turns the view with the mouse onto zone `zone` (the centre on it),
    /// a little off the zone's own aim as a player's hand is (a move whose
    /// turn starts within a tenth of a degree of the camera is a click that
    /// does nothing, as in the original).
    fn aim(&mut self, zone: usize) {
        let view = self.e.place().1;
        assert!(
            self.try_aim(zone, false),
            "the centre never reached zone {zone} of view {view}"
        );
    }

    /// [`Run::aim`], false when the zone cannot be reached (beyond the
    /// view's limits); with `clear`, a place of the zone with no sprite
    /// under the centre (a click there would be the sprite's).
    fn try_aim(&mut self, zone: usize, clear: bool) -> bool {
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        let on = move |e: &Engine| {
            e.centre_zone().map(|(i, _)| i) == Some(zone)
                && !(clear && e.sprite_under_centre().1 != 0)
        };
        let off = 0.6f32.to_radians();
        let mut targets = vec![
            (yaw + off, pitch),
            (yaw - off, pitch),
            (yaw, pitch + off),
            (yaw, pitch - off),
        ];
        // Then half way to each corner of the zone.
        let fpu = self.e.fpu();
        for p in &self.e.wam().view(view).unwrap().zones[zone].points {
            let (y, q) = atlantis2_engine::camera::direction_angles(fpu, *p);
            let dy = (y - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            targets.push((yaw + dy / 2.0, (pitch + q) / 2.0));
        }
        targets.into_iter().any(|(y, p)| self.steer(y, p, &on))
    }

    /// Turns the view with the mouse, as a player does, toward (`yaw`,
    /// `pitch`) until `on` holds for the engine with the turn spent: a
    /// move of d mickeys turns the view by about d / 500 radians in all
    /// (d / 2000 a frame, × 0.8 each frame), so each move aims at what is
    /// left of the way and the turn is let settle. False when `on` never
    /// holds (a direction beyond the view's limits).
    fn steer(&mut self, yaw: f32, pitch: f32, on: &dyn Fn(&Engine) -> bool) -> bool {
        let wrap = |a: f32| {
            let t = std::f32::consts::TAU;
            (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
        };
        let (mut sx, mut sy) = self.mouse_signs;
        for k in 0..24 {
            // A first move always: the hand moves to the target.
            if k > 0 && on(&self.e) {
                // Settled on it: the speeds spent.
                self.wait(24);
                if on(&self.e) {
                    self.mouse_signs = (sx, sy);
                    return true;
                }
            }
            let (y0, p0) = self.e.camera.angles();
            let (dy, dp) = (wrap(yaw - y0), pitch - p0);
            let mx = (sx * dy * 500.0).round() as i32;
            let my = (sy * dp * 500.0).round() as i32;
            if mx == 0 && my == 0 {
                break;
            }
            self.e.mouse_motion(mx, my);
            self.wait(30);
            let (y1, p1) = self.e.camera.angles();
            if mx != 0 && wrap(yaw - y1).abs() > dy.abs() {
                sx = -sx;
            }
            if my != 0 && (pitch - p1).abs() > dp.abs() {
                sy = -sy;
            }
        }
        self.mouse_signs = (sx, sy);
        self.wait(24);
        on(&self.e)
    }

    /// Where a player looks to have the sprite of slot `n` under the
    /// centre: the angles of texels of its box on each face it is drawn on
    /// (its current frame), the middle first, none under a clickable
    /// sprite drawn before it (a click there would be that one's).
    fn slot_aims(&self, n: usize) -> Vec<(f32, f32)> {
        // Another sprite's box, with a margin (its animation moves it).
        let near = |b: &atlantis2_data::spw::FaceBox, col: i32, row: i32| {
            let (x, y, w, h) = (
                i32::from(b.x) - 4,
                i32::from(b.y) - 4,
                i32::from(b.w) + 8,
                i32::from(b.h) + 8,
            );
            row < y + h + 1 && y < row && x < col && col < x + w + 1
        };
        let slots = &self.e.anim.slots;
        let mut out = Vec::new();
        for face in 0..6 {
            let Some(b) = slots[n].frame_box(face) else {
                continue;
            };
            if b.w == 0 || b.h == 0 {
                continue;
            }
            let mut texels = Vec::new();
            for i in 1..8 {
                for j in 1..8 {
                    let col = i32::from(b.x) + 1 + (i32::from(b.w) - 1) * i / 8;
                    let row = i32::from(b.y) + 1 + (i32::from(b.h) - 1) * j / 8;
                    let d = (i - 4) * (i - 4) + (j - 4) * (j - 4);
                    texels.push((d, col, row));
                }
            }
            texels.sort();
            for (_, col, row) in texels {
                let hidden = slots[..n].iter().any(|s| {
                    s.active
                        && s.click != 0
                        && s.frame_box(face).is_some_and(|c| near(&c, col, row))
                });
                if !hidden {
                    out.push(texel_angles(face, f64::from(col), f64::from(row)));
                }
            }
        }
        out.truncate(6);
        out
    }

    /// A click on the move zone to view `to`, the transition video
    /// clicked away; a dialog the arrival plays listened to. The events,
    /// in order.
    fn walk(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        // The move zones to view `to`: the first the centre can reach.
        let view = self.e.place().1;
        let zones: Vec<usize> = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.kind == 1 && z.value == to as i32)
            .map(|(i, _)| i)
            .collect();
        assert!(!zones.is_empty(), "no move from view {view} to {to}");
        assert!(
            zones.iter().any(|&z| self.try_aim(z, true)),
            "no move zone to view {to} within reach in view {view}"
        );
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
                let evs = self.take_events();
                all.extend(evs);
                if self.e.loop_type == LoopType::Dialog {
                    all.extend(self.listen());
                }
                assert_eq!(self.e.loop_type, LoopType::Warp);
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// Turns the view with the mouse onto each clickable sprite of the view
    /// in turn and clicks it, until the click's events hold `done`; those
    /// events.
    fn click_sprite(&mut self, done: impl Fn(&Event) -> bool) -> Vec<Event> {
        let n = self.e.anim.slots.len();
        for k in 0..n {
            if let Some(evs) = self.click_slot(k) {
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

    /// The sprite of slot `n`, when shown and clickable, turned to with
    /// the mouse and clicked: the click's events.
    fn click_slot(&mut self, n: usize) -> Option<Vec<Event>> {
        let s = &self.e.anim.slots[n];
        if !s.active || s.click == 0 {
            return None;
        }
        for (yaw, pitch) in self.slot_aims(n) {
            let on = |e: &Engine| e.sprite_under_centre().1 != 0 && centre_slot(e) == Some(n);
            if self.steer(yaw, pitch, &on) {
                self.tap();
                return Some(self.events());
            }
        }
        None
    }

    /// A click on the shown sprite of character (`character`) or object
    /// `id`, turned to with the mouse: the click's events.
    fn click_on(&mut self, character: bool, id: i32) -> Vec<Event> {
        let n = self.e.anim.slots.len();
        for k in 0..n {
            let s = &self.e.anim.slots[k];
            if s.id != id || matches!(s.kind, 1 | 3) != character {
                continue;
            }
            if let Some(evs) = self.click_slot(k) {
                return evs;
            }
        }
        panic!(
            "{} {id} not clickable in view {}",
            if character { "character" } else { "object" },
            self.e.place().1
        );
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
        self.choose(id);
        assert_eq!(self.var("OBJETENMAIN"), id);
    }

    /// The bag bar opened with the right button, scrolled with the pointer
    /// at its edge until object `id` shows (one cell a tenth of a second),
    /// and its cell clicked; the bar closed.
    fn choose(&mut self, id: i32) {
        if self.e.bar.open {
            self.e.right_click();
            self.step();
        }
        self.e.right_click();
        assert!(self.e.bar.open);
        self.step();
        let mut right = true;
        for _ in 0..400 {
            let bag = self.e.bag_entries();
            if let Some(k) = self.e.bar.shown.iter().position(|&n| bag[n].id == id) {
                self.point(40 + 80 * k as i32, 440);
                self.step();
                self.tap();
                self.step();
                assert!(!self.e.bar.open);
                return;
            }
            // Out of the cells: the pointer at the bar's right edge, then at
            // its left one.
            let first = self.e.bar.first;
            if right {
                self.point(636, 440);
            } else {
                self.point(1, 440);
            }
            self.wait(8);
            if self.e.bar.first == first {
                assert!(right, "object {id} not in the bag");
                right = false;
            }
        }
        panic!("object {id} never shown in the bar");
    }
}

/// The game from the start-up intro to Ireland, with the mouse: the
/// shaman asked, Ireland's boat taken and laid, the trip, the arrival's
/// dialog, Liam asked (chapter 5). The laid boat's order variable and
/// `pose` flag.
fn to_ireland(r: &mut Run) -> (&'static str, String) {
    tibet_start(r);
    to_ireland_from_view_2(r)
}

/// The start-up intro ended, a new game, its film clicked away, the
/// first click (chapter 75), in `tibet.wam` view 2.
fn tibet_start(r: &mut Run) {
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
}

/// [`to_ireland`] from view 2 of a new game's chapter 75.
fn to_ireland_from_view_2(r: &mut Run) -> (&'static str, String) {
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
    (order, pose)
}

#[test]
fn a_game_from_tibet_to_ireland_and_back() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow-tibet");
    let mut r = Run::new(&dir);
    let (order, pose) = to_ireland(&mut r);

    // Ireland played to the end of its story, stage by stage, with the
    // mouse.
    for (name, stage) in IRELAND {
        stage(&mut r);
        assert_eq!(r.e.loop_type, LoopType::Warp, "after {name}");
        assert!(r.e.events.is_empty() || r.e.loop_type == LoopType::Warp);
    }
    let journal = std::mem::take(&mut r.journal);
    ireland_lines_as_the_original(&journal);
    ireland_chapters_ended(&r.e, &journal);
    // Every puzzle hook ran as the original's, none is left noted.
    assert!(!journal
        .iter()
        .any(|e| matches!(e, Event::PuzzleNotPorted { .. })));
    for function in [
        0x0049_4120u32,
        0x0049_4190,
        0x0049_3cb0,
        0x0049_3d40,
        0x0049_6020,
        0x0049_60a0,
        0x0049_58f0,
    ] {
        assert!(
            journal.contains(&Event::PuzzleRan { function }),
            "FUN_{function:08x} never ran"
        );
    }
    let goddess = r.id(defnames::PERSONNAGES, "DEESSE");

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

    // The way back as chapter 16 makes it, now that the story has set its
    // flag (`flag71` = 1, Nuada's sword given back): out of the book by
    // view 55's link, views 4, 3 (chapter 16), 2; leaving view 2 for view
    // 1 plays film 19 with dialog 71 over it, and the game is back in
    // Tibet, view 7, chapter 75, Ireland's mandala in the bag and in hand.
    r.route(&[60, 54, 55]);
    r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("bigile1.wam", 5), 5));
    r.route(&[4, 3, 2]);
    assert_eq!(r.chapter(), 16);
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

/// The play of Ireland, with the mouse: what a player does, step by step.
impl Run {
    fn chapter(&self) -> i32 {
        self.e.script_state().unwrap().chapter
    }

    fn flag(&self, n: usize) -> i32 {
        self.e.script_state().unwrap().flags[n]
    }

    fn obj(&self, name: &str) -> i32 {
        self.id(defnames::OBJETS, name)
    }

    fn who(&self, name: &str) -> i32 {
        self.id(defnames::PERSONNAGES, name)
    }

    fn in_bag(&self, name: &str) -> bool {
        let id = self.obj(name);
        self.e.bag_entries().iter().any(|b| b.id == id)
    }

    /// Whether a slot of character (or object) `name` is shown here.
    fn shown(&self, character: bool, name: &str) -> bool {
        let id = if character {
            self.who(name)
        } else {
            self.obj(name)
        };
        self.e
            .anim
            .slots
            .iter()
            .any(|s| s.id == id && s.active && matches!(s.kind, 1 | 3) == character)
    }

    /// Walks through `views` in order.
    fn route(&mut self, views: &[u32]) {
        for &v in views {
            self.walk(v);
        }
    }

    /// A film clicked away after a second.
    fn watch(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..60 {
            all.extend(self.events());
            if self.e.loop_type != LoopType::Video {
                return all;
            }
        }
        self.tap();
        for _ in 0..3000 {
            all.extend(self.events());
            if self.e.loop_type != LoopType::Video {
                return all;
            }
        }
        panic!("the film does not end");
    }

    /// What an action starts played out: films clicked away, dialogs
    /// listened to, until the panorama is back and still.
    fn settle(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        let mut still = 0;
        for _ in 0..20_000 {
            match self.e.loop_type {
                LoopType::Video => {
                    all.extend(self.watch());
                    still = 0;
                }
                LoopType::Dialog => {
                    all.extend(self.listen());
                    still = 0;
                }
                _ => {
                    all.extend(self.events());
                    still += 1;
                    if still > 20 && !self.e.turning() {
                        return all;
                    }
                }
            }
        }
        panic!("the game never settles");
    }

    /// The object in hand put back in the bag (the bar opened and closed).
    fn stow(&mut self) {
        if self.var("OBJETENMAIN") != 0 {
            self.e.right_click();
            self.step();
            self.e.right_click();
            self.step();
            assert_eq!(self.var("OBJETENMAIN"), 0);
        }
    }

    /// Object `name` taken with a click on its sprite.
    fn take(&mut self, name: &str) -> Vec<Event> {
        let id = self.obj(name);
        let mut evs = self.click_on(false, id);
        assert!(evs.contains(&Event::Take { id }), "{name} not taken");
        evs.extend(self.settle());
        assert!(self.in_bag(name), "{name} not in the bag");
        self.stow();
        evs
    }

    /// Character `who` asked about `subject` through his menu: the dialog
    /// its answer starts.
    fn ask(&mut self, who: &str, subject: &str) -> u32 {
        let character = self.who(who);
        let value = self.subject(subject);
        self.stow();
        let evs = self.click_on(true, character);
        assert!(
            evs.iter()
                .any(|e| matches!(e, Event::SubjectMenu { character: c, .. } if *c == character)),
            "{who}: no subject menu"
        );
        let list = self.e.script_state().unwrap().subjects.list(character);
        let i = list
            .iter()
            .position(|e| e.value == value)
            .unwrap_or_else(|| panic!("{who} offers no {subject}"));
        let (x, y) = cell(list.len() as i32, i as i32);
        self.point(x + 31, y + 31);
        self.step();
        self.tap();
        let mut evs = self.events();
        assert!(evs.contains(&Event::Subject {
            character,
            value: Some(value)
        }));
        evs.extend(self.settle());
        self.stow();
        dialogs(&evs)
            .first()
            .copied()
            .unwrap_or_else(|| panic!("{who} {subject}: no dialog"))
    }

    /// Object `object` picked from the bag and shown to character `who`
    /// with a click on him: what follows, played out.
    fn give(&mut self, object: &str, who: &str) -> Vec<Event> {
        let id = self.obj(object);
        self.pick(id);
        let character = self.who(who);
        let mut evs = self.click_on(true, character);
        evs.extend(self.settle());
        evs
    }

    /// Object `object` (none: an empty hand) used on zone `zone` of the
    /// view with a click: what follows, played out.
    fn use_on(&mut self, object: Option<&str>, zone: usize) -> Vec<Event> {
        match object {
            Some(o) => {
                let id = self.obj(o);
                self.pick(id);
            }
            None => self.stow(),
        }
        self.aim(zone);
        self.tap();
        let mut evs = self.events();
        evs.extend(self.settle());
        evs
    }

    /// The zone of the view whose click value is `#ZONES_CLIC#`'s `name`
    /// (a type-3 zone) or the number `value` (a type-4 one).
    fn zone_named(&self, name: &str) -> usize {
        let value = self.id(defnames::ZONES_CLIC, name);
        let view = self.e.place().1;
        self.e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 3 && z.value == value)
            .unwrap_or_else(|| panic!("no zone {name} in view {view}"))
    }

    fn zone_valued(&self, value: i32) -> usize {
        let view = self.e.place().1;
        self.e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 4 && z.value == value)
            .unwrap_or_else(|| panic!("no zone of value {value} in view {view}"))
    }

    /// A click on the view's link to another place (its type-2 zone),
    /// until the arrival; the events.
    fn walk_link(&mut self) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let zone = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 2)
            .unwrap_or_else(|| panic!("no link in view {view}"));
        let wam = self.e.place().0.to_string();
        self.aim(zone);
        self.tap();
        let mut all = Vec::new();
        for _ in 0..3000 {
            all.extend(self.events());
            if self.e.loop_type == LoopType::Video {
                all.extend(self.watch());
            }
            if self.e.place().0 != wam && self.e.loop_type == LoopType::Warp {
                all.extend(self.settle());
                return all;
            }
        }
        panic!("the link of view {view} leads nowhere");
    }
}

/// The dialogs the events started, in order.
fn dialogs(evs: &[Event]) -> Vec<u32> {
    evs.iter()
        .filter_map(|e| match e {
            Event::DialogStart {
                number,
                started: true,
            } => Some(*number),
            _ => None,
        })
        .collect()
}

/// The films the events started, by number.
fn films(evs: &[Event]) -> Vec<u32> {
    evs.iter()
        .filter_map(|e| match e {
            Event::VideoStart { name, .. } => name
                .strip_prefix("cine")
                .and_then(|n| n.strip_suffix(".hnm"))
                .and_then(|n| n.parse().ok()),
            _ => None,
        })
        .collect()
}

/// The blackbird songs and the horse: the two plays timed or heard.
impl Run {
    /// A round of the blackbird songs: the sitting poet clicked (his menu,
    /// the game's mode 1), subject `k` of his menu clicked with the
    /// pointer on the game's grid, his question heard: the song he names
    /// (dialog 89 + song).
    fn blackbird_question(&mut self, poet: i32, k: i32) -> i32 {
        let evs = self.click_on(true, poet);
        assert!(evs
            .iter()
            .any(|e| matches!(e, Event::SubjectMenu { character, .. } if *character == poet)));
        assert_eq!(self.e.blackbird.mode, 1, "the poet's click sets mode 1");
        let (x, y) = cell(self.e.subject_count(poet), k);
        self.wait(2);
        self.point(x + 25, y + 25);
        self.wait(2);
        self.tap();
        let evs = self.events();
        let question = dialogs(&evs)[0] as i32;
        assert!((90..=99).contains(&question));
        assert_eq!(self.e.blackbird.mode, 2);
        self.wait(30);
        self.listen();
        self.step();
        assert!(self.e.subject_menu.open);
        question - 89
    }

    /// The pointer over each bird in turn, each song heard to its end:
    /// the song of each bird.
    fn hear_the_birds(&mut self) -> [i32; 5] {
        let mut songs = [0; 5];
        for (k, song) in songs.iter_mut().enumerate() {
            self.point(450, 180 + 62 * k as i32);
            for _ in 0..1200 {
                let evs = self.events();
                if let Some(n) = evs.iter().find_map(|x| match x {
                    Event::Sound(n) => n.strip_prefix("merle").and_then(|n| n.strip_suffix(".apc")),
                    _ => None,
                }) {
                    *song = n.parse().unwrap_or(-1);
                    break;
                }
            }
            assert_ne!(*song, 0, "bird {k}: no song heard");
            for _ in 0..1200 {
                if !self.e.voices().is_playing(VOICE_EFFECTS) {
                    break;
                }
                self.step();
            }
        }
        songs
    }

    /// In view 27 with the horse running: the view turned with the mouse
    /// where the horse passes while its frame is between 570 and 610 (the
    /// texel its sprite covers on most of those frames), and a click on it
    /// while it is there (chapter 3's timed section takes it).
    fn catch_the_horse(&mut self) {
        let horse = self.obj("CHEVAL");
        let slot = |e: &Engine| e.anim.slots.iter().position(|s| s.id == horse);
        let n = slot(&self.e).expect("the horse in view 27");
        let name = self.e.anim.slots[n].name.clone();
        let frame = self.e.anim.slots[n].frame;
        assert!(frame < 500, "the horse already gone by (frame {frame})");
        let spw = atlantis2_data::spw::Spw::parse(
            self.e
                .files()
                .read(atlantis2_data::Folder::Spw, &name)
                .unwrap(),
        )
        .unwrap();
        // The texel the most of frames 575..605 cover.
        let mut best = (0, 0, 0, 0);
        for face in 0..6 {
            let boxes: Vec<_> = (575..605)
                .filter_map(|f| spw.face_box(f, face))
                .filter(|b| b.w > 0 && b.h > 0)
                .collect();
            for b in &boxes {
                let (col, row) = (
                    i32::from(b.x) + i32::from(b.w.div_ceil(2)),
                    i32::from(b.y) + i32::from(b.h.div_ceil(2)),
                );
                let n = boxes
                    .iter()
                    .filter(|c| {
                        let (x, y, w, h) = (
                            i32::from(c.x),
                            i32::from(c.y),
                            i32::from(c.w),
                            i32::from(c.h),
                        );
                        row < y + h + 1 && y < row && x < col && col < x + w + 1
                    })
                    .count();
                if n > best.0 {
                    best = (n, face, col, row);
                }
            }
        }
        assert!(best.0 > 0, "the horse never shows in view 27 then");
        // Wait for it, looking there.
        while self.e.anim.slots[slot(&self.e).unwrap()].frame < 500 {
            self.step();
        }
        let (yaw, pitch) = texel_angles(best.1, f64::from(best.2), f64::from(best.3));
        self.steer(yaw, pitch, &|_| false);
        for _ in 0..600 {
            let f = self.e.anim.slots[slot(&self.e).unwrap()].frame;
            if f > 572 && f < 608 && self.e.sprite_under_centre().1 != 0 {
                self.tap();
                for _ in 0..30 {
                    self.events();
                    if self.in_bag("CHEVAL") {
                        return;
                    }
                }
            }
            assert!(f < 610, "the horse not caught");
            self.step();
        }
        panic!("the horse never came");
    }
}

/// A stage of the play: its name and what the player does.
type Stage = (&'static str, fn(&mut Run));

/// Ireland's story, stage by stage, as chapters 2-24 make it: each stage
/// starts and ends in the panorama, the game where the last one left it.
const IRELAND: &[Stage] = &[
    ("the book", the_book),
    ("Nuada and Dian Cecht", nuada_and_dian_cecht),
    ("the sword drawn", the_sword_drawn),
    ("the chapel's skull", the_chapels_skull),
    ("Finbar", finbar),
    ("Liam's chest", liams_chest),
    (
        "the fork and the sheep's skull",
        the_fork_and_the_sheeps_skull,
    ),
    ("the fox", the_fox),
    ("the beehive", the_beehive),
    ("the well's skull", the_wells_skull),
    ("the high cross", the_high_cross),
    ("the weeping head", the_weeping_head),
    ("the skull puzzle", the_skull_puzzle),
    ("Ailill", ailill),
    ("the ogham stones", the_ogham_stones),
    ("the menhir and the horse", the_menhir_and_the_horse),
    ("the big horse", the_big_horse),
    ("the blackbird songs", the_blackbird_songs),
    ("the dowsing rod", the_dowsing_rod),
    ("the serpent's well, first", the_serpents_well_first),
    ("Dian Cecht's statue", dian_cechts_statue),
    ("Ailill's statue", ailills_statue),
    ("Brigid's cross", brigids_cross),
    ("the salmon", the_salmon),
    ("the serpent", the_serpent),
    ("Nuada's hand", nuadas_hand),
];

/// Chapter 5, view 5 (after Liam's first answer): the pencil taken, and
/// held while leaving view 38 for the book (chapter 2's and 5's
/// departure sections: the move is cancelled without it); film 18, the
/// book's place (`livre.wam`), chapter 4, whose `/INIT` puts Liam in his
/// cell.
fn the_book(r: &mut Run) {
    assert_eq!((r.e.place(), r.chapter()), (("bigile1.wam", 5), 5));
    r.take("CRAYON");
    r.walk(38);
    let crayon = r.obj("CRAYON");
    r.pick(crayon);
    let evs = r.walk(50);
    assert!(films(&evs).contains(&18));
    assert_eq!((r.e.place(), r.chapter()), (("livre.wam", 50), 4));
    assert_eq!(r.flag(30), 1);
}

/// In the book: Dian Cecht asked about Nuada (dialog 8: the standing
/// poet's glass tower subject), Nuada met (dialog 4, flag 7), and the way
/// out by view 55's link (chapter 4: back to `bigile1.wam` view 5,
/// chapter 5).
fn nuada_and_dian_cecht(r: &mut Run) {
    r.route(&[51, 56]);
    assert_eq!(r.ask("DIAN_CECHT", "NUADA"), 8);
    assert_eq!(r.flag(73), 1);
    r.route(&[59, 61]);
    let nuada = r.who("NUADA");
    let mut evs = r.click_on(true, nuada);
    evs.extend(r.settle());
    assert_eq!(dialogs(&evs), [4]);
    assert_eq!(r.flag(7), 1);
    r.route(&[60, 54, 55]);
    r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("bigile1.wam", 5), 5));
}

/// View 38: the pencil on Nuada's page draws his sword (chapter 5,
/// flag 9): the sword shows in the book.
fn the_sword_drawn(r: &mut Run) {
    r.walk(38);
    let zone = r.zone_named("NUADA");
    r.use_on(Some("CRAYON"), zone);
    assert_eq!(r.flag(9), 1);
    r.stow();
}

/// Views 4, 3 (chapter 16), 2: the chapel's skull taken.
fn the_chapels_skull(r: &mut Run) {
    r.route(&[5, 4, 3]);
    assert_eq!(r.chapter(), 16);
    r.walk(2);
    r.take("CRANE_CHAPELLE");
}

/// Views 1 (chapter 12), 7 (3), 6 (15: Liam's ogham subject), 7, 12, 13:
/// Finbar met (dialog 30: Liam's Finbar subject).
fn finbar(r: &mut Run) {
    r.walk(1);
    assert_eq!(r.chapter(), 12);
    r.walk(7);
    assert_eq!(r.chapter(), 3);
    r.walk(6);
    assert_eq!(r.chapter(), 15);
    r.walk(7);
    assert_eq!(r.chapter(), 3);
    r.route(&[12, 13]);
    let finbar = r.who("FINBAR");
    let mut evs = r.click_on(true, finbar);
    evs.extend(r.settle());
    assert_eq!(dialogs(&evs), [30]);
}

/// The jug taken in view 11; view 10 (chapter 19): Liam in his cell asked
/// about Finbar (dialog 81) and the ogham puzzle (dialog 55: his key);
/// the key opens his chest: his knife, then the parchment.
fn liams_chest(r: &mut Run) {
    r.route(&[12, 11]);
    r.take("CRUCHE");
    r.walk(10);
    assert_eq!(r.chapter(), 19);
    assert_eq!(r.ask("LIAM_CELLULE", "FINBAR"), 81);
    assert_eq!(r.ask("LIAM_CELLULE", "PUZZLE_OGHAM"), 55);
    assert!(r.in_bag("CLEF_LIAM"));
    let key = r.obj("CLEF_LIAM");
    r.pick(key);
    let chest = r.obj("COFFRE_FERME");
    r.click_on(false, chest);
    r.settle();
    assert!(r.shown(false, "COFFRE_OCP1"));
    let open = r.obj("COFFRE_OCP1");
    r.click_on(false, open);
    r.settle();
    assert!(r.in_bag("COUTEAU_LIAM"));
    let open = r.obj("COFFRE_OP1");
    r.click_on(false, open);
    r.settle();
    assert!(r.in_bag("PARCHEMIN"));
    r.stow();
}

/// Views 11 (chapter 3), 12, 13, 14, 15: the fork; 14, 16, 17, 18: the
/// skull among the sheep.
fn the_fork_and_the_sheeps_skull(r: &mut Run) {
    r.walk(11);
    assert_eq!(r.chapter(), 3);
    r.route(&[12, 13, 14, 15]);
    r.take("FOURCHE");
    r.route(&[14, 16, 17, 18]);
    r.take("CRANE_W18");
}

/// Views 19, 20, 21 (chapter 13): the fox clicked runs off and leaves the
/// well's skull to find.
fn the_fox(r: &mut Run) {
    r.route(&[19, 20, 21]);
    assert_eq!(r.chapter(), 13);
    let fox = r.who("RENARD");
    r.click_on(true, fox);
    r.settle();
    assert_eq!(r.var("flagrenard"), 1);
}

/// Views 22 (chapter 3), 23, 24, 25, 26 (11): the beehive's skull.
fn the_beehive(r: &mut Run) {
    r.walk(22);
    assert_eq!(r.chapter(), 3);
    r.route(&[23, 24, 25, 26]);
    assert_eq!(r.chapter(), 11);
    r.take("CRANE_RUCHE");
}

/// Views 27 (chapter 3), 28, 33, 32, 13, 12, 11, 39: the well's skull.
fn the_wells_skull(r: &mut Run) {
    r.walk(27);
    assert_eq!(r.chapter(), 3);
    r.route(&[28, 33, 32, 13, 12, 11, 39]);
    r.take("CRANE_PUIT");
}

/// Views 8, 9, 37 (chapter 9): the fork knocks the skull off the high
/// cross (flag 34); views 9 (3), 8, 7, 1 (12), 2 (16), 3: taken there.
fn the_high_cross(r: &mut Run) {
    r.route(&[8, 9, 37]);
    assert_eq!(r.chapter(), 9);
    let zone = r.zone_named("HAUT_CROIX");
    r.use_on(Some("FOURCHE"), zone);
    assert_eq!(r.flag(34), 1);
    r.stow();
    r.walk(9);
    assert_eq!(r.chapter(), 3);
    r.route(&[8, 7, 1, 2, 3]);
    r.take("CRANE_CROIX");
}

/// Views 2, 1 (chapter 12): the jug filled at the weeping head.
fn the_weeping_head(r: &mut Run) {
    r.route(&[2, 1]);
    assert_eq!(r.chapter(), 12);
    let zone = r.zone_named("TETE_PLEURANTE");
    r.use_on(Some("CRUCHE"), zone);
    assert!(r.in_bag("CRUCHE_EAU"));
    r.stow();
}

/// Views 7 (chapter 3), 12, 13, 32, 33, 28, 40 (7), 41, 42: the five
/// skulls laid on the skull's place, each from the bag; at the fifth,
/// film 2 and Ailill.
fn the_skull_puzzle(r: &mut Run) {
    r.route(&[7, 12, 13, 32, 33, 28, 40]);
    assert_eq!(r.chapter(), 7);
    r.route(&[41, 42]);
    r.shot("1-skull-view-42");
    let mut evs = Vec::new();
    for piece in [
        "CRANE_CHAPELLE",
        "CRANE_W18",
        "CRANE_RUCHE",
        "CRANE_PUIT",
        "CRANE_CROIX",
    ] {
        let zone = r.zone_named("CRANE_ZONE");
        evs.extend(r.use_on(Some(piece), zone));
        assert!(!r.in_bag(piece), "{piece} not laid");
        if piece == "CRANE_PUIT" {
            r.shot("2-skull-four-laid");
        }
    }
    r.shot("3-skull-ailill");
    assert!(films(&evs).contains(&2));
    assert_eq!(r.var("fincrane"), 6);
    assert!(r.shown(true, "AILILL"));
}

/// Ailill given the water (dialog 32, flag 28), then Liam's knife: his
/// own in exchange (dialog 41).
fn ailill(r: &mut Run) {
    let evs = r.give("CRUCHE_EAU", "AILILL");
    assert_eq!(dialogs(&evs), [32]);
    assert_eq!(r.flag(28), 1);
    r.stow();
    let evs = r.give("COUTEAU_LIAM", "AILILL");
    assert_eq!(dialogs(&evs), [41]);
    assert!(r.in_bag("COUTEAU_AILILL"));
    r.stow();
}

/// Views 41, 40, 28 (chapter 3), 33, 32, 13, 12, 7, 6 (15): the ogham
/// stones, Ailill's knife in hand, solved by the original's shortest
/// path, then the check zone: `FINOGHAM`, film 3, the ogham stick in
/// view 106, taken.
fn the_ogham_stones(r: &mut Run) {
    r.route(&[41, 40, 28]);
    assert_eq!(r.chapter(), 3);
    r.route(&[33, 32, 13, 12, 7, 6]);
    assert_eq!(r.chapter(), 15);
    r.shot("4-ogham-stones");
    let knife = r.obj("COUTEAU_AILILL");
    r.pick(knife);
    let golden = include_str!("golden/ogham.txt");
    let solution: Vec<i32> = golden
        .lines()
        .find(|l| l.starts_with("D the whole puzzle"))
        .unwrap()
        .split(": ")
        .last()
        .unwrap()
        .split(' ')
        .map(|z| z.parse().unwrap())
        .collect();
    for z in solution {
        let zone = r.zone_valued(z);
        r.aim(zone);
        r.tap();
        let evs = r.events();
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_60a0
        }));
        r.wait(2);
    }
    assert_eq!(r.e.ogham, atlantis2_engine::engine::OGHAM_TARGET);
    r.shot("5-ogham-solved");
    let zone = r.zone_valued(99);
    let evs = r.use_on(Some("COUTEAU_AILILL"), zone);
    assert!(films(&evs).contains(&3));
    assert_eq!(r.var("FINOGHAM"), 1);
    assert_eq!(r.e.place(), ("bigile1.wam", 106));
    r.stow();
    r.take("BATON_OGHAM");
}

/// Views 7, 12, 13, 32, 31, 30 (chapter 8): the ogham stick on the
/// menhir wakes the little horse (film 5, flag 32), which runs from view
/// to view; caught in view 27 when it passes there (chapter 3's timed
/// section: its frame between 570 and 610).
fn the_menhir_and_the_horse(r: &mut Run) {
    r.route(&[7, 12, 13, 32, 31, 30]);
    assert_eq!(r.chapter(), 8);
    let zone = r.zone_named("MENHIR");
    let evs = r.use_on(Some("BATON_OGHAM"), zone);
    assert!(films(&evs).contains(&5));
    assert_eq!(r.flag(32), 1);
    r.walk(29);
    assert_eq!(r.chapter(), 3);
    r.route(&[28, 27]);
    r.catch_the_horse();
    r.shot("6-horse-caught");
    assert!(r.in_bag("CHEVAL"));
    r.stow();
}

/// Views 34, 35 (chapter 14): the little horse in the water grows (film
/// 6); the big horse clicked carries the hero to the poet's island (film
/// 7, `ilepoete.wam` view 71, chapter 20).
fn the_big_horse(r: &mut Run) {
    r.route(&[34, 35]);
    assert_eq!(r.chapter(), 14);
    let zone = r.zone_named("EAU");
    let evs = r.use_on(Some("CHEVAL"), zone);
    assert!(films(&evs).contains(&6));
    let horse = r.who("GRAND_CHEVAL");
    let mut evs = r.click_on(true, horse);
    evs.extend(r.settle());
    assert!(films(&evs).contains(&7));
    assert_eq!((r.e.place(), r.chapter()), (("ilepoete.wam", 71), 20));
}

/// Views 72, 73: the sitting poet's blackbird songs, played by ear: each
/// round a subject of his menu, his question (the song he names), each
/// bird heard with the pointer on it, the right one clicked; three right
/// in a row: film 14, the standing poet, dialog 73. Then he is asked
/// about himself (dialog 64, flag 50) and the glass tower (dialog 62:
/// the dowsing rod).
fn the_blackbird_songs(r: &mut Run) {
    r.route(&[72, 73]);
    let poet = r.who("POETE_ASSIS");
    let mut evs = Vec::new();
    for round in 0..3 {
        let song = r.blackbird_question(poet, round);
        let songs = r.hear_the_birds();
        if round == 0 {
            r.shot("7-blackbirds");
        }
        let right = songs
            .iter()
            .position(|&s| s == song)
            .expect("the asked song is one of the birds'") as i32;
        r.point(450, 180 + 62 * right);
        r.step();
        r.tap();
        let e = r.events();
        assert!(
            e.contains(&Event::Sound("merleb.apc".into())),
            "round {round}"
        );
        evs.extend(e);
        if round < 2 {
            r.wait(30);
        }
    }
    evs.extend(r.settle());
    assert!(films(&evs).contains(&14));
    assert!(dialogs(&evs).contains(&73));
    assert_eq!(r.var("FINMERLE"), 2);
    assert!(r.shown(true, "POETE_DEBOUT"));
    r.shot("8-blackbirds-won");
    assert_eq!(r.ask("POETE_DEBOUT", "POETE"), 64);
    assert_eq!(r.flag(50), 1);
    assert_eq!(r.ask("POETE_DEBOUT", "TOUR_VERRE"), 62);
    assert!(r.in_bag("BATON_SOURCE"));
}

/// Views 74, 77: the dowsing rod over the well (film 9): the island's
/// second form (`ilepoete2.wam` view 177, flag 70).
fn the_dowsing_rod(r: &mut Run) {
    r.route(&[74, 77]);
    let rod = r.obj("BATON_SOURCE");
    r.choose(rod);
    assert_eq!(r.var("SOURCIER"), 1);
    let zone = r.zone_named("PUIT_ZONE");
    r.aim(zone);
    r.tap();
    let mut evs = r.events();
    evs.extend(r.settle());
    assert!(films(&evs).contains(&9));
    assert_eq!(r.e.place(), ("ilepoete2.wam", 177));
    assert_eq!(r.flag(70), 1);
}

/// View 180 (chapter 24, `puit.wam`): the serpent's well, whose `/INIT`
/// gives the poet, the girl of the book and Dian Cecht their serpent and
/// well subjects; back to view 177 (chapter 20).
fn the_serpents_well_first(r: &mut Run) {
    r.walk(180);
    assert_eq!((r.e.place(), r.chapter()), (("puit.wam", 180), 24));
    assert_eq!(r.flag(8), 1);
    r.walk(177);
    assert_eq!((r.e.place(), r.chapter()), (("ilepoete2.wam", 177), 20));
}

/// View 171: the big horse back to `bigile1.wam` view 35 (film 8, chapter
/// 14); the walk to the book (views 34, 27, 28, 33, 32, 13, 12, 7, 1, 2,
/// 3, 4, 5, 38 and the pencil); Dian Cecht asked about the statue now
/// that the poet has spoken (dialog 23: Ailill's statue subject).
fn dian_cechts_statue(r: &mut Run) {
    r.walk(171);
    let horse = r.who("GRAND_CHEVAL");
    let mut evs = r.click_on(true, horse);
    evs.extend(r.settle());
    assert!(films(&evs).contains(&8));
    assert_eq!((r.e.place(), r.chapter()), (("bigile1.wam", 35), 14));
    r.walk(34);
    assert_eq!(r.chapter(), 3);
    r.route(&[27, 28, 33, 32, 13, 12, 7, 1, 2, 3, 4, 5, 38]);
    let crayon = r.obj("CRAYON");
    r.pick(crayon);
    r.walk(50);
    assert_eq!(r.chapter(), 4);
    r.route(&[51, 56]);
    assert_eq!(r.ask("DIAN_CECHT", "STATUE"), 23);
    r.route(&[54, 55]);
    r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("bigile1.wam", 5), 5));
}

/// Views 4, 3 (chapter 16), 2, 1 (12), 7 (3), 12, 13, 32, 33, 28, 40 (7),
/// 41, 42: Ailill asked about the statue (dialog 35, flag 51: Liam's
/// ancient sun subject).
fn ailills_statue(r: &mut Run) {
    r.route(&[4, 3, 2, 1, 7, 12, 13, 32, 33, 28, 40, 41, 42]);
    assert_eq!(r.chapter(), 7);
    assert_eq!(r.ask("AILILL", "STATUE"), 35);
    assert_eq!(r.flag(51), 1);
}

/// Views 41, 40, 28 (chapter 3), 33, 32, 13, 12, 11, 10 (19): Liam gives
/// Brigid's cross for the ancient sun (dialog 76); views 9 (3), 8, 7, 1
/// (12), 2 (16): the cross's place. A place clicked with an empty hand
/// (sound 9), the cross laid on place 5 (drawn, sounds 8 and 9), another
/// place clicked (sound 9), the cross taken back, laid on place 1:
/// `FINCROIX`, chapter 16's section: film 15, the goddess, dialog 56.
/// She is asked about the statue (dialog 60: the girl of the book shown,
/// flag 22).
fn brigids_cross(r: &mut Run) {
    r.route(&[41, 40, 28]);
    assert_eq!(r.chapter(), 3);
    r.route(&[33, 32, 13, 12, 11, 10]);
    assert_eq!(r.chapter(), 19);
    assert_eq!(r.ask("LIAM_CELLULE", "ANCIENT_SOLEIL"), 76);
    assert!(r.in_bag("CROIX_BRIGID"));
    r.walk(9);
    assert_eq!(r.chapter(), 3);
    r.route(&[8, 7, 1, 2]);
    assert_eq!(r.chapter(), 16);
    // A place clicked with an empty hand: the hook (FUN_00494190) plays
    // sound 9, the click goes on (`CLICZONE`, the click scan).
    let zone = r.zone_valued(3);
    let evs = r.use_on(None, zone);
    assert!(evs.iter().any(|e| matches!(e, Event::PuzzleHook { .. })));
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_4190
    }));
    assert!(evs.contains(&Event::Sound("sound009.apc".into())));
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::ClickScan(v) if v.cliczone == 3)));
    assert_eq!(r.e.cross_state(), 0);
    // Laid on place 5, another place clicked, taken back.
    let zone = r.zone_valued(5);
    let evs = r.use_on(Some("CROIX_BRIGID"), zone);
    assert!(evs.contains(&Event::CrossLaid { value: 5 }));
    assert!(evs.contains(&Event::Sound("sound008.apc".into())));
    r.shot("9-cross-on-place-5");
    assert_eq!(r.e.cross_state(), 5);
    let zone = r.zone_valued(7);
    let evs = r.use_on(None, zone);
    assert!(evs.contains(&Event::Sound("sound009.apc".into())));
    let zone = r.zone_valued(5);
    let evs = r.use_on(None, zone);
    assert!(evs.contains(&Event::CrossTaken { value: 5 }));
    assert_eq!(r.e.cross_state(), 0);
    r.stow();
    // Laid on place 1: the goddess.
    let zone = r.zone_valued(1);
    let evs = r.use_on(Some("CROIX_BRIGID"), zone);
    assert!(evs.contains(&Event::CrossLaid { value: 1 }));
    assert_eq!((r.e.cross_state(), r.var("FINCROIX")), (1, 1));
    assert!(films(&evs).contains(&15));
    assert!(dialogs(&evs).contains(&56));
    assert_eq!(r.var("flagdeesse"), 1);
    assert!(r.shown(true, "DEESSE"));
    r.shot("10-cross-goddess");
    assert_eq!(r.ask("DEESSE", "STATUE"), 60);
    assert_eq!(r.flag(22), 1);
    assert!(!r.in_bag("CROIX_BRIGID"));
}

/// Views 3, 4 (chapter 5), 5, 38 and the pencil: the book (chapter 4;
/// dialog 72 in view 56, flag 22 being set); the girl of the book asked
/// about the serpent (dialog 20, flag 16); out by view 55's link; view
/// 38: the pencil on the girl's page draws the salmon (flag 17); the
/// book again: the girl gives the salmon (dialog 22).
fn the_salmon(r: &mut Run) {
    r.route(&[3, 4]);
    assert_eq!(r.chapter(), 5);
    r.route(&[5, 38]);
    let crayon = r.obj("CRAYON");
    r.pick(crayon);
    r.walk(50);
    assert_eq!(r.chapter(), 4);
    r.walk(51);
    let evs = r.walk(56);
    assert!(dialogs(&evs).contains(&72));
    assert!(r.shown(true, "FILLE_LIVRE"));
    assert_eq!(r.ask("FILLE_LIVRE", "SERPENT"), 20);
    assert_eq!(r.flag(16), 1);
    r.route(&[54, 55]);
    r.walk_link();
    r.walk(38);
    let zone = r.zone_named("FILLE_LIVRE");
    r.use_on(Some("CRAYON"), zone);
    assert_eq!(r.flag(17), 1);
    r.walk(50);
    assert_eq!(r.chapter(), 4);
    r.route(&[51, 56]);
    assert_eq!(r.ask("FILLE_LIVRE", "SERPENT"), 22);
    assert!(r.in_bag("SAUMON"));
}

/// Out of the book (views 54, 55 and the link), views 4, 3 (16), 2, 1
/// (12), 7 (3), 12, 13, 14, 16, 36, 35 (14): the big horse to the
/// island's second form (view 171, chapter 20), views 177, 180 (24),
/// 181, 182 (the serpent rises: film 10), 183: the salmon given to the
/// serpent (film 13): the glass tower, back to view 177 (chapter 20).
fn the_serpent(r: &mut Run) {
    r.route(&[54, 55]);
    r.walk_link();
    r.route(&[4, 3, 2, 1, 7, 12, 13, 14, 16, 36, 35]);
    assert_eq!(r.chapter(), 14);
    let horse = r.who("GRAND_CHEVAL");
    let mut evs = r.click_on(true, horse);
    evs.extend(r.settle());
    assert!(films(&evs).contains(&7));
    assert_eq!((r.e.place(), r.chapter()), (("ilepoete2.wam", 171), 20));
    r.walk(177);
    r.walk(180);
    assert_eq!(r.chapter(), 24);
    r.walk(181);
    let evs = r.walk(182);
    assert!(films(&evs).contains(&10));
    r.walk(183);
    r.shot("11-serpent");
    let evs = r.give("SAUMON", "SERPENT2");
    assert!(films(&evs).contains(&13));
    assert!(r.in_bag("TOUR_VERRE"));
    assert_eq!((r.e.place(), r.chapter()), (("ilepoete2.wam", 177), 20));
    r.stow();
}

/// View 171: the big horse back (view 35, chapter 14); the walk to the
/// book; the glass tower given to Dian Cecht (dialog 67), Nuada's sword
/// taken in view 61, the tower broken with it in view 56 (sound 10): the
/// hand; the hand given to Nuada (dialog 69), then his sword (film 17):
/// flag 71, the story's end in Ireland.
fn nuadas_hand(r: &mut Run) {
    r.walk(171);
    let horse = r.who("GRAND_CHEVAL");
    let mut evs = r.click_on(true, horse);
    evs.extend(r.settle());
    assert_eq!((r.e.place(), r.chapter()), (("bigile1.wam", 35), 14));
    r.walk(34);
    r.route(&[27, 28, 33, 32, 13, 12, 7, 1, 2, 3, 4, 5, 38]);
    let crayon = r.obj("CRAYON");
    r.pick(crayon);
    r.walk(50);
    assert_eq!(r.chapter(), 4);
    r.route(&[51, 56]);
    let evs = r.give("TOUR_VERRE", "DIAN_CECHT");
    assert_eq!(dialogs(&evs), [67]);
    assert!(r.shown(false, "TOUR_VERREPOSE"));
    r.stow();
    r.route(&[59, 61]);
    r.take("EPEE_NUADA");
    r.route(&[59, 56]);
    let sword = r.obj("EPEE_NUADA");
    r.pick(sword);
    let tower = r.obj("TOUR_VERREPOSE");
    let mut evs = r.click_on(false, tower);
    evs.extend(r.settle());
    assert!(r.shown(false, "MAIN"));
    r.stow();
    r.take("MAIN");
    r.route(&[59, 61]);
    let evs = r.give("MAIN", "NUADA");
    assert_eq!(dialogs(&evs), [69]);
    assert!(r.shown(true, "NUADA_MAIN"));
    r.stow();
    r.shot("12-nuada-hand");
    let evs = r.give("EPEE_NUADA", "NUADA_MAIN");
    assert!(films(&evs).contains(&17));
    assert_eq!(r.flag(71), 1);
    r.stow();
}

/// The Irish chapters a game goes through (chapter 1 is `demo.wam`'s,
/// which no script reaches; 6 and 17 are named by no `chapter=` line).
const IRISH_CHAPTERS: [i32; 16] = [2, 3, 4, 5, 7, 8, 9, 11, 12, 13, 14, 15, 16, 19, 20, 24];

/// Each Irish chapter was left as its script leaves it: one of its own
/// `/set(chapter=..)` lines ran (the events' chapter and line, the
/// script's line numbers).
fn ireland_chapters_ended(e: &Engine, evs: &[Event]) {
    for c in IRISH_CHAPTERS {
        let text = e
            .files()
            .read(atlantis2_data::Folder::Scenar, &format!("chapi{c:03}.con"))
            .unwrap();
        let ends: Vec<usize> = text
            .split(|&b| b == b'\n')
            .enumerate()
            .filter(|(_, l)| {
                let l = String::from_utf8_lossy(l).trim().to_ascii_lowercase();
                l.starts_with("/set(chapter=")
            })
            .map(|(i, _)| i + 1)
            .collect();
        assert!(!ends.is_empty());
        assert!(
            evs.iter()
                .any(|x| matches!(x, Event::Set { chapter, line } if *chapter == c && ends.contains(line))),
            "chapter {c} never left by its script (lines {ends:?})"
        );
    }
}

/// A tool more than a test (ignored): the stages played one by one, each
/// from the save the previous one left (`A2_DEV_DIR`, `A2_DEV_FROM`,
/// `A2_DEV_TO`), for working on them.
#[test]
#[ignore]
fn ireland_stages_from_saves() {
    if game().is_none() {
        return;
    }
    let dir = PathBuf::from(std::env::var("A2_DEV_DIR").unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    let env = |k: &str, d: usize| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let from = env("A2_DEV_FROM", 0);
    let to = env("A2_DEV_TO", IRELAND.len());
    let mut r = Run::new(&dir);
    r.e.player_number = 4;
    if from == 0 {
        to_ireland(&mut r);
        assert!(r.e.save_slot(0));
    } else {
        r.e.load_slot(from as u8).unwrap();
        r.wait(30);
    }
    for (i, (name, stage)) in IRELAND.iter().enumerate().take(to).skip(from) {
        eprintln!(
            "stage {i} {name}: pass {} place {:?} chapter {}",
            r.pass,
            r.e.place(),
            r.chapter()
        );
        stage(&mut r);
        assert!(r.e.save_slot((i + 1) as u8));
    }
    eprintln!(
        "done: pass {} place {:?} chapter {}",
        r.pass,
        r.e.place(),
        r.chapter()
    );
    let journal = std::mem::take(&mut r.journal);
    ireland_lines_as_the_original(&journal);
}

/// Every `/set` line of an Irish chapter that ran (the events'
/// chapter and line) is one of the lines the original's handler ran
/// (`scripts_worlds_ireland.txt`, chapters 1-24), or a `#/set` line the
/// original's scan runs too (`scripts_con_comment.txt`: chapter 19's line
/// 74): the line numbers the port reports are the scripts' own.
fn ireland_lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_ireland.txt");
    let comment: Vec<usize> = include_str!("golden/scripts_con_comment.txt")
        .lines()
        .filter(|l| l.starts_with("chapi019 "))
        .flat_map(|l| {
            l.split("lines ")
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .split(',')
        })
        .filter_map(|n| n.parse().ok())
        .collect();
    let mut n = 0;
    for e in evs {
        if let Event::Set { chapter, line } = e {
            if (1..=24).contains(chapter) {
                let head = format!("chapi{chapter:03}.con:{line} ");
                assert!(
                    golden.lines().any(|l| l.starts_with(&head))
                        || (*chapter == 19 && comment.contains(line)),
                    "chapter {chapter} line {line}: not a line the original runs"
                );
                n += 1;
            }
        }
    }
    assert!(n > 0 || !evs.iter().any(|e| matches!(e, Event::Set { .. })));
}

/// A game from the start-up intro to China and back, with the mouse: the
/// walk to the shaman (his greeting), China's boat taken in view 9 and
/// laid on its zone in the hut (the hook, chapter 75's section, film 21),
/// the trip (`MONDE` 3 on disc 3, `chine.wam` view 10, chapter 42 and its
/// music), the gardener master of view 12 and the worried monk of view 25
/// asked about everything they offer (their answers spoken by the 3D
/// heads), the gong taken in view 7 and struck on its zone in view 5:
/// chapter 42's way back to Tibet's view 7 and chapter 75. Every Chinese
/// `/set` line reported is one the original runs
/// (`scripts_worlds_china.txt`).
#[test]
fn a_game_from_tibet_to_china_and_back() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow-china");
    let mut r = Run::new(&dir);
    tibet_start(&mut r);
    // The shaman's greeting on the way (view 7), China's boat in view 9.
    for to in 3..=9 {
        r.walk(to);
    }
    let boat = r.obj("MANDALA_BATEAU_CH");
    let evs = r.click_sprite(|e| matches!(e, Event::Take { .. }));
    assert!(evs.contains(&Event::Take { id: boat }));
    r.walk(10);
    r.pick(boat);
    let zi =
        r.e.wam()
            .view(10)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some("ZONE_CHB"))
            .unwrap();
    r.aim(zi);
    r.tap();
    let mut evs = r.events();
    assert!(evs.contains(&Event::Sound("sound400.apc".into())));
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.wait(60);
    r.shot_as("china-1-departure-film.png");
    evs.extend(r.settle());
    // The trip, as chapter 75's section goes on after the film.
    assert!(!evs.iter().any(|e| matches!(e, Event::WorldNotPorted(_))));
    assert!(evs.contains(&Event::Monde(3)));
    assert!(evs.contains(&Event::Wam("chine.wam".into())));
    assert!(evs.iter().any(
        |e| matches!(e, Event::VideoStart { name, frames: Some(_) } if name == "cine021.hnm")
    ));
    assert_eq!(r.e.place(), ("chine.wam", 10));
    assert_eq!(r.e.files().current_disc(), 3);
    assert_eq!(
        (
            r.chapter(),
            r.var("MONDE"),
            r.var("NbMandalaPosed"),
            r.var("OrdreMandala2"),
            r.var("posechb"),
            r.var("VignetteSave"),
            r.var("ChineVisited"),
        ),
        (42, 3, 1, 1, 1, 5, 1)
    );
    assert!(!r.in_bag("MANDALA_BATEAU_CH"));
    r.wait(30);
    assert_eq!(r.e.music_playing(), Some(23));
    r.shot_as("china-2-arrival.png");

    // The gardener master in view 12: everything he offers.
    r.route(&[11, 12]);
    let gardener = r.who("MAITRE_JARDIN");
    r.head_passes = 0;
    let asked = r.ask_all(gardener);
    let want: Vec<(i32, u32)> = [("MAITRE", 220), ("HEROS_CH", 221)]
        .iter()
        .map(|&(n, d)| (r.subject(n), d))
        .collect();
    assert_eq!(asked, want);
    assert!(r.head_passes > 0, "no head drawn for the gardener");
    r.shot_as("china-3-gardener.png");

    // The worried monk of view 25, by views 11, 10, 19 and 26.
    r.route(&[11, 10, 19, 26, 25]);
    let monk = r.who("MOINE1");
    let asked = r.ask_all(monk);
    let want: Vec<(i32, u32)> = [("MOINE1", 225), ("MAITRE", 230)]
        .iter()
        .map(|&(n, d)| (r.subject(n), d))
        .collect();
    assert_eq!(asked, want);
    r.shot_as("china-4-monk.png");
    assert_eq!(r.chapter(), 42);

    // The gong of view 7, struck on its zone in view 5: the way back.
    r.route(&[26, 19, 10, 7]);
    r.take("GONG");
    r.walk(5);
    let zone = r.zone_named("GONG_ZONE");
    let evs2 = r.use_on(Some("GONG"), zone);
    assert!(evs2.contains(&Event::Sound("sound200.apc".into())));
    assert!(evs2.contains(&Event::Monde(5)));
    assert!(evs2.contains(&Event::Wam("tibet.wam".into())));
    r.wait(10);
    r.shot_as("china-5-back-in-tibet.png");
    assert_eq!(r.e.place(), ("tibet.wam", 7));
    assert_eq!((r.chapter(), r.var("MONDE")), (75, 5));
    let journal = std::mem::take(&mut r.journal);
    china_lines_as_the_original(&journal);
    // No Chinese place on this route has a puzzle hook.
    assert!(!journal
        .iter()
        .any(|e| matches!(e, Event::PuzzleNotPorted { .. })));
}

/// Every `/set` line of a Chinese chapter that ran (the events' chapter
/// and line) is one of the lines the original's handler ran
/// (`scripts_worlds_china.txt`, chapters 42-60).
fn china_lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_china.txt");
    let mut n = 0;
    for e in evs {
        if let Event::Set { chapter, line } = e {
            if (42..=60).contains(chapter) {
                let head = format!("chapi{chapter:03}.con:{line} ");
                assert!(
                    golden.lines().any(|l| l.starts_with(&head)),
                    "chapter {chapter} line {line}: not a line the original runs"
                );
                n += 1;
            }
        }
    }
    assert!(n > 10, "{n}");
}
