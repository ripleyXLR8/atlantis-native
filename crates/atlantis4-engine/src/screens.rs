//! The in-game screens around the warp that the inventory and the
//! documents need (atlantis4-gog-en-2363910): the HUD of `GUI/InGame.lua`
//! (the inventory button with the selected object, the on-screen sprites
//! of `LoadSprite`), the inventory menu (`InventoryMenu/InventoryMenu.lua`
//! over `Inventory/Inventory.lua`), the shown document (`zoomed` of
//! `DocumentsBrowser/DocumentsBrowser.lua`) and the object notification
//! of `GUI/Notify.lua`. Every layout comes from the install's files
//! through the GUI base [`crate::gui`] (geometry, pictures, the text
//! layouts' glyphs, the colour animations).
//!
//! Draw order (`Game::load` @ 0x00528c10 inserts the screens so): the
//! HUD (`inGameSprites`, then `inventoryButton`), the inventory menu
//! (its children in the loader's order, 0x0057cdc0: the inventory, the
//! documents browser, the menu's own `inventoryMenu`, the diary, then
//! `zoomed`, 0x00553690), the dialog and the answers ([`crate::dialog`]),
//! then the notification.
//!
//! **Opening** (`STATIC`): the HUD's `inventoryButton` → 0x00524cc0
//! (`Sounds/SFX/ChoixMenu.wav` at 1.0) → `InventoryMenu::enter`
//! 0x0057c830 (warp input off, `App+0x7a10 = 0`; `inventoryMenu` shown)
//! → the inventory tab 0x0057c300 → `Inventory::enter` 0x0055fcb0 (the
//! input flag saved, the inventory shown on page 0, the selection
//! re-selected without its callback). **Closing**: the menu's
//! `quitButton` (0x0057cc40 → 0x0057c930: the inventory's drags ended
//! and hidden, the document browser and diary hidden, `inventoryMenu`
//! hidden, warp input on) or a double click on an object; the
//! inventory's `exit` button (0x0055fd30) hides only the inventory and
//! restores the saved flag (still off), leaving the menu up: it lies at
//! y 800/768 of the picture, so only its top rows are on screen.
//! `mainMenuButton` (0x0057cc50) saves `save.xml` and goes to the main
//! menu (M4: logged).
//!
//! **Buttons** (@ 0x004e5030 press, 0x004e52c0 validation): a click validates on a release inside the
//! button the press went to; the validation sound plays then. A visible
//! button swallows the press; the inventory's full-window `inventory`
//! button and `zoomed` swallow every click under them.
//!
//! **State layouts first** `[INFERRED]`: a button's `upLayout` is drawn
//! under its other children (the inventory's black `upLayout` would
//! hide the whole screen otherwise; `setUpLayout` @ 0x004e4ce0 calls the
//! up layout's slot `+0x4c` with 0, not walked), as [`crate::gui`] draws.
//!
//! **Drag** (0x0054da90 press, 0x0054d730 move, 0x0054d280 release,
//! 0x0055df70 drop): past `dragThreshold` pixels (`InventoryObject.lua`,
//! 25) from the press the object follows the pointer; released on an
//! empty slot of the page it moves there, else it goes back.
//! `[UNKNOWN]` where the pointer holds the object (taken: its centre)
//! and whether the drop's release also validates the object's click
//! (taken: not).
//!
//! **Notification** (`AddToBag` → 0x00568960, `STATIC`): names queue;
//! the next is shown when no fade runs (0x00568470): its text
//! (`Arial_r_16`, black, centred, then the name) in `text`, the `notify`
//! button shown, `fadeIn` (500 ms) played on `sprite`, then `visible`
//! (3000 ms), then `fadeOut` (500 ms), whose end hides `notify` and shows
//! the next (0x00568260, 0x00568050, 0x00568a60). The colour animations
//! are the file's, run by [`crate::gui::anim`]; `text`, a child of
//! `sprite`, takes its colour (`[INFERRED]`, [`crate::gui`]).
//!
//! **Text**: the inventory's `text` (an extended text layout: the
//! selected object's name and description through `textFormat`, its
//! auto-scroll after `autoScrollDelay`) and the notification's `text` are
//! the GUI base's text layouts, drawn with the dialogs' glyph walker
//! ([`crate::gui::text`]).

use std::collections::{HashMap, VecDeque};

