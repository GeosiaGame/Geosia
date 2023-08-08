//! Direction and grid-aligned rotation handling utilities
use std::fmt::Debug;

use bevy_math::prelude::*;
use bevy_math::{Mat3A, Vec3A};
use itertools::Itertools;

/// A direction in the right-handed coordinate system of the game
#[repr(i32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Direction {
    /// Left
    XMinus = 0,
    /// Right
    XPlus,
    /// Down/Bottom
    YMinus,
    /// Up/Top
    YPlus,
    /// Front (into the screen)
    ZMinus,
    /// Back (out of the screen)
    ZPlus,
}

/// List of all valid [`Direction`]s.
pub static ALL_DIRECTIONS: [Direction; 6] = {
    use Direction::*;
    [XMinus, XPlus, YMinus, YPlus, ZMinus, ZPlus]
};

impl Direction {
    /// X-
    pub const LEFT: Direction = Direction::XMinus;
    /// X+
    pub const RIGHT: Direction = Direction::XPlus;
    /// Y-
    pub const DOWN: Direction = Direction::YMinus;
    /// Y+
    pub const UP: Direction = Direction::YPlus;
    /// Z-, Into the screen
    pub const BACK: Direction = Direction::ZMinus;
    /// Z+, Out of the screen
    pub const FRONT: Direction = Direction::ZPlus;

    /// Calculates the direction with the sign flipped (X+ -> X- etc.)
    pub fn opposite(self) -> Self {
        use Direction::*;
        match self {
            XMinus => XPlus,
            XPlus => XMinus,
            YMinus => YPlus,
            YPlus => YMinus,
            ZMinus => ZPlus,
            ZPlus => ZMinus,
        }
    }

    /// Picks the approximate grid-aligned direction from a non-grid-aligned vector.
    /// Biases towards `UP` for zero vectors.
    pub fn from_approx_vecf(v: Vec3A) -> Self {
        if v.length_squared() == 0.0 {
            Self::UP
        } else {
            let vc: [f32; 3] = v.into();
            let maxaxis = vc
                .iter()
                .map(|x| x.abs())
                .position_max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(1);
            Self::try_from_index(maxaxis * 2 + if vc[maxaxis] < 0.0 { 0 } else { 1 }).unwrap()
        }
    }

    /// Tries to convert an integer vector into a direction it it's precisely an axis-aligned unit vector.
    pub fn try_from_vec(v: IVec3) -> Option<Self> {
        let va: [i32; 3] = v.into();
        match va {
            [1, 0, 0] => Some(Direction::XPlus),
            [-1, 0, 0] => Some(Direction::XMinus),
            [0, 1, 0] => Some(Direction::YPlus),
            [0, -1, 0] => Some(Direction::YMinus),
            [0, 0, 1] => Some(Direction::ZPlus),
            [0, 0, -1] => Some(Direction::ZMinus),
            _ => None,
        }
    }

    /// Converts the direction into an axis-aligned integer unit vector.
    pub fn to_veci(self) -> IVec3 {
        use Direction::*;
        match self {
            XMinus => IVec3::new(-1, 0, 0),
            XPlus => IVec3::new(1, 0, 0),
            YMinus => IVec3::new(0, -1, 0),
            YPlus => IVec3::new(0, 1, 0),
            ZMinus => IVec3::new(0, 0, -1),
            ZPlus => IVec3::new(0, 0, 1),
        }
    }

    /// Converts the direction into an axis-aligned floating point unit vector.
    pub fn to_vecf(self) -> Vec3A {
        use Direction::*;
        match self {
            XMinus => Vec3A::new(-1.0, 0.0, 0.0),
            XPlus => Vec3A::new(1.0, 0.0, 0.0),
            YMinus => Vec3A::new(0.0, -1.0, 0.0),
            YPlus => Vec3A::new(0.0, 1.0, 0.0),
            ZMinus => Vec3A::new(0.0, 0.0, -1.0),
            ZPlus => Vec3A::new(0.0, 0.0, 1.0),
        }
    }

