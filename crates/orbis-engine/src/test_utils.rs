use std::sync::OnceLock;
use wgpu::Extent3d;

static DEVICE_AND_QUEUE: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();
fn get_device_and_queue() -> &'static (wgpu::Device, wgpu::Queue) {
    DEVICE_AND_QUEUE.get_or_init(|| {
        let instance = wgpu::Instance::default();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .unwrap();

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

        (device, queue)
    })
}

#[allow(dead_code)]
pub fn get_gpu_context_test() -> crate::gpu::GpuContextRef<'static> {
    let (device, queue) = get_device_and_queue();
    crate::gpu::GpuContextRef { device, queue }
}

/// Save a 2D texture to a file (png).
/// Supported formats: Rgba8Unorm, Rg16Float, Rgba16Float
/// The output image will be in RGBA8 format.
/// # Arguments
/// * `device` - WGPU device
/// * `queue` - WGPU queue
/// * `filename` - Output filename
/// * `texture` - The texture to save
/// * `z` - The array layer or depth slice to save (for 2D textures, use 0)
/// # Returns
/// * `Ok(())` on success, or an error if something went wrong
///
#[allow(dead_code)]
pub fn save_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    filename: &str,
    texture: &wgpu::Texture,
    z: u32,
) -> anyhow::Result<()> {
    let width = texture.width();
    let height = texture.height();
    let format = texture.format();

    let pixel_size: u32 = match format {
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Rg16Float => 4,
        wgpu::TextureFormat::Rgba16Float => 8,
        _ => {
            println!("'save_texture': unsupported texture format {:?}", format);
            panic!()
        }
    };

    // Bytes per row per WGPU (padded a 256)
    let bytes_per_row_unpadded = width * pixel_size;
    let bytes_per_row_padded = align_to(bytes_per_row_unpadded, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let output_buffer_size = (bytes_per_row_padded * width) as wgpu::BufferAddress;

    let output_buffer_desc = wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST
        // this tells wpgu that we want to read this buffer from the cpu
        | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    };

    let output_buffer = device.create_buffer(&output_buffer_desc);

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z },
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row_padded),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                depth_or_array_layers: 1,
                width,
                height,
            },
        );

        queue.submit(Some(encoder.finish()));
    }

    // We need to scope the mapping variables so that we can
    // unmap the buffer
    {
        let buffer_slice = output_buffer.slice(..);

        // The mapping process is async, so we'll need to create a channel to get
        // the success flag for our mapping
        let (tx, rx) = std::sync::mpsc::channel();

        // We send the success or failure of our mapping via a callback
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });

        // The callback we submitted to map async will only get called after the
        // device is polled or the queue submitted
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;

        // We check if the mapping was successful here
        rx.recv()??;

        match format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                let padded_data = buffer_slice.get_mapped_range();
                let unpadded_data = unpad_image(
                    &padded_data,
                    width,
                    height,
                    pixel_size,
                    bytes_per_row_padded,
                );
                let data_rgba8 = unpadded_data;

                use image::{ImageBuffer, Rgba};
                let buffer =
                    ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, data_rgba8).unwrap();
                buffer.save(filename).unwrap();
            }
            wgpu::TextureFormat::Rg16Float => {
                let padded_data = buffer_slice.get_mapped_range();
                let unpadded_data = unpad_image(
                    &padded_data,
                    width,
                    height,
                    pixel_size,
                    bytes_per_row_padded,
                );
                let data_rgba8 = rg16float_to_rgba8(&unpadded_data, width, height);

                use image::{ImageBuffer, Rgba};
                let buffer =
                    ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, data_rgba8).unwrap();
                buffer.save(filename).unwrap();
            }
            wgpu::TextureFormat::Rgba16Float => {
                let padded_data = buffer_slice.get_mapped_range();

                let unpadded_data = unpad_image(
                    &padded_data,
                    width,
                    height,
                    pixel_size,
                    bytes_per_row_padded,
                );
                let data_rgba8 = rgba16float_to_rgba8(&unpadded_data, width, height);

                use image::{ImageBuffer, Rgba};
                let buffer =
                    ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, data_rgba8).unwrap();
                buffer.save(filename).unwrap();
            }
            _ => panic!("'save_texture': unsupported texture format"),
        }
    }
    output_buffer.unmap();

    Ok(())
}

