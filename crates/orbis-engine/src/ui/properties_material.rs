use imgui::{Drag, Image, TreeNodeFlags, Ui};

use crate::{
    assets::material_desc::{MaterialDesc, MaterialTextureSlot},
    editor::{MaterialCommand, MaterialDto},
    ui::UiContext,
};

pub fn draw(ui: &Ui, ctx: &mut UiContext, materials: &mut [MaterialDto]) {
    if !ui.collapsing_header("Materials", TreeNodeFlags::DEFAULT_OPEN) {
        return;
    }

    for material in materials {
        let id = ui.push_id(material.id.raw().to_string());
        if ui.collapsing_header(&material.name, TreeNodeFlags::DEFAULT_OPEN) {
            *ctx.material_preview = Some(material.id);
            draw_material(ui, ctx, material);
        }
        id.pop();
    }
}

fn draw_material(ui: &Ui, ctx: &mut UiContext, material: &mut MaterialDto) {
    ui.text(&material.name);
    ui.same_line();
    ui.text_disabled(format!("#{}", material.id.raw()));
    ui.separator();

    draw_material_preview(ui, ctx, material);

    let mut changed = false;

    ui.child_window("MaterialTabsContent")
        .size([0.0, 200.0])
        .build(|| {
            let Some(_tab_bar) = ui.tab_bar("MaterialTabs") else {
                return;
            };

            if let Some(_tab) = ui.tab_item("Surface") {
                changed |= draw_surface(ui, &mut material.desc);
            }

            if let Some(_tab) = ui.tab_item("Textures") {
                changed |= draw_textures(ui, ctx, &mut material.desc);
            }

            if material.desc.transmission.is_some()
                && let Some(_tab) = ui.tab_item("Transmission")
            {
                changed |= draw_transmission(ui, &mut material.desc);
            }

            if material.desc.volume.is_some()
                && let Some(_tab) = ui.tab_item("Volume")
            {
                changed |= draw_volume(ui, &mut material.desc);
            }

            if material.desc.sheen.is_some()
                && let Some(_tab) = ui.tab_item("Sheen")
            {
                changed |= draw_sheen(ui, &mut material.desc);
            }
        });

    if changed {
        send_material_update(ctx, material);
    }
}
fn draw_material_preview(ui: &Ui, ctx: &UiContext, material: &MaterialDto) {
    let preview = material
        .desc
        .texture(MaterialTextureSlot::BaseColor)
        .and_then(|texture| ctx.textures.asset(texture));

    ui.text("Preview");
    if let Some(texture) = ctx.textures.material_preview {
        Image::new(texture, [180.0, 180.0]).build(ui);
    } else if let Some(texture) = preview {
        Image::new(texture, [180.0, 180.0]).build(ui);
    } else {
        ui.text_disabled("PBR preview render target not available");
    }
}

fn draw_surface(ui: &Ui, material: &mut MaterialDesc) -> bool {
    let mut changed = false;

    let mut color: [f32; 4] = material.base_color_factor.into();
    if ui
        .color_edit4_config("Base Color", &mut color)
        .inputs(false)
        .build()
    {
        material.base_color_factor = color.into();
        changed = true;
    }

    changed |= Drag::new("Metallic")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut material.metallic_factor);
    changed |= Drag::new("Roughness")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut material.roughness_factor);
    changed |= Drag::new("Normal Scale")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut material.normal_scale);
    changed |= Drag::new("Occlusion")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut material.occlusion_strength);

    let mut emissive: [f32; 4] = material.emissive_factor.into();
    if ui
        .color_edit4_config("Emission", &mut emissive)
        .inputs(false)
        .build()
    {
        material.emissive_factor = emissive.into();
        changed = true;
    }

    draw_alpha(ui, material);

    changed
}

fn draw_alpha(ui: &Ui, material: &MaterialDesc) {
    use crate::assets::material_desc::AlphaMode;

    ui.text("Material Mode:");
    ui.same_line();

    let mut opaque = matches!(material.alpha_mode, AlphaMode::Opaque);
    let mut mask = matches!(material.alpha_mode, AlphaMode::Mask { .. });
    let mut blend = matches!(material.alpha_mode, AlphaMode::Blend);

    ui.checkbox("Opaque", &mut opaque);
    ui.same_line();
    ui.checkbox("Mask", &mut mask);
    ui.same_line();
    ui.checkbox("Blend", &mut blend);
}

