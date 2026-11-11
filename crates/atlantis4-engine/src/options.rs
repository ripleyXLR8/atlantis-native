//! The player's settings: `Backup/options.xml` as atlantis4-gog-en-2363910
//! reads it at start-up (0x00577530, from the `Application` start-up
//! @ 0x0057a558) and writes it when the game quits (0x005766b0, only
//! from the `Application` teardown 0x0057bce0 through 0x0057ac10), and
//! the options menu's volume steps (0x00514050 and its seven twins).
//!
//! **File** (TinyXML, `STATIC`): a declaration `<?xml version="1.0" ?>`
//! (encoding and standalone empty) then top-level elements, no root, one
//! per line, each `<Name attr="…" />`:
//!
//! - `GameVersion major="1" minor="0"` (constants);
//! - `DebugResolution x y` and `DebugFullScreen value`, only when the
//!   file read at start-up had them (`App+0x8760`, `+0x8762`); the
//!   original writes the window's size and full-screen state of the
//!   moment, Native the values read (`[INFERRED]` the same for a file
//!   nobody edited);
//! - `PermanentHelp`, `InverseLook`, `compassLook` (that case), `value`
//!   `%d` of `App+0x86b0`, `+0x86b1`, `+0x86b2` (defaults 1, 0, 0 set by
//!   the `Application` constructor @ 0x005790b5);
//! - `SFXVolume`, `MusicVolume`, `DialogVolume`, `VideoVolume`, `value`
//!   `%f` of the channel's volume (1.0 when unset);
//! - `DebugLanguage`, `DebugPlatform`, `DebugPart`, `DebugDistributor`
//!   `value`, each only when its string is not empty (they are set only
//!   by reading them);
//! - `ActivityTracking value path`, only when read (`App+0x8763`).
//!
//! **Reading**: every top-level element in file order (a repeated one:
//! the last wins); the three flags are `atoi(value) > 0`, the volumes
//! `(float)atof(value)` into `setChannelVolume`. A missing file leaves
//! the defaults.
//!
//! **Volume buttons** (`STATIC`, `EMULATED` steps,
//! [`step_up`]/[`step_down`]): `1/N` with `N` the row's
//! `<channel>VolumeSprite<i>` layouts (5), added or subtracted in
//! `float`, clamped to [0, 1], applied at once; from 1.0 down: 0.8,
//! 0.6000000238, 0.4000000358, 0.2000000328, 2.98e-08, 0. The row's
//! sprites ([`level_shown`], 0x00511490 and its three twins):
//! `<channel>VolumeSprite<i>` shows when `(i + 1) × step − 2⁻²³ ≤
//! volume` (x87, exact in `f64`), its `…Off` twin otherwise.

use std::fmt::Write as _;

use crate::savexml::XmlNode;

/// The four channels: the element written, the channel's name.
pub const VOLUMES: [(&str, &str); 4] = [
    ("SFXVolume", "sfx"),
    ("MusicVolume", "music"),
    ("DialogVolume", "dialog"),
    ("VideoVolume", "video"),
];

/// The file's name (the read prefixes `Backup/`, 0x0057a52f).
pub const FILE: &str = "options.xml";

/// The settings the original keeps in `Application` and the sound
/// manager.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `PermanentHelp` (`App+0x86b0`, default 1).
    pub permanent_help: bool,
    /// `InverseLook` (`+0x86b1`, default 0).
    pub inverse_look: bool,
    /// `compassLook` (`+0x86b2`, default 0).
    pub compass_look: bool,
    /// The channels set (`sfx`, `music`, `dialog`, `video`); unset is 1.0.
    pub volumes: Vec<(String, f32)>,
    pub debug_resolution: Option<(i32, i32)>,
    pub debug_full_screen: Option<i32>,
    pub debug_language: String,
    pub debug_platform: String,
    pub debug_part: String,
    pub debug_distributor: String,
    /// `ActivityTracking`'s `value` and `path`, when read.
    pub activity_tracking: Option<(i32, String)>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            permanent_help: true,
            inverse_look: false,
            compass_look: false,
            volumes: Vec::new(),
            debug_resolution: None,
            debug_full_screen: None,
            debug_language: String::new(),
            debug_platform: String::new(),
            debug_part: String::new(),
            debug_distributor: String::new(),
            activity_tracking: None,
        }
    }
}

