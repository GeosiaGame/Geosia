//! Criterion benchmarks for various core datastructures used by the game.

#![allow(missing_docs)] // We don't want these warnings inside macros

use criterion::criterion_main;

mod chunkbench;
mod zpackbench;

criterion_main!(chunkbench::chunk_benches, zpackbench::zpack_benches);
