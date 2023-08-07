use std::cell::Cell;

use criterion::{black_box, criterion_group, BenchmarkId, Criterion};
use ocg_schemas::coordinates::{InChunkPos, CHUNK_DIM3, CHUNK_DIM3Z};
use ocg_schemas::voxel::chunk_storage::{ChunkStorage, PaletteStorage};
use rand::distributions::Uniform;
use rand::prelude::*;
use rand_pcg::Pcg64Mcg;

const RANDOM_SEED: u64 = 0xd48ba01b5725fd49;

pub fn fill_chunk_with_random(block_types: u16, chunk: &mut dyn ChunkStorage<u64>) {
    let mut rng = Pcg64Mcg::seed_from_u64(RANDOM_SEED);
    let mut blocks = Vec::with_capacity(block_types as usize);
    for _ in 0..block_types {
        let mut blk = rng.next_u64();
        while blocks.contains(&blk) {
            blk = rng.next_u64();
        }
        blocks.push(blk);
    }
    let blockdist = Uniform::new(0, blocks.len());
    for pos in 0..CHUNK_DIM3Z {
        chunk.put(
            InChunkPos::try_from_index(pos).unwrap(),
            blocks[blockdist.sample(&mut rng)],
        );
    }
}

pub fn random_paletted_chunk(block_types: u16) -> PaletteStorage<u64> {
    let mut palstorage = PaletteStorage::default();
    fill_chunk_with_random(block_types, &mut palstorage);
    palstorage
}

fn bench_random_paletted_chunk(c: &mut Criterion) {
    for block_types in [1, 8, 32, 128, 16384] {
        c.bench_with_input(
            BenchmarkId::new("Random chunk - Palette storage", block_types.to_string()),
            &block_types,
            |b, &block_types| b.iter(|| random_paletted_chunk(block_types)),
        );
    }
}

fn chunk_get(c: &mut Criterion) {
    for block_types in [1, 8, 32, 128, 16384] {
        let chunk = random_paletted_chunk(block_types);
        let cidx = Cell::new(0usize);
        c.bench_with_input(
            BenchmarkId::new("Get block - Palette storage", block_types.to_string()),
            &block_types,
            |b, _| {
                b.iter(|| {
                    let cpos = InChunkPos::try_from_index(cidx.get()).unwrap();
                    let val = chunk.get(black_box(cpos));
                    cidx.set((cidx.get() + 1) % CHUNK_DIM3 as usize);
                    val
                })
            },
        );
    }
}

fn chunk_get_copy(c: &mut Criterion) {
    for block_types in [1, 8, 32, 128, 16384] {
        let chunk = random_paletted_chunk(block_types);
        let cidx = Cell::new(0usize);
        c.bench_with_input(
            BenchmarkId::new("Get block copy - Palette storage", block_types.to_string()),
            &block_types,
            |b, _| {
                b.iter(|| {
                    let cpos = InChunkPos::try_from_index(cidx.get()).unwrap();
                    let val = chunk.get_copy(black_box(cpos));
                    cidx.set((cidx.get() + 1) % CHUNK_DIM3 as usize);
                    val
                })
            },
        );
    }
}

criterion_group!(chunk_benches, bench_random_paletted_chunk, chunk_get, chunk_get_copy);
