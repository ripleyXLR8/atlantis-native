//! Pieces of the MSVC 6 C runtime whose exact results the game depends
//! on.

/// `rand` / `srand` (@ 0x005ae42a): the MSVC linear congruential
/// generator, one global state. The dialog seeds it with `srand(time(0))`
/// when the dialog object is built (@ 0x0045d7b0), so a port that wants
/// the original's choices (eye blinks, the idle mouth, the camera of a
/// line) seeds it as the original was seeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rand {
    pub state: u32,
}

impl Rand {
    /// `srand(seed)`.
    pub fn new(seed: u32) -> Rand {
        Rand { state: seed }
    }

    /// `rand()`: 0..=32767.
    pub fn rand(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
        ((self.state >> 16) & 0x7fff) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msvc_sequence() {
        // The well-known first values of MSVC's rand() after srand(1).
        let mut r = Rand::new(1);
        assert_eq!([r.rand(), r.rand(), r.rand()], [41, 18467, 6334]);
    }
}
