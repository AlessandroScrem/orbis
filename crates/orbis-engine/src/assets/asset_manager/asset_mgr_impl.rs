use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use super::asset_id::AssetHandle;
use super::asset_storage::{Asset, AssetStorage};
use super::dependency_graph::*;
use super::resource_stats::ResourceStats;
use crate::ResourceId;

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub enum AssetEventKind {
    Created,
    Updated,
    Removed,
    #[allow(unused)]
    DependencyAdded,
    #[allow(unused)]
    DependencyRemoved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetEvent {
    pub id: ResourceId,
    pub type_id: TypeId,
    pub kind: AssetEventKind,
}

trait KeyMap {
    fn insert(&mut self, key: &dyn Any, id: ResourceId);
    fn get(&self, key: &dyn Any) -> Option<ResourceId>;
    fn remove(&mut self, id: ResourceId);
}

struct TypedKeyMap<K: Eq + Hash + Clone> {
    map: HashMap<K, ResourceId>,
}

impl<K: Eq + Hash + Clone + 'static> KeyMap for TypedKeyMap<K> {
    fn insert(&mut self, key: &dyn Any, id: ResourceId) {
        let key = key.downcast_ref::<K>().unwrap();
        self.map.insert(key.clone(), id);
    }

    fn get(&self, key: &dyn Any) -> Option<ResourceId> {
        let key = key.downcast_ref::<K>()?;
        self.map.get(key).copied()
    }

    fn remove(&mut self, id: ResourceId) {
        self.map.retain(|_, v| *v != id);
    }
}

#[derive(Default)]
pub struct KeyRegistry {
    inner: HashMap<TypeId, Box<dyn KeyMap>>,
}
impl KeyRegistry {
    pub fn get<T: Asset>(&self, key: &T::Key) -> Option<ResourceId> {
        let type_id = TypeId::of::<T>();

        self.inner.get(&type_id)?.get(key)
    }

    pub fn insert<T: Asset>(&mut self, key: T::Key, id: ResourceId) {
        let type_id = TypeId::of::<T>();

        let entry = self.inner.entry(type_id).or_insert_with(|| {
            Box::new(TypedKeyMap::<T::Key> {
                map: HashMap::new(),
            })
        });

        entry.insert(&key, id);
    }

    pub fn remove(&mut self, id: ResourceId) {
        for map in self.inner.values_mut() {
            map.remove(id);
        }
    }
}

trait ErasedStorage {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn remove_by_id(&mut self, id: ResourceId) -> usize;
}

struct TypedStorage<T: Asset> {
    inner: AssetStorage<T>,
}

impl<T: Asset> ErasedStorage for TypedStorage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove_by_id(&mut self, id: ResourceId) -> usize {
        self.inner.remove_by_id(id)
    }
}
#[derive(Default)]
pub struct AssetManager {
    storages: HashMap<TypeId, Box<dyn ErasedStorage>>,
    stats: HashMap<TypeId, ResourceStats>,

    key_index: KeyRegistry,

    ref_count: HashMap<ResourceId, u32>,
    resource_types: HashMap<ResourceId, TypeId>,

    graph: DependencyGraph,

    events: VecDeque<AssetEvent>,
}

impl AssetManager {
    #[allow(unused)]
    pub fn new() -> AssetManager {
        AssetManager::default()
    }

    fn storage_mut<T: Asset>(&mut self) -> &mut AssetStorage<T> {
        let id = TypeId::of::<T>();

        self.storages.entry(id).or_insert_with(|| {
            Box::new(TypedStorage::<T> {
                inner: AssetStorage::<T>::default(),
            })
        });

        &mut self
            .storages
            .get_mut(&id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<TypedStorage<T>>()
            .unwrap()
            .inner
    }

    fn stats_mut<T: Asset>(&mut self) -> &mut ResourceStats {
        let id = TypeId::of::<T>();
        self.stats.entry(id).or_default()
    }
}

impl AssetManager {
    pub fn add<T: Asset>(&mut self, asset: T) -> ResourceId {
        let key = asset.key().clone();

        let deps = asset.dependencies();

        if let Some(existing) = self.key_index.get::<T>(&key) {
            self.retain(existing);
            return existing;
        }

        // insert ResourceSize
        let size = asset.estimated_size();
        self.stats_mut::<T>().add(size);

        let id = ResourceId::new();
        let handle = self.storage_mut::<T>().insert(id, asset);

        let id = handle.id();

        self.ref_count.insert(id, 0);
        self.resource_types.insert(id, TypeId::of::<T>());

        self.key_index.insert::<T>(key, id);

        for dep in deps {
            self.graph.add(id, dep);

            self.retain(dep);
        }

        self.events.push_back(AssetEvent {
            id,
            type_id: TypeId::of::<T>(),
            kind: AssetEventKind::Created,
        });

        id
    }

