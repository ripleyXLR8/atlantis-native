//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): the story
//! played through the game's own API, chapter by chapter, from a new game
//! to the credits, with the shipped scripts (`chapi*.con`).
//!
//! The walk (`tests/golden/story2004/walk.txt`) is a list of player
//! actions: `GO <view>` (the move zone to that view, under the centre,
//! clicked), `OBJ <#OBJETS#>` and `PERSO <#PERSONNAGES#>` (the place
//! sprite turned to and clicked), `SUBJECT <#SUJETS#>` (its cell of the
//! open subject menu), `ZONE <name>` (a click zone of the view), `HAND
//! <#OBJETS#>` (the object taken from the bag in the inventory, then the
//! inventory closed), `WAIT <s>`. Films are skipped (Space), dialogs
//! clicked through (a light one runs to its voice's end), and each
//! mini-game is ended by the variable its own win sets (the games
//! themselves are tested against the original in `puzzles_2004*.rs`).
//! `EXPECT CHAPTER=n` / `EXPECT VUE=n` / `EXPECT <VAR>=n` check the state.
//!
//! The walk was found by [`explore`] (ignored; `STORY2004_OUT=<file>`),
//! a search over the same actions that, from the current state, looks
//! breadth-first for the shortest action sequence that runs a script line
//! never run before, and goes on from there.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::defnames;
use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::puzzles2004::PuzzleId;

fn cd() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    Some(dir)
}

fn files(dir: &std::path::Path) -> A3Files {
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(dir),
    )));
    A3Files::evolution_2004(src)
}

/// The game and its clock.
struct H {
    g: Game,
    t: f64,
    /// Events of the last action (films, dialogs, puzzles, chapters).
    events: Vec<String>,
}

const DT: f64 = 0.04;

impl H {
    fn new(dir: &std::path::Path) -> H {
        let g = Game::new(files(dir)).expect("the 2004 program's game");
        H {
            g,
            t: 0.0,
            events: Vec::new(),
        }
    }

    fn trace_on(&mut self) {
        if let Some(s) = self.g.script_mut() {
            if s.lines.is_none() {
                s.lines = Some(Vec::new());
            }
        }
    }

    fn take_lines(&mut self) -> Vec<(i32, usize)> {
        self.g
            .script_mut()
            .and_then(|s| s.lines.as_mut().map(std::mem::take))
            .unwrap_or_default()
    }

    fn frame(&mut self) {
        self.t += DT;
        self.g.step(self.t);
    }

    /// The click's frame 1 ms after the aim: no 25 Hz look tick between
    /// (the ship's node would turn the camera back, as in
    /// `objects_2004.rs`).
    fn click_now(&mut self) {
        self.g.click();
        self.t += 0.001;
        self.g.step(self.t);
    }

    /// Turns to the zone and clicks it when it is still under the centre
    /// after a pass (the pitching ship turns the camera with it).
    fn aim_click(&mut self, pred: impl Fn(&atlantis2_data::wam::Zone) -> bool) -> bool {
        // A pass first (its 25 Hz look tick), then the aim and the click
        // 1 ms later: no tick between.
        self.frame();
        if !self.g.aim_at_zone_scan(&pred) {
            return false;
        }
        let i = self.g.centre_zone();
        self.click_now();
        if std::env::var_os("STORY2004_DEBUG").is_some() {
            eprintln!(
                "  aim_click: centre {i:?}, after the click {:?} place {:?} angles {:?}",
                self.g.centre_zone(),
                self.g.place(),
                self.g.angles_deg()
            );
        }
        true
    }

    fn var(&self, name: &str) -> i32 {
        self.g
            .script()
            .and_then(|s| s.var(name))
            .unwrap_or_else(|| panic!("{name}"))
    }

    fn chapter(&self) -> i32 {
        self.g.script().map_or(-1, |s| s.chapter)
    }

    fn id(&self, section: usize, name: &str) -> Option<i32> {
        self.g
            .names()
            .id(section, name.as_bytes())
            .ok()
            .map(|i| i as i32)
    }