    /// Converts a direction index (from [`Self::to_index`]) into a Direction, or None if not valid.
    pub fn try_from_index(idx: usize) -> Option<Self> {
        use Direction::*;
        match idx {
            0 => Some(XMinus),
            1 => Some(XPlus),
            2 => Some(YMinus),
            3 => Some(YPlus),
            4 => Some(ZMinus),
            5 => Some(ZPlus),
            _ => None,
        }
    }

    /// Provides the index of the axis of the direction: 0 for X, 1 for Y and 2 for Z.
    pub fn to_axis_index(self) -> usize {
        use Direction::*;
        match self {
            XMinus => 0,
            XPlus => 0,
            YMinus => 1,
            YPlus => 1,
            ZMinus => 2,
            ZPlus => 2,
        }
    }

    /// Converts the direction into an index:
    /// 0 for X-, 1 for X+, 2 for Y-, 3 for Y+, 4 for Z-, 5 for Z+.
    pub fn to_index(self) -> usize {
        use Direction::*;
        match self {
            XMinus => 0,
            XPlus => 1,
            YMinus => 2,
            YPlus => 3,
            ZMinus => 4,
            ZPlus => 5,
        }
    }

    /// Checks if the direction is facing the positive direction of its axis.
    pub fn is_positive(self) -> bool {
        use Direction::*;
        match self {
            XMinus | YMinus | ZMinus => false,
            XPlus | YPlus | ZPlus => true,
        }
    }

    /// Checks if the direction is facing the negative direction of its axis.
    pub fn is_negative(self) -> bool {
        use Direction::*;
        match self {
            XMinus | YMinus | ZMinus => true,
            XPlus | YPlus | ZPlus => false,
        }
    }

    /// Vector cross product of two directions (right-handed)
    pub fn cross(a: Self, b: Self) -> Option<Self> {
        let aidx = a.to_index();
        let bidx = b.to_index();
        DIRECTION_CROSS_TABLE[aidx * 6 + bidx]
    }
}

/// One of the 24 possible orientations for an octahedron or cube, assuming RIGHT x UP == FRONT
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct OctahedralOrientation {
    right: Direction,
    up: Direction,
}

impl Debug for OctahedralOrientation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OctoOrientation {{right: {:?}, up: {:?}, front: {:?}}}",
            self.right(),
            self.up(),
            self.front()
        )
    }
}

impl Default for OctahedralOrientation {
    fn default() -> Self {
        Self {
            right: Direction::RIGHT,
            up: Direction::UP,
        }
    }
}

impl OctahedralOrientation {
    /// A default orientation of local right&up aligning with global right&up.
    pub fn new() -> Self {
        Default::default()
    }

    /// Tries to construct an orientation from the given local directions.
    pub fn try_from_dirs(right: Direction, up: Direction, front: Direction) -> Option<Self> {
        if Direction::cross(right, up) != Some(front) {
            None
        } else {
            Some(Self { right, up })
        }
    }

    /// Constructs an orientation from a local right&up direction, calculating the front direction
    pub fn from_right_up(right: Direction, up: Direction) -> Option<Self> {
        if Direction::cross(right, up).is_none() {
            None
        } else {
            Some(Self { right, up })
        }
    }

    /// Constructs an orientation from a local up&front direction, calculating the right direction
    pub fn from_up_front(up: Direction, front: Direction) -> Option<Self> {
        Direction::cross(up, front).map(|right| Self { right, up })
    }

    /// Constructs an orientation from a local front&right direction, calculating the up direction
    pub fn from_front_right(front: Direction, right: Direction) -> Option<Self> {
        Direction::cross(front, right).map(|up| Self { right, up })
    }

