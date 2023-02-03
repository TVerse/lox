use log::trace;
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::fmt::{Display, Formatter};
use std::path::Component::Prefix;
use std::{ptr, slice};

pub struct MemoryManager {
    allocated: usize,
    known_objects: *mut Object,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            allocated: 0,
            known_objects: ptr::null_mut(),
        }
    }

    pub unsafe fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let ptr = alloc(layout);
        if ptr.is_null() {
            handle_alloc_error(layout)
        }
        self.allocated += layout.size();
        trace!(
            "Allocated {} bytes for a new total of {}",
            layout.size(),
            self.allocated
        );
        ptr
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        self.allocated -= layout.size();
        dealloc(ptr, layout);
        trace!(
            "Deallocated {} bytes for a new total of {}",
            layout.size(),
            self.allocated
        );
    }
}

impl Drop for MemoryManager {
    fn drop(&mut self) {
        unsafe {
            let mut obj = self.known_objects;
            while !obj.is_null() {
                let next = (*obj).next;
                Object::free(obj, self);
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
    String(*mut ObjString),
}

impl Object {
    pub fn new_str(s: &str, mm: &mut MemoryManager) -> *mut Self {
        let inner = Inner::String(ObjString::new_copied(s, mm) as *mut _);
        Self::new(inner, mm)
    }

    fn new(inner: Inner, mm: &mut MemoryManager) -> *mut Self {
        let self_ptr = unsafe {
            let self_ptr = mm.allocate(Layout::new::<Self>()) as *mut Self;
            self_ptr.write(Self {
                inner,
                next: mm.known_objects,
            });
            self_ptr
        };
        mm.known_objects = self_ptr;

        self_ptr
    }

    pub unsafe fn free(ptr: *mut Self, mm: &mut MemoryManager) {
        match (*ptr).inner {
            Inner::String(ptr) => ObjString::free(ptr as *const _, mm),
        }
        mm.dealloc(ptr as *mut _, Layout::new::<Self>())
    }

    pub fn as_str(&self) -> Option<&ObjString> {
        match self.inner {
            Inner::String(ptr) => Some(unsafe { &*ptr }),
        }
    }

    pub fn new_str_concat(a: &ObjString, b: &ObjString, mm: &mut MemoryManager) -> *mut Self {
        let inner = Inner::String(ObjString::new_concat(a, b, mm) as *mut _);
        Self::new(inner, mm)
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self.inner {
            Inner::String(ptr) => unsafe { (*ptr).to_string() },
        };
        write!(f, "{}", str)
    }
}

pub struct ObjString {
    len: usize,
    ptr: *const u8,
}

impl ObjString {
    fn new_copied(s: &str, mm: &mut MemoryManager) -> *const Self {
        let len = s.len();
        let str_ptr = unsafe {
            let str_ptr = mm.allocate(Layout::array::<u8>(len).unwrap());
            ptr::copy(s.as_ptr(), str_ptr, len);
            str_ptr as *const _
        };

        unsafe {
            let self_ptr = Self::new(mm);
            self_ptr.write(Self { len, ptr: str_ptr });
            self_ptr
        }
    }

    unsafe fn new(mm: &mut MemoryManager) -> *mut Self {
        mm.allocate(Layout::new::<Self>()) as *mut Self
    }

    fn as_str(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.ptr, self.len);
            std::str::from_utf8_unchecked(slice)
        }
    }

    unsafe fn free(ptr: *const Self, mm: &mut MemoryManager) {
        let len = (*ptr).len;
        mm.dealloc((*ptr).ptr as *mut _, Layout::array::<u8>(len).unwrap());

        mm.dealloc(ptr as *mut _, Layout::new::<Self>())
    }

    fn new_concat(a: &ObjString, b: &ObjString, mm: &mut MemoryManager) -> *const Self {
        let len = a.len + b.len;
        let str_ptr = unsafe {
            let str_ptr = mm.allocate(Layout::array::<u8>(len).unwrap());
            ptr::copy(a.ptr, str_ptr, a.len);
            ptr::copy(b.ptr, str_ptr.add(a.len), b.len);
            str_ptr
        };
        unsafe {
            let self_ptr = Self::new(mm);
            self_ptr.write(Self { len, ptr: str_ptr });
            self_ptr
        }
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

impl Display for ObjString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