use atlantis4_data::xml::to_cp1252;
use atlantis4_data::A4Files;

use crate::gui::text::FontCache;
use crate::gui::{self, draw_picture, Gui, Id, Painter, State};
use crate::inventory::{Inventory, SlotRef};
use crate::layout::{percent_size, RatioMode, Rect};
use crate::package::Picture;

pub const IN_GAME_LUA: &str = "GUI/InGame.lua";
pub const INVENTORY_LUA: &str = "Inventory/Inventory.lua";
pub const OBJECT_LUA: &str = "Inventory/InventoryObject.lua";
pub const MENU_LUA: &str = "InventoryMenu/InventoryMenu.lua";
pub const DOCUMENTS_LUA: &str = "DocumentsBrowser/DocumentsBrowser.lua";
pub const NOTIFY_LUA: &str = "GUI/Notify.lua";
/// The HUD button's sound (0x00524cc0).
pub const HUD_SOUND: &str = "Sounds/SFX/ChoixMenu.wav";
/// The notification's text before the name (0x005284e0).
pub const NOTIFY_FORMAT: &str = "<section style=\"center\" /><color r=\"0\" g=\"0\" b=\"0\"/><font file=\"Common/Fonts/Arial_r_16.tef\" />";
/// An object's picture (0x004107f0: `Inventory/Objects/` + id + `.png`).
pub fn object_picture(id: &str) -> String {
    format!("Inventory/Objects/{id}.png")
}

/// What a press went to (top first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Notify,
    Zoomed,
    Quit,
    MainMenu,
    Object(SlotRef),
    Exit,
    /// The inventory's background, an empty slot: swallowed.
    Inventory,
    Hud,
}

/// What a validated click asks the game to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Play this sound once at 1.0 (a validation sound, a handler's).
    Sound(String),
    OpenMenu,
    CloseMenu,
    HideInventory,
    MainMenu,
    ObjectClicked(SlotRef),
    Dropped(SlotRef, SlotRef),
    ZoomedClicked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    from: SlotRef,
    at: (f64, f64),
    moving: bool,
}

/// The notification's three colour animations, in the order they play.
const PHASES: [&str; 3] = ["fadeIn", "visible", "fadeOut"];

/// The screens and their state.
pub struct Screens {
    files: A4Files,
    lang: String,
    pictures: HashMap<String, Option<Picture>>,
    fonts: FontCache,
    pub in_game: Gui,
    pub inventory: Gui,
    pub menu: Gui,
    pub documents: Gui,
    pub notify: Gui,
    /// `LoadSprite`'s files in load order (a file loaded again is added
    /// again: the list is cleared only by `FinishGame` and the game's
    /// end, 0x00525f50).
    pub sprites: Vec<(String, Gui)>,
    /// `inventoryMenu` shown.
    pub menu_shown: bool,
    /// The inventory shown.
    pub inventory_shown: bool,
    /// `App+0x7a10`: the warp takes input.
    pub warp_input: bool,
    /// The flag saved by the inventory's opening (`+0x814`).
    saved_input: bool,
    /// `dragThreshold` (`InventoryObject.lua`).
    drag_threshold: f64,
    /// The inventory's `textFormat`.
    pub text_format: String,
    /// `InventoryObject.lua`'s `object` button's `validationSound`.
    object_sound: Option<String>,
    press: Option<Target>,
    drag: Option<Drag>,
    queue: VecDeque<String>,
    /// The notification's animation playing (index into [`PHASES`]).
    phase: Option<usize>,
    /// The picture of the shown document, if any.
    pub zoomed_picture: Option<String>,
}

/// The screens' pictures for [`Gui::draw`].
struct Pics<'a>(&'a HashMap<String, Option<Picture>>);

impl Painter for Pics<'_> {
    fn picture(&self, path: &str) -> Option<&Picture> {
        self.0.get(path).and_then(Option::as_ref)
    }

    fn video(&self, _: &Gui, _: Id, _: &mut [u32], _: u32, _: u32, _: Rect) -> bool {
        false
    }
}

fn hide(g: &mut Gui, name: &str) {
    if let Some(i) = g.find(name) {
        g.node_mut(i).visible = false;
    }
}

