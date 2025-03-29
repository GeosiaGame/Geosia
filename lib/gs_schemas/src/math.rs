//! Math helper functions.

use bevy_math::{Vec3A, prelude::*};

/// An epsilon value so small that it should be indistinguishable from zero for most operations performed on close-to-natural numbers.
/// Based on the floating-point precision around a value of `1`.
pub const VERY_CLOSE_TO_ZERO_F32: f32 = 1e-7f32;
/// An epsilon value so small that it should be indistinguishable from zero for most operations performed on close-to-natural numbers.
/// Based on the floating-point precision around a value of `1`.
pub const VERY_CLOSE_TO_ZERO_F64: f64 = 1e-16f64;

/// A true signum implementation that returns zero for very-close-to-zero floating point numbers.
pub trait ZeroRespectingSignum {
    /// Returns -1 for negative components, 0 for zero or very-close-to-zero components and 1 for positive components.
    fn zero_respecting_signum(self) -> Self;
}

/// [`ZeroRespectingSignum`] variant that returns the sign in an int (vector) for convenience.
pub trait ZeroRespectingSignumToInt {
    /// The corresponding integer type of Self.
    type IntVariant;
    /// Returns -1 for negative components, 0 for zero or very-close-to-zero components and 1 for positive components.
    fn zero_respecting_signum_int(self) -> Self::IntVariant;
}

impl ZeroRespectingSignum for f32 {
    #[inline]
    fn zero_respecting_signum(self) -> Self {
        if self.abs() < VERY_CLOSE_TO_ZERO_F32 {
            0.0
        } else {
            self.signum()
        }
    }
}

impl ZeroRespectingSignumToInt for f32 {
    type IntVariant = i32;

    #[inline]
    fn zero_respecting_signum_int(self) -> Self::IntVariant {
        Self::zero_respecting_signum(self) as Self::IntVariant
    }
}

impl ZeroRespectingSignum for f64 {
    #[inline]
    fn zero_respecting_signum(self) -> Self {
        if self.abs() < VERY_CLOSE_TO_ZERO_F64 {
            0.0
        } else {
            self.signum()
        }
    }
}

impl ZeroRespectingSignumToInt for f64 {
    type IntVariant = i32;

    #[inline]
    fn zero_respecting_signum_int(self) -> Self::IntVariant {
        Self::zero_respecting_signum(self) as Self::IntVariant
    }
}

impl ZeroRespectingSignum for Vec2 {
    #[inline]
    fn zero_respecting_signum(self) -> Self {
        let minuses = self.cmple(Vec2::splat(-VERY_CLOSE_TO_ZERO_F32));
        let pluses = self.cmpge(Vec2::splat(VERY_CLOSE_TO_ZERO_F32));
        Vec2::select(minuses, Vec2::NEG_ONE, Vec2::select(pluses, Vec2::ONE, Vec2::ZERO))
    }
}

impl ZeroRespectingSignumToInt for Vec2 {
    type IntVariant = IVec2;

    #[inline]
    fn zero_respecting_signum_int(self) -> Self::IntVariant {
        Self::zero_respecting_signum(self).as_ivec2()
    }
}

impl ZeroRespectingSignum for Vec3 {
    #[inline]
    fn zero_respecting_signum(self) -> Self {
        let minuses = self.cmple(Vec3::splat(-VERY_CLOSE_TO_ZERO_F32));
        let pluses = self.cmpge(Vec3::splat(VERY_CLOSE_TO_ZERO_F32));
        Vec3::select(minuses, Vec3::NEG_ONE, Vec3::select(pluses, Vec3::ONE, Vec3::ZERO))
    }
}

impl ZeroRespectingSignumToInt for Vec3 {
    type IntVariant = IVec3;

    #[inline]
    fn zero_respecting_signum_int(self) -> Self::IntVariant {
        Self::zero_respecting_signum(self).as_ivec3()
    }
}

impl ZeroRespectingSignum for Vec3A {
    #[inline]
    fn zero_respecting_signum(self) -> Self {
        let minuses = self.cmple(Vec3A::splat(-VERY_CLOSE_TO_ZERO_F32));
        let pluses = self.cmpge(Vec3A::splat(VERY_CLOSE_TO_ZERO_F32));
        Vec3A::select(minuses, Vec3A::NEG_ONE, Vec3A::select(pluses, Vec3A::ONE, Vec3A::ZERO))
    }
}

impl ZeroRespectingSignumToInt for Vec3A {
    type IntVariant = IVec3;

    #[inline]
    fn zero_respecting_signum_int(self) -> Self::IntVariant {
        Self::zero_respecting_signum(self).as_ivec3()
    }
}

impl ZeroRespectingSignum for Vec4 {
    #[inline]
    fn zero_respecting_signum(self) -> Self {
        let minuses = self.cmple(Vec4::splat(-VERY_CLOSE_TO_ZERO_F32));
        let pluses = self.cmpge(Vec4::splat(VERY_CLOSE_TO_ZERO_F32));
        Vec4::select(minuses, Vec4::NEG_ONE, Vec4::select(pluses, Vec4::ONE, Vec4::ZERO))
    }
}

impl ZeroRespectingSignumToInt for Vec4 {
    type IntVariant = IVec4;

    #[inline]
    fn zero_respecting_signum_int(self) -> Self::IntVariant {
        Self::zero_respecting_signum(self).as_ivec4()
    }
}
