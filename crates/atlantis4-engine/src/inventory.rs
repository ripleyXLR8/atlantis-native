//! The inventory's model: its objects in their slots, the selection, the
//! bindings' side and the save's `<Inventory>` element
//! (atlantis4-gog-en-2363910; `Inventory` at `Application+0x5088`, inside
//! the inventory menu at `+0x4990`).
//!
//! **Slots** (`Inventory::load` @ 0x00562450, `STATIC`): the pages are the
//! layouts `page<N>`, each page's slots its buttons `page<N>Slot<M>`, both
//! counted from 0 while the name exists. *Atlantis Evolution*'s
//! `Inventory.lua` has one page of 8 slots (`DATA`). `stackSlot` and
//! `combinationSlot<N>` do not exist in that file, so the stack
//! (`+0x7d0`) is null and there are no combination slots.
//!
//! **Adding** (`AddToBag` → 0x00560720, `STATIC`): a new
//! `InventoryObject` loads `Inventory/InventoryObject.lua` and its picture
//! `Inventory/Objects/<id>.png` (0x004107f0), then goes into the **first
//! empty slot** of the pages in order (0x0055adc0). When every slot is
//! taken the object is made but put nowhere: it is never drawn, counted,
//! selected or removed again (lost). The object's `contentID` is `""` for
//! every `AddToBag` (0x005284e0 passes an empty string), so its name is
//! its id.
//!
//! **Removing** (`RemoveFromBag(name)` 0x0055f5c0): the stack, the
//! combination slots, then the pages' slots in order; the **first**
//! object named `name` is deleted (the selection cleared first if it is
//! that object). Other objects keep their slots. `RemoveFromBag()`
//! (0x0055fbd0) deletes the selected object, if any, and clears the
//! selection.
//!
//! **Selection** (`setSelected` 0x0055e950, the only writer of `+0x6fc`;
//! its callers: the object click 0x00560380, both `RemoveFromBag`, the
//! inventory's opening 0x0055fcb0, which re-selects the same object, and
//! the combination code 0x00560800, unreachable here): a click on an
//! object selects it; a click on the selected object calls
//! `OnInventoryObjectReselected` and keeps it. **Nothing else clears it**:
//! the selection survives closing the inventory, node changes, movies,
//! documents and a use on the warp (the warp's click 0x00524b20 only calls
//! `OnWarpObjectHit`); it ends when that object is removed (either
//! `RemoveFromBag`) or another is selected. The save does not hold it.
//!
//! **Double click** (0x00560380): the object clicks share one timer
//! (`+0x778`), started at the first click; each later click reads the time
//! since the previous read and, under 300 ms (`300000.0` µs, @ 0x005bee68,
//! strict), closes the inventory menu (0x0057c930) with the clicked
//! object selected.
//!
//! **Save** (0x0055d9e0 / 0x0054d630): `<Inventory>` holding a `<Stack>`
//! only when the stack exists, one `<CombinationSlot>` per combination
//! slot, then one `<Page>` per page with one `<Slot>` per slot, each with
//! its objects as `<Object id="…" />` (`contentID="…"` added only when not
//! empty).

use std::fmt;

use atlantis4_data::texts::{parse_labelled, Labelled};

use crate::savexml::XmlNode;

/// The double click's limit (@ 0x005bee68: `300000.0` µs).
pub const DOUBLE_CLICK_US: f64 = 300_000.0;

/// One object in a slot (`InventoryObject`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    /// `+0x5b4`: the id (`Objects.xml`, picture `<id>.png`).
    pub id: String,
    /// `+0x77c`: always empty in this game.
    pub content_id: String,
}

impl Object {
    /// The name the bindings compare (`contentID` when set, else the id).
    pub fn name(&self) -> &str {
        if self.content_id.is_empty() {
            &self.id
        } else {
            &self.content_id
        }
    }
}

/// Where an object sits: page, slot.
pub type SlotRef = (usize, usize);

/// What a click on an object did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectClick {
    /// The object is now selected.
    Selected,
    /// It was already selected: `OnInventoryObjectReselected` is called.
    Reselected(String),
    /// A second click under 300 ms: the object is selected and the menu
    /// closes.
    Selected2AndClose,
    Reselected2AndClose(String),
}

