use std::fs;
use std::path::Path;

use crate::ast::{Argument, Call, Program, RoadDirection, Statement};

impl Program {
    /// Format the AST as a complete, parseable `.road` source file.
    pub fn to_road_source(&self) -> String {
        let mut source = String::new();
        for statement in &self.statements {
            if !source.is_empty() {
                source.push('\n');
            }
            match statement {
                Statement::Road(road) => {
                    source.push_str(&format!("road {}:\n    ", road.name));
                    for (index, element) in road.elements.iter().enumerate() {
                        if index > 0 {
                            source.push('-');
                        }
                        source.push_str(element.kind.keyword());
                        if let Some(catch) = element.catch {
                            source.push_str(&format!("(${catch})"));
                        }
                    }
                    source.push_str(";\n");
                    if road.direction != RoadDirection::Both {
                        source.push_str(&format!("    direction {};\n", road.direction.keyword()));
                    }
                    source.push_str("end;\n");
                }
                Statement::Block(block) => {
                    source.push_str(&format!("block {}:\n", block.name));
                    for call in &block.calls {
                        source.push_str("    ");
                        write_call(&mut source, call);
                    }
                    source.push_str("end;\n");
                }
                Statement::Call(call) => write_call(&mut source, call),
            }
        }
        source
    }

    /// Create parent directories as needed and write a `.road` file.
    pub fn write_road_file(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_road_source())
    }
}

fn write_call(source: &mut String, call: &Call) {
    if !call.assigns.is_empty() {
        source.push_str(&call.assigns.join(", "));
        source.push_str(" = ");
    }
    source.push_str(&call.name);
    source.push('(');
    for (index, arg) in call.args.iter().enumerate() {
        if index > 0 {
            source.push_str(", ");
        }
        match arg {
            Argument::Number(value) | Argument::Identifier(value) | Argument::String(value) => {
                source.push_str(value)
            }
        }
    }
    source.push_str(");\n");
}
