//! `vue\ho<from><to>.vue`: the start and end cameras of a travel video
//! `hnmtraj\HNM640\<world>\ho<from><to>.hnm`.
//!
//! **No reader in the executable.** `Game.exe` holds no `.vue` name, no
//! `vue\` folder (only the first game's `ubb_vue\`) and no `Camera` keyword
//! with numbers (string search of `Game.exe`, `spr_p5.dll`, `spr_p6.dll`
//! and `Atlantis3.exe`); the travel video is chosen from the WAM's path
//! records (@ 0x00462a60) and played without any camera file
//! (@ 0x0043ff20). The files look like exports of the 3D tool kept with
//! the videos. They are read here so that a port can use them.
//!
//! Text, two lines `Camera x y z tx ty tz` (CR LF). In every shipped file
//! the first camera's position is the position of view `<from>` in
//! `scenar\CoordHoggar.txt` and the second's that of view `<to>` (within
//! 3 units), and the second point lies at the same distance from the first
//! in all 44 lines `[CONFIRMED-DATA]`: an eye and a look-at point
//! `[INFERRED]`.

use std::fmt;

/// One `Camera` line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VueCamera {
    /// Values 1 to 3: the eye, in the units of `Coord*.txt`.
    pub position: [f32; 3],
    /// Values 4 to 6: a point the camera looks at `[INFERRED]`.
    pub target: [f32; 3],
}

/// A `.vue` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vue {
    /// The cameras in file order: the travel's start, then its end.
    pub cameras: Vec<VueCamera>,
}

/// Why a `.vue` file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VueError {
    /// A line (1-based) that is not `Camera` and six numbers.
    BadLine(usize),
}

impl fmt::Display for VueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VueError::BadLine(n) => write!(f, ".vue line {n} is not `Camera` and six numbers"),
        }
    }
}

impl std::error::Error for VueError {}

impl Vue {
    /// Reads the text; blank lines are skipped, any other line must be
    /// `Camera` followed by six numbers (spaces or tabs between them).
    pub fn parse(data: &[u8]) -> Result<Vue, VueError> {
        let text: String = data.iter().map(|&c| char::from(c)).collect();
        let mut cameras = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let mut words = line.split_whitespace();
            let Some(first) = words.next() else {
                continue;
            };
            let values: Vec<f32> = words
                .map(str::parse)
                .collect::<Result<_, _>>()
                .map_err(|_| VueError::BadLine(i + 1))?;
            if first != "Camera" || values.len() != 6 {
                return Err(VueError::BadLine(i + 1));
            }
            cameras.push(VueCamera {
                position: [values[0], values[1], values[2]],
                target: [values[3], values[4], values[5]],
            });
        }
        Ok(Vue { cameras })
    }

    /// The two views of a travel named `ho<from><to>` (three digits each),
    /// as in `ho004005`.
    pub fn views(stem: &str) -> Option<(u32, u32)> {
        let digits = stem.get(2..)?;
        if digits.len() != 6 || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some((digits[..3].parse().ok()?, digits[3..].parse().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_two_cameras() {
        let v = Vue::parse(b"Camera 1 2 3 4 5 6\r\nCamera -1.5 0 0\t1 1 1\r\n").unwrap();
        assert_eq!(v.cameras.len(), 2);
        assert_eq!(v.cameras[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(v.cameras[1].target, [1.0, 1.0, 1.0]);
        assert_eq!(v.cameras[1].position[0], -1.5);
    }

    #[test]
    fn refuses_other_lines() {
        assert_eq!(Vue::parse(b"Camera 1 2 3\r\n"), Err(VueError::BadLine(1)));
        assert_eq!(
            Vue::parse(b"\r\nLight 1 2 3 4 5 6\r\n"),
            Err(VueError::BadLine(2))
        );
        assert_eq!(
            Vue::parse(b"Camera 1 2 3 4 5 x\r\n"),
            Err(VueError::BadLine(1))
        );
    }

    #[test]
    fn views_from_the_name() {
        assert_eq!(Vue::views("ho004005"), Some((4, 5)));
        assert_eq!(Vue::views("ho04005"), None);
    }
}
