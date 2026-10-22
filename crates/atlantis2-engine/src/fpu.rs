//! The x87 control word the original's float code runs under.
//!
//! The warp's C code (the camera and the `warpC` rasteriser) changes
//! nothing in the x87 control word, so it runs under whatever the process
//! has: 0x027f (53-bit precision, rounding to nearest: the Win32 default
//! the MSVC 6 start-up keeps) or 0x007f (24-bit precision, which Direct3D
//! sets when a device is created without `DDSCL_FPUPRESERVE`, as the game
//! does). Which one the players had is not known.
//!
//! The assembly renderers (`warpP5`, `warpMMX`, `warpPII`) load 0x0e7f
//! (`fldcw` of the word @ 0x005edc78) at their entry and never restore it:
//! 53-bit precision, rounding toward zero. Once one of them has drawn, every
//! float operation of the game runs under it, the camera's included, unless
//! something sets the control word again.
//!
//! Every x87 operation rounds its exact result once, at the precision and
//! in the direction of the control word. At 53 bits to nearest that is the
//! IEEE double Rust computes; at 24 bits to nearest, rounding that double to
//! `f32` gives the same value (the operands are then 24-bit values, and
//! 53 >= 2 x 24 + 2); toward zero, the double is moved one unit toward zero
//! when the exact result is smaller in magnitude, which the exact error of
//! the operation tells (sum and product errors after Knuth and Dekker, the
//! remainder of a quotient or a square root). The x87's wider exponent
//! range does not matter for the values the warp computes. A value stored
//! with `fstp dword` is rounded to `f32` in the control word's direction.
//! The transcendental instructions (`fsin`, `fcos`, `fptan`, `fpatan`) are
//! not affected by the control word.
//!
//! What the original's frames tell: under 0x0e7f its matrices and frames
//! depend on the direction of the float stores and of the quotients; its
//! sums and products, mostly of float operands, are nearly all exact at 53
//! bits, and no frame checked depends on their direction. Those two are
//! checked against exact integer arithmetic in the tests below.

/// The x87 control word: precision and rounding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fpu {
    /// 0x027f: 53-bit mantissa, rounding to nearest.
    #[default]
    Double,
    /// 0x007f: 24-bit mantissa, rounding to nearest.
    Single,
    /// 0x0e7f: 53-bit mantissa, rounding toward zero (the word the assembly
    /// renderers load).
    Chop,
}

/// Moves a finite, non-zero double one unit toward zero.
#[inline(always)]
fn toward_zero(x: f64) -> f64 {
    f64::from_bits(x.to_bits() - 1)
}

/// Whether `s`, the rounded result, lies farther from zero than the exact
/// result `s + err`.
#[inline(always)]
fn away(s: f64, err: f64) -> bool {
    s.is_finite() && err != 0.0 && err.is_finite() && (err < 0.0) != (s < 0.0)
}

/// a · b = p + e exactly (Dekker), for operands far from overflow.
#[inline(always)]
fn two_product(a: f64, b: f64) -> (f64, f64) {
    const SPLIT: f64 = 134_217_729.0; // 2^27 + 1
    let p = a * b;
    let split = |x: f64| {
        let c = SPLIT * x;
        let hi = c - (c - x);
        (hi, x - hi)
    };
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let e = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    (p, e)
}

impl Fpu {
    /// The control word, for messages.
    pub fn control_word(self) -> u16 {
        match self {
            Fpu::Double => 0x027f,
            Fpu::Single => 0x007f,
            Fpu::Chop => 0x0e7f,
        }
    }

    /// a + b (`fadd`).
    #[inline(always)]
    pub fn add(self, a: f64, b: f64) -> f64 {
        let s = a + b;
        match self {
            Fpu::Double => s,
            Fpu::Single => s as f32 as f64,
            Fpu::Chop => {
                // Knuth's two-sum: the exact error of the sum.
                let bb = s - a;
                let err = (a - (s - bb)) + (b - bb);
                if away(s, err) {
                    toward_zero(s)
                } else {
                    s
                }
            }
        }
    }

    /// a − b (`fsub`).
    #[inline(always)]
    pub fn sub(self, a: f64, b: f64) -> f64 {
        self.add(a, -b)
    }

    /// a · b (`fmul`).
    #[inline(always)]
    pub fn mul(self, a: f64, b: f64) -> f64 {
        match self {
            Fpu::Double => a * b,
            Fpu::Single => (a * b) as f32 as f64,
            Fpu::Chop => {
                let (p, e) = two_product(a, b);
                if away(p, e) {
                    toward_zero(p)
                } else {
                    p
                }
            }
        }
    }

