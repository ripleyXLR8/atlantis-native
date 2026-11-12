//! *Atlantis Evolution*'s 2004 program (the retail CD of The Adventure
//! Company), which is *Atlantis III*'s engine grown (atlantis4-re decisions
//! 0007 and 0008): which build a folder holds, and where each of its data
//! files is.
//!
//! The CD's installer copies the four discs to the hard disk, one folder per
//! disc (`cd0` .. `cd4`, each with its `datas\`), and writes the install type
//! `FLA` in the registry (`HKLM\SOFTWARE\Atlantis Interactive\Atlantis
//! Evolution`, `Install`). With that type the program builds every data path
//! from the folder the file belongs to: the disc-0 folders under `cd0\`, the
//! per-disc ones under the current disc's `cd<N>\`, which `Menu_TestChangeCD`
//! sets (atlantis4-re `subsystems/cd2004-files.md`). Addresses are
//! atlantis4-cd-en-3022848's.

use md5::{Digest, Md5};

use crate::exe::pe_timestamp;

/// The file name of the program.
pub const EXE_NAME: &str = "Atlantis Evolution.exe";

/// A known build of the 2004 program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    /// `atlantis4-<medium>-<language>-<size>`.
    pub id: &'static str,
    /// The edition, for people.
    pub label: &'static str,
    pub size: u64,
    /// The PE header's `TimeDateStamp`.
    pub pe_timestamp: u32,
    /// MD5 of the whole file.
    pub md5: [u8; 16],
}

/// The builds this port knows: the North American CD's, which the European
/// CD ships too (the same `Data1.cab`).
pub const KNOWN_BUILDS: [Build; 1] = [Build {
    id: "atlantis4-cd-en-3022848",
    label: "Atlantis Evolution (2004), CD",
    size: 3_022_848,
    // 2004-09-28 09:51:02 UTC.
    pe_timestamp: 0x4159_17e6,
    md5: [
        0x21, 0x99, 0xbc, 0x93, 0xd2, 0xac, 0x3f, 0x8b, 0x42, 0x79, 0xe0, 0xdb, 0x89, 0x85, 0x59,
        0xe8,
    ],
}];

/// The two sprite libraries shipped beside the program (2002-05-14), the
/// same files as *Atlantis V*'s: name, size, PE timestamp, MD5.
pub const SPR_DLLS: [(&str, u64, u32, [u8; 16]); 2] = [
    (
        "spr_p5.dll",
        155_648,
        0x3ce1_3047,
        [
            0xdf, 0xf1, 0x0a, 0xb7, 0xa8, 0xb3, 0xc8, 0xa3, 0x55, 0x3b, 0x11, 0x65, 0xf5, 0x8b,
            0x76, 0x6e,
        ],
    ),
    (
        "spr_p6.dll",
        155_648,
        0x3ce1_33b7,
        [
            0xd8, 0x89, 0x44, 0x3f, 0xf5, 0xe4, 0x52, 0xe8, 0x13, 0x2e, 0x3f, 0x4b, 0xb2, 0xd6,
            0x13, 0x5c,
        ],
    ),
];

/// Why a file is not a known build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExeError {
    NotPe,
    Unknown {
        size: u64,
        pe_timestamp: u32,
        md5: String,
    },
}

impl std::fmt::Display for ExeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExeError::NotPe => write!(f, "the game's executable is not a Windows executable"),
            ExeError::Unknown {
                size,
                pe_timestamp,
                md5,
            } => write!(
                f,
                "the game's executable is not a known build of Atlantis Evolution (2004) ({size} bytes, PE timestamp {pe_timestamp:#010x}, MD5 {md5})"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Which known build `exe` is (size, PE timestamp, then MD5).
pub fn identify(exe: &[u8]) -> Result<&'static Build, ExeError> {
    let stamp = pe_timestamp(exe).ok_or(ExeError::NotPe)?;
    let size = exe.len() as u64;
    let md5: [u8; 16] = Md5::digest(exe).into();
    KNOWN_BUILDS
        .iter()
        .find(|b| b.size == size && b.pe_timestamp == stamp && b.md5 == md5)
        .ok_or(ExeError::Unknown {
            size,
            pe_timestamp: stamp,
            md5: hex(&md5),
        })
}

