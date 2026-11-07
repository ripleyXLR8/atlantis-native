//! The Hoggar played from the new game to the next world's arrival,
//! headless, through the game's own API (clicks, the inventory, the
//! subject menu), against the original's interpreter playing the same walk
//! (`tests/golden/hoggar_walk.txt`, `tests/golden/hoggar_script.txt`):
//! every `SetVariables` line in order, the quick save's text and the
//! walk's checks. Cinematics, travel videos and dialogs are ended as soon
//! as they start, as the original's run ended them; the spinning-top
//! puzzle's throws are replaced by their result (the five stars lit).
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis2_data::defnames;
use atlantis3_data::A3Files;
use atlantis3_engine::subjects::Layout;
use atlantis3_engine::{Game, Mode};

const WALK: &str = include_str!("golden/hoggar_walk.txt");
const GOLDEN: &str = include_str!("golden/hoggar_script.txt");

struct Run {
    g: Game,
    t: f64,
}

impl Run {
    fn frame(&mut self) {
        self.t += 1.0 / 30.0;
        self.g.step(self.t);
        if self.g.video().is_some() {
            self.g.finish_video();
        }
        if self.g.dialog_running() {
            self.g.finish_dialog();
        }
    }

    /// Frames until nothing moves: the warp, no turn, no pending move, no
    /// video, no dialog, two frames in a row.
    fn settle(&mut self) {
        let mut calm = 0;
        for _ in 0..2000 {
            self.frame();
            let still = matches!(self.g.mode(), Mode::Warp | Mode::Subjects)
                && !self.g.turning()
                && !self.g.move_pending()
                && self.g.video().is_none()
                && !self.g.dialog_running();
            calm = if still { calm + 1 } else { 0 };
            if calm >= 3 {
                return;
            }
        }
        panic!("the game did not settle");
    }

    fn id(&self, section: usize, name: &str) -> i32 {
        self.g
            .names()
            .id(section, name.as_bytes())
            .unwrap_or_else(|_| panic!("{name}")) as i32
    }

    fn var(&self, name: &str) -> i32 {
        self.g.script().unwrap().var(name).unwrap()
    }
}

