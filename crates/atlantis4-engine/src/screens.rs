//! The in-game screens around the warp that the inventory and the
//! documents need (atlantis4-gog-en-2363910): the HUD of `GUI/InGame.lua`
//! (the inventory button with the selected object, the on-screen sprites
//! of `LoadSprite`), the inventory menu (`InventoryMenu/InventoryMenu.lua`
//! over `Inventory/Inventory.lua`), the shown document (`zoomed` of
//! `DocumentsBrowser/DocumentsBrowser.lua`) and the object notification
//! of `GUI/Notify.lua`. Every layout comes from the install's files
//! through [`crate::layout_tree`]; text goes through [`TextDraw`].
//!
//! Draw order (`Game::load` @ 0x00528c10 inserts the screens so): the
//! HUD (`inGameSprites`, then `inventoryButton`), the inventory menu
//! (its children in the loader's order, 0x0057cdc0: the inventory, the
//! documents browser, the menu's own `inventoryMenu`, the diary, then
//! `zoomed`, 0x00553690), then the notification.
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
//! up layout's slot `+0x4c` with 0, not walked).
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
//! the next (0x00568260, 0x00568050, 0x00568a60). The colour animations'
//! values and curves are the file's.

use std::collections::{HashMap, VecDeque};

use atlantis4_data::image::{decode_jpeg, decode_png, Image};
use atlantis4_data::A4Files;
use atlantis4_script::BootValue;

use crate::inventory::{Inventory, SlotRef};
use crate::layout::{percent_size, RatioMode, Rect};
use crate::layout_tree::{self, draw_picture, Placed, UiFile};
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

/// Draws GUI text. The glyphs and the markup are the dialog work's
/// (`TeTextBase2`); until it is merged nothing is drawn.
pub trait TextDraw {
    /// `markup` (the engine's tags and the text) laid out in `rect`.
    fn draw(&mut self, out: &mut [u32], width: u32, height: u32, rect: Rect, markup: &str);
}

/// The placeholder: no text.
pub struct NoText;

impl TextDraw for NoText {
    fn draw(&mut self, _out: &mut [u32], _w: u32, _h: u32, _rect: Rect, _markup: &str) {}
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    FadeIn,
    Visible,
    FadeOut,
}

/// The notification in progress.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    name: String,
    phase: Phase,
    start_ms: f64,
}

/// The screens and their state.
pub struct Screens {
    files: A4Files,
    lang: String,
    pictures: HashMap<String, Option<Picture>>,
    pub in_game: UiFile,
    pub inventory: UiFile,
    pub menu: UiFile,
    pub documents: UiFile,
    pub notify: UiFile,
    /// `LoadSprite`'s files in load order (a file loaded again is added
    /// again: the list is cleared only by `FinishGame` and the game's
    /// end, 0x00525f50).
    pub sprites: Vec<(String, UiFile)>,
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
    shown: Option<Shown>,
    /// The picture of the shown document, if any.
    pub zoomed_picture: Option<String>,
}

fn rgba(img: Image) -> Option<Picture> {
    let rgba = match img.channels {
        4 => img.pixels,
        3 => img
            .pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        2 => img
            .pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        _ => img.pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    };
    Some(Picture {
        width: img.width,
        height: img.height,
        rgba,
    })
}

/// A picture file (a document's page can be JPEG).
fn picture_of(data: &[u8], path: &str) -> Option<Picture> {
    let img = if path.to_ascii_lowercase().ends_with(".jpg") {
        decode_jpeg(data).ok()?
    } else {
        decode_png(data).ok()?
    };
    rgba(img)
}

