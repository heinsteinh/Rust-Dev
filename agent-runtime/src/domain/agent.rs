use super::capability::Capability;

#[derive(Debug)]
pub struct Agent {
    id: u64,
    name: String,
    capabilities: Vec<Capability>,
}

impl Agent {
    pub fn new(id: u64, name: String) -> Self {
        Agent {
            id,
            name,
            capabilities: Vec::new(),
        }
    }

    pub fn add_capability(&mut self, capability: Capability) {
        self.capabilities.push(capability);
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }
}
