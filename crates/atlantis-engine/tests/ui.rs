//! The interface against the original `atlantis.exe` (2001 English build):
//! the menu walk `FUN_0041fb6c` and its fonts, the grid hit-test
//! `FUN_004211f0`, the options and sound menus' rules, the in-game menu
//! `FUN_0041c838`, the load picker `FUN_0041cf78`, the checkpoint names,
//! the credits `FUN_0041a72c`, the inventory icons `FUN_0042b328` and the
//! guests' hit-test `FUN_0042bdb8`; then the game played headless: the
//! start-up, the player screen, the in-game menu, the inventory bar, the
//! object picked up, the checkpoints and the credits.
//!
//! The expected values come from the original code run under emulation
//! (see the headers of the golden files); they hold numbers and digests
//! only. Ported from ScummVM's `test/engines/cryomni3d/atlantis/`
//! (`ui_menus.h`, `ui_saves.h`, `ui_credits.h`).
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory;
//! without it they are skipped. `ATLANTIS_GOG_FR` names a French GOG
//! installation whose `players.atl` and checkpoints the import test reads
//! (read only).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::saves::{epi_episode_name, Checkpoint, Players};
use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::checkpoint::PLAYERS_FILE;
use atlantis_engine::hooks::{DirectLoader, NoScripts};
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::credits::credits_events;
use atlantis_engine::ui::layout::{
    font_min_blits, font_min_load_list_width, menu_grid_hit, menu_volume_from_knob, menu_walk_hit,
    subtitle_toggle_click, walk_menu_text, FontMin, LoadPickerScroll, MenuWalk,
};
use atlantis_engine::ui::menus::{ingame_menu_outcome, MenuOutcome};
use atlantis_engine::ui::{guest_edge_original, object_icon, Guests};
use atlantis_engine::video_skip::VideoSkipState;
use atlantis_engine::{Dialogs, Engine, Event, MemStore, Options, SaveStore, Scripts};
use md5::{Digest, Md5};

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
}

fn golden_map(text: &str) -> HashMap<&str, &str> {
    lines(text).filter_map(|l| l.rsplit_once(' ')).collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

fn ints(s: &str) -> Vec<i32> {
    s.split_whitespace().map(|t| t.parse().unwrap()).collect()
}

const VECTORS: &str = include_str!("golden/ui_menu_vectors.txt");

/// The walk's records as the original's: x y w h, the glyphs drawn, the
/// first blit, the sprite.
fn walk_records(text: &[u8], menu: i32, a: i32, b: i32) -> Vec<String> {
    let mut walk = MenuWalk::default();
    walk_menu_text(text, menu, -1, a, b, &mut walk);
    walk.records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let ops: Vec<_> = walk.ops.iter().filter(|o| o.record == i as i32).collect();
            let label: String = ops
                .iter()
                .filter(|o| !o.sprite)
                .map(|o| char::from((o.index + 0x20) as u8))
                .collect();
            let spr = ops.iter().find(|o| o.sprite).map_or(-1, |o| o.index);
            let (x0, y0) = ops.first().map_or((0, 0), |o| (o.x, o.y));
            let label = if label.is_empty() { "-".into() } else { label };
            format!("{} {} {} {} {label} {x0} {y0} {spr}", r.x, r.y, r.w, r.h)
        })
        .collect()
}

#[test]
fn menu_walk_matches_original() {
    let mut cases = 0;
    let mut current: Option<(String, Vec<String>, Vec<String>)> = None;
    let check = |c: Option<(String, Vec<String>, Vec<String>)>| {
        if let Some((name, want, got)) = c {
            assert_eq!(got, want, "case {name}");
        }
    };
    for l in lines(VECTORS) {
        if let Some(rest) = l.strip_prefix("CASE ") {
            check(current.take());
            let t: Vec<&str> = rest.split(' ').collect();
            let text = t[5]
                .replace("\\s", " ")
                .replace("\\r", "\r")
                .replace("\\n", "\n");
            let got = walk_records(text.as_bytes(), ints(t[1])[0], ints(t[2])[0], ints(t[3])[0]);
            assert_eq!(got.len().to_string(), t[4], "case {} records", t[0]);
            current = Some((t[0].to_string(), Vec::new(), got));
            cases += 1;
        } else if let Some(rest) = l.strip_prefix("REC ") {
            current.as_mut().unwrap().1.push(rest.to_string());
        }
    }
    check(current);
    assert_eq!(cases, 9);
}

