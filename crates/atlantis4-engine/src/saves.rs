//! The saves on disk and their loading (atlantis4-gog-en-2363910).
//!
//! **Where** (`STATIC`): every save name is logical, `Backup/<file>`.
//! `TeFile`'s path helper (@ 0x0050f810) strips the `Backup/` prefix (the
//! global string @ 0x00605488, set from 0x005b4b24) and hands the rest to
//! `TeGetBackupPath` (@ 0x005513f0), which expands `backupPathTemplate`
//! (`Texts/PC/BackupPathTemplate.lua`: `"$(CSIDL_PERSONAL)/$(APPNAME)"`):
//! `$(CSIDL_PERSONAL)` by `SHGetFolderPathA(0x8005)` (the user's
//! documents, created if missing), `$(CSIDL_APPDATA)` by `0x801a`,
//! `$(APPNAME)` by the application's name, then `/` and the file. The
//! `SaveGame` worker (@ 0x00524100) passes `save.xml` or `N.xml` straight
//! to `TeGetBackupPath`; `Game::load` (@ 0x00529e30) passes `Backup/` +
//! the name through 0x0050f810: both reach `<Documents>/<APPNAME>/<file>`.
//! `$(APPNAME)` is `applicationTitle` (`Atlantis Evolution`)
//! `[INFERRED]`. Native keeps the same layout under its own data folder
//! ([`backup_dir`]); the game's folder is never written.
//!
//! **The file** (`STATIC` @ 0x00507e30): `fopen_s(…, "w")`, a text-mode
//! stream (the mode string @ 0x005b49a8), so every `"\n"` TinyXML prints
//! is `"\r\n"` on disk (MSVCR100's text mode); no byte-order mark (the
//! document's flag `+0x74` is clear on a new document). [`to_disk`].
//!
//! **The menus** (`STATIC`): the main menu's *continue* loads
//! `save.xml` (@ 0x00565e20: `Game::enter`, then `Game::load("save.xml")`);
//! its *new game* (@ 0x00565af0) asks for a confirmation when
//! `Backup/save.xml` exists, then (@ 0x005655c0) **removes**
//! `Backup/save.xml` and `Backup/0.xml`..`Backup/10.xml` before the game
//! starts. The loading menu (`GUI/LoadingMenu.lua`, @ 0x0051a2b0) makes
//! one `SaveButton` per layout named `"0"`, `"1"`, … that the file has
//! (six in the data: `0`..`5`); each (@ 0x005185b0) tests
//! `Backup/<N>.xml`: missing, the button is disabled; present, it is
//! parsed (`LoadingMenu::SaveButton::SaveButton: Failed to load file
//! "%s"` on an error) and its `PlayedTime` `value` read. A click
//! (@ 0x00518460) keeps `<N>.xml` (menu `+0x384`) and opens
//! `GUI/ConfirmLoadGame.lua`; its *yes* (@ 0x00519f40) runs `Game::enter`
//! and `Game::load("<N>.xml")`. The chapter saves are written by the
//! scripts themselves (`SaveGame("N.xml")`, six calls, N = 0..5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atlantis4_data::xml::{self, to_cp1252, Element};
use atlantis4_script::LoadedValue;

/// The autosave (`ChangeWarp`'s, `Game::leave`'s; the main menu's
/// *continue*).
pub const AUTOSAVE: &str = "save.xml";

/// The files the main menu's new game removes (@ 0x005655c0, strings
/// @ 0x005b6ed0..0x005b6f80), in its order.
pub const NEW_GAME_REMOVES: [&str; 12] = [
    "save.xml", "0.xml", "1.xml", "2.xml", "3.xml", "4.xml", "5.xml", "6.xml", "7.xml", "8.xml",
    "9.xml", "10.xml",
];

/// The loading menu's slots: its file's layouts `"0"`..`"5"` (`DATA`,
/// `GUI/HD/LoadingMenu.lua`; the menu stops at the first missing name).
pub const CHAPTER_SLOTS: usize = 6;

