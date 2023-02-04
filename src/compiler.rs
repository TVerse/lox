use crate::chunk::{Chunk, Opcode};
use crate::heap::HeapManager;
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

pub fn compile<'a, 'b>(
    iter: &'b mut impl Iterator<Item = ScanResult<Token<'a>>>,
    heap_manager: &'b mut HeapManager,
) -> CompileResult<Chunk> {
    let chunk = Chunk::new("main".to_string());
    let mut compiler = Compiler::new(iter, chunk, heap_manager);
    compiler.compile()?;
    let Compiler { mut chunk, .. } = compiler;

    // TODO
    chunk.add_opcode(Opcode::Return, 0);

    trace!("Emitting chunk:\n{:?}", &chunk);
    Ok(chunk)
}

struct Compiler<'a, 'b> {
    iter: Peekable<&'b mut dyn Iterator<Item = ScanResult<Token<'a>>>>,
    chunk: Chunk,
    heap_manager: &'b mut HeapManager,
    errors: CompileErrors,
}

impl<'a, 'b> Compiler<'a, 'b> {
    fn new(
        iter: &'b mut impl Iterator<Item = ScanResult<Token<'a>>>,
        chunk: Chunk,
        heap_manager: &'b mut HeapManager,
    ) -> Self {
        let iter: &mut dyn Iterator<Item = ScanResult<Token<'a>>> = iter;
        Self {
            iter: iter.peekable(),
            chunk,
            heap_manager,
            errors: CompileErrors::default(),
        }
    }

