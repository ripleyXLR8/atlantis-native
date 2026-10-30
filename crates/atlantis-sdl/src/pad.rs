//! The gamepad through SDL: which device, its events as [`Btn`] presses
//! and stick values (the meaning of both is in `atlantis_sdl`).
//!
//! Adapted from Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement. Changed here: a pad unplugged
//! lets go of its buttons and another one connected takes over; only the
//! events of the pad in use count; the dead zone of `controls.ini` holds
//! for a pad SDL maps too.
//!
//! SDL's game controller mapping is used when SDL knows the pad (its own
//! database, `$SDL_GAMECONTROLLERCONFIG`, or a `gamecontrollerdb.txt` next
//! to the program); otherwise the raw joystick, its buttons and axes
//! numbered by a preset chosen by the device (the R36S's, or the Linux
//! evdev order of an Xbox-style pad) as changed by `controls.ini` next to
//! the program ([`atlantis_sdl::config`]).

use std::collections::HashMap;
use std::path::Path;

use atlantis_sdl::buttons::{Action, Btn, Buttons};
use atlantis_sdl::config::{Controls, Device, Preset};
use atlantis_sdl::motion::{radial, Inputs};
use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::joystick::{HatState, Joystick};
use sdl2::{EventPump, GameControllerSubsystem, JoystickSubsystem};

pub struct Pad {
    controller: Option<GameController>,
    joystick: Option<Joystick>,
    pub controls: Controls,
    /// The text of `controls.ini`, applied over each device's preset.
    ini: Option<String>,
    /// The raw buttons read besides a mapped pad's (see [`Pad::attach`]).
    raw_extra: HashMap<u8, Btn>,
    /// The sticks' raw axes (left x, y, right x, y).
    axes: [i16; 4],
    pub buttons: Buttons,
}

impl Pad {
    pub fn open(pads: &GameControllerSubsystem, joys: &JoystickSubsystem, dir: &Path) -> Pad {
        for file in [
            dir.join("gamecontrollerdb.txt"),
            std::env::var_os("SDL_GAMECONTROLLERCONFIG_FILE")
                .map(Into::into)
                .unwrap_or_default(),
        ] {
            if file.is_file() {
                match pads.load_mappings(&file) {
                    Ok(n) => log::info!("{} pad mappings from {}", n, file.display()),
                    Err(e) => log::warn!("{}: {e}", file.display()),
                }
            }
        }
        let path = dir.join("controls.ini");
        let ini = std::fs::read_to_string(&path).ok();
        if ini.is_some() {
            log::info!("controls from {}", path.display());
        }
        let mut pad = Pad {
            controller: None,
            joystick: None,
            controls: Controls::parse_over(Controls::default(), ini.as_deref().unwrap_or("")),
            ini,
            raw_extra: HashMap::new(),
            axes: [0; 4],
            buttons: Buttons::default(),
        };
        pad.attach_any(pads, joys);
        if pad.controller.is_none() && pad.joystick.is_none() {
            log::warn!("no gamepad found (one plugged in later is taken)");
        }
        pad
    }

    fn attached(&self) -> bool {
        self.controller.is_some() || self.joystick.is_some()
    }

    /// The instance id of the pad in use.
    fn instance(&self) -> Option<u32> {
        match (&self.controller, &self.joystick) {
            (Some(c), _) => Some(c.instance_id()),
            (None, Some(j)) => Some(j.instance_id()),
            _ => None,
        }
    }

    /// Takes the first device that opens.
    fn attach_any(&mut self, pads: &GameControllerSubsystem, joys: &JoystickSubsystem) {
        for i in 0..joys.num_joysticks().unwrap_or(0) {
            self.attach(i, pads, joys);
            if self.attached() {
                break;
            }
        }
    }

    /// The numbering of `preset` changed by `controls.ini`.
    fn set_controls(&mut self, preset: Preset) {
        self.controls = Controls::parse_over(preset.controls(), self.ini.as_deref().unwrap_or(""));
    }

