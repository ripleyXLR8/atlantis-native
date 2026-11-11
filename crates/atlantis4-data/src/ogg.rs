//! Ogg pages and the headers of the streams they carry: Vorbis (`.ogg`
//! sounds), Theora (`.ogv` videos, and `.ogv` entries of the warp
//! packages) and Ogg Skeleton (the first stream of every `.ogv` file).
//!
//! The original links libogg, libvorbis 1.2.0 (`Xiph.Org libVorbis I
//! 20070622`, file +0x1f6764) and libtheora (`th_decode_headerin`,
//! +0x1af510). This module reads the container per RFC 3533 (page CRC
//! checked) and the identification headers per the Vorbis I and Theora
//! specifications; decoding the sound is [`crate::audio`]'s, decoding the
//! pictures is [`crate::video`]'s.

use std::collections::BTreeMap;

/// One page's header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub header_type: u8,
    pub granule: u64,
    pub serial: u32,
    pub sequence: u32,
    /// Where the page starts in the file, and its whole length.
    pub offset: usize,
    pub len: usize,
}

/// Why an Ogg file cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OggError {
    /// No `OggS` where a page should start.
    Capture(usize),
    /// A page runs past the end of the file.
    Truncated(usize),
    /// A page's CRC does not match.
    Crc(usize),
}

impl std::fmt::Display for OggError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OggError::Capture(at) => write!(f, "no Ogg page at byte {at}"),
            OggError::Truncated(at) => write!(f, "Ogg page at byte {at} truncated"),
            OggError::Crc(at) => write!(f, "Ogg page at byte {at}: wrong CRC"),
        }
    }
}

impl std::error::Error for OggError {}

/// The CRC of Ogg pages: polynomial 0x04c11db7, no reflection, initial
/// value 0, the CRC field taken as zero.
fn crc(data: &[u8], crc_at: usize) -> u32 {
    let mut c = 0u32;
    for (i, &b) in data.iter().enumerate() {
        let b = if (crc_at..crc_at + 4).contains(&i) {
            0
        } else {
            b
        };
        c ^= u32::from(b) << 24;
        for _ in 0..8 {
            c = if c & 0x8000_0000 != 0 {
                (c << 1) ^ 0x04c1_1db7
            } else {
                c << 1
            };
        }
    }
    c
}

/// The packets of each stream of a file, by serial number.
pub type Packets = BTreeMap<u32, Vec<Vec<u8>>>;

/// Every page of an Ogg file and the packets of each stream, complete
/// packets only, the first `max_packets` of each.
pub fn read(data: &[u8], max_packets: usize) -> Result<(Vec<Page>, Packets), OggError> {
    let mut pages = Vec::new();
    let mut packets = Packets::new();
    let mut partial = BTreeMap::<u32, Vec<u8>>::new();
    let mut at = 0;
    while at < data.len() {
        let h = data.get(at..at + 27).ok_or(OggError::Truncated(at))?;
        if &h[..4] != b"OggS" || h[4] != 0 {
            return Err(OggError::Capture(at));
        }
        let header_type = h[5];
        let granule = u64::from_le_bytes(h[6..14].try_into().unwrap_or_default());
        let serial = u32::from_le_bytes([h[14], h[15], h[16], h[17]]);
        let sequence = u32::from_le_bytes([h[18], h[19], h[20], h[21]]);
        let stored_crc = u32::from_le_bytes([h[22], h[23], h[24], h[25]]);
        let nseg = usize::from(h[26]);
        let lacing = data
            .get(at + 27..at + 27 + nseg)
            .ok_or(OggError::Truncated(at))?;
        let body_len: usize = lacing.iter().map(|&l| usize::from(l)).sum();
        let len = 27 + nseg + body_len;
        let page = data.get(at..at + len).ok_or(OggError::Truncated(at))?;
        if crc(page, 22) != stored_crc {
            return Err(OggError::Crc(at));
        }
        let mut body = &page[27 + nseg..];
        let cur = partial.entry(serial).or_default();
        for &l in lacing {
            let l = usize::from(l);
            cur.extend_from_slice(&body[..l]);
            body = &body[l..];
            if l < 255 {
                let done = std::mem::take(cur);
                let list = packets.entry(serial).or_default();
                if list.len() < max_packets {
                    list.push(done);
                }
            }
        }
        pages.push(Page {
            header_type,
            granule,
            serial,
            sequence,
            offset: at,
            len,
        });
        at += len;
    }
    Ok((pages, packets))
}

/// What the first packet of a stream says it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamKind {
    Vorbis(VorbisInfo),
    Theora(TheoraInfo),
    Skeleton,
    Unknown,
}

/// The Vorbis identification header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VorbisInfo {
    pub channels: u8,
    pub sample_rate: u32,
}

