//! GOG.com's edition: its installation folder opened as a game folder
//! (`Game.exe`, the 1.01 patch's executable with GOG's fixes; every disc in
//! its one `DATAS_A2` folder), the textures of the spider's web and of the
//! canoe (named with accents in their `.db` files, without them on GOG),
//! and a new game played to the shaman's first dialog, whose subtitles are
//! laid out as the 1.01 executable lays them out
//! (`tests/golden/subtitles_101_gog.txt`), then saved and loaded again;
//! and the ending's film 66 with the dialog line GOG's script adds over
//! it.
//!
//! Needs `ATLANTIS2_GOG`, the folder of GOG's installation (only read);
//! without it the tests are skipped. The textures are compared with the
//! CD's when `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` are set too. With
//! those two and `ATLANTIS2_PATCH101`, the 1.01 patch's `atlantis2.exe`,
//! a CD installation patched to 1.01 is put together in a scratch folder
//! (the patch's executable beside links to the installation's folders).

use std::path::PathBuf;

use atlantis2_data::db::Scene;
use atlantis2_data::dialog::parse_dial;
use atlantis2_data::exe::Release;
use atlantis2_data::files::holds_atlantis2;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::canoe::{HERO_FILES, STONES};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::web::{load_model, SLOTS};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::subtitle::{layout, TextOut};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const SUBTITLES_101_GOG: &str = include_str!("golden/subtitles_101_gog.txt");
const SUBTITLES_101_CD: &str = include_str!("golden/subtitles_101_cd.txt");

fn gog_dir() -> Option<PathBuf> {
    let d = std::env::var_os("ATLANTIS2_GOG").map(PathBuf::from);
    if d.is_none() {
        atlantis2_data::testing::missing("ATLANTIS2_GOG not set");
    }
    d
}

fn gog() -> Option<A2Files> {
    let (files, _) = A2Files::open_dir(&gog_dir()?, &[]).expect("GOG folder");
    Some(files)
}

fn cd() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn gogs_folder_is_a_game_folder() {
    let Some(dir) = gog_dir() else { return };
    assert!(holds_atlantis2(&dir));
    let (mut files, build) = A2Files::open_dir(&dir, &[]).unwrap();
    assert_eq!(
        (build.id, build.release, build.gog),
        ("atlantis2-gog-fr-2183168", Release::V101, true)
    );
    assert_eq!(files.exe_name(), Some("Game.exe"));
    assert_eq!(files.read_exe().map(|e| e.len()), Some(2_183_168));
    // Every disc, in the one folder: no disc is ever asked for.
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);
    for monde in 1..=4 {
        let disc = files.set_monde(monde);
        assert_eq!(disc, monde as u8);
        assert!(files.size_on_disc(disc, Folder::Spf, "sphex.spr").is_some());
    }
    // The data's own changes: dialog 398 over film 66, the corrected texts.
    let con = files.read(Folder::Scenar, "chapi083.con").unwrap();
    assert!(con
        .windows(b"/set(cinema=66,398)".len())
        .any(|w| w == b"/set(cinema=66,398)"));
    let font_txt =
        atlantis2_data::dialog::FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let dialog = font_txt.font("AtlFT_Dialog").unwrap();
    assert_eq!(dialog.height, 22);
}

