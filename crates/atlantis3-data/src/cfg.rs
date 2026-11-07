//! `Datas\Atlantis3.cfg`: the options, a text file of `Key = value`
//! lines.
//!
//! - The reader (@ 0x00406ea0) first sets the defaults (@ 0x004095a0), then
//!   reads the file line by line with the engine's tokenizer
//!   ([`crate::atl::TextCursor`]): blank lines and lines starting with `//`
//!   are skipped, the first word is the key (case-sensitive), the next
//!   word (normally `=`) is skipped whatever it is, and the value follows.
//!   `Time` alone has no `=`. Unknown keys are ignored; a bad value of a
//!   known key is logged and leaves the option unchanged (except
//!   `Resolution` and `WarpRes`, kept even when not one of the expected
//!   values).
//! - After the reader, the start-up (@ 0x004d5e80) replaces `Langue`,
//!   `SousTitre` and `Version` with the install's registry values
//!   (`HKLM\Software\CRYO\ATLANTIS 3`, `Language` and `Install`) and turns
//!   `SubTitleOn` on: [`Config::apply_install`]. So in this build the
//!   file's `Langue`, `SousTitre` and `SubTitleOn` have no effect at
//!   start.
//! - The writer (@ 0x00408f80) writes only what differs from the defaults
//!   (and `WarpRes` and `Time` always), in a fixed order, through a
//!   text-mode handle: every line ends in CR LF. [`Config::write`].
//!
//! The shipped file is one line `SousTitre<TAB>= <lang>` without an end of
//! line, which the reader takes like any other.

use crate::atl::{atoi, TextCursor};

/// The file in the install (`Atlantis3.cfg` @ 0x007148ec under `Datas\`;
/// the writer opens `datas\Atlantis3.cfg` @ 0x00714ab0).
pub const CFG_PATH: &str = "Datas/Atlantis3.cfg";

/// A display pixel format (the table @ 0x007225c0, looked up by
/// @ 0x004e7c80).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// `x8r8g8b8` or `32bits`: code 0x96, 32 bits.
    X8R8G8B8,
    /// `r8g8b8`: code 0x94, 24 bits.
    R8G8B8,
    /// `r5g6b5` or `16bits`: code 0x8a, 16 bits (the default).
    R5G6B5,
    /// `r5g5b5`: code 0x88, 16 bits.
    R5G5B5,
}

impl PixelFormat {
    /// The format a `PixelFormat` value names (@ 0x00407260..0x004074cc).
    pub fn from_name(name: &[u8]) -> Option<PixelFormat> {
        match name {
            b"x8r8g8b8" | b"32bits" => Some(PixelFormat::X8R8G8B8),
            b"r8g8b8" => Some(PixelFormat::R8G8B8),
            b"r5g6b5" | b"16bits" => Some(PixelFormat::R5G6B5),
            b"r5g5b5" => Some(PixelFormat::R5G5B5),
            _ => None,
        }
    }

    /// The engine's code of the format.
    pub fn code(self) -> u32 {
        match self {
            PixelFormat::X8R8G8B8 => 0x96,
            PixelFormat::R8G8B8 => 0x94,
            PixelFormat::R5G6B5 => 0x8a,
            PixelFormat::R5G5B5 => 0x88,
        }
    }

    /// Bits per pixel (the table's second field).
    pub fn bits(self) -> u32 {
        match self {
            PixelFormat::X8R8G8B8 => 32,
            PixelFormat::R8G8B8 => 24,
            PixelFormat::R5G6B5 | PixelFormat::R5G5B5 => 16,
        }
    }

    /// The red, green and blue masks (the table's fields 4..6).
    pub fn masks(self) -> [u32; 3] {
        match self {
            PixelFormat::X8R8G8B8 | PixelFormat::R8G8B8 => [0x00ff_0000, 0x0000_ff00, 0x0000_00ff],
            PixelFormat::R5G6B5 => [0xf800, 0x07e0, 0x001f],
            PixelFormat::R5G5B5 => [0x7c00, 0x03e0, 0x001f],
        }
    }
}

/// The screen resolutions the reader accepts without a message
/// (@ 0x004071b6..0x00407217).
pub const RESOLUTIONS: [(i32, i32); 3] = [(640, 480), (800, 600), (1024, 768)];

/// A bad value the reader logs (the option keeps its value unless said).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CfgWarning {
    /// `Resolution` is not one of [`RESOLUTIONS`] (kept anyway).
    Resolution(i32, i32),
    /// `PixelFormat` names no format (ignored).
    PixelFormat(Vec<u8>),
    /// A boolean is neither `true` nor `false` (ignored).
    Bool(&'static str, Vec<u8>),
    /// `WarpRes` is neither 512 nor 1024 (kept; the warp view is then
    /// 640 x 480).
    WarpRes(i32),
}