impl Screens {
    /// The screens' layout files read from the install.
    pub fn load(files: &A4Files, lang: &str) -> Screens {
        let load = |p: &str, g: &[&str]| layout_tree::load(files, lang, p, g);
        let mut s = Screens {
            files: files.clone(),
            lang: lang.to_string(),
            pictures: HashMap::new(),
            in_game: load(IN_GAME_LUA, &[]),
            inventory: load(INVENTORY_LUA, &["textFormat"]),
            menu: load(MENU_LUA, &[]),
            documents: load(DOCUMENTS_LUA, &[]),
            notify: load(NOTIFY_LUA, &[]),
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
            shown: None,
            zoomed_picture: None,
        };
        let object = load(OBJECT_LUA, &["dragThreshold"]);
        if let Some(t) = object.global_num("dragThreshold") {
            s.drag_threshold = t;
        }
        s.object_sound = object
            .find("object")
            .and_then(|i| object.nodes[i].validation_sound.clone());
        s.text_format = s.inventory.global_str("textFormat").unwrap_or_default();
        for f in [
            IN_GAME_LUA,
            INVENTORY_LUA,
            MENU_LUA,
            DOCUMENTS_LUA,
            NOTIFY_LUA,
        ] {
            s.prepare(f);
        }
        // Shown by their owners only: the movie's layouts (`movie`), the
        // selection sprite (on the selected object), `notify` and
        // `zoomed` until used.
        for (file, names) in [
            (
                IN_GAME_LUA,
                &["videoBackgroundButton", "video", "skipVideoButton"][..],
            ),
            (INVENTORY_LUA, &["selectionSprite"][..]),
        ] {
            let f = s.file_mut(file);
            for n in names {
                if let Some(i) = f.find(n) {
                    f.nodes[i].visible = false;
                }
            }
        }
        s
    }

    fn file_mut(&mut self, name: &str) -> &mut UiFile {
        match name {
            IN_GAME_LUA => &mut self.in_game,
            INVENTORY_LUA => &mut self.inventory,
            MENU_LUA => &mut self.menu,
            DOCUMENTS_LUA => &mut self.documents,
            _ => &mut self.notify,
        }
    }

    /// The pictures of a file loaded and their sizes applied.
    fn prepare(&mut self, name: &str) {
        let images: Vec<String> = self
            .file_mut(name)
            .nodes
            .iter()
            .filter_map(|n| n.image.clone())
            .collect();
        for i in &images {
            self.picture(i);
        }
        let sizes: HashMap<String, (u32, u32)> = images
            .iter()
            .filter_map(|i| {
                let p = self.pictures.get(i)?.as_ref()?;
                Some((i.clone(), (p.width, p.height)))
            })
            .collect();
        self.file_mut(name)
            .apply_pictures(&|p| sizes.get(p).copied());
    }

    /// Loads the picture a logical path stands for (once).
    pub fn picture(&mut self, path: &str) -> Option<&Picture> {
        if !self.pictures.contains_key(path) {
            let pic = layout_tree::resolve(&self.files, path, &self.lang)
                .and_then(|r| self.files.read(&r).and_then(|d| picture_of(&d, &r)));
            if pic.is_none() {
                log::warn!("no picture {path}");
            }
            self.pictures.insert(path.to_string(), pic);
        }
        self.pictures.get(path).and_then(Option::as_ref)
    }

    /// The slots per page of a file's `page<N>Slot<M>` layouts.
    pub fn slots(file: &UiFile) -> Vec<usize> {
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
        let mut f = layout_tree::load(&self.files, &self.lang, path, &[]);
        let images: Vec<String> = f.nodes.iter().filter_map(|n| n.image.clone()).collect();
        for i in &images {
            self.picture(i);
        }
        let sizes: HashMap<String, (u32, u32)> = images
            .iter()
            .filter_map(|i| {
                let p = self.pictures.get(i)?.as_ref()?;
                Some((i.clone(), (p.width, p.height)))
            })
            .collect();
        f.apply_pictures(&|p| sizes.get(p).copied());
        if f.find("inGameSprite").is_none() {
            log::warn!("LoadSprite({path}): no inGameSprite");
        }
        self.sprites.push((path.to_string(), f));
    }

    /// `UnloadSprite(path)` (0x00525e10): the sprites of that file.
    pub fn unload_sprite(&mut self, path: &str) {
        self.sprites.retain(|(p, _)| p != path);
    }

    /// The window as a parent: its size, centred.
    fn win(width: u32, height: u32) -> (f32, f32) {
        (width as f32, height as f32)
    }

    fn places(file: &UiFile, root: &str, width: u32, height: u32) -> Vec<Option<Placed>> {
        match file.find(root) {
            Some(r) => file.layout(r, Self::win(width, height), (0.0, 0.0)),
            None => vec![None; file.nodes.len()],
        }
    }

    fn rect_of(
        file: &UiFile,
        places: &[Option<Placed>],
        name: &str,
        w: u32,
        h: u32,
    ) -> Option<Rect> {
        let i = file.find(name)?;
        places[i].map(|p| p.rect(false, w, h))
    }