impl ObjectClick {
    /// The menu closes.
    pub fn closes(&self) -> bool {
        matches!(
            self,
            ObjectClick::Selected2AndClose | ObjectClick::Reselected2AndClose(_)
        )
    }

    /// The callback to run, if any.
    pub fn reselected(&self) -> Option<&str> {
        match self {
            ObjectClick::Reselected(n) | ObjectClick::Reselected2AndClose(n) => Some(n),
            _ => None,
        }
    }
}

/// Why `AddToBag` placed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lost(pub String);

impl fmt::Display for Lost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: every slot is taken; the object is put nowhere (as the original's 0x0055adc0)",
            self.0
        )
    }
}

/// The inventory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory {
    /// `pages[page][slot]`: the objects of each slot (one at most here).
    pub pages: Vec<Vec<Vec<Object>>>,
    /// The selected object.
    pub selected: Option<SlotRef>,
    /// The objects lost to full slots, for the log.
    pub lost: Vec<String>,
    /// `+0x6f8`: the page shown.
    pub page: usize,
    /// The object click timer (`+0x778`): the time of the last read, µs.
    click_timer: Option<f64>,
    /// `Objects.xml`: id, name, description.
    pub names: Vec<Labelled>,
}

impl Inventory {
    /// An empty inventory with `slots[page]` slots per page.
    pub fn new(slots: &[usize]) -> Inventory {
        Inventory {
            pages: slots.iter().map(|&n| vec![Vec::new(); n]).collect(),
            ..Inventory::default()
        }
    }

    /// The names and descriptions of `Objects.xml`.
    pub fn read_names(&mut self, data: &[u8]) {
        match parse_labelled(data, "Object") {
            Ok(n) => self.names = n,
            Err(e) => log::warn!("Objects.xml: {e}"),
        }
    }

    /// `Objects.xml`'s entry for `id` (0x0055b320 / 0x0055af80).
    pub fn label(&self, id: &str) -> Option<&Labelled> {
        self.names.iter().find(|l| l.id == id)
    }

    /// The object at `at`.
    pub fn object(&self, at: SlotRef) -> Option<&Object> {
        self.pages.get(at.0)?.get(at.1)?.first()
    }

    /// The objects in slot order with where they are.
    pub fn objects(&self) -> impl Iterator<Item = (SlotRef, &Object)> {
        self.pages.iter().enumerate().flat_map(|(p, page)| {
            page.iter()
                .enumerate()
                .flat_map(move |(s, slot)| slot.iter().map(move |o| ((p, s), o)))
        })
    }

    /// `AddToBag` / `AddToBagWithoutNotification`: the object in the
    /// first empty slot.
    pub fn add(&mut self, id: &str) -> Result<SlotRef, Lost> {
        for (p, page) in self.pages.iter_mut().enumerate() {
            for (s, slot) in page.iter_mut().enumerate() {
                if slot.is_empty() {
                    slot.push(Object {
                        id: id.to_string(),
                        content_id: String::new(),
                    });
                    return Ok((p, s));
                }
            }
        }
        self.lost.push(id.to_string());
        Err(Lost(id.to_string()))
    }

    /// `RemoveFromBag(name)`: the first object named `name` deleted; its
    /// slot, if one was.
    pub fn remove_named(&mut self, name: &str) -> Option<SlotRef> {
        let at = self
            .objects()
            .find(|(_, o)| o.name() == name)
            .map(|(at, _)| at)?;
        if self.selected == Some(at) {
            self.selected = None;
        }
        self.pages[at.0][at.1].remove(0);
        Some(at)
    }

    /// `RemoveFromBag()`: the selected object deleted, the selection
    /// cleared; its id.
    pub fn remove_selected(&mut self) -> Option<String> {
        let at = self.selected.take()?;
        let slot = &mut self.pages[at.0][at.1];
        (!slot.is_empty()).then(|| slot.remove(0).id)
    }

    /// The selected object's name (`Selected`'s comparison, 0x0055bb40).
    pub fn selected_name(&self) -> Option<&str> {
        self.selected
            .and_then(|at| self.object(at))
            .map(Object::name)
    }

