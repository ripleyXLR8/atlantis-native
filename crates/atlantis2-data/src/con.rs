//! What a new game starts with.
//!
//! A new game (menu action 10, @ 0x00486eb0, for a player without a save)
//! runs the `/INIT` section of chapter 0, `scenar\chapi000.con`
//! (@ 0x00484040): its `/set(KEY=value)` lines choose the world (`monde`),
//! the place (`changewam`), the arrival angles (`ANGLEWARPX` /
//! `ANGLEWARPY`, degrees) and the view (`vue`), then play the opening
//! cinematic. Only those keys are read here; the script interpreter comes
//! later.

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

/// C `atoi` (the `/set` handler converts values with it).
fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let v = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |v, d| {
            v.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

impl NewGame {
    /// Reads the start from chapter 0's text (keys in any case; the last
    /// line of a key wins). `None` when the place or the view is missing.
    pub fn from_chapter0(text: &str) -> Option<NewGame> {
        let (mut monde, mut wam, mut view, mut yaw, mut pitch) = (0, None, None, 0, 0);
        for (k, v) in init_sets(text) {
            match k.to_ascii_uppercase().as_str() {
                "MONDE" => monde = atoi(&v),
                "CHANGEWAM" => wam = Some(v),
                "VUE" => view = Some(atoi(&v) as u32),
                "ANGLEWARPX" => yaw = atoi(&v),
                "ANGLEWARPY" => pitch = atoi(&v),
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
        assert_eq!(atoi(" -12x"), -12);
        assert_eq!(atoi("-2147483648"), i32::MIN);
    }

    #[test]
    fn non_ascii_lines_are_skipped_not_split() {
        // 'é' (Latin-1 0xe9 decoded to U+00E9) spans bytes 4..6.
        assert!(init_sets("/INIT\nabcd\u{e9}x\n/set(vue=2)\n").len() == 1);
        assert!(init_sets("/INIT\n\u{e9}\n/se\n").is_empty());
    }
}
