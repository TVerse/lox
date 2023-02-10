use crate::heap::allocator::Allocator;
use crate::heap::{ObjString, VMHeap};
use crate::value::Value;
use std::alloc::Layout;
use std::fmt::{Debug, Formatter};
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

    pub(in crate::heap) fn get_string(&self, key: NonNull<ObjString>) -> Option<VMHeap<ObjString>> {
        if self.count == 0 {
            return None;
        }
        unsafe {
            let hash = ObjString::hash(key) as usize;
            let index = hash % self.capacity;
            for i in 0..self.capacity {
                let entry = self.entries.as_ptr().add((index + i) % self.capacity);
                match (*entry).key {
                    Some(entry_key) => {
                        if entry_key.as_str() == (*key.as_ptr()).as_str() {
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

    pub fn get(&self, key: VMHeap<ObjString>) -> Option<&Value> {
        if self.count == 0 {
            return None;
        }
        let entry = Self::find_entry(self.entries, key.0, self.capacity);
        unsafe {
            if (*entry.as_ptr()).key.is_some() {
                let entry = &*(entry.as_ptr());
                Some(&entry.value)
            } else {
                None
            }
        }
    }

    // TODO Option<Value>
    pub fn delete(&mut self, key: VMHeap<ObjString>) -> bool {
        if self.count == 0 {
            return false;
        }

        unsafe {
            let entry = Self::find_entry(self.entries, key.0, self.capacity);
            if (*entry.as_ptr()).key.is_none() {
                return false;
            }
            entry.as_ptr().write(Entry {
                key: None,
                value: Value::Boolean(true),
            });
            true
        }
    }

    // TODO Option<Value>
    pub fn insert(&mut self, key: VMHeap<ObjString>, value: Value) -> bool {
        if (self.count + 1) as f64 > (self.capacity as f64) * Self::MAX_LOAD {
            let new_capacity = self.grow_capacity();
            self.adjust_capacity(new_capacity)
        }
        let entry = Self::find_entry(self.entries, key.0, self.capacity);
        unsafe {
            let is_new_key = (*entry.as_ptr()).key.is_none();
            if is_new_key {
                self.count += 1;
            }

            (*entry.as_ptr()).key = Some(key);
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
                    key: None,
                    value: Value::Nil,
                };
                entries.as_ptr().add(i).write(entry)
            }
            let prev_count = self.count;
            self.count = 0;
            for i in 0..self.capacity {
                let source = self.entries.as_ptr().add(i).read();
                if let Some(key) = source.key {
                    let dest = Self::find_entry(entries, key.0, new_capacity);
                    dest.as_ptr().write(source);
                    self.count += 1;
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
            let hash = ObjString::hash(key) as usize;
            let index = hash % capacity;
            let mut tombstone: Option<NonNull<Entry>> = None;
            for i in 0..capacity {
                let entry = NonNull::new_unchecked(entries.as_ptr().add((index + i) % capacity));
                match (*entry.as_ptr()).key {
                    None => {
                        if (*entry.as_ptr()).value == Value::Nil {
                            return if let Some(tombstone) = tombstone {
                                tombstone
                            } else {
                                entry
                            };
                        } else if tombstone.is_none() {
                            tombstone = Some(entry)
                        }
                    }
                    Some(entry_key) => {
                        if entry_key.0 == key {
                            return entry;
                        }
                    }
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

struct Entry {
    key: Option<VMHeap<ObjString>>,
    value: Value,
}

impl Debug for Entry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("key", &self.key)
            .field("value", &self.value)
            .field("key_val", &self.key.map(|k| k.as_str().to_string()))
            .finish()
    }
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
        let key = heap_manager.new_str_copied("hi!");
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
                let key = heap_manager.new_str_copied(&format!("hi{i}"));
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(table.insert(*k, *v), "{k:?}, {k}, {v}");
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {k}, {v}");
            assert!(!table.insert(*k, *v), "{k:?}, {k}, {v}");
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {k}, {v}");
        }
    }

    #[test]
    fn get() {
        let alloc = Allocator::new();
        let strings = HashTable::new(alloc.clone());
        let mut heap_manager = HeapManager::new(alloc.clone(), strings);
        let mut table = HashTable::new(alloc);
        let key = heap_manager.new_str_copied("hi!");
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
                let key = heap_manager.new_str_copied(&format!("hi{i}"));
                let value = Value::Number(i as f64);
                (key, value)
            })
            .collect();

        for (k, v) in kvs.iter() {
            assert!(table.insert(*k, *v), "{k:?}, {k}, {v}");
            assert_eq!(table.get(*k).unwrap(), v, "{k:?}, {k}, {v}");
            assert!(!table.insert(*k, *v), "{k:?}, {k}, {v}");
            assert!(table.delete(*k));
            assert_eq!(table.get(*k), None, "{k:?}, {k}, {v}");
        }
        for (k, v) in kvs.iter() {
            assert_eq!(table.get(*k), None, "{k:?}, {k}, {v}");
        }
    }
}
