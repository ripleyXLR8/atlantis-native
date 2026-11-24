//! *Atlantis V* played from its first chapter to its credits, headless,
//! through the player's own scripts (`ATLANTIS5_DVD_FR`: the folder
//! holding `datas/`, the French DVD's cabinets extracted; skipped when
//! unset).
//!
//! The player's moves are clicks with the screen centre on a zone or a
//! sprite, the inventory's cells, and the subject menu's subjects, as in
//! the game: a walk between views clicks the views' move zones (a path
//! found over the scenes' WAM files), an item is put in the hand from the
//! inventory, a character is spoken to through its subject menu. Films
//! are skipped and dialogs ended with Space. What the steps do comes from
//! the scripts' own events and flags: each chapter's test plays its
//! chapter until the scripts start the next one (`Atl_InitChapter`), the
//! last one until `Atl_TerminateGame` and the credits.
//!
//! `chapter_0n` plays a new game through chapter n (the chapters before
//! set what it needs: the characters' subjects, the flags);
//! `whole_game` plays the nine and the credits; a game over (the death
//! panel, then the autosave loaded) has its own test. Puzzles are solved
//! from the scripts' own state (the codes, the boards), with the solvers
//! of `atlantis5_puzzles.rs`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis2_data::wam::Wam;
use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::{Hover, InventoryHit, Play};
use atlantis5_script::Value;
use atlantis_data::FileSource;

fn dvd() -> Option<PathBuf> {
    match std::env::var_os("ATLANTIS5_DVD_FR") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("SKIPPED: ATLANTIS5_DVD_FR is not set");
            None
        }
    }
}

fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

fn text(v: &[u8]) -> String {
    String::from_utf8_lossy(v).into_owned()
}

/// A player of the game.
struct Walker {
    play: Play,
    src: Arc<dyn FileSource>,
    wams: HashMap<String, Wam>,
    /// The cells of the inventory's grid (slot 0's point) and its right
    /// button, found once.
    cell0: Option<(i32, i32)>,
    right_button: Option<(i32, i32)>,
    steps: usize,
    /// Prefixes of the named move zones the walks do not take (they lead
    /// out of the scene: the lifts, the stairways).
    avoid: Vec<String>,
    /// Moves the scripts send to another view now: (scene, the zone's
    /// view, the view entered).
    redirects: Vec<(String, i32, i32)>,
}

