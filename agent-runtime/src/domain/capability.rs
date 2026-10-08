#[derive(Debug, Clone)]
pub struct Capability {
    name: String,
}

impl Capability {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
