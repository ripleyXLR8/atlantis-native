//! The gamepad's buttons: what a press or a release asks of the game.
//!
//! Adapted from Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis-native-handheld (GPL-3.0-or-later), brought
//! into Atlantis Native with his agreement. Added here: Select outside a
//! game menu opens the port's options menu, which the D-pad, A and B then
//! drive; a pad unplugged lets go of what it held.
//!
//! | Pad                 | Game                                          |
//! |---------------------|-----------------------------------------------|
//! | left stick, D-pad   | pointer (the view, in the panoramas)          |
//! | right stick         | pointer, slower (fine aim)                    |
//! | A                   | left click                                    |
//! | B                   | right click                                   |
//! | X                   | Space                                         |
//! | Y                   | Enter                                         |
//! | Start               | Escape (the game's menu, skip a video)        |
//! | L1 / R1 (held)      | pointer slower / faster                       |
//! | Select (in a menu)  | on-screen keyboard (see [`crate::osk`])       |
//! | Select (elsewhere)  | the port's options menu                       |
//! | Select + Start      | quit                                          |

use atlantis_engine::sys::{Key, MouseButton};

use crate::osk::Osk;

/// The pad's buttons, whatever their numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Btn {
    A,
    B,
    X,
    Y,
    L1,
    R1,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
}

/// What a press or a release asks of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Mouse(MouseButton, bool),
    Key(Key, bool),
    /// Open or close the port's options menu.
    OptionsMenu,
    /// A key for the open options menu.
    MenuKey(Key),
    Quit,
}

#[derive(Default)]
pub struct Buttons {
    held: Vec<Btn>,
    /// The buttons whose press reached the game (their release must too).
    in_game: Vec<Btn>,
    pub osk: Osk,
    /// Start was pressed while Select was held (the quit, not the
    /// keyboard or the menu, on Select's release).
    select_combo: bool,
    /// A game menu reads the keyboard (set by the caller every frame).
    pub in_menu: bool,
    /// The port's options menu is open (set by the caller).
    pub options_open: bool,
}

impl Buttons {
    pub fn held(&self, b: Btn) -> bool {
        self.held.contains(&b)
    }

    /// The D-pad held, -1..1 on each axis (x right, y down).
    pub fn dpad(&self) -> (i8, i8) {
        let h = |b| i8::from(self.held(b));
        (h(Btn::Right) - h(Btn::Left), h(Btn::Down) - h(Btn::Up))
    }

    /// Whether the pad drives a menu or the keyboard rather than the
    /// pointer with its D-pad.
    pub fn dpad_taken(&self) -> bool {
        self.osk.active || self.options_open
    }

    /// The D-pad as a hat: presses and releases for what changed.
    pub fn set_hat(&mut self, up: bool, down: bool, left: bool, right: bool) -> Vec<Action> {
        let mut out = Vec::new();
        for (on, b) in [
            (up, Btn::Up),
            (down, Btn::Down),
            (left, Btn::Left),
            (right, Btn::Right),
        ] {
            if on != self.held(b) {
                out.extend(self.set(b, on));
            }
        }
        out
    }

    /// The pad is gone: everything it held is let go (the mouse buttons
    /// and keys it pressed are released in the game).
    pub fn release_all(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        // (Select's release would open a menu: dropped without effect)
        self.select_combo = true;
        for b in self.held.clone() {
            out.extend(self.set(b, false));
        }
        self.select_combo = false;
        out
    }

    /// Button `btn` pressed (`down`) or released.
    pub fn set(&mut self, btn: Btn, down: bool) -> Vec<Action> {
        if down {
            if self.held(btn) {
                return Vec::new();
            }
            self.held.push(btn);
        } else {
            if !self.held(btn) {
                return Vec::new();
            }
            self.held.retain(|&b| b != btn);
        }
        log::debug!(
            "pad {btn:?} {} (menu: {}, keyboard: {}, options: {})",
            if down { "down" } else { "up" },
            self.in_menu,
            self.osk.active,
            self.options_open
        );
        let select = self.held(Btn::Select);
        let tap = |k: Key| vec![Action::Key(k, true), Action::Key(k, false)];
        match (btn, down) {
            (Btn::Select, true) => self.select_combo = false,
            (Btn::Start, true) if select => {
                self.select_combo = true;
                return vec![Action::Quit];
            }
            (Btn::Select, false) => {
                if self.select_combo {
                    return Vec::new();
                }
                if self.osk.active {
                    self.osk.close();
                } else if self.options_open {
                    return vec![Action::OptionsMenu];
                } else if self.in_menu {
                    self.osk.open();
                } else {
                    return vec![Action::OptionsMenu];
                }
                return Vec::new();
            }
            _ => {}
        }
        if self.options_open {
            if !down {
                // (the release of a press made before the menu opened)
                return self.game_action(btn, false);
            }
            let key = match btn {
                Btn::Up => Key::Up,
                Btn::Down => Key::Down,
                Btn::Left => Key::Left,
                Btn::Right => Key::Right,
                Btn::A | Btn::Y => Key::Enter,
                Btn::B | Btn::Start => Key::Escape,
                _ => return Vec::new(),
            };
            return vec![Action::MenuKey(key)];
        }
        if self.osk.active {
            return match (btn, down) {
                (Btn::A, true) => tap(self.osk.typed()),
                (Btn::B, true) => tap(Key::Backspace),
                (Btn::X, true) => tap(Key::Space),
                (Btn::Y, true) => {
                    self.osk.toggle_case();
                    Vec::new()
                }
                (Btn::Start, true) => {
                    self.osk.close();
                    tap(Key::Enter)
                }
                (b, false) => self.game_action(b, false),
                _ => Vec::new(),
            };
        }
        self.game_action(btn, down)
    }

