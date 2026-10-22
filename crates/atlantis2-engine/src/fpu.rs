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
/// a 64-bit `fistp`.
#[inline(always)]
pub fn ftol(x: f64) -> i32 {
    x as i64 as i32
}