impl Screens {
    /// The screens' layout files read from the install.
    pub fn load(files: &A4Files, lang: &str) -> Screens {
        let load = |p: &str| Gui::load(files, p).unwrap_or_default();
        let mut s = Screens {
            files: files.clone(),
            lang: lang.to_string(),
            pictures: HashMap::new(),
            fonts: FontCache::default(),
            in_game: load(IN_GAME_LUA),
            inventory: load(INVENTORY_LUA),
            menu: load(MENU_LUA),
            documents: load(DOCUMENTS_LUA),
            notify: load(NOTIFY_LUA),
            sprites: Vec::new(),
            menu_shown: false,
            inventory_shown: false,
            warp_input: true,
            saved_input: true,
            drag_threshold: 25.0,
            text_format: String::new(),
            object_sound: None,
            press: None,
            drag: None,
            queue: VecDeque::new(),
            phase: None,
            zoomed_picture: None,
        };
        let object = load(OBJECT_LUA);
        if let Some(t) = object.numbers.get("dragThreshold") {
            s.drag_threshold = *t;
        }
        s.object_sound = object
            .find("object")
            .and_then(|i| object.node(i).validation_sound.clone());
        s.text_format = s
            .inventory
            .text_format
            .as_ref()
            .map(|t| t.iter().map(|&c| char::from(c)).collect())
            .unwrap_or_default();
        // Shown by their owners only: the movie's layouts (`movie`), the
        // selection sprite (on the selected object), `notify` until a
        // notification plays.
        for n in ["videoBackgroundButton", "video", "skipVideoButton"] {
            hide(&mut s.in_game, n);
        }
        hide(&mut s.inventory, "selectionSprite");
        hide(&mut s.notify, "notify");
        // The fades drive `sprite` (0x00568b60 binds them).
        if let Some(sprite) = s.notify.find("sprite") {
            for a in PHASES {
                if let Some(i) = s.notify.find(a) {
                    s.notify.bind(i, sprite);
                }
            }
        }
        s
    }

    /// Loads the picture a logical path stands for (once).
    pub fn picture(&mut self, path: &str) -> Option<&Picture> {
        if !self.pictures.contains_key(path) {
            let pic = gui::resolve_in(&self.files, path, &self.lang)
                .and_then(|r| self.files.read(&r))
                .and_then(|d| gui::decode_picture(&d));
            if pic.is_none() {
                log::warn!("no picture {path}");
            }
            self.pictures.insert(path.to_string(), pic);
        }
        self.pictures.get(path).and_then(Option::as_ref)
    }

    /// The pictures the visible sprites of `root` in `g` show, loaded.
    fn load_pictures(&mut self, which: fn(&Screens) -> &Gui, root: &str) {
        let g = which(self);
        let mut paths = Vec::new();
        if let Some(r) = g.find(root) {
            g.pictures(r, &mut paths);
        }
        for p in paths {
            self.picture(&p);
        }
    }

    /// The slots per page of a file's `page<N>Slot<M>` layouts.
    pub fn slots(file: &Gui) -> Vec<usize> {
        let mut out = Vec::new();
        for p in 0.. {
            if file.find(&format!("page{p}")).is_none() {
                break;
            }
            let mut n = 0;
            while file.find(&format!("page{p}Slot{n}")).is_some() {
                n += 1;
            }
            out.push(n);
        }
        out
    }

    /// `LoadSprite(path)` (0x00527f70).
    pub fn load_sprite(&mut self, path: &str) {
        let f = Gui::load(&self.files, path).unwrap_or_default();
        if f.find("inGameSprite").is_none() {
            log::warn!("LoadSprite({path}): no inGameSprite");
        }
        self.sprites.push((path.to_string(), f));
    }

    /// `UnloadSprite(path)` (0x00525e10): the sprites of that file.
    pub fn unload_sprite(&mut self, path: &str) {
        self.sprites.retain(|(p, _)| p != path);
    }

