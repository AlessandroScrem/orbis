#[derive(Default, Debug, Clone)]
pub struct ResourceStats {
    pub count: usize,
    pub shared: usize,
    pub estimated_bytes: usize,
}

impl ResourceStats {
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
    #[allow(unused)]
    pub fn add_shared(&mut self) -> &mut Self {
        self.shared += 1;
        self
    }

    #[allow(unused)]
    pub fn remove_sahred(&mut self) -> &mut Self {
        if self.shared > 0 {
            self.shared -= 1;
        }
        self
    }
}
