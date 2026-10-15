#![no_std]

pub const MAX_TASKS: usize = 8;
pub const STACK_SIZE: usize = 256;
pub const BLOCK_SIZE: usize = 32;
pub const BLOCK_COUNT: usize = 8;
pub const POOL_SIZE: usize = BLOCK_SIZE * BLOCK_COUNT;
pub type TaskId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RTOSError {
    TaskCreationFailed,
    SchedulerOverflow,
    DeadlineMissed,
    ResourceUnavailable,
    InvalidTask,
    NotOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState { Ready, Sleeping(u64), Blocked, Finished }

// These are *simulated* register contents, NOT a hardware CPU context.
#[derive(Debug, Default, Clone, Copy)]
pub struct Context { pub pc: u32, pub registers: [u32; 8], pub stack_pointer_offset: usize }

pub type StepFn = fn(&mut Kernel, TaskId) -> Result<(), RTOSError>;

pub struct Task {
    pub priority: u8,
    pub base_priority: u8,
    pub state: TaskState,
    pub context: Context,
    pub deadline: Option<u64>,
    pub stack: [u8; STACK_SIZE],
    pub step: StepFn,
}

impl Task {
    fn new(priority: u8, step: StepFn) -> Self {
        Self { priority, base_priority: priority, state: TaskState::Ready,
               context: Context { stack_pointer_offset: STACK_SIZE, ..Context::default() },
               deadline: None, stack: [0; STACK_SIZE], step }
    }
}

pub struct MemoryPool {
    pool: [u8; POOL_SIZE],
    allocation_table: [bool; BLOCK_COUNT],
}

impl MemoryPool {
    pub const fn new() -> Self { Self { pool: [0; POOL_SIZE], allocation_table: [false; BLOCK_COUNT] } }
    pub fn allocate(&mut self) -> Result<usize, RTOSError> {
        for i in 0..BLOCK_COUNT {
            if !self.allocation_table[i] {
                self.allocation_table[i] = true;
                self.pool[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE].fill(0);
                return Ok(i);
            }
        }
        Err(RTOSError::ResourceUnavailable)
    }
    pub fn block_mut(&mut self, id: usize) -> Result<&mut [u8], RTOSError> {
        if id >= BLOCK_COUNT || !self.allocation_table[id] { return Err(RTOSError::ResourceUnavailable); }
        Ok(&mut self.pool[id * BLOCK_SIZE..(id + 1) * BLOCK_SIZE])
    }
    pub fn deallocate(&mut self, id: usize) -> Result<(), RTOSError> {
        if id >= BLOCK_COUNT || !self.allocation_table[id] { return Err(RTOSError::ResourceUnavailable); }
        self.allocation_table[id] = false;
        Ok(())
    }
    pub fn free_count(&self) -> usize { self.allocation_table.iter().filter(|&&b| !b).count() }
}
impl Default for MemoryPool { fn default() -> Self { Self::new() } }

pub struct Ring<T, const N: usize> {
    slots: [Option<T>; N], head: usize, len: usize,
}
impl<T, const N: usize> Ring<T, N> {
    pub fn new() -> Self { Self { slots: core::array::from_fn(|_| None), head: 0, len: 0 } }
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if N == 0 || self.len == N { return Err(value); }
        let tail = (self.head + self.len) % N;
        self.slots[tail] = Some(value); self.len += 1; Ok(())
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 { return None; }
        let result = self.slots[self.head].take();
        self.head = (self.head + 1) % N; self.len -= 1; result
    }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
}
impl<T, const N: usize> Default for Ring<T, N> { fn default() -> Self { Self::new() } }

