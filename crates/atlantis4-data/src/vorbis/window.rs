//! The Vorbis window (specification § 1.3.2.3): `sin(π/2 · sin²((i + ½) / h ·
//! π/2))` for the `h` = block size / 2 rising values. libvorbis 1.2.0 has
//! them as float literals printed to about ten digits (`window.c`), which
//! differ by one unit in the last place from the correctly rounded value
//! for 151 of the 8,160 entries: those are listed here with libvorbis's
//! bits, and a test checks the whole table's hash against libvorbis's.

use std::sync::OnceLock;

/// `(table, index, bits)` where libvorbis's literal is not the computed
/// value; table `t` is the window of block size `64 << t`.
#[rustfmt::skip]
const EXCEPTIONS: [(usize, usize, u32); 151] = [
    (0, 5, 0x3de45df1), (1, 0, 0x397809a8), (1, 1, 0x3b0b7711), (1, 5, 0x3ce9085e),
    (2, 0, 0x38780c05), (3, 0, 0x37780ca9), (3, 1, 0x390b8636), (3, 2, 0x39c1c626),
    (3, 3, 0x3a3de25e), (3, 5, 0x3aea6555), (4, 0, 0x36780c3c), (4, 1, 0x380b86fd),
    (4, 2, 0x38c1c913), (4, 3, 0x393de7f8), (4, 4, 0x399cf594), (4, 5, 0x39ea7673),
    (4, 6, 0x3a23baee), (4, 16, 0x3b83c8e9), (4, 29, 0x3c5239c2), (4, 45, 0x3cf9150e),
    (4, 181, 0x3ed97ffc), (5, 0, 0x35780fab), (5, 1, 0x370b8718), (5, 2, 0x37c1c9e1),
    (5, 3, 0x383de96b), (5, 4, 0x389cf780), (5, 5, 0x38ea7abb), (5, 6, 0x3923bf18),
    (5, 7, 0x395a00d5), (5, 8, 0x398c0138), (5, 10, 0x39d5a258), (5, 11, 0x3a00213c),
    (5, 13, 0x3a3090af), (5, 14, 0x3a4baff3), (5, 19, 0x3ab82a77), (5, 21, 0x3adfddcc),
    (5, 31, 0x3b702bf1), (5, 38, 0x3bb3516a), (5, 77, 0x3cb4ff2b), (5, 95, 0x3d09116d),
    (6, 0, 0x347808cc), (6, 1, 0x360b86ab), (6, 2, 0x36c1ca4f), (6, 3, 0x373de9be),
    (6, 4, 0x379cf7ee), (6, 5, 0x37ea7bc0), (6, 6, 0x3823c02b), (6, 7, 0x385a02a1),
    (6, 8, 0x388c02bd), (6, 9, 0x38aee44c), (6, 10, 0x38d5a5e3), (6, 11, 0x390023c7),
    (6, 12, 0x391764a8), (6, 13, 0x39309586), (6, 15, 0x3968c740), (6, 16, 0x3983e407),
    (6, 17, 0x39945c69), (6, 18, 0x39a5ccbf), (6, 19, 0x39b83506), (6, 21, 0x39dfed69),
    (6, 22, 0x39f53d78), (6, 23, 0x3a05c2b8), (6, 24, 0x3a1162a6), (6, 25, 0x3a1d7e86),
    (6, 28, 0x3a44b9ac), (6, 30, 0x3a614ca5), (6, 38, 0x3ab379ae), (6, 40, 0x3ac699ab),
    (6, 44, 0x3aefc04f), (6, 46, 0x3b02e36d), (6, 97, 0x3c0fa713), (6, 131, 0x3c8273a1),
    (6, 204, 0x3d1cf484), (6, 316, 0x3db99fa0), (6, 1196, 0x3f5622cc), (7, 0, 0x33783fc6),
    (7, 1, 0x350b8862), (7, 2, 0x35c1c897), (7, 3, 0x363de950), (7, 4, 0x369cf7b7),
    (7, 5, 0x36ea7c2e), (7, 6, 0x3723c099), (7, 7, 0x375a02f4), (7, 8, 0x378c0338),
    (7, 9, 0x37aee4e3), (7, 10, 0x37d5a6b1), (7, 11, 0x3800246c), (7, 12, 0x38176592),
    (7, 13, 0x383096c9), (7, 14, 0x384bb812), (7, 15, 0x3868c951), (7, 16, 0x3883e55e),
    (7, 17, 0x38945e1d), (7, 18, 0x38a5cee5), (7, 19, 0x38b837a7), (7, 20, 0x38cb9880),
    (7, 21, 0x38dff155), (7, 22, 0x38f54224), (7, 23, 0x3905c57e), (7, 24, 0x391165ee),
    (7, 25, 0x391d8263), (7, 26, 0x392a1acf), (7, 27, 0x39372f3f), (7, 28, 0x3944bfb3),
    (7, 29, 0x3952cc1e), (7, 30, 0x3961548d), (7, 31, 0x397058f3), (7, 33, 0x3987eae3),
    (7, 34, 0x39902712), (7, 35, 0x3998a140), (7, 36, 0x39a15969), (7, 38, 0x39b383b5),
    (7, 39, 0x39bcf5d7), (7, 40, 0x39c6a5f5), (7, 41, 0x39d0940e), (7, 42, 0x39dac022),
    (7, 43, 0x39e52a2e), (7, 44, 0x39efd239), (7, 45, 0x39fab83c), (7, 54, 0x3a33d97c),
    (7, 55, 0x3a3a8253), (7, 64, 0x3a7be3c4), (7, 68, 0x3a8e0bee), (7, 72, 0x3a9f1dae),
    (7, 78, 0x3aba88a9), (7, 80, 0x3ac42821), (7, 81, 0x3ac90f13), (7, 82, 0x3ace057e),
    (7, 115, 0x3b49d60c), (7, 116, 0x3b4d580a), (7, 125, 0x3b6e4552), (7, 126, 0x3b72147f),
    (7, 134, 0x3b88d1c6), (7, 151, 0x3bad8cc1), (7, 154, 0x3bb47ba3), (7, 162, 0x3bc7a1f2),
    (7, 165, 0x3bcf0fc4), (7, 233, 0x3c4dceaf), (7, 240, 0x3c5a4bb3), (7, 252, 0x3c708d51),
    (7, 267, 0x3c86f013), (7, 317, 0x3cbdd0b1), (7, 323, 0x3cc504c1), (7, 341, 0x3cdb6b56),
    (7, 425, 0x3d29c0a8), (7, 964, 0x3e50bbe1), (7, 1225, 0x3ea219c4), (7, 1245, 0x3ea6d817),
    (7, 1499, 0x3ee56e34), (7, 1542, 0x3ef043c4), (7, 1979, 0x3f2d66c1),
];