    pub fn iter<T: Asset>(&self) -> impl Iterator<Item = (ResourceId, &T)> + '_ {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any().downcast_ref::<TypedStorage<T>>())
            .into_iter()
            .flat_map(|storage| storage.inner.iter())
    }

    pub fn get<T: Asset>(&self, id: ResourceId) -> Option<&T> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any().downcast_ref::<TypedStorage<T>>())
            .and_then(|storage| storage.inner.get_by_id(id))
    }

    pub fn update<T: Asset>(&mut self, id: ResourceId, f: impl FnOnce(&mut T)) {
        let Some((previous_size, previous_dependencies, previous_key)) =
            self.get::<T>(id).map(|asset| {
                (
                    asset.estimated_size(),
                    asset.dependencies(),
                    asset.key().clone(),
                )
            })
        else {
            return;
        };

        let handle = AssetHandle::<T>::new(id);
        let (updated_size, updated_dependencies, updated_key) = {
            let Some(existing) = self.storage_mut::<T>().get_mut(handle) else {
                return;
            };
            f(existing);
            (
                existing.estimated_size(),
                existing.dependencies(),
                existing.key().clone(),
            )
        };

        if previous_key != updated_key {
            self.key_index.remove(id);
            match self.key_index.get::<T>(&updated_key) {
                Some(existing_id) if existing_id != id => {
                    log::warn!(
                        "Asset update produced a duplicate key for resource {}; key remains indexed to resource {}",
                        id.raw(),
                        existing_id.raw()
                    );
                }
                _ => self.key_index.insert::<T>(updated_key, id),
            }
        }

        if previous_size != updated_size {
            let stats = self.stats_mut::<T>();
            stats.estimated_bytes = stats
                .estimated_bytes
                .saturating_sub(previous_size)
                .saturating_add(updated_size);
        }

        if previous_dependencies != updated_dependencies {
            self.graph.replace_dependencies(id, &updated_dependencies);
            for dependency in &updated_dependencies {
                self.retain(*dependency);
            }
            for dependency in previous_dependencies {
                self.release(dependency);
            }
        }

        self.events.push_back(AssetEvent {
            id,
            type_id: TypeId::of::<T>(),
            kind: AssetEventKind::Updated,
        });
    }

    #[allow(dead_code)]
    pub fn get_stats<T: Asset>(&self) -> ResourceStats {
        self.stats
            .get(&TypeId::of::<T>())
            .map_or_else(ResourceStats::default, Clone::clone)
    }
}

impl AssetManager {
    pub fn retain(&mut self, id: ResourceId) {
        *self.ref_count.entry(id).or_insert(0) += 1;
    }

    pub fn release(&mut self, id: ResourceId) {
        let should_destroy = match self.ref_count.get_mut(&id) {
            Some(count) => {
                *count = count.saturating_sub(1);
                *count == 0
            }
            None => false,
        };

        if should_destroy {
            self.remove_recursive(id);
        }
    }
}

impl AssetManager {
    pub fn remove(&mut self, id: ResourceId) {
        self.release(id);
    }

    fn remove_from_storage(&mut self, id: ResourceId) {
        let Some(type_id) = self.resource_types.get(&id).copied() else {
            return;
        };

        if let Some(storage) = self.storages.get_mut(&type_id) {
            let size = storage.remove_by_id(id);
            self.stats.get_mut(&type_id).map(|stat| stat.remove(size));
        }
    }

    fn remove_recursive(&mut self, id: ResourceId) {
        // 1. prendi dipendenze PRIMA di rimuovere
        let dependencies = self.graph.dependencies_of(id);
        let type_id = self.resource_types.get(&id).copied();

        // 2. rimuovi dal grafo
        self.graph.remove_asset(id);

        // 3. rimuovi da storage
        self.remove_from_storage(id);

        // 4. rimuovi key + ref
        self.key_index.remove(id);
        self.ref_count.remove(&id);
        self.resource_types.remove(&id);

        // 5. evento
        self.events.push_back(AssetEvent {
            id,
            type_id: type_id.unwrap_or(TypeId::of::<()>()),
            kind: AssetEventKind::Removed,
        });

        // 6. rilascia dipendenze (IMPORTANTISSIMO ordine corretto)
        for dep in dependencies {
            self.release(dep);
        }
    }
}

