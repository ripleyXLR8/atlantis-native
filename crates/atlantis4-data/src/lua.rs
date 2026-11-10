//! The Lua scripts: an inventory and a loader, not an interpreter.
//!
//! The executable links Lua 5.1.4 (`$Lua: Lua 5.1.4 ...`, file +0x1ae4d0)
//! and every one of the 504 `.lua` files of the GOG.com install is source
//! text, none compiled (no file starts with the bytecode signature
//! `1b 4c 75 61`), in Windows-1252 with CRLF ends [CONFIRMED-DATA]. They
//! are of a few kinds ([`ScriptKind`]):
//!
//! - a scene's script `3D/<zone>/<node>.lua`, beside its `.xml`, loaded per
//!   scene (strings `xml`, `lua`, `warpName`, `entering %s` at file
//!   +0x1b7aa4..+0x1b7ac8) and answering the engine's callbacks
//!   (`OnWarpEnter`, `OnWarpLeave`, `OnWarpObjectHit`, ...);
//! - `3D/OnGameEnter.lua` (file +0x1b7be0), global helpers;
//! - the boot variables of `Texts/` (`versionString`, `applicationTitle`,
//!   `firstWarpPath` read through file +0x1b8a58, `backupPathTemplate`);
//! - `TeLuaGUI` layouts: a table constructor per file
//!   (`TeButtonLayout { ... }`), in `GUI/` and the menus' folders;
//! - the empty selectors of `Versions/`.
//!
//! What this module gives the engine to come: the source bytes, the kind,
//! and a Lua 5.1 lexer good enough to list the functions a script defines
//! and its plain `name = "string"` statements, for the inventory and the
//! tests. Running the scripts is the Lua runtime's work (not written yet).

/// What a script is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScriptKind {
    /// `3D/<zone>/<node>.lua`.
    Scene,
    /// `3D/OnGameEnter.lua`.
    GameEnter,
    /// `Texts/**.lua`: boot variables.
    Boot,
    /// `Versions/**.lua`: version selectors.
    Version,
    /// A `TeLuaGUI` layout: the file calls a `Te*` constructor with a table
    /// at its top level (`TeLayout { ... }`), possibly after variables.
    Layout,
    /// Anything else (inventory, documents, diary, menus' code).
    Other,
}

/// The kind of the script `name` (`/`-separated, relative to the
/// install) of source `src`.
pub fn kind(name: &str, src: &[u8]) -> ScriptKind {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    match parts.as_slice() {
        ["3d", "ongameenter.lua"] => ScriptKind::GameEnter,
        ["3d", _, node]
            if node
                .trim_end_matches(".lua")
                .bytes()
                .all(|c| c.is_ascii_digit()) =>
        {
            ScriptKind::Scene
        }
        ["texts", ..] => ScriptKind::Boot,
        ["versions", ..] => ScriptKind::Version,
        _ if holds_layout(src) => ScriptKind::Layout,
        _ => ScriptKind::Other,
    }
}

/// Whether `src` calls a `Te*` constructor with a table at its top level
/// (`TeLayout { ... }`), as `TeLuaGUI` layouts do.
pub fn holds_layout(src: &[u8]) -> bool {
    let toks = tokens(src);
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate() {
        match t {
            Token::Sym(b'(' | b'{' | b'[') => depth += 1,
            Token::Sym(b')' | b'}' | b']') => depth -= 1,
            Token::Name(n)
                if depth == 0
                    && n.starts_with("Te")
                    && toks.get(i + 1) == Some(&Token::Sym(b'{')) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Whether `src` is compiled Lua (the bytecode signature `\x1bLua`).
pub fn is_bytecode(src: &[u8]) -> bool {
    src.starts_with(b"\x1bLua")
}

/// A token of Lua 5.1 source, as far as the inventory needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// A name or a keyword.
    Name(String),
    /// A string literal, escapes decoded, bytes as Windows-1252.
    Str(String),
    /// A number literal, as written.
    Number(String),
    /// A one-byte symbol (`=`, `(`, `.`, `{`, ...); the two- and
    /// three-byte operators come as their bytes one by one.
    Sym(u8),
}

fn long_bracket_level(s: &[u8], at: usize) -> Option<usize> {
    // At '[': `[` `=`* `[`.
    let mut i = at + 1;
    while s.get(i) == Some(&b'=') {
        i += 1;
    }
    (s.get(i) == Some(&b'[')).then_some(i - at - 1)
}

/// The end (past the closing bracket) of a long bracket of `level` whose
/// content starts at `at`, or the end of the source.
fn long_bracket_end(s: &[u8], at: usize, level: usize) -> (usize, usize) {
    let mut i = at;
    while i < s.len() {
        if s[i] == b']' {
            let mut j = i + 1;
            while s.get(j) == Some(&b'=') {
                j += 1;
            }
            if j - i - 1 == level && s.get(j) == Some(&b']') {
                return (i, j + 1);
            }
        }
        i += 1;
    }
    (s.len(), s.len())
}

/// The tokens of `src`, comments dropped. Malformed input ends the list
/// where the lexer cannot go on (an unterminated string runs to the end).
pub fn tokens(src: &[u8]) -> Vec<Token> {
    let s = src;
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if s[i..].starts_with(b"--") {
            i += 2;
            if s.get(i) == Some(&b'[') {
                if let Some(level) = long_bracket_level(s, i) {
                    i = long_bracket_end(s, i + level + 2, level).1;
                    continue;
                }
            }
            while i < s.len() && s[i] != b'\n' {
                i += 1;
            }
        } else if c == b'[' && long_bracket_level(s, i).is_some() {
            let level = long_bracket_level(s, i).unwrap_or(0);
            let mut start = i + level + 2;
            // A newline right after the opening bracket is skipped.
            if s.get(start) == Some(&b'\r') {
                start += 1;
            }
            if s.get(start) == Some(&b'\n') {
                start += 1;
            }
            let (end, next) = long_bracket_end(s, start, level);
            out.push(Token::Str(crate::xml::cp1252(&s[start..end])));
            i = next;
        } else if c == b'"' || c == b'\'' {
            let mut v = Vec::new();
            i += 1;
            while i < s.len() && s[i] != c {
                if s[i] == b'\\' && i + 1 < s.len() {
                    i += 1;
                    let e = s[i];
                    match e {
                        b'n' => v.push(b'\n'),
                        b't' => v.push(b'\t'),
                        b'r' => v.push(b'\r'),
                        b'a' => v.push(7),
                        b'b' => v.push(8),
                        b'f' => v.push(12),
                        b'v' => v.push(11),
                        b'0'..=b'9' => {
                            let mut n = 0u32;
                            let mut k = 0;
                            while k < 3 && i < s.len() && s[i].is_ascii_digit() {
                                n = n * 10 + u32::from(s[i] - b'0');
                                i += 1;
                                k += 1;
                            }
                            v.push(n as u8);
                            continue;
                        }
                        _ => v.push(e),
                    }
                    i += 1;
                } else {
                    v.push(s[i]);
                    i += 1;
                }
            }
            i += 1;
            out.push(Token::Str(crate::xml::cp1252(&v)));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'_') {
                i += 1;
            }
            out.push(Token::Name(crate::xml::cp1252(&s[start..i])));
        } else if c.is_ascii_digit() || (c == b'.' && s.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let start = i;
            while i < s.len()
                && (s[i].is_ascii_alphanumeric()
                    || s[i] == b'.'
                    || ((s[i] == b'+' || s[i] == b'-') && matches!(s[i - 1], b'e' | b'E')))
            {
                i += 1;
            }
            out.push(Token::Number(crate::xml::cp1252(&s[start..i])));
        } else {
            out.push(Token::Sym(c));
            i += 1;
        }
    }
    out
}

