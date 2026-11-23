//! The bar of *The Secrets of Atlantis*' first start from the disc's
//! cabinets: the films' folder decoded up to the logos (about 460 MB with
//! the French DVD), drawn on the black screen until the game starts. The
//! port's own (the original plays from its install).

/// Draws the bar into `picture` (`width` × `height`, 0RGB): a third of
/// the screen wide, in its middle, `at` of `to` filled.
pub(crate) fn draw(picture: &mut [u32], width: usize, height: usize, at: u64, to: u64) {
    picture.fill(0);
    let (w, h) = (width / 3, 6);
    let (x0, y0) = ((width - w) / 2, (height - h) / 2);
    let filled = (w as u64 * at.min(to)).checked_div(to).unwrap_or(0) as usize;
    for y in y0..y0 + h {
        let row = &mut picture[y * width + x0..][..w];
        for (i, p) in row.iter_mut().enumerate() {
            *p = if i < filled { 0x00c8_a050 } else { 0x0030_2818 };
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn half_filled() {
        let (w, h) = (30, 12);
        let mut pic = vec![0xffff_ffff; w * h];
        super::draw(&mut pic, w, h, 5, 10);
        let row = &pic[3 * w..4 * w];
        assert_eq!(&row[10..15], &[0x00c8_a050; 5]);
        assert_eq!(&row[15..20], &[0x0030_2818; 5]);
        assert_eq!(row[9], 0);
        assert!(pic[..3 * w].iter().all(|&p| p == 0));
        // Nothing known yet: an empty bar.
        super::draw(&mut pic, w, h, 0, 0);
        assert_eq!(pic[3 * w + 10], 0x0030_2818);
    }
}
