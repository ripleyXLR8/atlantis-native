//! The GUI's 2-D layer: `TeLuaGUI` layout files built into a tree of
//! layouts, their geometry, buttons, animations, texts and drawing, as
//! atlantis4-gog-en-2363910 has them: the one GUI base of the dialog bar,
//! the answers and the talking heads ([`crate::dialog`]) and of the HUD,
//! the inventory menu, the shown document, the notification and the
//! on-screen sprites ([`crate::screens`]); each screen owns its trees.
//!
//! - **Files** (`TeLuaGUI::load` @ 0x0054b1d0): a fresh Lua state per
//!   file with the 13 constructors ([`atlantis4_script::GuiState`]); each
//!   constructor builds a layout from its table walked in `lua_next`
//!   order: string keys are attributes, number keys holding a layout are
//!   children appended in that order; a name already taken in its class's
//!   map deletes the new object (the constructor returns nothing).
//! - **Geometry** ([`crate::layout`], `updateSize` @ 0x004fc5b0,
//!   `updatePosition` @ 0x004fc950): the root is the window, one unit a
//!   pixel, origin at the centre, y down ([`projection`]); a layout's
//!   position is its centre's offset from its parent's centre.
//! - **Sprites** (`load` @ 0x0054c940): an `image` sets `ratio` to the
//!   picture's width / height and, for a `Pixel` sprite whose size was
//!   never set, its size to the picture's; a `Pixel` sprite is drawn with
//!   its translation truncated (@ 0x0054cde5).
//! - **Buttons** (@ 0x004e4810..0x004e52c0): states up 0, down 1,
//!   disabled 2, roll-over 3, each showing its layout (a missing down or
//!   roll-over layout falls back to up; disabled to down, then up); a
//!   press inside records the pointer (down); a release inside after a
//!   press inside validates the click (its `validationSound` first); a hit
//!   consumes the event; `setUpLayout` gives a `Pixel` button of user size
//!   (1, 1) its up layout's size.
//! - **Lists** (`TeListLayout` @ 0x0056ea70): [`list_layout`].
//! - **Colour**: a sprite's picture and a text's glyphs are multiplied by
//!   the layout's colour times its ancestors' (`GL_MODULATE`); the
//!   inheritance is `[INFERRED]` (not read here; the notification's fade
//!   animates its sprite, whose child is the text).
//! - **Pictures**: `GL_LINEAR` in a power-of-two texture, the taps beyond
//!   the picture as the other GUI sprites' ([`crate::sprite`]: clamped
//!   left and below, transparent right and above).
//! - **Draw order**: a later child above an earlier one (the z order of
//!   `updateZ` @ 0x004fbe50), except a button's state layouts, which are
//!   drawn first, under the button's other children. `[INFERRED]` from the
//!   data: `setUpLayout` appends its layout (`addChild`, slot `+0x10`
//!   @ 0x004e4d31) after the array's children, yet the dialog bar's opaque
//!   picture (`2D/Dialogs/HD/DialogBar.png`, alpha 255 where the text is)
//!   must lie under its subtitle, and `InGame.lua`'s
//!   `currentObjectSprite` under no picture of its button; how the
//!   renderer orders them is not read.

pub mod anim;
pub mod text;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::movie::{draw_stretched, Mode};

use atlantis4_data::image::decode_jpeg;
use atlantis4_data::A4Files;
use atlantis4_script::{
    BootValue, Call, Engine, EngineRef, Event, GuiKey, GuiState, GuiValue, Options, Reply,
};

use crate::layout::{percent_position, percent_size, RatioMode, Rect, SAFE_AREA_RATIO};
use crate::package::Picture;
use anim::LinearAnim;
use text::{Block, Clip, Font, FontCache, Frame};

/// A layout's index in its [`Gui`].
pub type Id = usize;

/// The 13 `TeLuaGUI` classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Layout,
    Sprite,
    Button,
    Checkbox,
    Text,
    ExtendedText,
    List,
    Clip,
    Scrolling,
    AnchorAnim,
    PositionAnim,
    ColorAnim,
    RotationAnim,
}

impl Class {
    fn from_name(s: &str) -> Option<Class> {
        Some(match s {
            "TeLayout" => Class::Layout,
            "TeSpriteLayout" => Class::Sprite,
            "TeButtonLayout" => Class::Button,
            "TeCheckboxLayout" => Class::Checkbox,
            "TeTextLayout" => Class::Text,
            "TeExtendedTextLayout" => Class::ExtendedText,
            "TeListLayout" => Class::List,
            "TeClipLayout" => Class::Clip,
            "TeScrollingLayout" => Class::Scrolling,
            "TeLayoutAnchorLinearAnimation" => Class::AnchorAnim,
            "TeLayoutPositionLinearAnimation" => Class::PositionAnim,
            "TeColorLinearAnimation" => Class::ColorAnim,
            "TeRotationLinearAnimation" => Class::RotationAnim,
            _ => return None,
        })
    }

    fn is_anim(self) -> bool {
        matches!(
            self,
            Class::AnchorAnim | Class::PositionAnim | Class::ColorAnim | Class::RotationAnim
        )
    }

    fn is_text(self) -> bool {
        matches!(self, Class::Text | Class::ExtendedText)
    }
}

/// A button's state (`+0x57c`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    #[default]
    Up,
    Down,
    Disabled,
    RollOver,
}

/// One layout.
#[derive(Clone, Debug)]
pub struct Node {
    pub class: Class,
    pub name: String,
    pub parent: Option<Id>,
    pub children: Vec<Id>,
    /// `sizeType = Percent`.
    pub size_percent: bool,
    /// The user size (`+0x60`).
    pub size: [f32; 3],
    /// A size was set (`setSize`; a sprite's `+0x568`).
    pub size_set: bool,
    pub ratio: f32,
    pub ratio_mode: RatioMode,
    pub safe_area_ratio: f32,
    pub anchor: [f32; 3],
    /// `positionType = Percent` (the default).
    pub position_percent: bool,
    pub position: [f32; 3],
    pub visible: bool,
    pub colour: [u8; 4],
    /// A sprite's picture (the path as given; resolved when drawn).
    pub image: Option<String>,
    // Buttons.
    pub up: Option<Id>,
    pub down: Option<Id>,
    pub disabled: Option<Id>,
    pub roll_over: Option<Id>,
    pub hit_zone: Option<Id>,
    pub enabled: bool,
    pub click_pass_through: bool,
    pub validation_sound: Option<String>,
    pub volume_sound: f32,
    pub state: State,
    /// A checkbox's state (`+0x564`, @ 0x004dc7d0): 0 active, 1 unactive,
    /// 2 active disabled, 3 unactive disabled, each showing only its own
    /// layout. `[INFERRED]` the constructor's 1.
    pub check: u8,
    /// A checkbox's `unactivationSound` (`+0x584`).
    pub unactivation_sound: Option<String>,
    /// The rotation about z set by a rotation animation, degrees.
    pub rotation: f32,
    pressed: bool,
    released: bool,
    // Texts.
    pub markup: Vec<u8>,
    pub inter_line: f32,
    pub single_line: bool,
    pub auto_scroll_delay: i32,
    pub auto_scroll_speed: f32,
    /// The text laid out (rebuilt when the text or the box width changes).
    pub block: Option<Block>,
    font: Option<Rc<Font>>,
    text_dirty: bool,
    /// When the auto-scroll may start (the clock at `setText` plus the
    /// delay), ms.
    scroll_deadline: f64,
    /// The scroll's start and duration, ms.
    scroll: Option<(f64, f64)>,
    /// The scroll's offset, pixels.
    pub scroll_offset: f32,
    // Lists.
    pub direction: [f32; 3],
    pub minimum_margin: [f32; 3],
    pub maximum_margin: [f32; 3],
    // Animations.
    pub anim: Option<LinearAnim>,
    /// The layout an animation drives (`layout`, or bound by a screen).
    pub target: Option<Id>,
    pub target_name: Option<String>,
    // Computed by [`Gui::update`].
    /// The size (`updateSize`).
    pub computed_size: (f32, f32),
    /// The centre in window-centred coordinates (y down).
    pub world: (f32, f32),
}

