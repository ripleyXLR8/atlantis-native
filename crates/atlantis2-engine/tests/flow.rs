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
//! to Tibet as chapter 16 makes it. China the same way, from a new game:
//! the trip, then the whole of its story as chapters 42-60 make it
//! ([`CHINA`]: the shadow, the abbot and the gardener master, the turtle
//! room and its walk, the chest and the incense, the figurines, their
//! coins and the five wheels, the arrow, the pearl and the dragon, the
//! door and the immortal, the labyrinth's forms, the lantern), a save and
//! a load through the menus, and the way back to Tibet with the lantern;
//! and a shorter trip back by chapter 42's gong. Yucatán and Xibalba the
//! same way, from a new game ([`whole_yucatan`]): Yucatán's chapters
//! 25-34 with the counting grids, the brazier back and the second trip,
//! the priestess's yes and Xibalba's chapters 35-39 (its three puzzles
//! played with the mouse: the canoe, the web's rounds, the star map), a save
//! and a load in each, the soul brought back to the king and Yucatán's
//! mandala laid in Tibet. Then the end of the game, from Tibet as the three
//! worlds leave it: the space-travel hub by Tibet's ball and back, the
//! crystal laid in the hut, Shamballa (the fireflies' win written by the
//! testing aid, the puzzle not being ported; the vortex), Rhea, the
//! ring's rolling moves and its star, Atlantis (the octopus fought with the
//! mouse: lost without the crystal, then won with it), Tibet again, the
//! hub's last trip, the films, the credits and the quit.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the test is skipped. The saves go to a
//! scratch folder of the build's.

use std::path::{Path, PathBuf};

use atlantis2_data::defnames::{self, DefNames};
use atlantis2_data::save::{entry, read_save, SystemTime};
use atlantis2_data::A2Files;
use atlantis2_engine::bag::{UiBank, UiDraw, FORM_FRAMES};
use atlantis2_engine::engine::{MANDALAS, OCTOPUS_ENTRY, OCTOPUS_MODE, STAR_HOLES};
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
    /// clicked: the object in hand, the bar closed. A special object (a
    /// form, the fan) opens its overlay: the click that closes it puts the
    /// object in hand.
    fn pick(&mut self, id: i32) {
        self.choose(id);
        if self.e.overlays.any() {
            self.tap();
            self.step();
            self.step();
            assert!(!self.e.overlays.any());
        }
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

    /// The octopus fight of Atlantis's view 16 played with the mouse, a
    /// frame a pass, until one of its two `chapter=` commands: the player
    /// faces the octopus (yaw −90°, its sector) and waits; with `shoot`,
    /// when a tentacle attacks, the view is turned onto it (a mouse move a
    /// few frames, aimed at the middle of its frame on the cube) and,
    /// while it is under the centre, clicked each frame. The fight's
    /// events and what its end starts, played out.
    fn octopus_fight(&mut self, shoot: bool) -> Vec<Event> {
        let wrap = |a: f32| {
            let t = std::f32::consts::TAU;
            (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
        };
        let face_octopus = -std::f32::consts::FRAC_PI_2;
        let mut all = Vec::new();
        let mut shots = 0;
        let mut shook = false;
        let mut idle = 0;
        for n in 0..20_000 {
            let (sx, sy) = self.mouse_signs;
            let attacker = self.e.octopus.records[..self.e.octopus.live()]
                .iter()
                .find(|r| r.state == 2 || r.state == 4)
                .and_then(|r| r.tentacles[r.attacking as usize]);
            let target = match attacker {
                Some(a) if shoot => self.slot_aims(a).first().copied(),
                _ => Some((face_octopus, 0.0)),
            };
            let on = attacker.is_some_and(|a| {
                self.e.sprite_under_centre().1 != 0 && centre_slot(&self.e) == Some(a)
            });
            if shoot && on {
                self.tap();
                shots += 1;
            } else if let Some((yaw, pitch)) = target {
                // A new mouse move once the last one's turn is spent.
                idle += 1;
                let (y0, p0) = self.e.camera.angles();
                let (dy, dp) = (wrap(yaw - y0), pitch - p0);
                let far = dy.abs() > 0.02 || dp.abs() > 0.02;
                if far && idle > 6 && self.e.look.yaw_speed.abs() < 1e-3 {
                    idle = 0;
                    let mx = (sx * dy * 500.0).round() as i32;
                    let my = (sy * dp * 500.0).round() as i32;
                    self.e.mouse_motion(mx, my);
                }
            }
            let evs = self.events();
            shook |= self.e.shake.counter != 0;
            if n % 400 == 200 || (shook && self.e.shake.counter == 3) {
                let k = if shoot { "won" } else { "lost" };
                if self.e.shake.counter == 3 {
                    self.shot_as(&format!("pz-octopus-{k}-blow.png"));
                } else if shots > 0 {
                    self.shot_as(&format!("pz-octopus-{k}-shooting.png"));
                } else {
                    self.shot_as(&format!("pz-octopus-{k}-waiting.png"));
                }
            }
            let end = evs
                .iter()
                .any(|e| matches!(e, Event::OctopusCommand(c) if c.starts_with("(chapter=")));
            all.extend(evs);
            if end {
                self.shot_as(&format!(
                    "pz-octopus-{}-end.png",
                    if shoot { "won" } else { "lost" }
                ));
                all.extend(self.settle());
                return all;
            }
        }
        panic!("the octopus fight does not end ({shots} shots)");
    }

    /// The canoe (loop type 9) played with the mouse, a frame a pass:
    /// each tile taken to its own square (a click on it, one on its
    /// square: the two swap) and turned upright (right clicks), the stage
    /// clicked, then the canoe left to paddle to the river's end. Its
    /// events.
    fn play_canoe(&mut self) -> Vec<Event> {
        let mut evs = Vec::new();
        let centre = |r: &Run, k: usize| {
            let s = r.e.canoe().unwrap().board[k];
            (s.x + 0x10, s.y + 0x36)
        };
        let mut frame = |r: &mut Run, k: usize, left: bool, right: bool| {
            let (x, y) = centre(r, k);
            r.point(x, y);
            if left {
                r.tap();
            }
            if right {
                r.e.right_click();
            }
            evs.extend(r.events());
        };
        for k in 0..8 {
            let board = self.e.canoe().unwrap().board;
            let s = (0..8).find(|&s| board[s].tile / 4 == k as i32).unwrap();
            if s != k {
                frame(self, s, true, false);
                frame(self, k, true, false);
            }
            while self.e.canoe().unwrap().board[k].tile % 4 != 0 {
                frame(self, k, false, true);
            }
        }
        frame(self, 8, true, false);
        for _ in 0..5000 {
            if self.e.loop_type != LoopType::Canoe {
                return evs;
            }
            frame(self, 8, false, false);
        }
        panic!("the canoe never arrives");
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

/// The game from the start-up intro to China, with the mouse: the walk to
/// the shaman (his greeting), China's boat taken in view 9 and laid on its
/// zone in the hut (the hook, chapter 75's section, film 21), the trip
/// (`MONDE` 3 on disc 3, `chine.wam` view 10, chapter 42 and its music).
fn to_china(r: &mut Run) {
    tibet_start(r);
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
}

/// A game from the start-up intro to China and back, with the mouse:
/// [`to_china`], the gardener master of view 12 and the worried monk of
/// view 25 asked about everything they offer (their answers spoken by the
/// 3D heads), the gong taken in view 7 and struck on its zone in view 5:
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
    to_china(&mut r);

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

// ---------------------------------------------------------------------------
// The whole of China, with the mouse.

/// The play of China: what a player does there, besides Ireland's.
impl Run {
    /// The fewest moves (type-1 zones of the current place) from the
    /// current view to view `to`, walked; views in `avoid` are not passed
    /// through.
    fn go(&mut self, to: u32, avoid: &[u32]) -> Vec<Event> {
        let from = self.e.place().1;
        let wam = self.e.wam();
        let mut prev = std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some(v) = queue.pop_front() {
            if v == to {
                break;
            }
            for z in wam.view(v).map(|w| w.zones.as_slice()).unwrap_or_default() {
                let n = z.value as u32;
                if z.kind != 1 || n == from || prev.contains_key(&n) || wam.view(n).is_none() {
                    continue;
                }
                if avoid.contains(&n) && n != to {
                    continue;
                }
                prev.insert(n, v);
                queue.push_back(n);
            }
        }
        let mut path = vec![to];
        while *path.last().unwrap() != from {
            let at = *path.last().unwrap();
            path.push(
                *prev
                    .get(&at)
                    .unwrap_or_else(|| panic!("no way from view {from} to {to}")),
            );
        }
        path.pop();
        path.reverse();
        let mut all = Vec::new();
        for v in path {
            all.extend(self.walk(v));
        }
        all
    }

    /// A click on the move zone to view `to` that the scripts send
    /// elsewhere (a `NEXTVUE`, a `vue=` at the arrival): played out until
    /// the panorama is still.
    fn walk_redirected(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        let zone = self.zone_to(to);
        assert!(self.try_aim(zone, true), "no move zone to view {to}");
        self.tap();
        let mut all = self.events();
        all.extend(self.settle());
        all
    }

    /// A click on zone `zone` with whatever is in hand (none stowed):
    /// what follows, played out.
    fn press_zone(&mut self, zone: usize) -> Vec<Event> {
        self.aim(zone);
        self.tap();
        let mut evs = self.events();
        evs.extend(self.settle());
        evs
    }

    /// A click on character `who`, whose animation moves it: its slots
    /// looked at again after each try (the labyrinth's demons).
    fn click_moving(&mut self, who: &str) -> Vec<Event> {
        let id = self.who(who);
        for _ in 0..8 {
            let shown: Vec<usize> = (0..self.e.anim.slots.len())
                .filter(|&k| {
                    let s = &self.e.anim.slots[k];
                    s.id == id && s.active && s.click == 2
                })
                .collect();
            for n in shown {
                for (yaw, pitch) in self.slot_aims(n) {
                    let on = |e: &Engine| {
                        e.sprite_under_centre().1 == 2
                            && centre_slot(e).is_some_and(|k| e.anim.slots[k].id == id)
                    };
                    if self.steer(yaw, pitch, &on) {
                        self.tap();
                        let mut evs = self.events();
                        evs.extend(self.events());
                        return evs;
                    }
                }
            }
            self.wait(15);
        }
        panic!("{who} never under the centre in view {}", self.e.place().1);
    }

    /// The `form.spr` frames the bag's overlay draws now, in order.
    fn form_frames(&mut self) -> Vec<i32> {
        self.step();
        self.e
            .ui_draws
            .iter()
            .filter_map(|d| match d {
                UiDraw::Sprite {
                    bank: UiBank::Form,
                    frame,
                    ..
                } => Some(*frame),
                _ => None,
            })
            .collect()
    }

    /// The bag's overlay closed with a click (the object back in hand).
    fn close_overlay(&mut self) {
        self.tap();
        self.step();
        self.step();
        assert!(!self.e.overlays.any());
    }

    /// The labyrinth's view `to` of `place` (`labysol.wam` or
    /// `labyplaf.wam`) by the fewest moves, through the link between the
    /// two places (views 28 and 33) when needed.
    fn go_laby(&mut self, place: &str, to: u32) -> Vec<Event> {
        let mut all = Vec::new();
        if self.e.place().0 != place {
            let here = if self.e.place().0 == "labysol.wam" {
                28
            } else {
                33
            };
            all.extend(self.go(here, &[]));
            all.extend(self.walk_link());
            assert_eq!(self.e.place().0, place);
        }
        all.extend(self.go(to, &[]));
        all
    }

    /// Passes until the dragon of view 31 is still and no cloud is
    /// transparent (what a player waits for before answering).
    fn until_still(&mut self) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..3000 {
            if self.e.dragon.state == 0 && self.var("Time_nuage_transparent") == 0 {
                return all;
            }
            all.extend(self.events());
        }
        panic!("the dragon never stops");
    }
}

/// The views of the current place a move leads to from `view`.
fn neighbours(e: &Engine, view: u32) -> Vec<u32> {
    e.wam()
        .view(view)
        .map(|v| {
            v.zones
                .iter()
                .filter(|z| z.kind == 1)
                .map(|z| z.value as u32)
                .collect()
        })
        .unwrap_or_default()
}

/// A walk of exactly `n` moves from `from` ending in `to` that does not
/// enter `to` before (breadth first over the place's moves).
fn walk_of(e: &Engine, from: u32, to: u32, n: usize) -> Option<Vec<u32>> {
    let mut queue = std::collections::VecDeque::from([vec![from]]);
    while let Some(path) = queue.pop_front() {
        let last = *path.last().unwrap();
        if path.len() == n + 1 {
            if last == to {
                return Some(path[1..].to_vec());
            }
            continue;
        }
        if last == to && path.len() > 1 {
            continue;
        }
        for v in neighbours(e, last) {
            let mut p = path.clone();
            p.push(v);
            queue.push_back(p);
        }
    }
    None
}

/// The sounds the events started, by name.
fn sounds(evs: &[Event]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            Event::Sound(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

/// China's story, stage by stage, as chapters 42-60 make it: each stage
/// starts and ends in the panorama, the game where the last one left it.
const CHINA: &[Stage] = &[
    ("the shadow", the_shadow),
    ("the abbot", the_abbot),
    ("the gardener's ritual", the_gardeners_ritual),
    ("the turtle room", the_turtle_room),
    ("the turtle's walk", the_turtles_walk),
    ("the chest", the_chest),
    ("the incense", the_incense),
    ("the figurines taken", the_figurines_taken),
    ("the figurines laid", the_figurines_laid),
    ("the coins", the_coins),
    ("the wheels", the_wheels),
    ("the arrow", the_arrow),
    ("the pearl", the_pearl),
    ("the door", the_door),
    ("the forms", the_forms),
    ("the lantern", the_lantern),
];

/// `chine.wam` view 10, chapter 42: views 7, 3, 1, the shadow at the gate,
/// whose arrival lets everyone be asked about it (chapter 42 lines
/// 32-35); views 3, 5, 11, 12: the gardener master asked about it
/// (dialog 222).
fn the_shadow(r: &mut Run) {
    assert_eq!((r.e.place(), r.chapter()), (("chine.wam", 10), 42));
    r.route(&[7, 3, 1]);
    assert!(r.shown(true, "OMBRE_CHINE"));
    r.shot_as("china-6-shadow.png");
    r.route(&[3, 5, 11, 12]);
    assert_eq!(r.ask("MAITRE_JARDIN", "OMBRE"), 222);
}

/// Views 11, 10, 19, 30 (chapter 43), 37 (44), 38: the abbot asked about
/// the shadow (dialog 232, flag 132: the immortal Tan Yun named, the
/// gardener master to be asked about him) and about the master (233).
fn the_abbot(r: &mut Run) {
    r.route(&[11, 10, 19, 30]);
    assert_eq!(r.chapter(), 43);
    r.walk(37);
    assert_eq!(r.chapter(), 44);
    r.walk(38);
    assert_eq!(r.ask("ABBE", "OMBRE"), 232);
    assert_eq!(r.flag(132), 1);
    assert_eq!(r.ask("ABBE", "MAITRE"), 233);
    r.shot_as("china-7-abbot.png");
}

/// Views 37, 30 (43), 19 (42), 10, 11, 12: the gardener master asked about
/// the immortal (dialog 223: flag 133, the turtle master shown in his
/// room) and about the turtle room (224: water).
fn the_gardeners_ritual(r: &mut Run) {
    r.route(&[37, 30]);
    assert_eq!(r.chapter(), 43);
    r.walk(19);
    assert_eq!(r.chapter(), 42);
    r.route(&[10, 11, 12]);
    assert_eq!(r.ask("MAITRE_JARDIN", "IMMORTEL"), 223);
    assert_eq!(r.flag(133), 1);
    assert_eq!(r.ask("MAITRE_JARDIN", "SALLE_TORTUE"), 224);
}

/// Views 11, 10, 19, 30 (43), 31, 32 and its link to the turtle room
/// (chapter 46):
/// the first arrival in view 40 is the meeting (dialog 260), the turtle
/// and the engraved ring in the bag; the ring's entry sets the puzzle mode
/// 2.
fn the_turtle_room(r: &mut Run) {
    r.route(&[11, 10, 19, 30, 31, 32]);
    assert_eq!(r.chapter(), 43);
    let evs = r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("salletortue.wam", 40), 46));
    assert!(dialogs(&evs).contains(&260));
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_c340
    }));
    assert_eq!(r.e.puzzle_mode(), 2);
    assert!(r.in_bag("TORTUE_NOIRE") && r.in_bag("ANNEAU_GRAVE"));
    assert_eq!(r.flag(134), 1);
    r.shot_as("china-8-turtle-room.png");
}

