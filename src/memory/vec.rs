use crate::memory::allocator::Allocator;
use std::alloc::Layout;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::Arc;
use std::{mem, ptr};

pub struct VMHeapVec<T> {
    cap: usize,
    len: usize,
    ptr: NonNull<T>,
    alloc: Arc<Allocator>,
}

impl<T> VMHeapVec<T> {
    pub fn new(alloc: Arc<Allocator>) -> Self {
        assert_ne!(mem::size_of::<T>(), 0, "ZSTs not supported");
        Self {
            cap: 0,
            len: 0,
            ptr: NonNull::dangling(),
            alloc,
        }
    }

    pub fn push(&mut self, elem: T) {
        if self.len == self.cap {
            self.grow()
        }

        unsafe {
            self.ptr.as_ptr().add(self.len).write(elem);
        }
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            unsafe { Some(ptr::read(self.ptr.as_ptr().add(self.len))) }
        }
    }

    fn grow(&mut self) {
        let (new_cap, new_layout) = if self.cap == 0 {
            let initial_capacity = 1;
            (
                initial_capacity,
                Layout::array::<T>(initial_capacity).unwrap(),
            )
        } else {
            let new_cap = 2 * self.cap;
            let new_layout = Layout::array::<T>(new_cap).unwrap();
            (new_cap, new_layout)
        };

        let new_ptr = if self.cap == 0 {
            unsafe { self.alloc.allocate(new_layout) }
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            let old_ptr = self.ptr.cast();
            unsafe { self.alloc.realloc(old_ptr, old_layout, new_layout) }
        };
        self.ptr = new_ptr.cast::<T>();
        self.cap = new_cap;
    }
}

impl<T> Drop for VMHeapVec<T> {
    fn drop(&mut self) {
        if self.cap != 0 {
            while self.pop().is_some() {}
            let layout = Layout::array::<T>(self.cap).unwrap();
            unsafe { self.alloc.dealloc(self.ptr.cast::<u8>(), layout) }
        }
    }
}

impl<T> Deref for VMHeapVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for VMHeapVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}
