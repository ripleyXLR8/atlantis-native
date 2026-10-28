//! What a new game starts with.
//!
//! A new game (menu action 10, @ 0x00486eb0, for a player without a save)
//! runs the `/INIT` section of chapter 0, `scenar\chapi000.con`
//! (@ 0x00484040): its `/set(KEY=value)` lines choose the world (`monde`),
//! the place (`changewam`), the arrival angles (`ANGLEWARPX` /
//! `ANGLEWARPY`, degrees) and the view (`vue`), then play the opening
//! cinematic. Only those keys are read here, for the engine's quick start;
//! the interpreter itself is `atlantis2_engine::script`, which takes the C
//! conversions it needs ([`atoi`], [`atof`]) from here.

/// The `/set` lines of the `/INIT` section (up to `/FININIT`) of a CON
/// script, as (key, value), in order. Lines starting with `#` are comments.
pub fn init_sets(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if line.eq_ignore_ascii_case("/INIT") {
            inside = true;
            continue;
        }
        if line.eq_ignore_ascii_case("/FININIT") {
            break;
        }
        let set = line
            .as_bytes()
            .get(..5)
            .is_some_and(|b| b.eq_ignore_ascii_case(b"/set("));
        if !inside || !set {
            continue;
        }
        let body = line[5..].trim_end_matches(')');
        if let Some((k, v)) = body.split_once('=') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

/// The start of a new game, from `chapi000.con`'s `/INIT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewGame {
    /// The `MONDE` value (5, Tibet).
    pub monde: i32,
    /// The place file (`tibet.wam`).
    pub wam: String,
    /// The view id (2).
    pub view: u32,
    /// `ANGLEWARPX` / `ANGLEWARPY` in degrees (-90, 0).
    pub yaw_deg: i32,
    pub pitch_deg: i32,
}

/// MSVC's `atoi` (@ 0x005adbe4, the one the interpreter calls): white
/// space (tab to CR, space), one sign, then digits, accumulated with
/// wrap-around; anything else ends the number.
pub fn atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while matches!(s.get(i), Some(9..=13 | b' ')) {
        i += 1;
    }
    let neg = s.get(i) == Some(&b'-');
    if matches!(s.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let v = s[i.min(s.len())..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0i32, |v, &d| {
            v.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// MSVC's `atof` (@ 0x005ae620): white space, a sign, digits with one
/// decimal point, an optional exponent; 0 when there is no number. The
/// value is the nearest double (the CRT's conversion is exact for the
/// short decimals the scripts use).
pub fn atof(s: &[u8]) -> f64 {
    let mut i = 0;
    while matches!(s.get(i), Some(9..=13 | b' ')) {
        i += 1;
    }
    let start = i;
    if matches!(s.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let b = *i;
        while s.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - b
    };
    let mut n = digits(&mut i);
    if s.get(i) == Some(&b'.') {
        i += 1;
        n += digits(&mut i);
    }
    if n == 0 {
        return 0.0;
    }
    let mantissa_end = i;
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        if digits(&mut j) > 0 {
            i = j;
        }
    }
    let end = if i > mantissa_end { i } else { mantissa_end };
    std::str::from_utf8(&s[start..end])
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.0)
}

impl NewGame {
    /// Reads the start from chapter 0's text (keys in any case; the last
    /// line of a key wins). `None` when the place or the view is missing.
    pub fn from_chapter0(text: &str) -> Option<NewGame> {
        let (mut monde, mut wam, mut view, mut yaw, mut pitch) = (0, None, None, 0, 0);
        for (k, v) in init_sets(text) {
            match k.to_ascii_uppercase().as_str() {
                "MONDE" => monde = atoi(v.as_bytes()),
                "CHANGEWAM" => wam = Some(v),
                "VUE" => view = Some(atoi(v.as_bytes()) as u32),
                "ANGLEWARPX" => yaw = atoi(v.as_bytes()),
                "ANGLEWARPY" => pitch = atoi(v.as_bytes()),
                _ => {}
            }
        }
        Some(NewGame {
            monde,
            wam: wam?,
            view: view?,
            yaw_deg: yaw,
            pitch_deg: pitch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_init_section() {
        let text = "/INIT\r\n/set(monde=5)\r\n#/set(vue=9)\r\n/set(changewam=tibet.wam)\r\n\
                    /set(ANGLEWARPX=-90)\r\n/set(ANGLEWARPY=0)\r\n/set(vue=2)\r\n\
                    /set(enablesuj=A,B)\r\n/FININIT\r\n/000\r\n/set(vue=10)\r\n";
        let sets = init_sets(text);
        assert_eq!(sets.len(), 6);
        assert_eq!(sets[5], ("enablesuj".to_string(), "A,B".to_string()));
        assert_eq!(
            NewGame::from_chapter0(text),
            Some(NewGame {
                monde: 5,
                wam: "tibet.wam".to_string(),
                view: 2,
                yaw_deg: -90,
                pitch_deg: 0,
            })
        );
        assert_eq!(
            NewGame::from_chapter0("/INIT\n/set(vue=2)\n/FININIT\n"),
            None
        );
        assert_eq!(atoi(b" -12x"), -12);
        assert_eq!(atoi(b"-2147483648"), i32::MIN);
        assert_eq!(atoi(b"\r\n+7)"), 7);
        assert_eq!(atoi(b"+)"), 0);
        assert_eq!(atoi(b"12abc"), 12);
        assert_eq!(atof(b"1.5)"), 1.5);
        assert_eq!(atof(b" -0.25,x"), -0.25);
        assert_eq!(atof(b"2e1"), 20.0);
        assert_eq!(atof(b"3e)"), 3.0);
        assert_eq!(atof(b".)"), 0.0);
    }

    #[test]
    fn non_ascii_lines_are_skipped_not_split() {
        // 'é' (Latin-1 0xe9 decoded to U+00E9) spans bytes 4..6.
        assert!(init_sets("/INIT\nabcd\u{e9}x\n/set(vue=2)\n").len() == 1);
        assert!(init_sets("/INIT\n\u{e9}\n/se\n").is_empty());
    }
}
