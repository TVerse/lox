use crate::compiler::compile;
use crate::heap::MemoryManager;
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
    let mut mm = MemoryManager::new();
    let chunk = compile(&mut scanner.iter(), &mut mm)?;
    let mut vm = VM::new(write, &mut mm);
    vm.run(&chunk)?;
    Ok(())
}