fn draw_textures(ui: &Ui, ctx: &UiContext, material: &mut MaterialDesc) -> bool {
    let mut changed = false;

    for slot in MaterialTextureSlot::ALL {
        changed |= draw_texture_slot(ui, ctx, material, slot);
    }

    changed
}

fn draw_texture_slot(
    ui: &Ui,
    ctx: &UiContext,
    material: &mut MaterialDesc,
    slot: MaterialTextureSlot,
) -> bool {
    let _id = ui.push_id(slot.as_str());
    ui.separator();
    ui.text(slot.as_str());

    if material.texture(slot).is_none() {
        ui.text_disabled("No texture assigned");
        return false;
    }

    let mut changed = false;
    let mut enabled = material.slot_get(slot);
    if ui.checkbox("Enabled", &mut enabled) {
        material.slot_set(slot, enabled);
        changed = true;
    }

    if enabled {
        draw_texture_preview(ui, ctx, material, slot);
        changed |= draw_texture_transform(ui, material, slot);
    }

    changed
}

fn draw_texture_transform(ui: &Ui, material: &mut MaterialDesc, slot: MaterialTextureSlot) -> bool {
    let Some(transform) = material.uvtransform_mut(slot) else {
        return false;
    };

    if !ui.collapsing_header("UV Transform", TreeNodeFlags::empty()) {
        return false;
    }

    let mut changed = false;
    changed |= Drag::new("Offset")
        .speed(0.01)
        .build_array(ui, &mut transform.offset);
    changed |= Drag::new("Rotation")
        .speed(0.01)
        .build(ui, &mut transform.rotation);
    changed |= Drag::new("Scale")
        .speed(0.01)
        .build_array(ui, &mut transform.scale);
    changed
}

fn draw_transmission(ui: &Ui, material: &mut MaterialDesc) -> bool {
    let Some(transmission) = material.transmission.as_mut() else {
        ui.text_disabled("Transmission not enabled");
        return false;
    };

    let mut changed = Drag::new("Weight")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut transmission.factor);

    changed |= Drag::new("IOR")
        .speed(0.01)
        .range(1.0, 2.5)
        .build(ui, &mut material.ior);

    changed
}

fn draw_volume(ui: &Ui, material: &mut MaterialDesc) -> bool {
    let Some(volume) = material.volume.as_mut() else {
        ui.text_disabled("Volume not enabled");
        return false;
    };

    let mut changed = false;
    changed |= Drag::new("Thickness")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut volume.thickness_factor);
    changed |= Drag::new("Distance")
        .speed(0.01)
        .range(0.01, 100.0)
        .build(ui, &mut volume.attenuation_distance);
    changed |= ui.color_edit3("Attenuation", &mut volume.attenuation_color);
    changed
}

fn draw_sheen(ui: &Ui, material: &mut MaterialDesc) -> bool {
    let Some(sheen) = material.sheen.as_mut() else {
        ui.text_disabled("Sheen not enabled");
        return false;
    };

    let mut changed = ui.color_edit3("Color", &mut sheen.color_factor);
    changed |= Drag::new("Roughness")
        .speed(0.01)
        .range(0.0, 1.0)
        .build(ui, &mut sheen.roughness_factor);
    changed
}

fn send_material_update(ctx: &UiContext, material: &MaterialDto) {
    ctx.commands.send(MaterialCommand::Update {
        id: material.id,
        desc: material.desc.clone(),
    });
}

fn draw_texture_preview(
    ui: &Ui,
    ctx: &UiContext,
    material: &MaterialDesc,
    slot: MaterialTextureSlot,
) {
    let Some(texture) = material.texture(slot) else {
        return;
    };

    if let Some(texture_id) = ctx.textures.asset(texture) {
        Image::new(texture_id, [64.0, 64.0]).build(ui);
        ui.same_line();
        ui.text(format!("Texture #{}", texture.raw()));
    } else {
        ui.text_disabled(format!("Texture #{} not available", texture.raw()));
    }
}
