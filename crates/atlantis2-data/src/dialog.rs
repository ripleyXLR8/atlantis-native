//! The dialog scripts of a world: `scenar\info<world>.txt` (the speakers)
//! and `scenar\com<world>.txt` (each line's speaker and options), loaded
//! with `loc\Dial<world>.txt` by `dlg_Dialog::LoadScript` (@ 0x0045dd60).
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

#[cfg(test)]
mod tests {
    use super::*;

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