    /// The slot rectangles of the inventory's page `page`.
    pub fn slot_rects(&self, page: usize, width: u32, height: u32) -> Vec<Rect> {
        let pl = Self::places(&self.inventory, "inventory", width, height);
        (0..)
            .map_while(|s| {
                let n = self.inventory.find(&format!("page{page}Slot{s}"))?;
                Some(pl[n].map(|p| p.rect(false, width, height)))
            })
            .map(|r| {
                r.unwrap_or(Rect {
                    left: 0.0,
                    top: 0.0,
                    width: 0.0,
                    height: 0.0,
                })
            })
            .collect()
    }

    /// The HUD's inventory button.
    pub fn hud_button(&self, width: u32, height: u32) -> Option<Rect> {
        let pl = Self::places(&self.in_game, "inGame", width, height);
        Self::rect_of(&self.in_game, &pl, "inventoryButton", width, height)
    }

    fn notify_rect(&self, width: u32, height: u32) -> Option<Rect> {
        let pl = Self::places(&self.notify, "notify", width, height);
        Self::rect_of(&self.notify, &pl, "notify", width, height)
    }

    /// What a press at (`x`, `y`) hits, top first (`None`: the warp's).
    #[allow(clippy::too_many_arguments)]
    pub fn target(
        &self,
        inv: &Inventory,
        doc_shown: bool,
        in_movie: bool,
        width: u32,
        height: u32,
        x: f64,
        y: f64,
    ) -> Option<Target> {
        if self.shown.is_some()
            && self
                .notify_rect(width, height)
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
            let pl = Self::places(&self.menu, "inventoryMenu", width, height);
            for (name, t) in [
                ("quitButton", Target::Quit),
                ("mainMenuButton", Target::MainMenu),
            ] {
                if Self::rect_of(&self.menu, &pl, name, width, height)
                    .is_some_and(|r| r.contains(x, y))
                {
                    return Some(t);
                }
            }
        }
        if self.inventory_shown {
            let pl = Self::places(&self.inventory, "inventory", width, height);
            if Self::rect_of(&self.inventory, &pl, "exit", width, height)
                .is_some_and(|r| r.contains(x, y))
            {
                return Some(Target::Exit);
            }
            for (s, r) in self.slot_rects(inv.page, width, height).iter().enumerate() {
                if r.contains(x, y) && inv.object((inv.page, s)).is_some() {
                    return Some(Target::Object((inv.page, s)));
                }
            }
            return Some(Target::Inventory);
        }
        if self
            .hud_button(width, height)
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
        let pressed = self.press.take();
        let drag = self.drag.take();
        let took = pressed.is_some() || !self.warp_input;
        if let Some(d) = drag.filter(|d| d.moving) {
            let to = self
                .slot_rects(inv.page, width, height)
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
        let sound = |file: &UiFile, name: &str| {
            file.find(name)
                .and_then(|i| file.nodes[i].validation_sound.clone())
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

    fn next_notification(&mut self, now_ms: f64) {
        if self.shown.is_none() {
            if let Some(name) = self.queue.pop_front() {
                self.shown = Some(Shown {
                    name,
                    phase: Phase::FadeIn,
                    start_ms: now_ms,
                });
            }
        }
    }

    /// The colour animation `name`'s duration (ms), curve and alpha ends.
    fn anim(&self, name: &str) -> (f64, Vec<f64>, f64, f64) {
        let Some(i) = self.notify.find(name) else {
            return (0.0, Vec::new(), 255.0, 255.0);
        };
        let n = &self.notify.nodes[i];
        let alpha = |k: &str| {
            n.extra_list(k)
                .and_then(|v| v.get(3).copied())
                .unwrap_or(255.0)
        };
        (
            n.extra_num("duration").unwrap_or(0.0),
            n.extra_list("curve").unwrap_or_default(),
            alpha("startValue"),
            alpha("endValue"),
        )
    }

    /// The notification's fades at `now_ms` (`updateAll`, once a frame).
    pub fn update(&mut self, now_ms: f64) {
        while let Some(s) = self.shown.clone() {
            let name = match s.phase {
                Phase::FadeIn => "fadeIn",
                Phase::Visible => "visible",
                Phase::FadeOut => "fadeOut",
            };
            let (duration, ..) = self.anim(name);
            if now_ms.max(s.start_ms) - s.start_ms < duration {
                break;
            }
            self.shown = match s.phase {
                Phase::FadeIn => Some(Shown {
                    phase: Phase::Visible,
                    start_ms: now_ms,
                    ..s
                }),
                Phase::Visible => Some(Shown {
                    phase: Phase::FadeOut,
                    start_ms: now_ms,
                    ..s
                }),
                Phase::FadeOut => None,
            };
            if self.shown.is_none() {
                self.next_notification(now_ms);
                break;
            }
        }
    }

    /// The notification's alpha at `now_ms` ([`curve_at`]).
    fn notify_alpha(&self, now_ms: f64) -> Option<(String, u8)> {
        let s = self.shown.as_ref()?;
        let name = match s.phase {
            Phase::FadeIn => "fadeIn",
            Phase::Visible => "visible",
            Phase::FadeOut => "fadeOut",
        };
        let (duration, curve, a0, a1) = self.anim(name);
        let e = (now_ms.max(s.start_ms) - s.start_ms) as f32;
        let f = curve_at(&curve, e, duration as f32);
        let v = a0 as f32 * (1.0 - f) + a1 as f32 * f;
        Some((s.name.clone(), v.clamp(0.0, 255.0) as u8))
    }

    /// Hashes what the screens show at `now_ms` (for the viewer's
    /// redraws).
    pub fn key(&self, now_ms: f64, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        (
            self.menu_shown,
            self.inventory_shown,
            self.sprites.len(),
            self.notify_alpha(now_ms),
            self.drag
                .filter(|d| d.moving)
                .map(|d| (d.from, (d.at.0.to_bits(), d.at.1.to_bits()))),
            &self.zoomed_picture,
            self.press,
        )
            .hash(h);
    }

    /// The HUD (on-screen sprites and the inventory button with the
    /// selected object) into `out`.
    pub fn draw_hud(&mut self, inv: &Inventory, out: &mut [u32], width: u32, height: u32) {
        let selected = inv.selected_name().map(object_picture);
        if let Some(p) = &selected {
            self.picture(p);
        }
        if let Some(i) = self.in_game.find("currentObjectSprite") {
            self.in_game.nodes[i].image = selected;
        }
        let pl = Self::places(&self.in_game, "inGame", width, height);
        // The sprites go into `inGameSprites` (Percent 1 × 1 of the window).
        let box_ = self
            .in_game
            .find("inGameSprites")
            .and_then(|i| pl[i])
            .unwrap_or(Placed {
                centre: (0.0, 0.0),
                size: Self::win(width, height),
            });
        let pictures = &self.pictures;
        let get = |p: &str| pictures.get(p).and_then(Option::as_ref);
        for (_, f) in &self.sprites {
            if let Some(r) = f.find("inGameSprite") {
                let spl = f.layout(r, box_.size, box_.centre);
                f.draw(r, &spl, out, width, height, &get, &|_| false);
            }
        }
        if let Some(b) = self.in_game.find("inventoryButton") {
            self.in_game
                .draw(b, &pl, out, width, height, &get, &|_| false);
        }
    }

    /// The inventory menu, the shown document and the notification.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_menu(
        &mut self,
        inv: &Inventory,
        text: &mut dyn TextDraw,
        out: &mut [u32],
        width: u32,
        height: u32,
        now_ms: f64,
        pointer: Option<(f64, f64)>,
    ) {
        let ids: Vec<String> = inv.objects().map(|(_, o)| object_picture(&o.id)).collect();
        for p in &ids {
            self.picture(p);
        }
        if let Some(p) = self.zoomed_picture.clone() {
            self.picture(&p);
        }
        let pressed_target = self.press;
        let hover = pointer;
        if self.inventory_shown {
            let pl = Self::places(&self.inventory, "inventory", width, height);
            let pictures = &self.pictures;
            let get = |p: &str| pictures.get(p).and_then(Option::as_ref);
            if let Some(r) = self.inventory.find("inventory") {
                self.inventory
                    .draw(r, &pl, out, width, height, &get, &|_| false);
            }
            if let Some(r) = Self::rect_of(&self.inventory, &pl, "text", width, height) {
                text.draw(out, width, height, r, &inv.description(&self.text_format));
            }
            let dragged = self.drag.filter(|d| d.moving).map(|d| d.from);
            let sel_pic = self
                .inventory
                .find("selectionSprite")
                .and_then(|i| self.inventory.nodes[i].image.clone());
            for (s, r) in self.slot_rects(inv.page, width, height).iter().enumerate() {
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
                    draw_picture(out, width, height, p, rect, 255);
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
                        draw_picture(out, width, height, p, sr, 255);
                    }
                }
            }
        }
        if self.menu_shown {
            let pl = Self::places(&self.menu, "inventoryMenu", width, height);
            let pictures = &self.pictures;
            let get = |p: &str| pictures.get(p).and_then(Option::as_ref);
            let menu = &self.menu;
            let pressed = |i: usize| {
                let name = menu.nodes[i].name.as_deref();
                let t = match name {
                    Some("quitButton") => Target::Quit,
                    Some("mainMenuButton") => Target::MainMenu,
                    _ => return false,
                };
                // Down while the press is held inside (@ 0x004e4e10).
                pressed_target == Some(t)
                    && hover.is_some_and(|(x, y)| {
                        pl[i].is_some_and(|p| p.rect(false, width, height).contains(x, y))
                    })
            };
            if let Some(r) = menu.find("inventoryMenu") {
                menu.draw(r, &pl, out, width, height, &get, &pressed);
            }
        }
        if let Some(pic) = self.zoomed_picture.clone() {
            let ratio = self
                .pictures
                .get(&pic)
                .and_then(Option::as_ref)
                .map(|p| (f64::from(p.width) / f64::from(p.height.max(1))) as f32);
            if let Some(i) = self.documents.find("zoomedSprite") {
                self.documents.nodes[i].image = Some(pic.clone());
                self.documents.nodes[i].ratio = ratio.unwrap_or(1.0);
            }
            let pl = Self::places(&self.documents, "zoomed", width, height);
            let pictures = &self.pictures;
            let get = |p: &str| pictures.get(p).and_then(Option::as_ref);
            if let Some(r) = self.documents.find("zoomed") {
                self.documents
                    .draw(r, &pl, out, width, height, &get, &|_| false);
            }
        }
        if let Some((name, alpha)) = self.notify_alpha(now_ms) {
            let pl = Self::places(&self.notify, "notify", width, height);
            if let Some(i) = self.notify.find("sprite") {
                let img = self.notify.nodes[i].image.clone();
                if let (Some(img), Some(p)) = (img, pl[i]) {
                    if let Some(pic) = self.pictures.get(&img).and_then(Option::as_ref) {
                        draw_picture(out, width, height, pic, p.rect(true, width, height), alpha);
                    }
                }
            }
            if let Some(r) = Self::rect_of(&self.notify, &pl, "text", width, height) {
                text.draw(out, width, height, r, &format!("{NOTIFY_FORMAT}{name}"));
            }
        }
    }
}

