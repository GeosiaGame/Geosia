use bevy_math::prelude::*;
use criterion::{black_box, criterion_group, BenchmarkId, Criterion};
use gs_schemas::coordinates::{zpack_3d, zpack_3d_naive};

fn bench_zpack_3d(c: &mut Criterion) {
    let some_vec = IVec3::new(12989, -2173, 889);
    c.bench_with_input(BenchmarkId::new("zpack_3d", some_vec), &some_vec, |b, &i| {
        b.iter(move || zpack_3d(black_box(i)))
    });
}

fn bench_zpack_3d_naive(c: &mut Criterion) {
    let some_vec = IVec3::new(12989, -2173, 889);
    c.bench_with_input(BenchmarkId::new("naive_zpack_3d", some_vec), &some_vec, |b, &i| {
        b.iter(move || zpack_3d_naive(black_box(i)))
    });
}

criterion_group!(zpack_benches, bench_zpack_3d, bench_zpack_3d_naive);
