use std::iter::Peekable;
use crate::chunk::{Chunk, Opcode};
use crate::scanner::{ScanError, ScanResult, Token, TokenContents};
use thiserror::Error;
use crate::value::Value;

type CompileResult<A> = Result<A, CompileErrors>;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Ord, Eq)]
enum BindingPower {
    None,
    Assignment,
    Or,
    And,
    Equality,
    Comparison,
    Term,
    Factor,
    Unary,
    Call,
    Primary,
}

impl BindingPower {
    fn previous(&self) -> Self {
        use BindingPower::*;
        match self {
            None => unreachable!(),
            Assignment => None,
            Or => Assignment,
            And => Or,
            Equality => And,
            Comparison => Equality,
            Term => Comparison,
            Factor => Term,
            Unary => Factor,
            Call => Unary,
            Primary => Call,
        }
    }
}

pub fn compile<'a>(iter: &'a mut impl Iterator<Item=ScanResult<Token<'a>>>) -> CompileResult<Chunk> {
    let mut chunk = Chunk::new("main".to_string());
    let mut compiler = Compiler::new(iter, chunk);
    compiler.compile()?;
    let Compiler { chunk, .. } = compiler;
    Ok(chunk)
}

struct Compiler<'a> {
    iter: Peekable<&'a mut dyn Iterator<Item=ScanResult<Token<'a>>>>,
    chunk: Chunk,
}

impl<'a> Compiler<'a> {
    fn new(iter: &'a mut impl Iterator<Item=ScanResult<Token<'a>>>, chunk: Chunk) -> Self {
        let iter: &mut dyn Iterator<Item=ScanResult<Token<'a>>> = iter;
        Self {
            iter: iter.peekable(),
            chunk,
        }
    }

    fn compile(
        &mut self,
    ) -> CompileResult<()> {
        self.compile_bp(BindingPower::None)
    }

    fn compile_bp<'b>(
        &'b mut self,
        min_bp: BindingPower,
    ) -> CompileResult<()> {
        let mut errors = CompileErrors::new();

        if let Some(token) = self.iter.next() {
            match token {
                Ok(token) => {
                    if let Some((prefix_rule, _)) = get_parser(token, OperatorType::Prefix) {
                        if let Err(e) = prefix_rule(self, token) {
                            errors.extend(e);
                        }
                    } else {
                        errors.push(CompileError::GeneralError(format!("No parser found for token {token:?} and type {:?}", OperatorType::Prefix)))
                    }

                },
                Err(e) => errors.push(e.clone().into())
            }
        }

        if errors.errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn parse_unary<'b>(&'b mut self, token: Token<'b>) -> CompileResult<()> {
        self.compile_bp(BindingPower::Unary)?;
        match token.contents {
            TokenContents::Minus => self.chunk.add_opcode(Opcode::Negate, token.line),
            _ => return Err(CompileError::GeneralError(format!("Unexpected unary token {token:?}")).into()),
        }
        self.chunk.add_opcode(Opcode::Negate, token.line);
        Ok(())
    }
}

fn get_parser(token: Token, operator_type: OperatorType) -> Option<(Parser, BindingPower)> {
    match (token.contents, operator_type) {
        (TokenContents::Minus, OperatorType::Prefix) => Some((Compiler::parse_unary, BindingPower::Unary)),
        _ => None
    }
}

fn emit_return(chunk: &mut Chunk, line: usize) {
    chunk.add_opcode(Opcode::Return, line);
}

fn number(chunk: &mut Chunk, line: usize, number: &str) -> Result<(), CompileError> {
    let number: f64 = number.parse().unwrap();
    let constant = chunk.add_constant(Value::Number(number)).ok_or(CompileError::TooManyConstants)?;
    chunk.add_opcode_and_operand(Opcode::Constant, constant, line);
    Ok(())
}

#[derive(Debug, Copy, Clone)]
enum OperatorType {
    Prefix,
    Infix,
}

type Parser<'a> = fn(&'a mut Compiler<'a>, Token<'a>) -> CompileResult<()>;




// TODO custom Display
#[derive(Error, Debug)]
#[error("{} error{}", .errors.len(), if.errors.len() == 1 {""} else {"s"})]
pub struct CompileErrors {
    errors: Vec<CompileError>,
}

impl CompileErrors {
    pub fn new() -> Self {
        Self {
            errors: Vec::with_capacity(4),
        }
    }

    fn push(&mut self, e: CompileError) {
        self.errors.push(e)
    }

    pub fn errors(&self) -> &[CompileError] {
        &self.errors
    }

    fn extend(&mut self, other: CompileErrors) {
        self.errors.extend(other.errors.into_iter())
    }
}

impl From<CompileError> for CompileErrors {
    fn from(value: CompileError) -> Self {
        let mut this = CompileErrors::new();
        this.push(value);
        this
    }
}

#[derive(Error, Debug)]
pub enum CompileError {
    #[error(transparent)]
    ScanError(#[from] ScanError),
    #[error("Unimplemented token")]
    TokenNotImplemented(String),
    #[error("Too many constants")]
    TooManyConstants,
    #[error("Compile error: {0}")]
    GeneralError(String),
}

#[cfg(test)]
mod tests {
    use super::*;


}