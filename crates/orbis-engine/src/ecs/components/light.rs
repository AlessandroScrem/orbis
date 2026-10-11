use legion::*;

use crate::ecs::components::{
    GlobalModelComponent, Hidden, HierarchyComponent, TransformComponent,
};

use super::types::{LightComponent, TagComponent};
use legion::world::World;

const DEFAULT_LIGHT_NAME: &str = "Light";

fn unique_light_name(world: &World) -> String {
    let mut names = std::collections::HashSet::new();

    for (_, tag) in <(&LightComponent, &TagComponent)>::query().iter(world) {
        names.insert(tag.name.as_str());
    }

    if !names.contains(DEFAULT_LIGHT_NAME) {
        return DEFAULT_LIGHT_NAME.to_owned();
    }

    for index in 1.. {
        let name = format!("{DEFAULT_LIGHT_NAME} #{index:02}");
        if !names.contains(name.as_str()) {
            return name;
        }
    }

    unreachable!()
}

/// A function to help create a light entity.
pub fn new(world: &mut World) -> Entity {
    world.push((
        TagComponent {
            name: unique_light_name(world),
        },
        TransformComponent {
            position: [3.0, 20.0, 10.0],
            ..Default::default()
        },
        HierarchyComponent::default(),
        GlobalModelComponent::default(),
        LightComponent::default(),
    ))
}

/// A function to help create a light entity.
pub fn add(
    world: &mut World,
    light: LightComponent,
    name: TagComponent,
    transform: TransformComponent,
) -> Entity {
    world.push((
        name,
        transform,
        HierarchyComponent::default(),
        GlobalModelComponent::default(),
        light,
    ))
}

pub fn enable_all_lights(enable: bool, world: &mut legion::World) {
    use legion::query::IntoQuery;

    let mut query = <(Entity, &LightComponent)>::query();
    let entities: Vec<Entity> = query.iter(world).map(|(entity, _)| *entity).collect();

    for entity in entities {
        if let Some(mut e) = world.entry(entity) {
            if enable {
                e.remove_component::<Hidden>();
            } else {
                e.add_component(Hidden);
            }
        }
    }
}
