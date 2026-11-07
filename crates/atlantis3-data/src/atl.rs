//! The menu layouts `Datas\menu\desc\*.atl` and their tooltips
//! `Datas\loc\text\<language>\i_*.txt`.
//!
//! A layout (read by the interface loader @ 0x004230c0) is a list of
//! elements:
//!
//! ```text
//! #ELEMENT
//! {
//!     nomref  BT_QUIT
//!     type    BT_ANIME
//!     nomspr  bank_name
//!     pos     0 0
//!     bulle   on
//!     text    on
//!     {
//!         align   center center
//!         font    AtlFont_Credit
//!     }
//! }
//! ```
//!
//! Every line is `key values...`, cut by the engine's tokenizer
//! (separators: space, tab, CR, LF, `,`, `;`, `(`, `)`, @ 0x00445fc0).
//! Two lines may come before or between the elements:
//! `#SOUND_WHEN_OPENED name` and `#SOUND_WHEN_CLOSED name`. Unknown keys
//! and lines are logged by the original and skipped.
//!
//! A tooltip file (read @ 0x00422970) names elements of a layout by their
//! `nomref`: `#ELEMENT NAME { bulle text }` gives the tooltip, and
//! `texte { lines }` the page text of a `CLICK_TEXT` element (the credits).
//!
//! The menus (@ 0x00451570) are `i_debut`, `i_princ`, `i_load`, `i_save`,
//! `i_outil`, `i_option`, `i_langue_<language or dvd>`, `i_cdchange` and
//! `i_credit`, each with the tooltip file of the same name (`i_langue.txt`
//! for the language page) except `i_load`, which has none. The sprite
//! banks of the elements are in `Datas\menu\spr_<screen width>\32bits\`
//! (@ 0x00417eb0).

/// The separators of the engine's tokenizer (@ 0x00445fc0).
pub const SEPARATORS: &[u8] = b" \t\r\n,;()";

/// Whether `c` is one of the tokenizer's [`SEPARATORS`].
pub fn is_separator(c: u8) -> bool {
    crate::tokens::is_separator(c)
}

/// A text with the read cursor of the engine's string class (its field
/// +0x10): the line, token and raw-line readers all advance it.
#[derive(Clone, Debug)]
pub struct TextCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> TextCursor<'a> {
    /// A cursor at the start of `data`.
    pub fn new(data: &'a [u8]) -> TextCursor<'a> {
        TextCursor { data, pos: 0 }
    }

    /// Whether the cursor is at the end (@ 0x00445df0).
    pub fn eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// What is left after the cursor.
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    /// The next line (@ 0x00444f60): skips separators (so blank lines are
    /// never returned before the end), then reads up to a CR or an LF,
    /// which is consumed and not returned.
    pub fn line(&mut self) -> &'a [u8] {
        let d = self.data;
        let mut p = self.pos;
        while p < d.len() && is_separator(d[p]) {
            p += 1;
        }
        let start = p;
        while p < d.len() {
            let c = d[p];
            p += 1;
            if c == b'\r' || c == b'\n' {
                self.pos = p;
                return &d[start..p - 1];
            }
        }
        self.pos = p;
        &d[start..p]
    }

    /// The next token (@ 0x00445e00): skips separators, then reads up to a
    /// separator, which is consumed. Empty at the end.
    pub fn token(&mut self) -> &'a [u8] {
        let d = self.data;
        let mut p = self.pos;
        while p < d.len() && is_separator(d[p]) {
            p += 1;
        }
        let start = p;
        while p < d.len() {
            let c = d[p];
            p += 1;
            if is_separator(c) {
                self.pos = p;
                return &d[start..p - 1];
            }
        }
        self.pos = p;
        &d[start..p]
    }

    /// Skips one token without reading it (@ 0x00444e30): the `=` of the
    /// `key = value` lines, whatever it is.
    pub fn skip_token(&mut self) {
        let d = self.data;
        let mut p = self.pos;
        while p < d.len() && is_separator(d[p]) {
            p += 1;
        }
        while p < d.len() && !is_separator(d[p]) {
            p += 1;
        }
        self.pos = p;
    }

    /// The next raw line (@ 0x00445110): skips LFs only, then reads up to
    /// an LF; a CR is dropped and the byte after it taken instead. Leading
    /// blanks are kept and so is an empty line written CR LF.
    pub fn raw_line(&mut self) -> Vec<u8> {
        let d = self.data;
        let mut p = self.pos;
        while p < d.len() && d[p] == b'\n' {
            p += 1;
        }
        let mut out = Vec::new();
        while p < d.len() {
            let mut c = d[p];
            p += 1;
            if c == b'\r' {
                // A CR at the very end: the original reads the string's NUL
                // past it; nothing useful is lost by stopping here.
                if p >= d.len() {
                    break;
                }
                c = d[p];
                p += 1;
            }
            if c == b'\n' {
                break;
            }
            out.push(c);
        }
        self.pos = p;
        out
    }
}