    /// Converts itself into an index in the range 0..24 (not inclusive)
    pub fn to_index(self) -> usize {
        // 0..6
        let right_idx = self.right.to_index();
        // 0..4
        let up_idx = {
            let i = self.up.to_index();
            if i > right_idx {
                i - 2
            } else {
                i
            }
        };
        // front is always determined by the cross product
        // make it so that 0 is the default orientation (X+ right, Y+ up)
        (right_idx * 4 + up_idx + 24 - 5) % 24
    }

    /// Converts an index (0..24, as returned from to_index) to an orientation
    pub fn from_index(i: usize) -> Option<Self> {
        if i >= 24 {
            None
        } else {
            // adjust for default orientation offset
            let i = (i + 5) % 24;
            let right_idx = i / 4;
            let right = Direction::try_from_index(right_idx).unwrap();
            let up_idx = i % 4;
            let up_idx = if (up_idx / 2) >= (right_idx / 2) {
                up_idx + 2
            } else {
                up_idx
            };
            let up = Direction::try_from_index(up_idx).unwrap();
            Some(Self { right, up })
        }
    }

    /// M * v will rotate the vector v to match this orientation
    pub fn to_matrixf(self) -> Mat3A {
        Mat3A::from_cols(
            self.right().to_veci().as_vec3a(),
            self.up().to_veci().as_vec3a(),
            self.front().to_veci().as_vec3a(),
        )
    }

    /// Rotates a global direction to the local space of this orientation.
    pub fn apply_to_dir(self, dir: Direction) -> Direction {
        APPLY_UNAPPLY_LUT[dir.to_index() * 24 + self.to_index()].0
    }

    /// Rotates a global direction vector to the local space of this orientation.
    pub fn apply_to_veci(self, vec: IVec3) -> IVec3 {
        (self.to_matrixf() * vec.as_vec3a()).as_ivec3()
    }

    /// Rotates a global direction vector to the local space of this orientation.
    pub fn apply_to_vecf(self, vec: Vec3A) -> Vec3A {
        self.to_matrixf() * vec
    }

    /// Rotates a direction in the local space of this orientation to the global space.
    pub fn unapply_to_dir(self, dir: Direction) -> Direction {
        APPLY_UNAPPLY_LUT[dir.to_index() * 24 + self.to_index()].1
    }

    /// Rotates a direction vector in the local space of this orientation to the global space.
    pub fn unapply_to_veci(self, vec: IVec3) -> IVec3 {
        (self.to_matrixf().transpose() * vec.as_vec3a()).as_ivec3()
    }

    /// Rotates a direction vector in the local space of this orientation to the global space.
    pub fn unapply_to_vecf(self, vec: Vec3A) -> Vec3A {
        self.to_matrixf().transpose() * vec
    }

    /// The local "right" direction converted to the global space
    pub fn right(self) -> Direction {
        self.right
    }

    /// The local "up" direction converted to the global space
    pub fn up(self) -> Direction {
        self.up
    }

    /// The local "front" direction converted to the global space
    pub fn front(self) -> Direction {
        Direction::cross(self.right, self.up).unwrap()
    }

    /// The local "left" direction converted to the global space
    pub fn left(self) -> Direction {
        self.right.opposite()
    }

    /// The local "down" direction converted to the global space
    pub fn down(self) -> Direction {
        self.up.opposite()
    }

    /// The local "back" direction converted to the global space
    pub fn back(self) -> Direction {
        Direction::cross(self.up, self.right).unwrap()
    }
}

#[cfg(test)]
mod test {
    use hashbrown::HashSet;

    use super::*;

    #[test]
    fn test_apply_unapply_lut() {
        let mut lut = [(Direction::FRONT, Direction::FRONT); 6 * 24];
        for dir_idx in 0..6 {
            for orientation_idx in 0..24 {
                let dir = Direction::try_from_index(dir_idx).unwrap();
                let orientation = OctahedralOrientation::from_index(orientation_idx).unwrap();
                let applied = Direction::from_approx_vecf(orientation.to_matrixf() * dir.to_vecf());
                let unapplied = Direction::from_approx_vecf(orientation.to_matrixf().transpose() * dir.to_vecf());
                lut[dir_idx * 24 + orientation_idx] = (applied, unapplied);
            }
        }
        assert_eq!(lut, APPLY_UNAPPLY_LUT);
    }