#[derive(Default)]
pub struct GroupedEvents {
    inner: HashMap<(TypeId, AssetEventKind), Vec<AssetEvent>>,
}

impl GroupedEvents {
    pub fn process_type<T: 'static, F>(&self, mut f: F)
    where
        F: FnMut(AssetEventKind, &Vec<AssetEvent>),
    {
        const ORDER: [AssetEventKind; 3] = [
            AssetEventKind::Created,
            AssetEventKind::Updated,
            AssetEventKind::Removed,
        ];
        let type_id = TypeId::of::<T>();

        for kind in ORDER {
            if let Some(events) = self.inner.get(&(type_id, kind)) {
                f(kind, events);
            }
        }
    }
}

impl AssetManager {
    pub fn drain_grouped_events(&mut self) -> GroupedEvents {
        let mut grouped: HashMap<(TypeId, AssetEventKind), Vec<AssetEvent>> = HashMap::new();

        for event in self.events.drain(..) {
            grouped
                .entry((event.type_id, event.kind))
                .or_default()
                .push(event);
        }

        GroupedEvents { inner: grouped }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Texture {
        name: String,
    }

    impl Asset for Texture {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.name
        }
    }

    #[test]
    fn add_dedup_and_refcount() {
        let mut mgr = AssetManager::new();

        let a = Texture { name: "tex".into() };

        let id1 = mgr.add(a.clone());
        assert_eq!(mgr.ref_count.get(&id1), Some(&0));

        // evento created
        let ev = mgr.events.back().unwrap();
        assert_eq!(ev.id, id1);
        assert_eq!(ev.kind, AssetEventKind::Created);

        // dedup
        let id2 = mgr.add(a.clone());
        assert_eq!(id1, id2);

        // refcount incrementato
        assert_eq!(mgr.ref_count.get(&id1), Some(&1));

        // 1 solo evento create
        assert_eq!(mgr.events.len(), 1);
    }

    #[test]
    fn remove_basic() {
        let mut mgr = AssetManager::new();

        let tex = Texture { name: "tex".into() };

        let id = mgr.add(tex.clone());

        mgr.remove(id);

        // deve essere rimosso
        assert!(!mgr.ref_count.contains_key(&id));

        // evento remove
        let ev = mgr.events.back().unwrap();
        assert_eq!(ev.id, id);
        assert_eq!(ev.kind, AssetEventKind::Removed);
    }

    #[test]
    fn retain_release() {
        let mut mgr = AssetManager::new();

        let tex = Texture { name: "tex".into() };

        let id = mgr.add(tex);

        mgr.retain(id);
        assert_eq!(mgr.ref_count.get(&id), Some(&1));

        mgr.release(id);
        // ora rimosso
        assert!(!mgr.ref_count.contains_key(&id));
    }

    #[test]
    fn remove_idempotent() {
        let mut mgr = AssetManager::new();

        let tex = Texture { name: "tex".into() };

        let id = mgr.add(tex);

        mgr.remove(id);
        mgr.remove(id);
        mgr.remove(id);

        // nessun crash
        assert!(!mgr.ref_count.contains_key(&id));
    }

    #[test]
    fn removing_owner_releases_its_asset_dependencies() {
        #[derive(Clone)]
        struct Owner {
            key: String,
            dependency: ResourceId,
        }

        impl Asset for Owner {
            type Key = String;

            fn key(&self) -> &Self::Key {
                &self.key
            }

            fn dependencies(&self) -> Vec<ResourceId> {
                vec![self.dependency]
            }
        }

        let mut manager = AssetManager::new();
        let dependency = manager.add(Texture {
            name: "dependency".into(),
        });
        let owner = manager.add(Owner {
            key: "owner".into(),
            dependency,
        });

        assert!(manager.get::<Texture>(dependency).is_some());
        assert_eq!(manager.ref_count.get(&dependency), Some(&1));

        manager.remove(owner);

        assert!(manager.get::<Owner>(owner).is_none());
        assert!(manager.get::<Texture>(dependency).is_none());
    }

