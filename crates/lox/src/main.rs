use clap::Parser;
use env_logger::Builder;
use log::LevelFilter;
use lox::chunk::{Chunk, Opcode};
use lox::value::Value;
use lox::vm::VM;
use std::error::Error;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    file: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let _args = Args::parse();

    init_logger();

    let mut chunk = Chunk::new("main".to_string());
    let constant = chunk.add_constant(Value::new(1.2));
    chunk.add_byte(Opcode::Constant.as_byte(), 1);
    chunk.add_byte(constant, 2);

    let constant = chunk.add_constant(Value::new(3.4));
    chunk.add_byte(Opcode::Constant.as_byte(), 1);
    chunk.add_byte(constant, 1);

    chunk.add_byte(Opcode::Add.as_byte(), 1);

    let constant = chunk.add_constant(Value::new(5.6));
    chunk.add_byte(Opcode::Constant.as_byte(), 1);
    chunk.add_byte(constant, 1);

    chunk.add_byte(Opcode::Divide.as_byte(), 1);

    chunk.add_byte(Opcode::Negate.as_byte(), 1);
    chunk.add_byte(Opcode::Return.as_byte(), 1);

    let mut vm = VM::new();
    vm.interpret(&chunk)?;
    Ok(())
}

fn init_logger() {
    let mut builder = Builder::new();
    if cfg!(debug_assertions) {
        builder.filter_level(LevelFilter::Trace);
    }
    builder.init()
}