/// The names each loader asks for (`datas_a2\3d\` paths) and whether GOG
/// has them.
#[test]
fn the_web_and_the_canoe_find_their_textures() {
    let Some(files) = gog() else { return };
    let reference = cd();
    let asked = std::cell::RefCell::new(Vec::<(String, Option<Vec<u8>>)>::new());
    let read = |p: &str| {
        let d = files.read(Folder::ThreeD, p);
        asked.borrow_mut().push((p.to_string(), d.clone()));
        d
    };
    for slot in SLOTS {
        assert!(load_model(Fpu::Chop, slot, read).is_some(), "{slot:?}");
    }
    for db in HERO_FILES
        .iter()
        .map(|f| f.to_string())
        .chain(STONES.iter().map(|(f, _)| format!("pz_pirog\\{f}")))
    {
        let scene = Scene::parse(&read(&db).unwrap()).unwrap();
        for m in &scene.materials {
            if let Some(t) = m.texture() {
                read(&format!("pz_pirog\\{t}"));
            }
        }
    }
    let asked = asked.into_inner();
    let missing: Vec<&str> = asked
        .iter()
        .filter(|(_, d)| d.is_none())
        .map(|(p, _)| p.as_str())
        .collect();
    assert!(missing.is_empty(), "not found: {missing:?}");
    // Among them the names with accents (`TR-D-tête araignée.tga`,
    // `TR-D-peau héros.tga`): GOG has them without.
    let accented: Vec<&str> = asked
        .iter()
        .map(|(p, _)| p.as_str())
        .filter(|p| !p.is_ascii())
        .collect();
    assert!(
        accented.iter().any(|p| p.starts_with("pz_araig")),
        "{accented:?}"
    );
    assert!(
        accented.iter().any(|p| p.starts_with("pz_pirog")),
        "{accented:?}"
    );
    // The same files as the CD's.
    if let Some(cd) = reference {
        for (p, d) in &asked {
            assert_eq!(cd.read(Folder::ThreeD, p).as_ref(), d.as_ref(), "{p}");
        }
    }
}

/// A call as the golden writes it (after the id and the index).
fn call_row(text: &[u8], c: &TextOut) -> String {
    let off = if c.bytes.is_empty() {
        0
    } else {
        text.windows(c.bytes.len())
            .position(|w| w == c.bytes.as_slice())
            .map_or(-1, |p| p as i64)
    };
    format!(
        "{} {} {:06x} {} {} {off}",
        c.x,
        c.y,
        c.colour,
        c.bytes.len(),
        hex(&c.bytes)
    )
}

/// The golden's calls of text `key`.
fn golden_calls(key: &str) -> Vec<String> {
    let head = format!("call {key} ");
    SUBTITLES_101_GOG
        .lines()
        .filter(|l| l.starts_with(&head))
        .map(|l| l.split_whitespace().skip(3).collect::<Vec<_>>().join(" "))
        .collect()
}

/// The engine at 59.94 passes a second, the clock running with them.
struct Run {
    e: Engine,
    pass: u64,
}

impl Run {
    fn step(&mut self) -> Vec<Event> {
        self.e.set_clock(self.pass as f64 * PASS);
        self.e.pass(None);
        self.pass += 1;
        std::mem::take(&mut self.e.events)
    }

    fn wait(&mut self, n: u64) -> Vec<Event> {
        (0..n).flat_map(|_| self.step()).collect()
    }

    fn until(&mut self, max: u64, until: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..max {
            let evs = self.step();
            let hit = evs.iter().any(&until);
            all.extend(evs);
            if hit {
                return all;
            }
        }
        panic!(
            "not reached in {max} passes: {:?} {:?}",
            self.e.loop_type,
            &all[all.len().saturating_sub(12)..]
        );
    }

    fn click(&mut self) {
        self.e.click(self.pass as f64 * PASS);
    }

    /// Zone `zone` of the view looked at and clicked, the transition
    /// video clicked away, up to the arrival in `to`.
    fn walk(&mut self, zone: usize, to: u32) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        self.click();
        let mut all = Vec::new();
        for _ in 0..600 {
            let evs = self.step();
            if evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            }) {
                all.extend(self.wait(15));
                self.click();
            }
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if arrived {
                assert_eq!(self.e.place().1, to);
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }
}

