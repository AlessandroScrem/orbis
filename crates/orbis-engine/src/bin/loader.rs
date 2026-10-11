use std::path::Path;

fn main() {
    let paths = [
        Path::new(orbis_engine::asset_path!("skybox/right.png")),
        Path::new(orbis_engine::asset_path!("skybox/left.png")),
        Path::new(orbis_engine::asset_path!("skybox/top.png")),
        Path::new(orbis_engine::asset_path!("skybox/bottom.png")),
        Path::new(orbis_engine::asset_path!("skybox/front.png")),
        Path::new(orbis_engine::asset_path!("skybox/back.png")),
    ];

    let start = std::time::Instant::now();
    let buffer = std::fs::read(paths[0]).expect("unable to read file");
    let elapsed = start.elapsed();

    println!("Read {} bytes in {:?}", buffer.len(), elapsed);

    let start = std::time::Instant::now();
    let image = image::load_from_memory(&buffer).unwrap();
    println!("Read image : {:?}", image.color());

    let elapsed = start.elapsed();
    println!(
        "Read from memory{} bytes in {:?}",
        image.as_bytes().len(),
        elapsed
    );

    let start = std::time::Instant::now();
    let image = image::ImageReader::open(paths[0])
        .unwrap()
        .decode()
        .unwrap();

    let elapsed = start.elapsed();
    println!(
        "Read fromImageReader {} bytes in {:?}",
        image.as_bytes().len(),
        elapsed
    );
}