    /// Every screen's geometry and texts for a `width × height` window
    /// (each root in the window, centred; the sprites in `inGameSprites`).
    fn layout(&mut self, width: u32, height: u32) {
        let win = (width as f32, height as f32);
        let Screens {
            files,
            fonts,
            in_game,
            inventory,
            menu,
            documents,
            notify,
            sprites,
            ..
        } = self;
        for (g, root) in [
            (&mut *in_game, "inGame"),
            (&mut *inventory, "inventory"),
            (&mut *menu, "inventoryMenu"),
            (&mut *documents, "zoomed"),
            (&mut *notify, "notify"),
        ] {
            let Some(r) = g.find(root) else { continue };
            g.update(r, win, (0.0, 0.0), None);
            g.layout_texts(files, fonts);
            // A text's size follows its block: placed again.
            g.update(r, win, (0.0, 0.0), None);
        }
        let (size, centre) = match in_game.find("inGameSprites") {
            Some(i) => (in_game.node(i).computed_size, in_game.node(i).world),
            None => (win, (0.0, 0.0)),
        };
        for (_, f) in sprites {
            if let Some(r) = f.find("inGameSprite") {
                f.update(r, size, centre, None);
            }
        }
    }

    fn rect_of(g: &Gui, name: &str, width: u32, height: u32) -> Option<Rect> {
        g.find(name).map(|i| g.rect(i, width, height))
    }

    /// The slot rectangles of the inventory's page `page`.
    pub fn slot_rects(&mut self, page: usize, width: u32, height: u32) -> Vec<Rect> {
        self.layout(width, height);
        self.slot_rects_laid(page, width, height)
    }

    fn slot_rects_laid(&self, page: usize, width: u32, height: u32) -> Vec<Rect> {
        let g = &self.inventory;
        (0..)
            .map_while(|s| g.find(&format!("page{page}Slot{s}")))
            .map(|n| g.rect(n, width, height))
            .collect()
    }

    /// The HUD's inventory button.
    pub fn hud_button(&mut self, width: u32, height: u32) -> Option<Rect> {
        self.layout(width, height);
        Self::rect_of(&self.in_game, "inventoryButton", width, height)
    }

    /// Whether the notification is shown and (`x`, `y`) on it: it lies
    /// above every other layer (`Notify.lua` is inserted last).
    pub fn notify_hit(&mut self, width: u32, height: u32, x: f64, y: f64) -> bool {
        if self.phase.is_none() {
            return false;
        }
        self.layout(width, height);
        Self::rect_of(&self.notify, "notify", width, height).is_some_and(|r| r.contains(x, y))
    }

    /// What a press at (`x`, `y`) hits, top first (`None`: the warp's);
    /// the screens laid out for the window.
    #[allow(clippy::too_many_arguments)]
    fn target(
        &self,
        inv: &Inventory,
        doc_shown: bool,
        in_movie: bool,
        width: u32,
        height: u32,
        x: f64,
        y: f64,
    ) -> Option<Target> {
        if self.phase.is_some()
            && Self::rect_of(&self.notify, "notify", width, height)
                .is_some_and(|r| r.contains(x, y))
        {
            return Some(Target::Notify);
        }
        if doc_shown {
            return Some(Target::Zoomed);
        }
        if in_movie {
            return None;
        }
        if self.menu_shown {
            for (name, t) in [
                ("quitButton", Target::Quit),
                ("mainMenuButton", Target::MainMenu),
            ] {
                if Self::rect_of(&self.menu, name, width, height).is_some_and(|r| r.contains(x, y))
                {
                    return Some(t);
                }
            }
        }
        if self.inventory_shown {
            if Self::rect_of(&self.inventory, "exit", width, height)
                .is_some_and(|r| r.contains(x, y))
            {
                return Some(Target::Exit);
            }
            for (s, r) in self
                .slot_rects_laid(inv.page, width, height)
                .iter()
                .enumerate()
            {
                if r.contains(x, y) && inv.object((inv.page, s)).is_some() {
                    return Some(Target::Object((inv.page, s)));
                }
            }
            return Some(Target::Inventory);
        }
        if Self::rect_of(&self.in_game, "inventoryButton", width, height)
            .is_some_and(|r| r.contains(x, y))
        {
            return Some(Target::Hud);
        }
        None
    }

    /// A press: whether a screen took it.
    #[allow(clippy::too_many_arguments)]
    pub fn press(
        &mut self,
        inv: &Inventory,
        doc_shown: bool,
        in_movie: bool,
        width: u32,
        height: u32,
        x: f64,
        y: f64,
    ) -> bool {
        self.layout(width, height);
        let t = self.target(inv, doc_shown, in_movie, width, height, x, y);
        self.press = t;
        self.drag = match t {
            Some(Target::Object(at)) => Some(Drag {
                from: at,
                at: (x, y),
                moving: false,
            }),
            _ => None,
        };
        t.is_some() || !self.warp_input
    }

