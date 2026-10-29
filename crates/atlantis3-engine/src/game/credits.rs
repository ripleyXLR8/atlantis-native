//! The credits: menu page 9, `i_credit` (`menu\desc\i_credit.atl` and
//! the page texts of `loc\text\<lang>\i_credit.txt`), what the game's end
//! (`EXIT`, @ 0x00449ceb) and the start page's `BT_QUIT` (@ 0x00452c38)
//! show. A slide show that never ends: 31 text pages (`PAGE0`..`PAGE30`,
//! each `link`ed to the next, `PAGE30` to `PAGE0`) over 10 full-screen
//! pictures (`IMGFOND1`..`IMGFOND10`); a new text page when the picture's
//! clock passes 3 s or on a click, a new picture every second page.
//! Escape writes the configuration and quits the program.
//!
//! - start @ 0x00457690: every element hidden and given the text box
//!   (0, 0.4 W, W, 0.8 H) and the font `AtlFont_Credit` (`AtlFont_Titre`
//!   for `PAGE1`); slide 1; `IMGFOND1` shown with its clock started;
//!   page 9; `PAGE0` shown and current;
//! - frame (mode 6 → @ 0x004591a0 → page 9 → @ 0x00457380): the click
//!   test (@ 0x00426470), the elements drawn into the page surface
//!   (@ 0x00425bc0: the shown picture), then the turn; the page surface
//!   copied to the screen (@ 0x004591ec), the shown pages' texts
//!   (@ 0x00426170 → @ 0x004149a0), the pointer (@ 0x004281e0);
//! - Escape (@ 0x0042ab20 → @ 0x0042ac50 → @ 0x00457360): the page
//!   closed, `AtlConfig_MainWrite` (@ 0x00408f80), the program's exit
//!   (@ 0x004d6aa0 → @ 0x004dd9e0).
//!
//! The texts are drawn by the port's text renderer (the original's go
//! through GDI, [`crate::text`]).

use atlantis3_data::atl::{self, Element, ElementType, Layout};
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::A3Files;

use super::{Game, Mode};
use crate::text::{FontId, Zone};

/// The text box's left edge and bottom, fractions of the screen (the
/// doubles @ 0x00700500 = 0.4 and @ 0x007004f8 = 0.8, read @ 0x0045778e,
/// @ 0x004577c3).
const BOX_LEFT: f64 = 0.4;
const BOX_BOTTOM: f64 = 0.8;
/// A page's time (the double @ 0x007004f0, compared @ 0x0045746a: the
/// page turns when the elapsed time is above it, strictly).
const PAGE_TIME: f64 = 3.0;
/// The pictures (`IMGFOND%d` @ 0x0071b604, 1..10, @ 0x004573ab).
const SLIDES: i32 = 10;

/// One text call (`AtlFont_SetLayout` @ 0x0041e190, then
/// `AtlFont_Print` @ 0x0041e2c0): the font named by the element, the page
/// text, the colour (r, g, b), the layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Print {
    pub font: Vec<u8>,
    pub text: Vec<u8>,
    pub rgb: [i32; 3],
    pub zone: Zone,
}

/// One element of the page (a 0x2288-byte record of the interface
/// @ 0x0077e5c8).
pub struct Item {
    pub el: Element,
    pub bank: Option<Bank>,
    /// Shown (+0x221f).
    pub shown: bool,
    /// The text box (+0x80, +0x84, +0x88, +0x8c) as the start sets it.
    pub text_box: [i32; 4],
    /// The clock: its start (+0x2230), the time added (+0x2240) and the
    /// elapsed time saved (+0x222c, a float).
    t0: f64,
    acc: f64,
    saved: f32,
}

impl Item {
    /// The clock restarted (@ 0x00457629..0x0045766d, @ 0x00457902..
    /// 0x00457968): zeroed, the start := now, the saved time := the
    /// elapsed time then (two reads of the same clock: 0).
    fn restart(&mut self, now: f64) {
        self.acc = 0.0;
        self.t0 = now;
        self.saved = ((now - self.t0) + self.acc) as f32;
    }