    #[test]
    fn updating_asset_dependencies_releases_old_and_retains_new() {
        #[derive(Clone)]
        struct Owner {
            key: String,
            dependency: ResourceId,
        }

        impl Asset for Owner {
            type Key = String;

            fn key(&self) -> &Self::Key {
                &self.key
            }

            fn dependencies(&self) -> Vec<ResourceId> {
                vec![self.dependency]
            }
        }

        let mut manager = AssetManager::new();
        let first = manager.add(Texture {
            name: "first".into(),
        });
        let second = manager.add(Texture {
            name: "second".into(),
        });
        let owner = manager.add(Owner {
            key: "owner".into(),
            dependency: first,
        });

        manager.update::<Owner>(owner, |asset| asset.dependency = second);

        assert!(manager.get::<Texture>(first).is_none());
        assert!(manager.get::<Texture>(second).is_some());
        assert_eq!(manager.ref_count.get(&second), Some(&1));
        assert_eq!(manager.graph.dependencies_of(owner), vec![second]);

        manager.remove(owner);
        assert!(manager.get::<Texture>(second).is_none());
    }

    #[test]
    fn same_texture_same_id() {
        use crate::assets::texture_asset::*;
        let mut mgr = AssetManager::new();

        let desc = TextureDesc::File {
            path: "albedo.png".into(),
            usage: TextureUsage::Albedo,
            sampler: SamplerDesc::default(),
            mipmaps: true,
        };

        let texture = TextureAsset { desc };

        let a = mgr.add(texture.clone());
        let b = mgr.add(texture);

        assert_eq!(a, b);
    }

    #[test]
    fn texture_created_event() {
        use crate::assets::texture_asset::*;
        use crate::assets::texture_upload::load_and_decode;
        use crate::gpu::{GpuTextureBuilder, GpuTextureCache};
        use crate::test_utils;

        const TEXTURE_PATH: &str = crate::asset_path!("core/white.png");

        let gpu = &test_utils::get_gpu_context_test();

        let mut mgr = AssetManager::new();

        let desc = TextureDesc::File {
            path: TEXTURE_PATH.into(),
            usage: TextureUsage::Albedo,
            sampler: SamplerDesc::default(),
            mipmaps: true,
        };

        let texture = TextureAsset { desc };

        let id = mgr.add(texture);

        assert_eq!(mgr.events.len(), 1);

        assert!(mgr.get::<TextureAsset>(id).is_some());

        let mut texture_cache = GpuTextureCache::new(gpu);

        let events: Vec<super::AssetEvent> = mgr.events.drain(..).collect();

        for ev in events {
            let asset = mgr.get::<TextureAsset>(ev.id).unwrap();
            let data = load_and_decode(asset.desc.clone()).unwrap();
            let texture = GpuTextureBuilder::from_cpu(data).build(gpu);

            texture_cache.insert(ev.id, texture);
        }

        assert!(mgr.events.is_empty());
        assert!(texture_cache.get(id).is_some())
    }

    #[test]
    fn material_created_event() {
        use crate::assets::material_desc::MaterialDesc;
        use crate::assets::*;
        use crate::gpu::GpuMaterial;
        use crate::gpu::{BindgroupLayoutCache, BindgroupLayoutKind, GpuTextureCache};
        use crate::test_utils;

        let gpu = &test_utils::get_gpu_context_test();
        let layout_cache = BindgroupLayoutCache::new(gpu.device);
        let bind_group_layout = layout_cache.get(BindgroupLayoutKind::Material);

        let texture_cache = GpuTextureCache::new(gpu);

        let mut mgr = AssetManager::new();

        let material = MaterialAsset {
            desc: MaterialDesc::default(),
            key: "material".into(),
        };

        let id = mgr.add(material);

        assert_eq!(mgr.events.len(), 1);

        let grouped = mgr.drain_grouped_events();

        let mut gpu_materials: HashMap<ResourceId, GpuMaterial> = Default::default();

        grouped.process_type::<MaterialAsset, _>(|_kind, events| {
            for ev in events {
                let asset = mgr.get::<MaterialAsset>(ev.id).unwrap();
                let gpu_material =
                    GpuMaterial::new(&texture_cache, &asset.desc, gpu.device, bind_group_layout);

                gpu_materials.insert(ev.id, gpu_material);
            }
        });

        assert!(mgr.events.is_empty());

        assert_eq!(gpu_materials.len(), 1);
        assert!(gpu_materials.contains_key(&id))
    }

