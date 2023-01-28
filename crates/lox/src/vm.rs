use crate::chunk::{Chunk, Opcode};
use crate::compiler::Compiler;
use crate::value::Value;
use arrayvec::ArrayVec;
use log::trace;
use num_enum::TryFromPrimitiveError;
use thiserror::Error;

const STACK_SIZE: usize = 256;

pub struct VM {
    ip: usize,
    // could this be a list of refs? Runs into lifetime issues!
    stack: ArrayVec<Value, STACK_SIZE>,
    compiler: Compiler,
}

impl VM {
    pub fn new() -> Self {
        Self {
            ip: 0,
            stack: ArrayVec::new(),
            compiler: Compiler::new(),
        }
    }

    pub fn interpret(&mut self, source: &str) -> Result<(), InterpretError> {
        self.ip = 0;

        todo!()
    }

    fn run(&mut self, chunk: &Chunk) -> Result<(), InterpretError> {
        // TODO some kind of iterator?
        loop {
            trace!("Stack:\n{stack:?}", stack = self.stack);
            trace!(
                "Instruction at {ip}: {instruction}",
                ip = self.ip,
                instruction = chunk
                    .disassemble_instruction_at(self.ip)
                    .unwrap_or_else(|| "Not found, crash imminent".to_string())
            );
            let opcode = Opcode::try_from(self.read_byte(chunk)?).map_err(CompileError::from)?;
            match opcode {
                Opcode::Constant => {
                    let constant = (*self.read_constant(chunk)?).clone();
                    self.push(constant)?;
                }
                Opcode::Return => {
                    let val = self.pop()?;
                    println!("{}", val);
                    break;
                }
                Opcode::Negate => {
                    let value = self.pop()?;
                    let new_value = match value {
                        Value::Number(num) => Value::Number(-num),
                    };
                    self.push(new_value)?;
                }
                Opcode::Add => self.binary_op(|a, b| a + b)?,
                Opcode::Subtract => self.binary_op(|a, b| a - b)?,
                Opcode::Multiply => self.binary_op(|a, b| a * b)?,
                Opcode::Divide => self.binary_op(|a, b| a / b)?,
            }
        }

        Ok(())
    }

    fn read_byte(&mut self, chunk: &Chunk) -> Result<u8, InterpretError> {
        let byte = chunk
            .get(self.ip)
            .copied()
            .ok_or(RuntimeError::InvalidInstructionPointer {
                pointer: self.ip,
                chunk_length: chunk.len(),
            })?;
        self.ip += 1;
        Ok(byte)
    }

    fn read_constant<'a, 'b>(&'a mut self, chunk: &'b Chunk) -> Result<&'b Value, InterpretError> {
        let byte = self.read_byte(chunk)?;
        let constant = chunk
            .get_constant(byte)
            .ok_or(CompileError::InvalidConstant { index: byte })?;
        Ok(constant)
    }

    fn push(&mut self, value: Value) -> Result<(), RuntimeError> {
        self.stack
            .try_push(value)
            .map_err(|_| RuntimeError::StackOverflow)
    }

    fn pop(&mut self) -> Result<Value, CompileError> {
        self.stack.pop().ok_or(CompileError::StackUnderflow)
    }

    fn binary_op(&mut self, f: impl Fn(f64, f64) -> f64) -> Result<(), InterpretError> {
        let b = self.pop()?;
        let a = self.pop()?;

        let res = match (a, b) {
            (Value::Number(a), Value::Number(b)) => Value::Number(f(a, b)),
            // (_, _) => return Err(InterpretError::RuntimeError(RuntimeError::InvalidTypes))
        };
        self.push(res)?;
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum InterpretError {
    #[error("Compilation error: {0}")]
    CompileError(#[from] CompileError),
    #[error("Runtime error: {0}")]
    RuntimeError(#[from] RuntimeError),
}

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("invalid opcode? {0}")]
    InvalidOpcode(#[from] TryFromPrimitiveError<Opcode>),
    #[error("invalid constant? {index}")]
    InvalidConstant { index: u8 },
    #[error("stack underflow?")]
    StackUnderflow,
}

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("invalid instruction pointer {pointer}, max length {chunk_length}")]
    InvalidInstructionPointer { pointer: usize, chunk_length: usize },
    #[error("stack overflow")]
    StackOverflow,
    #[error("invalid types")]
    InvalidTypes,
}
