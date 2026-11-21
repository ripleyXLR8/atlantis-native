//! *Atlantis V*'s three text tables (`datas/texts/<table>/<language>/*.xml`,
//! the paths of [`crate::atlantis5::text_paths`]): UTF-8 XML in the default
//! namespace `XMLSchema-Atlantis`, which the program reads through MSXML 4
//! and queries by XPath with the prefix `atl:` (`SelectionNamespaces`
//! `xmlns:atl='XMLSchema-Atlantis'`, 0x0066f840). Addresses are
//! atlantis5-dvd-int-3891200's.
//!
//! - `string_resources.xml` (menus, object descriptions): root
//!   `STRING_RESOURCE_SET` (checked, 0x0066fb60), `atl:STRING_RESOURCE`
//!   elements (0x0066fac4) with an `id` attribute and the string as text;
//! - `dialogs.xml`: root `dialog_set`; `atl:talker_set` (0x0066f7f8) of
//!   `atl:talker` (0x0066f880) `name` + `picturing`, then `atl:dialog`
//!   (0x0066f774) `id`, each with its own `atl:talker` elements (read by
//!   the same reader @ 0x00466cb0) and `atl:tirade` (0x0066f898) `talker`
//!   with the line as text;
//! - `kinematic_subtitles.xml` (films): root `KINEMATIC_SUBTITLES_SET`
//!   (checked, 0x0066fed4), `atl:KINEMATIC_SUBTITLES` `name` (0x0066fe28),
//!   `atl:SUBTITLE_TEXT` `start`, `end` (0x0066fe04, the error strings
//!   0x0066fbfc..0x0066fcf8) with the text. The unit of `start` and `end`
//!   is the subtitle player's [UNKNOWN]: they are kept as read.
//!
//! Element names are matched without the namespace (the files declare it as
//! the default one, so every element is in it).

use atlantis4_data::xml::{parse_utf8, Element, XmlError};

/// Why a table cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    Xml(XmlError),
    /// The root is not the table's (the program's check).
    Root(String),
    /// An element lacks an attribute the program requires.
    Missing(&'static str),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Xml(e) => write!(f, "{e}"),
            TextError::Root(r) => write!(f, "unexpected root element <{r}>"),
            TextError::Missing(what) => write!(f, "missing {what}"),
        }
    }
}

impl std::error::Error for TextError {}

/// Without the prefix (`atl:x` → `x`).
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, n)| n)
}

fn root(data: &[u8], name: &str) -> Result<Element, TextError> {
    let els = parse_utf8(data).map_err(TextError::Xml)?;
    let r = els
        .into_iter()
        .next()
        .ok_or_else(|| TextError::Root(String::new()))?;
    if local(&r.name) != name {
        return Err(TextError::Root(r.name));
    }
    Ok(r)
}

fn children<'a>(e: &'a Element, name: &'a str) -> impl Iterator<Item = &'a Element> {
    e.children.iter().filter(move |c| local(&c.name) == name)
}

/// The language the file says it is (`language` of the root).
fn language(r: &Element) -> String {
    r.attr("language").unwrap_or_default().to_string()
}

/// `string_resources.xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringResources {
    pub language: String,
    /// `id`, text, in the file's order.
    pub strings: Vec<(String, String)>,
}

impl StringResources {
    pub fn parse(data: &[u8]) -> Result<StringResources, TextError> {
        let r = root(data, "STRING_RESOURCE_SET")?;
        let mut strings = Vec::new();
        for e in children(&r, "STRING_RESOURCE") {
            let id = e
                .attr("id")
                .ok_or(TextError::Missing("STRING_RESOURCE id"))?;
            strings.push((id.to_string(), e.text.clone()));
        }
        Ok(StringResources {
            language: language(&r),
            strings,
        })
    }

    /// The string `id` (the first of that id).
    pub fn get(&self, id: &str) -> Option<&str> {
        self.strings
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.as_str())
    }
}

/// How a talker is shown (@ 0x00466cb0: `picturing` compared with
/// `pict_3d` (0x0066f92c) → 0, `pict_2d` (0x0066f91c) → 1, anything else
/// or no attribute → 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picturing {
    /// 0: `pict_3d`.
    ThreeD,
    /// 1: `pict_2d`.
    TwoD,
    /// 2: `pict_none` and any other value.
    None,
}