    #[test]
    fn orientation_permutation_count() {
        let mut allowed = 0;
        for &d1 in &ALL_DIRECTIONS {
            for &d2 in &ALL_DIRECTIONS {
                for &d3 in &ALL_DIRECTIONS {
                    if let Some(_orientation) = OctahedralOrientation::try_from_dirs(d1, d2, d3) {
                        allowed += 1;
                    }
                }
            }
        }
        assert_eq!(allowed, 24);
    }

    #[test]
    fn orientation_construction() {
        let mut indices_used = HashSet::new();
        for &d1 in &ALL_DIRECTIONS {
            for &d2 in &ALL_DIRECTIONS {
                for &d3 in &ALL_DIRECTIONS {
                    if let Some(orn) = OctahedralOrientation::try_from_dirs(d1, d2, d3) {
                        assert_eq!(orn.right(), d1);
                        assert_eq!(orn.up(), d2);
                        assert_eq!(orn.front(), d3);
                        assert_eq!(orn.left(), d1.opposite());
                        assert_eq!(orn.down(), d2.opposite());
                        assert_eq!(orn.back(), d3.opposite());
                        let idx = orn.to_index();
                        assert_eq!(Some(orn), OctahedralOrientation::from_index(idx));
                        assert!(indices_used.insert(idx));
                        assert_eq!(
                            Some(orn),
                            OctahedralOrientation::try_from_dirs(orn.right(), orn.up(), orn.front())
                        );
                        assert_eq!(Some(orn), OctahedralOrientation::from_right_up(orn.right(), orn.up()));
                        assert_eq!(Some(orn), OctahedralOrientation::from_up_front(orn.up(), orn.front()));
                        assert_eq!(
                            Some(orn),
                            OctahedralOrientation::from_front_right(orn.front(), orn.right())
                        );
                        assert_eq!(orn.apply_to_dir(Direction::FRONT), orn.front());
                        assert_eq!(orn.apply_to_dir(Direction::RIGHT), orn.right());
                        assert_eq!(orn.apply_to_dir(Direction::UP), orn.up());
                        assert_eq!(orn.apply_to_dir(Direction::BACK), orn.back());
                        assert_eq!(orn.apply_to_dir(Direction::LEFT), orn.left());
                        assert_eq!(orn.apply_to_dir(Direction::DOWN), orn.down());
                    }
                }
            }
        }
        assert_eq!(indices_used.len(), 24);
        assert!(indices_used.iter().all(|&n| n < 24));
        let default_orientation = OctahedralOrientation::default();
        assert_eq!(default_orientation.right(), Direction::RIGHT);
        assert_eq!(default_orientation.up(), Direction::UP);
        assert_eq!(default_orientation.front(), Direction::FRONT);
        assert_eq!(default_orientation.left(), Direction::LEFT);
        assert_eq!(default_orientation.down(), Direction::DOWN);
        assert_eq!(default_orientation.back(), Direction::BACK);
        let id3 = Mat3A::IDENTITY;
        assert_eq!(default_orientation.to_matrixf(), id3);
    }