impl Walker {
    fn new(d: &Path) -> Walker {
        let src: Arc<dyn FileSource> =
            Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
        let mut play = Play::new(src.clone(), "french", None).expect("play");
        // The player's saves (the autosave `Atl_Save` writes, `Atl_Load`
        // reads) in a scratch folder of their own.
        static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let base =
            std::env::var_os("ATLANTIS_NATIVE_DATA").map_or_else(std::env::temp_dir, PathBuf::from);
        let saves = base.join(format!(
            "a5-walk-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&saves);
        std::fs::create_dir_all(&saves).expect("scratch folder");
        play.set_save_dir(Some(saves));
        // `A5_GC_STRESS`: the Lua collector runs all the time.
        if std::env::var_os("A5_GC_STRESS").is_some() {
            play.game.script().gc_pace(100, 1000);
        }
        Walker {
            play,
            src,
            wams: HashMap::new(),
            cell0: None,
            right_button: None,
            steps: 0,
            avoid: vec![
                "lift_floor".into(),
                "stairway_".into(),
                "zeppelin_exit".into(),
                "ish_pal_zeppelin_in".into(),
            ],
            redirects: Vec::new(),
        }
    }

    /// A new game played to its first view.
    fn new_game(d: &Path) -> Walker {
        let mut w = Walker::new(d);
        w.play.new_game();
        w.settle();
        w
    }

    fn shot(&self, name: &str) {
        if let Some(dir) = std::env::var_os("ATLANTIS5_SHOT_DIR") {
            let mut px = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
            self.play.render(&mut px);
            let png = atlantis2_engine::png::encode_xrgb8888(&px, VIEW_WIDTH, VIEW_HEIGHT);
            let _ = std::fs::write(PathBuf::from(dir).join(name), png);
        }
    }

    /// Frames with the films skipped and the dialogs ended.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            if self.play.stage().film_playing() {
                self.play.skip();
            }
            if self.play.stage().dialog_running() {
                self.play.space();
            }
            self.play.update(1.0 / 30.0);
        }
    }

    /// Frames until the game is in play with no script running.
    fn settle(&mut self) {
        for _ in 0..20_000 {
            self.frames(1);
            if self.play.game.state() == 4
                && !self.play.game.busy()
                && !self.play.game.requests_pending()
            {
                self.frames(1);
                return;
            }
        }
        panic!(
            "not back to play (state {}, at {:?})",
            self.play.game.state(),
            self.address()
        );
    }

    fn lua(&mut self, expr: &str) -> Value {
        self.play
            .game
            .script()
            .one_shot(&format!("return {expr}"), None, 1)
            .unwrap_or_else(|e| panic!("{expr}: {e}"))
            .remove(0)
    }

    fn lua_str(&mut self, expr: &str) -> String {
        match self.lua(expr) {
            Value::Str(s) => text(&s),
            Value::Nil => String::new(),
            v => format!("{v:?}"),
        }
    }

    fn chapter_now(&mut self) -> u32 {
        match self.lua("Atl_Chapter_Get()") {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Value::Num(x) => x as u32,
            v => panic!("chapter {v:?}"),
        }
    }

    fn state(&mut self, object: &str) -> String {
        self.lua_str(&format!("Atl_Object_GetState(\"{object}\")"))
    }

    fn flag(&mut self, name: &str) -> String {
        self.lua_str(&format!("tostring(Atl_Flag_Get(\"{name}\"))"))
    }

    fn hand(&mut self) -> String {
        self.lua_str("Atl_Hero_GetHandledItem()")
    }

    /// Whether `item` is in the hand or the inventory.
    fn has(&mut self, item: &str) -> bool {
        self.hand() == item
            || matches!(
                self.lua(&format!(
                    "Atl_Inventory_HasItem(Atl_Hero_GetInventory(), \"{item}\")"
                )),
                Value::Bool(true)
            )
    }

    fn address(&self) -> (String, i32) {
        let (s, v) = self.play.stage().address().unwrap_or_default();
        (text(&s), v)
    }

    fn wam(&mut self, scene: &str) -> &Wam {
        if !self.wams.contains_key(scene) {
            let data = self
                .src
                .read(&format!("datas/scenes/{scene}.wam"))
                .unwrap_or_else(|| panic!("{scene}.wam"));
            self.wams
                .insert(scene.to_string(), Wam::parse(&data).expect("wam"));
        }
        &self.wams[scene]
    }

    /// The moves out of `view` of `scene`: the zones without a name, and
    /// the named move zones active now, with their target view; with
    /// `doors`, also the active named action zones whose number is a view
    /// of the scene (a door: a first click opens it, the scripts make it a
    /// move zone).
    fn moves(&mut self, scene: &str, view: i32, tier: u8) -> Vec<(i32, String)> {
        let (zones, views): (Vec<(i32, String, i32)>, Vec<i32>) = {
            let wam = self.wam(scene);
            #[allow(clippy::cast_sign_loss)]
            let zones = wam
                .view(view as u32)
                .map(|v| {
                    v.zones
                        .iter()
                        .map(|z| (z.value, z.name.clone().unwrap_or_default(), z.kind))
                        .collect()
                })
                .unwrap_or_default();
            #[allow(clippy::cast_possible_wrap)]
            let views = wam
                .places
                .iter()
                .flat_map(|p| p.views.iter().map(|v| v.id as i32))
                .collect();
            (zones, views)
        };
        let mut out = Vec::new();
        for (value, name, kind) in zones {
            if kind >= 1000 {
                continue;
            }
            if name.is_empty() {
                out.push((value, name));
                continue;
            }
            if tier == 0 || self.avoid.iter().any(|a| name.starts_with(a.as_str())) {
                continue;
            }
            match self.play.game.zone(name.as_bytes()) {
                Some((1, true)) => out.push((value, name)),
                Some((2, true)) if tier >= 2 && views.contains(&value) && value != view => {
                    out.push((value, name));
                }
                _ => {}
            }
        }
        out
    }

    /// The paths from `from` to the views of the current scene (by
    /// moves; doors only when no other way).
    fn paths(&mut self, scene: &str, from: i32) -> HashMap<i32, Vec<(i32, String)>> {
        let mut all = self.paths_with(scene, from, 0);
        for tier in 1..=2 {
            for (v, p) in self.paths_with(scene, from, tier) {
                all.entry(v).or_insert(p);
            }
        }
        all
    }

    fn paths_with(&mut self, scene: &str, from: i32, tier: u8) -> HashMap<i32, Vec<(i32, String)>> {
        let mut seen: HashMap<i32, Vec<(i32, String)>> = HashMap::new();
        seen.insert(from, Vec::new());
        let mut queue = VecDeque::from([from]);
        while let Some(v) = queue.pop_front() {
            let path = seen[&v].clone();
            for (to, name) in self.moves(scene, v, tier) {
                // A move the scripts send elsewhere lands there.
                let lands = self
                    .redirects
                    .iter()
                    .find(|r| r.0 == scene && r.1 == to && name.is_empty())
                    .map_or(to, |r| r.2);
                if seen.contains_key(&lands) {
                    continue;
                }
                let mut p = path.clone();
                p.push((to, name));
                seen.insert(lands, p);
                queue.push_back(lands);
            }
        }
        seen
    }

    /// Clicks the zone numbered `id` named `name` in this view.
    fn click_zone(&mut self, id: i32, name: &str) {
        assert!(
            self.play.aim_at_zone_named(id, name.as_bytes()),
            "zone {name:?} {id} in {:?}",
            self.address()
        );
        assert_eq!(
            self.play.hover(),
            Some(&Hover::Zone(id, bytes(name))),
            "the pick takes zone {name:?} {id} in {:?}",
            self.address()
        );
        self.play.click();
        self.steps += 1;
        self.settle();
        self.trace(&format!("zone {name:?} {id}"));
    }

    fn trace(&mut self, what: &str) {
        if std::env::var_os("A5_TRACE").is_some() {
            let hand = self.hand();
            let mut watch = String::new();
            if let Ok(w) = std::env::var("A5_WATCH") {
                for e in w.split(';') {
                    let v = self.lua_str(&format!("tostring({e})"));
                    watch.push_str(&format!(" {e}={v}"));
                }
            }
            eprintln!("{:?} hand {hand:?}: {what}{watch}", self.address());
        }
    }

    /// Walks to `view` of the current scene by its move zones.
    fn walk(&mut self, view: i32) {
        let chapter = self.chapter_now();
        let mut tries: HashMap<(i32, i32, String), u32> = HashMap::new();
        for _ in 0..40 {
            let (scene, here) = self.address();
            if here == view {
                return;
            }
            let paths = self.paths(&scene, here);
            let path = paths
                .get(&view)
                .unwrap_or_else(|| panic!("no path from {scene} {here} to {view}"))
                .clone();
            let (to, name) = path[0].clone();
            // A door opens at the first click and moves at the second.
            let n = tries.entry((here, to, name.clone())).or_default();
            *n += 1;
            assert!(
                *n <= 2,
                "the zone {name:?} {to} of {scene} {here} does not move"
            );
            if !name.is_empty() && self.play.game.zone(name.as_bytes()) == Some((2, true)) {
                // A door: the hand empty to open it.
                self.empty_hand();
            }
            self.click_zone(to, &name);
            let (s2, _) = self.address();
            if s2 != scene && self.chapter_now() != chapter {
                // The view's script started the next chapter.
                return;
            }
            assert_eq!(s2, scene, "a move out of {scene} {here}");
        }
        panic!("not at {view}: {:?}", self.address());
    }

    /// The views of the current scene holding a zone named `name`.
    fn views_with_zone(&mut self, name: &str) -> Vec<(i32, i32)> {
        let (scene, _) = self.address();
        let wam = self.wam(&scene);
        let mut out = Vec::new();
        for p in &wam.places {
            for v in &p.views {
                for z in &v.zones {
                    if z.kind < 1000 && z.name.as_deref() == Some(name) {
                        #[allow(clippy::cast_possible_wrap)]
                        out.push((v.id as i32, z.value));
                    }
                }
            }
        }
        out
    }

    /// Goes to the nearest view with the zone `name`, takes `item` in the
    /// hand when given, and clicks the zone.
    fn zone_with(&mut self, name: &str, item: Option<&str>) {
        let (scene, here) = self.address();
        let candidates = self.views_with_zone(name);
        assert!(!candidates.is_empty(), "no view of {scene} has zone {name}");
        let paths = self.paths(&scene, here);
        let best = candidates
            .iter()
            .filter(|(v, _)| paths.contains_key(v))
            .min_by_key(|(v, _)| paths[v].len())
            .copied()
            .unwrap_or_else(|| {
                panic!("no way from {scene} {here} to zone {name} ({candidates:?})")
            });
        self.walk(best.0);
        match item {
            Some("") => self.empty_hand(),
            Some(i) => self.hold(i),
            None => {}
        }
        self.click_zone(best.1, name);
    }

    /// Clicks the sprite of `object` in this view.
    fn click_sprite_here(&mut self, object: &str) -> bool {
        let mut aimed = self.play.aim_at_sprite(object.as_bytes());
        for _ in 0..10 {
            if aimed {
                break;
            }
            self.frames(10);
            aimed = self.play.aim_at_sprite(object.as_bytes());
        }
        if !aimed {
            return false;
        }
        self.play.click();
        self.steps += 1;
        self.settle();
        self.trace(&format!("sprite {object}"));
        true
    }

    /// The views the scripts let the player reach `object` from.
    fn reachable(&mut self, object: &str) -> Vec<i32> {
        let n = match self.lua(&format!(
            "(function() local t, n = atl.objects[\"{object}\"].warp_reachable, 0 \
             while t[n + 1] do n = n + 1 end return n end)()"
        )) {
            #[allow(clippy::cast_possible_truncation)]
            Value::Num(x) => x as i32,
            v => panic!("{object}: {v:?}"),
        };
        (1..=n)
            .map(
                |i| match self.lua(&format!("atl.objects[\"{object}\"].warp_reachable[{i}]")) {
                    #[allow(clippy::cast_possible_truncation)]
                    Value::Num(x) => x as i32,
                    v => panic!("{object}: {v:?}"),
                },
            )
            .collect()
    }

    /// Goes to a view the object can be reached from and clicks its
    /// sprite.
    fn sprite(&mut self, object: &str) {
        self.sprite_with(object, None);
    }

    /// Goes to a view the object can be reached from, takes `item` in
    /// the hand when given (`Some("")`: the hand emptied), and clicks its
    /// sprite.
    fn sprite_with(&mut self, object: &str, item: Option<&str>) {
        let (_, here) = self.address();
        let mut views = self.reachable(object);
        views.sort_by_key(|&v| i32::from(v != here));
        for v in views {
            self.walk(v);
            match item {
                Some("") => self.empty_hand(),
                Some(i) => self.hold(i),
                None => {}
            }
            if self.click_sprite_here(object) {
                return;
            }
        }
        panic!(
            "no sprite {object} to click (at {:?}, reachable from {:?})",
            self.address(),
            self.reachable(object)
        );
    }

    /// The hand emptied: the right button puts its item back.
    fn empty_hand(&mut self) {
        if !self.hand().is_empty() {
            self.play.right_press();
            self.settle();
        }
        assert_eq!(self.hand(), "");
    }

    /// The inventory opened (the right button with an empty hand).
    fn open_inventory(&mut self) {
        self.empty_hand();
        self.play.right_press();
        self.frames(1);
        assert!(self.play.stage().inventory_open_now(), "the inventory");
        if self.cell0.is_none() {
            'find: for y in (0..VIEW_HEIGHT as i32).step_by(2) {
                for x in (0..VIEW_WIDTH as i32).step_by(2) {
                    self.play.mouse_at(x, y);
                    if matches!(
                        self.play.stage().inventory_hit(),
                        Some(InventoryHit::Slot(_))
                    ) {
                        // The grid's first cell is the leftmost.
                        self.cell0 = Some((x + 4, y + 4));
                        break 'find;
                    }
                }
            }
            'right: for y in (0..VIEW_HEIGHT as i32).step_by(2) {
                for x in (0..VIEW_WIDTH as i32).step_by(2) {
                    self.play.mouse_at(x, y);
                    if self.play.stage().inventory_hit() == Some(InventoryHit::Right) {
                        self.right_button = Some((x + 2, y + 2));
                        break 'right;
                    }
                }
            }
        }
    }

    /// The inventory's slot of `item`.
    fn slot_of(&mut self, item: &str) -> usize {
        let (_, items) = self.play.game.script().inventory();
        items
            .iter()
            .position(|i| i.as_deref() == Some(item.as_bytes()))
            .unwrap_or_else(|| panic!("{item} in the inventory: {items:?}"))
    }

    /// Points the mouse at `slot`'s cell, paging the grid to it.
    fn point_slot(&mut self, slot: usize) {
        let (x0, y0) = self.cell0.expect("the grid");
        for _ in 0..40 {
            for c in 0..10 {
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                self.play.mouse_at(x0 + 80 * c, y0);
                if self.play.stage().inventory_hit() == Some(InventoryHit::Slot(slot)) {
                    return;
                }
            }
            // The page's buttons.
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let first = {
                self.play.mouse_at(x0, y0);
                match self.play.stage().inventory_hit() {
                    Some(InventoryHit::Slot(s)) => s,
                    _ => 0,
                }
            };
            if slot < first {
                panic!("slot {slot} left of the grid");
            }
            // A `CIE_Button`: hot after a move onto it, the page turned
            // when its click's picture ends (2 frames at 15 a second).
            let (rx, ry) = self.right_button.expect("the right button");
            self.play.mouse_at(rx, ry);
            self.frames(1);
            self.play.click();
            self.frames(6);
        }
        panic!("slot {slot} not shown");
    }

    /// `item` put in the hand from the inventory.
    fn hold(&mut self, item: &str) {
        if self.hand() == item {
            return;
        }
        self.open_inventory();
        let slot = self.slot_of(item);
        self.point_slot(slot);
        self.play.click();
        self.frames(1);
        self.close_inventory();
        assert_eq!(self.hand(), item);
    }

    fn close_inventory(&mut self) {
        if self.play.stage().inventory_open_now() {
            self.play.mouse_at(VIEW_WIDTH as i32 / 2, 40);
            self.play.right_press();
        }
        self.settle();
    }

    /// The hand's item used on the inventory's `item` (a compound
    /// object).
    fn combine(&mut self, held: &str, item: &str) {
        // The inventory opens with an empty hand; `held` taken from its
        // cell, then a click on `item`'s.
        self.open_inventory();
        let slot = self.slot_of(held);
        self.point_slot(slot);
        self.play.click();
        self.frames(1);
        assert_eq!(self.hand(), held);
        let slot = self.slot_of(item);
        self.point_slot(slot);
        self.play.click();
        for _ in 0..200 {
            self.frames(1);
            if !self.play.game.busy() {
                break;
            }
        }
        self.close_inventory();
    }

    /// Speaks to `character` (its sprite `sprite`) about `subject`
    /// through the subject menu, then closes the menu.
    fn talk(&mut self, sprite: &str, subject: &str) {
        self.empty_hand();
        let (_, here) = self.address();
        let mut views = self.reachable_or_here(sprite);
        views.sort_by_key(|&v| i32::from(v != here));
        let mut opened = false;
        for v in views {
            self.walk(v);
            if self.play.aim_at_sprite(sprite.as_bytes()) {
                opened = true;
                break;
            }
        }
        assert!(opened, "{sprite} to speak to at {:?}", self.address());
        self.play.click();
        self.steps += 1;
        let mut subjects = Vec::new();
        for _ in 0..600 {
            self.frames(1);
            subjects = self.play.stage().menu_subjects();
            if !subjects.is_empty() {
                break;
            }
        }
        let i = subjects
            .iter()
            .position(|s| s == subject.as_bytes())
            .unwrap_or_else(|| {
                panic!(
                    "{sprite}: no subject {subject} in {:?}",
                    subjects.iter().map(|s| text(s)).collect::<Vec<_>>()
                )
            });
        let (x, y) = self.play.stage().menu_subject_point(i).expect("subject");
        self.play.mouse_at(x, y);
        self.play.click();
        // The subject's script, then the menu made again (or closed by
        // the script, or a chapter started); a press in the frame it is
        // made again is dropped (@ 0x00410f90 state 2).
        for _ in 0..30 * 600 {
            self.frames(1);
            if !self.play.game.busy()
                && !self.play.stage().dialog_running()
                && (self.play.stage().menu_shown() || !self.play.stage().menu_open_now())
            {
                break;
            }
        }
        if self.play.stage().menu_open_now() {
            self.play.mouse_at(4, 4);
            self.play.click();
        }
        self.settle();
    }

    /// Picks `subject` of `sprite`, whose script opens a board (poker):
    /// frames until the board is up.
    fn talk_open(&mut self, sprite: &str, subject: &str) {
        self.empty_hand();
        assert!(self.play.aim_at_sprite(sprite.as_bytes()), "{sprite}");
        self.play.click();
        let mut subjects = Vec::new();
        for _ in 0..600 {
            self.frames(1);
            subjects = self.play.stage().menu_subjects();
            if !subjects.is_empty() {
                break;
            }
        }
        let i = subjects
            .iter()
            .position(|s| s == subject.as_bytes())
            .unwrap_or_else(|| panic!("{sprite}: no subject {subject}"));
        let (x, y) = self.play.stage().menu_subject_point(i).expect("subject");
        self.play.mouse_at(x, y);
        self.play.click();
        for _ in 0..600 {
            self.frames(1);
            if self.play.poker().is_some() {
                return;
            }
        }
        panic!("{sprite} {subject}: no board");
    }

    fn reachable_or_here(&mut self, object: &str) -> Vec<i32> {
        let known = matches!(
            self.lua(&format!("atl.objects[\"{object}\"] ~= nil")),
            Value::Bool(true)
        );
        if known {
            let v = self.reachable(object);
            if !v.is_empty() {
                return v;
            }
        }
        vec![self.address().1]
    }

    /// The zone `name` (a lift, a stairway, an exit) clicked with an
    /// empty hand to go to `scene`.
    fn go_to(&mut self, name: &str, scene: &str) {
        self.touch_zone(name);
        assert_eq!(self.address().0, scene, "{name} leads to {scene}");
    }

    /// The sprite of `object` clicked with an empty hand.
    fn touch(&mut self, object: &str) {
        self.sprite_with(object, Some(""));
    }

    /// The zone `name` clicked with an empty hand.
    fn touch_zone(&mut self, name: &str) {
        self.zone_with(name, Some(""));
    }

    /// `item` held and used on the zone `name`.
    fn use_on_zone(&mut self, item: &str, name: &str) {
        self.zone_with(name, Some(item));
    }

    /// `item` held and used on the sprite of `object`.
    fn use_on_sprite(&mut self, item: &str, object: &str) {
        self.sprite_with(object, Some(item));
    }
}

