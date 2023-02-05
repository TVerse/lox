use crate::heap::allocator::Allocator;
use crate::heap::ObjString;
use crate::value::Value;
use std::alloc::Layout;
use std::fmt::{Debug, Formatter};
use std::ptr;
use std::ptr::NonNull;
use std::sync::Arc;

pub struct HashTable {
    count: usize,
    capacity: usize,
    entries: NonNull<Entry>,
    alloc: Arc<Allocator>,
}

impl HashTable {
    const MAX_LOAD: f64 = 0.75;

    pub fn new(alloc: Arc<Allocator>) -> Self {
        Self {
            count: 0,
            capacity: 0,
            entries: NonNull::dangling(),
            alloc,
        }
    }

    pub(in crate::heap) fn get_string(
        &self,
        key: NonNull<ObjString>,
    ) -> Option<NonNull<ObjString>> {
        if self.count == 0 {
            return None;
        }
        unsafe {
            let hash = (*key.as_ptr()).hash() as usize;
            let index = hash % self.capacity;
            for i in 0..self.capacity {
                let entry = self.entries.as_ptr().add((index + i) % self.capacity);
                match NonNull::new((*entry).key as *mut _) {
                    Some(entry_key) => {
                        if ObjString::as_str(&*entry_key.as_ptr())
                            == ObjString::as_str(&*key.as_ptr())
                        {
                            return Some(entry_key);
                        }
                    }
                    None => {
                        if (*entry).value == Value::Nil {
                            return None;
                        }
                    }
                }
            }
            unreachable!("Didn't find string in intern table")
        }
    }

    pub unsafe fn clear(&mut self) {
        if self.count != 0 {
            self.alloc.dealloc(
                self.entries.cast::<u8>(),
                Layout::array::<Entry>(self.capacity).unwrap(),
            )
        }
        self.count = 0;
        self.capacity = 0;
    }

    pub fn get(&self, key: NonNull<ObjString>) -> Option<&Value> {
        if self.capacity == 0 {
            return None;
        }
        let entry = Self::find_entry(self.entries, key, self.capacity);
        unsafe {
            if !(*entry.as_ptr()).key.is_null() {
                let entry = &*(entry.as_ptr());
                Some(&entry.value)
            } else {
                None
            }
        }
    }

    // TODO Option<Value>
    pub fn delete(&mut self, key: NonNull<ObjString>) -> bool {
        if self.count == 0 {
            return false;
        }

        unsafe {
            let entry = Self::find_entry(self.entries, key, self.capacity);
            if (*entry.as_ptr()).key.is_null() {
                return false;
            }
            entry.as_ptr().write(Entry {
                key: ptr::null_mut(),
                value: Value::Boolean(true),
            });
            true
        }
    }

    // TODO Option<Value>
    pub fn insert(&mut self, key: NonNull<ObjString>, value: Value) -> bool {
        if (self.count + 1) as f64 > (self.capacity as f64) * Self::MAX_LOAD {
            let new_capacity = self.grow_capacity();
            self.adjust_capacity(new_capacity)
        }

        let entry = Self::find_entry(self.entries, key, self.capacity);
        unsafe {
            let is_new_key = (*entry.as_ptr()).key.is_null();
            if is_new_key {
                self.count += 1;
            }

            (*entry.as_ptr()).key = key.as_ptr();
            (*entry.as_ptr()).value = value;

            is_new_key
        }
    }

    fn grow_capacity(&mut self) -> usize {
        if self.capacity < 8 {
            8
        } else {
            self.capacity * 2
        }
    }

    fn entries_as_slice(&self) -> &[Entry] {
        unsafe { std::slice::from_raw_parts(self.entries.as_ptr() as *const _, self.capacity) }
    }

