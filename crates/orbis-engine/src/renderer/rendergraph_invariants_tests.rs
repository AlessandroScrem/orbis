use super::renderpass::{
    AxisPass, BuildMipmapsPass, LinearizePass, LinesPass, LightsIconPass, MeshPass, OutlinePass,
    RenderPass, RenderPassEnum, ShadowPass, SkyboxPass,
};
use super::rendergraph::{RenderGraph, ResourceId};

fn default_passes() -> Vec<RenderPassEnum> {
    vec![
        RenderPassEnum::Shadow(ShadowPass {}),
        RenderPassEnum::Mesh(MeshPass::opaque()),
        RenderPassEnum::Skybox(SkyboxPass {}),
        RenderPassEnum::BuildMipmaps(BuildMipmapsPass {}),
        RenderPassEnum::Transmission(MeshPass::transmission()),
        RenderPassEnum::LightsIcon(LightsIconPass {}),
        RenderPassEnum::Axis(AxisPass {}),
        RenderPassEnum::Lines(LinesPass {}),
        RenderPassEnum::Linearize(LinearizePass {}),
        RenderPassEnum::Outline(OutlinePass {}),
    ]
}

#[test]
fn default_passes_declare_their_resource_contracts() {
    let passes = default_passes();
    let expected: &[(&str, &[ResourceId], &[ResourceId])] = &[
        ("ShadowPass Opaque", &[], &[ResourceId::SHADOWMAP]),
        (
            "MeshPass Opaque",
            &[ResourceId::SHADOWMAP],
            &[ResourceId::HDR, ResourceId::ENTITY, ResourceId::DEPTH],
        ),
        (
            "SkyboxPass",
            &[ResourceId::HDR, ResourceId::DEPTH],
            &[ResourceId::HDR, ResourceId::DEPTH],
        ),
        (
            "BuildMipmapsPass",
            &[ResourceId::HDR],
            &[ResourceId::OPAQUE],
        ),
        (
            "MeshPass Transmission",
            &[
                ResourceId::HDR,
                ResourceId::OPAQUE,
                ResourceId::ENTITY,
                ResourceId::DEPTH,
                ResourceId::SHADOWMAP,
            ],
            &[ResourceId::HDR, ResourceId::ENTITY, ResourceId::DEPTH],
        ),
        (
            "LightPass",
            &[ResourceId::HDR, ResourceId::ENTITY, ResourceId::DEPTH],
            &[ResourceId::HDR, ResourceId::ENTITY, ResourceId::DEPTH],
        ),
        ("AxisPass", &[ResourceId::HDR], &[ResourceId::HDR]),
        ("BoundingboxPass", &[ResourceId::HDR], &[ResourceId::HDR]),
        ("LinearizePass", &[ResourceId::HDR], &[ResourceId::LDR]),
        (
            "OutlinePass",
            &[ResourceId::ENTITY, ResourceId::LDR],
            &[ResourceId::LDR],
        ),
    ];

    assert_eq!(passes.len(), expected.len());

    for (pass, (name, reads, writes)) in passes.iter().zip(expected) {
        assert_eq!(pass.name(), *name, "pass name changed");
        assert_eq!(pass.reads(), *reads, "{name}: read declaration changed");
        assert_eq!(pass.writes(), *writes, "{name}: write declaration changed");
    }
}

#[test]
fn default_graph_compiles_and_all_lifetimes_fit_execution_order() {
    let mut graph = RenderGraph::new();
    for pass in default_passes() {
        graph.add_pass(pass);
    }

    let (order, lifetimes) = graph.compile_lifetimes().unwrap();
    assert_eq!(order.len(), 10);

    for lifetime in lifetimes {
        assert!(lifetime.first_use <= lifetime.last_use);
        assert!(lifetime.last_use < order.len());
    }
}