// ---------------------------------------------------------------- chapters

/// Chapter 1, the zeppelin (`chapter_01.lua`, `on_click_zeppelin.lua`):
/// the gorilla's three missions, the pipe, the engine and the radio, then
/// the telex ribbon given to him (`Atl_InitChapter(2)`).
fn chapter_1(w: &mut Walker) {
    // The gorilla: who he is, then the Hindenburg (the pipe's mission).
    w.talk("gorilla", "gorille");
    w.talk("gorilla", "hindenburg");
    assert_eq!(w.flag("mission_fix_pipe"), "in");
    // The tools: the ashtray breaks the hammer's glass, the crab breaker,
    // the ladder, the radio drawer's nuts and bolts.
    w.touch("cendar");
    w.use_on_zone("cendar", "glass_breaker_door");
    w.touch_zone("glass_breaker_door");
    assert!(w.has("glass_breaker"));
    w.touch("crab_breaker");
    w.touch("ladder");
    w.touch_zone("radio_post_in");
    w.touch("radio_drawer");
    w.touch("radio_drawer");
    assert!(w.has("radio_nuts_and_bolts"));
    w.use_on_zone("ladder", "tank_top_access");
    // The healthy pipe taken with valve 3 closed (IsTankSupplied's w1).
    w.touch("valve_03");
    w.use_on_sprite("crab_breaker", "healthy_pipe");
    assert!(w.has("healthy_pipe"));
    // The gas cut, the broken pipe off, the healthy one in its place.
    w.touch("gaz_lever");
    w.use_on_sprite("crab_breaker", "broken_pipe_nuts");
    w.use_on_sprite("glass_breaker", "broken_pipe_bolts");
    w.touch("broken_pipe");
    w.use_on_zone("healthy_pipe", "broken_pipe_patch");
    w.use_on_sprite("radio_nuts_and_bolts", "healthy_pipe_patch");
    assert_eq!(w.state("healthy_pipe"), "patch");
    // IsPipeFixed: valves 1 and 2 open, 3 closed, the gas open.
    w.touch("valve_01");
    w.touch("gaz_lever");
    assert_eq!(w.flag("mission_fix_pipe"), "done");
    // The engine: its lever breaks the belt; the gorilla's belt instead.
    w.talk("gorilla", "hindenburg");
    assert_eq!(w.flag("mission_fix_engine"), "in");
    w.touch("engine_lever");
    w.touch("engine_lever");
    w.talk("gorilla", "ceinture");
    w.use_on_sprite("gorilla_belt", "engine");
    w.touch("engine_lever");
    assert_eq!(w.flag("mission_fix_engine"), "done");
    // The radio: the phone's cable as the radio's patch, the telex.
    w.talk("gorilla", "hindenburg");
    assert_eq!(w.flag("mission_fix_radio"), "in");
    w.touch("coin");
    w.use_on_zone("coin", "phone_cable_screw");
    w.touch_zone("phone_cable");
    for s in [
        "radio_screw_a",
        "radio_screw_b",
        "radio_screw_c",
        "radio_screw_d",
    ] {
        w.touch(s);
    }
    w.use_on_zone("phone_cable", "radio_patch");
    w.touch_zone("radio_keyboard");
    w.touch("telex_ribbon");
    w.use_on_sprite("telex_ribbon", "gorilla");
}

/// Chapter 2, floor 99 (`chapter_02.lua`, `on_click_etage99.lua`): Elmer's
/// note given back, the waste basket filled with paper and lit in a
/// brazier; Elmer leaves the lift for the fire and view 6 starts
/// chapter 3 (`on_enter_view_etage99.lua`).
fn chapter_2(w: &mut Walker) {
    w.touch("mailbox_elmer");
    w.touch("elmer_note");
    w.use_on_sprite("elmer_note", "elmer");
    assert_eq!(w.state("elmer_note"), "given");
    w.touch("side_table");
    w.touch("side_table");
    assert!(w.has("lighter"));
    w.touch("waste_basket");
    w.touch("trash_paper");
    assert!(w.has("paper_to_burn"));
    w.combine("paper_to_burn", "waste_basket");
    assert!(w.has("waste_basket_full_of_paper"));
    w.use_on_zone("waste_basket_full_of_paper", "brazier");
    let (_, view) = w.address();
    w.use_on_sprite("lighter", &format!("waste_basket_brazier_0{view}"));
    assert_eq!(w.flag("mission_elmer"), "done");
    w.walk(6);
}

/// Chapter 3, the ground floor (`chapter_03.lua`, `on_click_etage0.lua`):
/// the journalist's star leads to the hostess, whose flirt gives Elmore
/// his star subject; Elmore on Elmer tells about Garbo, so the journalist
/// gives his badge; shown to Elmore it opens the lift to floor 18, which
/// starts chapter 4.
fn chapter_3(w: &mut Walker) {
    w.talk("journalist", "journaliste");
    w.talk("journalist", "vedette");
    w.talk("hostess", "vedette");
    w.talk("hostess", "drague");
    w.talk("elmore", "vedette");
    w.talk("elmore", "groom_elmer");
    assert_eq!(w.flag("info_garbo"), "true");
    w.talk("journalist", "badge");
    assert!(w.has("badge"));
    w.use_on_sprite("badge", "elmore");
    assert_eq!(w.flag("elmore_saw_badge"), "yes");
    w.touch_zone("lift_floor_18");
}

