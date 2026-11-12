//! `Datas\Atlantis4.cfg` of *Atlantis Evolution*'s 2004 program: the
//! options, `Key = value` lines. Addresses are atlantis4-cd-en-3022848's.
//!
//! The reader (@ 0x0040a720) is *Atlantis III*'s ([`crate::cfg`]) in its
//! grammar: the defaults first (@ 0x0040cfa0), then line by line
//! (@ 0x00449d70: separators skipped, then up to a CR or an LF; the
//! separators of @ 0x0044ade0 are *Atlantis III*'s), `//` lines skipped,
//! the first word the key (case-sensitive), the next word skipped whatever
//! it is (@ 0x00449c40, the `=`), the value after it; `Time` and `Puzzles`
//! read their numbers with no word skipped. It differs in its keys and
//! defaults:
//!
//! - `Resolution` defaults to 1024 x 768 (@ 0x0040cfc4);
//! - `WarpRes` defaults to 768 (0x300, @ 0x0040d014; 1024 when `Version`
//!   is `DVD`, which it never is when the defaults are set) and accepts
//!   512, 768 and 1024 (@ 0x0040b5dc..0x0040b5ef);
//! - the warp view is always 800 x 600 (@ 0x0040b65d, and @ 0x0040a7c4
//!   before the file);
//! - `GlobalVolume` and `MusicVolume1` (not *Atlantis III*'s
//!   `MusicVolume`) are floats, read with `atof` (@ 0x004492a0) and kept in
//!   0 .. 99 (@ 0x0040bf24, 0x0040c168), default 99.0; `SFXVolume` and
//!   `VoicesVolume` are integers 0 .. 99, default 99;
//! - `Puzzles` holds eight integers (0x00719b58 .. 0x00719b74, @ 0x0040b4b9),
//!   default 0.
//!
//! What start-up then takes from the registry (`Install`, `Language`,
//! `LanguageST`, atlantis4-re `subsystems/cd2004-files.md`) is not modelled
//! here.

use crate::atl::{atoi, TextCursor};
use crate::cfg::{PixelFormat, RESOLUTIONS};
use crate::tokens::atof;

/// The file in the install (`Atlantis4.cfg` @ 0x006ac958 under `Datas\`).
pub const CFG_PATH: &str = "Datas/Atlantis4.cfg";

/// The `WarpRes` values the reader takes without a message.
pub const WARP_RES: [i32; 3] = [512, 768, 1024];

/// A bad value the reader logs (the option keeps its value).
#[derive(Clone, Debug, PartialEq)]
pub enum CfgWarning {
    Resolution(i32, i32),
    PixelFormat(Vec<u8>),
    Bool(&'static str, Vec<u8>),
    WarpRes(i32),
}

/// The options, with the globals they set in the original.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// `Resolution w h` (0x006e70b8, 0x006e6fbc). Default 1024 x 768.
    pub resolution: (i32, i32),
    /// `PixelFormat` (7 dwords @ 0x006e70e8 from the format table, the
    /// codes of *Atlantis III*). Default r5g6b5 (code 0x8a).
    pub pixel_format: PixelFormat,
    /// `FullScreen` (0x006e6fc0). Default true.
    pub full_screen: bool,
    /// `Langue` (0x00719b78; its folder `Langue\` 0x00719b28). Default
    /// empty.
    pub langue: Vec<u8>,
    /// `SousTitre` (0x00719b40; its folder 0x00719ba8). Default empty.
    pub sous_titre: Vec<u8>,
    /// `Version` (0x00719b10): the install type, which start-up then reads
    /// from the registry. Default empty.
    pub version: Vec<u8>,
    /// `Install` (0x00719ca0). Default `NORM` (@ 0x0040cff2).
    pub install: Vec<u8>,
    /// `Time a b c d e` (0x00719c44 .. 0x00719c54). Default 0.
    pub time: [i32; 5],
    /// `Puzzles` (0x00719b58 .. 0x00719b74), eight integers. Default 0.
    pub puzzles: [i32; 8],
    /// `WarpRes` (0x00719d6c). Default 768.
    pub warp_res: i32,
    /// The warp view (0x006acfa0, 0x006acfa4): 800 x 600.
    pub view: (i32, i32),
    /// `Smooth` (0x006e7114). Default true.
    pub smooth: bool,
    /// `Cursor` (0x006af054). Default true.
    pub cursor: bool,
    /// `InvertMouseY` (0x006e9fec). Default false.
    pub invert_mouse_y: bool,
    /// `SubTitleOn` (0x00719c08). Default true.
    pub subtitle_on: bool,
    /// `GlobalVolume` (0x006ae750), a float 0 .. 99. Default 99.
    pub global_volume: f32,
    /// `SFXVolume` (0x006ae758), 0 .. 99. Default 99.
    pub sfx_volume: i32,
    /// `VoicesVolume` (0x006ae75c), 0 .. 99. Default 99.
    pub voices_volume: i32,
    /// `MusicVolume1` (0x006ae754), a float 0 .. 99. Default 99.
    pub music_volume1: f32,
    /// `LogFile` (0x006e0a30). Default false.
    pub log_file: bool,
    /// `Humeur` (0x00719c74). Default false.
    pub humeur: bool,
    /// `SuperCine` (0x00719c9c). Default false.
    pub super_cine: bool,
    /// The bad values met by the reader.
    pub warnings: Vec<CfgWarning>,
}