    fn name(&self, section: usize, id: i32) -> String {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.g.names().name(section, i))
            .map_or_else(
                || format!("#{id}"),
                |n| String::from_utf8_lossy(n).into_owned(),
            )
    }

    /// Whether the warp is still busy with a move.
    fn busy(&self) -> bool {
        self.g.mode() == Mode::Warp
            && (self.g.turning() || self.g.move_pending() || self.var("ARRIVEEVUE") != 0)
    }

    /// The media and mini-games end, the moves end: back in the warp (or
    /// the subject menu, or the credits). Err when it does not settle.
    fn settle(&mut self) -> Result<(), String> {
        let mut quiet = 0;
        let mut last_chapter = self.chapter();
        for _ in 0..20_000 {
            if self.chapter() != last_chapter {
                last_chapter = self.chapter();
                self.events.push(format!("chapter {last_chapter}"));
            }
            match self.g.mode() {
                Mode::Video => {
                    if let Some((name, n, _)) = self.g.film_2004() {
                        if n <= 1 {
                            self.events.push(format!("film {name}"));
                        }
                        self.g.skip();
                    }
                    self.frame();
                    quiet = 0;
                    continue;
                }
                Mode::Puzzle2004 => {
                    self.puzzle()?;
                    quiet = 0;
                    continue;
                }
                Mode::Carte2004 => {
                    self.events.push("gameboy".into());
                    self.g.escape();
                    self.frame();
                    self.g.right_click();
                    self.frame();
                    if self.g.mode() == Mode::Carte2004 {
                        return Err("the Gameboy does not close".into());
                    }
                    continue;
                }
                Mode::Subjects => return Ok(()),
                Mode::Inventory => {
                    self.g.close_inventory_now();
                    self.frame();
                    continue;
                }
                Mode::Menu | Mode::Screen | Mode::Idle | Mode::Intro => {
                    return Ok(());
                }
                Mode::Dialog | Mode::Warp => {}
            }
            if let Some((n, l, _, light)) = self.g.dialog_2004() {
                if l == 1
                    && !self
                        .events
                        .last()
                        .is_some_and(|e| *e == format!("dialog {n}"))
                {
                    self.events.push(format!("dialog {n}"));
                }
                if !light {
                    self.g.click();
                }
                self.frame();
                quiet = 0;
                continue;
            }
            if self.busy() {
                self.frame();
                quiet = 0;
                continue;
            }
            quiet += 1;
            if quiet > 3 {
                return Ok(());
            }
            self.frame();
        }
        Err(format!(
            "does not settle: mode {:?} place {:?}",
            self.g.mode(),
            self.g.place()
        ))
    }

    /// A mini-game: its win, as its own code sets it (the game's frame
    /// then runs the chapter's `/puz` scan).
    fn puzzle(&mut self) -> Result<(), String> {
        let Some(p) = self.g.puzzle_2004() else {
            self.frame();
            return Ok(());
        };
        let tag = format!("puzzle {}", p.name());
        if !self.events.contains(&tag) {
            self.events.push(tag);
        }
        let win: &[&str] = match p {
            PuzzleId::Caisses => &["CAISSES_GAGNE"],
            PuzzleId::Bulle => &["BULLE_GAGNE"],
            PuzzleId::Defender => &["DEFENDER_GAGNE"],
            PuzzleId::Pong => &["PONG_GAGNE"],
            PuzzleId::Hanoi => &["HANOI_GAGNE"],
            PuzzleId::Tron => &["TRON_GAGNE"],
            PuzzleId::Frogger => &["FROG_GAGNE"],
            PuzzleId::Catapulte => &["CATA_CLIC_EXIT"],
        };
        for v in win {
            if self.var(v) == 0 {
                self.g.set_var(v, 1);
            }
        }
        self.frame();
        Ok(())
    }

    /// The save's text without its date: the state's key.
    fn snapshot(&mut self) -> Option<Vec<u8>> {
        let name = self.g.save_game()?;
        let d = self.g.saved(&name)?.to_vec();
        Some(d)
    }

    fn restore(dir: &std::path::Path, snap: &[u8]) -> H {
        let mut h = H::new(dir);
        assert!(
            h.g.load_save_text("game1_99.sav", snap.to_vec()),
            "the load"
        );
        h.trace_on();
        let _ = h.settle();
        h.take_lines();
        h
    }

    /// The walk's actions played from a new game.
    fn restore_main(dir: &std::path::Path, walk: &[String]) -> H {
        let mut h = H::new(dir);
        assert!(h.g.new_game());
        h.trace_on();
        h.settle().unwrap();
        for l in walk {
            if l.starts_with('#') || l == "START" || l.trim().is_empty() {
                continue;
            }
            h.act(l).unwrap_or_else(|e| panic!("{l}: {e}"));
        }
        h.take_lines();
        h
    }

    /// One walk action; Err when the game refuses it.
    fn act(&mut self, a: &str) -> Result<(), String> {
        self.events.clear();
        let w: Vec<&str> = a.split_whitespace().collect();
        match w[0] {
            "GO" => {
                let v: i32 = w[1].parse().unwrap();
                if !self.aim_click(|z| z.kind == 1 && z.value == v) {
                    return Err(format!("{a}: no zone to {v} under the centre"));
                }
            }
            "OBJ" | "PERSO" => {
                let (section, kind) = if w[0] == "PERSO" {
                    (defnames::PERSONNAGES, 1)
                } else {
                    (defnames::OBJETS, 2)
                };
                let id = self
                    .id(section, w[1])
                    .ok_or_else(|| format!("{a}: no such name"))?;
                let mut found = self.g.click_sprite(id, kind);
                for _ in 0..6 {
                    if found {
                        break;
                    }
                    for _ in 0..4 {
                        self.frame();
                    }
                    found = self.g.click_sprite(id, kind);
                }
                if !found {
                    return Err(format!("{a}: not shown"));
                }
                self.frame();
            }
            "SUBJECT" => {
                if !self.g.subjects_open() {
                    return Err(format!("{a}: no subject menu"));
                }
                let i = self
                    .g
                    .names()
                    .id(defnames::SUJETS, w[1].as_bytes())
                    .map_err(|_| format!("{a}: no such subject"))?;
                let value = self.g.names().param_of(defnames::SUJETS, i);
                let s = self.g.script().unwrap();
                let perso = s.var("CLICPERSO").unwrap();
                let lay = atlantis3_engine::subjects::Layout::for_width_2004(self.g.width()).0;
                let cell = lay
                    .cells(&s.subjects.list(perso))
                    .into_iter()
                    .find(|c| c.value == value)
                    .ok_or_else(|| format!("{a}: not offered"))?;
                self.g.set_pointer(cell.x, cell.y);
                self.g.click();
                self.frame();
            }
            "ZONE" => {
                let name = w[1];
                if !self.aim_click(|z| {
                    z.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                }) {
                    let (w, v) = self.g.place();
                    let zones: Vec<String> = v
                        .and_then(|v| self.g.wam().view(v))
                        .map(|v| {
                            v.zones
                                .iter()
                                .map(|z| format!("{:?}/{}/{}", z.name, z.kind, z.value))
                                .collect()
                        })
                        .unwrap_or_default();
                    return Err(format!(
                        "{a}: not under the centre ({w} {v:?} mode {:?}: {zones:?})",
                        self.g.mode()
                    ));
                }
            }
            "HAND" if w[1] == "-" => {
                // Opening the inventory empties the hand (FUN_00428750's
                // caller, `objects_2004::chapter_1_by_clicks`); closed above
                // the bar.
                self.g.right_click();
                self.frame();
                self.g.set_pointer(self.g.width() as i32 / 2, 300);
                for _ in 0..5 {
                    self.frame();
                }
                self.g.right_click();
                self.frame();
                if self.g.inventory_open() {
                    self.g.close_inventory_now();
                    self.frame();
                }
                if self.g.object_in_hand() != 0 {
                    return Err(format!("{a}: the hand is not empty"));
                }
            }
            "HAND" => {
                let id = self
                    .id(defnames::OBJETS, w[1])
                    .ok_or_else(|| format!("{a}: no such object"))?;
                self.g.right_click();
                self.frame();
                if !self.g.inventory_open() {
                    return Err(format!("{a}: no inventory"));
                }
                for _ in 0..10 {
                    self.frame();
                }
                let items = self.g.inventory_items();
                let j = items
                    .iter()
                    .position(|e| e.id == id)
                    .ok_or_else(|| format!("{a}: not in the bag"))?;
                let lay = atlantis3_engine::inventory2004::Layout::for_width(self.g.width());
                if j as i32 >= lay.slots {
                    return Err(format!("{a}: scrolling not handled"));
                }
                let (x, y) = lay.item_centre(j as i32, self.g.height());
                self.g.set_pointer(x, y);
                self.g.click();
                self.frame();
                self.g.set_pointer(self.g.width() as i32 / 2, 300);
                self.frame();
                self.g.right_click();
                self.frame();
                if self.g.inventory_open() {
                    return Err(format!("{a}: the inventory stays open"));
                }
                if self.g.object_in_hand() != id {
                    return Err(format!("{a}: not in the hand"));
                }
            }
            "COMBINE" => {
                // The inventory: the first object taken (the inventory
                // stays open), then the second clicked: a `#COMPOSITIONS#`
                // line of the two replaces them (FUN_0042ab80).
                let ids: Vec<i32> = w[1..3]
                    .iter()
                    .map(|n| {
                        self.id(defnames::OBJETS, n)
                            .ok_or_else(|| format!("{a}: {n}?"))
                    })
                    .collect::<Result<_, _>>()?;
                self.g.right_click();
                self.frame();
                if !self.g.inventory_open() {
                    return Err(format!("{a}: no inventory"));
                }
                for _ in 0..10 {
                    self.frame();
                }
                let lay = atlantis3_engine::inventory2004::Layout::for_width(self.g.width());
                for id in ids {
                    let items = self.g.inventory_items();
                    let j = items
                        .iter()
                        .position(|e| e.id == id)
                        .ok_or_else(|| format!("{a}: not in the bag"))?;
                    if j as i32 >= lay.slots {
                        return Err(format!("{a}: scrolling not handled"));
                    }
                    let (x, y) = lay.item_centre(j as i32, self.g.height());
                    self.g.set_pointer(x, y);
                    self.frame();
                    self.g.click();
                    self.frame();
                }
                self.g.set_pointer(self.g.width() as i32 / 2, 300);
                self.frame();
                self.g.right_click();
                self.frame();
                if self.g.inventory_open() {
                    self.g.close_inventory_now();
                    self.frame();
                }
            }
            "WAIT" => {
                let s: f64 = w[1].parse().unwrap();
                let end = self.t + s;
                while self.t < end {
                    self.frame();
                    if !matches!(self.g.mode(), Mode::Warp) {
                        break;
                    }
                }
            }
            _ => panic!("{a}: unknown action"),
        }
        self.settle()
    }
}

