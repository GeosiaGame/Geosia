//! Interpolation helper functions.

use std::ops::{Add, Mul, Sub};

/// Performs cubic interpolation between two values bound between two other values.
///
/// - n0 - The value before the first value.
/// - n1 - The first value.
/// - n2 - The second value.
/// - n3 - The value after the second value.
/// - alpha - The alpha value.
///
/// The alpha value should range from 0.0 to 1.0. If the alpha value is 0.0,
/// this function returns _n1_. If the alpha value is 1.0, this function returns _n2_.
#[inline]
pub fn cubic<T>(n: [T; 4], alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    let p = (n[3] - n[2]) - (n[0] - n[1]);
    let q = (n[0] - n[1]) - p;
    let r = n[2] - n[0];
    let s = n[1];
    p * alpha * alpha * alpha + q * alpha * alpha + r * alpha + s
}

/// Performs bicubic interpolation between two values bound between two other values.
///
/// - n0 - The value before the first value.
/// - n1 - The first value.
/// - n2 - The second value.
/// - n3 - The value after the second value.
/// - alpha - The alpha value.
///
/// The alpha value should range from 0.0 to 1.0. If the alpha value is 0.0,
/// this function returns _n1_. If the alpha value is 1.0, this function returns _n2_.
///
/// Source: <https://www.paulinternet.nl/?page=bicubic>
#[inline]
pub fn bicubic<T>(n: [[T; 4]; 4], alpha_1: T, alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    let n0 = cubic(n[0], alpha_1);
    let n1 = cubic(n[1], alpha_1);
    let n2 = cubic(n[2], alpha_1);
    let n3 = cubic(n[3], alpha_1);
    cubic([n0, n1, n2, n3], alpha)
}

#[allow(missing_docs)]
#[inline]
fn cubic_2<T>(n: [[T; 4]; 4], alpha_1: T, alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    bicubic(n, alpha_1, alpha)
}

#[allow(missing_docs)]
#[macro_export]
macro_rules! last_arg {
    ($x:expr) => ($x);
    ($x:expr, $($xs:expr),+) => ($crate::last_arg!($($xs),+));
}

#[allow(missing_docs)]
#[macro_export]
macro_rules! n_cubic {
    ($name:ident $N:literal $arr:tt $(($parent_name:ident, $arg_name:ident)),*) => {
        #[inline]
        fn $name<T>(n: $arr, $($arg_name: T),*, alpha: T) -> T
        where
            T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
        {
            let n0 = $crate::last_arg!($($parent_name),*)(n[0], $($arg_name),*);
            let n1 = $crate::last_arg!($($parent_name),*)(n[1], $($arg_name),*);
            let n2 = $crate::last_arg!($($parent_name),*)(n[2], $($arg_name),*);
            let n3 = $crate::last_arg!($($parent_name),*)(n[3], $($arg_name),*);
            cubic([n0, n1, n2, n3], alpha)
        }
    }
}

// Repeating macro definition for all N-cubic interpolations where N is 3..5.
gs_macros::all_arrays!(
    n_cubic,
    3, 5,
    [T; 4],
    cubic_,
    alpha_
);

#[allow(missing_docs)]
#[inline]
pub fn tricubic<T>(n: [[[T; 4]; 4]; 4], alpha_1: T, alpha_2: T, alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    cubic_3(n, alpha_1, alpha_2, alpha)
}

#[allow(missing_docs)]
#[inline]
pub fn pentcubic<T>(n: [[[[T; 4]; 4]; 4]; 4], alpha_1: T, alpha_2: T, alpha_3: T, alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    cubic_4(n, alpha_1, alpha_2, alpha_3, alpha)
}

#[allow(missing_docs)]
#[inline]
pub fn septcubic<T>(n: [[[[[T; 4]; 4]; 4]; 4]; 4], alpha_1: T, alpha_2: T, alpha_3: T, alpha_4: T, alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    cubic_5(n, alpha_1, alpha_2, alpha_3, alpha_4, alpha)
}

// internal map of (N)-cubic interpolation depth N -> function pointer
static INTERPOLATORS: [usize; 5] = [
    cubic::<f64> as usize,
    bicubic::<f64> as usize,
    tricubic::<f64> as usize,
    pentcubic::<f64> as usize,
    septcubic::<f64> as usize,
];

/// Get an interpolator function with specified depth `N`
pub fn get_interpolator<const N: usize, Input>() -> fn(Input, f64) -> f64 {
    let ptr = INTERPOLATORS[N] as *const ();
    unsafe { std::mem::transmute(ptr) }
}
