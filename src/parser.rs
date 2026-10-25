use std::fs;
use std::path::Path;

use crate::ast::{
    Argument, BlockDefinition, BlockParameter, BlockParameterType, Call, Program, RoadDefinition,
    RoadDirection, RoadElement, RoadElementKind, RoadRole, Statement,
};
use crate::error::{Error, SyntaxError};
use crate::lexer::{tokenize, Token, TokenKind};

#[path = "parser/network.rs"]
mod network;

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
                // Keep existing road files readable; the writer emits spaces.
                self.bump();
            } else if !self.at(TokenKind::Identifier) {
                break;
            }
        }
        self.expect(TokenKind::Semicolon)?;
        let mut direction = RoadDirection::Both;
        let mut role = RoadRole::Ordinary;
        let mut seen_direction = false;
        let mut seen_role = false;
        while self.at(TokenKind::Identifier) {
            let property = self.expect(TokenKind::Identifier)?;
            let value = self.expect(TokenKind::Identifier)?;
            match property.text.as_str() {
                "direction" if !seen_direction => {
                    direction =
                        RoadDirection::from_keyword(&value.text).ok_or_else(|| SyntaxError {
                            span: value.span,
                            message: format!("unknown road direction '{}'", value.text),
                        })?;
                    seen_direction = true;
                }
                "role" if !seen_role => {
                    role = RoadRole::from_keyword(&value.text).ok_or_else(|| SyntaxError {
                        span: value.span,
                        message: format!("unknown road role '{}'", value.text),
                    })?;
                    seen_role = true;
                }
                _ => {
                    return Err(SyntaxError {
                        span: property.span,
                        message: format!("unknown or duplicate road property '{}'", property.text),
                    })
                }
            }
            self.expect(TokenKind::Semicolon)?;
        }
        self.expect(TokenKind::End)?;
        if self.at(TokenKind::Semicolon) {
            self.bump();
        }
        Ok(RoadDefinition {
            name,
            elements,
            direction,
            role,
        })
    }

    fn block(&mut self) -> Result<BlockDefinition, SyntaxError> {
        self.expect(TokenKind::Block)?;
        let name = self.expect(TokenKind::Identifier)?.text;
        let parameters = if self.at(TokenKind::OpenParen) {
            Some(self.block_parameters()?)
        } else {
            None
        };
        self.expect(TokenKind::Colon)?;
        let mut calls = Vec::new();
        while !self.at(TokenKind::End) {
            if self.at(TokenKind::Eof) {
                return Err(self.error("expected End, found Eof".into()));
            }
            let call = self.call()?;
            if let Some(parameters) = &parameters {
                if call
                    .assigns
                    .iter()
                    .any(|name| parameters.iter().any(|p| &p.name == name))
                {
                    return Err(self.error("block result names cannot shadow parameters".into()));
                }
            }
            calls.push(call);
        }
        self.bump();
        if self.at(TokenKind::Semicolon) {
            self.bump();
        }
        Ok(BlockDefinition {
            name,
            parameters,
            calls,
        })
    }

    fn block_parameters(&mut self) -> Result<Vec<BlockParameter>, SyntaxError> {
        self.expect(TokenKind::OpenParen)?;
        self.expect(TokenKind::OpenBrace)?;
        let mut names = Vec::new();
        while !self.at(TokenKind::CloseBrace) {
            let token = self.expect(TokenKind::Identifier)?;
            if names.contains(&token.text) {
                return Err(SyntaxError {
                    span: token.span,
                    message: "duplicate block parameter".into(),
                });
            }
            names.push(token.text);
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump();
        }
        self.expect(TokenKind::CloseBrace)?;
        self.expect(TokenKind::Colon)?;
        self.expect(TokenKind::OpenBrace)?;
        let mut typed = Vec::<BlockParameter>::new();
        while !self.at(TokenKind::CloseBrace) {
            let name = self.expect(TokenKind::Identifier)?;
            if !names.contains(&name.text) || typed.iter().any(|p| p.name == name.text) {
                return Err(SyntaxError {
                    span: name.span,
                    message: "unknown or duplicate parameter type property".into(),
                });
            }
            self.expect(TokenKind::Colon)?;
            let token = self.expect(TokenKind::Identifier)?;
            let kind =
                BlockParameterType::from_keyword(&token.text).ok_or_else(|| SyntaxError {
                    span: token.span,
                    message: format!("unknown block parameter type '{}'", token.text),
                })?;
            typed.push(BlockParameter {
                name: name.text,
                kind,
            });
            if !self.at(TokenKind::Comma) {
                break;
            }
            self.bump();
        }
        self.expect(TokenKind::CloseBrace)?;
        self.expect(TokenKind::CloseParen)?;
        names
            .into_iter()
            .map(|name| {
                typed
                    .iter()
                    .find(|p| p.name == name)
                    .cloned()
                    .ok_or_else(|| self.error(format!("missing type for block parameter '{name}'")))
            })
            .collect()
    }

    fn argument(&mut self) -> Result<Argument, SyntaxError> {
        if self.at(TokenKind::OpenBrace) {
            self.bump();
            let mut properties = Vec::<(String, Argument)>::new();
            while !self.at(TokenKind::CloseBrace) {
                let name = self.expect(TokenKind::Identifier)?;
                if properties.iter().any(|(key, _)| key == &name.text) {
                    return Err(SyntaxError {
                        span: name.span,
                        message: "duplicate object property".into(),
                    });
                }
                let value = if self.at(TokenKind::Colon) {
                    self.bump();
                    self.argument()?
                } else {
                    Argument::Identifier(name.text.clone())
                };
                properties.push((name.text, value));
                if !self.at(TokenKind::Comma) {
                    break;
                }
                self.bump();
            }
            self.expect(TokenKind::CloseBrace)?;
            return Ok(Argument::Object(properties));
        }
        let token = self.bump();
        match token.kind {
            TokenKind::Number => Ok(Argument::Number(token.text)),
            TokenKind::Identifier => {
                let mut name = token.text;
                while self.at(TokenKind::Dot) {
                    self.bump();
                    name.push('.');
                    name.push_str(&self.expect(TokenKind::Identifier)?.text);
                }
                Ok(Argument::Identifier(name))
            }
            TokenKind::String => Ok(Argument::String(token.text)),
            _ => Err(SyntaxError {
                span: token.span,
                message: "expected a number, identifier, string, or object argument".into(),
            }),
        }
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
                args.push(self.argument()?);
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
                TokenKind::Identifier if self.network_ahead() => {
                    Statement::Network(self.network()?)
                }
                _ => Statement::Call(self.call()?),
            });
        }
        Ok(Program { statements })
    }
}

/// Parse a complete OpenCity source file.
pub fn parse(source: &str) -> Result<Program, SyntaxError> {
    let program = crate::patterns::expand(parse_raw(source)?)?;
    for (index, statement) in program.statements.iter().enumerate() {
        let Statement::Network(definition) = statement else {
            continue;
        };
        let Some(name) = &definition.name else {
            continue;
        };
        if program.statements.iter().enumerate().any(|(other, s)| {
            other != index
                && match s {
                    Statement::Network(d) => d.name.as_ref() == Some(name),
                    Statement::Road(d) => &d.name == name,
                    Statement::Block(d) => &d.name == name,
                    _ => false,
                }
        }) {
            return Err(SyntaxError {
                span: crate::Span {
                    offset: 0,
                    line: 1,
                    column: 1,
                },
                message: format!("duplicate component definition name '{name}'"),
            });
        }
    }
    Ok(program)
}

pub(crate) fn parse_raw(source: &str) -> Result<Program, SyntaxError> {
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
