
use ocg_common::voxel::generator::newgen::NewGenerator;
use ocg_schemas::{dependencies::{image::{self, GenericImage, Rgba}, itertools::{iproduct, Itertools}}, voxel::biome::BiomeRegistry};
use bevy::render::texture::Image;

fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
    to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
}

/// Make a bevy image out of the voronoi diagram.
pub fn draw_voronoi(generator: &NewGenerator<'_>, biome_registry: &BiomeRegistry, width: usize, height: usize) -> Image {
    let mut biome_img = image::DynamicImage::new_rgba8(width as u32, height as u32);
    
    let mut noise_img = image::DynamicImage::new_rgba8(width as u32, height as u32);
    let mut elevation_img = image::DynamicImage::new_rgba8(width as u32, height as u32);
    let mut temperature_img = image::DynamicImage::new_rgba8(width as u32, height as u32);
    let mut moisture_img = image::DynamicImage::new_rgba8(width as u32, height as u32);
    
    for (x,y) in iproduct!(0..width as u32, 0..height as u32) {
        let mapped_x = map_range((0.0, width as f64), (-((width/2) as f64), (width/2) as f64), x as f64) as i32;
        let mapped_y = map_range((0.0, height as f64), (-((height/2) as f64), (height/2) as f64), y as f64) as i32;
        let point = [mapped_x, mapped_y];
        let biomes = generator.get_biomes_at_point(&point);

        if biomes.is_some() {
            let average_color = biomes.unwrap().iter()
                .map(|p| p.lookup(biome_registry).unwrap().representative_color)
                .collect_vec();
            let mut color = [0_u32; 4];
            for c in &average_color {
                color[0] += c.r as u32;
                color[1] += c.g as u32;
                color[2] += c.b as u32;
                color[3] += c.a as u32;
            }
            let length = average_color.len() as u32;
            color[0] /= length;
            color[1] /= length;
            color[2] /= length;
            color[3] /= length;
            let color = [color[0] as u8, color[1] as u8, color[2] as u8, color[3] as u8];
            
            biome_img.put_pixel(x, y, Rgba(color));
        } else {
            biome_img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        }

        let noises = generator.get_noises_at_point(&point);
        if let Some(noises) = noises {
            let elevation = map_range((0.0, 5.0), (0.0, 255.0), noises.0) as u8;
            let temperature = map_range((0.0, 5.0), (0.0, 255.0), noises.1) as u8;
            let moisture = map_range((0.0, 5.0), (0.0, 255.0), noises.2) as u8;
            noise_img.put_pixel(x, y, Rgba([elevation, temperature, moisture, 255]));

            elevation_img.put_pixel(x, y, Rgba([elevation, elevation, elevation, 255]));
            temperature_img.put_pixel(x, y, Rgba([temperature, temperature, temperature, 255]));
            moisture_img.put_pixel(x, y, Rgba([moisture, moisture, moisture, 255]));
        }
    }
    
    std::fs::create_dir_all("./output").expect("failed to make image output directory.");
    noise_img.save("./output/total_noise_map.png").expect("failed to save total noise map image");
    elevation_img.save("./output/elevation_noise_map.png").expect("failed to save elevation noise map image");
    temperature_img.save("./output/temperature_noise_map.png").expect("failed to save temperature noise map image");
    moisture_img.save("./output/moisture_noise_map.png").expect("failed to save moisture noise map image");

    biome_img.save("./output/biome_map.png").expect("failed to save biome map image");
    // return a RGBA8 bevy image.
    Image::from_dynamic(biome_img, false)
}
