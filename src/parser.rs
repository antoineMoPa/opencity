use std::fs;
use std::path::Path;

use crate::ast::{
    Argument, BlockDefinition, Call, Program, RoadDefinition, RoadElement, RoadElementKind,
    Statement,
};
use crate::error::{Error, SyntaxError};
use crate::lexer::{tokenize, Token, TokenKind};

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        if token.kind != TokenKind::Eof {
            self.position += 1;
        }
        token
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, SyntaxError> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.error(format!(
                "expected {kind:?}, found {:?}",
                self.current().kind
            )))
        }
    }

    fn error(&self, message: String) -> SyntaxError {
        SyntaxError {
            span: self.current().span.clone(),
            message,
        }
    }

    fn road(&mut self) -> Result<RoadDefinition, SyntaxError> {
        self.expect(TokenKind::Road)?;
        let name = self.expect(TokenKind::Identifier)?.text;
        self.expect(TokenKind::Colon)?;
        let mut elements = Vec::new();
        loop {
            let token = self.expect(TokenKind::Identifier)?;
            let kind = RoadElementKind::from_keyword(&token.text).ok_or_else(|| SyntaxError {
                span: token.span,
                message: format!("unknown road element '{}'", token.text),
            })?;
            let catch = if self.at(TokenKind::OpenParen) {
                self.bump();
                let parameter = self.expect(TokenKind::Parameter)?;
                let value = parameter.text[1..]
                    .parse::<u32>()
                    .map_err(|_| SyntaxError {
                        span: parameter.span,
                        message: "road parameter is too large".into(),
                    })?;
                self.expect(TokenKind::CloseParen)?;
                Some(value)
            } else {
                None
            };
            elements.push(RoadElement { kind, catch });
            if self.at(TokenKind::Hyphen) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(TokenKind::Semicolon)?;
        self.expect(TokenKind::End)?;
        if self.at(TokenKind::Semicolon) {
            self.bump();
        }
        Ok(RoadDefinition { name, elements })
    }

    fn block(&mut self) -> Result<BlockDefinition, SyntaxError> {
        self.expect(TokenKind::Block)?;
        let name = self.expect(TokenKind::Identifier)?.text;
        self.expect(TokenKind::Colon)?;
        let mut calls = Vec::new();
        while !self.at(TokenKind::End) {
            if self.at(TokenKind::Eof) {
                return Err(self.error("expected End, found Eof".into()));
            }
            calls.push(self.call()?);
        }
        self.bump();
        if self.at(TokenKind::Semicolon) {
            self.bump();
        }
        Ok(BlockDefinition { name, calls })
    }

    fn call(&mut self) -> Result<Call, SyntaxError> {
        let first = self.expect(TokenKind::Identifier)?.text;
        let (assigns, name) = if self.at(TokenKind::OpenParen) {
            (Vec::new(), first)
        } else {
            let mut assigns = vec![first];
            while self.at(TokenKind::Comma) {
                self.bump();
                assigns.push(self.expect(TokenKind::Identifier)?.text);
            }
            self.expect(TokenKind::Equals)?;
            (assigns, self.expect(TokenKind::Identifier)?.text)
        };
        self.expect(TokenKind::OpenParen)?;
        let mut args = Vec::new();
        if !self.at(TokenKind::CloseParen) {
            loop {
                let token = self.bump();
                args.push(match token.kind {
                    TokenKind::Number => Argument::Number(token.text),
                    TokenKind::Identifier => Argument::Identifier(token.text),
                    _ => {
                        return Err(SyntaxError {
                            span: token.span,
                            message: "expected a number or identifier argument".into(),
                        })
                    }
                });
                if self.at(TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::CloseParen)?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Call {
            assigns,
            name,
            args,
        })
    }

    fn program(&mut self) -> Result<Program, SyntaxError> {
        let mut statements = Vec::new();
        while !self.at(TokenKind::Eof) {
            statements.push(match self.current().kind {
                TokenKind::Road => Statement::Road(self.road()?),
                TokenKind::Block => Statement::Block(self.block()?),
                _ => Statement::Call(self.call()?),
            });
        }
        Ok(Program { statements })
    }
}

/// Parse a complete OpenCity source file.
pub fn parse(source: &str) -> Result<Program, SyntaxError> {
    Parser {
        tokens: tokenize(source)?,
        position: 0,
    }
    .program()
}

/// Read and parse a `.road` file.
pub fn parse_road_file(path: impl AsRef<Path>) -> Result<Program, Error> {
    Ok(parse(&fs::read_to_string(path)?)?)
}