/// The C library's `atoi` of the original (`atol` @ 0x006d4d0f): leading
/// white space, an optional sign, then decimal digits; wraps on overflow.
pub fn atoi(s: &[u8]) -> i32 {
    crate::tokens::atoi(s)
}

/// The C library's `atof` (@ 0x006d4da5): the longest number at the start
/// of `s` after white space (`[sign] digits [. digits] [e [sign] digits]`),
/// 0 when there is none.
pub fn atof(s: &[u8]) -> f64 {
    crate::tokens::atof_f64(s)
}

/// The compiler's float-to-int conversion (`__ftol`): truncation toward
/// zero; out of range or not a number gives `i32::MIN`.
pub fn ftol(v: f64) -> i32 {
    let t = v.trunc();
    if (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&t) {
        // In range: the cast is exact.
        t as i32
    } else {
        i32::MIN
    }
}

/// The element types the interface knows (@ 0x00425bc0, 0x00426470,
/// 0x00425010).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementType {
    /// `IMG`: a still image (the reader clears `selectable` and
    /// `clickable` when it reads this type).
    Img,
    /// `IMG_ANIME`.
    ImgAnime,
    /// `BT_ANIME`.
    BtAnime,
    /// `BT_ANIME2`.
    BtAnime2,
    /// `BT_IMG_ANIME`.
    BtImgAnime,
    /// `BT_LOCK` (4 of the 8 shipped ones name another `BT_LOCK` with
    /// `link`).
    BtLock,
    /// `BT_LOCK1`.
    BtLock1,
    /// `SLIDER` (the shipped ones carry `xmin`/`xmax` or `ymin`/`ymax`).
    Slider,
    /// `CLICK_TEXT`: a page of text (the credits); its text comes from the
    /// tooltip file's `texte` block.
    ClickText,
    /// `ZONE_TEXT` (none in the shipped files; clears `selectable` and
    /// `clickable` like `IMG`).
    ZoneText,
    /// `RDM_TEXT` (none in the shipped files; same as `ZONE_TEXT`).
    RdmText,
    /// Any other word, kept as read.
    Other(Vec<u8>),
}

impl ElementType {
    /// The type named `word` (case-sensitive, as the original compares).
    pub fn from_word(word: &[u8]) -> ElementType {
        match word {
            b"IMG" => ElementType::Img,
            b"IMG_ANIME" => ElementType::ImgAnime,
            b"BT_ANIME" => ElementType::BtAnime,
            b"BT_ANIME2" => ElementType::BtAnime2,
            b"BT_IMG_ANIME" => ElementType::BtImgAnime,
            b"BT_LOCK" => ElementType::BtLock,
            b"BT_LOCK1" => ElementType::BtLock1,
            b"SLIDER" => ElementType::Slider,
            b"CLICK_TEXT" => ElementType::ClickText,
            b"ZONE_TEXT" => ElementType::ZoneText,
            b"RDM_TEXT" => ElementType::RdmText,
            other => ElementType::Other(other.to_vec()),
        }
    }
}

