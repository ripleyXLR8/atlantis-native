// Ported from ScummVM's `video/bink_decoder.cpp` (`initAudioTrack`,
// `BinkAudioTrack`) and `video/binkdata.h` (ScummVM - Graphic Adventure
// Engine, Copyright (C) the ScummVM team, GNU General Public License,
// version 3 or later). ScummVM's decoder is based on eos' Bink decoder,
// itself based heavily on FFmpeg's Bink decoders by Kostya Shishkov.

//! Bink audio: a track's set-up from its header (sample rate, flags) and
//! its packets decoded to 16-bit samples, DCT or RDFT codec, as ScummVM's
//! `BinkAudioTrack` does.

use super::bits::BitReader;
use super::transform::{Dct3, Rdft, RdftKind};

/// The header's audio flags (`kAudioFlagDCT`, `kAudioFlagStereo`).
pub const AUDIO_FLAG_DCT: u16 = 0x1000;
pub const AUDIO_FLAG_STEREO: u16 = 0x2000;

/// `binkCriticalFreqs`.
const CRITICAL_FREQS: [u32; 25] = [
    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700,
    4400, 5300, 6400, 7700, 9500, 12000, 15500, 24500,
];

/// `rleLengthTab`.
const RLE_LENGTHS: [u32; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

enum Codec {
    Dct(Dct3),
    Rdft(Rdft),
}

/// One audio track (`AudioInfo` and its `BinkAudioTrack`).
pub struct AudioTrack {
    /// The output's rate and channels.
    pub out_rate: u32,
    pub out_channels: u16,
    /// The codec's channels (1 for RDFT, whose frames hold both).
    channels: usize,
    codec: Codec,
    first: bool,
    frame_len: usize,
    overlap_len: usize,
    block_size: usize,
    bands: Vec<usize>,
    root: f32,
    coeffs: Vec<f32>,
    prev: Vec<i16>,
}

impl AudioTrack {
    /// `initAudioTrack` for a track of `sample_rate` and `flags`.
    pub fn new(sample_rate: u16, flags: u16) -> AudioTrack {
        let mut sample_rate = u32::from(sample_rate);
        let mut channels = if flags & AUDIO_FLAG_STEREO != 0 { 2 } else { 1 };
        let dct = flags & AUDIO_FLAG_DCT != 0;
        let mut frame_len_bits = if sample_rate < 22050 {
            9
        } else if sample_rate < 44100 {
            10
        } else {
            11
        };
        let mut frame_len = 1usize << frame_len_bits;
        let (out_rate, out_channels) = (sample_rate, channels as u16);
        if !dct {
            // RDFT audio already interleaves the samples correctly.
            if channels == 2 {
                frame_len_bits += 1;
            }
            sample_rate *= channels as u32;
            frame_len *= channels;
            channels = 1;
        }
        let overlap_len = frame_len / 16;
        let block_size = (frame_len - overlap_len) * channels;
        let root = (2.0 / (frame_len as f64).sqrt()) as f32;
        let half = sample_rate.div_ceil(2);
        let mut band_count = 1;
        while band_count < 25 {
            if half <= CRITICAL_FREQS[band_count - 1] {
                break;
            }
            band_count += 1;
        }
        let mut bands = vec![0usize; band_count + 1];
        bands[0] = 1;
        for (i, b) in bands.iter_mut().enumerate().take(band_count).skip(1) {
            *b = (CRITICAL_FREQS[i - 1] as usize * (frame_len / 2)) / half as usize;
        }
        bands[band_count] = frame_len / 2;
        let codec = if dct {
            Codec::Dct(Dct3::new(frame_len_bits))
        } else {
            Codec::Rdft(Rdft::new(frame_len_bits, RdftKind::DftC2r))
        };
        AudioTrack {
            out_rate,
            out_channels,
            channels,
            codec,
            first: true,
            frame_len,
            overlap_len,
            block_size,
            bands,
            root,
            coeffs: vec![0.0; frame_len * channels],
            prev: vec![0; overlap_len * channels],
        }
    }

    /// `decodePacket` on a packet's bytes after its sample count: the
    /// blocks' samples (interleaved) appended to `out`.
    pub fn decode_packet(&mut self, packet: &[u8], out: &mut Vec<i16>) {
        let mut bits = BitReader::new(packet);
        while bits.pos() < bits.size() {
            let mut block = vec![0i16; self.frame_len * self.channels];
            self.block(&mut bits, &mut block);
            out.extend_from_slice(&block[..self.block_size]);
            // The next block starts at a 32-bit boundary.
            if bits.pos() & 0x1f != 0 {
                bits.skip(32 - (bits.pos() & 0x1f));
            }
        }
    }

    /// `audioBlock`.
    fn block(&mut self, bits: &mut BitReader<'_>, out: &mut [i16]) {
        let n = self.frame_len;
        match &self.codec {
            Codec::Dct(_) => {
                bits.skip(2);
                for c in 0..self.channels {
                    let mut coeffs = std::mem::take(&mut self.coeffs);
                    let ch = &mut coeffs[c * n..(c + 1) * n];
                    self.read_coeffs(bits, ch);
                    ch[0] = (f64::from(ch[0]) / 0.5) as f32;
                    if let Codec::Dct(d) = &self.codec {
                        d.calc(ch);
                    }
                    for v in ch.iter_mut() {
                        *v = (f64::from(*v) * (n as f64 / 2.0)) as f32;
                    }
                    self.coeffs = coeffs;
                }
            }
            Codec::Rdft(_) => {
                let mut coeffs = std::mem::take(&mut self.coeffs);
                self.read_coeffs(bits, &mut coeffs[..n]);
                if let Codec::Rdft(r) = &self.codec {
                    r.calc(&mut coeffs[..n]);
                }
                self.coeffs = coeffs;
            }
        }
        // `floatToInt16Interleave`.
        let one = |f: f32| (f64::from(f) + 0.5).floor().clamp(-32768.0, 32767.0) as i16;
        if self.channels == 2 {
            for i in 0..n {
                out[2 * i] = one(self.coeffs[i]);
                out[2 * i + 1] = one(self.coeffs[n + i]);
            }
        } else {
            for c in 0..self.channels {
                for i in 0..n {
                    out[i * self.channels + c] = one(self.coeffs[c * n + i]);
                }
            }
        }
        let count = self.overlap_len * self.channels;
        if !self.first {
            let shift = count.ilog2();
            for (i, (o, &p)) in out[..count].iter_mut().zip(&self.prev).enumerate() {
                let v = (i32::from(p) * (count - i) as i32 + i32::from(*o) * i as i32) >> shift;
                *o = v as i16;
            }
        }
        self.prev
            .copy_from_slice(&out[self.block_size..self.block_size + count]);
        self.first = false;
    }

    /// `getFloat`.
    fn get_float(bits: &mut BitReader<'_>) -> f32 {
        let power = bits.get_bits(5) as i32;
        let mantissa = bits.get_bits(23) as f32;
        let f = (f64::from(mantissa) * 2f64.powi(power - 23)) as f32;
        if bits.get_bit() != 0 {
            -f
        } else {
            f
        }
    }

    /// `readAudioCoeffs` into one channel's `frame_len` coefficients.
    fn read_coeffs(&self, bits: &mut BitReader<'_>, coeffs: &mut [f32]) {
        let n = self.frame_len;
        coeffs[0] = Self::get_float(bits) * self.root;
        coeffs[1] = Self::get_float(bits) * self.root;
        let band_count = self.bands.len() - 1;
        let mut quant = [0f32; 25];
        for q in quant.iter_mut().take(band_count) {
            let value = bits.get_bits(8).min(95) as f32;
            // 0.066399999 / log10(M_E); `exp` of a float is C++'s float
            // overload (`expf`).
            *q = (value * 0.152_891_65_f32).exp() * self.root;
        }
        let mut q = 0f32;
        let mut k = 0usize;
        while self.bands[k] < 1 {
            q = quant[k];
            k += 1;
        }
        let mut i = 2usize;
        while i < n {
            let mut j = if bits.get_bit() != 0 {
                i + RLE_LENGTHS[bits.get_bits(4) as usize] as usize * 8
            } else {
                i + 8
            };
            j = j.min(n);
            let width = bits.get_bits(4);
            if width == 0 {
                coeffs[i..j].fill(0.0);
                i = j;
                while self.bands[k] * 2 < i {
                    q = quant[k];
                    k += 1;
                }
            } else {
                while i < j {
                    if self.bands[k] * 2 == i {
                        q = quant[k];
                        k += 1;
                    }
                    let coeff = bits.get_bits(width);
                    coeffs[i] = if coeff != 0 {
                        if bits.get_bit() != 0 {
                            -q * coeff as f32
                        } else {
                            q * coeff as f32
                        }
                    } else {
                        0.0
                    };
                    i += 1;
                }
            }
        }
    }
}