/// The ring turned with its zones in view 56 (the four directions; sound
/// 218, one frame a tick), the turtle laid on the water in view 57, the
/// turtle master asked about the tiger's home until he starts his
/// indications (dialog 266), then a walk of eight moves to view 47 (the
/// dragon gate): his indications one an arrival (dialogs 268-274), the
/// seventh arrival silent, the eighth dialog 276, film 22, `miniature.wam`
/// view 1, chapter 47. (Chapter 46 counts the arrivals only: any walk of
/// eight moves that ends in view 47 is the one his words describe.)
fn the_turtles_walk(r: &mut Run) {
    r.route(&[55, 56]);
    // A quarter left from frame 0 (9 frames), another (10), one back to
    // the right: the frames the ring is left on.
    for (zone, frame) in [
        ("TORTUE_GAUCHE", 9),
        ("TORTUE_GAUCHE", 19),
        ("TORTUE_DROITE", 9),
    ] {
        let z = r.zone_named(zone);
        r.stow();
        r.aim(z);
        r.tap();
        let evs = r.events();
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_c290
        }));
        assert_eq!(sounds(&evs), ["sound218.apc"]);
        assert_ne!(r.e.ring_count(), 0);
        r.wait(60);
        assert_eq!((r.e.ring_count(), r.e.ring_store()), (0, frame));
    }
    r.shot_as("pz-turtle-flow-ring.png");
    r.walk(57);
    let water = r.zone_named("EAU_TORTUE");
    r.use_on(Some("TORTUE_NOIRE"), water);
    assert!(!r.in_bag("TORTUE_NOIRE"));
    let mut heard = Vec::new();
    for _ in 0..6 {
        if r.var("indication_tortue") == 1 {
            break;
        }
        heard.push(r.ask("MAITRE_TORTUE", "DEMEURE_TIGRE"));
    }
    assert_eq!(r.var("indication_tortue"), 1, "{heard:?}");
    assert_eq!(heard.last(), Some(&266));
    let path = walk_of(&r.e, 57, 47, 8).expect("a walk of eight moves");
    let mut heard = Vec::new();
    for to in path {
        if to != 47 {
            heard.extend(dialogs(&r.walk(to)));
            continue;
        }
        // The arrival in view 47: dialog 276, then film 22 to the next
        // place.
        let evs = r.walk_redirected(to);
        heard.extend(dialogs(&evs));
        assert!(films(&evs).contains(&22));
    }
    assert_eq!(heard, [268, 269, 270, 271, 272, 274, 276]);
    assert_eq!((r.e.place(), r.chapter()), (("miniature.wam", 1), 47));
    assert_eq!((r.flag(135), r.flag(136), r.flag(141)), (1, 1, 1));
    r.shot_as("china-9-miniature.png");
}

/// Views 4, 5: the chest opened by four clicks on its zone (sounds 211 to
/// 214), the fifth takes the chopstick.
fn the_chest(r: &mut Run) {
    r.route(&[4, 5]);
    assert_eq!(r.e.puzzle_mode(), 5);
    let take = r.zone_named("PRENDRE_BAGUETTE");
    for k in 1..=4 {
        let evs = r.use_on(None, take);
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0049_3960
        }));
        assert_eq!(sounds(&evs), [format!("sound{}.apc", 210 + k)]);
    }
    r.shot_as("pz-chest-flow-open.png");
    r.use_on(None, take);
    assert!(r.in_bag("BAGUETTE"));
    assert_eq!(r.var("BAGUETTE_PRISE"), 1);
    r.stow();
}

/// Views 4, 1, 3, 6: the four incense sticks lit with the chopstick in
/// their order (chapter 47: `EtatPzEncens` 1 to 4), film 23, view 7,
/// chapter 48.
fn the_incense(r: &mut Run) {
    r.route(&[4, 1, 3, 6]);
    for (k, stick) in ["ENCENS1", "ENCENS2", "ENCENS3"].iter().enumerate() {
        let z = r.zone_named(stick);
        r.use_on(Some("BAGUETTE"), z);
        assert_eq!(r.var("EtatPzEncens"), k as i32 + 1);
    }
    r.shot_as("pz-chest-flow-incense.png");
    let z = r.zone_named("ENCENS4");
    let evs = r.use_on(Some("BAGUETTE"), z);
    assert!(films(&evs).contains(&23));
    assert_eq!((r.e.place(), r.chapter()), (("miniature.wam", 7), 48));
    assert_eq!(r.flag(142), 1);
    assert!(!r.in_bag("BAGUETTE"));
}

/// Views 10 (chapter 55), 11, 12, 13: the six figurines lying in the grass
/// taken with clicks.
fn the_figurines_taken(r: &mut Run) {
    r.walk(10);
    assert_eq!(r.chapter(), 55);
    r.route(&[11, 12, 13]);
    let ids: Vec<i32> = (1..=6).map(|k| r.obj(&format!("OB_FIG{k}HERBE"))).collect();
    // In the slots' order (an earlier sprite over a later one takes the
    // click).
    let grass: Vec<i32> =
        r.e.anim
            .slots
            .iter()
            .filter(|s| s.active && ids.contains(&s.id))
            .map(|s| s.id)
            .collect();
    assert_eq!(grass.len(), 6);
    for id in grass {
        let evs = r.click_on(false, id);
        assert!(evs.contains(&Event::Take { id }));
        r.settle();
        r.stow();
    }
    assert!(ids
        .iter()
        .all(|&id| r.e.bag_entries().iter().any(|b| b.id == id)));
    r.shot_as("pz-figurines-flow-taken.png");
}

/// Views 12, 11, 10, 7 (chapter 48), 6 (film 34, chapter 47): each
/// figurine laid on a place of the floor and clicked again with an empty
/// hand: back in hand when the place is not its own, held there for good
/// when it is (the click ends); the next place tried until it holds. All
/// six on their own places: `etat5statues`.
fn the_figurines_laid(r: &mut Run) {
    r.route(&[12, 11, 10, 7]);
    assert_eq!(r.chapter(), 48);
    let evs = r.walk(6);
    assert!(films(&evs).contains(&34));
    assert_eq!(r.chapter(), 47);
    let mut free: Vec<i32> = (1..=6).collect();
    let mut tries = 0;
    for k in 1..=6 {
        let name = format!("OB_FIG{k}HERBE");
        let id = r.obj(&name);
        let mut held = None;
        for &p in &free {
            let z = r.zone_named(&format!("SOL_FIGUR{p}"));
            let evs = r.use_on(Some(&name), z);
            assert!(sounds(&evs).contains(&"sound202.apc".to_string()));
            tries += 1;
            if r.var("etat5statues") == 1 {
                held = Some(p);
                break;
            }
            r.stow();
            r.use_on(None, z);
            if r.var("OBJETENMAIN") == id {
                // Not its place: taken back.
                r.stow();
                continue;
            }
            held = Some(p);
            break;
        }
        let p = held.unwrap_or_else(|| panic!("figurine {k}: no place holds it"));
        free.retain(|&x| x != p);
    }
    let pos: Vec<i32> = (1..=6).map(|k| r.var(&format!("pos_statue{k}"))).collect();
    assert_eq!(pos, atlantis2_engine::engine::FIGURINE_TARGETS);
    assert_eq!(r.var("etat5statues"), 1);
    assert!(tries > 6);
    r.wait(2);
    r.shot_as("pz-figurines-flow-laid.png");
}

/// View 7 (chapter 47 lines 27-33: film 23, chapter 48): the figurines
/// alive; the sixth's coin taken with a click, then each of the five given
/// a coin with a click: its disc (dialogs 281-298), one coin less.
fn the_coins(r: &mut Run) {
    let evs = r.walk(7);
    assert!(films(&evs).contains(&23));
    assert_eq!(r.chapter(), 48);
    let sixth = r.obj("OB_FIG6HERBE");
    let mut evs = r.click_on(false, sixth);
    evs.extend(r.settle());
    assert!(r.in_bag("PIECE5"));
    r.stow();
    for k in 1..=5 {
        let coin = format!("PIECE{}", 6 - k);
        let evs = r.give(&coin, &format!("FIGURINE{k}"));
        let first = 281 + 4 * (k as u32 - 1);
        assert_eq!(dialogs(&evs), [first, first + 1], "figurine {k}");
        assert!(r.in_bag(&format!("DISQUE{k}")));
        assert!(!r.in_bag(&coin));
        assert_eq!(r.flag(149 + k as usize), 1);
        r.stow();
    }
    r.shot_as("pz-figurines-flow-discs.png");
}

/// Views 10 (55), 11, 12, 13: the five discs laid on the axle's places in
/// the order of the elements that make each other (disc k on place k:
/// earth, metal, water, wood, fire): film 24, the bridge half lowered;
/// then in the order of those that overcome each other (water, fire,
/// metal, wood, earth: places 1-5 hold discs 3, 5, 2, 4, 1), by swaps:
/// film 25, the bridge lowered.
fn the_wheels(r: &mut Run) {
    r.walk(10);
    assert_eq!(r.chapter(), 55);
    r.route(&[11, 12, 13]);
    for k in 1..=5 {
        let z = r.zone_named(&format!("AXE_5ROUES{k}"));
        let evs = r.use_on(Some(&format!("DISQUE{k}")), z);
        assert!(evs.contains(&Event::PuzzleRan {
            function: 0x0048_d170
        }));
        if k == 5 {
            assert!(films(&evs).contains(&24));
        }
    }
    assert_eq!((r.var("findisque1"), r.var("findisque2")), (1, 0));
    r.shot_as("pz-wheels-flow-first.png");
    // Disc 1 taken from place 1, laid on 5 (disc 5 in hand), on 2 (disc
    // 2), on 3 (disc 3), on 1.
    let mut evs = Vec::new();
    for p in [1, 5, 2, 3, 1] {
        let z = r.zone_named(&format!("AXE_5ROUES{p}"));
        evs = r.press_zone(z);
    }
    assert!(films(&evs).contains(&25));
    let pos: Vec<i32> = (1..=5).map(|k| r.var(&format!("pos_roue{k}"))).collect();
    assert_eq!(pos, [5, 3, 1, 4, 2]);
    assert_eq!(r.var("findisque2"), 1);
    r.shot_as("pz-wheels-flow-second.png");
}

/// Views 12, 14 (chapter 55 line 18: chapter 56), 15, 16: the arrow turned
/// to the hunter with its zone (sound 201), view 17 (the fox woman saved:
/// flags 162 and 164), 18, 30 (chapter 58).
fn the_arrow(r: &mut Run) {
    r.walk(12);
    let evs = r.walk(14);
    assert_eq!(r.chapter(), 56);
    assert!(!evs.is_empty());
    r.route(&[15, 16]);
    let z = r.zone_named("CLIC_FL_CHASSEUR");
    let evs = r.use_on(None, z);
    assert_eq!(sounds(&evs), ["sound201.apc"]);
    assert_eq!(r.var("TueChasseur"), 1);
    r.shot_as("china-10-arrow.png");
    r.route(&[17, 18, 30]);
    assert_eq!((r.flag(162), r.flag(163), r.flag(164)), (1, 0, 1));
    assert_eq!(r.chapter(), 58);
}

/// View 31: the dragon woken with a click, then the rounds answered as the
/// pearl hides in the fixed order ABBAB ('A' behind the left cloud),
/// waiting each time for the dragon to stop: the five low clouds raised
/// (sounds 206-210), `FINPERLE`; the pearl with a hole taken, laid in the
/// mechanism of view 32: film 26, view 19, chapter 59.
fn the_pearl(r: &mut Run) {
    let evs = r.walk(31);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_44a0
    }));
    assert_eq!(r.e.puzzle_mode(), 4);
    let z = r.zone_named("CLIC_DRAGON");
    r.use_on(None, z);
    let mut heard = Vec::new();
    for k in 0..5 {
        r.until_still();
        let left = b"ABBAB"[k % 5] == b'A';
        assert_eq!(r.var("Perle_cachee"), if left { 0x41 } else { 0x42 });
        let z = r.zone_named(if left {
            "CLIC_NUAGE_GAUCHE"
        } else {
            "CLIC_NUAGE_DROITE"
        });
        r.aim(z);
        r.tap();
        let mut evs = r.events();
        assert_eq!(r.var("Action_nuage_bas"), 1);
        if k == 0 {
            r.wait(6);
            r.shot_as("pz-dragon-flow-cloud.png");
        }
        evs.extend(r.until_still());
        heard.extend(sounds(&evs));
    }
    assert_eq!(
        heard,
        [
            "sound206.apc",
            "sound207.apc",
            "sound208.apc",
            "sound209.apc",
            "sound210.apc"
        ]
    );
    assert_eq!(r.var("FINPERLE"), 1);
    r.shot_as("pz-dragon-flow-won.png");
    r.take("PERLE_TROU");
    let evs = r.walk(32);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_45f0
    }));
    assert_eq!(r.e.puzzle_mode(), 0);
    let z = r.zone_named("MECANISME_DRAGON");
    let evs = r.use_on(Some("PERLE_TROU"), z);
    assert!(films(&evs).contains(&26));
    assert_eq!((r.e.place(), r.chapter()), (("miniature.wam", 19), 59));
}

/// Views 20, 21, 22: the door of view 22 walked through three times (the
/// first two bring the game back to view 22 with sound 217; the third
/// plays film 28: the immortal, dialog 300); he is asked about the shadow
/// (dialog 301, film 29, flag 169: the labyrinth's door open).
fn the_door(r: &mut Run) {
    r.route(&[20, 21, 22]);
    for n in 1..=3 {
        let evs = r.walk_redirected(122);
        assert_eq!(r.e.place(), ("miniature.wam", 22));
        assert_eq!(r.var("nbpassageporte"), n);
        if n < 3 {
            assert!(sounds(&evs).contains(&"sound217.apc".to_string()));
        } else {
            assert!(films(&evs).contains(&28));
            assert!(dialogs(&evs).contains(&300));
        }
    }
    assert!(r.shown(true, "IMMORTEL"));
    r.shot_as("china-11-immortal.png");
    assert_eq!(r.ask("IMMORTEL", "OMBRE"), 301);
    assert_eq!(r.flag(169), 1);
}

