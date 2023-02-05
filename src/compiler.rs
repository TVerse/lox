use crate::chunk::{Chunk, Opcode};
use crate::heap::HeapManager;
use crate::scanner::{ScanError, ScanResult, Token, TokenContents};
use crate::value::Value;
use arrayvec::ArrayVec;
use log::trace;
use std::fmt::{Display, Formatter};
use std::iter::Peekable;
use std::num::NonZeroUsize;
use thiserror::Error;

type CompileResult<A> = Result<A, CompileErrors>;

const MAX_LOCALS: usize = 256;

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
    let chunk = Chunk::new("main".to_string(), heap_manager.alloc());
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
    locals: ArrayVec<Local<'a>, MAX_LOCALS>,
    scope_depth: usize,
}

#[derive(Debug)]
struct Local<'a> {
    name: &'a str,
    depth: Option<NonZeroUsize>,
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
            locals: ArrayVec::new(),
            scope_depth: 0,
        }
    }

    fn next_token(&mut self) -> CompileResult<Token> {
        match self.iter.next() {
            Some(token) => match token {
                Ok(token) => Ok(token),
                Err(e) => Err(CompileError::ScanError(e).into()),
            },
            None => Err(CompileError::GeneralError("Unexpected end of stream".to_string()).into()),
        }
    }

    fn peek_token(&mut self) -> CompileResult<&Token> {
        match self.iter.peek() {
            Some(token) => match token {
                Ok(token) => Ok(token),
                Err(e) => Err(CompileError::ScanError(e.clone()).into()),
            },
            None => Err(CompileError::GeneralError("Unexpected end of stream".to_string()).into()),
        }
    }

    fn compile(&mut self) -> CompileResult<()> {
        while let Some(Ok(_)) = self.iter.peek() {
            self.declaration()?;
        }

        if self.errors.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    fn declaration(&mut self) -> CompileResult<()> {
        let contents = &self.iter.peek().unwrap().as_ref().unwrap().contents;
        let result = if *contents == TokenContents::Var {
            let _ = self.iter.next();
            self.var_declaration()
        } else {
            self.statement()
        };
        if let Err(e) = result {
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

    fn var_declaration(&mut self) -> CompileResult<()> {
        let mut errors = CompileErrors::new();
        let constant_index = self.parse_variable()?;
        if let Some(Ok(token)) = self.iter.peek() {
            match token.contents {
                TokenContents::Equal => {
                    let _ = self.iter.next();
                    self.expression()?
                }
                _ => self.chunk.add_opcode(Opcode::Nil, token.line),
            }
        }
        if let Some(Ok(Token {
            contents: TokenContents::Semicolon,
            line,
        })) = self.iter.next()
        {
            self.define_variable(constant_index, line)
        } else {
            errors.push(CompileError::GeneralError(
                "Missing semicolon after variable declaration".to_string(),
            ));
            Err(errors)
        }
    }

    fn parse_variable(&mut self) -> CompileResult<Option<u8>> {
        let mut errors = CompileErrors::new();
        match self.iter.next() {
            Some(token) => match token {
                Ok(token) => {
                    let line = token.line;
                    match token.contents {
                        TokenContents::Identifier(id) => {
                            self.declare_variable(id)?;
                            if self.scope_depth > 0 {
                                Ok(None)
                            } else {
                                self.identifier_constant(id).map(Some)
                            }
                        }
                        _ => {
                            errors.push(CompileError::GeneralError(format!(
                                "Expected identifier after 'var' on line {line}"
                            )));
                            Err(errors)
                        }
                    }
                }
                Err(e) => {
                    errors.push(e.into());
                    Err(errors)
                }
            },
            None => {
                errors.push(CompileError::GeneralError(
                    "Unexpected end of stream after 'var' declaration".to_string(),
                ));
                Err(errors)
            }
        }
    }

    fn identifier_constant(&mut self, id: &str) -> CompileResult<u8> {
        self.chunk
            .add_constant(Value::Obj(self.heap_manager.create_string_copied(id)))
            .ok_or_else(|| CompileErrors::from(CompileError::TooManyConstants))
    }

    fn declare_variable(&mut self, name: &'a str) -> CompileResult<()> {
        if let Some(local_depth) = NonZeroUsize::new(self.scope_depth) {
            for local in self
                .locals
                .iter()
                .rev()
                .filter(|l| l.depth == Some(local_depth))
            {
                if name == local.name {
                    return Err(CompileError::GeneralError(format!(
                        "Already a variable with name {name}"
                    ))
                    .into());
                }
            }
            self.add_local(name)
        } else {
            Ok(())
        }
    }

    fn add_local(&mut self, name: &'a str) -> CompileResult<()> {
        self.locals
            .try_push(Local { name, depth: None })
            .map_err(|_| CompileError::GeneralError("Too many locals".to_string()).into())
    }

    fn define_variable(&mut self, idx: Option<u8>, line: usize) -> CompileResult<()> {
        if let Some(idx) = idx {
            self.chunk
                .add_opcode_and_operand(Opcode::DefineGlobal, idx, line);
        } else if let Some(local_depth) = NonZeroUsize::new(self.scope_depth) {
            if let Some(local) = self.locals.last_mut() {
                local.depth = Some(local_depth);
            } else {
                unreachable!("Invalid local count?")
            }
        } else {
            unreachable!("Not in global or local scope?")
        }
        Ok(())
    }

    fn statement(&mut self) -> CompileResult<()> {
        let mut errors = CompileErrors::new();
        let token = self.peek_token()?;
        let line = token.line;
        match token.contents {
            TokenContents::Print => {
                let _ = self.next_token();
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
            TokenContents::LeftBrace => {
                let _ = self.next_token()?;
                self.begin_scope();
                self.block()?;
                self.end_scope(line);
                Ok(())
            }
            _ => self.expression_statement(line),
        }
    }

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self, line: usize) {
        self.scope_depth -= 1;
        while let Some(last) = self.locals.last() {
            if let Some(local_depth) = last.depth {
                if local_depth.get() > self.scope_depth {
                    self.chunk.add_opcode(Opcode::Pop, line);
                    let _ = self.locals.pop();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn block(&mut self) -> CompileResult<()> {
        while let Ok(next) = self.peek_token() {
            match next.contents {
                TokenContents::RightBrace => break,
                _ => self.declaration()?,
            }
        }
        match self.next_token() {
            Ok(token) if token.contents == TokenContents::RightBrace => Ok(()),
            _ => Err(
                CompileError::GeneralError("Didn't find matching closing brace".to_string()).into(),
            ),
        }
    }

    fn expression_statement(&mut self, estimated_line: usize) -> CompileResult<()> {
        self.expression()?;
        if let Ok(Token {
            contents: TokenContents::Semicolon,
            line,
        }) = self.next_token()
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
                        let can_assign = min_bp <= BindingPower::Assignment;
                        if let Err(e) = prefix_rule(self, &token, can_assign) {
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

                        let can_assign = min_bp <= BindingPower::Assignment;
                        if let Err(e) = infix_rule(self, &token, can_assign) {
                            errors.extend(e);
                        }
                        let peek = self.peek_token()?;
                        if can_assign && peek.contents == TokenContents::Equal {
                            errors.push(CompileError::GeneralError(format!(
                                "Found invalid = around line {}",
                                peek.line
                            )));
                            break;
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

    fn parse_unary(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        self.expression_bp(BindingPower::Unary)?;
        match token.contents {
            TokenContents::Minus => self.chunk.add_opcode(Opcode::Negate, token.line),
            TokenContents::Bang => self.chunk.add_opcode(Opcode::Not, token.line),
            _ => unreachable!("Unexpected unary token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_number(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        let number: f64 = match &token.contents {
            TokenContents::Number(number) => number.parse().expect("Could not parse number"),
            _ => unreachable!("Expected number, got token {token:?}"),
        };
        let constant = self
            .chunk
            .add_constant(Value::Number(number))
            .ok_or_else(|| CompileErrors::from(CompileError::TooManyConstants))?;
        self.chunk
            .add_opcode_and_operand(Opcode::Constant, constant, token.line);
        Ok(())
    }

    fn parse_term(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        self.expression_bp(BindingPower::Term)?;
        match token.contents {
            TokenContents::Plus => self.chunk.add_opcode(Opcode::Add, token.line),
            TokenContents::Minus => self.chunk.add_opcode(Opcode::Subtract, token.line),
            _ => unreachable!("Unexpected term token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_factor(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        self.expression_bp(BindingPower::Factor)?;
        match token.contents {
            TokenContents::Asterisk => self.chunk.add_opcode(Opcode::Multiply, token.line),
            TokenContents::Slash => self.chunk.add_opcode(Opcode::Divide, token.line),
            _ => unreachable!("Unexpected term token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_grouping(&mut self, _token: &Token, _can_assign: bool) -> CompileResult<()> {
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

    fn parse_literal(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        match token.contents {
            TokenContents::True => self.chunk.add_opcode(Opcode::True, token.line),
            TokenContents::False => self.chunk.add_opcode(Opcode::False, token.line),
            TokenContents::Nil => self.chunk.add_opcode(Opcode::Nil, token.line),
            _ => unreachable!("Unexpected literal token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_equality(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        self.expression_bp(BindingPower::Equality)?;
        match token.contents {
            TokenContents::EqualEqual => self.chunk.add_opcode(Opcode::Equal, token.line),
            TokenContents::BangEqual => {
                self.chunk.add_opcode(Opcode::Equal, token.line);
                self.chunk.add_opcode(Opcode::Not, token.line);
            }
            _ => unreachable!("Unexpected equality token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_comparison(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
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
            _ => unreachable!("Unexpected comparison token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_string(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        match token.contents {
            TokenContents::String(s) => {
                let value = Value::Obj(self.heap_manager.create_string_copied(s));
                let constant = self
                    .chunk
                    .add_constant(value)
                    .ok_or_else(|| CompileErrors::from(CompileError::TooManyConstants))?;
                self.chunk
                    .add_opcode_and_operand(Opcode::Constant, constant, token.line)
            }
            _ => unreachable!("Unexpected string token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_identifier(&mut self, token: &Token, can_assign: bool) -> CompileResult<()> {
        match token.contents {
            TokenContents::Identifier(id) => {
                let (get_op, set_op, idx) = if let Some(idx) = self.resolve_local(id)? {
                    (Opcode::GetLocal, Opcode::SetLocal, idx)
                } else {
                    let idx = self.identifier_constant(id)?;
                    (Opcode::GetGlobal, Opcode::SetGlobal, idx)
                };
                if self.peek_token()?.contents == TokenContents::Equal && can_assign {
                    self.next_token()?;
                    self.expression()?;
                    self.chunk.add_opcode_and_operand(set_op, idx, token.line);
                }
                self.chunk.add_opcode_and_operand(get_op, idx, token.line);
            }
            _ => unreachable!("Unexpected identifier token, got {token:?}"),
        }
        Ok(())
    }

    fn resolve_local(&mut self, name: &str) -> CompileResult<Option<u8>> {
        for (idx, local) in self
            .locals
            .iter()
            .enumerate()
            .filter(|(_, l)| l.depth.is_some())
            .rev()
        {
            if local.name == name {
                return Ok(Some(idx as u8));
            }
        }
        Ok(None)
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
        (TokenContents::Identifier(_), OperatorType::Prefix) => {
            Some((Compiler::parse_identifier, BindingPower::None))
        }
        _ => None,
    }
}

#[derive(Debug, Copy, Clone)]
enum OperatorType {
    Prefix,
    Infix,
}

type Parser<'a, 'b, 'c> = fn(&'c mut Compiler<'a, 'b>, &'c Token<'b>, bool) -> CompileResult<()>;

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