/// Horizontal alignment codes of `align` (@ 0x004242b5..0x00424543).
pub const ALIGN_H_LEFT: i32 = 4;
/// `align right`.
pub const ALIGN_H_RIGHT: i32 = 1;
/// `align center` (horizontal).
pub const ALIGN_H_CENTER: i32 = 2;
/// `align judtify` (sic, horizontal).
pub const ALIGN_H_JUSTIFY: i32 = 3;
/// Vertical alignment codes of `align` (@ 0x00424551..0x0042474d).
pub const ALIGN_V_TOP: i32 = 0;
/// `align .. bottom`.
pub const ALIGN_V_BOTTOM: i32 = 1;
/// `align .. center` (vertical).
pub const ALIGN_V_CENTER: i32 = 2;
/// `align .. judtify` (sic, vertical).
pub const ALIGN_V_JUSTIFY: i32 = 3;

/// The font an element's text uses when `font` is not given (@ 0x00413ce0).
pub const DEFAULT_FONT: &[u8] = b"AtlFont_Default";

/// The frame time of an element without `fps` (@ 0x00413ce0: the f32
/// 0x3d888889, 1/15 s).
pub const DEFAULT_FRAME_TIME: f32 = 1.0 / 15.0;

/// One element of a layout. The defaults are those of the element reset
/// @ 0x00413ce0, which runs for every line before `#ELEMENT`.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// `nomref`: the name the code and the tooltips use (field +0x54).
    pub nomref: Vec<u8>,
    /// `type`, as written (field +0x8).
    pub type_word: Vec<u8>,
    /// `nomspr`: the sprite bank, without `.spr`; may hold a sub-folder
    /// (`save/...`) (field +0x44).
    pub nomspr: Option<Vec<u8>>,
    /// `pos x y [a b]`: four numbers read with `atof` and truncated; the
    /// shipped files give two. `x` is `i32::MIN` when `pos` is absent
    /// (fields +0x18..+0x24). The third and fourth are `[UNKNOWN]`.
    pub pos: [i32; 4],
    /// `xmin`, `xmax`, `ymin`, `ymax` (fields +0x30..+0x3c), default 0:
    /// the travel of a `SLIDER` `[INFERRED: only sliders carry them]`.
    pub xmin: i32,
    pub xmax: i32,
    pub ymin: i32,
    pub ymax: i32,
    /// `displayed` (+0x221f), default true.
    pub displayed: bool,
    /// `selectable` (+0x2219), default true.
    pub selectable: bool,
    /// `clickable` (+0x221a), default true.
    pub clickable: bool,
    /// `permanent` (+0x2208): the bank stays loaded (@ 0x00425010), default
    /// false.
    pub permanent: bool,
    /// `bulle` (+0x221d): shows its tooltip, default false.
    pub bulle: bool,
    /// `loop` (+0x2220), default false.
    pub looping: bool,
    /// `playOnce` (+0x2221), default false.
    pub play_once: bool,
    /// `fps v` stored as the frame time 1/v (+0x2224).
    pub frame_time: f32,
    /// `zoom` (+0x2228), default 1.
    pub zoom: f32,
    /// `link NAME`: another element, by `nomref` (+0x68; logged
    /// @ 0x004268cb when it does not exist). In the shipped files: the next
    /// page of a `CLICK_TEXT` and the other button of a `BT_LOCK` pair.
    pub link: Option<Vec<u8>>,
    /// `soundOnClickUp v`: run as the script command `(PlaySound = v)`
    /// (@ 0x00426945).
    pub sound_on_click_up: Option<Vec<u8>>,
    /// `soundOnClickDown v`.
    pub sound_on_click_down: Option<Vec<u8>>,
    /// `text on` (+0x221e) and the block after it.
    pub text: bool,
    /// `textpos x y w h` (+0x80..+0x8c), default `1 1` and the screen size.
    pub text_pos: [i32; 4],
    /// `color r g b` (+0x90..+0x98). Not reset between elements: an
    /// element without `color` keeps the previous element's.
    pub color: [i32; 3],
    /// `align h v` (+0x9c, +0xa0): `ALIGN_H_*`, `ALIGN_V_*` codes. Not
    /// reset between elements either.
    pub align_h: i32,
    pub align_v: i32,
    /// `font` (+0xa4), default [`DEFAULT_FONT`].
    pub font: Vec<u8>,
    /// The tooltip (`bulle` line of the tooltip file, +0x11f0).
    pub tooltip: Option<Vec<u8>>,
    /// The page text (`texte` block of the tooltip file, +0x138), lines
    /// joined with LF.
    pub page_text: Option<Vec<u8>>,
}

