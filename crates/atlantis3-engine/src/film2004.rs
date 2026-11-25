//! *Atlantis Evolution*'s 2004 program's films (`AtlBink`, atlantis4-cd-en-3022848):
//! a Bink file decoded by the port of ScummVM's decoder
//! ([`atlantis3_data::bink`]) in place of RAD's `binkw32.dll`, and its
//! Ogg Vorbis sound track in place of the FMOD stream.
//!
//! The RE note is atlantis4-re `subsystems/cd2004-films.md`. The loop mode 5
//! handler (`AtlMain_loop_BINK` @ 0x004a1560 → FUN_00402fb0, its end
//! FUN_00403340 → FUN_00403230) is in [`crate::game::Game`]; this module
//! holds the film's clock and its sound.
//!
//! The Bink calls the program makes (`BinkWait`, `BinkDoFrame`,
//! `BinkCopyToBuffer`, `BinkNextFrame`, the header's `FrameNum` +0xc and
//! `Frames` +0x8) are taken with the meaning of RAD's Bink SDK
//! ([INFERRED]: `binkw32.dll` is not read): `FrameNum` is the 1-based
//! number of the frame `BinkDoFrame` decodes next, and `BinkWait` returns
//! 0 once that frame's time, `(FrameNum − 1)` frame periods of the file's
//! rate after the open, has come.

use std::sync::Arc;

use atlantis3_data::bink::{BinkError, BinkVideo};
use atlantis3_data::spr::Depth;
use atlantis4_data::vorbis::VorbisStream;

use crate::audio::Audio;

/// The decoded frames kept ahead of the output.
const AHEAD_FRAMES: u64 = 22050;

/// A film of `AtlBink_Start` (the Bink handle 0x006e01fc).
pub struct Film {
    name: String,
    video: BinkVideo<Arc<[u8]>>,
    /// `FrameNum` (handle +0xc): 1 after the open.
    pub frame_num: u32,
    /// Whether frame `frame_num` was drawn (`BinkDoFrame` then
    /// `BinkNextFrame` only after a draw, @ 0x00402f60).
    drawn: bool,
    /// When `BinkOpen` ran (the game's seconds).
    start: f64,
    /// The frames of the passes before this one ([`Film::bink_step`]'s
    /// wrap): `BinkWait` times frames by the count `BinkDoFrame` keeps,
    /// which goes on across the wrap.
    looped: u32,
    /// The last frame decoded, in the display's format.
    pub rgb565: Vec<u16>,
    pub xrgb: Vec<u32>,
    depth: Depth,
}

