//! The documents browser, the shown document (`ShowDocument`), the diary
//! and the puzzle timer as far as the scripts and the save reach them
//! (atlantis4-gog-en-2363910; `DocumentsBrowser` at `Application+0x5960`,
//! the diary at `+0x63b4`, the puzzle timer at `game+0xda0`).
//!
//! **Browser** (`STATIC`): its slots are the layouts `page<N>Slot<M>` of
//! `DocumentsBrowser/DocumentsBrowser.lua` (pages `page<N>` from 0 while
//! they exist: 7 pages, 16 + 6 × 3 slots in the data). `AddDocument(name)`
//! (0x00559520 → 0x00558f80) puts a new `Document` in the **first slot
//! without a child**, pages then slots in order; with none free it is
//! deleted. The browser itself has no way in but the menu's invisible
//! `documentsButton` (a 1 × 1 px button without picture), and the one `AddDocument` call of the
//! scripts sits in a branch no object of this game reaches; the model is
//! kept for the save.
//!
//! **Shown document** (`ShowDocument(name)` → 0x00557840 with page 0): the
//! picture `DocumentsBrowser/Documents/<name>_zoomed_<page>.png`, else
//! `.jpg`, goes into `zoomedSprite` (`LetterBox` over
//! `DocumentsBrowser/ZoomBackground.png`, the full-screen `zoomed`
//! button, a child of the inventory menu inserted last, so above the
//! inventory and its menu); with neither, the document is hidden at once
//! (`HideDocument`, 0x00557020: `zoomed` hidden, then the scripts'
//! `OnDocumentClosed(name)`). The click count is reset.
//!
//! **Clicks on `zoomed`** (0x00558f00, `STATIC`): its validation sound
//! `Sounds/SFX/Clic_Deb-fin.wav` plays on each validated click (the
//! button's). The first click starts a timer whose alarm, 200 ms later
//! (`now + 200000` µs), calls 0x00558d70; a second click calls it at
//! once. 0x00558d70 reads the time since the first click and stops the
//! timer: under 200 ms (`200000.0`, the double click) it plays
//! `Sounds/SFX/Clic_News-GPS.ogg` and hides the document; otherwise it
//! shows page `page + 1`, which hides the document when that page has no
//! picture. The first node's tutorials have one page each, so one click
//! closes them 200 ms later.
//!
//! **Save** (0x00553f50): `<DocumentsBrowser>`, one `<Page>` per page, one
//! `<Slot>` per slot, its documents as `<Document id="…" />`. The diary
//! (0x0056b000): `<Diary>` with one `<Page id="…" />` per page set by
//! `SetDiaryPage` (never called: `<Diary />`). The puzzle timer
//! (0x00520ed0): `<PuzzleTimer>` with `<PuzzleEnCours value="%d" />`
//! (`+0x210`, 0 from the game's constructor @ 0x005288e3) and
//! `<TimeLeft value="%d" />` (`+0x20c`, written only when a timer starts
//! or loads: `[UNKNOWN]` before that, taken as 0).

use crate::savexml::XmlNode;

/// The alarm after a first click on the shown document, µs.
pub const SINGLE_CLICK_US: f64 = 200_000.0;
/// The sound of a double click on the shown document.
pub const DOUBLE_CLICK_SOUND: &str = "Sounds/SFX/Clic_News-GPS.ogg";
/// The browser's folder of documents (`DocumentsBrowser/Documents/`).
pub const DOCUMENTS_DIR: &str = "DocumentsBrowser/Documents/";

/// The picture of page `page` of document `name`, without its extension
/// (`.png` then `.jpg` are tried).
pub fn page_stem(name: &str, page: u32) -> String {
    format!("{DOCUMENTS_DIR}{name}_zoomed_{page}")
}

/// A document on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub name: String,
    pub page: u32,
    /// The picture's path in the install.
    pub picture: String,
    /// Clicks since it was shown (`+0x938`).
    clicks: u32,
    /// When the first click's timer started (µs), its alarm 200 ms later.
    first_click: Option<f64>,
}

/// What the shown document's update or click asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum DocAction {
    None,
    /// Show the next page (hidden when it has no picture).
    NextPage(String, u32),
    /// The double click: play [`DOUBLE_CLICK_SOUND`] and hide.
    Hide,
}

/// The documents browser and the shown document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Documents {
    /// `pages[page][slot]`: the documents of each slot.
    pub pages: Vec<Vec<Vec<String>>>,
    /// `+0x774`: the name `ShowDocument` last set (`""` after a hide).
    pub name: String,
    pub shown: Option<Shown>,
}

impl Documents {
    pub fn new(slots: &[usize]) -> Documents {
        Documents {
            pages: slots.iter().map(|&n| vec![Vec::new(); n]).collect(),
            name: String::new(),
            shown: None,
        }
    }

    /// `AddDocument(name)`: into the first empty slot; false (dropped)
    /// when there is none.
    pub fn add(&mut self, name: &str) -> bool {
        for page in &mut self.pages {
            for slot in page.iter_mut() {
                if slot.is_empty() {
                    slot.push(name.to_string());
                    return true;
                }
            }
        }
        false
    }

    /// `ShowDocument` found `picture` for page `page` of `name`.
    pub fn show(&mut self, name: &str, page: u32, picture: String) {
        self.name = name.to_string();
        self.shown = Some(Shown {
            name: name.to_string(),
            page,
            picture,
            clicks: 0,
            first_click: None,
        });
    }