#[test]
fn the_hoggar_to_the_land_of_the_dead_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).expect("the game starts");
    g.script_mut().unwrap().set_log = Some(Vec::new());
    let mut r = Run { g, t: 0.0 };
    let mut expects = 0;
    for line in WALK.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        match w[0] {
            "START" => {
                assert!(r.g.new_game());
                r.settle();
            }
            "GO" => {
                let v: i32 = w[1].parse().unwrap();
                assert!(
                    r.g.aim_at_zone(|z| z.kind == 1 && z.value == v),
                    "{line}: no zone to {v} under the centre"
                );
                r.g.click();
                r.settle();
            }
            "PERSO" | "OBJET" => {
                let (section, kind) = if w[0] == "PERSO" {
                    (defnames::PERSONNAGES, 1)
                } else {
                    (defnames::OBJETS, 2)
                };
                let id = r.id(section, w[1]);
                assert!(r.g.click_sprite(id, kind), "{line}: not shown");
                r.frame();
            }
            "SUBJECT" => {
                assert!(r.g.subjects_open(), "{line}: no subject menu");
                let value = {
                    let names = r.g.names();
                    let i = names.id(defnames::SUJETS, w[1].as_bytes()).unwrap();
                    names.param_of(defnames::SUJETS, i)
                };
                let s = r.g.script().unwrap();
                let perso = s.var("CLICPERSO").unwrap();
                let lay = Layout::for_width(r.g.width());
                let cell = lay
                    .cells(&s.subjects.list(perso))
                    .into_iter()
                    .find(|c| c.value == value)
                    .unwrap_or_else(|| panic!("{line}: not offered"));
                r.g.set_pointer(cell.x, cell.y);
                r.g.click();
                r.settle();
            }
            "ZONE" => {
                let name = w[1];
                assert!(
                    r.g.aim_at_zone(|z| z
                        .name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))),
                    "{line}"
                );
                r.g.click();
                r.settle();
            }
            "HAND" => {
                let id = r.id(defnames::OBJETS, w[1]);
                r.g.right_click();
                r.frame();
                assert!(r.g.inventory_open());
                let j =
                    r.g.inventory_items()
                        .iter()
                        .position(|e| e.id == id)
                        .unwrap_or_else(|| {
                            panic!(
                                "{line}: not in the bag {:?} {:?}",
                                r.g.script().unwrap().bag,
                                r.g.script().unwrap().a3.world
                            )
                        });
                let (n, margin) = atlantis3_engine::inventory::slots(r.g.width());
                assert!((j as i32) < n);
                r.g.set_pointer(margin + 80 * j as i32 + 40, r.g.height() as i32 - 40);
                r.g.click();
                r.frame();
                assert!(!r.g.inventory_open());
                r.settle();
            }
            "FRAME" => {
                let n: usize = w.get(1).map_or(1, |n| n.parse().unwrap());
                for _ in 0..n {
                    r.frame();
                }
            }
            "SKIP" => {
                for a in &w[3..] {
                    let (k, v) = a.split_once('=').unwrap();
                    if let Some(f) = k.strip_prefix("FLAG") {
                        let f: usize = f.parse().unwrap();
                        r.g.script_mut().unwrap().flags[f] = v.parse().unwrap();
                    } else if k == "SHOWOBJET" {
                        r.g.show_entity(2, v);
                    }
                }
                r.settle();
            }
            "EXPECT" => {
                let (k, v) = w[1].split_once('=').unwrap();
                let s = r.g.script().unwrap();
                let got = match k {
                    "WAM" => r.g.place().0.to_string(),
                    "HAND" => {
                        let h = s.var("OBJETENMAIN").unwrap();
                        if h == 0 {
                            "-".into()
                        } else {
                            String::from_utf8_lossy(
                                r.g.names().name(defnames::OBJETS, h as usize).unwrap(),
                            )
                            .into_owned()
                        }
                    }
                    "MONDE" => s.a3.world.to_string(),
                    _ => match k.strip_prefix("FLAG") {
                        Some(f) => s.flags[f.parse::<usize>().unwrap()].to_string(),
                        None => r.var(k).to_string(),
                    },
                };
                assert_eq!(got, v, "{line}");
                expects += 1;
            }
            other => panic!("unknown action {other}"),
        }
    }
    assert_eq!(expects, 15);
    let sets: Vec<String> =
        r.g.script_mut()
            .unwrap()
            .set_log
            .take()
            .unwrap()
            .into_iter()
            .map(|l| format!("SET{l}"))
            .collect();
    let want: Vec<&str> = GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix("LOG interp: "))
        .collect();
    // The original's run starts with the boot's chapter 0 /INIT, which
    // `Game::new` ran before the log was on.
    let skip = want.iter().position(|l| *l == "SET(chapter=600)").unwrap();
    for (i, (a, b)) in sets.iter().zip(&want[skip..]).enumerate() {
        assert_eq!(a, b, "SetVariables line {i}");
    }
    assert_eq!(sets.len(), want.len() - skip);
    // The quick save, as the original wrote it (but its date and camera).
    let save = r.g.saved("quick1_1.sav").expect("quick save 1");
    let text = String::from_utf8_lossy(save);
    let got: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with("DateAndTime") && !l.starts_with("Quaternion"))
        .collect();
    let mut first: Vec<&str> = Vec::new();
    for l in GOLDEN.lines() {
        if let Some(s) = l.strip_prefix("SAVEFILE ") {
            if s.starts_with("CurrentCD") && !first.is_empty() {
                break;
            }
            if !s.starts_with("Quaternion") {
                first.push(s);
            }
        }
    }
    assert_eq!(got, first);
    // The variables and flags the original's run changed, at the end.
    let mut vars = std::collections::BTreeMap::new();
    let mut flags = std::collections::BTreeMap::new();
    for l in GOLDEN.lines() {
        let (map, rest) = if let Some(r) = l.strip_prefix("VARS ") {
            (&mut vars, r)
        } else if let Some(r) = l.strip_prefix("FLAGS ") {
            (&mut flags, r)
        } else {
            continue;
        };
        for kv in rest.split_once(": ").map_or("", |x| x.1).split_whitespace() {
            let (k, v) = kv.split_once('=').unwrap();
            map.insert(k.to_string(), v.parse::<i32>().unwrap());
        }
    }
    let s = r.g.script().unwrap();
    // `ANGLEWARPX` / `ANGLEWARPY` come from the harness's stubbed angle
    // read-back after `AngleWarpXNow` (the camera code is not run there).
    for (k, v) in vars.iter().filter(|(k, _)| !k.starts_with("ANGLEWARP")) {
        assert_eq!(s.var(k), Some(*v), "variable {k}");
    }
    for (k, v) in &flags {
        assert_eq!(s.flags[k.parse::<usize>().unwrap()], *v, "flag {k}");
    }
    assert_eq!((vars.len(), flags.len()), (14, 24));
}