#[test]
fn a_new_game_the_shamans_dialog_in_1_01_and_a_save() {
    let Some(files) = gog() else { return };
    let data = scratch("a2-gog-new-game");
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.rand = Rand::new(1000);
    e.trace = true;
    e.set_data_dir(Some(data.clone()));
    e.start_new_game().unwrap();
    let mut r = Run { e, pass: 0 };
    // The opening film, a click, the music: chapter 0 ends in Tibet.
    // (After the music copy's please-wait page: GOG's `music\` holds empty copies.)
    r.until(600, |e| matches!(e, Event::VideoFrame(3)));
    r.click();
    r.until(600, |e| *e == Event::VideoEnd);
    r.until(600, |e| matches!(e, Event::Music { .. }));
    assert_eq!(r.e.loop_type, LoopType::Warp);
    r.wait(15);
    r.click();
    r.until(2, |e| matches!(e, Event::ClickScan(_)));
    assert_eq!(r.e.script_state().unwrap().chapter, 75);
    for (zone, to) in [(0, 3), (1, 4), (0, 5), (1, 6)] {
        r.walk(zone, to);
    }
    // View 7: dialog 360, its subtitles as 1.01 lays them out.
    let evs = r.walk(0, 7);
    assert!(evs.contains(&Event::DialogStart {
        number: 360,
        started: true
    }));
    let font = r.e.subtitle_font().unwrap();
    assert_eq!(font.line_step(), 22);
    let mut lines = Vec::new();
    let mut rows = 0;
    let mut wrap_differs = 0;
    for _ in 0..60 * 60 {
        let step = r.step();
        for ev in &step {
            if let Event::DialogLine(id) = ev {
                let line = r.e.dialog().unwrap().current_line().unwrap();
                let font = r.e.subtitle_font().unwrap();
                let calls = font.layout(&line.text, 640, 480);
                let got: Vec<String> = calls.iter().map(|c| call_row(&line.text, c)).collect();
                assert_eq!(got, golden_calls(&format!("TI.{id}")), "{id}");
                rows += calls.len() / 2;
                // 1.00's rule (an accented letter glued to the next
                // character) on the same font.
                let glued = layout(
                    &line.text,
                    |b| font.extent(b),
                    font.line_step(),
                    &[0x20],
                    true,
                    640,
                    480,
                );
                wrap_differs += usize::from(glued != calls);
                lines.push(id.clone());
            }
        }
        if step.contains(&Event::DialogEnd) {
            break;
        }
    }
    assert!(lines.len() >= 2, "{lines:?}");
    eprintln!(
        "dialog 360: {} lines, {rows} rows, {wrap_differs} laid out otherwise by 1.00's rule",
        lines.len()
    );
    assert_eq!(r.e.loop_type, LoopType::Warp);

    // A Tibet line that 1.01 breaks elsewhere than 1.00 would (a space
    // after an accented letter): its dialog played, the lines before it
    // clicked away, the line drawn.
    let font = r.e.subtitle_font().unwrap();
    let dial = r.e.files().read(Folder::Loc, "DialTI.txt").unwrap();
    let target = parse_dial(&dial)
        .into_iter()
        .find(|d| {
            let calls = font.layout(&d.text, 640, 480);
            let glued = layout(
                &d.text,
                |b| font.extent(b),
                font.line_step(),
                &[0x20],
                true,
                640,
                480,
            );
            calls.len() >= 4 && glued != calls
        })
        .expect("a line 1.01 breaks otherwise");
    let (n, _) = target.numbers().unwrap();
    eprintln!("dialog {n}: line {} laid out by 1.01's rule", target.id);
    r.e.start_dialog(n).unwrap();
    let mut screen = vec![0u16; 640 * 480];
    let mut shown = None;
    let mut other = None;
    for _ in 0..60 * 60 {
        r.e.set_clock(r.pass as f64 * PASS);
        r.e.pass(Some(&mut screen));
        r.pass += 1;
        for ev in std::mem::take(&mut r.e.events) {
            if let Event::DialogLine(id) = ev {
                if id == target.id {
                    shown = Some(r.pass);
                } else {
                    other = Some(r.pass);
                }
            }
        }
        if other.is_some_and(|p| r.pass == p + 30) {
            r.click();
        }
        if shown.is_some_and(|p| r.pass == p + 30) {
            break;
        }
    }
    assert!(shown.is_some());
    let line = r.e.dialog().unwrap().current_line().unwrap();
    assert_eq!(line.id, target.id);
    let calls = r.e.subtitle_font().unwrap().layout(&line.text, 640, 480);
    let got: Vec<String> = calls.iter().map(|c| call_row(&line.text, c)).collect();
    assert_eq!(got, golden_calls(&format!("TI.{}", target.id)));
    // The rows drawn on the screen: white pixels in each row's cell.
    for c in calls.iter().filter(|c| c.colour == 0xffffff) {
        let white = (c.y..c.y + 21)
            .flat_map(|y| (c.x..630).map(move |x| (x, y)))
            .filter(|&(x, y)| screen[y as usize * 640 + x as usize] == 0xffff)
            .count();
        assert!(white > 20, "row at {}: {white} white pixels", c.y);
    }
    if let Some(dir) = std::env::var_os("ATLANTIS2_PNG_DIR") {
        let png = atlantis2_engine::png::encode_rgb565(&screen, 640, 480);
        std::fs::write(PathBuf::from(dir).join("gog-subtitle.png"), png).unwrap();
    }
    r.e.escape();
    r.wait(30);
    assert_eq!(r.e.loop_type, LoopType::Warp);

    // A save in view 7, a walk to view 8, the save loaded again.
    assert!(r.e.save_slot(3));
    r.walk(1, 8);
    assert_eq!(r.e.place(), ("tibet.wam", 8));
    r.e.load_slot(3).unwrap();
    r.wait(5);
    assert_eq!(r.e.place(), ("tibet.wam", 7));
    assert!(r.e.saves().iter().any(|(s, _)| *s == 3));

    // Chapter 83's ending as GOG's script has it: film 66 with dialog 398
    // over it (`/set(cinema=66,398)`; the CD's has `cinema=66` alone).
    r.e.run_command(b"(monde=4)");
    r.e.run_command(b"(cinema=66,398)");
    let evs = r.wait(30);
    assert!(evs
        .iter()
        .any(|e| matches!(e, Event::VideoStart { name, .. } if name == "cine066.hnm")));
    let d = r.e.film_dialog().expect("dialog 398 over the film");
    assert_eq!(d.lines()[0].id, "398_01");
    assert!(!d.lines()[0].text.is_empty());
    let _ = std::fs::remove_dir_all(&data);
}