/// The shortest presses from `from` to `to` with the steps `moves`
/// (kept within 0..=max, as `Atl_PuzzleLanskyChest_OnButtonActivated`
/// keeps them).
fn presses(from: i32, to: i32, max: i32, moves: &[i32]) -> Vec<i32> {
    let idx = |x: i32| usize::try_from(x).expect("in range");
    let mut prev = vec![None; idx(max + 1)];
    let mut queue = VecDeque::from([from]);
    prev[idx(from)] = Some((from, 0));
    while let Some(x) = queue.pop_front() {
        if x == to {
            break;
        }
        for &m in moves {
            let y = x + m;
            if (0..=max).contains(&y) && prev[idx(y)].is_none() {
                prev[idx(y)] = Some((x, m));
                queue.push_back(y);
            }
        }
    }
    let mut out = Vec::new();
    let mut x = to;
    while x != from {
        let (p, m) = prev[idx(x)].expect("reachable");
        out.push(m);
        x = p;
    }
    out.reverse();
    out
}

/// Chapter 4, floors 18, 19 and 12 (`chapter_04.lua`, the floors'
/// `on_click`): the den mother's bugsy subject makes the guys suspect;
/// Garbo's gorilla, nice once the hostess's name is known, sends to Susan,
/// whose story stuns the den mother and tells of Lansky's door. The
/// magazine frees Ellen's hairpin; the den mother taken to the hospital
/// leaves her box (the lift board key); the hairpin opens Lansky's office
/// (the chest's engine room key, the painting's lift code). With the
/// three, the lift to floor −2 starts chapter 5.
fn chapter_4(w: &mut Walker) {
    // Floor 18: Ellen's name, Ellen, the den mother.
    w.touch_zone("ellen_name");
    assert_eq!(w.flag("ellen_name_seen"), "true");
    w.talk("ellen-attached_hairs", "ascenseur");
    w.talk("ellen-attached_hairs", "secretaire_ellen");
    w.talk("den_mother", "ascenseur");
    w.talk("den_mother", "bugsy");
    assert_eq!(w.flag("suspect_guys"), "true");
    // Floor 19: Cineworld's secretary (the magazine), Garbo's gorilla.
    w.go_to("stairway_floor_19", "etage19");
    w.talk("cineworld_secretary", "secretaire_ellen");
    w.touch("cineworld_magazine");
    w.talk("garbo_gorilla", "vedette");
    w.talk("garbo_gorilla", "receptionniste");
    assert_eq!(w.flag("require_hostess_name"), "true");
    // The ground floor: the hostess's name.
    w.go_to("stairway_floor_18", "etage18");
    w.go_to("lift_floor_0", "etage0");
    w.talk("hostess", "drague");
    assert_eq!(w.flag("know_hostess_name"), "true");
    w.go_to("lift_floor_18", "etage18");
    w.go_to("stairway_floor_19", "etage19");
    w.talk("garbo_gorilla", "receptionniste");
    w.talk("garbo_gorilla", "agresseurs");
    // Floor 12: Susan.
    w.go_to("lift_floor_12", "etage12");
    w.talk("susan", "agresseurs");
    assert_eq!(w.state("den_mother"), "stunted");
    // Floor 18: Ellen's hairpin, the den mother to the hospital, her box.
    w.go_to("lift_floor_19", "etage19");
    w.go_to("stairway_floor_18", "etage18");
    w.use_on_sprite("cineworld_magazine", "ellen-attached_hairs");
    w.touch("hairpin");
    // The den mother away, the moves toward her office's view 6 lead to
    // view 19 (on_click_etage18.lua); its inner room shows her stunned,
    // and Ellen gets the subject.
    w.redirects.push(("etage18".into(), 6, 19));
    w.walk(17);
    w.talk("ellen-detached_hairs", "cheftaine");
    assert_eq!(w.state("den_mother"), "at_the_hospital");
    w.touch("den_mother_notepad");
    w.walk(18);
    for wheel in 1..=5 {
        let code = w.lua_str(&format!("tostring(atl.puzzle.dmbox.combination[{wheel}])"));
        let _ = code;
    }
    solve_den_mother_box(w);
    w.touch("lift_board_key");
    assert!(w.has("lift_board_key"));
    // Floor 12: Lansky's office, the jacket, the chest, the painting.
    w.go_to("stairway_floor_19", "etage19");
    w.go_to("lift_floor_12", "etage12");
    w.use_on_zone("hairpin", "lansky_desk_in");
    w.touch_zone("lansky_jacket");
    solve_lansky_chest(w);
    w.touch("engine_room_key");
    w.touch("floor12_painting");
    w.touch_zone("floor12_ventilation");
    assert!(w.has("floor12_lift_code"));
    // Floor 19: the lift to floor −2.
    w.go_to("lift_floor_19", "etage19");
    w.touch_zone("lift_floor_-2");
}

/// The den mother's box (view 18 of floor 18): each wheel's click turns
/// it one step, modulo 9 (`Atl_PuzzleDenMotherBox_OnClick`), to the
/// code `Atl_PuzzleDenMotherBox_Init` set.
fn solve_den_mother_box(w: &mut Walker) {
    for wheel in 1..=5 {
        let code = num(&w.lua(&format!("atl.puzzle.dmbox.combination[{wheel}]")));
        let now = num(&w.lua(&format!("atl.puzzle.dmbox.state[{wheel}]")));
        for _ in 0..(code - now).rem_euclid(9) {
            w.touch(&format!("den_mother_box_number_{wheel}"));
        }
    }
    assert_eq!(w.lua("Atl_PuzzleDenMotherBox_Check()"), Value::Bool(true));
}

/// Lansky's chest (view 10 of floor 12): the zones' numbers are the
/// moves (7 → +7, 8 → −7, 5 → +5, 6 → −5) to the values
/// `Atl_PuzzleLanskyChest_Check` asks.
fn solve_lansky_chest(w: &mut Walker) {
    w.walk(10);
    for metal in ["gold", "silver", "copper"] {
        let target = num(&w.lua(&format!("atl.puzzle.lansky_chest.combinaison.{metal}")));
        let max = num(&w.lua(&format!("atl.puzzle.lansky_chest.max.{metal}")));
        let now = num(&w.lua(&format!("atl.puzzle.lansky_chest.state.{metal}")));
        for m in presses(now, target, max, &[7, -7, 5, -5]) {
            let id = match m {
                7 => 7,
                -7 => 8,
                5 => 5,
                _ => 6,
            };
            w.empty_hand();
            w.click_zone(id, metal);
        }
    }
    assert_eq!(w.lua("Atl_PuzzleLanskyChest_Check()"), Value::Bool(true));
}

fn num(v: &Value) -> i32 {
    match v {
        #[allow(clippy::cast_possible_truncation)]
        Value::Num(x) => *x as i32,
        _ => panic!("not a number: {v:?}"),
    }
}

/// Chapter 5, floor −2 (`chapter_05.lua`, `on_click_etage-2.lua`): the
/// extinguisher breaks the cupboard (the jumpers), the engine room's key
/// opens its door, the board's key unlocks the lift board, and the
/// jumpers plugged to the code of `Atl_PuzzleLiftBoard_Init` cheat the
/// lifts (`lifts_cheated`); the lift to floor 14 starts chapter 6.
fn chapter_5(w: &mut Walker) {
    w.use_on_zone("engine_room_key", "engine_room_in");
    assert_eq!(w.state("engine_room_door"), "open");
    w.touch("floor-2_extinguisher");
    w.use_on_sprite("floor-2_extinguisher", "floor-2_cupboard_door");
    w.touch("jumpers");
    w.use_on_zone("lift_board_key", "lift_board_lock");
    w.use_on_zone("jumpers", "drop_jumpers");
    assert_eq!(w.state("jumpers"), "in_liftboard");
    w.touch("floor-2_lift_board_key");
    assert_eq!(w.flag("lift_board_locked"), "false");
    // Elevator e spans the code's floors code[e]..code[e + 1].
    let plugs = [
        (1, 0),
        (1, 10),
        (2, 10),
        (2, 15),
        (3, 9),
        (3, 15),
        (4, 9),
        (4, 16),
    ];
    for (k, (e, floor)) in plugs.into_iter().enumerate() {
        w.touch(&format!("unplugged_jumper_0{}", k + 1));
        w.click_zone(floor, &format!("elevator{e}"));
    }
    assert_eq!(w.flag("lifts_cheated"), "true");
    w.touch_zone("lift_floor_14");
}

/// Chapter 6, floor 28 (`chapter_06.lua`, `on_click_etage28.lua`): the
/// secretary opens Bugsy's office; Bugsy's chessboard won gives the
/// medallion and his talk on Atlantis, the heroes, Elliot; Miss
/// Pennington leads to Miss Sullivan, Sing like Rain and the Hindenburg,
/// which open the way out of floor 14 (chapter 7).
fn chapter_6(w: &mut Walker) {
    w.talk("bugsy_secretary", "bugsy");
    w.talk("bugsy", "bugsy");
    assert_eq!(w.flag("chessboard_game"), "playing");
    // The line of `atlantis5_puzzles.rs` (a breadth-first walk of the
    // scripts' rules under the reference Lua 5.0.3).
    for sq in [11, 7, 3, 9, 15, 19, 25, 19, 13, 17, 21, 17, 11, 7, 3, 9, 5] {
        w.empty_hand();
        assert!(w.click_sprite_here("player_pawn"), "the pawn");
        assert_eq!(w.hand(), "player_pawn");
        let name = format!("chess_white_box_{sq}");
        let id = w
            .play
            .zones()
            .into_iter()
            .find(|z| z.1 == name.as_bytes())
            .map(|z| z.0)
            .unwrap_or_else(|| panic!("{name} lit"));
        w.click_zone(id, &name);
    }
    assert_eq!(w.flag("chessboard_game"), "won");
    w.talk("bugsy", "medaillon");
    assert!(w.has("medallion"));
    w.talk("bugsy", "medaillon");
    w.talk("bugsy", "atlantide");
    w.talk("bugsy", "atlantide");
    w.talk("bugsy", "elliot");
    w.talk("bugsy", "heros");
    w.talk("bugsy", "bugsy");
    w.talk("bugsy", "miss_pennington");
    w.talk("bugsy_secretary", "miss_sullivan");
    w.talk("bugsy_secretary", "sing_like_rain");
    w.talk("bugsy_secretary", "hindenburg");
    w.touch_zone("floor14_out");
}

