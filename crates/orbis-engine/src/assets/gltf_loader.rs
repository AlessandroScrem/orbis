use gltf::{
    Document,
    mesh::{Mode, Reader},
};
use std::collections::hash_map::HashMap;
use std::{path::Path, time::Instant};

use crate::error::ImportError;
use crate::math::*;
use crate::prelude::*;
use crate::{BoundingBox, ecs::components::TransformComponent};

use crate::ResourceId;
use crate::assets::asset_manager::AssetManager;
use crate::assets::material_asset::*;
use crate::assets::material_desc;
use crate::assets::mesh_asset::*;
use crate::assets::texture_asset::*;
use crate::assets::vertexdata::MeshVertexData;

pub struct GltfGroup {
    pub meshes: Vec<ResourceId>,
    pub nodes: Vec<NodeData>,
    pub name: String,
}

#[derive(Clone)]
pub struct NodeData {
    pub name: String,
    pub local_transform: TransformComponent,
    pub mesh: Option<usize>,  // index in meshes
    pub children: Vec<usize>, // index in nodes
}

// public wrapper manage error messages
pub fn load_gltf<P: AsRef<Path>>(path: P, asset_mgr: &mut AssetManager) -> Option<GltfGroup> {
    match load_gltf_internal(&path, asset_mgr) {
        Ok(scene) => Some(scene),
        Err(e) => {
            warn!("Failed to load glTF {}: {}", path.as_ref().display(), e);
            None
        }
    }
}

// step for load gltf
//     (LoadedScene)
//          |
//          Y
//   create Materialid
//          |
//          Y
//   crate Meshid
//          |
//          Y
//   spawn_scene ECS
fn load_gltf_internal<P: AsRef<Path>>(
    path: P,
    asset_mgr: &mut AssetManager,
) -> Result<GltfGroup, ImportError> {
    let timer = Instant::now();
    let (gltf, buffers, _) = gltf::import(path.as_ref())?;

    info!(
        "Import gltf {:?} ,took: {:?}",
        path.as_ref().display(),
        timer.elapsed()
    );

    let mut meshes = Vec::new();
    let material_map = create_materials(&gltf, &path, asset_mgr);

    for g_mesh in gltf.meshes() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut submeshes = Vec::new();
        for primitive in g_mesh.primitives() {
            assert_eq!(primitive.mode(), Mode::Triangles);

            let reader = primitive.reader(|b| Some(&buffers[b.index()]));
            let base_vertex = vertices.len() as u32;
            let index_start = indices.len();

            // assert che non ci siano set UV > 0
            debug_assert!(
                primitive.attributes().all(|(semantic, _)| {
                    use gltf::mesh::Semantic::TexCoords;
                    match semantic {
                        TexCoords(set) => set == 0,
                        _ => true,
                    }
                }),
                "Primitive {} set UV > 0 not yet supported",
                primitive.index()
            );

            let vertex_count = extract_vertices(&reader, &mut vertices)?;
            extract_indices(&reader, &mut indices, base_vertex, vertex_count);

            let index_end = indices.len();

            if reader.read_tangents().is_none() {
                trace!("generate tangent for {:?}", primitive.material().name());
                generate_mikktspace_tangents(&mut vertices, &indices[index_start..index_end]);
            }

            let g_mat = primitive.material().index().unwrap_or_default();
            let material_id = *material_map.get(&g_mat).unwrap();

            submeshes.push(SubMesh {
                index_range: index_start as u32..index_end as u32,
                material: material_id,
            });
        }

        let mesh_source = MeshSource::File {
            path: path.as_ref().into(),
            submesh_index: g_mesh.index(),
        };

        let desc = MeshDesc {
            vertices,
            indices,
            submeshes,
            bounds: extract_bbox(&g_mesh),
        };

        let asset = MeshAsset { desc, mesh_source };

        let mesh_id = asset_mgr.add(asset);

        meshes.push(mesh_id);
    }

    let root_name = path
        .as_ref()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("__gltf_root__")
        .to_string();
    let file_path = path.as_ref().to_string_lossy();

    let nodes = load_nodes(root_name.as_str(), gltf);

    let scene = GltfGroup {
        meshes,
        nodes,
        name: file_path.to_string(),
    };

    // print_gltf_document(&gltf);
    info!("loading gltf took: {:?}", timer.elapsed());

    Ok(scene)
}