    /// a / b (`fdiv`).
    #[inline(always)]
    pub fn div(self, a: f64, b: f64) -> f64 {
        let q = a / b;
        match self {
            Fpu::Double => q,
            Fpu::Single => q as f32 as f64,
            Fpu::Chop => {
                if !q.is_finite() || q == 0.0 || !b.is_finite() {
                    return q;
                }
                // The remainder a − q·b is a double (q correctly rounded);
                // the exact quotient is q + rem / b.
                let (p, e) = two_product(q, b);
                let rem = (a - p) - e;
                if rem != 0.0 && ((rem < 0.0) != (b < 0.0)) != (q < 0.0) {
                    toward_zero(q)
                } else {
                    q
                }
            }
        }
    }

    /// √a (`fsqrt`).
    #[inline(always)]
    pub fn sqrt(self, a: f64) -> f64 {
        let s = a.sqrt();
        match self {
            Fpu::Double => s,
            Fpu::Single => s as f32 as f64,
            Fpu::Chop => {
                if !s.is_finite() || s == 0.0 {
                    return s;
                }
                let (p, e) = two_product(s, s);
                if (a - p) - e < 0.0 {
                    toward_zero(s)
                } else {
                    s
                }
            }
        }
    }

    /// A value stored to memory as a float (`fst dword` / `fstp dword`).
    #[inline(always)]
    pub fn f(self, x: f64) -> f64 {
        self.f32(x) as f64
    }

    /// The same, as an `f32`.
    #[inline(always)]
    pub fn f32(self, x: f64) -> f32 {
        let y = x as f32;
        if self != Fpu::Chop || x.is_nan() {
            return y;
        }
        if y.is_infinite() && x.is_finite() {
            return f32::MAX.copysign(y);
        }
        if y != 0.0 && y.is_finite() && f64::from(y).abs() > x.abs() {
            f32::from_bits(y.to_bits() - 1)
        } else {
            y
        }
    }

    /// `fistp dword` under this control word: rounding toward zero for
    /// [`Fpu::Chop`] (the only word the assembly runs its `fistp` under),
    /// to nearest even otherwise; out of the 32-bit range, and for a NaN,
    /// the integer indefinite 0x8000_0000 (invalid operation masked).
    #[inline(always)]
    pub fn fistp(self, x: f64) -> i32 {
        let t = match self {
            Fpu::Chop => x.trunc(),
            _ => x.round_ties_even(),
        };
        if t.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&t) {
            i32::MIN
        } else {
            t as i32
        }
    }
}

/// A float constant of the data section as an `f64` operand.
#[inline(always)]
pub fn f(x: f64) -> f64 {
    x as f32 as f64
}

/// 2^63, the first value out of a 64-bit `fistp`'s range.
const TWO_POW_63: f64 = (1u64 << 63) as f64;