/// The labyrinth: view 23 (chapter 59's `NEXTVUE`: view 123), its link
/// (`labysol.wam` view 3, chapter 60, the souls); the buffalo asked about
/// the mushroom (dialog 316: the empty form); the fox woman of view 21 met
/// with an empty hand (dialog 345: her fan); the fan read in the bag (its
/// three symbols, the demons in their order); each of those demons given
/// the form (their stamp, a special event, the stamped form back);
/// the buffalo given the third form: dialog 322, the mushroom (flag 173).
fn the_forms(r: &mut Run) {
    r.walk_redirected(23);
    assert_eq!(r.e.place(), ("miniature.wam", 123));
    r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("labysol.wam", 3), 60));
    assert_eq!(r.e.puzzle_mode(), 3);
    assert_eq!(r.ask("DEMON_BUFFLE", "CHAMPIGNON"), 316);
    assert!(r.in_bag("FORMULAIRE_VIDE"));
    r.shot_as("pz-labyrinth-flow-buffalo.png");
    r.go_laby("labysol.wam", 21);
    r.stow();
    let mut evs = r.click_moving("FEMME_RENARDE");
    evs.extend(r.settle());
    assert!(dialogs(&evs).contains(&345));
    assert!(r.in_bag("EVENTAIL"));
    // The fan read: its three symbols, the demons' stamps in order.
    let fan = r.obj("EVENTAIL");
    r.choose(fan);
    let frames = r.form_frames();
    let order: Vec<usize> = frames[1..]
        .iter()
        .map(|&f| (1..=6).find(|&v| FORM_FRAMES[v] == f).unwrap())
        .collect();
    assert_eq!(order.len(), 3);
    r.shot_as("pz-labyrinth-flow-fan.png");
    r.close_overlay();
    r.stow();
    // The demons by their stamp's number, where they take the form.
    const DEMONS: [(&str, &str, u32); 6] = [
        ("DEMON_DOGUE", "labysol.wam", 12),
        ("DEMON_COCHON", "labysol.wam", 19),
        ("DEMON_RAT", "labysol.wam", 25),
        ("DEMON_LAPIN", "labyplaf.wam", 57),
        ("DEMON_CHEVRE", "labyplaf.wam", 51),
        ("DEMON_CHEVAL", "labyplaf.wam", 33),
    ];
    let forms = [
        "FORMULAIRE_VIDE",
        "FORMULAIRE_1",
        "FORMULAIRE_2",
        "FORMULAIRE_3",
    ];
    for (k, &v) in order.iter().enumerate() {
        let (name, place, view) = DEMONS[v - 1];
        r.go_laby(place, view);
        let form = r.obj(forms[k]);
        if r.var("OBJETENMAIN") != form {
            // A form picked from the bar opens its overlay; the click that
            // closes it gives it back to the hand.
            r.choose(form);
            r.close_overlay();
        }
        assert_eq!(r.var("OBJETENMAIN"), form);
        r.click_moving(name);
        let next = r.obj(forms[k + 1]);
        for _ in 0..6000 {
            if r.e.loop_type == LoopType::Dialog {
                r.listen();
            }
            if r.var("OBJETENMAIN") == next && r.e.loop_type == LoopType::Warp {
                break;
            }
            r.events();
        }
        assert_eq!(r.var("OBJETENMAIN"), next, "{name}");
        assert_eq!(r.var(&format!("essai_laby{}", k + 1)), v as i32);
        r.settle();
        if k == 0 {
            r.shot_as("pz-labyrinth-flow-stamp.png");
        }
    }
    r.stow();
    r.go_laby("labysol.wam", 3);
    let evs = r.give("FORMULAIRE_3", "DEMON_BUFFLE");
    assert_eq!(dialogs(&evs), [322]);
    assert_eq!(r.flag(173), 1);
    assert!(r.in_bag("CHAMPIGNON"));
    r.stow();
    r.shot_as("pz-labyrinth-flow-mushroom.png");
}

/// Out of the labyrinth by view 3's link (chapter 60: chapter 59, view
/// 123), view 22: the immortal given the mushroom (dialog 315, film 30):
/// the lantern (flag 177).
fn the_lantern(r: &mut Run) {
    r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("miniature.wam", 123), 59));
    r.walk(22);
    let evs = r.give("CHAMPIGNON", "IMMORTEL");
    assert!(dialogs(&evs).contains(&315));
    assert!(films(&evs).contains(&30));
    assert!(r.in_bag("LANTERNE"));
    assert_eq!(r.flag(177), 1);
    r.stow();
    r.shot_as("china-12-lantern.png");
}

/// The Chinese chapters a game goes through (45 only with the second monk,
/// 57 is named by no `chapter=` line).
const CHINESE_CHAPTERS: [i32; 11] = [42, 43, 44, 46, 47, 48, 55, 56, 58, 59, 60];

/// Each chapter of `chapters` was left as its script leaves it: one of its
/// own `/set(chapter=..)` lines ran (the events' chapter and line).
fn chapters_ended(e: &Engine, evs: &[Event], chapters: &[i32]) {
    for &c in chapters {
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

/// The way back with the lantern: views 21, 20, 19, 130, 31 (chapter 58),
/// 32: the pearl taken back from the mechanism and laid again (film 27:
/// view 18, chapter 56), views 17, 16, 15, 14, 12 (55), 11, 10, 7 (48), 6
/// (film 34, 47), 3, 1 and its link to the turtle room (chapter 46), the
/// walk to view 40 and its link to `chine.wam` view 32: chapter 46's end
/// of China (films 32 and 33, dialogs 346 and 347), Tibet's view 7,
/// chapter 75, China's mandala in the bag.
fn china_way_back(r: &mut Run) -> Vec<Event> {
    r.route(&[21, 20, 19, 130, 31]);
    assert_eq!(r.chapter(), 58);
    r.walk(32);
    let z = r.zone_named("MECANISME_DRAGON");
    r.use_on(None, z);
    assert!(r.in_bag("PERLE_TROU"));
    let evs = r.press_zone(z);
    assert!(films(&evs).contains(&27));
    assert_eq!((r.e.place(), r.chapter()), (("miniature.wam", 18), 56));
    r.route(&[17, 16, 15, 14, 12]);
    assert_eq!(r.chapter(), 55);
    r.route(&[11, 10, 7]);
    assert_eq!(r.chapter(), 48);
    r.route(&[6, 3, 1]);
    assert_eq!(r.chapter(), 47);
    r.walk_link();
    assert_eq!((r.e.place(), r.chapter()), (("salletortue.wam", 47), 46));
    r.go(40, &[]);
    let evs = r.walk_link();
    assert!(films(&evs).contains(&32) && films(&evs).contains(&33));
    let d = dialogs(&evs);
    assert!(d.contains(&346) && d.contains(&347));
    evs
}

/// A game from the start-up intro through the whole of China and back,
/// with the mouse only (the view turned by mouse moves, every action a
/// click, every object picked from the bag bar, every subject through the
/// menus): [`to_china`], then China's story as its chapters 42-60 make it
/// ([`CHINA`], stage by stage: the shadow and the abbot, the turtle room
/// and its walk, the chest and the incense, the figurines, their coins and
/// the five wheels, the arrow, the pearl and the dragon, the door and the
/// immortal, the labyrinth's forms, the lantern), every puzzle solved as a
/// player does, and the way back with the lantern to Tibet. Every chapter
/// it goes through is left by one of its own `chapter=` lines, every
/// Chinese puzzle hook runs and none is noted, and every Chinese `/set`
/// line reported is one the original runs.
#[test]
fn the_whole_of_china_with_the_mouse() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow-china-whole");
    let mut r = Run::new(&dir);
    to_china(&mut r);
    for (name, stage) in CHINA {
        stage(&mut r);
        assert_eq!(r.e.loop_type, LoopType::Warp, "after {name}");
        if *name == "the wheels" {
            r = saved_and_continued(r, &dir);
        }
    }
    china_way_back(&mut r);
    r.wait(10);
    r.shot_as("china-13-back-in-tibet.png");
    assert_eq!(r.e.place(), ("tibet.wam", 7));
    assert_eq!((r.chapter(), r.var("MONDE")), (75, 5));
    assert_eq!(r.var("isChineFinished"), 1);
    assert!(r.in_bag("MANDALA_CHINE"));
    let journal = std::mem::take(&mut r.journal);
    china_lines_as_the_original(&journal);
    chapters_ended(&r.e, &journal, &CHINESE_CHAPTERS);
    assert!(!journal
        .iter()
        .any(|e| matches!(e, Event::PuzzleNotPorted { .. })));
    for function in [
        0x0048_d680u32,
        0x0048_da20,
        0x0049_38f0,
        0x0049_3960,
        0x0048_d080,
        0x0048_d170,
        0x0049_44a0,
        0x0049_4600,
        0x0049_45f0,
        0x0049_c340,
        0x0049_c290,
        0x0049_c400,
        0x0048_df10,
    ] {
        assert!(
            journal.contains(&Event::PuzzleRan { function }),
            "FUN_{function:08x} never ran"
        );
    }
}

/// The game saved through the menus (Escape, the disc, save, yes) and
/// continued from the player menu of a new run: the place, the disc, the
/// chapter, the bag, the puzzles' state and the story as they were.
fn saved_and_continued(mut r: Run, dir: &Path) -> Run {
    let kept = |r: &Run| {
        let s = r.e.script_state().unwrap();
        let pos: Vec<i32> = (1..=5).map(|k| r.var(&format!("pos_roue{k}"))).collect();
        format!(
            "chapter {} place {:?} disc {} monde {} mode {} wheels {pos:?} flags {:?} bag {:?}",
            s.chapter,
            r.e.place(),
            r.e.files().current_disc(),
            r.var("MONDE"),
            r.e.puzzle_mode(),
            &s.flags[130..180],
            s.bag,
        )
    };
    let before = kept(&r);
    r.disc_panel();
    r.press_widget("a_save.spr");
    r.press_widget("b_oui.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    let journal = std::mem::take(&mut r.journal);
    let mut r = Run::new(dir);
    r.journal = journal;
    r.e.open_player_menu();
    r.wait(5);
    r.press_widget("b_ter.spr");
    assert_eq!((r.page(), r.e.loop_type), (None, LoopType::Warp));
    assert_eq!(kept(&r), before);
    r.wait(30);
    r
}

/// A tool more than a test (ignored): China's stages played one by one,
/// each from the save the previous one left (`A2_DEV_DIR`, `A2_DEV_FROM`,
/// `A2_DEV_TO`; stage `CHINA.len()` is the way back), for working on them.
#[test]
#[ignore]
fn china_stages_from_saves() {
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
    let to = env("A2_DEV_TO", CHINA.len() + 1);
    if from == 0 {
        // A new game: no save for the player menu to continue.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut r = Run::new(&dir);
    r.e.player_number = 4;
    if from == 0 {
        to_china(&mut r);
        assert!(r.e.save_slot(0));
    } else {
        r.e.load_slot(from as u8).unwrap();
        r.wait(30);
    }
    for i in from..to {
        eprintln!(
            "stage {i}: pass {} place {:?} chapter {}",
            r.pass,
            r.e.place(),
            r.chapter()
        );
        match CHINA.get(i) {
            Some((_, stage)) => {
                stage(&mut r);
                assert!(r.e.save_slot((i + 1) as u8));
            }
            None => {
                china_way_back(&mut r);
            }
        }
    }
    eprintln!(
        "done: pass {} place {:?} chapter {}",
        r.pass,
        r.e.place(),
        r.chapter()
    );
    let journal = std::mem::take(&mut r.journal);
    china_lines_as_the_original(&journal);
}

// ---------------------------------------------------------------------------
// Yucatán, with the mouse.

/// The game from the start-up intro to Yucatán, with the mouse: Yucatán's
/// boat taken in view 5, the walk to the hut (the shaman's greeting on
/// the way), the boat picked from the bar and laid on its zone (the hook,
/// chapter 75's section, film 40), the trip (`MONDE` 2 on disc 2,
/// `yucatan.wam` view 10, chapter 25 and its music).
fn to_yucatan(r: &mut Run) {
    tibet_start(r);
    for to in 3..=5 {
        r.walk(to);
    }
    let boat = r.obj("MANDALA_BATEAU_YU");
    let evs = r.click_sprite(|e| matches!(e, Event::Take { .. }));
    assert!(evs.contains(&Event::Take { id: boat }));
    for to in 6..=10 {
        r.walk(to);
    }
    r.pick(boat);
    let zi =
        r.e.wam()
            .view(10)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some("ZONE_YUB"))
            .unwrap();
    r.aim(zi);
    r.tap();
    let mut evs = r.events();
    assert!(evs.contains(&Event::Sound("sound400.apc".into())));
    assert_eq!(r.e.loop_type, LoopType::Video);
    r.wait(60);
    r.shot_as("yucatan-1-departure-film.png");
    evs.extend(r.settle());
    assert!(!evs.iter().any(|e| matches!(e, Event::WorldNotPorted(_))));
    assert!(evs.contains(&Event::Monde(2)));
    assert!(evs.contains(&Event::Wam("yucatan.wam".into())));
    assert!(evs.iter().any(
        |e| matches!(e, Event::VideoStart { name, frames: Some(_) } if name == "cine040.hnm")
    ));
    assert_eq!(r.e.place(), ("yucatan.wam", 10));
    assert_eq!(r.e.files().current_disc(), 2);
    assert_eq!(
        (
            r.chapter(),
            r.var("MONDE"),
            r.var("NbMandalaPosed"),
            r.var("OrdreMandala3"),
            r.var("poseyub"),
            r.var("VignetteSave"),
            r.var("YucatanVisited"),
        ),
        (25, 2, 1, 1, 1, 10, 1)
    );
    assert!(!r.in_bag("MANDALA_BATEAU_YU"));
    r.wait(30);
    assert!(r
        .e
        .music_playing()
        .is_some_and(|m| [11, 12, 15].contains(&m)));
    r.shot_as("yucatan-2-arrival.png");
}

/// The play of Yucatán: what a player does there, besides the others'.
impl Run {
    /// A click on the view's link to view `to` of another place (a
    /// type-2 zone), played out until the arrival.
    fn walk_link_to(&mut self, to: u32) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let zone = self
            .e
            .wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 2 && z.value == to as i32)
            .unwrap_or_else(|| panic!("no link from view {view} to {to}"));
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
                assert_eq!(self.e.place().1, to);
                return all;
            }
        }
        panic!("the link of view {view} to {to} leads nowhere");
    }

    /// A click on the move zone to view `to` whose departure section plays
    /// a dialog or a film first: played out until the panorama is still.
    fn depart(&mut self, to: u32) -> Vec<Event> {
        self.walk_redirected(to)
    }

    /// Character `who` asked about `subject` through his menu: what
    /// follows, played out (an answer that is not a dialog).
    fn ask_events(&mut self, who: &str, subject: &str) -> Vec<Event> {
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
        evs
    }

    /// Character `who` (one of several in the view) asked once about each
    /// subject his menu offers, each through the menu opened by a click on
    /// him: the subjects and their dialogs, in order.
    fn ask_each(&mut self, who: &str) -> Vec<(i32, u32)> {
        let character = self.who(who);
        let mut asked: Vec<(i32, u32)> = Vec::new();
        loop {
            let list = self.e.script_state().unwrap().subjects.list(character);
            let Some(entry) = list.iter().find(|e| asked.iter().all(|a| a.0 != e.value)) else {
                return asked;
            };
            let value = entry.value;
            self.stow();
            let evs = self.click_on(true, character);
            assert!(evs.contains(&Event::SubjectMenu {
                character,
                count: list.len() as i32
            }));
            let i = list.iter().position(|e| e.value == value).unwrap();
            let (x, y) = cell(list.len() as i32, i as i32);
            self.point(x + 31, y + 31);
            self.step();
            self.tap();
            let mut evs = self.events();
            evs.extend(self.settle());
            let d = dialogs(&evs)
                .first()
                .copied()
                .unwrap_or_else(|| panic!("{who} {value}: no dialog"));
            asked.push((value, d));
            assert!(asked.len() < 16, "the subjects never run out");
        }
    }

    /// The subjects character `who` offers now, by name.
    fn offers(&self, who: &str) -> Vec<i32> {
        let s = self.e.script_state().unwrap();
        s.subjects
            .list(self.who(who))
            .iter()
            .map(|e| e.value)
            .collect()
    }
}

