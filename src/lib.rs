use crate::compiler::{compile, CompileErrors};
use crate::memory::allocator::Allocator;
use crate::memory::hash_table::HashTable;
use crate::memory::MemoryManager;
use crate::scanner::Scanner;
use crate::vm::{VMError, VM};
use log::trace;
use std::io::Write;
use thiserror::Error;

mod chunk;
mod compiler;
mod memory;
mod scanner;
mod value;
mod vm;

pub fn interpret<W: Write>(source: &str, write: &mut W) -> Result<(), InterpretError> {
    trace!("Got input string: {source}");
    let scanner = Scanner::new(source);
    let alloc = Allocator::new();
    let strings = HashTable::new(alloc.clone());
    let mut heap_manager = MemoryManager::new(alloc.clone(), strings);
    let chunk = compile(&mut scanner.iter(), &mut heap_manager)?;
    let mut vm = VM::new(write, heap_manager, alloc);
    vm.run(&chunk)?;
    Ok(())
}

#[derive(Error, Debug, Clone)]
pub enum InterpretError {
    #[error(transparent)]
    CompileErrors(#[from] CompileErrors),
    #[error(transparent)]
    InterpretError(#[from] VMError),
}
