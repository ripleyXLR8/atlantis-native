//! A layout file's tree, as `TeLuaGUI::load` (atlantis4-gog-en-2363910
//! @ 0x0054b1d0) builds it, with the sizes and positions of
//! `TeLayout::updateSize` / `updatePosition` ([`crate::layout`]) and a
//! plain draw of its pictures: the small shared GUI base the inventory,
//! the documents, the HUD and the notification use.
//!
//! The file runs in a fresh Lua state of its own (`GuiState`); each
//! constructor call becomes a [`UiNode`] here. Its children are the
//! table's array part in declaration order. A button's `upLayout`,
//! `downLayout` and `disabledLayout` are appended after them
//! (`setUpLayout` @ 0x004e4ce0 adds with `addChild`, slot `+0x10`, and
//! `lua_next` walks the array part first) but are **drawn under** them
//! here `[INFERRED]`: `Inventory.lua`'s full-window black `upLayout`
//! would otherwise hide the whole inventory; `setUpLayout` calls the up
//! layout's slot `+0x4c` with 0 (not walked), the likely reason. The
//! rest draws in tree order, depth first.
//!
//! Rules kept (`STATIC`): defaults `Pixel` (1, 1),
//! anchor and position (0.5, 0.5), position `Percent`; a sprite whose size
//! is not set takes its picture's pixel size and every sprite takes its
//! picture's ratio; a `Pixel` (1, 1) button takes its `upLayout`'s size
//! when that one is `Pixel`; a `Pixel` sprite is drawn with its
//! translation truncated toward zero.
//!
//! Not here (the dialog work's GUI layer): text layouts' glyphs, list
//! layouts, clip layouts, animations other than the colour fades, the
//! button state machine's roll-over.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use atlantis4_data::A4Files;
use atlantis4_script::{
    BootValue, Call, Engine, EngineRef, ErrorMode, GuiKey, GuiState, GuiValue, Options, Reply,
};

use crate::layout::{percent_position, percent_size, pixel_snap, RatioMode, Rect};
use crate::movie::{draw_stretched, Mode};
use crate::package::Picture;

/// One constructor call of a layout file.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNode {
    /// The constructor (`TeLayout`, `TeSpriteLayout`, `TeButtonLayout`, …).
    pub class: String,
    /// `name`, when the file gives one.
    pub name: Option<String>,
    /// `sizeType = Percent`.
    pub size_percent: bool,
    /// `size` (x, y).
    pub size: (f32, f32),
    /// `size` was given (a sprite then keeps it over its picture's).
    pub size_set: bool,
    /// `ratio` (a sprite's picture sets it).
    pub ratio: f32,
    pub ratio_mode: RatioMode,
    pub safe_area: f32,
    pub anchor: (f32, f32),
    /// `positionType = Percent` (the default).
    pub position_percent: bool,
    pub position: (f32, f32),
    pub visible: bool,
    /// A sprite's `image` as written.
    pub image: Option<String>,
    /// A button's state layouts.
    pub up: Option<usize>,
    pub down: Option<usize>,
    pub disabled: Option<usize>,
    pub validation_sound: Option<String>,
    /// Children in drawing order.
    pub children: Vec<usize>,
    /// The other attributes as the file gives them (animations' values).
    pub extra: Vec<(String, GuiValue)>,
}

impl UiNode {
    fn new(class: &str) -> UiNode {
        UiNode {
            class: class.to_string(),
            name: None,
            size_percent: false,
            size: (1.0, 1.0),
            size_set: false,
            ratio: 1.0,
            ratio_mode: RatioMode::None,
            safe_area: crate::layout::SAFE_AREA_RATIO,
            anchor: (0.5, 0.5),
            position_percent: true,
            position: (0.5, 0.5),
            visible: true,
            image: None,
            up: None,
            down: None,
            disabled: None,
            validation_sound: None,
            children: Vec::new(),
            extra: Vec::new(),
        }
    }

    pub fn is_button(&self) -> bool {
        self.class == "TeButtonLayout"
    }