    fn compile(&mut self) -> CompileResult<()> {
        while self.iter.peek().is_some() {
            self.declaration()?;
        }

        if self.errors.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    fn declaration(&mut self) -> CompileResult<()> {
        if let Err(e) = self.statement() {
            self.synchronize(e);
        }
        Ok(())
    }

    fn synchronize(&mut self, e: CompileErrors) {
        self.errors.extend(e);
        while let Some(Ok(token)) = self.iter.next() {
            if token.contents == TokenContents::Semicolon {
                break;
            }
            if let Some(Ok(token)) = self.iter.peek() {
                match token.contents {
                    TokenContents::Class
                    | TokenContents::Fun
                    | TokenContents::Var
                    | TokenContents::For
                    | TokenContents::If
                    | TokenContents::While
                    | TokenContents::Print
                    | TokenContents::Return => break,
                    _ => continue,
                }
            }
        }
    }

    fn statement(&mut self) -> CompileResult<()> {
        let mut errors = CompileErrors::new();
        let token = self.iter.peek().unwrap();
        match token {
            Ok(token) => {
                let line = token.line;
                match token.contents {
                    TokenContents::Print => {
                        let _ = self.iter.next().unwrap().unwrap();
                        self.expression()?;
                        if let Some(Ok(Token {
                            contents: TokenContents::Semicolon,
                            line,
                        })) = self.iter.next()
                        {
                            self.chunk.add_opcode(Opcode::Print, line);
                            Ok(())
                        } else {
                            errors.push(CompileError::GeneralError(format!(
                                "Missing semicolon around line {}",
                                line
                            )));
                            Err(errors)
                        }
                    }
                    _ => self.expression_statement(line),
                }
            }
            Err(e) => {
                errors.push(e.clone().into());
                Err(errors)
            }
        }
    }

    fn expression_statement(&mut self, estimated_line: usize) -> CompileResult<()> {
        self.expression()?;
        if let Some(Ok(Token {
            contents: TokenContents::Semicolon,
            line,
        })) = self.iter.next()
        {
            self.chunk.add_opcode(Opcode::Pop, line);
            Ok(())
        } else {
            Err(CompileError::GeneralError(format!(
                "Missing semicolon around line {}",
                estimated_line
            ))
            .into())
        }
    }

    fn expression(&mut self) -> CompileResult<()> {
        self.expression_bp(BindingPower::None)
    }

    fn expression_bp(&mut self, min_bp: BindingPower) -> CompileResult<()> {
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
        self.expression_bp(BindingPower::Unary)?;
        match token.contents {
            TokenContents::Minus => self.chunk.add_opcode(Opcode::Negate, token.line),
            TokenContents::Bang => self.chunk.add_opcode(Opcode::Not, token.line),
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
        self.expression_bp(BindingPower::Term)?;
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
        self.expression_bp(BindingPower::Factor)?;
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
        self.expression_bp(BindingPower::None)?;
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

    fn parse_literal(&mut self, token: &Token) -> CompileResult<()> {
        match token.contents {
            TokenContents::True => self.chunk.add_opcode(Opcode::True, token.line),
            TokenContents::False => self.chunk.add_opcode(Opcode::False, token.line),
            TokenContents::Nil => self.chunk.add_opcode(Opcode::Nil, token.line),
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected term token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn parse_equality(&mut self, token: &Token) -> CompileResult<()> {
        self.expression_bp(BindingPower::Equality)?;
        match token.contents {
            TokenContents::EqualEqual => self.chunk.add_opcode(Opcode::Equal, token.line),
            TokenContents::BangEqual => {
                self.chunk.add_opcode(Opcode::Equal, token.line);
                self.chunk.add_opcode(Opcode::Not, token.line);
            }
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected equality token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn parse_comparison(&mut self, token: &Token) -> CompileResult<()> {
        self.expression_bp(BindingPower::Comparison)?;
        match token.contents {
            TokenContents::Greater => self.chunk.add_opcode(Opcode::Greater, token.line),
            TokenContents::GreaterEqual => {
                self.chunk.add_opcode(Opcode::Less, token.line);
                self.chunk.add_opcode(Opcode::Not, token.line);
            }
            TokenContents::Less => self.chunk.add_opcode(Opcode::Less, token.line),
            TokenContents::LessEqual => {
                self.chunk.add_opcode(Opcode::Greater, token.line);
                self.chunk.add_opcode(Opcode::Not, token.line);
            }
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected equality token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn parse_string(&mut self, token: &Token) -> CompileResult<()> {
        match token.contents {
            TokenContents::String(s) => {
                let value = Value::Obj(self.heap_manager.create_string_copied(s) as *mut _);
                let constant = self
                    .chunk
                    .add_constant(value)
                    .ok_or(CompileError::TooManyConstants)?;
                self.chunk
                    .add_opcode_and_operand(Opcode::Constant, constant, token.line)
            }
            _ => {
                return Err(CompileError::GeneralError(format!(
                    "Unexpected string token, got {token:?}"
                ))
                .into());
            }
        }
        Ok(())
    }
}

fn get_parser<'a, 'b, 'c>(
    token: &'c Token,
    operator_type: OperatorType,
) -> Option<(Parser<'a, 'b, 'c>, BindingPower)> {
    match (&token.contents, operator_type) {
        (TokenContents::Minus | TokenContents::Bang, OperatorType::Prefix) => {
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
        (TokenContents::True | TokenContents::False | TokenContents::Nil, OperatorType::Prefix) => {
            Some((Compiler::parse_literal, BindingPower::None))
        }
        (TokenContents::EqualEqual | TokenContents::BangEqual, OperatorType::Infix) => {
            Some((Compiler::parse_equality, BindingPower::Equality))
        }
        (
            TokenContents::Greater
            | TokenContents::GreaterEqual
            | TokenContents::Less
            | TokenContents::LessEqual,
            OperatorType::Infix,
        ) => Some((Compiler::parse_comparison, BindingPower::Comparison)),
        (TokenContents::String(_), OperatorType::Prefix) => {
            Some((Compiler::parse_string, BindingPower::None))
        }
        _ => None,
    }
}

#[derive(Debug, Copy, Clone)]
enum OperatorType {
    Prefix,
    Infix,
}

type Parser<'a, 'b, 'c> = fn(&'c mut Compiler<'a, 'b>, &'c Token<'b>) -> CompileResult<()>;

#[derive(Error, Debug, Clone)]
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

    fn extend(&mut self, other: CompileErrors) {
        self.errors.extend(other.errors.into_iter())
    }

    pub fn errors(&self) -> &[CompileError] {
        &self.errors
    }
}

impl Default for CompileErrors {
    fn default() -> Self {
        Self::new()
    }
}

impl From<CompileError> for CompileErrors {
    fn from(value: CompileError) -> Self {
        let mut this = CompileErrors::new();
        this.push(value);
        this
    }
}

#[derive(Error, Debug, Clone)]
pub enum CompileError {
    #[error(transparent)]
    ScanError(#[from] ScanError),
    #[error("Too many constants")]
    TooManyConstants,
    #[error("Compile error: {0}")]
    GeneralError(String),
}