    /// The clock's time (@ 0x00457451..0x00457468).
    fn elapsed(&self, now: f64) -> f64 {
        (now - self.t0) + self.acc - f64::from(self.saved)
    }
}

/// The credits page.
pub struct Credits {
    pub items: Vec<Item>,
    /// The picture shown (@ 0x0071b1e0, 1..10).
    pub slide: i32,
    /// The text page shown (@ 0x0077e5dc).
    pub current: Option<usize>,
    /// Every second turn changes the picture (@ 0x0077fa5a; a global
    /// that the start does not set: 0 at the first start).
    toggle: bool,
    /// The page surface (@ 0x0077fb00), copied to the screen each frame.
    pub surface: Vec<u16>,
    /// The last frame's sprites (`bank#frame@x,y`) and text calls.
    pub draws: Vec<String>,
    pub prints: Vec<Print>,
    /// The configuration Escape wrote (`Datas\Atlantis3.cfg`, @ 0x00408f80).
    pub config: Option<Vec<u8>>,
}

impl Credits {
    /// The page `i_credit` for a screen `w` × `h` (the start-up's menu
    /// load, @ 0x00451570: the layout, its page texts, the banks).
    pub fn load(files: &A3Files, language: &str, w: usize, h: usize) -> Option<Credits> {
        let path = atl::layout_path("i_credit");
        let data = files.read(path.trim_start_matches("Datas/"))?;
        let mut layout = Layout::parse(&data, (w as i32, h as i32));
        let tips = atl::tooltip_path(language, "i_credit");
        if let Some(t) = files.read(tips.trim_start_matches("Datas/")) {
            for name in layout.apply_tooltips(&atl::parse_tooltips(&t)) {
                log::warn!("on a pas trouve {}", String::from_utf8_lossy(&name));
            }
        }
        let items = layout
            .elements
            .into_iter()
            .map(|el| {
                let bank = el.nomspr.as_ref().and_then(|n| {
                    let p = atl::menu_sprite_path(w as i32, n);
                    files
                        .read(p.trim_start_matches("Datas/"))
                        .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok())
                });
                Item {
                    shown: el.displayed,
                    el,
                    bank,
                    text_box: [0; 4],
                    t0: 0.0,
                    acc: 0.0,
                    saved: 0.0,
                }
            })
            .collect();
        Some(Credits {
            items,
            slide: 1,
            current: None,
            toggle: false,
            surface: vec![0; w * h],
            draws: Vec::new(),
            prints: Vec::new(),
            config: None,
        })
    }

    /// The element named `name` (@ 0x00426db0: the first one).
    pub fn find(&self, name: &[u8]) -> Option<usize> {
        self.items.iter().position(|it| it.el.nomref == name)
    }

    fn picture(&self, n: i32) -> Option<usize> {
        self.find(format!("IMGFOND{n}").as_bytes())
    }

    /// The names of the shown elements, in the page's order.
    pub fn shown_names(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|it| it.shown)
            .map(|it| String::from_utf8_lossy(&it.el.nomref).into_owned())
            .collect()
    }

    /// The start (@ 0x00457690) at the game's time `now`.
    fn start(&mut self, w: usize, h: usize, now: f64) {
        // Every element hidden, its font and its text box (@ 0x004576c4..
        // 0x004577e1; the font compare @ 0x00457724 is `!=`).
        let left = (w as f64 * BOX_LEFT) as i32;
        let bottom = (h as f64 * BOX_BOTTOM) as i32;
        for it in &mut self.items {
            it.shown = false;
            it.el.font = if it.el.nomref != b"PAGE1" {
                b"AtlFont_Credit".to_vec()
            } else {
                b"AtlFont_Titre".to_vec()
            };
            it.text_box = [0, left, w as i32, bottom];
        }
        // Slide 1 (@ 0x004577eb); `PAGE2`..`PAGE10`, `IMGFOND2`..
        // `IMGFOND10` hidden (already); `IMGFOND1` shown, its clock
        // started; `PAGE0` shown and current.
        self.slide = 1;
        if let Some(k) = self.picture(1) {
            self.items[k].shown = true;
            self.items[k].restart(now);
        }
        self.current = self.find(b"PAGE0");
        if let Some(k) = self.current {
            self.items[k].shown = true;
        }
    }

    /// The click test (@ 0x00426470): the first shown, clickable element
    /// opaque under the pointer (here a picture: the pages are not
    /// clickable).
    fn clicked(&self, pointer: (i32, i32)) -> Option<usize> {
        self.items.iter().position(|it| {
            it.shown
                && it.el.clickable
                && it.bank.as_ref().is_some_and(|b| {
                    let x = it.el.has_pos().then_some(it.el.pos[0]);
                    let y = if it.el.has_pos() { it.el.pos[1] } else { 0 };
                    b.opaque_at(0, x, y, pointer.0, pointer.1)
                })
        })
    }

    /// The elements drawn into the page surface (@ 0x00425bc0): the shown
    /// ones in order, frame 0 (`IMG`), the texts skipped; a shown element
    /// without its bank is hidden ("Il manque %s").
    fn draw(&mut self, w: usize, h: usize) {
        self.draws.clear();
        self.surface.fill(0);
        for it in &mut self.items {
            if !it.shown
                || matches!(
                    it.el.element_type(),
                    ElementType::ClickText | ElementType::ZoneText
                )
            {
                continue;
            }
            let Some(bank) = &it.bank else {
                log::warn!("Il manque {}", String::from_utf8_lossy(&it.el.nomref));
                it.shown = false;
                continue;
            };
            let mut t = Target::screen(&mut self.surface, w, h);
            t.xmax = w as i32;
            t.ymax = h as i32;
            let name = it
                .el
                .nomspr
                .as_ref()
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default();
            if it.el.has_pos() {
                let (x, y) = (it.el.pos[0], it.el.pos[1]);
                bank.draw(&mut t, Some(x), y, 0);
                self.draws.push(format!("{name}#0@{x},{y}"));
            } else {
                bank.draw(&mut t, None, 0, 0);
                self.draws.push(format!("{name}#0@def"));
            }
        }
    }

    /// The turn (@ 0x004573a3..0x00457678): nothing when no element was
    /// clicked and the shown picture's clock is at most 3 s; else the
    /// current page hidden, its `link` shown and made current, every
    /// second time the picture n hidden and n + 1 shown (after 10, 1), and
    /// the picture's clock restarted.
    fn turn(&mut self, now: f64, clicked: bool) {
        let next = if self.slide + 1 > SLIDES {
            1
        } else {
            self.slide + 1
        };
        let picture = self.picture(self.slide);
        if !clicked {
            let Some(p) = picture else {
                return;
            };
            if self.items[p].elapsed(now) <= PAGE_TIME {
                return;
            }
        }
        let Some(cur) = self.current else {
            return;
        };
        self.items[cur].shown = false;
        let link = self.items[cur].el.link.clone().unwrap_or_default();
        let Some(page) = self.find(&link) else {
            log::warn!("credits: no page {}", String::from_utf8_lossy(&link));
            self.current = None;
            return;
        };
        self.items[page].shown = true;
        self.current = Some(page);
        let mut clock = picture;
        if self.toggle {
            if let Some(p) = picture {
                self.items[p].shown = false;
            }
            clock = self.picture(next);
            if let Some(n) = clock {
                self.items[n].shown = true;
            }
            self.slide = next;
        }
        self.toggle = !self.toggle;
        if let Some(k) = clock {
            self.items[k].restart(now);
        }
    }

    /// The texts of the shown elements with `text on` (@ 0x00426170 →
    /// @ 0x004149a0): the layout's margins from the text box,
    /// (x0, W − x1, y0, H − y1) with x0 = +0x84, y0 = +0x80, x1 = +0x88,
    /// y1 = +0x8c, the element's alignments (+0x9c, +0xa0); its font, page
    /// text and colour (+0x90..+0x98).
    fn texts(&self, w: usize, h: usize) -> Vec<Print> {
        self.items
            .iter()
            .filter(|it| it.shown && it.el.text)
            .map(|it| {
                let b = it.text_box;
                Print {
                    font: it.el.font.clone(),
                    text: it.el.page_text.clone().unwrap_or_default(),
                    rgb: it.el.color,
                    zone: Zone::new(
                        b[1],
                        w as i32 - b[2],
                        b[0],
                        h as i32 - b[3],
                        it.el.align_h,
                        it.el.align_v,
                    ),
                }
            })
            .collect()
    }
}

