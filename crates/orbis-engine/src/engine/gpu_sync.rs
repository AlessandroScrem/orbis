use crate::app::domain::events::DomainEvent::Selection;
use crate::app::domain::events::SelectionEvent::SelectIbl;
use crate::assets::asset_manager::{AssetEventKind, AssetManager};
use crate::assets::material_asset::MaterialAsset;
use crate::assets::mesh_asset::MeshAsset;
use crate::assets::texture_asset::{TextureAsset, TextureDesc};
use crate::assets::texture_upload::{TextureData, load_and_decode};
use crate::assets::{IblAsset, IblId, TextureId};
use crate::engine::RuntimeEvent;
use crate::engine::engine::EventBus;
use crate::gpu::texture::GpuTextureBuilder;
use crate::gpu::{
    BindgroupLayoutKind, GpuCache, GpuContext, GpuManager, GpuMaterial, GpuMaterialCache, GpuMesh,
    GpuTextureCache, IblManager,
};
use crate::renderer::{ImguiRender, MaterialPreviewRenderer};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

type DecodedTexture = (TextureId, u64, Result<TextureData, ()>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextureLoadState {
    Pending,
    Ready,
    Failed,
}

pub(crate) struct TextureLoadService {
    sender: Sender<Vec<DecodedTexture>>,
    receiver: Receiver<Vec<DecodedTexture>>,
    generations: HashMap<TextureId, u64>,
    states: HashMap<TextureId, TextureLoadState>,
    next_generation: u64,
}

impl TextureLoadService {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            generations: HashMap::new(),
            states: HashMap::new(),
            next_generation: 0,
        }
    }

    fn request(&mut self, jobs: Vec<(TextureId, TextureDesc)>) {
        let jobs = jobs
            .into_iter()
            .map(|(id, desc)| {
                self.next_generation = self.next_generation.wrapping_add(1);
                let generation = self.next_generation;
                self.generations.insert(id, generation);
                self.states.insert(id, TextureLoadState::Pending);
                (id, generation, desc)
            })
            .collect::<Vec<_>>();

        if jobs.is_empty() {
            return;
        }

        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let decoded = jobs
                .into_par_iter()
                .map(|(id, generation, desc)| (id, generation, load_and_decode(desc).ok_or(())))
                .collect();
            let _ = sender.send(decoded);
        });
    }

    fn invalidate(&mut self, id: TextureId) {
        self.generations.remove(&id);
        self.states.insert(id, TextureLoadState::Failed);
    }

    pub(crate) fn is_material_ready(
        &self,
        desc: &crate::assets::material_desc::MaterialDesc,
    ) -> bool {
        desc.get_textures().iter().all(|id| {
            matches!(
                self.states.get(id),
                Some(TextureLoadState::Ready | TextureLoadState::Failed)
            )
        })
    }

    fn poll(&mut self) -> Vec<(TextureId, Result<TextureData, ()>)> {
        let mut ready = Vec::new();
        while let Ok(batch) = self.receiver.try_recv() {
            for (id, generation, result) in batch {
                if self.generations.get(&id) != Some(&generation) {
                    continue;
                }
                self.states.insert(
                    id,
                    if result.is_ok() {
                        TextureLoadState::Ready
                    } else {
                        TextureLoadState::Failed
                    },
                );
                ready.push((id, result));
            }
        }
        ready
    }
}