    fn adjust_capacity(&mut self, new_capacity: usize) {
        unsafe {
            let entries = self
                .alloc
                .allocate(Layout::array::<Entry>(new_capacity).unwrap())
                .cast::<Entry>();
            for i in 0..new_capacity {
                let entry = Entry {
                    key: ptr::null_mut(),
                    value: Value::Nil,
                };
                entries.as_ptr().add(i).write(entry)
            }
            let prev_count = self.count;
            self.count = 0;
            for i in 0..self.capacity {
                let source = self.entries.as_ptr().add(i).read();
                match NonNull::new(source.key as *mut _) {
                    Some(key) => {
                        let dest = Self::find_entry(entries, key, new_capacity);
                        dest.as_ptr().write(source);
                        self.count += 1;
                    }
                    None => continue,
                }
            }

            if prev_count != 0 {
                self.alloc.dealloc(
                    self.entries.cast::<u8>(),
                    Layout::array::<Entry>(self.capacity).unwrap(),
                )
            }

            self.entries = entries;
            self.capacity = new_capacity;
        }
    }

    fn find_entry(
        entries: NonNull<Entry>,
        key: NonNull<ObjString>,
        capacity: usize,
    ) -> NonNull<Entry> {
        unsafe {
            let hash = (*key.as_ptr()).hash() as usize;
            let index = hash % capacity;
            let mut tombstone: Option<NonNull<Entry>> = None;
            for i in 0..capacity {
                let entry = NonNull::new_unchecked(entries.as_ptr().add((index + i) % capacity));
                if (*entry.as_ptr()).key.is_null() {
                    if (*entry.as_ptr()).value == Value::Nil {
                        return if let Some(tombstone) = tombstone {
                            tombstone
                        } else {
                            entry
                        };
                    } else if tombstone.is_none() {
                        tombstone = Some(entry)
                    }
                } else if (*entry.as_ptr()).key == key.as_ptr() {
                    return entry;
                }
            }
            unreachable!(
                "Didn't find entry for {key:?} in table {:?}",
                std::slice::from_raw_parts(entries.as_ptr() as *const _, capacity)
            )
        }
    }
}

impl Drop for HashTable {
    fn drop(&mut self) {
        unsafe { self.clear() }
    }
}

impl Debug for HashTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HashTable")
            .field("count", &self.count)
            .field("capacity", &self.capacity)
            .field("entries", &self.entries_as_slice())
            .field("alloc", &self.alloc)
            .finish()
    }
}

#[derive(Debug)]
struct Entry {
    key: *const ObjString,
    value: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::HeapManager;

    const MAX: usize = if cfg!(miri) { 17 } else { 2500 };

    #[test]
    fn insert() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let key = {
            let obj = heap_manager.create_string_copied("hi!");
            obj.as_objstring().unwrap()
        };
        let value = Value::Number(1.5);
        assert!(table.insert(key.0, value));
        assert!(!table.insert(key.0, value));
    }

    #[test]
    fn insert_multiple() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let kvs: Vec<_> = (0..MAX)
            .map(|i| {
                let key = {
                    let obj = heap_manager.create_string_copied(&format!("hi{i}"));
                    obj.as_objstring().unwrap()
                };
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(table.insert(k.0, *v), "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
            assert_eq!(table.get(k.0).unwrap(), v, "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
            assert!(!table.insert(k.0, *v), "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(k.0).unwrap(), v, "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
        }
    }

    #[test]
    fn get() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let obj = heap_manager.create_string_copied("hi!");
        let key = obj.as_objstring().unwrap();
        let value = Value::Number(1.5);
        assert_eq!(table.get(key.0), None);
        assert!(table.insert(key.0, value));
        assert_eq!(table.get(key.0).unwrap(), &value);
        assert!(!table.insert(key.0, value));
    }

    #[test]
    fn delete() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let kvs: Vec<_> = (0..MAX)
            .map(|i| {
                let key = {
                    let obj = heap_manager.create_string_copied(&format!("hi{i}"));
                    obj.as_objstring().unwrap()
                };
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(table.insert(k.0, *v), "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
            assert_eq!(table.get(k.0).unwrap(), v, "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
            assert!(!table.insert(k.0, *v), "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
            assert!(table.delete(k.0));
            assert_eq!(table.get(k.0), None, "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(k.0), None, "{k:?}, {}, {v}", unsafe {
                &*k.0.as_ptr()
            });
        }
    }
}