    /// A number attribute kept in [`UiNode::extra`] (`duration`).
    pub fn extra_num(&self, key: &str) -> Option<f64> {
        self.extra.iter().find_map(|(k, v)| match v {
            GuiValue::Num(x) if k == key => Some(*x),
            _ => None,
        })
    }

    /// A list of numbers kept in [`UiNode::extra`] (`curve`, `startValue`).
    pub fn extra_list(&self, key: &str) -> Option<Vec<f64>> {
        self.extra.iter().find_map(|(k, v)| match v {
            GuiValue::Table(t) if k == key => Some(numbers(t)),
            _ => None,
        })
    }
}

fn numbers(t: &[(GuiKey, GuiValue)]) -> Vec<f64> {
    let mut v: Vec<(f64, f64)> = t
        .iter()
        .filter_map(|(k, v)| match (k, v) {
            (GuiKey::Index(i), GuiValue::Num(x)) => Some((*i, *x)),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.into_iter().map(|(_, x)| x).collect()
}

/// A `Vec3` read (`0x004c0e50`): components 1 and 2 of the table, a
/// missing one keeping `current`.
fn vec2(t: &[(GuiKey, GuiValue)], current: (f32, f32)) -> (f32, f32) {
    let get = |i: f64| {
        t.iter().find_map(|(k, v)| match (k, v) {
            (GuiKey::Index(j), GuiValue::Num(x)) if *j == i => Some(*x as f32),
            _ => None,
        })
    };
    (get(1.0).unwrap_or(current.0), get(2.0).unwrap_or(current.1))
}

/// The loaded file: its nodes, the top-level ones and its string and
/// number globals (`textFormat`, `dragThreshold`).
#[derive(Clone, Debug, Default)]
pub struct UiFile {
    pub nodes: Vec<UiNode>,
    /// The constructors not made a child of another, in call order.
    pub roots: Vec<usize>,
    pub globals: HashMap<String, BootValue>,
}

/// The pictures a tree draws: the file a logical picture path stands for.
pub type PictureOf<'a> = dyn Fn(&str) -> Option<&'a Picture> + 'a;

impl UiFile {
    /// The node named `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.name.as_deref() == Some(name))
    }

    /// A string global of the file.
    pub fn global_str(&self, name: &str) -> Option<String> {
        match self.globals.get(name) {
            Some(BootValue::Str(s)) => Some(s.iter().map(|&c| char::from(c)).collect()),
            _ => None,
        }
    }

    /// A number global of the file.
    pub fn global_num(&self, name: &str) -> Option<f64> {
        match self.globals.get(name) {
            Some(BootValue::Num(x)) => Some(*x),
            _ => None,
        }
    }

    /// The pictures' sizes and ratios applied: a sprite without a `size`
    /// takes its picture's (`Pixel`), every sprite its picture's ratio,
    /// then a `Pixel` (1, 1) button its `Pixel` up layout's size.
    pub fn apply_pictures(&mut self, size_of: &dyn Fn(&str) -> Option<(u32, u32)>) {
        for n in &mut self.nodes {
            let Some((w, h)) = n.image.as_deref().and_then(size_of) else {
                continue;
            };
            n.ratio = if h == 0 {
                1.0
            } else {
                (f64::from(w) / f64::from(h)) as f32
            };
            if !n.size_set {
                n.size = (w as f32, h as f32);
                n.size_percent = false;
            }
        }
        for i in 0..self.nodes.len() {
            let n = &self.nodes[i];
            if !(n.is_button() && !n.size_percent && n.size == (1.0, 1.0)) {
                continue;
            }
            if let Some(u) = n.up {
                let up = &self.nodes[u];
                if !up.size_percent {
                    let s = up.size;
                    self.nodes[i].size = s;
                }
            }
        }
    }

    /// Where node `id` lands when its parent has size `parent` and its
    /// centre at `centre` (window pixels from the window's centre).
    pub fn place(&self, id: usize, parent: (f32, f32), centre: (f32, f32)) -> Placed {
        let n = &self.nodes[id];
        let size = if n.size_percent {
            percent_size(n.size, parent, n.ratio, n.ratio_mode, n.safe_area)
        } else {
            (n.size.0.abs(), n.size.1.abs())
        };
        let offset = if n.position_percent {
            percent_position(n.position, n.anchor, size, parent)
        } else {
            n.position
        };
        Placed {
            centre: (centre.0 + offset.0, centre.1 + offset.1),
            size,
        }
    }

    /// Every node's place under the top-level node `root` laid out in a
    /// `parent`-sized box centred at `centre`, by id (`None`: not under
    /// `root`).
    pub fn layout(
        &self,
        root: usize,
        parent: (f32, f32),
        centre: (f32, f32),
    ) -> Vec<Option<Placed>> {
        let mut out = vec![None; self.nodes.len()];
        self.layout_into(root, parent, centre, &mut out);
        out
    }

    fn layout_into(
        &self,
        id: usize,
        parent: (f32, f32),
        centre: (f32, f32),
        out: &mut Vec<Option<Placed>>,
    ) {
        let p = self.place(id, parent, centre);
        out[id] = Some(p);
        for &c in &self.nodes[id].children {
            self.layout_into(c, p.size, p.centre, out);
        }
    }

    /// Draws node `id` and its visible descendants placed by `places`
    /// into `out` (`width × height`): sprites' pictures, a button's
    /// current state layout only (`down` when `pressed` names it).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        id: usize,
        places: &[Option<Placed>],
        out: &mut [u32],
        width: u32,
        height: u32,
        pictures: &PictureOf<'_>,
        pressed: &dyn Fn(usize) -> bool,
    ) {
        let n = &self.nodes[id];
        if !n.visible {
            return;
        }
        if let (Some(img), Some(p)) = (&n.image, places[id]) {
            if let Some(pic) = pictures(img) {
                draw_picture(
                    out,
                    width,
                    height,
                    pic,
                    p.rect(!n.size_percent, width, height),
                    255,
                );
            }
        }
        let state = if n.is_button() {
            match (pressed(id), n.down) {
                (true, Some(d)) => Some(d),
                _ => n.up,
            }
        } else {
            None
        };
        for &c in &n.children {
            let is_state = n.is_button() && [n.up, n.down, n.disabled].contains(&Some(c));
            if is_state && Some(c) != state {
                continue;
            }
            self.draw(c, places, out, width, height, pictures, pressed);
        }
    }
}