fn create_virtual_root_node(root_name: &str) -> NodeData {
    NodeData {
        name: root_name.to_string(),
        local_transform: TransformComponent::default(),
        mesh: None,
        children: Vec::new(),
    }
}

fn load_nodes(root_name: &str, gltf: Document) -> Vec<NodeData> {
    let mut nodes = Vec::new();

    // Aggiunge il root virtuale del file glTF
    nodes.push(create_virtual_root_node(root_name));

    // Tiene traccia dei parent del glTF
    let mut has_parent = vec![false; gltf.nodes().len()];

    // Copia i nodi del glTF
    for node in gltf.nodes() {
        let children = node
            .children()
            .map(|c| c.index() + 1) // +1 per il root virtuale
            .collect();

        for child in node.children() {
            has_parent[child.index()] = true;
        }

        nodes.push(NodeData {
            name: node.name().unwrap_or("no-name").to_string(),
            local_transform: TransformComponent::from_gltf(&node),
            mesh: node.mesh().map(|m| m.index()),
            children,
        });
    }

    // Collega il root virtuale a tutti i root del glTF
    for (i, has_parent) in has_parent.iter().enumerate() {
        if !*has_parent {
            nodes[0].children.push(i + 1);
        }
    }
    nodes
}

fn generate_mikktspace_tangents(vertices: &mut [MeshVertexData], indices: &[u32]) {
    use mikktspace::{Geometry, generate_tangents};

    debug_assert!(indices.iter().all(|&i| (i as usize) < vertices.len()));

    struct Mikkt<'a> {
        vertices: &'a mut [MeshVertexData],
        indices: Vec<u32>,
    }

    impl Geometry for Mikkt<'_> {
        fn num_faces(&self) -> usize {
            self.indices.len() / 3
        }

        fn num_vertices_of_face(&self, _: usize) -> usize {
            3
        }

        fn position(&self, face: usize, vert: usize) -> [f32; 3] {
            self.vertices[self.indices[face * 3 + vert] as usize].position
        }

        fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
            self.vertices[self.indices[face * 3 + vert] as usize].normal
        }

        fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
            self.vertices[self.indices[face * 3 + vert] as usize].uv[0..2]
                .try_into()
                .unwrap()
        }

        fn set_tangent(
            &mut self,
            tangent: [f32; 3],
            _bitangent: [f32; 3],
            _f_mag_s: f32,
            _f_mag_t: f32,
            orientation: bool,
            face: usize,
            vert: usize,
        ) {
            let sign = if orientation { 1.0 } else { -1.0 };
            let idx = self.indices[face * 3 + vert] as usize;
            let v = &mut self.vertices[idx];

            v.tangent[0] += tangent[0];
            v.tangent[1] += tangent[1];
            v.tangent[2] += tangent[2];

            // w = ±1 senza accumulo
            // FIX: invert w
            v.tangent[3] = -sign;
        }
    }

    let mut geom = Mikkt {
        vertices,
        indices: indices.to_vec(),
    };

    let result = generate_tangents(&mut geom);
    debug_assert!(result, "mikkspace: failed to genetate tangents");
}

fn extract_indices<'a, F>(
    reader: &Reader<'a, 'a, F>,
    indices: &mut Vec<u32>,
    base_vertex: u32,
    vertex_count: usize,
) where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'a [u8]>,
{
    if let Some(read_indices) = reader.read_indices() {
        for i in read_indices.into_u32() {
            indices.push(base_vertex + i);
        }
    } else {
        warn!("Missing Indices, recalculating from vertices ...");
        indices.extend(base_vertex..base_vertex + vertex_count as u32);
    }
}

