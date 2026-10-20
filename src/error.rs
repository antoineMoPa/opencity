use std::fmt;

use crate::lexer::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.span.line, self.span.column, self.message
        )
    }
}

impl std::error::Error for SyntaxError {}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Syntax(SyntaxError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Syntax(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Syntax(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<SyntaxError> for Error {
    fn from(error: SyntaxError) -> Self {
        Self::Syntax(error)
    }
}
