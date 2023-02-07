use crate::heap::allocator::Allocator;
use crate::heap::hash_table::HashTable;
use crate::value::Value;
use std::alloc::Layout;
use std::fmt::{Debug, Display, Formatter};
use std::ptr::NonNull;
use std::sync::Arc;
use std::{ptr, slice};

pub mod allocator;
pub mod hash_table;
mod vec;

pub use vec::Vec;

#[derive(Debug)]
pub struct HeapManager {
    known_objects: *mut Object,
    alloc: Arc<Allocator>,
    strings: HashTable,
}

impl HeapManager {
    pub fn new(alloc: Arc<Allocator>, strings: HashTable) -> Self {
        Self {
            known_objects: ptr::null_mut(),
            alloc,
            strings,
        }
    }

    pub fn alloc(&self) -> Arc<Allocator> {
        self.alloc.clone()
    }

    unsafe fn register_object(&mut self, object: NonNull<Object>) {
        let old = self.known_objects;
        self.known_objects = object.as_ptr();
        (*object.as_ptr()).next = old;
    }

    pub fn create_string_copied(&mut self, s: &str) -> BoxedObject {
        let str = ObjString::new_copied(s, self.alloc.clone());
        if let Some(ptr) = self.strings.get_string(NonNull::from(&str)) {
            BoxedObject(ptr.0.cast::<Object>())
        } else {
            let ptr = unsafe { self.move_to_heap(str) };
            self.strings.insert(BoxedObjString(ptr), Value::Nil);
            BoxedObject(ptr.cast::<Object>())
        }
    }

    pub fn create_string_concat(&mut self, a: &BoxedObjString, b: &BoxedObjString) -> BoxedObject {
        let str = ObjString::concat(a.0, b.0);
        if let Some(ptr) = self.strings.get_string(NonNull::from(&str)) {
            BoxedObject(ptr.0.cast::<Object>())
        } else {
            let ptr = unsafe { self.move_to_heap(str) };
            self.strings.insert(BoxedObjString(ptr), Value::Nil);
            BoxedObject(ptr.cast::<Object>())
        }
    }

    pub unsafe fn drop_object(&self, ptr: NonNull<Object>) {
        let (layout, ptr) = match (*ptr.as_ptr()).obj_type {
            ObjType::String => (Layout::new::<ObjString>(), ptr.cast::<ObjString>()),
        };
        ptr.as_ptr().drop_in_place();
        self.alloc.dealloc(ptr.cast::<u8>(), layout);
    }

    unsafe fn move_to_heap<T>(&mut self, object: T) -> NonNull<T> {
        let ptr = self.alloc.allocate(Layout::new::<T>()).cast::<T>();
        ptr.as_ptr().write(object);
        self.register_object(ptr.cast::<Object>());
        ptr
    }
}

impl Drop for HeapManager {
    fn drop(&mut self) {
        unsafe {
            let mut obj = self.known_objects;
            while let Some(ptr) = NonNull::new(obj) {
                let next = (*obj).next;
                self.drop_object(ptr);
                obj = next;
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct BoxedObject(NonNull<Object>);

impl BoxedObject {
    pub fn as_objstring(&self) -> Option<BoxedObjString> {
        Object::as_objstring(self.0)
    }
}

impl Display for BoxedObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Object::to_string(self.0))
    }
}

impl PartialEq for BoxedObject {
    fn eq(&self, other: &Self) -> bool {
        Object::eq(self.0, other.0)
    }
}

pub struct Object {
    next: *mut Object,
    obj_type: ObjType,
    alloc: Arc<Allocator>,
}

enum ObjType {
    String,
}

impl Object {
    fn new(typ: ObjType, alloc: Arc<Allocator>) -> Self {
        Self {
            next: ptr::null_mut(),
            obj_type: typ,
            alloc,
        }
    }

    fn as_objstring(ptr: NonNull<Self>) -> Option<BoxedObjString> {
        unsafe {
            match (*ptr.as_ptr()).obj_type {
                ObjType::String => Some(BoxedObjString(ptr.cast::<ObjString>())),
            }
        }
    }