impl Element {
    /// A reset element (@ 0x00413ce0) for a screen of `screen` pixels,
    /// keeping `color` and `align` of `previous`.
    fn reset(screen: (i32, i32), previous: Option<&Element>) -> Element {
        let (color, align_h, align_v) =
            previous.map_or(([0; 3], 0, 0), |p| (p.color, p.align_h, p.align_v));
        Element {
            nomref: Vec::new(),
            type_word: Vec::new(),
            nomspr: None,
            pos: [i32::MIN, 0, 0, 0],
            xmin: 0,
            xmax: 0,
            ymin: 0,
            ymax: 0,
            displayed: true,
            selectable: true,
            clickable: true,
            permanent: false,
            bulle: false,
            looping: false,
            play_once: false,
            frame_time: DEFAULT_FRAME_TIME,
            zoom: 1.0,
            link: None,
            sound_on_click_up: None,
            sound_on_click_down: None,
            text: false,
            text_pos: [1, 1, screen.0, screen.1],
            color,
            align_h,
            align_v,
            font: DEFAULT_FONT.to_vec(),
            tooltip: None,
            page_text: None,
        }
    }

    /// The element's type.
    pub fn element_type(&self) -> ElementType {
        ElementType::from_word(&self.type_word)
    }

    /// Whether `pos` was given.
    pub fn has_pos(&self) -> bool {
        self.pos[0] != i32::MIN
    }
}

/// A structure error the original logs and goes on after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// A line outside an element that is neither `#ELEMENT` nor a sound
    /// line (logged @ 0x00424ef3).
    LineOutsideElement(Vec<u8>),
    /// The line after `#ELEMENT` is not `{` (@ 0x00423363); that line is
    /// consumed anyway.
    NoOpenBrace,
    /// An unknown key inside an element (@ 0x00424e9c), skipped.
    UnknownKey(Vec<u8>),
    /// The line after `text on` is not `{` (@ 0x00424220).
    NoTextBrace,
    /// The file ends inside an element (the original would loop for ever:
    /// its element loop has no end-of-file test).
    UnexpectedEnd,
}

/// A parsed `.atl` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// `#SOUND_WHEN_OPENED name` (field +0x44 of the interface).
    pub sound_when_opened: Option<Vec<u8>>,
    /// `#SOUND_WHEN_CLOSED name` (+0x58).
    pub sound_when_closed: Option<Vec<u8>>,
    /// The elements in file order (the interface's vector of 0x2288-byte
    /// descriptors).
    pub elements: Vec<Element>,
    /// The errors the original would log.
    pub errors: Vec<LayoutError>,
}

