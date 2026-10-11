use crate::gpu::{context::GpuContextRef, gpu_readback::*};
use std::collections::HashSet;

pub enum PollResult<T> {
    Idle,
    Pending,
    Ready(T),
}

#[derive(Default)]
enum PickState {
    #[default]
    Idle,
    Pending(ReadbackHandle),
}

#[derive(Default)]
pub struct PickObject {
    state: PickState,
}

impl PickObject {
    pub fn request(&mut self, gpu: &GpuContextRef, texture: &wgpu::Texture, pos: (u32, u32)) {
        if !matches!(self.state, PickState::Idle) {
            return;
        }

        let pos = (
            pos.0.clamp(0, texture.width() - 1),
            pos.1.clamp(0, texture.height() - 1),
        );

        let handle = GpuReadback::request_readback(gpu.device, gpu.queue, texture, pos, (1, 1));

        self.state = PickState::Pending(handle);
    }
}

impl PickObject {
    pub fn poll(&mut self) -> PollResult<Option<u64>> {
        match &self.state {
            PickState::Idle => PollResult::Idle,

            PickState::Pending(handle) => match handle.try_recv() {
                None => PollResult::Pending,

                Some(result) => {
                    self.state = PickState::Idle;
                    PollResult::Ready(Self::decode(result))
                }
            },
        }
    }

    fn decode(results: ReadbackResult) -> Option<u64> {
        decode_entity_id(&results.bytes)
    }
}

#[derive(Default)]
enum SelectState {
    #[default]
    Idle,
    Pending(ReadbackHandle),
}

#[derive(Default)]
pub struct Select {
    state: SelectState,
}

impl Select {
    pub fn request(
        &mut self,
        gpu: &GpuContextRef,
        texture: &wgpu::Texture,
        origin: (u32, u32),
        size: (u32, u32),
    ) {
        if !matches!(self.state, SelectState::Idle) {
            return;
        }

        let origin = (
            origin.0.clamp(0, texture.width() - 1),
            origin.1.clamp(0, texture.height() - 1),
        );

        let size = (
            size.0.clamp(1, texture.width() - origin.0),
            size.1.clamp(1, texture.height() - origin.1),
        );
        self.state = SelectState::Pending(GpuReadback::request_readback(
            gpu.device, gpu.queue, texture, origin, size,
        ));
    }
}

impl Select {
    pub fn poll(&mut self) -> PollResult<Vec<u64>> {
        match &self.state {
            SelectState::Idle => PollResult::Idle,

            SelectState::Pending(handle) => match handle.try_recv() {
                None => PollResult::Pending,

                Some(result) => {
                    self.state = SelectState::Idle;
                    PollResult::Ready(Self::decode(result))
                }
            },
        }
    }

    // if let Some(readback) = &mut self.readback {
    //     if let Some(data) = readback.poll(&self.gpu_context.device) {
    //         println!("Buffer len: {} ", data.len());
    //         let mut ids = HashSet::new();
    //         for chunk in data.chunks_exact(8) {
    //             let id = u64::from_le_bytes(chunk.try_into().unwrap());
    //             if id != 0 {
    //                 ids.insert(EntityRawU64::from_raw_u64(id));
    //             }
    //         }
    //         bus.send_domain(Selection(SelectMulti(ids.into_iter().collect())));
    //         self.readback = None;
    //     }
    // }

    fn decode(results: ReadbackResult) -> Vec<u64> {
        results
            .bytes
            .chunks_exact(8)
            .filter_map(decode_entity_id)
            .collect::<HashSet<u64>>()
            .into_iter()
            .collect()
    }
}

fn decode_entity_id(bytes: &[u8]) -> Option<u64> {
    let pixel: [u8; 8] = bytes.get(..8)?.try_into().ok()?;
    let low = u32::from_le_bytes(pixel[..4].try_into().ok()?);
    let high = u32::from_le_bytes(pixel[4..].try_into().ok()?);
    let id = (u64::from(high) << 32) | u64::from(low);

    (id != 0).then_some(id)
}

pub enum QueryResult {
    Pick(Option<u64>),
    Selection(Vec<u64>),
}

#[derive(Default)]
pub struct ReadbackManager {
    pick: PickObject,
    selection: Select,
}

impl ReadbackManager {
    pub fn request_pick(&mut self, gpu: &GpuContextRef, texture: &wgpu::Texture, pos: (u32, u32)) {
        self.pick.request(gpu, texture, pos);
    }

    pub fn request_selection(
        &mut self,
        gpu: &GpuContextRef,
        texture: &wgpu::Texture,
        origin: (u32, u32),
        size: (u32, u32),
    ) {
        self.selection.request(gpu, texture, origin, size);
    }