/// The animation curve (@ 0x005525c0): piecewise linear over equal steps
/// of the duration; 0 without samples, the last sample at or after the
/// end.
pub fn curve_at(s: &[f64], e: f32, duration: f32) -> f32 {
    let n = s.len();
    if n == 0 {
        return 0.0;
    }
    if e >= duration {
        return s[n - 1] as f32;
    }
    let x = e * (n as f32 - 1.0) / duration;
    let i = (x as usize).min(n - 1);
    let j = (i + 1).min(n - 1);
    s[i] as f32 * (i as f32 + 1.0 - x) + s[j] as f32 * (x - i as f32)
}

/// A `BootValue` string as text.
pub fn boot_str(v: &BootValue) -> Option<String> {
    match v {
        BootValue::Str(s) => Some(s.iter().map(|&c| char::from(c)).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_is_piecewise_linear() {
        let c = [0.0, 0.35, 0.68, 0.85, 0.93, 0.97, 1.0];
        assert_eq!(curve_at(&c, 0.0, 500.0), 0.0);
        assert_eq!(curve_at(&c, 500.0, 500.0), 1.0);
        // Sample 3 of 6 exactly, and 30 % of the way to the second one.
        assert_eq!(curve_at(&c, 250.0, 500.0), 0.85);
        assert!((curve_at(&c, 25.0, 500.0) - 0.105).abs() < 1e-6);
    }
}
