use log::trace;
use std::alloc::{alloc, dealloc, handle_alloc_error, realloc, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct Allocator {
    allocated: AtomicUsize,
}

impl Allocator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            allocated: AtomicUsize::new(0),
        })
    }

    pub unsafe fn allocate(&self, layout: Layout) -> NonNull<u8> {
        let ptr = alloc(layout);
        match NonNull::new(ptr) {
            Some(ptr) => {
                self.allocated.fetch_add(layout.size(), Ordering::Relaxed);
                trace!(
                    "Allocated {} bytes for a new total of {}",
                    layout.size(),
                    self.allocated.load(Ordering::Relaxed)
                );
                ptr
            }
            None => handle_alloc_error(layout),
        }
    }

    pub unsafe fn realloc(
        &self,
        old_ptr: *mut u8,
        old_layout: Layout,
        new_layout: Layout,
    ) -> NonNull<u8> {
        assert!(
            old_layout.size() < new_layout.size(),
            "Realloc can't shrink allocations"
        );
        let diff = new_layout.size() - old_layout.size();
        let ptr = realloc(old_ptr, old_layout, new_layout.size());
        match NonNull::new(ptr) {
            Some(ptr) => {
                self.allocated.fetch_add(diff, Ordering::Relaxed);
                trace!(
                    "Reallocated {} extra bytes for a new total of {}",
                    diff,
                    self.allocated.load(Ordering::Relaxed)
                );
                ptr
            }
            None => handle_alloc_error(new_layout),
        }
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
