
// My own type of ranges, now that I cannot use the built-in type...
/// Wrapper of Range that we can work with within Rust's type system
#[derive(Clone, Copy)]
pub enum Range<Idx> where Idx: PartialOrd {
    /// start..end
    Closed(Idx, Idx),
    /// start..end (inclusive)
    ClosedInclusive(Idx, Idx),
    /// start..
    Left(Idx),
    /// ..end
    Right(Idx),
    /// ..
    Full,
}

impl<Idx> Range<Idx> where Idx: PartialOrd {
    /// Does this range contain the given value?
    pub fn contains(&self, x: Idx) -> bool {
        match self {
            Range::Closed(s, e) => *s <= x && *e > x,
            Range::Left(s) => x >= *s,
            Range::Right(e) => x < *e,
            Range::Full => true,
            Range::ClosedInclusive(s, e) => *s <= x && *e >= x,
        }
    }
}

impl<Idx> std::fmt::Display for Range<Idx>
where
    Idx: std::fmt::Display + PartialOrd,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Range::Closed(start, end) => write!(f, "{}..{}", start, end),
            Range::Left(start) => write!(f, "{}..", start),
            Range::Right(end) => write!(f, "..{}", end),
            Range::Full => write!(f, ".."),
            Range::ClosedInclusive(start, end) => write!(f, "{}..={}", start, end),
        }
    }
}

/// A wrapper for `std::ops::range`s
pub trait GenRange<R, Idx> where Idx: PartialOrd {
    /// Converts the std range to this crate's range.
    fn range(r: R) -> Range<Idx> where Self: Sized;
}
impl<Idx> GenRange<std::ops::Range<Idx>, Idx> for std::ops::Range<Idx> where Idx: PartialOrd {
    fn range(r: std::ops::Range<Idx>) -> Range<Idx> {
        Range::Closed(r.start, r.end)
    }
}
impl<Idx> GenRange<std::ops::RangeInclusive<Idx>, Idx> for std::ops::RangeInclusive<Idx> where Idx: PartialOrd + Clone {
    fn range(r: std::ops::RangeInclusive<Idx>) -> Range<Idx> {
        Range::ClosedInclusive(r.start().clone(), r.end().clone())
    }
}
impl<Idx> GenRange<std::ops::RangeFrom<Idx>, Idx> for std::ops::RangeFrom<Idx> where Idx: PartialOrd {
    fn range(r: std::ops::RangeFrom<Idx>) -> Range<Idx> {
        Range::Left(r.start)
    }
}
impl<Idx> GenRange<std::ops::RangeTo<Idx>, Idx> for std::ops::RangeTo<Idx> where Idx: PartialOrd {
    fn range(r: std::ops::RangeTo<Idx>) -> Range<Idx> {
        Range::Right(r.end)
    }
}
impl<Idx> GenRange<std::ops::RangeFull, Idx> for std::ops::RangeFull where Idx: PartialOrd {
    fn range(_r: std::ops::RangeFull) -> Range<Idx> {
        Range::Full
    }
}


/// Now I can build ranges with this function:
pub fn range<Idx, R: GenRange<R, Idx>>(r: R) -> Range<Idx> where Idx: PartialOrd {
    R::range(r)
}