fn extract_vertices<'a, F>(
    reader: &Reader<'a, 'a, F>,
    vertices: &mut Vec<MeshVertexData>,
) -> Result<usize, ImportError>
where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'a [u8]>,
{
    let base = vertices.len();

    let positions = reader.read_positions().ok_or_else(|| {
        warn!("Gltf: missing POSITION");
        ImportError::MissingPositions
    })?;

    for position in positions {
        vertices.push(MeshVertexData {
            position,
            normal: [0.0, 1.0, 0.0],
            tangent: [0.0; 4],
            uv: [0.0; 4],
        });
    }

    let count = vertices.len() - base;

    if let Some(normals) = reader.read_normals() {
        for (i, normal) in normals.enumerate() {
            vertices[base + i].normal = normal;
        }
    }

    if let Some(uvs) = reader.read_tex_coords(0) {
        for (i, uv) in uvs.into_f32().enumerate() {
            vertices[base + i].uv[0] = uv[0];
            vertices[base + i].uv[1] = uv[1];
        }
    }

    if let Some(uvs) = reader.read_tex_coords(1) {
        for (i, uv) in uvs.into_f32().enumerate() {
            vertices[base + i].uv[2] = uv[0];
            vertices[base + i].uv[3] = uv[1];
        }
    }

    if let Some(tangents) = reader.read_tangents() {
        for (i, t) in tangents.enumerate() {
            vertices[base + i].tangent = t;
        }
    }

    Ok(count)
}

fn extract_bbox(mesh: &gltf::Mesh) -> BoundingBox {
    let mut bounding_box = BoundingBox::new_empty();
    for primitive in mesh.primitives() {
        let b = primitive.bounding_box();
        bounding_box.extend(&b.min);
        bounding_box.extend(&b.max);
    }
    bounding_box
}

#[allow(unused)]
fn get_primitive_mode(mode: gltf::mesh::Mode) -> wgpu::PrimitiveTopology {
    match mode {
        gltf::mesh::Mode::Points => wgpu::PrimitiveTopology::PointList,
        gltf::mesh::Mode::Lines => wgpu::PrimitiveTopology::LineList,
        gltf::mesh::Mode::LineStrip => wgpu::PrimitiveTopology::LineStrip,
        gltf::mesh::Mode::Triangles => wgpu::PrimitiveTopology::TriangleList,
        gltf::mesh::Mode::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        _ => panic!("Error loading mesh topology isn't supported!"),
    }
}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        ImportError::Io(e)
    }
}

impl From<gltf::Error> for ImportError {
    fn from(e: gltf::Error) -> Self {
        ImportError::Gltf(e)
    }
}

#[allow(dead_code)]
fn print_gltf_document(document: &gltf::Document) {
    fn print_mesh(mesh: gltf::Mesh) {
        println!(
            " - Mesh #{}: {}",
            mesh.index(),
            mesh.name().unwrap_or("<Unnamed>")
        );
        for primitive in mesh.primitives() {
            let index = primitive.indices().expect("indices not found");
            println!(
                " - Primitive #{} Index Count {}",
                primitive.index(),
                index.count()
            );
        }
    }

    fn print_node(node: &gltf::Node) {
        println!(
            " - Node {} ({}) - transform: TRS {:?}",
            node.index(),
            node.name().unwrap_or("<Unnamed>"),
            node.transform().decomposed()
        );
        if let Some(mesh) = node.mesh() {
            print_mesh(mesh);
        }

        for child in node.children() {
            println!(" - Child Node {}", child.index());
        }
        println!();
    }

    document.nodes().for_each(|node| {
        print_node(&node);
    });
}

// remove %20 from uri, gltf spec requires that spaces in uri are encoded as %20, but we want to support unencoded spaces in file paths
fn url_decoding(uri: &str) -> String {
    uri.replace("%20", " ")
}

fn path_from_ginfo(info: &gltf::texture::Info<'_>, base_dir: &Path) -> Option<std::path::PathBuf> {
    match info.texture().source().source() {
        gltf::image::Source::Uri { uri, .. } => Some(base_dir.join(url_decoding(uri))),
        _ => None,
    }
}

fn path_from_gtexture(
    texture: gltf::texture::Texture<'_>,
    base_dir: &Path,
) -> Option<std::path::PathBuf> {
    match texture.source().source() {
        gltf::image::Source::Uri { uri, .. } => Some(base_dir.join(url_decoding(uri))),
        _ => None,
    }
}