/// Save a cubemap texture to a cross image file (png).
/// The texture must have 6 array layers (faces) and mipmaps.
/// The output file will contain all mip levels, named filename_mip0.png, filename_mip1.png, etc.
/// Supported formats: Rgba8Unorm, Rg16Float, Rgba16Float
/// The output image will be in RGBA8 format.
/// # Arguments
/// * `device` - WGPU device
/// * `queue` - WGPU queue
/// * `filename_base` - Base filename for output files (mip level will be appended)
/// * `texture` - The cubemap texture to save
/// # Returns
/// * `Ok(())` on success, or an error if something went wrong
///
#[allow(dead_code)]
pub fn save_cubemap_cross(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    filename_base: &str,
    texture: &wgpu::Texture,
) -> anyhow::Result<()> {
    use image::{ImageBuffer, Rgba};

    let format = texture.format();
    let base_size = texture.width();
    let mip_level_count = texture.mip_level_count();
    let face_count = 6;

    let pixel_size: u32 = match format {
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Rg16Float => 4,
        wgpu::TextureFormat::Rgba16Float => 8,
        _ => {
            println!("'save_texture': unsupported texture format {:?}", format);
            panic!()
        }
    };

    for mip_level in 0..mip_level_count {
        let mip_size = (base_size >> mip_level).max(1);

        // Bytes per row per WGPU (padded a 256)
        let bytes_per_row_unpadded = mip_size * pixel_size;
        let bytes_per_row_padded =
            align_to(bytes_per_row_unpadded, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let output_buffer_size = (bytes_per_row_padded * mip_size) as wgpu::BufferAddress;

        // Bitmap croce
        let cross_width = mip_size * 4;
        let cross_height = mip_size * 3;
        let mut cross_image = vec![0u8; (cross_width * cross_height * 4) as usize]; // 4 byte per pixel finale

        for face in 0..face_count {
            let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                size: output_buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                label: None,
                mapped_at_creation: false,
            });

            // Copia la faccia del mipmap
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &output_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row_padded),
                        rows_per_image: Some(mip_size),
                    },
                },
                Extent3d {
                    width: mip_size,
                    height: mip_size,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));

            // Mappa e leggi dati
            let face_rgba8 = {
                let slice = output_buffer.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |res| tx.send(res).unwrap());
                device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })?;
                rx.recv()??;

                let data = slice.get_mapped_range();

                let unpadded_data = unpad_image(
                    &data,
                    mip_size,             // width
                    mip_size,             // height
                    pixel_size,           // ex, RGBA16F → 8 byte per pixel
                    bytes_per_row_padded, // pitch da wgpu
                );

                match format {
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                        unpadded_data
                    }
                    wgpu::TextureFormat::Rg16Float => {
                        rg16float_to_rgba8(&unpadded_data, mip_size, mip_size)
                    }
                    wgpu::TextureFormat::Rgba16Float => {
                        rgba16float_to_rgba8(&unpadded_data, mip_size, mip_size)
                    }
                    _ => {
                        println!("'save_texture': unsupported texture format {:?}", format);
                        panic!()
                    }
                }
            };

            output_buffer.unmap();

            // // Offset croce
            let (offset_x, offset_y) = match face {
                0 => (2 * mip_size, mip_size), // +X
                1 => (0, mip_size),            // -X
                3 => (mip_size, 2 * mip_size), // -Y
                2 => (mip_size, 0),            // +Y
                4 => (mip_size, mip_size),     // +Z
                5 => (3 * mip_size, mip_size), // -Z
                _ => (0, 0),
            };

            // Copia i dati nella cross_image
            for y in 0..mip_size as usize {
                for x in 0..mip_size as usize {
                    let src_idx = (y * mip_size as usize + x) * 4;
                    let dst_x = offset_x as usize + x;
                    let dst_y = offset_y as usize + y;
                    let dst_idx = (dst_y * cross_width as usize + dst_x) * 4;
                    cross_image[dst_idx..dst_idx + 4]
                        .copy_from_slice(&face_rgba8[src_idx..src_idx + 4]);
                }
            }
        }

        // Salva il mipmap
        let filename = format!("{}_mip{}.png", filename_base, mip_level);
        let buffer =
            ImageBuffer::<Rgba<u8>, _>::from_raw(cross_width, cross_height, cross_image).unwrap();
        buffer.save(filename)?;
    }

    Ok(())
}

/// Remove padding from data read from texture  eg: copy_texture_to_buffer().
/// `data` slice mapped from GPU buffer.
/// `width` = in pixel
/// `height` = in pixel
/// `bytes_per_pixel` = how many byte per pixel (es. RGBA8 = 4, RGBA16F = 8, ecc.)
/// `bytes_per_row_padded` = row pitch returned from wgpu
///
fn unpad_image(
    data: &[u8],
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
    bytes_per_row_padded: u32,
) -> Vec<u8> {
    let bytes_per_row_unpadded = width * bytes_per_pixel;
    let mut unpadded = vec![0u8; (bytes_per_row_unpadded * height) as usize];

    for y in 0..height as usize {
        let src_start = y * bytes_per_row_padded as usize;
        let dst_start = y * bytes_per_row_unpadded as usize;

        unpadded[dst_start..dst_start + bytes_per_row_unpadded as usize]
            .copy_from_slice(&data[src_start..src_start + bytes_per_row_unpadded as usize]);
    }

    unpadded
}