/// C's `atoi`: leading white space, a sign, digits; 0 without digits.
pub fn atoi(s: &str) -> i32 {
    let t = s.trim_start();
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut v: i64 = 0;
    for c in rest.bytes().take_while(u8::is_ascii_digit) {
        v = (v * 10 + i64::from(c - b'0')).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl Settings {
    /// A channel's volume (`channelVolume` @ 0x00505dc0: 1.0 when unset).
    pub fn volume(&self, channel: &str) -> f32 {
        self.volumes
            .iter()
            .find(|(c, _)| c == channel)
            .map_or(1.0, |(_, v)| *v)
    }

    pub fn set_volume(&mut self, channel: &str, v: f32) {
        match self.volumes.iter_mut().find(|(c, _)| c == channel) {
            Some(e) => e.1 = v,
            None => self.volumes.push((channel.to_string(), v)),
        }
    }

    /// The settings of a file's text read as 0x00577530 reads them; a
    /// file that is not XML leaves the defaults.
    pub fn read(data: &[u8]) -> Settings {
        let mut s = Settings::default();
        let Ok(elements) = atlantis4_data::xml::parse(data) else {
            log::warn!("options.xml: not XML");
            return s;
        };
        for e in &elements {
            let value = e.attr("value").unwrap_or("");
            match e.name.as_str() {
                "DebugResolution" => {
                    let f = |n: &str| e.attr(n).map_or(0, |v| crate::sound::atof(v) as i32);
                    s.debug_resolution = Some((f("x"), f("y")));
                }
                "DebugFullScreen" => s.debug_full_screen = Some(atoi(value)),
                "PermanentHelp" => s.permanent_help = atoi(value) > 0,
                "InverseLook" => s.inverse_look = atoi(value) > 0,
                "compassLook" => s.compass_look = atoi(value) > 0,
                "DebugLanguage" => s.debug_language = value.to_string(),
                "DebugPlatform" => s.debug_platform = value.to_string(),
                "DebugPart" => s.debug_part = value.to_string(),
                "DebugDistributor" => s.debug_distributor = value.to_string(),
                "ActivityTracking" => {
                    s.activity_tracking =
                        Some((atoi(value), e.attr("path").unwrap_or("").to_string()));
                }
                n => {
                    if let Some((_, ch)) = VOLUMES.iter().find(|(el, _)| *el == n) {
                        s.set_volume(ch, crate::sound::atof(value));
                    }
                }
            }
        }
        s
    }

    /// The file 0x005766b0 writes, byte for byte (TinyXML's printer: one
    /// element per line, a new line after each).
    pub fn write(&self) -> Vec<u8> {
        let mut nodes = vec![XmlNode::new("GameVersion")
            .attr("major", "1")
            .attr("minor", "0")];
        if let Some((x, y)) = self.debug_resolution {
            nodes.push(
                XmlNode::new("DebugResolution")
                    .attr("x", &x.to_string())
                    .attr("y", &y.to_string()),
            );
        }
        if let Some(v) = self.debug_full_screen {
            nodes.push(XmlNode::new("DebugFullScreen").attr("value", &v.to_string()));
        }
        for (name, v) in [
            ("PermanentHelp", self.permanent_help),
            ("InverseLook", self.inverse_look),
            ("compassLook", self.compass_look),
        ] {
            nodes.push(XmlNode::new(name).attr("value", if v { "1" } else { "0" }));
        }
        for (el, ch) in VOLUMES {
            nodes.push(XmlNode::new(el).attr("value", &percent_f(self.volume(ch))));
        }
        for (name, v) in [
            ("DebugLanguage", &self.debug_language),
            ("DebugPlatform", &self.debug_platform),
            ("DebugPart", &self.debug_part),
            ("DebugDistributor", &self.debug_distributor),
        ] {
            if !v.is_empty() {
                nodes.push(XmlNode::new(name).attr("value", v));
            }
        }
        if let Some((v, path)) = &self.activity_tracking {
            nodes.push(
                XmlNode::new("ActivityTracking")
                    .attr("value", &v.to_string())
                    .attr("path", path),
            );
        }
        let mut out = b"<?xml version=\"1.0\" ?>\n".to_vec();
        for n in nodes {
            n.print_bytes(0, &mut out);
            out.push(b'\n');
        }
        out
    }
}

/// C's `"%f"` of a `float` (promoted to `double`): six decimals.
pub fn percent_f(v: f32) -> String {
    let mut s = String::new();
    let _ = write!(s, "{:.6}", f64::from(v));
    s
}

/// The `+` button (@ 0x00514377): `float(v + step)`, at most 1.
pub fn step_up(v: f32, step: f32) -> f32 {
    let v = v + step;
    if 1.0 < v {
        1.0
    } else {
        v
    }
}

/// The `-` button (@ 0x005147a7): `float(v − step)`, at least 0.
pub fn step_down(v: f32, step: f32) -> f32 {
    let v = v - step;
    if 0.0 > v {
        0.0
    } else {
        v
    }
}

/// The step of a row of `count` sprites (`fld1; fdivrp` then a `float`
/// store @ 0x0051422c).
pub fn step(count: usize) -> f32 {
    (1.0 / count as f64) as f32
}

/// Whether sprite `i` of a row shows its "on" picture for `volume`
/// (@ 0x005118b3..0x005118e7): `(i + 1) × step − 2⁻²³ ≤ volume` in x87
/// extended precision, which `f64` holds exactly here (a 24-bit step
/// times a small integer, minus 2⁻²³).
pub fn level_shown(i: usize, step: f32, volume: f32) -> bool {
    let x = (i as f64 + 1.0) * f64::from(step) - 1.192_092_895_507_812_5e-7;
    x <= f64::from(volume)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_steps_leave_the_emulated_levels() {
        let s = step(5);
        assert_eq!(s.to_bits(), 0x3e4c_cccd);
        let mut v = 1.0f32;
        let mut down = Vec::new();
        for _ in 0..6 {
            v = step_down(v, s);
            down.push(v.to_bits());
        }
        assert_eq!(
            down,
            [
                0x3f4c_cccd,
                0x3f19_999a,
                0x3ecc_ccce,
                0x3e4c_cccf,
                0x3300_0000,
                0
            ]
        );
        let mut up = Vec::new();
        for _ in 0..6 {
            v = step_up(v, s);
            up.push(v.to_bits());
        }
        assert_eq!(
            up,
            [
                0x3e4c_cccd,
                0x3ecc_cccd,
                0x3f19_999a,
                0x3f4c_cccd,
                0x3f80_0000,
                0x3f80_0000
            ]
        );
    }

    #[test]
    fn the_leftover_shows_no_sprite_and_prints_as_zero() {
        let s = step(5);
        let left = f32::from_bits(0x3300_0000);
        assert!((0..5).all(|i| !level_shown(i, s, left)));
        assert_eq!(percent_f(left), "0.000000");
        // 0.2000000328 lights the first sprite only.
        let l = f32::from_bits(0x3e4c_cccf);
        let lit: Vec<bool> = (0..5).map(|i| level_shown(i, s, l)).collect();
        assert_eq!(lit, [true, false, false, false, false]);
        // 0.6000000238 lights three.
        let l = f32::from_bits(0x3f19_999a);
        assert_eq!((0..5).filter(|&i| level_shown(i, s, l)).count(), 3);
        assert!((0..5).all(|i| level_shown(i, s, 1.0)));
        assert!((0..5).all(|i| !level_shown(i, s, 0.0)));
    }

    #[test]
    fn a_default_file_is_written_as_tinyxml_prints_it() {
        let text = String::from_utf8(Settings::default().write()).unwrap();
        assert_eq!(
            text,
            "<?xml version=\"1.0\" ?>\n\
             <GameVersion major=\"1\" minor=\"0\" />\n\
             <PermanentHelp value=\"1\" />\n\
             <InverseLook value=\"0\" />\n\
             <compassLook value=\"0\" />\n\
             <SFXVolume value=\"1.000000\" />\n\
             <MusicVolume value=\"1.000000\" />\n\
             <DialogVolume value=\"1.000000\" />\n\
             <VideoVolume value=\"1.000000\" />\n"
        );
    }

    #[test]
    fn options_round_trip_through_atof_and_percent_f() {
        let mut s = Settings {
            inverse_look: true,
            permanent_help: false,
            ..Settings::default()
        };
        let step = step(5);
        let mut v = 1.0f32;
        for _ in 0..3 {
            v = step_down(v, step);
        }
        s.set_volume("music", v);
        s.set_volume("sfx", f32::from_bits(0x3300_0000));
        s.debug_language = "fr".into();
        let text = s.write();
        let back = Settings::read(&text);
        assert!(back.inverse_look && !back.permanent_help && !back.compass_look);
        // 0.4000000358 → "0.400000" → 0.4.
        assert_eq!(back.volume("music"), 0.4);
        assert_eq!(back.volume("sfx"), 0.0);
        assert_eq!(back.volume("video"), 1.0);
        assert_eq!(back.debug_language, "fr");
        // Written again: the same bytes.
        assert_eq!(back.write(), Settings::read(&back.write()).write());
        let t = String::from_utf8(text).unwrap();
        assert!(t.contains("<MusicVolume value=\"0.400000\" />\n"));
        assert!(t.contains("<DebugLanguage value=\"fr\" />\n"));
    }

    #[test]
    fn flags_are_atoi_greater_than_zero_and_the_last_element_wins() {
        let s = Settings::read(
            b"<?xml version=\"1.0\" ?>\n<PermanentHelp value=\"-3\" />\n\
              <InverseLook value=\" 2x\" />\n<InverseLook value=\"0\" />\n\
              <compassLook value=\"7\" />\n<SFXVolume value=\"0.25junk\" />\n",
        );
        assert!(!s.permanent_help && !s.inverse_look && s.compass_look);
        assert_eq!(s.volume("sfx"), 0.25);
        assert!(Settings::read(b"not xml <").permanent_help);
    }
}