impl Layout {
    /// Reads an `.atl` file as the interface loader @ 0x004230c0 does, for
    /// a screen of `screen` (width, height) pixels (the default
    /// `textpos`).
    pub fn parse(data: &[u8], screen: (i32, i32)) -> Layout {
        let mut out = Layout::default();
        let mut f = TextCursor::new(data);
        while !f.eof() {
            let mut line = TextCursor::new(f.line());
            let key = line.token();
            match key {
                b"#SOUND_WHEN_OPENED" => {
                    out.sound_when_opened = Some(line.token().to_vec());
                    continue;
                }
                b"#SOUND_WHEN_CLOSED" => {
                    out.sound_when_closed = Some(line.token().to_vec());
                    continue;
                }
                b"#ELEMENT" => {}
                other => {
                    out.errors
                        .push(LayoutError::LineOutsideElement(other.to_vec()));
                    continue;
                }
            }
            let mut el = Element::reset(screen, out.elements.last());
            let mut line = TextCursor::new(f.line());
            if line.token() != b"{" {
                out.errors.push(LayoutError::NoOpenBrace);
            }
            let mut ended = false;
            loop {
                if f.eof() {
                    ended = true;
                    break;
                }
                let mut line = TextCursor::new(f.line());
                let key = line.token();
                if key == b"}" {
                    break;
                }
                if !read_key(key, &mut line, &mut f, &mut el, &mut out.errors) {
                    ended = true;
                    break;
                }
            }
            if ended {
                out.errors.push(LayoutError::UnexpectedEnd);
            }
            out.elements.push(el);
        }
        out
    }

    /// The element named `nomref` (the first one, as @ 0x00426db0).
    pub fn element(&self, nomref: &[u8]) -> Option<&Element> {
        self.elements.iter().find(|e| e.nomref == nomref)
    }

    /// Gives the elements their tooltips and page texts (@ 0x00422970).
    /// Returns the entries that name no element (the original logs
    /// "on a pas trouve %s" for them).
    pub fn apply_tooltips(&mut self, entries: &[TooltipEntry]) -> Vec<Vec<u8>> {
        let mut missing = Vec::new();
        for entry in entries {
            let Some(el) = self.elements.iter_mut().find(|e| e.nomref == entry.name) else {
                missing.push(entry.name.clone());
                continue;
            };
            for text in &entry.texts {
                match text {
                    TooltipText::Bulle(t) => el.tooltip = Some(t.clone()),
                    TooltipText::Texte(t) => el.page_text = Some(t.clone()),
                }
            }
        }
        missing
    }
}

/// Reads one key line of an element. Returns false when the file ended
/// inside a `text` block.
fn read_key(
    key: &[u8],
    line: &mut TextCursor<'_>,
    f: &mut TextCursor<'_>,
    el: &mut Element,
    errors: &mut Vec<LayoutError>,
) -> bool {
    fn on(line: &mut TextCursor<'_>) -> bool {
        line.token() == b"on"
    }
    match key {
        b"soundOnClickUp" => el.sound_on_click_up = Some(line.token().to_vec()),
        b"playOnce" => el.play_once = on(line),
        b"soundOnClickDown" => el.sound_on_click_down = Some(line.token().to_vec()),
        b"nomref" => el.nomref = line.token().to_vec(),
        b"link" => el.link = Some(line.token().to_vec()),
        b"selectable" => el.selectable = on(line),
        b"permanent" => el.permanent = on(line),
        b"clickable" => el.clickable = on(line),
        b"type" => {
            el.type_word = line.token().to_vec();
            if matches!(el.type_word.as_slice(), b"ZONE_TEXT" | b"RDM_TEXT" | b"IMG") {
                el.selectable = false;
                el.clickable = false;
            }
        }
        b"pos" => {
            for v in el.pos.iter_mut() {
                *v = ftol(atof(line.token()));
            }
        }
        b"displayed" => el.displayed = on(line),
        b"nomspr" => el.nomspr = Some(line.token().to_vec()),
        b"bulle" => el.bulle = on(line),
        b"text" => {
            if !on(line) {
                el.text = false;
                return true;
            }
            el.text = true;
            let mut l = TextCursor::new(f.line());
            if l.token() != b"{" {
                errors.push(LayoutError::NoTextBrace);
            }
            loop {
                if f.eof() {
                    return false;
                }
                let mut l = TextCursor::new(f.line());
                match l.token() {
                    b"}" => break,
                    b"align" => {
                        match l.token() {
                            b"left" => el.align_h = ALIGN_H_LEFT,
                            b"right" => el.align_h = ALIGN_H_RIGHT,
                            b"center" => el.align_h = ALIGN_H_CENTER,
                            b"judtify" => el.align_h = ALIGN_H_JUSTIFY,
                            _ => {}
                        }
                        match l.token() {
                            b"top" => el.align_v = ALIGN_V_TOP,
                            b"bottom" => el.align_v = ALIGN_V_BOTTOM,
                            b"center" => el.align_v = ALIGN_V_CENTER,
                            b"judtify" => el.align_v = ALIGN_V_JUSTIFY,
                            _ => {}
                        }
                    }
                    b"textpos" => {
                        for v in el.text_pos.iter_mut() {
                            *v = atoi(l.token());
                        }
                    }
                    b"color" => {
                        for v in el.color.iter_mut() {
                            *v = atoi(l.token());
                        }
                    }
                    b"font" => el.font = l.token().to_vec(),
                    // Other keys of the block are skipped without a message.
                    _ => {}
                }
            }
        }
        b"loop" => el.looping = on(line),
        // x87: 1.0f / (double) then stored as f32.
        b"fps" => el.frame_time = (1.0 / atof(line.token())) as f32,
        b"zoom" => el.zoom = atof(line.token()) as f32,
        b"xmin" => el.xmin = atoi(line.token()),
        b"xmax" => el.xmax = atoi(line.token()),
        b"ymin" => el.ymin = atoi(line.token()),
        b"ymax" => el.ymax = atoi(line.token()),
        other => errors.push(LayoutError::UnknownKey(other.to_vec())),
    }
    true
}