/// `TeGetBackupPath`'s folder for `template`: `$(CSIDL_PERSONAL)` and
/// `$(CSIDL_APPDATA)` replaced by `personal` and `appdata`, `$(APPNAME)` by
/// `app_name`.
pub fn backup_dir(template: &str, app_name: &str, personal: &Path, appdata: &Path) -> PathBuf {
    let t = template
        .replace("$(CSIDL_PERSONAL)", &personal.to_string_lossy())
        .replace("$(CSIDL_APPDATA)", &appdata.to_string_lossy())
        .replace("$(APPNAME)", app_name);
    PathBuf::from(t.replace('\\', "/"))
}

/// The backup folder under Native's data folder `root`: the user's
/// documents mirrored as `root/Documents`, the application data as
/// `root/AppData`.
pub fn native_backup_dir(root: &Path, template: &str, app_name: &str) -> PathBuf {
    backup_dir(
        template,
        app_name,
        &root.join("Documents"),
        &root.join("AppData"),
    )
}

/// The text as a text-mode stream writes it: `"\n"` → `"\r\n"`.
pub fn to_disk(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + text.len() / 16);
    for &c in text {
        if c == b'\n' {
            out.push(b'\r');
        }
        out.push(c);
    }
    out
}

/// The save name of a logical path: `Backup/` dropped (0x0050f810), `\`
/// as `/` (0x00552bd0).
pub fn file_name(logical: &str) -> String {
    let p = logical.replace('\\', "/");
    match p.strip_prefix("Backup/") {
        Some(rest) => rest.to_string(),
        None => p,
    }
}

/// The folder the saves go to: a directory, or memory (tests, the web
/// build until it has its own store).
#[derive(Clone, Debug)]
pub enum BackupFolder {
    Memory(BTreeMap<String, Vec<u8>>),
    Dir(PathBuf),
}

impl Default for BackupFolder {
    fn default() -> Self {
        BackupFolder::Memory(BTreeMap::new())
    }
}

impl BackupFolder {
    /// The file's bytes as on disk.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let name = file_name(name);
        match self {
            BackupFolder::Memory(m) => m.get(&name).cloned(),
            BackupFolder::Dir(d) => std::fs::read(d.join(&name)).ok(),
        }
    }

    pub fn exists(&self, name: &str) -> bool {
        let name = file_name(name);
        match self {
            BackupFolder::Memory(m) => m.contains_key(&name),
            BackupFolder::Dir(d) => d.join(&name).is_file(),
        }
    }

    /// Writes the bytes as they go on disk; false when it failed (the
    /// original logs `ActionManager::Save: error %s`).
    pub fn write(&mut self, name: &str, bytes: &[u8]) -> bool {
        let name = file_name(name);
        match self {
            BackupFolder::Memory(m) => {
                m.insert(name, bytes.to_vec());
                true
            }
            BackupFolder::Dir(d) => {
                let path = d.join(&name);
                let r = path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, bytes));
                match r {
                    Ok(()) => true,
                    Err(e) => {
                        log::warn!("ActionManager::Save: error {}: {e}", path.display());
                        false
                    }
                }
            }
        }
    }

    /// `remove(path)` (0x0050fc20); a missing file is no error.
    pub fn remove(&mut self, name: &str) {
        let name = file_name(name);
        match self {
            BackupFolder::Memory(m) => {
                m.remove(&name);
            }
            BackupFolder::Dir(d) => {
                let _ = std::fs::remove_file(d.join(&name));
            }
        }
    }
}

/// A slot of the loading menu.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveSlot {
    /// The layout's name, `"0"`..`"5"`.
    pub index: usize,
    /// The save name `Game::load` gets (`"<N>.xml"`).
    pub file: String,
    /// The file exists (the button is enabled).
    pub present: bool,
    /// The file parsed (a broken file keeps an enabled button).
    pub parsed: bool,
    /// Its `PlayedTime` `value`, seconds.
    pub played_time: Option<f64>,
}

