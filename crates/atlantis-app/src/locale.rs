//! The system's language, for a game whose disc holds several and none was
//! chosen (The Secrets of Atlantis's international DVD: its installer
//! preselects the system's language, [`atlantis3_data::atlantis5::default_language`]).

/// The system's language as an ISO 639-1 code (`de`, `en`, ...), possibly
/// with a region (`de-DE`, `de_DE.UTF-8`); `None` when unknown.
///
/// Windows: the user's interface language (`GetUserDefaultUILanguage`, the
/// one NSIS installers start from). The web: the browser's
/// (`navigator.language`). macOS: the system's locale (`AppleLocale`),
/// else the environment's. Elsewhere: `LC_ALL`, `LC_MESSAGES`, then `LANG`
/// (`C` and `POSIX` are no language).
pub fn system_language() -> Option<String> {
    imp()
}

#[cfg(windows)]
fn imp() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // SAFETY: no arguments, no pointers.
    let id = unsafe { GetUserDefaultUILanguage() };
    code_of_langid(id).map(str::to_string)
}

#[cfg(target_arch = "wasm32")]
fn imp() -> Option<String> {
    let nav = js_sys::Reflect::get(&js_sys::global(), &"navigator".into()).ok()?;
    js_sys::Reflect::get(&nav, &"language".into())
        .ok()?
        .as_string()
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "macos")]
fn imp() -> Option<String> {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(from_env)
}

#[cfg(not(any(windows, target_arch = "wasm32", target_os = "macos")))]
fn imp() -> Option<String> {
    from_env()
}

/// `LC_ALL`, `LC_MESSAGES` or `LANG`, the first set to a language.
#[cfg(not(any(windows, target_arch = "wasm32")))]
fn from_env() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .filter(|v| v != "C" && v != "POSIX" && !v.starts_with("C."))
}

/// The ISO 639-1 code of a Windows language id's primary language (its
/// low 10 bits), for the languages the games come in.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn code_of_langid(id: u16) -> Option<&'static str> {
    Some(match id & 0x3ff {
        0x07 => "de",
        0x09 => "en",
        0x0a => "es",
        0x0c => "fr",
        0x10 => "it",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_language_ids() {
        assert_eq!(code_of_langid(1031), Some("de"));
        assert_eq!(code_of_langid(2057), Some("en"));
        assert_eq!(code_of_langid(3082), Some("es"));
        assert_eq!(code_of_langid(1040), Some("it"));
        assert_eq!(code_of_langid(1036), Some("fr"));
        assert_eq!(code_of_langid(1043), None);
    }
}
