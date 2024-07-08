use criterion::criterion_main;

pub mod chunkbench;
pub mod zpackbench;

criterion_main!(chunkbench::chunk_benches, zpackbench::zpack_benches);