impl Default for Config {
    /// The defaults (@ 0x0040cfa0).
    fn default() -> Config {
        Config {
            resolution: (1024, 768),
            pixel_format: PixelFormat::R5G6B5,
            full_screen: true,
            langue: Vec::new(),
            sous_titre: Vec::new(),
            version: Vec::new(),
            install: b"NORM".to_vec(),
            time: [0; 5],
            puzzles: [0; 8],
            warp_res: 768,
            view: (800, 600),
            smooth: true,
            cursor: true,
            invert_mouse_y: false,
            subtitle_on: true,
            global_volume: 99.0,
            sfx_volume: 99,
            voices_volume: 99,
            music_volume1: 99.0,
            log_file: false,
            humeur: false,
            super_cine: false,
            warnings: Vec::new(),
        }
    }
}

/// A float kept in 0 .. 99 as @ 0x0040bf3c does: below 0 is 0, above 99
/// is 99 (a NaN, failing both tests, is 0).
fn clamp_volume(v: f32) -> f32 {
    if v >= 0.0 {
        if v > 99.0 {
            99.0
        } else {
            v
        }
    } else {
        0.0
    }
}

impl Config {
    /// The defaults, then the file `data` (@ 0x0040a720).
    pub fn parse(data: &[u8]) -> Config {
        let mut c = Config::default();
        c.apply(data);
        c
    }

    /// Reads the lines of `data` over the current values.
    pub fn apply(&mut self, data: &[u8]) {
        let mut f = TextCursor::new(data);
        while !f.eof() {
            let line = f.line();
            if line.is_empty() || line.starts_with(b"//") {
                continue;
            }
            let mut l = TextCursor::new(line);
            let key = l.token();
            match key {
                b"Time" => {
                    for t in self.time.iter_mut() {
                        *t = atoi(l.token());
                    }
                    continue;
                }
                b"Puzzles" => {
                    for p in self.puzzles.iter_mut() {
                        *p = atoi(l.token());
                    }
                    continue;
                }
                _ => {}
            }
            l.skip_token();
            match key {
                b"Resolution" => {
                    let w = atoi(l.token());
                    let h = atoi(l.token());
                    self.resolution = (w, h);
                    if !RESOLUTIONS.contains(&(w, h)) {
                        self.warn(CfgWarning::Resolution(w, h));
                    }
                }
                b"PixelFormat" => {
                    let name = l.token();
                    match PixelFormat::from_name(name) {
                        Some(p) => self.pixel_format = p,
                        None => self.warn(CfgWarning::PixelFormat(name.to_vec())),
                    }
                }
                b"FullScreen" => self.bool_value("FullScreen", l.token(), |c| &mut c.full_screen),
                b"Langue" => self.langue = l.token().to_vec(),
                b"SousTitre" => self.sous_titre = l.token().to_vec(),
                b"Version" => self.version = l.token().to_vec(),
                b"Install" => self.install = l.token().to_vec(),
                b"WarpRes" => {
                    self.warp_res = atoi(l.token());
                    if !WARP_RES.contains(&self.warp_res) {
                        self.warn(CfgWarning::WarpRes(self.warp_res));
                    }
                    self.view = (800, 600);
                }
                b"Smooth" => self.bool_value("Smooth", l.token(), |c| &mut c.smooth),
                b"Cursor" => self.bool_value("Cursor", l.token(), |c| &mut c.cursor),
                b"InvertMouseY" => {
                    self.bool_value("InvertMouseY", l.token(), |c| &mut c.invert_mouse_y)
                }
                b"SubTitleOn" => self.bool_value("SubTitleOn", l.token(), |c| &mut c.subtitle_on),
                b"GlobalVolume" => self.global_volume = clamp_volume(atof(l.token())),
                b"SFXVolume" => self.sfx_volume = atoi(l.token()).clamp(0, 99),
                b"VoicesVolume" => self.voices_volume = atoi(l.token()).clamp(0, 99),
                b"MusicVolume1" => self.music_volume1 = clamp_volume(atof(l.token())),
                b"LogFile" => self.bool_value("LogFile", l.token(), |c| &mut c.log_file),
                b"Humeur" => self.bool_value("Humeur", l.token(), |c| &mut c.humeur),
                b"SuperCine" => self.bool_value("SuperCine", l.token(), |c| &mut c.super_cine),
                _ => {}
            }
        }
    }

