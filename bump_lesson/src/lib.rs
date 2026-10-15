#![no_std]

// Ensure the buffer starts on a 64-byte boundary.
#[repr(align(64))]
struct AlignedBuffer<const N: usize>([u8; N]);

pub struct BumpArena<const N: usize> {
    buffer: AlignedBuffer<N>,
    next: usize,
}

impl<const N: usize> BumpArena<N> {
    pub const fn new() -> Self {
        Self {
            buffer: AlignedBuffer([0; N]),
            next: 0,
        }
    }

    pub fn alloc_bytes(&mut self, size: usize, align: usize) -> Option<&mut [u8]> {
        // Require a nonzero allocation and power-of-two alignment.
        if size == 0 || align == 0 || !align.is_power_of_two() {
            return None;
        }

        // Actual address of our RAM buffer.
        let base = self.buffer.0.as_mut_ptr() as usize;
        let current = base.checked_add(self.next)?;

        // Round the current address up to the requested alignment.
        let padding = (align - (current % align)) % align;
        let start = self.next.checked_add(padding)?;
        let end = start.checked_add(size)?;

        // Reject allocations that exceed the reserved RAM.
        if end > N {
            return None;
        }

        // Bump the allocation cursor forward.
        self.next = end;

        // Return a safe, mutable view of the allocated bytes.
        Some(&mut self.buffer.0[start..end])
    }

    pub fn used(&self) -> usize {
        self.next
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub fn reset(&mut self) {
        self.next = 0;
    }
}

impl<const N: usize> Default for BumpArena<N> {
    fn default() -> Self {
        Self::new()
    }
}
