#[allow(dead_code)]
#[derive(Default)]
pub struct GpuInternalCounters {
    pub textures: GpuResourceStats,
    pub materials: GpuResourceStats,
    pub meshes: GpuResourceStats,
    pub shadows: GpuResourceStats,
    pub ibl: GpuResourceStats,
}

#[allow(dead_code)]
pub trait HasGpuStats {
    fn get_stats(&self) -> GpuResourceStats;
}

#[derive(Default, Debug, Clone)]
pub struct GpuResourceStats {
    pub count: usize,
    pub estimated_bytes: usize,
}

impl GpuResourceStats {
    pub fn add(&mut self, size: usize) -> &mut Self {
        self.estimated_bytes += size;
        self.count += 1;
        self
    }
    pub fn remove(&mut self, size: usize) -> &mut Self {
        if self.count > 0 {
            let result = self.estimated_bytes.saturating_sub(size);
            self.estimated_bytes = result;
            self.count -= 1;
        }
        self
    }
}