/// A save's top-level elements; `Err` when TinyXML would refuse it.
pub fn parse(bytes: &[u8]) -> Result<Vec<Element>, xml::XmlError> {
    xml::parse(bytes)
}

/// An attribute as the bytes TinyXML hands back (entities decoded,
/// single-byte text).
pub fn attr_bytes(el: &Element, name: &str) -> Option<Vec<u8>> {
    el.attr(name).map(to_cp1252)
}

/// `sscanf(s, "%d")`: white space, a sign, digits; `None` when no digit
/// (the variable keeps its value).
pub fn scan_int(s: &[u8]) -> Option<i32> {
    let mut i = 0;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let neg = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut v: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add(i64::from(s[i] - b'0'));
        i += 1;
    }
    if i == start {
        return None;
    }
    let v = if neg { v.wrapping_neg() } else { v };
    Some(v as i32)
}

/// `sscanf(s, "%lf")`: the longest decimal prefix (white space, sign,
/// digits, point, digits, exponent); `None` when there is none.
pub fn scan_double(s: &[u8]) -> Option<f64> {
    let mut i = 0;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    if matches!(s.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let mut digits = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if s.get(i) == Some(&b'.') {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        let e0 = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        if j > e0 {
            i = j;
        }
    }
    std::str::from_utf8(&s[start..i]).ok()?.parse().ok()
}

/// `TeLuaContext::load` (@ 0x004c1aa0): the globals of a save's
/// top-level `Boolean` / `Number` / `String` elements, in file order.
/// A value `sscanf` cannot read keeps the previous element's value (the
/// locals @ `[esp+0x18]` / `[esp+0x1c]` are not reset; their value before
/// the first is `[UNKNOWN]`, 0 here); an element without `name` is left
/// out (the original reads a null pointer; no file the game writes has
/// one).
pub fn globals(elements: &[Element]) -> Vec<(Vec<u8>, LoadedValue)> {
    let mut out = Vec::new();
    let mut last_int = 0i32;
    let mut last_num = 0.0f64;
    for el in elements {
        let kind = el.name.as_str();
        if !matches!(kind, "Boolean" | "Number" | "String") {
            continue;
        }
        let Some(name) = attr_bytes(el, "name") else {
            log::warn!("save: a {kind} element without name, left out");
            continue;
        };
        let value = attr_bytes(el, "value");
        let v = match kind {
            "Boolean" => {
                if let Some(x) = value.as_deref().and_then(scan_int) {
                    last_int = x;
                }
                LoadedValue::Boolean(last_int != 0)
            }
            "Number" => {
                if let Some(x) = value.as_deref().and_then(scan_double) {
                    last_num = x;
                }
                LoadedValue::Number(last_num)
            }
            _ => match value {
                Some(s) => LoadedValue::String(s),
                None => LoadedValue::Nil,
            },
        };
        out.push((name, v));
    }
    out
}

/// The first top-level element named `name` (0x005082a0).
pub fn top<'a>(elements: &'a [Element], name: &str) -> Option<&'a Element> {
    elements.iter().find(|e| e.name == name)
}

/// `Orientation`'s `x` or `y` (0x00406000: a float, 0 when absent): a
/// value below −360 or above 360 becomes 0; ±360 and NaN stay
/// (@ 0x005beb74 / 0x005beb78).
pub fn angle(el: Option<&Element>, name: &str) -> f32 {
    let v = el
        .and_then(|e| e.attr(name))
        .and_then(|s| scan_double(s.as_bytes()))
        .unwrap_or(0.0) as f32;
    if v < -360.0 || v > 360.0 {
        0.0
    } else {
        v
    }
}

