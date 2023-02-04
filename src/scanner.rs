use std::iter::FusedIterator;
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

pub type ScanResult<A> = Result<A, ScanError>;

static NEWLINE_GRAPHEMES: &[&str] = &["\r", "\n", "\r\n"];
static DIGITS: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
static LOWERCASE_LETTERS: &[&str] = &[
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

static UPPERCASE_LETTERS: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

static UNDERSCORE: &[&str] = &["_"];

#[derive(Debug, Clone, PartialEq)]
pub enum TokenContents<'a> {
    // One-character tokens
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Comma,
    Dot,
    Minus,
    Plus,
    Semicolon,
    Slash,
    Asterisk,
    // One- or two-character tokens
    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    // Literals
    Identifier(&'a str),
    String(&'a str),
    Number(&'a str),
    // Keywords
    And,
    Class,
    Else,
    False,
    For,
    Fun,
    If,
    Nil,
    Or,
    Print,
    Return,
    Super,
    This,
    True,
    Var,
    While,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Token<'a> {
    pub contents: TokenContents<'a>,
    pub line: usize,
}

impl<'a> Token<'a> {
    pub fn new(contents: TokenContents<'a>, line: usize) -> Self {
        Self { contents, line }
    }
}

pub struct Scanner<'a> {
    source: &'a str,
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn iter(&self) -> SourceIterator<'a> {
        SourceIterator::new(self.source)
    }
}

pub struct SourceIterator<'a> {
    source: &'a str,
    graphemes: Vec<&'a str>,
    line: usize,
    cur_char: usize,
}

impl<'a> SourceIterator<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            graphemes: source.graphemes(true).collect(),
            line: 1,
            cur_char: 0,
        }
    }

    // TODO why these lifetimes?
    fn get_and_advance<'b>(&'b mut self) -> Option<&'a str> {
        let res = *self.graphemes.get(self.cur_char)?;
        self.cur_char += 1;
        Some(res)
    }

    fn peek<'b>(&'b mut self) -> Option<&'a str> {
        self.graphemes.get(self.cur_char).copied()
    }

    fn peek_peek<'b>(&'b mut self) -> Option<&'a str> {
        if self.cur_char + 1 > self.graphemes.len() {
            None
        } else {
            self.graphemes.get(self.cur_char + 1).copied()
        }
    }

    fn advance_if_matches<'b>(&'b mut self, c: &'a str) -> bool {
        let res = self.graphemes.get(self.cur_char);
        if let Some(&res) = res {
            if res == c {
                self.cur_char += 1;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                " " | "\t" => {
                    let _ = self.get_and_advance();
                }
                "\n" | "\r" | "\r\n" => {
                    let _ = self.get_and_advance();
                    self.line += 1;
                }
                "/" => {
                    if let Some("/") = self.peek_peek() {
                        while let Some(c) = self.peek() {
                            if !NEWLINE_GRAPHEMES.contains(&c) {
                                let _ = self.get_and_advance();
                            } else {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ => {
                    break;
                }
            };
        }
        self.reset()
    }

    fn reset(&mut self) {
        let advance_len = self
            .graphemes
            .iter()
            .take(self.cur_char)
            .map(|c| c.len())
            .sum();
        self.graphemes.drain(0..self.cur_char);
        self.source = &self.source[advance_len..];
        self.cur_char = 0;
    }

    fn get_cur_str<'b>(&'b self) -> Option<&'a str> {
        let advance_len = self
            .graphemes
            .iter()
            .take(self.cur_char)
            .map(|c| c.len())
            .sum();
        self.source.get(0..advance_len)
    }

    fn string<'b>(&'b mut self) -> ScanResult<Token<'a>> {
        let starting_line = self.line;
        while let Some(c) = self.peek() {
            if NEWLINE_GRAPHEMES.contains(&c) {
                self.line += 1;
            }
            if c == "\"" {
                let _ = self.get_and_advance();
                let contents = self
                    .get_cur_str()
                    .expect("Should not find empty string, including start/end quotes");
                let contents = TokenContents::String(&contents[1..(contents.len() - 1)]);
                return Ok(Token::new(contents, starting_line));
            } else {
                let _ = self.get_and_advance();
            }
        }

        return Err(ScanError::UnterminatedString(
            self.get_cur_str().unwrap_or("").to_string(),
            self.line,
        ));
    }

    fn digit<'b>(&'b mut self) -> Token<'a> {
        while let Some(c) = self.peek() {
            if is_digit(c) {
                let _ = self.get_and_advance();
            } else {
                break;
            }
        }
        if let Some(c) = self.peek() {
            if c == "." {
                if let Some(c) = self.peek_peek() {
                    if is_digit(c) {
                        // Consume .
                        let _ = self.get_and_advance();
                        while let Some(c) = self.peek() {
                            if is_digit(c) {
                                let _ = self.get_and_advance();
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }

        let num = self.get_cur_str().expect("Should not find empty number");
        Token::new(TokenContents::Number(num), self.line)
    }

    fn identifier<'b>(&'b mut self) -> Token<'a> {
        while let Some(c) = self.peek() {
            if is_letter_or_underscore(c) || is_digit(c) {
                let _ = self.get_and_advance();
            } else {
                break;
            }
        }

        let identifier = self
            .get_cur_str()
            .expect("Should not find empty identifier");
        // TODO figure out if trie is worth it here
        use TokenContents::*;
        Token::new(
            match identifier {
                "and" => And,
                "class" => Class,
                "else" => Else,
                "false" => False,
                "for" => For,
                "fun" => Fun,
                "if" => If,
                "nil" => Nil,
                "or" => Or,
                "print" => Print,
                "return" => Return,
                "super" => Super,
                "this" => This,
                "true" => True,
                "var" => Var,
                "while" => While,
                identifier => Identifier(identifier),
            },
            self.line,
        )
    }

    fn match_token<'b>(&'b mut self, c: &'a str) -> Option<ScanResult<Token<'a>>> {
        use TokenContents::*;
        match c {
            "(" => Some(Ok(Token::new(LeftParen, self.line))),
            ")" => Some(Ok(Token::new(RightParen, self.line))),
            "{" => Some(Ok(Token::new(LeftBrace, self.line))),
            "}" => Some(Ok(Token::new(RightBrace, self.line))),
            ";" => Some(Ok(Token::new(Semicolon, self.line))),
            "," => Some(Ok(Token::new(Comma, self.line))),
            "." => Some(Ok(Token::new(Dot, self.line))),
            "-" => Some(Ok(Token::new(Minus, self.line))),
            "+" => Some(Ok(Token::new(Plus, self.line))),
            "/" => Some(Ok(Token::new(Slash, self.line))),
            "*" => Some(Ok(Token::new(Asterisk, self.line))),
            "!" => {
                if self.advance_if_matches("=") {
                    Some(Ok(Token::new(BangEqual, self.line)))
                } else {
                    Some(Ok(Token::new(Bang, self.line)))
                }
            }
            "=" => {
                if self.advance_if_matches("=") {
                    Some(Ok(Token::new(EqualEqual, self.line)))
                } else {
                    Some(Ok(Token::new(Equal, self.line)))
                }
            }
            "<" => {
                if self.advance_if_matches("=") {
                    Some(Ok(Token::new(LessEqual, self.line)))
                } else {
                    Some(Ok(Token::new(Less, self.line)))
                }
            }
            ">" => {
                if self.advance_if_matches("=") {
                    Some(Ok(Token::new(GreaterEqual, self.line)))
                } else {
                    Some(Ok(Token::new(Greater, self.line)))
                }
            }
            "\"" => Some(self.string()),
            _ => {
                if is_digit(c) {
                    Some(Ok(self.digit()))
                } else if is_letter_or_underscore(c) {
                    Some(Ok(self.identifier()))
                } else {
                    None
                }
            }
        }
    }
}

fn is_digit(c: &str) -> bool {
    DIGITS.contains(&c)
}

fn is_letter_or_underscore(c: &str) -> bool {
    LOWERCASE_LETTERS.contains(&c) || UPPERCASE_LETTERS.contains(&c) || UNDERSCORE.contains(&c)
}

impl<'a> Iterator for SourceIterator<'a> {
    type Item = ScanResult<Token<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.skip_whitespace();
        let line = self.line;
        let c = self.get_and_advance()?;
        let res = self
            .match_token(c)
            .or_else(|| Some(Err(ScanError::UnknownToken(c.to_string(), line))));
        self.reset();
        res
    }
}

