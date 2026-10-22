//! The dialog scripts of a world: `scenar\info<world>.txt` (the speakers),
//! `scenar\com<world>.txt` (each line's speaker and options) and
//! `loc\Dial<world>.txt` (each line's text), loaded together by
//! `dlg_Dialog::LoadScript` (@ 0x0045dd60); and `loc\Font.txt`, the fonts
//! of the texts (@ 0x00404140).
//!
//! The tokenizer here splits on whitespace and drops `//` comments; every
//! number goes through `atof` and is stored as a float, as the original's
//! @ 0x0046c4a0 does (the original's own tokenizer, @ 0x0046d290, is not
//! transcribed: it reads the shipped files the same way).

/// A speaker block `[Perso]` .. `[FinPerso]` of `info<world>.txt`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Perso {
    /// The name `com<world>.txt` uses.
    pub name: String,
    /// The prefix of the head and body files: `3d\dial\<Mesh>s<stand>.db`.
    pub mesh: String,
    /// The speaker's place animations, hidden while it speaks.
    pub id_texts: Vec<String>,
    /// Output smoothing of the lip-sync.
    pub smooth: f32,
    /// Look-ahead of the lip-sync, seconds.
    pub offset: f32,
    /// `[Focale1..4]`: the panorama's field of view in degrees per stand,
    /// one value per camera.
    pub focales: [Vec<f32>; 4],
    /// `[Matrice]`: 10 rows (sound classes) x 7 columns (`d i a o b s v`),
    /// row-major; `None` for speakers without a head.
    pub matrix: Option<[f32; 70]>,
}

/// `atof` then a float store.
fn atof(s: &str) -> f32 {
    // `atof` reads the longest number prefix.
    let end = s
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+')))
        .count();
    s[..end].parse::<f64>().unwrap_or(0.0) as f32
}

fn text(data: &[u8]) -> String {
    // The files are Windows-1252; only ASCII matters here.
    data.iter().map(|&c| char::from(c)).collect()
}

/// Reads `info<world>.txt`.
pub fn parse_info(data: &[u8]) -> Vec<Perso> {
    let mut out = Vec::new();
    let mut cur: Option<Perso> = None;
    let mut matrix: Vec<f32> = Vec::new();
    let mut key = String::new();
    for raw in text(data).lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let rest;
        if let Some(stripped) = line.strip_prefix('[') {
            let (k, r) = stripped.split_once(']').unwrap_or((stripped, ""));
            key = k.to_string();
            rest = r.trim();
            match key.as_str() {
                "Perso" => {
                    cur = Some(Perso {
                        name: rest.to_string(),
                        ..Perso::default()
                    });
                    matrix.clear();
                    continue;
                }
                "FinPerso" => {
                    if let Some(mut p) = cur.take() {
                        if matrix.len() == 70 {
                            p.matrix = Some(matrix[..].try_into().unwrap());
                        }
                        out.push(p);
                    }
                    key.clear();
                    continue;
                }
                _ => {}
            }
        } else {
            rest = line;
        }
        let Some(p) = cur.as_mut() else { continue };
        match key.as_str() {
            "Mesh" => p.mesh = rest.to_string(),
            "IdText" => p.id_texts.push(rest.to_string()),
            "Smooth" => p.smooth = atof(rest),
            "Offset" => p.offset = atof(rest),
            "Matrice" => matrix.extend(rest.split_whitespace().map(atof)),
            k if k.starts_with("Focale") => {
                if let Some(n) = k[6..].parse::<usize>().ok().filter(|n| (1..=4).contains(n)) {
                    p.focales[n - 1] = rest.split(',').map(|v| atof(v.trim())).collect();
                }
            }
            _ => {}
        }
    }
    out
}

/// What kind of line (réplique + 0x10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    /// A speaking head over its body video.
    #[default]
    Head,
    /// `voixoff`
    VoiceOver,
    /// `voixoff0`
    VoiceOver0,
    /// `cinema`
    Cinema,
}

/// A line of `com<world>.txt`: `NNN_MM<tab>Speaker,option,...`
/// (@ 0x0045ebc0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComLine {
    /// `NNN_MM`: dialog NNN, line MM (also the voice's file name).
    pub id: String,
    pub speaker: String,
    /// `humeurN`, 1..5: the morph-key set; 1 when absent.
    pub mood: u32,
    /// `standN`, 1..8: the head and video files; 1 when absent.
    pub stand: u32,
    /// `cameraN`, 1-based; `None` when the game chooses.
    pub camera: Option<u32>,
    pub kind: LineKind,
    pub from: u32,
    pub to: u32,
}

