//! Cryo APC sound: IMA-like 4-bit ADPCM (the WAV\\*.APC files and the
//! sound track of HNM6 videos).
//!
//! Translated from ScummVM's `Audio::APCStreamImpl` (audio/decoders/apc.cpp,
//! GPLv3, the ScummVM team) with the Atlantis engine's end rule
//! (engines/cryomni3d/atlantis/apc_stream.cpp): bit exact with the original
//! header reader FUN_00417dd0 and decode loop FUN_004101ce on every APC file
//! of the game.

use crate::DecodeError;

/// Size of the APC header.
pub const APC_HEADER_SIZE: usize = 32;

const MAGIC: &[u8; 12] = b"CRYO_APC1.20";

/// IMA ADPCM step sizes.
#[rustfmt::skip]
const IMA_STEPS: [i16; 89] = [
        7,     8,     9,    10,    11,    12,    13,    14,
       16,    17,    19,    21,    23,    25,    28,    31,
       34,    37,    41,    45,    50,    55,    60,    66,
       73,    80,    88,    97,   107,   118,   130,   143,
      157,   173,   190,   209,   230,   253,   279,   307,
      337,   371,   408,   449,   494,   544,   598,   658,
      724,   796,   876,   963,  1060,  1166,  1282,  1411,
     1552,  1707,  1878,  2066,  2272,  2499,  2749,  3024,
     3327,  3660,  4026,  4428,  4871,  5358,  5894,  6484,
     7132,  7845,  8630,  9493, 10442, 11487, 12635, 13899,
    15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

/// IMA ADPCM step index changes, per 4-bit code.
const STEP_ADJUST: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// The 32-byte header of an APC file (or of an HNM6 sound track).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApcHeader {
    /// Number of sample frames (one sample per channel) the original plays.
    pub frames: u32,
    pub sample_rate: u32,
    /// Initial predictor of the left (or only) channel.
    pub left: i32,
    /// Initial predictor of the right channel.
    pub right: i32,
    pub stereo: bool,
}

impl ApcHeader {
    /// Parses the header at the start of `data`.
    pub fn parse(data: &[u8]) -> Result<ApcHeader, DecodeError> {
        if data.len() < APC_HEADER_SIZE {
            return Err(DecodeError::Truncated);
        }
        if &data[..12] != MAGIC {
            return Err(DecodeError::NotApc);
        }
        let le32 = |pos: usize| {
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
        };
        Ok(ApcHeader {
            frames: le32(0x0c),
            sample_rate: le32(0x10),
            left: le32(0x14) as i32,
            right: le32(0x18) as i32,
            stereo: le32(0x1c) & 1 != 0,
        })
    }

    pub fn channels(&self) -> u16 {
        if self.stereo {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Status {
    last: i32,
    step_index: usize,
    step: i32,
}

impl Status {
    fn new(last: i32) -> Self {
        Status {
            last,
            step_index: 0,
            step: IMA_STEPS[0].into(),
        }
    }

    fn decode(&mut self, code: u8) -> i16 {
        // Cryo's decoder adds step / 8 and, for each set bit of the code,
        // step, step / 2 or step / 4, each shift truncated separately (not
        // the usual (2 * code + 1) * step / 8).
        let step = self.step;
        let mut diff = step >> 3;
        if code & 4 != 0 {
            diff += step;
        }
        if code & 2 != 0 {
            diff += step >> 1;
        }
        if code & 1 != 0 {
            diff += step >> 2;
        }
        if code & 8 != 0 {
            diff = -diff;
        }

        // The predictor is kept at full 32-bit width, wrapping like the
        // original; samples are truncated, not clipped, to 16 bits.
        self.last = self.last.wrapping_add(diff);

        let index = self.step_index as i32 + STEP_ADJUST[usize::from(code)];
        self.step_index = index.clamp(0, IMA_STEPS.len() as i32 - 1) as usize;
        self.step = IMA_STEPS[self.step_index].into();

        self.last as i16
    }
}

/// A streaming APC decoder: feed it the data after the header, in pieces
/// of any size.
#[derive(Clone, Debug)]
pub struct ApcDecoder {
    status: [Status; 2],
    stereo: bool,
}

impl ApcDecoder {
    /// A decoder starting from `header`'s predictors.
    pub fn new(header: &ApcHeader) -> Self {
        ApcDecoder {
            status: [Status::new(header.left), Status::new(header.right)],
            stereo: header.stereo,
        }
    }

    /// A decoder resuming in the middle of a stream from a saved state:
    /// each channel's predictor and step index (Atlantis II's `.spp` music
    /// marks keep them, with the step the index gives).
    pub fn resume(stereo: bool, predictors: [i32; 2], step_indexes: [u32; 2]) -> Self {
        let status = |c: usize| {
            let step_index = (step_indexes[c] as usize).min(IMA_STEPS.len() - 1);
            Status {
                last: predictors[c],
                step_index,
                step: IMA_STEPS[step_index].into(),
            }
        };
        ApcDecoder {
            status: [status(0), status(1)],
            stereo,
        }
    }

    pub fn stereo(&self) -> bool {
        self.stereo
    }

    /// Decodes `data`, two 4-bit codes per byte (high nibble first; left
    /// then right in stereo), appending the samples (interleaved in
    /// stereo) to `out`.
    pub fn decode(&mut self, data: &[u8], out: &mut Vec<i16>) {
        out.reserve(2 * data.len());
        let second = usize::from(self.stereo);
        for &byte in data {
            out.push(self.status[0].decode(byte >> 4));
            out.push(self.status[second].decode(byte & 0x0f));
        }
    }
}

/// A decoded APC file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Apc {
    pub sample_rate: u32,
    pub channels: u16,
    /// Signed 16-bit PCM, interleaved in stereo.
    pub samples: Vec<i16>,
}

/// Decodes a whole APC file. As the original's decode loop FUN_004101ce,
/// stops after the header's frame count (the files hold one more byte of
/// data), or at the end of the data if it comes first.
pub fn decode_apc(data: &[u8]) -> Result<Apc, DecodeError> {
    let header = ApcHeader::parse(data)?;
    let channels = header.channels();
    let wanted = header.frames as usize * usize::from(channels);
    let body = &data[APC_HEADER_SIZE..];
    // Each byte holds two samples: decode no more bytes than needed.
    let body = &body[..body.len().min(wanted.div_ceil(2))];
    let mut samples = Vec::with_capacity(wanted);
    ApcDecoder::new(&header).decode(body, &mut samples);
    samples.truncate(wanted);
    Ok(Apc {
        sample_rate: header.sample_rate,
        channels,
        samples,
    })
}
