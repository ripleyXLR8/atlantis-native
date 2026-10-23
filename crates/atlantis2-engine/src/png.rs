//! A small PNG writer for screenshots: 8-bit RGB, deflate's stored blocks
//! (no compression), so it needs no library.

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// The RGB values of an RGB565 pixel, each channel scaled to 0..255.
pub fn rgb565_to_rgb(p: u16) -> [u8; 3] {
    let (r, g, b) = (
        u32::from(p >> 11),
        u32::from((p >> 5) & 63),
        u32::from(p & 31),
    );
    [
        (r * 255 / 31) as u8,
        (g * 255 / 63) as u8,
        (b * 255 / 31) as u8,
    ]
}

/// A PNG file of a `width` x `height` RGB565 picture.
pub fn encode_rgb565(pixels: &[u16], width: usize, height: usize) -> Vec<u8> {
    assert!(pixels.len() >= width * height);
    encode_rgb(width, height, |row, raw| {
        for &p in &pixels[row * width..][..width] {
            raw.extend_from_slice(&rgb565_to_rgb(p));
        }
    })
}

/// A PNG file of a `width` x `height` XRGB8888 picture (the HD mode's).
pub fn encode_xrgb8888(pixels: &[u32], width: usize, height: usize) -> Vec<u8> {
    assert!(pixels.len() >= width * height);
    encode_rgb(width, height, |row, raw| {
        for &p in &pixels[row * width..][..width] {
            raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    })
}

/// A PNG file of 8-bit RGB rows, `row(y, raw)` appending row y's bytes.
fn encode_rgb(width: usize, height: usize, row: impl Fn(usize, &mut Vec<u8>)) -> Vec<u8> {
    let mut raw = Vec::with_capacity(height * (1 + 3 * width));
    for y in 0..height {
        raw.push(0); // filter: none
        row(y, &mut raw);
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_and_layout() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(rgb565_to_rgb(0xffff), [255, 255, 255]);
        let png = encode_rgb565(&[0xf800, 0x07e0, 0x001f, 0], 2, 2);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