    /// The pointer moved with the button held.
    pub fn drag_to(&mut self, x: f64, y: f64) {
        if let Some(d) = self.drag.as_mut() {
            if !d.moving {
                let dist = ((x - d.at.0).powi(2) + (y - d.at.1).powi(2)).sqrt() as f32;
                // `(float)(int)threshold < distance` (@ 0x0054d7c0).
                d.moving = (self.drag_threshold as i32 as f32) < dist;
            }
            if d.moving {
                d.at = (x, y);
            }
        }
    }

    /// A release: the actions of the click it validates; `true` in the
    /// second place when a screen took the press.
    #[allow(clippy::too_many_arguments)]
    pub fn release(
        &mut self,
        inv: &Inventory,
        doc_shown: bool,
        in_movie: bool,
        width: u32,
        height: u32,
        x: f64,
        y: f64,
    ) -> (Vec<Action>, bool) {
        self.layout(width, height);
        let pressed = self.press.take();
        let drag = self.drag.take();
        let took = pressed.is_some() || !self.warp_input;
        if let Some(d) = drag.filter(|d| d.moving) {
            let to = self
                .slot_rects_laid(inv.page, width, height)
                .iter()
                .position(|r| r.contains(x, y))
                .map(|s| (inv.page, s));
            log::info!("drop of {:?} on {to:?}", d.from);
            return (
                to.map(|t| vec![Action::Dropped(d.from, t)])
                    .unwrap_or_default(),
                took,
            );
        }
        let now = self.target(inv, doc_shown, in_movie, width, height, x, y);
        let Some(t) = pressed.filter(|p| Some(*p) == now) else {
            return (Vec::new(), took);
        };
        let sound = |file: &Gui, name: &str| {
            file.find(name)
                .and_then(|i| file.node(i).validation_sound.clone())
                .map(Action::Sound)
        };
        let mut out = Vec::new();
        match t {
            Target::Hud => {
                out.push(Action::Sound(HUD_SOUND.to_string()));
                out.push(Action::OpenMenu);
            }
            Target::Quit => {
                out.extend(sound(&self.menu, "quitButton"));
                out.push(Action::CloseMenu);
            }
            Target::MainMenu => {
                out.extend(sound(&self.menu, "mainMenuButton"));
                out.push(Action::MainMenu);
            }
            Target::Exit => out.push(Action::HideInventory),
            Target::Object(at) => {
                out.extend(self.object_sound.clone().map(Action::Sound));
                out.push(Action::ObjectClicked(at));
            }
            Target::Zoomed => {
                out.extend(sound(&self.documents, "zoomed"));
                out.push(Action::ZoomedClicked);
            }
            Target::Notify | Target::Inventory => {}
        }
        (out, took)
    }

    /// `InventoryMenu::enter` and the inventory tab.
    pub fn open_menu(&mut self, inv: &mut Inventory) {
        self.warp_input = false;
        self.menu_shown = true;
        self.saved_input = self.warp_input;
        self.inventory_shown = true;
        inv.page = 0;
    }

    /// `InventoryMenu::leave` (0x0057c930).
    pub fn close_menu(&mut self) {
        self.drag = None;
        self.inventory_shown = false;
        self.menu_shown = false;
        self.warp_input = true;
    }

    /// The inventory's `exit` (0x0055fd30).
    pub fn hide_inventory(&mut self) {
        if self.inventory_shown {
            self.drag = None;
            self.inventory_shown = false;
            self.warp_input = self.saved_input;
        }
    }

    /// `AddToBag`'s notification queued (0x00568960) at `now_ms`.
    pub fn notify(&mut self, name: String, now_ms: f64) {
        self.queue.push_back(name);
        self.next_notification(now_ms);
    }

    /// The next name shown when no fade runs (0x00568470): its text set,
    /// `notify` shown, `fadeIn` played.
    fn next_notification(&mut self, now_ms: f64) {
        if self.phase.is_some() {
            return;
        }
        let Some(name) = self.queue.pop_front() else {
            return;
        };
        let mut markup = NOTIFY_FORMAT.as_bytes().to_vec();
        markup.extend(to_cp1252(&name));
        let g = &mut self.notify;
        if let Some(t) = g.find("text") {
            g.set_text(t, markup, now_ms);
        }
        if let Some(n) = g.find("notify") {
            g.node_mut(n).visible = true;
        }
        self.play_phase(0, now_ms);
    }