    /// `Selected(name)` (0x0052b390): the selected object's name equals
    /// `name`; false when nothing is selected (the name is then `""`,
    /// and `""` is never asked).
    pub fn is_selected(&self, name: &str) -> bool {
        self.selected_name().unwrap_or("") == name
    }

    /// `ObjectCount(name)` (0x0055b6c0): the objects named `name`.
    pub fn count(&self, name: &str) -> usize {
        self.objects().filter(|(_, o)| o.name() == name).count()
    }

    /// A validated click on the object at `at`, at `now_us` on the
    /// engine clock (0x00560380).
    pub fn click(&mut self, at: SlotRef, now_us: f64) -> Option<ObjectClick> {
        let name = self.object(at)?.name().to_string();
        let again = self.selected == Some(at);
        self.selected = Some(at);
        let double = match self.click_timer.replace(now_us) {
            None => false,
            Some(t) => now_us - t < DOUBLE_CLICK_US,
        };
        Some(match (again, double) {
            (false, false) => ObjectClick::Selected,
            (true, false) => ObjectClick::Reselected(name),
            (false, true) => ObjectClick::Selected2AndClose,
            (true, true) => ObjectClick::Reselected2AndClose(name),
        })
    }

    /// A drop of the object at `from` on slot `to` of the page shown
    /// (0x0055df70): moved when that slot is empty; on a taken slot only
    /// a combination of the two could happen (0x0055dde0), and none of
    /// `Combinations.xml` names an object of this game, so nothing does.
    pub fn drop_on_slot(&mut self, from: SlotRef, to: SlotRef) -> bool {
        if from == to || self.object(from).is_none() {
            return false;
        }
        let Some(slot) = self.pages.get(to.0).and_then(|p| p.get(to.1)) else {
            return false;
        };
        if !slot.is_empty() {
            return false;
        }
        let o = self.pages[from.0][from.1].remove(0);
        self.pages[to.0][to.1].push(o);
        if self.selected == Some(from) {
            self.selected = Some(to);
        }
        true
    }

    /// The text of the description box for the selected object: the
    /// file's `textFormat` with `%s` = the name, then the description
    /// (0x0055e950 @ 0x0055ed49..0x0055ee77); empty without selection.
    pub fn description(&self, text_format: &str) -> String {
        let Some(name) = self.selected_name() else {
            return String::new();
        };
        let (n, d) = self
            .label(name)
            .map(|l| (l.name.as_str(), l.description.as_str()))
            .unwrap_or(("", ""));
        let mut out = String::new();
        let mut args = [n, d].into_iter();
        let mut rest = text_format;
        while let Some(i) = rest.find("%s") {
            out.push_str(&rest[..i]);
            out.push_str(args.next().unwrap_or(""));
            rest = &rest[i + 2..];
        }
        out.push_str(rest);
        out
    }

    /// The save's `<Inventory>` element.
    pub fn to_xml(&self) -> XmlNode {
        let mut inv = XmlNode::new("Inventory");
        for page in &self.pages {
            let mut p = XmlNode::new("Page");
            for slot in page {
                let mut s = XmlNode::new("Slot");
                for o in slot {
                    let mut e = XmlNode::new("Object").attr("id", &o.id);
                    if !o.content_id.is_empty() {
                        e = e.attr("contentID", &o.content_id);
                    }
                    s.children.push(e);
                }
                p.children.push(s);
            }
            inv.children.push(p);
        }
        inv
    }

