//! `controls.ini` next to the program: the raw joystick's numbering (for a
//! pad SDL has no mapping for), the sticks' dead zone and the speeds.
//!
//! The numbering and its R36S defaults are from Klaas Van den Broeck
//! (@Klaasvdb), github.com/Klaasvdb/atlantis-native-handheld
//! (GPL-3.0-or-later). Here the dead zone also applies to a pad SDL maps
//! (his was fixed at 6000 there), and two speeds were added.
//!
//! `name = value` lines (`#` or `;` start a comment): buttons `a b x y l1
//! r1 select start up down left right` (the D-pad as buttons; a hat is
//! read as well) and axes `lx ly rx ry` take the raw numbers; `deadzone`
//! is 0..32767 (default 6000 for a mapped pad, 4000 for a raw one);
//! `pointer_speed` and `look_speed` scale the pointer's and the view's
//! speeds (1 = as shipped, 0.5 = half as fast).

use std::collections::HashMap;

use crate::buttons::Btn;
use crate::motion::Tuning;

/// The dead zone of a pad SDL maps, of a raw joystick.
pub const MAPPED_DEADZONE: i16 = 6000;
pub const RAW_DEADZONE: i16 = 4000;

#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    /// The raw joystick's buttons.
    pub buttons: HashMap<u8, Btn>,
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
    /// Set in the file: for both kinds of pad.
    pub deadzone: Option<i16>,
    pub pointer_speed: f32,
    pub look_speed: f32,
}

impl Default for Controls {
    /// The R36S under ArkOS / dArkOS (its "GO-Super Gamepad" numbering).
    fn default() -> Self {
        Controls {
            buttons: HashMap::from([
                (0, Btn::A),
                (1, Btn::B),
                (2, Btn::X),
                (3, Btn::Y),
                (4, Btn::L1),
                (5, Btn::R1),
                (8, Btn::Up),
                (9, Btn::Down),
                (10, Btn::Left),
                (11, Btn::Right),
                (12, Btn::Select),
                (13, Btn::Start),
                (16, Btn::Select), // FN
            ]),
            lx: 0,
            ly: 1,
            rx: 2,
            ry: 3,
            deadzone: None,
            pointer_speed: 1.0,
            look_speed: 1.0,
        }
    }
}

impl Controls {
    /// The defaults changed by the text of a `controls.ini`; the lines not
    /// understood are logged and skipped.
    pub fn parse(text: &str) -> Controls {
        let mut c = Controls::default();
        let names = [
            ("a", Btn::A),
            ("b", Btn::B),
            ("x", Btn::X),
            ("y", Btn::Y),
            ("l1", Btn::L1),
            ("r1", Btn::R1),
            ("select", Btn::Select),
            ("start", Btn::Start),
            ("up", Btn::Up),
            ("down", Btn::Down),
            ("left", Btn::Left),
            ("right", Btn::Right),
        ];
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                log::warn!("controls.ini: '{line}' not understood");
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "pointer_speed" | "look_speed" => match v.parse::<f32>() {
                    Ok(f) if f.is_finite() && f > 0.0 => {
                        let f = f.min(10.0);
                        if k == "pointer_speed" {
                            c.pointer_speed = f;
                        } else {
                            c.look_speed = f;
                        }
                    }
                    _ => log::warn!("controls.ini: '{line}' not understood"),
                },
                _ => {
                    let Ok(n) = v.parse::<i32>() else {
                        log::warn!("controls.ini: '{line}' not understood");
                        continue;
                    };
                    let byte = n.clamp(0, 255) as u8;
                    match k.as_str() {
                        "lx" => c.lx = byte,
                        "ly" => c.ly = byte,
                        "rx" => c.rx = byte,
                        "ry" => c.ry = byte,
                        "deadzone" => c.deadzone = Some(n.clamp(0, 32767) as i16),
                        _ => match names.iter().find(|(name, _)| *name == k) {
                            Some(&(_, b)) => {
                                c.buttons.retain(|_, v| *v != b);
                                c.buttons.insert(byte, b);
                            }
                            None => log::warn!("controls.ini: '{k}' unknown"),
                        },
                    }
                }
            }
        }
        c
    }

    /// The dead zone of a mapped (`true`) or raw pad.
    pub fn deadzone(&self, mapped: bool) -> i16 {
        self.deadzone.unwrap_or(if mapped {
            MAPPED_DEADZONE
        } else {
            RAW_DEADZONE
        })
    }

    /// The motion's speeds with this file's factors.
    pub fn tuning(&self) -> Tuning {
        let t = Tuning::default();
        Tuning {
            pointer_speed: t.pointer_speed * self.pointer_speed,
            look_speed: t.look_speed * self.look_speed,
            ..t
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_without_a_file() {
        let c = Controls::parse("");
        assert_eq!(c, Controls::default());
        assert_eq!(c.deadzone(true), 6000);
        assert_eq!(c.deadzone(false), 4000);
        assert_eq!(c.tuning(), Tuning::default());
    }

    #[test]
    fn the_file_changes_numbers_deadzone_and_speeds() {
        let c = Controls::parse(
            "# my pad\n a = 1\nB=0\n; x\nstart = 13\nlx = 4\ndeadzone = 9000\n\
             pointer_speed = 0.5\nlook_speed = 2\nbogus = 3\nnonsense\nup = x\n",
        );
        assert_eq!(c.buttons.get(&1), Some(&Btn::A));
        assert_eq!(c.buttons.get(&0), Some(&Btn::B));
        // A button has one number: the old one is gone.
        assert_eq!(c.buttons.values().filter(|&&b| b == Btn::A).count(), 1);
        assert_eq!(c.lx, 4);
        // The dead zone of the file holds for a mapped pad too.
        assert_eq!(c.deadzone(true), 9000);
        assert_eq!(c.deadzone(false), 9000);
        let t = c.tuning();
        assert_eq!(t.pointer_speed, Tuning::default().pointer_speed * 0.5);
        assert_eq!(t.look_speed, Tuning::default().look_speed * 2.0);
        // Out-of-range values are clamped or refused.
        let c = Controls::parse("deadzone = 99999\npointer_speed = -1\nlook_speed = 50");
        assert_eq!(c.deadzone(true), 32767);
        assert_eq!(c.pointer_speed, 1.0);
        assert_eq!(c.look_speed, 10.0);
    }
}