/// A text of a tooltip entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TooltipText {
    /// `bulle text`: the rest of the line after the word and one separator.
    Bulle(Vec<u8>),
    /// `texte` then `{`, lines, `}`: the lines joined with LF (empty when
    /// the `{` is missing).
    Texte(Vec<u8>),
}

/// One `#ELEMENT NAME { ... }` entry of a tooltip file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TooltipEntry {
    /// The `nomref` of the element.
    pub name: Vec<u8>,
    /// Its texts in file order.
    pub texts: Vec<TooltipText>,
}

/// Reads a tooltip file `i_*.txt` (@ 0x00422970). Lines that do not start
/// with `#ELEMENT` are skipped; keys other than `bulle` and `texte` are
/// logged by the original and skipped. Texts are Windows-1252 bytes.
pub fn parse_tooltips(data: &[u8]) -> Vec<TooltipEntry> {
    let mut out = Vec::new();
    let mut f = TextCursor::new(data);
    while !f.eof() {
        let mut line = TextCursor::new(f.line());
        if line.token() != b"#ELEMENT" {
            continue;
        }
        let mut entry = TooltipEntry {
            name: line.token().to_vec(),
            texts: Vec::new(),
        };
        // The line after must be `{` (logged when not, and consumed).
        f.line();
        loop {
            if f.eof() {
                break;
            }
            let mut line = TextCursor::new(f.line());
            match line.token() {
                b"}" => break,
                b"bulle" => entry.texts.push(TooltipText::Bulle(line.rest().to_vec())),
                b"texte" => entry.texts.push(TooltipText::Texte(read_texte(&mut f))),
                _ => {}
            }
        }
        out.push(entry);
    }
    out
}

/// The `{ ... }` block after `texte` (@ 0x00422d64..0x00422f89), read with
/// the raw-line reader: the lines up to one that is exactly `}`.
fn read_texte(f: &mut TextCursor<'_>) -> Vec<u8> {
    let first = f.raw_line();
    if TextCursor::new(&first).token() != b"{" {
        return Vec::new();
    }
    let mut text = Vec::new();
    let mut started = false;
    loop {
        if f.eof() {
            break;
        }
        let line = f.raw_line();
        if line == b"}" {
            break;
        }
        if started {
            text.push(b'\n');
        }
        text.extend_from_slice(&line);
        started = true;
    }
    text
}

