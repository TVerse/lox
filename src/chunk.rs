use crate::heap::allocator::Allocator;
use crate::heap::Vec;
use crate::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::Write;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;

#[derive(Debug, Copy, Clone, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Opcode {
    Constant,
    Add,
    Subtract,
    Multiply,
    Divide,
    Negate,
    Return,
    True,
    False,
    Nil,
    Not,
    Equal,
    Greater,
    Less,
    Print,
    Pop,
    DefineGlobal,
    GetGlobal,
    SetGlobal,
    GetLocal,
    SetLocal,
}

impl Opcode {
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

pub struct Chunk {
    code: Vec<u8>,
    constants: Vec<Value>,
    name: String,
    lines: Vec<usize>,
}

impl Chunk {
    pub fn new(name: String, alloc: Arc<Allocator>) -> Self {
        Self {
            code: Vec::new(alloc.clone()),
            constants: Vec::new(alloc.clone()),
            name,
            lines: Vec::new(alloc),
        }
    }

    fn add_byte(&mut self, byte: u8, line: usize) {
        self.code.push(byte);
        self.lines.push(line);
    }

    pub fn add_opcode(&mut self, opcode: Opcode, line: usize) {
        self.add_byte(opcode.as_byte(), line)
    }

    pub fn add_opcode_and_operand(&mut self, opcode: Opcode, operand: u8, line: usize) {
        self.add_byte(opcode.as_byte(), line);
        self.add_byte(operand, line);
    }

    pub fn add_constant(&mut self, value: Value) -> Option<u8> {
        if self.constants.len() < 256 {
            // Maybe use some set for this? HashTable maybe?
            let existing_index = self
                .constants
                .iter()
                .enumerate()
                .find_map(|(idx, c)| (*c == value).then_some(idx));
            if let Some(idx) = existing_index {
                Some(idx as u8)
            } else {
                self.constants.push(value);
                Some((self.constants.len() - 1) as u8)
            }
        } else {
            None
        }
    }

    pub fn get_constant(&self, index: u8) -> Option<&Value> {
        self.constants.get(index as usize)
    }

    fn code_line_iter(&self) -> impl Iterator<Item = (u8, usize)> + '_ {
        self.code.iter().copied().zip(self.lines.iter().copied())
    }

    pub fn disassemble(&self) -> String {
        let mut iter = self.code_line_iter().enumerate();

        let mut result = String::new();

        let mut previous_line: Option<usize> = None;

        writeln!(result, "== {} ==", self.name).unwrap();

        while let Some((offset, (opcode, line))) = iter.next() {
            write!(result, "0x{offset:04x} ").unwrap();
            match previous_line {
                None => {
                    write!(result, "{line:04} ").unwrap();
                    previous_line = Some(line);
                }
                Some(prev_line) => {
                    if prev_line < line {
                        write!(result, "{line:04} ").unwrap();
                        previous_line = Some(line);
                    } else {
                        write!(result, "   | ").unwrap();
                    }
                }
            }
            self.write_single_instruction(&mut iter, &mut result, opcode);
            writeln!(result).unwrap();
        }

        result
    }

    fn write_single_instruction(
        &self,
        iter: &mut impl Iterator<Item = (usize, (u8, usize))>,
        result: &mut String,
        opcode: u8,
    ) {
        write!(
            result,
            "{}",
            if let Ok(opcode) = Opcode::try_from(opcode) {
                match opcode {
                    Opcode::Return
                    | Opcode::Negate
                    | Opcode::Add
                    | Opcode::Subtract
                    | Opcode::Multiply
                    | Opcode::Divide
                    | Opcode::True
                    | Opcode::False
                    | Opcode::Nil
                    | Opcode::Not
                    | Opcode::Equal
                    | Opcode::Greater
                    | Opcode::Less
                    | Opcode::Print
                    | Opcode::Pop => simple_instruction(opcode),
                    Opcode::Constant
                    | Opcode::DefineGlobal
                    | Opcode::GetGlobal
                    | Opcode::SetGlobal => self.constant_instruction(opcode, iter.next().map(code)),
                    Opcode::GetLocal | Opcode::SetLocal => {
                        self.byte_instruction(opcode, iter.next().map(code))
                    }
                }
            } else {
                format!("Unknown opcode 0x{opcode:02x}")
            }
        )
        .unwrap();
    }

    pub fn disassemble_instruction_at(&self, idx: usize) -> Option<String> {
        let mut iter = self.code_line_iter().enumerate().skip(idx);

        let mut result = String::new();

        if let Some((offset, (opcode, line))) = iter.next() {
            write!(result, "0x{offset:04x} ").unwrap();
            write!(result, "{line:04} ").unwrap();
            self.write_single_instruction(&mut iter, &mut result, opcode);
            Some(result)
        } else {
            None
        }
    }

    fn constant_instruction(&self, opcode: Opcode, operand: Option<u8>) -> String {
        let value = if let Some(idx) = operand {
            let value = self.get_constant(idx);
            if let Some(value) = value {
                format!("{} {}", idx, value)
            } else {
                format!("(index 0x{idx:02x} unknown)")
            }
        } else {
            "(unknown)".to_string()
        };
        format!("{opcode:?} {value}")
    }

    fn byte_instruction(&self, opcode: Opcode, operand: Option<u8>) -> String {
        let value = if let Some(idx) = operand {
            format!("{}", idx)
        } else {
            "(unknown)".to_string()
        };
        format!("{opcode:?} {value}")
    }
}

fn simple_instruction(opcode: Opcode) -> String {
    format!("{opcode:?}")
}

fn code(a: (usize, (u8, usize))) -> u8 {
    a.1 .0
}

impl Debug for Chunk {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.disassemble())?;
        writeln!(f, "Constants:")?;
        for (i, c) in self.constants.iter().enumerate() {
            writeln!(f, "{i:04}: {c:?}")?;
        }
        Ok(())
    }
}

impl Deref for Chunk {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.code
    }
}
