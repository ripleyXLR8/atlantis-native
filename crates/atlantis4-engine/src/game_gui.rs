//! The game's side of the inventory, the documents, the on-screen sprites
//! and the save's parts: the bindings' work, the screens' input and
//! drawing, and the callbacks they raise (atlantis4-gog-en-2363910; the
//! rules are in [`crate::inventory`], [`crate::documents`] and
//! [`crate::screens`]).

use atlantis4_script::Value;

use crate::documents::{self, DocAction};
use crate::game::Game;
use crate::gui;
use crate::inventory::{Inventory, ObjectClick};
use crate::savexml::SaveParts;
use crate::screens::{Action, Screens};

/// `AddToBag`'s sound (0x005284e0).
pub const TAKE_SOUND: &str = "Sounds/SFX/N_prendre.wav";

impl Game {
    /// The screens, their layouts read at the first call, and the
    /// inventory and the documents browser sized from them.
    pub fn gui(&mut self) -> &mut Screens {
        if self.screens.is_none() {
            let s = Screens::load(&self.files, &self.lang);
            let slots = Screens::slots(&s.inventory);
            let mut inv = Inventory::new(&slots);
            let objects = atlantis4_data::texts::objects_path(&self.lang);
            match self.files.read(&objects) {
                Some(d) => inv.read_names(&d),
                None => log::warn!("no {objects}"),
            }
            log::info!("inventory: {} page(s) of {:?} slots", slots.len(), slots);
            self.inventory = inv;
            self.documents = documents::Documents::new(&Screens::slots(&s.documents));
            self.screens = Some(s);
        }
        self.screens.as_mut().expect("screens")
    }

    /// The engine clock now, µs.
    fn now_us(&self) -> f64 {
        self.clock_ms() * 1000.0
    }

    /// `AddToBag(name)` (0x005284e0): the object added, its notification
    /// queued, `N_prendre.wav` played; `AddToBagWithoutNotification`
    /// (0x005283d0) adds only.
    pub fn add_to_bag(&mut self, name: &str, notify: bool) {
        self.gui();
        if let Err(e) = self.inventory.add(name) {
            log::warn!("AddToBag: {e}");
        }
        if notify {
            let label = self
                .inventory
                .label(name)
                .map(|l| l.name.clone())
                .unwrap_or_default();
            let now = self.clock_ms();
            self.gui().notify(label, now);
            self.sounds.play_free_sound(&self.files, TAKE_SOUND, 1.0);
        }
    }

    /// `ShowDocument(name)` for page `page` (0x00557840): its picture,
    /// `.png` then `.jpg`, else hidden at once.
    pub fn show_document(&mut self, name: &str, page: u32) {
        self.gui();
        let stem = documents::page_stem(name, page);
        let found = [".png", ".jpg"]
            .iter()
            .find_map(|e| gui::resolve_in(&self.files, &format!("{stem}{e}"), &self.lang));
        match found {
            Some(p) => {
                log::info!("document {name} page {page}: {p}");
                self.documents.show(name, page, p.clone());
                self.gui().zoomed_picture = Some(p);
            }
            None => {
                let n = self.documents.show_missing(name);
                self.document_closed(n);
            }
        }
    }

    /// `HideDocument` (0x00557020).
    pub fn hide_document(&mut self) {
        let n = self.documents.hide();
        self.document_closed(n);
    }

    fn document_closed(&mut self, name: String) {
        if let Some(s) = self.screens.as_mut() {
            s.zoomed_picture = None;
        }
        log::info!("document {name} closed");
        self.callbacks.push((
            "OnDocumentClosed".to_string(),
            vec![Value::Str(name.into_bytes())],
        ));
    }

    /// The GUI's timed work at `now_ms` (the shown document's alarm, the
    /// notification's fades).
    pub fn update_gui(&mut self, now_ms: f64) {
        if self.screens.is_none() {
            return;
        }
        self.gui().update(now_ms);
        match self.documents.update(now_ms * 1000.0) {
            DocAction::NextPage(n, p) => self.show_document(&n, p),
            DocAction::Hide => self.double_click_hide(),
            DocAction::None => {}
        }
    }

    fn double_click_hide(&mut self) {
        self.sounds
            .play_free_sound(&self.files, documents::DOUBLE_CLICK_SOUND, 1.0);
        self.hide_document();
    }