impl<'a> FusedIterator for SourceIterator<'a> {}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ScanError {
    #[error("Unknown token {0} at line {1}")]
    UnknownToken(String, usize),
    #[error("Unterminated string {0} at line {1}")]
    UnterminatedString(String, usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenContents::*;

    #[test]
    fn scanner_len() {
        let source = "a \tb\n\r//cömment\nc";
        let scanner = Scanner::new(source);
        let iter = scanner.iter();
        assert_eq!(iter.count(), 3);
    }

    #[test]
    fn single_char() {
        let source = "(){};,.-+/*";
        let scanner = Scanner::new(source);
        let iter = scanner.iter();
        let res: Vec<_> = iter.map(|t| t.unwrap().contents).collect();
        let expected = [
            LeftParen, RightParen, LeftBrace, RightBrace, Semicolon, Comma, Dot, Minus, Plus,
            Slash, Asterisk,
        ];
        assert_eq!(&res, &expected);
    }

    #[test]
    fn one_or_two_char() {
        let source = "= == ! != < <= > >= ===";
        let scanner = Scanner::new(source);
        let iter = scanner.iter();
        let res: Vec<_> = iter.map(|t| t.unwrap().contents).collect();
        let expected = [
            Equal,
            EqualEqual,
            Bang,
            BangEqual,
            Less,
            LessEqual,
            Greater,
            GreaterEqual,
            EqualEqual,
            Equal,
        ];
        assert_eq!(&res, &expected);
    }

    #[test]
    fn string() {
        let source = "\n\"hi!\nsup\"\n\"how are you?\"";
        let scanner = Scanner::new(source);
        let iter = scanner.iter();
        let res: Vec<_> = iter.map(|t| t.unwrap()).collect();
        let expected = [
            Token::new(String("hi!\nsup"), 2),
            Token::new(String("how are you?"), 4),
        ];
        assert_eq!(&res, &expected);
    }

    #[test]
    fn digit() {
        let source = "0.123456789\n14482.148210:";
        let scanner = Scanner::new(source);
        let iter = scanner.iter();
        let res: Vec<_> = iter.collect();
        let expected = [
            Ok(Token::new(Number("0.123456789"), 1)),
            Ok(Token::new(Number("14482.148210"), 2)),
            Err(ScanError::UnknownToken(":".to_owned(), 2)),
        ];
        assert_eq!(&res, &expected);
    }

    #[test]
    fn identifier() {
        let source = "a Beta _c class";
        let scanner = Scanner::new(source);
        let iter = scanner.iter();
        let res: Vec<_> = iter.collect();
        let expected = [
            Ok(Token::new(Identifier("a"), 1)),
            Ok(Token::new(Identifier("Beta"), 1)),
            Ok(Token::new(Identifier("_c"), 1)),
            Ok(Token::new(Class, 1)),
        ];
        assert_eq!(&res, &expected)
    }
}
