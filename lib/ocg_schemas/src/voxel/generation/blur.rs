//! blur algorithm.

use std::ops::Add;

use itertools::Itertools;
use smallvec::{smallvec, SmallVec};

use crate::voxel::biome::{biome_map::EXPECTED_BIOME_COUNT, BiomeEntry};

struct Convolution3<'a, T: 'a> {
    slice: &'a [T],
    state: Convolution3State<'a, T>,
}

enum Convolution3State<'a, T: 'a> {
    BeforeFirst,
    Middle(::std::slice::Windows<'a, T>),
    Done,
}

impl<'a, T: Clone> Convolution3<'a, T> {
    fn new(slice: &'a [T]) -> Self {
        Convolution3 {
            slice,
            state: Convolution3State::BeforeFirst,
        }
    }
}


impl<'a, T: Clone> Iterator for Convolution3<'a, T> {
    type Item = [T; 3];

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            Convolution3State::BeforeFirst => {
                self.state = Convolution3State::Middle(self.slice.windows(3));
                return Some([self.slice[0].clone(), self.slice[0].clone(), self.slice[1].clone()])
            }
            Convolution3State::Middle(ref mut windows) => {
                if let Some(window) = windows.next() {
                    return Some([window[0].clone(), window[1].clone(), window[2].clone()])
                }
            }
            Convolution3State::Done => return None
        }
        // windows.next() returned None
        self.state = Convolution3State::Done;

        let last = self.slice.len() - 1;
        Some([self.slice[last - 1].clone(), self.slice[last - 1].clone(), self.slice[last].clone()])
    }
}

/// do a convolution blur of the data.
pub fn blur<T: Add<Output = T> + Copy>(input: &[&[T]], output: &mut [&mut [T]]) {
    for (in_rows, out_row) in Convolution3::new(input).zip(output) {
        for (((r0, r1), r2), out) in
            Convolution3::new(in_rows[0])
            .zip(Convolution3::new(in_rows[1]))
            .zip(Convolution3::new(in_rows[2]))
            .zip(&mut **out_row)
        {
            *out = r0[0] + r0[1] + r0[2] +
                   r1[0] + r1[1] + r1[2] +
                   r2[0] + r2[1] + r2[2];
        }
    }
}

/// do a convolution blur of the data.
pub fn blur_biomes(input: &[&[SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>]]) -> Vec<Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>> {
    let mut output: Vec<Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>> = input.iter().cloned().map(|v| v.to_vec()).collect_vec();
    for (in_rows, out_row) in Convolution3::new(input).zip(&mut output) {
        for (((r0, r1), r2), out) in
            Convolution3::new(in_rows[0])
            .zip(Convolution3::new(in_rows[1]))
            .zip(Convolution3::new(in_rows[2]))
            .zip(out_row)
        {
            *out = r0.iter()
                .chain(&r1)
                .chain(&r2)
                .flat_map(|v| v)
                .fold(smallvec![], |mut acc: SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>, f| {
                    let blend = acc.iter_mut().find(|e| e.id == f.id);
                    if let Some(blend) = blend {
                        blend.weight += f.weight;
                    } else {
                        acc.push(*f);
                    }
                    acc
                });
        }
    }
    output
}