    /// Takes device `i`: as a game controller when SDL maps it, else raw.
    fn attach(&mut self, i: u32, pads: &GameControllerSubsystem, joys: &JoystickSubsystem) {
        if self.attached() {
            return;
        }
        self.axes = [0; 4];
        self.raw_extra.clear();
        if pads.is_game_controller(i) {
            match pads.open(i) {
                Ok(c) => {
                    log::info!("gamepad {i}: {} (mapped)", c.name());
                    self.set_controls(Preset::R36s);
                    let mapping = c.mapping();
                    log::debug!("mapping: {mapping}");
                    // Start and Select a mapping leaves out, or (the R36S's
                    // "GO-Super Gamepad") binds to other buttons: read raw.
                    let r36s = c.name() == "GO-Super Gamepad";
                    for (&n, &b) in &self.controls.buttons {
                        let name = match b {
                            Btn::Start => "start",
                            Btn::Select => "back",
                            _ => continue,
                        };
                        let bound = mapping
                            .split(',')
                            .any(|f| f.starts_with(&format!("{name}:")));
                        let here = mapping.split(',').any(|f| f == format!("{name}:b{n}"));
                        if !bound || (r36s && !here) {
                            self.raw_extra.insert(n, b);
                        }
                    }
                    if !self.raw_extra.is_empty() {
                        log::info!("read raw: {:?}", self.raw_extra);
                    }
                    self.controller = Some(c);
                    return;
                }
                Err(e) => log::warn!("gamepad {i}: {e}"),
            }
        }
        match joys.open(i) {
            Ok(j) => {
                log::info!(
                    "gamepad {i}: {} (raw: {} buttons, {} axes, {} hats; see controls.ini)",
                    j.name(),
                    j.num_buttons(),
                    j.num_axes(),
                    j.num_hats()
                );
                let dev = Device {
                    name: &j.name(),
                    guid: j.guid().raw().data,
                    buttons: j.num_buttons(),
                    axes: j.num_axes(),
                };
                let preset = Preset::for_device(&dev);
                log::info!(
                    "gamepad {i}: raw preset {} (vendor {:04x?}){}",
                    match preset {
                        Preset::R36s => "R36S",
                        Preset::Evdev => "evdev (Xbox-style: A B X Y LB RB Back Start Guide)",
                    },
                    dev.vendor(),
                    if self.ini.is_some() {
                        ", changed by controls.ini"
                    } else {
                        ""
                    }
                );
                self.set_controls(preset);
                self.joystick = Some(j);
            }
            Err(e) => log::warn!("gamepad {i}: {e}"),
        }
    }

    /// The pad with instance id `which` is gone: if it is the one in use,
    /// what it held is let go, and another connected pad takes over.
    fn detach(
        &mut self,
        which: u32,
        pads: &GameControllerSubsystem,
        joys: &JoystickSubsystem,
    ) -> Vec<Action> {
        if self.instance() != Some(which) {
            return Vec::new();
        }
        log::info!("gamepad removed");
        self.controller = None;
        self.joystick = None;
        self.axes = [0; 4];
        let out = self.buttons.release_all();
        self.attach_any(pads, joys);
        out
    }

    /// The pointer's inputs this frame (the D-pad not counted while it
    /// drives the keyboard or the options menu).
    pub fn inputs(&self) -> Inputs {
        let dz = self.controls.deadzone(self.controller.is_some());
        let [lx, ly, rx, ry] = self.axes;
        let typing = self.buttons.osk.active;
        Inputs {
            // (the left stick picks the keyboard's letters)
            left: if typing {
                (0.0, 0.0)
            } else {
                radial(lx, ly, dz)
            },
            right: if typing {
                (0.0, 0.0)
            } else {
                radial(rx, ry, dz)
            },
            dpad: if self.buttons.dpad_taken() {
                (0, 0)
            } else {
                self.buttons.dpad()
            },
            slow: self.buttons.held(Btn::L1),
            fast: self.buttons.held(Btn::R1),
        }
    }

    /// The left stick's x after the dead zone, for the keyboard.
    pub fn stick_x(&self) -> f32 {
        let dz = self.controls.deadzone(self.controller.is_some());
        radial(self.axes[0], self.axes[1], dz).0
    }

