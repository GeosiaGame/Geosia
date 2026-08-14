use core::marker::PhantomData;
use std::cmp::Ordering;
use std::ops::{Add, Mul, Sub};
use bevy_math::FloatExt;
use gs_schemas::dependencies::itertools::Itertools;
use noise::NoiseFn;

use gs_schemas::math::interpolate;

/// Noise function that maps the output value from the source function onto an
/// arbitrary function curve.
///
/// This noise function maps the output value from the source function onto an
/// application-defined curve. The curve is defined by a number of _control
/// points_; each control point has an _input value_ that maps to an _output
/// value_.
///
/// To add control points to the curve, use the `add_control_point` method.
///
/// Since the curve is a cubic spline, an application must have a minimum of
/// four control points to the curve. If there is less than four control
/// points, the `get()` method panics. Each control point can have any input
/// and output value, although no two control points can have the same input.
#[derive(Clone)]
pub struct Curve<T, Source, const DIM: usize>
where
    Source: NoiseFn<T, DIM>,
{
    /// Outputs a value.
    pub source: Source,

    /// Vec that stores the control points.
    control_points: Vec<ControlPoint<T, DIM>>,

    phantom: PhantomData<T>,
}

#[derive(Clone)]
struct ControlPoint<T, const DIM: usize> {
    input: f64,
    point: [T; DIM],
}

impl<T, Source, const DIM: usize> Curve<T, Source, DIM>
where
    Source: NoiseFn<T, DIM>,
{
    pub fn new(source: Source) -> Self {
        Self {
            source,
            control_points: Vec::with_capacity(4),
            phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn add_control_point(mut self, input: f64, point: [T; DIM]) -> Self {
        // check to see if the vector already contains the input point.
        if !self
            .control_points
            .iter()
            .any(|x| (x.input - input).abs() < f64::EPSILON)
        {
            // it doesn't, so find the correct position to insert the new
            // control point.
            let insertion_point = self
                .control_points
                .iter()
                .position(|x| x.input >= input)
                .unwrap_or(self.control_points.len());

            // add the new control point at the correct position.
            self.control_points.insert(
                insertion_point,
                ControlPoint { input, point },
            );
        }

        self
    }
}

impl<T, Source, const DIM: usize> NoiseFn<T, DIM> for Curve<T, Source, DIM>
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + PartialOrd + Copy + Default,
    Source: NoiseFn<T, DIM>,
{
    fn get(&self, point: [T; DIM]) -> f64 {
        // confirm that there's at least 4 control points in the vector.
        assert!(self.control_points.len() >= 4);

        // get output value from the source function
        let source_value = self.source.get(point);

        // Find the first element in the control point array that has a input
        // value larger than the output value from the source function
        let index_pos = self
            .control_points
            .iter()
            .position_min_by(|&x, &y| by_distance_to(x.point, y.point, point))
            .unwrap_or(self.control_points.len());

        // if index_pos < 2 {
        //     println!(
        //         "index_pos in curve was less than 2! source value was {}",
        //         source_value
        //     );
        // }

        // ensure that the index is at least 2 and less than control_points.len()
        let index_pos = index_pos.clamp(0, self.control_points.len() - 1);

        // Find the four nearest control points so that we can perform cubic interpolation.
        let index0 = (index_pos - 2).max(0);
        let index1 = (index_pos - 1).max(0);
        let index2 = index_pos;
        let index3 = (index_pos + 1).min(self.control_points.len() - 1);

        // If some control points are missing, return the original output value.
        // This can occur if the value from the source function is greater than
        // the largest input value or less than the smallest input value of the control point array
        if index1 == index2 {
            return self.source.get(self.control_points[index1].point);
        }

        // Compute the alpha value used for cubic interpolation
        let input0 = self.control_points[index1].input;
        let input1 = self.control_points[index2].input;

        let alpha = f64::inverse_lerp(input0, input1, source_value);

        // Now perform the cubic interpolation and return.
        interpolate::cubic([
                self.source.get(self.control_points[index0].point),
                self.source.get(self.control_points[index1].point),
                self.source.get(self.control_points[index2].point),
                self.source.get(self.control_points[index3].point),
            ],
            alpha,
        )
    }
}

fn by_distance_to<T, const DIM: usize>(p1: [T; DIM], p2: [T; DIM], source: [T; DIM]) -> Ordering
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + PartialOrd + Copy + Default,
{
    let dist_a = square_distance(source, p1);
    let dist_b = square_distance(source, p2);
    if dist_a < dist_b {
        Ordering::Less
    } else if dist_a > dist_b {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

fn square_distance<T, const DIM: usize>(p1: [T; DIM], p2: [T; DIM]) -> T
where
    T: Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy + Default,
{
    let mut result = T::default();
    for i in 0..DIM {
        let difference = p1[i] - p2[i];
        result = result + (difference * difference);
    }
    result
}

