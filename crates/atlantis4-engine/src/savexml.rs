//! The save's XML as the original's TinyXML prints it, and the whole
//! `save.xml` text of the `SaveGame` worker (atlantis4-gog-en-2363910
//! @ 0x00524100).
//!
//! Printing (`TiXmlElement::Print`; its strings `"    "`, `"<%s"`, `" "`,
//! `" />"`, `">"`, `"</%s>"`, `"\n"` @ 0x005b4968..0x005b49a0 are those
//! of TinyXML 2.5's `Print`, `DATA`; the order is TinyXML's,
//! `[INFERRED]`): an element at depth `d` is indented by `d` × 4 spaces;
//! one without children ends with `" />"`; one with element children
//! puts each on a new line one level deeper and closes on a new line at
//! its own indentation. The document prints each top-level node then
//! `"\n"`. Attribute values are escaped as `TeLuaContext::save`'s
//! (`TeLuaContext::save` @ 0x004c1670): `& < > " '` as entities, bytes
//! under 0x20 as `&#xNN;`, a `&#x` kept as is; a value holding `"` is
//! quoted with `'`.
//!
//! Top-level order (0x00524100, `STATIC`): the declaration,
//! `GameVersion`, `Inventory`, `DocumentsBrowser`, `Dialog`, `Diary`,
//! `PuzzleTimer`, `WarpName`, `Orientation`, `Music`, `PlayedTime`, then
//! the globals (their order follows the collector and is not reproduced).

use atlantis4_script::{SavedGlobal, SavedValue};

/// An element with attributes and element children.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XmlNode {
    pub name: String,
    /// Attributes in the order set; values as bytes (cp1252 kept).
    pub attrs: Vec<(String, Vec<u8>)>,
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    pub fn new(name: &str) -> XmlNode {
        XmlNode {
            name: name.to_string(),
            ..XmlNode::default()
        }
    }

    /// With attribute `name` = `value` added.
    pub fn attr(mut self, name: &str, value: &str) -> XmlNode {
        self.attrs
            .push((name.to_string(), value.as_bytes().to_vec()));
        self
    }

    /// With attribute `name` = `value` (bytes) added.
    pub fn attr_bytes(mut self, name: &str, value: &[u8]) -> XmlNode {
        self.attrs.push((name.to_string(), value.to_vec()));
        self
    }

    /// The element printed at `depth` (no final new line), bytes.
    pub fn print_bytes(&self, depth: usize, out: &mut Vec<u8>) {
        out.extend(std::iter::repeat_n(b' ', 4 * depth));
        out.push(b'<');
        out.extend_from_slice(self.name.as_bytes());
        for (k, v) in &self.attrs {
            out.push(b' ');
            out.extend_from_slice(k.as_bytes());
            let q = if v.contains(&b'"') { b'\'' } else { b'"' };
            out.push(b'=');
            out.push(q);
            escape(v, out);
            out.push(q);
        }
        if self.children.is_empty() {
            out.extend_from_slice(b" />");
            return;
        }
        out.push(b'>');
        for c in &self.children {
            out.push(b'\n');
            c.print_bytes(depth + 1, out);
        }
        out.push(b'\n');
        out.extend(std::iter::repeat_n(b' ', 4 * depth));
        out.extend_from_slice(b"</");
        out.extend_from_slice(self.name.as_bytes());
        out.push(b'>');
    }

    /// The element printed at `depth`, as text (Latin-1 bytes as chars).
    pub fn print(&self, depth: usize) -> String {
        let mut out = Vec::new();
        self.print_bytes(depth, &mut out);
        out.iter().map(|&c| char::from(c)).collect()
    }
}

/// TinyXML's `PutString` as the save uses it.
fn escape(v: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < v.len() {
        let c = v[i];
        if c == b'&' && v[i..].starts_with(b"&#x") {
            // An entity already written: copied up to its ';'.
            while i < v.len() {
                out.push(v[i]);
                i += 1;
                if v[i - 1] == b';' {
                    break;
                }
            }
            continue;
        }
        match c {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'"' => out.extend_from_slice(b"&quot;"),
            b'\'' => out.extend_from_slice(b"&apos;"),
            c if c < 0x20 => out.extend_from_slice(format!("&#x{c:02X};").as_bytes()),
            c => out.push(c),
        }
        i += 1;
    }
}

