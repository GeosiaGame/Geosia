//! Biome decorator utility functions.

use bevy::utils::smallvec::{smallvec, SmallVec};
use bevy_math::IVec2;
use hashbrown::HashMap;
use ocg_schemas::{
    coordinates::AbsBlockPos,
    registry::RegistryId,
    voxel::{
        biome::decorator::{BiomeDecoratorDefinition, BiomeDecoratorEntry},
        generation::Context,
    },
};
use rand::{Rng, SeedableRng};
use rand_xoshiro::Xoshiro512StarStar;

/// The size of a decorator group.
pub const GROUP_SIZE: i32 = 16;
/// half of the size of a decorator group.
pub const GROUP_SIZE_HALF: i32 = GROUP_SIZE / 2;
/// The area of a decorator group.
pub const GROUP_SIZE2: i32 = GROUP_SIZE * GROUP_SIZE;
/// A helper for the maximum size of a decorator group.
pub const GROUP_SIZEV: IVec2 = IVec2::splat(GROUP_SIZE);

fn decorator_positions_in_chunk(
    id: RegistryId,
    decorator: &BiomeDecoratorDefinition,
    ctx: &Context<'_>,
    group_xz: IVec2,
    global_xz: IVec2,
) -> SmallVec<[BiomeDecoratorEntry; 8]> {
    let heights = ctx.biome_map.heightmap_between(global_xz - GROUP_SIZE_HALF, global_xz + GROUP_SIZE_HALF);
    let (elevation, temperature, moisture) = ctx.biome_map.noise_map[&global_xz.to_array()];
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

    let mut noise = Xoshiro512StarStar::seed_from_u64(
        ctx.seed
            .wrapping_mul(decorator.salt.unsigned_abs() as u64)
            .wrapping_add(group_xz.x as u64)
            .wrapping_sub(group_xz.y as u64),
    );
    let range_x = rand::distributions::Uniform::new(-GROUP_SIZE_HALF, GROUP_SIZE_HALF);
    let range_y = rand::distributions::Uniform::new(-GROUP_SIZE_HALF, GROUP_SIZE_HALF);

    let mut seen = Vec::new();
    for _ in 0..count {
        let x = noise.sample(range_x);
        let y = noise.sample(range_y);
        let pos = IVec2::new(x + global_xz.x, y + global_xz.y);
        if seen.contains(&pos) {
            continue;
        }
        seen.push(pos);
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

/// Get positions for decorators around `pos_xz`, in a 16x16 area.
pub fn decorator_positions_around<'a>(
    id: RegistryId,
    decorator: &BiomeDecoratorDefinition,
    ctx: &Context<'_>,
    pos: IVec2,
    cache: &'a mut HashMap<IVec2, SmallVec<[BiomeDecoratorEntry; 8]>>,
) -> &'a mut SmallVec<[BiomeDecoratorEntry; 8]> {
    if cache.contains_key(&pos) {
        return cache.get_mut(&pos).unwrap();
    }
    let min_tree_group_xz = block_pos_to_decorator_group_pos(pos - GROUP_SIZE);
    let max_tree_group_xz = block_pos_to_decorator_group_pos(pos + GROUP_SIZE);
    let mut output = SmallVec::new();
    for tx in min_tree_group_xz.x..=max_tree_group_xz.x {
        for tz in min_tree_group_xz.y..=max_tree_group_xz.y {
            let group_pos = IVec2::new(tx, tz);
            let global_pos = pos + group_pos;
            output.append(
                &mut decorator_positions_in_chunk(id, decorator, ctx, group_pos, global_pos)
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
    cache.get_mut(&pos).unwrap()
}