/// The options, with the globals they set in the original.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `Resolution w h` (DAT_007558b8, DAT_007557bc): the screen size
    /// asked of DirectDraw (@ 0x0040f1a0 looks the mode up, @ 0x0040fa60
    /// sets it). Default 800 x 600.
    pub resolution: (i32, i32),
    /// `PixelFormat` (7 dwords @ DAT_007558e8 copied from the format
    /// table): the screen's pixel format. Default r5g6b5.
    pub pixel_format: PixelFormat,
    /// `FullScreen` (DAT_007557c0): passed to the display creation
    /// @ 0x0040fb6a. Default true.
    pub full_screen: bool,
    /// `Langue` (DAT_00789b40; its folder `Langue\` DAT_00789b10): the
    /// voices' folder `loc\voix\<Langue>\` (@ 0x00416030) and the language
    /// page `i_langue_<Langue>` (@ 0x0045192e). Default empty.
    pub langue: Vec<u8>,
    /// `SousTitre` (DAT_00789b28; folder DAT_00789b70): the texts' folder
    /// `loc\text\<SousTitre>\` (@ 0x00418240). Default empty.
    pub sous_titre: Vec<u8>,
    /// `Version` (DAT_00789af8): `CD`, `DVD` or `FLA` once the start-up has
    /// set it from the registry. Default empty.
    pub version: Vec<u8>,
    /// `Install` (DAT_00789c68). Default `NORM` (@ 0x0040961a).
    /// `[UNKNOWN]`: no reader of this global was found besides the
    /// accessor @ 0x004d5940.
    pub install: Vec<u8>,
    /// `Time a b c d e` (DAT_00789c0c..DAT_00789c1c): five play-time
    /// counters; the one indexed by DAT_00789bf0 (set by the game's modes
    /// in @ 0x00452000) grows by the time spent. Unit `[UNKNOWN]` (the
    /// difference of `__ftol` of the timer @ 0x004e7b20). Default 0.
    pub time: [i32; 5],
    /// `WarpRes` (DAT_00789d34): the size of the panorama cube faces,
    /// 512 or 1024 (@ 0x0043f420); 1024 also makes the warp view 800 x 600
    /// instead of 640 x 480 ([`Config::warp_view`]). Default 512.
    pub warp_res: i32,
    /// `Smooth` (DAT_00755914, passed to @ 0x0040f120): smoothing of the
    /// stretched picture. Default true.
    pub smooth: bool,
    /// `Cursor` (DAT_007182e0): draws the small cross at the centre of the
    /// warp view (@ 0x00440ab9). Default true.
    pub cursor: bool,
    /// `InvertMouseY` (DAT_007592bc): inverts the vertical mouse look
    /// (@ 0x0043146f). Default false.
    pub invert_mouse_y: bool,
    /// `SubTitleOn` (DAT_00789bd0): subtitles shown (@ 0x0040de6f).
    /// Default true.
    pub subtitle_on: bool,
    /// `GlobalVolume` (DAT_007178e4), 0..99, default 99: the music plays at
    /// `GlobalVolume * 0.01 * MusicVolume` (@ 0x00433bc8).
    pub global_volume: i32,
    /// `SFXVolume` (DAT_007178ec), 0..99, default 99.
    pub sfx_volume: i32,
    /// `VoicesVolume` (DAT_007178f0), 0..99, default 99.
    pub voices_volume: i32,
    /// `MusicVolume` (DAT_007178e8), 0..99, default 50.
    pub music_volume: i32,
    /// `LogFile` (DAT_0074f240): debug log of the variables (@ 0x0040a0b1).
    /// Default false.
    pub log_file: bool,
    /// `Humeur` (DAT_00789c3c): a second set of facial-animation keys
    /// (@ 0x0047d9b8 adds 4 to the key) `[INFERRED: "mood"]`. Default false.
    pub humeur: bool,
    /// `SuperCine` (DAT_00789c64): a mode branch of @ 0x00452000
    /// (@ 0x004528bd) `[UNKNOWN]`. Default false.
    pub super_cine: bool,
    /// The bad values met by the reader.
    pub warnings: Vec<CfgWarning>,
}