fn tables() -> &'static [Vec<f32>; 8] {
    static T: OnceLock<[Vec<f32>; 8]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t: [Vec<f32>; 8] = Default::default();
        for (k, tab) in t.iter_mut().enumerate() {
            let h = 32usize << k;
            *tab = (0..h)
                .map(|i| {
                    let s = libm::sin((i as f64 + 0.5) / h as f64 * std::f64::consts::PI / 2.0);
                    libm::sin(std::f64::consts::PI / 2.0 * s * s) as f32
                })
                .collect();
        }
        for &(k, i, bits) in &EXCEPTIONS {
            t[k][i] = f32::from_bits(bits);
        }
        t
    })
}

/// The rising half of the window of a block of `blocksize` samples
/// (`blocksize / 2` values), `None` outside 64..=8192.
pub(crate) fn window(blocksize: usize) -> Option<&'static [f32]> {
    if !(64..=8192).contains(&blocksize) || !blocksize.is_power_of_two() {
        return None;
    }
    Some(&tables()[(blocksize.trailing_zeros() - 6) as usize])
}

#[cfg(test)]
mod tests {
    use md5::{Digest, Md5};

    #[test]
    fn the_windows_are_libvorbis_1_2_0s() {
        let mut h = Md5::new();
        for t in super::tables() {
            for v in t {
                h.update(v.to_le_bytes());
            }
        }
        // MD5 of libvorbis 1.2.0's vwin64 … vwin8192 as little-endian floats.
        assert_eq!(
            format!("{:x}", h.finalize()),
            "51b7ded94e3585b99c8ee02715a563ed"
        );
        let mut h = Md5::new();
        for v in super::super::floor1::FLOOR1_FROMDB {
            h.update(v.to_le_bytes());
        }
        // MD5 of libvorbis 1.2.0's FLOOR1_fromdB_LOOKUP.
        assert_eq!(
            format!("{:x}", h.finalize()),
            "4be72a4dc3512b379ff3de3204e7e21e"
        );
    }
}
