//! Sounds: Ogg Vorbis (music, voices, ambiences, effects, the cut-scenes'
//! sound tracks) and RIFF WAVE (eleven effects).
//!
//! The original decodes Vorbis with libvorbis 1.2.0 (file +0x1f6764) and
//! plays through OpenAL. Vorbis is decoded here by [`crate::vorbis`], a
//! port of that decoder that gives the game's samples bit for bit.

/// A whole decoded Vorbis stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub channels: u8,
    pub sample_rate: u32,
    /// Interleaved 16-bit samples.
    pub samples: Vec<i16>,
}

/// Why a sound cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioError {}

/// An Ogg Vorbis file, decoded whole, as the game's `ov_read` loop gives it.
pub fn decode_vorbis(data: &[u8]) -> Result<Decoded, AudioError> {
    let (channels, sample_rate, samples) =
        crate::vorbis::decode(data).map_err(|e| AudioError(e.to_string()))?;
    Ok(Decoded {
        channels: channels as u8,
        sample_rate,
        samples,
    })
}

/// The format of a RIFF WAVE file and where its samples are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    /// `wFormatTag` (1: PCM).
    pub format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
    /// The `data` chunk: offset and length in bytes.
    pub data_offset: usize,
    pub data_len: usize,
}

/// The `fmt ` and `data` chunks of a RIFF WAVE file.
pub fn wav_info(data: &[u8]) -> Result<WavInfo, AudioError> {
    let bad = |w: &str| AudioError(format!("WAVE: {w}"));
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(bad("no RIFF WAVE header"));
    }
    let mut at = 12;
    let mut fmt = None;
    while at + 8 <= data.len() {
        let id = &data[at..at + 4];
        let len =
            u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let body = at + 8;
        if id == b"fmt " {
            let f = data.get(body..body + 16).ok_or_else(|| bad("short fmt"))?;
            fmt = Some((
                u16::from_le_bytes([f[0], f[1]]),
                u16::from_le_bytes([f[2], f[3]]),
                u32::from_le_bytes([f[4], f[5], f[6], f[7]]),
                u16::from_le_bytes([f[14], f[15]]),
            ));
        } else if id == b"data" {
            let (format, channels, sample_rate, bits) =
                fmt.ok_or_else(|| bad("data before fmt"))?;
            if body + len > data.len() {
                return Err(bad("data chunk truncated"));
            }
            return Ok(WavInfo {
                format,
                channels,
                sample_rate,
                bits,
                data_offset: body,
                data_len: len,
            });
        }
        // Chunks are padded to an even length.
        at = body + len + (len & 1);
    }
    Err(bad("no data chunk"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wave_file_gives_its_format_and_samples() {
        let mut w = b"RIFF\0\0\0\0WAVE".to_vec();
        w.extend(b"fmt \x10\0\0\0");
        w.extend([1, 0, 2, 0]);
        w.extend(44_100u32.to_le_bytes());
        w.extend((44_100u32 * 4).to_le_bytes());
        w.extend([4, 0, 16, 0]);
        w.extend(b"data\x04\0\0\0");
        w.extend([1, 2, 3, 4]);
        let i = wav_info(&w).unwrap();
        assert_eq!(
            (i.format, i.channels, i.sample_rate, i.bits),
            (1, 2, 44_100, 16)
        );
        assert_eq!((i.data_offset, i.data_len), (w.len() - 4, 4));
        assert!(wav_info(&w[..w.len() - 1]).is_err());
        assert!(decode_vorbis(b"not ogg").is_err());
    }
}
