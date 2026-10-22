//! `loc\Menu.txt`: the texts of the menus (`AtlMenu_MainInit`
//! @ 0x00485c00).
//!
//! `;` lines are comments; each section `[Name]` collects the lines after
//! it, one string per line, up to `[EndMenu]`. A section with fewer lines
//! than the pages need at the least gets a warning and is padded with
//! copies of its last line. The texts are Windows-1252 bytes.

/// The sections, in the order the reader pads them, and their minimum
/// line counts (the pages use more lines than the minimum of the options
/// and load sections: the shipped file has 11, 7, 10, 6, 5, 4 and 4).
pub const SECTIONS: [(&str, usize); 7] = [
    ("MainMenu", 11),
    ("PlayerMenu", 7),
    ("OptionsMenu", 8),
    ("SoundMenu", 6),
    ("LoadMenu", 3),
    ("CDMenu", 4),
    ("Messages", 4),
];

/// A section of `Menu.txt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Main = 0,
    Player = 1,
    Options = 2,
    Sound = 3,
    Load = 4,
    Cd = 5,
    Messages = 6,
}

/// The texts of `Menu.txt`, per section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuTxt {
    pub sections: [Vec<Vec<u8>>; 7],
    /// The sections the reader had to pad (each logs a warning).
    pub padded: Vec<&'static str>,
}

impl MenuTxt {
    pub fn parse(data: &[u8]) -> MenuTxt {
        let mut out = MenuTxt::default();
        let mut cur: Option<usize> = None;
        for raw in data.split(|&b| b == b'\n') {
            let line = raw.strip_suffix(b"\r").unwrap_or(raw);
            let trimmed = line.trim_ascii();
            if trimmed.is_empty() || trimmed.starts_with(b";") {
                continue;
            }
            if trimmed.eq_ignore_ascii_case(b"[EndMenu]") {
                cur = None;
                continue;
            }
            if trimmed.starts_with(b"[") && trimmed.ends_with(b"]") {
                let name = &trimmed[1..trimmed.len() - 1];
                cur = SECTIONS
                    .iter()
                    .position(|(n, _)| n.as_bytes().eq_ignore_ascii_case(name));
                continue;
            }
            if let Some(s) = cur {
                out.sections[s].push(trimmed.to_vec());
            }
        }
        for (i, (name, min)) in SECTIONS.iter().enumerate() {
            let lines = &mut out.sections[i];
            if lines.len() < *min {
                log::warn!("Menu.txt: [{name}] has {} lines, {min} wanted", lines.len());
                out.padded.push(name);
                let last = lines.last().cloned().unwrap_or_default();
                lines.resize(*min, last);
            }
        }
        out
    }

    /// Line `i` of a section (empty past its end: a short file would make
    /// the original read past its vector).
    pub fn line(&self, s: Section, i: usize) -> &[u8] {
        self.sections[s as usize]
            .get(i)
            .map_or(&[][..], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_and_padding() {
        let t = MenuTxt::parse(
            b"; comment\r\n[MainMenu]\r\nTitle\r\nQuit\r\n[EndMenu]\r\n\r\n[CDMenu]\r\nA\r\nB\r\nC\r\nD\r\n[EndMenu]\r\n",
        );
        assert_eq!(t.line(Section::Main, 1), b"Quit");
        assert_eq!(t.line(Section::Main, 10), b"Quit");
        assert_eq!(t.line(Section::Cd, 3), b"D");
        assert_eq!(t.line(Section::Load, 2), b"");
        assert!(t.padded.contains(&"MainMenu") && !t.padded.contains(&"CDMenu"));
    }
}