/// Yucatán's story without Xibalba, stage by stage, as chapters 25-34
/// make it: each stage starts and ends in the panorama.
const YUCATAN: &[Stage] = &[
    (
        "the prince and the high priest",
        the_prince_and_the_high_priest,
    ),
    ("the priestess", the_priestess),
    ("the king", the_king),
    ("the king's hall with the bloody bands", the_halls_mourning),
    ("the bloody bands", the_bloody_bands),
    ("the priestess's question", the_priestess_question),
    ("the obsidian", the_obsidian),
    ("the first guard", the_first_guard),
    ("the Maya numbers", the_maya_numbers),
    ("the first grid", the_first_grid),
    ("the second guard", the_second_guard),
    ("the entrance door", the_entrance_door),
    ("the crocodile's foot", the_crocodiles_foot),
    ("the mirrors", the_mirrors),
];

/// The views of the king's hall (`salleroi.wam` 86-88), where the king,
/// his daughter and the counsellor stand.
const HALL: [u32; 3] = [86, 87, 88];

impl Run {
    /// Character `who` of the king's hall found (the views of [`HALL`]
    /// walked until his sprite can be clicked) and asked about each
    /// subject he offers: the subjects' names and their dialogs, sorted.
    fn ask_in_hall(&mut self, who: &str) -> Vec<(String, u32)> {
        let id = self.who(who);
        let here = |r: &Run| {
            r.e.anim
                .slots
                .iter()
                .any(|s| s.id == id && s.active && s.click != 0 && matches!(s.kind, 1 | 3))
        };
        for v in HALL {
            if here(self) {
                break;
            }
            self.go(v, &[]);
        }
        assert!(here(self), "{who} is in none of the hall's views");
        let mut asked: Vec<(String, u32)> = self
            .ask_each(who)
            .into_iter()
            .map(|(value, d)| (self.subject_name(value), d))
            .collect();
        asked.sort();
        asked
    }

    /// The `defnames.con` name of subject value `value`.
    fn subject_name(&self, value: i32) -> String {
        let s = self.e.script_state().unwrap();
        [
            "PRINCE",
            "GRAND_PRETRE",
            "PRETRESSE",
            "ROI",
            "FILLE_ROI",
            "CONSEILLER",
            "MAIS",
            "ENNEMI",
            "SUPERNOVA_YU",
            "BANDELETTES",
            "BANDELETTES_SANG",
            "CHIFFRE_MAYA",
            "TEZ",
        ]
        .into_iter()
        .find(|n| s.names.param(defnames::SUJETS, n.as_bytes()) == value)
        .unwrap_or_else(|| panic!("subject {value} not one of the hall's"))
        .to_string()
    }
}

/// `(subject, dialog)` pairs, sorted.
fn answers(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = pairs.iter().map(|&(s, d)| (s.to_string(), d)).collect();
    v.sort();
    v
}

/// Views 17, 19, 22, 39 and the link to the king's hall's view 81
/// (chapter 25's departure: chapter 26, whose arrival section turns the
/// view, `AngleWarpXNow=270`, and plays dialog 101); in view 82 the prince
/// and the high priest asked about everything they offer.
fn the_prince_and_the_high_priest(r: &mut Run) {
    r.route(&[17, 19, 22, 39]);
    let evs = r.walk_link_to(81);
    assert!(dialogs(&evs).contains(&101));
    assert_eq!((r.e.place(), r.chapter()), (("salleroi.wam", 81), 26));
    assert_eq!(r.flag(76), 1);
    r.shot_as("yucatan-3-kings-hall.png");
    r.walk(82);
    r.head_passes = 0;
    let mut asked = r.ask_each("PRINCE");
    asked.sort_unstable();
    let mut want: Vec<(i32, u32)> = [("PRINCE", 104), ("MAIS", 102), ("ROI", 103)]
        .iter()
        .map(|&(n, d)| (r.subject(n), d))
        .collect();
    want.sort_unstable();
    assert_eq!(asked, want);
    assert!(r.head_passes > 0, "no head drawn for the prince");
    r.shot_as("yucatan-4-prince.png");
    let mut asked = r.ask_each("GRAND_PRETRE");
    asked.sort_unstable();
    let mut want: Vec<(i32, u32)> = [("GRAND_PRETRE", 106), ("SUPERNOVA_YU", 105)]
        .iter()
        .map(|&(n, d)| (r.subject(n), d))
        .collect();
    want.sort_unstable();
    assert_eq!(asked, want);
}

/// Views 81, 80, 83 (chapter 26's arrival: chapter 27, whose `/INIT`
/// plays dialog 107 and gives the clean bands); the priestess of view 84
/// asked about everything she offers.
fn the_priestess(r: &mut Run) {
    r.walk(81);
    r.walk(80);
    let evs = r.walk(83);
    assert!(dialogs(&evs).contains(&107));
    assert_eq!(r.chapter(), 27);
    assert!(r.in_bag("BANDES_PROPRES"));
    r.stow();
    r.walk(84);
    let asked = r.ask_each("PRETRESSE");
    let mut got: Vec<u32> = asked.iter().map(|a| a.1).collect();
    got.sort_unstable();
    assert_eq!(got, [108, 109, 110, 111, 112, 113]);
    r.shot_as("yucatan-5-priestess.png");
}

/// Views 85, 86 (chapter 27's departure: chapter 28, the king's greeting,
/// dialog 138); the king, his daughter and the counsellor asked about
/// everything (chapter 28's sections 3-5: dialogs 139, 141, 140 whatever
/// the subject); the priestess clicked again (view 84: dialog 114); view
/// 88 (chapter 28 again): the clean bands given to the king (dialog 142,
/// film 41); the three asked again (sections 7-9: dialogs 145-153); back
/// to view 86, then the move to 85 (chapter 28's departure: dialog 143,
/// the bloody bands, chapter 27).
fn the_king(r: &mut Run) {
    r.walk(85);
    let evs = r.walk(86);
    assert!(dialogs(&evs).contains(&138));
    assert_eq!(r.chapter(), 28);
    r.stow();
    assert_eq!(
        r.ask_in_hall("ROI"),
        answers(&[
            ("ENNEMI", 139),
            ("FILLE_ROI", 139),
            ("MAIS", 139),
            ("PRINCE", 139)
        ])
    );
    assert_eq!(
        r.ask_in_hall("FILLE_ROI"),
        answers(&[("FILLE_ROI", 141), ("ROI", 141)])
    );
    assert_eq!(
        r.ask_in_hall("CONSEILLER"),
        answers(&[
            ("GRAND_PRETRE", 140),
            ("MAIS", 140),
            ("ROI", 140),
            ("SUPERNOVA_YU", 140)
        ])
    );
    // Back to the priestess with the bands still in the bag: she asks
    // whether they were given (chapter 27's section 5: dialog 114).
    r.go(84, &[]);
    assert_eq!(r.chapter(), 27);
    let priestess = r.who("PRETRESSE");
    let mut evs = r.click_on(true, priestess);
    evs.extend(r.settle());
    assert_eq!(dialogs(&evs), [114]);
    r.go(88, &[]);
    assert_eq!(r.chapter(), 28);
    let evs = r.give("BANDES_PROPRES", "ROI");
    assert!(dialogs(&evs).contains(&142));
    assert!(films(&evs).contains(&41));
    assert_eq!(r.flag(77), 1);
    assert!(!r.in_bag("BANDES_PROPRES"));
    r.stow();
    r.shot_as("yucatan-6-king.png");
    assert_eq!(
        r.ask_in_hall("ROI"),
        answers(&[
            ("ENNEMI", 146),
            ("FILLE_ROI", 147),
            ("MAIS", 145),
            ("PRINCE", 148)
        ])
    );
    assert_eq!(
        r.ask_in_hall("FILLE_ROI"),
        answers(&[("FILLE_ROI", 152), ("ROI", 153)])
    );
    assert_eq!(
        r.ask_in_hall("CONSEILLER"),
        answers(&[("GRAND_PRETRE", 150), ("ROI", 151), ("SUPERNOVA_YU", 149)])
    );
    r.go(86, &[]);
    let evs = r.depart(85);
    assert!(dialogs(&evs).contains(&143));
    assert_eq!((r.e.place().1, r.chapter()), (85, 27));
    assert!(r.in_bag("BANDES_SANG"));
    assert_eq!(r.flag(78), 1);
    r.stow();
}

/// Back to view 86 with the bloody bands (chapter 27's departure: chapter
/// 28): the king, his daughter and the counsellor in mourning (sections
/// 11-13: dialogs 154, 156, 155 whatever the subject); view 85 again
/// (section 10: chapter 27).
fn the_halls_mourning(r: &mut Run) {
    r.walk(86);
    assert_eq!(r.chapter(), 28);
    assert_eq!(
        r.ask_in_hall("ROI"),
        answers(&[("BANDELETTES_SANG", 154), ("FILLE_ROI", 154), ("ROI", 154)])
    );
    assert_eq!(
        r.ask_in_hall("FILLE_ROI"),
        answers(&[("GRAND_PRETRE", 156), ("PRETRESSE", 156), ("PRINCE", 156)])
    );
    assert_eq!(
        r.ask_in_hall("CONSEILLER"),
        answers(&[("GRAND_PRETRE", 155), ("PRETRESSE", 155), ("PRINCE", 155)])
    );
    r.go(86, &[]);
    r.walk(85);
    assert_eq!(r.chapter(), 27);
}

/// View 84: the priestess clicked with an empty hand asks for the bands
/// (chapter 27's section 5: dialog 210); the bloody bands given to her
/// (dialog 115, film 42, dialog 116): her subjects now name Tezcatlipoca.
fn the_bloody_bands(r: &mut Run) {
    r.walk(84);
    r.stow();
    let priestess = r.who("PRETRESSE");
    let mut evs = r.click_on(true, priestess);
    evs.extend(r.settle());
    assert_eq!(dialogs(&evs), [210]);
    assert_eq!(r.flag(80), 0);
    let evs = r.give("BANDES_SANG", "PRETRESSE");
    let d = dialogs(&evs);
    assert!(d.contains(&115) && d.contains(&116), "{d:?}");
    assert!(films(&evs).contains(&42));
    assert_eq!(r.flag(80), 1);
    assert!(!r.in_bag("BANDES_SANG"));
    assert!(r.offers("PRETRESSE").contains(&r.subject("TEZ")));
}

/// The priestess asked about Tezcatlipoca (dialog 122, a yes or no
/// question): no (`flag82`); the way into Xibalba, her yes, is taken
/// later ([`the_way_into_xibalba`]).
fn the_priestess_question(r: &mut Run) {
    assert_eq!(r.ask("PRETRESSE", "TEZ"), 122);
    assert_eq!(r.var("Pretresse_AskOuiNon"), 1);
    r.ask_events("PRETRESSE", "NON");
    assert_eq!((r.flag(82), r.var("Pretresse_AskOuiNon")), (1, 0));
    assert_eq!(r.e.place(), ("salleroi.wam", 84));
    // After a no, Tezcatlipoca is the shorter question (dialog 123), no
    // again; then everything else she offers (section 7: dialogs
    // 117-121).
    assert_eq!(r.ask("PRETRESSE", "TEZ"), 123);
    assert_eq!(r.var("Pretresse_AskOuiNon"), 1);
    r.ask_events("PRETRESSE", "NON");
    assert_eq!(r.var("Pretresse_AskOuiNon"), 0);
    for (subject, dialog) in [
        ("PRINCE", 117),
        ("GRAND_PRETRE", 118),
        ("ROI", 119),
        ("FILLE_ROI", 120),
        ("SUPERNOVA_YU", 121),
    ] {
        assert_eq!(r.ask("PRETRESSE", subject), dialog, "{subject}");
    }
    // Each answered subject greyed (`DisableSuj`): only Tezcatlipoca left.
    let s = r.e.script_state().unwrap();
    let left: Vec<i32> = s
        .subjects
        .list(r.who("PRETRESSE"))
        .iter()
        .filter(|e| e.enabled())
        .map(|e| e.value)
        .collect();
    assert_eq!(left, [r.subject("TEZ")]);
}

/// Out of the king's hall (views 83, 80: chapter 26; the link to
/// `yucatan.wam` view 39: chapter 25), to the pyramid's top (view 34:
/// chapter 30) and view 40, where the obsidian is taken.
fn the_obsidian(r: &mut Run) {
    r.walk(83);
    r.walk(80);
    assert_eq!(r.chapter(), 26);
    r.walk_link_to(39);
    assert_eq!((r.e.place(), r.chapter()), (("yucatan.wam", 39), 25));
    r.go(34, &[]);
    assert_eq!(r.chapter(), 30);
    r.walk(40);
    r.take("OBSIDIENNE1");
    r.walk(34);
}

/// A counting grid played with the mouse: a click on its zone of type 50
/// of each value in turn (0x11 one more marble, 0x12 five or none, 1..16
/// a cell); the events.
fn play_grid(r: &mut Run, values: &[i32]) -> Vec<Event> {
    let view = r.e.place().1;
    let mut all = Vec::new();
    for &v in values {
        let z = (0..r.e.wam().view(view).unwrap().zones.len())
            .find(|&i| {
                let z = &r.e.wam().view(view).unwrap().zones[i];
                z.kind == 50 && z.value == v
            })
            .unwrap();
        all.extend(r.press_zone(z));
    }
    all
}

/// The clicks that lay number `n` on an empty counting grid as a Maya
/// number is written: its base-20 digits from the units up, digit k on
/// the group of cells 4k + 1..4k + 4 (the units' group is the panel's
/// lowest, its cell 1 the lowest row), each group from its first cell: a
/// bar per five (zone 0x12, five marbles in hand, then the cell), then the
/// dots left (zone 0x11 once a marble, then the cell). Each lay is on an
/// empty cell, so the hand is empty after it.
fn maya_clicks(mut n: u32) -> Vec<i32> {
    let mut clicks = Vec::new();
    let mut first = 1;
    while n > 0 {
        let (digit, mut cell) = (n % 20, first);
        for _ in 0..digit / 5 {
            clicks.extend([0x12, cell]);
            cell += 1;
        }
        if digit % 5 > 0 {
            clicks.extend(std::iter::repeat_n(0x11, (digit % 5) as usize));
            clicks.push(cell);
        }
        n /= 20;
        first += 4;
    }
    clicks
}