/// The parts of a save the engine holds, beside the globals.
#[derive(Clone, Debug)]
pub struct SaveParts {
    pub inventory: XmlNode,
    pub documents: XmlNode,
    pub diary: XmlNode,
    pub puzzle_timer: XmlNode,
    /// The current node's path (`WarpName`).
    pub warp_name: Vec<u8>,
    /// Yaw and pitch, degrees (floats widened to double).
    pub orientation: (f32, f32),
    /// The music's path.
    pub music: Vec<u8>,
    /// Seconds played.
    pub played_time: f64,
}

/// The whole `save.xml` text (`"\n"` line ends; the file has `"\r\n"`,
/// text mode).
pub fn save_text(parts: &SaveParts, globals: &[SavedGlobal]) -> Vec<u8> {
    let mut nodes = vec![
        XmlNode::new("GameVersion")
            .attr("major", "1")
            .attr("minor", "0"),
        parts.inventory.clone(),
        parts.documents.clone(),
        XmlNode::new("Dialog"),
        parts.diary.clone(),
        parts.puzzle_timer.clone(),
        XmlNode::new("WarpName").attr_bytes("name", &parts.warp_name),
        XmlNode::new("Orientation")
            .attr("x", &format!("{:.6}", f64::from(parts.orientation.0)))
            .attr("y", &format!("{:.6}", f64::from(parts.orientation.1))),
        XmlNode::new("Music").attr_bytes("path", &parts.music),
        XmlNode::new("PlayedTime").attr("value", &format!("{:.6}", parts.played_time)),
    ];
    for g in globals {
        let kind = match g.value {
            SavedValue::Boolean(_) => "Boolean",
            SavedValue::Number(_) => "Number",
            SavedValue::String(_) => "String",
        };
        nodes.push(
            XmlNode::new(kind)
                .attr_bytes("name", &g.name)
                .attr_bytes("value", &g.value.text()),
        );
    }
    let mut out = b"<?xml version=\"1.0\" ?>\n".to_vec();
    for n in &nodes {
        n.print_bytes(0, &mut out);
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_elements_print_as_tinyxml() {
        let mut a = XmlNode::new("A");
        let mut b = XmlNode::new("B").attr("id", "x");
        b.children.push(XmlNode::new("C"));
        a.children.push(b);
        a.children.push(XmlNode::new("D"));
        assert_eq!(
            a.print(0),
            "<A>\n    <B id=\"x\">\n        <C />\n    </B>\n    <D />\n</A>"
        );
    }

    #[test]
    fn values_are_escaped_as_the_globals_are() {
        let n = XmlNode::new("S").attr("v", "a<b>&\t'&#x41;");
        assert_eq!(n.print(0), "<S v=\"a&lt;b&gt;&amp;&#x09;&apos;&#x41;\" />");
        let q = XmlNode::new("S").attr("v", "say \"hi\"");
        assert_eq!(q.print(0), "<S v='say &quot;hi&quot;' />");
    }

    #[test]
    fn the_save_puts_the_parts_in_the_workers_order() {
        let parts = SaveParts {
            inventory: XmlNode::new("Inventory"),
            documents: XmlNode::new("DocumentsBrowser"),
            diary: XmlNode::new("Diary"),
            puzzle_timer: XmlNode::new("PuzzleTimer"),
            warp_name: b"3D/Bateau_CA/0001.wp".to_vec(),
            orientation: (10.0, -5.5),
            music: b"Sounds/Musiques/muzik20.ogg".to_vec(),
            played_time: 1.5,
        };
        let g = [SavedGlobal {
            name: b"INTRO".to_vec(),
            value: SavedValue::Boolean(true),
        }];
        let t = String::from_utf8(save_text(&parts, &g)).unwrap();
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "<?xml version=\"1.0\" ?>");
        assert_eq!(lines[1], "<GameVersion major=\"1\" minor=\"0\" />");
        assert_eq!(lines[4], "<Dialog />");
        assert_eq!(lines[8], "<Orientation x=\"10.000000\" y=\"-5.500000\" />");
        assert_eq!(lines[10], "<PlayedTime value=\"1.500000\" />");
        assert_eq!(lines[11], "<Boolean name=\"INTRO\" value=\"1\" />");
    }
}
