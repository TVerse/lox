use crate::heap::allocator::Allocator;
use crate::heap::hash_table::HashTable;
use crate::value::Value;
use std::alloc::Layout;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::{ptr, slice};

pub mod allocator;
pub mod hash_table;

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

    unsafe fn register_object(&mut self, object: *mut Object) {
        let old = self.known_objects;
        self.known_objects = object;
        (*object).next = old;
    }

    pub fn create_string_copied(&mut self, s: &str) -> *mut Object {
        let str = ObjString::new_copied(s, self.alloc.clone());
        if let Some(ptr) = self.strings.get_string(&str) {
            ptr as *mut _
        } else {
            let ptr = unsafe { self.move_to_heap(str) };
            self.strings.insert(ptr as *const _, Value::Nil);
            ptr
        }
    }

    pub fn create_string_concat(&mut self, a: &ObjString, b: &ObjString) -> *mut Object {
        let str = a.concat(b);
        if let Some(ptr) = self.strings.get_string(&str) {
            ptr as *mut _
        } else {
            let ptr = unsafe { self.move_to_heap(str) };
            self.strings.insert(ptr as *const _, Value::Nil);
            ptr
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
        let ptr = self.alloc.allocate(Layout::new::<T>()) as *mut T;
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

pub struct Object {
    next: *mut Object,
    obj_type: ObjType,
}

enum ObjType {
    String,
}

impl Object {
    fn new(typ: ObjType) -> Self {
        Self {
            next: ptr::null_mut(),
            obj_type: typ,
        }
    }

    pub fn as_objstring(ptr: *const Self) -> Option<*const ObjString> {
        unsafe {
            match (*ptr).obj_type {
                ObjType::String => Some(ptr as *const ObjString),
            }
        }
    }

    pub fn to_string(ptr: *const Self) -> String {
        unsafe {
            match (*ptr).obj_type {
                ObjType::String => (*(ptr as *const ObjString)).to_string(),
            }
        }
    }

    pub fn eq(a: *const Self, b: *const Self) -> bool {
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
            object: Object::new(ObjType::String),
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
            ptr::copy(other.ptr, str_ptr.add(self.len), other.len);
            str_ptr
        };
        let hash = Self::make_hash(str_ptr, len);
        Self {
            object: Object::new(ObjType::String),
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
            self.alloc
                .dealloc(self.ptr as *mut _, Layout::array::<u8>(len).unwrap());
            self.ptr = ptr::null_mut();
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
            heap_manager.create_string_concat(&*(a as *const ObjString), &*(b as *const ObjString))
        };
        assert_eq!(c, d);
    }
}