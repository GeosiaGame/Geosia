/// Noise for worldgen.

use noise::Seedable;


pub mod fbm_noise;

fn build_sources<Source>(seed: u32, octaves: &Vec<f64>) -> Vec<Source>
where
    Source: Default + Seedable,
{
    let mut sources = Vec::with_capacity(octaves.len());
    for x in 0..octaves.len() {
        let source = Source::default();
        sources.push(source.set_seed(seed + (octaves[x] * 100.0) as u32));
    }
    sources
}