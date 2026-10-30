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
//!
//! The file changes the numbering of a [`Preset`] chosen by the device
//! ([`Preset::for_device`]): the R36S's, or the Linux evdev order of an
//! Xbox-style pad SDL has no mapping for (an Xbox Wireless Controller over
//! Bluetooth, for one).

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

/// What SDL tells of a joystick it has no game controller mapping for.
#[derive(Clone, Debug, Default)]
pub struct Device<'a> {
    pub name: &'a str,
    /// The joystick's GUID (SDL's 16 bytes).
    pub guid: [u8; 16],
    pub buttons: u32,
    pub axes: u32,
}

impl Device<'_> {
    /// The USB vendor id the GUID holds (SDL 2's layout: 16-bit words
    /// bus, crc, vendor, 0, product, 0, version, ...).
    pub fn vendor(&self) -> Option<u16> {
        let w = |i: usize| u16::from_le_bytes([self.guid[2 * i], self.guid[2 * i + 1]]);
        (w(3) == 0 && w(5) == 0 && w(2) != 0).then(|| w(2))
    }
}

/// The raw numbering a device starts from before `controls.ini`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// The R36S under ArkOS / dArkOS ("GO-Super Gamepad"), and any pad
    /// not recognised below (the numbering before the presets).
    R36s,
    /// The Linux kernel's order (`BTN_SOUTH` on, as xpad, xpadneo and
    /// hid-microsoft give an Xbox pad): buttons 0..10 A B X Y LB RB
    /// Back Start Guide LS RS, axes 0 1 left stick, 2 LT, 3 4 right
    /// stick, 5 RT, the D-pad a hat.
    Evdev,
}

impl Preset {
    /// Microsoft, 8BitDo, PDP, PowerA (two ids): Xbox-style pads.
    const VENDORS: [u16; 5] = [0x045e, 0x2dc8, 0x0e6f, 0x24c6, 0x20d6];

    /// The R36S's own pad keeps its numbering. Another pad takes the evdev
    /// order when it looks like an Xbox-style one (its vendor, or a name
    /// with xbox / x-box / microsoft / 8bitdo / controller / gamepad) and
    /// has that order's 10 or 11 buttons (no Guide, or one) and at least
    /// the 5 axes up to the right stick's y: a pad with more buttons
    /// (digital triggers, C / Z) numbers them otherwise.
    pub fn for_device(d: &Device) -> Preset {
        let name = d.name.to_ascii_lowercase();
        if name.contains("go-super") {
            return Preset::R36s;
        }
        let looks = d.vendor().is_some_and(|v| Self::VENDORS.contains(&v))
            || [
                "xbox",
                "x-box",
                "microsoft",
                "8bitdo",
                "controller",
                "gamepad",
            ]
            .iter()
            .any(|w| name.contains(w));
        if looks && (10..=11).contains(&d.buttons) && d.axes >= 5 {
            Preset::Evdev
        } else {
            Preset::R36s
        }
    }

