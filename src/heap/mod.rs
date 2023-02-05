use crate::heap::allocator::Allocator;
use crate::heap::hash_table::HashTable;
use crate::value::Value;
use std::alloc::Layout;
use std::fmt::{Debug, Display, Formatter};
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

    unsafe fn register_object(&mut self, object: *mut Object) {
        let old = self.known_objects;
        self.known_objects = object;
        (*object).next = old;
    }

    pub fn create_string_copied(&mut self, s: &str) -> BoxedObject {
        let str = ObjString::new_copied(s, self.alloc.clone());
        if let Some(ptr) = self.strings.get_string(&str) {
            BoxedObject(ptr as *mut _)
        } else {
            let ptr = unsafe { self.move_to_heap(str) };
            self.strings.insert(ptr as *const _, Value::Nil);
            BoxedObject(ptr)
        }
    }

    pub fn create_string_concat(&mut self, a: &ObjString, b: &ObjString) -> BoxedObject {
        let str = a.concat(b);
        if let Some(ptr) = self.strings.get_string(&str) {
            BoxedObject(ptr as *mut _)
        } else {
            let ptr = unsafe { self.move_to_heap(str) };
            self.strings.insert(ptr as *const _, Value::Nil);
            BoxedObject(ptr)
        }
    }

    pub unsafe fn drop_object(&self, ptr: *mut Object) {
        let (layout, ptr) = match (*ptr).obj_type {
            ObjType::String => (Layout::new::<ObjString>(), ptr as *mut ObjString),
        };
        ptr.drop_in_place();
        self.alloc.dealloc(ptr as *mut u8, layout);
    }

    unsafe fn move_to_heap<T>(&mut self, object: T) -> *mut Object {
        let ptr = self.alloc.allocate(Layout::new::<T>()).as_ptr() as *mut T;
        ptr.write(object);
        let ptr = ptr as *mut Object;
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
                self.drop_object(obj);
                obj = next;
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct BoxedObject(*mut Object);

impl BoxedObject {
    pub fn as_objstring(&self) -> Option<*const ObjString> {
        Object::as_objstring(self.0 as *const _)
    }
}

impl Display for BoxedObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Object::to_string(self.0 as *const _))
    }
}

impl PartialEq for BoxedObject {
    fn eq(&self, other: &Self) -> bool {
        Object::eq(self.0 as *const _, other.0 as *const _)
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

    fn as_objstring(ptr: *const Self) -> Option<*const ObjString> {
        unsafe {
            match (*ptr).obj_type {
                ObjType::String => Some(ptr as *const ObjString),
            }
        }
    }

    fn to_string(ptr: *const Self) -> String {
        unsafe {
            match (*ptr).obj_type {
                ObjType::String => (*(ptr as *const ObjString)).to_string(),
            }
        }
    }

    fn eq(a: *const Self, b: *const Self) -> bool {
        unsafe {
            match (&(*a).obj_type, &(*b).obj_type) {
                (ObjType::String, ObjType::String) => a == b,
            }
        }
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
    ptr: *const u8,
}

impl ObjString {
    fn new_copied(s: &str, alloc: Arc<Allocator>) -> Self {
        let len = s.len();
        let str_ptr = unsafe {
            let str_ptr = alloc.allocate(Layout::array::<u8>(len).unwrap()).as_ptr();
            ptr::copy(s.as_ptr(), str_ptr, len);
            str_ptr as *const _
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

    pub fn as_str(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.internal.ptr, self.internal.len);
            std::str::from_utf8_unchecked(slice)
        }
    }

    pub fn hash(&self) -> u32 {
        self.internal.hash
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
        let len = self.internal.len + other.internal.len;
        let str_ptr = unsafe {
            let str_ptr = self
                .object
                .alloc
                .allocate(Layout::array::<u8>(len).unwrap())
                .as_ptr();
            ptr::copy(self.internal.ptr, str_ptr, self.internal.len);
            ptr::copy(
                other.internal.ptr,
                str_ptr.add(self.internal.len),
                other.internal.len,
            );
            str_ptr
        };
        let hash = Self::make_hash(str_ptr, len);
        Self {
            object: Object::new(ObjType::String, self.object.alloc.clone()),
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
            self.object.alloc.dealloc(
                self.internal.ptr as *mut _,
                Layout::array::<u8>(len).unwrap(),
            );
            self.internal.ptr = ptr::null_mut();
        }
    }
}

impl Display for ObjString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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
        let d = unsafe {
            heap_manager
                .create_string_concat(&*(a.0 as *const ObjString), &*(b.0 as *const ObjString))
        };
        assert_eq!(c, d);
    }
}