    fn play_phase(&mut self, k: usize, now_ms: f64) {
        self.phase = Some(k);
        if let Some(a) = self.notify.find(PHASES[k]) {
            self.notify.play(a, now_ms);
        }
    }

    /// The screens' animations and scrolls at `now_ms` (`updateAll`, once
    /// a frame): a finished fade plays the next, the last hides `notify`
    /// and shows the next name.
    pub fn update(&mut self, now_ms: f64) {
        self.inventory.tick(now_ms);
        let finished = self.notify.tick(now_ms);
        let Some(k) = self.phase else { return };
        let done = self
            .notify
            .find(PHASES[k])
            .is_none_or(|a| finished.contains(&a));
        if !done {
            return;
        }
        if k + 1 < PHASES.len() {
            self.play_phase(k + 1, now_ms);
        } else {
            self.phase = None;
            hide(&mut self.notify, "notify");
            self.next_notification(now_ms);
        }
    }

    /// The selected object's name and description in the inventory's
    /// `text` (`textFormat`), set again only when it changed (a new text
    /// restarts the auto-scroll).
    fn sync_text(&mut self, inv: &Inventory, now_ms: f64) {
        let markup = to_cp1252(&inv.description(&self.text_format));
        let g = &mut self.inventory;
        if let Some(t) = g.find("text") {
            if g.node(t).markup != markup {
                g.set_text(t, markup, now_ms);
            }
        }
    }

    /// Hashes what the screens show (for the viewer's redraws).
    pub fn key(&self, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        (
            self.menu_shown,
            self.inventory_shown,
            self.sprites.len(),
            self.phase,
            self.drag
                .filter(|d| d.moving)
                .map(|d| (d.from, (d.at.0.to_bits(), d.at.1.to_bits()))),
            &self.zoomed_picture,
            self.press,
        )
            .hash(h);
        for (g, root, shown) in [
            (&self.inventory, "inventory", self.inventory_shown),
            (&self.notify, "notify", self.phase.is_some()),
        ] {
            if let (true, Some(r)) = (shown, g.find(root)) {
                g.hash_into(r, h);
            }
        }
    }

    /// The HUD (on-screen sprites and the inventory button with the
    /// selected object) into `out`.
    pub fn draw_hud(&mut self, inv: &Inventory, out: &mut [u32], width: u32, height: u32) {
        let selected = inv.selected_name().map(object_picture);
        if let Some(i) = self.in_game.find("currentObjectSprite") {
            self.in_game.node_mut(i).image = selected;
        }
        self.layout(width, height);
        self.load_pictures(|s| &s.in_game, "inventoryButton");
        let mut paths = Vec::new();
        for (_, f) in &self.sprites {
            if let Some(r) = f.find("inGameSprite") {
                f.pictures(r, &mut paths);
            }
        }
        for p in paths {
            self.picture(&p);
        }
        let pics = Pics(&self.pictures);
        // The sprites go into `inGameSprites` (Percent 1 × 1 of the window).
        for (_, f) in &self.sprites {
            if let Some(r) = f.find("inGameSprite") {
                f.draw(r, out, width, height, &pics);
            }
        }
        if let Some(b) = self.in_game.find("inventoryButton") {
            self.in_game.draw(b, out, width, height, &pics);
        }
    }

