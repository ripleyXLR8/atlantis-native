//! The macOS Dock icon: macOS shows the application's icon, not the
//! window's, so the icon of the player's atlantis.exe is handed to
//! NSApplication at run time (the app bundle we distribute has none of
//! the game's pictures).

use atlantis_data::exe_icon::Icon;

/// `icon` enlarged `scale` times (nearest: the 32x32 art stays sharp) as
/// an ICO file holding one 32-bit image (BITMAPINFOHEADER, BGRA rows from
/// the bottom, an empty AND mask), which NSImage reads.
pub fn to_ico(icon: &Icon, scale: u32) -> Vec<u8> {
    let (w, h) = (icon.width * scale, icon.height * scale);
    let pixels = (w * h) as usize;
    let mask_stride = w.div_ceil(32) as usize * 4;
    let dib_len = 40 + pixels * 4 + mask_stride * h as usize;
    let mut out = Vec::with_capacity(22 + dib_len);
    // ICONDIR, one ICONDIRENTRY (a size of 0 stands for 256).
    out.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    out.push(if w >= 256 { 0 } else { w as u8 });
    out.push(if h >= 256 { 0 } else { h as u8 });
    out.extend_from_slice(&[0, 0, 1, 0, 32, 0]);
    out.extend_from_slice(&(dib_len as u32).to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    // BITMAPINFOHEADER: the height counts the XOR and AND halves.
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&(2 * h).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let o = (((y / scale) * icon.width + x / scale) * 4) as usize;
            let p = &icon.rgba[o..o + 4];
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out.resize(out.len() + mask_stride * h as usize, 0);
    out
}

/// Sets the Dock icon (macOS only; from the main thread).
#[cfg(target_os = "macos")]
pub fn set(icon: &Icon) {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(&to_ico(icon, 8));
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        log::warn!("Dock icon: not an image");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    #[allow(unused_unsafe)]
    unsafe {
        app.setApplicationIconImage(Some(&image));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ico_layout() {
        let icon = Icon {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0],
        };
        let ico = to_ico(&icon, 2);
        // 4x2 pixels, a 4-byte mask row per pixel row.
        assert_eq!(ico.len(), 22 + 40 + 4 * 2 * 4 + 4 * 2);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 1, 0]);
        assert_eq!((ico[6], ico[7]), (4, 2));
        assert_eq!(
            u32::from_le_bytes(ico[14..18].try_into().unwrap()),
            40 + 32 + 8
        );
        assert_eq!(i32::from_le_bytes(ico[30..34].try_into().unwrap()), 4); // height x 2
                                                                            // The first stored row is the bottom one: red, red, then clear blue.
        let px = &ico[62..];
        assert_eq!(&px[..8], &[0, 0, 255, 255, 0, 0, 255, 255]);
        assert_eq!(&px[8..16], &[255, 0, 0, 0, 255, 0, 0, 0]);
    }
}