/// Align `value` up to the nearest multiple of `alignment`.
/// `alignment` must be a power of two.
///
fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

/// Convert RG16F data to RGBA8 data.
/// `raw` = input data slice
/// `width` = in pixel
/// `height` = in pixel
/// Returns a Vec<u8> with RGBA8 data.
/// The B channel is set to 0, and A channel to 255.
/// Each pixel in input is 4 bytes (2 half-floats), output is 4 bytes (4 u8).
/// Clamps values to [0,1] before conversion.
/// # Panics
/// Panics if `raw` length is not equal to width * height * 4.
///
fn rg16float_to_rgba8(raw: &[u8], width: u32, height: u32) -> Vec<u8> {
    use half::f16;
    let mut out = Vec::with_capacity((width * height * 4) as usize);

    for i in 0..(width * height) as usize {
        // Ogni pixel = 4 byte = 2 half-float
        let offset = i * 4;
        let r_half = u16::from_le_bytes([raw[offset], raw[offset + 1]]);
        let g_half = u16::from_le_bytes([raw[offset + 2], raw[offset + 3]]);

        let r = f16::from_bits(r_half).to_f32();
        let g = f16::from_bits(g_half).to_f32();

        // Converti in [0,255]
        let r_u8 = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g_u8 = (g.clamp(0.0, 1.0) * 255.0).round() as u8;

        // B=0, A=255 (o come preferisci)
        out.push(r_u8);
        out.push(g_u8);
        out.push(0);
        out.push(255);
    }

    out
}
/// Convert RGBA16F data to RGBA8 data.
/// `raw` = input data slice
/// `width` = in pixel
/// `height` = in pixel
/// Returns a Vec<u8> with RGBA8 data.
/// Each pixel in input is 8 bytes (4 half-floats), output is 4 bytes (4 u8).
/// Clamps values to [0,1] before conversion.
/// # Panics
/// Panics if `raw` length is not equal to width * height * 8.
///
fn rgba16float_to_rgba8(raw: &[u8], width: u32, height: u32) -> Vec<u8> {
    use half::f16;
    // Ogni pixel = 8 byte = 4 half-float
    let pixel_size = 8;
    let mut out = Vec::with_capacity((width * height * pixel_size) as usize);

    for i in 0..(width * height) as usize {
        let offset = i * pixel_size as usize;
        let r_half = u16::from_le_bytes([raw[offset], raw[offset + 1]]);
        let g_half = u16::from_le_bytes([raw[offset + 2], raw[offset + 3]]);
        let b_half = u16::from_le_bytes([raw[offset + 4], raw[offset + 5]]);
        let a_half = u16::from_le_bytes([raw[offset + 6], raw[offset + 7]]);

        let r = f16::from_bits(r_half).to_f32();
        let g = f16::from_bits(g_half).to_f32();
        let b = f16::from_bits(b_half).to_f32();
        let a = f16::from_bits(a_half).to_f32();

        // Converti in [0,255]
        let r_u8 = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g_u8 = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b_u8 = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
        let a_u8 = (a.clamp(0.0, 1.0) * 255.0).round() as u8;

        // B=0, A=255 (o come preferisci)
        out.push(r_u8);
        out.push(g_u8);
        out.push(b_u8);
        out.push(a_u8);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use half::f16;
    /// Supported formats: Rgba8Unorm, Rg16Float, Rgba16Float, Rgba32Float
    pub fn create_debug_cube_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        base_size: u32,
    ) -> wgpu::Texture {
        let bytes_per_texel = match format {
            wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Rg16Float => 4,
            wgpu::TextureFormat::Rgba16Float => 8,
            wgpu::TextureFormat::Rgba32Float => 16,
            _ => panic!("Formato non supportato"),
        };

        let mip_levels = (base_size as f32).log2().floor() as u32 + 1;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Generic Test Cube Texture"),
            size: wgpu::Extent3d {
                width: base_size,
                height: base_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        // Base colors in 0..1 per face
        let colors: [[f32; 4]; 6] = [
            [1.0, 0.0, 0.0, 1.0], // +X red
            [0.0, 1.0, 0.0, 1.0], // -X green
            [0.0, 0.0, 1.0, 1.0], // +Y blue
            [1.0, 1.0, 0.0, 1.0], // -Y yellow
            [1.0, 0.0, 1.0, 1.0], // +Z magenta
            [0.0, 1.0, 1.0, 1.0], // -Z cyan
        ];

        for (face, c) in colors.iter().enumerate() {
            let mut mip_size = base_size;
            let mut mip_level = 0;

            while mip_size > 0 {
                let unaligned_bytes_per_row = mip_size * bytes_per_texel;
                let bytes_per_row = unaligned_bytes_per_row.div_ceil(256) * 256;
                let rows_per_image = mip_size;
                let mut data = vec![0u8; (bytes_per_row * rows_per_image) as usize];

                for y in 0..mip_size {
                    for x in 0..mip_size {
                        let offset = (y * bytes_per_row + x * bytes_per_texel) as usize;

                        match format {
                            wgpu::TextureFormat::Rgba8Unorm
                            | wgpu::TextureFormat::Rgba8UnormSrgb => {
                                data[offset] = (c[0] * 255.0) as u8;
                                data[offset + 1] = (c[1] * 255.0) as u8;
                                data[offset + 2] = (c[2] * 255.0) as u8;
                                data[offset + 3] = (c[3] * 255.0) as u8;
                            }
                            wgpu::TextureFormat::Rgba16Float => {
                                for i in 0..4 {
                                    let half = f16::from_f32(c[i] * (1.0 - mip_level as f32 * 0.2));
                                    let bytes = half.to_le_bytes();
                                    data[offset + i * 2] = bytes[0];
                                    data[offset + i * 2 + 1] = bytes[1];
                                }
                            }
                            wgpu::TextureFormat::Rg16Float => {
                                for i in 0..2 {
                                    let half = f16::from_f32(c[i] * (1.0 - mip_level as f32 * 0.2));
                                    let bytes = half.to_le_bytes();
                                    data[offset + i * 2] = bytes[0];
                                    data[offset + i * 2 + 1] = bytes[1];
                                }
                            }
                            wgpu::TextureFormat::Rgba32Float => {
                                for i in 0..4 {
                                    let bytes =
                                        (c[i] * (1.0 - mip_level as f32 * 0.2)).to_le_bytes();
                                    data[offset + i * 4..offset + i * 4 + 4]
                                        .copy_from_slice(&bytes);
                                }
                            }
                            _ => unreachable!(),
                        }
                    }
                }

                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(rows_per_image),
                    },
                    wgpu::Extent3d {
                        width: mip_size,
                        height: mip_size,
                        depth_or_array_layers: 1,
                    },
                );

                mip_size /= 2;
                mip_level += 1;
            }
        }

        texture
    }

    #[test]
    fn should_create_device_and_queue() {
        let _ = get_device_and_queue();
    }

    #[test]
    fn should_save_texture_dummy_rgba8unorm_to_file() {
        let (device, queue) = get_device_and_queue();
        let _rgba = create_debug_cube_texture(device, queue, wgpu::TextureFormat::Rgba8Unorm, 64);

        #[cfg(feature = "save_tests")]
        save_texture(&device, &queue, "testimage.png", &_rgba, 0).unwrap();
    }

    #[test]
    fn should_save_texture_dummy_rgba8unormsrgb_to_file() {
        let (device, queue) = get_device_and_queue();

        let _rgba =
            create_debug_cube_texture(device, queue, wgpu::TextureFormat::Rgba8UnormSrgb, 64);

        #[cfg(feature = "save_tests")]
        save_texture(&device, &queue, "testimage.png", &_rgba, 0).unwrap();
    }

    #[test]
    fn should_save_texture_dummy_rg16f_to_file() {
        let (device, queue) = get_device_and_queue();

        let _rg16f = create_debug_cube_texture(device, queue, wgpu::TextureFormat::Rg16Float, 64);

        #[cfg(feature = "save_tests")]
        save_texture(&device, &queue, "testimage.png", &_rg16f, 0).unwrap();
    }

    #[test]
    fn should_save_texture_dummy_rgba16f_to_file() {
        let (device, queue) = get_device_and_queue();

        let _rgba16f =
            create_debug_cube_texture(device, queue, wgpu::TextureFormat::Rgba16Float, 64);

        #[cfg(feature = "save_tests")]
        save_texture(&device, &queue, "testimage.png", &_rgba16f, 0).unwrap();
    }

    #[test]
    fn should_save_cubetexture_dummy_rgba8unorm_to_file() {
        let (device, queue) = get_device_and_queue();

        let _rgba8 = create_debug_cube_texture(device, queue, wgpu::TextureFormat::Rgba8Unorm, 64);

        #[cfg(feature = "save_tests")]
        save_cubemap_cross(&device, &queue, "testimage.png", &_rgba8).unwrap();
    }

    #[test]
    fn should_save_cubetexture_dummy_rgba16f_to_file() {
        let (device, queue) = get_device_and_queue();

        let _rgba16f =
            create_debug_cube_texture(device, queue, wgpu::TextureFormat::Rgba16Float, 64);

        #[cfg(feature = "save_tests")]
        save_cubemap_cross(&device, &queue, "testimage.png", &_rgba16f).unwrap();
    }
}