/// The menus the interface loads (@ 0x00451570): the layout and its
/// tooltip file. The language page is `i_langue_<Langue>` (or
/// `i_langue_dvd` when the version is `DVD`): see [`langue_layout`].
pub const MENUS: [(&str, Option<&str>); 9] = [
    ("i_debut", Some("i_debut")),
    ("i_princ", Some("i_princ")),
    ("i_load", None),
    ("i_save", Some("i_save")),
    ("i_outil", Some("i_outil")),
    ("i_option", Some("i_option")),
    ("i_langue_", Some("i_langue")),
    ("i_cdchange", Some("i_cdchange")),
    ("i_credit", Some("i_credit")),
];

/// The layout of the language page (@ 0x00451941..0x00451a54):
/// `i_langue_dvd` when `version` is `DVD`, else `i_langue_<langue>`.
pub fn langue_layout(version: &[u8], langue: &[u8]) -> Vec<u8> {
    let mut name = b"i_langue_".to_vec();
    if version == b"DVD" {
        name.extend_from_slice(b"dvd");
    } else {
        name.extend_from_slice(langue);
    }
    name
}

/// The path of layout `name` in the install: `Datas/menu/desc/<name>.atl`
/// (`menu\desc\` @ 0x00715a68, `.atl` @ 0x0071b218, under `Datas\`).
pub fn layout_path(name: &str) -> String {
    format!("Datas/menu/desc/{name}.atl")
}

/// The path of tooltip file `name` for the text language `language`:
/// `Datas/loc/text/<language>/<name>.txt` (@ 0x00418240: `loc\` + `text\`
/// + the `SousTitre` folder).
pub fn tooltip_path(language: &str, name: &str) -> String {
    format!("Datas/loc/text/{language}/{name}.txt")
}