    fn warn(&mut self, w: CfgWarning) {
        log::warn!("Atlantis4.cfg: {w:?}");
        self.warnings.push(w);
    }

    /// `false` / `true`, anything else logged and ignored.
    fn bool_value(&mut self, key: &'static str, v: &[u8], field: fn(&mut Config) -> &mut bool) {
        match v {
            b"false" => *field(self) = false,
            b"true" => *field(self) = true,
            _ => self.warn(CfgWarning::Bool(key, v.to_vec())),
        }
    }
}

impl Config {
    /// The options in *Atlantis III*'s [`crate::cfg::Config`], which the
    /// engine reads. `GlobalVolume` and `MusicVolume1` are truncated to
    /// integers [UNKNOWN: how the program uses its floats is not read
    /// yet]; `Puzzles` has no place there.
    pub fn to_a3(&self) -> crate::cfg::Config {
        crate::cfg::Config {
            resolution: self.resolution,
            pixel_format: self.pixel_format,
            full_screen: self.full_screen,
            langue: self.langue.clone(),
            sous_titre: self.sous_titre.clone(),
            version: self.version.clone(),
            install: self.install.clone(),
            time: self.time,
            warp_res: self.warp_res,
            view: self.view,
            smooth: self.smooth,
            cursor: self.cursor,
            invert_mouse_y: self.invert_mouse_y,
            subtitle_on: self.subtitle_on,
            global_volume: self.global_volume as i32,
            sfx_volume: self.sfx_volume,
            voices_volume: self.voices_volume,
            music_volume: self.music_volume1 as i32,
            log_file: self.log_file,
            humeur: self.humeur,
            super_cine: self.super_cine,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_keys() {
        let c = Config::parse(b"");
        assert_eq!(
            (c.resolution, c.warp_res, c.view),
            ((1024, 768), 768, (800, 600))
        );
        assert_eq!(
            (c.global_volume, c.music_volume1, c.sfx_volume),
            (99.0, 99.0, 99)
        );
        let c = Config::parse(
            b"// comment\r\n\r\nResolution\t= 800 600\r\nWarpRes = 1024\r\nMusicVolume1 = 120.5\r\n\
              GlobalVolume = -3\r\nSFXVolume = 150\r\nPuzzles 1 2 3 4 5 6 7 8\r\nTime 9 8 7 6 5\r\n\
              SubTitleOn = maybe\r\nMusicVolume = 10\r\n",
        );
        assert_eq!(c.resolution, (800, 600));
        assert_eq!(c.warp_res, 1024);
        assert_eq!(
            (c.music_volume1, c.global_volume, c.sfx_volume),
            (99.0, 0.0, 99)
        );
        assert_eq!(c.puzzles, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(c.time, [9, 8, 7, 6, 5]);
        assert!(c.subtitle_on);
        assert_eq!(
            c.warnings,
            vec![CfgWarning::Bool("SubTitleOn", b"maybe".to_vec())]
        );
        let c = Config::parse(b"WarpRes = 600\nResolution = 1 2\n");
        assert_eq!(
            c.warnings,
            vec![CfgWarning::WarpRes(600), CfgWarning::Resolution(1, 2)]
        );
    }

    /// The CD's file (`ATLANTIS4_CD`).
    #[test]
    fn the_shipped_file() {
        let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
            crate::testing::missing("ATLANTIS4_CD not set");
            return;
        };
        let data = std::fs::read(dir.join("datas/atlantis4.cfg")).unwrap();
        let c = Config::parse(&data);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.resolution, (1024, 768));
        assert!(c.full_screen);
        assert_eq!(c.warp_res, 768);
        assert_eq!((c.voices_volume, c.sfx_volume), (95, 95));
        assert_eq!((c.music_volume1, c.global_volume), (95.0, 95.0));
        assert!(!c.log_file && !c.subtitle_on);
        assert_eq!(c.puzzles, [0; 8]);
    }
}