/// The text id of a font name.
fn font_id(name: &[u8]) -> FontId {
    match name {
        b"AtlFont_Titre" => FontId::Titre,
        b"AtlFont_Credit" => FontId::Credit,
        b"AtlFont_Bulle" => FontId::Bulle,
        b"AtlFont_Special" => FontId::Special,
        _ => FontId::Default,
    }
}

impl Game {
    /// `EXIT` (@ 0x00449ce1) or `BT_QUIT`: loop mode 6 (@ 0x004d6b00) and
    /// the credits' start (@ 0x00457690); the click flags cleared
    /// (@ 0x00451560).
    pub fn start_credits(&mut self) {
        let lang = self.startup.text_language();
        let Some(mut c) = Credits::load(&self.files, &lang, self.width, self.height) else {
            log::warn!("menu\\desc\\i_credit.atl: not found, no credits");
            self.quit = true;
            return;
        };
        c.start(self.width, self.height, self.now);
        if self.mode != Mode::Menu {
            self.prev_mode = self.mode;
        }
        self.page = None;
        self.mode = Mode::Menu;
        self.credits = Some(c);
        self.click = false;
        self.right_clicked = false;
    }

    /// The credits shown.
    pub fn credits(&self) -> Option<&Credits> {
        self.credits.as_ref()
    }