    /// The objects back from a save's `<Inventory>` (0x00561910): each
    /// `<Page>`'s `<Slot>`s in order; no selection.
    pub fn load_xml(&mut self, el: &atlantis4_data::xml::Element) {
        for page in self.pages.iter_mut() {
            page.iter_mut().for_each(Vec::clear);
        }
        self.selected = None;
        for (p, page) in el.children_named("Page").enumerate() {
            for (s, slot) in page.children_named("Slot").enumerate() {
                let Some(dst) = self.pages.get_mut(p).and_then(|pg| pg.get_mut(s)) else {
                    log::warn!("save: no slot {s} on page {p}");
                    continue;
                };
                for o in slot.children_named("Object") {
                    dst.push(Object {
                        id: o.attr("id").unwrap_or_default().to_string(),
                        content_id: o.attr("contentID").unwrap_or_default().to_string(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bag() -> Inventory {
        Inventory::new(&[8])
    }

    #[test]
    fn objects_fill_the_first_empty_slot_and_a_ninth_is_lost() {
        let mut b = bag();
        for i in 0..8 {
            assert_eq!(b.add(&format!("O{i}")), Ok((0, i)));
        }
        assert_eq!(b.add("NINE"), Err(Lost("NINE".into())));
        assert_eq!(b.count("NINE"), 0);
        b.remove_named("O3");
        assert_eq!(b.add("TEN"), Ok((0, 3)));
        assert_eq!(b.object((0, 4)).unwrap().id, "O4");
    }

    #[test]
    fn the_selection_lasts_until_its_object_is_removed() {
        let mut b = bag();
        b.add("COUTEAU").unwrap();
        b.add("CASQUE").unwrap();
        assert!(!b.is_selected("COUTEAU"));
        assert_eq!(b.click((0, 0), 0.0), Some(ObjectClick::Selected));
        assert!(b.is_selected("COUTEAU"));
        // Another object removed: the selection stays.
        b.remove_named("CASQUE");
        assert!(b.is_selected("COUTEAU"));
        assert_eq!(b.remove_selected().as_deref(), Some("COUTEAU"));
        assert!(!b.is_selected("COUTEAU"));
        assert_eq!(b.selected, None);
        assert_eq!(b.remove_selected(), None);
    }

    #[test]
    fn removing_the_selected_object_by_name_clears_the_selection() {
        let mut b = bag();
        b.add("A").unwrap();
        b.add("A").unwrap();
        b.click((0, 1), 0.0);
        // The first `A` goes: the selected second one stays selected.
        assert_eq!(b.remove_named("A"), Some((0, 0)));
        assert!(b.is_selected("A"));
        assert_eq!(b.remove_named("A"), Some((0, 1)));
        assert_eq!(b.selected, None);
    }

    #[test]
    fn clicks_under_300_ms_close_the_menu() {
        let mut b = bag();
        b.add("A").unwrap();
        b.add("B").unwrap();
        assert_eq!(b.click((0, 0), 1_000_000.0), Some(ObjectClick::Selected));
        assert_eq!(
            b.click((0, 0), 1_299_999.0),
            Some(ObjectClick::Reselected2AndClose("A".into()))
        );
        assert_eq!(b.click((0, 1), 1_599_999.0), Some(ObjectClick::Selected));
        assert_eq!(
            b.click((0, 1), 2_000_000.0),
            Some(ObjectClick::Reselected("B".into()))
        );
        assert_eq!(b.click((0, 5), 2_000_001.0), None);
    }

    #[test]
    fn the_description_fills_the_format() {
        let mut b = bag();
        b.names = vec![Labelled {
            id: "A".into(),
            name: "Knife".into(),
            description: "Sharp".into(),
        }];
        b.add("A").unwrap();
        assert_eq!(b.description("[%s|%s]"), "");
        b.click((0, 0), 0.0);
        assert_eq!(b.description("[%s|%s]"), "[Knife|Sharp]");
    }

    #[test]
    fn a_drop_moves_to_an_empty_slot_only() {
        let mut b = bag();
        b.add("A").unwrap();
        b.add("B").unwrap();
        b.click((0, 0), 0.0);
        assert!(!b.drop_on_slot((0, 0), (0, 1)));
        assert!(b.drop_on_slot((0, 0), (0, 5)));
        assert!(b.is_selected("A"));
        assert_eq!(b.object((0, 5)).unwrap().id, "A");
        assert_eq!(b.add("C"), Ok((0, 0)));
    }

    #[test]
    fn the_save_element_nests_pages_slots_and_objects() {
        let mut b = Inventory::new(&[2]);
        b.add("ALLUMETTES").unwrap();
        let x = b.to_xml().print(0);
        assert_eq!(
            x,
            "<Inventory>\n    <Page>\n        <Slot>\n            <Object id=\"ALLUMETTES\" />\n        </Slot>\n        <Slot />\n    </Page>\n</Inventory>"
        );
        let el = atlantis4_data::xml::parse(x.as_bytes()).unwrap();
        let mut c = Inventory::new(&[2]);
        c.load_xml(&el[0]);
        assert_eq!(c.pages, b.pages);
    }
}