fn texture_transform_from_ginfo(
    info: gltf::texture::Info<'_>,
) -> Option<material_desc::TextureTransform> {
    info.texture_transform()
        .map(|t| material_desc::TextureTransform {
            offset: t.offset(),
            rotation: t.rotation(),
            scale: t.scale(),
        })
}

// Gltf 1.4.1, doesn't support texture transform natively,
// so we need to parse it manually
fn parse_transform(ext: &serde_json::Value) -> material_desc::TextureTransform {
    let offset = ext
        .get("offset")
        .and_then(|v| v.as_array())
        .map(|a| [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32])
        .unwrap_or([0.0, 0.0]);

    let scale = ext
        .get("scale")
        .and_then(|v| v.as_array())
        .map(|a| [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32])
        .unwrap_or([1.0, 1.0]);

    let rotation = ext.get("rotation").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

    material_desc::TextureTransform {
        offset,
        scale,
        rotation,
    }
}

#[derive(serde::Deserialize)]
struct SheenExt {
    #[serde(rename = "sheenColorFactor")]
    color_factor: Option<[f32; 3]>,

    #[serde(rename = "sheenRoughnessFactor")]
    roughness_factor: Option<f32>,
}

fn parse_gltf_material_sheen(mat: &gltf::Material) -> Option<material_desc::Sheen> {
    let ext = mat.extensions()?.get("KHR_materials_sheen")?;
    let sheen: SheenExt = serde_json::from_value(ext.clone()).ok()?;

    Some(material_desc::Sheen {
        color_factor: sheen.color_factor.unwrap_or([0.0, 0.0, 0.0]),
        roughness_factor: sheen.roughness_factor.unwrap_or(0.0),
    })
}

fn create_texture(
    path: Option<std::path::PathBuf>,
    usage: TextureUsage,
    asset_mgr: &mut AssetManager,
) -> Option<ResourceId> {
    let path = path?;

    let desc = TextureDesc::File {
        path,
        usage,
        sampler: SamplerDesc::default(),
        mipmaps: true,
    };

    let texture = TextureAsset {
        desc,
    };
    Some(asset_mgr.add(texture))
}