    /// The game's mouse button or key for `btn`; a release only for a
    /// press the game had.
    fn game_action(&mut self, btn: Btn, down: bool) -> Vec<Action> {
        let action = match btn {
            Btn::A => Action::Mouse(MouseButton::Left, down),
            Btn::B => Action::Mouse(MouseButton::Right, down),
            Btn::X => Action::Key(Key::Space, down),
            Btn::Y => Action::Key(Key::Enter, down),
            Btn::Start => Action::Key(Key::Escape, down),
            _ => return Vec::new(),
        };
        if down {
            self.in_game.push(btn);
        } else if self.in_game.contains(&btn) {
            self.in_game.retain(|&b| b != btn);
        } else {
            return Vec::new();
        }
        vec![action]
    }

    /// Once per engine frame: the keyboard's held moves (`stick_x`: the
    /// left stick's x, -1..1), and its closing when no menu reads the
    /// keys any more.
    pub fn tick(&mut self, stick_x: f32) {
        if !self.osk.active {
            return;
        }
        if !self.in_menu {
            self.osk.close();
            return;
        }
        let dir = if self.held(Btn::Left) || stick_x < -0.5 {
            -1
        } else if self.held(Btn::Right) || stick_x > 0.5 {
            1
        } else {
            0
        };
        self.osk.tick(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_click_and_press_keys() {
        let mut b = Buttons::default();
        assert_eq!(
            b.set(Btn::A, true),
            vec![Action::Mouse(MouseButton::Left, true)]
        );
        // A repeat of a held button does nothing.
        assert!(b.set(Btn::A, true).is_empty());
        assert_eq!(
            b.set(Btn::A, false),
            vec![Action::Mouse(MouseButton::Left, false)]
        );
        assert!(b.set(Btn::A, false).is_empty());
        assert_eq!(
            b.set(Btn::Start, true),
            vec![Action::Key(Key::Escape, true)]
        );
        assert_eq!(b.set(Btn::Y, true), vec![Action::Key(Key::Enter, true)]);
    }

    #[test]
    fn select_opens_the_options_menu_or_the_keyboard() {
        let mut b = Buttons::default();
        b.set(Btn::Select, true);
        assert_eq!(b.set(Btn::Select, false), vec![Action::OptionsMenu]);
        b.options_open = true;
        // In the options menu the D-pad and A / B drive it.
        assert_eq!(b.set(Btn::Down, true), vec![Action::MenuKey(Key::Down)]);
        // (no release reaches the game for a press it did not have)
        assert!(b.set(Btn::Down, false).is_empty());
        assert_eq!(b.set(Btn::A, true), vec![Action::MenuKey(Key::Enter)]);
        assert_eq!(b.set(Btn::B, true), vec![Action::MenuKey(Key::Escape)]);
        assert!(b.set(Btn::A, false).is_empty());
        assert!(b.dpad_taken());
        b.set(Btn::Select, true);
        assert_eq!(b.set(Btn::Select, false), vec![Action::OptionsMenu]);
        b.options_open = false;
        // In a game menu: the keyboard.
        b.in_menu = true;
        b.set(Btn::Select, true);
        assert!(b.set(Btn::Select, false).is_empty());
        assert!(b.osk.active);
        assert_eq!(
            b.set(Btn::X, true),
            vec![
                Action::Key(Key::Space, true),
                Action::Key(Key::Space, false)
            ]
        );
        assert_eq!(
            b.set(Btn::Start, true),
            vec![
                Action::Key(Key::Enter, true),
                Action::Key(Key::Enter, false)
            ]
        );
        assert!(!b.osk.active);
    }

    #[test]
    fn select_and_start_quit_without_opening_anything() {
        let mut b = Buttons::default();
        b.set(Btn::Select, true);
        assert_eq!(b.set(Btn::Start, true), vec![Action::Quit]);
        b.set(Btn::Start, false);
        assert!(b.set(Btn::Select, false).is_empty());
    }

    #[test]
    fn an_unplugged_pad_lets_go() {
        let mut b = Buttons::default();
        b.set(Btn::A, true);
        b.set(Btn::Right, true);
        b.set(Btn::Select, true);
        b.options_open = true;
        let out = b.release_all();
        assert_eq!(out, vec![Action::Mouse(MouseButton::Left, false)]);
        assert_eq!(b.dpad(), (0, 0));
        assert!(!b.held(Btn::Select));
        // Nothing left to release.
        assert!(b.release_all().is_empty());
    }

    #[test]
    fn the_hat_presses_and_releases_what_changed() {
        let mut b = Buttons::default();
        b.set_hat(true, false, false, true);
        assert_eq!(b.dpad(), (1, -1));
        b.set_hat(false, false, false, true);
        assert_eq!(b.dpad(), (1, 0));
        b.options_open = true;
        assert_eq!(
            b.set_hat(false, true, false, true),
            vec![Action::MenuKey(Key::Down)]
        );
    }

    #[test]
    fn the_keyboard_closes_when_no_menu_reads_keys() {
        let mut b = Buttons {
            in_menu: true,
            ..Buttons::default()
        };
        b.osk.open();
        b.tick(0.9);
        assert_eq!(b.osk.typed(), Key::Char('B'));
        b.in_menu = false;
        b.tick(0.0);
        assert!(!b.osk.active);
    }
}