    #[test]
    fn direction_cross_verify() {
        let mut non_zero = 0;
        for &d1 in &ALL_DIRECTIONS {
            for &d2 in &ALL_DIRECTIONS {
                let v1 = d1.to_veci();
                let v2 = d2.to_veci();
                let vcross = v1.cross(v2);
                let vdcross = Direction::try_from_vec(vcross);
                let dcross = Direction::cross(d1, d2);
                assert_eq!(dcross, vdcross);
                if dcross.is_some() {
                    non_zero += 1;
                }
            }
        }
        assert_eq!(non_zero, 24);
        assert_eq!(
            Direction::cross(Direction::RIGHT, Direction::UP),
            Some(Direction::FRONT)
        );
        assert_eq!(
            Direction::cross(Direction::UP, Direction::FRONT),
            Some(Direction::RIGHT)
        );
        assert_eq!(
            Direction::cross(Direction::FRONT, Direction::RIGHT),
            Some(Direction::UP)
        );
    }
}

/// Cross product table for a pair of directions, indexed by 6*a+b (a,b being signed axis indices)
static DIRECTION_CROSS_TABLE: [Option<Direction>; 36] = {
    use Direction::*;
    [
        None,         // XMinus*XMinus
        None,         // XMinus*XPlus
        Some(ZPlus),  // XMinus*YMinus
        Some(ZMinus), // XMinus*YPlus
        Some(YMinus), // XMinus*ZMinus
        Some(YPlus),  // XMinus*ZPlus
        None,         // XPlus*XMinus
        None,         // XPlus*XPlus
        Some(ZMinus), // XPlus*YMinus
        Some(ZPlus),  // XPlus*YPlus
        Some(YPlus),  // XPlus*ZMinus
        Some(YMinus), // XPlus*ZPlus
        Some(ZMinus), // YMinus*XMinus
        Some(ZPlus),  // YMinus*XPlus
        None,         // YMinus*YMinus
        None,         // YMinus*YPlus
        Some(XPlus),  // YMinus*ZMinus
        Some(XMinus), // YMinus*ZPlus
        Some(ZPlus),  // YPlus*XMinus
        Some(ZMinus), // YPlus*XPlus
        None,         // YPlus*YMinus
        None,         // YPlus*YPlus
        Some(XMinus), // YPlus*ZMinus
        Some(XPlus),  // YPlus*ZPlus
        Some(YPlus),  // ZMinus*XMinus
        Some(YMinus), // ZMinus*XPlus
        Some(XMinus), // ZMinus*YMinus
        Some(XPlus),  // ZMinus*YPlus
        None,         // ZMinus*ZMinus
        None,         // ZMinus*ZPlus
        Some(YMinus), // ZPlus*XMinus
        Some(YPlus),  // ZPlus*XPlus
        Some(XPlus),  // ZPlus*YMinus
        Some(XMinus), // ZPlus*YPlus
        None,         // ZPlus*ZMinus
        None,         // ZPlus*ZPlus
    ]
};