impl Picturing {
    pub fn of(value: Option<&str>) -> Picturing {
        match value {
            Some("pict_3d") => Picturing::ThreeD,
            Some("pict_2d") => Picturing::TwoD,
            _ => Picturing::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Talker {
    pub name: String,
    pub picturing: Picturing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tirade {
    pub talker: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub id: u32,
    /// The dialog's own `talker` elements (before its tirades).
    pub talkers: Vec<Talker>,
    pub tirades: Vec<Tirade>,
}

/// `dialogs.xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dialogs {
    pub language: String,
    pub talkers: Vec<Talker>,
    pub dialogs: Vec<Dialog>,
}

fn talker(e: &Element) -> Result<Talker, TextError> {
    Ok(Talker {
        name: e
            .attr("name")
            .ok_or(TextError::Missing("talker name"))?
            .to_string(),
        picturing: Picturing::of(e.attr("picturing")),
    })
}

impl Dialogs {
    pub fn parse(data: &[u8]) -> Result<Dialogs, TextError> {
        let r = root(data, "dialog_set")?;
        let mut d = Dialogs {
            language: language(&r),
            ..Dialogs::default()
        };
        for set in children(&r, "talker_set") {
            for t in children(set, "talker") {
                d.talkers.push(talker(t)?);
            }
        }
        for e in children(&r, "dialog") {
            let id = e
                .attr("id")
                .and_then(|v| v.trim().parse().ok())
                .ok_or(TextError::Missing("dialog id"))?;
            let talkers = children(e, "talker")
                .map(talker)
                .collect::<Result<_, _>>()?;
            let tirades = children(e, "tirade")
                .map(|t| {
                    Ok(Tirade {
                        talker: t
                            .attr("talker")
                            .ok_or(TextError::Missing("tirade talker"))?
                            .to_string(),
                        text: t.text.clone(),
                    })
                })
                .collect::<Result<_, TextError>>()?;
            d.dialogs.push(Dialog {
                id,
                talkers,
                tirades,
            });
        }
        Ok(d)
    }

    pub fn dialog(&self, id: u32) -> Option<&Dialog> {
        self.dialogs.iter().find(|d| d.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subtitle {
    /// `start` and `end` as written [UNKNOWN: their unit].
    pub start: i32,
    pub end: i32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilmSubtitles {
    /// The film (`videos/<name>.bik`).
    pub name: String,
    pub subtitles: Vec<Subtitle>,
}

/// `kinematic_subtitles.xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KinematicSubtitles {
    pub language: String,
    pub films: Vec<FilmSubtitles>,
}

impl KinematicSubtitles {
    pub fn parse(data: &[u8]) -> Result<KinematicSubtitles, TextError> {
        let r = root(data, "KINEMATIC_SUBTITLES_SET")?;
        let mut k = KinematicSubtitles {
            language: language(&r),
            ..KinematicSubtitles::default()
        };
        for f in children(&r, "KINEMATIC_SUBTITLES") {
            let name = f
                .attr("name")
                .ok_or(TextError::Missing("KINEMATIC_SUBTITLES name"))?;
            let num = |e: &Element, a: &'static str| -> Result<i32, TextError> {
                e.attr(a)
                    .and_then(|v| v.trim().parse().ok())
                    .ok_or(TextError::Missing(a))
            };
            let subtitles = children(f, "SUBTITLE_TEXT")
                .map(|s| {
                    Ok(Subtitle {
                        start: num(s, "start")?,
                        end: num(s, "end")?,
                        text: s.text.clone(),
                    })
                })
                .collect::<Result<_, TextError>>()?;
            k.films.push(FilmSubtitles {
                name: name.to_string(),
                subtitles,
            });
        }
        Ok(k)
    }

    /// The film `name`'s subtitles (any case).
    pub fn film(&self, name: &str) -> Option<&FilmSubtitles> {
        self.films
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = "xmlns=\"XMLSchema-Atlantis\" language=\"french\" version=\"1.5\"";

    #[test]
    fn strings() {
        let x = format!(
            "<?xml version=\"1.0\"?>\r\n<STRING_RESOURCE_SET {NS}>\r\n\t<STRING_RESOURCE id=\"a\">Un\r\ndeux</STRING_RESOURCE>\r\n</STRING_RESOURCE_SET>"
        );
        let s = StringResources::parse(x.as_bytes()).unwrap();
        assert_eq!(s.language, "french");
        assert_eq!(s.get("a"), Some("Un\ndeux"));
        assert!(matches!(
            StringResources::parse(b"<dialog_set/>"),
            Err(TextError::Root(_))
        ));
    }

    #[test]
    fn dialogs_and_talkers() {
        let x = format!(
            "<dialog_set {NS}><talker_set><talker name=\"h\" picturing=\"pict_2d\"/><talker name=\"o\" picturing=\"pict_none\"/><talker name=\"g\" picturing=\"pict_3d\"/></talker_set>\
             <dialog id=\"7\"><talker name=\"g\" picturing=\"pict_2d\"/><tirade talker=\"g\">Ça</tirade><tirade talker=\"h\">Oui</tirade></dialog></dialog_set>"
        );
        let d = Dialogs::parse(x.as_bytes()).unwrap();
        assert_eq!(
            d.talkers.iter().map(|t| t.picturing).collect::<Vec<_>>(),
            [Picturing::TwoD, Picturing::None, Picturing::ThreeD]
        );
        let g = d.dialog(7).unwrap();
        assert_eq!(g.talkers[0].picturing, Picturing::TwoD);
        assert_eq!(g.tirades[0].text, "Ça");
        assert_eq!(g.tirades[1].talker, "h");
    }

    #[test]
    fn subtitles() {
        let x = format!(
            "<KINEMATIC_SUBTITLES_SET {NS}><KINEMATIC_SUBTITLES name=\"cine01\"><SUBTITLE_TEXT start=\"2\" end=\"20\">L’Atlantide…</SUBTITLE_TEXT></KINEMATIC_SUBTITLES></KINEMATIC_SUBTITLES_SET>"
        );
        let k = KinematicSubtitles::parse(x.as_bytes()).unwrap();
        let f = k.film("CINE01").unwrap();
        assert_eq!((f.subtitles[0].start, f.subtitles[0].end), (2, 20));
        assert_eq!(f.subtitles[0].text, "L’Atlantide…");
    }
}