fn create_material<P: AsRef<Path>>(
    gltf_material: &gltf::Material,
    asset_mgr: &mut AssetManager,
    path: P,
) -> ResourceId {
    use material_desc::MaterialTextureSlot::*;
    use material_desc::*;

    let path_name = path
        .as_ref()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mat_id = gltf_material.index().unwrap_or_default();

    let key = format!("{path_name}[{mat_id}]");

    let material_name = gltf_material.name().unwrap_or(&key);

    let parent_path = path.as_ref().parent().unwrap_or_else(|| Path::new(""));

    // gltf pbr material
    let pbr = gltf_material.pbr_metallic_roughness();

    let mut material_desc = MaterialDesc::default();

    let alpha_mode = match gltf_material.alpha_mode() {
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        gltf::material::AlphaMode::Mask => gltf_material
            .alpha_cutoff()
            .map(|alpha_cutoff| AlphaMode::Mask { alpha_cutoff })
            .unwrap_or(AlphaMode::mask_default()),
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
    };

    material_desc.set_name(material_name);
    material_desc.alpha_mode = alpha_mode;
    material_desc.base_color_factor = pbr.base_color_factor().into();
    material_desc.roughness_factor = pbr.roughness_factor();
    material_desc.metallic_factor = pbr.metallic_factor();
    material_desc.emissive_factor = Vec3::from(gltf_material.emissive_factor()).extend(0.0);
    material_desc.ior = gltf_material.ior().unwrap_or(1.5);

    gltf_material.extensions().into_iter().for_each(|k| {
        println!("Material {} has extension {:#?}", material_name, k);
    });

    material_desc.sheen = parse_gltf_material_sheen(gltf_material);

    if let Some(info) = pbr.base_color_texture() {
        let path = path_from_ginfo(&info, parent_path);
        let texture_id = create_texture(path, TextureUsage::Albedo, asset_mgr);

        material_desc.set_texture(
            texture_id,
            BaseColor,
            info.tex_coord(),
            texture_transform_from_ginfo(info),
        );
    }

    if let Some(normal_texture) = gltf_material.normal_texture() {
        let path = path_from_gtexture(normal_texture.texture(), parent_path);
        let texture_id = create_texture(path, TextureUsage::Normal, asset_mgr);

        material_desc.normal_scale = normal_texture.scale();
        material_desc.set_texture(
            texture_id,
            Normal,
            normal_texture.tex_coord(),
            normal_texture
                .extension_value("KHR_texture_transform")
                .map(parse_transform),
        );
    }
    if let Some(info) = pbr.metallic_roughness_texture() {
        let path = path_from_ginfo(&info, parent_path);
        let texture_id = create_texture(path, TextureUsage::MetallicRoughness, asset_mgr);
        material_desc.set_texture(
            texture_id,
            MetallicRoughness,
            info.tex_coord(),
            texture_transform_from_ginfo(info),
        );
    }
    if let Some(occlusion_texture) = gltf_material.occlusion_texture() {
        let path = path_from_gtexture(occlusion_texture.texture(), parent_path);
        let texture_id = create_texture(path, TextureUsage::Occlusion, asset_mgr);

        material_desc.occlusion_strength = occlusion_texture.strength().clamp(0.0, 1.0);
        material_desc.set_texture(
            texture_id,
            Occlusion,
            occlusion_texture.tex_coord(),
            occlusion_texture
                .extension_value("KHR_texture_transform")
                .map(parse_transform),
        )
    }
    if let Some(info) = gltf_material.emissive_texture() {
        let path = path_from_ginfo(&info, parent_path);
        let texture_id = create_texture(path, TextureUsage::Emissive, asset_mgr);
        material_desc.set_texture(
            texture_id,
            Emissive,
            info.tex_coord(),
            texture_transform_from_ginfo(info),
        );
    }
    if let Some(transmission) = gltf_material.transmission() {
        let factor = transmission.transmission_factor();
        material_desc.transmission = Some(material_desc::Transmission { factor });

        if let Some(transmission_texture_info) = transmission.transmission_texture() {
            let path = path_from_ginfo(&transmission_texture_info, parent_path);
            let texture_id = create_texture(path, TextureUsage::Transmission, asset_mgr);
            material_desc.set_texture(
                texture_id,
                Transmission,
                transmission_texture_info.tex_coord(),
                texture_transform_from_ginfo(transmission_texture_info),
            );
        }
    }

    if let Some(volume) = gltf_material.volume() {
        material_desc.volume = Some(material_desc::Volume {
            thickness_factor: volume.thickness_factor(),
            attenuation_distance: volume.attenuation_distance(),
            attenuation_color: volume.attenuation_color(),
        });

        if let Some(volume_texture_info) = volume.thickness_texture() {
            let path = path_from_ginfo(&volume_texture_info, parent_path);
            let texture_id = create_texture(path, TextureUsage::Volume, asset_mgr);
            material_desc.set_texture(
                texture_id,
                Volume,
                volume_texture_info.tex_coord(),
                texture_transform_from_ginfo(volume_texture_info),
            );
        }
    }

    debug!("Metarial created {:#?}", material_desc);
    let asset = MaterialAsset {
        desc: material_desc,
        key,
    };

    asset_mgr.add(asset)
}

fn create_materials<P: AsRef<Path>>(
    gltf: &Document,
    path: P,
    asset_mgr: &mut AssetManager,
) -> HashMap<usize, ResourceId> {
    let mut materials = HashMap::new();
    for material in gltf.materials() {
        let mat_id = material.index().unwrap_or_default();
        let asset_id = create_material(&material, asset_mgr, &path);
        materials.insert(mat_id, asset_id);
    }

    materials
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_load_scene() {
        let path = "./assets/cube/cube.gltf";
        let mut asset_mgr = AssetManager::default();

        let e = load_gltf_internal(path, &mut asset_mgr);

        assert!(e.is_ok());
        assert_eq!(e.ok().iter().len(), 1);
    }
}