    #[test]
    fn mesh_created_event() {
        use crate::assets::mesh_asset::*;
        use crate::gpu::GpuMesh;
        use crate::test_utils;

        let gpu = &test_utils::get_gpu_context_test();

        let mut mgr = AssetManager::new();

        const MESH_PATH: &str = crate::asset_path!("core/cube/cube.gltf");
        let mesh_source = MeshSource::File {
            path: MESH_PATH.into(),
            submesh_index: 0,
        };

        let mesh = MeshAsset {
            mesh_source,
            desc: MeshDesc::default(),
        };

        let id = mgr.add(mesh);

        assert_eq!(mgr.events.len(), 1);

        let grouped = mgr.drain_grouped_events();

        let mut gpu_meshes: HashMap<ResourceId, GpuMesh> = Default::default();

        grouped.process_type::<MeshAsset, _>(|_kind, events| {
            for ev in events {
                let asset = mgr.get::<MeshAsset>(ev.id).unwrap();
                let gpu_mesh = GpuMesh::new(gpu.device, &asset.desc.vertices, &asset.desc.indices);
                gpu_meshes.insert(ev.id, gpu_mesh);
            }
        });

        assert!(mgr.events.is_empty());

        assert_eq!(gpu_meshes.len(), 1);
        assert!(gpu_meshes.contains_key(&id))
    }
}

#[cfg(test)]
mod test_api {
    use super::*;
    /// -----------------------------
    /// MOCK ASSETS (USER SIDE)
    /// -----------------------------

    #[derive(Clone)]
    struct Texture {
        name: String,
    }

    #[derive(Clone)]
    struct Material {
        name: String,
        albedo: Option<ResourceId>,
        normal: Option<ResourceId>,
    }

    #[derive(Clone)]
    struct Mesh {
        name: String,
        material: Option<ResourceId>,
    }

    // -----------------------------
    // IMPLEMENT Asset TRAIT
    // -----------------------------

