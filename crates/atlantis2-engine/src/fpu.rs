//! The x87 precision the original's float code runs at.
//!
//! The warp's C code (the camera and the `warpC` rasteriser) changes
//! nothing in the x87 control word, so it runs at whatever precision the
//! process has: 53 bits (control word 0x027f, the Win32 default the MSVC 6
//! start-up keeps) or 24 bits (0x007f, which Direct3D sets when a device is
//! created without `DDSCL_FPUPRESERVE`, as the game does). Which one the
//! players had is not known; the two differ by one texel in about 80
//! pixels of a 640 x 480 frame.
//!
//! Not modelled: the control word 0x0e7f (53-bit precision, rounding toward
//! zero) that the assembly renderers (`warpP5`, `warpMMX`, `warpPII`) load
//! and never restore. The reference renderer `warpPII` (atlantis2-re
//! decision 0003) needs it, for its own code and for the camera code that
//! runs after it; this enum gets a third mode with that transcription.
//!
//! Every x87 result is rounded to the precision: at 53 bits that is the
//! IEEE double Rust computes; at 24 bits, rounding that double to `f32`
//! gives the same value (every operand is then a 24-bit value, and 53 >=
//! 2 x 24 + 2). A value stored with `fstp dword` is rounded to `f32`
//! whatever the precision. The transcendental instructions (`fsin`,
//! `fcos`, `fptan`, `fpatan`) are not affected by the precision.

/// The precision of the x87 arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fpu {
    /// Control word 0x027f: 53-bit mantissa.
    #[default]
    Double,
    /// Control word 0x007f: 24-bit mantissa.
    Single,
}

impl Fpu {
    /// The result of an x87 operation at this precision.
    #[inline(always)]
    pub fn r(self, x: f64) -> f64 {
        match self {
            Fpu::Double => x,
            Fpu::Single => x as f32 as f64,
        }
    }

    /// The control word, for messages.
    pub fn control_word(self) -> u16 {
        match self {
            Fpu::Double => 0x027f,
            Fpu::Single => 0x007f,
        }
    }
}

/// A float stored to memory (`fstp dword`).
#[inline(always)]
pub fn f(x: f64) -> f64 {
    x as f32 as f64
}

/// MSVC's `_ftol` (@ 0x005ad870): truncation toward zero, the low dword of
/// a 64-bit `fistp`. Out of the 64-bit range, and for a NaN, the x87 stores
/// the integer indefinite 0x8000_0000_0000_0000 (invalid operation masked),
/// whose low dword is 0; Rust's saturating cast would give 0xffff_ffff for
/// values of 2^63 and above.
/// 2^63, the first value out of a 64-bit `fistp`'s range.
const TWO_POW_63: f64 = (1u64 << 63) as f64;

#[inline(always)]
pub fn ftol(x: f64) -> i32 {
    if x.is_nan() || x.abs() >= TWO_POW_63 {
        return 0;
    }
    x as i64 as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftol_truncates_and_gives_the_indefinite_low_dword_out_of_range() {
        assert_eq!(ftol(2.9), 2);
        assert_eq!(ftol(-2.9), -2);
        assert_eq!(ftol(4294967298.5), 2);
        assert_eq!(ftol(9.3e18), 0);
        assert_eq!(ftol(-9.3e18), 0);
        assert_eq!(ftol(f64::NAN), 0);
        assert_eq!(ftol(f64::INFINITY), 0);
    }
}