/// The names a chapter's conditions click, by kind.
#[derive(Default, Debug)]
struct Targets {
    objets: BTreeSet<String>,
    persos: BTreeSet<String>,
    sujets: BTreeSet<String>,
    zones: BTreeSet<String>,
    hand: BTreeSet<String>,
    timers: bool,
}

fn chapter_targets(dir: &std::path::Path, chapter: i32) -> Targets {
    let path = dir.join(format!("cd0/datas/Scenar/chapi{chapter:03}.con"));
    let text = std::fs::read(&path).unwrap_or_default();
    let text = String::from_utf8_lossy(&text).to_ascii_uppercase();
    let mut t = Targets::default();
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with("/TIM") {
            t.timers = true;
        }
        if !(l.starts_with("/CON") || l.starts_with("/TIM") || l.starts_with("/PUZ")) {
            continue;
        }
        for term in l.split('(').skip(1) {
            let term = term.trim_end_matches([')', '&', '|', ' ']);
            let Some((k, v)) = term.split_once('=') else {
                continue;
            };
            let v = v.trim_end_matches(')').trim().to_string();
            match k.trim() {
                "CLICOBJET" => {
                    t.objets.insert(v);
                }
                "CLICPERSO" => {
                    t.persos.insert(v);
                }
                "SUJET" => {
                    t.sujets.insert(v);
                }
                "CLICZONE" => {
                    t.zones.insert(v);
                }
                "OBJETENMAIN" => {
                    t.hand.insert(v);
                }
                _ => {}
            }
        }
    }
    t
}

