use crate::error::SyntaxError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Byte offset from the start of the source.
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Road,
    Block,
    End,
    Identifier,
    Number,
    String,
    Parameter,
    Colon,
    Equals,
    OpenParen,
    CloseParen,
    OpenBrace,
    CloseBrace,
    Dot,
    Range,
    Arrow,
    OpenBracket,
    CloseBracket,
    Comma,
    Hyphen,
    Semicolon,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

fn keyword_kind(word: &str) -> TokenKind {
    match word {
        "road" => TokenKind::Road,
        "block" => TokenKind::Block,
        "end" => TokenKind::End,
        _ => TokenKind::Identifier,
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Tokenize the definition language. Whitespace is omitted and an EOF token is appended.
pub fn tokenize(source: &str) -> Result<Vec<Token>, SyntaxError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let (mut offset, mut line, mut column) = (0, 1, 1);

    while offset < bytes.len() {
        let start = offset;
        let span = Span {
            offset,
            line,
            column,
        };
        let kind = match bytes[offset] {
            b'#' => {
                while offset < bytes.len() && bytes[offset] != b'\n' {
                    offset += 1;
                    column += 1;
                }
                continue;
            }
            b'[' => {
                offset += 1;
                TokenKind::OpenBracket
            }
            b']' => {
                offset += 1;
                TokenKind::CloseBracket
            }
            b'.' if bytes.get(offset + 1) == Some(&b'.') => {
                offset += 2;
                TokenKind::Range
            }
            b'.' => {
                offset += 1;
                TokenKind::Dot
            }
            b'-' if bytes.get(offset + 1) == Some(&b'>') => {
                offset += 2;
                TokenKind::Arrow
            }
            b' ' | b'\t' | b'\r' | b'\n' => {
                while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
                    if bytes[offset] == b'\n' {
                        line += 1;
                        column = 1;
                    } else {
                        column += 1;
                    }
                    offset += 1;
                }
                continue;
            }
            byte if is_identifier_start(byte) => {
                offset += 1;
                while offset < bytes.len() && is_identifier_continue(bytes[offset]) {
                    offset += 1;
                }
                keyword_kind(&source[start..offset])
            }
            byte if byte.is_ascii_digit() => {
                offset += 1;
                while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                    offset += 1;
                }
                if offset + 1 < bytes.len()
                    && bytes[offset] == b'.'
                    && bytes[offset + 1].is_ascii_digit()
                {
                    offset += 1;
                    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                        offset += 1;
                    }
                }
                TokenKind::Number
            }
            b'$' => {
                offset += 1;
                let digits_start = offset;
                while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                    offset += 1;
                }
                if offset == digits_start {
                    return Err(SyntaxError {
                        span,
                        message: "expected a numeric parameter after '$'".into(),
                    });
                }
                TokenKind::Parameter
            }
            b'"' => {
                offset += 1;
                while offset < bytes.len() && bytes[offset] != b'"' {
                    if bytes[offset] == b'\\' {
                        offset += 1;
                        if offset >= bytes.len() {
                            break;
                        }
                    }
                    if bytes[offset] == b'\n' {
                        line += 1;
                        column = 1;
                    }
                    offset += 1;
                }
                if offset >= bytes.len() {
                    return Err(SyntaxError {
                        span,
                        message: "unterminated string".into(),
                    });
                }
                offset += 1;
                TokenKind::String
            }
            b':' => {
                offset += 1;
                TokenKind::Colon
            }
            b'=' => {
                offset += 1;
                TokenKind::Equals
            }
            b'(' => {
                offset += 1;
                TokenKind::OpenParen
            }
            b')' => {
                offset += 1;
                TokenKind::CloseParen
            }
            b',' => {
                offset += 1;
                TokenKind::Comma
            }
            b'{' => {
                offset += 1;
                TokenKind::OpenBrace
            }
            b'}' => {
                offset += 1;
                TokenKind::CloseBrace
            }
            b'-' if bytes.get(offset + 1).is_some_and(u8::is_ascii_digit) => {
                offset += 2;
                while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                    offset += 1;
                }
                if offset + 1 < bytes.len()
                    && bytes[offset] == b'.'
                    && bytes[offset + 1].is_ascii_digit()
                {
                    offset += 1;
                    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                        offset += 1;
                    }
                }
                TokenKind::Number
            }
            b'-' => {
                offset += 1;
                TokenKind::Hyphen
            }
            b';' => {
                offset += 1;
                TokenKind::Semicolon
            }
            _ => {
                return Err(SyntaxError {
                    span,
                    message: format!(
                        "unexpected character {:?}",
                        source[offset..].chars().next().unwrap()
                    ),
                });
            }
        };
        column += offset - start;
        tokens.push(Token {
            kind,
            text: source[start..offset].into(),
            span,
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: String::new(),
        span: Span {
            offset,
            line,
            column,
        },
    });
    Ok(tokens)
}