/// The 15-puzzle's shortest line (A*), as slots to click (the slot
/// moved into the hole).
fn sliding_solution(start: [u8; 16]) -> Vec<usize> {
    use std::collections::BinaryHeap;
    let h = |s: &[u8; 16]| -> u32 {
        let mut d = 0;
        for (i, &p) in s.iter().enumerate() {
            if p != 16 {
                let g = usize::from(p - 1);
                d += (i / 4).abs_diff(g / 4) + (i % 4).abs_diff(g % 4);
            }
        }
        u32::try_from(d).expect("small")
    };
    /// Each board's cost and the board and slot it came from.
    type Seen = HashMap<[u8; 16], (u32, Option<([u8; 16], usize)>)>;
    let mut best: Seen = HashMap::new();
    let mut open = BinaryHeap::new();
    best.insert(start, (0, None));
    open.push(std::cmp::Reverse((3 * h(&start), 0u32, start)));
    while let Some(std::cmp::Reverse((_, g, s))) = open.pop() {
        if h(&s) == 0 {
            let mut out = Vec::new();
            let mut cur = s;
            while let Some((_, Some((prev, slot)))) = best.get(&cur).copied() {
                out.push(slot);
                cur = prev;
            }
            out.reverse();
            return out;
        }
        if best.get(&s).is_some_and(|b| b.0 < g) {
            continue;
        }
        let e = s.iter().position(|&p| p == 16).expect("a hole");
        let mut next = Vec::new();
        if e >= 4 {
            next.push(e - 4);
        }
        if e % 4 < 3 {
            next.push(e + 1);
        }
        if e < 12 {
            next.push(e + 4);
        }
        if e % 4 > 0 {
            next.push(e - 1);
        }
        for slot in next {
            let mut t = s;
            t.swap(e, slot);
            if best.get(&t).is_none_or(|b| b.0 > g + 1) {
                best.insert(t, (g + 1, Some((s, slot))));
                open.push(std::cmp::Reverse((g + 1 + 3 * h(&t), g + 1, t)));
            }
        }
    }
    panic!("no solution");
}

impl Walker {
    /// `seconds` of play, then settled (a timer's handler runs).
    fn wait(&mut self, seconds: f64) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        self.frames((seconds * 30.0) as usize);
        self.settle();
    }

    /// The poker board played to its end: on the panel (state 3) the
    /// stake followed in games 1, 4 and 5 and stopped in 2 and 3 (the
    /// scenario's strengths, as in `atlantis5_poker.rs`); Elliot's raise
    /// (state 0xb) followed; each game's end (0xd) to the next.
    fn play_poker(&mut self) {
        for _ in 0..30_000 {
            self.frames(1);
            let Some(state) = self.play.poker().map(|p| p.state) else {
                break;
            };
            if self.play.game.busy() {
                continue;
            }
            let button = match state {
                3 => match num(&self.lua("atl.puzzle.poker.game")) {
                    2 | 3 => "stop_bet",
                    _ => "follow_bet",
                },
                0xb => "follow_bet",
                0xd => "next_game",
                _ => continue,
            };
            assert!(self.play.click_poker(button), "{button}");
        }
        assert!(self.play.poker().is_none(), "the board ended");
        self.settle();
    }
}

/// Chapter 7, Macao (`chapter_07.lua`, `on_click_macao.lua`): the
/// punter's rat race bet (the cash room behind the 15-puzzle, the
/// password paper, the clerk's ticket); Ishtar's talk starts the
/// blackout, for which the barman is made drunk (the bar's way to the
/// technical room); Ishtar's money shows the gorilla the game, the paper's
/// password gives the winch code; the winches raise the sail to the poker
/// room, where the five games are won; Sing like Rain's talk then starts
/// chapter 8.
fn chapter_7(w: &mut Walker) {
    // The punter's bet: his money, the cash room.
    w.talk("punter-rat", "course_de_rat");
    w.talk("punter-rat", "course_de_rat");
    assert!(w.has("money_for_rat_run"));
    w.touch("taquin_curtain");
    let mut slots = [0u8; 16];
    for (i, v) in slots.iter_mut().enumerate() {
        *v = u8::try_from(num(&w.lua(&format!("atl.puzzle.taquin.slots[{}]", i + 1))))
            .expect("a piece");
    }
    for slot in sliding_solution(slots) {
        let id = i32::try_from(slot).expect("slot");
        w.click_zone(id, &format!("taquin_slot_{slot}"));
    }
    assert_eq!(w.state("cash_room_door"), "open");
    // The paper while the clerk sleeps; then he is woken.
    w.touch("password_paper");
    w.touch("cash_clerk");
    w.wait(6.0);
    w.use_on_sprite("money_for_rat_run", "cash_clerk");
    w.talk("cash_clerk", "course_de_rat");
    assert!(w.has("rat_run_ticket"));
    w.use_on_sprite("rat_run_ticket", "punter-rat");
    assert_eq!(w.flag("mission_punter_bet"), "done");
    w.talk("punter-rat", "course_de_rat");
    assert!(w.has("money_for_prostitute"));
    w.talk("punter-rat", "partie_poker");
    w.talk("punter-rat", "miss_sullivan");
    assert_eq!(w.state("ishtar"), "gambling");
    // Ishtar: the blackout.
    w.talk("ishtar", "miss_sullivan");
    w.talk("ishtar", "course_de_rat");
    assert_eq!(w.flag("mission_blackout"), "in");
    // The barman made drunk by the lady's drinks.
    w.talk("prostitute", "dame_de_compagnie");
    w.talk("barman", "payer_verre_dame_de_compagnie");
    w.talk("barman", "ishtar");
    w.talk("barman", "tripot");
    w.talk("prostitute", "barman");
    w.talk("prostitute", "barman");
    assert_eq!(w.flag("d80"), "1");
    w.use_on_sprite("money_for_prostitute", "barman");
    // Three drinks, 15 s apart (the timer barman_drink).
    w.wait(50.0);
    assert_eq!(w.state("barman"), "asleep");
    // The technical room, the generator's lever, out of its view before
    // the gorilla puts the light back (10 s).
    w.touch("technical_room_door");
    w.touch_zone("elec_sys_lever");
    assert_eq!(w.address().1, 21);
    w.walk(22);
    w.wait(12.0);
    assert_eq!(w.flag("mission_blackout"), "done");
    // Ishtar again: the poker money.
    w.talk("ishtar", "course_de_rat");
    w.talk("ishtar", "miss_sullivan");
    w.talk("ishtar", "medaillon");
    w.talk("ishtar", "bas_relief");
    w.talk("ishtar", "partie_poker");
    assert!(w.has("money_for_poker"));
    // The gorilla: the money shown, the paper's password (1, 8, 12).
    w.use_on_sprite("money_for_poker", "macao_gorilla-guard");
    w.talk("macao_gorilla-guard", "partie_poker");
    for mdp in ["mdp_01", "mdp_08", "mdp_12"] {
        w.talk("macao_gorilla-guard", mdp);
    }
    assert!(w.has("winch_code"));
    // The winches, to the states Atl_PuzzleWinch_Check asks (3, −4, 2).
    w.talk("ishtar", "partie_poker");
    assert_eq!(w.address().1, 18);
    for (winch, target) in [(1, 3), (2, -4), (3, 2)] {
        loop {
            let now = num(&w.lua(&format!("atl.puzzle.winch.states[{winch}]")));
            if now == target {
                break;
            }
            let way = if now < target {
                "winch_right"
            } else {
                "winch_left"
            };
            w.empty_hand();
            w.click_zone(winch, way);
        }
    }
    assert_eq!(w.state("veil"), "up");
    // The poker room: the three players' subjects, the seat, the game.
    w.touch_zone("poker_room_in");
    w.talk("ishtar-poker", "partie_poker");
    w.touch_zone("poker_seat");
    w.talk("punter-poker", "parieur");
    w.talk("fisherman_boss", "patron_pecheur");
    w.talk("elliot", "elliot");
    w.talk_open("punter-poker", "commencer_poker");
    w.play_poker();
    assert_eq!(w.state("ishtar"), "kidnapped");
    // Sing like Rain.
    w.talk("sing_like_rain", "ishtar");
    w.talk("sing_like_rain", "sing_like_rain");
    w.talk("sing_like_rain", "medaillon");
    w.talk("sing_like_rain", "ishtar");
}

/// The buttons of a 3 × 5 lights-out grid (`Atl_PuzzleLightOut_OnClicked`'s
/// neighbours: ±5, +1 unless at a row's end, −1 unless at its start) that
/// turn every light off from all on: the first subset found.
fn light_out_solution() -> Vec<i32> {
    let toggles = |b: i32| -> u32 {
        let mut m = 1 << (b - 1);
        if b - 5 > 0 {
            m |= 1 << (b - 6);
        }
        if b + 5 < 16 {
            m |= 1 << (b + 4);
        }
        if b % 5 != 0 {
            m |= 1 << b;
        }
        if b % 5 != 1 {
            m |= 1 << (b - 2);
        }
        m
    };
    (1u32..1 << 15)
        .map(|set| {
            let presses: Vec<i32> = (1..=15).filter(|b| set & (1 << (b - 1)) != 0).collect();
            let lit = presses.iter().fold(0x7fff, |g, &b| g ^ toggles(b));
            (presses, lit)
        })
        .find(|(_, lit)| *lit == 0)
        .map(|(p, _)| p)
        .expect("a solution")
}