/// A data folder, as one of the program's path builders names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    // The disc-0 folders.
    /// `wam\` (FUN_00417340).
    Wam,
    /// `sfx\` (FUN_004177e0).
    Sfx,
    /// `scenar\` (FUN_00419080).
    Scenar,
    /// `spr\` (FUN_00418d50).
    Spr,
    /// `menu\desc\` (FUN_00418a20).
    MenuDesc,
    /// `menu\spr_` (FUN_00419240, FUN_00419510): the name follows directly.
    MenuSpr,
    /// `loc\voix\<language>\` (FUN_00417b10, FUN_00417dc0).
    Voices,
    /// `loc\text\<language>\` (FUN_00419880, FUN_00419b30).
    Texts,
    // The per-disc folders.
    /// `warp\` (FUN_00417670).
    Warp,
    /// `music\` (FUN_004179a0).
    Music,
    /// `spw\` (FUN_00418070).
    Spw,
    /// `bink\` (FUN_004181e0, FUN_004185d0).
    Bink,
    /// `loc\bink\<language>\` (FUN_00418350).
    LocalBink,
    /// `binktraj\` (FUN_00418740).
    BinkTraj,
    /// `binkdial\` (FUN_004188b0).
    BinkDial,
    /// `map\` (FUN_00418be0).
    Map,
    /// `spf\` (FUN_00418f10).
    Spf,
    /// `puzzles\` (FUN_00417500).
    Puzzles,
}

impl Folder {
    /// Whether the folder is on disc 0 (`cd0\`), not the current disc's.
    pub fn on_disc0(self) -> bool {
        matches!(
            self,
            Folder::Wam
                | Folder::Sfx
                | Folder::Scenar
                | Folder::Spr
                | Folder::MenuDesc
                | Folder::MenuSpr
                | Folder::Voices
                | Folder::Texts
        )
    }

    /// The folder's text under `Datas\` (with the language for the `loc`
    /// folders: the registry's code, `gbr` for the English CD).
    fn text(self, language: &str) -> String {
        let fixed = match self {
            Folder::Wam => "wam\\",
            Folder::Sfx => "sfx\\",
            Folder::Scenar => "scenar\\",
            Folder::Spr => "spr\\",
            Folder::MenuDesc => "menu\\desc\\",
            Folder::MenuSpr => "menu\\spr_",
            Folder::Warp => "warp\\",
            Folder::Music => "music\\",
            Folder::Spw => "spw\\",
            Folder::Bink => "bink\\",
            Folder::BinkTraj => "binktraj\\",
            Folder::BinkDial => "binkdial\\",
            Folder::Map => "map\\",
            Folder::Spf => "spf\\",
            Folder::Puzzles => "puzzles\\",
            Folder::Voices => return format!("loc\\voix\\{language}\\"),
            Folder::Texts => return format!("loc\\text\\{language}\\"),
            Folder::LocalBink => return format!("loc\\bink\\{language}\\"),
        };
        fixed.to_string()
    }
}

/// The path of `name` in `folder` for an `FLA` install, the current disc
/// being `disc` (`Menu_TestChangeCD`'s number): `cd0\Datas\...` for a disc-0
/// folder, `cd<disc>\Datas\...` for the others, relative to the program's
/// folder.
pub fn data_path(folder: Folder, name: &str, disc: u32, language: &str) -> String {
    let root = if folder.on_disc0() {
        "cd0\\".to_string()
    } else {
        format!("cd{disc}\\")
    };
    format!("{root}Datas\\{}{name}", folder.text(language))
}

/// The folder a path under `Datas\` belongs to, by its first components
/// (`loc\<voix|text|bink>\<language>\`); `None` for a file of `Datas\`
/// itself or of a folder no path builder names.
pub fn folder_of(rest: &str) -> Option<Folder> {
    let parts: Vec<String> = rest
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase())
        .collect();
    let first = parts.first()?;
    if parts.len() < 2 {
        return None;
    }
    Some(match first.as_str() {
        "wam" => Folder::Wam,
        "sfx" => Folder::Sfx,
        "scenar" => Folder::Scenar,
        "spr" => Folder::Spr,
        // `menu\desc\` and `menu\spr_<name>`: both on disc 0.
        "menu" => Folder::MenuDesc,
        "warp" => Folder::Warp,
        "music" => Folder::Music,
        "spw" => Folder::Spw,
        "bink" => Folder::Bink,
        "binktraj" => Folder::BinkTraj,
        "binkdial" => Folder::BinkDial,
        "map" => Folder::Map,
        "spf" => Folder::Spf,
        "puzzles" => Folder::Puzzles,
        "loc" => match parts.get(1).map(String::as_str) {
            Some("voix") => Folder::Voices,
            Some("text") => Folder::Texts,
            Some("bink") => Folder::LocalBink,
            _ => return None,
        },
        _ => return None,
    })
}

