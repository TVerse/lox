use crate::compiler::compile;
use crate::heap::allocator::Allocator;
use crate::heap::hash_table::HashTable;
use crate::heap::HeapManager;
use crate::scanner::Scanner;
use crate::vm::VM;
use anyhow::Result;
use log::trace;
use std::io::Write;

mod chunk;
mod compiler;
mod heap;
mod scanner;
mod value;
mod vm;

pub fn interpret<W: Write>(source: &str, write: &mut W) -> Result<()> {
    trace!("Got input string: {source}");
    let scanner = Scanner::new(source);
    let alloc = Allocator::new();
    let strings = HashTable::new(alloc.clone());
    let mut heap_manager = HeapManager::new(alloc, strings);
    let chunk = compile(&mut scanner.iter(), &mut heap_manager)?;
    let mut vm = VM::new(write, heap_manager);
    vm.run(&chunk)?;
    Ok(())
}