#[test]
fn menu_rules_match_original() {
    let mut grid = 0;
    let (mut players, mut names, mut loads) = (Vec::new(), Vec::new(), Vec::new());
    for l in lines(VECTORS) {
        let (tag, rest) = l.split_once(' ').unwrap();
        match tag {
            "GRID" => {
                let g = ints(rest);
                let got = menu_grid_hit(g[0], g[1], g[2], g[3], g[4], g[5], g[6] == 1, g[7], g[8]);
                assert_eq!(got.unwrap_or(-1), g[9], "grid {rest}");
                grid += 1;
            }
            "VOLUME" => {
                for (i, v) in ints(rest).into_iter().enumerate() {
                    assert_eq!(i32::from(menu_volume_from_knob(306 + i as i32)), v);
                }
            }
            "VOLDEF" => {
                let knobs = [460, 465, 470, 490];
                for (k, v) in knobs.iter().zip(ints(rest)) {
                    assert_eq!(i32::from(menu_volume_from_knob(*k)), v);
                }
            }
            "SUBCLICKS" => {
                let (mut on, mut counter) = (false, 0u8);
                for v in ints(rest) {
                    subtitle_toggle_click(&mut on, &mut counter);
                    assert_eq!(i32::from(on), v);
                }
            }
            "OMNICLICKS" => {
                // 0x004960ac = (0x004960ac == 0), from 0 (MODE 1).
                let mut mode2 = false;
                for v in ints(rest) {
                    mode2 = !mode2;
                    assert_eq!(i32::from(mode2), v);
                }
            }
            "PLAYERLIST" => players.push(ints(rest)),
            "NAMEFIELD" => names.push(ints(rest)),
            "LOADLIST" => loads.push(ints(rest)),
            _ => {}
        }
    }
    assert_eq!(grid, 672);
    let blits = |r: FontMin, texts: &[&str], x: i32, y0: i32| -> Vec<Vec<i32>> {
        texts
            .iter()
            .enumerate()
            .flat_map(|(j, t)| font_min_blits(r, t.as_bytes(), x, y0 + 22 * j as i32))
            .map(|b| vec![b.x, b.y, i32::from(b.idx) + 0x20])
            .collect()
    };
    let strip = |v: &[Vec<i32>]| -> Vec<Vec<i32>> { v.iter().map(|r| r[..3].to_vec()).collect() };
    assert_eq!(
        blits(
            FontMin::PlayerList,
            &["Bob", "Al Smith", "New Player"],
            185,
            270
        ),
        strip(&players)
    );
    assert_eq!(
        blits(FontMin::NameField, &["Ab c...."], 160, 190),
        strip(&names)
    );
    assert_eq!(
        blits(
            FontMin::LoadList,
            &["First ep", "Second one", "Third"],
            55,
            130
        ),
        strip(&loads)
    );
}