    /// A menu frame on the credits page (@ 0x004591a0, page 9).
    pub(super) fn credits_frame(&mut self) {
        let Some(mut c) = self.credits.take() else {
            return;
        };
        let (w, h) = (self.width, self.height);
        let clicked = std::mem::take(&mut self.click) && c.clicked(self.pointer).is_some();
        c.draw(w, h);
        c.turn(self.now, clicked);
        if c.surface.len() == self.screen.len() {
            self.screen.copy_from_slice(&c.surface);
        }
        let prints = c.texts(w, h);
        self.texts();
        if let Some(t) = self.texts.as_ref() {
            for p in &prints {
                let [r, g, b] = p.rgb.map(|v| (v & 0xff) as u32);
                t.print(
                    &mut self.screen,
                    w,
                    h,
                    font_id(&p.font),
                    &p.text,
                    p.zone,
                    r << 16 | g << 8 | b,
                );
            }
        }
        c.prints = prints;
        self.credits = Some(c);
        self.draw_pointer();
    }

    /// Escape on the credits (@ 0x00457360): the configuration written
    /// (kept in [`Credits::config`] for the program to store), then the
    /// program quits. False when the credits are not shown.
    pub fn credits_escape(&mut self) -> bool {
        if self.mode != Mode::Menu || self.credits.is_none() {
            return false;
        }
        let cfg = self.startup.config.write(&self.startup.install_language);
        if let Some(c) = self.credits.as_mut() {
            c.config = Some(cfg);
        }
        self.quit = true;
        true
    }
}
