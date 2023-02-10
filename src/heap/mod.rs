use crate::heap::allocator::Allocator;
use crate::heap::hash_table::HashTable;
use crate::value::Value;
use std::alloc::Layout;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::Arc;
use std::{ptr, slice};

pub mod allocator;
pub mod hash_table;
mod vec;

pub use vec::VMHeapVec;

#[derive(Debug)]
pub struct HeapManager {
    known_objects: Option<Object>,
    alloc: Arc<Allocator>,
    strings: HashTable,
}

impl HeapManager {
    pub fn new(alloc: Arc<Allocator>, strings: HashTable) -> Self {
        Self {
            known_objects: None,
            alloc,
            strings,
        }
    }

    pub fn alloc(&self) -> Arc<Allocator> {
        self.alloc.clone()
    }

    pub fn new_str_copied(&mut self, s: &str) -> VMHeap<ObjString> {
        let s = ObjString::new_copied(s, self.alloc.clone());
        if let Some(str) = self.strings.get_string(NonNull::from(&s)) {
            str
        } else {
            let str = VMHeap::new(s, self.alloc.clone());
            self.strings.insert(str, Value::Nil);
            self.register_obj(Object::String(str));
            str
        }
    }

    pub fn new_str_concat(&mut self, a: &ObjString, b: &ObjString) -> VMHeap<ObjString> {
        let s = ObjString::new_concat(a, b);
        if let Some(str) = self.strings.get_string(NonNull::from(&s)) {
            str
        } else {
            let str = VMHeap::new(s, self.alloc.clone());
            self.strings.insert(str, Value::Nil);
            self.register_obj(Object::String(str));
            str
        }
    }

    fn register_obj(&mut self, mut obj: Object) {
        *obj.next_obj() = self.known_objects;
        self.known_objects = Some(obj);
    }

    unsafe fn drop_object(&mut self, obj: Object) {
        let layout = obj.layout();
        let ptr = obj.as_ptr_u8();
        obj.drop_in_place();
        self.alloc.dealloc(ptr, layout);
    }
}

impl Drop for HeapManager {
    fn drop(&mut self) {
        let mut obj = self.known_objects;
        while let Some(mut ptr) = obj {
            let next = *ptr.next_obj();
            unsafe { self.drop_object(ptr) };
            obj = next;
        }
    }
}

pub struct VMHeap<T: ?Sized>(NonNull<T>);

impl<T> VMHeap<T> {
    fn as_ptr_u8(self) -> NonNull<u8> {
        self.0.cast::<u8>()
    }
}

impl<T: GCAble> VMHeap<T> {
    fn new(t: T, alloc: Arc<Allocator>) -> Self {
        unsafe {
            let ptr = alloc.allocate(t.layout()).cast::<T>();
            ptr.as_ptr().write(t);
            Self(ptr)
        }
    }
}

impl<T: ?Sized> Clone for VMHeap<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<T: ?Sized> Copy for VMHeap<T> {}

impl<T: ?Sized> Debug for VMHeap<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Boxed").field("ptr", &self.0).finish()
    }
}

impl<T: ?Sized> Deref for VMHeap<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0.as_ptr() }
    }
}

impl<T: ?Sized> DerefMut for VMHeap<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0.as_ptr() }
    }
}

impl<T: ?Sized + PartialEq> PartialEq for VMHeap<T> {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&**self, &**other)
    }
}

impl<T: ?Sized + Display> Display for VMHeap<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&**self, f)
    }
}

/// Sealed trait, cannot be implemented by external types
///
/// # Safety
/// Layout must be valid for Self.
pub unsafe trait GCAble: private::GCAblePrivate {
    fn next_obj(&mut self) -> &mut Option<Object>;

    fn layout(&self) -> Layout
    where
        Self: Sized,
    {
        Layout::new::<Self>()
    }
}

#[doc(hidden)]
mod private {
    use crate::heap::{ObjString, Object};

    pub trait GCAblePrivate {}
    impl GCAblePrivate for Object {}
    impl GCAblePrivate for ObjString {}
}

#[derive(Debug, Copy, Clone)]
pub enum Object {
    String(VMHeap<ObjString>),
}

impl Object {
    unsafe fn drop_in_place(self) {
        match self {
            Object::String(s) => s.0.as_ptr().drop_in_place(),
        }
    }

    fn as_ptr_u8(self) -> NonNull<u8> {
        match self {
            Object::String(s) => s.as_ptr_u8(),
        }
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Object::String(a), Object::String(b)) => a == b,
        }
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::String(s) => Display::fmt(s, f),
        }
    }
}

unsafe impl GCAble for Object {
    fn next_obj(&mut self) -> &mut Option<Object> {
        match self {
            Object::String(s) => s.next_obj(),
        }
    }

    fn layout(&self) -> Layout {
        match self {
            Object::String(s) => s.layout(),
        }
    }
}

#[derive(Debug)]
pub struct ObjString {
    len: usize,
    hash: u32,
    ptr: NonNull<u8>,
    alloc: Arc<Allocator>,
    next: Option<Object>,
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
            len,
            hash,
            ptr: str_ptr,
            alloc,
            next: None,
        }
    }

    fn new_concat(&self, other: &Self) -> Self {
        let len = self.len + other.len;
        let alloc = self.alloc.clone();
        let str_ptr = if len == 0 {
            NonNull::dangling()
        } else {
            unsafe {
                let str_ptr = alloc.allocate(Layout::array::<u8>(len).unwrap());
                if self.len != 0 {
                    ptr::copy(self.ptr.as_ptr(), str_ptr.as_ptr(), self.len);
                }
                if other.len != 0 {
                    ptr::copy(
                        other.ptr.as_ptr(),
                        str_ptr.as_ptr().add(self.len),
                        other.len,
                    );
                }
                str_ptr
            }
        };
        let hash = Self::make_hash(str_ptr, len);
        Self {
            len,
            hash,
            ptr: str_ptr,
            alloc,
            next: None,
        }
    }

    fn make_hash(chars: NonNull<u8>, len: usize) -> u32 {
        let mut hash = 2166136261;
        for i in 0..len {
            hash ^= unsafe { *chars.as_ptr().add(i) } as u32;
            hash = hash.wrapping_mul(16777619);
        }
        hash
    }

    fn hash(s: NonNull<ObjString>) -> u32 {
        unsafe { (*s.as_ptr()).hash }
    }

    fn as_str(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.ptr.as_ptr() as *const _, self.len);
            std::str::from_utf8_unchecked(slice)
        }
    }
}

impl PartialEq for ObjString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

unsafe impl GCAble for ObjString {
    fn next_obj(&mut self) -> &mut Option<Object> {
        &mut self.next
    }
}

impl Display for ObjString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl Drop for ObjString {
    fn drop(&mut self) {
        unsafe {
            let len = self.len;
            if len != 0 {
                self.alloc
                    .dealloc(self.ptr, Layout::array::<u8>(len).unwrap());
            }
            self.ptr = NonNull::dangling();
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
        let a = heap_manager.new_str_copied("hi!");
        let b = heap_manager.new_str_copied("hi!");
        let c = heap_manager.new_str_copied("hi!hi!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        let d = heap_manager.new_str_concat(&a, &b);
        assert_eq!(c, d);
    }
}