/// The poll of the warp loop (`FUN_0041a298` and the test @ 0x0041c5f7)
/// in the 64 states, and the in-game menu's outcomes.
#[test]
fn ingame_menu_rules_match_original() {
    let text = include_str!("golden/ui_ingame_menu.txt");
    let (mut o, mut m) = (0, 0);
    for l in lines(text) {
        let (head, out) = match l.split_once(": ") {
            Some(p) => p,
            None => continue,
        };
        let t: Vec<&str> = head.split(' ').collect();
        let want: HashMap<&str, &str> = out
            .split(' ')
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| (c[0], c[1]))
            .collect();
        if t[0] == "O" {
            let keys = t[1];
            let flag = |name: &str| {
                let i = t.iter().position(|&x| x == name).unwrap();
                t[i + 1] == "1"
            };
            let mut s = VideoSkipState {
                bow_mode: flag("bow"),
                bar_request: flag("bar"),
                object_in_hand: flag("objet"),
                talking_head: flag("head"),
                section_play: flag("section"),
                ..VideoSkipState::default()
            };
            s.poll(false, keys.contains("rbutton"), keys.contains("escape"));
            let opens = s.menu_request && !s.bar_request && !s.bow_mode;
            let got = [
                ("menu", opens),
                ("request", s.menu_request),
                ("keep", s.keep_running),
                ("click", s.click),
                ("bar", s.bar_request),
            ];
            for (k, v) in got {
                assert_eq!(want[k], if v { "1" } else { "0" }, "{l}: {k}");
            }
            o += 1;
        } else if t[0] == "M" {
            let record: usize = t[2].parse().unwrap();
            let li = t.iter().position(|&x| x == "loadexit").unwrap();
            let qi = t.iter().position(|&x| x == "quitexit").unwrap();
            let (le, qe): (u8, u8) = (t[li + 1].parse().unwrap(), t[qi + 1].parse().unwrap());
            let out = ingame_menu_outcome((record < 3).then_some(record), le, qe);
            assert_eq!(want["reload"] == "1", out == MenuOutcome::Reload, "{l}");
            assert_eq!(want["keep"] == "1", out == MenuOutcome::Resume, "{l}");
            m += 1;
        }
    }
    assert_eq!((o, m), (64, 24));
}

/// `FUN_0042bdb8` on the edge pixels: the original reads the bytes before
/// the list; the port returns no guest there (a fix of a bug that builds a
/// subject menu from memory outside the tables).
#[test]
fn guest_edge_matches_original_model() {
    let text = include_str!("golden/scripts_guest_edge.txt");
    let mut g = Guests::default();
    g.add(587, &[587]);
    let mut n = 0;
    for l in lines(text) {
        // clic 0x.. zone 0x.. point x,y -> 0x.. (n)
        let t: Vec<&str> = l.split(' ').collect();
        let hex = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
        let (clic, zone, want) = (hex(t[1]), hex(t[3]), hex(t[7]));
        let (x, y) = t[5].split_once(',').unwrap();
        assert_eq!(guest_edge_original(clic, zone), want, "{l}");
        assert_eq!(g.at(x.parse().unwrap(), y.parse().unwrap()), 0, "{l}");
        n += 1;
    }
    assert_eq!(n, 18);
}

fn menu_text(g: &Game, name: &str) -> Vec<u8> {
    g.big.read(&format!("SPRLIST\\{name}")).unwrap()
}