    impl Asset for Texture {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.name
        }
    }

    impl Asset for Material {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.name
        }

        fn dependencies(&self) -> Vec<ResourceId> {
            let mut deps = Vec::new();

            if let Some(a) = self.albedo {
                deps.push(a);
            }

            if let Some(n) = self.normal {
                deps.push(n);
            }

            deps
        }
    }

    impl Asset for Mesh {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.name
        }

        fn dependencies(&self) -> Vec<ResourceId> {
            let mut deps = Vec::new();

            if let Some(m) = self.material {
                deps.push(m);
            }

            deps
        }
    }

    #[test]
    fn mesh_removal_reduces_material_refcount() {
        let mut mgr = AssetManager::new();

        let mat = mgr.add(Material {
            name: "Mat".into(),
            albedo: None,
            normal: None,
        });

        let mesh1 = mgr.add(Mesh {
            name: "M1".into(),
            material: Some(mat),
        });

        let mesh2 = mgr.add(Mesh {
            name: "M2".into(),
            material: Some(mat),
        });

        assert_eq!(mgr.ref_count.get(&mat), Some(&2));

        mgr.remove(mesh1);
        // ✔ mesh1 tolto → materiale ancora vivo
        assert_eq!(mgr.ref_count.get(&mat), Some(&1));

        mgr.remove(mesh2);
        // ✔ mesh2 tolto → materiale ancora vivo
        assert!(!mgr.ref_count.contains_key(&mat));
    }

    #[test]
    fn material_removal_reduces_texture_refcount() {
        let mut mgr = AssetManager::new();

        let tex = mgr.add(Texture {
            name: "T.png".into(),
        });

        let mat1 = mgr.add(Material {
            name: "M1".into(),
            albedo: Some(tex),
            normal: None,
        });

        let mat2 = mgr.add(Material {
            name: "M2".into(),
            albedo: Some(tex),
            normal: None,
        });

        assert_eq!(mgr.ref_count.get(&tex), Some(&2));

        mgr.remove(mat1);
        // texture ancora viva
        assert_eq!(mgr.ref_count.get(&tex), Some(&1));

        mgr.remove(mat2);
        // texture distrutta
        assert!(!mgr.ref_count.contains_key(&tex));
    }

    #[test]
    fn retain_release_behavior() {
        let mut mgr = AssetManager::new();

        let tex = mgr.add(Texture {
            name: "Tex.png".into(),
        });

        // add crea già l'asset vivo (ref = 1 o 0 dipende da design, ma NON 0 stabile)
        assert!(mgr.ref_count.contains_key(&tex));

        // primo retain
        mgr.retain(tex);
        assert_eq!(mgr.ref_count.get(&tex), Some(&1));

        // release finale → DEVE essere rimosso
        mgr.release(tex);
        assert!(!mgr.ref_count.contains_key(&tex));
    }

    #[test]
    fn dedup_chain_mesh_material_texture() {
        let mut mgr = AssetManager::new();

        let tex = mgr.add(Texture {
            name: "T.png".into(),
        });

        let mat = mgr.add(Material {
            name: "M".into(),
            albedo: Some(tex),
            normal: None,
        });

        let mesh1 = mgr.add(Mesh {
            name: "A".into(),
            material: Some(mat),
        });

        let mesh2 = mgr.add(Mesh {
            name: "A".into(),
            material: Some(mat),
        });

        // mesh dedup
        assert_eq!(mesh1, mesh2);
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::{Asset, AssetEventKind, AssetManager, ResourceId};

    struct LifecycleAsset {
        key: String,
        value: u32,
    }

    impl Asset for LifecycleAsset {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.key
        }

        fn estimated_size(&self) -> usize {
            self.value as usize
        }
    }

    #[test]
    fn updating_asset_key_keeps_key_index_consistent() {
        let mut manager = AssetManager::new();
        let original_key = "original".to_owned();
        let updated_key = "updated".to_owned();
        let id = manager.add(LifecycleAsset {
            key: original_key.clone(),
            value: 1,
        });

        manager.update::<LifecycleAsset>(id, |asset| asset.key = updated_key.clone());

        assert_eq!(manager.key_index.get::<LifecycleAsset>(&original_key), None);
        assert_eq!(
            manager.key_index.get::<LifecycleAsset>(&updated_key),
            Some(id)
        );
    }

    #[test]
    fn missing_asset_type_returns_empty_results() {
        let mut manager = AssetManager::new();

        assert!(manager.get::<LifecycleAsset>(ResourceId::new()).is_none());
        assert_eq!(manager.iter::<LifecycleAsset>().count(), 0);

        manager.update::<LifecycleAsset>(ResourceId::new(), |_| unreachable!());

        assert_eq!(manager.iter::<LifecycleAsset>().count(), 0);
    }

    #[test]
    fn create_update_remove_and_stale_id_lifecycle() {
        let mut manager = AssetManager::new();
        let key = "lifecycle".to_owned();

        let original_id = manager.add(LifecycleAsset {
            key: key.clone(),
            value: 1,
        });
        assert_eq!(manager.get::<LifecycleAsset>(original_id).unwrap().value, 1);
        assert_eq!(manager.get_stats::<LifecycleAsset>().estimated_bytes, 1);
        assert_eq!(manager.get_stats::<LifecycleAsset>().count, 1);
        assert_eq!(manager.events.back().unwrap().kind, AssetEventKind::Created);

        manager.update::<LifecycleAsset>(original_id, |asset| asset.value = 2);
        assert_eq!(manager.get::<LifecycleAsset>(original_id).unwrap().value, 2);
        assert_eq!(manager.get_stats::<LifecycleAsset>().estimated_bytes, 2);
        assert_eq!(manager.get_stats::<LifecycleAsset>().count, 1);
        assert_eq!(manager.events.back().unwrap().kind, AssetEventKind::Updated);

        manager.remove(original_id);
        assert!(manager.get::<LifecycleAsset>(original_id).is_none());
        assert_eq!(manager.events.back().unwrap().kind, AssetEventKind::Removed);
        assert_eq!(manager.get_stats::<LifecycleAsset>().estimated_bytes, 0);
        assert_eq!(manager.get_stats::<LifecycleAsset>().count, 0);

        let event_count_after_remove = manager.events.len();
        manager.update::<LifecycleAsset>(original_id, |asset| asset.value = 3);
        assert!(manager.get::<LifecycleAsset>(original_id).is_none());
        assert_eq!(manager.events.len(), event_count_after_remove);

        let replacement_id = manager.add(LifecycleAsset { key, value: 4 });
        assert_ne!(replacement_id, original_id);
        assert!(manager.get::<LifecycleAsset>(original_id).is_none());
        assert_eq!(
            manager.get::<LifecycleAsset>(replacement_id).unwrap().value,
            4
        );
    }
}