    pub fn poll_results(&mut self) -> Option<QueryResult> {
        if let PollResult::Ready(id) = self.pick.poll() {
            return Some(QueryResult::Pick(id));
        }

        if let PollResult::Ready(ids) = self.selection.poll() {
            return Some(QueryResult::Selection(ids));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::caches::GpuTextureBuilder;
    use crate::gpu::static_textures;
    use crate::test_utils::get_gpu_context_test;

    #[test]
    fn should_read_pick() {
        let gpu = &get_gpu_context_test();
        let expected = (0x1234_5678_u64 << 32) | 0x9abc_def0_u64;
        let pixel = [
            (expected as u32).to_le_bytes(),
            ((expected >> 32) as u32).to_le_bytes(),
        ]
        .concat();

        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pick readback test texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Uint,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        let mut pick = PickObject::default();
        pick.request(gpu, &texture, (0, 0));

        let result = loop {
            GpuReadback::poll(gpu.device);

            match pick.poll() {
                PollResult::Pending => std::thread::yield_now(),
                PollResult::Ready(result) => break result,
                PollResult::Idle => std::thread::yield_now(),
            }
        };

        assert_eq!(result, Some(expected));
    }

    #[test]
    fn decode_entity_id_preserves_both_u32_components() {
        let expected = (0x1234_5678_u64 << 32) | 0x9abc_def0_u64;
        let low = (expected as u32).to_le_bytes();
        let high = ((expected >> 32) as u32).to_le_bytes();
        let bytes = [low, high].concat();

        assert_eq!(decode_entity_id(&bytes), Some(expected));
        assert_eq!(
            PickObject::decode(ReadbackResult {
                bytes: bytes.clone(),
                size: (1, 1),
            }),
            Some(expected)
        );
        assert_eq!(
            Select::decode(ReadbackResult {
                bytes: [bytes.clone(), bytes].concat(),
                size: (2, 1),
            }),
            vec![expected]
        );
    }

    #[test]
    fn decode_entity_id_ignores_zero_and_incomplete_pixels() {
        assert_eq!(decode_entity_id(&[0; 8]), None);
        assert_eq!(decode_entity_id(&[1, 2, 3, 4]), None);

        let zero_low = 0_u32.to_le_bytes();
        let zero_high = 0_u32.to_le_bytes();
        assert_eq!(
            Select::decode(ReadbackResult {
                bytes: [zero_low, zero_high].concat(),
                size: (1, 1),
            }),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn rectangular_selection_decodes_multiple_ids_and_ignores_zero_and_trailing_bytes() {
        let first = (0x0000_0002_u64 << 32) | 0x0000_0003_u64;
        let second = (0x1234_5678_u64 << 32) | 0x9abc_def0_u64;

        let encode = |id: u64| {
            [
                (id as u32).to_le_bytes(),
                ((id >> 32) as u32).to_le_bytes(),
            ]
            .concat()
        };

        let mut bytes = encode(first);
        bytes.extend_from_slice(&encode(0));
        bytes.extend_from_slice(&encode(second));
        bytes.extend_from_slice(&encode(first));
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]); // incomplete trailing pixel

        let mut actual = Select::decode(ReadbackResult {
            bytes,
            size: (5, 1),
        });
        actual.sort_unstable();

        let mut expected = vec![first, second];
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }

    #[test]
    fn should_read_pick_empty_request() {
        let mut pick = PickObject::default();

        match pick.poll() {
            PollResult::Idle => {}
            _ => panic!("expected idle"),
        }
    }

    #[test]
    fn should_ignore_multiple_pick_requests() {
        let gpu = &get_gpu_context_test();

        let gpu_texture =
            GpuTextureBuilder::from_static(&static_textures::LIGHTBULB_STATIC_TEXTURE).build(gpu);

        let texture = gpu_texture.inner;

        let mut pick = PickObject::default();

        // invio 100 richieste
        for _ in 0..100 {
            pick.request(&gpu, &texture, (15, 15));
        }

        let mut count = 0;

        loop {
            GpuReadback::poll(gpu.device);

            match pick.poll() {
                PollResult::Pending => {
                    std::thread::yield_now();
                }
                PollResult::Ready(_) => {
                    count += 1;
                }
                PollResult::Idle => {
                    break;
                }
            }
        }

        assert_eq!(count, 1);
    }

    #[test]
    fn should_read_select() {
        let gpu = &get_gpu_context_test();

        let gpu_texture =
            GpuTextureBuilder::from_static(&static_textures::LIGHTBULB_STATIC_TEXTURE).build(gpu);

        let texture = gpu_texture.inner;

        let mut select = Select::default();

        select.request(gpu, &texture, (0, 0), (10, 8));

        let mut select_received = false;

        loop {
            GpuReadback::poll(gpu.device);

            match select.poll() {
                PollResult::Pending => {
                    std::thread::yield_now();
                }
                PollResult::Ready(_) => {
                    select_received = true;
                }
                PollResult::Idle => {}
            }
            if select_received {
                break;
            }
        }

        assert!(select_received);
    }

    #[test]
    fn should_request_manager_results() {
        let gpu = &get_gpu_context_test();

        let gpu_texture =
            GpuTextureBuilder::from_static(&static_textures::LIGHTBULB_STATIC_TEXTURE).build(gpu);

        let texture = gpu_texture.inner;

        let mut req_mgr = ReadbackManager::default();

        req_mgr.request_selection(gpu, &texture, (0, 0), (10, 8));

        req_mgr.request_pick(&gpu, &texture, (15, 15));

        let mut pick_received = false;
        let mut selection_received = false;

        loop {
            GpuReadback::poll(gpu.device);

            if let Some(result) = req_mgr.poll_results() {
                match result {
                    QueryResult::Pick(_) => {
                        pick_received = true;
                    }

                    QueryResult::Selection(_) => {
                        selection_received = true;
                    }
                }
            }

            if pick_received && selection_received {
                break;
            }

            std::thread::yield_now();
        }

        assert!(pick_received);
        assert!(selection_received);
    }
}
