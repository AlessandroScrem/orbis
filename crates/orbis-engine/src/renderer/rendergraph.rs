#![allow(dead_code)]

use super::*;
use crate::renderer::framebuilder::FrameData;
use crate::renderer::scene_renderer::RenderContext;
use crate::renderer::transient_pool::{
    TransientRequest, TransientResourcePool, TransientResourceResolver,
};
use std::collections::HashMap;

#[allow(clippy::upper_case_acronyms)]
#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub enum ResourceId {
    ENTITY,
    DEPTH,
    HDR,
    LDR,
    OPAQUE,
    PICKBUFFER,
    SHADOWMAP,
}

use std::fmt;
impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Self::HDR => "HDR",
            Self::OPAQUE => "Opaque",
            Self::DEPTH => "Depth",
            Self::ENTITY => "EntityID",
            Self::LDR => "LDR",
            Self::PICKBUFFER => "PickBuffer",
            Self::SHADOWMAP => "ShadowMap",
        };
        write!(f, "{}", name)
    }
}

impl fmt::Debug for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl ResourceId {
    /// Whether the graph owns this resource's per-frame allocation.
    ///
    /// Shadow maps are managed by ShadowManager, while pick buffers belong to
    /// the picking/readback path. Neither may be allocated or aliased by the
    /// transient render-target pool.
    fn is_transient(self) -> bool {
        matches!(self, Self::ENTITY | Self::DEPTH | Self::HDR | Self::OPAQUE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResourceAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceUse {
    pub(super) resource: ResourceId,
    pub(super) access: ResourceAccess,
}

#[derive(Clone, Copy)]
enum VisitState {
    NotVisited,
    Visiting, // ← in stack node
    Visited,
}

// =========================
// Graph internals
// =========================

#[derive(Debug)]
struct ResourceNode {
    id: ResourceId,
    writers: Vec<usize>,
    readers: Vec<usize>,
    accesses: Vec<(usize, ResourceAccess)>,
}

struct PassNode<'a> {
    pass: &'a dyn RenderPass,
    resources: Vec<ResourceUse>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeKind {
    ReadAfterWrite,
    WriteAfterRead,
    WriterOrder,
}

#[derive(Clone, Copy)]
struct Edge {
    from: usize,
    to: usize,
    resource: ResourceId,
    kind: EdgeKind,
}

// =========================
// RenderGraph
// =========================

pub(crate) struct RenderGraph {
    passes: Vec<Box<dyn RenderPass>>,
}

#[derive(Debug, Default)]
pub(crate) struct CompiledRenderGraph {
    order: Vec<usize>,
    lifetimes: Vec<ResourceLifetime>,
}

impl CompiledRenderGraph {
    pub(crate) fn lifetimes(&self) -> &[ResourceLifetime] {
        &self.lifetimes
    }
}

/// Inclusive pass-order lifetime of a logical render-graph resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceLifetime {
    pub(crate) resource: ResourceId,
    pub(crate) first_use: usize,
    pub(crate) last_use: usize,
}

impl RenderGraph {
    pub(crate) fn new() -> Self {
        Self { passes: Vec::new() }
    }

    // -------------------------
    // Add a pass to the render graph
    // -------------------------
    pub(crate) fn add_pass<P: RenderPass + 'static>(&mut self, pass: P) {
        self.passes.push(Box::new(pass));
    }

    // -------------------------
    //  Compiles the render graph into a persistent form that can be executed.
    // -------------------------
    pub(crate) fn compile_persistent(&self) -> Result<CompiledRenderGraph, String> {
        let (order, lifetimes) = self.compile_lifetimes()?;
        Ok(CompiledRenderGraph { order, lifetimes })
    }

    // -------------------------
    // Execute passes in dependency order
    // -------------------------
    pub(crate) fn execute(
        &mut self,
        compiled: &CompiledRenderGraph,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) {
        for &idx in &compiled.order {
            let pass = &mut self.passes[idx];
            pass.execute(encoder, ctx, frame);
        }
    }

    // -------------------------
    // Build Pass + Resource graph
    // -------------------------
    // Rules:
    // -------------------------
    // Access      Dependency
    // Read	       Depends from last writer
    // Write	   Depends from writer and previous readers
    // ReadWrite   Depends from last writer and previous readers; becomes the new writer

    fn build_graph(&self) -> (Vec<PassNode<'_>>, HashMap<ResourceId, ResourceNode>) {
        let mut resources: HashMap<ResourceId, ResourceNode> = HashMap::new();

        let pass_nodes: Vec<_> = self
            .passes
            .iter()
            .map(|p| PassNode {
                pass: p.as_ref(),
                resources: p.resources().to_vec(),
            })
            .collect();

        for (i, pass) in pass_nodes.iter().enumerate() {
            for resource_use in &pass.resources {
                let entry = resources
                    .entry(resource_use.resource)
                    .or_insert(ResourceNode {
                        id: resource_use.resource,
                        writers: Vec::new(),
                        readers: Vec::new(),
                        accesses: Vec::new(),
                    });

                entry.accesses.push((i, resource_use.access));

                match resource_use.access {
                    ResourceAccess::Read => {
                        entry.readers.push(i);
                    }
                    ResourceAccess::Write => {
                        entry.writers.push(i);
                    }
                    ResourceAccess::ReadWrite => {
                        entry.readers.push(i);
                        entry.writers.push(i);
                    }
                }
            }
        }

        (pass_nodes, resources)
    }

    // -------------------------
    // Build dependencies (with resource info)
    // -------------------------
    fn build_dependencies(&self, resources: &HashMap<ResourceId, ResourceNode>) -> Vec<Vec<Edge>> {
        let mut deps = vec![Vec::new(); self.passes.len()];

        for res in resources.values() {
            let mut accesses = res.accesses.clone();

            if let Some(first_write) = accesses
                .iter()
                .position(|(_, access)| *access == ResourceAccess::Write)
            {
                let preceding = accesses.drain(..first_write).collect::<Vec<_>>();
                let writer = accesses.remove(0);

                accesses.splice(0..0, preceding);
                accesses.insert(0, writer);
            }

            let mut last_writer: Option<usize> = None;
            let mut readers = Vec::new();

            for (pass, access) in &accesses {
                match access {
                    ResourceAccess::Read => {
                        if let Some(writer) = last_writer {
                            deps[*pass].push(Edge {
                                from: *pass,
                                to: writer,
                                resource: res.id,
                                kind: EdgeKind::ReadAfterWrite,
                            });
                        }

                        readers.push(*pass);
                    }

                    ResourceAccess::Write | ResourceAccess::ReadWrite => {
                        if let Some(writer) = last_writer {
                            let kind = if *access == ResourceAccess::ReadWrite {
                                EdgeKind::ReadAfterWrite
                            } else {
                                EdgeKind::WriterOrder
                            };

                            deps[*pass].push(Edge {
                                from: *pass,
                                to: writer,
                                resource: res.id,
                                kind,
                            });
                        }

                        for reader in readers.drain(..) {
                            if reader != *pass {
                                deps[*pass].push(Edge {
                                    from: *pass,
                                    to: reader,
                                    kind: EdgeKind::WriteAfterRead,
                                    resource: res.id,
                                });
                            }
                        }

                        last_writer = Some(*pass);
                    }
                }
            }
        }

        for edges in &mut deps {
            edges.sort_by(|left, right| {
                left.resource
                    .to_string()
                    .cmp(&right.resource.to_string())
                    .then_with(|| left.to.cmp(&right.to))
                    .then_with(|| match (left.kind, right.kind) {
                        (EdgeKind::ReadAfterWrite, EdgeKind::WriterOrder) => {
                            std::cmp::Ordering::Less
                        }
                        (EdgeKind::WriterOrder, EdgeKind::ReadAfterWrite) => {
                            std::cmp::Ordering::Greater
                        }
                        _ => std::cmp::Ordering::Equal,
                    })
            });
            edges.dedup_by(|left, right| {
                left.from == right.from && left.to == right.to && left.resource == right.resource
            });
        }

        deps
    }

    // -------------------------
    // Compile (topo sort + cycle detection)
    // Topological sort (DFS)
    // -------------------------
    fn compile(&self) -> Result<Vec<usize>, String> {
        let (_passes, resources) = self.build_graph();

        if let Some(resource_name) = resources
            .values()
            .filter(|resource| {
                !resource.readers.is_empty()
                    && !resource
                        .accesses
                        .iter()
                        .any(|(_, access)| *access == ResourceAccess::Write)
            })
            .map(|resource| resource.id.to_string())
            .min()
        {
            return Err(format!(
                "Resource {resource_name} is read but never written"
            ));
        }

        let deps = self.build_dependencies(&resources);

        let mut state = vec![VisitState::NotVisited; self.passes.len()];
        let mut result = Vec::new();
        let mut stack: Vec<Edge> = Vec::new();

        fn visit(
            i: usize,
            deps: &Vec<Vec<Edge>>,
            state: &mut Vec<VisitState>,
            stack: &mut Vec<Edge>,
            result: &mut Vec<usize>,
            passes: &Vec<Box<dyn RenderPass>>,
        ) -> Result<(), String> {
            match state[i] {
                VisitState::Visited => return Ok(()),
                VisitState::Visiting => {
                    let cycle_start = stack.iter().position(|edge| edge.from == i).unwrap_or(0);
                    let cycle_lines = stack[cycle_start..]
                        .iter()
                        .map(|edge| match edge.kind {
                            EdgeKind::ReadAfterWrite => format!(
                                "{} reads {} -> depends on {}",
                                passes[edge.from].name(),
                                edge.resource,
                                passes[edge.to].name()
                            ),
                            EdgeKind::WriteAfterRead => format!(
                                "{} writes {} after reader {}",
                                passes[edge.from].name(),
                                edge.resource,
                                passes[edge.to].name()
                            ),
                            EdgeKind::WriterOrder => format!(
                                "{} writes {} after {}",
                                passes[edge.from].name(),
                                edge.resource,
                                passes[edge.to].name()
                            ),
                        })
                        .collect::<Vec<_>>();

                    return Err(format!("Cycle detected:\n{}", cycle_lines.join("\n")));
                }
                VisitState::NotVisited => {}
            }

            state[i] = VisitState::Visiting;

            for edge in &deps[i] {
                stack.push(*edge);
                visit(edge.to, deps, state, stack, result, passes)?;
                stack.pop();
            }

            state[i] = VisitState::Visited;
            result.push(i);

            Ok(())
        }

        for i in 0..self.passes.len() {
            visit(i, &deps, &mut state, &mut stack, &mut result, &self.passes)?;
        }

        Ok(result)
    }

    /// Compiles the graph and derives resource lifetimes in execution-order indices.
    ///
    /// The framebuffer cache consumes these lifetimes to resolve logical targets
    /// to retained pool allocations before execution.
    fn compile_lifetimes(&self) -> Result<(Vec<usize>, Vec<ResourceLifetime>), String> {
        let order = self.compile()?;
        let mut execution_index = vec![0; order.len()];
        for (position, &pass_index) in order.iter().enumerate() {
            execution_index[pass_index] = position;
        }

        let mut lifetimes: HashMap<ResourceId, (usize, usize)> = HashMap::new();
        for (pass_index, pass) in self.passes.iter().enumerate() {
            let position = execution_index[pass_index];
            for resource_use in pass.resources() {
                lifetimes
                    .entry(resource_use.resource)
                    .and_modify(|(first, last)| {
                        *first = (*first).min(position);
                        *last = (*last).max(position);
                    })
                    .or_insert((position, position));
            }
        }

        let mut lifetimes: Vec<_> = lifetimes
            .into_iter()
            .map(|(resource, (first_use, last_use))| ResourceLifetime {
                resource,
                first_use,
                last_use,
            })
            .collect();
        lifetimes.sort_by_key(|lifetime| lifetime.resource.to_string());

        Ok((order, lifetimes))
    }

    /// Allocates physical slots for the graph's logical resources using compiled lifetimes.
    ///
    /// The descriptor callback must include every GPU property relevant to compatibility.
    /// The returned resolver maps graph resources to retained pool allocations. The
    /// framebuffer cache uses the same lifetime/slot contract for GPU-backed resources.
    fn allocate_transient_resources<D, R>(
        &self,
        pool: &mut TransientResourcePool<D, R>,
        mut descriptor_for: impl FnMut(ResourceId) -> D,
        create: impl FnMut(&D) -> R,
    ) -> Result<TransientResourceResolver<ResourceId>, String>
    where
        D: Eq + Clone,
    {
        let (_, lifetimes) = self.compile_lifetimes()?;
        let lifetimes: Vec<_> = lifetimes
            .into_iter()
            .filter(|lifetime| lifetime.resource.is_transient())
            .collect();
        let requests: Vec<_> = lifetimes
            .iter()
            .map(|lifetime| {
                TransientRequest::new(
                    descriptor_for(lifetime.resource),
                    lifetime.first_use,
                    lifetime.last_use,
                )
            })
            .collect();

        let slots = pool.allocate_frame(&requests, create);
        let slots = lifetimes
            .into_iter()
            .zip(slots)
            .map(|(lifetime, slot)| (lifetime.resource, slot))
            .collect();

        Ok(TransientResourceResolver::from_slots(slots))
    }
}

impl RenderGraph {
    pub(crate) fn compile_names(&self) -> Result<Vec<String>, String> {
        let order = self.compile()?;

        Ok(order
            .into_iter()
            .map(|i| self.passes[i].name().to_string())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPass {
        name: &'static str,
        resources: Vec<ResourceUse>,
    }

    impl RenderPass for TestPass {
        fn name(&self) -> &'static str {
            self.name
        }

        fn resources(&self) -> &[ResourceUse] {
            &self.resources
        }
    }

    fn test_pass(name: &'static str, resources: &[ResourceUse]) -> TestPass {
        TestPass {
            name,
            resources: resources.to_vec(),
        }
    }

    fn has_dependency(
        graph: &RenderGraph,
        dependencies: &[Vec<Edge>],
        dependent: &str,
        prerequisite: &str,
        kind: EdgeKind,
    ) -> bool {
        let find_pass = |name: &str| graph.passes.iter().position(|pass| pass.name() == name);

        let (Some(dependent), Some(prerequisite)) = (find_pass(dependent), find_pass(prerequisite))
        else {
            return false;
        };

        dependencies[dependent]
            .iter()
            .any(|edge| edge.to == prerequisite && edge.kind == kind)
    }

    #[test]
    fn compile_returns_persistent_graph() {
        let graph = RenderGraph::new();

        let compiled = graph.compile_persistent().unwrap();

        assert!(compiled.order.is_empty());
        assert!(compiled.lifetimes.is_empty());
    }

    #[test]
    fn compile_persistent_preserves_order_and_lifetimes() {
        let mut graph = RenderGraph::new();

        graph.add_pass(ShadowPass {});
        graph.add_pass(MeshPass::opaque());

        let compiled = graph.compile_persistent().unwrap();

        assert_eq!(compiled.order, vec![0, 1]);

        let shadow_map = compiled
            .lifetimes
            .iter()
            .find(|lifetime| lifetime.resource == ResourceId::SHADOWMAP)
            .unwrap();
        assert_eq!((shadow_map.first_use, shadow_map.last_use), (0, 1));
    }

    #[test]
    fn should_run_in_order() {
        let mut graph = RenderGraph::new();

        graph.add_pass(ShadowPass {});
        graph.add_pass(MeshPass::opaque());
        graph.add_pass(SkyboxPass {});
        graph.add_pass(BuildMipmapsPass {});
        graph.add_pass(MeshPass::transmission());
        graph.add_pass(LightsIconPass {});
        graph.add_pass(AxisPass {});
        graph.add_pass(LinesPass {});
        graph.add_pass(LinearizePass {});
        graph.add_pass(OutlinePass {});

        match graph.compile_names() {
            Ok(order) => {
                println!("Order: {:?}", order);
                assert_eq!(
                    order,
                    vec![
                        "ShadowPass Opaque",
                        "MeshPass Opaque",
                        "SkyboxPass",
                        "BuildMipmapsPass",
                        "MeshPass Transmission",
                        "LightPass",
                        "AxisPass",
                        "LinesPass",
                        "LinearizePass",
                        "OutlinePass",
                    ]
                );
            }
            Err(e) => panic!("{}", e),
        }
    }

    #[test]
    fn dependency_order_is_deterministic() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "Reader LDR",
            &[ResourceUse {
                resource: ResourceId::LDR,
                access: ResourceAccess::Read,
            }],
        ));
        graph.add_pass(test_pass(
            "Reader HDR",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Read,
            }],
        ));
        graph.add_pass(test_pass(
            "Writer LDR",
            &[ResourceUse {
                resource: ResourceId::LDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "Writer HDR",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let expected = graph.compile_names().unwrap();
        for _ in 0..32 {
            assert_eq!(graph.compile_names().unwrap(), expected);
        }
    }

    #[test]
    fn multiple_writers_preserve_registration_order() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "First writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "Second writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "Reader",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Read,
            }],
        ));

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["First writer", "Second writer", "Reader"]
        );
    }

    #[test]
    fn reader_between_writers_observes_the_preceding_write() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "First writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "Reader",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Read,
            }],
        ));

        graph.add_pass(test_pass(
            "Second writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let (_, resources) = graph.build_graph();
        let dependencies = graph.build_dependencies(&resources);

        assert!(dependencies[1].iter().any(|edge| {
            edge.to == 0
                && edge.resource == ResourceId::HDR
                && matches!(edge.kind, EdgeKind::ReadAfterWrite)
        }));
        assert!(dependencies[2].iter().any(|edge| {
            edge.to == 1
                && edge.resource == ResourceId::HDR
                && matches!(edge.kind, EdgeKind::WriteAfterRead)
        }));

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["First writer", "Reader", "Second writer"]
        );
    }

    #[test]
    fn read_without_any_writer_is_reported_as_an_error() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "Reader",
            &[ResourceUse {
                resource: ResourceId::PICKBUFFER,
                access: ResourceAccess::Read,
            }],
        ));
        assert!(
            graph
                .compile()
                .unwrap_err()
                .contains("PickBuffer is read but never written")
        );
    }

    #[test]
    fn should_find_cycles() {
        let mut graph = RenderGraph::new();

        graph.add_pass(test_pass(
            "Pass A",
            &[
                ResourceUse {
                    resource: ResourceId::LDR,
                    access: ResourceAccess::Read,
                },
                ResourceUse {
                    resource: ResourceId::DEPTH,
                    access: ResourceAccess::Write,
                },
            ],
        ));
        graph.add_pass(test_pass(
            "Pass B",
            &[
                ResourceUse {
                    resource: ResourceId::DEPTH,
                    access: ResourceAccess::Read,
                },
                ResourceUse {
                    resource: ResourceId::LDR,
                    access: ResourceAccess::Write,
                },
            ],
        ));

        let error = graph.compile().unwrap_err();

        assert!(error.contains("Pass A reads LDR -> depends on Pass B"));
        assert!(error.contains("Pass B reads Depth -> depends on Pass A"));
        assert_eq!(
            error.lines().count(),
            3,
            "diagnostic should contain only the cycle"
        );
    }
    #[test]
    fn graph_allocates_slots_from_compiled_resource_lifetimes() {
        struct Pass {
            resources: Vec<ResourceUse>,
        }
        impl RenderPass for Pass {
            fn name(&self) -> &'static str {
                "TestPass"
            }
            fn resources(&self) -> &[ResourceUse] {
                &self.resources
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "TestéPass",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "TestPass",
            &[
                ResourceUse {
                    resource: ResourceId::HDR,
                    access: ResourceAccess::Read,
                },
                ResourceUse {
                    resource: ResourceId::OPAQUE,
                    access: ResourceAccess::Write,
                },
            ],
        ));
        let mut pool = TransientResourcePool::new();

        let slots = graph
            .allocate_transient_resources(&mut pool, |_| "same-format", |_| ())
            .unwrap();

        // HDR remains live through pass 1, where OPAQUE is first written.
        assert_ne!(
            slots.slot_for(&ResourceId::HDR),
            slots.slot_for(&ResourceId::OPAQUE)
        );
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn graph_keeps_incompatible_formats_in_separate_slots() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "TestPass",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "TestPass",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Read,
            }],
        ));
        graph.add_pass(test_pass(
            "TestPass",
            &[ResourceUse {
                resource: ResourceId::OPAQUE,
                access: ResourceAccess::Write,
            }],
        ));

        let mut pool = TransientResourcePool::new();

        let slots = graph
            .allocate_transient_resources(
                &mut pool,
                |res| match res {
                    ResourceId::HDR => "format_A",
                    ResourceId::OPAQUE => "format_B",
                    _ => "other",
                },
                |_| (),
            )
            .unwrap();

        assert_ne!(
            slots.slot_for(&ResourceId::HDR),
            slots.slot_for(&ResourceId::OPAQUE)
        );
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn transient_allocation_excludes_externally_managed_resources() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "Resource writer",
            &[ResourceUse {
                resource: ResourceId::SHADOWMAP,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "Resource writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let mut pool = TransientResourcePool::new();
        let slots = graph
            .allocate_transient_resources(&mut pool, |_| "texture", |_| ())
            .unwrap();

        assert_eq!(slots.slot_for(&ResourceId::SHADOWMAP), None);
        assert!(slots.slot_for(&ResourceId::HDR).is_some());
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn resource_lifetimes_use_compiled_execution_order() {
        let mut graph = RenderGraph::new();
        // Registered in reverse dependency order to ensure lifetimes are
        // computed from the compiled order rather than registration indices.
        graph.add_pass(test_pass(
            "Reader",
            &[
                ResourceUse {
                    resource: ResourceId::HDR,
                    access: ResourceAccess::Read,
                },
                ResourceUse {
                    resource: ResourceId::LDR,
                    access: ResourceAccess::Write,
                },
            ],
        ));
        graph.add_pass(test_pass(
            "Writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let (order, lifetimes) = graph.compile_lifetimes().unwrap();
        assert_eq!(order, vec![1, 0]);

        let hdr = lifetimes
            .iter()
            .find(|item| item.resource == ResourceId::HDR)
            .unwrap();
        assert_eq!((hdr.first_use, hdr.last_use), (0, 1));

        let ldr = lifetimes
            .iter()
            .find(|item| item.resource == ResourceId::LDR)
            .unwrap();
        assert_eq!((ldr.first_use, ldr.last_use), (1, 1));
    }

    #[test]
    fn read_write_resource_is_registered_as_reader_and_writer() {
        let mut graph = RenderGraph::new();
        graph.add_pass(test_pass(
            "ReadWritePass",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::ReadWrite,
            }],
        ));

        let (_, resources) = graph.build_graph();
        let hdr = resources.get(&ResourceId::HDR).unwrap();

        assert_eq!(hdr.readers, vec![0]);
        assert_eq!(hdr.writers, vec![0]);
    }

    #[test]
    fn read_write_without_any_writer_is_reported_as_an_error() {
        let mut graph = RenderGraph::new();

        graph.add_pass(test_pass(
            "ReadWrite",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::ReadWrite,
            }],
        ));

        let error = graph.compile().unwrap_err();

        assert!(error.contains("HDR is read but never written"));
    }

    #[test]
    fn read_write_resource_depends_on_writer() {
        let mut graph = RenderGraph::new();

        graph.add_pass(test_pass(
            "ReadWrite",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::ReadWrite,
            }],
        ));
        graph.add_pass(test_pass(
            "Writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let (_, resources) = graph.build_graph();
        let dependencies = graph.build_dependencies(&resources);

        assert!(has_dependency(
            &graph,
            &dependencies,
            "ReadWrite",
            "Writer",
            EdgeKind::ReadAfterWrite
        ));

        assert_eq!(graph.compile_names().unwrap(), vec!["Writer", "ReadWrite"]);
    }

    #[test]
    fn read_write_between_writers_preserves_resource_order() {
        let mut graph = RenderGraph::new();

        graph.add_pass(test_pass(
            "FirstWriter",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "ReadWrite",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::ReadWrite,
            }],
        ));
        graph.add_pass(test_pass(
            "SecondWriter",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let (_, resources) = graph.build_graph();
        let dependencies = graph.build_dependencies(&resources);

        assert!(has_dependency(
            &graph,
            &dependencies,
            "ReadWrite",
            "FirstWriter",
            EdgeKind::ReadAfterWrite
        ));
        assert!(has_dependency(
            &graph,
            &dependencies,
            "SecondWriter",
            "ReadWrite",
            EdgeKind::WriterOrder
        ));

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["FirstWriter", "ReadWrite", "SecondWriter"]
        );
    }

    #[test]
    fn reader_after_read_write_depends_on_read_write() {
        let mut graph = RenderGraph::new();

        graph.add_pass(test_pass(
            "Writer",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "ReadWrite",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::ReadWrite,
            }],
        ));
        graph.add_pass(test_pass(
            "Reader",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Read,
            }],
        ));

        let (_, resources) = graph.build_graph();
        let dependencies = graph.build_dependencies(&resources);

        assert!(has_dependency(
            &graph,
            &dependencies,
            "ReadWrite",
            "Writer",
            EdgeKind::ReadAfterWrite
        ));

        assert!(has_dependency(
            &graph,
            &dependencies,
            "Reader",
            "ReadWrite",
            EdgeKind::ReadAfterWrite
        ));

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["Writer", "ReadWrite", "Reader"]
        );
    }

    #[test]
    fn read_write_output_is_consumed_before_next_write() {
        let mut graph = RenderGraph::new();

        graph.add_pass(test_pass(
            "FirstWriter",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));
        graph.add_pass(test_pass(
            "ReadWrite",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::ReadWrite,
            }],
        ));
        graph.add_pass(test_pass(
            "Reader",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Read,
            }],
        ));
        graph.add_pass(test_pass(
            "SecondWriter",
            &[ResourceUse {
                resource: ResourceId::HDR,
                access: ResourceAccess::Write,
            }],
        ));

        let (_, resources) = graph.build_graph();
        let dependencies = graph.build_dependencies(&resources);

        // ReadWrite deve leggere il valore prodotto da FirstWriter.
        assert!(has_dependency(
            &graph,
            &dependencies,
            "ReadWrite",
            "FirstWriter",
            EdgeKind::ReadAfterWrite,
        ));

        // SecondWriter deve attendere che ReadWrite abbia terminato.
        assert!(has_dependency(
            &graph,
            &dependencies,
            "SecondWriter",
            "ReadWrite",
            EdgeKind::WriterOrder,
        ));

        // SecondWriter non deve sovrascrivere la risorsa prima che Reader
        // abbia terminato di leggerla.
        assert!(has_dependency(
            &graph,
            &dependencies,
            "SecondWriter",
            "Reader",
            EdgeKind::WriteAfterRead,
        ));

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["FirstWriter", "ReadWrite", "Reader", "SecondWriter"]
        );
    }
}
