use criterion::{Criterion, criterion_group, criterion_main};
use rayon::prelude::*;
use std::{fs, time::Duration};

/// Run benchmark:
///
/// '''rust, ignore
/// cargo bench --bench hdr_bench --quiet -- conversion
/// cargo bench --bench hdr_bench --quiet -- load
/// '''
fn load_hdr_to_buffer_u8(path: &str) -> Vec<u8> {
    fs::read(path).expect("Failed to read HDR file")
}

fn decode_image_rs_serial(buffer: &[u8]) -> (Vec<u8>, u32, u32) {
    let image = image::load_from_memory(buffer).unwrap().to_rgba32f();
    let (width, height) = image.dimensions();
    let raw_f32: Vec<f32> = image.into_raw();
    // conversione diretta in Vec<u8> per Rgba16Float
    // fit 32bit value to range [0.0 65504.0]
    let raw_u8: Vec<u8> = raw_f32
        .iter()
        .flat_map(|f| {
            let clamped = f.clamp(0.0, half::f16::MAX.to_f32());
            half::f16::from_f32(clamped).to_le_bytes()
        })
        .collect();
    (raw_u8, width, height)
}

fn decode_image_rs_parallel(buffer: &[u8]) -> (Vec<u8>, u32, u32) {
    use rayon::iter::ParallelIterator;
    let image = image::load_from_memory(buffer).unwrap().to_rgba32f();
    let (width, height) = image.dimensions();
    let raw_f32: Vec<f32> = image.into_raw();

    // conversione parallela in Vec<u8> per Rgba16Float
    // fit 32bit value to range [0.0 65504.0]

    let raw_u8: Vec<u8> = raw_f32
        .par_iter() // Rayon parallel iterator
        .flat_map_iter(|f| {
            let clamped = f.clamp(0.0, half::f16::MAX.to_f32());
            half::f16::from_f32(clamped).to_le_bytes()
        })
        .collect();

    (raw_u8, width, height)
}

fn decode_stb_image_parallel(buffer: &[u8]) -> (Vec<u8>, u32, u32) {
    use half::f16;
    use rayon::prelude::*;
    use stb_image::image::LoadResult;
    use stb_image::image::load_from_memory_with_depth;
    // Caricamento HDR con stb_image

    let img = match load_from_memory_with_depth(buffer, 4, false) {
        LoadResult::ImageF32(img) => img,
        LoadResult::Error(e) => panic!("Failed: {}", e),
        _ => panic!("Unexpected format"),
    };

    let width = img.width as u32;
    let height = img.height as u32;
    let raw_f32 = img.data; // già in f32, RGB/RGBA

    // Conversione parallela in Vec<u8> per Rgba16Float
    let raw_u8: Vec<u8> = raw_f32
        .par_iter()
        .flat_map_iter(|f| {
            let clamped = f.clamp(0.0, f16::MAX.to_f32());
            f16::from_f32(clamped).to_le_bytes()
        })
        .collect();

    (raw_u8, width, height)
}

fn decode_stb_image_parallel2(buffer: &[u8]) -> (Vec<u8>, u32, u32) {
    use half::f16;
    use rayon::prelude::*;
    use stb_image::image::LoadResult;
    use stb_image::image::load_from_memory_with_depth;
    // Caricamento HDR con stb_image

    let img = match load_from_memory_with_depth(buffer, 4, false) {
        LoadResult::ImageF32(img) => img,
        LoadResult::Error(e) => panic!("Failed: {}", e),
        _ => panic!("Unexpected format"),
    };

    let width = img.width as u32;
    let height = img.height as u32;
    let num_pixels = (width * height) as usize;

    // Prealloca il buffer finale: 4 canali * 2 byte per pixel
    let mut raw_u8 = vec![0u8; num_pixels * 4 * 2];

    // Parallel map diretto: pixel source -> pixel destination
    raw_u8
        .par_chunks_mut(8) // 8 byte per pixel RGBA16
        .zip(img.data.par_chunks(4)) // 4 float per pixel RGBA, passiamo un reference
        .for_each(|(dst, src)| {
            dst[0..2].copy_from_slice(
                &f16::from_f32(src[0].clamp(0.0, f16::MAX.to_f32())).to_le_bytes(),
            );
            dst[2..4].copy_from_slice(
                &f16::from_f32(src[1].clamp(0.0, f16::MAX.to_f32())).to_le_bytes(),
            );
            dst[4..6].copy_from_slice(
                &f16::from_f32(src[2].clamp(0.0, f16::MAX.to_f32())).to_le_bytes(),
            );
            dst[6..8].copy_from_slice(
                &f16::from_f32(src[3].clamp(0.0, f16::MAX.to_f32())).to_le_bytes(),
            );
        });

    (raw_u8, width, height)
}

// Benchmark
fn bench_conversions(c: &mut Criterion) {
    let path = orbis_engine::asset_path!("core/clarens_night_02_2k.hdr");
    let raw_u8 = load_hdr_to_buffer_u8(path);

    c.bench_function("conversion: image_rs ser", |b| {
        b.iter(|| decode_image_rs_serial(&raw_u8))
    });
    c.bench_function("conversion: image_rs par", |b| {
        b.iter(|| decode_image_rs_parallel(&raw_u8))
    });
    c.bench_function("conversion: stb_image flat_map_iter + collect", |b| {
        b.iter(|| decode_stb_image_parallel(&raw_u8))
    });
    c.bench_function("conversion: stb_image par_chunks_mut + zip", |b| {
        b.iter(|| decode_stb_image_parallel2(&raw_u8))
    });
}

fn load_image_rs(buffer: &[u8]) -> (Vec<f32>, u32, u32) {
    let image = image::load_from_memory(buffer).unwrap().to_rgba32f();
    let (width, height) = image.dimensions();
    let raw_f32: Vec<f32> = image.into_raw();

    (raw_f32, width, height)
}

fn load_stb_image(buffer: &[u8]) -> (Vec<f32>, u32, u32) {
    use stb_image::image::LoadResult;
    use stb_image::image::load_from_memory_with_depth;
    // Caricamento HDR con stb_image

    let img = match load_from_memory_with_depth(buffer, 4, false) {
        LoadResult::ImageF32(img) => img,
        LoadResult::Error(e) => panic!("Failed: {}", e),
        _ => panic!("Unexpected format"),
    };

    let width = img.width as u32;
    let height = img.height as u32;
    let raw_f32 = img.data; // già in f32, RGB/RGBA

    (raw_f32, width, height)
}

// Benchmark
fn bench_loaders(c: &mut Criterion) {
    let path = orbis_engine::asset_path!("core/clarens_night_02_2k.hdr");
    let raw_u8 = load_hdr_to_buffer_u8(path);

    c.bench_function("load image_rs ", |b| b.iter(|| load_image_rs(&raw_u8)));
    c.bench_function("load stb_image", |b| b.iter(|| load_stb_image(&raw_u8)));
}

// ---------------------------
// Config personalizzata
// ---------------------------
fn custom_criterion() -> Criterion {
    Criterion::default()
        .sample_size(50) // meno sample -> meno warning
        .measurement_time(Duration::from_secs(10)) // più tempo di misura
        .warm_up_time(Duration::from_secs(3))
        .without_plots() // evita messaggio gnuplot
        .configure_from_args() // permette --quiet
}

// ---------------------------
// Group con config custom
// ---------------------------
criterion_group! {
    name = load_benches;
    config = custom_criterion();
    targets = bench_loaders
}

criterion_group! {
    name = conversion_benches;
    config = custom_criterion();
    targets = bench_conversions
}

criterion_main!(load_benches, conversion_benches);
