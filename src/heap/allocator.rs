use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use log::trace;

pub struct Allocator {
    allocated: AtomicUsize,
}

impl Allocator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            allocated: AtomicUsize::new(0),
        })
    }

    pub unsafe fn allocate(&self, layout: Layout) -> *mut u8 {
        let ptr = alloc(layout);
        if ptr.is_null() {
            handle_alloc_error(layout)
        }
        self.allocated.fetch_add(layout.size(), Ordering::Relaxed);
        trace!(
            "Allocated {} bytes for a new total of {}",
            layout.size(),
            self.allocated.load(Ordering::Relaxed)
        );
        ptr
    }

    pub unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.allocated.fetch_sub(layout.size(), Ordering::Relaxed);
        dealloc(ptr, layout);
        trace!(
            "Deallocated {} bytes for a new total of {}",
            layout.size(),
            self.allocated.load(Ordering::Relaxed)
        );
    }
}

