use crate::compiler::Compiler;
use crate::scanner::Scanner;
use crate::vm::VM;
use anyhow::Result;

pub mod chunk;
mod compiler;
mod scanner;
pub mod value;
pub mod vm;

pub fn interpret(source: &str) -> Result<()> {
    let scanner = Scanner::new(source);
    let mut compiler = Compiler::new();
    let chunk = compiler.compile(scanner.iter())?;
    let mut vm = VM::new();
    vm.run(&chunk)?;
    Ok(())
}