pub struct MessageQueue<T, const N: usize> {
    buffer: Ring<T, N>,
    waiting_tasks: Ring<TaskId, MAX_TASKS>,
}
impl<T, const N: usize> MessageQueue<T, N> {
    pub fn new() -> Self { Self { buffer: Ring::new(), waiting_tasks: Ring::new() } }
    pub fn send(&mut self, message: T) -> Result<Option<TaskId>, RTOSError> {
        self.buffer.push(message).map_err(|_| RTOSError::ResourceUnavailable)?;
        Ok(self.waiting_tasks.pop())
    }
    pub fn receive(&mut self) -> Option<T> { self.buffer.pop() }
    pub fn wait(&mut self, task: TaskId) -> Result<(), RTOSError> {
        self.waiting_tasks.push(task).map_err(|_| RTOSError::SchedulerOverflow)
    }
    pub fn len(&self) -> usize { self.buffer.len() }
}
impl<T, const N: usize> Default for MessageQueue<T, N> { fn default() -> Self { Self::new() } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline { pub absolute_time: u64, pub task_id: TaskId }

// Mutex synchronization is simulated: each step is atomic relative to other steps.
// This is NOT an SMP/interrupt-safe mutex and does not expose Unsafecell<T>.
pub struct RtosMutex<T> {
    data: T,
    owner: Option<TaskId>,
    original_priority: Option<u8>,
    waiting: Ring<TaskId, MAX_TASKS>,
}
impl<T> RtosMutex<T> {
    pub fn new(data: T) -> Self {
        Self { data, owner: None, original_priority: None, waiting: Ring::new() }
    }
    pub fn owner(&self) -> Option<TaskId> { self.owner }
    pub fn try_lock(&mut self, task: TaskId, priority: u8) -> Result<bool, RTOSError> {
        match self.owner {
            None => { self.owner = Some(task); self.original_priority = Some(priority); Ok(true) }
            Some(id) if id == task => Err(RTOSError::ResourceUnavailable),
            _ => { self.waiting.push(task).map_err(|_| RTOSError::SchedulerOverflow)?; Ok(false) }
        }
    }
    pub fn get_mut(&mut self, task: TaskId) -> Result<&mut T, RTOSError> {
        if self.owner == Some(task) { Ok(&mut self.data) } else { Err(RTOSError::NotOwner) }
    }
    pub fn unlock(&mut self, task: TaskId) -> Result<Option<TaskId>, RTOSError> {
        if self.owner != Some(task) { return Err(RTOSError::NotOwner); }
        self.owner = None; self.original_priority = None;
        Ok(self.waiting.pop())
    }
}

pub struct Kernel {
    tasks: [Option<Task>; MAX_TASKS],
    pub now: u64,
    pub pool: MemoryPool,
    pub messages: MessageQueue<u32, 4>,
    pub shared: RtosMutex<u32>,
    pub deadline_misses: usize,
}
impl Kernel {
    pub fn new() -> Self {
        Self { tasks: core::array::from_fn(|_| None), now: 0,
            pool: MemoryPool::new(), messages: MessageQueue::new(),
            shared: RtosMutex::new(0), deadline_misses: 0 }
    }
    pub fn spawn(&mut self, priority: u8, step: StepFn) -> Result<TaskId, RTOSError> {
        for i in 0..MAX_TASKS {
            if self.tasks[i].is_none() { self.tasks[i] = Some(Task::new(priority, step)); return Ok(i); }
        }
        Err(RTOSError::TaskCreationFailed)
    }
    pub fn task(&self, id: TaskId) -> Result<&Task, RTOSError> {
        self.tasks.get(id).and_then(Option::as_ref).ok_or(RTOSError::InvalidTask)
    }
    pub fn task_mut(&mut self, id: TaskId) -> Result<&mut Task, RTOSError> {
        self.tasks.get_mut(id).and_then(Option::as_mut).ok_or(RTOSError::InvalidTask)
    }
    pub fn sleep(&mut self, id: TaskId, ticks: u64) -> Result<(), RTOSError> {
        let wake = self.now.saturating_add(ticks);
        self.task_mut(id)?.state = TaskState::Sleeping(wake); Ok(())
    }
    pub fn finish(&mut self, id: TaskId) -> Result<(), RTOSError> {
        self.task_mut(id)?.state = TaskState::Finished; Ok(())
    }
    pub fn set_deadline(&mut self, id: TaskId, at: u64) -> Result<(), RTOSError> {
        self.task_mut(id)?.deadline = Some(at); Ok(())
    }
    pub fn send(&mut self, value: u32) -> Result<(), RTOSError> {
        if let Some(wakeup) = self.messages.send(value)? { self.task_mut(wakeup)?.state = TaskState::Ready; }
        Ok(())
    }
    pub fn receive_or_block(&mut self, id: TaskId) -> Result<Option<u32>, RTOSError> {
        if let Some(value) = self.messages.receive() { return Ok(Some(value)); }
        self.messages.wait(id)?;
        self.task_mut(id)?.state = TaskState::Blocked;
        Ok(None)
    }
    pub fn lock(&mut self, id: TaskId) -> Result<bool, RTOSError> {
        let priority = self.task(id)?.priority;
        let owner = self.shared.owner();
        if self.shared.try_lock(id, priority)? { return Ok(true); }
        self.task_mut(id)?.state = TaskState::Blocked;
        if let Some(owner_id) = owner {
            let owner_task = self.task_mut(owner_id)?;
            owner_task.priority = owner_task.priority.max(priority); // inheritance
        }
        Ok(false)
    }
    pub fn unlock(&mut self, id: TaskId) -> Result<(), RTOSError> {
        let waiter = self.shared.unlock(id)?;
        let owner = self.task_mut(id)?;
        owner.priority = owner.base_priority;
        if let Some(waiting_id) = waiter { self.task_mut(waiting_id)?.state = TaskState::Ready; }
        Ok(())
    }
    pub fn tick(&mut self) -> Result<Option<TaskId>, RTOSError> {
        self.now = self.now.saturating_add(1);
        for task in self.tasks.iter_mut().flatten() {
            if let TaskState::Sleeping(until) = task.state {
                if self.now >= until { task.state = TaskState::Ready; }
            }
            if let Some(deadline) = task.deadline {
                if self.now > deadline && task.state != TaskState::Finished {
                    self.deadline_misses += 1;
                    task.deadline = None; // report a miss only once
                }
            }
        }
        let selected = self.tasks.iter().enumerate()
            .filter_map(|(id, task)| task.as_ref().filter(|t| t.state == TaskState::Ready).map(|t| (id, t.priority)))
            .max_by_key(|(id, priority)| (*priority, core::cmp::Reverse(*id)))
            .map(|(id, _)| id);
        if let Some(id) = selected {
            let step = self.task(id)?.step;
            step(self, id)?;
        }
        Ok(selected)
    }
}
impl Default for Kernel { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    fn done(k: &mut Kernel, id: TaskId) -> Result<(), RTOSError> { k.finish(id) }
    #[test] fn pool_allocation_and_exhaustion() {
        let mut p = MemoryPool::new();
        let ids: [usize; BLOCK_COUNT] = core::array::from_fn(|_| p.allocate().unwrap());
        assert_eq!(p.allocate(), Err(RTOSError::ResourceUnavailable));
        p.block_mut(ids[0]).unwrap()[0] = 7;
        p.deallocate(ids[0]).unwrap();
        assert_eq!(p.allocate(), Ok(ids[0]));
        assert_eq!(p.block_mut(ids[0]).unwrap()[0], 0);
    }
    #[test] fn ring_wraparound_and_overflow() {
        let mut r = Ring::<u8, 2>::new();
        assert_eq!(r.push(1), Ok(())); assert_eq!(r.push(2), Ok(()));
        assert_eq!(r.push(3), Err(3)); assert_eq!(r.pop(), Some(1));
        r.push(4).unwrap(); assert_eq!(r.pop(), Some(2)); assert_eq!(r.pop(), Some(4));
    }
    #[test] fn scheduler_chooses_highest_priority() {
        let mut k = Kernel::new(); let low = k.spawn(1, done).unwrap(); let high = k.spawn(9, done).unwrap();
        assert_eq!(k.tick(), Ok(Some(high))); assert_eq!(k.tick(), Ok(Some(low)));
    }
    #[test] fn queue_wakes_blocked_receiver() {
        let mut k = Kernel::new(); let id = k.spawn(2, done).unwrap();
        assert_eq!(k.receive_or_block(id), Ok(None)); assert_eq!(k.task(id).unwrap().state, TaskState::Blocked);
        k.send(42).unwrap(); assert_eq!(k.task(id).unwrap().state, TaskState::Ready);
        assert_eq!(k.receive_or_block(id), Ok(Some(42)));
    }
    #[test] fn priority_inheritance_and_release() {
        let mut k = Kernel::new();
        let low = k.spawn(1, done).unwrap(); let high = k.spawn(9, done).unwrap();
        assert!(k.lock(low).unwrap()); assert!(!k.lock(high).unwrap());
        assert_eq!(k.task(low).unwrap().priority, 9);
        assert_eq!(k.task(high).unwrap().state, TaskState::Blocked);
        k.unlock(low).unwrap();
        assert_eq!(k.task(low).unwrap().priority, 1);
        assert_eq!(k.task(high).unwrap().state, TaskState::Ready);
        assert!(k.lock(high).unwrap());
    }
    #[test] fn only_owner_can_access_mutex() {
        let mut k = Kernel::new(); let a = k.spawn(1, done).unwrap(); let b = k.spawn(2, done).unwrap();
        assert!(k.lock(a).unwrap()); *k.shared.get_mut(a).unwrap() = 100;
        assert!(matches!(k.shared.get_mut(b), Err(RTOSError::NotOwner)));
        assert_eq!(k.unlock(b), Err(RTOSError::NotOwner)); k.unlock(a).unwrap();
    }
    #[test] fn deadline_miss_reported_once() {
        let mut k = Kernel::new(); let id = k.spawn(1, |_k, _id| Ok(())).unwrap();
        k.set_deadline(id, 1).unwrap(); k.tick().unwrap(); k.tick().unwrap(); k.tick().unwrap();
        assert_eq!(k.deadline_misses, 1);
    }
    #[test] fn task_capacity() {
        let mut k = Kernel::new();
        for _ in 0..MAX_TASKS { k.spawn(1, done).unwrap(); }
        assert_eq!(k.spawn(1, done), Err(RTOSError::TaskCreationFailed));
    }
}
