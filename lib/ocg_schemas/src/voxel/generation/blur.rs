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

impl<'a, T: Copy> Convolution3<'a, T> {
    fn new(slice: &'a [T]) -> Self {
        Convolution3 {
            slice,
            state: Convolution3State::BeforeFirst,
        }
    }
}

impl<'a, T: Copy> Iterator for Convolution3<'a, T> {
    type Item = [T; 3];

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            Convolution3State::BeforeFirst => {
                self.state = Convolution3State::Middle(self.slice.windows(3));
                return Some([self.slice[0], self.slice[0], self.slice[1]]);
            }
            Convolution3State::Middle(ref mut windows) => {
                if let Some(window) = windows.next() {
                    return Some([window[0], window[1], window[2]]);
                }
            }
            Convolution3State::Done => return None,
        }
        // windows.next() returned None
        self.state = Convolution3State::Done;

        let last = self.slice.len() - 1;
        Some([self.slice[last - 1], self.slice[last - 1], self.slice[last]])
    }
}

/// do a convolution blur of the data.
pub fn blur<T: Add<Output = T> + Copy>(input: &[&[T]], output: &mut [&mut [T]]) {
    for (in_rows, out_row) in Convolution3::new(input).zip(output) {
        for (((r0, r1), r2), out) in Convolution3::new(in_rows[0])
            .zip(Convolution3::new(in_rows[1]))
            .zip(Convolution3::new(in_rows[2]))
            .zip(&mut **out_row)
        {
            *out = r0[0] + r0[1] + r0[2] + r1[0] + r1[1] + r1[2] + r2[0] + r2[1] + r2[2];
        }
    }
}

/// 15-radius biome convolution
struct Convolution16<'a, T: 'a> {
    slice: &'a [T],
    state: Convolution3State<'a, T>,
}

impl<'a, T: Clone> Convolution16<'a, T> {
    fn new(slice: &'a [T]) -> Self {
        Convolution16 {
            slice,
            state: Convolution3State::BeforeFirst,
        }
    }
}

impl<'a, T: Clone> Iterator for Convolution16<'a, T> {
    type Item = [T; 16];

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            Convolution3State::BeforeFirst => {
                self.state = Convolution3State::Middle(self.slice.windows(16));
                let slice_0 = &self.slice[0];
                return Some([
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    slice_0.clone(),
                    self.slice[1].clone(),
                ]);
            }
            Convolution3State::Middle(ref mut windows) => {
                if let Some(window) = windows.next() {
                    return Some([
                        window[0].clone(),
                        window[1].clone(),
                        window[2].clone(),
                        window[3].clone(),
                        window[4].clone(),
                        window[5].clone(),
                        window[6].clone(),
                        window[7].clone(),
                        window[8].clone(),
                        window[9].clone(),
                        window[10].clone(),
                        window[11].clone(),
                        window[12].clone(),
                        window[13].clone(),
                        window[14].clone(),
                        window[15].clone(),
                    ]);
                }
            }
            Convolution3State::Done => return None,
        }
        // windows.next() returned None
        self.state = Convolution3State::Done;

        let last = self.slice.len() - 1;
        let slice_second_last = &self.slice[last - 1];
        Some([
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            slice_second_last.clone(),
            self.slice[last].clone(),
        ])
    }
}

/// do a convolution blur of the data.
pub fn blur_biomes(
    input: &[&[SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>]],
) -> Vec<Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>> {
    let mut output: Vec<Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>> =
        input.iter().cloned().map(|v| v.to_vec()).collect_vec();
    for (in_rows, out_row) in Convolution16::new(input).zip(&mut output) {
        for ((((((((((((((((r0, r1), r2), r3), r4), r5), r6), r7), r8), r9), r10), r11), r12), r13), r14), r15), out) in
            Convolution16::new(in_rows[0])
                .zip(Convolution16::new(in_rows[1]))
                .zip(Convolution16::new(in_rows[2]))
                .zip(Convolution16::new(in_rows[3]))
                .zip(Convolution16::new(in_rows[4]))
                .zip(Convolution16::new(in_rows[5]))
                .zip(Convolution16::new(in_rows[6]))
                .zip(Convolution16::new(in_rows[7]))
                .zip(Convolution16::new(in_rows[8]))
                .zip(Convolution16::new(in_rows[9]))
                .zip(Convolution16::new(in_rows[10]))
                .zip(Convolution16::new(in_rows[11]))
                .zip(Convolution16::new(in_rows[12]))
                .zip(Convolution16::new(in_rows[13]))
                .zip(Convolution16::new(in_rows[14]))
                .zip(Convolution16::new(in_rows[15]))
                .zip(out_row)
        {
            *out = r0
                .iter()
                .chain(&r1)
                .chain(&r2)
                .chain(&r3)
                .chain(&r4)
                .chain(&r5)
                .chain(&r6)
                .chain(&r7)
                .chain(&r8)
                .chain(&r9)
                .chain(&r10)
                .chain(&r11)
                .chain(&r12)
                .chain(&r13)
                .chain(&r14)
                .chain(&r15)
                .flat_map(|v| v)
                .fold(
                    smallvec![],
                    |mut acc: SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>, f| {
                        let blend = acc.iter_mut().find(|e| e.id == f.id);
                        if let Some(blend) = blend {
                            blend.weight += f.weight;
                        } else {
                            acc.push(*f);
                        }
                        acc
                    },
                );
        }
    }
    output
}
