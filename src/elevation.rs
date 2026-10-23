use crate::{Argument, Call, Program, Statement};

/// Height in map units. Zero is the ground plane; negative values are below it.
/// A named placement gets its height from `elevation(placement_name, height);`.
pub fn elevation_of(program: &Program, name: &str) -> Option<f32> {
    let calls: Vec<&Call> = program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            Some(call)
        })
        .collect();
    elevation_in_calls(&calls, name)
}

pub fn elevation_in_calls(calls: &[&Call], name: &str) -> Option<f32> {
    calls.iter().rev().find_map(|call| {
        if call.name != "elevation" || call.args.len() != 2 {
            return None;
        }
        match (&call.args[0], &call.args[1]) {
            (Argument::Identifier(target), Argument::Number(value)) if target == name => value
                .parse::<f32>()
                .ok()
                .filter(|height| height.is_finite()),
            _ => None,
        }
    })
}

/// Endpoint heights for a named road placement. Values are relative to the
/// containing block, just like a local `elevation` call.
pub fn grade_of(program: &Program, name: &str) -> Option<(f32, f32)> {
    let calls: Vec<&Call> = program
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::Call(call) = statement else {
                return None;
            };
            Some(call)
        })
        .collect();
    grade_in_calls(&calls, name)
}

pub fn grade_in_calls(calls: &[&Call], name: &str) -> Option<(f32, f32)> {
    calls.iter().rev().find_map(|call| {
        if call.name != "grade" || call.args.len() != 3 {
            return None;
        }
        match (&call.args[0], &call.args[1], &call.args[2]) {
            (Argument::Identifier(target), Argument::Number(start), Argument::Number(end))
                if target == name =>
            {
                let start = start.parse::<f32>().ok()?;
                let end = end.parse::<f32>().ok()?;
                (start.is_finite() && end.is_finite()).then_some((start, end))
            }
            _ => None,
        }
    })
}

pub fn set_grade(program: &mut Program, name: &str, start: f32, end: f32) -> bool {
    if name.is_empty() || !start.is_finite() || !end.is_finite() {
        return false;
    }
    let values = [
        Argument::Number(format!("{start:.4}")),
        Argument::Number(format!("{end:.4}")),
    ];
    if let Some(Statement::Call(call)) = program.statements.iter_mut().rev().find(|statement| {
        matches!(statement, Statement::Call(call) if call.name == "grade" && call.args.len() == 3
            && matches!(call.args.first(), Some(Argument::Identifier(target)) if target == name))
    }) {
        call.args[1] = values[0].clone();
        call.args[2] = values[1].clone();
        return true;
    }
    program.statements.push(Statement::Call(Call {
        assigns: Vec::new(),
        name: "grade".into(),
        args: vec![
            Argument::Identifier(name.into()),
            values[0].clone(),
            values[1].clone(),
        ],
    }));
    true
}

/// Return the explicit height of a block placement, if present.
pub fn block_height(args: &[Argument]) -> Option<f32> {
    if args.len() != 3 {
        return None;
    }
    match &args[2] {
        Argument::Number(value) => value
            .parse::<f32>()
            .ok()
            .filter(|height| height.is_finite()),
        Argument::Identifier(_) | Argument::String(_) => None,
    }
}

/// Set or replace the height of a named placement. The placement itself is
/// kept unchanged, so one reusable definition can appear at several heights.
pub fn set_elevation(program: &mut Program, name: &str, height: f32) -> bool {
    if !height.is_finite() || name.is_empty() {
        return false;
    }
    let value = Argument::Number(format!("{height:.4}"));
    if let Some(Statement::Call(call)) = program.statements.iter_mut().rev().find(|statement| {
        matches!(statement, Statement::Call(call) if call.name == "elevation" && call.args.len() == 2
            && matches!(call.args.first(), Some(Argument::Identifier(target)) if target == name))
    }) {
        call.args[1] = value;
        return true;
    }
    program.statements.push(Statement::Call(Call {
        assigns: Vec::new(),
        name: "elevation".into(),
        args: vec![Argument::Identifier(name.into()), value],
    }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn named_placements_have_independent_elevations() {
        let mut program =
            parse("road rail: traintrack; end; a = rail(0, 0, 1, 0); b = rail(0, 1, 1, 1);")
                .unwrap();
        assert!(set_elevation(&mut program, "a", 2.5));
        assert!(set_elevation(&mut program, "b", -1.0));
        assert_eq!(elevation_of(&program, "a"), Some(2.5));
        assert_eq!(elevation_of(&program, "b"), Some(-1.0));
        assert_eq!(
            elevation_of(&parse(&program.to_road_source()).unwrap(), "a"),
            Some(2.5)
        );
        assert!(set_elevation(&mut program, "a", 3.0));
        assert_eq!(elevation_of(&program, "a"), Some(3.0));
    }

    #[test]
    fn grade_round_trips_and_replaces_latest_call() {
        let mut program = parse("road ramp: carlane; end; exit = ramp(0, 0, 2, 0);").unwrap();
        assert!(set_grade(&mut program, "exit", 0.0, 0.25));
        assert_eq!(grade_of(&program, "exit"), Some((0.0, 0.25)));
        assert!(set_grade(&mut program, "exit", -0.5, 1.0));
        assert_eq!(
            grade_of(&parse(&program.to_road_source()).unwrap(), "exit"),
            Some((-0.5, 1.0))
        );
        assert!(!set_grade(&mut program, "exit", f32::NAN, 1.0));
    }
}
