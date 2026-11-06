//! Resolve each authored attachment once, including the established first-valid rule.
use crate::{map, Program, Statement};
use std::collections::HashMap;

pub(super) fn index(program: &Program) -> HashMap<String, (String, usize, f32)> {
    let mut result = HashMap::new();
    for statement in &program.statements {
        let Statement::Call(call) = statement else {
            continue;
        };
        if call.name != "lane_point" {
            continue;
        }
        let Some(name) = call.assigns.first() else {
            continue;
        };
        if result.contains_key(name) {
            continue;
        }
        if let Some(value) = map::lane_attachment(program, name) {
            result.insert(name.clone(), value);
        }
    }
    result
}
