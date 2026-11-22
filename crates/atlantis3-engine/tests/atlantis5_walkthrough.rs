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
//! last one until `Atl_TerminateGame` and the credits. Puzzles solved in
//! `atlantis5_puzzles.rs` are solved here the same way.
//!
//! Each chapter starts from `Atl_InitChapter(n)` (the scripts set every
//! object, flag and item of a chapter there); `whole_game` plays them all
//! in one run from a new game.

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
        let play = Play::new(src.clone(), "french", None).expect("play");
        Walker {
            play,
            src,
            wams: HashMap::new(),
            cell0: None,
            right_button: None,
            steps: 0,
            avoid: vec!["lift_floor".into(), "stairway_".into()],
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

    /// A new game, then chapter `n` started by the scripts' own
    /// `Atl_InitChapter`.
    fn chapter(d: &Path, n: u32) -> Walker {
        let mut w = Walker::new_game(d);
        if n != 1 {
            w.run(&format!("Atl_InitChapter({n})"));
        }
        assert_eq!(w.chapter_now(), n);
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

    /// Runs `body` as a script run (its calls may wait) and settles.
    fn run(&mut self, body: &str) {
        assert!(
            self.play
                .game
                .run_string(&format!("return function() {body} end")),
            "{body}"
        );
        self.settle();
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

    /// Clicks this view's move zone without a name toward `to` (the view's
    /// script may send elsewhere).
    fn click_move(&mut self, to: i32) {
        self.click_zone(to, "");
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

    /// Goes to the nearest view with the zone `name` and clicks it.
    fn zone(&mut self, name: &str) {
        self.zone_with(name, None);
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

    /// Clicks zone `name` in `view` (several views may hold it).
    fn zone_in(&mut self, view: i32, name: &str) {
        self.walk(view);
        let id = self
            .views_with_zone(name)
            .into_iter()
            .find(|z| z.0 == view)
            .unwrap_or_else(|| panic!("zone {name} in view {view}"))
            .1;
        self.click_zone(id, name);
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

    /// Clicks the sprite of `object` in `view`.
    fn sprite_in(&mut self, view: i32, object: &str) {
        self.walk(view);
        assert!(
            self.click_sprite_here(object),
            "sprite {object} in {:?}",
            self.address()
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
            let (rx, ry) = self.right_button.expect("the right button");
            self.play.mouse_at(rx, ry);
            self.play.click();
            self.frames(1);
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
        // the script, or a chapter started).
        for _ in 0..30 * 600 {
            self.frames(1);
            if !self.play.game.busy()
                && !self.play.stage().dialog_running()
                && (!self.play.stage().menu_subjects().is_empty()
                    || !self.play.stage().menu_open_now())
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

/// The chapters' plays, in order.
const CHAPTERS: [fn(&mut Walker); 6] = [
    chapter_1, chapter_2, chapter_3, chapter_4, chapter_5, chapter_6,
];

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
