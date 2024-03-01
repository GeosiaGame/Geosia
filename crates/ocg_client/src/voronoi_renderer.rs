
use ocg_common::voxel::generator::newgen::NewGenerator;
use ocg_schemas::{dependencies::{image::{self, GenericImage, GenericImageView, Rgba}, itertools::{iproduct, Itertools}, rand::{distributions::Uniform, rngs::ThreadRng, Rng, SeedableRng}, voronoice::{Point, Voronoi}}, voxel::biome::BiomeRegistry};
use bevy::render::texture::Image;

fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
    to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
}

/// Make a bevy image out of the voronoi diagram.
pub fn draw_voronoi(voronoi: &Voronoi, generator: &NewGenerator<'_>, biome_registry: &BiomeRegistry, width: usize, height: usize) -> Image {
    let mut img = image::DynamicImage::new_rgba8(width as u32, height as u32);
    
    let mut rand = ocg_schemas::dependencies::rand::thread_rng();
    let range = ocg_schemas::dependencies::rand::distributions::Uniform::new(0_u8, 0xFF);
    for (x,y) in iproduct!(0..width as u32, 0..height as u32) {
        let mapped_x = map_range((0.0, width as f64), (-((width/2) as f64), (width/2) as f64), x as f64);
        let mapped_y = map_range((0.0, height as f64), (-((height/2) as f64), (height/2) as f64), y as f64);
        let point = Point {x: mapped_x, y: mapped_y};
        let biomes_at_point = generator.get_biomes_at_point(&point);

        if biomes_at_point.is_some() {
            let average_color = biomes_at_point.unwrap().iter()
                .map(|p| p.lookup(biome_registry).unwrap().representative_color)
                .collect_vec();
            let mut color = [0_u32; 4];
            for c in &average_color {
                color[0] += c.r as u32;
                color[1] += c.g as u32;
                color[2] += c.b as u32;
                color[3] += c.a as u32;
            }
            color[0] /= average_color.len() as u32;
            color[1] /= average_color.len() as u32;
            color[2] /= average_color.len() as u32;
            color[3] /= average_color.len() as u32;
            let color = [color[0] as u8, color[1] as u8, color[2] as u8, color[3] as u8];
            
            img.put_pixel(x, y, Rgba(color));
        } else {
            img.put_pixel(x, y, Rgba([rand.sample(range), rand.sample(range), rand.sample(range), 255]));
        }
    }

    // keep track of accumulated color per cell
    let mut cells = vec![(0_usize, 0_usize, 0_usize, 0_usize); voronoi.sites().len()];
    let mut pixel_to_site = vec![0; width * height];

    println!("Accumulating cell colors");
    let mut last_site = 0;
    for (x,y) in iproduct!(0..width-1, 0..height-1) {
        let pindex = width * y + x;
        let x = x as u32;
        let y = y as u32;

        // get site/voronoi cell for which pixel belongs to
        let (site, color) = get_cell(voronoi, last_site, x, y, &range);
        last_site = site;
        pixel_to_site[pindex] = site;

        // accumulate color per cell
        let pixel = img.get_pixel(x, y);
        let cell_site = &mut cells[site];

        cell_site.0 += color[0] as usize;
        cell_site.1 += color[1] as usize;
        cell_site.2 += color[2] as usize;
        cell_site.3 += 1;
    }

    println!("Averaging cell colors");
    // average value per cell
    for cell in cells.iter_mut() {
        if cell.3 > 0 {
            cell.0 /= cell.3;
            cell.1 /= cell.3;
            cell.2 /= cell.3;
        }
    }

    println!("Generating image");
    // assign averaged color to pixels in cells
    for (x,y) in iproduct!(0..width-1, 0..height-1) {
        let pindex = width * y + x;
        let x = x as u32;
        let y = y as u32;

        let site = pixel_to_site[pindex];
        let color = cells[site];
        let mut pixel = img.get_pixel(x, y);
        pixel.0[0] = color.0 as u8;
        pixel.0[1] = color.1 as u8;
        pixel.0[2] = color.2 as u8;
        img.put_pixel(x, y, pixel);   
    }

    
    img.save("biome_map.png").unwrap();
    // return a RGBA8 bevy image.
    Image::from_dynamic(img, false)
}

fn get_cell(voronoi: &Voronoi, current_site: usize, x: u32, y: u32, range: &Uniform<u8>) -> (usize, [u8; 3]) {
    let p = Point { x: x as f64, y: y as f64 };
    let cell = voronoi
        .cell(current_site)
        .iter_path(p)
        .last()
        .expect("Expected to find site that contains point");
    let mut rand = ocg_schemas::dependencies::rand_xoshiro::SplitMix64::seed_from_u64((current_site as u64).wrapping_shl(cell as u32).wrapping_mul(cell as u64).wrapping_add(current_site as u64));
    let color1 = rand.sample(range);
    let color2 = rand.sample(range);
    let color3 = rand.sample(range);
    (cell, [color1, color2, color3])
}