    /// The inventory menu and the shown document.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_menu(
        &mut self,
        inv: &Inventory,
        out: &mut [u32],
        width: u32,
        height: u32,
        now_ms: f64,
        pointer: Option<(f64, f64)>,
    ) {
        if self.inventory_shown {
            self.sync_text(inv, now_ms);
        }
        if let Some(pic) = self.zoomed_picture.clone() {
            let ratio = self
                .picture(&pic)
                .map(|p| (f64::from(p.width) / f64::from(p.height.max(1))) as f32);
            if let Some(i) = self.documents.find("zoomedSprite") {
                let n = self.documents.node_mut(i);
                n.image = Some(pic);
                n.ratio = ratio.unwrap_or(1.0);
            }
        }
        self.layout(width, height);
        let ids: Vec<String> = inv.objects().map(|(_, o)| object_picture(&o.id)).collect();
        for p in &ids {
            self.picture(p);
        }
        if self.inventory_shown {
            self.load_pictures(|s| &s.inventory, "inventory");
            if let Some(p) = self
                .inventory
                .find("selectionSprite")
                .and_then(|i| self.inventory.node(i).image.clone())
            {
                self.picture(&p);
            }
        }
        if self.menu_shown {
            self.load_pictures(|s| &s.menu, "inventoryMenu");
            // Down while the press is held inside (@ 0x004e4e10).
            for (name, t) in [
                ("quitButton", Target::Quit),
                ("mainMenuButton", Target::MainMenu),
            ] {
                if let Some(i) = self.menu.find(name) {
                    let r = self.menu.rect(i, width, height);
                    let down =
                        self.press == Some(t) && pointer.is_some_and(|(x, y)| r.contains(x, y));
                    self.menu.node_mut(i).state = if down { State::Down } else { State::Up };
                }
            }
        }
        if self.zoomed_picture.is_some() {
            self.load_pictures(|s| &s.documents, "zoomed");
        }
        let pics = Pics(&self.pictures);
        let get = |p: &str| self.pictures.get(p).and_then(Option::as_ref);
        if self.inventory_shown {
            if let Some(r) = self.inventory.find("inventory") {
                self.inventory.draw(r, out, width, height, &pics);
            }
            let dragged = self.drag.filter(|d| d.moving).map(|d| d.from);
            let sel_pic = self
                .inventory
                .find("selectionSprite")
                .and_then(|i| self.inventory.node(i).image.clone());
            for (s, r) in self
                .slot_rects_laid(inv.page, width, height)
                .iter()
                .enumerate()
            {
                let at = (inv.page, s);
                let Some(o) = inv.object(at) else { continue };
                let rect = match dragged {
                    Some(d) if d == at => {
                        let (x, y) = self.drag.map(|d| d.at).unwrap_or((0.0, 0.0));
                        Rect {
                            left: x - r.width / 2.0,
                            top: y - r.height / 2.0,
                            ..*r
                        }
                    }
                    _ => *r,
                };
                if let Some(p) = get(&object_picture(&o.id)) {
                    draw_picture(out, width, height, p, rect, None, [255; 4]);
                }
                if inv.selected == Some(at) {
                    if let Some(p) = sel_pic.as_deref().and_then(get) {
                        let size = percent_size(
                            (1.1, 1.1),
                            (rect.width as f32, rect.height as f32),
                            1.0,
                            RatioMode::None,
                            crate::layout::SAFE_AREA_RATIO,
                        );
                        let (w, h) = (f64::from(size.0), f64::from(size.1));
                        let sr = Rect {
                            left: rect.left + rect.width / 2.0 - w / 2.0,
                            top: rect.top + rect.height / 2.0 - h / 2.0,
                            width: w,
                            height: h,
                        };
                        draw_picture(out, width, height, p, sr, None, [255; 4]);
                    }
                }
            }
        }
        if self.menu_shown {
            if let Some(r) = self.menu.find("inventoryMenu") {
                self.menu.draw(r, out, width, height, &pics);
            }
        }
        if self.zoomed_picture.is_some() {
            if let Some(r) = self.documents.find("zoomed") {
                self.documents.draw(r, out, width, height, &pics);
            }
        }
    }

    /// The notification (its picture and text, faded).
    pub fn draw_notify(&mut self, out: &mut [u32], width: u32, height: u32) {
        if self.phase.is_none() {
            return;
        }
        self.layout(width, height);
        self.load_pictures(|s| &s.notify, "notify");
        if let Some(r) = self.notify.find("notify") {
            self.notify
                .draw(r, out, width, height, &Pics(&self.pictures));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::gui::anim::curve;

    /// The notification's fade curve (`Notify.lua`'s samples) through the
    /// GUI base's curve (@ 0x005525c0).
    #[test]
    fn the_curve_is_piecewise_linear() {
        let c = [0.0, 0.35, 0.68, 0.85, 0.93, 0.97, 1.0];
        assert_eq!(curve(&c, 0.0, 500.0), 0.0);
        assert_eq!(curve(&c, 500.0, 500.0), 1.0);
        // Sample 3 of 6 exactly, and 30 % of the way to the second one.
        assert_eq!(curve(&c, 250.0, 500.0), 0.85);
        assert!((curve(&c, 25.0, 500.0) - 0.105).abs() < 1e-6);
    }
}