/// The font line of a 1.01 golden for `height`: ppem, ascent, cell.
fn golden_font(golden: &str, height: i32) -> (u16, i32, i32) {
    let l = golden
        .lines()
        .find(|l| {
            l.starts_with("font ") && l.split_whitespace().nth(3) == Some(&height.to_string())
        })
        .unwrap();
    let f: Vec<&str> = l.split_whitespace().filter(|&f| f != "|").collect();
    (
        f[4].parse().unwrap(),
        f[5].parse().unwrap(),
        f[6].parse().unwrap(),
    )
}

/// The French CD's installation with the 1.01 patch's `atlantis2.exe`:
/// the patch's build, its listener table, and 1.01's fonts on the CD's
/// `Font.txt`.
#[cfg(unix)]
#[test]
fn a_cd_installation_patched_to_1_01() {
    let (Some(dir), Some(discs), Some(patch)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
        std::env::var_os("ATLANTIS2_PATCH101"),
    ) else {
        atlantis2_data::testing::missing(
            "ATLANTIS2_DATA / ATLANTIS2_DISCS / ATLANTIS2_PATCH101 not set",
        );
        return;
    };
    let install = scratch("a2-patched-101");
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().eq_ignore_ascii_case("atlantis2.exe") {
            std::os::unix::fs::symlink(entry.path(), install.join(&name)).unwrap();
        }
    }
    std::fs::copy(&patch, install.join("atlantis2.exe")).unwrap();
    let (files, build) = A2Files::open_dir(&install, &[PathBuf::from(&discs)]).unwrap();
    assert_eq!(
        (build.id, build.release, build.gog),
        ("atlantis2-patch101-2183168", Release::V101, false)
    );
    let (cd, _) = A2Files::open_dir(&PathBuf::from(&dir), &[PathBuf::from(&discs)]).unwrap();
    use atlantis2_engine::place_sound::Listeners;
    assert_eq!(
        Listeners::from_exe(&files.read_exe().unwrap(), &build.place_sounds),
        Listeners::from_exe(&cd.read_exe().unwrap(), &cd.build().place_sounds)
    );
    let font_txt =
        atlantis2_data::dialog::FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let dialog =
        atlantis2_engine::subtitle::SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(dialog.metrics(), golden_font(SUBTITLES_101_CD, 12));
    assert_eq!(dialog.line_step(), 13);
    let popup =
        atlantis2_engine::subtitle::SubtitleFont::load(&files, &font_txt, "AtlFT_POPUP").unwrap();
    assert_eq!(popup.metrics(), golden_font(SUBTITLES_101_CD, 10));
    // The engine boots on it.
    let e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    assert_eq!(e.files().build().id, "atlantis2-patch101-2183168");
    drop(e);
    let _ = std::fs::remove_dir_all(&install);
}