/// The numbers the guards' riddles name: the bat's is 3 (dialog 169: the
/// jaguar's is the bat's times itself; dialog 170: the jaguar's times
/// itself is the serpent's), and the third grid the serpent's times
/// itself. They are the grids' solutions (@ 0x005e187c..: 9, 81, 6561).
const BAT: u32 = 3;
const JAGUAR: u32 = BAT * BAT;
const SERPENT: u32 = JAGUAR * JAGUAR;
const THIRD_GRID: u32 = SERPENT * SERPENT;

/// The link into the pyramid (chapter 30's departure: chapter 31), views
/// 51-54: the first guard (dialog 169, his riddle); view 55, the first
/// counting grid, seen (`flag94`); back in view 54 the way on to view 63
/// is barred (chapter 31's section 2: `abortdepart`).
fn the_first_guard(r: &mut Run) {
    r.walk_link_to(50);
    assert_eq!((r.e.place(), r.chapter()), (("pyratez.wam", 50), 31));
    r.go(54, &[]);
    let guard = r.who("GARDIEN1");
    let evs = r.click_on(true, guard);
    let mut evs2 = evs.clone();
    evs2.extend(r.settle());
    assert!(dialogs(&evs2).contains(&169));
    r.walk(55);
    assert_eq!(r.flag(94), 1);
    r.shot_as("yucatan-8-grid.png");
    r.walk(54);
    r.walk_redirected(63);
    assert_eq!((r.e.place().1, r.chapter()), (54, 31));
}

/// Out of the pyramid (view 50's link to `yucatan.wam` view 41: chapter
/// 30) to the king's hall (view 39's link to 81: chapter 26; 83: 27; 86:
/// 28, whose `/INIT` gives the king and the counsellor the Maya numbers'
/// subject once the grid was seen); the king, his daughter and the
/// counsellor asked about everything (sections 15-17: dialogs 157-167,
/// the counsellor's 162 on the numbers); back to the pyramid (chapter 31
/// again, its `/INIT` resetting the first grid).
fn the_maya_numbers(r: &mut Run) {
    r.go(50, &[]);
    r.walk_link_to(41);
    assert_eq!((r.e.place(), r.chapter()), (("yucatan.wam", 41), 30));
    r.go(39, &[]);
    assert_eq!(r.chapter(), 25);
    r.walk_link_to(81);
    assert_eq!(r.chapter(), 26);
    r.go(83, &[]);
    assert_eq!(r.chapter(), 27);
    r.go(86, &[84]);
    assert_eq!(r.chapter(), 28);
    assert!(r.offers("CONSEILLER").contains(&r.subject("CHIFFRE_MAYA")));
    assert_eq!(
        r.ask_in_hall("ROI"),
        answers(&[
            ("CHIFFRE_MAYA", 157),
            ("MAIS", 157),
            ("PRETRESSE", 157),
            ("SUPERNOVA_YU", 157)
        ])
    );
    assert_eq!(
        r.ask_in_hall("CONSEILLER"),
        answers(&[
            ("CHIFFRE_MAYA", 162),
            ("MAIS", 160),
            ("PRETRESSE", 159),
            ("ROI", 161),
            ("SUPERNOVA_YU", 158)
        ])
    );
    assert_eq!(
        r.ask_in_hall("FILLE_ROI"),
        answers(&[
            ("FILLE_ROI", 166),
            ("PRETRESSE", 167),
            ("PRINCE", 164),
            ("ROI", 165),
            ("SUPERNOVA_YU", 163)
        ])
    );
    r.go(86, &[]);
    r.walk(85);
    assert_eq!(r.chapter(), 27);
    r.go(80, &[84]);
    assert_eq!(r.chapter(), 26);
    r.walk_link_to(39);
    r.go(34, &[]);
    assert_eq!(r.chapter(), 30);
    r.walk_link_to(50);
    assert_eq!(r.chapter(), 31);
    r.go(54, &[]);
}

/// The first grid in view 55 (the jaguar's number, 9: a bar and four dots
/// in the lowest group), played as a player does: first three dots (the
/// bat's number) in the lowest row, which wins nothing; then a bar on that
/// row (the three dots come back in hand), the three laid on the row above,
/// four dots on it (three come back: the grid won, sound 101), and the
/// three put back one by one (zone 0x13). The guard again: he lets the hero
/// pass after five seconds; view 63 (chapter 32, whose arrival section
/// turns the view).
fn the_first_grid(r: &mut Run) {
    let guard = r.who("GARDIEN1");
    r.walk(55);
    let evs = play_grid(r, &[0x11, 0x11, 0x11, 1]);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_31a0
    }));
    assert_eq!(&r.e.grid_digits(0)[..16], b"0000000000000003");
    assert_eq!(r.var("finmaya1"), 0);
    play_grid(r, &[0x12, 1]);
    assert_eq!(r.e.grids.hand, 3);
    play_grid(r, &[2]);
    assert_eq!(r.e.grids.hand, 0);
    let evs = play_grid(r, &[0x11, 0x11, 0x11, 0x11, 2]);
    assert!(evs.contains(&Event::Sound("sound101.apc".into())));
    assert_eq!(&r.e.grid_digits(0)[..16], b"0000000000000045");
    assert_eq!(r.var("finmaya1"), 1);
    assert_eq!(r.e.grids.hand, 3);
    r.shot_as("yucatan-8b-grid-won.png");
    play_grid(r, &[0x13, 0x13, 0x13]);
    assert_eq!(r.e.grids.hand, 0);
    let evs = r.walk(54);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_2d60
    }));
    assert_eq!(r.var("finmaya1"), 1);
    r.click_on(true, guard);
    assert_eq!(r.var("gardien1Gentil"), 1);
    r.wait(6 * 60);
    assert_eq!(r.var("gardien1Gentil"), 2);
    let evs = r.walk(63);
    assert!(evs.contains(&Event::Set {
        chapter: 31,
        line: 17
    }));
    assert_eq!(r.chapter(), 32);
    // `AngleWarpXNow=360`: the read-back yaw 90 - 360 = -270 (90).
    let (yaw, pitch) = r.e.camera.angles();
    assert!((f64::from(yaw).to_degrees() - 90.0).abs() < 1e-3, "{yaw}");
    assert!(pitch.abs() < 1e-6);
}

/// The second guard (view 58, by 62, 60, 61): his riddle (dialog 170),
/// the way on to view 67 barred (chapter 32's section 3); the second grid
/// (view 64) laid with the serpent's number, 81 (a dot in the lowest
/// group, four in the one above); the guard again, view 67 (chapter 33).
fn the_second_guard(r: &mut Run) {
    r.go(58, &[54, 64]);
    let guard = r.who("GARDIEN2");
    let mut evs = r.click_on(true, guard);
    evs.extend(r.settle());
    assert_eq!(dialogs(&evs), [170]);
    r.walk_redirected(67);
    assert_eq!((r.e.place().1, r.chapter()), (58, 32));
    r.go(64, &[54, 63]);
    let clicks = maya_clicks(SERPENT);
    assert_eq!(clicks, [0x11, 1, 0x11, 0x11, 0x11, 0x11, 5]);
    let evs = play_grid(r, &clicks);
    assert!(evs.contains(&Event::Sound("sound101.apc".into())));
    assert_eq!(&r.e.grid_digits(1)[..16], b"0000000000040001");
    assert_eq!(r.var("finmaya2"), 1);
    r.go(58, &[54, 63]);
    r.click_on(true, guard);
    assert_eq!(r.var("gardien2Gentil"), 1);
    r.wait(6 * 60);
    assert_eq!(r.var("gardien2Gentil"), 2);
    r.walk(67);
    assert_eq!(r.chapter(), 33);
}

/// View 68: the closed entrance door bars the way to view 69 (chapter 33's
/// section 2); its button opens it (`ClicObjet=BOUTON_S`, sound 112), the
/// other button shown closes it (`BOUTON_R`, sound 113), the first opens
/// it again; view 69 (chapter 34).
fn the_entrance_door(r: &mut Run) {
    r.walk(68);
    r.walk_redirected(69);
    assert_eq!((r.e.place().1, r.chapter()), (68, 33));
    for (button, sound, open) in [
        ("BOUTON_S", "sound112.apc", 1),
        ("BOUTON_R", "sound113.apc", 0),
        ("BOUTON_S", "sound112.apc", 1),
    ] {
        let id = r.obj(button);
        let mut evs = r.click_on(false, id);
        evs.extend(r.settle());
        assert!(sounds(&evs).contains(&sound.to_string()), "{button}");
        assert_eq!(r.flag(101), open, "{button}");
    }
    r.walk(69);
    assert_eq!(r.chapter(), 34);
}

/// The third grid (view 75, by 72 and 78) laid with the serpent's number
/// times itself, 6561 (one dot; a bar and three dots; three bars and a
/// dot, in the three lowest groups): its last click wins and opens the
/// crocodile's jaw in the same frame (chapter 34's section 2, sound 112,
/// the entrance door open too), whose foot is taken; that closes the
/// entrance door (sound 113): from view 69 the way out to 68 is barred
/// (chapter 34's section 1).
fn the_crocodiles_foot(r: &mut Run) {
    r.go(75, &[]);
    let clicks = maya_clicks(THIRD_GRID);
    assert_eq!(
        clicks,
        [17, 1, 18, 5, 17, 17, 17, 6, 18, 9, 18, 10, 18, 11, 17, 12]
    );
    let (last, before) = clicks.split_last().unwrap();
    let evs = play_grid(r, before);
    assert_eq!(r.var("croco_ouvert"), 0);
    assert!(evs
        .iter()
        .all(|x| !matches!(x, Event::Sound(s) if s == "sound112.apc")));
    let evs = play_grid(r, &[*last]);
    assert_eq!(&r.e.grid_digits(2)[..16], b"0000155500350001");
    assert_eq!(r.var("finmaya3"), 1);
    assert_eq!(r.var("croco_ouvert"), 1);
    assert!(evs.contains(&Event::Sound("sound112.apc".into())));
    assert_eq!(r.flag(101), 1);
    let z = r.zone_named("GUEULE_CROCO");
    let evs = r.use_on(None, z);
    assert!(sounds(&evs).contains(&"sound113.apc".to_string()));
    assert_eq!(r.var("pied_pris"), 1);
    assert_eq!(r.flag(101), 0);
    assert!(r.in_bag("PIED2_CROCO"));
    r.stow();
    r.shot_as("yucatan-9-crocodile.png");
    r.go(69, &[]);
    r.walk_redirected(68);
    assert_eq!((r.e.place().1, r.chapter()), (69, 34));
}

/// View 71: the foot and the obsidian laid in the two mirror holes; the
/// secret door opens (sound 110); both taken back with an empty hand (the
/// door stays open: Xibalba's statues need them); its link back to
/// `yucatan.wam` view 28 (chapter 34's departure: sound 111, chapter 25).
fn the_mirrors(r: &mut Run) {
    r.go(71, &[]);
    let z1 = r.zone_named("TROU_MIROIR1");
    r.use_on(Some("PIED2_CROCO"), z1);
    assert_eq!(r.var("trou1_remplis"), 1);
    let z2 = r.zone_named("TROU_MIROIR2");
    let evs = r.use_on(Some("OBSIDIENNE1"), z2);
    assert!(sounds(&evs).contains(&"sound110.apc".to_string()));
    assert_eq!(r.var("porte_secrete_ouverte"), 1);
    assert!(!r.in_bag("PIED2_CROCO") && !r.in_bag("OBSIDIENNE1"));
    r.use_on(None, z1);
    assert!(r.in_bag("PIED2_CROCO"));
    r.use_on(None, z2);
    assert!(r.in_bag("OBSIDIENNE1"));
    r.stow();
    assert_eq!(
        (
            r.var("trou1_remplis"),
            r.var("trou2_remplis"),
            r.var("porte_secrete_ouverte")
        ),
        (0, 0, 1)
    );
    let evs = r.walk_link_to(28);
    assert!(sounds(&evs).contains(&"sound111.apc".to_string()));
    assert_eq!((r.e.place(), r.chapter()), (("yucatan.wam", 28), 25));
}

/// The way back: to the brazier of view 35 (chapter 30), clicked with an
/// empty hand: Tibet's view 7, chapter 75.
fn yucatan_way_back(r: &mut Run) -> Vec<Event> {
    r.go(35, &[]);
    assert_eq!(r.chapter(), 30);
    let z = r.zone_named("BRAZERO_ZONE");
    let evs = r.use_on(None, z);
    assert!(evs.contains(&Event::Monde(5)));
    assert!(evs.contains(&Event::Wam("tibet.wam".into())));
    evs
}

/// The whole of Yucatán's story, stage by stage, after [`to_yucatan`]:
/// [`YUCATAN`], the brazier back to Tibet and the second trip, the way
/// into Xibalba, [`XIBALBA`], the way back with the soul and Yucatán's
/// mandala laid in Tibet.
fn whole_yucatan() -> Vec<Stage> {
    let mut v: Vec<Stage> = YUCATAN.to_vec();
    v.push(("the brazier", |r| {
        yucatan_way_back(r);
        r.wait(10);
        r.shot_as("yucatan-10-back-in-tibet.png");
        assert_eq!(r.e.place(), ("tibet.wam", 7));
        assert_eq!((r.chapter(), r.var("MONDE")), (75, 5));
        assert_eq!(r.e.files().current_disc(), 1);
        assert!(!r.in_bag("MANDALA_YUCATAN"));
    }));
    v.push(("the second trip", the_second_trip));
    v.push(("the way into Xibalba", the_way_into_xibalba));
    v.extend_from_slice(XIBALBA);
    v.push(("the way back with the soul", |r| {
        xibalba_way_back(r);
        r.wait(10);
        r.shot_as("xibalba-10-back-in-tibet.png");
        assert_eq!(r.e.place(), ("tibet.wam", 7));
        assert_eq!((r.chapter(), r.var("MONDE")), (75, 5));
        assert_eq!(r.e.files().current_disc(), 1);
        assert_eq!(r.var("dial168_done"), 1);
        assert!(r.in_bag("MANDALA_YUCATAN") && !r.in_bag("CHULEL"));
    }));
    v.push(("Yucatán's mandala laid", yucatans_mandala_laid));
    v
}

/// The Yucatán chapters a game goes through (29 is empty and named by no
/// `chapter=` line).
const YUCATAN_CHAPTERS: [i32; 9] = [25, 26, 27, 28, 30, 31, 32, 33, 34];

