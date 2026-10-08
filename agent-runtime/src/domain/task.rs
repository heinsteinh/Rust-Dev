#[derive(Debug)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug)]
pub struct Task {
    id: u64,
    description: String,
    status: TaskStatus,
}

impl Task {
    pub fn new(id: u64, description: String) -> Self {
        Self {
            id,
            description,
            status: TaskStatus::Pending,
        }
    }

    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
    }

    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
    }

    pub fn status(&self) -> &TaskStatus {
        &self.status
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}
