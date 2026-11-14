use super::*;
use opencity::{Argument as A, Call, Program, Statement};
fn number(value: &A) -> Result<f32, String> {
    if let A::Number(s) = value {
        if let Ok(n) = s.parse::<f32>() {
            if n.is_finite() {
                return Ok(n);
            }
        }
    }
    Err("Prop coordinates must be finite numbers".into())
}
fn n(v: f32) -> A {
    A::Number(format!("{v:.7}"))
}
fn get<'a>(options: &'a [(String, A)], key: &str) -> Option<&'a A> {
    options.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}
fn opt(options: &[(String, A)], key: &str, default: f32) -> Result<f32, String> {
    get(options, key).map(number).unwrap_or(Ok(default))
}
pub fn read(program: &Program) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    for s in &program.statements {
        let Statement::Call(c) = s else {
            continue;
        };
        let count = match c.name.as_str() {
            "prop" => 3,
            "prop_row" => 7,
            _ => continue,
        };
        let name = c
            .assigns
            .first()
            .ok_or("Props need an assigned identity")?
            .clone();
        if c.assigns.len() != 1 || !names.insert(name.clone()) {
            return Err("Props need unique identities".into());
        }
        if c.args.len() < count || c.args.len() > count + 1 {
            return Err(format!("{} has invalid arguments", c.name));
        }
        let Some(A::Identifier(kind)) = c.args.first() else {
            return Err("Expected cone, tree, bench or lamp".into());
        };
        let kind = Kind::from_keyword(kind).ok_or("Unknown prop kind")?;
        let options = match c.args.get(count) {
            None => &[][..],
            Some(A::Object(o)) => o.as_slice(),
            _ => return Err("Expected prop options".into()),
        };
        let allowed = if count == 3 {
            &["heading", "elevation"][..]
        } else {
            &["heading", "elevation", "spacing", "offsets", "hidden"][..]
        };
        if options
            .iter()
            .any(|(key, _)| !allowed.contains(&key.as_str()))
        {
            return Err("Unknown prop option".into());
        }
        let values: Vec<_> = c.args[1..count]
            .iter()
            .map(number)
            .collect::<Result<_, _>>()?;
        let heading = opt(options, "heading", 0.)?;
        let elevation = opt(options, "elevation", 0.)?;
        let shape = if count == 3 {
            Shape::Single(Placement {
                kind,
                position: Vec3::new(values[0], values[1], elevation),
                heading,
            })
        } else {
            let mut row = Row {
                a: Vec2::new(values[0], values[1]),
                control: Vec2::new(values[2], values[3]),
                b: Vec2::new(values[4], values[5]),
                elevation,
                spacing: opt(options, "spacing", kind.spacing())?,
                heading,
                offsets: BTreeMap::new(),
                hidden: BTreeSet::new(),
            };
            for key in ["offsets", "hidden"] {
                if let Some(value) = get(options, key) {
                    let A::Object(items) = value else {
                        return Err("Expected prop row member options".into());
                    };
                    for (name, value) in items {
                        let index = name
                            .strip_prefix('p')
                            .and_then(|s| s.parse::<usize>().ok())
                            .filter(|&i| i < MAX_ROW)
                            .ok_or("Invalid prop row member")?;
                        if key == "hidden" {
                            if value != &A::Identifier("true".into()) {
                                return Err("Expected true for hidden prop".into());
                            }
                            row.hidden.insert(index);
                        } else {
                            let A::Object(o) = value else {
                                return Err("Expected prop member offset".into());
                            };
                            row.offsets.insert(
                                index,
                                Vec3::new(
                                    opt(o, "x", 0.)?,
                                    opt(o, "y", 0.)?,
                                    opt(o, "heading", 0.)?,
                                ),
                            );
                        }
                    }
                }
            }
            row.points()?;
            Shape::Row { kind, row }
        };
        entries.push(Entry { name, shape });
    }
    Ok(entries)
}
pub fn write(entry: &Entry) -> Call {
    let mut args = vec![A::Identifier(entry.kind().keyword().into())];
    let mut options = Vec::new();
    let name = match &entry.shape {
        Shape::Single(p) => {
            args.extend(p.position.to_array()[..2].iter().copied().map(n));
            options.extend([
                ("heading".into(), n(p.heading)),
                ("elevation".into(), n(p.position.z)),
            ]);
            "prop"
        }
        Shape::Row { row, .. } => {
            args.extend(
                [row.a, row.control, row.b]
                    .into_iter()
                    .flat_map(|p| p.to_array())
                    .map(n),
            );
            options.extend([
                ("heading".into(), n(row.heading)),
                ("elevation".into(), n(row.elevation)),
                ("spacing".into(), n(row.spacing)),
            ]);
            if !row.offsets.is_empty() {
                options.push((
                    "offsets".into(),
                    A::Object(
                        row.offsets
                            .iter()
                            .map(|(i, o)| {
                                (
                                    format!("p{i}"),
                                    A::Object(vec![
                                        ("x".into(), n(o.x)),
                                        ("y".into(), n(o.y)),
                                        ("heading".into(), n(o.z)),
                                    ]),
                                )
                            })
                            .collect(),
                    ),
                ));
            }
            if !row.hidden.is_empty() {
                options.push((
                    "hidden".into(),
                    A::Object(
                        row.hidden
                            .iter()
                            .map(|i| (format!("p{i}"), A::Identifier("true".into())))
                            .collect(),
                    ),
                ));
            }
            "prop_row"
        }
    };
    args.push(A::Object(options));
    Call {
        assigns: vec![entry.name.clone()],
        name: name.into(),
        args,
    }
}

/// Replace only prop calls, leaving roads, definitions and other authored calls untouched.
pub fn apply(program: &mut Program, entries: &[Entry]) -> Result<(), String> {
    let desired: BTreeMap<_, _> = entries.iter().map(|e| (e.name.as_str(), e)).collect();
    if desired.len() != entries.len() {
        return Err("Props need unique identities".into());
    }
    let mut used = BTreeSet::new();
    let mut statements = Vec::new();
    for statement in &program.statements {
        if let Statement::Call(call) = statement {
            if matches!(call.name.as_str(), "prop" | "prop_row") {
                if let Some(entry) = call.assigns.first().and_then(|n| desired.get(n.as_str())) {
                    statements.push(Statement::Call(write(entry)));
                    used.insert(entry.name.as_str());
                }
                continue;
            }
        }
        statements.push(statement.clone());
    }
    for entry in entries {
        if !used.contains(entry.name.as_str()) {
            statements.push(Statement::Call(write(entry)));
        }
    }
    let after = Program { statements };
    read(&after)?;
    *program = after;
    Ok(())
}
