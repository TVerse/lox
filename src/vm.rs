use crate::chunk::{Chunk, Opcode};
use crate::heap::{HeapManager, Object};
use crate::value::Value;
use arrayvec::ArrayVec;
use log::{error, trace};
use num_enum::TryFromPrimitiveError;
use std::io::Write;
use thiserror::Error;

type VMResult<A> = Result<A, InterpretError>;

const STACK_SIZE: usize = 256;

pub struct VM<'a, W: Write> {
    write: &'a mut W,
    ip: usize,
    // could this be a list of refs? Runs into lifetime issues!
    stack: ArrayVec<Value, STACK_SIZE>,
    heap_manager: HeapManager,
}

impl<'a, W: Write> VM<'a, W> {
    pub fn new(write: &'a mut W, heap_manager: HeapManager) -> Self {
        Self {
            write,
            ip: 0,
            stack: ArrayVec::new(),
            heap_manager,
        }
    }

    pub fn run(&mut self, chunk: &Chunk) -> VMResult<()> {
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
            let opcode =
                Opcode::try_from(self.read_byte(chunk)?).map_err(IncorrectInvariantError::from)?;
            match opcode {
                Opcode::Constant => {
                    let constant = (*self.read_constant(chunk)?).clone();
                    self.push(constant)?;
                }
                Opcode::Return => {
                    let val = self.pop()?;
                    if let Err(e) = writeln!(self.write, "{}", val) {
                        error!("Error writing output value: {e}")
                    }
                    break;
                }
                Opcode::Negate => {
                    let value = self.pop()?;
                    let value = match value {
                        Value::Number(num) => Value::Number(-num),
                        _ => return Err(RuntimeError::InvalidTypes.into()),
                    };
                    self.push(value)?;
                }
                Opcode::Add => {
                    match (self.peek(0)?, self.peek(1)?) {
                        (Value::Number(_), Value::Number(_)) => {
                            self.binary_op(|a, b| a + b, Value::Number)?
                        }
                        (Value::Obj(ptra), Value::Obj(ptrb)) => {
                            match (
                                Object::as_objstring(*ptra as *const _),
                                Object::as_objstring(*ptrb as *const _),
                            ) {
                                (Some(_), Some(_)) => self.concatenate()?,
                                _ => return Err(RuntimeError::InvalidTypes.into()),
                            }
                        }
                        _ => return Err(RuntimeError::InvalidTypes.into()),
                    };
                }
                Opcode::Subtract => self.binary_op(|a, b| a - b, Value::Number)?,
                Opcode::Multiply => self.binary_op(|a, b| a * b, Value::Number)?,
                Opcode::Divide => self.binary_op(|a, b| a / b, Value::Number)?,
                Opcode::True => self.push(Value::Boolean(true))?,
                Opcode::False => self.push(Value::Boolean(false))?,
                Opcode::Nil => self.push(Value::Nil)?,
                Opcode::Not => {
                    let value = self.pop()?;
                    self.push(Value::Boolean(value.is_falsey()))?
                }
                Opcode::Equal => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(Value::Boolean(a == b))?
                }
                Opcode::Greater => self.binary_op(|a, b| a > b, Value::Boolean)?,
                Opcode::Less => self.binary_op(|a, b| a < b, Value::Boolean)?,
            }
        }

        Ok(())
    }

    fn read_byte(&mut self, chunk: &Chunk) -> VMResult<u8> {
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

    fn read_constant<'b, 'c>(&'b mut self, chunk: &'c Chunk) -> VMResult<&'c Value> {
        let byte = self.read_byte(chunk)?;
        let constant = chunk
            .get_constant(byte)
            .ok_or(IncorrectInvariantError::InvalidConstant { index: byte })?;
        Ok(constant)
    }

    fn push(&mut self, value: Value) -> VMResult<()> {
        self.stack
            .try_push(value)
            .map_err(|_| RuntimeError::StackOverflow.into())
    }

    fn pop(&mut self) -> VMResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| IncorrectInvariantError::StackUnderflow.into())
    }

    fn binary_op<T>(&mut self, f: impl Fn(f64, f64) -> T, v: fn(T) -> Value) -> VMResult<()> {
        let b = self.pop()?;
        let a = self.pop()?;

        let res = match (a, b) {
            (Value::Number(a), Value::Number(b)) => v(f(a, b)),
            (_, _) => return Err(InterpretError::RuntimeError(RuntimeError::InvalidTypes)),
        };
        self.push(res)?;
        Ok(())
    }

    fn peek(&self, distance: usize) -> VMResult<&Value> {
        self.stack
            .get(self.stack.len() - distance - 1)
            .ok_or_else(|| IncorrectInvariantError::StackUnderflow.into())
    }

    fn concatenate(&mut self) -> VMResult<()> {
        let b = self.pop()?;
        let a = self.pop()?;
        let (a, b) = match (a, b) {
            (Value::Obj(a), Value::Obj(b)) => unsafe {
                match (Object::as_objstring(a), Object::as_objstring(b)) {
                    (Some(a), Some(b)) => (&*a, &*b),
                    _ => unreachable!(),
                }
            },
            _ => unreachable!(),
        };
        let value = Value::Obj(self.heap_manager.create_string_concat(a, b));
        self.push(value)
    }
}

#[derive(Error, Debug)]
pub enum InterpretError {
    #[error("Compilation error: {0}")]
    IncorrectInvariantError(#[from] IncorrectInvariantError),
    #[error("Runtime error: {0}")]
    RuntimeError(#[from] RuntimeError),
}

#[derive(Error, Debug)]
pub enum IncorrectInvariantError {
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
