use crate::compiler::compile;
use crate::scanner::Scanner;
use crate::vm::VM;
use anyhow::Result;
use log::trace;
use std::io::Write;

mod chunk;
mod compiler;
mod scanner;
mod value;
mod vm;

pub fn interpret<W: Write>(source: &str, write: &mut W) -> Result<()> {
    trace!("Got input string: {source}");
    let scanner = Scanner::new(source);
    let chunk = compile(&mut scanner.iter())?;
    let mut vm = VM::new(write);
    vm.run(&chunk)?;
    Ok(())
}