impl Film {
    /// `BinkOpen(path, 0)` (@ 0x0040248f): `None` when the file is no Bink
    /// film (`AtlBink_start - Cinematique non ouverte`).
    pub fn open(name: &str, data: Arc<[u8]>, depth: Depth, now: f64) -> Result<Film, BinkError> {
        let video = BinkVideo::open(data)?;
        let n = video.width() as usize * video.height() as usize;
        let (rgb565, xrgb) = match depth {
            Depth::Rgb565 => (vec![0; n], Vec::new()),
            Depth::Xrgb8888 => (Vec::new(), vec![0; n]),
        };
        Ok(Film {
            name: name.to_string(),
            video,
            frame_num: 1,
            drawn: false,
            start: now,
            looped: 0,
            rgb565,
            xrgb,
            depth,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The picture's size (handle +0x0, +0x4).
    pub fn size(&self) -> (usize, usize) {
        (self.video.width() as usize, self.video.height() as usize)
    }

    /// `Frames` (handle +0x8).
    pub fn frames(&self) -> u32 {
        self.video.frame_count()
    }

    /// The file's frame rate.
    pub fn fps(&self) -> f64 {
        self.video.fps()
    }

    /// The film's clock started again at `now` (the game's seconds): its
    /// frames timed from there, as from the open.
    pub fn restart_clock(&mut self, now: f64) {
        self.start = now;
    }

    /// `BinkWait` = 0: frame `frame_num` is not drawn yet and its time has
    /// come.
    pub fn due(&self, now: f64) -> bool {
        let ms = ((now - self.start) * 1000.0).max(0.0) as u64;
        !self.drawn && ms >= self.video.frame_time_ms(self.looped + self.frame_num - 1)
    }

    /// `BinkWait` = 0 for frame `frame_num` whether it was drawn or not
    /// (a caller that draws again without `BinkNextFrame`, as Bulle's
    /// frame while its start timer runs: [INFERRED, RAD's API]).
    pub fn time_come(&self, now: f64) -> bool {
        let ms = ((now - self.start) * 1000.0).max(0.0) as u64;
        ms >= self.video.frame_time_ms(self.looped + self.frame_num - 1)
    }

    /// Whether frame `frame_num` was decoded.
    pub fn drawn(&self) -> bool {
        self.drawn
    }

    /// `BinkDoFrame` and `BinkCopyToBuffer` (@ 0x00402e40, @ 0x00402e97)
    /// into the display's format. The copy's flags are the surface's type
    /// (FUN_00401ba0: 3 for 32 bits, 9..12 for the 16-bit layouts), no
    /// alpha type: an alpha plane is not used. False after a decoding
    /// error (logged).
    pub fn do_frame(&mut self) -> bool {
        self.drawn = true;
        match self.video.next_frame() {
            Some(Ok(f)) => {
                match self.depth {
                    Depth::Rgb565 => f.to_rgb565(&mut self.rgb565),
                    Depth::Xrgb8888 => f.to_xrgb8888(&mut self.xrgb),
                }
                true
            }
            Some(Err(e)) => {
                log::warn!("{}: {e:?}", self.name);
                false
            }
            None => false,
        }
    }

    /// Decodes the file's audio track `track` from the next frame on (the
    /// 2004 program's films have none; *The Secrets of Atlantis*' logos
    /// have one): its rate and channels, `None` without such a track.
    pub fn enable_audio(&mut self, track: usize) -> Option<(u32, u16)> {
        self.video.enable_audio(track)
    }

    /// The samples of the decoded audio track `track` since the last call
    /// (interleaved).
    pub fn take_audio(&mut self, track: usize) -> Vec<i16> {
        self.video.take_audio(track)
    }

    /// FUN_00402f60 after a drawn frame: at the last frame (`FrameNum` =
    /// `Frames`) the film ends (false); else `BinkNextFrame`.
    pub fn next_frame(&mut self) -> bool {
        if self.frame_num >= self.frames() {
            return false;
        }
        self.frame_num += 1;
        self.drawn = false;
        true
    }

    /// *The Secrets of Atlantis*' film player's step (atlantis5-dvd-int-3891200
    /// @ 0x00456cb0) when its caller ignores its return (the dialog's 2D
    /// talker, state 2): `if (BinkWait() == 0) { BinkDoFrame();
    /// BinkNextFrame(); }`, at most one frame per call. Its `binkw32.dll`
    /// (RAD 1994-2002, `BinkNextFrame` @ 0x30014550,
    /// 0x300146e0..0x30014702) goes to frame 1 again once `FrameNum` ≥
    /// `Frames`, so the film loops; `BinkWait` (@ 0x30014c10) times frames
    /// by the count `BinkDoFrame` keeps (+0x270, @ 0x300144c6..0x300144df),
    /// which the wrap does not reset: frame 1 of the next pass comes one
    /// period after the last frame. Whether a frame was decoded.
    pub fn bink_step(&mut self, now: f64) -> bool {
        if !self.due(now) {
            return false;
        }
        self.do_frame();
        if !self.next_frame() {
            self.looped += self.frames();
            self.video.rewind();
            self.frame_num = 1;
            self.drawn = false;
        }
        true
    }
}

/// The film's sound track, `FSOUND_Stream_Open(path, 0x2001, 0, 0)` then
/// `FSOUND_Stream_Play(FSOUND_FREE, stream)` (@ 0x00402743, @ 0x0040278b):
/// one pass of the Ogg file, decoded ahead of the output into one
/// [`Audio`] channel.
pub struct FilmSound {
    dec: VorbisStream,
    channel: usize,
    pushed: u64,
    done: bool,
}

impl FilmSound {
    /// Opens `data` and starts it on `channel` at gain `gain`.
    pub fn open(audio: &Audio, channel: usize, data: Vec<u8>, gain: f32) -> Option<FilmSound> {
        let dec = VorbisStream::new(data).ok()?;
        audio.start_stream(channel, dec.rate(), dec.channels() as u16, gain);
        let mut s = FilmSound {
            dec,
            channel,
            pushed: 0,
            done: false,
        };
        s.pump(audio);
        Some(s)
    }

    /// Decodes ahead of the output; false once the whole track was played
    /// (FMOD's end callback LAB_00401d10 then clears 0x006e0208).
    pub fn pump(&mut self, audio: &Audio) -> bool {
        let played = audio.played(self.channel).unwrap_or(0);
        let mut buf = Vec::new();
        while !self.done && self.pushed < played + AHEAD_FRAMES {
            buf.clear();
            let n = self.dec.read(&mut buf, 4096);
            if n == 0 {
                self.done = true;
                break;
            }
            audio.push(self.channel, &buf);
            self.pushed += n as u64;
        }
        !self.done || audio.is_playing(self.channel)
    }

    /// `FSOUND_Stream_Stop` and `FSOUND_Stream_Close`.
    pub fn stop(self, audio: &Audio) {
        audio.stop(self.channel);
    }
}

/// The sound track's volume, `FSOUND_SetVolume(…, ftol(G · 0.01 · M · 0.01
/// · 204))` (@ 0x0040287e..0x004028a5, `GlobalVolume` 0x006ae750,
/// `MusicVolume1` 0x006ae754, 0.01 at 0x00699334, 204 at 0x00699330), as a
/// gain `v / 255` [INFERRED from FMOD 3's API, as the music's].
///
/// The program passes the boolean "the play failed" (0 on success) as the
/// channel (@ 0x00402790..0x004028a4), so the call sets FMOD channel 0;
/// the stream played on `FSOUND_FREE` is taken to be channel 0 (the music
/// uses channels 1..5) [INFERRED].
pub fn sound_gain(global: f32, music: f32) -> f32 {
    let v =
        (f64::from(global) * f64::from(0.01f32) * (f64::from(music) * f64::from(0.01f32)) * 204.0)
            as i32;
    v.clamp(0, 255) as f32 / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume() {
        // The data's 99 and 99: ftol(0.99 · 0.99 · 204) = 199.
        assert!((sound_gain(99.0, 99.0) - 199.0 / 255.0).abs() < 1e-6);
        assert_eq!(sound_gain(0.0, 99.0), 0.0);
    }
}
