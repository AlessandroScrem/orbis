use stb_image::image::load_from_memory_with_depth;

/// Load HDR16 stb_image
pub fn decode_image_rgbaf32(buffer: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::load_from_memory(buffer)
        .map_err(|e| format!("Failed to decode image: {e}"))?
        .to_rgba32f();

    let (width, height) = image.dimensions();
    let raw_f32: Vec<f32> = image.into_raw();
    let pixels: Vec<u8> = bytemuck::cast_slice(&raw_f32).to_vec();

    Ok((pixels, width, height))
}

/// Load HDR16 stb_image
pub fn decode_stb_image_rgbaf16(buffer: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use half::f16;
    use rayon::prelude::*;
    use stb_image::image::LoadResult;
    const F16MAX: f32 = f16::MAX.to_f32_const();
    const CHANNELS: usize = 4;
    const BYTE_PER_PIXEL: usize = CHANNELS * 2;

    let img = match load_from_memory_with_depth(buffer, CHANNELS, false) {
        LoadResult::ImageF32(img) => img,
        LoadResult::Error(e) => return Err(format!("Failed: {e}")),
        _ => return Err("stb_image: Unexpected format".into()),
    };

    let width = img.width as u32;
    let height = img.height as u32;
    let num_pixels = (width * height) as usize;

    // Preallocate final buffer: 4 channels * 2 byte per pixel
    let mut raw_u8 = vec![0u8; num_pixels * BYTE_PER_PIXEL];

    // Parallel map: pixel source -> pixel destination
    raw_u8
        .par_chunks_mut(BYTE_PER_PIXEL) // 8 byte per pixel RGBA16
        .zip(img.data.par_chunks(CHANNELS)) // 4 float per pixel RGBA
        .for_each(|(dst, src)| {
            dst[0..2].copy_from_slice(&f16::from_f32(src[0].clamp(0.0, F16MAX)).to_le_bytes());
            dst[2..4].copy_from_slice(&f16::from_f32(src[1].clamp(0.0, F16MAX)).to_le_bytes());
            dst[4..6].copy_from_slice(&f16::from_f32(src[2].clamp(0.0, F16MAX)).to_le_bytes());
            dst[6..8].copy_from_slice(&f16::from_f32(src[3].clamp(0.0, F16MAX)).to_le_bytes());
        });
    Ok((raw_u8, width, height))
}

/// Load LDR stb_image
pub fn decode_stb_image_rgaba8(buffer: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use stb_image::image::LoadResult;

    let img = match load_from_memory_with_depth(buffer, 4, false) {
        LoadResult::ImageU8(img) => Ok(img),
        LoadResult::Error(e) => Err(format!("Failed: {e}")),
        _ => Err("stb_image: Unexpected format".into()),
    }?;

    Ok((img.data, img.width as u32, img.height as u32))
}
