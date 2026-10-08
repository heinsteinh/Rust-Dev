mod domain;

use domain::agent::Agent;
use domain::capability::Capability;
use domain::task::Task;

fn main() {
    println!("=== Agent Runtime ===");

    let mut agent = Agent::new(1, "Developer Agent".to_string());

    agent.add_capability(Capability::new("rust".to_string()));

    agent.add_capability(Capability::new("git".to_string()));

    println!("Agent: {}", agent.name());

    println!("Capabilities: {:?}", agent.capabilities());

    let mut task = Task::new(1, "Implement repository pattern".to_string());

    println!("Task: {:?}", task);

    task.start();

    println!("Task after start: {:?}", task);

    task.complete();

    println!("Task after completion: {:?}", task);
}