/// Lookup table for [`OctahedralOrientation::apply_to_dir`] and [`OctahedralOrientation::unapply_to_dir`]
/// Computed by running `test::test_apply_unapply_lut`.
static APPLY_UNAPPLY_LUT: [(Direction, Direction); 6 * 24] = {
    use Direction::*;
    [
        (XMinus, XMinus),
        (XMinus, XMinus),
        (XMinus, XMinus),
        (YPlus, YPlus),
        (YPlus, YMinus),
        (YPlus, ZMinus),
        (YPlus, ZPlus),
        (YMinus, YPlus),
        (YMinus, YMinus),
        (YMinus, ZPlus),
        (YMinus, ZMinus),
        (ZPlus, YPlus),
        (ZPlus, YMinus),
        (ZPlus, ZPlus),
        (ZPlus, ZMinus),
        (ZMinus, YPlus),
        (ZMinus, YMinus),
        (ZMinus, ZMinus),
        (ZMinus, ZPlus),
        (XPlus, XPlus),
        (XPlus, XPlus),
        (XPlus, XPlus),
        (XPlus, XPlus),
        (XMinus, XMinus),
        (XPlus, XPlus),
        (XPlus, XPlus),
        (XPlus, XPlus),
        (YMinus, YMinus),
        (YMinus, YPlus),
        (YMinus, ZPlus),
        (YMinus, ZMinus),
        (YPlus, YMinus),
        (YPlus, YPlus),
        (YPlus, ZMinus),
        (YPlus, ZPlus),
        (ZMinus, YMinus),
        (ZMinus, YPlus),
        (ZMinus, ZMinus),
        (ZMinus, ZPlus),
        (ZPlus, YMinus),
        (ZPlus, YPlus),
        (ZPlus, ZPlus),
        (ZPlus, ZMinus),
        (XMinus, XMinus),
        (XMinus, XMinus),
        (XMinus, XMinus),
        (XMinus, XMinus),
        (XPlus, XPlus),
        (YMinus, YMinus),
        (ZPlus, ZMinus),
        (ZMinus, ZPlus),
        (XPlus, XPlus),
        (XMinus, XPlus),
        (ZPlus, XPlus),
        (ZMinus, XPlus),
        (XPlus, XMinus),
        (XMinus, XMinus),
        (ZPlus, XMinus),
        (ZMinus, XMinus),
        (XPlus, ZMinus),
        (XMinus, ZPlus),
        (YPlus, YPlus),
        (YMinus, YMinus),
        (XPlus, ZPlus),
        (XMinus, ZMinus),
        (YPlus, YPlus),
        (YMinus, YMinus),
        (YPlus, YPlus),
        (YMinus, YMinus),
        (ZPlus, ZPlus),
        (ZMinus, ZMinus),
        (YPlus, YPlus),
        (YPlus, YPlus),
        (ZMinus, ZPlus),
        (ZPlus, ZMinus),
        (XMinus, XMinus),
        (XPlus, XMinus),
        (ZMinus, XMinus),
        (ZPlus, XMinus),
        (XMinus, XPlus),
        (XPlus, XPlus),
        (ZMinus, XPlus),
        (ZPlus, XPlus),
        (XMinus, ZPlus),
        (XPlus, ZMinus),
        (YMinus, YMinus),
        (YPlus, YPlus),
        (XMinus, ZMinus),
        (XPlus, ZPlus),
        (YMinus, YMinus),
        (YPlus, YPlus),
        (YMinus, YMinus),
        (YPlus, YPlus),
        (ZMinus, ZMinus),
        (ZPlus, ZPlus),
        (YMinus, YMinus),
        (ZMinus, ZMinus),
        (YMinus, YPlus),
        (YPlus, YMinus),
        (ZPlus, ZPlus),
        (ZMinus, ZMinus),
        (XMinus, YPlus),
        (XPlus, YMinus),
        (ZMinus, ZMinus),
        (ZPlus, ZPlus),
        (XPlus, YPlus),
        (XMinus, YMinus),
        (YMinus, XPlus),
        (YPlus, XPlus),
        (XPlus, XPlus),
        (XMinus, XPlus),
        (YPlus, XMinus),
        (YMinus, XMinus),
        (XMinus, XMinus),
        (XPlus, XMinus),
        (ZMinus, ZMinus),
        (ZPlus, ZPlus),
        (YPlus, YPlus),
        (YMinus, YMinus),
        (ZPlus, ZPlus),
        (ZPlus, ZPlus),
        (YPlus, YMinus),
        (YMinus, YPlus),
        (ZMinus, ZMinus),
        (ZPlus, ZPlus),
        (XPlus, YMinus),
        (XMinus, YPlus),
        (ZPlus, ZPlus),
        (ZMinus, ZMinus),
        (XMinus, YMinus),
        (XPlus, YPlus),
        (YPlus, XMinus),
        (YMinus, XMinus),
        (XMinus, XMinus),
        (XPlus, XMinus),
        (YMinus, XPlus),
        (YPlus, XPlus),
        (XPlus, XPlus),
        (XMinus, XPlus),
        (ZPlus, ZPlus),
        (ZMinus, ZMinus),
        (YMinus, YMinus),
        (YPlus, YPlus),
        (ZMinus, ZMinus),
    ]
};