#[test]
fn shipped_menus_match_original() {
    let Some(g) = game("shipped_menus_match_original") else {
        return;
    };
    let golden = golden_map(include_str!("golden/ui_menus.txt"));
    let files = [
        "MAINMENU", "LOADMENU", "OPTMENU", "NEWMENU", "SELEMENU", "SONMENU", "SUREMENU", "CD1MENU",
        "CD2MENU", "CD3MENU", "CD4MENU", "WAIT", "ERROR1", "ERROR2", "ERROR3",
    ];
    let mut checked = 0;
    for (f, name) in files.iter().enumerate() {
        let text = menu_text(&g, &format!("{name}.TXT"));
        let states = if f == 2 { 4 } else { 1 };
        for st in 0..states {
            let (a, b) = ((st >> 1) & 1, st & 1);
            let suf = if f == 2 {
                format!(".t{a}{b}")
            } else {
                String::new()
            };
            let mut walk = MenuWalk::default();
            walk_menu_text(&text, f as i32, -1, a, b, &mut walk);
            let rects: Vec<String> = walk
                .records
                .iter()
                .map(|r| format!("{},{},{},{}", r.x, r.y, r.w, r.h))
                .collect();
            let blits = |sprites: bool| -> Vec<String> {
                walk.ops
                    .iter()
                    .filter(|o| o.sprite == sprites)
                    .map(|o| format!("{},{},{}", o.x, o.y, o.index))
                    .collect()
            };
            for (what, got) in [
                ("rects", rects.join(";")),
                ("glyphs", blits(false).join(";")),
                ("sprites", blits(true).join(";")),
            ] {
                let key = format!("{name}.TXT.{what}{suf}");
                assert_eq!(golden[key.as_str()], md5_hex(got.as_bytes()), "{key}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, golden.len());
}

/// The MAINMENU record under every pixel (`FUN_004210c0`).
#[test]
fn ingame_menu_hit_map_matches_original() {
    let Some(g) = game("ingame_menu_hit_map_matches_original") else {
        return;
    };
    let mut walk = MenuWalk::default();
    walk_menu_text(&menu_text(&g, "MAINMENU.TXT"), 0, -1, 0, 0, &mut walk);
    let mut rows = 0;
    for l in lines(include_str!("golden/ui_ingame_menu.txt")) {
        let Some(rest) = l.strip_prefix("G ") else {
            continue;
        };
        let (y, runs) = rest.split_once(' ').unwrap();
        let y: i32 = y.parse().unwrap();
        let mut got = Vec::new();
        let mut start = 0;
        for x in 0..=640 {
            let rec = |x: i32| menu_walk_hit(&walk.records, x, y).map_or(255, |r| r as i32);
            if x == 640 || rec(x) != rec(start) {
                got.push(format!("{start}-{}:{}", x - 1, rec(start)));
                start = x;
            }
        }
        assert_eq!(got.join(" "), runs, "row {y}");
        rows += 1;
    }
    assert_eq!(rows, 480);
    // The M lines' points hit their records.
    assert_eq!(menu_walk_hit(&walk.records, 208, 203), Some(0));
    assert_eq!(menu_walk_hit(&walk.records, 173, 243), Some(1));
    assert_eq!(menu_walk_hit(&walk.records, 210, 283), Some(2));
}

#[test]
fn inventory_icons_match_original() {
    let Some(g) = game("inventory_icons_match_original") else {
        return;
    };
    let golden = golden_map(include_str!("golden/graphics_inventory.txt"));
    for (id, icon) in &golden {
        assert_eq!(
            object_icon(&g.exe.inventory_object_ids, id.parse().unwrap()),
            Some(icon.parse::<usize>().unwrap()),
            "object {id}"
        );
    }
    assert_eq!(golden.len(), 48);
}

#[test]
fn episode_names_match_original() {
    let Some(g) = game("episode_names_match_original") else {
        return;
    };
    let golden = golden_map(include_str!("golden/ui_saves_epi.txt"));
    let epi = g.big.read("SPRLIST\\EPI.TXT").unwrap();
    assert_eq!(md5_hex(&epi), golden["epi.txt"]);
    for ch in 2..=86u32 {
        let key = format!("chapter {ch}");
        assert_eq!(
            md5_hex(&epi_episode_name(&epi, ch)),
            golden[key.as_str()],
            "{key}"
        );
    }
}

#[test]
fn credits_match_original() {
    let Some(g) = game("credits_match_original") else {
        return;
    };
    let text = include_str!("golden/ui_credits.txt");
    let golden = golden_map(text);
    let ev = credits_events(
        &g.big.read("SPRLIST\\CREDITS.TXT").unwrap(),
        &g.exe.credit_widths,
    );
    let all: Vec<String> = ev.iter().map(ToString::to_string).collect();
    assert_eq!(all.len().to_string(), golden["events"]);
    assert_eq!(md5_hex(all.join("\n").as_bytes()), golden["all_events_md5"]);
    let pages: Vec<&String> = all.iter().filter(|l| !l.starts_with("glyph")).collect();
    let want: Vec<&str> = lines(text)
        .filter(|l| l.starts_with("image") || l.starts_with("present") || l.starts_with("clear"))
        .collect();
    assert_eq!(pages, want);
}

#[test]
fn load_picker_matches_original() {
    let Some(g) = game("load_picker_matches_original") else {
        return;
    };
    let epi = g.big.read("SPRLIST\\EPI.TXT").unwrap();
    let (mut k, mut s, mut t) = (0, 0, 0);
    for l in lines(include_str!("golden/ui_load_picker.txt")) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "K" => {
                let sb = LoadPickerScroll::new(f[1].parse().unwrap());
                // 0: the record keeps the x of LOADMENU.TXT (600).
                let x = if sb.knob_x != 0 { sb.knob_x } else { 600 };
                assert_eq!(x.to_string(), f[2], "{l}");
                k += 1;
            }
            "S" => {
                let count: i32 = f[1].parse().unwrap();
                let mut sb = LoadPickerScroll::new(count);
                // The emulation's first frame: mouse at y 200, nothing latched.
                sb.frame(false, false, 200);
                let rows: Vec<String> = (0..480)
                    .map(|y| {
                        sb.frame(true, true, y);
                        format!("{y} {} {} {}", sb.knob_y, sb.offset, sb.knob_y)
                    })
                    .collect();
                assert_eq!(rows.len().to_string(), f[2]);
                assert_eq!(md5_hex(rows.join("\n").as_bytes()), f[3], "{l}");
                s += 1;
            }
            "T" => {
                let ch: u32 = f[1].parse().unwrap();
                let title = epi_episode_name(&epi, ch);
                let x = (640 - font_min_load_list_width(&title)) / 2;
                let got: String = font_min_blits(FontMin::NameField, &title, x, 0x50)
                    .iter()
                    .map(|b| format!(" {}@{},{}", b.idx, b.x, b.y))
                    .collect();
                assert_eq!(md5_hex(got.as_bytes()), f[2], "chapter {ch}");
                t += 1;
            }
            _ => {}
        }
    }
    assert_eq!((k, s, t), (129, 129, 85));
}

// ---- headless ---------------------------------------------------------------

struct Headless {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    sound: Vec<f32>,
    alive: bool,
}

impl Headless {
    fn new(sys: Rc<Sys>, task: impl std::future::Future<Output = ()> + 'static) -> Headless {
        Headless {
            runner: Runner::new(sys.clone(), task),
            sys,
            now: 0,
            sound: vec![0.0; 2 * (22050 * 16 / 1000)],
            alive: true,
        }
    }

    fn step(&mut self) -> bool {
        self.alive = self.runner.step(self.now);
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        self.now += 16;
        self.alive
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            if !self.step() {
                break;
            }
        }
    }

    fn button(&mut self, button: MouseButton, x: i32, y: i32) {
        self.sys.push_input(Input::MouseMove { x, y });
        self.sys.push_input(Input::MouseDown { button, x, y });
        self.step();
        self.sys.push_input(Input::MouseUp { button, x, y });
        self.steps(3);
    }

    fn click(&mut self, x: i32, y: i32) {
        self.button(MouseButton::Left, x, y);
    }

    fn key(&mut self, k: Key) {
        self.sys.push_input(Input::KeyDown(k));
        self.sys.push_input(Input::KeyUp(k));
        self.steps(2);
    }

    /// Steps until `f` holds, at most `n` frames.
    fn until(&mut self, n: usize, mut f: impl FnMut(&Headless) -> bool) -> bool {
        for _ in 0..n {
            if f(self) {
                return true;
            }
            if !self.step() {
                return f(self);
            }
        }
        false
    }
}

/// The centre of record `r` of a menu's walk.
fn record_centre(g: &Game, menu: &str, number: i32, r: usize) -> (i32, i32) {
    record_centre_text(&menu_text(g, menu), number, r)
}

fn record_centre_text(text: &[u8], number: i32, r: usize) -> (i32, i32) {
    let mut walk = MenuWalk::default();
    walk_menu_text(text, number, -1, 0, 0, &mut walk);
    let rec = walk.records[r];
    (
        i32::from(rec.x) + i32::from(rec.w) / 2,
        i32::from(rec.y) + i32::from(rec.h) / 2,
    )
}

/// The game started as the original: the logos and PREINTRO (each cut
/// short by a click), the player screen (a new player typed and
/// committed), a new game; then the in-game menu, QUIT GAME, the
/// confirmation and the credits, cut short by Escape.
#[test]
fn boot_player_screen_and_quit() {
    let Some(g) = game("boot_player_screen_and_quit") else {
        return;
    };
    let ok = record_centre(&g, "SUREMENU.TXT", 6, 3);
    let opt_text = menu_text(&g, "OPTMENU.TXT");
    let sys = Sys::new(Mixer::new());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        ..Options::default()
    };
    let mut h = Headless::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let videos = |t: &Rc<RefCell<Vec<Event>>>| {
        t.borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Video { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // LOGO15N, cut short.
    h.steps(30);
    h.click(320, 240);
    assert!(h.until(200, |_| videos(&trace).len() == 1));
    // PREINTRO, cut short once it plays.
    h.steps(100);
    h.click(320, 240);
    assert!(h.until(300, |_| videos(&trace).len() == 2));
    assert_eq!(videos(&trace), ["LOGO15N.HNM", "PREINTRO.HNM"]);
    // The player screen: the "new player" row (row 0), a name, Enter.
    h.steps(10);
    let screen = sys.screen.borrow().clone();
    assert!(screen.iter().any(|&p| p != 0), "the player screen is black");
    h.click(200, 259);
    for c in "Tester".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))));
    let players = Players::read(&store.read(PLAYERS_FILE).unwrap(), b"#New Player").unwrap();
    assert_eq!(players.name(0), b"Tester");
    assert!(players.is_new_row(1));
    // Up to the warp view, past the chapter's opening (a left click ends
    // each line): Escape until the menu is up.
    let menus = |t: &Rc<RefCell<Vec<Event>>>, m: &str| {
        t.borrow()
            .iter()
            .filter(|e| matches!(e, Event::Menu(n) if n == m))
            .count()
    };
    let mut menu_up = false;
    for _ in 0..300 {
        h.steps(30);
        if sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 0) {
                menu_up = true;
                break;
            }
        } else {
            h.click(320, 240);
        }
    }
    assert!(menu_up, "the in-game menu did not open");
    h.steps(5);
    // OPTIONS (record 1): the options menu, left with OK (record 6).
    h.click(173, 243);
    assert!(h.until(20, |_| menus(&trace, "OPTMENU.TXT") > 0));
    let opt_ok = record_centre_text(&opt_text, 2, 6);
    h.steps(3);
    h.click(opt_ok.0, opt_ok.1);
    h.steps(10);
    // Escape again, QUIT GAME (record 2), then OK (record 3).
    assert!(h.until(200, |hh| hh.sys.mouse_locked()));
    h.steps(5);
    h.key(Key::Escape);
    assert!(h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 1));
    h.steps(5);
    h.click(210, 283);
    assert!(h.until(20, |_| menus(&trace, "SUREMENU.TXT") > 0));
    h.steps(3);
    h.click(ok.0, ok.1);
    h.steps(120);
    h.key(Key::Escape);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
    let t = trace.borrow();
    eprintln!(
        "boot: {} frames, {} events, menus {:?}",
        h.now / 16,
        t.len(),
        t.iter()
            .filter_map(|e| match e {
                Event::Menu(n) => Some(n.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
    for e in t
        .iter()
        .filter(|e| matches!(e, Event::Section { .. } | Event::Video { .. }))
    {
        eprintln!("  {e:?}");
    }
}

type Task = std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>;

/// Runs `f` on an engine with its interface loaded and a new game started.
fn with_engine(
    g: Game,
    store: Rc<MemStore>,
    f: impl FnOnce(Engine, Scripts) -> Task + 'static,
    mut drive: impl FnMut(&mut Headless, usize),
) {
    let sys = Sys::new(Mixer::new());
    let done = Rc::new(RefCell::new(false));
    let (s2, d2) = (sys.clone(), done.clone());
    let task = async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.saves = store;
        e.load_interface().await;
        let mut scripts = Scripts::with_dialog(Box::new(Dialogs::new()));
        e.new_game(&mut scripts, false).await;
        f(e, scripts).await;
        *d2.borrow_mut() = true;
    };
    let mut h = Headless::new(sys, task);
    let mut n = 0;
    while !*done.borrow() {
        h.step();
        drive(&mut h, n);
        n += 1;
        assert!(n < 100_000, "the task does not end");
    }
}

/// A chapter change writes the checkpoint `pa2.gam` and `players.atl`
/// (`FUN_0042d6b8`); the checkpoint reloads (`FUN_0042de5c`), and a game
/// over reload goes back to it.
#[test]
fn checkpoints_written_and_loaded() {
    let Some(g) = game("checkpoints_written_and_loaded") else {
        return;
    };
    let store = Rc::new(MemStore::new());
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    with_engine(
        g,
        store.clone(),
        move |mut e, mut s| {
            Box::pin(async move {
                e.inventory.push(259);
                s.exec_set(&mut e, None, "chapter=2").await;
                o.borrow_mut().push(format!("chapter {}", e.chapter));
                // Another state, then the reload.
                e.inventory.push(300);
                e.vars.set("gardecel", 77);
                e.load_checkpoint(&mut s, true).await;
                o.borrow_mut().push(format!(
                    "loaded chapter {} place {:?} inventory {:?} lastvue {} chapter var {}",
                    e.chapter,
                    e.next_place_id,
                    e.inventory,
                    e.last_vue,
                    e.vars.get("chapter")
                ));
            })
        },
        |_, _| {},
    );
    let cp = Checkpoint::parse(&store.read("pa2.gam").expect("pa2.gam not written")).unwrap();
    assert_eq!((cp.chapter, cp.prev_chapter), (2, 1));
    assert_eq!(cp.wam_name(), "ATLAN1.WAM");
    assert_eq!(cp.vars[0], 1, "variable 0 holds the chapter left");
    assert_eq!(
        cp.inventory
            .iter()
            .map(|r| (r.sprite, r.id))
            .collect::<Vec<_>>(),
        [(0, 257), (2, 259)]
    );
    assert_eq!(cp.disc, 1);
    let players = Players::read(&store.read(PLAYERS_FILE).unwrap(), b"#New Player").unwrap();
    assert_eq!(players.chapters[0], 2);
    let out = out.borrow();
    assert_eq!(out[0], "chapter 2");
    assert_eq!(
        out[1],
        format!(
            "loaded chapter 2 place Some({}) inventory [257, 259] lastvue {} chapter var 2",
            cp.place_id, cp.last_vue
        )
    );
}

/// The inventory bar: a left click on a cell takes its object in hand, a
/// right click empties the hand; the object picked up stays in hand on a
/// right click (`FUN_0042cc98`).
#[test]
fn inventory_bar_and_pickup() {
    let Some(g) = game("inventory_bar_and_pickup") else {
        return;
    };
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    with_engine(
        g,
        Rc::new(MemStore::new()),
        move |mut e, _s| {
            Box::pin(async move {
                e.inventory = vec![257, 259, 300];
                let mut hooks = NoScripts;
                e.display_bar(&mut hooks).await;
                o.borrow_mut().push(e.vars.get("objetenmain"));
                e.display_bar(&mut hooks).await;
                o.borrow_mut().push(e.vars.get("objetenmain"));
                e.vars.set("objetenmain", 300);
                let keep = e.display_pickup(&mut hooks).await;
                o.borrow_mut().push(i32::from(keep));
                o.borrow_mut().push(e.vars.get("objetenmain"));
                let keep = e.display_pickup(&mut hooks).await;
                o.borrow_mut().push(i32::from(keep));
                o.borrow_mut().push(e.vars.get("objetenmain"));
            })
        },
        |h, n| match n {
            // The slides take 8 and 11 frames.
            30 => h.click(112, 440),
            90 => h.button(MouseButton::Right, 320, 440),
            150 => h.button(MouseButton::Right, 320, 300),
            210 => h.click(320, 300),
            _ => {}
        },
    );
    assert_eq!(*out.borrow(), [259, 0, 1, 300, 0, 0]);
}

/// The credits play their pages and end on Escape.
#[test]
fn credits_play_headless() {
    let Some(g) = game("credits_play_headless") else {
        return;
    };
    let frames = Rc::new(RefCell::new(0usize));
    let f2 = frames.clone();
    with_engine(
        g,
        Rc::new(MemStore::new()),
        move |mut e, _s| {
            Box::pin(async move {
                e.play_credits().await;
            })
        },
        move |h, n| {
            *f2.borrow_mut() = n;
            // The first page shows after 0x556 - 0x53c ticks; Escape at
            // the second.
            if n == 600 {
                let s = h.sys.screen.borrow().clone();
                assert!(s.iter().any(|&p| p != 0), "no credits page shown");
                h.key(Key::Escape);
            }
        },
    );
    assert!(*frames.borrow() < 700);
}

/// An original installation's `players.atl` and checkpoints are taken over
/// (the French GOG release, whose marker row is its own), and the player's
/// checkpoint loads.
#[test]
fn original_saves_imported() {
    let Some(dir) = std::env::var_os("ATLANTIS_GOG_FR") else {
        eprintln!("ATLANTIS_GOG_FR not set: original_saves_imported skipped");
        return;
    };
    let g = Game::open(Arc::new(StdDir::new(dir))).unwrap();
    let marker = g.exe.new_player_marker.clone();
    let store = Rc::new(MemStore::new());
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    with_engine(
        g,
        store.clone(),
        move |mut e, mut s| {
            Box::pin(async move {
                e.player = 0;
                e.load_checkpoint(&mut s, false).await;
                o.borrow_mut().push(e.chapter);
                o.borrow_mut().push(u32::from(e.next_place_id.unwrap_or(0)));
            })
        },
        |_, _| {},
    );
    let p = Players::read(&store.read(PLAYERS_FILE).unwrap(), &marker).unwrap();
    assert_eq!(p.player_count(), 1);
    assert!(p.is_new_row(1));
    let resume = p.chapters[0];
    let cp = Checkpoint::parse(&store.read(&p.checkpoint_name(0, resume)).unwrap()).unwrap();
    assert_eq!(*out.borrow(), [u32::from(cp.chapter), cp.place_id as u32]);
}

/// Deleting a player (`FUN_0041e8a8` run twice under emulation: names
/// Alice, Bob, Carol with the files pa2, pa3, pb2, pc2, pc5): the first
/// deletion as the original; the second deletes Bob's own files, where the
/// original deletes those of the row's letter ('a', none left) and leaves
/// Bob's.
#[test]
fn delete_player_twice() {
    let Some(g) = game("delete_player_twice") else {
        return;
    };
    let store = Rc::new(MemStore::new());
    for f in ["pa2.gam", "pa3.gam", "pb2.gam", "pc2.gam", "pc5.gam"] {
        store.write(f, &[0]);
    }
    let out = Rc::new(RefCell::new(Vec::new()));
    let (o, st) = (out.clone(), store.clone());
    with_engine(
        g,
        store.clone(),
        move |mut e, _s| {
            Box::pin(async move {
                for n in [&b"Alice"[..], b"Bob", b"Carol"] {
                    e.players.create(n).unwrap();
                }
                e.players.letters = *b"abcdef";
                e.delete_player(0);
                o.borrow_mut().push((e.players.letters, st.list()));
                e.delete_player(0);
                o.borrow_mut().push((e.players.letters, st.list()));
            })
        },
        |_, _| {},
    );
    let out = out.borrow();
    assert_eq!(&out[0].0, b"bcdeaf");
    assert_eq!(out[0].1, ["pb2.gam", "pc2.gam", "pc5.gam", "players.atl"]);
    assert_eq!(&out[1].0, b"cdeabf");
    assert_eq!(out[1].1, ["pc2.gam", "pc5.gam", "players.atl"]);
}
