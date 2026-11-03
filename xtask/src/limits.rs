use std::path::Path;

pub(super) const MAX_WIDTH: usize = 100;

pub(super) fn width(line: &str) -> usize {
    line.chars().map(|ch| if ch == '\t' { 4 } else { 1 }).sum()
}

pub(super) fn inspect(file: &Path, source: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let mut lines = 0;
    for (index, line) in source.lines().enumerate() {
        lines += 1;
        let width = width(line);
        if width > MAX_WIDTH {
            failures.push(format!(
                "{}:{}: {width} characters; maximum is {MAX_WIDTH}.",
                file.display(),
                index + 1
            ));
        }
    }
    if file.extension().is_some_and(|extension| extension == "rs") && lines > 1200 {
        failures.push(format!(
            "{}: {lines} lines; Rust files must have at most 1200.",
            file.display()
        ));
    }
    failures
}