pub(crate) fn sync_gpu_assets(
    asset_mgr: &mut AssetManager,
    bus: &mut EventBus,
    gpu_context: &GpuContext,
    gpu_cache: &mut GpuCache,
    texture_loader: &mut TextureLoadService,
    gpu_manager: &mut GpuManager,
    ibl_manager: &mut IblManager,
    imgui_render: &mut ImguiRender,
    hdr_vec: &mut Vec<(TextureId, IblId)>,
    material_preview_renderer: &mut MaterialPreviewRenderer,
) {
    let texture_cache = &mut gpu_cache.textures;
    let material_cache = &mut gpu_cache.material;
    let mesh_cache = &mut gpu_cache.mesh;
    let grouped = asset_mgr.drain_grouped_events();
    let mut removed_ibl_from_texture = false;
    grouped.process_type::<TextureAsset, _>(|kind, events| match kind {
        AssetEventKind::Created | AssetEventKind::Updated => {
            let jobs: Vec<(TextureId, TextureDesc)> = events
                .iter()
                .filter_map(|ev| {
                    asset_mgr
                        .get::<TextureAsset>(ev.id)
                        .map(|a| (ev.id, a.desc.clone()))
                })
                .collect();
            // Do not expose an old GPU version while a replacement is loading.
            // If decoding fails, the material will be rebuilt against the white fallback.
            for (id, _) in &jobs {
                remove_gpu_texture(texture_cache, imgui_render, *id);
                rebuild_materials_using_texture(
                    asset_mgr,
                    texture_cache,
                    material_cache,
                    gpu_context,
                    gpu_manager,
                    *id,
                );
            }
            texture_loader.request(jobs);
        }
        AssetEventKind::Removed => events.iter().for_each(|ev| {
            texture_loader.invalidate(ev.id);
            remove_gpu_texture(texture_cache, imgui_render, ev.id);
            let removed_ibl = hdr_vec
                .iter()
                .filter(|(hdr_id, _)| *hdr_id == ev.id)
                .map(|(_, ibl_id)| *ibl_id)
                .collect::<Vec<_>>();
            for ibl_id in &removed_ibl {
                ibl_manager.remove(*ibl_id);
                hdr_vec.retain(|(_, existing_id)| existing_id != ibl_id);
            }
            removed_ibl_from_texture |= !removed_ibl.is_empty();

            rebuild_materials_using_texture(
                asset_mgr,
                texture_cache,
                material_cache,
                gpu_context,
                gpu_manager,
                ev.id,
            );
        }),
        _ => {}
    });
    let mut changed_textures = HashSet::new();
    for (id, result) in texture_loader.poll() {
        if let Ok(data) = result {
            let texture = GpuTextureBuilder::from_cpu(data).build(&gpu_context.as_ref());
            register_gpu_texture(
                texture_cache,
                imgui_render,
                &gpu_context.device,
                id,
                texture,
            );
        }
        // Failed loads are terminal too: materials resolve the missing slot to
        // the built-in white fallback and can be published without waiting forever.
        changed_textures.insert(id);

        let dependent_materials = asset_mgr
            .iter::<MaterialAsset>()
            .filter(|(_, material)| material.desc.get_textures().contains(&id))
            .map(|(material_id, _)| material_id)
            .collect::<Vec<_>>();
        for material_id in dependent_materials {
            if let Some(material) = asset_mgr.get::<MaterialAsset>(material_id) {
                let layout = gpu_manager.get_bindgroup_layout(BindgroupLayoutKind::Material);
                material_cache.insert(
                    material_id,
                    GpuMaterial::new(texture_cache, &material.desc, &gpu_context.device, layout),
                );
            }
        }
    }

    let mut removed_ibl_asset = false;
    grouped.process_type::<IblAsset, _>(|kind, events| {
        if let AssetEventKind::Removed = kind {
            for event in events {
                ibl_manager.remove(event.id);
                hdr_vec.retain(|(_, ibl_id)| *ibl_id != event.id);
                removed_ibl_asset = true;
            }
        }
    });
    if removed_ibl_asset || removed_ibl_from_texture {
        select_fallback_ibl(hdr_vec, bus);
    }

    // Create or refresh environments once their HDR texture is available.
    for (id, asset) in asset_mgr.iter::<IblAsset>() {
        let current_hdr = hdr_vec
            .iter()
            .find(|(_, ibl_id)| *ibl_id == id)
            .map(|(hdr_id, _)| *hdr_id);
        let needs_refresh =
            current_hdr != Some(asset.hrd_id) || changed_textures.contains(&asset.hrd_id);
        if !needs_refresh {
            continue;
        }

        let Some(hdr) = texture_cache.get(asset.hrd_id) else {
            continue;
        };
        let gpu_ibl = ibl_manager.create(hdr, &gpu_context.as_ref());
        ibl_manager.insert(id, gpu_ibl);
        hdr_vec.retain(|(_, ibl_id)| *ibl_id != id);
        hdr_vec.push((asset.hrd_id, id));
        material_preview_renderer.invalidate_environment();
        bus.send_domain(Selection(SelectIbl(id)));
        bus.send_runtime(RuntimeEvent::UpdateIblMaps(id));
    }
    grouped.process_type::<MaterialAsset, _>(|kind, events| match kind {
        AssetEventKind::Created | AssetEventKind::Updated => events
            .iter()
            .filter_map(|ev| asset_mgr.get::<MaterialAsset>(ev.id).map(|a| (ev.id, a)))
            .for_each(|(id, asset)| {
                let layout = gpu_manager.get_bindgroup_layout(BindgroupLayoutKind::Material);
                material_cache.insert(
                    id,
                    GpuMaterial::new(texture_cache, &asset.desc, &gpu_context.device, layout),
                );
            }),
        AssetEventKind::Removed => events.iter().for_each(|ev| material_cache.remove(ev.id)),
        _ => {}
    });
    grouped.process_type::<MeshAsset, _>(|kind, events| match kind {
        AssetEventKind::Created | AssetEventKind::Updated => events
            .iter()
            .filter_map(|ev| asset_mgr.get::<MeshAsset>(ev.id).map(|a| (ev.id, a)))
            .for_each(|(id, asset)| {
                mesh_cache.insert(
                    id,
                    GpuMesh::new(
                        &gpu_context.device,
                        &asset.desc.vertices,
                        &asset.desc.indices,
                    ),
                )
            }),
        AssetEventKind::Removed => events.iter().for_each(|ev| mesh_cache.remove(ev.id)),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::{DecodedTexture, TextureLoadService};
    use crate::assets::TextureId;
    use crate::assets::material_desc::{MaterialDesc, MaterialTextureSlot};
    use crate::assets::texture_asset::ColorSpace;

    #[test]
    fn material_waits_for_textures_and_accepts_failed_texture_fallback() {
        let mut loader = TextureLoadService::new();
        let id = TextureId::new();
        let mut material = MaterialDesc::default();
        material.set_texture(Some(id), MaterialTextureSlot::BaseColor, 0, None);

        assert!(!loader.is_material_ready(&material));

        loader.states.insert(id, super::TextureLoadState::Pending);
        assert!(!loader.is_material_ready(&material));

        loader.states.insert(id, super::TextureLoadState::Ready);
        assert!(loader.is_material_ready(&material));

        loader.states.insert(id, super::TextureLoadState::Failed);
        assert!(loader.is_material_ready(&material));
    }

    #[test]
    fn invalidated_texture_load_results_are_discarded() {
        let mut loader = TextureLoadService::new();
        let id = TextureId::new();
        loader.generations.insert(id, 7);
        loader.states.insert(id, super::TextureLoadState::Pending);

        let data = crate::assets::texture_upload::TextureData {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
            format: ColorSpace::Rgba8,
        };
        loader.sender.send(vec![(id, 7, Ok(data))]).unwrap();

        loader.invalidate(id);

        assert!(loader.poll().is_empty());
        assert_eq!(
            loader.states.get(&id),
            Some(&super::TextureLoadState::Failed)
        );
        assert!(!loader.generations.contains_key(&id));
    }

    #[test]
    fn stale_texture_load_results_are_discarded() {
        let mut loader = TextureLoadService::new();
        let id = TextureId::new();
        loader.generations.insert(id, 2);
        let data = crate::assets::texture_upload::TextureData {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
            format: ColorSpace::Rgba8,
        };

        let stale: Vec<DecodedTexture> = vec![(id, 1, Ok(data.clone()))];
        loader.sender.send(stale).unwrap();
        assert!(loader.poll().is_empty());

        let current: Vec<DecodedTexture> = vec![(id, 2, Ok(data))];
        loader.sender.send(current).unwrap();
        assert_eq!(loader.poll().len(), 1);
        assert_eq!(
            loader.states.get(&id),
            Some(&super::TextureLoadState::Ready)
        );

        loader.sender.send(vec![(id, 2, Err(()))]).unwrap();
        assert_eq!(loader.poll().len(), 1);
        assert_eq!(
            loader.states.get(&id),
            Some(&super::TextureLoadState::Failed)
        );
    }
}

fn select_fallback_ibl(hdr_vec: &[(TextureId, IblId)], bus: &mut EventBus) {
    if let Some((_, id)) = hdr_vec.first() {
        bus.send_domain(Selection(SelectIbl(*id)));
        bus.send_runtime(RuntimeEvent::UpdateIblMaps(*id));
    } else {
        bus.send_runtime(RuntimeEvent::ClearIblMaps);
    }
}

fn rebuild_materials_using_texture(
    asset_mgr: &AssetManager,
    texture_cache: &GpuTextureCache,
    material_cache: &mut GpuMaterialCache,
    gpu_context: &GpuContext,
    gpu_manager: &GpuManager,
    texture_id: TextureId,
) {
    let dependent_materials = asset_mgr
        .iter::<MaterialAsset>()
        .filter(|(_, material)| material.desc.get_textures().contains(&texture_id))
        .map(|(material_id, _)| material_id)
        .collect::<Vec<_>>();

    let layout = gpu_manager.get_bindgroup_layout(BindgroupLayoutKind::Material);
    for material_id in dependent_materials {
        if let Some(material) = asset_mgr.get::<MaterialAsset>(material_id) {
            material_cache.insert(
                material_id,
                GpuMaterial::new(
                    texture_cache,
                    &material.desc,
                    &gpu_context.device,
                    layout,
                ),
            );
        }
    }
}

fn register_gpu_texture(
    cache: &mut GpuTextureCache,
    imgui: &mut ImguiRender,
    device: &wgpu::Device,
    id: TextureId,
    texture: crate::gpu::texture::GpuTexture,
) {
    cache.insert(id, texture);
    let texture = cache
        .get(id)
        .expect("registered GPU texture must be available");
    imgui.registry.add(&mut imgui.renderer, device, id, texture);
}

fn remove_gpu_texture(cache: &mut GpuTextureCache, imgui: &mut ImguiRender, id: TextureId) {
    cache.remove(id);
    imgui.registry.remove(&mut imgui.renderer, id);
}