/// The sudoku's answer: the grid `DoSudoku` (the scripts' own debug
/// shortcut) sets.
const SUDOKU: [i32; 81] = [
    7, 3, 2, 4, 9, 8, 5, 1, 6, 4, 5, 1, 7, 6, 2, 9, 3, 8, 9, 6, 8, 5, 1, 3, 4, 7, 2, 6, 7, 4, 3, 8,
    1, 2, 9, 5, 1, 9, 5, 2, 4, 7, 8, 6, 3, 8, 2, 3, 6, 5, 9, 1, 4, 7, 3, 1, 7, 8, 2, 4, 6, 5, 9, 5,
    8, 9, 1, 3, 6, 7, 2, 4, 2, 4, 6, 9, 7, 5, 3, 8, 1,
];

/// Chapter 8, the zeppelin over Ishtar's palace (`chapter_08.lua`,
/// `on_click_ishtar.lua`): the sanskrit copied with the zeppelin's pencil
/// and paper and translated by Ishtar; the five stones (the book's
/// sapphire, the Vishnu chest's emerald, the lights-out statuette's ruby,
/// the elephant's pearl cut with the knife, Sing like Rain's moonstone
/// for it) in the light ray's slots; the bas-relief taken (Sing like
/// Rain's betrayal), the sudoku, the prong that opens Ishtar's cage; the
/// bas-relief code with Ishtar on the flagstone, the fresco's talk
/// (Mesopotamia), back to the zeppelin: chapter 9.
fn chapter_8(w: &mut Walker) {
    w.touch("zeppelin_pencil");
    w.touch("zeppelin_sheet_of_paper");
    w.combine("zeppelin_pencil", "zeppelin_sheet_of_paper");
    assert!(w.has("zeppelin_paper_and_pencil"));
    w.touch("zeppelin_knife");
    w.go_to("zeppelin_exit", "ishtar");
    w.use_on_zone("zeppelin_paper_and_pencil", "sanscrit_inscription");
    w.use_on_sprite("sanscrit_text", "ishtar-captive");
    assert!(w.has("translated_sanscrit_text"));
    // The stones.
    w.touch("sapphire_in_book");
    w.touch("sapphire_in_book");
    assert!(w.has("sapphire"));
    for (arm, target) in [(1, 4), (2, 4), (3, 2), (4, 4)] {
        while num(&w.lua(&format!("atl.puzzle.vishnu_chest.arm_state[{arm}]"))) != target {
            w.touch(&format!("vishnu_arm_{arm}"));
        }
    }
    assert_eq!(w.state("ishtar_palace_chest_lid"), "open");
    w.touch("emerald");
    w.walk(18);
    for b in light_out_solution() {
        w.empty_hand();
        w.click_zone(b, "light_out");
    }
    assert_eq!(w.state("vishnu_statuette"), "open");
    w.touch("ruby");
    w.use_on_sprite("zeppelin_knife", "perl");
    assert!(w.has("perl"));
    w.use_on_sprite("perl", "sing_like_rain-read_library");
    assert!(w.has("moon_stone"));
    for ivy in ["emerald_ivy", "sapphire_ivy", "ruby_ivy"] {
        w.use_on_sprite("zeppelin_knife", ivy);
    }
    for (slot, stone) in [
        (1, "emerald"),
        (2, "sapphire"),
        (3, "ruby"),
        (4, "moon_stone"),
    ] {
        w.use_on_zone(stone, &format!("gem_slot_{slot}"));
    }
    assert_eq!(w.flag("mission_light_ray"), "done");
    // The bas-relief taken: the betrayal, view 30 and the sudoku.
    w.touch("column_niche_door");
    w.touch("ishtar_palace_bas_relief");
    assert_eq!(w.address().1, 30);
    for (i, &v) in SUDOKU.iter().enumerate() {
        let case = i32::try_from(i + 1).expect("case");
        while num(&w.lua(&format!("atl.puzzle.sudoku.grid[{case}]"))) != v {
            w.empty_hand();
            w.click_zone(case, "sudoku");
        }
    }
    assert_eq!(w.address().1, 27);
    w.touch("prong");
    w.use_on_sprite("prong", "cage_mecanism");
    assert_eq!(w.state("ishtar"), "free");
    w.talk("ishtar-captive", "porte_de_la_verite");
    w.talk("ishtar-captive", "sing_like_rain");
    assert_eq!(w.state("ishtar"), "follow_howard");
    // The bas-relief's code with Ishtar on the flagstone.
    w.touch_zone("info_bas_relief");
    for part in [1, 2, 2, 2, 1, 1] {
        w.empty_hand();
        w.click_zone(part, "bas_relief");
    }
    assert_eq!(w.state("ishtar_palace_door_2"), "open");
    w.touch_zone("ish_pal_cavern_in_1");
    w.touch_zone("fresque");
    for subject in ["atlantide", "medaillon", "prophetie"] {
        w.talk("ishtar-front_fresque", subject);
    }
    assert_eq!(w.flag("know_mesopotamia"), "true");
    w.touch_zone("ish_pal_zeppelin_in");
}

/// The bees to move, from `slots` (0 the hole), until each side holds
/// the other's (`Atl_PuzzleBees`' `RefreshActivity` rule: a bee of the
/// left, numbered below `center`, steps or jumps right into the hole; one
/// of the right left), by a depth-first walk.
fn bees_line(slots: &[i32], center: i32) -> Option<Vec<i32>> {
    let n = slots.len();
    let hole = slots.iter().position(|&b| b == 0)?;
    let mut moves = Vec::new();
    for (c, right) in [
        (hole.wrapping_sub(2), true),
        (hole.wrapping_sub(1), true),
        (hole + 1, false),
        (hole + 2, false),
    ] {
        if c < n && slots[c] != 0 && ((slots[c] < center) == right) {
            moves.push(c);
        }
    }
    let solved = (0..n).all(|s| {
        let s1 = i32::try_from(s + 1).expect("slot");
        if s1 < center {
            slots[s] >= center
        } else if s1 > center {
            slots[s] != 0 && slots[s] < center
        } else {
            true
        }
    });
    if moves.is_empty() {
        return solved.then(Vec::new);
    }
    for c in moves {
        let mut t = slots.to_vec();
        t.swap(c, hole);
        if let Some(mut rest) = bees_line(&t, center) {
            rest.insert(0, slots[c]);
            return Some(rest);
        }
    }
    None
}

/// The trap's buttons to press, from step 4 up, until steps 2..=10 are
/// up: a breadth-first walk of `Atl_PuzzleTrap_OnClick`'s rule (as in
/// `atlantis5_puzzles.rs`).
fn trap_solution() -> Vec<usize> {
    const ACTIONS: [&[usize]; 11] = [
        &[],
        &[2, 3],
        &[3, 6, 9],
        &[5, 8],
        &[6, 8, 10],
        &[3, 9],
        &[2, 4, 7],
        &[3, 10],
        &[4, 5, 7],
        &[4, 10],
        &[2, 5, 9],
    ];
    let up = |s: u16, i: usize| s & (1 << i) != 0;
    let start: u16 = 1 << 4;
    let goal = |s: u16| (2..=10).all(|i| up(s, i));
    let mut prev = HashMap::from([(start, (start, 0))]);
    let mut queue = VecDeque::from([start]);
    while let Some(s) = queue.pop_front() {
        if goal(s) {
            let mut out = Vec::new();
            let mut x = s;
            while x != start {
                let (p, b) = prev[&x];
                out.push(b);
                x = p;
            }
            out.reverse();
            return out;
        }
        for b in (1..=10).filter(|&b| up(s, b)) {
            let mut t = s & !(1 << b);
            if b == 1 {
                t |= (1 << 2) | (1 << 3);
            } else {
                for &a in ACTIONS[b] {
                    t ^= 1 << a;
                }
                if (4..=10).all(|i| up(t, i)) {
                    if up(t, 1) {
                        continue;
                    }
                    t |= 1 << 1;
                }
            }
            if let std::collections::hash_map::Entry::Vacant(e) = prev.entry(t) {
                e.insert((s, b));
                queue.push_back(t);
            }
        }
    }
    panic!("no solution");
}

/// The hex player's search of `atlantis5_puzzles.rs`: run in the game's
/// own Lua state on the scripts' own `Atl_PuzzleHex_OnClick` (the
/// computer's answer included, its `math.random` replayed from the state
/// `seed`), a beam over the player's resistance (`resistance_board_2`);
/// the board, the generator and the zones put back. Returns "x,y;..." or
/// nil.
const HEX_SEARCH: &str = r#"
local seed, beam, depth = ...
local hx = atl.puzzle.hex
local function cp(b) local r = {} for y = 1, 7 do r[y] = {} for x = 1, 7 do r[y][x] = b[y][x] end end return r end
local function min2() local m for i = 1, 7 do local v = hx.resistance_board_2[1][i] if type(v) == "number" and (m == nil or m > v) then m = v end end return m end
local function min1() local m for i = 1, 7 do local v = hx.resistance_board_1[i][7] if type(v) == "number" and (m == nil or m > v) then m = v end end return m end
local function next_seed(s) return math.mod(s * 214013 + 2531011, 4294967296) end
local start, start_seed = cp(hx.board_state), seed
local function rec(board, s, d, path)
  local cands, n = {}, 0
  for y = 1, 7 do for x = 1, 7 do if board[y][x] == 0 then
    hx.board_state = cp(board)
    math.randomseed(s)
    Atl_PuzzleHex_OnClick(29, x, y)
    local h, c = min2(), min1()
    local s2 = s
    if h ~= nil and c ~= nil then s2 = next_seed(s) end
    local p = path .. x .. "," .. y .. ";"
    if h == 0 then return p end
    Atl_PuzzleHex_CalculateResistanceBoard(1, hx.resistance_board_1)
    local c2 = min1()
    if c2 ~= 0 and h ~= nil then
      n = n + 1
      cands[n] = { h = h, c = c2 or 99, board = cp(hx.board_state), s = s2, p = p }
    end
  end end end
  if d == 0 then return nil end
  for i = 2, n do
    local v, j = cands[i], i - 1
    while j >= 1 and (cands[j].h > v.h or (cands[j].h == v.h and cands[j].c < v.c)) do cands[j + 1] = cands[j] j = j - 1 end
    cands[j + 1] = v
  end
  for i = 1, math.min(beam, n) do
    local r = rec(cands[i].board, cands[i].s, d - 1, cands[i].p)
    if r then return r end
  end
  return nil
