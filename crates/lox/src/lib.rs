use crate::compiler::compile;
use crate::scanner::Scanner;
use crate::vm::VM;
use anyhow::Result;
use log::trace;

pub mod chunk;
mod compiler;
mod scanner;
pub mod value;
pub mod vm;

pub fn interpret(source: &str) -> Result<()> {
    trace!("Got input string: {source}");
    let scanner = Scanner::new(source);
    let chunk = compile(&mut scanner.iter())?;
    let mut vm = VM::new();
    vm.run(&chunk)?;
    Ok(())
}