impl Default for Config {
    /// The defaults (@ 0x004095a0). `Version` is still empty when they are
    /// set, so its CD / DVD test leaves `WarpRes` at 512.
    fn default() -> Config {
        Config {
            resolution: (800, 600),
            pixel_format: PixelFormat::R5G6B5,
            full_screen: true,
            langue: Vec::new(),
            sous_titre: Vec::new(),
            version: Vec::new(),
            install: b"NORM".to_vec(),
            time: [0; 5],
            warp_res: 512,
            smooth: true,
            cursor: true,
            invert_mouse_y: false,
            subtitle_on: true,
            global_volume: 99,
            sfx_volume: 99,
            voices_volume: 99,
            music_volume: 50,
            log_file: false,
            humeur: false,
            super_cine: false,
            warnings: Vec::new(),
        }
    }
}

impl Config {
    /// The defaults, then the file `data` (@ 0x00406ea0).
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
            if key == b"Time" {
                for t in self.time.iter_mut() {
                    *t = atoi(l.token());
                }
                continue;
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
                    if self.warp_res != 512 && self.warp_res != 1024 {
                        self.warn(CfgWarning::WarpRes(self.warp_res));
                    }
                }
                b"Smooth" => self.bool_value("Smooth", l.token(), |c| &mut c.smooth),
                b"Cursor" => self.bool_value("Cursor", l.token(), |c| &mut c.cursor),
                b"InvertMouseY" => {
                    self.bool_value("InvertMouseY", l.token(), |c| &mut c.invert_mouse_y)
                }
                b"SubTitleOn" => self.bool_value("SubTitleOn", l.token(), |c| &mut c.subtitle_on),
                b"GlobalVolume" => self.global_volume = atoi(l.token()).clamp(0, 99),
                b"SFXVolume" => self.sfx_volume = atoi(l.token()).clamp(0, 99),
                b"VoicesVolume" => self.voices_volume = atoi(l.token()).clamp(0, 99),
                b"MusicVolume" => self.music_volume = atoi(l.token()).clamp(0, 99),
                b"LogFile" => self.bool_value("LogFile", l.token(), |c| &mut c.log_file),
                b"Humeur" => self.bool_value("Humeur", l.token(), |c| &mut c.humeur),
                b"SuperCine" => self.bool_value("SuperCine", l.token(), |c| &mut c.super_cine),
                _ => {}
            }
        }
    }

    fn warn(&mut self, w: CfgWarning) {
        log::warn!("Atlantis3.cfg: {w:?}");
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

    /// What the start-up does after the reader (@ 0x004d6148..0x004d642a):
    /// `Version` := the registry's `Install` (`CD`, `DVD` or `FLA`, else
    /// the game stops), `Langue` and `SousTitre` := the registry's
    /// `Language`, `SubTitleOn` := true. The GOG.com edition writes
    /// `Install = CD` and `Language = fra` for its French install.
    pub fn apply_install(&mut self, language: &[u8], install: &[u8]) {
        self.version = install.to_vec();
        self.langue = language.to_vec();
        self.sous_titre = language.to_vec();
        self.subtitle_on = true;
    }

    /// The size of the warp view (DAT_00714f1c, DAT_00714f20 @ 0x00406f4c
    /// and 0x00407c9f, used @ 0x0040d0a3): 800 x 600 with 1024-pixel faces,
    /// else 640 x 480.
    pub fn warp_view(&self) -> (i32, i32) {
        if self.warp_res == 1024 {
            (800, 600)
        } else {
            (640, 480)
        }
    }

    /// The file the writer (@ 0x00408f80) makes from these options, CR LF
    /// line ends included. `install_language` is the registry language the
    /// start-up kept (DAT_00789c40): `Langue` and `SousTitre` are written
    /// only when they differ from it. The writer takes the resolution and
    /// the pixel depth from the screen in use; here they come from
    /// [`Config::resolution`] and [`Config::pixel_format`] (only `32bits`
    /// can be written: any format of more than 16 bits).
    pub fn write(&self, install_language: &[u8]) -> Vec<u8> {
        let mut s: Vec<u8> = Vec::new();
        let mut line = |text: &[u8]| s.extend_from_slice(text);
        if self.resolution != (800, 600) {
            line(
                format!(
                    "Resolution\t\t= {} {}\n",
                    self.resolution.0, self.resolution.1
                )
                .as_bytes(),
            );
        }
        if self.pixel_format.bits() > 16 {
            line(b"PixelFormat\t\t= 32bits\n");
        }
        if !self.full_screen {
            line(b"FullScreen\t\t= false\n");
        }
        if self.langue != install_language {
            line(b"Langue\t\t= ");
            line(&self.langue);
            line(b"\n");
        }
        if self.sous_titre != install_language {
            line(b"SousTitre\t\t= ");
            line(&self.sous_titre);
            line(b"\n");
        }
        if !self.subtitle_on {
            line(b"SubTitleOn\t\t= false\n");
        }
        line(format!("WarpRes\t\t= {}\n", self.warp_res).as_bytes());
        if !self.smooth {
            line(b"Smooth\t\t= false\n");
        }
        if !self.cursor {
            line(b"Cursor\t\t= false\n");
        }
        if self.invert_mouse_y {
            line(b"InvertMouseY\t= true\n");
        }
        if self.global_volume != 99 {
            line(format!("GlobalVolume\t= {}\n", self.global_volume).as_bytes());
        }
        if self.sfx_volume != 99 {
            line(format!("SFXVolume\t\t= {}\n", self.sfx_volume).as_bytes());
        }
        if self.voices_volume != 99 {
            line(format!("VoicesVolume\t= {}\n", self.voices_volume).as_bytes());
        }
        if self.music_volume != 50 {
            line(format!("MusicVolume\t\t= {}\n", self.music_volume).as_bytes());
        }
        if self.humeur {
            line(b"Humeur\t = true\n");
        }
        if self.super_cine {
            line(b"SuperCine\t = true\n");
        }
        if self.log_file {
            line(b"LogFile\t\t= true\n");
        }
        let t = self.time;
        line(format!("Time\t\t{} {} {} {} {}\n", t[0], t[1], t[2], t[3], t[4]).as_bytes());
        // `_open(.., 0x4301)`: _O_TEXT, so `_write` turns LF into CR LF.
        let mut out = Vec::with_capacity(s.len() + 32);
        for &b in &s {
            if b == b'\n' {
                out.push(b'\r');
            }
            out.push(b);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_installer_line() {
        let c = Config::parse(b"SousTitre\t= abc");
        assert_eq!(c.sous_titre, b"abc");
        assert_eq!(c.resolution, (800, 600));
        assert_eq!(c.pixel_format, PixelFormat::R5G6B5);
        assert_eq!(c.pixel_format.code(), 0x8a);
        assert_eq!(c.warp_res, 512);
        assert_eq!(c.warp_view(), (640, 480));
        assert_eq!(c.install, b"NORM");
        assert!(c.full_screen && c.smooth && c.cursor && c.subtitle_on);
        assert_eq!((c.global_volume, c.music_volume), (99, 50));
        assert!(c.warnings.is_empty());
    }

    #[test]
    fn values_and_warnings() {
        let c = Config::parse(
            b"// comment\r\nResolution x 1 2\r\nPixelFormat = 16bits\r\n\r\n\
SFXVolume = 120\r\nMusicVolume = -4\r\nCursor = maybe\r\nTime 1 2 3\r\nUnknown = 1\r\n",
        );
        assert_eq!(c.resolution, (1, 2));
        assert_eq!(c.pixel_format, PixelFormat::R5G6B5);
        assert_eq!(c.sfx_volume, 99);
        assert_eq!(c.music_volume, 0);
        assert!(c.cursor);
        assert_eq!(c.time, [1, 2, 3, 0, 0]);
        assert_eq!(
            c.warnings,
            vec![
                CfgWarning::Resolution(1, 2),
                CfgWarning::Bool("Cursor", b"maybe".to_vec())
            ]
        );
    }

    #[test]
    fn writer_defaults() {
        let mut c = Config::default();
        c.apply_install(b"fra", b"CD");
        assert_eq!(
            c.write(b"fra"),
            b"WarpRes\t\t= 512\r\nTime\t\t0 0 0 0 0\r\n"
        );
    }

    #[test]
    fn round_trip() {
        let c = Config {
            resolution: (1024, 768),
            pixel_format: PixelFormat::X8R8G8B8,
            full_screen: false,
            langue: b"ger".to_vec(),
            sous_titre: b"gbr".to_vec(),
            subtitle_on: false,
            warp_res: 1024,
            smooth: false,
            cursor: false,
            invert_mouse_y: true,
            global_volume: 10,
            sfx_volume: 20,
            voices_volume: 30,
            music_volume: 40,
            log_file: true,
            humeur: true,
            super_cine: true,
            time: [5, -1, 0, 7, 123_456],
            ..Config::default()
        };
        let bytes = c.write(b"fra");
        assert!(bytes.starts_with(b"Resolution\t\t= 1024 768\r\nPixelFormat\t\t= 32bits\r\n"));
        assert!(bytes.ends_with(b"LogFile\t\t= true\r\nTime\t\t5 -1 0 7 123456\r\n"));
        let back = Config::parse(&bytes);
        assert_eq!(back, c);
        assert_eq!(back.warp_view(), (800, 600));
    }
}
