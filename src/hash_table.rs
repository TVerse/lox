use crate::heap::{MemoryManager, ObjString};
use crate::value::Value;
use std::alloc::Layout;
use std::ptr;

#[derive(Debug)]
pub struct HashTable {
    count: usize,
    capacity: usize,
    entries: *mut Entry,
}

impl HashTable {
    const MAX_LOAD: f64 = 0.75;

    pub fn new() -> Self {
        Self {
            count: 0,
            capacity: 0,
            entries: ptr::null_mut(),
        }
    }

    pub unsafe fn clear(&mut self, mm: &mut MemoryManager) {
        if !self.entries.is_null() {
            mm.dealloc(
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
    pub fn insert(&mut self, key: *const ObjString, value: Value, mm: &mut MemoryManager) -> bool {
        if (self.count + 1) as f64 > (self.capacity as f64) * Self::MAX_LOAD {
            let new_capacity = self.grow_capacity();
            self.adjust_capacity(new_capacity, mm)
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

    fn adjust_capacity(&mut self, new_capacity: usize, mm: &mut MemoryManager) {
        unsafe {
            let entries = mm.allocate(Layout::array::<Entry>(new_capacity).unwrap()) as *mut Entry;
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
                mm.dealloc(
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

#[derive(Debug)]
struct Entry {
    key: *const ObjString,
    value: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Object;

    const MAX: usize = if cfg!(miri) { 17 } else { 2500 };

    #[test]
    fn insert() {
        let mut mm = MemoryManager::new();
        let mut table = HashTable::new();
        let key: *const ObjString = {
            let obj = Object::new_str("hi", &mut mm);
            unsafe { (&*obj).as_str().unwrap() }
        };
        let value = Value::Number(1.5);
        assert!(table.insert(key, value.clone(), &mut mm));
        assert!(!table.insert(key, value, &mut mm));
        unsafe { table.clear(&mut mm) };
    }

    #[test]
    fn insert_multiple() {
        let mut mm = MemoryManager::new();
        let mut table = HashTable::new();
        let kvs: Vec<_> = (0..MAX)
            .map(|i| {
                let key: *const ObjString = {
                    let obj = Object::new_str(&format!("hi{i}"), &mut mm);
                    unsafe { (&*obj).as_str().unwrap() }
                };
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(
                table.insert(*k, v.clone(), &mut mm),
                "{k:?}, {}, {v}",
                unsafe { &**k }
            );
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {}, {v}", unsafe { &**k });
            assert!(
                !table.insert(*k, v.clone(), &mut mm),
                "{k:?}, {}, {v}",
                unsafe { &**k }
            );
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {}, {v}", unsafe { &**k });
        }
        unsafe { table.clear(&mut mm) };
    }

    #[test]
    fn get() {
        let mut mm = MemoryManager::new();
        let mut table = HashTable::new();
        let obj = Object::new_str("hi", &mut mm);
        let key: *const ObjString = { unsafe { (&*obj).as_str().unwrap() } };
        let value = Value::Number(1.5);
        assert_eq!(table.get(key), None);
        assert!(table.insert(key, value.clone(), &mut mm));
        assert_eq!(table.get(key).unwrap(), &value);
        assert!(!table.insert(key, value.clone(), &mut mm));
        unsafe { table.clear(&mut mm) };
    }

    #[test]
    fn delete() {
        let mut mm = MemoryManager::new();
        let mut table = HashTable::new();
        let kvs: Vec<_> = (0..MAX)
            .map(|i| {
                let key: *const ObjString = {
                    let obj = Object::new_str(&format!("hi{i}"), &mut mm);
                    unsafe { (&*obj).as_str().unwrap() }
                };
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(
                table.insert(*k, v.clone(), &mut mm),
                "{k:?}, {}, {v}",
                unsafe { &**k }
            );
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {}, {v}", unsafe { &**k });
            assert!(
                !table.insert(*k, v.clone(), &mut mm),
                "{k:?}, {}, {v}",
                unsafe { &**k }
            );
            assert!(table.delete(*k));
            assert_eq!(table.get(*k), None, "{k:?}, {}, {v}", unsafe { &**k });
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(*k), None, "{k:?}, {}, {v}", unsafe { &**k });
        }
        unsafe { table.clear(&mut mm) };
    }
}