/// Every `/set` line of a Yucatán or Xibalba chapter that ran (the
/// events' chapter and line) is one of the lines the original's handler
/// ran (`scripts_worlds_china.txt`, which holds chapters 25-60).
fn yucatan_lines_as_the_original(evs: &[Event]) {
    let golden = include_str!("golden/scripts_worlds_china.txt");
    let mut n = 0;
    for e in evs {
        if let Event::Set { chapter, line } = e {
            if (25..=41).contains(chapter) {
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

/// The dialogs of chapters 25-41 the game plays: Yucatán's but 124, 126,
/// 127, 128, 130, 131, 133 and 137 (the priestess's answers at other
/// stages of Xibalba) and 144 (none: no script names it and `DialYU.txt` has no
/// line of it), 168 (played over film 55, not started as a dialog);
/// Xibalba's but those of the stages a won puzzle skips
/// (187-189, the spider's, and 208, the canoe started and not won) and
/// the bat's answers at other stages.
fn yucatan_dialogs_played() -> Vec<u32> {
    let mut v: Vec<u32> = (101..=123).chain(138..=143).chain(145..=167).collect();
    v.extend([169, 170]);
    v.extend([125, 129, 132, 134, 135, 136]);
    v.extend((171..=176).chain(184..=186).chain(190..=202));
    v.extend([204, 210]);
    v.sort_unstable();
    v
}

/// A game from the start-up intro through Yucatán, Xibalba and back, with
/// the mouse only: [`to_yucatan`], then Yucatán's story as its chapters
/// 25-34 make it ([`YUCATAN`]: the prince and the high priest, the
/// priestess, the king's hall and the bands, the priestess's question
/// declined, the obsidian, the pyramid's guards and their riddles, the
/// king's hall again for the Maya numbers, the counting grids played as a
/// player does (a wrong try on the first; the others laid from the
/// riddles' numbers), the entrance door and its buttons, the crocodile's
/// foot, the mirrors and the secret door), a save and a load through the
/// menus, the way back to Tibet by the brazier and the second trip; then
/// the priestess's yes and Xibalba as chapters 35-39 make it
/// ([`XIBALBA`]: the bat, the canoe played with the mouse, the web's
/// three rounds and Chac, the priestess visited on the way, the star map,
/// the statues and the serpent, the soul), the web played (its rounds won
/// as the original wins them), every puzzle hook run and none noted, a
/// save and a load in Xibalba; the way back with the soul to the king (the end of
/// Yucatán) and Yucatán's mandala laid in Tibet. Every chapter it goes
/// through is left by one of its own `chapter=` lines, no world is
/// refused, and every `/set` line of chapters 25-41 reported is one the
/// original runs.
#[test]
fn yucatan_with_the_mouse() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow-yucatan");
    let mut r = Run::new(&dir);
    to_yucatan(&mut r);
    for (name, stage) in whole_yucatan() {
        stage(&mut r);
        assert_eq!(r.e.loop_type, LoopType::Warp, "after {name}");
        if name == "the priestess's question" || name == "the star map" {
            // One save at a time: the player menu continues the latest,
            // and the test's clock does not move.
            for f in std::fs::read_dir(dir.join("save")).into_iter().flatten() {
                std::fs::remove_file(f.unwrap().path()).unwrap();
            }
            r = saved_and_continued(r, &dir);
        }
    }
    let journal = std::mem::take(&mut r.journal);
    let mut played = dialogs(&journal);
    played.retain(|d| (101..=210).contains(d));
    played.sort_unstable();
    played.dedup();
    assert_eq!(played, yucatan_dialogs_played());
    yucatan_lines_as_the_original(&journal);
    chapters_ended(&r.e, &journal, &YUCATAN_CHAPTERS);
    chapters_ended(&r.e, &journal, &XIBALBA_CHAPTERS);
    // Every puzzle hook ran as the original's, none is left noted.
    assert!(!journal
        .iter()
        .any(|e| matches!(e, Event::PuzzleNotPorted { .. })));
    let mut ran: Vec<u32> = journal
        .iter()
        .filter_map(|e| match e {
            Event::PuzzleRan { function } => Some(*function),
            _ => None,
        })
        .collect();
    ran.sort_unstable();
    ran.dedup();
    for function in XIBALBA_RAN {
        assert!(ran.contains(&function), "{function:#010x} not run");
    }
    assert!(!journal
        .iter()
        .any(|e| matches!(e, Event::WorldNotPorted(_))));
}

/// A tool more than a test (ignored): Yucatán's and Xibalba's stages
/// ([`whole_yucatan`]) played one by one, each from the save the previous
/// one left (`A2_DEV_DIR`, `A2_DEV_FROM`, `A2_DEV_TO`), for working on
/// them.
#[test]
#[ignore]
fn yucatan_stages_from_saves() {
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
    let stages = whole_yucatan();
    let from = env("A2_DEV_FROM", 0);
    let to = env("A2_DEV_TO", stages.len());
    if from == 0 {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut r = Run::new(&dir);
    r.e.player_number = 4;
    if from == 0 {
        to_yucatan(&mut r);
        assert!(r.e.save_slot(0));
    } else {
        r.e.load_slot(from as u8).unwrap();
        r.wait(30);
    }
    for (i, (name, stage)) in stages.iter().enumerate().take(to).skip(from) {
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
    yucatan_lines_as_the_original(&journal);
}

// ---------------------------------------------------------------------------
// Xibalba, with the mouse.

/// The hooks of Xibalba's puzzles not ported: noted where a player
/// The puzzle functions of Xibalba a player's game runs, each noted
/// [`Event::PuzzleRan`]: the web's entry of view 198, its mode-1 entry
/// and its move click; the star map's entry, click and win test; the
/// canoe's start (`pz_pirogue_init`) and its quit after the win.
const XIBALBA_RAN: [u32; 8] = [
    0x0048_f530,
    0x0048_f9a0,
    0x0048_fdd0,
    0x0049_4e00,
    0x0049_4e80,
    0x0049_4f60,
    0x0049_6a60,
    0x0049_a570,
];

impl Run {
    /// A click on zone `zone` (a `#ZONES_CLIC#` name) that opens the
    /// subject menu of character `who` (the Tezcatlipoca statue's branch),
    /// then subject `subject` chosen: the dialog its answer starts.
    fn ask_by_zone(&mut self, zone: &str, who: &str, subject: &str) -> u32 {
        let character = self.who(who);
        let value = self.subject(subject);
        self.stow();
        let z = self.zone_named(zone);
        self.aim(z);
        self.tap();
        let evs = self.events();
        assert!(
            evs.iter()
                .any(|e| matches!(e, Event::SubjectMenu { character: c, .. } if *c == character)),
            "{who}: no subject menu from zone {zone}"
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

    /// Passes until dialog `n` starts (a special event's end waits for an
    /// animation), then the game settled: the events.
    fn until_dialog(&mut self, n: u32) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..3000 {
            let evs = self.events();
            let started = evs.contains(&Event::DialogStart {
                number: n,
                started: true,
            });
            all.extend(evs);
            if started {
                all.extend(self.settle());
                return all;
            }
        }
        panic!("dialog {n} never starts");
    }

    /// The subjects of `who` asked once each ([`Run::ask_each`]): their
    /// names and dialogs, sorted.
    fn ask_each_named(&mut self, who: &str) -> Vec<(String, u32)> {
        let names = self.e.script_state().unwrap().names.clone();
        let mut v: Vec<(String, u32)> = self
            .ask_each(who)
            .into_iter()
            .map(|(value, d)| (subject_name(&names, value), d))
            .collect();
        v.sort();
        v
    }

    /// From any place of Xibalba to the river (`riviere.wam`, chapter 36):
    /// by the bat island's view 404, the clearing's view 124 or Chac's
    /// pond's view 117 and their links.
    fn reach_the_river(&mut self) {
        match self.e.place().0 {
            "riviere.wam" => return,
            "bat.wam" => {
                if self.e.place().1 != 404 {
                    self.go(403, &[]);
                    // From view 403 the move to 504 goes on to 404 once the
                    // canoe is won (chapter 35's `nextvue=404`).
                    self.walk_redirected(504);
                    assert_eq!(self.e.place().1, 404);
                }
                self.walk_link_to(300);
            }
            "xibalba.wam" if (117..=118).contains(&self.e.place().1) || self.e.place().1 == 123 => {
                self.go(117, &[]);
                self.walk_link_to(311);
            }
            "xibalba.wam" => {
                self.go(124, &[]);
                self.walk_link_to(301);
            }
            "toile.wam" => {
                self.go(198, &[]);
                self.walk_link_to(314);
            }
            p => panic!("{p} is not Xibalba's"),
        }
        assert_eq!((self.e.place().0, self.chapter()), ("riviere.wam", 36));
    }

    /// From the river to the place a link of view `from` leads to (`to`).
    fn river_to(&mut self, from: u32, to: u32) -> Vec<Event> {
        self.reach_the_river();
        self.go(from, &[]);
        self.walk_link_to(to)
    }
}

/// The `defnames.con` name of subject value `value`.
fn subject_name(names: &DefNames, value: i32) -> String {
    [
        "CHAUVE_SOURIS",
        "QUETZ",
        "ROI",
        "PONT",
        "PIROGUE",
        "ETOILE",
        "CHAC",
        "DEESSE_ARAIGNEE",
        "TEZ",
        "CHULEL",
        "PRETRESSE",
        "HEROS_YU",
    ]
    .into_iter()
    .find(|n| names.param(defnames::SUJETS, n.as_bytes()) == value)
    .unwrap_or_else(|| panic!("subject {value} not one of Xibalba's"))
    .to_string()
}

/// Named `(subject, dialog)` pairs, sorted.
fn named(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
    answers(pairs)
}

/// Tibet's view 10 after the brazier: the laid boat clicked (chapter
/// 75's section with `dial168_done` = 0): film 40, Yucatán's view 10,
/// chapter 25, on disc 2 again.
fn the_second_trip(r: &mut Run) {
    r.go(10, &[]);
    let zi =
        r.e.wam()
            .view(10)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some("ZONE_YUB"))
            .unwrap();
    let evs = r.use_on(None, zi);
    assert!(films(&evs).contains(&40));
    assert_eq!((r.e.place(), r.chapter()), (("yucatan.wam", 10), 25));
    assert_eq!(r.e.files().current_disc(), 2);
}

/// The way into Xibalba: view 39, the king's hall (views 81, 80, 83:
/// chapters 26, 27), the priestess of view 84 asked about Tezcatlipoca
/// (dialog 123 after her first no), yes (chapter 27's section 7: the
/// prince and the high priest hidden, `MONDE` 6 on disc 4, film 43,
/// `bat.wam` view 403, chapter 35, whose `/INIT` sets `flag81` and plays
/// dialog 171).
fn the_way_into_xibalba(r: &mut Run) {
    r.go(39, &[]);
    r.walk_link_to(81);
    assert_eq!(r.chapter(), 26);
    r.route(&[80, 83, 84]);
    assert_eq!(r.chapter(), 27);
    assert_eq!(r.ask("PRETRESSE", "TEZ"), 123);
    let evs = r.ask_events("PRETRESSE", "OUI");
    assert!(evs.contains(&Event::Monde(6)));
    assert!(films(&evs).contains(&43));
    assert!(dialogs(&evs).contains(&171));
    assert!(!evs.iter().any(|e| matches!(e, Event::WorldNotPorted(_))));
    assert_eq!((r.e.place(), r.chapter()), (("bat.wam", 403), 35));
    assert_eq!(r.e.files().current_disc(), 4);
    assert_eq!((r.var("MONDE"), r.flag(81), r.flag(102)), (6, 1, 1));
    r.wait(30);
    assert!(r.e.music_playing().is_some_and(|m| [14, 16].contains(&m)));
}

/// The bat of view 403 asked about everything it offers (chapter 35's
/// section 2): the bridge (dialog 175) enables the canoe.
fn the_bat(r: &mut Run) {
    assert_eq!(
        r.ask_each_named("CHAUVE_SOURIS"),
        named(&[
            ("CHAUVE_SOURIS", 172),
            ("PIROGUE", 176),
            ("PONT", 175),
            ("QUETZ", 173),
            ("ROI", 174)
        ])
    );
    assert_eq!((r.var("enable_pz_pirog"), r.flag(106)), (1, 1));
    r.shot_as("xibalba-01-the-bat.png");
}

/// View 402: the canoe's zone clicked (FUN_0049c010(1): the 3D canoe,
/// loop type 9), the canoe played with the mouse to its win: chapter 35's
/// first win sends the game to `xibalba.wam` view 124, chapter 39.
fn the_canoe(r: &mut Run) {
    r.walk(402);
    let z = r.zone_valued(469);
    let evs = r.use_on(None, z);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_6a60
    }));
    assert_eq!((r.e.loop_type, r.flag(103)), (LoopType::Canoe, 1));
    r.shot_as("xibalba-02-canoe.png");
    let mut evs = r.play_canoe();
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_a570
    }));
    evs.extend(r.settle());
    assert!(evs.contains(&Event::Wam("xibalba.wam".into())));
    assert_eq!((r.e.place(), r.chapter()), (("xibalba.wam", 124), 39));
    assert_eq!(
        (
            r.flag(103),
            r.var("finpirogue"),
            r.var("Pirogue_JustFinished"),
            r.flag(117)
        ),
        (1, 1, 0, 1)
    );
    r.shot_as("xibalba-03-clearing.png");
}

/// The rounds of the web from view 198, as the original plays them
/// (`tests/golden/web.txt` section F): each a view (its move zone
/// clicked), or −3 (the feather taken from the bag, then laid by the click
/// on the next move zone).
const WEB_ROUNDS: [&[i32]; 4] = [
    &[
        199, 213, 230, 213, 214, 231, 248, 249, 265, 252, 235, 218, 201, 199,
    ],
    &[
        199, 213, 230, 213, 214, 215, 216, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211,
        212, 213, 230, 213, 214, 215, 216, 217, 234, 251, 234, 217, 216, 199,
    ],
    &[
        199, 213, -3, 214, 214, 213, 230, 247, 248, 264, 265, 252, 235, 218, 201, 199,
    ],
    &[
        199, 213, 230, 213, 214, 215, 216, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211,
        212, 213, 230, 213, 214, 215, 216, 217, 234, 251, 234, 217, 216, 199,
    ],
];

impl Run {
    /// Whether the web's spiders all stand (none walks or turns): the web
    /// takes a move click while one moves.
    fn web_still(&self) -> bool {
        self.e.web.creatures.iter().all(|c| {
            !matches!(c.kind, 1 | 2) || (c.t == 0.0 && c.target == c.heading && c.from == c.to)
        })
    }

    /// Passes until the web's spiders stand.
    fn web_wait(&mut self) {
        for _ in 0..2000 {
            if self.web_still() {
                return;
            }
            self.step();
        }
        panic!("the spiders never stop");
    }

