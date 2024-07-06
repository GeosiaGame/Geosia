use gs_schemas::{
    coordinates::{AbsBlockPos, AbsChunkPos, InChunkPos, CHUNK_DIM},
    dependencies::itertools::{iproduct, Itertools},
    voxel::{
        biome::BiomeRegistry,
        chunk_storage::ChunkStorage,
        voxeltypes::{BlockRegistry, EMPTY_BLOCK_NAME},
    },
};
use image::{GenericImage, Rgba};

use crate::{
    voxel::{
        generator::StdGenerator,
        persistence::{memory::MemoryPersistenceLayer, ChunkPersistenceLayer},
    },
    ServerData,
};

fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
    to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
}

/// Make a bevy image out of the voronoi diagram.
#[allow(dead_code)]
pub fn draw_debug_maps(
    generator: &StdGenerator,
    biome_registry: &BiomeRegistry,
    block_registry: &BlockRegistry,
    persistence: &mut MemoryPersistenceLayer<ServerData>,
    width: usize,
    length: usize,
    height: i32,
) {
    let (empty_id, _) = block_registry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();

    let width_u32 = width as u32;
    let height_u32 = length as u32;
    let mut biome_img = image::DynamicImage::new_rgba8(width_u32, height_u32);
    let mut heightmap_img = image::DynamicImage::new_rgba8(width_u32, height_u32);

    let mut noise_img = image::DynamicImage::new_rgba8(width_u32, height_u32);
    let mut elevation_img = image::DynamicImage::new_rgba8(width_u32, height_u32);
    let mut temperature_img = image::DynamicImage::new_rgba8(width_u32, height_u32);
    let mut moisture_img = image::DynamicImage::new_rgba8(width_u32, height_u32);

    let mut original_cells_img = image::DynamicImage::new_rgba8(width_u32 * 2, height_u32 * 2);

    for (x, z) in iproduct!(0..width_u32, 0..height_u32) {
        let mapped_x = x as i32 - (width / 2) as i32;
        let mapped_z = z as i32 - (length / 2) as i32;

        // draw the world's borders on the voronoi cells image
        if x == 0 || x == width_u32 - 1 || z == 0 || z == width_u32 - 1 {
            original_cells_img.put_pixel(x + 64, z + 64, Rgba([0, 0, 255, 255]));
        }

        let point = [mapped_x, mapped_z];
        let biomes = generator.get_biomes_at_point(&point);
        if biomes.is_some() {
            let average_color = biomes
                .unwrap()
                .iter()
                .map(|p| (p.weight, p.lookup(biome_registry).unwrap().representative_color))
                .collect_vec();
            let mut color = [0.0; 3];
            let mut total_weight = 0.0;
            for (w, c) in &average_color {
                color[0] += c.r as f64 * w;
                color[2] += c.b as f64 * w;
                color[1] += c.g as f64 * w;
                total_weight += w;
            }
            if total_weight.abs() > f64::EPSILON {
                color[0] /= total_weight;
                color[1] /= total_weight;
                color[2] /= total_weight;
            }
            let color = [
                color[0].round() as u8,
                color[1].round() as u8,
                color[2].round() as u8,
                255,
            ];

            biome_img.put_pixel(x, z, Rgba(color));
        } else {
            biome_img.put_pixel(x, z, Rgba([0, 0, 0, 0]));
        }

        let noises = generator.get_noises_at_point(&point);
        if let Some(noises) = noises {
            let elevation = map_range((0.0, 5.0), (0.0, 255.0), noises.0) as u8;
            let temperature = map_range((0.0, 5.0), (0.0, 255.0), noises.1) as u8;
            let moisture = map_range((0.0, 5.0), (0.0, 255.0), noises.2) as u8;
            noise_img.put_pixel(x, z, Rgba([elevation, temperature, moisture, 255]));

            elevation_img.put_pixel(x, z, Rgba([elevation, elevation, elevation, 255]));
            temperature_img.put_pixel(x, z, Rgba([temperature, temperature, temperature, 255]));
            moisture_img.put_pixel(x, z, Rgba([moisture, moisture, moisture, 255]));
        }

        for y in (height - 1)..-height {
            let block_pos = AbsBlockPos::new(mapped_x, y, mapped_z);
            let chunk_pos = AbsChunkPos::from(block_pos);
            let in_chunk_pos = InChunkPos::try_new(
                block_pos.x - chunk_pos.x * CHUNK_DIM,
                block_pos.y - chunk_pos.y * CHUNK_DIM,
                block_pos.z - chunk_pos.z * CHUNK_DIM,
            )
            .unwrap();
            persistence.request_load(&[chunk_pos]);
            let chunk = persistence
                .try_dequeue_responses(1)
                .into_iter()
                .next()
                .unwrap()
                .1
                .unwrap();
            if chunk.blocks.get(in_chunk_pos).id != empty_id {
                let y_f = y as f64 / (height * 2) as f64 + height as f64;
                let y_c = (y_f * 255.0).clamp(0.0, 255.0).round() as u8;
                heightmap_img.put_pixel(x, z, Rgba([y_c, y_c, y_c, 255]));
                break;
            }
        }
    }

    for edge in generator.edges().iter() {
        let point_v_0 = edge.v0(generator).unwrap().point;
        let point_v_1 = edge.v1(generator).unwrap().point;
        let point_d_0 = edge.d0(generator).unwrap().point;
        let point_d_1 = edge.d1(generator).unwrap().point;
        let mut f = 0.0;
        while f <= 1.0 {
            let current_v = point_v_0.lerp(point_v_1, f);
            let current_d = point_d_0.lerp(point_d_1, f);
            original_cells_img.put_pixel(
                (current_v.x.round() as i32 + width as i32 / 2 + 64).clamp(0, width as i32 * 2 - 1) as u32,
                (current_v.y.round() as i32 + length as i32 / 2 + 64).clamp(0, length as i32 * 2 - 1) as u32,
                Rgba([255, 0, 0, 255]),
            );
            original_cells_img.put_pixel(
                (current_d.x.round() as i32 + width as i32 / 2 + 64).clamp(0, width as i32 * 2 - 1) as u32,
                (current_d.y.round() as i32 + length as i32 / 2 + 64).clamp(0, length as i32 * 2 - 1) as u32,
                Rgba([0, 255, 0, 255]),
            );
            f += 0.001;
        }
    }

    std::fs::create_dir_all("./output").expect("failed to make image output directory.");
    noise_img
        .save("./output/total_noise_map.png")
        .expect("failed to save total noise map image");
    elevation_img
        .save("./output/elevation_noise_map.png")
        .expect("failed to save elevation noise map image");
    temperature_img
        .save("./output/temperature_noise_map.png")
        .expect("failed to save temperature noise map image");
    moisture_img
        .save("./output/moisture_noise_map.png")
        .expect("failed to save moisture noise map image");

    biome_img
        .save("./output/biome_map.png")
        .expect("failed to save biome map image");
    heightmap_img
        .save("./output/height_map.png")
        .expect("failed to save height map image");

    original_cells_img
        .save("./output/original_cells.png")
        .expect("failed to save cell map image");
}
