# Rust RTOS lab (deterministic simulator)

`cargo test` and `cargo run` on a machine with a Rust toolchain installed.

The **library** uses `#![no_std]` and does not require heap allocation. The example executable uses `std` for printing.

## Learn the components
- `Task` with priority, simulated context, inline stack, sleep/blocked/finished states.
- `Kernel::tick`: increments simulated time, wakes sleepers, records deadline misses, schedules highest-priority ready task (lower TaskId on ties).
- `MemoryPool`: eight fixed 32-byte blocks with zero-on-allocation and validation.
- `Ring` and `MessageQueue`: bounded FIFO; empty reads can block a task; writes wake a waiter.
- `RtosMutex`: owner tracking and a **simplified** priority inheritance demonstration. Tests check inheritance and restoration.

## Critical scope and safety limitations
This is **not a deployable/preemptive RTOS**. A `Task`'s stack and `Context` fields do not participate in CPU register saving or task execution. Each task is a Rust step function, and the program counter is an application state-machine index. The library does not configure interrupt vectors, timers, a linker script, MPU, `PendSV`, or architecture-specific switching. `now` is a virtual tick counter, not `core::time::Instant` or wall-clock time. Higher numeric `priority` runs first.

This example's mutex uses safe direct ownership checks; it does not implement an interrupt/SMP-safe `UnsafeCell` lock. Priority inheritance only handles one level of contention and simply boosts the current owner. For a real RTOS, you would need transitive inheritance, multiple owners/locks, priority-ordered waiters, deadlock and cancellation handling. Queue waiter ordering is FIFO; there is no wait timeout or cancellation. Scheduler is cooperative, with no round-robin or budget, and can starve lower-priority tasks. Pool block IDs have no generations, so stale IDs could reference a reallocated block; introduce generation-tagged handles for stronger isolation. All tasks are assumed to obey their step-function contract and may not block OS threads. Deadline failures are recorded as a counter rather than aborting scheduling.

## Exercises
1. Add a task that sleeps for two ticks, and verify wake-up exactly at the requested tick.
2. Add priority-sorted queue waiters.
3. Add block-handle generations to the memory pool.
4. Add a deadline-miss event queue with bounded capacity and `Deadline` records.
5. Replace state-machine `step` functions with a target-specific task context switch on QEMU Cortex-M, after separately auditing memory and interrupt safety.