    /// Its numbering, before `controls.ini`.
    pub fn controls(self) -> Controls {
        match self {
            Preset::R36s => Controls::default(),
            Preset::Evdev => Controls {
                buttons: HashMap::from([
                    (0, Btn::A),
                    (1, Btn::B),
                    (2, Btn::X),
                    (3, Btn::Y),
                    (4, Btn::L1),
                    (5, Btn::R1),
                    (6, Btn::Select), // Back / View
                    (7, Btn::Start),  // Start / Menu
                    (8, Btn::Select), // Guide (Xbox button)
                ]),
                lx: 0,
                ly: 1,
                rx: 3,
                ry: 4,
                ..Controls::default()
            },
        }
    }
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
        Controls::parse_over(Controls::default(), text)
    }

    /// `base` (a [`Preset`]'s numbering) changed by a `controls.ini`.
    pub fn parse_over(base: Controls, text: &str) -> Controls {
        let mut c = base;
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

    fn xbox_bt() -> Device<'static> {
        // Bus 0005 (Bluetooth), vendor 045e, product 028e, version 1130.
        let mut guid = [0u8; 16];
        guid[0] = 0x05;
        guid[4..6].copy_from_slice(&0x045eu16.to_le_bytes());
        guid[8..10].copy_from_slice(&0x028eu16.to_le_bytes());
        guid[12..14].copy_from_slice(&0x1130u16.to_le_bytes());
        Device {
            name: "Xbox Wireless Controller",
            guid,
            buttons: 11,
            axes: 7,
        }
    }

    #[test]
    fn the_preset_by_device() {
        let x = xbox_bt();
        assert_eq!(x.vendor(), Some(0x045e));
        assert_eq!(Preset::for_device(&x), Preset::Evdev);
        // By its vendor alone, or by its name alone.
        let anon = Device {
            name: "Pad",
            ..x.clone()
        };
        assert_eq!(Preset::for_device(&anon), Preset::Evdev);
        for name in [
            "8BitDo SN30 Pro",
            "X-Box pad",
            "Generic USB Gamepad",
            "Wireless Controller",
        ] {
            let d = Device {
                name,
                guid: [0; 16],
                ..x.clone()
            };
            assert_eq!(d.vendor(), None);
            assert_eq!(Preset::for_device(&d), Preset::Evdev, "{name}");
        }
        // Without Guide (10 buttons) too.
        let d = Device {
            buttons: 10,
            ..x.clone()
        };
        assert_eq!(Preset::for_device(&d), Preset::Evdev);
        // The R36S's pad, an unknown one, a pad with digital triggers or
        // without a right stick: as before.
        for d in [
            Device {
                name: "GO-Super Gamepad",
                guid: [0; 16],
                buttons: 17,
                axes: 4,
            },
            Device {
                name: "GO-Super Gamepad",
                guid: [0; 16],
                buttons: 11,
                axes: 6,
            },
            Device {
                name: "retrogame_joypad",
                guid: [0; 16],
                ..x.clone()
            },
            Device {
                buttons: 15,
                ..x.clone()
            },
            Device {
                axes: 4,
                ..x.clone()
            },
        ] {
            assert_eq!(Preset::for_device(&d), Preset::R36s, "{d:?}");
        }
    }

    #[test]
    fn the_evdev_layout() {
        let c = Preset::Evdev.controls();
        let b = |n: u8| c.buttons.get(&n).copied();
        assert_eq!(
            (0..=10).map(b).collect::<Vec<_>>(),
            [
                Some(Btn::A),
                Some(Btn::B),
                Some(Btn::X),
                Some(Btn::Y),
                Some(Btn::L1),
                Some(Btn::R1),
                Some(Btn::Select),
                Some(Btn::Start),
                Some(Btn::Select), // Guide
                None,
                None,
            ]
        );
        assert_eq!((c.lx, c.ly, c.rx, c.ry), (0, 1, 3, 4));
        // No D-pad buttons: the hat is read.
        assert!(!c
            .buttons
            .values()
            .any(|&v| matches!(v, Btn::Up | Btn::Down | Btn::Left | Btn::Right)));
        assert_eq!(Preset::R36s.controls(), Controls::default());
        // controls.ini changes the preset's numbers (here the file put on
        // the Pi for this pad gives the preset back, D-pad buttons aside).
        let f = Controls::parse_over(
            Preset::Evdev.controls(),
            "a=0\nb=1\nx=2\ny=3\nl1=4\nr1=5\nselect=6\nstart=7\nup=20\ndown=21\nleft=22\nright=23\n\
             lx=0\nly=1\nrx=3\nry=4\n",
        );
        assert_eq!(f.buttons.get(&6), Some(&Btn::Select));
        assert_eq!(f.buttons.get(&8), None); // one number per button
        assert_eq!(f.buttons.get(&20), Some(&Btn::Up));
        let f = Controls::parse_over(
            Preset::Evdev.controls(),
            "rx = 2\nstart = 9\ndeadzone = 8000",
        );
        assert_eq!((f.rx, f.ry), (2, 4));
        assert_eq!(f.buttons.get(&9), Some(&Btn::Start));
        assert_eq!(f.buttons.get(&7), None);
        assert_eq!(f.buttons.get(&0), Some(&Btn::A));
        assert_eq!(f.deadzone(false), 8000);
    }
}
