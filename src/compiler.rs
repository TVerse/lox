use crate::chunk::{Chunk, Opcode};
use crate::memory::{MemoryManager, Object};
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
    memory_manager: &'b mut MemoryManager,
) -> CompileResult<Chunk> {
    let chunk = Chunk::new("main".to_string(), memory_manager.alloc());
    let mut compiler = Compiler::new(iter, chunk, memory_manager);
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
    memory_manager: &'b mut MemoryManager,
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
        memory_manager: &'b mut MemoryManager,
    ) -> Self {
        let iter: &mut dyn Iterator<Item = ScanResult<Token<'a>>> = iter;
        Self {
            iter: iter.peekable(),
            chunk,
            memory_manager,
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
            None => Err(ParseError::GeneralError("Unexpected end of stream".to_string()).into()),
        }
    }

    fn peek_token(&mut self) -> CompileResult<&Token> {
        match self.iter.peek() {
            Some(token) => match token {
                Ok(token) => Ok(token),
                Err(e) => Err(CompileError::ScanError(e.clone()).into()),
            },
            None => Err(ParseError::GeneralError("Unexpected end of stream".to_string()).into()),
        }
    }

    fn compile(&mut self) -> CompileResult<()> {
        while let Some(peeked) = self.iter.peek() {
            match peeked {
                Ok(_) => self.declaration()?,
                Err(e) => {
                    self.errors.push(e.clone().into());
                    break;
                }
            }
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
        match self.iter.next() {
            Some(Ok(Token {
                contents: TokenContents::Semicolon,
                line,
            })) => self.define_variable(constant_index, line),
            Some(Ok(token)) => {
                let line = token.line;
                errors.push(ParseError::MissingSemicolon(line, token.contents.to_string()).into());
                Err(errors)
            }
            _ => Err(ParseError::GeneralError(
                "Missing semicolon after variable declaration".to_string(),
            )
            .into()),
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
                            self.declare_variable(id, line)?;
                            if self.scope_depth > 0 {
                                Ok(None)
                            } else {
                                self.identifier_constant(id).map(Some)
                            }
                        }
                        _ => {
                            errors.push(
                                ParseError::NotAVariableName(line, token.contents.to_string())
                                    .into(),
                            );
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
                errors.push(
                    ParseError::GeneralError(
                        "Unexpected end of stream after 'var' declaration".to_string(),
                    )
                    .into(),
                );
                Err(errors)
            }
        }
    }

    fn identifier_constant(&mut self, id: &str) -> CompileResult<u8> {
        self.chunk
            .add_constant(Value::Obj(Object::String(
                self.memory_manager.new_str_copied(id),
            )))
            .ok_or_else(|| CompileErrors::from(ParseError::TooManyConstants))
    }

    fn declare_variable(&mut self, name: &'a str, line: usize) -> CompileResult<()> {
        if let Some(local_depth) = NonZeroUsize::new(self.scope_depth) {
            for local in self
                .locals
                .iter()
                .rev()
                .filter(|l| l.depth == Some(local_depth))
            {
                if name == local.name {
                    return Err(ParseError::DuplicateLocal(line, name.to_string()).into());
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
            .map_err(|_| ParseError::GeneralError("Too many locals".to_string()).into())
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
                match self.iter.next() {
                    Some(Ok(Token {
                        contents: TokenContents::Semicolon,
                        line,
                    })) => {
                        self.chunk.add_opcode(Opcode::Print, line);
                        Ok(())
                    }
                    Some(Ok(token)) => {
                        errors.push(
                            ParseError::MissingSemicolon(token.line, token.contents.to_string())
                                .into(),
                        );
                        Err(errors)
                    }
                    _ => {
                        errors.push(
                            ParseError::GeneralError(format!(
                                "Missing semicolon around line {}",
                                line
                            ))
                            .into(),
                        );
                        Err(errors)
                    }
                }
            }
            TokenContents::LeftBrace => {
                let _ = self.next_token()?;
                self.scoped(|s| s.block())?;
                Ok(())
            }
            TokenContents::If => {
                let _ = self.next_token()?;
                self.if_statement()
            }
            TokenContents::While => {
                let _ = self.next_token()?;
                self.while_statement()
            }
            TokenContents::For => {
                let _ = self.next_token()?;
                self.for_statement()
            }
            _ => self.expression_statement(line),
        }
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self) -> CompileResult<()>) -> CompileResult<()> {
        self.scope_depth += 1;
        let res = f(self);
        self.scope_depth -= 1;
        while let Some(last) = self.locals.last() {
            if let Some(local_depth) = last.depth {
                if local_depth.get() > self.scope_depth {
                    self.chunk.add_opcode(Opcode::Pop, 0);
                    let _ = self.locals.pop();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        res
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
                ParseError::GeneralError("Didn't find matching closing brace".to_string()).into(),
            ),
        }
    }

    fn if_statement(&mut self) -> CompileResult<()> {
        match self.next_token() {
            Ok(token) if token.contents == TokenContents::LeftParen => (),
            _ => {
                return Err(ParseError::GeneralError("Expected '(' after 'if'".to_string()).into());
            }
        }
        self.expression()?;
        let token = match self.next_token() {
            Ok(token) if token.contents == TokenContents::RightParen => token,
            _ => {
                return Err(
                    ParseError::GeneralError("Expected ')' after condition".to_string()).into(),
                );
            }
        };
        let line = token.line;
        // TODO fix the line numbers here
        let then_jump = self.emit_jump(Opcode::JumpIfFalse, line)?;
        self.chunk.add_opcode(Opcode::Pop, line);
        self.statement()?;
        let else_jump = self.emit_jump(Opcode::Jump, line)?;
        self.patch_jump(then_jump)?;
        self.chunk.add_opcode(Opcode::Pop, line);
        if let Some(Ok(t)) = self.iter.peek() {
            if t.contents == TokenContents::Else {
                let _ = self.next_token()?;
                self.statement()?;
            }
        }
        self.patch_jump(else_jump)
    }

    fn while_statement(&mut self) -> CompileResult<()> {
        let loop_start = self.chunk.get_loop_start();
        match self.next_token() {
            Ok(token) if token.contents == TokenContents::LeftParen => (),
            _ => {
                return Err(
                    ParseError::GeneralError("Expected '(' after 'while'".to_string()).into(),
                );
            }
        }
        self.expression()?;
        let token = match self.next_token() {
            Ok(token) if token.contents == TokenContents::RightParen => token,
            _ => {
                return Err(
                    ParseError::GeneralError("Expected ')' after condition".to_string()).into(),
                );
            }
        };
        let line = token.line;
        let exit_jump = self.emit_jump(Opcode::JumpIfFalse, line)?;
        self.chunk.add_opcode(Opcode::Pop, line);
        self.statement()?;

        self.emit_loop(loop_start, line)?;

        self.patch_jump(exit_jump)?;
        self.chunk.add_opcode(Opcode::Pop, line);

        Ok(())
    }

    fn for_statement(&mut self) -> CompileResult<()> {
        self.scoped(|s| {
            match s.next_token() {
                Ok(token) if token.contents == TokenContents::LeftParen => (),
                _ => {
                    return Err(
                        ParseError::GeneralError("Expected '(' after 'for'".to_string()).into(),
                    );
                }
            }
            match s.peek_token() {
                Ok(token) if token.contents == TokenContents::Semicolon => {
                    s.next_token()?;
                }
                Ok(token) if token.contents == TokenContents::Var => {
                    s.next_token()?;
                    s.var_declaration()?;
                }
                Ok(token) => {
                    let line = token.line;
                    s.expression_statement(line)?;
                }
                _ => return Err(ParseError::GeneralError("Expected ';'".to_string()).into()),
            }

            let loop_start = s.chunk.get_loop_start();

            let exit_jump = match s.peek_token() {
                Ok(token) if token.contents == TokenContents::Semicolon => {
                    let _ = s.next_token()?;
                    None
                }
                Ok(token) => {
                    let line = token.line;
                    s.expression()?;
                    match s.next_token() {
                        Ok(token) if token.contents == TokenContents::Semicolon => (),
                        _ => {
                            return Err(ParseError::GeneralError("Expected ';'".to_string()).into());
                        }
                    };
                    let exit_jump = s.emit_jump(Opcode::JumpIfFalse, line)?;
                    s.chunk.add_opcode(Opcode::Pop, line);
                    Some(exit_jump)
                }
                _ => return Err(ParseError::GeneralError("Expected ';'".to_string()).into()),
            };
            let (line, loop_start) = match s.peek_token() {
                Ok(token) if token.contents == TokenContents::RightParen => {
                    let token = s.next_token()?;
                    (token.line, loop_start)
                }
                Ok(token) => {
                    let line = token.line;
                    let body_jump = s.emit_jump(Opcode::Jump, line)?;
                    let increment_start = s.chunk.get_loop_start();
                    s.expression()?;
                    s.chunk.add_opcode(Opcode::Pop, line);
                    match s.next_token() {
                        Ok(token) if token.contents == TokenContents::RightParen => (),
                        _ => {
                            return Err(ParseError::GeneralError(
                                "Expected ')' after for clauses".to_string(),
                            )
                            .into());
                        }
                    };
                    s.emit_loop(loop_start, line)?;
                    s.patch_jump(body_jump)?;

                    (line, increment_start)
                }
                _ => {
                    return Err(ParseError::GeneralError(
                        "Expected ')' after condition".to_string(),
                    )
                    .into());
                }
            };
            s.statement()?;

            s.emit_loop(loop_start, line)?;

            if let Some(exit_jump) = exit_jump {
                s.patch_jump(exit_jump)?;
                s.chunk.add_opcode(Opcode::Pop, line);
            }
            Ok(())
        })
    }

    fn emit_jump(&mut self, opcode: Opcode, line: usize) -> CompileResult<usize> {
        Ok(self.chunk.add_dummy_jump(opcode, line))
    }

    fn patch_jump(&mut self, target: usize) -> CompileResult<()> {
        self.chunk
            .patch_jump(target)
            .map_err(|e| ParseError::GeneralError(e).into())
    }

    fn emit_loop(&mut self, loop_start: usize, line: usize) -> CompileResult<()> {
        self.chunk
            .emit_loop(loop_start, line)
            .map_err(|e| ParseError::GeneralError(e).into())
    }

    fn expression_statement(&mut self, estimated_line: usize) -> CompileResult<()> {
        self.expression()?;
        match self.next_token() {
            Ok(Token {
                contents: TokenContents::Semicolon,
                line,
            }) => {
                self.chunk.add_opcode(Opcode::Pop, line);
                Ok(())
            }
            Ok(token) => {
                Err(ParseError::MissingSemicolon(token.line, token.contents.to_string()).into())
            }
            _ => Err(ParseError::GeneralError(format!(
                "Missing semicolon around line {}",
                estimated_line
            ))
            .into()),
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
                        errors.push(
                            ParseError::NoPrefixParser(token.line, token.contents.to_string())
                                .into(),
                        )
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
                    let can_assign = min_bp <= BindingPower::Assignment;
                    if let Some((infix_rule, infix_bp)) = get_parser(token, OperatorType::Infix) {
                        if infix_bp < min_bp {
                            break;
                        }
                        let token = self.iter.next().unwrap().unwrap();

                        if let Err(e) = infix_rule(self, &token, can_assign) {
                            errors.extend(e);
                        }
                    } else {
                        let peek = self.peek_token()?;
                        if can_assign && peek.contents == TokenContents::Equal {
                            errors.push(ParseError::InvalidAssignmentTarget(peek.line).into());
                        }
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
            .ok_or_else(|| CompileErrors::from(ParseError::TooManyConstants))?;
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
                    return Err(ParseError::GeneralError(
                        "Unmatched opening parenthesis".to_owned(),
                    )
                    .into());
                }
            },
            _ => {
                return Err(
                    ParseError::GeneralError("Unmatched opening parenthesis".to_owned()).into(),
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
                let value = Value::Obj(Object::String(self.memory_manager.new_str_copied(s)));
                let constant = self
                    .chunk
                    .add_constant(value)
                    .ok_or_else(|| CompileErrors::from(ParseError::TooManyConstants))?;
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
                let (get_op, set_op, idx) = if let Some(idx) = self.resolve_local(id, token.line)? {
                    (Opcode::GetLocal, Opcode::SetLocal, idx)
                } else {
                    let idx = self.identifier_constant(id)?;
                    (Opcode::GetGlobal, Opcode::SetGlobal, idx)
                };
                if self.peek_token()?.contents == TokenContents::Equal && can_assign {
                    self.next_token()?;
                    self.expression()?;
                    self.chunk.add_opcode_and_operand(set_op, idx, token.line);
                } else {
                    self.chunk.add_opcode_and_operand(get_op, idx, token.line);
                }
            }
            _ => unreachable!("Unexpected identifier token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_and(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        match token.contents {
            TokenContents::And => {
                let end_jump = self.emit_jump(Opcode::JumpIfFalse, token.line)?;
                self.chunk.add_opcode(Opcode::Pop, token.line);
                self.expression_bp(BindingPower::And)?;
                self.patch_jump(end_jump)?;
            }
            _ => unreachable!("Unexpected 'and' token, got {token:?}"),
        }
        Ok(())
    }

    fn parse_or(&mut self, token: &Token, _can_assign: bool) -> CompileResult<()> {
        match token.contents {
            TokenContents::Or => {
                let else_jump = self.emit_jump(Opcode::JumpIfFalse, token.line)?;
                let end_jump = self.emit_jump(Opcode::Jump, token.line)?;
                self.patch_jump(else_jump)?;
                self.chunk.add_opcode(Opcode::Pop, token.line);
                self.expression_bp(BindingPower::Or)?;
                self.patch_jump(end_jump)?;
            }
            _ => unreachable!("Unexpected 'or' token, got {token:?}"),
        }
        Ok(())
    }

    fn resolve_local(&mut self, name: &str, line: usize) -> CompileResult<Option<u8>> {
        for (idx, local) in self
            .locals
            .iter()
            .enumerate()
            // .filter(|(_, l)| l.depth.is_some())
            .rev()
        {
            if local.name == name {
                if local.depth.is_none() {
                    return Err(ParseError::LocalInOwnInitializer(line, name.to_string()).into());
                }
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
        (TokenContents::And, OperatorType::Infix) => Some((Compiler::parse_and, BindingPower::And)),
        (TokenContents::Or, OperatorType::Infix) => Some((Compiler::parse_or, BindingPower::Or)),
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

impl From<ParseError> for CompileErrors {
    fn from(value: ParseError) -> Self {
        let mut this = CompileErrors::new();
        this.push(value.into());
        this
    }
}

impl From<ScanError> for CompileErrors {
    fn from(value: ScanError) -> Self {
        let mut this = CompileErrors::new();
        this.push(value.into());
        this
    }
}

#[derive(Error, Debug, Clone)]
pub enum CompileError {
    #[error(transparent)]
    ScanError(#[from] ScanError),
    #[error(transparent)]
    ParseError(#[from] ParseError),
}

#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("Too many constants in one chunk.")]
    TooManyConstants,
    #[error("[line {0}] Error at '=': Invalid assignment target.")]
    InvalidAssignmentTarget(usize),
    #[error("[line {0}] Error at '{1}': Expect expression. (prefix)")]
    NoPrefixParser(usize, String),
    #[error("[line {0}] Error at '{1}': Expect expression. (infix)")]
    NoInfixParser(usize, String),
    #[error("[line {0}] Error at '{1}': Can't read local variable in its own initializer.")]
    LocalInOwnInitializer(usize, String),
    #[error("[line {0}] Error at '{1}': Expect variable name.")]
    NotAVariableName(usize, String),
    #[error("[line {0}] Error at '{1}': Already a variable with this name in this scope.")]
    DuplicateLocal(usize, String),
    #[error("[line {0}] Error at '{1}': Expect ';' after expression.")]
    MissingSemicolon(usize, String),
    #[error("Compile error: {0}.")]
    GeneralError(String),
}