    /// `ShowDocument` found no picture for `name`: the name is set, then
    /// the document hidden (0x00557840 → 0x00557020).
    pub fn show_missing(&mut self, name: &str) -> String {
        self.name = name.to_string();
        self.hide()
    }

    /// `HideDocument` (0x00557020): the name, cleared, for
    /// `OnDocumentClosed` (called even when nothing was shown).
    pub fn hide(&mut self) -> String {
        self.shown = None;
        std::mem::take(&mut self.name)
    }

    /// A validated click on `zoomed` at `now_us`.
    pub fn click(&mut self, now_us: f64) -> DocAction {
        let Some(s) = self.shown.as_mut() else {
            return DocAction::None;
        };
        s.clicks += 1;
        match s.clicks {
            1 => {
                s.first_click = Some(now_us);
                DocAction::None
            }
            2 => Self::fire(s, now_us),
            _ => DocAction::None,
        }
    }

    /// The timer's alarm (0x00558d70 by the alarm signal) at `now_us`.
    pub fn update(&mut self, now_us: f64) -> DocAction {
        let Some(s) = self.shown.as_mut() else {
            return DocAction::None;
        };
        match s.first_click {
            Some(t) if now_us >= t + SINGLE_CLICK_US => Self::fire(s, now_us),
            _ => DocAction::None,
        }
    }

    /// 0x00558d70: the timer read and stopped, then hide or next page.
    fn fire(s: &mut Shown, now_us: f64) -> DocAction {
        let Some(t) = s.first_click.take() else {
            return DocAction::None;
        };
        if now_us - t < SINGLE_CLICK_US {
            DocAction::Hide
        } else {
            DocAction::NextPage(s.name.clone(), s.page + 1)
        }
    }

    /// The save's `<DocumentsBrowser>`.
    pub fn to_xml(&self) -> XmlNode {
        let mut root = XmlNode::new("DocumentsBrowser");
        for page in &self.pages {
            let mut p = XmlNode::new("Page");
            for slot in page {
                let mut s = XmlNode::new("Slot");
                s.children = slot
                    .iter()
                    .map(|d| XmlNode::new("Document").attr("id", d))
                    .collect();
                p.children.push(s);
            }
            root.children.push(p);
        }
        root
    }

    /// The documents back from a save (0x00559e60).
    pub fn load_xml(&mut self, el: &atlantis4_data::xml::Element) {
        for page in &mut self.pages {
            page.iter_mut().for_each(Vec::clear);
        }
        for (p, page) in el.children_named("Page").enumerate() {
            for (s, slot) in page.children_named("Slot").enumerate() {
                if let Some(dst) = self.pages.get_mut(p).and_then(|pg| pg.get_mut(s)) {
                    dst.extend(
                        slot.children_named("Document")
                            .map(|d| d.attr("id").unwrap_or_default().to_string()),
                    );
                }
            }
        }
    }
}

/// The diary: the pages `SetDiaryPage` set (never, in this game).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diary {
    pub pages: Vec<String>,
}

impl Diary {
    pub fn to_xml(&self) -> XmlNode {
        let mut d = XmlNode::new("Diary");
        d.children = self
            .pages
            .iter()
            .map(|p| XmlNode::new("Page").attr("id", p))
            .collect();
        d
    }
}

/// The puzzle timer's saved fields (dormant in this game).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PuzzleTimer {
    pub in_progress: i32,
    pub time_left: i32,
}

impl PuzzleTimer {
    pub fn to_xml(&self) -> XmlNode {
        let mut t = XmlNode::new("PuzzleTimer");
        t.children
            .push(XmlNode::new("PuzzleEnCours").attr("value", &self.in_progress.to_string()));
        t.children
            .push(XmlNode::new("TimeLeft").attr("value", &self.time_left.to_string()));
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_click_turns_the_page_200_ms_later() {
        let mut d = Documents::new(&[2]);
        d.show("Tuto_360", 0, "p.png".into());
        assert_eq!(d.click(1_000_000.0), DocAction::None);
        assert_eq!(d.update(1_199_999.0), DocAction::None);
        assert_eq!(
            d.update(1_200_000.0),
            DocAction::NextPage("Tuto_360".into(), 1)
        );
        assert_eq!(d.update(1_400_000.0), DocAction::None);
    }

    #[test]
    fn a_double_click_hides() {
        let mut d = Documents::new(&[2]);
        d.show("Tuto_360", 0, "p.png".into());
        d.click(0.0);
        assert_eq!(d.click(150_000.0), DocAction::Hide);
        assert_eq!(d.hide(), "Tuto_360");
        assert_eq!(d.hide(), "");
        assert_eq!(d.show_missing("Nope"), "Nope");
        assert!(d.shown.is_none());
    }

    #[test]
    fn documents_take_the_first_free_slot_and_save_by_page() {
        let mut d = Documents::new(&[1, 1]);
        assert!(d.add("A"));
        assert!(d.add("B"));
        assert!(!d.add("C"));
        assert_eq!(
            d.to_xml().print(0),
            "<DocumentsBrowser>\n    <Page>\n        <Slot>\n            <Document id=\"A\" />\n        </Slot>\n    </Page>\n    <Page>\n        <Slot>\n            <Document id=\"B\" />\n        </Slot>\n    </Page>\n</DocumentsBrowser>"
        );
        assert_eq!(Diary::default().to_xml().print(0), "<Diary />");
        assert_eq!(
            PuzzleTimer::default().to_xml().print(0),
            "<PuzzleTimer>\n    <PuzzleEnCours value=\"0\" />\n    <TimeLeft value=\"0\" />\n</PuzzleTimer>"
        );
    }
}
