use super::{Parser, SyntaxError, TokenKind};
use crate::network::*;

impl Parser {
    pub(super) fn network_ahead(&self) -> bool {
        if ComponentKind::from_keyword(&self.current().text).is_none() {
            return false;
        }
        match self.tokens.get(self.position + 1).map(|token| token.kind) {
            Some(TokenKind::Identifier | TokenKind::OpenBrace) => true,
            Some(TokenKind::OpenParen) => self.tokens[self.position + 2..]
                .iter()
                .position(|token| token.kind == TokenKind::CloseParen)
                .is_some_and(|offset| {
                    self.tokens
                        .get(self.position + offset + 3)
                        .is_some_and(|token| token.kind == TokenKind::OpenBrace)
                }),
            _ => false,
        }
    }
    fn word(&mut self, expected: &str) -> Result<(), SyntaxError> {
        if self.current().text == expected {
            self.bump();
            Ok(())
        } else {
            Err(self.error(format!("expected '{expected}'")))
        }
    }
    fn bound_word<'a>(&'a self, token: &'a crate::Token) -> Result<&'a str, SyntaxError> {
        match self.bindings.iter().find(|(name, _)| name == &token.text) {
            Some((_, crate::Argument::Identifier(text))) => Ok(text),
            Some(_) => Err(SyntaxError {
                span: token.span.clone(),
                message: format!(
                    "parameter '{}' must be an identifier in this position",
                    token.text
                ),
            }),
            None => Ok(&token.text),
        }
    }
    fn scalar(&mut self) -> Result<f32, SyntaxError> {
        if self.at(TokenKind::Identifier) {
            let token = self.bump();
            return self
                .bindings
                .iter()
                .find(|(name, _)| name == &token.text)
                .and_then(|(_, value)| match value {
                    crate::Argument::Number(text) => text.parse::<f32>().ok(),
                    _ => None,
                })
                .filter(|value| value.is_finite())
                .ok_or(SyntaxError {
                    span: token.span,
                    message: format!("unknown numeric parameter '{}'", token.text),
                });
        }
        let token = self.expect(TokenKind::Number)?;
        token
            .text
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or(SyntaxError {
                span: token.span,
                message: "expected a finite number".into(),
            })
    }
    fn network_point(&mut self) -> Result<Point, SyntaxError> {
        self.expect(TokenKind::OpenParen)?;
        let x = self.scalar()?;
        self.expect(TokenKind::Comma)?;
        let y = self.scalar()?;
        let z = if self.at(TokenKind::Comma) {
            self.bump();
            self.scalar()?
        } else {
            0.
        };
        self.expect(TokenKind::CloseParen)?;
        Ok(Point { x, y, z })
    }
    fn reference(&mut self) -> Result<String, SyntaxError> {
        let mut name = self.expect(TokenKind::Identifier)?.text;
        if self.at(TokenKind::Dot) {
            self.bump();
            name.push('.');
            let lane = self.expect(TokenKind::Identifier)?;
            if !matches!(lane.text.as_str(), "in" | "out") {
                return Err(SyntaxError {
                    span: lane.span,
                    message: "port lane must be in or out".into(),
                });
            }
            name.push_str(&lane.text);
        }
        Ok(name)
    }
    fn endpoint(&mut self) -> Result<Endpoint, SyntaxError> {
        if self.at(TokenKind::OpenParen) {
            Ok(Endpoint::Point(self.network_point()?))
        } else {
            Ok(Endpoint::Reference(self.reference()?))
        }
    }
    fn lane_role(&mut self) -> Result<LaneRole, SyntaxError> {
        let token = self.expect(TokenKind::Identifier)?;
        match token.text.as_str() {
            "in" => Ok(LaneRole::In),
            "out" => Ok(LaneRole::Out),
            _ => Err(SyntaxError {
                span: token.span,
                message: "expected in or out lane".into(),
            }),
        }
    }
    fn port_lane(&mut self, name: String, role: LaneRole) -> Result<Port, SyntaxError> {
        self.word("at")?;
        let position = self.network_point()?;
        self.word("heading")?;
        let token = self.expect(TokenKind::Identifier)?;
        let heading = Heading::from_keyword(self.bound_word(&token)?).ok_or(SyntaxError {
            span: token.span,
            message: "expected north, east, south, or west heading".into(),
        })?;
        let width = if self.current().text == "width" {
            self.bump();
            Some(self.scalar()?)
        } else {
            None
        };
        self.expect(TokenKind::Semicolon)?;
        Ok(Port {
            name,
            role,
            position,
            heading,
            width,
        })
    }
    fn network_path(&mut self) -> Result<Path, SyntaxError> {
        let name = self.expect(TokenKind::Identifier)?.text;
        self.expect(TokenKind::OpenBrace)?;
        let start = match self.current().text.as_str() {
            "from" => {
                self.bump();
                self.endpoint()?
            }
            "move" => {
                self.bump();
                Endpoint::Point(self.network_point()?)
            }
            _ => return Err(self.error("path must start with from or move".into())),
        };
        self.expect(TokenKind::Semicolon)?;
        let mut segments = Vec::new();
        while !self.at(TokenKind::CloseBrace) {
            let segment = match self.current().text.as_str() {
                "line" => {
                    self.bump();
                    if self.current().text == "to" {
                        self.bump();
                    }
                    PathSegment::Line(self.endpoint()?)
                }
                "cubic" => {
                    self.bump();
                    let first = self.network_point()?;
                    self.expect(TokenKind::Comma)?;
                    let second = self.network_point()?;
                    self.word("to")?;
                    PathSegment::Cubic {
                        first,
                        second,
                        end: self.endpoint()?,
                    }
                }
                "arc" => {
                    self.bump();
                    self.word("to")?;
                    let end = self.endpoint()?;
                    self.word("radius")?;
                    let radius = self.scalar()?;
                    let clockwise = match self.current().text.as_str() {
                        "counterclockwise" => {
                            self.bump();
                            false
                        }
                        "clockwise" => {
                            self.bump();
                            true
                        }
                        _ => true,
                    };
                    PathSegment::Arc {
                        end,
                        radius,
                        clockwise,
                    }
                }
                _ => return Err(self.error("expected line, cubic, arc, or closing brace".into())),
            };
            self.expect(TokenKind::Semicolon)?;
            segments.push(segment);
        }
        self.bump();
        Ok(Path {
            name,
            start,
            segments,
        })
    }
    fn range_bound(&mut self) -> Result<Bound, SyntaxError> {
        if self.at(TokenKind::Number)
            || self
                .bindings
                .iter()
                .any(|(name, _)| name == &self.current().text)
        {
            return Ok(Bound::Distance(self.scalar()?));
        }
        match self.current().text.as_str() {
            "start" => {
                self.bump();
                Ok(Bound::Start)
            }
            "end" => {
                self.bump();
                Ok(Bound::End)
            }
            _ => Ok(Bound::Anchor(self.expect(TokenKind::Identifier)?.text)),
        }
    }
    fn traversal(&mut self) -> Result<Traversal, SyntaxError> {
        let path = self.expect(TokenKind::Identifier)?.text;
        let (start, end) = if self.at(TokenKind::OpenBracket) {
            self.bump();
            let start = self.range_bound()?;
            self.expect(TokenKind::Range)?;
            let end = self.range_bound()?;
            self.expect(TokenKind::CloseBracket)?;
            (start, end)
        } else {
            (Bound::Start, Bound::End)
        };
        Ok(Traversal { path, start, end })
    }
    pub(super) fn network(&mut self) -> Result<Definition, SyntaxError> {
        let span = self.current().span.clone();
        let kind = ComponentKind::from_keyword(&self.bump().text).unwrap();
        let name = if self.at(TokenKind::Identifier) {
            Some(self.bump().text)
        } else {
            None
        };
        let template_parameters = if self.at(TokenKind::OpenParen)
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|token| token.kind == TokenKind::OpenBrace)
        {
            if name.is_none() {
                return Err(self.error("parameterized component needs a name".into()));
            }
            Some(self.block_parameters()?)
        } else {
            None
        };
        let origin = if self.at(TokenKind::OpenParen) {
            Some(self.network_point()?)
        } else {
            None
        };
        if name.is_none() && origin.is_none() {
            return Err(self.error("component needs a name or inline placement coordinates".into()));
        }
        self.expect(TokenKind::OpenBrace)?;
        if let Some(parameters) = template_parameters {
            if origin.is_some() {
                return Err(
                    self.error("parameterized components must be placed by a separate call".into())
                );
            }
            if self.at(TokenKind::Identifier)
                && self.tokens.get(self.position + 1).is_some_and(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Equals | TokenKind::OpenParen | TokenKind::Comma
                    )
                })
            {
                let mut calls = vec![];
                while !self.at(TokenKind::CloseBrace) {
                    calls.push(self.call()?);
                }
                self.bump();
                if self.at(TokenKind::Semicolon) {
                    self.bump();
                }
                crate::junction_layouts::validate(&calls).map_err(|message| SyntaxError {
                    span: span.clone(),
                    message,
                })?;
                return Ok(Definition {
                    kind,
                    name,
                    origin,
                    template: Some(ComponentTemplate {
                        parameters,
                        body: ComponentTemplateBody::Junction(calls),
                    }),
                    ports: vec![],
                    paths: vec![],
                    anchors: vec![],
                    movements: vec![],
                    bands: vec![],
                });
            }
            let start = self.position;
            let mut depth = 1;
            while depth > 0 {
                match self.current().kind {
                    TokenKind::OpenBrace => depth += 1,
                    TokenKind::CloseBrace => depth -= 1,
                    TokenKind::Eof => {
                        return Err(self.error("expected closing component brace".into()))
                    }
                    _ => {}
                }
                if depth > 0 {
                    self.bump();
                }
            }
            let body = self.tokens[start..self.position]
                .iter()
                .map(|token| token.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            self.bump();
            if self.at(TokenKind::Semicolon) {
                self.bump();
            }
            return Ok(Definition {
                kind,
                name,
                origin,
                template: Some(ComponentTemplate {
                    parameters,
                    body: ComponentTemplateBody::Network(body),
                }),
                ports: vec![],
                paths: vec![],
                anchors: vec![],
                movements: vec![],
                bands: vec![],
            });
        }
        let mut definition = Definition {
            template: None,
            kind,
            name,
            origin,
            ports: vec![],
            paths: vec![],
            anchors: vec![],
            movements: vec![],
            bands: vec![],
        };
        while !self.at(TokenKind::CloseBrace) {
            let declaration = self.expect(TokenKind::Identifier)?;
            match declaration.text.as_str() {
                "port" => {
                    let name = self.expect(TokenKind::Identifier)?.text;
                    if self.at(TokenKind::Dot) {
                        self.bump();
                        let role = self.lane_role()?;
                        definition.ports.push(self.port_lane(name, role)?);
                    } else {
                        self.expect(TokenKind::OpenBrace)?;
                        while !self.at(TokenKind::CloseBrace) {
                            let role = self.lane_role()?;
                            definition.ports.push(self.port_lane(name.clone(), role)?);
                        }
                        self.bump();
                    }
                }
                "path" => definition.paths.push(self.network_path()?),
                "anchor" => {
                    let name = self.expect(TokenKind::Identifier)?.text;
                    self.word("at")?;
                    let path = self.expect(TokenKind::Identifier)?.text;
                    self.word("distance")?;
                    let distance = self.scalar()?;
                    self.expect(TokenKind::Semicolon)?;
                    definition.anchors.push(Anchor {
                        name,
                        path,
                        distance,
                    });
                }
                "allow" => {
                    let class = self.expect(TokenKind::Identifier)?.text;
                    let from = self.reference()?;
                    self.expect(TokenKind::Arrow)?;
                    let to = self.reference()?;
                    self.word("via")?;
                    let mut via = vec![self.traversal()?];
                    while self.current().text == "then" {
                        self.bump();
                        via.push(self.traversal()?);
                    }
                    self.expect(TokenKind::Semicolon)?;
                    definition.movements.push(Movement {
                        class,
                        from,
                        to,
                        via,
                    });
                }
                "surface" | "marking" => {
                    let kind = if declaration.text == "surface" {
                        BandKind::Surface
                    } else {
                        BandKind::Marking
                    };
                    let name = self.expect(TokenKind::Identifier)?.text;
                    self.word("along")?;
                    let path = self.expect(TokenKind::Identifier)?.text;
                    self.word("width")?;
                    let width = self.scalar()?;
                    let mut side = Side::Center;
                    let mut material = if kind == BandKind::Surface {
                        Material::Asphalt
                    } else {
                        Material::White
                    };
                    let (mut seen_side, mut seen_material) = (false, false);
                    while !self.at(TokenKind::Semicolon) {
                        match self.current().text.as_str() {
                            "side" if !seen_side => {
                                self.bump();
                                seen_side = true;
                                side = match self.bump().text.as_str() {
                                    "left" => Side::Left,
                                    "right" => Side::Right,
                                    "center" => Side::Center,
                                    _ => return Err(self.error("unknown band side".into())),
                                };
                            }
                            "material" | "color" if !seen_material => {
                                self.bump();
                                seen_material = true;
                                let token = self.expect(TokenKind::Identifier)?;
                                material = Material::from_keyword(self.bound_word(&token)?).ok_or(
                                    SyntaxError {
                                        span: token.span,
                                        message: "unknown surface material or marking color".into(),
                                    },
                                )?;
                            }
                            _ => {
                                return Err(self.error("unknown or duplicate band property".into()))
                            }
                        }
                    }
                    self.bump();
                    definition.bands.push(Band {
                        name,
                        kind,
                        path,
                        width,
                        side,
                        material,
                    });
                }
                _ => {
                    return Err(SyntaxError {
                        span: declaration.span,
                        message: format!("unknown component declaration '{}'", declaration.text),
                    })
                }
            }
        }
        self.bump();
        if self.at(TokenKind::Semicolon) {
            self.bump();
        }
        crate::network::resolve(&definition).map_err(|message| SyntaxError { span, message })?;
        Ok(definition)
    }
}
