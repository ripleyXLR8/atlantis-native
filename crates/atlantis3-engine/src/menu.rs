//! The menu pages (loop mode 6) built from `menu\desc\<page>.atl`: the
//! elements drawn in file order at their position, or at their bank's
//! default position when the file gives none, onto the page surface
//! copied to the screen (@ 0x004591a0 → @ 0x00425bc0).
//!
//! The start page `i_debut` (@ 0x00452000): the background, five player
//! buttons (frame 1 while the pointer is on one, else 0) and the quit
//! button (an animation whose frame advances every 1/15 s while hovered,
//! frame 0 otherwise). The hovered element is the first displayed and
//! selectable one whose current sprite is opaque under the pointer
//! (@ 0x004261d0, `SPR_COLLISION`). A click (on release) on player n
//! starts a new game when `game<n>_1.sav` does not exist, else loads the
//! newest save of player n. The other pages' elements are drawn the same
//! way; their behaviours are not ported yet.

use atlantis3_data::atl::{menu_sprite_path, Element, ElementType, Layout};
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::A3Files;

/// One element and its bank.
pub struct Item {
    pub el: Element,
    pub bank: Option<Bank>,
    /// The animation's frame counter and its last step (`BT_ANIME2`).
    pub counter: i32,
    pub counter_time: f64,
    counter_started: bool,
    counter_base: i32,
    pub hovered: bool,
}

/// A page.
pub struct Page {
    pub name: String,
    pub items: Vec<Item>,
}

/// One sprite drawn: the bank's name, the frame, and the position (None:
/// the default one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draw {
    pub bank: String,
    pub frame: usize,
    pub pos: Option<(i32, i32)>,
}

impl Page {
    /// The page `name` (`i_debut`, `i_save`, ...) for a screen `w` x `h`.
    pub fn load(files: &A3Files, name: &str, w: usize, h: usize) -> Option<Page> {
        let path = atlantis3_data::atl::layout_path(name);
        let data = files.read(path.trim_start_matches("Datas/"))?;
        let layout = Layout::parse(&data, (w as i32, h as i32));
        let items = layout
            .elements
            .into_iter()
            .map(|el| {
                let bank = el.nomspr.as_ref().and_then(|n| {
                    let p = menu_sprite_path(w as i32, n);
                    files
                        .read(p.trim_start_matches("Datas/"))
                        .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok())
                });
                Item {
                    el,
                    bank,
                    counter: 0,
                    counter_time: 0.0,
                    counter_started: false,
                    counter_base: 0,
                    hovered: false,
                }
            })
            .collect();
        Some(Page {
            name: name.to_string(),
            items,
        })
    }

    fn position(el: &Element) -> Option<(i32, i32)> {
        el.has_pos().then_some((el.pos[0], el.pos[1]))
    }

    /// The frame an element shows.
    fn frame_of(it: &Item) -> usize {
        match it.el.element_type() {
            ElementType::BtAnime => usize::from(it.hovered),
            ElementType::BtAnime2 => {
                if it.hovered {
                    it.counter.max(0) as usize
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// The hover test (@ 0x004261d0): the first displayed and selectable
    /// element whose current sprite is opaque at the pointer.
    pub fn hit(&self, pointer: (i32, i32)) -> Option<usize> {
        self.items.iter().position(|it| {
            it.el.displayed
                && it.el.selectable
                && it.bank.as_ref().is_some_and(|b| {
                    let (x, y) = match Self::position(&it.el) {
                        Some((x, y)) => (Some(x), y),
                        None => (None, 0),
                    };
                    b.opaque_at(Self::frame_of(it), x, y, pointer.0, pointer.1)
                })
        })
    }

    /// One frame of the page at `now`: the hover, the animations, the
    /// elements drawn; the draws made.
    pub fn frame(
        &mut self,
        screen: &mut [u16],
        w: usize,
        h: usize,
        pointer: (i32, i32),
        now: f64,
    ) -> Vec<Draw> {
        let hit = self.hit(pointer);
        for (k, it) in self.items.iter_mut().enumerate() {
            it.hovered = hit == Some(k);
        }
        let mut out = Vec::new();
        for it in &self.items {
            if !it.el.displayed {
                continue;
            }
            let Some(bank) = &it.bank else { continue };
            let frame = Self::frame_of(it);
            let pos = Self::position(&it.el);
            let mut t = Target::screen(screen, w, h);
            t.xmax = w as i32;
            t.ymax = h as i32;
            match pos {
                Some((x, y)) => bank.draw(&mut t, Some(x), y, frame),
                None => bank.draw(&mut t, None, 0, frame),
            };
            out.push(Draw {
                bank: it
                    .el
                    .nomspr
                    .as_ref()
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default(),
                frame,
                pos,
            });
        }
        // The animations step after the draw: the first hovered frame
        // counts one, then one every frame time.
        for it in self.items.iter_mut() {
            if it.el.element_type() != ElementType::BtAnime2 || !it.hovered {
                it.counter_started = false;
                continue;
            }
            let n = it.bank.as_ref().map_or(1, |b| b.len().max(1)) as i32;
            if !it.counter_started {
                // The first hovered frame counts one at once.
                it.counter_started = true;
                it.counter_base = it.counter;
                it.counter = (it.counter + 1) % n;
                it.counter_time = now;
            } else {
                // Then the frame of the clock since the hover began, one
                // every frame time [the original's run, golden frames].
                let c = it.counter_base
                    + ((now - it.counter_time) / f64::from(it.el.frame_time)) as i32;
                if c > it.counter {
                    it.counter = c % n;
                }
            }
        }
        out
    }

    /// The element a click (on release) hits: its `nomref`.
    pub fn clicked(&self, pointer: (i32, i32)) -> Option<&[u8]> {
        let k = self.items.iter().position(|it| {
            it.el.displayed
                && it.el.clickable
                && it.bank.as_ref().is_some_and(|b| {
                    let (x, y) = match Self::position(&it.el) {
                        Some((x, y)) => (Some(x), y),
                        None => (None, 0),
                    };
                    b.opaque_at(Self::frame_of(it), x, y, pointer.0, pointer.1)
                })
        })?;
        Some(&self.items[k].el.nomref)
    }
}
