use crate::memory::allocator::Allocator;
use crate::memory::VMHeapVec;
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
    JumpIfFalse,
    Jump,
    Loop,
}

impl Opcode {
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

pub struct Chunk {
    code: VMHeapVec<u8>,
    constants: VMHeapVec<Value>,
    name: String,
    lines: VMHeapVec<usize>,
}

impl Chunk {
    pub fn new(name: String, alloc: Arc<Allocator>) -> Self {
        Self {
            code: VMHeapVec::new(alloc.clone()),
            constants: VMHeapVec::new(alloc.clone()),
            name,
            lines: VMHeapVec::new(alloc),
        }
    }

    pub fn line_for(&self, ip: usize) -> usize {
        self.lines[ip]
    }

    fn add_byte(&mut self, byte: u8, line: usize) {
        self.code.push(byte);
        self.lines.push(line);
    }

    pub fn add_opcode(&mut self, opcode: Opcode, line: usize) {
        self.add_byte(opcode.as_byte(), line)
    }

    pub fn add_opcode_and_operand(&mut self, opcode: Opcode, operand: u8, line: usize) {
        self.add_opcode(opcode, line);
        self.add_byte(operand, line);
    }

    pub fn add_dummy_jump(&mut self, opcode: Opcode, line: usize) -> usize {
        self.add_opcode(opcode, line);
        let target = self.code.len();
        self.add_byte(0xFF, line);
        self.add_byte(0xFF, line);
        target
    }

    pub fn patch_jump(&mut self, target: usize) -> Result<(), String> {
        let jump = self
            .code
            .len()
            .checked_sub(target)
            .and_then(|t| t.checked_sub(2));
        match jump {
            None => return Err("Too much code to jump over.".to_string()),
            Some(j) if j > u16::MAX as usize => {
                return Err("Too much code to jump over.".to_string());
            }
            Some(jump) => {
                let first_byte = ((jump >> 8) & 0xFF) as u8;
                let second_byte = (jump & 0xFF) as u8;
                self.code[target] = first_byte;
                self.code[target + 1] = second_byte;
            }
        }
        Ok(())
    }

    pub fn get_loop_start(&self) -> usize {
        self.code.len()
    }

    pub fn emit_loop(&mut self, loop_start: usize, line: usize) -> Result<(), String> {
        self.add_opcode(Opcode::Loop, line);
        let offset = self
            .code
            .len()
            .checked_sub(loop_start)
            .and_then(|i| i.checked_add(2));
        match offset {
            None => return Err("Loop body too large.".to_string()),
            Some(j) if j > u16::MAX as usize => return Err("Loop body too large.".to_string()),
            Some(jump) => {
                let first_byte = ((jump >> 8) & 0xFF) as u8;
                let second_byte = (jump & 0xFF) as u8;
                self.add_byte(first_byte, line);
                self.add_byte(second_byte, line);
            }
        }

        Ok(())
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
                    Opcode::JumpIfFalse | Opcode::Jump | Opcode::Loop => {
                        self.short_instruction(opcode, iter.next().map(code), iter.next().map(code))
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

    fn short_instruction(
        &self,
        opcode: Opcode,
        operand_high: Option<u8>,
        operand_low: Option<u8>,
    ) -> String {
        let value = if let Some((h, l)) = operand_high.zip(operand_low) {
            let full = ((h as u16) << 8) | (l as u16);
            format!("0x{full:04x}")
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
            writeln!(f, "{i:04}: {c}")?;
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
