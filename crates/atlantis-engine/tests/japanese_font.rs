//! The Japanese DVD's subtitle font (atlantis-dvd-ja-696832 asks GDI for
//! MS PGothic): every character of its chapter scripts in the bundled Noto
//! Sans JP; the player's `msgothic.ttc` (beside the game's files, or among
//! the system's fonts) used for its face MS PGothic.
//!
//! Needs a copy of the DVD's side B (`ATLANTIS_DVD_B_FILES`, the folder
//! holding `FACE_B`); the tests of MS PGothic also a `msgothic.ttc`, looked
//! for in `ATLANTIS_FONTS_DIR`, then in a Windows' font folder
//! (`%WINDIR%\Fonts`, or `/mnt/c/Windows/Fonts` under WSL). Each test says
//! what it skips.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis_data::dvd::{DvdFiles, LANGUAGES};
use atlantis_data::{FileSource, Game, StdDir, TextCodePage};
use atlantis_engine::subtitles::{
    font_in, pick_face, player_font, split_subtitle_cjk, CjkSplit, FaceSource, FontFace,
    SubtitleFont,
};

const CP: TextCodePage = TextCodePage::Windows932;

fn japanese_files() -> Option<DvdFiles> {
    let Some(dir) = std::env::var_os("ATLANTIS_DVD_B_FILES") else {
        eprintln!("ATLANTIS_DVD_B_FILES not set: skipped");
        return None;
    };
    let l = LANGUAGES.iter().find(|l| l.folder == "JAP").unwrap();
    Some(DvdFiles::new(Arc::new(StdDir::new(dir)), l).expect("side B"))
}

/// `msgothic.ttc`, from `ATLANTIS_FONTS_DIR` or a Windows' fonts.
fn msgothic() -> Option<Vec<u8>> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("ATLANTIS_FONTS_DIR")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    if let Some(w) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(w).join("Fonts"));
    }
    dirs.push(PathBuf::from("/mnt/c/Windows/Fonts"));
    let font = font_in(&dirs, "msgothic.ttc");
    if font.is_none() {
        eprintln!("no msgothic.ttc (ATLANTIS_FONTS_DIR, a Windows' fonts): skipped");
    }
    font
}

/// Every character of the Japanese folder's scripts (CHAPJ and CHAPI read
/// as Shift-JIS, and the rows the breaker cuts) has a glyph in the bundled
/// Noto Sans JP.
#[test]
fn japanese_scripts_in_bundled_font() {
    let Some(files) = japanese_files() else {
        return;
    };
    let face = FontFace::bundled(CP);
    assert_eq!(face.source(), FaceSource::Bundled);
    let split = CjkSplit::of_build(&atlantis_data::exe::DVD_JA, false).unwrap();
    let names: Vec<String> = files
        .list("SCENAR")
        .into_iter()
        .filter(|n| n.to_ascii_uppercase().ends_with(".CON"))
        .collect();
    assert!(names.len() > 140, "{} scripts", names.len());
    let mut chars = std::collections::BTreeSet::new();
    for n in &names {
        let name = format!("SCENAR\\{n}");
        let size = files.size(&name).unwrap() as usize;
        let data = files.read_at(&name, 0, size).unwrap();
        chars.extend(CP.decode(&data).chars());
        for line in data.split(|&c| c == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let Some(close) = line.iter().position(|&c| c == b']') else {
                continue;
            };
            let text = &line[close + 1..];
            for row in split_subtitle_cjk(text.strip_prefix(b" ").unwrap_or(text), split) {
                chars.extend(CP.decode(&row).chars());
            }
        }
    }
    let missing: Vec<char> = chars
        .iter()
        .copied()
        .filter(|&c| c >= ' ' && !('\u{e000}'..='\u{f8ff}').contains(&c))
        .filter(|&c| !face.has_char(c))
        .collect();
    assert!(missing.is_empty(), "no glyph for {missing:?}");
    let kana_kanji = chars.iter().filter(|&&c| c >= '\u{3040}').count();
    assert!(kana_kanji > 1000, "{kana_kanji} characters");
}

/// MS PGothic of `msgothic.ttc` (a collection: MS Gothic, MS UI Gothic,
/// MS PGothic): the face the player's file gives, drawn at 16 pixels per
/// em, the full-width characters one em wide, the proportional kana and
/// Latin letters narrower.
#[test]
fn ms_pgothic_of_the_players_file() {
    let Some(data) = msgothic() else { return };
    let (name, family) = player_font(CP).unwrap();
    assert_eq!((name, family), ("msgothic.ttc", "MS PGothic"));
    let index = pick_face(&data, family).unwrap();
    let face = FontFace::player(CP, data).expect("MS PGothic usable");
    assert_eq!(face.source(), FaceSource::Player);
    assert!(face.has_char('ゴ') && face.has_char('日'));
    let font = SubtitleFont::with_face(CP, face);
    // 日本 (93 FA 96 7B): two full-width ideographs.
    assert_eq!(font.width(b"\x93\xfa\x96\x7b"), 32, "face {index}");
    // ii: proportional Latin letters, much narrower than 16 px each.
    assert!(font.width(b"ii") < 16);
    let mut screen = vec![0u16; 640 * 480];
    font.draw(
        &mut screen,
        640,
        480,
        b"\x93\xfa\x96\x7b",
        5,
        400,
        630,
        0xffff,
    );
    assert!(screen.iter().filter(|&&p| p == 0xffff).count() > 30);
}

/// The player's `msgothic.ttc` in the Japanese folder (a copy of the side)
/// is read beside the game's files.
#[test]
fn msgothic_beside_the_japanese_folder() {
    let Some(dir) = std::env::var_os("ATLANTIS_DVD_B_FILES").map(PathBuf::from) else {
        eprintln!("ATLANTIS_DVD_B_FILES not set: skipped");
        return;
    };
    // A side of links, with the font in FACE_B/JAP.
    let side = std::env::temp_dir().join(format!("atlantis-ja-font-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&side);
    let face_b = side.join("FACE_B");
    std::fs::create_dir_all(face_b.join("JAP")).unwrap();
    for e in std::fs::read_dir(dir.join("FACE_B")).unwrap().flatten() {
        let n = e.file_name();
        let to = if n == "JAP" {
            for f in std::fs::read_dir(e.path()).unwrap().flatten() {
                link(&f.path(), &face_b.join("JAP").join(f.file_name()));
            }
            continue;
        } else {
            face_b.join(&n)
        };
        link(&e.path(), &to);
    }
    let font = b"ttcf\0\x02\0\0".to_vec();
    std::fs::write(face_b.join("JAP").join("MSGOTHIC.TTC"), &font).unwrap();
    let l = LANGUAGES.iter().find(|l| l.folder == "JAP").unwrap();
    let files = DvdFiles::new(Arc::new(StdDir::new(&side)), l).expect("the side");
    assert_eq!(files.read_beside("msgothic.ttc"), Some(font));
    let g = Game::open(Arc::new(files)).expect("the Japanese DVD");
    assert_eq!(g.exe.build.text_code_page, CP);
    let _ = std::fs::remove_dir_all(&side);
}

fn link(from: &std::path::Path, to: &std::path::Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(from, to).unwrap();
    #[cfg(windows)]
    {
        if from.is_dir() {
            std::os::windows::fs::symlink_dir(from, to).unwrap();
        } else {
            std::os::windows::fs::symlink_file(from, to).unwrap();
        }
    }
}
