use crate::chunk::Chunk;
use crate::scanner::{ScanError, Token};
use crate::vm::CompileError;

pub struct Compiler {}

impl Compiler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn compile<'a>(
        &mut self,
        mut scanned: impl Iterator<Item = Result<Token<'a>, ScanError<'a>>>,
    ) -> Result<Chunk, CompileError> {
        while let Some(_maybe_token) = scanned.next() {}

        todo!()
    }
}
