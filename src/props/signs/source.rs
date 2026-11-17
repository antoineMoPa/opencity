use super::*;
use crate::Argument as A;
pub(crate) fn option_allowed(kind: Kind, key: &str) -> bool {
    match kind {
        Kind::StopSign => key == "cluster",
        Kind::ParkingSign => key == "permit",
        Kind::SpeedSign => key == "limit",
        Kind::OneWaySign | Kind::CurveSign => key == "arrow",
        Kind::HighwaySign => matches!(key, "destination" | "secondary" | "exit" | "arrow"),
        _ => false,
    }
}
fn literal(a: &A) -> Result<String, String> {
    let A::String(s) = a else {
        return Err("Sign text must be quoted".into());
    };
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or("Invalid sign text")?;
    let mut value = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        value.push(if c == '\\' {
            match chars.next() {
                Some('"') => '"',
                Some('\\') => '\\',
                _ => return Err("Invalid sign text escape".into()),
            }
        } else {
            c
        });
    }
    Ok(value)
}
pub(crate) fn read(kind: Kind, options: &[(String, A)]) -> Result<Option<SignDesign>, String> {
    let Some(mut design) = SignDesign::for_kind(kind) else {
        return Ok(None);
    };
    for (key, value) in options {
        if !option_allowed(kind, key) {
            continue;
        }
        match (&mut design, key.as_str()) {
            (SignDesign::Stop { cluster }, "cluster")
            | (SignDesign::Parking { permit: cluster }, "permit") => {
                *cluster = match value {
                    A::Identifier(s) if s == "true" => true,
                    A::Identifier(s) if s == "false" => false,
                    _ => return Err("cluster must be true or false".into()),
                }
            }
            (SignDesign::Speed { limit }, "limit") => {
                let A::Number(s) = value else {
                    return Err("Expected speed number".into());
                };
                *limit = s.parse().map_err(|_| "Expected integer speed")?;
            }
            (
                SignDesign::OneWay { arrow }
                | SignDesign::Curve { arrow }
                | SignDesign::Highway { arrow, .. },
                "arrow",
            ) => {
                let A::Identifier(s) = value else {
                    return Err("Expected arrow keyword".into());
                };
                *arrow = SignArrow::from_keyword(s).ok_or("Unknown sign arrow")?;
            }
            (SignDesign::Highway { destination, .. }, "destination") => {
                *destination = literal(value)?
            }
            (SignDesign::Highway { secondary, .. }, "secondary") => *secondary = literal(value)?,
            (SignDesign::Highway { exit, .. }, "exit") => *exit = literal(value)?,
            _ => return Err("Invalid sign option".into()),
        }
    }
    design.validate()?;
    Ok(Some(design))
}
fn quoted(s: &str) -> A {
    A::String(format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
pub(crate) fn write(design: &SignDesign, options: &mut Vec<(String, A)>) {
    let mut push = |key: &str, value: A| options.push((key.into(), value));
    match design {
        SignDesign::Stop { cluster } => push("cluster", A::Identifier(cluster.to_string())),
        SignDesign::Parking { permit } => push("permit", A::Identifier(permit.to_string())),
        SignDesign::Speed { limit } => push("limit", A::Number(limit.to_string())),
        SignDesign::OneWay { arrow } | SignDesign::Curve { arrow } => {
            push("arrow", A::Identifier(arrow.keyword().into()))
        }
        SignDesign::Yield => {}
        SignDesign::Highway {
            destination,
            secondary,
            exit,
            arrow,
        } => {
            push("destination", quoted(destination));
            push("secondary", quoted(secondary));
            push("exit", quoted(exit));
            push("arrow", A::Identifier(arrow.keyword().into()));
        }
    }
}
