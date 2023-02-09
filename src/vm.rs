use crate::chunk::{Chunk, Opcode};
use crate::heap::allocator::Allocator;
use crate::heap::hash_table::HashTable;
use crate::heap::HeapManager;
use crate::value::Value;
use arrayvec::ArrayVec;
use log::{error, trace};
use num_enum::TryFromPrimitiveError;
use std::io::Write;
use std::sync::Arc;
use thiserror::Error;

type VMResult<A> = Result<A, VMError>;

const STACK_SIZE: usize = 256;

#[derive(Debug)]
pub struct VM<'a, W: Write> {
    write: &'a mut W,
    ip: usize,
    // could this be a list of refs? Runs into lifetime issues!
    stack: ArrayVec<Value, STACK_SIZE>,
    heap_manager: HeapManager,
    globals: HashTable,
}

impl<'a, W: Write> VM<'a, W> {
    pub fn new(write: &'a mut W, heap_manager: HeapManager, allocator: Arc<Allocator>) -> Self {
        Self {
            write,
            ip: 0,
            stack: ArrayVec::new(),
            heap_manager,
            globals: HashTable::new(allocator),
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
                    let constant = *self.read_constant(chunk)?;
                    self.push(constant)?;
                }
                Opcode::Return => break,
                Opcode::Negate => {
                    let value = self.pop()?;
                    let value = match value {
                        Value::Number(num) => Value::Number(-num),
                        _ => return Err(RuntimeError::InvalidType("number").into()),
                    };
                    self.push(value)?;
                }
                Opcode::Add => {
                    match (self.peek(0)?, self.peek(1)?) {
                        (Value::Number(_), Value::Number(_)) => {
                            self.binary_op(|a, b| a + b, Value::Number, chunk.line_for(self.ip))?
                        }
                        (Value::Obj(a), Value::Obj(b)) => {
                            match (a.as_objstring(), b.as_objstring()) {
                                (Some(_), Some(_)) => self.concatenate()?,
                                _ => {
                                    return Err(RuntimeError::InvalidTypes(
                                        chunk.line_for(self.ip),
                                        "strings",
                                    )
                                    .into())
                                }
                            }
                        }
                        _ => {
                            return Err(RuntimeError::InvalidTypes(
                                chunk.line_for(self.ip),
                                "two numbers or two strings",
                            )
                            .into())
                        }
                    };
                }
                Opcode::Subtract => {
                    self.binary_op(|a, b| a - b, Value::Number, chunk.line_for(self.ip))?
                }
                Opcode::Multiply => {
                    self.binary_op(|a, b| a * b, Value::Number, chunk.line_for(self.ip))?
                }
                Opcode::Divide => {
                    self.binary_op(|a, b| a / b, Value::Number, chunk.line_for(self.ip))?
                }
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
                Opcode::Greater => {
                    self.binary_op(|a, b| a > b, Value::Boolean, chunk.line_for(self.ip))?
                }
                Opcode::Less => {
                    self.binary_op(|a, b| a < b, Value::Boolean, chunk.line_for(self.ip))?
                }
                Opcode::Print => {
                    let value = self.pop()?;
                    self.print_value(value)?;
                }
                Opcode::Pop => {
                    let _ = self.pop()?;
                }
                Opcode::DefineGlobal => {
                    let name = self.read_constant(chunk)?;
                    match name {
                        Value::Obj(obj) => {
                            if let Some(s) = obj.as_objstring() {
                                let value = self.peek(0)?;
                                self.globals.insert(s, *value);
                                let _ = self.pop();
                            } else {
                                return Err(IncorrectInvariantError::InvalidTypes.into());
                            }
                        }
                        _ => return Err(IncorrectInvariantError::InvalidTypes.into()),
                    }
                }
                Opcode::GetGlobal => {
                    let name = self.read_constant(chunk)?;
                    match name {
                        Value::Obj(obj) => {
                            if let Some(s) = obj.as_objstring() {
                                if let Some(v) = self.globals.get(s) {
                                    self.push(*v)?;
                                } else {
                                    return Err(
                                        RuntimeError::UndefinedVariable(obj.to_string()).into()
                                    );
                                }
                            } else {
                                return Err(IncorrectInvariantError::InvalidTypes.into());
                            }
                        }
                        _ => return Err(IncorrectInvariantError::InvalidTypes.into()),
                    }
                }
                Opcode::SetGlobal => {
                    let name = self.read_constant(chunk)?;
                    match name {
                        Value::Obj(obj) => {
                            if let Some(s) = obj.as_objstring() {
                                if self.globals.insert(s, *self.peek(0)?) {
                                    self.globals.delete(s);
                                    return Err(
                                        RuntimeError::UndefinedVariable(obj.to_string()).into()
                                    );
                                }
                            } else {
                                return Err(IncorrectInvariantError::InvalidTypes.into());
                            }
                        }
                        _ => return Err(IncorrectInvariantError::InvalidTypes.into()),
                    }
                }
                Opcode::SetLocal => {
                    let slot = self.read_byte(chunk)?;
                    self.stack[slot as usize] = *self.peek(0)?;
                }
                Opcode::GetLocal => {
                    let slot = self.read_byte(chunk)?;
                    self.push(self.stack[slot as usize])?;
                }
                Opcode::JumpIfFalse => {
                    let offset = self.read_short(chunk)?;
                    if self.peek(0)?.is_falsey() {
                        self.ip += offset as usize;
                    }
                }
                Opcode::Jump => {
                    let offset = self.read_short(chunk)?;
                    self.ip += offset as usize;
                }
                Opcode::Loop => {
                    let offset = self.read_short(chunk)?;
                    self.ip -= offset as usize;
                }
            }
        }

        Ok(())
    }

    fn print_value(&mut self, value: Value) -> VMResult<()> {
        if let Err(e) = writeln!(self.write, "{}", value) {
            error!("Error writing output value: {e}")
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

    fn read_short(&mut self, chunk: &Chunk) -> VMResult<u16> {
        let h = self.read_byte(chunk)?;
        let l = self.read_byte(chunk)?;
        Ok(((h as u16) << 8) | (l as u16))
    }

    fn read_constant<'c>(&mut self, chunk: &'c Chunk) -> VMResult<&'c Value> {
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

    fn binary_op<T>(
        &mut self,
        f: impl Fn(f64, f64) -> T,
        v: fn(T) -> Value,
        line: usize,
    ) -> VMResult<()> {
        let b = self.pop()?;
        let a = self.pop()?;

        let res = match (a, b) {
            (Value::Number(a), Value::Number(b)) => v(f(a, b)),
            (_, _) => {
                return Err(VMError::RuntimeError(RuntimeError::InvalidTypes(
                    line, "numbers",
                )))
            }
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
            (Value::Obj(a), Value::Obj(b)) => match (a.as_objstring(), b.as_objstring()) {
                (Some(a), Some(b)) => (a, b),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        let value = Value::Obj(self.heap_manager.create_string_concat(&a, &b));
        self.push(value)
    }
}

#[derive(Error, Debug, Clone)]
pub enum VMError {
    #[error("Compilation error: {0}")]
    IncorrectInvariantError(#[from] IncorrectInvariantError),
    #[error("runtime error: {0}")]
    RuntimeError(#[from] RuntimeError),
}

#[derive(Error, Debug, Clone)]
pub enum IncorrectInvariantError {
    #[error("invalid opcode? {0}")]
    InvalidOpcode(#[from] TryFromPrimitiveError<Opcode>),
    #[error("invalid constant? {index}")]
    InvalidConstant { index: u8 },
    #[error("stack underflow?")]
    StackUnderflow,
    #[error("invalid compile time types")]
    InvalidTypes,
}

#[derive(Error, Debug, Clone)]
pub enum RuntimeError {
    #[error("invalid instruction pointer {pointer}, max length {chunk_length}")]
    InvalidInstructionPointer { pointer: usize, chunk_length: usize },
    #[error("stack overflow")]
    StackOverflow,
    #[error("Invalid types: Operands must be {1}. [line {0}]")]
    InvalidTypes(usize, &'static str),
    #[error("Invalid type: Operand must be a {0}.")]
    InvalidType(&'static str),
    #[error("Undefined variable '{0}'.")]
    UndefinedVariable(String),
}
