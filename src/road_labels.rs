//! Explicit semantic road labels, independent of profile and segment identifiers.
use crate::{Argument, Call, Program, Statement};

mod graph;
mod inference;
pub use graph::{continuation_links, Continuation, Continuations};
pub(crate) use inference::resolve_names;
pub use inference::{ResolvedRoadNames, RoadGroup, RoadNameState};

fn label(argument: &Argument) -> Option<String> {
    let Argument::String(literal) = argument else {
        return None;
    };
    let content = literal.strip_prefix('"')?.strip_suffix('"')?;
    let mut result = String::new();
    let mut chars = content.chars();
    while let Some(c) = chars.next() {
        result.push(if c == '\\' {
            match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                _ => return None,
            }
        } else {
            c
        });
    }
    (!result.chars().all(char::is_whitespace) && !result.chars().any(char::is_control))
        .then_some(result)
}

/// Read the latest explicit path for a placement in its lexical call scope.
/// The segment identifier is appended by the UI, rather than duplicated in metadata.
pub fn path_in_calls(calls: &[&Call], target: &str) -> Vec<String> {
    labels_in_calls(calls, target, "road_path")
}

pub fn routes_in_calls(calls: &[&Call], target: &str) -> Vec<String> {
    labels_in_calls(calls, target, "road_routes")
}

pub fn boundaries_in_calls(calls: &[&Call]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| match (c.name.as_str(), c.args.as_slice()) {
            ("road_boundary", [Argument::Identifier(reference)]) => Some(reference.clone()),
            _ => None,
        })
        .collect()
}

fn labels_in_calls(calls: &[&Call], target: &str, kind: &str) -> Vec<String> {
    calls
        .iter()
        .rev()
        .find_map(|call| {
            if call.name != kind
                || !matches!(call.args.first(), Some(Argument::Identifier(name)) if name == target)
            {
                return None;
            }
            call.args[1..].iter().map(label).collect::<Option<Vec<_>>>()
        })
        .unwrap_or_default()
}

/// Set or clear a named top-level placement's semantic hierarchy.
pub fn set_road_path(program: &mut Program, target: &str, path: &[String]) -> Result<(), String> {
    set_labels(program, target, path, "road_path")
}

pub fn set_road_routes(
    program: &mut Program,
    target: &str,
    routes: &[String],
) -> Result<(), String> {
    set_labels(program, target, routes, "road_routes")
}

fn set_labels(
    program: &mut Program,
    target: &str,
    path: &[String],
    kind: &str,
) -> Result<(), String> {
    if !program.statements.iter().any(|s| {
        matches!(s,
        Statement::Call(c) if c.assigns.first().is_some_and(|name| name == target))
    }) {
        return Err(format!("{kind} target {target:?} has no named placement"));
    }
    if path
        .iter()
        .any(|s| s.chars().all(char::is_whitespace) || s.chars().any(char::is_control))
    {
        return Err(format!(
            "{kind} labels must be nonempty, single-line strings"
        ));
    }
    program.statements.retain(|s| {
        let Statement::Call(call) = s else {
            return true;
        };
        call.name != kind
            || !matches!(call.args.first(), Some(Argument::Identifier(name)) if name == target)
    });
    if !path.is_empty() {
        let mut args = vec![Argument::Identifier(target.into())];
        args.extend(path.iter().map(|s| {
            let literal = s.replace('\\', "\\\\").replace('"', "\\\"");
            Argument::String(format!("\"{literal}\""))
        }));
        program.statements.push(Statement::Call(Call {
            assigns: Vec::new(),
            name: kind.into(),
            args,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map, parse};

    #[test]
    fn hierarchy_round_trips_and_follows_rename_and_removal_without_changing_geometry() {
        let mut program = parse("road local: carlane; end seg = local(0, 0, 2, 0);").unwrap();
        let before = map::project(&program).segments.remove(0);
        let labels = vec!["Route 7".into(), "River \"east\" \\ crossing".into()];
        set_road_path(&mut program, "seg", &labels).unwrap();
        let road = map::project(&program).segments.remove(0);
        assert_eq!(road.label_path, labels);
        assert_eq!(
            (road.start, road.end, road.controls),
            (before.start, before.end, before.controls)
        );
        assert!(map::set_name(&mut program, road.source, "renamed".into()));
        let road = map::project(&program).segments.remove(0);
        assert_eq!(road.label_path, labels);
        assert_eq!(road.name.as_deref(), Some("renamed"));
        assert_eq!(parse(&program.to_road_source()).unwrap(), program);
        assert!(map::remove(&mut program, road.source));
        assert!(!program.statements.iter().any(|s| matches!(s,
            Statement::Call(c) if c.name == "road_path")));
    }

    #[test]
    fn nested_instances_resolve_independent_paths_in_their_lexical_scopes() {
        let program = parse(
            r#"
            road local: carlane; end
            block span:
                seg = local(-1, 0, 1, 0);
                road_path(seg, "Deck");
            end
            block corridor:
                crossing = span(0, 0);
                road_path(crossing, "River Bridge");
            end
            west = corridor(0, 0);
            east = corridor(5, 0);
            road_path(west, "Route 7");
            road_path(east, "Route 8");
        "#,
        )
        .unwrap();
        let projection = map::project(&program);
        assert_eq!(projection.unresolved, 0);
        assert_eq!(
            projection.segments[0].label_path,
            ["Route 7", "River Bridge", "Deck"]
        );
        assert_eq!(
            projection.segments[1].label_path,
            ["Route 8", "River Bridge", "Deck"]
        );
        assert_eq!(projection.segments[0].name.as_deref(), Some("seg"));
    }

    #[test]
    fn invalid_edits_leave_existing_metadata_intact_and_empty_paths_clear_it() {
        let mut program = parse("road local: carlane; end seg = local(0, 0, 2, 0);").unwrap();
        set_road_path(&mut program, "seg", &["Route 7".into()]).unwrap();
        let before = program.clone();
        assert!(set_road_path(&mut program, "missing", &["Route 8".into()]).is_err());
        assert!(set_road_path(&mut program, "seg", &["\n".into()]).is_err());
        assert_eq!(program, before);
        set_road_path(&mut program, "seg", &[]).unwrap();
        assert!(map::project(&program).segments[0].label_path.is_empty());
    }
}

#[cfg(test)]
mod graph_tests;
