//! Biome decorator utility functions.

use bevy::utils::smallvec::{smallvec, SmallVec};
use bevy_math::IVec2;
use bluenoise::BlueNoise;
use hashbrown::HashMap;
use ocg_schemas::{
    coordinates::AbsBlockPos,
    dependencies::once_cell::sync::Lazy,
    registry::RegistryId,
    voxel::{
        biome::decorator::{BiomeDecoratorDefinition, BiomeDecoratorEntry},
        generation::Context,
    },
};
use rand_xoshiro::Xoshiro512StarStar;

const GROUP_SIZE: i32 = 16;
const GROUP_SIZE2: i32 = 16 * 16;
const GROUP_SIZEV: IVec2 = IVec2::splat(GROUP_SIZE);

static NOISE: Lazy<BlueNoise<Xoshiro512StarStar>> = Lazy::new(|| BlueNoise::<Xoshiro512StarStar>::new(16.0, 16.0, 4.0));

fn decorator_positions_in_chunk(
    id: RegistryId,
    decorator: &BiomeDecoratorDefinition,
    ctx: &Context<'_>,
    group_xz: IVec2,
) -> SmallVec<[BiomeDecoratorEntry; 8]> {
    let global_xz = group_xz * GROUP_SIZE;
    let heights = ctx.biome_map.heightmap_between(global_xz - 8, global_xz + 8);
    let (elevation, temperature, moisture) = ctx.biome_map.noise_map[&group_xz.to_array()];
    let count: usize = if let Some(count_fn) = decorator.count_fn {
        count_fn(decorator, ctx, elevation, temperature, moisture)
    } else {
        0
    };

    // generate random tree positions based on ctx.seed between group_xz +- (8,8)
    let mut positions = smallvec![];
    if count == 0 {
        return positions;
    }

    let mut noise = NOISE.clone();
    let noise = noise
        .with_seed(ctx.seed.wrapping_mul(decorator.salt.unsigned_abs() as u64))
        .take(count);

    for pos in noise {
        let pos = IVec2::new(pos.x as i32 + global_xz.x - 8, pos.y as i32 + global_xz.y - 8);
        positions.push(BiomeDecoratorEntry::new(
            id,
            AbsBlockPos::new(pos.x, *heights.get(&pos).unwrap_or(&0), pos.y),
            None,
        ));
    }
    positions
}

fn block_pos_to_decorator_group_pos(p: IVec2) -> IVec2 {
    p - p.rem_euclid(GROUP_SIZEV) // for both x/z
}

/// Get positions for decorators around the `pos_xz`, in a 16x16 area.
pub fn decorator_positions_around<'a>(
    id: RegistryId,
    decorator: &BiomeDecoratorDefinition,
    ctx: &Context<'_>,
    pos: IVec2,
    cache: &'a mut HashMap<IVec2, SmallVec<[BiomeDecoratorEntry; 8]>>,
) -> &'a SmallVec<[BiomeDecoratorEntry; 8]> {
    if cache.contains_key(&pos) {
        return &cache[&pos];
    }
    let min_tree_group_xz = block_pos_to_decorator_group_pos(pos - GROUP_SIZE);
    let max_tree_group_xz = block_pos_to_decorator_group_pos(pos + GROUP_SIZE);
    let mut output = SmallVec::new();
    for tx in min_tree_group_xz.x..=max_tree_group_xz.x {
        for tz in min_tree_group_xz.y..=max_tree_group_xz.y {
            output.append(
                &mut decorator_positions_in_chunk(id, decorator, ctx, IVec2::new(tx, tz))
                    .into_iter()
                    .filter(|entry| {
                        let distance_x = entry.pos.x - pos.x;
                        let distance_z = entry.pos.z - pos.y;
                        distance_x * distance_x + distance_z * distance_z <= GROUP_SIZE2
                    })
                    .collect::<SmallVec<[BiomeDecoratorEntry; 8]>>(),
            );
        }
    }
    cache.insert(pos, output);
    &cache[&pos]
}