end
local r = rec(start, seed, depth, "")
hx.board_state = start
math.randomseed(start_seed)
for y = 1, 7 do for x = 1, 7 do Atl_Sprite_Stop("hex_pawn_" .. string.sub("abcdefg", x, x) .. y) end end
Atl_PuzzleHex_SetActivityZone()
return r
"#;

impl Walker {
    /// The zone named `name` of this view (its number: the first active).
    fn zone_here(&mut self, name: &str) -> i32 {
        self.play
            .zones()
            .into_iter()
            .find(|z| z.1 == name.as_bytes())
            .map(|z| z.0)
            .unwrap_or_else(|| panic!("{name} active in {:?}", self.address()))
    }
}

/// Chapter 9, Mesopotamia (`chapter_09.lua`, `on_click_mesopotamie.lua`):
/// Ishtar's talk opens the village; her Nisir language makes the
/// villagers speak (the herbalist's cataplasm for the pearl, the farmer's
/// honey bread for the hermit); the bees smoked and their hive solved
/// give the honey; the hermit sees the medallion and asks for aloes (the
/// snake kept off by Ishtar and the cataplasm), whose remedy heals Sing
/// like Rain. The little girl freed from the taulas, her doll fished from
/// the well, tells where her uncle's key is: the sculptor's house (the
/// madder ground, sieved and blended into paints, the stones cut). In
/// Marduk's temple the stones set and painted give the Atlantean crystal;
/// the engraved prong opens the trap room (the gridlock), the medallion
/// on its post the dark trap (Ishtar's scarf lit with the matches), the
/// ring and the buttons the way up; the bull's horns lead to the ark,
/// whose hex game won ends the game (film cine06, `Atl_TerminateGame`).
fn chapter_9(w: &mut Walker) {
    for subject in ["atlantide", "elliot", "sing_like_rain"] {
        w.talk("ishtar-stand_near_zeppelin", subject);
    }
    assert_eq!(w.state("ishtar"), "follow_howard");
    // The village: the herbalist (the language, the cataplasm).
    w.talk("herbalist-searching_for_herbs", "marchand");
    assert_eq!(w.flag("know_nisir_language"), "true");
    w.talk("herbalist-searching_for_herbs", "marchand");
    w.talk("herbalist-searching_for_herbs", "plantes");
    w.use_on_sprite("perl", "herbalist-searching_for_herbs");
    assert!(w.has("cataplasm"));
    w.talk("herbalist-searching_for_herbs", "sing_like_rain");
    w.talk("herbalist-searching_for_herbs", "ermite");
    // The little girl under the taulas, the sculptor's closed door.
    w.touch("iron_bar");
    w.use_on_sprite("iron_bar", "taulas");
    w.talk("little_girl-free_near_taulas", "petite_fille");
    assert_eq!(w.flag("know_uncle_samal"), "true");
    w.touch("sculptor_house_door");
    // The farmer: the hermit's honey bread, the matches.
    w.talk("farmer-deal_with_fire", "fermiere");
    w.talk("farmer-deal_with_fire", "ermite");
    w.talk("farmer-deal_with_fire", "pain_au_miel");
    w.touch("match_box");
    // The bees: the smoker lit, the swarm away, the three trays.
    w.touch("bee_smoker");
    w.combine("bee_smoker_off", "match_box");
    assert_eq!(w.hand(), "bee_smoker_on");
    w.sprite("hive_lid");
    assert_eq!(w.state("bees_cloud"), "away");
    w.touch("hive_lid");
    for level in 1..=3 {
        assert_eq!(num(&w.lua("atl.puzzle.bees.level")), level);
        let nb = num(&w.lua(&format!("atl.puzzle.bees.nb_bees[{level}]")));
        let slots: Vec<i32> = (1..=2 * nb + 1)
            .map(|s| num(&w.lua(&format!("atl.puzzle.bees.slots[{s}]"))))
            .collect();
        for b in bees_line(&slots, nb + 1).expect("a solution") {
            w.hold("bee_smoker_on");
            assert!(w.click_sprite_here(&format!("bee_{level}{b}")), "bee {b}");
        }
        w.touch(&format!("hive_level_{level}"));
    }
    assert!(w.has("hive_level_3"));
    w.use_on_sprite("hive_level_3", "farmer-deal_with_fire");
    // The hermit comes after 45 s (the timer prophet_arrived).
    w.wait(47.0);
    assert_eq!(w.state("prophet"), "eat_bread");
    w.use_on_sprite("medallion", "prophet-eat_bread");
    // Sing like Rain lies in the abandoned house (its door a sprite).
    w.touch("abandoned_house_door");
    w.touch("sing_like_rain-sick");
    w.talk("prophet-eat_bread", "ermite");
    w.talk("prophet-eat_bread", "sing_like_rain");
    assert_eq!(w.state("little_girl_doll"), "in_well");
    // The aloes (Ishtar and the cataplasm keep the snake off), the remedy.
    w.talk("herbalist-searching_for_herbs", "aloes_pour_remede");
    w.touch("aloes");
    assert!(w.has("aloes"));
    w.use_on_sprite("aloes", "prophet-eat_bread");
    w.talk("prophet-eat_bread", "remede");
    w.use_on_sprite("remedy", "sing_like_rain-sick");
    assert_eq!(w.state("sing_like_rain"), "healed");
    for subject in [
        "sing_like_rain",
        "temple",
        "elliot",
        "sing_like_rain",
        "medaillon",
    ] {
        w.talk("sing_like_rain-healed", subject);
    }
    assert_eq!(w.state("sing_like_rain"), "follow_howard");
    // The doll: the wire cut with the torture, the bucket's handle, the
    // well.
    w.touch("torture");
    w.use_on_zone("torture", "nisir_wire");
    w.touch("bucket");
    w.combine("bucket", "nisir_wire");
    assert_eq!(w.hand(), "bucket_with_handle");
    w.use_on_sprite("bucket_with_handle", "well");
    w.touch("well");
    w.wait(5.0);
    w.touch("well");
    assert!(w.has("little_girl_doll"));
    w.use_on_sprite("little_girl_doll", "little_girl-near_well");
    w.talk("little_girl-near_well", "porte_fermee");
    // The uncle's key, the sculptor's house.
    w.touch("board_and_key");
    w.touch("board_and_key");
    w.use_on_sprite("sculptor_house_key", "sculptor_house_door");
    w.touch("sculptor_house_door");
    assert_eq!(w.state("sculptor_house_door"), "open");
    // The madder: dug, ground, sieved, refined, in the blender, lit.
    w.touch("hoe");
    w.use_on_sprite("hoe", "garance");
    w.touch("garance");
    w.touch("pestle");
    w.use_on_zone("garance_root", "mortar");
    w.use_on_sprite("pestle", "garance_root");
    w.touch("garance_root");
    assert!(w.has("smached_garance"));
    w.touch("sieve");
    w.use_on_zone("sieve", "sieve_place");
    w.use_on_sprite("smached_garance", "sieve");
    w.touch("garance_root");
    w.wait(5.0);
    w.touch("garance_root");
    w.use_on_zone("refined_garance", "blender_input");
    w.use_on_zone("match_box", "blender_light_zone");
    // The four paints: both valves open light orange, the left dark
    // orange, the right light blue, none dark blue.
    let (mut left, mut right) = (false, false);
    let mut paints = HashMap::new();
    for (l, r, name, colour) in [
        (true, true, "paint_light_orange", "lo"),
        (true, false, "paint_dark_orange", "do"),
        (false, true, "paint_light_blue", "lb"),
        (false, false, "paint_dark_blue", "db"),
    ] {
        if l != left {
            w.touch("nisir_left_valve");
            left = l;
        }
        if r != right {
            w.touch("nisir_right_valve");
            right = r;
        }
        w.touch("nisir_main_valve");
        w.touch(name);
        let held = w.hand();
        assert!(held.ends_with(colour), "{held}");
        paints.insert(colour, held);
    }
    // The stones cut to the holes' sizes (3, 5, 8, 8).
    w.touch("sculptor_tools");
    let sizes = [3, 5, 8, 8];
    for (n, size) in (1..=4).zip(sizes) {
        w.touch("gross_stone_heap");
        assert_eq!(w.hand(), format!("stone{n}_1"));
        w.use_on_zone(&format!("stone{n}_1"), "cut_zone");
        for _ in 1..size {
            w.use_on_sprite("sculptor_tools", "gross_stone");
        }
        w.touch("gross_stone");
        assert_eq!(w.hand(), format!("stone{n}_{size}"));
    }
    // Marduk's temple: the stones set and painted, the medallion on the
    // eye: the crystal.
    for (n, size) in (1..=4).zip(sizes) {
        w.use_on_zone(&format!("stone{n}_{size}"), &format!("marduk_stone_{n}"));
    }
    for (n, colour) in [(1, "db"), (2, "lo"), (3, "lb"), (4, "do")] {
        let paint = paints[colour].clone();
        w.use_on_sprite(&paint, &format!("marduk_stone_{n}"));
    }
    assert_eq!(w.lua("Atl_PuzzleStonePainting_Check()"), Value::Bool(true));
    w.use_on_zone("medallion", "marduk_eye");
    assert_eq!(w.hand(), "atlantean_cristal");
    w.touch_zone("marduk_eye");
    assert_eq!(w.hand(), "medallion");
    // The engraved prong turned by the medallion: the trap room's door.
    w.use_on_zone("medallion", "engraved_prong_1");
    w.use_on_zone("medallion", "engraved_prong_2");
    assert_eq!(w.state("nisir_trap_room_door"), "open");
    // The gridlock, set by the crystal (it comes back a second later).
    w.use_on_zone("atlantean_cristal", "gridlock_reset");
    w.wait(2.0);
    assert_eq!(w.hand(), "atlantean_cristal");
    w.empty_hand();
    for z in [
        3, 17, 11, 22, 15, 8, 9, 21, 14, 15, 19, 13, 7, 20, 14, 32, 27, 22, 21, 20, 27, 28, 32, 33,
        34, 27, 20, 13, 2, 35, 28, 21, 14, 9, 20, 15, 26, 21, 28, 22, 16, 10, 15, 16, 29, 24, 17,
        18,
    ] {
        w.click_zone(z, &format!("gridlock_{z}"));
    }
    assert_eq!(w.lua("Atl_PuzzleGridlock_IsSolved()"), Value::Bool(true));
    // The trap room: Elliot's backpack, the medallion on its post.
    w.touch_zone("trap_room_in");
    w.touch("elliot_backpack");
    assert!(w.has("diary_page"));
    w.touch("false_medallion");
    w.use_on_zone("medallion", "medallion_post");
    w.wait(2.0);
    assert_eq!(w.address().1, 22);
    // The dark trap: Ishtar's scarf on the torch, lit.
    w.touch_zone("trap_torch");
    w.talk("ishtar-inside_dark_trap_scarf", "echarpe");
    w.wait(2.0);
    assert_eq!(w.hand(), "scarf");
    w.use_on_zone("scarf", "trap_torch");
    w.use_on_zone("match_box", "trap_torch");
    w.wait(4.0);
    assert_eq!(w.address().1, 23);
    // The ring pulled thrice, the buttons.
    for _ in 0..3 {
        w.touch("trap_ring");
    }
    for b in trap_solution() {
        w.touch(&format!("trap_button_{b}"));
    }
    assert_eq!(w.lua("Atl_PuzzleTrap_IsSolved()"), Value::Bool(true));
    w.touch_zone("trap_upstairs");
    w.use_on_sprite("iron_bar", "bull_horns");
    assert_eq!(w.address().1, 25);
    // The ark: the crystal on its console, the hex game.
    w.use_on_zone("atlantean_cristal", "ark_crystal_place");
    // Each move searched again from the board and the generator as they
    // are (the scene's timers draw from the same `math.random`).
    for _ in 0..49 {
        let seed = w.play.game.script().lua_holdrand();
        let found = w
            .play
            .game
            .script()
            .one_shot(
                &format!("return (function(...) {HEX_SEARCH} end)({seed}, 3, 12)"),
                None,
                1,
            )
            .expect("the search")
            .remove(0);
        let Value::Str(line) = found else {
            panic!("no winning line: {found:?}")
        };
        let line = String::from_utf8(line).expect("text");
        let moves: Vec<&str> = line.split(';').filter(|m| !m.is_empty()).collect();
        let (x, y) = moves[0].split_once(',').expect("x,y");
        let name = format!("hex_pawn_{x}{y}");
        let id = w.zone_here(&name);
        if moves.len() > 1 {
            w.click_zone(id, &name);
        } else {
            // The last pawn: the crystal lit, a second later the last
            // film and the end of the game.
            assert!(w.play.aim_at_zone_named(id, name.as_bytes()));
            w.play.click();
            return;
        }
    }
    panic!("the hex game not won");
}