impl Node {
    fn new(class: Class) -> Node {
        Node {
            class,
            name: String::new(),
            parent: None,
            children: Vec::new(),
            size_percent: false,
            size: [1.0; 3],
            size_set: false,
            ratio: 1.0,
            ratio_mode: RatioMode::None,
            safe_area_ratio: SAFE_AREA_RATIO,
            anchor: [0.5; 3],
            position_percent: true,
            position: [0.5; 3],
            visible: true,
            colour: [255; 4],
            image: None,
            up: None,
            down: None,
            disabled: None,
            roll_over: None,
            hit_zone: None,
            enabled: true,
            click_pass_through: false,
            validation_sound: None,
            volume_sound: 1.0,
            state: State::Up,
            check: 1,
            unactivation_sound: None,
            rotation: 0.0,
            pressed: false,
            released: false,
            markup: Vec::new(),
            inter_line: 0.0,
            single_line: false,
            // `TeExtendedTextLayout`'s constructor @ 0x004c05fe..0x004c0649.
            auto_scroll_delay: 4000,
            auto_scroll_speed: 0.1,
            block: None,
            font: None,
            text_dirty: false,
            scroll_deadline: 0.0,
            scroll: None,
            scroll_offset: 0.0,
            // @ 0x0056f28d..0x0056f3a4.
            direction: [0.0, 1.0, 0.0],
            minimum_margin: [-1e6; 3],
            maximum_margin: [f32::MAX; 3],
            anim: None,
            target: None,
            target_name: None,
            computed_size: (0.0, 0.0),
            world: (0.0, 0.0),
        }
    }

    /// The layout shown for the button's state (@ 0x004e4810), or the
    /// checkbox's (@ 0x004dc7d0: only the state's own layout).
    pub fn state_layout(&self) -> Option<Id> {
        if self.class == Class::Checkbox {
            return match self.check {
                0 => self.up,
                1 => self.down,
                2 => self.disabled,
                _ => self.roll_over,
            };
        }
        match self.state {
            State::Up => self.up,
            State::Down => self.down.or(self.up),
            State::Disabled => self.disabled.or(self.down).or(self.up),
            State::RollOver => self.roll_over.or(self.up),
        }
    }

    fn is_state_layout(&self, id: Id) -> bool {
        [self.up, self.down, self.disabled, self.roll_over].contains(&Some(id))
    }
}

/// A layout file built: its layouts, its `textFormat` and the number
/// globals a screen's loader reads ([`NUMBER_GLOBALS`]).
#[derive(Clone, Debug, Default)]
pub struct Gui {
    pub path: String,
    pub nodes: Vec<Node>,
    pub text_format: Option<Vec<u8>>,
    pub numbers: HashMap<String, f64>,
    /// The string globals of [`STRING_GLOBALS`] the file set.
    pub strings: HashMap<String, Vec<u8>>,
}

/// The number globals the screens' loaders read after running a file
/// (`InventoryObject.lua`'s `dragThreshold`, @ 0x0054e540).
pub const NUMBER_GLOBALS: [&str; 2] = ["dragThreshold", "slideTranslation"];

/// The string globals the menus' loaders read (`musicPath` of the menus,
/// the options' test sounds, `RotationAnim.lua`'s sounds; 0x00549580).
pub const STRING_GLOBALS: [&str; 9] = [
    "musicPath",
    "dialogTestPath",
    "videoTestPath",
    "landscapeCloseSound",
    "landscapeRotateSound",
    "landscapeOpenSound",
    "portraitCloseSound",
    "portraitRotateSound",
    "portraitOpenSound",
];

/// What draws a sprite's content.
pub trait Painter {
    /// The picture of a path given to a sprite, if loaded.
    fn picture(&self, path: &str) -> Option<&Picture>;
    /// Draws a video sprite's frame over `rect`; `false` when the sprite
    /// has no video.
    fn video(
        &self,
        gui: &Gui,
        id: Id,
        out: &mut [u32],
        width: u32,
        height: u32,
        rect: Rect,
    ) -> bool;
}

/// The file a logical path stands for: as written, else in `PC-MacOSX/`
/// or `HD/` beside the file ([`crate::movie::resolve`]), else in the
/// install language's folders ([`resolve_in`], the language of
/// `lang.ini`, English without one).
pub fn resolve(files: &A4Files, path: &str) -> Option<String> {
    crate::movie::resolve(files, path).or_else(|| {
        let lang = files.lang_ini().unwrap_or_else(|| "en".to_string());
        resolve_in(files, path, &lang)
    })
}