/// A slot of the loading menu read from the folder (@ 0x005185b0).
pub fn slot(folder: &BackupFolder, index: usize) -> SaveSlot {
    let file = format!("{index}.xml");
    let bytes = folder.read(&file);
    let parsed = bytes.as_deref().map(parse);
    let played_time = match &parsed {
        Some(Ok(els)) => top(els, "PlayedTime")
            .and_then(|e| e.attr("value"))
            .and_then(|v| scan_double(v.as_bytes())),
        _ => None,
    };
    if let Some(Err(e)) = &parsed {
        log::warn!(
            "LoadingMenu::SaveButton::SaveButton: Failed to load file \"Backup/{file}\": {e}"
        );
    }
    SaveSlot {
        index,
        present: bytes.is_some(),
        parsed: matches!(parsed, Some(Ok(_))),
        played_time,
        file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_expands_under_natives_folder() {
        let d = native_backup_dir(
            Path::new("/data/atlantis4"),
            "$(CSIDL_PERSONAL)/$(APPNAME)",
            "Atlantis Evolution",
        );
        assert_eq!(
            d,
            PathBuf::from("/data/atlantis4/Documents/Atlantis Evolution")
        );
        assert_eq!(file_name("Backup/3.xml"), "3.xml");
        assert_eq!(file_name("save.xml"), "save.xml");
    }

    #[test]
    fn text_mode_writes_crlf() {
        assert_eq!(to_disk(b"<a />\n<b />\n"), b"<a />\r\n<b />\r\n");
    }

    #[test]
    fn scanf_reads_prefixes() {
        assert_eq!(scan_int(b" 12x"), Some(12));
        assert_eq!(scan_int(b"-3"), Some(-3));
        assert_eq!(scan_int(b"x"), None);
        assert_eq!(scan_double(b"3.000000"), Some(3.0));
        assert_eq!(scan_double(b"-0.5e1z"), Some(-5.0));
        assert_eq!(scan_double(b"1e"), Some(1.0));
        assert_eq!(scan_double(b"."), None);
    }

    #[test]
    fn globals_load_in_file_order_with_the_originals_quirks() {
        let els = parse(
            b"<?xml version=\"1.0\" ?>\n<GameVersion major=\"1\" minor=\"0\" />\n\
              <Number name=\"a\" value=\"2.500000\" />\n<Number name=\"b\" />\n\
              <Boolean name=\"c\" value=\"1\" />\n<String name=\"d\" value=\"x&#x0A;&amp;\" />\n\
              <String name=\"e\" />\n<Boolean name=\"f\" value=\"z\" />\n",
        )
        .unwrap();
        let g = globals(&els);
        assert_eq!(
            g,
            vec![
                (b"a".to_vec(), LoadedValue::Number(2.5)),
                (b"b".to_vec(), LoadedValue::Number(2.5)),
                (b"c".to_vec(), LoadedValue::Boolean(true)),
                (b"d".to_vec(), LoadedValue::String(b"x\n&".to_vec())),
                (b"e".to_vec(), LoadedValue::Nil),
                (b"f".to_vec(), LoadedValue::Boolean(true)),
            ]
        );
    }

    #[test]
    fn angles_out_of_range_become_zero() {
        let els = parse(b"<Orientation x=\"360.000000\" y=\"-360.5\" />").unwrap();
        let o = top(&els, "Orientation");
        assert_eq!(angle(o, "x"), 360.0);
        assert_eq!(angle(o, "y"), 0.0);
        assert_eq!(angle(None, "x"), 0.0);
    }

    #[test]
    fn slots_read_the_played_time() {
        let mut f = BackupFolder::default();
        f.write("Backup/2.xml", b"<PlayedTime value=\"61.250000\" />\r\n");
        f.write("3.xml", b"<broken");
        let s = slot(&f, 2);
        assert!(s.present && s.parsed);
        assert_eq!(s.played_time, Some(61.25));
        let s = slot(&f, 3);
        assert!(s.present && !s.parsed);
        assert!(!slot(&f, 0).present);
    }
}