    /// The callbacks the GUI raised since the last call, in order.
    pub fn gui_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        std::mem::take(&mut self.callbacks)
    }

    /// Whether the warp takes the pointer (`App+0x7a10`, and no document
    /// covering it).
    pub fn warp_takes_input(&self) -> bool {
        // `FinishGame` clears `game+0x4038` (@ 0x0052bc7e).
        if self.planks.is_some() {
            return false;
        }
        self.screens
            .as_ref()
            .is_none_or(|s| s.warp_input && self.documents.shown.is_none())
    }

    /// A press at window pixel (`x`, `y`): whether a screen took it.
    pub fn screens_press(&mut self, width: u32, height: u32, x: f64, y: f64) -> bool {
        let (doc, movie) = (self.documents.shown.is_some(), self.movie.is_some());
        self.gui();
        let Game {
            screens, inventory, ..
        } = self;
        screens
            .as_mut()
            .expect("screens")
            .press(inventory, doc, movie, width, height, x, y)
    }

    /// The pointer moved to (`x`, `y`) with the button held.
    pub fn gui_drag(&mut self, x: f64, y: f64) {
        if let Some(s) = self.screens.as_mut() {
            s.drag_to(x, y);
        }
    }

    /// A release at (`x`, `y`): the validated click's work; whether a
    /// screen had taken the press.
    pub fn screens_release(&mut self, width: u32, height: u32, x: f64, y: f64) -> bool {
        let (doc, movie) = (self.documents.shown.is_some(), self.movie.is_some());
        self.gui();
        let (actions, took) = {
            let Game {
                screens, inventory, ..
            } = &mut *self;
            screens
                .as_mut()
                .expect("screens")
                .release(inventory, doc, movie, width, height, x, y)
        };
        for a in actions {
            self.act(a);
        }
        took
    }

    fn act(&mut self, a: Action) {
        log::info!("GUI: {a:?}");
        match a {
            Action::Sound(p) => self.sounds.play_free_sound(&self.files, &p, 1.0),
            Action::OpenMenu => {
                let Game {
                    screens, inventory, ..
                } = self;
                screens.as_mut().expect("screens").open_menu(inventory);
            }
            Action::CloseMenu => self.gui().close_menu(),
            Action::HideInventory => self.gui().hide_inventory(),
            Action::MainMenu => {
                log::info!("mainMenuButton: save.xml then the main menu (M4, not done)")
            }
            Action::ObjectClicked(at) => {
                let now = self.now_us();
                if let Some(c) = self.inventory.click(at, now) {
                    if let Some(n) = c.reselected() {
                        self.callbacks.push((
                            "OnInventoryObjectReselected".to_string(),
                            vec![Value::Str(n.as_bytes().to_vec())],
                        ));
                    }
                    if c.closes() {
                        self.gui().close_menu();
                    }
                    if c == ObjectClick::Selected {
                        log::info!("selected {:?}", self.inventory.selected_name());
                    }
                }
            }
            Action::Dropped(from, to) => {
                self.inventory.drop_on_slot(from, to);
            }
            Action::ZoomedClicked => {
                let now = self.now_us();
                if self.documents.click(now) == DocAction::Hide {
                    self.double_click_hide();
                }
            }
        }
    }

    /// The HUD (before a movie's layouts).
    pub(crate) fn draw_hud(&mut self, out: &mut [u32], width: u32, height: u32) {
        self.gui();
        let Game {
            screens, inventory, ..
        } = self;
        screens
            .as_mut()
            .expect("screens")
            .draw_hud(inventory, out, width, height);
    }

    /// The inventory menu and the shown document.
    pub(crate) fn draw_menu(
        &mut self,
        out: &mut [u32],
        width: u32,
        height: u32,
        pointer: Option<(f64, f64)>,
    ) {
        self.gui();
        let now = self.now_ms;
        let Game {
            screens, inventory, ..
        } = self;
        screens
            .as_mut()
            .expect("screens")
            .draw_menu(inventory, out, width, height, now, pointer);
    }

    /// The object notification (above the dialogs).
    pub(crate) fn draw_notify(&mut self, out: &mut [u32], width: u32, height: u32) {
        if let Some(s) = self.screens.as_mut() {
            s.draw_notify(out, width, height);
        }
    }

    /// What the GUI shows, for [`Game::frame_key`].
    pub(crate) fn gui_key(&self, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        if let Some(s) = &self.screens {
            s.key(h);
        }
        self.inventory.selected.hash(h);
        for (at, o) in self.inventory.objects() {
            (at, &o.id).hash(h);
        }
        self.documents.shown.as_ref().map(|d| &d.picture).hash(h);
    }

    /// The save's parts the engine holds.
    pub fn save_parts(&mut self) -> SaveParts {
        self.gui();
        SaveParts {
            inventory: self.inventory.to_xml(),
            documents: self.documents.to_xml(),
            diary: self.diary.to_xml(),
            puzzle_timer: self.puzzle_timer.to_xml(),
            warp_name: self.warp_name.clone(),
            orientation: (self.view.yaw, self.view.pitch),
            music: self.sounds.music.path().as_bytes().to_vec(),
            // `game+0x4608` + the timer since `Game::enter` / 1,000,000
            // (@ 0x00524535..0x00524558).
            played_time: self.played_time(),
        }
    }
}