/// Reads `com<world>.txt`.
pub fn parse_com(data: &[u8]) -> Vec<ComLine> {
    let mut out = Vec::new();
    for raw in text(data).lines() {
        let Some((id, rest)) = raw.split_once('\t') else {
            continue;
        };
        let mut toks = rest.trim().split(',').map(str::trim);
        let mut l = ComLine {
            id: id.trim().to_string(),
            speaker: toks.next().unwrap_or("").to_string(),
            mood: 1,
            stand: 1,
            ..ComLine::default()
        };
        for t in toks {
            let num = |p: &str| t.strip_prefix(p).and_then(|n| n.parse::<u32>().ok());
            if let Some(n) = num("humeur") {
                l.mood = n;
            } else if let Some(n) = num("stand") {
                l.stand = n;
            } else if let Some(n) = num("camera") {
                l.camera = Some(n);
            } else if let Some(n) = num("from") {
                l.from = n;
            } else if let Some(n) = num("to") {
                l.to = n;
            } else {
                match t {
                    "voixoff" => l.kind = LineKind::VoiceOver,
                    "voixoff0" => l.kind = LineKind::VoiceOver0,
                    "cinema" => l.kind = LineKind::Cinema,
                    _ => {}
                }
            }
        }
        out.push(l);
    }
    out
}

/// A line of `loc\Dial<world>.txt`: `NNN_MM<tab>Speaker<tab>Text`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialLine {
    /// `NNN_MM`, as in `com<world>.txt`.
    pub id: String,
    pub speaker: String,
    /// The subtitle, Windows-1252 bytes as the file has them: the
    /// subtitle code works on bytes (@ 0x004611e0).
    pub text: Vec<u8>,
}

impl DialLine {
    /// The dialog number `NNN` and the line number `MM` of the id.
    pub fn numbers(&self) -> Option<(u32, u32)> {
        let (a, b) = self.id.split_once('_')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }
}

/// Reads `loc\Dial<world>.txt`: one line per CR LF, three tab-separated
/// columns; the third, the text, runs to the end of the line.
pub fn parse_dial(data: &[u8]) -> Vec<DialLine> {
    data.split(|&b| b == b'\n')
        .filter_map(|raw| {
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            let mut cols = raw.splitn(3, |&b| b == b'\t');
            let id = cols.next()?;
            let speaker = cols.next()?;
            let text = cols.next()?;
            Some(DialLine {
                id: text_of(id).trim().to_string(),
                speaker: text_of(speaker).trim().to_string(),
                text: text.to_vec(),
            })
        })
        .collect()
}

fn text_of(b: &[u8]) -> String {
    text(b)
}

/// The dialog files and the hero of a world (`MONDE`), as the world change
/// loads them (@ 0x00484df0 → @ 0x0047fe50): 1 Ireland, 2 and 6 Yucatán,
/// 3 China, 4 Shamballa, 5 Tibet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct World {
    /// The two letters of the file names (`TI`).
    pub code: &'static str,
    /// The world index the hero table is read with (@ 0x0047fe50: the
    /// last argument minus 1).
    pub index: usize,
}

/// The hero's speaker name per world index (the table @ 0x005df978, 5
/// entries of 5 dwords: the name, then four floats no dialog code reads).
pub const HEROES: [&str; 5] = ["H1", "Heros", "Heros", "Heros", "Heros"];

impl World {
    /// The world of `MONDE` = `monde`; `None` outside 1..6.
    pub fn of_monde(monde: i32) -> Option<World> {
        let (code, index) = match monde {
            1 => ("IR", 0),
            2 | 6 => ("YU", 1),
            3 => ("CH", 2),
            4 => ("SH", 3),
            5 => ("TI", 4),
            _ => return None,
        };
        Some(World { code, index })
    }

    /// `dial<XX>.txt` (in `loc\`), `com<XX>.txt` and `info<XX>.txt` (in
    /// `scenar\`).
    pub fn files(self) -> [String; 3] {
        let c = self.code;
        [
            format!("dial{c}.txt"),
            format!("com{c}.txt"),
            format!("info{c}.txt"),
        ]
    }

    /// The world's hero, whose lines turn the view around (@ 0x004801d0).
    pub fn hero(self) -> &'static str {
        HEROES[self.index]
    }
}

/// A `[Font]` block of `loc\Font.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontEntry {
    /// `NameID`: the name the code asks for (`AtlFT_Dialog`).
    pub id: String,
    /// `Name`: the TrueType file in `loc\`.
    pub file: String,
    /// `Type`: the face name.
    pub face: String,
    /// `Height`, in points: `CreateFontA` gets −MulDiv(height, DPI, 72)
    /// (@ 0x00404c70), and the subtitles step their lines by this value in
    /// pixels (@ 0x00404c50 returns it).
    pub height: i32,
    pub width: i32,
}