/// The file a GUI path stands for in the install for language `lang`: as
/// written, then with the platform, definition and language folders the
/// install keeps (`PC-MacOSX/`, `HD/`, `HD/<lang>/`,
/// `HD/<lang>/PC-MacOSX/`, `<lang>/` before the file name). `[UNKNOWN]`
/// the resolver's own order (@ 0x00552bd0); this is the narrowest reading
/// that finds every GUI picture, layout and document picture of the
/// install.
pub fn resolve_in(files: &A4Files, path: &str, lang: &str) -> Option<String> {
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

/// A picture's width and height, from a PNG's header or a JPEG.
pub fn image_size(files: &A4Files, path: &str) -> Option<(u32, u32)> {
    let file = resolve(files, path)?;
    let data = files.read(&file)?;
    if data.starts_with(b"\x89PNG") && data.len() >= 24 {
        let be = |o: usize| u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        return Some((be(16), be(20)));
    }
    decode_jpeg(&data).ok().map(|i| (i.width, i.height))
}

/// A picture (PNG or JPEG) as RGBA, from its logical path.
pub fn load_picture(files: &A4Files, path: &str) -> Option<Picture> {
    decode_picture(&files.read(&resolve(files, path)?)?)
}

/// A PNG or JPEG file's bytes as an RGBA picture.
pub fn decode_picture(data: &[u8]) -> Option<Picture> {
    let img = if data.starts_with(b"\x89PNG") {
        atlantis4_data::image::decode_png(data).ok()?
    } else {
        decode_jpeg(data).ok()?
    };
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

/// The GUI root's camera (@ 0x00510760 then the ortho matrix
/// @ 0x004f9930) for a `width × height` window: planes `−w/2, w/2`,
/// `h/2` (`+0xb0`), `−h/2` (`+0xb4`), near −2048, far 2048; GL's
/// column-major matrix.
pub fn projection(width: u32, height: u32) -> [f32; 16] {
    let (w, h) = (width as f32, height as f32);
    let (l, r) = (-w / 2.0, w / 2.0);
    let (b0, b4) = (h / 2.0, -h / 2.0);
    let (near, far) = (-2048f32, 2048f32);
    let d = f64::from;
    let mut m = [0f32; 16];
    m[0] = (2.0 / (d(r) - d(l))) as f32;
    m[5] = (2.0 / (d(b4) - d(b0))) as f32;
    m[10] = (-2.0 / (d(far) - d(near))) as f32;
    m[12] = (-((d(r) + d(l)) / (d(r) - d(l)))) as f32;
    m[13] = (-((d(b4) + d(b0)) / (d(b4) - d(b0)))) as f32;
    m[14] = (-((d(far) + d(near)) / (d(far) - d(near)))) as f32;
    m[15] = 1.0;
    m
}

/// The list layout's placement (@ 0x0056ea70): from the list's size `s`
/// (its `Percent` size, restored before each update), the children's
/// sizes and the attributes, the fitted size and each child's centre as
/// an offset from the list's centre.
pub fn list_layout(
    s: (f32, f32),
    sizes: &[(f32, f32)],
    direction: [f32; 3],
    minimum: [f32; 3],
    maximum: [f32; 3],
) -> ((f32, f32), Vec<(f32, f32)>) {
    let f = |x: f64| x as f32;
    let d = f64::from;
    let n = sizes.len();
    let s = [s.0, s.1];
    let mut extent = [0f32; 2];
    let mut o = [0f32; 2];
    for c in sizes {
        let c = [c.0, c.1];
        for k in 0..2 {
            extent[k] = extent[k].max(f(d(o[k]) + d(c[k])));
            o[k] = f(d(o[k]) + d(c[k]) * d(direction[k]));
        }
    }
    let mut m = [0f32; 2];
    for k in 0..2 {
        if n > 1 {
            m[k] = f((d(s[k]) - d(extent[k])) / (n - 1) as f64);
        }
        let max = f(d(maximum[k]) * d(s[k]));
        let min = f(d(minimum[k]) * d(s[k]));
        if m[k] > max {
            m[k] = max;
        } else if m[k] < min {
            m[k] = min;
        }
    }
    let gaps = n.saturating_sub(1) as f64;
    let fitted = [
        f(d(extent[0]) + d(m[0]) * gaps),
        f(d(extent[1]) + d(m[1]) * gaps),
    ];
    let mut o = [0f32; 2];
    let mut centres = Vec::with_capacity(n);
    for c in sizes {
        let c = [c.0, c.1];
        let at: [f32; 2] = std::array::from_fn(|k| f(d(o[k]) + d(c[k]) / 2.0 - d(fitted[k]) / 2.0));
        centres.push((at[0], at[1]));
        for k in 0..2 {
            o[k] = f(d(o[k]) + d(c[k]) * d(direction[k]) + d(m[k]));
        }
    }
    ((fitted[0], fitted[1]), centres)
}

/// Builds the layouts of one file from the constructor calls.
struct Builder {
    files: A4Files,
    nodes: Vec<Node>,
    names: HashMap<(Class, String), Id>,
}

fn vec3(v: &GuiValue, mut cur: [f32; 3]) -> [f32; 3] {
    if let GuiValue::Table(t) = v {
        for (k, x) in t {
            if let (GuiKey::Index(i), GuiValue::Num(x)) = (k, x) {
                let i = *i as usize;
                if (1..=3).contains(&i) {
                    cur[i - 1] = *x as f32;
                }
            }
        }
    }
    cur
}

fn colour4(v: &GuiValue) -> [f32; 4] {
    let mut c = [255.0f32; 4];
    if let GuiValue::Table(t) = v {
        for (k, x) in t {
            if let (GuiKey::Index(i), GuiValue::Num(x)) = (k, x) {
                let i = *i as usize;
                if (1..=4).contains(&i) {
                    c[i - 1] = f32::from(*x as u32 as u8);
                }
            }
        }
    }
    c
}

fn num(v: &GuiValue) -> f64 {
    match v {
        GuiValue::Num(x) => *x,
        GuiValue::Str(s) => std::str::from_utf8(s)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn string(v: &GuiValue) -> Option<String> {
    match v {
        GuiValue::Str(s) => Some(s.iter().map(|&c| char::from(c)).collect()),
        GuiValue::Num(x) => Some(format!("{x}")),
        _ => None,
    }
}

fn child(v: &GuiValue) -> Option<Id> {
    match v {
        GuiValue::Child(id) if *id > 0 => Some(id - 1),
        _ => None,
    }
}

impl Builder {
    fn add_child(&mut self, parent: Id, c: Id) {
        if let Some(old) = self.nodes[c].parent {
            self.nodes[old].children.retain(|&x| x != c);
        }
        self.nodes[c].parent = Some(parent);
        self.nodes[parent].children.push(c);
    }

    fn attribute(&mut self, id: Id, key: &str, v: &GuiValue) {
        let class = self.nodes[id].class;
        let n = &mut self.nodes[id];
        match key {
            "name" => n.name = string(v).unwrap_or_default(),
            "sizeType" => n.size_percent = num(v) as i32 == 1,
            "size" => {
                n.size = vec3(v, n.size);
                n.size_set = true;
            }
            "ratio" => n.ratio = num(v) as f32,
            "ratioMode" => {
                n.ratio_mode = match num(v) as i32 {
                    1 => RatioMode::LetterBox,
                    2 => RatioMode::PanScan,
                    _ => RatioMode::None,
                }
            }
            "safeAreaRatio" => n.safe_area_ratio = num(v) as f32,
            "anchor" => n.anchor = vec3(v, n.anchor),
            "positionType" => n.position_percent = num(v) as i32 == 1,
            "position" => n.position = vec3(v, n.position),
            "visible" => n.visible = matches!(v, GuiValue::Bool(true)),
            "color" if !class.is_anim() => {
                n.colour = colour4(v).map(|x| x as u8);
            }
            "image" if class == Class::Sprite => {
                let path = string(v);
                n.image = path.clone();
                if let Some((w, h)) = path.and_then(|p| image_size(&self.files, &p)) {
                    let n = &mut self.nodes[id];
                    n.ratio = if h == 0 {
                        1.0
                    } else {
                        (f64::from(w) / f64::from(h)) as f32
                    };
                    if !n.size_percent && !n.size_set {
                        n.size = [w as f32, h as f32, 1.0];
                    }
                }
            }
            "upLayout"
            | "downLayout"
            | "disabledLayout"
            | "rollOverLayout"
            | "hitZone"
            | "activeLayout"
            | "unactiveLayout"
            | "activeDisabledLayout"
            | "unactiveDisabledLayout"
                if matches!(class, Class::Button | Class::Checkbox) =>
            {
                let Some(c) = child(v) else { return };
                let n = &mut self.nodes[id];
                let old = match key {
                    "upLayout" | "activeLayout" => n.up.replace(c),
                    "downLayout" | "unactiveLayout" => n.down.replace(c),
                    "disabledLayout" | "activeDisabledLayout" => n.disabled.replace(c),
                    "rollOverLayout" | "unactiveDisabledLayout" => n.roll_over.replace(c),
                    _ => n.hit_zone.replace(c),
                };
                if let Some(o) = old {
                    self.nodes[id].children.retain(|&x| x != o);
                }
                self.add_child(id, c);
                // `setUpLayout` @ 0x004e4ce0: a `Pixel` button of user size
                // (1, 1) takes its up layout's size; a checkbox its active
                // layout's (`[INFERRED]`, the same pattern @ 0x004dcac8).
                if key == "upLayout" || key == "activeLayout" {
                    let n = &self.nodes[id];
                    if !n.size_percent && n.size[0] == 1.0 && n.size[1] == 1.0 {
                        let up = &self.nodes[c];
                        let s = if up.size_percent {
                            [0.0, 0.0]
                        } else {
                            [up.size[0].abs(), up.size[1].abs()]
                        };
                        let n = &mut self.nodes[id];
                        n.size[0] = s[0];
                        n.size[1] = s[1];
                    }
                }
            }
            "enable" => n.enabled = matches!(v, GuiValue::Bool(true)),
            "clickPassThrough" => n.click_pass_through = matches!(v, GuiValue::Bool(true)),
            "validationSound" | "activationSound" => n.validation_sound = string(v),
            "unactivationSound" => n.unactivation_sound = string(v),
            "volumeSound" => n.volume_sound = num(v) as f32,
            "text" if class.is_text() => {
                n.markup = match v {
                    GuiValue::Str(s) => s.clone(),
                    _ => Vec::new(),
                };
                n.text_dirty = true;
            }
            "interLine" | "interline" => n.inter_line = num(v) as f32,
            "wrapMode" => n.single_line = num(v) as i32 == 1,
            "autoScrollDelay" => n.auto_scroll_delay = num(v) as i32,
            "autoScrollSpeed" => n.auto_scroll_speed = num(v) as f32,
            "direction" => n.direction = vec3(v, n.direction),
            "minimumMargin" => n.minimum_margin = vec3(v, n.minimum_margin),
            "maximumMargin" => n.maximum_margin = vec3(v, n.maximum_margin),
            "duration" | "startValue" | "endValue" | "curve" | "layout" if class.is_anim() => {
                let a = n
                    .anim
                    .get_or_insert_with(|| LinearAnim::new(0.0, [0.0; 4], [0.0; 4], Vec::new()));
                let pad = |v3: [f32; 3]| [v3[0], v3[1], v3[2], 0.0];
                match key {
                    "duration" => a.duration = num(v) as f32,
                    "startValue" if class == Class::ColorAnim => a.start_value = colour4(v),
                    "endValue" if class == Class::ColorAnim => a.end_value = colour4(v),
                    // A rotation: axis x, y, z then the angle in degrees
                    // (0x004c0a30); the data's axis is always z.
                    "startValue" if class == Class::RotationAnim => a.start_value = rotation4(v),
                    "endValue" if class == Class::RotationAnim => a.end_value = rotation4(v),
                    "startValue" => a.start_value = pad(vec3(v, [0.0; 3])),
                    "endValue" => a.end_value = pad(vec3(v, [0.0; 3])),
                    "curve" => {
                        if let GuiValue::Table(t) = v {
                            a.curve = t
                                .iter()
                                .filter_map(|(_, x)| match x {
                                    GuiValue::Num(x) => Some(*x),
                                    _ => None,
                                })
                                .collect();
                        }
                    }
                    _ => n.target_name = string(v),
                }
            }
            _ => log::debug!("[TeLuaGUI.{class:?}Bindings] Unreconized attribute : {key}"),
        }
    }
}

impl Engine for Builder {
    fn call(&mut self, _call: &Call) -> Reply {
        Reply::None
    }

    fn read_script(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        let p: String = path.iter().map(|&c| char::from(c)).collect();
        self.files.read(&resolve(&self.files, &p)?)
    }

    fn gui_layout(&mut self, class: &str, entries: Option<&[(GuiKey, GuiValue)]>) -> usize {
        let Some(entries) = entries else {
            log::warn!("{class}Bindings:: the lua value is not a table");
            return 0;
        };
        let Some(c) = Class::from_name(class) else {
            return 0;
        };
        let id = self.nodes.len();
        let mut node = Node::new(c);
        if c.is_anim() {
            node.anim = Some(LinearAnim::new(0.0, [0.0; 4], [0.0; 4], Vec::new()));
        }
        self.nodes.push(node);
        for (k, v) in entries {
            match k {
                GuiKey::Name(name) => {
                    let name = String::from_utf8_lossy(name).into_owned();
                    self.attribute(id, &name, v);
                }
                GuiKey::Index(_) => {
                    if let Some(ch) = child(v) {
                        self.add_child(id, ch);
                    }
                }
                GuiKey::Other(_) => {}
            }
        }
        let name = self.nodes[id].name.clone();
        if !name.is_empty() {
            if self.names.contains_key(&(c, name.clone())) {
                log::warn!("{class}Bindings:: multiple objects with name {name}");
                // The new object is deleted: left unreachable here.
                let ch = std::mem::take(&mut self.nodes[id].children);
                for x in ch {
                    self.nodes[x].parent = None;
                }
                return 0;
            }
            self.names.insert((c, name), id);
        }
        id + 1
    }

    fn event(&mut self, event: &Event<'_>) {
        if let Event::Error(_, msg) = event {
            log::warn!("layout file error: {}", String::from_utf8_lossy(msg));
        }
    }
}

impl Gui {
    /// `TeLuaGUI::load(path)`: the file (resolved) run in a fresh state;
    /// `None` when it is missing.
    pub fn load(files: &A4Files, path: &str) -> Option<Gui> {
        let Some(file) = resolve(files, path) else {
            log::warn!("no layout file {path}");
            return None;
        };
        let builder = Rc::new(RefCell::new(Builder {
            files: files.clone(),
            nodes: Vec::new(),
            names: HashMap::new(),
        }));
        let engine: EngineRef = builder.clone();
        let mut numbers = HashMap::new();
        let mut strings = HashMap::new();
        let text_format = {
            let mut state = GuiState::new(&engine, Options::default()).ok()?;
            state.run_file(file.as_bytes());
            for g in NUMBER_GLOBALS {
                if let Ok(Some(BootValue::Num(x))) = state.read(g) {
                    numbers.insert(g.to_string(), x);
                }
            }
            for g in STRING_GLOBALS {
                if let Ok(Some(BootValue::Str(x))) = state.read(g) {
                    strings.insert(g.to_string(), x);
                }
            }
            match state.read("textFormat") {
                Ok(Some(BootValue::Str(s))) => Some(s),
                _ => None,
            }
        };
        drop(engine);
        let b = Rc::try_unwrap(builder).ok()?.into_inner();
        let mut gui = Gui {
            path: file,
            nodes: b.nodes,
            text_format,
            numbers,
            strings,
        };
        // An animation's `layout` names its target.
        for i in 0..gui.nodes.len() {
            if let Some(t) = gui.nodes[i].target_name.clone() {
                gui.nodes[i].target = gui.find(&t);
            }
        }
        Some(gui)
    }

    /// The layout named `name` (the screens' per-class lookups; the names
    /// of the data's files are unique across classes).
    pub fn find(&self, name: &str) -> Option<Id> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// The size `updateSize` gives `id` in a parent of size `parent`.
    pub fn own_size(&self, id: Id, parent: (f32, f32)) -> (f32, f32) {
        self.size_of(id, Some(parent))
    }

    pub fn node(&self, id: Id) -> &Node {
        &self.nodes[id]
    }

    pub fn node_mut(&mut self, id: Id) -> &mut Node {
        &mut self.nodes[id]
    }

    /// Whether `id` and all its ancestors are visible.
    pub fn shown(&self, id: Id) -> bool {
        let mut at = Some(id);
        while let Some(i) = at {
            if !self.nodes[i].visible {
                return false;
            }
            at = self.nodes[i].parent;
        }
        true
    }

    /// `setText` on a text layout at `now` ms: the auto-scroll restarts
    /// (`TeExtendedTextLayout::setText` @ 0x004c0280: deadline = now +
    /// delay).
    pub fn set_text(&mut self, id: Id, markup: Vec<u8>, now: f64) {
        let n = &mut self.nodes[id];
        n.markup = markup;
        n.text_dirty = true;
        n.scroll = None;
        n.scroll_offset = 0.0;
        n.scroll_deadline = now + f64::from(n.auto_scroll_delay);
    }

    /// A string global of the file ([`STRING_GLOBALS`]), as text.
    pub fn string(&self, name: &str) -> Option<String> {
        self.strings
            .get(name)
            .map(|b| b.iter().map(|&c| char::from(c)).collect())
    }

    /// Moves `child` to the end of `parent`'s children (`addChild`).
    pub fn reparent(&mut self, child: Id, parent: Id) {
        if let Some(old) = self.nodes[child].parent {
            self.nodes[old].children.retain(|&x| x != child);
        }
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    /// The tree roots of the file (its top-level layouts), in order.
    pub fn roots(&self) -> Vec<Id> {
        (0..self.nodes.len())
            .filter(|&i| self.nodes[i].parent.is_none() && !self.nodes[i].class.is_anim())
            .collect()
    }

    /// Binds an animation to the layout it drives.
    pub fn bind(&mut self, anim: Id, target: Id) {
        self.nodes[anim].target = Some(target);
    }

    /// `play()` of an animation at `now`.
    pub fn play(&mut self, anim: Id, now: f64) {
        let Some(a) = self.nodes[anim].anim.as_mut() else {
            return;
        };
        let step = a.play(now);
        self.apply(anim, step.value);
    }

    /// `stop()` of an animation.
    pub fn stop(&mut self, anim: Id) {
        if let Some(a) = self.nodes[anim].anim.as_mut() {
            a.stop();
        }
    }

    pub fn playing(&self, anim: Id) -> bool {
        self.nodes[anim].anim.as_ref().is_some_and(|a| a.playing)
    }

    fn apply(&mut self, anim: Id, value: Option<[f32; 4]>) {
        let (Some(v), Some(t)) = (value, self.nodes[anim].target) else {
            return;
        };
        match self.nodes[anim].class {
            Class::AnchorAnim => self.nodes[t].anchor = [v[0], v[1], v[2]],
            Class::PositionAnim => self.nodes[t].position = [v[0], v[1], v[2]],
            Class::ColorAnim => self.nodes[t].colour = v.map(|x| x as u8),
            Class::RotationAnim => self.nodes[t].rotation = v[3],
            _ => {}
        }
    }

    /// The frame's update of the animations and auto-scrolls at `now`:
    /// the animations that finished (`onFinished`), in tree order.
    pub fn tick(&mut self, now: f64) -> Vec<Id> {
        let mut finished = Vec::new();
        for i in 0..self.nodes.len() {
            let Some(a) = self.nodes[i].anim.as_mut() else {
                continue;
            };
            let step = a.update(now);
            self.apply(i, step.value);
            if step.finished {
                finished.push(i);
            }
        }
        for n in &mut self.nodes {
            scroll(n, now);
        }
        finished
    }

    /// The sizes and positions of `root` and its descendants in a parent
    /// of size `parent` whose centre is at `origin` (window-centred
    /// coordinates); `place` replaces the root's own position (a list
    /// placing a layout of another file).
    pub fn update(
        &mut self,
        root: Id,
        parent: (f32, f32),
        origin: (f32, f32),
        place: Option<(f32, f32)>,
    ) {
        self.update_node(root, Some(parent), origin, place);
    }

    fn size_of(&self, id: Id, parent: Option<(f32, f32)>) -> (f32, f32) {
        let n = &self.nodes[id];
        let mut s = if !n.size_percent {
            (n.size[0].abs(), n.size[1].abs())
        } else if let Some(p) = parent {
            percent_size(
                (n.size[0], n.size[1]),
                p,
                n.ratio,
                n.ratio_mode,
                n.safe_area_ratio,
            )
        } else {
            (0.0, 0.0)
        };
        // `TeTextLayout::updateSize` @ 0x004bf6e0: the layout takes the
        // block's height (its width too when `SingleLine`).
        if n.class == Class::Text {
            if let Some(b) = &n.block {
                s.1 = b.height as f32;
                if n.single_line {
                    s.0 = b.width as f32;
                }
            }
        }
        s
    }

    fn update_node(
        &mut self,
        id: Id,
        parent: Option<(f32, f32)>,
        origin: (f32, f32),
        place: Option<(f32, f32)>,
    ) {
        let size = self.size_of(id, parent);
        let n = &self.nodes[id];
        let local = match place {
            Some(p) => p,
            None if !n.position_percent => (n.position[0], n.position[1]),
            None => match parent {
                Some(p) => percent_position(
                    (n.position[0], n.position[1]),
                    (n.anchor[0], n.anchor[1]),
                    size,
                    p,
                ),
                None => (0.0, 0.0),
            },
        };
        let world = (origin.0 + local.0, origin.1 + local.1);
        let children = n.children.clone();
        if n.class == Class::List {
            let sizes: Vec<_> = children
                .iter()
                .map(|&c| self.size_of(c, Some(size)))
                .collect();
            let n = &self.nodes[id];
            let (fitted, centres) = list_layout(
                size,
                &sizes,
                n.direction,
                n.minimum_margin,
                n.maximum_margin,
            );
            let n = &mut self.nodes[id];
            n.computed_size = fitted;
            n.world = world;
            for (c, at) in children.into_iter().zip(centres) {
                self.update_node(c, Some(fitted), world, Some(at));
            }
            return;
        }
        let n = &mut self.nodes[id];
        n.computed_size = size;
        n.world = world;
        for c in children {
            self.update_node(c, Some(size), world, None);
        }
    }

    /// Lays out the texts whose markup or box width changed (fonts read
    /// through `fonts`).
    pub fn layout_texts(&mut self, files: &A4Files, fonts: &mut FontCache) {
        for n in &mut self.nodes {
            if !n.class.is_text() {
                continue;
            }
            let width = n.computed_size.0 as i32;
            let stale = n.text_dirty || n.block.as_ref().is_some_and(|b| b.frame_width != width);
            if !stale {
                continue;
            }
            n.text_dirty = false;
            let m = text::parse_markup(&n.markup);
            // The font at offset 0; without one nothing is built
            // (`[TeTextBase2::build()] Warning : font missing`).
            let font = m.fonts.first().and_then(|(_, p)| fonts.get(files, p));
            n.block = font.as_ref().map(|f| {
                text::build(
                    f,
                    m,
                    Frame {
                        width,
                        single_line: n.single_line,
                        inter_line: n.inter_line,
                    },
                )
            });
            n.font = font;
        }
    }

    /// The rectangle `id` covers in a `width × height` window.
    pub fn rect(&self, id: Id, width: u32, height: u32) -> Rect {
        let n = &self.nodes[id];
        crate::layout::window_rect(n.world, n.computed_size, width, height)
    }

    /// The buttons under `root` in draw order (bottom first), visible ones
    /// only.
    fn buttons(&self, root: Id, out: &mut Vec<Id>) {
        let n = &self.nodes[root];
        if !n.visible {
            return;
        }
        if matches!(n.class, Class::Button | Class::Checkbox) {
            out.push(root);
        }
        for &c in &n.children {
            self.buttons(c, out);
        }
    }

    fn hit(&self, b: Id, x: f64, y: f64, width: u32, height: u32) -> bool {
        let zone = self.nodes[b].hit_zone.unwrap_or(b);
        self.rect(zone, width, height).contains(x, y)
    }

    /// A press at window point (`x`, `y`) on the tree of `root`: the
    /// topmost visible button hit takes it (state down when enabled);
    /// `true` when it consumed the press.
    pub fn press(&mut self, root: Id, x: f64, y: f64, width: u32, height: u32) -> bool {
        let mut bs = Vec::new();
        self.buttons(root, &mut bs);
        for &b in bs.iter().rev() {
            if self.hit(b, x, y, width, height) {
                let n = &mut self.nodes[b];
                if n.enabled && n.class == Class::Button {
                    n.pressed = true;
                    n.state = State::Down;
                }
                // `clickPassThrough`: not consumed, the buttons under it
                // get the press too.
                if !n.click_pass_through {
                    return true;
                }
            }
        }
        false
    }

    /// The pointer moved to (`x`, `y`): a pressed button shows down
    /// inside and up outside; otherwise the topmost button hit rolls over
    /// (unless `taken`, a layer above took the roll-over) and the others
    /// go back up.
    pub fn hover(&mut self, root: Id, x: f64, y: f64, width: u32, height: u32, taken: &mut bool) {
        let mut bs = Vec::new();
        self.buttons(root, &mut bs);
        for &b in bs.iter().rev() {
            let inside = self.hit(b, x, y, width, height);
            let n = &mut self.nodes[b];
            if !n.enabled || n.class == Class::Checkbox {
                continue;
            }
            if n.pressed {
                n.state = if inside { State::Down } else { State::Up };
            } else if inside && !*taken {
                n.state = State::RollOver;
                *taken = true;
            } else {
                n.state = State::Up;
            }
        }
    }

    /// A release at (`x`, `y`): the pressed buttons let go; the topmost
    /// visible button hit validates its click if the press was on it.
    /// Returns whether a button consumed the release and the button
    /// validated, if any.
    pub fn release(
        &mut self,
        root: Id,
        x: f64,
        y: f64,
        width: u32,
        height: u32,
    ) -> (bool, Option<Id>) {
        let mut bs = Vec::new();
        self.buttons(root, &mut bs);
        for &b in &bs {
            let inside = self.hit(b, x, y, width, height);
            let n = &mut self.nodes[b];
            if n.pressed {
                n.pressed = false;
                n.released = true;
                n.state = if inside { State::RollOver } else { State::Up };
            }
        }
        let mut result = (false, None);
        for &b in bs.iter().rev() {
            if self.hit(b, x, y, width, height) {
                let n = &mut self.nodes[b];
                let through = n.click_pass_through;
                let valid = if n.class == Class::Checkbox {
                    // 0x004dd070: a release inside toggles it, whatever
                    // its state and wherever the press was: active (0)
                    // becomes unactive (1), any other state active.
                    n.check = if n.check == 0 { 1 } else { 0 };
                    true
                } else {
                    n.enabled && n.released
                };
                if valid && (result.1.is_none() || !through) {
                    result.1 = Some(b);
                }
                if !through {
                    result.0 = true;
                    break;
                }
            }
        }
        for n in &mut self.nodes {
            n.released = false;
            n.pressed = false;
        }
        result
    }

    /// Forgets presses and roll-overs (a layer hidden).
    pub fn reset_input(&mut self) {
        for n in &mut self.nodes {
            n.pressed = false;
            n.released = false;
            if n.class == Class::Button && n.enabled {
                n.state = State::Up;
            }
        }
    }

    /// The picture paths the visible sprites under `root` show.
    pub fn pictures(&self, root: Id, out: &mut Vec<String>) {
        let n = &self.nodes[root];
        if !n.visible {
            return;
        }
        if let Some(p) = &n.image {
            out.push(p.clone());
        }
        for &c in &n.children {
            self.pictures(c, out);
        }
    }

    /// Draws `root`'s tree into `out` (`width × height` XRGB).
    pub fn draw(&self, root: Id, out: &mut [u32], width: u32, height: u32, painter: &dyn Painter) {
        let tint = self.inherited_colour(root);
        self.draw_node(root, out, width, height, painter, None, tint);
    }

    /// The colour `id`'s ancestors pass down (the product of theirs).
    fn inherited_colour(&self, id: Id) -> [u8; 4] {
        let mut c = [255u8; 4];
        let mut at = self.nodes[id].parent;
        while let Some(i) = at {
            c = modulate(c, self.nodes[i].colour);
            at = self.nodes[i].parent;
        }
        c
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_node(
        &self,
        id: Id,
        out: &mut [u32],
        width: u32,
        height: u32,
        painter: &dyn Painter,
        clip: Option<Clip>,
        tint: [u8; 4],
    ) {
        let n = &self.nodes[id];
        if !n.visible {
            return;
        }
        let tint = modulate(tint, n.colour);
        let rect = self.rect(id, width, height);
        let mut clip = clip;
        match n.class {
            Class::Sprite => {
                if !painter.video(self, id, out, width, height, rect) {
                    let pic = n.image.as_deref().and_then(|p| painter.picture(p));
                    if let (Some(pic), true) = (pic, n.rotation != 0.0) {
                        draw_rotated(out, width, height, pic, rect, n.rotation, tint);
                    } else if let Some(pic) = pic {
                        let r = if n.size_percent {
                            rect
                        } else {
                            let c = crate::layout::pixel_snap(n.world);
                            crate::layout::window_rect(c, n.computed_size, width, height)
                        };
                        draw_picture(out, width, height, pic, r, clip, tint);
                    }
                }
            }
            Class::Text | Class::ExtendedText => {
                if let (Some(b), Some(font)) = (&n.block, &n.font) {
                    // The text's origin: the layout's top-left, floored,
                    // plus ½ where the window's size is odd
                    // (@ 0x004bfa85..0x004bfbaa); the scroll moves it up.
                    let left = n.world.0 - n.computed_size.0 / 2.0;
                    let top = n.world.1 - n.computed_size.1 / 2.0 - n.scroll_offset;
                    let half = |s: u32| if s % 2 == 1 { 0.5 } else { 0.0 };
                    let ox = f64::from(width) / 2.0 + f64::from(left.floor()) + half(width);
                    let oy = f64::from(height) / 2.0 + f64::from(top.floor()) + half(height);
                    text::draw(
                        out,
                        width,
                        height,
                        font,
                        b,
                        ox.round() as i64,
                        oy.round() as i64,
                        clip,
                        tint,
                    );
                }
            }
            Class::Clip => {
                let c = pixel_span(rect, width, height);
                clip = Some(match clip {
                    Some(o) => (o.0.max(c.0), o.1.min(c.1), o.2.max(c.2), o.3.min(c.3)),
                    None => c,
                });
            }
            _ => {}
        }
        if matches!(n.class, Class::Button | Class::Checkbox) {
            if let Some(s) = n.state_layout() {
                self.draw_node(s, out, width, height, painter, clip, tint);
            }
            for &c in &n.children {
                if !n.is_state_layout(c) {
                    self.draw_node(c, out, width, height, painter, clip, tint);
                }
            }
        } else {
            for &c in &n.children {
                self.draw_node(c, out, width, height, painter, clip, tint);
            }
        }
    }

    /// A digest of what [`Gui::draw`] reads under `root`.
    pub fn hash_into(&self, root: Id, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        let n = &self.nodes[root];
        n.visible.hash(h);
        if !n.visible {
            return;
        }
        n.anchor.map(f32::to_bits).hash(h);
        n.position.map(f32::to_bits).hash(h);
        n.colour.hash(h);
        (n.check, n.rotation.to_bits()).hash(h);
        (&n.image, n.state, n.scroll_offset.to_bits(), &n.markup).hash(h);
        n.block.is_some().hash(h);
        for &c in &n.children {
            self.hash_into(c, h);
        }
    }
}

/// The auto-scroll of an extended text (@ 0x004bc63a..0x004bc83e): after
/// the delay, a text taller than its box scrolls once to its end at
/// `speed` box heights of overflow per second. The animation's curve is
/// `[UNKNOWN]` (linear here).
fn scroll(n: &mut Node, now: f64) {
    if n.class != Class::ExtendedText {
        return;
    }
    let Some(b) = &n.block else { return };
    let (content, frame) = (b.height as f32, n.computed_size.1);
    if frame <= 0.0 || content <= frame {
        n.scroll_offset = 0.0;
        return;
    }
    if n.scroll.is_none() && now >= n.scroll_deadline {
        let speed = f64::from(n.auto_scroll_speed) / 1000.0;
        let ms = if speed > 0.0 {
            (f64::from(content) / f64::from(frame) - 1.0).abs() / speed
        } else {
            f64::INFINITY
        };
        n.scroll = Some((n.scroll_deadline, ms));
    }
    if let Some((start, ms)) = n.scroll {
        let p = if ms > 0.0 {
            ((now - start) / ms).clamp(0.0, 1.0)
        } else {
            1.0
        };
        n.scroll_offset = (p * f64::from(content - frame)) as f32;
    }
}

/// A picture over `rect` turned by `degrees` about its centre (a rotation
/// animation's layout): each pixel whose centre falls in the turned
/// rectangle takes the picture's bilinear sample there. `[INFERRED]` the
/// sense (a positive angle turns x toward y, clockwise on the y-down
/// screen) and GL's sampling of a turned quad.
pub fn draw_rotated(
    out: &mut [u32],
    width: u32,
    height: u32,
    pic: &Picture,
    rect: Rect,
    degrees: f32,
    tint: [u8; 4],
) {
    let (cx, cy) = (rect.left + rect.width / 2.0, rect.top + rect.height / 2.0);
    let a = f64::from(degrees).to_radians();
    let (s, c) = (libm::sin(a), libm::cos(a));
    let (hw, hh) = (rect.width / 2.0, rect.height / 2.0);
    let r = (hw * hw + hh * hh).sqrt();
    let i0 = ((cx - r).floor().max(0.0)) as i64;
    let i1 = ((cx + r).ceil() as i64).min(i64::from(width));
    let j0 = ((cy - r).floor().max(0.0)) as i64;
    let j1 = ((cy + r).ceil() as i64).min(i64::from(height));
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let (pw, ph) = (f64::from(pic.width), f64::from(pic.height));
    for j in j0..j1 {
        for i in i0..i1 {
            let (dx, dy) = (i as f64 + 0.5 - cx, j as f64 + 0.5 - cy);
            // Back to the unturned rectangle.
            let (u, v) = (c * dx + s * dy, -s * dx + c * dy);
            if u.abs() > hw || v.abs() > hh {
                continue;
            }
            let tx = (u + hw) / rect.width * pw - 0.5;
            let ty = (v + hh) / rect.height * ph - 0.5;
            let (x0, y0) = (tx.floor(), ty.floor());
            let (fx, fy) = (tx - x0, ty - y0);
            let tap = |x: f64, y: f64| pic.texel_clamped(x as i64, y as i64);
            let q = [
                tap(x0, y0),
                tap(x0 + 1.0, y0),
                tap(x0, y0 + 1.0),
                tap(x0 + 1.0, y0 + 1.0),
            ];
            let px: [u8; 4] = std::array::from_fn(|k| {
                let top = f64::from(q[0][k]) * (1.0 - fx) + f64::from(q[1][k]) * fx;
                let bot = f64::from(q[2][k]) * (1.0 - fx) + f64::from(q[3][k]) * fx;
                (top * (1.0 - fy) + bot * fy).round() as u8
            });
            let o = j as usize * width as usize + i as usize;
            out[o] = crate::sprite::blend(out[o], modulate(px, tint));
        }
    }
}

/// A rotation value (`{x, y, z, angle}`): the angle in the fourth place.
fn rotation4(v: &GuiValue) -> [f32; 4] {
    let GuiValue::Table(t) = v else {
        return [0.0; 4];
    };
    let mut out = [0f32; 4];
    for (k, x) in t {
        if let (GuiKey::Index(i), GuiValue::Num(x)) = (k, x) {
            let i = *i as usize;
            if (1..=4).contains(&i) {
                out[i - 1] = *x as f32;
            }
        }
    }
    out
}

/// The pixels whose centre is in `rect`: columns, rows.
fn pixel_span(rect: Rect, width: u32, height: u32) -> Clip {
    let i0 = (rect.left - 0.5).ceil().max(0.0) as i64;
    let i1 = ((rect.left + rect.width - 0.5).ceil() as i64).min(i64::from(width));
    let j0 = (rect.top - 0.5).ceil().max(0.0) as i64;
    let j1 = ((rect.top + rect.height - 0.5).ceil() as i64).min(i64::from(height));
    (i0, i1, j0, j1)
}

/// Two colours multiplied channel by channel, rounded (`× 255` is the
/// identity).
pub fn modulate(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    std::array::from_fn(|k| ((u32::from(a[k]) * u32::from(b[k]) + 127) / 255) as u8)
}

/// A tap of a GUI picture beyond its edges ([`crate::sprite`]'s rule: the
/// picture sits bottom row first in a power-of-two texture; clamped left
/// and below, the padding right and above taken as transparent).
fn gui_tap(pic: &Picture, x: i64, y: i64) -> [u8; 4] {
    let (w, h) = (i64::from(pic.width), i64::from(pic.height));
    if x >= w || y < 0 {
        return [0; 4];
    }
    pic.texel(x.max(0), y.min(h - 1))
}

/// A picture stretched over `rect` (`GL_LINEAR`, blended), its texels
/// multiplied by `tint`, inside `clip`.
pub fn draw_picture(
    out: &mut [u32],
    width: u32,
    height: u32,
    pic: &Picture,
    rect: Rect,
    clip: Option<Clip>,
    tint: [u8; 4],
) {
    let tinted;
    let pic = if tint == [255; 4] {
        pic
    } else {
        let mut p = pic.clone();
        for px in p.rgba.as_chunks_mut::<4>().0 {
            *px = modulate(*px, tint);
        }
        tinted = p;
        &tinted
    };
    let tap = |x: i64, y: i64| gui_tap(pic, x, y);
    let Some((x0, x1, y0, y1)) = clip else {
        draw_stretched(out, width, height, pic, rect, Mode::Blend, &tap);
        return;
    };
    // Clipped: drawn into a copy, the pixels inside the clip kept.
    let mut tmp = out.to_vec();
    draw_stretched(&mut tmp, width, height, pic, rect, Mode::Blend, &tap);
    for j in y0.max(0)..y1.min(i64::from(height)) {
        for i in x0.max(0)..x1.min(i64::from(width)) {
            let o = j as usize * width as usize + i as usize;
            out[o] = tmp[o];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_gaps_are_capped_at_0_045_of_the_height_and_go_negative() {
        // questions.lua at 768: S = (0, 691.2); 110 × 110 answers.
        let s = (0.0, (0.9f64 * 768.0) as f32);
        let gap = |n: usize| {
            let sizes = vec![(110.0, 110.0); n];
            let (_, c) = list_layout(s, &sizes, [0.0, 1.0, 0.0], [-1e6; 3], [0.0, 0.05, 0.0]);
            f64::from(c[1].1 - c[0].1) - 110.0
        };
        for n in 2..=5 {
            assert!((gap(n) - 34.56).abs() < 1e-3, "{n}: {}", gap(n));
        }
        assert!((gap(6) - 6.24).abs() < 1e-3);
        assert!((gap(7) + 13.133).abs() < 1e-2);
        assert!((gap(9) + 37.35).abs() < 1e-2);
        // Centred on the list: the first and last are symmetric, x 0.
        let (fit, c) = list_layout(
            s,
            &[(110.0, 110.0); 3],
            [0.0, 1.0, 0.0],
            [-1e6; 3],
            [0.0, 0.05, 0.0],
        );
        assert_eq!(fit.0, 110.0);
        assert_eq!(c[0].0, 0.0);
        assert!((c[0].1 + c[2].1).abs() < 1e-4);
    }

    /// A file set holding `files` (path, bytes).
    fn mem(files: &[(&str, Vec<u8>)]) -> A4Files {
        let mut m = atlantis_data::files::MemFiles::new();
        for (p, d) in files {
            m.insert(p, d.clone());
        }
        A4Files::new(std::sync::Arc::new(m))
    }

    /// A PNG's signature and `IHDR` size: all [`image_size`] reads.
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut d = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        d.extend(w.to_be_bytes());
        d.extend(h.to_be_bytes());
        d.extend([8, 6, 0, 0, 0]);
        d
    }

    #[test]
    fn a_pixel_button_takes_its_up_pictures_size() {
        let f = mem(&[
            (
                "b.lua",
                b"TeButtonLayout { name = \"b\", anchor = {1.0, 1.0}, position = {1.0, 1.0}, \
                  upLayout = TeSpriteLayout { name = \"up\", image = \"a.png\" } }"
                    .to_vec(),
            ),
            ("a.png", png_header(120, 80)),
        ]);
        let mut g = Gui::load(&f, "b.lua").unwrap();
        let (b, up) = (g.find("b").unwrap(), g.find("up").unwrap());
        assert_eq!(&g.node(b).size[..2], &[120.0, 80.0]);
        assert_eq!(g.node(up).ratio, 1.5);
        // Bottom-right corner of a 1365 × 768 window.
        g.update(b, (1365.0, 768.0), (0.0, 0.0), None);
        let r = g.rect(b, 1365, 768);
        assert_eq!((r.left, r.top), (1245.0, 688.0));
    }

    #[test]
    fn a_sized_sprite_keeps_its_size_and_takes_the_ratio() {
        let f = mem(&[
            (
                "s.lua",
                b"TeSpriteLayout { name = \"s\", image = \"a.png\", sizeType = Percent, \
                  size = {1.0, 1.0}, ratioMode = LetterBox }"
                    .to_vec(),
            ),
            ("a.png", png_header(400, 300)),
        ]);
        let mut g = Gui::load(&f, "s.lua").unwrap();
        let s = g.find("s").unwrap();
        g.update(s, (1365.0, 768.0), (0.0, 0.0), None);
        assert_eq!(g.node(s).computed_size, (1024.0, 768.0));
    }

    #[test]
    fn vectors_keep_missing_components() {
        let t = GuiValue::Table(vec![(GuiKey::Index(1.0), GuiValue::Num(0.25))]);
        assert_eq!(vec3(&t, [0.5, 0.5, 0.5]), [0.25, 0.5, 0.5]);
    }

    #[test]
    fn the_projection_maps_the_window_centre_to_the_origin_y_down() {
        let m = projection(1365, 768);
        assert_eq!(m[0].to_bits(), 0x3ac0_0c01);
        assert_eq!(m[5].to_bits(), 0xbb2a_aaab);
        assert_eq!(m[10].to_bits(), 0xba00_0000);
    }
}