/// The Theora identification header (all fields big-endian bit fields).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TheoraInfo {
    /// `VMAJ`, `VMIN`, `VREV`.
    pub version: (u8, u8, u8),
    /// Frame size in macroblocks × 16.
    pub frame_width: u32,
    pub frame_height: u32,
    /// The picture region.
    pub picture_width: u32,
    pub picture_height: u32,
    pub picture_x: u8,
    pub picture_y: u8,
    /// Frame rate numerator and denominator.
    pub fps: (u32, u32),
    /// 0 = 4:2:0, 2 = 4:2:2, 3 = 4:4:4.
    pub pixel_format: u8,
    /// `KFGSHIFT`.
    pub keyframe_shift: u8,
}

fn be(b: &[u8]) -> u32 {
    b.iter().fold(0, |v, &x| (v << 8) | u32::from(x))
}

/// The kind of a stream from its first packet.
pub fn identify(first: &[u8]) -> StreamKind {
    if first.len() >= 30 && first.starts_with(b"\x01vorbis") {
        return StreamKind::Vorbis(VorbisInfo {
            channels: first[11],
            sample_rate: u32::from_le_bytes([first[12], first[13], first[14], first[15]]),
        });
    }
    if first.len() >= 42 && first.starts_with(b"\x80theora") {
        let p = first;
        return StreamKind::Theora(TheoraInfo {
            version: (p[7], p[8], p[9]),
            frame_width: be(&p[10..12]) * 16,
            frame_height: be(&p[12..14]) * 16,
            picture_width: be(&p[14..17]),
            picture_height: be(&p[17..20]),
            picture_x: p[20],
            picture_y: p[21],
            fps: (be(&p[22..26]), be(&p[26..30])),
            // Byte 40: QUAL (6 bits) then the top 2 bits of KFGSHIFT;
            // byte 41: the 3 low bits of KFGSHIFT, PF (2 bits), 3 reserved.
            keyframe_shift: ((p[40] & 0x03) << 3) | (p[41] >> 5),
            pixel_format: (p[41] >> 3) & 0x03,
        });
    }
    if first.starts_with(b"fishead\0") {
        return StreamKind::Skeleton;
    }
    StreamKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(serial: u32, seq: u32, header_type: u8, body: &[u8]) -> Vec<u8> {
        let mut p = b"OggS\0".to_vec();
        p.push(header_type);
        p.extend(0u64.to_le_bytes());
        p.extend(serial.to_le_bytes());
        p.extend(seq.to_le_bytes());
        p.extend([0; 4]);
        let mut lacing = vec![255u8; body.len() / 255];
        lacing.push((body.len() % 255) as u8);
        p.push(lacing.len() as u8);
        p.extend(lacing);
        p.extend(body);
        let c = crc(&p, 22);
        p[22..26].copy_from_slice(&c.to_le_bytes());
        p
    }

    #[test]
    fn pages_carry_packets_and_their_crc_is_checked() {
        let mut ident = b"\x01vorbis".to_vec();
        ident.extend([0, 0, 0, 0, 2]);
        ident.extend(44_100u32.to_le_bytes());
        ident.resize(30, 0);
        let long = vec![7u8; 300];
        let mut file = page(5, 0, 2, &ident);
        file.extend(page(5, 1, 0, &long));
        let (pages, packets) = read(&file, 8).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(packets[&5][1], long);
        assert_eq!(
            identify(&packets[&5][0]),
            StreamKind::Vorbis(VorbisInfo {
                channels: 2,
                sample_rate: 44_100
            })
        );
        let mut bad = file.clone();
        bad[40] ^= 1;
        assert_eq!(read(&bad, 8).unwrap_err(), OggError::Crc(0));
    }

    #[test]
    fn a_theora_header_gives_its_picture() {
        let mut p = b"\x80theora".to_vec();
        p.extend([3, 2, 1]);
        p.extend([0, 50, 0, 28]); // 800 x 448 frame
        p.extend([0, 3, 0x20, 0, 1, 0xc0, 0, 0]); // picture 800 x 448 at 0, 0
        p.extend(25u32.to_be_bytes());
        p.extend(1u32.to_be_bytes());
        p.extend([0; 6]); // aspect
        p.push(0); // colour space
        p.extend([0; 3]); // bitrate
        p.push(48 << 2); // quality 48, KFGSHIFT top bits 0
        p.push(6 << 5); // KFGSHIFT low bits 6, pixel format 0 (4:2:0)
        let StreamKind::Theora(t) = identify(&p) else {
            panic!()
        };
        assert_eq!((t.frame_width, t.frame_height), (800, 448));
        assert_eq!((t.picture_width, t.picture_height), (800, 448));
        assert_eq!((t.fps, t.pixel_format, t.keyframe_shift), ((25, 1), 0, 6));
        assert_eq!(identify(b"fishead\0rest"), StreamKind::Skeleton);
    }
}