/// The names of the functions `src` defines with `function <name>`
/// (`a.b` and `a:b` names written with their separators), in order.
pub fn functions_defined(src: &[u8]) -> Vec<String> {
    let toks = tokens(src);
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if *t != Token::Name("function".into()) {
            continue;
        }
        let mut name = String::new();
        let mut j = i + 1;
        while let Some(Token::Name(n)) = toks.get(j) {
            name.push_str(n);
            match toks.get(j + 1) {
                Some(Token::Sym(sep @ (b'.' | b':'))) => {
                    name.push(char::from(*sep));
                    j += 2;
                }
                _ => break,
            }
        }
        if !name.is_empty() {
            out.push(name);
        }
    }
    out
}

/// The statements `name = "string"` of `src` whose name stands at the
/// start of a statement (after another statement's end, not inside a
/// table or a call), in order: the boot files' variables.
pub fn string_assignments(src: &[u8]) -> Vec<(String, String)> {
    let toks = tokens(src);
    let mut out = Vec::new();
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate() {
        match t {
            Token::Sym(b'(' | b'{' | b'[') => depth += 1,
            Token::Sym(b')' | b'}' | b']') => depth -= 1,
            Token::Name(n) if depth == 0 => {
                if let (Some(Token::Sym(b'=')), Some(Token::Str(v))) =
                    (toks.get(i + 1), toks.get(i + 2))
                {
                    let after_dot = i > 0 && matches!(toks[i - 1], Token::Sym(b'.'));
                    if !after_dot {
                        out.push((n.clone(), v.clone()));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_strings_hide_their_words() {
        let src = b"-- function NotOne()\r\n--[==[ function NotTwo() ]==]\r\nfunction OnWarpEnter()\r\n  x = \"function NotThree()\"\r\nend\r\nfunction a.b:c() end\r\n";
        assert_eq!(functions_defined(src), ["OnWarpEnter", "a.b:c"]);
    }

    #[test]
    fn boot_variables_are_found() {
        let src = b"--firstWarpPath = \"3D/Old/0001.wp\"\r\nfirstWarpPath = \"3D/Bateau_CA/0001.wp\"\r\nt = { k = \"no\" }\r\n";
        assert_eq!(
            string_assignments(src),
            [(
                "firstWarpPath".to_string(),
                "3D/Bateau_CA/0001.wp".to_string()
            )]
        );
    }

    #[test]
    fn escapes_and_long_strings_decode() {
        let toks = tokens(b"s = 'a\\'b\\65' .. [[\nline]] .. 0x10 .. 1e-3");
        assert_eq!(toks[2], Token::Str("a'bA".into()));
        assert_eq!(toks[5], Token::Str("line".into()));
        assert_eq!(toks[8], Token::Number("0x10".into()));
        assert_eq!(toks[11], Token::Number("1e-3".into()));
    }

    #[test]
    fn kinds_follow_the_folders() {
        assert_eq!(kind("3D/TA/0034.lua", b""), ScriptKind::Scene);
        assert_eq!(kind("3D/OnGameEnter.lua", b""), ScriptKind::GameEnter);
        assert_eq!(kind("Texts/Version.lua", b""), ScriptKind::Boot);
        assert_eq!(
            kind("GUI/HD/sujet1.lua", b"TeButtonLayout {\r\n}"),
            ScriptKind::Layout
        );
        assert_eq!(
            kind("Inventory/Inventory.lua", b"function f() end"),
            ScriptKind::Other
        );
        assert!(is_bytecode(b"\x1bLua\x51"));
    }
}
