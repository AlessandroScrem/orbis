#![allow(dead_code)]

use super::*;
use crate::renderer::framebuilder::FrameData;
use crate::renderer::scene_renderer::RenderContext;
use crate::renderer::transient_pool::{
    TransientRequest, TransientResourcePool, TransientResourceResolver,
};
use std::collections::HashMap;

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
        matches!(
            self,
            Self::ENTITY | Self::DEPTH | Self::HDR | Self::OPAQUE
        )
    }
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
}

struct PassNode<'a> {
    pass: &'a dyn RenderPass,
    reads: Vec<ResourceId>,
    writes: Vec<ResourceId>,
}

#[derive(Clone, Copy)]
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

    pub(crate) fn add_pass<P: RenderPass + 'static>(&mut self, pass: P) {
        self.passes.push(Box::new(pass));
    }

    // -------------------------
    // Build Pass + Resource graph
    // -------------------------
    fn build_graph(&self) -> (Vec<PassNode<'_>>, HashMap<ResourceId, ResourceNode>) {
        let mut resources: HashMap<ResourceId, ResourceNode> = HashMap::new();

        let pass_nodes: Vec<_> = self
            .passes
            .iter()
            .map(|p| PassNode {
                pass: p.as_ref(),
                reads: p.reads().to_vec(),
                writes: p.writes().to_vec(),
            })
            .collect();

        for (i, pass) in pass_nodes.iter().enumerate() {
            // writes
            for &res in &pass.writes {
                let entry = resources.entry(res).or_insert(ResourceNode {
                    id: res,
                    writers: Vec::new(),
                    readers: Vec::new(),
                });

                entry.writers.push(i);
            }

            // reads
            for &res in &pass.reads {
                let entry = resources.entry(res).or_insert(ResourceNode {
                    id: res,
                    writers: Vec::new(),
                    readers: Vec::new(),
                });

                entry.readers.push(i);
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
            // Preserve the declared order for multiple writes to the same resource.
            for writers in res.writers.windows(2) {
                let earlier = writers[0];
                let later = writers[1];
                deps[later].push(Edge {
                    from: later,
                    to: earlier,
                    resource: res.id,
                    kind: EdgeKind::WriterOrder,
                });
            }

            // A reader observes the latest preceding writer. If it is registered
            // before every writer, it observes the first writer instead. In either
            // case, force the reader to finish before the next write so unrelated
            // dependencies cannot move a clobbering write ahead of that read.
            for &reader in &res.readers {
                let preceding_writer = res
                    .writers
                    .iter()
                    .copied()
                    .take_while(|writer| *writer < reader)
                    .last();
                let observed_writer = preceding_writer.or_else(|| res.writers.first().copied());

                if let Some(writer) = observed_writer {
                    if writer != reader {
                        deps[reader].push(Edge {
                            from: reader,
                            to: writer,
                            resource: res.id,
                            kind: EdgeKind::ReadAfterWrite,
                        });
                    }
                }

                if let Some(next_writer) = res.writers.iter().copied().find(|writer| {
                    *writer > reader && Some(*writer) != observed_writer
                }) {
                    deps[next_writer].push(Edge {
                        from: next_writer,
                        to: reader,
                        resource: res.id,
                        kind: EdgeKind::WriteAfterRead,
                    });
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
    pub fn compile(&self) -> Result<Vec<usize>, String> {
        let (_passes, resources) = self.build_graph();

        if let Some(resource_name) = resources
            .values()
            .filter(|resource| !resource.readers.is_empty() && resource.writers.is_empty())
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
    /// to retained pool allocations before execution. RenderContext then checks
    /// each pass's declared resource access while exposing those resolved targets.
    pub(crate) fn compile_lifetimes(
        &self,
    ) -> Result<(Vec<usize>, Vec<ResourceLifetime>), String> {
        let order = self.compile()?;
        let mut execution_index = vec![0; order.len()];
        for (position, &pass_index) in order.iter().enumerate() {
            execution_index[pass_index] = position;
        }

        let mut lifetimes: HashMap<ResourceId, (usize, usize)> = HashMap::new();
        for (pass_index, pass) in self.passes.iter().enumerate() {
            let position = execution_index[pass_index];
            for resource in pass.reads().iter().chain(pass.writes()) {
                lifetimes
                    .entry(*resource)
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
    pub(crate) fn allocate_transient_resources<D, R>(
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

    // -------------------------
    // Execute passes in dependency order
    // -------------------------
    pub(crate) fn execute(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut RenderContext,
        frame: &FrameData,
    ) -> Result<(), String> {
        let order = self.compile()?;
        for idx in order {
            let pass = &mut self.passes[idx];
            ctx.begin_pass(pass.reads(), pass.writes());
            pass.execute(encoder, ctx, frame);
        }
        Ok(())
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

    #[test]
    fn should_run_in_order() {
        let mut graph = RenderGraph::new();
        let meshpass = MeshPass::opaque();
        let skyboxpass = SkyboxPass {};
        let build_mipmaps = BuildMipmapsPass {};
        let transmission = MeshPass::transmission();
        let lightpass = LightsIconPass {};
        let axispass = AxisPass {};
        let bboxpass = LinesPass {};
        let linearizepass = LinearizePass {};
        let outlinepass = OutlinePass {};

        graph.add_pass(ShadowPass {});
        graph.add_pass(meshpass);
        graph.add_pass(skyboxpass);
        graph.add_pass(build_mipmaps);
        graph.add_pass(transmission);
        graph.add_pass(lightpass);
        graph.add_pass(axispass);
        graph.add_pass(bboxpass);
        graph.add_pass(linearizepass);
        graph.add_pass(outlinepass);

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
                        "BoundingboxPass",
                        "LinearizePass",
                        "OutlinePass",
                    ]
                );
            }
            Err(e) => panic!("{}", e),
        }
    }

    #[test]
    fn dependencies_compile_writer_before_reader() {
        struct Writer;
        struct Reader;

        impl RenderPass for Writer {
            fn name(&self) -> &'static str {
                "Writer"
            }

            fn reads(&self) -> &[ResourceId] {
                &[]
            }

            fn writes(&self) -> &[ResourceId] {
                &[ResourceId::HDR]
            }
        }

        impl RenderPass for Reader {
            fn name(&self) -> &'static str {
                "Reader"
            }

            fn reads(&self) -> &[ResourceId] {
                &[ResourceId::HDR]
            }

            fn writes(&self) -> &[ResourceId] {
                &[]
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Reader);
        graph.add_pass(Writer);

        let order = graph.compile_names().unwrap();
        let writer = order.iter().position(|name| *name == "Writer").unwrap();
        let reader = order.iter().position(|name| *name == "Reader").unwrap();

        assert!(
            writer < reader,
            "writer must run before its reader: {order:?}"
        );
    }

    #[test]
    fn dependency_order_is_deterministic() {
        struct Writer(ResourceId, &'static str);
        struct Reader(ResourceId, &'static str);

        impl RenderPass for Writer {
            fn name(&self) -> &'static str {
                self.1
            }

            fn reads(&self) -> &[ResourceId] {
                &[]
            }

            fn writes(&self) -> &[ResourceId] {
                std::slice::from_ref(&self.0)
            }
        }

        impl RenderPass for Reader {
            fn name(&self) -> &'static str {
                self.1
            }

            fn reads(&self) -> &[ResourceId] {
                std::slice::from_ref(&self.0)
            }

            fn writes(&self) -> &[ResourceId] {
                &[]
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Reader(ResourceId::LDR, "Reader LDR"));
        graph.add_pass(Reader(ResourceId::HDR, "Reader HDR"));
        graph.add_pass(Writer(ResourceId::LDR, "Writer LDR"));
        graph.add_pass(Writer(ResourceId::HDR, "Writer HDR"));

        let expected = graph.compile_names().unwrap();
        for _ in 0..32 {
            assert_eq!(graph.compile_names().unwrap(), expected);
        }
    }

    #[test]
    fn multiple_writers_preserve_registration_order() {
        struct Pass {
            name: &'static str,
            reads: Vec<ResourceId>,
            writes: Vec<ResourceId>,
        }

        impl RenderPass for Pass {
            fn name(&self) -> &'static str {
                self.name
            }

            fn reads(&self) -> &[ResourceId] {
                &self.reads
            }

            fn writes(&self) -> &[ResourceId] {
                &self.writes
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Pass {
            name: "First writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });
        graph.add_pass(Pass {
            name: "Second writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });
        graph.add_pass(Pass {
            name: "Reader",
            reads: vec![ResourceId::HDR],
            writes: vec![],
        });

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["First writer", "Second writer", "Reader"]
        );
    }

    #[test]
    fn reader_is_ordered_before_the_next_writer() {
        struct Pass {
            name: &'static str,
            reads: Vec<ResourceId>,
            writes: Vec<ResourceId>,
        }

        impl RenderPass for Pass {
            fn name(&self) -> &'static str { self.name }
            fn reads(&self) -> &[ResourceId] { &self.reads }
            fn writes(&self) -> &[ResourceId] { &self.writes }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Pass {
            name: "First writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });
        graph.add_pass(Pass {
            name: "Reader",
            reads: vec![ResourceId::HDR],
            writes: vec![],
        });
        graph.add_pass(Pass {
            name: "Second writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });

        let (_, resources) = graph.build_graph();
        let dependencies = graph.build_dependencies(&resources);
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
    fn reader_between_writers_observes_the_preceding_write() {
        struct Pass {
            name: &'static str,
            reads: Vec<ResourceId>,
            writes: Vec<ResourceId>,
        }

        impl RenderPass for Pass {
            fn name(&self) -> &'static str {
                self.name
            }
            fn reads(&self) -> &[ResourceId] {
                &self.reads
            }
            fn writes(&self) -> &[ResourceId] {
                &self.writes
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Pass {
            name: "First writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });
        graph.add_pass(Pass {
            name: "Reader",
            reads: vec![ResourceId::HDR],
            writes: vec![],
        });
        graph.add_pass(Pass {
            name: "Second writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });

        assert_eq!(
            graph.compile_names().unwrap(),
            vec!["First writer", "Reader", "Second writer"]
        );
    }

    #[test]
    fn read_without_any_writer_is_reported_as_an_error() {
        struct Reader;
        impl RenderPass for Reader {
            fn name(&self) -> &'static str {
                "Reader"
            }
            fn reads(&self) -> &[ResourceId] {
                &[ResourceId::PICKBUFFER]
            }
            fn writes(&self) -> &[ResourceId] {
                &[]
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Reader);
        assert!(
            graph
                .compile()
                .unwrap_err()
                .contains("PickBuffer is read but never written")
        );
    }

    #[test]
    fn should_find_cycles() {
        struct PassA;
        struct PassB;

        impl RenderPass for PassA {
            fn name(&self) -> &'static str {
                "Pass A"
            }

            fn reads(&self) -> &[ResourceId] {
                &[ResourceId::LDR]
            }
            fn writes(&self) -> &[ResourceId] {
                &[ResourceId::DEPTH]
            }
        }

        impl RenderPass for PassB {
            fn name(&self) -> &'static str {
                "Pass B"
            }
            fn reads(&self) -> &[ResourceId] {
                &[ResourceId::DEPTH]
            }
            fn writes(&self) -> &[ResourceId] {
                &[ResourceId::LDR]
            }
        }

        let mut graph = RenderGraph::new();

        graph.add_pass(PassA);
        graph.add_pass(PassB);

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
            reads: Vec<ResourceId>,
            writes: Vec<ResourceId>,
        }
        impl RenderPass for Pass {
            fn name(&self) -> &'static str { "TestPass" }
            fn reads(&self) -> &[ResourceId] { &self.reads }
            fn writes(&self) -> &[ResourceId] { &self.writes }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Pass { reads: vec![], writes: vec![ResourceId::HDR] });
        graph.add_pass(Pass { reads: vec![ResourceId::HDR], writes: vec![ResourceId::OPAQUE] });
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
    fn transient_allocation_excludes_externally_managed_resources() {
        struct Pass(ResourceId);

        impl RenderPass for Pass {
            fn name(&self) -> &'static str {
                "Resource writer"
            }

            fn reads(&self) -> &[ResourceId] {
                &[]
            }

            fn writes(&self) -> &[ResourceId] {
                std::slice::from_ref(&self.0)
            }
        }

        let mut graph = RenderGraph::new();
        graph.add_pass(Pass(ResourceId::SHADOWMAP));
        graph.add_pass(Pass(ResourceId::HDR));

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
        struct Pass {
            name: &'static str,
            reads: Vec<ResourceId>,
            writes: Vec<ResourceId>,
        }

        impl RenderPass for Pass {
            fn name(&self) -> &'static str {
                self.name
            }
            fn reads(&self) -> &[ResourceId] {
                &self.reads
            }
            fn writes(&self) -> &[ResourceId] {
                &self.writes
            }
        }

        let mut graph = RenderGraph::new();
        // Registered in reverse dependency order to ensure lifetimes are
        // computed from the compiled order rather than registration indices.
        graph.add_pass(Pass {
            name: "Reader",
            reads: vec![ResourceId::HDR],
            writes: vec![ResourceId::LDR],
        });
        graph.add_pass(Pass {
            name: "Writer",
            reads: vec![],
            writes: vec![ResourceId::HDR],
        });

        let (order, lifetimes) = graph.compile_lifetimes().unwrap();
        assert_eq!(order, vec![1, 0]);

        let hdr = lifetimes.iter().find(|item| item.resource == ResourceId::HDR).unwrap();
        assert_eq!((hdr.first_use, hdr.last_use), (0, 1));

        let ldr = lifetimes.iter().find(|item| item.resource == ResourceId::LDR).unwrap();
        assert_eq!((ldr.first_use, ldr.last_use), (1, 1));
    }

}