/// A node's centre (window pixels from the window's centre, y down) and
/// size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub centre: (f32, f32),
    pub size: (f32, f32),
}

impl Placed {
    /// Its rectangle in window pixels; `snap`: the drawn one of a `Pixel`
    /// sprite (translation truncated, @ 0x0054cde5).
    pub fn rect(&self, snap: bool, width: u32, height: u32) -> Rect {
        let c = if snap {
            pixel_snap(self.centre)
        } else {
            self.centre
        };
        crate::layout::window_rect(c, self.size, width, height)
    }
}

/// A picture over `rect` (window pixels), blended, its alpha scaled by
/// `alpha` / 255 (the layout's colour, white with that alpha); the
/// texture beyond the picture as the GUI sprites' ([`crate::sprite`]):
/// clamped left and below, transparent right and above.
pub fn draw_picture(
    out: &mut [u32],
    width: u32,
    height: u32,
    pic: &Picture,
    rect: Rect,
    alpha: u8,
) {
    let faded;
    let pic = if alpha == 255 {
        pic
    } else {
        let mut p = pic.clone();
        for a in p.rgba.iter_mut().skip(3).step_by(4) {
            *a = ((f64::from(*a) * f64::from(alpha) / 255.0) + 0.5).floor() as u8;
        }
        faded = p;
        &faded
    };
    let (w, h) = (i64::from(pic.width), i64::from(pic.height));
    let tap = |x: i64, y: i64| -> [u8; 4] {
        if x >= w || y < 0 {
            return [0; 4];
        }
        pic.texel(x.max(0), y.min(h - 1))
    };
    draw_stretched(out, width, height, pic, rect, Mode::Blend, &tap);
}

