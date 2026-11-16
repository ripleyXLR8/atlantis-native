//! The Windows virtual-key codes of the keys, for the 2004 program of
//! *Atlantis Evolution* (Atlantis III's engine) on the desktop
//! (`crate::a3`) and in the browser (`crate::a3_web`).

use winit::keyboard::KeyCode;

/// A key's Windows virtual-key code (the 2004 program's key handlers and
/// `GetKeyboardState`): letters, digits, the arrows, the editing keys,
/// Enter, Escape, Space, Tab, Backspace, the function keys, the modifiers
/// (`WM_KEYDOWN`'s generic codes), the keypad.
pub(crate) fn virtual_key(c: KeyCode) -> Option<u8> {
    use KeyCode as K;
    let letters = [
        K::KeyA,
        K::KeyB,
        K::KeyC,
        K::KeyD,
        K::KeyE,
        K::KeyF,
        K::KeyG,
        K::KeyH,
        K::KeyI,
        K::KeyJ,
        K::KeyK,
        K::KeyL,
        K::KeyM,
        K::KeyN,
        K::KeyO,
        K::KeyP,
        K::KeyQ,
        K::KeyR,
        K::KeyS,
        K::KeyT,
        K::KeyU,
        K::KeyV,
        K::KeyW,
        K::KeyX,
        K::KeyY,
        K::KeyZ,
    ];
    if let Some(i) = letters.iter().position(|&l| l == c) {
        return Some(b'A' + i as u8);
    }
    let digits = [
        K::Digit0,
        K::Digit1,
        K::Digit2,
        K::Digit3,
        K::Digit4,
        K::Digit5,
        K::Digit6,
        K::Digit7,
        K::Digit8,
        K::Digit9,
    ];
    if let Some(i) = digits.iter().position(|&d| d == c) {
        return Some(b'0' + i as u8);
    }
    let pad = [
        K::Numpad0,
        K::Numpad1,
        K::Numpad2,
        K::Numpad3,
        K::Numpad4,
        K::Numpad5,
        K::Numpad6,
        K::Numpad7,
        K::Numpad8,
        K::Numpad9,
    ];
    if let Some(i) = pad.iter().position(|&d| d == c) {
        return Some(0x60 + i as u8);
    }
    let f = [
        K::F1,
        K::F2,
        K::F3,
        K::F4,
        K::F5,
        K::F6,
        K::F7,
        K::F8,
        K::F9,
        K::F10,
        K::F11,
        K::F12,
    ];
    if let Some(i) = f.iter().position(|&k| k == c) {
        return Some(0x70 + i as u8);
    }
    Some(match c {
        K::Backspace => 0x08,
        K::Tab => 0x09,
        K::Enter | K::NumpadEnter => 0x0d,
        // `WM_KEYDOWN`'s generic codes for the modifiers.
        K::ShiftLeft | K::ShiftRight => 0x10,
        K::ControlLeft | K::ControlRight => 0x11,
        K::AltLeft | K::AltRight => 0x12,
        K::Pause => 0x13,
        K::CapsLock => 0x14,
        K::Escape => 0x1b,
        K::Space => 0x20,
        K::PageUp => 0x21,
        K::PageDown => 0x22,
        K::End => 0x23,
        K::Home => 0x24,
        K::ArrowLeft => 0x25,
        K::ArrowUp => 0x26,
        K::ArrowRight => 0x27,
        K::ArrowDown => 0x28,
        K::Insert => 0x2d,
        K::Delete => 0x2e,
        K::NumpadMultiply => 0x6a,
        K::NumpadAdd => 0x6b,
        K::NumpadSubtract => 0x6d,
        K::NumpadDecimal => 0x6e,
        K::NumpadDivide => 0x6f,
        K::NumLock => 0x90,
        K::ScrollLock => 0x91,
        _ => return None,
    })
}