/// The path of the sprite bank of `nomspr` on a screen `screen_width`
/// pixels wide: `Datas/menu/spr_<width>/32bits/<nomspr>.spr`
/// (@ 0x00451450 and 0x00417eb0). Backslashes of the name become slashes;
/// the files on disc end in `.SPR`, so compare without case.
pub fn menu_sprite_path(screen_width: i32, nomspr: &[u8]) -> String {
    let name: String = nomspr
        .iter()
        .map(|&b| if b == b'\\' { '/' } else { char::from(b) })
        .collect();
    format!("Datas/menu/spr_{screen_width}/32bits/{name}.spr")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer() {
        let mut c = TextCursor::new(b"  key (a,b) ;c\r\n\r\nnext line\n");
        let mut line = TextCursor::new(c.line());
        assert_eq!(line.token(), b"key");
        assert_eq!(line.token(), b"a");
        assert_eq!(line.token(), b"b");
        assert_eq!(line.token(), b"c");
        assert_eq!(line.token(), b"");
        assert_eq!(c.line(), b"next line");
        assert!(c.eof());
        let mut s = TextCursor::new(b"k = 5");
        s.token();
        s.skip_token();
        assert_eq!(s.token(), b"5");
        let mut r = TextCursor::new(b"\n  a\r\n\r\nb\n");
        assert_eq!(r.raw_line(), b"  a");
        assert_eq!(r.raw_line(), b"");
        assert_eq!(r.raw_line(), b"b");
    }

    #[test]
    fn numbers() {
        assert_eq!(atoi(b" -12x"), -12);
        assert_eq!(atoi(b"+7"), 7);
        assert_eq!(atoi(b""), 0);
        assert_eq!(atoi(b"4294967297"), 1);
        assert_eq!(atof(b"0.33"), 0.33);
        assert_eq!(atof(b"15"), 15.0);
        assert_eq!(atof(b"2.e1x"), 20.0);
        assert_eq!(atof(b"-.5"), -0.5);
        assert_eq!(atof(b"abc"), 0.0);
        assert_eq!(ftol(-2.9), -2);
        assert_eq!(ftol(f64::NAN), i32::MIN);
    }

    const ATL: &[u8] = b"#SOUND_WHEN_OPENED 12\r\n\
#ELEMENT\r\n{\r\n\tnomref A\r\n\ttype IMG\r\n\tnomspr  bank_a\r\n\tpos 3 4\r\n\
\tbulle off\r\n}\r\n\r\n\
#ELEMENT\r\n{\r\n\tnomref B\r\n\ttype BT_ANIME\r\n\tnomspr save/x\r\n\tfps 0.5\r\n\
\tloop on\r\n\tsoundOnClickUp 500\r\n\tlink A\r\n\ttext on\r\n\t{\r\n\
\t\talign left bottom\r\n\t\tcolor 1 2 3\r\n\t\tfont F\r\n\t\ttextpos 5 6 7 8\r\n\t}\r\n\
\tbogus 1\r\n}\r\n\
#ELEMENT\r\n{\r\n\tnomref C\r\n\ttype CLICK_TEXT\r\n}";

    #[test]
    fn layout() {
        let l = Layout::parse(ATL, (800, 600));
        assert_eq!(l.sound_when_opened.as_deref(), Some(&b"12"[..]));
        assert_eq!(l.elements.len(), 3);
        let a = &l.elements[0];
        assert_eq!(a.element_type(), ElementType::Img);
        assert!(!a.selectable && !a.clickable && a.displayed && !a.bulle);
        assert_eq!(a.pos, [3, 4, 0, 0]);
        assert_eq!(a.frame_time, DEFAULT_FRAME_TIME);
        assert_eq!(a.text_pos, [1, 1, 800, 600]);
        let b = &l.elements[1];
        assert_eq!(b.element_type(), ElementType::BtAnime);
        assert!(b.selectable && b.clickable && b.looping && b.text);
        assert!(!b.has_pos());
        assert_eq!(b.frame_time, 2.0);
        assert_eq!(b.align_h, ALIGN_H_LEFT);
        assert_eq!(b.align_v, ALIGN_V_BOTTOM);
        assert_eq!(b.color, [1, 2, 3]);
        assert_eq!(b.font, b"F");
        assert_eq!(b.text_pos, [5, 6, 7, 8]);
        assert_eq!(b.link.as_deref(), Some(&b"A"[..]));
        assert_eq!(b.sound_on_click_up.as_deref(), Some(&b"500"[..]));
        assert_eq!(
            menu_sprite_path(800, b.nomspr.as_ref().unwrap()),
            "Datas/menu/spr_800/32bits/save/x.spr"
        );
        // Not reset between elements.
        let c = &l.elements[2];
        assert_eq!(c.color, [1, 2, 3]);
        assert_eq!(c.align_h, ALIGN_H_LEFT);
        assert_eq!(c.font, DEFAULT_FONT);
        assert_eq!(l.errors, vec![LayoutError::UnknownKey(b"bogus".to_vec())]);
    }

    #[test]
    fn tooltips() {
        let t = parse_tooltips(
            b"#ELEMENT A\r\n{\r\n\tbulle Two words \r\n}\r\n\
#ELEMENT C\r\n{\r\ntexte \r\n{\r\nline one\r\n\r\n  line three\r\n}\r\n}\r\n\
#ELEMENT Z\r\n{\r\nbulle x\r\n}\r\n",
        );
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].texts, vec![TooltipText::Bulle(b"Two words ".to_vec())]);
        assert_eq!(
            t[1].texts,
            vec![TooltipText::Texte(b"line one\n\n  line three".to_vec())]
        );
        let mut l = Layout::parse(ATL, (640, 480));
        let missing = l.apply_tooltips(&t);
        assert_eq!(missing, vec![b"Z".to_vec()]);
        assert_eq!(
            l.element(b"A").unwrap().tooltip.as_deref(),
            Some(&b"Two words "[..])
        );
        assert!(l.element(b"C").unwrap().page_text.is_some());
    }

    #[test]
    fn names() {
        assert_eq!(langue_layout(b"CD", b"fra"), b"i_langue_fra");
        assert_eq!(langue_layout(b"DVD", b"fra"), b"i_langue_dvd");
        assert_eq!(layout_path("i_princ"), "Datas/menu/desc/i_princ.atl");
        assert_eq!(
            tooltip_path("fra", "i_princ"),
            "Datas/loc/text/fra/i_princ.txt"
        );
    }
}
