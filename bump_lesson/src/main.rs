use bump_lesson::BumpArena;
use std::sync::Mutex;

static ARENA: Mutex<BumpArena<1024>> = Mutex::new(BumpArena::new());

fn main() {
    let mut arena = ARENA.lock().unwrap();

    {
        let bytes = arena.alloc_bytes(16, 8).unwrap();
        bytes.copy_from_slice(&[0xAB; 16]);

        println!("first address: {:p}", bytes.as_ptr());
        println!("first byte: {:X}", bytes[0]);
    }

    {
        let bytes = arena.alloc_bytes(32, 16).unwrap();
        println!("second address: {:p}", bytes.as_ptr());
    }

    println!("used: {} bytes", arena.used());

    arena.reset();

    println!("after reset: {} bytes", arena.used());
}
