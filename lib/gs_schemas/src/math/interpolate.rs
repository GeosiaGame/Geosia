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
pub fn bicubic<T>(n: [[T; 4]; 4], alpha_x: T, alpha_y: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    let n: [T; 4] = {
        let n0 = cubic(n[0], alpha_x);
        let n1 = cubic(n[1], alpha_x);
        let n2 = cubic(n[2], alpha_x);
        let n3 = cubic(n[3], alpha_x);
        [n0, n1, n2, n3]
    };
    cubic(n, alpha_y)
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
pub fn n_cubic<T, const N: usize>(n: [[T; 4]; 4], alpha: T) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
{
    let n: [T; 4] = {
        let n0 = cubic(n[0], alpha);
        let n1 = cubic(n[1], alpha);
        let n2 = cubic(n[2], alpha);
        let n3 = cubic(n[3], alpha);
        [n0, n1, n2, n3]
    };
    cubic(n, alpha)
}


macro_rules! n_cubic_aa {
    ($name:ident, $N:expr, $([$arr:ty; 4]);+) => {
        pub fn $name<T>(n: $($arr)*, alpha: T) -> T
        where
            T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy + Default,
        {
            for n in 0..$N {

            }
            T::default()
        }
    }
}

n_cubic_aa!(n_cubic_aa, 5, [[[[[T; 4]; 4]; 4]; 4]; 4]);
