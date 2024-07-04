use criterion::criterion_main;

pub mod chunkbench;

criterion_main!(chunkbench::chunk_benches);
