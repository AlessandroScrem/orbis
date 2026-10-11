use super::types::*;
use crate::ResourceId;
use crate::assets::asset_manager::AssetManager;
use crate::prelude::warn;

use legion::*;

pub(crate) fn disable_entity(entity: Entity, world: &mut legion::World, disable: bool) {
    // if not root node do nothing
    if !is_root(entity, world) {
        warn!("{:?} Not Root: Disable entity abort", entity);
        return;
    }

    if let Some(mut e) = world.entry(entity) {
        if disable {
            e.add_component(Hidden);
        } else {
            e.remove_component::<Hidden>();
        }
    }
}

pub(crate) fn is_hidden(world: &World, entity: Entity) -> bool {
    let Ok(entry) = world.entry_ref(entity) else {
        return false;
    };
    // check if has Hidden component
    if entry.get_component::<Hidden>().is_ok() {
        return true;
    }

    let Ok(hierarchy) = entry.get_component::<HierarchyComponent>() else {
        return false;
    };

    // recurse to parent
    if let Some(parent) = hierarchy.parent {
        return is_hidden(world, parent);
    }

    false
}

pub(crate) fn add_parent(entity: Entity, world: &mut legion::World) {
    // if not root node do nothing
    if !is_root(entity, world) {
        warn!("{:?} Not Root: Add Parent abort", entity);
        return;
    }

    // Add parent node and set entity as child
    let new_root = {
        world.push((
            TagComponent {
                name: "New Node".into(),
            },
            HierarchyComponent {
                parent: None,
                children: vec![entity],
            },
            GlobalModelComponent::default(),
            TransformComponent::default(),
        ))
    };

    // Register new node as root
    world.entry_mut(entity).ok().map(|mut e| {
        e.get_component_mut::<HierarchyComponent>()
            .map(|h| h.parent = Some(new_root))
    });
}

pub(crate) fn collect_hierarchy_root_entities(world: &legion::World) -> Vec<Entity> {
    use legion::query::IntoQuery;

    let mut query = <(Entity, &HierarchyComponent)>::query();
    query
        .iter(world)
        .filter(|(e, _)| is_root(**e, world))
        .map(|(e, _)| *e)
        .collect()
}

pub(crate) fn has_hierarchy(entity: Entity, world: &legion::World) -> bool {
    let Ok(entry) = world.entry_ref(entity) else {
        return false;
    };
    entry.get_component::<HierarchyComponent>().is_ok()
}

pub(crate) fn remove_entity(
    asset_mgr: &mut AssetManager,
    entity: Entity,
    world: &mut legion::World,
) {
    if has_hierarchy(entity, world) {
        remove_meshes_from_all(asset_mgr, entity, world);
    } else {
        // is light
        world.remove(entity);
    }
}

fn remove_meshes_from_all(asset_mgr: &mut AssetManager, entity: Entity, world: &mut legion::World) {
    let entities = collect_mesh_entities_from_root(entity, world);
    let mesh_ids = collect_mesh_asset_ids_from_entity(world, &entities);

    // remove entity from world
    for e in entities {
        world.remove(e);
    }

    // remove mesh from asset
    // asset will remove dependency : (material, textures)
    // TODO: check if mesh is shared by others before removing
    for mesh_id in mesh_ids {
        asset_mgr.remove(mesh_id);
    }
}

type ResourceIdCollectionCollection = Vec<ResourceId>;

fn collect_mesh_asset_ids_from_entity(
    world: &legion::World,
    entities: &[Entity],
) -> ResourceIdCollectionCollection {
    let mut ids = vec![];

    for e in entities {
        if let Ok(entry) = world.entry_ref(e.to_owned())
            && let Ok(mesh) = entry.get_component::<MeshComponent>()
        {
            ids.push(mesh.handle);
        }
    }
    ids
}

fn is_root(entity: Entity, world: &legion::World) -> bool {
    let Ok(entry) = world.entry_ref(entity) else {
        return false;
    };

    let Ok(hierarchy) = entry.get_component::<HierarchyComponent>() else {
        return false;
    };

    hierarchy.parent.is_none()
}

fn collect_mesh_entities_from_root(entity: Entity, world: &mut legion::World) -> Vec<Entity> {
    let mut entities = Vec::new();
    if !is_root(entity, world) {
        warn!("{:?} Not Root: Remove abort", entity);
        return entities;
    }

    fn collect_subtree(world: &legion::World, root: Entity, out: &mut Vec<Entity>) {
        out.push(root);

        if let Ok(entry) = world.entry_ref(root)
            && let Ok(h) = entry.get_component::<HierarchyComponent>()
        {
            for &child in &h.children {
                collect_subtree(world, child, out);
            }
        }
    }

    collect_subtree(world, entity, &mut entities);

    entities
}

#[cfg(test)]
mod tests {
    use super::is_hidden;
    use crate::ecs::components::{Hidden, HierarchyComponent};
    use legion::World;

    #[test]
    fn hidden_entity_is_hidden() {
        let mut world = World::default();
        let entity = world.push((HierarchyComponent::default(), Hidden));

        assert!(is_hidden(&world, entity));
    }

    #[test]
    fn child_of_hidden_entity_is_hidden() {
        let mut world = World::default();
        let parent = world.push((HierarchyComponent::default(), Hidden));
        let child = world.push((HierarchyComponent {
            parent: Some(parent),
            children: Vec::new(),
        },));

        assert!(is_hidden(&world, child));
    }
}
