use rust_rtos_lab::{Kernel, RTOSError, TaskId};

// A task is a resumable state machine. Context.pc is the current phase.
fn sensor(kernel: &mut Kernel, id: TaskId) -> Result<(), RTOSError> {
    let counter = kernel.task(id)?.context.pc;
    if counter >= 3 { return kernel.finish(id); }
    let value = 20 + counter;
    kernel.send(value)?;
    println!("tick {:02}: sensor -> queue: {}", kernel.now, value);
    kernel.task_mut(id)?.context.pc += 1;
    kernel.sleep(id, 3)
}

fn controller(kernel: &mut Kernel, id: TaskId) -> Result<(), RTOSError> {
    if kernel.task(id)?.context.pc >= 3 { return kernel.finish(id); }
    match kernel.receive_or_block(id)? {
        Some(value) => {
            println!("tick {:02}: controller <- queue: {}", kernel.now, value);
            kernel.task_mut(id)?.context.pc += 1;
        }
        None => println!("tick {:02}: controller blocked, queue empty", kernel.now),
    }
    Ok(())
}

fn logger(kernel: &mut Kernel, id: TaskId) -> Result<(), RTOSError> {
    let count = kernel.task(id)?.context.pc;
    if count >= 2 { return kernel.finish(id); }
    if kernel.lock(id)? {
        *kernel.shared.get_mut(id)? += 1;
        println!("tick {:02}: logger shared counter {}", kernel.now, *kernel.shared.get_mut(id)?);
        kernel.unlock(id)?;
        kernel.task_mut(id)?.context.pc += 1;
        kernel.sleep(id, 5)?;
    }
    Ok(())
}

fn main() -> Result<(), RTOSError> {
    let mut kernel = Kernel::new();
    let sensor_id = kernel.spawn(2, sensor)?;
    let controller_id = kernel.spawn(5, controller)?;
    let _logger_id = kernel.spawn(1, logger)?;
    kernel.set_deadline(sensor_id, 30)?;
    kernel.set_deadline(controller_id, 30)?;

    println!("Rust RTOS Lab: cooperative simulation, one task step per tick");
    for _ in 0..30 { kernel.tick()?; }
    println!("Simulation complete: deadline misses = {}", kernel.deadline_misses);
    Ok(())
}
