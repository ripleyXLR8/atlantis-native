//! The subtitles of the cinematics.
//!
//! `AtlHnm_Start` (@ 0x00420ee0) is given a video and a subtitle file name;
//! the script command that plays cinematic n names them `cine<nnn>.hnm`
//! and `cine<nnn>.txt` (@ 0x0044813b, format @ 0x00718f7c). Two files can
//! carry subtitles:
//!
//! - `hnm\cine<nnn>.txt` ([`CueFile`]), the same for every language: a
//!   list of timed lines whose texts are dialog lines of `dial.txt`
//!   (`Atl_Hnm_LoadUnderTitleFile` @ 0x004215d0, reading
//!   `CDialogue::GetTextFromDial` @ 0x004655a0). When it is there the
//!   video is taken from the language's folder (@ 0x00420f81 → @ 0x00416c60),
//!   else from `hnm\` (@ 0x00421089 → @ 0x00416880);
//! - `loc\text\<language>\cine<nnn>.txt` ([`Caption`]), one caption
//!   that fades in and out, read after it for every video but the travel
//!   ones (@ 0x00420fa8 .. 0x00420fef → @ 0x00420a20).
//!
//! Both are read with the shared reader of [`crate::tokens`], and both are
//! drawn while the video plays (@ 0x00421c10), against its frame number.

use crate::tokens::{atoi, ContentLines, Reader};

/// The video and subtitle file names of cinematic `n` (@ 0x0044813b).
pub fn cinema_names(n: u32) -> (String, String) {
    (format!("cine{n:03}.hnm"), format!("cine{n:03}.txt"))
}

/// The cue file of a subtitle name (`hnm\` + name, @ 0x00417250).
pub fn cue_path(name: &str) -> String {
    format!("Datas/hnm/{name}")
}

/// The caption file of a subtitle name in the subtitle language's folder
/// (`loc\text\<language>\` + name, @ 0x00418240).
pub fn caption_path(language: &str, name: &str) -> String {
    format!("Datas/loc/text/{language}/{name}")
}

/// A line of a cue file: `#NEW <dialog> <line> <from> <to> <red> <green>
/// <blue>` (@ 0x004219c2 .. 0x00421a1f, stored @ 0x00421a91 ..
/// 0x00421b13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cue {
    /// The dialog line whose `dial.txt` text is shown (fetched once, at
    /// load, @ 0x00421a8a).
    pub dialog: i32,
    pub line: i32,
    /// The first frame of the video showing it: shown once the frame
    /// number reaches it (node +0x24, @ 0x00421c10).
    pub from_frame: i32,
    /// Hidden once the frame number reaches it (node +0x28).
    pub to_frame: i32,
    /// The text colour as read (node +0x18, +0x1c, +0x20), drawn as the
    /// Windows colour `red | green << 8 | blue << 16`, each value cut to
    /// its low byte (@ 0x0041e2c0).
    pub color: [i32; 3],
}

impl Cue {
    /// The colour as drawn.
    pub fn rgb(&self) -> [u8; 3] {
        self.color.map(|c| c.to_le_bytes()[0])
    }
}

/// A cue file as read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CueFile {
    pub cues: Vec<Cue>,
    /// Lines whose first word does not start with `#NEW`: the original
    /// shows an error box (@ 0x00421958) and reads the line anyway; they
    /// are in [`CueFile::cues`] too.
    pub bad_markers: usize,
}

impl CueFile {
    /// Reads a cue file (@ 0x004215d0). Missing numbers read as 0.
    pub fn parse(data: &[u8]) -> CueFile {
        let mut out = CueFile::default();
        for (_, line) in ContentLines::new(data) {
            let mut w = Reader::new(line);
            if !w.word().starts_with(b"#NEW") {
                out.bad_markers += 1;
            }
            let mut v = [0; 7];
            for x in &mut v {
                *x = atoi(w.word());
            }
            out.cues.push(Cue {
                dialog: v[0],
                line: v[1],
                from_frame: v[2],
                to_frame: v[3],
                color: [v[4], v[5], v[6]],
            });
        }
        out
    }
}

/// How much a caption's opacity grows on each drawing step after its
/// first frame (@ 0x00421c10), from 0 to 1.
pub const CAPTION_FADE_IN_STEP: f32 = 0.05;
/// How much it falls on each step after its last frame, down to 0.
pub const CAPTION_FADE_OUT_STEP: f32 = 0.02;

/// The caption of a `loc\text\<language>\cine<nnn>.txt` file: its last
/// `#NEW <fade in> <fade out> <text>` line (@ 0x00420a20, values stored
/// @ 0x0075647c, @ 0x007560a0 and the string @ 0x00756428).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caption {
    /// The frame after which it fades in.
    pub fade_in_frame: i32,
    /// The frame after which it fades out.
    pub fade_out_frame: i32,
    /// The text: the rest of the line, Windows-1252 bytes.
    pub text: Vec<u8>,
}

impl Caption {
    /// Reads a caption file: lines whose first word does not start with
    /// `#NEW` are skipped, and each `#NEW` line replaces the previous one;
    /// `None` without one.
    pub fn parse(data: &[u8]) -> Option<Caption> {
        let mut out = None;
        for (_, line) in ContentLines::new(data) {
            let mut w = Reader::new(line);
            if !w.word().starts_with(b"#NEW") {
                continue;
            }
            let fade_in_frame = atoi(w.word());
            let fade_out_frame = atoi(w.word());
            out = Some(Caption {
                fade_in_frame,
                fade_out_frame,
                text: w.line().to_vec(),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues() {
        let f = CueFile::parse(
            b"#NEW\t301\t1\t168\t254\t255\t200\t100\r\n\r\n// c\r\nX 2 3 4 5 256 -1\r\n#NEW 1 2 3\r\n",
        );
        assert_eq!(f.cues.len(), 3);
        assert_eq!(f.bad_markers, 1);
        assert_eq!(
            f.cues[0],
            Cue {
                dialog: 301,
                line: 1,
                from_frame: 168,
                to_frame: 254,
                color: [255, 200, 100],
            }
        );
        assert_eq!(f.cues[1].color, [256, -1, 0]);
        assert_eq!(f.cues[1].rgb(), [0, 255, 0]);
        assert_eq!(f.cues[2].color, [0, 0, 0]);
        assert_eq!(f.cues[2].to_frame, 0);
    }

    #[test]
    fn captions_and_names() {
        let c = Caption::parse(b"// note\r\n#NEW\t50\t150\tFirst\r\n#NEW\t5\t15\t  Last, one \r\n");
        assert_eq!(
            c,
            Some(Caption {
                fade_in_frame: 5,
                fade_out_frame: 15,
                text: b"Last, one ".to_vec(),
            })
        );
        assert_eq!(Caption::parse(b"// only\r\nother 1 2 x\r\n"), None);
        let (video, subs) = cinema_names(2);
        assert_eq!(
            (video.as_str(), subs.as_str()),
            ("cine002.hnm", "cine002.txt")
        );
        assert_eq!(cue_path(&subs), "Datas/hnm/cine002.txt");
        assert_eq!(caption_path("fra", &subs), "Datas/loc/text/fra/cine002.txt");
    }
}