    /// The move zone to view `to` aimed with the mouse, the screen's
    /// centre off the spiders (a click on one is the web's: a dialog, or
    /// the sword's blow), once they stand.
    fn web_aim(&mut self, to: u32) {
        self.web_wait();
        self.wait(15);
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
        for z in zones {
            let (yaw, pitch) = self.e.zone_aim(view, z).unwrap();
            let on = move |e: &Engine| {
                e.centre_zone().map(|(i, _)| i) == Some(z)
                    && !e.web.hit(e.camera.fpu, &e.camera)
                    && e.sprite_under_centre().1 == 0
            };
            let off = 0.6f32.to_radians();
            let mut targets = vec![
                (yaw + off, pitch),
                (yaw - off, pitch),
                (yaw, pitch + off),
                (yaw, pitch - off),
            ];
            // Then towards each corner of the zone (the spider may stand
            // in front of its middle).
            let fpu = self.e.fpu();
            for p in &self.e.wam().view(view).unwrap().zones[z].points {
                let (y, q) = atlantis2_engine::camera::direction_angles(fpu, *p);
                let dy = (y - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                for f in [0.5, 0.75, 0.9] {
                    targets.push((yaw + dy * f, pitch + (q - pitch) * f));
                }
            }
            for (y, p) in targets {
                if self.steer(y, p, &on) {
                    return;
                }
            }
        }
        panic!("no move zone to view {to} off the spiders in view {view}");
    }

    /// A move on the web: the spiders let stand, the zone aimed off them
    /// and clicked, the move's video clicked away; the events, in order.
    fn web_walk(&mut self, to: u32) -> Vec<Event> {
        self.web_aim(to);
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
                all.extend(self.take_events());
                all.extend(self.settle());
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }

    /// A click on the move zone to view `to` that the web takes (the
    /// feather laid, the sword's blow): no move; what follows played out.
    fn web_click(&mut self, to: u32) -> Vec<Event> {
        let view = self.e.place().1;
        self.web_aim(to);
        self.tap();
        let mut all = self.events();
        all.extend(self.settle());
        assert_eq!(self.e.place().1, view, "the click on {to} moved");
        all
    }

    /// The view turned with the mouse onto the web's creature `i` (the
    /// screen's centre on its click quad's direction).
    fn web_look(&mut self, i: usize) {
        let fpu = self.e.camera.fpu;
        let p = self.e.web.creature_pos(fpu, self.e.place().1 as i32, i);
        let d = [p[0] as f32, (p[1] - 20.0) as f32, p[2] as f32];
        let (yaw, pitch) = atlantis2_engine::camera::direction_angles(fpu, d);
        let on = move |e: &Engine| {
            let (y, p) = e.camera.angles();
            let dy = (y - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            dy.abs() < 0.03 && (p - pitch).abs() < 0.03
        };
        self.steer(yaw, pitch, &on);
        self.wait(2);
    }

    /// A round of the web played with the mouse from view 198: `route`
    /// ([`WEB_ROUNDS`]); a picture of the spider after the round's second
    /// move (`pz-web-<stage>-spider.png`) and of the big spider before the
    /// sword's blow (`pz-web-3-blow.png`).
    fn web_play(&mut self, stage: i32, route: &[i32]) -> Vec<Event> {
        let mut all = Vec::new();
        let mut feather = false;
        let mut sword = false;
        for (k, &v) in route.iter().enumerate() {
            if k == 2 {
                self.web_wait();
                self.web_look(0);
                self.shot_as(&format!("pz-web-{stage}-spider.png"));
            }
            if v == -3 {
                let id = self.obj("PLUME");
                self.pick(id);
                feather = true;
                continue;
            }
            let v = v as u32;
            let from = self.e.place().1;
            if feather {
                all.extend(self.web_click(v));
                assert_eq!(self.e.web.feather, v as i32, "the feather not laid");
                feather = false;
            } else if from == 247 && v == 248 && !sword {
                // The sword's blow on the spider standing on 248: film 48.
                assert_eq!(self.var("OBJETENMAIN"), self.obj("EPEE"));
                self.web_wait();
                self.web_look(0);
                self.shot_as("pz-web-3-blow.png");
                let evs = self.web_click(v);
                assert!(films(&evs).contains(&48), "no blow");
                all.extend(evs);
                sword = true;
            } else {
                all.extend(self.web_walk(v));
            }
        }
        all
    }
}

/// To the web by the river (`toile.wam` view 198, chapter 38: its entry,
/// the puzzle mode 1), a round of stage `stage` played with the mouse
/// ([`WEB_ROUNDS`]), left by view 199 (chapter 38's film 47, the web's
/// flight), and back to the river (chapter 36).
fn web_round(r: &mut Run, stage: i32) -> Vec<Event> {
    let mut evs = r.river_to(314, 198);
    assert_eq!((r.e.place(), r.chapter()), (("toile.wam", 198), 38));
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0048_f530
    }));
    assert_eq!((r.e.puzzle_mode(), r.e.web.stage), (1, stage));
    evs.extend(r.web_play(stage, WEB_ROUNDS[stage as usize - 1]));
    assert!(films(&evs).contains(&47), "no flight");
    assert_eq!(r.e.place(), ("toile.wam", 199));
    assert!(!r.e.web.loaded);
    if stage == 1 {
        r.shot_as("xibalba-05-web.png");
    }
    evs.extend(r.walk(198));
    evs.extend(r.walk_link_to(314));
    assert_eq!((r.e.place(), r.chapter()), (("riviere.wam", 314), 36));
    evs
}

/// The web's first round: the caterpillar (sound 104, `finaraign1`).
fn the_caterpillar(r: &mut Run) {
    let evs = web_round(r, 1);
    assert!(sounds(&evs).contains(&"sound104.apc".to_string()));
    assert!(r.in_bag("CHENILLE"));
    assert_eq!(r.var("finaraign1"), 1);
}

/// Chac's pond (the river's view 311 and its link to `xibalba.wam` view
/// 117: chapter 37), view 123 (the first arrival enables Chac as a
/// subject), `prey` given to Chac (his animation 2, special event
/// `event`, which waits for its frame 60): dialog `dialog`.
fn feed_chac(r: &mut Run, prey: &str, event: i32, dialog: u32) -> Vec<Event> {
    r.river_to(311, 117);
    assert_eq!(r.chapter(), 37);
    r.route(&[118, 123]);
    assert_eq!(r.flag(91), 1);
    let id = r.obj(prey);
    r.pick(id);
    let chac = r.who("CHAC");
    let mut evs = r.click_on(true, chac);
    assert_eq!(r.var("TestSpecialEvent"), event);
    if event == 7 {
        r.wait(20);
        r.shot_as("xibalba-06-chac.png");
    }
    evs.extend(r.until_dialog(dialog));
    assert_eq!(
        (r.var("TestSpecialEvent"), r.var("SpecialEventDone")),
        (0, 0)
    );
    assert!(!r.in_bag(prey));
    evs
}

/// The caterpillar given to Chac (event 7, dialog 185, `flag84`).
fn chac_and_the_caterpillar(r: &mut Run) {
    feed_chac(r, "CHENILLE", 7, 185);
    assert_eq!(r.flag(84), 1);
}

/// Back to Yucatán in the middle of Xibalba: the bat's island (the
/// river's view 300 and its link to `bat.wam` 404: chapter 35), the bat
/// asked (only the king left, dialog 174), the way back by the zone of
/// view 403 (`BackToS27`: film 45, `MONDE` 2 on disc 2, the king's hall's
/// view 85, chapter 27); the priestess asked about Xibalba (chapter 27's
/// section 9: dialogs 125, 129, 132, 134; the bat is no subject of hers
/// since the canoe's win killed it); the bat of her view
/// clicked (section 8: `MONDE` 6, film 44, `bat.wam` view 403, chapter
/// 35).
fn the_priestess_meanwhile(r: &mut Run) {
    r.river_to(300, 404);
    assert_eq!(r.chapter(), 35);
    r.walk(403);
    assert_eq!(r.ask_each_named("CHAUVE_SOURIS"), named(&[("ROI", 174)]));
    let z = r.zone_named("BackToS27");
    let evs = r.use_on(None, z);
    assert!(films(&evs).contains(&45));
    assert!(evs.contains(&Event::Monde(2)));
    assert_eq!((r.e.place(), r.chapter()), (("salleroi.wam", 85), 27));
    assert_eq!(r.e.files().current_disc(), 2);
    r.walk(84);
    let names = r.e.script_state().unwrap().names.clone();
    let mut asked: Vec<(String, u32)> = r
        .ask_each("PRETRESSE")
        .into_iter()
        .map(|(v, d)| (subject_name(&names, v), d))
        .collect();
    asked.sort();
    assert_eq!(
        asked,
        named(&[("CHAC", 132), ("QUETZ", 129), ("ROI", 134), ("TEZ", 125)])
    );
    let bat = r.who("CHAUVE_SOURIS_SALLE_ROI");
    let mut evs = r.click_on(true, bat);
    evs.extend(r.settle());
    assert!(films(&evs).contains(&44));
    assert_eq!((r.e.place(), r.chapter()), (("bat.wam", 403), 35));
    assert_eq!(r.e.files().current_disc(), 4);
}

/// Xibalba's stars (`xibalba.wam` views 102, 103, 105, 108, 111, 113, 115)
/// and where they are taken.
const STARS: [(&str, u32); 7] = [
    ("ETOILE105", 105),
    ("ETOILE102", 102),
    ("ETOILE103", 103),
    ("ETOILE108", 108),
    ("ETOILE111", 111),
    ("ETOILE113", 113),
    ("ETOILE115", 115),
];

/// The seven stars taken in the clearing's views (by the river's view
/// 301 and its link to view 124: chapter 39), then the star map of view
/// 121 played with the mouse: a star laid on a wrong hole (sound 115) and
/// taken back with the empty hand (sound 114), then each star laid on a
/// right hole; the seventh wins (chapter 39's section: film 49, the
/// statue of Tezcatlipoca shown, its zones enabled).
fn the_star_map(r: &mut Run) {
    r.river_to(301, 124);
    assert_eq!(r.chapter(), 39);
    for (star, view) in STARS {
        r.go(view, &[]);
        r.take(star);
    }
    r.shot_as("xibalba-04-star.png");
    let evs = r.go(121, &[]);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_4e00
    }));
    assert_eq!(r.var("LIMITEANGLEWARPY"), 86);
    let hole = |r: &Run, h: i32| {
        let view = r.e.place().1;
        r.e.wam()
            .view(view)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 50 && z.value == h)
            .unwrap()
    };
    let z = hole(r, 5);
    let evs = r.use_on(Some("ETOILE102"), z);
    assert!(evs.contains(&Event::PuzzleRan {
        function: 0x0049_4e80
    }));
    assert!(sounds(&evs).contains(&"sound115.apc".to_string()));
    assert_eq!(r.var("etat_etoile5"), r.obj("ETOILE102"));
    let evs = r.use_on(None, z);
    assert!(sounds(&evs).contains(&"sound114.apc".to_string()));
    assert_eq!(r.var("etat_etoile5"), 0);
    let mut evs = Vec::new();
    for ((star, _), h) in STARS.iter().zip(STAR_HOLES) {
        let z = hole(r, h);
        evs = r.use_on(Some(star), z);
    }
    assert!(films(&evs).contains(&49));
    assert_eq!(
        (
            r.var("etoiles_fini"),
            r.var("Etoiles_JustFinished"),
            r.var("pz_etoile_started"),
            r.flag(85)
        ),
        (1, 0, 1, 1)
    );
    assert!(STARS.iter().all(|(s, _)| !r.in_bag(s)));
}

/// View 122: the statue of Tezcatlipoca asked through its zone (chapter
/// 39: dialogs 190-193); the obsidian laid on the small statue of view
/// 120 (sound 103); the statue asked again (dialogs 194-197); the
/// crocodile's foot laid on its own foot: it turns into a jaguar, then a
/// serpent (films 50, 51, dialogs 204, 198).
fn the_statues(r: &mut Run) {
    r.walk(122);
    r.shot_as("xibalba-07-statue.png");
    let mut got = Vec::new();
    for s in ["TEZ", "QUETZ", "HEROS_YU", "ROI"] {
        got.push(r.ask_by_zone("ClicTezStatue", "TEZ_STATUE", s));
    }
    assert_eq!(got, [193, 191, 192, 190]);
    r.walk(120);
    let z = r.zone_named("clic_PetiteSTATUE");
    let evs = r.use_on(Some("OBSIDIENNE1"), z);
    assert!(sounds(&evs).contains(&"sound103.apc".to_string()));
    assert_eq!((r.flag(87), r.var("obj120")), (1, 1));
    r.walk(122);
    let mut got = Vec::new();
    for s in ["TEZ", "QUETZ", "HEROS_YU", "ROI"] {
        got.push(r.ask_by_zone("ClicTezStatue", "TEZ_STATUE", s));
    }
    assert_eq!(got, [197, 195, 196, 194]);
    let z = r.zone_named("clicPied122");
    let evs = r.use_on(Some("PIED2_CROCO"), z);
    assert_eq!(films(&evs), [50, 51]);
    let d = dialogs(&evs);
    assert!(d.contains(&204) && d.contains(&198), "{d:?}");
    assert_eq!((r.flag(88), r.flag(119), r.var("obj122")), (1, 1, 2));
    assert!(r.shown(true, "SERPENT"));
    assert!(!r.shown(true, "TEZ_STATUE"));
    r.shot_as("xibalba-08-serpent.png");
}

/// The serpent asked: Quetzalcoatl (dialog 200), the king (201: the
/// feather), the king again (202).
fn the_xibalba_serpent(r: &mut Run) {
    assert_eq!(r.ask("SERPENT", "QUETZ"), 200);
    assert_eq!(r.ask("SERPENT", "ROI"), 201);
    assert!(r.in_bag("PLUME"));
    assert_eq!(r.flag(108), 1);
    assert_eq!(r.ask("SERPENT", "ROI"), 202);
}

/// The web's second round: the grasshopper (sound 105, `finaraign2`).
fn the_grasshopper(r: &mut Run) {
    let evs = web_round(r, 2);
    assert!(sounds(&evs).contains(&"sound105.apc".to_string()));
    assert!(r.in_bag("SAUTERELLE"));
    assert_eq!(r.var("finaraign2"), 1);
}

/// The grasshopper given to Chac (event 8, dialog 186, `flag86`).
fn chac_and_the_grasshopper(r: &mut Run) {
    feed_chac(r, "SAUTERELLE", 8, 186);
    assert_eq!(r.flag(86), 1);
}

/// The web's third round: the big spider (`flag92`, the spider goddess a
/// subject of the bat and the priestess), the sword, film 48, the
/// spider's skull, `finaraign3`; leaving by view 314 (chapter 38's
/// section 1 with `flag92`).
fn the_spiders_skull(r: &mut Run) {
    let evs = web_round(r, 3);
    assert!(films(&evs).contains(&48));
    assert!(r.in_bag("CRANE_ARAIG2"));
    assert!(!r.in_bag("EPEE"));
    assert_eq!((r.var("finaraign3"), r.flag(92)), (1, 1));
    assert_eq!(r.e.web.stage, 4);
}

/// The spider's skull given to the serpent (view 122, chapter 39: film
/// 52, dialog 199): the soul, `CHULEL`, in the bag; the statue back.
fn the_soul(r: &mut Run) {
    r.river_to(301, 124);
    r.go(122, &[]);
    let evs = r.give("CRANE_ARAIG2", "SERPENT");
    assert!(films(&evs).contains(&52));
    assert!(dialogs(&evs).contains(&199));
    assert!(r.in_bag("CHULEL"));
    assert!(!r.in_bag("CRANE_ARAIG2"));
    assert_eq!(r.flag(83), 1);
    assert!(r.shown(true, "TEZ_STATUE"));
    r.stow();
    r.shot_as("xibalba-09-soul.png");
}

/// Xibalba's stages, after [`the_way_into_xibalba`], as chapters 35-41
/// make them: each starts and ends in the panorama.
const XIBALBA: &[Stage] = &[
    ("the bat", the_bat),
    ("the canoe", the_canoe),
    ("the caterpillar", the_caterpillar),
    ("Chac and the caterpillar", chac_and_the_caterpillar),
    ("the priestess meanwhile", the_priestess_meanwhile),
    ("the star map", the_star_map),
    ("the statues", the_statues),
    ("the serpent", the_xibalba_serpent),
    ("the grasshopper", the_grasshopper),
    ("Chac and the grasshopper", chac_and_the_grasshopper),
    ("the spider's skull", the_spiders_skull),
    ("the soul", the_soul),
];

