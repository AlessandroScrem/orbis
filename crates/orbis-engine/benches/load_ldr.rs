use criterion::{Criterion, criterion_group, criterion_main};
use std::fs;

/// Run benchmark:
///
/// '''rust, ignore
/// cargo bench --bench hdr_bench
/// '''
fn load_ldr_to_buffer_u8(path: &str) -> Vec<u8> {
    fs::read(path).expect("Failed to read LDR file")
}

fn load_image_rs(buffer: &[u8]) -> (Vec<u8>, u32, u32) {
    let image = image::load_from_memory(buffer).unwrap().to_rgba8();
    let (width, height) = image.dimensions();
    let raw_u8: Vec<u8> = image.into_raw();

    (raw_u8, width, height)
}

fn load_stb_image(buffer: &[u8]) -> (Vec<u8>, u32, u32) {
    use stb_image::image::LoadResult;
    use stb_image::image::load_from_memory_with_depth;
    // Caricamento LDR con stb_image

    let img = match load_from_memory_with_depth(buffer, 4, false) {
        LoadResult::ImageU8(img) => img,
        LoadResult::Error(e) => panic!("Failed: {}", e),
        _ => panic!("Unexpected format"),
    };

    let width = img.width as u32;
    let height = img.height as u32;
    let raw_u8 = img.data; // già in u8, RGB/RGBA

    (raw_u8, width, height)
}

// Benchmark
fn bench_loaders(c: &mut Criterion) {
    let path = orbis_engine::asset_path!("avocado/Avocado_baseColor.png");
    let raw_u8 = load_ldr_to_buffer_u8(path);

    c.bench_function("load mage_rs ", |b| b.iter(|| load_image_rs(&raw_u8)));
    c.bench_function("load stb_image", |b| b.iter(|| load_stb_image(&raw_u8)));
}

criterion_group!(benches, bench_loaders);
criterion_main!(benches);
