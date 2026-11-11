//! The text tables: every spoken or narrated line, the inventory's and the
//! documents' names, the inventory combinations and the diary's layout.
//!
//! All are XML read by [`crate::xml`] (rootless except `Combinations.xml`,
//! Windows-1252):
//!
//! | File | Element | Attributes | Loader evidence |
//! |---|---|---|---|
//! | `Texts/<lang>.xml` | `dialog` | `name` (`<group>-<line>`), `avatar` (speaker), `text` | `Texts/` file +0x1b8ac4, `.xml` +0x1b8abc, `[LocFile::Load]` +0x1b7828, names +0x1b7878..+0x1b78bc |
//! | `Inventory/Objects/<lang>/Objects.xml` | `Object` | `id`, `name`, `description` | `Inventory/Objects/Objects.xml` +0x1b53dc |
//! | `DocumentsBrowser/Documents/HD/<lang>/Documents.xml` | `Document` | `id`, `name`, `description` | `DocumentsBrowser/Documents/Documents.xml` +0x1b4140 |
//! | `Inventory/Objects/Combinations.xml` | `Combinations` > `Combination` > `In`, `Out` | `comment`, `id` (6); `objectName`, `transformedIn` (79); `objectName` | +0x1b54e4, names +0x1b5548..+0x1b55a0 |
//! | `Diary/Diary.xml` | `sprite_button2` > `position`, `value` | | `Diary/` strings +0x1b7f18..+0x1b7fd4 |
//!
//! The executable names the object and document tables without the
//! language and `HD` folders the install has; how it inserts them (the
//! version "definitions" search) is not read yet, so [`objects_path`] and
//! [`documents_path`] give the install's layout.

use crate::xml::{self, Element, XmlError};

/// The languages of the GOG.com install, each with its own texts, voices,
/// object and document names.
pub const LANGUAGES: [&str; 5] = ["de", "en", "es", "fr", "it"];

/// `Texts/<lang>.xml`.
pub fn texts_path(lang: &str) -> String {
    format!("Texts/{lang}.xml")
}

/// `Inventory/Objects/<lang>/Objects.xml`.
pub fn objects_path(lang: &str) -> String {
    format!("Inventory/Objects/{lang}/Objects.xml")
}

/// `DocumentsBrowser/Documents/HD/<lang>/Documents.xml`.
pub fn documents_path(lang: &str) -> String {
    format!("DocumentsBrowser/Documents/HD/{lang}/Documents.xml")
}

/// The combinations (one file for every language).
pub const COMBINATIONS_PATH: &str = "Inventory/Objects/Combinations.xml";

/// The diary's layout.
pub const DIARY_PATH: &str = "Diary/Diary.xml";

/// A line of `Texts/<lang>.xml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    /// `<group>-<line>`, the id the scripts give `DrawText`.
    pub name: String,
    /// The speaker.
    pub avatar: String,
    pub text: String,
}

/// The lines of one language, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogTable {
    pub lines: Vec<Dialog>,
}

/// An entry of `Objects.xml` or `Documents.xml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labelled {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// An inventory combination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combination {
    /// `id` (6 of the 125 combinations have one).
    pub id: Option<String>,
    pub comment: String,
    /// `objectName`, `transformedIn` (absent: `None`).
    pub inputs: Vec<(String, Option<String>)>,
    pub outputs: Vec<String>,
}

/// Why a table cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    Xml(XmlError),
    /// An element of another name, or an attribute missing.
    Element(String),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Xml(e) => e.fmt(f),
            TextError::Element(n) => write!(f, "unexpected or incomplete element {n}"),
        }
    }
}

impl std::error::Error for TextError {}

fn attr(el: &Element, a: &str) -> Result<String, TextError> {
    el.attr(a)
        .map(str::to_string)
        .ok_or_else(|| TextError::Element(format!("{} (no {a})", el.name)))
}

fn elements(data: &[u8], name: &str) -> Result<Vec<Element>, TextError> {
    let els = xml::parse(data).map_err(TextError::Xml)?;
    if let Some(other) = els.iter().find(|e| e.name != name) {
        return Err(TextError::Element(other.name.clone()));
    }
    Ok(els)
}

impl DialogTable {
    pub fn parse(data: &[u8]) -> Result<DialogTable, TextError> {
        let lines = elements(data, "dialog")?
            .iter()
            .map(|el| {
                Ok(Dialog {
                    name: attr(el, "name")?,
                    avatar: attr(el, "avatar")?,
                    text: attr(el, "text")?,
                })
            })
            .collect::<Result<_, TextError>>()?;
        Ok(DialogTable { lines })
    }

    /// The line named `name`, the first if the name is repeated (the
    /// English table repeats one; which one the original keeps is not read
    /// yet).
    pub fn get(&self, name: &str) -> Option<&Dialog> {
        self.lines.iter().find(|d| d.name == name)
    }
}

/// The entries of an `Objects.xml` (`element` = `Object`) or a
/// `Documents.xml` (`Document`).
pub fn parse_labelled(data: &[u8], element: &str) -> Result<Vec<Labelled>, TextError> {
    elements(data, element)?
        .iter()
        .map(|el| {
            Ok(Labelled {
                id: attr(el, "id")?,
                name: attr(el, "name")?,
                description: attr(el, "description")?,
            })
        })
        .collect()
}

/// `Combinations.xml`.
pub fn parse_combinations(data: &[u8]) -> Result<Vec<Combination>, TextError> {
    let root = elements(data, "Combinations")?;
    let root = root
        .first()
        .ok_or_else(|| TextError::Element("Combinations".into()))?;
    root.children
        .iter()
        .map(|c| {
            if c.name != "Combination" {
                return Err(TextError::Element(c.name.clone()));
            }
            let mut inputs = Vec::new();
            let mut outputs = Vec::new();
            for io in &c.children {
                match io.name.as_str() {
                    "In" => inputs.push((
                        attr(io, "objectName")?,
                        io.attr("transformedIn").map(str::to_string),
                    )),
                    "Out" => outputs.push(attr(io, "objectName")?),
                    _ => return Err(TextError::Element(io.name.clone())),
                }
            }
            Ok(Combination {
                id: c.attr("id").map(str::to_string),
                comment: c.attr("comment").unwrap_or_default().to_string(),
                inputs,
                outputs,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_table_reads_its_lines() {
        let t = DialogTable::parse(
            b"<dialog name=\"1-1\" avatar=\"N\" text=\"&quot;Hi&quot;\"/>\r\n<dialog name=\"1-2\" avatar=\"N\" text=\"\xe9\"/>\r\n",
        )
        .unwrap();
        assert_eq!(t.lines.len(), 2);
        assert_eq!(t.get("1-1").unwrap().text, "\"Hi\"");
        assert_eq!(t.get("1-2").unwrap().text, "\u{e9}");
        assert!(DialogTable::parse(b"<other/>").is_err());
    }

    #[test]
    fn combinations_read_their_inputs_and_outputs() {
        let c = parse_combinations(
            b"<Combinations>\r\n\t<Combination comment=\"c\">\r\n\t\t<In objectName=\"A\"/>\r\n\t\t<In objectName=\"B\" transformedIn=\"C\"/>\r\n\t\t<Out objectName=\"D\"/>\r\n\t</Combination>\r\n</Combinations>\r\n",
        )
        .unwrap();
        assert_eq!(c[0].inputs[1], ("B".into(), Some("C".into())));
        assert_eq!(c[0].outputs, ["D"]);
    }
}