/// The chapters' plays, in order.
const CHAPTERS: [fn(&mut Walker); 9] = [
    chapter_1, chapter_2, chapter_3, chapter_4, chapter_5, chapter_6, chapter_7, chapter_8,
    chapter_9,
];

/// The end, after the hex game's last pawn: the timer `atlantean_crystal`
/// (1 s) plays cine06 and calls `Atl_TerminateGame`; the manager's state 7
/// deletes the autosave, ends the game and shows the credits.
fn the_end(w: &mut Walker) {
    let mut credits = false;
    for _ in 0..30 * 30 {
        w.frames(1);
        if w.play.menus.current() == Some(atlantis3_engine::atlantis5_menu::CREDITS) {
            credits = true;
            break;
        }
    }
    assert!(credits, "the credits (state {})", w.play.game.state());
    assert_eq!(w.play.game.state(), 3);
    w.frames(30 * 10);
    w.shot("a5-the-end-credits.png");
    assert_eq!(
        w.play.menus.current(),
        Some(atlantis3_engine::atlantis5_menu::CREDITS)
    );
}

/// A new game played through chapter `n`: the chapters before it set
/// what it needs (the characters' subjects, the flags), as in the game.
fn play_chapter(n: usize) {
    let Some(d) = dvd() else { return };
    let mut w = Walker::new_game(&d);
    for (i, body) in CHAPTERS.iter().enumerate().take(n) {
        let c = u32::try_from(i + 1).expect("chapter");
        assert_eq!(w.chapter_now(), c);
        let before = w.steps;
        body(&mut w);
        assert_eq!(w.chapter_now(), c + 1, "chapter {c} ends in the next");
        eprintln!("chapter {c}: {} clicks", w.steps - before);
    }
}

#[test]
#[ignore = "a tool: prints the scenes' views and zones"]
fn dump_scenes() {
    let Some(d) = dvd() else { return };
    let mut w = Walker::new(&d);
    for scene in std::env::var("A5_SCENES").unwrap_or_default().split(',') {
        let wam = w.wam(scene).clone();
        for p in &wam.places {
            for v in &p.views {
                let zones: Vec<String> = v
                    .zones
                    .iter()
                    .filter(|z| z.kind < 1000)
                    .map(|z| format!("{}:{}", z.name.clone().unwrap_or_default(), z.value))
                    .collect();
                println!("{scene} {} {}: {}", v.id, v.name, zones.join(" "));
            }
        }
    }
}

#[test]
fn chapter_01() {
    play_chapter(1);
}

#[test]
fn chapter_02() {
    play_chapter(2);
}

#[test]
fn chapter_03() {
    play_chapter(3);
}

#[test]
fn chapter_04() {
    play_chapter(4);
}

#[test]
fn chapter_05() {
    play_chapter(5);
}

#[test]
fn chapter_06() {
    play_chapter(6);
}

#[test]
fn chapter_07() {
    play_chapter(7);
}

#[test]
fn chapter_08() {
    play_chapter(8);
}

/// A game over: in chapter 3 Elmore accused twice of Harry's groom
/// business (`_gameover_groom`, on_click_etage0.lua): the death panel,
/// then `Atl_Load` of the autosave chapter 3's start wrote.
#[test]
fn a_game_over_loads_the_autosave() {
    let Some(d) = dvd() else { return };
    let mut w = Walker::new_game(&d);
    chapter_1(&mut w);
    chapter_2(&mut w);
    assert_eq!(w.chapter_now(), 3);
    let start = w.address();
    w.talk("journalist", "journaliste");
    w.talk("journalist", "vedette");
    w.talk("hostess", "vedette");
    w.talk("hostess", "drague");
    w.talk("elmore", "vedette");
    w.talk("elmore", "groom_harry");
    assert_eq!(w.flag("harry_accused"), "true");
    w.talk("elmore", "vedette");
    // The second accusation: the subject's script ends on the panel.
    w.empty_hand();
    assert!(w.play.aim_at_sprite(b"elmore"));
    w.play.click();
    let mut subjects = Vec::new();
    for _ in 0..600 {
        w.frames(1);
        subjects = w.play.stage().menu_subjects();
        if !subjects.is_empty() {
            break;
        }
    }
    let i = subjects
        .iter()
        .position(|s| s == b"groom_harry")
        .expect("groom_harry");
    let (x, y) = w.play.stage().menu_subject_point(i).expect("subject");
    w.play.mouse_at(x, y);
    w.play.click();
    for _ in 0..30 * 60 {
        w.frames(1);
        if w.play.menus.current() == Some(atlantis3_engine::atlantis5_menu::DEATH) {
            break;
        }
    }
    assert_eq!(
        w.play.menus.current(),
        Some(atlantis3_engine::atlantis5_menu::DEATH)
    );
    w.shot("a5-game-over.png");
    assert!(w.play.click_menu("button"));
    // The panel ends; the queued load (state 2) reads the autosave.
    w.settle();
    assert_eq!(w.chapter_now(), 3);
    assert_eq!(w.flag("harry_accused"), "false");
    assert_eq!(w.address(), start);
}

/// The whole game: the nine chapters from a new game, the last film,
/// `Atl_TerminateGame` and the credits.
#[test]
fn whole_game() {
    let Some(d) = dvd() else { return };
    let mut w = Walker::new_game(&d);
    for (i, body) in CHAPTERS.iter().enumerate() {
        let c = u32::try_from(i + 1).expect("chapter");
        assert_eq!(w.chapter_now(), c);
        let before = w.steps;
        body(&mut w);
        eprintln!("chapter {c}: {} clicks", w.steps - before);
        if c < 9 {
            assert_eq!(w.chapter_now(), c + 1, "chapter {c} ends in the next");
        }
    }
    the_end(&mut w);
}
