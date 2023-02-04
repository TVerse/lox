use log::trace;
use std::fmt::{Display, Formatter};
use std::{ptr, slice};
use std::alloc::Layout;
use std::sync::Arc;
use crate::heap::allocator::Allocator;

pub mod allocator;

pub struct HeapManager {
    known_objects: *mut Object,
    alloc: Arc<Allocator>,
}

impl HeapManager {
    pub fn new(alloc: Arc<Allocator>) -> Self {
        Self {
            known_objects: ptr::null_mut(),
            alloc,
        }
    }

    unsafe fn register_object(&mut self, object: *mut Object) {
        (*object).next = self.known_objects;
        self.known_objects = object;
    }

    pub fn create_string_copied(&mut self, s: &str) -> *mut Object {
        let inner = Inner::String(ObjString::new_copied(s, self.alloc.clone()));
        let object = Object::new(inner);
        let ptr = unsafe { self.move_object_to_heap(object) };
        ptr
    }

    pub fn create_string_concat(&mut self, a: &ObjString, b: &ObjString) -> *mut Object {
        let inner = Inner::String(a.concat(b));
        let object = Object::new(inner);
        let ptr = unsafe { self.move_object_to_heap(object) };
        ptr
    }

    pub unsafe fn drop_object(&self, ptr: *mut Object) {
        ptr.drop_in_place();
        self.alloc.dealloc(next as *mut u8, Layout::new::<Object>());
    }

    unsafe fn move_object_to_heap(&mut self, object: Object) -> *mut Object {
        let ptr = self.alloc.allocate(Layout::new::<Object>()) as *mut Object;
        ptr.write(object);
        self.register_object(ptr);
        ptr
    }
}

impl Drop for HeapManager {
    fn drop(&mut self) {
        unsafe {
            let mut obj = self.known_objects;
            while !obj.is_null() {
                let next = (*obj).next;
                self.drop_object(next);
                obj = next;
            }
        }
    }
}

pub struct Object {
    inner: Inner,
    next: *mut Object,
}

enum Inner {
    String(ObjString),
}

impl Object {
    fn new(inner: Inner) -> Self {
        Self {
            inner,
            next: ptr::null_mut(),
        }
    }

    pub fn as_str(&self) -> Option<&ObjString> {
        match &self.inner {
            Inner::String(ptr) => Some(unsafe { &**ptr }),
        }
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match &self.inner {
            Inner::String(ptr) => unsafe { (*ptr).to_string() },
        };
        write!(f, "{}", str)
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        match (&self.inner, &other.inner) {
            (Inner::String(ptra), Inner::String(ptrb)) => unsafe {
                let a = &**ptra;
                let b = &**ptrb;
                a.as_str() == b.as_str()
            }, // _ => false
        }
    }
}

pub struct ObjString {
    len: usize,
    hash: u32,
    ptr: *const u8,
    alloc: Arc<Allocator>,
}

impl ObjString {
    fn new_copied(s: &str, alloc: Arc<Allocator>) -> Self {
        let len = s.len();
        let str_ptr = unsafe {
            let str_ptr = alloc.allocate(Layout::array::<u8>(len).unwrap());
            ptr::copy(s.as_ptr(), str_ptr, len);
            str_ptr as *const _
        };

        let hash = Self::make_hash(str_ptr, len);

        Self {
            len,
            hash,
            ptr: str_ptr,
            alloc,
        }
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.ptr, self.len);
            std::str::from_utf8_unchecked(slice)
        }
    }

    pub fn hash(&self) -> u32 {
        self.hash
    }

    fn make_hash(chars: *const u8, len: usize) -> u32 {
        let mut hash = 2166136261;
        for i in 0..len {
            hash ^= unsafe { *chars.add(i) } as u32;
            hash = hash.wrapping_mul(16777619);
        }
        hash
    }

    fn concat(&self, other: &ObjString) -> Self {
        let len = self.len + other.len;
        let str_ptr = unsafe {
            let str_ptr = self.alloc.allocate(Layout::array::<u8>(len).unwrap());
            ptr::copy(self.ptr, str_ptr, self.len);
            ptr::copy(other.ptr, str_ptr.add(other.len), other.len);
            str_ptr
        };
        let hash = Self::make_hash(str_ptr, len);
        Self {
            len,
            hash,
            ptr: str_ptr,
            alloc: self.alloc.clone(),
        }
    }
}

impl Drop for ObjString {
    fn drop(&mut self) {
        unsafe {
            let len = self.len;
            self.alloc.dealloc(self.ptr as *mut _, Layout::array::<u8>(len).unwrap());
            self.ptr = ptr::null_mut();
        }
    }
}

impl Display for ObjString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