    /// The SDL event's actions (none when it is not the pad's).
    pub fn event(
        &mut self,
        e: &Event,
        pads: &GameControllerSubsystem,
        joys: &JoystickSubsystem,
    ) -> Vec<Action> {
        let mapped = self.controller.is_some();
        let inst = self.instance();
        let ours = |which: u32| inst == Some(which);
        match *e {
            Event::ControllerDeviceAdded { which, .. } | Event::JoyDeviceAdded { which, .. } => {
                self.attach(which, pads, joys);
                Vec::new()
            }
            Event::ControllerDeviceRemoved { which, .. }
            | Event::JoyDeviceRemoved { which, .. } => self.detach(which, pads, joys),
            Event::ControllerButtonDown { which, button, .. } if mapped && ours(which) => {
                controller_btn(button).map_or_else(Vec::new, |b| self.buttons.set(b, true))
            }
            Event::ControllerButtonUp { which, button, .. } if mapped && ours(which) => {
                controller_btn(button).map_or_else(Vec::new, |b| self.buttons.set(b, false))
            }
            Event::ControllerAxisMotion {
                which, axis, value, ..
            } if mapped && ours(which) => {
                let i = match axis {
                    Axis::LeftX => 0,
                    Axis::LeftY => 1,
                    Axis::RightX => 2,
                    Axis::RightY => 3,
                    _ => return Vec::new(),
                };
                self.axes[i] = value;
                Vec::new()
            }
            Event::JoyButtonDown {
                which, button_idx, ..
            } if ours(which) => match self.raw_button(button_idx) {
                Some(b) => self.buttons.set(b, true),
                None => Vec::new(),
            },
            Event::JoyButtonUp {
                which, button_idx, ..
            } if ours(which) => match self.raw_button(button_idx) {
                Some(b) => self.buttons.set(b, false),
                None => Vec::new(),
            },
            Event::JoyAxisMotion {
                which,
                axis_idx,
                value,
                ..
            } if !mapped && ours(which) => {
                let c = &self.controls;
                for (i, n) in [c.lx, c.ly, c.rx, c.ry].into_iter().enumerate() {
                    if axis_idx == n {
                        self.axes[i] = value;
                    }
                }
                Vec::new()
            }
            Event::JoyHatMotion { which, state, .. } if !mapped && ours(which) => {
                let (u, d, l, r) = match state {
                    HatState::Up => (true, false, false, false),
                    HatState::Down => (false, true, false, false),
                    HatState::Left => (false, false, true, false),
                    HatState::Right => (false, false, false, true),
                    HatState::LeftUp => (true, false, true, false),
                    HatState::RightUp => (true, false, false, true),
                    HatState::LeftDown => (false, true, true, false),
                    HatState::RightDown => (false, true, false, true),
                    HatState::Centered => (false, false, false, false),
                };
                self.buttons.set_hat(u, d, l, r)
            }
            _ => Vec::new(),
        }
    }

    /// The pad's button for raw button `n`: all of them without a mapping,
    /// the left-out ones with one.
    fn raw_button(&self, n: u8) -> Option<Btn> {
        if self.controller.is_some() {
            self.raw_extra.get(&n).copied()
        } else {
            self.controls.buttons.get(&n).copied()
        }
    }
}

fn controller_btn(b: Button) -> Option<Btn> {
    Some(match b {
        Button::A => Btn::A,
        Button::B => Btn::B,
        Button::X => Btn::X,
        Button::Y => Btn::Y,
        Button::LeftShoulder => Btn::L1,
        Button::RightShoulder => Btn::R1,
        // (some mappings name the R36S's Select or FN key differently)
        Button::Back | Button::Guide | Button::Misc1 => Btn::Select,
        Button::Start => Btn::Start,
        Button::DPadUp => Btn::Up,
        Button::DPadDown => Btn::Down,
        Button::DPadLeft => Btn::Left,
        Button::DPadRight => Btn::Right,
        _ => return None,
    })
}

/// Prints the raw joystick's events for 30 seconds (or until a quit): the
/// numbers to put in `controls.ini`.
pub fn input_test(events: &mut EventPump, joys: &JoystickSubsystem) {
    println!("Press every button, move the sticks and the D-pad (30 s)...");
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut joysticks = Vec::new();
    while std::time::Instant::now() < end {
        for e in events.poll_iter() {
            match e {
                Event::JoyDeviceAdded { which, .. } => {
                    if let Ok(j) = joys.open(which) {
                        println!(
                            "device {which}: {} ({} buttons, {} axes, {} hats)",
                            j.name(),
                            j.num_buttons(),
                            j.num_axes(),
                            j.num_hats()
                        );
                        joysticks.push(j);
                    }
                }
                Event::JoyButtonDown { button_idx, .. } => println!("button {button_idx} down"),
                Event::JoyAxisMotion {
                    axis_idx, value, ..
                } if value.unsigned_abs() > 16000 => println!("axis {axis_idx} = {value}"),
                Event::JoyHatMotion { hat_idx, state, .. } => {
                    println!("hat {hat_idx} {state:?}")
                }
                Event::ControllerButtonDown { button, .. } => {
                    println!("(mapped) {button:?}")
                }
                Event::Quit { .. } => return,
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