/// The file a GUI path stands for in the install: as written, then with
/// the platform, definition and language folders the install keeps
/// (`PC-MacOSX/`, `HD/`, `HD/<lang>/`, `HD/<lang>/PC-MacOSX/`,
/// `<lang>/` before the file name). `[UNKNOWN]` the resolver's own order
/// (@ 0x00552bd0, the M1 question); this is the narrowest reading that
/// finds every GUI picture, layout and document picture of the install.
pub fn resolve(files: &A4Files, path: &str, lang: &str) -> Option<String> {
    if files.source().size(path).is_some() {
        return Some(path.to_string());
    }
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    let subs = [
        "PC-MacOSX".to_string(),
        "HD".to_string(),
        format!("HD/{lang}"),
        format!("HD/{lang}/PC-MacOSX"),
        lang.to_string(),
    ];
    subs.iter().find_map(|s| {
        let p = if dir.is_empty() {
            format!("{s}/{file}")
        } else {
            format!("{dir}/{s}/{file}")
        };
        files.source().size(&p).is_some().then_some(p)
    })
}

/// The engine side of a layout state: reads the file, records the
/// constructors.
struct Recorder {
    files: A4Files,
    lang: String,
    nodes: Vec<UiNode>,
    /// Ids made a child of another.
    adopted: Vec<bool>,
}

fn ratio_mode(x: f64) -> RatioMode {
    match x as i32 {
        1 => RatioMode::LetterBox,
        2 => RatioMode::PanScan,
        _ => RatioMode::None,
    }
}

impl Engine for Recorder {
    fn call(&mut self, _call: &Call) -> Reply {
        Reply::None
    }

    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        let p: String = path.iter().map(|&c| char::from(c)).collect();
        let r = resolve(&self.files, &p, &self.lang)?;
        self.files.read(&r)
    }

    /// Ids start at 1 (0 is `__TeLuaGUIThis`).
    fn gui_layout(&mut self, class: &str, entries: Option<&[(GuiKey, GuiValue)]>) -> usize {
        let mut n = UiNode::new(class);
        let mut children = Vec::new();
        let (mut states, mut array) = (Vec::new(), Vec::new());
        for (k, v) in entries.unwrap_or_default() {
            match (k, v) {
                (GuiKey::Index(i), GuiValue::Child(c)) => array.push((*i, *c)),
                (GuiKey::Name(key), v) => {
                    let key: String = key.iter().map(|&c| char::from(c)).collect();
                    let text = |v: &GuiValue| match v {
                        GuiValue::Str(s) => Some(s.iter().map(|&c| char::from(c)).collect()),
                        _ => None,
                    };
                    let num = |v: &GuiValue| match v {
                        GuiValue::Num(x) => Some(*x),
                        _ => None,
                    };
                    match (key.as_str(), v) {
                        ("name", v) => n.name = text(v),
                        ("sizeType", v) => n.size_percent = num(v) == Some(1.0),
                        ("size", GuiValue::Table(t)) => {
                            n.size = vec2(t, n.size);
                            n.size_set = true;
                        }
                        ("ratio", v) => n.ratio = num(v).unwrap_or(1.0) as f32,
                        ("ratioMode", v) => n.ratio_mode = ratio_mode(num(v).unwrap_or(0.0)),
                        ("safeAreaRatio", v) => n.safe_area = num(v).unwrap_or(4.0 / 3.0) as f32,
                        ("anchor", GuiValue::Table(t)) => n.anchor = vec2(t, n.anchor),
                        ("positionType", v) => n.position_percent = num(v) != Some(0.0),
                        ("position", GuiValue::Table(t)) => n.position = vec2(t, n.position),
                        ("visible", GuiValue::Bool(b)) => n.visible = *b,
                        ("image", v) => n.image = text(v),
                        ("validationSound", v) => n.validation_sound = text(v),
                        ("upLayout", GuiValue::Child(c)) => {
                            n.up = Some(c - 1);
                            states.push(c - 1);
                        }
                        ("downLayout", GuiValue::Child(c)) => {
                            n.down = Some(c - 1);
                            states.push(c - 1);
                        }
                        ("disabledLayout", GuiValue::Child(c)) => {
                            n.disabled = Some(c - 1);
                            states.push(c - 1);
                        }
                        _ => n.extra.push((key, v.clone())),
                    }
                }
                _ => {}
            }
        }
        array.sort_by(|a, b| a.0.total_cmp(&b.0));
        children.extend(states);
        children.extend(array.into_iter().map(|(_, c)| c - 1));
        for &c in &children {
            if let Some(a) = self.adopted.get_mut(c) {
                *a = true;
            }
        }
        n.children = children;
        self.nodes.push(n);
        self.adopted.push(false);
        self.nodes.len()
    }
}