/// The way back with the soul: the bat's island (chapter 35's `/INIT`
/// with `flag83`: the soul its subject), the bat asked; `BackToS27`
/// (film 45, chapter 27, whose `/INIT` plays dialog 135); the priestess
/// asked (dialog 136 for everything); view 86 (chapter 27's departure,
/// `flag93`: chapter 28's end of Yucatán, film 55 with dialog 168 over
/// it, Yucatán's mandala, Tibet's view 7, chapter 75).
fn xibalba_way_back(r: &mut Run) -> Vec<Event> {
    r.river_to(300, 404);
    r.walk(403);
    let bat = r.ask_each_named("CHAUVE_SOURIS");
    assert!(bat.contains(&("CHULEL".to_string(), 184)), "{bat:?}");
    let z = r.zone_named("BackToS27");
    let evs = r.use_on(None, z);
    assert!(films(&evs).contains(&45));
    assert!(dialogs(&evs).contains(&135));
    assert_eq!((r.e.place(), r.chapter()), (("salleroi.wam", 85), 27));
    r.walk(84);
    let asked = r.ask_each("PRETRESSE");
    assert!(
        !asked.is_empty() && asked.iter().all(|a| a.1 == 136),
        "{asked:?}"
    );
    r.walk(85);
    let evs = r.walk_redirected(86);
    assert!(films(&evs).contains(&55));
    // Dialog 168 over the film (`cinema=55,168`, chapter 28's line 18).
    assert!(evs.contains(&Event::Set {
        chapter: 28,
        line: 18
    }));
    assert!(evs.contains(&Event::Monde(5)));
    evs
}

/// Back in Tibet with Yucatán's mandala: view 10, the mandala laid on its
/// zone (the hook: sound 400, `OrdreMandala6`; chapter 75's section:
/// `poseyua`, the mandala shown, out of the bag).
fn yucatans_mandala_laid(r: &mut Run) {
    r.go(10, &[]);
    let id = r.obj("MANDALA_YUCATAN");
    r.pick(id);
    let zi =
        r.e.wam()
            .view(10)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.name.as_deref() == Some("ZONE_YUA"))
            .unwrap();
    r.aim(zi);
    r.tap();
    let mut evs = r.events();
    evs.extend(r.settle());
    assert!(evs.contains(&Event::Sound("sound400.apc".into())));
    assert_eq!((r.var("poseyua"), r.var("NbMandalaPosed")), (1, 2));
    assert_eq!(r.var("OrdreMandala6"), 2);
    assert!(!r.in_bag("MANDALA_YUCATAN"));
    assert!(r.shown(false, "MANDALA_YU2"));
    // The game goes on in Tibet: the soul given, Xibalba's puzzles all
    // won.
    assert_eq!((r.e.place(), r.chapter()), (("tibet.wam", 10), 75));
    assert!(!r.in_bag("CHULEL"));
    assert_eq!(
        (
            r.var("finpirogue"),
            r.var("etoiles_fini"),
            r.var("finaraign1"),
            r.var("finaraign2"),
            r.var("finaraign3")
        ),
        (1, 1, 1, 1, 1)
    );
    r.shot_as("xibalba-11-mandala-in-tibet.png");
}

/// The Xibalba chapters a game goes through (40 and 41 hold only an
/// empty `/INIT`, named by no `chapter=` line).
const XIBALBA_CHAPTERS: [i32; 5] = [35, 36, 37, 38, 39];

/// The end of the game with the mouse, from Tibet as the three worlds
/// leave it (played above; here their ends are written as the scripts
/// leave them: the six mandalas laid, the shaman's four balls in the bag,
/// the three worlds' first space trips made): the ball of Tibet's view 4
/// (the hub's first trip from Tibet, the balls made crystals in view 4,
/// the way back), the crystal laid in the hut (Shamballa), the fireflies
/// (the testing aid writes their win: the puzzle is not ported) and the
/// vortex, Rhea asked about all she offers, the ring walked round with
/// its rolling moves, the star taken and laid, Atlantis: the octopus of
/// view 16 faced without the crystal until the hero dies (film 64, view 8
/// again), the sunken crystal taken in view 18, the octopus shot down in
/// view 16 (film 65), Tibet's dialog 372, the crystal laid again, the
/// hub's last trip to view 5, its last zone, the films and the credits to
/// their end, the last film and the quit. Pictures with
/// `ATLANTIS2_PNG_DIR` (`endgame-*.png`, `pz-octopus-*.png`).
#[test]
fn the_end_of_the_game_with_the_mouse() {
    if game().is_none() {
        return;
    }
    let dir = scratch("a2-flow-endgame");
    let mut r = Run::new(&dir);
    tibet_start(&mut r);
    for (k, (object, _, order)) in MANDALAS.iter().enumerate() {
        let pose = match k {
            0 => "poseirb",
            1 => "posechb",
            2 => "poseyub",
            3 => "poseira",
            4 => "posecha",
            _ => "poseyua",
        };
        for c in [
            format!("({pose}=1)"),
            format!("({order}={})", k + 1),
            format!("(sacados=NOT_{object})"),
        ] {
            r.e.run_command(c.as_bytes());
        }
    }
    for c in [
        "(NbMandalaPosed=6)",
        "(sacados=BOULE_SHAMAN_IR)",
        "(sacados=BOULE_SHAMAN_YU)",
        "(sacados=BOULE_SHAMAN_CH)",
        "(ChangeObjectWorld=BOULE_SHAMAN_IR,1)",
        "(ChangeObjectWorld=BOULE_SHAMAN_YU,2)",
        "(ChangeObjectWorld=BOULE_SHAMAN_CH,3)",
        "(sacados=BOULE_SHAMAN)",
        "(TravelIRDone=1)",
        "(TravelYUDone=1)",
        "(TravelCHDone=1)",
        "(SpaceDeep=3)",
    ] {
        r.e.run_command(c.as_bytes());
    }

    // Tibet's ball in view 4: the hub.
    r.go(4, &[]);
    let ball = r.obj("BOULE_SHAMAN");
    r.choose(ball);
    assert!(r.e.overlays.ball);
    let zone =
        r.e.wam()
            .view(4)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 1000)
            .unwrap();
    let evs = r.press_zone(zone);
    assert_eq!(films(&evs), [200]);
    assert!(r.e.place().0.eq_ignore_ascii_case("planetes.wam"));
    assert_eq!((r.e.place().1, r.chapter()), (1, 83));
    assert_eq!((r.var("SpaceDeep"), r.var("MONDE")), (4, 5));
    r.shot_as("endgame-01-hub.png");
    r.route(&[2, 3, 4]);
    assert!(r.in_bag("CRISTAL_SHAMAN") && !r.in_bag("BOULE_SHAMAN"));
    r.shot_as("endgame-02-hub-view-4.png");
    r.route(&[3, 2, 1]);
    let evs = r.walk_redirected(10);
    assert_eq!(films(&evs), [201]);
    assert_eq!((r.e.place(), r.chapter()), (("tibet.wam", 4), 75));

    // The crystal laid in the hut: Shamballa.
    r.go(10, &[]);
    let crystal = r.obj("CRISTAL_SHAMAN");
    r.pick(crystal);
    let zone = r.zone_named("ZonePoseCristal");
    r.aim(zone);
    r.tap();
    let mut evs = r.events();
    evs.extend(r.settle());
    assert_eq!(films(&evs), [61]);
    assert_eq!((r.e.place(), r.chapter()), (("voiles.wam", 1), 78));
    assert_eq!((r.var("MONDE"), r.e.files().current_disc()), (4, 4));
    r.shot_as("endgame-03-fireflies.png");
    // A firefly lit (its hook noted), the aid's win, the vortex.
    let firefly = r.zone_to(3);
    let evs = r.press_zone(firefly);
    assert!(evs.contains(&Event::PuzzleNotPorted {
        function: 0x0049_5210
    }));
    assert!(r.shown(false, "LUCIOLLE3"));
    r.e.win_fireflies();
    let vortex = r.obj("TOURBILLON");
    let mut evs = r.click_on(false, vortex);
    evs.extend(r.settle());
    assert_eq!(films(&evs), [62]);
    assert_eq!((r.e.place(), r.chapter()), (("rhea.wam", 1), 80));

    // Rhea.
    r.route(&[2, 3, 4, 5, 6]);
    r.shot_as("endgame-04-rhea.png");
    let asked = r.ask_each("RHEA");
    let mut heard: Vec<u32> = asked.iter().map(|a| a.1).collect();
    heard.sort_unstable();
    heard.dedup();
    assert_eq!(heard, [391, 392, 393, 394, 395, 396, 397], "{asked:?}");
    assert_eq!(r.flag(232), 1);
    r.route(&[5, 4, 3]);
    let evs = r.walk_link();
    assert_eq!(films(&evs), [67]);
    assert_eq!((r.e.place(), r.chapter()), (("annulair.wam", 1), 81));

    // The ring: round to the star, then to the stone.
    let mut rolled = false;
    for v in [16, 15, 14, 13, 12] {
        r.wait(15);
        let z = r.zone_to(v);
        assert!(r.try_aim(z, true));
        r.tap();
        let mut shot = false;
        for _ in 0..600 {
            r.step();
            rolled |= r.e.zoom.roll != 0.0;
            if v == 14 && !shot && r.e.zoom.roll < -3.0 {
                r.shot_as("endgame-05-ring-rolling.png");
                shot = true;
            }
            if r.e.loop_type == LoopType::Video {
                r.watch();
            }
            if r.e.place().1 == v && !r.e.turning() && r.e.loop_type == LoopType::Warp {
                break;
            }
        }
        assert_eq!(r.e.place().1, v);
        if v == 12 {
            // The arrival's frame is not drawn (the zoom's end skips one
            // draw): the next ones are.
            r.wait(5);
            r.shot_as("endgame-06-ring-star.png");
        }
    }
    assert!(rolled);
    r.take("ETOILE_LA");
    r.route(&[11, 10, 9, 8, 7, 6, 5]);
    let star = r.obj("ETOILE_LA");
    r.pick(star);
    let stone =
        r.e.wam()
            .view(5)
            .unwrap()
            .zones
            .iter()
            .position(|z| z.kind == 2)
            .unwrap();
    r.aim(stone);
    r.tap();
    let mut evs = r.events();
    for _ in 0..3000 {
        if r.e.place().0 == "atlantis.wam" && r.e.loop_type == LoopType::Warp {
            break;
        }
        if r.e.loop_type == LoopType::Video {
            evs.extend(r.watch());
        }
        evs.extend(r.events());
    }
    evs.extend(r.settle());
    assert_eq!(films(&evs), [63]);
    assert_eq!((r.e.place(), r.chapter()), (("atlantis.wam", 8), 82));
    r.shot_as("endgame-07-atlantis.png");

    // Atlantis: the octopus of view 16 faced without the crystal: its
    // tentacles strike (`EnergieJoueur` 10 to 0, the eleventh blow kills)
    // until the hero dies: film 64, view 8 again, chapter 82.
    let evs = r.go(16, &[]);
    assert!(evs.contains(&Event::PuzzleRan {
        function: OCTOPUS_ENTRY
    }));
    assert_eq!(r.e.puzzle_mode(), OCTOPUS_MODE);
    let evs = r.octopus_fight(false);
    let blows = evs
        .iter()
        .filter(|e| matches!(e, Event::OctopusCommand(c) if *c == "(NewSound=303)"))
        .count();
    assert_eq!(blows, 11);
    assert!(evs.contains(&Event::OctopusCommand("(chapter=201)")));
    assert_eq!(films(&evs), [64]);
    assert_eq!((r.e.place(), r.chapter()), (("atlantis.wam", 8), 82));
    assert_eq!(r.var("EnergieJoueur"), 10);
    // The sunken crystal of view 18 (gone from the bar in Atlantis: the
    // fight's tick puts its bag entry in world 123), then the octopus
    // shot down: 36 shots, six per tentacle, the octopus's death, film 65,
    // Tibet, dialog 372.
    r.go(18, &[]);
    r.take("CRISTAL_ENGLOUTI");
    r.shot_as("endgame-08-sunken-crystal.png");
    r.step();
    let sunken = r.obj("CRISTAL_ENGLOUTI");
    assert!(r
        .e
        .bag_entries()
        .iter()
        .any(|b| b.id == sunken && b.world == 123));
    r.go(16, &[]);
    let evs = r.octopus_fight(true);
    let shots = evs
        .iter()
        .filter(|e| matches!(e, Event::OctopusCommand(c) if *c == "(NewSound=301)"))
        .count();
    assert_eq!(shots, 36);
    let deaths = evs
        .iter()
        .filter(|e| matches!(e, Event::OctopusCommand(c) if *c == "(NewSound=304)"))
        .count();
    assert_eq!(deaths, 5);
    assert!(evs.contains(&Event::OctopusCommand("(chapter=200)")));
    assert!((1..=6).all(|k| r.var(&format!("BigTenta{k}Died")) == 1));
    assert_eq!(films(&evs), [65]);
    assert_eq!(dialogs(&evs), [372]);
    assert_eq!((r.e.place(), r.chapter()), (("tibet.wam", 7), 75));
    assert_eq!((r.flag(260), r.flag(261), r.var("SpaceDeep")), (1, 1, 5));

    // The crystal again: the hub's last trip.
    r.go(10, &[]);
    r.pick(crystal);
    let zone = r.zone_named("ZonePoseCristal");
    r.aim(zone);
    r.tap();
    let mut evs = r.events();
    evs.extend(r.settle());
    assert_eq!(films(&evs), [200]);
    assert_eq!((r.e.place().1, r.chapter()), (1, 83));
    r.route(&[2, 3, 4, 5]);
    r.shot_as("endgame-09-hub-view-5.png");
    // The last zone: the end.
    r.wait(15);
    let z = r.zone_to(77450);
    assert!(r.try_aim(z, true));
    r.tap();
    let mut films_seen = Vec::new();
    let mut credits_shot = false;
    for n in 0..30_000 {
        let evs = r.events();
        films_seen.extend(films(&evs));
        if r.e.loop_type == LoopType::Video && films_seen.last() != Some(&99) {
            r.wait(30);
            // A click on the film (one in the credits would quit at once).
            if r.e.loop_type == LoopType::Video {
                r.tap();
            }
        }
        if r.e.loop_type == LoopType::Menu && !credits_shot && n % 600 == 599 {
            r.shot_as("endgame-10-credits.png");
            credits_shot = true;
        }
        if r.e.quit() {
            break;
        }
    }
    assert!(r.e.quit());
    assert!(credits_shot);
    assert_eq!(films_seen, [68, 66, 99]);
    let journal = std::mem::take(&mut r.journal);
    assert!(!journal
        .iter()
        .any(|e| matches!(e, Event::WorldNotPorted(_))));
    // Only the two puzzles' hooks are noted.
    let mut noted: Vec<u32> = journal
        .iter()
        .filter_map(|e| match e {
            Event::PuzzleNotPorted { function } => Some(*function),
            _ => None,
        })
        .collect();
    noted.sort_unstable();
    noted.dedup();
    assert_eq!(noted, [0x0049_5210]);
}
