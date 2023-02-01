use crate::chunk::{Chunk, Opcode};
use crate::scanner::{ScanError, ScanResult, Token, TokenContents};
use crate::value::Value;
use log::trace;
use std::fmt::{Display, Formatter};
use std::iter::Peekable;
use thiserror::Error;

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

pub fn compile(iter: &mut impl Iterator<Item = ScanResult<Token>>) -> CompileResult<Chunk> {
    let chunk = Chunk::new("main".to_string());
    let mut compiler = Compiler::new(iter, chunk);
    compiler.compile()?;
    let Compiler { mut chunk, .. } = compiler;

    // TODO
    chunk.add_opcode(Opcode::Return, 0);

    trace!("Emitting chunk:\n{:?}", &chunk);
    Ok(chunk)
}

struct Compiler<'a> {
    iter: Peekable<&'a mut dyn Iterator<Item = ScanResult<Token>>>,
    chunk: Chunk,
}

impl<'a> Compiler<'a> {
    fn new(iter: &'a mut impl Iterator<Item = ScanResult<Token>>, chunk: Chunk) -> Self {
        let iter: &mut dyn Iterator<Item = ScanResult<Token>> = iter;
        Self {
            iter: iter.peekable(),
            chunk,
        }
    }

    fn compile(&mut self) -> CompileResult<()> {
        self.compile_bp(BindingPower::None)
    }

    fn compile_bp(&mut self, min_bp: BindingPower) -> CompileResult<()> {
        let mut errors = CompileErrors::new();

        if let Some(token) = self.iter.next() {
            match token {
                Ok(token) => {
                    if let Some((prefix_rule, _)) = get_parser(&token, OperatorType::Prefix) {
                        if let Err(e) = prefix_rule(self, &token) {
                            errors.extend(e);
                        }
                    } else {
                        errors.push(CompileError::GeneralError(format!(
                            "No parser found for token {token:?} and type {:?}",
                            OperatorType::Prefix
                        )))
                    }
                }
                Err(e) => {
                    errors.push(e.into());
                    return Err(errors);
                }
            }
        }

        while let Some(token) = self.iter.peek() {
            match token {
                Ok(token) => {
                    if let Some((infix_rule, infix_bp)) = get_parser(token, OperatorType::Infix) {
                        if infix_bp < min_bp {
                            break;
                        }
                        let token = self.iter.next().unwrap().unwrap();

                        if let Err(e) = infix_rule(self, &token) {
                            errors.extend(e);
                        }
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    errors.push(e.clone().into());
                    break;
                }
            }
        }

        if errors.errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn parse_unary(&mut self, token: &Token) -> CompileResult<()> {
        self.compile_bp(BindingPower::Unary)?;
        match token.contents {
            TokenContents::Minus => self.chunk.add_opcode(Opcode::Negate, token.line),
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected unary token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn parse_number(&mut self, token: &Token) -> CompileResult<()> {
        let number: f64 = match &token.contents {
            TokenContents::Number(number) => number.parse().expect("Could not parse number"),
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Expected number, got token {token:?}"
                ))
                .into());
            }
        };
        let constant = self
            .chunk
            .add_constant(Value::Number(number))
            .ok_or(CompileError::TooManyConstants)?;
        self.chunk
            .add_opcode_and_operand(Opcode::Constant, constant, token.line);
        Ok(())
    }

    fn parse_term(&mut self, token: &Token) -> CompileResult<()> {
        self.compile_bp(BindingPower::Term)?;
        match token.contents {
            TokenContents::Plus => self.chunk.add_opcode(Opcode::Add, token.line),
            TokenContents::Minus => self.chunk.add_opcode(Opcode::Subtract, token.line),
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected term token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn parse_factor(&mut self, token: &Token) -> CompileResult<()> {
        self.compile_bp(BindingPower::Factor)?;
        match token.contents {
            TokenContents::Asterisk => self.chunk.add_opcode(Opcode::Multiply, token.line),
            TokenContents::Slash => self.chunk.add_opcode(Opcode::Divide, token.line),
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected term token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn parse_grouping(&mut self, _token: &Token) -> CompileResult<()> {
        self.compile_bp(BindingPower::None)?;
        match self.iter.next() {
            Some(Ok(token)) => match token.contents {
                TokenContents::RightParen => {}
                _ => {
                    return Err(CompileError::GeneralError(
                        "Unmatched opening parenthesis".to_owned(),
                    )
                    .into());
                }
            },
            _ => {
                return Err(
                    CompileError::GeneralError("Unmatched opening parenthesis".to_owned()).into(),
                );
            }
        }
        Ok(())
    }
}

fn get_parser<'a, 'b>(
    token: &'b Token,
    operator_type: OperatorType,
) -> Option<(Parser<'a, 'b>, BindingPower)> {
    match (&token.contents, operator_type) {
        (TokenContents::Minus, OperatorType::Prefix) => {
            Some((Compiler::parse_unary, BindingPower::Unary))
        }
        (TokenContents::Number(_), OperatorType::Prefix) => {
            Some((Compiler::parse_number, BindingPower::None))
        }
        (TokenContents::Plus | TokenContents::Minus, OperatorType::Infix) => {
            Some((Compiler::parse_term, BindingPower::Term))
        }
        (TokenContents::Asterisk | TokenContents::Slash, OperatorType::Infix) => {
            Some((Compiler::parse_factor, BindingPower::Factor))
        }
        (TokenContents::LeftParen, OperatorType::Prefix) => {
            Some((Compiler::parse_grouping, BindingPower::None))
        }
        _ => None,
    }
}

#[derive(Debug, Copy, Clone)]
enum OperatorType {
    Prefix,
    Infix,
}

type Parser<'a, 'b> = fn(&'b mut Compiler<'a>, &'b Token) -> CompileResult<()>;

// TODO custom Display
#[derive(Error, Debug)]
pub struct CompileErrors {
    errors: Vec<CompileError>,
}

impl Display for CompileErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} compilation error{}",
            self.errors.len(),
            if self.errors.len() == 1 { "" } else { "s" }
        )?;
        for e in self.errors.iter() {
            writeln!(f, "{e}")?;
        }
        Ok(())
    }
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