/// Runs layout file `path` (as written; its install variant found by
/// [`resolve`]) in a fresh layout state and returns its tree and the
/// globals named in `globals`; pictures are not applied yet
/// ([`UiFile::apply_pictures`]).
pub fn load(files: &A4Files, lang: &str, path: &str, globals: &[&str]) -> UiFile {
    let rec = Rc::new(RefCell::new(Recorder {
        files: files.clone(),
        lang: lang.to_string(),
        nodes: Vec::new(),
        adopted: Vec::new(),
    }));
    let engine: EngineRef = rec.clone();
    let options = Options {
        errors: ErrorMode::Log,
        ..Options::default()
    };
    let mut values = HashMap::new();
    match GuiState::new(&engine, options) {
        Ok(mut st) => {
            st.run_file(path.as_bytes());
            for g in globals {
                if let Ok(Some(v)) = st.read(g) {
                    values.insert((*g).to_string(), v);
                }
            }
        }
        Err(e) => log::warn!("{path}: no layout state: {e}"),
    }
    drop(engine);
    let rec = Rc::try_unwrap(rec)
        .map(RefCell::into_inner)
        .unwrap_or_else(|r| {
            let r = r.borrow();
            Recorder {
                files: r.files.clone(),
                lang: r.lang.clone(),
                nodes: r.nodes.clone(),
                adopted: r.adopted.clone(),
            }
        });
    if rec.nodes.is_empty() {
        log::warn!("{path}: no layout");
    }
    UiFile {
        roots: (0..rec.nodes.len()).filter(|&i| !rec.adopted[i]).collect(),
        nodes: rec.nodes,
        globals: values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(class: &str) -> UiNode {
        UiNode::new(class)
    }

    #[test]
    fn a_pixel_button_takes_its_up_pictures_size() {
        let mut f = UiFile::default();
        let mut up = node("TeSpriteLayout");
        up.image = Some("a.png".into());
        let mut b = node("TeButtonLayout");
        b.up = Some(0);
        b.children = vec![0];
        b.anchor = (1.0, 1.0);
        b.position = (1.0, 1.0);
        f.nodes = vec![up, b];
        f.roots = vec![1];
        f.apply_pictures(&|_| Some((120, 80)));
        assert_eq!(f.nodes[1].size, (120.0, 80.0));
        assert_eq!(f.nodes[0].ratio, 1.5);
        // Bottom-right corner of a 1365 × 768 window.
        let p = f.place(1, (1365.0, 768.0), (0.0, 0.0));
        let r = p.rect(false, 1365, 768);
        assert_eq!((r.left, r.top), (1245.0, 688.0));
    }

    #[test]
    fn a_sized_sprite_keeps_its_size_and_takes_the_ratio() {
        let mut f = UiFile::default();
        let mut s = node("TeSpriteLayout");
        s.image = Some("a.png".into());
        s.size_percent = true;
        s.size = (1.0, 1.0);
        s.size_set = true;
        s.ratio_mode = RatioMode::LetterBox;
        f.nodes = vec![s];
        f.apply_pictures(&|_| Some((400, 300)));
        let p = f.place(0, (1365.0, 768.0), (0.0, 0.0));
        assert_eq!(p.size, (1024.0, 768.0));
    }

    #[test]
    fn vectors_keep_missing_components() {
        let t = vec![(GuiKey::Index(1.0), GuiValue::Num(0.25))];
        assert_eq!(vec2(&t, (0.5, 0.5)), (0.25, 0.5));
    }
}
