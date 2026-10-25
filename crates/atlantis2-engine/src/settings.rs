//! The port's settings for Atlantis II: `settings.ini`, kept beside the
//! saves and `atlant2.cfg` (the `atlantis2` folder of the port's data
//! folder). The first game's mechanism and keys
//! (`atlantis_engine::persist::Settings`): `key = value` lines, `#`
//! comments, unknown keys and bad values ignored with a warning; the
//! options menu (F10, [`crate::options_menu`]) writes it at each change.
//!
//! It holds the port's options only ([`Enhancements`]: the minor bug
//! fixes, the graphics and the HD mode's options). The options of the
//! original's own menus, the 800 x 600 screen among them, stay in
//! `atlant2.cfg` as the original keeps them.

use atlantis_engine::hd::{PanoFilter, VideoSides};
use atlantis_engine::persist::Graphics;

use crate::enhancements::Enhancements;

/// The settings file's name.
pub const SETTINGS_FILE: &str = "settings.ini";

/// The bars beside a video that Atlantis II offers in widescreen: black or
/// blurred (the first game's zoomed and panorama sides need its videos'
/// panoramas).
pub fn video_sides(v: VideoSides) -> VideoSides {
    match v {
        VideoSides::Blur => VideoSides::Blur,
        _ => VideoSides::Black,
    }
}

/// The settings: the enhancements, the HD mode and its options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub enhancements: Enhancements,
}

impl Settings {
    /// `key = value` lines; unknown keys and bad values keep the default.
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        let x = &mut s.enhancements;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                log::warn!("{SETTINGS_FILE}: '{line}' not understood");
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            let b = match v.to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "yes" => Some(true),
                "0" | "false" | "off" | "no" => Some(false),
                _ => None,
            };
            let ok = match (k.as_str(), b) {
                ("minor_bug_fixes", Some(b)) => {
                    x.minor_bug_fixes = b;
                    true
                }
                ("graphics", _) => Graphics::from_name(v)
                    .map(|g| x.hd = g == Graphics::Hd)
                    .is_some(),
                ("hd_panorama", _) => PanoFilter::from_name(v)
                    .map(|f| x.hd_options.panorama = f)
                    .is_some(),
                ("hd_sharpen", Some(b)) => {
                    x.hd_options.sharpen = b;
                    true
                }
                ("hd_interface", _) => match v.to_ascii_lowercase().as_str() {
                    "smooth" => {
                        x.hd_options.smooth_interface = true;
                        true
                    }
                    "pixels" => {
                        x.hd_options.smooth_interface = false;
                        true
                    }
                    _ => false,
                },
                ("hd_sharp_subtitles", Some(b)) => {
                    x.hd_options.sharp_subtitles = b;
                    true
                }
                ("hd_widescreen", Some(b)) => {
                    x.hd_options.widescreen = b;
                    true
                }
                ("hd_video_sides", _) => VideoSides::from_name(v)
                    .map(|s| x.hd_options.video_sides = video_sides(s))
                    .is_some(),
                _ => false,
            };
            if !ok {
                log::warn!("{SETTINGS_FILE}: '{line}' not understood");
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let b = |v: bool| if v { "on" } else { "off" };
        let x = &self.enhancements;
        let o = &x.hd_options;
        format!(
            "# Atlantis Native settings (Atlantis II)\n\
             # Enhancements: where the port may differ from the original game\n\
             minor_bug_fixes = {}\n\
             # The port's graphics (options menu, F10): original or hd\n\
             graphics = {}\n\
             # HD mode: panorama filter (bicubic or bilinear), its sharpening,\n\
             # the interface art (smooth: xBR, or pixels), the texts drawn at\n\
             # the window's resolution, widescreen (the panorama fills the\n\
             # sides of a wide window) and the bars beside the videos (black\n\
             # or blur)\n\
             hd_panorama = {}\n\
             hd_sharpen = {}\n\
             hd_interface = {}\n\
             hd_sharp_subtitles = {}\n\
             hd_widescreen = {}\n\
             hd_video_sides = {}\n\
             # The original's options (subtitles, volumes, the 800 x 600\n\
             # screen) are in atlant2.cfg, as the original keeps them\n",
            b(x.minor_bug_fixes),
            if x.hd { "hd" } else { "original" },
            o.panorama.name(),
            b(o.sharpen),
            if o.smooth_interface {
                "smooth"
            } else {
                "pixels"
            },
            b(o.sharp_subtitles),
            b(o.widescreen),
            video_sides(o.video_sides).name(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hd::HdOptions;

    #[test]
    fn settings_round_trip() {
        let d = Settings::default();
        assert!(d.enhancements.minor_bug_fixes && !d.enhancements.hd);
        assert_eq!(Settings::parse(&d.to_text()), d);
        let s = Settings {
            enhancements: Enhancements {
                minor_bug_fixes: false,
                hd: true,
                hd_options: HdOptions {
                    panorama: PanoFilter::Bilinear,
                    sharpen: false,
                    smooth_interface: false,
                    sharp_subtitles: false,
                    widescreen: true,
                    video_sides: VideoSides::Blur,
                },
            },
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
        // The first game's keys and values.
        let p = Settings::parse(
            "graphics = HD\nhd_panorama = Bilinear\nhd_sharpen = 0\nhd_interface = pixels\n\
             hd_sharp_subtitles = 0\nhd_widescreen = yes\nhd_video_sides = blur\nminor_bug_fixes = off",
        );
        assert_eq!(p, s);
        // Bad values keep the defaults; the sides Atlantis II lacks are black.
        let p = Settings::parse("graphics = 4k\nhd_panorama = lanczos\nnonsense\nvolume = 3");
        assert_eq!(p, d);
        let p = Settings::parse("hd_video_sides = panorama");
        assert_eq!(p.enhancements.hd_options.video_sides, VideoSides::Black);
    }
}