/// GOG's CD page (`atlantis2-gog-fr-2183168` @ 0x00488e57..0x00488ec2,
/// state 9 patched): the start is set again every frame and the page
/// closes when the low byte of the ticks since the last frame is 1, without
/// looking for the disc, the current disc left as it was. GOG's folder
/// holds every disc, so the page is opened here on a scratch copy of it
/// (links) without disc 4's marker: the folder is then disc 3 alone.
#[cfg(unix)]
#[test]
fn gogs_cd_page_closes_at_the_first_tick() {
    use std::os::unix::fs::symlink;
    use std::path::Path;
    let Some(dir) = gog_dir() else { return };
    let link_all = |from: &Path, to: &Path, skip: &str| {
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().eq_ignore_ascii_case(skip) {
                symlink(entry.path(), to.join(&name)).unwrap();
            }
        }
    };
    let install = scratch("a2-gog-without-disc-4");
    link_all(&dir, &install, "DATAS_A2");
    let datas = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("datas_a2")
        })
        .unwrap()
        .path();
    let spf = std::fs::read_dir(&datas)
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("spf"))
        .unwrap()
        .path();
    let to_datas = install.join(datas.file_name().unwrap());
    std::fs::create_dir(&to_datas).unwrap();
    link_all(&datas, &to_datas, "SPF");
    let to_spf = to_datas.join(spf.file_name().unwrap());
    std::fs::create_dir(&to_spf).unwrap();
    link_all(&spf, &to_spf, "MANTE.SPR");
    let (files, build) = A2Files::open_dir(&install, &[]).unwrap();
    assert!(build.gog);
    assert_eq!(files.disc_numbers(), vec![3]);

    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    let mut clock = 1000.0;
    let mut pass = |e: &mut Engine| {
        clock += PASS;
        e.set_clock(clock);
        e.pass(None);
    };
    e.run_command(b"(monde=3)");
    pass(&mut e);
    assert_ne!(e.loop_type, LoopType::Menu);
    // Disc 4's world: the page opens.
    e.run_command(b"(monde=4)");
    pass(&mut e);
    assert_eq!(e.loop_type, LoopType::Menu);
    assert_eq!(
        e.menu().and_then(|m| m.kind()),
        Some(atlantis2_engine::menu::PageKind::Cd(4))
    );
    // It closes at the pass of the next tick, the disc still missing.
    let opened = e.ticks;
    while e.loop_type == LoopType::Menu {
        pass(&mut e);
        assert!(
            e.ticks.wrapping_sub(opened) <= 1,
            "{}",
            e.ticks.wrapping_sub(opened)
        );
    }
    assert_eq!(e.ticks.wrapping_sub(opened), 1);
    assert_eq!(e.files().disc_numbers(), vec![3]);
    assert_eq!(e.files().current_disc(), 4);
    drop(e);
    let _ = std::fs::remove_dir_all(&install);
}
