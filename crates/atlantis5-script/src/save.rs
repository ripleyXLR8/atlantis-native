//! The XML save (`CSaveFileWriter` @ 0x0043ad90, `CSaveFileReader`
//! @ 0x0043b210): an `ATL_FILE` document (0x0066de4c) with the attribute
//! `type` = `atlantis_5_save` (0x0066de2c), its sections in the writer's
//! order: `LUA_DB` (@ 0x00438920), the configuration, `CAMERA_MOVER`
//! (@ 0x00437f40), `CAMERA` (@ 0x00437920), `SOUND` (@ 0x0043a4f0),
//! `PERSISTANT_SPRITES` (@ 0x00439f10), `CALLBACK_SHOOTER_MANAGER`
//! (@ 0x00436f20). The file is `saves/<name>` with no extension
//! (`Atl_Save` @ 0x00408fb0: the path table's entry 0x27 + `autoslot`).

use atlantis4_data::xml::Element;

/// The root element's name and its `type`.
pub const ROOT: &str = "ATL_FILE";
pub const TYPE: &str = "atlantis_5_save";

/// The autosave's name (0x0066a07c), which `Atl_Save` writes and the
/// load state (@ 0x0042f000 state 2) reads.
pub const AUTOSLOT: &str = "autoslot";

/// An empty element.
pub fn element(name: &str) -> Element {
    Element {
        name: name.to_string(),
        attributes: Vec::new(),
        children: Vec::new(),
        text: String::new(),
    }
}

/// An element with the attribute `v`.
pub fn element_v(name: &str, v: String) -> Element {
    let mut e = element(name);
    e.attributes.push(("v".into(), v));
    e
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
}

fn write(e: &Element, depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push('\t');
    }
    out.push('<');
    out.push_str(&e.name);
    for (k, v) in &e.attributes {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape(v, out);
        out.push('"');
    }
    if e.children.is_empty() && e.text.is_empty() {
        out.push_str("/>\r\n");
        return;
    }
    out.push('>');
    if !e.text.is_empty() {
        escape(&e.text, out);
    }
    if !e.children.is_empty() {
        out.push_str("\r\n");
        for c in &e.children {
            write(c, depth + 1, out);
        }
        for _ in 0..depth {
            out.push('\t');
        }
    }
    out.push_str("</");
    out.push_str(&e.name);
    out.push_str(">\r\n");
}

/// The document as UTF-8 text (MSXML's `save` of a document without a
/// processing instruction writes UTF-8; its exact spacing is not known,
/// so Native indents with tabs).
pub fn to_text(root: &Element) -> String {
    let mut out = String::new();
    write(root, 0, &mut out);
    out
}

/// The document's root when it is an `ATL_FILE` (the reader looks it up
/// by name, @ 0x00465110).
pub fn parse(data: &[u8]) -> Option<Element> {
    let roots = atlantis4_data::xml::parse_utf8(data).ok()?;
    roots.into_iter().find(|e| e.name == ROOT)
}

/// A float attribute as @ 0x00465390 writes one (a `VT_R4`).
pub fn attr_f32(e: &mut Element, name: &str, x: f32) {
    e.attributes
        .push((name.into(), crate::engine::number_text(f64::from(x))));
}

/// A boolean attribute (a `VT_BOOL`; MSXML's text of it is not read:
/// `True` / `False`, `VariantChangeType`'s).
pub fn attr_bool(e: &mut Element, name: &str, b: bool) {
    e.attributes
        .push((name.into(), if b { "True" } else { "False" }.into()));
}

/// An attribute's number (either decimal separator).
pub fn get_f64(e: &Element, name: &str) -> Option<f64> {
    e.attr(name)?.trim().replace(',', ".").parse().ok()
}

/// An attribute's boolean.
pub fn get_bool(e: &Element, name: &str) -> Option<bool> {
    let v = e.attr(name)?.trim().to_ascii_lowercase();
    Some(matches!(v.as_str(), "true" | "-1" | "1" | "vrai"))
}
