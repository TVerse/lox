use crate::heap::allocator::Allocator;
use crate::heap::ObjString;
use crate::value::Value;
use std::alloc::Layout;
use std::ptr;
use std::sync::Arc;

#[derive(Debug)]
pub struct HashTable {
    count: usize,
    capacity: usize,
    entries: *mut Entry,
    alloc: Arc<Allocator>,
}

impl HashTable {
    const MAX_LOAD: f64 = 0.75;

    pub fn new(alloc: Arc<Allocator>) -> Self {
        Self {
            count: 0,
            capacity: 0,
            entries: ptr::null_mut(),
            alloc,
        }
    }

    pub(in crate::heap) fn get_string(&self, key: *const ObjString) -> Option<*const ObjString> {
        if self.count == 0 {
            return None;
        }
        unsafe {
            let hash = (*key).hash as usize;
            let index = hash % self.capacity;
            for i in 0..self.capacity {
                let entry = self.entries.add((index + i) % self.capacity);
                if (*entry).key.is_null() {
                    if (*entry).value == Value::Nil {
                        return None;
                    }
                } else if ObjString::as_str(&*(*entry).key) == ObjString::as_str(&*key) {
                    return Some((*entry).key);
                }
            }
            unreachable!("Didn't find string in intern table")
        }
    }

    pub unsafe fn clear(&mut self) {
        if !self.entries.is_null() {
            self.alloc.dealloc(
                self.entries as *mut u8,
                Layout::array::<Entry>(self.capacity).unwrap(),
            )
        }
        self.count = 0;
        self.capacity = 0;
    }

    pub fn get(&self, key: *const ObjString) -> Option<&Value> {
        if self.capacity == 0 {
            return None;
        }
        let entry = Self::find_entry(self.entries, key, self.capacity);
        unsafe {
            if !(*entry).key.is_null() {
                let entry = &*entry;
                Some(&entry.value)
            } else {
                None
            }
        }
    }

    // TODO Option<Value>
    pub fn delete(&mut self, key: *const ObjString) -> bool {
        if self.count == 0 {
            return false;
        }

        unsafe {
            let entry = Self::find_entry(self.entries, key, self.capacity);
            if (*entry).key.is_null() {
                return false;
            }
            entry.write(Entry {
                key: ptr::null_mut(),
                value: Value::Boolean(true),
            });
            true
        }
    }

    // TODO Option<Value>
    pub fn insert(&mut self, key: *const ObjString, value: Value) -> bool {
        if (self.count + 1) as f64 > (self.capacity as f64) * Self::MAX_LOAD {
            let new_capacity = self.grow_capacity();
            self.adjust_capacity(new_capacity)
        }

        let entry = Self::find_entry(self.entries, key, self.capacity);
        unsafe {
            let is_new_key = (*entry).key.is_null();
            if is_new_key {
                self.count += 1;
            }

            (*entry).key = key;
            (*entry).value = value;

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
        unsafe { std::slice::from_raw_parts(self.entries as *const _, self.capacity) }
    }

    fn adjust_capacity(&mut self, new_capacity: usize) {
        unsafe {
            let entries = self
                .alloc
                .allocate(Layout::array::<Entry>(new_capacity).unwrap())
                as *mut Entry;
            for i in 0..new_capacity {
                let entry = Entry {
                    key: ptr::null_mut(),
                    value: Value::Nil,
                };
                entries.add(i).write(entry)
            }
            self.count = 0;
            for i in 0..self.capacity {
                let source = self.entries.add(i).read();
                if source.key.is_null() {
                    continue;
                }
                let dest = Self::find_entry(entries, source.key, new_capacity);
                dest.write(source);
                self.count += 1;
            }

            if !self.entries.is_null() {
                self.alloc.dealloc(
                    self.entries as *mut u8,
                    Layout::array::<Entry>(self.capacity).unwrap(),
                )
            }

            self.entries = entries;
            self.capacity = new_capacity;
        }
    }

    fn find_entry(entries: *mut Entry, key: *const ObjString, capacity: usize) -> *mut Entry {
        unsafe {
            let hash = (*key).hash() as usize;
            let index = hash % capacity;
            let mut tombstone: *mut Entry = ptr::null_mut();
            for i in 0..capacity {
                let entry = entries.add((index + i) % capacity);
                if (*entry).key.is_null() {
                    if (*entry).value == Value::Nil {
                        return if !tombstone.is_null() {
                            tombstone
                        } else {
                            entry
                        };
                    } else if tombstone.is_null() {
                        tombstone = entry
                    }
                } else if (*entry).key == key {
                    return entry;
                }
            }
            unreachable!(
                "Didn't find entry for {key:?} in table {:?}",
                std::slice::from_raw_parts(entries as *const _, capacity)
            )
        }
    }
}

impl Drop for HashTable {
    fn drop(&mut self) {
        unsafe { self.clear() }
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
    use crate::heap::{HeapManager, Object};

    const MAX: usize = if cfg!(miri) { 17 } else { 2500 };

    #[test]
    fn insert() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let key = {
            let obj = heap_manager.create_string_copied("hi!");
            Object::as_objstring(obj).unwrap()
        };
        let value = Value::Number(1.5);
        assert!(table.insert(key, value));
        assert!(!table.insert(key, value));
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
                    Object::as_objstring(obj).unwrap()
                };
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(table.insert(*k, *v), "{k:?}, {}, {v}", unsafe {
                &**k
            });
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {}, {v}", unsafe { &**k });
            assert!(!table.insert(*k, *v), "{k:?}, {}, {v}", unsafe {
                &**k
            });
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {}, {v}", unsafe { &**k });
        }
    }

    #[test]
    fn get() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let obj = heap_manager.create_string_copied("hi!");
        let key = Object::as_objstring(obj).unwrap();
        let value = Value::Number(1.5);
        assert_eq!(table.get(key), None);
        assert!(table.insert(key, value));
        assert_eq!(table.get(key).unwrap(), &value);
        assert!(!table.insert(key, value));
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
                    Object::as_objstring(obj).unwrap()
                };
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(table.insert(*k, *v), "{k:?}, {}, {v}", unsafe {
                &**k
            });
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {}, {v}", unsafe { &**k });
            assert!(!table.insert(*k, *v), "{k:?}, {}, {v}", unsafe {
                &**k
            });
            assert!(table.delete(*k));
            assert_eq!(table.get(*k), None, "{k:?}, {}, {v}", unsafe { &**k });
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(*k), None, "{k:?}, {}, {v}", unsafe { &**k });
        }
    }
}