/// `loc\Font.txt`, as `AtlFont_MainInit` (@ 0x00404140) reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontTxt {
    /// `[PartitionCode]`: the line-break characters, read in hex (the
    /// stream is switched to `ios::hex`), as 16-bit codes (a single byte,
    /// or a lead and a trail byte with the high bit set) into the table @
    /// 0x006167bc.
    pub breaks: Vec<u16>,
    pub fonts: Vec<FontEntry>,
}

impl FontTxt {
    pub fn parse(data: &[u8]) -> FontTxt {
        let mut out = FontTxt::default();
        let mut cur: Option<FontEntry> = None;
        for raw in text(data).lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            let mut toks = line.split_whitespace();
            let key = toks.next().unwrap_or("");
            let rest: Vec<&str> = toks.collect();
            match key {
                "[PartitionCode]" => {
                    for t in rest {
                        let t = t.trim_start_matches("0x").trim_start_matches("0X");
                        if let Ok(v) = u16::from_str_radix(t, 16) {
                            out.breaks.push(v);
                        }
                    }
                }
                "[Font]" => cur = Some(FontEntry::default()),
                "[EndFont]" => out.fonts.extend(cur.take()),
                _ => {
                    let Some(f) = cur.as_mut() else { continue };
                    let value = rest.join(" ");
                    match key {
                        "NameID" => f.id = value,
                        "Name" => f.file = value,
                        "Type" => f.face = value,
                        "Height" => f.height = value.parse().unwrap_or(0),
                        "Width" => f.width = value.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
        }
        out
    }

    /// The font named `id`, or `AtlFT_Default` when there is none (the
    /// text begin @ 0x00404d70 falls back to it).
    pub fn font(&self, id: &str) -> Option<&FontEntry> {
        self.fonts
            .iter()
            .find(|f| f.id == id)
            .or_else(|| self.fonts.iter().find(|f| f.id == "AtlFT_Default"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dial_lines_worlds_and_fonts() {
        let d = parse_dial(b"001_01\tA\tOne two.\r\n001_02\tB\tT\xe9\tx\r\nbad\r\n");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].text, b"One two.");
        assert_eq!(d[1].text, b"T\xe9\tx");
        assert_eq!(d[1].numbers(), Some((1, 2)));
        let ti = World::of_monde(5).unwrap();
        assert_eq!(ti.files()[0], "dialTI.txt");
        assert_eq!(ti.hero(), "Heros");
        assert_eq!(World::of_monde(1).unwrap().hero(), "H1");
        assert_eq!(World::of_monde(6), World::of_monde(2));
        assert!(World::of_monde(7).is_none());
        let f = FontTxt::parse(
            b"; c\r\n[PartitionCode]\t0x20\r\n[Font]\r\nNameID\tAtlFT_Dialog\r\nName\tA.ttf\r\nType\tGill Alt One MT\r\nHeight\t12\r\nWidth\t0\r\n[EndFont]\r\n",
        );
        assert_eq!(f.breaks, [0x20]);
        let d = f.font("AtlFT_Dialog").unwrap();
        assert_eq!((d.face.as_str(), d.height), ("Gill Alt One MT", 12));
        assert!(f.font("AtlFT_Popup").is_none());
    }

    #[test]
    fn blocks_and_lines() {
        let mut info = String::from("// comment\r\n[Perso]\t\tSomeone  \r\n[Mesh]\tSo\r\n");
        info += "[IdText]\tA\r\n[IdText]\tB\r\n[Smooth]\t0.25\r\n[Offset]\t0.5\r\n";
        info += "[Matrice]\t1 0 0 0 0 0 0\t// first\r\n";
        for _ in 0..9 {
            info += "\t\t0.5 0 0 0 0 0 0.5\r\n";
        }
        info += "[Focale2]\t30.0,45.5\r\n[FinPerso]\r\n[Perso] NoHead\r\n[FinPerso]\r\n";
        let p = parse_info(info.as_bytes());
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].name.as_str(), p[0].mesh.as_str()), ("Someone", "So"));
        assert_eq!(p[0].id_texts, ["A", "B"]);
        assert_eq!((p[0].smooth, p[0].offset), (0.25, 0.5));
        assert_eq!(p[0].focales[1], [30.0, 45.5]);
        let m = p[0].matrix.unwrap();
        assert_eq!((m[0], m[7], m[13], m[69]), (1.0, 0.5, 0.5, 0.5));
        assert!(p[1].matrix.is_none());
        let c =
            parse_com(b"001_01\tSomeone,humeur3,stand2,camera2\r\n001_02\tX,cinema,from4,to9\r\n");
        assert_eq!((c[0].mood, c[0].stand, c[0].camera), (3, 2, Some(2)));
        assert_eq!((c[1].kind, c[1].from, c[1].to), (LineKind::Cinema, 4, 9));
    }
}