/// The `#COMPOSITIONS#` lines of `defnames.con`: the two objects.
fn compositions(dir: &std::path::Path) -> Vec<(String, String)> {
    let text = std::fs::read(dir.join("cd0/datas/Scenar/defnames.con")).unwrap_or_default();
    let text = String::from_utf8_lossy(&text).into_owned();
    let mut out = Vec::new();
    let mut inside = false;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('#') {
            inside = l.starts_with("#COMPOSITIONS#");
            continue;
        }
        if !inside || l.is_empty() || l.starts_with("//") {
            continue;
        }
        let f: Vec<&str> = l.split(',').map(str::trim).collect();
        if f.len() >= 3 {
            out.push((f[0].to_string(), f[1].to_string()));
        }
    }
    out
}

/// The actions to try in the current state.
fn candidates(h: &mut H, dir: &std::path::Path, cache: &mut HashMap<i32, Targets>) -> Vec<String> {
    let chapter = h.chapter();
    let t = cache
        .entry(chapter)
        .or_insert_with(|| chapter_targets(dir, chapter));
    let mut out = Vec::new();
    if h.g.mode() == Mode::Subjects {
        let s = h.g.script().unwrap();
        let perso = s.var("CLICPERSO").unwrap();
        for sub in s.subjects.list(perso) {
            if sub.value <= 0 {
                continue;
            }
            if let Some(name) = (0..h.g.names().len(defnames::SUJETS))
                .find(|&i| h.g.names().param_of(defnames::SUJETS, i) == sub.value)
                .and_then(|i| h.g.names().name(defnames::SUJETS, i))
            {
                out.push(format!("SUBJECT {}", String::from_utf8_lossy(name)));
            }
        }
        return out;
    }
    if h.g.mode() != Mode::Warp {
        return out;
    }
    let (_, view) = h.g.place();
    let Some(v) = view.and_then(|v| h.g.wam().view(v)) else {
        return out;
    };
    let zones = v.zones.clone();
    // Shown sprites of this view.
    let mut sprites = BTreeSet::new();
    for s in &h.g.slots().slots {
        if s.shown && s.click != 0 && (s.kind == 1 || s.kind == 2) {
            let section = if s.kind == 1 {
                defnames::PERSONNAGES
            } else {
                defnames::OBJETS
            };
            let n = h.name(section, s.id);
            sprites.insert((s.kind, n));
        }
    }
    let hand = h.g.object_in_hand();
    let targets_objets: Vec<String> = sprites
        .iter()
        .filter(|(k, n)| *k == 2 && t.objets.contains(n))
        .map(|(_, n)| format!("OBJ {n}"))
        .collect();
    let targets_persos: Vec<String> = sprites
        .iter()
        .filter(|(k, n)| *k == 1 && t.persos.contains(n))
        .map(|(_, n)| format!("PERSO {n}"))
        .collect();
    let targets_zones: Vec<String> = zones
        .iter()
        .filter(|z| z.kind != 1 && z.kind < 88888)
        .filter_map(|z| z.name.clone())
        .filter(|n| t.zones.contains(&n.to_ascii_uppercase()))
        .map(|n| format!("ZONE {n}"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    out.extend(targets_objets.iter().cloned());
    out.extend(targets_persos.iter().cloned());
    out.extend(targets_zones.iter().cloned());
    for z in &zones {
        if z.kind == 1 {
            out.push(format!("GO {}", z.value));
        }
    }
    // Objects of the bag the chapter tests in the hand.
    let bag: Vec<i32> = h.g.inventory_items().iter().map(|e| e.id).collect();
    for id in bag {
        let n = h.name(defnames::OBJETS, id);
        if id != hand && t.hand.contains(&n) {
            out.push(format!("HAND {n}"));
        }
    }
    if hand != 0 {
        out.push("HAND -".into());
    }
    // The `#COMPOSITIONS#` pairs both in the bag.
    for (a, b) in compositions(dir) {
        let (Some(x), Some(y)) = (h.id(defnames::OBJETS, &a), h.id(defnames::OBJETS, &b)) else {
            continue;
        };
        let ids: Vec<i32> = h.g.inventory_items().iter().map(|e| e.id).collect();
        if ids.contains(&x) && ids.contains(&y) {
            out.push(format!("COMBINE {a} {b}"));
        }
    }
    if t.timers {
        out.push("WAIT 5".into());
        out.push("WAIT 30".into());
    }
    let mut seen = HashSet::new();
    out.retain(|a| seen.insert(a.clone()));
    out
}

/// A condition term `(KEY op VALUE)`.
#[derive(Debug, Clone)]
struct Term {
    key: String,
    op: u8,
    value: String,
}

/// A section of a chapter: its condition lines (each a list of `&`
/// groups of `|` terms) and the numbers of its `/set` lines.
#[derive(Debug, Clone, Default)]
struct Section {
    conds: Vec<Vec<Vec<Term>>>,
    sets: Vec<usize>,
}

fn parse_terms(l: &str) -> Vec<Vec<Term>> {
    // The text after the keyword: `(a=b)&(c=d)|(e=f)`.
    let body = &l[l.find('(').unwrap_or(l.len())..];
    let mut groups: Vec<Vec<Term>> = vec![Vec::new()];
    let mut rest = body;
    while let Some(o) = rest.find('(') {
        let Some(c) = rest[o..].find(')') else { break };
        let inner = &rest[o + 1..o + c];
        let after = &rest[o + c + 1..];
        if let Some(i) = inner.find(['=', '<', '>', '!']) {
            groups.last_mut().unwrap().push(Term {
                key: inner[..i].trim().to_string(),
                op: inner.as_bytes()[i],
                value: inner[i + 1..].trim().to_string(),
            });
        }
        let next = after.trim_start();
        if !next.starts_with('|') {
            groups.push(Vec::new());
        }
        rest = after;
    }
    groups.retain(|g| !g.is_empty());
    groups
}

fn chapter_sections(dir: &std::path::Path, chapter: i32) -> Vec<Section> {
    let path = dir.join(format!("cd0/datas/Scenar/chapi{chapter:03}.con"));
    let text = std::fs::read(&path).unwrap_or_default();
    let text = String::from_utf8_lossy(&text).to_ascii_uppercase();
    let mut out: Vec<Section> = Vec::new();
    let mut after_init = false;
    let mut cur: Option<Section> = None;
    let mut last_cond = false;
    for (n, l) in text.lines().enumerate() {
        let l = l.trim();
        if l.starts_with("/FININIT") {
            after_init = true;
            continue;
        }
        if !after_init {
            continue;
        }
        let is_cond = l.starts_with("/CON") || l.starts_with("/TIM") || l.starts_with("/PUZ");
        if is_cond {
            if !last_cond {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
                cur = Some(Section::default());
            }
            cur.as_mut().unwrap().conds.push(parse_terms(l));
            last_cond = true;
        } else {
            last_cond = false;
            if l.starts_with("/SET") {
                if let Some(c) = cur.as_mut() {
                    c.sets.push(n + 1);
                }
            } else if l.starts_with('/') {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
            }
        }
    }
    if let Some(c) = cur.take() {
        out.push(c);
    }
    out.retain(|s| !s.sets.is_empty());
    out
}

/// The keys the player sets by acting (or the clock): not state.
fn is_trigger(key: &str) -> bool {
    matches!(
        key,
        "CLICOBJET"
            | "CLICPERSO"
            | "CLICZONE"
            | "SUJET"
            | "DEPARTVUE"
            | "ARRIVEEVUE"
            | "OBJETENMAIN"
            | "VUE"
            | "LASTVUE"
            | "TIME1"
            | "TIME2"
            | "RANDOM"
            | "RND"
            | "NUMFRAMEPERSO"
            | "NUMFRAMEVIDEO"
    )
}

/// The unsatisfied state groups of a section in the current state.
fn unsatisfied(h: &H, sec: &Section) -> usize {
    let s = h.g.script().unwrap();
    let bag: Vec<i32> = s.bag.iter().map(|e| e.id).collect();
    let holds = |t: &Term| -> bool {
        if is_trigger(&t.key) {
            return true;
        }
        let v = if let Some(n) = t
            .key
            .strip_prefix("FLAG")
            .and_then(|n| n.parse::<usize>().ok())
        {
            s.flags.get(n).copied().unwrap_or(0)
        } else if t.key == "SACADOS" {
            let (not, name) = match t.value.strip_prefix("NOT_") {
                Some(n) => (true, n),
                None => (false, t.value.as_str()),
            };
            let Some(id) = h.id(defnames::OBJETS, name) else {
                return true;
            };
            return bag.contains(&id) != not;
        } else {
            match s.var(&t.key) {
                Some(v) => v,
                None => return true,
            }
        };
        let Ok(want) = t.value.parse::<i32>() else {
            return true;
        };
        match t.op {
            b'=' => v == want,
            b'<' => v < want,
            b'>' => v > want,
            _ => v != want,
        }
    };
    sec.conds
        .iter()
        .flatten()
        .filter(|g| !g.iter().any(holds))
        .count()
}

/// The search heuristic: the fewest unsatisfied groups of a section of
/// the chapter not run yet.
fn heuristic(h: &H, secs: &HashMap<i32, Vec<Section>>, seen: &HashSet<(i32, usize)>) -> usize {
    let c = h.chapter();
    let Some(list) = secs.get(&c) else {
        return 20;
    };
    list.iter()
        .filter(|sec| !sec.sets.iter().any(|&n| seen.contains(&(c, n))))
        .map(|sec| unsatisfied(h, sec))
        .min()
        .unwrap_or(20)
}

/// The state's key: the script's state without its clocks.
fn state_key(h: &H) -> Vec<u8> {
    let s = h.g.script().unwrap();
    let mut vars = s.vars.clone();
    for k in ["RND", "RANDOM", "NUMFRAMEVIDEO"] {
        if let Ok(i) = s.names.id(defnames::VARIABLES, k.as_bytes()) {
            vars[i] = 0;
        }
    }
    format!(
        "{} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {}",
        s.chapter,
        h.g.place(),
        vars,
        s.flags
            .iter()
            .enumerate()
            .filter(|(_, &f)| f != 0)
            .collect::<Vec<_>>(),
        s.bag.iter().map(|e| e.id).collect::<Vec<_>>(),
        s.shown,
        h.g.zone_table_2004(),
        h.g.mode(),
        h.g.object_in_hand()
    )
    .into_bytes()
}

/// The search (see the module's text). `STORY2004_OUT` = the walk written.
#[test]
#[ignore]
fn explore() {
    let Some(dir) = cd() else { return };
    let out = std::env::var("STORY2004_OUT").unwrap_or_else(|_| "/dev/null".into());
    let max_tries: usize = std::env::var("STORY2004_TRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let mut h = H::new(&dir);
    assert!(h.g.new_game());
    h.trace_on();
    h.settle().unwrap();
    let mut seen_lines: HashSet<(i32, usize)> = h.take_lines().into_iter().collect();
    let mut walk: Vec<String> = vec!["START".into()];
    if let Ok(w) = std::env::var("STORY2004_RESUME") {
        // Replays a walk first (its actions), then searches on.
        let text = std::fs::read_to_string(w).unwrap();
        for l in text.lines() {
            if l.starts_with('#') || l == "START" || l.trim().is_empty() {
                continue;
            }
            h.act(l).unwrap_or_else(|e| panic!("{l}: {e}"));
            if std::env::var_os("STORY2004_DEBUG").is_some() {
                eprintln!(
                    "resume {l}: chapter {} place {:?} events {:?} centre {:?}",
                    h.chapter(),
                    h.g.place(),
                    h.events,
                    h.g.centre_zone()
                );
            }
            seen_lines.extend(h.take_lines());
            walk.push(l.to_string());
        }
        walk.push(format!(
            "# resumed: chapter {} place {:?}",
            h.chapter(),
            h.g.place()
        ));
    }
    let secs: HashMap<i32, Vec<Section>> = (0..1000)
        .filter(|c| {
            dir.join(format!("cd0/datas/Scenar/chapi{c:03}.con"))
                .exists()
        })
        .map(|c| (c, chapter_sections(&dir, c)))
        .collect();
    let threads: usize = std::env::var("STORY2004_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let mut cur = h.snapshot().unwrap();
    let mut log = String::new();
    let started = std::time::Instant::now();
    'milestones: for _ in 0..5000 {
        // Best first from `cur`: (heuristic, cost, order).
        let mut q: std::collections::BinaryHeap<std::cmp::Reverse<(usize, usize, usize)>> =
            Default::default();
        let mut nodes: Vec<(Vec<u8>, Vec<String>)> = Vec::new();
        let mut visited: HashSet<Vec<u8>> = HashSet::new();
        {
            let h0 = H::restore(&dir, &cur);
            visited.insert(state_key(&h0));
        }
        nodes.push((cur.clone(), Vec::new()));
        q.push(std::cmp::Reverse((0, 0, 0)));
        let mut expanded = 0;
        log.clear();
        // The nodes taken `threads` at a time (best first), their tries
        // run in parallel, each from its own load of the node's save.
        struct Try {
            node: usize,
            a: String,
            r: Result<(), String>,
            lines: Vec<(i32, usize)>,
            end: Option<String>,
            key: Vec<u8>,
            snap: Option<Vec<u8>>,
            hv: usize,
        }
        fn par_map<T: Sync, R: Send>(
            threads: usize,
            items: &[T],
            f: impl Fn(&T) -> R + Sync,
        ) -> Vec<R> {
            let next = std::sync::atomic::AtomicUsize::new(0);
            let mut out: Vec<(usize, R)> = std::thread::scope(|sc| {
                let hs: Vec<_> = (0..threads)
                    .map(|_| {
                        sc.spawn(|| {
                            let mut v = Vec::new();
                            loop {
                                let j = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                if j >= items.len() {
                                    break;
                                }
                                v.push((j, f(&items[j])));
                            }
                            v
                        })
                    })
                    .collect();
                hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
            });
            out.sort_by_key(|(j, _)| *j);
            out.into_iter().map(|(_, r)| r).collect()
        }
        loop {
            if expanded > max_tries {
                break;
            }
            let mut batch = Vec::new();
            while batch.len() < threads {
                match q.pop() {
                    Some(std::cmp::Reverse((_, cost, i))) => batch.push((i, cost)),
                    None => break,
                }
            }
            if batch.is_empty() {
                break;
            }
            let work: Vec<(usize, String)> = {
                let dir = &dir;
                let nodes = &nodes;
                let lists = par_map(threads, &batch, |&(i, _)| {
                    let mut h0 = H::restore(dir, &nodes[i].0);
                    let mut cache = HashMap::new();
                    candidates(&mut h0, dir, &mut cache)
                });
                batch
                    .iter()
                    .zip(lists)
                    .flat_map(|(&(i, _), l)| l.into_iter().map(move |a| (i, a)))
                    .collect()
            };
            let tries: Vec<Try> = {
                let seen = &seen_lines;
                let secs = &secs;
                let dir = &dir;
                let nodes = &nodes;
                par_map(threads, &work, |(i, a)| {
                    let mut h1 = H::restore(dir, &nodes[*i].0);
                    let r = h1.act(a);
                    let lines = h1.take_lines();
                    let end = (r.is_ok() && (h1.g.mode() == Mode::Menu || h1.var("CREDITS") != 0))
                        .then(|| format!("mode {:?} events {:?}", h1.g.mode(), h1.events));
                    let (key, snap2, hv) = if r.is_ok() && end.is_none() {
                        (state_key(&h1), h1.snapshot(), heuristic(&h1, secs, seen))
                    } else {
                        (Vec::new(), None, 0)
                    };
                    Try {
                        node: *i,
                        a: a.clone(),
                        r,
                        lines,
                        end,
                        key,
                        snap: snap2,
                        hv,
                    }
                })
            };
            for t in tries {
                expanded += 1;
                let a = t.a;
                let lines = t.lines;
                let cost = batch.iter().find(|b| b.0 == t.node).unwrap().1;
                let mut p2 = nodes[t.node].1.clone();
                p2.push(a.clone());
                if let Err(e) = t.r {
                    log.push_str(&format!("  refused {p2:?}: {e}\n"));
                    continue;
                }
                let new: Vec<(i32, usize)> = lines
                    .iter()
                    .copied()
                    .filter(|l| !seen_lines.contains(l))
                    .collect();
                if let Some(e) = t.end {
                    walk.push(format!("# the end: {e}"));
                    walk.extend(p2.iter().cloned());
                    eprintln!("END reached: {p2:?}");
                    break 'milestones;
                }
                if !new.is_empty() {
                    // The path played on the continuous game: the walk
                    // must hold without the loads.
                    let mut main_lines = Vec::new();
                    let mut failed = None;
                    let mut m = H::new(&dir);
                    std::mem::swap(&mut m, &mut h);
                    for x in &p2 {
                        if let Err(e) = m.act(x) {
                            failed = Some(format!("{x}: {e}"));
                            break;
                        }
                        main_lines.extend(m.take_lines());
                    }
                    let ok = failed.is_none() && main_lines.iter().any(|l| new.contains(l));
                    if !ok {
                        // Back to the committed state.
                        m = H::restore_main(&dir, &walk);
                        log.push_str(&format!(
                            "  diverged {p2:?}: {failed:?} (new {:?} not run)\n",
                            new
                        ));
                        eprintln!("DIVERGED {p2:?}: {failed:?}");
                        std::mem::swap(&mut m, &mut h);
                        continue;
                    }
                    std::mem::swap(&mut m, &mut h);
                    let Some(s2) = h.snapshot() else {
                        log.push_str(&format!("  no save {p2:?}\n"));
                        continue;
                    };
                    seen_lines.extend(lines.iter().copied());
                    seen_lines.extend(main_lines.iter().copied());
                    let line = format!(
                        "# chapter {} place {:?} new lines {} events {:?} ({:.0} s, {expanded} tried)",
                        h.chapter(),
                        h.g.place(),
                        new.len(),
                        h.events,
                        started.elapsed().as_secs_f64()
                    );
                    eprintln!("{line}\n  {p2:?}");
                    walk.push(line);
                    walk.extend(p2.iter().cloned());
                    std::fs::write(&out, walk.join("\n") + "\n").unwrap();
                    cur = s2;
                    continue 'milestones;
                }
                if visited.insert(t.key) {
                    let Some(s2) = t.snap else {
                        log.push_str(&format!("  no save {p2:?}\n"));
                        continue;
                    };
                    let c2 = cost + usize::from(!a.starts_with("GO "));
                    nodes.push((s2, p2));
                    q.push(std::cmp::Reverse((t.hv * 4 + c2, c2, nodes.len() - 1)));
                }
            }
        }
        eprintln!(
            "STUCK after {expanded} tries at chapter {} \n{log}",
            H::restore(&dir, &cur).chapter()
        );
        walk.push(format!("# STUCK after {expanded} tries"));
        break;
    }
    for l in log.lines().take(200) {
        walk.push(format!("# {l}"));
    }
    std::fs::write(&out, walk.join("\n") + "\n").unwrap();
}

/// The walk played in one run from a new game: every action accepted,
/// and at each milestone (`# chapter N place ("x.wam", Some(v))`) the
/// chapter and the place the search reached.
#[test]
fn walk() {
    let Some(dir) = cd() else { return };
    let text = include_str!("golden/story2004/walk.txt");
    let mut h = H::new(&dir);
    let mut steps = 0;
    let mut chapters = BTreeSet::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "START" {
            assert!(h.g.new_game());
            h.settle().unwrap();
            continue;
        }
        if let Some(rest) = line.strip_prefix("# chapter ") {
            let (c, rest) = rest.split_once(' ').unwrap();
            let c: i32 = c.parse().unwrap();
            let place = rest
                .strip_prefix("place (\"")
                .and_then(|r| r.split_once("\", Some("))
                .map(|(w, r)| {
                    (
                        w.to_string(),
                        r.split(')').next().unwrap().parse::<u32>().unwrap(),
                    )
                });
            assert_eq!(h.chapter(), c, "line {}: {line}", n + 1);
            if let Some((w, v)) = place {
                assert_eq!(h.g.place(), (w.as_str(), Some(v)), "line {}: {line}", n + 1);
            }
            chapters.insert(c);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(e) = line.strip_prefix("EXPECT ") {
            let (k, v) = e.split_once('=').unwrap();
            let v: i32 = v.parse().unwrap();
            assert_eq!(h.var(k), v, "line {}: {line}", n + 1);
            continue;
        }
        h.act(line)
            .unwrap_or_else(|e| panic!("line {}: {line}: {e}", n + 1));
        steps += 1;
    }
    eprintln!("{steps} actions, chapters {chapters:?}");
    assert!(steps > 0);
}