/// MSVC's `_ftol` (@ 0x005ad870): truncation toward zero (it sets the
/// rounding itself, whatever the control word), the low dword of a 64-bit
/// `fistp`. Out of the 64-bit range, and for a NaN, the x87 stores the
/// integer indefinite 0x8000_0000_0000_0000 (invalid operation masked),
/// whose low dword is 0; Rust's saturating cast would give 0xffff_ffff for
/// values of 2^63 and above.
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

    /// The next double toward zero.
    fn below(x: f64) -> f64 {
        toward_zero(x)
    }

    #[test]
    fn chop_rounds_every_operation_toward_zero() {
        let c = Fpu::Chop;
        // 1/10 rounded to nearest is above the exact value (…999a), so
        // chop is one unit below; 1/3 rounded to nearest is below (…5555),
        // so it is kept.
        let tenth = 1.0f64 / 10.0;
        assert_eq!(tenth.to_bits() & 0xf, 0xa);
        assert_eq!(c.div(1.0, 10.0), below(tenth));
        assert_eq!(c.div(-1.0, 10.0), -below(tenth));
        assert_eq!(c.div(1.0, -10.0), -below(tenth));
        assert_eq!(c.div(-1.0, -10.0), below(tenth));
        assert_eq!(c.div(1.0, 3.0), 1.0 / 3.0);
        assert_eq!(c.div(-1.0, 3.0), -1.0 / 3.0);
        // Exact results are kept.
        assert_eq!(c.div(1.0, 4.0), 0.25);
        assert_eq!(c.add(0.5, 0.25), 0.75);
        assert_eq!(c.mul(3.0, 7.0), 21.0);
        assert_eq!(c.sqrt(16.0), 4.0);
        // 1 + 2^-53 lies halfway: nearest-even gives 1, chop gives 1 too;
        // 1 + 3·2^-54 is above halfway: nearest gives 1 + 2^-52, chop 1.
        let e = f64::EPSILON;
        assert_eq!(Fpu::Double.add(1.0, 0.75 * e), 1.0 + e);
        assert_eq!(c.add(1.0, 0.75 * e), 1.0);
        assert_eq!(c.add(-1.0, -0.75 * e), -1.0);
        assert_eq!(c.sub(1.0, 0.25 * e), below(1.0));
        // (1 + 2^-52)² = 1 + 2^-51 + 2^-104: chop drops the last term.
        assert_eq!(c.mul(1.0 + e, 1.0 + e), 1.0 + 2.0 * e);
        // 3 · 0.1 rounded to nearest (0.30000000000000004) is above the
        // exact product.
        assert_eq!(Fpu::Double.mul(0.1, 3.0), 0.30000000000000004);
        assert_eq!(c.mul(0.1, 3.0), below(0.30000000000000004));
        assert_eq!(c.mul(-0.1, 3.0), -below(0.30000000000000004));
        // √2 rounded to nearest is above the exact value.
        let s2 = 2f64.sqrt();
        assert!(s2 * s2 > 2.0);
        assert_eq!(c.sqrt(2.0), below(s2));
        // √3 rounded to nearest is below it.
        let s3 = 3f64.sqrt();
        let (p3, e3) = two_product(s3, s3);
        assert!(p3 < 3.0 || (p3 == 3.0 && e3 < 0.0));
        assert_eq!(c.sqrt(3.0), s3);
    }

    #[test]
    fn chop_agrees_with_exact_integer_arithmetic() {
        // Products and quotients of integers checked in i128: the chopped
        // result is the largest double not above the exact value.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20000 {
            let a = (next() >> 11) as i64 | 1; // up to 53 bits
            let b = (next() >> 40) as i64 | 1; // up to 24 bits
            let (fa, fb) = (a as f64, b as f64);
            let exact = a as i128 * b as i128;
            let p = Fpu::Chop.mul(fa, fb);
            assert!(p as i128 <= exact && up(p) as i128 > exact, "{a} * {b}");
            let q = Fpu::Chop.div(fa, fb);
            assert!(le_ratio(q, a, b) && !le_ratio(up(q), a, b), "{a} / {b}");
            let s = Fpu::Chop.sqrt(fa);
            let (p1, e1) = two_product(s, s);
            assert!(p1 < fa || (p1 == fa && e1 <= 0.0), "sqrt {a}");
            let (p2, e2) = two_product(up(s), up(s));
            assert!(p2 > fa || (p2 == fa && e2 > 0.0), "sqrt {a}");
        }
    }

    /// The next double away from zero.
    fn up(x: f64) -> f64 {
        f64::from_bits(x.to_bits() + 1)
    }

    /// Whether q <= a / b exactly (positive operands).
    fn le_ratio(q: f64, a: i64, b: i64) -> bool {
        let bits = q.to_bits();
        let e = ((bits >> 52) & 0x7ff) as i32 - 1075;
        let m = ((bits & ((1 << 52) - 1)) | (1 << 52)) as i128;
        // m * 2^e * b <= a
        if e >= 0 {
            (m * b as i128) << e <= a as i128
        } else {
            m * b as i128 <= (a as i128) << -e
        }
    }

    #[test]
    fn stores_and_integer_stores_follow_the_rounding() {
        let c = Fpu::Chop;
        // 0.1 as a double is above 0.1's float, so both roundings agree
        // only when the nearest float is below.
        let x = 1.0 + f64::from(f32::EPSILON) * 0.75;
        assert_eq!(Fpu::Double.f32(x), 1.0 + f32::EPSILON);
        assert_eq!(c.f32(x), 1.0);
        assert_eq!(c.f32(-x), -1.0);
        assert_eq!(c.f32(1e39), f32::MAX);
        assert_eq!(c.fistp(2.9), 2);
        assert_eq!(c.fistp(-2.9), -2);
        assert_eq!(Fpu::Double.fistp(2.5), 2);
        assert_eq!(Fpu::Double.fistp(3.5), 4);
        assert_eq!(c.fistp(2147483647.9), i32::MAX);
        assert_eq!(c.fistp(2147483648.0), i32::MIN);
        assert_eq!(c.fistp(-2147483648.9), i32::MIN);
        assert_eq!(c.fistp(-2147483649.0), i32::MIN);
        assert_eq!(c.fistp(f64::NAN), i32::MIN);
    }

    #[test]
    fn single_rounds_every_result_to_a_float() {
        let s = Fpu::Single;
        assert_eq!(s.div(1.0, 3.0), f64::from(1f32 / 3.0));
        assert_eq!(s.add(1.0, 1e-9), 1.0);
        assert_eq!(s.sqrt(2.0), f64::from(2f32.sqrt()));
    }
}