/// The install of the 2004 program seen as one game folder: a name under
/// `Datas\` goes to `cd0\Datas\` or to the current disc's `cd<N>\Datas\`
/// by its folder ([`folder_of`], [`Folder::on_disc0`]), a file of `Datas\`
/// itself to the install's `datas\` (`Atlantis4.cfg`), any other name is
/// unchanged. The current disc is `Menu_TestChangeCD`'s number: 1 at
/// start-up for an `FLA` install (@ 0x004a1c24), then `CHANGECD=<n>` of the
/// scripts (@ 0x0045091a) or a loaded game (@ 0x00461b06).
pub struct Evolution2004Source {
    src: std::sync::Arc<dyn atlantis_data::FileSource>,
    disc: std::sync::atomic::AtomicU32,
}

impl Evolution2004Source {
    pub fn new(src: std::sync::Arc<dyn atlantis_data::FileSource>) -> Evolution2004Source {
        Evolution2004Source {
            src,
            disc: std::sync::atomic::AtomicU32::new(1),
        }
    }

    /// The current disc.
    pub fn disc(&self) -> u32 {
        self.disc.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// `Menu_TestChangeCD(n)` for an `FLA` install: the per-disc folders are
    /// read from `cd<n>\` from now on.
    pub fn set_disc(&self, n: u32) {
        self.disc.store(n, std::sync::atomic::Ordering::Relaxed);
    }

    /// Where `name` is in the install.
    pub fn map(&self, name: &str) -> String {
        let n = name.replace('\\', "/");
        let Some(rest) = n
            .get(..6)
            .filter(|d| d.eq_ignore_ascii_case("datas/"))
            .map(|_| &n[6..])
        else {
            return n;
        };
        match folder_of(rest) {
            Some(f) if f.on_disc0() => format!("cd0/datas/{rest}"),
            Some(_) => format!("cd{}/datas/{rest}", self.disc()),
            None => format!("datas/{rest}"),
        }
    }
}

impl atlantis_data::FileSource for Evolution2004Source {
    fn size(&self, name: &str) -> Option<u64> {
        self.src.size(&self.map(name))
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.src.read_at(&self.map(name), offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.src.has_dir(&self.map(name))
    }
    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.src.read_beside(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        self.src.list(&self.map(dir))
    }
}

/// Whether `dir` looks like the 2004 program's install: the program and
/// `cd0\datas\scenar`.
pub fn holds_evolution_2004(dir: &std::path::Path) -> bool {
    use atlantis_data::FileSource;
    let src = atlantis_data::StdDir::new(dir);
    src.size(EXE_NAME).is_some() && src.has_dir("cd0/datas/scenar")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_builds() {
        assert_eq!(
            hex(&KNOWN_BUILDS[0].md5),
            "2199bc93d2ac3f8b4279e0db898559e8"
        );
        assert_eq!(hex(&SPR_DLLS[0].3), "dff10ab7a8b3c8a3553b1165f58b766e");
        assert_eq!(hex(&SPR_DLLS[1].3), "d889443ff5e452e8132e3f4bb2d6135c");
        assert_eq!(identify(b"MZ"), Err(ExeError::NotPe));
    }

    #[test]
    fn paths_by_folder_and_disc() {
        assert_eq!(
            data_path(Folder::Scenar, "chapi038.con", 3, "gbr"),
            "cd0\\Datas\\scenar\\chapi038.con"
        );
        assert_eq!(
            data_path(Folder::Warp, "c20004.jp6", 1, "gbr"),
            "cd1\\Datas\\warp\\c20004.jp6"
        );
        assert_eq!(
            data_path(Folder::LocalBink, "x.bik", 2, "gbr"),
            "cd2\\Datas\\loc\\bink\\gbr\\x.bik"
        );
        assert_eq!(
            data_path(Folder::Voices, "a.ogg", 4, "gbr"),
            "cd0\\Datas\\loc\\voix\\gbr\\a.ogg"
        );
        assert_eq!(
            data_path(Folder::MenuSpr, "x.spr", 1, "gbr"),
            "cd0\\Datas\\menu\\spr_x.spr"
        );
    }

    #[test]
    fn names_map_to_the_discs() {
        let m = atlantis_data::files::MemFiles::default();
        let src = Evolution2004Source::new(std::sync::Arc::new(m));
        assert_eq!(
            src.map("Datas/scenar/chapi038.con"),
            "cd0/datas/scenar/chapi038.con"
        );
        assert_eq!(
            src.map("Datas\\warp\\c20004.jp6"),
            "cd1/datas/warp/c20004.jp6"
        );
        src.set_disc(3);
        assert_eq!(
            src.map("datas/loc/bink/gbr/x.bik"),
            "cd3/datas/loc/bink/gbr/x.bik"
        );
        assert_eq!(
            src.map("datas/loc/voix/gbr/a.ogg"),
            "cd0/datas/loc/voix/gbr/a.ogg"
        );
        assert_eq!(src.map("Datas/Atlantis4.cfg"), "datas/Atlantis4.cfg");
        assert_eq!(src.map("Atlantis Evolution.exe"), "Atlantis Evolution.exe");
    }

    /// The CD's install (`ATLANTIS4_CD`): the build is known, and every data
    /// file sits where the rule puts it, the disc-0 folders under `cd0` only
    /// and the others under `cd1`..`cd4` only.
    #[test]
    fn the_cd_install_follows_the_rule() {
        let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
            crate::testing::missing("ATLANTIS4_CD not set");
            return;
        };
        assert!(holds_evolution_2004(&dir));
        let exe = std::fs::read(dir.join(EXE_NAME)).unwrap();
        assert_eq!(identify(&exe).unwrap().id, "atlantis4-cd-en-3022848");
        for (name, size, stamp, md5) in SPR_DLLS {
            let d = std::fs::read(dir.join(name)).unwrap();
            let m: [u8; 16] = Md5::digest(&d).into();
            assert_eq!(
                (d.len() as u64, pe_timestamp(&d), m),
                (size, Some(stamp), md5),
                "{name}"
            );
        }
        let folders = [
            (Folder::Wam, "wam"),
            (Folder::Sfx, "sfx"),
            (Folder::Scenar, "scenar"),
            (Folder::Spr, "spr"),
            (Folder::Warp, "warp"),
            (Folder::Music, "music"),
            (Folder::Spw, "spw"),
            (Folder::Bink, "bink"),
            (Folder::BinkTraj, "binktraj"),
            (Folder::BinkDial, "binkdial"),
            (Folder::Map, "map"),
            (Folder::Spf, "spf"),
            (Folder::Puzzles, "puzzles"),
        ];
        for disc in 0..=4u32 {
            let datas = dir.join(format!("cd{disc}")).join("datas");
            for e in std::fs::read_dir(&datas).unwrap().flatten() {
                let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                if n == "menu" || n == "loc" {
                    continue;
                }
                let (f, _) = folders
                    .iter()
                    .find(|(_, t)| *t == n)
                    .unwrap_or_else(|| panic!("cd{disc}: {n}"));
                assert_eq!(f.on_disc0(), disc == 0, "cd{disc}: {n}");
            }
        }
        // Every data file of the install, read through the source by its
        // `Datas\` name with its disc current.
        let src = Evolution2004Source::new(std::sync::Arc::new(atlantis_data::StdDir::new(&dir)));
        let mut n = 0;
        for disc in 0..=4u32 {
            let datas = dir.join(format!("cd{disc}")).join("datas");
            let mut stack = vec![datas.clone()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                        continue;
                    }
                    let rel = p
                        .strip_prefix(&datas)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    if disc > 0 {
                        src.set_disc(disc);
                    }
                    let name = format!("Datas/{rel}");
                    assert_eq!(
                        atlantis_data::FileSource::size(&src, &name),
                        Some(e.metadata().unwrap().len()),
                        "{name} (disc {disc})"
                    );
                    n += 1;
                }
            }
        }
        assert_eq!(n, 7629 - 10, "data files");
        // `loc`: text and voices on disc 0, the localised videos elsewhere.
        assert!(dir.join("cd0/datas/loc/text/gbr").is_dir());
        assert!(dir.join("cd0/datas/loc/voix/gbr").is_dir());
        assert!(dir.join("cd1/datas/loc/bink/gbr").is_dir());
    }
}
