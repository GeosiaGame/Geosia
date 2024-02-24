//! Miscallaneous utility methods & types.

use crate::range::Range;

impl Range<f64> {
    /// minimum point.
    pub fn min(&self) -> f64 {
        match self {
            Range::Closed(s, _e) => *s,
            Range::Left(s) => *s,
            Range::Right(_e) => f64::MIN,
            Range::RightInclusive(_e) => f64::MIN,
            Range::Full => f64::MIN,
            Range::ClosedInclusive(s, _e) => *s,
        }
    }

    /// maximum point.
    pub fn max(&self) -> f64 {
        match self {
            Range::Closed(_s, e) => *e - f64::EPSILON,
            Range::Left(_s) => f64::MAX,
            Range::Right(e) => *e - f64::EPSILON,
            Range::RightInclusive(e) => *e,
            Range::Full => f64::MAX,
            Range::ClosedInclusive(_s, e) => *e,
        }
    }
}


impl Range<i32> {
    /// minimum point.
    pub fn min(&self) -> i32 {
        match self {
            Range::Closed(s, _e) => *s,
            Range::Left(s) => *s,
            Range::Right(_e) => i32::MIN,
            Range::RightInclusive(_e) => i32::MIN,
            Range::Full => i32::MIN,
            Range::ClosedInclusive(s, _e) => *s,
        }
    }

    /// maximum point.
    pub fn max(&self) -> i32 {
        match self {
            Range::Closed(_s, e) => *e - 1,
            Range::Left(_s) => i32::MAX,
            Range::Right(e) => *e - 1,
            Range::RightInclusive(e) => *e,
            Range::Full => i32::MAX,
            Range::ClosedInclusive(_s, e) => *e,
        }
    }
}