    fn to_string(ptr: NonNull<Self>) -> String {
        unsafe {
            match (*ptr.as_ptr()).obj_type {
                ObjType::String => (BoxedObjString(ptr.cast::<ObjString>())).to_string(),
            }
        }
    }

    fn eq(a: NonNull<Self>, b: NonNull<Self>) -> bool {
        unsafe {
            match (&(*a.as_ptr()).obj_type, &(*b.as_ptr()).obj_type) {
                (ObjType::String, ObjType::String) => a == b,
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct BoxedObjString(NonNull<ObjString>);

impl BoxedObjString {
    pub fn as_str<'a>(&'a self) -> &'a str {
        ObjString::as_str::<'a>(self.0)
    }
}

impl Display for BoxedObjString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[repr(C)]
pub struct ObjString {
    object: Object,
    internal: ObjStringInternal,
}

struct ObjStringInternal {
    len: usize,
    hash: u32,
    ptr: NonNull<u8>,
}

impl ObjString {
    fn new_copied(s: &str, alloc: Arc<Allocator>) -> Self {
        let len = s.len();
        let str_ptr = if len != 0 {
            unsafe {
                let str_ptr = alloc.allocate(Layout::array::<u8>(len).unwrap());
                ptr::copy(s.as_ptr(), str_ptr.as_ptr(), len);
                str_ptr
            }
        } else {
            NonNull::dangling()
        };

        let hash = Self::make_hash(str_ptr, len);

        Self {
            object: Object::new(ObjType::String, alloc),
            internal: ObjStringInternal {
                len,
                hash,
                ptr: str_ptr,
            },
        }
    }

    pub fn as_str<'a>(ptr: NonNull<Self>) -> &'a str {
        unsafe {
            let slice = slice::from_raw_parts(
                (*ptr.as_ptr()).internal.ptr.as_ptr() as *const _,
                (*ptr.as_ptr()).internal.len,
            );
            std::str::from_utf8_unchecked(slice)
        }
    }

    pub fn hash(ptr: NonNull<Self>) -> u32 {
        unsafe { (*ptr.as_ptr()).internal.hash }
    }

    fn make_hash(chars: NonNull<u8>, len: usize) -> u32 {
        let mut hash = 2166136261;
        for i in 0..len {
            hash ^= unsafe { *chars.as_ptr().add(i) } as u32;
            hash = hash.wrapping_mul(16777619);
        }
        hash
    }

    fn concat(a: NonNull<Self>, b: NonNull<Self>) -> Self {
        let a = unsafe { &*a.as_ptr() };
        let b = unsafe { &*b.as_ptr() };
        let len = a.internal.len + b.internal.len;
        let str_ptr = if len == 0 {
            NonNull::dangling()
        } else {
            unsafe {
                let str_ptr = a.object.alloc.allocate(Layout::array::<u8>(len).unwrap());
                if a.internal.len != 0 {
                    ptr::copy(a.internal.ptr.as_ptr(), str_ptr.as_ptr(), a.internal.len);
                }
                if b.internal.len != 0 {
                    ptr::copy(
                        b.internal.ptr.as_ptr(),
                        str_ptr.as_ptr().add(a.internal.len),
                        b.internal.len,
                    );
                }
                str_ptr
            }
        };
        let hash = Self::make_hash(str_ptr, len);
        Self {
            object: Object::new(ObjType::String, a.object.alloc.clone()),
            internal: ObjStringInternal {
                len,
                hash,
                ptr: str_ptr,
            },
        }
    }
}

impl Drop for ObjString {
    fn drop(&mut self) {
        unsafe {
            let len = self.internal.len;
            if len != 0 {
                self.object
                    .alloc
                    .dealloc(self.internal.ptr, Layout::array::<u8>(len).unwrap());
            }
            self.internal.ptr = NonNull::dangling();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_interning() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc, strings);
        let a = heap_manager.create_string_copied("hi!");
        let b = heap_manager.create_string_copied("hi!");
        let c = heap_manager.create_string_copied("hi!hi!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        let d = heap_manager
            .create_string_concat(&a.as_objstring().unwrap(), &b.as_objstring().unwrap());
        assert_eq!(c, d);
    }
}
